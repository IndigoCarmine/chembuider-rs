//! An "Analysis" panel in the manner of ChemDraw's: chemical formula, exact mass, molecular
//! weight, the m/z pattern of a chosen ESI ion and the elemental analysis of the structure
//! being drawn.
//!
//! [`AnalysisPanel`] holds only the ion choice (and a cache); the host passes the molecule
//! and selection in every frame:
//!
//! ```no_run
//! # use chembuider_rs::{egui, ChemStructEditor, widget::analysis::AnalysisPanel};
//! # let ui: &mut egui::Ui = unimplemented!();
//! # let editor = ChemStructEditor::default();
//! let mut panel = AnalysisPanel::default(); // keep between frames
//! panel.ui(ui, &editor.molecule, &editor.selected_atoms);
//! ```
//!
//! With atoms selected, the panel analyses the structures they belong to (whole connected
//! components, so a selected part of a molecule does not grow implicit hydrogens at the cut).

use std::collections::HashSet;

use crate::molecule::Molecule;
use crate::reaction::ion::{Adduct, Ion, IonError};
use crate::reaction::{FormulaAnalysis, MolecularFormula};
use crate::widget::rich_text::RichLabel;

/// Which adduct the m/z line is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdductChoice {
    /// The neutral molecule's isotope pattern (what ChemDraw shows).
    Neutral,
    /// One of [`Adduct::presets`], by index.
    Preset(usize),
    /// A species typed in by the user.
    Custom,
}

/// State of the Analysis panel: the ion the user picked, and the last result.
pub struct AnalysisPanel {
    pub adduct: AdductChoice,
    /// Net charge z of the ion (the adduct count follows from it).
    pub charge: i32,
    /// Molecules in the ion (`2` for `[2M+Na]+`).
    pub multimer: usize,
    /// [`AdductChoice::Custom`]: name, formula and charge per unit.
    pub custom_name: String,
    pub custom_formula: String,
    pub custom_charge: i32,
    /// Show the labels in Japanese (the numbers and notation are the same either way).
    pub ja: bool,
    cache: Option<(MolecularFormula, Option<Ion>, FormulaAnalysis)>,
}

/// `en`, or `ja` when the panel is in Japanese.
fn t<'a>(ja: bool, en: &'a str, ja_text: &'a str) -> &'a str {
    if ja { ja_text } else { en }
}

impl Default for AnalysisPanel {
    fn default() -> Self {
        Self {
            adduct: AdductChoice::Neutral,
            charge: 1,
            multimer: 1,
            custom_name: "ACN+H".into(),
            custom_formula: "C2H4N".into(),
            custom_charge: 1,
            ja: false,
            cache: None,
        }
    }
}

impl AnalysisPanel {
    /// The ion the current choice describes: `Ok(None)` for the neutral molecule, `Err` when
    /// the choice makes no ion (zero charge, a charge the adduct cannot reach, a bad formula).
    pub fn ion(&self) -> Result<Option<Ion>, IonError> {
        let adduct = match &self.adduct {
            AdductChoice::Neutral => return Ok(None),
            AdductChoice::Preset(i) => Adduct::presets().swap_remove(*i),
            AdductChoice::Custom => {
                Adduct::custom(&self.custom_name, &self.custom_formula, self.custom_charge)?
            }
        };
        Ok(Some(Ion::simple(adduct, self.charge)?.multimer(self.multimer)))
    }

    /// The analysis of `mol` (or of the selected structures) for the current ion choice.
    /// Recomputed only when the formula or the ion changed.
    pub fn analysis(&mut self, mol: &Molecule, selection: &HashSet<u32>) -> Result<&FormulaAnalysis, IonError> {
        let formula = MolecularFormula::of(&analysis_target(mol, selection));
        let ion = self.ion()?;
        let fresh = matches!(&self.cache, Some((f, i, _)) if *f == formula && *i == ion);
        if !fresh {
            let analysis = FormulaAnalysis::of_formula_with_ion(formula.clone(), ion.clone());
            self.cache = Some((formula, ion, analysis));
        }
        Ok(&self.cache.as_ref().unwrap().2)
    }

