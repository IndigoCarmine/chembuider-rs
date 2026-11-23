//! The Curly arrow tool and how curly arrows are bent, flipped, moved with their atoms, deleted,
//! copied, saved and drawn, driven through whole headless frames.

use super::*;
use crate::molecule::curly::{self, CurlyPath};
use crate::molecule::{CurlyArrow, CurlyEnd, CurlyKind};
use crate::testkit::run_frame_with_modifiers;

/// The 640x480 test screen; its center is molecule (0, 0) at the default view, 50 px a unit.
const SCREEN: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(640.0, 480.0));
const MID: egui::Pos2 = egui::pos2(320.0, 240.0);

fn at(mol: [f32; 2]) -> egui::Pos2 {
    MID + egui::vec2(mol[0], mol[1]) * SCALE_FACTOR
}

const SHIFT: egui::Modifiers = egui::Modifiers { shift: true, ..egui::Modifiers::NONE };

struct Driver {
    ctx: egui::Context,
    editor: ChemStructEditor,
}

/// The atoms of the drawing: a carbonyl carbon C bonded to O above it (a double bond) and to a
/// methyl carbon Me, and a hydroxide oxygen Nu off to the right.
struct Atoms {
    c: u32,
    o: u32,
    me: u32,
    nu: u32,
    co: u32,
}

impl Driver {
    fn new(kind: CurlyKind) -> (Self, Atoms) {
        let mut editor = ChemStructEditor { tool: Tool::CurlyArrow(kind), ..Default::default() };
        let m = &mut editor.molecule;
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        let o = m.add_atom("O".into(), [0.0, -1.5], 0);
        let me = m.add_atom("C".into(), [-1.3, 0.75], 0);
        let nu = m.add_atom("O".into(), [3.0, -0.5], -1);
        let co = m.add_bond(c, o, BondOrder::Double);
        m.add_bond(c, me, BondOrder::Single);
        let d = Driver { ctx: egui::Context::default(), editor };
        let mut d = d;
        // A first frame lays the labels out, for hit testing.
        d.frame_with(Vec::new(), Default::default());
        (d, Atoms { c, o, me, nu, co })
    }

    fn frame_with(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        run_frame_with_modifiers(&self.ctx, SCREEN, &mut self.editor, events, modifiers);
    }

    fn button(pos: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers }
    }

    fn click(&mut self, p: egui::Pos2) {
        self.frame_with(vec![egui::Event::PointerMoved(p)], Default::default());
        self.frame_with(vec![Self::button(p, true, Default::default())], Default::default());
        self.frame_with(vec![Self::button(p, false, Default::default())], Default::default());
    }

    fn drag(&mut self, from: egui::Pos2, to: egui::Pos2, modifiers: egui::Modifiers) {
        self.frame_with(vec![egui::Event::PointerMoved(from)], modifiers);
        self.frame_with(vec![Self::button(from, true, modifiers)], modifiers);
        for k in 1..=8 {
            let p = from + (to - from) * (k as f32 / 8.0);
            self.frame_with(vec![egui::Event::PointerMoved(p)], modifiers);
        }
        self.frame_with(vec![Self::button(to, false, modifiers)], modifiers);
    }

    fn curly(&self) -> Vec<CurlyArrow> {
        self.editor.molecule.curly_arrows().cloned().collect()
    }

    fn path(&self, c: &CurlyArrow) -> CurlyPath {
        curly::geometry(&self.editor.molecule, c).expect("placed")
    }
}

#[test]
fn a_drag_from_a_lone_pair_to_an_atom_draws_one_undoable_arrow_left_selected() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at([3.0, -0.5]), at([0.0, 0.0]), Default::default());
    let arrows = d.curly();
    assert_eq!(arrows.len(), 1);
    let c = &arrows[0];
    assert_eq!((c.tail, c.head, c.kind), (CurlyEnd::Atom(a.nu), CurlyEnd::Atom(a.c), CurlyKind::Pair));
    assert!(c.dots, "a lone pair shows its dots");
    assert!((c.angle.abs() - curly::DEFAULT_CURLY_ANGLE).abs() < 1e-3);
    assert_eq!(d.editor.selected_objects, [c.id].into_iter().collect(), "left selected, its handle showing");
    assert_eq!(d.editor.undo_stack.len(), 1);
    assert!(d.editor.undo());
    assert!(d.curly().is_empty());
    assert!(d.editor.redo());
    assert_eq!(d.curly().len(), 1);
}

#[test]
fn a_drag_from_a_bond_keeps_the_tail_on_the_side_pressed() {
    for (press, side) in [([0.0, -0.78], 1), ([0.0, -0.4], 0)] {
        let (mut d, a) = Driver::new(CurlyKind::Pair);
        let side = [a.c, a.o][side];
        d.drag(at(press), at([3.0, -0.5]), Default::default());
        let c = &d.curly()[0];
        assert_eq!(c.tail, CurlyEnd::Bond { bond: a.co, side }, "pressed at {press:?}");
        assert_eq!(c.head, CurlyEnd::Atom(a.nu));
        assert!(!c.dots);
    }
}

