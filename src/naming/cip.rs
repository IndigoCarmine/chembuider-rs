//! CIP priority rules (P-92): the ranking of the ligands of a stereogenic centre or of an end of
//! a double bond, by atomic number over the hierarchical digraph (Rule 1a, P-92.1).
//!
//! The digraph is grown from the root atom along every path that does not revisit an atom. A
//! ring closure ends in a duplicate of the atom it closes on, and a multiple bond adds a
//! duplicate of each of its atoms to the other (P-92.1: C=O counts as C(O)(O) and O(C)).
//! Duplicates carry only phantom atoms (atomic number 0) further out. Two branches are compared
//! sphere by sphere; in each sphere the sets of atoms are compared in the order of the branches
//! they come from, the higher-ranked branch first (P-92.1), each set from its highest atom
//! down.
//!
//! Not implemented, and reported as a tie: Rule 1b (duplicate atoms' distance to the root), Rule 2
//! (mass numbers: isotopes are refused by the namer anyway) and Rules 3–5 (ligands that differ
//! only in their own stereo configuration: pseudoasymmetric centres, like/unlike pairs).
//!
//! The ranking only needs atomic numbers, hydrogen counts and bonds, so it works on any
//! structure given in that form ([`Atoms`]).

use std::cmp::Ordering;

/// A structure as the CIP rules see it.
pub(crate) struct Atoms<'a> {
    /// Atomic number of each atom.
    pub z: Vec<u8>,
    /// The hydrogens on each atom (not drawn as atoms of `adj`).
    pub h: &'a [u8],
    /// Neighbours with the bond order to each.
    pub adj: &'a [Vec<(usize, u8)>],
}

/// A ligand of the root atom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ligand {
    Atom(usize),
    H,
}

/// The digraph is abandoned past this many nodes (the ranking is then undecided).
const MAX_NODES: usize = 50_000;

/// Why a ranking was not found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unranked {
    /// Two ligands are alike under Rule 1a: the indices into the ligands given.
    Tie(usize, usize),
    /// The digraph grew too large to explore.
    TooLarge,
}

/// The order of `ligands` of `root` from the highest-ranked to the lowest (indices into
/// `ligands`).
pub(crate) fn rank(atoms: &Atoms, root: usize, ligands: &[Ligand]) -> Result<Vec<usize>, Unranked> {
    let mut d = Digraph::default();
    let mut tops = Vec::new();
    for &l in ligands {
        tops.push(match l {
            Ligand::H => d.push(1, None, NO_NODE)?,
            Ligand::Atom(a) => {
                let order = atoms.adj[root].iter().find(|e| e.0 == a).map_or(1, |e| e.1);
                d.grow(atoms, root, a, order)?
            }
        });
    }
    d.sort_branches();
    let mut order: Vec<usize> = (0..ligands.len()).collect();
    order.sort_by(|&x, &y| d.compare(tops[y], tops[x]));
    for w in order.windows(2) {
        if d.compare(tops[w[0]], tops[w[1]]) == Ordering::Equal {
            return Err(Unranked::Tie(w[0].min(w[1]), w[0].max(w[1])));
        }
    }
    Ok(order)
}

/// No node: the parent of a branch of the root.
const NO_NODE: usize = usize::MAX;

/// A hierarchical digraph: each node an atom (or a duplicate) with its atomic number.
#[derive(Default)]
struct Digraph {
    z: Vec<u8>,
    /// Each node's branches, highest-ranked first once sorted.
    kids: Vec<Vec<usize>>,
    /// The atom a node stands for, `None` for a duplicate or a hydrogen (which end there).
    atom: Vec<Option<usize>>,
    /// Each node's parent.
    up: Vec<usize>,
}

impl Digraph {
    fn push(&mut self, z: u8, atom: Option<usize>, up: usize) -> Result<usize, Unranked> {
        if self.z.len() >= MAX_NODES {
            return Err(Unranked::TooLarge);
        }
        self.z.push(z);
        self.kids.push(Vec::new());
        self.atom.push(atom);
        self.up.push(up);
        if up != NO_NODE {
            self.kids[up].push(self.z.len() - 1);
        }
        Ok(self.z.len() - 1)
    }

