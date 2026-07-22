use super::{ChemStructEditor, DEFAULT_BOND_LENGTH};
use crate::config::{AtomAction, BondAction, ResolvedAtomAction, SelectionAction};
use crate::molecule::{BondOrder, BondStereo};

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

    // ── Drag tracking: dragging from an atom always draws a bond, never moves it — the
    // Bond tool's own gesture list is "drag atom→atom to draw a bond"; repositioning atoms
    // via drag is Select tool's job (`process_select_tool`), not this one. ─────────────────
    if response.drag_started() {
        editor.push_undo(); // snapshot before drag adds a bond/atom
        // `drag_started()` only fires once the pointer has moved past egui's drag threshold,
        // so by now `mouse` (the *current* position) may already be well past the atom the
        // drag actually started on. Hit-test at the press origin instead, or a fast drag off
        // an atom silently does nothing (see `bond_tool_drag_from_atom_to_empty_space_creates_bonded_atom`).
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(mouse);
        // Prefer hit atom; fall back to currently hovered atom for slight-miss tolerance
        editor.bond_start = editor.hit_test_atom(origin, center).or(editor.hovered_atom);
        editor.preview_end_screen = Some(mouse);
    }

    if response.dragged() {
        editor.preview_end_screen = Some(mouse);
    }

    if response.drag_stopped() {
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

/// Delete the current atom or bond target.
///
/// Priority: pointer targets first (an atom under the cursor beats a bond under it), then
/// keyboard targets. The keyboard *bond* target outranks the keyboard *atom* target because
/// it is only ever set by an explicit Shift+arrow, whereas the atom hotspot lingers after
/// every build action — deleting the atom when the user just selected a bond would be wrong.
///
/// The keyboard fallbacks are what make Delete work with no pointer at all.
///
/// Returns true if something was removed.
fn delete_hovered(editor: &mut ChemStructEditor) -> bool {
    enum Target { Atom(u32), Bond(u32) }

    let target = editor.hovered_atom.map(Target::Atom)
        .or_else(|| editor.hovered_bond.map(Target::Bond))
        .or_else(|| editor.hotspot_bond.map(Target::Bond))
        .or_else(|| editor.hotspot_atom.map(Target::Atom));

    match target {
        Some(Target::Atom(atom_id)) => {
            editor.push_undo();
            // Note a neighbour before the bonds disappear with the atom. Deleting the hotspot
            // used to clear it outright, which is unrecoverable without a pointer: `Enter` and
            // `g` both re-acquire a hotspot only from a *hovered* atom. A keyboard user could
            // therefore delete exactly once per structure and then had no way to keep editing.
            // Stepping to a former neighbour keeps the structure reachable.
            let fallback = editor.molecule.neighbor_atom_ids(atom_id).into_iter().next();
            editor.molecule.remove_atom(atom_id);
            if editor.hotspot_atom == Some(atom_id) { editor.hotspot_atom = fallback; }
            editor.selected_atoms.remove(&atom_id);
            editor.hovered_atom = None;
            true
        }
        Some(Target::Bond(bond_id)) => {
            editor.push_undo();
            editor.molecule.remove_bond(bond_id);
            editor.hovered_bond = None;
            editor.hotspot_bond = None;
            true
        }
        None => false,
    }
}

pub fn process_select_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
) -> bool {
    let mut modified = false;
    let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);

    // Double-click an atom or bond → select that whole connected molecule.
    if response.double_clicked() {
        editor.lasso_path.clear();
        let seed = editor.hovered_atom.or_else(|| {
            editor.hovered_bond
                .and_then(|bid| editor.molecule.bonds.iter().find(|b| b.id == bid))
                .map(|b| b.begin)
        });
        if let Some(atom_id) = seed {
            editor.selected_atoms = editor.molecule.connected_atoms(atom_id).into_iter().collect();
        }
    } else if response.clicked() {
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
        // Alt+drag with a selection rotates the molecule around its centroid.
        if ui.input(|i| i.modifiers.alt) && !editor.selected_atoms.is_empty() {
            editor.push_undo();
            let c = selection_centroid(editor);
            let mp = editor.screen_to_mol(mouse, center);
            editor.rotate_last_angle = Some((mp[1] - c[1]).atan2(mp[0] - c[0]));
            editor.dragging_atom = None;
        } else {
            editor.rotate_last_angle = None;
            editor.dragging_atom = editor.hovered_atom;
        }
    }

    if response.dragged() {
        if let Some(last) = editor.rotate_last_angle {
            // Rotate the selection by the change in cursor angle about its centroid.
            let c = selection_centroid(editor);
            let mp = editor.screen_to_mol(mouse, center);
            let angle = (mp[1] - c[1]).atan2(mp[0] - c[0]);
            let ids: Vec<u32> = editor.selected_atoms.iter().copied().collect();
            rotate_atoms(editor, &ids, angle - last);
            editor.rotate_last_angle = Some(angle);
            modified = true;
        } else if let Some(atom_id) = editor.dragging_atom {
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
        if editor.rotate_last_angle.take().is_some() {
            // rotation finished
        } else if editor.dragging_atom.is_some() {
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
            editor.push_undo();
            editor.molecule.remove_atom(atom_id);
            modified = true;
        } else if let Some(bond_id) = editor.hovered_bond {
            editor.push_undo();
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
                editor.hotspot_bond = None;
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
            let target = editor.hovered_atom.or(editor.hotspot_atom);
            if let Some(atom_id) = target {
                editor.push_undo();
                if expand_label(editor, atom_id) { modified = true; }
                continue;
            }
        }

        // Open label-edit mode: backtick key
        if key == "`" {
            let target = editor.hovered_atom.or(editor.hotspot_atom);
            if let Some(atom_id) = target {
                let current = editor.molecule.atom_by_id(atom_id)
                    .map(|a| a.element.clone())
                    .unwrap_or_default();
                editor.label_edit = Some((atom_id, current));
                continue;
            }
        }

        // Target resolution, in priority order:
        //  1. hovering an atom → act on it (change element / attach fragment)
        //  2. else, a key that names an element → drop that atom at the cursor, ahead of any
        //     stale hotspot (so O/N/S place a new atom rather than editing the hotspot)
        //  3. else, a hovered bond, or a keyboard-selected bond → bond key (either beats a
        //     stale atom hotspot so e.g. fusing a ring/fragment targets the bond, not the atom)
        //  4. else, a hotspot → build from it (chains, rings, fused fragments)
        if let Some(atom_id) = editor.hovered_atom {
            editor.push_undo();
            if dispatch_atom_key(editor, atom_id, key) { modified = true; }
        } else if place_element_at_cursor(editor, key) {
            modified = true;
        } else if let Some(bond_id) = editor.hovered_bond.or(editor.hotspot_bond) {
            editor.push_undo();
            if dispatch_bond_key(editor, bond_id, key) { modified = true; }
        } else if let Some(atom_id) = editor.hotspot_atom {
            editor.push_undo();
            if dispatch_atom_key(editor, atom_id, key) { modified = true; }
        }
    }

    // Arrow key navigation (with or without repeat).
    //
    // Plain arrow walks the hotspot atom to the neighbour in that direction. Shift+arrow
    // instead targets the *bond* to that neighbour: bond actions (order, stereo, ring and
    // fragment fusion) are otherwise reachable only by hovering a bond, so without this the
    // keyboard cannot express a double bond or a fused ring at all.
    let (dir, shift) = ui.input(|i| {
        let d = if i.key_pressed(egui::Key::ArrowRight) { Some((1.0_f32,  0.0_f32)) }
            else if i.key_pressed(egui::Key::ArrowLeft)  { Some((-1.0, 0.0)) }
            else if i.key_pressed(egui::Key::ArrowUp)    { Some((0.0, -1.0)) }
            else if i.key_pressed(egui::Key::ArrowDown)  { Some((0.0,  1.0)) }
            else { None };
        (d, i.modifiers.shift)
    });

    if let Some((dx, dy)) = dir {
        match editor.hotspot_atom {
            Some(src_id) => {
                if let Some(next) = navigate_direction(&editor.molecule, src_id, dx, dy) {
                    if shift {
                        editor.hotspot_bond =
                            editor.molecule.bond_between(src_id, next).map(|b| b.id);
                    } else {
                        editor.hotspot_atom = Some(next);
                        // A bond target from the old position would be stale and would
                        // silently steer the next bond key at the wrong bond.
                        editor.hotspot_bond = None;
                    }
                }
            }
            // With no hotspot the arrows move the *insertion cursor* instead. Without this,
            // `last_mouse_mol` never changes in keyboard-only use, so every seeded atom lands
            // on the same coordinate and a second, separate component cannot be placed at all
            // (it snaps into the first). Space/Tab drops the hotspot to get here.
            None => {
                editor.last_mouse_mol[0] += dx * DEFAULT_BOND_LENGTH;
                editor.last_mouse_mol[1] += dy * DEFAULT_BOND_LENGTH;
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
            "Space" | "Tab" => {
                editor.hotspot_atom = None;
                editor.hotspot_bond = None;
                continue;
            }
            "g" | "G" => {
                // Fall back to the hotspot: this is the only key that grows the selection,
                // so requiring a hovered atom left selection (and therefore Delete, move and
                // rotate) completely unreachable without a pointer.
                if let Some(id) = editor.hovered_atom.or(editor.hotspot_atom) {
                    editor.hotspot_atom = Some(id);
                    editor.selected_atoms.insert(id);
                }
                continue;
            }
            _ => {}
        }

        if !ctrl && !alt {
            let atom_target = editor.hovered_atom.or(editor.hotspot_atom);
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
                // `insert_fragment` merges the fragment's attach atom into the existing one by
                // ID only — it doesn't touch its element/charge. Set those first (mirrors
                // `expand_label`), or e.g. pressing 'o' on a C would bond a new O onto an atom
                // still labeled "C" instead of turning it into the O of an O-H.
                let root_elem = frag.atoms[frag.attach_idx].element.clone();
                let root_charge = frag.atoms[frag.attach_idx].charge;
                if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
                    atom.element = root_elem;
                    atom.charge = root_charge;
                }

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
        BondAction::Fragment { fragment } => {
            // Fuse a named fragment (e.g. benzene) onto the bond, sharing it as one edge.
            // Ignore unknown fragment names.
            if let Some(frag) = editor.config.resolve_fragment(fragment) {
                fuse_fragment_onto_bond(editor, bond_id, &frag);
            }
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

/// Fuse a named fragment onto a bond, sharing that bond as one edge.
///
/// The fragment's attach edge (`attach_idx` → `next`) is mapped rigidly onto the existing bond's
/// endpoints with a similarity transform (translation + rotation + uniform scale). A reflection is
/// chosen so the fragment's remaining atoms land on the side AWAY from existing neighbor atoms of
/// the bond's endpoints. New atoms snap to existing atoms within SNAP_RADIUS. The shared edge's
/// order is updated to the fragment's attach-edge order (so e.g. benzene ends up fully alternating
/// relative to the shared bond).
fn fuse_fragment_onto_bond(
    editor: &mut ChemStructEditor,
    bond_id: u32,
    frag: &crate::molecule::fragment::Fragment,
) {
    let n = frag.atoms.len();
    if n < 2 { return; }
    let attach = frag.attach_idx;
    let next = (attach + 1) % n;

    let (a_id, b_id) = {
        let Some(bond) = editor.molecule.bonds.iter().find(|b| b.id == bond_id) else { return };
        (bond.begin, bond.end)
    };
    let (pa, pb) = {
        let Some(a) = editor.molecule.atom_by_id(a_id) else { return };
        let Some(b) = editor.molecule.atom_by_id(b_id) else { return };
        (a.pos, b.pos)
    };

    // Raw fragment attach-edge endpoints.
    let p0 = frag.atoms[attach].pos;
    let p1 = frag.atoms[next].pos;
    let fdx = p1[0] - p0[0];
    let fdy = p1[1] - p0[1];
    let flen = (fdx * fdx + fdy * fdy).sqrt();
    if flen < 1e-6 { return; }

    let bdx = pb[0] - pa[0];
    let bdy = pb[1] - pa[1];
    let blen = (bdx * bdx + bdy * bdy).sqrt();
    if blen < 1e-6 { return; }

    let scale = blen / flen;
    // Rotation that takes the (normalized) fragment edge onto the (normalized) bond edge.
    let fang = fdy.atan2(fdx);
    let bang = bdy.atan2(bdx);

    // Transform a raw fragment point with the given handedness (flip = mirror across the edge).
    let transform = |p: [f32; 2], flip: bool| -> [f32; 2] {
        // Translate so p0 is at origin.
        let mut x = p[0] - p0[0];
        let mut y = p[1] - p0[1];
        // Optional reflection across the fragment edge axis: rotate edge to x-axis, flip y, rotate back.
        if flip {
            let (s, c) = (-fang).sin_cos();
            let rx = x * c - y * s;
            let ry = x * s + y * c;
            let ry = -ry;
            let (s2, c2) = fang.sin_cos();
            x = rx * c2 - ry * s2;
            y = rx * s2 + ry * c2;
        }
        // Scale + rotate from fragment edge angle to bond edge angle, then translate to pa.
        let dang = bang - fang;
        let (s, c) = dang.sin_cos();
        let sx = x * scale;
        let sy = y * scale;
        [pa[0] + sx * c - sy * s, pa[1] + sx * s + sy * c]
    };

    // Existing neighbor atom positions (excluding a_id/b_id themselves) to clear from.
    let mut neighbors: Vec<[f32; 2]> = Vec::new();
    for &nid in editor.molecule.neighbor_atom_ids(a_id).iter()
        .chain(editor.molecule.neighbor_atom_ids(b_id).iter())
    {
        if nid == a_id || nid == b_id { continue; }
        if let Some(at) = editor.molecule.atom_by_id(nid) {
            neighbors.push(at.pos);
        }
    }

    // Score a handedness by the minimum clearance of the fragment's non-edge atoms from neighbors.
    let clearance = |flip: bool| -> f32 {
        let mut min_clear = f32::MAX;
        for (i, fa) in frag.atoms.iter().enumerate() {
            if i == attach || i == next { continue; }
            let q = transform(fa.pos, flip);
            for nb in &neighbors {
                let dx = q[0] - nb[0];
                let dy = q[1] - nb[1];
                let d = (dx * dx + dy * dy).sqrt();
                if d < min_clear { min_clear = d; }
            }
        }
        min_clear
    };

    let flip = if neighbors.is_empty() {
        false
    } else {
        clearance(true) > clearance(false)
    };

    // Build id_map: attach→a_id, next→b_id, rest transformed + snapped or created.
    let snap = crate::molecule::Molecule::SNAP_RADIUS;
    let mut id_map: Vec<u32> = vec![0; n];
    id_map[attach] = a_id;
    id_map[next] = b_id;
    for (i, fa) in frag.atoms.iter().enumerate() {
        if i == attach || i == next { continue; }
        let pos = transform(fa.pos, flip);
        let id = editor.molecule.find_atom_near(pos, snap)
            .unwrap_or_else(|| editor.molecule.add_atom(fa.element.clone(), pos, fa.charge));
        id_map[i] = id;
    }

    // The shared edge's original order, kept so the repair pass below can restore it.
    let shared_order = editor.molecule.bond_between(a_id, b_id).map(|b| b.order.clone());

    // Add fragment bonds. The shared edge already exists: update its order instead of adding.
    let mut touched: Vec<(u32, u32)> = Vec::new();
    for fb in &frag.bonds {
        let u = id_map[fb.begin];
        let v = id_map[fb.end];
        if u == v { continue; }
        if let Some(existing) = editor.molecule.bond_between(u, v) {
            let existing_id = existing.id;
            if let Some(bond) = editor.molecule.bond_by_id_mut(existing_id) {
                bond.order = fb.order.clone();
            }
        } else {
            editor.molecule.add_bond(u, v, fb.order.clone());
        }
        touched.push((u, v));
    }

    // A fragment's bond orders describe a *free-standing* ring, so they know nothing about
    // the double bonds the two fusion atoms already carry. Adopting them verbatim can leave
    // a carbon with two double bonds — fusing benzene onto benzene did exactly that, giving
    // pentavalent ring-fusion carbons instead of naphthalene.
    //
    // Only re-derive when the fragment's own pattern is actually invalid, so saturated
    // fragments (cyclohexane, piperidine, …) keep their orders untouched.
    if touched.iter().any(|&(u, v)| double_count(&editor.molecule, u) > 1
                                 || double_count(&editor.molecule, v) > 1)
    {
        // The pre-existing bond was already consistent with the rest of the molecule; the
        // fragment has no standing to change it.
        if let (Some(order), Some(id)) =
            (shared_order, editor.molecule.bond_between(a_id, b_id).map(|b| b.id))
        {
            if let Some(bond) = editor.molecule.bond_by_id_mut(id) {
                bond.order = order;
            }
        }
        let is_shared = |u: u32, v: u32| (u == a_id && v == b_id) || (u == b_id && v == a_id);

        // Clear the fragment's doubles before re-deriving. Without this, deciding bond k
        // would count doubles on bonds k+1.. that are still carrying the fragment's own
        // pattern, so every bond looks blocked and the ring comes out fully saturated.
        for &(u, v) in &touched {
            if is_shared(u, v) { continue; }
            if let Some(id) = editor.molecule.bond_between(u, v).map(|b| b.id) {
                if let Some(bond) = editor.molecule.bond_by_id_mut(id) {
                    // Never demote a triple bond: it is an explicit choice, not alternation.
                    if bond.order == BondOrder::Double {
                        bond.order = BondOrder::Single;
                    }
                }
            }
        }

        // Walk the newly contributed bonds in fragment order — for a ring fused on one edge
        // this is the path from one fusion atom to the other — making each one double only
        // where both ends still need one. This reproduces the Kekulé structure the fused
        // system actually admits, rather than the isolated ring's.
        for &(u, v) in &touched {
            if is_shared(u, v) { continue; }
            if double_count(&editor.molecule, u) != 0 || double_count(&editor.molecule, v) != 0 {
                continue;
            }
            if let Some(id) = editor.molecule.bond_between(u, v).map(|b| b.id) {
                if let Some(bond) = editor.molecule.bond_by_id_mut(id) {
                    if bond.order != BondOrder::Triple {
                        bond.order = BondOrder::Double;
                    }
                }
            }
        }
    }
}

/// How many double bonds `atom_id` currently carries.
fn double_count(mol: &crate::molecule::Molecule, atom_id: u32) -> usize {
    mol.bonds.iter()
        .filter(|b| (b.begin == atom_id || b.end == atom_id) && b.order == BondOrder::Double)
        .count()
}

/// Find the neighbor atom most in direction (dx, dy) from source.
///
/// `pub(crate)` so the keyboard solver can enumerate arrow-key navigation without replaying
/// frames: a duplicate of this rule in the search would silently drift from the real one.
pub(crate) fn navigate_direction(
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

/// Centroid (molecule coords) of the current selection, or origin when empty.
fn selection_centroid(editor: &ChemStructEditor) -> [f32; 2] {
    let (mut cx, mut cy, mut n) = (0.0_f32, 0.0_f32, 0u32);
    for &id in &editor.selected_atoms {
        if let Some(a) = editor.molecule.atom_by_id(id) {
            cx += a.pos[0];
            cy += a.pos[1];
            n += 1;
        }
    }
    if n == 0 { [0.0, 0.0] } else { [cx / n as f32, cy / n as f32] }
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

pub(crate) fn key_to_str(key: egui::Key, shift: bool) -> Option<String> {
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

/// If `key` names an element, drop a bare atom of it at the cursor (`last_mouse_mol`).
///
/// Applies to any fragment whose root (attach) atom is a heteroatom — so "O"/"o" (O, OH),
/// "N"/"n" (N, NH2), "s" (SH), halogens, etc. all place that atom — and to a lone carbon
/// ("c"/"m", Me) **only while there is no hotspot**.
///
/// That carbon exception is what lets a structure be started at all: on an empty canvas
/// nothing is hovered and no hotspot exists, so without it no carbon skeleton could begin.
/// Multi-atom carbon-rooted fragments (chains, benzene) always fall through to the
/// build-from-hotspot path.
///
/// **Requires a live pointer once a hotspot exists.** With the mouse, dropping a new atom
/// wherever the cursor sits — ahead of a stale hotspot — is the useful behaviour. With the
/// keyboard there is no cursor to aim: `last_mouse_mol` is a leftover, so placing there
/// stranded every heteroatom as its own component (`c 1 o` gave a loose oxygen instead of an
/// alcohol) and made most structures unbuildable. Deferring to the hotspot instead lets
/// `dispatch_atom_key` retarget that atom, which is how a heteroatom joins the structure.
/// Space/Tab drops the hotspot when a separate component really is wanted.
fn place_element_at_cursor(editor: &mut ChemStructEditor, key: &str) -> bool {
    if !editor.pointer_active && editor.hotspot_atom.is_some() {
        return false;
    }

    let actions: Vec<crate::config::AtomAction> = editor.config.atom_shortcuts.iter()
        .filter(|s| s.key == key && !s.ctrl && !s.alt)
        .map(|s| s.action.clone())
        .collect();

    for action in &actions {
        if let Some(crate::config::ResolvedAtomAction::InsertFragment(frag)) =
            editor.config.atom_action_to_fragment(action)
        {
            let root = &frag.atoms[frag.attach_idx];
            let is_single = frag.atoms.len() == 1 && frag.bonds.is_empty();
            if is_single || root.element != "C" {
                editor.push_undo();
                let new_id = editor.molecule.add_atom(
                    root.element.clone(),
                    editor.last_mouse_mol,
                    root.charge,
                );
                editor.hotspot_atom = Some(new_id);
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AtomAction, AtomShortcut};
    use crate::testkit::run_frame;

    /// Regression test: dragging from an existing atom out into empty space (Bond tool)
    /// should draw a bond to a newly created atom at the drop point, not silently do
    /// nothing (see the `process_bond_tool` drag_stopped handler).
    #[test]
    fn bond_tool_drag_from_atom_to_empty_space_creates_bonded_atom() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        assert_eq!(editor.tool, crate::Tool::Bond);

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let p1 = egui::pos2(300.0, 200.0);
        let p2 = egui::pos2(450.0, 280.0); // far from p1, empty space

        // Frame 1: press, then (frame 1b) release on empty canvas -> places the first atom
        // exactly at p1. Split across two frames: egui's click detection needs the release
        // to arrive in a later input batch than the press, like a real mouse click would.
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p1)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);
        assert_eq!(editor.molecule.atoms.len(), 1, "click on empty canvas should place one atom");
        let atom_id = editor.molecule.atoms[0].id;
        let pos_before = editor.molecule.atom_by_id(atom_id).unwrap().pos;

        // Frame 2: press down exactly on that atom.
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]);
        // Frame 3: drag out to p2 (well past any drag threshold).
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerMoved(p2),
        ]);
        // Frame 4: release at p2, in empty space.
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p2, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);

        assert_eq!(editor.molecule.atoms.len(), 2, "dragging from an atom to empty space should create a new atom");
        assert_eq!(editor.molecule.bonds.len(), 1, "dragging from an atom to empty space should bond it to the new atom");
        let pos_after = editor.molecule.atom_by_id(atom_id).unwrap().pos;
        assert_eq!(pos_before, pos_after, "the source atom must stay put, not be moved by the drag");
    }

    /// Dragging from one existing atom onto another draws a bond between them (both atoms
    /// keep their own identity/position — this is a bond, not a merge).
    #[test]
    fn bond_tool_drag_from_atom_to_atom_creates_bond() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let p1 = egui::pos2(300.0, 200.0);
        let p2 = egui::pos2(450.0, 280.0);

        // Place two atoms, one at p1 and one at p2.
        for p in [p1, p2] {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p)]);
            run_frame(&ctx, screen_rect, &mut editor, vec![
                egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            ]);
            run_frame(&ctx, screen_rect, &mut editor, vec![
                egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
            ]);
        }
        assert_eq!(editor.molecule.atoms.len(), 2);
        assert!(editor.molecule.bonds.is_empty());

        // Drag from the atom at p1 onto the atom at p2.
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p1, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]);
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p2)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p2, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);

        assert_eq!(editor.molecule.atoms.len(), 2, "both atoms must still exist — this bonds, it doesn't merge");
        assert_eq!(editor.molecule.bonds.len(), 1, "dragging atom-to-atom should draw exactly one bond");
    }

    /// Regression test: deleting with the Eraser tool must be undoable, like every other
    /// mutation. Without a `push_undo()` before the delete, Ctrl+Z after an eraser click
    /// doesn't restore the deleted atom — it skips past it to whatever state preceded the
    /// *previous* edit, silently losing the deleted atom for good.
    #[test]
    fn eraser_tool_delete_is_undoable() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        editor.molecule.add_bond(a, b, BondOrder::Single);
        editor.tool = crate::Tool::Eraser;

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let center = screen_rect.center();
        let p_a = editor.mol_to_screen([0.0, 0.0], center);

        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(p_a)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p_a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: p_a, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);

        assert_eq!(editor.molecule.atoms.len(), 1, "eraser click should delete the hovered atom");
        assert!(editor.undo(), "the delete must have pushed an undo snapshot");
        assert_eq!(editor.molecule.atoms.len(), 2, "undo should restore the deleted atom");
        assert_eq!(editor.molecule.bonds.len(), 1, "undo should restore its bond too");
    }

    fn frag_shortcut(key: &str, fragment: &str) -> AtomShortcut {
        AtomShortcut {
            key: key.to_string(),
            ctrl: false,
            alt: false,
            action: AtomAction::Fragment { fragment: fragment.to_string() },
        }
    }

    /// Editor with a controlled element-shortcut set (independent of the user's on-disk config).
    /// The referenced fragments (O/OH/N/NH2/SH/Br/benzene) are all built-in.
    fn editor() -> ChemStructEditor {
        let mut e = ChemStructEditor::default();
        e.config.atom_shortcuts = vec![
            frag_shortcut("O", "O"),
            frag_shortcut("o", "OH"),
            frag_shortcut("N", "N"),
            frag_shortcut("n", "NH2"),
            frag_shortcut("s", "SH"),
            frag_shortcut("b", "Br"),
            frag_shortcut("a", "benzene"),
        ];
        e
    }

    /// Element keys (single-atom or heteroatom-rooted fragments) drop a lone atom of that
    /// element at the cursor — no stray carbon, no attached H group.
    #[test]
    fn element_keys_place_bare_atom_at_cursor() {
        for (key, want) in [("O", "O"), ("o", "O"), ("N", "N"), ("n", "N"), ("s", "S"), ("b", "Br")] {
            let mut e = editor();
            e.last_mouse_mol = [2.0, 3.0];
            assert!(place_element_at_cursor(&mut e, key), "key '{key}' should place an atom");
            assert_eq!(e.molecule.atoms.len(), 1, "key '{key}' must add exactly one atom");
            let atom = &e.molecule.atoms[0];
            assert_eq!(atom.element, want, "key '{key}' should place element {want}");
            assert_eq!(atom.pos, [2.0, 3.0], "key '{key}' should place at the cursor");
            assert!(e.molecule.bonds.is_empty(), "key '{key}' must not create bonds");
        }
    }

    /// Multi-atom carbon-rooted fragments (benzene) are NOT placed at the cursor; they fall
    /// through to the build-from-hotspot path.
    #[test]
    fn carbon_multi_fragments_do_not_place_at_cursor() {
        let mut e = editor();
        assert!(!place_element_at_cursor(&mut e, "a"), "benzene should not place at cursor");
        assert!(e.molecule.atoms.is_empty());
    }

    /// Fusing benzene onto a single bond yields a 6-membered ring sharing that bond:
    /// 6 atoms, 6 ring bonds, 3 double bonds.
    #[test]
    fn fuse_benzene_onto_bond_builds_aromatic_ring() {
        let mut e = editor();
        let a = e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = e.molecule.add_atom("C".to_string(), [1.0, 0.0], 0);
        let bond_id = e.molecule.add_bond(a, b, BondOrder::Single);

        let frag = e.config.resolve_fragment("benzene").expect("benzene fragment");
        fuse_fragment_onto_bond(&mut e, bond_id, &frag);

        assert_eq!(e.molecule.atoms.len(), 6, "benzene fuse should yield 6 atoms");
        assert_eq!(e.molecule.bonds.len(), 6, "benzene fuse should yield 6 ring bonds");
        let doubles = e.molecule.bonds.iter()
            .filter(|bd| bd.order == BondOrder::Double)
            .count();
        assert_eq!(doubles, 3, "aromatic benzene should have 3 double bonds");
    }
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