    /// Draw the panel. Returns `true` when the user copied the report to the clipboard.
    pub fn ui(&mut self, ui: &mut egui::Ui, mol: &Molecule, selection: &HashSet<u32>) -> bool {
        let ja = self.ja;
        let scope = if selection.is_empty() {
            t(ja, "Whole drawing", "図全体")
        } else {
            t(ja, "Selected structures", "選択した構造")
        };
        ui.heading(t(ja, "Analysis", "解析"));
        ui.weak(scope);
        ui.separator();

        self.ion_controls(ui);
        ui.separator();

        let mut copied = false;
        let ion_result = self.analysis(mol, selection).cloned();
        match ion_result {
            Ok(a) if a.formula.is_empty() => {
                ui.weak(t(ja, "Nothing drawn yet.", "まだ何も描かれていません"));
            }
            Ok(a) => {
                show_values(ui, &a, ja);
                ui.add_space(4.0);
                let copy = ui
                    .button(t(ja, "📋 Copy", "📋 コピー"))
                    .on_hover_text(t(ja, "Copy the analysis as text", "解析結果をテキストでコピー"));
                if copy.clicked() {
                    ui.ctx().copy_text(a.report());
                    copied = true;
                }
            }
            Err(e) => {
                ui.colored_label(ui.visuals().error_fg_color, format!("{}: {e}", t(ja, "Ion", "イオン")));
            }
        }
        copied
    }

    fn ion_controls(&mut self, ui: &mut egui::Ui) {
        let ja = self.ja;
        let presets = Adduct::presets();
        let label = |choice: &AdductChoice| match choice {
            AdductChoice::Neutral => t(ja, "Neutral (M)", "中性 (M)").to_string(),
            AdductChoice::Preset(i) => adduct_label(&presets[*i]),
            AdductChoice::Custom => t(ja, "Custom…", "カスタム…").to_string(),
        };
        egui::Grid::new("analysis_ion").num_columns(2).show(ui, |ui| {
            ui.label(t(ja, "Adduct", "付加イオン"));
            let before = self.adduct.clone();
            egui::ComboBox::from_id_salt("analysis_adduct")
                .selected_text(label(&self.adduct))
                .show_ui(ui, |ui| {
                    let choices = std::iter::once(AdductChoice::Neutral)
                        .chain((0..presets.len()).map(AdductChoice::Preset))
                        .chain(std::iter::once(AdductChoice::Custom));
                    for choice in choices {
                        let text = label(&choice);
                        ui.selectable_value(&mut self.adduct, choice, text);
                    }
                });
            if self.adduct != before {
                self.charge = default_charge(&self.adduct, &presets, self.custom_charge);
            }
            ui.end_row();

            if self.adduct == AdductChoice::Custom {
                ui.label(t(ja, "Name", "名前"));
                ui.add(egui::TextEdit::singleline(&mut self.custom_name).desired_width(80.0));
                ui.end_row();
                ui.label(t(ja, "Formula", "組成式"));
                ui.add(egui::TextEdit::singleline(&mut self.custom_formula).desired_width(80.0));
                ui.end_row();
                ui.label(t(ja, "Charge / unit", "1 単位の電荷"));
                ui.add(egui::DragValue::new(&mut self.custom_charge).range(-3..=3));
                ui.end_row();
            }

            if self.adduct != AdductChoice::Neutral {
                ui.label(t(ja, "Charge", "電荷"));
                ui.add(egui::DragValue::new(&mut self.charge).range(-10..=10).custom_formatter(
                    |z, _| if z > 0.0 { format!("+{z}") } else { format!("{z}") },
                ));
                ui.end_row();
                ui.label(t(ja, "Molecules", "分子数"));
                ui.add(egui::DragValue::new(&mut self.multimer).range(1..=10).prefix("n = "));
                ui.end_row();
                ui.label(t(ja, "Ion", "イオン"));
                match self.ion() {
                    Ok(Some(ion)) => ui.strong(ion.notation()),
                    Ok(None) => ui.label("—"),
                    Err(e) => ui.colored_label(ui.visuals().error_fg_color, e.to_string()),
                };
                ui.end_row();
            }
        });
    }
}

