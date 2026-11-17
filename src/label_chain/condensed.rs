//! Condensed formulas as atom labels: `CN` is C≡N, `CHO` is C(=O)H, `CO2Et` is C(=O)–O–Et.
//!
//! This is the reading [`resolve_label`](super::resolve_label) gives a label that is not a
//! fragment name. The label is split into **units** — fragment names and element symbols, each
//! with an optional count, and parenthesised groups — and the units are joined into a tree whose
//! bond orders are whatever the valences demand. A label is read only when every atom it
//! spells comes out with exactly its valence filled; anything else has no reading and the
//! label is left alone. Expanding nothing is always preferred to expanding a guess.
//!
//! # The rules
//!
//! * **The first unit is the attachment atom.** It must keep a valence free for the bond to the
//!   rest of the structure; when the caller says how many bonds the labelled atom already has,
//!   it must keep exactly that many (`CO` reads on an atom with two bonds, as a ketone carbon,
//!   and not on a chain end, where it would silently become an aldehyde).
//! * **Hydrogens belong to the atom before them** (`CH2`, `NH`, `OH`) and are not stored, like
//!   every implicit hydrogen of the editor: they are what the atom's valence must leave over.
//! * **A count repeats a unit on the same atom** (`CCl3`, `NMe2`, `SO2`), and a parenthesised
//!   group hangs as a whole from the atom it is attached to (`CH(CH3)2`, `B(OH)2`).
//! * **A unit stays on the earliest atom that has room for it.** The label lists a tree in the
//!   order it is walked, so each unit can hang from the unit before it or from any atom on the
//!   way back to the first; the candidates are tried from the first atom on, and the first one
//!   that lets every valence come out exact is taken. That is the conventional reading:
//!   `COOH` is C(=O)OH and not a peroxide, `SO2Me` a sulfone and not a sulfinate ester, while
//!   `CH2OMe` has no room on the carbon and continues from the oxygen.
//! * **Bond orders follow from the valences**: an atom bonds to the atom it hangs from with
//!   whatever its own valence has left (one to three). Sulfur and phosphorus take their higher
//!   valences only through terminal oxygens (`SO2`, `P(O)`), so `SSMe` is a disulfide.
//! * **A fragment is joined by single bonds** and keeps its own atoms as they are. The label
//!   continues *through* a fragment only when that is unambiguous — a lone atom (`Me`), a plain
//!   chain (`Et`, from its far end) or a plain ring (`Ph`, from the opposite atom). A group with
//!   one point of attachment (`Ac`, `Boc`, `Ts`, `Bn`, `OH`) ends its branch.
//! * **An element with no standard valence** (`Na`, `Sn`, …) takes single bonds, as it did when
//!   labels were plain chains: `NaOMe` is Na–O–CH3.

use super::{element_atom, token_names};
use crate::config::Config;
use crate::molecule::fragment::{FragAtom, FragBond, Fragment};
use crate::molecule::{charged_valence, max_valence, normal_valence, BondOrder, Molecule};
use std::f32::consts::{FRAC_PI_3, PI, TAU};

/// Bond length of the layout, matching `widget::DEFAULT_BOND_LENGTH` and the fragment files.
const BOND: f32 = 1.5;
/// Longest label read, in bytes. Longer text is no atom label.
const MAX_LABEL: usize = 64;
/// Largest count a unit or a group may carry.
const MAX_COUNT: usize = 12;
/// Most ways of splitting a label into names that are tried.
const MAX_SPLITS: usize = 32;
/// Most units a label may come to once its counts are written out.
const MAX_UNITS: usize = 48;
/// Most attachment choices tried before a label is given up as unreadable.
const MAX_STEPS: usize = 20_000;

/// Other spellings of a built-in fragment's name: a prefix written with a hyphen, the *n*- of
/// a straight chain, and the names a group is as often written by. A fragment a user saves
/// under one of these spellings is found first and wins.
const ALIASES: [(&str, &str); 11] = [
    ("t-Bu", "tBu"),
    ("i-Pr", "iPr"),
    ("i-Bu", "iBu"),
    ("s-Bu", "sBu"),
    ("n-Pr", "Pr"),
    ("nPr", "Pr"),
    ("n-Bu", "Bu"),
    ("nBu", "Bu"),
    ("Bpin", "BPin"),
    ("TBDMS", "TBS"),
    ("C6H5", "Ph"),
];

/// The fragment `name` is another spelling of, if any.
fn alias(name: &str) -> Option<&'static str> {
    ALIASES.iter().find(|(from, _)| *from == name).map(|(_, to)| *to)
}

