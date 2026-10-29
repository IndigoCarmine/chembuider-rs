//! Compositional names of inorganic compounds (IUPAC 2005, the Red Book): elements, the
//! anions of the tables, and salts drawn with their metal bonded to the anion.
//!
//! A structure is the namer's here when it has a metal, or no carbon, or is one of the
//! carbon-containing anions of the tables (carbonate, hydrogencarbonate, cyanide); everything
//! else goes to the substitutive engine. What comes out is kept in parts — an element and its
//! count, ions of the tables by their English names — so that a translation can render them.
//!
//! - An element (IR-3.4.3, IR-5.2): every atom of one element and no hydrogen, the charges
//!   drawn summing to zero (ozone as O=O⁺–O⁻), named with the multiplying prefix of the atoms
//!   ("trioxygen", "octasulfur"); one atom is the bare element name ("argon", "iron").
//! - An anion of [`ANIONS`] or [`OXOANIONS`] (IR-5.3.3, Table IR-8.1, and the hydrogen names of
//!   IR-8.5, a list the Red Book asks to be read as limiting): recognised by its atoms, their
//!   bonds and hydrogens and its net charge, wherever the drawing puts the charges.
//! - A metal of the cation table bonded to anions ("Na–Cl", "Cl–Ca–Cl"): the metals have one
//!   charge each, which is the number of their single bonds, so the structure is the salt of
//!   that cation and of the anions left when the bonds are cut. A compositional name says
//!   nothing of the bonding (IR-5.2). The anions must be of the tables: a metal bonded to an
//!   organic structure may be an organometallic compound (CH₃–Li, CH₃–Mg–Br), and a salt of
//!   an organic anion is to be drawn as ions.
//!
//! Refused, with the reason: metals with more than one common oxidation state (the transition
//! metals other than zinc and cadmium, tin, lead, thallium, the lanthanoids and actinoids…),
//! whose salts need an oxidation or charge number ("iron(II) chloride", IR-5.4.2.2), and their
//! coordination compounds (IR-9); a cation-table metal with another charge or number of bonds;
//! ions outside the tables; and molecular compounds other than elements — hydrides (water,
//! ammonia, hydrogen chloride), oxoacids and binary compounds of nonmetals, whose names need the
//! parent hydrides of IR-6 or the element sequence of Table VI.

use crate::molecule::charged_valence;
use crate::molecule::elements::atomic_number;

use super::graph::Raw;
use super::ions::{self, Ion, IonName};

/// The English names of the elements (IR-3, Table I), by atomic number.
#[rustfmt::skip]
const NAMES: [&str; 118] = [
    "hydrogen", "helium", "lithium", "beryllium", "boron", "carbon", "nitrogen", "oxygen",
    "fluorine", "neon", "sodium", "magnesium", "aluminium", "silicon", "phosphorus", "sulfur",
    "chlorine", "argon", "potassium", "calcium", "scandium", "titanium", "vanadium", "chromium",
    "manganese", "iron", "cobalt", "nickel", "copper", "zinc", "gallium", "germanium", "arsenic",
    "selenium", "bromine", "krypton", "rubidium", "strontium", "yttrium", "zirconium", "niobium",
    "molybdenum", "technetium", "ruthenium", "rhodium", "palladium", "silver", "cadmium",
    "indium", "tin", "antimony", "tellurium", "iodine", "xenon", "caesium", "barium",
    "lanthanum", "cerium", "praseodymium", "neodymium", "promethium", "samarium", "europium",
    "gadolinium", "terbium", "dysprosium", "holmium", "erbium", "thulium", "ytterbium",
    "lutetium", "hafnium", "tantalum", "tungsten", "rhenium", "osmium", "iridium", "platinum",
    "gold", "mercury", "thallium", "lead", "bismuth", "polonium", "astatine", "radon",
    "francium", "radium", "actinium", "thorium", "protactinium", "uranium", "neptunium",
    "plutonium", "americium", "curium", "berkelium", "californium", "einsteinium", "fermium",
    "mendelevium", "nobelium", "lawrencium", "rutherfordium", "dubnium", "seaborgium",
    "bohrium", "hassium", "meitnerium", "darmstadtium", "roentgenium", "copernicium",
    "nihonium", "flerovium", "moscovium", "livermorium", "tennessine", "oganesson",
];

