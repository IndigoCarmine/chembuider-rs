//! Curly arrows: the electron-pushing arrows of a reaction mechanism.
//!
//! A [`CurlyArrow`] says where a pair of electrons (or, as a fish-hook, a single electron) comes
//! from and where it goes, and what shape it takes on the way. Its two ends name what they are
//! on rather than where they are: a lone pair of an atom or a bond at the tail; an atom, a bond,
//! or the gap between two atoms where a new bond forms at the head ([`CurlyEnd`]). So the arrow
//! follows its atoms when they are moved, rotated or cleaned up, the way ring fills and brackets
//! do, and it lives in [`Molecule::objects`](super::Molecule::objects) beside them, so undo, JSON
//! saving and the shared scene cover it without a path of its own.
//!
//! Its shape is one of two kinds:
//!
//! - **An arc** (the default, what a new arrow is): a circular arc of the stored sweep
//!   ([`CurlyArrow::angle`], ChemDraw's `AngularSize`), which may lean toward one end
//!   ([`CurlyArrow::skew`]). It is laid between the two anchors (an atom's centre, a point on a
//!   bond, the middle between two atoms) and then pulled off them along its own direction.
//! - **A free path** ([`CurlyArrow::path`]): a chain of cubic Bézier pieces, for an arrow that
//!   has to wind round atoms — an S, or a long U or L from one structure to another. Its control
//!   points and the knots between pieces are kept in the frame of the line from tail anchor to
//!   head anchor (along it, and across it to the right), so the shape turns, stretches and
//!   mirrors with its atoms. The path runs from anchor to anchor and is cut back at each end by
//!   the room the end needs.
//!
//! Either way the tail leaves from just outside the lone pair (outside the atom's label, as far
//! as the label reaches the way the arrow leaves) or just off the bond, and the head stops short
//! of its atom's label, so where the arrow is drawn is worked out from the drawing each time
//! ([`geometry`]).
//!
//! An end whose atom or bond is deleted becomes a free point where the arrow was drawn
//! ([`CurlyEnd::Point`]), so nothing jumps; an arrow read from a ChemDraw file that cannot be
//! tied to atoms has free ends from the start.

use serde::{Deserialize, Serialize};

use super::Molecule;

/// How many electrons an arrow moves, which decides its head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum CurlyKind {
    /// An electron pair: a full head.
    #[default]
    Pair,
    /// A single electron (a radical step): a fish-hook, half a head on the outer side of the
    /// curve's last turn.
    Single,
}

/// What one end of a curly arrow is on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CurlyEnd {
    /// At the tail, a lone pair of this atom; at the head, the atom the electrons go to.
    Atom(u32),
    /// A bond. At the tail, its electrons, the tail standing on the bond toward `side` (one of
    /// its two atoms: the one the electrons stay with); at the head, the bond the electrons go
    /// into (a single bond becoming double), pointed at its middle. `side` means nothing there.
    Bond { bond: u32, side: u32 },
    /// The middle between two atoms that are not bonded yet: the head of an arrow that makes a
    /// new bond between them.
    Between(u32, u32),
    /// A free point on the page (molecule units), tied to nothing.
    Point([f32; 2]),
}

impl CurlyEnd {
    /// Whether the end is tied to the drawing (anything but a free point).
    pub fn is_attached(&self) -> bool {
        !matches!(self, CurlyEnd::Point(_))
    }

    /// The atoms the end depends on: an atom, a bond's two atoms, the two atoms of a gap.
    pub fn atoms(&self, mol: &Molecule) -> Vec<u32> {
        match *self {
            CurlyEnd::Atom(a) => vec![a],
            CurlyEnd::Bond { bond, .. } => mol.bond_by_id(bond).map(|b| vec![b.begin, b.end]).unwrap_or_default(),
            CurlyEnd::Between(a, b) => vec![a, b],
            CurlyEnd::Point(_) => Vec::new(),
        }
    }

    /// Whether removing atom `atom` (and with it its bonds) leaves this end without its anchor.
    fn loses_atom(&self, mol: &Molecule, atom: u32) -> bool {
        match *self {
            CurlyEnd::Atom(a) => a == atom,
            CurlyEnd::Bond { bond, side } => side == atom || mol.bond_by_id(bond).is_none_or(|b| b.begin == atom || b.end == atom),
            CurlyEnd::Between(a, b) => a == atom || b == atom,
            CurlyEnd::Point(_) => false,
        }
    }

    fn loses_bond(&self, bond_id: u32) -> bool {
        matches!(*self, CurlyEnd::Bond { bond, .. } if bond == bond_id)
    }
}

/// An electron-pushing arrow from `tail` to `head`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurlyArrow {
    pub id: u32,
    pub tail: CurlyEnd,
    pub head: CurlyEnd,
    /// How far the arc turns from tail to head, in degrees: ChemDraw's `AngularSize`, with its
    /// sign. Positive turns counterclockwise as seen on the page, which bends the arc out to the
    /// right of the way it goes; negative bends it to the left. Near 0 the arrow is straight.
    /// Not used while the arrow has a [`path`](Self::path).
    pub angle: f32,
    /// How far the arc's highest point leans toward the head (positive) or the tail (negative),
    /// as a share of half the distance between the ends; 0 is a plain circular arc.
    #[serde(default)]
    pub skew: f32,
    /// A free shape instead of the arc: the inner points of a chain of cubic Bézier pieces from
    /// the tail anchor to the head anchor — each piece's two control points, with the knot it
    /// ends at between one piece and the next (`[c1, c2, knot, c1, c2, knot, …, c1, c2]`, so
    /// `3n − 1` points for `n` pieces). Each point is `[along, across]` in the frame of the line
    /// from tail anchor to head anchor: `along` in lengths of that line from the tail anchor,
    /// `across` the same to its right on the page. Empty for an arc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<[f32; 2]>,
    #[serde(default)]
    pub kind: CurlyKind,
    /// Draw the lone pair a tail on an atom starts from, as two dots.
    #[serde(default)]
    pub dots: bool,
    /// The id of the ChemDraw object this was read from (its `arrow`, or its `curve`); see
    /// [`PageArrow::source`](super::PageArrow::source).
    #[serde(default)]
    pub source: Option<u32>,
}

impl CurlyArrow {
    /// A new arrow from `tail` to `head` bending `angle` degrees: a plain arc. A lone-pair tail
    /// shows its dots.
    pub fn new(tail: CurlyEnd, head: CurlyEnd, angle: f32, kind: CurlyKind) -> Self {
        let dots = matches!(tail, CurlyEnd::Atom(_));
        CurlyArrow { id: 0, tail, head, angle: clamp_angle(angle), skew: 0.0, path: Vec::new(), kind, dots, source: None }
    }

    /// The atoms the arrow depends on, tail first.
    pub fn atoms(&self, mol: &Molecule) -> Vec<u32> {
        let mut atoms = self.tail.atoms(mol);
        atoms.extend(self.head.atoms(mol));
        atoms
    }

    /// Whether the arrow has a free shape ([`path`](Self::path)) rather than the arc. A path of
    /// the wrong length is no shape.
    pub fn is_shaped(&self) -> bool {
        !self.path.is_empty() && self.path.len() % 3 == 2
    }

    /// Bend the other way, as a mirror image does: the arc's sweep, and a path's points across
    /// the line between the ends.
    pub fn flip(&mut self) {
        self.angle = -self.angle;
        for p in &mut self.path {
            p[1] = -p[1];
        }
    }

