//! The Curly arrow tool, and how curly arrows are drawn from their roles, reshaped by hand,
//! re-pointed, moved with their atoms, deleted, copied, saved and drawn, driven through whole
//! headless frames.

use super::*;
use crate::molecule::curly::{self, CurlyCase, CurlyPath};
use crate::molecule::{CurlyArrow, CurlyEnd, CurlyKind};

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
    /// The frames' clock, so that egui tells a double-click from two clicks a while apart.
    time: f64,
}

/// The user's sketch: acetone's C=O (O above C) with its methyls, an H up and to the right of
/// O, and a methoxide O⁻ (Nu) down and to the right.
struct Atoms {
    c: u32,
    o: u32,
    me: u32,
    h: u32,
    nu: u32,
    co: u32,
}

/// Where the sketch's atoms are (molecule units).
const C: [f32; 2] = [0.0, 0.0];
const O: [f32; 2] = [0.0, -1.5];
const H: [f32; 2] = [2.0, -2.8];
const NU: [f32; 2] = [3.2, 2.2];
/// On the C=O bond, nearer O (outside O's label), and nearer C.
const ON_CO_NEAR_O: [f32; 2] = [0.0, -0.8];
const ON_CO_NEAR_C: [f32; 2] = [0.0, -0.4];

impl Driver {
    fn new(kind: CurlyKind) -> (Self, Atoms) {
        let mut editor = ChemStructEditor { tool: Tool::CurlyArrow(kind), ..Default::default() };
        let m = &mut editor.molecule;
        let c = m.add_atom("C".into(), C, 0);
        let o = m.add_atom("O".into(), O, 0);
        let me = m.add_atom("C".into(), [-1.3, 0.75], 0);
        let me2 = m.add_atom("C".into(), [1.3, 0.75], 0);
        let co = m.add_bond(c, o, BondOrder::Double);
        m.add_bond(c, me, BondOrder::Single);
        m.add_bond(c, me2, BondOrder::Single);
        let h = m.add_atom("H".into(), H, 0);
        let nu = m.add_atom("O".into(), NU, -1);
        let nu_me = m.add_atom("C".into(), [4.5, 1.45], 0);
        m.add_bond(nu, nu_me, BondOrder::Single);
        let mut d = Driver { ctx: egui::Context::default(), editor, time: 0.0 };
        // A first frame lays the labels out, for hit testing.
        d.frame_with(Vec::new(), Default::default());
        (d, Atoms { c, o, me, h, nu, co })
    }

    fn frame_with(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.time += 1.0 / 60.0;
        let events = std::iter::once(egui::Event::ModifiersChanged(modifiers)).chain(events).collect();
        let raw = egui::RawInput { screen_rect: Some(SCREEN), events, focused: true, time: Some(self.time), ..Default::default() };
        let editor = &mut self.editor;
        let _ = crate::testkit::run_ui(&self.ctx, raw, |ui| {
            editor.ui(ui);
        });
    }

    fn button(pos: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers }
    }

    fn click(&mut self, p: egui::Pos2) {
        self.frame_with(vec![egui::Event::PointerMoved(p)], Default::default());
        self.frame_with(vec![Self::button(p, true, Default::default())], Default::default());
        self.frame_with(vec![Self::button(p, false, Default::default())], Default::default());
    }

    fn double_click(&mut self, p: egui::Pos2) {
        // Well after any click before it.
        self.time += 1.0;
        self.frame_with(vec![egui::Event::PointerMoved(p)], Default::default());
        for _ in 0..2 {
            self.frame_with(vec![Self::button(p, true, Default::default())], Default::default());
            self.frame_with(vec![Self::button(p, false, Default::default())], Default::default());
        }
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

    fn handles(&self) -> curly::CurlyHandles {
        crate::widget::interact::curly_handles(&self.editor).expect("a lone selected curly arrow").1
    }
}

