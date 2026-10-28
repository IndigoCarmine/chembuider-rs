//! Chained fragment labels: writing `MeOPh` on an atom means `Me–O–Ph`.
//!
//! An atom label is normally *one* fragment name — `OMe`, `NO2`, `Boc` — which
//! [`Config::resolve_fragment`] looks up by exact name. This module adds the second reading:
//! when the label is not a name of its own, it is split into a **sequence** of tokens — fragment
//! names and element symbols — and those are welded head-to-tail into one composite
//! [`Fragment`], which the caller then inserts exactly as it would insert any other fragment.
//! An element token is a lone atom of that element, so `NaH` reads as Na–H.
//!
//! The entry point is [`resolve_label`]. It tries the exact name **first**, so every label
//! that resolved before resolves to the identical fragment now; the chain reading is only ever
//! reached by labels that previously expanded to nothing.
//!
//! ```text
//!   "MeOPh"  →  ["Me", "O", "Ph"]  →  CH3–O–C6H5
//!   "NaOMe"  →  ["Na", "OMe"]       →  Na–O–CH3
//!   "NO2"    →  exact fragment name, never split
//!   "Qq"     →  no reading at all → the label is left alone
//! ```
//!
//! # Direction and the three rules
//!
//! **1. The leftmost token attaches to the parent atom.** The composite's `attach_idx` is the
//! entry atom of the first token, so `OMe` written on a ring carbon still joins through its
//! oxygen — the same end it joins through today. Reading left-to-right, each token hangs off
//! the one before it.
//!
//! **2. A fragment's *entry* atom is its `attach_idx`.** That is what fragment JSON already
//! declares, and it is the atom the previous link in the chain bonds to.
//!
//! **3. A fragment's *exit* atom — the atom the *next* link bonds to — is derived**, because
//! nothing in the fragment format declares one. The rule ([`exit_idx`]):
//!
//! > The exit is the **non-hydrogen atom farthest from the entry** by bond count, ties broken
//! > by the lowest index in the fragment's own atom list. If the fragment has no non-hydrogen
//! > atom other than the entry, **the exit is the entry itself**.
//!
//! Which gives, for the fragments that ship with the app:
//!
//! | fragment | entry | exit | chain reading |
//! |---|---|---|---|
//! | `Me`, `O`, `H`, `Cl`, … | the lone atom | itself | trivially unambiguous |
//! | `OMe` | O | the methyl C | `…–O–CH2–…` |
//! | `Et` | CH2 | the terminal CH3 | `…–CH2CH2–…` |
//! | `Ph` | ipso C | the **para** C | `…–C6H4–…`, the conventional reading |
//! | `N3` | N1 | N3 | `…–N=N=N–…` |
//! | `OH`, `NH2`, `SH` | the heteroatom | itself (only H beyond it) | the H stays put |
//!
//! Hydrogens are excluded on purpose: routing a chain *through* an H (`OH` + `Me` giving
//! O–H–C) is never what anyone means, and the H would end up divalent.
//!
//! Links between tokens are **single bonds**. A fragment's own internal bond orders are
//! copied verbatim, so `Ph` keeps its Kekulé pattern and `NO2` keeps its charges.
//!
//! # What the parent atom's own bonds do
//!
//! Nothing here, and that is the point. `MeOPh` on a *bare* atom yields CH3–O–C6H5 (anisole);
//! on an atom that already carries one bond it yields –CH2–O–C6H5, because the leading carbon
//! simply has one more neighbour. Hydrogens are implicit — they are counted at draw time from
//! valence (`molecule::depict`), never stored — so the difference costs this module no code at all.

use crate::config::Config;
use crate::molecule::fragment::{FragAtom, FragBond, Fragment};
use crate::molecule::BondOrder;

/// Length of a link bond between two tokens, matching `widget::DEFAULT_BOND_LENGTH` and the
/// spacing the fragment JSON files are drawn at.
const LINK_LENGTH: f32 = 1.5;

/// Half-angle of the zigzag the chain is laid out along. Consecutive links alternate ±30°
/// about the +x axis, which is how a chain is drawn by hand and — more importantly — keeps
/// the layout monotone in x, so a later token can never fold back onto an earlier one and be
/// swallowed by `Molecule::insert_fragment`'s snap radius.
const ZIGZAG: f32 = std::f32::consts::FRAC_PI_6;

