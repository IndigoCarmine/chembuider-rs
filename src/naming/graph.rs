//! The molecule as the namer sees it: one connected structure of heavy atoms with their
//! hydrogen counts, abbreviations expanded, plus the characteristic groups found in it.
//!
//! Everything the namer does not support is rejected here with a reason, so the naming code
//! itself only ever meets structures it knows how to name.

use std::cmp::Reverse;
use std::collections::BTreeMap;

use super::hetero::{self, Hetero};
use crate::config::Config;
use crate::label_chain::resolve_label;
use crate::molecule::elements::element_of_label;
use crate::molecule::{BondOrder, BondStereo, Molecule, charged_valence};
use crate::widget::rich_text::RichText;

/// The elements the namer handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum El {
    C,
    N,
    O,
    /// Sulfur: a ring atom of a heterocycle (thiophene, thiolane) or a thiol's –SH.
    S,
    F,
    Cl,
    Br,
    I,
}

impl El {
    fn from_symbol(s: &str) -> Option<El> {
        Some(match s {
            "C" => El::C,
            "N" => El::N,
            "O" => El::O,
            "S" => El::S,
            "F" => El::F,
            "Cl" => El::Cl,
            "Br" => El::Br,
            "I" => El::I,
            _ => return None,
        })
    }

    pub(crate) fn is_halogen(self) -> bool {
        matches!(self, El::F | El::Cl | El::Br | El::I)
    }

    /// The substituent prefix of a halogen atom.
    pub(crate) fn halo_prefix(self) -> &'static str {
        match self {
            El::F => "fluoro",
            El::Cl => "chloro",
            El::Br => "bromo",
            El::I => "iodo",
            _ => unreachable!("not a halogen"),
        }
    }

    /// The class name of a halide in a functional class name ("acetyl chloride").
    pub(crate) fn halide_word(self) -> &'static str {
        match self {
            El::F => "fluoride",
            El::Cl => "chloride",
            El::Br => "bromide",
            El::I => "iodide",
            _ => unreachable!("not a halogen"),
        }
    }
}

/// Rounds of colour refinement `Graph::automorphic` runs at most. Refinement only prunes the
/// exact mapping search, and a round costs a pass over the whole structure while a long chain
/// needs one round per pair of its atoms to settle.
pub(crate) const MAX_REFINEMENT_ROUNDS: usize = 64;

#[cfg(test)]
thread_local! {
    /// The most rounds one colour refinement ran on this thread, for tests to bound.
    pub(crate) static REFINEMENT_ROUNDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// One connected structure: heavy atoms, their charges and hydrogen counts, and bonds.
pub(crate) struct Graph {
    pub el: Vec<El>,
    pub charge: Vec<i8>,
    pub h: Vec<u8>,
    /// Neighbours with the bond order to each.
    pub adj: Vec<Vec<(usize, u8)>>,
    /// Free valences drawn as attachment points ("*"): the atom and the order of the bond to
    /// the point. A structure with any is named as a substituent group (P-29).
    pub free: Vec<(usize, u8)>,
    /// How the structure is drawn, for its stereodescriptors.
    pub drawn: Drawn,
}

/// The 2D drawing of a structure: where its atoms are and its stereo bonds. Atoms that an
/// abbreviation stands for ("Ph", "CO2Et") are not drawn; the abbreviation's own atom is drawn
/// where its label is.
#[derive(Clone, Debug, Default)]
pub(crate) struct Drawn {
    /// Where each heavy atom is drawn, in molecule units (y downward).
    pub pos: Vec<Option<[f32; 2]>>,
    /// The hydrogens drawn as atoms: the heavy atom each is on, and where it is drawn.
    pub h: Vec<(usize, Option<[f32; 2]>)>,
    /// The bonds drawn as wedges, hashes or wavy lines.
    pub marks: Vec<Mark>,
}

/// One end of a drawn bond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// A heavy atom.
    Atom(usize),
    /// A hydrogen drawn as an atom: an index into [`Drawn::h`].
    H(usize),
    /// An attachment point.
    Free,
}

/// What a stereo bond says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Wedge {
    /// A solid wedge: its wide end toward the viewer.
    Up,
    /// A hashed wedge: its wide end away from the viewer.
    Down,
    /// A wavy bond: the configuration is not specified.
    Wavy,
    /// A bold, hollow-wedged, plain hashed or dashed bond, which says nothing definite about a
    /// configuration (IUPAC 2006 graphical representation, ST-1.1).
    Unclear,
}

/// A stereo bond, from its narrow end (where a wedge starts) to its wide end.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mark {
    pub narrow: End,
    pub wide: End,
    pub wedge: Wedge,
}

impl Graph {
    pub(crate) fn len(&self) -> usize {
        self.el.len()
    }

    /// Order of the bond between `a` and `b` (0 when not bonded).
    pub(crate) fn order(&self, a: usize, b: usize) -> u8 {
        self.adj[a]
            .iter()
            .find(|(n, _)| *n == b)
            .map_or(0, |(_, o)| *o)
    }