#[test]
fn the_sketch_drawn_with_the_tool_records_its_roles_and_is_laid_out_as_an_s() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    // Pressed on C=O nearer O, released on H.
    d.drag(at(ON_CO_NEAR_O), at(H), Default::default());
    let c = d.curly()[0].clone();
    assert_eq!((c.from, c.to), (CurlyEnd::Bond { bond: a.co, onto: a.o }, CurlyEnd::Atom(a.h)));
    assert!(!c.is_shaped() && !c.flip, "drawn automatically");
    assert_eq!(c.case(&d.editor.molecule), CurlyCase::BondViaAtomToPartner);
    assert_eq!(d.editor.selected_curly_description().as_deref(), Some("C=O → (O) → H"));
    // The S: off the bond's middle, past O on its right, into H along O···H.
    let path = d.path(&c);
    assert!(path.pieces.len() == 2 && path.arc.is_none());
    assert!(path.points.iter().any(|p| (p[1] - O[1]).abs() < 0.3 && p[0] > O[0] + 0.5));
    // One undo step.
    assert_eq!(d.editor.undo_stack.len(), 1);
    assert!(d.editor.undo());
    assert!(d.curly().is_empty());
    assert!(d.editor.redo());
    // It follows the H: dragged further out with the Select tool, the S is laid out anew.
    d.editor.set_tool(Tool::Select);
    d.editor.selected_objects.clear();
    d.drag(at(H), at([3.0, -3.3]), Default::default());
    let moved = d.path(&d.curly()[0]);
    assert!(moved.head[0] > path.head[0] + 0.5, "{:?} {:?}", path.head, moved.head);
    assert!(moved.points.iter().any(|p| (p[1] - O[1]).abs() < 0.3 && p[0] > O[0] + 0.5), "still round O");
}

#[test]
fn a_lone_pair_drag_gives_a_lone_pair_to_atom_arrow_with_its_dots() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    let c = d.curly()[0].clone();
    assert_eq!((c.from, c.to, c.kind), (CurlyEnd::Atom(a.nu), CurlyEnd::Atom(a.c), CurlyKind::Pair));
    assert!(c.dots);
    assert_eq!(c.case(&d.editor.molecule), CurlyCase::LonePairToAtom);
    assert_eq!(d.editor.selected_objects, [c.id].into_iter().collect(), "left selected, its handles showing");
    assert!(d.path(&c).dots.is_some());
}

#[test]
fn a_bond_drag_takes_onto_from_the_half_pressed() {
    for (press, onto) in [(ON_CO_NEAR_O, 1), (ON_CO_NEAR_C, 0)] {
        let (mut d, a) = Driver::new(CurlyKind::Pair);
        let onto = [a.c, a.o][onto];
        d.drag(at(press), at(NU), Default::default());
        let c = &d.curly()[0];
        assert_eq!(c.from, CurlyEnd::Bond { bond: a.co, onto }, "pressed at {press:?}");
        assert_eq!(c.to, CurlyEnd::Atom(a.nu));
        assert!(!c.dots);
    }
}

#[test]
fn shift_points_between_the_atoms_and_a_bond_can_be_the_target() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), SHIFT);
    assert_eq!(d.curly()[0].to, CurlyEnd::Between(a.nu, a.c));
    // From O's lone pair into the C–Me bond.
    d.drag(at(O), at([-0.65, 0.375]), Default::default());
    let c = &d.curly()[1];
    assert!(matches!(c.to, CurlyEnd::Bond { bond, .. } if d.editor.molecule.bond_by_id(bond).unwrap().order == BondOrder::Single));
    let _ = a.me;
}

#[test]
fn a_drag_too_short_draws_nothing_and_one_to_empty_space_a_free_end() {
    let (mut d, a) = Driver::new(CurlyKind::Single);
    d.drag(at([5.0, -3.0]), at([5.1, -3.0]), Default::default());
    assert!(d.curly().is_empty(), "too short to point anywhere");
    d.drag(at(NU), at([5.0, -3.0]), Default::default());
    let c = &d.curly()[0];
    assert_eq!((c.from, c.kind), (CurlyEnd::Atom(a.nu), CurlyKind::Single));
    assert!(matches!(c.to, CurlyEnd::Point(p) if (p[0] - 5.0).abs() < 0.05 && (p[1] + 3.0).abs() < 0.05));
}

