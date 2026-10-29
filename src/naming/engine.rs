//! Substitutive naming: choose the parent (chain or ring), number it, name what hangs on it.
//!
//! The same procedure names the whole molecule and every substituent. A substituent is a
//! *unit* whose "principal group" is its free valence (or, for an acyl group, its C=O carbon),
//! so choosing a substituent's chain and numbering it follow the very rules the molecule's
//! parent does — which is how IUPAC 2013 defines them.
//!
//! The order in which candidate parents and numberings are compared (P-44, P-45, P-31.1.4):
//!
//! 1. most principal characteristic groups (or the free valence) on the parent;
//! 2. a ring rather than a chain (P-44.1.2.2, 2013 rule: rings are always senior);
//! 3. for rings: the most rings (a bicycle or spiro system before a monocycle), the most ring
//!    atoms, then the most multiple bonds (P-44.2.1, P-44.4.1.1) — benzene before cyclohexene
//!    before cyclohexane — a ring assembly of identical rings (biphenyl) being one parent; for
//!    chains: the most skeletal atoms, then the most multiple bonds, then double bonds;
//! 4. in a von Baeyer system, within its fixed numbering, lowest locants for its heteroatoms
//!    together, then for O, S and N in turn (P-23.3.2, P-24.2.4.1);
//! 5. lowest locants for the principal groups / free valence;
//! 6. lowest locants for all multiple bonds together, then for double bonds (in a von Baeyer
//!    system: first the fewest compound locants, then the locants without those in
//!    parentheses, then all of them, P-31.1.4.2);
//! 7. most substituent prefixes, then lowest locants for them all together;
//! 8. lowest locants for the prefix cited first in alphanumerical order, then the
//!    alphanumerically first name.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use super::graph::{self, Class, El, Graph, Perceived, RingParent, Unsupported};
use super::hetero::HeteroRing;
use super::stereo::{Descriptor, Site, Unit};
use super::words::{
    MAX_NUMERAL, alpha_key, enclose, multiplier, multiplier_complex, starts_with_vowel, stem,
};

/// The atom a substituent named on its own is attached to: none (its attachment point "*" is
/// not an atom of the graph).
const NONE: usize = usize::MAX;

/// A named unit: the molecule's name, or a substituent prefix ("2-chloroethyl").
#[derive(Clone, Debug)]
pub(crate) struct Prefix {
    pub text: String,
    /// The prefix has prefixes of its own, which makes it compound: it is enclosed in
    /// parentheses and multiplied by "bis", "tris", …
    pub compound: bool,
    /// How deeply the enclosing marks inside `text` nest.
    pub depth: usize,
    /// The prefix contains the whole name of a parent hydride ("cyclohexanecarbonyl",
    /// "cyclopropanecarboxamido"), which encloses it even when it is simple (P-16.5.1.4).
    pub hydride: bool,
    /// Its "-yl" may contract to "-oxy" when an oxygen follows: methyl → methoxy,
    /// 2-chloroethyl → 2-chloroethoxy, phenyl → phenoxy, tert-butyl → tert-butoxy.
    pub oxy: bool,
}

impl Prefix {
    fn simple(text: &str) -> Self {
        Prefix {
            text: text.into(),
            compound: false,
            depth: 0,
            hydride: false,
            oxy: false,
        }
    }

    pub(crate) fn needs_enclosure(&self) -> bool {
        self.compound || self.hydride || self.text.contains(|c: char| c.is_ascii_digit())
    }

    /// The prefix as it is written when cited once, and the depth of its marks.
    fn cited(&self) -> (String, usize) {
        if self.needs_enclosure() {
            (enclose(&self.text, self.depth), self.depth + 1)
        } else {
            (self.text.clone(), self.depth)
        }
    }
}

/// Skeletal locants are stored as `number * PRIMES + primes`, so that they sort the way IUPAC
/// compares them: 1 < 1' < 1'' < 2 (P-31.1.4). Only ring assemblies have primed locants.
const PRIMES: u32 = 16;

/// The locant `n` (unprimed).
fn loc(n: usize) -> u32 {
    n as u32 * PRIMES
}

/// A locant as written: "4", "4'", "1''".
fn loc_text(l: u32) -> String {
    format!("{}{}", l / PRIMES, "'".repeat((l % PRIMES) as usize))
}

fn loc_list(locs: &[u32]) -> String {
    locs.iter()
        .map(|&l| loc_text(l))
        .collect::<Vec<_>>()
        .join(",")
}

/// A locant: the nitrogen of an amine or amide, or a numbered skeletal atom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Loc {
    N,
    C(u32),
}

impl std::fmt::Display for Loc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Loc::N => write!(f, "N"),
            Loc::C(n) => write!(f, "{}", loc_text(*n)),
        }
    }
}

/// The kind of a candidate parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    Chain,
    Ring {
        benzene: bool,
        /// A heterocycle, named and numbered from its table entry.
        hetero: Option<&'static HeteroRing>,
    },
    /// A ring assembly of `rings` identical rings joined by single bonds.
    Assembly {
        rings: usize,
        benzene: bool,
        hetero: Option<&'static HeteroRing>,
    },
    /// A von Baeyer system of two rings (bicycle or spiro): `ring` indexes
    /// [`Perceived::rings`].
    Poly {
        ring: usize,
    },
}

/// One numbering of a candidate parent.
struct Numbering {
    /// Its atoms, in the order of their locants.
    atoms: Vec<usize>,
    /// The locant of each atom.
    locs: Vec<u32>,
    shape: Shape,
    /// For an assembly: the locants of each ring-to-ring bond.
    joins: Vec<(u32, u32)>,
    /// A chain whose principal groups are attached to it rather than end it, expressed like
    /// those on a ring: "propane-1,2,3-tricarboxylic acid" (P-65.1.2.2.1).
    attached: bool,
    /// For a von Baeyer system: which of its Kekulé forms ([`graph::Poly::forms`]).
    form: usize,
}

/// What the unit being named is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The whole structure, with the principal characteristic group perception found.
    Molecule,
    /// A substituent attached through `atom` by a bond of `order` (yl, ylidene, ylidyne).
    FreeValence { atom: usize, order: u8 },
    /// An acyl substituent whose C=O carbon `atom` carries the free valence (acetyl, benzoyl).
    Acyl { atom: usize },
    /// The amide whose C=O carbon is `atom`, named as a substituent through its nitrogen: an
    /// acylamino prefix, the amide's name ending in "amido" ("acetamido", P-66.1.1.4.3).
    Amido { atom: usize },
    /// A substituent group drawn on its own with several free valences, all single bonds on
    /// carbon ([`Graph::free`]): propane-1,3-diyl, methylene, 1,4-phenylene.
    Valences,
    /// A substituent group drawn on its own whose free valences are all on C=O carbons, one on
    /// each: oxalyl, butanedioyl, benzene-1,4-dicarbonyl (P-65.1.7).
    Acyls,
}

/// One principal group instance and where it may sit on a parent.
#[derive(Clone, Debug)]
enum Pg {
    /// A group whose carbon belongs to it: inside a chain (as its end) or bonded to a ring.
    InChain(usize),
    /// A group on a skeletal atom: on whichever of `anchors` is in the parent; `group` is the
    /// atom that makes up the group (the O of an OH, the N of an amine, the O of a ketone).
    OnAtom {
        anchors: Vec<usize>,
        group: Option<usize>,
    },
}

/// The suffix the parent takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Suffix {
    None,
    Class(Class),
    Yl(u8),
    Acyl,
}

type Key = (
    Reverse<usize>,
    Reverse<usize>,
    Reverse<usize>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Vec<u32>,
    Reverse<usize>,
    Vec<u32>,
);

/// Which locants of its parent a name leaves out, and so the stereodescriptors there
/// (P-93.4.1.1: "When no locant is required in the name …, no locant is necessary before the
/// stereodescriptor"): "(R)-bromo(chloro)(fluoro)methane", "(E)-1,2-dichloroethene",
/// "(Z)-cyclooctene".
#[derive(Clone, Copy, Default)]
struct Uncited {
    /// The locants of the parent's atoms: methane, a monosubstituted ring.
    atoms: bool,
    /// The locants of its multiple bonds: ethene, cyclooctene (P-31.1.3.1).
    bonds: bool,
}

/// One numbered candidate parent, evaluated.
struct Eval {
    atoms: Vec<usize>,
    /// The locant of each atom.
    locs: Vec<u32>,
    shape: Shape,
    joins: Vec<(u32, u32)>,
    attached: bool,
    suffix: Vec<u32>,
    /// The multiple bonds: the lower locant of each and its order.
    mults: Vec<(u32, u8)>,
    /// For a von Baeyer system, the higher locant of each multiple bond, cited in parentheses
    /// when the two are not consecutive: "oct-1(6)-ene" (P-31.1.1).
    mult_ends: Vec<u32>,
    prefixes: Vec<(Loc, usize, usize)>,
    rprime: Option<(usize, usize)>,
    key: Key,
}

/// The vocabulary a name is written in: the preferred IUPAC name's, or that of one of the other
/// names IUPAC 2013 accepts in general nomenclature (OPE-24). Each style changes only what it
/// says; the rest of the name is the PIN's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Style {
    #[default]
    Preferred,
    /// Systematic names for the retained preferred ones: methanal, ethanoic acid, ethanoyl
    /// chloride, ethanedioic acid, benzenecarboxylic acid, benzenol, methylbenzene (P-65.1.1.1,
    /// P-66.6.1.1.1, P-22.1.3); substituted, with the locants a systematic name cites
    /// ("2-chloroethanoic acid", "4-methylbenzene-1-carboxylic acid").
    Systematic,
    /// The names retained for general nomenclature only, of the unsubstituted acids
    /// (P-65.1.1.2, which allows them no substitution): propionic acid, propionate, propionyl,
    /// succinic acid, succinate.
    Retained,
    /// "per" for a chain whose every hydrogen one halogen replaces: perbromoethane
    /// (P-14.3.4.5: not in PINs).
    Per,
}

