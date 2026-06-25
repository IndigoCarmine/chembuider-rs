pub mod draw;
pub mod interact;

use crate::config::Config;
use crate::molecule::{BondOrder, BondStereo, Molecule};
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

pub const DEFAULT_BOND_LENGTH: f32 = 1.5;

/// Marker prefix on the clipboard text so paste can recognise our own format and
/// ignore unrelated text. The remainder is JSON (`ClipMol`).
const CLIP_PREFIX: &str = "chembuilder-mol:";

#[derive(Serialize, Deserialize)]
struct ClipAtom {
    element: String,
    pos: [f32; 2],
    charge: i8,
}

#[derive(Serialize, Deserialize)]
struct ClipBond {
    begin: usize,
    end: usize,
    order: BondOrder,
    #[serde(default)]
    stereo: BondStereo,
}

/// A self-contained copy of a sub-structure (atoms + the bonds among them), used for
/// clipboard copy/paste. Atom indices in bonds are positions within `atoms`.
#[derive(Serialize, Deserialize)]
struct ClipMol {
    atoms: Vec<ClipAtom>,
    bonds: Vec<ClipBond>,
}
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

    /// The "active" atom for keyboard navigation (hotspot).
    pub hotspot_atom: Option<u32>,

    /// Loaded keyboard/fragment configuration (from JSON).
    pub config: Config,

    /// Label text-edit mode: (atom_id, current_text).
    /// While Some, keyboard input goes to the text field instead of shortcuts.
    pub label_edit: Option<(u32, String)>,

    /// Undo history (molecule snapshots, newest at back).
    pub undo_stack: VecDeque<crate::molecule::Molecule>,

    /// Stereo type applied to newly drawn bonds.
    pub current_bond_stereo: BondStereo,

    /// Last known mouse position in molecule coordinates (for no-target atom placement).
    pub last_mouse_mol: [f32; 2],

    /// Active background cleanup computation, if any (None when idle).
    pub cleanup_job: Option<crate::molecule::cleanup::CleanupJob>,

    /// Previous-frame state of the "V" key (Windows OS-level Ctrl+V edge detection).
    last_v_down: bool,

    /// While rotating the selection (Alt+drag): the cursor angle from the last frame.
    rotate_last_angle: Option<f32>,
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
            hotspot_atom: None,
            config: Config::load(),
            label_edit: None,
            undo_stack: VecDeque::new(),
            current_bond_stereo: BondStereo::None,
            last_mouse_mol: [0.0; 2],
            cleanup_job: None,
            last_v_down: false,
            rotate_last_angle: None,
        }
    }
}

impl ChemStructEditor {
    /// Push a snapshot of the current molecule state for undo.
    /// Deduplicates: does nothing if the state is identical to the last snapshot.
    pub fn push_undo(&mut self) {
        let same = self.undo_stack.back().map_or(false, |s| s == &self.molecule);
        if !same {
            if self.undo_stack.len() >= 50 {
                self.undo_stack.pop_front();
            }
            self.undo_stack.push_back(self.molecule.clone());
        }
    }

