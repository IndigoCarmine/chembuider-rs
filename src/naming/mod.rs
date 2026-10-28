//! Structure → IUPAC name (substitutive nomenclature, IUPAC 2013 recommendations).
//!
//! [`iupac_name`] names every connected structure of a drawing. The namer covers a deliberately
//! narrow set of classes and refuses everything else with a reason, rather than guess:
//!
//! - acyclic hydrocarbons and hydrocarbons with carbocycles (cycloalkanes, cycloalkenes,
//!   benzene drawn in Kekulé form) that share no atoms, with branching, unsaturation and
//!   ylidene substituents. Of several rings the senior one is the parent (P-44.2, P-44.4.1.1:
//!   the larger ring, then the one with more multiple bonds — cyclohexylbenzene,
//!   phenylcycloheptane), the others are prefixes;
//! - ring assemblies (P-28) of identical rings joined by single bonds, unbranched: benzene
//!   rings (1,1'-biphenyl, 1,1':4',1''-terphenyl, [1,1'-biphenyl]-4-carboxylic acid,
//!   [1,1'-biphenyl]-4-yl) and saturated rings (1,1'-bi(cyclohexane), with prefixes only);
//!   primed locants are written with ASCII apostrophes;
//! - monocyclic heterocycles with N, O and S (see `hetero.rs` for the table), fully
//!   unsaturated in Kekulé form or saturated: pyridine, the diazines, 1,3,5-triazine, furan,
//!   thiophene, 1H-pyrrole, 1H-imidazole, 1H-pyrazole, 1,3- and 1,2-oxazole and -thiazole,
//!   oxirane, oxetane, oxolane, oxane, pyrrolidine, piperidine, piperazine, morpholine,
//!   1,4-dioxane and others. They keep their fixed numbering and cite every locant
//!   (2-methylpyridine, pyridin-3-ol, 1-methyl-1H-pyrrole, piperidin-1-yl). A heterocycle is
//!   senior to a carbocycle (P-44.2.1.2: nitrogen first, then O before S, then ring size,
//!   then the heteroatoms, then hydrogenation): 2-phenylpyridine, 1-phenylpiperidine.
//!   Assemblies of identical fully unsaturated heterocycles take the lowest locants for their
//!   bonds (2,2'-bipyridine, 2,3'-bipyridine, 2,2'-bithiophene), with prefixes only;
//! - the principal characteristic groups, in order of seniority: carboxylic acid > ester >
//!   amide > nitrile > aldehyde > ketone > alcohol (phenol) > amine, as suffixes; the junior
//!   ones and any that cannot sit on the parent as prefixes (carboxy, carbamoyl, cyano, formyl,
//!   oxo, hydroxy, amino, acyl groups such as acetyl and benzoyl);
//! - halogens, nitro groups and ethers (alkoxy prefixes) as prefixes; N-substituted amines
//!   and amides with N locants; esters as "alkyl …oate".
//!
//! Not supported (an [`NameResult::Unsupported`] with the reason): rings sharing atoms (fused,
//! bridged, spiro), branched ring assemblies, suffixes or free valences on assemblies of
//! rings other than benzene, directly bonded rings of one size with partial unsaturation
//! (cyclohexenylbenzene: a hydrogenated biphenyl?), an assembly competing with a larger or
//! less saturated ring for parent, heterocycles outside the table, partly hydrogenated ones
//! (2,3-dihydrofuran) and those with indicated hydrogen on carbon (2H-pyran) or an exocyclic
//! double bond on a fully unsaturated ring (pyridin-2(1H)-one), lactones and lactams, amides
//! on a ring nitrogen (1-acetylpiperidine), assemblies of saturated heterocycles or of those
//! with indicated hydrogen (bipiperidine, bipyrrole), sulfur outside a ring, elements other
//! than C, H, N, O, S and the halogens, charged species other than nitro groups, isotopes, and other
//! functional groups (imines, azo, peroxides, acid halides, anhydrides, carbonates, ureas…).
//! Stereodescriptors (E/Z, R/S) are not generated: the name describes the constitution only;
//! see [`has_stereo_marks`].
//!
//! Choices made where IUPAC 2013 offers more than one name, or where PubChem's names (OpenEye
//! Lexichem) differ:
//!
//! - Preferred prefixes: propan-2-yl (not isopropyl), butan-2-yl, 2-methylpropyl, tert-butyl,
//!   ethenyl (not vinyl), prop-2-en-1-yl (not allyl), benzyl (unsubstituted only).
//! - Retained names used: formic/acetic/benzoic/oxalic acid, formaldehyde, acetaldehyde,
//!   benzaldehyde, formamide, acetamide, benzamide, formonitrile, acetonitrile, benzonitrile,
//!   phenol, aniline, toluene, xylene and anisole (the last three unsubstituted only, P-22.1.3,
//!   P-63.2.3). Substituted acetic acid (and acetate, acetaldehyde, acetonitrile, acetyl) cite
//!   no locant ("chloroacetic acid", P-65.1.1.1); acetamide does ("2-chloroacetamide").
//! - Locants are cited except where P-31.1.4.2.4 omits them (mononuclear and monosubstituted
//!   two-carbon parents, monosubstituted rings, terminal chain suffixes, fully substituted
//!   parents): "propan-2-one", "prop-1-ene", "3-methylcyclohex-1-ene", "2-methylcyclohexan-1-ol".
//! - Enclosing marks follow P-16.5: "4-(propan-2-yl)benzoic acid", "di(propan-2-yl)",
//!   "bis(2-chloroethyl)", "dichloro(fluoro)methane".
//! - Heterocycles take the 2013 preferred names: Hantzsch–Widman names where the older
//!   retained names are no longer preferred (oxolane, not tetrahydrofuran; oxane, not
//!   tetrahydropyran; 1,3-oxazole and 1,2-oxazole, not oxazole and isoxazole; 1,3-thiazole),
//!   and the retained pyrrolidine, piperidine, piperazine and morpholine. On a heterocycle
//!   every locant is cited, a fully substituted one included ("2,3,4,5,6-pentachloropyridine").
//! - Multiplicative names (the preferred names of some symmetrical compounds, e.g.
//!   2,2′-oxydi(ethan-1-ol), 1,1'-methylenedibenzene) and the retained names of α-amino acids
//!   are not produced; the substitutive name given instead ("benzylbenzene",
//!   "phenoxybenzene") is a correct, if not preferred, IUPAC name.

