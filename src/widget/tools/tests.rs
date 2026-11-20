//! The default behaviour and what each tool changes of it: the same keys in every tool, one undo
//! step per change and none for no change, drags that end cleanly when the tool changes, and the
//! keyboard and pointer reaching the canvas when other widgets have focus.

use super::Gesture;
use crate::molecule::{ArrowKind, BondOrder};
use crate::testkit::{chord, run_frame, run_frame_with_modifiers, run_ui, CANVAS};
use crate::widget::{ChemStructEditor, Tool};

const ALL_TOOLS: [Tool; 5] = [
    Tool::Select,
    Tool::Bond,
    Tool::Eraser,
    Tool::Arrow(ArrowKind::Reaction),
    Tool::Text,
];

fn mid() -> egui::Pos2 {
    CANVAS.center()
}

fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
}

fn frame(ctx: &egui::Context, editor: &mut ChemStructEditor, events: Vec<egui::Event>) {
    run_frame(ctx, CANVAS, editor, events);
}

/// Bring the pointer to `at`. Two frames: egui hit-tests against the widgets of the frame
/// before, and a new context has none.
fn point(ctx: &egui::Context, editor: &mut ChemStructEditor, at: egui::Pos2) {
    frame(ctx, editor, vec![egui::Event::PointerMoved(at)]);
    frame(ctx, editor, vec![egui::Event::PointerMoved(at)]);
}

/// Press `spec` (see [`chord`]) with the pointer resting where it is.
fn key(ctx: &egui::Context, editor: &mut ChemStructEditor, spec: &str) {
    let (events, modifiers) = chord(spec);
    run_frame_with_modifiers(ctx, CANVAS, editor, events, modifiers);
}

/// An editor with `tool` and a C–C bond: the first carbon at the canvas centre.
fn two_carbons(tool: Tool) -> (ChemStructEditor, u32, u32, u32) {
    let mut editor = ChemStructEditor { tool, ..Default::default() };
    let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
    let b = editor.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
    let bond = editor.molecule.add_bond(a, b, BondOrder::Single);
    (editor, a, b, bond)
}

/// The midpoint of the bond between the two carbons on screen.
fn bond_mid(editor: &ChemStructEditor) -> egui::Pos2 {
    editor.mol_to_screen([0.75, 0.0], mid())
}

/// The drawing keys act the same whatever the tool: an element on the atom under the pointer,
/// one undo step, which undo takes back.
#[test]
fn an_atom_key_does_the_same_in_every_tool_and_is_one_undo_step() {
    for tool in ALL_TOOLS {
        let ctx = egui::Context::default();
        let (mut editor, a, _, _) = two_carbons(tool.clone());
        point(&ctx, &mut editor, mid());
        key(&ctx, &mut editor, "o");
        assert_eq!(editor.molecule.atom_by_id(a).unwrap().element, "O", "{tool}: `o` on the hovered atom");
        assert_eq!(editor.undo_stack.len(), 1, "{tool}: one undo step");
        assert!(editor.undo());
        assert_eq!(editor.molecule.atom_by_id(a).unwrap().element, "C", "{tool}: undo puts it back");
    }
}

/// A bond key on the hovered bond, in every tool.
#[test]
fn a_bond_key_does_the_same_in_every_tool() {
    for tool in ALL_TOOLS {
        let ctx = egui::Context::default();
        let (mut editor, _, _, bond) = two_carbons(tool.clone());
        let on_bond = bond_mid(&editor);
        point(&ctx, &mut editor, on_bond);
        assert_eq!(editor.hovered_bond, Some(bond), "{tool}");
        key(&ctx, &mut editor, "2");
        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().order, BondOrder::Double, "{tool}: `2` on the bond");
        assert_eq!(editor.undo_stack.len(), 1, "{tool}");
    }
}

