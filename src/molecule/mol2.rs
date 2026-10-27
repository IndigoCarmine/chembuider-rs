use super::{Atom, Bond, BondOrder, Molecule};

pub fn to_mol2_string(mol: &Molecule) -> String {
    let mut s = String::new();

    s.push_str("@<TRIPOS>MOLECULE\n");
    s.push_str(&format!("{}\n", mol.name));
    s.push_str(&format!("{} {} 0 0 0\n", mol.atoms.len(), mol.bonds.len()));
    s.push_str("SMALL\n");
    // Formal charges are carried by the SYBYL atom type (N.4, N.pl3, O.co2; see
    // `tripos_atom_type`), not by the charge column. That column holds *partial* charges, and
    // RDKit and Open Babel read it as partial charges, never as formal ones; writing integer
    // formal charges there under USER_CHARGES would misstate the partial charges and still not
    // restore the formal charge. So the header honestly says there are no partial charges.
    s.push_str("NO_CHARGES\n");
    s.push('\n');

    s.push_str("@<TRIPOS>ATOM\n");
    for (i, atom) in mol.atoms.iter().enumerate() {
        let atom_name = format!("{}{}", atom.element, i + 1);
        let atom_type = tripos_atom_type(mol, atom);
        s.push_str(&format!(
            "{:>7} {:<8} {:>10.4} {:>10.4} {:>10.4} {:<8} 1 MOLE   0.0000\n",
            i + 1,
            atom_name,
            atom.pos[0],
            atom.pos[1],
            0.0f32,
            atom_type,
        ));
    }
    s.push('\n');

    s.push_str("@<TRIPOS>BOND\n");
    let atom_index: std::collections::HashMap<u32, usize> = mol
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i + 1))
        .collect();

    for (i, bond) in mol.bonds.iter().enumerate() {
        let b_idx = atom_index.get(&bond.begin).copied().unwrap_or(0);
        let e_idx = atom_index.get(&bond.end).copied().unwrap_or(0);
        s.push_str(&format!(
            "{:>6} {:>5} {:>5} {}\n",
            i + 1,
            b_idx,
            e_idx,
            bond.order.mol2_type(),
        ));
    }

    s
}

