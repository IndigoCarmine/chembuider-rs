//! What the canvas does with the pointer and the keyboard: one **default behaviour** that every
//! tool shares, and the few gestures each tool does its own way.
//!
//! A tool is a [`ToolBehavior`]. Each of its methods has the default behaviour as its default
//! implementation ([`default`]), so a tool writes only what it does differently, and can still
//! call the default (`default::on_click(...)`) for the cases it leaves alone. The default is what
//! the Select tool does; the Select tool overrides nothing.
//!
//! | input | default | overridden by |
//! |---|---|---|
//! | click | toggle what is under the pointer in the selection; on empty canvas, clear it | Bond (draw), Eraser (erase), Arrow (place), Text (type), Curly arrow (select one) |
//! | double-click | an atom or bond: select its molecule; a text or table cell: edit it | Bond, Eraser, Arrow (a second click) |
//! | drag | from an atom, object, arrow end or a curly arrow's handle: move it (reshape or re-point the arrow); from empty canvas: lasso | Bond (from an atom: draw a bond), Arrow (draw an arrow), Curly arrow (draw one) |
//! | keys | the drawing shortcuts, Delete, Escape, arrows | — |
//!
//! Some things belong to no tool and are not overridable: the selection's rotation handle and
//! flip button, Alt+drag rotation, middle-button pan and wheel zoom (in
//! [`ChemStructEditor::show`]).
//!
//! # Undo
//!
//! Undo steps are taken here, not by the tools: each click and each key is one step if it
//! changed the drawing, and so is each drag, from its start to its end. A tool that changes
//! nothing leaves no empty step behind.
//!
//! # When the canvas takes input
//!
//! [`dispatch`] is given two switches by [`ChemStructEditor::show`]:
//! - `pointer`: off while one of the editor's own boxes is open (a label, a text, a table cell,
//!   the angle box) — a click outside the box closes it and is spent doing so — and while a
//!   cleanup runs. A text field of the host does *not* stop the pointer.
//! - `keys`: off while any text field (the editor's or the host's) has the keyboard, and while
//!   a cleanup runs.

mod arrow;
mod bond;
mod curly;
pub(crate) mod default;
mod eraser;
mod text;

use super::interact::{self, ObjectDrag, PrimaryDrag, RotateDrag};
use super::{ChemStructEditor, KeyboardTarget, Tool};
use crate::molecule::Molecule;

/// What is under a point of the canvas, in the order the pointer takes them: an atom beats a
/// page object, and a page object a bond.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hit {
    Atom(u32),
    Object(u32),
    Bond(u32),
    Empty,
}

impl ChemStructEditor {
    /// What is under screen point `at`.
    pub(crate) fn hit_at(&self, at: egui::Pos2, center: egui::Pos2) -> Hit {
        if let Some(id) = self.hit_test_atom(at, center) {
            Hit::Atom(id)
        } else if let Some(id) = self.hit_test_object(at, center) {
            Hit::Object(id)
        } else if let Some(id) = self.hit_test_bond(at, center) {
            Hit::Bond(id)
        } else {
            Hit::Empty
        }
    }
}

/// What a frame's handlers are given besides the editor.
pub(crate) struct Cx<'a> {
    pub response: &'a egui::Response,
    pub ui: &'a egui::Ui,
    pub center: egui::Pos2,
}

impl Cx<'_> {
    /// Whether Alt was held (at the end of the frame).
    pub fn alt(&self) -> bool {
        self.ui.input(|i| i.modifiers.alt)
    }

    /// Whether Shift was held (at the end of the frame).
    pub fn shift(&self) -> bool {
        self.ui.input(|i| i.modifiers.shift)
    }
}

/// A key pressed this frame (see [`interact::pressed_keys`]).
pub(crate) use interact::PressedKey;

/// Whether the keys after this one are still the tools' to read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum KeyFlow {
    Continue,
    /// The rest of the frame's keys have been handed on (the backtick that opens the label box
    /// does this: what follows it is the label's text).
    Stop,
}

