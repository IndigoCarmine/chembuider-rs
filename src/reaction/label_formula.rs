//! The formula an atom label spells when it names groups as well as elements: `Pd(OAc)2`,
//! `PdCl2(amphos)2`, `[(TMEDA)Zn(OH)(OTf)]3`, `Pd(dppf)Cl2·CH2Cl2`.
//!
//! [`MolecularFormula::parse`] reads element symbols only, which is right for the text over an
//! arrow (`THF` and `MeOH` there are conditions, not columns of the table). A label on an atom
//! is different: it stands for a compound or a group, so its composition — and with it the
//! molecular weight the stoichiometry table needs — must come out even when it is written with
//! abbreviations and never expanded into a structure.
//!
//! # Groups
//!
//! The abbreviations are [`FormulaGroup`]s: a name, other spellings, and a formula. They are
//! process-wide ([`set_formula_groups`]), because [`MolecularFormula::of`] is called with
//! nothing but a molecule. Until the embedding app sets its own, they are the built-in
//! fragments, each counted as the substituent it expands into (`Ph` is C6H5, `OMe` CH3O), under
//! the spellings Expand also reads (`t-Bu`, `nBu`, `Bpin`).
//!
//! # Grammar
//!
//! ```text
//! label := part ( '·' part )*            an adduct or hydrate: CuSO4·5H2O
//! part  := [count] item+
//! item  := ( '(' item+ ')' | '[' item+ ']' | '{' item+ '}' | element | group ) [count]
//! ```
//!
//! Brackets nest to any depth and in any mix (`((tmeda)Zn(OH)(OTf))3`, `[Ir(cod)Cl]2`), each
//! closed by its own kind; one that is not gives [`LabelFormulaError::UnbalancedBrackets`].
//! Full-width forms — `（` `）` `［` `］`, digits, letters, and `・` for the dot — are read as
//! their ASCII equivalents, so a label typed with an input method left in full-width mode
//! reads the same.
//!
//! # Which reading wins
//!
//! A *unit* is a whole part, or everything inside one pair of brackets.
//!
//! Everything is matched exactly as written, case included: `tmeda` is a group only if a group
//! lists that spelling, and `Cod` is nothing.
//!
//! 1. A unit that is exactly a group's name (or one of its spellings) is that group — even when
//!    its letters also spell elements, so `Ni(COD)2` holds cyclooctadiene where `COD` is listed
//!    as a spelling of `cod`, not C, O and D.
//! 2. Otherwise the unit is read token by token: element symbols and group names, the longest
//!    first (`Co` is cobalt, `CO` carbon and oxygen, `Cp` the group), a group before an element
//!    of the same length (`Pr` is propyl), going back to a shorter token when the rest cannot
//!    be read. A group whose spelling reads as element symbols (`COD`, `DBU`) is no token: only
//!    a whole unit is taken for it (rule 1), so `CH3COD` stays acetaldehyde-d.
//!
//! A label with no reading has no formula, and so no weight — never a partial one. When what
//! could not be read is one unit of an otherwise well-formed formula, the error names it
//! ([`LabelFormulaError::UnknownGroup`]: `TEMDA` in `[(TEMDA)Zn(OH)(OTf)]3`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, OnceLock, RwLock};

use super::formula::{
    add, atomic_weight, counts_of, MolecularFormula, ABBREVIATIONS, ABBREVIATION_SYMBOLS, MAX_GROUP_DEPTH,
};
use crate::config::FragmentDef;
use crate::molecule::elements::{atomic_number, GROUP_LABELS};
use crate::molecule::BondOrder;

/// An abbreviation a written formula may hold, and what it counts as.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaGroup {
    /// The name as it is usually written: `Ph`, `dppf`, `Cp*`.
    pub name: String,
    /// Other spellings read as the same group (`t-Bu` for `tBu`, `COD` for `cod`). Names are
    /// matched exactly, so a spelling in another case counts only when it is listed.
    pub aliases: Vec<String>,
    /// The atoms the group stands for, as it is written in a compound: `OAc` is C2H3O2.
    pub formula: MolecularFormula,
    /// Given by its formula ([`FormulaGroup::written`]) rather than drawn as a fragment.
    pub written: bool,
}

