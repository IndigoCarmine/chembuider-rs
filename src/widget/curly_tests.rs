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
fn the_handle_bends_the_arc_and_a_click_selects_it() {
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
    // A click with the tool selects it alone; the menu's flip turns it over, one undo step.
    let leaning = d.curly()[0].clone();
    let p = d.path(&leaning).points[12];
    d.editor.selected_objects.clear();
    d.click(at(p));
    assert_eq!(d.editor.selected_objects, [leaning.id].into_iter().collect());
    assert_eq!(d.curly()[0].angle, leaning.angle);
    assert!(d.editor.flip_selected_curly_arrows());
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
    let gap = (after.head[0] - nu[0]).hypot(after.head[1] - nu[1]);
    assert!(gap > 0.45 && gap < 0.75, "stops short of the oxygen's label: {gap} {after:?}");
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
    assert!(obj.fills[0][0].distance(xf.apply(path.head)) < 1e-3, "the head's tip is at the end of the curve");
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

// ─── Free shapes and re-pointing ──────────────────────────────────────────────

/// The user's two arrows: a ketone C=O (O above C), an H up and to the right, and a methoxide
/// O⁻ below and to the right. Atoms: (c, o, h, nu), and the C=O bond.
fn sketch() -> (Driver, [u32; 4], u32) {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    let h = d.editor.molecule.add_atom("H".into(), [2.4, -2.4], 0);
    d.editor.molecule.atom_by_id_mut(a.nu).unwrap().pos = [3.5, 1.8];
    d.frame_with(Vec::new(), Default::default());
    (d, [a.c, a.o, h, a.nu], a.co)
}

fn handle_of(d: &Driver, find: impl Fn(&crate::molecule::curly::CurlyHandles) -> [f32; 2]) -> [f32; 2] {
    let (_, h) = crate::widget::interact::curly_handles(&d.editor).expect("a lone selected curly arrow");
    find(&h)
}

#[test]
fn an_s_shaped_arrow_is_two_control_drags_each_undoable() {
    let (mut d, [_, o, h, _], co) = sketch();
    // From the C=O bond, on the O side, to the H.
    d.drag(at([0.0, -0.78]), at([2.4, -2.4]), Default::default());
    let c = d.curly()[0].clone();
    assert_eq!((c.tail, c.head), (CurlyEnd::Bond { bond: co, side: o }, CurlyEnd::Atom(h)));
    let steps = d.editor.undo_stack.len();
    // The first control point down and right, the second up and left: an S.
    let c1 = handle_of(&d, |h| h.controls[0].1);
    d.drag(at(c1), at([1.6, 0.3]), Default::default());
    let c2 = handle_of(&d, |h| h.controls[1].1);
    d.drag(at(c2), at([0.4, -2.6]), Default::default());
    let s = d.curly()[0].clone();
    assert!(s.is_shaped() && s.path.len() == 2, "{s:?}");
    assert_eq!(d.editor.undo_stack.len(), steps + 2, "one undo step per drag");
    // It runs right and down, turns back up and left, then reaches the H: its points cross the
    // line between its ends.
    let path = d.path(&s);
    let (p, q) = (path.tail, path.head);
    let side = |x: [f32; 2]| ((q[0] - p[0]) * (x[1] - p[1]) - (q[1] - p[1]) * (x[0] - p[0])).signum();
    let sides: Vec<f32> = path.points.iter().step_by(6).skip(1).map(|x| side(*x)).collect();
    assert!(sides.contains(&1.0) && sides.contains(&-1.0), "an S crosses its chord: {sides:?}");
    // The handle went where it was dragged.
    let moved = handle_of(&d, |h| h.controls[1].1);
    assert!((moved[0] - 0.4).abs() < 0.05 && (moved[1] + 2.6).abs() < 0.05, "{moved:?}");
    assert!(d.editor.undo());
    assert!(d.curly()[0].is_shaped(), "one drag back: still shaped by the first");
    assert!(d.editor.undo());
    assert!(!d.curly()[0].is_shaped(), "two back: the arc it was drawn as");
    assert!(d.editor.redo() && d.editor.redo());
    assert_eq!(d.curly()[0], s);
    // The S follows its atoms: everything moved by (1, 1) moves the curve by the same.
    let all: Vec<u32> = d.editor.molecule.atoms.iter().map(|a| a.id).collect();
    for id in all {
        let a = d.editor.molecule.atom_by_id_mut(id).unwrap();
        a.pos = [a.pos[0] + 1.0, a.pos[1] + 1.0];
    }
    let shifted = d.path(&d.curly()[0]);
    for (x, y) in path.points.iter().zip(&shifted.points) {
        assert!((y[0] - x[0] - 1.0).abs() < 1e-3 && (y[1] - x[1] - 1.0).abs() < 1e-3);
    }
}

#[test]
fn a_routed_arrow_gets_knots_by_double_clicks_and_follows_its_atoms() {
    let (mut d, [c, _, _, nu], _) = sketch();
    // From the methoxide's lone pair to the carbonyl carbon.
    d.drag(at([3.5, 1.8]), at([0.0, 0.0]), Default::default());
    let arrow = d.curly()[0].clone();
    assert_eq!((arrow.tail, arrow.head), (CurlyEnd::Atom(nu), CurlyEnd::Atom(c)));
    // A double-click on the curve adds a knot; the shape does not change.
    let mid = d.path(&arrow).middle();
    let steps = d.editor.undo_stack.len();
    d.frame_with(vec![egui::Event::PointerMoved(at(mid))], Default::default());
    for _ in 0..2 {
        d.frame_with(vec![Driver::button(at(mid), true, Default::default())], Default::default());
        d.frame_with(vec![Driver::button(at(mid), false, Default::default())], Default::default());
    }
    let knotted = d.curly()[0].clone();
    assert_eq!(knotted.path.len(), 5, "{knotted:?}");
    assert_eq!(d.editor.undo_stack.len(), steps + 1);
    // Route it: the knot down to the bottom left, below the carbon.
    let knot = handle_of(&d, |h| h.knots[0].1);
    d.drag(at(knot), at([0.2, 2.6]), Default::default());
    let first_control = handle_of(&d, |h| h.controls[0].1);
    d.drag(at(first_control), at([2.0, 2.6]), Default::default());
    let last_control = handle_of(&d, |h| h.controls[3].1);
    d.drag(at(last_control), at([0.0, 1.6]), Default::default());
    let routed = d.curly()[0].clone();
    let path = d.path(&routed);
    // It runs along the bottom and comes up into the carbon from below.
    assert!(path.points.iter().any(|p| p[1] > 2.0), "{path:?}");
    let n = path.points.len();
    let into = [path.points[n - 1][0] - path.points[n - 4][0], path.points[n - 1][1] - path.points[n - 4][1]];
    assert!(into[1] < 0.0 && into[1].abs() > into[0].abs(), "the head points up: {into:?}");
    // Moving the methoxide stretches the route with it; the carbon end stays.
    d.editor.molecule.atom_by_id_mut(nu).unwrap().pos[0] += 1.0;
    let moved = d.path(&d.curly()[0]);
    assert!((moved.head[0] - path.head[0]).abs() < 0.05 && (moved.head[1] - path.head[1]).abs() < 0.05);
    assert!(moved.tail[0] > path.tail[0] + 0.8);
    // A double-click on the knot takes it out again.
    let knot = handle_of(&d, |h| h.knots[0].1);
    d.frame_with(vec![egui::Event::PointerMoved(at(knot))], Default::default());
    for _ in 0..2 {
        d.frame_with(vec![Driver::button(at(knot), true, Default::default())], Default::default());
        d.frame_with(vec![Driver::button(at(knot), false, Default::default())], Default::default());
    }
    assert_eq!(d.curly()[0].path.len(), 2);
}

#[test]
fn an_end_dragged_onto_something_else_stands_on_it() {
    let (mut d, [c, o, h, nu], co) = sketch();
    d.drag(at([3.5, 1.8]), at([0.0, 0.0]), Default::default());
    let id = d.curly()[0].id;
    // The head onto the H.
    let head = handle_of(&d, |h| h.head);
    let steps = d.editor.undo_stack.len();
    d.drag(at(head), at([2.4, -2.4]), Default::default());
    assert_eq!(d.curly()[0].head, CurlyEnd::Atom(h));
    assert_eq!(d.editor.undo_stack.len(), steps + 1, "one undo step");
    // The tail onto the C=O bond, near O: its electrons, kept on O's side.
    let tail = handle_of(&d, |h| h.tail);
    d.drag(at(tail), at([0.0, -0.78]), Default::default());
    let a = d.curly()[0].clone();
    assert_eq!(a.tail, CurlyEnd::Bond { bond: co, side: o });
    assert!(!a.dots, "no lone pair on a bond");
    // The head off into empty space: a free end there.
    let head = handle_of(&d, |h| h.head);
    d.drag(at(head), at([4.0, -2.0]), Default::default());
    assert!(matches!(d.curly()[0].head, CurlyEnd::Point(p) if (p[0] - 4.0).abs() < 0.05 && (p[1] + 2.0).abs() < 0.05));
    // Back onto an atom with Shift: between it and the tail's atom.
    let head = handle_of(&d, |h| h.head);
    d.drag(at(head), at([0.0, 0.0]), SHIFT);
    assert_eq!(d.curly()[0].head, CurlyEnd::Between(o, c));
    assert!(d.editor.undo());
    assert!(matches!(d.curly()[0].head, CurlyEnd::Point(_)));
    let _ = (id, nu);
}

#[test]
fn shaped_arrows_copy_paste_and_save_with_their_shape() {
    let (mut d, [c, o, h, nu], _) = sketch();
    d.drag(at([0.0, -0.78]), at([2.4, -2.4]), Default::default());
    let c1 = handle_of(&d, |h| h.controls[0].1);
    d.drag(at(c1), at([1.6, 0.3]), Default::default());
    let shaped = d.curly()[0].clone();
    assert!(shaped.is_shaped());
    let json = serde_json::to_string(&d.editor.molecule).unwrap();
    assert!(json.contains("\"path\""), "{json}");
    assert_eq!(serde_json::from_str::<crate::molecule::Molecule>(&json).unwrap(), d.editor.molecule);
    d.editor.selected_atoms = [c, o, h, nu].into_iter().chain([d.editor.molecule.atoms[2].id]).collect();
    d.editor.selected_objects.clear();
    let text = d.editor.copy_to_string().unwrap();
    assert!(d.editor.paste_from_string(&text));
    let pasted = d.curly()[1].clone();
    assert_eq!(pasted.path, shaped.path);
    // Pasted a bond length right and down, it is drawn so.
    let (x, y) = (d.path(&shaped), d.path(&pasted));
    for (p, q) in x.points.iter().zip(&y.points) {
        assert!((q[0] - p[0] - DEFAULT_BOND_LENGTH).abs() < 1e-3 && (q[1] - p[1] - DEFAULT_BOND_LENGTH).abs() < 1e-3);
    }
}
