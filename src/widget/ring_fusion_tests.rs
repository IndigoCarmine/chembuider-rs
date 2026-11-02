//! Ring fusion on polycyclic systems, driven through the real key path (OPE-32).
//!
//! `a` on a bond has to add a ring wherever a regular hexagon fits. On a polycyclic aromatic
//! that hexagon often shares more than the bond it was asked for: it can coincide with the next
//! bond round the molecule, and its far vertices can land on atoms already there (peri fusion,
//! the way phenalene, pyrene, coronene and graphene grow).
//!
//! The geometry here is an exact hexagonal lattice, so what a key press must produce can be
//! worked out independently of the code under test: [`Lattice::census`] says how many atoms and
//! bonds a set of rings has, and [`max_double_bonds`] how many double bonds they can carry.

use crate::molecule::{BondOrder, BondStereo, Molecule};
use crate::testkit::{CANVAS, chord, kbd_frame, run_frame, run_frame_with_modifiers, type_keys};
use crate::widget::{ChemStructEditor, DEFAULT_BOND_LENGTH};
use std::collections::{BTreeSet, HashMap};

const L: f32 = DEFAULT_BOND_LENGTH;

// ═══════════════════════════════════════════════════════════════════════════════
// Driving the editor
// ═══════════════════════════════════════════════════════════════════════════════

/// Press `key` with `bond_id` targeted from the keyboard (Shift+arrow's `hotspot_bond`).
fn press_on_bond(e: &mut ChemStructEditor, bond_id: u32, key: &str) {
    e.hotspot_atom = None;
    e.hotspot_bond = Some(bond_id);
    kbd_frame(&egui::Context::default(), e, key);
}

/// Press `key` with the pointer resting on `bond_id`: the bond the user sees in green.
fn hover_and_press(e: &mut ChemStructEditor, bond_id: u32, key: &str) {
    let bond = e.molecule.bond_by_id(bond_id).expect("bond exists").clone();
    let p = e.molecule.atom_by_id(bond.begin).unwrap().pos;
    let q = e.molecule.atom_by_id(bond.end).unwrap().pos;
    let at = e.mol_to_screen([(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0], CANVAS.center());

    let ctx = egui::Context::default();
    run_frame(&ctx, CANVAS, e, vec![egui::Event::PointerMoved(at)]);
    // egui reports a hover against the rects of the frame before.
    run_frame(&ctx, CANVAS, e, vec![egui::Event::PointerMoved(at)]);
    assert_eq!(
        e.hovered_bond,
        Some(bond_id),
        "the pointer should be resting on the bond"
    );
    let (mut events, modifiers) = chord(key);
    events.insert(0, egui::Event::PointerMoved(at));
    run_frame_with_modifiers(&ctx, CANVAS, e, events, modifiers);
}

/// A new editor holding a copy of `mol`.
fn fresh(mol: &Molecule) -> ChemStructEditor {
    ChemStructEditor {
        molecule: mol.clone(),
        ..Default::default()
    }
}

/// Turn the whole drawing about the origin.
fn rotate(mol: &mut Molecule, degrees: f32) {
    let (s, c) = degrees.to_radians().sin_cos();
    for a in &mut mol.atoms {
        let [x, y] = a.pos;
        a.pos = [x * c - y * s, x * s + y * c];
    }
}

/// Move the drawing's centre to the origin, the middle of the canvas.
fn centre(mol: &mut Molecule) {
    let n = mol.atoms.len() as f32;
    let cx = mol.atoms.iter().map(|a| a.pos[0]).sum::<f32>() / n;
    let cy = mol.atoms.iter().map(|a| a.pos[1]).sum::<f32>() / n;
    for a in &mut mol.atoms {
        a.pos[0] -= cx;
        a.pos[1] -= cy;
    }
}

fn degree(mol: &Molecule, id: u32) -> usize {
    mol.bonds_for_atom(id).len()
}

/// The bond both of whose atoms are ring-fusion atoms.
fn fusion_bond(mol: &Molecule) -> u32 {
    mol.bonds
        .iter()
        .find(|b| degree(mol, b.begin) == 3 && degree(mol, b.end) == 3)
        .expect("a fused system has a fusion bond")
        .id
}

/// A bond of naphthalene from a fusion atom to its outer neighbour: where `a` is a peri fusion.
fn peri_bond(mol: &Molecule) -> u32 {
    let fusion = fusion_bond(mol);
    let atom = mol.bond_by_id(fusion).unwrap().begin;
    mol.bonds_for_atom(atom)
        .iter()
        .find(|b| b.id != fusion)
        .unwrap()
        .id
}

/// Naphthalene with its fusion bond lying `tilt` degrees off horizontal; at 0 the two rings
/// stand one above the other with flat sides, the drawing the report is about.
fn naphthalene(tilt: f32) -> ChemStructEditor {
    let mut e = type_keys("c a Shift+ArrowRight a");
    centre(&mut e.molecule);
    let b = e.molecule.bond_by_id(fusion_bond(&e.molecule)).unwrap();
    let p = e.molecule.atom_by_id(b.begin).unwrap().pos;
    let q = e.molecule.atom_by_id(b.end).unwrap().pos;
    let now = (q[1] - p[1]).atan2(q[0] - p[0]).to_degrees();
    rotate(&mut e.molecule, tilt - now);
    e.hotspot_atom = None;
    e.hotspot_bond = None;
    e.undo_stack.clear();
    e
}

// ═══════════════════════════════════════════════════════════════════════════════
// What a sound drawing looks like
// ═══════════════════════════════════════════════════════════════════════════════

fn count_doubles(mol: &Molecule) -> usize {
    mol.bonds
        .iter()
        .filter(|b| b.order == BondOrder::Double)
        .count()
}

/// Independent rings of a connected drawing (the cycle rank).
fn ring_count(mol: &Molecule) -> usize {
    mol.bonds.len() + 1 - mol.atoms.len()
}

fn assert_valences(mol: &Molecule, what: &str) {
    for a in &mol.atoms {
        let sum = mol.bond_order_sum(a.id);
        assert!(sum <= 4, "{what}: atom {} has valence {sum}", a.id);
        let doubles = mol
            .bonds_for_atom(a.id)
            .iter()
            .filter(|b| b.order == BondOrder::Double)
            .count();
        assert!(
            doubles <= 1,
            "{what}: atom {} carries {doubles} double bonds",
            a.id
        );
    }
}

/// The chemistry and geometry every key press must leave behind on an all-carbon lattice.
fn assert_sound(mol: &Molecule, what: &str) {
    for (i, a) in mol.atoms.iter().enumerate() {
        for b in &mol.atoms[i + 1..] {
            let d = (a.pos[0] - b.pos[0]).hypot(a.pos[1] - b.pos[1]);
            assert!(
                d > 0.5,
                "{what}: atoms {} and {} lie on top of each other",
                a.id,
                b.id
            );
        }
    }
    let mut seen = BTreeSet::new();
    for b in &mol.bonds {
        let key = (b.begin.min(b.end), b.begin.max(b.end));
        assert!(seen.insert(key), "{what}: bond {key:?} is drawn twice");
        let (p, q) = (
            mol.atom_by_id(b.begin).unwrap().pos,
            mol.atom_by_id(b.end).unwrap().pos,
        );
        let len = (p[0] - q[0]).hypot(p[1] - q[1]);
        assert!(
            (len - L).abs() < 0.05,
            "{what}: bond {key:?} is {len:.3} long"
        );
    }
    assert_valences(mol, what);
}

/// The most double bonds the skeleton can carry: a maximum matching, found here by Kuhn's
/// algorithm over the two sublattices so that it shares nothing with the code under test.
fn max_double_bonds(mol: &Molecule) -> usize {
    fn augment(
        mol: &Molecule,
        u: u32,
        seen: &mut BTreeSet<u32>,
        mate: &mut HashMap<u32, u32>,
    ) -> bool {
        for v in mol.neighbor_atom_ids(u) {
            if seen.insert(v) {
                let free = match mate.get(&v).copied() {
                    None => true,
                    Some(w) => augment(mol, w, seen, mate),
                };
                if free {
                    mate.insert(v, u);
                    return true;
                }
            }
        }
        false
    }

    let mut colour: HashMap<u32, bool> = HashMap::new();
    for start in mol.atoms.iter().map(|a| a.id) {
        if colour.contains_key(&start) {
            continue;
        }
        colour.insert(start, false);
        let mut stack = vec![start];
        while let Some(u) = stack.pop() {
            for v in mol.neighbor_atom_ids(u) {
                match colour.get(&v) {
                    Some(&c) => assert_ne!(c, colour[&u], "the lattice is bipartite"),
                    None => {
                        colour.insert(v, !colour[&u]);
                        stack.push(v);
                    }
                }
            }
        }
    }
    let mut mate = HashMap::new();
    mol.atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| !colour[id])
        .filter(|&u| augment(mol, u, &mut BTreeSet::new(), &mut mate))
        .count()
}

