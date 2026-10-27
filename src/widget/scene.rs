//! Backend-independent geometry + style computation for drawing a molecule.
//!
//! The on-screen editor (`draw.rs`, egui `Painter` backend) and the exported image
//! (`molecule::image`, SVG/resvg + EMF backends) used to compute their own, subtly different
//! geometry — different scale, bond width, double-bond offset, no stereo or rich labels in the
//! export — so the copied picture never matched the canvas. This module is the single source of
//! that geometry: given a molecule, a [`StyleConfig`], a [`RenderTransform`] and a galley source,
//! it produces a [`MoleculeScene`] — a list of backend-neutral drawing primitives (fill polygons,
//! stroked lines, filled polygons, laid-out labels). Each backend only has to turn those
//! primitives into its own draw calls, so the canvas and the exported image share one look.
//!
//! Coordinate model: primitives are in *target pixels*. [`RenderTransform`] maps molecule
//! coordinates to pixels (`coord_scale`, `offset`) and scales visual sizes — bond widths, label
//! font size, offsets — by `visual_scale`. The editor passes `coord_scale` = its zoom times
//! `SCALE_FACTOR` (see `ViewTransform::render_transform`), `visual_scale = zoom`; the exporter
//! passes `coord_scale = SCALE_FACTOR`, `visual_scale = 1`.
//! Both therefore agree exactly at the editor's zoom = 1 baseline.

use std::collections::HashMap;
use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Color32, Pos2, Rect, Vec2};

use crate::config::StyleConfig;
use crate::molecule::lookup::{AtomLookup, MolIndex};
use crate::molecule::{ArrowKind, BondOrder, BondStereo, DoublePosition, Molecule, PageObject, PageText, TextAlign};

use crate::molecule::depict::{atom_label, folded_h, should_show_label};
use super::rich_text::{RichGalley, RichTextStyle};

/// Arrowhead proportions, in line widths: what ChemDraw 25 writes for a new reaction arrow
/// (`HeadSize="2250" ArrowheadCenterSize="1969" ArrowheadWidth="563"`, hundredths of the line
/// width). The head is `ARROW_HEAD_LENGTH` long, `2 × ARROW_HEAD_HALF_WIDTH` wide, and its back
/// is notched: the shaft meets it `ARROW_HEAD_CENTER` behind the tip.
pub const ARROW_HEAD_LENGTH: f32 = 22.5;
/// See [`ARROW_HEAD_LENGTH`].
pub const ARROW_HEAD_CENTER: f32 = 19.69;
/// See [`ARROW_HEAD_LENGTH`].
pub const ARROW_HEAD_HALF_WIDTH: f32 = 5.63;
/// Distance between the two shafts of an equilibrium arrow, in line widths.
pub const ARROW_SHAFT_SPACING: f32 = 6.0;
/// Baseline-to-baseline distance of a page text's lines, in font sizes.
pub const TEXT_LINE_HEIGHT: f32 = 1.2;

// Fixed internal detail (not user-tunable): hash-wedge line count and wavy-bond resolution.
const HASH_COUNT: usize = 6;
const WAVY_STEPS: usize = 16;

/// Maps molecule coordinates to target pixels and scales visual sizes. See the module docs.
#[derive(Clone, Copy, Debug)]
pub struct RenderTransform {
    /// Molecule units → pixels for atom positions.
    pub coord_scale: f32,
    /// Added (in pixels) after scaling — the editor's `center + pan`, the exporter's margin.
    pub offset: Vec2,
    /// Multiplier for visual sizes (widths, offsets, font size). `zoom` in the editor, `1` on export.
    pub visual_scale: f32,
}

impl RenderTransform {
    pub fn apply(&self, p: [f32; 2]) -> Pos2 {
        Pos2::new(
            p[0] * self.coord_scale + self.offset.x,
            p[1] * self.coord_scale + self.offset.y,
        )
    }
}

/// A laid-out atom label: the galley plus the screen rect it occupies. The rect is positioned so
/// the label's *element symbol* (not the bounding box) is centered on the atom position.
pub struct SceneLabel {
    pub galley: RichGalley,
    pub rect: Rect,
}

/// A stroked line segment (bond line, hash, dash, wavy segment).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneStroke {
    pub a: Pos2,
    pub b: Pos2,
    pub width: f32,
}

/// The drawing primitives of one bond. `strokes` are stroked with the bond color; `fills` are
/// filled with the bond color (solid stereo wedges). Color is applied by the backend so the
/// editor can tint a hovered bond without changing the geometry.
pub struct SceneBond {
    pub bond_id: u32,
    pub strokes: Vec<SceneStroke>,
    pub fills: Vec<Vec<Pos2>>,
}

/// A colored fill polygon (a ring interior), drawn behind everything else. `color` is the
/// premultiplied egui color (for the `Painter` backend); `rgba` is the original straight
/// (un-premultiplied) RGBA (for the SVG/EMF backends, which need the true channel values).
pub struct ScenePolygon {
    pub points: Vec<Pos2>,
    pub color: Color32,
    pub rgba: [u8; 4],
}

/// Everything needed to draw a molecule, computed once and rendered by any backend. Draw order
/// is back-to-front: [`ring_fills`](Self::ring_fills), then [`bonds`](Self::bonds), then the white
/// label masks (each label rect expanded by [`label_bg_pad`](Self::label_bg_pad)), then labels.
pub struct MoleculeScene {
    pub ring_fills: Vec<ScenePolygon>,
    pub bonds: Vec<SceneBond>,
    pub labels: HashMap<u32, SceneLabel>,
    /// Padding of the white rectangle painted behind an atom label (already scaled).
    pub label_bg_pad: f32,
    /// Arrows and texts on the page ([`Molecule::objects`]), drawn over the structures.
    pub objects: Vec<SceneObject>,
}

