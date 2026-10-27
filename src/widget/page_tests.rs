//! The page-object tools (Arrow, Text) and how Select, Eraser, Delete and undo treat arrows
//! and texts, driven through whole headless frames.

use super::*;
use crate::molecule::{PageArrow, PageText};
use crate::testkit::run_frame_with_modifiers;

/// The 640x480 test screen; its center is molecule (0, 0) at the default view, 50 px a unit.
const SCREEN: egui::Rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(640.0, 480.0));
const MID: egui::Pos2 = egui::pos2(320.0, 240.0);

fn at(mol: [f32; 2]) -> egui::Pos2 {
    MID + egui::vec2(mol[0], mol[1]) * SCALE_FACTOR
}

struct Driver {
    ctx: egui::Context,
    editor: ChemStructEditor,
}

impl Driver {
    fn new(tool: Tool) -> Self {
        Self { ctx: egui::Context::default(), editor: ChemStructEditor { tool, ..Default::default() } }
    }

    fn frame_with(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        run_frame_with_modifiers(&self.ctx, SCREEN, &mut self.editor, events, modifiers);
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.frame_with(events, Default::default());
    }

    fn button(pos: egui::Pos2, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers }
    }

    fn click(&mut self, p: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(p)]);
        self.frame(vec![Self::button(p, true, Default::default())]);
        self.frame(vec![Self::button(p, false, Default::default())]);
    }

    fn double_click(&mut self, p: egui::Pos2) {
        self.frame(vec![egui::Event::PointerMoved(p)]);
        for _ in 0..2 {
            self.frame(vec![Self::button(p, true, Default::default())]);
            self.frame(vec![Self::button(p, false, Default::default())]);
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

    fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.frame_with(
            vec![
                egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers },
                egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers },
            ],
            modifiers,
        );
    }

    fn type_text(&mut self, text: &str) {
        // The box takes focus the frame after it opens (see `testkit::text_frames`).
        self.frame(Vec::new());
        self.frame(vec![egui::Event::Text(text.to_string())]);
    }

    fn arrows(&self) -> Vec<PageArrow> {
        self.editor.molecule.objects.iter().filter_map(|o| o.as_arrow().cloned()).collect()
    }

    fn texts(&self) -> Vec<PageText> {
        self.editor.molecule.objects.iter().filter_map(|o| o.as_text().cloned()).collect()
    }
}

fn close(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

#[test]
fn dragging_with_the_arrow_tool_draws_a_snapped_arrow_as_one_undo_step() {
    let mut d = Driver::new(Tool::Arrow(ArrowKind::Reaction));
    // 4 units right and a little down: snaps to horizontal.
    d.drag(at([0.0, 0.0]), at([4.0, 0.3]), Default::default());
    let a = d.arrows();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].kind, ArrowKind::Reaction);
    assert!(close(a[0].tail, [0.0, 0.0]), "{:?}", a[0]);
    assert!(a[0].head[1].abs() < 1e-3 && (a[0].head[0] - (16.0f32 + 0.09).sqrt()).abs() < 1e-3, "{:?}", a[0]);
    assert!(d.editor.object_drag.is_none());
    assert_eq!(d.editor.undo_stack.len(), 1);
    assert!(d.editor.undo());
    assert!(d.editor.molecule.objects.is_empty());
    assert!(d.editor.redo());
    assert_eq!(d.arrows().len(), 1);
}

#[test]
fn shift_draws_an_arrow_at_any_angle() {
    let mut d = Driver::new(Tool::Arrow(ArrowKind::Equilibrium));
    d.drag(at([0.0, 0.0]), at([4.0, 0.3]), egui::Modifiers::SHIFT);
    let a = d.arrows();
    assert_eq!(a[0].kind, ArrowKind::Equilibrium);
    assert!(close(a[0].head, [4.0, 0.3]), "{:?}", a[0]);
}