/// The English name of the element `symbol` ("sulfur", "caesium"), if it is one.
pub(crate) fn element_name(symbol: &str) -> Option<&'static str> {
    atomic_number(symbol).map(|z| NAMES[z as usize - 1])
}

/// The nonmetals and metalloids; every other element is a metal.
#[rustfmt::skip]
const NONMETALS: &[&str] = &[
    "H", "He", "B", "C", "N", "O", "F", "Ne", "Si", "P", "S", "Cl", "Ar", "Ge", "As", "Se", "Br",
    "Kr", "Sb", "Te", "I", "Xe", "At", "Rn", "Ts", "Og",
];

fn is_metal(symbol: &str) -> bool {
    atomic_number(symbol).is_some() && !NONMETALS.contains(&symbol)
}

/// An anion of the tables other than an oxoanion: its atoms (element and hydrogens on it, the
/// first bonded to each of the others), its bonds' orders, its charge and name.
struct Anion {
    atoms: &'static [(&'static str, u8)],
    order: u8,
    charge: i8,
    name: &'static str,
    /// Multiplied as "bis(peroxide)", not "diperoxide" (IR-5.4.2.1).
    compound: bool,
}

const fn anion(atoms: &'static [(&'static str, u8)], charge: i8, name: &'static str) -> Anion {
    Anion {
        atoms,
        order: 1,
        charge,
        name,
        compound: false,
    }
}

/// The monoatomic anions (IR-5.3.3.2, with the charges their elements take in salts), the
/// hydroxide ion, and the homo- and heteropolyatomic anions of Table IX the namer knows.
const ANIONS: &[Anion] = &[
    anion(&[("H", 0)], -1, "hydride"),
    anion(&[("F", 0)], -1, "fluoride"),
    anion(&[("Cl", 0)], -1, "chloride"),
    anion(&[("Br", 0)], -1, "bromide"),
    anion(&[("I", 0)], -1, "iodide"),
    anion(&[("O", 0)], -2, "oxide"),
    anion(&[("S", 0)], -2, "sulfide"),
    anion(&[("Se", 0)], -2, "selenide"),
    anion(&[("Te", 0)], -2, "telluride"),
    anion(&[("N", 0)], -3, "nitride"),
    anion(&[("P", 0)], -3, "phosphide"),
    anion(&[("As", 0)], -3, "arsenide"),
    anion(&[("O", 1)], -1, "hydroxide"),
    Anion {
        order: 3,
        ..anion(&[("C", 0), ("N", 0)], -1, "cyanide")
    },
    Anion {
        compound: true,
        ..anion(&[("O", 0), ("O", 0)], -2, "peroxide")
    },
    // N=N⁺=N⁻ or N⁻–N⁺≡N: the central nitrogen bonded to both ends, bond orders as drawn.
    Anion {
        order: 0,
        compound: true,
        ..anion(&[("N", 0), ("N", 0), ("N", 0)], -1, "azide")
    },
];

/// An oxoanion (Table IR-8.1; the hydrogen names of IR-8.5): a central atom bonded to
/// terminal oxygens only, `hydrogens` of them carrying a hydrogen.
struct Oxoanion {
    central: &'static str,
    oxygens: u8,
    hydrogens: u8,
    charge: i8,
    name: &'static str,
}

const fn oxo(
    central: &'static str,
    oxygens: u8,
    hydrogens: u8,
    charge: i8,
    name: &'static str,
) -> Oxoanion {
    Oxoanion {
        central,
        oxygens,
        hydrogens,
        charge,
        name,
    }
}

const OXOANIONS: &[Oxoanion] = &[
    oxo("C", 3, 0, -2, "carbonate"),
    oxo("C", 3, 1, -1, "hydrogencarbonate"),
    oxo("N", 3, 0, -1, "nitrate"),
    oxo("N", 2, 0, -1, "nitrite"),
    oxo("P", 4, 0, -3, "phosphate"),
    oxo("P", 4, 1, -2, "hydrogenphosphate"),
    oxo("P", 4, 2, -1, "dihydrogenphosphate"),
    oxo("S", 4, 0, -2, "sulfate"),
    oxo("S", 4, 1, -1, "hydrogensulfate"),
    oxo("S", 3, 0, -2, "sulfite"),
    oxo("S", 3, 1, -1, "hydrogensulfite"),
    oxo("Se", 4, 0, -2, "selenate"),
    oxo("Se", 3, 0, -2, "selenite"),
    oxo("Cl", 4, 0, -1, "perchlorate"),
    oxo("Cl", 3, 0, -1, "chlorate"),
    oxo("Cl", 2, 0, -1, "chlorite"),
    oxo("Cl", 1, 0, -1, "hypochlorite"),
    oxo("Br", 4, 0, -1, "perbromate"),
    oxo("Br", 3, 0, -1, "bromate"),
    oxo("Br", 2, 0, -1, "bromite"),
    oxo("Br", 1, 0, -1, "hypobromite"),
    oxo("I", 4, 0, -1, "periodate"),
    oxo("I", 3, 0, -1, "iodate"),
    oxo("I", 2, 0, -1, "iodite"),
    oxo("I", 1, 0, -1, "hypoiodite"),
];

