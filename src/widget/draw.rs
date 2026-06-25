use super::ChemStructEditor;
use crate::molecule::{BondOrder, BondStereo};

// Visual sizes (bond width, label size, etc.) are user-configurable via `config.style`
// (see StyleConfig). These two only affect internal rendering detail and stay fixed.
const HASH_COUNT: usize = 6;
const WAVY_STEPS: usize = 16;

// ─── Layer 0: bonds ──────────────────────────────────────────────────────────

pub fn draw_bonds(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    let s = &editor.config.style;
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
        let stroke = egui::Stroke::new(s.bond_width, color);

        match &bond.stereo {
            BondStereo::None => draw_bond_order(painter, p1, p2, &bond.order, stroke, s.double_bond_offset),
            BondStereo::WedgeUp => draw_wedge_solid(painter, p1, p2, color, s.wedge_width),
            BondStereo::WedgeDown => draw_wedge_hash(painter, p1, p2, color, s.wedge_width, s.bond_width),
            BondStereo::Bold => draw_bold(painter, p1, p2, color, s.bold_width),
            BondStereo::Dashed => draw_dashed(painter, p1, p2, stroke, s.dash_len, s.dash_gap),
            BondStereo::Wavy => draw_wavy(painter, p1, p2, stroke, s.wavy_amplitude),
        }
    }
}

fn draw_bond_order(
    painter: &egui::Painter,
    p1: egui::Pos2,
    p2: egui::Pos2,
    order: &BondOrder,
    stroke: egui::Stroke,
    double_bond_offset: f32,
) {
    match order {
        BondOrder::Single => {
            painter.line_segment([p1, p2], stroke);
        }
        BondOrder::Double => {
            let (nx, ny) = perp_normal(p1, p2);
            let half = double_bond_offset * 0.5;
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

/// Hash wedge: series of short lines widening from p1 to p2.
fn draw_wedge_hash(painter: &egui::Painter, p1: egui::Pos2, p2: egui::Pos2, color: egui::Color32, wedge_width: f32, line_width: f32) {
    let (nx, ny) = perp_normal(p1, p2);
    for i in 1..=HASH_COUNT {
        let t = i as f32 / HASH_COUNT as f32;
        let px = p1.x + (p2.x - p1.x) * t;
        let py = p1.y + (p2.y - p1.y) * t;
        let half = wedge_width * t;
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

// ─── Layer 1: atom background circles ────────────────────────────────────────

pub fn draw_atom_backgrounds(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
) {
    let bg_radius = editor.config.style.atom_bg_radius;
    for atom in &editor.molecule.atoms {
        if should_show_label(editor, atom.id) {
            let sp = editor.mol_to_screen(atom.pos, center);
            painter.circle_filled(sp, bg_radius, egui::Color32::WHITE);
        }
    }
}

// ─── Layer 2: atom labels + selection/hover/hotspot rings ────────────────────

pub fn draw_atom_labels(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    let bg_radius = editor.config.style.atom_bg_radius;
    let label_size = editor.config.style.label_size;
    for atom in &editor.molecule.atoms {
        let sp = editor.mol_to_screen(atom.pos, center);
        let is_hovered  = editor.hovered_atom  == Some(atom.id);
        let is_selected = editor.selected_atoms.contains(&atom.id);
        let is_hotspot  = editor.hotspot_atom  == Some(atom.id);

        // Hotspot ring (orange, outermost)
        if is_hotspot {
            painter.circle_stroke(
                sp,
                bg_radius + 9.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 140, 0)),
            );
        }
        // Selection ring (blue)
        if is_selected {
            painter.circle_stroke(
                sp,
                bg_radius + 5.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 130, 255)),
            );
        }
        // Hover ring (green)
        if is_hovered {
            painter.circle_stroke(
                sp,
                bg_radius + 2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 180, 50)),
            );
        }
        // Over-valence warning (red dashed) — applies to all atoms, even unlabeled carbons.
        if is_valence_invalid(&editor.molecule, atom.id) {
            draw_dashed_circle(painter, sp, bg_radius + 7.0, egui::Color32::from_rgb(220, 30, 30));
        }

        if should_show_label(editor, atom.id) {
            let job = atom_label_job(&editor.molecule, atom, label_size, egui::Color32::BLACK);
            let galley = painter.layout_job(job);
            let pos = sp - galley.size() * 0.5; // center the laid-out label on the atom
            painter.galley(pos, galley, egui::Color32::BLACK);
        }
    }
}

/// Build a laid-out atom label with the hydrogen count rendered as a subscript
/// (e.g. CH₃, NH₂). The element symbol, the "H", and the charge are at the base
/// size; only the multi-H count digit is smaller and bottom-aligned.
fn atom_label_job(
    mol: &crate::molecule::Molecule,
    atom: &crate::molecule::Atom,
    base_size: f32,
    color: egui::Color32,
) -> egui::text::LayoutJob {
    use egui::text::{LayoutJob, TextFormat};
    let mut job = LayoutJob::default();
    let normal = TextFormat {
        font_id: egui::FontId::proportional(base_size),
        color,
        ..Default::default()
    };
    let subscript = TextFormat {
        font_id: egui::FontId::proportional(base_size * 0.7),
        color,
        valign: egui::Align::BOTTOM,
        ..Default::default()
    };

    job.append(&atom.element, 0.0, normal.clone());
    let h = displayed_h_count(mol, atom.id);
    if h >= 1 {
        job.append("H", 0.0, normal.clone());
        if h > 1 {
            job.append(&h.to_string(), 0.0, subscript);
        }
    }
    match atom.charge {
        0 => {}
        1 => job.append("+", 0.0, normal),
        -1 => job.append("-", 0.0, normal),
        c if c > 0 => job.append(&format!("{c}+"), 0.0, normal),
        c => job.append(&format!("{c}"), 0.0, normal),
    }
    job
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

/// True when an atom is over-bonded: its total bond order exceeds what its element/charge
/// allows (e.g. a carbon with more than 4 bond-orders, an oxygen with more than 2). Such
/// atoms are marked with a red dashed warning ring. Under-bonded atoms are valid — the
/// missing valence is shown as implicit H — and elements with no standard valence aren't checked.
pub fn is_valence_invalid(mol: &crate::molecule::Molecule, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    let Some(valence) = normal_valence(&atom.element) else { return false };
    let bond_sum: i16 = mol.bonds_for_atom(atom_id).iter()
        .map(|b| match b.order {
            crate::molecule::BondOrder::Single => 1,
            crate::molecule::BondOrder::Double => 2,
            crate::molecule::BondOrder::Triple => 3,
        })
        .sum();
    bond_sum > valence as i16 + atom.charge as i16
}

/// Draw a dashed circle (used as the over-valence warning ring).
fn draw_dashed_circle(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32) {
    const SEGMENTS: usize = 20;
    let stroke = egui::Stroke::new(1.5, color);
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

/// Plain-text label (no subscript) — the rendering path uses `atom_label_job`; this is the
/// readable oracle used by tests.
#[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, Molecule};

    fn label(mol: &Molecule, id: u32) -> String {
        atom_label_text(mol, mol.atom_by_id(id).unwrap())
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
    }
}