#[test]
fn shift_at_the_release_points_between_the_atoms_and_a_bond_can_be_the_head() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at([3.0, -0.5]), at([0.0, 0.0]), SHIFT);
    assert_eq!(d.curly()[0].head, CurlyEnd::Between(a.nu, a.c));
    // From the oxygen's lone pair into the C–Me bond.
    d.drag(at([0.0, -1.5]), at([-0.65, 0.375]), Default::default());
    let c = &d.curly()[1];
    assert!(matches!(c.head, CurlyEnd::Bond { bond, .. } if d.editor.molecule.bond_by_id(bond).unwrap().order == BondOrder::Single));
}

#[test]
fn a_drag_ending_on_its_own_atom_or_nowhere_much_draws_nothing_attached() {
    let (mut d, a) = Driver::new(CurlyKind::Single);
    d.drag(at([5.0, 3.0]), at([5.1, 3.0]), Default::default());
    assert!(d.curly().is_empty(), "too short to point anywhere");
    // To empty canvas: a free head.
    d.drag(at([3.0, -0.5]), at([5.0, 2.0]), Default::default());
    let c = &d.curly()[0];
    assert_eq!((c.tail, c.kind), (CurlyEnd::Atom(a.nu), CurlyKind::Single));
    assert!(matches!(c.head, CurlyEnd::Point(p) if (p[0] - 5.0).abs() < 0.05 && (p[1] - 2.0).abs() < 0.05));
}

#[test]
fn the_handle_bends_the_arrow_and_a_click_flips_it() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at([3.0, -0.5]), at([0.0, 0.0]), Default::default());
    let before = d.curly()[0].clone();
    let path = d.path(&before);
    // Drag the handle across the chord, farther out on the other side.
    let mid = [(path.tail[0] + path.head[0]) / 2.0, (path.tail[1] + path.head[1]) / 2.0];
    let out = [path.middle()[0] - mid[0], path.middle()[1] - mid[1]];
    let to = [mid[0] - out[0] * 1.5, mid[1] - out[1] * 1.5];
    d.drag(at(path.middle()), at(to), Default::default());
    let bent = d.curly()[0].clone();
    assert!(bent.angle.signum() != before.angle.signum(), "{} → {}", before.angle, bent.angle);
    let handle = d.path(&bent).middle();
    assert!((handle[0] - to[0]).hypot(handle[1] - to[1]) < 0.05, "the handle follows the pointer: {handle:?} {to:?}");
    assert_eq!(d.editor.undo_stack.len(), 2, "one undo step for the bend");
    // With Shift it stays a plain arc, only its depth following the pointer.
    let path = d.path(&bent);
    let deeper = [path.middle()[0] * 1.1, path.middle()[1] * 1.1];
    d.drag(at(path.middle()), at(deeper), SHIFT);
    assert_eq!(d.curly()[0].skew, 0.0, "{:?}", d.curly()[0]);
    // Without Shift it leans toward where it is pulled.
    let path = d.path(&d.curly()[0]);
    let toward_head = [path.middle()[0] + (path.head[0] - path.tail[0]) * 0.2, path.middle()[1] + (path.head[1] - path.tail[1]) * 0.2];
    d.drag(at(path.middle()), at(toward_head), Default::default());
    assert!(d.curly()[0].skew > 0.2, "{:?}", d.curly()[0]);
    // A click with the tool flips it.
    let leaning = d.curly()[0].clone();
    let p = d.path(&leaning).points[12];
    d.click(at(p));
    assert_eq!(d.curly()[0].angle, -leaning.angle);
    assert!(d.editor.undo());
    assert_eq!(d.curly()[0].angle, leaning.angle);
}

#[test]
fn the_arrow_follows_moved_atoms() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at([0.0, -0.4]), at([3.0, -0.5]), Default::default());
    let c = d.curly()[0].clone();
    let before = d.path(&c);
    // Move the hydroxide with the Select tool.
    d.editor.set_tool(Tool::Select);
    d.editor.selected_objects.clear();
    d.drag(at([3.0, -0.5]), at([3.0, 1.5]), Default::default());
    assert!((d.editor.molecule.atom_by_id(a.nu).unwrap().pos[1] - 1.5).abs() < 1e-3);
    let after = d.path(&d.curly()[0]);
    let nu = d.editor.molecule.atom_by_id(a.nu).unwrap().pos;
    assert!(((after.head[0] - nu[0]).hypot(after.head[1] - nu[1]) - 0.55).abs() < 1e-3, "{after:?}");
    assert!((after.tail[0] - before.tail[0]).abs() < 0.2, "the tail stays on its bond");
    // Turning the whole drawing turns the arrow with it.
    d.editor.selected_atoms = d.editor.molecule.atoms.iter().map(|a| a.id).collect();
    d.editor.selected_objects = [c.id].into_iter().collect();
    assert!(d.editor.rotate_selection_by(180.0));
    let turned = d.curly()[0].clone();
    assert_eq!((turned.tail, turned.head, turned.angle), (c.tail, c.head, c.angle), "only its atoms moved");
}

