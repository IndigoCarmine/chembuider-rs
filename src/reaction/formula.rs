//! Molecular formula (Hill notation) and molecular weight, **derived from the drawn
//! structure**.
//!
//! The hydrogen count is the same one the renderer displays: explicit (folded) H neighbours
//! plus the implicit H that fills the atom's valence, i.e.
//! [`crate::molecule::depict::displayed_h_count`]. Formula and picture therefore never disagree —
//! if the structure shows `OH`, the formula counts that H.
//!
//! Element ordering is [Hill notation](https://en.wikipedia.org/wiki/Chemical_formula#Hill_system):
//! carbon first, hydrogen second, everything else alphabetically; with no carbon present,
//! *all* elements go alphabetically (`H2O`, `KI`). That is what ChemDraw prints — which is why
//! potassium carbonate reads `CK2O3` and not `K2CO3`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::molecule::Molecule;
use crate::molecule::bracket::open_ends;
use crate::molecule::depict::{displayed_h_count, is_folded_h};
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MolecularFormula {
    /// `(symbol, count)` pairs in Hill order. Counts are never zero.
    pub counts: Vec<(String, usize)>,
}

impl MolecularFormula {
    /// Count the atoms of `mol`, including the hydrogens the renderer displays.
    ///
    /// Folded terminal hydrogens (the `H` of an `OH` label) are counted once, through their
    /// heavy neighbour's displayed H count — never twice.
    ///
    /// Brackets ([`crate::molecule::bracket`]) count as they read: the atoms of a multiple group
    /// as many times as it repeats; a polymer's repeat unit once — with no hydrogen where a bond
    /// crosses its brackets, and without the bare carbons that only stand for the chain going on
    /// ([`open_ends`]) — so the formula is the unit's, `C2H4` for polyethylene (see
    /// [`PolymerFormula`] for writing it `(C2H4)n`).
    pub fn of(mol: &Molecule) -> Self {
        Self::from_counts(counts_of(mol, |_| true))
    }

