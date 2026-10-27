//! A ring fill only exists while its cycle does: deleting one of the ring's bonds drops it,
//! and undo/redo (whole-molecule snapshots) bring it back and take it away again with the bond.

use chembuider_rs::{BondOrder, ChemStructEditor};

#[test]
fn deleting_a_ring_bond_drops_the_fill_and_undo_redo_restore_it() {
    let mut e = ChemStructEditor::default();
    let ids: Vec<u32> = (0..6)
        .map(|k| {
            let a = k as f32 * std::f32::consts::TAU / 6.0;
            e.molecule.add_atom("C".to_string(), [a.cos(), a.sin()], 0)
        })
        .collect();
    for k in 0..6 {
        e.molecule.add_bond(ids[k], ids[(k + 1) % 6], BondOrder::Single);
    }
    assert!(e.fill_ring_at(ids[0], [0, 200, 0, 128]));
    assert_eq!(e.molecule.ring_fills.len(), 1);

    // Delete one ring bond the way the editor does: snapshot, then remove.
    let bond = e.molecule.bond_between(ids[2], ids[3]).unwrap().id;
    e.push_undo();
    e.molecule.remove_bond(bond);
    assert!(e.molecule.ring_fills.is_empty(), "the opened chain carries no ring fill");

    assert!(e.undo());
    assert!(e.molecule.bond_between(ids[2], ids[3]).is_some(), "undo restores the bond");
    assert_eq!(e.molecule.ring_fills.len(), 1, "and the ring fill with it");
    assert_eq!(e.molecule.ring_fills[0].color, [0, 200, 0, 128]);

    assert!(e.redo());
    assert!(e.molecule.bond_between(ids[2], ids[3]).is_none());
    assert!(e.molecule.ring_fills.is_empty(), "redo removes the fill again");
}