/// The backtick opens the label box, and `x` expands a label, in every tool.
#[test]
fn label_keys_work_in_every_tool() {
    for tool in ALL_TOOLS {
        let ctx = egui::Context::default();
        let (mut editor, a, _, _) = two_carbons(tool.clone());
        point(&ctx, &mut editor, mid());
        key(&ctx, &mut editor, "`");
        assert_eq!(editor.label_edit.as_ref().map(|(id, _)| *id), Some(a), "{tool}: the label box opens");
        editor.label_edit = None;

        editor.molecule.atom_by_id_mut(a).unwrap().element = "Ph".to_string();
        frame(&ctx, &mut editor, Vec::new());
        let before = editor.molecule.atoms.len();
        key(&ctx, &mut editor, "x");
        assert!(editor.molecule.atoms.len() > before, "{tool}: `x` expands Ph");
    }
}

/// An element key on empty canvas starts a new atom at the pointer, in every tool.
#[test]
fn an_element_key_places_an_atom_at_the_pointer_in_every_tool() {
    for tool in ALL_TOOLS {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor { tool: tool.clone(), ..Default::default() };
        frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid() + egui::vec2(50.0, 0.0))]);
        key(&ctx, &mut editor, "O");
        assert_eq!(editor.molecule.atoms.len(), 1, "{tool}");
        assert_eq!(editor.molecule.atoms[0].element, "O", "{tool}");
    }
}

/// Delete takes the selection away whatever the tool, the Bond tool's lasso included.
#[test]
fn delete_removes_the_selection_in_every_tool() {
    for tool in ALL_TOOLS {
        let ctx = egui::Context::default();
        let (mut editor, a, b, _) = two_carbons(tool.clone());
        editor.selected_atoms.extend([a, b]);
        frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(CANVAS.min + egui::vec2(10.0, 10.0))]);
        key(&ctx, &mut editor, "Delete");
        assert!(editor.molecule.atoms.is_empty(), "{tool}: the selection is deleted");
        assert_eq!(editor.undo_stack.len(), 1, "{tool}: in one undo step");
    }
}

/// Escape clears the selection.
#[test]
fn escape_clears_the_selection() {
    let ctx = egui::Context::default();
    let (mut editor, a, _, _) = two_carbons(Tool::Bond);
    editor.selected_atoms.insert(a);
    key(&ctx, &mut editor, "Escape");
    assert!(editor.selected_atoms.is_empty());
}

/// A click that changes nothing leaves no undo step behind (one used to be pushed before every
/// Bond-tool click, so undo then seemed to do nothing).
#[test]
fn a_click_that_changes_nothing_leaves_no_undo_step() {
    for tool in [Tool::Select, Tool::Eraser] {
        let ctx = egui::Context::default();
        let (mut editor, _, _, _) = two_carbons(tool.clone());
        let empty = mid() + egui::vec2(0.0, 150.0);
        frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(empty)]);
        frame(&ctx, &mut editor, vec![button(empty, true)]);
        frame(&ctx, &mut editor, vec![button(empty, false)]);
        assert!(editor.undo_stack.is_empty(), "{tool}");
    }
    // The Bond tool on a page text: the default, a selection, which is no change to undo.
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor { tool: Tool::Text, ..Default::default() };
    editor.molecule.add_text([0.0, 0.0], "rt".to_string());
    frame(&ctx, &mut editor, Vec::new());
    editor.tool = Tool::Bond;
    let on_text = mid() + egui::vec2(8.0, 0.0);
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(on_text)]);
    assert!(editor.hovered_object.is_some(), "the pointer is on the text");
    frame(&ctx, &mut editor, vec![button(on_text, true)]);
    frame(&ctx, &mut editor, vec![button(on_text, false)]);
    assert!(editor.molecule.atoms.is_empty(), "no atom dropped on the text");
    assert!(editor.undo_stack.is_empty());
}

/// Moving an atom by dragging it (the default drag) is one undo step, in the tools that leave
/// the drag alone.
#[test]
fn dragging_an_atom_moves_it_in_one_undo_step() {
    for tool in [Tool::Select, Tool::Eraser, Tool::Text] {
        let ctx = egui::Context::default();
        let (mut editor, a, _, _) = two_carbons(tool.clone());
        frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid())]);
        frame(&ctx, &mut editor, vec![button(mid(), true)]);
        for k in 1..=5 {
            frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid() + egui::vec2(0.0, 12.0 * k as f32))]);
        }
        frame(&ctx, &mut editor, vec![button(mid() + egui::vec2(0.0, 60.0), false)]);
        assert!(editor.molecule.atom_by_id(a).unwrap().pos[1] > 0.5, "{tool}: the atom moved");
        assert_eq!(editor.undo_stack.len(), 1, "{tool}: one step");
        assert!(editor.gesture().is_none(), "{tool}");
    }
}