/// What the namer makes of an inorganic structure.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Inorganic {
    /// A neutral element.
    Element(Element),
    /// Ions of a salt: one anion of the tables, or a salt drawn with bonds.
    Ions(Vec<Ion>),
}

/// An element: `atoms` atoms of the element `name`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Element {
    pub name: &'static str,
    pub atoms: usize,
}

impl Element {
    /// The English name: "trioxygen", "argon".
    pub(crate) fn english(&self) -> String {
        format!("{}{}", super::words::multiplier(self.atoms), self.name)
    }
}

/// One structure's atoms as the inorganic namer sees them: every atom, hydrogens included,
/// with the hydrogens its valence adds.
struct Atoms<'a> {
    raw: &'a Raw,
    implicit: Vec<u8>,
    nbrs: Vec<Vec<(usize, u8)>>,
}

impl<'a> Atoms<'a> {
    fn new(raw: &'a Raw) -> Self {
        let n = raw.label.len();
        let mut nbrs = vec![Vec::new(); n];
        for &(a, b, o) in &raw.bonds {
            nbrs[a].push((b, o));
            nbrs[b].push((a, o));
        }
        let implicit = (0..n)
            .map(|i| {
                let bonds: i16 = nbrs[i].iter().map(|&(_, o)| o as i16).sum();
                // A metal or noble gas gets none (the editor knows no valence for it).
                (charged_valence(&raw.label[i], raw.charge[i]).unwrap_or(0) - bonds).max(0) as u8
            })
            .collect();
        Atoms {
            raw,
            implicit,
            nbrs,
        }
    }

    fn el(&self, i: usize) -> &str {
        &self.raw.label[i]
    }

    fn is_h(&self, i: usize) -> bool {
        self.el(i) == "H"
    }

    /// The hydrogens on atom `i`: its implicit ones and the hydrogen atoms bonded to it.
    fn hydrogens(&self, i: usize, part: &[usize]) -> usize {
        self.implicit[i] as usize
            + self.nbrs[i]
                .iter()
                .filter(|&&(j, _)| part.contains(&j) && self.is_h(j))
                .count()
    }
}

/// The inorganic name of the connected structure `raw`, if it is the inorganic namer's:
/// `None` hands it to the substitutive engine.
pub(crate) fn name(raw: &Raw) -> Option<Result<Inorganic, String>> {
    if raw.label.iter().any(|l| l == super::graph::ATTACHMENT) {
        return None;
    }
    let atoms = Atoms::new(raw);
    let all: Vec<usize> = (0..raw.label.len()).collect();
    let has_carbon = raw.label.iter().any(|l| l == "C");
    if raw.label.iter().any(|l| is_metal(l)) {
        return Some(with_metals(&atoms));
    }
    let charge: i32 = raw.charge.iter().map(|&q| q as i32).sum();
    if has_carbon {
        // Only the anions of the tables: neutral carbon compounds and the other anions are
        // organic.
        return (charge != 0)
            .then(|| table_anion(&atoms, &all, charge))
            .flatten()
            .map(|ion| Ok(Inorganic::Ions(vec![ion])));
    }
    Some(without_metals(&atoms, &all, charge))
}

