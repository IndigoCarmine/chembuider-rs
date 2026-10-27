//! A host that replaces the whole molecule (the examples' "Clear" button) must snapshot it with
//! `push_undo` first, as the editor does before its own edits: undo restores the last snapshot,
//! so a replacement without one is not a step Ctrl+Z can take back.

use chembuider_rs::{BondOrder, ChemStructEditor, Molecule};

/// What the examples' Clear button does.
fn clear(e: &mut ChemStructEditor) {
    e.push_undo();
    e.molecule = Molecule::default();
    e.selected_atoms.clear();
}

#[test]
fn undo_after_clear_restores_the_cleared_structure() {
    let mut e = ChemStructEditor::default();
    // Two edits the way the editor makes them: snapshot, then change.
    e.push_undo();
    let a = e.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
    e.push_undo();
    let b = e.molecule.add_atom("O".to_string(), [1.5, 0.0], 0);
    e.molecule.add_bond(a, b, BondOrder::Double);
    e.selected_atoms.insert(a);
    let before_clear = e.molecule.clone();

    clear(&mut e);
    assert!(e.molecule.atoms.is_empty());
    assert!(e.selected_atoms.is_empty());

    assert!(e.undo(), "Clear is an undo step");
    assert!(e.molecule == before_clear, "undo brings back the structure as it was just before Clear");

    assert!(e.redo());
    assert!(e.molecule.atoms.is_empty(), "redo clears it again");
}