impl FormulaGroup {
    /// A group given by its formula (elements only, as [`MolecularFormula::parse`] reads them):
    /// a ligand or a reagent with no structure to draw. `None` when the name cannot be written
    /// in a formula (see [`valid_name`]) or the formula does not read.
    pub fn written(name: &str, aliases: &[&str], formula: &str) -> Option<Self> {
        if !valid_name(name) || !aliases.iter().all(|a| valid_name(a)) {
            return None;
        }
        Some(Self {
            name: name.to_string(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            formula: MolecularFormula::parse(formula)?,
            written: true,
        })
    }

    /// A fragment as the group its name stands for in a formula: the atoms Expand puts on a
    /// singly bonded atom labelled with the name — the fragment with one hydrogen less on its
    /// attachment atom, if that atom has one to give (`Ph` is C6H5, `OH` is HO, `MgBr` BrMg).
    ///
    /// `None` for a fragment whose name is itself a formula of elements (`N`, `Cl`, `OH`, `CF3`,
    /// `N3`: the name is read as it is spelled, so such a label counts what it always did), one
    /// whose name cannot be written in a formula, and one holding an atom that is no plain
    /// element. `Pr` is the exception to the first: propyl, as for Expand, not praseodymium.
    pub fn of_fragment(def: &FragmentDef) -> Option<Self> {
        let name = def.name.as_str();
        let spelled = (atomic_number(name).is_some() || MolecularFormula::parse(name).is_some())
            && !GROUP_LABELS.contains(&name);
        if spelled || !valid_name(name) || def.validate().is_err() {
            return None;
        }
        let plain = |element: &str| atomic_weight(element).is_some() && !GROUP_LABELS.contains(&element);
        if !def.atoms.iter().all(|a| plain(&a.element)) {
            return None;
        }
        let mut mol = def.to_fragment().to_molecule();
        let attach = mol.atoms.get(def.attach_idx)?.id;
        // The bond the label's atom has to the rest of the structure.
        let rest = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        mol.add_bond(attach, rest, BondOrder::Single);
        let formula = counts_of(&mol, |id| id != rest);
        formula.weight()?;
        Some(Self { name: name.to_string(), aliases: Vec::new(), formula, written: false })
    }
}

/// Whether `name` can be a group's name in a formula: not empty, starting with a letter, and
/// free of what the grammar itself uses (brackets, the adduct dot, spaces, markup).
pub fn valid_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_alphabetic())
        && !name.chars().any(|c| is_open(c) || is_close(c) || is_adduct_dot(c) || c.is_whitespace() || "^_{}".contains(c))
}

/// The groups of `fragments` ([`FormulaGroup::of_fragment`]), under the other spellings Expand
/// reads for the built-in ones (`t-Bu`, `nBu`, `Bpin`, …).
pub fn fragment_groups(fragments: &[FragmentDef]) -> Vec<FormulaGroup> {
    let mut groups: Vec<FormulaGroup> = fragments.iter().filter_map(FormulaGroup::of_fragment).collect();
    for (alias, name) in crate::label_chain::ALIASES {
        // A fragment saved under the spelling itself is the one meant.
        if groups.iter().any(|g| g.name == alias) {
            continue;
        }
        if let Some(group) = groups.iter_mut().find(|g| g.name == name) {
            group.aliases.push(alias.to_string());
        }
    }
    groups
}

/// Why a label has no formula.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelFormulaError {
    /// The label is not a written formula: a plain element, a charge, free text.
    NotAFormula,
    /// The label is a formula but for this unit, which is no element and no known group.
    UnknownGroup(String),
    /// A bracket is never closed, closed without being opened, or closed by another kind.
    UnbalancedBrackets,
}

impl std::fmt::Display for LabelFormulaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAFormula => f.write_str("not a formula"),
            Self::UnknownGroup(name) => write!(f, "unknown group: {name}"),
            Self::UnbalancedBrackets => f.write_str("the brackets do not match"),
        }
    }
}

// ─── The process-wide table ───────────────────────────────────────────────────

/// Groups indexed for reading.
#[derive(Debug, Default)]
pub struct GroupTable {
    groups: Vec<FormulaGroup>,
    /// Spelling → group. A later group takes a spelling an earlier one had.
    exact: HashMap<String, usize>,
    /// The spellings that also read as a formula of elements (`COD`, `DBU`).
    spelled: HashSet<String>,
    /// The longest spelling, in characters.
    longest: usize,
}

