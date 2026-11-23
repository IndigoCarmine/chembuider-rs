//! 2D clean-up of coordination complexes: the dative bonds (arrows from a ligand's donor atom
//! to a metal) of one structure come out equally long, and, being far weaker than the
//! ligands' own bonds, bend neither the ligands nor make any bond cross another.

use chembuider_rs::molecule::cleanup::cleanup_2d;
use chembuider_rs::{BondOrder, BondStereo, Molecule};

/// 1,10-Phenanthroline in a frame of its own: its nitrogens on the u axis either side of the
/// origin, the rings above them (v > 0). Kekulé bonds.
const PHEN_ATOMS: [(&str, f32, f32); 14] = [
    ("C", -1.5000, 2.5981),
    ("C", -0.7500, 1.2990),
    ("C", 0.7500, 1.2990),
    ("C", 1.5000, 2.5981),
    ("C", 0.7500, 3.8971),
    ("C", -0.7500, 3.8971),
    ("N", 1.5000, 0.0000),
    ("C", 3.0000, 0.0000),
    ("C", 3.7500, 1.2990),
    ("C", 3.0000, 2.5981),
    ("C", -3.7500, 1.2990),
    ("C", -3.0000, 0.0000),
    ("N", -1.5000, 0.0000),
    ("C", -3.0000, 2.5981),
];
const PHEN_BONDS: [(usize, usize, u8); 16] = [
    (0, 1, 2),
    (0, 5, 1),
    (0, 13, 1),
    (1, 2, 1),
    (1, 12, 1),
    (2, 3, 2),
    (2, 6, 1),
    (3, 4, 1),
    (3, 9, 1),
    (4, 5, 2),
    (6, 7, 2),
    (7, 8, 1),
    (8, 9, 2),
    (10, 11, 1),
    (10, 13, 2),
    (11, 12, 2),
];

fn dative(mol: &mut Molecule, donor: u32, acceptor: u32) {
    let id = mol.add_bond(donor, acceptor, BondOrder::Single);
    mol.bonds.iter_mut().find(|b| b.id == id).unwrap().stereo = BondStereo::Dative;
}

/// A phenanthroline placed with its nitrogens' midpoint at `origin`, the rings pointing along
/// `theta` (radians) away from it; its two nitrogens bound to `metal` by arrows.
fn add_phen(mol: &mut Molecule, metal: u32, origin: [f32; 2], theta: f32) {
    let (c, s) = (theta.cos(), theta.sin());
    let ids: Vec<u32> = PHEN_ATOMS
        .iter()
        .map(|&(element, u, v)| {
            let pos = [origin[0] - u * s + v * c, origin[1] + u * c + v * s];
            mol.add_atom(element.to_owned(), pos, 0)
        })
        .collect();
    for (a, b, order) in PHEN_BONDS {
        let order = if order == 2 { BondOrder::Double } else { BondOrder::Single };
        mol.add_bond(ids[a], ids[b], order);
    }
    for (i, &(element, _, _)) in PHEN_ATOMS.iter().enumerate() {
        if element == "N" {
            dative(mol, ids[i], metal);
        }
    }
}

/// [Fe(phen)₃]²⁺ roughly drawn: the three ligands around the iron, crowding each other.
fn fe_phen3() -> Molecule {
    let mut mol = Molecule::default();
    let fe = mol.add_atom("Fe".to_owned(), [0.0, 0.0], 2);
    for k in 0..3 {
        let theta = (90.0 + 120.0 * k as f32).to_radians();
        add_phen(&mut mol, fe, [1.5 * theta.cos(), 1.5 * theta.sin()], theta);
    }
    mol
}

fn is_dative(mol: &Molecule, bond: usize) -> bool {
    mol.bonds[bond].stereo == BondStereo::Dative
}

fn length(mol: &Molecule, bond: usize) -> f32 {
    let b = &mol.bonds[bond];
    let p = mol.atom_by_id(b.begin).unwrap().pos;
    let q = mol.atom_by_id(b.end).unwrap().pos;
    ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
}

/// Pairs of bonds sharing no atom whose segments cross.
fn crossings(mol: &Molecule) -> usize {
    let seg = |i: usize| {
        let b = &mol.bonds[i];
        (
            mol.atom_by_id(b.begin).unwrap().pos,
            mol.atom_by_id(b.end).unwrap().pos,
            b.begin,
            b.end,
        )
    };
    let cross = |p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], p4: [f32; 2]| {
        let d = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| {
            (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
        };
        let (d1, d2) = (d(p3, p4, p1), d(p3, p4, p2));
        let (d3, d4) = (d(p1, p2, p3), d(p1, p2, p4));
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    };
    let mut n = 0;
    for i in 0..mol.bonds.len() {
        for j in i + 1..mol.bonds.len() {
            let (a1, a2, i1, i2) = seg(i);
            let (b1, b2, j1, j2) = seg(j);
            if [i1, i2].iter().any(|x| [j1, j2].contains(x)) {
                continue;
            }
            if cross(a1, a2, b1, b2) {
                n += 1;
            }
        }
    }
    n
}

