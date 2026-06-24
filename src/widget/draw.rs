use super::ChemStructEditor;
use crate::molecule::{BondOrder, BondStereo};
use eframe::egui;

const BOND_WIDTH: f32 = 1.5;
const DOUBLE_BOND_OFFSET: f32 = 3.5;
pub const ATOM_LABEL_SIZE: f32 = 13.0;
const ATOM_BG_RADIUS: f32 = 9.0;
const WEDGE_HALF_WIDTH: f32 = 5.0;
const HASH_COUNT: usize = 6;
const DASH_LEN: f32 = 5.0;
const DASH_GAP: f32 = 4.0;
const WAVY_STEPS: usize = 16;
const WAVY_AMPLITUDE: f32 = 3.0;

// ─── Layer 0: bonds ──────────────────────────────────────────────────────────

pub fn draw_bonds(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    for bond in &editor.molecule.bonds {
        // Bonds to a folded terminal H are suppressed (the H is shown in the heavy atom's label).
        if is_folded_h(&editor.molecule, bond.begin) || is_folded_h(&editor.molecule, bond.end) {
            continue;
        }
        let Some(a) = editor.molecule.atom_by_id(bond.begin) else { continue };
        let Some(b) = editor.molecule.atom_by_id(bond.end) else { continue };
        let p1 = editor.mol_to_screen(a.pos, center);
        let p2 = editor.mol_to_screen(b.pos, center);

        let color = if editor.hovered_bond == Some(bond.id) {
            egui::Color32::from_rgb(50, 180, 50)
        } else {
            egui::Color32::BLACK
        };
        let stroke = egui::Stroke::new(BOND_WIDTH, color);

        match &bond.stereo {
            BondStereo::None => draw_bond_order(painter, p1, p2, &bond.order, stroke),
            BondStereo::WedgeUp => draw_wedge_solid(painter, p1, p2, color),
            BondStereo::WedgeDown => draw_wedge_hash(painter, p1, p2, color),
            BondStereo::Bold => draw_bold(painter, p1, p2, color),
            BondStereo::Dashed => draw_dashed(painter, p1, p2, stroke),
            BondStereo::Wavy => draw_wavy(painter, p1, p2, stroke),
        }
    }
}

fn draw_bond_order(
    painter: &egui::Painter,
    p1: egui::Pos2,
    p2: egui::Pos2,
    order: &BondOrder,
    stroke: egui::Stroke,
) {
    match order {
        BondOrder::Single => {
            painter.line_segment([p1, p2], stroke);
        }
        BondOrder::Double => {
            let (nx, ny) = perp_normal(p1, p2);
            let half = DOUBLE_BOND_OFFSET * 0.5;
            let a1 = egui::Pos2::new(p1.x + nx * half, p1.y + ny * half);
            let a2 = egui::Pos2::new(p2.x + nx * half, p2.y + ny * half);
            let b1 = egui::Pos2::new(p1.x - nx * half, p1.y - ny * half);
            let b2 = egui::Pos2::new(p2.x - nx * half, p2.y - ny * half);
            painter.line_segment([a1, a2], stroke);
            painter.line_segment([b1, b2], stroke);
        }
        BondOrder::Triple => {
            painter.line_segment([p1, p2], stroke);
            let (nx, ny) = perp_normal(p1, p2);
            let off = DOUBLE_BOND_OFFSET;
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
fn draw_wedge_solid(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32) {
    let (nx, ny) = perp_normal(p1, p2);
    let w_a = egui::Pos2::new(p2.x + nx * WEDGE_HALF_WIDTH, p2.y + ny * WEDGE_HALF_WIDTH);
    let w_b = egui::Pos2::new(p2.x - nx * WEDGE_HALF_WIDTH, p2.y - ny * WEDGE_HALF_WIDTH);
    painter.add(egui::Shape::convex_polygon(
        vec![p1, w_a, w_b],
        color,
        egui::Stroke::NONE,
    ));
}

/// Hash wedge: series of short lines widening from p1 to p2.
fn draw_wedge_hash(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32) {
    let (nx, ny) = perp_normal(p1, p2);
    for i in 1..=HASH_COUNT {
        let t = i as f32 / HASH_COUNT as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        let half = WEDGE_HALF_WIDTH * t;
        painter.line_segment([
            egui::Pos2::new(px + nx * half, py + ny * half),
            egui::Pos2::new(px - nx * half, py - ny * half),
        ], egui::Stroke::new(1.5, color));
    }
}

/// Heavy/bold bond.
fn draw_bold(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32) {
    painter.line_segment([p1, p2], egui::Stroke::new(4.0, color));
}

/// Dashed line.
fn draw_dashed(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, stroke: egui::Stroke) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let total = (dx * dx + dy * dy).sqrt();
    if total < 0.001 { return; }
    let (ux, uy) = (dx / total, dy / total);
    let mut t = 0.0_f32;
    let mut drawing = true;
    while t < total {
        let seg = if drawing { DASH_LEN } else { DASH_GAP };
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
fn draw_wavy(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, stroke: egui::Stroke) {
    let (nx, ny) = perp_normal(p1, p2);
    let mut pts: Vec<egui::Pos2> = Vec::with_capacity(WAVY_STEPS + 1);
    for i in 0..=WAVY_STEPS {
        let t = i as f32 / WAVY_STEPS as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        let wave = (t * std::f32::consts::TAU * 2.0).sin() * WAVY_AMPLITUDE;
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

// ─── Layer 1: atom background circles ────────────────────────────────────────

pub fn draw_atom_backgrounds(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
) {
    for atom in &editor.molecule.atoms {
        if should_show_label(editor, atom.id) {
            let sp = editor.mol_to_screen(atom.pos, center);
            painter.circle_filled(sp, ATOM_BG_RADIUS, egui::Color32::WHITE);
        }
    }
}

// ─── Layer 2: atom labels + selection/hover/hotspot rings ────────────────────

pub fn draw_atom_labels(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    for atom in &editor.molecule.atoms {
        let sp = editor.mol_to_screen(atom.pos, center);
        let is_hovered  = editor.hovered_atom  == Some(atom.id);
        let is_selected = editor.selected_atoms.contains(&atom.id);
        let is_hotspot  = editor.hotspot_atom  == Some(atom.id);

        // Hotspot ring (orange, outermost)
        if is_hotspot {
            painter.circle_stroke(
                sp,
                ATOM_BG_RADIUS + 9.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 140, 0)),
            );
        }
        // Selection ring (blue)
        if is_selected {
            painter.circle_stroke(
                sp,
                ATOM_BG_RADIUS + 5.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 130, 255)),
            );
        }
        // Hover ring (green)
        if is_hovered {
            painter.circle_stroke(
                sp,
                ATOM_BG_RADIUS + 2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 180, 50)),
            );
        }

        if should_show_label(editor, atom.id) {
            let label = atom_label_text(&editor.molecule, atom);
            painter.text(
                sp,
                egui::Align2::CENTER_CENTER,
                &label,
                egui::FontId::proportional(ATOM_LABEL_SIZE),
                egui::Color32::BLACK,
            );
        }
    }
}

