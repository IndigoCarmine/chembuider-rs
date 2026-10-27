//! ChemDraw-style "Analysis" values of a drawn structure: chemical formula, exact mass,
//! molecular weight, the isotope pattern (m/z) and the elemental analysis.
//!
//! Everything starts from the [`MolecularFormula`] the structure shows (implicit hydrogens
//! included, see [`formula`](super::formula)), so the values never disagree with the
//! picture. Average quantities (molecular weight, elemental analysis) use the standard atomic
//! weights of [`atomic_weight`]; isotope-resolved ones (exact mass, m/z) use the natural
//! isotopic compositions of [`isotopes`].
//!
//! The formula carries no charge, so no electron-mass correction is applied: the masses are
//! those of the neutral formula, as ChemDraw prints them.

use crate::molecule::Molecule;

use super::formula::{MolecularFormula, atomic_weight};
use super::ion::{Ion, IonError};
use super::isotopes::isotopes;

/// Smallest peak, in % of the base peak, that [`FormulaAnalysis::of`] reports.
pub const REPORT_MIN_RELATIVE: f64 = 0.1;

/// Peaks closer than this (in u) are one peak: below the four decimals the report prints.
const MERGE_TOLERANCE: f64 = 1e-4;

/// While convolving, peaks below this fraction of the current largest one are dropped.
const PRUNE_RELATIVE: f64 = 1e-6;

/// One line of an isotope pattern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IsotopePeak {
    /// Mass of the isotopologue(s), in u (= m/z for a singly charged ion).
    pub mz: f64,
    /// Intensity in % of the most intense peak.
    pub relative: f64,
}

impl MolecularFormula {
    /// Monoisotopic ("exact") mass: every atom as its most abundant isotope. `None` when the
    /// formula holds a group label (see [`unknown_elements`](Self::unknown_elements)).
    pub fn exact_mass(&self) -> Option<f64> {
        let mut sum = 0.0;
        for (el, n) in &self.counts {
            let (mass, _) = isotopes(el)?.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1))?;
            sum += mass * *n as f64;
        }
        Some(sum)
    }

    /// The isotope pattern, most intense peak first, keeping the peaks of at least
    /// `min_relative` % of the base peak.
    ///
    /// Fine structure stays resolved: ¹³C and ¹⁵N (or ¹³C and D) isotopologues are separate
    /// peaks. Only peaks closer than 0.0001 u — the same mass at the printed precision — are
    /// merged, at their abundance-weighted mean mass.
    pub fn isotope_pattern(&self, min_relative: f64) -> Option<Vec<IsotopePeak>> {
        if self.counts.is_empty() {
            return Some(Vec::new());
        }
        let mut dist: Vec<(f64, f64)> = vec![(0.0, 1.0)];
        for (el, n) in &self.counts {
            let iso = isotopes(el)?;
            for _ in 0..*n {
                dist = convolve(&dist, iso);
            }
        }
        let base = dist.iter().map(|&(_, p)| p).fold(0.0, f64::max);
        let mut peaks: Vec<IsotopePeak> = dist
            .into_iter()
            .map(|(mz, p)| IsotopePeak { mz, relative: p / base * 100.0 })
            .filter(|peak| peak.relative >= min_relative)
            .collect();
        peaks.sort_by(|a, b| b.relative.total_cmp(&a.relative).then(a.mz.total_cmp(&b.mz)));
        Some(peaks)
    }

    /// Mass percentage of each element, in Hill order: what a combustion analysis should find.
    /// `None` when the formula holds a group label.
    pub fn elemental_analysis(&self) -> Option<Vec<(String, f64)>> {
        let total = self.weight()?;
        Some(
            self.counts
                .iter()
                .map(|(el, n)| (el.clone(), atomic_weight(el).unwrap() * *n as f64 / total * 100.0))
                .collect(),
        )
    }
}

/// Add one atom with isotopes `iso` to the distribution `dist` (`(mass, probability)`, sorted
/// by mass), merging coinciding masses and pruning negligible peaks.
fn convolve(dist: &[(f64, f64)], iso: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut next: Vec<(f64, f64)> = dist
        .iter()
        .flat_map(|&(m, p)| iso.iter().map(move |&(im, ip)| (m + im, p * ip)))
        .collect();
    next.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(next.len());
    for (m, p) in next {
        match merged.last_mut() {
            Some((lm, lp)) if m - *lm < MERGE_TOLERANCE => {
                *lm = (*lm * *lp + m * p) / (*lp + p);
                *lp += p;
            }
            _ => merged.push((m, p)),
        }
    }
    let max = merged.iter().map(|&(_, p)| p).fold(0.0, f64::max);
    merged.retain(|&(_, p)| p >= max * PRUNE_RELATIVE);
    merged
}

