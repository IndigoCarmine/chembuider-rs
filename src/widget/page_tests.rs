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

/// Ctrl+G switches Symbol typing on: "p" goes in as ChemDraw's Symbol `p` (π), then off again.
#[test]
fn ctrl_g_types_greek_in_the_symbol_font() {
    let mut d = Driver::new(Tool::Text);
    d.click(at([1.0, 2.0]));
    d.type_text("60 ");
    d.key(egui::Key::G, egui::Modifiers::COMMAND);
    assert!(d.editor.text_edit.as_ref().unwrap().symbol, "Symbol typing is on");
    d.type_text("m");
    d.key(egui::Key::G, egui::Modifiers::COMMAND);
    d.type_text("L");
    d.key(egui::Key::Enter, Default::default());
    assert_eq!(d.texts()[0].text, "60 {@Symbol:m}L");
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

/// Ethane → propane, drawn left of the screen's middle, for the reaction-table tests.
fn reaction(d: &mut Driver) {
    let m = &mut d.editor.molecule;
    let a = m.add_atom("C".into(), [-5.0, -3.0], 0);
    let b = m.add_atom("C".into(), [-3.7, -3.0], 0);
    m.add_bond(a, b, crate::molecule::BondOrder::Single);
    m.add_arrow([-2.5, -3.0], [0.5, -3.0], ArrowKind::Reaction);
    let c = m.add_atom("C".into(), [1.5, -3.0], 0);
    let e = m.add_atom("C".into(), [2.8, -3.0], 0);
    let f = m.add_atom("C".into(), [4.1, -3.0], 0);
    m.add_bond(c, e, crate::molecule::BondOrder::Single);
    m.add_bond(e, f, crate::molecule::BondOrder::Single);
}

fn table(d: &Driver) -> crate::molecule::ReactionTable {
    d.editor.molecule.objects.iter().find_map(|o| o.as_table().cloned()).expect("a table")
}

fn cell_center(d: &Driver, id: u32, cell: crate::reaction::edit::CellRef) -> egui::Pos2 {
    d.editor.last_table_cells[&id].iter().find(|(c, _)| *c == cell).expect("painted").1.center()
}

#[test]
fn a_reaction_table_is_inserted_edited_moved_and_deleted() {
    use crate::reaction::edit::CellRef;
    use crate::reaction::BlockKind;
    let mut d = Driver::new(Tool::Select);
    // Small labels, so the whole table fits the test screen.
    d.editor.config.style.label_size = 6.0;
    reaction(&mut d);
    let id = d.editor.insert_reaction_table().expect("there is a reaction");
    assert_eq!(d.editor.undo_stack.len(), 1);
    assert_eq!(d.editor.selected_objects, [id].into_iter().collect());
    d.frame(Vec::new());
    assert!(d.editor.last_object_rects.contains_key(&id), "painted");

    // Double-click the reactant's Sample Mass and type a value: the table recomputes.
    let sample = CellRef::new(BlockKind::Reactants, 0, 4);
    d.double_click(cell_center(&d, id, sample));
    assert_eq!(d.editor.table_edit.as_ref().map(|e| (e.id, e.cell)), Some((id, sample)));
    d.type_text("300 mg");
    d.key(egui::Key::Enter, Default::default());
    assert!(d.editor.table_edit.is_none(), "{:?}", d.editor.table_edit);
    let t = table(&d);
    assert!((t.reactants[0].sample_mass.unwrap() - 0.3).abs() < 1e-9, "{t:?}");
    let state = crate::molecule::reaction_table::TableState::of(&d.editor.molecule, &t);
    assert_eq!(state.blocks()[1].columns[0][4].plain(), "439.94mg", "expected mass of propane");
    assert_eq!(d.editor.undo_stack.len(), 2, "one undo step for the entry");

    // A value that does not read keeps the box open with the reason. (The harness does not
    // advance time, so a second double-click would count as more clicks of the first.)
    d.frame(Vec::new());
    let eq = CellRef::new(BlockKind::Reactants, 0, 3);
    assert!(!d.editor.edit_table_cell(id, eq), "opening the box changes nothing");
    assert!(d.editor.table_edit.is_some(), "the box opens");
    d.type_text("abc");
    assert_eq!(d.editor.table_edit.as_ref().map(|e| e.text.as_str()), Some("abc"));
    d.key(egui::Key::Enter, Default::default());
    assert!(d.editor.table_edit.as_ref().is_some_and(|e| e.error.is_some()), "{:?}", d.editor.table_edit);
    d.key(egui::Key::Escape, Default::default());
    assert!(d.editor.table_edit.is_none());

    // A computed cell has nothing to edit.
    assert!(!d.editor.edit_table_cell(id, CellRef::new(BlockKind::Reactants, 0, 9)));
    assert!(d.editor.table_edit.is_none());

    // Dragging the table moves it.
    let before = table(&d).pos;
    let from = cell_center(&d, id, CellRef::new(BlockKind::Reactants, 0, 9));
    d.drag(from, from + egui::vec2(50.0, 25.0), Default::default());
    let after = table(&d).pos;
    assert!(close(after, [before[0] + 1.0, before[1] + 0.5]), "{before:?} → {after:?}");

    // Delete removes it; undo brings it back.
    d.editor.selected_objects = [id].into_iter().collect();
    assert!(d.editor.delete_selection());
    assert!(d.editor.molecule.objects.iter().all(|o| o.as_table().is_none()));
    assert!(d.editor.undo());
    assert_eq!(table(&d).reactants[0].sample_mass, Some(0.3));
}

#[test]
fn the_tables_limiting_cell_toggles() {
    use crate::reaction::edit::CellRef;
    use crate::reaction::BlockKind;
    let mut d = Driver::new(Tool::Select);
    reaction(&mut d);
    let id = d.editor.insert_reaction_table().unwrap();
    assert!(table(&d).reactants[0].limiting, "a new table's first reactant is limiting");
    assert!(d.editor.edit_table_cell(id, CellRef::new(BlockKind::Reactants, 0, 2)));
    assert!(!table(&d).reactants[0].limiting);
    assert!(d.editor.undo());
    assert!(table(&d).reactants[0].limiting);
}

#[test]
fn a_table_without_structures_is_not_inserted() {
    let mut d = Driver::new(Tool::Select);
    d.editor.molecule.add_arrow([0.0, 0.0], [3.0, 0.0], ArrowKind::Reaction);
    assert_eq!(d.editor.insert_reaction_table(), None);
    assert!(d.editor.undo_stack.is_empty());
}

/// Polyethylene's unit drawn as C–C–C–C along x (zigzag), its middle two carbons selected.
fn chain(d: &mut Driver) -> Vec<u32> {
    let ids: Vec<u32> = (0..4)
        .map(|k| d.editor.molecule.add_atom("C".into(), [-2.0 + k as f32 * 1.3, if k % 2 == 0 { 0.0 } else { -0.75 }], 0))
        .collect();
    for w in ids.windows(2) {
        d.editor.molecule.add_bond(w[0], w[1], BondOrder::Single);
    }
    d.editor.selected_atoms = [ids[1], ids[2]].into_iter().collect();
    ids
}

#[test]
fn bracketing_the_selection_is_one_undo_step_and_selects_the_brackets() {
    let mut d = Driver::new(Tool::Select);
    let ids = chain(&mut d);
    let id = d.editor.bracket_selection(crate::molecule::BracketUsage::Sru).unwrap();
    let b = d.editor.molecule.object_by_id(id).unwrap().as_bracket().unwrap().clone();
    assert_eq!((b.atoms, b.label.as_str()), (vec![ids[1], ids[2]], "n"));
    assert!(d.editor.selected_objects.contains(&id));
    assert_eq!(d.editor.undo_stack.len(), 1);
    assert!(d.editor.undo());
    assert!(d.editor.molecule.objects.is_empty());
    d.editor.selected_atoms.clear();
    assert_eq!(d.editor.bracket_selection(crate::molecule::BracketUsage::Sru), None, "nothing selected");
}

#[test]
fn a_bracket_is_hit_on_its_mark_dragged_with_its_atoms_and_deleted_alone() {
    let mut d = Driver::new(Tool::Select);
    let ids = chain(&mut d);
    let id = d.editor.bracket_selection(crate::molecule::BracketUsage::Sru).unwrap();
    d.editor.selected_atoms.clear();
    d.editor.selected_objects.clear();
    let g = crate::molecule::bracket::geometry(&d.editor.molecule, d.editor.molecule.object_by_id(id).unwrap().as_bracket().unwrap()).unwrap();
    let mark = [g.sides[0].start[0], (g.sides[0].start[1] + g.sides[0].end[1]) / 2.0 + 0.3];
    d.frame(Vec::new());
    d.click(at(mark));
    assert!(d.editor.selected_objects.contains(&id), "a click on a mark selects the bracket");
    // Dragging the bracket moves the atoms it encloses (and so the bracket), not the others.
    let before: Vec<[f32; 2]> = ids.iter().map(|&a| d.editor.molecule.atom_by_id(a).unwrap().pos).collect();
    d.drag(at(mark), at([mark[0], mark[1] + 1.0]), Default::default());
    let after: Vec<[f32; 2]> = ids.iter().map(|&a| d.editor.molecule.atom_by_id(a).unwrap().pos).collect();
    assert!(close(before[0], after[0]) && close(before[3], after[3]), "{before:?} {after:?}");
    assert!((after[1][1] - before[1][1] - 1.0).abs() < 0.05 && (after[2][1] - before[2][1] - 1.0).abs() < 0.05);
    // Delete takes the brackets, not the atoms.
    d.editor.selected_objects = [id].into_iter().collect();
    d.key(egui::Key::Delete, Default::default());
    assert!(d.editor.molecule.objects.is_empty());
    assert_eq!(d.editor.molecule.atoms.len(), 4);
}

#[test]
fn deleting_enclosed_atoms_shrinks_then_drops_the_brackets() {
    let mut d = Driver::new(Tool::Select);
    let ids = chain(&mut d);
    let id = d.editor.bracket_selection(crate::molecule::BracketUsage::Sru).unwrap();
    d.editor.molecule.remove_atom(ids[1]);
    assert_eq!(d.editor.molecule.object_by_id(id).unwrap().as_bracket().unwrap().atoms, vec![ids[2]]);
    d.editor.molecule.remove_atom(ids[2]);
    assert!(d.editor.molecule.objects.is_empty(), "brackets around nothing go");
}

#[test]
fn the_lasso_takes_a_bracket_with_all_its_atoms() {
    let mut d = Driver::new(Tool::Select);
    chain(&mut d);
    let id = d.editor.bracket_selection(crate::molecule::BracketUsage::Sru).unwrap();
    d.editor.selected_objects.clear();
    d.editor.selected_atoms.clear();
    let corners = [at([-3.0, -2.0]), at([3.0, -2.0]), at([3.0, 2.0]), at([-3.0, 2.0])];
    d.frame(vec![egui::Event::PointerMoved(corners[0])]);
    d.frame(vec![Driver::button(corners[0], true, Default::default())]);
    let mut prev = corners[0];
    for &p in &corners[1..] {
        for k in 1..=4 {
            d.frame(vec![egui::Event::PointerMoved(prev + (p - prev) * (k as f32 / 4.0))]);
        }
        prev = p;
    }
    d.frame(vec![Driver::button(corners[3], false, Default::default())]);
    assert!(d.editor.selected_objects.contains(&id), "{:?}", d.editor.selected_objects);
}

#[test]
fn a_brackets_label_is_edited_in_the_text_box() {
    let mut d = Driver::new(Tool::Select);
    chain(&mut d);
    let id = d.editor.bracket_selection(crate::molecule::BracketUsage::Sru).unwrap();
    assert!(d.editor.edit_text(id));
    assert_eq!(d.editor.text_edit.as_ref().map(|e| e.text.as_str()), Some("n"));
    d.editor.text_edit.as_mut().unwrap().text = "x".into();
    assert!(d.editor.commit_text_edit());
    assert_eq!(d.editor.molecule.object_by_id(id).unwrap().as_bracket().unwrap().label, "x");
    // Cleared, it goes back to the usage's own.
    assert!(d.editor.edit_text(id));
    d.editor.text_edit.as_mut().unwrap().text.clear();
    assert!(d.editor.commit_text_edit());
    assert_eq!(d.editor.molecule.object_by_id(id).unwrap().as_bracket().unwrap().label, "n");
    assert!(d.editor.set_bracket_usage(id, crate::molecule::BracketUsage::MultipleGroup));
    assert_eq!(d.editor.molecule.object_by_id(id).unwrap().as_bracket().unwrap().label, "2");
    assert!(d.editor.set_bracket_shape(id, crate::molecule::BracketShape::Round));
    assert!(!d.editor.set_bracket_shape(id, crate::molecule::BracketShape::Round));
}