/// One laid-out line of a page text: the galley and the rect it is painted in.
pub struct SceneText {
    pub galley: RichGalley,
    pub rect: Rect,
}

/// The drawing primitives of one page object: strokes and filled polygons (an arrow's shafts
/// and heads), laid-out text lines, and the rect covering all of it (for hit testing and
/// bounds). Color is up to the backend, as for [`SceneBond`].
pub struct SceneObject {
    pub object_id: u32,
    pub strokes: Vec<SceneStroke>,
    pub fills: Vec<Vec<Pos2>>,
    pub texts: Vec<SceneText>,
    pub rect: Rect,
}

impl MoleculeScene {
    /// Lay out labels, then build the bonds (trimmed at those labels) and ring fills of `mol`
    /// under `xf`. `layout` is the galley source, as for [`build_labels`].
    pub fn build(
        mol: &Molecule,
        style: &StyleConfig,
        xf: RenderTransform,
        layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
    ) -> Self {
        let mut layout = layout;
        let labels = build_labels(mol, style, xf, &mut layout);
        let bonds = build_bonds(mol, style, xf, &labels);
        let ring_fills = build_ring_fills(mol, xf);
        let objects = build_objects(mol, style, xf, &mut layout);
        MoleculeScene { ring_fills, bonds, labels, label_bg_pad: style.label_bg_pad * xf.visual_scale, objects }
    }

    /// The pixel rect covering everything drawn: every atom position of `mol` (under `xf`, the
    /// transform this scene was built with — bare atoms have no primitive of their own), the ring
    /// fills, bond strokes / wedge fills and label rects. `None` when there is nothing to cover.
    pub fn bounds(&self, mol: &Molecule, xf: RenderTransform) -> Option<Rect> {
        let mut r = Rect::NOTHING;
        for a in &mol.atoms {
            r.extend_with(xf.apply(a.pos));
        }
        for poly in &self.ring_fills {
            poly.points.iter().for_each(|p| r.extend_with(*p));
        }
        for b in &self.bonds {
            for s in &b.strokes {
                r.extend_with(s.a);
                r.extend_with(s.b);
            }
            for f in &b.fills {
                f.iter().for_each(|p| r.extend_with(*p));
            }
        }
        for l in self.labels.values() {
            r.extend_with(l.rect.min);
            r.extend_with(l.rect.max);
        }
        for o in &self.objects {
            r.extend_with(o.rect.min);
            r.extend_with(o.rect.max);
        }
        (r.min.x <= r.max.x && r.min.y <= r.max.y).then_some(r)
    }
}

// ─── Label layout ─────────────────────────────────────────────────────────────

