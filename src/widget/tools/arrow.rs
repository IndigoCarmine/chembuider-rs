//! The Arrow tool: a drag from anywhere draws a reaction or equilibrium arrow from tail to head
//! (its direction snaps to 15° steps unless Shift is held); a click puts one of the default
//! length there, pointing right. Everything else is the default.

use super::{Cx, Gesture, Hit, ToolBehavior};
use crate::molecule::page::DEFAULT_ARROW_LENGTH;
use crate::molecule::ArrowKind;
use crate::widget::interact::ObjectDrag;
use crate::widget::{ChemStructEditor, Tool};

pub(crate) struct Arrow;

/// The kind of arrow the tool draws.
fn kind(editor: &ChemStructEditor) -> ArrowKind {
    match &editor.tool {
        Tool::Arrow(kind) => *kind,
        _ => ArrowKind::Reaction,
    }
}

impl ToolBehavior for Arrow {
    fn on_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, _hit: Hit) {
        let tail = editor.screen_to_mol(at, cx.center);
        let kind = kind(editor);
        editor.molecule.add_arrow(tail, [tail[0] + DEFAULT_ARROW_LENGTH, tail[1]], kind);
    }

    /// Quick clicks put one arrow each.
    fn on_double_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        self.on_click(editor, cx, at, hit);
    }

    fn on_drag_start(
        &self,
        editor: &mut ChemStructEditor,
        cx: &Cx,
        origin: egui::Pos2,
        _at: egui::Pos2,
        _hit: Hit,
    ) -> Option<Gesture> {
        let tail = editor.screen_to_mol(origin, cx.center);
        Some(Gesture::Object(ObjectDrag::NewArrow { tail, head: tail, kind: kind(editor) }))
    }
}
