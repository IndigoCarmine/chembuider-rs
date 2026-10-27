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
//! 3. for chains: the most skeletal atoms, then the most multiple bonds, then double bonds;
//! 4. lowest locants for the principal groups / free valence;
//! 5. lowest locants for all multiple bonds together, then for double bonds;
//! 6. most substituent prefixes, then lowest locants for them all together;
//! 7. lowest locants for the prefix cited first in alphanumerical order, then the
//!    alphanumerically first name.

use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use super::graph::{Class, El, Graph, Perceived, Unsupported};
use super::words::{alpha_key, enclose, multiplier, multiplier_complex, starts_with_vowel, stem};

/// A named unit: the molecule's name, or a substituent prefix ("2-chloroethyl").
#[derive(Clone, Debug)]
pub(crate) struct Prefix {
    pub text: String,
    /// The prefix has prefixes of its own, which makes it compound: it is enclosed in
    /// parentheses and multiplied by "bis", "tris", …
    pub compound: bool,
    /// How deeply the enclosing marks inside `text` nest.
    pub depth: usize,
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
            oxy: false,
        }
    }

    fn needs_enclosure(&self) -> bool {
        self.compound || self.text.contains(|c: char| c.is_ascii_digit())
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
            Loc::C(n) => write!(f, "{n}"),
        }
    }
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
    Reverse<usize>,
    Vec<u32>,
);

/// One numbered candidate parent, evaluated.
struct Eval {
    atoms: Vec<usize>,
    ring: bool,
    suffix: Vec<u32>,
    mults: Vec<(u32, u8)>,
    prefixes: Vec<(Loc, usize, usize)>,
    rprime: Option<(usize, usize)>,
    key: Key,
}

pub(crate) struct Namer<'a> {
    g: &'a Graph,
    p: &'a Perceived,
    cache: RefCell<HashMap<(usize, usize), Result<Prefix, Unsupported>>>,
}

impl<'a> Namer<'a> {
    pub(crate) fn new(g: &'a Graph, p: &'a Perceived) -> Self {
        Namer {
            g,
            p,
            cache: RefCell::new(HashMap::new()),
        }
    }