/// The fragment called `name`, by its own name or by an [alias](ALIASES).
pub(super) fn fragment_named(config: &Config, name: &str) -> Option<Fragment> {
    config
        .resolve_fragment(name)
        .or_else(|| alias(name).and_then(|canonical| config.resolve_fragment(canonical)))
}

/// The one fragment whose name (or alias) is `text` but for the case of its letters. `None`
/// when there is none or more than one, so `ph` is `Ph` only while nothing else is spelled so.
pub(super) fn fragment_but_for_case(config: &Config, text: &str) -> Option<Fragment> {
    let mut found: Option<&str> = None;
    let own = config.fragments.iter().map(|f| (f.name.as_str(), f.name.as_str()));
    for (name, fragment) in own.chain(ALIASES) {
        if name.eq_ignore_ascii_case(text) && found != Some(fragment) {
            if found.is_some() {
                return None;
            }
            found = Some(fragment);
        }
    }
    config.resolve_fragment(found?)
}

/// A label's text without its rich-text markup (`CO_2H` → `CO2H`, `//t//Bu` → `tBu`), with
/// every kind of hyphen written as `-`.
pub(super) fn plain(label: &str) -> String {
    crate::widget::rich_text::RichText::parse(label)
        .spans
        .iter()
        .flat_map(|s| s.text.chars())
        .map(|c| if matches!(c, '\u{2010}'..='\u{2013}' | '\u{2212}') { '-' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Units
// ═══════════════════════════════════════════════════════════════════════════════

/// One piece of a label as written. A count of 1 is no count.
#[derive(Clone, Copy)]
enum Lexed<'a> {
    Open,
    Close(usize),
    Name(&'a str, usize),
}

/// A label as a tree of units and groups.
enum Item {
    Unit(String, usize),
    Group(Vec<Item>, usize),
}

/// The ways to split `label` into names, counts and parentheses, best first: the longest name
/// at each place, as [`tokenize`](super::tokenize) prefers, then the shorter ones. A count is
/// the digits after a name or a `)` that are not part of a longer name (`NO2` is the nitro
/// fragment before it is N and two O). At most [`MAX_SPLITS`] are returned; none when no split
/// covers the label.
///
/// More than one split matters because the longest name is not always the one meant: `COMe`
/// is C, O, Me (acetyl) rather than C and the fragment `OMe`, and `OCOMe` has no cobalt in it.
fn lex<'a>(names: &[&'a str], label: &str) -> Vec<Vec<Lexed<'a>>> {
    if label.is_empty() || label.len() > MAX_LABEL {
        return Vec::new();
    }
    let bytes = label.as_bytes();
    // The count written at `pos`, and where it ends.
    let count_at = |pos: usize| -> Option<(usize, usize)> {
        let digits = bytes[pos..].iter().take_while(|b| b.is_ascii_digit()).count();
        let count: usize = label.get(pos..pos + digits)?.parse().ok()?;
        (2..=MAX_COUNT).contains(&count).then_some((count, pos + digits))
    };
    // What may stand at `pos`, best first, and where each ends.
    let options = |pos: usize| -> Vec<(Lexed<'a>, usize)> {
        let mut out = Vec::new();
        match bytes[pos] {
            b'(' => out.push((Lexed::Open, pos + 1)),
            b')' => {
                if let Some((count, end)) = count_at(pos + 1) {
                    out.push((Lexed::Close(count), end));
                }
                out.push((Lexed::Close(1), pos + 1));
            }
            _ => {
                for &name in names.iter().filter(|n| bytes[pos..].starts_with(n.as_bytes())) {
                    let end = pos + name.len();
                    if let Some((count, end)) = count_at(end) {
                        out.push((Lexed::Name(name, count), end));
                    }
                    out.push((Lexed::Name(name, 1), end));
                }
            }
        }
        out
    };

    let mut readable = vec![false; label.len() + 1];
    readable[label.len()] = true;
    for pos in (0..label.len()).rev() {
        readable[pos] = options(pos).iter().any(|(_, end)| readable[*end]);
    }

    // Depth-first over the options that can still reach the end; the label is short
    // ([`MAX_LABEL`]), so the walk is as deep as that at most.
    fn walk<'a>(
        pos: usize,
        end: usize,
        options: &dyn Fn(usize) -> Vec<(Lexed<'a>, usize)>,
        readable: &[bool],
        so_far: &mut Vec<Lexed<'a>>,
        out: &mut Vec<Vec<Lexed<'a>>>,
    ) {
        if pos == end {
            out.push(so_far.clone());
            return;
        }
        for (lexed, next) in options(pos) {
            if readable[next] && out.len() < MAX_SPLITS {
                so_far.push(lexed);
                walk(next, end, options, readable, so_far, out);
                so_far.pop();
            }
        }
    }
    let mut out = Vec::new();
    if readable[0] {
        walk(0, label.len(), &options, &readable, &mut Vec::new(), &mut out);
    }
    out
}

