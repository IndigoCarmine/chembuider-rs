use super::ChemStructEditor;
use super::rich_text::{RichGalley, RichText, RichTextStyle};
use crate::molecule::{max_valence, normal_valence, BondOrder, BondStereo};
use std::collections::HashMap;

// Visual sizes (bond width, label size, etc.) are user-configurable via `config.style`
// (see StyleConfig). These two only affect internal rendering detail and stay fixed.
const HASH_COUNT: usize = 6;
const WAVY_STEPS: usize = 16;

// ─── Per-frame label layout pass ─────────────────────────────────────────────

/// A laid-out atom label: the galley plus the screen rect it occupies. The rect is
/// positioned so the label's *element symbol* (not the bounding box) is centered on the
/// atom position — wide labels and scripts grow outward from the anchor.
pub struct AtomLabelLayout {
    pub galley: RichGalley,
    pub rect: egui::Rect,
}

/// Lay out every visible atom label once per frame. The map is shared by the bond pass
/// (endpoint trimming), the background pass (white mask rects) and the label pass
/// (painting), so the baseline math runs once. Galley construction itself is cheap:
/// egui caches galleys by layout-job hash across frames.
pub fn compute_label_layouts(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
) -> HashMap<u32, AtomLabelLayout> {
    let style = RichTextStyle::new(
        editor.config.style.label_size * editor.zoom,
        egui::Color32::BLACK,
    );
    let mut map = HashMap::new();
    for atom in &editor.molecule.atoms {
        if !should_show_label(editor, atom.id) {
            continue;
        }
        let label = atom_label(&editor.molecule, atom);
        let galley = label.rich.layout_with(&style, |job| painter.layout_job(job));
        let anchor = galley
            .char_range_rect(label.anchor_chars)
            .map(|r| r.center().to_vec2())
            .unwrap_or_else(|| galley.size() * 0.5); // no glyphs: fall back to bbox center
        let sp = editor.mol_to_screen(atom.pos, center);
        let rect = egui::Rect::from_min_size(sp - anchor, galley.size());
        map.insert(atom.id, AtomLabelLayout { galley, rect });
    }
    map
}

// ─── Layer 0: bonds ──────────────────────────────────────────────────────────

pub fn draw_bonds(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
    labels: &HashMap<u32, AtomLabelLayout>,
) {
    let s = &editor.config.style;
    // Visual sizes scale with zoom so they track the panzoom transform (z == 1.0 = baseline).
    let z = editor.zoom;
    for bond in &editor.molecule.bonds {
        // Bonds to a folded terminal H are suppressed (the H is shown in the heavy atom's label).
        if is_folded_h(&editor.molecule, bond.begin) || is_folded_h(&editor.molecule, bond.end) {
            continue;
        }
        let Some(a) = editor.molecule.atom_by_id(bond.begin) else { continue };
        let Some(b) = editor.molecule.atom_by_id(bond.end) else { continue };
        let p1 = editor.mol_to_screen(a.pos, center);
        let p2 = editor.mol_to_screen(b.pos, center);

        // Trim each endpoint at its label's (padded) rect so bonds stop short of the
        // text instead of striking through it. A bond fully swallowed by its labels
        // (very short bond) is skipped — the labels still show the connectivity.
        let trim_rect = |id: u32| labels.get(&id).map(|l| l.rect.expand(s.label_trim_pad * z));
        let Some((tp1, tp2)) =
            trimmed_bond_endpoints(p1, p2, trim_rect(bond.begin), trim_rect(bond.end))
        else {
            continue;
        };

        let color = if editor.hovered_bond == Some(bond.id) {
            egui::Color32::from_rgb(50, 180, 50)
        } else {
            egui::Color32::BLACK
        };
        let stroke = egui::Stroke::new(s.bond_width * z, color);

        match &bond.stereo {
            BondStereo::None => {
                // For a plain (non-stereo) double bond, decide where the two lines go based on
                // which side of the bond axis the neighboring bonds sit (ChemDraw convention):
                // more neighbors on one side ⇒ main line on the axis + a short inset line toward
                // the majority side; equal counts (or none) ⇒ symmetric pair (legacy behavior).
                // The side decision is pure geometry classification and uses the UNTRIMMED
                // atom positions; only the drawn lines use the trimmed endpoints.
                let placement = if matches!(bond.order, BondOrder::Double) {
                    let (nx, ny) = perp_normal(p1, p2);
                    // Collect screen positions of the OTHER neighbors of each endpoint.
                    let mut neighbors: Vec<egui::Pos2> = Vec::new();
                    for nid in editor.molecule.neighbor_atom_ids(bond.begin) {
                        if nid == bond.end { continue; }
                        if let Some(na) = editor.molecule.atom_by_id(nid) {
                            neighbors.push(editor.mol_to_screen(na.pos, center));
                        }
                    }
                    for nid in editor.molecule.neighbor_atom_ids(bond.end) {
                        if nid == bond.begin { continue; }
                        if let Some(na) = editor.molecule.atom_by_id(nid) {
                            neighbors.push(editor.mol_to_screen(na.pos, center));
                        }
                    }
                    double_bond_placement(p1, p2, (nx, ny), &neighbors)
                } else {
                    DoubleBondPlacement::Symmetric
                };
                draw_bond_order(painter, tp1, tp2, &bond.order, stroke, s.double_bond_offset * z, placement)
            }
            BondStereo::WedgeUp => draw_wedge_solid(painter, tp1, tp2, color, s.wedge_width * z),
            BondStereo::WedgeDown => {
                // Hash half-widths keep the taper rate of the untrimmed wedge: each hash's
                // width is its fraction along the ORIGINAL p1→p2 segment.
                draw_wedge_hash(painter, (tp1, tp2), (p1, p2), color, s.wedge_width * z, s.bond_width * z)
            }
            BondStereo::Bold => draw_bold(painter, tp1, tp2, color, s.bold_width * z),
            BondStereo::Dashed => draw_dashed(painter, tp1, tp2, stroke, s.dash_len * z, s.dash_gap * z),
            BondStereo::Wavy => draw_wavy(painter, tp1, tp2, stroke, s.wavy_amplitude * z),
        }
    }
}

