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

// ═══════════════════════════════════════════════════════════════════════════════
// Partly saturated systems
// ═══════════════════════════════════════════════════════════════════════════════

/// The atom at `p`.
fn atom_at(mol: &Molecule, p: [f32; 2]) -> u32 {
    mol.atoms
        .iter()
        .find(|a| (a.pos[0] - p[0]).hypot(a.pos[1] - p[1]) < 0.1)
        .expect("an atom on that vertex")
        .id
}

/// The atoms of `hex`.
fn atoms_of(mol: &Molecule, lattice: Lattice, hex: Hex) -> Vec<u32> {
    lattice
        .vertices(hex)
        .into_iter()
        .map(|p| atom_at(mol, p))
        .collect()
}

/// The atoms only `hex` has among `hexes`.
fn own_atoms(mol: &Molecule, lattice: Lattice, hexes: &[Hex], hex: Hex) -> Vec<u32> {
    let shared: BTreeSet<u32> = hexes
        .iter()
        .filter(|&&h| h != hex)
        .flat_map(|&h| atoms_of(mol, lattice, h))
        .collect();
    atoms_of(mol, lattice, hex)
        .into_iter()
        .filter(|id| !shared.contains(id))
        .collect()
}

/// `atoms`, which form an unbroken chain, walked from one end of it to the other.
fn in_chain_order(mol: &Molecule, atoms: &[u32]) -> Vec<u32> {
    let neighbours = |id: u32| -> Vec<u32> {
        mol.neighbor_atom_ids(id)
            .into_iter()
            .filter(|n| atoms.contains(n))
            .collect()
    };
    let start = atoms
        .iter()
        .copied()
        .find(|&id| neighbours(id).len() < 2)
        .expect("a chain has an end");
    let mut walk = vec![start];
    while let Some(next) = neighbours(*walk.last().unwrap())
        .into_iter()
        .find(|n| !walk.contains(n))
    {
        walk.push(next);
    }
    assert_eq!(walk.len(), atoms.len(), "the atoms form one chain");
    walk
}

/// Pair off `atoms` along bonds, every one of them, or say it cannot be done.
fn pair_off(mol: &Molecule, atoms: &mut Vec<u32>, pairs: &mut Vec<(u32, u32)>) -> bool {
    let Some(first) = atoms.pop() else {
        return true;
    };
    for partner in mol.neighbor_atom_ids(first) {
        if let Some(at) = atoms.iter().position(|&id| id == partner) {
            atoms.remove(at);
            pairs.push((first, partner));
            if pair_off(mol, atoms, pairs) {
                return true;
            }
            pairs.pop();
            atoms.insert(at, partner);
        }
    }
    atoms.push(first);
    false
}

/// Redraw the double bonds of `mol` as a Kekulé pattern of everything but `saturated`, whatever
/// they were. `false`, with the bonds as they were, if there is no such pattern.
fn kekulize_all_but(mol: &mut Molecule, saturated: &[u32]) -> bool {
    let mut atoms: Vec<u32> = mol
        .atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| !saturated.contains(id))
        .collect();
    let mut pairs = Vec::new();
    if !pair_off(mol, &mut atoms, &mut pairs) {
        return false;
    }
    for b in &mut mol.bonds {
        b.order = if pairs.contains(&(b.begin, b.end)) || pairs.contains(&(b.end, b.begin)) {
            BondOrder::Double
        } else {
            BondOrder::Single
        };
    }
    true
}

/// A flake of `hexes` and the lattice it lies on, drawn with the keys.
fn lattice_flake(hexes: &[Hex]) -> (Molecule, Lattice) {
    let (mut e, lattice) = benzene();
    let mut drawn = vec![hexes[0]];
    for &hex in &hexes[1..] {
        grow(&mut e, lattice, &mut drawn, hex);
    }
    (e.molecule, lattice)
}

/// `a` on the outer bonds of the benzene ring (0, 0) that `edges` picks. `kept` is out of reach of
/// the aromatic system that press extends: every bond of those atoms stays as it was, unless the
/// new ring itself runs through the atom.
fn assert_a_leaves_alone(
    mol: &Molecule,
    lattice: Lattice,
    edges: std::ops::Range<usize>,
    kept: &[u32],
    what: &str,
) {
    let hex = (0, 0);
    let mut rings_drawn = 0;
    for k in edges {
        let far = Lattice::neighbour(hex, k);
        let Some(bond) = lattice.bond_at(mol, lattice.edge_mid(hex, k)) else {
            continue;
        };
        if lattice.is_drawn(mol, far) {
            continue;
        }
        let mut e = fresh(mol);
        press_on_bond(&mut e, bond, "a");
        let what = format!("{what}, edge {k}");
        assert_valences(&e.molecule, &what);
        if !lattice.is_drawn(&e.molecule, far) {
            assert_eq!(&e.molecule, mol, "{what}: no ring, no change");
            continue;
        }
        rings_drawn += 1;

        let on_new_ring = atoms_of(&e.molecule, lattice, far);
        let off_the_ring = |id: u32| kept.contains(&id) && !on_new_ring.contains(&id);
        for b in &mol.bonds {
            if off_the_ring(b.begin) || off_the_ring(b.end) {
                assert_eq!(
                    e.molecule.bond_by_id(b.id).unwrap().order,
                    b.order,
                    "{what}: bond {}–{} changed",
                    b.begin,
                    b.end
                );
            }
        }
    }
    assert!(rings_drawn > 0, "{what}: some bond has to take a ring");
}

