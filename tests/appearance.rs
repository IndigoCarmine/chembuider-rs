//! Appearance settings (OPE-16): ring fills, line and text colours, and the drop shadow — in
//! the saved file, through the editor's API with undo/redo, and through ChemDraw's CDXML.

use chembuider_rs::molecule::cdx::{molecule_to_cdxml, read_cdxml};
use chembuider_rs::molecule::{PageObject, PageText};
use chembuider_rs::{BondOrder, ChemStructEditor, Molecule, Shadow};

const RED: [u8; 4] = [220, 20, 30, 255];
const BLUE: [u8; 4] = [10, 40, 230, 255];
const GREEN: [u8; 4] = [0, 200, 0, 128];

/// A ring of `n` carbons centred on `c`, its atom ids in ring order.
fn ring(m: &mut Molecule, n: usize, c: [f32; 2]) -> Vec<u32> {
    let ids: Vec<u32> = (0..n)
        .map(|k| {
            let a = k as f32 * std::f32::consts::TAU / n as f32;
            m.add_atom("C".into(), [c[0] + 1.5 * a.cos(), c[1] + 1.5 * a.sin()], 0)
        })
        .collect();
    for k in 0..n {
        m.add_bond(ids[k], ids[(k + 1) % n], BondOrder::Single);
    }
    ids
}

/// Ethanol (C–C–O) and a text: a bond, a label and a page text to colour.
fn ethanol_with_text() -> (ChemStructEditor, [u32; 3], u32, u32) {
    let mut e = ChemStructEditor::default();
    let c1 = e.molecule.add_atom("C".into(), [0.0, 0.0], 0);
    let c2 = e.molecule.add_atom("C".into(), [1.3, 0.75], 0);
    let o = e.molecule.add_atom("O".into(), [2.6, 0.0], 0);
    e.molecule.add_bond(c1, c2, BondOrder::Single);
    let co = e.molecule.add_bond(c2, o, BondOrder::Single);
    let text = e.molecule.add_text([0.0, 4.0], "K_2CO_3");
    (e, [c1, c2, o], co, text)
}

// ─── Saved file ──────────────────────────────────────────────────────────────

/// A drawing without colours or shadow saves exactly as it did before they existed, and that
/// file still loads.
#[test]
fn a_colourless_drawing_saves_byte_for_byte_as_before() {
    let mut m = Molecule::default();
    let ids: Vec<u32> = [[1.5, 0.0], [-0.75, 1.25], [-0.75, -1.25]].into_iter().map(|p| m.add_atom("C".into(), p, 0)).collect();
    for k in 0..3 {
        m.add_bond(ids[k], ids[(k + 1) % 3], BondOrder::Single);
    }
    m.set_ring_fill(ids.clone(), GREEN);
    m.add_text([0.0, 3.0], "Et_2O");
    let json = serde_json::to_string(&m).unwrap();
    let expected = concat!(
        r#"{"name":"MOL","atoms":["#,
        r#"{"id":1,"element":"C","pos":[1.5,0.0],"charge":0,"group_h":false},"#,
        r#"{"id":2,"element":"C","pos":[-0.75,1.25],"charge":0,"group_h":false},"#,
        r#"{"id":3,"element":"C","pos":[-0.75,-1.25],"charge":0,"group_h":false}],"#,
        r#""bonds":["#,
        r#"{"id":4,"begin":1,"end":2,"order":"Single","stereo":"None","double_position":"Auto"},"#,
        r#"{"id":5,"begin":2,"end":3,"order":"Single","stereo":"None","double_position":"Auto"},"#,
        r#"{"id":6,"begin":3,"end":1,"order":"Single","stereo":"None","double_position":"Auto"}],"#,
        r#""ring_fills":[{"atoms":[1,2,3],"color":[0,200,0,128]}],"#,
        r#""objects":[{"Text":{"id":7,"pos":[0.0,3.0],"text":"Et_2O","align":"Left","source":null}}]}"#,
    );
    assert_eq!(json, expected);
    let back: Molecule = serde_json::from_str(expected).unwrap();
    assert_eq!(back, m);
    assert!(back.atoms.iter().all(|a| a.color.is_none()));
    assert!(back.bonds.iter().all(|b| b.color.is_none()));
    assert!(back.shadow.is_none());
}