#[test]
fn dragging_a_control_point_shapes_it_by_hand_until_it_is_reset() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    let auto = d.curly()[0].clone();
    let auto_path = d.path(&auto);
    let steps = d.editor.undo_stack.len();
    let c1 = d.handles().controls[0].1;
    d.drag(at(c1), at([2.0, 3.2]), Default::default());
    let shaped = d.curly()[0].clone();
    assert!(shaped.is_shaped());
    assert_eq!(d.editor.undo_stack.len(), steps + 1, "one undo step");
    let moved = d.handles().controls[0].1;
    assert!((moved[0] - 2.0).abs() < 0.05 && (moved[1] - 3.2).abs() < 0.05, "{moved:?}");
    // The hand-set shape follows the atoms.
    let before = d.path(&shaped);
    let nu = d.editor.molecule.atoms.iter().position(|x| x.pos == NU).unwrap();
    d.editor.molecule.atoms[nu].pos = [NU[0] + 1.0, NU[1]];
    let after = d.path(&d.curly()[0]);
    assert!(after.tail[0] > before.tail[0] + 0.8);
    d.editor.molecule.atoms[nu].pos = NU;
    // Undo, redo; then back to automatic, one undo step.
    assert!(d.editor.undo() && !d.curly()[0].is_shaped());
    assert!(d.editor.redo() && d.curly()[0].is_shaped());
    d.editor.selected_objects = [shaped.id].into_iter().collect();
    assert!(d.editor.reset_selected_curly_shapes());
    assert!(!d.curly()[0].is_shaped());
    assert_eq!(d.path(&d.curly()[0]), auto_path);
    assert!(d.editor.undo() && d.curly()[0].is_shaped());
    // A double-click on a control point gives the automatic shape back too.
    d.editor.selected_objects = [shaped.id].into_iter().collect();
    let c1 = d.handles().controls[0].1;
    d.double_click(at(c1));
    assert!(!d.curly()[0].is_shaped());
}

#[test]
fn a_routed_arrow_gets_knots_by_double_clicks() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    // A double-click on the curve adds a knot; the shape does not change.
    let arrow = d.curly()[0].clone();
    let before = d.path(&arrow);
    let steps = d.editor.undo_stack.len();
    let on = before.points[before.points.len() / 3];
    d.double_click(at(on));
    let knotted = d.curly()[0].clone();
    assert_eq!(knotted.path.len(), 5, "{knotted:?}");
    assert_eq!(d.editor.undo_stack.len(), steps + 1);
    // Route it round the bottom: the knot below and left of the carbon.
    let knot = d.handles().knots[0].1;
    d.drag(at(knot), at([0.3, 2.6]), Default::default());
    let routed = d.path(&d.curly()[0]);
    assert!(routed.points.iter().any(|p| p[1] > 2.3), "{routed:?}");
    // A double-click on the knot takes it out again.
    let knot = d.handles().knots[0].1;
    d.double_click(at(knot));
    assert_eq!(d.curly()[0].path.len(), 2);
}