/// The largest departure (in the editor's bond length, 1.5) of an ordinary bond from 1.5.
fn worst_ordinary_bond(mol: &Molecule) -> f32 {
    (0..mol.bonds.len())
        .filter(|&i| !is_dative(mol, i))
        .map(|i| (length(mol, i) - 1.5).abs())
        .fold(0.0, f32::max)
}

/// The dative bonds' lengths, shortest first.
fn dative_lengths(mol: &Molecule) -> Vec<f32> {
    let mut v: Vec<f32> = (0..mol.bonds.len())
        .filter(|&i| is_dative(mol, i))
        .map(|i| length(mol, i))
        .collect();
    v.sort_by(f32::total_cmp);
    v
}

/// [Fe(phen)₃]²⁺ with its ligands `r` from the iron (1.5: crowding each other), each atom
/// nudged by up to `jitter` as a hand would leave it.
fn fe_phen3_at(r: f32, jitter: f32) -> Molecule {
    let mut mol = Molecule::default();
    let fe = mol.add_atom("Fe".to_owned(), [0.0, 0.0], 2);
    for k in 0..3 {
        let theta = (90.0 + 120.0 * k as f32).to_radians();
        add_phen(&mut mol, fe, [r * theta.cos(), r * theta.sin()], theta);
    }
    for (k, a) in mol.atoms.iter_mut().enumerate() {
        let t = k as f32 * 2.39996;
        a.pos[0] += jitter * t.sin();
        a.pos[1] += jitter * (1.7 * t).cos();
    }
    mol
}

/// The dative bonds' spread: longest over shortest.
fn spread(lengths: &[f32]) -> f32 {
    lengths[lengths.len() - 1] / lengths[0]
}

/// Ligands drawn across each other are moved apart, every arrow ends up equally long, and the
/// ligands keep their shape.
#[test]
fn a_crowded_complex_is_spread_out_with_equal_arrows() {
    let mut mol = fe_phen3_at(1.5, 0.0);
    assert!(crossings(&mol) > 0, "drawn crowded");
    cleanup_2d(&mut mol);
    let dative = dative_lengths(&mol);
    assert_eq!(crossings(&mol), 0, "nothing crosses");
    assert!(spread(&dative) < 1.01, "equal arrows: {dative:?}");
    assert!(worst_ordinary_bond(&mol) < 0.06, "undistorted: {}", worst_ordinary_bond(&mol));
}

/// A complex drawn roughly but without crossings: equal arrows, whole ligands, no crossing.
#[test]
fn a_rough_complex_gets_equal_arrows_and_keeps_its_ligands() {
    let mut mol = fe_phen3_at(3.0, 0.3);
    assert_eq!(crossings(&mol), 0);
    assert!(spread(&dative_lengths(&mol)) > 1.1, "drawn uneven");
    cleanup_2d(&mut mol);
    let dative = dative_lengths(&mol);
    assert_eq!(crossings(&mol), 0);
    assert!(spread(&dative) < 1.01, "equal arrows: {dative:?}");
    assert!(worst_ordinary_bond(&mol) < 0.06, "undistorted: {}", worst_ordinary_bond(&mol));
}

/// [Co(NH₃)₆]³⁺ with arrows of every length, and a second complex on the page: each comes
/// out with arrows equal among themselves, of the length its own ligands leave room for.
#[test]
fn each_complex_evens_out_its_own_arrows() {
    let mut mol = fe_phen3_at(3.0, 0.0);
    let co = mol.add_atom("Co".to_owned(), [20.0, 0.0], 3);
    for k in 0..6 {
        let a = (60.0 * k as f32 + 7.0 * k as f32).to_radians();
        let r = 1.2 + 0.25 * k as f32;
        let n = mol.add_atom("N".to_owned(), [20.0 + r * a.cos(), r * a.sin()], 0);
        dative(&mut mol, n, co);
    }
    cleanup_2d(&mut mol);
    let lengths = |metal: &str| {
        let m = mol.atoms.iter().find(|a| a.element == metal).unwrap().id;
        let mut v: Vec<f32> = (0..mol.bonds.len())
            .filter(|&i| is_dative(&mol, i) && mol.bonds[i].end == m)
            .map(|i| length(&mol, i))
            .collect();
        v.sort_by(f32::total_cmp);
        v
    };
    let (fe, co) = (lengths("Fe"), lengths("Co"));
    assert!(spread(&fe) < 1.01, "iron: {fe:?}");
    assert!(spread(&co) < 1.01, "cobalt: {co:?}");
    assert!((co[0] - fe[0]).abs() > 0.3, "each its own length: {fe:?} {co:?}");
    assert_eq!(crossings(&mol), 0);
}