/// Formula, masses, m/z and elemental analysis as a two-column grid.
fn show_values(ui: &mut egui::Ui, a: &FormulaAnalysis, ja: bool) {
    let unknown = || {
        format!("— ({}: {})", t(ja, "unknown", "不明"), a.formula.unknown_elements().join(", "))
    };
    let mass = |m: Option<f64>| m.map_or_else(unknown, |m| format!("{m:.4}"));
    egui::Grid::new("analysis_values").num_columns(2).striped(true).show(ui, |ui| {
        ui.label(t(ja, "Chemical formula", "分子式"));
        ui.add(RichLabel::new(a.formula.rich()));
        ui.end_row();

        ui.label(t(ja, "Exact mass", "精密質量"));
        ui.label(mass(a.exact_mass));
        ui.end_row();

        ui.label(t(ja, "Molecular weight", "分子量"));
        ui.label(mass(a.molecular_weight));
        ui.end_row();

        ui.label(match &a.ion {
            Some(ion) => format!("m/z {ion}"),
            None => "m/z".to_string(),
        });
        match &a.isotope_pattern {
            Ok(peaks) => {
                ui.vertical(|ui| {
                    for p in peaks {
                        ui.label(format!("{:.4} ({:.1}%)", p.mz, p.relative));
                    }
                });
            }
            Err(e) => {
                ui.label(format!("— ({e})"));
            }
        }
        ui.end_row();

        ui.label(t(ja, "Elemental analysis", "元素分析"));
        match &a.elemental_analysis {
            Some(ea) => {
                ui.vertical(|ui| {
                    for (el, pct) in ea {
                        ui.label(format!("{el}, {pct:.2}"));
                    }
                });
            }
            None => {
                ui.label(unknown());
            }
        }
        ui.end_row();
    });
}

/// `H⁺`, `Cl⁻`, `e⁻` — the species with its charge sign, for the adduct list.
fn adduct_label(a: &Adduct) -> String {
    let sign = match a.charge {
        z if z > 0 => "⁺",
        z if z < 0 => "⁻",
        _ => "",
    };
    if a.atoms.is_empty() {
        format!("e{sign} (radical)")
    } else {
        format!("{}{sign}", a.name)
    }
}

/// The charge a newly picked adduct starts at: one unit of it — except the electron, which is
/// picked to make the radical *cation* `[M]+•` (taking one electron away).
fn default_charge(choice: &AdductChoice, presets: &[Adduct], custom_charge: i32) -> i32 {
    match choice {
        AdductChoice::Neutral => 1,
        AdductChoice::Preset(i) if presets[*i].atoms.is_empty() => 1,
        AdductChoice::Preset(i) => presets[*i].charge,
        AdductChoice::Custom if custom_charge != 0 => custom_charge,
        AdductChoice::Custom => 1,
    }
}

