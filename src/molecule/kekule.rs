//! Re-deriving the double bonds of an aromatic system a ring has just been fused onto.
//!
//! A fragment's double bonds describe a free-standing ring. Fused onto a system that already
//! has double bonds they can clash with it, and where the new ring shares more than one bond
//! (peri fusion) no local patch will do: naphthalene's pattern has to change at the far end of
//! the molecule for pyrene's to exist at all.
//!
//! A Kekulé pattern is a matching on the carbon skeleton, each double bond pairing two atoms, so
//! that is how it is solved here. The pattern already drawn is the starting matching and only
//! augmenting paths change it, which keeps the double bonds where they are wherever they can
//! stay. Blossoms are handled because a system with a five-membered ring is not bipartite. Where
//! no perfect matching exists (phenalene's 13 carbons) the result is a maximum one: an atom left
//! over simply has single bonds, which is a valid CH₂ and never an over-bonded carbon.

use super::{BondOrder, BondStereo, Molecule, charged_valence};
use std::collections::{HashMap, HashSet};

/// How many atoms of a ring may lack a double bond with the ring still counted as aromatic.
///
/// The rings a matching leaves an atom over on have to stay in the system, or the next ring could
/// never pair that atom up: phenalene's CH₂ is one, and a non-Kekulé system such as triangulene
/// leaves two. A cyclohexene (four) or a tetralin's saturated ring (four) stays out.
const MAX_ATOMS_WITHOUT_DOUBLE_BOND: usize = 2;

/// Sentinel for "no atom" in the blossom search, whose arrays are indexed constantly.
const NONE: usize = usize::MAX;

impl Molecule {
    /// Re-derive the double bonds of the aromatic system around `ring`, the atoms of a ring just
    /// drawn with single bonds.
    ///
    /// `first` lists bonds to try as double before anything else, so a ring fused on one edge
    /// comes out as the walk along them gives it.
    ///
    /// The system is `ring` plus the aromatic rings connected to it through shared bonds. Inside
    /// it, what may change is a single or double bond between atoms that can carry a double
    /// bond (see [`takes_double_bond`](Self::takes_double_bond)). Left as they are: anything
    /// outside the system, any bond with stereo, and any double bond to a heteroatom. A
    /// heteroatom's own ring (a pyridine fused in) therefore keeps its C=N, and the carbon of it
    /// is simply spoken for.
    pub(crate) fn rekekulize(&mut self, ring: &[u32], first: &[(u32, u32)]) {
        let Some(&seed) = ring.first() else { return };
        let system = self.aromatic_system(ring, seed);
        let in_ring: HashSet<u32> = ring.iter().copied().collect();

        let mut movable: HashSet<u32> = system
            .iter()
            .copied()
            .filter(|&id| self.takes_double_bond(id, in_ring.contains(&id)))
            .collect();
        for b in self.bonds.iter().filter(|b| b.order == BondOrder::Double) {
            let can_move = b.stereo == BondStereo::None
                && movable.contains(&b.begin)
                && movable.contains(&b.end)
                && self.is_carbon(b.begin)
                && self.is_carbon(b.end);
            if !can_move {
                movable.remove(&b.begin);
                movable.remove(&b.end);
            }
        }

        // Sorted, so the result does not hang on the iteration order of a hash set.
        let mut atoms: Vec<u32> = movable.into_iter().collect();
        atoms.sort_unstable();
        let index: HashMap<u32, usize> = atoms.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        let mut adjacent = vec![Vec::new(); atoms.len()];
        let mut mate = vec![NONE; atoms.len()];
        let mut edges: Vec<(u32, usize, usize)> = Vec::new();
        for b in &self.bonds {
            let (Some(&i), Some(&j)) = (index.get(&b.begin), index.get(&b.end)) else {
                continue;
            };
            if b.stereo != BondStereo::None
                || !matches!(b.order, BondOrder::Single | BondOrder::Double)
            {
                continue;
            }
            adjacent[i].push(j);
            adjacent[j].push(i);
            edges.push((b.id, i, j));
            if b.order == BondOrder::Double && mate[i] == NONE && mate[j] == NONE {
                mate[i] = j;
                mate[j] = i;
            }
        }
        for &(u, v) in first {
            let (Some(&i), Some(&j)) = (index.get(&u), index.get(&v)) else {
                continue;
            };
            if mate[i] == NONE && mate[j] == NONE && adjacent[i].contains(&j) {
                mate[i] = j;
                mate[j] = i;
            }
        }

        // The new ring's atoms are what needs a partner. The rest of the system only joins in if
        // that completes the pattern: two CH₂ groups that were always there are not an invitation
        // to rewrite the system around them, unless every atom ends up paired.
        let (ours, theirs): (Vec<usize>, Vec<usize>) =
            (0..atoms.len()).partition(|&i| in_ring.contains(&atoms[i]));
        for &root in &ours {
            augment(&adjacent, &mut mate, root);
        }
        let kept = mate.clone();
        for &root in &theirs {
            augment(&adjacent, &mut mate, root);
        }
        if mate.contains(&NONE) {
            mate = kept;
        }

        for (id, i, j) in edges {
            let order = if mate[i] == j {
                BondOrder::Double
            } else {
                BondOrder::Single
            };
            if let Some(bond) = self.bond_by_id_mut(id) {
                bond.order = order;
            }
        }
    }

