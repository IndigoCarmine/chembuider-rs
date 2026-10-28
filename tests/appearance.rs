//! Appearance settings (OPE-16): ring fills, line and text colours, and the drop shadow — in
//! the saved file, through the editor's API with undo/redo, and through ChemDraw's CDX and
//! CDXML.

use chembuider_rs::molecule::cdx::{
    cdxml_document, merge_into_cdx, molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml, CdxImport,
};
use chembuider_rs::molecule::cdx_codec;
use chembuider_rs::molecule::{PageObject, PageText, RingFill};
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
    assert!(back.atoms.iter().all(|a| a.shadow.is_none()));
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
    assert!(e.set_shadow(Some(shadow)));
    let json = serde_json::to_string(&e.molecule).unwrap();
    let back: Molecule = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e.molecule);
    assert_eq!(back.bond_by_id(co).unwrap().color, Some(RED));
    assert_eq!(back.atom_by_id(o).unwrap().color, Some(BLUE));
    assert_eq!(back.atom_by_id(c2).unwrap().color, None);
    assert_eq!(back.shadow_of(c2), Some(shadow));
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
    let (mut e, [c1, ..], ..) = ethanol_with_text();
    let shadow = Shadow { blur: 0.2, ..Shadow::default() };
    assert!(e.set_shadow(Some(shadow)));
    assert_eq!(e.molecule.shadow_of(c1), Some(shadow));
    assert!(!e.set_shadow(Some(shadow)), "the same shadow again is no edit");
    let changed = Shadow { offset: [0.3, 0.3], ..shadow };
    assert!(e.set_shadow(Some(changed)));
    assert!(e.undo());
    assert_eq!(e.molecule.shadow_of(c1), Some(shadow));
    assert!(e.undo());
    assert_eq!(e.molecule.shadow_of(c1), None);
    assert!(e.redo());
    assert_eq!(e.molecule.shadow_of(c1), Some(shadow));
    assert!(e.set_shadow(None));
    assert_eq!(e.molecule.shadow_of(c1), None);
}

/// Ethanol (C–C–O) and, apart from it, a methylamine (C–N): two structures.
fn two_structures() -> (ChemStructEditor, [u32; 3], [u32; 2]) {
    let (mut e, ethanol, ..) = ethanol_with_text();
    let c = e.molecule.add_atom("C".into(), [8.0, 0.0], 0);
    let n = e.molecule.add_atom("N".into(), [9.5, 0.0], 0);
    e.molecule.add_bond(c, n, BondOrder::Single);
    (e, ethanol, [c, n])
}

/// The shadow is set per structure: on those the selection touches (by an atom or a bond),
/// or on every one when nothing is selected; the others keep theirs.
#[test]
fn the_shadow_is_set_on_the_selected_structures() {
    let (mut e, [c1, c2, o], [c, n]) = two_structures();
    let shadow = Shadow::default();
    e.selected_atoms = [o].into_iter().collect();
    assert!(e.set_shadow(Some(shadow)));
    assert_eq!(e.molecule.shadow_of(c1), Some(shadow), "the whole of ethanol, from its O");
    assert_eq!(e.molecule.shadow_of(n), None, "not the amine");
    assert_eq!(e.selected_shadow(), Some(shadow));
    assert_eq!(e.molecule.structure_shadows(), [(vec![c1, c2, o], shadow)]);

    // A bond of the amine, selected on its own, gives it another.
    let other = Shadow { color: [200, 0, 0, 80], ..shadow };
    e.selected_atoms.clear();
    e.selected_bonds = [e.molecule.bond_between(c, n).unwrap().id].into_iter().collect();
    assert_eq!(e.selected_shadow(), None);
    assert!(e.set_shadow(Some(other)));
    assert_eq!(e.molecule.shadow_of(c), Some(other));
    assert_eq!(e.molecule.shadow_of(c2), Some(shadow), "ethanol keeps its own");

    // Removing it from the amine leaves ethanol's; one undo step brings it back.
    assert!(e.set_shadow(None));
    assert_eq!(e.molecule.shadow_of(n), None);
    assert_eq!(e.molecule.shadow_of(o), Some(shadow));
    assert!(e.undo());
    assert_eq!(e.molecule.shadow_of(n), Some(other));

    // A text alone selected touches no structure.
    e.selected_bonds.clear();
    e.selected_objects = [e.molecule.objects[0].id()].into_iter().collect();
    assert!(!e.set_shadow(Some(other)));
    assert_eq!(e.selected_shadow(), None);

    // Nothing selected: every structure.
    e.selected_objects.clear();
    assert!(e.set_shadow(None));
    assert!(e.molecule.structure_shadows().is_empty());
    assert!(e.set_shadow(Some(shadow)));
    assert_eq!(e.molecule.structure_shadows().len(), 2);
}

