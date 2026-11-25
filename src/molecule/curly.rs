//! Curly arrows: the electron-pushing arrows of a reaction mechanism.
//!
//! A [`CurlyArrow`] is first of all a statement about electrons, and that is what it stores:
//!
//! - its **source** ([`CurlyArrow::from`]): a lone pair of an atom, or a bond — and for a bond,
//!   the atom of it the electrons move onto or through (`onto`);
//! - its **target** ([`CurlyArrow::to`]): the atom the electrons go to (a new bond forms between
//!   it and the lone pair's atom, or the bond's `onto` atom), a bond they move into, the gap
//!   between two atoms, or a free point.
//!
//! The common steps of a mechanism are then ([`CurlyCase`]): a lone pair to an atom (a new
//! bond), a lone pair into a bond, a bond onto one of its own atoms (heterolysis), a bond to a
//! new partner by way of one of its atoms (the π electrons of C=O, carried by O, bonding to an
//! H⁺), and a bond into another bond (an allylic shift).
//!
//! The arrow is **drawn from that meaning** ([`layout`]), afresh each time, so it follows its
//! atoms when they are moved, rotated, mirrored or cleaned up: a lone pair's arrow leaves from
//! its dots on the side facing the target; a bond's leaves it square, from just off its middle
//! and clear of its lines, and an arrow into a bond or a gap comes into it square; an arrow
//! carried by an atom swings round that atom on the outside and comes into its partner along the
//! bond being made; and each bends the way that keeps it clear of the atoms, labels and bonds
//! around it. Only when the user reshapes it by hand does it keep a shape of its own
//! ([`CurlyArrow::path`]), stored relative to its ends so that it, too, follows the atoms;
//! clearing the path gives the automatic shape back.
//!
//! It lives in [`Molecule::objects`](super::Molecule::objects) beside ring fills and brackets,
//! so undo, JSON saving and the shared scene cover it without a path of its own. An end whose
//! atom or bond is deleted becomes a free point where the arrow was drawn ([`CurlyEnd::Point`]).

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

/// One end of a curly arrow: its source ([`CurlyArrow::from`]) or its target
/// ([`CurlyArrow::to`]).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CurlyEnd {
    /// As the source, a lone pair of this atom; as the target, the atom the electrons go to.
    Atom(u32),
    /// A bond. As the source, its electrons, moving onto (or by way of) `onto`, one of its two
    /// atoms; as the target, the bond the electrons go into. `onto` means nothing there.
    Bond {
        bond: u32,
        #[serde(alias = "side")]
        onto: u32,
    },
    /// As the target, the middle between two atoms that are not bonded yet: the bond being made.
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
            CurlyEnd::Bond { bond, onto } => onto == atom || mol.bond_by_id(bond).is_none_or(|b| b.begin == atom || b.end == atom),
            CurlyEnd::Between(a, b) => a == atom || b == atom,
            CurlyEnd::Point(_) => false,
        }
    }

    fn loses_bond(&self, bond_id: u32) -> bool {
        matches!(*self, CurlyEnd::Bond { bond, .. } if bond == bond_id)
    }
}

/// The step of a mechanism a curly arrow shows, read from its roles ([`CurlyArrow::case`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurlyCase {
    /// A lone pair to an atom (or to the gap where a bond to it forms): a new bond.
    LonePairToAtom,
    /// A lone pair into a bond (a π bond forming).
    LonePairToBond,
    /// A bond's electrons onto one of its own atoms: heterolysis.
    BondOntoOwnAtom,
    /// A bond's electrons, carried by `onto`, to a new partner: `onto` bonds to it (the π
    /// electrons of C=O, through O, to H⁺).
    BondViaAtomToPartner,
    /// A bond's electrons into another bond (an allylic shift).
    BondToBond,
    /// An end tied to nothing, or a bond into the gap between two atoms.
    Other,
}

/// An electron-pushing arrow: from its source to its target (see the module docs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurlyArrow {
    pub id: u32,
    /// The source: a lone pair ([`CurlyEnd::Atom`]) or a bond with the atom the electrons move
    /// onto ([`CurlyEnd::Bond`]), or a free point.
    #[serde(alias = "tail")]
    pub from: CurlyEnd,
    /// The target: an atom, a bond, the gap between two atoms, or a free point.
    #[serde(alias = "head")]
    pub to: CurlyEnd,
    /// A shape set by hand, overriding the automatic one: the inner points of a chain of cubic
    /// Bézier pieces from the source's anchor to the target's — each piece's two control
    /// points, with the knot it ends at between one piece and the next
    /// (`[c1, c2, knot, c1, c2, knot, …, c1, c2]`, `3n − 1` points for `n` pieces). Each point is
    /// `[along, across]` in the frame of the line between the two anchors: `along` in lengths of
    /// that line from the source's anchor, `across` the same to its right on the page. Empty: the
    /// arrow is drawn automatically ([`layout`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<[f32; 2]>,
    /// Bend the other way than the automatic layout would.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
    #[serde(default)]
    pub kind: CurlyKind,
    /// Draw the lone pair the arrow starts from, as two dots.
    #[serde(default)]
    pub dots: bool,
    /// The id of the ChemDraw object this was read from (its `arrow` or `curve`); see
    /// [`PageArrow::source`](super::PageArrow::source).
    #[serde(default)]
    pub source: Option<u32>,
}

impl CurlyArrow {
    /// A new arrow from `from` to `to`, drawn automatically. A lone pair shows its dots.
    pub fn new(from: CurlyEnd, to: CurlyEnd, kind: CurlyKind) -> Self {
        let dots = matches!(from, CurlyEnd::Atom(_));
        CurlyArrow { id: 0, from, to, path: Vec::new(), flip: false, kind, dots, source: None }
    }

    /// The atoms the arrow depends on, its source's first.
    pub fn atoms(&self, mol: &Molecule) -> Vec<u32> {
        let mut atoms = self.from.atoms(mol);
        atoms.extend(self.to.atoms(mol));
        atoms
    }

    /// Whether the arrow keeps a shape set by hand ([`path`](Self::path)) rather than the
    /// automatic one. A path of the wrong length is none.
    pub fn is_shaped(&self) -> bool {
        !self.path.is_empty() && self.path.len() % 3 == 2
    }

    /// Bend the other way, as a mirror image does: a hand-set shape mirrored across the line
    /// between its ends, an automatic one laid out on its other side.
    pub fn flip(&mut self) {
        if self.is_shaped() {
            for p in &mut self.path {
                p[1] = -p[1];
            }
        } else {
            self.flip = !self.flip;
        }
    }