// ─── Layer 3: overlays ───────────────────────────────────────────────────────

pub fn draw_overlays(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    // Bond preview line
    if let (Some(src_id), Some(end)) = (editor.bond_start, editor.preview_end_screen) {
        if let Some(src) = editor.molecule.atom_by_id(src_id) {
            let sp = editor.mol_to_screen(src.pos, center);
            painter.line_segment(
                [sp, end],
                egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(80, 140, 255, 180)),
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
            stroke: egui::epaint::PathStroke::new(1.5, egui::Color32::from_rgb(80, 130, 255)),
        }));
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// A terminal hydrogen bonded to a single heavy (non-H) atom. Such H atoms are
/// folded into the heavy atom's label (e.g. N–H renders as "NH2"), so they are
/// not drawn as separate vertices and their bond is suppressed. Deuterium ("D")
/// is treated as a normal atom, never folded.
pub fn is_folded_h(mol: &crate::molecule::Molecule, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    if atom.element != "H" { return false; }
    let bonds = mol.bonds_for_atom(atom_id);
    if bonds.len() != 1 { return false; }
    let b = bonds[0];
    let other = if b.begin == atom_id { b.end } else { b.begin };
    mol.atom_by_id(other).map_or(false, |n| n.element != "H")
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

/// Standard organic valence for implicit H calculation.
fn normal_valence(element: &str) -> Option<u8> {
    match element {
        "C"  => Some(4),
        "N"  => Some(3),
        "O"  => Some(2),
        "S"  => Some(2),
        "P"  => Some(3),
        "F" | "Cl" | "Br" | "I" => Some(1),
        "B"  => Some(3),
        "Si" => Some(4),
        "H"  => Some(1),
        _    => None,
    }
}

/// Total hydrogens to display on a heavy atom = explicit folded-H neighbors +
/// implicit H needed to satisfy valence. Computed as `valence + charge` minus the
/// bond-order sum to *non-folded* neighbors, which naturally includes both.
fn displayed_h_count(mol: &crate::molecule::Molecule, atom_id: u32) -> u8 {
    let Some(atom) = mol.atom_by_id(atom_id) else { return 0 };
    let Some(valence) = normal_valence(&atom.element) else { return 0 };
    let bond_sum_heavy: i16 = mol.bonds_for_atom(atom_id).iter()
        .filter(|b| {
            let other = if b.begin == atom_id { b.end } else { b.begin };
            !is_folded_h(mol, other)
        })
        .map(|b| match b.order {
            crate::molecule::BondOrder::Single => 1_i16,
            crate::molecule::BondOrder::Double => 2,
            crate::molecule::BondOrder::Triple => 3,
        })
        .sum();
    // Positive charge raises effective valence (e.g. N+ = 4), negative lowers it
    let effective = valence as i16 + atom.charge as i16;
    (effective - bond_sum_heavy).max(0) as u8
}

fn atom_label_text(mol: &crate::molecule::Molecule, atom: &crate::molecule::Atom) -> String {
    let mut s = atom.element.clone();
    let h = displayed_h_count(mol, atom.id);
    if h == 1 {
        s.push('H');
    } else if h > 1 {
        s.push('H');
        s.push_str(&h.to_string());
    }
    match atom.charge {
        0          => {}
        1          => s.push('+'),
        -1         => s.push('-'),
        c if c > 0 => s.push_str(&format!("{}+", c)),
        c          => s.push_str(&format!("{}", c)),
    }
    s
}
