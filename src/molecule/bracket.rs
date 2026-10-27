//! Polymer and repeating-unit brackets: ChemDraw's *bracketed groups*.
//!
//! A [`Bracket`] encloses a set of atoms and says what the enclosure means ([`BracketUsage`]): a
//! structural repeat unit `[…]n`, a multiple group `[…]3`, one unit of a copolymer, … It lives
//! in [`Molecule::objects`](super::Molecule::objects) as
//! [`PageObject::Bracket`](super::PageObject::Bracket), beside arrows and text, so undo, JSON
//! saving and the shared scene cover it as they cover them.
//!
//! Only the enclosed atoms are stored. The bonds that cross the brackets and where the two
//! bracket marks stand are worked out from the drawing whenever they are needed
//! ([`crossing_bonds`], [`geometry`]), so the brackets follow their atoms as they are moved,
//! rotated or cleaned up, and never point at a bond that is gone. ChemDraw places them the same
//! way: one bracket across each crossing bond, at its middle, both standing square to the line
//! through the two crossings; with no crossing bond (a multiple group or a mixture drawn as a
//! whole structure) the brackets stand left and right of the enclosed atoms.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::Molecule;

/// What a bracket means: ChemDraw's `BracketUsage` (CDX property 0x0A24), every value of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BracketUsage {
    /// A structural repeat unit (structure-based polymer): `[…]n`.
    #[default]
    Sru,
    /// The enclosed atoms repeat a known number of times: `[…]3`.
    MultipleGroup,
    /// One repeating unit of a copolymer (`co`).
    Copolymer,
    /// One unit of an alternating copolymer (`alt`).
    CopolymerAlternating,
    /// One unit of a random copolymer (`ran`).
    CopolymerRandom,
    /// One unit of a block copolymer (`blk`).
    CopolymerBlock,
    /// A generic polymer (`gen`).
    Generic,
    /// A source-based monomer (`mon`).
    Monomer,
    /// A source-based monomer that does not self-polymerize (`mer`).
    Mer,
    /// A cross-link (`xl`).
    Crosslink,
    /// A graft (`grf`).
    Graft,
    /// A modification (`mod`).
    Modification,
    /// A component of a mixture (`c`).
    Component,
    /// An unordered mixture (`mix`).
    MixtureUnordered,
    /// An ordered mixture, a formulation (`f`).
    MixtureOrdered,
    /// No meaning given.
    Unspecified,
}

impl BracketUsage {
    /// Every usage, in the order a menu lists them.
    pub const ALL: [BracketUsage; 16] = [
        BracketUsage::Sru,
        BracketUsage::MultipleGroup,
        BracketUsage::Copolymer,
        BracketUsage::CopolymerAlternating,
        BracketUsage::CopolymerRandom,
        BracketUsage::CopolymerBlock,
        BracketUsage::Generic,
        BracketUsage::Monomer,
        BracketUsage::Mer,
        BracketUsage::Crosslink,
        BracketUsage::Graft,
        BracketUsage::Modification,
        BracketUsage::Component,
        BracketUsage::MixtureUnordered,
        BracketUsage::MixtureOrdered,
        BracketUsage::Unspecified,
    ];

    /// The value ChemDraw writes for this usage.
    pub fn cdx_value(self) -> i8 {
        match self {
            BracketUsage::Unspecified => 0,
            BracketUsage::Sru => 3,
            BracketUsage::Monomer => 4,
            BracketUsage::Mer => 5,
            BracketUsage::Copolymer => 6,
            BracketUsage::CopolymerAlternating => 7,
            BracketUsage::CopolymerRandom => 8,
            BracketUsage::CopolymerBlock => 9,
            BracketUsage::Crosslink => 10,
            BracketUsage::Graft => 11,
            BracketUsage::Modification => 12,
            BracketUsage::Component => 13,
            BracketUsage::MixtureUnordered => 14,
            BracketUsage::MixtureOrdered => 15,
            BracketUsage::MultipleGroup => 16,
            BracketUsage::Generic => 17,
        }
    }

