//! Order- and id-independent comparison of molecules.
//!
//! [`Molecule`] atom ids are a function of construction history — `add_atom` always
//! allocates a fresh id — so two chemically identical structures built by different routes
//! share no ids and list their atoms in different orders. Comparing them therefore needs
//! graph isomorphism over the chemical labels, not structural equality.
//!
//! Two entry points, with different guarantees:
//!
//! - [`canonical_key`] is a fast, sortable, hashable string. Equal molecules always produce
//!   equal keys, but it is a refinement heuristic (1-dimensional Weisfeiler-Leman), so in
//!   principle two *different* molecules could collide. Use it for bucketing, dedup and
//!   human-readable failure messages.
//! - [`is_isomorphic`] is **exact**. It uses the same colouring only to prune, then verifies
//!   with backtracking search. Use it for assertions.
//!
//! Atom **positions are deliberately ignored**: 2D cleanup moves atoms freely without
//! changing the chemistry.

use super::{BondOrder, BondStereo, Molecule};
use std::collections::HashMap;

/// Adjacency in index space (positions in `mol.atoms`), with the chemical labels attached.
struct Graph<'a> {
    mol: &'a Molecule,
    /// `adj[i]` = neighbours of atom `i` as `(neighbour_index, bond_label)`.
    adj: Vec<Vec<(usize, BondLabel)>>,
}

/// A bond's chemical identity from one endpoint's point of view.
///
/// `from_begin` records which end we are looking from, but **only for wedges**. A wedge is
/// directional — its narrow end sits at `begin`, so A→B is not the same bond as B→A — while
/// every other bond type is symmetric. Recording direction for symmetric bonds would make a
/// molecule differ from itself whenever a bond happened to be stored end-swapped, which is
/// exactly what varies between construction routes.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct BondLabel {
    order: u8,
    stereo: u8,
    from_begin: bool,
}

/// Whether `stereo` distinguishes the two ends of the bond.
fn is_directional(stereo: u8) -> bool {
    // 1 = WedgeUp, 2 = WedgeDown (hashed wedge). Bold/Dashed/Wavy are drawn symmetrically.
    stereo == 1 || stereo == 2
}

fn order_code(o: &BondOrder) -> u8 {
    match o {
        BondOrder::Single => 1,
        BondOrder::Double => 2,
        BondOrder::Triple => 3,
    }
}

fn stereo_code(s: &BondStereo) -> u8 {
    match s {
        BondStereo::None => 0,
        BondStereo::WedgeUp => 1,
        BondStereo::WedgeDown => 2,
        BondStereo::Bold => 3,
        BondStereo::Dashed => 4,
        BondStereo::Wavy => 5,
    }
}

impl<'a> Graph<'a> {
    fn new(mol: &'a Molecule) -> Self {
        let index_of: HashMap<u32, usize> =
            mol.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();

        let mut adj = vec![Vec::new(); mol.atoms.len()];
        for b in &mol.bonds {
            // A bond referencing a missing atom is malformed; skip rather than panic so
            // comparison stays usable on partially-built states.
            let (Some(&i), Some(&j)) = (index_of.get(&b.begin), index_of.get(&b.end)) else {
                continue;
            };
            let order = order_code(&b.order);
            let stereo = stereo_code(&b.stereo);
            let dir = is_directional(stereo);
            adj[i].push((j, BondLabel { order, stereo, from_begin: dir }));
            adj[j].push((i, BondLabel { order, stereo, from_begin: false }));
        }
        Graph { mol, adj }
    }

    fn len(&self) -> usize {
        self.mol.atoms.len()
    }

    /// Element + formal charge — the part of an atom's identity that does not depend on
    /// the rest of the graph.
    fn atom_label(&self, i: usize) -> (&str, i8) {
        let a = &self.mol.atoms[i];
        (a.element.as_str(), a.charge)
    }