#[test]
fn deleting_atoms_frees_or_takes_the_arrows_on_them() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at([3.0, -0.5]), at([0.0, 0.0]), Default::default());
    let drawn = d.path(&d.curly()[0]);
    // The eraser on the hydroxide: the tail stays where it was drawn, free.
    d.editor.set_tool(Tool::Eraser);
    d.click(at([3.0, -0.5]));
    let c = d.curly()[0].clone();
    assert!(matches!(c.tail, CurlyEnd::Point(p) if (p[0] - drawn.tail[0]).abs() < 1e-3), "{c:?}");
    assert!(d.editor.undo());
    // Deleting the whole selection takes the arrows standing on it.
    d.editor.selected_atoms = [a.c, a.o, a.me, a.nu].into_iter().collect();
    d.editor.selected_objects.clear();
    assert!(d.editor.delete_selection());
    assert!(d.curly().is_empty());
}

#[test]
fn copy_and_paste_keep_the_arrows_on_the_pasted_atoms() {
    let (mut d, a) = Driver::new(CurlyKind::Single);
    d.drag(at([0.0, -0.78]), at([3.0, -0.5]), Default::default());
    d.editor.selected_atoms = [a.c, a.o, a.me, a.nu].into_iter().collect();
    d.editor.selected_objects.clear();
    let text = d.editor.copy_to_string().unwrap();
    assert!(d.editor.paste_from_string(&text));
    let arrows = d.curly();
    assert_eq!(arrows.len(), 2);
    let pasted = &arrows[1];
    let CurlyEnd::Bond { bond, side } = pasted.tail else { panic!("{pasted:?}") };
    assert!(!d.editor.molecule.atom_by_id(side).is_none() && side != a.o && side != a.c);
    assert_eq!(d.editor.molecule.atom_by_id(side).unwrap().element, "O");
    assert!(d.editor.molecule.bond_by_id(bond).is_some());
    assert_eq!((pasted.kind, pasted.angle), (arrows[0].kind, arrows[0].angle));
    assert!(d.editor.selected_objects.contains(&pasted.id));
}

#[test]
fn curly_arrows_save_as_json() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at([3.0, -0.5]), at([0.0, 0.0]), SHIFT);
    let json = serde_json::to_string(&d.editor.molecule).unwrap();
    assert!(json.contains("\"Curly\""), "{json}");
    let back: crate::molecule::Molecule = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d.editor.molecule);
}

#[test]
fn the_scene_draws_a_curve_a_head_and_the_lone_pair() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at([3.0, -0.5]), at([0.0, 0.0]), Default::default());
    let style = d.editor.config.style.clone();
    let xf = scene::RenderTransform { coord_scale: SCALE_FACTOR, offset: egui::vec2(320.0, 240.0), visual_scale: 1.0 };
    let c = d.curly()[0].clone();
    let obj = scene::build_curly(&c, &d.editor.molecule, &style, xf);
    assert!(obj.strokes.len() > 20, "a curve of many pieces");
    // Two head triangles and the two dots.
    assert_eq!(obj.fills.len(), 4);
    let path = d.path(&c);
    assert_eq!(obj.fills[0][0], xf.apply(path.head), "the head's tip is at the end of the curve");
    let nu = xf.apply(d.editor.molecule.atom_by_id(a.nu).unwrap().pos);
    let dot = obj.fills[2].iter().fold(egui::Vec2::ZERO, |s, p| s + p.to_vec2()) / 12.0;
    assert!((dot.to_pos2().distance(nu) - 0.52 * SCALE_FACTOR).abs() < 4.0, "beside the oxygen");
    // A fish-hook has half a head, on the outer side of the arc.
    let hook = CurlyArrow { kind: CurlyKind::Single, dots: false, ..c.clone() };
    let obj = scene::build_curly(&hook, &d.editor.molecule, &style, xf);
    assert_eq!(obj.fills.len(), 1);
    let barb = obj.fills[0][1];
    let chord_mid = (xf.apply(path.tail).to_vec2() + xf.apply(path.head).to_vec2()) * 0.5;
    let apex = xf.apply(path.middle()).to_vec2();
    let out = (apex - chord_mid).normalized();
    let head = xf.apply(path.head).to_vec2();
    assert!((barb.to_vec2() - head).dot(out) > 0.0, "the barb points out of the arc");
    // Drawn in the editor: the arrow is hit on its curve.
    let center = egui::pos2(320.0, 240.0);
    assert_eq!(d.editor.hit_test_object(xf.apply(path.points[20]), center), Some(c.id));
}
