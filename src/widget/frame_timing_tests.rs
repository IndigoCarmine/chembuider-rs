//! Frame-split invariance: the same input must build the same structure however it is batched.
//!
//! A real backend hands the widget whatever arrived since the last frame. A fast typist, a
//! slow frame (a big drawing, a running cleanup) or simply key rollover puts two or three
//! keystrokes in one frame; a slow typist gets one keystroke per frame with idle frames in
//! between. The editor must not care: [`assert_split_invariant`] replays one script of user
//! actions under several batchings and requires every one of them to build the structure the
//! slow, one-action-per-frame replay builds.
//!
//! Unlike [`crate::testkit::chord`], the actions here carry what egui-winit really sends for
//! a keystroke: the `ModifiersChanged` events around a chord, and the `Text` event a
//! printable key produces, so the label editor sees typing exactly as it would in the app.

use crate::molecule::canonical::canonical_key;
use crate::testkit::{chord, run_ui, CANVAS};
use crate::widget::ChemStructEditor;

/// One user action, as the events egui-winit emits for it, in order.
#[derive(Clone, Debug)]
struct Action {
    name: String,
    events: Vec<egui::Event>,
}

/// A keystroke: the modifier presses, the key press, the text it types (if any), the release,
/// and the modifier releases.
fn key(spec: &str) -> Action {
    let (chord_events, modifiers) = chord(spec);
    let mut events = Vec::new();
    if modifiers != egui::Modifiers::NONE {
        events.push(egui::Event::ModifiersChanged(modifiers));
    }
    events.push(chord_events[0].clone());
    // A printable key also types its character, unless Ctrl/Alt turn it into a command.
    let text = match spec {
        "Space" => Some(" ".to_string()),
        _ if spec.chars().count() == 1 => Some(spec.to_string()),
        _ => None,
    };
    if let Some(text) = text
        && !modifiers.ctrl
        && !modifiers.alt
    {
        events.push(egui::Event::Text(text));
    }
    events.push(chord_events[1].clone());
    if modifiers != egui::Modifiers::NONE {
        events.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
    }
    Action { name: spec.to_string(), events }
}

/// Type `text` one keystroke per character.
fn typed(text: &str) -> Vec<Action> {
    text.chars().map(|c| key(&c.to_string())).collect()
}

/// Move the pointer to `pos` (screen coordinates).
fn move_to(pos: egui::Pos2) -> Action {
    Action { name: format!("move{pos:?}"), events: vec![egui::Event::PointerMoved(pos)] }
}

/// A primary-button press or release at `pos`.
fn button(pos: egui::Pos2, pressed: bool) -> Action {
    Action {
        name: format!("{}{pos:?}", if pressed { "press" } else { "release" }),
        events: vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }],
    }
}

/// The pointer leaves the window.
fn pointer_gone() -> Action {
    Action { name: "gone".into(), events: vec![egui::Event::PointerGone] }
}

fn frame(ctx: &egui::Context, editor: &mut ChemStructEditor, events: Vec<egui::Event>) {
    let raw = egui::RawInput {
        screen_rect: Some(CANVAS),
        events,
        focused: true,
        ..Default::default()
    };
    let _ = run_ui(ctx, raw, |ui| {
        editor.ui(ui);
    });
}

/// How a script's actions are batched into frames.
#[derive(Clone, Debug)]
enum Split {
    /// One action per frame, with `idle` empty frames after each: a slow, deliberate user.
    Each { idle: usize },
    /// Every action in a single frame.
    All,
    /// Actions `i` and `i + 1` share a frame; everything else is slow.
    Pair(usize),
    /// Actions `i`, `i + 1` and `i + 2` share a frame; everything else is slow.
    Triple(usize),
}

impl Split {
    fn frames(&self, actions: &[Action]) -> Vec<Vec<egui::Event>> {
        let slow = |a: &Action| vec![a.events.clone(), Vec::new(), Vec::new()];
        let merged = |group: &[Action]| group.iter().flat_map(|a| a.events.clone()).collect();
        let grouped = |start: usize, len: usize| -> Vec<Vec<egui::Event>> {
            let mut frames = Vec::new();
            let mut i = 0;
            while i < actions.len() {
                if i == start {
                    let end = (start + len).min(actions.len());
                    frames.push(merged(&actions[start..end]));
                    frames.extend([Vec::new(), Vec::new()]);
                    i = end;
                } else {
                    frames.extend(slow(&actions[i]));
                    i += 1;
                }
            }
            frames
        };
        match *self {
            Split::Each { idle } => actions
                .iter()
                .flat_map(|a| std::iter::once(a.events.clone()).chain((0..idle).map(|_| Vec::new())))
                .chain([Vec::new(), Vec::new()])
                .collect(),
            Split::All => vec![merged(actions), Vec::new(), Vec::new()],
            Split::Pair(i) => grouped(i, 2),
            Split::Triple(i) => grouped(i, 3),
        }
    }
}