// ═══════════════════════════════════════════════════════════════════════════════
// A hexagonal lattice to aim at
// ═══════════════════════════════════════════════════════════════════════════════

/// Axial coordinates of a hexagon on the lattice.
type Hex = (i32, i32);

/// The lattice a drawn hexagon lies on.
#[derive(Clone, Copy)]
struct Lattice {
    origin: [f32; 2],
    /// From a hexagon's centre to the middle of one of its edges.
    apothem: [f32; 2],
}

fn turned(v: [f32; 2], degrees: f32) -> [f32; 2] {
    let (s, c) = degrees.to_radians().sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c]
}

impl Lattice {
    /// The lattice of a lone benzene, which is its hexagon (0, 0).
    fn of_benzene(mol: &Molecule) -> Self {
        let n = mol.atoms.len() as f32;
        let origin = [
            mol.atoms.iter().map(|a| a.pos[0]).sum::<f32>() / n,
            mol.atoms.iter().map(|a| a.pos[1]).sum::<f32>() / n,
        ];
        let b = &mol.bonds[0];
        let (p, q) = (
            mol.atom_by_id(b.begin).unwrap().pos,
            mol.atom_by_id(b.end).unwrap().pos,
        );
        Self {
            origin,
            apothem: [
                (p[0] + q[0]) / 2.0 - origin[0],
                (p[1] + q[1]) / 2.0 - origin[1],
            ],
        }
    }

    fn centre(&self, (q, r): Hex) -> [f32; 2] {
        let u = self.apothem;
        let v = turned(u, 60.0);
        [
            self.origin[0] + 2.0 * (q as f32 * u[0] + r as f32 * v[0]),
            self.origin[1] + 2.0 * (q as f32 * u[1] + r as f32 * v[1]),
        ]
    }

    /// The middle of the edge a hexagon shares with its `k`th neighbour.
    fn edge_mid(&self, hex: Hex, k: usize) -> [f32; 2] {
        let c = self.centre(hex);
        let a = turned(self.apothem, 60.0 * k as f32);
        [c[0] + a[0], c[1] + a[1]]
    }

    /// The six neighbours of a hexagon, in the order of [`edge_mid`](Self::edge_mid).
    fn neighbour((q, r): Hex, k: usize) -> Hex {
        let [dq, dr] = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]][k];
        (q + dq, r + dr)
    }

    fn vertices(&self, hex: Hex) -> Vec<[f32; 2]> {
        let c = self.centre(hex);
        let circum = turned(self.apothem, 30.0);
        let scale = 2.0 / 3.0_f32.sqrt();
        (0..6)
            .map(|k| {
                let v = turned(circum, 60.0 * k as f32);
                [c[0] + scale * v[0], c[1] + scale * v[1]]
            })
            .collect()
    }

    /// The bond whose midpoint is `mid`.
    fn bond_at(&self, mol: &Molecule, mid: [f32; 2]) -> Option<u32> {
        mol.bonds
            .iter()
            .find(|b| {
                let (p, q) = (
                    mol.atom_by_id(b.begin).unwrap().pos,
                    mol.atom_by_id(b.end).unwrap().pos,
                );
                let m = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
                (m[0] - mid[0]).hypot(m[1] - mid[1]) < 0.1
            })
            .map(|b| b.id)
    }

    /// Whether a hexagon is drawn: all six of its bonds are there.
    fn is_drawn(&self, mol: &Molecule, hex: Hex) -> bool {
        (0..6).all(|k| self.bond_at(mol, self.edge_mid(hex, k)).is_some())
    }

    /// How many atoms and bonds these rings have between them.
    fn census(&self, hexes: &[Hex]) -> (usize, usize) {
        fn distinct(points: impl Iterator<Item = [f32; 2]>) -> usize {
            let mut out: Vec<[f32; 2]> = Vec::new();
            for p in points {
                if !out.iter().any(|q| (p[0] - q[0]).hypot(p[1] - q[1]) < 0.1) {
                    out.push(p);
                }
            }
            out.len()
        }
        (
            distinct(hexes.iter().flat_map(|&h| self.vertices(h))),
            distinct(
                hexes
                    .iter()
                    .flat_map(|&h| (0..6).map(move |k| (h, k)))
                    .map(|(h, k)| self.edge_mid(h, k)),
            ),
        )
    }
}

/// Press `a` on a bond of `hex` that is already drawn, and check that the ring appeared and
/// everything else about the drawing is what the rings drawn so far call for.
fn grow(e: &mut ChemStructEditor, lattice: Lattice, drawn: &mut Vec<Hex>, hex: Hex) {
    let bond = (0..6)
        .find_map(|k| lattice.bond_at(&e.molecule, lattice.edge_mid(hex, k)))
        .expect("the ring must touch what is drawn");
    press_on_bond(e, bond, "a");
    drawn.push(hex);
    let (atoms, bonds) = lattice.census(drawn);
    let what = format!("{} rings, last {hex:?}", drawn.len());
    assert!(
        lattice.is_drawn(&e.molecule, hex),
        "{what}: the ring must be drawn"
    );
    assert_eq!(e.molecule.atoms.len(), atoms, "{what}: atoms");
    assert_eq!(e.molecule.bonds.len(), bonds, "{what}: bonds");
    assert_sound(&e.molecule, &what);
    assert_eq!(
        count_doubles(&e.molecule),
        max_double_bonds(&e.molecule),
        "{what}: double bonds"
    );
}

fn benzene() -> (ChemStructEditor, Lattice) {
    let e = type_keys("c a");
    let lattice = Lattice::of_benzene(&e.molecule);
    (e, lattice)
}

// ═══════════════════════════════════════════════════════════════════════════════
// The report
// ═══════════════════════════════════════════════════════════════════════════════

