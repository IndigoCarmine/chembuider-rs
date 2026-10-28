//! Table-driven tests: SMILES (Kekulé, read by the test-only parser) → expected name.
//!
//! The reference names were obtained from PubChem (`IUPACName`, computed by OpenEye Lexichem)
//! for each SMILES. Where the expected name differs, the PubChem name is noted beside it; the
//! differences are all IUPAC 2013 rules Lexichem does not follow:
//!
//! - retained names that are not preferred IUPAC names: acetylene, cumene, styrene,
//!   chloroform, terephthalic acid, phthalaldehyde, oxaldehyde, oxaldehydic acid,
//!   cyclooctatetraene (→ ethyne, (propan-2-yl)benzene, ethenylbenzene, trichloromethane,
//!   benzene-1,4-dicarboxylic acid, benzene-1,2-dicarbaldehyde, ethanedial, oxoacetic acid,
//!   cycloocta-1,3,5,7-tetraene);
//! - locants that 2013 cites and Lexichem drops: "ethan-1-ol" when a second feature is present,
//!   "ethan-1-one", "cyclohex-1-ene"; and locants 2013 omits that Lexichem cites: substituted
//!   acetic acid, acetaldehyde and acetonitrile ("chloroacetic acid", P-65.1.1.1), fully
//!   substituted parents ("hexachloroethane", "tetrachloroethene");
//! - simple prefixes after the first one enclosed when no locants are cited
//!   ("hydroxy(phenyl)acetic acid", P-16.5.1.3.1; Lexichem: "2-hydroxy-2-phenylacetic acid");
//! - enclosing marks around prefixes that contain locants or are compound ("(propan-2-yl)",
//!   "(chloromethyl)"), and "pentyloxy" for the C5 alkoxy group (Lexichem: "pentoxy").

use super::{NameResult, iupac_name, iupac_names};
use crate::molecule::smiles::parser::parse;
use crate::molecule::{BondOrder, Molecule};

fn name_of(smiles: &str) -> NameResult {
    iupac_name(&parse(smiles).0)
}

