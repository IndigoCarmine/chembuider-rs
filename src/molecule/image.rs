//! Render a `Molecule` to a bitmap (Windows DIB) / PNG / EMF for the clipboard, like ChemDraw's
//! image flavor. The raster path draws to an SVG, rasterized with resvg; the EMF path draws GDI
//! records. Windows-only (depends on resvg).
//!
//! Both paths build their geometry from [`crate::widget::scene`] — the exact same backend-neutral
//! primitive list the on-screen editor draws — so the copied/exported picture matches the canvas:
//! same scale ([`SCALE_FACTOR`]), bond width / double-bond offset / stereo wedges / rich atom
//! labels (H counts, charges) / ring fills, all from one source. The SVG path additionally
//! positions each label glyph at egui's own layout coordinates and (when available) rasterizes it
//! with egui's own bundled font, so the label placement is pixel-faithful to the widget.

use super::Molecule;
use crate::config::StyleConfig;
use crate::widget::scene::{MoleculeScene, RenderTransform};
use crate::widget::SCALE_FACTOR;
use egui::Vec2;
use resvg::usvg::fontdb;
use std::sync::{Arc, OnceLock};

/// Extra pixels of empty space around the drawn structure.
const MARGIN_PX: f32 = 12.0;

/// System fonts plus egui's own bundled proportional font, loaded once. The returned family name
/// is egui's font (so SVG labels rasterize with the same glyphs the widget shows); it falls back
/// to `"sans-serif"` if the bundled font can't be extracted.
fn egui_font() -> (Arc<fontdb::Database>, String) {
    static DB: OnceLock<(Arc<fontdb::Database>, String)> = OnceLock::new();
    DB.get_or_init(|| {
        // The system fonts (named fonts in text are drawn with them), read once for everything.
        let mut db = crate::widget::fonts::system_database().clone();
        let mut family = String::from("sans-serif");
        let defs = egui::FontDefinitions::default();
        if let Some(first) = defs
            .families
            .get(&egui::FontFamily::Proportional)
            .and_then(|names| names.first())
        {
            if let Some(data) = defs.font_data.get(first) {
                let before: Vec<fontdb::ID> = db.faces().map(|f| f.id).collect();
                db.load_font_data((*data.font).as_ref().to_vec());
                if let Some(face) = db.faces().find(|f| !before.contains(&f.id)) {
                    if let Some((fam, _)) = face.families.first() {
                        family = fam.clone();
                    }
                }
            }
        }
        (Arc::new(db), family)
    })
    .clone()
}

/// Render the molecule to an RGBA pixmap. `background` paints an opaque fill (e.g. white);
/// None leaves the background transparent.
fn render_pixmap(mol: &Molecule, background: Option<resvg::tiny_skia::Color>) -> Option<resvg::tiny_skia::Pixmap> {
    if mol.is_blank() {
        return None;
    }
    let svg = molecule_to_svg(mol)?;
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb = egui_font().0;
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).ok()?;

    let size = tree.size();
    let w = size.width().ceil() as u32;
    let h = size.height().ceil() as u32;
    if w == 0 || h == 0 || w > 4000 || h > 4000 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    if let Some(color) = background {
        pixmap.fill(color);
    }
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Some(pixmap)
}

/// Points per pixel of the drawing: its 1.5-unit bond is ChemDraw's 14.4 pt one.
pub const PT_PER_PX: f32 = 14.4 / (1.5 * SCALE_FACTOR);

/// A picture of the structure for a document (transparent PNG, at most `max_side` pixels a
/// side), drawn in ChemDraw's style, and the size it is drawn at, in points at ChemDraw's
/// scale. `None` with no atoms.
pub fn preview_png(mol: &Molecule, max_side: u32) -> Option<(Vec<u8>, [f32; 2])> {
    if mol.is_blank() {
        return None;
    }
    let svg = molecule_to_svg_styled(mol, &StyleConfig::chemdraw())?;
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb = egui_font().0;
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).ok()?;
    let size = tree.size();
    let scale = (max_side as f32 / size.width().max(size.height())).min(1.0);
    let w = (size.width() * scale).ceil().max(1.0) as u32;
    let h = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let png = pixmap.encode_png().ok()?;
    Some((png, [size.width() * PT_PER_PX, size.height() * PT_PER_PX]))
}

/// An empty transparent PNG of `w` × `h` pixels (the picture of an object with nothing drawn yet).
pub fn blank_png(w: u32, h: u32) -> Vec<u8> {
    resvg::tiny_skia::Pixmap::new(w.max(1), h.max(1)).and_then(|p| p.encode_png().ok()).unwrap_or_default()
}

/// PNG bytes of the structure with a TRANSPARENT background (modern Office pastes the registered
/// "PNG" format as an image).
pub fn molecule_to_png(mol: &Molecule) -> Option<Vec<u8>> {
    render_pixmap(mol, None)?.encode_png().ok()
}

/// CF_DIB byte blob: BITMAPINFOHEADER + bottom-up 24bpp BI_RGB pixels — the most broadly
/// accepted DIB layout. (32bpp/top-down DIBs are rejected by some apps, incl. PowerPoint, which
/// then falls back to pasting our text.)
pub fn molecule_to_dib(mol: &Molecule) -> Option<Vec<u8>> {
    // 24bpp DIB can't carry alpha, so keep the legacy raster on white.
    let pixmap = render_pixmap(mol, Some(resvg::tiny_skia::Color::WHITE))?;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let rgba = pixmap.data();
    let stride = (w * 3 + 3) & !3; // each row padded to a 4-byte boundary
    let image_size = stride * h;

    let mut dib = Vec::with_capacity(40 + image_size);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(w as i32).to_le_bytes());
    dib.extend_from_slice(&(h as i32).to_le_bytes()); // positive = bottom-up
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&24u16.to_le_bytes()); // bpp
    dib.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    dib.extend_from_slice(&(image_size as u32).to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    // Bottom-up rows; RGBA → BGR; pad each row to `stride`.
    for row in (0..h).rev() {
        let start = row * w * 4;
        for px in rgba[start..start + w * 4].chunks_exact(4) {
            dib.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        for _ in 0..(stride - w * 3) {
            dib.push(0);
        }
    }
    Some(dib)
}

// ─── Shared scene bounds ───────────────────────────────────────────────────────

/// Build the shared scene at `SCALE_FACTOR` px/unit (offset 0), plus the pixel bounding rect of
/// everything in it ([`MoleculeScene::bounds`]). Returned so both the SVG and EMF paths lay the
/// structure out identically. `layout` is the galley source. `None` when nothing is drawn.
fn build_scene(
    mol: &Molecule,
    style: &StyleConfig,
    layout: impl FnMut(egui::text::LayoutJob) -> Arc<egui::Galley>,
) -> Option<(MoleculeScene, egui::Rect)> {
    let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 1.0 };
    let scene = MoleculeScene::build(mol, style, xf, layout);
    let bounds = scene.bounds(mol, xf)?;
    Some((scene, bounds))
}

// ─── SVG (raster) path ─────────────────────────────────────────────────────────

fn xml_escape(c: char) -> String {
    match c {
        '&' => "&amp;".into(),
        '<' => "&lt;".into(),
        '>' => "&gt;".into(),
        '"' => "&quot;".into(),
        '\'' => "&apos;".into(),
        other => other.to_string(),
    }
}