// ═══════════════════════════════════════════════════════════════════════════════
// Entry point
// ═══════════════════════════════════════════════════════════════════════════════

/// Resolve an atom label to the fragment it should expand into.
///
/// Drop-in replacement for [`Config::resolve_fragment`] on the label-expansion path: an exact
/// fragment name resolves through `resolve_fragment` unchanged, and only a label that is *not*
/// a fragment name is considered as a chain. Returns `None` when the label has no reading at
/// all, which callers must treat as "leave the label alone".
pub fn resolve_label(config: &Config, label: &str) -> Option<Fragment> {
    // Exact name first — this is the pre-existing behaviour, bit for bit.
    if let Some(frag) = config.resolve_fragment(label) {
        return Some(frag);
    }

    let names = tokenize(config, label)?;
    // A one-token split means the label *is* a fragment name, which the branch above already
    // took, or a bare element symbol (`Na`), which stays the plain atom it already is.
    if names.len() < 2 {
        return None;
    }

    // A name that is both a fragment and an element symbol is the fragment (see `tokenize`).
    let parts: Vec<Fragment> = names
        .iter()
        .map(|n| config.resolve_fragment(n).or_else(|| element_atom(n)))
        .collect::<Option<Vec<_>>>()?;

    let mut composite = compose(&parts)?;
    composite.name = label.to_string();
    Some(composite)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tokenizer
// ═══════════════════════════════════════════════════════════════════════════════

/// Split `label` into a sequence of tokens — fragment names and element symbols — or `None` if
/// no split covers it exactly.
///
/// **Longest match first, with backtracking.** At each position the candidate names are tried
/// longest-first (ties broken lexicographically, so the result never depends on the order
/// fragments happened to load in); if the remainder of the label turns out to be unreadable,
/// the next-shorter candidate is tried instead. That search is run without recursion or
/// re-exploration: first, from the end of the label backwards, which positions can be read to
/// the end at all; then, from the front, the first candidate (in longest-first order) that
/// lands on such a position — the very split the backtracking search finds, in time linear in
/// the label length (times the number of names), and with no call-stack depth to exhaust on a
/// long label.
///
/// Longest-first is what keeps existing names whole: `NO2` is the nitro fragment, not `N`
/// followed by an unreadable `O2`; `CO2Me`, `SiH3`, `TMS` and `CF3` are likewise never
/// nibbled apart from the front. Backtracking is what stops longest-first from being a trap —
/// a greedy-only split of `Xyz` against names `{Xy, X, yz}` dead-ends at `Xy` and would wrongly
/// report the label unreadable. None of the built-in fragments can produce that dead end today,
/// but fragment names are user data (`fragments/*.json`), so the parser must not depend on that.
///
/// Matching is **case-sensitive**, so `Ph` (phenyl) and `PH2` (phosphino) stay distinct and
/// `ME` is not `Me`.
///
/// **Element symbols** ([`element_tokens`]) are candidates on the same footing, under the same
/// longest-first order, which settles the element/fragment ambiguities one way throughout:
///
/// * a fragment name longer than a symbol it starts with wins: `MgBr` is the fragment, not
///   `Mg` + `Br`, and `OMe` is not `O` + `Me`;
/// * a two-letter symbol beats its first letter — but only when the second letter matches, and
///   case decides that: `Co` is cobalt, `CO` is `C` + `O`;
/// * a name that is **both** a fragment and a symbol (`H`, `Cl`, `Li`) is the fragment — the
///   two readings are the same atom for every built-in one, and a user's fragment of that name
///   is what the user means.
///
/// A symbol carries no count: `H4` is not a token, so `LiAlH4`, `H2O` and `CH3` have no chain
/// reading, just as before. Counted labels are formulas, which
/// [`MolecularFormula`](crate::reaction::MolecularFormula) reads; a chain of single bonds is
/// no way to spell them.
pub fn tokenize(config: &Config, label: &str) -> Option<Vec<String>> {
    if label.is_empty() {
        return None;
    }
    let names = token_names(config);
    let bytes = label.as_bytes();
    // A name matching at `pos`. Compared as bytes: a name starts on a character boundary, so
    // it can never match from inside a multi-byte character.
    let matches = |pos: usize| {
        names
            .iter()
            .copied()
            .filter(move |n| bytes[pos..].starts_with(n.as_bytes()))
    };

    // readable[pos]: the rest of the label from `pos` splits into tokens.
    let mut readable = vec![false; label.len() + 1];
    readable[label.len()] = true;
    for pos in (0..label.len()).rev() {
        readable[pos] = matches(pos).any(|n| readable[pos + n.len()]);
    }
    if !readable[0] {
        return None;
    }

    let mut out: Vec<String> = Vec::new();
    let mut pos = 0;
    while pos < label.len() {
        let name = matches(pos).find(|n| readable[pos + n.len()])?;
        out.push(name.to_string());
        pos += name.len();
    }
    Some(out)
}

/// Candidate token names: every fragment the config knows and every [element
/// symbol](element_tokens), longest first.
///
/// Generated names (`ring6`, …) are deliberately excluded — they are a shortcut mechanism, not
/// label vocabulary, and admitting them would widen the ambiguity surface for no gain. Anything
/// that should be spellable in a label is spellable as a `fragments/<name>.json` file.
fn token_names(config: &Config) -> Vec<&str> {
    let mut names: Vec<&str> = config
        .fragments
        .iter()
        .map(|f| f.name.as_str())
        .filter(|n| !n.is_empty())
        .collect();
    // Pushed one by one: `extend` would demand the fragment names be `'static` as well.
    for symbol in element_tokens() {
        names.push(symbol);
    }
    names.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    names.dedup();
    names
}

/// Element symbols that may be a token: every element, except the symbols a label means as a
/// group — `Ac` (acetyl) and `Ts` (tosyl), the editor's [group
/// labels](crate::molecule::elements::GROUP_LABELS), and `Ar` (aryl) and `Pr` (propyl), the
/// [abbreviations](crate::reaction::formula::ABBREVIATION_SYMBOLS) arrow text refuses too.
/// So `OPr` stays unread rather than becoming praseodymium oxide.
fn element_tokens() -> impl Iterator<Item = &'static str> {
    use crate::molecule::elements;
    (1..=118)
        .filter_map(elements::symbol)
        .filter(|s| elements::element_of_label(s).is_some())
        .filter(|s| !crate::reaction::formula::ABBREVIATION_SYMBOLS.contains(s))
}