    /// The step of a mechanism the arrow shows.
    pub fn case(&self, mol: &Molecule) -> CurlyCase {
        match (self.from, self.to) {
            (CurlyEnd::Atom(_), CurlyEnd::Atom(_) | CurlyEnd::Between(..)) => CurlyCase::LonePairToAtom,
            (CurlyEnd::Atom(_), CurlyEnd::Bond { .. }) => CurlyCase::LonePairToBond,
            (CurlyEnd::Bond { bond, onto }, CurlyEnd::Atom(t)) => {
                let own = mol.bond_by_id(bond).is_some_and(|b| b.begin == t || b.end == t);
                if t == onto || own { CurlyCase::BondOntoOwnAtom } else { CurlyCase::BondViaAtomToPartner }
            }
            (CurlyEnd::Bond { .. }, CurlyEnd::Bond { .. }) => CurlyCase::BondToBond,
            _ => CurlyCase::Other,
        }
    }

    /// The positions of the ends tied to nothing, which move when the arrow is moved on its own.
    pub fn free_points_mut(&mut self) -> Vec<&mut [f32; 2]> {
        [&mut self.from, &mut self.to]
            .into_iter()
            .filter_map(|e| match e {
                CurlyEnd::Point(p) => Some(p),
                _ => None,
            })
            .collect()
    }
}

/// The roles of `arrow` in words, the way a mechanism is read: `C=O → (O) → H` for a bond's
/// electrons carried by O to H, `O: → C` for a lone pair of O bonding to C, `C–Br → (Br)` for
/// heterolysis. Atoms are named by their labels.
pub fn describe(mol: &Molecule, arrow: &CurlyArrow) -> String {
    let name = |id: u32| mol.atom_by_id(id).map_or_else(|| "?".to_string(), |a| a.element.clone());
    let bond = |id: u32| {
        mol.bond_by_id(id).map_or_else(
            || "?".to_string(),
            |b| {
                let order = match b.order {
                    super::BondOrder::Single => "–",
                    super::BondOrder::Double => "=",
                    super::BondOrder::Triple => "≡",
                    super::BondOrder::Quadruple => "≣",
                };
                format!("{}{order}{}", name(b.begin), name(b.end))
            },
        )
    };
    let from = match arrow.from {
        CurlyEnd::Atom(a) => format!("{}:", name(a)),
        CurlyEnd::Bond { bond: b, onto } => format!("{} → ({})", bond(b), name(onto)),
        CurlyEnd::Between(a, b) => format!("{}···{}", name(a), name(b)),
        CurlyEnd::Point(_) => "·".to_string(),
    };
    if arrow.case(mol) == CurlyCase::BondOntoOwnAtom {
        return from;
    }
    let to = match arrow.to {
        CurlyEnd::Atom(a) => name(a),
        CurlyEnd::Bond { bond: b, .. } => bond(b),
        CurlyEnd::Between(a, b) => format!("{}···{}", name(a), name(b)),
        CurlyEnd::Point(_) => "·".to_string(),
    };
    format!("{from} → {to}")
}

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
/// How far a tail on a bond is lifted off its lines, and how far short of a bond's (or a gap's)
/// middle (a bond's lines) a head stops.
const BOND_TAIL_LIFT: f32 = 0.12;
const BOND_HEAD_GAP: f32 = 0.16;
const BETWEEN_HEAD_GAP: f32 = 0.08;
/// Where on a bond the tail of an arrow into a bond or a gap (or of a hand-shaped one) stands:
/// this share of the bond's length from its `onto` atom.
pub const BOND_TAIL_AT: f32 = 0.4;
/// The most an arc square to a bond may turn (more is a loop); the least it turns before it
/// reads as straight rather than curly; and how far off its chord the free end of the curve
/// drawn instead of either turns.
const MAX_TURN: f32 = 270.0;
const FLAT_TURN: f32 = 60.0;
const FLAT_BEND: f32 = 90.0;
/// How many straight pieces an arc is drawn in, and each piece of a path.
const CURVE_STEPS: usize = 48;
const PIECE_STEPS: usize = 32;
/// The size of a label's letters, in molecule units: a 10 pt font beside a 14.4 pt bond is
/// 1.04 units, its capitals about 0.75 high; a point is 1/9.6 of a unit.
const LABEL_HALF_HEIGHT: f32 = 0.38;
const UNITS_PER_POINT: f32 = 1.5 / 14.4;
/// The spacing of a multiple bond's lines, as the style has it by default (2.6 pt).
const BOND_LINE_SPACING: f32 = 2.6 * UNITS_PER_POINT;

/// Where a curly arrow is drawn, in molecule units.
#[derive(Debug, Clone, PartialEq)]
pub struct CurlyPath {
    /// Where the shaft starts, and where the head's tip is.
    pub tail: [f32; 2],
    pub head: [f32; 2],
    /// The curve from `tail` to `head`, in order.
    pub points: Vec<[f32; 2]>,
    /// The lone pair the arrow starts from, when it is drawn: the middle of the pair, and the
    /// direction across it (the two dots stand either side of the middle along it).
    pub dots: Option<([f32; 2], [f32; 2])>,
    /// The cubic pieces that draw the curve (an arc is drawn exactly, from `arc`, and these are
    /// its cubic likeness): what a hand-shaped arrow starts from, and its handles.
    pub pieces: Vec<[[f32; 2]; 4]>,
    /// For a circular arc from `tail` to `head`, how far it turns: ChemDraw's `AngularSize`,
    /// positive counterclockwise on the page. `None` for any other curve.
    pub arc: Option<f32>,
}