/// The full set of Analysis values of one structure.
#[derive(Clone, Debug, PartialEq)]
pub struct FormulaAnalysis {
    pub formula: MolecularFormula,
    /// Monoisotopic mass, see [`MolecularFormula::exact_mass`].
    pub exact_mass: Option<f64>,
    /// Average molecular weight in g/mol, see [`MolecularFormula::weight`].
    pub molecular_weight: Option<f64>,
    /// The ion the m/z line is for; `None` for the neutral molecule.
    pub ion: Option<Ion>,
    /// Peaks of at least [`REPORT_MIN_RELATIVE`] %, most intense first — of [`ion`](Self::ion)
    /// when one is set, else of the neutral molecule.
    pub isotope_pattern: Result<Vec<IsotopePeak>, IonError>,
    /// Mass % per element, in Hill order.
    pub elemental_analysis: Option<Vec<(String, f64)>>,
}

impl FormulaAnalysis {
    /// Analyse the structure as drawn.
    pub fn of(mol: &Molecule) -> Self {
        Self::of_formula(MolecularFormula::of(mol))
    }

    /// Analyse the structure as drawn, with the m/z line for `ion` (`[M+Na]+`, …). Formula,
    /// exact mass, MW and elemental analysis stay those of the neutral molecule.
    pub fn with_ion(mol: &Molecule, ion: Ion) -> Self {
        Self::of_formula_with_ion(MolecularFormula::of(mol), Some(ion))
    }

    /// Analyse a formula directly.
    pub fn of_formula(formula: MolecularFormula) -> Self {
        Self::of_formula_with_ion(formula, None)
    }

    /// Analyse a formula directly, with the m/z line for `ion` when one is given.
    pub fn of_formula_with_ion(formula: MolecularFormula, ion: Option<Ion>) -> Self {
        let isotope_pattern = match &ion {
            Some(ion) => formula.ion_pattern(ion, REPORT_MIN_RELATIVE),
            None => formula.isotope_pattern(REPORT_MIN_RELATIVE).ok_or_else(|| {
                IonError::UnknownElement(formula.unknown_elements().join(", "))
            }),
        };
        Self {
            exact_mass: formula.exact_mass(),
            molecular_weight: formula.weight(),
            isotope_pattern,
            elemental_analysis: formula.elemental_analysis(),
            formula,
            ion,
        }
    }