    /// `ring` plus every aromatic ring reachable from it through rings that share a bond.
    fn aromatic_system(&self, ring: &[u32], seed: u32) -> HashSet<u32> {
        let doubly_bonded: HashSet<u32> = self
            .bonds
            .iter()
            .filter(|b| b.order == BondOrder::Double)
            .flat_map(|b| [b.begin, b.end])
            .collect();
        let mut rings = self.rings_from_atoms(&self.connected_atoms(seed));
        rings.retain(|r| {
            r.iter().filter(|id| !doubly_bonded.contains(id)).count()
                <= MAX_ATOMS_WITHOUT_DOUBLE_BOND
        });

        let mut system: HashSet<u32> = ring.iter().copied().collect();
        loop {
            let before = rings.len();
            rings.retain(|r| {
                // Two shared atoms: a ring joined at a single spiro atom is not conjugated.
                let joined = r.iter().filter(|id| system.contains(id)).count() >= 2;
                if joined {
                    system.extend(r.iter().copied());
                }
                !joined
            });
            if rings.len() == before {
                return system;
            }
        }
    }

    fn is_carbon(&self, id: u32) -> bool {
        self.atom_by_id(id).is_some_and(|a| a.element == "C")
    }

    /// Whether atom `id` has the valence to carry one double bond in a ring: with every bond
    /// a single one, there is one to spare. A triple bond rules it out.
    ///
    /// Only carbon, unless `in_new_ring`: a heteroatom elsewhere in the system is left as drawn,
    /// while one the new ring runs through may take part in that ring's pattern (a fused
    /// pyridine's nitrogen). Group hydrogens do not count, since a label's hydrogen makes way
    /// for the double bond ([`consume_excess_group_h`](Self::consume_excess_group_h)).
    fn takes_double_bond(&self, id: u32, in_new_ring: bool) -> bool {
        let Some(atom) = self.atom_by_id(id) else {
            return false;
        };
        if atom.element != "C" && !in_new_ring {
            return false;
        }
        let Some(valence) = charged_valence(&atom.element, atom.charge) else {
            return false;
        };
        let bonds = self.bonds_for_atom(id);
        if bonds
            .iter()
            .any(|b| matches!(b.order, BondOrder::Triple | BondOrder::Quadruple))
        {
            return false;
        }
        let sigma = bonds
            .iter()
            .filter(|b| {
                let other = if b.begin == id { b.end } else { b.begin };
                !self.atom_by_id(other).is_some_and(|a| a.group_h)
            })
            .count() as i16;
        sigma < valence
    }
}

