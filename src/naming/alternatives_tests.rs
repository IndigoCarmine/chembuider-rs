//! Alternative names (OPE-24): the other names IUPAC 2013 accepts beside the preferred IUPAC
//! name — functional class (radicofunctional), retained, additive, conjunctive and systematic
//! names — for the examples of Wikipedia (ja) "IUPAC命名法" (oldid 111380645) and of IUPAC 2013.
//! Every structure's PIN is checked too: the alternatives never change it.

use super::{NameKind, NameResult, iupac_alternatives, iupac_name, iupac_naming};
use crate::molecule::smiles::parser::parse;

use NameKind::*;

fn pin(smiles: &str) -> String {
    match iupac_name(&parse(smiles).0) {
        NameResult::Ok(n) => n,
        other => panic!("{smiles}: not named: {other:?}"),
    }
}

fn alternatives(smiles: &str) -> Vec<(NameKind, String)> {
    iupac_alternatives(&parse(smiles).0)
}

/// Each structure's PIN, and an alternative of `kind` it must have.
fn check(cases: &[(&str, &str, NameKind, &str)]) {
    let mut failures = Vec::new();
    for &(smiles, expected_pin, kind, alternative) in cases {
        let got = pin(smiles);
        if got != expected_pin {
            failures.push(format!("{smiles}: PIN {got:?}, expected {expected_pin:?}"));
        }
        let alts = alternatives(smiles);
        if !alts.contains(&(kind, alternative.to_string())) {
            failures.push(format!("{smiles}: no {kind:?} {alternative:?} among {alts:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// No alternative of these structures contains the given text.
fn check_absent(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for &(smiles, text) in cases {
        let alts = alternatives(smiles);
        if let Some(a) = alts.iter().find(|(_, n)| n.contains(text)) {
            failures.push(format!("{smiles} ({}): unexpected {a:?}", pin(smiles)));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Functional class names of alcohols, halides and ethers (the alkyl group, then the class
/// name: P-63.1.5, P-63.2.1.1, P-61.3.2), and amines named with "amine" after the group
/// (P-62.2.1.1.2).
#[test]
fn radicofunctional_names() {
    check(&[
        ("CO", "methanol", Radicofunctional, "methyl alcohol"),
        ("CCCO", "propan-1-ol", Radicofunctional, "propyl alcohol"),
        ("CCCCCl", "1-chlorobutane", Radicofunctional, "butyl chloride"),
        ("CN", "methanamine", Radicofunctional, "methylamine"),
        ("COCC", "methoxyethane", Radicofunctional, "ethyl methyl ether"),
        // Alphanumerical order, "tert" not counted; two identical groups multiplied.
        ("CCOCC", "ethoxyethane", Radicofunctional, "diethyl ether"),
        ("COC(C)(C)C", "2-methoxy-2-methylpropane", Radicofunctional, "tert-butyl methyl ether"),
        ("CC(C)O", "propan-2-ol", Radicofunctional, "propan-2-yl alcohol"),
        ("OCC1=CC=CC=C1", "phenylmethanol", Radicofunctional, "benzyl alcohol"),
        ("CI", "iodomethane", Radicofunctional, "methyl iodide"),
        ("NC1CCCCC1", "cyclohexanamine", Radicofunctional, "cyclohexylamine"),
    ]);
    check_absent(&[
        // A phenol is not an alcohol, nor an aryl halide an alkyl halide.
        ("OC1=CC=CC=C1", "alcohol"),
        ("ClC1=CC=CC=C1", "chloride"),
        // Several groups, or groups that are not hydrocarbyl: no functional class name.
        ("OCCO", "alcohol"),
        ("OCCCl", "alcohol"),
        ("OCCCl", "chloride"),
        ("ClCCl", "chloride"),
        ("COCCO", "ether"),
        ("NCCN", "ylamine"),
        // An amine on a substituted group would need enclosing marks the namer does not decide.
        ("CC(C)N", "ylamine"),
    ]);
}

/// Retained names for general nomenclature (P-63.1.1.2 glycerol, P-65.1.1.2 propionic and
/// succinic acid, P-65.6.3.5.1 ε-caprolactone, IR-3.4.3 ozone), of the unsubstituted compounds.
#[test]
fn retained_names() {
    check(&[
        ("OCC(O)CO", "propane-1,2,3-triol", Retained, "glycerol"),
        ("OC(=O)CCC(O)=O", "butanedioic acid", Retained, "succinic acid"),
        ("CCC(=O)O", "propanoic acid", Retained, "propionic acid"),
        ("CCC(=O)[O-].[K+]", "potassium propanoate", Retained, "potassium propionate"),
        ("CCC(=O)OCC", "ethyl propanoate", Retained, "ethyl propionate"),
        ("CCC(=O)OC(=O)CC", "propanoic anhydride", Retained, "propionic anhydride"),
        ("CCC(Cl)=O", "propanoyl chloride", Retained, "propionyl chloride"),
        // Wikipedia: "ozone"; the Red Book gives "trioxygen" first (IR-3.4.3) and "ozone" as
        // the acceptable alternative.
        ("O1OO1", "trioxygen", Retained, "ozone"),
        ("O=[O+][O-]", "trioxygen", Retained, "ozone"),
        ("O=C1CCCCCO1", "oxepan-2-one", Retained, "ε-caprolactone"),
        ("O=C1CCCCCN1", "azepan-2-one", Retained, "ε-caprolactam"),
        // The substituent group –CH(CH3)–CH2– (P-29.6.2.3); only the group, never propene.
        ("*C(C)C*", "propane-1,2-diyl", Retained, "propylene"),
        ("*CC*", "ethane-1,2-diyl", Retained, "ethylene"),
    ]);
    // Not dioxygen: one of the retained names of elements is ozone alone.
    assert!(alternatives("O=O").is_empty(), "{:?}", alternatives("O=O"));
}

/// Retained names are not derivatised (P-65.1.1.2: no substitution on these names; Wikipedia's
/// "1-chloro-2-propyl" for what is not "1-chloroisopropyl"): an alternative is never a retained
/// name with substituents, and never names a compound other than the retained one.
#[test]
fn retained_names_are_not_derivatised() {
    assert_eq!(pin("*C(C)CCl"), "1-chloropropan-2-yl");
    check_absent(&[
        ("*C(C)CCl", "isopropyl"),
        ("*C(C)CCl", "propylene"),
        ("*C(C)C", "isopropyl"),
        ("ClCC(O)CO", "glycerol"),
        ("OCC(O)C(O)CO", "glycerol"),
        ("CC(Cl)C(O)=O", "propionic"),
        ("CC=CC(O)=O", "propionic"),
        ("CCCC(O)=O", "propionic"),
        ("CC(CC(O)=O)C(O)=O", "succinic"),
        ("OC(=O)C=CC(O)=O", "succinic"),
        ("OC(=O)CCC([O-])=O.[Na+]", "succinate"),
        ("CC1CCCCC(=O)O1", "caprolact"),
        ("O=C1CCCCCCO1", "caprolact"),
        ("CC1CCCO1", "tetrahydrofuran"),
        ("C1CCOCC1", "tetrahydrofuran"),
        ("*CCC*", "propylene"),
        ("*C(Cl)C*", "ethylene"),
        ("CC=C", "propylene"),
    ]);
}

/// Systematic names other than the PIN: the retained preferred names written systematically
/// (P-65.1.1.1, P-65.5.1, P-65.7.2, P-66.6.1.1.1), "per" for complete substitution, the
/// lactones and lactams named from the hydroxy and amino acids, the amine as a prefix, and
/// [n]annulene (P-22.1.2).
#[test]
fn systematic_names() {
    check(&[
        // P-66.6.1.1.1: formaldehyde is the retained PIN, methanal the systematic name.
        ("C=O", "formaldehyde", Systematic, "methanal"),
        ("CC=O", "acetaldehyde", Systematic, "ethanal"),
        ("OC=O", "formic acid", Systematic, "methanoic acid"),
        ("CC(O)=O", "acetic acid", Systematic, "ethanoic acid"),
        ("OC(=O)C(O)=O", "oxalic acid", Systematic, "ethanedioic acid"),
        ("OC(=O)C1=CC=CC=C1", "benzoic acid", Systematic, "benzenecarboxylic acid"),
        ("CC(Cl)=O", "acetyl chloride", Systematic, "ethanoyl chloride"),
        ("CC(=O)OC(=O)CC", "acetic propanoic anhydride", Systematic, "ethanoic propanoic anhydride"),
        ("CC(=O)OCC", "ethyl acetate", Systematic, "ethyl ethanoate"),
        ("CC1=CC=CC=C1", "toluene", Systematic, "methylbenzene"),
        ("OC1=CC=CC=C1", "phenol", Systematic, "benzenol"),
        ("NC1=CC=CC=C1", "aniline", Systematic, "benzenamine"),
        ("OC(=O)C1=CC=C(C)C=C1", "4-methylbenzoic acid", Systematic, "4-methylbenzene-1-carboxylic acid"),
        ("OC(=O)CCl", "chloroacetic acid", Systematic, "2-chloroethanoic acid"),
        // Wikipedia: "perbromoethane" (P-14.3.4.5: not for PINs).
        ("BrC(Br)(Br)C(Br)(Br)Br", "hexabromoethane", Systematic, "perbromoethane"),
        // Wikipedia: "6-hexanelactone", "6-hexanelactam" (P-65.6.3.5.1, P-66.1.5.1).
        ("O=C1CCCCCO1", "oxepan-2-one", Systematic, "6-hexanelactone"),
        ("O=C1CCCCCN1", "azepan-2-one", Systematic, "6-hexanelactam"),
        ("O=C1CCCO1", "oxolan-2-one", Systematic, "4-butanelactone"),
        ("CN", "methanamine", Systematic, "aminomethane"),
        ("NCCN", "ethane-1,2-diamine", Systematic, "1,2-diaminoethane"),
        // Wikipedia: "[14]annulene" (P-22.1.2: general nomenclature).
        ("C1=CC=CC=CC=CC=CC=CC=C1", "cyclotetradeca-1,3,5,7,9,11,13-heptaene", Systematic, "[14]annulene"),
        ("C1=CC=CC=CC=C1", "cycloocta-1,3,5,7-tetraene", Systematic, "[8]annulene"),
    ]);
    check_absent(&[
        // "per" for halogens on a chain only; not for a partly substituted one.
        ("BrC(Br)C(Br)(Br)Br", "per"),
        ("BrC(Br)(Br)Br", "per"),
        // Benzene is not [6]annulene, and a substituted annulene has no such name here.
        ("C1=CC=CC=C1", "annulene"),
        ("CC1=CC=CC=CC=C1", "annulene"),
        // The aminoalkane form of an N-substituted amine is not produced.
        ("CNC", "amino"),
        ("NCCO", "amino"),
        // Substituted lactones keep their PIN alone.
        ("CC1CCCCC(=O)O1", "lactone"),
    ]);
}

/// Additive names (P-31.1.4.2.4: hydro prefixes): tetrahydrofuran for oxolane, whose PIN is
/// the Hantzsch–Widman name.
#[test]
fn additive_names() {
    check(&[("C1CCOC1", "oxolane", Additive, "tetrahydrofuran")]);
}

/// Conjunctive names (P-15.6): the ring's name in front of the chain's, the chain carrying the
/// principal group at its end; the PIN is substitutive (P-15.6.0: not used in PINs).
#[test]
fn conjunctive_names() {
    check(&[
        ("OCC1CCCCC1", "cyclohexylmethanol", Conjunctive, "cyclohexanemethanol"),
        ("OCCC1CCCCC1", "2-cyclohexylethan-1-ol", Conjunctive, "cyclohexaneethanol"),
        ("OCC1=CC=CC=C1", "phenylmethanol", Conjunctive, "benzenemethanol"),
        ("OC(=O)CC1CCCCC1", "cyclohexylacetic acid", Conjunctive, "cyclohexaneacetic acid"),
    ]);
    check_absent(&[
        ("OCC1CCCCC1C", "methanol"),
        ("OCC(C)C1CCCCC1", "ethanol"),
        ("OCC1=CC=CC=C1Cl", "benzenemethanol"),
    ]);
}

/// Subtractive names (deoxyribose) are out of scope, and the retained names of sugars with them.
#[test]
fn no_subtractive_or_japanese_names() {
    for smiles in ["OCC1OC(O)CC1O", "OCC(O)CO", "CO", "C=O"] {
        let alts = alternatives(smiles);
        assert!(!alts.iter().any(|(k, _)| *k == Japanese), "{smiles}: {alts:?}");
        assert!(!alts.iter().any(|(_, n)| n.contains("deoxy")), "{smiles}: {alts:?}");
    }
}

/// The alternatives never repeat the PIN or one another, and come in a fixed order: by kind,
/// then as generated.
#[test]
fn alternatives_are_distinct_and_ordered() {
    let all = [
        "CO", "CCCO", "CN", "COCC", "OCC(O)CO", "C=O", "OC(=O)CCC(O)=O", "OC(=O)C1=CC=CC=C1",
        "CC(Cl)=O", "CCC(=O)[O-].[K+]", "CCC(=O)OCC", "O1OO1", "O=C1CCCCCO1", "O=C1CCCCCN1",
        "CC(=O)OC(=O)CC", "BrC(Br)(Br)C(Br)(Br)Br", "*C(C)C*", "C1CCOC1", "OCC1CCCCC1",
        "C1=CC=CC=CC=CC=CC=CC=C1", "CCCCCl",
    ];
    for smiles in all {
        let naming = iupac_naming(&parse(smiles).0);
        let pin = naming.name.name().expect("named").to_string();
        let alts = naming.alternatives;
        assert_eq!(alts, alternatives(smiles), "{smiles}");
        assert!(!alts.iter().any(|(_, n)| *n == pin), "{smiles}: PIN repeated in {alts:?}");
        for (i, (_, a)) in alts.iter().enumerate() {
            assert!(!alts[..i].iter().any(|(_, b)| b == a), "{smiles}: {a:?} twice in {alts:?}");
        }
        let kinds: Vec<NameKind> = alts.iter().map(|(k, _)| *k).collect();
        let mut sorted = kinds.clone();
        sorted.sort();
        assert_eq!(kinds, sorted, "{smiles}: {alts:?}");
        assert_eq!(alts, alternatives(smiles), "{smiles}: not deterministic");
    }
    // Methanol has one alternative; methanamine two, by kind.
    assert_eq!(alternatives("CO"), vec![(Radicofunctional, "methyl alcohol".to_string())]);
    assert_eq!(
        alternatives("CN"),
        vec![
            (Radicofunctional, "methylamine".to_string()),
            (Systematic, "aminomethane".to_string()),
        ]
    );
}

/// A drawing of several compounds: each alternative is that of one compound, the others by
/// their PINs; a structure that cannot be named leaves the drawing without alternatives.
#[test]
fn alternatives_of_several_compounds() {
    assert_eq!(pin("CO.C=O"), "methanol; formaldehyde");
    assert_eq!(
        alternatives("CO.C=O"),
        vec![
            (Radicofunctional, "methyl alcohol; formaldehyde".to_string()),
            (Systematic, "methanol; methanal".to_string()),
        ]
    );
    assert!(alternatives("CO.C1CCC2CCCCC2C1").is_empty());
    assert!(alternatives("").is_empty());
}

/// Salts of one cation and one anion of the tables without multiplying prefixes, as IR-5.2
/// allows where the fixed charges fix the stoichiometry; the PIN states every proportion.
#[test]
fn salts_without_multiplying_prefixes() {
    check(&[
        ("[Ca+2].[Cl-].[Cl-]", "calcium dichloride", Systematic, "calcium chloride"),
        ("Cl[Ca]Cl", "calcium dichloride", Systematic, "calcium chloride"),
        ("[Na+].[Na+].[O-]S([O-])(=O)=O", "disodium sulfate", Systematic, "sodium sulfate"),
    ]);
    check_absent(&[
        // Two cations: the charges do not fix the proportion.
        ("[K+].[Al+3].[O-]S([O-])(=O)=O.[O-]S([O-])(=O)=O", "aluminium potassium sulfate"),
        // An organic anion keeps its prefix (the namer does not judge partial salts).
        ("[O-]C(=O)CCC([O-])=O.[Na+].[Na+]", "sodium butanedioate"),
    ]);
}

/// Structures without alternatives have none, and those the namer refuses none either.
#[test]
fn no_alternatives() {
    for smiles in ["CC(C)C(C)CC", "CCC(C)=O", "C1CCC2CCCCC2C1", "[Na+].[Cl-]"] {
        assert!(alternatives(smiles).is_empty(), "{smiles}: {:?}", alternatives(smiles));
    }
}

#[test]
fn kind_labels() {
    assert_eq!(Retained.label(), "retained name");
    assert_eq!(Radicofunctional.label(), "functional class name");
    assert_eq!(Japanese.label(), "Japanese name");
    assert_eq!(NameKind::ALL.len(), 6);
}

