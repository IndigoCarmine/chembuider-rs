use super::{ChemStructEditor, DEFAULT_BOND_LENGTH};
use crate::config::{AtomAction, BondAction, ResolvedAtomAction, SelectionAction};
use crate::molecule::{BondOrder, BondStereo};
use eframe::egui;

// ═══════════════════════════════════════════════════════════════════════════════
// Public tool handlers
// ═══════════════════════════════════════════════════════════════════════════════

pub fn process_bond_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
) -> bool {
    let mut modified = false;
    let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);

    // ── Drag tracking ────────────────────────────────────────────────────────
    if response.drag_started() {
        editor.push_undo(); // snapshot before drag modifies positions
        editor.drag_origin_screen = Some(mouse);
        // Prefer hit atom; fall back to currently hovered atom for slight-miss tolerance
        let hit = editor.hit_test_atom(mouse, center).or(editor.hovered_atom);
        editor.dragging_atom = hit;
        editor.bond_start = None;
    }

    if response.dragged() {
        let drag_delta = response.drag_delta();
        let total_delta = mouse - editor.drag_origin_screen.unwrap_or(mouse - drag_delta);

        if let Some(atom_id) = editor.dragging_atom {
            let inv = 1.0 / (editor.zoom * super::SCALE_FACTOR);
            if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
                atom.pos[0] += drag_delta.x * inv;
                atom.pos[1] += drag_delta.y * inv;
                modified = true;
            }
            editor.bond_start = None;
            editor.preview_end_screen = None;
        } else if total_delta.length() > 4.0 {
            if editor.bond_start.is_none() {
                let origin = editor.drag_origin_screen.unwrap_or(mouse);
                editor.bond_start = editor.hit_test_atom(origin, center);
            }
            editor.preview_end_screen = Some(mouse);
        }
    }

    if response.drag_stopped() {
        // ── Atom merge: if dragged atom lands on another atom, fuse them ──────
        if let Some(drag_id) = editor.dragging_atom {
            let current_pos = editor.molecule.atom_by_id(drag_id).map(|a| a.pos);
            if let Some(pos) = current_pos {
                let merge_r = crate::molecule::Molecule::SNAP_RADIUS;
                let merge_target = editor.molecule.atoms.iter()
                    .filter(|a| a.id != drag_id)
                    .filter(|a| {
                        let dx = a.pos[0] - pos[0];
                        let dy = a.pos[1] - pos[1];
                        dx * dx + dy * dy < merge_r * merge_r
                    })
                    .min_by(|a, b| {
                        let da = (a.pos[0]-pos[0]).powi(2) + (a.pos[1]-pos[1]).powi(2);
                        let db = (b.pos[0]-pos[0]).powi(2) + (b.pos[1]-pos[1]).powi(2);
                        da.partial_cmp(&db).unwrap()
                    })
                    .map(|a| a.id);

                if let Some(target_id) = merge_target {
                    for bond in &mut editor.molecule.bonds {
                        if bond.begin == drag_id { bond.begin = target_id; }
                        if bond.end   == drag_id { bond.end   = target_id; }
                    }
                    editor.molecule.bonds.retain(|b| b.begin != b.end);
                    // Remove duplicate bonds (keep first encountered)
                    let mut seen = std::collections::HashSet::new();
                    editor.molecule.bonds.retain(|b| {
                        let key = if b.begin < b.end { (b.begin, b.end) } else { (b.end, b.begin) };
                        seen.insert(key)
                    });
                    editor.molecule.atoms.retain(|a| a.id != drag_id);
                    if editor.hotspot_atom == Some(drag_id) {
                        editor.hotspot_atom = Some(target_id);
                    }
                    modified = true;
                }
            }
        }

        let start_id   = editor.bond_start.take();
        let end_screen = editor.preview_end_screen.take();

        if let (Some(src_id), Some(end)) = (start_id, end_screen) {
            let target_id = editor.hit_test_atom(end, center)
                .or_else(|| {
                    let mol_pos = editor.screen_to_mol(end, center);
                    editor.molecule.find_atom_near(mol_pos, crate::molecule::Molecule::SNAP_RADIUS)
                })
                .unwrap_or_else(|| {
                    let mol_pos = editor.screen_to_mol(end, center);
                    editor.molecule.add_atom(editor.current_element.clone(), mol_pos, 0)
                });
            if target_id != src_id {
                if let Some(existing) = editor.molecule.bond_between(src_id, target_id).map(|b| b.id) {
                    if let Some(bond) = editor.molecule.bond_by_id_mut(existing) {
                        bond.order = bond.order.cycle();
                        modified = true;
                    }
                } else {
                    let bid = editor.molecule.add_bond(src_id, target_id, editor.current_bond_order.clone());
                    if bid != 0 {
                        if let Some(bond) = editor.molecule.bond_by_id_mut(bid) {
                            bond.stereo = editor.current_bond_stereo.clone();
                        }
                    }
                    modified = true;
                }
            }
        }
        editor.dragging_atom = None;
        editor.drag_start_mol = None;
        editor.drag_origin_screen = None;
    }

    // ── Click ────────────────────────────────────────────────────────────────
    if response.clicked() {
        editor.push_undo();
        if let Some(atom_id) = editor.hovered_atom {
            let angle = editor.best_new_bond_angle(atom_id);
            let new_pos = {
                let src = editor.molecule.atom_by_id(atom_id).unwrap();
                [src.pos[0] + angle.cos() * DEFAULT_BOND_LENGTH,
                 src.pos[1] + angle.sin() * DEFAULT_BOND_LENGTH]
            };
            let new_id = find_or_create_atom(editor, new_pos, center);
            if new_id != atom_id {
                if let Some(existing) = editor.molecule.bond_between(atom_id, new_id).map(|b| b.id) {
                    if let Some(bond) = editor.molecule.bond_by_id_mut(existing) {
                        bond.order = bond.order.cycle();
                        modified = true;
                    }
                } else {
                    let bid = editor.molecule.add_bond(atom_id, new_id, editor.current_bond_order.clone());
                    if bid != 0 {
                        if let Some(bond) = editor.molecule.bond_by_id_mut(bid) {
                            bond.stereo = editor.current_bond_stereo.clone();
                        }
                    }
                    modified = true;
                }
            }
            editor.hotspot_atom = Some(new_id);
        } else if let Some(bond_id) = editor.hovered_bond {
            if let Some(bond) = editor.molecule.bond_by_id_mut(bond_id) {
                bond.order = bond.order.cycle();
                modified = true;
            }
        } else {
            let mol_pos = editor.screen_to_mol(mouse, center);
            let new_id = editor.molecule.add_atom(editor.current_element.clone(), mol_pos, 0);
            editor.hotspot_atom = Some(new_id);
            modified = true;
        }
    }

    // ── Middle-click pan ─────────────────────────────────────────────────────
    if ui.input(|i| i.pointer.middle_down()) {
        if let Some(delta) = ui.input(|i| i.pointer.delta()).into() {
            editor.pan += delta;
        }
    }

    // ── Hover + Delete: remove the atom or bond under the cursor ─────────────
    let delete_pressed =
        ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete_pressed && delete_hovered(editor) {
        modified = true;
    }

    // ── Keyboard shortcuts ───────────────────────────────────────────────────
    modified |= handle_keys_bond_tool(editor, ui);

    modified
}