/// The names a unit may be: fragment names, their aliases and element symbols, longest first.
///
/// A fragment whose name spells its own structure (`OMe`, `OH`, `CF3`, `CO2Me`) is left out:
/// inside a longer label it is read as the atoms it is written with, so that those take part
/// in the reading like any others. `SOMe` is then a sulfoxide as `SOEt` is, not a fragment
/// `OMe` hung from the sulfur. A name that spells something else (`TIPS` is no Ti–P=S) or
/// nothing (`NO2`, `Ph`) stays a unit.
fn unit_names(config: &Config) -> Vec<&str> {
    let mut names = token_names(config);
    names.extend(ALIASES.iter().map(|(from, _)| *from));
    names.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    names.dedup();
    let spelled_out: Vec<&str> = config
        .fragments
        .iter()
        .filter(|def| def.atoms.len() > 1)
        .filter(|def| {
            let others: Vec<&str> = names.iter().copied().filter(|n| *n != def.name).collect();
            read_with(config, &others, &def.name, None, true)
                .is_some_and(|spelled| same_heavy_atoms(&spelled, &def.to_fragment()))
        })
        .map(|def| def.name.as_str())
        .collect();
    names.retain(|n| !spelled_out.contains(n));
    names
}

/// Whether two fragments are the same structure, attached through the same atom, hydrogens
/// aside (a fragment file may carry as atoms the hydrogens a formula only counts).
fn same_heavy_atoms(a: &Fragment, b: &Fragment) -> bool {
    let heavy = |frag: &Fragment| {
        let mut mol = Molecule::default();
        let ids: Vec<Option<u32>> = frag
            .atoms
            .iter()
            .enumerate()
            // The attachment atom is marked by a charge no real atom has, so that it must map
            // onto the other fragment's.
            .map(|(i, a)| {
                let charge = if i == frag.attach_idx { i8::MAX } else { a.charge };
                (a.element != "H").then(|| mol.add_atom(a.element.clone(), a.pos, charge))
            })
            .collect();
        for bond in &frag.bonds {
            if let (Some(begin), Some(end)) = (ids[bond.begin], ids[bond.end]) {
                mol.add_bond(begin, end, bond.order.clone());
            }
        }
        mol
    };
    crate::molecule::canonical::is_isomorphic(&heavy(a), &heavy(b))
}

/// `label` as trees of units, one for each way it splits whose parentheses pair, best first.
fn items(names: &[&str], label: &str) -> Vec<Vec<Item>> {
    fn tree(lexed: Vec<Lexed<'_>>) -> Option<Vec<Item>> {
        let mut open: Vec<Vec<Item>> = vec![Vec::new()];
        for lexed in lexed {
            match lexed {
                Lexed::Open => open.push(Vec::new()),
                Lexed::Close(count) => {
                    let inner = open.pop().filter(|inner| !inner.is_empty())?;
                    open.last_mut()?.push(Item::Group(inner, count));
                }
                Lexed::Name(name, count) => open.last_mut()?.push(Item::Unit(name.to_string(), count)),
            }
        }
        let top = open.pop()?;
        open.is_empty().then_some(top)
    }
    lex(names, label).into_iter().filter_map(tree).collect()
}

/// Whether `label` splits into units at all, read or not.
pub(super) fn splits(config: &Config, label: &str) -> bool {
    !items(&unit_names(config), label).is_empty()
}

/// `label` with its units in reverse order — what a label written toward the left spells
/// (`HOOC` → `COOH`, `EtO2C` → `CO2Et`, `O2N` → `NO2`, `(CH3)2N` → `N(CH3)2`). A count stays
/// with its unit and a parenthesised group is kept as written. `None` when there is nothing to
/// reverse.
pub(super) fn reversed(config: &Config, label: &str) -> Option<String> {
    fn write(items: &[Item], out: &mut String) {
        for item in items {
            let count = match item {
                Item::Unit(name, count) => {
                    out.push_str(name);
                    *count
                }
                Item::Group(inner, count) => {
                    out.push('(');
                    write(inner, out);
                    out.push(')');
                    *count
                }
            };
            if count > 1 {
                out.push_str(&count.to_string());
            }
        }
    }
    // The finest split, so that the atoms are what is reversed: `AcOMe` backwards is
    // Me, O, Ac, not the methoxy fragment followed by Ac. The first of the finest on a tie.
    let mut items = items(&unit_names(config), label).into_iter().rev().max_by_key(|items| items.len())?;
    if items.len() < 2 {
        return None;
    }
    items.reverse();
    let mut out = String::new();
    write(&items, &mut out);
    Some(out)
}

