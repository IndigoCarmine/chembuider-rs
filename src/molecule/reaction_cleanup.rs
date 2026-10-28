//! Reaction cleanup: tidy a reaction scheme drawn on the page around its arrows.
//!
//! The arrows are the reference and stay where they are. Every structure is redrawn by the 2-D
//! cleanup ([`cleanup_2d`]) on its own, and then everything on a reaction line — structures,
//! `+` signs and other text — is centred on the height of the arrows and spaced evenly from left
//! to right, keeping its left-to-right order.
//!
//! What rides on an arrow (conditions written over or under the shaft, a reagent drawn there)
//! belongs to the arrow and stays with it; a caption just under a structure (a compound number)
//! goes where its structure goes. Reaction tables and brackets are left alone (brackets stand
//! where their atoms are).
//!
//! The placement itself is the pure [`arrange_reaction`]; [`Molecule::cleanup_reaction_scheme`]
//! measures the drawing, cleans the structures and applies it. Text has no size without fonts,
//! so the caller measures the drawing ([`Extents`]).

use std::collections::{HashMap, HashSet};

use super::cleanup::cleanup_2d;
use super::page::{PageArrow, PageObject};
use super::reaction_table::structures;
use super::Molecule;

/// Arrows whose shafts are at most this far apart in height (molecule units) are on one line:
/// drawn at the same height, give or take rounding. Arrows any further apart are separate
/// lines, since an item between them could not be centred on both.
const ROW_TOLERANCE: f32 = 0.05;
/// The smallest gap left between neighbours outside the arrows.
const MIN_GAP: f32 = 0.75;
/// The gap used when a line has nothing to measure it from (an arrow alone with one item).
const DEFAULT_GAP: f32 = 1.0;
/// How far from its shaft a text or structure may be and still ride on the arrow (as for the
/// reagents of a reaction table).
const REAGENT_REACH: f32 = 3.0;
/// How far under a structure a text may start and still be its caption.
const CAPTION_REACH: f32 = 1.5;

/// An axis-aligned box in molecule units (y grows downward).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Bounds {
    pub fn new(min: [f32; 2], max: [f32; 2]) -> Self {
        Self { min, max }
    }

    /// The smallest box holding every point; `None` for no points.
    pub fn around(points: impl IntoIterator<Item = [f32; 2]>) -> Option<Self> {
        points.into_iter().fold(None, |acc: Option<Bounds>, p| {
            let b = Bounds::new(p, p);
            Some(acc.map_or(b, |a| a.union(b)))
        })
    }

    pub fn union(self, other: Bounds) -> Self {
        Self::new(
            [self.min[0].min(other.min[0]), self.min[1].min(other.min[1])],
            [self.max[0].max(other.max[0]), self.max[1].max(other.max[1])],
        )
    }

    pub fn width(&self) -> f32 {
        self.max[0] - self.min[0]
    }

    pub fn center(&self) -> [f32; 2] {
        [(self.min[0] + self.max[0]) * 0.5, (self.min[1] + self.max[1]) * 0.5]
    }

    /// The box moved by `d`.
    pub fn translate(self, d: [f32; 2]) -> Self {
        Self::new([self.min[0] + d[0], self.min[1] + d[1]], [self.max[0] + d[0], self.max[1] + d[1]])
    }
}

/// One thing on a reaction line, as [`arrange_reaction`] sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineItem {
    /// The room it takes on the line, with whatever moves with it (an arrow's conditions, a
    /// structure's caption).
    pub bounds: Bounds,
    /// The height that goes on the reaction line: an arrow's shaft, the middle of a structure
    /// or a text.
    pub center_y: f32,
    /// Arrows stay where they are; everything else is placed around them.
    pub arrow: bool,
}

