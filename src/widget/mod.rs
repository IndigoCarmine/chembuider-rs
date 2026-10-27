pub mod analysis;
pub mod bond_menu;
pub mod draw;
pub mod interact;
#[cfg(test)]
mod keyboard_solver;
#[cfg(test)]
mod keyboard_tests;
#[cfg(test)]
mod page_tests;
#[cfg(test)]
mod render_tests;
pub mod rich_text;
pub mod scene;
pub mod view;

pub use view::{FitOptions, ViewTransform};

use crate::config::Config;
use crate::molecule::{ArrowKind, BondOrder, BondStereo, DoublePosition, Molecule, PageObject, TextAlign};
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
    /// Carried across so a copy/paste does not turn a group's folded hydrogen into a drawn
    /// one. Defaulted, so clipboard text (and the test fixtures) written before it existed
    /// still parse.
    #[serde(default)]
    group_h: bool,
}

#[derive(Serialize, Deserialize)]
struct ClipBond {
    begin: usize,
    end: usize,
    order: BondOrder,
    #[serde(default)]
    stereo: BondStereo,
    #[serde(default)]
    double_position: DoublePosition,
}

/// A ring fill on the clipboard: atom *indices* (positions within `ClipMol::atoms`, not ids,
/// which change on paste) plus the straight RGBA color.
#[derive(Serialize, Deserialize)]
struct ClipRingFill {
    atoms: Vec<usize>,
    color: [u8; 4],
}

/// A self-contained copy of a sub-structure (atoms + the bonds among them), used for
/// clipboard copy/paste. Atom indices in bonds are positions within `atoms`.
#[derive(Serialize, Deserialize)]
struct ClipMol {
    atoms: Vec<ClipAtom>,
    bonds: Vec<ClipBond>,
    /// Colored ring interiors. Defaulted, so clipboard text written before this field existed
    /// still parses (and a copy with no fills omits it entirely).
    #[serde(default)]
    ring_fills: Vec<ClipRingFill>,
}
pub const SCALE_FACTOR: f32 = 50.0;
pub const NODE_HIT_RADIUS_PX: f32 = 15.0;
pub const BOND_HIT_THRESHOLD_PX: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Tool {
    Select,
    Bond,
    Eraser,
    /// Draw a reaction or equilibrium arrow: drag from tail to head (the direction snaps to
    /// 15° steps; hold Shift for any angle), or click to place one of the default length.
    Arrow(ArrowKind),
    /// Place or edit a text (reaction conditions, captions): click empty canvas to start one,
    /// click a text to edit it. Enter or a click elsewhere commits, Shift+Enter starts a new
    /// line, Escape cancels; a text left empty is removed.
    Text,
}

impl std::fmt::Display for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tool::Select => write!(f, "Select"),
            Tool::Bond => write!(f, "Bond"),
            Tool::Eraser => write!(f, "Eraser"),
            Tool::Arrow(ArrowKind::Reaction) => write!(f, "Reaction Arrow"),
            Tool::Arrow(ArrowKind::Equilibrium) => write!(f, "Equilibrium Arrow"),
            Tool::Text => write!(f, "Text"),
        }
    }
}

/// A page text being typed or edited ([`ChemStructEditor::text_edit`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TextEditState {
    /// The text being edited, or `None` for a new one, added when committed.
    pub id: Option<u32>,
    /// Where the text is anchored (see [`PageText::pos`](crate::molecule::PageText::pos)).
    pub pos: [f32; 2],
    pub align: TextAlign,
    /// The markup typed so far, lines separated by `\n`.
    pub text: String,
}

/// A reaction-table cell being typed into ([`ChemStructEditor::table_edit`]).
#[derive(Debug, Clone, PartialEq)]
pub struct TableCellEdit {
    /// The table ([`ReactionTable::id`](crate::molecule::ReactionTable::id)).
    pub id: u32,
    pub cell: crate::reaction::edit::CellRef,
    /// What has been typed: a value with an optional unit (`1.5`, `250 mg`, `2 M`).
    pub text: String,
    /// Why the last Enter was refused, shown under the box.
    pub error: Option<String>,
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

    /// The "active" bond for keyboard bond actions (order, stereo, ring/fragment fusion).
    ///
    /// Bond shortcuts are otherwise reachable only by hovering a bond with the mouse, which
    /// makes bond order and ring fusion impossible to express with the keyboard alone. Set
    /// with Shift+arrow from [`hotspot_atom`](Self::hotspot_atom); cleared by Space/Tab or by
    /// moving the atom hotspot.
    pub hotspot_bond: Option<u32>,

    /// Loaded keyboard/fragment configuration (from JSON).
    pub config: Config,

    /// Label text-edit mode: (atom_id, current_text).
    /// While Some, keyboard input goes to the text field instead of shortcuts.
    pub label_edit: Option<(u32, String)>,

    /// Undo history (molecule snapshots, newest at back).
    pub undo_stack: VecDeque<crate::molecule::Molecule>,

    /// Redo history (molecule snapshots undone via `undo`, newest at back). Cleared
    /// whenever a new edit is pushed onto `undo_stack` — the usual undo/redo contract.
    pub redo_stack: VecDeque<crate::molecule::Molecule>,

    /// Stereo type applied to newly drawn bonds.
    pub current_bond_stereo: BondStereo,

    /// Last known mouse position in molecule coordinates (for no-target atom placement).
    pub last_mouse_mol: [f32; 2],

    /// Whether a pointer was actually over the canvas this frame.
    ///
    /// Distinguishes "the cursor is somewhere the user can see" from keyboard-only use, where
    /// [`last_mouse_mol`](Self::last_mouse_mol) is a stale leftover rather than a place the
    /// user is pointing at. Element keys use this to decide between dropping a new atom at
    /// the cursor and retargeting the hotspot.
    pub pointer_active: bool,

    /// Active background cleanup computation, if any (None when idle).
    pub cleanup_job: Option<crate::molecule::cleanup::CleanupJob>,

    /// Starts the runs behind [`toggle_cleanup`](Self::toggle_cleanup); the built-in cleanup
    /// unless replaced with [`set_cleanup_engine`](Self::set_cleanup_engine).
    cleanup_engine: Box<dyn crate::molecule::cleanup::CleanupEngine + Send + Sync>,

    /// Previous-frame state of the "V" key (Windows OS-level Ctrl+V edge detection).
    last_v_down: bool,

    /// While rotating the selection (Alt+drag): the cursor angle from the last frame.
    rotate_last_angle: Option<f32>,

    /// Screen rects of the atom labels painted last frame, for hit testing (wide labels
    /// like "CO2Et" should be clickable over their whole text, not just a small circle).
    /// One frame of lag is fine — hover runs before the draw pass.
    last_label_rects: std::collections::HashMap<u32, egui::Rect>,

    /// Set by [`request_fit`](Self::request_fit); the next `ui()` fits the view and clears it.
    fit_requested: bool,

    /// The bond under the pointer when the canvas was last right-clicked: what the host's
    /// context menu acts on. Kept until [`clear_context_bond`](Self::clear_context_bond).
    context_bond: Option<u32>,

    /// Selected page objects (arrows, texts), by id: moved, deleted and rotated with the
    /// selected atoms.
    pub selected_objects: HashSet<u32>,

    /// The page object under the pointer (when no atom is).
    pub hovered_object: Option<u32>,

    /// The page text being typed, while the text box is open. Keyboard input goes to it.
    pub text_edit: Option<TextEditState>,

    /// A drag on page objects in progress (drawing an arrow, moving objects, an arrow's end).
    pub(crate) object_drag: Option<interact::ObjectDrag>,

    /// A reaction-table cell being typed into, while its box is open.
    pub table_edit: Option<TableCellEdit>,

    /// Screen rects of the page texts and tables painted last frame, for hit testing, as for
    /// labels; and the cells of each table.
    last_object_rects: std::collections::HashMap<u32, egui::Rect>,
    last_table_cells: std::collections::HashMap<u32, Vec<(crate::reaction::edit::CellRef, egui::Rect)>>,
}