    /// The usage ChemDraw's value stands for (`None` for a value it does not define).
    pub fn from_cdx(value: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|u| u.cdx_value() as i32 == value)
    }

    /// The label a new bracket of this usage shows: `n` for a repeat unit, `2` for a multiple
    /// group, the usual abbreviation for the others (empty when unspecified).
    pub fn default_label(self) -> &'static str {
        match self {
            BracketUsage::Sru => "n",
            BracketUsage::MultipleGroup => "2",
            BracketUsage::Copolymer => "co",
            BracketUsage::CopolymerAlternating => "alt",
            BracketUsage::CopolymerRandom => "ran",
            BracketUsage::CopolymerBlock => "blk",
            BracketUsage::Generic => "gen",
            BracketUsage::Monomer => "mon",
            BracketUsage::Mer => "mer",
            BracketUsage::Crosslink => "xl",
            BracketUsage::Graft => "grf",
            BracketUsage::Modification => "mod",
            BracketUsage::Component => "c",
            BracketUsage::MixtureUnordered => "mix",
            BracketUsage::MixtureOrdered => "f",
            BracketUsage::Unspecified => "",
        }
    }

    /// Whether the brackets enclose one repeating unit of a polymer chain (a structure-based
    /// repeat unit, a copolymer's unit, …): its formula is a unit's, written `(C2H4)n`.
    pub fn is_polymer_unit(self) -> bool {
        matches!(
            self,
            BracketUsage::Sru
                | BracketUsage::Copolymer
                | BracketUsage::CopolymerAlternating
                | BracketUsage::CopolymerRandom
                | BracketUsage::CopolymerBlock
                | BracketUsage::Generic
                | BracketUsage::Monomer
                | BracketUsage::Mer
                | BracketUsage::Crosslink
                | BracketUsage::Graft
                | BracketUsage::Modification
        )
    }
}

/// How the repeat units of a polymer join: ChemDraw's `PolymerRepeatPattern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RepeatPattern {
    /// One end of a unit joins the other end of the next (ChemDraw's default).
    #[default]
    HeadToTail,
    /// One end of a unit joins the same end of the next.
    HeadToHead,
    /// A mixture of both, or not known.
    EitherUnknown,
}

impl RepeatPattern {
    pub fn cdx_value(self) -> i8 {
        match self {
            RepeatPattern::HeadToTail => 0,
            RepeatPattern::HeadToHead => 1,
            RepeatPattern::EitherUnknown => 2,
        }
    }

    pub fn from_cdx(value: i32) -> Self {
        match value {
            1 => RepeatPattern::HeadToHead,
            2 => RepeatPattern::EitherUnknown,
            _ => RepeatPattern::HeadToTail,
        }
    }
}

/// The shape of the bracket marks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum BracketShape {
    /// `[ ]` (ChemDraw's default for polymers).
    #[default]
    Square,
    /// `( )`.
    Round,
    /// `{ }`.
    Curly,
}

impl BracketShape {
    /// ChemDraw's `BracketType` for one bracket of this shape (`Square`, `Round`, `Curly`).
    pub fn cdx_value(self) -> i16 {
        match self {
            BracketShape::Square => 3,
            BracketShape::Curly => 4,
            BracketShape::Round => 5,
        }
    }

    /// The shape of ChemDraw's `BracketType` (pairs included); absent means round, as ChemDraw
    /// reads it.
    pub fn from_cdx(value: Option<i32>) -> Self {
        match value {
            Some(1 | 3) => BracketShape::Square,
            Some(2 | 4) => BracketShape::Curly,
            _ => BracketShape::Round,
        }
    }
}

/// A pair of brackets around some atoms (see the [module docs](self)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bracket {
    pub id: u32,
    /// The enclosed atoms. Atoms deleted since are dropped (see
    /// [`Molecule::remove_atom`](super::Molecule::remove_atom)); any still listed that are
    /// gone are ignored.
    pub atoms: Vec<u32>,
    #[serde(default)]
    pub usage: BracketUsage,
    /// The label at the lower right of the closing bracket: `n`, `x`, the repeat count `3` of a
    /// multiple group, `co` of a copolymer.
    #[serde(default)]
    pub label: String,
    /// For a repeat unit: how the units join.
    #[serde(default)]
    pub repeat_pattern: RepeatPattern,
    #[serde(default)]
    pub shape: BracketShape,
    /// The id of the ChemDraw bracketed group this was read from; see
    /// [`PageArrow::source`](super::PageArrow::source).
    #[serde(default)]
    pub source: Option<u32>,
}

