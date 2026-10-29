//! Compositional names of inorganic compounds (IUPAC 2005, the Red Book): elements (IR-3.4.3,
//! IR-5.2), binary compounds (IR-5.2) and salts of the cations and anions of the tables
//! (IR-5.3, IR-5.4, IR-8.5), drawn as separate ions or with the metal bonded to the anion.
//!
//! The reference names are Wikipedia's (en) where it has the compound; where its name differs
//! from the Red Book's, the Red Book's is expected and Wikipedia's noted beside it:
//!
//! - "hydrogencarbonate", one word: the accepted abbreviated hydrogen names of IR-8.5 are "all
//!   in one word" (IR-8.4: "hydrogen" is joined with no space to the anion name), and the Red
//!   Book's own salt is "calcium bis(hydrogencarbonate)" (IR-5.4.2.1, Example 11). Wikipedia
//!   writes "sodium hydrogen carbonate", the 1990 spelling (IR-8.4 keeps the separate word only
//!   for salts of organic polyvalent acids: "potassium hydrogen phthalate").
//! - The proportions are stated by multiplying prefixes: "calcium dichloride", not Wikipedia's
//!   "calcium chloride". IR-5.2 lets them be left out "if there is no ambiguity about the
//!   stoichiometry" ("tricalcium diphosphide, or calcium phosphide"), but the full name is the
//!   one the Red Book gives first everywhere (IR-5.2, IR-5.4.2.1), it is never ambiguous —
//!   "calcium phosphate" is (Example 9 keeps "tricalcium bis(phosphate)"), and with two kinds of
//!   cation the prefixes cannot go ("magnesium potassium trichloride", Example 4) — and it is
//!   what the namer gives the salts of carboxylic acids ("calcium diacetate", "disodium
//!   butanedioate", P-65.6.2.1). The short names are other names (OPE-24).
//! - Oxoanion names are multiplied with "bis", "tris" and parentheses (IR-5.4.2.1:
//!   "tricalcium bis(phosphate)", "aluminium potassium bis(sulfate)"): "diphosphate",
//!   "disulfate" and "dinitrate" name other anions.

use super::{NameResult, iupac_name};
use crate::molecule::smiles::parser::parse;
use crate::molecule::{BondOrder, Molecule};

fn name_of(smiles: &str) -> NameResult {
    iupac_name(&parse(smiles).0)
}

