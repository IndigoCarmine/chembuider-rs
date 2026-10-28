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

use super::{Molecule, depict};
use super::lookup::MolIndex;
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

/// [`molecule_to_svg`] in `style`.
fn molecule_to_svg_styled(mol: &Molecule, style: &StyleConfig) -> Option<String> {
    let family = egui_font().1;

    // Standalone egui font stack: the same layout egui does on-screen, so label geometry matches
    // (named fonts included: they are laid out with the system font the SVG names).
    let mut fonts = crate::widget::fonts::standalone_fonts();
    let mut view = fonts.with_pixels_per_point(1.0);
    let (scene, bounds) = build_scene(mol, style, |job| view.layout_job(job))?;
    // A glyph's font family: its span's named font first, then egui's.
    let families = |font: Option<crate::widget::fonts::FontName>| match font {
        Some(f) => format!("{}, {family}", svg_family(f.as_str())),
        None => family.clone(),
    };

    let w = bounds.width() + 2.0 * MARGIN_PX;
    let h = bounds.height() + 2.0 * MARGIN_PX;
    let dx = MARGIN_PX - bounds.min.x;
    let dy = MARGIN_PX - bounds.min.y;

    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}"><g transform="translate({dx:.3},{dy:.3})">"#
    );

    // Labels cut a hole through the ring fills and bonds under them (rather than covering them
    // with white), so the background stays transparent around the labels.
    let masked = !scene.labels.is_empty();
    if masked {
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

    // The structures' drop shadows, under all of their ring fills, bonds and labels: each a copy
    // of what casts it, drawn through a filter that keeps only the copy's alpha (so what is
    // transparent casts nothing), blurred, moved and flooded with the shadow's colour. Page
    // objects cast none.
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
        push_structure_svg(&mut s, &scene, Some(shadow), masked, &labels, &families);
        s.push_str("</g>");
    }

    let labels: Vec<&crate::widget::scene::SceneLabel> = scene.labels.values().collect();
    push_structure_svg(&mut s, &scene, None, masked, &labels, &families);

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
            let base = line.rect.min;
            for g in line.galley.glyph_placements() {
                let ch = xml_escape(g.chr);
                let style_attr = if g.italic { r#" font-style="italic""# } else { "" };
                let y = base.y + g.baseline_y;
                let family = families(g.font);
                let fill = svg_glyph_fill(g.color);
                let mut emit = |gx: f32| {
                    s.push_str(&format!(
                        r#"<text x="{gx:.3}" y="{y:.3}" font-size="{:.3}" font-family="{family}"{style_attr} {fill} xml:space="preserve">{ch}</text>"#,
                        g.size
                    ));
                };
                emit(base.x + g.x);
                if let Some(dxb) = g.bold_offset {
                    emit(base.x + g.x + dxb);
                }
            }
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
    families: &dyn Fn(Option<crate::widget::fonts::FontName>) -> String,
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
        let base = label.rect.min;
        for g in label.galley.glyph_placements() {
            let ch = xml_escape(g.chr);
            let style_attr = if g.italic { r#" font-style="italic""# } else { "" };
            let x = base.x + g.x;
            let y = base.y + g.baseline_y;
            let family = families(g.font);
            let fill = svg_glyph_fill(g.color);
            let mut emit = |gx: f32| {
                s.push_str(&format!(
                    r#"<text x="{gx:.3}" y="{y:.3}" font-size="{:.3}" font-family="{family}"{style_attr} {fill} xml:space="preserve">{ch}</text>"#,
                    g.size
                ));
            };
            emit(x);
            if let Some(dxb) = g.bold_offset {
                emit(x + dxb); // faux-bold double strike, as egui does on-screen
            }
        }
    }
}

/// A font family name for an SVG `font-family` attribute: quoted, with what XML reserves escaped.
fn svg_family(name: &str) -> String {
    let escaped: String = name.chars().filter(|c| *c != '\'').map(xml_escape).collect();
    format!("'{escaped}'")
}

// ─── EMF (GDI vector) path ─────────────────────────────────────────────────────

