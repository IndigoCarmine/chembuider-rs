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
    ("CSC", "sulfide"),
    ("CC=NC", "imine"),
    ("CN=NC", "nitrogen group"),
    ("C[NH3+]", "charged"),
    ("CC(=O)[O-]", "charged"),
    ("COC(=O)OC", "carbonate"),
    ("NC(N)=O", "urea"),
    ("CC(=O)OCC(=O)O", "ester together with a carboxylic acid"),
    ("CCOOCC", "O–O"),
    ("CC(=O)OCCOC(C)=O", "more than one ester"),
    ("OC(=O)CCC(N)=O", "carbamoyl on a chain"),
    ("NCCNC", "N,N′"),
    ("O", "no carbon"),
    (
        "CC(=O)OC1=CC=CC=C1C(O)=O",
        "ester together with a carboxylic acid",
    ),
    ("CCOC(=O)CC(=O)OCC", "more than one ester"),
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
        // Amides on a ring nitrogen (lactones and lactams are in `lactones_and_lactams`).
        ("CC(=O)N1CCCCC1", "amide whose nitrogen is a ring atom"),
        // Ring nitrogen bonded to O or N, charged rings, oxidised sulfur.
        ("ON1CCCC1", "O–N"),
        ("NN1CCCCC1", "hydrazine"),
        ("C1=CC=[N+]([O-])C=C1", "charged"),
        ("O=S1CCCC1", "valence"),
        // A nitrile carbon on a ring nitrogen (a cyanamide).
        ("N#CN1CCCCC1", "cyanamide"),
        ("CN(C)C#N", "cyanamide"),
        // Sulfur outside a ring other than a thiol.
        ("CSC1=CC=CC=N1", "sulfide"),
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

