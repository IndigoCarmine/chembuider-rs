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
//!   (a compositional name says nothing of the bonding, IR-5.2). Such a structure is neutral
//!   and complete, one formula unit, and is named on its own; only the charged structures of a
//!   drawing are named together as one salt ("Cl–Ca–Cl" and "Na–Cl": "calcium dichloride;
//!   sodium chloride", not the double salt "calcium sodium trichloride");
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
//!   omits only "when there is no ambiguity" — a judgement the namer does not make, so the
//!   metals of one common charge outside the cation table (silver, gallium) are refused too;
//! - an element molecule or a salt formula unit with more than 9999 of one atom or ion (the
//!   multiplying prefixes end at 9999, P-14.2.1);
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
//!   ("hexabromoethane"; P-14.3.4.5: "per" is no longer recommended; "perbromoethane" is a
//!   traditional alternative).
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
//!   lactams their pseudoketone names ("oxepan-2-one", P-65.6.3.5.1 method (1)), not
//!   "hexano-6-lactone" (method (2); P-66.1.5.1 for lactams); those names are alternatives.
//! - Acylamino prefixes are the "amido" names ("acetamido", P-66.1.1.4.3 method (1)), not
//!   "acetylamino".
//! - Inorganic names follow the Red Book (2005) where Wikipedia differs: "hydrogencarbonate"
//!   in one word (IR-8.4, IR-8.5; Wikipedia's "sodium hydrogen carbonate" is the older
//!   spelling), and every proportion stated by a multiplying prefix, "calcium dichloride" (the
//!   form the Red Book gives first, IR-5.2, IR-5.4.2.1, and the one the namer gives organic
//!   salts, "calcium diacetate"), not "calcium chloride", which IR-5.2 allows "if there is no
//!   ambiguity about the stoichiometry" — an alternative name, for a salt of one cation and one
//!   anion of the tables. "Ozone" is an alternative too, of the open O–O–O molecule only.
//! - Multiplicative names (the preferred names of some symmetrical compounds, e.g.
//!   2,2′-oxydi(ethan-1-ol), 1,1'-methylenedibenzene) and the retained names of α-amino acids
//!   are not produced; the substitutive name given instead ("benzylbenzene",
//!   "phenoxybenzene") is a correct, if not preferred, IUPAC name.
//!
//! Alternative names ([`iupac_alternatives`], [`NameKind`]): beside the one PIN, the names
//! IUPAC 2013 accepts in general nomenclature, made from the structure — by the engine in
//! another vocabulary (`engine::Style`) or by recognisers of exact structures
//! (`alternatives.rs`), never by rewriting the PIN's text — and only where they are certain:
//!
//! - retained names of the unsubstituted compounds (P-65.1.1.2 allows no substitution on them,
//!   so "1-chloroisopropyl" is never made): glycerol (propane-1,2,3-triol, not a ring),
//!   propionic acid (propionate, propionyl, propionic anhydride), succinic acid (succinate),
//!   ozone (the open molecule), and the group ethylene (–CH2–CH2–, P-29.6.2.3);
//! - traditional names IUPAC 2013 no longer recommends, given because they are still met and
//!   listed after the others ([`NameKind::Traditional`]): ε-caprolactone and ε-caprolactam
//!   (P-65.6.3.5 rejects such names: "not γ-butyrolactone"), "per" for a fully halogenated chain
//!   ("perbromoethane"; P-14.3.4.5), and the group propylene (–CH(CH3)–CH2–, of the 1979 and
//!   1993 rules; P-29.6.2.3 lists ethylene but not propylene). "Propylene" is only the name of
//!   that divalent group: the compound propene is not given it, nor is propane-1,3-diyl
//!   (Wikipedia's "propylene" for –CH2CH2CH2– is trimethylene, whose status the namer does not
//!   decide);
//! - functional class names of a compound with one heteroatom on a hydrocarbyl group:
//!   "methyl alcohol", "butyl chloride", "ethyl methyl ether", "diethyl ether", and amines
//!   "methylamine" (P-62.2.1.2 method (3), formed like a functional class name; where the
//!   group's name needs no enclosing marks);
//! - additive: "tetrahydrofuran" (oxolane itself); conjunctive: an unsubstituted benzene or
//!   cycloalkane ring with one chain carrying the group at its end, "cyclohexanemethanol",
//!   "cyclohexaneethanol", "cyclohexaneacetic acid";
//! - systematic names other than the PIN: the retained preferred names written
//!   systematically ("methanal", "ethanoic acid", "ethanoyl chloride", "benzenecarboxylic
//!   acid", "methylbenzene", "ethanoic propanoic anhydride"; formaldehyde stays the PIN,
//!   P-66.6.1.1.1), lactones and lactams from their acids in the 2013 form ("hexano-6-lactone",
//!   "butano-4-lactam": P-65.6.3.5.1 and P-66.1.5.1, method (2); not Wikipedia's
//!   "6-hexanelactone"), primary amines by "amino" prefixes ("aminomethane"), "[n]annulene"
//!   for the unsubstituted even rings of eight or more (also when the drawing fixes the ring's
//!   double bonds as Z or E: the name states no configuration, and a ring of only CH atoms has
//!   no other), salts without multiplying prefixes
//!   ("calcium chloride"), and "cis-"/"trans-" for the one stereogenic double bond of a
//!   structure whose two atoms each carry one hydrogen (there Z is cis): "trans-but-2-ene",
//!   "cis-cyclooctene". A structure whose name cites any other descriptor gets no
//!   alternative but "[n]annulene", since one written without it would leave the
//!   configuration out.
//!
//! Not produced: subtractive names (deoxy-, anhydro-, nor-: "2-deoxyribose"), since they
//! modify the retained names of natural products and sugars (P-31.1.5, P-102), which the namer
//! does not perceive — the deoxy sugar is named as the oxolane or oxane it is; cis/trans on
//! rings (relative configuration of ring substituents: the namer gives absolute R/S only) and on
//! double bonds of more than two substituents, where cis/trans is not defined by E/Z; Japanese
//! names ([`NameKind::Japanese`], OPE-26).

