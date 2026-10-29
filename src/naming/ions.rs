//! Ions and salts: the cations the namer knows by name, and the name of a salt from its ions.
//!
//! A salt is drawn as separate structures, each carrying its charges: "[K+].CCC(=O)[O-]". Each
//! ion is named on its own — a cation from [`CATIONS`], an inorganic anion from the tables of
//! `inorganic.rs` ("chloride", "sulfate"), an organic one by the substitutive engine
//! ("propanoate") — and [`salt`] puts them together (IR-5.4, P-65.6.2.1, P-72): the cations in
//! alphabetical order with their multiplying prefixes, then the anion: "potassium propanoate",
//! "disodium butanedioate", "ammonium potassium hexanedioate", "calcium dichloride",
//! "tricalcium bis(phosphate)". The names are kept in parts ([`IonName`], [`Salt`]) so that a
//! translation can render them.

use crate::molecule::{Molecule, charged_valence};

use super::words::{MAX_NUMERAL, enclose, multiplier, multiplier_complex};

/// A cation named from a table: one atom of `symbol` with `hydrogens` hydrogens and `charge`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Cation {
    pub symbol: &'static str,
    pub hydrogens: u8,
    pub charge: i8,
    pub name: &'static str,
}

const fn cation(symbol: &'static str, hydrogens: u8, charge: i8, name: &'static str) -> Cation {
    Cation {
        symbol,
        hydrogens,
        charge,
        name,
    }
}

/// The cations the namer knows (IR-5.3.2, P-73.1): ammonium and the metals of one common
/// charge — the alkali and alkaline-earth metals, aluminium, zinc and cadmium — whose salts
/// need no oxidation number (IR-5.4.2.2). Their names are the element names
/// (`inorganic::element_name`).
pub(crate) const CATIONS: &[Cation] = &[
    cation("Li", 0, 1, "lithium"),
    cation("Na", 0, 1, "sodium"),
    cation("K", 0, 1, "potassium"),
    cation("Rb", 0, 1, "rubidium"),
    cation("Cs", 0, 1, "caesium"),
    cation("Fr", 0, 1, "francium"),
    cation("Be", 0, 2, "beryllium"),
    cation("Mg", 0, 2, "magnesium"),
    cation("Ca", 0, 2, "calcium"),
    cation("Sr", 0, 2, "strontium"),
    cation("Ba", 0, 2, "barium"),
    cation("Ra", 0, 2, "radium"),
    cation("Al", 0, 3, "aluminium"),
    cation("Zn", 0, 2, "zinc"),
    cation("Cd", 0, 2, "cadmium"),
    cation("N", 4, 1, "ammonium"),
];

/// The one charge of the metal `symbol` of [`CATIONS`], if it is one.
pub(crate) fn fixed_charge(symbol: &str) -> Option<i8> {
    CATIONS
        .iter()
        .find(|c| c.hydrogens == 0 && c.symbol == symbol)
        .map(|c| c.charge)
}

/// The name of the cation of the metal `symbol` of [`CATIONS`] with `charge`.
pub(crate) fn cation_name(symbol: &str, charge: i8) -> &'static str {
    CATIONS
        .iter()
        .find(|c| c.hydrogens == 0 && c.symbol == symbol && c.charge == charge)
        .expect("a cation of the table")
        .name
}

/// The cation of [`CATIONS`] that the connected structure `atoms` (indices into `mol.atoms`)
/// is, if any: one heavy atom with its charge and hydrogens, drawn or implicit.
pub(crate) fn recognise_cation(mol: &Molecule, atoms: &[usize]) -> Option<&'static Cation> {
    let mut heavy = atoms.iter().filter(|&&i| mol.atoms[i].element != "H");
    let &a = heavy.next()?;
    if heavy.next().is_some() {
        return None;
    }
    let atom = &mol.atoms[a];
    let drawn_h = atoms.len() as i16 - 1;
    // Every bond is one of those hydrogens' (a lone hydrogen is a component of its own).
    let bonds = mol
        .bonds
        .iter()
        .filter(|b| b.begin == atom.id || b.end == atom.id)
        .count() as i16;
    if bonds != drawn_h || atoms.iter().any(|&i| i != a && mol.atoms[i].charge != 0) {
        return None;
    }
    // A metal gets no implicit hydrogens (the editor knows no valence for it).
    let implicit = (charged_valence(&atom.element, atom.charge).unwrap_or(0) - bonds).max(0);
    let h = drawn_h + implicit;
    CATIONS
        .iter()
        .find(|c| c.symbol == atom.element && c.charge == atom.charge && c.hydrogens as i16 == h)
}

/// The name of an ion, in parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum IonName {
    /// An ion of the tables ([`CATIONS`], the anions of `inorganic.rs`), by its English name;
    /// a `compound` one is multiplied as "bis(sulfate)", not "disulfate" (IR-5.4.2.1).
    Table { name: &'static str, compound: bool },
    /// An ion named by the substitutive engine ("propanoate").
    Organic(String),
}

impl IonName {
    /// The English name.
    pub(crate) fn english(&self) -> &str {
        match self {
            IonName::Table { name, .. } => name,
            IonName::Organic(name) => name,
        }
    }

    /// `n` times the ion: "acetate", "diacetate", "bis(2-chloropropanoate)", "bis(sulfate)".
    fn multiplied(&self, n: usize) -> String {
        let name = self.english();
        let compound = match self {
            IonName::Table { compound, .. } => *compound,
            IonName::Organic(name) => {
                name.contains(|c: char| c.is_ascii_digit() || c == ' ' || c == '(')
            }
        };
        if n == 1 {
            name.to_string()
        } else if compound {
            format!("{}{}", multiplier_complex(n), enclose(name, 0))
        } else {
            format!("{}{name}", multiplier(n))
        }
    }
}