#[test]
fn clicking_with_the_arrow_tool_places_a_default_arrow() {
    let mut d = Driver::new(Tool::Arrow(ArrowKind::Reaction));
    d.click(at([1.0, 1.0]));
    let a = d.arrows();
    assert_eq!(a.len(), 1);
    assert!(close(a[0].tail, [1.0, 1.0]) && close(a[0].head, [1.0 + crate::molecule::page::DEFAULT_ARROW_LENGTH, 1.0]), "{:?}", a[0]);
}

#[test]
fn the_text_tool_types_a_text_and_enter_commits_it() {
    let mut d = Driver::new(Tool::Text);
    d.click(at([1.0, 2.0]));
    assert!(d.editor.text_edit.is_some(), "a click opens a text box");
    d.type_text("H_2O, 25 \u{b0}C");
    d.key(egui::Key::Enter, Default::default());
    assert!(d.editor.text_edit.is_none(), "Enter closes the box");
    let t = d.texts();
    assert_eq!(t.len(), 1);
    assert_eq!(t[0].text, "H_2O, 25 \u{b0}C");
    assert!((t[0].pos[0] - 1.0).abs() < 1e-3, "{:?}", t[0]);
    assert_eq!(d.editor.undo_stack.len(), 1, "one undo step");
    // Drawn, and so hit-testable, from the next frame on.
    // (Two frames: the closed text box still holds the pointer for one.)
    d.frame(vec![egui::Event::PointerMoved(at([1.1, 2.0]))]);
    d.frame(vec![egui::Event::PointerMoved(at([1.1, 2.0]))]);
    assert_eq!(d.editor.hovered_object, Some(t[0].id), "{:?} {:?}", d.editor.last_object_rects, at([1.1, 2.0]));
    assert!(d.editor.undo());
    assert!(d.texts().is_empty());
}

#[test]
fn shift_enter_starts_a_new_line_and_escape_cancels() {
    let mut d = Driver::new(Tool::Text);
    d.click(at([0.0, 0.0]));
    d.type_text("Pd(PPh_3)_4");
    d.key(egui::Key::Enter, egui::Modifiers::SHIFT);
    d.frame(vec![egui::Event::Text("THF".into())]);
    d.key(egui::Key::Enter, Default::default());
    assert_eq!(d.texts()[0].text, "Pd(PPh_3)_4\nTHF");

    d.click(at([0.0, 5.0]));
    d.type_text("discarded");
    d.key(egui::Key::Escape, Default::default());
    assert!(d.editor.text_edit.is_none());
    assert_eq!(d.texts().len(), 1, "Escape adds nothing");
}

#[test]
fn editing_a_text_to_nothing_removes_it() {
    let mut d = Driver::new(Tool::Text);
    let id = d.editor.molecule.add_text([0.0, 0.0], "old");
    d.frame(Vec::new()); // lay it out, so it can be clicked
    d.click(at([0.2, -0.05]));
    assert_eq!(d.editor.text_edit.as_ref().and_then(|e| e.id), Some(id), "a click on a text edits it");
    d.editor.text_edit.as_mut().unwrap().text.clear();
    d.key(egui::Key::Enter, Default::default());
    assert!(d.editor.molecule.objects.is_empty());
    assert!(d.editor.undo());
    assert_eq!(d.texts()[0].text, "old");
}

#[test]
fn select_click_and_delete_remove_an_arrow_undoably() {
    let mut d = Driver::new(Tool::Select);
    let id = d.editor.molecule.add_arrow([0.0, 0.0], [4.0, 0.0], ArrowKind::Reaction);
    d.click(at([2.0, 0.05]));
    assert!(d.editor.selected_objects.contains(&id), "a click selects the arrow");
    d.key(egui::Key::Delete, Default::default());
    assert!(d.editor.molecule.objects.is_empty());
    assert!(d.editor.undo());
    assert_eq!(d.arrows().len(), 1);
    assert!(d.editor.selected_objects.is_empty(), "undo clears the selection");
}

