//! Ions for mass spectrometry: which species an ESI (or similar) spectrum shows the molecule
//! as — `[M+H]+`, `[M+Na]+`, `[M+2H]2+`, `[M-H]-`, `[2M+Na]+`, `[M+H+Na]2+`, `[M]+•`.
//!
//! An [`Ion`] is `n` molecules plus a list of [`Adduct`]s, each with a signed count: a
//! negative count *removes* the adduct, so a deprotonation `[M-H]-` is the proton taken
//! away once and a radical cation `[M]+•` is the electron taken away once. The charge
//! follows from the adducts; [`Ion::simple`] goes the other way, from a chosen adduct and
//! charge to the count.
//!
//! m/z values subtract the electrons the charge stands for ([`ELECTRON_MASS`]) and divide by
//! |z|, so a doubly charged ion's isotope peaks sit half a unit apart.

use std::collections::BTreeMap;
use std::fmt;

use super::formula::MolecularFormula;
use super::isotopes::isotopes;
use super::properties::IsotopePeak;

/// Electron rest mass in u (CODATA 2018).
pub const ELECTRON_MASS: f64 = 0.000548579909;

/// Why an ion cannot be computed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IonError {
    /// The adducts add up to zero charge: nothing to put on a mass axis.
    Neutral,
    /// [`Ion::simple`]: the charge is no whole multiple of the adduct's charge.
    NotDivisible { adduct: String, charge: i32 },
    /// More of this element is removed than the ion has.
    MissingAtoms(String),
    /// A symbol with no isotope data — a group label such as `OMe`.
    UnknownElement(String),
    /// [`Adduct::custom`]: the text is no formula.
    BadFormula(String),
}

impl fmt::Display for IonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IonError::Neutral => write!(f, "the ion has no net charge"),
            IonError::NotDivisible { adduct, charge } => {
                write!(f, "charge {charge} is no whole number of {adduct}")
            }
            IonError::MissingAtoms(el) => write!(f, "removes more {el} than there is"),
            IonError::UnknownElement(el) => write!(f, "unknown: {el}"),
            IonError::BadFormula(text) => write!(f, "not a formula: {text}"),
        }
    }
}

impl std::error::Error for IonError {}

/// One adduct species: what it adds to the molecule and what charge it carries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adduct {
    /// Name as written in the ion notation (`H`, `Na`, `NH4`, `ACN`).
    pub name: String,
    /// Atoms the adduct adds, per unit.
    pub atoms: Vec<(String, usize)>,
    /// Charge per unit.
    pub charge: i32,
}

impl Adduct {
    fn preset(name: &str, atoms: &[(&str, usize)], charge: i32) -> Self {
        Self {
            name: name.to_string(),
            atoms: atoms.iter().map(|&(el, n)| (el.to_string(), n)).collect(),
            charge,
        }
    }

    /// H⁺. Taken away (count −1) it is the deprotonation of `[M-H]-`.
    pub fn proton() -> Self {
        Self::preset("H", &[("H", 1)], 1)
    }
    pub fn sodium() -> Self {
        Self::preset("Na", &[("Na", 1)], 1)
    }
    pub fn potassium() -> Self {
        Self::preset("K", &[("K", 1)], 1)
    }
    pub fn lithium() -> Self {
        Self::preset("Li", &[("Li", 1)], 1)
    }
    pub fn ammonium() -> Self {
        Self::preset("NH4", &[("N", 1), ("H", 4)], 1)
    }
    pub fn chloride() -> Self {
        Self::preset("Cl", &[("Cl", 1)], -1)
    }
    pub fn formate() -> Self {
        Self::preset("HCOO", &[("C", 1), ("H", 1), ("O", 2)], -1)
    }
    pub fn acetate() -> Self {
        Self::preset("CH3COO", &[("C", 2), ("H", 3), ("O", 2)], -1)
    }
    /// e⁻, no atoms. Taken away (count −1) it makes the radical cation `[M]+•`.
    pub fn electron() -> Self {
        Self::preset("e", &[], -1)
    }

    /// The preset species, for a selection list.
    pub fn presets() -> Vec<Self> {
        vec![
            Self::proton(),
            Self::sodium(),
            Self::potassium(),
            Self::lithium(),
            Self::ammonium(),
            Self::chloride(),
            Self::formate(),
            Self::acetate(),
            Self::electron(),
        ]
    }