/// A named ion of a salt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ion {
    pub name: IonName,
    pub charge: i32,
}

/// A salt in parts: its cations in the order of citation, each with its count in the formula
/// unit, and its one kind of anion with its count.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Salt {
    pub cations: Vec<(IonName, usize)>,
    pub anion: (IonName, usize),
}

impl Salt {
    /// The English name: "potassium sodium sulfate", "calcium dichloride".
    pub(crate) fn english(&self) -> String {
        let mut words: Vec<String> = self
            .cations
            .iter()
            .map(|(c, k)| format!("{}{}", multiplier(*k), c.english()))
            .collect();
        words.push(self.anion.0.multiplied(self.anion.1));
        words.join(" ")
    }
}

/// The salt made of `ions` (each drawn ion once): the cations in alphabetical order (IR-5.4.1,
/// P-65.6.2.1), then the anion, each with its count in the smallest formula unit ("disodium
/// butanedioate", "calcium diacetate"). Only one kind of anion is supported, the charges must
/// balance, and no count may exceed the multiplying prefixes ([`MAX_NUMERAL`]).
pub(crate) fn salt(ions: &[Ion]) -> Result<Salt, String> {
    let anions: Vec<&Ion> = ions.iter().filter(|i| i.charge < 0).collect();
    let mut cations: Vec<(IonName, usize)> = Vec::new();
    for i in ions.iter().filter(|i| i.charge > 0) {
        match cations.iter_mut().find(|(n, _)| *n == i.name) {
            Some((_, k)) => *k += 1,
            None => cations.push((i.name.clone(), 1)),
        }
    }
    let total: i32 = ions.iter().map(|i| i.charge).sum();
    if anions.is_empty() || cations.is_empty() || total != 0 {
        return Err(
            "charged species whose charges do not balance (a lone ion, or a salt \
                    with a missing or extra ion)"
                .into(),
        );
    }
    let anion = &anions[0].name;
    if anions.iter().any(|a| a.name != *anion) {
        return Err("salt of more than one kind of anion".into());
    }
    let unit = cations.iter().fold(anions.len(), |g, &(_, k)| gcd(g, k));
    for (_, k) in &mut cations {
        *k /= unit;
    }
    let anion_count = anions.len() / unit;
    if anion_count > MAX_NUMERAL || cations.iter().any(|&(_, k)| k > MAX_NUMERAL) {
        return Err(format!(
            "salt with more than {MAX_NUMERAL} of one ion in its formula unit"
        ));
    }
    cations.sort_by(|a, b| a.0.english().cmp(b.0.english()));
    Ok(Salt {
        cations,
        anion: (anion.clone(), anion_count),
    })
}

/// The English name of the salt made of `ions` ([`salt`]).
pub(crate) fn salt_name(ions: &[Ion]) -> Result<String, String> {
    salt(ions).map(|s| s.english())
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ion(name: &str, charge: i32) -> Ion {
        Ion {
            name: IonName::Organic(name.into()),
            charge,
        }
    }

    /// The cations of the metals are named as their elements.
    #[test]
    fn cation_names_are_the_element_names() {
        for c in CATIONS.iter().filter(|c| c.hydrogens == 0) {
            assert_eq!(
                super::super::inorganic::element_name(c.symbol),
                Some(c.name)
            );
        }
    }

    #[test]
    fn salts() {
        assert_eq!(
            salt_name(&[ion("potassium", 1), ion("propanoate", -1)]),
            Ok("potassium propanoate".into())
        );
        assert_eq!(
            salt_name(&[ion("sodium", 1), ion("butanedioate", -2), ion("sodium", 1)]),
            Ok("disodium butanedioate".into())
        );
        assert_eq!(
            salt_name(&[
                ion("sodium", 1),
                ion("potassium", 1),
                ion("butanedioate", -2)
            ]),
            Ok("potassium sodium butanedioate".into())
        );
        // Two formula units drawn: still one name.
        assert_eq!(
            salt_name(&[
                ion("acetate", -1),
                ion("sodium", 1),
                ion("acetate", -1),
                ion("sodium", 1)
            ]),
            Ok("sodium acetate".into())
        );
        // A divalent cation multiplies the anion (P-65.6.2.1).
        assert_eq!(
            salt_name(&[ion("calcium", 2), ion("acetate", -1), ion("acetate", -1)]),
            Ok("calcium diacetate".into())
        );
        assert_eq!(
            salt_name(&[
                ion("calcium", 2),
                ion("2-chloropropanoate", -1),
                ion("2-chloropropanoate", -1)
            ]),
            Ok("calcium bis(2-chloropropanoate)".into())
        );
        // A count beyond the numerals (9999) is refused, not written in digits.
        let mut many = vec![ion("potassium", 1); 10_000];
        many.push(ion("sodium", 1));
        many.extend(vec![ion("chloride", -1); 10_001]);
        assert!(salt_name(&many).unwrap_err().contains("9999"));
        assert!(salt_name(&[ion("acetate", -1)]).is_err());
        assert!(salt_name(&[ion("sodium", 1)]).is_err());
        assert!(salt_name(&[ion("sodium", 1), ion("butanedioate", -2)]).is_err());
        assert!(
            salt_name(&[
                ion("sodium", 1),
                ion("sodium", 1),
                ion("acetate", -1),
                ion("formate", -1)
            ])
            .is_err()
        );
    }
}