/// What the panel analyses: the whole drawing, or — with atoms selected — every connected
/// structure that has a selected atom.
pub fn analysis_target(mol: &Molecule, selection: &HashSet<u32>) -> Molecule {
    if selection.is_empty() {
        return mol.clone();
    }
    let mut keep: HashSet<u32> = selection.iter().copied().filter(|id| mol.atom_by_id(*id).is_some()).collect();
    let mut stack: Vec<u32> = keep.iter().copied().collect();
    while let Some(id) = stack.pop() {
        for b in &mol.bonds {
            let other = if b.begin == id {
                b.end
            } else if b.end == id {
                b.begin
            } else {
                continue;
            };
            if keep.insert(other) {
                stack.push(other);
            }
        }
    }
    let mut out = mol.clone();
    out.atoms.retain(|a| keep.contains(&a.id));
    out.bonds.retain(|b| keep.contains(&b.begin) && keep.contains(&b.end));
    out.ring_fills.clear();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;

    /// Ethylene and, apart from it, methanol.
    fn two_structures() -> (Molecule, u32, u32) {
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".into(), [1.5, 0.0], 0);
        m.add_bond(c1, c2, BondOrder::Double);
        let c3 = m.add_atom("C".into(), [5.0, 0.0], 0);
        let o = m.add_atom("O".into(), [6.5, 0.0], 0);
        m.add_bond(c3, o, BondOrder::Single);
        (m, c1, o)
    }

    #[test]
    fn selection_picks_whole_structures() {
        let (m, c1, o) = two_structures();
        let formula = |sel: &[u32]| {
            MolecularFormula::of(&analysis_target(&m, &sel.iter().copied().collect())).plain()
        };
        assert_eq!(formula(&[]), "C3H8O");
        assert_eq!(formula(&[c1]), "C2H4", "one atom selects its whole molecule, H count intact");
        assert_eq!(formula(&[o]), "CH4O");
        assert_eq!(formula(&[c1, o]), "C3H8O");
    }

    #[test]
    fn ion_choice() {
        let mut p = AnalysisPanel::default();
        assert_eq!(p.ion(), Ok(None));

        let presets = Adduct::presets();
        let idx = |name: &str| presets.iter().position(|a| a.name == name).unwrap();

        p.adduct = AdductChoice::Preset(idx("Na"));
        p.charge = default_charge(&p.adduct, &presets, p.custom_charge);
        assert_eq!(p.ion().unwrap().unwrap().notation(), "[M+Na]+");

        p.adduct = AdductChoice::Preset(idx("Cl"));
        p.charge = default_charge(&p.adduct, &presets, p.custom_charge);
        assert_eq!(p.ion().unwrap().unwrap().notation(), "[M+Cl]-");

        p.adduct = AdductChoice::Preset(idx("e"));
        p.charge = default_charge(&p.adduct, &presets, p.custom_charge);
        assert_eq!(p.ion().unwrap().unwrap().notation(), "[M]+•", "the electron starts as M+•");

        p.adduct = AdductChoice::Preset(idx("H"));
        p.charge = -1;
        p.multimer = 2;
        assert_eq!(p.ion().unwrap().unwrap().notation(), "[2M-H]-");

        p.charge = 0;
        assert_eq!(p.ion(), Err(IonError::Neutral));

        p.adduct = AdductChoice::Custom;
        p.charge = 1;
        p.multimer = 1;
        assert_eq!(p.ion().unwrap().unwrap().notation(), "[M+ACN+H]+");
        p.custom_formula = "c2".into();
        assert!(matches!(p.ion(), Err(IonError::BadFormula(_))));
    }

    #[test]
    fn analysis_is_cached_and_follows_the_ion() {
        let (m, c1, _) = two_structures();
        let sel: HashSet<u32> = [c1].into();
        let mut p = AnalysisPanel::default();
        assert_eq!(p.analysis(&m, &sel).unwrap().formula.plain(), "C2H4");
        assert_eq!(p.analysis(&m, &sel).unwrap().ion, None);

        p.adduct = AdductChoice::Preset(0); // H⁺
        p.charge = 1;
        let a = p.analysis(&m, &sel).unwrap();
        assert_eq!(a.ion.as_ref().unwrap().notation(), "[M+H]+");
        assert_eq!(format!("{:.4}", a.isotope_pattern.as_ref().unwrap()[0].mz), "29.0386");
    }

    /// The panel draws for an empty canvas, a structure, a group label and a bad ion choice.
    #[test]
    fn panel_renders() {
        let ctx = egui::Context::default();
        let (m, _, _) = two_structures();
        let mut labelled = Molecule::default();
        labelled.add_atom("OMe".into(), [0.0, 0.0], 0);
        let mut p = AnalysisPanel::default();
        for (mol, adduct, charge) in [
            (Molecule::default(), AdductChoice::Neutral, 1),
            (m.clone(), AdductChoice::Neutral, 1),
            (m.clone(), AdductChoice::Preset(0), 2),
            (labelled, AdductChoice::Preset(0), 1),
            (m, AdductChoice::Preset(0), 0),
        ] {
            p.adduct = adduct;
            p.charge = charge;
            let _ = crate::testkit::run_ui(&ctx, egui::RawInput::default(), |ui| {
                p.ui(ui, &mol, &HashSet::new());
            });
        }
    }
}