/// The shadow stays the structure's however it is edited: an atom drawn onto it is in it, two
/// structures joined cast the shadow of the lowest-id shadowed atom, an atom cut off is its own.
#[test]
fn the_shadow_follows_its_structure_through_edits() {
    let (mut e, [c1, c2, o], [c, n]) = two_structures();
    let shadow = Shadow::default();
    let other = Shadow { color: [200, 0, 0, 80], ..shadow };
    e.selected_atoms = [c1].into_iter().collect();
    e.set_shadow(Some(shadow));
    e.selected_atoms = [n].into_iter().collect();
    e.set_shadow(Some(other));

    let added = e.molecule.add_atom("C".into(), [2.6, -1.5], 0);
    e.molecule.add_bond(o, added, BondOrder::Single);
    assert_eq!(e.molecule.shadow_of(added), Some(shadow), "drawn onto ethanol");
    e.molecule.add_bond(added, c, BondOrder::Single);
    assert_eq!(e.molecule.shadow_of(n), Some(shadow), "joined, the lowest-id shadowed atom's (ethanol's C1)");
    let cc = e.molecule.bond_between(c1, c2).unwrap().id;
    e.molecule.remove_bond(cc);
    assert_eq!(e.molecule.shadow_of(c1), Some(shadow), "cut off, the atom keeps the shadow it carries");

    // Saved and loaded, and through the clipboard, it stays.
    let back: Molecule = serde_json::from_str(&serde_json::to_string(&e.molecule).unwrap()).unwrap();
    assert_eq!(back.structure_shadows(), e.molecule.structure_shadows());
    let (mut e, _, [c, n]) = two_structures();
    e.selected_atoms = [c].into_iter().collect();
    e.set_shadow(Some(other));
    e.selected_atoms = [c, n].into_iter().collect();
    let mut pasted = ChemStructEditor::default();
    assert!(pasted.paste_from_string(&e.copy_to_string().unwrap()));
    assert_eq!(pasted.molecule.structure_shadows().len(), 1);
    assert_eq!(pasted.molecule.structure_shadows()[0].1, other);
    // A structure without a shadow copies without one: its clipboard text names none.
    let (plain, ..) = ethanol_with_text();
    assert!(!plain.copy_to_string().unwrap().contains("shadow"));
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

// ─── CDXML written by others ─────────────────────────────────────────────────

/// A ChemDraw document: an oxygen (labelled) bonded to a carbon, `doc`, `node`, `bond` and
/// `run` being extra attributes of the document, both nodes and the fragment, the bond, and
/// the label's style run, `table` its colour table.
fn chemdraw_doc(doc: &str, table: &str, frag: &str, node: &str, bond: &str, run: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" ?>
<CDXML {doc}>{table}
 <page id="1">
  <fragment id="2" {frag}>
   <n id="3" p="36 36" Element="8" NumHydrogens="1" {node}>
    <t id="4" p="39.89 39.9" LabelAlignment="Right"><s font="3" size="10" face="96" {run}>HO</s></t>
   </n>
   <n id="5" p="50.4 36" {node}/>
   <b id="6" B="3" E="5" {bond}/>
  </fragment>
  <t id="7" p="36 80"><s font="3" size="10" {run}>THF</s></t>
 </page>
</CDXML>"#
    )
}