/// Render the molecule to an Enhanced Metafile (GDI) and return the HENHMETAFILE handle, for the
/// CF_ENHMETAFILE clipboard format (what PowerPoint/Word paste as a picture, and what an embedded
/// OLE object uses for its on-page presentation). Geometry (scale, bond lines, double-bond
/// placement, stereo wedges, ring fills, which atoms are labeled) comes from the shared scene, so
/// it tracks the on-screen structure. Labels are drawn with GDI text (no true sub/superscripts,
/// and GDI's Arial rather than egui's font — the one place the EMF picture can drift from the
/// canvas). None if empty. The returned handle is owned by the caller.
pub fn molecule_to_emf(mol: &Molecule) -> Option<isize> {
    #[repr(C)]
    struct Point_ { x: i32, y: i32 }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateEnhMetaFileW(h: isize, f: *const u16, r: *const i32, d: *const u16) -> isize;
        fn CloseEnhMetaFile(h: isize) -> isize;
        fn MoveToEx(h: isize, x: i32, y: i32, p: *mut core::ffi::c_void) -> i32;
        fn LineTo(h: isize, x: i32, y: i32) -> i32;
        fn Polygon(h: isize, pts: *const Point_, n: i32) -> i32;
        fn CreatePen(style: i32, width: i32, color: u32) -> isize;
        fn CreateSolidBrush(color: u32) -> isize;
        fn SelectObject(h: isize, o: isize) -> isize;
        fn DeleteObject(o: isize) -> i32;
        fn SetBkMode(h: isize, m: i32) -> i32;
        fn SetTextAlign(h: isize, a: u32) -> u32;
        fn SetTextColor(h: isize, c: u32) -> u32;
        fn TextOutW(h: isize, x: i32, y: i32, s: *const u16, c: i32) -> i32;
        fn ExcludeClipRect(h: isize, l: i32, t: i32, r: i32, b: i32) -> i32;
        fn SaveDC(h: isize) -> i32;
        fn RestoreDC(h: isize, saved: i32) -> i32;
        #[allow(clippy::too_many_arguments)]
        fn CreateFontW(ch: i32, cw: i32, esc: i32, ori: i32, wt: i32, it: u32, un: u32, so: u32,
            cs: u32, op: u32, cp: u32, q: u32, pf: u32, face: *const u16) -> isize;
    }
    if mol.is_blank() {
        return None;
    }
    const BLACK: u32 = 0x0000_0000;
    const WHITE: u32 = 0x00FF_FFFF;

    let style = StyleConfig::default();
    let mut fonts = crate::widget::fonts::standalone_fonts();
    let mut view = fonts.with_pixels_per_point(1.0);
    let (scene, bounds) = build_scene(mol, &style, |job| view.layout_job(job))?;

    let dx = MARGIN_PX - bounds.min.x;
    let dy = MARGIN_PX - bounds.min.y;
    let tx = |x: f32| (x + dx).round() as i32;
    let ty = |y: f32| (y + dy).round() as i32;

    // GDI solid brushes have no alpha; approximate a translucent ring fill over a white page
    // (the one part of the EMF picture that assumes a white background).
    let over_white = |rgba: [u8; 4]| -> u32 {
        let a = rgba[3] as f32 / 255.0;
        let blend = |v: u8| (v as f32 * a + 255.0 * (1.0 - a)).round() as u32;
        (blend(rgba[2]) << 16) | (blend(rgba[1]) << 8) | blend(rgba[0])
    };

    unsafe {
        let hdc = CreateEnhMetaFileW(0, std::ptr::null(), std::ptr::null(), std::ptr::null());
        if hdc == 0 {
            return None;
        }
        SetBkMode(hdc, 1); // TRANSPARENT
        let pen_width = style.bond_width.round().max(1.0) as i32;
        let pen = CreatePen(0, pen_width, BLACK); // PS_SOLID
        let no_pen = CreatePen(5, 0, WHITE); // PS_NULL
        let black_brush = CreateSolidBrush(BLACK);
        let old_pen = SelectObject(hdc, pen);

        // Shown labels, centered on their atoms (the anchor point of the on-screen label).
        let r = (style.label_size * 0.75).round() as i32;
        let index = MolIndex::new(mol);
        /// A shown label: its atom, centre, flattened text and colour.
        type Label = (u32, i32, i32, String, Option<[u8; 4]>);
        let labels: Vec<Label> = mol
            .atoms
            .iter()
            .filter(|atom| depict::should_show_label(&index, atom.id))
            .map(|atom| {
                let c = egui::pos2(atom.pos[0] * SCALE_FACTOR, atom.pos[1] * SCALE_FACTOR);
                (atom.id, tx(c.x), ty(c.y), flatten_label(&index, atom), atom.color)
            })
            .collect();
        let face: Vec<u16> = "Arial\0".encode_utf16().collect();
        let font_h = -(style.label_size.round() as i32);

        // GDI has no blur and no alpha: each structure's shadow is a sharp copy in its colour
        // over white, of what casts one (nothing transparent).
        let shadows: Vec<(i32, i32, u32, &crate::widget::scene::SceneShadow)> = scene
            .shadows
            .iter()
            .filter(|s| s.rgba[3] > 0)
            .map(|s| (s.offset.x.round() as i32, s.offset.y.round() as i32, over_white(s.rgba), s))
            .collect();
        let transparent = |c: &Option<[u8; 4]>| c.is_some_and(|c| c[3] == 0);

        // The labels' shadows first, under every structure's ring fills and bonds.
        if !shadows.is_empty() {
            let font = CreateFontW(font_h, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr());
            let old_font = SelectObject(hdc, font);
            SetTextAlign(hdc, 6); // TA_CENTER
            for &(sx, sy, tint, shadow) in &shadows {
                SetTextColor(hdc, tint);
                for (id, cx, cy, text, color) in &labels {
                    if shadow.labels.contains(id) && !transparent(color) {
                        let utf16: Vec<u16> = text.encode_utf16().collect();
                        TextOutW(hdc, cx + sx, cy + font_h / 2 + sy, utf16.as_ptr(), utf16.len() as i32);
                    }
                }
            }
            SetTextColor(hdc, BLACK);
            SelectObject(hdc, old_font);
            DeleteObject(font);
        }

        // Ring fills and bonds are clipped out around each label (rather than covered with
        // white), so the background stays transparent there.
        let saved = SaveDC(hdc);
        for &(_, cx, cy, _, _) in &labels {
            ExcludeClipRect(hdc, cx - r, cy - r, cx + r, cy + r);
        }

        // Ring fills and bonds, moved by `(sx, sy)` pixels, in their own colours; or, for a
        // structure's shadow, its own all in `tint`.
        let draw_structure = |sx: i32, sy: i32, tint: Option<(u32, &crate::widget::scene::SceneShadow)>| {
            let pt = |p: &egui::Pos2| Point_ { x: tx(p.x) + sx, y: ty(p.y) + sy };
            // Whether the `i`th of the ring fills (`fill`) or bonds is drawn: all of them, or
            // what casts the shadow being drawn.
            let drawn = |i: usize, fill: bool, transparent: bool| match tint {
                None => true,
                Some((_, s)) => !transparent && if fill { s.ring_fills.contains(&i) } else { s.bonds.contains(&i) },
            };
            let tint = tint.map(|(c, _)| c);
            // Ring fills (backmost): filled polygons, no outline.
            for (i, poly) in scene.ring_fills.iter().enumerate() {
                if !drawn(i, true, poly.rgba[3] == 0) {
                    continue;
                }
                let brush = CreateSolidBrush(tint.unwrap_or_else(|| over_white(poly.rgba)));
                SelectObject(hdc, brush);
                SelectObject(hdc, no_pen);
                let pts: Vec<Point_> = poly.points.iter().map(pt).collect();
                Polygon(hdc, pts.as_ptr(), pts.len() as i32);
                SelectObject(hdc, pen);
                DeleteObject(brush);
            }

            // Bonds: stroked lines + filled stereo wedges, black unless coloured.
            SelectObject(hdc, pen);
            for (i, bond) in scene.bonds.iter().enumerate() {
                if !drawn(i, false, transparent(&bond.color)) {
                    continue;
                }
                let color = tint.or(bond.color.map(over_white));
                let (bond_pen, bond_brush) = match color {
                    Some(c) => (CreatePen(0, pen_width, c), CreateSolidBrush(c)),
                    None => (pen, black_brush),
                };
                if color.is_some() {
                    SelectObject(hdc, bond_pen);
                }
                for st in &bond.strokes {
                    MoveToEx(hdc, tx(st.a.x) + sx, ty(st.a.y) + sy, std::ptr::null_mut());
                    LineTo(hdc, tx(st.b.x) + sx, ty(st.b.y) + sy);
                }
                for fill in &bond.fills {
                    SelectObject(hdc, bond_brush);
                    SelectObject(hdc, no_pen);
                    let pts: Vec<Point_> = fill.iter().map(pt).collect();
                    Polygon(hdc, pts.as_ptr(), pts.len() as i32);
                    SelectObject(hdc, bond_pen);
                }
                if color.is_some() {
                    SelectObject(hdc, pen);
                    DeleteObject(bond_pen);
                    DeleteObject(bond_brush);
                }
            }
        };
        for &(sx, sy, tint, shadow) in &shadows {
            draw_structure(sx, sy, Some((tint, shadow)));
        }
        draw_structure(0, 0, None);

        // Labels: centered plain text (flattened rich label), unclipped.
        RestoreDC(hdc, saved);
        let font = CreateFontW(font_h, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr());
        let old_font = SelectObject(hdc, font);
        SetTextColor(hdc, BLACK);
        SetTextAlign(hdc, 6); // TA_CENTER
        for (_, cx, cy, text, color) in &labels {
            if let Some(c) = color {
                SetTextColor(hdc, over_white(*c));
            }
            let utf16: Vec<u16> = text.encode_utf16().collect();
            TextOutW(hdc, *cx, cy + font_h / 2, utf16.as_ptr(), utf16.len() as i32);
            if color.is_some() {
                SetTextColor(hdc, BLACK);
            }
        }

        // Page objects: arrow shafts and heads, then text lines (flattened, as the labels).
        SelectObject(hdc, pen);
        for obj in &scene.objects {
            for st in &obj.strokes {
                MoveToEx(hdc, tx(st.a.x), ty(st.a.y), std::ptr::null_mut());
                LineTo(hdc, tx(st.b.x), ty(st.b.y));
            }
            for fill in &obj.fills {
                SelectObject(hdc, black_brush);
                SelectObject(hdc, no_pen);
                let pts: Vec<Point_> = fill.iter().map(|p| Point_ { x: tx(p.x), y: ty(p.y) }).collect();
                Polygon(hdc, pts.as_ptr(), pts.len() as i32);
                SelectObject(hdc, pen);
            }
        }
        // Each styled part on its own, in its font (GDI's face of that name; Arial for the base
        // font), size, weight and slant, on its own baseline (sub/superscripts included).
        SetTextAlign(hdc, 24); // TA_LEFT | TA_BASELINE
        for obj in &scene.objects {
            for line in &obj.texts {
                for part in &line.galley.parts {
                    let text: String = part.galley.rows.iter().flat_map(|r| r.row.glyphs.iter().map(|g| g.chr)).collect();
                    let utf16: Vec<u16> = text.encode_utf16().collect();
                    let face: Vec<u16> = part.font.map_or("Arial", |f| f.as_str()).encode_utf16().chain([0]).collect();
                    let weight = if part.bold_offset.is_some() { 700 } else { 400 };
                    let height = -(part.size.round().max(1.0) as i32);
                    let part_font = CreateFontW(height, 0, 0, 0, weight, part.italic as u32, 0, 0, 1, 0, 0, 0, 0, face.as_ptr());
                    SelectObject(hdc, part_font);
                    let x = line.rect.min.x + part.offset.x;
                    let baseline = line.rect.min.y + part.offset.y + part.baseline();
                    SetTextColor(hdc, over_white(part.color.to_srgba_unmultiplied()));
                    TextOutW(hdc, tx(x), ty(baseline), utf16.as_ptr(), utf16.len() as i32);
                    SelectObject(hdc, font);
                    DeleteObject(part_font);
                }
            }
        }

        SelectObject(hdc, old_font);
        SelectObject(hdc, old_pen);
        DeleteObject(font);
        DeleteObject(black_brush);
        DeleteObject(no_pen);
        DeleteObject(pen);
        let hemf = CloseEnhMetaFile(hdc);
        (hemf != 0).then_some(hemf)
    }
}

/// The plain-text form of an atom's on-screen label (rich spans flattened, scripts inlined). Used
/// by the GDI/EMF path, which has no sub/superscript support.
fn flatten_label(mol: &MolIndex, atom: &super::Atom) -> String {
    depict::atom_label(mol, atom)
        .rich
        .spans
        .iter()
        .map(|s| s.text.as_str())
        .collect()
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
}
