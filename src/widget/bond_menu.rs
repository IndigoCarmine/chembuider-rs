//! The contents of a ChemDraw-style bond context menu, for a host to show.
//!
//! The editor does not open menus itself: a host takes the canvas response from
//! [`ChemStructEditor::show`], and inside its own
//! [`context_menu`](egui::Response::context_menu) calls [`bond_menu`] for the bond the canvas
//! was right-clicked on ([`ChemStructEditor::context_bond`]). The menu only *reports* the
//! choice; the host applies it after the closure, with [`ChemStructEditor::apply_bond_menu`]:
//!
//! ```ignore
//! let out = editor.show(ui);
//! let mut chosen = None;
//! let menu = out.response.context_menu(|ui| {
//!     match editor.context_bond().and_then(|id| editor.molecule.bond_by_id(id)) {
//!         Some(bond) => chosen = bond_menu(ui, bond, false),
//!         None => { ui.label("Right-click a bond"); }
//!     }
//! });
//! if menu.is_none() { editor.clear_context_bond(); }
//! if let (Some(action), Some(id)) = (chosen, editor.context_bond()) {
//!     editor.apply_bond_menu(id, action);
//! }
//! ```

use super::ChemStructEditor;
use crate::i18n::{self, Language, Msg, tr_in};
use crate::molecule::{Bond, BondOrder, BondStereo, DoublePosition};

/// What the user picked from [`bond_menu`].
#[derive(Debug, Clone, PartialEq)]
pub enum BondMenuAction {
    /// Give the bond this order and line style ("Single ▸ Hashed Wedged", "Double", …).
    SetType(BondOrder, BondStereo),
    /// Put the double bond's second line on this side ("Bond Position ▸ Left").
    SetDoublePosition(DoublePosition),
    /// Remove the bond.
    Delete,
}

/// The single-bond styles, in ChemDraw's menu order.
const SINGLE_STYLES: [(Msg, BondStereo); 9] = [
    (Msg::StylePlain, BondStereo::None),
    (Msg::StyleDashed, BondStereo::Dashed),
    (Msg::StyleHashed, BondStereo::Hashed),
    (Msg::StyleHashedWedged, BondStereo::WedgeDown),
    (Msg::StyleBold, BondStereo::Bold),
    (Msg::StyleBoldWedged, BondStereo::WedgeUp),
    (Msg::StyleHollowWedged, BondStereo::HollowWedge),
    (Msg::StyleDative, BondStereo::Dative),
    (Msg::StyleWavy, BondStereo::Wavy),
];

const POSITIONS: [(Msg, DoublePosition); 4] = [
    (Msg::PositionAuto, DoublePosition::Auto),
    (Msg::PositionLeft, DoublePosition::Left),
    (Msg::PositionCenter, DoublePosition::Center),
    (Msg::PositionRight, DoublePosition::Right),
];

/// A menu entry with ChemDraw's check mark in front when it is the bond's current value.
fn item(ui: &mut egui::Ui, checked: bool, label: &str) -> bool {
    let mark = if checked { "✔" } else { "   " };
    ui.add(egui::Button::new(format!("{mark} {label}"))).clicked()
}

/// Draw the bond menu's entries into `ui` (the inside of a context menu) for `bond`, checking
/// the bond's current order, style and double-bond position. Labels are in the language set
/// with [`i18n::set_language`], or while none is, Japanese when `ja` and English otherwise.
/// Returns the entry clicked this frame, closing the menu when there is one.
pub fn bond_menu(ui: &mut egui::Ui, bond: &Bond, ja: bool) -> Option<BondMenuAction> {
    bond_menu_in(ui, bond, i18n::language_or(if ja { Language::Ja } else { Language::En }))
}