    pub(crate) fn nbrs(&self, a: usize) -> impl Iterator<Item = usize> + '_ {
        self.adj[a].iter().map(|(n, _)| *n)
    }

    /// The bond orders on `a`, free valences included.
    fn bond_sum(&self, a: usize) -> i16 {
        let free = self.free.iter().filter(|(f, _)| *f == a);
        self.adj[a].iter().chain(free).map(|(_, o)| *o as i16).sum()
    }

    /// The number of free valences on `a`.
    pub(crate) fn free_on(&self, a: usize) -> usize {
        self.free.iter().filter(|(f, _)| *f == a).count()
    }

    /// Whether a symmetry of the structure (an automorphism) maps atom `a` to atom `b`, atoms
    /// being told apart by `kind` and by their charge, hydrogens and free valences, bonds by
    /// `bond` (from the two atoms and the drawn order: the bonds of a Kekulé ring all alike).
    /// Colour refinement prunes an exact backtracking search for the mapping. Colours that
    /// differ after any round of refinement already rule a mapping out, so refinement stops
    /// there, once the colours settle, or after `MAX_REFINEMENT_ROUNDS`; `None` when the search
    /// runs out of steps before it finds the answer.
    pub(crate) fn automorphic(
        &self,
        a: usize,
        b: usize,
        kind: &[u8],
        bond: impl Fn(usize, usize, u8) -> u8,
    ) -> Option<bool> {
        if a == b {
            return Some(true);
        }
        let n = self.len();
        let adj: Vec<Vec<(usize, u8)>> = (0..n)
            .map(|x| {
                self.adj[x]
                    .iter()
                    .map(|&(m, o)| (m, bond(x, m, o)))
                    .collect()
            })
            .collect();
        let order = |x: usize, y: usize| adj[x].iter().find(|e| e.0 == y).map_or(0, |e| e.1);
        let initial: Vec<_> = (0..n)
            .map(|x| {
                let mut free: Vec<u8> =
                    self.free.iter().filter(|f| f.0 == x).map(|f| f.1).collect();
                free.sort_unstable();
                (kind[x], self.charge[x], self.h[x], free)
            })
            .collect();
        let mut color = ranks(&initial);
        for _round in 0..MAX_REFINEMENT_ROUNDS {
            if color[a] != color[b] {
                return Some(false);
            }
            let signature: Vec<(usize, Vec<(u8, usize)>)> = (0..n)
                .map(|x| {
                    let mut s: Vec<(u8, usize)> =
                        adj[x].iter().map(|&(m, o)| (o, color[m])).collect();
                    s.sort_unstable();
                    (color[x], s)
                })
                .collect();
            #[cfg(test)]
            REFINEMENT_ROUNDS.set(REFINEMENT_ROUNDS.get().max(_round + 1));
            let next = ranks(&signature);
            let classes = |c: &[usize]| c.iter().max().map_or(0, |m| m + 1);
            let done = classes(&next) == classes(&color);
            color = next;
            if done {
                break;
            }
        }
        if color[a] != color[b] {
            return Some(false);
        }
        // Map the atoms in breadth-first order from `a`, each to a neighbour of the image of
        // the atom it was reached from (the structure is connected).
        let mut bfs = vec![a];
        let mut parent = vec![usize::MAX; n];
        let mut seen = vec![false; n];
        seen[a] = true;
        let mut i = 0;
        while i < bfs.len() {
            let x = bfs[i];
            for y in self.nbrs(x) {
                if !seen[y] {
                    seen[y] = true;
                    parent[y] = x;
                    bfs.push(y);
                }
            }
            i += 1;
        }
        let mut image = vec![usize::MAX; n];
        let mut used = vec![false; n];
        image[a] = b;
        used[b] = true;
        let candidates = |x: usize, image: &[usize], used: &[bool]| -> Vec<usize> {
            let (p, o) = (parent[x], order(parent[x], x));
            adj[image[p]]
                .iter()
                .filter(|&&(y, oy)| oy == o && !used[y] && color[y] == color[x])
                .map(|&(y, _)| y)
                .collect()
        };
        if bfs.len() == 1 {
            return Some(true);
        }
        // `options[k - 1]`: the images left to try for `bfs[k]`.
        let mut options = vec![candidates(bfs[1], &image, &used)];
        let mut k = 1;
        // Images left to try before the search gives up undecided.
        let mut steps = 64 * n.max(64);
        loop {
            let x = bfs[k];
            if image[x] != usize::MAX {
                used[image[x]] = false;
                image[x] = usize::MAX;
            }
            let Some(y) = options[k - 1].pop() else {
                options.pop();
                k -= 1;
                if k == 0 {
                    return Some(false);
                }
                continue;
            };
            if steps == 0 {
                return None;
            }
            steps -= 1;
            // Every bond of x to a mapped atom must have its image, and y no other bond to a
            // mapped atom.
            let mapped = adj[x].iter().filter(|&&(m, _)| image[m] != usize::MAX);
            let fits = mapped.clone().all(|&(m, o)| order(image[m], y) == o)
                && self.nbrs(y).filter(|&m| used[m]).count() == mapped.count();
            if !fits {
                continue;
            }
            image[x] = y;
            used[y] = true;
            k += 1;
            if k == bfs.len() {
                return Some(true);
            }
            options.push(candidates(bfs[k], &image, &used));
        }
    }

    /// The net charge of the structure.
    pub(crate) fn net_charge(&self) -> i32 {
        self.charge.iter().map(|&c| c as i32).sum()
    }

    /// The structure on the far side of the bond `from`–`c` (everything reachable from `c`
    /// without passing `from`), with a hydroxy group on `c` in place of `from`: the acid of
    /// one acyl half of an anhydride.
    pub(crate) fn acid_half(&self, c: usize, from: usize) -> Graph {
        let mut index = vec![usize::MAX; self.len()];
        let mut atoms = vec![c];
        index[c] = 0;
        let mut k = 0;
        while k < atoms.len() {
            let x = atoms[k];
            k += 1;
            for y in self.nbrs(x) {
                if y != from && index[y] == usize::MAX {
                    index[y] = atoms.len();
                    atoms.push(y);
                }
            }
        }
        let mut adj: Vec<Vec<(usize, u8)>> = atoms
            .iter()
            .map(|&x| {
                self.adj[x]
                    .iter()
                    .filter(|(y, _)| *y != from)
                    .map(|&(y, o)| (index[y], o))
                    .collect()
            })
            .collect();
        let oh = atoms.len();
        adj[0].push((oh, 1));
        adj.push(vec![(0, 1)]);
        let mut el: Vec<El> = atoms.iter().map(|&x| self.el[x]).collect();
        let mut charge: Vec<i8> = atoms.iter().map(|&x| self.charge[x]).collect();
        let mut h: Vec<u8> = atoms.iter().map(|&x| self.h[x]).collect();
        el.push(El::O);
        charge.push(0);
        h.push(1);
        Graph {
            el,
            charge,
            h,
            adj,
            free: Vec::new(),
            drawn: Drawn::default(),
        }
    }
}

/// The principal characteristic groups the namer knows, least senior first, so that the
/// derived `Ord` is seniority (P-41: anions > acids > anhydrides > esters > acid halides >
/// amides > nitriles > aldehydes > ketones > alcohols > thiols > amines).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    Amine,
    /// –SH: with the alcohols in class 17 of P-41, after them (O > S, P-63.1.5).
    Thiol,
    Alcohol,
    Ketone,
    Aldehyde,
    Nitrile,
    Amide,
    /// Acyl halides: "acetyl chloride", "butanedioyl dichloride".
    AcidHalide,
    Ester,
    /// Acyclic anhydrides of carboxylic acids, –C(=O)–O–C(=O)–: named by functional class
    /// ("propanoic anhydride"), never as a suffix.
    Anhydride,
    Acid,
    /// Carboxylate anions, –C(=O)O⁻: "propanoate", the anion of a salt.
    Carboxylate,
}

impl Class {
    /// Groups whose carbon atom belongs to the group (and so ends a chain, or is expressed by
    /// a "-carbo…" suffix on a ring): acids and their anions, esters, acid halides, amides,
    /// nitriles and aldehydes.
    pub(crate) fn has_carbon(self) -> bool {
        matches!(
            self,
            Class::Acid
                | Class::Carboxylate
                | Class::Ester
                | Class::AcidHalide
                | Class::Amide
                | Class::Nitrile
                | Class::Aldehyde
        )
    }
}

/// What perception found in a [`Graph`].
pub(crate) struct Perceived {
    /// For a carbon that carries an acid, ester, amide, nitrile or aldehyde group: which one.
    pub ctype: Vec<Option<Class>>,
    /// Ketone carbons (a C=O carbon bonded to two carbons, or in the ring).
    pub ketone: Vec<bool>,
    /// Hydroxy oxygens on a non-acyl carbon (alcohols and phenols).
    pub hydroxyl: Vec<bool>,
    /// Sulfanyl sulfurs (–SH) on a non-acyl carbon.
    pub thiol: Vec<bool>,
    /// Amine nitrogens (not amide, nitrile or nitro nitrogens).
    pub amine: Vec<bool>,
    /// Nitro nitrogens.
    pub nitro: Vec<bool>,
    /// The rings, each a monocycle; no two share an atom.
    pub rings: Vec<Ring>,
    /// The ring parents the structure may be named from: every ring on its own, except that
    /// identical rings joined by single bonds form one ring assembly (P-28).
    pub parents: Vec<RingParent>,
    pub in_ring: Vec<bool>,
    /// The most senior class present.
    pub principal: Option<Class>,
}