/// Lay out every visible atom label. Shared by the bond pass (endpoint trimming), the white
/// background pass and the label pass, so the baseline math runs once. `layout` is any galley
/// source: an `egui::Painter` (editor) or a standalone `Fonts` view (image export).
pub fn build_labels(
    mol: &Molecule,
    style: &StyleConfig,
    xf: RenderTransform,
    mut layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> HashMap<u32, SceneLabel> {
    let text_style = RichTextStyle::new(style.label_size * xf.visual_scale, Color32::BLACK);
    let index = MolIndex::new(mol);
    let mut map = HashMap::new();
    for atom in &mol.atoms {
        if !should_show_label(&index, atom.id) {
            continue;
        }
        let label = atom_label(&index, atom);
        let galley = label.rich.layout_with(&text_style, |job| layout(job));
        let anchor = galley
            .char_range_rect(label.anchor_chars)
            .map(|r| r.center().to_vec2())
            .unwrap_or_else(|| galley.size() * 0.5);
        let sp = xf.apply(atom.pos);
        let rect = Rect::from_min_size(sp - anchor, galley.size());
        map.insert(atom.id, SceneLabel { galley, rect });
    }
    map
}

// ─── Page objects ─────────────────────────────────────────────────────────────

/// Primitives for every page object of `mol` (arrows and texts), in their order. Arrow lines are
/// as wide as bonds and their heads are sized in line widths, as ChemDraw sizes them; text is set
/// at the atom-label size. `layout` is the galley source, as for [`build_labels`].
pub fn build_objects(
    mol: &Molecule,
    style: &StyleConfig,
    xf: RenderTransform,
    mut layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> Vec<SceneObject> {
    mol.objects.iter().map(|o| build_object(o, style, xf, &mut layout)).collect()
}

/// Primitives of one page object; see [`build_objects`].
pub fn build_object(
    object: &PageObject,
    style: &StyleConfig,
    xf: RenderTransform,
    layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> SceneObject {
    match object {
        PageObject::Arrow(a) => {
            let width = style.bond_width * xf.visual_scale;
            let (strokes, fills) = arrow_primitives(xf.apply(a.tail), xf.apply(a.head), a.kind, width);
            let mut rect = Rect::NOTHING;
            for st in &strokes {
                rect.extend_with(st.a);
                rect.extend_with(st.b);
            }
            fills.iter().flatten().for_each(|p| rect.extend_with(*p));
            if strokes.is_empty() {
                rect = Rect::from_center_size(xf.apply(a.tail), Vec2::splat(width));
            }
            SceneObject { object_id: a.id, strokes, fills, texts: Vec::new(), rect: rect.expand(width * 0.5) }
        }
        PageObject::Text(t) => {
            let texts = text_lines(t, style, xf, layout);
            let rect = texts.iter().map(|l| l.rect).reduce(|a, b| a.union(b)).unwrap_or_else(|| {
                let size = style.label_size * xf.visual_scale;
                Rect::from_min_size(xf.apply(t.pos) - Vec2::new(0.0, size * 0.8), Vec2::new(1.0, size))
            });
            SceneObject { object_id: t.id, strokes: Vec::new(), fills: Vec::new(), texts, rect }
        }
    }
}

/// The lines of a page text laid out and placed: line `k`'s baseline is `k` line heights below
/// the anchor, and each line sits right of, around or left of the anchor by its alignment.
fn text_lines(
    t: &PageText,
    style: &StyleConfig,
    xf: RenderTransform,
    mut layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> Vec<SceneText> {
    let size = style.label_size * xf.visual_scale;
    let text_style = RichTextStyle::new(size, Color32::BLACK);
    let anchor = xf.apply(t.pos);
    let mut out = Vec::new();
    for (k, line) in t.lines().iter().enumerate() {
        let galley = line.layout_with(&text_style, &mut layout);
        if galley.parts.is_empty() {
            continue;
        }
        let w = galley.size().x;
        let x = match t.align {
            TextAlign::Left => anchor.x,
            TextAlign::Center => anchor.x - w * 0.5,
            TextAlign::Right => anchor.x - w,
        };
        let y = anchor.y + k as f32 * size * TEXT_LINE_HEIGHT - galley.baseline();
        let rect = Rect::from_min_size(Pos2::new(x, y), galley.size());
        out.push(SceneText { galley, rect });
    }
    out
}

/// Shafts and heads of an arrow from `tail` to `head` (pixels), lines `width` wide. A head never
/// takes more than half the arrow, so a short arrow still shows a shaft. Each head is filled as
/// two triangles (tip, barb, notch), since the notched head is not convex.
pub fn arrow_primitives(tail: Pos2, head: Pos2, kind: ArrowKind, width: f32) -> (Vec<SceneStroke>, Vec<Vec<Pos2>>) {
    let mut strokes = Vec::new();
    let mut fills = Vec::new();
    let d = head - tail;
    let len = d.length();
    if len < 1e-3 {
        return (strokes, fills);
    }
    let u = d / len;
    // Left of the direction of travel, on a y-down screen.
    let left = Vec2::new(u.y, -u.x);
    let scale = (len * 0.5 / (ARROW_HEAD_LENGTH * width)).min(1.0);
    let head_len = ARROW_HEAD_LENGTH * width * scale;
    let center = ARROW_HEAD_CENTER * width * scale;
    let half = ARROW_HEAD_HALF_WIDTH * width * scale;
    // One shaft from `a` to `b` with a head at `b`: whole, or only its half on `side`.
    let mut one = |a: Pos2, b: Pos2, dir: Vec2, side: Vec2, half_head: bool| {
        let base = b - dir * head_len;
        let notch = b - dir * center;
        strokes.push(SceneStroke { a, b: notch, width });
        fills.push(vec![b, base + side * half, notch]);
        if !half_head {
            fills.push(vec![b, notch, base - side * half]);
        }
    };
    match kind {
        ArrowKind::Reaction => one(tail, head, u, left, false),
        ArrowKind::Equilibrium => {
            let off = left * (ARROW_SHAFT_SPACING * width * 0.5);
            // Forward on the left, back on the right; each half head points outward.
            one(tail + off, head + off, u, left, true);
            one(head - off, tail - off, -u, -left, true);
        }
    }
    (strokes, fills)
}

// ─── Ring fills ───────────────────────────────────────────────────────────────

/// The colored ring-interior polygons of `mol` in target-pixel coordinates. A fill referencing a
/// removed atom, or with fewer than three vertices, is skipped.
pub fn build_ring_fills(mol: &Molecule, xf: RenderTransform) -> Vec<ScenePolygon> {
    let index = MolIndex::new(mol);
    mol.ring_fills
        .iter()
        .filter_map(|rf| {
            if rf.atoms.len() < 3 {
                return None;
            }
            let mut points = Vec::with_capacity(rf.atoms.len());
            for &id in &rf.atoms {
                points.push(xf.apply(index.atom_by_id(id)?.pos));
            }
            let [r, g, b, a] = rf.color;
            Some(ScenePolygon {
                points,
                color: Color32::from_rgba_unmultiplied(r, g, b, a),
                rgba: rf.color,
            })
        })
        .collect()
}

// ─── Bonds ────────────────────────────────────────────────────────────────────

/// Bond primitives for every drawn bond, trimmed at the atom labels in `labels`. Bonds to a
/// folded terminal hydrogen are suppressed (the H shows in the heavy atom's label), and a bond
/// swallowed whole by its labels is skipped.
pub fn build_bonds(
    mol: &Molecule,
    style: &StyleConfig,
    xf: RenderTransform,
    labels: &HashMap<u32, SceneLabel>,
) -> Vec<SceneBond> {
    let vs = xf.visual_scale;
    let width = style.bond_width * vs;
    let index = MolIndex::new(mol);
    let mut out = Vec::new();
    for bond in &mol.bonds {
        if folded_h(&index, bond.begin) || folded_h(&index, bond.end) {
            continue;
        }
        let (Some(a), Some(b)) = (index.atom_by_id(bond.begin), index.atom_by_id(bond.end)) else {
            continue;
        };
        let p1 = xf.apply(a.pos);
        let p2 = xf.apply(b.pos);

        let trim_rect = |id: u32| labels.get(&id).map(|l| l.rect.expand(style.label_trim_pad * vs));
        let Some((tp1, tp2)) =
            trimmed_bond_endpoints(p1, p2, trim_rect(bond.begin), trim_rect(bond.end))
        else {
            continue;
        };

        let mut strokes = Vec::new();
        let mut fills = Vec::new();
        match &bond.stereo {
            BondStereo::None => {
                // Whether each end carries other bonds: a bare terminal end (vinyl CH₂) has
                // nothing to make room for, so the inner line runs to the end there.
                let mut inset_ends = (false, false);
                let placement = if matches!(bond.order, BondOrder::Double) {
                    let (nx, ny) = perp_normal(p1, p2);
                    let mut neighbors: Vec<Pos2> = Vec::new();
                    for nid in index.neighbor_atom_ids(bond.begin) {
                        if nid == bond.end {
                            continue;
                        }
                        if let Some(na) = index.atom_by_id(nid) {
                            neighbors.push(xf.apply(na.pos));
                            inset_ends.0 = true;
                        }
                    }
                    for nid in index.neighbor_atom_ids(bond.end) {
                        if nid == bond.begin {
                            continue;
                        }
                        if let Some(na) = index.atom_by_id(nid) {
                            neighbors.push(xf.apply(na.pos));
                            inset_ends.1 = true;
                        }
                    }
                    // A side the user chose wins over the one the substituents suggest.
                    chosen_double_placement(bond.double_position)
                        .unwrap_or_else(|| double_bond_placement(p1, p2, (nx, ny), &neighbors))
                } else {
                    DoubleBondPlacement::Symmetric
                };
                push_bond_order(
                    &mut strokes,
                    tp1,
                    tp2,
                    &bond.order,
                    width,
                    style.double_bond_offset * vs,
                    placement,
                    inset_ends,
                );
            }
            BondStereo::WedgeUp => push_wedge_solid(&mut fills, tp1, tp2, style.wedge_width * vs),
            BondStereo::WedgeDown => push_wedge_hash(
                &mut strokes,
                (tp1, tp2),
                (p1, p2),
                style.wedge_width * vs,
                width,
            ),
            BondStereo::Bold => strokes.push(SceneStroke { a: tp1, b: tp2, width: style.bold_width * vs }),
            BondStereo::Dashed => push_dashed(&mut strokes, tp1, tp2, width, style.dash_len * vs, style.dash_gap * vs),
            BondStereo::Wavy => push_wavy(&mut strokes, tp1, tp2, width, style.wavy_amplitude * vs),
            BondStereo::Hashed => push_hashed(&mut strokes, tp1, tp2, style.wedge_width * vs * 0.6, width),
            BondStereo::HollowWedge => push_wedge_hollow(&mut strokes, tp1, tp2, style.wedge_width * vs, width),
            BondStereo::Dative => push_dative(&mut strokes, &mut fills, tp1, tp2, style.wedge_width * vs, width),
        }
        out.push(SceneBond { bond_id: bond.id, strokes, fills });
    }
    out
}

// ─── Geometry helpers (backend-neutral; shared by all renderers) ───────────────

/// Parametric `t` in `(0, 1]` where the segment `p → q` exits `rect` (slab method), for a `p`
/// inside the rect. `None` when `q` is also inside (the segment never leaves).
pub(crate) fn segment_rect_exit(p: Pos2, q: Pos2, rect: Rect) -> Option<f32> {
    let d = q - p;
    let tx = if d.x > 0.0 {
        (rect.max.x - p.x) / d.x
    } else if d.x < 0.0 {
        (rect.min.x - p.x) / d.x
    } else {
        f32::INFINITY
    };
    let ty = if d.y > 0.0 {
        (rect.max.y - p.y) / d.y
    } else if d.y < 0.0 {
        (rect.min.y - p.y) / d.y
    } else {
        f32::INFINITY
    };
    let t = tx.min(ty);
    (t.is_finite() && t <= 1.0).then_some(t.max(0.0))
}

/// Trim a bond's endpoints at the (already padded) label rects of its endpoint atoms. `None`
/// rect = unlabeled endpoint, no trim there. Returns `None` when the labels swallow the whole bond.
pub(crate) fn trimmed_bond_endpoints(
    p1: Pos2,
    p2: Pos2,
    r1: Option<Rect>,
    r2: Option<Rect>,
) -> Option<(Pos2, Pos2)> {
    let t1 = match r1.map(|r| segment_rect_exit(p1, p2, r)) {
        None => 0.0,
        Some(Some(t)) => t,
        Some(None) => return None,
    };
    let t2 = match r2.map(|r| segment_rect_exit(p2, p1, r)) {
        None => 1.0,
        Some(Some(t)) => 1.0 - t,
        Some(None) => return None,
    };
    let len = (p2 - p1).length();
    if len < 1e-3 || (t2 - t1) * len < 1.0 {
        return None;
    }
    let at = |t: f32| p1 + (p2 - p1) * t;
    Some((at(t1), at(t2)))
}

/// The placement a user-chosen [`DoublePosition`] asks for, or `None` for `Auto`.
///
/// `perp_normal` points to the right of the begin→end direction on screen (y grows downward),
/// so the inset line goes to `+1` for `Right` and `-1` for `Left`.
pub(crate) fn chosen_double_placement(position: DoublePosition) -> Option<DoubleBondPlacement> {
    match position {
        DoublePosition::Auto => None,
        DoublePosition::Center => Some(DoubleBondPlacement::Symmetric),
        DoublePosition::Right => Some(DoubleBondPlacement::Centered(1.0)),
        DoublePosition::Left => Some(DoubleBondPlacement::Centered(-1.0)),
    }
}

/// Where to place the two lines of a plain (non-stereo) double bond.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DoubleBondPlacement {
    /// Two lines symmetric about the bond axis (terminal/isolated bonds or trans-disubstituted).
    Symmetric,
    /// Main line on the axis + a shorter inset line offset toward one side (`+1`/`-1` normal side).
    Centered(f32),
}

fn push_bond_order(
    strokes: &mut Vec<SceneStroke>,
    p1: Pos2,
    p2: Pos2,
    order: &BondOrder,
    width: f32,
    double_bond_offset: f32,
    placement: DoubleBondPlacement,
    inset_ends: (bool, bool),
) {
    let stroke = |a: Pos2, b: Pos2| SceneStroke { a, b, width };
    match order {
        BondOrder::Single => strokes.push(stroke(p1, p2)),
        BondOrder::Double => match placement {
            DoubleBondPlacement::Symmetric => {
                let (nx, ny) = perp_normal(p1, p2);
                let half = double_bond_offset * 0.5;
                strokes.push(stroke(
                    Pos2::new(p1.x + nx * half, p1.y + ny * half),
                    Pos2::new(p2.x + nx * half, p2.y + ny * half),
                ));
                strokes.push(stroke(
                    Pos2::new(p1.x - nx * half, p1.y - ny * half),
                    Pos2::new(p2.x - nx * half, p2.y - ny * half),
                ));
            }
            DoubleBondPlacement::Centered(side) => {
                strokes.push(stroke(p1, p2));
                let (nx, ny) = perp_normal(p1, p2);
                let dx = p2.x - p1.x;
                let dy = p2.y - p1.y;
                let len = (dx * dx + dy * dy).sqrt();
                let (ox, oy) = (nx * double_bond_offset * side, ny * double_bond_offset * side);
                // Shorten the inner line only at ends that carry other bonds, so it stays clear
                // of them; at a bare terminal end it lines up with the main line.
                let inset = 0.15 * len;
                let in1 = if inset_ends.0 { inset } else { 0.0 };
                let in2 = if inset_ends.1 { inset } else { 0.0 };
                let (ux, uy) = if len > 0.001 { (dx / len, dy / len) } else { (0.0, 0.0) };
                strokes.push(stroke(
                    Pos2::new(p1.x + ux * in1 + ox, p1.y + uy * in1 + oy),
                    Pos2::new(p2.x - ux * in2 + ox, p2.y - uy * in2 + oy),
                ));
            }
        },
        BondOrder::Triple => {
            let (nx, ny) = perp_normal(p1, p2);
            let off = double_bond_offset;
            strokes.push(stroke(p1, p2));
            strokes.push(stroke(
                Pos2::new(p1.x + nx * off, p1.y + ny * off),
                Pos2::new(p2.x + nx * off, p2.y + ny * off),
            ));
            strokes.push(stroke(
                Pos2::new(p1.x - nx * off, p1.y - ny * off),
                Pos2::new(p2.x - nx * off, p2.y - ny * off),
            ));
        }
        BondOrder::Quadruple => {
            // Four evenly spaced lines, symmetric about the axis.
            let (nx, ny) = perp_normal(p1, p2);
            for k in [-1.5_f32, -0.5, 0.5, 1.5] {
                let off = double_bond_offset * k;
                strokes.push(stroke(
                    Pos2::new(p1.x + nx * off, p1.y + ny * off),
                    Pos2::new(p2.x + nx * off, p2.y + ny * off),
                ));
            }
        }
    }
}

/// Solid filled wedge: narrow at p1 (begin), wide at p2 (end).
fn push_wedge_solid(fills: &mut Vec<Vec<Pos2>>, p1: Pos2, p2: Pos2, wedge_width: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    fills.push(vec![
        p1,
        Pos2::new(p2.x + nx * wedge_width, p2.y + ny * wedge_width),
        Pos2::new(p2.x - nx * wedge_width, p2.y - ny * wedge_width),
    ]);
}

/// Hash wedge: short lines widening from p1 to p2, over the (possibly trimmed) segment; each
/// hash's half-width comes from its fraction along the ORIGINAL `full` segment.
fn push_wedge_hash(
    strokes: &mut Vec<SceneStroke>,
    trimmed: (Pos2, Pos2),
    full: (Pos2, Pos2),
    wedge_width: f32,
    line_width: f32,
) {
    let (p1, p2) = trimmed;
    let (full1, full2) = full;
    let (nx, ny) = perp_normal(p1, p2);
    let full_len = (full2 - full1).length().max(1e-3);
    for i in 1..=HASH_COUNT {
        let t = i as f32 / HASH_COUNT as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        let full_t = ((Pos2::new(px, py) - full1).length() / full_len).min(1.0);
        let half = wedge_width * full_t;
        strokes.push(SceneStroke {
            a: Pos2::new(px + nx * half, py + ny * half),
            b: Pos2::new(px - nx * half, py - ny * half),
            width: line_width,
        });
    }
}

/// Plain hashed bond: crosswise ticks of one constant half-width, as many and as far apart as
/// the hashed wedge's (one more, since its first tick has no width at the narrow end).
fn push_hashed(strokes: &mut Vec<SceneStroke>, p1: Pos2, p2: Pos2, half_width: f32, line_width: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    for i in 0..=HASH_COUNT {
        let t = i as f32 / HASH_COUNT as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        strokes.push(SceneStroke {
            a: Pos2::new(px + nx * half_width, py + ny * half_width),
            b: Pos2::new(px - nx * half_width, py - ny * half_width),
            width: line_width,
        });
    }
}

/// Hollow wedge: the outline of the solid wedge's triangle, narrow at p1 (begin).
fn push_wedge_hollow(strokes: &mut Vec<SceneStroke>, p1: Pos2, p2: Pos2, wedge_width: f32, line_width: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    let left = Pos2::new(p2.x + nx * wedge_width, p2.y + ny * wedge_width);
    let right = Pos2::new(p2.x - nx * wedge_width, p2.y - ny * wedge_width);
    for (a, b) in [(p1, left), (left, right), (right, p1)] {
        strokes.push(SceneStroke { a, b, width: line_width });
    }
}

/// Dative bond: a line from p1 (begin) with a filled arrowhead at p2 (end).
fn push_dative(
    strokes: &mut Vec<SceneStroke>,
    fills: &mut Vec<Vec<Pos2>>,
    p1: Pos2,
    p2: Pos2,
    head_width: f32,
    line_width: f32,
) {
    let len = (p2 - p1).length();
    if len < 0.001 {
        return;
    }
    let dir = (p2 - p1) / len;
    let (nx, ny) = perp_normal(p1, p2);
    // The head is at most a third of the bond, so a short bond still shows its shaft.
    let head_len = (head_width * 2.5).min(len / 3.0);
    let base = p2 - dir * head_len;
    strokes.push(SceneStroke { a: p1, b: base, width: line_width });
    fills.push(vec![
        p2,
        Pos2::new(base.x + nx * head_width, base.y + ny * head_width),
        Pos2::new(base.x - nx * head_width, base.y - ny * head_width),
    ]);
}

/// Dashed line.
fn push_dashed(strokes: &mut Vec<SceneStroke>, p1: Pos2, p2: Pos2, width: f32, dash_len: f32, dash_gap: f32) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let total = (dx * dx + dy * dy).sqrt();
    if total < 0.001 {
        return;
    }
    let (ux, uy) = (dx / total, dy / total);
    let mut t = 0.0_f32;
    let mut drawing = true;
    while t < total {
        let seg = if drawing { dash_len } else { dash_gap };
        let end_t = (t + seg).min(total);
        if drawing {
            strokes.push(SceneStroke {
                a: Pos2::new(p1.x + ux * t, p1.y + uy * t),
                b: Pos2::new(p1.x + ux * end_t, p1.y + uy * end_t),
                width,
            });
        }
        t = end_t;
        drawing = !drawing;
    }
}

/// Wavy bond (sine-wave approximation).
fn push_wavy(strokes: &mut Vec<SceneStroke>, p1: Pos2, p2: Pos2, width: f32, wavy_amplitude: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    let mut pts: Vec<Pos2> = Vec::with_capacity(WAVY_STEPS + 1);
    for i in 0..=WAVY_STEPS {
        let t = i as f32 / WAVY_STEPS as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        let wave = (t * std::f32::consts::TAU * 2.0).sin() * wavy_amplitude;
        pts.push(Pos2::new(px + nx * wave, py + ny * wave));
    }
    for win in pts.windows(2) {
        strokes.push(SceneStroke { a: win[0], b: win[1], width });
    }
}

pub(crate) fn perp_normal(p1: Pos2, p2: Pos2) -> (f32, f32) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return (0.0, 1.0);
    }
    (-dy / len, dx / len)
}