/// [`bond_menu`] with its labels in `language`, whatever language is set.
pub fn bond_menu_in(ui: &mut egui::Ui, bond: &Bond, language: Language) -> Option<BondMenuAction> {
    let t = |msg| tr_in(language, msg);
    let mut action = None;

    let single = bond.order == BondOrder::Single;
    ui.menu_button(format!("{} {}", if single { "✔" } else { "   " }, t(Msg::BondSingle)), |ui| {
        for (msg, stereo) in SINGLE_STYLES {
            if item(ui, single && bond.stereo == stereo, t(msg)) {
                action = Some(BondMenuAction::SetType(BondOrder::Single, stereo));
            }
        }
    });
    for (msg, order) in [
        (Msg::BondDouble, BondOrder::Double),
        (Msg::BondTriple, BondOrder::Triple),
        (Msg::BondQuadruple, BondOrder::Quadruple),
    ] {
        if item(ui, bond.order == order, t(msg)) {
            action = Some(BondMenuAction::SetType(order, BondStereo::None));
        }
    }

    ui.separator();
    // Only a double bond has a second line to place.
    ui.add_enabled_ui(bond.order == BondOrder::Double, |ui| {
        ui.menu_button(format!("    {}", t(Msg::BondPosition)), |ui| {
            for (msg, position) in POSITIONS {
                if item(ui, bond.double_position == position, t(msg)) {
                    action = Some(BondMenuAction::SetDoublePosition(position));
                }
            }
        });
    });

    ui.separator();
    if item(ui, false, t(Msg::DeleteBond)) {
        action = Some(BondMenuAction::Delete);
    }

    if action.is_some() {
        ui.close();
    }
    action
}