// ═══════════════════════════════════════════════════════════════════════════════
// Steps
// ═══════════════════════════════════════════════════════════════════════════════

/// What a unit stands for.
#[derive(Clone)]
enum Kind {
    /// One atom of an element, bonded as its valence demands.
    Elem(String),
    /// A fragment, joined by single bonds.
    Frag(Fragment),
}

/// The label as the steps that build it, counts written out.
enum Step {
    /// An atom or fragment, with the hydrogens written after it. `copy`: one more of the unit
    /// before it, on the same atom.
    Unit { kind: Kind, h: u8, copy: bool },
    /// A group begins. `copy`: one more of the group before it, on the same atom.
    Open { copy: bool },
    Close,
}

fn is_hydrogen(symbol: &str) -> bool {
    matches!(symbol, "H" | "D" | "T")
}

/// What the unit `name` stands for. A fragment that is just the atom of its own name (`O`,
/// `N`, `Cl`) is that element, so it takes multiple bonds like any other atom of it.
fn kind_of(config: &Config, name: &str) -> Option<Kind> {
    let Some(frag) = fragment_named(config, name) else {
        return element_atom(name).map(|_| Kind::Elem(name.to_string()));
    };
    let lone = frag.atoms.len() == 1 && frag.atoms[0].charge == 0 && frag.atoms[0].element == name;
    Some(if lone && element_atom(name).is_some() { Kind::Elem(name.to_string()) } else { Kind::Frag(frag) })
}

fn steps(config: &Config, items: &[Item], out: &mut Vec<Step>) -> Option<()> {
    // The unit a hydrogen written next would belong to, and the hydrogen isotope written last.
    let mut heavy: Option<usize> = None;
    let mut last_hydrogen: Option<&str> = None;
    for item in items {
        match item {
            Item::Unit(name, count) => {
                let kind = kind_of(config, name)?;
                let entry = match &kind {
                    Kind::Elem(symbol) => symbol.as_str(),
                    Kind::Frag(f) => f.atoms.get(f.attach_idx)?.element.as_str(),
                };
                if is_hydrogen(entry) {
                    // Hydrogens are counted, not repeated: `NHH` and `DDP` spell nothing.
                    if last_hydrogen == Some(name.as_str()) {
                        return None;
                    }
                    last_hydrogen = Some(name.as_str());
                    if name == "H"
                        && let Some(Step::Unit { h, .. }) = heavy.and_then(|i| out.get_mut(i))
                    {
                        *h += *count as u8;
                        continue;
                    }
                    // A hydrogen nothing can count in (`AlH4`) is a formula's, not a group's.
                    if name == "H" && *count != 1 {
                        return None;
                    }
                } else {
                    last_hydrogen = None;
                }
                for k in 0..*count {
                    out.push(Step::Unit { kind: kind.clone(), h: 0, copy: k > 0 });
                }
                // Only an atom with a valence can be told how many hydrogens it has.
                let counted = matches!(&kind, Kind::Elem(e) if !is_hydrogen(e) && normal_valence(e).is_some());
                heavy = counted.then(|| out.len() - 1);
            }
            Item::Group(inner, count) => {
                for k in 0..*count {
                    out.push(Step::Open { copy: k > 0 });
                    steps(config, inner, out)?;
                    out.push(Step::Close);
                }
                heavy = None;
                last_hydrogen = None;
            }
        }
        if out.len() > MAX_UNITS {
            return None;
        }
    }
    Some(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// The tree
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Clone)]
struct Node {
    kind: Kind,
    h: u8,
    parent: Option<usize>,
    children: Vec<usize>,
}

/// A partly built tree. `stack` is the way back from the last unit to the first: the atoms the
/// next unit may hang from. Inside a group only the group's own atoms (`stack[floor..]`) are.
#[derive(Clone, Default)]
struct Partial {
    nodes: Vec<Node>,
    stack: Vec<usize>,
    floor: usize,
    /// For each open group: the stack length and floor to go back to when it closes.
    groups: Vec<(usize, usize)>,
    /// The atom the first unit of a group just opened hangs from.
    pending: Option<usize>,
    /// The atom the last unit was hung from, for its copies.
    last_anchor: Option<usize>,
}

