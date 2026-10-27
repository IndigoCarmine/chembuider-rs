//! What a [`ReactionTable`] on the page shows: which structures are its columns, and the
//! stoichiometry computed for them.
//!
//! The columns are found from the drawing, the way a chemist reads a scheme: the structures
//! before the table's arrow (and the reagents drawn over or under it) are the reactants, the
//! ones after it the products. The table keeps only what was entered for each column
//! ([`TableEntry`]), tied to its structure by one of its atoms, and the numbers come from
//! [`reaction::stoich`](crate::reaction::stoich) — the same computation and the same cells
//! ([`reaction::table`](crate::reaction::table)) as the reaction-scheme figure.

use std::collections::{HashMap, HashSet};

use super::page::{ArrowKind, PageArrow, PageObject, ReactionTable, TableEntry};
use super::Molecule;
use crate::reaction::edit::{field_at, parse_value, set_field, CellRef, Field, ParseError};
use crate::reaction::model::{Analysis, Mass, Product, Reactant};
use crate::reaction::table::{product_cells, reactant_cells, BlockKind, TableBlock, PRODUCT_ROWS, REACTANT_ROWS};

/// How far (molecule units) a structure may sit off the arrow's line and still be one of its
/// reactants or products: the line has to pass through the structure, give or take a bond.
const SIDE_REACH: f32 = 1.5;
/// How far a reagent drawn over or under the arrow may be from its shaft.
const REAGENT_REACH: f32 = 3.0;
/// Gap between a reaction and a table inserted under it.
const INSERT_GAP: f32 = 1.5;
/// A caption's line spacing and the depth below its last baseline, in molecule units (generous
/// for the editor's label sizes).
const CAPTION_LINE: f32 = 0.5;
const CAPTION_DESCENT: f32 = 0.2;

/// The structures a table's columns stand for, each as its atom ids (sorted), in column order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TableColumns {
    pub reactants: Vec<Vec<u32>>,
    pub products: Vec<Vec<u32>>,
}

/// The connected structures of `mol`, each as its sorted atom ids, in the order of their
/// first atom.
pub fn structures(mol: &Molecule) -> Vec<Vec<u32>> {
    let mut adjacent: HashMap<u32, Vec<u32>> = HashMap::new();
    for b in &mol.bonds {
        adjacent.entry(b.begin).or_default().push(b.end);
        adjacent.entry(b.end).or_default().push(b.begin);
    }
    let mut seen: HashSet<u32> = HashSet::new();
    let mut out = Vec::new();
    for atom in &mol.atoms {
        if !seen.insert(atom.id) {
            continue;
        }
        let mut members = vec![atom.id];
        let mut queue = vec![atom.id];
        while let Some(a) = queue.pop() {
            for &n in adjacent.get(&a).into_iter().flatten() {
                if seen.insert(n) {
                    members.push(n);
                    queue.push(n);
                }
            }
        }
        members.sort_unstable();
        out.push(members);
    }
    out
}

/// The atom a column is tied to: the structure's first atom that is not a group's hydrogen.
fn anchor_of(mol: &Molecule, atoms: &[u32]) -> Option<u32> {
    atoms
        .iter()
        .copied()
        .find(|&id| mol.atom_by_id(id).is_some_and(|a| !a.group_h))
        .or_else(|| atoms.first().copied())
}

/// A structure measured in an arrow's frame: `t` along the arrow from its tail, `s` across it.
struct Placement {
    t_center: f32,
    s_min: f32,
    s_max: f32,
}

impl Placement {
    /// How far the structure is from the arrow's line (0 when the line crosses it).
    fn off_line(&self) -> f32 {
        if self.s_min <= 0.0 && self.s_max >= 0.0 {
            0.0
        } else {
            self.s_min.abs().min(self.s_max.abs())
        }
    }
}