/// Index 2 is the background (white), 3 the foreground: here red.
const RED_FOREGROUND: &str = r#"<colortable><color r="1" g="1" b="1"/><color r="1" g="0" b="0"/></colortable>"#;
/// ChemDraw's own table: white, black, red, yellow, green, cyan, blue, magenta (indices 2–9).
const CHEMDRAW_TABLE: &str = r#"<colortable><color r="1" g="1" b="1"/><color r="0" g="0" b="0"/><color r="1" g="0" b="0"/><color r="1" g="1" b="0"/><color r="0" g="1" b="0"/><color r="0" g="1" b="1"/><color r="0" g="0" b="1"/><color r="1" g="0" b="1"/></colortable>"#;

type Colours = (Vec<Option<[u8; 4]>>, Vec<Option<[u8; 4]>>, Vec<Option<[u8; 4]>>);

/// The colours of the atoms, the bonds and the page texts of `m`, in their order.
fn colours_of(m: &Molecule) -> Colours {
    let texts = m.objects.iter().filter_map(|o| if let PageObject::Text(t) = o { Some(t.color) } else { None }).collect();
    (m.atoms.iter().map(|a| a.color).collect(), m.bonds.iter().map(|b| b.color).collect(), texts)
}

fn read_colours(xml: &str) -> Colours {
    colours_of(&read_cdxml(xml).expect("reads").molecule)
}

const RED_FULL: Option<[u8; 4]> = Some([255, 0, 0, 255]);

/// What names no colour is in the document's foreground (index 3 of the table), not black.
#[test]
fn what_names_no_colour_is_in_the_documents_foreground() {
    let (atoms, bonds, texts) = read_colours(&chemdraw_doc("", RED_FOREGROUND, "", "", "", ""));
    assert_eq!(atoms, [RED_FULL, RED_FULL]);
    assert_eq!(bonds, [RED_FULL]);
    assert_eq!(texts, [RED_FULL]);
}