#[test]
fn dragging_an_end_changes_the_roles_and_keeps_it_automatic() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    // The target onto the H.
    let head = d.handles().head;
    let steps = d.editor.undo_stack.len();
    d.drag(at(head), at(H), Default::default());
    assert_eq!(d.curly()[0].to, CurlyEnd::Atom(a.h));
    assert_eq!(d.editor.undo_stack.len(), steps + 1, "one undo step");
    // The source onto the C=O bond, near O: its electrons, onto O; the sketch's arrow.
    let tail = d.handles().tail;
    d.drag(at(tail), at(ON_CO_NEAR_O), Default::default());
    let c = d.curly()[0].clone();
    assert_eq!(c.from, CurlyEnd::Bond { bond: a.co, onto: a.o });
    assert!(!c.dots && !c.is_shaped());
    assert_eq!(c.case(&d.editor.molecule), CurlyCase::BondViaAtomToPartner);
    // The target off into empty space: a free end there.
    let head = d.handles().head;
    d.drag(at(head), at([4.0, -2.0]), Default::default());
    assert!(matches!(d.curly()[0].to, CurlyEnd::Point(p) if (p[0] - 4.0).abs() < 0.05 && (p[1] + 2.0).abs() < 0.05));
    // Onto an atom with Shift: the gap between it and the source's atom (O).
    let head = d.handles().head;
    d.drag(at(head), at(NU), SHIFT);
    assert_eq!(d.curly()[0].to, CurlyEnd::Between(a.o, a.nu));
    assert!(d.editor.undo());
    assert!(matches!(d.curly()[0].to, CurlyEnd::Point(_)));
}

#[test]
fn a_click_selects_and_flip_turns_the_layout_to_the_other_side() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    let c = d.curly()[0].clone();
    let p = d.path(&c).points[16];
    d.editor.selected_objects.clear();
    d.click(at(p));
    assert_eq!(d.editor.selected_objects, [c.id].into_iter().collect());
    let before = d.path(&c);
    assert!(d.editor.flip_selected_curly_arrows());
    let flipped = d.curly()[0].clone();
    assert!(flipped.flip && !flipped.is_shaped());
    let after = d.path(&flipped);
    let side = |p: &CurlyPath| {
        let chord = [p.head[0] - p.tail[0], p.head[1] - p.tail[1]];
        let m = p.middle();
        (chord[0] * (m[1] - p.tail[1]) - chord[1] * (m[0] - p.tail[0])).signum()
    };
    assert_ne!(side(&before), side(&after));
    assert!(d.editor.undo());
    assert!(!d.curly()[0].flip);
}

#[test]
fn deleting_atoms_frees_or_takes_the_arrows_on_them() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    let drawn = d.path(&d.curly()[0]);
    // The eraser on the methoxide: the source stays where it was drawn, free.
    d.editor.set_tool(Tool::Eraser);
    d.click(at(NU));
    let c = d.curly()[0].clone();
    assert!(matches!(c.from, CurlyEnd::Point(p) if (p[0] - drawn.tail[0]).abs() < 1e-3), "{c:?}");
    assert!(d.editor.undo());
    // Deleting the whole selection takes the arrows standing on it.
    d.editor.selected_atoms = d.editor.molecule.atoms.iter().map(|x| x.id).collect();
    d.editor.selected_objects.clear();
    assert!(d.editor.delete_selection());
    assert!(d.curly().is_empty());
}

#[test]
fn copy_paste_and_json_keep_the_roles_and_the_shape() {
    let (mut d, a) = Driver::new(CurlyKind::Single);
    d.drag(at(ON_CO_NEAR_O), at(H), Default::default());
    assert!(d.editor.flip_selected_curly_arrows());
    d.drag(at(NU), at(C), Default::default());
    let c1 = d.handles().controls[0].1;
    d.drag(at(c1), at([2.0, 3.2]), Default::default());
    let json = serde_json::to_string(&d.editor.molecule).unwrap();
    assert!(json.contains("\"from\"") && json.contains("\"onto\"") && json.contains("\"path\""), "{json}");
    assert_eq!(serde_json::from_str::<crate::molecule::Molecule>(&json).unwrap(), d.editor.molecule);
    d.editor.selected_atoms = d.editor.molecule.atoms.iter().map(|x| x.id).collect();
    d.editor.selected_objects.clear();
    let text = d.editor.copy_to_string().unwrap();
    assert!(d.editor.paste_from_string(&text));
    let arrows = d.curly();
    assert_eq!(arrows.len(), 4);
    let (s, pasted) = (&arrows[0], &arrows[2]);
    let CurlyEnd::Bond { bond, onto } = pasted.from else { panic!("{pasted:?}") };
    assert!(bond != a.co && onto != a.o);
    assert_eq!(d.editor.molecule.atom_by_id(onto).unwrap().element, "O");
    assert_eq!((pasted.kind, pasted.flip), (s.kind, s.flip));
    assert_eq!(arrows[3].path, arrows[1].path, "the hand-set shape too");
    // Pasted a bond length right and down, it is drawn so.
    let (x, y) = (d.path(s), d.path(pasted));
    for (p, q) in x.points.iter().zip(&y.points) {
        assert!((q[0] - p[0] - DEFAULT_BOND_LENGTH).abs() < 1e-3 && (q[1] - p[1] - DEFAULT_BOND_LENGTH).abs() < 1e-3);
    }
}

