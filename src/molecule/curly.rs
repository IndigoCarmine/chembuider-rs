//! Curly arrows: the electron-pushing arrows of a reaction mechanism.
//!
//! A [`CurlyArrow`] says where a pair of electrons (or, as a fish-hook, a single electron) comes
//! from and where it goes, and how far its arc bends. Its two ends name what they are on rather
//! than where they are: a lone pair of an atom or a bond at the tail; an atom, a bond, or the
//! gap between two atoms where a new bond forms at the head ([`CurlyEnd`]). So the arrow follows
//! its atoms when they are moved, rotated or cleaned up, the way ring fills and brackets do, and
//! it lives in [`Molecule::objects`](super::Molecule::objects) beside them, so undo, JSON saving
//! and the shared scene cover it without a path of its own.
//!
//! Where the arrow is drawn is worked out from the drawing each time ([`geometry`]). The arc is
//! laid between the two anchors (an atom's centre, a point on a bond, the middle between two
//! atoms) and then pulled off them along its own direction: the tail leaves from just outside
//! the lone pair (outside the atom's label, when it shows one) or just off the bond, and the head
//! stops short of its atom's label. The curve itself is a circular arc of the stored sweep
//! ([`CurlyArrow::angle`], ChemDraw's `AngularSize`), which may be skewed so its highest point
//! leans toward one end ([`CurlyArrow::skew`]); ChemDraw has no skew and draws the plain arc.
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
    /// A single electron (a radical step): a fish-hook, half a head on the outer side of the arc.
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
    pub angle: f32,
    /// How far the arc's highest point leans toward the head (positive) or the tail (negative),
    /// as a share of half the distance between the ends; 0 is a plain circular arc.
    #[serde(default)]
    pub skew: f32,
    #[serde(default)]
    pub kind: CurlyKind,
    /// Draw the lone pair a tail on an atom starts from, as two dots.
    #[serde(default)]
    pub dots: bool,
    /// The id of the ChemDraw `arrow` this was read from; see [`PageArrow::source`](super::PageArrow::source).
    #[serde(default)]
    pub source: Option<u32>,
}

impl CurlyArrow {
    /// A new arrow from `tail` to `head` bending `angle` degrees, with no skew; a lone-pair tail
    /// shows its dots.
    pub fn new(tail: CurlyEnd, head: CurlyEnd, angle: f32, kind: CurlyKind) -> Self {
        let dots = matches!(tail, CurlyEnd::Atom(_));
        CurlyArrow { id: 0, tail, head, angle: clamp_angle(angle), skew: 0.0, kind, dots, source: None }
    }

    /// The atoms the arrow depends on, tail first.
    pub fn atoms(&self, mol: &Molecule) -> Vec<u32> {
        let mut atoms = self.tail.atoms(mol);
        atoms.extend(self.head.atoms(mol));
        atoms
    }

