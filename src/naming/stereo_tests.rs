//! Stereodescriptors (P-9): E/Z from the drawn geometry of double bonds, R/S from wedges and
//! hashes, ranked by the CIP rules (P-92).
//!
//! The structures are drawn here with explicit coordinates, as a textbook figure: x to the
//! right, y *upward* (the molecule stores y downward, so a reading that forgot to flip it would
//! name every centre's mirror image), bonds one unit long. Each expected descriptor was worked
//! out by hand from the figure: the ligands' CIP ranks, then the sense of rotation 1 → 2 → 3 seen
//! with the lowest-ranked ligand away from the viewer.
//!
//! IUPAC 2013 cites a descriptor with the locant of its unit whenever the name cites that locant
//! — "(2R)-butan-2-ol (PIN)", "(2Z)-but-2-ene (PIN)" (P-93.4.1.1) — where Wikipedia and many
//! textbooks write the shorter "(E)-pent-2-ene", "(R)-butan-2-ol" for a compound with one
//! stereogenic unit. Where the name cites no locant for the unit, the descriptor has none either:
//! "(R)-bromo(chloro)(fluoro)methane (PIN)" (P-91.3), "(E)-cyclooctene (PIN)" (P-93.5.1.4.1).
//! The names here follow 2013.

use super::{NameResult, iupac_name, iupac_name_with_notes};
use crate::molecule::{BondOrder, BondStereo, Molecule};

/// A molecule drawn from `atoms` (element, x, y with y upward) and `bonds` (from, to, kind):
/// `-` single, `=` double, `w` solid wedge, `h` hashed wedge, `~` wavy, `b` bold, `H` plain
/// hashed bond. A wedge's narrow end is at `from`.
fn draw(atoms: &[(&str, f32, f32)], bonds: &[(usize, usize, char)]) -> Molecule {
    let mut mol = Molecule::default();
    let ids: Vec<u32> = atoms
        .iter()
        .map(|&(el, x, y)| mol.add_atom(el.into(), [x * 1.5, -y * 1.5], 0))
        .collect();
    for &(a, b, kind) in bonds {
        let order = if kind == '=' {
            BondOrder::Double
        } else {
            BondOrder::Single
        };
        mol.add_bond(ids[a], ids[b], order);
        mol.bonds.last_mut().unwrap().stereo = match kind {
            '-' | '=' => BondStereo::None,
            'w' => BondStereo::WedgeUp,
            'h' => BondStereo::WedgeDown,
            '~' => BondStereo::Wavy,
            'b' => BondStereo::Bold,
            'H' => BondStereo::Hashed,
            _ => panic!("bond kind {kind:?}"),
        };
    }
    mol
}

/// The same drawing with every solid wedge hashed and every hashed wedge solid: the mirror
/// image.
fn mirrored(mut mol: Molecule) -> Molecule {
    for b in &mut mol.bonds {
        b.stereo = match std::mem::take(&mut b.stereo) {
            BondStereo::WedgeUp => BondStereo::WedgeDown,
            BondStereo::WedgeDown => BondStereo::WedgeUp,
            s => s,
        };
    }
    mol
}

/// The `k`-th atom of a zigzag chain drawn left to right, starting low.
fn zz(k: usize) -> (f32, f32) {
    (0.866 * k as f32, if k % 2 == 1 { 0.5 } else { 0.0 })
}

