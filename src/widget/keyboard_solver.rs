//! Search for a keyboard-only key sequence that builds a given structure.
//!
//! [`keyboard_tests`](super::keyboard_tests) holds hand-authored witnesses; there are 123
//! structures to witness, and hand-authoring them is both tedious and brittle. This module
//! finds them instead, using the **real editor as the simulator**: a candidate sequence is
//! "correct" exactly when driving [`ChemStructEditor`] with it produces something
//! [`is_isomorphic`] to the target. Nothing here models the editor's semantics twice.
//!
//! ## Shape of the search
//!
//! A blind search over single chords does not work, for two reasons:
//!
//! - Arrow keys do not change the molecule at all, so they fan out enormously while making no
//!   progress that any goal-directed score can see.
//! - Replaying a whole sequence from scratch for every candidate is quadratic in depth.
//!
//! So the unit of search is a **move**: an arrow-key navigation prefix followed by exactly one
//! mutating key. Navigation is enumerated up front (breadth-first over
//! [`navigate_direction`](super::interact::navigate_direction), the editor's own rule), which
//! collapses the "free" branching into the move itself. And each node caches a [`State`] — the
//! only editor fields a keyboard-only continuation can read — so expanding a node replays just
//! that move's two or three frames.
//!
//! ## What prunes the space
//!
//! [`fit`] asks whether the partially-built molecule can still *become* the target: every built
//! atom must map to a distinct target atom, with every built bond present in the target and no
//! atom already over-connected. A neutral carbon is treated as a wildcard because the normal
//! idiom is to lay down a carbon skeleton and retarget atoms afterwards (`c 1 1 O` is ethanol),
//! so intermediate states are legitimately all-carbon. That makes the check a strong but
//! *deliberately incomplete* filter — it is a search heuristic, not a proof of unreachability.
//!
//! The same mapping then drives move generation: only atoms the target still wants something
//! from (a missing neighbour, a different element, a different charge) are worth acting on, and
//! only with the key the target actually calls for. That is what keeps the branching factor
//! near 20 instead of near 200.
//!
//! ## Alphabet
//!
//! Restricted per target — a structure with no five-ring never tries `5`. Two entries are
//! macros rather than single keys, both for elements with no bare-atom binding:
//! [`element_key`]'s `s Delete` (insert S–H, strip the H) and [`label_move`]'s
//! `` ` text:<elem> Enter `` (type the element into the label editor). The label route reaches
//! *any* element and is therefore the general answer, but it is offered **only** where no key
//! exists: it costs three tokens against one, and letting it compete everywhere would multiply
//! the branching factor across the whole goal set to reach states the cheaper key already
//! reaches.
//!
//! ## Determinism
//!
//! No randomness, and no iteration over a `HashMap` whose order could reach the output: every
//! ranking ends in a comparison on the sequence text itself.

use super::interact::navigate_direction;
use super::ChemStructEditor;
use crate::molecule::canonical::{canonical_key, is_isomorphic};
use crate::molecule::{BondOrder, Molecule};
use crate::testkit::kbd_frame;
use std::collections::{HashMap, HashSet, VecDeque};

// ═══════════════════════════════════════════════════════════════════════════════
// Simulator
// ═══════════════════════════════════════════════════════════════════════════════

/// Everything a keyboard-only continuation can depend on.
///
/// Snapshots are only ever taken *between* moves, and a move always leaves the bond target
/// cleared (see [`bond_move`]), so `hotspot_bond` needs no slot here. Undo history, selection
/// and label state are unreachable from the keys this search uses.
#[derive(Clone)]
struct State {
    molecule: Molecule,
    hotspot_atom: Option<u32>,
    last_mouse_mol: [f32; 2],
}

/// One editor plus one egui context, reused for every candidate.
///
/// Building a fresh [`egui::Context`] per candidate dominates the runtime (font atlas), and
/// `ChemStructEditor` is not `Clone` (it owns a cleanup worker handle), so the search restores
/// a [`State`] into a single long-lived editor instead of cloning one.
struct Sim {
    ctx: egui::Context,
    editor: ChemStructEditor,
}