/// An SVG paint attribute (`fill` or `stroke`) for straight RGBA, `None` being black: `black`
/// for opaque black (what everything was before colours), otherwise `rgb(…)` with an opacity
/// attribute when it is translucent.
fn svg_paint(attr: &str, rgba: Option<[u8; 4]>) -> String {
    match rgba {
        None | Some([0, 0, 0, 255]) => format!(r#"{attr}="black""#),
        Some([r, g, b, 255]) => format!(r#"{attr}="rgb({r},{g},{b})""#),
        Some([r, g, b, a]) => format!(r#"{attr}="rgb({r},{g},{b})" {attr}-opacity="{:.3}""#, a as f32 / 255.0),
    }
}

/// [`svg_paint`] for a glyph's (premultiplied) egui colour.
fn svg_glyph_fill(color: egui::Color32) -> String {
    svg_paint("fill", Some(color.to_srgba_unmultiplied()))
}

/// Render the molecule to an SVG string from the shared scene: ring-fill polygons, then bond
/// strokes / wedge fills (with a hole cut under each label), then each label glyph placed at
/// egui's exact layout position. The background stays transparent. `x`/`y` translate the scene so its bounding box sits at `MARGIN_PX`.
/// `None` when there is nothing to draw.
fn molecule_to_svg(mol: &Molecule) -> Option<String> {
    molecule_to_svg_styled(mol, &StyleConfig::default())
}

/// The shared scene of `mol` in `style`, laid out with a standalone egui font stack: the same
/// layout egui does on-screen, so label geometry matches (named fonts included: they are laid
/// out with the system font the SVG names). With its bounds, as [`build_scene`].
fn layout_scene(mol: &Molecule, style: &StyleConfig) -> Option<(MoleculeScene, egui::Rect)> {
    let mut fonts = crate::widget::fonts::standalone_fonts();
    let mut view = fonts.with_pixels_per_point(1.0);
    build_scene(mol, style, |job| view.layout_job(job))
}

/// A glyph's SVG font family: its span's named font first, then egui's.
fn glyph_family(font: Option<crate::widget::fonts::FontName>) -> String {
    let family = egui_font().1;
    match font {
        Some(f) => format!("{}, {family}", svg_family(f.as_str())),
        None => family,
    }
}

/// The opening of the picture's SVG: its size (`bounds` and a margin of `MARGIN_PX` all round)
/// and the group that moves the scene into it. Closed with `</g></svg>`.
fn svg_open(bounds: egui::Rect) -> String {
    let w = bounds.width() + 2.0 * MARGIN_PX;
    let h = bounds.height() + 2.0 * MARGIN_PX;
    let dx = MARGIN_PX - bounds.min.x;
    let dy = MARGIN_PX - bounds.min.y;
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}"><g transform="translate({dx:.3},{dy:.3})">"#
    )
}

/// The mask that cuts a hole through the ring fills and bonds under each label (rather than
/// covering them with white), so the background stays transparent around the labels. Whether
/// there is one (there is none without labels).
fn push_label_mask_svg(s: &mut String, scene: &MoleculeScene, bounds: egui::Rect) -> bool {
    let masked = !scene.labels.is_empty();
    if masked {
        let w = bounds.width() + 2.0 * MARGIN_PX;
        let h = bounds.height() + 2.0 * MARGIN_PX;
        let (x, y) = (bounds.min.x - MARGIN_PX, bounds.min.y - MARGIN_PX);
        s.push_str(&format!(
            r#"<defs><mask id="labels" maskUnits="userSpaceOnUse" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"><rect x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}" fill="white"/>"#
        ));
        for label in scene.labels.values() {
            let r = label.rect.expand(scene.label_bg_pad);
            s.push_str(&format!(
                r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="black"/>"#,
                r.min.x, r.min.y, r.width(), r.height()
            ));
        }
        s.push_str(r#"</mask></defs>"#);
    }
    masked
}

/// The structures' drop shadows: each a copy of what casts it, drawn through a filter that
/// keeps only the copy's alpha (so what is transparent casts nothing), blurred, moved and
/// flooded with the shadow's colour. Page objects cast none.
fn push_shadows_svg(s: &mut String, scene: &MoleculeScene, bounds: egui::Rect, masked: bool) {
    let w = bounds.width() + 2.0 * MARGIN_PX;
    let h = bounds.height() + 2.0 * MARGIN_PX;
    for (k, shadow) in scene.shadows.iter().enumerate() {
        let (x, y) = (bounds.min.x - MARGIN_PX, bounds.min.y - MARGIN_PX);
        let [r, g, b, a] = shadow.rgba;
        s.push_str(&format!(
            r#"<defs><filter id="shadow{k}" filterUnits="userSpaceOnUse" x="{x:.2}" y="{y:.2}" width="{w:.2}" height="{h:.2}"><feGaussianBlur in="SourceAlpha" stdDeviation="{:.3}"/><feOffset dx="{:.3}" dy="{:.3}" result="cast"/><feFlood flood-color="rgb({r},{g},{b})" flood-opacity="{:.3}"/><feComposite in2="cast" operator="in"/></filter></defs><g filter="url(#shadow{k})">"#,
            shadow.blur * 0.5,
            shadow.offset.x,
            shadow.offset.y,
            a as f32 / 255.0
        ));
        let labels: Vec<&crate::widget::scene::SceneLabel> = shadow.labels.iter().filter_map(|a| scene.labels.get(a)).collect();
        push_structure_svg(s, scene, Some(shadow), masked, &labels);
        s.push_str("</g>");
    }
}

/// The glyphs of `galley`, laid out from `base`, each a `<text>` at egui's own position.
fn push_glyphs_svg(s: &mut String, base: egui::Pos2, galley: &crate::widget::rich_text::RichGalley) {
    for g in galley.glyph_placements() {
        let ch = xml_escape(g.chr);
        let style_attr = if g.italic { r#" font-style="italic""# } else { "" };
        let y = base.y + g.baseline_y;
        let family = glyph_family(g.font);
        let fill = svg_glyph_fill(g.color);
        let mut emit = |gx: f32| {
            s.push_str(&format!(
                r#"<text x="{gx:.3}" y="{y:.3}" font-size="{:.3}" font-family="{family}"{style_attr} {fill} xml:space="preserve">{ch}</text>"#,
                g.size
            ));
        };
        emit(base.x + g.x);
        if let Some(dxb) = g.bold_offset {
            emit(base.x + g.x + dxb); // faux-bold double strike, as egui does on-screen
        }
    }
}

/// [`molecule_to_svg`] in `style`.
fn molecule_to_svg_styled(mol: &Molecule, style: &StyleConfig) -> Option<String> {
    let (scene, bounds) = layout_scene(mol, style)?;
    let mut s = svg_open(bounds);
    let masked = push_label_mask_svg(&mut s, &scene, bounds);

    // The shadows, under all of the structures' ring fills, bonds and labels.
    push_shadows_svg(&mut s, &scene, bounds, masked);

    let labels: Vec<&crate::widget::scene::SceneLabel> = scene.labels.values().collect();
    push_structure_svg(&mut s, &scene, None, masked, &labels);

    // Page objects (arrows, texts) over everything else.
    for obj in &scene.objects {
        for st in &obj.strokes {
            s.push_str(&format!(
                r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" stroke="black" stroke-width="{:.3}" stroke-linecap="butt"/>"#,
                st.a.x, st.a.y, st.b.x, st.b.y, st.width
            ));
        }
        for fill in &obj.fills {
            let pts = fill.iter().map(|p| format!("{:.2},{:.2}", p.x, p.y)).collect::<Vec<_>>().join(" ");
            s.push_str(&format!(r#"<polygon points="{pts}" fill="black"/>"#));
        }
        for line in &obj.texts {
            push_glyphs_svg(&mut s, line.rect.min, &line.galley);
        }
    }

    s.push_str("</g></svg>");
    Some(s)
}

/// The ring fills and bonds of `scene` (through the labels' mask when `masked`), then the glyphs
/// of `labels`: everything, or only what casts `shadow`.
fn push_structure_svg(
    s: &mut String,
    scene: &MoleculeScene,
    shadow: Option<&crate::widget::scene::SceneShadow>,
    masked: bool,
    labels: &[&crate::widget::scene::SceneLabel],
) {
    if masked {
        s.push_str(r#"<g mask="url(#labels)">"#);
    }

    // Ring fills (backmost). Translucent RGBA → fill + fill-opacity.
    for (i, poly) in scene.ring_fills.iter().enumerate() {
        if shadow.is_some_and(|sh| !sh.ring_fills.contains(&i)) {
            continue;
        }
        let pts = poly
            .points
            .iter()
            .map(|p| format!("{:.2},{:.2}", p.x, p.y))
            .collect::<Vec<_>>()
            .join(" ");
        let [r, g, b, a] = poly.rgba;
        s.push_str(&format!(
            r#"<polygon points="{pts}" fill="rgb({r},{g},{b})" fill-opacity="{:.3}"/>"#,
            a as f32 / 255.0
        ));
    }

    // Bonds: stroked lines + filled stereo wedges, in the bond's colour (black by default).
    for (i, bond) in scene.bonds.iter().enumerate() {
        if shadow.is_some_and(|sh| !sh.bonds.contains(&i)) {
            continue;
        }
        let stroke = svg_paint("stroke", bond.color);
        for st in &bond.strokes {
            s.push_str(&format!(
                r#"<line x1="{:.2}" y1="{:.2}" x2="{:.2}" y2="{:.2}" {stroke} stroke-width="{:.3}" stroke-linecap="round"/>"#,
                st.a.x, st.a.y, st.b.x, st.b.y, st.width
            ));
        }
        let fill_attr = svg_paint("fill", bond.color);
        for fill in &bond.fills {
            let pts = fill
                .iter()
                .map(|p| format!("{:.2},{:.2}", p.x, p.y))
                .collect::<Vec<_>>()
                .join(" ");
            s.push_str(&format!(r#"<polygon points="{pts}" {fill_attr}/>"#));
        }
    }

    if masked {
        s.push_str("</g>");
    }
    for label in labels {
        push_glyphs_svg(s, label.rect.min, &label.galley);
    }
}

/// A font family name for an SVG `font-family` attribute: quoted, with what XML reserves escaped.
fn svg_family(name: &str) -> String {
    let escaped: String = name.chars().filter(|c| *c != '\'').map(xml_escape).collect();
    format!("'{escaped}'")
}

// ─── EMF (GDI vector) path ─────────────────────────────────────────────────────

/// Metafile units per scene pixel: GDI coordinates are whole numbers, so the metafile is drawn
/// in sixteenths of a pixel (mapped back onto pixels), which keeps bond ends, line widths and
/// glyph outlines where the scene puts them instead of on the nearest pixel.
const EMF_UNITS: f32 = 16.0;

/// One piece of a glyph outline, in picture pixels (the scene moved as [`svg_open`] moves it).
#[derive(Clone, Copy, Debug, PartialEq)]
enum OutlineSegment {
    Move([f32; 2]),
    Line([f32; 2]),
    Cubic([[f32; 2]; 3]),
    Close,
}

/// A filled glyph outline and its colour (straight RGBA).
struct Outline {
    segments: Vec<OutlineSegment>,
    rgba: [u8; 4],
}

/// The outlines of the glyphs `push` writes (as [`push_glyphs_svg`] does): the very shapes the
/// raster path fills, since the same SVG text is laid out by the same code with the same fonts.
fn glyph_outlines(bounds: egui::Rect, push: impl FnOnce(&mut String)) -> Vec<Outline> {
    use resvg::usvg;
    fn collect(group: &usvg::Group, text: Option<usvg::Transform>, out: &mut Vec<Outline>) {
        for node in group.children() {
            match node {
                usvg::Node::Group(g) => collect(g, text, out),
                // A text's outlines are in the text's own coordinates.
                usvg::Node::Text(t) => collect(t.flattened(), Some(t.abs_transform()), out),
                usvg::Node::Path(p) => {
                    let Some(fill) = p.fill() else { continue };
                    let usvg::Paint::Color(c) = fill.paint() else { continue };
                    let ts = text.unwrap_or_else(|| p.abs_transform());
                    let at = |mut q: resvg::tiny_skia::Point| {
                        ts.map_point(&mut q);
                        [q.x, q.y]
                    };
                    let mut segments = Vec::new();
                    let mut last = [0.0f32; 2];
                    for seg in p.data().segments() {
                        use resvg::tiny_skia::PathSegment as S;
                        match seg {
                            S::MoveTo(q) => {
                                last = at(q);
                                segments.push(OutlineSegment::Move(last));
                            }
                            S::LineTo(q) => {
                                last = at(q);
                                segments.push(OutlineSegment::Line(last));
                            }
                            // GDI has cubic curves only: the same curve, its degree raised.
                            S::QuadTo(c1, q) => {
                                let (c1, end) = (at(c1), at(q));
                                let toward = |from: [f32; 2]| [from[0] + (c1[0] - from[0]) * 2.0 / 3.0, from[1] + (c1[1] - from[1]) * 2.0 / 3.0];
                                segments.push(OutlineSegment::Cubic([toward(last), toward(end), end]));
                                last = end;
                            }
                            S::CubicTo(c1, c2, q) => {
                                last = at(q);
                                segments.push(OutlineSegment::Cubic([at(c1), at(c2), last]));
                            }
                            S::Close => segments.push(OutlineSegment::Close),
                        }
                    }
                    let alpha = (fill.opacity().get() * 255.0).round() as u8;
                    out.push(Outline { segments, rgba: [c.red, c.green, c.blue, alpha] });
                }
                usvg::Node::Image(_) => {}
            }
        }
    }

    let mut svg = svg_open(bounds);
    let empty = svg.len();
    push(&mut svg);
    if svg.len() == empty {
        return Vec::new();
    }
    svg.push_str("</g></svg>");
    let opt = usvg::Options { fontdb: egui_font().0, ..Default::default() };
    let mut out = Vec::new();
    if let Ok(tree) = usvg::Tree::from_str(&svg, &opt) {
        collect(tree.root(), None, &mut out);
    }
    out
}

/// The structures' blurred shadows alone, as the raster path draws them: the part of the
/// picture they cover (its left, top, width and height in picture pixels) and its pixels,
/// premultiplied BGRA, top row first. `None` when nothing casts a visible shadow.
fn shadow_bitmap(scene: &MoleculeScene, bounds: egui::Rect) -> Option<([u32; 4], Vec<u8>)> {
    if scene.shadows.iter().all(|s| s.rgba[3] == 0) {
        return None;
    }
    let mut svg = svg_open(bounds);
    let masked = push_label_mask_svg(&mut svg, scene, bounds);
    push_shadows_svg(&mut svg, scene, bounds, masked);
    svg.push_str("</g></svg>");
    let opt = resvg::usvg::Options { fontdb: egui_font().0, ..Default::default() };
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).ok()?;
    let (w, h) = (tree.size().width().ceil() as u32, tree.size().height().ceil() as u32);
    if w == 0 || h == 0 || w > 4000 || h > 4000 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    // Only the rows and columns with something in them.
    let data = pixmap.data();
    let alpha = |x: u32, y: u32| data[(y * w + x) as usize * 4 + 3];
    let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| alpha(x, y) != 0)).collect();
    let cols: Vec<u32> = (0..w).filter(|&x| (0..h).any(|y| alpha(x, y) != 0)).collect();
    let (top, bottom) = (*rows.first()?, *rows.last()?);
    let (left, right) = (*cols.first()?, *cols.last()?);
    let (cw, ch) = (right - left + 1, bottom - top + 1);
    let mut bgra = Vec::with_capacity((cw * ch) as usize * 4);
    for y in top..=bottom {
        let row = &data[((y * w + left) as usize * 4)..((y * w + right + 1) as usize * 4)];
        for px in row.as_chunks::<4>().0 {
            bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    Some(([left, top, cw, ch], bgra))
}

/// The part of the polygon `points` inside `rect` (Sutherland–Hodgman, one side of the rect at
/// a time); fewer than three points when there is none.
fn polygon_in_rect(points: &[egui::Pos2], rect: egui::Rect) -> Vec<egui::Pos2> {
    // Each side: which coordinate, its limit, and whether inside is below it.
    let sides = [(0, rect.min.x, false), (0, rect.max.x, true), (1, rect.min.y, false), (1, rect.max.y, true)];
    let mut out = points.to_vec();
    for (axis, limit, below) in sides {
        let at = |p: egui::Pos2| if axis == 0 { p.x } else { p.y };
        let inside = |p: egui::Pos2| if below { at(p) <= limit } else { at(p) >= limit };
        let input = std::mem::take(&mut out);
        for (i, &p) in input.iter().enumerate() {
            let q = input[(i + 1) % input.len()];
            if inside(p) {
                out.push(p);
            }
            if inside(p) != inside(q) {
                out.push(p + (q - p) * ((limit - at(p)) / (at(q) - at(p))));
            }
        }
        if out.len() < 3 {
            return Vec::new();
        }
    }
    out
}

/// Twice the area of the polygon `points` (unsigned).
fn polygon_area2(points: &[egui::Pos2]) -> f32 {
    let n = points.len();
    (0..n).map(|i| points[i].x * points[(i + 1) % n].y - points[(i + 1) % n].x * points[i].y).sum::<f32>().abs()
}

/// Render the molecule to an Enhanced Metafile (GDI) and return the HENHMETAFILE handle, for the
/// CF_ENHMETAFILE clipboard format (what PowerPoint/Word paste as a picture, and what an embedded
/// OLE object uses for its on-page presentation), in the default style. See
/// [`molecule_to_emf_styled`]. None if empty. The returned handle is owned by the caller.
pub fn molecule_to_emf(mol: &Molecule) -> Option<isize> {
    molecule_to_emf_styled(mol, &StyleConfig::default())
}

/// [`molecule_to_emf`] in `style`. Everything comes from the shared scene, drawn as the canvas
/// draws it: bond strokes at their own widths with flat ends, ring fills and stereo wedges, all
/// cut away under each label's rect (grown by the label background's padding; the cut is in the
/// shapes themselves, as Word misplaces a GDI clip region in this metafile's units), and every glyph
/// at egui's layout position as the filled outline of the font the raster path uses (so sub- and
/// superscripts, named fonts and faux bold come out as on-screen, whatever fonts the reader
/// has). GDI has neither blur nor alpha, so the blurred shadows are one translucent bitmap under
/// the structures, and translucent colours are blended over white.
pub fn molecule_to_emf_styled(mol: &Molecule, style: &StyleConfig) -> Option<isize> {
    #[repr(C)]
    struct Point_ { x: i32, y: i32 }
    #[repr(C)]
    struct LogBrush { style: u32, color: u32, hatch: usize }
    #[repr(C)]
    struct BitmapInfoHeader {
        size: u32, width: i32, height: i32, planes: u16, bit_count: u16, compression: u32,
        size_image: u32, x_ppm: i32, y_ppm: i32, clr_used: u32, clr_important: u32,
    }
    #[repr(C)]
    struct BlendFunction { op: u8, flags: u8, alpha: u8, format: u8 }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateEnhMetaFileW(h: isize, f: *const u16, r: *const i32, d: *const u16) -> isize;
        fn CloseEnhMetaFile(h: isize) -> isize;
        fn SetMapMode(h: isize, mode: i32) -> i32;
        fn SetWindowExtEx(h: isize, x: i32, y: i32, old: *mut core::ffi::c_void) -> i32;
        fn SetViewportExtEx(h: isize, x: i32, y: i32, old: *mut core::ffi::c_void) -> i32;
        fn MoveToEx(h: isize, x: i32, y: i32, p: *mut core::ffi::c_void) -> i32;
        fn LineTo(h: isize, x: i32, y: i32) -> i32;
        fn Polygon(h: isize, pts: *const Point_, n: i32) -> i32;
        fn PolyPolygon(h: isize, pts: *const Point_, counts: *const i32, n: i32) -> i32;
        fn PolyBezierTo(h: isize, pts: *const Point_, n: u32) -> i32;
        fn BeginPath(h: isize) -> i32;
        fn CloseFigure(h: isize) -> i32;
        fn EndPath(h: isize) -> i32;
        fn FillPath(h: isize) -> i32;
        fn SetPolyFillMode(h: isize, mode: i32) -> i32;
        fn CreatePen(style: i32, width: i32, color: u32) -> isize;
        fn ExtCreatePen(style: u32, width: u32, brush: *const LogBrush, n: u32, dashes: *const u32) -> isize;
        fn CreateSolidBrush(color: u32) -> isize;
        fn SelectObject(h: isize, o: isize) -> isize;
        fn DeleteObject(o: isize) -> i32;
        fn SetBkMode(h: isize, m: i32) -> i32;
        fn CreateCompatibleDC(h: isize) -> isize;
        fn DeleteDC(h: isize) -> i32;
        fn CreateDIBSection(h: isize, info: *const BitmapInfoHeader, usage: u32, bits: *mut *mut u8, section: isize, offset: u32) -> isize;
        fn GdiAlphaBlend(dst: isize, x: i32, y: i32, w: i32, h: i32, src: isize, sx: i32, sy: i32, sw: i32, sh: i32, f: BlendFunction) -> i32;
    }
    if mol.is_blank() {
        return None;
    }
    const BLACK: u32 = 0x0000_0000;
    const WHITE: u32 = 0x00FF_FFFF;

    let (scene, bounds) = layout_scene(mol, style)?;

    // Scene pixels → picture pixels (as the SVG's) → metafile units.
    let dx = MARGIN_PX - bounds.min.x;
    let dy = MARGIN_PX - bounds.min.y;
    let unit = |v: f32| (v * EMF_UNITS).round() as i32;
    let tx = |x: f32| unit(x + dx);
    let ty = |y: f32| unit(y + dy);

    // GDI solid colours have no alpha; a translucent one is approximated over a white page
    // (the one part of the EMF picture that assumes a white background).
    let over_white = |rgba: [u8; 4]| -> u32 {
        let a = rgba[3] as f32 / 255.0;
        let blend = |v: u8| (v as f32 * a + 255.0 * (1.0 - a)).round() as u32;
        (blend(rgba[2]) << 16) | (blend(rgba[1]) << 8) | blend(rgba[0])
    };

    // What is not GDI's to draw: the glyph outlines and the shadows' pixels.
    let label_outlines = glyph_outlines(bounds, |s| {
        for label in scene.labels.values() {
            push_glyphs_svg(s, label.rect.min, &label.galley);
        }
    });
    let text_outlines = glyph_outlines(bounds, |s| {
        for line in scene.objects.iter().flat_map(|o| &o.texts) {
            push_glyphs_svg(s, line.rect.min, &line.galley);
        }
    });
    let shadows = shadow_bitmap(&scene, bounds);

    unsafe {
        let hdc = CreateEnhMetaFileW(0, std::ptr::null(), std::ptr::null(), std::ptr::null());
        if hdc == 0 {
            return None;
        }
        // Sixteen units to the pixel, both ways (MM_ANISOTROPIC).
        SetMapMode(hdc, 8);
        SetWindowExtEx(hdc, EMF_UNITS as i32, EMF_UNITS as i32, std::ptr::null_mut());
        SetViewportExtEx(hdc, 1, 1, std::ptr::null_mut());
        SetBkMode(hdc, 1); // TRANSPARENT
        let no_pen = CreatePen(5, 0, WHITE); // PS_NULL
        let old_pen = SelectObject(hdc, no_pen);

        // A filled polygon in `color`, without an outline and without what of it is in `cuts`:
        // those parts are second outlines, which the alternate fill mode leaves empty.
        let polygon = |points: &[egui::Pos2], color: u32, cuts: &[egui::Rect]| {
            let holes: Vec<Vec<egui::Pos2>> = cuts
                .iter()
                .map(|&cut| polygon_in_rect(points, cut))
                .filter(|hole| polygon_area2(hole) > 1e-3)
                .collect();
            let brush = CreateSolidBrush(color);
            let old = SelectObject(hdc, brush);
            let pts: Vec<Point_> = std::iter::once(points).chain(holes.iter().map(Vec::as_slice)).flatten().map(|p| Point_ { x: tx(p.x), y: ty(p.y) }).collect();
            if holes.is_empty() {
                Polygon(hdc, pts.as_ptr(), pts.len() as i32);
            } else {
                let counts: Vec<i32> = std::iter::once(points.len()).chain(holes.iter().map(Vec::len)).map(|n| n as i32).collect();
                SetPolyFillMode(hdc, 1); // ALTERNATE
                PolyPolygon(hdc, pts.as_ptr(), counts.as_ptr(), counts.len() as i32);
            }
            SelectObject(hdc, old);
            DeleteObject(brush);
        };
        // A line of `width` pixels in `color`, its ends cut square at its end points (as the
        // canvas's line segments are): PS_GEOMETRIC | PS_ENDCAP_FLAT | PS_JOIN_MITER. One that
        // runs into any of `cuts` is the polygon it covers instead, with that cut out of it.
        let stroke = |a: egui::Pos2, b: egui::Pos2, width: f32, color: u32, cuts: &[egui::Rect]| {
            let along = (b - a).normalized();
            let side = egui::vec2(-along.y, along.x) * (width * 0.5);
            let covered = [a + side, b + side, b - side, a - side];
            if cuts.iter().any(|&cut| polygon_area2(&polygon_in_rect(&covered, cut)) > 1e-3) {
                return polygon(&covered, color, cuts);
            }
            let brush = LogBrush { style: 0, color, hatch: 0 };
            let pen = ExtCreatePen(0x0001_0000 | 0x0000_0200 | 0x0000_2000, unit(width).max(1) as u32, &brush, 0, std::ptr::null());
            SelectObject(hdc, pen);
            MoveToEx(hdc, tx(a.x), ty(a.y), std::ptr::null_mut());
            LineTo(hdc, tx(b.x), ty(b.y));
            SelectObject(hdc, no_pen);
            DeleteObject(pen);
        };
        // Filled glyph outlines (already in picture pixels).
        let fill_outlines = |outlines: &[Outline]| {
            let pt = |p: [f32; 2]| Point_ { x: unit(p[0]), y: unit(p[1]) };
            SetPolyFillMode(hdc, 2); // WINDING, how font outlines are filled
            for outline in outlines.iter().filter(|o| o.rgba[3] > 0) {
                let brush = CreateSolidBrush(over_white(outline.rgba));
                let old = SelectObject(hdc, brush);
                BeginPath(hdc);
                for seg in &outline.segments {
                    match *seg {
                        OutlineSegment::Move(p) => {
                            let p = pt(p);
                            MoveToEx(hdc, p.x, p.y, std::ptr::null_mut());
                        }
                        OutlineSegment::Line(p) => {
                            let p = pt(p);
                            LineTo(hdc, p.x, p.y);
                        }
                        OutlineSegment::Cubic(c) => {
                            let pts = c.map(pt);
                            PolyBezierTo(hdc, pts.as_ptr(), 3);
                        }
                        OutlineSegment::Close => {
                            CloseFigure(hdc);
                        }
                    }
                }
                EndPath(hdc);
                FillPath(hdc);
                SelectObject(hdc, old);
                DeleteObject(brush);
            }
        };

        // The shadows, under every structure's ring fills, bonds and labels.
        if let Some(([left, top, w, h], bgra)) = &shadows {
            let info = BitmapInfoHeader {
                size: 40, width: *w as i32, height: -(*h as i32), planes: 1, bit_count: 32, compression: 0,
                size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0,
            };
            let mem = CreateCompatibleDC(0);
            let mut bits: *mut u8 = std::ptr::null_mut();
            let bitmap = CreateDIBSection(mem, &info, 0, &mut bits, 0, 0);
            if bitmap != 0 && !bits.is_null() {
                std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits, bgra.len());
                let old = SelectObject(mem, bitmap);
                // AC_SRC_OVER with the bitmap's own (premultiplied) alpha.
                let blend = BlendFunction { op: 0, flags: 0, alpha: 255, format: 1 };
                GdiAlphaBlend(
                    hdc, unit(*left as f32), unit(*top as f32), unit(*w as f32), unit(*h as f32),
                    mem, 0, 0, *w as i32, *h as i32, blend,
                );
                SelectObject(mem, old);
            }
            if bitmap != 0 {
                DeleteObject(bitmap);
            }
            DeleteDC(mem);
        }

        // Ring fills and bonds are cut away under each label (rather than covered with white),
        // so the background stays transparent there: the rects the canvas paints white.
        let cuts: Vec<egui::Rect> = scene.labels.values().map(|l| l.rect.expand(scene.label_bg_pad)).collect();
        // Ring fills (backmost): filled polygons, no outline.
        for poly in scene.ring_fills.iter().filter(|p| p.rgba[3] > 0) {
            polygon(&poly.points, over_white(poly.rgba), &cuts);
        }
        // Bonds: stroked lines + filled stereo wedges, black unless coloured.
        for bond in scene.bonds.iter().filter(|b| b.color.is_none_or(|c| c[3] > 0)) {
            let color = bond.color.map_or(BLACK, over_white);
            for st in &bond.strokes {
                stroke(st.a, st.b, st.width, color, &cuts);
            }
            for fill in &bond.fills {
                polygon(fill, color, &cuts);
            }
        }

        // Labels.
        fill_outlines(&label_outlines);

        // Page objects: arrow shafts and heads, then their text.
        for obj in &scene.objects {
            for st in &obj.strokes {
                stroke(st.a, st.b, st.width, BLACK, &[]);
            }
            for fill in &obj.fills {
                polygon(fill, BLACK, &[]);
            }
        }
        fill_outlines(&text_outlines);

        SelectObject(hdc, old_pen);
        DeleteObject(no_pen);
        let hemf = CloseEnhMetaFile(hdc);
        (hemf != 0).then_some(hemf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, BondStereo};

    /// Labeled heteroatoms (O, N with H), a placed double bond, a solid wedge and a ring fill.
    fn bounds_sample() -> Molecule {
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        let c3 = m.add_atom("C".to_string(), [0.75, 1.3], 0);
        let o = m.add_atom("O".to_string(), [-1.0, -0.8], 0);
        let n = m.add_atom("N".to_string(), [2.6, 0.9], 0);
        m.add_bond(c1, c2, BondOrder::Double);
        m.add_bond(c2, c3, BondOrder::Single);
        m.add_bond(c3, c1, BondOrder::Single);
        let w = m.add_bond(c1, o, BondOrder::Single);
        m.bond_by_id_mut(w).unwrap().stereo = BondStereo::WedgeUp;
        m.add_bond(c2, n, BondOrder::Single);
        m.set_ring_fill(vec![c1, c2, c3], [0, 200, 0, 128]);
        m
    }

    /// Pins the exported picture's bounds (atoms ∪ ring fills ∪ bonds ∪ labels) to fixed numbers,
    /// so moving the bounds computation cannot shift the picture's size or placement.
    #[test]
    fn scene_bounds_are_pinned() {
        let mol = bounds_sample();
        let style = StyleConfig::default();
        let mut fonts = egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            egui::FontDefinitions::default(),
        );
        let mut view = fonts.with_pixels_per_point(1.0);
        let (_, b) = build_scene(&mol, &style, |job| view.layout_job(job)).unwrap();
        let expected = [-106.156, -70.0, 205.068, 80.333];
        for (got, want) in [b.min.x, b.min.y, b.max.x, b.max.y].into_iter().zip(expected) {
            assert!((got - want).abs() < 1e-3, "bounds {b:?}, expected {expected:?}");
        }
        let svg = molecule_to_svg(&mol).unwrap();
        assert!(
            svg.starts_with(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="335" height="174" viewBox="0 0 335 174"><g transform="translate(118.156,82.000)">"#
            ),
            "picture size and placement: {}",
            &svg[..120]
        );
    }

    /// Pins the whole SVG (FNV-1a of its elements, sorted: labels come out of a `HashMap`, so
    /// their order varies run to run) of a structure with folded group hydrogens, a charge,
    /// placed double bonds, a wedge and a ring fill, so the lookups behind the scene can change
    /// without changing a single drawn coordinate.
    #[test]
    fn svg_output_is_pinned() {
        let mut mol = bounds_sample();
        let n = mol.atoms[4].id;
        for (i, y) in [(0, 0.9), (1, -0.1)] {
            let h = mol.add_atom("H".to_string(), [3.4 + i as f32 * 0.1, y], 0);
            mol.atom_by_id_mut(h).unwrap().group_h = true;
            mol.add_bond(n, h, BondOrder::Single);
        }
        let c4 = mol.add_atom("C".to_string(), [0.75, 2.8], 0);
        mol.add_bond(mol.atoms[2].id, c4, BondOrder::Double);
        let s = mol.add_atom("S".to_string(), [-0.5, 3.5], -1);
        mol.add_bond(c4, s, BondOrder::Single);
        let svg = molecule_to_svg(&mol).unwrap();
        assert!(!svg.contains(">H</text><text x=\"3"), "the group hydrogens fold into NH2");
        let mut elements: Vec<&str> = svg.split('<').collect();
        elements.sort_unstable();
        let fnv = elements.concat().bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
        assert_eq!((svg.len(), fnv), (2394, 15025168934221154562), "{svg}");
    }

    #[test]
    fn preview_is_drawn_at_chemdraws_scale() {
        let mut mol = Molecule::default();
        let a = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [1.5, 0.0], 0);
        mol.add_bond(a, b, crate::molecule::BondOrder::Single);
        let (png, [w, h]) = preview_png(&mol, 2400).expect("a picture");
        assert_eq!(&png[1..4], b"PNG");
        // One 14.4 pt bond plus the margins on both sides.
        assert!((w - (14.4 + 2.0 * MARGIN_PX * PT_PER_PX)).abs() < 2.0, "width {w} pt");
        assert!(h > 0.0);
        let (small, _) = preview_png(&mol, 20).unwrap();
        let width = u32::from_be_bytes([small[16], small[17], small[18], small[19]]);
        assert!(width <= 20, "fits {width}");
        assert!(preview_png(&Molecule::default(), 100).is_none());
        assert_eq!(&blank_png(4, 2)[1..4], b"PNG");
    }

    #[test]
    fn svg_draws_bonds_and_heteroatom_labels() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Double);
        let svg = molecule_to_svg(&mol).unwrap();
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert_eq!(svg.matches("<line").count(), 2, "double bond renders two lines");
        assert!(svg.contains(">O</text>"), "oxygen gets a label");
        assert!(!svg.contains(">C</text>"), "carbon stays a skeletal vertex");
    }

    /// Arrows and text are part of the picture, and are enough of one on their own.
    #[test]
    fn svg_and_emf_draw_arrows_and_text() {
        use crate::molecule::ArrowKind;
        let mut mol = Molecule::default();
        mol.add_arrow([0.0, 0.0], [4.5, 0.0], ArrowKind::Reaction);
        mol.add_arrow([0.0, 2.0], [4.5, 2.0], ArrowKind::Equilibrium);
        mol.add_text([0.5, -0.5], "K_2CO_3");
        let svg = molecule_to_svg(&mol).expect("arrows alone are a picture");
        assert_eq!(svg.matches("<line").count(), 3, "one shaft, and two for the equilibrium");
        assert_eq!(svg.matches("<polygon").count(), 4, "a full head is two triangles, a half head one");
        assert!(svg.contains(">K</text>") && svg.contains(">O</text>"), "{svg}");
        assert!(molecule_to_png(&mol).is_some());
        let emf = molecule_to_emf(&mol).expect("an EMF of the arrows");
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn DeleteEnhMetaFile(h: isize) -> i32;
        }
        unsafe { DeleteEnhMetaFile(emf) };
    }

    /// Named fonts reach the SVG (by name, with egui's font after them) and Symbol text is drawn
    /// as its Greek.
    #[test]
    fn svg_and_emf_draw_texts_in_their_fonts() {
        let mut mol = Molecule::default();
        mol.add_text([0.0, 0.0], "{@Times New Roman,14:x}{@Symbol:p}y");
        let svg = molecule_to_svg(&mol).expect("text alone is a picture");
        assert!(svg.contains("font-family=\"'Times New Roman', ") && svg.contains(">x</text>"), "{svg}");
        assert!(svg.contains(">π</text>") && !svg.contains(">p</text>"), "{svg}");
        assert!(molecule_to_png(&mol).is_some());
        let emf = molecule_to_emf(&mol).expect("an EMF of the text");
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn DeleteEnhMetaFile(h: isize) -> i32;
        }
        unsafe { DeleteEnhMetaFile(emf) };
    }

    #[test]
    fn labels_cut_holes_instead_of_painting_white() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Single);
        let svg = molecule_to_svg(&mol).unwrap();
        assert!(svg.contains(r#"<mask id="labels""#), "labels mask the drawing");
        assert!(svg.contains(r#"mask="url(#labels)""#));
        assert_eq!(svg.matches(r#"fill="white""#).count(), 1, "the only white is the mask's own");

        // Without labels there is no mask at all.
        let mut bare = Molecule::default();
        let a = bare.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = bare.add_atom("C".to_string(), [1.5, 0.0], 0);
        bare.add_bond(a, b, BondOrder::Single);
        assert!(!molecule_to_svg(&bare).unwrap().contains("mask"));

        // The preview has a transparent background and no white box behind the label.
        let (png, _) = preview_png(&mol, 2400).unwrap();
        let pixmap = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
        assert!(pixmap.data().chunks_exact(4).any(|p| p[3] == 0), "background is transparent");
        assert!(!pixmap.data().chunks_exact(4).any(|p| p == [255, 255, 255, 255]), "no white boxes");
    }

    #[test]
    fn svg_renders_wedge_and_ring_fill() {
        // Solid wedge → a black polygon; a filled ring → a translucent polygon.
        let mut mol = Molecule::default();
        let a = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [1.5, 0.0], 0);
        let bond = mol.add_bond(a, b, BondOrder::Single);
        mol.bond_by_id_mut(bond).unwrap().stereo = BondStereo::WedgeUp;
        let svg = molecule_to_svg(&mol).unwrap();
        assert!(svg.contains(r#"fill="black""#), "solid wedge is a black polygon");

        // A triangle ring, filled green.
        let mut ring = Molecule::default();
        let r1 = ring.add_atom("C".to_string(), [0.0, 0.0], 0);
        let r2 = ring.add_atom("C".to_string(), [1.5, 0.0], 0);
        let r3 = ring.add_atom("C".to_string(), [0.75, 1.3], 0);
        ring.add_bond(r1, r2, BondOrder::Single);
        ring.add_bond(r2, r3, BondOrder::Single);
        ring.add_bond(r3, r1, BondOrder::Single);
        ring.set_ring_fill(vec![r1, r2, r3], [0, 200, 0, 128]);
        let svg = molecule_to_svg(&ring).unwrap();
        assert!(svg.contains("fill-opacity"), "ring fill is a translucent polygon");
        assert!(svg.contains("rgb(0,200,0)"), "ring fill keeps its color");
    }

    /// Ethanol, its C–O bond red and its O blue, beside a translucent green text.
    fn coloured() -> Molecule {
        let mut mol = Molecule::default();
        let c1 = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c2 = mol.add_atom("C".to_string(), [1.3, 0.75], 0);
        let o = mol.add_atom("O".to_string(), [2.6, 0.0], 0);
        mol.add_bond(c1, c2, BondOrder::Single);
        let co = mol.add_bond(c2, o, BondOrder::Single);
        mol.bond_by_id_mut(co).unwrap().color = Some([220, 20, 30, 255]);
        mol.atom_by_id_mut(o).unwrap().color = Some([10, 40, 230, 255]);
        let t = mol.add_text([0.0, 3.0], "THF");
        if let Some(crate::molecule::PageObject::Text(t)) = mol.object_by_id_mut(t) {
            t.color = Some([0, 128, 0, 128]);
        }
        mol
    }

    #[test]
    fn svg_draws_line_and_text_colours_independently() {
        let svg = molecule_to_svg(&coloured()).unwrap();
        assert!(svg.contains(r#"stroke="rgb(220,20,30)""#), "the red bond's lines: {svg}");
        let mut wedged = coloured();
        let co = wedged.bonds.iter().find(|b| b.color.is_some()).unwrap().id;
        wedged.bond_by_id_mut(co).unwrap().stereo = BondStereo::WedgeUp;
        let wedge = molecule_to_svg(&wedged).unwrap();
        assert!(wedge.contains(r#"fill="rgb(220,20,30)"/>"#), "the red bond's wedge: {wedge}");
        assert!(svg.contains(r#"stroke="black""#), "the C–C bond stays black");
        let glyphs = |ch: char| -> Vec<&str> {
            svg.split("<text ").filter(|t| t.contains(&format!(">{ch}</text>"))).collect()
        };
        assert!(glyphs('O').iter().all(|t| t.contains(r#"fill="rgb(10,40,230)""#)), "the O is blue: {svg}");
        // The label's H (THF's H is the text's).
        assert_eq!(glyphs('H').iter().filter(|t| t.contains(r#"fill="rgb(10,40,230)""#)).count(), 1, "and its H: {svg}");
        assert!(!svg.contains(r#"stroke="rgb(10,40,230)""#), "the label's colour is not the bond's");
        assert!(
            glyphs('T').iter().all(|t| t.contains(r#"fill="rgb(0,128,0)" fill-opacity="0.502""#)),
            "the translucent green text: {svg}"
        );
        assert!(!svg.contains("feDropShadow"), "no shadow unless set");
    }

    /// Give every atom of `atoms` the shadow `shadow` (their structure casts it).
    fn shadow_atoms(mol: &mut Molecule, atoms: &[u32], shadow: crate::molecule::Shadow) {
        for a in mol.atoms.iter_mut().filter(|a| atoms.contains(&a.id)) {
            a.shadow = Some(shadow);
        }
    }

    fn shadow_all(mol: &mut Molecule, shadow: crate::molecule::Shadow) {
        let ids: Vec<u32> = mol.atoms.iter().map(|a| a.id).collect();
        shadow_atoms(mol, &ids, shadow);
    }

    #[test]
    fn svg_casts_the_shadow_of_the_structures_only() {
        let mut mol = coloured();
        let plain = molecule_to_svg(&mol).unwrap();
        shadow_all(&mut mol, crate::molecule::Shadow { color: [40, 50, 60, 128], offset: [0.2, 0.1], blur: 0.1 });
        let svg = molecule_to_svg(&mol).unwrap();
        // Blurred, moved and flooded alpha of a copy of what casts it.
        assert!(
            svg.contains(r#"<feGaussianBlur in="SourceAlpha" stdDeviation="2.500"/><feOffset dx="10.000" dy="5.000" result="cast"/><feFlood flood-color="rgb(40,50,60)" flood-opacity="0.502"/><feComposite in2="cast" operator="in"/>"#),
            "{svg}"
        );
        // The shadow's group holds a copy of the bonds and labels, before the structure itself,
        // and the page text after both casts none.
        let group = svg.find(r#"<g filter="url(#shadow0)">"#).expect("a shadowed group");
        // The structure itself is the last group through the labels' mask.
        let structure = svg.rfind(r#"<g mask="url(#labels)">"#).unwrap();
        let shadow = &svg[group..structure];
        assert!(shadow.contains("<line") && shadow.contains(">O</text>"), "{svg}");
        let foreground = &svg[structure..];
        assert_eq!(foreground.matches("<line").count(), shadow.matches("<line").count(), "every bond, once each");
        let text = svg.find(">T</text>").unwrap();
        assert!(text > structure && !shadow.contains(">T</text>"), "the text casts no shadow: {svg}");
        // The picture grows to hold the shadow.
        let width = |s: &str| s.split("width=\"").nth(1).unwrap().split('"').next().unwrap().parse::<f32>().unwrap();
        assert!(width(&svg) > width(&plain), "{} vs {}", width(&svg), width(&plain));
        // And the raster shows it.
        assert!(render_pixmap(&mol, None).is_some());
    }

    /// Ethanol beside a lone, labelled nitrogen: two structures.
    fn two_structures() -> (Molecule, Vec<u32>, u32) {
        let mut mol = coloured();
        let ethanol: Vec<u32> = mol.atoms.iter().map(|a| a.id).collect();
        let n = mol.add_atom("N".to_string(), [6.0, 0.0], 0);
        (mol, ethanol, n)
    }

    #[test]
    fn svg_shadows_only_the_shadowed_structure() {
        let (mut mol, ethanol, n) = two_structures();
        shadow_atoms(&mut mol, &[n], crate::molecule::Shadow::default());
        let svg = molecule_to_svg(&mol).unwrap();
        assert_eq!(svg.matches("<filter ").count(), 1, "{svg}");
        let group = svg.find(r#"<g filter="url(#shadow0)">"#).unwrap();
        let end = svg.rfind(r#"<g mask="url(#labels)">"#).unwrap();
        let shadow = &svg[group..end];
        // The nitrogen's label (NH3) casts it; ethanol's bonds and its O do not.
        assert!(shadow.contains(">N</text>"), "{shadow}");
        assert!(!shadow.contains(">O</text>") && !shadow.contains("<line"), "{shadow}");

        // Shadowing ethanol instead: its bonds and O, not the nitrogen.
        let (mut mol, ..) = two_structures();
        shadow_atoms(&mut mol, &ethanol[..1], crate::molecule::Shadow::default());
        let svg = molecule_to_svg(&mol).unwrap();
        let group = svg.find(r#"<g filter="url(#shadow0)">"#).unwrap();
        let end = svg.rfind(r#"<g mask="url(#labels)">"#).unwrap();
        let shadow = &svg[group..end];
        assert_eq!(shadow.matches("<line").count(), 2, "{shadow}");
        assert!(shadow.contains(">O</text>") && !shadow.contains(">N</text>"), "{shadow}");
    }

    /// Every structure's shadow comes before any structure's ring fills, bonds and labels, which
    /// come in that order.
    #[test]
    fn svg_draws_all_shadows_then_ring_fills_then_bonds_then_labels() {
        let (mut mol, ethanol, n) = two_structures();
        let tri: Vec<u32> = [[0.0, 4.0], [1.5, 4.0], [0.75, 5.3]].into_iter().map(|p| mol.add_atom("C".into(), p, 0)).collect();
        for k in 0..3 {
            mol.add_bond(tri[k], tri[(k + 1) % 3], BondOrder::Single);
        }
        mol.set_ring_fill(tri.clone(), [0, 200, 0, 255]);
        shadow_atoms(&mut mol, &ethanol, crate::molecule::Shadow { color: [1, 2, 3, 255], ..Default::default() });
        shadow_atoms(&mut mol, &tri, crate::molecule::Shadow { color: [4, 5, 6, 255], ..Default::default() });
        shadow_atoms(&mut mol, &[n], crate::molecule::Shadow { color: [7, 8, 9, 255], ..Default::default() });
        let svg = molecule_to_svg(&mol).unwrap();
        let at = |needle: &str| svg.find(needle).unwrap_or_else(|| panic!("{needle} in {svg}"));
        let shadows = [at(r#"url(#shadow0)">"#), at(r#"url(#shadow1)">"#), at(r#"url(#shadow2)">"#)];
        assert!(svg.contains("rgb(1,2,3)") && svg.contains("rgb(4,5,6)") && svg.contains("rgb(7,8,9)"));
        let structures = svg.rfind(r#"<g mask="url(#labels)">"#).unwrap();
        assert!(shadows.iter().all(|&s| s < structures), "{svg}");
        let foreground = &svg[structures..];
        let fill = foreground.find("<polygon").unwrap();
        let bond = foreground.find("<line").unwrap();
        let label = foreground.find("<text").unwrap();
        assert!(fill < bond && bond < label, "{foreground}");
    }

    /// A shadow is under every structure, not only its own: one cast right onto another
    /// structure's red bond leaves the bond red.
    #[test]
    fn a_shadow_cast_onto_another_structure_goes_under_it() {
        let mut mol = Molecule::default();
        // The red bond comes first, the one casting a shadow onto it second.
        let a = mol.add_atom("C".into(), [0.0, 1.0], 0);
        let b = mol.add_atom("C".into(), [1.5, 1.0], 0);
        let red = mol.add_bond(a, b, BondOrder::Single);
        mol.bond_by_id_mut(red).unwrap().color = Some([255, 0, 0, 255]);
        let c = mol.add_atom("C".into(), [0.0, 0.0], 0);
        let d = mol.add_atom("C".into(), [1.5, 0.0], 0);
        mol.add_bond(c, d, BondOrder::Single);
        shadow_atoms(&mut mol, &[c, d], crate::molecule::Shadow { color: [0, 255, 0, 255], offset: [0.0, 1.0], blur: 0.0 });
        let pixmap = render_pixmap(&mol, None).unwrap();
        let count = |f: &dyn Fn(u8, u8, u8) -> bool| {
            pixmap.pixels().iter().filter(|p| p.alpha() == 255 && f(p.red(), p.green(), p.blue())).count()
        };
        let (reds, greens) = (count(&|r, g, _| r > 200 && g < 60), count(&|r, g, _| g > 200 && r < 60));
        assert!(reds > 20 && greens * 10 < reds, "{reds} red, {greens} green pixels");
    }

    #[test]
    fn emf_draws_colours_and_the_shadows() {
        unsafe extern "system" {
            fn DeleteEnhMetaFile(h: isize) -> i32;
        }
        let (mut mol, _, n) = two_structures();
        shadow_atoms(&mut mol, &[n], crate::molecule::Shadow::default());
        let h = molecule_to_emf(&mol).expect("a metafile");
        unsafe { DeleteEnhMetaFile(h) };
        shadow_all(&mut mol, crate::molecule::Shadow::default());
        let h = molecule_to_emf(&mol).expect("a metafile");
        unsafe { DeleteEnhMetaFile(h) };
    }

    #[test]
    fn png_and_dib_are_well_formed() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Single);

        let png = molecule_to_png(&mol).expect("png");
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "PNG signature");

        let dib = molecule_to_dib(&mol).expect("dib");
        assert_eq!(u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]), 40, "BITMAPINFOHEADER");
        let h = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
        assert!(h > 0, "bottom-up DIB (positive height)");
    }

    /// Timing of `preview_png` on long chains (every 10th atom an O, so labels are laid out
    /// too). Run in release: `cargo test --release --lib preview_scales -- --ignored --nocapture`.
    /// The time per atom should stay flat as the chain grows.
    #[test]
    #[ignore = "timing; run explicitly in release"]
    fn preview_scales_linearly_with_chain_length() {
        for n in [25_000u32, 50_000, 100_000] {
            let mut mol = Molecule::default();
            let mut prev = None;
            for i in 0..n {
                let element = if i % 10 == 5 { "O" } else { "C" };
                let y = if i % 2 == 0 { 0.0 } else { 0.75 };
                let id = mol.add_atom(element.to_string(), [i as f32 * 1.3, y], 0);
                if let Some(p) = prev {
                    mol.add_bond(p, id, BondOrder::Single);
                }
                prev = Some(id);
            }
            let start = std::time::Instant::now();
            let mut fonts = egui::epaint::text::Fonts::new(
                egui::epaint::text::TextOptions::default(),
                egui::FontDefinitions::default(),
            );
            let mut view = fonts.with_pixels_per_point(1.0);
            build_scene(&mol, &StyleConfig::chemdraw(), |job| view.layout_job(job)).unwrap();
            let scene = start.elapsed();
            let start = std::time::Instant::now();
            preview_png(&mol, 2400).unwrap();
            let total = start.elapsed();
            eprintln!("{n} atoms: scene {scene:?}, preview_png {total:?}");
        }
    }

    #[test]
    fn emf_is_a_valid_metafile() {
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetEnhMetaFileBits(h: isize, n: u32, b: *mut u8) -> u32;
            fn DeleteEnhMetaFile(h: isize) -> i32;
        }
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Double);

        let hemf = molecule_to_emf(&mol).expect("emf handle");
        unsafe {
            let size = GetEnhMetaFileBits(hemf, 0, std::ptr::null_mut());
            assert!(size > 88, "non-trivial EMF");
            let mut buf = vec![0u8; size as usize];
            GetEnhMetaFileBits(hemf, size, buf.as_mut_ptr());
            assert_eq!(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]), 1, "EMR_HEADER");
            assert_eq!(&buf[40..44], b" EMF", "EMF signature");
            DeleteEnhMetaFile(hemf);
        }
    }

    // ─── The metafile against the scene and the raster picture ───

    /// 5-benzylidenebarbituric acid: three labelled carbonyl oxygens, two ring NH whose
    /// hydrogens sit on opposite sides, an exocyclic double bond and a benzene ring.
    fn benzylidene_barbiturate() -> Molecule {
        let mut m = Molecule::default();
        let at = |c: [f32; 2], r: f32, k: i32| {
            let a = (30.0 + 60.0 * k as f32).to_radians();
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        };
        let ring: Vec<u32> = ["C", "C", "N", "C", "N", "C"]
            .iter()
            .enumerate()
            .map(|(k, el)| m.add_atom(el.to_string(), at([0.0, 0.0], 1.5, k as i32), 0))
            .collect();
        for k in 0..6 {
            m.add_bond(ring[k], ring[(k + 1) % 6], BondOrder::Single);
        }
        for k in [1, 3, 5] {
            let o = m.add_atom("O".to_string(), at([0.0, 0.0], 3.0, k), 0);
            m.add_bond(ring[k as usize], o, BondOrder::Double);
        }
        let ch = m.add_atom("C".to_string(), at([0.0, 0.0], 3.0, 0), 0);
        m.add_bond(ring[0], ch, BondOrder::Double);
        let centre = {
            let p = at([0.0, 0.0], 3.0, 0);
            [p[0] + 3.0, p[1]]
        };
        let phenyl: Vec<u32> = (0..6).map(|k| m.add_atom("C".to_string(), at(centre, 1.5, k), 0)).collect();
        for k in 0..6 {
            let order = if k % 2 == 0 { BondOrder::Double } else { BondOrder::Single };
            m.add_bond(phenyl[k], phenyl[(k + 1) % 6], order);
        }
        // The ring vertex nearest the CH.
        let nearest = *phenyl
            .iter()
            .min_by(|a, b| {
                let d = |id: &u32| {
                    let p = m.atoms.iter().find(|x| x.id == *id).unwrap().pos;
                    let q = m.atoms.iter().find(|x| x.id == ch).unwrap().pos;
                    (p[0] - q[0]).hypot(p[1] - q[1])
                };
                d(a).total_cmp(&d(b))
            })
            .unwrap();
        m.add_bond(ch, nearest, BondOrder::Single);
        m
    }

    /// The records of a metafile (type, then the bytes after the type and size), and the bounds
    /// of what it draws in pixels (left, top, right, bottom, both ends included). Frees it.
    fn emf_records(hemf: isize) -> (Vec<(u32, Vec<u8>)>, [i32; 4]) {
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetEnhMetaFileBits(h: isize, n: u32, b: *mut u8) -> u32;
            fn DeleteEnhMetaFile(h: isize) -> i32;
        }
        let buf = unsafe {
            let size = GetEnhMetaFileBits(hemf, 0, std::ptr::null_mut());
            let mut buf = vec![0u8; size as usize];
            GetEnhMetaFileBits(hemf, size, buf.as_mut_ptr());
            DeleteEnhMetaFile(hemf);
            buf
        };
        let int = |at: usize| i32::from_le_bytes(buf[at..at + 4].try_into().unwrap());
        let bounds = [int(8), int(12), int(16), int(20)];
        let mut records = Vec::new();
        let mut at = 0;
        while at + 8 <= buf.len() {
            let size = int(at + 4) as usize;
            records.push((int(at) as u32, buf[at + 8..at + size].to_vec()));
            at += size;
        }
        (records, bounds)
    }

    fn ints(bytes: &[u8]) -> Vec<i32> {
        bytes.as_chunks::<4>().0.iter().map(|c| i32::from_le_bytes(*c)).collect()
    }

    const EMR_MOVETOEX: u32 = 27;
    const EMR_POLYPOLYGON16: u32 = 91;
    const EMR_LINETO: u32 = 54;
    const EMR_BEGINPATH: u32 = 59;
    const EMR_FILLPATH: u32 = 62;
    const EMR_EXTTEXTOUTA: u32 = 83;
    const EMR_EXTTEXTOUTW: u32 = 84;
    const EMR_ALPHABLEND: u32 = 114;

    /// The metafile's bond lines run between the scene's own end points, to a sixteenth of a
    /// pixel, and it sets no clip region (Word misplaces one in these units).
    #[test]
    fn emf_draws_the_scenes_bonds_without_a_clip_region() {
        let mol = benzylidene_barbiturate();
        let style = StyleConfig::default();
        let (scene, bounds) = layout_scene(&mol, &style).unwrap();
        let (dx, dy) = (MARGIN_PX - bounds.min.x, MARGIN_PX - bounds.min.y);
        let unit = |v: f32| (v * EMF_UNITS).round() as i32;
        let (records, _) = emf_records(molecule_to_emf(&mol).unwrap());
        assert_eq!(scene.labels.len(), 5, "three O and two NH");
        // EMR_EXCLUDECLIPRECT, EMR_INTERSECTCLIPRECT, EMR_SELECTCLIPPATH, EMR_EXTSELECTCLIPRGN.
        assert!(!records.iter().any(|r| [29, 30, 67, 75].contains(&r.0)), "no clipping");

        // The lines before the first glyph outline are the bonds' strokes, in order: all but
        // the two a label's rect reaches (the outer lines of two C=O, beside the O), which are
        // shapes with that corner cut off.
        let first_path = records.iter().position(|r| r.0 == EMR_BEGINPATH).expect("glyph outlines");
        let ends: Vec<Vec<i32>> = records[..first_path]
            .iter()
            .filter(|r| r.0 == EMR_MOVETOEX || r.0 == EMR_LINETO)
            .map(|r| ints(&r.1))
            .collect();
        let strokes: Vec<Vec<i32>> = scene
            .bonds
            .iter()
            .flat_map(|b| &b.strokes)
            .flat_map(|s| [vec![unit(s.a.x + dx), unit(s.a.y + dy)], vec![unit(s.b.x + dx), unit(s.b.y + dy)]])
            .collect();
        assert!(strokes.len() >= 2 * 24, "{} stroke ends", strokes.len());
        let cut = records.iter().filter(|r| r.0 == EMR_POLYPOLYGON16).count();
        assert_eq!(cut, 2);
        assert_eq!(ends.len(), strokes.len() - 2 * cut);
        let mut rest = strokes.chunks(2);
        for line in ends.chunks(2) {
            assert!(rest.any(|stroke| stroke == line), "{line:?} is not the scene's next stroke");
        }
    }

    /// Labels are glyph outlines, not text the reader's GDI lays out in a font of its own, and
    /// every outline lies inside its label's rect.
    #[test]
    fn emf_labels_are_the_layouts_glyph_outlines() {
        let mol = benzylidene_barbiturate();
        let (records, _) = emf_records(molecule_to_emf(&mol).unwrap());
        assert!(!records.iter().any(|r| r.0 == EMR_EXTTEXTOUTW || r.0 == EMR_EXTTEXTOUTA), "no GDI text");
        // O, O, O, N, H, N, H.
        assert_eq!(records.iter().filter(|r| r.0 == EMR_FILLPATH).count(), 7);

        let (scene, bounds) = layout_scene(&mol, &StyleConfig::default()).unwrap();
        let (dx, dy) = (MARGIN_PX - bounds.min.x, MARGIN_PX - bounds.min.y);
        for label in scene.labels.values() {
            let outlines = glyph_outlines(bounds, |s| push_glyphs_svg(s, label.rect.min, &label.galley));
            assert!(!outlines.is_empty());
            let rect = label.rect.translate(egui::vec2(dx, dy)).expand(1.0);
            let mut ink = egui::Rect::NOTHING;
            for seg in outlines.iter().flat_map(|o| &o.segments) {
                let points: Vec<[f32; 2]> = match *seg {
                    OutlineSegment::Move(p) | OutlineSegment::Line(p) => vec![p],
                    OutlineSegment::Cubic(c) => c.to_vec(),
                    OutlineSegment::Close => vec![],
                };
                points.iter().for_each(|p| ink.extend_with(egui::pos2(p[0], p[1])));
            }
            assert!(rect.contains_rect(ink), "glyphs {ink:?} outside their label {rect:?}");
            // The glyphs fill most of the label's width: they are where the layout put them.
            assert!(ink.width() > 0.7 * label.rect.width(), "glyphs {ink:?} in {rect:?}");
        }
    }

    /// What a label covers is cut out of the shapes themselves: a ring fill with a labelled
    /// atom is drawn with a hole of the label's rect (and its padding) in it, and so is a bond
    /// line that runs under a label.
    #[test]
    fn emf_cuts_fills_and_crossing_bonds_at_the_labels() {
        let mol = filled_ring_with_a_label();
        let (scene, _) = layout_scene(&mol, &StyleConfig::default()).unwrap();
        assert_eq!(scene.labels.len(), 1);
        let (records, _) = emf_records(molecule_to_emf(&mol).unwrap());
        // The fill and the bond across the ring, each an outline and a hole.
        let holed: Vec<Vec<i32>> = records.iter().filter(|r| r.0 == EMR_POLYPOLYGON16).map(|r| ints(&r.1)).collect();
        assert_eq!(holed.len(), 2, "{} records", records.len());
        // After the bounds (4 ints): the number of polygons.
        assert!(holed.iter().all(|r| r[4] == 2), "{holed:?}");
    }

    /// A triangle of two carbons and a nitrogen, filled dark, and a bond between two more
    /// carbons drawn right across the nitrogen's label.
    fn filled_ring_with_a_label() -> Molecule {
        let mut m = Molecule::default();
        let n = m.add_atom("N".to_string(), [0.0, 0.0], 0);
        let a = m.add_atom("C".to_string(), [1.3, 0.75], 0);
        let b = m.add_atom("C".to_string(), [1.3, -0.75], 0);
        m.add_bond(n, a, BondOrder::Single);
        m.add_bond(a, b, BondOrder::Single);
        m.add_bond(b, n, BondOrder::Single);
        m.set_ring_fill(vec![n, a, b], [0, 0, 90, 255]);
        let left = m.add_atom("C".to_string(), [-1.5, 0.1], 0);
        let right = m.add_atom("C".to_string(), [3.0, 0.1], 0);
        m.add_bond(left, right, BondOrder::Single);
        m
    }

    /// A blurred shadow is one translucent bitmap under the structure (GDI cannot blur), and
    /// there is none without a shadow.
    #[test]
    fn emf_shadow_is_one_blurred_bitmap() {
        let mut mol = benzylidene_barbiturate();
        let (records, _) = emf_records(molecule_to_emf(&mol).unwrap());
        assert_eq!(records.iter().filter(|r| r.0 == EMR_ALPHABLEND).count(), 0);
        let lines = records.iter().filter(|r| r.0 == EMR_LINETO).count();

        shadow_all(&mut mol, crate::molecule::Shadow::default());
        let (records, _) = emf_records(molecule_to_emf(&mol).unwrap());
        assert_eq!(records.iter().filter(|r| r.0 == EMR_ALPHABLEND).count(), 1);
        // No hard-edged copies of the bonds.
        assert_eq!(records.iter().filter(|r| r.0 == EMR_LINETO).count(), lines);
    }

    /// The metafile, played back at one pixel per scene pixel on white: its bounds' left and
    /// top in the picture, its width and height, and its pixels (BGRA, top row first).
    fn play_emf(hemf: isize) -> ([i32; 2], [usize; 2], Vec<u8>) {
        #[repr(C)]
        struct Header {
            size: u32, width: i32, height: i32, planes: u16, bit_count: u16, compression: u32,
            size_image: u32, x_ppm: i32, y_ppm: i32, clr_used: u32, clr_important: u32,
        }
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetEnhMetaFileHeader(h: isize, n: u32, b: *mut u8) -> u32;
            fn PlayEnhMetaFile(dc: isize, h: isize, rect: *const i32) -> i32;
            fn DeleteEnhMetaFile(h: isize) -> i32;
            fn CreateCompatibleDC(h: isize) -> isize;
            fn DeleteDC(h: isize) -> i32;
            fn CreateDIBSection(h: isize, info: *const Header, usage: u32, bits: *mut *mut u8, section: isize, offset: u32) -> isize;
            fn SelectObject(h: isize, o: isize) -> isize;
            fn DeleteObject(o: isize) -> i32;
        }
        unsafe {
            let mut header = [0u8; 88];
            GetEnhMetaFileHeader(hemf, 88, header.as_mut_ptr());
            let int = |at: usize| i32::from_le_bytes(header[at..at + 4].try_into().unwrap());
            let (left, top, right, bottom) = (int(8), int(12), int(16), int(20));
            let (w, h) = ((right - left + 1) as usize, (bottom - top + 1) as usize);
            let info = Header {
                size: 40, width: w as i32, height: -(h as i32), planes: 1, bit_count: 32, compression: 0,
                size_image: 0, x_ppm: 0, y_ppm: 0, clr_used: 0, clr_important: 0,
            };
            let mem = CreateCompatibleDC(0);
            let mut bits: *mut u8 = std::ptr::null_mut();
            let bitmap = CreateDIBSection(mem, &info, 0, &mut bits, 0, 0);
            assert!(bitmap != 0 && !bits.is_null());
            std::ptr::write_bytes(bits, 0xFF, w * h * 4);
            let old = SelectObject(mem, bitmap);
            let rect = [0, 0, w as i32, h as i32];
            assert!(PlayEnhMetaFile(mem, hemf, rect.as_ptr()) != 0);
            let pixels = std::slice::from_raw_parts(bits, w * h * 4).to_vec();
            SelectObject(mem, old);
            DeleteObject(bitmap);
            DeleteDC(mem);
            DeleteEnhMetaFile(hemf);
            ([left, top], [w, h], pixels)
        }
    }

    /// Where a grid of pixels is dark (`true`), from a per-pixel darkness test.
    fn dark_grid(w: usize, h: usize, dark: impl Fn(usize, usize) -> bool) -> Vec<Vec<bool>> {
        (0..h).map(|y| (0..w).map(|x| dark(x, y)).collect()).collect()
    }

    /// The share of `a`'s dark pixels with a dark pixel of `b` within `reach` pixels.
    fn covered(a: &[Vec<bool>], b: &[Vec<bool>], reach: i32) -> f32 {
        let (h, w) = (a.len() as i32, a[0].len() as i32);
        let (mut all, mut hit) = (0u32, 0u32);
        for y in 0..h {
            for x in 0..w {
                if !a[y as usize][x as usize] {
                    continue;
                }
                all += 1;
                let near = (-reach..=reach).any(|j| {
                    (-reach..=reach).any(|i| {
                        let (xx, yy) = (x + i, y + j);
                        xx >= 0 && yy >= 0 && xx < w && yy < h && b[yy as usize][xx as usize]
                    })
                });
                hit += near as u32;
            }
        }
        assert!(all > 300, "{all} dark pixels");
        hit as f32 / all as f32
    }

    /// Played back, the metafile is the raster picture (which is the canvas's scene, with the
    /// canvas's font): the same ink in the same places, labels and their gaps included. Writes
    /// both to `target/emf_compare` for a look.
    #[test]
    fn emf_plays_back_as_the_raster_picture() {
        for case in 0..3 {
            let shadowed = case == 1;
            let mut mol = if case == 2 { filled_ring_with_a_label() } else { benzylidene_barbiturate() };
            if shadowed {
                shadow_all(&mut mol, crate::molecule::Shadow::default());
            }
            let raster = render_pixmap(&mol, Some(resvg::tiny_skia::Color::WHITE)).unwrap();
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("emf_compare");
            std::fs::create_dir_all(&dir).unwrap();
            let name = ["plain", "shadowed", "cut"][case];
            let hemf = molecule_to_emf(&mol).unwrap();
            // The metafile itself too, to put into a document by hand.
            #[link(name = "gdi32")]
            unsafe extern "system" {
                fn CopyEnhMetaFileW(h: isize, file: *const u16) -> isize;
                fn DeleteEnhMetaFile(h: isize) -> i32;
            }
            let file: Vec<u16> = dir.join(format!("{name}.emf")).to_string_lossy().encode_utf16().chain([0]).collect();
            unsafe { DeleteEnhMetaFile(CopyEnhMetaFileW(hemf, file.as_ptr())) };
            let ([left, top], [w, h], emf) = play_emf(hemf);
            let mut played = resvg::tiny_skia::Pixmap::new(w as u32, h as u32).unwrap();
            for (px, src) in played.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(emf.as_chunks::<4>().0) {
                px.copy_from_slice(&[src[2], src[1], src[0], 255]);
            }
            played.save_png(dir.join(format!("{name}_emf.png"))).unwrap();
            raster.save_png(dir.join(format!("{name}_raster.png"))).unwrap();

            let level = |x: usize, y: usize| -> [u32; 2] {
                let e = &emf[(y * w + x) * 4..];
                let (rx, ry) = (x as i32 + left, y as i32 + top);
                let r = if rx >= 0 && ry >= 0 && (rx as u32) < raster.width() && (ry as u32) < raster.height() {
                    let p = raster.pixel(rx as u32, ry as u32).unwrap();
                    (p.red() as u32 + p.green() as u32 + p.blue() as u32) / 3
                } else {
                    255
                };
                [(e[0] as u32 + e[1] as u32 + e[2] as u32) / 3, r]
            };
            let from_emf = dark_grid(w, h, |x, y| level(x, y)[0] < 96);
            let from_raster = dark_grid(w, h, |x, y| level(x, y)[1] < 96);
            let (a, b) = (covered(&from_emf, &from_raster, 2), covered(&from_raster, &from_emf, 2));
            assert!(a > 0.985 && b > 0.985, "{name}: {a} of the metafile's ink is in the raster, {b} the other way");

            // The shadow is the raster's: as much soft grey, not a second hard-edged structure.
            let grey = |which: usize| (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).filter(|&(x, y)| (120..250).contains(&level(x, y)[which])).count() as f32;
            if shadowed {
                let (e, r) = (grey(0), grey(1));
                assert!(e > 2000.0 && (e / r - 1.0).abs() < 0.15, "{e} grey pixels in the metafile, {r} in the raster");
            }
        }
    }
}
