//! The default behaviour, which every tool has unless it says otherwise ([`ToolBehavior`]). It
//! is all the Select tool does.

use super::{Cx, Gesture, Hit, KeyFlow, PressedKey, ToolBehavior};
use crate::widget::interact::{self, Aim, KeyOutcome, ObjectDrag};
use crate::widget::ChemStructEditor;

/// The Select tool: the default behaviour, nothing changed.
pub(crate) struct Select;

impl ToolBehavior for Select {}

/// A click toggles what is under the pointer in the selection; one on empty canvas clears it.
pub(crate) fn on_click(editor: &mut ChemStructEditor, _cx: &Cx, _at: egui::Pos2, hit: Hit) {
    match hit {
        Hit::Atom(id) => {
            if !editor.selected_atoms.remove(&id) {
                editor.selected_atoms.insert(id);
            }
        }
        Hit::Object(id) => {
            if !editor.selected_objects.remove(&id) {
                editor.selected_objects.insert(id);
            }
        }
        // A bond on its own (see `ChemStructEditor::selected_bonds`).
        Hit::Bond(id) => {
            if !editor.selected_bonds.remove(&id) {
                editor.selected_bonds.insert(id);
            }
        }
        Hit::Empty => {
            editor.selected_atoms.clear();
            editor.selected_bonds.clear();
            editor.selected_objects.clear();
        }
    }
}

/// A double-click on an atom or bond selects its whole molecule; one on a text (or a bracket,
/// for its label) edits it, one on a table's cell enters its value.
pub(crate) fn on_double_click(editor: &mut ChemStructEditor, _cx: &Cx, at: egui::Pos2, hit: Hit) {
    let seed = match hit {
        Hit::Atom(id) => Some(id),
        Hit::Bond(id) => editor.molecule.bond_by_id(id).map(|b| b.begin),
        Hit::Object(id) => {
            edit_object(editor, id, at);
            None
        }
        Hit::Empty => None,
    };
    if let Some(atom) = seed {
        editor.selected_atoms = editor.molecule.connected_atoms(atom).into_iter().collect();
        editor.selected_bonds.clear();
    }
}

/// Open page object `id` for typing: a table at its cell under `at`, anything else as a text.
pub(crate) fn edit_object(editor: &mut ChemStructEditor, id: u32, at: egui::Pos2) {
    if editor.molecule.object_by_id(id).is_some_and(|o| o.as_table().is_some()) {
        if let Some(cell) = editor.table_cell_at(id, at) {
            editor.edit_table_cell(id, cell);
        }
    } else {
        editor.edit_text(id);
    }
}

/// A drag moves what it was pressed on (an end of the lone selected arrow, an atom, a page
/// object), and draws a rubber band from anywhere else.
pub(crate) fn on_drag_start(
    editor: &mut ChemStructEditor,
    cx: &Cx,
    origin: egui::Pos2,
    _at: egui::Pos2,
    hit: Hit,
) -> Option<Gesture> {
    if let Some(end) = interact::arrow_end_at(editor, origin, cx.center) {
        return Some(Gesture::Object(end));
    }
    Some(match hit {
        Hit::Atom(atom) => Gesture::MoveAtom { atom },
        Hit::Object(id) => Gesture::Object(ObjectDrag::Move {
            with_selection: editor.selected_objects.contains(&id),
            id,
        }),
        Hit::Bond(_) | Hit::Empty => Gesture::Lasso { path: vec![origin] },
    })
}

