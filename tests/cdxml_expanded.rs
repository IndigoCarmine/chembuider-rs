//! CDXML for readers that do not know the editor's abbreviations.
//!
//! `molecule_to_cdxml` writes a label such as `Ph` as a text node of no element. RDKit's
//! `MolsFromCDXML` reads such a node as one carbon, so a host that hands the CDXML to RDKit
//! needs the labels expanded first: `ChemStructEditor::to_cdxml_expanded`.

use chembuider_rs::molecule::cdx::{molecule_to_cdxml, read_cdxml};
use chembuider_rs::molecule::elements::element_of_label;
use chembuider_rs::{BondOrder, ChemStructEditor, Molecule};

/// A methyl carrying each of `labels` on its own atom: CH3–label, side by side.
fn methyls_with(labels: &[&str]) -> ChemStructEditor {
    let mut e = ChemStructEditor::default();
    for (k, label) in labels.iter().enumerate() {
        let y = 6.0 * k as f32;
        let c = e.molecule.add_atom("C".into(), [0.0, y], 0);
        let x = e.molecule.add_atom((*label).into(), [1.5, y], 0);
        e.molecule.add_bond(c, x, BondOrder::Single);
    }
    e
}

fn labels(mol: &Molecule) -> Vec<String> {
    let mut v: Vec<String> = mol
        .atoms
        .iter()
        .filter(|a| element_of_label(&a.element).is_none())
        .map(|a| a.element.clone())
        .collect();
    v.sort();
    v
}

fn heavy_atoms(mol: &Molecule) -> usize {
    mol.atoms.iter().filter(|a| a.element != "H").count()
}

/// The plain writer keeps the labels as labels; that is what ChemDraw expects and is unchanged.
#[test]
fn the_plain_writer_keeps_labels() {
    let e = methyls_with(&["Ph", "OMe"]);
    let back = read_cdxml(&molecule_to_cdxml(&e.molecule))
        .unwrap()
        .molecule;
    assert_eq!(labels(&back), ["OMe", "Ph"]);
}

/// The expanded writer leaves no label behind and writes every atom of every group.
#[test]
fn the_expanded_writer_writes_the_atoms_of_every_label() {
    let e = methyls_with(&["Ph", "OMe", "CF3", "MeOPh"]);
    let back = read_cdxml(&e.to_cdxml_expanded()).unwrap().molecule;
    assert!(
        labels(&back).is_empty(),
        "left as labels: {:?}",
        labels(&back)
    );
    // CH3–Ph 7, CH3–OMe 3, CH3–CF3 5, CH3–CH2–O–Ph 9.
    assert_eq!(heavy_atoms(&back), 7 + 3 + 5 + 9);
    // The editor's own drawing keeps its labels.
    assert_eq!(labels(&e.molecule), ["CF3", "MeOPh", "OMe", "Ph"]);
}

/// A label with no reading cannot be expanded; it is written as it was and reported by
/// `expand_all_labels`.
#[test]
fn a_label_with_no_reading_stays_and_is_reported() {
    let mut e = methyls_with(&["Ph", "Xyzzy"]);
    let back = read_cdxml(&e.to_cdxml_expanded()).unwrap().molecule;
    assert_eq!(labels(&back), ["Xyzzy"]);
    assert_eq!(e.expand_all_labels(), ["Xyzzy"]);
    assert_eq!(labels(&e.molecule), ["Xyzzy"]);
    assert_eq!(heavy_atoms(&e.molecule), 2 + 7);
}