/// Parametric `t` in `(0, 1]` where the segment `p → q` exits `rect` (slab method), for a
/// `p` lying inside the rect. `None` when `q` is also inside (the segment never leaves).
fn segment_rect_exit(p: egui::Pos2, q: egui::Pos2, rect: egui::Rect) -> Option<f32> {
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

/// Trim a bond's endpoints at the (already padded) label rects of its endpoint atoms.
/// `None` rect = unlabeled endpoint (bare carbon), no trim at that end. Returns `None`
/// when the labels swallow the whole bond (degenerate short bond → skip drawing).
fn trimmed_bond_endpoints(
    p1: egui::Pos2,
    p2: egui::Pos2,
    r1: Option<egui::Rect>,
    r2: Option<egui::Rect>,
) -> Option<(egui::Pos2, egui::Pos2)> {
    // A labeled endpoint whose rect the segment never exits swallows the whole bond.
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
    // Require ~1px of remaining bond, else consider it swallowed.
    if len < 1e-3 || (t2 - t1) * len < 1.0 {
        return None;
    }
    let at = |t: f32| p1 + (p2 - p1) * t;
    Some((at(t1), at(t2)))
}

/// Where to place the two lines of a `BondOrder::Double` (with `BondStereo::None`).
/// Decided from the side on which the bond's substituents lie (see `double_bond_placement`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum DoubleBondPlacement {
    /// Two lines symmetric about the bond axis (terminal/isolated bonds or trans-disubstituted).
    Symmetric,
    /// Main line on the axis + a shorter inset line offset toward one side. The `f32` is the
    /// side sign (+1.0 = +normal side, -1.0 = -normal side).
    Centered(f32),
}

fn draw_bond_order(
    painter: &egui::Painter,
    p1: egui::Pos2,
    p2: egui::Pos2,
    order: &BondOrder,
    stroke: egui::Stroke,
    double_bond_offset: f32,
    placement: DoubleBondPlacement,
) {
    match order {
        BondOrder::Single => {
            painter.line_segment([p1, p2], stroke);
        }
        BondOrder::Double => match placement {
            DoubleBondPlacement::Symmetric => {
                let (nx, ny) = perp_normal(p1, p2);
                let half = double_bond_offset * 0.5;
                let a1 = egui::Pos2::new(p1.x + nx * half, p1.y + ny * half);
                let a2 = egui::Pos2::new(p2.x + nx * half, p2.y + ny * half);
                let b1 = egui::Pos2::new(p1.x - nx * half, p1.y - ny * half);
                let b2 = egui::Pos2::new(p2.x - nx * half, p2.y - ny * half);
                painter.line_segment([a1, a2], stroke);
                painter.line_segment([b1, b2], stroke);
            }
            DoubleBondPlacement::Centered(side) => {
                // Main line runs full-length on the bond axis.
                painter.line_segment([p1, p2], stroke);
                // Second line: full offset toward the substituent side, inset 15% at each end so
                // it sits inside the ring/cis region.
                let (nx, ny) = perp_normal(p1, p2);
                let dx = p2.x - p1.x;
                let dy = p2.y - p1.y;
                let len = (dx * dx + dy * dy).sqrt();
                let (ox, oy) = (nx * double_bond_offset * side, ny * double_bond_offset * side);
                let inset = 0.15 * len;
                let (ux, uy) = if len > 0.001 { (dx / len, dy / len) } else { (0.0, 0.0) };
                let inner_start = egui::Pos2::new(p1.x + ux * inset + ox, p1.y + uy * inset + oy);
                let inner_end   = egui::Pos2::new(p2.x - ux * inset + ox, p2.y - uy * inset + oy);
                painter.line_segment([inner_start, inner_end], stroke);
            }
        },
        BondOrder::Triple => {
            painter.line_segment([p1, p2], stroke);
            let (nx, ny) = perp_normal(p1, p2);
            let off = double_bond_offset;
            painter.line_segment([
                egui::Pos2::new(p1.x + nx * off, p1.y + ny * off),
                egui::Pos2::new(p2.x + nx * off, p2.y + ny * off),
            ], stroke);
            painter.line_segment([
                egui::Pos2::new(p1.x - nx * off, p1.y - ny * off),
                egui::Pos2::new(p2.x - nx * off, p2.y - ny * off),
            ], stroke);
        }
    }
}

/// Solid filled wedge: narrow at p1 (begin), wide at p2 (end).
fn draw_wedge_solid(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32, wedge_width: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    let w_a = egui::Pos2::new(p2.x + nx * wedge_width, p2.y + ny * wedge_width);
    let w_b = egui::Pos2::new(p2.x - nx * wedge_width, p2.y - ny * wedge_width);
    painter.add(egui::Shape::convex_polygon(
        vec![p1, w_a, w_b],
        color,
        egui::Stroke::NONE,
    ));
}

/// Hash wedge: series of short lines widening from p1 to p2. The hashes are drawn over
/// the (possibly trimmed) `trimmed` segment, but each hash's half-width comes from its
/// fraction along the ORIGINAL `full` segment so the taper rate matches the untrimmed wedge.
fn draw_wedge_hash(
    painter: &egui::Painter,
    trimmed: (egui::Pos2, egui::Pos2),
    full: (egui::Pos2, egui::Pos2),
    color: egui::Color32,
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
        let full_t = ((egui::Pos2::new(px, py) - full1).length() / full_len).min(1.0);
        let half = wedge_width * full_t;
        painter.line_segment([
            egui::Pos2::new(px + nx * half, py + ny * half),
            egui::Pos2::new(px - nx * half, py - ny * half),
        ], egui::Stroke::new(line_width, color));
    }
}

/// Heavy/bold bond.
fn draw_bold(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32, bold_width: f32) {
    painter.line_segment([p1, p2], egui::Stroke::new(bold_width, color));
}

/// Dashed line.
fn draw_dashed(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, stroke: egui::Stroke, dash_len: f32, dash_gap: f32) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let total = (dx * dx + dy * dy).sqrt();
    if total < 0.001 { return; }
    let (ux, uy) = (dx / total, dy / total);
    let mut t = 0.0_f32;
    let mut drawing = true;
    while t < total {
        let seg = if drawing { dash_len } else { dash_gap };
        let end_t = (t + seg).min(total);
        if drawing {
            painter.line_segment([
                egui::Pos2::new(p1.x + ux * t,     p1.y + uy * t),
                egui::Pos2::new(p1.x + ux * end_t, p1.y + uy * end_t),
            ], stroke);
        }
        t = end_t;
        drawing = !drawing;
    }
}

