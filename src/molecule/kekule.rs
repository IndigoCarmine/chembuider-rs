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
//!
//! Which atoms the matching gets to touch is the other half of it. A carbon the drawing leaves
//! without a double bond is one of two things, and the graph tells them apart: if a pattern could
//! have paired it, it was drawn saturated and stays that way (the CH₂ groups of a dihydroarene);
//! if none could, it is what a matching leaves over, and the next ring may be what pairs it
//! (phenalene's, on the way to pyrene).

use super::{BondOrder, BondStereo, Molecule, charged_valence};
use std::collections::{HashMap, HashSet};

/// Sentinel for "no atom" in the blossom search, whose arrays are indexed constantly.
const NONE: usize = usize::MAX;

/// A Kekulé pattern as a matching: the atoms it may change, the bonds between them, and who is
/// paired with whom.
struct Pattern {
    /// Sorted, so the result does not hang on the iteration order of a hash set.
    atoms: Vec<u32>,
    index: HashMap<u32, usize>,
    adjacent: Vec<Vec<usize>>,
    mate: Vec<usize>,
    /// Bond id and the two atoms' places in `atoms`.
    edges: Vec<(u32, usize, usize)>,
}

impl Molecule {
    /// Re-derive the double bonds of the aromatic system around `ring`, the atoms of a ring just
    /// drawn with single bonds. `new_bonds` are the bonds that drawing added; they are tried as
    /// double before anything else, so a ring fused on one edge comes out as the walk along them
    /// gives it. Returns the atoms a bond order changed on.
    ///
    /// The system is `ring` plus the aromatic rings connected to it through shared bonds
    /// ([`aromatic_system`](Self::aromatic_system)). Inside it, what may change is a single or
    /// double bond between atoms that can carry a double bond (see
    /// [`takes_double_bond`](Self::takes_double_bond)). Left as they are: anything outside the
    /// system, any bond with stereo, and any double bond to a heteroatom. A heteroatom's own ring
    /// (a pyridine fused in) therefore keeps its C=N, and the carbon of it is simply spoken for.
    pub(crate) fn rekekulize(&mut self, ring: &[u32], new_bonds: &[(u32, u32)]) -> Vec<u32> {
        let Some(&seed) = ring.first() else {
            return Vec::new();
        };
        let in_ring: HashSet<u32> = ring.iter().copied().collect();
        let system = self.aromatic_system(ring, seed, new_bonds);
        let candidates = system
            .into_iter()
            .filter(|&id| self.takes_double_bond(id, in_ring.contains(&id)))
            .collect();
        let mut pattern = self.pattern_over(candidates);
        for &(u, v) in new_bonds {
            let (Some(&i), Some(&j)) = (pattern.index.get(&u), pattern.index.get(&v)) else {
                continue;
            };
            if pattern.mate[i] == NONE
                && pattern.mate[j] == NONE
                && pattern.adjacent[i].contains(&j)
            {
                pattern.mate[i] = j;
                pattern.mate[j] = i;
            }
        }

        // The new ring's atoms are what needs a partner, so they choose first.
        let (ours, theirs): (Vec<usize>, Vec<usize>) =
            (0..pattern.atoms.len()).partition(|&i| in_ring.contains(&pattern.atoms[i]));
        for root in ours.into_iter().chain(theirs) {
            augment(&pattern.adjacent, &mut pattern.mate, root);
        }

        let mut changed = Vec::new();
        for &(id, i, j) in &pattern.edges {
            let order = if pattern.mate[i] == j {
                BondOrder::Double
            } else {
                BondOrder::Single
            };
            if let Some(bond) = self.bond_by_id_mut(id)
                && bond.order != order
            {
                bond.order = order;
                changed.extend([pattern.atoms[i], pattern.atoms[j]]);
            }
        }
        changed.sort_unstable();
        changed.dedup();
        changed
    }

