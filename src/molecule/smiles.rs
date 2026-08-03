//! SMILES generation from a drawn [`Molecule`].
//!
//! [`to_smiles`] turns whatever is on the canvas into a SMILES string. The output is
//! **canonical**: it depends only on the chemical graph, never on atom ids, atom order or the
//! route the structure was built by, so two drawings of the same molecule always give the same
//! string and a test can pin that string exactly.
//!
//! # How canonical
//!
//! The canonical form is defined as *the lexicographically smallest SMILES string over every
//! depth-first traversal of the structure* — every start atom, every branch ordering. A minimum
//! over a set that is itself defined purely by the graph is invariant under relabelling by
//! construction, which is a stronger and much simpler guarantee than deriving an atom ordering
//! from a refinement colouring: [`canonical_key`](super::canonical::canonical_key) uses
//! `DefaultHasher`, whose values std explicitly declines to keep stable across releases, so
//! ordering atoms by those colours would silently change the output on a compiler upgrade.
//! (The colouring is still the right tool for its own job; [`is_isomorphic`] verifies this
//! module's round-trip tests.)
//!
//! Exhaustive enumeration is affordable because drawn structures are small and because a
//! candidate is abandoned as soon as its first atom cannot beat the best string found so far.
//! [`TRAVERSAL_BUDGET`] caps the search anyway; the fixtures use a low four-figure fraction of
//! it, and a structure that hit it would still get a valid, deterministic-for-that-input string.
//!
//! # What is written, and what is not
//!
//! - **Kekulé, always.** The editor stores aromatic rings as alternating single/double bonds
//!   and has no aromaticity flag, so benzene comes out as `C1=CC=CC=C1`, not `c1ccccc1`. Both
//!   are valid SMILES and every toolkit perceives aromaticity when reading; writing lowercase
//!   would mean *guessing* which rings are aromatic, and a wrong guess produces a string that
//!   other software rejects outright as unkekulizable.
//! - **Hydrogens are folded**, not written as atoms: a hydrogen that hangs off a heavy atom by
//!   a single bond becomes part of that atom's hydrogen count, whether the fragment brought it
//!   ([`Atom::group_h`](super::Atom::group_h)) or the user typed it. So an O–H drawn with the
//!   hydrogen visible and an O whose hydrogen is only implied both give `O`, which is right:
//!   they are the same molecule. Hydrogens that are *not* a heavy atom's substituent — H₂, a
//!   bridging hydrogen, a bare H, an H⁺ — stay atoms and are written `[H]`.
//! - **Deuterium is an isotope**, not a hydrogen: the `D` fragment is written `[2H]` (and `T`
//!   as `[3H]`) and is never folded into a neighbour's count, because folding it would silently
//!   turn a labelled compound into an unlabelled one.
//! - **Stereochemistry is dropped.** See [`to_smiles`].

use super::{BondOrder, Molecule, normal_valence};
use std::collections::{HashMap, HashSet};

/// How many complete traversals [`to_smiles`] will enumerate per connected component before
/// settling for the best string it has found.
///
/// The search is exhaustive, so this is the only thing standing between a pathologically
/// symmetric graph and an exponential blow-up. Every fixture in `tests/fixtures/rendering/`
/// stays under 20 000, and `no_fixture_comes_near_the_traversal_budget` keeps it that way.
const TRAVERSAL_BUDGET: usize = 200_000;

/// The canonical SMILES string for `mol`.
///
/// Empty for an empty molecule. Disconnected components are written smallest-string-first,
/// separated by `.`.
///
/// **Stereochemistry is not encoded.** [`BondStereo`](super::BondStereo) records a wedge, a
/// hash, a wavy bond — 2D drawing marks — and turning those into `@`/`@@` or `/`…`\` means
/// reading tetrahedral parity and double-bond geometry out of the 2D coordinates. This
/// crate's own position on coordinates is that they are not chemistry: `canonical.rs` ignores
/// them because "2D cleanup moves atoms freely without changing the chemistry", and
/// [`cleanup`](super::cleanup) really does move them — a relaxation pass could flip a
/// cis double bond's apparent geometry and, with it, the SMILES. Emitting stereo that a
/// clean-up can silently invert is worse than emitting none, so enantiomers and E/Z isomers
/// both come out as the same string. If you need stereo, export MOL2 and keep the coordinates.
pub fn to_smiles(mol: &Molecule) -> String {
    generate(mol).smiles
}

/// [`to_smiles`] plus the search statistics the tests assert on.
struct Generated {
    smiles: String,
    /// Largest number of complete traversals any single component needed.
    #[cfg_attr(not(test), allow(dead_code))]
    traversals: usize,
    /// Whether any component ran out of [`TRAVERSAL_BUDGET`].
    #[cfg_attr(not(test), allow(dead_code))]
    exhausted: bool,
}

fn generate(mol: &Molecule) -> Generated {
    let sk = Skeleton::build(mol);
    let mut search = Search::new(&sk);
    let mut parts: Vec<String> = Vec::new();
    let mut traversals = 0usize;
    let mut exhausted = false;

    for comp in sk.components() {
        search.traversals = 0;
        parts.push(search.component_smiles(&comp));
        traversals = traversals.max(search.traversals);
        exhausted |= search.exhausted;
    }

    // Sorting makes component order in `mol.atoms` unobservable, the same reason
    // `canonical_key` sorts its per-component parts.
    parts.sort();
    Generated { smiles: parts.join("."), traversals, exhausted }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Element knowledge
// ═══════════════════════════════════════════════════════════════════════════════

/// Every real element symbol, so that a hand-typed atom label can be told apart from an
/// element. A label the periodic table does not contain ("Ph", "Xx", a group abbreviation)
/// has no SMILES spelling and is written as the wildcard `[*]`.
const ELEMENT_SYMBOLS: [&str; 118] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S",
    "Cl", "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga",
    "Ge", "As", "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd",
    "Ag", "Cd", "In", "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm",
    "Sm", "Eu", "Gd", "Tb", "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os",
    "Ir", "Pt", "Au", "Hg", "Tl", "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa",
    "U", "Np", "Pu", "Am", "Cm", "Bk", "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg",
    "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh", "Fl", "Mc", "Lv", "Ts", "Og",
];