impl Sim {
    fn new() -> Self {
        Sim { ctx: egui::Context::default(), editor: ChemStructEditor::default() }
    }

    fn restore(&mut self, s: &State) {
        self.editor.molecule = s.molecule.clone();
        self.editor.hotspot_atom = s.hotspot_atom;
        self.editor.hotspot_bond = None;
        self.editor.last_mouse_mol = s.last_mouse_mol;
        // Left to grow, the undo stack would clone the molecule once per frame for the whole
        // search. Nothing here presses Ctrl+Z.
        self.editor.undo_stack.clear();
        self.editor.redo_stack.clear();
        self.editor.selected_atoms.clear();
    }

    /// Drive a whitespace-separated chord sequence. Goes through [`kbd_frame`], so the
    /// no-fabricated-hover post-condition holds for every frame the search ever runs.
    fn run(&mut self, seq: &str) {
        for spec in seq.split_whitespace() {
            kbd_frame(&self.ctx, &mut self.editor, spec);
        }
    }

    fn state(&self) -> State {
        State {
            molecule: self.editor.molecule.clone(),
            hotspot_atom: self.editor.hotspot_atom,
            last_mouse_mol: self.editor.last_mouse_mol,
        }
    }
}

/// Build a molecule by typing `seq` from an empty canvas — the independent check that a
/// sequence really works, with no state carried over from the search.
pub(super) fn build(seq: &str) -> Molecule {
    let mut sim = Sim::new();
    sim.run(seq);
    sim.editor.molecule
}

// ═══════════════════════════════════════════════════════════════════════════════
// Graph view
// ═══════════════════════════════════════════════════════════════════════════════

/// Index-space adjacency, which is what both the fit search and the scoring want.
struct Graph {
    elem: Vec<String>,
    charge: Vec<i8>,
    /// `adj[i]` = `(neighbour index, bond order code)`.
    adj: Vec<Vec<(usize, u8)>>,
    bonds: usize,
}

fn order_code(o: &BondOrder) -> u8 {
    match o {
        BondOrder::Single => 1,
        BondOrder::Double => 2,
        BondOrder::Triple => 3,
    }
}

impl Graph {
    fn of(mol: &Molecule) -> Graph {
        let index: HashMap<u32, usize> =
            mol.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        let mut adj = vec![Vec::new(); mol.atoms.len()];
        let mut bonds = 0;
        for b in &mol.bonds {
            let (Some(&i), Some(&j)) = (index.get(&b.begin), index.get(&b.end)) else { continue };
            let code = order_code(&b.order);
            adj[i].push((j, code));
            adj[j].push((i, code));
            bonds += 1;
        }
        Graph {
            elem: mol.atoms.iter().map(|a| a.element.clone()).collect(),
            charge: mol.atoms.iter().map(|a| a.charge).collect(),
            adj,
            bonds,
        }
    }

    fn len(&self) -> usize {
        self.elem.len()
    }

    fn deg(&self, i: usize) -> usize {
        self.adj[i].len()
    }

    fn order_between(&self, i: usize, j: usize) -> Option<u8> {
        self.adj[i].iter().find(|&&(n, _)| n == j).map(|&(_, o)| o)
    }

    /// Atom indices ordered so that, after the first, each one is adjacent to an earlier one
    /// where possible. Keeps the partial map connected, which is what makes [`fit`] prune.
    fn connectivity_order(&self) -> Vec<usize> {
        let mut placed = vec![false; self.len()];
        let mut out = Vec::with_capacity(self.len());
        for seed in 0..self.len() {
            if placed[seed] {
                continue;
            }
            placed[seed] = true;
            out.push(seed);
            while let Some(next) =
                (0..self.len()).find(|&c| !placed[c] && self.adj[c].iter().any(|&(j, _)| placed[j]))
            {
                placed[next] = true;
                out.push(next);
            }
        }
        out
    }

