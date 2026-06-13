pub mod draw;
pub mod interact;

use crate::molecule::{BondOrder, Molecule};
use eframe::egui;
use std::collections::HashSet;

pub const DEFAULT_BOND_LENGTH: f32 = 1.5;
pub const SCALE_FACTOR: f32 = 50.0;
pub const NODE_HIT_RADIUS_PX: f32 = 15.0;
pub const BOND_HIT_THRESHOLD_PX: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    Select,
    Bond,
    Eraser,
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tool::Select => write!(f, "Select"),
            Tool::Bond => write!(f, "Bond"),
            Tool::Eraser => write!(f, "Eraser"),
        }
    }
}

pub struct ChemStructEditor {
    pub molecule: Molecule,
    pub tool: Tool,
    pub current_element: String,
    pub current_bond_order: BondOrder,
    pub zoom: f32,
    pub pan: egui::Vec2,

    pub selected_atoms: HashSet<u32>,

    pub dragging_atom: Option<u32>,
    pub drag_start_mol: Option<[f32; 2]>,

    pub bond_start: Option<u32>,
    pub preview_end_screen: Option<egui::Pos2>,

    pub hovered_atom: Option<u32>,
    pub hovered_bond: Option<u32>,

    pub lasso_path: Vec<egui::Pos2>,

    // Track drag origin to distinguish click vs drag
    pub drag_origin_screen: Option<egui::Pos2>,
}

impl Default for ChemStructEditor {
    fn default() -> Self {
        Self {
            molecule: Molecule::default(),
            tool: Tool::Bond,
            current_element: "C".to_string(),
            current_bond_order: BondOrder::Single,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            selected_atoms: HashSet::new(),
            dragging_atom: None,
            drag_start_mol: None,
            bond_start: None,
            preview_end_screen: None,
            hovered_atom: None,
            hovered_bond: None,
            lasso_path: Vec::new(),
            drag_origin_screen: None,
        }
    }
}

impl ChemStructEditor {
    pub fn mol_to_screen(&self, pos: [f32; 2], center: egui::Pos2) -> egui::Pos2 {
        egui::Pos2 {
            x: center.x + pos[0] * self.zoom * SCALE_FACTOR + self.pan.x,
            y: center.y + pos[1] * self.zoom * SCALE_FACTOR + self.pan.y,
        }
    }

    pub fn screen_to_mol(&self, screen: egui::Pos2, center: egui::Pos2) -> [f32; 2] {
        let inv = 1.0 / (self.zoom * SCALE_FACTOR);
        [
            (screen.x - center.x - self.pan.x) * inv,
            (screen.y - center.y - self.pan.y) * inv,
        ]
    }

    pub fn hit_test_atom(&self, screen: egui::Pos2, center: egui::Pos2) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for atom in &self.molecule.atoms {
            let sp = self.mol_to_screen(atom.pos, center);
            let d = sp.distance(screen);
            if d <= NODE_HIT_RADIUS_PX {
                if best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((atom.id, d));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    pub fn hit_test_bond(&self, screen: egui::Pos2, center: egui::Pos2) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for bond in &self.molecule.bonds {
            let Some(a) = self.molecule.atom_by_id(bond.begin) else { continue };
            let Some(b) = self.molecule.atom_by_id(bond.end) else { continue };
            let p1 = self.mol_to_screen(a.pos, center);
            let p2 = self.mol_to_screen(b.pos, center);
            let d = point_to_segment_dist(screen, p1, p2);
            if d <= BOND_HIT_THRESHOLD_PX {
                if best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((bond.id, d));
                }
            }
        }
        best.map(|(id, _)| id)
    }

    pub fn bond_angles_from(&self, atom_id: u32) -> Vec<f32> {
        let Some(src) = self.molecule.atom_by_id(atom_id) else {
            return vec![];
        };
        self.molecule
            .neighbor_atom_ids(atom_id)
            .iter()
            .filter_map(|&nid| {
                let n = self.molecule.atom_by_id(nid)?;
                let dx = n.pos[0] - src.pos[0];
                let dy = n.pos[1] - src.pos[1];
                Some(dy.atan2(dx))
            })
            .collect()
    }

    pub fn best_new_bond_angle(&self, atom_id: u32) -> f32 {
        let mut angles = self.bond_angles_from(atom_id);

        if angles.is_empty() {
            return std::f32::consts::PI / 3.0;
        }

        if angles.len() == 1 {
            let existing = angles[0];
            let opt1 = existing + std::f32::consts::PI * 2.0 / 3.0;
            let opt2 = existing - std::f32::consts::PI * 2.0 / 3.0;
            return if opt1.sin() <= opt2.sin() {
                normalize_angle(opt1)
            } else {
                normalize_angle(opt2)
            };
        }

        angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut max_gap = 0.0_f32;
        let mut best = 0.0_f32;
        let n = angles.len();
        for i in 0..n {
            let cur = angles[i];
            let next = if i + 1 < n {
                angles[i + 1]
            } else {
                angles[0] + std::f32::consts::TAU
            };
            let gap = next - cur;
            if gap > max_gap {
                max_gap = gap;
                best = cur + gap * 0.5;
            }
        }
        normalize_angle(best)
    }

    /// Main entry point called from the app each frame. Returns true if molecule was modified.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );
        let center = rect.center();
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 0.0, egui::Color32::WHITE);

        // Update hover state
        let mouse_pos = response.hover_pos().unwrap_or(egui::Pos2::ZERO);
        if rect.contains(mouse_pos) {
            self.hovered_atom = self.hit_test_atom(mouse_pos, center);
            self.hovered_bond = if self.hovered_atom.is_none() {
                self.hit_test_bond(mouse_pos, center)
            } else {
                None
            };
        } else {
            self.hovered_atom = None;
            self.hovered_bond = None;
        }

        // Zoom via scroll wheel
        if rect.contains(mouse_pos) {
            let scroll = ui.input(|i| i.raw_scroll_delta.y);
            if scroll != 0.0 {
                let factor: f32 = if scroll > 0.0 { 1.1 } else { 1.0 / 1.1 };
                let before = self.screen_to_mol(mouse_pos, center);
                self.zoom = (self.zoom * factor).clamp(0.1, 20.0);
                let after = self.mol_to_screen(before, center);
                self.pan += mouse_pos - after;
            }
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if (zoom_delta - 1.0).abs() > f32::EPSILON {
                let before = self.screen_to_mol(mouse_pos, center);
                self.zoom = (self.zoom * zoom_delta).clamp(0.1, 20.0);
                let after = self.mol_to_screen(before, center);
                self.pan += mouse_pos - after;
            }
        }

        let modified = match self.tool.clone() {
            Tool::Bond => interact::process_bond_tool(self, &response, center, ui),
            Tool::Select => interact::process_select_tool(self, &response, center, ui),
            Tool::Eraser => interact::process_eraser_tool(self, &response, center),
        };

        draw::draw_bonds(self, &painter, center);
        draw::draw_atom_backgrounds(self, &painter, center);
        draw::draw_atom_labels(self, &painter, center);
        draw::draw_overlays(self, &painter, center);

        modified
    }
}

pub fn point_to_segment_dist(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_sq = ab.length_sq();
    if ab_sq == 0.0 {
        return ap.length();
    }
    let t = ((ap.x * ab.x + ap.y * ab.y) / ab_sq).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

pub fn normalize_angle(a: f32) -> f32 {
    let mut a = a;
    while a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    while a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}