/// What [`ChemStructEditor::show`] hands back: the canvas response, for a host to attach a
/// [`context_menu`](egui::Response::context_menu) to, and whether the molecule was modified.
pub struct EditorResponse {
    pub response: egui::Response,
    pub modified: bool,
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
            hotspot_bond: None,
            config: Config::default(),
            label_edit: None,
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            current_bond_stereo: BondStereo::None,
            last_mouse_mol: [0.0; 2],
            pointer_active: false,
            cleanup_job: None,
            cleanup_engine: Box::new(crate::molecule::cleanup::DefaultCleanupEngine),
            last_v_down: false,
            rotate_last_angle: None,
            last_label_rects: std::collections::HashMap::new(),
            fit_requested: false,
            context_bond: None,
            selected_objects: HashSet::new(),
            hovered_object: None,
            text_edit: None,
            object_drag: None,
            table_edit: None,
            last_object_rects: std::collections::HashMap::new(),
            last_table_cells: std::collections::HashMap::new(),
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
            self.redo_stack.clear();
        }
    }

    /// The bond the canvas was right-clicked on, for the host's context menu. The pointer is
    /// over the menu while it is open, so the hover cannot say this; read it inside the
    /// [`context_menu`](egui::Response::context_menu) closure of [`show`](Self::show)'s response.
    pub fn context_bond(&self) -> Option<u32> {
        self.context_bond
    }

    /// Forget the context-menu bond (once the menu has closed).
    pub fn clear_context_bond(&mut self) {
        self.context_bond = None;
    }

    /// Give bond `bond_id` an order and line style in one undo step — what a ChemDraw-style
    /// bond menu's "Single ▸ Hashed Wedged" or "Double" does. Raising the order consumes the
    /// group hydrogens it leaves no room for, as the order keys do. A style is drawn only on a
    /// single bond, so it is dropped for any higher order. Returns false when the bond does
    /// not exist or already looks like this.
    pub fn set_bond_type(&mut self, bond_id: u32, order: BondOrder, stereo: BondStereo) -> bool {
        let stereo = if order == BondOrder::Single { stereo } else { BondStereo::None };
        let Some(bond) = self.molecule.bond_by_id(bond_id) else { return false };
        if bond.order == order && bond.stereo == stereo {
            return false;
        }
        self.push_undo();
        let Some(bond) = self.molecule.bond_by_id_mut(bond_id) else { return false };
        let grew = order.valence() > bond.order.valence();
        bond.order = order;
        bond.stereo = stereo;
        let (begin, end) = (bond.begin, bond.end);
        if grew {
            interact::consume_group_h(self, &[begin, end]);
        }
        true
    }

    /// Choose which side of bond `bond_id` its double-bond line goes on ([`DoublePosition`]).
    /// One undo step; false when the bond does not exist or already has this position.
    pub fn set_bond_double_position(&mut self, bond_id: u32, position: DoublePosition) -> bool {
        match self.molecule.bond_by_id(bond_id) {
            Some(bond) if bond.double_position != position => {}
            _ => return false,
        }
        self.push_undo();
        if let Some(bond) = self.molecule.bond_by_id_mut(bond_id) {
            bond.double_position = position;
        }
        true
    }

    /// Delete bond `bond_id` (its atoms stay). One undo step; false when there is no such bond.
    pub fn delete_bond(&mut self, bond_id: u32) -> bool {
        if self.molecule.bond_by_id(bond_id).is_none() {
            return false;
        }
        self.push_undo();
        self.molecule.remove_bond(bond_id);
        for slot in [&mut self.hovered_bond, &mut self.hotspot_bond, &mut self.context_bond] {
            if *slot == Some(bond_id) {
                *slot = None;
            }
        }
        true
    }

    /// Fill the smallest ring passing through `atom_id` with `color` (straight RGBA), drawn
    /// behind the bonds. Returns false (and does nothing) when the atom is on no ring. Records
    /// an undo step so the fill can be reverted with Ctrl+Z.
    pub fn fill_ring_at(&mut self, atom_id: u32, color: [u8; 4]) -> bool {
        match self.molecule.smallest_ring_containing(atom_id) {
            Some(ring) => {
                self.push_undo();
                self.molecule.set_ring_fill(ring, color);
                true
            }
            None => false,
        }
    }

    /// Fill the ring the current selection points at.
    ///
    /// A selection of two or more atoms *is* the ring the user means, so it wins over the
    /// hotspot (which only follows the cursor and, on a fused ring, may sit on the wrong cycle):
    /// every ring confined to the selected atoms is filled — the four-membered ring for the four
    /// atoms of a square fused onto benzene, the hexagon for benzene's six, both for a selection
    /// spanning several rings — under a single undo step. When the selection
    /// encloses no ring of its own we fall back to a single seed, still preferring an explicit
    /// selection over the hotspot; a lone selected atom or a bare hotspot keeps the old
    /// smallest-ring-through-the-seed behavior. Returns false when nothing is selected/hovered
    /// or the seed atom is on no ring.
    pub fn fill_selected_ring(&mut self, color: [u8; 4]) -> bool {
        if self.selected_atoms.len() >= 2 {
            let sel: Vec<u32> = self.selected_atoms.iter().copied().collect();
            let rings = self.molecule.rings_from_atoms(&sel);
            if !rings.is_empty() {
                self.push_undo();
                for ring in rings {
                    self.molecule.set_ring_fill(ring, color);
                }
                return true;
            }
        }
        let seed = self
            .selected_atoms
            .iter()
            .copied()
            .min()
            .or(self.hotspot_atom);
        match seed {
            Some(a) => self.fill_ring_at(a, color),
            None => false,
        }
    }

    /// Remove the ring fill through the hotspot / any selected atom. Returns true if one went.
    pub fn clear_selected_ring_fill(&mut self) -> bool {
        let seed = self
            .hotspot_atom
            .or_else(|| self.selected_atoms.iter().copied().min());
        match seed {
            Some(a) => {
                self.push_undo();
                let removed = self.molecule.clear_ring_fill_containing(a);
                if !removed {
                    self.undo_stack.pop_back();
                }
                removed
            }
            None => false,
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
        // The name becomes `fragments/<name>.json`; refuse one that is not a plain file name.
        crate::molecule::fragment::Fragment::validate_name(name)?;
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
            ClipAtom { element: a.element.clone(), pos: a.pos, charge: a.charge, group_h: a.group_h }
        }).collect();
        let bonds = self.molecule.bonds.iter().filter_map(|b| {
            Some(ClipBond {
                begin: *index.get(&b.begin)?,
                end: *index.get(&b.end)?,
                order: b.order.clone(),
                stereo: b.stereo.clone(),
                double_position: b.double_position,
            })
        }).collect();

        // Ring fills survive the copy only when every ring atom is in the copied set.
        let ring_fills = self.molecule.ring_fills.iter().filter_map(|rf| {
            let atoms: Option<Vec<usize>> = rf.atoms.iter().map(|id| index.get(id).copied()).collect();
            Some(ClipRingFill { atoms: atoms?, color: rf.color })
        }).collect();

        serde_json::to_string(&ClipMol { atoms, bonds, ring_fills })
            .ok()
            .map(|json| format!("{CLIP_PREFIX}{json}"))
    }

    /// Copy the selection (or the whole molecule when nothing is selected) to the system
    /// clipboard, as Ctrl+C does: for a host's Edit menu. Returns false when there is nothing to
    /// copy.
    pub fn copy(&self, ctx: &egui::Context) -> bool {
        let Some(text) = self.copy_to_string() else { return false };
        self.copy_to_clipboard(ctx, text);
        true
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
        // The pasted atoms join the molecule a running cleanup is rewriting; stop it first.
        self.cancel_cleanup();
        self.push_undo();
        const OFFSET: [f32; 2] = [DEFAULT_BOND_LENGTH, DEFAULT_BOND_LENGTH];
        let new_ids: Vec<u32> = clip.atoms.iter().map(|a| {
            let id = self.molecule.add_atom(a.element.clone(),
                [a.pos[0] + OFFSET[0], a.pos[1] + OFFSET[1]], a.charge);
            if let Some(atom) = self.molecule.atom_by_id_mut(id) {
                atom.group_h = a.group_h;
            }
            id
        }).collect();
        for b in &clip.bonds {
            if let (Some(&begin), Some(&end)) = (new_ids.get(b.begin), new_ids.get(b.end)) {
                let bid = self.molecule.add_bond(begin, end, b.order.clone());
                if bid != 0 {
                    if let Some(bond) = self.molecule.bond_by_id_mut(bid) {
                        bond.stereo = b.stereo.clone();
                        bond.double_position = b.double_position;
                    }
                }
            }
        }
        // Restore ring fills, remapping the copied atom indices to the freshly created ids.
        for rf in &clip.ring_fills {
            let ids: Option<Vec<u32>> = rf.atoms.iter().map(|i| new_ids.get(*i).copied()).collect();
            if let Some(ids) = ids {
                self.molecule.set_ring_fill(ids, rf.color);
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
        // The pasted atoms join the molecule a running cleanup is rewriting; stop it first.
        self.cancel_cleanup();
        self.push_undo();
        let n = other.atoms.len() as f32;
        let cx = other.atoms.iter().map(|a| a.pos[0]).sum::<f32>() / n;
        let cy = other.atoms.iter().map(|a| a.pos[1]).sum::<f32>() / n;
        let target = self.last_mouse_mol; // drop the paste at the cursor
        let mut map: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        for a in &other.atoms {
            let pos = [a.pos[0] - cx + target[0], a.pos[1] - cy + target[1]];
            let id = self.molecule.add_atom(a.element.clone(), pos, a.charge);
            if let Some(atom) = self.molecule.atom_by_id_mut(id) {
                atom.group_h = a.group_h;
            }
            map.insert(a.id, id);
        }
        for b in &other.bonds {
            if let (Some(&begin), Some(&end)) = (map.get(&b.begin), map.get(&b.end)) {
                let bid = self.molecule.add_bond(begin, end, b.order.clone());
                if bid != 0 {
                    if let Some(bond) = self.molecule.bond_by_id_mut(bid) {
                        bond.stereo = b.stereo.clone();
                        bond.double_position = b.double_position;
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
    /// Starting pushes an undo step and starts a run from the cleanup engine (see
    /// [`set_cleanup_engine`](Self::set_cleanup_engine)). The built-in engine relaxes a snapshot
    /// of the molecule in the background (a worker thread natively, one chunk per poll on
    /// wasm32) so the UI never freezes.
    ///
    /// Stopping this way only asks the run to stop: the next [`poll_cleanup`](Self::poll_cleanup)
    /// still copies in the positions the run ended on, so the layout reached so far is kept.
    /// To drop a run without letting it write anything more, use
    /// [`cancel_cleanup`](Self::cancel_cleanup).
    pub fn toggle_cleanup(&mut self) {
        match &self.cleanup_job {
            Some(job) => job.cancel(),
            None => {
                if self.molecule.atoms.is_empty() {
                    return;
                }
                self.push_undo();
                self.cleanup_job = Some(self.cleanup_engine.start(&self.molecule));
            }
        }
    }

    /// Stop a running cleanup at once and discard it: the run is cancelled and dropped, and no
    /// more of its positions are written, so the molecule keeps whatever it holds now (the
    /// last preview).  Returns whether a cleanup was running.
    ///
    /// Call this before replacing or restructuring the molecule (undo/redo, paste, a host's
    /// Clear or load): the run works on a snapshot of the old molecule and would otherwise go
    /// on writing its positions, by atom id, into the new one.
    ///
    /// A run that has already finished is joined.  One still going is dropped instead
    /// (dropping must stop its background work, see
    /// [`CleanupRun`](crate::molecule::cleanup::CleanupRun)): the built-in worker only sees the
    /// cancel at its next chunk boundary, which for a few hundred atoms or more is a second
    /// or more away, and joining would freeze the UI that long.  The worker then stops on its
    /// own, writing only to its own buffer.
    pub fn cancel_cleanup(&mut self) -> bool {
        let Some(job) = self.cleanup_job.take() else { return false };
        job.cancel();
        if job.is_done() {
            job.join();
        }
        true
    }

    /// Replace the engine that [`toggle_cleanup`](Self::toggle_cleanup) starts runs from.
    /// A cleanup already running is unaffected; the next one uses `engine`.
    pub fn set_cleanup_engine(
        &mut self,
        engine: impl crate::molecule::cleanup::CleanupEngine + Send + Sync + 'static,
    ) {
        self.cleanup_engine = Box::new(engine);
    }

    /// Pump the background cleanup once per frame: copy the latest positions in for a live
    /// preview, and finalize (join the worker) once it reports done. Returns true while active.
    pub fn poll_cleanup(&mut self) -> bool {
        let Some(job) = &self.cleanup_job else { return false };
        // Read `is_done` before `latest`: only positions read after `is_done` returned true are
        // guaranteed final.  Checked the other way round, a run finishing between the two
        // calls would end the cleanup on the preview read just before its final positions.
        let done = job.is_done();
        for (id, pos) in job.latest() {
            if let Some(atom) = self.molecule.atom_by_id_mut(id) {
                atom.pos = pos;
            }
        }
        if done {
            self.cleanup_job.take().unwrap().join();
            false
        } else {
            true
        }
    }

    /// Restore the previous molecule state, pushing the current one onto the redo stack.
    ///
    /// While a cleanup runs, undo cancels it instead: the run is discarded
    /// ([`cancel_cleanup`](Self::cancel_cleanup)) and the molecule goes back to the snapshot
    /// Cleanup took when it started (the top of the undo stack).  The half-relaxed preview is
    /// dropped rather than pushed onto the redo stack, so the redo history stays the one from
    /// before Cleanup started.
    pub fn undo(&mut self) -> bool {
        let cancelled_cleanup = self.cancel_cleanup();
        if let Some(prev) = self.undo_stack.pop_back() {
            if cancelled_cleanup {
                self.molecule = prev;
            } else {
                if self.redo_stack.len() >= 50 {
                    self.redo_stack.pop_front();
                }
                self.redo_stack.push_back(std::mem::replace(&mut self.molecule, prev));
            }
            self.clear_transient_state();
            true
        } else {
            false
        }
    }

    /// Re-apply a state previously undone, pushing the current one back onto the undo stack.
    ///
    /// A running cleanup is stopped first ([`cancel_cleanup`](Self::cancel_cleanup)), keeping
    /// the layout it reached as a Ctrl+K stop would, so it cannot overwrite the redone state.
    pub fn redo(&mut self) -> bool {
        self.cancel_cleanup();
        if let Some(next) = self.redo_stack.pop_back() {
            if self.undo_stack.len() >= 50 {
                self.undo_stack.pop_front();
            }
            self.undo_stack.push_back(std::mem::replace(&mut self.molecule, next));
            self.clear_transient_state();
            true
        } else {
            false
        }
    }

    /// Forget selections and in-progress edits, which may point at things the molecule just
    /// lost (undo, redo).
    fn clear_transient_state(&mut self) {
        self.selected_atoms.clear();
        self.selected_objects.clear();
        self.hotspot_atom = None;
        self.label_edit = None;
        self.text_edit = None;
        self.table_edit = None;
        self.object_drag = None;
    }

    /// Delete everything selected, atoms and page objects, in one undo step. Returns false
    /// when nothing is selected.
    pub fn delete_selection(&mut self) -> bool {
        if self.selected_atoms.is_empty() && self.selected_objects.is_empty() {
            return false;
        }
        self.push_undo();
        for id in std::mem::take(&mut self.selected_atoms) {
            self.molecule.remove_atom(id);
        }
        for id in std::mem::take(&mut self.selected_objects) {
            self.molecule.remove_object(id);
        }
        true
    }

    /// Open the text box on page text `id` (as a double-click or the Text tool does). False
    /// when there is no such text.
    pub fn edit_text(&mut self, id: u32) -> bool {
        let Some(PageObject::Text(t)) = self.molecule.object_by_id(id) else { return false };
        self.text_edit = Some(TextEditState { id: Some(id), pos: t.pos, align: t.align, text: t.text.clone() });
        true
    }

    /// Close the text box, keeping what was typed: a new text is added, an edited one changed,
    /// and one left blank removed, each as one undo step. Returns whether the molecule changed.
    pub fn commit_text_edit(&mut self) -> bool {
        let Some(edit) = self.text_edit.take() else { return false };
        let text = edit.text.trim_end().to_string();
        let blank = text.trim().is_empty();
        match edit.id {
            Some(id) => {
                let Some(PageObject::Text(current)) = self.molecule.object_by_id(id) else { return false };
                if blank {
                    self.push_undo();
                    self.molecule.remove_object(id);
                    self.selected_objects.remove(&id);
                    true
                } else if current.text != text {
                    self.push_undo();
                    if let Some(PageObject::Text(t)) = self.molecule.object_by_id_mut(id) {
                        t.text = text;
                    }
                    true
                } else {
                    false
                }
            }
            None if blank => false,
            None => {
                self.push_undo();
                let id = self.molecule.add_text(edit.pos, text);
                if let Some(PageObject::Text(t)) = self.molecule.object_by_id_mut(id) {
                    t.align = edit.align;
                }
                true
            }
        }
    }

    /// Insert a reaction table for the reaction of the selected arrow (or of the first reaction
    /// arrow on the page; every structure when there is none), under it, as one undo step, and
    /// select it. Returns its id, or `None` when there are no structures to tabulate.
    pub fn insert_reaction_table(&mut self) -> Option<u32> {
        let arrow = self
            .selected_objects
            .iter()
            .copied()
            .find(|id| self.molecule.object_by_id(*id).is_some_and(|o| o.as_arrow().is_some()));
        let table = crate::molecule::reaction_table::new_table(&self.molecule, arrow)?;
        self.push_undo();
        let id = self.molecule.push_object(PageObject::Table(table));
        self.selected_atoms.clear();
        self.selected_objects = [id].into_iter().collect();
        Some(id)
    }

    /// Edit `cell` of table `id`, as a double-click on it does: `Limiting?` toggles at once (one
    /// undo step), an entered quantity opens the edit box. Returns whether the molecule changed.
    pub fn edit_table_cell(&mut self, id: u32, cell: crate::reaction::edit::CellRef) -> bool {
        use crate::molecule::reaction_table::{cell_action, edit_cell, CellAction};
        let Some(PageObject::Table(table)) = self.molecule.object_by_id(id) else { return false };
        match cell_action(&self.molecule, table, cell) {
            CellAction::ReadOnly => false,
            CellAction::Toggle => match edit_cell(&self.molecule, table, cell, "") {
                Ok(Some(edited)) if &edited != table => {
                    self.push_undo();
                    if let Some(PageObject::Table(t)) = self.molecule.object_by_id_mut(id) {
                        *t = edited;
                    }
                    true
                }
                _ => false,
            },
            CellAction::Type(text) => {
                self.table_edit = Some(TableCellEdit { id, cell, text, error: None });
                false
            }
        }
    }

    /// Close the table cell box, writing what was typed (one undo step when it changes the
    /// table). A value that does not read is refused: the box stays open with the reason in
    /// [`TableCellEdit::error`]. Returns whether the molecule changed.
    pub fn commit_table_edit(&mut self) -> bool {
        use crate::molecule::reaction_table::edit_cell;
        let Some(edit) = self.table_edit.take() else { return false };
        let Some(PageObject::Table(table)) = self.molecule.object_by_id(edit.id) else { return false };
        match edit_cell(&self.molecule, table, edit.cell, &edit.text) {
            Ok(Some(edited)) if &edited != table => {
                self.push_undo();
                if let Some(PageObject::Table(t)) = self.molecule.object_by_id_mut(edit.id) {
                    *t = edited;
                }
                true
            }
            Ok(_) => false,
            Err(e) => {
                self.table_edit = Some(TableCellEdit { error: Some(e.to_string()), ..edit });
                false
            }
        }
    }

    /// The value cell of table `id` under screen point `screen`, as painted last frame.
    pub fn table_cell_at(&self, id: u32, screen: egui::Pos2) -> Option<crate::reaction::edit::CellRef> {
        self.last_table_cells.get(&id)?.iter().find(|(_, r)| r.contains(screen)).map(|(c, _)| *c)
    }

    /// The page object under screen point `screen`: an arrow near its shaft, a text over the
    /// rect it was painted in last frame. The topmost (last drawn) wins.
    pub fn hit_test_object(&self, screen: egui::Pos2, center: egui::Pos2) -> Option<u32> {
        let view = self.view(center);
        self.molecule.objects.iter().rev().find_map(|o| {
            let hit = match o {
                PageObject::Arrow(a) => {
                    let spread = match a.kind {
                        ArrowKind::Reaction => 0.0,
                        ArrowKind::Equilibrium => {
                            scene::ARROW_SHAFT_SPACING * self.config.style.bond_width * self.zoom * 0.5
                        }
                    };
                    let d = point_to_segment_dist(screen, view.mol_to_screen(a.tail), view.mol_to_screen(a.head));
                    d <= BOND_HIT_THRESHOLD_PX + spread
                }
                PageObject::Text(t) => match self.last_object_rects.get(&t.id) {
                    Some(r) => r.expand(3.0).contains(screen),
                    None => view.mol_to_screen(t.pos).distance(screen) <= NODE_HIT_RADIUS_PX,
                },
                PageObject::Table(t) => match self.last_object_rects.get(&t.id) {
                    Some(r) => r.contains(screen),
                    None => view.mol_to_screen(t.pos).distance(screen) <= NODE_HIT_RADIUS_PX,
                },
            };
            hit.then(|| o.id())
        })
    }

    /// The current camera (zoom, pan) placed on a canvas centered at `center`.
    pub fn view(&self, center: egui::Pos2) -> ViewTransform {
        ViewTransform::new(self.zoom, self.pan, center)
    }

    /// Zoom and pan so the whole drawing — atoms, bonds, ring fills and atom labels — sits
    /// centered on `canvas` (the rect [`ui`](Self::ui) draws into) with [`FitOptions::default`].
    /// Does nothing when the molecule is empty. Labels are laid out with `ctx`'s fonts; before
    /// the first frame, use [`request_fit`](Self::request_fit) instead.
    ///
    /// # Panics
    ///
    /// If `ctx` has never started a frame (egui creates its fonts then). Calling it during the
    /// first frame, inside `run_ui`, is fine.
    pub fn fit_view(&mut self, ctx: &egui::Context, canvas: egui::Rect) {
        self.fit_view_with(ctx, canvas, FitOptions::default());
    }

    /// Fit the view (as [`fit_view`](Self::fit_view)) at the start of the next
    /// [`ui`](Self::ui) call, on the canvas that frame draws into. Usable before any frame
    /// has run and without knowing the canvas size, e.g. right after loading a drawing.
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
    }

    /// [`fit_view`](Self::fit_view) with a chosen margin, zoom range, or zoom kept as is.
    ///
    /// # Panics
    ///
    /// If `ctx` has never started a frame, as for [`fit_view`](Self::fit_view).
    pub fn fit_view_with(&mut self, ctx: &egui::Context, canvas: egui::Rect, opts: FitOptions) {
        self.fit_with_layout(canvas, opts, |job| ctx.fonts_mut(|f| f.layout_job(job)));
    }

    fn fit_with_layout(
        &mut self,
        canvas: egui::Rect,
        opts: FitOptions,
        layout: impl FnMut(egui::text::LayoutJob) -> std::sync::Arc<egui::Galley>,
    ) {
        let style = &self.config.style;
        if let Some(v) = view::fit_transform(&self.molecule, style, canvas, self.zoom, opts, layout) {
            self.zoom = v.zoom;
            self.pan = v.pan;
        }
    }

    /// Zoom by `factor` about the screen point `anchor` (see [`ViewTransform::zoomed_about`]).
    fn zoom_about(&mut self, factor: f32, anchor: egui::Pos2, center: egui::Pos2) {
        let zoomed = self.view(center).zoomed_about(factor, anchor);
        self.zoom = zoomed.zoom;
        self.pan = zoomed.pan;
    }

    pub fn mol_to_screen(&self, pos: [f32; 2], center: egui::Pos2) -> egui::Pos2 {
        self.view(center).mol_to_screen(pos)
    }

    pub fn screen_to_mol(&self, screen: egui::Pos2, center: egui::Pos2) -> [f32; 2] {
        self.view(center).screen_to_mol(screen)
    }

    /// Screen rects of the atom labels painted last frame (atom id → rect). Empty until
    /// the first `ui()` frame. Useful for fitting a view around the drawn structure.
    pub fn label_rects(&self) -> &std::collections::HashMap<u32, egui::Rect> {
        &self.last_label_rects
    }

    pub fn hit_test_atom(&self, screen: egui::Pos2, center: egui::Pos2) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for atom in &self.molecule.atoms {
            // Folded terminal H atoms are invisible; don't let them capture hover/clicks.
            if draw::is_folded_h(&self.molecule, atom.id) { continue; }
            let sp = self.mol_to_screen(atom.pos, center);
            let d = sp.distance(screen);
            // Hit within the base radius, or anywhere on the label text painted last
            // frame (wide labels like "CO2Et" stay clickable over their whole extent).
            let in_label = self
                .last_label_rects
                .get(&atom.id)
                .is_some_and(|r| r.contains(screen));
            if d <= NODE_HIT_RADIUS_PX || in_label {
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
    ///
    /// The angle starts at [`best_new_bond_angle`](Self::best_new_bond_angle) — which only
    /// decides where the *first* bond leaves and knows nothing about the rest of the group
    /// hanging off it — and is then rotated, in 60° steps, until the fragment stops losing
    /// atoms to [`insert_fragment`](crate::Molecule::insert_fragment)'s snap (see
    /// [`fragment_atoms_lost_to_snap`](crate::Molecule::fragment_atoms_lost_to_snap), which
    /// defines what counts as a loss and what is a wanted ring fusion).
    ///
    /// That second step is not cosmetic. `best_new_bond_angle` puts the new bond 120° away
    /// from the one the atom already has, and a three-fold group (SiH₃, CF₃, TMS, TBS) branches
    /// ±120° off *its own* axis — so one branch points straight back down the existing bond and
    /// lands exactly where the neighbour already sits, one bond-length out. The snap then
    /// merged the two and the group quietly arrived short: `c 1 S` drew SiH₂.
    pub fn best_fragment_placement(
        &self,
        atom_id: u32,
        frag: &crate::molecule::fragment::Fragment,
    ) -> (f32, bool) {
        let (angle, flip) = self.preferred_fragment_placement(atom_id, frag);
        if self.molecule.fragment_atoms_lost_to_snap(frag, atom_id, angle, flip) == 0 {
            return (angle, flip);
        }

        // Rotate away from the preferred angle, nearest first, and prefer un-mirroring over
        // rotating: both keep more of the preferred layout than the next step out does. 60° is
        // the step because it is the grid everything else in the editor is drawn on.
        const STEP: f32 = std::f32::consts::FRAC_PI_3;
        let mut best = (angle, flip, usize::MAX);
        for turns in [0.0, 1.0, -1.0, 2.0, -2.0, 3.0] {
            let candidate = normalize_angle(angle + turns * STEP);
            for mirrored in [flip, !flip] {
                let lost =
                    self.molecule.fragment_atoms_lost_to_snap(frag, atom_id, candidate, mirrored);
                if lost == 0 {
                    return (candidate, mirrored);
                }
                if lost < best.2 {
                    best = (candidate, mirrored, lost);
                }
            }
        }
        // Nowhere is clear — a fully enclosed atom. Take the least-bad orientation and let the
        // snap do what it can; there is no room left to draw the group whole.
        (best.0, best.1)
    }

    /// The orientation [`best_fragment_placement`](Self::best_fragment_placement) would like,
    /// before it checks whether the fragment actually fits there: the standard bond angle, plus
    /// the mirroring that keeps the fragment's bulk clear of the atom's existing neighbours.
    fn preferred_fragment_placement(
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

    /// Show the page text box while [`text_edit`](Self::text_edit) is open, and close it on
    /// Enter (commit), Escape (cancel) or a click elsewhere (commit). Returns whether a commit
    /// changed the molecule.
    fn show_text_edit(&mut self, ui: &mut egui::Ui, center: egui::Pos2) -> bool {
        let Some(edit) = self.text_edit.as_ref() else { return false };
        // An edited text that undo or a host removed has nothing left to edit.
        if edit.id.is_some_and(|id| self.molecule.object_by_id(id).is_none()) {
            self.text_edit = None;
            return false;
        }
        let size = self.config.style.label_size * self.zoom;
        let anchor = self.mol_to_screen(edit.pos, center);
        let top_left = anchor - egui::vec2(4.0, size * 1.1);
        let rows = edit.text.lines().count().max(1);
        let Some(edit) = self.text_edit.as_mut() else { return false };
        let area = egui::Area::new(egui::Id::unique("chembuilder_page_text_edit"))
            .fixed_pos(top_left)
            .show(ui.ctx(), |ui| {
                egui::TextEdit::multiline(&mut edit.text)
                    .desired_width((size * 12.0).max(120.0))
                    .desired_rows(rows)
                    .font(egui::FontId::proportional(size))
                    .hint_text("H_2O, 25 \u{b0}C")
                    .return_key(egui::KeyboardShortcut::new(egui::Modifiers::SHIFT, egui::Key::Enter))
                    .show(ui)
                    .response
            });
        let response = area.inner;
        response.request_focus();
        let (enter, escape) = ui.input(|i| {
            (i.key_pressed(egui::Key::Enter) && !i.modifiers.shift, i.key_pressed(egui::Key::Escape))
        });
        let clicked_elsewhere = ui.input(|i| i.pointer.any_pressed())
            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if escape {
            self.text_edit = None;
            response.surrender_focus();
            false
        } else if enter || clicked_elsewhere {
            response.surrender_focus();
            self.commit_text_edit()
        } else {
            false
        }
    }

    /// Show the box of the table cell being typed into, over that cell; Enter commits (unless
    /// the value does not read), Escape cancels, a click elsewhere commits. Returns whether a
    /// commit changed the molecule.
    fn show_table_edit(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(edit) = self.table_edit.as_ref() else { return false };
        let cell_rect = self.last_table_cells.get(&edit.id).and_then(|cells| cells.iter().find(|(c, _)| *c == edit.cell)).map(|(_, r)| *r);
        let (Some(rect), true) = (cell_rect, self.molecule.object_by_id(edit.id).is_some()) else {
            self.table_edit = None;
            return false;
        };
        let size = (rect.height() * 0.6).max(10.0);
        let Some(edit) = self.table_edit.as_mut() else { return false };
        let area = egui::Area::new(egui::Id::unique("chembuilder_table_cell_edit"))
            .fixed_pos(rect.min)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                let response = egui::TextEdit::singleline(&mut edit.text)
                    .desired_width(rect.width().max(80.0))
                    .font(egui::FontId::proportional(size))
                    .show(ui)
                    .response;
                if let Some(error) = &edit.error {
                    ui.colored_label(egui::Color32::from_rgb(200, 40, 40), error);
                }
                response
            });
        let response = area.inner;
        response.request_focus();
        let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
        let clicked_elsewhere = ui.input(|i| i.pointer.any_pressed())
            && ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| !area.response.rect.contains(p));
        if escape {
            self.table_edit = None;
            response.surrender_focus();
            false
        } else if enter || clicked_elsewhere {
            let changed = self.commit_table_edit();
            if clicked_elsewhere {
                // A click away never leaves a refused box behind.
                self.table_edit = None;
            }
            if self.table_edit.is_none() {
                response.surrender_focus();
            }
            changed
        } else {
            false
        }
    }

    /// Main entry point called from the app each frame. Returns true if molecule was modified.
    ///
    /// A host that wants the canvas [`Response`](egui::Response) — to attach a context menu,
    /// say — calls [`show`](Self::show) instead.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> bool {
        self.show(ui).modified
    }

    /// [`ui`](Self::ui), also returning the canvas response.
    pub fn show(&mut self, ui: &mut egui::Ui) -> EditorResponse {
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::click_and_drag(),
        );
        let center = rect.center();
        let painter = ui.painter_at(rect);

        // Ctrl+0: fit the whole drawing to the canvas. Read here, ahead of the request below, so
        // the fit lands in this very frame. Not while a text field (the label editor, or a host
        // field such as the fragment-name input) takes the keyboard. The atom and bond drawing
        // shortcuts skip Ctrl-modified keys, so this never also fires the plain `0` shortcut.
        let text_input = self.label_edit.is_some()
            || self.text_edit.is_some()
            || self.table_edit.is_some()
            || ui.ctx().egui_wants_keyboard_input();
        if !text_input && ui.input(|i| i.key_pressed(egui::Key::Num0) && i.modifiers.ctrl) {
            self.request_fit();
        }

        // A fit asked for by `request_fit`: this frame's canvas, laid out with the same fonts the
        // draw pass below uses, and before hover/hit testing so the whole frame sees one view.
        if std::mem::take(&mut self.fit_requested) {
            self.fit_with_layout(rect, FitOptions::default(), |job| painter.layout_job(job));
        }

        painter.rect_filled(rect, 0.0, egui::Color32::WHITE);

        // Update hover state and track mouse in mol coords.
        //
        // `hover_pos()` is None whenever no pointer is over the widget — which is every frame
        // of keyboard-only use. Substituting a position for that absence (this used to be
        // `unwrap_or(Pos2::ZERO)`) makes the editor behave as though the cursor were parked at
        // the screen origin: if the canvas contains that point, it hovers whatever is drawn
        // there and keys get routed to that atom instead of placing new ones.
        let mouse_pos = response.hover_pos().filter(|p| rect.contains(*p));
        self.pointer_active = mouse_pos.is_some();
        if let Some(mouse_pos) = mouse_pos {
            self.last_mouse_mol = self.screen_to_mol(mouse_pos, center);
            self.hovered_atom = self.hit_test_atom(mouse_pos, center);
            // An atom beats a page object, and a page object a bond (a label or condition
            // text sitting over a bond is what the pointer is meant to be on).
            self.hovered_object = if self.hovered_atom.is_none() {
                self.hit_test_object(mouse_pos, center)
            } else {
                None
            };
            self.hovered_bond = if self.hovered_atom.is_none() && self.hovered_object.is_none() {
                self.hit_test_bond(mouse_pos, center)
            } else {
                None
            };
        } else {
            self.hovered_atom = None;
            self.hovered_bond = None;
            self.hovered_object = None;
        }
        // Selected objects that undo, redo or deletion removed are no longer selected.
        if !self.selected_objects.is_empty() {
            let mol = &self.molecule;
            self.selected_objects.retain(|id| mol.object_by_id(*id).is_some());
        }

        // A keyboard bond target is only meaningful while that bond exists, and undo, redo,
        // deletion and label expansion can all remove it. Validating centrally here beats
        // relying on every mutation site to remember — the same call-site discipline that
        // `push_undo` already depends on, and occasionally loses.
        if let Some(bid) = self.hotspot_bond {
            if !self.molecule.bonds.iter().any(|b| b.id == bid) {
                self.hotspot_bond = None;
            }
        }
        if let Some(bid) = self.context_bond {
            if !self.molecule.bonds.iter().any(|b| b.id == bid) {
                self.context_bond = None;
            }
        }

        // A right-click remembers the bond under the pointer (or none) for the host's context
        // menu: once the menu opens the pointer is over it, and the hover is gone.
        if response.secondary_clicked() {
            self.context_bond = self.hovered_bond;
        }

        // Zoom via scroll wheel. `smooth_scroll_delta` is egui's *smoothed* delta — one wheel
        // notch is spread across several frames, not delivered as a single non-zero value —
        // so the factor must scale with the delta's magnitude. A fixed factor applied on every
        // frame the (smoothed, still-decaying) delta happens to be nonzero compounds
        // exponentially over those frames, turning one notch into a multi-fold zoom jump.
        if let Some(mouse_pos) = mouse_pos {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom_about((scroll * 0.002).exp(), mouse_pos, center);
            }
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if (zoom_delta - 1.0).abs() > f32::EPSILON {
                self.zoom_about(zoom_delta, mouse_pos, center);
            }
        }

        // Ctrl+1/2/3: switch tool. Handled here, above the tool dispatch, because the Bond
        // tool discards every Ctrl-modified key and there was otherwise no way to leave a
        // tool without clicking the toolbar.
        for (key, tool) in [
            (egui::Key::Num1, Tool::Select),
            (egui::Key::Num2, Tool::Bond),
            (egui::Key::Num3, Tool::Eraser),
        ] {
            if ui.input(|i| i.key_pressed(key) && i.modifiers.ctrl) {
                self.tool = tool;
                // Targets and in-flight gestures belong to the tool that was active.
                self.hotspot_bond = None;
                self.bond_start = None;
                self.lasso_path.clear();
                self.drag_origin_screen = None;
                self.preview_end_screen = None;
            }
        }

        // Ctrl+Z: undo. Ctrl+Y or Ctrl+Shift+Z: redo.
        let undo_pressed = ui.input(|i| i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && !i.modifiers.shift);
        // The text box keeps Ctrl+Z / Ctrl+Y for its own typing.
        let did_undo = undo_pressed && self.text_edit.is_none() && self.table_edit.is_none() && self.undo();
        let redo_pressed = ui.input(|i| {
            (i.key_pressed(egui::Key::Y) && i.modifiers.ctrl)
                || (i.key_pressed(egui::Key::Z) && i.modifiers.ctrl && i.modifiers.shift)
        });
        let did_redo = redo_pressed && self.text_edit.is_none() && self.table_edit.is_none() && self.redo();

        // Copy/paste only act on the canvas when no text field has focus.
        let editing_text = ui.ctx().egui_wants_keyboard_input();

        // Copy (Ctrl/Cmd+C): egui converts the shortcut into Event::Copy/Cut — `key_pressed(C)`
        // does NOT fire — so we listen for those events (with a raw-key fallback). On Windows
        // we publish text + CDX (+ image + Embed Source); elsewhere just text.
        let do_copy = !editing_text && ui.input(|i| {
            i.events.iter().any(|e| matches!(e, egui::Event::Copy | egui::Event::Cut))
                || (i.modifiers.command && i.key_pressed(egui::Key::C))
        });
        if do_copy {
            self.copy(ui.ctx());
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
        // Font size scales with zoom so it tracks the panzoom transform (zoom == 1.0 = baseline).
        let label_size = self.config.style.label_size * self.zoom;
        let label_state: Option<(u32, egui::Pos2)> = self.label_edit.as_ref().and_then(|(atom_id, _)| {
            let pos = self.molecule.atom_by_id(*atom_id)?.pos;
            Some((*atom_id, self.mol_to_screen(pos, center)))
        });

        if let (Some((edit_id, screen_pos)), Some((_, text))) =
            (label_state, self.label_edit.as_mut())
        {
            let area_resp = egui::Area::new(egui::Id::unique("label_edit_area"))
                .fixed_pos(screen_pos + egui::vec2(12.0, -12.0))
                .show(ui.ctx(), |ui| {
                    let mut output = egui::TextEdit::singleline(text)
                        .desired_width(80.0)
                        .font(egui::FontId::proportional(label_size))
                        .show(ui);
                    // Select the pre-filled element text (e.g. "C") on the first frame the box
                    // is open, so typing replaces it outright instead of appending onto it —
                    // `has_focus()` is false only on that first frame, before `request_focus()`
                    // below takes effect.
                    if !output.response.has_focus() {
                        let range = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(text.chars().count()),
                        );
                        output.state.cursor.set_char_range(Some(range));
                        output.state.store(ui.ctx(), output.response.id);
                    }
                    output.response
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

            // Hand keyboard focus back to the canvas the moment the editor closes.
            //
            // `egui_wants_keyboard_input` reports the focus still held at the *end* of a frame,
            // and a `TextEdit` that simply stops being shown keeps its focus until egui reaps
            // it. So without this the suppression below stayed on for one frame after the box
            // was gone, and the first key pressed after committing a label was silently eaten
            // — invisible at 60 Hz, perfectly reproducible in a scripted key sequence.
            if label_confirmed {
                area_resp.inner.surrender_focus();
            }
        } else if self.label_edit.is_some() {
            label_confirmed = true; // atom no longer exists
        }
        if label_confirmed { self.label_edit = None; }
        let label_editing = self.label_edit.is_some();

        // ── Page text box ────────────────────────────────────────────────────
        let text_committed = self.show_text_edit(ui, center) | self.show_table_edit(ui);
        let text_editing = self.text_edit.is_some() || self.table_edit.is_some();

        // Suppress canvas key/mouse handling whenever a text field (our label editor or a
        // toolbar field like the fragment-name input) has keyboard focus, so typed characters
        // don't leak into atom/bond shortcuts; also while a background cleanup is rewriting
        // positions (editing would fight the worker).
        let canvas_input = !(label_editing || text_editing || cleaning || ui.ctx().egui_wants_keyboard_input());
        let tool_modified = if canvas_input {
            match self.tool.clone() {
                Tool::Bond   => interact::process_bond_tool(self, &response, center, ui),
                Tool::Select => interact::process_select_tool(self, &response, center, ui),
                Tool::Eraser => interact::process_eraser_tool(self, &response, center),
                Tool::Arrow(kind) => interact::process_arrow_tool(self, &response, center, ui, kind),
                Tool::Text => interact::process_text_tool(self, &response, center, ui),
            }
        } else {
            false
        };

        // Middle-button drag pans the camera, whatever the tool. It runs after the tool
        // handler so the tool still sees this frame's pre-pan view, as the Bond tool did when
        // the pan lived inside it.
        if canvas_input && ui.input(|i| i.pointer.middle_down()) {
            self.pan += ui.input(|i| i.pointer.delta());
        }
        let modified = cleanup_toggle | label_confirmed | text_committed | tool_modified | cleaning | did_undo | did_redo;

        // Lay all labels out first: bonds trim at the label rects, backgrounds mask
        // crossing bonds, and labels paint the cached galleys. Keep the rects for next
        // frame's hit testing.
        let label_layouts = draw::compute_label_layouts(self, &painter, center);
        self.last_label_rects = label_layouts.iter().map(|(id, l)| (*id, l.rect)).collect();
        // Ring fills sit behind the bonds (ChemDraw-style shading). Same shared fill polygons the
        // image exporter draws, so the canvas and the copied picture look identical.
        draw::draw_ring_fills(self, &painter, center);
        draw::draw_bonds(self, &painter, center, &label_layouts);
        draw::draw_atom_backgrounds(self, &painter, center, &label_layouts);
        draw::draw_atom_labels(self, &painter, center, &label_layouts);
        let painted = draw::draw_page_objects(self, &painter, center);
        self.last_object_rects = painted.rects;
        self.last_table_cells = painted.cells;
        draw::draw_overlays(self, &painter, center);

        EditorResponse { response, modified }
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
    fn copy_paste_preserves_ring_fills() {
        let mut e = ChemStructEditor::default();
        // A triangle ring, filled green.
        let a = e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = e.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        let c = e.molecule.add_atom("C".to_string(), [0.75, 1.3], 0);
        e.molecule.add_bond(a, b, BondOrder::Single);
        e.molecule.add_bond(b, c, BondOrder::Single);
        e.molecule.add_bond(c, a, BondOrder::Single);
        e.molecule.set_ring_fill(vec![a, b, c], [0, 200, 0, 128]);

        let text = e.copy_to_string().expect("copy");
        // A fresh editor receives the paste; the fill's color survives the round-trip.
        let mut e2 = ChemStructEditor::default();
        assert!(e2.paste_from_string(&text));
        assert_eq!(e2.molecule.ring_fills.len(), 1, "the ring fill was pasted");
        assert_eq!(e2.molecule.ring_fills[0].color, [0, 200, 0, 128], "with its color");
        // And it references three real, freshly-created atoms.
        assert_eq!(e2.molecule.ring_fills[0].atoms.len(), 3);
        for id in &e2.molecule.ring_fills[0].atoms {
            assert!(e2.molecule.atom_by_id(*id).is_some());
        }
    }

    #[test]
    fn fill_ring_at_colors_the_smallest_ring() {
        let mut e = ChemStructEditor::default();
        let ids: Vec<u32> = (0..6)
            .map(|k| {
                let a = (k as f32) * std::f32::consts::TAU / 6.0;
                e.molecule.add_atom("C".to_string(), [a.cos(), a.sin()], 0)
            })
            .collect();
        for k in 0..6 {
            e.molecule.add_bond(ids[k], ids[(k + 1) % 6], BondOrder::Single);
        }
        assert!(e.fill_ring_at(ids[0], [255, 0, 0, 255]));
        assert_eq!(e.molecule.ring_fills.len(), 1);
        assert_eq!(e.molecule.ring_fills[0].atoms.len(), 6);
        // Undo (the fill recorded a snapshot) removes it again.
        assert!(e.undo());
        assert!(e.molecule.ring_fills.is_empty());
    }

    // Benzene (6-ring) with a 4-ring fused onto its b0—b1 edge, plus the two square-only atoms.
    // Returns (editor, benzene_ids, square_only_s0, square_only_s1).
    fn fused_benzene_and_square() -> (ChemStructEditor, Vec<u32>, u32, u32) {
        use std::f32::consts::TAU;
        let mut e = ChemStructEditor::default();
        let b: Vec<u32> = (0..6)
            .map(|k| {
                let a = (k as f32) * TAU / 6.0;
                e.molecule.add_atom("C".to_string(), [a.cos(), a.sin()], 0)
            })
            .collect();
        for k in 0..6 {
            e.molecule.add_bond(b[k], b[(k + 1) % 6], BondOrder::Single);
        }
        // Square: b1—s0—s1—b0, sharing the b0—b1 edge with benzene.
        let s0 = e.molecule.add_atom("C".to_string(), [2.0, 0.5], 0);
        let s1 = e.molecule.add_atom("C".to_string(), [2.0, -0.5], 0);
        e.molecule.add_bond(b[1], s0, BondOrder::Single);
        e.molecule.add_bond(s0, s1, BondOrder::Single);
        e.molecule.add_bond(s1, b[0], BondOrder::Single);
        (e, b, s0, s1)
    }

    // Every consecutive pair (including the wrap) of a filled ring must be a real bond, or the
    // fill polygon self-intersects. Also returns the fill's atoms as an unordered set.
    fn assert_ring_is_a_cycle(e: &ChemStructEditor) -> std::collections::HashSet<u32> {
        let ring = &e.molecule.ring_fills[0].atoms;
        for w in 0..ring.len() {
            let (u, v) = (ring[w], ring[(w + 1) % ring.len()]);
            assert!(
                e.molecule.neighbor_atom_ids(u).contains(&v),
                "polygon edge {u}->{v} must be a real bond (cyclic order)"
            );
        }
        ring.iter().copied().collect()
    }

    #[test]
    fn fill_selected_ring_fills_the_selected_ring_on_a_fused_bicyclic() {
        // Bug repro: selecting the square's four atoms must fill the square, even when the
        // hotspot sits on a benzene-only atom (the cursor must not override the selection).
        let (mut e, b, s0, s1) = fused_benzene_and_square();
        e.selected_atoms = [b[0], b[1], s0, s1].into_iter().collect();
        e.hotspot_atom = Some(b[3]); // on benzene, off the square

        assert!(e.fill_selected_ring([255, 0, 0, 255]));
        assert_eq!(e.molecule.ring_fills.len(), 1);
        let filled = assert_ring_is_a_cycle(&e);
        assert_eq!(
            filled,
            [b[0], b[1], s0, s1].into_iter().collect(),
            "the four selected atoms — the square — are filled, not benzene"
        );
    }

    #[test]
    fn fill_selected_ring_fills_benzene_when_benzene_is_selected() {
        // The mirror case: selecting benzene's six atoms fills the hexagon, even with the
        // hotspot parked on a square-only atom.
        let (mut e, b, s0, _s1) = fused_benzene_and_square();
        e.selected_atoms = b.iter().copied().collect();
        e.hotspot_atom = Some(s0); // on the square, off benzene

        assert!(e.fill_selected_ring([0, 0, 255, 255]));
        assert_eq!(e.molecule.ring_fills.len(), 1);
        let filled = assert_ring_is_a_cycle(&e);
        assert_eq!(
            filled,
            b.iter().copied().collect(),
            "the six selected atoms — benzene — are filled, not the square"
        );
    }

    #[test]
    fn fill_selected_ring_fills_every_ring_of_a_fused_selection() {
        // Bug repro: selecting both fused rings whole must fill both, not just the smaller one.
        let (mut e, b, s0, s1) = fused_benzene_and_square();
        e.selected_atoms = b.iter().copied().chain([s0, s1]).collect();

        assert!(e.fill_selected_ring([255, 0, 0, 255]));
        let mut sizes: Vec<usize> = e.molecule.ring_fills.iter().map(|rf| rf.atoms.len()).collect();
        sizes.sort_unstable();
        assert_eq!(sizes, vec![4, 6], "square and hexagon filled, no envelope ring");
        // One undo step reverts all of them.
        assert!(e.undo());
        assert!(e.molecule.ring_fills.is_empty());
    }

    #[test]
    fn fill_selected_ring_fills_separate_selected_rings() {
        // Two disconnected hexagons selected together: both are filled.
        use std::f32::consts::TAU;
        let mut e = ChemStructEditor::default();
        for cx in [0.0f32, 5.0] {
            let ids: Vec<u32> = (0..6)
                .map(|k| {
                    let a = (k as f32) * TAU / 6.0;
                    e.molecule.add_atom("C".to_string(), [cx + a.cos(), a.sin()], 0)
                })
                .collect();
            for k in 0..6 {
                e.molecule.add_bond(ids[k], ids[(k + 1) % 6], BondOrder::Single);
            }
            e.selected_atoms.extend(ids);
        }
        assert!(e.fill_selected_ring([0, 0, 255, 255]));
        assert_eq!(e.molecule.ring_fills.len(), 2);
    }

    #[test]
    fn fill_selected_ring_fills_a_ring_whose_atoms_are_all_shared() {
        // Triphenylene-like: a central hexagon c0..c5 with a hexagon fused on c0—c1, c2—c3 and
        // c4—c5. Every central atom also lies on an outer ring, yet the central ring is filled.
        use std::f32::consts::TAU;
        let mut e = ChemStructEditor::default();
        let c: Vec<u32> = (0..6)
            .map(|k| {
                let a = (k as f32) * TAU / 6.0;
                e.molecule.add_atom("C".to_string(), [a.cos(), a.sin()], 0)
            })
            .collect();
        for k in 0..6 {
            e.molecule.add_bond(c[k], c[(k + 1) % 6], BondOrder::Single);
        }
        let mut all = c.clone();
        for k in [0usize, 2, 4] {
            // Outer ring: c[k]—o0—o1—o2—o3—c[k+1].
            let o: Vec<u32> = (0..4)
                .map(|j| e.molecule.add_atom("C".to_string(), [10.0 + k as f32, j as f32], 0))
                .collect();
            e.molecule.add_bond(c[k], o[0], BondOrder::Single);
            for j in 0..3 {
                e.molecule.add_bond(o[j], o[j + 1], BondOrder::Single);
            }
            e.molecule.add_bond(o[3], c[k + 1], BondOrder::Single);
            all.extend(o);
        }
        e.selected_atoms = all.into_iter().collect();
        assert!(e.fill_selected_ring([0, 200, 0, 255]));
        assert_eq!(e.molecule.ring_fills.len(), 4, "three outer rings plus the central one");
        let central: std::collections::HashSet<u32> = c.iter().copied().collect();
        assert!(e
            .molecule
            .ring_fills
            .iter()
            .any(|rf| rf.atoms.iter().copied().collect::<std::collections::HashSet<_>>() == central));
    }

    #[test]
    fn fill_selected_ring_falls_back_to_seed_when_selection_forms_no_ring() {
        // A selection that encloses no ring of its own falls back to the smallest ring through
        // the lowest selected atom (still preferring the selection over the hotspot).
        let (mut e, b, s0, _s1) = fused_benzene_and_square();
        // b[0] and s0 do not form a cycle by themselves; b[0] is on both rings, and its
        // smallest ring is the 4-membered square.
        e.selected_atoms = [b[0], s0].into_iter().collect();
        e.hotspot_atom = Some(b[3]);
        assert!(e.fill_selected_ring([0, 200, 0, 255]));
        assert_eq!(e.molecule.ring_fills[0].atoms.len(), 4, "fell back to b0's smallest ring");
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

#[cfg(test)]
mod undo_redo_tests {
    use super::*;

    #[test]
    fn undo_then_redo_restores_the_undone_state() {
        let mut e = ChemStructEditor::default();
        e.push_undo(); // snapshot of the empty molecule
        e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);

        assert!(e.undo(), "should undo back to the empty molecule");
        assert!(e.molecule.atoms.is_empty());

        assert!(e.redo(), "should redo back to the one-atom state");
        assert_eq!(e.molecule.atoms.len(), 1);

        assert!(!e.redo(), "nothing left to redo");
    }

    #[test]
    fn a_new_edit_clears_the_redo_stack() {
        let mut e = ChemStructEditor::default();
        e.push_undo();
        e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        assert!(e.undo());
        assert_eq!(e.redo_stack.len(), 1);

        // A fresh edit (push_undo + mutation) should invalidate the old redo branch.
        e.push_undo();
        e.molecule.add_atom("O".to_string(), [1.5, 0.0], 0);
        assert!(e.redo_stack.is_empty(), "starting a new edit after undo must clear redo");
        assert!(!e.redo());
    }

    #[test]
    fn ctrl_z_undoes_a_click_placed_atom_through_the_full_ui() {
        use crate::testkit::{run_frame, run_frame_with_modifiers};

        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let p1 = egui::pos2(300.0, 200.0);
        let p2 = egui::pos2(450.0, 280.0);

        for p in [p1, p2] {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p)]);
            run_frame(&ctx, screen_rect, &mut editor, vec![
                egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            ]);
            run_frame(&ctx, screen_rect, &mut editor, vec![
                egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
            ]);
        }
        assert_eq!(editor.molecule.atoms.len(), 2, "two clicks on empty canvas should place two atoms");

        let (events, modifiers) = crate::testkit::chord("Ctrl+Z");
        run_frame_with_modifiers(&ctx, screen_rect, &mut editor, events, modifiers);
        assert_eq!(editor.molecule.atoms.len(), 1, "Ctrl+Z should undo the second atom placement");
    }
}

/// Characterization tests for the editor's view (camera): the mol <-> screen mapping, scroll
/// zoom and middle-button pan. They pin today's behavior so the view code can be consolidated
/// without changing what the user sees.
#[cfg(test)]
mod view_tests {
    use super::*;
    use crate::testkit::run_frame;

    /// Several non-trivial cameras, including the zoom clamp bounds.
    fn cameras() -> Vec<(f32, egui::Vec2)> {
        vec![
            (1.0, egui::Vec2::ZERO),
            (0.1, egui::vec2(-40.0, 12.5)),
            (2.75, egui::vec2(130.0, -77.0)),
            (20.0, egui::vec2(3.0, 9000.0)),
        ]
    }

    fn assert_close(a: egui::Pos2, b: egui::Pos2, what: &str) {
        assert!(
            (a.x - b.x).abs() <= 1e-3 && (a.y - b.y).abs() <= 1e-3,
            "{what}: {a:?} != {b:?}"
        );
    }

    #[test]
    fn mol_to_screen_and_screen_to_mol_round_trip() {
        let center = egui::pos2(321.0, 245.0);
        for (zoom, pan) in cameras() {
            let e = ChemStructEditor { zoom, pan, ..Default::default() };
            for p in [[0.0, 0.0], [1.5, -2.25], [-7.0, 3.0]] {
                let back = e.screen_to_mol(e.mol_to_screen(p, center), center);
                assert!(
                    (back[0] - p[0]).abs() <= 1e-4 && (back[1] - p[1]).abs() <= 1e-4,
                    "zoom {zoom} pan {pan:?}: {p:?} round-tripped to {back:?}"
                );
            }
            for s in [center, egui::pos2(0.0, 0.0), egui::pos2(800.0, 13.0)] {
                let back = e.mol_to_screen(e.screen_to_mol(s, center), center);
                assert_close(back, s, &format!("zoom {zoom} pan {pan:?}: screen round trip"));
            }
        }
    }

    #[test]
    fn mol_to_screen_places_the_origin_at_center_plus_pan_and_scales_by_zoom() {
        let center = egui::pos2(321.0, 245.0);
        let e = ChemStructEditor { zoom: 2.0, pan: egui::vec2(10.0, -5.0), ..Default::default() };
        assert_close(e.mol_to_screen([0.0, 0.0], center), center + e.pan, "origin");
        assert_close(
            e.mol_to_screen([1.0, 1.0], center),
            center + e.pan + egui::vec2(2.0 * SCALE_FACTOR, 2.0 * SCALE_FACTOR),
            "unit offset",
        );
    }

    #[test]
    fn draw_transform_agrees_with_mol_to_screen() {
        let center = egui::pos2(321.0, 245.0);
        for (zoom, pan) in cameras() {
            let e = ChemStructEditor { zoom, pan, ..Default::default() };
            let t = draw::editor_transform_for_tests(&e, center);
            assert_eq!(t.visual_scale, zoom, "visual sizes scale by zoom alone");
            for p in [[0.0, 0.0], [1.5, -2.25], [-7.0, 3.0]] {
                assert_close(
                    t.apply(p),
                    e.mol_to_screen(p, center),
                    &format!("zoom {zoom} pan {pan:?} point {p:?}"),
                );
            }
        }
    }

    #[test]
    fn scroll_zoom_keeps_the_point_under_the_cursor_fixed() {
        let ctx = egui::Context::default();
        let mut editor =
            ChemStructEditor { zoom: 1.3, pan: egui::vec2(25.0, -15.0), ..Default::default() };
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        // Off-center, so a zoom about the canvas center (rather than the cursor) would move it.
        let cursor = egui::pos2(500.0, 120.0);

        // Two frames: the canvas only reports hover once egui has laid it out a frame earlier.
        for _ in 0..2 {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(cursor)]);
        }
        assert!(editor.pointer_active, "the cursor must be over the canvas");
        // `last_mouse_mol` is `screen_to_mol(cursor, center)` taken at the start of each frame.
        let before = editor.last_mouse_mol;
        let zoom_before = editor.zoom;

        for (i, dy) in [120.0_f32, -40.0].into_iter().enumerate() {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Default::default(),
            }]);
            // egui spreads one wheel event over several frames; let it play out.
            for _ in 0..30 {
                run_frame(&ctx, screen_rect, &mut editor, vec![]);
            }
            // One more frame so `last_mouse_mol` is re-sampled under the final camera.
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(cursor)]);
            let after = editor.last_mouse_mol;
            assert!(
                (after[0] - before[0]).abs() <= 1e-3 && (after[1] - before[1]).abs() <= 1e-3,
                "scroll #{i}: the molecule point under the cursor moved from {before:?} to {after:?}"
            );
        }
        assert!(
            (editor.zoom - zoom_before).abs() > 1e-3,
            "the scroll must actually have zoomed (zoom stayed {})",
            editor.zoom
        );
    }

    #[test]
    fn pinch_zoom_keeps_the_point_under_the_cursor_fixed_and_is_clamped() {
        let ctx = egui::Context::default();
        let mut editor =
            ChemStructEditor { zoom: 1.3, pan: egui::vec2(25.0, -15.0), ..Default::default() };
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let cursor = egui::pos2(500.0, 120.0);
        for _ in 0..2 {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(cursor)]);
        }
        assert!(editor.pointer_active, "the cursor must be over the canvas");
        let before = editor.last_mouse_mol;

        for (factor, zoom) in [(1.5_f32, 1.3_f32 * 1.5), (0.5, 1.3 * 1.5 * 0.5)] {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::Zoom(factor)]);
            assert!((editor.zoom - zoom).abs() <= 1e-5, "pinch {factor}: zoom {}", editor.zoom);
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(cursor)]);
            let after = editor.last_mouse_mol;
            assert!(
                (after[0] - before[0]).abs() <= 1e-3 && (after[1] - before[1]).abs() <= 1e-3,
                "pinch {factor}: the molecule point under the cursor moved from {before:?} to {after:?}"
            );
        }
        for (factor, bound) in [(1.0e4_f32, 20.0_f32), (1.0e-4, 0.1)] {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::Zoom(factor)]);
            assert_eq!(editor.zoom, bound, "pinch {factor} should end at the clamp bound");
        }
    }

    #[test]
    fn select_drag_moves_the_atom_by_the_pointer_delta_in_molecule_units() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor {
            tool: Tool::Select,
            zoom: 1.7,
            pan: egui::vec2(-20.0, 30.0),
            ..Default::default()
        };
        let id = editor.molecule.add_atom("C".to_string(), [0.5, -0.25], 0);
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let center = screen_rect.center();
        let p0 = editor.mol_to_screen([0.5, -0.25], center);

        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p0)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerButton {
            pos: p0,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        // The first 5 px step stays under egui's drag threshold, so it is not a drag yet and
        // does not move the atom; the drag (and the atom) follows from the next step on.
        let step = egui::vec2(4.0, -3.0);
        let mut p = p0 + step;
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p)]);
        let drag_from = p;
        for _ in 0..5 {
            p += step;
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p)]);
        }
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerButton {
            pos: p,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
        let moved = p - drag_from;
        let s = 1.7 * SCALE_FACTOR;
        let pos = editor.molecule.atom_by_id(id).unwrap().pos;
        assert!(
            (pos[0] - (0.5 + moved.x / s)).abs() <= 1e-4
                && (pos[1] - (-0.25 + moved.y / s)).abs() <= 1e-4,
            "atom ended at {pos:?} after a {moved:?} drag"
        );
    }

    #[test]
    fn scroll_up_zooms_in_and_zoom_is_clamped_to_0_1_through_20() {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let cursor = egui::pos2(320.0, 240.0);
        for (dy, bound) in [(1.0e5_f32, 20.0_f32), (-1.0e5, 0.1)] {
            let mut editor = ChemStructEditor::default();
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(cursor)]);
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, dy),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: Default::default(),
            }]);
            for _ in 0..60 {
                run_frame(&ctx, screen_rect, &mut editor, vec![]);
            }
            assert_eq!(editor.zoom, bound, "scroll delta {dy} should end at the clamp bound");
        }
    }

    #[test]
    fn middle_drag_pans_by_the_pointer_delta_in_the_bond_tool() {
        let editor = ChemStructEditor::default();
        assert_eq!(editor.tool, Tool::Bond);
        assert_middle_drag_pans(editor);
    }

    #[test]
    fn middle_drag_pans_by_the_pointer_delta_in_the_select_tool() {
        assert_middle_drag_pans(ChemStructEditor { tool: Tool::Select, ..Default::default() });
    }

    #[test]
    fn middle_drag_pans_by_the_pointer_delta_in_the_eraser_tool() {
        assert_middle_drag_pans(ChemStructEditor { tool: Tool::Eraser, ..Default::default() });
    }

    /// A focused text field keeps the canvas from reacting to input, the camera included.
    #[test]
    fn middle_drag_does_not_pan_while_a_text_field_has_focus() {
        for tool in [Tool::Bond, Tool::Select, Tool::Eraser] {
            let ctx = egui::Context::default();
            let mut editor = ChemStructEditor { tool: tool.clone(), ..Default::default() };
            let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
            let field = egui::Id::unique("some_text_field");
            let p0 = egui::pos2(300.0, 200.0);
            let p1 = egui::pos2(340.0, 170.0);
            let frame = |editor: &mut ChemStructEditor, events: Vec<egui::Event>| {
                let raw = egui::RawInput {
                    screen_rect: Some(screen_rect),
                    events,
                    focused: true,
                    ..Default::default()
                };
                let _ = crate::testkit::run_ui(&ctx, raw, |ui| {
                    ui.memory_mut(|m| m.request_focus(field));
                    editor.ui(ui);
                });
            };
            frame(&mut editor, vec![egui::Event::PointerMoved(p0)]);
            frame(&mut editor, vec![egui::Event::PointerButton {
                pos: p0,
                button: egui::PointerButton::Middle,
                pressed: true,
                modifiers: Default::default(),
            }]);
            let pan_start = editor.pan;
            frame(&mut editor, vec![egui::Event::PointerMoved(p1)]);
            assert_eq!(editor.pan, pan_start, "{tool:?}: no pan while a text field has focus");
        }
    }

    fn assert_middle_drag_pans(mut editor: ChemStructEditor) {
        let ctx = egui::Context::default();
        let tool = editor.tool.clone();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let p0 = egui::pos2(300.0, 200.0);
        let p1 = egui::pos2(340.0, 170.0);
        let p2 = egui::pos2(360.0, 230.0);

        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p0)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerButton {
            pos: p0,
            button: egui::PointerButton::Middle,
            pressed: true,
            modifiers: Default::default(),
        }]);
        let pan_start = editor.pan;
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p1)]);
        assert_eq!(editor.pan, pan_start + (p1 - p0), "{tool:?}: pan follows the middle-drag delta");
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p2)]);
        assert_eq!(editor.pan, pan_start + (p2 - p0), "{tool:?}: pan accumulates over the drag");
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerButton {
            pos: p2,
            button: egui::PointerButton::Middle,
            pressed: false,
            modifiers: Default::default(),
        }]);
        let pan_end = editor.pan;
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p0)]);
        assert_eq!(editor.pan, pan_end, "{tool:?}: moving after the release does not pan");
    }

    /// Middle-drag along `path` (press at the first point, release at the last), calling
    /// `during` after every frame the button is held. Returns whether any frame's `ui()`
    /// reported a modification.
    fn middle_drag_along(
        editor: &mut ChemStructEditor,
        path: &[egui::Pos2],
        mut during: impl FnMut(&ChemStructEditor),
    ) -> bool {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let frame = |editor: &mut ChemStructEditor, events: Vec<egui::Event>| {
            let raw = egui::RawInput {
                screen_rect: Some(screen_rect),
                events,
                focused: true,
                ..Default::default()
            };
            let mut modified = false;
            let _ = crate::testkit::run_ui(&ctx, raw, |ui| modified = editor.ui(ui));
            modified
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Middle,
            pressed,
            modifiers: Default::default(),
        };
        let (first, last) = (path[0], path[path.len() - 1]);
        let mut modified = frame(editor, vec![egui::Event::PointerMoved(first)]);
        modified |= frame(editor, vec![button(first, true)]);
        during(editor);
        for &p in &path[1..] {
            modified |= frame(editor, vec![egui::Event::PointerMoved(p)]);
            during(editor);
        }
        modified |= frame(editor, vec![button(last, false)]);
        modified
    }

    /// The canvas center of the 640x480 test screen, where mol (0, 0) sits at the default view.
    const MID: egui::Pos2 = egui::pos2(320.0, 240.0);

    #[test]
    fn select_middle_drag_from_an_atom_only_pans() {
        let mut editor = ChemStructEditor { tool: Tool::Select, ..Default::default() };
        let atom = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let undo_len = editor.undo_stack.len();
        let pan_start = editor.pan;

        // Small first steps so the drag starts while the pointer is still over the atom (the
        // Select tool picks the dragged atom from the hover at drag start).
        let path = [
            MID,
            MID + egui::vec2(4.0, 2.0),
            MID + egui::vec2(8.0, 4.0),
            MID + egui::vec2(60.0, 30.0),
            MID + egui::vec2(100.0, 50.0),
        ];
        let modified = middle_drag_along(&mut editor, &path, |_| {});

        assert_ne!(editor.pan, pan_start, "the middle drag pans");
        assert_eq!(editor.molecule.atom_by_id(atom).unwrap().pos, [0.0, 0.0], "atom not moved");
        assert!(editor.selected_atoms.is_empty(), "selection unchanged");
        assert_eq!(editor.undo_stack.len(), undo_len, "no undo snapshot");
        assert!(!modified, "ui() reports no modification");
    }

    #[test]
    fn select_middle_drag_around_an_atom_does_not_lasso() {
        let mut editor = ChemStructEditor { tool: Tool::Select, ..Default::default() };
        editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let pan_start = editor.pan;

        // Starts in empty space and circles the atom at MID.
        let path = [
            MID + egui::vec2(-150.0, 0.0),
            MID + egui::vec2(0.0, -150.0),
            MID + egui::vec2(150.0, 0.0),
            MID + egui::vec2(0.0, 150.0),
            MID + egui::vec2(-150.0, 0.0),
        ];
        // The path is closed, so the pan ends where it began; check it moved along the way.
        let mut panned = false;
        middle_drag_along(&mut editor, &path, |e| {
            assert!(e.lasso_path.is_empty(), "no lasso path during a middle drag");
            panned |= e.pan != pan_start;
        });

        assert!(panned, "the middle drag pans");
        assert!(editor.selected_atoms.is_empty(), "nothing lasso-selected");
    }

    /// Escape aborts an egui drag while the primary button is still held; the tool must still
    /// see that as the end of its primary drag and tear the lasso down.
    #[test]
    fn select_primary_lasso_aborted_by_escape_is_torn_down() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor { tool: Tool::Select, ..Default::default() };
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let p0 = MID + egui::vec2(-150.0, 0.0);
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p0)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerButton {
            pos: p0,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        for p in [p0 + egui::vec2(40.0, -40.0), p0 + egui::vec2(80.0, -60.0)] {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p)]);
        }
        assert!(!editor.lasso_path.is_empty(), "the primary drag draws a lasso");
        assert!(editor.drag_origin_screen.is_some());

        let (events, modifiers) = crate::testkit::chord("Escape");
        crate::testkit::run_frame_with_modifiers(&ctx, screen_rect, &mut editor, events, modifiers);
        assert!(editor.lasso_path.is_empty(), "the aborted lasso is cleared");
        assert!(editor.drag_origin_screen.is_none(), "the aborted lasso is torn down");
    }

    #[test]
    fn bond_middle_drag_from_an_atom_does_not_draw_a_bond() {
        let mut editor = ChemStructEditor::default();
        assert_eq!(editor.tool, Tool::Bond);
        editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let undo_len = editor.undo_stack.len();

        let path = [MID, MID + egui::vec2(80.0, 40.0), MID + egui::vec2(150.0, 90.0)];
        middle_drag_along(&mut editor, &path, |e| {
            assert!(e.bond_start.is_none(), "no bond preview during a middle drag");
        });

        assert_eq!(editor.molecule.atoms.len(), 1, "no atom added");
        assert!(editor.molecule.bonds.is_empty(), "no bond added");
        assert_eq!(editor.undo_stack.len(), undo_len, "no undo snapshot");
    }
}