#[test]
fn colours_and_shadow_survive_save_and_load() {
    let (mut e, [_, c2, o], co, text) = ethanol_with_text();
    e.molecule.bond_by_id_mut(co).unwrap().color = Some(RED);
    e.molecule.atom_by_id_mut(o).unwrap().color = Some(BLUE);
    if let Some(PageObject::Text(t)) = e.molecule.object_by_id_mut(text) {
        t.color = Some(GREEN);
    }
    let shadow = Shadow { color: [10, 20, 30, 100], offset: [0.2, -0.1], blur: 0.3 };
    e.molecule.shadow = Some(shadow);
    let json = serde_json::to_string(&e.molecule).unwrap();
    let back: Molecule = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e.molecule);
    assert_eq!(back.bond_by_id(co).unwrap().color, Some(RED));
    assert_eq!(back.atom_by_id(o).unwrap().color, Some(BLUE));
    assert_eq!(back.atom_by_id(c2).unwrap().color, None);
    assert_eq!(back.shadow, Some(shadow));
}

// ─── Fill Ring ───────────────────────────────────────────────────────────────

/// Naphthalene: two fused hexagons.
fn naphthalene() -> (ChemStructEditor, Vec<u32>, Vec<u32>) {
    let mut e = ChemStructEditor::default();
    let a = ring(&mut e.molecule, 6, [0.0, 0.0]);
    // The second hexagon shares the bond a[0]–a[1].
    let (p, q) = (e.molecule.atom_by_id(a[0]).unwrap().pos, e.molecule.atom_by_id(a[1]).unwrap().pos);
    let mirror = |id: u32, m: &Molecule| {
        let r = m.atom_by_id(id).unwrap().pos;
        // Reflect r across the line p–q.
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let t = ((r[0] - p[0]) * dx + (r[1] - p[1]) * dy) / (dx * dx + dy * dy);
        let f = [p[0] + t * dx, p[1] + t * dy];
        [2.0 * f[0] - r[0], 2.0 * f[1] - r[1]]
    };
    let mut b = vec![a[0], a[1]];
    for &k in &[a[2], a[3], a[4], a[5]] {
        let pos = mirror(k, &e.molecule);
        b.push(e.molecule.add_atom("C".into(), pos, 0));
    }
    // b = [a0, a1, b2, b3, b4, b5]: a1–b2–b3–b4–b5–a0.
    for w in b[1..].windows(2) {
        e.molecule.add_bond(w[0], w[1], BondOrder::Single);
    }
    e.molecule.add_bond(b[5], b[0], BondOrder::Single);
    (e, a, b)
}

#[test]
fn filling_a_selection_of_two_rings_fills_each_smallest_ring_and_clear_removes_both() {
    let (mut e, a, b) = naphthalene();
    e.selected_atoms = a.iter().chain(&b).copied().collect();
    assert!(e.fill_selected_ring(GREEN));
    assert_eq!(e.molecule.ring_fills.len(), 2, "one fill per hexagon, not the decalin perimeter");
    assert!(e.molecule.ring_fills.iter().all(|f| f.atoms.len() == 6 && f.color == GREEN));

    // Saved and loaded, the fills stay.
    let back: Molecule = serde_json::from_str(&serde_json::to_string(&e.molecule).unwrap()).unwrap();
    assert_eq!(back.ring_fills, e.molecule.ring_fills);

    // Clearing with the same selection clears both, in one undo step, whatever the hotspot.
    e.hotspot_atom = Some(a[3]);
    assert!(e.clear_selected_ring_fill());
    assert!(e.molecule.ring_fills.is_empty());
    assert!(e.undo());
    assert_eq!(e.molecule.ring_fills.len(), 2);
    assert!(e.redo());
    assert!(e.molecule.ring_fills.is_empty());
}

#[test]
fn a_ring_selected_by_its_bonds_is_filled_and_cleared() {
    let mut e = ChemStructEditor::default();
    let ids = ring(&mut e.molecule, 6, [0.0, 0.0]);
    // The Select tool can pick bonds alone (OPE-8): the ring's bonds are the ring.
    e.selected_bonds = e.molecule.bonds.iter().map(|b| b.id).collect();
    assert!(e.fill_selected_ring(RED));
    assert_eq!(e.molecule.ring_fills.len(), 1);
    let mut got = e.molecule.ring_fills[0].atoms.clone();
    got.sort_unstable();
    let mut want = ids.clone();
    want.sort_unstable();
    assert_eq!(got, want);
    assert!(e.clear_selected_ring_fill());
    assert!(e.molecule.ring_fills.is_empty());
}