    /// Any other species, from a formula such as `C2H3N` (acetonitrile) and its charge — `0`
    /// for a neutral solvent adduct, which then takes a charged one beside it.
    pub fn custom(name: &str, formula: &str, charge: i32) -> Result<Self, IonError> {
        Ok(Self { name: name.to_string(), atoms: parse_formula(formula)?, charge })
    }

    fn is_electron(&self) -> bool {
        self.atoms.is_empty()
    }
}

/// `C2H3N` → `[(C, 2), (H, 3), (N, 1)]`: element symbols, each with an optional count.
fn parse_formula(text: &str) -> Result<Vec<(String, usize)>, IonError> {
    let bad = || IonError::BadFormula(text.to_string());
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut chars = text.trim().chars().peekable();
    if chars.peek().is_none() {
        return Err(bad());
    }
    while let Some(c) = chars.next() {
        if !c.is_ascii_uppercase() {
            return Err(bad());
        }
        let mut symbol = c.to_string();
        while let Some(&l) = chars.peek().filter(|l| l.is_ascii_lowercase()) {
            symbol.push(l);
            chars.next();
        }
        let mut digits = String::new();
        while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
            digits.push(d);
            chars.next();
        }
        let n = if digits.is_empty() { 1 } else { digits.parse().map_err(|_| bad())? };
        if n == 0 {
            return Err(bad());
        }
        *counts.entry(symbol).or_default() += n;
    }
    Ok(counts.into_iter().collect())
}

/// `n` molecules and the adducts that make them an ion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ion {
    molecules: usize,
    adducts: Vec<(i32, Adduct)>,
}

impl Default for Ion {
    fn default() -> Self {
        Self::new()
    }
}

impl Ion {
    /// One molecule, no adducts yet (`[M]`, still neutral).
    pub fn new() -> Self {
        Self { molecules: 1, adducts: Vec::new() }
    }

    /// The common case: one adduct species and the charge to reach with it.
    /// `(proton, 2)` is `[M+2H]2+`, `(proton, -1)` is `[M-H]-`, `(electron, 1)` is `[M]+•`.
    pub fn simple(adduct: Adduct, charge: i32) -> Result<Self, IonError> {
        if charge == 0 {
            return Err(IonError::Neutral);
        }
        if adduct.charge == 0 || charge % adduct.charge != 0 {
            return Err(IonError::NotDivisible { adduct: adduct.name, charge });
        }
        let count = charge / adduct.charge;
        Ok(Self::new().with(adduct, count))
    }

    /// `n` molecules in the ion (`[2M+Na]+`); at least one.
    pub fn multimer(mut self, n: usize) -> Self {
        self.molecules = n.max(1);
        self
    }

    /// Add `count` of `adduct` (negative: take it away). Repeating a species adds up.
    pub fn with(mut self, adduct: Adduct, count: i32) -> Self {
        match self.adducts.iter_mut().find(|(_, a)| *a == adduct) {
            Some((n, _)) => *n += count,
            None => self.adducts.push((count, adduct)),
        }
        self.adducts.retain(|(n, _)| *n != 0);
        self
    }

    pub fn molecules(&self) -> usize {
        self.molecules
    }

    pub fn adducts(&self) -> &[(i32, Adduct)] {
        &self.adducts
    }

    /// Net charge z.
    pub fn charge(&self) -> i32 {
        self.adducts.iter().map(|(n, a)| n * a.charge).sum()
    }

    /// Conventional notation: `[M+H]+`, `[2M+Na]+`, `[M+2H]2+`, `[M-H]-`, `[M+H+Na]2+`,
    /// and `[M]+•` when the charge comes from electrons alone.
    pub fn notation(&self) -> String {
        let mut s = String::from("[");
        if self.molecules > 1 {
            s.push_str(&self.molecules.to_string());
        }
        s.push('M');
        for (n, a) in self.adducts.iter().filter(|(_, a)| !a.is_electron()) {
            s.push(if *n > 0 { '+' } else { '-' });
            if n.abs() > 1 {
                s.push_str(&n.abs().to_string());
            }
            s.push_str(&a.name);
        }
        s.push(']');
        let z = self.charge();
        if z.abs() > 1 {
            s.push_str(&z.abs().to_string());
        }
        if z != 0 {
            s.push(if z > 0 { '+' } else { '-' });
        }
        if self.adducts.iter().any(|(_, a)| a.is_electron()) {
            s.push('•');
        }
        s
    }
}