/// Naphthalene with ring (0, 0) benzene and the two given positions of ring (1, 0) saturated, and
/// that ring's four own atoms from one end to the other.
fn dihydronaphthalene(positions: [usize; 2]) -> (Molecule, Lattice, Vec<u32>) {
    let (mut mol, lattice) = lattice_flake(NAPHTHALENE);
    let own = own_atoms(&mol, lattice, NAPHTHALENE, (1, 0));
    let run = in_chain_order(&mol, &own);
    let saturated = positions.map(|i| run[i]);
    assert!(kekulize_all_but(&mut mol, &saturated));
    (mol, lattice, run)
}

#[test]
fn a_leaves_the_saturated_carbons_of_12_dihydronaphthalene() {
    let (mol, lattice, run) = dihydronaphthalene([0, 1]);
    assert_eq!(count_doubles(&mol), 4);
    assert_a_leaves_alone(&mol, lattice, 0..6, &run, "1,2-dihydronaphthalene");
}

#[test]
fn a_leaves_the_saturated_carbons_of_14_dihydronaphthalene() {
    let (mol, lattice, run) = dihydronaphthalene([0, 3]);
    assert_eq!(count_doubles(&mol), 4);
    assert_a_leaves_alone(&mol, lattice, 0..6, &run, "1,4-dihydronaphthalene");
}

#[test]
fn a_leaves_the_saturated_carbons_of_910_dihydroanthracene() {
    const ANTHRACENE: &[Hex] = &[(0, 0), (1, 0), (2, 0)];
    let (mut mol, lattice) = lattice_flake(ANTHRACENE);
    let mut kept = own_atoms(&mol, lattice, ANTHRACENE, (1, 0));
    assert_eq!(kept.len(), 2, "C9 and C10");
    assert!(kekulize_all_but(&mut mol, &kept));
    kept.extend(atoms_of(&mol, lattice, (2, 0)));
    assert_a_leaves_alone(&mol, lattice, 0..6, &kept, "9,10-dihydroanthracene");
}

#[test]
fn a_leaves_the_saturated_carbons_of_910_dihydrophenanthrene() {
    const PHENANTHRENE: &[Hex] = &[(0, 0), (1, 0), (1, 1)];
    let (mut mol, lattice) = lattice_flake(PHENANTHRENE);
    let mut kept = own_atoms(&mol, lattice, PHENANTHRENE, (1, 0));
    assert_eq!(kept.len(), 2, "C9 and C10");
    assert!(kekulize_all_but(&mut mol, &kept));
    kept.extend(atoms_of(&mol, lattice, (1, 1)));
    assert_a_leaves_alone(&mol, lattice, 0..6, &kept, "9,10-dihydrophenanthrene");
}

#[test]
fn a_leaves_the_saturated_carbons_of_1_indanone() {
    let (mut e, lattice) = benzene();
    let bond = lattice
        .bond_at(&e.molecule, lattice.edge_mid((0, 0), 0))
        .unwrap();
    press_on_bond(&mut e, bond, "5");
    let benzene_atoms = atoms_of(&e.molecule, lattice, (0, 0));
    let own: Vec<u32> = e
        .molecule
        .atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| !benzene_atoms.contains(id))
        .collect();
    assert_eq!(own.len(), 3, "the five-membered ring's own carbons");
    let c2 = own
        .iter()
        .copied()
        .find(|&id| {
            let m = &e.molecule;
            m.neighbor_atom_ids(id).iter().all(|n| own.contains(n))
        })
        .expect("the carbon between the other two");
    let c1 = own.iter().copied().find(|&id| id != c2).unwrap();
    let c3 = own
        .iter()
        .copied()
        .find(|&id| id != c2 && id != c1)
        .unwrap();

    let [x, y] = e.molecule.atom_by_id(c1).unwrap().pos;
    let [cx, cy] = lattice.origin;
    let scale = L / (x - cx).hypot(y - cy);
    let at = [x + (x - cx) * scale, y + (y - cy) * scale];
    let oxygen = e.molecule.add_atom("O".into(), at, 0);
    e.molecule.add_bond(c1, oxygen, BondOrder::Double);

    // The two bonds beside the five-membered ring have no hexagon of the lattice to take.
    let kept = [c1, c2, c3, oxygen];
    assert_a_leaves_alone(&e.molecule, lattice, 2..5, &kept, "1-indanone");
}