    /// The positions of the ends tied to nothing, which move when the arrow is moved on its own.
    pub fn free_points_mut(&mut self) -> Vec<&mut [f32; 2]> {
        [&mut self.tail, &mut self.head]
            .into_iter()
            .filter_map(|e| match e {
                CurlyEnd::Point(p) => Some(p),
                _ => None,
            })
            .collect()
    }
}

/// The sweep a new arrow bends by, in degrees (before its side is chosen).
pub const DEFAULT_CURLY_ANGLE: f32 = 120.0;
/// The most an arc may turn, either way, in degrees: past it the arc nearly closes on itself.
pub const MAX_CURLY_ANGLE: f32 = 300.0;
/// The most an arc may lean ([`CurlyArrow::skew`]).
pub const MAX_CURLY_SKEW: f32 = 0.8;
/// Where on a bond a tail stands: this share of the bond's length from its `side` atom.
pub const BOND_TAIL_AT: f32 = 0.4;

// Clearances, in molecule units (a bond is 1.5).
/// Beyond a label's edge: to the middle of the lone pair, to where the tail leaves, and to a
/// head's tip.
const DOTS_PAST_LABEL: f32 = 0.1;
const TAIL_PAST_LABEL: f32 = 0.3;
const HEAD_PAST_LABEL: f32 = 0.12;
/// The same from the centre of an atom shown as a bare vertex.
const DOTS_BARE: f32 = 0.26;
const TAIL_BARE: f32 = 0.44;
const HEAD_GAP_BARE: f32 = 0.34;
/// How far a tail on a bond is lifted off its line, and how far short of a bond's (or a gap's)
/// middle a head stops.
const BOND_TAIL_LIFT: f32 = 0.1;
const BOND_HEAD_GAP: f32 = 0.16;
const BETWEEN_HEAD_GAP: f32 = 0.08;
/// A head atom nearer its tail's anchor than this lands beside the atom (see [`geometry`]).
const CLOSE_HEAD: f32 = 1.1;
/// How many straight pieces an arc is drawn in, and each piece of a path.
const CURVE_STEPS: usize = 48;
const PIECE_STEPS: usize = 32;
/// The size of a label's letters, in molecule units: a 10 pt font beside a 14.4 pt bond is
/// 1.04 units, its capitals about 0.75 high; a point is 1/9.6 of a unit.
const LABEL_HALF_HEIGHT: f32 = 0.38;
const UNITS_PER_POINT: f32 = 1.5 / 14.4;

/// `angle` kept within ±[`MAX_CURLY_ANGLE`] (and a number).
pub fn clamp_angle(angle: f32) -> f32 {
    if angle.is_finite() { angle.clamp(-MAX_CURLY_ANGLE, MAX_CURLY_ANGLE) } else { DEFAULT_CURLY_ANGLE }
}

/// `skew` kept within ±[`MAX_CURLY_SKEW`].
pub fn clamp_skew(skew: f32) -> f32 {
    if skew.is_finite() { skew.clamp(-MAX_CURLY_SKEW, MAX_CURLY_SKEW) } else { 0.0 }
}

/// Where a curly arrow is drawn, in molecule units.
#[derive(Debug, Clone, PartialEq)]
pub struct CurlyPath {
    /// Where the shaft starts, and where the head's tip is.
    pub tail: [f32; 2],
    pub head: [f32; 2],
    /// The curve from `tail` to `head`, in order.
    pub points: Vec<[f32; 2]>,
    /// The lone pair the tail starts from, when it is drawn: the middle of the pair, and the
    /// direction across it (the two dots stand either side of the middle along it).
    pub dots: Option<([f32; 2], [f32; 2])>,
}

impl CurlyPath {
    /// The middle of the curve: where the handle that bends an arc is.
    pub fn middle(&self) -> [f32; 2] {
        self.points[self.points.len() / 2]
    }

    /// Which side of the curve's last turn is its outside: `Some(true)` when it is the left of
    /// the way the curve goes (on a y-down page), `Some(false)` the right, `None` when the end
    /// runs straight. A fish-hook's barb goes there.
    pub fn outer_is_left(&self) -> Option<bool> {
        outer_is_left(&self.points)
    }
}

/// Whether a fish-hook `arrow` drawn along `path` has its barb on the left of the way it ends:
/// on the outside of its last turn, or for an end running straight, the outside of its arc.
pub fn barb_left(arrow: &CurlyArrow, path: &CurlyPath) -> bool {
    // A positive angle bends the arc out to the right, its outside.
    path.outer_is_left().unwrap_or(arrow.angle <= 0.0)
}

/// See [`CurlyPath::outer_is_left`]; `points` is a curve in order, in any units.
pub fn outer_is_left(points: &[[f32; 2]]) -> Option<bool> {
    let n = points.len();
    if n < 3 {
        return None;
    }
    // The turn over the last fifth of the curve.
    let step = ((n - 1) / 5).max(1);
    let k = n - 1 - step;
    let a = sub(points[k], points[k.saturating_sub(step)]);
    let b = sub(points[n - 1], points[k]);
    let cross = a[0] * b[1] - a[1] * b[0];
    // On a y-down page a positive cross product turns right, so the outside is the left.
    (cross.abs() > 1e-6 * len(a) * len(b)).then_some(cross > 0.0)
}

pub(crate) fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

pub(crate) fn add_scaled(a: [f32; 2], d: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] + d[0] * k, a[1] + d[1] * k]
}