    /// Connected components as index lists.
    fn components(&self) -> Vec<Vec<usize>> {
        let mut seen = vec![false; self.len()];
        let mut out = Vec::new();
        for start in 0..self.len() {
            if seen[start] {
                continue;
            }
            let mut stack = vec![start];
            let mut comp = Vec::new();
            seen[start] = true;
            while let Some(i) = stack.pop() {
                comp.push(i);
                for &(j, _) in &self.adj[i] {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            comp.sort_unstable();
            out.push(comp);
        }
        out
    }

    /// Length of the shortest cycle through each atom (`0` if the atom is acyclic).
    ///
    /// This is what separates "two cyclopropanes" from "one cyclohexane": both are all-carbon,
    /// all-degree-2, all-single-bond, so plain 1-WL cannot tell them apart. Component
    /// splitting also catches that particular pair, but the girth term additionally separates
    /// fused ring systems of equal size and degree sequence, which splitting does not.
    fn girths(&self) -> Vec<u32> {
        (0..self.len()).map(|root| self.girth_at(root)).collect()
    }

    fn girth_at(&self, root: usize) -> u32 {
        // BFS from `root`; a non-tree edge joining two branches closes a cycle through root.
        let mut dist = vec![u32::MAX; self.len()];
        let mut parent = vec![usize::MAX; self.len()];
        let mut best = u32::MAX;
        dist[root] = 0;
        let mut queue = std::collections::VecDeque::from([root]);
        while let Some(i) = queue.pop_front() {
            for &(j, _) in &self.adj[i] {
                if j == parent[i] {
                    continue;
                }
                if dist[j] == u32::MAX {
                    dist[j] = dist[i] + 1;
                    parent[j] = i;
                    queue.push_back(j);
                } else {
                    // Only cycles that pass through `root` count, i.e. those whose two arms
                    // leave root separately. dist[i] + dist[j] + 1 is the cycle length.
                    best = best.min(dist[i] + dist[j] + 1);
                }
            }
        }
        if best == u32::MAX { 0 } else { best }
    }

    /// Stable 1-WL colouring, restricted to `subset`.
    ///
    /// Returns one colour per atom index in `subset` order.
    fn wl_colors(&self, subset: &[usize]) -> Vec<u64> {
        let in_subset: HashMap<usize, usize> =
            subset.iter().enumerate().map(|(k, &i)| (i, k)).collect();
        let girth = self.girths();

        // Initial colour: everything about the atom that needs no neighbourhood context.
        let mut colors: Vec<u64> = subset
            .iter()
            .map(|&i| {
                let (elem, charge) = self.atom_label(i);
                let mut incident: Vec<BondLabel> =
                    self.adj[i].iter().map(|&(_, l)| l).collect();
                incident.sort_unstable();
                hash_of(&(elem, charge, incident, girth[i]))
            })
            .collect();

        // Refine until the partition stops changing. n rounds is the standard bound: each
        // round either splits a class or the partition is already stable.
        for _ in 0..subset.len() {
            let mut next = Vec::with_capacity(subset.len());
            for (k, &i) in subset.iter().enumerate() {
                let mut nbr: Vec<(BondLabel, u64)> = self.adj[i]
                    .iter()
                    .filter_map(|&(j, l)| in_subset.get(&j).map(|&kj| (l, colors[kj])))
                    .collect();
                nbr.sort_unstable();
                next.push(hash_of(&(colors[k], nbr)));
            }
            if next == colors {
                break;
            }
            colors = next;
        }
        colors
    }
}

fn hash_of<T: std::hash::Hash>(v: &T) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// A canonical string for `mol`, invariant under atom id and ordering.
///
/// Equal molecules always produce equal keys. Different molecules *almost* always produce
/// different keys — see the module docs; use [`is_isomorphic`] when you need certainty.
pub fn canonical_key(mol: &Molecule) -> String {
    let g = Graph::new(mol);

    // Canonicalize each component independently, then sort, so component order in
    // `mol.atoms` cannot leak into the key.
    let mut parts: Vec<String> = g
        .components()
        .into_iter()
        .map(|comp| {
            let colors = g.wl_colors(&comp);
            let index_in_comp: HashMap<usize, usize> =
                comp.iter().enumerate().map(|(k, &i)| (i, k)).collect();

            let mut atoms: Vec<String> = comp
                .iter()
                .enumerate()
                .map(|(k, &i)| {
                    let (elem, charge) = g.atom_label(i);
                    format!("{elem}{charge:+}:{:016x}", colors[k])
                })
                .collect();
            atoms.sort_unstable();

            let mut bonds: Vec<String> = mol
                .bonds
                .iter()
                .filter_map(|b| {
                    let i = mol.atoms.iter().position(|a| a.id == b.begin)?;
                    let j = mol.atoms.iter().position(|a| a.id == b.end)?;
                    let (&ki, &kj) = (index_in_comp.get(&i)?, index_in_comp.get(&j)?);
                    let (lo, hi) = if colors[ki] <= colors[kj] {
                        (colors[ki], colors[kj])
                    } else {
                        (colors[kj], colors[ki])
                    };
                    Some(format!(
                        "{lo:016x}-{hi:016x}/{}{}",
                        order_code(&b.order),
                        stereo_code(&b.stereo)
                    ))
                })
                .collect();
            bonds.sort_unstable();

            format!("[{}|{}]", atoms.join(","), bonds.join(","))
        })
        .collect();
    parts.sort_unstable();
    parts.join("+")
}

/// Exact graph isomorphism over element, charge, bond order and bond stereo.
///
/// Positions are ignored. This is the assertion-grade comparison: the WL colouring only
/// restricts the candidate set, and a match is confirmed by explicit backtracking search.
pub fn is_isomorphic(a: &Molecule, b: &Molecule) -> bool {
    if a.atoms.len() != b.atoms.len() || a.bonds.len() != b.bonds.len() {
        return false;
    }

    let (ga, gb) = (Graph::new(a), Graph::new(b));
    let all_a: Vec<usize> = (0..ga.len()).collect();
    let all_b: Vec<usize> = (0..gb.len()).collect();
    let ca = ga.wl_colors(&all_a);
    let cb = gb.wl_colors(&all_b);

    // Cheap reject: the colour multisets must agree.
    let (mut sa, mut sb) = (ca.clone(), cb.clone());
    sa.sort_unstable();
    sb.sort_unstable();
    if sa != sb {
        return false;
    }

    // Order the search most-constrained-first: rarest colour class first, and thereafter
    // prefer atoms adjacent to something already mapped so partial maps fail fast.
    let mut freq: HashMap<u64, usize> = HashMap::new();
    for &c in &ca {
        *freq.entry(c).or_default() += 1;
    }
    let mut order: Vec<usize> = all_a.clone();
    order.sort_by_key(|&i| (freq[&ca[i]], std::cmp::Reverse(ga.adj[i].len())));
    let order = connectivity_order(&ga, order);

    let mut map = vec![usize::MAX; ga.len()];
    let mut used = vec![false; gb.len()];
    backtrack(&ga, &gb, &ca, &cb, &order, 0, &mut map, &mut used)
}

/// Reorder so that, after the first atom, every atom is adjacent to an earlier one where
/// possible. Keeps the partial map connected, which is what makes pruning effective.
fn connectivity_order(g: &Graph, seed_order: Vec<usize>) -> Vec<usize> {
    let mut placed = vec![false; g.len()];
    let mut out = Vec::with_capacity(g.len());
    for &seed in &seed_order {
        if placed[seed] {
            continue;
        }
        placed[seed] = true;
        out.push(seed);
        // Grow the component from this seed, preferring the seed_order ranking.
        loop {
            let next = seed_order.iter().copied().find(|&cand| {
                !placed[cand] && g.adj[cand].iter().any(|&(j, _)| placed[j])
            });
            match next {
                Some(c) => {
                    placed[c] = true;
                    out.push(c);
                }
                None => break,
            }
        }
    }
    out
}

fn backtrack(
    ga: &Graph,
    gb: &Graph,
    ca: &[u64],
    cb: &[u64],
    order: &[usize],
    depth: usize,
    map: &mut Vec<usize>,
    used: &mut Vec<bool>,
) -> bool {
    if depth == order.len() {
        return true;
    }
    let i = order[depth];

    for j in 0..gb.len() {
        if used[j] || ca[i] != cb[j] {
            continue;
        }
        if ga.atom_label(i) != gb.atom_label(j) {
            continue;
        }
        if !edges_consistent(ga, gb, map, i, j) {
            continue;
        }
        map[i] = j;
        used[j] = true;
        if backtrack(ga, gb, ca, cb, order, depth + 1, map, used) {
            return true;
        }
        map[i] = usize::MAX;
        used[j] = false;
    }
    false
}

/// Every already-mapped neighbour of `i` must correspond to a neighbour of `j` joined by an
/// identically-labelled bond, and `j` must have no extra edges into the mapped set.
fn edges_consistent(ga: &Graph, gb: &Graph, map: &[usize], i: usize, j: usize) -> bool {
    if ga.adj[i].len() != gb.adj[j].len() {
        return false;
    }
    let mut want = 0usize;
    for &(ni, li) in &ga.adj[i] {
        let mi = map[ni];
        if mi == usize::MAX {
            continue;
        }
        want += 1;
        if !gb.adj[j].iter().any(|&(nj, lj)| nj == mi && lj == li) {
            return false;
        }
    }
    // Count edges from j back into the mapped image; a mismatch means j is over-connected.
    let got = gb.adj[j]
        .iter()
        .filter(|&&(nj, _)| map.iter().any(|&m| m == nj))
        .count();
    got == want
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::Molecule;

    fn chain(elements: &[&str]) -> Molecule {
        let mut m = Molecule::default();
        let ids: Vec<u32> = elements
            .iter()
            .enumerate()
            .map(|(k, e)| m.add_atom((*e).to_string(), [k as f32, 0.0], 0))
            .collect();
        for w in ids.windows(2) {
            m.add_bond(w[0], w[1], BondOrder::Single);
        }
        m
    }

    fn ring(n: usize) -> Molecule {
        let mut m = Molecule::default();
        let ids: Vec<u32> = (0..n)
            .map(|k| m.add_atom("C".to_string(), [k as f32, 0.0], 0))
            .collect();
        for k in 0..n {
            m.add_bond(ids[k], ids[(k + 1) % n], BondOrder::Single);
        }
        m
    }

    #[test]
    fn identical_structures_built_with_different_ids_match() {
        let a = chain(&["C", "C", "O"]);
        let mut b = Molecule::default();
        // Same molecule, allocated in the opposite order and bonded back-to-front.
        let o = b.add_atom("O".to_string(), [9.0, 9.0], 0);
        let c2 = b.add_atom("C".to_string(), [3.0, 1.0], 0);
        let c1 = b.add_atom("C".to_string(), [0.0, 5.0], 0);
        b.add_bond(o, c2, BondOrder::Single);
        b.add_bond(c2, c1, BondOrder::Single);

        assert_eq!(canonical_key(&a), canonical_key(&b));
        assert!(is_isomorphic(&a, &b));
    }

    #[test]
    fn positions_are_ignored() {
        let a = chain(&["C", "C", "O"]);
        let mut b = chain(&["C", "C", "O"]);
        for atom in &mut b.atoms {
            atom.pos = [atom.pos[0] * 37.0 - 11.0, atom.pos[1] + 4.0];
        }
        assert!(is_isomorphic(&a, &b), "2D cleanup must not change identity");
    }

    #[test]
    fn element_charge_order_and_stereo_all_matter() {
        let base = chain(&["C", "C", "O"]);

        assert!(!is_isomorphic(&base, &chain(&["C", "C", "S"])), "element");

        let mut charged = chain(&["C", "C", "O"]);
        charged.atoms[2].charge = -1;
        assert!(!is_isomorphic(&base, &charged), "charge");

        let mut doubled = chain(&["C", "C", "O"]);
        doubled.bonds[1].order = BondOrder::Double;
        assert!(!is_isomorphic(&base, &doubled), "bond order");

        let mut wedged = chain(&["C", "C", "O"]);
        wedged.bonds[0].stereo = BondStereo::WedgeUp;
        assert!(!is_isomorphic(&base, &wedged), "bond stereo");
    }

    /// The classic 1-WL blind spot: two triangles vs one hexagon. Same atom count, same bond
    /// count, all carbon, every atom degree 2, every bond single.
    #[test]
    fn two_cyclopropanes_are_not_one_cyclohexane() {
        let mut two = ring(3);
        let second = ring(3);
        let base: Vec<u32> = second
            .atoms
            .iter()
            .map(|a| two.add_atom(a.element.clone(), a.pos, a.charge))
            .collect();
        for b in &second.bonds {
            let i = second.atoms.iter().position(|a| a.id == b.begin).unwrap();
            let j = second.atoms.iter().position(|a| a.id == b.end).unwrap();
            two.add_bond(base[i], base[j], b.order.clone());
        }
        let six = ring(6);

        assert_eq!(two.atoms.len(), six.atoms.len());
        assert_eq!(two.bonds.len(), six.bonds.len());
        assert!(!is_isomorphic(&two, &six));
        assert_ne!(canonical_key(&two), canonical_key(&six));
    }

    #[test]
    fn component_order_does_not_matter() {
        let mut ab = Molecule::default();
        let o = ab.add_atom("O".to_string(), [0.0, 0.0], 0);
        let n = ab.add_atom("N".to_string(), [5.0, 0.0], 0);
        let _ = (o, n);

        let mut ba = Molecule::default();
        ba.add_atom("N".to_string(), [0.0, 0.0], 0);
        ba.add_atom("O".to_string(), [5.0, 0.0], 0);

        assert_eq!(canonical_key(&ab), canonical_key(&ba));
        assert!(is_isomorphic(&ab, &ba));
    }

    #[test]
    fn disconnected_pair_differs_from_bonded_pair() {
        let mut apart = Molecule::default();
        apart.add_atom("C".to_string(), [0.0, 0.0], 0);
        apart.add_atom("O".to_string(), [5.0, 0.0], 0);
        let together = chain(&["C", "O"]);
        assert!(!is_isomorphic(&apart, &together));
    }
}
