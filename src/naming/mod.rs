//! Structure → IUPAC name (substitutive nomenclature, IUPAC 2013 recommendations; for inorganic
//! compounds compositional nomenclature, IUPAC 2005).
//!
//! [`iupac_name`] names every connected structure of a drawing. The namer covers a deliberately
//! narrow set of classes and refuses everything else with a reason, rather than guess:
//!
//! - acyclic hydrocarbons (chains of up to 9999 carbons: "henicosane", "hectane",
//!   "pentactane", "kiliane", P-14.2.1) and hydrocarbons with carbocycles (cycloalkanes,
//!   cycloalkenes, benzene drawn in Kekulé form) that share no atoms, with branching,
//!   unsaturation and ylidene substituents. Of several rings the senior one is the parent (P-44.2, P-44.4.1.1:
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
//! - ring systems of two rings sharing atoms, by von Baeyer nomenclature: bicycles
//!   ("bicyclo[2.2.1]heptane", "bicyclo[3.2.0]heptane", P-23.2) and monospiro systems of two
//!   monocycles ("spiro[4.5]decane", P-24.2.1), with their fixed numbering (P-23.2.3: the main
//!   bridge from the atom next to bridgehead 1; P-24.2.1: from the smaller ring next to the
//!   spiro atom), heteroatoms by replacement prefixes in the order O, S, N with the lowest
//!   locants (P-23.3.2, P-24.2.4.1: "4-thia-1-azabicyclo[3.2.0]heptane",
//!   "1,4-dioxaspiro[4.5]decane"), double bonds with compound locants where the two ends are not
//!   consecutive, as few as possible, over every Kekulé form of the system ("bicyclo[4.2.0]oct-
//!   1(6)-ene", "bicyclo[4.1.0]hepta-1,3,5-triene", P-31.1.4.2), and their suffixes, prefixes and
//!   free valences ("1,7,7-trimethylbicyclo[2.2.1]heptan-2-one", "spiro[4.5]decan-8-yl"). A
//!   ring system with more rings is senior to one with more ring atoms (P-44.2.1.5);
//! - lactones, lactams, cyclic anhydrides and imides as heterocyclic pseudoketones, with the
//!   seniority of ketones (P-64.3.1, P-64.7.2): "oxepan-2-one", "azepan-2-one",
//!   "oxolane-2,5-dione", "pyrrolidine-2,5-dione", "4-thia-1-azabicyclo[3.2.0]heptan-7-one";
//! - the principal characteristic groups, in order of seniority (P-41): carboxylate anion >
//!   carboxylic acid > anhydride > ester > acid halide > amide > nitrile > aldehyde > ketone >
//!   alcohol (phenol) > thiol > amine, as suffixes; the junior ones and any that cannot sit on
//!   the parent as prefixes (carboxy, carbamoyl, cyano, formyl, oxo, hydroxy, sulfanyl, amino,
//!   acyl groups such as acetyl and benzoyl). More than two acid (nitrile, aldehyde…) groups
//!   on one unbranched chain are all suffixes, the chain without their carbons
//!   (P-65.1.2.2.1: propane-1,2,3-tricarboxylic acid); otherwise two end the chain and the
//!   others are prefixes (3-(carboxymethyl)heptanedioic acid);
//! - halogens, nitro groups and ethers (alkoxy prefixes) as prefixes; N-substituted amines
//!   and amides with N locants; thiols ("pentane-2-thiol", "2-sulfanylpentan-1-ol"); amides
//!   bonded through their nitrogen as acylamino prefixes, the amide's name ending in "amido"
//!   ("acetamido", "4-(N-methylacetamido)benzoic acid", "6-(2-phenylacetamido)…",
//!   P-66.1.1.4.3);
//! - functional class names: esters ("ethyl propanoate"), acyl halides with F, Cl, Br, I
//!   ("acetyl chloride", "butanedioyl bromide chloride", P-65.5.1; different halides only
//!   where the acyl halide groups are interchangeable by symmetry), acyclic anhydrides
//!   ("propanoic anhydride", "acetic propanoic anhydride", P-65.7);
//! - salts of carboxylic acids drawn as separate ions (P-65.6.2): the cations of `ions.rs` in
//!   alphabetical order, then the carboxylate anion ("potassium propanoate", "disodium
//!   butanedioate", "calcium diacetate", "ammonium 3-carboxypropanoate"). The ions of a drawing
//!   are named together as one salt;
//! - inorganic compounds by compositional nomenclature (IUPAC 2005, the Red Book;
//!   `inorganic.rs`): elements, with the multiplying prefix of the atoms of the molecule
//!   ("trioxygen", "dinitrogen", "octasulfur"; one atom the bare name, "argon", "iron";
//!   IR-3.4.3, IR-5.2), the English name of every element; and salts (IR-5.4) of the cations of one common charge —
//!   the alkali and alkaline-earth metals, aluminium, zinc, cadmium, ammonium — with one kind of
//!   anion of the tables: monoatomic ("chloride", "hydride", "oxide", "nitride"), hydroxide,
//!   cyanide, peroxide, azide, and the oxoanions of Table IR-8.1 with the hydrogen names of
//!   IR-8.5 ("sulfate", "hydrogencarbonate", "dihydrogenphosphate", "perchlorate"…). The
//!   cations are cited in alphabetical order, every proportion with a multiplying prefix,
//!   oxoanions with "bis", "tris" (IR-5.4.2.1): "sodium chloride", "calcium dichloride",
//!   "potassium sodium sulfate", "sodium hydrogencarbonate", "tricalcium bis(phosphate)". A salt
//!   may be drawn as ions or with the metal bonded to its anions ("Na–Cl", "Cl–Ca–Cl", "Na–H"):
//!   each metal has one charge, the number of its single bonds, so the structure is that salt
//!   (a compositional name says nothing of the bonding, IR-5.2);
//! - substituent groups drawn with attachment points ("*", P-29): "pentyl", "pentyloxy",
//!   "hexanoyl", "1-chloropropan-2-yl", and with several free valences on carbon, all on the
//!   parent, "propane-1,3-diyl", "methylene", "1,4-phenylene"; or all on C=O carbons,
//!   "carbonyl", "oxalyl", "butanedioyl", "benzene-1,4-dicarbonyl" (P-65.1.7).
//!
//! Not supported (an [`NameResult::Unsupported`] with the reason): ring systems of more than
//! two rings (tricyclo…, dispiro…, adamantane), two ortho-fused rings of five or more members
//! (they take fusion names: decahydronaphthalene, indole, P-52.2.4.1), a benzene (or other
//! mancude six-membered) ring bridged at its 1,4-positions ([n]paracyclophanes, whose PIN is a
//! phane name, P-52.2.5.1; the 1,3-bridged ring keeps its von Baeyer name), assemblies of
//! bicyclic or spiro systems, the choice of parent between different von Baeyer systems of equal rank,
//! branched ring assemblies, suffixes or free valences on assemblies of
//! rings other than benzene, directly bonded rings of one size with partial unsaturation
//! (cyclohexenylbenzene: a hydrogenated biphenyl?), an assembly competing with a larger or
//! less saturated ring for parent, heterocycles outside the table, partly hydrogenated ones
//! (2,3-dihydrofuran) and those with indicated hydrogen on carbon (2H-pyran) or an exocyclic
//! double bond on a fully unsaturated ring (pyridin-2(1H)-one, so lactones and lactams only on
//! saturated monocycles of the table or in von Baeyer systems), monocycles of more than eight
//! members with heteroatoms (1-oxacyclododecan-2-one), amides on a ring nitrogen
//! (1-acetylpiperidine), assemblies of saturated heterocycles or of those
//! with indicated hydrogen (bipiperidine, bipyrrole), sulfur outside a ring other than in a
//! thiol (sulfides, disulfides, thiones, thioacids, sulfoxides…), elements other than C, H, N,
//! O, S and the halogens in organic structures, charged species other than nitro groups,
//! carboxylate anions and the ions above (substituted ammonium, zwitterions, metals bonded to
//! an organic structure — CH₃–Li, CH₃–Mg–Br may be organometallic, an organic salt is to be
//! drawn as ions —, salts whose charges do not balance or with two kinds of anion), isotopes,
//! acid halide, anhydride and carboxylate groups as prefixes, cyclic anhydrides, partial salts
//! of acids with more than two acid groups ("hydrogen …tricarboxylate"), prefixes on a
//! one-carbon acid parent (carboxyformic acid, P-65.1.8), different halides on acyl halide
//! groups that are not interchangeable (no locants for them), free valences on sulfur or an
//! acid's oxygen (acyloxy), several free valences other than single bonds on carbon or not
//! all on one parent ("1,4-phenylenebis(methylene)", P-29.4.2) or both on C=O carbons and
//! others, and other functional groups (imines, azo, peroxides, carbonates, ureas…).
//!
//! Not supported among inorganic compounds, each for its reason:
//!
//! - salts of the transition metals (other than zinc and cadmium) and of the other metals of
//!   more than one common oxidation state (tin, lead, thallium, mercury, the lanthanoids and
//!   actinoids…), even where the drawn charge fixes it: their names need an oxidation number or
//!   charge number ("iron(II) chloride", "iron(2+) chloride", IR-5.4.2.2), which the Red Book
//!   omits only "when there is no ambiguity" — a judgement the namer does not make;
//! - coordination compounds and complexes (IR-9: ligand names, additive nomenclature);
//! - hydrates and other addition compounds (IR-5.5: "·5H₂O");
//! - ions outside the tables (amide NH₂⁻, diphosphate, the hydrogen names that IR-8.5 does not
//!   accept in short form, hydron H⁺, oxonium), and molecular compounds other than elements:
//!   hydrides (water, ammonia, hydrogen chloride: the parent hydrides of IR-6), oxoacids
//!   (sulfuric acid) and binary compounds of nonmetals (SF₆, SO₂), whose names need the element
//!   sequence of Table VI; carbon dioxide and carbon monoxide among them;
//! - a cation-table metal with another charge or other bonds than its ion has ("Na²⁺",
//!   "Cl–Na–Cl", "Na=O", "Na–Na"), and a lone neutral hydrogen atom (by its valence H₂).
//!
//! Stereodescriptors (P-9, `stereo.rs`, ranked by the CIP rules in `cip.rs`) are cited in front
//! of the name, with their locants, in ascending order of the locants whatever their kind
//! (P-93): "(2R)-butan-2-ol", "(2E)-pent-2-ene", "(3S,4E)-hex-4-en-3-ol", "(2S,5R,6R)-3,3-
//! dimethyl-7-oxo-6-(2-phenylacetamido)-4-thia-1-azabicyclo[3.2.0]heptane-2-carboxylic acid";
//! of an ester before its acid part ("ethyl (2R)-…oate"). Where the name cites no locant for
//! the unit, its descriptor has none (P-93.4.1.1): "(R)-bromo(chloro)(fluoro)methane",
//! "(E)-1,2-dichloroethene", "(Z)-cyclooctene". R/S comes from the wedges and hashes
//! drawn at a tetrahedral carbon centre, E/Z from the drawn geometry of a C=C double bond (not
//! in a ring of fewer than eight members, P-93; nor the Z double bonds of an eight-membered
//! ring with more than one, P-91.2.2: "cycloocta-1,3,5,7-tetraene"). Of numberings that tie on
//! everything else, the lower locants go to Z, then to R (P-14.4 (j): meso "(2R,3S)"). The CIP
//! ranking is Rule 1a over the hierarchical digraph with duplicate atoms (P-92.1), the
//! duplicates of a mancude ring averaged over its Kekulé structures (P-92.1.4.4), so that the
//! Kekulé structure drawn does not matter. A unit gets no
//! descriptor, and [`iupac_name_with_notes`] says why, where the drawing does not fix it (no
//! wedge at a centre, a wavy, bold or plain hashed bond, a double bond drawn straight), where it
//! is not on the parent (a stereocentre in a substituent prefix: "[(2R)-butan-2-yl]" is not
//! produced, nor are the descriptors of substituent groups drawn on their own and anhydrides),
//! and for what is not supported: pseudoasymmetric centres (r/s) and other ligands told apart
//! only by CIP Rules 1b–5, allenes (axial chirality); centres on atoms other than carbon,
//! cumulenes, atropisomers and planar chirality are not perceived at all. Wedges are read as
//! absolute configuration: relative descriptors (rel-, R*, rac-) are not produced. See also
//! [`has_stereo_marks`].
//!
//! Choices made where IUPAC 2013 offers more than one name, or where PubChem's names (OpenEye
//! Lexichem) differ:
//!
//! - Preferred prefixes: propan-2-yl (not isopropyl), butan-2-yl, 2-methylpropyl, tert-butyl,
//!   ethenyl (not vinyl), prop-2-en-1-yl (not allyl), benzyl (unsubstituted only), sulfanyl
//!   (not mercapto), methoxy to butoxy, tert-butoxy and phenoxy, then pentyloxy, hexyloxy…
//!   (P-63.2.2), methylene, 1,2-, 1,3- and 1,4-phenylene, otherwise "-diyl" (propane-1,3-diyl,
//!   not trimethylene or propylene; ethane-1,2-diyl, not ethylene; P-29.6).
//! - Retained names used: formic/acetic/benzoic/oxalic acid, formaldehyde, acetaldehyde,
//!   benzaldehyde, formamide, acetamide, benzamide, formonitrile, acetonitrile, benzonitrile,
//!   phenol, aniline, toluene, xylene and anisole (the last three unsubstituted only, P-22.1.3,
//!   P-63.2.3); acetyl, formyl, benzoyl and oxalyl in acyl halides ("acetyl chloride", not
//!   ethanoyl chloride; "oxalyl dichloride", P-65.5.1). Substituted acetic acid (and acetate, acetaldehyde, acetonitrile, acetyl) cite
//!   no locant ("chloroacetic acid", P-65.1.1.1); acetamide does ("2-chloroacetamide").
//! - Fully substituted parents are named with multiplying prefixes, not "per"
//!   ("hexabromoethane", P-14.3.4.5).
//! - Locants are cited except where P-31.1.4.2.4 omits them (mononuclear and monosubstituted
//!   two-carbon parents, monosubstituted rings, terminal chain suffixes, fully substituted
//!   parents): "propan-2-one", "prop-1-ene", "3-methylcyclohex-1-ene", "2-methylcyclohexan-1-ol".
//! - Enclosing marks follow P-16.5: "4-(propan-2-yl)benzoic acid", "di(propan-2-yl)",
//!   "bis(2-chloroethyl)", "dichloro(fluoro)methane"; a prefix containing a parent hydride's
//!   name is enclosed even when simple (P-16.5.1.4): "4-(cyclohexanecarbonyl)benzoic acid",
//!   "4-(cyclohexanecarboxamido)benzoic acid", but "4-propanoylbenzoic acid", "4-benzamido…".
//! - Heterocycles take the 2013 preferred names: Hantzsch–Widman names where the older
//!   retained names are no longer preferred (oxolane, not tetrahydrofuran; oxane, not
//!   tetrahydropyran; 1,3-oxazole and 1,2-oxazole, not oxazole and isoxazole; 1,3-thiazole),
//!   and the retained pyrrolidine, piperidine, piperazine and morpholine. On a heterocycle
//!   every locant is cited, a fully substituted one included ("2,3,4,5,6-pentachloropyridine").
//! - Fully conjugated monocycles take their cycloalkapolyene names
//!   ("cyclotetradeca-1,3,5,7,9,11,13-heptaene", P-22.1.2), not "\[14\]annulene"; lactones and
//!   lactams their pseudoketone names ("oxepan-2-one"), not "6-hexanelactone" (P-65.6.3.5.1).
//! - Acylamino prefixes are the "amido" names ("acetamido", P-66.1.1.4.3 method (1)), not
//!   "acetylamino".
//! - Inorganic names follow the Red Book (2005) where Wikipedia differs: "hydrogencarbonate"
//!   in one word (IR-8.4, IR-8.5; Wikipedia's "sodium hydrogen carbonate" is the older
//!   spelling), and every proportion stated by a multiplying prefix, "calcium dichloride" (the
//!   form the Red Book gives first, IR-5.2, IR-5.4.2.1, and the one the namer gives organic
//!   salts, "calcium diacetate"), not "calcium chloride", which IR-5.2 allows "if there is no
//!   ambiguity about the stoichiometry" — an alternative name (OPE-24). Traditional names such
//!   as ozone are alternatives too.
//! - Multiplicative names (the preferred names of some symmetrical compounds, e.g.
//!   2,2′-oxydi(ethan-1-ol), 1,1'-methylenedibenzene) and the retained names of α-amino acids
//!   are not produced; the substitutive name given instead ("benzylbenzene",
//!   "phenoxybenzene") is a correct, if not preferred, IUPAC name.

