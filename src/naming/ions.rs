//! Ions and salts: the cations the namer knows by name, and the name of a salt from its ions.
//!
//! A salt is drawn as separate structures, each carrying its charges: "[K+].CCC(=O)[O-]". Each
//! ion is named on its own — a cation from [`CATIONS`], an anion by the substitutive engine
//! ("propanoate") — and [`salt_name`] puts them together (P-65.6.2.1, P-72): the cations in
//! alphabetical order with their multiplying prefixes, then the anion: "potassium propanoate",
//! "disodium butanedioate", "ammonium potassium hexanedioate".

use crate::molecule::{Molecule, charged_valence};

use super::words::{enclose, multiplier, multiplier_complex};

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

/// The cations the namer knows: the alkali metals and ammonium (P-73.1, IUPAC 2005 IR-5.2).
pub(crate) const CATIONS: &[Cation] = &[
    cation("Li", 0, 1, "lithium"),
    cation("Na", 0, 1, "sodium"),
    cation("K", 0, 1, "potassium"),
    cation("Rb", 0, 1, "rubidium"),
    cation("Cs", 0, 1, "caesium"),
    cation("N", 4, 1, "ammonium"),
];

/// Whether `symbol` is the element of a monatomic cation of [`CATIONS`] (a metal the namer
/// accepts only as a lone ion).
pub(crate) fn is_cation_element(symbol: &str) -> bool {
    CATIONS
        .iter()
        .any(|c| c.hydrogens == 0 && c.symbol == symbol)
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

/// A named ion of a salt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ion {
    pub name: String,
    pub charge: i32,
}

/// The name of the salt made of `ions` (each drawn ion once): the cations in alphabetical
/// order, then the anion, each with the multiplying prefix of its count in the smallest
/// formula unit ("disodium butanedioate", "calcium diacetate"). Only one kind of anion is
/// supported, and the charges must balance.
pub(crate) fn salt_name(ions: &[Ion]) -> Result<String, String> {
    let anions: Vec<&Ion> = ions.iter().filter(|i| i.charge < 0).collect();
    let mut cations: Vec<(&str, usize)> = Vec::new();
    for i in ions.iter().filter(|i| i.charge > 0) {
        match cations.iter_mut().find(|(n, _)| *n == i.name) {
            Some((_, k)) => *k += 1,
            None => cations.push((&i.name, 1)),
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
    cations.sort_unstable();
    let mut words: Vec<String> = cations
        .iter()
        .map(|&(n, k)| format!("{}{n}", multiplier(k / unit)))
        .collect();
    words.push(multiplied_name(anion, anions.len() / unit));
    Ok(words.join(" "))
}

/// `n` times the ion `name`: "acetate", "diacetate", "bis(2-chloropropanoate)".
fn multiplied_name(name: &str, n: usize) -> String {
    if n == 1 {
        name.to_string()
    } else if name.contains(|c: char| c.is_ascii_digit() || c == ' ' || c == '(') {
        format!("{}{}", multiplier_complex(n), enclose(name, 0))
    } else {
        format!("{}{name}", multiplier(n))
    }
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ion(name: &str, charge: i32) -> Ion {
        Ion {
            name: name.into(),
            charge,
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
        // A divalent cation (not yet in the table) multiplies the anion (P-65.6.2.1).
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