mod alternatives;
mod cip;
mod engine;
mod graph;
mod hetero;
mod inorganic;
mod ions;
mod stereo;
mod words;

#[cfg(test)]
mod alternatives_tests;
#[cfg(test)]
mod inorganic_tests;
#[cfg(test)]
mod stereo_tests;
#[cfg(test)]
mod tests;

use crate::config::Config;
use crate::molecule::{BondStereo, Molecule};
use engine::Style;

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

/// The kind of an alternative name ([`iupac_alternatives`]), in the order alternatives are
/// listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NameKind {
    /// A retained name IUPAC 2013 (or, for ozone, the Red Book) accepts in general
    /// nomenclature: "glycerol", "succinic acid", "potassium propionate", "ozone", "ethylene"
    /// (the group).
    Retained,
    /// A functional class (radicofunctional) name: "methyl alcohol", "butyl chloride", "ethyl
    /// methyl ether"; and "methylamine" (P-62.2.1.2, method (3): the substituent group's name
    /// followed by "amine"), listed here because it is formed like the functional class names,
    /// the group's name then the class word, though IUPAC counts it among the substitutive
    /// methods for amines.
    Radicofunctional,
    /// An additive name: "tetrahydrofuran".
    Additive,
    /// A conjunctive name: "cyclohexanemethanol".
    Conjunctive,
    /// A systematic name other than the PIN: "methanal", "benzenecarboxylic acid", "ethanoyl
    /// chloride", "hexano-6-lactone", "aminomethane", "[14]annulene", "trans-but-2-ene".
    Systematic,
    /// A traditional name of older rules or common use that IUPAC 2013 no longer recommends,
    /// given because it is still met: "ε-caprolactone", "ε-caprolactam" (P-65.6.3.5 rejects
    /// such names, "not γ-butyrolactone"), "propylene" for –CH(CH3)–CH2– (the 1979 and 1993
    /// rules; P-29.6.2.3 keeps ethylene only), "perbromoethane" ("per", P-14.3.4.5: no longer
    /// recommended). Listed after the names IUPAC 2013 accepts.
    Traditional,
    /// A Japanese name (reserved for OPE-26; not produced yet).
    Japanese,
}

impl NameKind {
    /// Every kind, in the order alternatives are listed.
    pub const ALL: [NameKind; 7] = [
        NameKind::Retained,
        NameKind::Radicofunctional,
        NameKind::Additive,
        NameKind::Conjunctive,
        NameKind::Systematic,
        NameKind::Traditional,
        NameKind::Japanese,
    ];

    /// The kind in English words ("retained name"), for reports; the panel shows it localised.
    pub fn label(self) -> &'static str {
        match self {
            NameKind::Retained => "retained name",
            NameKind::Radicofunctional => "functional class name",
            NameKind::Additive => "additive name",
            NameKind::Conjunctive => "conjunctive name",
            NameKind::Systematic => "systematic name",
            NameKind::Traditional => "traditional name",
            NameKind::Japanese => "Japanese name",
        }
    }
}

/// Everything naming says about a drawing: the IUPAC name ([`iupac_name`]), the notes of
/// [`iupac_name_with_notes`] and the alternative names of [`iupac_alternatives`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Naming {
    pub name: NameResult,
    pub notes: Vec<String>,
    pub alternatives: Vec<(NameKind, String)>,
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
    if let Some(refused) = refused_brackets(mol) {
        return (refused, vec![]);
    }
    let (names, notes) = names_and_notes(mol, config);
    (joined(names), notes)
}