/// `fit_view` frames the drawing the editor then actually paints.
#[cfg(test)]
mod fit_tests {
    use super::*;

    /// A drawing far from the origin whose extremes are labels: H2N on the left, CO2Et on the
    /// right, 45 units apart.
    fn labeled_editor() -> ChemStructEditor {
        let mut editor = ChemStructEditor::default();
        let m = &mut editor.molecule;
        for [(e1, p1), (e2, p2)] in [
            [("N", [-20.0, 3.0]), ("C", [-18.5, 3.0])],
            [("CO2Et", [25.0, 4.0]), ("C", [23.5, 4.0])],
        ] {
            let a = m.add_atom(e1.to_string(), p1, 0);
            let b = m.add_atom(e2.to_string(), p2, 0);
            m.add_bond(a, b, BondOrder::Single);
        }
        editor
    }

    /// Run one frame; returns the canvas rect the editor drew into.
    fn frame(ctx: &egui::Context, editor: &mut ChemStructEditor) -> egui::Rect {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, 480.0));
        let raw = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
        let mut canvas = egui::Rect::NOTHING;
        let _ = crate::testkit::run_ui(&ctx, raw, |ui| {
            canvas = ui.available_rect_before_wrap();
            editor.ui(ui);
        });
        canvas
    }

    /// Every atom and every label rect painted in the last frame lies inside the margin, and
    /// the drawing's bounding rect is centered on the canvas.
    fn assert_framed(editor: &ChemStructEditor, canvas: egui::Rect) {
        let avail = canvas.shrink(FitOptions::default().margin).expand(0.01);
        let center = canvas.center();
        let mut bounds = egui::Rect::NOTHING;
        for a in &editor.molecule.atoms {
            let p = editor.mol_to_screen(a.pos, center);
            assert!(avail.contains(p), "atom {p:?} outside {avail:?}");
            bounds.extend_with(p);
        }
        assert_eq!(editor.label_rects().len(), 2, "both labels painted");
        for r in editor.label_rects().values() {
            assert!(avail.contains_rect(*r), "label {r:?} outside {avail:?}");
            bounds = bounds.union(*r);
        }
        let off = bounds.center() - center;
        assert!(off.x.abs() <= 1.0 && off.y.abs() <= 1.0, "drawing center off by {off:?}");
    }

    #[test]
    fn fit_view_frames_the_painted_drawing() {
        let ctx = egui::Context::default();
        let mut editor = labeled_editor();
        let canvas = frame(&ctx, &mut editor);
        editor.fit_view(&ctx, canvas);
        assert!(editor.zoom < 1.0, "45 units do not fit 600 px at zoom 1");
        assert_eq!(frame(&ctx, &mut editor), canvas);
        assert_framed(&editor, canvas);

        let (zoom, pan) = (editor.zoom, editor.pan);
        editor.fit_view_with(&ctx, canvas, FitOptions { keep_zoom: true, ..Default::default() });
        assert_eq!(editor.zoom, zoom, "keep_zoom keeps the zoom");
        assert!((editor.pan - pan).length() < 1e-3, "an already fitted view stays put");

        let mut empty =
            ChemStructEditor { zoom: 2.0, pan: egui::vec2(5.0, 6.0), ..Default::default() };
        empty.fit_view(&ctx, canvas);
        assert_eq!((empty.zoom, empty.pan), (2.0, egui::vec2(5.0, 6.0)), "nothing to fit");
    }

    #[test]
    fn request_fit_applies_on_the_next_frame_with_its_canvas_once() {
        // A fresh context: no frame has run, so `fit_view` could not lay labels out yet.
        let ctx = egui::Context::default();
        let mut editor = labeled_editor();
        editor.request_fit();
        assert_eq!((editor.zoom, editor.pan), (1.0, egui::Vec2::ZERO), "nothing until ui()");
        let canvas = frame(&ctx, &mut editor);
        assert_framed(&editor, canvas);

        // The request is used up: a later pan stays.
        editor.pan += egui::vec2(40.0, -25.0);
        let pan = editor.pan;
        frame(&ctx, &mut editor);
        assert_eq!(editor.pan, pan);
    }

    /// Run one frame delivering the chord `spec`; with `text_field`, a host text field that
    /// holds keyboard focus is shown above the editor. Returns the editor's canvas rect.
    fn chord_frame(
        ctx: &egui::Context,
        editor: &mut ChemStructEditor,
        spec: &str,
        mut text_field: Option<&mut String>,
    ) -> egui::Rect {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(640.0, 480.0));
        let (events, modifiers) = crate::testkit::chord(spec);
        let events = std::iter::once(egui::Event::ModifiersChanged(modifiers)).chain(events).collect();
        let raw = egui::RawInput {
            screen_rect: Some(screen),
            events,
            focused: true,
            ..Default::default()
        };
        let mut canvas = egui::Rect::NOTHING;
        let _ = crate::testkit::run_ui(&ctx, raw, |ui| {
            if let Some(text) = text_field.as_deref_mut() {
                ui.add(egui::TextEdit::singleline(text)).request_focus();
            }
            canvas = ui.available_rect_before_wrap();
            editor.ui(ui);
        });
        canvas
    }

    #[test]
    fn ctrl_0_fits_the_view_in_the_same_frame_in_every_tool() {
        // A hotspot gives the plain `0` shortcut (extend by one carbon) a target, so a Ctrl+0
        // that leaked into the drawing shortcuts would add an atom.
        let with_hotspot = |tool: Tool| {
            let mut editor = ChemStructEditor { tool, ..labeled_editor() };
            editor.hotspot_atom = Some(editor.molecule.atoms[1].id);
            editor
        };

        // Control: with that hotspot, plain `0` does draw in the Bond tool.
        let mut plain = with_hotspot(Tool::Bond);
        let atoms = plain.molecule.atoms.len();
        chord_frame(&egui::Context::default(), &mut plain, "0", None);
        assert!(plain.molecule.atoms.len() > atoms, "plain `0` extends the hotspot");

        for tool in [Tool::Bond, Tool::Select, Tool::Eraser] {
            let ctx = egui::Context::default();
            let mut editor = with_hotspot(tool.clone());
            let atoms = editor.molecule.atoms.len();
            let undo_len = editor.undo_stack.len();
            let canvas = chord_frame(&ctx, &mut editor, "Ctrl+0", None);
            assert!(editor.zoom < 1.0, "{tool:?}: Ctrl+0 zooms out to fit");
            assert_framed(&editor, canvas);
            // Ctrl+0 only moves the view: it does not also run the plain `0` shortcut.
            assert_eq!(editor.molecule.atoms.len(), atoms, "{tool:?}: nothing drawn");
            assert_eq!(editor.undo_stack.len(), undo_len, "{tool:?}: no undo snapshot");
        }
    }

    #[test]
    fn ctrl_0_does_nothing_while_a_host_text_field_has_focus() {
        let ctx = egui::Context::default();
        let mut editor = labeled_editor();
        let mut text = String::new();
        // First frame: the field takes focus.
        chord_frame(&ctx, &mut editor, "Escape", Some(&mut text));
        assert!(ctx.egui_wants_keyboard_input());
        chord_frame(&ctx, &mut editor, "Ctrl+0", Some(&mut text));
        assert_eq!((editor.zoom, editor.pan), (1.0, egui::Vec2::ZERO), "view unchanged");
    }

    #[test]
    fn ctrl_0_does_nothing_while_the_label_editor_is_open() {
        let ctx = egui::Context::default();
        let mut editor = labeled_editor();
        let id = editor.molecule.atoms[1].id;
        editor.label_edit = Some((id, "C".to_string()));
        // On its first frame the label box is not focused yet; Ctrl+0 is still text input.
        chord_frame(&ctx, &mut editor, "Ctrl+0", None);
        assert_eq!((editor.zoom, editor.pan), (1.0, egui::Vec2::ZERO), "view unchanged");
        chord_frame(&ctx, &mut editor, "Ctrl+0", None);
        assert_eq!((editor.zoom, editor.pan), (1.0, egui::Vec2::ZERO), "view unchanged");
        assert!(editor.label_edit.is_some(), "the label editor stays open");
    }
}