    /// Build a fragment from the current selection and save it to `fragments/<name>.json`.
    /// The attach point (the atom that will bond to the rest of a molecule on insertion) is
    /// the hotspot atom when it is part of the selection, otherwise the lowest-id selected atom.
    /// Only bonds whose both endpoints are selected are included. Positions are stored relative
    /// to the attach atom. The new fragment is also registered in the live config for immediate use.
    pub fn save_selection_as_fragment(&mut self, name: &str) -> Result<usize, String> {
        use crate::config::{FragAtomDef, FragBondDef, FragmentDef};

        let name = name.trim();
        if name.is_empty() {
            return Err("fragment name is empty".into());
        }
        let mut ids: Vec<u32> = self.selected_atoms.iter().copied().collect();
        if ids.is_empty() {
            return Err("select the atoms to save first (Select tool)".into());
        }
        ids.sort_unstable();

        let attach_id = match self.hotspot_atom {
            Some(h) if ids.contains(&h) => h,
            _ => ids[0],
        };
        let attach_idx = ids.iter().position(|&i| i == attach_id).unwrap();
        let base = self.molecule.atom_by_id(attach_id).ok_or("attach atom missing")?.pos;

        let atoms: Vec<FragAtomDef> = ids.iter().map(|&id| {
            let a = self.molecule.atom_by_id(id).unwrap();
            FragAtomDef {
                element: a.element.clone(),
                pos: [a.pos[0] - base[0], a.pos[1] - base[1]],
                charge: a.charge,
            }
        }).collect();

        let bonds: Vec<FragBondDef> = self.molecule.bonds.iter().filter_map(|b| {
            let begin = ids.iter().position(|&i| i == b.begin)?;
            let end   = ids.iter().position(|&i| i == b.end)?;
            Some(FragBondDef { begin, end, order: b.order.clone() })
        }).collect();

        let def = FragmentDef { name: name.to_string(), atoms, bonds, attach_idx };

        std::fs::create_dir_all("fragments").map_err(|e| e.to_string())?;
        let path = format!("fragments/{}.json", name);
        let json = serde_json::to_string_pretty(&def).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;

        // Register in the live config (override by name) so it's usable without restart.
        match self.config.fragments.iter_mut().find(|f| f.name == name) {
            Some(slot) => *slot = def,
            None        => self.config.fragments.push(def),
        }
        Ok(ids.len())
    }

    /// Serialize the current selection (or the whole molecule when nothing is selected)
    /// into clipboard text. Returns None if there is nothing to copy.
    pub fn copy_to_string(&self) -> Option<String> {
        let ids: Vec<u32> = self.molecule.atoms.iter()
            .map(|a| a.id)
            .filter(|id| self.selected_atoms.is_empty() || self.selected_atoms.contains(id))
            .collect();
        if ids.is_empty() {
            return None;
        }
        let index: std::collections::HashMap<u32, usize> =
            ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        let atoms = ids.iter().map(|&id| {
            let a = self.molecule.atom_by_id(id).unwrap();
            ClipAtom { element: a.element.clone(), pos: a.pos, charge: a.charge }
        }).collect();
        let bonds = self.molecule.bonds.iter().filter_map(|b| {
            Some(ClipBond {
                begin: *index.get(&b.begin)?,
                end: *index.get(&b.end)?,
                order: b.order.clone(),
                stereo: b.stereo.clone(),
            })
        }).collect();

        serde_json::to_string(&ClipMol { atoms, bonds })
            .ok()
            .map(|json| format!("{CLIP_PREFIX}{json}"))
    }

    /// Publish a copy to the system clipboard. On Windows this also attaches a CDX blob under
    /// the "ChemDraw Interchange Format" so the structure can be pasted into ChemDraw.
    #[cfg(windows)]
    fn copy_to_clipboard(&self, _ctx: &egui::Context, text: String) {
        let emf = crate::molecule::image::molecule_to_emf(&self.molecule);
        let png = crate::molecule::image::molecule_to_png(&self.molecule);
        let dib = crate::molecule::image::molecule_to_dib(&self.molecule);
        let cdx = crate::molecule::cdx::molecule_to_cdx_bytes(&self.molecule);
        let embed = crate::molecule::ole::molecule_to_ole_embed(&self.molecule);
        let descriptor = embed
            .as_ref()
            .map(|_| crate::molecule::ole::object_descriptor(&self.molecule));
        if crate::clipboard::set_clipboard(
            &text,
            emf,
            png.as_deref(),
            dib.as_deref(),
            cdx.as_deref(),
            embed.as_deref(),
            descriptor.as_deref(),
        )
        .is_err()
        {
            // Fall back to egui's text-only clipboard if the native path fails.
            _ctx.copy_text(text);
        }
    }