/// A lone atom of element `symbol`, as the fragment an element token stands for, or `None`
/// when `symbol` is not an [element token](element_tokens).
fn element_atom(symbol: &str) -> Option<Fragment> {
    element_tokens().any(|s| s == symbol).then(|| Fragment {
        name: symbol.to_string(),
        atoms: vec![FragAtom { element: symbol.to_string(), pos: [0.0, 0.0], charge: 0 }],
        bonds: Vec::new(),
        attach_idx: 0,
    })
}

// ═══════════════════════════════════════════════════════════════════════════════
// Exit atom
// ═══════════════════════════════════════════════════════════════════════════════

/// The atom the *next* fragment in a chain bonds to. See the module docs for the rule.
///
/// Always returns a valid index; for a fragment with no reachable non-hydrogen atom beyond its
/// entry (`Me`, `O`, `OH`, `NH2`, …) that index is `attach_idx` itself, so the chain simply
/// continues from the entry atom.
pub fn exit_idx(frag: &Fragment) -> usize {
    let n = frag.atoms.len();
    let entry = frag.attach_idx.min(n.saturating_sub(1));
    if n <= 1 {
        return entry;
    }

    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for b in &frag.bonds {
        if b.begin < n && b.end < n && b.begin != b.end {
            adj[b.begin].push(b.end);
            adj[b.end].push(b.begin);
        }
    }

    // Breadth-first distance from the entry atom.
    let mut dist = vec![usize::MAX; n];
    dist[entry] = 0;
    let mut queue = std::collections::VecDeque::from([entry]);
    while let Some(i) = queue.pop_front() {
        for &j in &adj[i] {
            if dist[j] == usize::MAX {
                dist[j] = dist[i] + 1;
                queue.push_back(j);
            }
        }
    }

    let mut best = entry;
    let mut best_dist = 0usize;
    for i in 0..n {
        if i == entry || dist[i] == usize::MAX || frag.atoms[i].element == "H" {
            continue;
        }
        // Strictly greater keeps the lowest index on a tie.
        if dist[i] > best_dist {
            best_dist = dist[i];
            best = i;
        }
    }
    best
}

// ═══════════════════════════════════════════════════════════════════════════════
// Composition
// ═══════════════════════════════════════════════════════════════════════════════

