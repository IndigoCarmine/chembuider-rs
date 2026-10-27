//! Characterization tests for what [`ChemStructEditor::ui`] paints.
//!
//! These pin the editor canvas's shape list — paint order, coordinates, colours and label
//! text — as a [`shape_fingerprint`], so that restructuring the draw path (scene building,
//! decorations, renderer) can be shown not to change the picture. The expected fingerprints
//! record the current behaviour; they are not a spec of what the drawing *should* be.
//!
//! Each scene is chosen to reach a different part of the draw path: ring fills, bond kinds,
//! label backgrounds, rich labels, the per-atom ring order (hotspot → selection → hover →
//! over-valence), hovered bonds under a non-default camera, and the bond-preview and lasso
//! overlays.

use super::ChemStructEditor;
use crate::molecule::BondOrder;
use crate::testkit::{shape_fingerprint, CANVAS};

/// Run one frame on [`CANVAS`] and return its fingerprint.
fn frame(ctx: &egui::Context, editor: &mut ChemStructEditor, events: Vec<egui::Event>) -> String {
    let raw = egui::RawInput {
        screen_rect: Some(CANVAS),
        events,
        focused: true,
        ..Default::default()
    };
    let output = ctx.run_ui(raw, |ui| {
        editor.ui(ui);
    });
    shape_fingerprint(&output.shapes)
}

fn pointer_button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::default(),
    }
}

/// Compare against the pinned fingerprint, printing the whole actual value on mismatch so a
/// reviewer can see exactly which shapes moved.
#[track_caller]
fn assert_fingerprint(name: &str, actual: &str, expected: &str) {
    let expected = expected.trim();
    if actual != expected {
        let first = actual
            .lines()
            .zip(expected.lines())
            .position(|(a, e)| a != e)
            .unwrap_or_else(|| actual.lines().count().min(expected.lines().count()));
        panic!(
            "{name}: painted shapes differ from the pinned fingerprint (first difference at \
             line {})\n--- actual ---\n{actual}\n--- expected ---\n{expected}",
            first + 1,
        );
    }
}

/// A small molecule that reaches every static layer: a ring fill, single/double/triple
/// bonds, a charged rich label ("NH3+"), a plain label, and an over-valence atom.
///
/// Returns the editor, the id of the charged nitrogen and the id of the carbon bonded to it.
fn decorated_molecule() -> (ChemStructEditor, u32, u32) {
    let mut editor = ChemStructEditor::default();
    let m = &mut editor.molecule;
    let c1 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
    let n = m.add_atom("N".to_string(), [1.5, 0.0], 1);
    let o = m.add_atom("O".to_string(), [-0.75, 1.3], 0);
    let c2 = m.add_atom("C".to_string(), [-0.75, -1.3], 0);
    let o2 = m.add_atom("O".to_string(), [-2.25, -1.3], 0);
    m.add_bond(c1, n, BondOrder::Single);
    m.add_bond(c1, o, BondOrder::Double);
    m.add_bond(c1, c2, BondOrder::Single);
    // O≡C: the oxygen is over-valence, so it gets the dashed warning ring.
    m.add_bond(c2, o2, BondOrder::Triple);
    m.set_ring_fill(vec![c1, n, o], [0, 200, 0, 128]);
    (editor, n, c1)
}

/// Every per-atom decoration at once on the charged nitrogen — hotspot, selection and hover
/// — plus a selected unlabeled carbon, so the relative order of the rings is pinned.
fn decorated_atoms_fingerprint() -> String {
    let ctx = egui::Context::default();
    let (mut editor, n, c1) = decorated_molecule();
    editor.hotspot_atom = Some(n);
    editor.selected_atoms.insert(n);
    editor.selected_atoms.insert(c1);

    // A frame without a pointer to record the label rects, then hover the nitrogen's label.
    frame(&ctx, &mut editor, Vec::new());
    let target = editor.last_label_rects[&n].center();
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(target)]);
    let fp = frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(target)]);

    assert_eq!(editor.hovered_atom, Some(n), "the pointer must hover the nitrogen");
    assert_eq!(editor.hotspot_atom, Some(n));
    assert!(editor.selected_atoms.contains(&n) && editor.selected_atoms.contains(&c1));
    fp
}