/// Wavy bond (sine-wave approximation).
fn draw_wavy(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, stroke: egui::Stroke, wavy_amplitude: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    let mut pts: Vec<egui::Pos2> = Vec::with_capacity(WAVY_STEPS + 1);
    for i in 0..=WAVY_STEPS {
        let t = i as f32 / WAVY_STEPS as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        let wave = (t * std::f32::consts::TAU * 2.0).sin() * wavy_amplitude;
        pts.push(egui::Pos2::new(px + nx * wave, py + ny * wave));
    }
    for win in pts.windows(2) {
        painter.line_segment([win[0], win[1]], stroke);
    }
}

fn perp_normal(p1: egui::Pos2, p2: egui::Pos2) -> (f32, f32) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 { return (0.0, 1.0); }
    (-dy / len, dx / len)
}

/// Decide the line placement for a plain double bond from where its neighboring atoms sit
/// relative to the bond axis. `normal` is the perpendicular unit normal of the bond (see
/// `perp_normal`); `neighbors` are the screen positions of the other atoms bonded to either
/// endpoint. The signed component `(neighbor - midpoint) · normal` classifies each neighbor as
/// being on the +normal side (`dot > eps`) or -normal side (`dot < -eps`).
///
/// The inset line goes toward the side with MORE neighbors (into the ring / the
/// substituted side, ChemDraw convention):
///
/// - more neighbors on the +normal side → `Centered(+1.0)` (inset line toward +normal)
/// - more neighbors on the -normal side → `Centered(-1.0)` (inset line toward -normal)
/// - equal counts (both sides alike, or none) → `Symmetric`
///
/// Coordinate space: this function is space-agnostic — it works in whatever space the caller
/// supplies, as long as `p1/p2`, `normal`, and `neighbors` are all in the same space. The
/// rendering path uses screen coordinates (where y is flipped vs molecule coords); the
/// symmetric-vs-Centered decision is sign-consistent either way, only the meaning of the
/// returned side sign is tied to the chosen `normal`.
fn double_bond_placement(
    p1: egui::Pos2,
    p2: egui::Pos2,
    normal: (f32, f32),
    neighbors: &[egui::Pos2],
) -> DoubleBondPlacement {
    let mid = egui::Pos2::new((p1.x + p2.x) * 0.5, (p1.y + p2.y) * 0.5);
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

// ─── Layer 1: atom label backgrounds ─────────────────────────────────────────

/// White rectangles behind labels: incident bonds are already trimmed at the label rect,
/// so this only masks NON-incident bonds that happen to cross behind the text.
pub fn draw_atom_backgrounds(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    _center: egui::Pos2,
    labels: &HashMap<u32, AtomLabelLayout>,
) {
    let pad = editor.config.style.label_bg_pad * editor.zoom;
    for layout in labels.values() {
        painter.rect_filled(layout.rect.expand(pad), 0.0, egui::Color32::WHITE);
    }
}

// ─── Layer 2: atom labels + selection/hover/hotspot rings ────────────────────

pub fn draw_atom_labels(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
    labels: &HashMap<u32, AtomLabelLayout>,
) {
    // Visual sizes scale with zoom so they track the panzoom transform (z == 1.0 = baseline).
    let z = editor.zoom;
    let bg = editor.config.style.atom_bg_radius * z;
    for atom in &editor.molecule.atoms {
        // A folded hydrogen is drawn nowhere, so a ring around it is a marker hovering in
        // empty space — which is exactly how a lasso-selected NH2 used to look.
        if is_folded_h(&editor.molecule, atom.id) {
            continue;
        }
        let sp = editor.mol_to_screen(atom.pos, center);
        let is_hovered  = editor.hovered_atom  == Some(atom.id);
        let is_selected = editor.selected_atoms.contains(&atom.id);
        let is_hotspot  = editor.hotspot_atom  == Some(atom.id);

        // Hotspot ring (orange, outermost)
        if is_hotspot {
            painter.circle_stroke(
                sp,
                bg + 9.0 * z,
                egui::Stroke::new(2.0 * z, egui::Color32::from_rgb(255, 140, 0)),
            );
        }
        // Selection ring (blue)
        if is_selected {
            painter.circle_stroke(
                sp,
                bg + 5.0 * z,
                egui::Stroke::new(2.0 * z, egui::Color32::from_rgb(80, 130, 255)),
            );
        }
        // Hover ring (green)
        if is_hovered {
            painter.circle_stroke(
                sp,
                bg + 2.0 * z,
                egui::Stroke::new(1.5 * z, egui::Color32::from_rgb(50, 180, 50)),
            );
        }
        // Over-valence warning (red dashed) — applies to all atoms, even unlabeled carbons.
        if is_valence_invalid(&editor.molecule, atom.id) {
            draw_dashed_circle(painter, sp, bg + 7.0 * z, egui::Color32::from_rgb(220, 30, 30), 1.5 * z);
        }

        if let Some(layout) = labels.get(&atom.id) {
            // Pre-laid-out with the element symbol anchored on the atom position.
            layout.galley.paint(painter, layout.rect.min);
        }
    }
}

/// Which side of the element symbol the displayed hydrogens go on.
/// `Right` is the normal reading order ("OH", "NH2"); `Left` mirrors it ("HO", "H2N")
/// so the hydrogens point into free space when the bonds come from the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HSide {
    Right,
    Left,
}