/// A ring system of a structure: a monocycle, or two rings sharing atoms ([`Poly`]).
pub(crate) struct Ring {
    /// Its atoms: a monocycle's in cyclic order, a bicycle's or spiro system's in the order of
    /// one of its numberings.
    pub atoms: Vec<usize>,
    /// A benzene ring (six carbons, alternating double bonds).
    pub benzene: bool,
    /// For a heterocycle: which one, and how it may be numbered.
    pub hetero: Option<Hetero>,
    /// For two rings sharing atoms: the von Baeyer system.
    pub poly: Option<Poly>,
    /// Its seniority as a parent.
    pub rank: Rank,
}

/// A ring system of two rings sharing atoms, named by von Baeyer nomenclature: a bicycle
/// ("bicyclo[3.2.0]heptane", P-23.2) or a monospiro system of two monocycles ("spiro[4.5]decane",
/// P-24.2.1), its heteroatoms cited by replacement ('a') prefixes (P-23.3, P-24.2.4).
pub(crate) struct Poly {
    /// "bicyclo[3.2.0]" or "spiro[4.5]".
    pub descriptor: String,
    /// Every numbering the system's rules allow (P-23.2.3, P-24.2.1), each its atoms in the
    /// order of their locants; the other criteria choose among them.
    pub numberings: Vec<Vec<usize>>,
    /// The bond orders of the system's bonds, keyed by (lower, higher atom index): as drawn,
    /// then every other Kekulé form of the same compound (a benzene ring of
    /// bicyclo[4.1.0]hepta-1,3,5-triene drawn either way).
    pub forms: Vec<BTreeMap<(usize, usize), u8>>,
}

/// The seniority of a ring as a parent, greatest first (P-44.2.1.2, then P-44.4.1.1): a
/// nitrogenous heterocycle; without nitrogen, a heteroatom early in the order O > S; more
/// rings; more ring atoms; more heteroatoms; more kinds of heteroatom; more O, then S, then N atoms; lower
/// locants for the heteroatoms; then more multiple bonds (benzene counts three) and more
/// double bonds. Carbocycles tie on every heteroatom criterion.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Rank {
    nitrogen: bool,
    first_hetero: u8,
    rings: usize,
    size: usize,
    heteroatoms: usize,
    variety: usize,
    by_element: [usize; 3],
    hetero_locants: Reverse<Vec<usize>>,
    pub multiple: usize,
    double: usize,
}

/// A ring parent: one ring, or an unbranched assembly of identical rings joined by single
/// bonds (1,1'-biphenyl, 1,1'-bi(cyclohexane), 1,1':4',1''-terphenyl).
pub(crate) struct RingParent {
    /// Indices into [`Perceived::rings`], from one end of the assembly to the other.
    pub rings: Vec<usize>,
    /// The bond from each ring to the next: (atom in `rings[i]`, atom in `rings[i + 1]`).
    pub joins: Vec<(usize, usize)>,
    /// All its atoms.
    pub atoms: Vec<usize>,
}

/// The most rings an assembly may have: its multiplying prefixes run from "bi" to "deci".
pub(crate) const MAX_ASSEMBLY: usize = 10;

/// A reason a structure cannot be named.
pub(crate) type Unsupported = String;

/// Each value's rank among the distinct values of `v` (equal values, equal ranks).
fn ranks<T: Ord + Clone>(v: &[T]) -> Vec<usize> {
    let mut sorted = v.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    v.iter()
        .map(|x| sorted.binary_search(x).expect("value of v"))
        .collect()
}

