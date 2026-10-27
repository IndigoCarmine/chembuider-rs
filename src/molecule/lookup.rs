//! Id lookups over a molecule, for passes that query every atom (scene building, labels).
//!
//! `Molecule::atom_by_id` / `bonds_for_atom` scan the whole atom / bond list, which is right for
//! a single query but makes a per-atom pass quadratic. [`MolIndex`] answers the same queries
//! from maps built once, with exactly the same results (first atom with an id; a bond's
//! appearances in `mol.bonds` order), so code written against [`AtomLookup`] draws the same
//! picture either way.

use std::collections::HashMap;

use super::{Atom, Bond, Molecule};

/// The id lookups the label and scene code needs.
pub(crate) trait AtomLookup {
    fn atom_by_id(&self, id: u32) -> Option<&Atom>;
    fn bonds_for_atom(&self, atom_id: u32) -> Vec<&Bond>;
    fn neighbor_atom_ids(&self, atom_id: u32) -> Vec<u32> {
        self.bonds_for_atom(atom_id)
            .iter()
            .map(|b| if b.begin == atom_id { b.end } else { b.begin })
            .collect()
    }
}

impl AtomLookup for Molecule {
    fn atom_by_id(&self, id: u32) -> Option<&Atom> {
        Molecule::atom_by_id(self, id)
    }
    fn bonds_for_atom(&self, atom_id: u32) -> Vec<&Bond> {
        Molecule::bonds_for_atom(self, atom_id)
    }
}

/// [`AtomLookup`] by hash map: atom index by id and incident bond indices by atom id.
pub(crate) struct MolIndex<'a> {
    mol: &'a Molecule,
    atoms: HashMap<u32, usize>,
    bonds: HashMap<u32, Vec<usize>>,
}

impl<'a> MolIndex<'a> {
    pub(crate) fn new(mol: &'a Molecule) -> Self {
        let mut atoms = HashMap::with_capacity(mol.atoms.len());
        for (i, a) in mol.atoms.iter().enumerate() {
            atoms.entry(a.id).or_insert(i);
        }
        let mut bonds: HashMap<u32, Vec<usize>> = HashMap::with_capacity(mol.atoms.len());
        for (i, b) in mol.bonds.iter().enumerate() {
            bonds.entry(b.begin).or_default().push(i);
            if b.end != b.begin {
                bonds.entry(b.end).or_default().push(i);
            }
        }
        MolIndex { mol, atoms, bonds }
    }
}

impl AtomLookup for MolIndex<'_> {
    fn atom_by_id(&self, id: u32) -> Option<&Atom> {
        self.atoms.get(&id).map(|&i| &self.mol.atoms[i])
    }
    fn bonds_for_atom(&self, atom_id: u32) -> Vec<&Bond> {
        self.bonds
            .get(&atom_id)
            .map(|v| v.iter().map(|&i| &self.mol.bonds[i]).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;

    /// The index answers every query exactly as the scanning `Molecule` does, including a
    /// duplicated atom id, a self-bond, a bond to a missing atom and unknown ids.
    #[test]
    fn index_matches_the_scans() {
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("O".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("N".to_string(), [2.0, 0.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        m.add_bond(c, b, BondOrder::Double);
        m.add_bond(a, c, BondOrder::Triple);
        let mut dup = m.atoms[1].clone();
        dup.element = "S".to_string();
        m.atoms.push(dup);
        let mut odd = m.bonds[0].clone();
        (odd.id, odd.begin, odd.end) = (900, c, c);
        m.bonds.push(odd.clone());
        (odd.id, odd.begin, odd.end) = (901, a, 77);
        m.bonds.push(odd);

        let idx = MolIndex::new(&m);
        for id in [a, b, c, 77, 12345] {
            assert_eq!(
                AtomLookup::atom_by_id(&idx, id).map(|x| x as *const Atom),
                m.atom_by_id(id).map(|x| x as *const Atom),
                "atom {id}"
            );
            let ids = |v: Vec<&Bond>| v.iter().map(|x| x.id).collect::<Vec<_>>();
            assert_eq!(ids(AtomLookup::bonds_for_atom(&idx, id)), ids(m.bonds_for_atom(id)), "bonds {id}");
            assert_eq!(AtomLookup::neighbor_atom_ids(&idx, id), m.neighbor_atom_ids(id), "neighbors {id}");
        }
    }
}