/// Name every structure of `cases` and report all mismatches at once.
fn check_names(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (smiles, expected) in cases {
        match name_of(smiles) {
            NameResult::Ok(name) if name == *expected => {}
            other => failures.push(format!("{smiles}: expected {expected:?}, got {other:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} names wrong:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// Check that every structure of `cases` is refused with a reason containing the given text.
fn check_refused(cases: &[(&str, &str)]) {
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

/// Every example of acyclic compounds and their characteristic groups in Wikipedia (ja)
/// "IUPAC命名法" (oldid 111380645), structure → preferred IUPAC name. Where Wikipedia gives a
/// name that is not the 2013 preferred name, the PIN is expected and Wikipedia's name noted;
/// functional class names and retained names it lists beside the PIN (butyl chloride, methyl
/// alcohol, glycerol, propylene…) are other names, not produced here.
const WIKIPEDIA_ACYCLIC: &[(&str, &str)] = &[
    // CH3–CH2–CH2–CH(C2H5)–CH3: the longest chain (not 2-ethylpentane).
    ("CCCC(CC)C", "3-methylhexane"),
    ("CCC=CC", "pent-2-ene"),
    ("CCCCC#CCC", "oct-3-yne"),
    ("CCC=CC=CC", "hepta-2,4-diene"),
    ("CCCCCl", "1-chlorobutane"),
    ("ClCC(Cl)Cl", "1,1,2-trichloroethane"),
    // Wikipedia: "perbromoethane". "per" is no longer recommended (P-14.3.4.5); all locants
    // are omitted for a fully substituted parent.
    ("BrC(Br)(Br)C(Br)(Br)Br", "hexabromoethane"),
    ("OCC(O)CO", "propane-1,2,3-triol"),
    ("CC(O)C=O", "2-hydroxypropanal"),
    ("COCC", "methoxyethane"),
    // Wikipedia: "methanal"; formaldehyde is the retained PIN (P-66.6.1.1.1).
    ("C=O", "formaldehyde"),
    ("CCC(C)=O", "butan-2-one"),
    ("OC(=O)CCC(O)=O", "butanedioic acid"),
    // Wikipedia: "benzenecarboxylic acid"; benzoic acid is the retained PIN (P-65.1.1.1).
    ("OC(=O)C1=CC=CC=C1", "benzoic acid"),
    (
        "OC(=O)CC(CC(O)=O)C(O)=O",
        "propane-1,2,3-tricarboxylic acid",
    ),
    // Wikipedia: "2-(carboxymethyl)heptanedioic acid" for HOOC–(CH2)2–CH(CH2COOH)–(CH2)2–COOH;
    // the branch is on C-4 of that heptanedioic acid from either end. (IUPAC 2013's example
    // 3-(carboxymethyl)heptanedioic acid is in `polycarboxylic_acids`.)
    (
        "OC(=O)CCC(CC(O)=O)CCC(O)=O",
        "4-(carboxymethyl)heptanedioic acid",
    ),
    ("CCC(=O)[O-].[K+]", "potassium propanoate"),
    ("CCC(=O)OCC", "ethyl propanoate"),
    // Wikipedia: "ethanoyl chloride"; acetyl is the retained preferred prefix (P-65.5.1).
    ("CC(Cl)=O", "acetyl chloride"),
    ("CCC(=O)OC(=O)CC", "propanoic anhydride"),
    // Wikipedia: "ethanoic propanoic anhydride"; acetic acid is the retained PIN (P-65.7.2).
    ("CC(=O)OC(=O)CC", "acetic propanoic anhydride"),
    ("CN", "methanamine"),
    ("CCC(CC)C#N", "2-ethylbutanenitrile"),
    ("CC(S)CCC", "pentane-2-thiol"),
    ("OCC(S)CCC", "2-sulfanylpentan-1-ol"),
    ("C1CCC1", "cyclobutane"),
    // Substituent groups, drawn with attachment points.
    ("*CCCCC", "pentyl"),
    // Wikipedia: "propylene"; propane-1,3-diyl is the preferred prefix (P-29.3.2.2; not
    // trimethylene).
    ("*CCC*", "propane-1,3-diyl"),
    ("*C(=O)CCCCC", "hexanoyl"),
    ("*OCCCCC", "pentyloxy"),
    // Wikipedia: "1-chloro-2-propyl" (to show isopropyl cannot be substituted); the 2013 form
    // cites the locant of the free valence before "yl".
    ("*C(C)CCl", "1-chloropropan-2-yl"),
];

#[test]
fn wikipedia_acyclic_examples() {
    check_names(WIKIPEDIA_ACYCLIC);
}

/// Wikipedia's table of alkanes, each drawn as an unbranched chain, and its example of how
/// the numerical terms combine (2567). Wikipedia writes the C500 alkane "pentacane": IUPAC
/// 2013 Table 1.4 gives "pentacta" for 500, so the name is "pentactane".
#[test]
fn alkane_names_up_to_9999() {
    let table = [
        (1, "methane"),
        (2, "ethane"),
        (3, "propane"),
        (4, "butane"),
        (5, "pentane"),
        (6, "hexane"),
        (7, "heptane"),
        (8, "octane"),
        (9, "nonane"),
        (10, "decane"),
        (11, "undecane"),
        (12, "dodecane"),
        (13, "tridecane"),
        (14, "tetradecane"),
        (15, "pentadecane"),
        (16, "hexadecane"),
        (17, "heptadecane"),
        (18, "octadecane"),
        (19, "nonadecane"),
        (20, "icosane"),
        (21, "henicosane"),
        (22, "docosane"),
        (23, "tricosane"),
        (24, "tetracosane"),
        (25, "pentacosane"),
        (26, "hexacosane"),
        (27, "heptacosane"),
        (28, "octacosane"),
        (29, "nonacosane"),
        (30, "triacontane"),
        (31, "hentriacontane"),
        (32, "dotriacontane"),
        (40, "tetracontane"),
        (50, "pentacontane"),
        (60, "hexacontane"),
        (70, "heptacontane"),
        (80, "octacontane"),
        (90, "nonacontane"),
        (100, "hectane"),
        (101, "henhectane"),
        (111, "undecahectane"),
        (200, "dictane"),
        (300, "trictane"),
        (400, "tetractane"),
        (500, "pentactane"),
        (600, "hexactane"),
        (700, "heptactane"),
        (800, "octactane"),
        (900, "nonactane"),
        (1000, "kiliane"),
        (2000, "diliane"),
        (2567, "heptahexacontapentactadiliane"),
        (3000, "triliane"),
        (4000, "tetraliane"),
        (5000, "pentaliane"),
        (6000, "hexaliane"),
        (7000, "heptaliane"),
        (8000, "octaliane"),
        (9000, "nonaliane"),
        (9999, "nonanonacontanonactanonaliane"),
    ];
    for (n, expected) in table {
        assert_eq!(
            name_of(&"C".repeat(n)),
            NameResult::Ok(expected.into()),
            "C{n}"
        );
    }
    assert!(matches!(
        name_of(&"C".repeat(10000)),
        NameResult::Unsupported(r) if r.contains("longer than 9999")
    ));
}

/// Thiols (P-63.1.5): the suffix "thiol" (the "e" of the parent kept), the prefix "sulfanyl"
/// when an alcohol or a higher group is principal (OH > SH, P-41 class 17; "mercapto" is no
/// longer used). IUPAC 2013 examples are marked.
#[test]
fn thiols() {
    check_names(&[
        ("CS", "methanethiol"),
        ("CCS", "ethanethiol"),
        ("CC(C)S", "propane-2-thiol"),    // P-63.1.5
        ("SCCCCS", "butane-1,4-dithiol"), // P-63.1.5
        ("SC1=CC=CC=C1", "benzenethiol"), // P-63.1.5 (not thiophenol)
        ("CC1=CC=C(S)C=C1", "4-methylbenzene-1-thiol"),
        ("SC1CCCCC1", "cyclohexanethiol"),
        ("SC1=CC=CC=N1", "pyridine-2-thiol"),
        ("OC1=CC=CC=C1S", "2-sulfanylphenol"),     // P-63.1.5
        ("OC(=O)CCS", "3-sulfanylpropanoic acid"), // P-63.1.5
        ("OCCS", "2-sulfanylethan-1-ol"),
        ("NCCS", "2-aminoethane-1-thiol"), // thiol > amine (P-41)
        ("CC(=O)CCS", "4-sulfanylbutan-2-one"),
    ]);
    check_refused(&[
        ("CSC", "sulfide"),
        ("CSC1=CC=CC=C1", "sulfide"),
        ("CSSC", "disulfide"),
        ("CC(C)=S", "C=S"),
        ("CC(=O)S", "thioacid"),
        ("CS(C)=O", "valence"),
    ]);
}

/// Acyl halides (P-65.5.1): the acyl group, then the halide(s) as separate words in
/// alphabetical order. They rank after esters and before amides (P-41): an amide or a
/// nitrile becomes a prefix, an acid or ester makes the halide a prefix, which is refused
/// ("carbonochloridoyl" or "chloro…oxo", P-65.5.4). IUPAC 2013 examples are marked.
#[test]
fn acyl_halides() {
    check_names(&[
        ("CC(F)=O", "acetyl fluoride"),
        ("CC(Cl)=O", "acetyl chloride"), // P-65.5.1
        ("CC(Br)=O", "acetyl bromide"),
        ("CC(I)=O", "acetyl iodide"),
        ("O=CBr", "formyl bromide"), // P-65.5.1
        ("CCC(Cl)=O", "propanoyl chloride"),
        ("CCCCCC(F)=O", "hexanoyl fluoride"), // P-65.5.1
        ("ClC(=O)CC(Cl)=O", "propanedioyl dichloride"), // P-65.5.1
        ("ClC(=O)C(Cl)=O", "oxalyl dichloride"), // P-65.5.1
        ("BrC(=O)CCC(Cl)=O", "butanedioyl bromide chloride"), // P-65.5.1
        (
            "ClC(=O)C1=CC=C(C=C1)C(Cl)=O",
            "benzene-1,4-dicarbonyl dichloride", // P-65.5.1
        ),
        ("ClC(=O)C1=CC=CC=C1", "benzoyl chloride"),
        ("ClC(=O)C1CCCCC1", "cyclohexanecarbonyl chloride"),
        ("ClC(=O)C1=CC=CN=C1", "pyridine-3-carbonyl chloride"),
        ("ClCC(Cl)=O", "chloroacetyl chloride"),
        ("CC(C)C(Cl)=O", "2-methylpropanoyl chloride"),
        ("OCCC(Cl)=O", "3-hydroxypropanoyl chloride"),
        ("N#CCC(Cl)=O", "cyanoacetyl chloride"),
        ("NC(=O)C1=CC=C(C=C1)C(Cl)=O", "4-carbamoylbenzoyl chloride"),
        (
            "ClC(=O)C1=CC=C(C=C1)[N+]([O-])=O",
            "4-nitrobenzoyl chloride",
        ),
    ]);
    check_refused(&[
        ("OC(=O)CCC(Cl)=O", "acid halide group as a prefix"),
        ("COC(=O)CCC(Cl)=O", "acid halide group as a prefix"),
        ("ClC(Cl)=O", "carbonate"),
    ]);
}

/// Acyclic anhydrides (P-65.7.1, P-65.7.2): symmetric ones replace "acid" by "anhydride",
/// substituted or not; mixed ones cite both acids in alphanumerical order. IUPAC 2013
/// examples are marked. Cyclic anhydrides are named as pseudoketones (oxolane-2,5-dione, in
/// `lactones_and_lactams`).
#[test]
fn anhydrides() {
    check_names(&[
        ("CC(=O)OC(C)=O", "acetic anhydride"), // P-65.7.1
        ("CCC(=O)OC(=O)CC", "propanoic anhydride"),
        ("CCCCCC(=O)OC(=O)CCCCC", "hexanoic anhydride"), // P-65.7.1
        ("ClCC(=O)OC(=O)CCl", "chloroacetic anhydride"), // P-65.7.1
        ("O=C(OC(=O)C1=CC=CC=C1)C1=CC=CC=C1", "benzoic anhydride"),
        ("CC(=O)OC(=O)CC", "acetic propanoic anhydride"), // P-65.7.2
        ("CC(=O)OC(=O)CCl", "acetic chloroacetic anhydride"), // P-65.7.2
        ("CC(=O)OC(=O)C1=CC=CC=C1", "acetic benzoic anhydride"),
        ("O=COC(C)=O", "acetic formic anhydride"),
        (
            "CC(O)C(=O)OC(=O)CC",
            "2-hydroxypropanoic propanoic anhydride",
        ),
    ]);
    check_refused(&[
        ("CC(=O)OC(=O)CC(O)=O", "anhydride group as a substituent"),
        ("CC(=O)OC(=O)CCC(=O)OC(C)=O", "more than one anhydride"),
    ]);
}

/// Salts of carboxylic acids (P-65.6.2): the cations in alphabetical order, then the anion.
/// The cations are the alkali metals and ammonium, drawn as separate ions; a partly
/// neutralised acid names its free acid groups as "carboxy" (P-65.6.2.3.1, method 1).
/// IUPAC 2013 examples are marked.
#[test]
fn carboxylate_salts() {
    check_names(&[
        ("CCC(=O)[O-].[K+]", "potassium propanoate"),
        ("[K+].CCCC([O-])=O", "potassium butanoate"), // P-65.6.2.1
        ("[Na+].CC([O-])=O", "sodium acetate"),
        ("[Li+].[O-]C(=O)C1=CC=CC=C1", "lithium benzoate"),
        ("[Rb+].[O-]C=O", "rubidium formate"),
        ("[Cs+].CC([O-])=O", "caesium acetate"),
        ("[NH4+].CC([O-])=O", "ammonium acetate"),
        ("[O-]C(=O)CCC([O-])=O.[Na+].[Na+]", "disodium butanedioate"),
        ("[Na+].[O-]C(=O)C([O-])=O.[Na+]", "disodium oxalate"),
        (
            "[K+].[O-]C(=O)CCC([O-])=O.[Na+]",
            "potassium sodium butanedioate", // P-65.6.2.1
        ),
        (
            "[NH4+].[O-]C(=O)CCCCC([O-])=O.[K+]",
            "ammonium potassium hexanedioate", // P-65.6.2.1
        ),
        ("[NH4+].OC(=O)CCC([O-])=O", "ammonium 3-carboxypropanoate"), // P-65.6.2.3.1
        ("[K+].OC(=O)CCCCCC([O-])=O", "potassium 6-carboxyhexanoate"), // P-65.6.2.3.1
        ("[Na+].[O-]C(=O)C1=CC=CC=C1O", "sodium 2-hydroxybenzoate"),
        ("[Na+].[O-]C(=O)C1CCCCC1", "sodium cyclohexanecarboxylate"),
        (
            "[O-]C(=O)CC(CC([O-])=O)C([O-])=O.[Na+].[Na+].[Na+]",
            "trisodium propane-1,2,3-tricarboxylate",
        ),
        // Two formula units drawn.
        ("[Na+].CC([O-])=O.[Na+].CC([O-])=O", "sodium acetate"),
        // A neutral compound beside the salt keeps its own name.
        ("CCO.[Na+].CC([O-])=O", "ethanol; sodium acetate"),
    ]);
    check_refused(&[
        ("CC([O-])=O", "charges do not balance"),
        ("[Na+]", "charges do not balance"),
        ("[Na+].CC([O-])=O.[Na+]", "charges do not balance"),
        (
            "[Na+].[Na+].CC([O-])=O.[O-]C=O",
            "more than one kind of anion",
        ),
        ("CC(=O)O[Na]", "draw a salt as separate ions"),
        ("C[NH3+].CC([O-])=O", "charged"),
        ("[Ca+2].CC([O-])=O.CC([O-])=O", "element Ca"),
        ("CC(=O)OCC(=O)[O-].[Na+]", "ester group as a substituent"),
    ]);
}

/// More than two acid (nitrile, aldehyde…) groups: on one unbranched chain all are expressed
/// by the "carboxylic acid" kind of suffix (P-65.1.2.2.1); otherwise two end the chain and the
/// others are prefixes (P-65.1.2.2.3). IUPAC 2013 examples are marked.
#[test]
fn polycarboxylic_acids() {
    check_names(&[
        (
            "OC(=O)CC(CC(O)=O)C(O)=O",
            "propane-1,2,3-tricarboxylic acid",
        ),
        (
            "OC(=O)CCC(CCC(O)=O)C(O)=O",
            "pentane-1,3,5-tricarboxylic acid",
        ), // P-65.1.2.2.1
        (
            "OC(=O)C(C(O)=O)C(C(O)=O)C(O)=O",
            "ethane-1,1,2,2-tetracarboxylic acid", // P-65.1.2.2.1
        ),
        (
            "OC(=O)CC(O)(CC(O)=O)C(O)=O",
            "2-hydroxypropane-1,2,3-tricarboxylic acid", // P-65.1.1.2.3 (citric acid)
        ),
        (
            "OC(=O)C(O)C(C(O)=O)C(=O)C(O)=O",
            "1-hydroxy-3-oxopropane-1,2,3-tricarboxylic acid", // P-65.1.2.4
        ),
        ("OC(=O)C(C(O)=O)C(O)=O", "methanetricarboxylic acid"),
        // P-65.1.2.2.3 (the structure is drawn in the source; this is the one its name fits).
        (
            "OC(=O)CC(CC(O)=O)CCCC(O)=O",
            "3-(carboxymethyl)heptanedioic acid",
        ),
        (
            "OC(=O)CCC(CC(O)=O)CCC(O)=O",
            "4-(carboxymethyl)heptanedioic acid",
        ),
        (
            "OC(=O)C(CC(O)=O)CCCCC(O)=O",
            "hexane-1,2,6-tricarboxylic acid",
        ),
        (
            "OC(=O)CC(C(O)=O)CCCC(O)=O",
            "pentane-1,2,5-tricarboxylic acid",
        ),
        ("N#CCC(CC#N)C#N", "propane-1,2,3-tricarbonitrile"),
        ("O=CCC(CC=O)C=O", "propane-1,2,3-tricarbaldehyde"),
        ("OC(=O)CCC(O)=O", "butanedioic acid"),
    ]);
}

/// Substituent groups drawn with attachment points ("*"), named with their free valences
/// (P-29, P-63.2.2): alkyl, acyl and alkoxy groups — the contracted methoxy to butoxy,
/// tert-butoxy and phenoxy, "pentyloxy" and longer ones by concatenation — and divalent
/// groups: methylene and the phenylenes are retained (P-29.6.1), the others are "-diyl"
/// ("propane-1,3-diyl", not trimethylene or propylene; "ethane-1,2-diyl", not ethylene).
#[test]
fn substituent_groups() {
    check_names(&[
        ("*C", "methyl"),
        ("*CCCCC", "pentyl"),
        ("*C(C)C", "propan-2-yl"),
        ("*C(C)(C)C", "tert-butyl"),
        ("*C=C", "ethenyl"),
        ("*CC=C", "prop-2-en-1-yl"),
        ("*=C", "methylidene"),
        ("*C1=CC=CC=C1", "phenyl"),
        ("*CC1=CC=CC=C1", "benzyl"),
        ("*C1CCCCC1", "cyclohexyl"),
        ("*C1=CC=NC=C1", "pyridin-4-yl"),
        ("*C(=O)C", "acetyl"),
        ("*C=O", "formyl"),
        ("*C(=O)CCCCC", "hexanoyl"),
        ("*C(=O)C1=CC=CC=C1", "benzoyl"),
        ("*C(O)=O", "carboxy"),
        ("*C#N", "cyano"),
        ("*NC", "methylamino"),
        ("*OC", "methoxy"),
        ("*OCC", "ethoxy"),
        ("*OCCC", "propoxy"),
        ("*OCCCC", "butoxy"),
        ("*OCCCCC", "pentyloxy"),
        ("*OCCCCCC", "hexyloxy"),
        ("*OC(C)(C)C", "tert-butoxy"),
        ("*OC(C)C", "(propan-2-yl)oxy"),
        ("*OC1=CC=CC=C1", "phenoxy"),
        ("*CCS", "2-sulfanylethyl"),
        ("*C(C)CCl", "1-chloropropan-2-yl"),
        ("*C*", "methylene"),
        ("*C(Br)*", "bromomethylene"),
        ("*CC*", "ethane-1,2-diyl"),
        ("*C(*)C", "ethane-1,1-diyl"),
        ("*CCC*", "propane-1,3-diyl"),
        ("*CC(C)*", "propane-1,2-diyl"),
        ("*C(C)(*)C", "propane-2,2-diyl"),
        ("*CCCC*", "butane-1,4-diyl"),
        ("*CC=CC*", "but-2-ene-1,4-diyl"),
        ("*CC(*)CC*", "butane-1,2,4-triyl"),
        ("*C1=CC=CC=C1*", "1,2-phenylene"),
        ("*C1=CC(*)=CC=C1", "1,3-phenylene"),
        ("*C1=CC=C(*)C=C1", "1,4-phenylene"),
        ("*C1=CC(Cl)=C(*)C=C1", "2-chloro-1,4-phenylene"),
        ("*C1=CC(*)=CC(*)=C1", "benzene-1,3,5-triyl"),
        ("*C1CCC(*)CC1", "cyclohexane-1,4-diyl"),
        ("*C1=CC=C(C=C1)C1=CC=C(*)C=C1", "[1,1'-biphenyl]-4,4'-diyl"),
    ]);
    check_refused(&[
        ("*OC(=O)C", "acyloxy"),
        ("*SC", "free valence on sulfur"),
        (
            "*CCO*",
            "several free valences other than single bonds on carbon",
        ),
        ("*C(=O)OC", "ester group as a substituent"),
        ("*Cl", "no carbon"),
    ]);
}

/// Alkoxy prefixes on whole molecules (P-63.2.2): contracted up to butoxy, "pentyloxy" and
/// longer by concatenation, enclosed as compound prefixes.
#[test]
fn alkoxy_prefixes() {
    check_names(&[
        ("CCCOC1=CC=CC=C1", "propoxybenzene"),
        ("CCCCOC1=CC=CC=C1", "butoxybenzene"),
        ("CCCCCOC1=CC=CC=C1", "(pentyloxy)benzene"),
        ("CCCCCCOC1=CC=CC=C1", "(hexyloxy)benzene"),
        ("CCCCCOCC(O)=O", "(pentyloxy)acetic acid"),
        ("CCCCCOCCO", "2-(pentyloxy)ethan-1-ol"),
        ("CCCCCOCCCCC", "1-(pentyloxy)pentane"),
    ]);
}

/// Substituent groups with several free valences that are not all on one parent chain or
/// ring: IUPAC 2013 names them as compound or multiplied groups ("1,4-phenylenebis(methylene)",
/// "oxydi(ethane-2,1-diyl)", P-29.4.2), which this namer does not produce. Naming the parent
/// alone would drop a free valence and name a different, monovalent group.
#[test]
fn free_valences_off_the_parent_are_refused() {
    let reason = "free valences not all on one parent";
    check_refused(&[
        ("*CC1=CC=C(C*)C=C1", reason), // not (4-methylphenyl)methyl
        ("*C1=CC=C(C=C1)C*", reason),  // not 4-methylphenyl
        ("*CC1=CC=CC=C1*", reason),    // not 2-methylphenyl
        ("*C1CCC(CC*)CC1", reason),    // not 4-ethylcyclohexyl
        ("*CCOCC*", reason),           // not 2-ethoxyethyl
        ("*C1CC1C*", reason),          // not 2-methylcyclopropyl
        ("*CC(C*)C*", reason),         // three free valences, not 2-methylpropane-1,3-diyl
        ("*C(=O)CC1CCC(CC1)C(=O)*", reason),
    ]);
}

/// Divalent acyl groups (P-65.1.7): "carbonyl" (P-65.2.1.5), "oxalyl" (P-65.1.7.2.1), the
/// "-dioyl" groups of chain diacids (P-65.1.7.3.1) and the "-dicarbonyl" groups of rings and
/// of chains with more than two acid groups (P-65.1.7.2.2).
#[test]
fn diacyl_groups() {
    check_names(&[
        ("*C(=O)*", "carbonyl"),
        ("*C(*)=O", "carbonyl"),
        ("*C(=O)C(=O)*", "oxalyl"),
        ("*C(=O)CC(=O)*", "propanedioyl"),
        ("*C(=O)CCC(=O)*", "butanedioyl"),
        ("*C(=O)CC(C)C(=O)*", "2-methylbutanedioyl"),
        ("*C(=O)C1=CC=C(C=C1)C(=O)*", "benzene-1,4-dicarbonyl"),
        ("*C(=O)C1=CC=CC(C(=O)*)=C1", "benzene-1,3-dicarbonyl"),
        ("*C(=O)C1CCC(CC1)C(=O)*", "cyclohexane-1,4-dicarbonyl"),
        ("*C(=O)CC(CC(=O)*)C(=O)*", "propane-1,2,3-tricarbonyl"),
    ]);
    check_refused(&[
        (
            "*C(=O)CC*",
            "free valences both on C=O carbons and on other atoms",
        ),
        (
            "*C(=O)C1=CC=C(C*)C=C1",
            "free valences both on C=O carbons and on other atoms",
        ),
    ]);
}

/// Acyl halides with different halides (P-65.5.1) cite them without locants, so the name fits
/// one structure only when the acyl halide groups are interchangeable; otherwise the namer
/// refuses rather than give two isomers one name.
#[test]
fn mixed_acyl_halides() {
    check_names(&[
        ("BrC(=O)CCC(Cl)=O", "butanedioyl bromide chloride"),
        ("ClC(=O)CCC(Br)=O", "butanedioyl bromide chloride"),
        (
            "BrC(=O)C1=CC=CC(C(Cl)=O)=C1",
            "benzene-1,3-dicarbonyl bromide chloride",
        ),
        (
            "BrC(=O)CC(Cl)CC(Cl)=O",
            "3-chloropentanedioyl bromide chloride",
        ),
        (
            "BrC(=O)C1=CC(=CC(=C1)C(Cl)=O)C(Cl)=O",
            "benzene-1,3,5-tricarbonyl bromide dichloride",
        ),
        (
            "ClC(=O)CC(CC(Cl)=O)C(Cl)=O",
            "propane-1,2,3-tricarbonyl trichloride",
        ),
        // Kekulé rings: the symmetry ignores where the double bonds are drawn.
        (
            "BrC(=O)C1=CC=CC(=N1)C(Cl)=O",
            "pyridine-2,6-dicarbonyl bromide chloride",
        ),
        (
            "BrC(=O)C1=CC=C(C=C1)C1=CC=C(C=C1)C(Cl)=O",
            "[1,1'-biphenyl]-4,4'-dicarbonyl bromide chloride",
        ),
    ]);
    let reason = "different halides on acyl groups that are not interchangeable";
    check_refused(&[
        ("BrC(=O)C(Cl)CC(Cl)=O", reason),
        ("ClC(=O)C(Cl)CC(Br)=O", reason),
        ("BrC(=O)CC(CC(Cl)=O)C(Cl)=O", reason),
        ("ClC(=O)CC(CC(Cl)=O)C(Br)=O", reason),
        ("BrC(=O)C(Cl)CCC(Cl)=O", reason),
        ("BrC(=O)C1=CC=C(C)C(C(Cl)=O)=C1", reason),
        ("BrC(=O)C1=CC(=CC(=C1)C(I)=O)C(Cl)=O", reason),
        ("BrC(=O)C1=CC(=NC=C1)C(Cl)=O", reason),
        ("BrC(=O)C1=CC=C(C=C1)C1=CC=CC(=C1)C(Cl)=O", reason),
    ]);
}

/// A one-carbon acid, nitrile or acyl parent takes no prefix on its carbon: carboxy- and
/// cyanoformic acid are named otherwise (P-65.1.8: hydrogen oxalate, carbonocyanidic acid).
/// The partial salt of an acid with more than two acid groups is "hydrogen …tricarboxylate"
/// (P-65.6.2.3), not produced here.
#[test]
fn substituted_formic_acid_and_hydrogen_salts_are_refused() {
    check_refused(&[
        ("[O-]C(=O)C(=O)O.[Na+]", "one-carbon"),
        ("OC(=O)C#N", "one-carbon"),
        ("*C(=O)C#N", "one-carbon"),
        (
            "OC(=O)CC(CC([O-])=O)C([O-])=O.[K+].[Na+]",
            "partial salt of an acid with more than two acid groups",
        ),
    ]);
}

/// A free valence is a bond: an atom drawn with more bonds and attachment points than its
/// valence allows is refused, not named as a group with fewer free valences.
#[test]
fn free_valences_count_toward_valence() {
    let reason = "more bonds than its valence allows";
    check_refused(&[
        ("*C(*)(=O)C", reason),   // not carbonyl
        ("*C(=O)(C)C", reason),   // not propanoyl
        ("*C(C)(C)(C)C", reason), // not 2,2-dimethylpropan-2-yl
        ("*N(C)(C)C", reason),    // not trimethylamino
        ("*C(*)(*)(*)*", reason), // not methanepentayl
        ("*O(C)C", reason),
        ("*=C(C)(C)C", reason),
        ("*#C(C)C=O", reason),
        ("*S(C)C", reason),
        ("*Cl(C)", reason),
    ]);
    // The oxygens and nitrogen of a nitro group have no room for a free valence either.
    check_refused(&[
        ("C[N+](*)(=O)[O-]", "charged"),
        ("C[N+](=O)[O-]*", "charged"),
        ("C[N+](=O*)[O-]", "charged"),
        ("CN(*)(=O)=O", reason),
        ("CN(=O)=O*", reason),
    ]);
}

/// The symmetry check of mixed acyl halides refines atom colours a bounded number of rounds,
/// not one per pair of chain atoms (a 400-carbon chain would take 200): on long chains it stops as soon as the two acyl halide
/// carbons differ, and otherwise leaves the rest to the exact mapping search. The names are
/// those of the matching diacids, as on short chains.
#[test]
fn mixed_acyl_halides_on_long_chains_refine_few_rounds() {
    use super::graph::{MAX_REFINEMENT_ROUNDS, REFINEMENT_ROUNDS};
    let chain = "C".repeat(400);
    let rounds = |smiles: &str| {
        REFINEMENT_ROUNDS.set(0);
        let result = name_of(smiles);
        (result, REFINEMENT_ROUNDS.get())
    };
    // The methyl tells the two ends apart at once.
    let (result, n) = rounds(&format!("BrC(=O)C(C){chain}C(Cl)=O"));
    match result {
        NameResult::Unsupported(r) => assert!(r.contains("not interchangeable"), "{r}"),
        other => panic!("expected unsupported, got {other:?}"),
    }
    assert!(n <= 2, "{n} rounds");
    // Symmetric: the name of the diacid with the acyl halide ending.
    let NameResult::Ok(acid) = name_of(&format!("OC(=O){chain}C(O)=O")) else {
        panic!("diacid not named");
    };
    let stem = acid.strip_suffix("dioic acid").expect("a dioic acid");
    let (result, n) = rounds(&format!("BrC(=O){chain}C(Cl)=O"));
    assert_eq!(
        result,
        NameResult::Ok(format!("{stem}dioyl bromide chloride"))
    );
    assert!(n <= MAX_REFINEMENT_ROUNDS, "{n} rounds");
}

/// Monospiro compounds of two monocycles (P-24.2.1): numbered from a ring atom next to the
/// spiro atom in the smaller ring, around it, through the spiro atom and around the larger
/// ring; then low locants for heteroatoms (P-24.2.4.1), principal groups, multiple bonds and
/// prefixes. Heteroatoms are cited by 'a' prefixes in the order O, S, N. IUPAC 2013 examples
/// are marked.
#[test]
fn spiro_compounds() {
    check_names(&[
        ("C1CCC2(C1)CCCCC2", "spiro[4.5]decane"), // P-24.2.1
        ("C1CCC2(C1)CCCC2", "spiro[4.4]nonane"),  // P-24.2.1
        ("C1CC12CC2", "spiro[2.2]pentane"),
        ("C1CCC2(CC1)CCCCC2", "spiro[5.5]undecane"),
        ("CC1CCC2(C1)CCCCC2", "2-methylspiro[4.5]decane"),
        ("O=C1CCCC12CCCCC2", "spiro[4.5]decan-1-one"),
        ("O=C1CCC2(CC1)CCCC2", "spiro[4.5]decan-8-one"),
        ("O=C1CCCCC11CCCC1", "spiro[4.5]decan-6-one"),
        (
            "OC(=O)C1CCC2(CCCC2)CC1",
            "spiro[4.5]decane-8-carboxylic acid",
        ),
        ("C1CC2(C=C1)CCCCC2", "spiro[4.5]dec-1-ene"),
        ("C1CCC2(C1)C=CCCC2", "spiro[4.5]dec-6-ene"),
        ("O=C1CCC2(CCCC2)C=C1", "spiro[4.5]dec-6-en-8-one"),
        ("O=C1CCCC12CCCC2", "spiro[4.4]nonan-1-one"),
        ("CC1CCCC2(C1)CCC2", "6-methylspiro[3.5]nonane"),
        ("OCC1CCC2(CC1)CCCC2", "(spiro[4.5]decan-8-yl)methanol"),
        // Heteroatoms (replacement names, P-24.2.4.1).
        ("C1CCC2(C1)CCCCO2", "6-oxaspiro[4.5]decane"), // P-24.2.4.1.1
        ("C1CCC2(C1)NCCOC2", "9-oxa-6-azaspiro[4.5]decane"), // P-24.2.4.1.2
        ("C1CCC2(C1)CSCNC2", "7-thia-9-azaspiro[4.5]decane"), // P-24.2.4.1.2
        ("C1COC2(O1)CCCCC2", "1,4-dioxaspiro[4.5]decane"),
        ("O=C1CCC2(CC1)OCCO2", "1,4-dioxaspiro[4.5]decan-8-one"),
    ]);
}

/// Bicyclic von Baeyer hydrocarbons (P-23.2): bicyclo[x.y.z] with the bridges in decreasing
/// size; numbered from a main bridgehead along the largest bridge to the other bridgehead,
/// back along the second, then the main bridge from the atom next to bridgehead 1 (P-23.2.3).
/// Double bonds between non-consecutive locants take compound locants, as few as possible
/// (P-31.1.4.2). Ortho-fused bicycles with a ring of fewer than five members are named this
/// way (P-52.2.4.1). IUPAC 2013 examples are marked.
#[test]
fn von_baeyer_bicycles() {
    check_names(&[
        ("C12CCCC1CC2", "bicyclo[3.2.0]heptane"),
        ("C1CC2CCC1C2", "bicyclo[2.2.1]heptane"),
        ("C1CC2CCC1CC2", "bicyclo[2.2.2]octane"),
        ("C1CC2CCC(C1)C2", "bicyclo[3.2.1]octane"), // P-23.2.3
        ("C1C2CC12", "bicyclo[1.1.0]butane"),
        ("C1CCC2CC2C1", "bicyclo[4.1.0]heptane"),
        ("C1CCC2CCC2C1", "bicyclo[4.2.0]octane"),
        // Camphor (constitution only).
        (
            "CC1(C)C2CCC1(C)C(=O)C2",
            "1,7,7-trimethylbicyclo[2.2.1]heptan-2-one",
        ),
        ("OC1CC2CCC1C2", "bicyclo[2.2.1]heptan-2-ol"),
        (
            "OC(=O)C1CC2CCC1CC2",
            "bicyclo[2.2.2]octane-2-carboxylic acid",
        ),
        ("CC12CCC(CC1)CC2", "1-methylbicyclo[2.2.2]octane"),
        ("NC12CCC(CC1)CC2", "bicyclo[2.2.2]octan-1-amine"),
        ("C1CC2CC1C=C2", "bicyclo[2.2.1]hept-2-ene"),
        ("C1=CC2C=CC1C2", "bicyclo[2.2.1]hepta-2,5-diene"),
        ("C1CC2CCC1C=C2", "bicyclo[2.2.2]oct-2-ene"),
        // The suffix before the double bond (P-31.1.4.3.4).
        ("OC1CC2CCC1C=C2", "bicyclo[2.2.2]oct-5-en-2-ol"),
        ("OC1C=CC2CC1CC2", "bicyclo[3.2.1]oct-3-en-2-ol"),
        // The main bridge is numbered from the atom next to bridgehead 1: methyls next to the
        // same bridgehead on the two two-atom bridges are 6 and 9 (not 6,8 or 7,8).
        ("C12CCCC(C(C)C1)C(C)C2", "6,9-dimethylbicyclo[3.2.2]nonane"),
        // Compound locants: as few as possible (P-31.1.4.2).
        ("C1CCC2=CCC2C1", "bicyclo[4.2.0]oct-6-ene"), // P-31.1.4.2, not oct-1(8)-ene
        // Both Kekulé forms of the benzene ring name the same compound (P-31.1.4.2, not
        // hepta-1(6),2,4-triene).
        ("C1=CC=C2CC2=C1", "bicyclo[4.1.0]hepta-1,3,5-triene"), // P-31.1.4.2
        ("C1=CC2=C(C2)C=C1", "bicyclo[4.1.0]hepta-1,3,5-triene"),
        ("C1=CC=C2CCC2=C1", "bicyclo[4.2.0]octa-1,3,5-triene"),
        // As substituents.
        ("OCC1CC2CCC1C2", "(bicyclo[2.2.1]heptan-2-yl)methanol"),
        (
            "OC(=O)C1=CC=C(C=C1)C12CCC(CC1)CC2",
            "4-(bicyclo[2.2.2]octan-1-yl)benzoic acid",
        ),
        // More rings before more ring atoms (P-44.2.1.5): the bicycle is the parent.
        (
            "C(C1CCCCCCC1)C1CC2CCC1C2",
            "2-(cyclooctylmethyl)bicyclo[2.2.1]heptane",
        ),
    ]);
}

/// Heteroatoms in von Baeyer systems by skeletal replacement (P-23.3): low locants for the
/// heteroatoms together, then in the order O > S > N (P-23.3.2); the prefixes are cited in
/// that order. IUPAC 2013 examples are marked.
#[test]
fn replacement_names() {
    check_names(&[
        ("N12CCSC1CC2", "4-thia-1-azabicyclo[3.2.0]heptane"),
        ("C1CCC2OC2C1", "7-oxabicyclo[4.1.0]heptane"),
        ("C1CC2CCC1O2", "7-oxabicyclo[2.2.1]heptane"),
        ("C1CN2CCC1CC2", "1-azabicyclo[2.2.2]octane"),
        ("OC1CN2CCC1CC2", "1-azabicyclo[2.2.2]octan-3-ol"),
        ("CN1C2CCC1CC(O)C2", "8-methyl-8-azabicyclo[3.2.1]octan-3-ol"),
        ("C12COCC(OC1)CO2", "3,6,8-trioxabicyclo[3.2.2]nonane"), // P-23.3.2.1
        ("C12OCSC(CC1)C2", "2-oxa-4-thiabicyclo[3.2.1]octane"),  // P-23.3.2.2
        // The cephalosporin skeleton.
        (
            "C1C=CN2C(S1)CC2=O",
            "5-thia-1-azabicyclo[4.2.0]oct-2-en-8-one",
        ),
    ]);
}

/// Fully conjugated monocyclic hydrocarbons: the PIN is the cycloalkapolyene name (P-22.1.2);
/// "[n]annulene" is a name for general nomenclature only (OPE-24).
#[test]
fn annulenes() {
    check_names(&[
        ("C1=CC=CC=CC=CC=C1", "cyclodeca-1,3,5,7,9-pentaene"),
        ("C1=CC=CC=CC=CC=CC=C1", "cyclododeca-1,3,5,7,9,11-hexaene"), // P-22.1.2
        (
            "C1=CC=CC=CC=CC=CC=CC=C1",
            "cyclotetradeca-1,3,5,7,9,11,13-heptaene",
        ),
        (
            "C1=CC=CC=CC=CC=CC=CC=CC=CC=C1",
            "cyclooctadeca-1,3,5,7,9,11,13,15,17-nonaene",
        ),
    ]);
}

/// Lactones, lactams, cyclic anhydrides and imides are heterocyclic pseudoketones (P-64.3.1,
/// P-65.6.3.5.1, P-66.1.5.1): "oxolan-2-one", not butano-4-lactone. Pseudoketones and ketones
/// have the same seniority (P-64.7.2). Names such as "6-hexanelactone" are other names
/// (OPE-24). Only saturated monocycles are supported: on a fully unsaturated ring the C=O
/// needs indicated hydrogen (pyridin-2(1H)-one, refused). IUPAC 2013 examples are marked.
#[test]
fn lactones_and_lactams() {
    check_names(&[
        ("O=C1CCO1", "oxetan-2-one"),
        ("O=C1CCN1", "azetidin-2-one"),
        ("O=C1CCCO1", "oxolan-2-one"),     // P-65.6.3.5.1
        ("O=C1CCCN1", "pyrrolidin-2-one"), // P-64.3.1
        ("O=C1CCCCO1", "oxan-2-one"),
        ("O=C1CCCCN1", "piperidin-2-one"),
        ("O=C1CCCCCO1", "oxepan-2-one"),
        ("O=C1CCCCCN1", "azepan-2-one"), // P-64.3.1
        ("O=C1CCCCCCN1", "azocan-2-one"),
        ("O=C1CCCS1", "thiolan-2-one"),
        ("CC1CCC(=O)O1", "5-methyloxolan-2-one"),
        ("CN1CCCC1=O", "1-methylpyrrolidin-2-one"),
        ("O=C1COCCN1", "morpholin-3-one"),
        ("O=C1CNC(=O)CN1", "piperazine-2,5-dione"),
        ("O=C1OCCO1", "1,3-dioxolan-2-one"),
        // A cyclic anhydride and a cyclic imide.
        ("O=C1CCC(=O)O1", "oxolane-2,5-dione"),
        ("O=C1CCC(=O)N1", "pyrrolidine-2,5-dione"),
        // Below a carboxylic acid, the C=O is an oxo prefix.
        ("OC(=O)C1CCCNC1=O", "2-oxopiperidine-3-carboxylic acid"),
        // A heterocycle is senior to a carbocycle (P-64.7.2).
        ("O=C1CCC(CC1)C1CC(=O)OC1", "4-(4-oxocyclohexyl)oxolan-2-one"),
    ]);
}

/// Acylamino substituents (P-66.1.1.4.3): the amide name with its final "e" changed to "o"
/// gives the preferred prefix ("acetamido", not "acetylamino"). IUPAC 2013 examples are
/// marked.
#[test]
fn acylamino_prefixes() {
    check_names(&[
        ("OC(=O)C1=CC=C(NC=O)C=C1", "4-formamidobenzoic acid"), // P-66.1.1.4.3
        ("OC(=O)C1=CC=C(NC(C)=O)C=C1", "4-acetamidobenzoic acid"),
        ("CC(=O)NCC(O)=O", "acetamidoacetic acid"),
        ("CCC(=O)NCCC(O)=O", "3-propanamidopropanoic acid"),
        (
            "OC(=O)C1=CC=C(N(C)C(C)=O)C=C1",
            "4-(N-methylacetamido)benzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(NC(=O)C2=CC=CC=C2)C=C1",
            "4-benzamidobenzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(NC(=O)CC2=CC=CC=C2)C=C1",
            "4-(2-phenylacetamido)benzoic acid",
        ),
    ]);
}

/// A prefix that contains the name of a parent hydride is enclosed even when it is simple
/// (P-16.5.1.4): "4-(cyclohexanecarbonyl)benzene-1-carbothioic acid (PIN)", "3-(methane-
/// sulfonamido)propanoic acid (PIN)". The acyl and acylamino prefixes of rings end in
/// "carbonyl" and "carboxamido" after the ring's whole name; those of chains ("acetyl",
/// "propanamido", "4,4'-butanediamidodibenzoic acid (PIN)") and the retained "benzoyl" and
/// "benzamido" are not built on a parent hydride name and stay unenclosed.
#[test]
fn prefixes_with_a_parent_hydride_name_are_enclosed() {
    check_names(&[
        (
            "OC(=O)C1=CC=C(NC(=O)C2CCCCC2)C=C1",
            "4-(cyclohexanecarboxamido)benzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(NC(=O)C2CC2)C=C1",
            "4-(cyclopropanecarboxamido)benzoic acid",
        ),
        (
            "OC(=O)CCNC(=O)C1CCCCC1",
            "3-(cyclohexanecarboxamido)propanoic acid",
        ),
        (
            "OC(=O)C1=CC=C(C=C1)C(=O)C1CCCCC1",
            "4-(cyclohexanecarbonyl)benzoic acid",
        ),
        // Built on a chain or retained: unenclosed.
        ("OC(=O)C1=CC=C(C=C1)C(=O)CC", "4-propanoylbenzoic acid"),
        ("CCC(=O)NCCC(O)=O", "3-propanamidopropanoic acid"),
        (
            "OC(=O)C1=CC=C(C=C1)C(=O)C1=CC=CC=C1",
            "4-benzoylbenzoic acid",
        ),
        (
            "OC(=O)C1=CC=C(NC(=O)C2=CC=CC=C2)C=C1",
            "4-benzamidobenzoic acid",
        ),
    ]);
}

/// The ring examples of Wikipedia (ja) "IUPAC命名法" (oldid 111380645), structure → preferred
/// IUPAC name, with the Penicillin G walk-through (constitution only: SMILES read by the test
/// parser carry no stereo; the full name, "(2S,5R,6R)-…", is `stereo_tests::penicillin_g_full_name`).
/// Where Wikipedia's name is not the 2013 PIN, the PIN is expected and the difference noted.
#[test]
fn wikipedia_ring_examples() {
    check_names(&[
        ("C1CCC2(C1)CCCCC2", "spiro[4.5]decane"),
        // Wikipedia: "[14]annulene"; the PIN is the cycloalkapolyene name (P-22.1.2).
        (
            "C1=CC=CC=CC=CC=CC=CC=C1",
            "cyclotetradeca-1,3,5,7,9,11,13-heptaene",
        ),
        // Wikipedia: "6-hexanelactone" (ε-caprolactone) and "oxepane-2-one"; the PIN is the
        // pseudoketone name, the "e" elided before "one" (P-65.6.3.5.1).
        ("O=C1CCCCCO1", "oxepan-2-one"),
        // Wikipedia: "6-hexanelactam" (ε-caprolactam) and "azocan-2-one". The formula given,
        // –NH–CO–(CH2)5– in a ring, is a seven-membered ring: azepan-2-one (P-64.3.1); azocane
        // is the eight-membered ring.
        ("O=C1CCCCCN1", "azepan-2-one"),
        // Penicillin G, step 1: the parent hydrocarbon.
        ("C12CCCC1CC2", "bicyclo[3.2.0]heptane"),
        // Step 2: thia and aza by replacement, N lowest (P-23.3.2).
        ("N12CCSC1CC2", "4-thia-1-azabicyclo[3.2.0]heptane"),
        // Step 3: 6-aminopenicillanic acid.
        (
            "CC1(C)SC2C(N)C(=O)N2C1C(O)=O",
            "6-amino-3,3-dimethyl-7-oxo-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid",
        ),
        // Step 5, without stereodescriptors. Wikipedia: "6-phenylacetylamino-2,2-dimethyl-
        // 7-oxo-…". Its step 3 puts the two methyl groups on C-3 (C-2 carries the carboxy
        // group): 3,3-dimethyl. The acylamino prefix is the amide name ending in "amido"
        // (P-66.1.1.4.3: "Method (1) generates preferred IUPAC names", "4-formamidobenzoic
        // acid (PIN)"), with the locant of 2-phenylacetamide; the prefixes are in
        // alphanumerical order.
        (
            "CC1(C)SC2C(NC(=O)CC3=CC=CC=C3)C(=O)N2C1C(O)=O",
            "3,3-dimethyl-7-oxo-6-(2-phenylacetamido)-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid",
        ),
    ]);
    check_refused(&[
        // DBU (1,8-diazabicyclo[5.4.0]undec-7-ene): two fused rings of five or more members
        // take a fusion name (P-52.2.4.1), which is not supported.
        ("C1CCC2=NCCCN2CC1", "fusion nomenclature"),
    ]);
}

/// Ring systems outside the supported ones are refused with a reason, never misnamed.
#[test]
fn unsupported_ring_systems_say_why() {
    check_refused(&[
        // Ortho-fused rings of five or more members: fusion names (decahydronaphthalene,
        // octahydropentalene, indole).
        ("C1CCC2CCCCC2C1", "fusion nomenclature"),
        ("C1CC2CCCC2C1", "fusion nomenclature"),
        ("C1=CC=C2NC=CC2=C1", "fusion nomenclature"),
        // More than two rings: adamantane, a dispiro compound.
        ("C1C2CC3CC1CC(C2)C3", "more than two rings"),
        ("C1CC11CCC2(CC1)CC2", "more than two rings"),
        // An assembly of identical bicycles.
        (
            "C1CC2CC1CC2C1CC2CCC1C2",
            "ring assembly of bicyclic or spiro",
        ),
        // A spiro atom other than carbon would be charged (spiro ammonium): refused as such.
        ("C1CC[N+]2(C1)CCCC2", "charged"),
        // A benzene ring bridged at its 1,4-positions by a chain ([n]paracyclophanes): the
        // von Baeyer name "bicyclo[10.2.2]hexadeca-1(14),12,15-triene" (P-31.1.4.2) and the
        // phane name "1(1,4)-benzenacyclotridecaphane" (P-52.2.5.1(1)) compete, and the 2013
        // recommendations label the phane name the PIN for such analogues.
        ("C1CCCCCCCC2=CC=C(CC1)C=C2", "paracyclophane"),
        ("C1CCCCCCCCCC2=CC=C1C=C2", "paracyclophane"),
        ("C1CCCCCCCCCC2=CC=C1C=C2C", "paracyclophane"),
    ]);
}

/// A benzene ring bridged at its 1,3-positions keeps its von Baeyer name, the PIN by
/// P-52.2.5.2.1: "bicyclo[16.3.1]docosa-1(22),18,20-triene".
#[test]
fn meta_bridged_benzene_keeps_its_von_baeyer_name() {
    check_names(&[
        (
            "C1CCCCCCCCCCCCCCCC2=CC=CC1=C2",
            "bicyclo[16.3.1]docosa-1(22),18,20-triene",
        ),
        // Two-atom bridges whose six-membered cycles are not mancude (barrelene): not refused.
        ("C1=CC2C=CC1C=C2", "bicyclo[2.2.2]octa-2,5,7-triene"),
    ]);
}