/// Switching tools in the middle of a drag gives the drag up: what it moved goes back, and
/// nothing of it is left to steer the next tool.
#[test]
fn switching_tools_mid_drag_gives_the_drag_up() {
    for direct in [false, true] {
        let ctx = egui::Context::default();
        let (mut editor, a, _, _) = two_carbons(Tool::Select);
        frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid())]);
        frame(&ctx, &mut editor, vec![button(mid(), true)]);
        for k in 1..=4 {
            frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid() + egui::vec2(15.0 * k as f32, 0.0))]);
        }
        assert!(matches!(editor.gesture(), Some(Gesture::MoveAtom { .. })));
        if direct {
            // A host that writes the field: the next frame gives the drag up.
            editor.tool = Tool::Bond;
            frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid() + egui::vec2(70.0, 0.0))]);
        } else {
            editor.set_tool(Tool::Bond);
        }
        assert!(editor.gesture().is_none(), "direct={direct}: the drag is given up");
        assert_eq!(editor.molecule.atom_by_id(a).unwrap().pos, [0.0, 0.0], "direct={direct}: the atom is back");
        frame(&ctx, &mut editor, vec![button(mid() + egui::vec2(70.0, 0.0), false)]);
        assert!(editor.undo_stack.is_empty(), "direct={direct}: nothing to undo");
        assert_eq!(editor.molecule.bonds.len(), 1, "direct={direct}: the release draws nothing");
    }
}

/// Escape during a drag gives it up the same way.
#[test]
fn escape_mid_drag_puts_the_atom_back() {
    let ctx = egui::Context::default();
    let (mut editor, a, _, _) = two_carbons(Tool::Select);
    frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid())]);
    frame(&ctx, &mut editor, vec![button(mid(), true)]);
    for k in 1..=4 {
        frame(&ctx, &mut editor, vec![egui::Event::PointerMoved(mid() + egui::vec2(15.0 * k as f32, 0.0))]);
    }
    key(&ctx, &mut editor, "Escape");
    assert!(editor.gesture().is_none());
    assert_eq!(editor.molecule.atom_by_id(a).unwrap().pos, [0.0, 0.0]);
    assert!(editor.undo_stack.is_empty());
}

/// Tab used to hand egui's keyboard focus to the canvas (or a button), after which the canvas
/// took it for a text field having the keyboard and ignored every key and click.
#[test]
fn tab_does_not_lock_the_canvas() {
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor::default();
    let frame = |editor: &mut ChemStructEditor, events: Vec<egui::Event>| {
        let raw = egui::RawInput { screen_rect: Some(CANVAS), events, focused: true, ..Default::default() };
        let _ = run_ui(&ctx, raw, |ui| {
            let _ = ui.button("a host button");
            editor.ui(ui);
        });
    };
    let press = |key| {
        vec![
            egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() },
            egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: Default::default() },
        ]
    };
    frame(&mut editor, Vec::new());
    frame(&mut editor, press(egui::Key::Tab));
    frame(&mut editor, press(egui::Key::Tab));
    frame(&mut editor, press(egui::Key::C));
    assert_eq!(editor.molecule.atoms.len(), 1, "a key after Tab still draws");

    let at = CANVAS.center() + egui::vec2(0.0, 120.0);
    frame(&mut editor, vec![egui::Event::PointerMoved(at)]);
    frame(&mut editor, vec![button(at, true)]);
    frame(&mut editor, vec![button(at, false)]);
    assert_eq!(editor.molecule.atoms.len(), 2, "a click after Tab still draws");
}