impl GroupTable {
    /// Index `groups`. Where two share a spelling, the later one has it.
    pub fn new(groups: Vec<FormulaGroup>) -> Self {
        let mut exact: HashMap<String, usize> = HashMap::new();
        for (i, group) in groups.iter().enumerate() {
            for spelling in std::iter::once(&group.name).chain(&group.aliases) {
                exact.insert(spelling.clone(), i);
            }
        }
        let longest = exact.keys().map(|spelling| spelling.chars().count()).max().unwrap_or(0);
        let spelled = exact.keys().filter(|spelling| spells_elements(spelling)).cloned().collect();
        Self { groups, exact, spelled, longest }
    }

    /// The groups, in the order given.
    pub fn groups(&self) -> &[FormulaGroup] {
        &self.groups
    }

    fn exact(&self, text: &str) -> Option<&FormulaGroup> {
        self.exact.get(text).map(|&i| &self.groups[i])
    }

    /// The group the token `text` stands for inside a run: one of that spelling which does not
    /// read as elements.
    fn token(&self, text: &str) -> Option<&FormulaGroup> {
        if self.spelled.contains(text) { None } else { self.exact(text) }
    }
}

/// Whether `spelling` reads, from end to end, as the element symbols a label holds (with
/// counts): `COD` is C, O, D and `DBU` D, B, U; `IPr` is not, `Pr` being propyl in a label.
fn spells_elements(spelling: &str) -> bool {
    let chars: Vec<char> = spelling.chars().collect();
    let mut readable = vec![false; chars.len() + 1];
    readable[chars.len()] = true;
    for pos in (0..chars.len()).rev() {
        readable[pos] = (1..=2usize.min(chars.len() - pos)).any(|len| {
            let symbol: String = chars[pos..pos + len].iter().collect();
            let digits = chars[pos + len..].iter().take_while(|c| c.is_ascii_digit()).count();
            is_element_token(&symbol) && readable[pos + len + digits]
        });
    }
    !chars.is_empty() && readable[0]
}

/// Whether `symbol` is an element in a label: one with a weight, and not `Ar` (aryl) or `Pr`
/// (propyl), which are abbreviations there — as the reading of a label for Expand has it.
fn is_element_token(symbol: &str) -> bool {
    atomic_weight(symbol).is_some() && !ABBREVIATION_SYMBOLS.contains(&symbol)
}

static GROUPS: RwLock<Option<Arc<GroupTable>>> = RwLock::new(None);

/// The groups labels are read with: those last given to [`set_formula_groups`], or the
/// built-in fragments ([`fragment_groups`]) when none were.
pub fn formula_groups() -> Arc<GroupTable> {
    if let Some(table) = GROUPS.read().unwrap_or_else(|e| e.into_inner()).as_ref() {
        return table.clone();
    }
    static BUILT_IN: OnceLock<Arc<GroupTable>> = OnceLock::new();
    BUILT_IN
        .get_or_init(|| Arc::new(GroupTable::new(fragment_groups(&crate::config::Config::embedded().fragments))))
        .clone()
}

/// Replace the groups every label formula is read with, for the whole process: an app calls
/// this with its fragments ([`fragment_groups`]) and its formula-only groups
/// ([`FormulaGroup::written`]) whenever they change. Where two groups share a spelling, the
/// later one has it.
pub fn set_formula_groups(groups: Vec<FormulaGroup>) {
    *GROUPS.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(GroupTable::new(groups)));
}

// ─── Reading ──────────────────────────────────────────────────────────────────

