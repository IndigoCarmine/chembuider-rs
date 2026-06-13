use super::Molecule;

pub fn to_mol2_string(mol: &Molecule) -> String {
    let mut s = String::new();

    s.push_str("@<TRIPOS>MOLECULE\n");
    s.push_str(&format!("{}\n", mol.name));
    s.push_str(&format!("{} {} 0 0 0\n", mol.atoms.len(), mol.bonds.len()));
    s.push_str("SMALL\n");
    s.push_str("NO_CHARGES\n");
    s.push('\n');

    s.push_str("@<TRIPOS>ATOM\n");
    for (i, atom) in mol.atoms.iter().enumerate() {
        let atom_name = format!("{}{}", atom.element, i + 1);
        let atom_type = tripos_atom_type(&atom.element);
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

fn tripos_atom_type(element: &str) -> &'static str {
    match element {
        "C" => "C.3",
        "N" => "N.3",
        "O" => "O.3",
        "S" => "S.3",
        "H" => "H",
        "P" => "P.3",
        "F" => "F",
        "Cl" => "Cl",
        "Br" => "Br",
        "I" => "I",
        _ => "Du",
    }
}
