//! A minimal SMILES reader, for tests only.
//!
//! Reading SMILES is not a feature of this crate: [`to_smiles`](super::to_smiles) writes, and
//! nothing else. This parser exists so that writing can be *checked* — a string is only correct
//! if an independent reader recovers the molecule from it, and a reader written against the
//! specification rather than against the writer is the closest thing to an independent one that
//! fits in a test module.
//!
//! It deliberately covers only what a valid SMILES string may contain, not what this crate
//! happens to emit: bracket atoms with isotopes, explicit hydrogen counts and charges; the
//! organic subset with its inferred hydrogens; branches; ring closures including `%nn`;
//! disconnected components. Aromatic lowercase atoms and stereo markers are *not* supported and
//! panic loudly — the writer never produces them, and a parser that silently accepted them
//! would hide the day it started to.
//!
//! Hydrogens are counted the way a reader counts them (bracket atoms say, organic-subset atoms
//! infer), never the way the writer counted them, so [`formula`] is a genuinely separate
//! opinion about how many hydrogens the string describes.

use super::{ELEMENT_SYMBOLS, smiles_implicit_h};
use crate::molecule::{BondOrder, Molecule};
use std::collections::{BTreeMap, HashMap};

/// One atom as the string describes it.
struct ReadAtom {
    /// The element label a [`Molecule`] would use — `D`/`T` for hydrogen's isotopes.
    element: String,
    charge: i8,
    /// `Some` for a bracket atom (which states its hydrogens), `None` for the organic subset
    /// (which leaves them to be inferred).
    stated_h: Option<u32>,
    /// The SMILES symbol, needed to infer hydrogens for the organic subset.
    symbol: String,
}

/// Read `s` into a molecule plus the hydrogen count each atom carries.
///
/// Hydrogens are *not* materialised as atoms — that is what makes the result comparable with a
/// drawn structure whose hydrogens were folded away.
pub fn parse(s: &str) -> (Molecule, Vec<u32>) {
    let src: Vec<char> = s.chars().collect();
    let mut atoms: Vec<ReadAtom> = Vec::new();
    let mut bonds: Vec<(usize, usize, BondOrder)> = Vec::new();

    let mut prev: Option<usize> = None;
    let mut pending: Option<BondOrder> = None;
    let mut branches: Vec<Option<usize>> = Vec::new();
    let mut open: HashMap<u8, (usize, Option<BondOrder>)> = HashMap::new();

    let mut i = 0usize;
    while i < src.len() {
        let c = src[i];

        // ── an atom ────────────────────────────────────────────────────────────
        let atom = if c == '[' {
            let close = (i..src.len())
                .find(|&k| src[k] == ']')
                .unwrap_or_else(|| panic!("unterminated bracket atom in {s:?}"));
            let a = bracket_atom(&src[i + 1..close], s);
            i = close + 1;
            Some(a)
        } else if let Some(sym) = organic_symbol(&src[i..]) {
            i += sym.len();
            Some(ReadAtom {
                element: sym.to_string(),
                charge: 0,
                stated_h: None,
                symbol: sym.to_string(),
            })
        } else if c == '*' {
            i += 1;
            Some(ReadAtom {
                element: "*".to_string(),
                charge: 0,
                stated_h: Some(0),
                symbol: "*".to_string(),
            })
        } else {
            None
        };

        if let Some(a) = atom {
            let k = atoms.len();
            atoms.push(a);
            if let Some(p) = prev {
                bonds.push((p, k, pending.take().unwrap_or(BondOrder::Single)));
            }
            pending = None;
            prev = Some(k);
            continue;
        }

        // ── everything else ────────────────────────────────────────────────────
        match c {
            '-' => pending = Some(BondOrder::Single),
            '=' => pending = Some(BondOrder::Double),
            '#' => pending = Some(BondOrder::Triple),
            '$' => pending = Some(BondOrder::Quadruple),
            '(' => branches.push(prev),
            ')' => prev = branches.pop().unwrap_or_else(|| panic!("unbalanced ')' in {s:?}")),
            '.' => prev = None,
            '%' | '0'..='9' => {
                let (n, width) = if c == '%' {
                    let d = |k: usize| {
                        src.get(k)
                            .and_then(|c| c.to_digit(10))
                            .unwrap_or_else(|| panic!("malformed %nn ring closure in {s:?}"))
                    };
                    ((d(i + 1) * 10 + d(i + 2)) as u8, 3)
                } else {
                    (c.to_digit(10).expect("matched a digit") as u8, 1)
                };
                let here = prev.unwrap_or_else(|| panic!("ring closure before any atom in {s:?}"));
                match open.remove(&n) {
                    Some((there, opened_with)) => {
                        let order = pending.take().or(opened_with).unwrap_or(BondOrder::Single);
                        bonds.push((there, here, order));
                    }
                    None => {
                        open.insert(n, (here, pending.take()));
                    }
                }
                i += width;
                continue;
            }
            ':' | '/' | '\\' | '@' | 'a'..='z' => panic!(
                "the test parser does not accept aromatic or stereo notation ({c:?} in {s:?}); \
                 to_smiles is not supposed to write any",
            ),
            other => panic!("unexpected character {other:?} in {s:?}"),
        }
        i += 1;
    }

    assert!(open.is_empty(), "unclosed ring bond in {s:?}");
    assert!(branches.is_empty(), "unclosed branch in {s:?}");

    // ── assemble ───────────────────────────────────────────────────────────────
    let mut mol = Molecule::default();
    let ids: Vec<u32> = atoms
        .iter()
        .enumerate()
        .map(|(k, a)| mol.add_atom(a.element.clone(), [k as f32, 0.0], a.charge))
        .collect();
    let mut bond_sum = vec![0i16; atoms.len()];
    for (a, b, order) in &bonds {
        let v = match order {
            BondOrder::Single => 1,
            BondOrder::Double => 2,
            BondOrder::Triple => 3,
            BondOrder::Quadruple => 4,
        };
        bond_sum[*a] += v;
        bond_sum[*b] += v;
        mol.add_bond(ids[*a], ids[*b], order.clone());
    }

    let h = atoms
        .iter()
        .enumerate()
        .map(|(k, a)| a.stated_h.unwrap_or_else(|| smiles_implicit_h(&a.symbol, bond_sum[k])))
        .collect();

    (mol, h)
}