/// The reported press: flat-sided naphthalene, pointer on the bond from a fusion atom to its
/// outer neighbour. The new ring shares two bonds and gives phenalene.
#[test]
fn a_on_the_bond_beside_a_fusion_atom_adds_a_peri_ring() {
    for tilt in [0.0, 90.0] {
        let mut e = naphthalene(tilt);
        let bond = peri_bond(&e.molecule);

        hover_and_press(&mut e, bond, "a");

        assert_eq!(
            e.molecule.atoms.len(),
            13,
            "tilt {tilt}: phenalene has 13 carbons"
        );
        assert_eq!(
            e.molecule.bonds.len(),
            15,
            "tilt {tilt}: phenalene has 15 bonds"
        );
        assert_eq!(
            ring_count(&e.molecule),
            3,
            "tilt {tilt}: phenalene has 3 rings"
        );
        assert_eq!(
            count_doubles(&e.molecule),
            6,
            "tilt {tilt}: 13 carbons pair off into 6"
        );
        assert_sound(&e.molecule, "phenalene");
    }
}

/// Every bond of naphthalene, however it is drawn and whichever way the bond happens to run.
///
/// | bonds | what `a` gives                    | atoms | bonds |
/// |-------|-----------------------------------|-------|-------|
/// | 1     | nothing: rings on both sides      | 10    | 11    |
/// | 2     | anthracene (linear fusion)        | 14    | 16    |
/// | 4     | phenanthrene (angular fusion)     | 14    | 16    |
/// | 4     | phenalene (peri fusion)           | 13    | 15    |
#[test]
fn a_on_every_bond_of_naphthalene() {
    for tilt in [0.0, 30.0, 90.0, 137.0] {
        for reversed in [false, true] {
            for hover in [false, true] {
                let mut base = naphthalene(tilt);
                if reversed {
                    for b in &mut base.molecule.bonds {
                        std::mem::swap(&mut b.begin, &mut b.end);
                    }
                }
                let ids: Vec<u32> = base.molecule.bonds.iter().map(|b| b.id).collect();
                assert_eq!(ids.len(), 11);
                let mut outcomes = HashMap::<(usize, usize), usize>::new();
                for id in ids {
                    let mut e = fresh(&base.molecule);
                    if hover {
                        hover_and_press(&mut e, id, "a");
                    } else {
                        press_on_bond(&mut e, id, "a");
                    }
                    let what = format!("tilt {tilt} reversed {reversed} hover {hover} bond {id}");
                    assert_sound(&e.molecule, &what);
                    assert_eq!(
                        count_doubles(&e.molecule),
                        e.molecule.atoms.len() / 2,
                        "{what}"
                    );
                    let size = (e.molecule.atoms.len(), e.molecule.bonds.len());
                    *outcomes.entry(size).or_default() += 1;
                }
                let want = HashMap::from([((10, 11), 1), ((14, 16), 6), ((13, 15), 4)]);
                assert_eq!(
                    outcomes, want,
                    "tilt {tilt} reversed {reversed} hover {hover}"
                );
            }
        }
    }
}