/// Where each item goes, as a move `[dx, dy]` per item (in order): arrows stay, everything else
/// is centred on its reaction line and spaced evenly along it, in its present left-to-right
/// order.
///
/// * Arrows at the same height ([`ROW_TOLERANCE`]) make a line, and its height (their shafts')
///   is where its items are centred; arrows at different heights are different lines. Every
///   other item joins the line nearest to its height. With no arrow nothing moves.
/// * Between two arrows of a line, the items share the room the arrows leave in equal gaps
///   (both ends are fixed, so the gap is whatever that room allows). When the room is too
///   narrow for them even with no gaps, they are left exactly as they are: spreading them
///   would push them over the arrows or past them, and lining them up on the shaft would
///   pile them onto the arrows and each other, so the room has to be widened by hand.
/// * Before the first arrow and after the last, items are laid out from the arrow outward with
///   the line's gap — the mean of the gaps between neighbours on the line now, at least
///   [`MIN_GAP`].
pub fn arrange_reaction(items: &[LineItem]) -> Vec<[f32; 2]> {
    let mut moves = vec![[0.0; 2]; items.len()];
    let mut arrows: Vec<usize> = (0..items.len()).filter(|&i| items[i].arrow).collect();
    arrows.sort_by(|&a, &b| items[a].center_y.total_cmp(&items[b].center_y));
    let mut rows: Vec<Vec<usize>> = Vec::new();
    for i in arrows {
        match rows.last_mut() {
            Some(row) if items[i].center_y - items[row[0]].center_y <= ROW_TOLERANCE => row.push(i),
            _ => rows.push(vec![i]),
        }
    }
    if rows.is_empty() {
        return moves;
    }
    let row_y: Vec<f32> =
        rows.iter().map(|r| r.iter().map(|&i| items[i].center_y).sum::<f32>() / r.len() as f32).collect();
    for i in (0..items.len()).filter(|&i| !items[i].arrow) {
        let off = |r: usize| (items[i].center_y - row_y[r]).abs();
        let nearest = (0..rows.len()).min_by(|&a, &b| off(a).total_cmp(&off(b))).unwrap_or(0);
        rows[nearest].push(i);
    }
    for (row, y) in rows.into_iter().zip(row_y) {
        arrange_row(items, row, y, &mut moves);
    }
    moves
}

/// Lay out one reaction line at height `y`: `row` holds at least one arrow.
fn arrange_row(items: &[LineItem], mut row: Vec<usize>, y: f32, moves: &mut [[f32; 2]]) {
    row.sort_by(|&a, &b| items[a].bounds.center()[0].total_cmp(&items[b].bounds.center()[0]));
    let at = |k: usize| &items[row[k]];
    let gaps: Vec<f32> = (1..row.len()).map(|k| at(k).bounds.min[0] - at(k - 1).bounds.max[0]).collect();
    let gap = match gaps.len() {
        0 => DEFAULT_GAP,
        n => (gaps.iter().sum::<f32>() / n as f32).max(MIN_GAP),
    };
    let arrows: Vec<usize> = (0..row.len()).filter(|&k| at(k).arrow).collect();
    let (Some(&first), Some(&last)) = (arrows.first(), arrows.last()) else { return };
    let mut place = |k: usize, left: f32| {
        let item = at(k);
        moves[row[k]] = [left - item.bounds.min[0], y - item.center_y];
    };

    // Before the first arrow: right to left from it.
    let mut right = at(first).bounds.min[0];
    for k in (0..first).rev() {
        right -= gap + at(k).bounds.width();
        place(k, right);
    }
    // After the last arrow: left to right from it.
    let mut left = at(last).bounds.max[0];
    for k in last + 1..row.len() {
        place(k, left + gap);
        left += gap + at(k).bounds.width();
    }
    // Between two arrows: the room they leave, shared out in equal gaps.
    for pair in arrows.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if b == a + 1 {
            continue;
        }
        let room = at(b).bounds.min[0] - at(a).bounds.max[0];
        let widths: f32 = (a + 1..b).map(|k| at(k).bounds.width()).sum();
        let gap = (room - widths) / (b - a) as f32;
        if gap < 0.0 {
            continue; // Too crowded: left as it is.
        }
        let mut left = at(a).bounds.max[0];
        for k in a + 1..b {
            place(k, left + gap);
            left += gap + at(k).bounds.width();
        }
    }
}

/// Boxes measured on the drawing, in molecule units: the drawn label of each atom that shows
/// one, and each page object (an arrow with its head, the lines of a text). What is missing
/// counts as a point: an atom at its position, an arrow along its shaft, a text at its anchor.
#[derive(Clone, Debug, Default)]
pub struct Extents {
    pub atoms: HashMap<u32, Bounds>,
    pub objects: HashMap<u32, Bounds>,
}