    /// `ring` plus every aromatic ring reachable from it through rings that share a bond.
    ///
    /// Aromatic is a judgement on the drawing as it stood before the press, which is why
    /// `new_bonds` have to be set aside: every carbon of the ring has to be
    /// [conjugated](Self::conjugated_atoms). The atoms of `ring` itself are in whatever they were,
    /// because the press has just made them part of a new aromatic ring.
    fn aromatic_system(&self, ring: &[u32], seed: u32, new_bonds: &[(u32, u32)]) -> HashSet<u32> {
        let mut before = self.clone();
        before.bonds.retain(|b| {
            !new_bonds.contains(&(b.begin, b.end)) && !new_bonds.contains(&(b.end, b.begin))
        });
        let mut rings = before.rings_from_atoms(&self.connected_atoms(seed));
        let conjugated = before.conjugated_atoms(&rings);
        rings.retain(|r| {
            r.iter()
                .all(|&id| conjugated.contains(&id) || !self.is_carbon(id))
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

    /// The atoms with a double bond, and the atoms of `rings` a matching has to leave over: no
    /// pattern on the skeleton of `rings` could give them a partner.
    ///
    /// An atom that could have had a partner is out, however few of its neighbours lack one:
    /// the two CH₂ groups of a 9,10-dihydroanthracene are three bonds apart and pair up along the
    /// ring between them just as adjacent ones do. A pattern a press has grown is maximum (that
    /// is what [`augment`] leaves behind), so its leftovers really are left over and all stay in.
    /// An atom on a wedge is out whatever the graph says: the wedge has already said it is sp³.
    fn conjugated_atoms(&self, rings: &[Vec<u32>]) -> HashSet<u32> {
        let mut conjugated: HashSet<u32> = self
            .bonds
            .iter()
            .filter(|b| b.order == BondOrder::Double)
            .flat_map(|b| [b.begin, b.end])
            .collect();
        let candidates = rings
            .iter()
            .flatten()
            .copied()
            .filter(|&id| self.takes_double_bond(id, false))
            .collect();
        let pattern = self.pattern_over(candidates);
        for (i, &id) in pattern.atoms.iter().enumerate() {
            let free = pattern.mate[i] == NONE && !conjugated.contains(&id);
            if free && !self.has_stereo_bond(id) {
                let mut trial = pattern.mate.clone();
                augment(&pattern.adjacent, &mut trial, i);
                if trial[i] == NONE {
                    conjugated.insert(id);
                }
            }
        }
        conjugated
    }

    /// The Kekulé pattern as drawn on those of `candidates` it may change: an atom whose double
    /// bond must stay, being to an atom outside them or to a heteroatom or carrying stereo, is
    /// spoken for and left out.
    fn pattern_over(&self, mut candidates: HashSet<u32>) -> Pattern {
        for b in self.bonds.iter().filter(|b| b.order == BondOrder::Double) {
            let can_move = b.stereo == BondStereo::None
                && candidates.contains(&b.begin)
                && candidates.contains(&b.end)
                && self.is_carbon(b.begin)
                && self.is_carbon(b.end);
            if !can_move {
                candidates.remove(&b.begin);
                candidates.remove(&b.end);
            }
        }

        let mut atoms: Vec<u32> = candidates.into_iter().collect();
        atoms.sort_unstable();
        let index: HashMap<u32, usize> = atoms.iter().enumerate().map(|(i, &id)| (id, i)).collect();

        let mut adjacent = vec![Vec::new(); atoms.len()];
        let mut mate = vec![NONE; atoms.len()];
        let mut edges = Vec::new();
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
        Pattern {
            atoms,
            index,
            adjacent,
            mate,
            edges,
        }
    }

    fn has_stereo_bond(&self, id: u32) -> bool {
        self.bonds_for_atom(id)
            .iter()
            .any(|b| b.stereo != BondStereo::None)
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

    /// A cyclohexadiene is not aromatic, for all that only two of its atoms lack a double bond:
    /// they could have had one between them and were drawn without it. Fused onto 0–1, the new
    /// ring pairs off among its own atoms and neither of the two gains a bond to the other.
    #[test]
    fn a_cyclohexadiene_keeps_its_saturated_carbons() {
        let (mut m, ids) = skeleton(
            10,
            &[
                (0, 1, false),
                (1, 2, true),
                (2, 3, false),
                (3, 4, true),
                (4, 5, false),
                (5, 0, false),
                (0, 6, false),
                (6, 7, false),
                (7, 8, false),
                (8, 9, false),
                (9, 1, false),
            ],
        );
        let ring = [ids[0], ids[6], ids[7], ids[8], ids[9], ids[1]];
        let new_bonds: Vec<(u32, u32)> = [(0, 6), (6, 7), (7, 8), (8, 9), (9, 1)]
            .iter()
            .map(|&(a, b)| (ids[a], ids[b]))
            .collect();
        m.rekekulize(&ring, &new_bonds);
        for (a, b, order) in [
            (5, 0, BondOrder::Single),
            (4, 5, BondOrder::Single),
            (1, 2, BondOrder::Double),
            (3, 4, BondOrder::Double),
        ] {
            assert_eq!(
                m.bond_between(ids[a], ids[b]).unwrap().order,
                order,
                "bond {a}–{b}"
            );
        }
        assert_at_most_one_double_each(&m);
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