/// Run `setup`, then `actions` batched as `split`, on a fresh editor.
fn replay(
    setup: &dyn Fn(&egui::Context, &mut ChemStructEditor),
    actions: &[Action],
    split: &Split,
) -> ChemStructEditor {
    let ctx = egui::Context::default();
    let mut editor = ChemStructEditor::default();
    setup(&ctx, &mut editor);
    for events in split.frames(actions) {
        frame(&ctx, &mut editor, events);
    }
    // The frames egui runs on its own after a burst (the editor asks for them while it still
    // has keystrokes to work through).
    for _ in 0..8 {
        frame(&ctx, &mut editor, Vec::new());
    }
    editor
}

/// Every batching worth trying for `n` actions.
fn splits(n: usize) -> Vec<Split> {
    let mut out = vec![Split::Each { idle: 0 }, Split::All];
    out.extend((0..n.saturating_sub(1)).map(Split::Pair));
    out.extend((0..n.saturating_sub(2)).map(Split::Triple));
    out
}

/// Replay `actions` slowly, check the result with `check`, then require every other batching
/// to build the same structure. Returns the slow replay's editor.
///
/// All divergences are collected before failing, so one run shows the whole pattern.
fn assert_split_invariant(
    setup: &dyn Fn(&egui::Context, &mut ChemStructEditor),
    actions: &[Action],
    check: &dyn Fn(&ChemStructEditor) -> Result<(), String>,
) -> ChemStructEditor {
    assert_invariant_over(setup, actions, check, splits(actions.len()))
}

/// As [`assert_split_invariant`], for a script that also moves or clicks the pointer, short of
/// putting the *whole* script in one frame.
///
/// That one is out of reach, and not only for the widget: egui works out the pointer once a
/// frame, and the keystrokes the widget hands on to the next frame (`carried_keys`) cannot take
/// the pointer as it was with them. Two or three actions to a frame — what a quick user or a
/// slow frame really produces — must still all agree.
fn assert_pointer_split_invariant(
    setup: &dyn Fn(&egui::Context, &mut ChemStructEditor),
    actions: &[Action],
    check: &dyn Fn(&ChemStructEditor) -> Result<(), String>,
) -> ChemStructEditor {
    let splits = splits(actions.len()).into_iter().filter(|s| !matches!(s, Split::All)).collect();
    assert_invariant_over(setup, actions, check, splits)
}

