//! Molecular formula (Hill notation) and molecular weight, **derived from the drawn
//! structure**.
//!
//! The hydrogen count is the same one the renderer displays: explicit (folded) H neighbours
//! plus the implicit H that fills the atom's valence, i.e.
//! [`crate::widget::draw::displayed_h_count`]. Formula and picture therefore never disagree —
//! if the structure shows `OH`, the formula counts that H.
//!
//! Element ordering is [Hill notation](https://en.wikipedia.org/wiki/Chemical_formula#Hill_system):
//! carbon first, hydrogen second, everything else alphabetically; with no carbon present,
//! *all* elements go alphabetically (`H2O`, `KI`). That is what ChemDraw prints — which is why
//! potassium carbonate reads `CK2O3` and not `K2CO3`.

use std::collections::BTreeMap;

use crate::molecule::Molecule;
use crate::widget::draw::{displayed_h_count, is_folded_h};
use crate::widget::rich_text::{RichText, SpanStyle};

/// Standard atomic weight in g/mol (IUPAC conventional values, 2021), or `None` for a symbol
/// that is not a plain element — a group label such as `OMe`, `Ts` or `CO2Et`, which the atom
/// editor also allows.
///
/// `D` (deuterium) is included because the editor treats it as its own element symbol.
/// Elements with no standard atomic weight (Tc, Pm, Po–Ac, …) are absent; `Ac` and `Ts` would
/// clash with the acetyl and tosyl group labels anyway.
pub fn atomic_weight(element: &str) -> Option<f64> {
    Some(match element {
        "H" => 1.008,
        "D" => 2.0141,
        "He" => 4.0026,
        "Li" => 6.94,
        "Be" => 9.0122,
        "B" => 10.81,
        "C" => 12.011,
        "N" => 14.007,
        "O" => 15.999,
        "F" => 18.998,
        "Ne" => 20.180,
        "Na" => 22.990,
        "Mg" => 24.305,
        "Al" => 26.982,
        "Si" => 28.085,
        "P" => 30.974,
        "S" => 32.06,
        "Cl" => 35.45,
        "Ar" => 39.95,
        "K" => 39.0983,
        "Ca" => 40.078,
        "Sc" => 44.956,
        "Ti" => 47.867,
        "V" => 50.942,
        "Cr" => 51.996,
        "Mn" => 54.938,
        "Fe" => 55.845,
        "Co" => 58.933,
        "Ni" => 58.693,
        "Cu" => 63.546,
        "Zn" => 65.38,
        "Ga" => 69.723,
        "Ge" => 72.630,
        "As" => 74.922,
        "Se" => 78.971,
        "Br" => 79.904,
        "Kr" => 83.798,
        "Rb" => 85.468,
        "Sr" => 87.62,
        "Y" => 88.906,
        "Zr" => 91.224,
        "Nb" => 92.906,
        "Mo" => 95.95,
        "Ru" => 101.07,
        "Rh" => 102.91,
        "Pd" => 106.42,
        "Ag" => 107.87,
        "Cd" => 112.41,
        "In" => 114.82,
        "Sn" => 118.71,
        "Sb" => 121.76,
        "Te" => 127.60,
        "I" => 126.90447,
        "Xe" => 131.29,
        "Cs" => 132.91,
        "Ba" => 137.33,
        "La" => 138.91,
        "Ce" => 140.12,
        "Pr" => 140.91,
        "Nd" => 144.24,
        "Sm" => 150.36,
        "Eu" => 151.96,
        "Gd" => 157.25,
        "Tb" => 158.93,
        "Dy" => 162.50,
        "Ho" => 164.93,
        "Er" => 167.26,
        "Tm" => 168.93,
        "Yb" => 173.05,
        "Lu" => 174.97,
        "Hf" => 178.49,
        "Ta" => 180.95,
        "W" => 183.84,
        "Re" => 186.21,
        "Os" => 190.23,
        "Ir" => 192.22,
        "Pt" => 195.08,
        "Au" => 196.97,
        "Hg" => 200.59,
        "Tl" => 204.38,
        "Pb" => 207.2,
        "Bi" => 208.98,
        "Th" => 232.04,
        "Pa" => 231.04,
        "U" => 238.03,
        _ => return None,
    })
}