/// Delete whichever atom or bond is currently hovered (atom takes priority).
/// Returns true if something was removed.
fn delete_hovered(editor: &mut ChemStructEditor) -> bool {
    if let Some(atom_id) = editor.hovered_atom {
        editor.push_undo();
        editor.molecule.remove_atom(atom_id);
        if editor.hotspot_atom == Some(atom_id) { editor.hotspot_atom = None; }
        editor.selected_atoms.remove(&atom_id);
        editor.hovered_atom = None;
        return true;
    }
    if let Some(bond_id) = editor.hovered_bond {
        editor.push_undo();
        editor.molecule.remove_bond(bond_id);
        editor.hovered_bond = None;
        return true;
    }
    false
}

pub fn process_select_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
) -> bool {
    let mut modified = false;
    let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);

    if response.clicked() {
        editor.lasso_path.clear();
        if let Some(atom_id) = editor.hovered_atom {
            if editor.selected_atoms.contains(&atom_id) {
                editor.selected_atoms.remove(&atom_id);
            } else {
                editor.selected_atoms.insert(atom_id);
            }
        } else {
            editor.selected_atoms.clear();
        }
    }

    if response.drag_started() {
        editor.lasso_path.clear();
        editor.drag_origin_screen = Some(mouse);
        editor.dragging_atom = editor.hovered_atom;
    }

    if response.dragged() {
        if let Some(atom_id) = editor.dragging_atom {
            let drag_delta = response.drag_delta();
            let inv = 1.0 / (editor.zoom * super::SCALE_FACTOR);
            let (dx, dy) = (drag_delta.x * inv, drag_delta.y * inv);
            let atoms_to_move: Vec<u32> = if editor.selected_atoms.contains(&atom_id) {
                editor.selected_atoms.iter().cloned().collect()
            } else {
                vec![atom_id]
            };
            for id in atoms_to_move {
                if let Some(atom) = editor.molecule.atom_by_id_mut(id) {
                    atom.pos[0] += dx;
                    atom.pos[1] += dy;
                }
            }
            modified = true;
        } else {
            editor.lasso_path.push(mouse);
        }
    }

    if response.drag_stopped() {
        if editor.dragging_atom.is_some() {
            editor.dragging_atom = None;
        } else if editor.lasso_path.len() > 2 {
            editor.selected_atoms.clear();
            let lasso = editor.lasso_path.clone();
            let hits: Vec<u32> = editor.molecule.atoms.iter().filter_map(|atom| {
                let sp = editor.mol_to_screen(atom.pos, center);
                if is_point_in_polygon(sp, &lasso) { Some(atom.id) } else { None }
            }).collect();
            editor.selected_atoms.extend(hits);
        }
        editor.lasso_path.clear();
        editor.drag_origin_screen = None;
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────
    // Delete/Backspace: remove the selection, or fall back to the hovered atom/bond.
    let delete_pressed =
        ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete_pressed {
        if !editor.selected_atoms.is_empty() {
            editor.push_undo();
            let ids: Vec<u32> = editor.selected_atoms.drain().collect();
            for id in ids { editor.molecule.remove_atom(id); }
            modified = true;
        } else if delete_hovered(editor) {
            modified = true;
        }
    }

    modified |= handle_keys_select_tool(editor, ui);

    modified
}