/// A hovered bond under a non-default camera (zoom and pan), so the zoom-scaled widths and
/// the hovered-bond tint are pinned.
fn hovered_bond_fingerprint() -> String {
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor { zoom: 1.5, pan: egui::vec2(20.0, -10.0), ..Default::default() };
    let m = &mut editor.molecule;
    let a = m.add_atom("C".to_string(), [-1.5, 0.0], 0);
    let b = m.add_atom("C".to_string(), [0.0, 0.75], 0);
    let c = m.add_atom("Cl".to_string(), [1.5, 0.0], 0);
    let ab = m.add_bond(a, b, BondOrder::Double);
    m.add_bond(b, c, BondOrder::Single);

    let center = CANVAS.center();
    let mid = editor.mol_to_screen([-0.75, 0.375], center);
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid)]);
    let fp = frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid)]);

    assert_eq!(editor.hovered_bond, Some(ab), "the pointer must hover the double bond");
    fp
}

/// The two overlays: a Bond-tool drag in progress (preview line) and, separately, a
/// Select-tool drag on empty canvas (lasso).
fn overlays_fingerprint() -> String {
    let center = CANVAS.center();

    // Bond preview: press on the atom, drag away, and stop mid-gesture.
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor { tool: super::Tool::Bond, ..Default::default() };
    let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
    let start = editor.mol_to_screen([0.0, 0.0], center);
    let path = [start + egui::vec2(20.0, 5.0), start + egui::vec2(60.0, 20.0), start + egui::vec2(90.0, 30.0)];
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(start)]);
    frame(&ctx, &mut editor, vec![pointer_button(start, true)]);
    let mut preview = String::new();
    for p in path {
        preview = frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(p)]);
    }
    assert_eq!(editor.bond_start, Some(a), "the drag must start a bond from the atom");
    assert!(editor.preview_end_screen.is_some(), "a preview line must be in flight");

    // Lasso: press on empty canvas and trace a triangle.
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor::default();
    editor.molecule.add_atom("O".to_string(), [0.0, 0.0], 0);
    let origin = center + egui::vec2(-120.0, -90.0);
    let trace = [origin + egui::vec2(40.0, 0.0), origin + egui::vec2(80.0, 10.0), origin + egui::vec2(60.0, 70.0)];
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(origin)]);
    frame(&ctx, &mut editor, vec![pointer_button(origin, true)]);
    let mut lasso = String::new();
    for p in trace {
        lasso = frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(p)]);
    }
    assert!(editor.lasso_path.len() > 1, "a lasso must be in flight");

    format!("# bond preview\n{preview}\n# lasso\n{lasso}")
}

#[test]
fn decorated_atoms_are_pinned() {
    assert_fingerprint("decorated_atoms", &decorated_atoms_fingerprint(), DECORATED_ATOMS);
}

#[test]
fn hovered_bond_is_pinned() {
    assert_fingerprint("hovered_bond", &hovered_bond_fingerprint(), HOVERED_BOND);
}

#[test]
fn overlays_are_pinned() {
    assert_fingerprint("overlays", &overlays_fingerprint(), OVERLAYS);
}

/// The fingerprint must not depend on anything carried between contexts (font atlas state,
/// ids, hashing order): building the same scenes again in fresh contexts gives the same text.
#[test]
fn fingerprints_are_reproducible() {
    assert_eq!(decorated_atoms_fingerprint(), decorated_atoms_fingerprint());
    assert_eq!(hovered_bond_fingerprint(), hovered_bond_fingerprint());
    assert_eq!(overlays_fingerprint(), overlays_fingerprint());
}