impl Bracket {
    /// New brackets of `usage` around `atoms`, with that usage's default label.
    pub fn new(atoms: Vec<u32>, usage: BracketUsage) -> Self {
        Bracket {
            id: 0,
            atoms,
            usage,
            label: usage.default_label().to_string(),
            repeat_pattern: RepeatPattern::HeadToTail,
            shape: BracketShape::Square,
            source: None,
        }
    }

    /// How many times a multiple group repeats: its label as a positive whole number (`None`
    /// for any other usage, or a label that is not one).
    pub fn repeat_count(&self) -> Option<usize> {
        if self.usage != BracketUsage::MultipleGroup {
            return None;
        }
        self.label.trim().parse::<usize>().ok().filter(|&n| n > 0)
    }
}

/// A bond crossing the brackets: the bond, its atom inside and its atom outside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CrossingBond {
    pub bond: u32,
    pub inner: u32,
    pub outer: u32,
}

/// The enclosed atoms of `bracket` that still exist (hydrogens folded into a label included,
/// as they are part of their atom).
pub fn enclosed_atoms(mol: &Molecule, bracket: &Bracket) -> HashSet<u32> {
    bracket.atoms.iter().copied().filter(|&id| mol.atom_by_id(id).is_some()).collect()
}

/// The bonds that leave the enclosed atoms, in bond order (a hydrogen folded into a label
/// never makes one).
pub fn crossing_bonds(mol: &Molecule, bracket: &Bracket) -> Vec<CrossingBond> {
    let inside = enclosed_atoms(mol, bracket);
    mol.bonds
        .iter()
        .filter_map(|b| {
            let (inner, outer) = match (inside.contains(&b.begin), inside.contains(&b.end)) {
                (true, false) => (b.begin, b.end),
                (false, true) => (b.end, b.begin),
                _ => return None,
            };
            if super::depict::is_folded_h(mol, outer) || super::depict::is_folded_h(mol, inner) {
                return None;
            }
            mol.atom_by_id(outer)?;
            Some(CrossingBond { bond: b.id, inner, outer })
        })
        .collect()
}

/// One bracket mark: a line from `start` to `end` whose lips turn to its left on the page (y
/// down) — ChemDraw's convention for a bracket graphic's two points — and the bond it crosses.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BracketSide {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub crossing: Option<CrossingBond>,
}

impl BracketSide {
    /// The unit vector the lips point along (into the brackets).
    pub fn lip_direction(&self) -> [f32; 2] {
        let (dx, dy) = (self.end[0] - self.start[0], self.end[1] - self.start[1]);
        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
        [-dy / len, dx / len]
    }

    pub fn length(&self) -> f32 {
        ((self.end[0] - self.start[0]).powi(2) + (self.end[1] - self.start[1]).powi(2)).sqrt()
    }

    fn center(&self) -> [f32; 2] {
        [(self.start[0] + self.end[0]) / 2.0, (self.start[1] + self.end[1]) / 2.0]
    }
}

/// Where a bracket pair is drawn, in molecule units.
#[derive(Debug, Clone, PartialEq)]
pub struct BracketGeometry {
    /// One mark per crossing bond (two around atoms nothing crosses); the closing one — the
    /// one the label goes with — last.
    pub sides: Vec<BracketSide>,
    /// The left end of the label's baseline: below and right of the closing bracket's lower end.
    pub label_at: [f32; 2],
}

/// A bond's length, in molecule units.
pub const BOND_LENGTH: f32 = 1.5;
/// How far a bracket reaches past the outermost enclosed atom.
const MARGIN: f32 = 0.25 * BOND_LENGTH;
/// The shortest bracket: one bond long.
const MIN_LENGTH: f32 = BOND_LENGTH;
/// From the closing bracket's lower end to the label's baseline origin (ChemDraw: 3.5 pt
/// right, 2.6 pt down, at 14.4 pt per bond).
const LABEL_OFFSET: [f32; 2] = [3.5 * BOND_LENGTH / 14.4, 2.6 * BOND_LENGTH / 14.4];