/// Split `mol` into its connected structures, each as a list of atom indices into `mol.atoms`.
/// Hydrogens drawn on their own stay with the structure they are bonded to; a structure that
/// is nothing but a page object never appears (page objects are not atoms).
pub(crate) fn components(mol: &Molecule) -> Vec<Vec<usize>> {
    let index: std::collections::HashMap<u32, usize> = mol
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let mut parent: Vec<usize> = (0..mol.atoms.len()).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    for b in &mol.bonds {
        if let (Some(&a), Some(&c)) = (index.get(&b.begin), index.get(&b.end)) {
            let (ra, rc) = (find(&mut parent, a), find(&mut parent, c));
            if ra != rc {
                parent[ra] = rc;
            }
        }
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut root_slot: std::collections::HashMap<usize, usize> = Default::default();
    for i in 0..mol.atoms.len() {
        let r = find(&mut parent, i);
        let slot = *root_slot.entry(r).or_insert_with(|| {
            groups.push(Vec::new());
            groups.len() - 1
        });
        groups[slot].push(i);
    }
    groups
}

/// Atoms and bonds by label, hydrogens still explicit, while abbreviations are expanded.
pub(crate) struct Raw {
    pub label: Vec<String>,
    pub charge: Vec<i8>,
    /// Both ends (indices into `label`) and the order.
    pub bonds: Vec<(usize, usize, u8)>,
    /// Where each atom is drawn (`None`: an atom an abbreviation stands for).
    pos: Vec<Option<[f32; 2]>>,
    /// The stereo bonds: begin (a wedge's narrow end), end, and what they say.
    marks: Vec<(usize, usize, Wedge)>,
}

/// What a bond's stereo style says about a configuration, if anything.
fn wedge_of(stereo: &BondStereo) -> Option<Wedge> {
    match stereo {
        BondStereo::None | BondStereo::Dative => None,
        BondStereo::WedgeUp => Some(Wedge::Up),
        BondStereo::WedgeDown => Some(Wedge::Down),
        BondStereo::Wavy => Some(Wedge::Wavy),
        BondStereo::Bold | BondStereo::Dashed | BondStereo::Hashed | BondStereo::HollowWedge => {
            Some(Wedge::Unclear)
        }
    }
}

fn order_value(o: &BondOrder) -> u8 {
    o.valence() as u8
}

/// A label's text without its rich-text markup ("CO_2Et" → "CO2Et").
fn plain_text(label: &str) -> String {
    RichText::parse(label)
        .spans
        .into_iter()
        .map(|s| s.text)
        .collect()
}

/// The atoms and bonds of one connected structure (`atoms` index `mol.atoms`), every
/// abbreviation expanded into the atoms it stands for; [`to_graph`] makes its graph.
pub(crate) fn expand(mol: &Molecule, atoms: &[usize], config: &Config) -> Result<Raw, Unsupported> {
    let local: std::collections::HashMap<u32, usize> = atoms
        .iter()
        .enumerate()
        .map(|(k, &i)| (mol.atoms[i].id, k))
        .collect();
    let mut raw = Raw {
        label: atoms
            .iter()
            .map(|&i| mol.atoms[i].element.clone())
            .collect(),
        charge: atoms.iter().map(|&i| mol.atoms[i].charge).collect(),
        bonds: Vec::new(),
        // A hydrogen that came with a group label is drawn folded into the label, not where
        // its atom is.
        pos: atoms
            .iter()
            .map(|&i| (!mol.atoms[i].group_h).then_some(mol.atoms[i].pos))
            .collect(),
        marks: Vec::new(),
    };
    for b in &mol.bonds {
        if let (Some(&a), Some(&c)) = (local.get(&b.begin), local.get(&b.end))
            && a != c
        {
            raw.bonds.push((a, c, order_value(&b.order)));
            if let Some(w) = wedge_of(&b.stereo) {
                raw.marks.push((a, c, w));
            }
        }
    }
    expand_labels(&mut raw, config)?;
    Ok(raw)
}

/// Replace every abbreviation ("Ph", "OMe", "CO2Et", …) by the atoms it stands for.
fn expand_labels(raw: &mut Raw, config: &Config) -> Result<(), Unsupported> {
    // A fragment may itself contain labels; a few rounds settle any sane nesting.
    for _ in 0..8 {
        let Some(i) = (0..raw.label.len()).find(|&i| !is_atom_label(&raw.label[i])) else {
            return Ok(());
        };
        let label = raw.label[i].clone();
        let frag = resolve_label(config, &label)
            .or_else(|| {
                let plain = plain_text(&label);
                (plain != label)
                    .then(|| resolve_label(config, &plain))
                    .flatten()
            })
            .ok_or_else(|| format!("unknown label \u{201c}{label}\u{201d}"))?;
        let root = &frag.atoms[frag.attach_idx];
        raw.label[i] = root.element.clone();
        raw.charge[i] = root.charge;
        let mut map = vec![i; frag.atoms.len()];
        for (k, fa) in frag.atoms.iter().enumerate() {
            if k != frag.attach_idx {
                map[k] = raw.label.len();
                raw.label.push(fa.element.clone());
                raw.charge.push(fa.charge);
                raw.pos.push(None);
            }
        }
        for fb in &frag.bonds {
            raw.bonds
                .push((map[fb.begin], map[fb.end], order_value(&fb.order)));
        }
    }
    match raw.label.iter().find(|l| !is_atom_label(l)) {
        Some(l) => Err(format!("unknown label \u{201c}{l}\u{201d}")),
        None => Ok(()),
    }
}

/// An attachment point of a substituent group drawn on its own.
pub(crate) const ATTACHMENT: &str = "*";

/// A label that stands for one atom: an element or an attachment point.
fn is_atom_label(label: &str) -> bool {
    label == ATTACHMENT || element_of_label(label).is_some()
}

/// Fold the hydrogens into their heavy atoms and check every element is one the namer knows.
pub(crate) fn to_graph(raw: &Raw) -> Result<Graph, Unsupported> {
    let n = raw.label.len();
    let mut explicit_h = vec![0u8; n];
    let mut sum = vec![0i16; n];
    let mut heavy_index = vec![usize::MAX; n];
    let mut el = Vec::new();
    let mut charge = Vec::new();
    for (i, label) in raw.label.iter().enumerate() {
        if label == "H" || label == ATTACHMENT {
            continue;
        }
        // A structure with a metal is the inorganic namer's (`inorganic.rs`).
        let e =
            El::from_symbol(label).ok_or_else(|| format!("element {label} is not supported"))?;
        heavy_index[i] = el.len();
        el.push(e);
        charge.push(raw.charge[i]);
    }
    if el.is_empty() {
        return Err("no carbon atom".into());
    }
    let mut adj: Vec<Vec<(usize, u8)>> = vec![Vec::new(); el.len()];
    let mut free = Vec::new();
    for &(a, b, o) in &raw.bonds {
        sum[a] += o as i16;
        sum[b] += o as i16;
        let star = |x: usize| raw.label[x] == ATTACHMENT;
        if star(a) || star(b) {
            let (pt, atom) = if star(a) { (a, b) } else { (b, a) };
            if star(atom) || raw.label[atom] == "H" || raw.charge[pt] != 0 {
                return Err("attachment point not on a heavy atom".into());
            }
            free.push((heavy_index[atom], o));
            continue;
        }
        match (raw.label[a] == "H", raw.label[b] == "H") {
            (true, true) => return Err("hydrogen bonded to hydrogen".into()),
            (true, false) | (false, true) => {
                let (hx, heavy) = if raw.label[a] == "H" { (a, b) } else { (b, a) };
                if o != 1 || raw.charge[hx] != 0 {
                    return Err("unusual hydrogen".into());
                }
                explicit_h[heavy] += 1;
            }
            (false, false) => {
                if o > 3 {
                    return Err("quadruple bond".into());
                }
                let (x, y) = (heavy_index[a], heavy_index[b]);
                if adj[x].iter().any(|(m, _)| *m == y) {
                    return Err("duplicate bond".into());
                }
                adj[x].push((y, o));
                adj[y].push((x, o));
            }
        }
    }
    // Every drawn hydrogen and attachment point must hang on exactly one heavy atom.
    for i in 0..n {
        if raw.label[i] == "H" || raw.label[i] == ATTACHMENT {
            let bonds = raw
                .bonds
                .iter()
                .filter(|(a, b, _)| *a == i || *b == i)
                .count();
            if bonds != 1 {
                return Err(if raw.label[i] == "H" {
                    "unusual hydrogen"
                } else {
                    "attachment point with other than one bond"
                }
                .into());
            }
        }
    }
    let mut drawn = Drawn {
        pos: (0..n)
            .filter(|&i| heavy_index[i] != usize::MAX)
            .map(|i| raw.pos[i])
            .collect(),
        ..Drawn::default()
    };
    let mut h_index = vec![usize::MAX; n];
    for &(a, b, _) in &raw.bonds {
        for (x, y) in [(a, b), (b, a)] {
            if raw.label[x] == "H" && heavy_index[y] != usize::MAX {
                h_index[x] = drawn.h.len();
                drawn.h.push((heavy_index[y], raw.pos[x]));
            }
        }
    }
    let end = |i: usize| {
        if heavy_index[i] != usize::MAX {
            End::Atom(heavy_index[i])
        } else if h_index[i] != usize::MAX {
            End::H(h_index[i])
        } else {
            End::Free
        }
    };
    drawn.marks = raw
        .marks
        .iter()
        .map(|&(a, b, wedge)| Mark {
            narrow: end(a),
            wide: end(b),
            wedge,
        })
        .collect();
    let mut h = vec![0u8; el.len()];
    for i in 0..n {
        let k = heavy_index[i];
        if k == usize::MAX {
            continue;
        }
        let valence = charged_valence(&raw.label[i], raw.charge[i]).unwrap_or(0);
        let implicit = if sum[i] == 0 {
            valence
        } else {
            (valence - sum[i]).max(0)
        };
        h[k] = explicit_h[i] + implicit as u8;
    }
    Ok(Graph {
        el,
        charge,
        h,
        adj,
        free,
        drawn,
    })
}

/// Find the rings and the characteristic groups, rejecting anything outside the supported
/// classes.
pub(crate) fn perceive(g: &Graph) -> Result<Perceived, Unsupported> {
    let n = g.len();
    if !g.el.contains(&El::C) {
        return Err("no carbon atom".into());
    }
    let (cycles, systems, in_ring) = find_rings(g)?;
    let mut rings: Vec<Ring> = Vec::new();
    for atoms in cycles {
        let k = atoms.len();
        let orders: Vec<u8> = (0..k)
            .map(|i| g.order(atoms[i], atoms[(i + 1) % k]))
            .collect();
        let hetero = if atoms.iter().any(|&a| g.el[a] != El::C) {
            Some(hetero::recognise(g, &atoms)?)
        } else {
            None
        };
        let benzene = hetero.is_none()
            && k == 6
            && atoms
                .iter()
                .all(|&a| atoms.iter().filter(|&&b| g.order(a, b) == 2).count() == 1);
        let (multiple, double) = if benzene {
            (3, 3)
        } else {
            (
                orders.iter().filter(|&&o| o > 1).count(),
                orders.iter().filter(|&&o| o == 2).count(),
            )
        };
        let hetero_locants = hetero
            .as_ref()
            .map_or_else(Vec::new, |h| h.kind.hetero_locants());
        rings.push(Ring {
            rank: rank(g, &atoms, 1, hetero_locants, multiple, double),
            atoms,
            benzene,
            hetero,
            poly: None,
        });
    }
    for poly in systems {
        let atoms = poly.numberings[0].clone();
        let drawn = &poly.forms[0];
        let multiple = drawn.values().filter(|&&o| o > 1).count();
        let double = drawn.values().filter(|&&o| o == 2).count();
        let hetero_locants = poly
            .numberings
            .iter()
            .map(|num| {
                (1..=num.len())
                    .filter(|&l| g.el[num[l - 1]] != El::C)
                    .collect::<Vec<_>>()
            })
            .min()
            .unwrap_or_default();
        rings.push(Ring {
            rank: rank(g, &atoms, 2, hetero_locants, multiple, double),
            atoms,
            benzene: false,
            hetero: None,
            poly: Some(poly),
        });
    }
    let parents = ring_parents(g, &rings)?;
    let mut ring_of = vec![usize::MAX; n];
    for (i, r) in rings.iter().enumerate() {
        for &a in &r.atoms {
            ring_of[a] = i;
        }
    }
    // A bond of a ring (rather than one leaving it); the heterocycle checks vouch for these.
    let ring_bond = |a: usize, b: usize| ring_of[a] != usize::MAX && ring_of[a] == ring_of[b];

    // Nitro groups first: they are the only charged atoms allowed, and their nitrogen may be
    // drawn with five bonds.
    let mut nitro = vec![false; n];
    let mut nitro_o = vec![false; n];
    for a in (0..n).filter(|&a| g.el[a] == El::N) {
        let os: Vec<(usize, u8)> = g.adj[a]
            .iter()
            .copied()
            .filter(|&(m, _)| g.el[m] == El::O)
            .collect();
        let cs: Vec<(usize, u8)> = g.adj[a]
            .iter()
            .copied()
            .filter(|&(m, _)| g.el[m] == El::C)
            .collect();
        if os.len() != 2 || cs.len() != 1 || g.adj[a].len() != 3 || cs[0].1 != 1 || g.free_on(a) > 0
        {
            continue;
        }
        if os
            .iter()
            .any(|&(o, _)| g.adj[o].len() != 1 || g.free_on(o) > 0)
        {
            continue;
        }
        let charged = g.charge[a] == 1
            && os.iter().any(|&(o, ord)| ord == 2 && g.charge[o] == 0)
            && os.iter().any(|&(o, ord)| ord == 1 && g.charge[o] == -1);
        let neutral = g.charge[a] == 0 && os.iter().all(|&(o, ord)| ord == 2 && g.charge[o] == 0);
        if charged || neutral {
            nitro[a] = true;
            for (o, _) in os {
                nitro_o[o] = true;
            }
        }
    }
    // Carboxylate anions: the O⁻ of a –C(=O)O⁻ group.
    let mut carboxylate_o = vec![false; n];
    for a in (0..n).filter(|&a| g.el[a] == El::O && g.charge[a] == -1) {
        if let [(c, 1)] = g.adj[a][..]
            && g.el[c] == El::C
            && !in_ring[c]
            && g.free_on(a) == 0
            && g.adj[c]
                .iter()
                .any(|&(m, o)| o == 2 && g.el[m] == El::O && g.charge[m] == 0)
        {
            carboxylate_o[a] = true;
        }
    }
    for a in 0..n {
        if nitro[a] || nitro_o[a] || carboxylate_o[a] {
            continue;
        }
        if g.charge[a] != 0 {
            return Err("charged atoms (ions, salts, zwitterions)".into());
        }
        let valence = match g.el[a] {
            El::C => 4,
            El::N => 3,
            El::O | El::S => 2,
            _ => 1,
        };
        if g.bond_sum(a) > valence {
            return Err("an atom has more bonds than its valence allows".into());
        }
    }

    let mut ctype: Vec<Option<Class>> = vec![None; n];
    let mut ketone = vec![false; n];
    let mut hydroxyl = vec![false; n];
    let mut thiol = vec![false; n];
    let mut amine = vec![false; n];

    // Heteroatoms that may appear only in certain places.
    for a in 0..n {
        match g.el[a] {
            e if e.is_halogen() => {
                if g.adj[a].len() != 1 || g.el[g.adj[a][0].0] != El::C || g.adj[a][0].1 != 1 {
                    return Err("halogen not bonded to carbon".into());
                }
            }
            // A ring oxygen or sulfur has only its two ring bonds, which the heterocycle vouches
            // for (the O–N bond of 1,2-oxazole).
            El::O | El::S if in_ring[a] => {}
            // Outside a ring, sulfur is only a thiol's –SH.
            El::S => {
                if g.free_on(a) > 0 {
                    return Err("free valence on sulfur (sulfanyl substituent groups)".into());
                }
                const ONLY_THIOL: &str = "sulfur outside a ring is supported only as a thiol";
                match g.adj[a][..] {
                    [(c, 1)] if g.el[c] == El::C => thiol[a] = true,
                    [(c, 2)] if g.el[c] == El::C => {
                        return Err(format!("C=S double bond (thione, thial; {ONLY_THIOL})"));
                    }
                    [(c, 1), (d, 1)] if g.el[c] == El::C && g.el[d] == El::C => {
                        return Err(format!("sulfide (thioether C–S–C; {ONLY_THIOL})"));
                    }
                    _ => {
                        return Err(format!(
                            "sulfur bonded to an atom other than carbon or to several atoms \
                             (disulfide, sulfoxide, sulfonic acid…; {ONLY_THIOL})"
                        ));
                    }
                }
            }
            El::O if !nitro_o[a] => {
                if g.adj[a].iter().any(|&(m, _)| g.el[m] != El::C) {
                    return Err("O–O or O–N bond (peroxide, hydroxylamine, N-oxide)".into());
                }
            }
            El::N if in_ring[a] => {
                if g.adj[a]
                    .iter()
                    .any(|&(m, o)| !ring_bond(a, m) && (o != 1 || g.el[m] != El::C))
                {
                    return Err(
                        "ring nitrogen bonded to an atom other than carbon, or by a \
                         multiple bond (N-oxide, N-amino…)"
                            .into(),
                    );
                }
            }
            El::N if !nitro[a] => {
                let nitrile =
                    g.adj[a].len() == 1 && g.adj[a][0].1 == 3 && g.el[g.adj[a][0].0] == El::C;
                let plain = g.adj[a].iter().all(|&(m, o)| o == 1 && g.el[m] == El::C);
                if !nitrile && !plain {
                    return Err("nitrogen group other than amine, amide, nitrile or nitro (imine, azo, hydrazine…)".into());
                }
            }
            _ => {}
        }
    }

    // Carbons: carbonyl and nitrile groups.
    let carbonyl_o = |c: usize| -> Vec<usize> {
        g.adj[c]
            .iter()
            .filter(|&&(m, o)| g.el[m] == El::O && o == 2)
            .map(|&(m, _)| m)
            .collect()
    };
    for c in 0..n {
        if g.el[c] != El::C {
            continue;
        }
        let triple_n = g.adj[c].iter().any(|&(m, o)| g.el[m] == El::N && o == 3);
        if g.adj[c]
            .iter()
            .any(|&(m, o)| g.el[m] == El::N && o == 2 && !ring_bond(c, m))
        {
            return Err("C=N double bond (imine)".into());
        }
        if triple_n {
            let others: Vec<&(usize, u8)> = g.adj[c]
                .iter()
                .filter(|&&(m, o)| !(g.el[m] == El::N && o == 3))
                .collect();
            if others.iter().any(|(m, o)| g.el[*m] != El::C || *o != 1) {
                return Err("unsupported nitrile (cyanate, cyanamide…)".into());
            }
            ctype[c] = Some(Class::Nitrile);
            continue;
        }
        let oxo = carbonyl_o(c);
        if oxo.is_empty() {
            continue;
        }
        if oxo.len() > 1 {
            return Err("carbon with two C=O bonds".into());
        }
        if g.adj[c].iter().any(|&(m, o)| o >= 2 && m != oxo[0]) {
            return Err("cumulated C=O (ketene)".into());
        }
        let hetero: Vec<usize> = g.adj[c]
            .iter()
            .filter(|&&(m, o)| o == 1 && g.el[m] != El::C)
            .map(|&(m, _)| m)
            .collect();
        // A ring C=O carbon's other bonds are its two ring bonds. Next to a ring heteroatom it
        // is a pseudoketone (a lactone, lactam, cyclic anhydride or imide: oxolan-2-one,
        // pyrrolidine-2,5-dione), with the same seniority as a ketone (P-64.3.1, P-64.7.2).
        if in_ring[c] {
            ketone[c] = true;
            continue;
        }
        if hetero.iter().any(|&m| g.el[m] == El::S) {
            return Err(
                "thioacid or thioester (sulfur on a C=O carbon; sulfur outside a ring is \
                 supported only as a thiol)"
                    .into(),
            );
        }
        match hetero.as_slice() {
            [] => {
                if g.h[c] >= 1 {
                    ctype[c] = Some(Class::Aldehyde);
                } else {
                    ketone[c] = true;
                }
            }
            [x] if g.el[*x].is_halogen() => ctype[c] = Some(Class::AcidHalide),
            [x] if g.el[*x] == El::O => {
                if g.free_on(*x) > 0 {
                    return Err("free valence on the oxygen of an acid (acyloxy groups)".into());
                }
                if carboxylate_o[*x] {
                    ctype[c] = Some(Class::Carboxylate);
                } else if g.adj[*x].len() == 1 {
                    ctype[c] = Some(Class::Acid);
                } else {
                    let other = g.nbrs(*x).find(|&m| m != c).unwrap();
                    ctype[c] = Some(if carbonyl_o(other).is_empty() {
                        Class::Ester
                    } else {
                        Class::Anhydride
                    });
                }
            }
            [x] if g.el[*x] == El::N && in_ring[*x] => {
                return Err("amide whose nitrogen is a ring atom (1-acylpiperidine…)".into());
            }
            [x] if g.el[*x] == El::N && !nitro[*x] => {
                ctype[c] = Some(Class::Amide);
            }
            _ => return Err("carbonate, carbamate, urea or similar group".into()),
        }
    }
    // Nitrogens: an amine unless it carries an acyl group.
    for a in 0..n {
        if g.el[a] != El::N || nitro[a] || in_ring[a] || g.adj[a].iter().any(|&(_, o)| o == 3) {
            continue;
        }
        let acyls = g
            .nbrs(a)
            .filter(|&m| ctype[m] == Some(Class::Amide))
            .count();
        match acyls {
            0 => amine[a] = true,
            1 => {}
            _ => return Err("imide".into()),
        }
    }
    // Oxygens: hydroxy groups (not those of an acid or its anion, nor an alkoxy group's
    // oxygen with its free valence).
    for a in 0..n {
        if g.el[a] == El::O
            && !nitro_o[a]
            && !carboxylate_o[a]
            && g.free_on(a) == 0
            && g.adj[a].len() == 1
            && g.adj[a][0].1 == 1
        {
            let c = g.adj[a][0].0;
            if ctype[c] != Some(Class::Acid) {
                hydroxyl[a] = true;
            }
        }
    }

    let count = |k: Class| ctype.iter().filter(|t| **t == Some(k)).count();
    if count(Class::Anhydride) > 2 {
        return Err("more than one anhydride group".into());
    }
    // The ester would be a prefix of one of the anhydride's acids (alkoxy…oxo or
    // alkoxycarbonyl), which is not built.
    if count(Class::Anhydride) > 0 && count(Class::Ester) > 0 {
        return Err("anhydride together with an ester (alkoxycarbonyl prefix)".into());
    }
    if count(Class::Ester) > 1 {
        return Err("more than one ester group".into());
    }
    if count(Class::Ester) == 1 && count(Class::Acid) > 0 {
        return Err("ester together with a carboxylic acid".into());
    }
    let principal = [
        (Class::Carboxylate, count(Class::Carboxylate) > 0),
        (Class::Acid, count(Class::Acid) > 0),
        (Class::Anhydride, count(Class::Anhydride) > 0),
        (Class::Ester, count(Class::Ester) > 0),
        (Class::AcidHalide, count(Class::AcidHalide) > 0),
        (Class::Amide, count(Class::Amide) > 0),
        (Class::Nitrile, count(Class::Nitrile) > 0),
        (Class::Aldehyde, count(Class::Aldehyde) > 0),
        (Class::Ketone, ketone.contains(&true)),
        (Class::Alcohol, hydroxyl.contains(&true)),
        (Class::Thiol, thiol.contains(&true)),
        (Class::Amine, amine.contains(&true)),
    ]
    .into_iter()
    .find(|(_, present)| *present)
    .map(|(k, _)| k);

    Ok(Perceived {
        ctype,
        ketone,
        hydroxyl,
        thiol,
        amine,
        nitro,
        rings,
        parents,
        in_ring,
        principal,
    })
}

/// The seniority of a ring system of `rings` rings with `atoms` as a parent.
fn rank(
    g: &Graph,
    atoms: &[usize],
    rings: usize,
    hetero_locants: Vec<usize>,
    multiple: usize,
    double: usize,
) -> Rank {
    let els: Vec<El> = atoms
        .iter()
        .map(|&a| g.el[a])
        .filter(|&e| e != El::C)
        .collect();
    let count = |e: El| els.iter().filter(|&&x| x == e).count();
    let nitrogen = count(El::N) > 0;
    // Criterion (b) applies only to rings without nitrogen.
    let first_hetero = if nitrogen {
        0
    } else if count(El::O) > 0 {
        2
    } else if count(El::S) > 0 {
        1
    } else {
        0
    };
    let variety = [El::O, El::S, El::N]
        .iter()
        .filter(|&&e| count(e) > 0)
        .count();
    Rank {
        nitrogen,
        first_hetero,
        rings,
        size: atoms.len(),
        heteroatoms: els.len(),
        variety,
        by_element: [count(El::O), count(El::S), count(El::N)],
        hetero_locants: Reverse(hetero_locants),
        multiple,
        double,
    }
}

const MORE_RINGS: &str = "more than one ring sharing atoms: a ring system of more than two rings \
                          (tricyclo…, dispiro…, fused polycycles)";

const FUSED: &str = "more than one ring sharing atoms: two ortho-fused rings of five or more \
                     members, named by fusion nomenclature (decahydronaphthalene, indole, \
                     P-52.2.4.1), which is not supported";

/// The ring systems of a structure: the monocycles, each in cyclic order; the systems of two
/// rings sharing atoms (bicycles, monospiro systems); and which atoms are in a ring. Systems of
/// more rings, and ortho-fused bicycles that take fusion names, are refused.
#[allow(clippy::type_complexity)]
fn find_rings(g: &Graph) -> Result<(Vec<Vec<usize>>, Vec<Poly>, Vec<bool>), Unsupported> {
    let n = g.len();
    // Bridges (bonds on no cycle), by an iterative depth-first search with low-links.
    let mut disc = vec![usize::MAX; n];
    let mut low = vec![0; n];
    let mut bridges = std::collections::HashSet::new();
    let mut time = 0;
    for root in 0..n {
        if disc[root] != usize::MAX {
            continue;
        }
        disc[root] = time;
        low[root] = time;
        time += 1;
        // (atom, the atom it was reached from, index of its next neighbour to visit)
        let mut stack = vec![(root, usize::MAX, 0)];
        while let Some(&(v, parent, i)) = stack.last() {
            if let Some(&(w, _)) = g.adj[v].get(i) {
                if let Some(top) = stack.last_mut() {
                    top.2 += 1;
                }
                if w == parent {
                    continue;
                }
                if disc[w] == usize::MAX {
                    disc[w] = time;
                    low[w] = time;
                    time += 1;
                    stack.push((w, v, 0));
                } else {
                    low[v] = low[v].min(disc[w]);
                }
            } else {
                stack.pop();
                if let Some(&(u, _, _)) = stack.last() {
                    low[u] = low[u].min(low[v]);
                    if low[v] > disc[u] {
                        bridges.insert((u.min(v), u.max(v)));
                    }
                }
            }
        }
    }
    let ring_nbrs: Vec<Vec<usize>> = (0..n)
        .map(|a| {
            g.nbrs(a)
                .filter(|&b| !bridges.contains(&(a.min(b), a.max(b))))
                .collect()
        })
        .collect();
    let in_ring: Vec<bool> = (0..n).map(|a| !ring_nbrs[a].is_empty()).collect();
    let mut seen = vec![false; n];
    let mut rings = Vec::new();
    let mut systems = Vec::new();
    for start in 0..n {
        if !in_ring[start] || seen[start] {
            continue;
        }
        // The ring system: the atoms reachable by ring bonds. It has as many rings as it has
        // bonds less atoms plus one.
        let mut system = vec![start];
        seen[start] = true;
        let mut k = 0;
        while k < system.len() {
            for &b in &ring_nbrs[system[k]] {
                if !seen[b] {
                    seen[b] = true;
                    system.push(b);
                }
            }
            k += 1;
        }
        let bonds = system.iter().map(|&a| ring_nbrs[a].len()).sum::<usize>() / 2;
        match bonds + 1 - system.len() {
            1 => {
                // A monocycle: walk it in cyclic order.
                let mut ring = vec![start];
                let (mut prev, mut cur) = (usize::MAX, start);
                loop {
                    let nb = &ring_nbrs[cur];
                    let next = if nb[0] != prev { nb[0] } else { nb[1] };
                    if next == start {
                        break;
                    }
                    ring.push(next);
                    (prev, cur) = (cur, next);
                }
                rings.push(ring);
            }
            2 => systems.push(two_rings(g, &ring_nbrs, &system)?),
            _ => return Err(MORE_RINGS.into()),
        }
    }
    Ok((rings, systems, in_ring))
}

/// A ring system of two rings (`system`, its atoms; `nb`, the ring bonds of every atom): a
/// monospiro system or a bicycle, with every numbering its rules allow.
fn two_rings(g: &Graph, nb: &[Vec<usize>], system: &[usize]) -> Result<Poly, Unsupported> {
    let internal = || "internal error: ring system of two rings".to_string();
    // The atoms from `first` on, leaving `from`, up to the next atom with more than two ring
    // bonds (not included), and that atom.
    let walk = |from: usize, first: usize| -> (Vec<usize>, usize) {
        let (mut prev, mut cur) = (from, first);
        let mut path = Vec::new();
        while nb[cur].len() == 2 {
            path.push(cur);
            let next = if nb[cur][0] != prev {
                nb[cur][0]
            } else {
                nb[cur][1]
            };
            (prev, cur) = (cur, next);
        }
        (path, cur)
    };
    let reversed = |v: &[usize]| v.iter().rev().copied().collect::<Vec<_>>();
    let mut numberings = Vec::new();
    // The cycles of the system, each its atoms in cyclic order, for the Kekulé forms.
    let mut cycles: Vec<Vec<usize>> = Vec::new();
    // The six-membered cycles of a bicycle made of two bridges of two atoms each: a ring
    // bridged at its 1,4-positions by the third bridge.
    let mut para: Vec<Vec<usize>> = Vec::new();
    let descriptor;
    if let Some(&s) = system.iter().find(|&&a| nb[a].len() == 4) {
        // Spiro: each ring, from a neighbour of the spiro atom round to the other.
        let mut ring_paths: Vec<Vec<usize>> = Vec::new();
        for &f in &nb[s] {
            let (path, end) = walk(s, f);
            if end != s {
                return Err(internal());
            }
            if !ring_paths.iter().any(|r| r.last() == Some(&f)) {
                ring_paths.push(path);
            }
        }
        let [a, b] = &ring_paths[..] else {
            return Err(internal());
        };
        let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
        descriptor = format!("spiro[{}.{}]", small.len(), large.len());
        // P-24.2.1: from the smaller ring (either, if they are alike) next to the spiro atom,
        // around it, through the spiro atom and around the other ring.
        let mut orders = vec![(small, large)];
        if small.len() == large.len() {
            orders.push((large, small));
        }
        for (first, second) in orders {
            for p in [first.clone(), reversed(first)] {
                for q in [second.clone(), reversed(second)] {
                    let mut num = p.clone();
                    num.push(s);
                    num.extend(q);
                    numberings.push(num);
                }
            }
        }
        for r in [a, b] {
            let mut c = vec![s];
            c.extend(r);
            cycles.push(c);
        }
    } else {
        let heads: Vec<usize> = system
            .iter()
            .copied()
            .filter(|&a| nb[a].len() == 3)
            .collect();
        let [h1, h2] = heads[..] else {
            return Err(internal());
        };
        // The three bridges, each from h1's side, the largest first.
        let mut bridges: Vec<Vec<usize>> = Vec::new();
        for &f in &nb[h1] {
            let (path, end) = if f == h2 { (vec![], h2) } else { walk(h1, f) };
            if end != h2 {
                return Err(internal());
            }
            bridges.push(path);
        }
        bridges.sort_by_key(|b| Reverse(b.len()));
        let [x, y, z] = [0, 1, 2].map(|i| bridges[i].len());
        // P-52.2.4.1: two ortho-fused rings of five or more members take a fusion name.
        if z == 0 && y >= 3 {
            return Err(FUSED.into());
        }
        descriptor = format!("bicyclo[{x}.{y}.{z}]");
        // P-23.2.3: from a main bridgehead along the largest bridge to the other bridgehead,
        // back along the second largest, then the main bridge from the atom next to the first
        // bridgehead. Bridges of one length may take each other's place.
        const PERMS: [[usize; 3]; 6] = [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ];
        for (b1, b2, flip) in [(h1, h2, false), (h2, h1, true)] {
            let from_b1 = |i: usize| {
                if flip {
                    reversed(&bridges[i])
                } else {
                    bridges[i].clone()
                }
            };
            for [i, j, k] in PERMS {
                if bridges[i].len() != x || bridges[j].len() != y {
                    continue;
                }
                let mut num = vec![b1];
                num.extend(from_b1(i));
                num.push(b2);
                num.extend(reversed(&from_b1(j)));
                num.extend(from_b1(k));
                numberings.push(num);
            }
        }
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            let mut c = vec![h1];
            c.extend(&bridges[i]);
            c.push(h2);
            c.extend(reversed(&bridges[j]));
            if bridges[i].len() == 2 && bridges[j].len() == 2 {
                para.push(c.clone());
            }
            cycles.push(c);
        }
    }
    let forms = kekule_forms(g, nb, system, &cycles);
    // P-31.1.4.2 gives the von Baeyer name ("bicyclo[10.2.2]hexadeca-1(14),12,15-triene"), but
    // a mancude six-membered ring bridged at its 1,4-positions is a phane-type structure
    // (P-52.2.5.1(1)), whose phane name ("1(1,4)-benzenacyclotridecaphane") IUPAC 2013 labels
    // the PIN; the 1,3-bridged ring keeps the von Baeyer name (P-52.2.5.2.1).
    let mancude = |c: &[usize]| {
        forms.iter().any(|f| {
            (0..c.len())
                .filter(|&i| {
                    let (a, b) = (c[i], c[(i + 1) % c.len()]);
                    f.get(&(a.min(b), a.max(b))) == Some(&2)
                })
                .count()
                == 3
        })
    };
    if para.iter().any(|c| mancude(c.as_slice())) {
        return Err(
            "benzene (or another mancude six-membered) ring bridged at its 1,4-positions \
                    ([n]paracyclophane-type bicycle): its preferred name is a phane name \
                    (1(1,4)-benzenacyclo…phane, P-52.2.5.1), which is not supported"
                .into(),
        );
    }
    Ok(Poly {
        descriptor,
        numberings,
        forms,
    })
}