/// An arrow's direction `u`, its left-hand normal `n` and its length; `None` for a point.
fn arrow_frame(a: &PageArrow) -> Option<([f32; 2], [f32; 2], f32)> {
    let d = [a.head[0] - a.tail[0], a.head[1] - a.tail[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    (len > 1e-6).then(|| ([d[0] / len, d[1] / len], [-d[1] / len, d[0] / len], len))
}

/// `b` in `arrow`'s frame: how far its middle is along the shaft from the tail, and the range
/// it covers across the shaft's line (toward the normal `n`). `None` for an arrow of no length.
fn across(arrow: &PageArrow, b: &Bounds) -> Option<(f32, f32, f32)> {
    let (u, n, _) = arrow_frame(arrow)?;
    let project = |p: [f32; 2], axis: [f32; 2]| (p[0] - arrow.tail[0]) * axis[0] + (p[1] - arrow.tail[1]) * axis[1];
    let corners = [b.min, [b.max[0], b.min[1]], b.max, [b.min[0], b.max[1]]];
    let (lo, hi) = corners
        .iter()
        .map(|&p| project(p, n))
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| (lo.min(v), hi.max(v)));
    Some((project(b.center(), u), lo, hi))
}

/// Whether `b` rides on `arrow`: its middle is alongside the shaft, and it sits clear of the
/// shaft's line on one side, within [`REAGENT_REACH`] of it. Returns where its near side is
/// across the line (signed: positive on the normal's side), so that distance can be kept.
fn riding_on(arrow: &PageArrow, b: &Bounds) -> Option<f32> {
    let (_, _, len) = arrow_frame(arrow)?;
    let (t, lo, hi) = across(arrow, b)?;
    let near = if lo >= 0.0 {
        lo
    } else if hi <= 0.0 {
        hi
    } else {
        return None;
    };
    ((0.0..=len).contains(&t) && near.abs() <= REAGENT_REACH).then_some(near)
}

/// Whether `caption` starts just under `structure` and is centred within its width.
fn caption_of(structure: &Bounds, caption: &Bounds) -> bool {
    let x = caption.center()[0];
    let drop = caption.min[1] - structure.max[1];
    x >= structure.min[0] && x <= structure.max[0] && (-0.1..=CAPTION_REACH).contains(&drop)
}

/// What a structure or text is to the cleanup.
#[derive(Clone, Copy, PartialEq)]
enum Role {
    /// On a reaction line: centred and spaced.
    Free,
    /// Rides on arrow `objects[i]`: stays with it.
    OnArrow(usize),
    /// The caption of structure `i`: goes with it.
    Caption(usize),
}

impl Molecule {
    /// Whether the page has an arrow (of some length) for
    /// [`cleanup_reaction_scheme`](Self::cleanup_reaction_scheme) to line things up on.
    pub fn has_reaction_arrow(&self) -> bool {
        self.objects.iter().any(|o| o.as_arrow().and_then(arrow_frame).is_some())
    }

    /// Clean up the reaction scheme on the page: keep every arrow where it is, redraw each
    /// structure with the 2-D cleanup, and line up and space everything else along the arrows
    /// (see the [module docs](self) and [`arrange_reaction`]). `measure` gives the drawing's
    /// boxes; it is called once before the structures are cleaned and once after.
    ///
    /// Returns `false`, changing nothing, when the page has no arrow to line things up on.
    pub fn cleanup_reaction_scheme(&mut self, mut measure: impl FnMut(&Molecule) -> Extents) -> bool {
        let arrows: Vec<usize> = (0..self.objects.len())
            .filter(|&i| self.objects[i].as_arrow().and_then(arrow_frame).is_some())
            .collect();
        if arrows.is_empty() {
            return false;
        }
        let groups = structures(self);
        let texts: Vec<usize> = (0..self.objects.len()).filter(|&i| self.objects[i].as_text().is_some()).collect();

        // Who rides on which arrow, and which text is whose caption, from the drawing as it is.
        let before = measure(self);
        let boxes_before: Vec<Bounds> = groups.iter().map(|g| self.structure_bounds(g, &before)).collect();
        let text_boxes: Vec<Bounds> = texts.iter().map(|&i| self.object_bounds(i, &before)).collect();
        let on_arrow = |b: &Bounds| {
            arrows.iter().copied().find_map(|i| riding_on(self.objects[i].as_arrow()?, b).map(|near| (i, near)))
        };
        let structure_roles: Vec<(Role, f32)> = boxes_before
            .iter()
            .map(|b| on_arrow(b).map_or((Role::Free, 0.0), |(i, near)| (Role::OnArrow(i), near)))
            .collect();
        let text_roles: Vec<Role> = text_boxes
            .iter()
            .map(|b| {
                if let Some((i, _)) = on_arrow(b) {
                    return Role::OnArrow(i);
                }
                let owner = (0..groups.len())
                    .filter(|&s| structure_roles[s].0 == Role::Free && caption_of(&boxes_before[s], b))
                    .min_by(|&a, &c| {
                        let drop = |s: usize| b.min[1] - boxes_before[s].max[1];
                        drop(a).abs().total_cmp(&drop(c).abs())
                    });
                owner.map_or(Role::Free, Role::Caption)
            })
            .collect();

        // Each structure redrawn on its own, kept centred where it was.
        for group in &groups {
            let mut redrawn = self.substructure(group);
            cleanup_2d(&mut redrawn);
            let old = Bounds::around(group.iter().filter_map(|&id| self.atom_by_id(id)).map(|a| a.pos));
            let new = Bounds::around(redrawn.atoms.iter().map(|a| a.pos));
            let (Some(old), Some(new)) = (old, new) else { continue };
            let shift = [old.center()[0] - new.center()[0], old.center()[1] - new.center()[1]];
            let at: HashMap<u32, [f32; 2]> = redrawn.atoms.iter().map(|a| (a.id, a.pos)).collect();
            for atom in self.atoms.iter_mut() {
                if let Some(p) = at.get(&atom.id) {
                    atom.pos = [p[0] + shift[0], p[1] + shift[1]];
                }
            }
        }

        // A structure riding on an arrow keeps its near side where it was: the same distance
        // from the shaft, however much bigger or smaller the cleanup made it.
        let after = measure(self);
        let mut structure_boxes: Vec<Bounds> = groups.iter().map(|g| self.structure_bounds(g, &after)).collect();
        for (s, group) in groups.iter().enumerate() {
            let (Role::OnArrow(i), near) = structure_roles[s] else { continue };
            let Some(arrow) = self.objects[i].as_arrow() else { continue };
            let (Some((_, n, _)), Some((_, lo, hi))) = (arrow_frame(arrow), across(arrow, &structure_boxes[s])) else {
                continue;
            };
            let now = if near >= 0.0 { lo } else { hi };
            let d = [n[0] * (near - now), n[1] * (near - now)];
            self.translate_atoms(group, d);
            structure_boxes[s] = structure_boxes[s].translate(d);
        }
        // A caption keeps its distance under its structure's (new) bottom.
        let mut text_boxes: Vec<Bounds> = texts.iter().map(|&i| self.object_bounds(i, &after)).collect();
        for t in 0..texts.len() {
            let Role::Caption(s) = text_roles[t] else { continue };
            let d = [0.0, structure_boxes[s].max[1] - boxes_before[s].max[1]];
            self.objects[texts[t]].translate(d);
            text_boxes[t] = text_boxes[t].translate(d);
        }

        // The lines: arrows (with what rides on them), structures (with their captions), texts.
        enum Member {
            Arrow,
            Structure(usize),
            Text(usize),
        }
        let mut members: Vec<Member> = Vec::new();
        let mut items: Vec<LineItem> = Vec::new();
        for &i in &arrows {
            // Only an arrow running across the page makes a reaction line; one running up or
            // down (to the next row) stays where it is with what rides on it, and that is all.
            let PageObject::Arrow(a) = &self.objects[i] else { continue };
            if (a.head[0] - a.tail[0]).abs() < (a.head[1] - a.tail[1]).abs() {
                continue;
            }
            let mut bounds = self.object_bounds(i, &after);
            for (s, b) in structure_boxes.iter().enumerate() {
                if structure_roles[s].0 == Role::OnArrow(i) {
                    bounds = bounds.union(*b);
                }
            }
            for (t, b) in text_boxes.iter().enumerate() {
                if text_roles[t] == Role::OnArrow(i) {
                    bounds = bounds.union(*b);
                }
            }
            members.push(Member::Arrow);
            items.push(LineItem { bounds, center_y: (a.tail[1] + a.head[1]) * 0.5, arrow: true });
        }
        for (s, b) in structure_boxes.iter().enumerate() {
            if structure_roles[s].0 != Role::Free {
                continue;
            }
            let bounds = (0..texts.len())
                .filter(|&t| text_roles[t] == Role::Caption(s))
                .fold(*b, |acc, t| acc.union(text_boxes[t]));
            members.push(Member::Structure(s));
            items.push(LineItem { bounds, center_y: b.center()[1], arrow: false });
        }
        for (t, b) in text_boxes.iter().enumerate() {
            if text_roles[t] != Role::Free {
                continue;
            }
            members.push(Member::Text(t));
            items.push(LineItem { bounds: *b, center_y: b.center()[1], arrow: false });
        }

        let moves = arrange_reaction(&items);
        for (member, d) in members.iter().zip(moves) {
            match *member {
                Member::Arrow => {}
                Member::Structure(s) => {
                    self.translate_atoms(&groups[s], d);
                    for t in (0..texts.len()).filter(|&t| text_roles[t] == Role::Caption(s)) {
                        self.objects[texts[t]].translate(d);
                    }
                }
                Member::Text(t) => self.objects[texts[t]].translate(d),
            }
        }
        true
    }

    /// The box of the structure made of `atoms`: its atoms and their drawn labels.
    fn structure_bounds(&self, atoms: &[u32], extents: &Extents) -> Bounds {
        let points = atoms.iter().filter_map(|&id| self.atom_by_id(id)).map(|a| a.pos);
        let mut b = Bounds::around(points).unwrap_or(Bounds::new([0.0; 2], [0.0; 2]));
        for id in atoms {
            if let Some(label) = extents.atoms.get(id) {
                b = b.union(*label);
            }
        }
        b
    }

    /// The box of page object `objects[i]`, as measured or else its key points.
    fn object_bounds(&self, i: usize, extents: &Extents) -> Bounds {
        let object = &self.objects[i];
        extents
            .objects
            .get(&object.id())
            .copied()
            .or_else(|| Bounds::around(object.key_points()))
            .unwrap_or(Bounds::new([0.0; 2], [0.0; 2]))
    }

    /// Move the atoms `atoms` by `d`.
    fn translate_atoms(&mut self, atoms: &[u32], d: [f32; 2]) {
        let ids: HashSet<u32> = atoms.iter().copied().collect();
        for atom in self.atoms.iter_mut().filter(|a| ids.contains(&a.id)) {
            atom.pos[0] += d[0];
            atom.pos[1] += d[1];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{ArrowKind, BondOrder};

    const EPS: f32 = 1e-3;

    fn item(min_x: f32, max_x: f32, center_y: f32, height: f32, arrow: bool) -> LineItem {
        let h = height * 0.5;
        LineItem { bounds: Bounds::new([min_x, center_y - h], [max_x, center_y + h]), center_y, arrow }
    }

    /// The items moved as `moves` says.
    fn moved(items: &[LineItem], moves: &[[f32; 2]]) -> Vec<LineItem> {
        items
            .iter()
            .zip(moves)
            .map(|(it, d)| LineItem { bounds: it.bounds.translate(*d), center_y: it.center_y + d[1], ..*it })
            .collect()
    }

    /// The gaps between neighbours, left to right.
    fn gaps(items: &[LineItem]) -> Vec<f32> {
        let mut sorted: Vec<&LineItem> = items.iter().collect();
        sorted.sort_by(|a, b| a.bounds.min[0].total_cmp(&b.bounds.min[0]));
        sorted.windows(2).map(|w| w[1].bounds.min[0] - w[0].bounds.max[0]).collect()
    }

    fn assert_all_close(values: &[f32], want: f32, what: &str) {
        for v in values {
            assert!((v - want).abs() < EPS, "{what}: {values:?}, expected all {want}");
        }
    }

    /// One step: the arrow stays, everything is centred on its shaft and spaced by the mean of
    /// the gaps there were, in the same order.
    #[test]
    fn one_step_centres_on_the_arrow_and_spaces_evenly() {
        let items = [
            item(0.0, 2.0, 0.0, 2.0, false),   // reactant
            item(3.0, 3.4, 0.3, 0.6, false),   // +
            item(5.0, 8.0, -0.5, 3.0, false),  // reactant
            item(10.0, 14.0, 0.5, 0.4, true),  // arrow
            item(16.0, 19.0, 2.0, 2.0, false), // product
        ];
        let moves = arrange_reaction(&items);
        assert_eq!(moves[3], [0.0, 0.0], "the arrow stays");
        let after = moved(&items, &moves);
        for it in &after {
            assert!((it.center_y - 0.5).abs() < EPS, "centred on the shaft: {after:?}");
        }
        // Gaps were 1, 1.6, 2, 2.
        assert_all_close(&gaps(&after), 1.65, "even gaps");
        let order: Vec<f32> = after.iter().map(|i| i.bounds.min[0]).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "left-to-right order kept: {order:?}");
    }

    /// Two steps: between the arrows the intermediate shares the room they leave equally on
    /// both sides; outside them the line's gap is used. Both arrows stay.
    #[test]
    fn two_steps_share_the_room_between_the_arrows() {
        let items = [
            item(0.0, 2.0, 1.0, 2.0, false),  // start
            item(3.0, 6.0, 0.0, 0.4, true),   // arrow 1
            item(6.5, 8.5, 1.5, 2.0, false),  // intermediate, too close to arrow 1
            item(12.0, 15.0, 0.0, 0.4, true), // arrow 2
            item(18.0, 20.0, -1.0, 2.0, false),
        ];
        let moves = arrange_reaction(&items);
        assert_eq!((moves[1], moves[3]), ([0.0, 0.0], [0.0, 0.0]), "arrows stay");
        let after = moved(&items, &moves);
        for it in &after {
            assert!(it.center_y.abs() < EPS, "centred on the shafts: {after:?}");
        }
        let g = gaps(&after);
        assert!((g[1] - g[2]).abs() < EPS && (g[1] - 2.0).abs() < EPS, "(room 6 - width 2) / 2 each side: {g:?}");
        // The line's gap: the mean of 1, 0.5, 3.5, 3.
        assert!((g[0] - 2.0).abs() < EPS && (g[3] - 2.0).abs() < EPS, "outer gaps: {g:?}");
    }

    /// Lines of arrows at different heights are laid out apart: each item goes on the nearest.
    #[test]
    fn each_item_goes_on_the_nearest_line() {
        let items = [
            item(0.0, 2.0, 0.5, 2.0, false),
            item(4.0, 7.0, 0.0, 0.4, true),
            item(0.0, 2.0, 9.0, 2.0, false),
            item(4.0, 7.0, 10.0, 0.4, true),
            item(9.0, 11.0, 10.5, 2.0, false),
        ];
        let after = moved(&items, &arrange_reaction(&items));
        assert!(after[0].center_y.abs() < EPS);
        assert!((after[2].center_y - 10.0).abs() < EPS && (after[4].center_y - 10.0).abs() < EPS);
        assert_all_close(&[after[1].bounds.min[0] - after[0].bounds.max[0]], 2.0, "first line's own gap");
        let second: Vec<LineItem> = vec![after[2], after[3], after[4]];
        assert_all_close(&gaps(&second), 2.0, "second line's gaps (2, 2)");
    }

    /// Arrows at different heights, even close ones, are separate lines: an item between them
    /// goes on one arrow's shaft, never halfway between the two.
    #[test]
    fn arrows_at_different_heights_are_separate_lines() {
        let items = [
            item(0.0, 3.0, 0.0, 0.4, true),
            item(4.0, 6.0, 0.6, 2.0, false),
            item(8.0, 11.0, 1.0, 0.4, true),
            item(12.0, 14.0, 0.4, 2.0, false),
        ];
        let after = moved(&items, &arrange_reaction(&items));
        for it in &after {
            let on_a_shaft = [0.0, 1.0].iter().any(|y| (it.center_y - y).abs() < EPS);
            assert!(on_a_shaft, "every item on one arrow's shaft: {after:?}");
        }
        assert!((after[1].center_y - 1.0).abs() < EPS, "the nearer line: {after:?}");
        assert!(after[3].center_y.abs() < EPS, "the nearer line: {after:?}");
    }

    /// A room between two arrows too narrow for what is in it is left as drawn: nothing is put
    /// over an arrow or past it, and the order stays.
    #[test]
    fn a_crowded_room_between_arrows_is_left_alone() {
        let items = [
            item(1.0, 3.0, 0.0, 0.4, true),    // arrow, centre 2
            item(5.5, 6.5, 0.5, 0.6, false),   // centre 6
            item(0.1, 12.1, -2.0, 2.0, false), // centre 6.1, wider than the room
            item(6.0, 8.0, 0.0, 0.4, true),    // arrow, centre 7
            item(10.0, 12.0, 1.0, 2.0, false), // after the last arrow: still laid out
        ];
        let moves = arrange_reaction(&items);
        let after = moved(&items, &moves);
        let x = |it: &LineItem| it.bounds.center()[0];
        assert!(x(&after[1]) < x(&after[2]), "order kept: {after:?}");
        for it in &after[1..3] {
            assert!(x(it) > 2.0 && x(it) < 7.0, "not across a fixed arrow: {after:?}");
        }
        assert_eq!((moves[1], moves[2]), ([0.0; 2], [0.0; 2]), "left as drawn");
        assert!(after[4].center_y.abs() < EPS, "the rest of the line is still tidied");
    }

    #[test]
    fn nothing_moves_without_an_arrow() {
        let items = [item(0.0, 2.0, 0.0, 2.0, false), item(5.0, 6.0, 3.0, 1.0, false)];
        assert_eq!(arrange_reaction(&items), vec![[0.0, 0.0]; 2]);
    }

    // ── On the page ──────────────────────────────────────────────────────────

    /// A badly drawn chain of `n` carbons starting at `at`: bonds too long and the zig-zag
    /// uneven, so the cleanup changes its shape.
    fn sloppy_chain(mol: &mut Molecule, at: [f32; 2], n: usize) -> Vec<u32> {
        let ids: Vec<u32> = (0..n)
            .map(|k| {
                let y = if k % 2 == 0 { 0.0 } else { 0.4 + 0.3 * k as f32 };
                mol.add_atom("C".into(), [at[0] + 2.1 * k as f32, at[1] + y], 0)
            })
            .collect();
        for w in ids.windows(2) {
            mol.add_bond(w[0], w[1], BondOrder::Single);
        }
        ids
    }

    /// Text measured as if each character were 0.4 wide and a line 0.6 high around its anchor
    /// (the baseline's left end); arrows as their shaft, 0.4 high.
    fn fake_extents(mol: &Molecule) -> Extents {
        let mut e = Extents::default();
        for o in &mol.objects {
            let b = match o {
                PageObject::Text(t) => {
                    let w = 0.4 * t.text.chars().count() as f32;
                    Bounds::new([t.pos[0], t.pos[1] - 0.45], [t.pos[0] + w, t.pos[1] + 0.15])
                }
                PageObject::Arrow(a) => {
                    let b = Bounds::around([a.tail, a.head]).unwrap();
                    Bounds::new([b.min[0], b.min[1] - 0.2], [b.max[0], b.max[1] + 0.2])
                }
                _ => continue,
            };
            e.objects.insert(o.id(), b);
        }
        e
    }

    fn atoms_box(mol: &Molecule, ids: &[u32]) -> Bounds {
        Bounds::around(ids.iter().map(|&id| mol.atom_by_id(id).unwrap().pos)).unwrap()
    }

    fn object_box(mol: &Molecule, id: u32) -> Bounds {
        fake_extents(mol).objects[&id]
    }

    /// The structure's atoms relative to its first atom.
    fn shape(mol: &Molecule, ids: &[u32]) -> Vec<[f32; 2]> {
        let o = mol.atom_by_id(ids[0]).unwrap().pos;
        ids.iter().map(|&id| mol.atom_by_id(id).unwrap().pos).map(|p| [p[0] - o[0], p[1] - o[1]]).collect()
    }

    fn assert_same_shape(got: &[[f32; 2]], want: &[[f32; 2]]) {
        for (g, w) in got.iter().zip(want) {
            assert!((g[0] - w[0]).abs() < EPS && (g[1] - w[1]).abs() < EPS, "{got:?} vs {want:?}");
        }
    }

    /// What the plain 2-D cleanup makes of structure `ids` on its own.
    fn cleaned_shape(mol: &Molecule, ids: &[u32]) -> Vec<[f32; 2]> {
        let mut sub = mol.substructure(ids);
        cleanup_2d(&mut sub);
        let cleaned = shape(&sub, ids);
        assert!(cleaned != shape(mol, ids), "the test structure must need cleaning up");
        cleaned
    }

    /// A + B → C with conditions over and under the arrow, everything off the line.
    #[test]
    fn a_one_step_scheme_lines_up_on_its_fixed_arrow() {
        let mut mol = Molecule::default();
        let a = sloppy_chain(&mut mol, [-14.0, 1.0], 3);
        let plus = mol.add_text([-8.5, -0.8], "+");
        let b = sloppy_chain(&mut mol, [-6.0, 2.5], 2);
        let arrow = mol.add_arrow([0.0, 0.0], [5.0, 0.0], ArrowKind::Reaction);
        let above = mol.add_text([0.8, -0.6], "MeI");
        let below = mol.add_text([0.5, 1.0], "K_2CO_3");
        let c = sloppy_chain(&mut mol, [9.0, -2.0], 4);
        let shapes: Vec<Vec<[f32; 2]>> = [&a, &b, &c].iter().map(|s| cleaned_shape(&mol, s)).collect();
        let (above_box, below_box) = (object_box(&mol, above), object_box(&mol, below));

        assert!(mol.cleanup_reaction_scheme(fake_extents));

        let PageObject::Arrow(after) = mol.object_by_id(arrow).unwrap() else { panic!("an arrow") };
        assert_eq!((after.tail, after.head), ([0.0, 0.0], [5.0, 0.0]), "the arrow stays");
        assert_eq!(object_box(&mol, above), above_box, "conditions ride on the arrow");
        assert_eq!(object_box(&mol, below), below_box, "conditions ride on the arrow");
        for (s, want) in [&a, &b, &c].iter().zip(&shapes) {
            assert!(atoms_box(&mol, s).center()[1].abs() < EPS, "structure centred on the shaft");
            assert_same_shape(&shape(&mol, s), want);
        }
        assert!(object_box(&mol, plus).center()[1].abs() < EPS, "+ centred on the shaft");

        let arrow_box = object_box(&mol, arrow).union(above_box).union(below_box);
        let line = [atoms_box(&mol, &a), object_box(&mol, plus), atoms_box(&mol, &b), arrow_box, atoms_box(&mol, &c)];
        let g: Vec<f32> = line.windows(2).map(|w| w[1].min[0] - w[0].max[0]).collect();
        assert!(g.iter().all(|&x| x > 0.0), "order kept, nothing overlapping: {g:?}");
        assert_all_close(&g, g[0], "even gaps");
    }

    /// A → B → C: both arrows stay, B sits in the middle of the room between them, and A and C
    /// are as far from their arrows as the line's gap.
    #[test]
    fn a_two_step_scheme_keeps_both_arrows() {
        let mut mol = Molecule::default();
        let a = sloppy_chain(&mut mol, [-9.0, 2.0], 3);
        let first = mol.add_arrow([0.0, 0.0], [4.0, 0.0], ArrowKind::Reaction);
        let b = sloppy_chain(&mut mol, [4.5, -1.5], 3);
        let second = mol.add_arrow([14.0, 0.0], [18.0, 0.0], ArrowKind::Equilibrium);
        let c = sloppy_chain(&mut mol, [25.0, 3.0], 2);

        assert!(mol.cleanup_reaction_scheme(fake_extents));

        assert_eq!(mol.object_by_id(first).unwrap().key_points(), vec![[0.0, 0.0], [4.0, 0.0]]);
        assert_eq!(mol.object_by_id(second).unwrap().key_points(), vec![[14.0, 0.0], [18.0, 0.0]]);
        for s in [&a, &b, &c] {
            assert!(atoms_box(&mol, s).center()[1].abs() < EPS, "centred on the shafts");
        }
        let (first_box, second_box) = (object_box(&mol, first), object_box(&mol, second));
        let bb = atoms_box(&mol, &b);
        let (left, right) = (bb.min[0] - first_box.max[0], second_box.min[0] - bb.max[0]);
        assert!((left - right).abs() < EPS && left > 0.0, "B halfway between the arrows: {left} {right}");
        let before = first_box.min[0] - atoms_box(&mol, &a).max[0];
        let after = atoms_box(&mol, &c).min[0] - second_box.max[0];
        assert!((before - after).abs() < EPS && before > 0.0, "the line's gap on both ends: {before} {after}");
    }

    /// A compound number under a structure goes with it, keeping its place under it.
    #[test]
    fn a_caption_goes_with_its_structure() {
        let mut mol = Molecule::default();
        let a = sloppy_chain(&mut mol, [-9.0, 3.0], 3);
        let ab = atoms_box(&mol, &a);
        let caption = mol.add_text([ab.center()[0] - 0.2, ab.max[1] + 0.8], "1");
        let drop = object_box(&mol, caption).min[1] - ab.max[1];
        mol.add_arrow([0.0, 0.0], [4.0, 0.0], ArrowKind::Reaction);

        assert!(mol.cleanup_reaction_scheme(fake_extents));

        let (ab, cb) = (atoms_box(&mol, &a), object_box(&mol, caption));
        assert!(ab.center()[1].abs() < EPS, "the structure is centred, not its caption with it");
        assert!((cb.min[1] - ab.max[1] - drop).abs() < EPS, "same distance under it");
        assert!((cb.center()[0] - ab.center()[0]).abs() < EPS, "still under its middle");
    }

    /// A reagent drawn over the arrow is cleaned up but stays over it, as far from the shaft.
    #[test]
    fn a_structure_over_the_arrow_stays_over_it() {
        let mut mol = Molecule::default();
        mol.add_arrow([0.0, 0.0], [8.0, 0.0], ArrowKind::Reaction);
        let r = sloppy_chain(&mut mol, [1.0, -2.5], 3);
        let want = cleaned_shape(&mol, &r);
        let before = atoms_box(&mol, &r);

        assert!(mol.cleanup_reaction_scheme(fake_extents));

        let after = atoms_box(&mol, &r);
        assert!((after.max[1] - before.max[1]).abs() < EPS, "its bottom stays: {before:?} {after:?}");
        assert!((after.center()[0] - before.center()[0]).abs() < EPS);
        assert_same_shape(&shape(&mol, &r), &want);
    }

    /// An arrow running down the page (to a next row) stays, with its conditions, but makes no
    /// line: the structure beside it goes on the line of the arrow across the page.
    #[test]
    fn an_arrow_down_the_page_makes_no_line() {
        let mut mol = Molecule::default();
        let across = mol.add_arrow([0.0, 0.0], [4.0, 0.0], ArrowKind::Reaction);
        let down = mol.add_arrow([16.0, 2.0], [16.0, 8.0], ArrowKind::Reaction);
        let condition = mol.add_text([16.5, 5.0], "H_2");
        let p = sloppy_chain(&mut mol, [7.0, 4.0], 3);
        let condition_box = object_box(&mol, condition);

        assert!(mol.cleanup_reaction_scheme(fake_extents));

        assert_eq!(mol.object_by_id(across).unwrap().key_points(), vec![[0.0, 0.0], [4.0, 0.0]]);
        assert_eq!(mol.object_by_id(down).unwrap().key_points(), vec![[16.0, 2.0], [16.0, 8.0]]);
        assert_eq!(object_box(&mol, condition), condition_box, "rides on the arrow down");
        assert!(atoms_box(&mol, &p).center()[1].abs() < EPS, "on the line across, not at 5");
    }

    #[test]
    fn without_an_arrow_nothing_changes() {
        let mut mol = Molecule::default();
        sloppy_chain(&mut mol, [0.0, 0.0], 3);
        mol.add_text([5.0, 0.0], "+");
        let before = mol.clone();
        assert!(!mol.cleanup_reaction_scheme(fake_extents));
        assert_eq!(mol, before);
    }
}