#[test]
fn dragging_an_arrow_moves_it_and_its_selected_company() {
    let mut d = Driver::new(Tool::Select);
    let arrow = d.editor.molecule.add_arrow([0.0, 0.0], [4.0, 0.0], ArrowKind::Reaction);
    let text = d.editor.molecule.add_text([0.0, -1.0], "cat.");
    let atom = d.editor.molecule.add_atom("C".into(), [-2.0, 2.0], 0);
    d.editor.selected_objects.extend([arrow, text]);
    d.editor.selected_atoms.insert(atom);
    d.drag(at([2.0, 0.0]), at([2.0, 1.0]), Default::default());
    assert!(close(d.arrows()[0].tail, [0.0, 1.0]), "{:?}", d.arrows());
    assert!(close(d.texts()[0].pos, [0.0, 0.0]), "{:?}", d.texts());
    assert!(close(d.editor.molecule.atom_by_id(atom).unwrap().pos, [-2.0, 3.0]));
    assert_eq!(d.editor.undo_stack.len(), 1, "one undo step for the move");

    // Unselected, it moves alone.
    d.editor.selected_objects.clear();
    d.editor.selected_atoms.clear();
    d.drag(at([2.0, 1.0]), at([3.0, 1.0]), Default::default());
    assert!(close(d.arrows()[0].tail, [1.0, 1.0]));
    assert!(close(d.texts()[0].pos, [0.0, 0.0]));
}

#[test]
fn the_ends_of_a_lone_selected_arrow_can_be_dragged() {
    let mut d = Driver::new(Tool::Select);
    let id = d.editor.molecule.add_arrow([0.0, 0.0], [4.0, 0.0], ArrowKind::Reaction);
    d.editor.selected_objects.insert(id);
    d.drag(at([4.0, 0.0]), at([4.0, 4.0]), Default::default());
    let a = &d.arrows()[0];
    assert!(close(a.tail, [0.0, 0.0]), "the tail stays: {a:?}");
    assert!(close(a.head, [4.0, 4.0]), "the head follows (45° is a snap angle): {a:?}");
}

#[test]
fn the_lasso_takes_arrows_lying_inside_it() {
    let mut d = Driver::new(Tool::Select);
    let inside = d.editor.molecule.add_arrow([-1.0, -1.0], [1.0, -1.0], ArrowKind::Reaction);
    d.editor.molecule.add_arrow([-1.0, 1.0], [5.0, 1.0], ArrowKind::Reaction);
    // A rubber band around (-2, -2)..(2, 2), drawn as a square path.
    let corners = [at([-2.0, -2.0]), at([2.0, -2.0]), at([2.0, 2.0]), at([-2.0, 2.0])];
    d.frame(vec![egui::Event::PointerMoved(corners[0])]);
    d.frame(vec![Driver::button(corners[0], true, Default::default())]);
    for &p in &corners[1..] {
        for k in 1..=4 {
            let prev = d.ctx.input(|i| i.pointer.latest_pos()).unwrap();
            d.frame(vec![egui::Event::PointerMoved(prev + (p - prev) * (k as f32 / 4.0))]);
        }
    }
    d.frame(vec![Driver::button(corners[3], false, Default::default())]);
    assert_eq!(d.editor.selected_objects, [inside].into_iter().collect());
}

#[test]
fn the_eraser_removes_a_text() {
    let mut d = Driver::new(Tool::Eraser);
    d.editor.molecule.add_text([0.0, 0.0], "gone");
    d.frame(Vec::new());
    d.click(at([0.2, -0.05]));
    assert!(d.editor.molecule.objects.is_empty());
    assert!(d.editor.undo());
}

#[test]
fn double_clicking_a_text_with_select_edits_it() {
    let mut d = Driver::new(Tool::Select);
    let id = d.editor.molecule.add_text([0.0, 0.0], "reflux");
    d.frame(Vec::new());
    d.double_click(at([0.2, -0.05]));
    assert_eq!(d.editor.text_edit.as_ref().and_then(|e| e.id), Some(id));
}