    pub(crate) fn name(&self) -> Result<String, Unsupported> {
        let region = vec![true; self.g.len()];
        Ok(self.unit(Mode::Molecule, &region)?.text)
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
                Some(c) => mode == Mode::Molecule && Some(c) == self.p.principal,
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
            Mode::Acyl { atom } => vec![Pg::InChain(atom)],
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
                Some(Class::Amine) => atoms
                    .filter(|&a| p.amine[a])
                    .map(|n| Pg::OnAtom {
                        anchors: g.nbrs(n).collect(),
                        group: Some(n),
                    })
                    .collect(),
                Some(_) => unreachable!("every class is handled above"),
            },
        }
    }

    /// All chains (as numbered atom lists, both directions) of the acyclic skeleton in
    /// `region`, from end to end; in a substituent, only those through its free valence.
    fn chains(&self, mode: Mode, region: &[bool]) -> Vec<Vec<usize>> {
        let g = self.g;
        let n = g.len();
        let ok: Vec<bool> = (0..n)
            .map(|a| region[a] && !self.p.in_ring[a] && self.is_skeletal(a, mode))
            .collect();
        let nb = |a: usize| g.nbrs(a).filter(|&m| ok[m]).collect::<Vec<_>>();
        let must = match mode {
            Mode::FreeValence { atom, .. } | Mode::Acyl { atom } => Some(atom),
            Mode::Molecule => None,
        };
        let mut ends: Vec<usize> = (0..n).filter(|&a| ok[a] && nb(a).len() <= 1).collect();
        if let Some(m) = must
            && ok[m]
            && !ends.contains(&m)
        {
            ends.push(m);
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
                if must.is_none_or(|m| path.contains(&m)) {
                    out.push(path);
                }
            }
        }
        out
    }

    fn counts(&self, pgs: &[Pg], atoms: &[usize], ring: bool) -> usize {
        pgs.iter()
            .filter(|pg| match pg {
                Pg::InChain(c) => {
                    if ring {
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
        let p = self.p;
        let pgs = self.principal_groups(mode, region);
        let ring = p.ring.as_ref().filter(|r| r.iter().all(|&a| region[a]));
        let chains = match mode {
            Mode::FreeValence { atom, .. } if p.in_ring[atom] => vec![],
            _ => self.chains(mode, region),
        };
        let chain_max = chains.iter().map(|c| self.counts(&pgs, c, false)).max();
        let ring_count = ring.map(|r| self.counts(&pgs, r, true));
        let pick_ring = match mode {
            Mode::FreeValence { atom, .. } => p.in_ring[atom],
            _ => match (ring_count, chain_max) {
                (Some(rc), Some(cm)) => rc >= cm,
                (Some(_), None) => true,
                _ => false,
            },
        };
        let numberings: Vec<(Vec<usize>, bool)> = if pick_ring {
            let r = ring.ok_or("internal error: ring parent outside the region")?;
            let n = r.len();
            (0..n)
                .flat_map(|s| {
                    [1, n - 1].map(|step| ((0..n).map(|k| r[(s + k * step) % n]).collect(), true))
                })
                .collect()
        } else {
            let best = chain_max.ok_or("internal error: no parent chain")?;
            chains
                .into_iter()
                .filter(|c| self.counts(&pgs, c, false) == best)
                .map(|c| (c, false))
                .collect()
        };
        if mode == Mode::Molecule
            && !pick_ring
            && p.principal.is_some_and(Class::has_carbon)
            && pgs.len() > 2
        {
            return Err("more than two acid/amide/nitrile/aldehyde groups on a chain".into());
        }

        let evals: Vec<Eval> = numberings
            .into_iter()
            .map(|(atoms, ring)| self.evaluate(atoms, ring, &pgs, region))
            .collect();
        let best = evals
            .iter()
            .map(|e| &e.key)
            .min()
            .ok_or("internal error: no numbering")?
            .clone();
        let mut chosen: Option<(Vec<Loc>, Prefix)> = None;
        for e in evals.iter().filter(|e| e.key == best) {
            let (order, named) = self.render(e, mode, region)?;
            let better = match &chosen {
                None => true,
                Some((o, n)) => (&order, &named.text) < (o, &n.text),
            };
            if better {
                chosen = Some((order, named));
            }
        }
        Ok(chosen.expect("at least one candidate").1)
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

    fn evaluate(&self, atoms: Vec<usize>, ring: bool, pgs: &[Pg], region: &[bool]) -> Eval {
        let g = self.g;
        let mut pos = vec![0u32; g.len()];
        for (i, &a) in atoms.iter().enumerate() {
            pos[a] = i as u32 + 1;
        }
        let mut skip: HashSet<(usize, usize)> = HashSet::new();
        let mut suffix = Vec::new();
        let mut nprefix: Vec<(Loc, usize, usize)> = Vec::new();
        let mut rprime = None;

        for pg in pgs {
            match pg {
                Pg::InChain(c) => {
                    if !ring && pos[*c] > 0 {
                        suffix.push(pos[*c]);
                        self.group_parts(*c, region, &mut skip, &mut nprefix, &mut rprime);
                    } else if ring
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
        let benzene = ring && self.p.benzene;
        if !benzene {
            let n = atoms.len();
            let bonds = if ring { n } else { n.saturating_sub(1) };
            for i in 0..bonds {
                let o = g.order(atoms[i], atoms[(i + 1) % n]);
                if o > 1 {
                    let loc = if i + 1 == n { n as u32 } else { i as u32 + 1 };
                    mults.push((loc, o));
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

        let mult_locs: Vec<u32> = mults.iter().map(|m| m.0).collect();
        let double_locs: Vec<u32> = mults.iter().filter(|m| m.1 == 2).map(|m| m.0).collect();
        let key = (
            Reverse(atoms.len()),
            Reverse(mults.len()),
            Reverse(double_locs.len()),
            suffix.clone(),
            mult_locs,
            double_locs,
            Reverse(nprefixes),
            prefix_locs,
        );
        Eval {
            atoms,
            ring,
            suffix,
            mults,
            prefixes,
            rprime,
            key,
        }
    }

    /// The name of an evaluated candidate, with the locants in the order its prefixes are
    /// cited (the last tie-breaker).
    fn render(
        &self,
        e: &Eval,
        mode: Mode,
        region: &[bool],
    ) -> Result<(Vec<Loc>, Prefix), Unsupported> {
        let g = self.g;
        let suffix = match mode {
            Mode::Molecule => match self.p.principal {
                Some(c) if !e.suffix.is_empty() => Suffix::Class(c),
                _ => Suffix::None,
            },
            Mode::FreeValence { order, .. } => Suffix::Yl(order),
            Mode::Acyl { .. } => Suffix::Acyl,
        };
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

        let len = e.atoms.len();
        let chain = !e.ring;
        let benzene = e.ring && self.p.benzene;
        let nsuf = e.suffix.len();
        let n_c_prefix = e.prefixes.iter().filter(|(l, _, _)| *l != Loc::N).count();
        let n_feat = nsuf + n_c_prefix;
        let terminal =
            matches!(suffix, Suffix::Acyl) || matches!(suffix, Suffix::Class(c) if c.has_carbon());
        let retained_two =
            chain && len == 2 && nsuf == 1 && terminal && suffix != Suffix::Class(Class::Amide);
        // Every hydrogen of the parent replaced by one and the same prefix: hexachloroethane.
        let fully = matches!(suffix, Suffix::None | Suffix::Yl(_))
            && groups.len() == 1
            && !has_n_prefix
            && e.atoms.iter().all(|&a| g.h[a] == 0);
        let omit_c = (chain && len == 1)
            || retained_two
            || (chain && len == 2 && n_feat <= 1)
            || (e.ring && !benzene && n_feat + e.mults.len() <= 1)
            || (benzene && n_feat <= 1);
        let contracted = matches!(suffix, Suffix::Yl(_)) && e.suffix == [1] && e.mults.is_empty();
        let cite_suffix = !omit_c && !(chain && terminal) && !contracted && !(benzene && nsuf == 1);
        let cite_unsat = !(chain && len <= 2) && !(e.ring && n_feat + e.mults.len() <= 1);

        let core = self.core(
            len,
            e.ring,
            benzene,
            &e.mults,
            suffix,
            &e.suffix,
            cite_suffix,
            cite_unsat,
            contracted,
        )?;
        // With N-substituents present, carbon locants are cited to tell the two apart:
        // N,N-dimethyl-1-phenylmethanamine.
        let (prefix_text, depth, order) = compose(&groups, (omit_c || fully) && !has_n_prefix);

        let mut text = format!("{prefix_text}{core}");
        let mut compound = !groups.is_empty();
        let mut oxy = (contracted && chain && len <= 4 && suffix == Suffix::Yl(1))
            || (benzene && suffix == Suffix::Yl(1));
        if mode == Mode::Molecule {
            if benzene
                && suffix == Suffix::None
                && let Some(t) = retained_benzene(&groups)
            {
                text = t;
            }
            if let Some((r, o)) = e.rprime
                && self.p.principal == Some(Class::Ester)
                && region[r]
            {
                let alkyl = self.sub(r, o)?;
                text = format!("{} {text}", alkyl.text);
            }
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
                oxy,
            },
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn core(
        &self,
        len: usize,
        ring: bool,
        benzene: bool,
        mults: &[(u32, u8)],
        suffix: Suffix,
        locs: &[u32],
        cite_suffix: bool,
        cite_unsat: bool,
        contracted: bool,
    ) -> Result<String, Unsupported> {
        let count = locs.len();
        let loc_list = || {
            locs.iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        if benzene {
            return Ok(match (suffix, count) {
                (Suffix::None, _) => "benzene".into(),
                (Suffix::Class(c), 1) => match c {
                    Class::Acid => "benzoic acid",
                    Class::Ester => "benzoate",
                    Class::Amide => "benzamide",
                    Class::Nitrile => "benzonitrile",
                    Class::Aldehyde => "benzaldehyde",
                    Class::Alcohol => "phenol",
                    Class::Amine => "aniline",
                    Class::Ketone => return Err("internal error: ketone on benzene".into()),
                }
                .into(),
                (Suffix::Class(c), _) => {
                    format!(
                        "benzene-{}-{}",
                        loc_list(),
                        multiplied(count, suffix_word(Suffix::Class(c), true))
                    )
                }
                (Suffix::Yl(1), _) => "phenyl".into(),
                (Suffix::Acyl, _) => "benzoyl".into(),
                (Suffix::Yl(_), _) => return Err("internal error: ylidene on benzene".into()),
            });
        }
        let stem = stem(len).ok_or("chain or ring longer than 99 atoms")?;
        let base = if ring { format!("cyclo{stem}") } else { stem };
        if !ring && count == 1 && (len == 1 || len == 2) {
            let retained = match (len, suffix) {
                (1, Suffix::Class(Class::Acid)) => Some("formic acid"),
                (1, Suffix::Class(Class::Ester)) => Some("formate"),
                (1, Suffix::Class(Class::Amide)) => Some("formamide"),
                (1, Suffix::Class(Class::Nitrile)) => Some("formonitrile"),
                (1, Suffix::Class(Class::Aldehyde)) => Some("formaldehyde"),
                (1, Suffix::Acyl) => Some("formyl"),
                (2, Suffix::Class(Class::Acid)) => Some("acetic acid"),
                (2, Suffix::Class(Class::Ester)) => Some("acetate"),
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
        if !ring && len == 2 && count == 2 && suffix == Suffix::Class(Class::Acid) {
            return Ok("oxalic acid".into());
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
        let word = multiplied(count, suffix_word(suffix, ring));
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
    fn sub(&self, q: usize, from: usize) -> Result<Prefix, Unsupported> {
        if let Some(r) = self.cache.borrow().get(&(q, from)) {
            return r.clone();
        }
        let r = self.sub_uncached(q, from);
        self.cache.borrow_mut().insert((q, from), r.clone());
        r
    }

    fn sub_uncached(&self, q: usize, from: usize) -> Result<Prefix, Unsupported> {
        let (g, p) = (self.g, self.p);
        let order = g.order(q, from);
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
                                oxy: false,
                            }
                        })
                    }
                    _ => Err("internal error: oxygen with three bonds".into()),
                }
            }
            El::N => {
                if p.nitro[q] {
                    return Ok(Prefix::simple("nitro"));
                }
                if others.iter().any(|&r| p.ctype[r] == Some(Class::Amide)) {
                    return Err(
                        "amide bonded through its nitrogen (acylamino) as a substituent".into(),
                    );
                }
                self.amino_like(q, &others, "amino")
            }
            El::C => match p.ctype[q] {
                Some(Class::Acid) => Ok(Prefix::simple("carboxy")),
                Some(Class::Nitrile) => Ok(Prefix::simple("cyano")),
                Some(Class::Amide) => {
                    // On a ring the prefix is "carbamoyl"; at the end of a chain IUPAC 2013 may
                    // prefer "amino…oxo" instead, which this namer does not decide.
                    if !p.in_ring[from] {
                        return Err("amide group as a prefix (carbamoyl on a chain)".into());
                    }
                    let n = g.nbrs(q).find(|&m| g.el[m] == El::N).unwrap();
                    let subs: Vec<usize> = g.nbrs(n).filter(|&m| m != q).collect();
                    self.amino_like(n, &subs, "carbamoyl")
                }
                Some(Class::Ester) => {
                    Err("ester group as a substituent (alkoxycarbonyl, acyloxy)".into())
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

/// The hydride name with its unsaturation: "butane", "but-2-ene", "buta-1,3-diene",
/// "but-1-en-3-yne", "cyclohexene".
fn unsaturation(base: &str, mults: &[(u32, u8)], cite: bool) -> String {
    let enes: Vec<u32> = mults.iter().filter(|m| m.1 == 2).map(|m| m.0).collect();
    let ynes: Vec<u32> = mults.iter().filter(|m| m.1 == 3).map(|m| m.0).collect();
    if enes.is_empty() && ynes.is_empty() {
        return format!("{base}ane");
    }
    let join = |l: &[u32]| l.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let mut s = base.to_string();
    if enes.len() > 1 || (enes.is_empty() && ynes.len() > 1) {
        s.push('a');
    }
    if !enes.is_empty() {
        if cite {
            s += &format!("-{}-", join(&enes));
        }
        s += &format!("{}ene", multiplier(enes.len()));
    }
    if !ynes.is_empty() {
        if !enes.is_empty() && ynes.len() == 1 {
            s.pop();
        }
        if cite {
            s += &format!("-{}-", join(&ynes));
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
        (Suffix::Class(Class::Amine), _) => "amine",
        (Suffix::Acyl, false) => "oyl",
        (Suffix::Acyl, true) => "carbonyl",
        (Suffix::Yl(2), _) => "ylidene",
        (Suffix::Yl(3), _) => "ylidyne",
        (Suffix::Yl(_), _) => "yl",
        (Suffix::None, _) => "",
    }
}
