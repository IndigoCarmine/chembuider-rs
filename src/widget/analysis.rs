//! An "Analysis" panel in the manner of ChemDraw's: chemical formula, exact mass, molecular
//! weight, the m/z pattern of a chosen ESI ion, the elemental analysis and the IUPAC name
//! ([`crate::naming`]) of the structure being drawn.
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
//!
//! Brackets with a number (`[CH2–CH2–O]3`, a multiple group) are written out, so every value is
//! that of the molecule they stand for. A repeat unit labelled `n` is shown as `C2H6(C2H4O)n`,
//! end groups first; its values are those of `n = 1`, and the masses are also given as the end
//! groups' plus `n` times the unit's.

use std::collections::HashSet;

use crate::molecule::Molecule;
use crate::molecule::bracket::expand;
use crate::naming::{has_stereo_marks, iupac_name, NameResult};
use crate::reaction::ion::{Adduct, Ion, IonError};
use crate::reaction::{FormulaAnalysis, MolecularFormula, PolymerFormula};
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
    /// The structures last named, and their name (naming is too slow to redo every frame).
    name_cache: Option<(Molecule, NameResult)>,
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
            name_cache: None,
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

    /// The IUPAC name of `mol` (or of the selected structures). Recomputed only when they
    /// changed.
    pub fn iupac_name(&mut self, mol: &Molecule, selection: &HashSet<u32>) -> &NameResult {
        let target = expand(&analysis_target(mol, selection));
        let fresh = matches!(&self.name_cache, Some((m, _)) if *m == target);
        if !fresh {
            let name = iupac_name(&target);
            self.name_cache = Some((target, name));
        }
        &self.name_cache.as_ref().unwrap().1
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
        // A polymer's formula is written with its repeat units labelled `n`, `C2H6(C2H4)n`; the
        // values are then those of n = 1 (the end groups and each unit once).
        let polymer = if mol.brackets().next().is_some() {
            PolymerFormula::of(&analysis_target(mol, selection))
        } else {
            None
        };
        match ion_result {
            Ok(a) if a.formula.is_empty() => {
                ui.weak(t(ja, "Nothing drawn yet.", "まだ何も描かれていません"));
            }
            Ok(a) => {
                let name = self.iupac_name(mol, selection).clone();
                let stereo = has_stereo_marks(&analysis_target(mol, selection));
                if show_values(ui, &a, polymer.as_ref(), &name, stereo, ja) {
                    copied = true;
                }
                ui.add_space(4.0);
                let copy = ui
                    .button(t(ja, "📋 Copy", "📋 コピー"))
                    .on_hover_text(t(ja, "Copy the analysis as text", "解析結果をテキストでコピー"));
                if copy.clicked() {
                    let mut report = a.report();
                    if let Some(p) = &polymer {
                        let plain = format!("Chemical formula: {}", a.formula.plain());
                        let mut with = format!("Chemical formula: {} (values for n = 1: {})", p.plain(), a.formula.plain());
                        if let Some(m) = per_unit(p, MolecularFormula::exact_mass) {
                            with.push_str(&format!("\nExact mass with n: {m}"));
                        }
                        report = report.replacen(&plain, &with, 1);
                    }
                    ui.ctx().copy_text(format!("{}
IUPAC name: {}", report, name_text(&name)));
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

/// The name, or why there is none, as one line of text.
fn name_text(name: &NameResult) -> String {
    match name {
        NameResult::Ok(n) => n.clone(),
        NameResult::Unsupported(reason) => format!("— (unsupported: {reason})"),
    }
}

/// A polymer's mass as the end groups' plus each unit's times its label: `30.0470 + 44.0262 n`
/// (`mass` is [`MolecularFormula::exact_mass`] or [`MolecularFormula::weight`]); `None` when a
/// part holds a group label.
fn per_unit(p: &PolymerFormula, mass: fn(&MolecularFormula) -> Option<f64>) -> Option<String> {
    let mut parts = Vec::new();
    if !p.rest.is_empty() {
        parts.push(format!("{:.4}", mass(&p.rest)?));
    }
    for (f, label) in &p.units {
        parts.push(format!("{:.4} {label}", mass(f)?));
    }
    Some(parts.join(" + "))
}

/// Formula, masses, m/z, elemental analysis and IUPAC name as a two-column grid. A polymer's
/// formula is shown with its repeat units (`polymer`), its values marked as those of n = 1 and
/// its masses also given per n. Returns `true` when the user copied the name.
fn show_values(
    ui: &mut egui::Ui,
    a: &FormulaAnalysis,
    polymer: Option<&PolymerFormula>,
    name: &NameResult,
    stereo: bool,
    ja: bool,
) -> bool {
    let mut copied = false;
    let unknown = || {
        format!("— ({}: {})", t(ja, "unknown", "不明"), a.formula.unknown_elements().join(", "))
    };
    let mass = |m: Option<f64>| m.map_or_else(unknown, |m| format!("{m:.4}"));
    egui::Grid::new("analysis_values").num_columns(2).striped(true).show(ui, |ui| {
        ui.label(t(ja, "Chemical formula", "分子式"));
        match polymer {
            Some(p) => {
                ui.vertical(|ui| {
                    ui.add(RichLabel::new(p.rich()));
                    ui.weak(format!(
                        "{} ({})",
                        t(ja, "Values for n = 1", "数値は n = 1 の値"),
                        a.formula.plain()
                    ));
                });
            }
            None => {
                ui.add(RichLabel::new(a.formula.rich()));
            }
        }
        ui.end_row();

        // With `n`, the mass for any n under the n = 1 value.
        let with_n = |ui: &mut egui::Ui, value: Option<f64>, of: fn(&MolecularFormula) -> Option<f64>| {
            match polymer.and_then(|p| per_unit(p, of)) {
                Some(general) => {
                    ui.vertical(|ui| {
                        ui.label(mass(value));
                        ui.weak(general);
                    });
                }
                None => {
                    ui.label(mass(value));
                }
            }
        };
        ui.label(t(ja, "Exact mass", "精密質量"));
        with_n(ui, a.exact_mass, MolecularFormula::exact_mass);
        ui.end_row();

        ui.label(t(ja, "Molecular weight", "分子量"));
        with_n(ui, a.molecular_weight, MolecularFormula::weight);
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

        ui.label(t(ja, "IUPAC name", "IUPAC 名"));
        ui.vertical(|ui| match name {
            NameResult::Ok(n) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(n);
                    let copy = ui
                        .small_button("📋")
                        .on_hover_text(t(ja, "Copy the name", "名前をコピー"));
                    if copy.clicked() {
                        ui.ctx().copy_text(n.clone());
                        copied = true;
                    }
                });
                if stereo {
                    ui.weak(t(ja, "(stereodescriptors not included)", "（立体表記は含まれません）"));
                }
            }
            NameResult::Unsupported(reason) => {
                ui.weak(format!("— ({}: {reason})", t(ja, "unsupported", "未対応")));
            }
        });
        ui.end_row();
    });
    copied
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

    #[test]
    fn name_follows_the_selection() {
        let (m, c1, o) = two_structures();
        let mut p = AnalysisPanel::default();
        assert_eq!(p.iupac_name(&m, &HashSet::new()), &NameResult::Ok("ethene; methanol".into()));
        assert_eq!(p.iupac_name(&m, &[c1].into()), &NameResult::Ok("ethene".into()));
        assert_eq!(p.iupac_name(&m, &[o].into()), &NameResult::Ok("methanol".into()));
    }

    /// Me–[CH2–CH2–O]3–Me: every value is that of CH3(CH2CH2O)3CH3, the end groups included.
    #[test]
    fn a_numbered_repeat_unit_is_analysed_written_out() {
        let m = crate::testkit::glyme("3");
        let mut p = AnalysisPanel::default();
        let a = p.analysis(&m, &HashSet::new()).unwrap().clone();
        assert_eq!(a.formula.plain(), "C8H18O3");
        assert_eq!(format!("{:.3}", a.exact_mass.unwrap()), "162.126");
        assert_eq!(format!("{:.4}", a.isotope_pattern.unwrap()[0].mz), "162.1256");
        let ea: Vec<String> = a.elemental_analysis.unwrap().iter().map(|(el, pct)| format!("{el} {pct:.2}")).collect();
        assert_eq!(ea, ["C 59.23", "H 11.18", "O 29.59"]);
        assert!(PolymerFormula::of(&m).is_none());
        assert!(matches!(p.iupac_name(&m, &HashSet::new()), NameResult::Ok(_)), "named as the written-out chain");
    }

    /// Me–[CH2–CH2–O]n–Me: written C2H6(C2H4O)n, the values those of n = 1, the masses also
    /// per n.
    #[test]
    fn an_n_repeat_unit_keeps_its_end_groups() {
        let m = crate::testkit::glyme("n");
        let mut p = AnalysisPanel::default();
        let a = p.analysis(&m, &HashSet::new()).unwrap().clone();
        assert_eq!(a.formula.plain(), "C4H10O");
        let poly = PolymerFormula::of(&m).unwrap();
        assert_eq!(poly.plain(), "C2H6(C2H4O)n");
        assert_eq!(per_unit(&poly, MolecularFormula::exact_mass).as_deref(), Some("30.0470 + 44.0262 n"));
        assert_eq!(per_unit(&poly, MolecularFormula::weight).as_deref(), Some("30.0700 + 44.0530 n"));
    }

    /// The panel draws for an empty canvas, a structure, a group label, a bad ion choice and
    /// polymers.
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
            (crate::testkit::glyme("n"), AdductChoice::Neutral, 1),
            (crate::testkit::glyme("3"), AdductChoice::Neutral, 1),
        ] {
            p.adduct = adduct;
            p.charge = charge;
            let _ = crate::testkit::run_ui(&ctx, egui::RawInput::default(), |ui| {
                p.ui(ui, &mol, &HashSet::new());
            });
        }
    }
}