/// Atoms to draw: element, x, y (upward).
type Atoms = Vec<(&'static str, f32, f32)>;
/// Bonds to draw: from, to, kind (see [`draw`]).
type Bonds = Vec<(usize, usize, char)>;

/// A zigzag chain of `n` carbons (atoms 0..n), single bonds except the `doubles` (index of the
/// first atom of each).
fn chain(n: usize, doubles: &[usize]) -> (Atoms, Bonds) {
    let atoms = (0..n)
        .map(|k| {
            let (x, y) = zz(k);
            ("C", x, y)
        })
        .collect();
    let bonds = (0..n - 1)
        .map(|k| (k, k + 1, if doubles.contains(&k) { '=' } else { '-' }))
        .collect();
    (atoms, bonds)
}

/// `chain`, plus `extra` atoms bonded to chain atoms: (element, x, y, chain atom, bond kind).
fn chain_with(
    n: usize,
    doubles: &[usize],
    extra: &[(&'static str, f32, f32, usize, char)],
) -> Molecule {
    let (mut atoms, mut bonds) = chain(n, doubles);
    for &(el, x, y, on, kind) in extra {
        bonds.push((on, atoms.len(), kind));
        atoms.push((el, x, y));
    }
    draw(&atoms, &bonds)
}

fn name(mol: &Molecule) -> String {
    match iupac_name(mol) {
        NameResult::Ok(n) => n,
        other => panic!("not named: {other:?}"),
    }
}

/// The name and the notes about stereo units left without a descriptor.
fn noted(mol: &Molecule) -> (String, Vec<String>) {
    match iupac_name_with_notes(mol) {
        (NameResult::Ok(n), notes) => (n, notes),
        other => panic!("not named: {other:?}"),
    }
}

/// Butan-2-ol (or 2-bromobutane with `x` = "Br") with the group on C2 drawn upward as `kind`.
fn butan_2_x(x: &'static str, kind: char) -> Molecule {
    chain_with(4, &[], &[(x, 0.866, 1.5, 1, kind)])
}

/// The textbook example: the hydroxy group on a solid wedge, C1 lower left, C3 lower right, the
/// hydrogen behind: O → C3 → C1 is clockwise, R.
#[test]
fn butan_2_ol() {
    assert_eq!(name(&butan_2_x("O", 'w')), "(2R)-butan-2-ol"); // Wikipedia: "(R)-butan-2-ol"
    assert_eq!(name(&butan_2_x("O", 'h')), "(2S)-butan-2-ol");
}

#[test]
fn bromobutane() {
    assert_eq!(name(&butan_2_x("Br", 'w')), "(2R)-2-bromobutane");
    assert_eq!(name(&butan_2_x("Br", 'h')), "(2S)-2-bromobutane");
}

/// The hydrogen drawn as an atom, on a hashed wedge, the hydroxy group on a plain bond: O (upper
/// left) → C3 → C1 clockwise with H behind, R.
#[test]
fn drawn_hydrogen_on_a_wedge() {
    let (c2x, c2y) = zz(1);
    let mol = chain_with(
        4,
        &[],
        &[
            ("O", c2x - 0.5, c2y + 0.866, 1, '-'),
            ("H", c2x + 0.5, c2y + 0.866, 1, 'h'),
        ],
    );
    assert_eq!(name(&mol), "(2R)-butan-2-ol");
    assert_eq!(name(&mirrored(mol)), "(2S)-butan-2-ol");
}

/// Mirroring the wedges inverts every centre.
#[test]
fn reversed_wedges_invert_the_descriptor() {
    for (mol, r, s) in [
        (butan_2_x("O", 'w'), "(2R)-butan-2-ol", "(2S)-butan-2-ol"),
        (
            butan_2_x("Br", 'w'),
            "(2R)-2-bromobutane",
            "(2S)-2-bromobutane",
        ),
        (
            alanine('h'),
            "(2S)-2-aminopropanoic acid",
            "(2R)-2-aminopropanoic acid",
        ),
    ] {
        assert_eq!(name(&mol), r);
        assert_eq!(name(&mirrored(mol)), s);
    }
}

/// The alanine skeleton (the retained name "alanine" is not produced): N > COOH > CH3 > H. With
/// the amino group hashed, the hydrogen is toward the viewer and N → COOH → CH3, clockwise as
/// drawn, is S — L-alanine.
fn alanine(kind: char) -> Molecule {
    // C3 (methyl), C2, C1 (carboxy), with the oxygens on C1 and the nitrogen on C2.
    let (c1x, c1y) = zz(2);
    let mut atoms = vec![
        ("C", zz(0).0, zz(0).1),
        ("C", zz(1).0, zz(1).1),
        ("C", c1x, c1y),
    ];
    atoms.extend([
        ("O", c1x, c1y - 1.0),
        ("O", c1x + 0.866, c1y + 0.5),
        ("N", zz(1).0, 1.5),
    ]);
    draw(
        &atoms,
        &[
            (0, 1, '-'),
            (1, 2, '-'),
            (2, 3, '='),
            (2, 4, '-'),
            (1, 5, kind),
        ],
    )
}

#[test]
fn alanine_skeleton() {
    assert_eq!(name(&alanine('h')), "(2S)-2-aminopropanoic acid"); // L-alanine
    assert_eq!(name(&alanine('w')), "(2R)-2-aminopropanoic acid"); // D-alanine
}

/// An ester's descriptors go before its acid part, after the alkyl group: ethyl
/// lactate drawn like L-alanine, the hydroxy group hashed (O > CO2Et > CH3 > H): S.
#[test]
fn ester() {
    let (c1x, c1y) = zz(2);
    let mol = draw(
        &[
            ("C", zz(0).0, zz(0).1),
            ("C", zz(1).0, zz(1).1),
            ("C", c1x, c1y),
            ("O", c1x, c1y - 1.0),
            ("O", c1x + 0.866, c1y + 0.5),
            ("O", zz(1).0, 1.5),
            ("C", c1x + 1.732, c1y),
            ("C", c1x + 2.598, c1y + 0.5),
        ],
        &[
            (0, 1, '-'),
            (1, 2, '-'),
            (2, 3, '='),
            (2, 4, '-'),
            (1, 5, 'h'),
            (4, 6, '-'),
            (6, 7, '-'),
        ],
    );
    assert_eq!(name(&mol), "ethyl (2S)-2-hydroxypropanoate");
}

/// But-2-ene and pent-2-ene: a zigzag is trans (E); the cis isomer turns the chain back.
#[test]
fn double_bond_geometry() {
    let trans = |n| {
        let (a, b) = chain(n, &[1]);
        draw(&a, &b)
    };
    // The cis isomers: C4 below C3, C5 continuing the zigzag from there.
    let cis = |n: usize| {
        let mut atoms = vec![
            ("C", zz(0).0, zz(0).1),
            ("C", zz(1).0, zz(1).1),
            ("C", zz(2).0, zz(2).1),
        ];
        atoms.push(("C", zz(2).0, zz(2).1 - 1.0));
        let mut bonds = vec![(0, 1, '-'), (1, 2, '='), (2, 3, '-')];
        if n == 5 {
            atoms.push(("C", zz(3).0, zz(2).1 - 1.5));
            bonds.push((3, 4, '-'));
        }
        draw(&atoms, &bonds)
    };
    assert_eq!(name(&trans(4)), "(2E)-but-2-ene");
    assert_eq!(name(&cis(4)), "(2Z)-but-2-ene");
    assert_eq!(name(&trans(5)), "(2E)-pent-2-ene"); // Wikipedia: "(E)-pent-2-ene"
    assert_eq!(name(&cis(5)), "(2Z)-pent-2-ene"); // Wikipedia: "(Z)-pent-2-ene"
}

/// E/Z and R/S together, in ascending order of their locants (P-93): C3 has O >
/// C4 (a vinylic carbon: C, C, H with its duplicate) > C2 > H; the hydroxy group on a solid
/// wedge, below the chain, gives O → C4 → C2 counterclockwise, S.
#[test]
fn descriptors_in_order_of_locants() {
    let mol = chain_with(6, &[3], &[("O", zz(2).0, -1.0, 2, 'w')]);
    assert_eq!(name(&mol), "(3S,4E)-hex-4-en-3-ol");
    assert_eq!(name(&mirrored(mol)), "(3R,4E)-hex-4-en-3-ol");
}

/// Duplicate atoms (P-92.1): the vinyl group's double bond counts its carbon twice, so
/// –CH=CH2 (C, C, H) outranks –CH2CH3 (C, H, H). Without the duplicate the ethyl group would
/// win and the descriptor would be the opposite one. Vinyl (upper left) → ethyl (upper right) →
/// methyl (on a wedge, below): clockwise, R.
#[test]
fn duplicate_atoms() {
    let mol = chain_with(5, &[0], &[("C", zz(2).0, -1.0, 2, 'w')]);
    assert_eq!(name(&mol), "(3R)-3-methylpent-1-ene");
}

/// Vinyl against isopropyl: both (C, C, H) at the first sphere; at the next, the vinyl group's
/// real CH2 (C, H, H — the duplicate of C2) beats the isopropyl group's methyls (H, H, H), the
/// branches explored in order of their rank (P-92.1). Vinyl > isopropyl > methyl > H.
#[test]
fn duplicate_atoms_one_sphere_further() {
    let mol = chain_with(
        5,
        &[0],
        &[("C", zz(2).0, -1.0, 2, 'w'), ("C", zz(3).0, 1.5, 3, '-')],
    );
    assert_eq!(name(&mol), "(3R)-3,4-dimethylpent-1-ene");
}

/// A comparison decided only at the fourth sphere: from C4 of 4-methyloctane, butyl and propyl
/// are alike (C; C, H, H; C, H, H) until the propyl group ends: butyl > propyl. Butyl (lower
/// right) → propyl (lower left) → methyl (on a wedge, above): clockwise, R.
#[test]
fn deep_comparison() {
    let mol = chain_with(8, &[], &[("C", zz(3).0, 1.5, 3, 'w')]);
    assert_eq!(name(&mol), "(4R)-4-methyloctane");
    assert_eq!(name(&mirrored(mol)), "(4S)-4-methyloctane");
}

/// Tartaric acid: C2 has O > C1 (O, O, O) > C3 (O, C, H) > H. Both hydroxy groups hashed:
/// (2R,3R), the natural L-(+)-tartaric acid. The meso acid is (2R,3S), not (2S,3R): the two
/// numberings tie on everything else and the lower locant goes to R (P-14.4 (j)).
#[test]
fn tartaric_acid() {
    let tartaric = |k2: char, k3: char| {
        draw(
            &[
                ("C", zz(0).0, zz(0).1),
                ("C", zz(1).0, zz(1).1),
                ("C", zz(2).0, zz(2).1),
                ("C", zz(3).0, zz(3).1),
                ("O", zz(0).0, -1.0),
                ("O", -0.866, 0.5),
                ("O", zz(1).0, 1.5),
                ("O", zz(2).0, -1.0),
                ("O", zz(3).0, 1.5),
                ("O", zz(4).0, zz(4).1),
            ],
            &[
                (0, 1, '-'),
                (1, 2, '-'),
                (2, 3, '-'),
                (0, 4, '='),
                (0, 5, '-'),
                (1, 6, k2),
                (2, 7, k3),
                (3, 8, '='),
                (3, 9, '-'),
            ],
        )
    };
    assert_eq!(
        name(&tartaric('h', 'h')),
        "(2R,3R)-2,3-dihydroxybutanedioic acid"
    );
    assert_eq!(
        name(&tartaric('w', 'w')),
        "(2S,3S)-2,3-dihydroxybutanedioic acid"
    );
    assert_eq!(
        name(&tartaric('w', 'h')),
        "(2R,3S)-2,3-dihydroxybutanedioic acid"
    );
    assert_eq!(
        name(&tartaric('h', 'w')),
        "(2R,3S)-2,3-dihydroxybutanedioic acid"
    );
}

/// Penicillin G, drawn the usual way: the β-lactam on the left, the thiazolidine on the right,
/// sharing the bond C5–N1 (C5 above). C2 (N1 > C3 (S, C, C) > COOH (O, O, O) > H) with the
/// carboxy group hashed: S. C5 (S > N1 > C6 > H) with its hydrogen drawn and hashed: R. C6
/// (N > C5 (S, N, H) > C7 (O, O, N) > H) with the amide nitrogen on a solid wedge: R. The two
/// hydrogens on C5 and C6 are both behind: cis, as in the natural compound.
fn penicillin_g(c2: char, c5: char, c6: char) -> Molecule {
    let atoms = [
        ("N", 0.0, -0.5),     // 0: N1
        ("C", 0.951, -0.809), // 1: C2
        ("C", 1.539, 0.0),    // 2: C3
        ("S", 0.951, 0.809),  // 3: S4
        ("C", 0.0, 0.5),      // 4: C5
        ("C", -1.0, 0.5),     // 5: C6
        ("C", -1.0, -0.5),    // 6: C7
        ("O", -1.7, -1.2),    // 7: O on C7
        ("C", 2.3, 0.5),      // 8: methyl on C3
        ("C", 2.3, -0.5),     // 9: methyl on C3
        ("C", 1.26, -1.76),   // 10: carboxy C on C2
        ("O", 2.2, -2.1),     // 11: =O
        ("O", 0.6, -2.5),     // 12: OH
        ("H", 0.0, 1.5),      // 13: H on C5
        ("N", -1.5, 1.37),    // 14: amide N on C6
        ("C", -2.5, 1.37),    // 15: amide C=O
        ("O", -3.0, 0.5),     // 16: =O
        ("C", -3.0, 2.24),    // 17: CH2
        ("C", -4.0, 2.24),    // 18: phenyl C1
        ("C", -4.5, 3.11),    // 19
        ("C", -5.5, 3.11),    // 20
        ("C", -6.0, 2.24),    // 21
        ("C", -5.5, 1.37),    // 22
        ("C", -4.5, 1.37),    // 23
    ];
    let bonds = [
        (0, 1, '-'),
        (1, 2, '-'),
        (2, 3, '-'),
        (4, 3, '-'),
        (4, 0, '-'),
        (4, 5, '-'),
        (5, 6, '-'),
        (6, 0, '-'),
        (6, 7, '='),
        (2, 8, '-'),
        (2, 9, '-'),
        (1, 10, c2),
        (10, 11, '='),
        (10, 12, '-'),
        (4, 13, c5),
        (5, 14, c6),
        (14, 15, '-'),
        (15, 16, '='),
        (15, 17, '-'),
        (17, 18, '-'),
        (18, 19, '='),
        (19, 20, '-'),
        (20, 21, '='),
        (21, 22, '-'),
        (22, 23, '='),
        (23, 18, '-'),
    ];
    draw(&atoms, &bonds)
}

/// The full name of penicillin G: OPE-22 settled the constitution (the acylamino prefix is the
/// amide's "amido" name, P-66.1.1.4.3, not Wikipedia's "[(phenylacetyl)amino]").
#[test]
fn penicillin_g_full_name() {
    let constitution = "3,3-dimethyl-7-oxo-6-(2-phenylacetamido)-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid";
    let natural = penicillin_g('h', 'h', 'w');
    assert_eq!(name(&natural), format!("(2S,5R,6R)-{constitution}"));
    assert_eq!(
        name(&mirrored(natural)),
        format!("(2R,5S,6S)-{constitution}")
    );
    // Only C6 inverted (the 6-epimer): the hydrogens trans.
    assert_eq!(
        name(&penicillin_g('h', 'h', 'h')),
        format!("(2S,5R,6S)-{constitution}")
    );
}

/// A drawing that does not fix a configuration gets no descriptor, and a note says why.
#[test]
fn undetermined_configurations_get_no_descriptor() {
    let cases = [
        // No wedge at the centre.
        (butan_2_x("O", '-'), "butan-2-ol", "C2"),
        // A wavy bond: the configuration is left open.
        (butan_2_x("O", '~'), "butan-2-ol", "wavy"),
        // A bold or plain hashed bond says nothing definite about a tetrahedral centre.
        (butan_2_x("O", 'b'), "butan-2-ol", "C2"),
        (butan_2_x("O", 'H'), "butan-2-ol", "C2"),
        // The wedge's narrow end is at the oxygen, so it describes the oxygen, not C2.
        (
            draw(
                &[
                    ("C", 0.0, 0.0),
                    ("C", 0.866, 0.5),
                    ("C", 1.732, 0.0),
                    ("C", 2.598, 0.5),
                    ("O", 0.866, 1.5),
                ],
                &[(0, 1, '-'), (1, 2, '-'), (2, 3, '-'), (4, 1, 'w')],
            ),
            "butan-2-ol",
            "C2",
        ),
        // But-2-ene drawn straight: no side for the methyl groups.
        (
            draw(
                &[
                    ("C", 0.0, 0.0),
                    ("C", 1.0, 0.0),
                    ("C", 2.0, 0.0),
                    ("C", 3.0, 0.0),
                ],
                &[(0, 1, '-'), (1, 2, '='), (2, 3, '-')],
            ),
            "but-2-ene",
            "C2",
        ),
        // A wavy bond on a double bond's carbon: either isomer.
        (
            draw(
                &[
                    ("C", 0.0, 0.0),
                    ("C", 0.866, 0.5),
                    ("C", 1.732, 0.0),
                    ("C", 2.598, 0.5),
                ],
                &[(0, 1, '~'), (1, 2, '='), (2, 3, '-')],
            ),
            "but-2-ene",
            "wavy",
        ),
    ];
    for (mol, expected, why) in cases {
        let (n, notes) = noted(&mol);
        assert_eq!(n, expected);
        assert!(
            notes.iter().any(|t| t.contains(why)),
            "{expected}: notes {notes:?} lack {why:?}"
        );
    }
}

/// An end of a double bond whose two ligands are alike but for their own configuration: the
/// two 1-hydroxyethyl groups on C3 of 3-ethylidenepentane-2,4-diol, one hydroxy group on a
/// solid wedge, the other hashed. Rule 1a ties them; Rules 3–5 are not implemented, so the
/// double bond gets no descriptor, with a note rather than silently.
#[test]
fn double_bond_end_with_ligands_alike_but_for_configuration() {
    let mol = draw(
        &[
            ("C", zz(0).0, zz(0).1),
            ("C", zz(1).0, zz(1).1),
            ("C", zz(2).0, zz(2).1),
            ("C", zz(3).0, zz(3).1),
            ("C", zz(4).0, zz(4).1),
            ("O", zz(1).0, 1.5),
            ("O", zz(3).0, 1.5),
            ("C", zz(2).0, -1.0),
            ("C", zz(3).0, -1.5),
        ],
        &[
            (0, 1, '-'),
            (1, 2, '-'),
            (2, 3, '-'),
            (3, 4, '-'),
            (1, 5, 'w'),
            (3, 6, 'h'),
            (2, 7, '='),
            (7, 8, '-'),
        ],
    );
    let (_, notes) = noted(&mol);
    assert!(
        notes
            .iter()
            .any(|t| t.contains("double bond") && t.contains("own configuration")),
        "{notes:?}"
    );
}

/// Where there is no stereogenic unit there is nothing to describe, and nothing to note: two
/// alike ligands on the centre or on an end of the double bond, a double bond in a ring of fewer
/// than eight members (P-93), a terminal double bond.
#[test]
fn no_stereogenic_unit() {
    let pentan_3_ol = chain_with(5, &[], &[("O", zz(2).0, -1.0, 2, 'w')]);
    let methylbutene = chain_with(4, &[1], &[("C", zz(1).0, 1.5, 1, '-')]);
    let butene = {
        let (a, b) = chain(4, &[0]);
        draw(&a, &b)
    };
    let hexagon: Vec<(&str, f32, f32)> = (0..6)
        .map(|k| {
            let t = std::f32::consts::PI / 3.0 * k as f32;
            ("C", t.cos(), t.sin())
        })
        .collect();
    let cyclohexene = draw(
        &hexagon,
        &(0..6)
            .map(|k| (k, (k + 1) % 6, if k == 0 { '=' } else { '-' }))
            .collect::<Vec<_>>(),
    );
    for (mol, expected) in [
        (pentan_3_ol, "pentan-3-ol"),
        (methylbutene, "2-methylbut-2-ene"),
        (butene, "but-1-ene"),
        (cyclohexene, "cyclohexene"),
    ] {
        assert_eq!(noted(&mol), (expected.to_string(), vec![]));
    }
}

/// A regular `n`-gon of carbons (atoms 0..n), radius `r`, single bonds except the `doubles`
/// (index of the first atom of each).
fn polygon(n: usize, r: f32, doubles: &[usize]) -> (Atoms, Bonds) {
    let atoms = (0..n)
        .map(|k| {
            let t = std::f32::consts::TAU / n as f32 * k as f32;
            ("C", r * t.cos(), r * t.sin())
        })
        .collect();
    let bonds = (0..n)
        .map(|k| (k, (k + 1) % n, if doubles.contains(&k) { '=' } else { '-' }))
        .collect();
    (atoms, bonds)
}

/// In a ring of eight or more members a double bond is stereogenic (P-93.5.1.4.1). A regular
/// polygon draws it cis. The name "cyclooctene" cites no locant (P-31.1.3.1: a lone locant 1
/// is omitted), so neither does its descriptor (P-93.4.1.1): "(E)-cyclooctene (PIN)",
/// "(Z)-cyclooctene" (P-91.2.2). Wikipedia: "(Z)-cyclooctene".
#[test]
fn double_bond_in_a_large_ring() {
    let (a, b) = polygon(8, 1.3, &[0]);
    assert_eq!(name(&draw(&a, &b)), "(Z)-cyclooctene");
    let (a, b) = polygon(10, 1.6, &[0]);
    assert_eq!(name(&draw(&a, &b)), "(Z)-cyclodecene");
}

/// An eight-membered ring needs its descriptor for one double bond, but not for "the
/// corresponding di-, tri-, or tetraene, i.e. 'cycloocta-1,3,5,7-tetraene'" (P-91.2.2): their
/// Z double bonds get none, and no note.
#[test]
fn cis_double_bonds_of_an_eight_membered_polyene() {
    for (doubles, expected) in [
        (&[0, 2, 4, 6][..], "cycloocta-1,3,5,7-tetraene"),
        (&[1, 3, 5, 7][..], "cycloocta-1,3,5,7-tetraene"),
        (&[0, 2][..], "cycloocta-1,3-diene"),
        (&[0, 4][..], "cycloocta-1,5-diene"),
    ] {
        let (a, b) = polygon(8, 1.3, doubles);
        assert_eq!(noted(&draw(&a, &b)), (expected.to_string(), vec![]));
    }
}

/// The trans isomer: cyclododecene with C12 (atom 11) moved across the line through the double
/// bond C1=C2, so that C12 and C3 are on opposite sides of it: E.
#[test]
fn trans_double_bond_in_a_large_ring() {
    let (mut a, b) = polygon(12, 1.9, &[0]);
    let (p, q) = ((a[0].1, a[0].2), (a[1].1, a[1].2));
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let (x, y) = (a[11].1 - p.0, a[11].2 - p.1);
    let t = (x * dx + y * dy) / (dx * dx + dy * dy);
    let (fx, fy) = (p.0 + t * dx, p.1 + t * dy);
    a[11] = ("C", 2.0 * fx - a[11].1, 2.0 * fy - a[11].2);
    assert_eq!(name(&draw(&a, &b)), "(E)-cyclododecene");
}

/// Where the name cites the locants of the ring, the descriptors carry them: the regular decagon
/// draws both double bonds of cyclodeca-1,3-diene cis.
#[test]
fn ring_descriptors_with_locants() {
    let (a, b) = polygon(10, 1.6, &[0, 2]);
    assert_eq!(name(&draw(&a, &b)), "(1Z,3Z)-cyclodeca-1,3-diene");
}

/// Cyclonon-4-en-1-ol, C1 (atom 0) on the right of a regular nonagon, the hydroxy group on a
/// solid wedge pointing outward (right). C1 has O > C2 (toward the double bond: the third
/// sphere holds C4 with its duplicate, (C, C, H), against C7's (C, H, H)) > C9 > H; O (right) →
/// C2 (upper left) → C9 (lower left) counterclockwise with H behind: S. The double bond is cis.
#[test]
fn ring_centre_and_double_bond() {
    let (mut a, mut b) = polygon(9, 1.5, &[3]);
    a.push(("O", 2.5, 0.0));
    b.push((0, 9, 'w'));
    let mol = draw(&a, &b);
    assert_eq!(name(&mol), "(1S,4Z)-cyclonon-4-en-1-ol");
    assert_eq!(name(&mirrored(mol)), "(1R,4Z)-cyclonon-4-en-1-ol");
}

/// A one-carbon parent cites no locant, so neither does its descriptor: "(R)-bromo(chloro)
/// (fluoro)methane (PIN)" (P-91.3, P-93.4.1.1). Br (on a solid wedge, up) > Cl (lower left) > F
/// (lower right) > H (implicit, behind): counterclockwise, S.
#[test]
fn one_carbon_parent() {
    let methane = |x: &'static str, y: &'static str, z: &'static str| {
        draw(
            &[
                ("C", 0.0, 0.0),
                (x, 0.0, 1.0),
                (y, -0.866, -0.5),
                (z, 0.866, -0.5),
            ],
            &[(0, 1, 'w'), (0, 2, '-'), (0, 3, '-')],
        )
    };
    let mol = methane("Br", "Cl", "F");
    assert_eq!(name(&mol), "(S)-bromo(chloro)(fluoro)methane");
    assert_eq!(name(&mirrored(mol)), "(R)-bromo(chloro)(fluoro)methane");
    // Cl > F > O > H, in the same places: S.
    let mol = methane("Cl", "F", "O");
    assert_eq!(name(&mol), "(S)-chloro(fluoro)methanol");
    assert_eq!(name(&mirrored(mol)), "(R)-chloro(fluoro)methanol");
}

/// 1,2-Dichloroethene. The name of ethene cites no locant for its double bond ("ethene", not
/// "eth-1-ene": the locants of a two-carbon parent are left out); the "1,2" locate the chlorine
/// atoms. The descriptor belongs to the stereogenic unit, the double bond, which the name
/// locates by no locant, so it is cited alone (P-93.4.1.1: "When no locant is required in the
/// name …, no locant is necessary before the stereodescriptor"; the examples
/// "(E)-2-…ethen-1-yl" of P-93.6 likewise cite none, the locant 1 of the double bond being
/// implied): "(E)-1,2-dichloroethene". A zigzag Cl–CH=CH–Cl is trans; with the second chlorine
/// below C2, cis.
#[test]
fn ethene_double_bond() {
    let ethene = |cl2: (f32, f32)| {
        draw(
            &[
                ("Cl", zz(0).0, zz(0).1),
                ("C", zz(1).0, zz(1).1),
                ("C", zz(2).0, zz(2).1),
                ("Cl", cl2.0, cl2.1),
            ],
            &[(0, 1, '-'), (1, 2, '='), (2, 3, '-')],
        )
    };
    assert_eq!(name(&ethene(zz(3))), "(E)-1,2-dichloroethene");
    assert_eq!(name(&ethene((zz(2).0, -1.0))), "(Z)-1,2-dichloroethene");
}

/// 1-Phenylethan-1-ol cites its locant: O (upper left, on a solid wedge) > phenyl (right) >
/// methyl (lower left) > H (behind): clockwise, R. The benzene ring gives the same ranking in
/// either Kekulé structure (P-92.1.4.4: for mancude hydrocarbons it is immaterial).
#[test]
fn phenylethanol_in_either_kekule_structure() {
    for first in [0, 1] {
        let (mut a, mut b) = polygon(6, 1.0, &[first, first + 2, first + 4]);
        // The ring to the right of the centre, its atom 3 (at the left) bonded to it.
        for atom in &mut a {
            atom.1 += 2.0;
        }
        a.extend([("C", 0.0, 0.0), ("O", -0.5, 0.866), ("C", -0.5, -0.866)]);
        b.extend([(6, 3, '-'), (6, 7, 'w'), (6, 8, '-')]);
        let mol = draw(&a, &b);
        assert_eq!(name(&mol), "(1R)-1-phenylethan-1-ol", "Kekulé {first}");
        assert_eq!(name(&mirrored(mol)), "(1S)-1-phenylethan-1-ol");
    }
}

/// Pyridin-2-yl–C*H(OH)–C(CH3)(NMe2)–CH2OH, the hydroxy group on C* on a solid wedge (up), the
/// pyridyl group lower left, C2 lower right. In a mancude heterocycle each duplicate atom has
/// the mean of the atomic numbers it would have for every position of the double bonds
/// (P-92.1.4.4): C2' of pyridine is doubly bonded either to N1' or to C3', so its duplicate
/// counts 6½, and the pyridyl group (N, 6½, C) outranks C2 (N, C, C) whichever Kekulé structure
/// is drawn (drawn C2'=C3', the duplicate alone would make it (N, C, C), and C2 would win at
/// the next sphere). O → pyridyl → C2 counterclockwise with H behind: S. (C2 is a stereocentre
/// too, drawn without a wedge.)
#[test]
fn mancude_heterocycle_in_either_kekule_structure() {
    let mol = |c2_n: bool| {
        // The ring: C2' (atom 0) at the centre's lower left, then N1', C6', C5', C4', C3'
        // around a centre further out.
        let (cx, cy) = (-1.732, -1.0);
        let mut atoms: Atoms = (0..6)
            .map(|k| {
                let t = (30.0 + 60.0 * k as f32).to_radians();
                (if k == 1 { "N" } else { "C" }, cx + t.cos(), cy + t.sin())
            })
            .collect();
        let doubles: [usize; 3] = if c2_n { [0, 2, 4] } else { [1, 3, 5] };
        let mut bonds: Bonds = (0..6)
            .map(|k| (k, (k + 1) % 6, if doubles.contains(&k) { '=' } else { '-' }))
            .collect();
        atoms.extend([
            ("C", 0.0, 0.0),     // 6: C1
            ("O", 0.0, 1.0),     // 7
            ("C", 0.866, -0.5),  // 8: C2
            ("C", 1.366, 0.366), // 9: C3
            ("O", 2.366, 0.366), // 10
            ("C", 1.866, -0.5),  // 11: the methyl group
            ("N", 0.866, -1.5),  // 12
            ("C", 0.0, -2.0),    // 13
            ("C", 1.732, -2.0),  // 14
        ]);
        bonds.extend([
            (6, 0, '-'),
            (6, 7, 'w'),
            (6, 8, '-'),
            (8, 9, '-'),
            (9, 10, '-'),
            (8, 11, '-'),
            (8, 12, '-'),
            (12, 13, '-'),
            (12, 14, '-'),
        ]);
        draw(&atoms, &bonds)
    };
    let expected = "(1S)-2-(dimethylamino)-2-methyl-1-(pyridin-2-yl)propane-1,3-diol";
    for c2_n in [true, false] {
        assert_eq!(name(&mol(c2_n)), expected, "C2'=N1' drawn: {c2_n}");
        assert_eq!(
            name(&mirrored(mol(c2_n))),
            expected.replace("(1S)", "(1R)"),
            "C2'=N1' drawn: {c2_n}"
        );
    }
}

/// Descriptors are given for the parent chain or ring only; a stereocentre in a substituent
/// prefix ("[(2R)-butan-2-yl]") is left out, with a note.
#[test]
fn stereocentre_in_a_substituent() {
    // Butane drawn as a zigzag, a benzene ring above C2, bonded to it by a solid wedge.
    let ring: Vec<(&str, f32, f32)> = (0..6)
        .map(|k| {
            let t = std::f32::consts::PI / 3.0 * k as f32 - std::f32::consts::PI / 2.0;
            ("C", zz(1).0 + t.cos(), 2.5 + t.sin())
        })
        .collect();
    let (mut atoms, mut bonds) = chain(4, &[]);
    let base = atoms.len();
    atoms.extend(ring);
    for k in 0..6 {
        bonds.push((
            base + k,
            base + (k + 1) % 6,
            if k % 2 == 0 { '=' } else { '-' },
        ));
    }
    bonds.push((1, base, 'w'));
    let mol = draw(&atoms, &bonds);
    let (n, notes) = noted(&mol);
    assert_eq!(n, "(butan-2-yl)benzene");
    assert!(notes.iter().any(|t| t.contains("substituent")), "{notes:?}");
}

/// The CIP digraph is grown without recursion: a centre at the end of a long chain is ranked
/// without exhausting the stack, its ligand toward the chain explored to the far end.
#[test]
fn long_chain() {
    let mol = chain_with(1500, &[], &[("Br", 0.866, 1.5, 1, 'w')]);
    let n = name(&mol);
    assert!(n.starts_with("(2R)-2-bromo"), "{n}");
}