/// The wedge says which way a substituent points on an sp³ carbon: a double bond there would
/// contradict the drawing, whatever pattern it completed.
#[test]
fn a_leaves_a_wedged_stereocentre_saturated() {
    let (mut mol, lattice, run) = dihydronaphthalene([0, 1]);
    let c1 = run[0];
    let [x, y] = mol.atom_by_id(c1).unwrap().pos;
    let [cx, cy] = lattice.centre((1, 0));
    let scale = L / (x - cx).hypot(y - cy);
    let tip = mol.add_atom("C".into(), [x + (x - cx) * scale, y + (y - cy) * scale], 0);
    let wedge = mol.add_bond(c1, tip, BondOrder::Single);
    mol.bond_by_id_mut(wedge).unwrap().stereo = BondStereo::WedgeUp;

    let mut kept = run;
    kept.push(tip);
    assert_a_leaves_alone(&mol, lattice, 0..6, &kept, "the wedged stereocentre");
}

/// A carbon a rewrite gives a double bond to has to make room for it, wherever in the system it
/// is. Here it is phenalene's leftover, across the molecule from the ring that pairs it up.
#[test]
fn a_rewrite_consumes_the_hydrogens_it_leaves_no_room_for() {
    const PHENALENE: &[Hex] = &[(0, 0), (1, 0), (0, 1)];
    let (mut mol, lattice) = lattice_flake(PHENALENE);
    let leftover = own_atoms(&mol, lattice, PHENALENE, (0, 1))
        .into_iter()
        .find(|&id| mol.bond_order_sum(id) == 2)
        .expect("the last ring's press left one of its atoms over");
    let [x, y] = mol.atom_by_id(leftover).unwrap().pos;
    let [cx, cy] = lattice.centre((0, 1));
    for turn in [-30.0, 30.0] {
        let [dx, dy] = turned([x - cx, y - cy], turn);
        let h = mol.add_fragment_atom("H".into(), [x + dx * 0.7, y + dy * 0.7], 0);
        mol.add_bond(leftover, h, BondOrder::Single);
    }
    assert_eq!(mol.bond_order_sum(leftover), 4);

    let mut e = fresh(&mol);
    let bond = lattice.bond_at(&mol, lattice.edge_mid((0, 0), 5)).unwrap();
    press_on_bond(&mut e, bond, "a");

    assert!(lattice.is_drawn(&e.molecule, (1, -1)), "the ring is drawn");
    assert!(
        e.molecule
            .bonds_for_atom(leftover)
            .iter()
            .any(|b| b.order == BondOrder::Double),
        "the leftover pairs up: all of pyrene's sixteen carbons do"
    );
    assert_eq!(
        e.molecule.bond_order_sum(leftover),
        4,
        "no pentavalent carbon"
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Carbons drawn saturated
// ═══════════════════════════════════════════════════════════════════════════════

/// Two benzene rings on a central ring, and what bridges them: a lone CH₂, and for a six-membered
/// centre whatever the caller makes of `centre[2]`. The centre runs `centre[0]`, `centre[1]`, … and
/// the rings stand on `centre[0]`–`centre[1]` and the bond opposite it. Each ring is listed as a
/// walk starting along that bond backwards, so `rings[i][1]` is the lower-numbered centre atom and
/// `rings[i][2]` the benzene atom beside it.
struct Bridged {
    mol: Molecule,
    centre: Vec<u32>,
    rings: [Vec<u32>; 2],
    ch2: u32,
}

/// Give `ring`, a walk round six atoms, a Kekulé pattern: `phase` 0 starts on the bond from
/// `ring[0]` to `ring[1]`, and 1 on the next one.
fn alternate(mol: &mut Molecule, ring: &[u32], phase: usize) {
    for k in (phase..ring.len()).step_by(2) {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        let id = mol.bond_between(a, b).unwrap().id;
        mol.bond_by_id_mut(id).unwrap().order = BondOrder::Double;
    }
}

fn bridged(n: usize, phases: [usize; 2]) -> Bridged {
    let mut e = ChemStructEditor::default();
    let centre = polygon(&mut e, n, None);
    let far = n / 2;
    let rings = [(0, 1), (far, far + 1)].map(|(a, b)| {
        let edge = (centre[b], centre[a]);
        polygon(&mut e, 6, Some(edge))
    });
    for (ring, phase) in rings.iter().zip(phases) {
        alternate(&mut e.molecule, ring, phase);
    }
    Bridged {
        mol: e.molecule,
        ch2: centre[n - 1],
        centre,
        rings,
    }
}

fn fluorene(phases: [usize; 2]) -> Bridged {
    bridged(5, phases)
}

/// The six-membered centre with `element` opposite the CH₂: xanthene, thioxanthene and
/// 9,10-dihydroacridine.
fn bridged_by(element: &str, phases: [usize; 2]) -> Bridged {
    let mut b = bridged(6, phases);
    b.mol.atom_by_id_mut(b.centre[2]).unwrap().element = element.into();
    b
}

fn anthrone(phases: [usize; 2]) -> Bridged {
    let mut b = bridged(6, phases);
    let carbonyl = b.centre[2];
    let [x, y] = b.mol.atom_by_id(carbonyl).unwrap().pos;
    let n = b.centre.len() as f32;
    let middle = |axis: usize| {
        b.centre
            .iter()
            .map(|&id| b.mol.atom_by_id(id).unwrap().pos[axis])
            .sum::<f32>()
            / n
    };
    let (cx, cy) = (middle(0), middle(1));
    let scale = L / (x - cx).hypot(y - cy);
    let oxygen = b
        .mol
        .add_atom("O".into(), [x + (x - cx) * scale, y + (y - cy) * scale], 0);
    b.mol.add_bond(carbonyl, oxygen, BondOrder::Double);
    b
}

/// Every Kekulé pattern of the two benzene rings.
const PHASES: [[usize; 2]; 4] = [[0, 0], [0, 1], [1, 0], [1, 1]];

/// That no atom of `saturated` has come by a double bond, and nothing is over-bonded.
fn assert_still_saturated(mol: &Molecule, saturated: &[u32], what: &str) {
    for &id in saturated {
        let doubles: Vec<(u32, u32)> = mol
            .bonds_for_atom(id)
            .iter()
            .filter(|b| b.order == BondOrder::Double)
            .map(|b| (b.begin, b.end))
            .collect();
        assert!(doubles.is_empty(), "{what}: {id} gained {doubles:?}");
    }
    assert_valences(mol, what);
}

/// `a` on every bond of `mol`, and from each result `a` again, both on every bond and wherever the
/// target moved to by itself: no atom of `saturated` may come out with a double bond.
fn assert_a_keeps_saturated(mol: &Molecule, saturated: &[u32], what: &str) {
    let mut rings_drawn = 0;
    for first in mol.bonds.iter().map(|b| b.id) {
        let mut once = fresh(mol);
        press_on_bond(&mut once, first, "a");
        let what = format!("{what}, bond {first}");
        assert_still_saturated(&once.molecule, saturated, &what);
        if once.molecule.bonds.len() == mol.bonds.len() {
            continue;
        }
        rings_drawn += 1;

        let mut twice = fresh(mol);
        press_on_bond(&mut twice, first, "a");
        kbd_frame(&egui::Context::default(), &mut twice, "a");
        let moved_on = format!("{what}, then wherever the target went");
        assert_still_saturated(&twice.molecule, saturated, &moved_on);

        for second in once.molecule.bonds.iter().map(|b| b.id) {
            let mut twice = fresh(&once.molecule);
            press_on_bond(&mut twice, second, "a");
            let what = format!("{what}, then {second}");
            assert_still_saturated(&twice.molecule, saturated, &what);
        }
    }
    assert!(rings_drawn > 0, "{what}: some bond has to take a ring");
}

#[test]
fn a_leaves_the_ch2_of_fluorene() {
    for phases in PHASES {
        let f = fluorene(phases);
        assert_a_keeps_saturated(&f.mol, &[f.ch2], &format!("fluorene {phases:?}"));
    }
}

#[test]
fn a_leaves_the_ch2_of_xanthene() {
    for phases in PHASES {
        let f = bridged_by("O", phases);
        assert_a_keeps_saturated(&f.mol, &[f.ch2], &format!("xanthene {phases:?}"));
    }
}

#[test]
fn a_leaves_the_ch2_of_dihydroacridine() {
    for phases in PHASES {
        let f = bridged_by("N", phases);
        let saturated = [f.ch2, f.centre[2]];
        assert_a_keeps_saturated(&f.mol, &saturated, &format!("dihydroacridine {phases:?}"));
    }
}

#[test]
fn a_leaves_the_ch2_of_thioxanthene() {
    for phases in PHASES {
        let f = bridged_by("S", phases);
        assert_a_keeps_saturated(&f.mol, &[f.ch2], &format!("thioxanthene {phases:?}"));
    }
}

#[test]
fn a_leaves_the_ch2_of_anthrone() {
    for phases in PHASES {
        let f = anthrone(phases);
        assert_a_keeps_saturated(&f.mol, &[f.ch2], &format!("anthrone {phases:?}"));
    }
}

/// Two lone CH₂ groups: a ring through the one must not make the other look like a leftover.
#[test]
fn a_leaves_the_ch2_groups_of_dihydroanthracene() {
    for phases in PHASES {
        let f = bridged(6, phases);
        let saturated = [f.ch2, f.centre[2]];
        let what = format!("dihydroanthracene {phases:?}");
        assert_a_keeps_saturated(&f.mol, &saturated, &what);
    }
}

/// Indene, drawn both ways round: the CH₂ can sit on either side of the five-membered ring's C=C.
#[test]
fn a_leaves_the_ch2_of_indene() {
    for phase in [0, 1] {
        for (ch2, double) in [(4, (2, 3)), (2, (3, 4))] {
            let mut e = ChemStructEditor::default();
            let five = polygon(&mut e, 5, None);
            let six = polygon(&mut e, 6, Some((five[1], five[0])));
            alternate(&mut e.molecule, &six, phase);
            let id = e
                .molecule
                .bond_between(five[double.0], five[double.1])
                .unwrap()
                .id;
            e.molecule.bond_by_id_mut(id).unwrap().order = BondOrder::Double;
            let what = format!("indene, phase {phase}, CH2 at {ch2}");
            assert_a_keeps_saturated(&e.molecule, &[five[ch2]], &what);
        }
    }
}

/// The hexagon on these bonds comes down on the CH₂ itself: the outer bonds beside the bridge and
/// the bridge's own bonds. The ring is still drawn, the CH₂ stays one, and the rest of the ring
/// takes the best pattern there is around it.
#[test]
fn a_ring_drawn_through_a_ch2_goes_round_it() {
    type Make = fn([usize; 2]) -> Bridged;
    let molecules: [(&str, Make); 3] = [
        ("anthrone", anthrone),
        ("xanthene", |p| bridged_by("O", p)),
        ("dihydroacridine", |p| bridged_by("N", p)),
    ];
    for (name, make) in molecules {
        for phases in PHASES {
            let f = make(phases);
            let ch2 = f.ch2;
            let (first, last) = (f.centre[0], f.centre[4]);
            for (u, v) in [
                (f.rings[0][2], first),
                (f.rings[1][5], last),
                (last, ch2),
                (ch2, first),
            ] {
                let what = format!("{name} {phases:?}, bond {u}-{v}");
                let bond = f.mol.bond_between(u, v).unwrap().id;
                let mut e = fresh(&f.mol);
                press_on_bond(&mut e, bond, "a");
                let m = &e.molecule;
                assert_eq!(ring_count(m), ring_count(&f.mol) + 1, "{what}: the ring");
                assert_still_saturated(m, &[ch2], &what);
                for end in [first, last] {
                    assert_eq!(
                        m.bond_between(end, ch2).unwrap().order,
                        BondOrder::Single,
                        "{what}: the bridge's bonds stay single"
                    );
                }

                // All that can carry a double bond here is the skeleton left without the CH₂ and
                // the atoms that speak for themselves: heteroatoms, and the carbonyl's carbon.
                let carbonyl = m
                    .atoms
                    .iter()
                    .filter(|a| a.element == "O" && degree(m, a.id) == 1)
                    .map(|a| m.neighbor_atom_ids(a.id)[0]);
                let elsewhere = m.atoms.iter().filter(|a| a.element != "C").map(|a| a.id);
                let inert: Vec<u32> = elsewhere.chain(carbonyl).chain([ch2]).collect();
                let mut rest = m.clone();
                for &id in &inert {
                    rest.remove_atom(id);
                }
                assert_eq!(
                    count_doubles(&rest),
                    max_double_bonds(&rest),
                    "{what}: double bonds"
                );
            }
        }
    }
}

/// Pointing at the CH₂ itself is asking for the ring there: it joins the ring's pattern, and the
/// exception goes no further than that one atom. The next CH₂ along is nobody's target.
#[test]
fn a_aimed_at_a_ch2_is_the_users_to_ask_for() {
    let f = bridged_by("O", [0, 0]);
    let mut e = fresh(&f.mol);
    press_on_atom(&mut e, f.ch2, "a");
    assert_eq!(ring_count(&e.molecule), ring_count(&f.mol) + 1);
    assert!(
        e.molecule
            .bonds_for_atom(f.ch2)
            .iter()
            .any(|b| b.order == BondOrder::Double),
        "the atom the user pointed at takes part"
    );
    assert_valences(&e.molecule, "`a` on the CH2");

    const PHENANTHRENE: &[Hex] = &[(0, 0), (1, 0), (1, 1)];
    let (mut mol, lattice) = lattice_flake(PHENANTHRENE);
    let both = own_atoms(&mol, lattice, PHENANTHRENE, (1, 0));
    assert!(kekulize_all_but(&mut mol, &both));
    let (aimed, other) = (both[0], both[1]);
    // On the bond out of the benzene ring, not the one between the two CH₂ groups, so that the
    // press has to rewrite the pattern.
    let mut e = fresh(&mol);
    let beside = mol
        .neighbor_atom_ids(aimed)
        .into_iter()
        .find(|&id| id != other)
        .unwrap();
    let bond = mol.bond_between(aimed, beside).unwrap().id;
    press_on_bond(&mut e, bond, "a");
    assert_still_saturated(&e.molecule, &both, "the bond beside it");
    let mut e = fresh(&mol);
    press_on_atom(&mut e, aimed, "a");
    assert_eq!(ring_count(&e.molecule), ring_count(&mol) + 1);
    assert_still_saturated(&e.molecule, &[other], "the other CH2");
}

/// Repeated presses from an atom go on from wherever the last ring left off, and that can be a CH₂
/// a ring went through. Pressing on from there is not the user pointing at it. Going there with
/// the arrows is.
#[test]
fn repeated_a_from_an_atom_does_not_count_as_aiming_at_a_ch2() {
    type Make = fn([usize; 2]) -> Bridged;
    let molecules: [(&str, Make); 3] = [
        ("xanthene", |p| bridged_by("O", p)),
        ("thioxanthene", |p| bridged_by("S", p)),
        ("anthrone", anthrone),
    ];
    let mut went_on_from_it = 0;
    for (name, make) in molecules {
        for phases in PHASES {
            let f = make(phases);
            let carbons = f.mol.atoms.iter().filter(|a| a.element == "C");
            for start in carbons.map(|a| a.id).filter(|&id| id != f.ch2) {
                let mut e = fresh(&f.mol);
                press_on_atom(&mut e, start, "a");
                let ctx = egui::Context::default();
                for press in 2..=10 {
                    let next = e.ring_repeat.as_ref().map(|r| r.atom);
                    went_on_from_it += usize::from(next == Some(f.ch2));
                    kbd_frame(&ctx, &mut e, "a");
                    let what = format!("{name} {phases:?}, from {start}, press {press}");
                    assert_still_saturated(&e.molecule, &[f.ch2], &what);
                }
            }
        }
    }
    assert!(went_on_from_it > 0, "some repeat has to go on from the CH2");

    // The same atom, reached with the arrows, is pointed at.
    let f = bridged_by("O", [0, 0]);
    let start = f.centre[0];
    let mut e = fresh(&f.mol);
    e.hotspot_atom = Some(start);
    let [dx, dy] = {
        let (p, q) = (
            f.mol.atom_by_id(start).unwrap().pos,
            f.mol.atom_by_id(f.ch2).unwrap().pos,
        );
        [q[0] - p[0], q[1] - p[1]]
    };
    let arrow = if dx.abs() > dy.abs() {
        if dx > 0.0 { "ArrowRight" } else { "ArrowLeft" }
    } else if dy > 0.0 {
        "ArrowDown"
    } else {
        "ArrowUp"
    };
    let ctx = egui::Context::default();
    kbd_frame(&ctx, &mut e, arrow);
    assert_eq!(e.hotspot_atom, Some(f.ch2), "the arrow goes to the CH2");
    kbd_frame(&ctx, &mut e, "a");
    assert!(
        e.molecule
            .bonds_for_atom(f.ch2)
            .iter()
            .any(|b| b.order == BondOrder::Double),
        "pressed there on purpose"
    );
}

/// What a press left over is what the next one may pair, for as long as it is still in the
/// history: undone with the press that made it, and back with the redo.
#[test]
fn a_leftover_follows_undo_and_redo() {
    let mut e = naphthalene(0.0);
    let bond = peri_bond(&e.molecule);
    press_on_bond(&mut e, bond, "a");
    let phenalene = e.molecule.clone();
    assert_eq!(phenalene.press_leftovers.len(), 1, "13 carbons, 6 pairs");

    assert!(e.undo());
    assert!(e.molecule.press_leftovers.is_empty());
    assert!(e.redo());
    assert_eq!(e.molecule, phenalene);
    assert_eq!(e.molecule.press_leftovers, phenalene.press_leftovers);
}

/// Working out where the target goes next tries presses and takes them back, and those can pair
/// the leftover up. None of that may reach the record.
#[test]
fn a_press_taken_back_records_nothing() {
    let mut e = naphthalene(0.0);
    let bond = peri_bond(&e.molecule);
    press_on_bond(&mut e, bond, "a");
    assert_ne!(e.hotspot_bond, Some(bond), "the trial presses ran");

    let single: BTreeSet<u32> = e
        .molecule
        .atoms
        .iter()
        .map(|a| a.id)
        .filter(|&id| {
            let bonds = e.molecule.bonds_for_atom(id);
            bonds.iter().all(|b| b.order == BondOrder::Single)
        })
        .collect();
    assert_eq!(e.molecule.press_leftovers, single, "the one left over");
}

/// The record is of the drawing, not a licence: a wedge drawn on the leftover since says it is
/// sp³, deleting it forgets it, and pairing it by hand or by a ring strikes it off.
#[test]
fn edits_between_presses_keep_the_record_true() {
    const PHENALENE: &[Hex] = &[(0, 0), (1, 0), (0, 1)];
    let (mol, lattice) = lattice_flake(PHENALENE);
    let leftover = *mol.press_leftovers.first().expect("phenalene leaves one");
    let bond = lattice.bond_at(&mol, lattice.edge_mid((0, 0), 5)).unwrap();

    let mut paired = fresh(&mol);
    press_on_bond(&mut paired, bond, "a");
    assert!(
        paired.molecule.press_leftovers.is_empty(),
        "pyrene has none left: {:?}",
        paired.molecule.press_leftovers
    );

    let mut wedged = mol.clone();
    let [x, y] = mol.atom_by_id(leftover).unwrap().pos;
    let [cx, cy] = lattice.centre((0, 1));
    let scale = L / (x - cx).hypot(y - cy);
    let tip = wedged.add_atom("C".into(), [x + (x - cx) * scale, y + (y - cy) * scale], 0);
    let wedge = wedged.add_bond(leftover, tip, BondOrder::Single);
    wedged.bond_by_id_mut(wedge).unwrap().stereo = BondStereo::WedgeUp;
    let mut e = fresh(&wedged);
    press_on_bond(&mut e, bond, "a");
    assert!(lattice.is_drawn(&e.molecule, (1, -1)), "the ring is drawn");
    assert_still_saturated(&e.molecule, &[leftover], "the wedged leftover");

    let mut deleted = mol.clone();
    deleted.remove_atom(leftover);
    assert!(deleted.press_leftovers.is_empty());
}

/// Manually choosing a bond order makes its ends part of the drawing, even after changing back.
#[test]
fn manual_bond_orders_forget_a_press_leftover() {
    let mut e = naphthalene(0.0);
    let bond = peri_bond(&e.molecule);
    press_on_bond(&mut e, bond, "a");
    assert_eq!(e.molecule.atoms.len(), 13);
    assert_eq!(e.molecule.press_leftovers, BTreeSet::from([24]));

    for key in ["1", "2", "1"] {
        press_on_bond(&mut e, 27, key);
    }
    assert_eq!(e.molecule.bond_by_id(27).unwrap().order, BondOrder::Single);
    let json = serde_json::to_string(&e.molecule).unwrap();
    let mut reloaded = fresh(&serde_json::from_str(&json).unwrap());
    press_on_bond(&mut reloaded, 9, "a");
    press_on_bond(&mut e, 9, "a");
    assert_still_saturated(&reloaded.molecule, &[24], "the reloaded drawing");
    assert_still_saturated(&e.molecule, &[24], "the manually edited drawing");
    assert_eq!(e.molecule, reloaded.molecule);
}

#[test]
fn deleting_and_redrawing_a_bond_forgets_a_press_leftover() {
    let mut e = naphthalene(0.0);
    let bond = peri_bond(&e.molecule);
    press_on_bond(&mut e, bond, "a");
    assert_eq!(e.molecule.press_leftovers, BTreeSet::from([24]));
    let removed = e.molecule.bond_by_id(27).unwrap().clone();
    assert!(e.delete_bond(27));
    e.molecule
        .add_bond(removed.begin, removed.end, removed.order);
    assert!(e.molecule.press_leftovers.is_empty());

    let json = serde_json::to_string(&e.molecule).unwrap();
    let mut reloaded = fresh(&serde_json::from_str(&json).unwrap());
    press_on_bond(&mut reloaded, 9, "a");
    press_on_bond(&mut e, 9, "a");
    assert_still_saturated(&e.molecule, &[24], "the deleted and redrawn drawing");
    assert_eq!(e.molecule, reloaded.molecule);
}

#[test]
fn drawing_a_bond_forgets_a_press_leftover() {
    for existing_target in [false, true] {
        let mut e = naphthalene(0.0);
        let bond = peri_bond(&e.molecule);
        press_on_bond(&mut e, bond, "a");
        assert_eq!(e.molecule.press_leftovers, BTreeSet::from([24]));
        let p = e.molecule.atom_by_id(24).unwrap().pos;
        let tip = [p[0] + 2.0 * L, p[1] + 2.0 * L];
        let target = existing_target.then(|| e.molecule.add_atom("C".into(), tip, 0));
        e.tool = super::Tool::Bond;
        let start = e.mol_to_screen(p, CANVAS.center());
        let end = e.mol_to_screen(tip, CANVAS.center());
        let ctx = egui::Context::default();
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        run_frame(&ctx, CANVAS, &mut e, vec![egui::Event::PointerMoved(start)]);
        run_frame(&ctx, CANVAS, &mut e, vec![button(start, true)]);
        run_frame(&ctx, CANVAS, &mut e, vec![egui::Event::PointerMoved(end)]);
        run_frame(&ctx, CANVAS, &mut e, vec![button(end, false)]);
        let drawn = e.molecule.bonds_for_atom(24);
        assert_eq!(drawn.len(), 3, "the substituent was drawn");
        if let Some(target) = target {
            assert!(e.molecule.bond_between(24, target).is_some());
        }
        assert!(
            e.molecule.press_leftovers.is_empty(),
            "existing target: {existing_target}"
        );
    }
}

/// The record is not in the file, and a leftover read back from one is a plain CH₂: it stays one,
/// which is a sound drawing, one double bond short of the most it could have.
#[test]
fn a_leftover_is_not_saved() {
    const PHENALENE: &[Hex] = &[(0, 0), (1, 0), (0, 1)];
    let (mol, lattice) = lattice_flake(PHENALENE);
    let leftover = *mol.press_leftovers.first().expect("phenalene leaves one");

    let json = serde_json::to_string(&mol).unwrap();
    assert!(!json.contains("press_leftovers"), "{json}");
    let read: Molecule = serde_json::from_str(&json).unwrap();
    assert!(read.press_leftovers.is_empty());

    let bond = lattice.bond_at(&read, lattice.edge_mid((0, 0), 5)).unwrap();
    let mut e = fresh(&read);
    press_on_bond(&mut e, bond, "a");
    assert!(lattice.is_drawn(&e.molecule, (1, -1)), "the ring is drawn");
    assert_still_saturated(&e.molecule, &[leftover], "a CH2 read from a file");
}

// ═══════════════════════════════════════════════════════════════════════════════
// The keyboard target from an atom, and undo
// ═══════════════════════════════════════════════════════════════════════════════

/// The atom target goes on as the bond target does. The atom a ring ends on can be a ring-fusion
/// atom with nowhere to go, and that must not be the end of the sheet.
#[test]
fn repeated_a_from_an_atom_target_grows_a_sheet() {
    let mut e = type_keys("c a");
    assert!(e.hotspot_bond.is_none(), "the target is the atom");
    press_repeatedly(&mut e, "a", 36);
    assert_eq!(ring_count(&e.molecule), 37);
}

/// The arrows walk from where the ring left the hotspot, however it would have gone on: going on
/// is what the same key does, and nothing else.
#[test]
fn only_the_same_key_goes_on_from_the_next_atom() {
    let mut e = type_keys("c a");
    let ctx = egui::Context::default();
    while e.ring_repeat.as_ref().is_none_or(|r| r.atom == r.from) {
        kbd_frame(&ctx, &mut e, "a");
        assert!(
            ring_count(&e.molecule) < 25,
            "a press has to end on a full atom"
        );
    }
    let parked = e.hotspot_atom.unwrap();
    assert_eq!(e.ring_repeat.as_ref().unwrap().from, parked);
    assert_ne!(e.ring_repeat.as_ref().unwrap().atom, parked);

    let mut other_key = fresh(&e.molecule);
    other_key.hotspot_atom = e.hotspot_atom;
    other_key.ring_repeat = e.ring_repeat.clone();
    kbd_frame(&ctx, &mut other_key, "v");
    assert!(
        other_key.ring_repeat.is_none(),
        "another key ends the repeat"
    );

    let rings = ring_count(&e.molecule);
    let (dx, dy) = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
        .into_iter()
        .find(|&(dx, dy)| {
            crate::widget::interact::navigate_direction(&e.molecule, parked, dx, dy).is_some()
        })
        .expect("a ring atom has a neighbour");
    let arrow = match (dx as i32, dy as i32) {
        (1, _) => "ArrowRight",
        (-1, _) => "ArrowLeft",
        (_, 1) => "ArrowDown",
        _ => "ArrowUp",
    };
    let walked = crate::widget::interact::navigate_direction(&e.molecule, parked, dx, dy);
    kbd_frame(&ctx, &mut e, arrow);
    assert_eq!(e.hotspot_atom, walked, "the arrow walks from the hotspot");
    assert!(e.ring_repeat.is_none(), "and ends the repeat");
    assert_eq!(ring_count(&e.molecule), rings);
}

/// A ring key from an atom is the same press with a different ring.
#[test]
fn repeated_ring_key_from_an_atom_target_keeps_adding_rings() {
    let mut e = type_keys("c 6");
    let ctx = egui::Context::default();
    for rings in 2..=12 {
        kbd_frame(&ctx, &mut e, "6");
        assert_eq!(ring_count(&e.molecule), rings);
    }
    assert_sound(&e.molecule, "saturated rings from an atom");
}

/// The Select tool's `a` presses on the same atom target.
#[test]
fn repeated_a_from_an_atom_target_in_the_select_tool() {
    let mut e = type_keys("c a");
    e.tool = crate::widget::Tool::Select;
    let ctx = egui::Context::default();
    for rings in 2..=12 {
        kbd_frame(&ctx, &mut e, "a");
        assert_eq!(ring_count(&e.molecule), rings);
    }
}

/// Overshoot, undo, carry on: the natural way to draw a sheet. The press after the undo does what
/// the undone one did, from a bond target and from an atom target, at every depth: some of these
/// presses go on from the hotspot and some from the next atom.
#[test]
fn a_a_undo_a_carries_on_from_the_same_target() {
    for start in ["c a Shift+ArrowRight", "c a"] {
        for presses in 1..=7 {
            let what = format!("{start}, {presses} presses");
            let mut e = type_keys(start);
            press_repeatedly(&mut e, "a", presses);
            let before = e.molecule.clone();
            press_repeatedly(&mut e, "a", 1);
            let after = e.molecule.clone();

            kbd_frame(&egui::Context::default(), &mut e, "Ctrl+Z");
            assert_eq!(e.molecule, before, "{what}: undo takes one press back");

            press_repeatedly(&mut e, "a", 1);
            assert_eq!(e.molecule, after, "{what}: the press is drawn again");
        }
    }
}

/// Redo puts the target where the redone press left it, so a redo does not stop the next press
/// either.
#[test]
fn a_undo_redo_a_carries_on_from_the_same_target() {
    for start in ["c a Shift+ArrowRight", "c a"] {
        for presses in 1..=7 {
            let mut e = type_keys(start);
            press_repeatedly(&mut e, "a", presses);
            let ctx = egui::Context::default();
            kbd_frame(&ctx, &mut e, "Ctrl+Z");
            kbd_frame(&ctx, &mut e, "Ctrl+Y");
            press_repeatedly(&mut e, "a", 1);
        }
    }
}