/// The atom of `frag` a label continues from, when there is exactly one sensible answer: the
/// atom itself for a lone atom, the far end of a plain single-bonded chain entered at one end,
/// the opposite atom of a plain even ring. It must have a valence left (`has_parent`: one is
/// already taken by the bond the fragment hangs from).
fn continuation(frag: &Fragment, has_parent: bool) -> Option<usize> {
    let n = frag.atoms.len();
    let entry = frag.attach_idx;
    let mut degree = vec![0usize; n];
    let mut bonded = vec![0i16; n];
    for b in &frag.bonds {
        degree[b.begin] += 1;
        degree[b.end] += 1;
        bonded[b.begin] += b.order.valence();
        bonded[b.end] += b.order.valence();
    }
    let exit = if n == 1 {
        entry
    } else if frag.atoms.iter().any(|a| is_hydrogen(&a.element)) {
        return None;
    } else if frag.bonds.len() == n - 1
        && degree.iter().all(|d| (1..=2).contains(d))
        && degree[entry] == 1
        && frag.bonds.iter().all(|b| b.order == BondOrder::Single)
    {
        (0..n).find(|&i| i != entry && degree[i] == 1)?
    } else if frag.bonds.len() == n && n.is_multiple_of(2) && degree.iter().all(|d| *d == 2) {
        // Walk half way round. (A set of separate rings has the same counts, but is no fragment
        // anyone draws; the walk would simply end on some atom of the entry's ring.)
        let (mut prev, mut at) = (entry, entry);
        for _ in 0..n / 2 {
            let next = frag.bonds.iter().find_map(|b| {
                let other = if b.begin == at { b.end } else if b.end == at { b.begin } else { return None };
                (other != prev).then_some(other)
            })?;
            (prev, at) = (at, next);
        }
        at
    } else {
        return None;
    };
    let atom = &frag.atoms[exit];
    let taken = bonded[exit] + (exit == entry && has_parent) as i16;
    match charged_valence(&atom.element, atom.charge) {
        Some(valence) => (valence - taken >= 1).then_some(exit),
        // A hydrogen isotope has its one bond; anything else of no known valence may go on.
        None => (!(is_hydrogen(&atom.element) && taken > 0)).then_some(exit),
    }
}

/// Whether one more unit may hang from node `at`. A cheap bound; [`orders`] is the judge.
fn accepts(nodes: &[Node], at: usize, copy: bool) -> bool {
    let node = &nodes[at];
    let has_parent = node.parent.is_some();
    match &node.kind {
        Kind::Elem(e) => match max_valence(e) {
            Some(max) => {
                // The first atom keeps a bond for the structure it is written on — but a
                // leading hydrogen is a molecule of its own (`HCl`).
                let back = (has_parent || e != "H") as usize;
                node.h as usize + node.children.len() + 1 + back <= max as usize
            }
            // No known valence: one bond onward, or several of one counted unit (`SnBu3`).
            None => node.children.is_empty() || copy,
        },
        Kind::Frag(f) => node.children.is_empty() && continuation(f, has_parent).is_some(),
    }
}

/// The bond order from each node to the node it hangs from (index 0, the first atom: the
/// valence it has left for the rest of the structure), or `None` when some valence cannot come
/// out exact.
fn orders(nodes: &[Node]) -> Option<Vec<i16>> {
    fn order(nodes: &[Node], at: usize, out: &mut [i16]) -> Option<i16> {
        let node = &nodes[at];
        let mut sum = 0;
        for &child in &node.children {
            out[child] = order(nodes, child, out)?;
            sum += out[child];
        }
        let all_single = sum == node.children.len() as i16;
        let Kind::Elem(e) = &node.kind else { return all_single.then_some(1) };
        let Some(valence) = normal_valence(e) else { return (all_single && node.h == 0).then_some(1) };
        // Sulfur and phosphorus reach their higher valences through terminal oxygens only.
        let oxo = node
            .children
            .iter()
            .filter(|&&c| {
                let child = &nodes[c];
                matches!(&child.kind, Kind::Elem(o) if o == "O") && child.h == 0 && child.children.is_empty()
            })
            .count() as i16;
        let expanded = match e.as_str() {
            "S" => 2 * oxo.min(2),
            "P" => 2 * oxo.min(1),
            _ => 0,
        };
        let left = valence as i16 + expanded - node.h as i16 - sum;
        let wanted = if node.parent.is_some() { 1..=3 } else { 0..=i16::MAX };
        wanted.contains(&left).then_some(left)
    }
    let mut out = vec![0; nodes.len()];
    out[0] = order(nodes, 0, &mut out)?;
    Some(out)
}