/// A host's text field keeps the keys, but a click on the canvas still draws, and takes the
/// keyboard from the field for the keys after it.
#[test]
fn a_host_text_field_keeps_the_keys_but_not_the_pointer() {
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor::default();
    let text = std::cell::RefCell::new(String::new());
    let field = egui::Id::unique("host_field");
    let frame = |editor: &mut ChemStructEditor, events: Vec<egui::Event>, focus: bool| {
        let raw = egui::RawInput { screen_rect: Some(CANVAS), events, focused: true, ..Default::default() };
        let _ = run_ui(&ctx, raw, |ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut *text.borrow_mut()).id(field));
            if focus {
                response.request_focus();
            }
            editor.ui(ui);
        });
    };
    frame(&mut editor, Vec::new(), true);
    let typed = vec![
        egui::Event::Key { key: egui::Key::C, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() },
        egui::Event::Text("c".to_string()),
    ];
    frame(&mut editor, typed, false);
    assert!(editor.molecule.atoms.is_empty(), "typing in the field draws nothing");
    assert_eq!(*text.borrow(), "c");

    let at = CANVAS.center() + egui::vec2(0.0, 120.0);
    frame(&mut editor, vec![egui::Event::PointerMoved(at)], false);
    frame(&mut editor, vec![button(at, true)], false);
    frame(&mut editor, vec![button(at, false)], false);
    assert_eq!(editor.molecule.atoms.len(), 1, "the click draws");
    assert!(!ctx.text_edit_focused(), "the click took the keyboard from the field");
}

/// A ring with an N in it, drawn as a user would (click, `6`, `n` on the first atom), with the
/// pointer resting on the N. `n` leaves the N wearing a group H, on the outside of the ring.
fn ring_with_nh() -> (egui::Context, ChemStructEditor, u32) {
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor { tool: Tool::Bond, ..Default::default() };
    point(&ctx, &mut editor, mid());
    frame(&ctx, &mut editor, vec![button(mid(), true)]);
    frame(&ctx, &mut editor, vec![button(mid(), false)]);
    let n = editor.molecule.atoms[0].id;
    key(&ctx, &mut editor, "6");
    key(&ctx, &mut editor, "n");
    assert_eq!(editor.molecule.atom_by_id(n).unwrap().element, "N");
    (ctx, editor, n)
}

/// The direction of the bond from `from` to its neighbour that is not in `ring`, in degrees.
fn substituent_angle(editor: &ChemStructEditor, from: u32, ring: &[u32]) -> f32 {
    let at = editor.molecule.atom_by_id(from).unwrap().pos;
    let out = editor.molecule.neighbor_atom_ids(from).into_iter().find(|id| !ring.contains(id)).expect("a substituent");
    let to = editor.molecule.atom_by_id(out).unwrap().pos;
    (to[1] - at[1]).atan2(to[0] - at[0]).to_degrees()
}

/// What grows out of a ring N–H takes the H's place, straight out of the ring, by a key or by a
/// click. The H used to be counted as a bond while the direction was chosen and only then
/// removed, so the new bond came out squeezed between it and the ring.
#[test]
fn a_bond_out_of_a_ring_nh_takes_the_place_of_the_h() {
    for how in ["key 1", "click"] {
        let (ctx, mut editor, n) = ring_with_nh();
        let h = editor.molecule.neighbor_atom_ids(n).into_iter().find(|&id| editor.molecule.atom_by_id(id).unwrap().element == "H").expect("the group H");
        let ring: Vec<u32> = editor.molecule.atoms.iter().map(|a| a.id).filter(|&id| id != h).collect();
        let outward = substituent_angle(&editor, n, &ring);
        if how == "click" {
            frame(&ctx, &mut editor, vec![button(mid(), true)]);
            frame(&ctx, &mut editor, vec![button(mid(), false)]);
        } else {
            key(&ctx, &mut editor, "1");
        }
        assert!(editor.molecule.atom_by_id(h).is_none(), "{how}: the H gave way");
        let angle = substituent_angle(&editor, n, &ring);
        let off = ((angle - outward + 540.0) % 360.0 - 180.0).abs();
        assert!(off < 1.0, "{how}: the new bond at {angle:.0}°, the H was at {outward:.0}°");
        assert_eq!(editor.molecule.neighbor_atom_ids(n).len(), 3, "{how}: N keeps three bonds");
    }
}