/// The molecular formula `s` describes, hydrogens included.
pub fn formula(s: &str) -> BTreeMap<String, u32> {
    let (mol, h) = parse(s);
    let mut f: BTreeMap<String, u32> = BTreeMap::new();
    for a in &mol.atoms {
        *f.entry(a.element.clone()).or_default() += 1;
    }
    let total: u32 = h.iter().sum();
    if total > 0 {
        *f.entry("H".to_string()).or_default() += total;
    }
    f.retain(|_, v| *v > 0);
    f
}

/// An organic-subset atom at the head of `rest`, if there is one. Two-letter symbols first, or
/// `Cl` would be read as a carbon followed by nothing legal.
fn organic_symbol(rest: &[char]) -> Option<&'static str> {
    let two: String = rest.iter().take(2).collect();
    match two.as_str() {
        "Cl" => return Some("Cl"),
        "Br" => return Some("Br"),
        _ => {}
    }
    match rest.first() {
        Some('B') => Some("B"),
        Some('C') => Some("C"),
        Some('N') => Some("N"),
        Some('O') => Some("O"),
        Some('P') => Some("P"),
        Some('S') => Some("S"),
        Some('F') => Some("F"),
        Some('I') => Some("I"),
        _ => None,
    }
}

/// The inside of a `[...]`: `isotope? symbol H<count>? charge?`.
fn bracket_atom(body: &[char], whole: &str) -> ReadAtom {
    let mut i = 0usize;

    let mut isotope = 0u16;
    let mut had_isotope = false;
    while let Some(d) = body.get(i).and_then(|c| c.to_digit(10)) {
        isotope = isotope * 10 + d as u16;
        had_isotope = true;
        i += 1;
    }

    let symbol: String = if body.get(i) == Some(&'*') {
        i += 1;
        "*".to_string()
    } else {
        let first = *body
            .get(i)
            .unwrap_or_else(|| panic!("bracket atom with no element in {whole:?}"));
        assert!(first.is_ascii_uppercase(), "bad element {first:?} in {whole:?}");
        i += 1;
        // A two-letter symbol has a lowercase second letter, which is exactly what tells `Si`
        // from the `N` of `[NH4+]`.
        match body.get(i) {
            Some(c) if c.is_ascii_lowercase() => {
                let two: String = [first, *c].iter().collect();
                if ELEMENT_SYMBOLS.contains(&two.as_str()) {
                    i += 1;
                    two
                } else {
                    first.to_string()
                }
            }
            _ => first.to_string(),
        }
    };

    let mut stated_h = 0u32;
    if body.get(i) == Some(&'H') {
        i += 1;
        let mut n = 0u32;
        let mut digits = false;
        while let Some(d) = body.get(i).and_then(|c| c.to_digit(10)) {
            n = n * 10 + d;
            digits = true;
            i += 1;
        }
        stated_h = if digits { n } else { 1 };
    }

    let mut charge = 0i32;
    while let Some(&sign) = body.get(i) {
        let step = match sign {
            '+' => 1,
            '-' => -1,
            other => panic!("unexpected {other:?} in bracket atom of {whole:?}"),
        };
        i += 1;
        let mut n = 0i32;
        let mut digits = false;
        while let Some(d) = body.get(i).and_then(|c| c.to_digit(10)) {
            n = n * 10 + d as i32;
            digits = true;
            i += 1;
        }
        charge += step * if digits { n } else { 1 };
    }
    assert_eq!(i, body.len(), "trailing junk in bracket atom of {whole:?}");

    // SMILES writes hydrogen's isotopes as a mass number; the editor writes them as element
    // labels, so they have to change back on the way in or a deuteration would look like an
    // ordinary hydrogen.
    let element = match (symbol.as_str(), had_isotope.then_some(isotope)) {
        ("H", Some(2)) => "D".to_string(),
        ("H", Some(3)) => "T".to_string(),
        _ => symbol.clone(),
    };

    ReadAtom {
        element,
        charge: charge as i8,
        stated_h: Some(stated_h),
        symbol,
    }
}