pub(crate) struct Namer<'a> {
    g: &'a Graph,
    p: &'a Perceived,
    cache: RefCell<HashMap<(usize, usize), Result<Prefix, Unsupported>>>,
    /// The stereogenic units whose descriptors the name cites.
    stereo: &'a [Unit],
    /// The stereogenic units the name leaves without a descriptor, and why.
    notes: RefCell<Vec<String>>,
    style: Style,
}

impl<'a> Namer<'a> {
    pub(crate) fn new(g: &'a Graph, p: &'a Perceived) -> Self {
        Namer {
            g,
            p,
            cache: RefCell::new(HashMap::new()),
            stereo: &[],
            notes: RefCell::new(Vec::new()),
            style: Style::Preferred,
        }
    }

    /// The namer writing names in `style`.
    pub(crate) fn with_style(self, style: Style) -> Self {
        Namer { style, ..self }
    }

    /// The namer citing the descriptors of `units` (P-93).
    pub(crate) fn with_stereo(self, units: &'a [Unit]) -> Self {
        Namer {
            stereo: units,
            ..self
        }
    }

    /// The stereogenic units the name left without a descriptor, and why (after [`name`]).
    ///
    /// [`name`]: Self::name
    pub(crate) fn notes(&self) -> Vec<String> {
        self.notes.borrow().clone()
    }

    /// Notes for every stereogenic unit, none of whose descriptors is cited because the
    /// structure is named `how`.
    fn note_all(&self, how: &str) {
        for u in self.stereo {
            let why = match &u.descriptor {
                Ok(_) => format!("stereodescriptors are not generated for {how}"),
                Err(r) => r.clone(),
            };
            self.notes
                .borrow_mut()
                .push(format!("{}: {why}", site_text(u.site, None)));
        }
    }

    pub(crate) fn name(&self) -> Result<String, Unsupported> {
        if !self.g.free.is_empty() {
            let n = self.substituent()?;
            self.note_all("substituent groups");
            return Ok(n);
        }
        let count = |k: Class| self.p.ctype.iter().filter(|t| **t == Some(k)).count();
        if self.p.principal == Some(Class::Carboxylate)
            && count(Class::Acid) > 0
            && count(Class::Acid) + count(Class::Carboxylate) > 2
        {
            return Err("partial salt of an acid with more than two acid groups \
                        (hydrogen …tricarboxylate, P-65.6.2.3)"
                .into());
        }
        if self.p.principal == Some(Class::Anhydride) {
            let n = self.anhydride()?;
            self.note_all("anhydrides");
            return Ok(n);
        }
        let region = vec![true; self.g.len()];
        Ok(self.unit_cited(Mode::Molecule, &region, true)?.text)
    }

    /// The name of a substituent group drawn with attachment points (P-29): "pentyl",
    /// "pentyloxy", "hexanoyl", "1-chloropropan-2-yl"; with several free valences on carbon,
    /// "propane-1,3-diyl", "methylene", "1,4-phenylene".
    fn substituent(&self) -> Result<String, Unsupported> {
        let g = self.g;
        if g.net_charge() != 0 {
            return Err("charged substituent group".into());
        }
        match g.free[..] {
            [(a, order)] => Ok(self.sub_with(a, NONE, order)?.text),
            _ => {
                if g.free.iter().any(|&(a, o)| o != 1 || g.el[a] != El::C) {
                    return Err("several free valences other than single bonds on carbon \
                                (…diylidene, …diylbis(oxy))"
                        .into());
                }
                let acyl = |a: usize| !self.p.in_ring[a] && self.has_carbonyl(a).is_some();
                let acyls = g.free.iter().filter(|&&(a, _)| acyl(a)).count();
                let mode = if acyls == 0 {
                    Mode::Valences
                } else if acyls < g.free.len() {
                    return Err("free valences both on C=O carbons and on other atoms \
                                (…oxo…diyl or acyl…yl groups)"
                        .into());
                } else if g.free[0].0 == g.free[1].0 {
                    // Two free valences on one C=O carbon, which then has no other neighbour:
                    // P-65.2.1.5.
                    return Ok("carbonyl".into());
                } else {
                    Mode::Acyls
                };
                let region = vec![true; g.len()];
                Ok(self.unit(mode, &region)?.text)
            }
        }
    }

    /// An acyclic anhydride (P-65.7.1, P-65.7.2): the names of its two acids, "acid" replaced
    /// by "anhydride" — "propanoic anhydride", "chloroacetic anhydride" — or, for two different
    /// acids, both cited in alphanumerical order: "acetic propanoic anhydride".
    fn anhydride(&self) -> Result<String, Unsupported> {
        let (g, p) = (self.g, self.p);
        let internal = || "internal error: anhydride".to_string();
        let c = (0..g.len())
            .find(|&c| p.ctype[c] == Some(Class::Anhydride))
            .ok_or_else(internal)?;
        let o = g
            .nbrs(c)
            .find(|&m| g.el[m] == El::O && g.order(c, m) == 1)
            .ok_or_else(internal)?;
        let d = g.nbrs(o).find(|&m| m != c).ok_or_else(internal)?;
        let mut acids = Vec::new();
        for half in [g.acid_half(c, o), g.acid_half(d, o)] {
            let hp = graph::perceive(&half)?;
            let name = Namer::new(&half, &hp).with_style(self.style).name()?;
            let acid = name
                .strip_suffix(" acid")
                .filter(|_| hp.principal == Some(Class::Acid))
                .ok_or_else(|| format!("anhydride of \u{201c}{name}\u{201d}"))?;
            acids.push(acid.to_string());
        }
        acids.sort_by_key(|a| (alpha_key(a), a.clone()));
        acids.dedup();
        Ok(format!("{} anhydride", acids.join(" ")))
    }

    fn has_carbonyl(&self, c: usize) -> Option<usize> {
        self.g.adj[c]
            .iter()
            .find(|&&(m, o)| o == 2 && self.g.el[m] == El::O)
            .map(|&(m, _)| m)
    }

    /// Whether `a` may be part of a chain or ring parent in `mode`.
    fn is_skeletal(&self, a: usize, mode: Mode) -> bool {
        self.g.el[a] == El::C
            && match self.p.ctype[a] {
                None | Some(Class::Aldehyde) => true,
                Some(c) => {
                    (mode == Mode::Molecule && Some(c) == self.p.principal)
                        || mode == Mode::Amido { atom: a }
                }
            }
    }

    fn principal_groups(&self, mode: Mode, region: &[bool]) -> Vec<Pg> {
        let (g, p) = (self.g, self.p);
        let atoms = (0..g.len()).filter(|&a| region[a]);
        match mode {
            Mode::FreeValence { atom, .. } => vec![Pg::OnAtom {
                anchors: vec![atom],
                group: None,
            }],
            Mode::Valences => g
                .free
                .iter()
                .map(|&(a, _)| Pg::OnAtom {
                    anchors: vec![a],
                    group: None,
                })
                .collect(),
            Mode::Acyls => g.free.iter().map(|&(a, _)| Pg::InChain(a)).collect(),
            Mode::Acyl { atom } | Mode::Amido { atom } => vec![Pg::InChain(atom)],
            Mode::Molecule => match p.principal {
                None => vec![],
                Some(c) if c.has_carbon() => atoms
                    .filter(|&a| p.ctype[a] == Some(c))
                    .map(Pg::InChain)
                    .collect(),
                Some(Class::Ketone) => atoms
                    .filter(|&a| p.ketone[a])
                    .map(|a| Pg::OnAtom {
                        anchors: vec![a],
                        group: self.has_carbonyl(a),
                    })
                    .collect(),
                Some(Class::Alcohol) => atoms
                    .filter(|&a| p.hydroxyl[a])
                    .map(|o| Pg::OnAtom {
                        anchors: vec![g.adj[o][0].0],
                        group: Some(o),
                    })
                    .collect(),
                Some(Class::Thiol) => atoms
                    .filter(|&a| p.thiol[a])
                    .map(|s| Pg::OnAtom {
                        anchors: vec![g.adj[s][0].0],
                        group: Some(s),
                    })
                    .collect(),
                Some(Class::Amine) => atoms
                    .filter(|&a| p.amine[a])
                    .map(|n| Pg::OnAtom {
                        anchors: g.nbrs(n).collect(),
                        group: Some(n),
                    })
                    .collect(),
                // Anhydrides are named by functional class (`anhydride`), never as a suffix.
                Some(_) => unreachable!("every class is handled above"),
            },
        }
    }

    /// All chains (as numbered atom lists, both directions) of the acyclic skeleton in
    /// `region`, from end to end; in a substituent, only those through its free valence. With
    /// `attached`, the carbons of the principal groups stay out of the chains.
    fn chains(&self, mode: Mode, region: &[bool], attached: bool) -> Vec<Vec<usize>> {
        let (g, p) = (self.g, self.p);
        let n = g.len();
        let ok: Vec<bool> = (0..n)
            .map(|a| {
                region[a]
                    && !p.in_ring[a]
                    && self.is_skeletal(a, mode)
                    && !(attached && self.is_group_carbon(a, mode))
            })
            .collect();
        let nb = |a: usize| g.nbrs(a).filter(|&m| ok[m]).collect::<Vec<_>>();
        let must: Vec<usize> = match mode {
            Mode::FreeValence { atom, .. } | Mode::Acyl { atom } | Mode::Amido { atom } => {
                vec![atom]
            }
            Mode::Valences => g.free.iter().map(|&(a, _)| a).collect(),
            Mode::Molecule | Mode::Acyls => vec![],
        };
        let mut ends: Vec<usize> = (0..n).filter(|&a| ok[a] && nb(a).len() <= 1).collect();
        for &m in &must {
            if ok[m] && !ends.contains(&m) {
                ends.push(m);
            }
        }
        let mut out = Vec::new();
        for &u in &ends {
            // Breadth-first parents from u: the skeleton is a forest, so paths are unique.
            let mut prev = vec![usize::MAX; n];
            let mut seen = vec![false; n];
            seen[u] = true;
            let mut queue = std::collections::VecDeque::from([u]);
            while let Some(x) = queue.pop_front() {
                for y in nb(x) {
                    if !seen[y] {
                        seen[y] = true;
                        prev[y] = x;
                        queue.push_back(y);
                    }
                }
            }
            for &v in &ends {
                if !seen[v] {
                    continue;
                }
                let mut path = vec![v];
                let mut cur = v;
                while cur != u {
                    cur = prev[cur];
                    path.push(cur);
                }
                path.reverse();
                if must.is_empty() || must.iter().any(|m| path.contains(m)) {
                    out.push(path);
                }
            }
        }
        out
    }

