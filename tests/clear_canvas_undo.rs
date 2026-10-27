//! Widget behavior a host relies on when it replaces the whole molecule itself: if it calls
//! `push_undo` first and then swaps in a new `molecule`, `undo` brings back the replaced
//! structure and `redo` re-applies the replacement. This checks the editor's undo/redo, not any
//! example's code; the examples' Clear buttons follow this sequence but are not run by tests.

use chembuider_rs::{BondOrder, ChemStructEditor, Molecule};

/// A host-side whole-molecule replacement: snapshot, then swap in an empty molecule.
fn replace_with_empty(e: &mut ChemStructEditor) {
    e.push_undo();
    e.molecule = Molecule::default();
    e.selected_atoms.clear();
}

#[test]
fn undo_after_a_snapshotted_replacement_restores_the_replaced_structure() {
    let mut e = ChemStructEditor::default();
    // Two edits the way the editor makes them: snapshot, then change.
    e.push_undo();
    let a = e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
    e.push_undo();
    let b = e.molecule.add_atom("O".to_string(), [1.5, 0.0], 0);
    e.molecule.add_bond(a, b, BondOrder::Double);
    e.selected_atoms.insert(a);
    let before = e.molecule.clone();

    replace_with_empty(&mut e);
    assert!(e.molecule.atoms.is_empty());
    assert!(e.selected_atoms.is_empty());

    assert!(e.undo(), "the snapshotted replacement is an undo step");
    assert!(e.molecule == before, "undo brings back the structure as it was just before the replacement");

    assert!(e.redo());
    assert!(e.molecule.atoms.is_empty(), "redo applies the replacement again");
}