pub fn process_eraser_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    _center: egui::Pos2,
) -> bool {
    let mut modified = false;
    if response.clicked() {
        if let Some(atom_id) = editor.hovered_atom {
            editor.molecule.remove_atom(atom_id);
            modified = true;
        } else if let Some(bond_id) = editor.hovered_bond {
            editor.molecule.remove_bond(bond_id);
            modified = true;
        }
    }
    modified
}

// ═══════════════════════════════════════════════════════════════════════════════
// Key handling (Bond tool)
// ═══════════════════════════════════════════════════════════════════════════════

fn handle_keys_bond_tool(editor: &mut ChemStructEditor, ui: &egui::Ui) -> bool {
    let mut modified = false;
    let keys = pressed_key_strings(ui);

    for (key, _shift, ctrl, alt) in &keys {
        if *ctrl || *alt { continue; }

        // Hotspot / navigation keys
        match key.as_str() {
            "Enter" => {
                if let Some(id) = editor.hovered_atom { editor.hotspot_atom = Some(id); }
                continue;
            }
            "Space" | "Tab" => {
                editor.hotspot_atom = None;
                continue;
            }
            "g" | "G" => {
                if let Some(id) = editor.hovered_atom { editor.hotspot_atom = Some(id); }
                continue;
            }
            _ => {}
        }

        // ExpandLabel: x / X key — expands a label-named atom (e.g. "Bor") into its fragment
        if key == "x" || key == "X" {
            let target = editor.hotspot_atom.or(editor.hovered_atom);
            if let Some(atom_id) = target {
                editor.push_undo();
                if expand_label(editor, atom_id) { modified = true; }
                continue;
            }
        }

        // Open label-edit mode: backtick key
        if key == "`" {
            let target = editor.hotspot_atom.or(editor.hovered_atom);
            if let Some(atom_id) = target {
                let current = editor.molecule.atom_by_id(atom_id)
                    .map(|a| a.element.clone())
                    .unwrap_or_default();
                editor.label_edit = Some((atom_id, current));
                continue;
            }
        }

        // Determine target
        let atom_target = editor.hotspot_atom.or(editor.hovered_atom);
        let bond_target = if atom_target.is_none() { editor.hovered_bond } else { None };

        if let Some(atom_id) = atom_target {
            editor.push_undo();
            if dispatch_atom_key(editor, atom_id, key) { modified = true; }
        } else if let Some(bond_id) = bond_target {
            editor.push_undo();
            if dispatch_bond_key(editor, bond_id, key) { modified = true; }
        } else {
            // No atom or bond target: if key maps to a single-atom fragment, place at cursor
            if place_element_at_cursor(editor, key) { modified = true; }
        }
    }

    // Arrow key navigation (with or without repeat)
    if let Some(src_id) = editor.hotspot_atom {
        let dir = ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowRight) { Some((1.0_f32,  0.0_f32)) }
            else if i.key_pressed(egui::Key::ArrowLeft)  { Some((-1.0, 0.0)) }
            else if i.key_pressed(egui::Key::ArrowUp)    { Some((0.0, -1.0)) }
            else if i.key_pressed(egui::Key::ArrowDown)  { Some((0.0,  1.0)) }
            else { None }
        });
        if let Some((dx, dy)) = dir {
            if let Some(next) = navigate_direction(&editor.molecule, src_id, dx, dy) {
                editor.hotspot_atom = Some(next);
            }
        }
    }

    modified
}

