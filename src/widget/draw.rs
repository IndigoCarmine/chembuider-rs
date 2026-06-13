use super::ChemStructEditor;
use crate::molecule::BondOrder;
use eframe::egui;

const BOND_WIDTH: f32 = 1.5;
const DOUBLE_BOND_OFFSET: f32 = 3.5;
const ATOM_LABEL_SIZE: f32 = 13.0;
const ATOM_BG_RADIUS: f32 = 9.0;

// Layer 0: draw all bonds
pub fn draw_bonds(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    for bond in &editor.molecule.bonds {
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
        draw_bond_lines(painter, p1, p2, &bond.order, stroke);
    }
}

fn draw_bond_lines(
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
            let a1 = egui::Pos2::new(p1.x + nx * off, p1.y + ny * off);
            let a2 = egui::Pos2::new(p2.x + nx * off, p2.y + ny * off);
            let b1 = egui::Pos2::new(p1.x - nx * off, p1.y - ny * off);
            let b2 = egui::Pos2::new(p2.x - nx * off, p2.y - ny * off);
            painter.line_segment([a1, a2], stroke);
            painter.line_segment([b1, b2], stroke);
        }
    }
}

fn perp_normal(p1: egui::Pos2, p2: egui::Pos2) -> (f32, f32) {
    let dx = p2.x - p1.x;
    let dy = p2.y - p1.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return (0.0, 1.0);
    }
    (-dy / len, dx / len)
}

// Layer 1: white background circles to occlude bond lines at atom junctions
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

// Layer 2: atom element labels with hover/selection rings
pub fn draw_atom_labels(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    for atom in &editor.molecule.atoms {
        let sp = editor.mol_to_screen(atom.pos, center);
        let is_hovered = editor.hovered_atom == Some(atom.id);
        let is_selected = editor.selected_atoms.contains(&atom.id);

        if is_selected {
            painter.circle_stroke(
                sp,
                ATOM_BG_RADIUS + 5.0,
                egui::Stroke::new(2.0, egui::Color32::from_rgb(80, 130, 255)),
            );
        }
        if is_hovered {
            painter.circle_stroke(
                sp,
                ATOM_BG_RADIUS + 2.0,
                egui::Stroke::new(1.5, egui::Color32::from_rgb(50, 180, 50)),
            );
        }

        if should_show_label(editor, atom.id) {
            let label = atom_label_text(atom);
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

// Layer 3: overlays (bond preview, lasso)
pub fn draw_overlays(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    if let (Some(src_id), Some(end)) = (editor.bond_start, editor.preview_end_screen) {
        if let Some(src) = editor.molecule.atom_by_id(src_id) {
            let sp = editor.mol_to_screen(src.pos, center);
            painter.line_segment(
                [sp, end],
                egui::Stroke::new(
                    1.5,
                    egui::Color32::from_rgba_unmultiplied(80, 140, 255, 180),
                ),
            );
        }
    }

    if editor.lasso_path.len() > 1 {
        let pts: Vec<egui::Pos2> = editor.lasso_path.clone();
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: pts,
            closed: false,
            fill: egui::Color32::from_rgba_unmultiplied(100, 150, 255, 30),
            stroke: egui::epaint::PathStroke::new(
                1.5,
                egui::Color32::from_rgb(80, 130, 255),
            ),
        }));
    }
}

fn should_show_label(editor: &ChemStructEditor, atom_id: u32) -> bool {
    let Some(atom) = editor.molecule.atom_by_id(atom_id) else {
        return false;
    };
    if atom.element != "C" {
        return true;
    }
    if atom.charge != 0 {
        return true;
    }
    let degree = editor.molecule.bonds_for_atom(atom_id).len();
    degree <= 1
}

fn atom_label_text(atom: &crate::molecule::Atom) -> String {
    let mut s = atom.element.clone();
    match atom.charge {
        0 => {}
        1 => s.push('+'),
        -1 => s.push('-'),
        c if c > 0 => s.push_str(&format!("{}+", c)),
        c => s.push_str(&format!("{}", c)),
    }
    s
}
