//! The periodic table as symbols, for the formats that store an element as its atomic number
//! (CDX, CDXML). One table so the writers and the CDX reader always agree.

const SYMBOLS: [&str; 118] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
];

/// Atomic number of an element symbol (`None` for anything else: a custom label, "D", "Ph").
pub fn atomic_number(symbol: &str) -> Option<u16> {
    SYMBOLS.iter().position(|s| *s == symbol).map(|i| i as u16 + 1)
}

/// Symbol of an atomic number (1–118).
pub fn symbol(z: u16) -> Option<&'static str> {
    SYMBOLS.get((z as usize).checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_round_trips() {
        assert_eq!(atomic_number("H"), Some(1));
        assert_eq!(atomic_number("C"), Some(6));
        assert_eq!(atomic_number("Br"), Some(35));
        assert_eq!(atomic_number("Pt"), Some(78));
        assert_eq!(atomic_number("Og"), Some(118));
        assert_eq!(atomic_number("Ph"), None);
        assert_eq!(symbol(0), None);
        assert_eq!(symbol(119), None);
        for z in 1..=118 {
            assert_eq!(atomic_number(symbol(z).unwrap()), Some(z));
        }
    }
}
