//! The Bond tool: draws. A click grows a bond from an atom, retypes a bond with the chosen
//! order and style, or puts a new atom on empty canvas; a drag from an atom draws a bond to
//! wherever it is let go. Everything else is the default.

use super::{default, Cx, Gesture, Hit, ToolBehavior};
use crate::widget::interact;
use crate::widget::ChemStructEditor;

pub(crate) struct Bond;

impl ToolBehavior for Bond {
    fn on_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        match hit {
            Hit::Atom(atom) => interact::grow_bond(editor, atom, cx.center),
            Hit::Bond(bond) => {
                // With a wedge (or another stereo style) chosen, a single bond becomes that
                // style, its narrow end at the atom nearer the click.
                let narrow = interact::nearer_bond_atom(editor, bond, editor.screen_to_mol(at, cx.center));
                interact::retype_bond_with_tool(editor, bond, narrow);
            }
            Hit::Empty => {
                let pos = editor.screen_to_mol(at, cx.center);
                let id = editor.molecule.add_atom(editor.current_element.clone(), pos, 0);
                editor.hotspot_atom = Some(id);
            }
            Hit::Object(_) => default::on_click(editor, cx, at, hit),
        }
    }

    /// Quick clicks on an atom grow one bond each, as single clicks do; a text or table is
    /// edited, as by default.
    fn on_double_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        match hit {
            Hit::Object(_) => default::on_double_click(editor, cx, at, hit),
            _ => self.on_click(editor, cx, at, hit),
        }
    }

    fn on_drag_start(
        &self,
        editor: &mut ChemStructEditor,
        cx: &Cx,
        origin: egui::Pos2,
        at: egui::Pos2,
        hit: Hit,
    ) -> Option<Gesture> {
        // From an atom, or one the pointer has just crossed (a press slightly off it).
        let from = match hit {
            Hit::Atom(atom) => Some(atom),
            Hit::Empty => editor.hovered_atom,
            Hit::Bond(_) | Hit::Object(_) => None,
        };
        match from {
            Some(from) => Some(Gesture::DrawBond { from, end: at }),
            None => default::on_drag_start(editor, cx, origin, at, hit),
        }
    }
}