impl fmt::Display for Ion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.notation())
    }
}

impl MolecularFormula {
    /// Elemental formula of `ion`: this formula times the multimer count, plus the adducts.
    pub fn ion_formula(&self, ion: &Ion) -> Result<MolecularFormula, IonError> {
        let mut counts: BTreeMap<String, i64> = BTreeMap::new();
        for (el, n) in &self.counts {
            *counts.entry(el.clone()).or_default() += (*n * ion.molecules) as i64;
        }
        for (k, adduct) in &ion.adducts {
            for (el, n) in &adduct.atoms {
                *counts.entry(el.clone()).or_default() += *k as i64 * *n as i64;
            }
        }
        let mut out = BTreeMap::new();
        for (el, n) in counts {
            if n < 0 {
                return Err(IonError::MissingAtoms(el));
            }
            if n > 0 && isotopes(&el).is_none() {
                return Err(IonError::UnknownElement(el));
            }
            out.insert(el, n as usize);
        }
        Ok(MolecularFormula::from_counts(out))
    }

    /// Monoisotopic m/z of `ion`.
    pub fn ion_mz(&self, ion: &Ion) -> Result<f64, IonError> {
        let z = nonzero_charge(ion)?;
        let mass = self.ion_formula(ion)?.exact_mass().expect("ion_formula checked the elements");
        Ok(mz(mass, z))
    }

    /// Isotope pattern of `ion` on the m/z axis, most intense peak first, keeping peaks of at
    /// least `min_relative` % (see [`isotope_pattern`](Self::isotope_pattern)). The adducts'
    /// own isotopes (³⁷Cl, ⁴¹K, …) are part of it.
    pub fn ion_pattern(&self, ion: &Ion, min_relative: f64) -> Result<Vec<IsotopePeak>, IonError> {
        let z = nonzero_charge(ion)?;
        let peaks = self
            .ion_formula(ion)?
            .isotope_pattern(min_relative)
            .expect("ion_formula checked the elements");
        Ok(peaks.into_iter().map(|p| IsotopePeak { mz: mz(p.mz, z), ..p }).collect())
    }
}

fn nonzero_charge(ion: &Ion) -> Result<i32, IonError> {
    match ion.charge() {
        0 => Err(IonError::Neutral),
        z => Ok(z),
    }
}