/// The SMILES "organic subset": the only elements that may be written without brackets, and
/// then only when their hydrogen count is exactly what a reader would infer.
const ORGANIC_SUBSET: [&str; 10] = ["B", "C", "N", "O", "P", "S", "F", "Cl", "Br", "I"];

/// A hydrogen in any of its isotopic spellings. These never take implicit hydrogens of their
/// own, and never fold into a neighbour's count as *its* hydrogens.
fn is_hydrogenic(element: &str) -> bool {
    matches!(element, "H" | "D" | "T")
}

/// Split a drawn element label into the (symbol, isotope) pair SMILES needs.
///
/// The editor has no isotope field; deuterium and tritium arrive as the *element labels* `D`
/// and `T` (see `fragments/D.json`). SMILES has no such symbols — the mass number carries the
/// meaning — so they become `H` with an isotope, which is what `[2H]` says.
fn split_isotope(element: &str) -> (&str, Option<u16>) {
    match element {
        "D" => ("H", Some(2)),
        "T" => ("H", Some(3)),
        other if ELEMENT_SYMBOLS.contains(&other) => (other, None),
        _ => ("*", None),
    }
}

/// Hydrogens a SMILES *reader* will add to a bare organic-subset atom carrying `bond_sum`
/// worth of bonds: the shortfall against the next standard valence at or above it.
///
/// These are the OpenSMILES default valences, and they are not the same list as
/// [`normal_valence`] — that one is deliberately the *lowest* common valence so a drawn S reads
/// as H₂S, whereas a SMILES reader will happily take an `S` with four bonds as a sulfoxide. The
/// two lists disagreeing is exactly why [`atom_token`] checks this function instead of assuming:
/// where they differ, the atom gets brackets and says its hydrogen count out loud.
fn smiles_implicit_h(symbol: &str, bond_sum: i16) -> u32 {
    let valences: &[i16] = match symbol {
        "B" => &[3],
        "C" => &[4],
        "N" => &[3, 5],
        "O" => &[2],
        "P" => &[3, 5],
        "S" => &[2, 4, 6],
        "F" | "Cl" | "Br" | "I" => &[1],
        _ => return 0,
    };
    valences
        .iter()
        .find(|&&v| v >= bond_sum)
        .map(|&v| (v - bond_sum) as u32)
        .unwrap_or(0)
}

/// One atom's SMILES spelling, brackets and all.
///
/// The bare form is used only when a reader would reconstruct the atom exactly: no isotope, no
/// charge, an organic-subset element, and a hydrogen count that matches what the reader infers
/// from `bond_sum`. Anything else is bracketed, where the hydrogen count is stated explicitly
/// and nothing is left to inference.
fn atom_token(symbol: &str, isotope: Option<u16>, charge: i8, h: u32, bond_sum: i16) -> String {
    if isotope.is_none()
        && charge == 0
        && ORGANIC_SUBSET.contains(&symbol)
        && smiles_implicit_h(symbol, bond_sum) == h
    {
        return symbol.to_string();
    }

    let mut s = String::from("[");
    if let Some(iso) = isotope {
        s.push_str(&iso.to_string());
    }
    s.push_str(symbol);
    match h {
        0 => {}
        1 => s.push('H'),
        n => {
            s.push('H');
            s.push_str(&n.to_string());
        }
    }
    match charge {
        0 => {}
        1 => s.push('+'),
        -1 => s.push('-'),
        c if c > 0 => {
            s.push('+');
            s.push_str(&c.to_string());
        }
        c => {
            s.push('-');
            s.push_str(&(-(c as i16)).to_string());
        }
    }
    s.push(']');
    s
}

fn bond_symbol(order: u8) -> &'static str {
    match order {
        2 => "=",
        3 => "#",
        _ => "",
    }
}