/// The most Kekulé forms of one ring system considered.
const MAX_FORMS: usize = 16;

/// The bond orders of a ring system as drawn, then those of every other Kekulé form reached by
/// swapping the single and double bonds around a cycle in which they alternate.
fn kekule_forms(
    g: &Graph,
    nb: &[Vec<usize>],
    system: &[usize],
    cycles: &[Vec<usize>],
) -> Vec<BTreeMap<(usize, usize), u8>> {
    let drawn: BTreeMap<(usize, usize), u8> = system
        .iter()
        .flat_map(|&a| nb[a].iter().map(move |&b| (a, b)))
        .filter(|&(a, b)| a < b)
        .map(|(a, b)| ((a, b), g.order(a, b)))
        .collect();
    let key = |a: usize, b: usize| (a.min(b), a.max(b));
    let mut forms = vec![drawn];
    let mut i = 0;
    while i < forms.len() && forms.len() < MAX_FORMS {
        for c in cycles {
            let k = c.len();
            let orders: Vec<u8> = (0..k)
                .map(|j| forms[i][&key(c[j], c[(j + 1) % k])])
                .collect();
            let alternating = k % 2 == 0
                && orders.iter().all(|&o| o == 1 || o == 2)
                && (0..k).all(|j| orders[j] != orders[(j + 1) % k]);
            if !alternating {
                continue;
            }
            let mut form = forms[i].clone();
            for j in 0..k {
                form.insert(key(c[j], c[(j + 1) % k]), 3 - orders[j]);
            }
            if !forms.contains(&form) {
                forms.push(form);
            }
        }
        i += 1;
    }
    forms
}

