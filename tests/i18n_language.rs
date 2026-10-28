//! The process-wide UI language (in a test binary of its own, as it is global state): unset,
//! the widgets keep their old defaults; set, it decides for all of them.

use chembuider_rs::i18n::{self, Language, Msg};
use chembuider_rs::widget::analysis::AnalysisPanel;
use chembuider_rs::{BondOrder, Molecule, egui};

#[test]
fn the_language_setting() {
    // Unset: English, and the older `ja` flag still gives Japanese, as before.
    assert_eq!(i18n::explicit_language(), None);
    assert_eq!(i18n::language(), Language::En);
    assert_eq!(i18n::tr(Msg::ExactMass), "Exact mass");
    let mut panel = AnalysisPanel::default();
    assert_eq!(
        panel.language(),
        Language::En,
        "the panel's default is unchanged"
    );
    panel.ja = true;
    assert_eq!(panel.language(), Language::Ja);

    // Set: every widget follows it, whatever its flag says.
    i18n::set_language(Language::ZhHant);
    assert_eq!(i18n::language(), Language::ZhHant);
    assert_eq!(i18n::tr(Msg::ExactMass), "精確質量");
    assert_eq!(panel.language(), Language::ZhHant);
    panel.ja = false;
    assert_eq!(panel.language(), Language::ZhHant);

    // The panel draws in each language.
    let ctx = egui::Context::default();
    let mut mol = Molecule::default();
    let c = mol.add_atom("C".into(), [0.0, 0.0], 0);
    let o = mol.add_atom("O".into(), [1.5, 0.0], 0);
    mol.add_bond(c, o, BondOrder::Single);
    for language in Language::ALL {
        i18n::set_language(language);
        assert_eq!(i18n::language(), language);
        assert_eq!(panel.language(), language);
        let mut output = ctx.run_ui(Default::default(), |ui| {
            panel.ui(ui, &mol, &Default::default());
        });
        output.textures_delta.clear(); // no renderer takes them
    }

    i18n::clear_language();
    assert_eq!(i18n::explicit_language(), None);
    assert_eq!(panel.language(), Language::En);
}