/// Grow the matching by one pair along an augmenting path from `root`, if it is unmatched and
/// there is one (Edmonds' blossom algorithm, breadth-first, so the path is a shortest one and as
/// few double bonds as possible move).
fn augment(adjacent: &[Vec<usize>], mate: &mut [usize], root: usize) {
    if mate[root] != NONE {
        return;
    }
    let n = adjacent.len();
    // The vertex each one was reached from, and the base of the blossom it is in.
    let mut parent = vec![NONE; n];
    let mut base: Vec<usize> = (0..n).collect();
    let mut used = vec![false; n];
    used[root] = true;
    let mut queue = std::collections::VecDeque::from([root]);

    while let Some(v) = queue.pop_front() {
        for &to in &adjacent[v] {
            if base[v] == base[to] || mate[v] == to {
                continue;
            }
            if to == root || (mate[to] != NONE && parent[mate[to]] != NONE) {
                // An odd cycle: shrink it to its base and go on searching from all of it.
                let lowest = common_base(&base, mate, &parent, v, to);
                let mut in_blossom = vec![false; n];
                mark_blossom(&base, mate, &mut parent, &mut in_blossom, v, lowest, to);
                mark_blossom(&base, mate, &mut parent, &mut in_blossom, to, lowest, v);
                for i in 0..n {
                    if in_blossom[base[i]] {
                        base[i] = lowest;
                        if !used[i] {
                            used[i] = true;
                            queue.push_back(i);
                        }
                    }
                }
            } else if parent[to] == NONE {
                parent[to] = v;
                if mate[to] == NONE {
                    flip_path(mate, &parent, to);
                    return;
                }
                used[mate[to]] = true;
                queue.push_back(mate[to]);
            }
        }
    }
}

/// Match along the augmenting path ending at the unmatched vertex `end`.
fn flip_path(mate: &mut [usize], parent: &[usize], end: usize) {
    let mut v = end;
    while v != NONE {
        let previous = parent[v];
        let next = mate[previous];
        mate[v] = previous;
        mate[previous] = v;
        v = next;
    }
}

/// The base of the blossom that closes where the search paths to `a` and `b` meet.
fn common_base(base: &[usize], mate: &[usize], parent: &[usize], a: usize, b: usize) -> usize {
    let mut on_path = vec![false; base.len()];
    let mut v = a;
    loop {
        v = base[v];
        on_path[v] = true;
        if mate[v] == NONE {
            break;
        }
        v = parent[mate[v]];
    }
    let mut v = b;
    loop {
        v = base[v];
        if on_path[v] {
            return v;
        }
        v = parent[mate[v]];
    }
}