/// A structure of nonmetals other than carbon: an element or an anion of the tables.
fn without_metals(atoms: &Atoms, all: &[usize], charge: i32) -> Result<Inorganic, String> {
    if let [h] = all
        && atoms.is_h(*h)
    {
        return match charge {
            -1 => Ok(Inorganic::Ions(vec![table_anion(atoms, all, -1).unwrap()])),
            1 => Err("hydron (H\u{207a}): acids drawn as ions are not supported".into()),
            _ => Err(
                "a lone hydrogen atom (by its valence H\u{2082}: draw dihydrogen as H\u{2013}H)"
                    .into(),
            ),
        };
    }
    if charge != 0 {
        return match table_anion(atoms, all, charge) {
            Some(ion) => Ok(Inorganic::Ions(vec![ion])),
            None => Err(format!(
                "{} not in the table of inorganic ions",
                if charge > 0 { "cation" } else { "anion" }
            )),
        };
    }
    let first = atoms.el(0);
    let hydrogens: usize = all.iter().map(|&i| atoms.implicit[i] as usize).sum();
    if hydrogens == 0 && all.iter().all(|&i| atoms.el(i) == first) {
        let name = element_name(first).expect("an element label");
        return Ok(Inorganic::Element(Element {
            name,
            atoms: all.len(),
        }));
    }
    Err(
        "no carbon atom, and molecular compounds other than elements (hydrides such as water, \
         ammonia and hydrogen chloride, oxoacids, binary compounds of nonmetals) are not \
         supported"
            .into(),
    )
}

/// A structure with a metal: a lone atom of it, or a cation-table metal bonded to the anions of
/// its salt.
fn with_metals(atoms: &Atoms) -> Result<Inorganic, String> {
    let n = atoms.raw.label.len();
    if n == 1 {
        let (el, q) = (atoms.el(0), atoms.raw.charge[0]);
        if q == 0 {
            let name = element_name(el).expect("a metal is an element");
            return Ok(Inorganic::Element(Element { name, atoms: 1 }));
        }
        // A cation of the table was recognised before.
        return Err(match ions::fixed_charge(el) {
            Some(c) => format!(
                "{el} with the charge {q:+}: its ion is {el}{}",
                charge_text(c)
            ),
            None => several_oxidation_states(el),
        });
    }
    let metals: Vec<usize> = (0..n).filter(|&i| is_metal(atoms.el(i))).collect();
    let mut ions = Vec::new();
    for &m in &metals {
        let el = atoms.el(m);
        let Some(charge) = ions::fixed_charge(el) else {
            let ligand = atoms.raw.charge[m] != 0
                || atoms.nbrs[m]
                    .iter()
                    .any(|&(j, _)| atoms.nbrs[j].iter().any(|&(k, _)| k != m && !atoms.is_h(k)));
            return Err(if ligand {
                format!(
                    "coordination compound of {} (IR-9: ligands, and an oxidation number), not \
                     supported",
                    element_name(el).unwrap()
                )
            } else {
                several_oxidation_states(el)
            });
        };
        let bonds = &atoms.nbrs[m];
        if atoms.raw.charge[m] != 0
            || bonds.iter().any(|&(j, o)| o != 1 || is_metal(atoms.el(j)))
            || bonds.len() != charge as usize
        {
            return Err(format!(
                "{el} drawn with other bonds than its ion {el}{} has (one single bond to a \
                 nonmetal per charge)",
                charge_text(charge)
            ));
        }
        ions.push(Ion {
            name: IonName::Table {
                name: ions::cation_name(el, charge),
                compound: false,
            },
            charge: charge as i32,
        });
    }
    // The anions: what is left once the metals' bonds are cut, each with one negative charge
    // more per bond cut.
    let mut seen = vec![false; n];
    for &m in &metals {
        seen[m] = true;
    }
    for start in 0..n {
        if seen[start] {
            continue;
        }
        let mut part = vec![start];
        seen[start] = true;
        let mut k = 0;
        while k < part.len() {
            for &(j, _) in &atoms.nbrs[part[k]] {
                if !seen[j] {
                    seen[j] = true;
                    part.push(j);
                }
            }
            k += 1;
        }
        let cut = part
            .iter()
            .map(|&i| {
                atoms.nbrs[i]
                    .iter()
                    .filter(|&&(j, _)| is_metal(atoms.el(j)))
                    .count()
            })
            .sum::<usize>() as i32;
        let charge = part
            .iter()
            .map(|&i| atoms.raw.charge[i] as i32)
            .sum::<i32>()
            - cut;
        match table_anion(atoms, &part, charge) {
            Some(ion) if charge < 0 => ions.push(ion),
            _ => {
                return Err(
                    "metal bonded to a structure other than an inorganic anion of the \
                            tables (an organometallic compound, or a salt: draw a salt as \
                            separate ions)"
                        .into(),
                );
            }
        }
    }
    Ok(Inorganic::Ions(ions))
}