/// The length of a bracket's lips, in molecule units: a tenth of the bracket, kept within
/// sensible bounds.
pub fn lip_length(side: &BracketSide) -> f32 {
    (side.length() * 0.1).clamp(0.12, 0.3)
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn normalized(v: [f32; 2]) -> Option<[f32; 2]> {
    let len = dot(v, v).sqrt();
    (len > 1e-5).then(|| [v[0] / len, v[1] / len])
}

/// A bracket mark through `at`, square to `axis` (a unit vector), spanning the positions
/// `points` project onto across it (with a margin, and at least [`MIN_LENGTH`]), its lips
/// turned towards `inward`.
fn side_across(at: [f32; 2], axis: [f32; 2], points: &[[f32; 2]], inward: [f32; 2], crossing: Option<CrossingBond>) -> BracketSide {
    let across = [-axis[1], axis[0]];
    let (mut lo, mut hi) = (0.0f32, 0.0f32);
    for p in points {
        let t = dot(sub(*p, at), across);
        lo = lo.min(t);
        hi = hi.max(t);
    }
    lo -= MARGIN;
    hi += MARGIN;
    if hi - lo < MIN_LENGTH {
        let mid = (hi + lo) / 2.0;
        lo = mid - MIN_LENGTH / 2.0;
        hi = mid + MIN_LENGTH / 2.0;
    }
    let a = [at[0] + across[0] * lo, at[1] + across[1] * lo];
    let b = [at[0] + across[0] * hi, at[1] + across[1] * hi];
    let side = BracketSide { start: a, end: b, crossing };
    if dot(side.lip_direction(), inward) >= 0.0 { side } else { BracketSide { start: b, end: a, crossing } }
}

/// Where `bracket`'s marks stand on `mol` as drawn now (see the [module docs](self)), or `None`
/// when none of its atoms is left.
pub fn geometry(mol: &Molecule, bracket: &Bracket) -> Option<BracketGeometry> {
    let inside = enclosed_atoms(mol, bracket);
    let points: Vec<[f32; 2]> =
        mol.atoms.iter().filter(|a| inside.contains(&a.id) && !super::depict::is_folded_h(mol, a.id)).map(|a| a.pos).collect();
    if points.is_empty() {
        return None;
    }
    let n = points.len() as f32;
    let centroid = [points.iter().map(|p| p[0]).sum::<f32>() / n, points.iter().map(|p| p[1]).sum::<f32>() / n];
    let pos = |id: u32| mol.atom_by_id(id).map(|a| a.pos).unwrap_or(centroid);
    let crossings = crossing_bonds(mol, bracket);
    let mid = |c: &CrossingBond| {
        let (p, q) = (pos(c.inner), pos(c.outer));
        [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]
    };
    let mut sides = match crossings.as_slice() {
        [] => {
            // Left and right of the atoms.
            let (min_x, max_x) = points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p[0]), hi.max(p[0])));
            let left = [min_x - MARGIN, centroid[1]];
            let right = [max_x + MARGIN, centroid[1]];
            vec![
                side_across(left, [1.0, 0.0], &points, [1.0, 0.0], None),
                side_across(right, [1.0, 0.0], &points, [-1.0, 0.0], None),
            ]
        }
        [c] => {
            // Across the bond, and on the far side of the atoms from it.
            let m = mid(c);
            let axis = normalized(sub(pos(c.inner), pos(c.outer))).unwrap_or([1.0, 0.0]);
            let far = points.iter().map(|p| dot(sub(*p, m), axis)).fold(0.0f32, f32::max) + MARGIN;
            let other = [m[0] + axis[0] * far, m[1] + axis[1] * far];
            vec![
                side_across(m, axis, &points, axis, Some(*c)),
                side_across(other, axis, &points, [-axis[0], -axis[1]], None),
            ]
        }
        [c1, c2] => {
            // Both square to the line through the two crossings.
            let (m1, m2) = (mid(c1), mid(c2));
            let axis = normalized(sub(m2, m1)).or_else(|| normalized(sub(pos(c1.inner), pos(c1.outer)))).unwrap_or([1.0, 0.0]);
            let mut span = points.clone();
            span.extend([m1, m2]);
            vec![
                side_across(m1, axis, &span, sub(centroid, m1), Some(*c1)),
                side_across(m2, axis, &span, sub(centroid, m2), Some(*c2)),
            ]
        }
        many => many
            .iter()
            .map(|c| {
                let m = mid(c);
                let axis = normalized(sub(pos(c.inner), pos(c.outer))).unwrap_or([1.0, 0.0]);
                side_across(m, axis, &[m], axis, Some(*c))
            })
            .collect(),
    };
    // The closing bracket is the rightmost (the lowest of two side by side).
    let key = |s: &BracketSide| {
        let c = s.center();
        c[0] + c[1] * 1e-3
    };
    let closing = (0..sides.len()).max_by(|&a, &b| key(&sides[a]).total_cmp(&key(&sides[b]))).unwrap_or(0);
    let last = sides.remove(closing);
    sides.push(last);
    let lower = if last.start[1] > last.end[1] + 1e-4 || ((last.start[1] - last.end[1]).abs() <= 1e-4 && last.start[0] > last.end[0]) {
        last.start
    } else {
        last.end
    };
    let label_at = [lower[0] + LABEL_OFFSET[0], lower[1] + LABEL_OFFSET[1]];
    Some(BracketGeometry { sides, label_at })
}