impl CurlyPath {
    /// The middle of the curve.
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

/// Whether a fish-hook drawn along `path` has its barb on the left of the way it ends: on the
/// outside of its last turn (the left for an end running straight).
pub fn barb_left(path: &CurlyPath) -> bool {
    path.outer_is_left().unwrap_or(true)
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

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

/// `d` turned a quarter to the right on a y-down page.
fn right_of(d: [f32; 2]) -> [f32; 2] {
    [-d[1], d[0]]
}

/// `d` turned `degrees` toward its right on a y-down page.
fn turned(d: [f32; 2], degrees: f32) -> [f32; 2] {
    let (s, c) = degrees.to_radians().sin_cos();
    let r = right_of(d);
    [d[0] * c + r[0] * s, d[1] * c + r[1] * s]
}

/// The angle from `u` to `d`, in degrees, positive toward the right of `u` on a y-down page.
fn angle_to(u: [f32; 2], d: [f32; 2]) -> f32 {
    dot(d, right_of(u)).atan2(dot(d, u)).to_degrees()
}

/// The point `p` (`[along, across]` in the frame of the line from `from` to `to`, see
/// [`CurlyArrow::path`]) on the page.
pub fn from_frame(from: [f32; 2], to: [f32; 2], p: [f32; 2]) -> [f32; 2] {
    let u = sub(to, from);
    let right = right_of(u);
    [from[0] + u[0] * p[0] + right[0] * p[1], from[1] + u[1] * p[0] + right[1] * p[1]]
}

/// The page point `p` in the frame of the line from `from` to `to`; `None` when the two are one
/// point.
pub fn to_frame(from: [f32; 2], to: [f32; 2], p: [f32; 2]) -> Option<[f32; 2]> {
    let u = sub(to, from);
    let l2 = dot(u, u);
    if l2 < 1e-10 {
        return None;
    }
    let d = sub(p, from);
    Some([dot(d, u) / l2, (-d[0] * u[1] + d[1] * u[0]) / l2])
}

/// The point at `t` (0 at `p`, 1 at `q`) of the circular arc from `p` to `q` turning `angle`
/// degrees (ChemDraw's sign: positive bends right of the way from `p` to `q` on a y-down page).
pub fn curve_point(p: [f32; 2], q: [f32; 2], angle: f32, t: f32) -> [f32; 2] {
    let d = sub(q, p);
    let c = len(d);
    if c < 1e-6 {
        return p;
    }
    let u = [d[0] / c, d[1] / c];
    let theta = angle.to_radians();
    if theta.abs() < 1e-3 {
        return add_scaled(p, d, t);
    }
    let right = right_of(u);
    let bulge = if theta > 0.0 { right } else { [-right[0], -right[1]] };
    let half = theta.abs() / 2.0;
    let radius = c / (2.0 * half.sin());
    let mid = midpoint(p, q);
    // Opposite the bulge for less than half a turn; past it, on the bulge's side.
    let center = add_scaled(mid, bulge, -radius * half.cos());
    let a0 = (p[1] - center[1]).atan2(p[0] - center[0]);
    // A positive ChemDraw angle turns counterclockwise on the page, which is a negative turn
    // in y-down coordinates.
    let a = a0 - theta * t;
    [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
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

/// The cubic pieces that follow the arc from `p` to `q` turning `angle`: one up to 150°, two
/// (meeting at its middle) beyond, each with control points along the arc at its ends, as far
/// out as makes a cubic follow a circle.
pub fn arc_pieces(p: [f32; 2], q: [f32; 2], angle: f32) -> Vec<[[f32; 2]; 4]> {
    let n = if angle.abs() > 150.0 { 2 } else { 1 };
    let pt = |t: f32| curve_point(p, q, angle, t);
    let dir = |t: f32| unit(sub(pt((t + 0.005).min(1.0)), pt((t - 0.005).max(0.0)))).unwrap_or([1.0, 0.0]);
    (0..n)
        .map(|i| {
            let (t0, t1) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
            let (p0, p1) = (pt(t0), pt(t1));
            let sweep = (angle / n as f32).abs().to_radians();
            let chord = len(sub(p1, p0));
            // A cubic through an arc of sweep φ: handles of 4/3 · tan(φ/4) · radius.
            let h = if sweep < 1e-3 { chord / 3.0 } else { 4.0 / 3.0 * (sweep / 4.0).tan() * chord / (2.0 * (sweep / 2.0).sin()) };
            [p0, add_scaled(p0, dir(t0), h), add_scaled(p1, dir(t1), -h), p1]
        })
        .collect()
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
    let (left, right) = label_extent(mol, id)?;
    let reach_x = if dir[0] > 1e-6 { right / dir[0] } else if dir[0] < -1e-6 { left / -dir[0] } else { f32::INFINITY };
    let reach_y = if dir[1].abs() > 1e-6 { LABEL_HALF_HEIGHT / dir[1].abs() } else { f32::INFINITY };
    Some(reach_x.min(reach_y))
}

/// How far atom `id`'s label reaches to its left and to its right, from the atom's centre.
fn label_extent(mol: &Molecule, id: u32) -> Option<(f32, f32)> {
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
    Some((centre * UNITS_PER_POINT, (total - centre) * UNITS_PER_POINT))
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

fn pos(mol: &Molecule, id: u32) -> Option<[f32; 2]> {
    mol.atom_by_id(id).map(|a| a.pos)
}

fn midpoint(p: [f32; 2], q: [f32; 2]) -> [f32; 2] {
    [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0]
}

/// A bond's two ends: its `onto` atom's position first, then the other's.
fn bond_ends(mol: &Molecule, bond: u32, onto: u32) -> Option<([f32; 2], [f32; 2])> {
    let b = mol.bond_by_id(bond)?;
    let (p, q) = (pos(mol, b.begin)?, pos(mol, b.end)?);
    Some(if onto == b.end { (q, p) } else { (p, q) })
}

/// How far the lines of bond `bond` reach from its axis to the side `n` (a unit normal), as
/// they are drawn: a double bond's second line on its inner side, both of a centred one's, a
/// triple bond's outer two.
fn bond_reach(mol: &Molecule, bond: u32, n: [f32; 2]) -> f32 {
    use crate::widget::scene::{chosen_double_placement, double_bond_placement, DoubleBondPlacement};
    let Some(b) = mol.bond_by_id(bond) else { return 0.0 };
    match b.order {
        super::BondOrder::Single => 0.0,
        super::BondOrder::Triple => BOND_LINE_SPACING,
        super::BondOrder::Quadruple => 1.5 * BOND_LINE_SPACING,
        super::BondOrder::Double if b.stereo != super::BondStereo::None => BOND_LINE_SPACING / 2.0,
        super::BondOrder::Double => {
            let (Some(p), Some(q)) = (pos(mol, b.begin), pos(mol, b.end)) else { return 0.0 };
            let Some(right) = unit(sub(q, p)).map(right_of) else { return 0.0 };
            let neighbours: Vec<egui::Pos2> = mol
                .bonds
                .iter()
                .filter(|o| o.id != b.id)
                .filter_map(|o| match (o.begin, o.end) {
                    (x, y) | (y, x) if (x == b.begin || x == b.end) && y != b.begin && y != b.end => pos(mol, y),
                    _ => None,
                })
                .map(|a| egui::pos2(a[0], a[1]))
                .collect();
            let placement = chosen_double_placement(b.double_position)
                .unwrap_or_else(|| double_bond_placement(egui::pos2(p[0], p[1]), egui::pos2(q[0], q[1]), (right[0], right[1]), &neighbours));
            match placement {
                DoubleBondPlacement::Symmetric => BOND_LINE_SPACING / 2.0,
                DoubleBondPlacement::Centered(side) if dot(n, right) * side > 0.0 => BOND_LINE_SPACING,
                DoubleBondPlacement::Centered(_) => 0.0,
            }
        }
    }
}

/// The point the source is laid on: an atom's centre, a point of a bond (`at` of its length
/// from its `onto` atom), a free point.
fn source_anchor(mol: &Molecule, end: &CurlyEnd, at: f32) -> Option<[f32; 2]> {
    Some(match *end {
        CurlyEnd::Atom(a) => pos(mol, a)?,
        CurlyEnd::Bond { bond, onto } => {
            let (near, far) = bond_ends(mol, bond, onto)?;
            add_scaled(near, sub(far, near), at)
        }
        CurlyEnd::Between(a, b) => midpoint(pos(mol, a)?, pos(mol, b)?),
        CurlyEnd::Point(p) => p,
    })
}

/// The point the target is laid on: an atom's centre, a bond's middle, the middle of a gap, a
/// free point.
fn target_anchor(mol: &Molecule, end: &CurlyEnd) -> Option<[f32; 2]> {
    Some(match *end {
        CurlyEnd::Atom(a) => pos(mol, a)?,
        CurlyEnd::Bond { bond, .. } => {
            let b = mol.bond_by_id(bond)?;
            midpoint(pos(mol, b.begin)?, pos(mol, b.end)?)
        }
        CurlyEnd::Between(a, b) => midpoint(pos(mol, a)?, pos(mol, b)?),
        CurlyEnd::Point(p) => p,
    })
}

/// The two anchors of `arrow` on `mol`, source first: what a hand-set shape's frame is laid
/// between.
pub fn anchors(mol: &Molecule, arrow: &CurlyArrow) -> Option<([f32; 2], [f32; 2])> {
    let at = source_anchor(mol, &arrow.from, BOND_TAIL_AT)?;
    let ah = target_anchor(mol, &arrow.to)?;
    (len(sub(ah, at)) >= 1e-4).then_some((at, ah))
}

/// Where `arrow` is drawn on `mol`: by hand, or else as laid out from its roles ([`layout`]).
/// `None` when one of its atoms or bonds is missing, or its ends fall on one point.
pub fn geometry(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyPath> {
    if arrow.is_shaped() { shaped_geometry(mol, arrow) } else { layout(mol, arrow) }
}

// ─── The automatic layout ─────────────────────────────────────────────────────

/// What the layout keeps clear of: each atom near the arrow, with the radius it needs (its
/// label's, or a bare vertex's), and each bond, shortened at its ends; and the middle of those
/// atoms, which an arrow had rather bend away from.
struct Obstacles {
    atoms: Vec<(u32, [f32; 2], f32)>,
    bonds: Vec<([f32; 2], [f32; 2])>,
    centre: Option<[f32; 2]>,
}

impl Obstacles {
    fn around(mol: &Molecule, centre: [f32; 2], reach: f32) -> Self {
        let near = |p: [f32; 2]| len(sub(p, centre)) <= reach;
        let atoms: Vec<(u32, [f32; 2], f32)> = mol
            .atoms
            .iter()
            .filter(|a| near(a.pos) && !super::depict::folded_h(mol, a.id))
            .map(|a| {
                let r = label_extent(mol, a.id).map_or(0.22, |(l, r)| l.max(r).max(LABEL_HALF_HEIGHT) + 0.08);
                (a.id, a.pos, r)
            })
            .collect();
        let centre = (!atoms.is_empty()).then(|| {
            let n = atoms.len() as f32;
            let sum = atoms.iter().fold([0.0, 0.0], |s, a| [s[0] + a.1[0], s[1] + a.1[1]]);
            [sum[0] / n, sum[1] / n]
        });
        let bonds = mol
            .bonds
            .iter()
            .filter_map(|b| {
                let (p, q) = (pos(mol, b.begin)?, pos(mol, b.end)?);
                if super::depict::folded_h(mol, b.begin) || super::depict::folded_h(mol, b.end) || !(near(p) || near(q)) {
                    return None;
                }
                let d = sub(q, p);
                Some((add_scaled(p, d, 0.18), add_scaled(p, d, 0.82)))
            })
            .collect();
        Obstacles { atoms, bonds, centre }
    }

    /// A little more for a curve whose middle bends toward the middle of the atoms around it
    /// than its chord's is: between two shapes that clear everything, the one bending away.
    fn inward(&self, path: &CurlyPath) -> f32 {
        let Some(c) = self.centre else { return 0.0 };
        let chord_mid = midpoint(path.tail, path.head);
        if len(sub(path.middle(), c)) < len(sub(chord_mid, c)) { 0.3 } else { 0.0 }
    }

    /// How badly the curve `points` runs into what is around it: how deep it goes into each
    /// atom's room (the arrow's own atoms only along its middle part, as its ends belong next
    /// to them), and how many bonds it crosses.
    fn cost(&self, points: &[[f32; 2]], own: &[u32]) -> f32 {
        let n = points.len();
        let mut cost = 0.0;
        for &(id, p, r) in &self.atoms {
            let range = if own.contains(&id) { n / 6..n - n / 6 } else { 0..n };
            let nearest = points[range].iter().map(|q| len(sub(*q, p))).fold(f32::INFINITY, f32::min);
            if nearest < r {
                cost += (r - nearest) * 12.0;
            }
        }
        for &(a, b) in &self.bonds {
            cost += 3.0 * points.windows(2).filter(|w| segments_cross(w[0], w[1], a, b)).count() as f32;
        }
        cost
    }
}

fn segments_cross(p: [f32; 2], q: [f32; 2], a: [f32; 2], b: [f32; 2]) -> bool {
    let side = |x: [f32; 2], y: [f32; 2], z: [f32; 2]| (y[0] - x[0]) * (z[1] - x[1]) - (y[1] - x[1]) * (z[0] - x[0]);
    let (d1, d2) = (side(a, b, p), side(a, b, q));
    let (d3, d4) = (side(p, q, a), side(p, q, b));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// A way of drawing an arrow the layout weighs: the drawn curve, which side it bends to (+1
/// right of the way from source to target, −1 left), and how costly it is.
struct Candidate {
    path: CurlyPath,
    side: f32,
    cost: f32,
}

/// The arc from `from_at` to `to_at` turning `angle`, pulled off its anchors by the room each
/// end needs, measured the way the arc leaves and arrives: a lone pair's dots and tail outside
/// its label, a bond's tail just off it, a head short of its atom's label. A head on an atom
/// right by the source (a bond's electrons going onto one of its own atoms) comes down beside
/// that atom on the arc's outer side instead, so the arrow keeps an arc at all.
fn arc_candidate(mol: &Molecule, arrow: &CurlyArrow, from_at: [f32; 2], to_at: [f32; 2], angle: f32) -> Option<CurlyPath> {
    let leave = unit(sub(curve_point(from_at, to_at, angle, 0.01), from_at))?;
    let arrive = unit(sub(to_at, curve_point(from_at, to_at, angle, 0.99)))?;
    let mut dots = None;
    let mut tail = match arrow.from {
        CurlyEnd::Atom(a) => {
            let (d, t) = tail_room(mol, a, leave);
            if arrow.dots {
                dots = Some((add_scaled(from_at, leave, d), right_of(leave)));
            }
            add_scaled(from_at, leave, t)
        }
        CurlyEnd::Bond { .. } => add_scaled(from_at, leave, BOND_TAIL_LIFT),
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => from_at,
    };
    let close = len(sub(to_at, from_at)) < 1.1;
    let mut head = match arrow.to {
        CurlyEnd::Atom(a) if close => {
            let u = unit(sub(to_at, from_at))?;
            let side = if angle >= 0.0 { right_of(u) } else { [u[1], -u[0]] };
            add_scaled(to_at, side, head_room(mol, a, side))
        }
        CurlyEnd::Atom(a) => add_scaled(to_at, arrive, -head_room(mol, a, [-arrive[0], -arrive[1]])),
        CurlyEnd::Bond { .. } => add_scaled(to_at, arrive, -BOND_HEAD_GAP),
        CurlyEnd::Between(..) => add_scaled(to_at, arrive, -BETWEEN_HEAD_GAP),
        CurlyEnd::Point(_) => to_at,
    };
    // Ends so close that pulling them off their anchors would cross them: draw between the
    // anchors themselves.
    if len(sub(head, tail)) < 0.2 {
        (tail, head) = (from_at, to_at);
        dots = None;
    }
    let points = (0..=CURVE_STEPS).map(|k| curve_point(tail, head, angle, k as f32 / CURVE_STEPS as f32)).collect();
    Some(CurlyPath { tail, head, points, dots, pieces: arc_pieces(tail, head, angle), arc: Some(angle) })
}

/// The ways a tail on the source can leave it square: off a bond, along its normal on either
/// side (to the right of the bond's line from its `onto` atom first). `None` for any other end.
fn square_leaves(mol: &Molecule, end: &CurlyEnd) -> Option<[[f32; 2]; 2]> {
    let CurlyEnd::Bond { bond, onto } = *end else { return None };
    let (near, far) = bond_ends(mol, bond, onto)?;
    let n = right_of(unit(sub(far, near))?);
    Some([n, [-n[0], -n[1]]])
}

/// The ways a head can come square into the target: into a bond, or the gap between two atoms,
/// across its line from either side (from its right first). `None` for any other end.
fn square_arrivals(mol: &Molecule, end: &CurlyEnd) -> Option<[[f32; 2]; 2]> {
    let (p, q) = match *end {
        CurlyEnd::Bond { bond, .. } => {
            let b = mol.bond_by_id(bond)?;
            (pos(mol, b.begin)?, pos(mol, b.end)?)
        }
        CurlyEnd::Between(a, b) => (pos(mol, a)?, pos(mol, b)?),
        CurlyEnd::Atom(_) | CurlyEnd::Point(_) => return None,
    };
    let n = right_of(unit(sub(q, p))?);
    Some([[-n[0], -n[1]], n])
}

/// A curve's cubic pieces, and the turn of the circular arc they follow when they do.
type Shape = (Vec<[[f32; 2]; 4]>, Option<f32>);

/// The curve from `tail` to `head` that leaves along `leave` and arrives along `arrive` (unit
/// vectors, at least one given): the circular arc that does, when one is given or the two
/// agree, and its turn; else one cubic, each handle as long as an arc's would be for the angle
/// its end makes with the chord. `None` for an arc that would turn more than [`MAX_TURN`].
fn square_curve(tail: [f32; 2], leave: Option<[f32; 2]>, head: [f32; 2], arrive: Option<[f32; 2]>) -> Option<Shape> {
    let u = unit(sub(head, tail))?;
    // An arc leaves and arrives at the same angle to its chord, half its turn.
    let by_leave = leave.map(|d| 2.0 * angle_to(u, d));
    let by_arrive = arrive.map(|d| -2.0 * angle_to(u, d));
    let arc = match (by_leave, by_arrive) {
        (Some(a), None) | (None, Some(a)) => Some(a),
        (Some(a), Some(b)) if (a - b).abs() < 4.0 => Some(a),
        (Some(_), Some(_)) => None,
        (None, None) => return None,
    };
    if let Some(a) = arc {
        return (a.abs() <= MAX_TURN).then(|| (arc_pieces(tail, head, a), Some(a)));
    }
    let (d0, d1) = (leave?, arrive?);
    let c = len(sub(head, tail));
    // A cubic through an arc whose ends make angle α with its chord: handles of c / (3 cos²(α/2)).
    let handle = |d: [f32; 2]| {
        let half = dot(d, u).clamp(-1.0, 1.0).acos() / 2.0;
        (c / (3.0 * half.cos().powi(2))).min(1.2 * c)
    };
    Some((vec![[tail, add_scaled(tail, d0, handle(d0)), add_scaled(head, d1, -handle(d1)), head]], None))
}

/// An arrow with an end on a bond (or a gap) square to it: its tail stands off the source bond
/// along `leave`, the bond's normal on the side it is drawn, and leaves along it; its head stops
/// short of the target's middle, coming in along `arrive`. An end on anything else is placed as
/// on an arc ([`arc_candidate`]), the way the curve leaves or arrives there: the arc square to
/// the other end — or, with `bend`, for one too nearly straight to read as curly, a cubic whose
/// free end turns `bend` degrees off its chord.
fn square_candidate(
    mol: &Molecule,
    arrow: &CurlyArrow,
    from_at: [f32; 2],
    to_at: [f32; 2],
    leave: Option<[f32; 2]>,
    arrive: Option<[f32; 2]>,
    bend: Option<f32>,
) -> Option<CurlyPath> {
    let lift = |n: [f32; 2]| match arrow.from {
        CurlyEnd::Bond { bond, .. } => bond_reach(mol, bond, n) + BOND_TAIL_LIFT,
        _ => BOND_TAIL_LIFT,
    };
    let mut tail = leave.map_or(from_at, |n| add_scaled(from_at, n, lift(n)));
    let gap = |d: [f32; 2]| match arrow.to {
        CurlyEnd::Bond { bond, .. } => bond_reach(mol, bond, [-d[0], -d[1]]) + BOND_HEAD_GAP,
        _ => BETWEEN_HEAD_GAP,
    };
    let mut head = arrive.map_or(to_at, |d| add_scaled(to_at, d, -gap(d)));
    // The way the free end goes: its arc's mirror of the square end's, or `bend` off the chord.
    let u = unit(sub(head, tail))?;
    let free = |square: Option<[f32; 2]>| bend.map_or_else(|| square.map(|d| turned(u, -angle_to(u, d))), |b| Some(turned(u, b)));
    let mut dots = None;
    if leave.is_none() {
        let d = free(arrive)?;
        if let CurlyEnd::Atom(a) = arrow.from {
            let (room, t) = tail_room(mol, a, d);
            if arrow.dots {
                dots = Some((add_scaled(from_at, d, room), right_of(d)));
            }
            tail = add_scaled(from_at, d, t);
        }
    }
    if arrive.is_none() {
        let d = free(leave)?;
        let close = len(sub(to_at, from_at)) < 1.1;
        head = match (arrow.to, leave) {
            // Onto an atom of its own bond: down beside it, on the side the tail leaves to.
            (CurlyEnd::Atom(a), Some(n)) if close => add_scaled(to_at, n, head_room(mol, a, n)),
            (CurlyEnd::Atom(a), _) => add_scaled(to_at, d, -head_room(mol, a, [-d[0], -d[1]])),
            _ => to_at,
        };
    }
    if len(sub(head, tail)) < 0.2 {
        return None;
    }
    let u = unit(sub(head, tail))?;
    let bent = bend.map(|b| turned(u, b));
    let (pieces, arc) = square_curve(tail, leave.or(bent), head, arrive.or(bent))?;
    let points = match arc {
        Some(a) => (0..=CURVE_STEPS).map(|k| curve_point(tail, head, a, k as f32 / CURVE_STEPS as f32)).collect(),
        None => sample(&pieces),
    };
    Some(CurlyPath { tail, head, points, dots, pieces, arc })
}

/// How far the curve `points` turns, all told, in degrees.
fn turning(points: &[[f32; 2]]) -> f32 {
    let steps: Vec<[f32; 2]> = points.windows(2).filter_map(|w| unit(sub(w[1], w[0]))).collect();
    steps.windows(2).map(|w| angle_to(w[0], w[1]).abs()).sum()
}

/// The S an arrow takes from a bond's electrons, carried by its `onto` atom, to a new partner
/// `target` (the π electrons of C=O, through O, to H): it leaves the bond's middle square to
/// it on `side` (+1 or −1 across the bond from its other atom to `onto`, +1 to the right),
/// turns up along the bond past `onto` on that side, outside its label — the turn that makes it
/// an S — and comes into the partner along the bond being made, its head short of the
/// partner's label.
fn via_candidate(mol: &Molecule, bond: u32, onto: u32, target: u32, side: f32) -> Option<CurlyPath> {
    let (b, a) = bond_ends(mol, bond, onto)?;
    let t = pos(mol, target)?;
    let ub = unit(sub(b, a))?;
    let r = right_of(ub);
    let n = [r[0] * side, r[1] * side];
    let p0 = add_scaled(midpoint(a, b), n, bond_reach(mol, bond, n) + BOND_TAIL_LIFT);
    // Beside `onto`, well outside its label.
    let room = label_reach(mol, onto, n).unwrap_or(0.15) + 0.45;
    let w = add_scaled(b, n, room);
    let v = unit(sub(t, b))?;
    let q = add_scaled(t, v, -head_room(mol, target, [-v[0], -v[1]]));
    if len(sub(q, w)) < 0.3 || dot(sub(q, w), v) <= 0.0 {
        return None;
    }
    // Past `onto` it runs along the bond, turning toward the partner.
    let tw = unit(add_scaled(ub, v, 0.6))?;
    let (l1, l2) = (len(sub(w, p0)), len(sub(q, w)));
    let pieces = vec![
        [p0, add_scaled(p0, n, 0.5 * l1), add_scaled(w, tw, -0.5 * l1), w],
        [w, add_scaled(w, tw, 0.4 * l2), add_scaled(q, v, -0.4 * l2), q],
    ];
    let points = sample(&pieces);
    Some(CurlyPath { tail: p0, head: q, points, dots: None, pieces, arc: None })
}

/// The polyline of cubic `pieces`, in order.
fn sample(pieces: &[[[f32; 2]; 4]]) -> Vec<[f32; 2]> {
    let mut pts = vec![pieces[0][0]];
    for b in pieces {
        pts.extend((1..=PIECE_STEPS).map(|k| bezier_point(b, k as f32 / PIECE_STEPS as f32)));
    }
    pts
}

/// How `arrow` is drawn automatically on `mol`, from its roles: the candidate shapes of its
/// case, each weighed by how much it runs into the atoms, labels and bonds around it (and, for
/// arcs, how far it is from a comfortable bend, and whether it bends in toward the atoms rather
/// than away from them), the least costly drawn — or, with
/// [`CurlyArrow::flip`], the least costly bending the other way.
///
/// An arrow leaves a bond square to it, on either side, and comes square into a bond or a gap
/// ([`square_candidate`]): the arc that does so, or, between two such ends, the curve that does
/// at both.
///
/// - **A lone pair to an atom:** an arc from the source (its dots on the side facing the
///   target) to the target, turning 60° to 150° either way.
/// - **A lone pair into a bond or a gap; a bond into a bond or a gap:** square to each bond and
///   gap.
/// - **A bond onto one of its own atoms:** an arc square to the bond from its middle, coming down
///   beside the atom on the same side.
/// - **A bond to a new partner by way of `onto`:** the S of [`via_candidate`], on either side of
///   the bond.
///
/// `None` when an atom or bond it names is missing, or its ends fall on one point.
pub fn layout(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyPath> {
    let case = arrow.case(mol);
    let to_at = target_anchor(mol, &arrow.to)?;
    let middle = matches!(case, CurlyCase::BondOntoOwnAtom | CurlyCase::BondViaAtomToPartner);
    let from_at = source_anchor(mol, &arrow.from, if middle { 0.5 } else { BOND_TAIL_AT })?;
    let chord = sub(to_at, from_at);
    if len(chord) < 1e-4 {
        return None;
    }
    let own = arrow.atoms(mol);
    let obstacles = Obstacles::around(mol, midpoint(from_at, to_at), len(chord) / 2.0 + 2.5);
    let comfort = if len(chord) > 3.0 { 90.0 } else { 120.0 };
    let mut candidates: Vec<Candidate> = Vec::new();
    let weigh = |path: &CurlyPath| {
        let turn = path.arc.map_or_else(|| turning(&path.points), f32::abs);
        obstacles.cost(&path.points, &own) + obstacles.inward(path) + (turn - comfort).abs() / 60.0 * 0.4
    };
    // Square to a bond or a gap at either end, on each side of it.
    let sides = |ways: Option<[[f32; 2]; 2]>| match ways {
        Some([right, left]) => vec![(1.0f32, Some(right)), (-1.0, Some(left))],
        None => vec![(0.0, None)],
    };
    let (leaves, arrivals) = (square_leaves(mol, &arrow.from), square_arrivals(mol, &arrow.to));
    let square = leaves.is_some() || arrivals.is_some();
    let arcs = |angles: &[f32], candidates: &mut Vec<Candidate>| {
        if square {
            for (s0, leave) in sides(leaves) {
                for &(s1, arrive) in &sides(arrivals) {
                    // Its side: the side of the source bond it leaves, else of the target's line.
                    let side = if leave.is_some() { s0 } else { s1 };
                    let path = square_candidate(mol, arrow, from_at, to_at, leave, arrive, None);
                    // An arc too nearly straight, or one that would loop: a curve bent at its
                    // free end instead.
                    let bent = path.as_ref().is_none_or(|p| p.arc.is_some_and(|a| a.abs() < FLAT_TURN));
                    if let Some(path) = path {
                        candidates.push(Candidate { cost: weigh(&path), path, side });
                    }
                    if bent && (leave.is_none() || arrive.is_none()) {
                        for bend in [FLAT_BEND, -FLAT_BEND] {
                            if let Some(path) = square_candidate(mol, arrow, from_at, to_at, leave, arrive, Some(bend)) {
                                candidates.push(Candidate { cost: weigh(&path), path, side });
                            }
                        }
                    }
                }
            }
            if !candidates.is_empty() {
                return;
            }
        }
        for &a in angles {
            for sign in [1.0f32, -1.0] {
                if let Some(path) = arc_candidate(mol, arrow, from_at, to_at, a * sign) {
                    candidates.push(Candidate { cost: weigh(&path), path, side: sign });
                }
            }
        }
    };
    match (case, arrow.from, arrow.to) {
        (CurlyCase::BondViaAtomToPartner, CurlyEnd::Bond { bond, onto }, CurlyEnd::Atom(t)) => {
            for side in [1.0f32, -1.0] {
                if let Some(path) = via_candidate(mol, bond, onto, t, side) {
                    // Its side is the side of the bond it goes round `onto` on (an S crosses
                    // the line from source to target, so that says nothing).
                    let cost = obstacles.cost(&path.points, &own);
                    candidates.push(Candidate { path, side, cost });
                }
            }
            if candidates.is_empty() {
                arcs(&[90.0, 120.0], &mut candidates);
            }
        }
        // Short arrows next to their atoms: deeper arcs, so they show at all.
        (CurlyCase::BondOntoOwnAtom | CurlyCase::LonePairToBond, ..) => arcs(&[120.0, 150.0, 180.0], &mut candidates),
        _ => arcs(&[60.0, 90.0, 120.0, 150.0], &mut candidates),
    }
    // The least costly; ties go to the earlier one, so the choice is steady.
    let best = candidates.iter().enumerate().min_by(|x, y| x.1.cost.total_cmp(&y.1.cost).then(x.0.cmp(&y.0)))?.0;
    let pick = if arrow.flip {
        let side = candidates[best].side;
        candidates
            .iter()
            .enumerate()
            .filter(|(_, c)| c.side != side)
            .min_by(|x, y| x.1.cost.total_cmp(&y.1.cost).then(x.0.cmp(&y.0)))
            .map_or(best, |(i, _)| i)
    } else {
        best
    };
    Some(candidates.swap_remove(pick).path)
}

// ─── A shape set by hand ──────────────────────────────────────────────────────

/// The cubic pieces of `arrow`'s hand-set shape on the page, from the source's anchor to the
/// target's; `None` for an automatic arrow, or one that cannot be placed.
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

/// The hand-set shape (see [`CurlyArrow::path`]) of cubic `pieces` running from `from` to `to`:
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

/// Give `arrow` the hand-set shape of what it is drawn as now, unless it has one already: what
/// editing a handle of an automatic arrow starts from. False when it cannot be placed.
pub fn shape_from_layout(mol: &Molecule, arrow: &mut CurlyArrow) -> bool {
    if arrow.is_shaped() {
        return true;
    }
    let (Some(drawn), Some((at, ah))) = (layout(mol, arrow), anchors(mol, arrow)) else { return false };
    match path_of(at, ah, &drawn.pieces) {
        Some(path) => {
            arrow.path = path;
            true
        }
        None => false,
    }
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

/// Where a lone pair's dots go: their middle, and the direction across them.
type Dots = Option<([f32; 2], [f32; 2])>;

/// How much of `full` (a hand-shaped arrow's whole curve, anchor to anchor) is cut off its start
/// and its end, measured the way it leaves and arrives, and where the lone pair's dots go.
/// Nothing is cut from a curve too short to cut both ends back.
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
    let start = match arrow.from {
        CurlyEnd::Atom(a) => {
            let (d, t) = tail_room(mol, a, leave);
            if arrow.dots && d < total {
                let (at, dir) = along(full, d);
                dots = Some((at, right_of(dir)));
            }
            t
        }
        CurlyEnd::Bond { .. } => BOND_TAIL_LIFT,
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => 0.0,
    };
    let end = match arrow.to {
        CurlyEnd::Atom(a) => head_room(mol, a, [-arrive[0], -arrive[1]]),
        CurlyEnd::Bond { .. } => BOND_HEAD_GAP,
        CurlyEnd::Between(..) => BETWEEN_HEAD_GAP,
        CurlyEnd::Point(_) => 0.0,
    };
    if start + end + 0.2 < total { (start, end, dots) } else { (0.0, 0.0, None) }
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

/// [`geometry`] of a hand-shaped arrow: its pieces from anchor to anchor, cut back at each end
/// by the room the end needs (the pieces themselves cut there too, so they draw what is drawn).
fn shaped_geometry(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyPath> {
    let mut pieces = pieces(mol, arrow)?;
    let full = sample(&pieces);
    let total: f32 = full.windows(2).map(|w| len(sub(w[1], w[0]))).sum();
    if total < 1e-4 {
        return None;
    }
    let (start, end, dots) = cuts(mol, arrow, &full);
    let points = trim(&full, start, end);
    if points.len() < 2 {
        return None;
    }
    if start > 0.0 {
        pieces[0] = split(&pieces[0], param_at(&pieces[0], start).min(0.95)).1;
    }
    if end > 0.0 {
        let last = pieces.len() - 1;
        let b = pieces[last];
        let reversed = [b[3], b[2], b[1], b[0]];
        let kept = split(&reversed, param_at(&reversed, end).min(0.95)).1;
        pieces[last] = [kept[3], kept[2], kept[1], kept[0]];
    }
    let (tail, head) = (points[0], points[points.len() - 1]);
    Some(CurlyPath { tail, head, points, dots, pieces, arc: None })
}

/// The handles a selected curly arrow is edited by: its ends, which can be dragged onto
/// something else; the knots between the pieces of its curve; and each piece's two control
/// points, with the point each one is the tangent from. An automatic arrow shows those of the
/// curve it is drawn as: dragging one gives it that shape to edit by hand.
#[derive(Debug, Clone, PartialEq)]
pub struct CurlyHandles {
    pub tail: [f32; 2],
    pub head: [f32; 2],
    /// The knots: their index in [`CurlyArrow::path`], and where they are.
    pub knots: Vec<(usize, [f32; 2])>,
    /// The control points: their index in the path, where they are, and the end of the piece
    /// they pull on (the knot or end their tangent line runs to).
    pub controls: Vec<(usize, [f32; 2], [f32; 2])>,
}

/// [`CurlyHandles`] of `arrow` on `mol`: a hand-set shape's own control points and knots (not
/// those of its curve cut back at the ends), an automatic arrow's of the curve it is drawn as.
pub fn handles(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyHandles> {
    let drawn = geometry(mol, arrow)?;
    let pieces = pieces(mol, arrow).unwrap_or_else(|| drawn.pieces.clone());
    let mut knots = Vec::new();
    let mut controls = Vec::new();
    for (i, b) in pieces.iter().enumerate() {
        controls.push((3 * i, b[1], b[0]));
        controls.push((3 * i + 1, b[2], b[3]));
        if i + 1 < pieces.len() {
            knots.push((3 * i + 2, b[3]));
        }
    }
    Some(CurlyHandles { tail: drawn.tail, head: drawn.head, knots, controls })
}

/// Move point `index` of `arrow`'s path (see [`CurlyArrow::path`]) to page point `to`, an
/// automatic arrow first taking the shape it is drawn as. A knot takes its two control points
/// with it; a control point next to a knot turns the one across the knot with it, keeping that
/// one's length, so the curve stays smooth there. False when nothing could be moved.
pub fn move_path_point(mol: &Molecule, arrow: &mut CurlyArrow, index: usize, to: [f32; 2]) -> bool {
    if !shape_from_layout(mol, arrow) {
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

/// Split `arrow`'s curve where it passes nearest page point `near`, adding a knot there (an
/// automatic arrow first taking the shape it is drawn as). The curve does not change. Returns
/// the knot's index in the path.
pub fn insert_knot(mol: &Molecule, arrow: &mut CurlyArrow, near: [f32; 2]) -> Option<usize> {
    if !shape_from_layout(mol, arrow) {
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
            let (from, to) = (loses(self, &c.from), loses(self, &c.to));
            if !from && !to {
                continue;
            }
            // Where it stands now; an arrow already broken keeps its ends as they are.
            if let Some(path) = geometry(self, c) {
                changes.push((k, from.then_some(path.tail), to.then_some(path.head), pieces(self, c)));
            }
        }
        for (k, from, to, pieces) in changes {
            let super::PageObject::Curly(c) = &mut self.objects[k] else { continue };
            if let Some(p) = from {
                c.from = CurlyEnd::Point(p);
                c.dots = false;
            }
            if let Some(p) = to {
                c.to = CurlyEnd::Point(p);
            }
            // A hand-set shape keeps its control points where they were on the page, in the
            // frame of its new ends.
            if let Some(pieces) = pieces {
                let start = from.unwrap_or(pieces[0][0]);
                let end = to.unwrap_or(pieces[pieces.len() - 1][3]);
                if let Some(path) = path_of(start, end, &pieces) {
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
    let l2 = dot(d, d);
    if l2 < 1e-12 {
        return (len(sub(p, a)), 0.0);
    }
    let t = (dot(sub(p, a), d) / l2).clamp(0.0, 1.0);
    (len(sub(p, add_scaled(a, d, t))), t)
}

/// What a free arrow end at `p` most likely stands on, for an arrow read from a file that does
/// not say: the middle part of a bond it lies on, or else the one atom it is clearly next to.
/// `None` when that is not clear (two atoms about as near, or none near at all), so the end
/// stays free rather than being tied to the wrong atom.
pub fn end_near(mol: &Molecule, p: [f32; 2], source: bool) -> Option<CurlyEnd> {
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
        let onto = if t <= 0.5 { b.begin } else { b.end };
        return Some(CurlyEnd::Bond { bond: b.id, onto });
    }
    let mut near: Vec<(f32, u32)> = mol.atoms.iter().map(|a| (len(sub(a.pos, p)), a.id)).collect();
    near.sort_by(|x, y| x.0.total_cmp(&y.0));
    let reach = if source { 1.0 } else { 0.9 };
    match near[..] {
        [(d, id), ..] if d <= reach && near.get(1).is_none_or(|n| n.0 >= d * 1.6) => Some(CurlyEnd::Atom(id)),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