/// Index 0 is black whatever the foreground: a document (or its objects) saying so reads
/// uncoloured.
#[test]
fn index_zero_is_black_whatever_the_foreground() {
    let (atoms, bonds, texts) = read_colours(&chemdraw_doc(r#"color="0""#, RED_FOREGROUND, "", "", "", ""));
    assert_eq!((atoms, bonds, texts), (vec![None, None], vec![None], vec![None]));
    let (atoms, bonds, _) =
        read_colours(&chemdraw_doc("", RED_FOREGROUND, "", r#"color="0""#, r#"color="0""#, r#"color="0""#));
    assert_eq!((atoms, bonds), (vec![None, None], vec![None]));
}

/// Without a colour table, 2 is white and 3 black (the default background and foreground).
#[test]
fn without_a_table_two_is_white_and_three_black() {
    let white = Some([255, 255, 255, 255]);
    let (atoms, bonds, _) = read_colours(&chemdraw_doc("", "", "", r#"color="2""#, r#"color="2""#, ""));
    assert_eq!((atoms, bonds), (vec![white, white], vec![white]));
    let (atoms, bonds, texts) = read_colours(&chemdraw_doc("", "", "", r#"color="3""#, r#"color="3""#, r#"color="3""#));
    assert_eq!((atoms, bonds, texts), (vec![None, None], vec![None], vec![None]));
}

/// Explicit indices name ChemDraw's table: 4 red, 6 green, 8 blue; a label's run colour is its
/// atom's when the node names none.
#[test]
fn explicit_indices_name_the_table() {
    let (atoms, bonds, texts) =
        read_colours(&chemdraw_doc(r#"color="0""#, CHEMDRAW_TABLE, "", "", r#"color="8""#, r#"color="6""#));
    let (green, blue) = (Some([0, 255, 0, 255]), Some([0, 0, 255, 255]));
    assert_eq!(atoms, [green, None], "the O's label run is green; the bare carbon names nothing");
    assert_eq!(bonds, [blue]);
    assert_eq!(texts, [green]);
    let (atoms, ..) = read_colours(&chemdraw_doc(r#"color="0""#, CHEMDRAW_TABLE, "", r#"color="4""#, "", ""));
    assert_eq!(atoms, [RED_FULL, RED_FULL]);
}

/// A fragment's colour is that of its nodes and bonds that name none of their own.
#[test]
fn a_fragments_colour_is_inherited() {
    let (atoms, bonds, texts) =
        read_colours(&chemdraw_doc(r#"color="0""#, CHEMDRAW_TABLE, r#"color="4""#, "", r#"color="8""#, ""));
    assert_eq!(atoms, [RED_FULL, RED_FULL]);
    assert_eq!(bonds, [Some([0, 0, 255, 255])], "the bond's own colour wins");
    assert_eq!(texts, [None], "the page text is not in the fragment");
}

/// Merged into a document whose foreground is red, an uncoloured atom is written black (0), so
/// ChemDraw does not paint it red, and a red one keeps its colour.
#[test]
fn writing_into_a_coloured_foreground_keeps_uncoloured_things_black() {
    use chembuider_rs::molecule::{cdx, cdx_codec};
    let original = cdx_codec::write(&cdx::cdxml_document(&chemdraw_doc("", RED_FOREGROUND, "", "", "", "")).unwrap());
    let mut m = Molecule::default();
    let o = m.add_atom("O".into(), [0.0, 0.0], 0);
    let c = m.add_atom("C".into(), [1.5, 0.0], 0);
    m.add_bond(o, c, BondOrder::Single);
    m.atom_by_id_mut(c).unwrap().color = Some(BLUE);
    let (atoms, bonds, _) = colours_of(&cdx::read_cdx(&cdx::merge_into_cdx(&original, &m)).expect("reads").molecule);
    assert_eq!(atoms, [None, Some(BLUE)]);
    assert_eq!(bonds, [None]);
}

/// The same when only the page names a colour (red): what the editor writes goes on that page,
/// so an uncoloured atom or bond is written black there too.
#[test]
fn writing_onto_a_coloured_page_keeps_uncoloured_things_black() {
    use chembuider_rs::molecule::{cdx, cdx_codec};
    let xml = chemdraw_doc("", CHEMDRAW_TABLE, "", "", "", "").replace(r#"<page id="1">"#, r#"<page id="1" color="4">"#);
    let original = cdx_codec::write(&cdx::cdxml_document(&xml).unwrap());
    assert_eq!(colours_of(&cdx::read_cdx(&original).unwrap().molecule).0, [RED_FULL, RED_FULL], "the page's red");
    let mut m = Molecule::default();
    let o = m.add_atom("O".into(), [0.0, 0.0], 0);
    let c = m.add_atom("C".into(), [1.5, 0.0], 0);
    m.add_bond(o, c, BondOrder::Single);
    m.atom_by_id_mut(c).unwrap().color = Some(BLUE);
    let (atoms, bonds, _) = colours_of(&cdx::read_cdx(&cdx::merge_into_cdx(&original, &m)).expect("reads").molecule);
    assert_eq!(atoms, [None, Some(BLUE)]);
    assert_eq!(bonds, [None]);
}

// ─── Clearing ring fills only within the selection ───────────────────────────

/// Two separate triangles, only the second filled.
fn two_triangles() -> (ChemStructEditor, Vec<u32>, Vec<u32>) {
    let mut e = ChemStructEditor::default();
    let a = ring(&mut e.molecule, 3, [0.0, 0.0]);
    let b = ring(&mut e.molecule, 3, [8.0, 0.0]);
    e.molecule.set_ring_fill(b.clone(), GREEN);
    (e, a, b)
}

/// A selection is the rings the user means: clearing an unfilled selected ring leaves the fill
/// of the ring under the cursor alone.
#[test]
fn clearing_a_selected_unfilled_ring_leaves_the_hotspots_fill() {
    let (mut e, a, b) = two_triangles();
    e.selected_atoms = a.iter().copied().collect();
    e.hotspot_atom = Some(b[0]);
    let undo = e.undo_stack.len();
    assert!(!e.clear_selected_ring_fill());
    assert_eq!(e.molecule.ring_fills.len(), 1, "the other ring keeps its fill");
    assert_eq!(e.undo_stack.len(), undo);
}

#[test]
fn clearing_with_selected_bonds_of_an_unfilled_ring_leaves_the_hotspots_fill() {
    let (mut e, a, b) = two_triangles();
    e.selected_bonds = e.molecule.bonds.iter().filter(|bd| a.contains(&bd.begin)).map(|bd| bd.id).collect();
    e.hotspot_atom = Some(b[0]);
    assert!(!e.clear_selected_ring_fill());
    assert_eq!(e.molecule.ring_fills.len(), 1);
    // One bond of the filled ring selected clears that ring's fill.
    e.selected_bonds = [e.molecule.bond_between(b[0], b[1]).unwrap().id].into_iter().collect();
    e.hotspot_atom = Some(a[0]);
    assert!(e.clear_selected_ring_fill());
    assert!(e.molecule.ring_fills.is_empty());
}

// ─── Page texts through the clipboard ────────────────────────────────────────

/// A coloured text keeps its words, alignment and colour through copy and paste, whether
/// copied with the structure or on its own; an uncoloured one stays uncoloured.
#[test]
fn copy_and_paste_keep_page_texts_and_their_colours() {
    let (mut e, [_, c2, o], _, text) = ethanol_with_text();
    if let Some(PageObject::Text(t)) = e.molecule.object_by_id_mut(text) {
        t.color = Some(GREEN);
    }
    let plain = e.molecule.add_text([5.0, 4.0], "THF, 60 °C");
    let texts = |m: &Molecule| -> Vec<(String, Option<[u8; 4]>)> {
        m.objects.iter().filter_map(|o| if let PageObject::Text(t) = o { Some((t.text.clone(), t.color)) } else { None }).collect()
    };

    // Everything (nothing selected).
    let mut other = ChemStructEditor::default();
    assert!(other.paste_from_string(&e.copy_to_string().unwrap()));
    assert_eq!(texts(&other.molecule), [("K_2CO_3".to_string(), Some(GREEN)), ("THF, 60 °C".to_string(), None)]);
    assert_eq!(other.molecule.atoms.len(), 3);

    // Atoms and the coloured text selected: that text only, with the structure.
    e.selected_atoms = [c2, o].into_iter().collect();
    e.selected_objects = [text].into_iter().collect();
    let mut other = ChemStructEditor::default();
    assert!(other.paste_from_string(&e.copy_to_string().unwrap()));
    assert_eq!(texts(&other.molecule), [("K_2CO_3".to_string(), Some(GREEN))]);
    assert_eq!(other.molecule.atoms.len(), 2);
    // Pasted texts get ids of their own, selected with the atoms.
    let pasted = other.molecule.objects[0].id();
    assert!(other.molecule.atoms.iter().all(|a| a.id != pasted));
    assert!(other.selected_objects.contains(&pasted));

    // A text alone, pasted into the same drawing: a second copy, moved over.
    e.selected_atoms.clear();
    e.selected_objects = [plain].into_iter().collect();
    let clip = e.copy_to_string().unwrap();
    assert!(e.paste_from_string(&clip));
    assert_eq!(e.molecule.atoms.len(), 3, "no atoms copied");
    let copies: Vec<&PageText> = e
        .molecule
        .objects
        .iter()
        .filter_map(|o| if let PageObject::Text(t) = o { Some(t) } else { None })
        .filter(|t| t.text == "THF, 60 °C")
        .collect();
    assert_eq!(copies.len(), 2);
    assert_ne!(copies[0].pos, copies[1].pos);
    assert!(e.undo());
    assert_eq!(texts(&e.molecule).len(), 2);
}

/// Clipboard text written before texts were copied still pastes; a copy without texts writes
/// none.
#[test]
fn clipboard_text_without_texts_reads_and_writes_as_before() {
    let old = r#"chembuilder-mol:{"atoms":[{"element":"O","pos":[0.0,0.0],"charge":0}],"bonds":[]}"#;
    let mut e = ChemStructEditor::default();
    assert!(e.paste_from_string(old));
    assert!(e.molecule.objects.is_empty());
    let clip = e.copy_to_string().unwrap();
    assert!(!clip.contains("texts"), "{clip}");
}

// ─── Ring fills and shadows in CDX and CDXML ─────────────────────────────────

/// `m` saved and read back each way a document goes: a new CDX file, a new CDXML file, and the
/// CDX file saved again over itself (as the editor saves a file it opened).
fn saved_and_read(m: &Molecule) -> Vec<(&'static str, Molecule)> {
    let cdx = molecule_to_cdx_document(m);
    let again = merge_into_cdx(&cdx, m);
    let read = |name, import: Option<CdxImport>| {
        let import = import.unwrap_or_else(|| panic!("{name} reads back"));
        assert_eq!(import.other_objects, 0, "{name}: a fill is no shape of its own");
        (name, import.molecule)
    };
    vec![
        read("CDX", read_cdx(&cdx)),
        read("CDXML", read_cdxml(&molecule_to_cdxml(m))),
        read("CDX saved again", read_cdx(&again)),
    ]
}

/// The atom of `read` each atom of `m` became: the one in the same place of the list (a
/// structure's atoms are written, and read, in their order).
fn atom_map(m: &Molecule, read: &Molecule, name: &str) -> std::collections::HashMap<u32, u32> {
    assert_eq!(read.atoms.len(), m.atoms.len(), "{name}: the same atoms");
    m.atoms
        .iter()
        .zip(&read.atoms)
        .map(|(a, b)| {
            assert_eq!(a.element, b.element, "{name}");
            (a.id, b.id)
        })
        .collect()
}

/// The fills of `read` are `m`'s, on the atoms they became, in the same ring order and colour
/// (alpha included); the order of the fills aside.
fn assert_same_fills(m: &Molecule, read: &Molecule, name: &str) {
    let map = atom_map(m, read, name);
    let mut want: Vec<RingFill> = m
        .ring_fills
        .iter()
        .map(|rf| RingFill { atoms: rf.atoms.iter().map(|a| map[a]).collect(), color: rf.color })
        .collect();
    let mut got = read.ring_fills.clone();
    want.sort_by_key(|rf| rf.atoms.clone());
    got.sort_by_key(|rf| rf.atoms.clone());
    assert_eq!(got, want, "{name}");
}

/// Every atom of `read` has the shadow its atom of `m` had, and so every structure casts the
/// shadow it cast.
fn assert_same_shadows(m: &Molecule, read: &Molecule, name: &str) {
    let map = atom_map(m, read, name);
    for a in &m.atoms {
        assert_eq!(read.atom_by_id(map[&a.id]).unwrap().shadow, a.shadow, "{name}: atom {}", a.id);
    }
    let want: Vec<(Vec<u32>, Shadow)> = m
        .structure_shadows()
        .into_iter()
        .map(|(atoms, s)| {
            let mut atoms: Vec<u32> = atoms.iter().map(|a| map[a]).collect();
            atoms.sort_unstable();
            (atoms, s)
        })
        .collect();
    assert_eq!(read.structure_shadows(), want, "{name}");
}

#[test]
fn a_ring_fill_survives_cdx_and_cdxml_with_its_alpha() {
    let mut m = Molecule::default();
    let ids = ring(&mut m, 6, [0.0, 0.0]);
    m.set_ring_fill(ids, GREEN);
    for (name, read) in saved_and_read(&m) {
        assert_eq!(read.ring_fills.len(), 1, "{name}");
        assert_same_fills(&m, &read, name);
    }
}

/// ChemDraw draws the fill itself: a closed, filled freeform curve in the fill's colour and
/// alpha, on the page behind the structure.
#[test]
fn a_ring_fill_is_a_filled_closed_curve_behind_its_structure_in_chemdraw() {
    let mut m = Molecule::default();
    let ids = ring(&mut m, 6, [0.0, 0.0]);
    m.set_ring_fill(ids, GREEN);
    let xml = molecule_to_cdxml(&m);
    let curve = xml.find("<curve ").unwrap_or_else(|| panic!("a curve in {xml}"));
    assert!(curve < xml.find("<fragment ").unwrap(), "drawn first, behind the bonds: {xml}");
    let tag = &xml[curve..curve + xml[curve..].find('>').unwrap()];
    for attr in [r#"CurveType="129""#, r#"FillType="Solid""#, r#"alpha="32896""#] {
        assert!(tag.contains(attr), "{attr} in {tag}");
    }
    // Six vertices, each with its two Bézier control points on it: straight edges.
    let points = tag.split(r#"CurvePoints=""#).nth(1).unwrap().split('"').next().unwrap();
    assert_eq!(points.split_whitespace().count(), 6 * 3 * 2, "{points}");
}

#[test]
fn several_ring_fills_survive_cdx_and_cdxml() {
    let (mut e, a, b) = naphthalene();
    e.molecule.set_ring_fill(a, RED);
    e.molecule.set_ring_fill(b, [0, 0, 0, 60]);
    let c = ring(&mut e.molecule, 5, [10.0, 0.0]);
    e.molecule.set_ring_fill(c, GREEN);
    let m = e.molecule;
    for (name, read) in saved_and_read(&m) {
        assert_eq!(read.ring_fills.len(), 3, "{name}");
        assert_same_fills(&m, &read, name);
    }
}

#[test]
fn a_shadow_on_one_of_two_structures_survives_cdx_and_cdxml() {
    let (mut e, [_, _, o], [c, _]) = two_structures();
    let shadow = Shadow { color: [10, 20, 30, 100], offset: [0.2, -0.1], blur: 0.3 };
    e.selected_atoms = [o].into_iter().collect();
    assert!(e.set_shadow(Some(shadow)));
    let m = e.molecule;
    assert_eq!(m.shadow_of(c), None);
    for (name, read) in saved_and_read(&m) {
        assert_same_shadows(&m, &read, name);
        let amine = read.atoms.iter().find(|a| a.element == "N").unwrap().id;
        assert_eq!(read.shadow_of(amine), None, "{name}: the other structure casts none");
        let oxygen = read.atoms.iter().find(|a| a.element == "O").unwrap().id;
        assert_eq!(read.shadow_of(oxygen), Some(shadow), "{name}");
    }
}

/// A fill or shadow removed again leaves nothing behind: the file is the one the plain drawing
/// writes (the writer's golden file pins that one), with no curve and no object tag in it.
#[test]
fn a_drawing_without_fills_and_shadows_writes_the_same_cdx_and_cdxml_as_before() {
    let (plain, ..) = two_structures();
    let mut p = plain.molecule;
    ring(&mut p, 6, [0.0, 10.0]);
    let (mut e, [c1, ..], _) = two_structures();
    let a = ring(&mut e.molecule, 6, [0.0, 10.0]);
    e.molecule.set_ring_fill(a.clone(), RED);
    assert!(e.set_shadow(Some(Shadow::default())));
    assert!(e.molecule.clear_ring_fill_containing(a[0]));
    assert!(e.set_shadow(None));
    assert!(e.molecule.shadow_of(c1).is_none());
    assert_eq!(molecule_to_cdx_document(&e.molecule), molecule_to_cdx_document(&p));
    let xml = molecule_to_cdxml(&e.molecule);
    assert_eq!(xml, molecule_to_cdxml(&p));
    assert!(!xml.contains("<curve") && !xml.contains("<objecttag"), "{xml}");
}

/// A curve drawn in ChemDraw is a shape the editor keeps, not a fill: saved over, it stays as
/// it was, and the editor's fill is written beside it once.
#[test]
fn chemdraws_own_curves_stay_shapes_and_fills_are_not_written_twice() {
    let mut m = Molecule::default();
    let ids = ring(&mut m, 6, [0.0, 0.0]);
    m.set_ring_fill(ids, RED);
    let xml = molecule_to_cdxml(&m).replace(
        "<fragment ",
        r#"<curve id="900" CurveType="129" FillType="Solid" CurvePoints="10 10 10 10 10 10 30 10 30 10 30 10 20 30 20 30 20 30"/><fragment "#,
    );
    let original = cdx_codec::write(&cdxml_document(&xml).unwrap());
    let import = read_cdx(&original).unwrap();
    assert_eq!(import.other_objects, 1, "ChemDraw's curve");
    assert_eq!(import.molecule.ring_fills.len(), 1);
    let mut again = original;
    for _ in 0..2 {
        again = merge_into_cdx(&again, &import.molecule);
    }
    let read = read_cdx(&again).unwrap();
    assert_eq!(read.other_objects, 1);
    assert_same_fills(&import.molecule, &read.molecule, "saved twice");
    let doc = cdx_codec::parse(&again).unwrap();
    let curves = doc.children[0].children.iter().filter(|c| c.tag == 0x8008).count();
    assert_eq!(curves, 2, "ChemDraw's and the fill");
}