/// Averages the unit directions from the atom toward its (non-folded) neighbors: when
/// they point right (x-sum above a small deadzone) the free side is the left, so the
/// hydrogens flip there. Near-vertical sums inside the deadzone and unbonded atoms keep
/// the default `Right`. (Vertical H placement — H above/below for straight-up bonds, as
/// ChemDraw does in crowded rings — is a possible future refinement.)
fn h_side(mol: &crate::molecule::Molecule, atom_id: u32) -> HSide {
    const H_FLIP_DEADZONE: f32 = 0.1;
    let Some(atom) = mol.atom_by_id(atom_id) else { return HSide::Right };
    let mut sum_x = 0.0_f32;
    for nid in mol.neighbor_atom_ids(atom_id) {
        if is_folded_h(mol, nid) {
            continue;
        }
        let Some(n) = mol.atom_by_id(nid) else { continue };
        let dx = n.pos[0] - atom.pos[0];
        let dy = n.pos[1] - atom.pos[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1e-6 {
            sum_x += dx / len;
        }
    }
    if sum_x > H_FLIP_DEADZONE { HSide::Left } else { HSide::Right }
}

/// A styled atom label plus the char range (over the flattened text) of the element
/// symbol — the part that gets centered on the atom position.
pub struct AtomLabel {
    pub rich: RichText,
    pub anchor_chars: std::ops::Range<usize>,
}

/// Length in chars of the leading element token of a parsed label's flat text: the first
/// `[A-Z][a-z]*` run ("CO2Et" → "C", "Cl" → "Cl", "OTf" → "O"), falling back to a single
/// char for labels that start otherwise (e.g. italic "tBu").
fn element_token_len(flat: &str) -> usize {
    let mut chars = flat.chars();
    let Some(first) = chars.next() else { return 0 };
    if !first.is_ascii_uppercase() {
        return 1;
    }
    1 + chars.take_while(|c| c.is_ascii_lowercase()).count()
}

/// Build the styled atom label: hydrogen count as a true subscript (CH₃, NH₂), the charge
/// as a trailing superscript (N⁺, SO₄²⁻), and the hydrogens flipped to the atom's free
/// side ("HO", "H₂N" when the bonds come from the right). The element text itself goes
/// through the rich-text markup parser, so custom labels typed into the editor can use
/// `_`/`^` scripts, `**bold**` and `//italic//` (e.g. `CO_2Et`, `//t//Bu`).
fn atom_label(
    mol: &crate::molecule::Molecule,
    atom: &crate::molecule::Atom,
) -> AtomLabel {
    let element = RichText::parse(&atom.element);
    let element_flat: String = element.spans.iter().map(|s| s.text.as_str()).collect();
    let h = displayed_h_count(mol, atom.id);
    let side = if h >= 1 { h_side(mol, atom.id) } else { HSide::Right };

    let mut rich = RichText::new();
    let mut anchor_start = 0usize;
    if side == HSide::Left {
        // Hydrogens first: "H", then the count subscript ("H₂N").
        rich = rich.text("H");
        anchor_start = 1;
        if h > 1 {
            let count = h.to_string();
            anchor_start += count.chars().count();
            rich = rich.subscript(count);
        }
    }
    for span in element.spans {
        rich = rich.span(span.text, span.style);
    }
    if side == HSide::Right && h >= 1 {
        rich = rich.text("H");
        if h > 1 {
            rich = rich.subscript(h.to_string());
        }
    }
    // The charge stays a trailing superscript in both orders (top-right, ChemDraw-style;
    // the strict-ChemDraw leading variant "⁺H₃N" is a possible future refinement).
    match atom.charge {
        0 => {}
        1 => rich = rich.superscript("+"),
        -1 => rich = rich.superscript("-"),
        c if c > 0 => rich = rich.superscript(format!("{c}+")),
        c => rich = rich.superscript(format!("{}-", -c)),
    }

    // Anchor on the element token; degenerate labels fall back to the first char.
    let token_len = element_token_len(&element_flat);
    let (start, len) = if token_len == 0 { (0, 1) } else { (anchor_start, token_len) };
    AtomLabel { rich, anchor_chars: start..start + len }
}

// ─── Layer 3: overlays ───────────────────────────────────────────────────────

pub fn draw_overlays(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    // Bond preview line
    if let (Some(src_id), Some(end)) = (editor.bond_start, editor.preview_end_screen) {
        if let Some(src) = editor.molecule.atom_by_id(src_id) {
            let sp = editor.mol_to_screen(src.pos, center);
            painter.line_segment(
                [sp, end],
                egui::Stroke::new(1.5 * editor.zoom, egui::Color32::from_rgba_unmultiplied(80, 140, 255, 180)),
            );
        }
    }

    // Lasso
    if editor.lasso_path.len() > 1 {
        let pts: Vec<egui::Pos2> = editor.lasso_path.clone();
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: pts,
            // Must be closed: epaint panics when filling an open path.
            closed: true,
            fill: egui::Color32::from_rgba_unmultiplied(100, 150, 255, 30),
            stroke: egui::epaint::PathStroke::new(1.5 * editor.zoom, egui::Color32::from_rgb(80, 130, 255)),
        }));
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// A hydrogen that is drawn inside its heavy neighbour's label instead of on the canvas,
/// so it gets no vertex of its own and its bond is suppressed (N–H renders as "NH2").
///
/// Three conditions, and the last two are what keep the folding honest:
///
/// - Exactly one bond, to a heavy (non-H) atom. Deuterium ("D") is a normal atom, never folded.
/// - It came in with a group ([`Atom::group_h`](crate::molecule::Atom::group_h)) — `n` inserts
///   NH₂, whose hydrogen is really just the "2" in the label. A hydrogen the user asked for by
///   name is an atom like any other; folding it made it vanish outright.
/// - Its neighbour's label still has room for it. [`displayed_h_count`] derives the label's
///   hydrogens from the *heavy* bond sum, so once those fill the valence the label shows no
///   hydrogen at all and there is nothing left to fold into. An invisible hydrogen was the
///   sole cause of the over-valence ring on both nitrogens of a Ph–N=N–Ph drawn from two `n`
///   keys: a warning the user could neither see the reason for nor delete.
pub fn is_folded_h(mol: &crate::molecule::Molecule, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    if atom.element != "H" || !atom.group_h { return false; }
    let bonds = mol.bonds_for_atom(atom_id);
    if bonds.len() != 1 { return false; }
    let b = bonds[0];
    let other = if b.begin == atom_id { b.end } else { b.begin };
    let Some(heavy) = mol.atom_by_id(other) else { return false };
    if heavy.element == "H" { return false; }
    let Some(valence) = normal_valence(&heavy.element) else { return false };

    // Room left on the neighbour: its valence, minus every bond that is not a group hydrogen,
    // minus the group hydrogens folded ahead of this one. Ordering those by id makes the
    // choice stable when a group brings more hydrogens than the atom can show (SiH₃ on a
    // silicon that has since grown a second bond).
    let used: i16 = mol.bonds_for_atom(other).iter().map(|nb| {
        let n = if nb.begin == other { nb.end } else { nb.begin };
        match mol.atom_by_id(n) {
            Some(a) if a.element == "H" && a.group_h => (n < atom_id) as i16,
            _ => nb.order.valence(),
        }
    }).sum();
    used < valence as i16 + heavy.charge as i16
}