fn check_names(cases: &[(&str, &str)]) {
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(smiles, expected)| match name_of(smiles) {
            NameResult::Ok(name) if name == *expected => None,
            other => Some(format!("{smiles}: expected {expected:?}, got {other:?}")),
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} names wrong:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

fn check_refused(cases: &[(&str, &str)]) {
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(smiles, reason)| match name_of(smiles) {
            NameResult::Unsupported(r) if r.contains(reason) => None,
            other => Some(format!(
                "{smiles}: expected a refusal with {reason:?}, got {other:?}"
            )),
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Elements (IR-3.4.3, IR-5.2): the multiplying prefix of the atoms in the molecule, then the
/// element name; one atom is the bare name.
#[test]
fn elements() {
    check_names(&[
        // Wikipedia "Ozone": IUPAC name "ozone", systematic "trioxygen" (Red Book IR-1.5.3.2,
        // Example 1; "ozone" is a traditional name, OPE-24). The neutral O3 is drawn with its
        // formal charges, a ring of three oxygens is cyclo-O3; both are O3.
        ("O=[O+][O-]", "trioxygen"),
        ("O1OO1", "trioxygen"),
        ("O=O", "dioxygen"),
        ("N#N", "dinitrogen"),
        ("ClCl", "dichlorine"),
        ("BrBr", "dibromine"),
        ("[H][H]", "dihydrogen"),
        ("S1SSSSSSS1", "octasulfur"), // IR-5.2
        ("P12P3P1P23", "tetraphosphorus"),
        ("[Ar]", "argon"),
        ("[Na]", "sodium"),
        ("[Fe]", "iron"),
        ("[Hg]", "mercury"),
        ("[Al]", "aluminium"),
        ("[Cs]", "caesium"),
        ("[Og]", "oganesson"),
    ]);
}

/// Every element has its English name (IR-3, Table I), spelt as IUPAC does.
#[test]
fn every_element_is_named() {
    use crate::molecule::elements::symbol;
    for z in 1..=118 {
        let s = symbol(z).unwrap();
        assert!(
            !super::inorganic::element_name(s).unwrap().is_empty(),
            "{s}"
        );
    }
    assert_eq!(super::inorganic::element_name("S"), Some("sulfur"));
    assert_eq!(super::inorganic::element_name("Ds"), Some("darmstadtium"));
    assert_eq!(super::inorganic::element_name("Ph"), None);
}

/// Salts drawn as separate ions (IR-5.4): the cations in alphabetical order, then the anion,
/// each with the multiplying prefix of its count in the formula unit.
#[test]
fn salts_of_separate_ions() {
    check_names(&[
        ("[Na+].[Cl-]", "sodium chloride"), // Wikipedia; Red Book IR-1.5.3.2, Example 2
        ("[K+].[Br-]", "potassium bromide"), // Wikipedia
        ("[Na+].[H-]", "sodium hydride"),   // Wikipedia
        // Wikipedia: "calcium chloride" (see the module notes).
        ("[Ca+2].[Cl-].[Cl-]", "calcium dichloride"),
        ("[Cl-].[Mg+2].[Cl-]", "magnesium dichloride"),
        ("[Li+].[F-]", "lithium fluoride"),
        ("[Cs+].[I-]", "caesium iodide"),
        ("[Na+].[Na+].[O-2]", "disodium oxide"),
        ("[Al+3].[Al+3].[O-2].[O-2].[O-2]", "dialuminium trioxide"),
        ("[Na+].[Na+].[S-2]", "disodium sulfide"),
        ("[Mg+2].[Mg+2].[Mg+2].[N-3].[N-3]", "trimagnesium dinitride"),
        ("[Ca+2].[Ca+2].[Ca+2].[P-3].[P-3]", "tricalcium diphosphide"), // IR-5.2, Example 10
        ("[Zn+2].[Cl-].[Cl-]", "zinc dichloride"),
        ("[NH4+].[Cl-]", "ammonium chloride"), // IR-1.5.3.2, Example 7
        ("[Ba+2].[O-][O-]", "barium peroxide"),
        ("[Na+].[N-]=[N+]=[N-]", "sodium azide"),
        ("[Na+].[OH-]", "sodium hydroxide"),
        ("[Ca+2].[OH-].[OH-]", "calcium dihydroxide"),
        ("[Na+].[C-]#N", "sodium cyanide"), // IR-1.5.3.2, Example 6
        // The organic anion keeps its name from the substitutive engine (IR-1.5.3.2, Example 8;
        // P-65.6.2.1), now with the divalent cations too.
        ("[Na+].CC([O-])=O", "sodium acetate"),
        ("[Ca+2].CC([O-])=O.CC([O-])=O", "calcium diacetate"),
        // Two formula units drawn: one name.
        ("[Na+].[Cl-].[Na+].[Cl-]", "sodium chloride"),
        // A neutral compound beside the salt keeps its own name.
        ("CCO.[Na+].[Cl-]", "ethanol; sodium chloride"),
    ]);
}

/// Several cations are cited in alphabetical order (IR-5.4.1).
#[test]
fn several_cations() {
    check_names(&[
        ("[K+].[Na+].[O-]S([O-])(=O)=O", "potassium sodium sulfate"),
        ("[Na+].[K+].[O-]S(=O)(=O)[O-]", "potassium sodium sulfate"),
        (
            "[K+].[Mg+2].[Cl-].[Cl-].[Cl-]",
            "magnesium potassium trichloride",
        ), // IR-5.4.2.1, Ex. 4
        (
            "[K+].[Al+3].[O-]S([O-])(=O)=O.[O-]S([O-])(=O)=O",
            "aluminium potassium bis(sulfate)", // IR-5.4.2.1 (KAl(SO4)2)
        ),
        (
            "[NH4+].[Na+].OP([O-])([O-])=O",
            "ammonium sodium hydrogenphosphate",
        ), // IR-5.4.1, Ex. 7
    ]);
}

/// The oxoanions of the table (IR-8, Table IR-8.1) and the hydrogen names of IR-8.5, whatever
/// the drawing puts the charges on.
#[test]
fn oxoanions() {
    check_names(&[
        ("[Na+].[Na+].[O-]C([O-])=O", "disodium carbonate"),
        // Wikipedia: "sodium hydrogen carbonate" (see the module notes).
        ("[Na+].OC([O-])=O", "sodium hydrogencarbonate"),
        (
            "[Ca+2].OC([O-])=O.OC([O-])=O",
            "calcium bis(hydrogencarbonate)",
        ), // IR-5.4.2.1, Ex. 11
        ("[Na+].[O-][N+]([O-])=O", "sodium nitrate"),
        ("[Na+].[O-]N(=O)=O", "sodium nitrate"),
        (
            "[Ca+2].[O-][N+]([O-])=O.[O-][N+]([O-])=O",
            "calcium bis(nitrate)",
        ),
        ("[Na+].[O-]N=O", "sodium nitrite"),
        ("[Na+].[Na+].[O-]S([O-])(=O)=O", "disodium sulfate"),
        ("[Na+].[Na+].[O-][S+2]([O-])([O-])[O-]", "disodium sulfate"),
        ("[Na+].OS([O-])(=O)=O", "sodium hydrogensulfate"),
        ("[Na+].[Na+].[O-]S([O-])=O", "disodium sulfite"),
        ("[Na+].OS([O-])=O", "sodium hydrogensulfite"),
        (
            "[Na+].[Na+].[Na+].[O-]P([O-])([O-])=O",
            "trisodium phosphate",
        ),
        (
            "[Ca+2].[Ca+2].[Ca+2].[O-]P([O-])([O-])=O.[O-]P([O-])([O-])=O",
            "tricalcium bis(phosphate)", // IR-5.4.2.1, Example 9
        ),
        ("[Na+].[Na+].OP([O-])([O-])=O", "disodium hydrogenphosphate"),
        ("[K+].OP(O)([O-])=O", "potassium dihydrogenphosphate"),
        ("[K+].[O-]Cl(=O)(=O)=O", "potassium perchlorate"),
        ("[K+].[O-]Cl(=O)=O", "potassium chlorate"),
        ("[Na+].[O-]Cl=O", "sodium chlorite"),
        ("[Na+].[O-]Cl", "sodium hypochlorite"),
        ("[K+].[O-]Br(=O)=O", "potassium bromate"),
        ("[K+].[O-]I(=O)=O", "potassium iodate"),
        ("[K+].[O-]I(=O)(=O)=O", "potassium periodate"),
        ("[Na+].[Na+].[O-][Se]([O-])(=O)=O", "disodium selenate"),
    ]);
}

/// A salt drawn with the metal bonded to its anion ("Na–Cl") is named as the salt: a
/// compositional name says nothing of the bonding (IR-5.2), and the metals of the cation table
/// have one charge, which is the number of their bonds. Only inorganic anions: a metal bonded to
/// an organic structure is refused (it may be an organometallic compound).
#[test]
fn salts_drawn_with_bonds() {
    check_names(&[
        ("[Na]Cl", "sodium chloride"),
        ("[K]Br", "potassium bromide"),
        ("[Na][H]", "sodium hydride"),
        ("Cl[Ca]Cl", "calcium dichloride"),
        ("[Na]O[Na]", "disodium oxide"),
        ("[Na]O", "sodium hydroxide"),
        ("[Na]OC(O)=O", "sodium hydrogencarbonate"),
        ("[Na]OS(=O)(=O)O[K]", "potassium sodium sulfate"),
        ("[Na]C#N", "sodium cyanide"),
    ]);
    check_refused(&[
        ("CC(=O)O[Na]", "draw a salt as separate ions"),
        ("C[Li]", "draw a salt as separate ions"),
        ("C[Mg]Br", "draw a salt as separate ions"),
        ("Cl[Na]Cl", "bonds"),
        ("[Na]=O", "bonds"),
        ("[Na][Na]", "bonds"),
    ]);
}

/// A drawing whose hydrogen was drawn as an atom on the ion, or whose ion is an abbreviation.
#[test]
fn drawn_hydrogens_and_labels() {
    // Na+ and a hydride drawn as a lone "H" atom carrying the charge.
    let mut m = Molecule::default();
    m.add_atom("Na".into(), [0.0, 0.0], 1);
    m.add_atom("H".into(), [1.5, 0.0], -1);
    assert_eq!(iupac_name(&m), NameResult::Ok("sodium hydride".into()));
    // Na–OH with the hydroxide drawn as an "OH" label.
    let mut m = Molecule::default();
    let na = m.add_atom("Na".into(), [0.0, 0.0], 0);
    let oh = m.add_atom("OH".into(), [1.5, 0.0], 0);
    m.add_bond(na, oh, BondOrder::Single);
    assert_eq!(iupac_name(&m), NameResult::Ok("sodium hydroxide".into()));
}

/// What the namer cannot name safely is refused with the reason.
#[test]
fn refusals() {
    check_refused(&[
        // Charges that do not balance.
        ("[Na+].[Cl-].[Cl-]", "charges do not balance"),
        ("[Ca+2].[Cl-]", "charges do not balance"),
        ("[Cl-]", "charges do not balance"),
        ("[Na+].[O-]S([O-])(=O)=O", "charges do not balance"),
        // Transition-metal salts need an oxidation or charge number: iron(II) or iron(III)?
        ("[Fe+2].[Cl-].[Cl-]", "oxidation number"),
        ("[Cu+].[Cl-]", "oxidation number"),
        ("Cl[Fe](Cl)Cl", "oxidation number"),
        ("[Sn+2].[Cl-].[Cl-]", "oxidation number"),
        // Coordination compounds: K4[Fe(CN)6], [Cu(NH3)4]2+.
        (
            "[K+].[K+].[K+].[K+].N#C[Fe-4](C#N)(C#N)(C#N)(C#N)C#N",
            "coordination",
        ),
        ("N[Cu+2](N)(N)N.[O-]S([O-])(=O)=O", "coordination"),
        // A charge the metal does not take.
        ("[Na+2].[O-2]", "charge"),
        ("[Ca+].[Cl-]", "charge"),
        // Ions outside the tables, and more than one kind of anion.
        ("[Na+].[NH2-]", "not in the table"),
        (
            "[Na+].[O-]P([O-])(=O)P([O-])([O-])=O.[Na+].[Na+].[Na+]",
            "not in the table",
        ),
        ("[Na+].[Na+].[Cl-].[Br-]", "more than one kind of anion"),
        ("[H+].[Cl-]", "hydron"),
        // Molecular compounds other than elements: hydrides, oxoacids, binary compounds of
        // nonmetals (their names need the element order of Table VI).
        ("O", "no carbon"),
        ("N", "no carbon"),
        ("Cl", "no carbon"),
        ("OO", "no carbon"),
        ("OS(O)(=O)=O", "no carbon"),
        ("FS(F)(F)(F)(F)F", "no carbon"),
        // A lone neutral hydrogen atom is H2 by its valence; H–H is drawn for dihydrogen.
        ("[H]", "hydrogen"),
    ]);
}
