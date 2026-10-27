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

/// Element symbols this editor reads as group labels instead: `Ac` is acetyl and `Ts` tosyl,
/// the fragments of those names in `assets/fragments/`. An atom labelled so is no actinium or
/// tennessine, so it is written with no atomic number.
pub const GROUP_LABELS: [&str; 2] = ["Ac", "Ts"];

/// The atomic number an atom's label stands for: its element's, or `None` for a label that is
/// no element — a custom label, "D", "Ph", or one of the [`GROUP_LABELS`].
pub fn element_of_label(label: &str) -> Option<u16> {
    if GROUP_LABELS.contains(&label) {
        return None;
    }
    atomic_number(label)
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

    #[test]
    fn group_labels_are_no_elements_but_the_symbols_still_are() {
        assert_eq!(element_of_label("Ac"), None);
        assert_eq!(element_of_label("Ts"), None);
        assert_eq!(atomic_number("Ac"), Some(89));
        assert_eq!(atomic_number("Ts"), Some(117));
        assert_eq!(element_of_label("N"), Some(7));
        assert_eq!(element_of_label("Ph"), None);
    }
}