    /// Whether the path from the root to `node` passes through `atom` (the root itself not
    /// counted).
    fn on_path(&self, mut node: usize, atom: usize) -> bool {
        while node != NO_NODE {
            if self.atom[node] == Some(atom) {
                return true;
            }
            node = self.up[node];
        }
        false
    }

    /// The branch of `root` through its neighbour `a` (bonded by `order`), breadth first, so
    /// that every node is created after its parent.
    fn grow(&mut self, atoms: &Atoms, root: usize, a: usize, order: u8) -> Result<usize, Unranked> {
        let top = self.push(atoms.z[a], Some(a), NO_NODE)?;
        let mut queue = std::collections::VecDeque::from([(top, root, order)]);
        while let Some((node, from, order)) = queue.pop_front() {
            let x = self.atom[node].expect("only atoms are grown");
            for _ in 1..order {
                self.push(atoms.z[from], None, node)?;
            }
            for _ in 0..atoms.h[x] {
                self.push(1, None, node)?;
            }
            for &(y, o) in &atoms.adj[x] {
                if y == from {
                    continue;
                }
                if y == root || self.on_path(node, y) {
                    // A ring closure: a duplicate of the atom it closes on.
                    self.push(atoms.z[y], None, node)?;
                } else {
                    let k = self.push(atoms.z[y], Some(y), node)?;
                    queue.push_back((k, x, o));
                }
                for _ in 1..o {
                    self.push(atoms.z[y], None, node)?;
                }
            }
        }
        Ok(top)
    }

    /// Order every node's branches, highest first. A node's branches are created after it, so
    /// going backwards ranks every branch's own branches before the branch itself is compared.
    fn sort_branches(&mut self) {
        for n in (0..self.z.len()).rev() {
            let mut kids = std::mem::take(&mut self.kids[n]);
            kids.sort_by(|&a, &b| self.compare(b, a));
            self.kids[n] = kids;
        }
    }