const CASES: &[(&str, &str)] = &[
    ("C", "methane"),
    ("CC", "ethane"),
    ("CCC", "propane"),
    ("CCCC", "butane"),
    ("CC(C)C", "2-methylpropane"),
    ("CC(C)(C)C", "2,2-dimethylpropane"),
    ("CCC(C)CC", "3-methylpentane"),
    ("CCC(CC)C(C)C", "3-ethyl-2-methylpentane"),
    ("CCCCCCCCCC", "decane"),
    ("CCCCC(CC)CCC", "4-ethyloctane"),
    ("CC(C)CC(C)(C)C", "2,2,4-trimethylpentane"),
    ("CCC(C)C(C)(CC)CC", "3-ethyl-3,4-dimethylhexane"),
    ("C=C", "ethene"),
    ("CC=C", "prop-1-ene"),
    ("CC=CC", "but-2-ene"),
    ("C=CC=C", "buta-1,3-diene"),
    ("C#C", "ethyne"), // PubChem: "acetylene"
    ("CC#CC", "but-2-yne"),
    ("C=CC#C", "but-1-en-3-yne"),
    ("C#CCC=C", "pent-1-en-4-yne"),
    ("CC(=C)C", "2-methylprop-1-ene"),
    ("CCCCC(=C)CCCC", "5-methylidenenonane"),
    ("C=CC(C)C", "3-methylbut-1-ene"),
    ("C1CC1", "cyclopropane"),
    ("C1CCCCC1", "cyclohexane"),
    ("CC1CCCCC1", "methylcyclohexane"),
    ("CC1CCCC(C)C1", "1,3-dimethylcyclohexane"),
    ("C1=CCCCC1", "cyclohexene"),
    ("CC1=CCCCC1", "1-methylcyclohex-1-ene"), // PubChem: "1-methylcyclohexene"
    ("CC1CCCC=C1", "3-methylcyclohex-1-ene"), // PubChem: "3-methylcyclohexene"
    ("C1=CC=CCC1", "cyclohexa-1,3-diene"),
    ("C1=CC=CC=C1", "benzene"),
    ("CC1=CC=CC=C1", "toluene"),
    ("CC1=CC=CC=C1C", "1,2-xylene"),
    ("CCC1=CC=CC=C1", "ethylbenzene"),
    ("CC(C)C1=CC=CC=C1", "(propan-2-yl)benzene"), // PubChem: "cumene"
    ("CC(C)(C)C1=CC=CC=C1", "tert-butylbenzene"),
    ("C=CC1=CC=CC=C1", "ethenylbenzene"), // PubChem: "styrene"
    ("CCCCCCC1=CC=CC=C1", "hexylbenzene"),
    ("CC1CCCCC1C", "1,2-dimethylcyclohexane"),
    ("CC(C)C1CCCCC1", "(propan-2-yl)cyclohexane"), // PubChem: "propan-2-ylcyclohexane"
    ("C1CCC(CC1)C=C", "ethenylcyclohexane"),
    ("C=C1CCCCC1", "methylidenecyclohexane"),
    ("CCC=C1CCCCC1", "propylidenecyclohexane"),
    ("CCl", "chloromethane"),
    ("ClCCl", "dichloromethane"),
    ("ClC(Cl)Cl", "trichloromethane"), // PubChem: "chloroform"
    ("ClC(Cl)(Cl)Cl", "tetrachloromethane"),
    ("FC(Cl)Cl", "dichloro(fluoro)methane"),
    ("BrCCl", "bromo(chloro)methane"),
    ("CCBr", "bromoethane"),
    ("ClCCCl", "1,2-dichloroethane"),
    ("CC(Cl)Cl", "1,1-dichloroethane"),
    ("CC(Br)CC", "2-bromobutane"),
    ("CC(C)(C)Cl", "2-chloro-2-methylpropane"),
    ("ClC(Cl)(Cl)C(Cl)(Cl)Cl", "hexachloroethane"), // PubChem: "1,1,1,2,2,2-hexachloroethane"
    ("C=CCl", "chloroethene"),
    ("ClC1=CC=CC=C1", "chlorobenzene"),
    ("ClC1=CC=C(Br)C=C1", "1-bromo-4-chlorobenzene"),
    ("CC1=CC=C(Cl)C=C1", "1-chloro-4-methylbenzene"),
    ("FC(F)(F)C1=CC=CC=C1", "(trifluoromethyl)benzene"), // PubChem: "trifluoromethylbenzene"
    ("ClCC1=CC=CC=C1", "(chloromethyl)benzene"),         // PubChem: "chloromethylbenzene"
    ("CC(Cl)CC(C)Br", "2-bromo-4-chloropentane"),
    ("ClC1=CC(Cl)=CC(Cl)=C1", "1,3,5-trichlorobenzene"),
    ("FC1=C(F)C(F)=C(F)C(F)=C1F", "hexafluorobenzene"), // PubChem: "1,2,3,4,5,6-hexafluorobenzene"
    ("C[N+](=O)[O-]", "nitromethane"),
    ("[O-][N+](=O)C1=CC=CC=C1", "nitrobenzene"),
    ("CC1=CC=C(C=C1)[N+]([O-])=O", "1-methyl-4-nitrobenzene"),
    ("COC", "methoxymethane"),
    ("CCOCC", "ethoxyethane"),
    ("CCCOC", "1-methoxypropane"),
    ("COC1=CC=CC=C1", "anisole"),
    ("CC(C)OC", "2-methoxypropane"),
    ("CCOC1=CC=C(Cl)C=C1", "1-chloro-4-ethoxybenzene"),
    ("CC(C)(C)OC", "2-methoxy-2-methylpropane"),
    ("CCCCCOC1CCCCC1", "(pentyloxy)cyclohexane"), // PubChem: "pentoxycyclohexane"
    ("COCCO", "2-methoxyethan-1-ol"),             // PubChem: "2-methoxyethanol"
    ("CO", "methanol"),
    ("CCO", "ethanol"),
    ("CCCO", "propan-1-ol"),
    ("CC(C)O", "propan-2-ol"),
    ("CC(C)(C)O", "2-methylpropan-2-ol"),
    ("OCCO", "ethane-1,2-diol"),
    ("OCC(O)CO", "propane-1,2,3-triol"),
    ("C=CCO", "prop-2-en-1-ol"),
    ("OC1CCCCC1", "cyclohexanol"),
    ("CC1CCCCC1O", "2-methylcyclohexan-1-ol"),
    ("OC1=CC=CC=C1", "phenol"),
    ("CC1=CC=C(O)C=C1", "4-methylphenol"),
    ("OC1=CC=CC=C1O", "benzene-1,2-diol"),
    ("OCC1=CC=CC=C1", "phenylmethanol"),
    ("OCC1CCCCC1O", "2-(hydroxymethyl)cyclohexan-1-ol"),
    ("ClCCO", "2-chloroethan-1-ol"), // PubChem: "2-chloroethanol"
    ("CC(O)C#C", "but-3-yn-2-ol"),
    ("OC1C=CCCC1", "cyclohex-2-en-1-ol"),
    ("OCC(CO)(CO)CO", "2,2-bis(hydroxymethyl)propane-1,3-diol"),
    ("CN", "methanamine"),
    ("CCN", "ethanamine"),
    ("CCCN", "propan-1-amine"),
    ("CC(C)N", "propan-2-amine"),
    ("CNC", "N-methylmethanamine"),
    ("CN(C)C", "N,N-dimethylmethanamine"),
    ("CCNC", "N-methylethanamine"),
    ("CCCN(C)CC", "N-ethyl-N-methylpropan-1-amine"),
    ("NC1=CC=CC=C1", "aniline"),
    ("CNC1=CC=CC=C1", "N-methylaniline"),
    ("CN(C)C1=CC=CC=C1", "N,N-dimethylaniline"),
    ("NC1CCCCC1", "cyclohexanamine"),
    ("NCCN", "ethane-1,2-diamine"),
    ("NCCO", "2-aminoethan-1-ol"), // PubChem: "2-aminoethanol"
    ("CN(C)CCO", "2-(dimethylamino)ethan-1-ol"), // PubChem: "2-(dimethylamino)ethanol"
    ("CC1=CC=C(N)C=C1", "4-methylaniline"),
    ("CC(C)NC(C)C", "N-(propan-2-yl)propan-2-amine"), // PubChem: "N-propan-2-ylpropan-2-amine"
    ("CC(C)=O", "propan-2-one"),
    ("CCC(C)=O", "butan-2-one"),
    ("CC(=O)CC(C)=O", "pentane-2,4-dione"),
    ("O=C1CCCCC1", "cyclohexanone"),
    ("CC(=O)C1=CC=CC=C1", "1-phenylethan-1-one"), // PubChem: "1-phenylethanone"
    ("CC1CCCC(=O)C1", "3-methylcyclohexan-1-one"),
    ("O=C1C=CCCC1", "cyclohex-2-en-1-one"),
    ("CC(=O)C=C", "but-3-en-2-one"),
    ("O=C1C=CC(=O)C=C1", "cyclohexa-2,5-diene-1,4-dione"),
    ("CC(=O)CCO", "4-hydroxybutan-2-one"),
    ("CC(=O)C1CCCCC1", "1-cyclohexylethan-1-one"), // PubChem: "1-cyclohexylethanone"
    ("C=O", "formaldehyde"),
    ("CC=O", "acetaldehyde"),
    ("CCC=O", "propanal"),    // PubChem: (no name)
    ("O=CC=O", "ethanedial"), // PubChem: "oxaldehyde"
    ("O=CC1=CC=CC=C1", "benzaldehyde"),
    ("O=CC1CCCCC1", "cyclohexanecarbaldehyde"),
    ("CC(C)C=O", "2-methylpropanal"),
    ("OC1=CC=CC=C1C=O", "2-hydroxybenzaldehyde"),
    ("O=CCC1=CC=CC=C1", "phenylacetaldehyde"), // PubChem: "2-phenylacetaldehyde"
    ("C=CC=O", "prop-2-enal"),
    ("CC#N", "acetonitrile"),
    ("CCC#N", "propanenitrile"),
    ("N#CC1=CC=CC=C1", "benzonitrile"),
    ("N#CCC#N", "propanedinitrile"),
    ("N#CC1CCCCC1", "cyclohexanecarbonitrile"),
    ("N#CCC1=CC=CC=C1", "phenylacetonitrile"), // PubChem: "2-phenylacetonitrile"
    ("OCCC#N", "3-hydroxypropanenitrile"),
    ("C=CC#N", "prop-2-enenitrile"),
    ("OC=O", "formic acid"),
    ("CC(O)=O", "acetic acid"),
    ("CCC(O)=O", "propanoic acid"),
    ("CC(C)C(O)=O", "2-methylpropanoic acid"),
    ("OC(=O)C(O)=O", "oxalic acid"),
    ("OC(=O)CCC(O)=O", "butanedioic acid"),
    ("OC(=O)C1=CC=CC=C1", "benzoic acid"),
    ("OC(=O)C1CCCCC1", "cyclohexanecarboxylic acid"),
    ("ClCC(O)=O", "chloroacetic acid"), // PubChem: "2-chloroacetic acid"
    ("OC(=O)C(F)(F)F", "trifluoroacetic acid"), // PubChem: "2,2,2-trifluoroacetic acid"
    ("OC(=O)CC1=CC=CC=C1", "phenylacetic acid"), // PubChem: "2-phenylacetic acid"
    ("CC(O)C(O)=O", "2-hydroxypropanoic acid"),
    ("OC(=O)CCC=O", "4-oxobutanoic acid"),
    ("OC(=O)C1=CC=C(C=C1)C(O)=O", "benzene-1,4-dicarboxylic acid"), // PubChem: "terephthalic acid"
    ("OC(=O)C1=CC=C(Cl)C=C1", "4-chlorobenzoic acid"),
    ("C=CC(O)=O", "prop-2-enoic acid"),
    ("CC(=O)C(O)=O", "2-oxopropanoic acid"),
    ("OC(=O)CC#N", "cyanoacetic acid"), // PubChem: "2-cyanoacetic acid"
    ("OC(=O)C1=CC=CC=C1CC(O)=O", "2-(carboxymethyl)benzoic acid"),
    (
        "OC(=O)C1CCC(CC1)C=O",
        "4-formylcyclohexane-1-carboxylic acid",
    ),
    ("CC(=O)C1=CC=C(C=C1)C(O)=O", "4-acetylbenzoic acid"),
    ("OC(=O)CC(=O)C1=CC=CC=C1", "3-oxo-3-phenylpropanoic acid"),
    ("NC=O", "formamide"),
    ("CC(N)=O", "acetamide"),
    ("CCC(N)=O", "propanamide"),
    ("CNC(C)=O", "N-methylacetamide"),
    ("CN(C)C=O", "N,N-dimethylformamide"),
    ("NC(=O)C1=CC=CC=C1", "benzamide"),
    ("CNC(=O)C1=CC=CC=C1", "N-methylbenzamide"),
    ("CC(=O)NC1=CC=CC=C1", "N-phenylacetamide"),
    ("NC(=O)CCl", "2-chloroacetamide"),
    ("NC(=O)C1CCCCC1", "cyclohexanecarboxamide"),
    ("NC(=O)CCC(N)=O", "butanediamide"),
    ("CC(C)CC(N)=O", "3-methylbutanamide"),
    ("COC=O", "methyl formate"),
    ("CCOC(C)=O", "ethyl acetate"),
    ("COC(=O)C1=CC=CC=C1", "methyl benzoate"),
    ("CC(=O)OC1=CC=CC=C1", "phenyl acetate"),
    ("CC(C)OC(C)=O", "propan-2-yl acetate"),
    ("CCCC(=O)OCC", "ethyl butanoate"),
    ("CC(=O)OCC1=CC=CC=C1", "benzyl acetate"),
    ("COC(=O)C=C", "methyl prop-2-enoate"),
    ("CC(=O)OC(C)(C)C", "tert-butyl acetate"),
    ("COC(=O)CCl", "methyl chloroacetate"), // PubChem: "methyl 2-chloroacetate"
    ("OCCOC(C)=O", "2-hydroxyethyl acetate"),
    ("C=COC(C)=O", "ethenyl acetate"),
    ("NC1=CC=C(C=C1)C(O)=O", "4-aminobenzoic acid"),
    ("OC1=CC=C(C=C1)C#N", "4-hydroxybenzonitrile"),
    ("COC1=CC=C(C=O)C=C1", "4-methoxybenzaldehyde"),
    ("CC(C)C1=CC=C(C)C=C1", "1-methyl-4-(propan-2-yl)benzene"), // PubChem: "1-methyl-4-propan-2-ylbenzene"
    ("CCCC(C)C(O)=O", "2-methylpentanoic acid"),
    ("OCC=O", "hydroxyacetaldehyde"), // PubChem: "2-hydroxyacetaldehyde"
    ("CC(=O)CC(O)=O", "3-oxobutanoic acid"),
    ("CCC(=O)C1=CC=CC=C1", "1-phenylpropan-1-one"),
    (
        "CC(C)CC1=CC=C(C=C1)C(C)C(O)=O",
        "2-[4-(2-methylpropyl)phenyl]propanoic acid",
    ),
    ("CCOC(=O)C(C)Br", "ethyl 2-bromopropanoate"),
    ("CCN(CC)CC", "N,N-diethylethanamine"),
    ("CC(C)N(C(C)C)CC", "N-ethyl-N-(propan-2-yl)propan-2-amine"), // PubChem: "N-ethyl-N-propan-2-ylpropan-2-amine"
    ("NCC1=CC=CC=C1", "phenylmethanamine"),
    ("OCCN(CCO)CCO", "2-[bis(2-hydroxyethyl)amino]ethan-1-ol"), // PubChem: "2-[bis(2-hydroxyethyl)amino]ethanol"
    (
        "ClCCN(CCCl)C",
        "2-chloro-N-(2-chloroethyl)-N-methylethan-1-amine",
    ), // PubChem: "2-chloro-N-(2-chloroethyl)-N-methylethanamine"
    ("CC(C)(C)C1=CC=C(O)C=C1", "4-tert-butylphenol"),
    ("CCCCC=O", "pentanal"),
    ("CC=CC=O", "but-2-enal"),
    ("CC(=O)C(C)(C)C", "3,3-dimethylbutan-2-one"),
    ("CCC(CC)C(C)=O", "3-ethylpentan-2-one"),
    ("C1CC1C(O)=O", "cyclopropanecarboxylic acid"),
    ("OC1=CC(O)=CC(O)=C1", "benzene-1,3,5-triol"),
    ("OC(=O)C1=CC=CC(=C1)C(O)=O", "benzene-1,3-dicarboxylic acid"),
    ("COC(C)OC", "1,1-dimethoxyethane"),
    ("CC(C)=CC(C)=O", "4-methylpent-3-en-2-one"),
    ("CC(O)CC(C)(C)O", "2-methylpentane-2,4-diol"),
    ("OCC#CCO", "but-2-yne-1,4-diol"),
    ("BrC1=CC=C(C=C1)C(C)=O", "1-(4-bromophenyl)ethan-1-one"), // PubChem: "1-(4-bromophenyl)ethanone"
    ("OC(=O)CC1CCCCC1", "cyclohexylacetic acid"),              // PubChem: "2-cyclohexylacetic acid"
    ("NC(=O)C1=CC=C(C=C1)C(O)=O", "4-carbamoylbenzoic acid"),
    ("CCCCCC(CC)CCO", "3-ethyloctan-1-ol"),
    ("CCC(C)(C)CC(C)C", "2,4,4-trimethylhexane"),
    ("CC(C)CC(C)CC", "2,4-dimethylhexane"),
    ("CCCC(CCC)C(C)C", "4-(propan-2-yl)heptane"), // PubChem: "4-propan-2-ylheptane"
    ("C=CC(=C)C", "2-methylbuta-1,3-diene"),
    ("CC=C(C)C", "2-methylbut-2-ene"),
    ("CC(C)=CC=C", "4-methylpenta-1,3-diene"),
    ("C#CC(C)(C)O", "2-methylbut-3-yn-2-ol"),
    ("OC(=O)C=CC(O)=O", "but-2-enedioic acid"),
    ("CC(C)(O)C(O)=O", "2-hydroxy-2-methylpropanoic acid"),
    ("OC1=CC=C(C=C1)C(O)=O", "4-hydroxybenzoic acid"),
    ("NC1=CC=C(O)C=C1", "4-aminophenol"),
    ("OC(=O)C1=CC=CC=C1O", "2-hydroxybenzoic acid"),
    ("OC(=O)CCCCC(O)=O", "hexanedioic acid"),
    ("CC(C)(C)C(O)=O", "2,2-dimethylpropanoic acid"),
    ("CCC(C)C", "2-methylbutane"),
    ("CC1=CC(C)=CC(C)=C1", "1,3,5-trimethylbenzene"),
    ("CC1=CC=C(C)C=C1", "1,4-xylene"),
    ("ClC1=CC=CC(Cl)=C1Cl", "1,2,3-trichlorobenzene"),
    ("OC1=C(Cl)C=CC=C1Cl", "2,6-dichlorophenol"),
    ("BrC1=CC=C(I)C(Cl)=C1", "4-bromo-2-chloro-1-iodobenzene"),
    ("CC(C)C1=CC=C(C)C=C1O", "5-methyl-2-(propan-2-yl)phenol"), // PubChem: "5-methyl-2-propan-2-ylphenol"
    ("COC1=CC=C(CC=C)C=C1", "1-methoxy-4-(prop-2-en-1-yl)benzene"), // PubChem: "1-methoxy-4-prop-2-enylbenzene"
    ("CCCCC(CC)CO", "2-ethylhexan-1-ol"),
    ("CC(C)CCO", "3-methylbutan-1-ol"),
    ("OCC(O)C(O)C(O)C(O)CO", "hexane-1,2,3,4,5,6-hexol"),
    ("CC(N)C(O)=O", "2-aminopropanoic acid"),
    ("C=CC=CC=C", "hexa-1,3,5-triene"),
    ("CC#CC#CC", "hexa-2,4-diyne"),
    ("CC=CCC#C", "hex-4-en-1-yne"),
    ("CC1=CC=CC=C1O", "2-methylphenol"),
    ("CC1CCCCC1=O", "2-methylcyclohexan-1-one"),
    ("CC(C)C(=O)C(C)C", "2,4-dimethylpentan-3-one"),
    ("C1CCC(=O)C1", "cyclopentanone"),
    ("C=C1CCC(=O)CC1", "4-methylidenecyclohexan-1-one"),
    ("CCCC#N", "butanenitrile"),
    ("NC(=O)C=C", "prop-2-enamide"),
    ("CC(C)(C)N", "2-methylpropan-2-amine"),
    ("CCCCN(CCCC)CCCC", "N,N-dibutylbutan-1-amine"),
    ("OCCOCCO", "2-(2-hydroxyethoxy)ethan-1-ol"), // PubChem: "2-(2-hydroxyethoxy)ethanol"
    ("COCCOC", "1,2-dimethoxyethane"),
    ("CCOC(C)OCC", "1,1-diethoxyethane"),
    ("CC(C)OC(C)C", "2-[(propan-2-yl)oxy]propane"), // PubChem: "2-propan-2-yloxypropane"
    ("OCC1=CC=C(CO)C=C1", "[4-(hydroxymethyl)phenyl]methanol"),
    ("CC(C)(C)C1CCC(=O)CC1", "4-tert-butylcyclohexan-1-one"),
    ("ClC(Cl)=C(Cl)Cl", "tetrachloroethene"), // PubChem: "1,1,2,2-tetrachloroethene"
    ("FC(F)(F)C(F)(F)F", "hexafluoroethane"), // PubChem: "1,1,1,2,2,2-hexafluoroethane"
    ("CC(Cl)(Cl)Cl", "1,1,1-trichloroethane"),
    ("OC(=O)CCCC(=O)C1=CC=CC=C1", "5-oxo-5-phenylpentanoic acid"),
    ("CCCCOC(=O)C1=CC=CC=C1", "butyl benzoate"),
    ("COC(=O)C1CCCCC1", "methyl cyclohexanecarboxylate"),
    ("CCOC(=O)C1=CC=C(N)C=C1", "ethyl 4-aminobenzoate"),
    ("CN(C)C(=O)C1=CC=CC=C1", "N,N-dimethylbenzamide"),
    ("CCN(CC)C(C)=O", "N,N-diethylacetamide"),
    ("O=CC1=CC=CC=C1C=O", "benzene-1,2-dicarbaldehyde"), // PubChem: "phthalaldehyde"
    ("N#CC1=CC=CC(C#N)=C1", "benzene-1,3-dicarbonitrile"),
    ("NC1=CC=CC=C1N", "benzene-1,2-diamine"),
    ("CC(=O)C(C)=O", "butane-2,3-dione"),
    ("CC(O)C(C)=O", "3-hydroxybutan-2-one"),
    ("CCC(=O)CC(O)=O", "3-oxopentanoic acid"),
    ("OC(=O)C(=O)CC(O)=O", "2-oxobutanedioic acid"),
    ("CC(C)(O)C#N", "2-hydroxy-2-methylpropanenitrile"),
    ("CCCC(CC)(CC)C", "3-ethyl-3-methylhexane"),
    ("CC(C)C(C)(C)C(C)C", "2,3,3,4-tetramethylpentane"),
    ("O=C1CCC(=O)CC1", "cyclohexane-1,4-dione"),
    ("OC1CCC(O)CC1", "cyclohexane-1,4-diol"),
    ("NC1CCC(CC1)C(O)=O", "4-aminocyclohexane-1-carboxylic acid"),
    ("O=CC1=CC=C(C=C1)C(O)=O", "4-formylbenzoic acid"),
    ("OC(=O)C1=CC=CC=C1[N+]([O-])=O", "2-nitrobenzoic acid"),
    ("CC(=C)C(O)=O", "2-methylprop-2-enoic acid"),
    ("CC=CC(O)=O", "but-2-enoic acid"),
    ("C=CCCl", "3-chloroprop-1-ene"),
    ("ClC=C(Cl)Cl", "1,1,2-trichloroethene"),
    ("CC(C)CC(C)(C)O", "2,4-dimethylpentan-2-ol"),
    ("CC1(C)CCCCC1", "1,1-dimethylcyclohexane"),
    ("ClC1(Cl)CC1", "1,1-dichlorocyclopropane"),
    ("C1=CC=CC=CC=C1", "cycloocta-1,3,5,7-tetraene"), // PubChem: "cyclooctatetraene"
    ("C1=CCC=CC1", "cyclohexa-1,4-diene"),
    ("C#CC1=CC=CC=C1", "ethynylbenzene"),
    ("CC#CC(O)=O", "but-2-ynoic acid"),
    ("C=C=C", "propa-1,2-diene"),
    ("C=C=CC", "buta-1,2-diene"),
    ("CC(C)C(C)C(C)C", "2,3,4-trimethylpentane"),
    ("CCC(CC)C(CC)CC", "3,4-diethylhexane"),
    ("CC(C)C(CC)C(C)C", "3-ethyl-2,4-dimethylpentane"),
    ("CC(O)CO", "propane-1,2-diol"),
    ("OCC(N)CO", "2-aminopropane-1,3-diol"),
    ("CCCCCCCCCCCCCCCCCCCCC", "henicosane"),
    ("CC(C)(C)C(C)(C)C", "2,2,3,3-tetramethylbutane"),
    ("ClCC(Cl)C(Cl)Cl", "1,1,2,3-tetrachloropropane"),
    ("CC(=O)N(C)C", "N,N-dimethylacetamide"),
    ("O=C(O)C1=CC=CC(C=O)=C1", "3-formylbenzoic acid"),
    ("CCC(C)(C)O", "2-methylbutan-2-ol"),
    ("C=CC(C)(C)O", "2-methylbut-3-en-2-ol"), // PubChem: (no name)
    ("OC(C1=CC=CC=C1)C", "1-phenylethan-1-ol"), // PubChem: "1-phenylethanol"
    ("NC(C)C1=CC=CC=C1", "1-phenylethan-1-amine"), // PubChem: "1-phenylethanamine"
    ("OC(=O)C(O)C1=CC=CC=C1", "hydroxy(phenyl)acetic acid"), // PubChem: "2-hydroxy-2-phenylacetic acid"
    ("COC(=O)C(C)=C", "methyl 2-methylprop-2-enoate"),
    ("CC(C)C(N)C(O)=O", "2-amino-3-methylbutanoic acid"),
    ("CC(C)CC(CC)CC(C)C", "4-ethyl-2,6-dimethylheptane"),
    ("CCC(C)C(CC)CC", "3-ethyl-4-methylhexane"),
    ("CC(Cl)C(C)Br", "2-bromo-3-chlorobutane"),
    ("ClCC(Br)CC", "2-bromo-1-chlorobutane"),
    ("CC(C)(C)CC(C)(C)C", "2,2,4,4-tetramethylpentane"),
    ("CC(C)C(CC(C)C)C(C)C", "2,5-dimethyl-3-(propan-2-yl)hexane"), // PubChem: "2,5-dimethyl-3-propan-2-ylhexane"
    ("C=CC(CC)CC=C", "3-ethylhexa-1,5-diene"),
    ("CC=CC(C=C)CC", "3-ethylhexa-1,4-diene"),
    ("CCC(=C)CC", "3-methylidenepentane"),
    ("C=C(CC)CCC", "3-methylidenehexane"),
    ("CC(C)C1=CC=CC=C1C(C)C", "1,2-di(propan-2-yl)benzene"),
    ("ClCCC1=CC=CC=C1", "(2-chloroethyl)benzene"), // PubChem: "2-chloroethylbenzene"
    ("C1=CC=C(C=C1)CCCC(=O)O", "4-phenylbutanoic acid"),
    (
        "CC(C)(C)C1=CC(C)=CC(C(C)(C)C)=C1O",
        "2,6-di-tert-butyl-4-methylphenol",
    ), // PubChem: "2,6-ditert-butyl-4-methylphenol"
    ("OCC(Cl)(Cl)Cl", "2,2,2-trichloroethan-1-ol"), // PubChem: "2,2,2-trichloroethanol"
    ("OC(C(F)(F)F)C(F)(F)F", "1,1,1,3,3,3-hexafluoropropan-2-ol"),
    ("CC(O)(C)C1=CC=CC=C1", "2-phenylpropan-2-ol"),
    ("NC(=O)C1=CC=C(Cl)C=C1", "4-chlorobenzamide"),
    ("CC(=O)NC(C)C", "N-(propan-2-yl)acetamide"), // PubChem: "N-propan-2-ylacetamide"
    ("CNC(=O)CCl", "2-chloro-N-methylacetamide"),
    ("O=C(N)CC1=CC=CC=C1", "2-phenylacetamide"),
    ("CC(C)(C)OC(=O)C1=CC=CC=C1", "tert-butyl benzoate"),
    ("CCOC(=O)CC1=CC=CC=C1", "ethyl phenylacetate"), // PubChem: "ethyl 2-phenylacetate"
    ("CC(C)OC1=CC=CC=C1", "[(propan-2-yl)oxy]benzene"), // PubChem: "propan-2-yloxybenzene"
    ("C(C1=CC=CC=C1)OC", "(methoxymethyl)benzene"),  // PubChem: "methoxymethylbenzene"
    ("C1=CC=C(C=C1)OCC(O)CO", "3-phenoxypropane-1,2-diol"),
    ("CCCCC(CC)C(=O)O", "2-ethylhexanoic acid"),
    ("CC(C)(C)CC(=O)C", "4,4-dimethylpentan-2-one"),
    ("CC(C=O)CC=O", "2-methylbutanedial"),
    ("N#CCCCC#N", "pentanedinitrile"),
    ("CC(C#N)C#N", "2-methylpropanedinitrile"),
    ("C=CCN", "prop-2-en-1-amine"),
    ("NCC=CCN", "but-2-ene-1,4-diamine"),
    ("C1CCCCCCCCCCC1", "cyclododecane"),
    ("CCCCCCCCCCCCCCCCCC(=O)O", "octadecanoic acid"),
    ("CC(=CC(=O)O)C", "3-methylbut-2-enoic acid"),
    ("OC1=CC=CC=C1C(=O)C", "1-(2-hydroxyphenyl)ethan-1-one"), // PubChem: "1-(2-hydroxyphenyl)ethanone"
    ("CC1=CC=C(C=C1)C(C)=O", "1-(4-methylphenyl)ethan-1-one"), // PubChem: "1-(4-methylphenyl)ethanone"
    ("NCCC1=CC=CC=C1", "2-phenylethan-1-amine"),               // PubChem: "2-phenylethanamine"
    ("CN(C)CC1=CC=CC=C1", "N,N-dimethyl-1-phenylmethanamine"),
    ("OCC(O)=O", "hydroxyacetic acid"), // PubChem: "2-hydroxyacetic acid"
    ("CC(C)(CO)CO", "2,2-dimethylpropane-1,3-diol"),
    ("C=CC(=O)C(C)=C", "2-methylpenta-1,4-dien-3-one"),
    ("CC#CCO", "but-2-yn-1-ol"),
    ("O=C1CCCC1=O", "cyclopentane-1,2-dione"),
    ("CC1CC(C)CC(C)C1", "1,3,5-trimethylcyclohexane"),
    ("CC1=CC=CC(Br)=C1N", "2-bromo-6-methylaniline"),
    ("CC1=C(C(=O)O)C=CC=C1", "2-methylbenzoic acid"),
    ("C=CC=CC(=O)O", "penta-2,4-dienoic acid"),
    ("CCC(C)=C(C)C", "2,3-dimethylpent-2-ene"),
    ("CC(C)=C(C)C", "2,3-dimethylbut-2-ene"),
    ("CC=C=CC", "penta-2,3-diene"),
    ("CC(C)C#CC(C)C", "2,5-dimethylhex-3-yne"),
    ("OCCCCO", "butane-1,4-diol"),
    ("OC(CCl)CCl", "1,3-dichloropropan-2-ol"),
    ("CC(C)(N)CO", "2-amino-2-methylpropan-1-ol"),
    ("CC(CN)O", "1-aminopropan-2-ol"),
    ("NCCCCCCN", "hexane-1,6-diamine"),
    ("NC(=O)CC(N)=O", "propanediamide"),
    ("O=C(O)C=O", "oxoacetic acid"), // PubChem: "oxaldehydic acid"
    ("CCCOC(=O)C", "propyl acetate"),
    ("CCCCOC=O", "butyl formate"),
    ("COC(=O)CC(C)C", "methyl 3-methylbutanoate"),
    ("COC(=O)C1=CC=CC=C1O", "methyl 2-hydroxybenzoate"),
    ("O=COCC1CCCCC1", "cyclohexylmethyl formate"),
    ("CC(=O)OC1CCCCC1", "cyclohexyl acetate"),
    ("CC(=O)OCCC(C)C", "3-methylbutyl acetate"),
    ("CC(C)CC(C)(C)CC(C)C", "2,4,4,6-tetramethylheptane"),
    ("OC(=O)C(Cl)(Cl)Cl", "trichloroacetic acid"), // PubChem: "2,2,2-trichloroacetic acid"
    ("CC(=O)CCC(=O)O", "4-oxopentanoic acid"),
    ("OC(=O)CC(O)=O", "propanedioic acid"),
    ("CC(C)(C)C(=O)C(C)(C)C", "2,2,4,4-tetramethylpentan-3-one"),
    ("CCCC(=O)CCC", "heptan-4-one"),
    ("C(CC(=O)O)CC=O", "5-oxopentanoic acid"),
    ("COCC(O)=O", "methoxyacetic acid"), // PubChem: "2-methoxyacetic acid"
    ("OCCCC(O)=O", "4-hydroxybutanoic acid"),
    ("CC(C)=CCCC(C)=CC=O", "3,7-dimethylocta-2,6-dienal"),
    ("CC(=O)CCC=C(C)C", "6-methylhept-5-en-2-one"),
];