/// Decide the line placement for a plain double bond from where its neighboring atoms sit
/// relative to the bond axis (ChemDraw convention: inset line toward the more-substituted side).
/// Space-agnostic — works in whatever space `p1/p2`, `normal`, `neighbors` share.
pub(crate) fn double_bond_placement(
    p1: Pos2,
    p2: Pos2,
    normal: (f32, f32),
    neighbors: &[Pos2],
) -> DoubleBondPlacement {
    let mid = Pos2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
    let eps = 1e-3_f32;
    let mut positive = 0;
    let mut negative = 0;
    for n in neighbors {
        let dot = (n.x - mid.x) * normal.0 + (n.y - mid.y) * normal.1;
        if dot > eps {
            positive += 1;
        } else if dot < -eps {
            negative += 1;
        }
    }
    match (positive, negative) {
        (p, n) if p > n => DoubleBondPlacement::Centered(1.0),
        (p, n) if n > p => DoubleBondPlacement::Centered(-1.0),
        _ => DoubleBondPlacement::Symmetric,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, BondStereo};
    use crate::widget::SCALE_FACTOR;

    /// A standalone egui font stack (no GPU), the same one the image exporter uses, so scene
    /// layout in a test matches what ships.
    fn fonts() -> egui::epaint::text::Fonts {
        egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            egui::FontDefinitions::default(),
        )
    }

    /// A small structure exercising a placed double bond, a stereo wedge and a labeled hetero-atom.
    fn sample() -> Molecule {
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let c3 = m.add_atom("C".to_string(), [1.5, 1.0], 0);
        let o = m.add_atom("O".to_string(), [-0.6, 0.6], 0);
        m.add_bond(c1, c2, BondOrder::Double);
        m.add_bond(c2, c3, BondOrder::Single);
        let w = m.add_bond(c1, o, BondOrder::Single);
        m.bond_by_id_mut(w).unwrap().stereo = BondStereo::WedgeUp;
        m
    }

    /// The whole point of `scene`: the on-screen editor and the image exporter get identical
    /// geometry. At the editor's zoom = 1 the two transforms differ only by their pan offset, so
    /// every bond stroke/fill and every label rect must coincide once that offset is removed.
    #[test]
    fn editor_and_export_geometry_match_up_to_offset() {
        let m = sample();
        let style = StyleConfig::default();
        let mut fonts = fonts();
        let mut view = fonts.with_pixels_per_point(1.0);

        let pan = Vec2::new(137.0, 42.0);
        let editor = RenderTransform { coord_scale: SCALE_FACTOR, offset: pan, visual_scale: 1.0 };
        let export = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 1.0 };

        let ed_labels = build_labels(&m, &style, editor, |j| view.layout_job(j));
        let ex_labels = build_labels(&m, &style, export, |j| view.layout_job(j));
        let ed_bonds = build_bonds(&m, &style, editor, &ed_labels);
        let ex_bonds = build_bonds(&m, &style, export, &ex_labels);

        assert_eq!(ed_bonds.len(), ex_bonds.len(), "same bonds drawn");
        let mut saw_double = false;
        let mut saw_wedge = false;
        for (a, b) in ed_bonds.iter().zip(&ex_bonds) {
            assert_eq!(a.strokes.len(), b.strokes.len());
            assert_eq!(a.fills.len(), b.fills.len());
            if a.strokes.len() == 2 {
                saw_double = true;
            }
            if !a.fills.is_empty() {
                saw_wedge = true;
            }
            for (sa, sb) in a.strokes.iter().zip(&b.strokes) {
                assert!((sa.width - sb.width).abs() < 1e-4, "stroke widths match");
                assert!(((sa.a - sb.a) - pan).length() < 1e-3, "stroke start differs only by pan");
                assert!(((sa.b - sb.b) - pan).length() < 1e-3, "stroke end differs only by pan");
            }
            for (fa, fb) in a.fills.iter().zip(&b.fills) {
                assert_eq!(fa.len(), fb.len());
                for (pa, pb) in fa.iter().zip(fb) {
                    assert!(((*pa - *pb) - pan).length() < 1e-3, "wedge vertex differs only by pan");
                }
            }
        }
        assert!(saw_double, "the C=C double bond yields two strokes");
        assert!(saw_wedge, "the C–O wedge yields a filled polygon");

        // The one labeled atom (O) lays out to the same rect once the pan is removed.
        let o_id = *ed_labels.keys().next().unwrap();
        let ed_rect = ed_labels[&o_id].rect;
        let ex_rect = ex_labels[&o_id].rect;
        assert!((ed_rect.min - ex_rect.min - pan).length() < 1e-3);
        assert!((ed_rect.size() - ex_rect.size()).length() < 1e-3);
    }

    /// A single mol-unit maps to `SCALE_FACTOR` pixels; visual sizes track `visual_scale`.
    #[test]
    fn double_bond_offset_scales_with_visual_scale() {
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        m.add_bond(a, b, BondOrder::Double); // isolated ⇒ symmetric pair
        let style = StyleConfig::default();
        let labels = HashMap::new();

        let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 2.0 };
        let bonds = build_bonds(&m, &style, xf, &labels);
        let strokes = &bonds[0].strokes;
        assert_eq!(strokes.len(), 2);
        // Symmetric lines sit ±(offset*visual_scale)/2 off the axis (y = 0).
        let expected = style.double_bond_offset * 2.0 * 0.5;
        assert!((strokes[0].a.y.abs() - expected).abs() < 1e-3);
        assert!((strokes[1].a.y.abs() - expected).abs() < 1e-3);
        assert!((strokes[0].width - style.bond_width * 2.0).abs() < 1e-4);
    }

    /// Vinyl (R–CH=CH₂): the inner line of the offset double bond is shortened only at the end
    /// that carries another bond; at the bare terminal CH₂ it reaches as far as the main line.
    #[test]
    fn double_bond_inner_line_reaches_bare_terminal_end() {
        let mut m = Molecule::default();
        let r = m.add_atom("C".to_string(), [-0.5, 0.87], 0);
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let t = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        m.add_bond(r, c, BondOrder::Single);
        m.add_bond(c, t, BondOrder::Double);
        let style = StyleConfig::default();
        let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 1.0 };
        let bonds = build_bonds(&m, &style, xf, &HashMap::new());
        let strokes = &bonds[1].strokes;
        assert_eq!(strokes.len(), 2, "main line + inner line");
        let (main, inner) = (&strokes[0], &strokes[1]);
        // Bond runs along +x from c (x=0) to t (x=SCALE_FACTOR).
        assert!((inner.b.x - main.b.x).abs() < 1e-3, "inner line ends flush at the terminal CH₂");
        assert!(inner.a.x > main.a.x + 1.0, "inner line is still inset at the substituted end");
    }

    /// `bounds` covers the atoms, the bonds and the label rects, follows the transform's offset,
    /// and is `None` for an empty molecule.
    #[test]
    fn scene_bounds_cover_labels_and_follow_offset() {
        let m = sample();
        let style = StyleConfig::default();
        let mut fonts = fonts();
        let mut view = fonts.with_pixels_per_point(1.0);
        let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 2.0 };
        let scene = MoleculeScene::build(&m, &style, xf, |j| view.layout_job(j));
        assert!((scene.label_bg_pad - style.label_bg_pad * 2.0).abs() < 1e-6);
        let b = scene.bounds(&m, xf).expect("non-empty");
        for a in &m.atoms {
            assert!(b.contains(xf.apply(a.pos)));
        }
        let label = scene.labels.values().next().expect("the O label").rect;
        assert!(b.contains_rect(label), "label rect {label:?} inside {b:?}");
        assert!(b.min.x < xf.apply(m.atoms[3].pos).x, "the O label extends past its atom");

        let pan = Vec2::new(30.0, -12.0);
        let moved = RenderTransform { offset: pan, ..xf };
        let moved_scene = MoleculeScene::build(&m, &style, moved, |j| view.layout_job(j));
        let mb = moved_scene.bounds(&m, moved).unwrap();
        assert!((mb.min - b.min - pan).length() < 1e-3 && (mb.max - b.max - pan).length() < 1e-3);

        let empty = Molecule::default();
        let es = MoleculeScene::build(&empty, &style, xf, |j| view.layout_job(j));
        assert!(es.bounds(&empty, xf).is_none());
    }

    #[test]
    fn ring_fill_polygon_follows_atoms() {
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [2.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [1.0, 2.0], 0);
        m.set_ring_fill(vec![a, b, c], [10, 20, 30, 200]);
        let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 1.0 };
        let polys = build_ring_fills(&m, xf);
        assert_eq!(polys.len(), 1);
        assert_eq!(polys[0].points.len(), 3);
        assert_eq!(polys[0].points[1], Pos2::new(2.0 * SCALE_FACTOR, 0.0));
        assert_eq!(polys[0].color, Color32::from_rgba_unmultiplied(10, 20, 30, 200));
    }

    /// The one bond of `begin → end` drawn with `order`/`stereo`/`position`, at scale 1 with
    /// no labels (so nothing is trimmed): its scene primitives.
    fn lone_bond(order: BondOrder, stereo: BondStereo, position: DoublePosition) -> SceneBond {
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let id = m.add_bond(a, b, order);
        let bond = m.bond_by_id_mut(id).unwrap();
        bond.stereo = stereo;
        bond.double_position = position;
        let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 1.0 };
        build_bonds(&m, &StyleConfig::default(), xf, &HashMap::new()).remove(0)
    }

    #[test]
    fn a_quadruple_bond_is_four_parallel_lines_symmetric_about_the_axis() {
        let bond = lone_bond(BondOrder::Quadruple, BondStereo::None, DoublePosition::Auto);
        assert_eq!(bond.strokes.len(), 4);
        let mut ys: Vec<f32> = bond.strokes.iter().map(|s| s.a.y).collect();
        ys.sort_by(f32::total_cmp);
        for s in &bond.strokes {
            assert!((s.a.y - s.b.y).abs() < 1e-4, "parallel to the (horizontal) bond");
        }
        assert!((ys[0] + ys[3]).abs() < 1e-4 && (ys[1] + ys[2]).abs() < 1e-4, "symmetric: {ys:?}");
        assert!(((ys[1] - ys[0]) - (ys[2] - ys[1])).abs() < 1e-4, "evenly spaced: {ys:?}");
    }

    #[test]
    fn a_hashed_bond_has_ticks_of_one_width_unlike_the_hashed_wedge() {
        let len = |s: &SceneStroke| (s.b - s.a).length();
        let hashed = lone_bond(BondOrder::Single, BondStereo::Hashed, DoublePosition::Auto);
        assert!(hashed.strokes.len() >= 3 && hashed.fills.is_empty());
        let first = len(&hashed.strokes[0]);
        assert!(first > 1.0);
        assert!(hashed.strokes.iter().all(|s| (len(s) - first).abs() < 1e-3));

        let wedge = lone_bond(BondOrder::Single, BondStereo::WedgeDown, DoublePosition::Auto);
        assert!(len(wedge.strokes.last().unwrap()) > len(&wedge.strokes[0]) + 1.0, "the wedge widens");
    }

    #[test]
    fn a_hollow_wedge_is_the_outline_of_the_solid_one() {
        let solid = lone_bond(BondOrder::Single, BondStereo::WedgeUp, DoublePosition::Auto);
        let hollow = lone_bond(BondOrder::Single, BondStereo::HollowWedge, DoublePosition::Auto);
        assert!(hollow.fills.is_empty());
        assert_eq!(hollow.strokes.len(), 3);
        // Each corner of the solid wedge's triangle is a corner of the outline.
        for corner in &solid.fills[0] {
            assert!(hollow.strokes.iter().any(|s| s.a == *corner || s.b == *corner), "{corner:?}");
        }
    }

    #[test]
    fn a_dative_bond_is_a_line_with_an_arrowhead_at_its_end() {
        let bond = lone_bond(BondOrder::Single, BondStereo::Dative, DoublePosition::Auto);
        let end = Pos2::new(SCALE_FACTOR, 0.0);
        assert_eq!(bond.strokes.len(), 1);
        assert_eq!(bond.strokes[0].a, Pos2::ZERO, "the shaft leaves the begin atom");
        assert_eq!(bond.fills.len(), 1);
        assert_eq!(bond.fills[0][0], end, "the arrow points at the end atom");
        assert!(bond.fills[0][1..].iter().all(|p| p.x < end.x && p.x > SCALE_FACTOR * 0.5));
    }

    /// A chosen double-bond side beats the automatic one, and `Left`/`Right` are as seen
    /// looking from the begin atom to the end atom on screen (y grows downward): for a bond
    /// drawn left to right, Right is below the axis.
    #[test]
    fn a_chosen_double_bond_position_places_the_inset_line_on_that_side() {
        let inset_y = |position| {
            let bond = lone_bond(BondOrder::Double, BondStereo::None, position);
            assert_eq!(bond.strokes.len(), 2);
            assert!(bond.strokes.iter().any(|s| s.a.y == 0.0 && s.b.y == 0.0), "{position:?} keeps the axis line");
            bond.strokes.iter().find(|s| s.a.y != 0.0).unwrap().a.y
        };
        assert!(inset_y(DoublePosition::Right) > 0.0);
        assert!(inset_y(DoublePosition::Left) < 0.0);

        let center = lone_bond(BondOrder::Double, BondStereo::None, DoublePosition::Center);
        assert!((center.strokes[0].a.y + center.strokes[1].a.y).abs() < 1e-4, "symmetric");
        assert!(center.strokes.iter().all(|s| s.a.y != 0.0));
    }

    #[test]
    fn a_chosen_side_wins_over_the_substituents() {
        // A neighbour above the axis would pull the inset line up; Right puts it below anyway.
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [1.5, -0.8], 0);
        let id = m.add_bond(a, b, BondOrder::Double);
        m.add_bond(b, c, BondOrder::Single);
        let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: Vec2::ZERO, visual_scale: 1.0 };
        let inset = |m: &Molecule| {
            let bonds = build_bonds(m, &StyleConfig::default(), xf, &HashMap::new());
            let double = bonds.into_iter().find(|sb| sb.bond_id == id).unwrap();
            double.strokes.iter().find(|s| s.a.y != 0.0).unwrap().a.y
        };
        assert!(inset(&m) < 0.0, "automatic: toward the substituent");
        m.bond_by_id_mut(id).unwrap().double_position = DoublePosition::Right;
        assert!(inset(&m) > 0.0, "chosen: Right");
    }
}