    #[cfg(not(windows))]
    fn copy_to_clipboard(&self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text);
    }

    /// Parse clipboard text in our format and paste the atoms/bonds, offset slightly so the
    /// copy is visible, leaving the pasted atoms selected. Returns true if anything was added.
    pub fn paste_from_string(&mut self, text: &str) -> bool {
        let json = match text.strip_prefix(CLIP_PREFIX) {
            Some(rest) => rest,
            None => return false, // not our format
        };
        let Ok(clip) = serde_json::from_str::<ClipMol>(json) else { return false };
        if clip.atoms.is_empty() {
            return false;
        }
        self.push_undo();
        const OFFSET: [f32; 2] = [DEFAULT_BOND_LENGTH, DEFAULT_BOND_LENGTH];
        let new_ids: Vec<u32> = clip.atoms.iter().map(|a| {
            self.molecule.add_atom(a.element.clone(),
                [a.pos[0] + OFFSET[0], a.pos[1] + OFFSET[1]], a.charge)
        }).collect();
        for b in &clip.bonds {
            if let (Some(&begin), Some(&end)) = (new_ids.get(b.begin), new_ids.get(b.end)) {
                let bid = self.molecule.add_bond(begin, end, b.order.clone());
                if bid != 0 {
                    if let Some(bond) = self.molecule.bond_by_id_mut(bid) {
                        bond.stereo = b.stereo.clone();
                    }
                }
            }
        }
        self.selected_atoms = new_ids.into_iter().collect();
        true
    }

    /// Merge another molecule's atoms/bonds into this one, recentered on the cursor and
    /// selected. Recentering is essential for CDX from ChemDraw, whose absolute page
    /// coordinates would otherwise land the structure far off-screen.
    fn paste_molecule(&mut self, other: &Molecule) -> bool {
        if other.atoms.is_empty() {
            return false;
        }
        self.push_undo();
        let n = other.atoms.len() as f32;
        let cx = other.atoms.iter().map(|a| a.pos[0]).sum::<f32>() / n;
        let cy = other.atoms.iter().map(|a| a.pos[1]).sum::<f32>() / n;
        let target = self.last_mouse_mol; // drop the paste at the cursor
        let mut map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for a in &other.atoms {
            let pos = [a.pos[0] - cx + target[0], a.pos[1] - cy + target[1]];
            let id = self.molecule.add_atom(a.element.clone(), pos, a.charge);
            map.insert(a.id, id);
        }
        for b in &other.bonds {
            if let (Some(&begin), Some(&end)) = (map.get(&b.begin), map.get(&b.end)) {
                let bid = self.molecule.add_bond(begin, end, b.order.clone());
                if bid != 0 {
                    if let Some(bond) = self.molecule.bond_by_id_mut(bid) {
                        bond.stereo = b.stereo.clone();
                    }
                }
            }
        }
        self.selected_atoms = map.values().copied().collect();
        true
    }

    /// Try to paste a structure from the clipboard's ChemDraw CDX format (Windows only).
    #[cfg(windows)]
    fn try_paste_cdx(&mut self) -> bool {
        let Some(bytes) = crate::clipboard::read_cdx() else { return false };
        match crate::molecule::cdx::cdx_bytes_to_molecule(&bytes) {
            Some(mol) => self.paste_molecule(&mol),
            None => false,
        }
    }

    /// Detect a fresh Ctrl+V keystroke straight from the OS keyboard state. egui consumes
    /// Ctrl+V into a text-only `Event::Paste` (which never fires for ChemDraw's non-text CDX),
    /// so we bypass it. Returns true once per V press while Ctrl is held.
    #[cfg(windows)]
    fn ctrl_v_pressed(&mut self) -> bool {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetAsyncKeyState(v_key: i32) -> i16;
        }
        const VK_CONTROL: i32 = 0x11;
        const VK_V: i32 = 0x56;
        let down = |key: i32| (unsafe { GetAsyncKeyState(key) } as u16) & 0x8000 != 0;
        let v = down(VK_V);
        let edge = v && !self.last_v_down; // only on the press transition
        self.last_v_down = v;
        edge && down(VK_CONTROL)
    }

    /// Paste from the system clipboard on demand (toolbar button / Ctrl+V). Needed because egui
    /// converts Ctrl+V into a text-only `Event::Paste`, which never fires for non-text content
    /// like ChemDraw's CDX. Tries our richer JSON text first, then the ChemDraw CDX format.
    pub fn paste_clipboard(&mut self) -> bool {
        #[cfg(windows)]
        {
            // Our own copy uses the private "ChemBuilder Molecule" format (full fidelity);
            // then fall back to ChemDraw CDX, then any plain text.
            if let Some(s) = crate::clipboard::read_app_format() {
                if self.paste_from_string(&s) {
                    return true;
                }
            }
            if self.try_paste_cdx() {
                return true;
            }
            if let Some(text) = crate::clipboard::read_text() {
                return self.paste_from_string(&text);
            }
            false
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    /// True while a background cleanup computation is running.
    pub fn is_cleaning(&self) -> bool {
        self.cleanup_job.is_some()
    }

    /// Toggle the background cleanup: start it if idle, otherwise request it to stop.
    /// Starting snapshots the current molecule into a worker thread so the UI never freezes.
    pub fn toggle_cleanup(&mut self) {
        match &self.cleanup_job {
            Some(job) => job.cancel(),
            None => {
                if self.molecule.atoms.is_empty() {
                    return;
                }
                self.push_undo();
                self.cleanup_job = Some(crate::molecule::cleanup::CleanupJob::start(&self.molecule));
            }
        }
    }

    /// Pump the background cleanup once per frame: copy the latest positions in for a live
    /// preview, and finalize (join the worker) once it reports done. Returns true while active.
    pub fn poll_cleanup(&mut self) -> bool {
        let Some(job) = &self.cleanup_job else { return false };
        for (id, pos) in job.latest() {
            if let Some(atom) = self.molecule.atom_by_id_mut(id) {
                atom.pos = pos;
            }
        }
        if job.is_done() {
            self.cleanup_job.take().unwrap().join();
            false
        } else {
            true
        }
    }

    /// Restore the previous molecule state.
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop_back() {
            self.molecule = prev;
            self.selected_atoms.clear();
            self.hotspot_atom = None;
            self.label_edit = None;
            true
        } else {
            false
        }
    }

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
            // Folded terminal H atoms are invisible; don't let them capture hover/clicks.
            if draw::is_folded_h(&self.molecule, atom.id) { continue; }
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

    /// Returns `(angle, flip)` for inserting a fragment at `atom_id`.
    /// `angle` is the direction from the attach atom toward the fragment's next atom.
    /// `flip`  is true when mirroring the fragment across that axis gives better clearance
    /// from existing bonds (i.e. the bulk of the ring/group should go the other way).
    pub fn best_fragment_placement(
        &self,
        atom_id: u32,
        frag: &crate::molecule::fragment::Fragment,
    ) -> (f32, bool) {
        let base_angle = self.best_new_bond_angle(atom_id);

        // Linear or trivial fragments: no flip needed.
        if frag.atoms.len() <= 2 {
            return (base_angle, false);
        }

        let attach_raw = frag.atoms[frag.attach_idx].pos;
        let next_idx   = (frag.attach_idx + 1) % frag.atoms.len();
        let next_raw   = frag.atoms[next_idx].pos;
        let raw_angle  = (next_raw[1] - attach_raw[1]).atan2(next_raw[0] - attach_raw[0]);

        // Centroid of the fragment in canonical frame (raw_angle = 0).
        let n = frag.atoms.len() as f32;
        let sum_dx: f32 = frag.atoms.iter().map(|a| a.pos[0] - attach_raw[0]).sum::<f32>() / n;
        let sum_dy: f32 = frag.atoms.iter().map(|a| a.pos[1] - attach_raw[1]).sum::<f32>() / n;
        let (sin_neg, cos_neg) = (-raw_angle).sin_cos();
        let dx_c =  sum_dx * cos_neg - sum_dy * sin_neg;
        let dy_c =  sum_dx * sin_neg + sum_dy * cos_neg;

        // If the centroid is nearly on-axis, flipping has no effect.
        if dy_c.abs() < 0.05 {
            return (base_angle, false);
        }

        let src_pos = match self.molecule.atom_by_id(atom_id) {
            Some(a) => a.pos,
            None    => return (base_angle, false),
        };
        let (sin_a, cos_a) = base_angle.sin_cos();

        // World position of centroid for normal and flipped orientations.
        let center_normal = [
            src_pos[0] + dx_c * cos_a - dy_c * sin_a,
            src_pos[1] + dx_c * sin_a + dy_c * cos_a,
        ];
        let center_flip = [
            src_pos[0] + dx_c * cos_a + dy_c * sin_a,  // dy_c negated
            src_pos[1] + dx_c * sin_a - dy_c * cos_a,
        ];

        // Score: min squared distance from centroid to existing neighbor atoms.
        // Larger score = more space = preferred.
        let neighbors = self.molecule.neighbor_atom_ids(atom_id);
        if neighbors.is_empty() {
            return (base_angle, false);
        }

        let min_dist_sq = |cx: f32, cy: f32| -> f32 {
            neighbors.iter()
                .filter_map(|&nid| self.molecule.atom_by_id(nid))
                .map(|nb| {
                    let ddx = nb.pos[0] - cx;
                    let ddy = nb.pos[1] - cy;
                    ddx * ddx + ddy * ddy
                })
                .fold(f32::MAX, f32::min)
        };

        let score_normal = min_dist_sq(center_normal[0], center_normal[1]);
        let score_flip   = min_dist_sq(center_flip[0],   center_flip[1]);

        (base_angle, score_flip > score_normal)
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

        // Update hover state and track mouse in mol coords
        let mouse_pos = response.hover_pos().unwrap_or(egui::Pos2::ZERO);
        if rect.contains(mouse_pos) {
            self.last_mouse_mol = self.screen_to_mol(mouse_pos, center);
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

        // Ctrl+Z: undo
        let did_undo = ui.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift);
        if did_undo { self.undo(); }

        // Copy/paste only act on the canvas when no text field has focus.
        let editing_text = ui.ctx().wants_keyboard_input();

        // Copy (Ctrl/Cmd+C): egui converts the shortcut into Event::Copy/Cut — `key_pressed(C)`
        // does NOT fire — so we listen for those events (with a raw-key fallback). On Windows
        // we publish text + CDX (+ image + Embed Source); elsewhere just text.
        let do_copy = !editing_text && ui.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut))
                || (i.modifiers.command && i.key_pressed(egui::Key::C))
        });
        if do_copy {
            if let Some(text) = self.copy_to_string() {
                self.copy_to_clipboard(ui.ctx(), text);
            }
        }

        // Paste (Ctrl+V). On Windows, detect the keystroke from the OS (egui's Ctrl+V is
        // text-only and won't fire for ChemDraw's non-text CDX) and read the clipboard
        // directly. Elsewhere, use egui's Event::Paste(text).
        #[cfg(windows)]
        {
            let app_focused = ui.input(|i| i.focused);
            // Always sample the key so edge-detection state stays current, even if suppressed.
            let ctrl_v = self.ctrl_v_pressed();
            if ctrl_v && app_focused && !editing_text {
                self.paste_clipboard();
            }
        }
        #[cfg(not(windows))]
        if !editing_text {
            let pasted = ui.input(|i| i.events.iter().find_map(|e| match e {
                egui::Event::Paste(t) => Some(t.clone()),
                _ => None,
            }));
            if let Some(text) = pasted {
                self.paste_from_string(&text);
            }
        }

        // Ctrl+K: toggle background clean-up (start, or stop if already running).
        let cleanup_toggle = ui.input(|i| i.key_pressed(egui::Key::K) && i.modifiers.ctrl);
        if cleanup_toggle {
            self.toggle_cleanup();
        }
        // Pump the background job: live-preview positions, finalize when done.
        let was_cleaning = self.is_cleaning();
        let cleaning = self.poll_cleanup();
        if cleaning || was_cleaning {
            // Keep animating while the worker runs, and force one more frame on completion
            // so the toolbar button reverts from "Stop" to "Clean Up" (egui is otherwise
            // idle without input).
            ui.ctx().request_repaint();
        }

        // ── Label text-edit overlay ──────────────────────────────────────────
        let mut label_confirmed = false;
        // Extract what we need before the mutable borrow of label_edit
        let label_size = self.config.style.label_size;
        let label_state: Option<(u32, egui::Pos2)> = self.label_edit.as_ref().and_then(|(atom_id, _)| {
            let pos = self.molecule.atom_by_id(*atom_id)?.pos;
            Some((*atom_id, self.mol_to_screen(pos, center)))
        });

        if let (Some((edit_id, screen_pos)), Some((_, text))) =
            (label_state, self.label_edit.as_mut())
        {
            let area_resp = egui::Area::new(egui::Id::new("label_edit_area"))
                .fixed_pos(screen_pos + egui::vec2(12.0, -12.0))
                .show(ui.ctx(), |ui| {
                    let edit = egui::TextEdit::singleline(text)
                        .desired_width(80.0)
                        .font(egui::FontId::proportional(label_size));
                    ui.add(edit)
                });
            area_resp.inner.request_focus();

            let enter  = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if enter || area_resp.inner.lost_focus() {
                let t = text.trim().to_string();
                if !t.is_empty() {
                    if let Some(a) = self.molecule.atom_by_id_mut(edit_id) {
                        a.element = t;
                    }
                }
                label_confirmed = true;
            }
            if escape { label_confirmed = true; }
        } else if self.label_edit.is_some() {
            label_confirmed = true; // atom no longer exists
        }
        if label_confirmed { self.label_edit = None; }
        let label_editing = self.label_edit.is_some();

        // Suppress canvas key/mouse handling whenever a text field (our label editor or a
        // toolbar field like the fragment-name input) has keyboard focus, so typed characters
        // don't leak into atom/bond shortcuts; also while a background cleanup is rewriting
        // positions (editing would fight the worker).
        let tool_modified = if label_editing || cleaning || ui.ctx().wants_keyboard_input() {
            false
        } else {
            match self.tool.clone() {
                Tool::Bond   => interact::process_bond_tool(self, &response, center, ui),
                Tool::Select => interact::process_select_tool(self, &response, center, ui),
                Tool::Eraser => interact::process_eraser_tool(self, &response, center),
            }
        };
        let modified = cleanup_toggle | label_confirmed | tool_modified | cleaning;

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