fn frame(arrow: &PageArrow) -> Option<([f32; 2], [f32; 2], f32)> {
    let d = [arrow.head[0] - arrow.tail[0], arrow.head[1] - arrow.tail[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    (len > 1e-6).then(|| ([d[0] / len, d[1] / len], [-d[1] / len, d[0] / len], len))
}

fn along(arrow: &PageArrow, u: [f32; 2], n: [f32; 2], p: [f32; 2]) -> (f32, f32) {
    let r = [p[0] - arrow.tail[0], p[1] - arrow.tail[1]];
    (r[0] * u[0] + r[1] * u[1], r[0] * n[0] + r[1] * n[1])
}

impl Molecule {
    /// The reaction arrows on the page.
    fn arrows(&self) -> impl Iterator<Item = &PageArrow> {
        self.objects.iter().filter_map(PageObject::as_arrow)
    }

    /// The arrow whose reaction `table` describes: the one it names, else the arrow nearest to
    /// the table's top edge (a table sits under its reaction). `None` when there is no arrow.
    pub fn reaction_table_arrow(&self, table: &ReactionTable) -> Option<&PageArrow> {
        if let Some(a) = table.arrow.and_then(|id| self.object_by_id(id)).and_then(PageObject::as_arrow) {
            return Some(a);
        }
        let at = table.pos;
        let dist = |a: &PageArrow| {
            let mid = [(a.tail[0] + a.head[0]) / 2.0, (a.tail[1] + a.head[1]) / 2.0];
            // Arrows above the table come first; among them the closest.
            let below = if mid[1] <= at[1] + 1.0 { 0.0 } else { 1e6 };
            below + (mid[0] - at[0]).abs() * 0.25 + (mid[1] - at[1]).abs()
        };
        self.arrows().min_by(|a, b| dist(a).total_cmp(&dist(b)))
    }

    /// The structures `arrow` turns into which: before its tail (and drawn over or under its
    /// shaft) the reactants, after its head the products — each only as far as the next arrow
    /// along the same line, so a two-step scheme's intermediate is the first step's product and
    /// the second step's reactant, and nothing further. With no arrow, every structure is a
    /// reactant.
    pub fn reaction_sides(&self, arrow: Option<&PageArrow>) -> TableColumns {
        let all = structures(self);
        let pos = |id: u32| self.atom_by_id(id).map(|a| a.pos).unwrap_or_default();
        let Some((arrow, (u, n, len))) = arrow.and_then(|a| Some((a, frame(a)?))) else {
            let mut reactants = all;
            let left = |s: &Vec<u32>| s.iter().map(|&id| pos(id)[0]).fold(f32::INFINITY, f32::min);
            reactants.sort_by(|a, b| left(a).total_cmp(&left(b)));
            return TableColumns { reactants, products: Vec::new() };
        };
        // Other arrows' midpoints in this arrow's frame: what blocks the view along the line.
        let others: Vec<(f32, f32)> = self
            .arrows()
            .filter(|a| a.id != arrow.id)
            .map(|a| along(arrow, u, n, [(a.tail[0] + a.head[0]) / 2.0, (a.tail[1] + a.head[1]) / 2.0]))
            .collect();
        let blocked = |from: f32, to: f32| {
            let (lo, hi) = (from.min(to), from.max(to));
            others.iter().any(|&(t, s)| t > lo && t < hi && s.abs() <= REAGENT_REACH)
        };
        let mut before: Vec<(f32, Vec<u32>)> = Vec::new();
        let mut over: Vec<(f32, Vec<u32>)> = Vec::new();
        let mut after: Vec<(f32, Vec<u32>)> = Vec::new();
        for s in all {
            let (mut t_sum, mut s_min, mut s_max) = (0.0, f32::INFINITY, f32::NEG_INFINITY);
            for &id in &s {
                let (t, across) = along(arrow, u, n, pos(id));
                t_sum += t;
                s_min = s_min.min(across);
                s_max = s_max.max(across);
            }
            let p = Placement { t_center: t_sum / s.len().max(1) as f32, s_min, s_max };
            if p.t_center < 0.0 {
                if p.off_line() <= SIDE_REACH && !blocked(p.t_center, 0.0) {
                    before.push((p.t_center, s));
                }
            } else if p.t_center > len {
                if p.off_line() <= SIDE_REACH && !blocked(len, p.t_center) {
                    after.push((p.t_center, s));
                }
            } else if p.off_line() <= REAGENT_REACH {
                over.push((p.t_center, s));
            }
        }
        let sorted = |mut v: Vec<(f32, Vec<u32>)>| {
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            v.into_iter().map(|(_, s)| s).collect::<Vec<_>>()
        };
        let mut reactants = sorted(before);
        reactants.extend(sorted(over));
        TableColumns { reactants, products: sorted(after) }
    }

    /// The structures `table`'s columns stand for.
    pub fn reaction_table_columns(&self, table: &ReactionTable) -> TableColumns {
        self.reaction_sides(self.reaction_table_arrow(table))
    }

    /// A copy of the atoms `atoms` and the bonds among them: one structure on its own, to take
    /// its formula from.
    pub fn substructure(&self, atoms: &[u32]) -> Molecule {
        let keep: HashSet<u32> = atoms.iter().copied().collect();
        let mut m = Molecule::default();
        m.atoms = self.atoms.iter().filter(|a| keep.contains(&a.id)).cloned().collect();
        m.bonds = self.bonds.iter().filter(|b| keep.contains(&b.begin) && keep.contains(&b.end)).cloned().collect();
        m.next_id = m.atoms.iter().map(|a| a.id).chain(m.bonds.iter().map(|b| b.id)).max().map_or(1, |x| x + 1);
        m
    }
}

/// `entries` matched to `structures`: each structure takes the entry tied to one of its atoms,
/// else the next entry tied to none (read from a file), else a blank one; every entry comes
/// back tied to its structure's anchor atom. Entries whose structure is gone are dropped.
pub fn match_entries(mol: &Molecule, entries: &[TableEntry], structures: &[Vec<u32>]) -> Vec<TableEntry> {
    let mut used = vec![false; entries.len()];
    let mut out: Vec<Option<TableEntry>> = vec![None; structures.len()];
    for (k, s) in structures.iter().enumerate() {
        if let Some(i) = entries
            .iter()
            .enumerate()
            .position(|(i, e)| !used[i] && e.atom.is_some_and(|a| s.binary_search(&a).is_ok()))
        {
            used[i] = true;
            out[k] = Some(entries[i].clone());
        }
    }
    let mut free = entries.iter().enumerate().filter(|(_, e)| e.atom.is_none()).map(|(i, _)| i);
    for (k, s) in structures.iter().enumerate() {
        let mut entry = match out[k].take() {
            Some(e) => e,
            None => free.find(|&i| !used[i]).map(|i| entries[i].clone()).unwrap_or_default(),
        };
        entry.atom = anchor_of(mol, s);
        out[k] = Some(entry);
    }
    out.into_iter().map(|e| e.expect("filled above")).collect()
}

/// A table resolved against the drawing: its columns' structures and entries, and the
/// stoichiometry they make.
#[derive(Debug, Clone)]
pub struct TableState {
    pub columns: TableColumns,
    pub reactants: Vec<TableEntry>,
    pub products: Vec<TableEntry>,
    pub analysis: Analysis,
}

impl TableState {
    /// Resolve `table` against `mol`.
    pub fn of(mol: &Molecule, table: &ReactionTable) -> Self {
        let columns = mol.reaction_table_columns(table);
        let reactants = match_entries(mol, &table.reactants, &columns.reactants);
        let products = match_entries(mol, &table.products, &columns.products);
        let mut analysis = Analysis::new();
        for (atoms, e) in columns.reactants.iter().zip(&reactants) {
            let mut r = Reactant::new(mol.substructure(atoms));
            r.limiting = e.limiting;
            r.equivalents = e.equivalents;
            r.sample_mass = e.sample_mass.map(Mass::grams);
            r.percent_weight = e.percent_weight;
            r.molarity = e.molarity;
            r.density = e.density;
            r.volume = e.volume;
            analysis.add_reactant(r);
        }
        for (atoms, e) in columns.products.iter().zip(&products) {
            let mut p = Product::new(mol.substructure(atoms));
            p.equivalents = e.equivalents;
            p.percent_completion = e.percent_completion;
            p.measured_mass = e.measured_mass.map(Mass::grams);
            p.purity = e.purity;
            analysis.add_product(p);
        }
        Self { columns, reactants, products, analysis }
    }

    /// The two blocks of cells, *Reactants* then *Products* — both always, as ChemDraw draws
    /// them, even with no column under one.
    pub fn blocks(&self) -> [TableBlock; 2] {
        let result = self.analysis.compute();
        [
            TableBlock {
                kind: BlockKind::Reactants,
                title: "Reactants".into(),
                row_labels: REACTANT_ROWS.iter().map(|s| s.to_string()).collect(),
                columns: result.reactants.iter().map(reactant_cells).collect(),
            },
            TableBlock {
                kind: BlockKind::Products,
                title: "Products".into(),
                row_labels: PRODUCT_ROWS.iter().map(|s| s.to_string()).collect(),
                columns: result.products.iter().map(product_cells).collect(),
            },
        ]
    }

    /// The entries as they stand after the analysis was edited.
    fn entries_from_analysis(&mut self) {
        for (e, r) in self.reactants.iter_mut().zip(&self.analysis.reactants) {
            e.limiting = r.limiting;
            e.equivalents = r.equivalents;
            e.sample_mass = r.sample_mass.map(Mass::as_grams);
            e.percent_weight = r.percent_weight;
            e.molarity = r.molarity;
            e.density = r.density;
            e.volume = r.volume;
        }
        for (e, p) in self.products.iter_mut().zip(&self.analysis.products) {
            e.equivalents = p.equivalents;
            e.percent_completion = p.percent_completion;
            e.measured_mass = p.measured_mass.map(Mass::as_grams);
            e.purity = p.purity;
        }
    }

    /// Each column's formula as plain text (`C13H12OS`), reactants then products.
    pub fn formulas(&self) -> (Vec<String>, Vec<String>) {
        (
            self.analysis.reactants.iter().map(|r| r.formula().plain()).collect(),
            self.analysis.products.iter().map(|p| crate::reaction::MolecularFormula::of(&p.molecule).plain()).collect(),
        )
    }
}

/// What a double-click on a table cell does.
#[derive(Debug, Clone, PartialEq)]
pub enum CellAction {
    /// The cell shows a computed value: nothing to edit.
    ReadOnly,
    /// `Limiting?`: toggled in place.
    Toggle,
    /// A typed value; the edit box starts with this text (the entered value, or empty).
    Type(String),
}

/// What editing `cell` of `table` means; see [`CellAction`].
pub fn cell_action(mol: &Molecule, table: &ReactionTable, cell: CellRef) -> CellAction {
    match field_at(cell.block, cell.row) {
        None => CellAction::ReadOnly,
        Some(Field::Limiting) => CellAction::Toggle,
        Some(_) => {
            let state = TableState::of(mol, table);
            let exists = match cell.block {
                BlockKind::Reactants => cell.column < state.reactants.len(),
                BlockKind::Products => cell.column < state.products.len(),
            };
            if exists {
                CellAction::Type(crate::reaction::edit::field_text(&state.analysis, cell))
            } else {
                CellAction::ReadOnly
            }
        }
    }
}

/// `table` with `cell` set to the typed `text` (empty clears the entry), or toggled when it
/// is `Limiting?`. The entries come back tied to the structures they stand for now. `Ok(None)`
/// when the cell is not an entry.
pub fn edit_cell(mol: &Molecule, table: &ReactionTable, cell: CellRef, text: &str) -> Result<Option<ReactionTable>, ParseError> {
    let Some(field) = field_at(cell.block, cell.row) else { return Ok(None) };
    let mut state = TableState::of(mol, table);
    match field {
        Field::Limiting => {
            if cell.block != BlockKind::Reactants || cell.column >= state.reactants.len() {
                return Ok(None);
            }
            let now = state.analysis.limiting();
            state.analysis.set_limiting(if now == Some(cell.column) { None } else { Some(cell.column) });
        }
        _ => {
            let value = parse_value(field.unit(), text)?;
            if !set_field(&mut state.analysis, cell, value) {
                return Ok(None);
            }
        }
    }
    state.entries_from_analysis();
    Ok(Some(ReactionTable { reactants: state.reactants, products: state.products, ..table.clone() }))
}

/// A new table for the reaction of `arrow` (or of the first reaction arrow, or of every
/// structure when there is no arrow), placed under it: its left edge at the reaction's, its top
/// 1.5 units (a bond) below the lowest structure. The first reactant is the limiting reagent, as in
/// a new ChemDraw table. `None` when the drawing has no structure to tabulate.
pub fn new_table(mol: &Molecule, arrow: Option<u32>) -> Option<ReactionTable> {
    let arrow = arrow
        .and_then(|id| mol.object_by_id(id))
        .and_then(PageObject::as_arrow)
        .or_else(|| mol.arrows().find(|a| a.kind == ArrowKind::Reaction))
        .or_else(|| mol.arrows().next());
    let columns = mol.reaction_sides(arrow);
    if columns.reactants.is_empty() && columns.products.is_empty() {
        return None;
    }
    let mut points: Vec<[f32; 2]> = columns
        .reactants
        .iter()
        .chain(&columns.products)
        .flatten()
        .filter_map(|&id| mol.atom_by_id(id).map(|a| a.pos))
        .collect();
    if let Some(a) = arrow {
        points.extend([a.tail, a.head]);
    }
    let left = points.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let right = points.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let top = points.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let mut bottom = points.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    // Captions just under the structures (compound numbers, conditions) stay above the table.
    for t in mol.objects.iter().filter_map(PageObject::as_text) {
        let [x, y] = t.pos;
        if x >= left - 1.0 && x <= right + 1.0 && y >= top && y <= bottom + 2.0 * INSERT_GAP {
            let lines = t.text.split('\n').count() as f32;
            bottom = bottom.max(y + (lines - 1.0) * CAPTION_LINE + CAPTION_DESCENT);
        }
    }
    let mut reactants = match_entries(mol, &[], &columns.reactants);
    if let Some(first) = reactants.first_mut() {
        first.limiting = true;
    }
    let products = match_entries(mol, &[], &columns.products);
    Some(ReactionTable { id: 0, pos: [left, bottom + INSERT_GAP], arrow: arrow.map(|a| a.id), reactants, products, source: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;

    /// A chain of `n` carbons starting at `at`, going right.
    fn chain(m: &mut Molecule, at: [f32; 2], n: usize) -> Vec<u32> {
        let ids: Vec<u32> = (0..n).map(|i| m.add_atom("C".into(), [at[0] + 1.3 * i as f32, at[1]], 0)).collect();
        for w in ids.windows(2) {
            m.add_bond(w[0], w[1], BondOrder::Single);
        }
        ids
    }

    /// Ethane + propane → butane, with methane over the arrow.
    fn scheme() -> (Molecule, u32) {
        let mut m = Molecule::default();
        chain(&mut m, [0.0, 0.0], 2);
        chain(&mut m, [3.0, 0.0], 3);
        chain(&mut m, [9.0, -1.0], 1);
        let arrow = m.add_arrow([7.0, 0.0], [11.0, 0.0], ArrowKind::Reaction);
        chain(&mut m, [13.0, 0.0], 4);
        (m, arrow)
    }

    #[test]
    fn sides_follow_the_arrow() {
        let (m, arrow) = scheme();
        let cols = m.reaction_sides(m.object_by_id(arrow).and_then(PageObject::as_arrow));
        let sizes = |v: &Vec<Vec<u32>>| v.iter().map(Vec::len).collect::<Vec<_>>();
        assert_eq!(sizes(&cols.reactants), [2, 3, 1], "left to right, then the reagent over the arrow");
        assert_eq!(sizes(&cols.products), [4]);
    }

    #[test]
    fn a_second_arrow_splits_the_scheme() {
        let mut m = Molecule::default();
        chain(&mut m, [0.0, 0.0], 2);
        let a1 = m.add_arrow([3.0, 0.0], [6.0, 0.0], ArrowKind::Reaction);
        chain(&mut m, [7.0, 0.0], 3);
        let a2 = m.add_arrow([11.0, 0.0], [14.0, 0.0], ArrowKind::Reaction);
        chain(&mut m, [15.0, 0.0], 4);
        let sizes = |id: u32| {
            let c = m.reaction_sides(m.object_by_id(id).and_then(PageObject::as_arrow));
            (c.reactants.iter().map(Vec::len).collect::<Vec<_>>(), c.products.iter().map(Vec::len).collect::<Vec<_>>())
        };
        assert_eq!(sizes(a1), (vec![2], vec![3]));
        assert_eq!(sizes(a2), (vec![3], vec![4]));
    }

    #[test]
    fn a_new_table_sits_under_its_reaction_and_computes() {
        let (mut m, arrow) = scheme();
        let mut table = new_table(&m, None).unwrap();
        assert_eq!(table.arrow, Some(arrow));
        assert!(table.pos[1] > 0.0 && table.pos[0] <= 0.0);
        assert!(table.reactants[0].limiting);
        // 1 g of ethane, 1.5 eq of propane: 33.26 mmol and 49.88 mmol.
        table = edit_cell(&m, &table, CellRef::new(BlockKind::Reactants, 0, 4), "1 g").unwrap().unwrap();
        table = edit_cell(&m, &table, CellRef::new(BlockKind::Reactants, 1, 3), "1.5").unwrap().unwrap();
        let state = TableState::of(&m, &table);
        let [r, p] = state.blocks();
        let flat = |c: &crate::reaction::Cell| c.plain();
        assert_eq!(flat(&r.columns[0][0]), "C2H6");
        assert_eq!(flat(&r.columns[0][9]), "33.26mmol");
        assert_eq!(flat(&r.columns[1][9]), "49.88mmol");
        assert_eq!(flat(&p.columns[0][5]), "33.26mmol", "expected moles of butane");
        assert!(edit_cell(&m, &table, CellRef::new(BlockKind::Reactants, 1, 3), "abc").is_err());
        assert_eq!(edit_cell(&m, &table, CellRef::new(BlockKind::Reactants, 1, 9), "1").unwrap(), None, "computed");
        // The entries stay with their structures when a structure is added in front.
        let id = m.push_object(PageObject::Table(table.clone()));
        chain(&mut m, [-4.0, 0.0], 5);
        let PageObject::Table(t) = m.object_by_id(id).unwrap().clone() else { unreachable!() };
        let state = TableState::of(&m, &t);
        assert_eq!(state.reactants.len(), 4);
        assert_eq!(state.reactants[0], TableEntry { atom: state.reactants[0].atom, ..TableEntry::default() });
        assert_eq!(state.reactants[2].equivalents, Some(1.5), "propane is the third column now");
        // Toggling the limiting reagent moves it.
        let t = edit_cell(&m, &t, CellRef::new(BlockKind::Reactants, 0, 2), "").unwrap().unwrap();
        assert!(t.reactants[0].limiting && !t.reactants[1].limiting);
    }

    #[test]
    fn a_new_table_goes_under_the_captions_too() {
        let (mut m, _) = scheme();
        let without = new_table(&m, None).unwrap().pos[1];
        m.add_text([0.5, 1.0], "(1)");
        let with = new_table(&m, None).unwrap().pos[1];
        assert!(with > without && with >= 1.0 + INSERT_GAP, "{without} → {with}");
        // A text far away does not push it down.
        m.add_text([0.0, 30.0], "far");
        assert_eq!(new_table(&m, None).unwrap().pos[1], with);
    }

    #[test]
    fn no_structures_no_table() {
        assert!(new_table(&Molecule::default(), None).is_none());
    }
}
