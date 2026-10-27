//! Characterization tests for what [`ChemStructEditor::ui`] paints.
//!
//! These pin the editor canvas's shape list — paint order, coordinates, colours and label
//! text — as a [`shape_fingerprint`], so that restructuring the draw path (scene building,
//! decorations, renderer) can be shown not to change the picture. The expected fingerprints
//! record the current behaviour; they are not a spec of what the drawing *should* be.
//!
//! Each scene is chosen to reach a different part of the draw path: ring fills, bond kinds,
//! label backgrounds, rich labels, the per-atom ring order (hotspot → selection → hover →
//! over-valence → label, also all on one atom), hovered bonds under a non-default camera,
//! solid and hashed wedges (the bond `fills` path, plain and hovered), and the bond-preview
//! and lasso overlays.

use super::ChemStructEditor;
use crate::molecule::{BondOrder, BondStereo};
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

/// Every per-atom decoration on an atom that is also over-valence, so the full per-atom order
/// hotspot → selection → hover → over-valence ring → label is pinned on one atom.
fn valence_ring_order_fingerprint() -> String {
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor::default();
    let m = &mut editor.molecule;
    let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
    let o = m.add_atom("O".to_string(), [1.5, 0.0], 0);
    m.add_bond(c, o, BondOrder::Triple);
    assert!(super::draw::is_valence_invalid(&editor.molecule, o), "C≡O must flag the oxygen");
    editor.hotspot_atom = Some(o);
    editor.selected_atoms.insert(o);

    frame(&ctx, &mut editor, Vec::new());
    let target = editor.last_label_rects[&o].center();
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(target)]);
    let fp = frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(target)]);

    assert_eq!(editor.hovered_atom, Some(o), "the pointer must hover the oxygen");
    assert_eq!(editor.hotspot_atom, Some(o));
    assert!(editor.selected_atoms.contains(&o));
    fp
}

/// Stereo bonds, which paint through the bond `fills` path: a solid wedge drawn normally, a
/// solid wedge under the pointer (hover tint on the fill), and a hashed wedge.
fn stereo_bonds_fingerprint() -> String {
    fn set_stereo(editor: &mut ChemStructEditor, bond: u32, stereo: BondStereo) {
        editor.molecule.bonds.iter_mut().find(|b| b.id == bond).expect("bond exists").stereo = stereo;
    }

    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor::default();
    let m = &mut editor.molecule;
    let c0 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
    let c1 = m.add_atom("C".to_string(), [1.5, 0.0], 0);
    let c2 = m.add_atom("C".to_string(), [-0.75, 1.3], 0);
    let o = m.add_atom("O".to_string(), [-0.75, -1.3], 0);
    let plain = m.add_bond(c0, c1, BondOrder::Single);
    let hovered = m.add_bond(c0, c2, BondOrder::Single);
    let hashed = m.add_bond(c0, o, BondOrder::Single);
    set_stereo(&mut editor, plain, BondStereo::WedgeUp);
    set_stereo(&mut editor, hovered, BondStereo::WedgeUp);
    set_stereo(&mut editor, hashed, BondStereo::WedgeDown);

    let mid = editor.mol_to_screen([-0.375, 0.65], CANVAS.center());
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid)]);
    let fp = frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid)]);

    assert_eq!(editor.hovered_bond, Some(hovered), "the pointer must hover the second wedge");
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
fn valence_ring_order_is_pinned() {
    assert_fingerprint("valence_ring_order", &valence_ring_order_fingerprint(), VALENCE_RING_ORDER);
}

#[test]
fn stereo_bonds_are_pinned() {
    assert_fingerprint("stereo_bonds", &stereo_bonds_fingerprint(), STEREO_BONDS);
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
    assert_eq!(valence_ring_order_fingerprint(), valence_ring_order_fingerprint());
    assert_eq!(stereo_bonds_fingerprint(), stereo_bonds_fingerprint());
    assert_eq!(overlays_fingerprint(), overlays_fingerprint());
}

