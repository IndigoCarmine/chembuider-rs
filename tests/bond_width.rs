//! Bond line widths: set on the selected bonds through the editor with undo, kept by copy and
//! paste, and written to and read from ChemDraw's CDX and CDXML as `LineWidth`, the bonds that
//! name none reading the width of their fragment or document.

use chembuider_rs::molecule::cdx::{molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml};
use chembuider_rs::molecule::Bond;
use chembuider_rs::{BondOrder, ChemStructEditor, Molecule};

/// Propane (C–C–C): the editor and the ids of its two bonds.
fn propane() -> (ChemStructEditor, [u32; 3], [u32; 2]) {
    let mut e = ChemStructEditor::default();
    let a = e.molecule.add_atom("C".into(), [0.0, 0.0], 0);
    let b = e.molecule.add_atom("C".into(), [1.3, 0.75], 0);
    let c = e.molecule.add_atom("C".into(), [2.6, 0.0], 0);
    let ab = e.molecule.add_bond(a, b, BondOrder::Single);
    let bc = e.molecule.add_bond(b, c, BondOrder::Double);
    (e, [a, b, c], [ab, bc])
}

fn width(m: &Molecule, bond: u32) -> Option<f32> {
    m.bond_by_id(bond).unwrap().width
}

#[test]
fn the_selected_bonds_take_a_width_in_one_undo_step() {
    let (mut e, [a, b, _], [ab, bc]) = propane();
    e.selected_atoms = [a, b].into_iter().collect();
    assert!(e.set_selected_bond_width(Some(1.5)));
    assert_eq!(width(&e.molecule, ab), Some(1.5));
    assert_eq!(width(&e.molecule, bc), None, "only bonds inside the selection");

    // The same width again is no edit.
    let undo = e.undo_stack.len();
    assert!(!e.set_selected_bond_width(Some(1.5)));
    assert_eq!(e.undo_stack.len(), undo);

    assert!(e.undo());
    assert_eq!(width(&e.molecule, ab), None);
    assert!(e.redo());
    assert_eq!(width(&e.molecule, ab), Some(1.5));

    // `None` goes back to the style's width; a bond selected on its own takes one too.
    e.selected_atoms = [a, b].into_iter().collect();
    assert!(e.set_selected_bond_width(None));
    assert_eq!(width(&e.molecule, ab), None);
    e.selected_atoms.clear();
    e.selected_bonds = [bc].into_iter().collect();
    assert!(e.set_selected_bond_width(Some(100.0)));
    assert_eq!(width(&e.molecule, bc), Some(*Bond::WIDTH_RANGE.end()), "kept within ChemDraw's limits");
}

#[test]
fn nothing_selected_widens_nothing() {
    let (mut e, ..) = propane();
    assert!(!e.set_selected_bond_width(Some(2.0)));
    assert!(e.undo_stack.is_empty());
}

#[test]
fn copy_and_paste_keep_widths() {
    let (mut e, _, [ab, _]) = propane();
    e.molecule.bond_by_id_mut(ab).unwrap().width = Some(2.0);
    e.selected_atoms.clear();
    let clip = e.copy_to_string().unwrap();
    let mut other = ChemStructEditor::default();
    assert!(other.paste_from_string(&clip));
    let mut widths: Vec<Option<f32>> = other.molecule.bonds.iter().map(|b| b.width).collect();
    widths.sort_by(|x, y| x.partial_cmp(y).unwrap());
    assert_eq!(widths, [None, Some(2.0)]);
}

#[test]
fn a_bond_with_no_width_saves_as_before() {
    let (e, ..) = propane();
    let json = serde_json::to_string(&e.molecule).unwrap();
    assert!(!json.contains("width"), "{json}");
}

#[test]
fn widths_go_through_cdx_and_cdxml_as_line_width() {
    let (mut e, _, [ab, _]) = propane();
    e.molecule.bond_by_id_mut(ab).unwrap().width = Some(1.2);
    let xml = molecule_to_cdxml(&e.molecule);
    let bonds: Vec<&str> = xml.split("<b ").skip(1).map(|s| &s[..s.find('>').unwrap()]).collect();
    assert_eq!(bonds.len(), 2);
    let line_width = |b: &str| -> Option<f32> { b.split("LineWidth=\"").nth(1)?.split('"').next()?.parse().ok() };
    let widths: Vec<f32> = bonds.iter().filter_map(|b| line_width(b)).collect();
    assert!(widths.len() == 1 && (widths[0] - 1.2).abs() < 1e-3, "{xml}");
    assert_eq!(bonds.iter().filter(|b| b.contains("LineWidth")).count(), 1, "a default bond names none: {xml}");

    for read in [read_cdxml(&xml).unwrap().molecule, read_cdx(&molecule_to_cdx_document(&e.molecule)).unwrap().molecule] {
        let mut widths: Vec<Option<f32>> = read.bonds.iter().map(|b| b.width.map(|w| (w * 100.0).round() / 100.0)).collect();
        widths.sort_by(|x, y| x.partial_cmp(y).unwrap());
        assert_eq!(widths, [None, Some(1.2)]);
    }
}

/// `xml` with the attribute `name` of its first `tag` element set to `value`.
fn set_attr(xml: &str, tag: &str, name: &str, value: &str) -> String {
    let start = xml.find(&format!("<{tag} ")).unwrap();
    let end = start + xml[start..].find('>').unwrap();
    let head = &xml[start..end];
    let attr = format!(" {name}=\"");
    let new_head = match head.find(&attr) {
        Some(k) => {
            let v = k + attr.len();
            let close = v + head[v..].find('"').unwrap();
            format!("{}{value}{}", &head[..v], &head[close..])
        }
        None => format!("<{tag} {name}=\"{value}\"{}", &head[tag.len() + 1..]),
    };
    format!("{}{new_head}{}", &xml[..start], &xml[end..])
}

/// A ChemDraw document drawn in 1 pt lines keeps them: its bonds name no width, and read the
/// document's. A fragment's width overrides the document's, and a bond's own both.
#[test]
fn bonds_read_the_line_width_of_their_fragment_and_document() {
    let (e, ..) = propane();
    let xml = molecule_to_cdxml(&e.molecule);
    let widths = |xml: &str| -> Vec<Option<f32>> {
        let mut w: Vec<Option<f32>> =
            read_cdxml(xml).unwrap().molecule.bonds.iter().map(|b| b.width.map(|w| (w * 100.0).round() / 100.0)).collect();
        w.sort_by(|x, y| x.partial_cmp(y).unwrap());
        w
    };
    assert_eq!(widths(&xml), [None, None], "ChemDraw's own 0.6 pt is the default");

    let one_pt = set_attr(&xml, "CDXML", "LineWidth", "1");
    assert_eq!(widths(&one_pt), [Some(1.0), Some(1.0)], "{one_pt}");

    let fragment = set_attr(&one_pt, "fragment", "LineWidth", "2");
    assert_eq!(widths(&fragment), [Some(2.0), Some(2.0)], "{fragment}");

    // A bond's own width wins over the document's (here the default it names explicitly).
    let own = set_attr(&set_attr(&xml, "b", "LineWidth", "0.6"), "CDXML", "LineWidth", "1");
    assert_eq!(widths(&own), [None, Some(1.0)], "{own}");
}