/// The outline of one bracket mark as polylines (molecule units): a square bracket's spine and
/// lips, a round one's arc, a curly one's brace.
pub fn side_polylines(side: &BracketSide, shape: BracketShape) -> Vec<Vec<[f32; 2]>> {
    let lip = lip_length(side);
    let n = side.lip_direction();
    let at = |t: f32, inward: f32| {
        [
            side.start[0] + (side.end[0] - side.start[0]) * t + n[0] * inward,
            side.start[1] + (side.end[1] - side.start[1]) * t + n[1] * inward,
        ]
    };
    const STEPS: usize = 24;
    match shape {
        BracketShape::Square => vec![vec![at(0.0, lip), at(0.0, 0.0), at(1.0, 0.0), at(1.0, lip)]],
        BracketShape::Round => {
            vec![(0..=STEPS).map(|k| k as f32 / STEPS as f32).map(|t| at(t, lip * (2.0 * t - 1.0).powi(2))).collect()]
        }
        BracketShape::Curly => {
            // Ends turned in, a point out at the middle.
            vec![(0..=STEPS)
                .map(|k| k as f32 / STEPS as f32)
                .map(|t| {
                    let u = (2.0 * t - 1.0).abs();
                    at(t, lip * u * u - lip * 0.8 * (1.0 - u).powi(3))
                })
                .collect()]
        }
    }
}

/// The atoms to enclose for a selection: the selected atoms that exist, with the hydrogens
/// folded into their labels.
pub fn atoms_for_selection(mol: &Molecule, selected: &HashSet<u32>) -> Vec<u32> {
    let mut out: Vec<u32> = mol.atoms.iter().filter(|a| selected.contains(&a.id)).map(|a| a.id).collect();
    let chosen: HashSet<u32> = out.iter().copied().collect();
    for a in &mol.atoms {
        if !chosen.contains(&a.id) && super::depict::is_folded_h(mol, a.id) && mol.neighbor_atom_ids(a.id).iter().any(|n| chosen.contains(n)) {
            out.push(a.id);
        }
    }
    out
}

