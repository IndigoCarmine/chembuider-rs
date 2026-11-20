//! The Eraser tool: a click removes what it is on. Everything else is the default (a drag
//! lassoes, and Delete then removes what it took).

use super::{Cx, Hit, ToolBehavior};
use crate::widget::interact;
use crate::widget::ChemStructEditor;

pub(crate) struct Eraser;

impl ToolBehavior for Eraser {
    fn on_click(&self, editor: &mut ChemStructEditor, _cx: &Cx, _at: egui::Pos2, hit: Hit) {
        interact::erase(editor, hit);
    }

    /// Quick clicks erase one thing each.
    fn on_double_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        self.on_click(editor, cx, at, hit);
    }
}
