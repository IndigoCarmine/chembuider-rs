//! The molecule as the namer sees it: one connected structure of heavy atoms with their
//! hydrogen counts, abbreviations expanded, plus the characteristic groups found in it.
//!
//! Everything the namer does not support is rejected here with a reason, so the naming code
//! itself only ever meets structures it knows how to name.

use crate::config::Config;
use crate::label_chain::resolve_label;
use crate::molecule::elements::element_of_label;
use crate::molecule::{BondOrder, Molecule, charged_valence};
use crate::widget::rich_text::RichText;

/// The elements the namer handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum El {
    C,
    N,
    O,
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
}

/// One connected structure: heavy atoms, their charges and hydrogen counts, and bonds.
pub(crate) struct Graph {
    pub el: Vec<El>,
    pub charge: Vec<i8>,
    pub h: Vec<u8>,
    /// Neighbours with the bond order to each.
    pub adj: Vec<Vec<(usize, u8)>>,
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

    fn bond_sum(&self, a: usize) -> i16 {
        self.adj[a].iter().map(|(_, o)| *o as i16).sum()
    }
}

/// The principal characteristic groups the namer knows, least senior first, so that the
/// derived `Ord` is seniority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Class {
    Amine,
    Alcohol,
    Ketone,
    Aldehyde,
    Nitrile,
    Amide,
    Ester,
    Acid,
}