fn should_show_label(editor: &ChemStructEditor, atom_id: u32) -> bool {
    let Some(atom) = editor.molecule.atom_by_id(atom_id) else { return false };
    // Folded hydrogens are merged into their heavy neighbor's label.
    if is_folded_h(&editor.molecule, atom_id) { return false; }
    if atom.element == "C" {
        // Carbon: only show when completely isolated (no bonds) so the atom stays visible
        return atom.charge != 0 || editor.molecule.bonds_for_atom(atom_id).is_empty();
    }
    true
}

/// True when an atom is over-bonded: its total bond order exceeds what its element/charge
/// allows (e.g. a carbon with more than 4 bond-orders, an oxygen with more than 2). Such
/// atoms are marked with a red dashed warning ring. Under-bonded atoms are valid — the
/// missing valence is shown as implicit H — and elements with no standard valence aren't checked.
///
/// The warning threshold is the element's *highest* common valence (`max_valence`), so
/// hypervalent-but-ordinary S and P (sulfones, sulfates, phosphates) are not flagged,
/// while implicit-H counting keeps using the lowest (`normal_valence`).
pub fn is_valence_invalid(mol: &crate::molecule::Molecule, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    let Some(valence) = max_valence(&atom.element) else { return false };
    mol.bond_order_sum(atom_id) > valence as i16 + atom.charge as i16
}

/// Draw a dashed circle (used as the over-valence warning ring).
fn draw_dashed_circle(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32, stroke_width: f32) {
    const SEGMENTS: usize = 20;
    let stroke = egui::Stroke::new(stroke_width, color);
    for i in (0..SEGMENTS).step_by(2) {
        let a0 = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        painter.line_segment(
            [
                egui::pos2(center.x + radius * a0.cos(), center.y + radius * a0.sin()),
                egui::pos2(center.x + radius * a1.cos(), center.y + radius * a1.sin()),
            ],
            stroke,
        );
    }
}

/// Total hydrogens to display on a heavy atom = explicit folded-H neighbors +
/// implicit H needed to satisfy valence. Computed as `valence + charge` minus the
/// bond-order sum to *non-folded* neighbors, which naturally includes both.
pub(crate) fn displayed_h_count(mol: &crate::molecule::Molecule, atom_id: u32) -> u8 {
    let Some(atom) = mol.atom_by_id(atom_id) else { return 0 };
    let Some(valence) = normal_valence(&atom.element) else { return 0 };
    // A lone atom (no bonds yet) shows just its symbol — placing "O"/"N"/"S" at the cursor
    // should read as O/N/S, not OH2/NH3/SH2. Implicit H appears once it gets a bond.
    if mol.bonds_for_atom(atom_id).is_empty() {
        return 0;
    }
    let bond_sum_heavy: i16 = mol.bonds_for_atom(atom_id).iter()
        .filter(|b| {
            let other = if b.begin == atom_id { b.end } else { b.begin };
            !is_folded_h(mol, other)
        })
        .map(|b| b.order.valence())
        .sum();
    // Positive charge raises effective valence (e.g. N+ = 4), negative lowers it
    let effective = valence as i16 + atom.charge as i16;
    (effective - bond_sum_heavy).max(0) as u8
}

