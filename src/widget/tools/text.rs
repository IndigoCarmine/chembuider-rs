//! The Text tool: a click on a text opens it for editing; a click anywhere else opens a new,
//! empty text there (the click is on its first line, at its left end). Typing goes to the text
//! box ([`ChemStructEditor::text_edit`]) until it is committed. Everything else is the default.

use super::{Cx, Hit, ToolBehavior};
use crate::widget::{ChemStructEditor, TextEditState, SCALE_FACTOR};

pub(crate) struct Text;

impl ToolBehavior for Text {
    fn on_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, _hit: Hit) {
        // A text under an atom label is still the text the click is meant for.
        let on_text = editor
            .hit_test_object(at, cx.center)
            .filter(|id| editor.molecule.object_by_id(*id).is_some_and(|o| o.as_text().is_some()));
        match on_text {
            Some(id) => {
                editor.edit_text(id);
            }
            None => {
                // The click marks the middle of the first line's capital letters, so the
                // baseline sits a little below it.
                let size = editor.config.style.label_size / SCALE_FACTOR;
                let pos = editor.screen_to_mol(at, cx.center);
                editor.text_edit = Some(TextEditState::new([pos[0], pos[1] + size * 0.35]));
            }
        }
    }
}