    fn components(&self) -> usize {
        let mut seen = vec![false; self.len()];
        let mut n = 0;
        for start in 0..self.len() {
            if seen[start] {
                continue;
            }
            n += 1;
            let mut stack = vec![start];
            seen[start] = true;
            while let Some(i) = stack.pop() {
                for &(j, _) in &self.adj[i] {
                    if !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        n
    }

    /// Sizes of the smallest cycle through each bond — the ring sizes a builder has to produce.
    fn ring_sizes(&self) -> Vec<usize> {
        let mut sizes = Vec::new();
        for i in 0..self.len() {
            for &(j, _) in &self.adj[i] {
                if j <= i {
                    continue;
                }
                if let Some(len) = self.shortest_path_avoiding(i, j) {
                    sizes.push(len + 1);
                }
            }
        }
        sizes.sort_unstable();
        sizes.dedup();
        sizes
    }

    /// Shortest i→j path length that does not use the direct i–j edge.
    fn shortest_path_avoiding(&self, i: usize, j: usize) -> Option<usize> {
        let mut dist = vec![usize::MAX; self.len()];
        dist[i] = 0;
        let mut q = VecDeque::from([i]);
        while let Some(u) = q.pop_front() {
            for &(v, _) in &self.adj[u] {
                if u == i && v == j {
                    continue; // the edge under test
                }
                if v == j {
                    return Some(dist[u] + 1);
                }
                if dist[v] == usize::MAX {
                    dist[v] = dist[u] + 1;
                    q.push_back(v);
                }
            }
        }
        None
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Fit: can this partial build still become the target?
// ═══════════════════════════════════════════════════════════════════════════════

/// A mapping of every built atom onto a distinct target atom, plus how much of it is already
/// chemically right.
struct Fit {
    /// `map[built_index] = target_index`.
    map: Vec<usize>,
    /// Built atoms whose element **and** charge already match the target's.
    elem_ok: usize,
    /// Built bonds whose order already matches the target's.
    order_ok: usize,
}

/// Backtracking limit. Exhausting it means "keep the best mapping found so far" rather than
/// "no mapping", so a hard case degrades into a weaker heuristic instead of a wrong prune.
const FIT_BUDGET: u32 = 60_000;

/// Whether `built` can still be grown into `target`, and the best correspondence found.
///
/// A **neutral carbon is a wildcard**: every element key retargets an atom in place, and the
/// natural idiom lays down a carbon skeleton first (`c 1 1 O`), so an all-carbon intermediate
/// must not be rejected. Non-carbon built atoms are held to their element — retargeting an
/// already-placed heteroatom is legal in the editor but never a route worth searching.
///
/// Bond *order* is ignored for admissibility (`1`/`2`/`3` rewrite it at any time) and only
/// scored.
fn fit(b: &Graph, t: &Graph) -> Option<Fit> {
    if b.len() > t.len() || b.bonds > t.bonds {
        return None;
    }
    let order = b.connectivity_order();
    let mut map = vec![usize::MAX; b.len()];
    let mut used = vec![false; t.len()];
    let mut best: Option<Fit> = None;
    let mut budget = FIT_BUDGET;
    fit_rec(b, t, &order, 0, &mut map, &mut used, &mut best, &mut budget);
    best
}

fn fit_rec(
    b: &Graph,
    t: &Graph,
    order: &[usize],
    depth: usize,
    map: &mut Vec<usize>,
    used: &mut Vec<bool>,
    best: &mut Option<Fit>,
    budget: &mut u32,
) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;

    if depth == order.len() {
        let cand = score_fit(b, t, map);
        let better = best.as_ref().is_none_or(|prev| {
            (cand.elem_ok, cand.order_ok) > (prev.elem_ok, prev.order_ok)
        });
        if better {
            // A perfect correspondence cannot be improved on; stop paying for the rest.
            if cand.elem_ok == b.len() && cand.order_ok == b.bonds {
                *budget = 0;
            }
            *best = Some(cand);
        }
        return;
    }

    let i = order[depth];
    for j in 0..t.len() {
        if used[j] || !compatible(b, t, map, i, j) {
            continue;
        }
        map[i] = j;
        used[j] = true;
        fit_rec(b, t, order, depth + 1, map, used, best, budget);
        map[i] = usize::MAX;
        used[j] = false;
        if *budget == 0 {
            return;
        }
    }
}

fn compatible(b: &Graph, t: &Graph, map: &[usize], i: usize, j: usize) -> bool {
    if b.elem[i] != "C" && b.elem[i] != t.elem[j] {
        return false;
    }
    // An atom that already has more neighbours than the target's can never be repaired: the
    // keyboard has no way to remove a bond without also dropping the hotspot.
    if b.deg(i) > t.deg(j) {
        return false;
    }
    b.adj[i].iter().all(|&(n, _)| {
        let mn = map[n];
        mn == usize::MAX || t.order_between(mn, j).is_some()
    })
}

fn score_fit(b: &Graph, t: &Graph, map: &[usize]) -> Fit {
    let elem_ok = (0..b.len())
        .filter(|&i| b.elem[i] == t.elem[map[i]] && b.charge[i] == t.charge[map[i]])
        .count();
    let mut order_ok = 0;
    for i in 0..b.len() {
        for &(j, o) in &b.adj[i] {
            if j > i && t.order_between(map[i], map[j]) == Some(o) {
                order_ok += 1;
            }
        }
    }
    Fit { map: map.to_vec(), elem_ok, order_ok }
}

/// Estimated keys still to press. Lower is closer; zero only at the goal.
///
/// Each term counts something that provably needs at least one more key: an atom that does not
/// exist yet, an atom wearing the wrong element or charge, a bond with the wrong order, and a
/// ring closure that no atom-adding key will supply for free.
fn remaining(b: &Graph, t: &Graph, f: &Fit) -> usize {
    let missing_atoms = t.len() - b.len();
    let missing_bonds = t.bonds - b.bonds;
    missing_atoms
        + (b.len() - f.elem_ok)
        + (b.bonds - f.order_ok)
        + missing_bonds.saturating_sub(missing_atoms)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Per-target key alphabet
// ═══════════════════════════════════════════════════════════════════════════════

/// The key that turns the hotspot atom into a bare atom of `elem`.
///
/// Lowercase heteroatom keys insert *groups* carrying explicit hydrogens (OH, NH2, SH) and the
/// fixtures have none, so only the bare forms are listed.
///
/// Sulfur is the odd one out: there is no bare-S key, so `s` inserts S–H and parks the hotspot
/// on that H, which then has to be deleted. That works because `Delete` steps the hotspot to a
/// former neighbour of the atom it removes — here, the sulfur itself — so the macro leaves the
/// hotspot exactly where an ordinary element key would have.
///
/// P and Si have no key of any kind (`p` → PH2, `S` → SiH3, both with more hydrogens than one
/// `Delete` can strip). They go through [`label_move`] instead.
fn element_key(elem: &str) -> Option<&'static str> {
    Some(match elem {
        "C" => "c",
        "O" => "O",
        "N" => "N",
        "F" => "f",
        "Cl" => "l",
        "Br" => "b",
        "I" => "i",
        "S" => "s Delete",
        _ => return None,
    })
}

/// Set the hotspot atom's element by typing it into the label editor.
///
/// The general escape hatch: `` ` `` opens the editor with the current element pre-selected,
/// the typed text replaces it, and Enter writes it straight into `atom.element`. That reaches
/// *any* element, including the ones with no binding at all.
///
/// Deliberately **not** offered for elements [`element_key`] already covers. It costs three
/// tokens against one and would multiply the branching factor for every structure in the goal
/// set, to reach states the cheaper key already reaches.
fn label_move(elem: &str) -> String {
    format!("` text:{elem} Enter")
}

/// The atom key that grows an `n`-membered carbocycle out of the hotspot atom.
fn ring_key(n: usize) -> Option<&'static str> {
    Some(match n {
        3 => "v",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        _ => return None,
    })
}

/// Keys worth trying for one particular target.
///
/// Restricting up front is what makes the search tractable: a target with no triple bond never
/// tries `3`, one with no five-ring never tries `5`, and so on.
struct Plan {
    /// Applied to an atom the target still wants a neighbour on.
    growth: Vec<&'static str>,
    /// Applied to a bond whose order is wrong.
    orders: Vec<&'static str>,
    /// Applied to a bond both of whose atoms want more neighbours (ring fusion).
    fusions: Vec<&'static str>,
    /// Applied on an empty canvas, or after `Space`, to start a component.
    seeds: Vec<&'static str>,
    components: usize,
    charged: bool,
}

impl Plan {
    fn for_target(t: &Graph) -> Plan {
        let has = |e: &str| t.elem.iter().any(|x| x == e);
        let rings = t.ring_sizes();

        let mut growth = vec!["1"];
        if has("O") {
            growth.push("2"); // Oxo: the only key that adds a doubly-bonded oxygen
        }
        for &n in &rings {
            if let Some(k) = ring_key(n) {
                growth.push(k);
            }
        }
        if rings.contains(&6) {
            growth.push("a"); // benzene, i.e. a 6-ring arriving pre-Kekuléd
        }

        let mut orders = vec!["1"];
        if t.adj.iter().flatten().any(|&(_, o)| o == 2) {
            orders.push("2");
        }
        if t.adj.iter().flatten().any(|&(_, o)| o == 3) {
            orders.push("3");
        }

        // Ring fusion only pays when two rings actually share a bond, which shows up as an atom
        // in more than one ring — cheaply approximated by "there is a ring and a degree-3 atom".
        let fused = !rings.is_empty() && (0..t.len()).any(|i| t.deg(i) >= 3);
        let mut fusions = Vec::new();
        if fused {
            for &n in &rings {
                if let Some(k) = ring_key(n) {
                    fusions.push(k);
                }
            }
            if rings.contains(&6) {
                fusions.push("a");
            }
        }

        let mut seeds = vec!["c"];
        for e in ["O", "N", "F", "Cl", "Br", "I"] {
            if has(e) && let Some(k) = element_key(e) {
                seeds.push(k);
            }
        }

        Plan {
            growth,
            orders,
            fusions,
            seeds,
            components: t.components(),
            charged: t.charge.iter().any(|&c| c != 0),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Move generation
// ═══════════════════════════════════════════════════════════════════════════════

const DIRS: [(&str, f32, f32); 4] = [
    ("ArrowRight", 1.0, 0.0),
    ("ArrowLeft", -1.0, 0.0),
    ("ArrowUp", 0.0, -1.0),
    ("ArrowDown", 0.0, 1.0),
];

/// Every atom the hotspot can walk to with plain arrows, with the shortest way there.
///
/// Arrow navigation is direction-based over the 2D layout, so it is neither symmetric nor
/// guaranteed to reach every atom — which atoms are reachable is a property of where cleanup
/// and fragment placement happened to put things. Enumerating it beats reasoning about it.
fn nav_paths(mol: &Molecule, start: u32) -> Vec<(u32, String)> {
    let mut out = vec![(start, String::new())];
    let mut seen: HashSet<u32> = HashSet::from([start]);
    let mut queue = VecDeque::from([(start, String::new())]);
    while let Some((at, path)) = queue.pop_front() {
        for (name, dx, dy) in DIRS {
            let Some(next) = navigate_direction(mol, at, dx, dy) else { continue };
            if !seen.insert(next) {
                continue;
            }
            let np = if path.is_empty() { name.to_string() } else { format!("{path} {name}") };
            out.push((next, np.clone()));
            queue.push_back((next, np));
        }
    }
    out
}

/// A bond action: aim at the bond with `Shift+<arrow>`, act, then repeat the arrow **unshifted**.
///
/// That trailing arrow is not cosmetic. A bond target survives its own action, and while one is
/// set every atom key is routed to the bond instead of the atom — so without clearing it the
/// next move would be silently misinterpreted. Space would clear it too, but Space also drops
/// the atom hotspot, which nothing can re-acquire from the keyboard. The unshifted arrow is
/// always available here: `Shift+<arrow>` only found a bond because that direction has a
/// neighbour, and fusing onto a bond never moves the atoms already present.
fn bond_move(nav: &str, dir: &str, key: &str) -> String {
    let head = if nav.is_empty() { String::new() } else { format!("{nav} ") };
    format!("{head}Shift+{dir} {key} {dir}")
}

fn atom_move(nav: &str, key: &str) -> String {
    if nav.is_empty() { key.to_string() } else { format!("{nav} {key}") }
}

/// Candidate moves from `state`, restricted to what the target still asks for.
fn moves(state: &State, built: &Graph, target: &Graph, f: &Fit, plan: &Plan) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    // A component still to start: on an empty canvas directly, otherwise by dropping the
    // hotspot and walking the insertion cursor clear of what is already drawn.
    if built.components() < plan.components || built.len() == 0 {
        let prefix = if state.hotspot_atom.is_none() {
            if built.len() == 0 { "" } else { "ArrowRight ArrowRight ArrowRight " }
        } else {
            "Space ArrowRight ArrowRight ArrowRight "
        };
        for seed in &plan.seeds {
            out.push(format!("{prefix}{seed}"));
        }
    }

    let Some(hotspot) = state.hotspot_atom else {
        return out;
    };
    let paths = nav_paths(&state.molecule, hotspot);
    let index: HashMap<u32, usize> =
        state.molecule.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();

    for (id, nav) in &paths {
        let Some(&i) = index.get(id) else { continue };
        let j = f.map[i];

        // Wrong element: ask for exactly the one the target wants there, by key where one
        // exists and through the label editor where none does.
        if built.elem[i] != target.elem[j] {
            match element_key(&target.elem[j]) {
                Some(k) => out.push(atom_move(nav, k)),
                None => out.push(atom_move(nav, &label_move(&target.elem[j]))),
            }
        }
        // Wrong charge: `+` and `-` each step by one.
        if plan.charged && built.charge[i] != target.charge[j] {
            let k = if target.charge[j] > built.charge[i] { "+" } else { "-" };
            out.push(atom_move(nav, k));
        }
        // Room for another neighbour: any growth key may supply it.
        if built.deg(i) < target.deg(j) {
            for k in &plan.growth {
                out.push(atom_move(nav, k));
            }
        }

        // Bond actions, aimed by Shift+arrow from this atom.
        for (dir, dx, dy) in DIRS {
            let Some(other) = navigate_direction(&state.molecule, *id, dx, dy) else { continue };
            let Some(&n) = index.get(&other) else { continue };
            let Some(have) = built.order_between(i, n) else { continue };
            let want = target.order_between(f.map[i], f.map[n]);

            if want.is_some_and(|w| w != have) {
                for k in &plan.orders {
                    out.push(bond_move(nav, dir, k));
                }
            }
            // Both ends short of neighbours: a ring fused onto this bond can supply them.
            if built.deg(i) < target.deg(j) && built.deg(n) < target.deg(f.map[n]) {
                for k in &plan.fusions {
                    out.push(bond_move(nav, dir, k));
                }
            }
        }
    }

    out.sort_unstable();
    out.dedup();
    out
}

// ═══════════════════════════════════════════════════════════════════════════════
// Beam search
// ═══════════════════════════════════════════════════════════════════════════════

pub(super) struct Opts {
    pub beam: usize,
    pub max_depth: usize,
}

impl Default for Opts {
    fn default() -> Self {
        Opts { beam: 60, max_depth: 0 }
    }
}

struct Node {
    state: State,
    seq: String,
    fit: Fit,
    graph: Graph,
    score: usize,
}

/// Search for a sequence that builds `target`, or `None` if the beam runs out.
pub(super) fn solve(target: &Molecule, opts: &Opts) -> Option<String> {
    let t = Graph::of(target);
    if t.len() == 0 {
        return Some(String::new());
    }
    let plan = Plan::for_target(&t);
    let max_depth = if opts.max_depth > 0 {
        opts.max_depth
    } else {
        // Every move fixes at most one thing `remaining` counts, plus slack for the moves that
        // set up rather than fix (seeding, walking to a second component).
        t.len() * 2 + t.bonds + 8
    };

    let mut sim = Sim::new();
    let mut seen: HashSet<String> = HashSet::new();

    let empty = Sim::new().state();
    let root_graph = Graph::of(&empty.molecule);
    let root_fit = Fit { map: Vec::new(), elem_ok: 0, order_ok: 0 };
    let root_score = remaining(&root_graph, &t, &root_fit);
    let mut beam = vec![Node {
        state: empty,
        seq: String::new(),
        fit: root_fit,
        graph: root_graph,
        score: root_score,
    }];

    for _ in 0..max_depth {
        let mut next: Vec<Node> = Vec::new();

        for node in &beam {
            for mv in moves(&node.state, &node.graph, &t, &node.fit, &plan) {
                sim.restore(&node.state);
                sim.run(&mv);

                let seq = if node.seq.is_empty() {
                    mv.clone()
                } else {
                    format!("{} {}", node.seq, mv)
                };
                if is_isomorphic(&sim.editor.molecule, target) {
                    return Some(seq);
                }

                // A bond action that failed to clear its target would make every following
                // move mean something else; drop the branch rather than search a lie.
                if sim.editor.hotspot_bond.is_some() {
                    continue;
                }

                let g = Graph::of(&sim.editor.molecule);
                let Some(f) = fit(&g, &t) else { continue };

                let key = format!(
                    "{}|{}",
                    canonical_key(&sim.editor.molecule),
                    sim.editor.hotspot_atom.is_some() as u8
                );
                if !seen.insert(key) {
                    continue;
                }

                let score = remaining(&g, &t, &f);
                next.push(Node { state: sim.state(), seq, fit: f, graph: g, score });
            }
        }

        if next.is_empty() {
            return None;
        }
        // Closest first, then shortest, then by text so ties never depend on push order.
        next.sort_by(|a, b| {
            (a.score, a.seq.split_whitespace().count(), &a.seq)
                .cmp(&(b.score, b.seq.split_whitespace().count(), &b.seq))
        });
        next.truncate(opts.beam);
        beam = next;
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{load_fixture, script_fixtures};

    /// The solver must keep working, so a couple of tiny targets are solved on every run.
    /// Both are also in the golden table, so a regression shows up as "the search broke"
    /// rather than "the goldens are stale".
    #[test]
    fn solves_small_targets_quickly() {
        for name in ["benzene", "ethanol"] {
            let target = load_fixture(name);
            let seq = solve(&target, &Opts { beam: 16, max_depth: 8 })
                .unwrap_or_else(|| panic!("solver found no sequence for {name}"));
            assert!(
                is_isomorphic(&build(&seq), &target),
                "{name}: solver returned {seq:?}, which does not build it",
            );
        }
    }

    /// The label route is the fallback, not the default. If a bare-atom key ever appears for P
    /// or Si this test says so, and the alphabet should switch to it; if one is ever *lost* for
    /// an element that has one today, the solver would quietly get slower rather than fail, so
    /// both directions are pinned.
    #[test]
    fn only_elements_without_a_key_go_through_the_label_editor() {
        for elem in ["C", "O", "N", "F", "Cl", "Br", "I", "S"] {
            assert!(element_key(elem).is_some(), "{elem} has a key and must not need the label editor");
        }
        for elem in ["P", "Si"] {
            assert!(element_key(elem).is_none(), "{elem} unexpectedly has a bare-atom key now");
        }
    }

    /// The label move must genuinely set the element and leave the build able to continue —
    /// a hotspot lost here would strand every P- and Si-bearing target.
    #[test]
    fn a_label_move_sets_the_element_and_keeps_building() {
        let mut sim = Sim::new();
        sim.run("c 1");
        sim.run(&label_move("P"));
        assert!(
            sim.editor.molecule.atoms.iter().any(|a| a.element == "P"),
            "the label move should have written P",
        );
        sim.run("1");
        assert_eq!(sim.editor.molecule.atoms.len(), 3, "building must continue after a label move");
    }

    /// Arrow navigation is enumerated from `navigate_direction` rather than replayed, so this
    /// pins the two to each other: if the widget's rule changes, the enumeration must too.
    #[test]
    fn nav_enumeration_matches_the_editor() {
        let mut sim = Sim::new();
        sim.run("c a Shift+ArrowRight a"); // naphthalene: plenty of directions to disagree on
        let start = sim.editor.hotspot_atom.expect("a build leaves a hotspot");
        let state = sim.state();

        for (id, path) in nav_paths(&state.molecule, start) {
            sim.restore(&state);
            sim.run(&path);
            assert_eq!(
                sim.editor.hotspot_atom,
                Some(id),
                "path {path:?} should land the hotspot on atom {id}",
            );
        }
    }

    /// A bond move must leave no bond target behind, or the next move's atom key would be
    /// routed to the bond instead. This is the invariant `bond_move`'s trailing arrow exists
    /// to maintain.
    #[test]
    fn a_bond_move_clears_its_own_target() {
        let mut sim = Sim::new();
        sim.run("c 1");
        let state = sim.state();
        sim.restore(&state);
        sim.run(&bond_move("", "ArrowLeft", "2"));
        assert_eq!(
            sim.editor.molecule.bonds.iter().filter(|b| b.order == BondOrder::Double).count(),
            1,
            "the bond action itself must still land",
        );
        assert!(sim.editor.hotspot_bond.is_none(), "the bond target must be cleared");
        assert!(sim.editor.hotspot_atom.is_some(), "the atom hotspot must survive");
    }

    /// Search the whole goal set and write the results out. Ignored by default: it is a
    /// generator, not an assertion, and it takes minutes.
    ///
    /// `cargo test --lib solve_all_fixtures -- --ignored --nocapture`
    #[test]
    #[ignore = "search tool: run explicitly to regenerate target/keyboard_goldens.txt"]
    fn solve_all_fixtures() {
        let started = std::time::Instant::now();
        let mut lines = Vec::new();
        let mut solved = 0;

        for (name, target) in script_fixtures() {
            let t0 = std::time::Instant::now();
            let found = solve(&target, &Opts::default());
            match found {
                // Re-verify from scratch: a sequence is only worth reporting if it builds the
                // structure with no help from the search's own cached state.
                Some(seq) if is_isomorphic(&build(&seq), &target) => {
                    solved += 1;
                    println!("{:>7.2}s  {name:<24} {seq}", t0.elapsed().as_secs_f32());
                    lines.push(format!("{name}\t{seq}"));
                }
                Some(seq) => {
                    println!("{:>7.2}s  {name:<24} REPLAY MISMATCH: {seq}", t0.elapsed().as_secs_f32());
                    lines.push(format!("{name}\tUNSOLVED"));
                }
                None => {
                    println!("{:>7.2}s  {name:<24} UNSOLVED", t0.elapsed().as_secs_f32());
                    lines.push(format!("{name}\tUNSOLVED"));
                }
            }
        }

        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/keyboard_goldens.txt");
        std::fs::create_dir_all(out.parent().expect("target dir has a parent")).ok();
        std::fs::write(&out, lines.join("\n") + "\n").expect("write goldens");

        println!(
            "\nsolved {solved} / {} in {:.1}s -> {}",
            lines.len(),
            started.elapsed().as_secs_f32(),
            out.display(),
        );
    }
}
