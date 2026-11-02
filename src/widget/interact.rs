use super::{ChemStructEditor, DEFAULT_BOND_LENGTH};
use crate::config::{AtomAction, BondAction, ResolvedAtomAction, SelectionAction};
use crate::molecule::{ArrowKind, BondOrder, BondStereo, PageArrow, PageObject};
use crate::molecule::page::{snap_direction, DEFAULT_ARROW_LENGTH};

// ═══════════════════════════════════════════════════════════════════════════════
// Group-hydrogen bookkeeping
// ═══════════════════════════════════════════════════════════════════════════════

/// Strip the group hydrogens that the bonds just drawn have left no room for on `atoms`.
///
/// The rule lives on [`Molecule::consume_excess_group_h`]; what belongs *here* is deciding
/// when it applies. It is called from the **editing** paths only — every place a user action
/// adds a bond or raises a bond order — and deliberately not hooked into
/// [`Molecule::add_bond`](crate::molecule::Molecule::add_bond), which is also how ChemDraw
/// import, the cleanup/canonicalisation passes and fragment→molecule conversion build their
/// graphs. Those must reproduce their input exactly, hydrogens included.
///
/// Only the two atoms of a new bond can have gained valence, so callers pass those (plus, for
/// a fragment or ring insertion, everything the insertion touched).
///
/// Editor state pointing at a removed hydrogen is cleaned up here; the atom hotspot is left
/// to the caller, which knows what it should fall back to.
pub(crate) fn consume_group_h(editor: &mut ChemStructEditor, atoms: &[u32]) {
    for &id in atoms {
        for h in editor.molecule.consume_excess_group_h(id) {
            editor.selected_atoms.remove(&h);
            if editor.hovered_atom == Some(h) { editor.hovered_atom = None; }
        }
    }
}

/// Cycle a bond's order (the mouse gesture for "make this a double bond"), consuming any
/// group hydrogen the *increase* leaves no room for. A wrap back to single removes nothing.
fn cycle_bond_order(editor: &mut ChemStructEditor, bond_id: u32) {
    let Some(bond) = editor.molecule.bond_by_id(bond_id) else {
        return;
    };
    edit_bond_type(editor, bond_id, bond.order.cycle(), bond.stereo.clone());
}