/// Plain-text label (scripts flattened) — the rendering path uses `atom_label`; this is
/// the readable oracle used by tests. Kept in sync by `label_oracle_matches_rich_label`.
#[cfg(test)]
fn atom_label_text(mol: &crate::molecule::Molecule, atom: &crate::molecule::Atom) -> String {
    let h = displayed_h_count(mol, atom.id);
    let side = if h >= 1 { h_side(mol, atom.id) } else { HSide::Right };
    let h_part = match h {
        0 => String::new(),
        1 => "H".to_string(),
        n => format!("H{n}"),
    };
    let mut s = match side {
        HSide::Right => format!("{}{}", atom.element, h_part),
        HSide::Left => format!("{}{}", h_part, atom.element),
    };
    match atom.charge {
        0          => {}
        1          => s.push('+'),
        -1         => s.push('-'),
        c if c > 0 => s.push_str(&format!("{}+", c)),
        c          => s.push_str(&format!("{}-", -c)),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, Molecule};

    fn label(mol: &Molecule, id: u32) -> String {
        atom_label_text(mol, mol.atom_by_id(id).unwrap())
    }

    /// The real NH₂ group, from the same embedded fragment set the editor uses — so these
    /// tests see the geometry that actually ships rather than a hand-written stand-in.
    fn nh2() -> crate::molecule::fragment::Fragment {
        crate::Config::embedded()
            .resolve_fragment("NH2")
            .expect("NH2 is a built-in fragment")
    }

    /// A lone (unbonded) atom shows just its symbol — no implicit H (so placing O/N/S reads
    /// as O/N/S, not OH2/NH3/SH2).
    #[test]
    fn lone_atoms_show_bare_symbol() {
        let mut m = Molecule::default();
        for el in ["O", "N", "S", "Br", "C", "P", "F"] {
            let id = m.add_atom(el.to_string(), [0.0, 0.0], 0);
            assert_eq!(label(&m, id), el, "lone {el} should render as '{el}'");
        }
    }

    /// Once bonded, valence is filled with implicit H (hydroxyl/amine/thiol, etc.).
    #[test]
    fn bonded_heteroatoms_fill_valence_with_h() {
        for (el, want) in [("O", "OH"), ("N", "NH2"), ("S", "SH"), ("Br", "Br"), ("P", "PH2")] {
            let mut m = Molecule::default();
            let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
            let x = m.add_atom(el.to_string(), [1.0, 0.0], 0);
            m.add_bond(c, x, BondOrder::Single);
            assert_eq!(label(&m, x), want, "C-{el} should render as '{want}'");
        }
        // Carbonyl: a double bond consumes two valences → no H.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = m.add_atom("O".to_string(), [1.0, 0.0], 0);
        m.add_bond(c, o, BondOrder::Double);
        assert_eq!(label(&m, o), "O", "C=O oxygen should render as 'O'");
    }

    #[test]
    fn over_valence_is_flagged() {
        // Carbon with 5 single bonds → invalid; 4 → valid; 3 → valid (CH).
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let mut ns = Vec::new();
        for i in 0..5 {
            let n = m.add_atom("H".to_string(), [i as f32 + 1.0, 0.0], 0);
            m.add_bond(c, n, BondOrder::Single);
            ns.push(n);
        }
        assert!(is_valence_invalid(&m, c), "pentavalent carbon must be flagged");
        m.remove_atom(ns[4]);
        assert!(!is_valence_invalid(&m, c), "tetravalent carbon is valid");
        m.remove_atom(ns[3]);
        assert!(!is_valence_invalid(&m, c), "under-valence carbon is valid (implicit H)");

        // Oxygen with a triple bond → over-valence.
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = m.add_atom("O".to_string(), [1.0, 0.0], 0);
        m.add_bond(a, o, BondOrder::Triple);
        assert!(is_valence_invalid(&m, o), "O with a triple bond is over-valence");

        // Hypervalent-but-ordinary sulfur: a sulfonamide S (two S=O, S-N, S-C = 6) is
        // valid; seven bond-orders tips it over.
        let mut m = Molecule::default();
        let s = m.add_atom("S".to_string(), [0.0, 0.0], 0);
        let o1 = m.add_atom("O".to_string(), [0.0, 1.0], 0);
        let o2 = m.add_atom("O".to_string(), [0.0, -1.0], 0);
        let n = m.add_atom("N".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [-1.0, 0.0], 0);
        m.add_bond(s, o1, BondOrder::Double);
        m.add_bond(s, o2, BondOrder::Double);
        m.add_bond(s, n, BondOrder::Single);
        m.add_bond(s, c, BondOrder::Single);
        assert!(!is_valence_invalid(&m, s), "sulfonamide S(VI) must not be flagged");
        let x = m.add_atom("C".to_string(), [1.0, 1.0], 0);
        m.add_bond(s, x, BondOrder::Single);
        assert!(is_valence_invalid(&m, s), "seven bond-orders on S is over-valence");
        // But H-filling still uses the low valence: S with one bond shows SH, not SH5.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let s = m.add_atom("S".to_string(), [1.0, 0.0], 0);
        m.add_bond(c, s, BondOrder::Single);
        assert_eq!(label(&m, s), "SH");
    }

    /// The rich label carries the same characters as the plain-text oracle, with the H
    /// count as a subscript span and the charge as a superscript span.
    #[test]
    fn label_oracle_matches_rich_label() {
        use crate::widget::rich_text::Script;
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 1); // NH3+ once bonded (bond from the left)
        let o = m.add_atom("O".to_string(), [3.0, 0.0], -2); // O 2- (over-reduced, but exercises multi-charge)
        m.add_bond(c, n, BondOrder::Single);

        for id in [n, o] {
            let atom = m.atom_by_id(id).unwrap();
            let rich = atom_label(&m, atom).rich;
            let flat: String = rich.spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(flat, atom_label_text(&m, atom));
        }

        // NH3+ (bond from the left keeps reading order): 3 subscript, + superscript.
        let rich = atom_label(&m, m.atom_by_id(n).unwrap()).rich;
        let scripts: Vec<(String, Script)> = rich.spans.iter()
            .map(|s| (s.text.clone(), s.style.script)).collect();
        assert_eq!(scripts, vec![
            ("NH".to_string(), Script::Normal),
            ("3".to_string(), Script::Sub),
            ("+".to_string(), Script::Super),
        ]);

        // O with charge -2 renders the chemistry-style "2-" superscript.
        let rich = atom_label(&m, m.atom_by_id(o).unwrap()).rich;
        assert_eq!(rich.spans.last().map(|s| (s.text.as_str(), s.style.script)),
                   Some(("2-", Script::Super)));
    }

    /// Only a *group's* hydrogen folds into the label. One the user drew is an ordinary
    /// atom, and folding it made it disappear completely — no vertex, no bond, no label.
    #[test]
    fn only_a_groups_hydrogen_folds_into_the_label() {
        // Drawn on purpose: its own vertex, and it counts against the neighbour's valence
        // like any other bond (C–H with three implicit hydrogens is still methane).
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let h = m.add_atom("H".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, h, BondOrder::Single);
        assert!(!is_folded_h(&m, h), "a hydrogen placed by hand must stay visible");
        assert_eq!(label(&m, h), "H");

        // Arrived inside an NH2 group: label decoration, folded into "NH2".
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, n, BondOrder::Single);
        let group = m.insert_fragment(&nh2(), n, 0.0, false);
        assert!(is_folded_h(&m, group[0]), "a group's hydrogen belongs in its label");
        assert_eq!(label(&m, n), "NH2");
    }

    /// A group hydrogen stays folded only while the label can still show it: once the heavy
    /// bonds fill the valence there is no implicit hydrogen left to hide inside, so it is
    /// drawn — which is what keeps the over-valence warning pointing at something visible.
    #[test]
    fn a_group_hydrogen_unfolds_when_the_label_has_no_room() {
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, n, BondOrder::Single);
        let h = m.insert_fragment(&nh2(), n, 0.0, false)[0];
        assert!(is_folded_h(&m, h), "one heavy bond leaves the label two hydrogens to show");

        // A second heavy neighbour, double-bonded: N now has three bond-orders of its own.
        let n2 = m.add_atom("N".to_string(), [1.5, 1.5], 0);
        m.add_bond(n, n2, BondOrder::Double);
        assert!(!is_folded_h(&m, h), "with no implicit hydrogen left, the group's must be drawn");
        assert_eq!(label(&m, n), "N", "the label shows no hydrogen it is not holding");
        assert!(is_valence_invalid(&m, n), "four bond-orders on N is over-valence, visibly so");
    }

    /// Hydrogens flip to the free side: a terminal O whose bond comes from the RIGHT
    /// renders "HO"; from the LEFT renders "OH". Anchor chars always cover the element.
    #[test]
    fn h_side_flips_terminal_labels() {
        use crate::widget::rich_text::Script;

        // C at the right of O → bond direction +x from O's perspective → flip: "HO".
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        m.add_bond(o, c, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Left);
        let label = atom_label(&m, m.atom_by_id(o).unwrap());
        let flat: String = label.rich.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(flat, "HO");
        assert_eq!(flat, atom_label_text(&m, m.atom_by_id(o).unwrap()));
        assert_eq!(label.anchor_chars, 1..2, "anchor must be the O, after the H");

        // C at the left of O → default order "OH", anchored on the leading O.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = m.add_atom("O".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, o, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Right);
        let label = atom_label(&m, m.atom_by_id(o).unwrap());
        let flat: String = label.rich.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(flat, "OH");
        assert_eq!(label.anchor_chars, 0..1);

        // Flipped charged amine: H first, count subscript right after the H, charge last.
        let mut m = Molecule::default();
        let n = m.add_atom("N".to_string(), [0.0, 0.0], 1);
        let c = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        m.add_bond(n, c, BondOrder::Single);
        let label = atom_label(&m, m.atom_by_id(n).unwrap());
        let scripts: Vec<(String, Script)> = label.rich.spans.iter()
            .map(|s| (s.text.clone(), s.style.script)).collect();
        assert_eq!(scripts, vec![
            ("H".to_string(), Script::Normal),
            ("3".to_string(), Script::Sub),
            ("N".to_string(), Script::Normal),
            ("+".to_string(), Script::Super),
        ]);
        assert_eq!(label.anchor_chars, 2..3, "anchor is the N after H and the count");

        // Near-vertical bond stays in the deadzone → no flip.
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [0.0, 1.5], 0);
        m.add_bond(o, c, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Right);

        // Two neighbors straddling the atom cancel out → no flip.
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let a = m.add_atom("C".to_string(), [-1.5, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        m.add_bond(a, o, BondOrder::Single);
        m.add_bond(o, b, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Right);

        // Custom label with an element token: anchor covers the whole leading token.
        let mut m = Molecule::default();
        let x = m.add_atom("Cl".to_string(), [0.0, 0.0], 0);
        let label = atom_label(&m, m.atom_by_id(x).unwrap());
        assert_eq!(label.anchor_chars, 0..2);
        let x2 = m.add_atom("CO2Et".to_string(), [1.0, 0.0], 0);
        let label = atom_label(&m, m.atom_by_id(x2).unwrap());
        assert_eq!(label.anchor_chars, 0..1, "CO2Et anchors on the C only");
    }

    #[test]
    fn segment_rect_exit_finds_the_leaving_point() {
        let rect = egui::Rect::from_min_max(egui::pos2(-10.0, -5.0), egui::pos2(10.0, 5.0));
        let c = egui::pos2(0.0, 0.0);

        // Exit right: t where x reaches 10 on the way to (40, 0).
        let t = segment_rect_exit(c, egui::pos2(40.0, 0.0), rect).unwrap();
        assert!((t - 0.25).abs() < 1e-5);
        // Exit top (y-min side).
        let t = segment_rect_exit(c, egui::pos2(0.0, -20.0), rect).unwrap();
        assert!((t - 0.25).abs() < 1e-5);
        // Diagonal: the y slab (5/20) is hit before the x slab (10/20).
        let t = segment_rect_exit(c, egui::pos2(20.0, 20.0), rect).unwrap();
        assert!((t - 0.25).abs() < 1e-5);
        // Axis-parallel with zero x movement.
        let t = segment_rect_exit(c, egui::pos2(0.0, 50.0), rect).unwrap();
        assert!((t - 0.1).abs() < 1e-5);
        // Never exits: destination inside the rect.
        assert!(segment_rect_exit(c, egui::pos2(5.0, 0.0), rect).is_none());
    }

    #[test]
    fn trimmed_bond_endpoints_clip_at_labels() {
        let p1 = egui::pos2(0.0, 0.0);
        let p2 = egui::pos2(100.0, 0.0);
        let r1 = egui::Rect::from_center_size(p1, egui::vec2(20.0, 10.0));
        let r2 = egui::Rect::from_center_size(p2, egui::vec2(40.0, 10.0));

        // Both ends labeled: clipped at each rect's edge.
        let (a, b) = trimmed_bond_endpoints(p1, p2, Some(r1), Some(r2)).unwrap();
        assert!((a.x - 10.0).abs() < 1e-4 && a.y == 0.0);
        assert!((b.x - 80.0).abs() < 1e-4 && b.y == 0.0);

        // Only one end labeled: the other end stays put.
        let (a, b) = trimmed_bond_endpoints(p1, p2, Some(r1), None).unwrap();
        assert!((a.x - 10.0).abs() < 1e-4);
        assert_eq!(b, p2);

        // Unlabeled: identity.
        assert_eq!(trimmed_bond_endpoints(p1, p2, None, None), Some((p1, p2)));

        // Overlapping label rects swallow the bond entirely.
        let big1 = egui::Rect::from_center_size(p1, egui::vec2(130.0, 10.0));
        let big2 = egui::Rect::from_center_size(p2, egui::vec2(130.0, 10.0));
        assert_eq!(trimmed_bond_endpoints(p1, p2, Some(big1), Some(big2)), None);

        // Whole bond inside one label rect → swallowed too.
        let huge = egui::Rect::from_center_size(p1, egui::vec2(300.0, 30.0));
        assert_eq!(trimmed_bond_endpoints(p1, p2, Some(huge), None), None);
    }

    /// End-to-end: run one headless editor frame (real egui context, real fonts) and check
    /// that a charged amine paints its label as separate galleys — the base "NH", the
    /// subscript count, and the superscript charge — through the full `ui()` paint path.
    #[test]
    fn editor_frame_paints_rich_labels() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let c = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = editor.molecule.add_atom("N".to_string(), [1.5, 0.0], 1);
        editor.molecule.add_bond(c, n, BondOrder::Single);

        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            editor.ui(ui);
        });

        // Collect the text of every painted galley (recursing into shape groups).
        fn collect_text(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect_text(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        for expect in ["NH", "3", "+"] {
            assert!(
                texts.iter().any(|t| t == expect),
                "expected a '{expect}' galley among painted text {texts:?}"
            );
        }
    }

    /// Anchoring: the element glyph (not the bounding box) is centered on the atom, so a
    /// flipped "HO" extends left of the atom and the O sits exactly on it. Runs the real
    /// layout path (fonts + galleys) inside a headless frame.
    #[test]
    fn labels_anchor_the_element_glyph_on_the_atom() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let o = editor.molecule.add_atom("O".to_string(), [0.0, 0.0], 0);
        let c = editor.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        editor.molecule.add_bond(o, c, BondOrder::Single); // bond from the right → "HO"

        let center = egui::pos2(200.0, 150.0);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let layouts = compute_label_layouts(&editor, ui.painter(), center);
            let layout = &layouts[&o];
            let sp = editor.mol_to_screen([0.0, 0.0], center);
            assert!(layout.rect.contains(sp), "atom must sit inside its label rect");

            // The anchor char (the O of "HO") is centered on the atom position.
            let label = atom_label(&editor.molecule, editor.molecule.atom_by_id(o).unwrap());
            let flat: String = label.rich.spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(flat, "HO");
            let a = layout.galley.char_range_rect(label.anchor_chars).unwrap();
            let anchor_screen = layout.rect.min + a.center().to_vec2();
            assert!((anchor_screen.x - sp.x).abs() < 0.75, "{anchor_screen:?} vs {sp:?}");
            assert!((anchor_screen.y - sp.y).abs() < 0.75, "{anchor_screen:?} vs {sp:?}");

            // "HO" extends to the left of the anchored O.
            assert!(layout.rect.center().x < sp.x);
        });
    }

    /// Bond lines stop at the label boundary instead of striking through the text.
    #[test]
    fn editor_frame_trims_bonds_at_labels() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let c = editor.molecule.add_atom("C".to_string(), [-1.5, 0.0], 0);
        let o = editor.molecule.add_atom("O".to_string(), [0.0, 0.0], 0);
        editor.molecule.add_bond(c, o, BondOrder::Single);

        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            editor.ui(ui);
        });

        let label_rect = *editor.last_label_rects.get(&o).expect("O label rect recorded");
        fn collect_lines(shape: &egui::Shape, out: &mut Vec<[egui::Pos2; 2]>) {
            match shape {
                egui::Shape::LineSegment { points, .. } => out.push(*points),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect_lines(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut lines = Vec::new();
        for clipped in &output.shapes {
            collect_lines(&clipped.shape, &mut lines);
        }
        // Exactly one bond line, and neither endpoint reaches into the label rect.
        assert!(!lines.is_empty(), "the C-O bond must be painted");
        for [a, b] in &lines {
            assert!(
                !label_rect.contains(*a) && !label_rect.contains(*b),
                "bond endpoint inside label rect: {a:?}/{b:?} in {label_rect:?}"
            );
        }
    }

    /// Wide labels are clickable over their whole text via last-frame label rects.
    #[test]
    fn hit_test_covers_the_label_rect() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let a = editor.molecule.add_atom("CO2Et".to_string(), [0.0, 0.0], 0);

        let center = egui::pos2(200.0, 150.0);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let layouts = compute_label_layouts(&editor, ui.painter(), center);
            editor.last_label_rects =
                layouts.iter().map(|(id, l)| (*id, l.rect)).collect();
        });

        let rect = editor.last_label_rects[&a];
        assert!(rect.width() > 30.0, "CO2Et should lay out wide, got {rect:?}");
        let sp = editor.mol_to_screen([0.0, 0.0], center);
        let far_point = egui::pos2(rect.max.x - 2.0, rect.center().y);
        assert!(sp.distance(far_point) > super::super::NODE_HIT_RADIUS_PX,
            "test point must be outside the base hit circle");
        assert_eq!(editor.hit_test_atom(far_point, center), Some(a));
        // Outside both circle and rect: no hit.
        assert_eq!(editor.hit_test_atom(egui::pos2(rect.max.x + 30.0, rect.max.y + 30.0), center), None);
    }

    /// Side-decision for double-bond line placement. The helper is space-agnostic, so the test
    /// works in plain (molecule-like) coordinates: bond along the x axis with normal (0, 1).
    #[test]
    fn double_bond_placement_picks_substituent_side() {
        let p1 = egui::Pos2::new(0.0, 0.0);
        let p2 = egui::Pos2::new(2.0, 0.0);
        let normal = perp_normal(p1, p2); // (0, 1) for a horizontal bond

        // Ring-like / cis: both neighbors above the axis (same, +normal side) → Centered(+1).
        let cis = [egui::Pos2::new(0.0, 1.0), egui::Pos2::new(2.0, 1.0)];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &cis),
            DoubleBondPlacement::Centered(1.0),
        );

        // Cis on the other side → Centered(-1).
        let cis_neg = [egui::Pos2::new(0.0, -1.0), egui::Pos2::new(2.0, -1.0)];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &cis_neg),
            DoubleBondPlacement::Centered(-1.0),
        );

        // Trans: one neighbor on each side → Symmetric.
        let trans = [egui::Pos2::new(0.0, 1.0), egui::Pos2::new(2.0, -1.0)];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &trans),
            DoubleBondPlacement::Symmetric,
        );

        // Asymmetric: two neighbors above, one below → inset line toward the majority.
        let asym = [
            egui::Pos2::new(0.0, 1.0),
            egui::Pos2::new(2.0, 1.0),
            egui::Pos2::new(2.0, -1.0),
        ];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &asym),
            DoubleBondPlacement::Centered(1.0),
        );
        let asym_neg = [
            egui::Pos2::new(0.0, -1.0),
            egui::Pos2::new(2.0, -1.0),
            egui::Pos2::new(0.0, 1.0),
        ];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &asym_neg),
            DoubleBondPlacement::Centered(-1.0),
        );

        // Terminal / isolated: no neighbors → Symmetric.
        assert_eq!(
            double_bond_placement(p1, p2, normal, &[]),
            DoubleBondPlacement::Symmetric,
        );
    }
}