/// m/z of an ion of neutral-atom mass `mass` and charge `z`: the charge's electrons removed
/// (or added) and the mass shared over |z|.
fn mz(mass: f64, z: i32) -> f64 {
    (mass - z as f64 * ELECTRON_MASS) / z.unsigned_abs() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formula(counts: &[(&str, usize)]) -> MolecularFormula {
        MolecularFormula { counts: counts.iter().map(|&(el, n)| (el.to_string(), n)).collect() }
    }

    fn ethylene() -> MolecularFormula {
        formula(&[("C", 2), ("H", 4)])
    }

    fn phenol() -> MolecularFormula {
        formula(&[("C", 6), ("H", 6), ("O", 1)])
    }

    fn mz4(f: &MolecularFormula, ion: &Ion) -> String {
        format!("{:.4}", f.ion_mz(ion).unwrap())
    }

    #[test]
    fn single_adduct_ions() {
        let simple = |a, z| Ion::simple(a, z).unwrap();
        assert_eq!(mz4(&ethylene(), &simple(Adduct::proton(), 1)), "29.0386");
        assert_eq!(mz4(&ethylene(), &simple(Adduct::sodium(), 1)), "51.0205");
        assert_eq!(mz4(&ethylene(), &simple(Adduct::electron(), 1)), "28.0308");
        assert_eq!(mz4(&ethylene(), &simple(Adduct::chloride(), -1)), "63.0007");
        assert_eq!(mz4(&phenol(), &simple(Adduct::proton(), -1)), "93.0346");
    }

    #[test]
    fn notation() {
        let simple = |a, z| Ion::simple(a, z).unwrap().notation();
        assert_eq!(simple(Adduct::proton(), 1), "[M+H]+");
        assert_eq!(simple(Adduct::proton(), 2), "[M+2H]2+");
        assert_eq!(simple(Adduct::proton(), -1), "[M-H]-");
        assert_eq!(simple(Adduct::electron(), 1), "[M]+•");
        assert_eq!(simple(Adduct::formate(), -1), "[M+HCOO]-");
        assert_eq!(Ion::new().multimer(2).with(Adduct::sodium(), 1).notation(), "[2M+Na]+");
    }

    /// The adduct's own isotopes show: [M+Cl]- has the ³⁷Cl peak two units up at ~32 %.
    #[test]
    fn chloride_adduct_carries_its_isotopes() {
        let ion = Ion::simple(Adduct::chloride(), -1).unwrap();
        let peaks = ethylene().ion_pattern(&ion, 0.1).unwrap();
        assert_eq!(format!("{:.4}", peaks[0].mz), "63.0007");
        let cl37 = peaks.iter().find(|p| (p.mz - 64.9978).abs() < 1e-3).expect("37Cl peak");
        assert!((31.5..32.5).contains(&cl37.relative), "{}", cl37.relative);
    }

    #[test]
    fn multimer() {
        let ion = Ion::new().multimer(2).with(Adduct::proton(), 1);
        assert_eq!(mz4(&ethylene(), &ion), "57.0699");
        assert_eq!(ethylene().ion_formula(&ion).unwrap().plain(), "C4H9");
    }

    /// A doubly charged ion: m/z halves, and so does the isotope spacing.
    #[test]
    fn doubly_charged() {
        let ion = Ion::simple(Adduct::proton(), 2).unwrap();
        assert_eq!(mz4(&phenol(), &ion), "48.0282");
        let peaks = phenol().ion_pattern(&ion, 0.1).unwrap();
        assert!(((peaks[1].mz - peaks[0].mz) - 0.5017).abs() < 1e-3);
    }

    #[test]
    fn mixed_adducts() {
        let ion = Ion::new().with(Adduct::proton(), 1).with(Adduct::sodium(), 1);
        assert_eq!(ion.charge(), 2);
        assert_eq!(ion.notation(), "[M+H+Na]2+");
        // (28.0313 + 1.00783 + 22.98977 - 2 mₑ) / 2
        assert_eq!(mz4(&ethylene(), &ion), "26.0139");
    }

    #[test]
    fn custom_adduct() {
        let acn = Adduct::custom("ACN", "C2H3N", 0).unwrap();
        let ion = Ion::new().with(acn, 1).with(Adduct::proton(), 1);
        assert_eq!(ion.notation(), "[M+ACN+H]+");
        assert_eq!(ethylene().ion_formula(&ion).unwrap().plain(), "C4H8N");
        for bad in ["", "c2", "C0", "C2-H"] {
            assert_eq!(Adduct::custom("x", bad, 1), Err(IonError::BadFormula(bad.to_string())));
        }
    }

    #[test]
    fn errors() {
        assert_eq!(Ion::simple(Adduct::proton(), 0), Err(IonError::Neutral));
        assert_eq!(
            Ion::simple(Adduct::custom("X", "C", 2).unwrap(), 3),
            Err(IonError::NotDivisible { adduct: "X".into(), charge: 3 })
        );
        let no_na = Ion::new().with(Adduct::sodium(), -1);
        assert_eq!(ethylene().ion_mz(&no_na), Err(IonError::MissingAtoms("Na".into())));
        // Adding and removing the same adduct cancels out to a neutral [M].
        let neutral = Ion::new().with(Adduct::proton(), 1).with(Adduct::proton(), -1);
        assert_eq!(ethylene().ion_mz(&neutral), Err(IonError::Neutral));
        let labelled = formula(&[("C", 1), ("H", 3), ("OMe", 1)]);
        let ion = Ion::simple(Adduct::proton(), 1).unwrap();
        assert_eq!(labelled.ion_mz(&ion), Err(IonError::UnknownElement("OMe".into())));
    }
}