    /// The values as ChemDraw's Analysis window words them, one per line:
    ///
    /// ```text
    /// Chemical formula: C2H4
    /// Exact mass: 28.0313
    /// Molecular weight: 28.0540
    /// m/z: 28.0313 (100.0%), 29.0347 (2.2%)
    /// Elemental analysis: C, 85.63; H, 14.37
    /// ```
    ///
    /// With an [`ion`](Self::ion) the m/z line names it: `m/z ([M+Na]+): 51.0205 (100.0%), …`.
    /// A value that cannot be computed (a group label in the structure) reads
    /// `— (unknown: OMe)`.
    pub fn report(&self) -> String {
        let unknown = format!("— (unknown: {})", self.formula.unknown_elements().join(", "));
        let mass = |m: Option<f64>| m.map_or_else(|| unknown.clone(), |m| format!("{m:.4}"));
        let mz = self.isotope_pattern.as_ref().map_or_else(
            |err| format!("— ({err})"),
            |peaks| {
                peaks
                    .iter()
                    .map(|p| format!("{:.4} ({:.1}%)", p.mz, p.relative))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        );
        let ea = self.elemental_analysis.as_ref().map_or_else(
            || unknown.clone(),
            |ea| {
                ea.iter()
                    .map(|(el, pct)| format!("{el}, {pct:.2}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
        );
        format!(
            "Chemical formula: {}\nExact mass: {}\nMolecular weight: {}\nm/z{}: {}\nElemental analysis: {}",
            self.formula.plain(),
            mass(self.exact_mass),
            mass(self.molecular_weight),
            self.ion.as_ref().map_or_else(String::new, |ion| format!(" ({ion})")),
            mz,
            ea,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;
    use crate::reaction::demo;

    fn formula(counts: &[(&str, usize)]) -> MolecularFormula {
        MolecularFormula { counts: counts.iter().map(|&(el, n)| (el.to_string(), n)).collect() }
    }

    /// Ethylene drawn as C=C reproduces ChemDraw's Analysis window line for line.
    #[test]
    fn ethylene_report_matches_chemdraw() {
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".into(), [1.5, 0.0], 0);
        m.add_bond(c1, c2, BondOrder::Double);
        assert_eq!(
            FormulaAnalysis::of(&m).report(),
            "Chemical formula: C2H4\n\
             Exact mass: 28.0313\n\
             Molecular weight: 28.0540\n\
             m/z: 28.0313 (100.0%), 29.0347 (2.2%)\n\
             Elemental analysis: C, 85.63; H, 14.37"
        );
    }

    #[test]
    fn phenol_exact_mass_and_elemental_analysis() {
        let a = FormulaAnalysis::of(&demo::phenol());
        assert_eq!(format!("{:.4}", a.exact_mass.unwrap()), "94.0419");
        let ea: Vec<String> = a
            .elemental_analysis
            .unwrap()
            .iter()
            .map(|(el, pct)| format!("{el} {pct:.2}"))
            .collect();
        assert_eq!(ea, ["C 76.57", "H 6.43", "O 17.00"]);
    }

    /// Bromine's two isotopes give the M / M+2 doublet; the ¹³C peak sits apart at M+1.
    #[test]
    fn bromobenzene_pattern() {
        let peaks = formula(&[("C", 6), ("H", 5), ("Br", 1)]).isotope_pattern(0.1).unwrap();
        let fmt = |p: &IsotopePeak| format!("{:.4} ({:.1}%)", p.mz, p.relative);
        assert_eq!(fmt(&peaks[0]), "155.9575 (100.0%)");
        assert_eq!(fmt(&peaks[1]), "157.9554 (97.3%)");
        assert!(peaks.iter().any(|p| format!("{:.4}", p.mz) == "156.9608"));
    }

    /// Fine structure is not merged: the ¹³C and ¹⁵N isotopologues of acetonitrile are two
    /// peaks 0.006 u apart.
    #[test]
    fn fine_structure_stays_resolved() {
        let f = formula(&[("C", 2), ("H", 3), ("N", 1)]);
        let mono = f.exact_mass().unwrap();
        let peaks = f.isotope_pattern(0.1).unwrap();
        let near = |dm: f64| peaks.iter().any(|p| (p.mz - mono - dm).abs() < 1e-4);
        assert!(near(13.00335483507 - 12.0), "13C peak");
        assert!(near(15.00010889888 - 14.00307400443), "15N peak");
    }

    #[test]
    fn deuterium_is_its_own_isotope() {
        let f = formula(&[("C", 1), ("D", 4)]);
        assert_eq!(format!("{:.4}", f.exact_mass().unwrap()), "20.0564");
        assert_eq!(f.isotope_pattern(0.1).unwrap()[0].mz, f.exact_mass().unwrap());
    }

    /// A group label makes the isotope-dependent values unknown, without panicking.
    #[test]
    fn group_labels_are_unknown() {
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        let g = m.add_atom("OMe".into(), [1.5, 0.0], 0);
        m.add_bond(c, g, BondOrder::Single);
        let a = FormulaAnalysis::of(&m);
        assert_eq!(a.exact_mass, None);
        assert_eq!(a.isotope_pattern, Err(IonError::UnknownElement("OMe".into())));
        assert_eq!(a.elemental_analysis, None);
        assert!(a.report().contains("Exact mass: — (unknown: OMe)"));
        assert!(a.report().contains("m/z: — (unknown: OMe)"));
    }

    /// With an ion chosen, the m/z line is that ion's; the rest stays the neutral molecule's.
    #[test]
    fn report_for_an_ion() {
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".into(), [1.5, 0.0], 0);
        m.add_bond(c1, c2, BondOrder::Double);
        let ion = Ion::simple(crate::reaction::ion::Adduct::sodium(), 1).unwrap();
        let report = FormulaAnalysis::with_ion(&m, ion).report();
        assert!(report.contains("Exact mass: 28.0313\n"), "{report}");
        assert!(report.contains("\nm/z ([M+Na]+): 51.0205 (100.0%), 52.0239 (2.2%)\n"), "{report}");
    }
}