const VALENCE_RING_ORDER: &str = r#"
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
line (500.000,400.000) (561.390,400.000) stroke=1.500/#000000ff
line (500.000,407.000) (561.390,407.000) stroke=1.500/#000000ff
line (500.000,393.000) (561.390,393.000) stroke=1.500/#000000ff
rect (562.390,382.000)-(587.609,418.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
circle c=(575.000,400.000) r=18.000 fill=#00000000 stroke=2.000/#ff8c00ff
circle c=(575.000,400.000) r=14.000 fill=#00000000 stroke=2.000/#5082ffff
circle c=(575.000,400.000) r=11.000 fill=#00000000 stroke=1.500/#32b432ff
line (591.000,400.000) (590.217,404.944) stroke=1.500/#dc1e1eff
line (587.944,409.405) (584.405,412.944) stroke=1.500/#dc1e1eff
line (579.944,415.217) (575.000,416.000) stroke=1.500/#dc1e1eff
line (570.056,415.217) (565.595,412.944) stroke=1.500/#dc1e1eff
line (562.056,409.405) (559.783,404.944) stroke=1.500/#dc1e1eff
line (559.000,400.000) (559.783,395.056) stroke=1.500/#dc1e1eff
line (562.056,390.595) (565.595,387.056) stroke=1.500/#dc1e1eff
line (570.056,384.783) (575.000,384.000) stroke=1.500/#dc1e1eff
line (579.944,384.783) (584.405,387.056) stroke=1.500/#dc1e1eff
line (587.944,390.595) (590.217,395.056) stroke=1.500/#dc1e1eff
text "O" at=(563.390,383.000) sections=["O":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;

const STEREO_BONDS: &str = r#"
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
path closed=true fill=#000000ff stroke=0.000/#00000000 pts=[(500.000,400.000) (575.000,405.000) (575.000,395.000)] kind=Middle
path closed=true fill=#32b432ff stroke=0.000/#00000000 pts=[(500.000,400.000) (458.169,462.501) (466.831,467.499)] kind=Middle
line (496.088,392.039) (495.066,392.628) stroke=1.500/#000000ff
line (492.176,384.077) (490.132,385.256) stroke=1.500/#000000ff
line (488.263,376.116) (485.198,377.884) stroke=1.500/#000000ff
line (484.351,368.155) (480.264,370.512) stroke=1.500/#000000ff
line (480.439,360.193) (475.330,363.140) stroke=1.500/#000000ff
line (476.527,352.232) (470.397,355.768) stroke=1.500/#000000ff
rect (429.890,317.000)-(475.859,353.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "HO" at=(430.890,318.000) sections=["HO":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;

const DECORATED_ATOMS: &str = r#"
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
path closed=true fill=#00640080 stroke=0.000/#00000000 pts=[(500.000,400.000) (575.000,400.000) (462.500,465.000)] kind=Middle
line (500.000,400.000) (562.365,400.000) stroke=1.500/#000000ff
line (496.968,398.251) (470.430,444.251) stroke=1.500/#000000ff
line (503.032,401.749) (476.493,447.749) stroke=1.500/#000000ff
line (500.000,400.000) (462.500,335.000) stroke=1.500/#000000ff
line (462.500,335.000) (401.109,335.000) stroke=1.500/#000000ff
line (462.500,328.000) (401.109,328.000) stroke=1.500/#000000ff
line (462.500,342.000) (401.109,342.000) stroke=1.500/#000000ff
rect (374.890,317.000)-(400.109,353.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (449.890,447.000)-(475.109,483.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (563.365,379.600)-(631.084,421.800) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
circle c=(500.000,400.000) r=14.000 fill=#00000000 stroke=2.000/#5082ffff
circle c=(575.000,400.000) r=18.000 fill=#00000000 stroke=2.000/#ff8c00ff
circle c=(575.000,400.000) r=14.000 fill=#00000000 stroke=2.000/#5082ffff
circle c=(575.000,400.000) r=11.000 fill=#00000000 stroke=1.500/#32b432ff
text "NH" at=(564.365,383.000) sections=["NH":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3" at=(606.396,396.800) sections=["3":#000000ff@21.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "+" at=(618.240,380.600) sections=["+":#000000ff@21.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(450.890,448.000) sections=["O":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
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
text "O" at=(375.890,318.000) sections=["O":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;

const HOVERED_BOND: &str = r#"
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
line (407.500,390.000) (520.000,446.250) stroke=2.250/#32b432ff
line (412.196,380.609) (507.821,428.421) stroke=2.250/#32b432ff
line (520.000,446.250) (610.263,401.119) stroke=2.250/#000000ff
rect (611.763,362.500)-(653.638,417.500) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "Cl" at=(613.263,364.000) sections=["Cl":#000000ff@45.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;

const OVERLAYS: &str = r#"
# bond preview
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (489.865,382.000)-(510.146,418.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "C" at=(490.865,383.000) sections=["C":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (500.000,400.000) (590.000,430.000) stroke=1.500/#3863b4b4
# lasso
clip (100.000,100.000)-(900.000,700.000)
rect (100.000,100.000)-(900.000,700.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (487.390,382.000)-(512.609,418.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "O" at=(488.390,383.000) sections=["O":#000000ff@30.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
path closed=true fill=#0c121e1e stroke=1.500/#5082ffff pts=[(380.000,310.000) (420.000,310.000) (460.000,320.000) (440.000,380.000)] kind=Middle
"#;