const DECORATED_ATOMS: &str = r#"
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000
path closed=true fill=#00640080 stroke=0.000/#00000000 pts=[(500.000,400.000) (575.000,400.000) (462.500,465.000)]
line (500.000,400.000) (562.365,400.000) stroke=1.500/#000000ff
line (496.968,398.251) (470.430,444.251) stroke=1.500/#000000ff
line (503.032,401.749) (476.493,447.749) stroke=1.500/#000000ff
line (500.000,400.000) (462.500,335.000) stroke=1.500/#000000ff
line (462.500,335.000) (401.109,335.000) stroke=1.500/#000000ff
line (462.500,328.000) (401.109,328.000) stroke=1.500/#000000ff
line (462.500,342.000) (401.109,342.000) stroke=1.500/#000000ff
rect (374.890,317.000)-(400.109,353.000) fill=#ffffffff stroke=0.000/#00000000
rect (449.890,447.000)-(475.109,483.000) fill=#ffffffff stroke=0.000/#00000000
rect (563.365,379.600)-(631.084,421.800) fill=#ffffffff stroke=0.000/#00000000
circle c=(500.000,400.000) r=14.000 fill=#00000000 stroke=2.000/#5082ffff
circle c=(575.000,400.000) r=18.000 fill=#00000000 stroke=2.000/#ff8c00ff
circle c=(575.000,400.000) r=14.000 fill=#00000000 stroke=2.000/#5082ffff
circle c=(575.000,400.000) r=11.000 fill=#00000000 stroke=1.500/#32b432ff
text "NH" at=(564.365,383.000) sections=["NH":#000000ff@30.000]
text "3" at=(606.396,396.800) sections=["3":#000000ff@21.000]
text "+" at=(618.240,380.600) sections=["+":#000000ff@21.000]
text "O" at=(450.890,448.000) sections=["O":#000000ff@30.000]
line (403.500,335.000) (402.717,339.944) stroke=1.500/#dc1e1eff
line (400.444,344.405) (396.905,347.944) stroke=1.500/#dc1e1eff
line (392.444,350.217) (387.500,351.000) stroke=1.500/#dc1e1eff
line (382.556,350.217) (378.095,347.944) stroke=1.500/#dc1e1eff
line (374.556,344.405) (372.283,339.944) stroke=1.500/#dc1e1eff
line (371.500,335.000) (372.283,330.056) stroke=1.500/#dc1e1eff
line (374.556,325.595) (378.095,322.056) stroke=1.500/#dc1e1eff
line (382.556,319.783) (387.500,319.000) stroke=1.500/#dc1e1eff
line (392.444,319.783) (396.905,322.056) stroke=1.500/#dc1e1eff
line (400.444,325.595) (402.717,330.056) stroke=1.500/#dc1e1eff
text "O" at=(375.890,318.000) sections=["O":#000000ff@30.000]
"#;

const HOVERED_BOND: &str = r#"
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000
line (407.500,390.000) (520.000,446.250) stroke=2.250/#32b432ff
line (429.071,389.046) (507.821,428.421) stroke=2.250/#32b432ff
line (520.000,446.250) (610.263,401.119) stroke=2.250/#000000ff
rect (611.763,362.500)-(653.638,417.500) fill=#ffffffff stroke=0.000/#00000000
text "Cl" at=(613.263,364.000) sections=["Cl":#000000ff@45.000]
"#;

const OVERLAYS: &str = r#"
# bond preview
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000
rect (489.865,382.000)-(510.146,418.000) fill=#ffffffff stroke=0.000/#00000000
text "C" at=(490.865,383.000) sections=["C":#000000ff@30.000]
line (500.000,400.000) (590.000,430.000) stroke=1.500/#3863b4b4
# lasso
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000
rect (487.390,382.000)-(512.609,418.000) fill=#ffffffff stroke=0.000/#00000000
text "O" at=(488.390,383.000) sections=["O":#000000ff@30.000]
path closed=true fill=#0c121e1e stroke=1.500/#5082ffff pts=[(380.000,310.000) (420.000,310.000) (460.000,320.000) (440.000,380.000)]
"#;