    /// A formula from per-element counts, put into Hill order. Zero counts are dropped.
    pub fn from_counts(mut counts: BTreeMap<String, usize>) -> Self {
        counts.retain(|_, n| *n > 0);
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

    /// Read a written formula such as `K2CO3`, `NaHCO3` or `Ca(OH)2` — the text of a reagent
    /// written over an arrow — into Hill order (`CK2O3`). `None` unless the whole string is
    /// a formula of known elements: element symbols (with a weight, see [`atomic_weight`]),
    /// counts, and parenthesised groups with a count. So `cat.`, `rt`, `THF`, `Et3N`, `MeOH`
    /// or `24 h` are not formulas.
    ///
    /// A few reagent abbreviations happen to spell valid element symbols (`NBS` is N-B-S,
    /// `DCC` is D-C-C, `Ar` is argon, not aryl); those are refused rather than given a
    /// meaningless weight.
    pub fn parse(text: &str) -> Option<Self> {
        const ABBREVIATIONS: [&str; 10] =
            ["NBS", "NCS", "NIS", "DCC", "DIC", "PCC", "PDC", "NMO", "Ar", "Pr"];
        if ABBREVIATIONS.contains(&text) {
            return None;
        }
        let chars: Vec<char> = text.chars().collect();
        let mut pos = 0;
        let counts = parse_group(&chars, &mut pos, 0)?;
        if pos != chars.len() || counts.is_empty() {
            return None;
        }
        Some(Self::from_counts(counts))
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

/// How many times atom `id` counts: once, times the repeat count of every multiple group
/// around it.
fn multiplicity(mol: &Molecule, id: u32) -> usize {
    mol.brackets()
        .filter(|b| b.atoms.contains(&id))
        .filter_map(|b| b.repeat_count())
        .fold(1usize, |n, k| n.saturating_mul(k))
}

/// Element counts of the atoms of `mol` that `keep` accepts (each with its hydrogens), the
/// brackets taken into account as [`MolecularFormula::of`] says.
fn counts_of(mol: &Molecule, keep: impl Fn(u32) -> bool) -> BTreeMap<String, usize> {
    let has_brackets = mol.brackets().next().is_some();
    let ends = if has_brackets { open_ends(mol) } else { Default::default() };
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for atom in &mol.atoms {
        // Folded H is part of its neighbour's label; `displayed_h_count` already has it.
        if is_folded_h(mol, atom.id) || ends.contains(&atom.id) || !keep(atom.id) {
            continue;
        }
        let times = if has_brackets { multiplicity(mol, atom.id) } else { 1 };
        *counts.entry(atom.element.clone()).or_default() += times;
        let h = displayed_h_count(mol, atom.id) as usize;
        if h > 0 {
            *counts.entry("H".to_string()).or_default() += h * times;
        }
    }
    counts
}

/// The formula of a drawing with polymer brackets as it is written: what lies outside the
/// repeat units, then each unit in parentheses with its label — `(C2H4)n`, `C2H6(C2H4)n`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PolymerFormula {
    /// Everything outside the repeat units (the bare chain ends left out). Often empty.
    pub rest: MolecularFormula,
    /// Each repeat unit's formula and label (`n`), in page order.
    pub units: Vec<(MolecularFormula, String)>,
}

impl PolymerFormula {
    /// The formula of `mol` written with its repeat units, or `None` when it has no polymer
    /// brackets (its formula is then [`MolecularFormula::of`]'s).
    pub fn of(mol: &Molecule) -> Option<Self> {
        let units: Vec<_> = mol.brackets().filter(|b| b.usage.is_polymer_unit()).collect();
        if units.is_empty() {
            return None;
        }
        let inside: HashSet<u32> = units.iter().flat_map(|b| b.atoms.iter().copied()).collect();
        let rest = MolecularFormula::from_counts(counts_of(mol, |id| !inside.contains(&id)));
        let units = units
            .iter()
            .map(|b| {
                let mine: HashSet<u32> = b.atoms.iter().copied().collect();
                (MolecularFormula::from_counts(counts_of(mol, |id| mine.contains(&id))), b.label.trim().to_string())
            })
            .filter(|(f, _)| !f.is_empty())
            .collect();
        Some(Self { rest, units })
    }

    /// Flat text: `(C2H4)n`.
    pub fn plain(&self) -> String {
        let mut s = self.rest.plain();
        for (f, label) in &self.units {
            s.push_str(&format!("({}){label}", f.plain()));
        }
        s
    }

    /// Styled text, the counts and the labels as subscripts: `(C₂H₄)ₙ`.
    pub fn rich(&self) -> RichText {
        let mut rich = self.rest.rich();
        let sub = SpanStyle { script: crate::widget::rich_text::Script::Sub, ..SpanStyle::default() };
        for (f, label) in &self.units {
            rich = rich.span("(", SpanStyle::default());
            for span in f.rich().spans {
                rich = rich.span(span.text, span.style);
            }
            rich = rich.span(")", SpanStyle::default());
            if !label.is_empty() {
                rich = rich.span(label.clone(), sub);
            }
        }
        rich
    }
}

/// One level of [`MolecularFormula::parse`]: element symbols with counts, and `( … )n`
/// groups, until the end of the text or (when `depth > 0`) the closing parenthesis.
fn parse_group(chars: &[char], pos: &mut usize, depth: usize) -> Option<BTreeMap<String, usize>> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        let group = if c == '(' {
            *pos += 1;
            let inner = parse_group(chars, pos, depth + 1)?;
            if chars.get(*pos) != Some(&')') || inner.is_empty() {
                return None;
            }
            *pos += 1;
            inner
        } else if c == ')' && depth > 0 {
            return Some(counts);
        } else if c.is_ascii_uppercase() {
            // Prefer the two-letter symbol (`Co`, `Cs`) when it is an element; a lowercase
            // letter that does not complete one (`Me`, `Et`) is not a formula.
            let two: Option<String> = chars
                .get(*pos + 1)
                .filter(|n| n.is_ascii_lowercase())
                .map(|n| [c, *n].iter().collect());
            let symbol = match two {
                Some(sym) if atomic_weight(&sym).is_some() => {
                    *pos += 2;
                    sym
                }
                Some(_) => return None,
                None => {
                    let sym = c.to_string();
                    atomic_weight(&sym)?;
                    *pos += 1;
                    sym
                }
            };
            BTreeMap::from([(symbol, 1)])
        } else {
            return None;
        };
        let mut n = 0usize;
        let start = *pos;
        while let Some(d) = chars.get(*pos).and_then(|c| c.to_digit(10)) {
            n = n.checked_mul(10)?.checked_add(d as usize)?;
            *pos += 1;
        }
        let n = if *pos == start { 1 } else { n };
        if n == 0 {
            return None;
        }
        for (el, k) in group {
            *counts.entry(el).or_default() += k * n;
        }
    }
    (depth == 0).then_some(counts)
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