/// Apply a manual bond edit. Ring matching writes orders directly and keeps its own record.
pub(crate) fn edit_bond_type(
    editor: &mut ChemStructEditor,
    bond_id: u32,
    order: BondOrder,
    stereo: BondStereo,
) {
    let Some(bond) = editor.molecule.bond_by_id_mut(bond_id) else {
        return;
    };
    let grew = order.valence() > bond.order.valence();
    bond.order = order;
    bond.stereo = stereo;
    let (begin, end) = (bond.begin, bond.end);
    editor.molecule.press_leftovers.remove(&begin);
    editor.molecule.press_leftovers.remove(&end);
    if grew {
        consume_group_h(editor, &[begin, end]);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Primary-button drags
// ═══════════════════════════════════════════════════════════════════════════════

/// The phases of a drag made with the primary button, for the tools' drag gestures.
///
/// egui's `drag_started()` / `dragged()` / `drag_stopped()` fire for a drag by *any* button,
/// but the middle button belongs to the camera (`ChemStructEditor::ui` pans with it), so the
/// tools read their drags through this instead.
struct PrimaryDrag {
    started: bool,
    dragged: bool,
    stopped: bool,
}

impl PrimaryDrag {
    fn of(response: &egui::Response, ui: &egui::Ui) -> Self {
        let (held, released) = ui.input(|i| {
            (i.pointer.button_down(egui::PointerButton::Primary),
             i.pointer.button_released(egui::PointerButton::Primary))
        });
        Self {
            started: response.drag_started() && held,
            dragged: response.dragged() && held,
            // A primary drag ends by releasing the button, or is aborted (Escape) while it is
            // still held. `drag_stopped_by(Primary)` would only see the release, leaving the
            // gesture's state behind after an abort.
            stopped: response.drag_stopped() && (released || held),
        }
    }
}

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
    let drag = PrimaryDrag::of(response, ui);

    // ── Drag tracking ────────────────────────────────────────────────────────
    // Dragging *from an atom* always draws a bond, never moves it — the Bond tool's own
    // gesture list is "drag atom→atom to draw a bond"; repositioning atoms via drag is the
    // Select tool's job (`process_select_tool`), not this one.
    //
    // Dragging *from empty canvas* (no atom and no bond under the press) instead runs the same
    // rubber-band selection the Select tool offers, so a user can marquee-select without
    // leaving the Bond tool. A drag that begins on a bond keeps the Bond tool's old no-op.
    if drag.started {
        // `drag_started()` only fires once the pointer has moved past egui's drag threshold,
        // so by now `mouse` (the *current* position) may already be well past the atom the
        // drag actually started on. Hit-test at the press origin instead, or a fast drag off
        // an atom silently does nothing (see `bond_tool_drag_from_atom_to_empty_space_creates_bonded_atom`).
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(mouse);
        // Prefer hit atom; fall back to currently hovered atom for slight-miss tolerance
        editor.bond_start = editor.hit_test_atom(origin, center).or(editor.hovered_atom);
        if editor.bond_start.is_some() {
            editor.push_undo(); // snapshot before the drag adds a bond/atom
            editor.preview_end_screen = Some(mouse);
        } else if editor.hit_test_bond(origin, center).is_none() {
            // Empty space: start a rubber-band selection instead of a bond. No undo snapshot —
            // selection does not mutate the molecule.
            lasso_begin(editor, origin);
        }
    }

    if drag.dragged {
        if editor.bond_start.is_some() {
            editor.preview_end_screen = Some(mouse);
        } else if editor.drag_origin_screen.is_some() {
            // Extend the rubber-band path; `draw_overlays` renders it, same as the Select tool.
            editor.lasso_path.push(mouse);
        }
    }

    if drag.stopped {
        if editor.bond_start.is_none() && editor.drag_origin_screen.is_some() {
            // Empty-space drag: commit the rubber-band selection and tear its state down.
            lasso_select(editor, center);
            editor.lasso_path.clear();
            editor.drag_origin_screen = None;
            editor.preview_end_screen = None;
        } else {
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
                        cycle_bond_order(editor, existing);
                        modified = true;
                    } else {
                        let bid = editor.molecule.add_bond(src_id, target_id, editor.current_bond_order.clone());
                        if bid != 0 {
                            if let Some(bond) = editor.molecule.bond_by_id_mut(bid) {
                                bond.stereo = editor.current_bond_stereo.clone();
                            }
                            consume_group_h(editor, &[src_id, target_id]);
                        }
                        modified = true;
                    }
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
                    cycle_bond_order(editor, existing);
                    modified = true;
                } else {
                    let bid = editor.molecule.add_bond(atom_id, new_id, editor.current_bond_order.clone());
                    if bid != 0 {
                        if let Some(bond) = editor.molecule.bond_by_id_mut(bid) {
                            bond.stereo = editor.current_bond_stereo.clone();
                        }
                        consume_group_h(editor, &[atom_id, new_id]);
                    }
                    modified = true;
                }
            }
            editor.hotspot_atom = Some(new_id);
        } else if let Some(bond_id) = editor.hovered_bond {
            if editor.molecule.bond_by_id_mut(bond_id).is_some() {
                cycle_bond_order(editor, bond_id);
                modified = true;
            }
        } else {
            let mol_pos = editor.screen_to_mol(mouse, center);
            let new_id = editor.molecule.add_atom(editor.current_element.clone(), mol_pos, 0);
            editor.hotspot_atom = Some(new_id);
            modified = true;
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
    enum Target { Atom(u32), Bond(u32), Object(u32) }

    let target = editor.hovered_atom.map(Target::Atom)
        .or_else(|| editor.hovered_object.map(Target::Object))
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
        Some(Target::Object(id)) => {
            editor.push_undo();
            editor.molecule.remove_object(id);
            editor.selected_objects.remove(&id);
            editor.hovered_object = None;
            true
        }
        None => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rubber-band (lasso) selection — shared by the Select and Bond tools
// ═══════════════════════════════════════════════════════════════════════════════

/// Begin a rubber-band selection at `origin`, the pointer's press origin: reset the
/// in-progress path to start there and remember it as the drag origin.
///
/// The path starts at the press, not at the pointer when `drag_started()` fires: by then the
/// pointer is past egui's drag threshold, possibly far past it after a fast first move, and
/// the polygon would lose the stretch between the two.
///
/// Factored out so the Bond tool can start the exact same gesture the Select tool does when a
/// drag begins over empty canvas (no atom and no bond under the press).
fn lasso_begin(editor: &mut ChemStructEditor, origin: egui::Pos2) {
    editor.lasso_path.clear();
    editor.lasso_path.push(origin);
    editor.drag_origin_screen = Some(origin);
}

/// Finish a rubber-band selection: replace the selection with every atom whose screen position
/// falls inside the drawn polygon. A bond is taken through its atoms: one with both ends inside
/// is covered (Cleanup moves both, Delete removes it with them), one with an end outside is
/// not, wherever its midpoint lies. Bonds clicked into the selection
/// ([`ChemStructEditor::selected_bonds`]) are dropped with the rest of the old selection. A
/// polygon needs at least three vertices to bound an area, so a path too short to be a real
/// lasso (a mere flick) selects nothing and leaves the selection untouched.
///
/// Does not clear `lasso_path`/`drag_origin_screen`; the caller owns that teardown, since the
/// Select tool clears them on every drag-stop regardless of which sub-gesture actually ran.
fn lasso_select(editor: &mut ChemStructEditor, center: egui::Pos2) {
    if editor.lasso_path.len() > 2 {
        editor.selected_atoms.clear();
        editor.selected_bonds.clear();
        editor.selected_objects.clear();
        let lasso = editor.lasso_path.clone();
        let hits: Vec<u32> = editor.molecule.atoms.iter().filter_map(|atom| {
            let sp = editor.mol_to_screen(atom.pos, center);
            if is_point_in_polygon(sp, &lasso) { Some(atom.id) } else { None }
        }).collect();
        editor.selected_atoms.extend(hits);
        // A page object is taken when it lies inside whole: both ends of an arrow, a text's
        // anchor, all the atoms of a bracket.
        let objects: Vec<u32> = editor.molecule.objects.iter().filter(|o| match o {
            PageObject::Bracket(b) => {
                let atoms: Vec<u32> = b.atoms.iter().copied().filter(|a| editor.molecule.atom_by_id(*a).is_some()).collect();
                !atoms.is_empty() && atoms.iter().all(|a| editor.selected_atoms.contains(a))
            }
            _ => o.key_points().iter().all(|p| is_point_in_polygon(editor.mol_to_screen(*p, center), &lasso)),
        }).map(PageObject::id).collect();
        editor.selected_objects.extend(objects);
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
    let drag = PrimaryDrag::of(response, ui);

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
            editor.selected_bonds.clear();
        } else if let Some(id) = editor.hovered_object {
            // Double-click a text (or a bracket, for its label) to edit it, a table's cell to
            // enter its value.
            if editor.molecule.object_by_id(id).is_some_and(|o| o.as_table().is_some()) {
                // The release that completes a double-click may carry no interaction position.
                let at = response
                    .interact_pointer_pos()
                    .or_else(|| ui.input(|i| i.pointer.latest_pos()))
                    .unwrap_or(mouse);
                if let Some(cell) = editor.table_cell_at(id, at) {
                    modified |= editor.edit_table_cell(id, cell);
                }
            } else {
                editor.edit_text(id);
            }
        }
    } else if response.clicked() {
        editor.lasso_path.clear();
        if let Some(atom_id) = editor.hovered_atom {
            if editor.selected_atoms.contains(&atom_id) {
                editor.selected_atoms.remove(&atom_id);
            } else {
                editor.selected_atoms.insert(atom_id);
            }
        } else if let Some(id) = editor.hovered_object {
            if !editor.selected_objects.remove(&id) {
                editor.selected_objects.insert(id);
            }
        } else if let Some(bond_id) = editor.hovered_bond {
            // A bond on its own (see `ChemStructEditor::selected_bonds`).
            if !editor.selected_bonds.remove(&bond_id) {
                editor.selected_bonds.insert(bond_id);
            }
        } else {
            editor.selected_atoms.clear();
            editor.selected_bonds.clear();
            editor.selected_objects.clear();
        }
    }

    if drag.started {
        // As in the Bond tool, pick what the drag acts on at the press origin: by the time
        // `drag_started()` fires the pointer may already have left the atom it was pressed on.
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(mouse);
        lasso_begin(editor, origin);
        editor.object_drag = None;
        // Alt+drag with a selection rotates the molecule around its centroid.
        if ui.input(|i| i.modifiers.alt) && !editor.selected_atoms.is_empty() {
            editor.push_undo();
            let c = selection_centroid(editor);
            let mp = editor.screen_to_mol(mouse, center);
            editor.rotate_last_angle = Some((mp[1] - c[1]).atan2(mp[0] - c[0]));
            editor.dragging_atom = None;
        } else {
            editor.rotate_last_angle = None;
            editor.dragging_atom = None;
            if let Some(end) = arrow_end_at(editor, origin, center) {
                // An end of the lone selected arrow: drag that end.
                editor.push_undo();
                editor.object_drag = Some(end);
            } else {
                editor.dragging_atom = editor.hit_test_atom(origin, center);
                if editor.dragging_atom.is_some() {
                    // Snapshot once, before the first frame of movement, so the whole move is
                    // one undo step (the Alt-rotation branch above does the same).
                    editor.push_undo();
                } else if let Some(id) = editor.hit_test_object(origin, center) {
                    // A page object: move it, or the whole selection when it is part of it.
                    editor.push_undo();
                    editor.object_drag = Some(ObjectDrag::Move { with_selection: editor.selected_objects.contains(&id), id });
                }
            }
        }
    }

    if drag.dragged {
        if let Some(last) = editor.rotate_last_angle {
            // Rotate the selection by the change in cursor angle about its centroid.
            let c = selection_centroid(editor);
            let mp = editor.screen_to_mol(mouse, center);
            let angle = (mp[1] - c[1]).atan2(mp[0] - c[0]);
            let ids: Vec<u32> = editor.selected_atoms.iter().copied().collect();
            rotate_atoms(editor, &ids, angle - last);
            for id in editor.selected_objects.clone() {
                if let Some(o) = editor.molecule.object_by_id_mut(id) {
                    o.rotate_about(c, angle - last);
                }
            }
            editor.rotate_last_angle = Some(angle);
            modified = true;
        } else if let Some(atom_id) = editor.dragging_atom {
            let [dx, dy] = editor.view(center).screen_delta_to_mol(response.drag_delta());
            let with_selection = editor.selected_atoms.contains(&atom_id);
            let atoms_to_move: Vec<u32> = if with_selection {
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
            if with_selection {
                let moved = editor.selected_atoms.clone();
                move_objects(editor, &editor.selected_objects.clone(), [dx, dy], &moved);
            }
            modified = true;
        } else if let Some(drag) = editor.object_drag.clone() {
            match drag {
                ObjectDrag::Move { id, with_selection } => {
                    let delta = editor.view(center).screen_delta_to_mol(response.drag_delta());
                    if with_selection {
                        let moved = editor.selected_atoms.clone();
                        move_objects(editor, &editor.selected_objects.clone(), delta, &moved);
                        let atoms: Vec<u32> = editor.selected_atoms.iter().copied().collect();
                        move_atoms(editor, &atoms, delta);
                    } else {
                        move_objects(editor, &[id].into_iter().collect(), delta, &Default::default());
                    }
                }
                ObjectDrag::ArrowEnd { id, head } => {
                    let free = ui.input(|i| i.modifiers.shift);
                    let to = editor.screen_to_mol(mouse, center);
                    if let Some(PageObject::Arrow(a)) = editor.molecule.object_by_id_mut(id) {
                        let (fixed, moving) = if head { (a.tail, &mut a.head) } else { (a.head, &mut a.tail) };
                        *moving = if free { to } else { snap_direction(fixed, to, 15.0) };
                    }
                }
                ObjectDrag::NewArrow { .. } => {}
            }
            modified = true;
        } else {
            editor.lasso_path.push(mouse);
        }
    }

    if drag.stopped {
        if editor.rotate_last_angle.take().is_some() {
            // rotation finished
        } else if editor.dragging_atom.is_some() {
            editor.dragging_atom = None;
        } else if editor.object_drag.take().is_some() {
            // move finished
        } else {
            lasso_select(editor, center);
        }
        editor.lasso_path.clear();
        editor.drag_origin_screen = None;
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────
    // Delete/Backspace: remove the selection, or fall back to the hovered atom/bond.
    let delete_pressed =
        ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete_pressed {
        if editor.delete_selection() {
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
        } else if let Some(id) = editor.hovered_object {
            editor.push_undo();
            editor.molecule.remove_object(id);
            editor.selected_objects.remove(&id);
            editor.hovered_object = None;
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
// Page objects: the Arrow and Text tools, and dragging objects
// ═══════════════════════════════════════════════════════════════════════════════

/// A drag on page objects in progress.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ObjectDrag {
    /// The Arrow tool drawing a new arrow, from where the drag began to the pointer.
    NewArrow { tail: [f32; 2], head: [f32; 2], kind: ArrowKind },
    /// Moving page object `id` with the Select tool: alone, or with the whole selection (atoms
    /// included) when it is part of it.
    Move { id: u32, with_selection: bool },
    /// Dragging one end of arrow `id` (its head, or its tail).
    ArrowEnd { id: u32, head: bool },
}

/// The arrow that is the whole selection, whose ends can then be dragged.
pub(crate) fn lone_selected_arrow(editor: &ChemStructEditor) -> Option<&PageArrow> {
    if editor.selected_objects.len() != 1 || !editor.selected_atoms.is_empty() {
        return None;
    }
    let id = *editor.selected_objects.iter().next()?;
    editor.molecule.object_by_id(id)?.as_arrow()
}

/// The end of the lone selected arrow under screen point `screen`, if any.
fn arrow_end_at(editor: &ChemStructEditor, screen: egui::Pos2, center: egui::Pos2) -> Option<ObjectDrag> {
    let a = lone_selected_arrow(editor)?;
    let near = |p: [f32; 2]| editor.mol_to_screen(p, center).distance(screen) <= super::NODE_HIT_RADIUS_PX * 0.7;
    if near(a.head) {
        Some(ObjectDrag::ArrowEnd { id: a.id, head: true })
    } else if near(a.tail) {
        Some(ObjectDrag::ArrowEnd { id: a.id, head: false })
    } else {
        None
    }
}

/// Move page objects `ids` by `delta`. A bracket stands where its atoms are, so moving it
/// moves them, except those in `moved` (which the caller moves itself).
fn move_objects(
    editor: &mut ChemStructEditor,
    ids: &std::collections::HashSet<u32>,
    delta: [f32; 2],
    moved: &std::collections::HashSet<u32>,
) {
    let mut atoms: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for o in &mut editor.molecule.objects {
        if ids.contains(&o.id()) {
            o.translate(delta);
            if let PageObject::Bracket(b) = o {
                atoms.extend(b.atoms.iter().copied().filter(|a| !moved.contains(a)));
            }
        }
    }
    for a in &mut editor.molecule.atoms {
        if atoms.contains(&a.id) {
            a.pos[0] += delta[0];
            a.pos[1] += delta[1];
        }
    }
}

/// The Arrow tool: drag from tail to head to draw an arrow of `kind` (its direction snaps to
/// 15° steps unless Shift is held), or click to place one of the default length pointing
/// right. Delete removes what is under the pointer, as in the Bond tool.
pub fn process_arrow_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
    kind: ArrowKind,
) -> bool {
    let mut modified = false;
    let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
    let drag = PrimaryDrag::of(response, ui);
    if drag.started {
        let origin = ui.input(|i| i.pointer.press_origin()).unwrap_or(mouse);
        let tail = editor.screen_to_mol(origin, center);
        editor.object_drag = Some(ObjectDrag::NewArrow { tail, head: tail, kind });
    }
    if drag.dragged {
        let free = ui.input(|i| i.modifiers.shift);
        let to = editor.screen_to_mol(mouse, center);
        if let Some(ObjectDrag::NewArrow { tail, head, .. }) = &mut editor.object_drag {
            *head = if free { to } else { snap_direction(*tail, to, 15.0) };
        }
    }
    if drag.stopped {
        if let Some(ObjectDrag::NewArrow { tail, head, kind }) = editor.object_drag.take() {
            let (dx, dy) = (head[0] - tail[0], head[1] - tail[1]);
            // A drag too short to give the arrow a direction draws nothing.
            if (dx * dx + dy * dy).sqrt() >= DEFAULT_BOND_LENGTH * 0.25 {
                editor.push_undo();
                editor.molecule.add_arrow(tail, head, kind);
                modified = true;
            }
        }
    }
    if response.clicked() {
        let tail = editor.screen_to_mol(mouse, center);
        editor.push_undo();
        editor.molecule.add_arrow(tail, [tail[0] + DEFAULT_ARROW_LENGTH, tail[1]], kind);
        modified = true;
    }
    let delete_pressed =
        ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete_pressed && delete_hovered(editor) {
        modified = true;
    }
    modified
}

/// The Text tool: a click on a text opens it for editing; a click anywhere else opens a new,
/// empty text there (the click is on its first line, at its left end). Typing goes to the text
/// box ([`ChemStructEditor::text_edit`]) until it is committed.
pub fn process_text_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    _ui: &egui::Ui,
) -> bool {
    if response.clicked() {
        let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);
        let on_text = editor
            .hit_test_object(mouse, center)
            .filter(|id| editor.molecule.object_by_id(*id).is_some_and(|o| o.as_text().is_some()));
        match on_text {
            Some(id) => {
                editor.edit_text(id);
            }
            None => {
                // The click marks the middle of the first line's capital letters, so the
                // baseline sits a little below it.
                let size = editor.config.style.label_size / super::SCALE_FACTOR;
                let at = editor.screen_to_mol(mouse, center);
                editor.text_edit = Some(super::TextEditState::new([at[0], at[1] + size * 0.35]));
            }
        }
    }
    false
}

// ═══════════════════════════════════════════════════════════════════════════════
// Key handling (Bond tool)
// ═══════════════════════════════════════════════════════════════════════════════

fn handle_keys_bond_tool(editor: &mut ChemStructEditor, ui: &egui::Ui) -> bool {
    let mut modified = false;
    let keys = pressed_key_strings(ui);

    for (key, _shift, ctrl, alt) in &keys {
        if *ctrl || *alt { continue; }
        let target = editor.keyboard_target();
        let repeat = take_ring_repeat(editor, key);

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
        //
        // A ring with nowhere to go changes nothing, and must not become a step that undo spends
        // a press on, or wipe the redo history; so the snapshot waits for the outcome.
        if let Some(atom_id) = editor.hovered_atom {
            modified |= press_on_atom(editor, atom_id, key, target, Aim::User);
        } else if place_element_at_cursor(editor, key) {
            modified = true;
        } else if let Some(bond_id) = editor.hovered_bond.or(editor.hotspot_bond) {
            let before = (editor.molecule.clone(), target);
            let outcome = dispatch_bond_key(editor, bond_id, key);
            if outcome != KeyOutcome::Nothing {
                push_undo_of(editor, before);
                modified = true;
            }
            // The mouse aims for itself, so only the keyboard's own target moves on.
            if let (KeyOutcome::RingDrawn(ring), None) = (outcome, editor.hovered_bond)
                && let Some(next) = next_bond_round_ring(editor, &ring, key)
            {
                editor.hotspot_bond = Some(next);
            }
        } else if let Some(atom_id) = repeat {
            modified |= press_on_atom(editor, atom_id, key, target, Aim::Repeat);
        } else if let Some(atom_id) = editor.hotspot_atom {
            modified |= press_on_atom(editor, atom_id, key, target, Aim::User);
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

/// Press `key` on atom `atom_id` as one undo step, if it changes anything, with `target` as where
/// undo goes back to aiming. Returns whether it did.
fn press_on_atom(
    editor: &mut ChemStructEditor,
    atom_id: u32,
    key: &str,
    target: super::KeyboardTarget,
    aim: Aim,
) -> bool {
    let before = (editor.molecule.clone(), target);
    let outcome = dispatch_atom_key(editor, atom_id, key, aim);
    let modified = outcome != KeyOutcome::Nothing;
    if modified {
        push_undo_of(editor, before);
    }
    aim_ring_repeat(editor, outcome, key);
    modified
}

/// Record `before`, the drawing as it was before the press just made and where the keyboard was
/// aiming, as an undo step.
fn push_undo_of(
    editor: &mut ChemStructEditor,
    (molecule, target): (crate::molecule::Molecule, super::KeyboardTarget),
) {
    let after = std::mem::replace(&mut editor.molecule, molecule);
    editor.push_undo_aimed_at(target);
    editor.molecule = after;
}

/// Where the keyboard target goes after `key` drew `ring`, given as a walk round it from one end
/// of the bond it was drawn on to the other: the first bond round the ring on which `key` would
/// draw another. `None` if there is no such bond.
///
/// The bond a ring was drawn on has rings on both sides, so the target has to move on for the
/// next press to do anything. Always walking round the ring in the same sense is what makes that
/// a sheet: the first free bond in that sense is the one against what is already there, so the
/// rings wind round the first one in a spiral with no gaps, where the bond opposite would give a
/// ribbon one ring wide.
fn next_bond_round_ring(editor: &mut ChemStructEditor, ring: &[u32], key: &str) -> Option<u32> {
    let positions: Vec<[f32; 2]> = ring
        .iter()
        .filter_map(|&id| editor.molecule.atom_by_id(id).map(|a| a.pos))
        .collect();
    let area: f32 = (0..positions.len())
        .map(|i| {
            let (p, q) = (positions[i], positions[(i + 1) % positions.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum();
    let mut walk = ring.to_vec();
    if area < 0.0 {
        walk.reverse();
    }

    let bonds: Vec<u32> = walk
        .windows(2)
        .filter_map(|pair| editor.molecule.bond_between(pair[0], pair[1]).map(|b| b.id))
        .collect();
    bonds
        .into_iter()
        .find(|&bond| would_draw_ring(editor, bond, key))
}

/// Whether `key` on `bond_id` would draw a ring. Asking the press itself means the answer cannot
/// differ from what the press then does.
fn would_draw_ring(editor: &mut ChemStructEditor, bond_id: u32, key: &str) -> bool {
    try_press(editor, |editor| dispatch_bond_key(editor, bond_id, key))
}

/// [`would_draw_ring`], for `key` on atom `atom_id`.
fn would_draw_ring_at_atom(editor: &mut ChemStructEditor, atom_id: u32, key: &str) -> bool {
    editor.molecule.atom_by_id(atom_id).is_some()
        && try_press(editor, |editor| {
            dispatch_atom_key(editor, atom_id, key, Aim::Repeat)
        })
}

/// Whether `press` adds a ring, leaving everything as it found it.
fn try_press(
    editor: &mut ChemStructEditor,
    press: impl FnOnce(&mut ChemStructEditor) -> KeyOutcome,
) -> bool {
    let molecule = editor.molecule.clone();
    let selected = editor.selected_atoms.clone();
    let target = editor.hotspot_atom;
    let bonds = molecule.bonds.len();
    let drawn =
        matches!(press(editor), KeyOutcome::RingDrawn(_)) && editor.molecule.bonds.len() > bonds;
    editor.molecule = molecule;
    editor.selected_atoms = selected;
    editor.hotspot_atom = target;
    drawn
}

/// After `key` drew a ring from an atom, work out where pressing it again should go on from.
///
/// The press has left the hotspot on the ring's last atom, and that is where the arrows walk on
/// from. But it can be an existing atom the ring merged with, a ring-fusion atom with rings all
/// round it, on which the next press would do nothing. As on a bond ([`next_bond_round_ring`]) the
/// target has to move on for repeated presses to go on drawing, so the same key straight after
/// acts on the last atom of the ring where it can ([`RingRepeat`](super::RingRepeat)).
///
/// That is the hotspot itself where it can, and it is still a repeat: the press only left the
/// hotspot there, so the next one is not the user pointing at that atom ([`Aim`]).
fn aim_ring_repeat(editor: &mut ChemStructEditor, outcome: KeyOutcome, key: &str) {
    let (KeyOutcome::RingDrawn(ring), Some(from)) = (outcome, editor.hotspot_atom) else {
        return;
    };
    let next = std::iter::once(from)
        .chain(ring.iter().rev().copied())
        .find(|&id| would_draw_ring_at_atom(editor, id, key));
    editor.ring_repeat = next.map(|atom| super::RingRepeat {
        key: key.to_string(),
        from,
        atom,
    });
}

/// The atom this press of `key` is the repeat of a ring press on, if it is one. Any press but a
/// chord ends the repeat, so an arrow or another key finds the hotspot as the ring left it, and
/// Ctrl+Z does not spend the repeat it has just put back.
fn take_ring_repeat(editor: &mut ChemStructEditor, key: &str) -> Option<u32> {
    editor
        .ring_repeat
        .take()
        .filter(|r| r.key == key && editor.hotspot_atom == Some(r.from))
        .map(|r| r.atom)
        .filter(|&id| editor.molecule.atom_by_id(id).is_some())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Key handling (Select tool)
// ═══════════════════════════════════════════════════════════════════════════════

fn handle_keys_select_tool(editor: &mut ChemStructEditor, ui: &egui::Ui) -> bool {
    let mut modified = false;
    let keys = pressed_key_strings(ui);

    for (key, _shift, ctrl, alt) in &keys {
        let repeat = if *ctrl || *alt {
            None
        } else {
            take_ring_repeat(editor, key)
        };

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
            let atom_target = editor.hovered_atom.or(repeat).or(editor.hotspot_atom);
            let aim = if editor.hovered_atom.is_none() && repeat.is_some() {
                Aim::Repeat
            } else {
                Aim::User
            };
            if let Some(atom_id) = atom_target {
                let outcome = dispatch_atom_key(editor, atom_id, key, aim);
                modified |= outcome != KeyOutcome::Nothing;
                aim_ring_repeat(editor, outcome, key);
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

fn dispatch_atom_key(
    editor: &mut ChemStructEditor,
    atom_id: u32,
    key: &str,
    aim: Aim,
) -> KeyOutcome {
    let actions: Vec<AtomAction> = editor.config.atom_shortcuts.iter()
        .filter(|s| s.key == key && !s.ctrl && !s.alt)
        .map(|s| s.action.clone())
        .collect();

    for action in &actions {
        let resolved = editor.config.atom_action_to_fragment(action);
        if let Some(resolved) = resolved {
            return apply_atom_action(editor, atom_id, resolved, aim);
        }
    }
    KeyOutcome::Nothing
}

/// How a press came to be on the atom it is on.
#[derive(Clone, Copy, PartialEq)]
enum Aim {
    /// The user pointed at it, with the mouse or the arrows.
    User,
    /// It is the same ring key again, going on from where the last ring left off. That can be a
    /// saturated atom the ring went through, which nobody asked to change.
    Repeat,
}

/// What a key did to the atom or bond it was pressed on.
#[derive(PartialEq)]
enum KeyOutcome {
    /// Nothing changed: no such key, or no room for what it draws.
    Nothing,
    /// The target was edited, or something with no ring through it was drawn on it.
    Edited,
    /// A ring was drawn. On a bond: its atoms, walking round from one end of the bond to the
    /// other. On an atom: its atoms in the fragment's order.
    RingDrawn(Vec<u32>),
}

fn dispatch_bond_key(editor: &mut ChemStructEditor, bond_id: u32, key: &str) -> KeyOutcome {
    let actions: Vec<BondAction> = editor.config.bond_shortcuts.iter()
        .filter(|s| s.key == key && !s.ctrl && !s.alt)
        .map(|s| s.action.clone())
        .collect();

    for action in &actions {
        let outcome = apply_bond_action(editor, bond_id, action);
        if outcome != KeyOutcome::Nothing {
            return outcome;
        }
    }
    KeyOutcome::Nothing
}

// ═══════════════════════════════════════════════════════════════════════════════
// Action applicators
// ═══════════════════════════════════════════════════════════════════════════════

/// Whether `element` carries chemistry a skeleton fragment must not silently discard.
///
/// Carbon is the skeleton's own default, and hydrogen (or an isotope label for it) is a cap
/// rather than a structural position — everything else is information the user typed in and
/// expects to still be there afterwards.
fn is_heteroatom(element: &str) -> bool {
    !matches!(element, "C" | "H" | "D" | "T")
}

/// Apply `action` to an atom. A ring with nowhere to go at that atom leaves everything as it was.
fn apply_atom_action(
    editor: &mut ChemStructEditor,
    atom_id: u32,
    action: ResolvedAtomAction,
    aim: Aim,
) -> KeyOutcome {
    let mut outcome = KeyOutcome::Edited;
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
                // ID only — it doesn't touch its element/charge. Which of the two elements wins
                // is decided here, and it depends on what kind of fragment this is:
                //
                // - **Heteroatom-rooted** (OH, NH2, OMe, NO2, …): the root element *is* what the
                //   key means, so it overwrites (mirrors `expand_label`). Pressing 'o' on a C
                //   must turn that atom into the O of an O–H, not bond a new O onto an atom
                //   still labelled "C".
                // - **Carbon-rooted** (benzene, the ring keys, Et, Ac, Boc, …): the fragment is
                //   a skeleton grown *from* the atom and says nothing about what that atom is,
                //   so an existing heteroatom is kept — `a` on an N is pyridine, not a benzene
                //   with the nitrogen erased. Charge is kept with it, or an N⁺ would silently
                //   lose its charge the moment a ring grew off it.
                //
                // Hydrogen is deliberately not preserved: it is a terminal cap, never a ring
                // vertex or a chain atom, so a skeleton grown from one has to take it over.
                let before = editor.molecule.clone();
                let keep_existing = frag.atoms[frag.attach_idx].element == "C"
                    && editor.molecule.atom_by_id(atom_id)
                        .is_some_and(|a| is_heteroatom(&a.element));
                if !keep_existing {
                    let root_elem = frag.atoms[frag.attach_idx].element.clone();
                    let root_charge = frag.atoms[frag.attach_idx].charge;
                    if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
                        atom.element = root_elem;
                        atom.charge = root_charge;
                    }
                }

                // Detect best orientation: angle + flip to keep ring bulk away from existing bonds
                let (angle, flip) = editor.best_fragment_placement(atom_id, &frag);
                let on_ring = crate::molecule::fragment::ring_membership(&frag);
                let new_ids = if !on_ring[frag.attach_idx] {
                    editor.molecule.insert_fragment(&frag, atom_id, angle, flip)
                } else if let Some(new_ids) = keep_existing
                    .then(|| insert_keeping_carbons_sound(editor, &frag, atom_id, angle, flip))
                    .flatten()
                {
                    new_ids
                } else {
                    let drawn = draw_ring_at_atom(editor, &frag, atom_id, angle, flip, aim);
                    let Some(atoms) = drawn else {
                        editor.molecule = before;
                        return KeyOutcome::Nothing;
                    };
                    outcome = KeyOutcome::RingDrawn(atoms.clone());
                    let mut new_ids: Vec<u32> = Vec::new();
                    for id in atoms {
                        if id != atom_id && !new_ids.contains(&id) {
                            new_ids.push(id);
                        }
                    }
                    new_ids
                };

                // The fragment's bonds are new bonds on the atom it landed on, so any group
                // hydrogen that atom was already wearing may now have nowhere to sit: growing
                // a ring off an NH₂ nitrogen is pyridine, not a pentavalent N. The fragment's
                // own atoms are included because a snapped-on atom gains bonds the same way.
                let mut touched = vec![atom_id];
                touched.extend(new_ids.iter().copied());
                consume_group_h(editor, &touched);

                // The hotspot goes to the last new atom that still exists — `s` on a saturated
                // centre inserts an S–H whose hydrogen is consumed on the spot, and parking the
                // hotspot on a removed atom would leave a keyboard user with no way to move it.
                if !new_ids.is_empty() {
                    let alive = new_ids.iter().rev().copied()
                        .find(|&id| editor.molecule.atom_by_id(id).is_some());
                    editor.hotspot_atom = Some(alive.unwrap_or(atom_id));
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
    outcome
}

fn apply_bond_action(
    editor: &mut ChemStructEditor,
    bond_id: u32,
    action: &BondAction,
) -> KeyOutcome {
    match action {
        BondAction::Stereo { stereo } => {
            if let Some(bond) = editor.molecule.bond_by_id(bond_id) {
                // Cycle through stereo if same type pressed again
                let next = if bond.stereo == *stereo {
                    BondStereo::None
                } else {
                    stereo.clone()
                };
                edit_bond_type(editor, bond_id, bond.order.clone(), next);
            }
            KeyOutcome::Edited
        }
        BondAction::Order { order } => {
            edit_bond_type(editor, bond_id, order.clone(), BondStereo::None);
            KeyOutcome::Edited
        }
        // Fuse ring onto the bond (bond becomes one edge of the ring)
        BondAction::Ring { ring } => fuse_ring_onto_bond(editor, bond_id, *ring),
        // Fuse a named fragment (e.g. benzene) onto the bond, sharing it as one edge.
        // Ignore unknown fragment names.
        BondAction::Fragment { fragment } => editor
            .config
            .resolve_fragment(fragment)
            .map_or(KeyOutcome::Nothing, |frag| {
                fuse_fragment_onto_bond(editor, bond_id, &frag)
            }),
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
        if editor.molecule.add_bond(current, new_id, BondOrder::Single) != 0 {
            consume_group_h(editor, &[current, new_id]);
        }
        if new_id == current { break; } // guard: snapped to self
        current = new_id;
    }
    editor.hotspot_atom = Some(current);
}

/// Fuse an n-membered ring onto a bond, sharing that bond as one edge.
///
/// The polygon's side is the **shared bond's own length**, not [`DEFAULT_BOND_LENGTH`]: the new
/// ring keeps that edge, so it is the only side length for which the ring can be both regular
/// and closed. Sizing it from the constant left every ring fused onto a bond of some other
/// length lopsided: on a 1.0-long bond a fused three-ring came out with 0.35 edges.
///
/// The ring goes on the left of the bond's begin→end direction unless that side is taken, in
/// which case it goes on the other, as anything drawn on a bond does ([`fuse_on_bond`]).
fn fuse_ring_onto_bond(editor: &mut ChemStructEditor, bond_id: u32, n: usize) -> KeyOutcome {
    if n < 3 {
        return KeyOutcome::Nothing;
    }
    let frag = crate::molecule::fragment::Fragment::ring(n);
    // `Fragment::ring` lies on the right of its first edge, so its left is the mirror image.
    let left = true;
    let atoms = fuse_on_bond(editor, bond_id, &frag, Some(left), None);
    bond_key_outcome(&frag, atoms)
}

/// Fuse a named fragment onto a bond, sharing that bond as one edge.
///
/// With both sides of the bond free the fragment goes on the side away from the neighbours of the
/// bond's end atoms.
fn fuse_fragment_onto_bond(
    editor: &mut ChemStructEditor,
    bond_id: u32,
    frag: &crate::molecule::fragment::Fragment,
) -> KeyOutcome {
    let atoms = fuse_on_bond(editor, bond_id, frag, None, None);
    bond_key_outcome(frag, atoms)
}

/// What [`fuse_on_bond`] coming back with `atoms` amounts to.
fn bond_key_outcome(
    frag: &crate::molecule::fragment::Fragment,
    atoms: Option<Vec<u32>>,
) -> KeyOutcome {
    let Some(atoms) = atoms else {
        return KeyOutcome::Nothing;
    };
    let (attach, next) = (frag.attach_idx, (frag.attach_idx + 1) % frag.atoms.len());
    match crate::molecule::fragment::ring_through_bond(frag, attach, next) {
        Some(cycle) => KeyOutcome::RingDrawn(cycle.iter().map(|&i| atoms[i]).collect()),
        None => KeyOutcome::Edited,
    }
}

/// [`insert_fragment`](crate::molecule::Molecule::insert_fragment), for a ring grown from a
/// heteroatom, taken back if a carbon cannot hold it. Returns the atoms it added.
///
/// How many bonds a heteroatom should have is the user's to judge: a ring grown from an N with a
/// hydrogen they placed themselves over-bonds that N, and the warning is the right answer, so the
/// bonds are left as the fragment draws them. A carbon has no such latitude.
fn insert_keeping_carbons_sound(
    editor: &mut ChemStructEditor,
    frag: &crate::molecule::fragment::Fragment,
    atom_id: u32,
    angle: f32,
    flip: bool,
) -> Option<Vec<u32>> {
    let before = editor.molecule.clone();
    let new_ids = editor.molecule.insert_fragment(frag, atom_id, angle, flip);
    let over_bonded = editor
        .molecule
        .atoms
        .iter()
        .any(|a| a.element == "C" && is_over_bonded(&before, &editor.molecule, a.id));
    if over_bonded {
        editor.molecule = before;
        return None;
    }
    Some(new_ids)
}

/// Whether atom `id` has more bonds in `after` than it can hold and than it had in `before`.
fn is_over_bonded(
    before: &crate::molecule::Molecule,
    after: &crate::molecule::Molecule,
    id: u32,
) -> bool {
    let was = before.valence_excess(id);
    after
        .valence_excess(id)
        .is_some_and(|excess| excess > 0 && was.is_none_or(|old| excess > old))
}

/// Draw a fragment whose attach atom is on one of its rings at atom `atom_id`, placed by `angle`
/// and `flip` if that can be drawn soundly and on one of the atom's bonds otherwise. Returns the
/// atom each fragment atom became, or `None`, with the bonds as they were, if the ring has nowhere
/// to go at this atom.
///
/// A ring drawn at an atom usually lands on what is there. A vertex beside the attach atom that
/// comes down on a neighbour means the ring lies along that bond, which makes this the same act as
/// pressing the key on the bond, and it gets the same answers: whether it fits, and which double
/// bonds the system ends up with. With nothing beside it the ring stands on the atom alone. That
/// takes two new bonds, which an atom with three already (a ring-fusion carbon) has no room for,
/// but such an atom may still have a bond with a free side.
///
/// `aim` says whether the user pointed at the atom, which is what lets a ring at a saturated atom
/// give it a double bond (see [`draw_fragment_on`]).
fn draw_ring_at_atom(
    editor: &mut ChemStructEditor,
    frag: &crate::molecule::fragment::Fragment,
    atom_id: u32,
    angle: f32,
    flip: bool,
    aim: Aim,
) -> Option<Vec<u32>> {
    let aimed = (aim == Aim::User).then_some(atom_id);
    let attach = frag.attach_idx;
    let positions = editor
        .molecule
        .fragment_placement(frag, atom_id, angle, flip);
    if positions.is_empty() {
        return None;
    }

    let on_ring = crate::molecule::fragment::ring_membership(frag);
    let beside: Vec<usize> = frag
        .bonds
        .iter()
        .filter_map(|b| match (b.begin == attach, b.end == attach) {
            (true, _) => Some(b.end),
            (_, true) => Some(b.begin),
            _ => None,
        })
        .filter(|&i| on_ring[i])
        .collect();
    let along_bonds: Vec<(usize, u32)> = beside
        .into_iter()
        .filter_map(|i| {
            let neighbour = editor.molecule.fusion_snap(positions[i])?;
            editor
                .molecule
                .bond_between(atom_id, neighbour)
                .map(|_| (i, neighbour))
        })
        .collect();

    let drawn = if along_bonds.is_empty() {
        draw_unless_over_bonded(editor, frag, &positions, &[(attach, atom_id)], aimed)
    } else {
        along_bonds.into_iter().find_map(|(i, neighbour)| {
            let shared = (atom_id, neighbour);
            fuse_at(editor, frag, &positions, (attach, i), shared, aimed)
        })
    };
    if drawn.is_some() {
        return drawn;
    }

    let bonds: Vec<u32> = editor
        .molecule
        .bonds_for_atom(atom_id)
        .iter()
        .map(|b| b.id)
        .collect();
    bonds
        .into_iter()
        .find_map(|bond| fuse_on_bond(editor, bond, frag, None, aimed))
}

/// Draw `frag` on a bond, its attach edge (`attach_idx` → next) mapped onto the bond's endpoints
/// by a similarity transform (translation + rotation + uniform scale), on the mirror side `flip`
/// if given and on the side clear of the bond's neighbours otherwise.
///
/// Those are preferences; what a side has to offer is decided by actually asking whether the
/// fragment fits there ([`Molecule::fusion_fits`](crate::molecule::Molecule::fusion_fits)). How
/// close the fragment comes to the neighbours cannot answer that. A hexagon on a bond beside a
/// ring-fusion atom has a vertex exactly on an atom on *both* sides: on the free side that atom
/// is merged into the new ring, on the other the whole ring is already there. That is a tie, and
/// which way the bond happened to run decided it: on the wrong side the ring landed in the one
/// already drawn, every atom snapped, and nothing appeared.
///
/// If neither side fits, or the only ones that do would over-bond an atom, nothing changes.
/// Returns the atom each fragment atom became, or `None` if the fragment was not drawn. `aimed` is
/// as for [`draw_fragment_on`].
fn fuse_on_bond(
    editor: &mut ChemStructEditor,
    bond_id: u32,
    frag: &crate::molecule::fragment::Fragment,
    flip: Option<bool>,
    aimed: Option<u32>,
) -> Option<Vec<u32>> {
    let n = frag.atoms.len();
    if n < 2 {
        return None;
    }
    let attach = frag.attach_idx;
    let next = (attach + 1) % n;

    let (a_id, b_id) = {
        let bond = editor.molecule.bonds.iter().find(|b| b.id == bond_id)?;
        (bond.begin, bond.end)
    };
    let (pa, pb) = (
        editor.molecule.atom_by_id(a_id)?.pos,
        editor.molecule.atom_by_id(b_id)?.pos,
    );

    // Raw fragment attach-edge endpoints.
    let p0 = frag.atoms[attach].pos;
    let p1 = frag.atoms[next].pos;
    let fdx = p1[0] - p0[0];
    let fdy = p1[1] - p0[1];
    let flen = (fdx * fdx + fdy * fdy).sqrt();
    if flen < 1e-6 {
        return None;
    }

    let bdx = pb[0] - pa[0];
    let bdy = pb[1] - pa[1];
    let blen = (bdx * bdx + bdy * bdy).sqrt();
    if blen < 1e-6 {
        return None;
    }

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

    let preferred =
        flip.unwrap_or_else(|| !neighbors.is_empty() && clearance(true) > clearance(false));

    [preferred, !preferred].into_iter().find_map(|flip| {
        let positions: Vec<[f32; 2]> = frag
            .atoms
            .iter()
            .map(|fa| transform(fa.pos, flip))
            .collect();
        fuse_at(
            editor,
            frag,
            &positions,
            (attach, next),
            (a_id, b_id),
            aimed,
        )
    })
}

/// Draw `frag` at `positions`, its `edge` being the bond `shared`, if it fits there and leaves no
/// atom over-bonded. Returns the atom each fragment atom became.
fn fuse_at(
    editor: &mut ChemStructEditor,
    frag: &crate::molecule::fragment::Fragment,
    positions: &[[f32; 2]],
    edge: (usize, usize),
    shared: (u32, u32),
    aimed: Option<u32>,
) -> Option<Vec<u32>> {
    if !editor.molecule.fusion_fits(frag, positions, edge, shared) {
        return None;
    }
    let positions = editor.molecule.fit_to_atoms(positions, edge, shared);
    let on_bond = [(edge.0, shared.0), (edge.1, shared.1)];
    draw_unless_over_bonded(editor, frag, &positions, &on_bond, aimed)
}

/// [`draw_fragment_on`], taken back if it leaves an atom with more bonds than it can hold.
fn draw_unless_over_bonded(
    editor: &mut ChemStructEditor,
    frag: &crate::molecule::fragment::Fragment,
    positions: &[[f32; 2]],
    on: &[(usize, u32)],
    aimed: Option<u32>,
) -> Option<Vec<u32>> {
    let before = editor.molecule.clone();
    let atoms = draw_fragment_on(editor, frag, positions, on, aimed);
    if atoms
        .iter()
        .any(|&id| is_over_bonded(&before, &editor.molecule, id))
    {
        editor.molecule = before;
        return None;
    }
    Some(atoms)
}

/// Add `frag` to the drawing at `positions` and settle the bond orders. `on` pairs the fragment
/// atoms that stand on an existing atom with that atom: one for a fragment on an atom, or the two
/// ends of the bond it shares. Returns the atom each fragment atom became. `aimed` is the atom the
/// user pointed at, if it was an atom.
///
/// Vertices on an existing atom merge with it, and bonds that coincide with existing ones are not
/// drawn again and keep their order: whatever the fragment says about them, those bonds are part
/// of a drawing that was consistent without it. The one exception is the shared edge of an
/// unsaturated fragment, which takes the fragment's order so that a benzene on a lone C=C is
/// fully alternating relative to it, and a benzene on a saturated bond with nothing else under it
/// is a whole one: that is how `5` and then `a` draws an indole.
fn draw_fragment_on(
    editor: &mut ChemStructEditor,
    frag: &crate::molecule::fragment::Fragment,
    positions: &[[f32; 2]],
    on: &[(usize, u32)],
    aimed: Option<u32>,
) -> Vec<u32> {
    let mut merged = false;
    let mut id_map: Vec<u32> = Vec::with_capacity(frag.atoms.len());
    for (i, fa) in frag.atoms.iter().enumerate() {
        id_map.push(if let Some(&(_, id)) = on.iter().find(|p| p.0 == i) {
            id
        } else if let Some(existing) = editor.molecule.fusion_snap(positions[i]) {
            merged = true;
            existing
        } else {
            editor
                .molecule
                .add_fragment_atom(fa.element.clone(), positions[i], fa.charge)
        });
    }

    let shared = match on {
        &[(_, u), (_, v)] => Some((u, v)),
        _ => None,
    };
    let is_shared = |u: u32, v: u32| shared == Some((u, v)) || shared == Some((v, u));
    let unsaturated = frag.bonds.iter().any(|b| b.order == BondOrder::Double);
    // The shared edge's original order, kept so the repair pass below can restore it.
    let shared_bond = shared
        .and_then(|(u, v)| editor.molecule.bond_between(u, v))
        .map(|b| (b.id, b.order.clone()));
    let on_ring = crate::molecule::fragment::ring_membership(frag);

    let mut new_ring_bonds: Vec<(u32, u32)> = Vec::new();
    let mut rewritten: Vec<u32> = Vec::new();
    for fb in &frag.bonds {
        let (u, v) = (id_map[fb.begin], id_map[fb.end]);
        if u == v { continue; }
        match editor.molecule.bond_between(u, v).map(|b| b.id) {
            Some(id) if is_shared(u, v) => {
                if let (true, Some(bond)) = (unsaturated, editor.molecule.bond_by_id_mut(id)) {
                    bond.order = fb.order.clone();
                }
            }
            Some(_) => merged = true,
            None => {
                editor.molecule.add_bond(u, v, fb.order.clone());
                if on_ring[fb.begin] && on_ring[fb.end] {
                    new_ring_bonds.push((u, v));
                }
            }
        }
    }

    // A fragment's bond orders describe a *free-standing* ring, so they know nothing about the
    // double bonds of what it lands on. Adopting them verbatim can leave a carbon with two double
    // bonds (fusing benzene onto benzene did exactly that), and a ring sharing more than one bond
    // has a pattern only the whole system can settle.
    //
    // A saturated fragment brings no double bonds to settle, and a fragment whose own pattern is
    // valid on a plain fusion keeps it.
    let clash = id_map.iter().any(|&id| {
        double_count(&editor.molecule, id) > 1
            || editor
                .molecule
                .valence_excess(id)
                .is_some_and(|excess| excess > 0)
    });
    if unsaturated && (merged || clash) {
        // The pre-existing bond was already consistent with the rest of the molecule; the
        // fragment has no standing to change it.
        if let Some((id, order)) = shared_bond
            && let Some(bond) = editor.molecule.bond_by_id_mut(id)
        {
            bond.order = order;
        }
        for &(u, v) in &new_ring_bonds {
            if let Some(id) = editor.molecule.bond_between(u, v).map(|b| b.id)
                && let Some(bond) = editor.molecule.bond_by_id_mut(id)
                // Never demote a triple bond: it is an explicit choice, not alternation.
                && bond.order == BondOrder::Double
            {
                bond.order = BondOrder::Single;
            }
        }
        // Trying the new bonds first, in fragment order, is the walk from one fusion atom to the
        // other that gives a ring fused on one edge its pattern.
        let ring_atoms: Vec<u32> = (0..id_map.len())
            .filter(|&i| on_ring[i])
            .map(|i| id_map[i])
            .collect();
        // A ring atom that was saturated stays so, and the ring takes what pattern it can around
        // it, unless the user pointed at that very atom.
        let mut saturated = editor
            .molecule
            .saturated_ring_atoms(&ring_atoms, &new_ring_bonds);
        saturated.retain(|&id| Some(id) != aimed);
        rewritten = editor
            .molecule
            .rekekulize(&ring_atoms, &new_ring_bonds, &saturated);
    }

    // Last, once every order has settled: the fused ring is new valence on the shared atoms, and
    // a rewrite on whatever it paired up, so any group hydrogen they no longer have room for goes.
    consume_group_h(editor, &id_map);
    consume_group_h(editor, &rewritten);
    id_map
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
///
/// The label may also *chain* names (`MeOPh` = Me–O–Ph); see
/// [`label_chain`](crate::label_chain), which welds such a chain into one fragment so the
/// insertion below is the same either way.
fn expand_label(editor: &mut ChemStructEditor, atom_id: u32) -> bool {
    let element = match editor.molecule.atom_by_id(atom_id) {
        Some(a) => a.element.clone(),
        None    => return false,
    };

    let Some(frag) = crate::label_chain::resolve_label(&editor.config, &element) else { return false };

    // Change attach atom's element to match fragment's root atom
    let root_elem   = frag.atoms[frag.attach_idx].element.clone();
    let root_charge = frag.atoms[frag.attach_idx].charge;
    if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
        atom.element = root_elem;
        atom.charge  = root_charge;
    }

    // Pick direction & flip, then insert the fragment in-place
    let (angle, flip) = editor.best_fragment_placement(atom_id, &frag);
    let new_ids = editor.molecule.insert_fragment(&frag, atom_id, angle, flip);

    // Same rule as the atom key that inserts a fragment: expanding "SiH3" onto a silicon that
    // already carries two bonds writes one hydrogen too many, and it is not one the user typed.
    let mut touched = vec![atom_id];
    touched.extend(new_ids);
    consume_group_h(editor, &touched);

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

    /// Bond tool: a drag that *begins over empty canvas* (no atom, no bond under the press)
    /// runs the same rubber-band selection the Select tool offers, rather than doing nothing.
    /// The atom enclosed by the marquee is selected; one outside it is not; and nothing is
    /// created or moved.
    #[test]
    fn bond_tool_drag_from_empty_space_rubber_band_selects() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        assert_eq!(editor.tool, crate::Tool::Bond);

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let center = screen_rect.center();

        // Two atoms, far enough apart that a small marquee around one excludes the other.
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [4.0, 0.0], 0);
        let a_screen = editor.mol_to_screen([0.0, 0.0], center);

        // A rectangular path around atom `a`. Every corner is >15px (NODE_HIT_RADIUS_PX) from
        // any atom, so the press begins in genuinely empty space.
        let corners = [
            a_screen + egui::vec2(-40.0, -40.0),
            a_screen + egui::vec2( 40.0, -40.0),
            a_screen + egui::vec2( 40.0,  40.0),
            a_screen + egui::vec2(-40.0,  40.0),
            a_screen + egui::vec2(-40.0, -40.0),
        ];

        // Move to the first corner, then press there (empty space).
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(corners[0])]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: corners[0], button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]);
        // Drag around the box, one move per frame, to build up the lasso polygon.
        for c in &corners[1..] {
            run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(*c)]);
        }
        // Release to commit the selection.
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: *corners.last().unwrap(), button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);

        assert!(editor.selected_atoms.contains(&a), "atom inside the marquee should be selected");
        assert!(!editor.selected_atoms.contains(&b), "atom outside the marquee must not be selected");
        assert_eq!(editor.molecule.atoms.len(), 2, "rubber-band selection must not create atoms");
        assert!(editor.molecule.bonds.is_empty(), "rubber-band selection must not create bonds");
    }

    /// Bond tool: a drag that *begins on an atom* still draws a bond (the empty-space marquee
    /// must not have cannibalised the tool's core gesture).
    #[test]
    fn bond_tool_drag_from_atom_still_draws_bond_after_marquee_support() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();

        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let center = screen_rect.center();
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let a_screen = editor.mol_to_screen([0.0, 0.0], center);
        let end = a_screen + egui::vec2(150.0, 80.0); // empty space, past the drag threshold

        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(a_screen)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: a_screen, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
        ]);
        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(end)]);
        run_frame(&ctx, screen_rect, &mut editor, vec![
            egui::Event::PointerButton { pos: end, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() },
        ]);

        assert_eq!(editor.molecule.atoms.len(), 2, "dragging from an atom into empty space should create a bonded atom");
        assert_eq!(editor.molecule.bonds.len(), 1, "and bond it to the source atom");
        assert!(editor.selected_atoms.is_empty(), "a bond-draw drag must not select anything");
        let _ = a;
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

    /// Primary-button drag along `path` (press at the first point, release at the last), one
    /// pointer move per frame, with `modifiers` held throughout.
    fn primary_drag_along(
        editor: &mut ChemStructEditor,
        path: &[egui::Pos2],
        modifiers: egui::Modifiers,
    ) {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let frame = |editor: &mut ChemStructEditor, events| {
            crate::testkit::run_frame_with_modifiers(&ctx, screen_rect, editor, events, modifiers);
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers,
        };
        let (first, last) = (path[0], path[path.len() - 1]);
        frame(editor, vec![egui::Event::PointerMoved(first)]);
        frame(editor, vec![button(first, true)]);
        for &p in &path[1..] {
            frame(editor, vec![egui::Event::PointerMoved(p)]);
        }
        frame(editor, vec![button(last, false)]);
    }

    /// The canvas center of the 640x480 test screen, where mol (0, 0) sits at the default view.
    const MID: egui::Pos2 = egui::pos2(320.0, 240.0);

    /// A drag path starting on the atom at `MID`: a few small steps, then well away over many
    /// frames.
    fn drag_path_from_mid() -> Vec<egui::Pos2> {
        let mut path = vec![MID, MID + egui::vec2(4.0, 2.0), MID + egui::vec2(8.0, 4.0)];
        path.extend((1..=10).map(|k| MID + egui::vec2(8.0 + 10.0 * k as f32, 4.0 + 5.0 * k as f32)));
        path
    }

    /// Select tool: dragging an atom moves it, and that move is one undo step — undo puts the
    /// atom back where it was, redo moves it again. The drag spans many frames; it must not
    /// leave a snapshot per frame behind.
    #[test]
    fn select_drag_of_an_atom_is_one_undo_step() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [3.0, 0.0], 0);
        editor.molecule.add_bond(a, b, BondOrder::Single);
        let before = editor.molecule.clone();

        primary_drag_along(&mut editor, &drag_path_from_mid(), Default::default());

        let moved = editor.molecule.atom_by_id(a).unwrap().pos;
        assert_ne!(moved, [0.0, 0.0], "the drag moves the atom");
        assert_eq!(editor.molecule.atom_by_id(b).unwrap().pos, [3.0, 0.0], "an unselected neighbour stays");
        assert_eq!(editor.undo_stack.len(), 1, "one drag pushes exactly one undo snapshot");

        assert!(editor.undo(), "the move is undoable");
        assert_eq!(editor.molecule, before, "undo restores the pre-drag position");
        assert!(editor.redo(), "the move is redoable");
        assert_eq!(editor.molecule.atom_by_id(a).unwrap().pos, moved, "redo re-applies the move");
    }

    /// Select tool: dragging one atom of a multi-atom selection moves the whole selection, as
    /// one undo step.
    #[test]
    fn select_drag_of_a_selection_is_one_undo_step() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        let c = editor.molecule.add_atom("C".to_string(), [5.0, 0.0], 0);
        editor.selected_atoms.extend([a, b]);
        let before = editor.molecule.clone();

        primary_drag_along(&mut editor, &drag_path_from_mid(), Default::default());

        assert_ne!(editor.molecule.atom_by_id(a).unwrap().pos, [0.0, 0.0], "the dragged atom moves");
        assert_ne!(editor.molecule.atom_by_id(b).unwrap().pos, [1.5, 0.0], "the rest of the selection moves");
        assert_eq!(editor.molecule.atom_by_id(c).unwrap().pos, [5.0, 0.0], "an unselected atom stays");
        assert_eq!(editor.undo_stack.len(), 1, "one drag pushes exactly one undo snapshot");

        assert!(editor.undo(), "the move is undoable");
        assert_eq!(editor.molecule, before, "undo restores every selected atom");
    }

    /// Select tool: Alt+drag rotates the selection about its centroid, as one undo step.
    #[test]
    fn select_alt_drag_rotation_is_one_undo_step() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let a = editor.molecule.add_atom("C".to_string(), [-1.5, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        editor.selected_atoms.extend([a, b]);
        let before = editor.molecule.clone();

        // Start off the centroid (mol (0, 0) is at MID) and sweep round it.
        let path: Vec<egui::Pos2> = (0..=8)
            .map(|k| {
                let t = k as f32 * 0.15;
                MID + 80.0 * egui::vec2(t.cos(), t.sin())
            })
            .collect();
        primary_drag_along(&mut editor, &path, egui::Modifiers::ALT);

        assert_ne!(editor.molecule, before, "the Alt+drag rotates the selection");
        assert_eq!(editor.undo_stack.len(), 1, "one rotation pushes exactly one undo snapshot");
        assert!(editor.undo(), "the rotation is undoable");
        assert_eq!(editor.molecule, before, "undo restores the pre-rotation positions");
    }

    /// Select tool: a click on an atom (no movement) only toggles the selection; it changes no
    /// coordinates, so it must not leave an undo snapshot behind.
    #[test]
    fn select_click_on_an_atom_pushes_no_undo() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);

        primary_drag_along(&mut editor, &[MID], Default::default());

        assert!(editor.selected_atoms.contains(&a), "the click selects the atom");
        assert_eq!(editor.molecule.atom_by_id(a).unwrap().pos, [0.0, 0.0], "the click moves nothing");
        assert!(editor.undo_stack.is_empty(), "a click pushes no undo snapshot");
    }

    /// Select tool: a drag that leaves the atom in a single frame, far past egui's drag
    /// threshold, still grabs the atom it was pressed on. `drag_started()` only fires once the
    /// pointer is already off the atom, so the dragged atom must come from the press origin,
    /// not from the hover at drag start.
    #[test]
    fn select_fast_drag_off_an_atom_still_moves_it() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);

        primary_drag_along(&mut editor, &[MID, MID + egui::vec2(150.0, 80.0)], Default::default());

        assert_ne!(editor.molecule.atom_by_id(a).unwrap().pos, [0.0, 0.0], "the fast drag moves the atom");
        assert!(editor.selected_atoms.is_empty(), "a move drag runs no lasso");
        assert_eq!(editor.undo_stack.len(), 1, "the move is one undo step");
    }

    /// Select tool: the lasso polygon starts where the button was pressed, not where the
    /// pointer was when the drag threshold was crossed. Here the first move jumps a whole side
    /// of the square in one frame; an atom inside the square but outside the triangle left
    /// after dropping the press corner must still be selected.
    #[test]
    fn select_fast_lasso_includes_the_press_origin() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        // Near the press corner (but outside the atom hit radius of it), on its side of the
        // square's diagonal.
        let near_corner = editor.screen_to_mol(MID + egui::vec2(-20.0, -20.0), MID);
        let a = editor.molecule.add_atom("C".to_string(), near_corner, 0);

        let path = [
            MID + egui::vec2(-40.0, -40.0),
            MID + egui::vec2( 40.0, -40.0),
            MID + egui::vec2( 40.0,  40.0),
            MID + egui::vec2(-40.0,  40.0),
        ];
        primary_drag_along(&mut editor, &path, Default::default());

        assert!(editor.selected_atoms.contains(&a), "the atom inside the lasso is selected");
        assert!(editor.undo_stack.is_empty(), "a lasso changes no coordinates");
    }

    /// A five-carbon zigzag chain with its middle bond (atoms 2–3) dragged out of shape, and
    /// the screen point at the middle of that bond on the 640x480 test screen.
    fn chain_with_distorted_middle_bond(editor: &mut ChemStructEditor) -> (Vec<u32>, u32, egui::Pos2) {
        let (dx, dy) = (1.5 * 0.75_f32.sqrt(), 0.75);
        let chain: Vec<u32> = (0..5)
            .map(|i| editor.molecule.add_atom("C".to_string(), [i as f32 * dx, (i % 2) as f32 * dy], 0))
            .collect();
        for w in chain.windows(2) {
            editor.molecule.add_bond(w[0], w[1], BondOrder::Single);
        }
        editor.molecule.atom_by_id_mut(chain[2]).unwrap().pos = [2.2, -0.9];
        editor.molecule.atom_by_id_mut(chain[3]).unwrap().pos = [4.6, 1.6];
        let bond = editor.molecule.bond_between(chain[2], chain[3]).unwrap().id;
        let mid = editor.mol_to_screen([3.4, 0.35], MID);
        (chain, bond, mid)
    }

    /// Select tool: clicking a bond selects that bond alone, and Cleanup then lays out just
    /// its two atoms against the rest, which keeps its exact coordinates. Driven through the
    /// real pointer path, so the selection the Cleanup sees is the one the click made.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn select_click_on_a_bond_then_cleanup_moves_only_its_two_atoms() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let (chain, bond, mid) = chain_with_distorted_middle_bond(&mut editor);
        let before = editor.molecule.clone();

        primary_drag_along(&mut editor, &[mid], Default::default());
        assert_eq!(editor.selected_bonds, [bond].into_iter().collect(), "the click selects the bond");
        assert!(editor.selected_atoms.is_empty(), "and no atom of its own");
        assert_eq!(editor.molecule, before, "selecting changes nothing");

        editor.toggle_cleanup();
        while editor.poll_cleanup() {
            std::thread::yield_now();
        }

        let ends = [chain[2], chain[3]];
        for a in &before.atoms {
            let now = editor.molecule.atom_by_id(a.id).unwrap().pos;
            if ends.contains(&a.id) {
                assert_ne!(now, a.pos, "end atom {} of the selected bond is cleaned up", a.id);
            } else {
                assert_eq!(
                    [now[0].to_bits(), now[1].to_bits()],
                    [a.pos[0].to_bits(), a.pos[1].to_bits()],
                    "atom {} is not selected and must not move", a.id
                );
            }
        }
        for w in chain.windows(2) {
            let (p, q) = (editor.molecule.atom_by_id(w[0]).unwrap().pos, editor.molecule.atom_by_id(w[1]).unwrap().pos);
            let d = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
            assert!((d - 1.5).abs() < 0.05, "bond {}-{} is {d} after the cleanup", w[0], w[1]);
        }
    }

    /// Select tool: a second click on a selected bond deselects it, a click on empty canvas
    /// clears a bond selection, and a lasso replaces it.
    #[test]
    fn select_click_toggles_a_bond_and_empty_click_or_lasso_clears_it() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let (_, bond, mid) = chain_with_distorted_middle_bond(&mut editor);
        let empty = MID + egui::vec2(-200.0, 150.0);

        primary_drag_along(&mut editor, &[mid], Default::default());
        assert!(editor.selected_bonds.contains(&bond));
        primary_drag_along(&mut editor, &[mid], Default::default());
        assert!(editor.selected_bonds.is_empty(), "a second click deselects the bond");

        primary_drag_along(&mut editor, &[mid], Default::default());
        primary_drag_along(&mut editor, &[empty], Default::default());
        assert!(editor.selected_bonds.is_empty(), "a click on empty canvas clears it");

        primary_drag_along(&mut editor, &[mid], Default::default());
        let lasso = [empty, empty + egui::vec2(30.0, 0.0), empty + egui::vec2(30.0, 30.0), empty + egui::vec2(0.0, 30.0)];
        primary_drag_along(&mut editor, &lasso, Default::default());
        assert!(editor.selected_bonds.is_empty(), "a lasso replaces the selection");
        assert!(editor.undo_stack.is_empty(), "selecting pushes no undo snapshot");
    }

    /// Delete with a bond selected removes that bond only: its atoms, and their other bonds,
    /// stay.
    #[test]
    fn deleting_a_selected_bond_removes_only_the_bond() {
        let mut editor = ChemStructEditor { tool: crate::Tool::Select, ..Default::default() };
        let (chain, bond, mid) = chain_with_distorted_middle_bond(&mut editor);
        primary_drag_along(&mut editor, &[mid], Default::default());

        assert!(editor.delete_selection());
        assert!(editor.molecule.bond_by_id(bond).is_none(), "the selected bond is deleted");
        assert_eq!(editor.molecule.atoms.len(), chain.len(), "no atom is deleted");
        assert_eq!(editor.molecule.bonds.len(), chain.len() - 2, "the other bonds stay");
        assert!(editor.selected_bonds.is_empty());
        assert!(editor.undo());
        assert!(editor.molecule.bond_by_id(bond).is_some(), "undo brings the bond back");
    }

    /// The Bond tool keeps its own click on a bond: it cycles the bond's order and selects
    /// nothing (bond selection is the Select tool's).
    #[test]
    fn bond_tool_click_on_a_bond_still_cycles_its_order() {
        let mut editor = ChemStructEditor::default();
        assert_eq!(editor.tool, crate::Tool::Bond);
        let (_, bond, mid) = chain_with_distorted_middle_bond(&mut editor);

        primary_drag_along(&mut editor, &[mid], Default::default());

        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().order, BondOrder::Double);
        assert!(editor.selected_bonds.is_empty() && editor.selected_atoms.is_empty());
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

    /// Cycling a bond's order is the mouse gesture for "make this a double bond", and it takes
    /// valence from both ends exactly as the `2`/`3` keys do — so a group's hydrogen goes with
    /// it. Cycling back down to single frees valence and must take nothing.
    #[test]
    fn cycling_a_bond_order_consumes_only_on_the_way_up() {
        let mut e = editor();
        let c = e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = e.molecule.add_atom("N".to_string(), [1.5, 0.0], 0);
        let bid = e.molecule.add_bond(c, n, BondOrder::Single);
        let nh2 = crate::Config::embedded()
            .resolve_fragment("NH2")
            .expect("NH2 is a built-in fragment");
        let h = e.molecule.insert_fragment(&nh2, n, 0.0, false)[0];

        cycle_bond_order(&mut e, bid); // single → double: N is at 2 + 1 = 3, exactly full
        assert!(e.molecule.atom_by_id(h).is_some(), "a full valence is not an excess");

        cycle_bond_order(&mut e, bid); // double → triple: 3 + 1 = 4, over
        assert!(e.molecule.atom_by_id(h).is_none(), "the group's hydrogen has nowhere left to sit");
        assert_eq!(e.molecule.atoms.len(), 2);

        // Triple → single wraps back around; there is nothing left to take and nothing to undo.
        cycle_bond_order(&mut e, bid);
        assert_eq!(e.molecule.atoms.len(), 2);
        assert_eq!(e.molecule.bond_between(c, n).map(|b| b.order.clone()), Some(BondOrder::Single));
    }

    /// A ring fused onto an existing bond must be a *regular* polygon **on that bond**.
    ///
    /// This pins down two separate ways the polygon used to come out wrong.
    ///
    /// **Scale.** The fused ring inherits the shared edge, so the only polygon that is both
    /// regular and closed is the one whose side equals that edge's actual length. The
    /// circumradius was sized from `DEFAULT_BOND_LENGTH` while the ring was anchored on the
    /// real endpoints, so any bond that was not exactly 1.5 long — a fragment's own bond, a
    /// dragged atom, a ring built at another scale — came out lopsided: on a 1.0 bond a fused
    /// three-ring measured 0.35 across one edge. (`fuse_fragment_onto_bond` has always matched
    /// the bond it is given, scaling by `blen / flen`; the ring path now agrees with it.)
    ///
    /// **Vertex order.** The walk ran the wrong way round the circle, so the last vertex landed
    /// exactly on `b` and the vertex past it was never placed. Even on a perfectly ordinary
    /// 1.5 bond that left two atoms sharing one position and one "edge" cutting across the
    /// ring — `c a Shift+ArrowRight 6` really did produce a 3.00-long bond and a zero atom
    /// separation. Topology-only tests never saw it: the result is still an n-cycle.
    #[test]
    fn fused_ring_is_regular_on_any_bond_length() {
        for shared in [1.0_f32, DEFAULT_BOND_LENGTH, 2.4] {
            for n in 3..=8usize {
                let mut e = ChemStructEditor::default();
                let a = e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
                let b = e.molecule.add_atom("C".to_string(), [shared, 0.0], 0);
                let bond_id = e.molecule.add_bond(a, b, BondOrder::Single);

                fuse_ring_onto_bond(&mut e, bond_id, n);

                assert_eq!(e.molecule.atoms.len(), n, "ring {n} on a {shared} bond: atom count");
                assert_eq!(e.molecule.bonds.len(), n, "ring {n} on a {shared} bond: not closed");
                for bd in &e.molecule.bonds {
                    let p = e.molecule.atom_by_id(bd.begin).unwrap().pos;
                    let q = e.molecule.atom_by_id(bd.end).unwrap().pos;
                    let len = (p[0] - q[0]).hypot(p[1] - q[1]);
                    assert!(
                        (len - shared).abs() < 0.01,
                        "ring {n} fused onto a {shared} bond has a {len:.3} edge — not regular",
                    );
                }
            }
        }
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