/// Weld fragments head-to-tail into one composite fragment.
///
/// `parts[0]`'s entry atom becomes the composite's `attach_idx` — i.e. the atom that will be
/// merged into the parent — and each subsequent part's entry is single-bonded to the previous
/// part's [`exit_idx`]. Atoms keep their element and charge; internal bonds keep their order.
///
/// Coordinates are re-laid out rather than copied: each part is rotated so its entry→exit axis
/// follows the chain, and translated so its entry sits one bond-length past the previous exit.
/// A part with no exit of its own (`OH`, `NH2`) has its body swung 90° off the chain, so its
/// hydrogens do not land on top of the next link. This matters beyond looks —
/// `Molecule::insert_fragment` merges any inserted atom that falls within `SNAP_RADIUS` of an
/// existing one, so overlapping coordinates would silently delete atoms.
pub fn compose(parts: &[Fragment]) -> Option<Fragment> {
    if parts.is_empty() || parts.iter().any(|f| f.atoms.is_empty()) {
        return None;
    }

    let mut atoms: Vec<FragAtom> = Vec::new();
    let mut bonds: Vec<FragBond> = Vec::new();
    let mut attach_idx = 0usize;
    let mut prev_exit: Option<usize> = None;
    let mut cursor = [0.0f32, 0.0f32];

    for (k, part) in parts.iter().enumerate() {
        let base = atoms.len();
        let entry = part.attach_idx.min(part.atoms.len() - 1);
        let exit = exit_idx(part);
        let dir = link_direction(k);

        // Rotation that puts this part's own axis onto the chain.
        let rot = match axis_angle(part, entry, exit) {
            Some(local) if exit != entry => dir - local,
            // Nothing downstream: swing the body clear of where the next link will go.
            Some(local) => dir + std::f32::consts::FRAC_PI_2 - local,
            None => 0.0, // a lone atom — rotation is a no-op
        };

        let (sin, cos) = rot.sin_cos();
        let origin = part.atoms[entry].pos;
        for a in &part.atoms {
            let (dx, dy) = (a.pos[0] - origin[0], a.pos[1] - origin[1]);
            atoms.push(FragAtom {
                element: a.element.clone(),
                pos: [cursor[0] + dx * cos - dy * sin, cursor[1] + dx * sin + dy * cos],
                charge: a.charge,
            });
        }
        for b in &part.bonds {
            if b.begin < part.atoms.len() && b.end < part.atoms.len() {
                bonds.push(FragBond {
                    begin: base + b.begin,
                    end: base + b.end,
                    order: b.order.clone(),
                });
            }
        }

        match prev_exit {
            None => attach_idx = base + entry,
            Some(prev) => bonds.push(FragBond {
                begin: prev,
                end: base + entry,
                order: BondOrder::Single,
            }),
        }

        prev_exit = Some(base + exit);
        let exit_pos = atoms[base + exit].pos;
        let next = link_direction(k + 1);
        cursor = [
            exit_pos[0] + LINK_LENGTH * next.cos(),
            exit_pos[1] + LINK_LENGTH * next.sin(),
        ];
    }

    Some(Fragment { name: String::new(), atoms, bonds, attach_idx })
}

/// Direction of the `k`-th link of the zigzag backbone.
fn link_direction(k: usize) -> f32 {
    if k % 2 == 0 { -ZIGZAG } else { ZIGZAG }
}