    /// Whether `a` is the carbon of a principal group of the kind a chain may have attached
    /// rather than end it (propane-1,2,3-tricarboxylic acid, propane-1,2,3-tricarbonyl).
    fn is_group_carbon(&self, a: usize, mode: Mode) -> bool {
        match mode {
            Mode::Molecule => self.p.ctype[a].is_some() && self.p.ctype[a] == self.p.principal,
            Mode::Acyls => self.g.free_on(a) > 0,
            _ => false,
        }
    }

    /// How many of the principal groups `pgs` a parent with `atoms` carries; with `attached`
    /// (a ring, or a chain of attached groups), groups bonded to it rather than in it.
    fn counts(&self, pgs: &[Pg], atoms: &[usize], attached: bool) -> usize {
        pgs.iter()
            .filter(|pg| match pg {
                Pg::InChain(c) => {
                    if attached {
                        !self.p.in_ring[*c] && self.g.nbrs(*c).any(|m| atoms.contains(&m))
                    } else {
                        atoms.contains(c)
                    }
                }
                Pg::OnAtom { anchors, .. } => anchors.iter().any(|a| atoms.contains(a)),
            })
            .count()
    }

    fn unit(&self, mode: Mode, region: &[bool]) -> Result<Prefix, Unsupported> {
        self.unit_cited(mode, region, false)
    }

    /// The unit named in `mode`; with `stereo`, citing the descriptors of the stereogenic units
    /// on its parent.
    fn unit_cited(&self, mode: Mode, region: &[bool], stereo: bool) -> Result<Prefix, Unsupported> {
        let p = self.p;
        let pgs = self.principal_groups(mode, region);
        // A substituent whose free valence is on a ring is named from that ring (or assembly).
        let rings: Vec<&RingParent> = match mode {
            Mode::FreeValence { atom, .. } => p
                .parents
                .iter()
                .filter(|r| r.atoms.contains(&atom))
                .collect(),
            _ => p
                .parents
                .iter()
                .filter(|r| r.atoms.iter().all(|&a| region[a]))
                .collect(),
        };
        let mut chains = match mode {
            Mode::FreeValence { atom, .. } if p.in_ring[atom] => vec![],
            _ => self.chains(mode, region, false),
        };
        let mut chain_max = chains.iter().map(|c| self.counts(&pgs, c, false)).max();
        // More than two acid (amide, nitrile…) groups on one unbranched chain: all of them are
        // expressed by the "carboxylic acid" kind of suffix, the chain not including their
        // carbons (P-65.1.2.2.1: propane-1,2,3-tricarboxylic acid). Otherwise two end the chain
        // ("dioic acid") and the others are prefixes: 3-(carboxymethyl)heptanedioic acid.
        let mut attached = false;
        let carbon_groups = match mode {
            Mode::Molecule => p.principal.is_some_and(Class::has_carbon),
            Mode::Acyls => true,
            _ => false,
        };
        if carbon_groups && pgs.len() > 2 {
            let alt = self.chains(mode, region, true);
            let alt_max = alt.iter().map(|c| self.counts(&pgs, c, true)).max();
            if alt_max.is_some_and(|m| m > 2) {
                (chains, chain_max, attached) = (alt, alt_max, true);
            }
        }
        let ring_max = rings
            .iter()
            .map(|r| self.counts(&pgs, &r.atoms, true))
            .max();
        let pick_ring = match mode {
            Mode::FreeValence { atom, .. } => p.in_ring[atom],
            _ => match (ring_max, chain_max) {
                (Some(rc), Some(cm)) => rc >= cm,
                (Some(_), None) => true,
                _ => false,
            },
        };
        // A group's free valences must all be on its parent: a parent with fewer would name a
        // different group with fewer valences (the others lost in its prefixes). Groups whose
        // valences lie on several parents are compound or multiplied (P-29.4.2).
        if matches!(mode, Mode::Valences | Mode::Acyls)
            && (if pick_ring { ring_max } else { chain_max }) != Some(pgs.len())
        {
            return Err("free valences not all on one parent chain or ring \
                        (…phenylenebis(methylene), oxydi(ethane-2,1-diyl), P-29.4.2)"
                .into());
        }
        let numberings: Vec<Numbering> = if pick_ring {
            let best = ring_max.ok_or("internal error: ring parent outside the region")?;
            let rings: Vec<&RingParent> = rings
                .into_iter()
                .filter(|r| self.counts(&pgs, &r.atoms, true) == best)
                .collect();
            self.senior_rings(rings)?
                .into_iter()
                .flat_map(|r| self.ring_numberings(r))
                .collect()
        } else {
            let best = chain_max.ok_or("internal error: no parent chain")?;
            chains
                .into_iter()
                .filter(|c| self.counts(&pgs, c, attached) == best)
                .map(|atoms| Numbering {
                    locs: (1..=atoms.len()).map(loc).collect(),
                    atoms,
                    shape: Shape::Chain,
                    joins: vec![],
                    attached,
                    form: 0,
                })
                .collect()
        };

        let evals: Vec<Eval> = numberings
            .into_iter()
            .map(|num| self.evaluate(num, &pgs, region))
            .collect();
        let best = evals
            .iter()
            .map(|e| &e.key)
            .min()
            .ok_or("internal error: no numbering")?
            .clone();
        // The candidates that tie on everything but their stereodescriptors.
        let mut chosen: Vec<(&Eval, Vec<Loc>, Prefix, Uncited)> = Vec::new();
        for e in evals.iter().filter(|e| e.key == best) {
            let (order, named, uncited) = self.render(e, mode, region, "")?;
            let rank = chosen
                .first()
                .map(|(_, o, n, _)| (&order, &named.text).cmp(&(o, &n.text)));
            match rank {
                Some(std::cmp::Ordering::Greater) => {}
                Some(std::cmp::Ordering::Equal) => chosen.push((e, order, named, uncited)),
                _ => chosen = vec![(e, order, named, uncited)],
            }
        }
        if !stereo {
            return Ok(chosen.swap_remove(0).2);
        }
        // Among those, the lower locants go to Z, then to R (P-14.4 (j)).
        let cited: Vec<_> = chosen.iter().map(|(e, ..)| self.cite(e)).collect();
        let locants_of = |c: &[(u32, Descriptor)], d: Descriptor| -> Vec<u32> {
            c.iter().filter(|x| x.1 == d).map(|x| x.0).collect()
        };
        let pick = (0..cited.len())
            .min_by_key(|&i| {
                let c = &cited[i].0;
                (
                    Reverse(c.len()),
                    locants_of(c, Descriptor::Z),
                    locants_of(c, Descriptor::R),
                )
            })
            .expect("at least one candidate");
        let (descriptors, notes) = &cited[pick];
        self.notes.borrow_mut().extend(notes.iter().cloned());
        let (e, _, named, uncited) = chosen.swap_remove(pick);
        if descriptors.is_empty() {
            return Ok(named);
        }
        let list: Vec<String> = descriptors
            .iter()
            .map(|&(l, d)| {
                let bare = match d {
                    Descriptor::R | Descriptor::S => uncited.atoms,
                    Descriptor::E | Descriptor::Z => uncited.bonds,
                };
                if bare {
                    d.to_string()
                } else {
                    format!("{}{d}", loc_text(l))
                }
            })
            .collect();
        Ok(self
            .render(e, mode, region, &format!("({})-", list.join(",")))?
            .1)
    }

    /// The descriptors of the stereogenic units on the parent of `e`, with their locants, in
    /// ascending order of the locants (P-93); and notes on the units left without one.
    fn cite(&self, e: &Eval) -> (Vec<(u32, Descriptor)>, Vec<String>) {
        let mut pos = vec![None; self.g.len()];
        for (&a, &l) in e.atoms.iter().zip(&e.locs) {
            pos[a] = Some(l);
        }
        let mut cited = Vec::new();
        let mut notes = Vec::new();
        for u in self.stereo {
            let locant = match u.site {
                Site::Centre(c) => pos[c].ok_or("in a substituent group"),
                Site::Double(a, b) => match (pos[a], pos[b]) {
                    (Some(x), Some(y)) if x.abs_diff(y) == loc(1) => Ok(x.min(y)),
                    (Some(_), Some(_)) => Err("a double bond with a compound locant"),
                    _ => Err("in a substituent group"),
                },
                Site::Axis(..) => Err("an allene"),
            };
            let at = site_text(u.site, Some(&pos));
            match (&u.descriptor, locant) {
                (Ok(d), Ok(l)) => cited.push((l, *d)),
                (Ok(_), Err(why)) => notes.push(format!(
                    "{at}: {why}: descriptors are only given for the parent chain or ring"
                )),
                (Err(why), _) => notes.push(format!("{at}: {why}")),
            }
        }
        cited.sort_unstable_by_key(|c| c.0);
        (cited, notes)
    }

    /// The heteroatoms of a group carbon belong to the group; an amide's N-substituents and
    /// an ester's alkyl group are named separately.
    fn group_parts(
        &self,
        c: usize,
        region: &[bool],
        skip: &mut HashSet<(usize, usize)>,
        nprefix: &mut Vec<(Loc, usize, usize)>,
        rprime: &mut Option<(usize, usize)>,
    ) {
        let g = self.g;
        for &(q, o) in &g.adj[c] {
            if g.el[q] == El::C || !region[q] {
                continue;
            }
            skip.insert((c, q));
            if g.el[q] == El::N && o == 1 {
                for r in g.nbrs(q).filter(|&r| r != c && region[r]) {
                    nprefix.push((Loc::N, r, q));
                }
            }
            if g.el[q] == El::O && o == 1 && g.adj[q].len() == 2 {
                *rprime = g.nbrs(q).find(|&r| r != c).map(|r| (r, q));
            }
        }
    }