fn assert_invariant_over(
    setup: &dyn Fn(&egui::Context, &mut ChemStructEditor),
    actions: &[Action],
    check: &dyn Fn(&ChemStructEditor) -> Result<(), String>,
    splits: Vec<Split>,
) -> ChemStructEditor {
    let names: Vec<&str> = actions.iter().map(|a| a.name.as_str()).collect();
    let reference = replay(setup, actions, &Split::Each { idle: 2 });
    if let Err(e) = check(&reference) {
        panic!("{names:?} replayed slowly did not build the expected structure: {e}");
    }
    let want = canonical_key(&reference.molecule);

    let mut failures = Vec::new();
    for split in splits {
        let got = replay(setup, actions, &split);
        let what = match &split {
            Split::Pair(i) => format!("{split:?} = {:?}", &names[*i..*i + 2]),
            Split::Triple(i) => format!("{split:?} = {:?}", &names[*i..*i + 3]),
            _ => format!("{split:?}"),
        };
        if canonical_key(&got.molecule) != want {
            failures.push(format!(
                "{what}: {} atoms {:?}, want {} atoms {:?}",
                got.molecule.atoms.len(),
                elements(&got),
                reference.molecule.atoms.len(),
                elements(&reference),
            ));
        } else if let Err(e) = check(&got) {
            failures.push(format!("{what}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{names:?} depends on frame timing:\n  {}", failures.join("\n  "));
    reference
}

fn elements(editor: &ChemStructEditor) -> Vec<String> {
    let mut v: Vec<String> = editor.molecule.atoms.iter().map(|a| a.element.clone()).collect();
    v.sort();
    v
}

fn no_setup(_: &egui::Context, _: &mut ChemStructEditor) {}

/// Build the script `keys` (whitespace-separated chord specs) for a keyboard-only replay.
fn keys(seq: &str) -> Vec<Action> {
    seq.split_whitespace().map(key).collect()
}

fn atoms_are(n: usize) -> impl Fn(&ChemStructEditor) -> Result<(), String> {
    move |e: &ChemStructEditor| {
        let got = e.molecule.atoms.len();
        if got == n { Ok(()) } else { Err(format!("{got} atoms, want {n}: {:?}", elements(e))) }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fragment keys from the keyboard hotspot
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn fragment_keys_on_the_hotspot_ignore_frame_batching() {
    for (seq, n) in [
        ("c 1 a", 7),        // benzene off the chain end
        ("c 1 e", 3),        // ethyl
        ("c 1 o", 3),        // OH retargets the end carbon
        ("c 1 E", 5),        // CO2Me
        ("c 1 Q", 8),        // Boc
        ("c 1 y", 8),        // Boc, the other key
        ("c 1 6", 7),        // cyclohexane
        ("c 1 a e", 8),      // two fragments in a row
        ("c 1 6 6", 11),     // ring repeat
    ] {
        assert_split_invariant(&no_setup, &keys(seq), &atoms_are(n));
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Arrows, then a fragment key
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn an_arrow_then_a_fragment_key_ignores_frame_batching() {
    // `c 1 1` leaves the hotspot on the third carbon; the arrow walks it back to the middle
    // one, where `a` draws a ring through the middle and end carbons (3 + 4 atoms). Pressed
    // on the end carbon instead, `a` would hang a phenyl off it (3 + 5).
    assert_split_invariant(&no_setup, &keys("c 1 1 ArrowLeft a"), &atoms_are(7));
    // There and back again: two arrows in one frame must both move the hotspot.
    assert_split_invariant(&no_setup, &keys("c 1 1 ArrowLeft ArrowRight a"), &atoms_are(8));
    assert_split_invariant(&no_setup, &keys("c 1 1 ArrowLeft ArrowRight e"), &atoms_are(4));
}

#[test]
fn a_shift_arrow_then_a_fragment_key_ignores_frame_batching() {
    // Shift+arrow targets the bond, so `a` fuses benzene onto it: 2 + 4 atoms.
    assert_split_invariant(&no_setup, &keys("c 1 Shift+ArrowLeft a"), &atoms_are(6));
}

#[test]
fn an_arrow_then_a_shifted_fragment_key_ignores_frame_batching() {
    // `E` is typed with Shift held. If Shift goes down in the arrow's frame, the arrow must
    // still be a plain arrow.
    assert_split_invariant(&no_setup, &keys("c 1 1 ArrowLeft E"), &atoms_are(6));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Undo / redo straight after an insertion
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn undo_and_redo_after_a_fragment_ignore_frame_batching() {
    assert_split_invariant(&no_setup, &keys("c 1 a Ctrl+Z"), &atoms_are(2));
    assert_split_invariant(&no_setup, &keys("c 1 a Ctrl+Z Ctrl+Y"), &atoms_are(7));
    assert_split_invariant(&no_setup, &keys("c 1 E Ctrl+Z e"), &atoms_are(3));
    // Undo twice, then redo once.
    assert_split_invariant(&no_setup, &keys("c 1 a e Ctrl+Z Ctrl+Z Ctrl+Y"), &atoms_are(7));
}

// ═══════════════════════════════════════════════════════════════════════════════
// The label editor
// ═══════════════════════════════════════════════════════════════════════════════

fn label_script(label: &str, then: &str) -> Vec<Action> {
    let mut a = keys("c 1 `");
    a.extend(typed(label));
    a.extend(keys(then));
    a
}

#[test]
fn a_typed_label_ignores_frame_batching() {
    let single = |want: &'static str| {
        move |e: &ChemStructEditor| {
            if e.label_edit.is_some() {
                return Err("the label editor is still open".into());
            }
            if e.molecule.atoms.iter().any(|a| a.element == want) {
                Ok(())
            } else {
                Err(format!("no {want} label: {:?}", elements(e)))
            }
        }
    };
    assert_split_invariant(&no_setup, &label_script("Si", "Enter"), &single("Si"));
    assert_split_invariant(&no_setup, &label_script("Ph", "Enter"), &single("Ph"));
}

#[test]
fn a_typed_fragment_label_expands_whatever_the_batching() {
    for (label, n) in [("Ph", 7), ("Boc", 8), ("CO2Me", 5), ("C2H5", 3), ("Et", 3)] {
        assert_split_invariant(&no_setup, &label_script(label, "Enter x"), &atoms_are(n));
    }
}

#[test]
fn building_goes_on_after_a_label_whatever_the_batching() {
    // A second label straight after the first.
    let mut script = label_script("Ph", "Enter x");
    script.extend(keys("ArrowLeft `"));
    script.extend(typed("OMe"));
    script.extend(keys("Enter x"));
    assert_split_invariant(&no_setup, &script, &atoms_are(8));
}

#[test]
fn relabelling_and_cancelling_ignore_frame_batching() {
    let labelled = |want: &'static str| {
        move |e: &ChemStructEditor| {
            let got = elements(e);
            if e.label_edit.is_none() && got.iter().any(|el| el == want) {
                Ok(())
            } else {
                Err(format!("want a closed editor and a {want}: {got:?}, open={}", e.label_edit.is_some()))
            }
        }
    };
    // The Enter closing one label and the backtick opening the next, side by side.
    let mut script = label_script("Si", "Enter `");
    script.extend(typed("Cl"));
    script.extend(keys("Enter"));
    assert_split_invariant(&no_setup, &script, &labelled("Cl"));

    // Escape throws the typing away, whenever it comes.
    assert_split_invariant(&no_setup, &label_script("Ph", "Escape"), &labelled("C"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// The pointer
// ═══════════════════════════════════════════════════════════════════════════════

/// Setup: `c 1 1`, three carbons, then Space to drop the hotspot, so only the pointer aims.
fn three_carbons(ctx: &egui::Context, editor: &mut ChemStructEditor) {
    for spec in ["c", "1", "1", "Space"] {
        frame(ctx, editor, key(spec).events);
    }
    frame(ctx, editor, Vec::new());
}

/// Where atom `index` of [`three_carbons`] is on screen.
fn atom_screen(index: usize) -> egui::Pos2 {
    let ctx = egui::Context::default();
    let mut e = ChemStructEditor::default();
    three_carbons(&ctx, &mut e);
    e.mol_to_screen(e.molecule.atoms[index].pos, CANVAS.center())
}

#[test]
fn hovering_atoms_and_pressing_fragment_keys_ignores_frame_batching() {
    let (a0, a2) = (atom_screen(0), atom_screen(2));
    // Sanity: the computed position really is over the atom.
    let ctx = egui::Context::default();
    let mut e = ChemStructEditor::default();
    three_carbons(&ctx, &mut e);
    frame(&ctx, &mut e, vec![egui::Event::PointerMoved(a0)]);
    assert_eq!(e.hovered_atom, Some(e.molecule.atoms[0].id), "harness: a0 must hover atom 0");

    let script = vec![move_to(a0), key("a"), move_to(a2), key("o"), pointer_gone()];
    assert_pointer_split_invariant(&three_carbons, &script, &atoms_are(3 + 5 + 1));

    let script = vec![move_to(a0), key("e"), move_to(a2), key("E"), pointer_gone()];
    assert_pointer_split_invariant(&three_carbons, &script, &atoms_are(3 + 1 + 3));
}

#[test]
fn a_hovered_label_edit_ignores_frame_batching() {
    let a2 = atom_screen(2);
    let mut script = vec![move_to(a2), key("`")];
    script.extend(typed("Boc"));
    script.extend(keys("Enter x"));
    script.push(pointer_gone());
    assert_pointer_split_invariant(&three_carbons, &script, &atoms_are(3 + 6));
}

/// Clicking away from the label editor commits the label, as Enter does — and that is all the
/// click does: it does not also draw on the canvas.
#[test]
fn clicking_away_commits_the_label_and_draws_nothing() {
    let away = egui::pos2(150.0, 150.0);
    let mut script = label_script("Ph", "");
    script.extend([move_to(away), button(away, true), button(away, false)]);
    let committed = |e: &ChemStructEditor| {
        if e.label_edit.is_some() {
            return Err("the click did not close the label editor".to_string());
        }
        if !e.molecule.atoms.iter().any(|a| a.element == "Ph") {
            return Err(format!("the label was not committed: {:?}", elements(e)));
        }
        atoms_are(2)(e)
    };
    assert_pointer_split_invariant(&no_setup, &script, &committed);

    // ...and `x` then expands it (the hotspot is still on the labelled atom).
    script.push(key("x"));
    assert_pointer_split_invariant(&no_setup, &script, &atoms_are(7));
}