#[test]
fn table_of_names() {
    let mut failures = Vec::new();
    for (smiles, expected) in CASES {
        match name_of(smiles) {
            NameResult::Ok(name) if name == *expected => {}
            other => failures.push(format!("{smiles}: expected {expected:?}, got {other:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} names wrong:\n{}",
        failures.len(),
        CASES.len(),
        failures.join("\n")
    );
}

/// Structures outside the supported classes are refused, never misnamed.
const UNSUPPORTED: &[(&str, &str)] = &[
    ("C1CCC2CCCCC2C1", "more than one ring"),
    ("C1=CC=C2C=CC=CC2=C1", "more than one ring"),
    ("C1=COCC1", "heterocycle"),
    ("O=C1NC=CC=C1", "heterocycle"),
    ("CS", "element S"),
    ("CC(=O)Cl", "acyl halide"),
    ("CC(=O)OC(C)=O", "anhydride"),
    ("CC=NC", "imine"),
    ("CN=NC", "nitrogen group"),
    ("C[NH3+]", "charged"),
    ("CC(=O)[O-]", "charged"),
    ("COC(=O)OC", "carbonate"),
    ("NC(N)=O", "urea"),
    ("CC(=O)OCC(=O)O", "ester together with a carboxylic acid"),
    ("OC(=O)CC(O)(CC(=O)O)C(=O)O", "more than two"),
    ("CCOOCC", "O–O"),
    ("CC(=O)OCCOC(C)=O", "more than one ester"),
    ("CC(=O)NCC(=O)O", "acylamino"),
    ("OC(=O)CCC(N)=O", "carbamoyl on a chain"),
    ("NCCNC", "N,N′"),
    ("O", "no carbon"),
    (
        "CC(=O)OC1=CC=CC=C1C(O)=O",
        "ester together with a carboxylic acid",
    ),
    ("CCOC(=O)CC(=O)OCC", "more than one ester"),
    ("CC1=CC=C(C=C1)S", "element S"),
];

#[test]
fn unsupported_structures_say_why() {
    for (smiles, reason) in UNSUPPORTED {
        match name_of(smiles) {
            NameResult::Unsupported(r) => assert!(
                r.contains(reason),
                "{smiles}: reason {r:?} lacks {reason:?}"
            ),
            other => panic!("{smiles}: expected unsupported ({reason}), got {other:?}"),
        }
    }
}

#[test]
fn several_structures_are_named_each() {
    assert_eq!(name_of("CCO.C"), NameResult::Ok("ethanol; methane".into()));
    let names = iupac_names(&parse("CCO.C1=COCC1").0);
    assert_eq!(names[0], NameResult::Ok("ethanol".into()));
    assert!(matches!(&names[1], NameResult::Unsupported(r) if r.contains("heterocycle")));
    assert!(
        matches!(name_of("CCO.C1=COCC1"), NameResult::Unsupported(r) if r.starts_with("structure 2: "))
    );
    assert!(matches!(
        iupac_name(&Molecule::default()),
        NameResult::Unsupported(_)
    ));
}

/// A drawing's abbreviations are expanded before naming, and drawn hydrogens are counted.
#[test]
fn labels_and_explicit_hydrogens() {
    // Ph–CH2–OH drawn as a "Ph" label on a carbon with an "OH" label.
    let mut m = Molecule::default();
    let ph = m.add_atom("Ph".into(), [0.0, 0.0], 0);
    let c = m.add_atom("C".into(), [1.5, 0.0], 0);
    let oh = m.add_atom("OH".into(), [3.0, 0.0], 0);
    m.add_bond(ph, c, BondOrder::Single);
    m.add_bond(c, oh, BondOrder::Single);
    assert_eq!(iupac_name(&m), NameResult::Ok("phenylmethanol".into()));

    // Ethane with one hydrogen drawn as an atom.
    let mut m = Molecule::default();
    let a = m.add_atom("C".into(), [0.0, 0.0], 0);
    let b = m.add_atom("C".into(), [1.5, 0.0], 0);
    let h = m.add_atom("H".into(), [2.5, 0.0], 0);
    m.add_bond(a, b, BondOrder::Single);
    m.add_bond(b, h, BondOrder::Single);
    assert_eq!(iupac_name(&m), NameResult::Ok("ethane".into()));

    // An unknown label is refused.
    let mut m = Molecule::default();
    let a = m.add_atom("C".into(), [0.0, 0.0], 0);
    let x = m.add_atom("Xyzzy".into(), [1.5, 0.0], 0);
    m.add_bond(a, x, BondOrder::Single);
    assert!(matches!(iupac_name(&m), NameResult::Unsupported(r) if r.contains("Xyzzy")));
}

/// Hand-checked names for features PubChem has no entry to compare with.
#[test]
fn substituent_edge_cases() {
    let cases = [
        // Lowest locants for all prefixes tie; then the first-cited prefix (fluoro) decides.
        (
            "CC(C(F)(F)F)(F)C(F)(F)C(F)(F)F",
            "1,1,1,2,2,3,4,4,4-nonafluoro-3-methylbutane",
        ),
        // A fully fluorinated substituent keeps its free-valence locant.
        (
            "OC(=O)C1=CC=C(C=C1)C(F)(C(F)(F)F)C(F)(F)F",
            "4-(heptafluoropropan-2-yl)benzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(C=C1)C(F)(F)C(F)(F)F",
            "4-(pentafluoroethyl)benzoic acid",
        ),
        // Longest chain first (2013): a methylidene branch rather than a shorter unsaturated chain.
        ("C=C(CC)CCC", "3-methylidenehexane"),
        ("CCC(C)(C)C(C)(C)C", "2,2,3,3-tetramethylpentane"),
        // Nested enclosing marks.
        (
            "OC(=O)C1=CC=C(C=C1)N(CCCl)CCCl",
            "4-[bis(2-chloroethyl)amino]benzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(C=C1)OC(C)C",
            "4-[(propan-2-yl)oxy]benzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(C=C1)C(=O)CCl",
            "4-(chloroacetyl)benzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(C=C1)C(=O)C(C)C",
            "4-(2-methylpropanoyl)benzoic acid",
        ),
        ("OC(=O)C1=CC=C(C=C1)C=O", "4-formylbenzoic acid"),
        ("OC(=O)C1=CC=C(C=C1)CC=O", "4-(2-oxoethyl)benzoic acid"),
        ("CC(C)(C)OC1=CC=CC=C1", "tert-butoxybenzene"),
        (
            "OC(=O)C1CCC(CC1)=C",
            "4-methylidenecyclohexane-1-carboxylic acid",
        ),
        ("C1CCC(=CC)CC1", "ethylidenecyclohexane"),
        ("CCC(C)C1=CC=CC=C1", "(butan-2-yl)benzene"),
        ("CC(C)CC1=CC=C(O)C=C1", "4-(2-methylpropyl)phenol"),
        ("CC(=C)C1=CC=CC=C1", "(prop-1-en-2-yl)benzene"),
    ];
    let mut failures = Vec::new();
    for (smiles, expected) in cases {
        let got = name_of(smiles);
        if got != NameResult::Ok(expected.into()) {
            failures.push(format!("{smiles}: expected {expected:?}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A large, highly branched structure is named without blowing up.
#[test]
fn large_branched_structure() {
    let smiles = format!("C{}", "C(C)".repeat(40));
    let started = std::time::Instant::now();
    let name = name_of(&smiles);
    assert!(
        name.name().is_some_and(|n| n.ends_with("dotetracontane")),
        "{name:?}"
    );
    assert!(
        started.elapsed().as_secs() < 20,
        "took {:?}",
        started.elapsed()
    );
}

/// Ring assemblies (P-28) and rings as substituents of rings (P-44.2, P-44.4.1.1).
#[test]
fn ring_assemblies_and_rings_on_rings() {
    let cases = [
        ("C1=CC=C(C=C1)C1=CC=CC=C1", "1,1'-biphenyl"),
        ("CC1=CC=C(C=C1)C1=CC=CC=C1", "4-methyl-1,1'-biphenyl"),
        ("CC1=CC(=CC=C1)C1=CC=CC=C1", "3-methyl-1,1'-biphenyl"),
        (
            "ClC1=CC=C(C=C1)C1=CC=C(Cl)C=C1",
            "4,4'-dichloro-1,1'-biphenyl",
        ),
        (
            "ClC1=CC=CC=C1C1=CC=C(Cl)C=C1",
            "2,4'-dichloro-1,1'-biphenyl",
        ),
        ("CC1=CC=CC=C1C1=CC=CC=C1C", "2,2'-dimethyl-1,1'-biphenyl"),
        (
            "CC1=CC=C(C=C1)C1=CC=C(Cl)C=C1",
            "4-chloro-4'-methyl-1,1'-biphenyl",
        ),
        (
            "ClC1=C(Cl)C(Cl)=C(C(Cl)=C1Cl)C1=C(Cl)C(Cl)=C(Cl)C(Cl)=C1Cl",
            "2,2',3,3',4,4',5,5',6,6'-decachloro-1,1'-biphenyl",
        ),
        (
            "C1=CC=C(C=C1)C1=CC=C(C=C1)C1=CC=CC=C1",
            "1,1':4',1''-terphenyl",
        ),
        (
            "C1=CC=C(C=C1)C1=CC(=CC=C1)C1=CC=CC=C1",
            "1,1':3',1''-terphenyl",
        ),
        (
            "C1=CC=C(C=C1)C1=CC=CC=C1C1=CC=CC=C1",
            "1,1':2',1''-terphenyl",
        ),
        (
            "C1=CC=C(C=C1)C1=CC=C(C=C1)C1=CC=C(C=C1)C1=CC=CC=C1",
            "1,1':4',1'':4'',1'''-quaterphenyl",
        ),
        (
            "CC1=CC=C(C=C1)C1=CC=C(C=C1)C1=CC=CC=C1",
            "4-methyl-1,1':4',1''-terphenyl",
        ),
        ("C1CCC(CC1)C1CCCCC1", "1,1'-bi(cyclohexane)"),
        ("C1CC1C1CC1", "1,1'-bi(cyclopropane)"),
        ("CC1CCC(CC1)C1CCCCC1", "4-methyl-1,1'-bi(cyclohexane)"),
        (
            "OC(=O)C1=CC=C(C=C1)C1=CC=CC=C1",
            "[1,1'-biphenyl]-4-carboxylic acid",
        ),
        ("OC1=CC=C(C=C1)C1=CC=CC=C1", "[1,1'-biphenyl]-4-ol"),
        ("OC1=CC=C(C=C1)C1=CC=C(O)C=C1", "[1,1'-biphenyl]-4,4'-diol"),
        ("NC1=CC=C(C=C1)C1=CC=CC=C1", "[1,1'-biphenyl]-4-amine"),
        (
            "CNC1=CC=C(C=C1)C1=CC=CC=C1",
            "N-methyl[1,1'-biphenyl]-4-amine",
        ),
        (
            "COC(=O)C1=CC=C(C=C1)C1=CC=CC=C1",
            "methyl [1,1'-biphenyl]-4-carboxylate",
        ),
        (
            "CC1=CC=C(C=C1)C1=CC=C(C=C1)C(O)=O",
            "4'-methyl[1,1'-biphenyl]-4-carboxylic acid",
        ),
        (
            "CC(C(O)=O)C1=CC=C(C=C1)C1=CC=CC=C1",
            "2-([1,1'-biphenyl]-4-yl)propanoic acid",
        ),
        (
            "CC(=O)C1=CC=C(C=C1)C1=CC=CC=C1",
            "1-([1,1'-biphenyl]-4-yl)ethan-1-one",
        ),
        ("O=C(C1=CC=CC=C1)C1=CC=CC=C1", "diphenylmethanone"),
        (
            "OCC1=CC=C(C=C1)C1=CC=CC=C1",
            "([1,1'-biphenyl]-4-yl)methanol",
        ),
        ("C1CCC(CC1)C1=CC=CC=C1", "cyclohexylbenzene"),
        ("C1CCC(C1)C1=CC=CC=C1", "cyclopentylbenzene"),
        ("C1CCCC(CC1)C1=CC=CC=C1", "phenylcycloheptane"),
        ("C1CCC(CC1)C1CCCCCC1", "cyclohexylcycloheptane"),
        ("C1CC1C1CCCCC1", "cyclopropylcyclohexane"),
        ("C1CCC(CC1)=C1CCCCC1", "cyclohexylidenecyclohexane"),
        // The larger ring is senior, whatever its unsaturation.
        ("C1CCC(CC1)C1=CCCC1", "(cyclopent-1-en-1-yl)cyclohexane"),
        ("OC(=O)C1=CC=C(C=C1)C1CCCCC1", "4-cyclohexylbenzoic acid"),
        ("OC1CCC(CC1)C1=CC=CC=C1", "4-phenylcyclohexan-1-ol"),
        ("C(C1=CC=CC=C1)C1=CC=CC=C1", "benzylbenzene"),
        ("O(C1=CC=CC=C1)C1=CC=CC=C1", "phenoxybenzene"),
        (
            "C1=CC=C(C=C1)C1CCCCC1C1=CC=CC=C1",
            "(2-phenylcyclohexyl)benzene",
        ),
        (
            "OC(=O)CCC1=CC=C(C=C1)C1=CC=CC=C1",
            "3-([1,1'-biphenyl]-4-yl)propanoic acid",
        ),
        (
            "C1=CC=C(C=C1)C1=CC=C(C=C1)OC1=CC=CC=C1",
            "4-phenoxy-1,1'-biphenyl",
        ),
    ];
    let mut failures = Vec::new();
    for (smiles, expected) in cases {
        let got = name_of(smiles);
        if got != NameResult::Ok(expected.into()) {
            failures.push(format!("{smiles}: expected {expected:?}, got {got:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Ring combinations outside the supported classes are refused with a reason.
#[test]
fn unsupported_ring_combinations_say_why() {
    let cases = [
        // Suffixes on an assembly of rings other than benzene.
        (
            "OC1CCC(CC1)C1CCCCC1",
            "ring assembly of rings other than benzene",
        ),
        // 1,3,5-triphenylbenzene.
        (
            "C1=CC=C(C=C1)C1=CC(=CC(=C1)C1=CC=CC=C1)C1=CC=CC=C1",
            "branched ring assembly",
        ),
        // Cyclohexenylbenzene: a hydrogenated biphenyl, or substitutive?
        ("C1CCC(=CC1)C1=CC=CC=C1", "partly unsaturated"),
        ("C1CCC(CC1)C1=CCCCC1", "partly unsaturated"),
        // Biphenyl against a larger ring.
        (
            "C1CCCC(CC1)C1=CC=C(C=C1)C1=CC=CC=C1",
            "seniority of a ring assembly",
        ),
        // Bi(cyclohexane) against benzene.
        (
            "C1CCC(CC1)C1CCCCC1C1=CC=CC=C1",
            "seniority of a ring assembly",
        ),
        ("C1CCC2(CC1)CCCC2", "more than one ring"),
    ];
    for (smiles, reason) in cases {
        match name_of(smiles) {
            NameResult::Unsupported(r) => assert!(
                r.contains(reason),
                "{smiles}: reason {r:?} lacks {reason:?}"
            ),
            other => panic!("{smiles}: expected unsupported ({reason}), got {other:?}"),
        }
    }
}

/// Monocyclic heterocycles (P-22.2): retained and Hantzsch–Widman names, their fixed
/// numbering, indicated hydrogen, suffixes, prefixes, seniority over carbocycles (P-44.2.1.2)
/// and ring assemblies of heterocycles (P-28).
const HETEROCYCLES: &[(&str, &str)] = &[
    // Six-membered, fully unsaturated.
    ("C1=CC=NC=C1", "pyridine"),
    ("CC1=CC=CC=N1", "2-methylpyridine"),
    ("CC1=CC=NC=C1", "4-methylpyridine"),
    ("CC1=CC=CC(C)=N1", "2,6-dimethylpyridine"),
    ("OC1=CN=CC=C1", "pyridin-3-ol"),
    ("NC1=CC=CC=N1", "pyridin-2-amine"),
    ("CN(C)C1=CC=NC=C1", "N,N-dimethylpyridin-4-amine"),
    ("OC(=O)C1=CN=CC=C1", "pyridine-3-carboxylic acid"),
    ("NC(=O)C1=CN=CC=C1", "pyridine-3-carboxamide"),
    ("COC(=O)C1=CN=CC=C1", "methyl pyridine-3-carboxylate"),
    ("N#CC1=CC=NC=C1", "pyridine-4-carbonitrile"),
    ("O=CC1=CC=CC=N1", "pyridine-2-carbaldehyde"),
    (
        "OC(=O)C1=CC=CC(=N1)C(O)=O",
        "pyridine-2,6-dicarboxylic acid",
    ),
    (
        "ClC1=CC=C(C=N1)C(O)=O",
        "6-chloropyridine-3-carboxylic acid",
    ),
    ("ClC1=CC=CC=N1", "2-chloropyridine"),
    ("C1=CC=NN=C1", "pyridazine"),
    ("C1=CN=CN=C1", "pyrimidine"),
    ("NC1=NC=CC=N1", "pyrimidin-2-amine"),
    ("C1=CN=CC=N1", "pyrazine"),
    ("CC1=CN=CC=N1", "2-methylpyrazine"),
    ("C1=NC=NC=N1", "1,3,5-triazine"),
    ("NC1=NC=NC=N1", "1,3,5-triazin-2-amine"),
    ("NC1=NC(N)=NC(N)=N1", "1,3,5-triazine-2,4,6-triamine"),
    // Five-membered, fully unsaturated.
    ("C1=COC=C1", "furan"),
    ("OC(=O)C1=CC=CO1", "furan-2-carboxylic acid"),
    ("O=CC1=CC=CO1", "furan-2-carbaldehyde"),
    ("C1=CSC=C1", "thiophene"),
    ("ClC1=CC=CS1", "2-chlorothiophene"),
    ("CC1=CSC=C1", "3-methylthiophene"),
    ("C1=CNC=C1", "1H-pyrrole"),
    ("CN1C=CC=C1", "1-methyl-1H-pyrrole"),
    ("CC1=CC=CN1", "2-methyl-1H-pyrrole"),
    ("OC(=O)C1=CC=CN1", "1H-pyrrole-2-carboxylic acid"),
    ("C1=CN=CN1", "1H-imidazole"),
    ("CN1C=CN=C1", "1-methyl-1H-imidazole"),
    ("CC1=NC=CN1", "2-methyl-1H-imidazole"),
    ("C1=CNN=C1", "1H-pyrazole"),
    ("C1=COC=N1", "1,3-oxazole"),
    ("CC1=NC=CO1", "2-methyl-1,3-oxazole"),
    ("C1=CON=C1", "1,2-oxazole"),
    ("C1=CSC=N1", "1,3-thiazole"),
    ("NC1=NC=CS1", "1,3-thiazol-2-amine"),
    ("C1=CSN=C1", "1,2-thiazole"),
    // Saturated.
    ("C1CO1", "oxirane"),
    ("CC1CO1", "2-methyloxirane"),
    ("C1CN1", "aziridine"),
    ("C1CS1", "thiirane"),
    ("C1COC1", "oxetane"),
    ("C1CNC1", "azetidine"),
    ("C1CCOC1", "oxolane"),
    ("C1CCNC1", "pyrrolidine"),
    ("C1CCSC1", "thiolane"),
    ("C1COCO1", "1,3-dioxolane"),
    ("C1CCOCC1", "oxane"),
    ("OC1CCOCC1", "oxan-4-ol"),
    ("C1CCNCC1", "piperidine"),
    ("CN1CCCCC1", "1-methylpiperidine"),
    ("CC1CCCCN1", "2-methylpiperidine"),
    ("O=C1CCNCC1", "piperidin-4-one"),
    ("CN1CCC(=O)CC1", "1-methylpiperidin-4-one"),
    ("OC(=O)C1CCCCN1", "piperidine-2-carboxylic acid"),
    ("C1CCSCC1", "thiane"),
    ("C1COCCN1", "morpholine"),
    ("CN1CCOCC1", "4-methylmorpholine"),
    ("C1CNCCN1", "piperazine"),
    ("CN1CCNCC1", "1-methylpiperazine"),
    ("C1COCCO1", "1,4-dioxane"),
    ("C1COCOC1", "1,3-dioxane"),
    ("C1CCCNCC1", "azepane"),
    ("C1CCCOCC1", "oxepane"),
    // Heterocycles as substituents.
    ("OCC1CCCO1", "(oxolan-2-yl)methanol"),
    ("OCC1=CC=CC=N1", "(pyridin-2-yl)methanol"),
    ("CC(=O)C1=CC=CC=N1", "1-(pyridin-2-yl)ethan-1-one"),
    ("CC(=O)NC1=CC=NC=C1", "N-(pyridin-4-yl)acetamide"),
    ("CC(=O)OC1=CN=CC=C1", "pyridin-3-yl acetate"),
    (
        "OC(=O)C1=CC=C(C=C1)N1CCCCC1",
        "4-(piperidin-1-yl)benzoic acid",
    ),
    ("OC(=O)C1=CC=C(C=C1)C1=CC=CO1", "4-(furan-2-yl)benzoic acid"),
    (
        "OC(=O)C1=CC=C(C=C1)N1C=CN=C1",
        "4-(1H-imidazol-1-yl)benzoic acid",
    ),
    (
        "OC(=O)C1=CC=C(C=C1)N1CCOCC1",
        "4-(morpholin-4-yl)benzoic acid",
    ),
    (
        "OC(=O)C1=CC=C(C=C1)C1=CC=CS1",
        "4-(thiophen-2-yl)benzoic acid",
    ),
    (
        "COC1=CC=C(C=C1)C1=NC=CS1",
        "2-(4-methoxyphenyl)-1,3-thiazole",
    ),
    ("NC1=CC=C(C=C1)N1CCNCC1", "4-(piperazin-1-yl)aniline"),
    // Seniority: a heterocycle before a carbocycle, nitrogen first, then O before S, then
    // the larger ring, then the less hydrogenated one.
    ("C1=CC=C(C=C1)C1=CC=CC=N1", "2-phenylpyridine"),
    ("C1CCN(CC1)C1=CC=CC=C1", "1-phenylpiperidine"),
    ("C1=CC=C(C=C1)N1CCOCC1", "4-phenylmorpholine"),
    ("C1=CC=C(C=C1)C1=CC=CS1", "2-phenylthiophene"),
    ("C1=CN(C=N1)C1=CC=CC=C1", "1-phenyl-1H-imidazole"),
    ("C1CCC(O1)C1=CC=CC=C1", "2-phenyloxolane"),
    ("C1CCC(CC1)N1CCCCC1", "1-cyclohexylpiperidine"),
    ("C(C1=CC=CC=N1)C1=CC=CC=C1", "2-benzylpyridine"),
    ("C1=CC=C(N=C1)OC1=CC=CC=C1", "2-phenoxypyridine"),
    ("C1CCN(C1)C1=CC=NC=C1", "4-(pyrrolidin-1-yl)pyridine"),
    ("C1CCN(CC1)C1=CC=CC=N1", "2-(piperidin-1-yl)pyridine"),
    ("C1=COC(=C1)C1=CC=CS1", "2-(thiophen-2-yl)furan"),
    ("C1=CC=C(N=C1)C1=CC=CS1", "2-(thiophen-2-yl)pyridine"),
    ("C1=CN=C(N=C1)C1=CC=CC=N1", "2-(pyridin-2-yl)pyrimidine"),
    // Ring assemblies of heterocycles.
    ("C1=CC=NC(=C1)C1=CC=CC=N1", "2,2'-bipyridine"),
    ("C1=CN=CC=C1C1=CC=NC=C1", "4,4'-bipyridine"),
    ("C1=CC(=CN=C1)C1=CC=CC=N1", "2,3'-bipyridine"),
    ("C1=CSC(=C1)C1=CC=CS1", "2,2'-bithiophene"),
    (
        "CC1=CC=NC(=C1)C1=NC=CC(C)=C1",
        "4,4'-dimethyl-2,2'-bipyridine",
    ),
    (
        "C1=CC=NC(=C1)C1=CC=CC(=N1)C1=CC=CC=N1",
        "2,2':6',2''-terpyridine",
    ),
];

#[test]
fn heterocycles() {
    let mut failures = Vec::new();
    for (smiles, expected) in HETEROCYCLES {
        match name_of(smiles) {
            NameResult::Ok(name) if name == *expected => {}
            other => failures.push(format!("{smiles}: expected {expected:?}, got {other:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} names wrong:\n{}",
        failures.len(),
        HETEROCYCLES.len(),
        failures.join("\n")
    );
}

/// Heterocycles outside the supported ones are refused with a reason, never misnamed.
#[test]
fn unsupported_heterocycles_say_why() {
    let cases = [
        // Fused heterocycles: indole, quinoline.
        ("C1=CC=C2NC=CC2=C1", "more than one ring"),
        ("C1=CC=C2N=CC=CC2=C1", "more than one ring"),
        // Partly hydrogenated rings and exocyclic double bonds on mancude rings.
        ("C1=COCC1", "partly hydrogenated"),
        ("C1CC=CNC1", "partly hydrogenated"),
        ("O=C1NC=CC=C1", "partly hydrogenated"),
        ("C1=COCC=C1", "partly hydrogenated"),
        // Indicated hydrogen on carbon.
        ("C1C=CC=N1", "indicated hydrogen on a ring carbon"),
        // Rings not in the table.
        ("C1CSCS1", "not among the supported monocycles"),
        ("C1=CN=NN1", "not among the supported monocycles"),
        ("C1COOC1", "not among the supported monocycles"),
        // Lactones, lactams, amides on a ring nitrogen.
        ("O=C1CCCO1", "lactone or lactam"),
        ("O=C1CCCCN1", "lactone or lactam"),
        ("CC(=O)N1CCCCC1", "amide whose nitrogen is a ring atom"),
        // Ring nitrogen bonded to O or N, charged rings, oxidised sulfur.
        ("ON1CCCC1", "O–N"),
        ("NN1CCCCC1", "hydrazine"),
        ("C1=CC=[N+]([O-])C=C1", "charged"),
        ("O=S1CCCC1", "valence"),
        // A nitrile carbon on a ring nitrogen (a cyanamide).
        ("N#CN1CCCCC1", "cyanamide"),
        ("CN(C)C#N", "cyanamide"),
        // Sulfur outside a ring.
        ("CSC1=CC=CC=N1", "element S"),
        // Assemblies of saturated heterocycles or of rings with indicated hydrogen.
        (
            "C1CCNC(C1)C1CCCCN1",
            "ring assembly of saturated heterocycles",
        ),
        ("C1=CNC(=C1)C1=CC=CN1", "indicated hydrogen"),
        // Suffixes on a heterocyclic assembly.
        (
            "OC(=O)C1=CC=NC(=C1)C1=CC=CC=N1",
            "ring assembly of rings other than benzene",
        ),
    ];
    for (smiles, reason) in cases {
        match name_of(smiles) {
            NameResult::Unsupported(r) => assert!(
                r.contains(reason),
                "{smiles}: reason {r:?} lacks {reason:?}"
            ),
            other => panic!("{smiles}: expected unsupported ({reason}), got {other:?}"),
        }
    }
}