/// The keys, the same in every tool.
///
/// What a drawing key acts on, in order:
///  1. the atom under the pointer (change its element, attach a fragment);
///  2. else, for a key that names an element, a new atom at the pointer, ahead of a stale
///     hotspot (so O, N, S start a new atom rather than editing the hotspot);
///  3. else the bond under the pointer, or the keyboard's bond target (either beats a stale
///     atom hotspot, so fusing a ring targets the bond);
///  4. else the hotspot atom (build from it: chains, rings, fused fragments).
///
/// Besides those: arrows walk the hotspot (Shift: aim at the bond that way), or with a selection
/// and a modifier move or turn it (the configured selection shortcuts); Enter and `g` take the
/// hovered atom as the hotspot (`g` also adds it to the selection); Space and Tab drop the
/// hotspot; `x` expands a label; `` ` `` opens the label box; Delete removes the selection, or
/// else what is under the pointer, or the keyboard's target; Escape clears the selection.
pub(crate) fn on_key(editor: &mut ChemStructEditor, cx: &Cx, k: &PressedKey) -> KeyFlow {
    let key = k.key.as_str();
    let (shift, ctrl, alt) = (k.shift, k.ctrl, k.alt);

    // The selection shortcuts (Ctrl, Alt or Shift with an arrow) act on a selection.
    if (ctrl || alt || shift) && !editor.selected_atoms.is_empty() {
        let actions: Vec<_> = editor.config.selection_shortcuts.iter()
            .filter(|s| s.key == key && s.ctrl == ctrl && s.alt == alt && s.shift == shift)
            .map(|s| s.action.clone())
            .collect();
        if !actions.is_empty() {
            for action in &actions {
                interact::apply_selection_action(editor, action);
            }
            return KeyFlow::Continue;
        }
    }

    // Every arrow is taken, auto-repeat included, in the order pressed: a frame holding
    // `ArrowLeft a` must aim the `a` where the arrow went.
    if let Some((dx, dy)) = interact::arrow_direction(key) {
        if !(ctrl || alt) {
            // A press like any other, it ends a ring repeat.
            editor.ring_repeat = None;
            interact::walk_keyboard_target(editor, dx, dy, shift);
        }
        return KeyFlow::Continue;
    }
    if k.repeat || ctrl || alt {
        return KeyFlow::Continue;
    }

    match key {
        "Delete" | "Backspace" => {
            if !editor.delete_selection() {
                interact::delete_hovered(editor);
            }
            return KeyFlow::Continue;
        }
        "Escape" => {
            editor.selected_atoms.clear();
            editor.selected_bonds.clear();
            editor.selected_objects.clear();
            return KeyFlow::Continue;
        }
        _ => {}
    }

    let target = editor.keyboard_target();
    let repeat = interact::take_ring_repeat(editor, key);

    match key {
        "Enter" => {
            if let Some(id) = editor.hovered_atom {
                editor.hotspot_atom = Some(id);
            }
            return KeyFlow::Continue;
        }
        "Space" | "Tab" => {
            editor.hotspot_atom = None;
            editor.hotspot_bond = None;
            return KeyFlow::Continue;
        }
        "g" | "G" => {
            // Falls back to the hotspot: this is the key that grows the selection, so it must
            // work without a pointer, or selection (and so Delete, move and rotate) would be out
            // of the keyboard's reach.
            if let Some(id) = editor.hovered_atom.or(editor.hotspot_atom) {
                editor.hotspot_atom = Some(id);
                editor.selected_atoms.insert(id);
            }
            return KeyFlow::Continue;
        }
        // Expand a label (e.g. "Bor") into its fragment.
        "x" | "X" => {
            if let Some(atom_id) = editor.hovered_atom.or(editor.hotspot_atom) {
                // A label with no reading is reported (`unexpanded_label`), unless it is a plain
                // element, which has nothing to expand.
                if interact::label_reading(editor, atom_id).is_some() {
                    interact::expand_label(editor, atom_id);
                } else if let Some(atom) = editor.molecule.atom_by_id(atom_id)
                    && crate::molecule::elements::atomic_number(&atom.element).is_none()
                {
                    editor.unexpanded_label = Some(atom.element.clone());
                }
                return KeyFlow::Continue;
            }
        }
        // Open the label box.
        "`" => {
            if let Some(atom_id) = editor.hovered_atom.or(editor.hotspot_atom) {
                interact::open_label_edit(editor, cx.ui, atom_id, k.index);
                return KeyFlow::Stop;
            }
        }
        _ => {}
    }

    if let Some(atom_id) = editor.hovered_atom {
        interact::press_on_atom(editor, atom_id, key, target, Aim::User);
        return KeyFlow::Continue;
    }
    if interact::place_element_at_cursor(editor, key) {
        return KeyFlow::Continue;
    }
    if let Some(bond_id) = editor.hovered_bond.or(editor.hotspot_bond) {
        let outcome = interact::dispatch_bond_key(editor, bond_id, key);
        // The mouse aims for itself, so only the keyboard's own target moves on.
        if let (KeyOutcome::RingDrawn(ring), None) = (outcome, editor.hovered_bond)
            && let Some(next) = interact::next_bond_round_ring(editor, &ring, key)
        {
            editor.hotspot_bond = Some(next);
        }
    } else if let Some(atom_id) = repeat {
        interact::press_on_atom(editor, atom_id, key, target, Aim::Repeat);
    } else if let Some(atom_id) = editor.hotspot_atom {
        interact::press_on_atom(editor, atom_id, key, target, Aim::User);
    }
    KeyFlow::Continue
}