/// A tool: the default behaviour, with what it does differently.
pub(crate) trait ToolBehavior: Sync {
    /// A click (press and release without moving) on `hit` at `at`.
    fn on_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        default::on_click(editor, cx, at, hit)
    }

    /// The second click of a double-click. The first one was an [`on_click`](Self::on_click).
    fn on_double_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        default::on_double_click(editor, cx, at, hit)
    }

    /// A primary drag begins, pressed at `origin` on `hit`; `at` is where the pointer is now
    /// (past egui's drag threshold, maybe well past). The gesture it starts, if any.
    fn on_drag_start(
        &self,
        editor: &mut ChemStructEditor,
        cx: &Cx,
        origin: egui::Pos2,
        at: egui::Pos2,
        hit: Hit,
    ) -> Option<Gesture> {
        default::on_drag_start(editor, cx, origin, at, hit)
    }

    /// A key pressed on the canvas. The hover is set to where the pointer was at that press.
    fn on_key(&self, editor: &mut ChemStructEditor, cx: &Cx, key: &PressedKey) -> KeyFlow {
        default::on_key(editor, cx, key)
    }
}

impl Tool {
    /// How this tool handles input.
    pub(crate) fn behavior(&self) -> &'static dyn ToolBehavior {
        match self {
            Tool::Select => &default::Select,
            Tool::Bond => &bond::Bond,
            Tool::Eraser => &eraser::Eraser,
            Tool::Arrow(_) => &arrow::Arrow,
            Tool::Text => &text::Text,
            Tool::CurlyArrow(_) => &curly::Curly,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Gestures: the drags in progress
// ═══════════════════════════════════════════════════════════════════════════════

/// A primary-button drag in progress. Which one starts is the tool's choice
/// ([`ToolBehavior::on_drag_start`]); how it goes on and ends is the gesture's own, whatever the
/// tool.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Gesture {
    /// Drawing a bond from atom `from` to the pointer at `end` (screen).
    DrawBond { from: u32, end: egui::Pos2 },
    /// A rubber-band selection along `path` (screen), from the press.
    Lasso { path: Vec<egui::Pos2> },
    /// Moving atom `atom`, with the whole selection when it is part of it.
    MoveAtom { atom: u32 },
    /// Moving page objects, or an arrow's end, or drawing a new arrow.
    Object(ObjectDrag),
    /// Turning the selection about its centroid.
    Rotate(RotateDrag),
}

impl Gesture {
    /// Whether the drawing changes while the drag goes on (rather than when it ends).
    fn moves(&self) -> bool {
        matches!(
            self,
            Gesture::MoveAtom { .. }
                | Gesture::Rotate(_)
                | Gesture::Object(
                    ObjectDrag::Move { .. }
                        | ObjectDrag::ArrowEnd { .. }
                        | ObjectDrag::CurlyBend { .. }
                        | ObjectDrag::CurlyPoint { .. }
                        | ObjectDrag::CurlyRepoint { .. }
                )
        )
    }
}

/// The gesture in progress, with the tool it belongs to and the drawing as it was when it began
/// (what undo goes back to, and what an aborted move is put back to).
#[derive(Debug, Clone)]
pub(crate) struct ActiveGesture {
    pub(crate) gesture: Gesture,
    tool: Tool,
    before: Molecule,
    target: KeyboardTarget,
}

impl ChemStructEditor {
    /// The drag in progress, if any.
    pub(crate) fn gesture(&self) -> Option<&Gesture> {
        self.gesture.as_ref().map(|g| &g.gesture)
    }

    fn gesture_mut(&mut self) -> Option<&mut Gesture> {
        self.gesture.as_mut().map(|g| &mut g.gesture)
    }

    /// The path of the rubber band being drawn (empty when there is none).
    pub(crate) fn lasso_path(&self) -> &[egui::Pos2] {
        match self.gesture() {
            Some(Gesture::Lasso { path }) => path,
            _ => &[],
        }
    }

    /// The bond being drawn: the atom it starts at and where the pointer is (screen).
    pub(crate) fn bond_preview(&self) -> Option<(u32, egui::Pos2)> {
        match self.gesture() {
            Some(Gesture::DrawBond { from, end }) => Some((*from, *end)),
            _ => None,
        }
    }