fn order_code(o: &BondOrder) -> u8 {
    match o {
        BondOrder::Single => 1,
        BondOrder::Double => 2,
        BondOrder::Triple => 3,
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// The written graph
// ═══════════════════════════════════════════════════════════════════════════════

/// Which hydrogens [`to_smiles`] folds into a neighbour's hydrogen count instead of writing
/// as atoms of their own.
///
/// A hydrogen qualifies when it is a heavy atom's *substituent* and nothing more: uncharged,
/// exactly one bond, that bond single, and the other end is not itself a hydrogen. Everything
/// else is a hydrogen that carries information a count cannot hold — the two atoms of H₂, a
/// hydrogen bridging two centres, a bare H, an H⁺ — and stays an atom.
///
/// This deliberately catches hand-placed hydrogens as well as
/// [`group_h`](super::Atom::group_h) ones: SMILES has no notion of a hydrogen being "drawn",
/// and `OH` and `O` with an implied hydrogen are the same molecule, so they must produce the
/// same string.
fn folded_hydrogens(mol: &Molecule) -> HashSet<u32> {
    mol.atoms
        .iter()
        .filter(|a| {
            if a.element != "H" || a.charge != 0 {
                return false;
            }
            let bonds = mol.bonds_for_atom(a.id);
            let [b] = bonds[..] else { return false };
            if b.order != BondOrder::Single {
                return false;
            }
            let other = if b.begin == a.id { b.end } else { b.begin };
            mol.atom_by_id(other).is_some_and(|o| !is_hydrogenic(&o.element))
        })
        .map(|a| a.id)
        .collect()
}

/// The molecule as SMILES sees it: folded hydrogens gone, every remaining atom reduced to the
/// token that will be written for it.
struct Skeleton {
    token: Vec<String>,
    /// Hydrogen count folded onto each atom (explicit neighbours plus implicit valence fill).
    #[cfg_attr(not(test), allow(dead_code))]
    h: Vec<u32>,
    /// [`Molecule`] atom id of each entry, so tests can talk about the original atoms.
    #[cfg_attr(not(test), allow(dead_code))]
    ids: Vec<u32>,
    /// `(atom, atom, order code)`.
    bonds: Vec<(usize, usize, u8)>,
    /// Per atom, the indices into `bonds` it takes part in.
    adj: Vec<Vec<usize>>,
}

impl Skeleton {
    fn build(mol: &Molecule) -> Self {
        let folded = folded_hydrogens(mol);
        let kept: Vec<&super::Atom> =
            mol.atoms.iter().filter(|a| !folded.contains(&a.id)).collect();
        let index_of: HashMap<u32, usize> =
            kept.iter().enumerate().map(|(i, a)| (a.id, i)).collect();

        let mut bonds = Vec::new();
        let mut adj = vec![Vec::new(); kept.len()];
        for b in &mol.bonds {
            // A bond onto a folded hydrogen is now part of an atom's hydrogen count, and one
            // referencing a missing atom is malformed; both are simply not written.
            let (Some(&i), Some(&j)) = (index_of.get(&b.begin), index_of.get(&b.end)) else {
                continue;
            };
            if i == j {
                continue;
            }
            let k = bonds.len();
            bonds.push((i, j, order_code(&b.order)));
            adj[i].push(k);
            adj[j].push(k);
        }

        let mut token = Vec::with_capacity(kept.len());
        let mut h = Vec::with_capacity(kept.len());
        for a in &kept {
            // Valence is spent on *every* bond the atom really has, folded hydrogens included —
            // a hydrogen that has been folded still occupies the bond it came in on, so counting
            // only the written bonds would hand an O–H drawn with its hydrogen visible a second,
            // imaginary one.
            let bond_sum_all = mol.bond_order_sum(a.id);
            let folded_count = mol
                .neighbor_atom_ids(a.id)
                .into_iter()
                .filter(|n| folded.contains(n))
                .count() as u32;
            let bond_sum_written = bond_sum_all - folded_count as i16;

            let implicit = if is_hydrogenic(&a.element) {
                0
            } else {
                normal_valence(&a.element)
                    .map(|v| ((v as i16 + a.charge as i16) - bond_sum_all).max(0) as u32)
                    .unwrap_or(0)
            };
            let total_h = folded_count + implicit;

            let (symbol, isotope) = split_isotope(&a.element);
            token.push(atom_token(symbol, isotope, a.charge, total_h, bond_sum_written));
            h.push(total_h);
        }

        Skeleton { token, h, ids: kept.iter().map(|a| a.id).collect(), bonds, adj }
    }

    fn len(&self) -> usize {
        self.token.len()
    }

    fn other(&self, bond: usize, from: usize) -> usize {
        let (i, j, _) = self.bonds[bond];
        if i == from { j } else { i }
    }

    fn order(&self, bond: usize) -> u8 {
        self.bonds[bond].2
    }

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
                for &b in &self.adj[i] {
                    let j = self.other(b, i);
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
}

// ═══════════════════════════════════════════════════════════════════════════════
// The search for the smallest string
// ═══════════════════════════════════════════════════════════════════════════════

/// How two candidate strings are compared, and therefore what "smallest" means.
///
/// Plain byte order would be a perfectly good canonical form but a poor-looking one: `(` sorts
/// below every letter, so the smallest string for ethanol is `C(C)O` rather than `CCO`, and a
/// chemist reading that reasonably wonders what went wrong. Sorting `(` *above* everything the
/// writer can emit says "branch as late as possible" instead, which recovers the shapes people
/// write by hand — `CCO`, `CC(C)O`, `C[N+](C)(C)C` — while remaining a plain lexicographic
/// order on a function of the string, so the minimum is just as well defined and just as
/// independent of atom order.
fn collate(s: &str) -> Vec<u8> {
    s.bytes().map(|b| if b == b'(' { 0xff } else { b }).collect()
}

/// Backtracking enumeration of every depth-first traversal of one component.
///
/// The depth-first stack is explicit, so one call to [`Search::search`] performs exactly one
/// *step* — choose a child, or finish an atom and hand control back to its parent — and
/// backtracking is then ordinary recursion with an undo on the way out. A traversal is complete
/// when the stack empties; its string is rendered and kept if it is the smallest so far.
struct Search<'a> {
    sk: &'a Skeleton,
    visited: Vec<bool>,
    /// Per bond: already classified as a tree bond or a ring closure.
    used: Vec<bool>,
    stack: Vec<usize>,
    /// Per atom, the bonds descended through, in the order they will be written.
    children: Vec<Vec<usize>>,
    /// Per atom, the ring-closure bonds touching it, in discovery order.
    rings: Vec<Vec<usize>>,
    root: usize,
    /// Best string so far, with its [`collate`] key alongside so the comparison and the
    /// prefix pruning do not have to recompute it.
    best: Option<(Vec<u8>, String)>,
    traversals: usize,
    exhausted: bool,
}

impl<'a> Search<'a> {
    fn new(sk: &'a Skeleton) -> Self {
        Search {
            sk,
            visited: vec![false; sk.len()],
            used: vec![false; sk.bonds.len()],
            stack: Vec::new(),
            children: vec![Vec::new(); sk.len()],
            rings: vec![Vec::new(); sk.len()],
            root: 0,
            best: None,
            traversals: 0,
            exhausted: false,
        }
    }

    fn component_smiles(&mut self, comp: &[usize]) -> String {
        self.best = None;

        // Every string starts with its root's token, so trying the smallest tokens first gets a
        // tight bound immediately and `cannot_beat_best` then throws away most of the remaining
        // roots without a single traversal.
        let mut roots = comp.to_vec();
        roots.sort_by(|&a, &b| self.sk.token[a].cmp(&self.sk.token[b]).then(a.cmp(&b)));

        for r in roots {
            if self.cannot_beat_best(r) {
                continue;
            }
            self.root = r;
            self.visited[r] = true;
            self.stack.push(r);
            self.search();
            self.stack.pop();
            self.visited[r] = false;
        }
        self.best.take().map(|(_, s)| s).unwrap_or_default()
    }

    /// Whether every string rooted at `r` is bound to lose to the best one found so far.
    ///
    /// Cheap and sound: the string starts with `token[r]`, so if that token already compares
    /// greater than the corresponding prefix of the incumbent, so does everything built on it.
    /// An atom token never contains `(`, so it is its own [`collate`] key.
    fn cannot_beat_best(&self, r: usize) -> bool {
        let Some((best, _)) = &self.best else { return false };
        let token = self.sk.token[r].as_bytes();
        let k = token.len().min(best.len());
        token[..k] > best[..k]
    }

    fn search(&mut self) {
        if self.traversals >= TRAVERSAL_BUDGET {
            self.exhausted = true;
            return;
        }
        let Some(&i) = self.stack.last() else {
            self.traversals += 1;
            let s = self.render();
            let key = collate(&s);
            if self.best.as_ref().is_none_or(|(b, _)| key < *b) {
                self.best = Some((key, s));
            }
            return;
        };

        let sk = self.sk;

        // Every unused bond from `i` to an atom already visited closes a ring. Classifying them
        // here — before each child choice, since a child's subtree can visit them — is what lets
        // an atom's ring digits be known by the time it is written.
        let mut closed: Vec<(usize, usize)> = Vec::new();
        for &b in &sk.adj[i] {
            if self.used[b] {
                continue;
            }
            let j = sk.other(b, i);
            if self.visited[j] {
                self.used[b] = true;
                closed.push((b, j));
            }
        }
        for &(b, j) in &closed {
            self.rings[i].push(b);
            self.rings[j].push(b);
        }

        let candidates: Vec<(usize, usize)> = sk.adj[i]
            .iter()
            .filter_map(|&b| {
                let j = sk.other(b, i);
                (!self.used[b] && !self.visited[j]).then_some((b, j))
            })
            .collect();

        if candidates.is_empty() {
            // `i` is finished: every one of its bonds is classified, so no later ring closure
            // can appear on it and it is safe to leave the stack for good.
            self.stack.pop();
            self.search();
            self.stack.push(i);
        } else {
            for (b, j) in candidates {
                self.used[b] = true;
                self.visited[j] = true;
                self.children[i].push(b);
                self.stack.push(j);
                self.search();
                self.stack.pop();
                self.children[i].pop();
                self.visited[j] = false;
                self.used[b] = false;
            }
        }

        for &(b, j) in closed.iter().rev() {
            self.rings[j].pop();
            self.rings[i].pop();
            self.used[b] = false;
        }
    }

    fn render(&self) -> String {
        // Where each atom falls in the written order. An atom's ring digits are sorted by it,
        // so that which digit an atom shows first is decided by the traversal and not by the
        // order the bonds happen to sit in `Molecule::bonds` — a ring-fusion carbon carries two
        // of them, and without this the same molecule built two ways writes `C2=CC=CC=C21` one
        // time and `C2=CC=CC=C12` the next.
        let mut pos = vec![usize::MAX; self.sk.len()];
        let mut next = 0usize;
        self.number(self.root, &mut pos, &mut next);

        let mut out = String::new();
        let mut open: HashMap<usize, u8> = HashMap::new();
        let mut free = [true; 100];
        self.write(self.root, &pos, &mut out, &mut open, &mut free);
        out
    }

    fn number(&self, i: usize, pos: &mut Vec<usize>, next: &mut usize) {
        pos[i] = *next;
        *next += 1;
        for &b in &self.children[i] {
            self.number(self.sk.other(b, i), pos, next);
        }
    }

    fn write(
        &self,
        i: usize,
        pos: &[usize],
        out: &mut String,
        open: &mut HashMap<usize, u8>,
        free: &mut [bool; 100],
    ) {
        out.push_str(&self.sk.token[i]);

        let mut rings = self.rings[i].clone();
        rings.sort_by_key(|&b| pos[self.sk.other(b, i)]);
        for &b in &rings {
            match open.remove(&b) {
                // Second time this ring bond is seen: repeat its number and hand it back. The
                // bond symbol was written at the opening end, so it is not repeated here.
                Some(n) => {
                    free[n as usize] = true;
                    push_ring_number(out, n);
                }
                None => {
                    let n = (1u8..100).find(|&k| free[k as usize]).unwrap_or(1);
                    free[n as usize] = false;
                    open.insert(b, n);
                    out.push_str(bond_symbol(self.sk.order(b)));
                    push_ring_number(out, n);
                }
            }
        }

        let kids = &self.children[i];
        for (k, &b) in kids.iter().enumerate() {
            let last = k + 1 == kids.len();
            if !last {
                out.push('(');
            }
            out.push_str(bond_symbol(self.sk.order(b)));
            self.write(self.sk.other(b, i), pos, out, open, free);
            if !last {
                out.push(')');
            }
        }
    }
}

/// Ring-closure numbers past 9 need the `%nn` escape, single digits must not use it.
fn push_ring_number(out: &mut String, n: u8) {
    if n < 10 {
        out.push((b'0' + n) as char);
    } else {
        out.push('%');
        out.push_str(&n.to_string());
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod parser;

#[cfg(test)]
mod tests {
    use super::*;
    use super::parser::parse;
    use crate::molecule::canonical::is_isomorphic;
    use crate::molecule::{BondOrder, BondStereo};
    use crate::testkit::all_fixtures;
    use std::collections::BTreeMap;

    // ── helpers ────────────────────────────────────────────────────────────────

    fn mol(atoms: &[(&str, i8)], bonds: &[(usize, usize, BondOrder)]) -> Molecule {
        let mut m = Molecule::default();
        let ids: Vec<u32> = atoms
            .iter()
            .enumerate()
            .map(|(k, (e, q))| m.add_atom((*e).to_string(), [k as f32, 0.0], *q))
            .collect();
        for (i, j, o) in bonds {
            m.add_bond(ids[*i], ids[*j], o.clone());
        }
        m
    }

    /// `mol` with the hydrogens [`to_smiles`] folds actually removed, which is the molecule a
    /// SMILES string can possibly describe.
    fn without_folded_hydrogens(m: &Molecule) -> Molecule {
        let mut out = m.clone();
        for h in folded_hydrogens(m) {
            out.remove_atom(h);
        }
        out
    }

    /// Molecular formula as the *model* counts it: every atom that exists, plus the hydrogens
    /// only valence implies. Folded hydrogens are already atoms, so they are not added twice.
    fn model_formula(m: &Molecule) -> BTreeMap<String, u32> {
        let sk = Skeleton::build(m);
        let folded = folded_hydrogens(m);
        let mut f: BTreeMap<String, u32> = BTreeMap::new();
        for a in &m.atoms {
            *f.entry(a.element.clone()).or_default() += 1;
        }
        for (k, &id) in sk.ids.iter().enumerate() {
            let folded_here = m
                .neighbor_atom_ids(id)
                .into_iter()
                .filter(|n| folded.contains(n))
                .count() as u32;
            *f.entry("H".to_string()).or_default() += sk.h[k] - folded_here;
        }
        f.retain(|_, v| *v > 0);
        f
    }

    /// Rebuild `m` with its atoms added in the order given by `perm`, and every bond stored
    /// end-swapped, so no atom id, index or endpoint order survives.
    fn rebuild(m: &Molecule, perm: &[usize]) -> Molecule {
        let mut out = Molecule::default();
        let mut id_map = HashMap::new();
        for &k in perm {
            let a = &m.atoms[k];
            let new = out.add_atom(a.element.clone(), [a.pos[1], a.pos[0]], a.charge);
            if a.element == "H" {
                out.atom_by_id_mut(new).expect("just added").group_h = a.group_h;
            }
            id_map.insert(a.id, new);
        }
        for b in m.bonds.iter().rev() {
            let nb = out.add_bond(id_map[&b.end], id_map[&b.begin], b.order.clone());
            if let Some(bond) = out.bond_by_id_mut(nb) {
                bond.stereo = b.stereo.clone();
            }
        }
        out
    }

    /// A deterministic shuffle — a plain LCG, so a failure is reproducible from its seed.
    fn shuffled_indices(n: usize, seed: u64) -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).collect();
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        for i in (1..n).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let j = (state >> 33) as usize % (i + 1);
            v.swap(i, j);
        }
        v
    }

    // ── the round trip ─────────────────────────────────────────────────────────

    /// The load-bearing test: write every fixture as SMILES, read it back with an independent
    /// parser, and demand that what comes back is the same molecule.
    ///
    /// Three separate claims, because graph identity alone would not catch a hydrogen bug —
    /// implicit hydrogens are not atoms, so a molecule with the wrong ones is still isomorphic
    /// to the right one:
    ///
    /// 1. **Skeleton** — [`is_isomorphic`] against the original (minus folded hydrogens).
    /// 2. **Hydrogens** — the formula the *parser* derives, applying SMILES' own valence rules
    ///    to what was written, must equal the formula the *model* derives from
    ///    [`normal_valence`]. The two arrive at the count by different routes and different
    ///    valence tables, so agreement on all 125 structures is real evidence rather than a
    ///    tautology.
    /// 3. **Idempotence** — re-writing the parsed molecule reproduces the string, so the
    ///    canonical form is a fixed point and not an artefact of how the fixture was stored.
    #[test]
    fn every_fixture_survives_a_round_trip_through_a_parser() {
        let fixtures = all_fixtures();
        assert_eq!(fixtures.len(), 125, "expected 125 fixtures on disk");

        for (name, m) in &fixtures {
            let s = to_smiles(m);
            let (parsed, _) = parse(&s);

            assert!(
                is_isomorphic(&parsed, &without_folded_hydrogens(m)),
                "{name}: {s} did not read back as the same structure",
            );
            assert_eq!(
                parser::formula(&s),
                model_formula(m),
                "{name}: {s} does not carry the same hydrogens the editor counts",
            );
            assert_eq!(to_smiles(&parsed), s, "{name}: {s} is not a fixed point");
        }
    }

    /// The whole point of a canonical form: the string is a property of the molecule, not of
    /// the ids it happens to carry or the order it was built in.
    #[test]
    fn smiles_does_not_depend_on_how_the_molecule_was_built() {
        for (name, m) in all_fixtures() {
            let want = to_smiles(&m);
            let n = m.atoms.len();

            let reversed: Vec<usize> = (0..n).rev().collect();
            assert_eq!(to_smiles(&rebuild(&m, &reversed)), want, "{name}: reversed order");

            for seed in [1u64, 7, 2024, 99991] {
                let perm = shuffled_indices(n, seed);
                assert_eq!(
                    to_smiles(&rebuild(&m, &perm)),
                    want,
                    "{name}: shuffled with seed {seed}",
                );
            }
        }
    }

    /// The search must stay a long way inside its budget, or the "smallest string over every
    /// traversal" definition quietly degrades into "smallest of the first 200 000".
    #[test]
    fn no_fixture_comes_near_the_traversal_budget() {
        let mut worst = (0usize, String::new());
        for (name, m) in all_fixtures() {
            let g = generate(&m);
            assert!(!g.exhausted, "{name} exhausted the traversal budget");
            if g.traversals > worst.0 {
                worst = (g.traversals, name);
            }
        }
        assert!(
            worst.0 < TRAVERSAL_BUDGET / 10,
            "{} needed {} traversals, uncomfortably close to the {TRAVERSAL_BUDGET} budget",
            worst.1,
            worst.0,
        );
    }

    // ── hydrogens ──────────────────────────────────────────────────────────────

    /// SMILES must agree with what the canvas shows. For every bonded atom of every fixture —
    /// none of which contain hydrogen atoms — the count written into the SMILES is exactly the
    /// count [`displayed_h_count`](crate::widget::draw::displayed_h_count) puts in the label,
    /// so `CH₃` on screen and `C` with three implicit hydrogens are the same statement.
    #[test]
    fn implicit_hydrogen_matches_the_drawn_label() {
        use crate::widget::draw::displayed_h_count;
        for (name, m) in all_fixtures() {
            let sk = Skeleton::build(&m);
            for (k, &id) in sk.ids.iter().enumerate() {
                if m.bonds_for_atom(id).is_empty() {
                    continue; // see `a_lone_atom_still_gets_its_hydrogens`
                }
                assert_eq!(
                    sk.h[k],
                    displayed_h_count(&m, id) as u32,
                    "{name}: atom {id} ({}) disagrees with its label",
                    m.atom_by_id(id).expect("id came from the molecule").element,
                );
            }
        }
    }

    /// A hydrogen drawn as its own vertex and a hydrogen only implied are the same molecule, so
    /// they must give the same string — whichever way the fragment library or the user produced
    /// it. The relation to the label is exact: the SMILES count is the label's count plus the
    /// hydrogens the label had no room to show.
    #[test]
    fn drawn_hydrogens_fold_into_their_neighbours_count() {
        use crate::widget::draw::{displayed_h_count, is_folded_h};

        // Ethanol, written CCO, with the hydroxyl hydrogen drawn by hand.
        let implied = mol(&[("C", 0), ("C", 0), ("O", 0)], &[(0, 1, BondOrder::Single), (1, 2, BondOrder::Single)]);
        let mut drawn = implied.clone();
        let h = drawn.add_atom("H".to_string(), [3.0, 0.0], 0);
        drawn.add_bond(drawn.atoms[2].id, h, BondOrder::Single);

        assert_eq!(to_smiles(&implied), "CCO");
        assert_eq!(to_smiles(&drawn), "CCO", "a drawn O–H is still an alcohol");

        // The label shows nothing on that oxygen — the hydrogen has a vertex of its own — yet
        // the SMILES has to carry it, and the difference is exactly the unfolded hydrogen.
        let o = drawn.atoms[2].id;
        assert_eq!(displayed_h_count(&drawn, o), 0);
        assert!(!is_folded_h(&drawn, h), "a hand-placed hydrogen is drawn, not folded");
        let sk = Skeleton::build(&drawn);
        let k = sk.ids.iter().position(|&i| i == o).expect("the oxygen is written");
        assert_eq!(sk.h[k], 1);
    }

    /// Hydrogens that are not somebody's substituent stay atoms, because a count cannot say
    /// where they are.
    #[test]
    fn hydrogens_that_are_not_substituents_stay_atoms() {
        let h2 = mol(&[("H", 0), ("H", 0)], &[(0, 1, BondOrder::Single)]);
        assert_eq!(to_smiles(&h2), "[H][H]");

        let lone = mol(&[("H", 0)], &[]);
        assert_eq!(to_smiles(&lone), "[H]", "a bare hydrogen never gains hydrogens of its own");

        let proton = mol(&[("H", 1)], &[]);
        assert_eq!(to_smiles(&proton), "[H+]");

        // A bridging hydrogen has two bonds, so no single neighbour can claim it. Each boron
        // then carries the two hydrogens its own valence implies, which `B` already says.
        // (Two bonds is over-valence for hydrogen and the editor draws its warning ring on it;
        // the writer's job is to say so faithfully, not to quietly fold the bridge away.)
        let bridge = mol(
            &[("B", 0), ("H", 0), ("B", 0)],
            &[(0, 1, BondOrder::Single), (1, 2, BondOrder::Single)],
        );
        assert_eq!(to_smiles(&bridge), "B[H]B");
    }

    /// An atom with no bonds is a molecule, not a cursor decoration. The label deliberately
    /// shows a lone O as "O" rather than "OH₂" — see
    /// [`displayed_h_count`](crate::widget::draw::displayed_h_count) — but SMILES has no such
    /// thing as an unfinished drawing, and the counter-ion in the `ammonium_acetate` fixture is
    /// a real ammonium. This is the one place the two counts part company, on purpose.
    #[test]
    fn a_lone_atom_still_gets_its_hydrogens() {
        use crate::widget::draw::displayed_h_count;
        let m = mol(&[("N", 1)], &[]);
        assert_eq!(displayed_h_count(&m, m.atoms[0].id), 0, "the label shows a bare N⁺");
        assert_eq!(to_smiles(&m), "[NH4+]", "the chemistry is ammonium");
        assert_eq!(to_smiles(&mol(&[("C", 0)], &[])), "C", "and a lone carbon is methane");
    }

    // ── spelling ───────────────────────────────────────────────────────────────

    #[test]
    fn charges_are_written_in_brackets() {
        assert_eq!(to_smiles(&mol(&[("O", -1), ("C", 0)], &[(0, 1, BondOrder::Single)])), "C[O-]");
        assert_eq!(to_smiles(&mol(&[("N", 1)], &[])), "[NH4+]");
        assert_eq!(to_smiles(&mol(&[("Fe", 2)], &[])), "[Fe+2]", "past ±1 the number is spelled");
        assert_eq!(to_smiles(&mol(&[("S", -2)], &[])), "[S-2]");
    }

    /// Deuterium is a label in this editor and an isotope in SMILES, and it must never be
    /// folded away — `[2H]` on a carbon is a labelling experiment, `C` is not.
    #[test]
    fn deuterium_is_written_as_an_isotope() {
        let cd = mol(&[("C", 0), ("D", 0)], &[(0, 1, BondOrder::Single)]);
        assert_eq!(to_smiles(&cd), "C[2H]", "deuterium stays an atom");

        let ch = mol(&[("C", 0), ("H", 0)], &[(0, 1, BondOrder::Single)]);
        assert_eq!(to_smiles(&ch), "C", "an ordinary hydrogen does not");

        assert_eq!(
            to_smiles(&mol(&[("C", 0), ("T", 0)], &[(0, 1, BondOrder::Single)])),
            "C[3H]",
        );

        // CD₃ is CD₃, not CH₃: the deuteriums neither fold nor turn into hydrogen count.
        let cd3 = mol(
            &[("C", 0), ("D", 0), ("D", 0), ("D", 0)],
            &[(0, 1, BondOrder::Single), (0, 2, BondOrder::Single), (0, 3, BondOrder::Single)],
        );
        assert_eq!(to_smiles(&cd3), "C([2H])([2H])[2H]");
    }

    /// Outside the organic subset there are no implicit hydrogens to infer, so the atom has to
    /// spell out both itself and its hydrogens.
    #[test]
    fn elements_outside_the_organic_subset_are_bracketed() {
        let sih3 = mol(
            &[("C", 0), ("Si", 0)],
            &[(0, 1, BondOrder::Single)],
        );
        assert_eq!(to_smiles(&sih3), "C[SiH3]");

        assert_eq!(to_smiles(&mol(&[("Li", 0)], &[])), "[Li]", "no valence, so no hydrogens");
        assert_eq!(
            to_smiles(&mol(&[("C", 0), ("Mg", 0), ("Br", 0)],
                &[(0, 1, BondOrder::Single), (1, 2, BondOrder::Single)])),
            "Br[Mg]C",
        );
    }

    /// A label the periodic table does not contain has no SMILES spelling. The wildcard is the
    /// honest answer; inventing `[Xx]` would produce a string no reader can accept.
    #[test]
    fn an_unknown_label_becomes_a_wildcard() {
        let m = mol(&[("C", 0), ("Xx", 0)], &[(0, 1, BondOrder::Single)]);
        assert_eq!(to_smiles(&m), "C[*]");
    }

    /// The organic subset is only usable when a reader would infer the same hydrogen count, and
    /// the two valence tables disagree often enough for this to matter: sulfur.
    #[test]
    fn brackets_appear_wherever_inference_would_get_it_wrong() {
        // Sulfoxide: our S has valence 2 and so no hydrogens at four bond-orders, and SMILES
        // agrees (its next valence up is 4). Bare `S` is safe.
        let dmso = mol(
            &[("C", 0), ("S", 0), ("O", 0), ("C", 0)],
            &[(0, 1, BondOrder::Single), (1, 2, BondOrder::Double), (1, 3, BondOrder::Single)],
        );
        assert_eq!(to_smiles(&dmso), "CS(=O)C");

        // A sulfanyl radical has three bond-orders, where SMILES would read a bare `S` as SH.
        let radical = mol(
            &[("C", 0), ("S", 0), ("O", 0)],
            &[(0, 1, BondOrder::Single), (1, 2, BondOrder::Double)],
        );
        assert_eq!(to_smiles(&radical), "C[S]=O", "bare `S` here would grow a hydrogen");

        // Thiol: one bond, one hydrogen, and both tables agree.
        let thiol = mol(&[("C", 0), ("S", 0)], &[(0, 1, BondOrder::Single)]);
        assert_eq!(to_smiles(&thiol), "CS");
    }

    #[test]
    fn disconnected_components_are_dot_separated_and_sorted() {
        let mut m = mol(&[("O", 0), ("C", 0)], &[(0, 1, BondOrder::Single)]);
        let na = m.add_atom("Na".to_string(), [9.0, 9.0], 1);
        let _ = na;
        assert_eq!(to_smiles(&m), "CO.[Na+]");

        // Component order in `mol.atoms` must not show through.
        let flipped = rebuild(&m, &[2, 1, 0]);
        assert_eq!(to_smiles(&flipped), "CO.[Na+]");
    }

    #[test]
    fn an_empty_molecule_has_an_empty_smiles() {
        assert_eq!(to_smiles(&Molecule::default()), "");
    }

    /// Ring-closure numbers are a scarce resource: they are handed back when a ring closes and
    /// reused, and once more than nine are open at a time the `%nn` escape is required. K₈ — a
    /// carbon bonded to every other carbon — opens eleven, which no drawn structure would, and
    /// that is the point: it exercises the escape and the recycling in one go.
    #[test]
    fn ring_closure_numbers_are_recycled_and_survive_going_past_nine() {
        let n = 8;
        let mut m = Molecule::default();
        let ids: Vec<u32> =
            (0..n).map(|k| m.add_atom("C".to_string(), [k as f32, 0.0], 0)).collect();
        for i in 0..n {
            for j in (i + 1)..n {
                m.add_bond(ids[i], ids[j], BondOrder::Single);
            }
        }

        let s = to_smiles(&m);
        assert!(s.contains("%1"), "eleven simultaneous rings must use the %nn escape: {s}");
        let (parsed, _) = parse(&s);
        assert!(is_isomorphic(&parsed, &m), "{s} did not read back as K8");
        assert_eq!(to_smiles(&parsed), s);

        // Recycling: cubane-shaped or not, a molecule with many *sequential* rings must keep
        // reusing the low numbers rather than counting upwards forever.
        let mut chain = Molecule::default();
        let mut prev: Option<[u32; 2]> = None;
        for k in 0..12 {
            let a = chain.add_atom("C".to_string(), [k as f32, 0.0], 0);
            let b = chain.add_atom("C".to_string(), [k as f32, 1.0], 0);
            let c = chain.add_atom("C".to_string(), [k as f32, 2.0], 0);
            chain.add_bond(a, b, BondOrder::Single);
            chain.add_bond(b, c, BondOrder::Single);
            chain.add_bond(c, a, BondOrder::Single);
            if let Some([_, pc]) = prev {
                chain.add_bond(pc, a, BondOrder::Single);
            }
            prev = Some([a, c]);
        }
        let s = to_smiles(&chain);
        assert!(!s.contains('%'), "twelve rings that never overlap need one digit: {s}");
        let (parsed, _) = parse(&s);
        assert!(is_isomorphic(&parsed, &chain));
    }

    /// Stereochemistry is not encoded, and this pins that as a decision rather than an
    /// oversight: a wedge changes the drawing and `canonical.rs`' idea of identity, but not the
    /// SMILES. If stereo is ever implemented, this test is the one that should fail first.
    #[test]
    fn wedges_do_not_reach_the_smiles() {
        let plain = mol(
            &[("C", 0), ("C", 0), ("O", 0), ("N", 0)],
            &[(0, 1, BondOrder::Single), (1, 2, BondOrder::Single), (1, 3, BondOrder::Single)],
        );
        let mut wedged = plain.clone();
        wedged.bonds[1].stereo = BondStereo::WedgeUp;
        let mut hashed = plain.clone();
        hashed.bonds[1].stereo = BondStereo::WedgeDown;

        assert!(!is_isomorphic(&plain, &wedged), "the model does distinguish them");
        assert_eq!(to_smiles(&plain), to_smiles(&wedged), "but the SMILES does not");
        assert_eq!(to_smiles(&wedged), to_smiles(&hashed));
    }

    /// Not an assertion — `cargo test --lib smiles::tests::dump -- --ignored --nocapture` prints
    /// every fixture's SMILES, which is how the pinned strings above were arrived at and the
    /// quickest way to see what a change to the writer actually did.
    #[test]
    #[ignore]
    fn dump() {
        for (name, m) in all_fixtures() {
            let g = generate(&m);
            println!("{name:24} {:>7} traversals  {}", g.traversals, g.smiles);
        }
    }

    /// Representative structures, pinned exactly. Kekulé aromatics, a charge, a salt, a
    /// hypervalent sulfur and a fused bicycle — the strings a reader can check against any
    /// toolkit.
    #[test]
    fn representative_structures_have_their_expected_strings() {
        let cases = [
            ("benzene", "C1=CC=CC=C1"),
            ("pyridine", "C1=CC=CC=N1"),
            ("phenol", "C1=CC=CC=C1O"),
            ("naphthalene", "C12=CC=CC=C1C=CC=C2"),
            ("caffeine", "C12=C(C(=O)N(C)C(=O)N1C)N(C)C=N2"),
            ("acetic_acid", "CC(=O)O"),
            ("acetate", "CC(=O)[O-]"),
            ("ammonium_acetate", "CC(=O)[O-].[NH4+]"),
            ("sulfate", "O=S(=O)([O-])[O-]"),
            ("nitrobenzene", "C1=CC=CC=C1[N+](=O)[O-]"),
            ("tms_ether", "CCO[Si](C)(C)C"),
            ("glucose_ring", "C1(CO)C(C(C(C(O)O1)O)O)O"),
            ("tetramethylammonium", "C[N+](C)(C)C"),
            ("carbon_tetrachloride", "ClC(Cl)(Cl)Cl"),
        ];
        for (name, want) in cases {
            let m = crate::testkit::load_fixture(name);
            assert_eq!(to_smiles(&m), want, "{name}");
        }
    }
}