/// The two kinds of 14-carbon outcome are the right systems, not just the right size. They
/// differ in how many bonds join two ring-fusion atoms: anthracene's two fusion bonds, and in
/// phenanthrene those plus the bond across its bay.
#[test]
fn linear_and_angular_fusion_give_anthracene_and_phenanthrene() {
    let base = naphthalene(0.0);
    let (mut linear, mut angular) = (0, 0);
    for b in &base.molecule.bonds {
        let mut e = fresh(&base.molecule);
        press_on_bond(&mut e, b.id, "a");
        let m = &e.molecule;
        if m.atoms.len() == 14 {
            match m
                .bonds
                .iter()
                .filter(|b| degree(m, b.begin) == 3 && degree(m, b.end) == 3)
                .count()
            {
                2 => linear += 1,
                3 => angular += 1,
                n => panic!("{n} bonds between fusion atoms"),
            }
        }
    }
    assert_eq!((linear, angular), (2, 4));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Graphene
// ═══════════════════════════════════════════════════════════════════════════════

/// `a` on outer bonds walks through phenalene and pyrene to coronene.
#[test]
fn repeated_a_builds_pyrene_and_coronene() {
    let (mut e, lattice) = benzene();
    let mut drawn = vec![(0, 0)];
    for hex in [(1, 0), (0, 1)] {
        grow(&mut e, lattice, &mut drawn, hex);
    }
    assert_eq!(
        (e.molecule.atoms.len(), ring_count(&e.molecule)),
        (13, 3),
        "phenalene"
    );

    grow(&mut e, lattice, &mut drawn, (1, -1));
    assert_eq!(e.molecule.atoms.len(), 16, "pyrene has 16 carbons");
    assert_eq!(e.molecule.bonds.len(), 19, "pyrene has 19 bonds");
    assert_eq!(ring_count(&e.molecule), 4, "pyrene has 4 rings");
    assert_eq!(count_doubles(&e.molecule), 8, "pyrene is fully Kekulé");

    for hex in [(0, -1), (-1, 0), (-1, 1)] {
        grow(&mut e, lattice, &mut drawn, hex);
    }
    assert_eq!(e.molecule.atoms.len(), 24, "coronene has 24 carbons");
    assert_eq!(e.molecule.bonds.len(), 30, "coronene has 30 bonds");
    assert_eq!(ring_count(&e.molecule), 7, "coronene has 7 rings");
    assert_eq!(count_doubles(&e.molecule), 12, "coronene is fully Kekulé");
    assert_sound(&e.molecule, "coronene");
}

/// A ten-ring flake, grown outwards from coronene.
#[test]
fn repeated_a_builds_a_ten_ring_flake() {
    let (mut e, lattice) = benzene();
    let mut drawn = vec![(0, 0)];
    for k in 0..6 {
        grow(&mut e, lattice, &mut drawn, Lattice::neighbour((0, 0), k));
    }
    for hex in [(2, -1), (1, 1), (-1, 2)] {
        grow(&mut e, lattice, &mut drawn, hex);
    }
    assert_eq!(drawn.len(), 10);
    assert_eq!(ring_count(&e.molecule), 10);
    assert_eq!(
        e.molecule.atoms.len(),
        33,
        "coronene plus three bay rings of three carbons each"
    );
    assert_eq!(e.molecule.bonds.len(), 42);
    assert_eq!(
        count_doubles(&e.molecule),
        16,
        "33 carbons pair off into 16"
    );
}

/// Grow flakes by pressing `a` on arbitrary bonds, in arbitrary orders: bays, fjords and holes
/// included. Every press draws the ring on the far side of the bond (or finds it drawn), and the
/// double bonds are always as many as the skeleton can hold.
#[test]
fn any_order_of_outer_bonds_grows_a_sound_flake() {
    for seed in 1..=8u32 {
        let (mut e, lattice) = benzene();
        let mut drawn = BTreeSet::from([(0, 0)]);
        let mut state = seed.wrapping_mul(2_654_435_761);
        let mut next = |n: usize| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as usize % n
        };
        for press in 0..40 {
            // A bond on the rim of a random ring.
            let hexes: Vec<Hex> = drawn.iter().copied().collect();
            let hex = hexes[next(hexes.len())];
            let k = next(6);
            let far = Lattice::neighbour(hex, k);
            let bond = lattice
                .bond_at(&e.molecule, lattice.edge_mid(hex, k))
                .unwrap();
            press_on_bond(&mut e, bond, "a");

            let what = format!("seed {seed} press {press}, {hex:?} towards {far:?}");
            assert_sound(&e.molecule, &what);
            assert_eq!(
                count_doubles(&e.molecule),
                max_double_bonds(&e.molecule),
                "{what}"
            );
            assert!(
                lattice.is_drawn(&e.molecule, far),
                "{what}: the ring must be drawn"
            );
            drawn.insert(far);
            // A ring closed by its neighbours: the last of a hole.
            for h in drawn.clone() {
                for j in 0..6 {
                    let n = Lattice::neighbour(h, j);
                    if lattice.is_drawn(&e.molecule, n) {
                        drawn.insert(n);
                    }
                }
            }
            let hexes: Vec<Hex> = drawn.iter().copied().collect();
            let want = lattice.census(&hexes);
            assert_eq!(
                (e.molecule.atoms.len(), e.molecule.bonds.len()),
                want,
                "{what}"
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Undo
// ═══════════════════════════════════════════════════════════════════════════════

/// One press is one undo step, peri fusion included, and undo restores the bond orders the
/// re-derived pattern changed.
#[test]
fn one_press_is_one_undo_step() {
    let (mut e, lattice) = benzene();
    let mut drawn = vec![(0, 0)];
    let mut history = vec![e.molecule.clone()];
    e.undo_stack.clear();
    for hex in [(1, 0), (0, 1), (1, -1), (0, -1)] {
        grow(&mut e, lattice, &mut drawn, hex);
        history.push(e.molecule.clone());
    }
    assert_eq!(e.undo_stack.len(), 4, "four presses, four steps");
    for want in history.iter().rev().skip(1) {
        assert!(e.undo());
        assert_eq!(&e.molecule, want, "each undo goes back exactly one press");
    }
}

/// A press that changes nothing is no step at all: it must not make undo appear to do nothing,
/// and it must not cost the user their redo history.
#[test]
fn a_press_with_nowhere_to_go_is_no_undo_step() {
    let mut e = naphthalene(0.0);
    let fusion = fusion_bond(&e.molecule);
    let peri = peri_bond(&e.molecule);
    press_on_bond(&mut e, peri, "a");
    assert!(e.undo());
    assert_eq!(e.redo_stack.len(), 1);
    let (before, undo_len) = (e.molecule.clone(), e.undo_stack.len());

    press_on_bond(&mut e, fusion, "a");

    assert_eq!(e.molecule, before, "both sides are taken: nothing to draw");
    assert_eq!(
        e.undo_stack.len(),
        undo_len,
        "nothing happened, so nothing to undo"
    );
    assert_eq!(e.redo_stack.len(), 1, "and nothing was lost");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Sides
// ═══════════════════════════════════════════════════════════════════════════════

/// Toluene from `c 1 a`, with its methyl carbon and the ring carbon that carries it.
fn toluene() -> (ChemStructEditor, u32, u32) {
    let mut e = type_keys("c 1 a");
    assert_eq!(e.molecule.atoms.len(), 7);
    e.hotspot_atom = None;
    let methyl = e
        .molecule
        .atoms
        .iter()
        .find(|a| degree(&e.molecule, a.id) == 1)
        .unwrap()
        .id;
    let ipso = e.molecule.neighbor_atom_ids(methyl)[0];
    (e, methyl, ipso)
}

/// A ring bond with a substituent on its free side: the hexagon's vertex lands on that
/// substituent, which is no reason to draw the ring inside the existing one.
#[test]
fn a_substituent_on_the_free_side_does_not_send_the_ring_inwards() {
    let (base, methyl, ipso) = toluene();
    let ring_bonds: Vec<u32> = base
        .molecule
        .bonds_for_atom(ipso)
        .iter()
        .filter(|b| b.begin != methyl && b.end != methyl)
        .map(|b| b.id)
        .collect();
    assert_eq!(ring_bonds.len(), 2);
    for bond in ring_bonds {
        let mut e = fresh(&base.molecule);
        press_on_bond(&mut e, bond, "a");
        assert_eq!(
            e.molecule.atoms.len(),
            10,
            "the methyl carbon is a vertex of the new ring"
        );
        assert_eq!(e.molecule.bonds.len(), 11);
        assert_eq!(count_doubles(&e.molecule), 5, "that is naphthalene");
        assert_sound(&e.molecule, "benzene fused across a methyl");
    }
}

/// A group's hydrogen is part of a label: it neither blocks a ring nor becomes a vertex of one.
/// The NH₂ nitrogen on the free side does become a vertex, as any atom there would.
#[test]
fn a_group_hydrogen_does_not_stand_in_the_way() {
    let (mut base, carrier, ipso) = toluene();
    base.molecule.atom_by_id_mut(carrier).unwrap().element = "N".into();
    let nh2 = crate::Config::embedded()
        .resolve_fragment("NH2")
        .expect("a built-in fragment");
    base.molecule.insert_fragment(&nh2, carrier, 0.0, false);
    assert!(
        base.molecule.atoms.iter().any(|a| a.group_h),
        "the nitrogen wears a group hydrogen"
    );
    let bond = base
        .molecule
        .bonds_for_atom(ipso)
        .iter()
        .find(|b| b.begin != carrier && b.end != carrier)
        .unwrap()
        .id;
    let heavy = |m: &Molecule| m.atoms.iter().filter(|a| a.element != "H").count();

    let mut e = fresh(&base.molecule);
    press_on_bond(&mut e, bond, "a");

    assert_eq!(
        heavy(&e.molecule),
        heavy(&base.molecule) + 3,
        "three new carbons, and the N"
    );
    for a in &e.molecule.atoms {
        assert!(
            e.molecule.valence_excess(a.id).is_none_or(|x| x <= 0),
            "atom {} over-bonded",
            a.id
        );
        if a.element == "H" {
            assert_eq!(
                degree(&e.molecule, a.id),
                1,
                "a hydrogen is never a ring vertex"
            );
        }
    }
}

/// The ring keys share the placement: they land outside a ring and merge what they coincide
/// with, and being saturated they leave every double bond where it is.
#[test]
fn ring_keys_land_on_the_free_side_and_merge() {
    for (key, n) in [("5", 5), ("6", 6)] {
        let (base, lattice) = benzene();
        for b in &base.molecule.bonds {
            let mut e = fresh(&base.molecule);
            press_on_bond(&mut e, b.id, key);
            assert_eq!(e.molecule.atoms.len(), 6 + n - 2, "`{key}` on benzene");
            assert_eq!(
                count_doubles(&e.molecule),
                3,
                "`{key}` keeps benzene's double bonds"
            );
            let new = &e.molecule.atoms[6..];
            let cx = new.iter().map(|a| a.pos[0]).sum::<f32>() / new.len() as f32;
            let cy = new.iter().map(|a| a.pos[1]).sum::<f32>() / new.len() as f32;
            let [ox, oy] = lattice.origin;
            assert!(
                (cx - ox).hypot(cy - oy) > 1.5 * L,
                "`{key}` must land outside benzene"
            );
        }
    }

    let mut e = naphthalene(0.0);
    let peri = peri_bond(&e.molecule);
    press_on_bond(&mut e, peri, "6");
    assert_eq!(e.molecule.atoms.len(), 13, "`6` shares the peri atoms too");
    assert_eq!(e.molecule.bonds.len(), 15);
    assert_eq!(
        count_doubles(&e.molecule),
        5,
        "a saturated ring brings no double bonds"
    );
    assert_sound(&e.molecule, "`6` on a peri bond");
}

/// Add a regular `n`-gon, on `edge` when given (the ring goes on that edge's own left).
fn polygon(e: &mut ChemStructEditor, n: usize, edge: Option<(u32, u32)>) -> Vec<u32> {
    use std::f32::consts::{PI, TAU};
    let r = L / (2.0 * (PI / n as f32).sin());
    let apothem = r * (PI / n as f32).cos();
    let (p, q) = match edge {
        Some((a, b)) => (
            e.molecule.atom_by_id(a).unwrap().pos,
            e.molecule.atom_by_id(b).unwrap().pos,
        ),
        None => ([-L / 2.0, 0.0], [L / 2.0, 0.0]),
    };
    let (ux, uy) = ((q[0] - p[0]) / L, (q[1] - p[1]) / L);
    let (cx, cy) = (
        (p[0] + q[0]) / 2.0 - uy * apothem,
        (p[1] + q[1]) / 2.0 + ux * apothem,
    );
    let start = (p[1] - cy).atan2(p[0] - cx);
    let mut ids = Vec::new();
    for k in 0..n {
        ids.push(match (k, edge) {
            (0, Some((a, _))) => a,
            (1, Some((_, b))) => b,
            _ => {
                let t = start + k as f32 * TAU / n as f32;
                e.molecule
                    .add_atom("C".into(), [cx + r * t.cos(), cy + r * t.sin()], 0)
            }
        });
    }
    for k in 0..n {
        let (a, b) = (ids[k], ids[(k + 1) % n]);
        if e.molecule.bond_between(a, b).is_none() {
            e.molecule.add_bond(a, b, BondOrder::Single);
        }
    }
    ids
}

/// A bond with a ring on each side has no room for another, whatever size those rings are:
/// a hexagon over a five-ring or inside an eight-ring is not a structure.
#[test]
fn a_bond_between_two_rings_of_any_size_takes_no_ring() {
    for (first, second) in [(5, 5), (5, 6), (6, 6), (6, 7), (6, 8), (5, 8), (7, 8)] {
        let mut e = ChemStructEditor::default();
        let ring = polygon(&mut e, first, None);
        // The first ring runs counter-clockwise round its own left, so the second, on the
        // reversed edge, lies on the other side of that edge.
        let shared = (ring[1], ring[0]);
        polygon(&mut e, second, Some(shared));
        let bond = e.molecule.bond_between(shared.0, shared.1).unwrap().id;
        assert_eq!(e.molecule.atoms.len(), first + second - 2);
        let before = e.molecule.clone();
        for key in ["a", "5", "6"] {
            press_on_bond(&mut e, bond, key);
            assert_eq!(
                e.molecule, before,
                "`{key}` between a {first}- and a {second}-ring"
            );
        }
    }
}

/// A hole with room for a hexagon is a free side: the new ring closes across it, and the rest
/// of the hole with it. Eight rings round a hole two hexagons long.
#[test]
fn a_ring_closes_across_a_hole_that_fits_it() {
    let (mut e, lattice) = benzene();
    let mut drawn = vec![(0, 0)];
    for hex in [(0, 1), (1, 1), (2, 1), (3, 0), (3, -1), (2, -1), (1, -1)] {
        grow(&mut e, lattice, &mut drawn, hex);
    }
    assert!(!lattice.is_drawn(&e.molecule, (1, 0)) && !lattice.is_drawn(&e.molecule, (2, 0)));
    let atoms = e.molecule.atoms.len();
    let bond = lattice
        .bond_at(&e.molecule, lattice.edge_mid((0, 0), 0))
        .unwrap();

    press_on_bond(&mut e, bond, "a");

    assert!(
        lattice.is_drawn(&e.molecule, (1, 0)),
        "the ring must close across the hole"
    );
    assert!(
        lattice.is_drawn(&e.molecule, (2, 0)),
        "and what is left of the hole is a ring"
    );
    assert_eq!(
        e.molecule.atoms.len(),
        atoms,
        "every vertex of it is already drawn"
    );
    assert_sound(&e.molecule, "a ring across a hole");
    assert_eq!(count_doubles(&e.molecule), max_double_bonds(&e.molecule));
}

// ═══════════════════════════════════════════════════════════════════════════════
// What must not change
// ═══════════════════════════════════════════════════════════════════════════════

/// Elements, wedges and the double bonds of a heteroatom stay as they were when the pattern
/// around them is re-derived.
#[test]
fn peri_fusion_leaves_heteroatoms_and_wedges_alone() {
    let mut base = naphthalene(0.0);
    let peri = peri_bond(&base.molecule);
    let at = base
        .molecule
        .atom_by_id(base.molecule.bond_by_id(peri).unwrap().end)
        .unwrap()
        .pos;
    let distance = |p: [f32; 2]| (p[0] - at[0]).hypot(p[1] - at[1]);

    // The far end of the far ring: a nitrogen, and beside it a wedge to a substituent.
    let far = base
        .molecule
        .atoms
        .iter()
        .filter(|a| degree(&base.molecule, a.id) == 2)
        .max_by(|x, y| distance(x.pos).partial_cmp(&distance(y.pos)).unwrap())
        .unwrap()
        .id;
    base.molecule.atom_by_id_mut(far).unwrap().element = "N".into();
    let beside = base
        .molecule
        .neighbor_atom_ids(far)
        .into_iter()
        .find(|&id| degree(&base.molecule, id) == 2)
        .unwrap();
    let [x, y] = base.molecule.atom_by_id(beside).unwrap().pos;
    let tip = base.molecule.add_atom("C".into(), [x * 1.6, y * 1.6], 0);
    let wedge = base.molecule.add_bond(beside, tip, BondOrder::Single);
    base.molecule.bond_by_id_mut(wedge).unwrap().stereo = BondStereo::WedgeUp;

    let mut e = fresh(&base.molecule);
    press_on_bond(&mut e, peri, "a");

    assert_eq!(
        e.molecule.atoms.len(),
        base.molecule.atoms.len() + 3,
        "the ring is drawn"
    );
    assert_valences(&e.molecule, "peri fusion beside a nitrogen");
    assert_eq!(e.molecule.atom_by_id(far).unwrap().element, "N");
    let w = e.molecule.bond_by_id(wedge).unwrap();
    assert_eq!(
        (&w.order, &w.stereo),
        (&BondOrder::Single, &BondStereo::WedgeUp)
    );
    let around_n = |m: &Molecule| {
        let mut orders: Vec<(u32, i16)> = m
            .bonds_for_atom(far)
            .iter()
            .map(|b| (b.id, b.order.valence()))
            .collect();
        orders.sort_unstable();
        orders
    };
    assert_eq!(
        around_n(&e.molecule),
        around_n(&base.molecule),
        "the C=N stays where it was"
    );
}

/// A double bond on a chain off the ring is not part of the aromatic system.
#[test]
fn peri_fusion_leaves_a_vinyl_group_alone() {
    let mut base = naphthalene(0.0);
    let peri = peri_bond(&base.molecule);
    let carrier = base
        .molecule
        .atoms
        .iter()
        .find(|a| degree(&base.molecule, a.id) == 2)
        .unwrap();
    let (carrier, [x, y]) = (carrier.id, carrier.pos);
    let c1 = base.molecule.add_atom("C".into(), [x * 8.0, y * 8.0], 0);
    let c2 = base
        .molecule
        .add_atom("C".into(), [x * 8.0 + L, y * 8.0], 0);
    base.molecule.add_bond(carrier, c1, BondOrder::Single);
    let vinyl = base.molecule.add_bond(c1, c2, BondOrder::Double);

    let mut e = fresh(&base.molecule);
    press_on_bond(&mut e, peri, "a");

    assert_eq!(
        e.molecule.bonds.len(),
        base.molecule.bonds.len() + 4,
        "the ring is drawn"
    );
    assert_eq!(
        e.molecule.bond_by_id(vinyl).unwrap().order,
        BondOrder::Double
    );
    assert_valences(&e.molecule, "peri fusion beside a vinyl group");
}

/// A hexagon whose vertex lands on a full carbon cannot be drawn: that vertex would need a
/// fifth bond. The press does nothing rather than draw one.
#[test]
fn a_vertex_on_a_full_carbon_is_no_fit() {
    let (mut base, methyl, ipso) = toluene();
    let [mx, my] = base.molecule.atom_by_id(methyl).unwrap().pos;
    let [ix, iy] = base.molecule.atom_by_id(ipso).unwrap().pos;
    // Make the methyl a tert-butyl: its three other bonds fan out away from the ring.
    for t in [-90.0, 0.0, 90.0] {
        let [dx, dy] = turned([mx - ix, my - iy], t);
        let id = base.molecule.add_atom("C".into(), [mx + dx, my + dy], 0);
        base.molecule.add_bond(methyl, id, BondOrder::Single);
    }
    let bond = base
        .molecule
        .bonds_for_atom(ipso)
        .iter()
        .find(|b| b.begin != methyl && b.end != methyl)
        .unwrap()
        .id;

    let mut e = fresh(&base.molecule);
    press_on_bond(&mut e, bond, "a");

    assert_eq!(
        e.molecule, base.molecule,
        "no room for the ring, so no ring"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// On an atom
// ═══════════════════════════════════════════════════════════════════════════════

/// Press `key` with `atom_id` targeted from the keyboard.
fn press_on_atom(e: &mut ChemStructEditor, atom_id: u32, key: &str) {
    e.hotspot_bond = None;
    e.hotspot_atom = Some(atom_id);
    kbd_frame(&egui::Context::default(), e, key);
}

/// Press `key` with the pointer resting on `atom_id`.
fn hover_atom_and_press(e: &mut ChemStructEditor, atom_id: u32, key: &str) {
    let at = e.mol_to_screen(
        e.molecule.atom_by_id(atom_id).expect("atom exists").pos,
        CANVAS.center(),
    );
    let ctx = egui::Context::default();
    run_frame(&ctx, CANVAS, e, vec![egui::Event::PointerMoved(at)]);
    run_frame(&ctx, CANVAS, e, vec![egui::Event::PointerMoved(at)]);
    assert_eq!(
        e.hovered_atom,
        Some(atom_id),
        "the pointer should be resting on the atom"
    );
    let (mut events, modifiers) = chord(key);
    events.insert(0, egui::Event::PointerMoved(at));
    run_frame_with_modifiers(&ctx, CANVAS, e, events, modifiers);
}

/// A flake of the given rings, in the middle of the canvas, and the atoms on a bond that has a
/// free side: those are the atoms a ring can still be drawn at.
fn flake(hexes: &[Hex]) -> (Molecule, BTreeSet<u32>) {
    let (mut e, lattice) = benzene();
    let mut drawn = vec![hexes[0]];
    for &hex in &hexes[1..] {
        grow(&mut e, lattice, &mut drawn, hex);
    }
    let mut rim = BTreeSet::new();
    for &hex in hexes {
        for k in 0..6 {
            if !hexes.contains(&Lattice::neighbour(hex, k)) {
                let id = lattice
                    .bond_at(&e.molecule, lattice.edge_mid(hex, k))
                    .unwrap();
                let bond = e.molecule.bond_by_id(id).unwrap();
                rim.extend([bond.begin, bond.end]);
            }
        }
    }
    centre(&mut e.molecule);
    (e.molecule.clone(), rim)
}

const NAPHTHALENE: &[Hex] = &[(0, 0), (1, 0)];
const PYRENE: &[Hex] = &[(0, 0), (1, 0), (0, 1), (1, -1)];
const CORONENE: &[Hex] = &[(0, 0), (1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];

/// The reported press: `a` on any atom of a polycyclic. Where a ring can go it goes, as soundly
/// as on a bond; where it cannot (an atom with rings all round it) nothing happens and undo has
/// nothing to spend a press on.
#[test]
fn a_on_every_atom_of_a_polycyclic() {
    for (name, hexes) in [
        ("naphthalene", NAPHTHALENE),
        ("pyrene", PYRENE),
        ("coronene", CORONENE),
    ] {
        let (base, rim) = flake(hexes);
        let rings = ring_count(&base);
        for hover in [false, true] {
            for atom in base.atoms.iter().map(|a| a.id) {
                let mut e = fresh(&base);
                if hover {
                    hover_atom_and_press(&mut e, atom, "a");
                } else {
                    press_on_atom(&mut e, atom, "a");
                }
                let what = format!("{name} atom {atom} hover {hover}");
                if rim.contains(&atom) {
                    assert!(
                        ring_count(&e.molecule) > rings,
                        "{what}: a ring must appear"
                    );
                    assert_eq!(e.undo_stack.len(), 1, "{what}: one press, one step");
                    assert_sound(&e.molecule, &what);
                    assert_eq!(
                        count_doubles(&e.molecule),
                        max_double_bonds(&e.molecule),
                        "{what}: double bonds"
                    );
                } else {
                    assert_eq!(e.molecule, base, "{what}: rings all round, no room");
                    assert!(e.undo_stack.is_empty(), "{what}: nothing to undo");
                }
            }
        }
    }
}

/// | atoms       | what `a` gives                 | atoms | bonds |
/// |-------------|--------------------------------|-------|-------|
/// | 6 outer CH  | anthracene or phenanthrene     | 14    | 16    |
/// | 2 outer CH  | phenalene (peri fusion)        | 13    | 15    |
/// | 2 fusion    | phenalene, on a free bond      | 13    | 15    |
#[test]
fn a_on_each_kind_of_naphthalene_atom() {
    let (base, _) = flake(NAPHTHALENE);
    let mut outer = HashMap::<(usize, usize), usize>::new();
    for a in &base.atoms {
        let mut e = fresh(&base);
        press_on_atom(&mut e, a.id, "a");
        let size = (e.molecule.atoms.len(), e.molecule.bonds.len());
        if degree(&base, a.id) == 3 {
            assert_eq!(size, (13, 15), "a fusion atom takes a peri ring");
        } else {
            *outer.entry(size).or_default() += 1;
        }
    }
    assert_eq!(outer, HashMap::from([((14, 16), 6), ((13, 15), 2)]));
}

/// The ring keys take the same road on an atom: no over-bonded carbon, and no step for nothing.
#[test]
fn ring_keys_on_every_atom_of_a_polycyclic() {
    for (name, hexes) in [
        ("naphthalene", NAPHTHALENE),
        ("pyrene", PYRENE),
        ("coronene", CORONENE),
    ] {
        let (base, rim) = flake(hexes);
        for key in ["4", "5", "6", "7", "8"] {
            for atom in base.atoms.iter().map(|a| a.id) {
                let mut e = fresh(&base);
                press_on_atom(&mut e, atom, key);
                let what = format!("`{key}` on {name} atom {atom}");
                assert_valences(&e.molecule, &what);
                assert_eq!(
                    e.undo_stack.len(),
                    usize::from(e.molecule != base),
                    "{what}: a step is a change"
                );
                if !rim.contains(&atom) {
                    assert_eq!(e.molecule, base, "{what}: rings all round, no room");
                }
            }
        }
    }
}

/// What was sound on an atom before stays. A chain end grows a phenyl (`c 1 1 a` is what the
/// biphenyl and triphenylphosphine goldens are built with), and an aromatic CH does not make a
/// biaryl and never did: its hexagon lies on the ring bond, so the result is a fused ring.
#[test]
fn a_keeps_what_was_sound_on_an_atom() {
    let (toluene, methyl, _) = toluene();
    let mut e = fresh(&toluene.molecule);
    press_on_atom(&mut e, methyl, "a");
    assert_eq!(e.molecule.atoms.len(), 12, "two rings on one bond");
    assert_eq!(e.molecule.bonds.len(), 13);
    assert_eq!(count_doubles(&e.molecule), 6, "two whole benzene rings");

    let ethane = type_keys("c 1").molecule;
    for a in &ethane.atoms {
        let mut e = fresh(&ethane);
        press_on_atom(&mut e, a.id, "a");
        assert_eq!(e.molecule.atoms.len(), 7, "a phenyl on a chain end");
        assert_eq!(e.molecule.bonds.len(), 7);
        assert_eq!(count_doubles(&e.molecule), 3);
        assert_valences(&e.molecule, "a phenyl on a chain end");
    }

    let (base, _) = benzene();
    for a in &base.molecule.atoms {
        let mut e = fresh(&base.molecule);
        press_on_atom(&mut e, a.id, "a");
        assert_eq!(e.molecule.atoms.len(), 10, "a ring carbon's `a` fuses");
        assert_eq!(e.molecule.bonds.len(), 11);
        assert_eq!(count_doubles(&e.molecule), 5, "naphthalene");
        assert_sound(&e.molecule, "`a` on benzene's carbon");
    }
}

/// An atom inside a chain: the hexagon lies along one of its bonds, and that bond is then a ring
/// bond of a benzene like any other.
#[test]
fn a_inside_a_chain_draws_a_whole_benzene_on_its_bond() {
    let propane = type_keys("c 1 1").molecule;
    let middle = propane
        .atoms
        .iter()
        .find(|a| degree(&propane, a.id) == 2)
        .unwrap()
        .id;
    let mut e = fresh(&propane);
    press_on_atom(&mut e, middle, "a");
    assert_eq!(e.molecule.atoms.len(), 7, "toluene's carbons");
    assert_eq!(e.molecule.bonds.len(), 7);
    assert_eq!(count_doubles(&e.molecule), 3, "and its three double bonds");
    assert_valences(&e.molecule, "toluene from propane");
}

/// Atoms that could not hold what was drawn on them, off the polycyclics. An `=CH₂` keeps its
/// double bond and gets a ring with one fewer; a tertiary carbon has no room for two new bonds,
/// so the ring goes on one of the bonds it already has.
#[test]
fn a_never_over_bonds_a_carbon() {
    let ethene = type_keys("c 1 Shift+ArrowLeft 2").molecule;
    for a in &ethene.atoms {
        let mut e = fresh(&ethene);
        press_on_atom(&mut e, a.id, "a");
        assert_eq!(e.molecule.atoms.len(), 7, "the ring is drawn");
        assert_valences(&e.molecule, "`a` on =CH₂");
    }

    let isobutane = type_keys("c 1 1 ArrowLeft 1").molecule;
    let tertiary = isobutane
        .atoms
        .iter()
        .find(|a| degree(&isobutane, a.id) == 3)
        .unwrap()
        .id;
    let mut e = fresh(&isobutane);
    press_on_atom(&mut e, tertiary, "a");
    assert_eq!(ring_count(&e.molecule), 1, "the ring is drawn");
    assert_valences(&e.molecule, "`a` on a tertiary carbon");

    let (toluene, _, ipso) = toluene();
    let mut e = fresh(&toluene.molecule);
    press_on_atom(&mut e, ipso, "a");
    assert_eq!(ring_count(&e.molecule), 2, "the ring is drawn");
    assert_valences(&e.molecule, "`a` on a ring carbon with a substituent");
}

/// Neopentane's middle carbon, charged so that the press would have rewritten it: there is no
/// bond to spare for a ring anywhere, so nothing may change, not even the atom.
#[test]
fn a_where_no_ring_can_go_is_no_undo_step() {
    let mut base = ChemStructEditor::default();
    let middle = base.molecule.add_atom("C".into(), [0.0, 0.0], 1);
    let mut far = 0;
    for pos in [[L, 0.0], [-L, 0.0], [0.0, L], [0.0, -L]] {
        far = base.molecule.add_atom("C".into(), pos, 0);
        base.molecule.add_bond(middle, far, BondOrder::Single);
    }

    let mut e = fresh(&base.molecule);
    press_on_atom(&mut e, far, "1");
    assert!(e.undo());
    assert_eq!(e.redo_stack.len(), 1);
    let undo_len = e.undo_stack.len();

    press_on_atom(&mut e, middle, "a");

    assert_eq!(e.molecule, base.molecule, "no room, so nothing changes");
    assert_eq!(e.undo_stack.len(), undo_len, "nothing to undo");
    assert_eq!(e.redo_stack.len(), 1, "and nothing was lost");
}

/// A heteroatom is left as the key has always left it, but a carbon next to one is held to four:
/// on quinoline's nitrogen, the ring goes on bonds the carbons can take, or nowhere.
#[test]
fn a_on_a_ring_nitrogen_does_not_over_bond_its_carbons() {
    let (base, _) = flake(NAPHTHALENE);
    for n in base.atoms.iter().map(|a| a.id) {
        let mut hetero = base.clone();
        hetero.atom_by_id_mut(n).unwrap().element = "N".into();
        let mut e = fresh(&hetero);
        press_on_atom(&mut e, n, "a");
        for a in e.molecule.atoms.iter().filter(|a| a.element == "C") {
            assert!(
                e.molecule.bond_order_sum(a.id) <= 4,
                "`a` on N at {n}: carbon {} over-bonded",
                a.id
            );
        }
    }
}

/// One press on an atom is one undo step, and undo puts back what re-deriving the pattern moved.
#[test]
fn a_on_an_atom_is_one_undo_step() {
    let (base, rim) = flake(PYRENE);
    for &atom in &rim {
        let mut e = fresh(&base);
        press_on_atom(&mut e, atom, "a");
        assert!(e.undo());
        assert_eq!(e.molecule, base, "atom {atom}: undo goes back to pyrene");
        assert!(!e.undo(), "atom {atom}: and that was the only step");
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Shift+A
// ═══════════════════════════════════════════════════════════════════════════════

/// `A` on a bond is `a`, whichever way the bond is targeted.
#[test]
fn shift_a_on_a_bond_is_a() {
    let base = naphthalene(0.0).molecule;
    for bond in base.bonds.iter().map(|b| b.id) {
        for hover in [false, true] {
            let mut lower = fresh(&base);
            let mut upper = fresh(&base);
            if hover {
                hover_and_press(&mut lower, bond, "a");
                hover_and_press(&mut upper, bond, "A");
            } else {
                press_on_bond(&mut lower, bond, "a");
                press_on_bond(&mut upper, bond, "A");
            }
            assert_eq!(upper.molecule, lower.molecule, "bond {bond} hover {hover}");
            assert_eq!(upper.undo_stack.len(), lower.undo_stack.len());
        }
    }
}

/// `A` on an atom already means something, and goes on meaning it: an acetyl group.
#[test]
fn shift_a_on_an_atom_is_still_acetyl() {
    let mut e = type_keys("c 1");
    let end = e.hotspot_atom.unwrap();
    press_on_atom(&mut e, end, "A");
    let oxygens = e.molecule.atoms.iter().filter(|a| a.element == "O").count();
    assert_eq!(oxygens, 1, "the acetyl's carbonyl oxygen");
    assert_eq!(
        e.molecule.atoms.len(),
        4,
        "the end carbon is now the C=O of C(=O)CH₃"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// The keyboard target
// ═══════════════════════════════════════════════════════════════════════════════

/// Press `key` `presses` times with nothing else in between, each one having to add a ring.
fn press_repeatedly(e: &mut ChemStructEditor, key: &str, presses: usize) {
    let ctx = egui::Context::default();
    for press in 1..=presses {
        let rings = ring_count(&e.molecule);
        let steps = e.undo_stack.len();
        kbd_frame(&ctx, e, key);
        let what = format!("`{key}` press {press}");
        assert_eq!(ring_count(&e.molecule), rings + 1, "{what}: one more ring");
        assert_eq!(e.undo_stack.len(), steps + 1, "{what}: one more step");
        assert_sound(&e.molecule, &what);
        assert_eq!(
            count_doubles(&e.molecule),
            max_double_bonds(&e.molecule),
            "{what}: double bonds"
        );
    }
}

/// Pressing `a` over and over from any bond of benzene draws a sheet, not a ribbon: the rings go
/// round the first one, which is coronene at seven, and round that, which is circumcoronene at
/// nineteen.
#[test]
fn repeated_a_with_no_other_input_grows_a_sheet() {
    let (base, _) = benzene();
    for bond in base.molecule.bonds.iter().map(|b| b.id) {
        let mut e = fresh(&base.molecule);
        e.hotspot_bond = Some(bond);

        press_repeatedly(&mut e, "a", 6);
        assert_eq!(e.molecule.atoms.len(), 24, "coronene has 24 carbons");
        assert_eq!(e.molecule.bonds.len(), 30, "coronene has 30 bonds");

        press_repeatedly(&mut e, "a", 12);
        assert_eq!(ring_count(&e.molecule), 19);
        assert_eq!(e.molecule.atoms.len(), 54, "circumcoronene has 54 carbons");
        assert_eq!(e.molecule.bonds.len(), 72, "circumcoronene has 72 bonds");
    }
}

/// Far enough out that rounding would have ruined the lattice if each ring were placed from one
/// bond: three shells round the first ring, with every bond still a bond long.
#[test]
fn a_sheet_stays_on_its_lattice() {
    let (base, _) = benzene();
    let mut e = fresh(&base.molecule);
    e.hotspot_bond = Some(base.molecule.bonds[0].id);
    press_repeatedly(&mut e, "a", 36);
    assert_eq!(ring_count(&e.molecule), 37);
    assert_eq!(e.molecule.atoms.len(), 96, "three shells have 96 carbons");
    assert_eq!(e.molecule.bonds.len(), 132);
}

/// The real sequence, keys only, and `A` repeats as well as `a` does.
#[test]
fn repeated_a_straight_after_aiming_with_the_arrows() {
    for key in ["a", "A"] {
        let mut e = type_keys("c a Shift+ArrowRight");
        press_repeatedly(&mut e, key, 8);
        assert_eq!(ring_count(&e.molecule), 9);
    }
}

/// The start need not be a lone ring: from every rim bond of a flake that takes a ring, it goes
/// on taking them.
#[test]
fn repeated_a_keeps_going_from_a_flake() {
    for hexes in [NAPHTHALENE, PYRENE, CORONENE] {
        let (base, _) = flake(hexes);
        for bond in base.bonds.iter().map(|b| b.id) {
            let mut e = fresh(&base);
            press_on_bond(&mut e, bond, "a");
            if ring_count(&e.molecule) > ring_count(&base) {
                press_repeatedly(&mut e, "a", 8);
            }
        }
    }
}

/// The target lands on a bond of the ring just drawn, with room on its far side, and the atom
/// hotspot stays so that the arrows walk on from where they were.
#[test]
fn the_target_moves_to_an_outer_bond_of_the_new_ring() {
    let (base, lattice) = benzene();
    let start = lattice
        .bond_at(&base.molecule, lattice.edge_mid((0, 0), 0))
        .unwrap();
    let mut e = fresh(&base.molecule);
    e.hotspot_atom = base.hotspot_atom;
    e.hotspot_bond = Some(start);

    kbd_frame(&egui::Context::default(), &mut e, "a");

    let target = e.hotspot_bond.expect("a target remains");
    assert_ne!(target, start, "the old bond has rings on both sides");
    let on_new_ring = (0..6)
        .filter_map(|k| lattice.bond_at(&e.molecule, lattice.edge_mid((1, 0), k)))
        .any(|b| b == target);
    assert!(on_new_ring, "the target is on the ring just drawn");
    assert_eq!(e.hotspot_atom, base.hotspot_atom);
}

/// A ring key moves the target too: it is the same press with a different ring.
#[test]
fn a_ring_key_moves_the_target_too() {
    let mut e = type_keys("c 6 Shift+ArrowRight");
    let ctx = egui::Context::default();
    for rings in 2..=7 {
        kbd_frame(&ctx, &mut e, "6");
        assert_eq!(ring_count(&e.molecule), rings);
    }
    assert_sound(&e.molecule, "saturated rings round a saturated ring");
    assert_eq!(count_doubles(&e.molecule), 0);
}

/// The mouse has its own target, and a hovered press leaves the keyboard's where it was.
#[test]
fn a_hovered_press_does_not_move_the_keyboard_target() {
    let (base, lattice) = benzene();
    let hovered = lattice
        .bond_at(&base.molecule, lattice.edge_mid((0, 0), 0))
        .unwrap();
    let parked = lattice
        .bond_at(&base.molecule, lattice.edge_mid((0, 0), 3))
        .unwrap();
    for target in [None, Some(parked)] {
        let mut e = fresh(&base.molecule);
        e.hotspot_bond = target;
        centre(&mut e.molecule);
        hover_and_press(&mut e, hovered, "a");
        assert_eq!(e.molecule.atoms.len(), 10, "the ring is drawn");
        assert_eq!(e.hotspot_bond, target, "the keyboard target stays put");
    }
}

/// A press that adds no ring does not move the target: there was no room, or it was not a ring.
#[test]
fn a_press_that_adds_no_ring_leaves_the_target_alone() {
    let mut e = naphthalene(0.0);
    let fusion = fusion_bond(&e.molecule);
    press_on_bond(&mut e, fusion, "a");
    assert_eq!(e.hotspot_bond, Some(fusion));

    let bond = peri_bond(&e.molecule);
    press_on_bond(&mut e, bond, "2");
    assert_eq!(e.hotspot_bond, Some(bond), "an order key draws no ring");
}