/// Brackets (a polymer's repeat unit, a multiple group) change what the atoms stand for;
/// naming the atoms as drawn would name the wrong compound.
fn refused_brackets(mol: &Molecule) -> Option<NameResult> {
    mol.brackets()
        .next()
        .is_some()
        .then(|| NameResult::Unsupported("brackets (polymers, multiple groups)".into()))
}

/// The other names of everything drawn in `mol` beside its IUPAC name (OPE-24): retained,
/// functional class, additive, conjunctive and systematic names that IUPAC 2013 accepts in
/// general nomenclature, each with its kind. None repeats the IUPAC name or another; they are
/// listed by kind ([`NameKind::ALL`]), then in a fixed order. A drawing of several compounds
/// has the alternatives of each, the others by their IUPAC names ("methyl alcohol;
/// formaldehyde"); one the namer cannot name has none. Names are given only for structures
/// recognised exactly — never a retained name with substituents (P-65.1.1.2) — so most
/// structures have none.
pub fn iupac_alternatives(mol: &Molecule) -> Vec<(NameKind, String)> {
    iupac_naming(mol).alternatives
}

/// The IUPAC name, its notes and the alternatives of `mol` at once.
pub fn iupac_naming(mol: &Molecule) -> Naming {
    iupac_naming_with_config(mol, &Config::default())
}

/// [`iupac_naming`] with the abbreviations of `config`.
pub fn iupac_naming_with_config(mol: &Molecule, config: &Config) -> Naming {
    if let Some(name) = refused_brackets(mol) {
        return Naming {
            name,
            notes: vec![],
            alternatives: vec![],
        };
    }
    // One preferred naming gives the name, its notes and the structures' alternatives.
    let preferred = compounds(mol, config, Style::Preferred, true);
    let name = joined(preferred.names.clone());
    let notes = preferred.notes.clone();
    let alternatives = match &name {
        NameResult::Ok(_) => alternatives_of(mol, config, preferred),
        NameResult::Unsupported(_) => vec![],
    };
    Naming {
        name,
        notes,
        alternatives,
    }
}

/// The styles of the engine that give alternatives, and their kind.
const STYLES: [(Style, NameKind); 3] = [
    (Style::Retained, NameKind::Retained),
    (Style::Systematic, NameKind::Systematic),
    (Style::Per, NameKind::Traditional),
];