fn is_open(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

fn is_close(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

/// The bracket that closes `open`.
fn closer(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        _ => '}',
    }
}

/// The dot between the parts of an adduct or hydrate: `·` (middle dot), `•`, `⋅`, or the
/// katakana middle dot `・` a Japanese input method types for it (`･` when half-width).
fn is_adduct_dot(c: char) -> bool {
    matches!(c, '\u{b7}' | '\u{2022}' | '\u{22c5}' | '\u{30fb}' | '\u{ff65}')
}

/// `c` as its ASCII equivalent when it is a full-width form (`（` `）` `［` `３` `Ｚ`, the
/// ideographic space): what an input method left in full-width mode types.
fn narrow(c: char) -> char {
    match c {
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
        '\u{3000}' => ' ',
        _ => c,
    }
}

type Counts = BTreeMap<String, usize>;

/// Why a unit did not read.
enum Unread {
    /// No formula at all (a syntax error, an overflowing count).
    Malformed,
    /// This run of text is no element and no group.
    Unknown(String),
}

/// Read `text` (plain, no markup) as a formula of elements and `table`'s groups; see the
/// [module docs](self).
pub fn parse_with_groups(text: &str, table: &GroupTable) -> Result<MolecularFormula, LabelFormulaError> {
    let chars: Vec<char> = text.chars().map(narrow).collect();
    let text: String = chars.iter().collect();
    let text = text.as_str();
    // Brackets are matched, and their nesting bounded, before anything recurses: labels are
    // user input.
    let mut open: Vec<char> = Vec::new();
    let mut deepest = 0usize;
    for &c in &chars {
        if is_open(c) {
            open.push(c);
            deepest = deepest.max(open.len());
        } else if is_close(c) && open.pop().map(closer) != Some(c) {
            return Err(LabelFormulaError::UnbalancedBrackets);
        }
    }
    if !open.is_empty() {
        return Err(LabelFormulaError::UnbalancedBrackets);
    }
    if deepest > MAX_GROUP_DEPTH || chars.is_empty() {
        return Err(LabelFormulaError::NotAFormula);
    }
    // Reagent abbreviations that happen to spell elements (`NBS`), unless a group has the name.
    if ABBREVIATIONS.contains(&text) && table.exact(text).is_none() {
        return Err(LabelFormulaError::NotAFormula);
    }
    // A hole can be named only in a label with some structure around it; a label that is one
    // unreadable run is just no formula.
    let structured = chars.iter().any(|&c| is_open(c) || is_adduct_dot(c));
    let whole = || -> Result<Counts, Unread> {
        let mut counts = Counts::new();
        for part in chars.split(|&c| is_adduct_dot(c)) {
            let part = trim(part);
            let digits = part.iter().take_while(|c| c.is_ascii_digit()).count();
            let times = if digits == 0 { 1 } else { number(&part[..digits]).ok_or(Unread::Malformed)? };
            merge(&mut counts, read_unit(trim(&part[digits..]), table)?, times)?;
        }
        Ok(counts)
    };
    let counts = whole().map_err(|unread| match unread {
        Unread::Unknown(run) if structured => LabelFormulaError::UnknownGroup(run),
        _ => LabelFormulaError::NotAFormula,
    })?;
    if counts.is_empty() {
        return Err(LabelFormulaError::NotAFormula);
    }
    Ok(MolecularFormula::from_counts(counts))
}

fn trim(mut chars: &[char]) -> &[char] {
    while let [first, rest @ ..] = chars {
        if !first.is_whitespace() {
            break;
        }
        chars = rest;
    }
    while let [rest @ .., last] = chars {
        if !last.is_whitespace() {
            break;
        }
        chars = rest;
    }
    chars
}

/// The count `digits` write; `None` for zero or one too large.
fn number(digits: &[char]) -> Option<usize> {
    let mut n = 0usize;
    for d in digits {
        n = n.checked_mul(10)?.checked_add(d.to_digit(10)? as usize)?;
    }
    (n > 0).then_some(n)
}

/// Add `times` × `group` to `counts`.
fn merge(counts: &mut Counts, group: Counts, times: usize) -> Result<(), Unread> {
    for (el, n) in group {
        let n = n.checked_mul(times).ok_or(Unread::Malformed)?;
        add(counts, el, n).ok_or(Unread::Malformed)?;
    }
    Ok(())
}

fn group_counts(group: &FormulaGroup) -> Counts {
    group.formula.counts.iter().cloned().collect()
}

/// Read one unit: a group by its whole name, or its items one after another.
fn read_unit(unit: &[char], table: &GroupTable) -> Result<Counts, Unread> {
    if unit.is_empty() {
        return Err(Unread::Malformed);
    }
    let text: String = unit.iter().collect();
    if let Some(group) = table.exact(&text) {
        return Ok(group_counts(group));
    }
    let mut counts = Counts::new();
    let mut pos = 0;
    while pos < unit.len() {
        let item = if is_open(unit[pos]) {
            let close = closing(unit, pos).ok_or(Unread::Malformed)?;
            let inner = read_unit(&unit[pos + 1..close], table)?;
            pos = close + 1;
            let digits = unit[pos..].iter().take_while(|c| c.is_ascii_digit()).count();
            let times = if digits == 0 { 1 } else { number(&unit[pos..pos + digits]).ok_or(Unread::Malformed)? };
            pos += digits;
            (inner, times)
        } else if is_close(unit[pos]) {
            return Err(Unread::Malformed);
        } else {
            // A run of tokens, up to the next bracket.
            let end = unit[pos..].iter().position(|&c| is_open(c) || is_close(c)).map_or(unit.len(), |n| pos + n);
            let run = read_run(&unit[pos..end], table)?;
            pos = end;
            (run, 1)
        };
        merge(&mut counts, item.0, item.1)?;
    }
    Ok(counts)
}

/// The index of the bracket closing the one at `open` (their kinds were matched already).
fn closing(unit: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (i, &c) in unit.iter().enumerate().skip(open) {
        if is_open(c) {
            depth += 1;
        } else if is_close(c) {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// What a token stands for, and how many characters it takes.
struct Token {
    len: usize,
    counts: Counts,
}

/// The tokens that can start at `run[pos]`, best first: the longest first, and a group before
/// an element of its length.
fn tokens_at(run: &[char], pos: usize, table: &GroupTable) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    for len in (1..=table.longest.max(2).min(run.len() - pos)).rev() {
        let text: String = run[pos..pos + len].iter().collect();
        if let Some(group) = table.token(&text) {
            tokens.push(Token { len, counts: group_counts(group) });
        }
        if len <= 2 && is_element_token(&text) {
            tokens.push(Token { len, counts: Counts::from([(text, 1)]) });
        }
    }
    tokens
}

/// Read a run of tokens with their counts (`PdCl2`, `PPh3`, `CH2Cl2`): no brackets inside.
fn read_run(run: &[char], table: &GroupTable) -> Result<Counts, Unread> {
    let unknown = || Unread::Unknown(run.iter().collect());
    // After a token, its count: all the digits that follow.
    let after = |pos: usize, token: &Token| {
        let digits_at = pos + token.len;
        digits_at + run[digits_at..].iter().take_while(|c| c.is_ascii_digit()).count()
    };
    // readable[pos]: the run from `pos` on splits into tokens. Filled from the end, so the walk
    // from the front never has to back out of a choice: linear, with no recursion.
    let mut readable = vec![false; run.len() + 1];
    readable[run.len()] = true;
    for pos in (0..run.len()).rev() {
        readable[pos] = tokens_at(run, pos, table).iter().any(|t| readable[after(pos, t)]);
    }
    if !readable[0] {
        return Err(unknown());
    }
    let mut counts = Counts::new();
    let mut pos = 0;
    while pos < run.len() {
        let token = tokens_at(run, pos, table).into_iter().find(|t| readable[after(pos, t)]).ok_or_else(unknown)?;
        let digits_at = pos + token.len;
        let end = after(pos, &token);
        let times = if end == digits_at { 1 } else { number(&run[digits_at..end]).ok_or(Unread::Malformed)? };
        merge(&mut counts, token.counts, times)?;
        pos = end;
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// The built-in fragments and a few ligands, as an app would register them.
    fn table() -> GroupTable {
        let mut groups = vec![
            FormulaGroup::written("cod", &["COD"], "C8H12").unwrap(),
            FormulaGroup::written("dppf", &[], "C34H28FeP2").unwrap(),
            FormulaGroup::written("amphos", &[], "C16H28NP").unwrap(),
            FormulaGroup::written("dba", &[], "C17H14O").unwrap(),
            FormulaGroup::written("TMEDA", &["tmeda"], "C6H16N2").unwrap(),
            FormulaGroup::written("DBU", &[], "C9H16N2").unwrap(),
            FormulaGroup::written("bpy", &["bipy"], "C10H8N2").unwrap(),
            FormulaGroup::written("Cp", &[], "C5H5").unwrap(),
            FormulaGroup::written("Cp*", &[], "C10H15").unwrap(),
            FormulaGroup::written("IPr", &[], "C27H36N2").unwrap(),
        ];
        groups.extend(fragment_groups(&Config::embedded().fragments));
        GroupTable::new(groups)
    }

    fn read(text: &str) -> Result<String, LabelFormulaError> {
        parse_with_groups(text, &table()).map(|f| f.plain())
    }

    fn weight(text: &str) -> String {
        format!("{:.2}", parse_with_groups(text, &table()).unwrap().weight().unwrap())
    }

    /// A fragment counts as the substituent it expands into.
    #[test]
    fn fragments_are_groups_with_the_formula_of_the_substituent() {
        let table = table();
        for (name, formula) in [
            ("Ph", "C6H5"),
            ("Me", "CH3"),
            ("OMe", "CH3O"),
            ("Ac", "C2H3O"),
            ("Tf", "CF3O2S"),
            ("Ts", "C7H7O2S"),
            ("Boc", "C5H9O2"),
            ("tBu", "C4H9"),
            ("Cy", "C6H11"),
            ("Pr", "C3H7"),
            ("Bn", "C7H7"),
            ("BPin", "C6H12BO2"),
            ("TMS", "C3H9Si"),
        ] {
            assert_eq!(table.exact(name).map(|g| g.formula.plain()).as_deref(), Some(formula), "{name}");
        }
        // A name that spells elements is read as it is spelled, not as the fragment.
        for name in ["N", "O", "Cl", "H", "D", "Li", "OH", "NH2", "CF3", "NO2", "N3", "MgBr", "C2H5"] {
            assert!(table.exact(name).is_none(), "{name}");
        }
        assert_eq!(table.exact("t-Bu").map(|g| g.name.as_str()), Some("tBu"));
    }

    /// Everything [`MolecularFormula::parse`] reads, reads the same.
    #[test]
    fn formulas_of_elements_read_as_before() {
        for text in [
            "K2CO3", "KHF2", "NaH", "NaOH", "KOH", "LiAlH4", "NaBH4", "Cs2CO3", "K3PO4", "Na2SO4", "MgSO4",
            "NaHCO3", "NH4Cl", "CuI", "H2O", "HCl", "Ca(OH)2", "Ca((OH)2)3", "CH3", "CN", "CHO", "CoCl2", "CO",
            "BN", "PH3", "NO", "Co", "SiCl4", "C6H12O6",
        ] {
            let before = MolecularFormula::parse(text).unwrap_or_else(|| panic!("{text} is a formula"));
            assert_eq!(parse_with_groups(text, &table()), Ok(before), "{text}");
        }
    }

    #[test]
    fn complexes_written_with_ligands_have_a_formula_and_a_weight() {
        for (text, formula, mw) in [
            ("Pd(OAc)2", "C4H6O4Pd", "224.51"),
            ("PdCl2(amphos)2", "C32H56Cl2N2P2Pd", "708.08"),
            ("Pd(dppf)Cl2", "C34H28Cl2FeP2Pd", "731.71"),
            ("[(TMEDA)Zn(OH)(OTf)]3", "C21H51F9N6O12S3Zn3", "1042.97"),
            ("Pd(PPh3)4", "C72H60P4Pd", "1155.59"),
            ("Pd2(dba)3", "C51H42O3Pd2", "915.73"),
            ("Ni(cod)2", "C16H24Ni", "275.06"),
            ("[Ir(cod)Cl]2", "C16H24Cl2Ir2", "671.71"),
            ("RuCl2(PPh3)3", "C54H45Cl2P3Ru", "958.85"),
            ("Cu(OTf)2", "C2CuF6O6S2", "361.67"),
            ("Pd(dppf)Cl2\u{b7}CH2Cl2", "C35H30Cl4FeP2Pd", "816.64"),
            ("CuSO4\u{b7}5H2O", "CuH10O9S", "249.68"),
            ("Cp2ZrCl2", "C10H10Cl2Zr", "292.31"),
            ("Cp*2Fe", "C20H30Fe", "326.30"),
            ("P(tBu)3", "C12H27P", "202.32"),
            ("PCy3", "C18H33P", "280.44"),
            ("NEt3", "C6H15N", "101.19"),
            ("MeCN", "C2H3N", "41.05"),
            ("KOtBu", "C4H9KO", "112.21"),
            ("NaOMe", "CH3NaO", "54.02"),
            ("n-BuLi", "C4H9Li", "64.06"),
        ] {
            assert_eq!(read(text).as_deref(), Ok(formula), "{text}");
            assert_eq!(weight(text), mw, "{text}");
        }
    }

    /// The precedence rules of the module docs, one by one.
    #[test]
    fn elements_and_groups_do_not_pass_for_each_other() {
        // 1: a whole unit that is a group's spelling is the group, even one that spells elements.
        assert_eq!(read("Ni(COD)2").as_deref(), Ok("C16H24Ni"));
        assert_eq!(read("COD").as_deref(), Ok("C8H12"));
        assert_eq!(read("DBU").as_deref(), Ok("C9H16N2"));
        assert_eq!(read("ZnCl2(tmeda)"), read("ZnCl2(TMEDA)"));
        // … but inside a run of tokens such a spelling is its elements.
        assert_eq!(read("CH3COD").as_deref(), Ok("C2H3DO"));
        assert_eq!(read("NaCOD").as_deref(), Ok("CDNaO"));
        // Case is part of the name: a spelling that is not listed is no group.
        assert_eq!(read("Ni(Cod)2"), Err(LabelFormulaError::UnknownGroup("Cod".into())));
        assert_eq!(read("Cod"), Err(LabelFormulaError::NotAFormula));
        // An upper-case spelling that is not listed reads as the elements its letters spell, as
        // any text does: a group written that way must list the spelling (as `cod` lists `COD`).
        assert_eq!(read("Pd(DPPF)Cl2").as_deref(), Ok("Cl2DFP2Pd"));
        assert_eq!(read("PdCl2(AmPhos)2"), Err(LabelFormulaError::UnknownGroup("AmPhos".into())));
        assert_eq!(read("Pd(Dba)2"), Err(LabelFormulaError::UnknownGroup("Dba".into())));
        assert_eq!(read("Zn(Tmeda)Cl2"), Err(LabelFormulaError::UnknownGroup("Tmeda".into())));
        assert_eq!(read("boc"), Err(LabelFormulaError::NotAFormula));
        assert_eq!(read("Ti(Oipr)4"), Err(LabelFormulaError::UnknownGroup("Oipr".into())));
        // These are formulas of elements, never benzyl, cobalt, phenyl or Boc.
        assert_eq!(read("BN").as_deref(), Ok("BN"));
        assert_eq!(read("CO").as_deref(), Ok("CO"));
        assert_eq!(read("Co").as_deref(), Ok("Co"));
        assert_eq!(read("PH3").as_deref(), Ok("H3P"));
        assert_eq!(read("BOC").as_deref(), Ok("CBO"));
        assert_eq!(read("Fe(CO)5").as_deref(), Ok("C5FeO5"));
        // 2: the longest token first; cobalt is never a piece of `cod`.
        assert_eq!(read("CoCl2").as_deref(), Ok("Cl2Co"));
        assert_eq!(read("Co(cod)2").as_deref(), Ok("C16H24Co"));
        assert_eq!(read("Co2(CO)8").as_deref(), Ok("C8Co2O8"));
        assert_eq!(read("CpCo(CO)2").as_deref(), Ok("C7H5CoO2"));
        assert_eq!(read("Zn(tmeda)(PH3)").as_deref(), Ok("C6H19N2PZn"));
        // `Pr` is propyl and `Ar` no element, as for Expand.
        assert_eq!(read("OPr").as_deref(), Ok("C3H7O"));
        assert_eq!(read("OAr"), Err(LabelFormulaError::NotAFormula));
        // `iPr` and `IPr` differ only in case: each is itself.
        assert_eq!(read("Zn(iPr)2").as_deref(), Ok("C6H14Zn"));
        assert_eq!(read("Pd(IPr)Cl2").as_deref(), Ok("C27H36Cl2N2Pd"));
        assert_eq!(read("IPrCuCl").as_deref(), Ok("C27H36ClCuN2"), "I and Pr are not its elements");
        assert_eq!(read("Ti(OiPr)4").as_deref(), Ok("C12H28O4Ti"));
    }

    #[test]
    fn what_cannot_be_read_has_no_formula_and_is_named() {
        assert_eq!(read("[(TEMDA)Zn(OH)(OTf)]3"), Err(LabelFormulaError::UnknownGroup("TEMDA".into())));
        assert_eq!(read("Pd(xyzphos)Cl2"), Err(LabelFormulaError::UnknownGroup("xyzphos".into())));
        assert_eq!(read("Pd(dppf)Qq2"), Err(LabelFormulaError::UnknownGroup("Qq2".into())));
        for text in ["", "Qq", "R", "cat.", "rt", "()", "Pd()2", "Pd(OAc)0", "24 h", "NH4+"] {
            assert_eq!(read(text), Err(LabelFormulaError::NotAFormula), "{text:?}");
        }
        let huge = format!("Pd(OAc){}", usize::MAX);
        assert_eq!(read(&huge), Err(LabelFormulaError::NotAFormula));
        let deep = format!("{}C{}", "(".repeat(MAX_GROUP_DEPTH + 1), ")".repeat(MAX_GROUP_DEPTH + 1));
        assert_eq!(read(&deep), Err(LabelFormulaError::NotAFormula));
    }

    /// Brackets of every kind nest to any depth, and a group's listed spellings are read at
    /// every level.
    #[test]
    fn brackets_nest_in_any_mix() {
        let trimer = Ok("C21H51F9N6O12S3Zn3".to_string());
        for text in [
            "((tmeda)Zn(OH)(OTf))3",
            "((TMEDA)Zn(OH)(OTf))3",
            "[(tmeda)Zn(OH)(OTf)]3",
            "[(TMEDA)Zn(OH)(OTf)]3",
            "{[(tmeda)Zn(OH)(OTf)]}3",
            "(((tmeda))Zn((OH))((OTf)))3",
        ] {
            assert_eq!(read(text), trimer, "{text}");
            assert_eq!(weight(text), "1042.97", "{text}");
        }
        assert_eq!(read("(Pd(dppf)Cl2)2").as_deref(), Ok("C68H56Cl4Fe2P4Pd2"));
        assert_eq!(weight("(Pd(dppf)Cl2)2"), "1463.42");
        assert_eq!(read("((cod)IrCl)2").as_deref(), Ok("C16H24Cl2Ir2"));
        assert_eq!(read("((COD)IrCl)2"), read("[Ir(cod)Cl]2"));
        assert_eq!(read("((Tmeda)Zn(OH)(OTf))3"), Err(LabelFormulaError::UnknownGroup("Tmeda".into())));
        assert_eq!(weight("((cod)IrCl)2"), "671.71");
        assert_eq!(read("(((((Pd(((dppf)))Cl2)))))").as_deref(), Ok("C34H28Cl2FeP2Pd"));
    }

    /// Full-width brackets, digits and letters — an input method left in full-width mode —
    /// read as their ASCII equivalents, and the katakana middle dot as the adduct dot.
    #[test]
    fn full_width_forms_read_as_ascii() {
        let wide = "\u{ff08}\u{ff08}tmeda\u{ff09}Zn\u{ff08}OH\u{ff09}\u{ff08}OTf\u{ff09}\u{ff09}\u{ff13}";
        assert_eq!(read(wide), read("((tmeda)Zn(OH)(OTf))3"));
        assert_eq!(weight(wide), "1042.97");
        assert_eq!(read("\u{ff3b}Ir\u{ff08}cod\u{ff09}Cl\u{ff3d}\u{ff12}"), read("[Ir(cod)Cl]2"));
        assert_eq!(read("\u{ff2b}\u{ff12}\u{ff23}\u{ff2f}\u{ff13}").as_deref(), Ok("CK2O3"), "Ｋ２ＣＯ３");
        assert_eq!(read("CuSO4\u{30fb}\u{ff15}H2O").as_deref(), Ok("CuH10O9S"));
        assert_eq!(read("\u{ff08}\u{ff08}tmeda\u{ff09}Zn"), Err(LabelFormulaError::UnbalancedBrackets));
    }

    /// A bracket left open, closed twice or closed by another kind: no formula, no weight,
    /// and the error says it is the brackets.
    #[test]
    fn unbalanced_brackets_are_refused_as_such() {
        for text in [
            "((tmeda)Zn(OH)(OTf)3",
            "(tmeda)Zn(OH)(OTf))3",
            "Pd(OAc",
            "Pd)OAc(",
            "[(tmeda)Zn(OH)(OTf))3",
            "(Pd[dppf)Cl2]",
            "Pd(OAc)2)",
            "(",
        ] {
            assert_eq!(read(text), Err(LabelFormulaError::UnbalancedBrackets), "{text:?}");
        }
    }

    #[test]
    fn names_the_grammar_uses_are_refused() {
        for name in ["", "2Me", "a(b)", "a[b", "a b", "a\u{b7}b", "a_b", "*"] {
            assert!(!valid_name(name), "{name:?}");
            assert!(FormulaGroup::written(name, &[], "C").is_none(), "{name:?}");
        }
        assert!(valid_name("Cp*") && valid_name("t-Bu") && valid_name("CO2Me"));
        assert!(FormulaGroup::written("x", &[], "Qq").is_none());
    }

    /// A long label is read in time linear in its length, on a small stack.
    #[test]
    fn long_labels_are_read_without_recursion() {
        let start = std::time::Instant::now();
        std::thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(|| {
                let table = table();
                assert!(parse_with_groups(&"CoCOCNOMe".repeat(2_000), &table).is_ok());
                assert!(parse_with_groups(&format!("{}q", "CO".repeat(10_000)), &table).is_err());
                assert!(parse_with_groups(&"(".repeat(20_000), &table).is_err());
                let deep = format!("{}C{}", "(".repeat(20_000), ")".repeat(20_000));
                assert_eq!(parse_with_groups(&deep, &table), Err(LabelFormulaError::NotAFormula));
            })
            .unwrap()
            .join()
            .unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(30), "{:?}", start.elapsed());
    }
}