/// What the first atom of a reading must have left for the rest of the structure.
#[derive(Clone, Copy)]
struct Want {
    /// Exactly this many valences, or (`None`) at least one.
    left: Option<i16>,
    /// The label is on an atom with no bonds, so it may be a molecule of its own (`HCl`).
    alone: bool,
}

/// Whether the finished tree is a reading of the label, and its bond orders if so.
fn solved(nodes: &[Node], want: Want) -> Option<Vec<i16>> {
    let orders = orders(nodes)?;
    let root = &nodes[0];
    let ok = match &root.kind {
        Kind::Frag(_) => true,
        Kind::Elem(e) if normal_valence(e).is_none() => !root.children.is_empty(),
        // A hydrogen in front is a molecule of its own (`HCl`), never a substituent.
        Kind::Elem(e) if e == "H" => orders[0] == 0 && want.alone,
        // A bare element symbol is an atom label, not a formula.
        Kind::Elem(_) if root.children.is_empty() && root.h == 0 => false,
        Kind::Elem(_) => want.left.map_or(orders[0] >= 1, |left| orders[0] == left),
    };
    ok.then_some(orders)
}

/// Hang the steps from `at` on, trying for each the atoms it may hang from, earliest first.
fn search(
    all: &[Step],
    at: usize,
    partial: Partial,
    want: Want,
    tried: &mut usize,
) -> Option<(Vec<Node>, Vec<i16>)> {
    *tried += 1;
    if *tried > MAX_STEPS {
        return None;
    }
    let Some(step) = all.get(at) else {
        let orders = solved(&partial.nodes, want)?;
        return Some((partial.nodes, orders));
    };
    match step {
        Step::Close => {
            let mut next = partial;
            let (depth, floor) = next.groups.pop()?;
            next.stack.truncate(depth);
            next.floor = floor;
            search(all, at + 1, next, want, tried)
        }
        Step::Open { copy } => {
            // A copy hangs where the group before it does: the atom its closing left on top.
            let from = if *copy { partial.stack.len().checked_sub(1)? } else { partial.floor };
            for position in from..partial.stack.len() {
                let mut next = partial.clone();
                next.stack.truncate(position + 1);
                next.groups.push((position + 1, next.floor));
                next.floor = position + 1;
                next.pending = Some(next.stack[position]);
                if let Some(found) = search(all, at + 1, next, want, tried) {
                    return Some(found);
                }
            }
            None
        }
        Step::Unit { kind, h, copy } => {
            let node = |parent| Node { kind: kind.clone(), h: *h, parent, children: Vec::new() };
            if partial.nodes.is_empty() {
                let mut next = partial;
                next.nodes.push(node(None));
                next.stack.push(0);
                return search(all, at + 1, next, want, tried);
            }
            let anchors: Vec<usize> = match (partial.pending, copy) {
                (Some(anchor), _) => vec![anchor],
                (None, true) => vec![partial.last_anchor?],
                (None, false) => partial.stack[partial.floor..].to_vec(),
            };
            for anchor in anchors {
                if !accepts(&partial.nodes, anchor, *copy) {
                    continue;
                }
                let mut next = partial.clone();
                next.pending = None;
                if let Some(position) = next.stack.iter().rposition(|&n| n == anchor) {
                    next.stack.truncate(position + 1);
                }
                let id = next.nodes.len();
                next.nodes.push(node(Some(anchor)));
                next.nodes[anchor].children.push(id);
                next.stack.push(id);
                next.last_anchor = Some(anchor);
                if let Some(found) = search(all, at + 1, next, want, tried) {
                    return Some(found);
                }
            }
            None
        }
    }
}

/// Read `label` as a condensed formula written on an atom that already has `bonds` bond orders
/// (`None`: not known). See the [module docs](self) for the rules.
///
/// Every way the label splits is tried, best split first — first for a reading whose first
/// atom has exactly the valences left that the atom's bonds take (one, when it has none or they
/// are not known), then, where the bonds do not say otherwise, for one that has any left.
///
/// `element_first`: the label must start with an element, whose valence then vouches for the
/// reading. A reversed label is read so.
pub(super) fn read(config: &Config, label: &str, bonds: Option<i16>, element_first: bool) -> Option<Fragment> {
    read_with(config, &unit_names(config), label, bonds, element_first)
}