#[test]
fn clearing_where_there_is_no_fill_changes_nothing() {
    let mut e = ChemStructEditor::default();
    let ids = ring(&mut e.molecule, 6, [0.0, 0.0]);
    e.selected_atoms = ids.into_iter().collect();
    let undo = e.undo_stack.len();
    assert!(!e.clear_selected_ring_fill());
    assert_eq!(e.undo_stack.len(), undo, "no undo step for nothing");
}

// ─── Colours through the editor ──────────────────────────────────────────────

#[test]
fn line_and_text_colours_are_set_independently_and_undo_redo() {
    let (mut e, [c1, c2, o], co, text) = ethanol_with_text();
    let cc = e.molecule.bond_between(c1, c2).unwrap().id;

    // Select the C–O bond (by its atoms) and colour its line red.
    e.selected_atoms = [c2, o].into_iter().collect();
    assert!(e.set_selected_bond_color(Some(RED)));
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, Some(RED));
    assert_eq!(e.molecule.bond_by_id(cc).unwrap().color, None, "only bonds inside the selection");
    assert!(e.molecule.atoms.iter().all(|a| a.color.is_none()), "the line colour leaves labels alone");

    // Now the O's label (and the selected text) blue: the bond stays red.
    e.selected_atoms = [o].into_iter().collect();
    e.selected_objects = [text].into_iter().collect();
    assert!(e.set_selected_text_color(Some(BLUE)));
    assert_eq!(e.molecule.atom_by_id(o).unwrap().color, Some(BLUE));
    assert_eq!(e.molecule.atom_by_id(c2).unwrap().color, None);
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, Some(RED));
    let text_color = |m: &Molecule| match m.object_by_id(text) {
        Some(PageObject::Text(PageText { color, .. })) => *color,
        _ => panic!("the text is there"),
    };
    assert_eq!(text_color(&e.molecule), Some(BLUE));

    // Setting the same colour again is no edit.
    let undo = e.undo_stack.len();
    assert!(!e.set_selected_text_color(Some(BLUE)));
    assert_eq!(e.undo_stack.len(), undo);

    // Each was one undo step.
    assert!(e.undo());
    assert_eq!(e.molecule.atom_by_id(o).unwrap().color, None);
    assert_eq!(text_color(&e.molecule), None);
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, Some(RED));
    assert!(e.undo());
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, None);
    assert!(e.redo());
    assert!(e.redo());
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, Some(RED));
    assert_eq!(e.molecule.atom_by_id(o).unwrap().color, Some(BLUE));

    // `None` goes back to the style's colour.
    e.selected_atoms = [c2, o].into_iter().collect();
    e.selected_objects.clear();
    assert!(e.set_selected_bond_color(None));
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, None);
}

#[test]
fn a_bond_selected_on_its_own_takes_the_line_colour() {
    let (mut e, _, co, _) = ethanol_with_text();
    e.selected_bonds = [co].into_iter().collect();
    assert!(e.set_selected_bond_color(Some(RED)));
    assert_eq!(e.molecule.bond_by_id(co).unwrap().color, Some(RED));
    assert_eq!(e.molecule.bonds.iter().filter(|b| b.color.is_some()).count(), 1);
}

#[test]
fn nothing_selected_colours_nothing() {
    let (mut e, ..) = ethanol_with_text();
    assert!(!e.set_selected_bond_color(Some(RED)));
    assert!(!e.set_selected_text_color(Some(RED)));
    assert!(e.undo_stack.is_empty());
}