#[test]
fn turning_the_drawing_keeps_the_roles() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at(ON_CO_NEAR_O), at(H), Default::default());
    let c = d.curly()[0].clone();
    d.editor.selected_atoms = d.editor.molecule.atoms.iter().map(|x| x.id).collect();
    d.editor.selected_objects = [c.id].into_iter().collect();
    assert!(d.editor.rotate_selection_by(180.0));
    let turned = d.curly()[0].clone();
    assert_eq!((turned.from, turned.to, turned.flip), (c.from, c.to, c.flip), "only its atoms moved");
}

#[test]
fn the_scene_draws_a_curve_a_head_and_the_lone_pair() {
    let (mut d, a) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
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
    assert!(dot.to_pos2().distance(nu) < 0.8 * SCALE_FACTOR, "beside the oxygen");
    // A fish-hook has half a head, on the outer side of the curve's last turn.
    let hook = CurlyArrow { kind: CurlyKind::Single, dots: false, ..c.clone() };
    let obj = scene::build_curly(&hook, &d.editor.molecule, &style, xf);
    assert_eq!(obj.fills.len(), 1);
    let barb = obj.fills[0][1];
    let chord_mid = (xf.apply(path.tail).to_vec2() + xf.apply(path.head).to_vec2()) * 0.5;
    let apex = xf.apply(path.middle()).to_vec2();
    let out = (apex - chord_mid).normalized();
    assert!((barb.to_vec2() - xf.apply(path.head).to_vec2()).dot(out) > 0.0, "the barb points out of the arc");
    // In the editor the arrow is hit on its curve.
    assert_eq!(d.editor.hit_test_object(xf.apply(path.points[20]), egui::pos2(320.0, 240.0)), Some(c.id));
}

/// A click whose press this editor never saw — pressed on a dialog or a menu, or on another
/// document's canvas in the same place, before this one was shown — does nothing to it: an
/// opened drawing is neither selected nor changed by the click that opened it.
#[test]
fn a_click_pressed_before_the_editor_was_shown_is_not_its_own() {
    let (mut d, _) = Driver::new(CurlyKind::Pair);
    d.drag(at(NU), at(C), Default::default());
    let opened = d.editor.molecule.clone();
    let p = at(d.path(&d.curly()[0]).points[16]);
    // The press goes to another editor shown in the same place.
    let mut other = ChemStructEditor { tool: Tool::CurlyArrow(CurlyKind::Pair), ..Default::default() };
    std::mem::swap(&mut d.editor, &mut other);
    d.time += 1.0;
    d.frame_with(vec![egui::Event::PointerMoved(p)], Default::default());
    d.frame_with(vec![Driver::button(p, true, Default::default())], Default::default());
    // The drawing is shown, and the button comes up over its arrow.
    std::mem::swap(&mut d.editor, &mut other);
    d.editor.selected_objects.clear();
    d.frame_with(vec![Driver::button(p, false, Default::default())], Default::default());
    assert!(d.editor.selected_objects.is_empty(), "nothing selected");
    assert_eq!(d.editor.molecule, opened, "nothing changed");
    // Its own clicks still work.
    d.click(p);
    assert_eq!(d.editor.selected_objects.len(), 1);
}