/// [`read`] with `names` as the names a unit may be.
fn read_with(
    config: &Config,
    names: &[&str],
    label: &str,
    bonds: Option<i16>,
    element_first: bool,
) -> Option<Fragment> {
    let lists: Vec<Vec<Step>> = items(names, label)
        .iter()
        .filter_map(|items| {
            let mut all = Vec::new();
            steps(config, items, &mut all)?;
            // The first atom is one atom: `K2CO3` is a formula, not a group.
            let one_atom = match all.first()? {
                Step::Unit { kind, .. } => !element_first || matches!(kind, Kind::Elem(_)),
                _ => false,
            } && !matches!(all.get(1), Some(Step::Unit { copy: true, .. }));
            one_atom.then_some(all)
        })
        .collect();
    let drawn = bonds.filter(|b| *b > 0);
    let alone = bonds.unwrap_or(0) == 0;
    let mut wants = vec![Want { left: Some(drawn.unwrap_or(1)), alone }];
    if drawn.is_none() {
        wants.push(Want { left: None, alone });
    }
    let mut tried = 0;
    for want in wants {
        for all in &lists {
            if let Some((nodes, orders)) = search(all, 0, Partial::default(), want, &mut tried) {
                let layout = Layout { nodes: &nodes, orders: &orders, atoms: Vec::new(), bonds: Vec::new() };
                return layout.fragment();
            }
        }
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════════════
// Layout
// ═══════════════════════════════════════════════════════════════════════════════

/// Lays the tree out as a fragment: each atom's neighbours spread evenly around it, a chain in
/// a zigzag, a fragment turned so the bond it hangs from points away from its own bonds.
struct Layout<'a> {
    nodes: &'a [Node],
    orders: &'a [i16],
    atoms: Vec<FragAtom>,
    bonds: Vec<FragBond>,
}

/// The middle of the widest gap between `angles`, the directions already taken at an atom.
fn widest_gap(angles: &[f32]) -> f32 {
    let mut sorted: Vec<f32> = angles.iter().map(|a| a.rem_euclid(TAU)).collect();
    sorted.sort_by(f32::total_cmp);
    let mut best = (0.0, -1.0);
    for (k, &from) in sorted.iter().enumerate() {
        let to = if k + 1 < sorted.len() { sorted[k + 1] } else { sorted[0] + TAU };
        if to - from > best.1 {
            best = (from + (to - from) / 2.0, to - from);
        }
    }
    best.0
}

/// The turn a chain takes at depth `depth`, alternating so it runs as a zigzag.
fn zig(depth: usize) -> f32 {
    if depth.is_multiple_of(2) { FRAC_PI_3 } else { -FRAC_PI_3 }
}

fn step_from(pos: [f32; 2], direction: f32) -> [f32; 2] {
    [pos[0] + BOND * direction.cos(), pos[1] + BOND * direction.sin()]
}

impl Layout<'_> {
    fn fragment(mut self) -> Option<Fragment> {
        let attach_idx = self.place(0, [0.0, 0.0], None, 0);
        // `insert_fragment` merges atoms that land within its snap radius, so a layout that
        // crowds two atoms would silently lose one: such a label is not expanded at all.
        let clear = Molecule::SNAP_RADIUS * 1.5;
        for (i, a) in self.atoms.iter().enumerate() {
            for b in &self.atoms[i + 1..] {
                if (a.pos[0] - b.pos[0]).hypot(a.pos[1] - b.pos[1]) < clear {
                    return None;
                }
            }
        }
        Some(Fragment { name: String::new(), atoms: self.atoms, bonds: self.bonds, attach_idx })
    }

    /// Atoms in the subtree of node `at`.
    fn size(&self, at: usize) -> usize {
        let own = match &self.nodes[at].kind {
            Kind::Elem(_) => 1,
            Kind::Frag(f) => f.atoms.len(),
        };
        own + self.nodes[at].children.iter().map(|&c| self.size(c)).sum::<usize>()
    }

    fn bond(&mut self, begin: usize, end: usize, order: i16) {
        let order = match order {
            2 => BondOrder::Double,
            3 => BondOrder::Triple,
            _ => BondOrder::Single,
        };
        self.bonds.push(FragBond { begin, end, order });
    }

    /// Place node `at` with its entry atom at `pos`, reached along `incoming` (`None` for the
    /// first atom), and everything hanging from it. Returns the index of its entry atom.
    fn place(&mut self, at: usize, pos: [f32; 2], incoming: Option<f32>, depth: usize) -> usize {
        match self.nodes[at].kind.clone() {
            Kind::Elem(symbol) => self.place_atom(at, symbol, pos, incoming, depth),
            Kind::Frag(frag) => self.place_fragment(at, &frag, pos, incoming, depth),
        }
    }

    fn place_atom(&mut self, at: usize, symbol: String, pos: [f32; 2], incoming: Option<f32>, depth: usize) -> usize {
        let index = self.atoms.len();
        self.atoms.push(FragAtom { element: symbol, pos, charge: 0 });
        let children = self.nodes[at].children.clone();
        let count = children.len();
        let directions: Vec<f32> = match incoming {
            // The first atom: the bond to the rest of the structure takes the place a further
            // neighbour would (`NO2`: 120° apart; `SO2Me`: a cross). Three groups 120° apart
            // would look as well, but turned to clear the atom's own bond one of them can land
            // on its neighbour — and a ring that does is fused to it, not moved.
            None => (0..count).map(|k| k as f32 * TAU / (count + 1) as f32).collect(),
            Some(incoming) if count == 1 => {
                let (own, next) = (self.orders[at], self.orders[children[0]]);
                let straight = own == 3 || next == 3 || (own == 2 && next == 2);
                vec![if straight { incoming } else { incoming + zig(depth) }]
            }
            Some(incoming) => {
                // Evenly around the atom; the largest group goes on straightest.
                let mut slots: Vec<f32> =
                    (1..=count).map(|k| incoming + PI + k as f32 * TAU / (count + 1) as f32).collect();
                let off = |slot: &f32| {
                    let turn = (slot - incoming + PI).rem_euclid(TAU) - PI;
                    (turn.abs() * 100.0).round() as i32 * 2 + (turn.signum() != zig(depth).signum()) as i32
                };
                slots.sort_by_key(off);
                let mut by_size: Vec<usize> = (0..count).collect();
                by_size.sort_by_key(|&k| std::cmp::Reverse(self.size(children[k])));
                let mut directions = vec![0.0; count];
                for (slot, k) in slots.into_iter().zip(by_size) {
                    directions[k] = slot;
                }
                directions
            }
        };
        for (child, direction) in children.into_iter().zip(directions) {
            let entry = self.place(child, step_from(pos, direction), Some(direction), depth + 1);
            self.bond(index, entry, self.orders[child]);
        }
        index
    }

    fn place_fragment(&mut self, at: usize, frag: &Fragment, pos: [f32; 2], incoming: Option<f32>, depth: usize) -> usize {
        let entry = frag.attach_idx;
        let origin = frag.atoms[entry].pos;
        // Directions of an atom's bonds, in the fragment's own coordinates.
        let bond_angles = |atom: usize| -> Vec<f32> {
            frag.bonds
                .iter()
                .filter_map(|b| {
                    let other = if b.begin == atom { b.end } else if b.end == atom { b.begin } else { return None };
                    let (p, q) = (frag.atoms[atom].pos, frag.atoms[other].pos);
                    Some((q[1] - p[1]).atan2(q[0] - p[0]))
                })
                .collect()
        };
        let turn = match incoming {
            None => 0.0,
            Some(incoming) => match bond_angles(entry).as_slice() {
                [] => 0.0,
                [only] => incoming + zig(depth) - only,
                angles => incoming + PI - widest_gap(angles),
            },
        };
        // The first fragment keeps its atom order, and with it the atom its placement is aimed
        // by; a later one goes in entry atom first.
        let order: Vec<usize> = match incoming {
            None => (0..frag.atoms.len()).collect(),
            Some(_) => std::iter::once(entry).chain((0..frag.atoms.len()).filter(|&i| i != entry)).collect(),
        };
        let base = self.atoms.len();
        let mut index = vec![0; frag.atoms.len()];
        let (sin, cos) = turn.sin_cos();
        for (k, &i) in order.iter().enumerate() {
            index[i] = base + k;
            let a = &frag.atoms[i];
            let (dx, dy) = (a.pos[0] - origin[0], a.pos[1] - origin[1]);
            self.atoms.push(FragAtom {
                element: a.element.clone(),
                pos: [pos[0] + dx * cos - dy * sin, pos[1] + dx * sin + dy * cos],
                charge: a.charge,
            });
        }
        for b in &frag.bonds {
            self.bonds.push(FragBond { begin: index[b.begin], end: index[b.end], order: b.order.clone() });
        }
        if let Some(&child) = self.nodes[at].children.first() {
            // `accepts` let the child in, so the fragment has an atom to go on from.
            let exit = continuation(frag, incoming.is_some()).unwrap_or(entry);
            let mut taken: Vec<f32> = bond_angles(exit).into_iter().map(|a| a + turn).collect();
            if exit == entry {
                taken.extend(incoming.map(|incoming| incoming + PI));
            }
            let direction = match taken.as_slice() {
                [] => 0.0,
                [only] => only + PI + zig(depth),
                angles => widest_gap(angles),
            };
            let from = self.atoms[index[exit]].pos;
            let child_entry = self.place(child, step_from(from, direction), Some(direction), depth + 1);
            self.bond(index[exit], child_entry, 1);
        }
        index[entry]
    }
}