impl Class {
    /// Groups whose carbon atom belongs to the group (and so ends a chain, or is expressed by
    /// a "-carbo…" suffix on a ring): acids, esters, amides, nitriles and aldehydes.
    pub(crate) fn has_carbon(self) -> bool {
        matches!(
            self,
            Class::Acid | Class::Ester | Class::Amide | Class::Nitrile | Class::Aldehyde
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
    /// Amine nitrogens (not amide, nitrile or nitro nitrogens).
    pub amine: Vec<bool>,
    /// Nitro nitrogens.
    pub nitro: Vec<bool>,
    /// The ring's atoms in cyclic order, when there is a ring.
    pub ring: Option<Vec<usize>>,
    pub in_ring: Vec<bool>,
    /// The ring is a benzene ring (six carbons, alternating double bonds).
    pub benzene: bool,
    /// The most senior class present.
    pub principal: Option<Class>,
}

/// A reason a structure cannot be named.
pub(crate) type Unsupported = String;

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
struct Raw {
    label: Vec<String>,
    charge: Vec<i8>,
    bonds: Vec<(usize, usize, u8)>,
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

/// Build the graph of one connected structure (`atoms` index `mol.atoms`).
pub(crate) fn build(
    mol: &Molecule,
    atoms: &[usize],
    config: &Config,
) -> Result<Graph, Unsupported> {
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
    };
    for b in &mol.bonds {
        if let (Some(&a), Some(&c)) = (local.get(&b.begin), local.get(&b.end))
            && a != c
        {
            raw.bonds.push((a, c, order_value(&b.order)));
        }
    }
    expand_labels(&mut raw, config)?;
    to_graph(&raw)
}

/// Replace every abbreviation ("Ph", "OMe", "CO2Et", …) by the atoms it stands for.
fn expand_labels(raw: &mut Raw, config: &Config) -> Result<(), Unsupported> {
    // A fragment may itself contain labels; a few rounds settle any sane nesting.
    for _ in 0..8 {
        let Some(i) = (0..raw.label.len()).find(|&i| element_of_label(&raw.label[i]).is_none())
        else {
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
            }
        }
        for fb in &frag.bonds {
            raw.bonds
                .push((map[fb.begin], map[fb.end], order_value(&fb.order)));
        }
    }
    match raw.label.iter().find(|l| element_of_label(l).is_none()) {
        Some(l) => Err(format!("unknown label \u{201c}{l}\u{201d}")),
        None => Ok(()),
    }
}

/// Fold the hydrogens into their heavy atoms and check every element is one the namer knows.
fn to_graph(raw: &Raw) -> Result<Graph, Unsupported> {
    let n = raw.label.len();
    let mut explicit_h = vec![0u8; n];
    let mut sum = vec![0i16; n];
    let mut heavy_index = vec![usize::MAX; n];
    let mut el = Vec::new();
    let mut charge = Vec::new();
    for (i, label) in raw.label.iter().enumerate() {
        if label == "H" {
            continue;
        }
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
    for &(a, b, o) in &raw.bonds {
        sum[a] += o as i16;
        sum[b] += o as i16;
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
    // Every drawn hydrogen must hang on exactly one heavy atom.
    for i in 0..n {
        if raw.label[i] == "H" {
            let bonds = raw
                .bonds
                .iter()
                .filter(|(a, b, _)| *a == i || *b == i)
                .count();
            if bonds != 1 {
                return Err("unusual hydrogen".into());
            }
        }
    }
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
    Ok(Graph { el, charge, h, adj })
}

/// Find the ring and the characteristic groups, rejecting anything outside the supported
/// classes.
pub(crate) fn perceive(g: &Graph) -> Result<Perceived, Unsupported> {
    let n = g.len();
    if !g.el.contains(&El::C) {
        return Err("no carbon atom".into());
    }
    let edges: usize = g.adj.iter().map(Vec::len).sum::<usize>() / 2;
    // The structure is connected, so this is its number of independent rings.
    let rings = edges + 1 - n;
    if rings > 1 {
        return Err("more than one ring (fused, bridged, spiro or separate rings)".into());
    }
    let (ring, in_ring) = if rings == 1 {
        find_ring(g)
    } else {
        (None, vec![false; n])
    };
    if let Some(r) = &ring
        && r.iter().any(|&a| g.el[a] != El::C)
    {
        return Err("heterocycle".into());
    }
    let benzene = ring.as_ref().is_some_and(|r| {
        r.len() == 6
            && r.iter()
                .all(|&a| r.iter().filter(|&&b| g.order(a, b) == 2).count() == 1)
    });

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
        if os.len() != 2 || cs.len() != 1 || g.adj[a].len() != 3 || cs[0].1 != 1 {
            continue;
        }
        if os.iter().any(|&(o, _)| g.adj[o].len() != 1) {
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
    for a in 0..n {
        if nitro[a] || nitro_o[a] {
            continue;
        }
        if g.charge[a] != 0 {
            return Err("charged atoms (ions, salts, zwitterions)".into());
        }
        let valence = match g.el[a] {
            El::C => 4,
            El::N => 3,
            El::O => 2,
            _ => 1,
        };
        if g.bond_sum(a) > valence {
            return Err("an atom has more bonds than its valence allows".into());
        }
    }

    let mut ctype: Vec<Option<Class>> = vec![None; n];
    let mut ketone = vec![false; n];
    let mut hydroxyl = vec![false; n];
    let mut amine = vec![false; n];

    // Heteroatoms that may appear only in certain places.
    for a in 0..n {
        match g.el[a] {
            e if e.is_halogen() => {
                if g.adj[a].len() != 1 || g.el[g.adj[a][0].0] != El::C || g.adj[a][0].1 != 1 {
                    return Err("halogen not bonded to carbon".into());
                }
            }
            El::O if !nitro_o[a] => {
                if g.adj[a].iter().any(|&(m, _)| g.el[m] != El::C) {
                    return Err("O–O or O–N bond (peroxide, hydroxylamine, N-oxide)".into());
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
        if g.adj[c].iter().any(|&(m, o)| g.el[m] == El::N && o == 2) {
            return Err("C=N double bond (imine)".into());
        }
        if triple_n {
            let others: Vec<&(usize, u8)> =
                g.adj[c].iter().filter(|(m, _)| g.el[*m] != El::N).collect();
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
        if hetero.iter().any(|&m| g.el[m].is_halogen()) {
            return Err("acyl halide".into());
        }
        match hetero.as_slice() {
            [] => {
                if in_ring[c] {
                    ketone[c] = true;
                } else if g.h[c] >= 1 {
                    ctype[c] = Some(Class::Aldehyde);
                } else {
                    ketone[c] = true;
                }
            }
            [x] if g.el[*x] == El::O => {
                if g.adj[*x].len() == 1 {
                    ctype[c] = Some(Class::Acid);
                } else {
                    let other = g.nbrs(*x).find(|&m| m != c).unwrap();
                    if !carbonyl_o(other).is_empty() {
                        return Err("acid anhydride".into());
                    }
                    ctype[c] = Some(Class::Ester);
                }
            }
            [x] if g.el[*x] == El::N && !nitro[*x] => {
                ctype[c] = Some(Class::Amide);
            }
            _ => return Err("carbonate, carbamate, urea or similar group".into()),
        }
    }
    // Nitrogens: an amine unless it carries an acyl group.
    for a in 0..n {
        if g.el[a] != El::N || nitro[a] || g.adj[a].iter().any(|&(_, o)| o == 3) {
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
    // Oxygens: hydroxy groups (not those of an acid).
    for a in 0..n {
        if g.el[a] == El::O && !nitro_o[a] && g.adj[a].len() == 1 && g.adj[a][0].1 == 1 {
            let c = g.adj[a][0].0;
            if ctype[c] != Some(Class::Acid) {
                hydroxyl[a] = true;
            }
        }
    }

    let count = |k: Class| ctype.iter().filter(|t| **t == Some(k)).count();
    if count(Class::Ester) > 1 {
        return Err("more than one ester group".into());
    }
    if count(Class::Ester) == 1 && count(Class::Acid) > 0 {
        return Err("ester together with a carboxylic acid".into());
    }
    let principal = [
        (Class::Acid, count(Class::Acid) > 0),
        (Class::Ester, count(Class::Ester) > 0),
        (Class::Amide, count(Class::Amide) > 0),
        (Class::Nitrile, count(Class::Nitrile) > 0),
        (Class::Aldehyde, count(Class::Aldehyde) > 0),
        (Class::Ketone, ketone.contains(&true)),
        (Class::Alcohol, hydroxyl.contains(&true)),
        (Class::Amine, amine.contains(&true)),
    ]
    .into_iter()
    .find(|(_, present)| *present)
    .map(|(k, _)| k);

    Ok(Perceived {
        ctype,
        ketone,
        hydroxyl,
        amine,
        nitro,
        ring,
        in_ring,
        benzene,
        principal,
    })
}

/// The single ring of a structure with one independent cycle: prune chain ends until only
/// the cycle is left, then walk it.
fn find_ring(g: &Graph) -> (Option<Vec<usize>>, Vec<bool>) {
    let n = g.len();
    let mut deg: Vec<usize> = (0..n).map(|a| g.adj[a].len()).collect();
    let mut alive = vec![true; n];
    let mut stack: Vec<usize> = (0..n).filter(|&a| deg[a] <= 1).collect();
    while let Some(a) = stack.pop() {
        if !alive[a] {
            continue;
        }
        alive[a] = false;
        for m in g.nbrs(a) {
            if alive[m] {
                deg[m] -= 1;
                if deg[m] == 1 {
                    stack.push(m);
                }
            }
        }
    }
    let Some(start) = (0..n).find(|&a| alive[a]) else {
        return (None, vec![false; n]);
    };
    let mut ring = vec![start];
    let mut prev = usize::MAX;
    let mut cur = start;
    loop {
        let next = g.nbrs(cur).find(|&m| alive[m] && m != prev).unwrap();
        if next == start {
            break;
        }
        ring.push(next);
        prev = cur;
        cur = next;
    }
    (Some(ring), alive)
}