    /// CH2–CH2 in brackets, a bond leaving each side to a bare carbon (the chain going on).
    fn polyethylene() -> (Molecule, Vec<u32>) {
        let mut m = Molecule::default();
        let ids: Vec<u32> =
            (0..4).map(|k| m.add_atom("C".into(), [k as f32 * 1.3, if k % 2 == 0 { 0.0 } else { -0.75 }], 0)).collect();
        for w in ids.windows(2) {
            m.add_bond(w[0], w[1], BondOrder::Single);
        }
        m.add_bracket(&[ids[1], ids[2]].into(), crate::molecule::BracketUsage::Sru);
        (m, ids)
    }

    #[test]
    fn a_repeat_unit_counts_once_with_no_hydrogen_at_its_crossings() {
        let (m, _) = polyethylene();
        assert_eq!(plain(&m), "C2H4");
        let p = PolymerFormula::of(&m).unwrap();
        assert_eq!(p.plain(), "(C2H4)n");
        let rich = p.rich();
        assert_eq!(rich.spans.last().map(|s| (s.text.as_str(), s.style.script)), Some(("n", Script::Sub)));
        assert!(PolymerFormula::of(&demo::phenol()).is_none());
    }

    #[test]
    fn end_groups_stay_outside_the_unit() {
        // ClCH2–[CH2–CH2]n–: the chloromethyl is an end group, the other end stays open.
        let (mut m, ids) = polyethylene();
        let cl = m.add_atom("Cl".into(), [-1.3, -0.75], 0);
        m.add_bond(cl, ids[0], BondOrder::Single);
        assert_eq!(PolymerFormula::of(&m).unwrap().plain(), "CH2Cl(C2H4)n");
        assert_eq!(plain(&m), "C3H6Cl");
    }

    #[test]
    fn a_multiple_group_multiplies() {
        // H3C–[CH2]3–CH3 drawn as one CH2 in brackets ×3: pentane.
        let mut m = Molecule::default();
        let ids: Vec<u32> = (0..3).map(|k| m.add_atom("C".into(), [k as f32 * 1.3, 0.0], 0)).collect();
        m.add_bond(ids[0], ids[1], BondOrder::Single);
        m.add_bond(ids[1], ids[2], BondOrder::Single);
        let id = m.add_bracket(&[ids[1]].into(), crate::molecule::BracketUsage::MultipleGroup).unwrap();
        if let Some(crate::molecule::PageObject::Bracket(b)) = m.object_by_id_mut(id) {
            b.label = "3".into();
        }
        assert_eq!(plain(&m), "C5H12");
        assert!(PolymerFormula::of(&m).is_none());
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

    /// Written formulas read into Hill order; text that is not a formula is refused.
    #[test]
    fn parse_written_formulas() {
        let p = |t: &str| MolecularFormula::parse(t).map(|f| f.plain());
        assert_eq!(p("K2CO3").as_deref(), Some("CK2O3"));
        assert_eq!(p("Cs2CO3").as_deref(), Some("CCs2O3"));
        assert_eq!(p("NaHCO3").as_deref(), Some("CHNaO3"));
        assert_eq!(p("Ca(OH)2").as_deref(), Some("CaH2O2"));
        assert_eq!(p("H2O").as_deref(), Some("H2O"));
        assert_eq!(p("CO").as_deref(), Some("CO"));
        assert_eq!(p("Co").as_deref(), Some("Co"));
        assert_eq!(
            format!("{:.2}", MolecularFormula::parse("K2CO3").unwrap().weight().unwrap()),
            "138.20",
            "same MW as the drawn carbonate"
        );
        for text in ["", "cat.", "rt", "THF", "Et3N", "MeOH", "24", "h", "Ca(OH", "Ca)2", "()", "H0", "NBS", "Ar"] {
            assert_eq!(p(text), None, "{text:?} is not a formula");
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