/// Group the rings into ring parents: identical rings (benzene rings, or saturated rings of
/// one size) joined by single bonds form a ring assembly (P-28.1), which must be unbranched.
fn ring_parents(g: &Graph, rings: &[Ring]) -> Result<Vec<RingParent>, Unsupported> {
    let mut ring_of = vec![usize::MAX; g.len()];
    for (i, r) in rings.iter().enumerate() {
        for &a in &r.atoms {
            ring_of[a] = i;
        }
    }
    let saturated = |r: &Ring| r.hetero.is_none() && r.rank.multiple == 0;
    let benzene = |r: &Ring| r.benzene;
    let same_hetero = |a: &Ring, b: &Ring| match (&a.hetero, &b.hetero) {
        (Some(x), Some(y)) => std::ptr::eq(x.kind, y.kind),
        _ => false,
    };
    // links[i]: (the other ring, the atom in ring i, the atom in the other ring)
    let mut links: Vec<Vec<(usize, usize, usize)>> = vec![Vec::new(); rings.len()];
    for a in 0..g.len() {
        for &(b, o) in &g.adj[a] {
            let (i, j) = (ring_of[a], ring_of[b]);
            if i == usize::MAX || j == usize::MAX || i == j || o != 1 {
                continue;
            }
            let (ri, rj) = (&rings[i], &rings[j]);
            if ri.atoms.len() != rj.atoms.len() {
                continue;
            }
            match (&ri.poly, &rj.poly) {
                (None, None) => {}
                (Some(p), Some(q)) if p.descriptor == q.descriptor => {
                    return Err("ring assembly of bicyclic or spiro ring systems \
                                (2,2'-bi(bicyclo[2.2.1]heptane))"
                        .into());
                }
                // Different ring systems: one is a substituent of the other.
                _ => continue,
            }
            if (benzene(ri) && benzene(rj))
                || (saturated(ri) && saturated(rj))
                || same_hetero(ri, rj)
            {
                links[i].push((j, a, b));
            } else if ri.hetero.is_none()
                && rj.hetero.is_none()
                && (!(benzene(ri) || saturated(ri)) || !(benzene(rj) || saturated(rj)))
            {
                // A cyclohexene ring on a benzene or cyclohexene ring: whether to name it as a
                // hydrogenated ring assembly is not decided here.
                return Err("directly bonded rings of one size, partly unsaturated \
                     (hydrogenated ring assemblies)"
                    .into());
            }
        }
    }
    if links.iter().any(|l| l.len() > 2) {
        return Err(
            "branched ring assembly (a ring bonded directly to three identical rings)".into(),
        );
    }
    let mut done = vec![false; rings.len()];
    let mut parents = Vec::new();
    for start in 0..rings.len() {
        // Each assembly is walked from one of its ends (a lone ring is its own end).
        if done[start] || links[start].len() > 1 {
            continue;
        }
        let mut order = vec![start];
        let mut joins = Vec::new();
        done[start] = true;
        let mut cur = start;
        while let Some(&(next, a, b)) = links[cur].iter().find(|l| !done[l.0]) {
            done[next] = true;
            order.push(next);
            joins.push((a, b));
            cur = next;
        }
        if order.len() > MAX_ASSEMBLY {
            return Err(format!("ring assembly of more than {MAX_ASSEMBLY} rings"));
        }
        if order.len() > 1
            && let Some(h) = &rings[start].hetero
            && (!h.kind.mancude || h.kind.indicated)
        {
            return Err(
                "ring assembly of saturated heterocycles or of heterocycles with \
                 indicated hydrogen (bipiperidine, bipyrrole)"
                    .into(),
            );
        }
        let atoms = order
            .iter()
            .flat_map(|&i| rings[i].atoms.iter().copied())
            .collect();
        parents.push(RingParent {
            rings: order,
            joins,
            atoms,
        });
    }
    // Directly bonded rings in a cycle close a further ring, which find_rings refuses.
    if done.contains(&false) {
        return Err("internal error: ring assembly without an end".into());
    }
    Ok(parents)
}
