//! The four structures of the reference figure (phenol + MeI / K₂CO₃ → anisole).
//!
//! They are library code rather than example code because both `examples/reaction_scheme.rs`
//! and the crate's own tests need the exact same molecules — the tests pin their formulas,
//! weights and the whole stoichiometry table against hand-checked values.
//!
//! All bonds are [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH) long, like the
//! built-in fragments, and molecule-space *y* grows downward (as on screen).

use crate::molecule::{BondOrder, Molecule};
use crate::widget::DEFAULT_BOND_LENGTH as L;

/// sin 60° — the vertical step of a hexagon edge, and of a substituent leaving one.
const SIN60: f32 = 0.866_025_4;

/// Benzene ring (alternating single/double bonds) with atom 0 at the origin and the ring
/// body above it on screen. Flat top and bottom edges, vertices left and right — the
/// orientation ChemDraw uses. Ring atoms are `atoms[0..6]`, going clockwise on screen from
/// the bottom-left; `atoms[3]` is the **top-right** vertex, where substituents go.
pub fn benzene() -> Molecule {
    let mut m = Molecule::default();
    m.name = "benzene".into();
    let h = L * SIN60; // ring height step (√3/2 · bond)
    let pos = [
        [0.0, 0.0],
        [L, 0.0],
        [L * 1.5, -h],
        [L, -2.0 * h],
        [0.0, -2.0 * h],
        [-L * 0.5, -h],
    ];
    let ids: Vec<u32> = pos.iter().map(|&p| m.add_atom("C".into(), p, 0)).collect();
    for k in 0..6 {
        let order = if k % 2 == 0 { BondOrder::Single } else { BondOrder::Double };
        m.add_bond(ids[k], ids[(k + 1) % 6], order);
    }
    m
}

/// Phenol, C₆H₆O — a benzene ring with a hydroxyl leaving the top-right vertex up and to
/// the right, so the label reads `OH`.
pub fn phenol() -> Molecule {
    let mut m = benzene();
    m.name = "phenol".into();
    // Straight out of the ring at the top-right vertex: (+0.5, -0.866) · bond length.
    let ipso = m.atoms[3].id;
    let ipso_pos = m.atoms[3].pos;
    let o = m.add_atom(
        "O".into(),
        [ipso_pos[0] + L * 0.5, ipso_pos[1] - L * SIN60],
        0,
    );
    m.add_bond(ipso, o, BondOrder::Single);
    m
}

/// Anisole, C₇H₈O — phenol's methyl ether: the chain turns 120° at the oxygen and carries
/// on to the right, the methyl carbon staying unlabelled.
pub fn anisole() -> Molecule {
    let mut m = phenol();
    m.name = "anisole".into();
    let (o_id, o_pos) = {
        let o = m.atoms.last().expect("phenol has an oxygen");
        (o.id, o.pos)
    };
    let c = m.add_atom("C".into(), [o_pos[0] + L, o_pos[1]], 0);
    m.add_bond(o_id, c, BondOrder::Single);
    m
}

/// Iodomethane, CH₃I — one C–I bond; the methyl hydrogens are implicit.
pub fn methyl_iodide() -> Molecule {
    let mut m = Molecule::default();
    m.name = "MeI".into();
    let c = m.add_atom("C".into(), [0.0, 0.0], 0);
    let i = m.add_atom("I".into(), [L, 0.0], 0);
    m.add_bond(c, i, BondOrder::Single);
    m
}

/// Potassium carbonate, drawn covalently as K–O–C(=O)–O–K. Its Hill formula is `CK2O3`.
pub fn potassium_carbonate() -> Molecule {
    let mut m = Molecule::default();
    m.name = "K2CO3".into();
    let dx = L * SIN60;
    let dy = L * 0.5;
    let c = m.add_atom("C".into(), [0.0, 0.0], 0);
    let o_double = m.add_atom("O".into(), [0.0, -L], 0);
    let o_left = m.add_atom("O".into(), [-dx, dy], 0);
    let o_right = m.add_atom("O".into(), [dx, dy], 0);
    let k_left = m.add_atom("K".into(), [-2.0 * dx, 2.0 * dy], 0);
    let k_right = m.add_atom("K".into(), [2.0 * dx, 2.0 * dy], 0);
    m.add_bond(c, o_double, BondOrder::Double);
    m.add_bond(c, o_left, BondOrder::Single);
    m.add_bond(c, o_right, BondOrder::Single);
    m.add_bond(o_left, k_left, BondOrder::Single);
    m.add_bond(o_right, k_right, BondOrder::Single);
    m
}

/// The complete reference reaction: phenol (limiting, 1.00 g) + MeI (1.10 eq) + K₂CO₃
/// (3.00 eq) → anisole, 700.00 mg isolated. Scheme and analysis, ready to draw.
pub fn williamson_ether_synthesis() -> crate::reaction::Reaction {
    use crate::reaction::{Analysis, Arrow, Mass, Product, Reactant, Reaction, ReactionScheme};

    let scheme = ReactionScheme::new()
        .structure(phenol())
        .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
        .structure(anisole());

    let analysis = Analysis::new()
        .reactant(
            Reactant::new(phenol())
                .limiting()
                .equivalents(1.00)
                .sample_mass(Mass::grams(1.00)),
        )
        .reactant(Reactant::new(methyl_iodide()).equivalents(1.10))
        .reactant(Reactant::new(potassium_carbonate()).equivalents(3.00))
        .product(Product::new(anisole()).measured_mass(Mass::milligrams(700.00)));

    Reaction { scheme, analysis }
}