/// Mark the blossom's vertices on the path from `v` down to `lowest`, re-pointing their parents
/// so the path can later be walked through the blossom from `child`'s side.
fn mark_blossom(
    base: &[usize],
    mate: &[usize],
    parent: &mut [usize],
    in_blossom: &mut [bool],
    mut v: usize,
    lowest: usize,
    mut child: usize,
) {
    while base[v] != lowest {
        in_blossom[base[v]] = true;
        in_blossom[base[mate[v]]] = true;
        parent[v] = child;
        child = mate[v];
        v = parent[mate[v]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An all-carbon molecule from its bonds, `true` for a double one. Positions do not matter.
    fn skeleton(n: usize, bonds: &[(usize, usize, bool)]) -> (Molecule, Vec<u32>) {
        let mut m = Molecule::default();
        let ids: Vec<u32> = (0..n)
            .map(|i| m.add_atom("C".into(), [i as f32, 0.0], 0))
            .collect();
        for &(a, b, double) in bonds {
            let order = if double {
                BondOrder::Double
            } else {
                BondOrder::Single
            };
            m.add_bond(ids[a], ids[b], order);
        }
        (m, ids)
    }

    fn doubles(m: &Molecule) -> usize {
        m.bonds
            .iter()
            .filter(|b| b.order == BondOrder::Double)
            .count()
    }

    fn assert_at_most_one_double_each(m: &Molecule) {
        for a in &m.atoms {
            let n = m
                .bonds_for_atom(a.id)
                .iter()
                .filter(|b| b.order == BondOrder::Double)
                .count();
            assert!(n <= 1, "atom {} carries {n} double bonds", a.id);
        }
    }

    /// Pairing two free atoms can mean moving every double bond between them: the pattern
    /// changes along the alternating path, and nowhere else.
    #[test]
    fn a_pattern_shifts_along_an_alternating_path() {
        let (mut m, ids) = skeleton(
            5,
            &[(0, 1, false), (1, 2, true), (2, 3, false), (3, 4, false)],
        );
        let moved = m.bond_between(ids[1], ids[2]).unwrap().id;
        m.rekekulize(&ids[..4], &[]);
        assert_eq!(doubles(&m), 2, "0–1=2–3 becomes 0=1–2=3");
        assert_eq!(m.bond_by_id(moved).unwrap().order, BondOrder::Single);
        assert_eq!(
            m.bond_between(ids[3], ids[4]).unwrap().order,
            BondOrder::Single
        );
        assert_at_most_one_double_each(&m);
    }

    /// A five-membered ring makes the system non-bipartite, which is what a plain alternating
    /// search gets wrong. Ring 0–4 with 0=1 and 2=3 drawn, atom 5 hanging off 0: atoms 4 and 5
    /// are free, and pairing them means going round the odd ring to reach 0 from the far side.
    #[test]
    fn an_odd_ring_is_searched_through() {
        let (mut m, ids) = skeleton(
            6,
            &[
                (0, 1, true),
                (1, 2, false),
                (2, 3, true),
                (3, 4, false),
                (4, 0, false),
                (5, 0, false),
            ],
        );
        m.rekekulize(&ids, &[]);
        assert_eq!(doubles(&m), 3, "5=0, 1=2, 3=4 pairs all six");
        assert_at_most_one_double_each(&m);
        assert_eq!(
            m.bond_between(ids[5], ids[0]).unwrap().order,
            BondOrder::Double
        );
    }

    /// With an odd number of carbons one atom is left with single bonds and nothing is corrupted.
    #[test]
    fn an_odd_system_leaves_one_atom_over() {
        let (mut m, ids) = skeleton(
            7,
            &[
                (0, 1, false),
                (1, 2, false),
                (2, 3, false),
                (3, 4, false),
                (4, 5, false),
                (5, 6, false),
                (6, 0, false),
            ],
        );
        m.rekekulize(&ids, &[]);
        assert_eq!(doubles(&m), 3);
        assert_at_most_one_double_each(&m);
    }

    /// A double bond on an atom that has no valence to spare for it stays out of the matching,
    /// and a quaternary carbon gets no double bond.
    #[test]
    fn a_full_atom_takes_no_double_bond() {
        let (mut m, ids) = skeleton(
            8,
            &[
                (0, 1, false),
                (1, 2, false),
                (2, 3, false),
                (3, 4, false),
                (4, 5, false),
                (5, 0, false),
                (0, 6, false),
                (0, 7, false),
            ],
        );
        m.rekekulize(&ids[..6], &[]);
        for b in m.bonds_for_atom(ids[0]) {
            assert_eq!(b.order, BondOrder::Single, "atom 0 has four neighbours");
        }
        assert_eq!(
            doubles(&m),
            2,
            "of the five left in the ring, four pair off"
        );
    }

    /// Stereo marks a bond the user chose; a matching does not get to reuse it.
    #[test]
    fn a_bond_with_stereo_is_left_alone() {
        let (mut m, ids) = skeleton(2, &[(0, 1, false)]);
        m.bonds[0].stereo = BondStereo::WedgeUp;
        m.rekekulize(&ids, &[]);
        assert_eq!(doubles(&m), 0);
    }

    /// Aromatic rings joined through a shared bond are one system. A ring that is not aromatic
    /// keeps its double bond, and so does a ring joined to the system at a single atom.
    #[test]
    fn the_system_stops_at_a_saturated_ring() {
        // A cyclohexene (0–5, with 2=3) sharing the bond 0–5 with the new ring.
        let (mut m, ids) = skeleton(
            10,
            &[
                (0, 1, false),
                (1, 2, false),
                (2, 3, true),
                (3, 4, false),
                (4, 5, false),
                (5, 0, false),
                (0, 6, false),
                (6, 7, false),
                (7, 8, false),
                (8, 9, false),
                (9, 5, false),
            ],
        );
        let ring = [ids[0], ids[6], ids[7], ids[8], ids[9], ids[5]];
        let kept = m.bond_between(ids[2], ids[3]).unwrap().id;
        m.rekekulize(&ring, &[]);
        assert_eq!(m.bond_by_id(kept).unwrap().order, BondOrder::Double);
        assert_eq!(
            doubles(&m),
            4,
            "the new ring's six pair off among themselves"
        );
    }

    /// `first` decides between patterns that are otherwise equally good.
    #[test]
    fn the_preferred_bonds_go_first() {
        let (mut m, ids) = skeleton(
            4,
            &[(0, 1, false), (1, 2, false), (2, 3, false), (3, 0, false)],
        );
        m.rekekulize(&ids, &[(ids[1], ids[2])]);
        assert_eq!(
            m.bond_between(ids[1], ids[2]).unwrap().order,
            BondOrder::Double
        );
        assert_eq!(
            m.bond_between(ids[3], ids[0]).unwrap().order,
            BondOrder::Double
        );
    }
}