impl ChemStructEditor {
    /// Apply what [`bond_menu`] returned to bond `bond_id`, as one undo step. Returns whether
    /// the molecule changed.
    pub fn apply_bond_menu(&mut self, bond_id: u32, action: BondMenuAction) -> bool {
        match action {
            BondMenuAction::SetType(order, stereo) => self.set_bond_type(bond_id, order, stereo),
            BondMenuAction::SetDoublePosition(position) => self.set_bond_double_position(bond_id, position),
            BondMenuAction::Delete => self.delete_bond(bond_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::Molecule;
    use crate::testkit::run_frame;

    /// `N–N`, each nitrogen with its two group hydrogens (as the `n` key's NH₂ draws them).
    fn hydrazine() -> (ChemStructEditor, u32) {
        let mut editor = ChemStructEditor::default();
        let mol = &mut editor.molecule;
        let n1 = mol.add_atom("N".to_string(), [0.0, 0.0], 0);
        let n2 = mol.add_atom("N".to_string(), [1.5, 0.0], 0);
        for (n, x) in [(n1, -0.8), (n2, 2.3)] {
            for y in [-0.6, 0.6] {
                let h = mol.add_atom("H".to_string(), [x, y], 0);
                mol.atom_by_id_mut(h).unwrap().group_h = true;
                mol.add_bond(n, h, BondOrder::Single);
            }
        }
        let bond = mol.add_bond(n1, n2, BondOrder::Single);
        (editor, bond)
    }

    fn hydrogens(editor: &ChemStructEditor) -> usize {
        editor.molecule.atoms.iter().filter(|a| a.element == "H").count()
    }

    #[test]
    fn choosing_a_bond_type_is_one_undo_step() {
        let (mut editor, bond) = hydrazine();
        let before = editor.molecule.clone();
        let action = BondMenuAction::SetType(BondOrder::Single, BondStereo::HollowWedge);
        assert!(editor.apply_bond_menu(bond, action.clone()));
        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().stereo, BondStereo::HollowWedge);
        assert!(!editor.apply_bond_menu(bond, action), "choosing the current type changes nothing");
        assert!(editor.undo());
        assert_eq!(editor.molecule, before);
    }

    /// Raising the order takes the group hydrogens it leaves no room for, as the order keys
    /// do (N–N becomes the N=N of a diazene); lowering it again takes none.
    #[test]
    fn raising_the_order_consumes_group_hydrogens_and_lowering_it_does_not() {
        let (mut editor, bond) = hydrazine();
        assert_eq!(hydrogens(&editor), 4);
        editor.apply_bond_menu(bond, BondMenuAction::SetType(BondOrder::Double, BondStereo::None));
        assert_eq!(hydrogens(&editor), 2);
        editor.apply_bond_menu(bond, BondMenuAction::SetType(BondOrder::Single, BondStereo::Dashed));
        assert_eq!(hydrogens(&editor), 2);
        let b = editor.molecule.bond_by_id(bond).unwrap();
        assert_eq!((&b.order, &b.stereo), (&BondOrder::Single, &BondStereo::Dashed));
    }

    #[test]
    fn a_style_is_dropped_on_a_multiple_bond() {
        let (mut editor, bond) = hydrazine();
        editor.set_bond_type(bond, BondOrder::Triple, BondStereo::WedgeUp);
        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().stereo, BondStereo::None);
    }

    #[test]
    fn the_double_position_and_delete_are_undoable() {
        let (mut editor, bond) = hydrazine();
        assert!(editor.apply_bond_menu(bond, BondMenuAction::SetDoublePosition(DoublePosition::Left)));
        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().double_position, DoublePosition::Left);
        assert!(editor.apply_bond_menu(bond, BondMenuAction::Delete));
        assert!(editor.molecule.bond_by_id(bond).is_none());
        assert!(editor.undo());
        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().double_position, DoublePosition::Left);
        assert!(editor.undo());
        assert_eq!(editor.molecule.bond_by_id(bond).unwrap().double_position, DoublePosition::Auto);
    }

    /// A right-click remembers the bond under the pointer for the host's menu, and keeps it
    /// while the pointer moves off onto the menu; a right-click on empty canvas forgets it.
    #[test]
    fn a_right_click_on_a_bond_makes_it_the_context_bond() {
        let ctx = egui::Context::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
        let mut editor = ChemStructEditor::default();
        let mut mol = Molecule::default();
        let a = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [1.5, 0.0], 0);
        let bond = mol.add_bond(a, b, BondOrder::Single);
        editor.molecule = mol;

        let right_click = |editor: &mut ChemStructEditor, p: egui::Pos2| {
            run_frame(&ctx, screen_rect, editor, vec![egui::Event::PointerMoved(p)]);
            for pressed in [true, false] {
                run_frame(&ctx, screen_rect, editor, vec![egui::Event::PointerButton {
                    pos: p,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: Default::default(),
                }]);
            }
        };
        let on_bond = editor.mol_to_screen([0.75, 0.0], screen_rect.center());
        right_click(&mut editor, on_bond);
        assert_eq!(editor.context_bond(), Some(bond));
        assert_eq!(editor.molecule.atoms.len(), 2, "a right-click draws nothing");

        run_frame(&ctx, screen_rect, &mut editor, vec![egui::Event::PointerMoved(on_bond + egui::vec2(40.0, 60.0))]);
        assert_eq!(editor.hovered_bond, None);
        assert_eq!(editor.context_bond(), Some(bond), "kept while the pointer is on the menu");

        right_click(&mut editor, egui::pos2(20.0, 20.0));
        assert_eq!(editor.context_bond(), None);
    }

    /// The menu draws in a real context menu, checks the bond's current entries, and reports
    /// nothing until one is clicked.
    #[test]
    fn the_menu_draws_and_reports_no_choice_without_a_click() {
        let ctx = egui::Context::default();
        let (editor, bond) = hydrazine();
        let bond = editor.molecule.bond_by_id(bond).unwrap().clone();
        for ja in [false, true] {
            let _ = crate::testkit::run_ui(&ctx, egui::RawInput::default(), |ui| {
                assert_eq!(bond_menu(ui, &bond, ja), None);
            });
        }
        for language in Language::ALL {
            let _ = crate::testkit::run_ui(&ctx, egui::RawInput::default(), |ui| {
                assert_eq!(bond_menu_in(ui, &bond, language), None);
            });
        }
    }
}