/// SYBYL (Tripos) atom type of `atom`, chosen from its element, formal charge and the orders of
/// the bonds actually drawn on it. A type that cannot be told apart with confidence falls back to
/// the generic `.3` type rather than guessing.
///
/// | element | condition (first match wins)                                            | type    |
/// |---------|-------------------------------------------------------------------------|---------|
/// | C       | a triple bond, or two or more double bonds (alkyne, nitrile, allene, CO2) | `C.1`   |
/// | C       | exactly one double bond                                                 | `C.2`   |
/// | C       | otherwise (incl. carbocations / carbanions)                             | `C.3`   |
/// | N       | a triple bond, or two or more double bonds                              | `N.1`   |
/// | N       | one double bond, charge 0 (imine, azo)                                  | `N.2`   |
/// | N       | one double bond, charge +1 (iminium, nitro, N-oxide)                    | `N.pl3` |
/// | N       | single bonds only, charge +1 (ammonium)                                 | `N.4`   |
/// | N       | single bonds only, charge 0, bonded to a C that has a C=O (amide)       | `N.am`  |
/// | N       | otherwise                                                               | `N.3`   |
/// | O       | terminal O of a carboxylate C(=O)O⁻ (both the =O and the O⁻)            | `O.co2` |
/// | O       | a double bond and no triple bond                                        | `O.2`   |
/// | O       | otherwise (incl. alkoxide / nitro O⁻)                                   | `O.3`   |
/// | S       | exactly one =O and no other double bond (sulfoxide)                     | `S.O`   |
/// | S       | exactly two =O and no other double bond (sulfone, sulfonamide, sulfate) | `S.O2`  |
/// | S       | exactly one double bond, not to O (thione)                              | `S.2`   |
/// | S       | otherwise                                                               | `S.3`   |
/// | P       | always                                                                  | `P.3`   |
/// | H, F, Cl, Br, I | always                                                          | element |
/// | other   | labels, metals, `D`, …                                                  | `Du`    |
///
/// Deliberately never written:
/// - `C.ar` / `N.ar` (and `ar` bonds): the editor stores rings in Kekulé form with no aromatic
///   flag (see `smiles.rs`), so an aromatic type would be a guess. Kekulé rings are typed by
///   their bond orders (`C.2`, `N.2`), and readers perceive aromaticity from them.
/// - `C.cat` (guanidinium carbon only) and phosphate `O.co2`: Tripos defines them over a
///   delocalised group, and how readers combine them with explicitly drawn single/double bonds
///   is not certain, so those atoms get the ordinary bond-order type.
/// - `N.pl3` for neutral conjugated nitrogens (aniline, enamine, pyrrole): telling them apart
///   needs conjugation perception, so they stay `N.3`.
///
/// Formal charge is expressed only through these types (`N.4`, `N.pl3`, `O.co2`); a charge with
/// no dedicated type (e.g. an alkoxide `O.3`) has no other representation in the file.
fn tripos_atom_type(mol: &Molecule, atom: &Atom) -> &'static str {
    let bonds = mol.bonds_for_atom(atom.id);
    let count = |order: BondOrder| bonds.iter().filter(|b| b.order == order).count();
    let (doubles, triples) = (count(BondOrder::Double), count(BondOrder::Triple));
    let neighbour = |b: &Bond| partner(mol, b, atom.id);

    match atom.element.as_str() {
        "C" => {
            if triples > 0 || doubles >= 2 {
                "C.1"
            } else if doubles == 1 {
                "C.2"
            } else {
                "C.3"
            }
        }
        "N" => {
            if triples > 0 || doubles >= 2 {
                "N.1"
            } else if doubles == 1 {
                match atom.charge {
                    0 => "N.2",
                    1 => "N.pl3",
                    _ => "N.3",
                }
            } else if atom.charge == 1 {
                "N.4"
            } else if atom.charge == 0
                && bonds.iter().any(|b| {
                    neighbour(b).is_some_and(|c| c.element == "C" && has_double_bond_to_oxygen(mol, c))
                })
            {
                "N.am"
            } else {
                "N.3"
            }
        }
        "O" => {
            if bonds.len() == 1 && neighbour(bonds[0]).is_some_and(|c| is_carboxylate_carbon(mol, c)) {
                "O.co2"
            } else if doubles > 0 && triples == 0 {
                "O.2"
            } else {
                "O.3"
            }
        }
        "S" => {
            let to_oxygen = bonds
                .iter()
                .filter(|b| b.order == BondOrder::Double && neighbour(b).is_some_and(|o| o.element == "O"))
                .count();
            match (triples, to_oxygen, doubles - to_oxygen) {
                (0, 1, 0) => "S.O",
                (0, 2, 0) => "S.O2",
                (0, 0, 1) => "S.2",
                _ => "S.3",
            }
        }
        "H" => "H",
        "P" => "P.3",
        "F" => "F",
        "Cl" => "Cl",
        "Br" => "Br",
        "I" => "I",
        _ => "Du",
    }
}

/// The atom bonded to `atom_id` through `bond`, if it exists.
fn partner<'a>(mol: &'a Molecule, bond: &Bond, atom_id: u32) -> Option<&'a Atom> {
    mol.atom_by_id(if bond.begin == atom_id { bond.end } else { bond.begin })
}

/// Whether `atom` carries a double bond to an oxygen (the carbonyl of an amide).
fn has_double_bond_to_oxygen(mol: &Molecule, atom: &Atom) -> bool {
    mol.bonds_for_atom(atom.id).iter().any(|b| {
        b.order == BondOrder::Double && partner(mol, b, atom.id).is_some_and(|o| o.element == "O")
    })
}