/// The fragment's own "forward" direction: entry→exit, or entry→body-centroid when the exit is
/// the entry. `None` for a fragment whose atoms all sit on the entry (a lone atom), where any
/// rotation is a no-op.
fn axis_angle(frag: &Fragment, entry: usize, exit: usize) -> Option<f32> {
    let origin = frag.atoms[entry].pos;
    let (dx, dy) = if exit != entry {
        let p = frag.atoms[exit].pos;
        (p[0] - origin[0], p[1] - origin[1])
    } else {
        let others = frag.atoms.len().checked_sub(1).filter(|n| *n > 0)? as f32;
        let (mut sx, mut sy) = (0.0f32, 0.0f32);
        for (i, a) in frag.atoms.iter().enumerate() {
            if i == entry {
                continue;
            }
            sx += a.pos[0] - origin[0];
            sy += a.pos[1] - origin[1];
        }
        (sx / others, sy / others)
    };
    (dx.hypot(dy) > 1e-6).then(|| dy.atan2(dx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FragAtomDef, FragBondDef, FragmentDef};
    use crate::molecule::canonical::is_isomorphic;
    use crate::molecule::Molecule;

    fn cfg() -> Config {
        // Never `Config::load` — that reads the developer's own `fragments/` directory.
        Config::embedded()
    }

    fn frag(name: &str) -> Fragment {
        cfg()
            .resolve_fragment(name)
            .unwrap_or_else(|| panic!("fragment {name} must exist"))
    }

    fn split(label: &str) -> Option<Vec<String>> {
        tokenize(&cfg(), label)
    }

    fn names(label: &str) -> Vec<String> {
        split(label).unwrap_or_else(|| panic!("{label} should tokenize"))
    }

    /// Elements of a molecule, sorted — a cheap composition fingerprint for messages.
    fn formula(mol: &Molecule) -> Vec<String> {
        let mut v: Vec<String> = mol.atoms.iter().map(|a| a.element.clone()).collect();
        v.sort();
        v
    }

    // ── Tokenizer ────────────────────────────────────────────────────────────

    #[test]
    fn splits_the_users_example() {
        assert_eq!(names("MeOPh"), ["Me", "O", "Ph"]);
    }

    /// Longest-match keeps every multi-character fragment name whole, both on its own and in
    /// the middle of a chain. A parser that nibbled from the front would read `NO2` as `N` plus
    /// an unreadable rest and silently refuse to expand the nitro group at all.
    #[test]
    fn longest_match_keeps_existing_names_whole() {
        for (label, want) in [
            ("NO2", vec!["NO2"]),
            ("OMe", vec!["OMe"]),
            ("CO2Me", vec!["CO2Me"]),
            ("SiH3", vec!["SiH3"]),
            ("CF3", vec!["CF3"]),
            ("TMS", vec!["TMS"]),
            ("TBS", vec!["TBS"]),
            ("MeNO2", vec!["Me", "NO2"]),
            ("MeCO2Me", vec!["Me", "CO2Me"]),
            ("MeSiH3", vec!["Me", "SiH3"]),
            ("MeCF3", vec!["Me", "CF3"]),
            ("MeTMS", vec!["Me", "TMS"]),
            ("MeTBS", vec!["Me", "TBS"]),
            ("PhOMe", vec!["Ph", "OMe"]),
        ] {
            assert_eq!(names(label), want, "{label}");
        }
    }

    /// A label with no reading is left completely alone — no partial expansion, no guessing.
    #[test]
    fn an_unreadable_label_has_no_split() {
        for label in ["", "Qq", "MeQq", "QqMe", "Benzene", "H2O", "CH3"] {
            assert!(split(label).is_none(), "{label:?} must not tokenize");
            assert!(
                resolve_label(&cfg(), label).is_none(),
                "{label:?} must not expand",
            );
        }
    }

    /// Bare element labels a user types by hand must stay bare. `P` and `Si` in particular are
    /// how `phosphate` / `tms_ether` are built through the label editor; turning either into a
    /// chain would corrupt those structures.
    #[test]
    fn bare_element_labels_do_not_expand() {
        for label in ["C", "P", "Si", "S", "B", "Mg", "Na"] {
            assert!(
                resolve_label(&cfg(), label).is_none(),
                "{label:?} is a plain element label and must not expand",
            );
        }
    }

    /// Case matters: `Ph` (phenyl) and `PH2` (phosphino) differ only in case, and both exist.
    #[test]
    fn resolution_is_case_sensitive() {
        assert_eq!(names("MePh"), ["Me", "Ph"]);
        assert_eq!(names("MePH2"), ["Me", "PH2"]);
        assert!(split("ME").is_none(), "ME is not Me");
        assert!(split("mePh").is_none(), "me is not Me");
    }

    /// Longest-match alone is not enough: a long first token can strand the rest of the label.
    ///
    /// No built-in fragment set produces this today, so it is exercised against a synthetic one
    /// — but fragment names come from `fragments/*.json`, which is user data, so the parser has
    /// to survive names that overlap this way.
    #[test]
    fn a_dead_end_longest_match_backtracks() {
        let mut config = cfg();
        config.fragments = ["Xy", "X", "yz"].iter().map(|n| lone_atom_def(n)).collect();

        // Greedy takes "Xy", leaves "z", and dies; the split that works is "X" + "yz".
        assert_eq!(
            tokenize(&config, "Xyz").as_deref(),
            Some(["X".to_string(), "yz".to_string()].as_slice()),
        );
        // And a label that really has no split still reports none.
        assert!(tokenize(&config, "Xyq").is_none());
    }

    fn lone_atom_def(name: &str) -> FragmentDef {
        FragmentDef {
            name: name.to_string(),
            atoms: vec![FragAtomDef { element: "C".into(), pos: [0.0, 0.0], charge: 0 }],
            bonds: Vec::<FragBondDef>::new(),
            attach_idx: 0,
        }
    }

    // ── Exit atom ────────────────────────────────────────────────────────────

    #[test]
    fn the_exit_is_the_farthest_heavy_atom() {
        // Lone atoms: entry is exit.
        for name in ["Me", "O", "H", "Cl", "F"] {
            let f = frag(name);
            assert_eq!(exit_idx(&f), f.attach_idx, "{name}");
        }
        // OMe: O in, methyl carbon out.
        let ome = frag("OMe");
        assert_eq!(exit_idx(&ome), 1);
        assert_eq!(ome.atoms[exit_idx(&ome)].element, "C");
        // Et: the terminal CH3.
        assert_eq!(exit_idx(&frag("Et")), 1);
        // Ph: the para carbon, three bonds from the ipso one.
        assert_eq!(exit_idx(&frag("Ph")), 3);
        // N3: the terminal nitrogen.
        assert_eq!(exit_idx(&frag("N3")), 2);
    }

    /// A chain must never be routed *through* a hydrogen — that would leave the H divalent.
    #[test]
    fn hydrogens_are_never_an_exit() {
        for name in ["OH", "NH2", "SH", "PH2", "SiH3", "BH2"] {
            let f = frag(name);
            let exit = exit_idx(&f);
            assert_ne!(f.atoms[exit].element, "H", "{name}: exit landed on a hydrogen");
        }
        // With nothing but hydrogens beyond it, the chain continues from the entry atom.
        assert_eq!(exit_idx(&frag("OH")), frag("OH").attach_idx);
    }

    // ── Composition ──────────────────────────────────────────────────────────

    /// `OMe` and `O` + `Me` describe the same thing, so they must build the same thing.
    #[test]
    fn ome_and_o_plus_me_are_the_same_structure() {
        let joined = compose(&[frag("O"), frag("Me")]).expect("O + Me composes");
        let direct = frag("OMe");
        assert!(
            is_isomorphic(&joined.to_molecule(), &direct.to_molecule()),
            "O+Me built {:?}, OMe is {:?}",
            formula(&joined.to_molecule()),
            formula(&direct.to_molecule()),
        );
        // Both join the rest of the structure through their oxygen.
        assert_eq!(joined.atoms[joined.attach_idx].element, "O");
        assert_eq!(direct.atoms[direct.attach_idx].element, "O");
    }

    /// The same equivalence one level up: however `MeOMe` is split, it is dimethyl ether.
    #[test]
    fn a_chain_is_independent_of_how_the_middle_is_split() {
        let three = compose(&[frag("Me"), frag("O"), frag("Me")]).unwrap();
        let two = compose(&[frag("Me"), frag("OMe")]).unwrap();
        assert!(is_isomorphic(&three.to_molecule(), &two.to_molecule()));
        assert!(is_isomorphic(
            &three.to_molecule(),
            &crate::testkit::load_fixture("dimethyl_ether"),
        ));
    }

    #[test]
    fn the_chain_is_bonded_head_to_tail() {
        let chain = resolve_label(&cfg(), "MeOPh").expect("MeOPh expands");
        assert_eq!(chain.atoms.len(), 8, "CH3 + O + C6H5");
        assert_eq!(chain.bonds.len(), 8, "6 ring + ring–O + O–CH3");
        assert_eq!(chain.atoms[chain.attach_idx].element, "C", "the methyl leads");
        assert_eq!(formula(&chain.to_molecule()), ["C", "C", "C", "C", "C", "C", "C", "O"]);
        assert!(
            is_isomorphic(&chain.to_molecule(), &crate::testkit::load_fixture("anisole")),
            "Me–O–Ph is anisole",
        );
    }

    /// Reading the same molecule from the other end must give the same molecule.
    #[test]
    fn a_chain_reads_the_same_from_either_end() {
        let a = resolve_label(&cfg(), "MeOPh").unwrap();
        let b = resolve_label(&cfg(), "PhOMe").unwrap();
        assert!(
            is_isomorphic(&a.to_molecule(), &b.to_molecule()),
            "MeOPh and PhOMe are both anisole",
        );
    }

    /// Nothing may be lost to `insert_fragment`'s snap radius, which silently merges atoms that
    /// land within `SNAP_RADIUS` of one another.
    #[test]
    fn composed_atoms_stay_far_enough_apart_to_survive_insertion() {
        let min_gap = crate::molecule::Molecule::SNAP_RADIUS;
        for label in [
            "MeOPh", "PhOMe", "MeOMe", "EtOEt", "PhPh", "MeOHMe", "MeNH2Me", "PhOPhOPh",
            "MeCF3", "MeNO2", "MeTMS", "BocOMe",
        ] {
            let f = resolve_label(&cfg(), label).unwrap_or_else(|| panic!("{label} expands"));
            for i in 0..f.atoms.len() {
                for j in (i + 1)..f.atoms.len() {
                    let (p, q) = (f.atoms[i].pos, f.atoms[j].pos);
                    let d = (p[0] - q[0]).hypot(p[1] - q[1]);
                    assert!(
                        d > min_gap,
                        "{label}: atoms {i} and {j} are {d:.3} apart (snap radius {min_gap})",
                    );
                }
            }
        }
    }

    /// Charges and bond orders inside a token are carried over verbatim.
    #[test]
    fn a_token_keeps_its_own_charges_and_bond_orders() {
        let f = resolve_label(&cfg(), "MeNO2").expect("MeNO2 expands");
        let nitro = frag("NO2");
        let charges: i32 = f.atoms.iter().map(|a| a.charge as i32).sum();
        assert_eq!(charges, nitro.atoms.iter().map(|a| a.charge as i32).sum::<i32>());
        assert_eq!(
            f.bonds.iter().filter(|b| b.order == BondOrder::Double).count(),
            1,
            "the N=O survives",
        );
    }

    // ── Element symbols as tokens ────────────────────────────────────────────

    /// A label spelled from element symbols is a single-bonded chain of those atoms: `NaH`
    /// is Na–H, the reagent written over an arrow.
    #[test]
    fn element_symbols_are_tokens_too() {
        assert_eq!(names("NaH"), ["Na", "H"]);
        let f = resolve_label(&cfg(), "NaH").expect("NaH expands");
        assert_eq!(f.atoms.iter().map(|a| a.element.as_str()).collect::<Vec<_>>(), ["Na", "H"]);
        assert_eq!(f.atoms[f.attach_idx].element, "Na", "the leftmost token leads");
        assert_eq!(f.bonds.len(), 1);
        assert_eq!(f.bonds[0].order, BondOrder::Single);
        let mw = crate::reaction::MolecularFormula::of(&f.to_molecule()).weight().unwrap();
        assert!((mw - 24.00).abs() < 0.01, "Na–H weighs {mw:.2}");
    }

    /// Element symbols and fragment names mix freely in one chain.
    #[test]
    fn elements_and_fragments_mix_in_one_chain() {
        assert_eq!(names("NaOMe"), ["Na", "OMe"]);
        let f = resolve_label(&cfg(), "NaOMe").expect("NaOMe expands");
        assert_eq!(formula(&f.to_molecule()), ["C", "Na", "O"]);
        assert_eq!(f.bonds.len(), 2, "Na–O–C");
        assert!(f.bonds.iter().all(|b| b.order == BondOrder::Single));
        assert_eq!(f.atoms[f.attach_idx].element, "Na");

        assert_eq!(names("MeMgBr"), ["Me", "MgBr"]);
        assert_eq!(names("LiAlH"), ["Li", "Al", "H"]);
    }

    /// A count is not a token: `H4` is no element and no fragment, so `LiAlH4` — like `H2O`
    /// and `CH3` — has no chain reading, and the label is left alone.
    #[test]
    fn counted_element_labels_still_have_no_split() {
        for label in ["LiAlH4", "NaBH4", "H2O", "CH3", "K2CO3"] {
            assert!(split(label).is_none(), "{label:?} must not tokenize");
            assert!(resolve_label(&cfg(), label).is_none(), "{label:?} must not expand");
        }
    }

    /// The ambiguity rules. Longest match first, as for fragments; a name that is both a
    /// fragment and an element symbol is the fragment; matching stays case-sensitive.
    #[test]
    fn element_and_fragment_ambiguity_is_settled_consistently() {
        // Longest first: the fragment `MgBr` beats `Mg` + `Br`, `OMe` beats `O` + `Me`, and the
        // two-letter symbol `Co` (cobalt) beats nothing shorter, since `o` is no token.
        assert_eq!(names("MgBr"), ["MgBr"]);
        assert_eq!(names("Co"), ["Co"]);
        // Case decides `Co` (cobalt) against `CO` (C + O).
        assert_eq!(names("CO"), ["C", "O"]);
        assert_eq!(names("MeCo"), ["Me", "Co"]);
        // `Ac` and `Ts` are acetyl and tosyl, never actinium and tennessine.
        let ac = resolve_label(&cfg(), "MeAc").expect("MeAc expands");
        assert!(ac.atoms.len() > 2, "Ac is the acetyl fragment, not one actinium atom");
        let mut config = cfg();
        config.fragments.clear();
        assert!(tokenize(&config, "Ac").is_none(), "Ac is a group label even with no fragment");
        assert!(tokenize(&config, "Ts").is_none(), "Ts is a group label even with no fragment");
        // `Ar` and `Pr` are aryl and propyl in a label, as `MolecularFormula::parse` reads them.
        for label in ["OAr", "OPr", "ArH", "PrOH"] {
            assert!(split(label).is_none(), "{label:?}: Ar/Pr are abbreviations, not elements");
        }
        // `B` + `u` is no reading of `Bu`: lowercase `u` starts no token.
        assert!(split("Bu").is_none());
    }

    /// A label that is a fragment name is still never split, element symbols or not.
    #[test]
    fn exact_fragment_labels_are_not_split_into_elements() {
        for label in ["NO2", "OMe", "CO2Me", "OH", "NH2", "SiH3", "CF3", "MgBr", "Cl"] {
            assert_eq!(names(label), [label], "{label}");
            let before = cfg().resolve_fragment(label).unwrap();
            let after = resolve_label(&cfg(), label).unwrap();
            assert_eq!(after.atoms.len(), before.atoms.len(), "{label}");
            assert_eq!(after.bonds.len(), before.bonds.len(), "{label}");
        }
    }

    /// A very long label tokenizes without recursion — on a thread with a small stack — and
    /// in time linear in its length, even when every prefix is ambiguous (`C`/`Co`/`CO…`).
    #[test]
    fn long_labels_tokenize_without_recursion_in_linear_time() {
        let start = std::time::Instant::now();
        let (na, mixed, dead) = std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let config = cfg();
                let na = tokenize(&config, &"Na".repeat(20_000)).map(|t| t.len());
                let mixed = tokenize(&config, &"CoCOCNOMe".repeat(2_000)).map(|t| t.len());
                // Readable up to the very end, then a dead end: every split must fail.
                let dead = tokenize(&config, &format!("{}q", "CO".repeat(10_000)));
                (na, mixed, dead)
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(na, Some(20_000));
        assert!(mixed.is_some());
        assert_eq!(dead, None);
        assert!(start.elapsed() < std::time::Duration::from_secs(30), "{:?}", start.elapsed());
    }

    // ── Non-regression ───────────────────────────────────────────────────────

    /// Every label that resolved before must still resolve to the *identical* fragment. This is
    /// the whole safety argument for putting the chain reading behind the exact-name lookup.
    #[test]
    fn every_existing_fragment_name_resolves_unchanged() {
        let config = cfg();
        for def in &config.fragments {
            let before = config.resolve_fragment(&def.name).expect("name resolves");
            let after = resolve_label(&config, &def.name).expect("name still resolves");
            assert_eq!(after.name, before.name, "{}", def.name);
            assert_eq!(after.attach_idx, before.attach_idx, "{}", def.name);
            assert_eq!(after.atoms.len(), before.atoms.len(), "{}", def.name);
            assert_eq!(after.bonds.len(), before.bonds.len(), "{}", def.name);
            for (a, b) in after.atoms.iter().zip(&before.atoms) {
                assert_eq!((&a.element, a.pos, a.charge), (&b.element, b.pos, b.charge));
            }
        }
        // The generated names `resolve_fragment` also understands.
        for name in ["benzene", "ring3", "ring6", "ring12"] {
            let before = config.resolve_fragment(name).expect("name resolves");
            let after = resolve_label(&config, name).expect("name still resolves");
            assert_eq!(after.atoms.len(), before.atoms.len(), "{name}");
        }
    }
}