/// A molecular formula: element symbols with their counts, already in Hill order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MolecularFormula {
    /// `(symbol, count)` pairs in Hill order. Counts are never zero.
    pub counts: Vec<(String, usize)>,
}

impl MolecularFormula {
    /// Count the atoms of `mol`, including the hydrogens the renderer displays.
    ///
    /// Folded terminal hydrogens (the `H` of an `OH` label) are counted once, through their
    /// heavy neighbour's displayed H count — never twice.
    pub fn of(mol: &Molecule) -> Self {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for atom in &mol.atoms {
            // Folded H is part of its neighbour's label; `displayed_h_count` already has it.
            if is_folded_h(mol, atom.id) {
                continue;
            }
            *counts.entry(atom.element.clone()).or_default() += 1;
            let h = displayed_h_count(mol, atom.id) as usize;
            if h > 0 {
                *counts.entry("H".to_string()).or_default() += h;
            }
        }

        // Hill order: C, H, then the rest alphabetically — unless there is no carbon, in
        // which case everything (hydrogen included) goes alphabetically.
        let mut out: Vec<(String, usize)> = Vec::with_capacity(counts.len());
        if counts.contains_key("C") {
            for lead in ["C", "H"] {
                if let Some(n) = counts.remove(lead) {
                    out.push((lead.to_string(), n));
                }
            }
        }
        out.extend(counts); // BTreeMap iterates alphabetically
        Self { counts: out }
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Flat text, counts inline: `C6H6O`, `CK2O3`.
    pub fn plain(&self) -> String {
        let mut s = String::new();
        for (el, n) in &self.counts {
            s.push_str(el);
            if *n > 1 {
                s.push_str(&n.to_string());
            }
        }
        s
    }

    /// Styled text with the counts as true subscripts (`C₆H₆O`). `style` is applied to the
    /// element symbols; the counts get the same style plus [`Script::Sub`](crate::Script::Sub).
    pub fn rich_styled(&self, style: SpanStyle) -> RichText {
        let sub = SpanStyle { script: crate::widget::rich_text::Script::Sub, ..style };
        let mut rich = RichText::new();
        for (el, n) in &self.counts {
            rich = rich.span(el.clone(), style);
            if *n > 1 {
                rich = rich.span(n.to_string(), sub);
            }
        }
        rich
    }

    /// Styled text in the default (regular) style.
    pub fn rich(&self) -> RichText {
        self.rich_styled(SpanStyle::default())
    }

    /// Molecular weight in g/mol, or `None` when the structure carries a symbol that is not a
    /// plain element (see [`unknown_elements`](Self::unknown_elements)).
    pub fn weight(&self) -> Option<f64> {
        let mut sum = 0.0;
        for (el, n) in &self.counts {
            sum += atomic_weight(el)? * *n as f64;
        }
        Some(sum)
    }

    /// Symbols with no atomic weight — group labels like `OMe` or `Ts` typed into the editor.
    pub fn unknown_elements(&self) -> Vec<&str> {
        self.counts
            .iter()
            .filter(|(el, _)| atomic_weight(el).is_none())
            .map(|(el, _)| el.as_str())
            .collect()
    }
}

/// Convenience: [`MolecularFormula::of`].
pub fn formula_of(mol: &Molecule) -> MolecularFormula {
    MolecularFormula::of(mol)
}

/// Convenience: molecular weight of a drawn structure, in g/mol.
pub fn molecular_weight(mol: &Molecule) -> Option<f64> {
    MolecularFormula::of(mol).weight()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;
    use crate::reaction::demo;
    use crate::widget::rich_text::Script;

    fn plain(mol: &Molecule) -> String {
        MolecularFormula::of(mol).plain()
    }

    fn mw2(mol: &Molecule) -> String {
        format!("{:.2}", MolecularFormula::of(mol).weight().unwrap())
    }

    /// The four species of the reference figure: Hill formula and MW to two decimals.
    #[test]
    fn figure_species_formulas_and_weights() {
        for (mol, formula, mw) in [
            (demo::phenol(), "C6H6O", "94.11"),
            (demo::methyl_iodide(), "CH3I", "141.94"),
            (demo::potassium_carbonate(), "CK2O3", "138.20"),
            (demo::anisole(), "C7H8O", "108.14"),
        ] {
            assert_eq!(plain(&mol), formula);
            assert_eq!(mw2(&mol), mw, "MW of {formula}");
        }
    }

    /// Hill order: C first, H second, the rest alphabetical — but with no carbon at all,
    /// every element (H included) is ordered alphabetically.
    #[test]
    fn hill_order() {
        // Carbonate: C, then K before O. No hydrogens at all.
        assert_eq!(plain(&demo::potassium_carbonate()), "CK2O3");

        // Carbon-free: water is H2O, but KI/HI order alphabetically.
        let mut m = Molecule::default();
        let o = m.add_atom("O".into(), [0.0, 0.0], 0);
        let h = m.add_atom("H".into(), [1.5, 0.0], 0);
        m.add_bond(o, h, BondOrder::Single);
        assert_eq!(plain(&m), "H2O");

        let mut m = Molecule::default();
        let i = m.add_atom("I".into(), [0.0, 0.0], 0);
        let k = m.add_atom("K".into(), [1.5, 0.0], 0);
        m.add_bond(i, k, BondOrder::Single);
        assert_eq!(plain(&m), "IK", "no carbon → strictly alphabetical");

        // Halides sort alphabetically after H: CHBrClF.
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        for (el, pos) in [("Br", [1.5, 0.0]), ("Cl", [-1.5, 0.0]), ("F", [0.0, 1.5])] {
            let x = m.add_atom(el.into(), pos, 0);
            m.add_bond(c, x, BondOrder::Single);
        }
        assert_eq!(plain(&m), "CHBrClF");
    }

    /// Implicit hydrogens are counted exactly as the renderer displays them, and an
    /// explicitly drawn (folded) H is counted once, not twice.
    #[test]
    fn implicit_hydrogen_counting() {
        // A lone atom carries no implicit H — the editor draws a bare "O", so the formula
        // is "O" and not "H2O". (Consistent with `displayed_h_count`.)
        let mut m = Molecule::default();
        m.add_atom("O".into(), [0.0, 0.0], 0);
        assert_eq!(plain(&m), "O");

        // C-O: methanol drawn as two heavy atoms → CH3 + OH = CH4O.
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        let o = m.add_atom("O".into(), [1.5, 0.0], 0);
        m.add_bond(c, o, BondOrder::Single);
        assert_eq!(plain(&m), "CH4O");

        // The same molecule with the hydroxyl H drawn explicitly must not double-count it.
        let h = m.add_atom("H".into(), [3.0, 0.0], 0);
        m.add_bond(o, h, BondOrder::Single);
        assert_eq!(plain(&m), "CH4O", "a folded H is counted once");

        // Multiple bonds consume valence: acetone C3H6O, acetonitrile C2H3N.
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".into(), [1.5, 0.0], 0);
        let c3 = m.add_atom("C".into(), [3.0, 0.0], 0);
        let o = m.add_atom("O".into(), [1.5, -1.5], 0);
        m.add_bond(c1, c2, BondOrder::Single);
        m.add_bond(c2, c3, BondOrder::Single);
        m.add_bond(c2, o, BondOrder::Double);
        assert_eq!(plain(&m), "C3H6O");

        let mut m = Molecule::default();
        let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".into(), [1.5, 0.0], 0);
        let n = m.add_atom("N".into(), [3.0, 0.0], 0);
        m.add_bond(c1, c2, BondOrder::Single);
        m.add_bond(c2, n, BondOrder::Triple);
        assert_eq!(plain(&m), "C2H3N");

        // Charge shifts the valence the same way the label does: an ammonium N takes 4 H.
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        let n = m.add_atom("N".into(), [1.5, 0.0], 1);
        m.add_bond(c, n, BondOrder::Single);
        assert_eq!(plain(&m), "CH6N", "CH3-NH3+ (charge is not shown in the formula)");

        // ...but a cationic carbon loses one: the ethyl cation CH3-CH2+ is C2H5, not C2H7.
        let mut m = Molecule::default();
        let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
        let c2 = m.add_atom("C".into(), [1.5, 0.0], 1);
        m.add_bond(c1, c2, BondOrder::Single);
        assert_eq!(plain(&m), "C2H5", "CH3-CH2+ (a carbocation has valence 3)");

        // Elements with no standard valence take no implicit H (K in the carbonate).
        let mut m = Molecule::default();
        let k = m.add_atom("K".into(), [0.0, 0.0], 0);
        let o = m.add_atom("O".into(), [1.5, 0.0], 0);
        m.add_bond(k, o, BondOrder::Single);
        assert_eq!(plain(&m), "HKO", "only the O picks up an implicit H");
    }

    #[test]
    fn subscripts_are_real_subscript_spans() {
        let rich = MolecularFormula::of(&demo::phenol()).rich();
        let spans: Vec<(&str, Script)> = rich
            .spans
            .iter()
            .map(|s| (s.text.as_str(), s.style.script))
            .collect();
        assert_eq!(
            spans,
            vec![
                ("C", Script::Normal),
                ("6", Script::Sub),
                ("H", Script::Normal),
                ("6", Script::Sub),
                ("O", Script::Normal),
            ]
        );
        // A count of 1 is not written out.
        let flat: String = MolecularFormula::of(&demo::methyl_iodide())
            .rich()
            .spans
            .iter()
            .map(|s| s.text.as_str())
            .collect();
        assert_eq!(flat, "CH3I");
    }

    /// Group labels (`OMe`, `Ts`, …) have no atomic weight, so the MW is unknown rather
    /// than silently wrong.
    #[test]
    fn group_labels_have_no_weight() {
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        let g = m.add_atom("OMe".into(), [1.5, 0.0], 0);
        m.add_bond(c, g, BondOrder::Single);
        let f = MolecularFormula::of(&m);
        assert_eq!(f.weight(), None);
        assert_eq!(f.unknown_elements(), vec!["OMe"]);
    }

    /// Every element up to uranium that has an IUPAC standard atomic weight has one in the
    /// table — the lanthanides of Lewis acids such as Er(OTf)3 or Dy(OTf)3 included. The
    /// elements skipped here have no standard atomic weight (no stable isotope).
    #[test]
    fn every_element_with_a_standard_weight_has_one() {
        const NO_STANDARD_WEIGHT: [&str; 8] = ["Tc", "Pm", "Po", "At", "Rn", "Fr", "Ra", "Ac"];
        for z in 1..=92 {
            let el = crate::molecule::elements::symbol(z).unwrap();
            if NO_STANDARD_WEIGHT.contains(&el) {
                continue;
            }
            assert!(atomic_weight(el).is_some(), "no atomic weight for {el} (Z = {z})");
        }
        assert_eq!(atomic_weight("Er").map(|w| format!("{w:.2}")).as_deref(), Some("167.26"));
        // `Ac` is the acetyl group label in this editor, `Ts` the tosyl one: never an element.
        assert_eq!(atomic_weight("Ac"), None);
        assert_eq!(atomic_weight("Ts"), None);
    }
}