    /// How the branch at `a` ranks against the branch at `b` (`Greater`: `a` is higher), their
    /// own branches already sorted: the atoms themselves, then sphere by sphere the sets of
    /// atoms each node of the previous sphere bears, in the order of those nodes.
    fn compare(&self, a: usize, b: usize) -> Ordering {
        let first = self.z[a].cmp(&self.z[b]);
        if first != Ordering::Equal {
            return first;
        }
        let (mut sa, mut sb) = (vec![a], vec![b]);
        loop {
            let (mut na, mut nb) = (Vec::new(), Vec::new());
            // The spheres so far are alike, so they hold the same number of real atoms (the
            // phantom atoms, 0, are left out of the sets).
            for (&x, &y) in sa.iter().zip(&sb) {
                let (kx, ky) = (&self.kids[x], &self.kids[y]);
                for i in 0..kx.len().max(ky.len()) {
                    let zx = kx.get(i).map_or(0, |&k| self.z[k]);
                    let zy = ky.get(i).map_or(0, |&k| self.z[k]);
                    if zx != zy {
                        return zx.cmp(&zy);
                    }
                }
                na.extend_from_slice(kx);
                nb.extend_from_slice(ky);
            }
            if na.is_empty() {
                return Ordering::Equal;
            }
            (sa, sb) = (na, nb);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Atoms from (atomic number, hydrogens) and bonds.
    fn rank_of(
        atoms: &[(u8, u8)],
        bonds: &[(usize, usize, u8)],
        root: usize,
        ligands: &[Ligand],
    ) -> Result<Vec<usize>, Unranked> {
        let mut adj = vec![Vec::new(); atoms.len()];
        for &(a, b, o) in bonds {
            adj[a].push((b, o));
            adj[b].push((a, o));
        }
        let h: Vec<u8> = atoms.iter().map(|a| a.1).collect();
        let cip = Atoms {
            z: atoms.iter().map(|a| a.0).collect(),
            h: &h,
            adj: &adj,
        };
        rank(&cip, root, ligands)
    }

    /// Glyceraldehyde's C2: OH > CHO (O, O, H) > CH2OH (O, H, H) > H.
    #[test]
    fn glyceraldehyde() {
        // 0 C1 (CHO), 1 O1, 2 C2, 3 O2, 4 C3, 5 O3.
        let atoms = [(6, 1), (8, 0), (6, 1), (8, 1), (6, 2), (8, 1)];
        let bonds = [(0, 1, 2), (0, 2, 1), (2, 3, 1), (2, 4, 1), (4, 5, 1)];
        let ligands = [Ligand::Atom(0), Ligand::Atom(3), Ligand::Atom(4), Ligand::H];
        assert_eq!(rank_of(&atoms, &bonds, 2, &ligands), Ok(vec![1, 0, 2, 3]));
    }

    /// Phenyl (C, C, C with its duplicates) outranks tert-butyl? No: both are (C, C, C); at the
    /// next sphere phenyl's ring carbons bear (C, C, H) — one duplicate each — against the
    /// methyls' (H, H, H). Phenyl > tert-butyl.
    #[test]
    fn phenyl_against_tert_butyl() {
        // 0 the root CH, 1–6 benzene, 7 quaternary C, 8–10 its methyls, 11 a methyl on the root.
        let mut atoms = vec![(6, 1)];
        atoms.extend([(6, 0), (6, 1), (6, 1), (6, 1), (6, 1), (6, 1)]);
        atoms.extend([(6, 0), (6, 3), (6, 3), (6, 3), (6, 3)]);
        let bonds = [
            (0, 1, 1),
            (1, 2, 2),
            (2, 3, 1),
            (3, 4, 2),
            (4, 5, 1),
            (5, 6, 2),
            (6, 1, 1),
            (0, 7, 1),
            (7, 8, 1),
            (7, 9, 1),
            (7, 10, 1),
            (0, 11, 1),
        ];
        let ligands = [
            Ligand::Atom(7),
            Ligand::Atom(1),
            Ligand::Atom(11),
            Ligand::H,
        ];
        assert_eq!(rank_of(&atoms, &bonds, 0, &ligands), Ok(vec![1, 0, 2, 3]));
    }

    /// The sets of a sphere are compared in the order of the branches they come from:
    /// –CH(CH3)CH2OH against –CH(CH2CH3)2 (pentan-3-yl). Both are (C, C, H), then the first
    /// group's higher branch, CH2OH (O, H, H), meets one of the ethyl groups' CH2 (C, H, H):
    /// O > C, so –CH(CH3)CH2OH ranks higher, although its methyl (H, H, H), bonded first, is
    /// lower than either CH2.
    #[test]
    fn sets_in_the_order_of_their_branches() {
        // 0 the root CH, 1 CH (methyl 2 first, then 3 CH2 with 4 OH), 5 CH with 6–7 and 8–9
        // ethyl groups, 10 a methyl on the root.
        let atoms = [
            (6, 1),
            (6, 1),
            (6, 3),
            (6, 2),
            (8, 1),
            (6, 1),
            (6, 2),
            (6, 3),
            (6, 2),
            (6, 3),
            (6, 3),
        ];
        let bonds = [
            (0, 1, 1),
            (1, 2, 1),
            (1, 3, 1),
            (3, 4, 1),
            (0, 5, 1),
            (5, 6, 1),
            (6, 7, 1),
            (5, 8, 1),
            (8, 9, 1),
            (0, 10, 1),
        ];
        let ligands = [
            Ligand::Atom(5),
            Ligand::Atom(1),
            Ligand::Atom(10),
            Ligand::H,
        ];
        assert_eq!(rank_of(&atoms, &bonds, 0, &ligands), Ok(vec![1, 0, 2, 3]));
    }

    /// Two ethyl groups tie.
    #[test]
    fn tie() {
        // 0 the root CH(OH), 1–2 ethyl, 3–4 ethyl, 5 O.
        let atoms = [(6, 1), (6, 2), (6, 3), (6, 2), (6, 3), (8, 1)];
        let bonds = [(0, 1, 1), (1, 2, 1), (0, 3, 1), (3, 4, 1), (0, 5, 1)];
        let ligands = [Ligand::Atom(1), Ligand::Atom(3), Ligand::Atom(5), Ligand::H];
        assert_eq!(
            rank_of(&atoms, &bonds, 0, &ligands),
            Err(Unranked::Tie(0, 1))
        );
    }
}