/// [`iupac_alternatives`] of a drawing whose every compound is named, `preferred` its compounds
/// named in [`Style::Preferred`] with their alternatives.
fn alternatives_of(
    mol: &Molecule,
    config: &Config,
    preferred: Compounds,
) -> Vec<(NameKind, String)> {
    let Some(pins) = preferred
        .names
        .iter()
        .map(|n| n.name().map(str::to_string))
        .collect::<Option<Vec<String>>>()
    else {
        return vec![];
    };
    let mut each = preferred.alternatives;
    for (style, kind) in STYLES {
        let styled = compounds(mol, config, style, false);
        for (i, name) in styled.names.into_iter().enumerate() {
            if let NameResult::Ok(n) = name
                && n != pins[i]
            {
                each[i].push((kind, n));
            }
        }
    }
    let pin = pins.join("; ");
    let mut out: Vec<(NameKind, String)> = Vec::new();
    for (i, alternatives) in each.into_iter().enumerate() {
        for (kind, alternative) in alternatives {
            let mut names = pins.clone();
            names[i] = alternative;
            let whole = names.join("; ");
            if whole != pin && !out.iter().any(|(_, n)| *n == whole) {
                out.push((kind, whole));
            }
        }
    }
    out.sort_by_key(|&(kind, _)| kind);
    out
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

/// One name per compound drawn in `mol`, in drawing order: each neutral connected structure (a
/// salt drawn with bonds, "Cl–Ca–Cl", among them), and all the charged structures together as
/// one salt ("potassium propanoate"), in the place of the first.
pub fn iupac_names(mol: &Molecule) -> Vec<NameResult> {
    iupac_names_with_config(mol, &Config::default())
}

/// [`iupac_names`] with the abbreviations of `config`.
pub fn iupac_names_with_config(mol: &Molecule, config: &Config) -> Vec<NameResult> {
    names_and_notes(mol, config).0
}

/// [`iupac_names_with_config`], with the notes of [`iupac_name_with_notes`].
fn names_and_notes(mol: &Molecule, config: &Config) -> (Vec<NameResult>, Vec<String>) {
    let c = compounds(mol, config, Style::Preferred, false);
    (c.names, c.notes)
}

/// The compounds of a drawing as named in one style: each one's name, with the alternatives
/// that come from its structure when `alternatives` is set; and the notes on the stereogenic
/// units.
struct Compounds {
    names: Vec<NameResult>,
    notes: Vec<String>,
    alternatives: Vec<Vec<(NameKind, String)>>,
}

/// [`iupac_names_with_config`] in `style`.
fn compounds(mol: &Molecule, config: &Config, style: Style, alternatives: bool) -> Compounds {
    let mut names = Vec::new();
    let mut each = Vec::new();
    let mut notes = Vec::new();
    let mut ions = Vec::new();
    let mut salt_at = None;
    // The reason a charged structure was refused: the reason the salt it belongs to is.
    let mut refused_ion = None;
    for atoms in graph::components(mol) {
        let mut alts = alternatives.then(Vec::new);
        match name_component(mol, &atoms, config, style, &mut notes, alts.as_mut()) {
            Ok(Component::Ions(found)) => {
                if salt_at.is_none() {
                    salt_at = Some(names.len());
                    names.push(NameResult::Unsupported(String::new()));
                    each.push(Vec::new());
                }
                ions.extend(found);
            }
            Ok(Component::Neutral(name)) => {
                names.push(NameResult::Ok(name));
                each.push(alts.unwrap_or_default());
            }
            Err(r) => {
                let charge: i32 = atoms.iter().map(|&i| mol.atoms[i].charge as i32).sum();
                if refused_ion.is_none() && charge != 0 {
                    refused_ion = Some(r.clone());
                }
                names.push(NameResult::Unsupported(r));
                each.push(Vec::new());
            }
        }
    }
    if let Some(i) = salt_at {
        if alternatives && let Some(n) = ions::salt_name_unmultiplied(&ions) {
            each[i].push((NameKind::Systematic, n));
        }
        names[i] = match (ions::salt_name(&ions), refused_ion) {
            (Ok(n), _) => NameResult::Ok(n),
            (Err(_), Some(r)) | (Err(r), None) => NameResult::Unsupported(r),
        };
    }
    Compounds {
        names,
        notes,
        alternatives: each,
    }
}

/// What one connected structure is.
enum Component {
    /// A neutral compound, by its name; a salt drawn with bonds ("Na–Cl") among them.
    Neutral(String),
    /// The ions of a charged structure: one ion drawn on its own, or a metal bonded to anions
    /// of more charge than its own ("Na–O–SO₃⁻"). The ions of a drawing make one salt.
    Ions(Vec<ions::Ion>),
}

/// The name of one connected structure in `style`: a cation of the table, an inorganic
/// structure (`inorganic.rs`), or one named by the substitutive engine (a neutral compound, an
/// anion, a substituent). The notes on its stereogenic units go to `notes`, the alternatives
/// that come from its structure (`alternatives.rs`) to `alternatives`.
fn name_component(
    mol: &Molecule,
    atoms: &[usize],
    config: &Config,
    style: Style,
    notes: &mut Vec<String>,
    alternatives: Option<&mut Vec<(NameKind, String)>>,
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
            inorganic::Inorganic::Element(e) => {
                // IR-3.4.3: "ozone", acceptable beside "trioxygen", for the open O–O–O molecule
                // (two bonds); a ring of three is another species (cyclo-, IR-3.4.3).
                if let Some(alts) = alternatives
                    && e.name == "oxygen"
                    && e.atoms == 3
                    && raw.bonds.len() == 2
                {
                    alts.push((NameKind::Retained, "ozone".into()));
                }
                Component::Neutral(e.english()?)
            }
            // A salt drawn with bonds is neutral and complete: one formula unit, named on its
            // own (IR-5.4), not pooled with the other structures' ions into a double salt.
            inorganic::Inorganic::Ions(ions) if ions.iter().map(|i| i.charge).sum::<i32>() == 0 => {
                if let Some(alts) = alternatives
                    && let Some(n) = ions::salt_name_unmultiplied(&ions)
                {
                    alts.push((NameKind::Systematic, n));
                }
                Component::Neutral(ions::salt_name(&ions)?)
            }
            inorganic::Inorganic::Ions(ions) => Component::Ions(ions),
        });
    }
    let g = graph::to_graph(&raw)?;
    let p = graph::perceive(&g)?;
    let units = stereo::units(&g);
    let namer = engine::Namer::new(&g, &p)
        .with_stereo(&units)
        .with_style(style);
    let name = namer.name()?;
    let unit_notes = namer.notes();
    let charge = g.net_charge();
    if let Some(alts) = alternatives
        && charge == 0
    {
        alts.extend(alternatives::of_structure(
            &g,
            &p,
            &units,
            &namer,
            unit_notes.is_empty(),
        ));
    }
    notes.extend(unit_notes);
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