/// "Open ends" of the polymer brackets of `mol`: the atoms at the outer end of a bond crossing
/// a repeat unit's brackets that only stand for the chain going on — an unlabelled carbon with
/// no other bond, outside every bracket. Their formula is not counted.
pub fn open_ends(mol: &Molecule) -> HashSet<u32> {
    let brackets: Vec<&Bracket> = mol.objects.iter().filter_map(|o| o.as_bracket()).collect();
    let enclosed: HashSet<u32> = brackets.iter().flat_map(|b| b.atoms.iter().copied()).collect();
    let mut out = HashSet::new();
    for b in brackets.iter().filter(|b| b.usage.is_polymer_unit()) {
        for c in crossing_bonds(mol, b) {
            let Some(atom) = mol.atom_by_id(c.outer) else { continue };
            let heavy_bonds = mol.neighbor_atom_ids(c.outer).into_iter().filter(|&n| !super::depict::is_folded_h(mol, n)).count();
            if atom.element == "C" && atom.charge == 0 && heavy_bonds == 1 && !enclosed.contains(&c.outer) {
                out.insert(c.outer);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, PageObject};

    /// Polyethylene's repeat unit as drawn: C0–[C1–C2]–C3, a zigzag along x.
    pub(crate) fn polyethylene() -> (Molecule, u32) {
        let mut m = Molecule::default();
        let ids: Vec<u32> = (0..4)
            .map(|k| m.add_atom("C".into(), [k as f32 * 1.3, if k % 2 == 0 { 0.0 } else { -0.75 }], 0))
            .collect();
        for w in ids.windows(2) {
            m.add_bond(w[0], w[1], BondOrder::Single);
        }
        let id = m.push_object(PageObject::Bracket(Bracket::new(vec![ids[1], ids[2]], BracketUsage::Sru)));
        (m, id)
    }

    fn bracket(m: &Molecule, id: u32) -> &Bracket {
        m.object_by_id(id).unwrap().as_bracket().unwrap()
    }

    #[test]
    fn a_repeat_unit_is_crossed_by_two_bonds_with_a_bracket_on_each() {
        let (m, id) = polyethylene();
        let b = bracket(&m, id);
        assert_eq!(crossing_bonds(&m, b).len(), 2);
        let g = geometry(&m, b).unwrap();
        assert_eq!(g.sides.len(), 2);
        // Vertical (the crossings are level), lips inward, the closing one on the right.
        for s in &g.sides {
            assert!((s.start[0] - s.end[0]).abs() < 1e-4, "{s:?}");
            assert!(s.length() >= MIN_LENGTH - 1e-4);
        }
        assert!(g.sides[0].lip_direction()[0] > 0.9 && g.sides[1].lip_direction()[0] < -0.9, "{g:?}");
        assert!(g.sides[1].start[0] > g.sides[0].start[0]);
        // Each one across the middle of its bond.
        assert!((g.sides[0].start[0] - 0.65).abs() < 1e-4 && (g.sides[1].start[0] - 3.25).abs() < 1e-4, "{g:?}");
        // The label right of and below the closing bracket.
        let lower = g.sides[1].start[1].max(g.sides[1].end[1]);
        assert!(g.label_at[0] > g.sides[1].start[0] && g.label_at[1] > lower);
    }

    #[test]
    fn brackets_follow_their_atoms() {
        let (mut m, id) = polyethylene();
        let before = geometry(&m, bracket(&m, id)).unwrap();
        for a in &mut m.atoms {
            a.pos[0] += 2.0;
            a.pos[1] += 1.0;
        }
        let after = geometry(&m, bracket(&m, id)).unwrap();
        for (p, q) in before.sides.iter().zip(&after.sides) {
            assert!((q.start[0] - p.start[0] - 2.0).abs() < 1e-4 && (q.end[1] - p.end[1] - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn nothing_crossing_puts_the_brackets_left_and_right() {
        let mut m = Molecule::default();
        let a = m.add_atom("C".into(), [0.0, 0.0], 0);
        let b = m.add_atom("O".into(), [1.5, 0.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        let id = m.push_object(PageObject::Bracket(Bracket::new(vec![a, b], BracketUsage::MultipleGroup)));
        let g = geometry(&m, bracket(&m, id)).unwrap();
        assert!(g.sides[0].start[0] < 0.0 && g.sides[1].start[0] > 1.5, "{g:?}");
        assert_eq!(bracket(&m, id).repeat_count(), Some(2));
    }

    #[test]
    fn open_ends_are_the_bare_carbons_the_chain_goes_on_through() {
        let (m, _) = polyethylene();
        let ends = open_ends(&m);
        assert_eq!(ends.len(), 2);
        assert!(ends.contains(&m.atoms[0].id) && ends.contains(&m.atoms[3].id));
    }

    #[test]
    fn shapes_have_outlines() {
        let side = BracketSide { start: [0.0, 1.0], end: [0.0, -1.0], crossing: None };
        assert_eq!(side_polylines(&side, BracketShape::Square)[0].len(), 4);
        for shape in [BracketShape::Round, BracketShape::Curly] {
            let line = &side_polylines(&side, shape)[0];
            assert!(line.len() > 10);
            assert!(line[0][0] > 0.0, "{shape:?}: the ends turn in (to the right here)");
        }
    }

    #[test]
    fn usages_map_to_chemdraw_and_back() {
        for u in BracketUsage::ALL {
            assert_eq!(BracketUsage::from_cdx(u.cdx_value() as i32), Some(u));
        }
        assert_eq!(BracketUsage::from_cdx(1), None);
        assert_eq!(BracketShape::from_cdx(Some(3)), BracketShape::Square);
        assert_eq!(BracketShape::from_cdx(None), BracketShape::Round);
    }
}