#[test]
fn copy_and_paste_keep_colours() {
    let (mut e, [_, c2, o], co, _) = ethanol_with_text();
    e.molecule.bond_by_id_mut(co).unwrap().color = Some(RED);
    e.molecule.atom_by_id_mut(o).unwrap().color = Some(BLUE);
    e.selected_atoms = [c2, o].into_iter().collect();
    let clip = e.copy_to_string().unwrap();
    let mut other = ChemStructEditor::default();
    assert!(other.paste_from_string(&clip));
    assert_eq!(other.molecule.bonds[0].color, Some(RED));
    let o2 = other.molecule.atoms.iter().find(|a| a.element == "O").unwrap();
    assert_eq!(o2.color, Some(BLUE));
    assert_eq!(other.molecule.atoms.iter().find(|a| a.element == "C").unwrap().color, None);
}

// ─── Shadow through the editor ───────────────────────────────────────────────

#[test]
fn the_shadow_is_set_in_one_undo_step() {
    let (mut e, ..) = ethanol_with_text();
    let shadow = Shadow { blur: 0.2, ..Shadow::default() };
    assert!(e.set_shadow(Some(shadow)));
    assert_eq!(e.molecule.shadow, Some(shadow));
    assert!(!e.set_shadow(Some(shadow)), "the same shadow again is no edit");
    let changed = Shadow { offset: [0.3, 0.3], ..shadow };
    assert!(e.set_shadow(Some(changed)));
    assert!(e.undo());
    assert_eq!(e.molecule.shadow, Some(shadow));
    assert!(e.undo());
    assert_eq!(e.molecule.shadow, None);
    assert!(e.redo());
    assert_eq!(e.molecule.shadow, Some(shadow));
    assert!(e.set_shadow(None));
    assert_eq!(e.molecule.shadow, None);
}

// ─── CDXML ───────────────────────────────────────────────────────────────────

#[test]
fn cdxml_round_trip_keeps_line_and_text_colours() {
    let (mut e, [c1, c2, o], co, text) = ethanol_with_text();
    let cc = e.molecule.bond_between(c1, c2).unwrap().id;
    e.molecule.bond_by_id_mut(co).unwrap().color = Some(RED);
    e.molecule.atom_by_id_mut(o).unwrap().color = Some(BLUE);
    if let Some(PageObject::Text(t)) = e.molecule.object_by_id_mut(text) {
        t.color = Some([0, 128, 0, 255]);
    }
    let xml = molecule_to_cdxml(&e.molecule);
    assert!(xml.contains("<colortable>"), "{xml}");
    // The new colours join ChemDraw's eight; red (220, 20, 30) is the ninth, index 10.
    assert!(xml.contains(r#"<color r="0.862745" g="0.078431" b="0.117647"/>"#), "{xml}");
    let read = read_cdxml(&xml).expect("reads back").molecule;
    let bond = |m: &Molecule, el: &str| {
        let a = m.atoms.iter().find(|a| a.element == el).unwrap().id;
        m.bonds.iter().find(|b| b.begin == a || b.end == a).unwrap().color
    };
    assert_eq!(bond(&read, "O"), Some(RED));
    assert_eq!(read.atoms.iter().find(|a| a.element == "O").unwrap().color, Some(BLUE));
    assert_eq!(read.atoms.iter().filter(|a| a.color.is_some()).count(), 1);
    let _ = cc;
    assert_eq!(read.bonds.iter().filter(|b| b.color.is_some()).count(), 1, "the C–C bond stays uncoloured");
    let texts: Vec<&PageText> = read
        .objects
        .iter()
        .filter_map(|o| if let PageObject::Text(t) = o { Some(t) } else { None })
        .collect();
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].color, Some([0, 128, 0, 255]));
}

#[test]
fn a_colourless_drawing_writes_the_same_cdxml_as_before() {
    let (e, ..) = ethanol_with_text();
    let xml = molecule_to_cdxml(&e.molecule);
    // ChemDraw's own eight colours and nothing coloured.
    assert_eq!(xml.matches("<color ").count(), 8, "{xml}");
    // Every colour index is the default foreground (0) or background (1).
    for (at, _) in xml.match_indices(r#"color=""#) {
        let index: String = xml[at + 7..].chars().take_while(|c| c.is_ascii_digit()).collect();
        assert!(index == "0" || index == "1", "colour index {index} in {xml}");
    }
    let read = read_cdxml(&xml).unwrap().molecule;
    assert!(read.atoms.iter().all(|a| a.color.is_none()));
    assert!(read.bonds.iter().all(|b| b.color.is_none()));
}