#[cfg(test)]
mod clipboard_tests {
    use super::*;

    #[test]
    fn copy_paste_roundtrip() {
        let mut e = ChemStructEditor::default();
        let a = e.molecule.add_atom("O".to_string(), [0.0, 0.0], 0);
        let b = e.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        e.molecule.add_bond(a, b, BondOrder::Double);

        // Copy (whole molecule, since nothing is selected).
        let text = e.copy_to_string().expect("should copy");
        assert!(text.starts_with(CLIP_PREFIX));

        let atoms_before = e.molecule.atoms.len();
        let bonds_before = e.molecule.bonds.len();
        assert!(e.paste_from_string(&text));
        assert_eq!(e.molecule.atoms.len(), atoms_before + 2);
        assert_eq!(e.molecule.bonds.len(), bonds_before + 1);
        assert_eq!(e.selected_atoms.len(), 2, "pasted atoms should be selected");

        // Foreign clipboard text is ignored.
        assert!(!e.paste_from_string("just some text"));
        assert!(!e.paste_from_string(""));
    }

    #[test]
    fn paste_molecule_recenters_on_cursor() {
        let mut e = ChemStructEditor::default();
        e.last_mouse_mol = [5.0, 7.0];
        // A structure with large absolute coords, like CDX pasted from ChemDraw.
        let mut other = Molecule::default();
        other.add_atom("C".to_string(), [100.0, 200.0], 0);
        other.add_atom("O".to_string(), [101.5, 200.0], 0);
        assert!(e.paste_molecule(&other));
        let n = e.molecule.atoms.len() as f32;
        let cx = e.molecule.atoms.iter().map(|a| a.pos[0]).sum::<f32>() / n;
        let cy = e.molecule.atoms.iter().map(|a| a.pos[1]).sum::<f32>() / n;
        assert!((cx - 5.0).abs() < 0.01 && (cy - 7.0).abs() < 0.01,
            "pasted centroid should land on the cursor, got ({cx},{cy})");
    }
}