pub(crate) fn len(d: [f32; 2]) -> f32 {
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

fn unit(d: [f32; 2]) -> Option<[f32; 2]> {
    let l = len(d);
    (l > 1e-6).then(|| [d[0] / l, d[1] / l])
}

/// The point `p` (`[along, across]` in the frame of the line from `from` to `to`, see
/// [`CurlyArrow::path`]) on the page.
pub fn from_frame(from: [f32; 2], to: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let u = sub(to, from);
    let right = [-u[1], u[0]];
    [from[0] + u[0] * p[0] + right[0] * p[1], from[1] + u[1] * p[0] + right[1] * p[1]]
}

/// The page point `p` in the frame of the line from `from` to `to`; `None` when the two are one
/// point.
pub fn to_frame(from: [f32; 2], to: [f32; 2], p: [f32; 2]) -> Option<[f32; 2]> {
    let u = sub(to, from);
    let l2 = u[0] * u[0] + u[1] * u[1];
    if l2 < 1e-10 {
        return None;
    }
    let d = sub(p, from);
    Some([(d[0] * u[0] + d[1] * u[1]) / l2, (-d[0] * u[1] + d[1] * u[0]) / l2])
}

/// The point at `t` (0 at `p`, 1 at `q`) of the arc from `p` to `q` turning `angle` degrees
/// (ChemDraw's sign: positive bends right of the way from `p` to `q` on a y-down page), leaning
/// by `skew`. A plain circular arc, its points slid along the chord by `skew` times a sine bump,
/// so the ends stay put and the middle moves `skew` × half the chord.
pub fn curve_point(p: [f32; 2], q: [f32; 2], angle: f32, skew: f32, t: f32) -> [f32; 2] {
    let d = sub(q, p);
    let c = len(d);
    if c < 1e-6 {
        return p;
    }
    let u = [d[0] / c, d[1] / c];
    let theta = angle.to_radians();
    let base = if theta.abs() < 1e-3 {
        add_scaled(p, d, t)
    } else {
        // Right of the way from p to q on a y-down page.
        let right = [-u[1], u[0]];
        let bulge = if theta > 0.0 { right } else { [-right[0], -right[1]] };
        let half = theta.abs() / 2.0;
        let radius = c / (2.0 * half.sin());
        let mid = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
        // Opposite the bulge for less than half a turn; past it, on the bulge's side.
        let center = add_scaled(mid, bulge, -radius * half.cos());
        let a0 = (p[1] - center[1]).atan2(p[0] - center[0]);
        // A positive ChemDraw angle turns counterclockwise on the page, which is a negative
        // turn in y-down coordinates.
        let a = a0 - theta * t;
        [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
    };
    add_scaled(base, u, skew * c / 2.0 * (std::f32::consts::PI * t).sin())
}

/// The point at `t` of the cubic Bézier piece `b`.
pub fn bezier_point(b: &[[f32; 2]; 4], t: f32) -> [f32; 2] {
    let s = 1.0 - t;
    let (w0, w1, w2, w3) = (s * s * s, 3.0 * s * s * t, 3.0 * s * t * t, t * t * t);
    [
        b[0][0] * w0 + b[1][0] * w1 + b[2][0] * w2 + b[3][0] * w3,
        b[0][1] * w0 + b[1][1] * w1 + b[2][1] * w2 + b[3][1] * w3,
    ]
}

/// The angle and skew that make the arc from `p` to `q` pass through `through` at its middle:
/// what dragging an arc's handle there does.
pub fn bend_through(p: [f32; 2], q: [f32; 2], through: [f32; 2]) -> Option<(f32, f32)> {
    let d = sub(q, p);
    let c = len(d);
    if c < 1e-4 {
        return None;
    }
    let u = [d[0] / c, d[1] / c];
    let right = [-u[1], u[0]];
    let mid = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
    let x = sub(through, mid);
    let across = x[0] * right[0] + x[1] * right[1];
    let along = x[0] * u[0] + x[1] * u[1];
    // The sagitta of an arc turning θ over chord c is c/2 · tan(θ/4).
    let angle = across.signum() * 4.0 * (2.0 * across.abs() / c).atan().to_degrees();
    Some((clamp_angle(angle), clamp_skew(2.0 * along / c)))
}

/// Whether atom `id` shows a label, so arrows keep clear of it.
fn labelled(mol: &Molecule, id: u32) -> bool {
    super::depict::should_show_label(mol, id)
}

/// How far from atom `id`'s centre its label reaches in direction `dir` (a unit vector): to the
/// edge of the box its letters fill, estimated from Arial's widths as a ChemDraw file's labels
/// are, the box placed as the label is drawn (its element symbol centred on the atom). `None`
/// for an atom that shows no label.
pub fn label_reach(mol: &Molecule, id: u32, dir: [f32; 2]) -> Option<f32> {
    if !labelled(mol, id) {
        return None;
    }
    let atom = mol.atom_by_id(id)?;
    let label = super::depict::atom_label(mol, atom);
    // Each character's width in points, in order, as the label sets them.
    let mut widths: Vec<f32> = Vec::new();
    for span in &label.rich.spans {
        let script = if span.style.script == crate::widget::rich_text::Script::Normal { 1.0 } else { 0.7 };
        let size = span.style.size.unwrap_or(crate::widget::rich_text::BASE_POINTS) / 10.0;
        widths.extend(span.text.chars().map(|c| (super::cdx::arial_width(c) / super::cdx::PT) as f32 * script * size));
    }
    let before: f32 = widths.iter().take(label.anchor_chars.start).sum();
    let anchor: f32 = widths.iter().skip(label.anchor_chars.start).take(label.anchor_chars.len()).sum();
    let total: f32 = widths.iter().sum();
    let centre = before + anchor / 2.0;
    let (left, right) = (centre * UNITS_PER_POINT, (total - centre) * UNITS_PER_POINT);
    let reach_x = if dir[0] > 1e-6 { right / dir[0] } else if dir[0] < -1e-6 { left / -dir[0] } else { f32::INFINITY };
    let reach_y = if dir[1].abs() > 1e-6 { LABEL_HALF_HEIGHT / dir[1].abs() } else { f32::INFINITY };
    Some(reach_x.min(reach_y))
}

/// The point an end is laid on before it is pulled off it: an atom's centre, the point of a
/// bond toward its side atom (a tail) or its middle (a head), the middle of a gap, a free point.
fn anchor(mol: &Molecule, end: &CurlyEnd, tail: bool) -> Option<[f32; 2]> {
    let pos = |id: u32| mol.atom_by_id(id).map(|a| a.pos);
    Some(match *end {
        CurlyEnd::Atom(a) => pos(a)?,
        CurlyEnd::Bond { bond, side } => {
            let b = mol.bond_by_id(bond)?;
            let (p, q) = (pos(b.begin)?, pos(b.end)?);
            if tail {
                let (near, far) = if side == b.end { (q, p) } else { (p, q) };
                add_scaled(near, sub(far, near), BOND_TAIL_AT)
            } else {
                [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]
            }
        }
        CurlyEnd::Between(a, b) => {
            let (p, q) = (pos(a)?, pos(b)?);
            [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]
        }
        CurlyEnd::Point(p) => p,
    })
}

/// The two anchors of `arrow` on `mol`, tail first: what its path's frame is laid between.
pub fn anchors(mol: &Molecule, arrow: &CurlyArrow) -> Option<([f32; 2], [f32; 2])> {
    let at = anchor(mol, &arrow.tail, true)?;
    let ah = anchor(mol, &arrow.head, false)?;
    (len(sub(ah, at)) >= 1e-4).then_some((at, ah))
}

/// How far a tail on an atom leaves in direction `dir`: to the middle of its lone pair, and to
/// where the shaft starts.
fn tail_room(mol: &Molecule, a: u32, dir: [f32; 2]) -> (f32, f32) {
    match label_reach(mol, a, dir) {
        Some(r) => (r + DOTS_PAST_LABEL, r + TAIL_PAST_LABEL),
        None => (DOTS_BARE, TAIL_BARE),
    }
}

/// How far short of atom `a`'s centre a head coming in from direction `from` (a unit vector
/// from the atom toward the arrow) stops.
fn head_room(mol: &Molecule, a: u32, from: [f32; 2]) -> f32 {
    label_reach(mol, a, from).map_or(HEAD_GAP_BARE, |r| r + HEAD_PAST_LABEL)
}

/// The cubic pieces of `arrow`'s free shape on the page, from the tail anchor to the head
/// anchor; `None` for an arc, or when the arrow cannot be placed.
pub fn pieces(mol: &Molecule, arrow: &CurlyArrow) -> Option<Vec<[[f32; 2]; 4]>> {
    if !arrow.is_shaped() {
        return None;
    }
    let (at, ah) = anchors(mol, arrow)?;
    Some(pieces_between(at, ah, &arrow.path))
}

/// The cubic pieces of `path` (see [`CurlyArrow::path`]) laid between `at` and `ah`.
pub fn pieces_between(at: [f32; 2], ah: [f32; 2], path: &[[f32; 2]]) -> Vec<[[f32; 2]; 4]> {
    let abs: Vec<[f32; 2]> = path.iter().map(|p| from_frame(at, ah, *p)).collect();
    let n = abs.len().div_ceil(3);
    (0..n)
        .map(|i| {
            let start = if i == 0 { at } else { abs[3 * i - 1] };
            let end = if i + 1 == n { ah } else { abs[3 * i + 2] };
            [start, abs[3 * i], abs[3 * i + 1], end]
        })
        .collect()
}

/// The free shape (see [`CurlyArrow::path`]) that draws the arc from `at` to `ah` turning
/// `angle` with `skew`: one cubic piece for a turn up to 150°, two (meeting at the arc's middle)
/// for more, each piece's control points along the arc at its ends, as far out as makes a cubic
/// follow a circular arc.
pub fn arc_path(at: [f32; 2], ah: [f32; 2], angle: f32, skew: f32) -> Vec<[f32; 2]> {
    let n = if angle.abs() > 150.0 { 2 } else { 1 };
    let pt = |t: f32| curve_point(at, ah, angle, skew, t);
    let dir = |t: f32| unit(sub(pt((t + 0.005).min(1.0)), pt((t - 0.005).max(0.0)))).unwrap_or([1.0, 0.0]);
    let mut out = Vec::new();
    for i in 0..n {
        let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
        let (p0, p1) = (pt(t0), pt(t1));
        let sweep = (angle / n as f32).abs().to_radians();
        let chord = len(sub(p1, p0));
        // A cubic through an arc of sweep φ: handles of 4/3 · tan(φ/4) · radius.
        let h = if sweep < 1e-3 { chord / 3.0 } else { 4.0 / 3.0 * (sweep / 4.0).tan() * chord / (2.0 * (sweep / 2.0).sin()) };
        out.push(add_scaled(p0, dir(t0), h));
        out.push(add_scaled(p1, dir(t1), -h));
        if i + 1 < n {
            out.push(p1);
        }
    }
    out.into_iter().map(|p| to_frame(at, ah, p).unwrap_or([0.5, 0.0])).collect()
}

/// Give `arrow` the free shape that draws its arc, unless it has one already: what editing a
/// control point of an arc starts from. False when the arrow cannot be placed.
pub fn shape_from_arc(mol: &Molecule, arrow: &mut CurlyArrow) -> bool {
    if arrow.is_shaped() {
        return true;
    }
    let Some((at, ah)) = anchors(mol, arrow) else { return false };
    arrow.path = arc_path(at, ah, clamp_angle(arrow.angle), clamp_skew(arrow.skew));
    true
}

/// `points` (a polyline) with `from_start` cut off its start and `from_end` off its end, by
/// length along it.
fn trim(points: &[[f32; 2]], from_start: f32, from_end: f32) -> Vec<[f32; 2]> {
    let cut_start = |pts: &[[f32; 2]], cut: f32| -> Vec<[f32; 2]> {
        let mut left = cut;
        for k in 1..pts.len() {
            let step = len(sub(pts[k], pts[k - 1]));
            if step >= left && step > 0.0 {
                let mut out = vec![add_scaled(pts[k - 1], sub(pts[k], pts[k - 1]), left / step)];
                out.extend_from_slice(&pts[k..]);
                return out;
            }
            left -= step;
        }
        pts[pts.len() - 1..].to_vec()
    };
    let mut out = cut_start(points, from_start);
    out.reverse();
    let mut out = cut_start(&out, from_end);
    out.reverse();
    out
}

/// The point `at` along polyline `points` from its start, and the way it runs there.
fn along(points: &[[f32; 2]], at: f32) -> ([f32; 2], [f32; 2]) {
    let mut left = at;
    for k in 1..points.len() {
        let d = sub(points[k], points[k - 1]);
        let step = len(d);
        if step >= left && step > 0.0 {
            return (add_scaled(points[k - 1], d, left / step), unit(d).unwrap_or([1.0, 0.0]));
        }
        left -= step;
    }
    (points[points.len() - 1], [1.0, 0.0])
}

/// Where `arrow` is drawn on `mol`; `None` when one of its atoms or bonds is missing, or its
/// two ends fall on one point.
pub fn geometry(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyPath> {
    if arrow.is_shaped() {
        return shaped_geometry(mol, arrow);
    }
    let (at, ah) = anchors(mol, arrow)?;
    let (angle, skew) = (clamp_angle(arrow.angle), clamp_skew(arrow.skew));
    // The way the arc laid between the anchors leaves the tail and comes into the head.
    let leave = unit(sub(curve_point(at, ah, angle, skew, 0.01), at))?;
    let arrive = unit(sub(ah, curve_point(at, ah, angle, skew, 0.99)))?;
    let mut dots = None;
    let tail = match arrow.tail {
        CurlyEnd::Atom(a) => {
            let (d, t) = tail_room(mol, a, leave);
            if arrow.dots {
                dots = Some((add_scaled(at, leave, d), [-leave[1], leave[0]]));
            }
            add_scaled(at, leave, t)
        }
        CurlyEnd::Bond { .. } => add_scaled(at, leave, BOND_TAIL_LIFT),
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => at,
    };
    let close = len(sub(ah, at)) < CLOSE_HEAD;
    let mut head = match arrow.head {
        // An atom right by the tail (a bond's electrons going onto one of its own atoms): come
        // down beside it on the side the arc bends to, rather than stop short of it, which
        // would leave next to no arc.
        CurlyEnd::Atom(a) if close => {
            let u = unit(sub(ah, at))?;
            let side = if angle >= 0.0 { [-u[1], u[0]] } else { [u[1], -u[0]] };
            add_scaled(ah, side, head_room(mol, a, side))
        }
        CurlyEnd::Atom(a) => add_scaled(ah, arrive, -head_room(mol, a, [-arrive[0], -arrive[1]])),
        CurlyEnd::Bond { .. } => add_scaled(ah, arrive, -BOND_HEAD_GAP),
        CurlyEnd::Between(..) => add_scaled(ah, arrive, -BETWEEN_HEAD_GAP),
        CurlyEnd::Point(_) => ah,
    };
    let mut tail = tail;
    // Ends so close that pulling them off their anchors would cross them: draw between the
    // anchors themselves.
    if len(sub(head, tail)) < 0.2 {
        (tail, head) = (at, ah);
        dots = None;
    }
    let points = (0..=CURVE_STEPS).map(|k| curve_point(tail, head, angle, skew, k as f32 / CURVE_STEPS as f32)).collect();
    Some(CurlyPath { tail, head, points, dots })
}

/// The full curve of an arrow with a free shape, anchor to anchor, as a polyline.
fn full_curve(pieces: &[[[f32; 2]; 4]]) -> Vec<[f32; 2]> {
    let mut pts = vec![pieces[0][0]];
    for b in pieces {
        pts.extend((1..=PIECE_STEPS).map(|k| bezier_point(b, k as f32 / PIECE_STEPS as f32)));
    }
    pts
}

/// Where a lone pair's dots go: their middle, and the direction across them.
type Dots = Option<([f32; 2], [f32; 2])>;

/// How much of `full` (an arrow's whole curve, anchor to anchor) is cut off its start and its
/// end, measured the way it leaves and arrives, and where the lone pair's dots go. Nothing is
/// cut from a curve too short to cut both ends back.
fn cuts(mol: &Molecule, arrow: &CurlyArrow, full: &[[f32; 2]]) -> (f32, f32, Dots) {
    let total: f32 = full.windows(2).map(|w| len(sub(w[1], w[0]))).sum();
    let probe = 0.05_f32.min(total / 4.0);
    let leave = along(full, probe).1;
    let arrive = {
        let mut rev = full.to_vec();
        rev.reverse();
        let d = along(&rev, probe).1;
        [-d[0], -d[1]]
    };
    let mut dots = None;
    let start = match arrow.tail {
        CurlyEnd::Atom(a) => {
            let (d, t) = tail_room(mol, a, leave);
            if arrow.dots && d < total {
                let (at, dir) = along(full, d);
                dots = Some((at, [-dir[1], dir[0]]));
            }
            t
        }
        CurlyEnd::Bond { .. } => BOND_TAIL_LIFT,
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => 0.0,
    };
    let end = match arrow.head {
        CurlyEnd::Atom(a) => head_room(mol, a, [-arrive[0], -arrive[1]]),
        CurlyEnd::Bond { .. } => BOND_HEAD_GAP,
        CurlyEnd::Between(..) => BETWEEN_HEAD_GAP,
        CurlyEnd::Point(_) => 0.0,
    };
    if start + end + 0.2 < total { (start, end, dots) } else { (0.0, 0.0, None) }
}

/// [`geometry`] of an arrow with a free shape: its pieces drawn from anchor to anchor, cut back
/// at each end by the room the end needs.
fn shaped_geometry(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyPath> {
    let full = full_curve(&pieces(mol, arrow)?);
    let total: f32 = full.windows(2).map(|w| len(sub(w[1], w[0]))).sum();
    if total < 1e-4 {
        return None;
    }
    let (start, end, dots) = cuts(mol, arrow, &full);
    let points = trim(&full, start, end);
    if points.len() < 2 {
        return None;
    }
    let (tail, head) = (points[0], points[points.len() - 1]);
    Some(CurlyPath { tail, head, points, dots })
}

/// The two halves of cubic `b` at `t`.
fn split(b: &[[f32; 2]; 4], t: f32) -> ([[f32; 2]; 4], [[f32; 2]; 4]) {
    let lerp = |p: [f32; 2], q: [f32; 2]| add_scaled(p, sub(q, p), t);
    let (p01, p12, p23) = (lerp(b[0], b[1]), lerp(b[1], b[2]), lerp(b[2], b[3]));
    let (p012, p123) = (lerp(p01, p12), lerp(p12, p23));
    let mid = lerp(p012, p123);
    ([b[0], p01, p012, mid], [mid, p123, p23, b[3]])
}

/// The parameter of cubic `b` at length `at` along it from its start (1 when it is shorter).
fn param_at(b: &[[f32; 2]; 4], at: f32) -> f32 {
    let steps = PIECE_STEPS * 2;
    let mut done = 0.0;
    let mut prev = b[0];
    for k in 1..=steps {
        let t = k as f32 / steps as f32;
        let p = bezier_point(b, t);
        let step = len(sub(p, prev));
        if done + step >= at && step > 0.0 {
            return t - (1.0 - (at - done) / step) / steps as f32;
        }
        done += step;
        prev = p;
    }
    1.0
}

/// The cubic pieces of an arrow with a free shape as it is drawn: cut back at each end as
/// [`geometry`] cuts its curve, so a file's Bézier curve draws what this editor draws. `None`
/// for an arc, or an arrow that cannot be placed.
pub fn drawn_pieces(mol: &Molecule, arrow: &CurlyArrow) -> Option<Vec<[[f32; 2]; 4]>> {
    let mut pieces = pieces(mol, arrow)?;
    let (start, end, _) = cuts(mol, arrow, &full_curve(&pieces));
    if start > 0.0 {
        let t = param_at(&pieces[0], start);
        pieces[0] = split(&pieces[0], t.min(0.95)).1;
    }
    if end > 0.0 {
        let last = pieces.len() - 1;
        let b = pieces[last];
        let reversed = [b[3], b[2], b[1], b[0]];
        let kept = split(&reversed, param_at(&reversed, end).min(0.95)).1;
        pieces[last] = [kept[3], kept[2], kept[1], kept[0]];
    }
    Some(pieces)
}

/// The free shape (see [`CurlyArrow::path`]) of cubic `pieces` running from `from` to `to`:
/// their inner control points and knots in the frame of that line.
pub fn path_of(from: [f32; 2], to: [f32; 2], pieces: &[[[f32; 2]; 4]]) -> Option<Vec<[f32; 2]>> {
    let mut out = Vec::new();
    for (i, b) in pieces.iter().enumerate() {
        out.push(to_frame(from, to, b[1])?);
        out.push(to_frame(from, to, b[2])?);
        if i + 1 < pieces.len() {
            out.push(to_frame(from, to, b[3])?);
        }
    }
    Some(out)
}

/// The handles a selected curly arrow is edited by: its ends, which can be dragged onto
/// something else; the knots between the pieces of its path; each piece's two control points,
/// with the point each one is the tangent from; and, for an arc, the point in its middle that
/// bends it. An arc's control points are those of the path it would become ([`arc_path`]):
/// dragging one gives it that shape.
#[derive(Debug, Clone, PartialEq)]
pub struct CurlyHandles {
    pub tail: [f32; 2],
    pub head: [f32; 2],
    /// The knots: their index in [`CurlyArrow::path`], and where they are.
    pub knots: Vec<(usize, [f32; 2])>,
    /// The control points: their index in the path, where they are, and the end of the piece
    /// they pull on (the knot or anchor their tangent line runs to).
    pub controls: Vec<(usize, [f32; 2], [f32; 2])>,
    /// The middle of an arc (not of a free shape).
    pub bend: Option<[f32; 2]>,
}

/// [`CurlyHandles`] of `arrow` on `mol`.
pub fn handles(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyHandles> {
    let drawn = geometry(mol, arrow)?;
    let (at, ah) = anchors(mol, arrow)?;
    let path = if arrow.is_shaped() { arrow.path.clone() } else { arc_path(at, ah, clamp_angle(arrow.angle), clamp_skew(arrow.skew)) };
    let pieces = pieces_between(at, ah, &path);
    let mut knots = Vec::new();
    let mut controls = Vec::new();
    for (i, b) in pieces.iter().enumerate() {
        controls.push((3 * i, b[1], b[0]));
        controls.push((3 * i + 1, b[2], b[3]));
        if i + 1 < pieces.len() {
            knots.push((3 * i + 2, b[3]));
        }
    }
    let bend = (!arrow.is_shaped()).then(|| drawn.middle());
    Some(CurlyHandles { tail: drawn.tail, head: drawn.head, knots, controls, bend })
}

/// Move point `index` of `arrow`'s path (see [`CurlyArrow::path`]) to page point `to`, an arc
/// first taking the shape that draws it. A knot takes its two control points with it; a control
/// point next to a knot turns the one across the knot with it, keeping that one's length, so
/// the path stays smooth there. False when nothing could be moved.
pub fn move_path_point(mol: &Molecule, arrow: &mut CurlyArrow, index: usize, to: [f32; 2]) -> bool {
    if !shape_from_arc(mol, arrow) {
        return false;
    }
    let Some((at, ah)) = anchors(mol, arrow) else { return false };
    let Some(to) = to_frame(at, ah, to) else { return false };
    let path = &mut arrow.path;
    if index >= path.len() {
        return false;
    }
    if index % 3 == 2 {
        let d = sub(to, path[index]);
        for k in [index - 1, index, index + 1] {
            path[k] = add_scaled(path[k], d, 1.0);
        }
        return true;
    }
    path[index] = to;
    // The control point across the knot, when there is one.
    let (knot, other) = match index % 3 {
        1 if index + 2 < path.len() => (index + 1, index + 2),
        0 if index >= 3 => (index - 1, index - 2),
        _ => return true,
    };
    let k = path[knot];
    let keep = len(sub(path[other], k));
    if let Some(dir) = unit(sub(to, k)) {
        path[other] = add_scaled(k, dir, -keep);
    }
    true
}

/// Split `arrow`'s path where it passes nearest page point `near`, adding a knot there (an arc
/// first taking the shape that draws it). The path's shape does not change. Returns the knot's
/// index in the path.
pub fn insert_knot(mol: &Molecule, arrow: &mut CurlyArrow, near: [f32; 2]) -> Option<usize> {
    if !shape_from_arc(mol, arrow) {
        return None;
    }
    let (at, ah) = anchors(mol, arrow)?;
    let pieces = pieces_between(at, ah, &arrow.path);
    // The piece and parameter nearest `near`.
    let (mut best, mut best_d) = ((0, 0.5), f32::INFINITY);
    for (i, b) in pieces.iter().enumerate() {
        for k in 1..64 {
            let t = k as f32 / 64.0;
            let d = len(sub(bezier_point(b, t), near));
            if d < best_d {
                (best, best_d) = ((i, t), d);
            }
        }
    }
    let (i, t) = best;
    // de Casteljau: the two halves of piece i at t.
    let (left, right) = split(&pieces[i], t);
    let frame = |p: [f32; 2]| to_frame(at, ah, p).unwrap_or([0.5, 0.0]);
    let new = [frame(left[1]), frame(left[2]), frame(left[3]), frame(right[1]), frame(right[2])];
    arrow.path.splice(3 * i..3 * i + 2, new);
    Some(3 * i + 2)
}

/// Take knot `index` out of `arrow`'s path, joining the pieces either side of it into one that
/// keeps their outer control points. False when `index` is no knot.
pub fn remove_knot(arrow: &mut CurlyArrow, index: usize) -> bool {
    if !arrow.is_shaped() || index % 3 != 2 || index + 1 >= arrow.path.len() {
        return false;
    }
    arrow.path.drain(index - 1..index + 2);
    true
}

/// Which way a new arrow from `tail` to `head` should bend: away from the atoms around it, so
/// it does not run over the structure. [`DEFAULT_CURLY_ANGLE`] with the sign that bends that way.
pub fn default_angle(mol: &Molecule, tail: &CurlyEnd, head: &CurlyEnd) -> f32 {
    let (Some(at), Some(ah)) = (anchor(mol, tail, true), anchor(mol, head, false)) else { return DEFAULT_CURLY_ANGLE };
    let d = sub(ah, at);
    let c = len(d);
    if c < 1e-4 {
        return DEFAULT_CURLY_ANGLE;
    }
    let right = [-d[1] / c, d[0] / c];
    let mid = [(at[0] + ah[0]) / 2.0, (at[1] + ah[1]) / 2.0];
    let reach = c / 2.0 + 1.5;
    let weight: f32 = mol
        .atoms
        .iter()
        .map(|a| sub(a.pos, mid))
        .filter(|x| len(*x) <= reach)
        .map(|x| x[0] * right[0] + x[1] * right[1])
        .sum();
    // Positive bends right: do so unless the atoms are on the right.
    if weight > 1e-3 { -DEFAULT_CURLY_ANGLE } else { DEFAULT_CURLY_ANGLE }
}

impl Molecule {
    /// Add a curly arrow as it is, giving it a fresh id; returns the id.
    pub fn add_curly_arrow(&mut self, arrow: CurlyArrow) -> u32 {
        self.push_object(super::PageObject::Curly(arrow))
    }

    /// The curly arrows on the page.
    pub fn curly_arrows(&self) -> impl Iterator<Item = &CurlyArrow> {
        self.objects.iter().filter_map(super::PageObject::as_curly)
    }

    /// Let go of the arrow ends that stand on atom `atom` (or a bond of it), before it is
    /// removed: each becomes a free point where it is drawn now.
    pub(crate) fn detach_curly_from_atom(&mut self, atom: u32) {
        self.detach_curly(|mol, end| end.loses_atom(mol, atom));
    }

    /// The same for bond `bond`.
    pub(crate) fn detach_curly_from_bond(&mut self, bond: u32) {
        self.detach_curly(|_, end| end.loses_bond(bond));
    }

    fn detach_curly(&mut self, loses: impl Fn(&Molecule, &CurlyEnd) -> bool) {
        if !self.objects.iter().any(|o| matches!(o, super::PageObject::Curly(_))) {
            return;
        }
        let mut changes = Vec::new();
        for (k, o) in self.objects.iter().enumerate() {
            let super::PageObject::Curly(c) = o else { continue };
            let (tail, head) = (loses(self, &c.tail), loses(self, &c.head));
            if !tail && !head {
                continue;
            }
            // Where it stands now; an arrow already broken keeps its ends as they are.
            if let Some(path) = geometry(self, c) {
                changes.push((k, tail.then_some(path.tail), head.then_some(path.head), pieces(self, c)));
            }
        }
        for (k, tail, head, pieces) in changes {
            let super::PageObject::Curly(c) = &mut self.objects[k] else { continue };
            if let Some(p) = tail {
                c.tail = CurlyEnd::Point(p);
                c.dots = false;
            }
            if let Some(p) = head {
                c.head = CurlyEnd::Point(p);
            }
            // A free shape keeps its control points where they were on the page, in the frame
            // of its new ends.
            if let Some(pieces) = pieces {
                let from = tail.unwrap_or(pieces[0][0]);
                let to = head.unwrap_or(pieces[pieces.len() - 1][3]);
                if let Some(path) = path_of(from, to, &pieces) {
                    c.path = path;
                }
            }
        }
    }
}

/// The distance from `p` to the segment `a`–`b`, and how far along it (0 at `a`, 1 at `b`) the
/// nearest point is.
fn segment_distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let d = sub(b, a);
    let l2 = d[0] * d[0] + d[1] * d[1];
    if l2 < 1e-12 {
        return (len(sub(p, a)), 0.0);
    }
    let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / l2).clamp(0.0, 1.0);
    (len(sub(p, add_scaled(a, d, t))), t)
}

/// What a free arrow end at `p` most likely stands on, for an arrow read from a file that does
/// not say: the middle part of a bond it lies on, or else the one atom it is clearly next to.
/// `None` when that is not clear (two atoms about as near, or none near at all), so the end
/// stays free rather than being tied to the wrong atom.
pub fn end_near(mol: &Molecule, p: [f32; 2], tail: bool) -> Option<CurlyEnd> {
    // On a bond: within a fifth of a bond of its line, away from both atoms.
    let on_bond = mol
        .bonds
        .iter()
        .filter_map(|b| {
            let (a, c) = (mol.atom_by_id(b.begin)?.pos, mol.atom_by_id(b.end)?.pos);
            let (d, t) = segment_distance(p, a, c);
            (d <= 0.3 && (0.2..=0.8).contains(&t)).then_some((b, d, t))
        })
        .min_by(|x, y| x.1.total_cmp(&y.1));
    if let Some((b, _, t)) = on_bond {
        let side = if t <= 0.5 { b.begin } else { b.end };
        return Some(CurlyEnd::Bond { bond: b.id, side });
    }
    let mut near: Vec<(f32, u32)> = mol.atoms.iter().map(|a| (len(sub(a.pos, p)), a.id)).collect();
    near.sort_by(|x, y| x.0.total_cmp(&y.0));
    let reach = if tail { 1.0 } else { 0.9 };
    match near[..] {
        [(d, id), ..] if d <= reach && near.get(1).is_none_or(|n| n.0 >= d * 1.6) => Some(CurlyEnd::Atom(id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
    }

    #[test]
    fn the_arc_turns_the_way_chemdraw_counts() {
        // ChemDraw's 180° arrow from left to right bends below (y down); -180° above.
        let (p, q) = ([0.0, 0.0], [2.0, 0.0]);
        let below = curve_point(p, q, 180.0, 0.0, 0.5);
        assert!(close(below, [1.0, 1.0]), "{below:?}");
        let above = curve_point(p, q, -180.0, 0.0, 0.5);
        assert!(close(above, [1.0, -1.0]), "{above:?}");
        assert!(close(curve_point(p, q, 120.0, 0.0, 0.0), p) && close(curve_point(p, q, 120.0, 0.3, 1.0), q));
        // A quarter turn's sagitta: c/2 · tan(22.5°).
        let mid = curve_point(p, q, 90.0, 0.0, 0.5);
        assert!((mid[1] - (22.5f32).to_radians().tan()).abs() < 1e-4, "{mid:?}");
        // Skew slides the middle toward the head.
        let skewed = curve_point(p, q, 90.0, 0.5, 0.5);
        assert!((skewed[0] - 1.5).abs() < 1e-4, "{skewed:?}");
    }

    #[test]
    fn bending_through_a_point_finds_that_arc() {
        let (p, q) = ([0.0, 0.0], [2.0, 0.0]);
        for (angle, skew) in [(120.0, 0.0), (-90.0, 0.3), (250.0, -0.4), (20.0, 0.0)] {
            let through = curve_point(p, q, angle, skew, 0.5);
            let (a, s) = bend_through(p, q, through).unwrap();
            assert!((a - angle).abs() < 0.01 && (s - skew).abs() < 1e-4, "{angle} {skew}: {a} {s}");
        }
    }

    #[test]
    fn the_frame_turns_and_stretches_with_its_ends() {
        let p = [0.3, -0.4];
        let abs = from_frame([1.0, 1.0], [3.0, 1.0], p);
        assert!(close(abs, [1.6, 0.2]), "right of a line going right is down: {abs:?}");
        assert!(close(to_frame([1.0, 1.0], [3.0, 1.0], abs).unwrap(), p));
        // Turned a quarter and doubled, the point turns and doubles with it.
        let turned = from_frame([0.0, 0.0], [0.0, 4.0], p);
        assert!(close(turned, [1.6, 1.2]), "{turned:?}");
    }

    #[test]
    fn an_arc_becomes_a_path_that_draws_it() {
        let (at, ah) = ([0.0, 0.0], [2.0, 0.0]);
        for angle in [60.0f32, -120.0, 200.0] {
            let path = arc_path(at, ah, angle, 0.0);
            assert_eq!(path.len(), if angle > 150.0 { 5 } else { 2 });
            let half = (angle.abs() / 2.0).to_radians();
            let radius = 1.0 / half.sin();
            let apex = curve_point(at, ah, angle, 0.0, 0.5);
            let centre = [1.0, apex[1] - apex[1].signum() * radius];
            for b in pieces_between(at, ah, &path) {
                for k in 0..=8 {
                    // Every point of the cubic lies on the arc's circle, within a few thousandths.
                    let p = bezier_point(&b, k as f32 / 8.0);
                    assert!((len(sub(p, centre)) - radius).abs() < 0.01 * radius, "{angle}: {p:?}");
                }
            }
        }
    }

    #[test]
    fn knots_split_without_changing_the_shape_and_join_again() {
        let m = Molecule::default();
        let (at, ah) = ([0.0, 0.0], [3.0, 0.0]);
        let mut a = CurlyArrow::new(CurlyEnd::Point(at), CurlyEnd::Point(ah), 100.0, CurlyKind::Pair);
        a.path = vec![[0.3, 0.8], [0.7, -0.8]];
        let before = geometry(&m, &a).unwrap();
        let k = insert_knot(&m, &mut a, before.middle()).unwrap();
        assert_eq!((k, a.path.len()), (2, 5));
        let after = geometry(&m, &a).unwrap();
        for p in [0.1, 0.33, 0.5, 0.8] {
            let i = (p * (before.points.len() - 1) as f32) as usize;
            let nearest = after.points.iter().map(|q| len(sub(*q, before.points[i]))).fold(f32::INFINITY, f32::min);
            assert!(nearest < 0.03, "{p}: {nearest}");
        }
        // Dragging the knot takes its control points along.
        let knot = from_frame(at, ah, a.path[2]);
        let (c2, c1) = (from_frame(at, ah, a.path[1]), from_frame(at, ah, a.path[3]));
        assert!(move_path_point(&m, &mut a, 2, [knot[0], knot[1] + 1.0]));
        assert!(close(from_frame(at, ah, a.path[2]), [knot[0], knot[1] + 1.0]));
        assert!(close(from_frame(at, ah, a.path[1]), [c2[0], c2[1] + 1.0]) && close(from_frame(at, ah, a.path[3]), [c1[0], c1[1] + 1.0]));
        // Dragging a control point next to the knot turns the other one with it.
        let k = from_frame(at, ah, a.path[2]);
        assert!(move_path_point(&m, &mut a, 1, [k[0] - 1.0, k[1]]));
        let other = from_frame(at, ah, a.path[3]);
        assert!(other[0] > k[0] && (other[1] - k[1]).abs() < 1e-4, "smooth through the knot: {other:?}");
        assert!(remove_knot(&mut a, 2));
        assert_eq!(a.path.len(), 2);
        assert!(!remove_knot(&mut a, 1));
    }

    /// A carbonyl, C1=O2, with C1 bonded to C3.
    fn acetyl() -> (Molecule, [u32; 3], u32) {
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [0.0, 0.0], 0);
        let o = m.add_atom("O".into(), [0.0, -1.5], 0);
        let me = m.add_atom("C".into(), [1.3, 0.75], 0);
        let co = m.add_bond(c, o, BondOrder::Double);
        m.add_bond(c, me, BondOrder::Single);
        (m, [c, o, me], co)
    }

    #[test]
    fn a_label_reaches_as_far_as_its_letters() {
        let (mut m, [c, o, _], _) = acetyl();
        let up = label_reach(&m, o, [0.0, -1.0]).unwrap();
        assert!((up - LABEL_HALF_HEIGHT).abs() < 1e-4);
        let side = label_reach(&m, o, [1.0, 0.0]).unwrap();
        assert!(side > 0.3 && side < 0.5, "half an O: {side}");
        // A wide label reaches farther to its side than up.
        m.atom_by_id_mut(o).unwrap().element = "Br".into();
        assert!(label_reach(&m, o, [1.0, 0.0]).unwrap() > side);
        assert_eq!(label_reach(&m, c, [1.0, 0.0]), None, "a bare carbon has none");
    }

    #[test]
    fn a_lone_pair_tail_leaves_outside_the_label_and_the_head_stops_short() {
        let (mut m, [c, o, _], _) = acetyl();
        let id = m.add_curly_arrow(CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Atom(c), 120.0, CurlyKind::Pair));
        let arrow = m.object_by_id(id).unwrap().as_curly().unwrap().clone();
        let path = geometry(&m, &arrow).unwrap();
        let opos = m.atom_by_id(o).unwrap().pos;
        let from = |p: [f32; 2], id: u32| len(sub(p, m.atom_by_id(id).unwrap().pos));
        let leave = unit(sub(path.tail, opos)).unwrap();
        let reach = label_reach(&m, o, leave).unwrap();
        assert!((from(path.tail, o) - reach - TAIL_PAST_LABEL).abs() < 1e-3, "{path:?}");
        assert!((from(path.head, c) - HEAD_GAP_BARE).abs() < 1e-4, "{path:?}");
        let (dots, across) = path.dots.expect("a lone pair tail shows its dots");
        assert!((from(dots, o) - reach - DOTS_PAST_LABEL).abs() < 1e-3);
        assert!((len(across) - 1.0).abs() < 1e-4);
        // Following the atoms.
        m.atom_by_id_mut(o).unwrap().pos[0] += 3.0;
        m.atom_by_id_mut(c).unwrap().pos[0] += 3.0;
        let moved = geometry(&m, &arrow).unwrap();
        assert!(close(moved.tail, [path.tail[0] + 3.0, path.tail[1]]), "{moved:?}");
    }

    #[test]
    fn a_shaped_path_runs_between_its_anchors_cut_back_at_the_ends_and_follows_them() {
        let (mut m, [c, o, me], co) = acetyl();
        // An S from the C=O bond (on O's side) to the methyl carbon.
        let mut arrow = CurlyArrow::new(CurlyEnd::Bond { bond: co, side: o }, CurlyEnd::Atom(me), 90.0, CurlyKind::Pair);
        arrow.path = vec![[0.4, 0.9], [0.6, -0.9]];
        let path = geometry(&m, &arrow).unwrap();
        let (at, ah) = anchors(&m, &arrow).unwrap();
        assert!((len(sub(path.tail, at)) - BOND_TAIL_LIFT).abs() < 0.02, "{:?} {at:?}", path.tail);
        assert!((len(sub(path.head, ah)) - HEAD_GAP_BARE).abs() < 0.02);
        assert!(path.outer_is_left().is_some());
        // Turned half a turn about C, the path turns with it.
        for id in [c, o, me] {
            let p = &mut m.atom_by_id_mut(id).unwrap().pos;
            *p = [-p[0], -p[1]];
        }
        let turned = geometry(&m, &arrow).unwrap();
        for (p, q) in path.points.iter().zip(&turned.points) {
            assert!(close(*q, [-p[0], -p[1]]), "{p:?} {q:?}");
        }
        // Mirrored, it mirrors.
        let mut mirrored = arrow.clone();
        mirrored.flip();
        assert_eq!(mirrored.path, vec![[0.4, -0.9], [0.6, 0.9]]);
    }

    #[test]
    fn the_drawn_pieces_are_the_drawn_curve() {
        let (m, [_, o, me], co) = acetyl();
        let mut arrow = CurlyArrow::new(CurlyEnd::Bond { bond: co, side: o }, CurlyEnd::Atom(me), 90.0, CurlyKind::Pair);
        arrow.path = vec![[0.2, 0.9], [0.4, 0.0], [0.5, -0.2], [0.6, -0.4], [0.9, -0.8]];
        let drawn = geometry(&m, &arrow).unwrap();
        let pieces = drawn_pieces(&m, &arrow).unwrap();
        assert_eq!(pieces.len(), 2);
        assert!(len(sub(pieces[0][0], drawn.tail)) < 0.01 && len(sub(pieces[1][3], drawn.head)) < 0.01, "{pieces:?} {drawn:?}");
        // A path made of them between their own ends draws the same curve.
        let path = path_of(pieces[0][0], pieces[1][3], &pieces).unwrap();
        let again = pieces_between(pieces[0][0], pieces[1][3], &path);
        for (x, y) in pieces.iter().zip(&again) {
            for k in 0..4 {
                assert!(len(sub(x[k], y[k])) < 1e-4);
            }
        }
    }

    #[test]
    fn handles_show_an_arcs_would_be_controls_and_dragging_one_shapes_it() {
        let (m, [c, o, _], _) = acetyl();
        let mut arrow = CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Atom(c), 120.0, CurlyKind::Pair);
        let h = handles(&m, &arrow).unwrap();
        assert!(h.bend.is_some() && h.knots.is_empty() && h.controls.len() == 2);
        let (index, at, _) = h.controls[0];
        assert!(move_path_point(&m, &mut arrow, index, [at[0] - 1.0, at[1]]));
        assert!(arrow.is_shaped());
        let h = handles(&m, &arrow).unwrap();
        assert!(h.bend.is_none());
        assert!(close(h.controls[0].1, [at[0] - 1.0, at[1]]));
    }

    #[test]
    fn a_bond_tail_stands_toward_its_side_atom() {
        let (m, [c, o, _], co) = acetyl();
        let toward_o = CurlyArrow::new(CurlyEnd::Bond { bond: co, side: o }, CurlyEnd::Atom(o), 120.0, CurlyKind::Pair);
        let toward_c = CurlyArrow { tail: CurlyEnd::Bond { bond: co, side: c }, ..toward_o.clone() };
        let (a, b) = (geometry(&m, &toward_o).unwrap(), geometry(&m, &toward_c).unwrap());
        let y = |p: &CurlyPath| p.tail[1];
        // O is above C (y -1.5): the tail on O's side is higher.
        assert!(y(&a) < -0.75 && y(&b) > -0.75, "{:?} {:?}", a.tail, b.tail);
    }

    #[test]
    fn deleting_an_atom_frees_the_ends_on_it_where_they_were() {
        let (mut m, [_, o, me], co) = acetyl();
        let id = m.add_curly_arrow(CurlyArrow::new(CurlyEnd::Bond { bond: co, side: o }, CurlyEnd::Atom(me), -120.0, CurlyKind::Single));
        let before = geometry(&m, m.object_by_id(id).unwrap().as_curly().unwrap()).unwrap();
        m.remove_atom(me);
        let a = m.object_by_id(id).unwrap().as_curly().unwrap();
        assert!(matches!(a.tail, CurlyEnd::Bond { .. }));
        assert!(matches!(a.head, CurlyEnd::Point(p) if close(p, before.head)), "{a:?}");
        // (The tail leaves its bond along the arc, which now ends a little elsewhere.)
        let between = geometry(&m, a).unwrap();
        assert!(len(sub(between.tail, before.tail)) < 0.05, "{between:?}");
        m.remove_bond(co);
        let a = m.object_by_id(id).unwrap().as_curly().unwrap();
        assert!(matches!(a.tail, CurlyEnd::Point(p) if close(p, between.tail)), "{a:?}");
        // Free at both ends, it is drawn where it was.
        let after = geometry(&m, a).unwrap();
        assert!(close(after.tail, between.tail) && close(after.head, before.head));
    }

    #[test]
    fn a_freed_shaped_arrow_keeps_its_shape_on_the_page() {
        let (mut m, [_, o, me], _) = acetyl();
        let mut arrow = CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Atom(me), 90.0, CurlyKind::Pair);
        arrow.path = vec![[0.2, -0.6], [0.5, 0.4], [0.6, 0.0], [0.7, -0.4], [0.9, 0.5]];
        let id = m.add_curly_arrow(arrow);
        let before = pieces(&m, m.object_by_id(id).unwrap().as_curly().unwrap()).unwrap();
        m.remove_atom(me);
        let a = m.object_by_id(id).unwrap().as_curly().unwrap();
        let after = pieces(&m, a).unwrap();
        // The pieces' control points are where they were; only the last piece ends elsewhere.
        for (x, y) in before.iter().zip(&after) {
            for k in 1..3 {
                assert!(len(sub(x[k], y[k])) < 1e-3, "{x:?} {y:?}");
            }
        }
    }

    #[test]
    fn a_new_arrow_bends_away_from_the_structure() {
        let (m, [c, o, me], _) = acetyl();
        // From O to the methyl carbon: C1 is on the right of the way O → C3 goes (y down)...
        let angle = default_angle(&m, &CurlyEnd::Atom(o), &CurlyEnd::Atom(me));
        let arrow = CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Atom(me), angle, CurlyKind::Pair);
        let path = geometry(&m, &arrow).unwrap();
        // ... so the middle of the arc is farther from C1 than the chord's middle is.
        let cpos = m.atom_by_id(c).unwrap().pos;
        let chord_mid = [(path.tail[0] + path.head[0]) / 2.0, (path.tail[1] + path.head[1]) / 2.0];
        assert!(len(sub(path.middle(), cpos)) > len(sub(chord_mid, cpos)), "{angle}");
    }

    #[test]
    fn ends_are_tied_only_when_that_is_clear() {
        let (m, [c, o, _], co) = acetyl();
        assert_eq!(end_near(&m, [0.05, -0.75], true), Some(CurlyEnd::Bond { bond: co, side: c }));
        assert_eq!(end_near(&m, [-0.6, -1.9], true), Some(CurlyEnd::Atom(o)));
        assert_eq!(end_near(&m, [5.0, 5.0], false), None);
    }
}