    /// The rotation in progress.
    pub(crate) fn rotate_drag(&self) -> Option<&RotateDrag> {
        match self.gesture() {
            Some(Gesture::Rotate(rot)) => Some(rot),
            _ => None,
        }
    }

    /// The page-object drag in progress.
    pub(crate) fn object_drag(&self) -> Option<&ObjectDrag> {
        match self.gesture() {
            Some(Gesture::Object(drag)) => Some(drag),
            _ => None,
        }
    }

    /// Give up the gesture in progress, putting back what it moved: what Escape does to a drag,
    /// and what switching tools does. Returns whether the drawing changed (back).
    pub(crate) fn abort_gesture(&mut self) -> bool {
        match self.gesture.take() {
            Some(active) if active.gesture.moves() && self.molecule != active.before => {
                self.molecule = active.before;
                true
            }
            _ => false,
        }
    }

    /// Switch to `tool`. What was in progress with the old one ends: a drag is given up, and the
    /// keyboard's bond target and ring repeat are dropped. Hosts should switch tools with this;
    /// a `tool` written directly still has its old drag given up on the next frame.
    pub fn set_tool(&mut self, tool: Tool) {
        if self.tool == tool {
            return;
        }
        let _ = self.abort_gesture();
        self.hotspot_bond = None;
        self.ring_repeat = None;
        self.tool = tool;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// The frame
// ═══════════════════════════════════════════════════════════════════════════════

/// Do one change as one undo step: run `change`, and if the drawing is not what it was, record
/// what it was (aimed where the keyboard was aiming). A change that pushed its own step leaves
/// the same one, which this then does not repeat. Returns what `change` did, and whether the
/// drawing changed.
pub(crate) fn as_one_step<R>(
    editor: &mut ChemStructEditor,
    change: impl FnOnce(&mut ChemStructEditor) -> R,
) -> (R, bool) {
    let before = (editor.molecule.clone(), editor.keyboard_target());
    let result = change(editor);
    let changed = editor.molecule != before.0;
    if changed {
        record_step(editor, before.0, before.1);
    }
    (result, changed)
}

/// Record `molecule`, the drawing before a change already made, as an undo step aimed at
/// `target`.
fn record_step(editor: &mut ChemStructEditor, molecule: Molecule, target: KeyboardTarget) {
    let after = std::mem::replace(&mut editor.molecule, molecule);
    editor.push_undo_aimed_at(target);
    editor.molecule = after;
}

/// Handle this frame's canvas input with the current tool. Returns whether the drawing changed.
pub(crate) fn dispatch(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
    pointer: bool,
    keys: bool,
) -> bool {
    let cx = Cx { response, ui, center };
    let tool = editor.tool.behavior();
    let mut changed = false;

    // A drag left over from another tool (a host that set `tool` directly) is given up.
    if editor.gesture.as_ref().is_some_and(|g| g.tool != editor.tool) {
        changed |= editor.abort_gesture();
    }

    let mut aborted = false;
    if pointer {
        let (drag_aborted, pointer_changed) = pointer_input(editor, &cx, tool);
        aborted = drag_aborted;
        changed |= pointer_changed;
    } else if editor.gesture.is_some() {
        // The canvas stopped taking the pointer in mid-drag (a cleanup started).
        changed |= editor.abort_gesture();
    }
    if keys {
        changed |= key_input(editor, &cx, tool, aborted);
    }
    changed
}

/// The pointer: the selection's handles, then clicks and drags as the tool has them. Returns
/// whether a drag was aborted (Escape) this frame, and whether the drawing changed.
fn pointer_input(editor: &mut ChemStructEditor, cx: &Cx, tool: &dyn ToolBehavior) -> (bool, bool) {
    let mut changed = false;
    let response = cx.response;
    let at = response
        .interact_pointer_pos()
        .or_else(|| cx.ui.input(|i| i.pointer.latest_pos()));
    let drag = PrimaryDrag::of(response, cx.ui);

    // The selection's rotation handle and flip button, whatever the tool.
    let handle = interact::rotation_handle(editor, cx.center);
    let on_handle = |p: egui::Pos2| handle.is_some_and(|h| h.distance(p) <= interact::ROTATE_HANDLE_HIT);
    let flip_at = interact::flip_button(editor, cx.center);
    let on_flip = |p: egui::Pos2| flip_at.is_some_and(|f| f.distance(p) <= interact::ROTATE_HANDLE_HIT);
    let rotating = editor.rotate_drag().is_some();
    let hover = cx.ui.input(|i| i.pointer.hover_pos());
    if rotating || hover.is_some_and(on_handle) {
        cx.ui.ctx().set_cursor_icon(if rotating { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
    } else if hover.is_some_and(on_flip) {
        cx.ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // ── Clicks ───────────────────────────────────────────────────────────────
    if (response.clicked() || response.double_clicked())
        && let Some(at) = at
    {
        if on_flip(at) {
            changed |= editor.flip_selection();
        } else if on_handle(at) {
            // A double-click on the handle asks for the angle instead of a drag.
            if response.double_clicked() {
                editor.open_rotate_input();
            }
        } else if interact::curly_handle_at(editor, at, cx.center).is_some()
            || (response.double_clicked() && is_curly_at(editor, at, cx.center))
        {
            // A curly arrow's handles are dragged, not clicked: a click on one keeps the
            // selection, and a double-click there, or on a curly arrow's curve, shapes it.
            if response.double_clicked() {
                let (_, shaped) = as_one_step(editor, |editor| interact::curly_double_click(editor, at, cx.center));
                changed |= shaped;
            }
        } else {
            let hit = editor.hit_at(at, cx.center);
            let ((), click_changed) = as_one_step(editor, |editor| {
                if response.double_clicked() {
                    tool.on_double_click(editor, cx, at, hit);
                } else {
                    tool.on_click(editor, cx, at, hit);
                }
            });
            changed |= click_changed;
        }
    }

    // ── Drags ────────────────────────────────────────────────────────────────
    if drag.started
        && let Some(at) = at
    {
        // What the drag acts on is picked at the press: by the time `drag_started()` fires the
        // pointer may already have left the atom it was pressed on.
        let origin = cx.ui.input(|i| i.pointer.press_origin()).unwrap_or(at);
        editor.gesture = None;
        let before = editor.molecule.clone();
        let target = editor.keyboard_target();
        let by_handle = on_handle(origin);
        let gesture = if by_handle || (cx.alt() && !editor.selected_atoms.is_empty()) {
            Some(Gesture::Rotate(interact::start_rotation(editor, cx.center, origin, at, handle, by_handle)))
        } else {
            let hit = editor.hit_at(origin, cx.center);
            tool.on_drag_start(editor, cx, origin, at, hit)
        };
        editor.gesture = gesture.map(|gesture| ActiveGesture { gesture, tool: editor.tool.clone(), before, target });
    }

    if drag.dragged
        && let Some(at) = at
    {
        drag_on(editor, cx, at);
        changed |= editor.gesture().is_some_and(Gesture::moves);
    }

    let mut aborted = false;
    if drag.stopped {
        aborted = drag.aborted;
        if aborted {
            changed |= editor.abort_gesture();
        } else if let Some(active) = editor.gesture.take() {
            finish(editor, cx, active.gesture);
            if editor.molecule != active.before {
                record_step(editor, active.before, active.target);
                changed = true;
            }
        }
    }

    // Keyboard focus comes to the canvas pressed on: away from a host's text field, and onto
    // the canvas, so a Tab or an arrow does not move it on to a button that Space or Enter would
    // then press.
    canvas_focus(cx);
    (aborted, changed)
}

/// Whether a curly arrow is what is under screen point `at`.
fn is_curly_at(editor: &ChemStructEditor, at: egui::Pos2, center: egui::Pos2) -> bool {
    matches!(editor.hit_at(at, center), Hit::Object(id) if editor.molecule.object_by_id(id).is_some_and(|o| o.as_curly().is_some()))
}

/// The pointer moved to `at` during the drag.
fn drag_on(editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2) {
    let delta = editor.view(cx.center).screen_delta_to_mol(cx.response.drag_delta());
    let shift = cx.shift();
    let Some(gesture) = editor.gesture().cloned() else { return };
    match gesture {
        Gesture::DrawBond { .. } => {
            if let Some(Gesture::DrawBond { end, .. }) = editor.gesture_mut() {
                *end = at;
            }
        }
        Gesture::Lasso { .. } => {
            if let Some(Gesture::Lasso { path }) = editor.gesture_mut() {
                path.push(at);
            }
        }
        Gesture::MoveAtom { atom } => interact::drag_atom(editor, atom, delta),
        Gesture::Object(ObjectDrag::NewCurly(mut drawing)) => {
            drawing.head = interact::curly_target(editor, &drawing.tail, at, cx.center, shift);
            if let Some(Gesture::Object(ObjectDrag::NewCurly(d))) = editor.gesture_mut() {
                *d = drawing;
            }
        }
        Gesture::Object(drag) => {
            let drag = interact::drag_object(editor, drag, delta, editor.screen_to_mol(at, cx.center), shift);
            if let Some(Gesture::Object(d)) = editor.gesture_mut() {
                *d = drag;
            }
        }
        Gesture::Rotate(mut rot) => {
            interact::drag_rotation(editor, &mut rot, editor.screen_to_mol(at, cx.center), shift);
            if let Some(Gesture::Rotate(r)) = editor.gesture_mut() {
                *r = rot;
            }
        }
    }
}

/// The drag ended (the button was released): finish `gesture`.
fn finish(editor: &mut ChemStructEditor, cx: &Cx, gesture: Gesture) {
    match gesture {
        Gesture::DrawBond { from, end } => interact::finish_bond(editor, from, end, cx.center),
        Gesture::Lasso { path } => interact::lasso_select(editor, &path, cx.center),
        Gesture::Object(ObjectDrag::NewArrow { tail, head, kind }) => interact::finish_arrow(editor, tail, head, kind),
        Gesture::Object(ObjectDrag::NewCurly(drag)) => interact::finish_curly(editor, &drag),
        Gesture::Object(ObjectDrag::CurlyRepoint { id, head, at }) => {
            interact::finish_curly_repoint(editor, id, head, at, cx.center, cx.shift())
        }
        Gesture::MoveAtom { .. } | Gesture::Object(_) | Gesture::Rotate(_) => {}
    }
}

/// Give the canvas egui's keyboard focus when it is pressed, and keep Tab, the arrows and
/// Escape from moving it on (they are the canvas's keys).
///
/// A host therefore cannot read "the canvas has no keyboard" from
/// [`egui_wants_keyboard_input`](egui::Context::egui_wants_keyboard_input): whether a text
/// field is typing is [`text_edit_focused`](egui::Context::text_edit_focused).
fn canvas_focus(cx: &Cx) {
    let id = cx.response.id;
    let pressed_here = cx.response.is_pointer_button_down_on();
    cx.ui.ctx().memory_mut(|m| {
        if pressed_here && m.focused() != Some(id) {
            m.request_focus(id);
        }
        if m.has_focus(id) {
            m.set_focus_lock_filter(id, egui::EventFilter {
                tab: true,
                horizontal_arrows: true,
                vertical_arrows: true,
                escape: true,
            });
        }
    });
}

/// The keys, each in the order it was pressed, with the hover where the pointer was at that
/// press, each one undo step. Returns whether the drawing changed.
fn key_input(editor: &mut ChemStructEditor, cx: &Cx, tool: &dyn ToolBehavior, aborted: bool) -> bool {
    let mut changed = false;
    let end_hover = interact::Hover::of(editor);
    let end_pointer = cx.response.hover_pos().filter(|p| cx.response.rect.contains(*p));
    for key in &interact::pressed_keys(cx.ui) {
        // The Escape that aborted a drag is spent on that.
        if aborted && key.key == "Escape" {
            continue;
        }
        interact::hover_at_key(editor, cx.ui, cx.response, cx.center, key.index, end_pointer, end_hover);
        let (flow, key_changed) = as_one_step(editor, |editor| tool.on_key(editor, cx, key));
        changed |= key_changed;
        if flow == KeyFlow::Stop {
            break;
        }
    }
    // The frame ends with the pointer where it ended up, for drawing and its own gestures.
    end_hover.restore(editor);
    changed
}

#[cfg(test)]
mod tests;