    /// The senior of ring parents that tie on principal groups: the most ring atoms, then the
    /// most multiple bonds (`Ring::rank`), then the most rings. Whether an assembly of junior
    /// rings beats a senior single ring is not decided (that is refused).
    fn senior_rings<'r>(
        &self,
        rings: Vec<&'r RingParent>,
    ) -> Result<Vec<&'r RingParent>, Unsupported> {
        let rank = |r: &RingParent| &self.p.rings[r.rings[0]].rank;
        let top = rings.iter().map(|r| rank(r)).max();
        let most = rings
            .iter()
            .filter(|r| Some(rank(r)) == top)
            .map(|r| r.rings.len())
            .max();
        if rings.iter().any(|r| Some(r.rings.len()) > most) {
            return Err(
                "seniority of a ring assembly against a larger or less saturated ring".into(),
            );
        }
        let senior: Vec<&RingParent> = rings
            .into_iter()
            .filter(|r| Some(rank(r)) == top && Some(r.rings.len()) == most)
            .collect();
        // Different von Baeyer systems of one rank (spiro[4.5]decane, bicyclo[4.2.2]decane):
        // the further criteria of P-44.2 are not implemented.
        let descriptor = |r: &RingParent| {
            self.p.rings[r.rings[0]]
                .poly
                .as_ref()
                .map(|p| p.descriptor.as_str())
        };
        if senior
            .iter()
            .any(|r| descriptor(r) != descriptor(senior[0]))
        {
            return Err("seniority between different bicyclic or spiro ring systems".into());
        }
        Ok(senior)
    }

    /// Every numbering of a ring parent. A carbocycle is numbered from any atom in either
    /// direction, a heterocycle in the ways its table entry allows. An assembly (P-28.3) is
    /// numbered ring by ring from one end, the rings getting unprimed, primed, double-primed …
    /// locants; each carbocycle is numbered from its bond to the previous ring (the first ring:
    /// to the next one), and an inner ring in the direction that gives its bond to the next
    /// ring the lower locant. Heterocycles keep their own numbering, and the bonds between them
    /// take the lowest locants (2,2'-bipyridine, 2,3'-bipyridine).
    fn ring_numberings(&self, rp: &RingParent) -> Vec<Numbering> {
        let rings = &self.p.rings;
        let walks = |ring: usize, start: usize| -> Vec<Vec<usize>> {
            let r = &rings[ring].atoms;
            let n = r.len();
            let s = r
                .iter()
                .position(|&a| a == start)
                .expect("atom of the ring");
            [1, n - 1]
                .map(|step| (0..n).map(|k| r[(s + k * step) % n]).collect())
                .into()
        };
        let first = &rings[rp.rings[0]];
        // A von Baeyer system: each of its numberings, with each of its Kekulé forms.
        if let Some(poly) = &first.poly {
            let shape = Shape::Poly { ring: rp.rings[0] };
            return poly
                .numberings
                .iter()
                .flat_map(|atoms| {
                    (0..poly.forms.len()).map(move |form| Numbering {
                        locs: (1..=atoms.len()).map(loc).collect(),
                        atoms: atoms.clone(),
                        shape,
                        joins: vec![],
                        attached: true,
                        form,
                    })
                })
                .collect();
        }
        let hetero = first.hetero.as_ref().map(|h| h.kind);
        if rp.rings.len() == 1 {
            let shape = Shape::Ring {
                benzene: first.benzene,
                hetero,
            };
            let all: Vec<Vec<usize>> = match &first.hetero {
                Some(h) => h.numberings.clone(),
                None => first
                    .atoms
                    .iter()
                    .flat_map(|&s| walks(rp.rings[0], s))
                    .collect(),
            };
            return all
                .into_iter()
                .map(|atoms| Numbering {
                    locs: (1..=atoms.len()).map(loc).collect(),
                    atoms,
                    shape,
                    joins: vec![],
                    attached: true,
                    form: 0,
                })
                .collect();
        }
        let shape = Shape::Assembly {
            rings: rp.rings.len(),
            benzene: first.benzene,
            hetero,
        };
        let mut out = Vec::new();
        for reverse in [false, true] {
            let (order, joins): (Vec<usize>, Vec<(usize, usize)>) = if reverse {
                (
                    rp.rings.iter().rev().copied().collect(),
                    rp.joins.iter().rev().map(|&(a, b)| (b, a)).collect(),
                )
            } else {
                (rp.rings.clone(), rp.joins.clone())
            };
            let k = order.len();
            // The ways of numbering each ring, given where its numbering starts.
            let options: Vec<Vec<Vec<usize>>> = (0..k)
                .map(|i| {
                    if let Some(h) = &rings[order[i]].hetero {
                        return h.numberings.clone();
                    }
                    let start = if i == 0 { joins[0].0 } else { joins[i - 1].1 };
                    let mut w = walks(order[i], start);
                    if i > 0 && i + 1 < k {
                        let exit = |walk: &Vec<usize>| walk.iter().position(|&a| a == joins[i].0);
                        let lowest = w.iter().map(exit).min().flatten();
                        w.retain(|walk| exit(walk) == lowest);
                    }
                    w
                })
                .collect();
            let mut partial: Vec<Vec<&Vec<usize>>> = vec![vec![]];
            for opts in &options {
                partial = partial
                    .into_iter()
                    .flat_map(|p| {
                        opts.iter().map(move |o| {
                            let mut p = p.clone();
                            p.push(o);
                            p
                        })
                    })
                    .collect();
            }
            for choice in partial {
                let mut atoms = Vec::new();
                let mut locs = Vec::new();
                let mut pos = HashMap::new();
                for (primes, walk) in choice.into_iter().enumerate() {
                    for (i, &a) in walk.iter().enumerate() {
                        let l = loc(i + 1) + primes as u32;
                        atoms.push(a);
                        locs.push(l);
                        pos.insert(a, l);
                    }
                }
                out.push(Numbering {
                    joins: joins.iter().map(|(a, b)| (pos[a], pos[b])).collect(),
                    atoms,
                    locs,
                    shape,
                    attached: true,
                    form: 0,
                });
            }
        }
        if hetero.is_some() {
            let join_locs = |n: &Numbering| {
                let mut l: Vec<u32> = n.joins.iter().flat_map(|&(a, b)| [a, b]).collect();
                l.sort_unstable();
                l
            };
            if let Some(lowest) = out.iter().map(join_locs).min() {
                out.retain(|n| join_locs(n) == lowest);
            }
        }
        out
    }

    fn evaluate(&self, num: Numbering, pgs: &[Pg], region: &[bool]) -> Eval {
        let g = self.g;
        let Numbering {
            atoms,
            locs,
            shape,
            joins,
            attached,
            form,
        } = num;
        let ring = shape != Shape::Chain;
        let mut pos = vec![0u32; g.len()];
        for (&a, &l) in atoms.iter().zip(&locs) {
            pos[a] = l;
        }
        let mut skip: HashSet<(usize, usize)> = HashSet::new();
        let mut suffix = Vec::new();
        let mut nprefix: Vec<(Loc, usize, usize)> = Vec::new();
        let mut rprime = None;

        for pg in pgs {
            match pg {
                Pg::InChain(c) => {
                    if !attached && pos[*c] > 0 {
                        suffix.push(pos[*c]);
                        self.group_parts(*c, region, &mut skip, &mut nprefix, &mut rprime);
                    } else if attached
                        && !self.p.in_ring[*c]
                        && let Some(a) = g.nbrs(*c).find(|&m| pos[m] > 0)
                    {
                        suffix.push(pos[a]);
                        skip.insert((a, *c));
                        self.group_parts(*c, region, &mut skip, &mut nprefix, &mut rprime);
                    }
                }
                Pg::OnAtom { anchors, group } => {
                    if let Some(&a) = anchors.iter().find(|&&a| pos[a] > 0) {
                        suffix.push(pos[a]);
                        if let Some(x) = *group {
                            skip.insert((a, x));
                            if self.p.amine[x] {
                                for r in g.nbrs(x).filter(|&r| r != a && region[r]) {
                                    nprefix.push((Loc::N, r, x));
                                }
                            }
                        }
                    }
                }
            }
        }
        suffix.sort_unstable();

        let mut mults = Vec::new();
        let mut mult_ends = Vec::new();
        // The heteroatoms of a von Baeyer system: low locants for all of them, then for O, S
        // and N in turn (P-23.3.2, P-24.2.4.1.2), before the suffixes.
        let mut hetero = Vec::new();
        // The multiple bonds of a von Baeyer system: as few compound locants as possible, then
        // the lower locants (those in parentheses ignored), then all locants (P-31.1.4.2).
        let poly_mult_key = |bonds: &[&(u32, u32, u8)]| -> Vec<u32> {
            let compound = bonds.iter().filter(|b| b.1 != b.0 + loc(1)).count();
            let mut all: Vec<u32> = bonds.iter().flat_map(|b| [b.0, b.1]).collect();
            all.sort_unstable();
            let mut key = vec![compound as u32];
            key.extend(bonds.iter().map(|b| b.0));
            key.extend(all);
            key
        };
        let mut poly_keys = None;
        if let Shape::Poly { ring } = shape {
            let poly = self.p.rings[ring]
                .poly
                .as_ref()
                .expect("a von Baeyer system");
            for el in [None, Some(El::O), Some(El::S), Some(El::N)] {
                hetero.extend(
                    atoms
                        .iter()
                        .zip(&locs)
                        .filter(|&(&a, _)| g.el[a] != El::C && el.is_none_or(|e| g.el[a] == e))
                        .map(|(_, &l)| l),
                );
            }
            let mut bonds: Vec<(u32, u32, u8)> = poly.forms[form]
                .iter()
                .filter(|&(_, &o)| o > 1)
                .map(|(&(a, b), &o)| (pos[a].min(pos[b]), pos[a].max(pos[b]), o))
                .collect();
            bonds.sort_unstable();
            mults = bonds.iter().map(|&(l, _, o)| (l, o)).collect();
            mult_ends = bonds.iter().map(|&(_, e, _)| e).collect();
            let all: Vec<&(u32, u32, u8)> = bonds.iter().collect();
            let doubles: Vec<&(u32, u32, u8)> = bonds.iter().filter(|b| b.2 == 2).collect();
            poly_keys = Some((poly_mult_key(&all), poly_mult_key(&doubles)));
        }
        // Benzene rings have no "ene" endings; an assembly's rings are benzene or saturated.
        if matches!(
            shape,
            Shape::Chain
                | Shape::Ring {
                    benzene: false,
                    hetero: None
                }
        ) {
            let n = atoms.len();
            let bonds = if ring { n } else { n.saturating_sub(1) };
            for i in 0..bonds {
                let o = g.order(atoms[i], atoms[(i + 1) % n]);
                if o > 1 {
                    mults.push((if i + 1 == n { loc(n) } else { locs[i] }, o));
                }
            }
            mults.sort_unstable();
        }

        let mut prefixes = Vec::new();
        for &a in &atoms {
            for q in g.nbrs(a) {
                if region[q] && pos[q] == 0 && !skip.contains(&(a, q)) {
                    prefixes.push((Loc::C(pos[a]), q, a));
                }
            }
        }
        let mut prefix_locs: Vec<u32> = prefixes
            .iter()
            .map(|(l, _, _)| match l {
                Loc::C(n) => *n,
                Loc::N => 0,
            })
            .collect();
        prefix_locs.sort_unstable();
        let nprefixes = prefixes.len() + nprefix.len();
        prefixes.extend(nprefix);

        let doubles = mults.iter().filter(|m| m.1 == 2).count();
        let (mult_locs, double_locs) = poly_keys.unwrap_or_else(|| {
            (
                mults.iter().map(|m| m.0).collect(),
                mults.iter().filter(|m| m.1 == 2).map(|m| m.0).collect(),
            )
        });
        let key = (
            Reverse(atoms.len()),
            Reverse(mults.len()),
            Reverse(doubles),
            hetero,
            suffix.clone(),
            mult_locs,
            double_locs,
            Reverse(nprefixes),
            prefix_locs,
        );
        Eval {
            atoms,
            locs,
            shape,
            joins,
            attached,
            suffix,
            mults,
            mult_ends,
            prefixes,
            rprime,
            key,
        }
    }

    /// The name of an evaluated candidate, with the locants in the order its prefixes are
    /// cited (the last tie-breaker), and the locants of the parent it leaves out.
    ///
    /// `stereo` (the stereodescriptors, "(2R)-") goes before the name of a molecule, after an
    /// ester's alkyl group: "ethyl (2R)-2-hydroxypropanoate".
    fn render(
        &self,
        e: &Eval,
        mode: Mode,
        region: &[bool],
        stereo: &str,
    ) -> Result<(Vec<Loc>, Prefix, Uncited), Unsupported> {
        let g = self.g;
        let suffix = match mode {
            Mode::Molecule => match self.p.principal {
                Some(c) if !e.suffix.is_empty() => Suffix::Class(c),
                _ => Suffix::None,
            },
            Mode::FreeValence { order, .. } => Suffix::Yl(order),
            Mode::Valences => Suffix::Yl(1),
            // The stems of the acyl halides: oxalyl, butanedioyl, benzene-1,4-dicarbonyl.
            Mode::Acyls => Suffix::Class(Class::AcidHalide),
            Mode::Acyl { .. } => Suffix::Acyl,
            Mode::Amido { .. } => Suffix::Class(Class::Amide),
        };
        // The acyl and acylamino groups of a ring cite its whole name before "carbonyl" or
        // "carboxamido" (P-16.5.1.4); those of chains ("propanoyl") and the retained "benzoyl"
        // and "benzamido" do not.
        let acyl_or_amido = matches!(mode, Mode::Acyl { .. } | Mode::Amido { .. });
        let has_n_prefix = e.prefixes.iter().any(|(l, _, _)| *l == Loc::N);
        if e.suffix.len() > 1
            && has_n_prefix
            && matches!(suffix, Suffix::Class(Class::Amine | Class::Amide))
        {
            return Err("N-substituted diamines/diamides (N,N′ locants)".into());
        }
        let mut named: Vec<(Loc, Prefix)> = Vec::new();
        for &(loc, q, from) in &e.prefixes {
            named.push((loc, self.sub(q, from)?));
        }
        let groups = group_prefixes(named);
        let special = match e.shape {
            Shape::Ring {
                hetero: Some(h), ..
            } => Some(hetero_name(e, h, suffix, &groups)),
            Shape::Poly { ring } => {
                let poly = self.p.rings[ring]
                    .poly
                    .as_ref()
                    .expect("a von Baeyer system");
                Some(poly_name(g, e, poly, suffix, &groups)?)
            }
            _ => None,
        };
        if let Some((mut text, depth, order)) = special {
            if mode == Mode::Molecule {
                text = self.with_ester(e, region, format!("{stereo}{text}"))?;
            }
            return Ok((
                order,
                Prefix {
                    text,
                    compound: !groups.is_empty(),
                    depth,
                    hydride: acyl_or_amido,
                    oxy: false,
                },
                Uncited::default(),
            ));
        }
        if let Shape::Assembly {
            rings,
            benzene,
            hetero,
        } = e.shape
        {
            let (mut text, depth, order) =
                assembly_name(e, rings, benzene, hetero, suffix, &groups)?;
            if mode == Mode::Molecule {
                text = self.with_ester(e, region, format!("{stereo}{text}"))?;
            }
            return Ok((
                order,
                Prefix {
                    text,
                    compound: !groups.is_empty(),
                    depth,
                    hydride: acyl_or_amido,
                    oxy: false,
                },
                Uncited::default(),
            ));
        }

        let len = e.atoms.len();
        let chain = e.shape == Shape::Chain;
        let ring = !chain;
        let benzene = matches!(e.shape, Shape::Ring { benzene: true, .. });
        let nsuf = e.suffix.len();
        let n_c_prefix = e.prefixes.iter().filter(|(l, _, _)| *l != Loc::N).count();
        let n_feat = nsuf + n_c_prefix;
        let terminal = !e.attached
            && (matches!(suffix, Suffix::Acyl)
                || matches!(suffix, Suffix::Class(c) if c.has_carbon()));
        // Formic acid, formate, formyl… take no prefix on their carbon (P-65.1.8): carboxy- and
        // cyanoformic acid are a hydrogen oxalate and carbonocyanidic acid.
        if chain && len == 1 && terminal && n_c_prefix > 0 {
            return Err("prefix on a one-carbon acid parent \
                        (carboxy- or cyanoformic acid, P-65.1.8)"
                .into());
        }
        let systematic = self.style == Style::Systematic;
        let retained_two = chain
            && len == 2
            && nsuf == 1
            && terminal
            && suffix != Suffix::Class(Class::Amide)
            && !systematic;
        // Every hydrogen of the parent replaced by one and the same prefix: hexachloroethane.
        let fully = matches!(suffix, Suffix::None | Suffix::Yl(_))
            && groups.len() == 1
            && !has_n_prefix
            && e.atoms.iter().all(|&a| g.h[a] == 0);
        let omit_c = (chain && len == 1)
            || retained_two
            || (chain && len == 2 && n_feat <= 1)
            || (ring && !benzene && n_feat + e.mults.len() <= 1)
            || (benzene && n_feat <= 1);
        let contracted =
            matches!(suffix, Suffix::Yl(_)) && e.suffix == [loc(1)] && e.mults.is_empty();
        // Benzene's retained names (phenol, benzoic acid…) cite no locant; "4-methylbenzene-1-
        // thiol" does.
        let retained_benzene_suffix =
            benzene && nsuf == 1 && suffix != Suffix::Class(Class::Thiol) && !systematic;
        let cite_suffix =
            !omit_c && !(chain && terminal) && !contracted && !retained_benzene_suffix;
        let cite_unsat = !(chain && len <= 2) && !(ring && n_feat + e.mults.len() <= 1);

        // The retained names for general nomenclature, of the unsubstituted acids only.
        let unsubstituted_chain = self.style == Style::Retained
            && mode == Mode::Molecule
            && chain
            && !e.attached
            && groups.is_empty()
            && e.mults.is_empty();
        let general = match (len, nsuf, suffix) {
            _ if !unsubstituted_chain => None,
            (3, 1, Suffix::Class(Class::Acid)) => Some("propionic acid"),
            (3, 1, Suffix::Class(Class::Ester | Class::Carboxylate)) => Some("propionate"),
            (3, 1, Suffix::Class(Class::AcidHalide)) => Some("propionyl"),
            (4, 2, Suffix::Class(Class::Acid)) => Some("succinic acid"),
            (4, 2, Suffix::Class(Class::Carboxylate)) => Some("succinate"),
            _ => None,
        };
        let core = match general {
            Some(name) => name.to_string(),
            None => self.core(
                len,
                ring,
                e.attached,
                benzene,
                &e.mults,
                suffix,
                &e.suffix,
                cite_suffix,
                cite_unsat,
                contracted,
            )?,
        };
        // With N-substituents present, carbon locants are cited to tell the two apart:
        // N,N-dimethyl-1-phenylmethanamine.
        let (prefix_text, depth, order) = compose(&groups, (omit_c || fully) && !has_n_prefix);

        let hyphen = if !prefix_text.is_empty() && core.starts_with(|c: char| c.is_ascii_digit()) {
            "-"
        } else {
            ""
        };
        let mut text = format!("{prefix_text}{hyphen}{core}");
        if self.style == Style::Per
            && mode == Mode::Molecule
            && fully
            && chain
            && len >= 2
            && let [(halo, _)] = &groups[..]
            && ["fluoro", "chloro", "bromo", "iodo"].contains(&halo.text.as_str())
        {
            text = format!("per{}{core}", halo.text);
        }
        let mut compound = !groups.is_empty();
        let mut oxy = (contracted && chain && len <= 4 && suffix == Suffix::Yl(1))
            || (benzene && suffix == Suffix::Yl(1));
        if mode == Mode::Molecule {
            if benzene
                && suffix == Suffix::None
                && !systematic
                && let Some(t) = retained_benzene(&groups)
            {
                text = t;
            }
            text = self.with_ester(e, region, format!("{stereo}{text}"))?;
        } else if text == "2-methylpropan-2-yl" {
            text = "tert-butyl".into();
            compound = false;
            oxy = true;
        } else if text == "phenylmethyl" {
            text = "benzyl".into();
            compound = false;
        }
        Ok((
            order,
            Prefix {
                text,
                compound,
                depth,
                hydride: acyl_or_amido && ring && (!benzene || systematic),
                oxy,
            },
            Uncited {
                atoms: (omit_c || fully) && !has_n_prefix,
                bonds: !cite_unsat,
            },
        ))
    }

    /// The functional class words around the parent name `text`: an ester's alkyl group
    /// ("ethyl propanoate"), an acyl halide's halides ("acetyl chloride", "butanedioyl bromide
    /// chloride", "propanedioyl dichloride": P-65.5.1, in alphabetical order).
    fn with_ester(&self, e: &Eval, region: &[bool], text: String) -> Result<String, Unsupported> {
        let (g, p) = (self.g, self.p);
        Ok(match (p.principal, e.rprime) {
            (Some(Class::Ester), Some((r, o))) if region[r] => {
                format!("{} {text}", self.sub(r, o)?.text)
            }
            (Some(Class::AcidHalide), _) if !e.suffix.is_empty() => {
                let groups: Vec<(usize, usize)> = (0..g.len())
                    .filter(|&c| region[c] && p.ctype[c] == Some(Class::AcidHalide))
                    .flat_map(|c| {
                        g.nbrs(c)
                            .filter(|&x| g.el[x].is_halogen())
                            .map(move |x| (c, x))
                    })
                    .collect();
                let mut halides: Vec<&str> =
                    groups.iter().map(|&(_, x)| g.el[x].halide_word()).collect();
                halides.sort_unstable();
                self.halides_interchangeable(&groups, &halides)?;
                let mut words = Vec::new();
                for w in &halides {
                    if !words.iter().any(|(v, _)| v == w) {
                        let k = halides.iter().filter(|v| *v == w).count();
                        words.push((*w, k));
                    }
                }
                let words: Vec<String> = words
                    .into_iter()
                    .map(|(w, k)| format!("{}{w}", multiplier(k)))
                    .collect();
                format!("{text} {}", words.join(" "))
            }
            _ => text,
        })
    }

    /// Different halides are cited without locants (P-65.5.1), so the name fits one structure
    /// only when every arrangement of the halides over the acyl halide `groups` (C=O carbon,
    /// halogen) is the same compound: here, when there are two kinds, one of them once, and a
    /// symmetry of the structure maps each group onto every other ("butanedioyl bromide
    /// chloride", "benzene-1,3,5-tricarbonyl bromide dichloride"). `halides` is sorted.
    fn halides_interchangeable(
        &self,
        groups: &[(usize, usize)],
        halides: &[&str],
    ) -> Result<(), Unsupported> {
        let g = self.g;
        let mut kinds = halides.to_vec();
        kinds.dedup();
        if kinds.len() < 2 {
            return Ok(());
        }
        let once = kinds
            .iter()
            .any(|k| halides.iter().filter(|h| *h == k).count() == 1);
        // Atoms by element, the halogens of the groups all alike.
        let kind: Vec<u8> = (0..g.len())
            .map(|a| {
                if groups.iter().any(|&(_, x)| x == a) {
                    u8::MAX
                } else {
                    g.el[a] as u8
                }
            })
            .collect();
        // The bonds of a benzene ring or of a fully unsaturated heterocycle, drawn in one of its
        // Kekulé forms, are all alike.
        let mut kekule = HashSet::new();
        for r in &self.p.rings {
            if r.benzene || r.hetero.as_ref().is_some_and(|h| h.kind.mancude) {
                let n = r.atoms.len();
                for i in 0..n {
                    let (x, y) = (r.atoms[i], r.atoms[(i + 1) % n]);
                    kekule.insert((x, y));
                    kekule.insert((y, x));
                }
            }
        }
        let bond = |x: usize, y: usize, o: u8| if kekule.contains(&(x, y)) { 4 } else { o };
        let not_interchangeable = || -> Unsupported {
            "different halides on acyl groups that are not interchangeable (P-65.5.1 cites \
             no locants for them)"
                .into()
        };
        if kinds.len() != 2 || !once {
            return Err(not_interchangeable());
        }
        // Refuse rather than guess when the symmetry search gives up.
        let c0 = groups[0].0;
        let mut undecided = false;
        for &(c, _) in groups {
            match g.automorphic(c0, c, &kind, bond) {
                Some(true) => {}
                Some(false) => return Err(not_interchangeable()),
                None => undecided = true,
            }
        }
        if undecided {
            return Err(
                "different halides on acyl groups whose symmetry is too costly to check \
                 (P-65.5.1 cites no locants for them)"
                    .into(),
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn core(
        &self,
        len: usize,
        ring: bool,
        attached: bool,
        benzene: bool,
        mults: &[(u32, u8)],
        suffix: Suffix,
        locs: &[u32],
        cite_suffix: bool,
        cite_unsat: bool,
        contracted: bool,
    ) -> Result<String, Unsupported> {
        let count = locs.len();
        let loc_list = || loc_list(locs);
        let systematic = self.style == Style::Systematic;
        if benzene {
            return Ok(match (suffix, count) {
                (Suffix::None, _) => "benzene".into(),
                (Suffix::Class(c), 1) if c != Class::Thiol && !systematic => match c {
                    Class::Acid => "benzoic acid",
                    Class::Ester | Class::Carboxylate => "benzoate",
                    Class::AcidHalide => "benzoyl",
                    Class::Amide => "benzamide",
                    Class::Nitrile => "benzonitrile",
                    Class::Aldehyde => "benzaldehyde",
                    Class::Alcohol => "phenol",
                    Class::Amine => "aniline",
                    Class::Ketone | Class::Thiol | Class::Anhydride => {
                        return Err("internal error: ketone or anhydride on benzene".into());
                    }
                }
                .into(),
                (Suffix::Class(c), _) => {
                    let word = multiplied(count, suffix_word(Suffix::Class(c), true));
                    // The final "e" elided before a vowel: "benzenol", "benzen-1-amine".
                    let base = if starts_with_vowel(&word) {
                        "benzen"
                    } else {
                        "benzene"
                    };
                    if cite_suffix {
                        format!("{base}-{}-{word}", loc_list())
                    } else {
                        format!("{base}{word}")
                    }
                }
                (Suffix::Yl(1), 1) => "phenyl".into(),
                // P-29.6.1: the retained 1,2-, 1,3- and 1,4-phenylene.
                (Suffix::Yl(1), 2) => format!("{}-phenylene", loc_list()),
                (Suffix::Yl(1), _) => format!("benzene-{}-{}", loc_list(), multiplied(count, "yl")),
                (Suffix::Acyl, _) if systematic => "benzenecarbonyl".into(),
                (Suffix::Acyl, _) => "benzoyl".into(),
                (Suffix::Yl(_), _) => return Err("internal error: ylidene on benzene".into()),
            });
        }
        let stem =
            stem(len).ok_or_else(|| format!("chain or ring longer than {MAX_NUMERAL} atoms"))?;
        let base = if ring { format!("cyclo{stem}") } else { stem };
        if !ring && count == 1 && (len == 1 || len == 2) && !systematic {
            let retained = match (len, suffix) {
                (1, Suffix::Class(Class::Acid)) => Some("formic acid"),
                (1, Suffix::Class(Class::Ester | Class::Carboxylate)) => Some("formate"),
                (1, Suffix::Class(Class::AcidHalide)) => Some("formyl"),
                (1, Suffix::Class(Class::Amide)) => Some("formamide"),
                (1, Suffix::Class(Class::Nitrile)) => Some("formonitrile"),
                (1, Suffix::Class(Class::Aldehyde)) => Some("formaldehyde"),
                (1, Suffix::Acyl) => Some("formyl"),
                (2, Suffix::Class(Class::Acid)) => Some("acetic acid"),
                (2, Suffix::Class(Class::Ester | Class::Carboxylate)) => Some("acetate"),
                (2, Suffix::Class(Class::AcidHalide)) => Some("acetyl"),
                (2, Suffix::Class(Class::Amide)) => Some("acetamide"),
                (2, Suffix::Class(Class::Nitrile)) => Some("acetonitrile"),
                (2, Suffix::Class(Class::Aldehyde)) => Some("acetaldehyde"),
                (2, Suffix::Acyl) => Some("acetyl"),
                _ => None,
            };
            if let Some(r) = retained {
                return Ok(r.into());
            }
        }
        if !ring && !attached && len == 2 && count == 2 && !systematic {
            match suffix {
                Suffix::Class(Class::Acid) => return Ok("oxalic acid".into()),
                Suffix::Class(Class::Carboxylate) => return Ok("oxalate".into()),
                // P-65.5.1: oxalyl is a retained preferred prefix.
                Suffix::Class(Class::AcidHalide) => return Ok("oxalyl".into()),
                _ => {}
            }
        }
        // P-29.6.1: "methylene" (not "methanediyl").
        if !ring && len == 1 && count == 2 && suffix == Suffix::Yl(1) {
            return Ok("methylene".into());
        }
        if contracted {
            let Suffix::Yl(order) = suffix else {
                unreachable!()
            };
            return Ok(format!(
                "{base}{}",
                ["yl", "ylidene", "ylidyne"][order as usize - 1]
            ));
        }
        let mut hydride = unsaturation(&base, mults, cite_unsat);
        if suffix == Suffix::None {
            return Ok(hydride);
        }
        let word = multiplied(count, suffix_word(suffix, ring || attached));
        if starts_with_vowel(&word) {
            hydride.pop();
        }
        Ok(if cite_suffix {
            format!("{hydride}-{}-{word}", loc_list())
        } else {
            format!("{hydride}{word}")
        })
    }

    /// The region of a substituent: everything reachable from `q` without passing `from`.
    fn region_from(&self, q: usize, from: usize) -> Vec<bool> {
        let mut region = vec![false; self.g.len()];
        region[q] = true;
        let mut stack = vec![q];
        while let Some(x) = stack.pop() {
            for y in self.g.nbrs(x) {
                if y != from && !region[y] {
                    region[y] = true;
                    stack.push(y);
                }
            }
        }
        region
    }

    /// The prefix for the substituent that starts at `q` and is attached to `from`.
    pub(crate) fn sub(&self, q: usize, from: usize) -> Result<Prefix, Unsupported> {
        if let Some(r) = self.cache.borrow().get(&(q, from)) {
            return r.clone();
        }
        let r = self.sub_with(q, from, self.g.order(q, from));
        self.cache.borrow_mut().insert((q, from), r.clone());
        r
    }

    /// The prefix for the substituent that starts at `q`, attached to `from` (or, [`NONE`],
    /// drawn on its own) by a bond of `order`.
    fn sub_with(&self, q: usize, from: usize, order: u8) -> Result<Prefix, Unsupported> {
        let (g, p) = (self.g, self.p);
        let others: Vec<usize> = g.nbrs(q).filter(|&m| m != from).collect();
        match g.el[q] {
            e if e.is_halogen() => Ok(Prefix::simple(e.halo_prefix())),
            El::O => {
                if order == 2 {
                    return Ok(Prefix::simple("oxo"));
                }
                match others.as_slice() {
                    [] => Ok(Prefix::simple("hydroxy")),
                    [r] => {
                        let alkyl = self.sub(*r, q)?;
                        Ok(if alkyl.oxy {
                            let stem = alkyl.text.strip_suffix("yl").unwrap_or(&alkyl.text);
                            Prefix {
                                text: format!("{stem}oxy"),
                                oxy: false,
                                ..alkyl
                            }
                        } else {
                            let (t, d) = alkyl.cited();
                            Prefix {
                                text: format!("{t}oxy"),
                                compound: true,
                                depth: d,
                                hydride: false,
                                oxy: false,
                            }
                        })
                    }
                    _ => Err("internal error: oxygen with three bonds".into()),
                }
            }
            // A ring nitrogen: the free valence of its ring (piperidin-1-yl).
            El::N if p.in_ring[q] => self.unit(
                Mode::FreeValence { atom: q, order },
                &self.region_from(q, from),
            ),
            El::N => {
                if p.nitro[q] {
                    return Ok(Prefix::simple("nitro"));
                }
                // An acylamino group: the amide's name ending in "amido" (perception refuses
                // imides, so there is one acyl group).
                if let Some(&c) = others.iter().find(|&&r| p.ctype[r] == Some(Class::Amide)) {
                    let amide = self.unit(Mode::Amido { atom: c }, &self.region_from(q, from))?;
                    let stem = amide
                        .text
                        .strip_suffix("amide")
                        .ok_or("internal error: amide name")?;
                    return Ok(Prefix {
                        text: format!("{stem}amido"),
                        ..amide
                    });
                }
                self.amino_like(q, &others, "amino")
            }
            El::C => match p.ctype[q] {
                Some(Class::Acid) => Ok(Prefix::simple("carboxy")),
                Some(Class::Nitrile) => Ok(Prefix::simple("cyano")),
                Some(Class::Amide) => {
                    // On a ring the prefix is "carbamoyl"; at the end of a chain IUPAC 2013 may
                    // prefer "amino…oxo" instead, which this namer does not decide.
                    if from == NONE || !p.in_ring[from] {
                        return Err("amide group as a prefix (carbamoyl on a chain)".into());
                    }
                    let n = g.nbrs(q).find(|&m| g.el[m] == El::N).unwrap();
                    let subs: Vec<usize> = g.nbrs(n).filter(|&m| m != q).collect();
                    self.amino_like(n, &subs, "carbamoyl")
                }
                Some(Class::Ester) => {
                    Err("ester group as a substituent (alkoxycarbonyl, acyloxy)".into())
                }
                // P-65.5.4: "carbonochloridoyl" on a ring, "chloro…oxo" at the end of a chain;
                // not decided here.
                Some(Class::AcidHalide) => {
                    Err("acid halide group as a prefix (carbonochloridoyl, chloro…oxo)".into())
                }
                Some(Class::Anhydride) => Err(
                    "anhydride group as a substituent (acyloxycarbonyl), or one together with \
                     a carboxylic acid"
                        .into(),
                ),
                Some(Class::Carboxylate) => {
                    Err("carboxylate group as a prefix (carboxylato)".into())
                }
                _ => {
                    let region = self.region_from(q, from);
                    let mode = if !p.in_ring[q] && order == 1 && self.has_carbonyl(q).is_some() {
                        Mode::Acyl { atom: q }
                    } else {
                        Mode::FreeValence { atom: q, order }
                    };
                    self.unit(mode, &region)
                }
            },
            El::S if order == 1 && others.is_empty() => Ok(Prefix::simple("sulfanyl")),
            _ => Err("internal error: unknown element".into()),
        }
    }

    /// "amino", "methylamino", "ethyl(methyl)amino", "dimethylcarbamoyl": `word` with the
    /// substituents `subs` of nitrogen `n` cited in front, without locants.
    fn amino_like(&self, n: usize, subs: &[usize], word: &str) -> Result<Prefix, Unsupported> {
        let mut named = Vec::new();
        for &r in subs {
            named.push((Loc::C(0), self.sub(r, n)?));
        }
        let groups = group_prefixes(named);
        let (text, depth, _) = compose(&groups, true);
        Ok(Prefix {
            text: format!("{text}{word}"),
            compound: !subs.is_empty(),
            depth,
            hydride: false,
            oxy: false,
        })
    }
}

/// Identical prefixes collected with their locants, in alphanumerical order.
fn group_prefixes(named: Vec<(Loc, Prefix)>) -> Vec<(Prefix, Vec<Loc>)> {
    let mut groups: Vec<(Prefix, Vec<Loc>)> = Vec::new();
    for (loc, prefix) in named {
        match groups.iter_mut().find(|(p, _)| p.text == prefix.text) {
            Some((_, locs)) => locs.push(loc),
            None => groups.push((prefix, vec![loc])),
        }
    }
    for (_, locs) in &mut groups {
        locs.sort();
    }
    groups.sort_by(|(a, la), (b, lb)| {
        (alpha_key(&a.text), &a.text, la).cmp(&(alpha_key(&b.text), &b.text, lb))
    });
    groups
}

/// Write the prefixes in front of a parent name. With `omit_c`, carbon locants are left out
/// (methane, acetic acid, a monosubstituted ring); a prefix following another one is then
/// enclosed to keep the reading unambiguous: dichloro(fluoro)methane, ethyl(methyl)amino.
fn compose(groups: &[(Prefix, Vec<Loc>)], omit_c: bool) -> (String, usize, Vec<Loc>) {
    let mut out = String::new();
    let mut depth = 0;
    let mut order = Vec::new();
    for (i, (prefix, locs)) in groups.iter().enumerate() {
        let cite = !omit_c || locs.contains(&Loc::N);
        let n = locs.len();
        let (mut body, mut d) = if n == 1 {
            prefix.cited()
        } else if prefix.compound {
            (
                format!(
                    "{}{}",
                    multiplier_complex(n),
                    enclose(&prefix.text, prefix.depth)
                ),
                prefix.depth + 1,
            )
        } else if prefix.needs_enclosure() {
            (
                format!("{}{}", multiplier(n), enclose(&prefix.text, prefix.depth)),
                prefix.depth + 1,
            )
        } else if prefix.text.starts_with("tert-") {
            (format!("{}-{}", multiplier(n), prefix.text), prefix.depth)
        } else {
            (format!("{}{}", multiplier(n), prefix.text), prefix.depth)
        };
        if !cite && i > 0 && d == 0 {
            body = enclose(&body, 0);
            d = 1;
        }
        if cite {
            if i > 0 {
                out.push('-');
            }
            out += &locs
                .iter()
                .map(Loc::to_string)
                .collect::<Vec<_>>()
                .join(",");
            out.push('-');
        }
        out += &body;
        depth = depth.max(d);
        order.extend(locs.iter().copied());
    }
    (out, depth, order)
}

/// Retained names of substituted benzenes that are preferred IUPAC names, for the
/// unsubstituted parents only (P-22.1.3, P-63.2.3): toluene, the xylenes, anisole.
fn retained_benzene(groups: &[(Prefix, Vec<Loc>)]) -> Option<String> {
    match groups {
        [(p, locs)] if p.text == "methyl" && locs.len() == 1 => Some("toluene".into()),
        [(p, locs)] if p.text == "methyl" && locs.len() == 2 => Some(format!(
            "{}-xylene",
            locs.iter()
                .map(Loc::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )),
        [(p, locs)] if p.text == "methoxy" && locs.len() == 1 => Some("anisole".into()),
        _ => None,
    }
}

/// The name of a ring assembly parent (P-28.2): "1,1'-biphenyl", "1,1':4',1''-terphenyl",
/// "1,1'-bi(cyclohexane)", with its prefixes and its suffix or free valence:
/// "4,4'-dichloro-1,1'-biphenyl", "[1,1'-biphenyl]-4-carboxylic acid", "[1,1'-biphenyl]-4-yl".
/// The brackets around the assembly do not count as enclosing marks of the prefix
/// ("([1,1'-biphenyl]-4-yl)", P-16.5.4).
fn assembly_name(
    e: &Eval,
    rings: usize,
    benzene: bool,
    hetero: Option<&HeteroRing>,
    suffix: Suffix,
    groups: &[(Prefix, Vec<Loc>)],
) -> Result<(String, usize, Vec<Loc>), Unsupported> {
    const MULT: [&str; 9] = [
        "bi", "ter", "quater", "quinque", "sexi", "septi", "octi", "novi", "deci",
    ];
    let joins = e
        .joins
        .iter()
        .map(|&(a, b)| format!("{},{}", loc_text(a), loc_text(b)))
        .collect::<Vec<_>>()
        .join(":");
    let mult = MULT
        .get(rings.wrapping_sub(2))
        .ok_or("internal error: ring assembly size")?;
    let ring = if benzene {
        "phenyl".to_string()
    } else if let Some(h) = hetero {
        h.name.to_string()
    } else {
        let size = e.atoms.len() / rings;
        format!(
            "(cyclo{}ane)",
            stem(size).ok_or_else(|| format!("ring larger than {MAX_NUMERAL} atoms"))?
        )
    };
    let hydride = format!("{joins}-{mult}{ring}");
    let (prefixes, depth, order) = compose(groups, false);
    let locs = loc_list(&e.suffix);
    let text = match suffix {
        Suffix::None if prefixes.is_empty() => hydride,
        Suffix::None => format!("{prefixes}-{hydride}"),
        // "[1,1'-bi(cyclohexan)]-4-ol"? The elision and enclosing marks are not settled here.
        _ if !benzene => {
            return Err(
                "suffix or free valence on a ring assembly of rings other than benzene".into(),
            );
        }
        Suffix::Class(Class::Ketone) | Suffix::Yl(2..) => {
            return Err("internal error: ketone or ylidene on benzene".into());
        }
        Suffix::Class(_) | Suffix::Acyl => format!(
            "{prefixes}[{hydride}]-{locs}-{}",
            multiplied(e.suffix.len(), suffix_word(suffix, true))
        ),
        Suffix::Yl(_) => format!(
            "{prefixes}[{hydride}]-{locs}-{}",
            multiplied(e.suffix.len(), "yl")
        ),
    };
    Ok((text, depth, order))
}

/// The name of a heterocyclic parent (P-22.2, P-31.1.4.2.4): "pyridine", "1H-pyrrole",
/// "2-methylpyridine", "1-methyl-1H-pyrrole", "pyridin-3-ol", "pyridine-3-carboxylic acid",
/// "piperidin-1-yl". Every locant is cited, even for a single prefix: the ring is not
/// homogeneous. The final "e" of the ring name is elided before a vowel.
fn hetero_name(
    e: &Eval,
    h: &HeteroRing,
    suffix: Suffix,
    groups: &[(Prefix, Vec<Loc>)],
) -> (String, usize, Vec<Loc>) {
    let indicated = if h.indicated { "1H-" } else { "" };
    let core = match suffix {
        Suffix::None => format!("{indicated}{}", h.name),
        _ => {
            let word = multiplied(e.suffix.len(), suffix_word(suffix, true));
            let mut stem = h.name;
            if starts_with_vowel(&word) {
                stem = stem.strip_suffix('e').unwrap_or(stem);
            }
            format!("{indicated}{stem}-{}-{word}", loc_list(&e.suffix))
        }
    };
    let (prefixes, depth, order) = compose(groups, false);
    let text = if prefixes.is_empty() {
        core
    } else if core.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{prefixes}-{core}")
    } else {
        format!("{prefixes}{core}")
    };
    (text, depth, order)
}

/// The name of a von Baeyer parent (P-23.2, P-24.2.1, P-31.1.4): the heteroatoms by
/// replacement prefixes in the order O, S, N ("4-thia-1-aza", P-23.3.2), the descriptor and
/// the hydride's stem — "bicyclo[3.2.0]heptane", "1,4-dioxaspiro[4.5]decane",
/// "bicyclo[4.2.0]oct-1(6)-ene" — with its prefixes and suffix: "bicyclo[2.2.1]heptan-2-one",
/// "spiro[4.5]decan-8-yl". Every locant is cited.
fn poly_name(
    g: &Graph,
    e: &Eval,
    poly: &graph::Poly,
    suffix: Suffix,
    groups: &[(Prefix, Vec<Loc>)],
) -> Result<(String, usize, Vec<Loc>), Unsupported> {
    let mut replacement = Vec::new();
    for (el, word) in [(El::O, "oxa"), (El::S, "thia"), (El::N, "aza")] {
        let locs: Vec<u32> = (0..e.atoms.len())
            .filter(|&i| g.el[e.atoms[i]] == el)
            .map(|i| loc(i + 1))
            .collect();
        if !locs.is_empty() {
            replacement.push(format!(
                "{}-{}{word}",
                loc_list(&locs),
                multiplier(locs.len())
            ));
        }
    }
    let stem = stem(e.atoms.len())
        .ok_or_else(|| format!("ring system larger than {MAX_NUMERAL} atoms"))?;
    let base = format!("{}{}{stem}", replacement.join("-"), poly.descriptor);
    let cited = |order: u8| -> Vec<String> {
        e.mults
            .iter()
            .zip(&e.mult_ends)
            .filter(|((_, o), _)| *o == order)
            .map(|(&(l, _), &end)| {
                if end == l + loc(1) {
                    loc_text(l)
                } else {
                    format!("{}({})", loc_text(l), loc_text(end))
                }
            })
            .collect()
    };
    let mut hydride = unsaturation_cited(&base, &cited(2), &cited(3), true);
    let core = match suffix {
        Suffix::None => hydride,
        Suffix::Acyl => return Err("internal error: acyl group on a ring".into()),
        _ => {
            let word = multiplied(e.suffix.len(), suffix_word(suffix, true));
            if starts_with_vowel(&word) {
                hydride.pop();
            }
            format!("{hydride}-{}-{word}", loc_list(&e.suffix))
        }
    };
    let (prefixes, depth, order) = compose(groups, false);
    let text = if prefixes.is_empty() {
        core
    } else if core.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{prefixes}-{core}")
    } else {
        format!("{prefixes}{core}")
    };
    Ok((text, depth, order))
}

/// The hydride name with its unsaturation: "butane", "but-2-ene", "buta-1,3-diene",
/// "but-1-en-3-yne", "cyclohexene".
fn unsaturation(base: &str, mults: &[(u32, u8)], cite: bool) -> String {
    let locs = |order: u8| -> Vec<String> {
        mults
            .iter()
            .filter(|m| m.1 == order)
            .map(|m| loc_text(m.0))
            .collect()
    };
    unsaturation_cited(base, &locs(2), &locs(3), cite)
}

/// The hydride name with double bonds at `enes` and triple bonds at `ynes` (their locants as
/// cited).
fn unsaturation_cited(base: &str, enes: &[String], ynes: &[String], cite: bool) -> String {
    if enes.is_empty() && ynes.is_empty() {
        return format!("{base}ane");
    }
    let join = |l: &[String]| l.join(",");
    let mut s = base.to_string();
    if enes.len() > 1 || (enes.is_empty() && ynes.len() > 1) {
        s.push('a');
    }
    if !enes.is_empty() {
        if cite {
            s += &format!("-{}-", join(enes));
        }
        s += &format!("{}ene", multiplier(enes.len()));
    }
    if !ynes.is_empty() {
        if !enes.is_empty() && ynes.len() == 1 {
            s.pop();
        }
        if cite {
            s += &format!("-{}-", join(ynes));
        }
        s += &format!("{}yne", multiplier(ynes.len()));
    }
    s
}

/// `count` suffixes: "diol", "tetrol", "hexamine"… The final "a" of the multiplying prefix is
/// elided before a suffix beginning with "a" or "o" (P-16.7.1(c)).
fn multiplied(count: usize, word: &str) -> String {
    let mut m = multiplier(count);
    if m.ends_with('a') && word.starts_with(['a', 'o']) {
        m.pop();
    }
    format!("{m}{word}")
}

fn suffix_word(suffix: Suffix, ring: bool) -> &'static str {
    match (suffix, ring) {
        (Suffix::Class(Class::Acid), false) => "oic acid",
        (Suffix::Class(Class::Acid), true) => "carboxylic acid",
        (Suffix::Class(Class::Carboxylate), false) => "oate",
        (Suffix::Class(Class::Carboxylate), true) => "carboxylate",
        (Suffix::Class(Class::AcidHalide), false) => "oyl",
        (Suffix::Class(Class::AcidHalide), true) => "carbonyl",
        (Suffix::Class(Class::Anhydride), _) => {
            unreachable!("anhydrides are named by functional class")
        }
        (Suffix::Class(Class::Ester), false) => "oate",
        (Suffix::Class(Class::Ester), true) => "carboxylate",
        (Suffix::Class(Class::Amide), false) => "amide",
        (Suffix::Class(Class::Amide), true) => "carboxamide",
        (Suffix::Class(Class::Nitrile), false) => "nitrile",
        (Suffix::Class(Class::Nitrile), true) => "carbonitrile",
        (Suffix::Class(Class::Aldehyde), false) => "al",
        (Suffix::Class(Class::Aldehyde), true) => "carbaldehyde",
        (Suffix::Class(Class::Ketone), _) => "one",
        (Suffix::Class(Class::Alcohol), _) => "ol",
        (Suffix::Class(Class::Thiol), _) => "thiol",
        (Suffix::Class(Class::Amine), _) => "amine",
        (Suffix::Acyl, false) => "oyl",
        (Suffix::Acyl, true) => "carbonyl",
        (Suffix::Yl(2), _) => "ylidene",
        (Suffix::Yl(3), _) => "ylidyne",
        (Suffix::Yl(_), _) => "yl",
        (Suffix::None, _) => "",
    }
}

/// Where a stereogenic unit is, for a note: by its locants on the parent ("C2", "the double
/// bond C2=C3"), or in words when it is not on the parent (or no parent is numbered).
fn site_text(site: Site, pos: Option<&[Option<u32>]>) -> String {
    let at = |a: usize| pos.and_then(|p| p[a]).map(|l| format!("C{}", loc_text(l)));
    match site {
        Site::Centre(c) => at(c).unwrap_or_else(|| "a stereocentre".into()),
        Site::Double(a, b) => match (at(a), at(b)) {
            (Some(x), Some(y)) => format!("the double bond {x}={y}"),
            _ => "a double bond".into(),
        },
        Site::Axis(a, b) => match (at(a), at(b)) {
            (Some(x), Some(y)) => format!("the allene {x}=C={y}"),
            _ => "an allene".into(),
        },
    }
}
