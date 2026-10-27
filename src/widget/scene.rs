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
use crate::molecule::{BondOrder, BondStereo, Molecule};

use super::draw::{atom_label, folded_h, should_show_label};
use super::rich_text::{RichGalley, RichTextStyle};

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
        let labels = build_labels(mol, style, xf, layout);
        let bonds = build_bonds(mol, style, xf, &labels);
        let ring_fills = build_ring_fills(mol, xf);
        MoleculeScene { ring_fills, bonds, labels, label_bg_pad: style.label_bg_pad * xf.visual_scale }
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
                let placement = if matches!(bond.order, BondOrder::Double) {
                    let (nx, ny) = perp_normal(p1, p2);
                    let mut neighbors: Vec<Pos2> = Vec::new();
                    for nid in index.neighbor_atom_ids(bond.begin) {
                        if nid == bond.end {
                            continue;
                        }
                        if let Some(na) = index.atom_by_id(nid) {
                            neighbors.push(xf.apply(na.pos));
                        }
                    }
                    for nid in index.neighbor_atom_ids(bond.end) {
                        if nid == bond.begin {
                            continue;
                        }
                        if let Some(na) = index.atom_by_id(nid) {
                            neighbors.push(xf.apply(na.pos));
                        }
                    }
                    double_bond_placement(p1, p2, (nx, ny), &neighbors)
                } else {
                    DoubleBondPlacement::Symmetric
                };
                push_bond_order(&mut strokes, tp1, tp2, &bond.order, width, style.double_bond_offset * vs, placement);
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
                let inset = 0.15 * len;
                let (ux, uy) = if len > 0.001 { (dx / len, dy / len) } else { (0.0, 0.0) };
                strokes.push(stroke(
                    Pos2::new(p1.x + ux * inset + ox, p1.y + uy * inset + oy),
                    Pos2::new(p2.x - ux * inset + ox, p2.y - uy * inset + oy),
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
}
