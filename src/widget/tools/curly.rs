//! The Curly arrow tool: a drag from an atom (a lone pair) or a bond (its electrons) to the atom
//! or bond the electrons go to draws an electron-pushing arrow ([`crate::molecule::curly`]); a
//! click on a curly arrow flips the side it bends to. The handle in the middle of the selected
//! arrow bends it, here as with every tool. Everything else is the default.

use super::{default, Cx, Gesture, Hit, ToolBehavior};
use crate::molecule::{CurlyEnd, CurlyKind, PageObject};
use crate::widget::interact::{self, CurlyDrag, ObjectDrag};
use crate::widget::{ChemStructEditor, Tool};

pub(crate) struct Curly;

/// The kind of arrow the tool draws.
fn kind(editor: &ChemStructEditor) -> CurlyKind {
    match &editor.tool {
        Tool::CurlyArrow(kind) => *kind,
        _ => CurlyKind::Pair,
    }
}

/// Flip curly arrow `id` when `hit` is one; whether it was.
fn flip(editor: &mut ChemStructEditor, hit: Hit) -> bool {
    let Hit::Object(id) = hit else { return false };
    match editor.molecule.object_by_id_mut(id) {
        Some(PageObject::Curly(c)) => {
            c.flip();
            true
        }
        _ => false,
    }
}

impl ToolBehavior for Curly {
    fn on_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        if !flip(editor, hit) {
            default::on_click(editor, cx, at, hit);
        }
    }

    /// Quick clicks on an arrow flip it each time.
    fn on_double_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        if !flip(editor, hit) {
            default::on_double_click(editor, cx, at, hit);
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
        if let Some(bend) = interact::curly_handle_at(editor, origin, cx.center) {
            return Some(Gesture::Object(bend));
        }
        let from = editor.screen_to_mol(origin, cx.center);
        // A bond under an arrow already drawn over it is still a bond to start from.
        let bond = match hit {
            Hit::Bond(b) => Some(b),
            Hit::Object(_) => editor.hit_test_bond(origin, cx.center),
            _ => None,
        };
        let tail = match (hit, bond) {
            (Hit::Atom(a), _) => CurlyEnd::Atom(a),
            (_, Some(bond)) => CurlyEnd::Bond { bond, side: interact::nearer_bond_atom(editor, bond, from)? },
            (Hit::Object(_), None) => return default::on_drag_start(editor, cx, origin, at, hit),
            _ => CurlyEnd::Point(from),
        };
        Some(Gesture::Object(ObjectDrag::NewCurly(CurlyDrag { tail, head: CurlyEnd::Point(from), kind: kind(editor) })))
    }
}