/// The refusal for a metal of more than one common oxidation state.
fn several_oxidation_states(el: &str) -> String {
    format!(
        "compound of {}, a metal of more than one oxidation state: its salts need an oxidation \
         number or a charge number (iron(II) chloride, iron(2+) chloride; IR-5.4.2.2), not \
         supported",
        element_name(el).unwrap_or(el)
    )
}

/// A charge as a superscript: "⁺", "²⁺", "³⁻".
fn charge_text(q: i8) -> String {
    const DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
    let n = q.unsigned_abs();
    let digits = if n == 1 {
        String::new()
    } else {
        n.to_string()
            .chars()
            .map(|d| DIGITS[d.to_digit(10).unwrap() as usize])
            .collect()
    };
    format!("{digits}{}", if q > 0 { '⁺' } else { '⁻' })
}

/// The anion of the tables that the atoms `part` with the net charge `charge` are, if any.
fn table_anion(atoms: &Atoms, part: &[usize], charge: i32) -> Option<Ion> {
    let heavy: Vec<usize> = part
        .iter()
        .copied()
        .filter(|&i| !atoms.is_h(i) || part.len() == 1)
        .collect();
    // Every hydrogen atom bonded to one heavy atom of the part, by a single bond.
    let h_ok = part.iter().all(|&i| {
        !atoms.is_h(i)
            || part.len() == 1
            || atoms.nbrs[i]
                .iter()
                .filter(|&&(j, _)| part.contains(&j))
                .count()
                == 1
                && atoms.nbrs[i]
                    .iter()
                    .all(|&(j, o)| !part.contains(&j) || (o == 1 && !atoms.is_h(j)))
    });
    if !h_ok {
        return None;
    }
    let bonds_in = |i: usize| -> Vec<(usize, u8)> {
        atoms.nbrs[i]
            .iter()
            .copied()
            .filter(|&(j, _)| heavy.contains(&j))
            .collect()
    };
    let hydrogens = |i: usize| {
        if atoms.is_h(i) {
            0
        } else {
            atoms.hydrogens(i, part)
        }
    };
    let table = |name: &'static str, compound: bool| Ion {
        name: IonName::Table { name, compound },
        charge,
    };
    for a in ANIONS {
        if a.charge as i32 != charge || a.atoms.len() != heavy.len() {
            continue;
        }
        // The first atom of the entry is bonded to all the others.
        let matches = |first: usize| {
            let (el, h) = a.atoms[0];
            if atoms.el(first) != el || hydrogens(first) != h as usize {
                return false;
            }
            let mut rest: Vec<usize> = heavy.iter().copied().filter(|&i| i != first).collect();
            let all_bonded = rest.iter().all(|&i| {
                let b = bonds_in(i);
                b.len() == 1 && b[0].0 == first && (a.order == 0 || b[0].1 == a.order)
            });
            if !all_bonded || bonds_in(first).len() != rest.len() {
                return false;
            }
            a.atoms[1..].iter().all(|&(el, h)| {
                match rest
                    .iter()
                    .position(|&i| atoms.el(i) == el && hydrogens(i) == h as usize)
                {
                    Some(k) => {
                        rest.swap_remove(k);
                        true
                    }
                    None => false,
                }
            })
        };
        if heavy.iter().any(|&f| matches(f)) {
            return Some(table(a.name, a.compound));
        }
    }
    // An oxoanion: one central atom, every other heavy atom an oxygen bonded to it alone.
    let (central, oxygens): (Vec<usize>, Vec<usize>) = heavy
        .iter()
        .partition(|&&i| atoms.el(i) != "O" || heavy.len() == 1);
    let &[c] = central.as_slice() else {
        return None;
    };
    if hydrogens(c) != 0
        || bonds_in(c).len() != oxygens.len()
        || oxygens
            .iter()
            .any(|&o| bonds_in(o).len() != 1 || bonds_in(o)[0].0 != c || hydrogens(o) > 1)
    {
        return None;
    }
    let h: usize = oxygens.iter().map(|&o| hydrogens(o)).sum();
    OXOANIONS
        .iter()
        .find(|x| {
            x.central == atoms.el(c)
                && x.oxygens as usize == oxygens.len()
                && x.hydrogens as usize == h
                && x.charge as i32 == charge
        })
        .map(|x| table(x.name, true))
}