mod engine;
mod graph;
mod hetero;
mod words;

#[cfg(test)]
mod tests;

use crate::config::Config;
use crate::molecule::{BondStereo, Molecule};

/// The outcome of naming.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NameResult {
    /// The IUPAC name (several structures: their names joined by "; ").
    Ok(String),
    /// The structure is outside the supported classes; the reason, in English.
    Unsupported(String),
}

impl NameResult {
    /// The name, if there is one.
    pub fn name(&self) -> Option<&str> {
        match self {
            NameResult::Ok(n) => Some(n),
            NameResult::Unsupported(_) => None,
        }
    }
}

/// The IUPAC name of everything drawn in `mol` (every connected structure, joined by "; "),
/// expanding abbreviations with the built-in fragment set.
pub fn iupac_name(mol: &Molecule) -> NameResult {
    iupac_name_with_config(mol, &Config::default())
}

/// [`iupac_name`] with the abbreviations of `config`.
pub fn iupac_name_with_config(mol: &Molecule, config: &Config) -> NameResult {
    // Brackets (a polymer's repeat unit, a multiple group) change what the atoms stand for;
    // naming the atoms as drawn would name the wrong compound.
    if mol.brackets().next().is_some() {
        return NameResult::Unsupported("brackets (polymers, multiple groups)".into());
    }
    let names = iupac_names_with_config(mol, config);
    if names.is_empty() {
        return NameResult::Unsupported("nothing drawn".into());
    }
    let several = names.len() > 1;
    let mut ok = Vec::new();
    for (i, n) in names.into_iter().enumerate() {
        match n {
            NameResult::Ok(s) => ok.push(s),
            NameResult::Unsupported(r) if several => {
                return NameResult::Unsupported(format!("structure {}: {r}", i + 1));
            }
            unsupported => return unsupported,
        }
    }
    NameResult::Ok(ok.join("; "))
}

/// One name per connected structure of `mol`, in drawing order.
pub fn iupac_names(mol: &Molecule) -> Vec<NameResult> {
    iupac_names_with_config(mol, &Config::default())
}

/// [`iupac_names`] with the abbreviations of `config`.
pub fn iupac_names_with_config(mol: &Molecule, config: &Config) -> Vec<NameResult> {
    graph::components(mol)
        .iter()
        .map(|atoms| match name_component(mol, atoms, config) {
            Ok(n) => NameResult::Ok(n),
            Err(r) => NameResult::Unsupported(r),
        })
        .collect()
}

fn name_component(mol: &Molecule, atoms: &[usize], config: &Config) -> Result<String, String> {
    let g = graph::build(mol, atoms, config)?;
    let p = graph::perceive(&g)?;
    engine::Namer::new(&g, &p).name()
}

/// Whether the drawing has stereo bonds (wedges, hashes, wavy bonds), whose meaning the
/// generated name leaves out.
pub fn has_stereo_marks(mol: &Molecule) -> bool {
    mol.bonds.iter().any(|b| {
        matches!(
            b.stereo,
            BondStereo::WedgeUp
                | BondStereo::WedgeDown
                | BondStereo::Hashed
                | BondStereo::HollowWedge
                | BondStereo::Wavy
        )
    })
}
