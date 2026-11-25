//! The Curly arrow tool: a drag from an atom (a lone pair) or a bond (its electrons) to the atom
//! or bond the electrons go to draws an electron-pushing arrow ([`crate::molecule::curly`]); a
//! click on a curly arrow makes it the selection, showing its handles (which shape it, here as
//! with every tool). Everything else is the default.

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

/// Make curly arrow `id` the selection when `hit` is one; whether it was.
fn select(editor: &mut ChemStructEditor, hit: Hit) -> bool {
    let Hit::Object(id) = hit else { return false };
    if !matches!(editor.molecule.object_by_id(id), Some(PageObject::Curly(_))) {
        return false;
    }
    editor.selected_atoms.clear();
    editor.selected_bonds.clear();
    editor.selected_objects = [id].into_iter().collect();
    true
}

impl ToolBehavior for Curly {
    fn on_click(&self, editor: &mut ChemStructEditor, cx: &Cx, at: egui::Pos2, hit: Hit) {
        if !select(editor, hit) {
            default::on_click(editor, cx, at, hit);
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
        // The source: a lone pair, or a bond's electrons onto the atom nearer the press.
        let source = match (hit, bond) {
            (Hit::Atom(a), _) => CurlyEnd::Atom(a),
            (_, Some(bond)) => CurlyEnd::Bond { bond, onto: interact::nearer_bond_atom(editor, bond, from)? },
            (Hit::Object(_), None) => return default::on_drag_start(editor, cx, origin, at, hit),
            _ => CurlyEnd::Point(from),
        };
        Some(Gesture::Object(ObjectDrag::NewCurly(CurlyDrag { from: source, to: CurlyEnd::Point(from), kind: kind(editor) })))
    }
}