/// Whether `atom` is the carbon of a carboxylate: its terminal oxygens are exactly one neutral
/// `=O` and one `-O⁻`. An acid's `-OH` (implicit H) is neutral, a drawn `-O-H` or an ester O is
/// not terminal, and a carbonate has three terminal oxygens, so none of those match.
fn is_carboxylate_carbon(mol: &Molecule, atom: &Atom) -> bool {
    if atom.element != "C" {
        return false;
    }
    let terminal_oxygens: Vec<(&BondOrder, i8)> = mol
        .bonds_for_atom(atom.id)
        .into_iter()
        .filter_map(|b| {
            let o = partner(mol, b, atom.id)?;
            (o.element == "O" && mol.bonds_for_atom(o.id).len() == 1).then_some((&b.order, o.charge))
        })
        .collect();
    let has = |order: BondOrder, charge: i8| terminal_oxygens.iter().any(|&(o, q)| *o == order && q == charge);
    terminal_oxygens.len() == 2 && has(BondOrder::Double, 0) && has(BondOrder::Single, -1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The SYBYL type column (6th field) of every `@<TRIPOS>ATOM` record, in atom order.
    /// This crate has no mol2 reader, so the tests read back the one field they check.
    fn atom_types(mol2: &str) -> Vec<String> {
        mol2.lines()
            .skip_while(|l| *l != "@<TRIPOS>ATOM")
            .skip(1)
            .take_while(|l| !l.is_empty())
            .map(|l| l.split_whitespace().nth(5).expect("atom record has a type column").to_string())
            .collect()
    }

    /// Build a molecule from `(element, charge)` atoms and `(i, j, order)` bonds by index,
    /// then return the written SYBYL types.
    fn types_of(atoms: &[(&str, i8)], bonds: &[(usize, usize, BondOrder)]) -> Vec<String> {
        let mut mol = Molecule::default();
        let ids: Vec<u32> = atoms
            .iter()
            .enumerate()
            .map(|(i, (el, q))| mol.add_atom(el.to_string(), [i as f32 * 1.5, 0.0], *q))
            .collect();
        for (i, j, order) in bonds {
            mol.add_bond(ids[*i], ids[*j], order.clone());
        }
        atom_types(&to_mol2_string(&mol))
    }

    use BondOrder::{Double as D, Single as S, Triple as T};

    #[test]
    fn methylammonium_nitrogen_is_n4() {
        assert_eq!(types_of(&[("C", 0), ("N", 1)], &[(0, 1, S)]), ["C.3", "N.4"]);
    }

    #[test]
    fn neutral_amine_stays_n3() {
        assert_eq!(types_of(&[("C", 0), ("N", 0)], &[(0, 1, S)]), ["C.3", "N.3"]);
    }

    #[test]
    fn carbonyl_carbon_and_oxygen_are_sp2() {
        // Acetone: CH3-C(=O)-CH3
        assert_eq!(
            types_of(&[("C", 0), ("C", 0), ("O", 0), ("C", 0)], &[(0, 1, S), (1, 2, D), (1, 3, S)]),
            ["C.3", "C.2", "O.2", "C.3"]
        );
    }

    #[test]
    fn nitrile_carbon_and_nitrogen_are_sp() {
        // Acetonitrile: CH3-C#N
        assert_eq!(
            types_of(&[("C", 0), ("C", 0), ("N", 0)], &[(0, 1, S), (1, 2, T)]),
            ["C.3", "C.1", "N.1"]
        );
    }

    #[test]
    fn cumulated_double_bonds_make_the_centre_sp() {
        // Allene CH2=C=CH2 and carbon dioxide O=C=O.
        assert_eq!(
            types_of(&[("C", 0), ("C", 0), ("C", 0)], &[(0, 1, D), (1, 2, D)]),
            ["C.2", "C.1", "C.2"]
        );
        assert_eq!(
            types_of(&[("O", 0), ("C", 0), ("O", 0)], &[(0, 1, D), (1, 2, D)]),
            ["O.2", "C.1", "O.2"]
        );
    }

    #[test]
    fn kekule_benzene_is_typed_c2_not_c_ar() {
        let mut mol = Molecule::default();
        let ids: Vec<u32> = (0..6)
            .map(|i| {
                let a = std::f32::consts::PI / 3.0 * i as f32;
                mol.add_atom("C".to_string(), [a.cos() * 1.5, a.sin() * 1.5], 0)
            })
            .collect();
        for i in 0..6 {
            let order = if i % 2 == 0 { D } else { S };
            mol.add_bond(ids[i], ids[(i + 1) % 6], order);
        }
        assert_eq!(atom_types(&to_mol2_string(&mol)), vec!["C.2"; 6]);
    }

    #[test]
    fn imine_and_iminium_nitrogens() {
        // CH2=NH (imine) -> N.2; CH2=N+H2 (iminium) -> N.pl3.
        assert_eq!(types_of(&[("C", 0), ("N", 0)], &[(0, 1, D)]), ["C.2", "N.2"]);
        assert_eq!(types_of(&[("C", 0), ("N", 1)], &[(0, 1, D)]), ["C.2", "N.pl3"]);
    }

    #[test]
    fn nitro_nitrogen_is_planar() {
        // CH3-N+(=O)O-
        assert_eq!(
            types_of(&[("C", 0), ("N", 1), ("O", 0), ("O", -1)], &[(0, 1, S), (1, 2, D), (1, 3, S)]),
            ["C.3", "N.pl3", "O.2", "O.3"]
        );
    }

    #[test]
    fn amide_nitrogen_is_n_am() {
        // Acetamide: CH3-C(=O)-NH2
        assert_eq!(
            types_of(&[("C", 0), ("C", 0), ("O", 0), ("N", 0)], &[(0, 1, S), (1, 2, D), (1, 3, S)]),
            ["C.3", "C.2", "O.2", "N.am"]
        );
    }

    #[test]
    fn carboxylate_oxygens_are_o_co2_but_acid_oxygens_are_not() {
        // Acetate CH3-C(=O)O-  vs  acetic acid CH3-C(=O)OH
        assert_eq!(
            types_of(&[("C", 0), ("C", 0), ("O", 0), ("O", -1)], &[(0, 1, S), (1, 2, D), (1, 3, S)]),
            ["C.3", "C.2", "O.co2", "O.co2"]
        );
        assert_eq!(
            types_of(&[("C", 0), ("C", 0), ("O", 0), ("O", 0)], &[(0, 1, S), (1, 2, D), (1, 3, S)]),
            ["C.3", "C.2", "O.2", "O.3"]
        );
    }

    #[test]
    fn sulfur_types_follow_its_double_bonds() {
        // Dimethyl sulfide, thioformaldehyde, DMSO, dimethyl sulfone.
        assert_eq!(
            types_of(&[("C", 0), ("S", 0), ("C", 0)], &[(0, 1, S), (1, 2, S)]),
            ["C.3", "S.3", "C.3"]
        );
        assert_eq!(types_of(&[("C", 0), ("S", 0)], &[(0, 1, D)]), ["C.2", "S.2"]);
        assert_eq!(
            types_of(&[("C", 0), ("S", 0), ("C", 0), ("O", 0)], &[(0, 1, S), (1, 2, S), (1, 3, D)]),
            ["C.3", "S.O", "C.3", "O.2"]
        );
        assert_eq!(
            types_of(
                &[("C", 0), ("S", 0), ("C", 0), ("O", 0), ("O", 0)],
                &[(0, 1, S), (1, 2, S), (1, 3, D), (1, 4, D)]
            ),
            ["C.3", "S.O2", "C.3", "O.2", "O.2"]
        );
    }

    #[test]
    fn untyped_elements_keep_their_previous_types() {
        assert_eq!(
            types_of(&[("C", 0), ("Cl", 0), ("P", 0), ("Ph", 0)], &[(0, 1, S), (0, 2, S), (0, 3, S)]),
            ["C.3", "Cl", "P.3", "Du"]
        );
    }
}