mod cip;
mod engine;
mod graph;
mod hetero;
mod inorganic;
mod ions;
mod stereo;
mod words;

#[cfg(test)]
mod inorganic_tests;
#[cfg(test)]
mod stereo_tests;
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
    name_and_notes(mol, config).0
}

/// [`iupac_name`], with a note on every stereogenic unit the name leaves without a descriptor
/// and why (in English): "C2: stereocentre drawn without a wedge or hash bond starting at it".
pub fn iupac_name_with_notes(mol: &Molecule) -> (NameResult, Vec<String>) {
    name_and_notes(mol, &Config::default())
}

fn name_and_notes(mol: &Molecule, config: &Config) -> (NameResult, Vec<String>) {
    // Brackets (a polymer's repeat unit, a multiple group) change what the atoms stand for;
    // naming the atoms as drawn would name the wrong compound.
    if mol.brackets().next().is_some() {
        return (
            NameResult::Unsupported("brackets (polymers, multiple groups)".into()),
            vec![],
        );
    }
    let (names, notes) = names_and_notes(mol, config);
    (joined(names), notes)
}

/// The names of the compounds of a drawing as one result.
fn joined(names: Vec<NameResult>) -> NameResult {
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

/// One name per compound drawn in `mol`, in drawing order: each neutral connected structure,
/// and all the ions together as one salt ("potassium propanoate"), in the place of the first.
pub fn iupac_names(mol: &Molecule) -> Vec<NameResult> {
    iupac_names_with_config(mol, &Config::default())
}

/// [`iupac_names`] with the abbreviations of `config`.
pub fn iupac_names_with_config(mol: &Molecule, config: &Config) -> Vec<NameResult> {
    names_and_notes(mol, config).0
}

/// [`iupac_names_with_config`], with the notes of [`iupac_name_with_notes`].
fn names_and_notes(mol: &Molecule, config: &Config) -> (Vec<NameResult>, Vec<String>) {
    let mut names = Vec::new();
    let mut notes = Vec::new();
    let mut ions = Vec::new();
    let mut salt_at = None;
    // The reason a charged structure was refused: the reason the salt it belongs to is.
    let mut refused_ion = None;
    for atoms in graph::components(mol) {
        match name_component(mol, &atoms, config, &mut notes) {
            Ok(Component::Ions(found)) => {
                if salt_at.is_none() {
                    salt_at = Some(names.len());
                    names.push(NameResult::Unsupported(String::new()));
                }
                ions.extend(found);
            }
            Ok(Component::Neutral(name)) => names.push(NameResult::Ok(name)),
            Err(r) => {
                let charge: i32 = atoms.iter().map(|&i| mol.atoms[i].charge as i32).sum();
                if refused_ion.is_none() && charge != 0 {
                    refused_ion = Some(r.clone());
                }
                names.push(NameResult::Unsupported(r));
            }
        }
    }
    if let Some(i) = salt_at {
        names[i] = match (ions::salt_name(&ions), refused_ion) {
            (Ok(n), _) => NameResult::Ok(n),
            (Err(_), Some(r)) | (Err(r), None) => NameResult::Unsupported(r),
        };
    }
    (names, notes)
}

/// What one connected structure is.
enum Component {
    /// A neutral compound, by its name.
    Neutral(String),
    /// Ions of a salt: one drawn on its own, or a salt drawn with bonds ("Na–Cl").
    Ions(Vec<ions::Ion>),
}

/// The name of one connected structure: a cation of the table, an inorganic structure
/// (`inorganic.rs`), or one named by the substitutive engine (a neutral compound, an anion, a
/// substituent). The notes on its stereogenic units go to `notes`.
fn name_component(
    mol: &Molecule,
    atoms: &[usize],
    config: &Config,
    notes: &mut Vec<String>,
) -> Result<Component, String> {
    if let Some(c) = ions::recognise_cation(mol, atoms) {
        return Ok(Component::Ions(vec![ions::Ion {
            name: ions::IonName::Table {
                name: c.name,
                compound: false,
            },
            charge: c.charge.into(),
        }]));
    }
    let raw = graph::expand(mol, atoms, config)?;
    if let Some(found) = inorganic::name(&raw) {
        return Ok(match found? {
            inorganic::Inorganic::Element(e) => Component::Neutral(e.english()),
            inorganic::Inorganic::Ions(ions) => Component::Ions(ions),
        });
    }
    let g = graph::to_graph(&raw)?;
    let p = graph::perceive(&g)?;
    let units = stereo::units(&g);
    let namer = engine::Namer::new(&g, &p).with_stereo(&units);
    let name = namer.name()?;
    notes.extend(namer.notes());
    let charge = g.net_charge();
    Ok(if charge == 0 {
        Component::Neutral(name)
    } else {
        Component::Ions(vec![ions::Ion {
            name: ions::IonName::Organic(name),
            charge,
        }])
    })
}

/// Whether the drawing has stereo bonds (wedges, hashes, wavy bonds). The name cites the
/// configurations they fix where it can; [`iupac_name_with_notes`] lists those it leaves out.
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