// ═══════════════════════════════════════════════════════════════════════════════
// Key handling (Select tool)
// ═══════════════════════════════════════════════════════════════════════════════

fn handle_keys_select_tool(editor: &mut ChemStructEditor, ui: &egui::Ui) -> bool {
    let mut modified = false;
    let keys = pressed_key_strings(ui);

    for (key, _shift, ctrl, alt) in &keys {
        // Hotspot keys
        match key.as_str() {
            "Enter" => {
                if let Some(id) = editor.hovered_atom { editor.hotspot_atom = Some(id); }
                continue;
            }
            "Space" | "Tab" => { editor.hotspot_atom = None; continue; }
            "g" | "G" => {
                if let Some(id) = editor.hovered_atom {
                    editor.hotspot_atom = Some(id);
                    editor.selected_atoms.insert(id);
                }
                continue;
            }
            _ => {}
        }

        if !ctrl && !alt {
            let atom_target = editor.hotspot_atom.or(editor.hovered_atom);
            if let Some(atom_id) = atom_target {
                if dispatch_atom_key(editor, atom_id, key) { modified = true; }
            }
        }

        // Selection arrow operations (Ctrl/Alt/Shift+arrows)
        if *ctrl || *alt || *_shift {
            let actions: Vec<SelectionAction> = editor.config.selection_shortcuts.iter()
                .filter(|s| {
                    s.key == *key
                    && s.ctrl  == *ctrl
                    && s.alt   == *alt
                    && s.shift == *_shift
                })
                .map(|s| s.action.clone())
                .collect();
            for action in actions {
                apply_selection_action(editor, &action);
                modified = true;
            }
        }
    }

    // Hotspot arrow navigation (no modifier)
    if let Some(src_id) = editor.hotspot_atom {
        let dir = ui.input(|i| {
            let no_mod = !i.modifiers.ctrl && !i.modifiers.shift && !i.modifiers.alt;
            if no_mod && i.key_pressed(egui::Key::ArrowRight) { Some((1.0_f32,  0.0_f32)) }
            else if no_mod && i.key_pressed(egui::Key::ArrowLeft)  { Some((-1.0,  0.0)) }
            else if no_mod && i.key_pressed(egui::Key::ArrowUp)    { Some((0.0, -1.0)) }
            else if no_mod && i.key_pressed(egui::Key::ArrowDown)  { Some((0.0,  1.0)) }
            else { None }
        });
        if let Some((dx, dy)) = dir {
            if let Some(next) = navigate_direction(&editor.molecule, src_id, dx, dy) {
                editor.hotspot_atom = Some(next);
            }
        }
    }

    modified
}