    /// Bend the other way, as a mirror image does.
    pub fn flip(&mut self) {
        self.angle = -self.angle;
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
pub const BOND_TAIL_AT: f32 = 0.35;

// Clearances, in molecule units (a bond is 1.5). An atom's label is about 0.75 high and 0.7
// wide per letter (10 pt beside a 14.4 pt bond), so a shown label needs more room than a bare
// carbon vertex.
/// From a labelled atom's centre to the middle of its lone pair, and to where the tail leaves.
const DOTS_LABELLED: f32 = 0.52;
const TAIL_LABELLED: f32 = 0.72;
/// The same for an atom shown as a bare vertex.
const DOTS_BARE: f32 = 0.26;
const TAIL_BARE: f32 = 0.44;
/// How far short of an atom's centre a head stops.
const HEAD_GAP_LABELLED: f32 = 0.55;
const HEAD_GAP_BARE: f32 = 0.24;
/// How far a tail on a bond is lifted off its line, and how far short of a bond's (or a gap's)
/// middle a head stops.
const BOND_TAIL_LIFT: f32 = 0.1;
const BOND_HEAD_GAP: f32 = 0.16;
const BETWEEN_HEAD_GAP: f32 = 0.08;
/// A head atom nearer its tail's anchor than this lands beside the atom (see [`geometry`]).
const CLOSE_HEAD: f32 = 1.1;
/// How many straight pieces the curve is drawn in.
const CURVE_STEPS: usize = 48;

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
    /// The middle of the curve: where the handle that bends it is.
    pub fn middle(&self) -> [f32; 2] {
        self.points[self.points.len() / 2]
    }
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add_scaled(a: [f32; 2], d: [f32; 2], k: f32) -> [f32; 2] {
    [a[0] + d[0] * k, a[1] + d[1] * k]
}

fn len(d: [f32; 2]) -> f32 {
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

fn unit(d: [f32; 2]) -> Option<[f32; 2]> {
    let l = len(d);
    (l > 1e-6).then(|| [d[0] / l, d[1] / l])
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

/// The angle and skew that make the arc from `p` to `q` pass through `through` at its middle:
/// what dragging the arrow's handle there does.
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

/// Where `arrow` is drawn on `mol`; `None` when one of its atoms or bonds is missing, or its
/// two ends fall on one point.
pub fn geometry(mol: &Molecule, arrow: &CurlyArrow) -> Option<CurlyPath> {
    let at = anchor(mol, &arrow.tail, true)?;
    let ah = anchor(mol, &arrow.head, false)?;
    if len(sub(ah, at)) < 1e-4 {
        return None;
    }
    let (angle, skew) = (clamp_angle(arrow.angle), clamp_skew(arrow.skew));
    // The way the arc laid between the anchors leaves the tail and comes into the head.
    let leave = unit(sub(curve_point(at, ah, angle, skew, 0.01), at))?;
    let arrive = unit(sub(ah, curve_point(at, ah, angle, skew, 0.99)))?;
    let mut dots = None;
    let tail = match arrow.tail {
        CurlyEnd::Atom(a) => {
            let (d, t) = if labelled(mol, a) { (DOTS_LABELLED, TAIL_LABELLED) } else { (DOTS_BARE, TAIL_BARE) };
            if arrow.dots {
                dots = Some((add_scaled(at, leave, d), [-leave[1], leave[0]]));
            }
            add_scaled(at, leave, t)
        }
        CurlyEnd::Bond { .. } => add_scaled(at, leave, BOND_TAIL_LIFT),
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => at,
    };
    let gap = match arrow.head {
        CurlyEnd::Atom(a) if labelled(mol, a) => HEAD_GAP_LABELLED,
        CurlyEnd::Atom(_) => HEAD_GAP_BARE,
        CurlyEnd::Bond { .. } => BOND_HEAD_GAP,
        CurlyEnd::Between(..) => BETWEEN_HEAD_GAP,
        CurlyEnd::Point(_) => 0.0,
    };
    let close = len(sub(ah, at)) < CLOSE_HEAD;
    let mut head = match arrow.head {
        // An atom right by the tail (a bond's electrons going onto one of its own atoms): come
        // down beside it on the side the arc bends to, rather than stop short of it, which
        // would leave next to no arc.
        CurlyEnd::Atom(_) if close => {
            let u = unit(sub(ah, at))?;
            let side = if angle >= 0.0 { [-u[1], u[0]] } else { [u[1], -u[0]] };
            add_scaled(ah, side, gap)
        }
        _ => add_scaled(ah, arrive, -gap),
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
                changes.push((k, tail.then_some(path.tail), head.then_some(path.head)));
            }
        }
        for (k, tail, head) in changes {
            if let super::PageObject::Curly(c) = &mut self.objects[k] {
                if let Some(p) = tail {
                    c.tail = CurlyEnd::Point(p);
                    c.dots = false;
                }
                if let Some(p) = head {
                    c.head = CurlyEnd::Point(p);
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
    fn a_lone_pair_tail_leaves_outside_the_label_and_the_head_stops_short() {
        let (mut m, [c, o, _], _) = acetyl();
        let id = m.add_curly_arrow(CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Atom(c), 120.0, CurlyKind::Pair));
        let arrow = m.object_by_id(id).unwrap().as_curly().unwrap().clone();
        let path = geometry(&m, &arrow).unwrap();
        let from = |p: [f32; 2], id: u32| len(sub(p, m.atom_by_id(id).unwrap().pos));
        assert!((from(path.tail, o) - TAIL_LABELLED).abs() < 1e-4, "{path:?}");
        assert!((from(path.head, c) - HEAD_GAP_BARE).abs() < 1e-4, "{path:?}");
        let (dots, across) = path.dots.expect("a lone pair tail shows its dots");
        assert!((from(dots, o) - DOTS_LABELLED).abs() < 1e-4);
        assert!((len(across) - 1.0).abs() < 1e-4);
        // Following the atoms.
        m.atom_by_id_mut(o).unwrap().pos[0] += 3.0;
        m.atom_by_id_mut(c).unwrap().pos[0] += 3.0;
        let moved = geometry(&m, &arrow).unwrap();
        assert!(close(moved.tail, [path.tail[0] + 3.0, path.tail[1]]), "{moved:?}");
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
        let (mut m, [c, o, me], co) = acetyl();
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
        let _ = c;
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