// ═══════════════════════════════════════════════════════════════════════════════
// Config-driven dispatch
// ═══════════════════════════════════════════════════════════════════════════════

fn dispatch_atom_key(editor: &mut ChemStructEditor, atom_id: u32, key: &str) -> bool {
    let actions: Vec<AtomAction> = editor.config.atom_shortcuts.iter()
        .filter(|s| s.key == key && !s.ctrl && !s.alt)
        .map(|s| s.action.clone())
        .collect();

    for action in &actions {
        let resolved = editor.config.atom_action_to_fragment(action);
        if let Some(resolved) = resolved {
            apply_atom_action(editor, atom_id, resolved);
            return true;
        }
    }
    false
}

fn dispatch_bond_key(editor: &mut ChemStructEditor, bond_id: u32, key: &str) -> bool {
    let actions: Vec<BondAction> = editor.config.bond_shortcuts.iter()
        .filter(|s| s.key == key && !s.ctrl && !s.alt)
        .map(|s| s.action.clone())
        .collect();

    for action in &actions {
        if apply_bond_action(editor, bond_id, action) { return true; }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════════════
// Action applicators
// ═══════════════════════════════════════════════════════════════════════════════

fn apply_atom_action(editor: &mut ChemStructEditor, atom_id: u32, action: ResolvedAtomAction) {
    match action {
        ResolvedAtomAction::InsertFragment(frag) => {
            // Single-atom fragment with no bonds = change the element of the existing atom
            // Change element in-place: typing Br on a C atom turns it into Br, not C-Br
            if frag.atoms.len() == 1 && frag.bonds.is_empty() {
                if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
                    atom.element = frag.atoms[0].element.clone();
                    atom.charge  = frag.atoms[0].charge;
                }
                editor.hotspot_atom = Some(atom_id);
            } else {
                // Detect best orientation: angle + flip to keep ring bulk away from existing bonds
                let (angle, flip) = editor.best_fragment_placement(atom_id, &frag);
                let new_ids = editor.molecule.insert_fragment(&frag, atom_id, angle, flip);
                if let Some(&last) = new_ids.last() {
                    editor.hotspot_atom = Some(last);
                }
            }
        }
        ResolvedAtomAction::ExtendChain(n, _zigzag) => {
            extend_chain(editor, atom_id, n);
        }
        ResolvedAtomAction::ModifyCharge(delta) => {
            if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
                atom.charge = atom.charge.saturating_add(delta);
            }
        }
    }
}

fn apply_bond_action(editor: &mut ChemStructEditor, bond_id: u32, action: &BondAction) -> bool {
    match action {
        BondAction::Stereo { stereo } => {
            if let Some(bond) = editor.molecule.bond_by_id_mut(bond_id) {
                // Cycle through stereo if same type pressed again
                if bond.stereo == *stereo {
                    bond.stereo = BondStereo::None;
                } else {
                    bond.stereo = stereo.clone();
                }
            }
            true
        }
        BondAction::Order { order } => {
            if let Some(bond) = editor.molecule.bond_by_id_mut(bond_id) {
                bond.order = order.clone();
                bond.stereo = BondStereo::None;
            }
            true
        }
        BondAction::Ring { ring } => {
            // Fuse ring onto the bond (bond becomes one edge of the ring)
            fuse_ring_onto_bond(editor, bond_id, *ring);
            true
        }
    }
}

fn apply_selection_action(editor: &mut ChemStructEditor, action: &SelectionAction) {
    const STEP: f32 = DEFAULT_BOND_LENGTH;
    const ROTATE_DEG: f32 = std::f32::consts::PI / 12.0; // 15°

    let selected: Vec<u32> = editor.selected_atoms.iter().cloned().collect();
    if selected.is_empty() { return; }

    match action {
        SelectionAction::MoveRight => move_atoms(editor, &selected, [ STEP, 0.0]),
        SelectionAction::MoveLeft  => move_atoms(editor, &selected, [-STEP, 0.0]),
        SelectionAction::MoveUp    => move_atoms(editor, &selected, [0.0, -STEP]),
        SelectionAction::MoveDown  => move_atoms(editor, &selected, [0.0,  STEP]),
        SelectionAction::RotateCW  => rotate_atoms(editor, &selected,  ROTATE_DEG),
        SelectionAction::RotateCCW => rotate_atoms(editor, &selected, -ROTATE_DEG),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Geometric helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Extend a chain of `n` carbons from `from_atom_id`, using best angle each step.
/// Snaps to existing atoms when the computed position is within SNAP_RADIUS.
fn extend_chain(editor: &mut ChemStructEditor, from_atom_id: u32, n: usize) {
    let snap = crate::molecule::Molecule::SNAP_RADIUS;
    let mut current = from_atom_id;
    for _ in 0..n {
        let angle = editor.best_new_bond_angle(current);
        let new_pos = {
            let src = editor.molecule.atom_by_id(current).unwrap();
            [src.pos[0] + angle.cos() * DEFAULT_BOND_LENGTH,
             src.pos[1] + angle.sin() * DEFAULT_BOND_LENGTH]
        };
        let new_id = editor.molecule.find_atom_near(new_pos, snap)
            .unwrap_or_else(|| editor.molecule.add_atom("C".to_string(), new_pos, 0));
        editor.molecule.add_bond(current, new_id, BondOrder::Single);
        if new_id == current { break; } // guard: snapped to self
        current = new_id;
    }
    editor.hotspot_atom = Some(current);
}

/// Fuse an n-membered ring onto a bond, sharing that bond as one edge.
fn fuse_ring_onto_bond(editor: &mut ChemStructEditor, bond_id: u32, n: usize) {
    if n < 3 { return; }
    let (a_id, b_id) = {
        let Some(bond) = editor.molecule.bonds.iter().find(|b| b.id == bond_id) else { return };
        (bond.begin, bond.end)
    };
    let (pa, pb) = {
        let Some(a) = editor.molecule.atom_by_id(a_id) else { return };
        let Some(b) = editor.molecule.atom_by_id(b_id) else { return };
        (a.pos, b.pos)
    };

    use std::f32::consts::PI;
    let l = DEFAULT_BOND_LENGTH;
    let r = l / (2.0 * (PI / n as f32).sin());
    // Place ring on the "free" side of the bond
    let mx = (pa[0] + pb[0]) * 0.5;
    let my = (pa[1] + pb[1]) * 0.5;
    let dx = pb[0] - pa[0];
    let dy = pb[1] - pa[1];
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    // Perpendicular: choose side away from majority of existing neighbors
    let perp_len = (r * r - l * l * 0.25).max(0.0).sqrt();
    let cx = mx + (-dy / len) * perp_len;
    let cy = my + ( dx / len) * perp_len;

    let alpha_a = (pa[1] - cy).atan2(pa[0] - cx);
    let mut ring_atoms: Vec<u32> = vec![a_id, b_id];
    for k in 2..n {
        let angle = alpha_a - k as f32 * std::f32::consts::TAU / n as f32;
        let pos = [cx + r * angle.cos(), cy + r * angle.sin()];
        let new_id = editor.molecule.add_atom("C".to_string(), pos, 0);
        ring_atoms.push(new_id);
    }
    // Close the ring
    for k in 0..n {
        let a = ring_atoms[k];
        let b = ring_atoms[(k + 1) % n];
        if editor.molecule.bond_between(a, b).is_none() {
            editor.molecule.add_bond(a, b, BondOrder::Single);
        }
    }
}

/// Find the neighbor atom most in direction (dx, dy) from source.
fn navigate_direction(
    mol: &crate::molecule::Molecule,
    src_id: u32,
    dx: f32,
    dy: f32,
) -> Option<u32> {
    let src = mol.atom_by_id(src_id)?;
    mol.neighbor_atom_ids(src_id)
        .into_iter()
        .filter_map(|nid| {
            let n = mol.atom_by_id(nid)?;
            let ddx = n.pos[0] - src.pos[0];
            let ddy = n.pos[1] - src.pos[1];
            let len = (ddx * ddx + ddy * ddy).sqrt();
            if len < 0.001 { return None; }
            let dot = (ddx / len) * dx + (ddy / len) * dy;
            if dot > 0.0 { Some((nid, dot)) } else { None }
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(id, _)| id)
}

fn move_atoms(editor: &mut ChemStructEditor, ids: &[u32], delta: [f32; 2]) {
    for &id in ids {
        if let Some(atom) = editor.molecule.atom_by_id_mut(id) {
            atom.pos[0] += delta[0];
            atom.pos[1] += delta[1];
        }
    }
}

fn rotate_atoms(editor: &mut ChemStructEditor, ids: &[u32], angle: f32) {
    // Centroid
    let (mut cx, mut cy) = (0.0_f32, 0.0_f32);
    let mut count = 0;
    for &id in ids {
        if let Some(a) = editor.molecule.atom_by_id(id) {
            cx += a.pos[0]; cy += a.pos[1]; count += 1;
        }
    }
    if count == 0 { return; }
    cx /= count as f32; cy /= count as f32;

    let (sin_a, cos_a) = angle.sin_cos();
    for &id in ids {
        if let Some(atom) = editor.molecule.atom_by_id_mut(id) {
            let dx = atom.pos[0] - cx;
            let dy = atom.pos[1] - cy;
            atom.pos = [cx + dx * cos_a - dy * sin_a,
                        cy + dx * sin_a + dy * cos_a];
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Key-string helpers
// ═══════════════════════════════════════════════════════════════════════════════

/// Returns (key_str, shift, ctrl, alt) for each key pressed this frame (no repeat).
fn pressed_key_strings(ui: &egui::Ui) -> Vec<(String, bool, bool, bool)> {
    ui.input(|i| {
        i.events.iter().filter_map(|event| {
            match event {
                egui::Event::Key { key, pressed: true, modifiers, repeat: false, .. } => {
                    let s = key_to_str(*key, modifiers.shift)?;
                    Some((s, modifiers.shift, modifiers.ctrl, modifiers.alt))
                }
                _ => None,
            }
        }).collect()
    })
}

fn key_to_str(key: egui::Key, shift: bool) -> Option<String> {
    use egui::Key::*;
    let s: &str = match key {
        A => if shift { "A" } else { "a" },
        B => if shift { "B" } else { "b" },
        C => if shift { "C" } else { "c" },
        D => if shift { "D" } else { "d" },
        E => if shift { "E" } else { "e" },
        F => if shift { "F" } else { "f" },
        G => if shift { "G" } else { "g" },
        H => if shift { "H" } else { "h" },
        I => if shift { "I" } else { "i" },
        J => if shift { "J" } else { "j" },
        K => if shift { "K" } else { "k" },
        L => if shift { "L" } else { "l" },
        M => if shift { "M" } else { "m" },
        N => if shift { "N" } else { "n" },
        O => if shift { "O" } else { "o" },
        P => if shift { "P" } else { "p" },
        Q => if shift { "Q" } else { "q" },
        R => if shift { "R" } else { "r" },
        S => if shift { "S" } else { "s" },
        T => if shift { "T" } else { "t" },
        U => if shift { "U" } else { "u" },
        V => if shift { "V" } else { "v" },
        W => if shift { "W" } else { "w" },
        X => if shift { "X" } else { "x" },
        Y => if shift { "Y" } else { "y" },
        Z => if shift { "Z" } else { "z" },
        Num0 => if shift { ")" } else { "0" },
        Num1 => if shift { "!" } else { "1" },
        Num2 => if shift { "@" } else { "2" },
        Num3 => if shift { "#" } else { "3" },
        Num4 => if shift { "$" } else { "4" },
        Num5 => if shift { "%" } else { "5" },
        Num6 => if shift { "^" } else { "6" },
        Num7 => if shift { "&" } else { "7" },
        Num8 => if shift { "*" } else { "8" },
        Num9 => if shift { "(" } else { "9" },
        Plus   => "+",
        Minus  => if shift { "_" } else { "-" },
        Equals => if shift { "+" } else { "=" },
        Backtick => "`",
        ArrowRight => "ArrowRight",
        ArrowLeft  => "ArrowLeft",
        ArrowUp    => "ArrowUp",
        ArrowDown  => "ArrowDown",
        Enter  => "Enter",
        Space  => "Space",
        Tab    => "Tab",
        _ => return None,
    };
    Some(s.to_string())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Geometry / polygon helpers
// ═══════════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════════
// ExpandLabel
// ═══════════════════════════════════════════════════════════════════════════════

/// Replace an atom whose element text matches a known fragment name with the
/// full fragment structure, preserving all bonds to the rest of the molecule.
fn expand_label(editor: &mut ChemStructEditor, atom_id: u32) -> bool {
    let element = match editor.molecule.atom_by_id(atom_id) {
        Some(a) => a.element.clone(),
        None    => return false,
    };

    let Some(frag) = editor.config.resolve_fragment(&element) else { return false };

    // Change attach atom's element to match fragment's root atom
    let root_elem   = frag.atoms[frag.attach_idx].element.clone();
    let root_charge = frag.atoms[frag.attach_idx].charge;
    if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
        atom.element = root_elem;
        atom.charge  = root_charge;
    }

    // Pick direction & flip, then insert the fragment in-place
    let (angle, flip) = editor.best_fragment_placement(atom_id, &frag);
    editor.molecule.insert_fragment(&frag, atom_id, angle, flip);

    true
}

/// When no atom is targeted, check if the key maps to a single-atom fragment and place
/// that element at the current cursor position (last_mouse_mol).
fn place_element_at_cursor(editor: &mut ChemStructEditor, key: &str) -> bool {
    let actions: Vec<crate::config::AtomAction> = editor.config.atom_shortcuts.iter()
        .filter(|s| s.key == key && !s.ctrl && !s.alt)
        .map(|s| s.action.clone())
        .collect();

    for action in &actions {
        if let Some(crate::config::ResolvedAtomAction::InsertFragment(frag)) =
            editor.config.atom_action_to_fragment(action)
        {
            if frag.atoms.len() == 1 && frag.bonds.is_empty() {
                editor.push_undo();
                let pos = editor.last_mouse_mol;
                let new_id = editor.molecule.add_atom(
                    frag.atoms[0].element.clone(),
                    pos,
                    frag.atoms[0].charge,
                );
                editor.hotspot_atom = Some(new_id);
                return true;
            }
        }
    }
    false
}

fn find_or_create_atom(
    editor: &mut ChemStructEditor,
    mol_pos: [f32; 2],
    center: egui::Pos2,
) -> u32 {
    // Screen-space snap (covers normal hover radius)
    let screen = editor.mol_to_screen(mol_pos, center);
    if let Some(id) = editor.hit_test_atom(screen, center) { return id; }
    // Molecular-coordinate snap (catches ring closures and overlapping atoms)
    if let Some(id) = editor.molecule.find_atom_near(mol_pos, crate::molecule::Molecule::SNAP_RADIUS) {
        return id;
    }
    editor.molecule.add_atom(editor.current_element.clone(), mol_pos, 0)
}

fn is_point_in_polygon(point: egui::Pos2, polygon: &[egui::Pos2]) -> bool {
    if polygon.len() < 3 { return false; }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let pi = polygon[i];
        let pj = polygon[j];
        if ((pi.y > point.y) != (pj.y > point.y))
            && (point.x < (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}
