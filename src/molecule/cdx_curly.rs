//! Curly arrows ([`CurlyArrow`]) in a ChemDraw document.
//!
//! ChemDraw draws a curved arrow as an `arrow` object whose `AngularSize` is not 0: an arc of
//! the circle about `Center3D` (its axes ending at `MajorAxisEnd3D` and `MinorAxisEnd3D`), from
//! `Tail3D` to `Head3D`, turning `AngularSize` degrees — counterclockwise on the page when
//! positive (ChemDraw writes a 180° arrow from left to right bending below, a −180° one bending
//! above). Its head is `ArrowheadHead`: `Full`, or `HalfLeft` / `HalfRight` for a fish-hook. Since
//! ChemDraw 9 the arrow supersedes an older arc `graphic` (`GraphicType="Arc"`, its "bounding
//! box" holding the head and the centre, its angle rounded to 90, 120, 180 or 270°), kept for
//! older readers. ChemDraw 23 also ties an arrow to what it starts and ends on: `ArrowSource`
//! and `ArrowTarget` hold the id of a node or a bond.
//!
//! Writing puts each curly arrow on the page that way — the arc between the ends this editor
//! draws, the head, the superseded graphic, `ArrowSource` / `ArrowTarget` for an end on an atom
//! or a bond — so ChemDraw shows a curved arrow where this editor shows one. What ChemDraw has no
//! word for (the side of a bond a tail stands on, a head between two atoms, the skew, the lone
//! pair's dots) goes into an object tag on the arrow, [`CURLY_TAG`], whose value names the ends
//! by the ids of the nodes and bonds they were written as:
//!
//! ```text
//! <arrow id="20" ... AngularSize="-120" ArrowSource="3" ArrowTarget="9" ...>
//!   <objecttag TagType="String" Name="ChemBuilder/CurlyArrow" Persistent="yes"
//!     Value="v=1 tail=bond:9:4 head=atom:3 angle=-120 skew=0.15 kind=pair dots=0"/>
//! </arrow>
//! ```
//!
//! **A free shape** (an S, a long U or L round other atoms: [`CurlyArrow::path`]) is no arc, so
//! it is written as ChemDraw's Bézier `curve` instead (`CurvePoints`: each knot as control-in,
//! knot, control-out), drawn as this editor draws it, cut back at the ends, with the same
//! arrowheads (`ArrowheadHead`, `ArrowheadType`) and the same tag, whose value then carries the
//! path in the frame of its anchors:
//!
//! ```text
//! <curve id="30" CurvePoints="..." FillType="None" ArrowheadHead="Full" ArrowheadType="Solid" ...>
//!   <objecttag ... Name="ChemBuilder/CurlyArrow" Value="v=2 tail=bond:9:4 head=atom:12 ...
//!     path=0.2,0.9;0.4,0;0.5,-0.2;0.6,-0.4;0.9,-0.8"/>
//! </curve>
//! ```
//!
//! A ChemDraw curve with a head at its end and none at its start is read back as a curly arrow
//! of that shape. A ChemDraw curve has no `ArrowSource` / `ArrowTarget`, so without the tag its
//! ends are tied only where they clearly stand.
//!
//! Reading takes an arrow with the tag back exactly as it was written — provided the tag still
//! describes the arrow: ChemDraw keeps a persistent tag, but if the arrow or its atoms were moved
//! there, or ChemDraw gave the objects new ids, the ends the tag names are no longer where the
//! arrow's `Tail3D` and `Head3D` are, and the tag is not trusted. An arrow without a tag that
//! holds (one drawn in ChemDraw) is read from its geometry: its ends are tied to what
//! `ArrowSource` / `ArrowTarget` name, or else to a bond or atom they clearly stand on
//! ([`curly::end_near`]), or else left free where they are.

use std::collections::HashMap;

use super::cdx::{self, WrittenIds, OBJ_BOND, PROP_BOUNDING_BOX, PT, SCALE};
use super::cdx_appearance;
use super::cdx_codec::CdxObject;
use super::curly::{self, CurlyArrow, CurlyEnd, CurlyKind};
use super::Molecule;
use crate::widget::scene::{CURLY_HEAD_CENTER, CURLY_HEAD_HALF_WIDTH, CURLY_HEAD_LENGTH};

pub(crate) const OBJ_GRAPHIC: u16 = 0x8007;
pub(crate) const OBJ_ARROW: u16 = 0x8021;
pub(crate) const OBJ_CURVE: u16 = 0x8008;

const PROP_SUPERSEDED_BY: u16 = 0x0013;
const PROP_HEAD_3D: u16 = 0x0207;
const PROP_TAIL_3D: u16 = 0x0208;
const PROP_CENTER_3D: u16 = 0x020D;
const PROP_MAJOR_AXIS_END_3D: u16 = 0x020E;
const PROP_MINOR_AXIS_END_3D: u16 = 0x020F;
const PROP_BOND_BEGIN: u16 = 0x0604;
const PROP_BOND_END: u16 = 0x0605;
const PROP_GRAPHIC_TYPE: u16 = 0x0A00;
const PROP_ARROW_TYPE: u16 = 0x0A02;
const PROP_HEAD_SIZE: u16 = 0x0A20;
const PROP_ANGULAR_SIZE: u16 = 0x0A21;
const PROP_ARROWHEAD_TYPE: u16 = 0x0A2F;
const PROP_ARROWHEAD_CENTER_SIZE: u16 = 0x0A30;
const PROP_ARROWHEAD_WIDTH: u16 = 0x0A31;
const PROP_ARROW_SHAFT_SPACING: u16 = 0x0A33;
const PROP_ARROWHEAD_HEAD: u16 = 0x0A35;
const PROP_ARROWHEAD_TAIL: u16 = 0x0A36;
const PROP_FILL_TYPE: u16 = 0x0A37;
const PROP_DIPOLE: u16 = 0x0A3A;
const PROP_NO_GO: u16 = 0x0A3B;
const PROP_ARROW_SOURCE: u16 = 0x0A3E;
const PROP_CURVE_TYPE: u16 = 0x0A08;
const PROP_CURVE_POINTS: u16 = 0x0A23;
const PROP_CLOSED: u16 = 0x0A39;
const PROP_ARROW_TARGET: u16 = 0x0A3F;

const GRAPHIC_ARC: i16 = 2;
const ARROW_TYPE_HALF_HEAD: i16 = 1;
const ARROW_TYPE_FULL_HEAD: i16 = 2;
const HEAD_NONE: i16 = 1;
const HEAD_FULL: i16 = 2;
const HEAD_HALF_LEFT: i16 = 3;
const HEAD_HALF_RIGHT: i16 = 4;
const ARROWHEAD_SOLID: i16 = 1;
const FILL_NONE: i16 = 1;

/// The name of the object tag that holds what ChemDraw cannot say about a curly arrow.
pub(crate) const CURLY_TAG: &str = "ChemBuilder/CurlyArrow";
/// The version of the tag's value this writes; a reader takes this one and earlier ones. 1: the
/// ends, angle, skew, kind and dots; 2: and the free shape (`path`).
const TAG_VERSION: u32 = 2;
/// kCDXCurveType: closed, and the arrowheads an older curve has at its start (whole or half).
const CURVE_CLOSED: i16 = 0x0001;
const CURVE_ARROWS_AT_START: i16 = 0x0010 | 0x0040;
/// kCDXCurveType: an arrowhead at the end, whole or half (an older curve's head).
const CURVE_ARROW_AT_END: i16 = 0x0008;
const CURVE_HALF_ARROW_AT_END: i16 = 0x0020;
/// How far an end the tag names may lie from the arrow's `Tail3D` / `Head3D` (molecule units, a
/// bond is 1.5) for the tag to be trusted: rounding, not an edit.
const TAG_TOLERANCE: f32 = 0.12;
/// The smallest turn written, in degrees: ChemDraw's arc needs a centre, and a nearly straight
/// arc's centre is far away (at 2°, 29 chords off).
const MIN_WRITTEN_ANGLE: f32 = 2.0;

fn hundredths(v: f32) -> i16 {
    (v * 100.0).round() as i16
}

fn i16_bytes(v: i16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn point_3d_value(p: (f64, f64)) -> Vec<u8> {
    [p.0, p.1, 0.0].iter().flat_map(|v| (v.round() as i32).to_le_bytes()).collect()
}

fn point_3d(obj: &CdxObject, tag: u16) -> Option<(f64, f64)> {
    let v = obj.prop(tag).filter(|v| v.len() >= 8)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some((n(0), n(4)))
}

fn to_cdx(p: [f32; 2], shift: (f64, f64)) -> (f64, f64) {
    (p[0] as f64 * SCALE + shift.0, p[1] as f64 * SCALE + shift.1)
}

fn to_mol(p: (f64, f64)) -> [f32; 2] {
    [(p.0 / SCALE) as f32, (p.1 / SCALE) as f32]
}

// ─── Reading ─────────────────────────────────────────────────────────────────

/// Whether a ChemDraw `arrow` is a curved arrow this editor draws as a curly arrow: an arc, solid,
/// one shaft, neither crossed out nor a dipole, with a full or half head and nothing at its tail.
pub(crate) fn is_curly_arrow(a: &CdxObject) -> bool {
    a.tag == OBJ_ARROW
        && a.prop_int(PROP_ANGULAR_SIZE).is_some_and(|v| v != 0)
        && !a.prop_int(PROP_NO_GO).is_some_and(|v| v > 1)
        && !a.prop_int(PROP_DIPOLE).is_some_and(|v| v != 0)
        && !a.prop_i16(PROP_ARROWHEAD_TYPE).is_some_and(|t| t != 0 && t != ARROWHEAD_SOLID)
        && a.prop_int(PROP_ARROW_SHAFT_SPACING).unwrap_or(0) == 0
        && matches!(a.prop_i16(PROP_ARROWHEAD_HEAD), Some(HEAD_FULL | HEAD_HALF_LEFT | HEAD_HALF_RIGHT))
        && matches!(a.prop_i16(PROP_ARROWHEAD_TAIL), None | Some(0 | HEAD_NONE))
        && point_3d(a, PROP_HEAD_3D).is_some()
        && point_3d(a, PROP_TAIL_3D).is_some()
}

/// Whether a legacy `graphic` is an arc arrow with no `arrow` object: one this editor draws.
pub(crate) fn is_curly_graphic(g: &CdxObject) -> bool {
    g.tag == OBJ_GRAPHIC
        && g.prop_i16(PROP_GRAPHIC_TYPE) == Some(GRAPHIC_ARC)
        && matches!(g.prop_i16(PROP_ARROW_TYPE), Some(ARROW_TYPE_HALF_HEAD | ARROW_TYPE_FULL_HEAD))
        && cdx::bounding_box(g).is_some()
}

/// Whether a ChemDraw `curve` is a curved arrow this editor draws as a curly arrow: open, with a
/// head at its end (whole or half) and none at its start, and points enough for one piece.
pub(crate) fn is_curly_curve(c: &CdxObject) -> bool {
    if c.tag != OBJ_CURVE || cdx_appearance::is_ring_fill(c) {
        return false;
    }
    let curve_type = c.prop_i16(PROP_CURVE_TYPE).unwrap_or(0);
    let head = matches!(c.prop_i16(PROP_ARROWHEAD_HEAD), Some(HEAD_FULL | HEAD_HALF_LEFT | HEAD_HALF_RIGHT))
        || curve_type & (CURVE_ARROW_AT_END | CURVE_HALF_ARROW_AT_END) != 0;
    head && curve_type & (CURVE_CLOSED | CURVE_ARROWS_AT_START) == 0
        && !c.prop(PROP_CLOSED).is_some_and(|v| v.first().is_some_and(|&b| b != 0))
        && matches!(c.prop_i16(PROP_ARROWHEAD_TAIL), None | Some(0 | HEAD_NONE))
        && !c.prop_i16(PROP_ARROWHEAD_TYPE).is_some_and(|t| t != 0 && t != ARROWHEAD_SOLID)
        && curve_knots(c).is_some_and(|k| k.len() >= 2)
}

/// A curve's knots, each as (control in, knot, control out), in molecule units.
fn curve_knots(c: &CdxObject) -> Option<Vec<[[f32; 2]; 3]>> {
    let v = c.prop(PROP_CURVE_POINTS)?;
    let n = u16::from_le_bytes([*v.first()?, *v.get(1)?]) as usize;
    if n < 3 || !n.is_multiple_of(3) {
        return None;
    }
    let int = |at: usize| v.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64 / SCALE);
    // A CDX 2D point is y, then x.
    let point = |k: usize| Some([int(2 + k * 8 + 4)? as f32, int(2 + k * 8)? as f32]);
    (0..n / 3).map(|k| Some([point(3 * k)?, point(3 * k + 1)?, point(3 * k + 2)?])).collect()
}

/// A ChemDraw `curve` arrow as a curly arrow of its shape with free ends where it starts and
/// ends, and what may tie those ends to atoms (only its tag: a curve has no source or target).
pub(crate) fn read_curve(c: &CdxObject) -> Option<(CurlyArrow, Links)> {
    let knots = curve_knots(c)?;
    let pieces: Vec<[[f32; 2]; 4]> = knots.windows(2).map(|w| [w[0][1], w[0][2], w[1][0], w[1][1]]).collect();
    let (tail, head) = (pieces[0][0], pieces[pieces.len() - 1][3]);
    let path = curly::path_of(tail, head, &pieces)?;
    let curve_type = c.prop_i16(PROP_CURVE_TYPE).unwrap_or(0);
    let kind = match c.prop_i16(PROP_ARROWHEAD_HEAD) {
        Some(HEAD_FULL) => CurlyKind::Pair,
        Some(_) => CurlyKind::Single,
        None if curve_type & CURVE_HALF_ARROW_AT_END != 0 => CurlyKind::Single,
        None => CurlyKind::Pair,
    };
    let mut arrow = CurlyArrow::new(CurlyEnd::Point(tail), CurlyEnd::Point(head), curly::DEFAULT_CURLY_ANGLE, kind);
    arrow.path = path;
    arrow.source = Some(c.id);
    let tag = cdx_appearance::tag_named(c, CURLY_TAG).and_then(cdx_appearance::tag_text).filter(|t| !t.is_empty());
    Some((arrow, Links { tag, source: None, target: None }))
}

/// What reading a curly arrow found that ties it to the drawing, applied by [`link`] once the
/// structures are read.
#[derive(Debug, Clone, Default)]
pub(crate) struct Links {
    /// The tag's value, when the arrow has one.
    tag: Option<String>,
    /// `ArrowSource` and `ArrowTarget`.
    source: Option<u32>,
    target: Option<u32>,
}

/// A ChemDraw curved `arrow` as a curly arrow with free ends where its tail and head are, and
/// what may tie those ends to atoms.
pub(crate) fn read_arrow(a: &CdxObject) -> Option<(CurlyArrow, Links)> {
    let tail = to_mol(point_3d(a, PROP_TAIL_3D)?);
    let head = to_mol(point_3d(a, PROP_HEAD_3D)?);
    // Tenths of a degree in CDX (CDXML's degrees, scaled on reading).
    let angle = a.prop_int(PROP_ANGULAR_SIZE)? as f32 / 10.0;
    let kind = match a.prop_i16(PROP_ARROWHEAD_HEAD)? {
        HEAD_FULL => CurlyKind::Pair,
        _ => CurlyKind::Single,
    };
    let mut arrow = CurlyArrow::new(CurlyEnd::Point(tail), CurlyEnd::Point(head), angle, kind);
    arrow.source = Some(a.id);
    let tag = cdx_appearance::tag_named(a, CURLY_TAG)
        .and_then(cdx_appearance::tag_text)
        .filter(|t| !t.is_empty());
    let id = |tag: u16| a.prop_u32(tag).or_else(|| a.prop_int(tag).map(|v| v as u32)).filter(|&v| v != 0);
    Some((arrow, Links { tag, source: id(PROP_ARROW_SOURCE), target: id(PROP_ARROW_TARGET) }))
}

/// A legacy arc `graphic` arrow as a curly arrow with free ends. Its box holds the head and the
/// arc's centre; the tail is the head turned back about the centre by the (rounded) angle.
pub(crate) fn read_graphic(g: &CdxObject) -> Option<CurlyArrow> {
    let b = cdx::bounding_box(g)?;
    let (head, center) = ((b[0], b[1]), (b[2], b[3]));
    let angle = g.prop_int(PROP_ANGULAR_SIZE).map_or(90.0, |v| v as f32 / 10.0);
    let (dx, dy) = (head.0 - center.0, head.1 - center.1);
    let r = (dx * dx + dy * dy).sqrt();
    if r < 1.0 {
        return None;
    }
    // Counterclockwise on the page from tail to head is a negative turn in y-down coordinates:
    // the tail is the head turned the other way.
    let a = dy.atan2(dx) + (angle as f64).to_radians();
    let tail = (center.0 + r * a.cos(), center.1 + r * a.sin());
    let kind = if g.prop_i16(PROP_ARROW_TYPE) == Some(ARROW_TYPE_HALF_HEAD) { CurlyKind::Single } else { CurlyKind::Pair };
    let mut arrow = CurlyArrow::new(CurlyEnd::Point(to_mol(tail)), CurlyEnd::Point(to_mol(head)), angle, kind);
    arrow.source = Some(g.id);
    Some(arrow)
}

/// Every bond object inside `obj`, by id: the two nodes it joins.
pub(crate) fn bond_nodes(obj: &CdxObject, out: &mut HashMap<u32, (u32, u32)>) {
    for c in &obj.children {
        if c.tag == OBJ_BOND
            && let (Some(b), Some(e)) = (c.prop_u32(PROP_BOND_BEGIN), c.prop_u32(PROP_BOND_END))
        {
            out.insert(c.id, (b, e));
        }
        bond_nodes(c, out);
    }
}

/// What the document's ids became: the atom each node was read into, and the nodes each bond
/// joins.
pub(crate) struct ReadIds<'a> {
    pub atoms_of: &'a HashMap<u32, Vec<u32>>,
    pub bonds: &'a HashMap<u32, (u32, u32)>,
}

impl ReadIds<'_> {
    fn atom(&self, node: u32) -> Option<u32> {
        self.atoms_of.get(&node).and_then(|a| (a.len() == 1).then(|| a[0]))
    }

    fn bond(&self, mol: &Molecule, id: u32) -> Option<u32> {
        let &(b, e) = self.bonds.get(&id)?;
        mol.bond_between(self.atom(b)?, self.atom(e)?).map(|b| b.id)
    }
}

/// One end as the tag names it.
fn tag_end(mol: &Molecule, ids: &ReadIds, text: &str) -> Option<CurlyEnd> {
    let mut parts = text.split(':');
    let kind = parts.next()?;
    let mut num = || parts.next().and_then(|v| v.parse::<u32>().ok());
    Some(match kind {
        "atom" => CurlyEnd::Atom(ids.atom(num()?)?),
        "bond" => {
            let bond = ids.bond(mol, num()?)?;
            let side = ids.atom(num()?)?;
            let b = mol.bond_by_id(bond)?;
            if side != b.begin && side != b.end {
                return None;
            }
            CurlyEnd::Bond { bond, side }
        }
        "between" => CurlyEnd::Between(ids.atom(num()?)?, ids.atom(num()?)?),
        _ => return None,
    })
}

/// `arrow` (with free ends, as read) as the tag says it was written: its ends, angle, skew, kind,
/// dots and free shape. `None` when the tag does not read, is of a later version, or no longer describes
/// the arrow (its ends are not where the file draws them).
fn from_tag(mol: &Molecule, ids: &ReadIds, arrow: &CurlyArrow, tag: &str) -> Option<CurlyArrow> {
    let fields: HashMap<&str, &str> = tag.split_whitespace().filter_map(|f| f.split_once('=')).collect();
    let version: u32 = fields.get("v")?.parse().ok()?;
    if version == 0 || version > TAG_VERSION {
        return None;
    }
    let mut out = arrow.clone();
    let end = |key: &str, free: CurlyEnd| -> Option<CurlyEnd> {
        match *fields.get(key)? {
            "free" => Some(free),
            text => tag_end(mol, ids, text),
        }
    };
    out.tail = end("tail", arrow.tail)?;
    out.head = end("head", arrow.head)?;
    if let Some(a) = fields.get("angle") {
        out.angle = curly::clamp_angle(a.parse().ok()?);
    }
    out.skew = fields.get("skew").map_or(Some(0.0), |s| s.parse().ok()).map(curly::clamp_skew)?;
    out.kind = match fields.get("kind").copied() {
        Some("single") => CurlyKind::Single,
        Some("pair") | None => CurlyKind::Pair,
        Some(_) => return None,
    };
    out.dots = fields.get("dots").is_some_and(|d| *d == "1");
    out.path = match fields.get("path") {
        Some(text) => text
            .split(';')
            .map(|p| {
                let (s, t) = p.split_once(',')?;
                Some([s.parse::<f32>().ok().filter(|v| v.is_finite())?, t.parse::<f32>().ok().filter(|v| v.is_finite())?])
            })
            .collect::<Option<Vec<_>>>()?,
        None => Vec::new(),
    };
    if !out.path.is_empty() && !out.is_shaped() {
        return None;
    }
    // Still the arrow the file draws?
    let path = curly::geometry(mol, &out)?;
    let (CurlyEnd::Point(tail), CurlyEnd::Point(head)) = (arrow.tail, arrow.head) else { return None };
    let near = |p: [f32; 2], q: [f32; 2]| ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt() <= TAG_TOLERANCE;
    (near(path.tail, tail) && near(path.head, head)).then_some(out)
}

/// What ChemDraw's `ArrowSource` / `ArrowTarget` id names: a node's atom, or a bond (a tail on it
/// standing toward the atom nearer the drawn point `at`).
fn native_end(mol: &Molecule, ids: &ReadIds, id: u32, at: [f32; 2]) -> Option<CurlyEnd> {
    if let Some(atom) = ids.atom(id) {
        return Some(CurlyEnd::Atom(atom));
    }
    let bond = mol.bond_by_id(ids.bond(mol, id)?)?;
    let dist = |a: u32| mol.atom_by_id(a).map_or(f32::INFINITY, |a| (a.pos[0] - at[0]).powi(2) + (a.pos[1] - at[1]).powi(2));
    let side = if dist(bond.end) < dist(bond.begin) { bond.end } else { bond.begin };
    Some(CurlyEnd::Bond { bond: bond.id, side })
}

/// Tie `arrow`, read with free ends, to the atoms and bonds of `mol` (the structures read with
/// it): as its tag says, when the tag holds; else as ChemDraw's own source and target say, or
/// where its ends clearly stand.
pub(crate) fn link(arrow: &mut CurlyArrow, links: &Links, mol: &Molecule, ids: &ReadIds) {
    if let Some(tagged) = links.tag.as_deref().and_then(|t| from_tag(mol, ids, arrow, t)) {
        *arrow = tagged;
        return;
    }
    let (CurlyEnd::Point(tail), CurlyEnd::Point(head)) = (arrow.tail, arrow.head) else { return };
    let tie = |id: Option<u32>, at: [f32; 2], is_tail: bool| {
        id.and_then(|id| native_end(mol, ids, id, at))
            .or_else(|| curly::end_near(mol, at, is_tail))
            .unwrap_or(CurlyEnd::Point(at))
    };
    let drawn = curly::pieces(mol, arrow);
    let read = arrow.clone();
    arrow.tail = tie(links.source, tail, true);
    arrow.head = tie(links.target, head, false);
    // A tail and a head on one atom is no arrow to redraw: keep it as ChemDraw drew it.
    if arrow.tail == arrow.head || (matches!(arrow.tail, CurlyEnd::Atom(a) if arrow.head == CurlyEnd::Atom(a))) {
        *arrow = read;
        return;
    }
    // A free shape keeps its control points where the file has them, now between the anchors
    // of the ends it is tied to.
    if let (Some(pieces), Some((at, ah))) = (drawn, curly::anchors(mol, arrow))
        && let Some(path) = curly::path_of(at, ah, &pieces)
    {
        arrow.path = path;
    }
}

// ─── Writing ─────────────────────────────────────────────────────────────────

/// The tag's text for one end; `None` when what it stands on was not written as a node or
/// bond, so it is written free.
fn end_text(end: &CurlyEnd, ids: &WrittenIds) -> Option<String> {
    let node = |a: u32| ids.node_of.get(&a).copied();
    Some(match *end {
        CurlyEnd::Atom(a) => format!("atom:{}", node(a)?),
        CurlyEnd::Bond { bond, side } => format!("bond:{}:{}", ids.bond_of.get(&bond)?, node(side)?),
        CurlyEnd::Between(a, b) => format!("between:{}:{}", node(a)?, node(b)?),
        CurlyEnd::Point(_) => return None,
    })
}

/// The id ChemDraw's `ArrowSource` / `ArrowTarget` takes for an end: a node, or a bond.
fn native_id(end: &CurlyEnd, ids: &WrittenIds) -> Option<u32> {
    match *end {
        CurlyEnd::Atom(a) => ids.node_of.get(&a).copied(),
        CurlyEnd::Bond { bond, .. } => ids.bond_of.get(&bond).copied(),
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => None,
    }
}

/// The tag's value for `arrow`.
fn tag_value(arrow: &CurlyArrow, ids: &WrittenIds) -> String {
    let end = |e: &CurlyEnd| end_text(e, ids).unwrap_or_else(|| "free".to_string());
    let mut value = format!(
        "v={TAG_VERSION} tail={} head={} angle={} skew={} kind={} dots={}",
        end(&arrow.tail),
        end(&arrow.head),
        arrow.angle,
        arrow.skew,
        match arrow.kind {
            CurlyKind::Pair => "pair",
            CurlyKind::Single => "single",
        },
        u8::from(arrow.dots),
    );
    if arrow.is_shaped() {
        let points: Vec<String> = arrow.path.iter().map(|p| format!("{},{}", p[0], p[1])).collect();
        value.push_str(&format!(" path={}", points.join(";")));
    }
    value
}

/// The arc ChemDraw draws for `arrow`: its tail, head and centre (CDX units), its radius and the
/// angle written. `None` when the arrow cannot be placed.
struct Arc {
    path: curly::CurlyPath,
    tail: (f64, f64),
    head: (f64, f64),
    center: (f64, f64),
    radius: f64,
    angle: f32,
    points: Vec<(f64, f64)>,
}

fn arc_of(mol: &Molecule, arrow: &CurlyArrow, shift: (f64, f64)) -> Option<Arc> {
    let path = curly::geometry(mol, arrow)?;
    let angle = if arrow.angle.abs() < MIN_WRITTEN_ANGLE { MIN_WRITTEN_ANGLE.copysign(arrow.angle) } else { arrow.angle };
    let (tail, head) = (to_cdx(path.tail, shift), to_cdx(path.head, shift));
    // The circle through tail and head turning `angle`, as `curly::curve_point` lays it.
    let (dx, dy) = (head.0 - tail.0, head.1 - tail.1);
    let c = (dx * dx + dy * dy).sqrt();
    if c < 1.0 {
        return None;
    }
    let (ux, uy) = (dx / c, dy / c);
    let half = (angle.abs() as f64).to_radians() / 2.0;
    let radius = c / (2.0 * half.sin());
    let s = if angle > 0.0 { 1.0 } else { -1.0 };
    let (bx, by) = (-uy * s, ux * s);
    let mid = ((tail.0 + head.0) / 2.0, (tail.1 + head.1) / 2.0);
    let center = (mid.0 - bx * radius * half.cos(), mid.1 - by * radius * half.cos());
    let points = (0..=32)
        .map(|k| {
            let p = curly::curve_point(path.tail, path.head, angle, 0.0, k as f32 / 32.0);
            to_cdx(p, shift)
        })
        .collect();
    Some(Arc { path, tail, head, center, radius, angle, points })
}

/// `ArrowheadHead` for `arrow` drawn along `path`: a fish-hook's barb on the outer side of its
/// last turn, as the editor draws it ([`curly::barb_left`]).
fn head_value(arrow: &CurlyArrow, path: &curly::CurlyPath) -> i16 {
    match arrow.kind {
        CurlyKind::Pair => HEAD_FULL,
        CurlyKind::Single if curly::barb_left(arrow, path) => HEAD_HALF_LEFT,
        CurlyKind::Single => HEAD_HALF_RIGHT,
    }
}

/// ChemDraw's rounding of an arc graphic's angle: 90, 120, 180 or 270°, keeping the sign.
fn graphic_angle(angle: f32) -> i16 {
    let a = angle.abs();
    let nearest = [90.0f32, 120.0, 180.0, 270.0].into_iter().min_by(|x, y| (x - a).abs().total_cmp(&(y - a).abs())).unwrap();
    (nearest.copysign(angle) * 10.0) as i16
}

/// Set everything this editor says about a curved arrow on `a` (a new `arrow` object or the one
/// it was read from) and on `g`, the graphic it supersedes.
fn set_curly(a: &mut CdxObject, g: Option<&mut CdxObject>, arrow: &CurlyArrow, arc: &Arc, ids: &WrittenIds, next: &mut u32) {
    let line = 0.6 * PT;
    let mut b: Option<[f64; 4]> = None;
    for p in &arc.points {
        b = Some(cdx::union(b, [p.0, p.1, p.0, p.1]));
    }
    let pad = line / 2.0 + CURLY_HEAD_HALF_WIDTH as f64 * line;
    let [l, t, r, bt] = b.expect("an arc has points");
    a.set(PROP_BOUNDING_BOX, cdx::rect([l - pad, t - pad, r + pad, bt + pad]));
    a.set(PROP_FILL_TYPE, i16_bytes(FILL_NONE));
    a.set(PROP_ARROWHEAD_HEAD, i16_bytes(head_value(arrow, &arc.path)));
    a.props.retain(|(t, _)| *t != PROP_ARROWHEAD_TAIL && *t != PROP_ARROW_SHAFT_SPACING);
    a.set(PROP_ARROWHEAD_TYPE, i16_bytes(ARROWHEAD_SOLID));
    a.set(PROP_HEAD_SIZE, i16_bytes(hundredths(CURLY_HEAD_LENGTH)));
    a.set(PROP_ARROWHEAD_CENTER_SIZE, i16_bytes(hundredths(CURLY_HEAD_CENTER)));
    a.set(PROP_ARROWHEAD_WIDTH, i16_bytes(hundredths(CURLY_HEAD_HALF_WIDTH)));
    a.set(PROP_ANGULAR_SIZE, i16_bytes((arc.angle * 10.0).round() as i16));
    match native_id(&arrow.tail, ids) {
        Some(id) => a.set(PROP_ARROW_SOURCE, id.to_le_bytes().to_vec()),
        None => a.props.retain(|(t, _)| *t != PROP_ARROW_SOURCE),
    }
    match native_id(&arrow.head, ids) {
        Some(id) => a.set(PROP_ARROW_TARGET, id.to_le_bytes().to_vec()),
        None => a.props.retain(|(t, _)| *t != PROP_ARROW_TARGET),
    }
    a.set(PROP_HEAD_3D, point_3d_value(arc.head));
    a.set(PROP_TAIL_3D, point_3d_value(arc.tail));
    a.set(PROP_CENTER_3D, point_3d_value(arc.center));
    a.set(PROP_MAJOR_AXIS_END_3D, point_3d_value((arc.center.0 + arc.radius, arc.center.1)));
    a.set(PROP_MINOR_AXIS_END_3D, point_3d_value((arc.center.0, arc.center.1 + arc.radius)));
    // The tag: the one read is replaced.
    a.children.retain(|c| !cdx_appearance::is_tag_named(c, CURLY_TAG));
    *next += 1;
    a.children.push(cdx_appearance::object_tag(*next, CURLY_TAG, Some(&tag_value(arrow, ids))));
    if let Some(g) = g {
        g.set(PROP_SUPERSEDED_BY, a.id.to_le_bytes().to_vec());
        g.set(PROP_BOUNDING_BOX, cdx::rect([arc.head.0, arc.head.1, arc.center.0, arc.center.1]));
        g.set(PROP_GRAPHIC_TYPE, i16_bytes(GRAPHIC_ARC));
        let arrow_type = if arrow.kind == CurlyKind::Single { ARROW_TYPE_HALF_HEAD } else { ARROW_TYPE_FULL_HEAD };
        g.set(PROP_ARROW_TYPE, i16_bytes(arrow_type));
        g.set(PROP_HEAD_SIZE, i16_bytes(hundredths(CURLY_HEAD_LENGTH)));
        g.set(PROP_ANGULAR_SIZE, i16_bytes(graphic_angle(arc.angle)));
    }
}

/// Set everything this editor says about a curly arrow with a free shape on `c` (a new `curve`
/// or the one it was read from): the curve as it is drawn, cut back at the ends, moved by
/// `shift`, its head, and the tag. False when the arrow cannot be placed.
fn set_curve(c: &mut CdxObject, mol: &Molecule, arrow: &CurlyArrow, ids: &WrittenIds, shift: (f64, f64), next: &mut u32) -> bool {
    let (Some(pieces), Some(path)) = (curly::drawn_pieces(mol, arrow), curly::geometry(mol, arrow)) else { return false };
    // Each knot as (control in, knot, control out); the ends' outer controls are the ends.
    let mut knots: Vec<[[f32; 2]; 3]> = vec![[pieces[0][0], pieces[0][0], pieces[0][1]]];
    for (i, b) in pieces.iter().enumerate() {
        let out = pieces.get(i + 1).map_or(b[3], |n| n[1]);
        knots.push([b[2], b[3], out]);
    }
    let mut points = ((knots.len() * 3) as u16).to_le_bytes().to_vec();
    let mut bbox: Option<[f64; 4]> = None;
    for k in &knots {
        for p in k {
            let (x, y) = to_cdx(*p, shift);
            points.extend(cdx::point(x, y));
        }
    }
    for p in &path.points {
        let (x, y) = to_cdx(*p, shift);
        bbox = Some(cdx::union(bbox, [x, y, x, y]));
    }
    let line = 0.6 * PT;
    let pad = line / 2.0 + CURLY_HEAD_HALF_WIDTH as f64 * line;
    let [l, t, r, bt] = bbox.expect("a curve has points");
    c.set(PROP_BOUNDING_BOX, cdx::rect([l - pad, t - pad, r + pad, bt + pad]));
    c.set(PROP_CURVE_POINTS, points);
    // An open curve: what made the one read closed or headed at its start is dropped.
    if let Some(t) = c.prop_i16(PROP_CURVE_TYPE) {
        c.set(PROP_CURVE_TYPE, i16_bytes(t & !(CURVE_CLOSED | CURVE_ARROWS_AT_START | CURVE_ARROW_AT_END | CURVE_HALF_ARROW_AT_END)));
    }
    c.props.retain(|(t, _)| *t != PROP_CLOSED && *t != PROP_ARROWHEAD_TAIL);
    c.set(PROP_FILL_TYPE, i16_bytes(FILL_NONE));
    c.set(PROP_ARROWHEAD_HEAD, i16_bytes(head_value(arrow, &path)));
    c.set(PROP_ARROWHEAD_TYPE, i16_bytes(ARROWHEAD_SOLID));
    c.set(PROP_HEAD_SIZE, i16_bytes(hundredths(CURLY_HEAD_LENGTH)));
    c.set(PROP_ARROWHEAD_CENTER_SIZE, i16_bytes(hundredths(CURLY_HEAD_CENTER)));
    c.set(PROP_ARROWHEAD_WIDTH, i16_bytes(hundredths(CURLY_HEAD_HALF_WIDTH)));
    c.children.retain(|o| !cdx_appearance::is_tag_named(o, CURLY_TAG));
    *next += 1;
    c.children.push(cdx_appearance::object_tag(*next, CURLY_TAG, Some(&tag_value(arrow, ids))));
    true
}

/// The ChemDraw objects for `arrow` on `mol`, moved by `shift`: the objects it was read from
/// (`original`, rewritten), or new ones with new ids from `next` — an arc graphic and the
/// `arrow` superseding it for an arc, a `curve` for a free shape. An arrow whose shape changed
/// kind since it was read is written new. Nothing when the arrow cannot be placed.
pub(crate) fn write(
    mol: &Molecule,
    arrow: &CurlyArrow,
    original: Option<Vec<CdxObject>>,
    ids: &WrittenIds,
    shift: (f64, f64),
    next: &mut u32,
) -> Vec<CdxObject> {
    let original = original.unwrap_or_default();
    let read_as_curve = original.iter().any(|o| o.tag == OBJ_CURVE);
    if arrow.is_shaped() {
        let mut c = match original.into_iter().find(|o| o.tag == OBJ_CURVE) {
            Some(c) => c,
            None => {
                *next += 1;
                let mut c = CdxObject::new(OBJ_CURVE, *next);
                c.set(PROP_BOUNDING_BOX, Vec::new()); // first, as ChemDraw orders it; set below
                c
            }
        };
        return if set_curve(&mut c, mol, arrow, ids, shift, next) { vec![c] } else { Vec::new() };
    }
    let Some(arc) = arc_of(mol, arrow, shift) else { return Vec::new() };
    let mut objects = if read_as_curve { Vec::new() } else { original };
    if !objects.iter().any(|o| o.tag == OBJ_ARROW) {
        // Read from a lone graphic, or new: a new arrow object, and a graphic for it.
        if !objects.iter().any(|o| o.tag == OBJ_GRAPHIC) {
            *next += 1;
            objects.push(CdxObject::new(OBJ_GRAPHIC, *next));
        }
        *next += 1;
        let mut a = CdxObject::new(OBJ_ARROW, *next);
        a.set(PROP_BOUNDING_BOX, Vec::new()); // first, as ChemDraw orders it; set below
        objects.push(a);
    }
    let arrow_at = objects.iter().position(|o| o.tag == OBJ_ARROW).expect("an arrow object");
    let mut a = objects.remove(arrow_at);
    set_curly(&mut a, objects.iter_mut().find(|o| o.tag == OBJ_GRAPHIC), arrow, &arc, ids, next);
    objects.insert(arrow_at, a);
    objects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx::{merge_into_cdx, molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml};
    use crate::molecule::cdx_codec;
    use crate::molecule::{BondOrder, PageObject};

    /// Acetone's carbonyl (C=O, the carbon bonded to two methyls) beside a hydroxide O,
    /// with the four kinds of end.
    fn mechanism() -> (Molecule, Vec<CurlyArrow>) {
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [9.0, 7.0], 0);
        let o = m.add_atom("O".into(), [9.0, 5.5], 0);
        let me1 = m.add_atom("C".into(), [7.7, 7.75], 0);
        let me2 = m.add_atom("C".into(), [10.3, 7.75], 0);
        let oh = m.add_atom("O".into(), [12.5, 6.0], -1);
        let co = m.add_bond(c, o, BondOrder::Double);
        m.add_bond(c, me1, BondOrder::Single);
        m.add_bond(c, me2, BondOrder::Single);
        let arrows = vec![
            CurlyArrow::new(CurlyEnd::Atom(oh), CurlyEnd::Between(oh, c), -120.0, CurlyKind::Pair),
            CurlyArrow { skew: 0.3, ..CurlyArrow::new(CurlyEnd::Bond { bond: co, side: c }, CurlyEnd::Atom(o), 150.0, CurlyKind::Pair) },
            CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Bond { bond: co, side: o }, -90.0, CurlyKind::Single),
            CurlyArrow::new(CurlyEnd::Point([13.0, 9.0]), CurlyEnd::Atom(me2), 60.0, CurlyKind::Single),
        ];
        for a in &arrows {
            m.add_curly_arrow(a.clone());
        }
        (m, arrows)
    }

    fn curlies(m: &Molecule) -> Vec<CurlyArrow> {
        m.curly_arrows().cloned().collect()
    }

    /// The same arrows: the same ends (by the atoms' positions, as ids change), angle, skew,
    /// kind and dots.
    fn same_arrows(a: &Molecule, xs: &[CurlyArrow], b: &Molecule, ys: &[CurlyArrow]) {
        assert_eq!(xs.len(), ys.len());
        let pos = |m: &Molecule, id: u32| m.atom_by_id(id).map(|a| [(a.pos[0] * 100.0).round() as i32, (a.pos[1] * 100.0).round() as i32]);
        let end = |m: &Molecule, e: &CurlyEnd| match *e {
            CurlyEnd::Atom(x) => format!("atom {:?}", pos(m, x)),
            CurlyEnd::Bond { bond, side } => {
                let b = m.bond_by_id(bond).unwrap();
                let mut ends = [pos(m, b.begin), pos(m, b.end)];
                ends.sort();
                format!("bond {ends:?} side {:?}", pos(m, side))
            }
            CurlyEnd::Between(x, y) => format!("between {:?} {:?}", pos(m, x), pos(m, y)),
            CurlyEnd::Point(p) => format!("point {:.2} {:.2}", p[0], p[1]),
        };
        for (x, y) in xs.iter().zip(ys) {
            assert_eq!((end(a, &x.tail), end(a, &x.head)), (end(b, &y.tail), end(b, &y.head)), "{x:?} vs {y:?}");
            if x.is_shaped() {
                assert_eq!(x.path.len(), y.path.len(), "{x:?} vs {y:?}");
                for (p, q) in x.path.iter().zip(&y.path) {
                    assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3, "{x:?} vs {y:?}");
                }
            } else {
                assert!(!y.is_shaped(), "{y:?}");
                assert!((x.angle - y.angle).abs() < 0.05 && (x.skew - y.skew).abs() < 1e-4, "{x:?} vs {y:?}");
            }
            assert_eq!((x.kind, x.dots), (y.kind, y.dots));
        }
    }

    #[test]
    fn curly_arrows_come_back_from_cdx_and_cdxml_exactly() {
        let (m, arrows) = mechanism();
        let from_cdx = read_cdx(&molecule_to_cdx_document(&m)).unwrap();
        same_arrows(&m, &arrows, &from_cdx.molecule, &curlies(&from_cdx.molecule));
        assert_eq!(from_cdx.other_objects, 0, "the graphics go with their arrows");
        let xml = molecule_to_cdxml(&m);
        let from_xml = read_cdxml(&xml).unwrap();
        same_arrows(&m, &arrows, &from_xml.molecule, &curlies(&from_xml.molecule));
        // Saved again, still the same.
        let again = read_cdx(&molecule_to_cdx_document(&from_cdx.molecule)).unwrap();
        same_arrows(&m, &arrows, &again.molecule, &curlies(&again.molecule));
    }

    #[test]
    fn the_cdxml_is_chemdraws_curved_arrow_with_an_inert_tag() {
        let (m, _) = mechanism();
        let xml = molecule_to_cdxml(&m);
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert!(cdx_file_rs::file::CdxFile::from_bytes(&molecule_to_cdx_document(&m)).is_ok());
        for needle in [
            "AngularSize=\"-120\"",
            "AngularSize=\"150\"",
            "ArrowheadHead=\"Full\"",
            "ArrowheadHead=\"HalfLeft\"",
            "ArrowheadHead=\"HalfRight\"",
            "GraphicType=\"Arc\"",
            "ArrowSource=",
            "ArrowTarget=",
            "Name=\"ChemBuilder/CurlyArrow\"",
            "v=2 tail=atom:",
            "head=between:",
            "tail=bond:",
        ] {
            assert!(xml.contains(needle), "{needle}: {xml}");
        }
        // The arc's centre and axes describe the circle through tail and head.
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        let arrow = doc.children[0].children.iter().find(|o| o.tag == OBJ_ARROW).unwrap();
        let (t, h, c) = (point_3d(arrow, PROP_TAIL_3D).unwrap(), point_3d(arrow, PROP_HEAD_3D).unwrap(), point_3d(arrow, PROP_CENTER_3D).unwrap());
        let d = |p: (f64, f64)| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt();
        assert!((d(t) - d(h)).abs() < 2.0 && (d(point_3d(arrow, PROP_MAJOR_AXIS_END_3D).unwrap()) - d(t)).abs() < 2.0);
    }

    /// Every tag stripped: what a reader that ignores it (or a ChemDraw that lost it) sees.
    fn strip_tags(o: &mut CdxObject) {
        o.children.retain(|c| !cdx_appearance::is_tag_named(c, CURLY_TAG));
        o.children.iter_mut().for_each(strip_tags);
    }

    #[test]
    fn without_the_tag_chemdraws_source_and_target_still_tie_the_arrows() {
        let (m, _) = mechanism();
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        strip_tags(&mut doc);
        let read = read_cdx(&cdx_codec::write(&doc)).unwrap().molecule;
        let a = curlies(&read);
        assert_eq!(a.len(), 4);
        // Lone pair → (a gap: no ChemDraw word, so the head is free).
        assert!(matches!(a[0].tail, CurlyEnd::Atom(_)) && matches!(a[0].head, CurlyEnd::Point(_)), "{:?}", a[0]);
        // Bond → atom, the bond's tail on the carbon's side, as drawn.
        let CurlyEnd::Bond { bond, side } = a[1].tail else { panic!("{:?}", a[1]) };
        assert_eq!(read.atom_by_id(side).unwrap().element, "C");
        assert!(read.bond_by_id(bond).is_some() && matches!(a[1].head, CurlyEnd::Atom(_)));
        assert!((a[1].angle - 150.0).abs() < 0.05 && a[1].skew == 0.0, "no skew without the tag");
        assert_eq!(a[2].kind, CurlyKind::Single);
        assert!(matches!(a[3].tail, CurlyEnd::Point(_)) && matches!(a[3].head, CurlyEnd::Atom(_)));
    }

    #[test]
    fn a_tag_that_no_longer_fits_the_arrow_is_not_trusted() {
        let (m, _) = mechanism();
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        // ChemDraw moved the first arrow away (its tag says it stands on the hydroxide).
        let page = &mut doc.children[0];
        let arrow = page.children.iter_mut().find(|o| o.tag == OBJ_ARROW).unwrap();
        for tag in [PROP_TAIL_3D, PROP_HEAD_3D] {
            let p = point_3d(arrow, tag).unwrap();
            arrow.set(tag, point_3d_value((p.0 + 200.0 * PT, p.1 + 200.0 * PT)));
        }
        arrow.props.retain(|(t, _)| *t != PROP_ARROW_SOURCE && *t != PROP_ARROW_TARGET);
        let read = read_cdx(&cdx_codec::write(&doc)).unwrap().molecule;
        let a = &curlies(&read)[0];
        assert!(matches!((a.tail, a.head), (CurlyEnd::Point(_), CurlyEnd::Point(_))), "{a:?}");
    }

    #[test]
    fn a_chemdraw_curved_arrow_reads_as_drawn() {
        // ChemDraw 23 (OrgChemBG's Mg.cdxml): a −30° arrow from a methyl's node to a bond, and
        // ChemDraw 12's 180° arrows with no source or target.
        let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<CDXML><page id="1">
<fragment id="2"><n id="51" p="178.44 138.65"/><n id="52" p="200 150"/><n id="23" p="40 170"/><n id="31" p="70 175"/>
<b id="30" B="51" E="52"/><b id="32" B="23" E="31" Order="2"/></fragment>
<arrow id="69" BoundingBox="57.51 144.68 187.26 166.39" FillType="None" ArrowheadHead="Full" ArrowheadType="Solid" HeadSize="400" ArrowheadCenterSize="400" ArrowheadWidth="200" AngularSize="-30" ArrowSource="51" ArrowTarget="32" Head3D="57.51 164.25 0" Tail3D="186.77 145.17 0" Center3D="86.53 -86.48 0" MajorAxisEnd3D="338.97 -86.48 0" MinorAxisEnd3D="86.53 165.95 0"/>
<graphic id="64" SupersededBy="69" BoundingBox="57.51 164.25 86.53 -86.48" GraphicType="Arc" ArrowType="FullHead" HeadSize="1000" AngularSize="-90"/>
<graphic id="96" BoundingBox="199.02 84 173.15 84" GraphicType="Arc" ArrowType="FullHead" HeadSize="1000" AngularSize="180"/>
</page></CDXML>"#;
        let import = read_cdxml(xml).unwrap();
        let m = &import.molecule;
        let a = curlies(m);
        assert_eq!(a.len(), 2, "{a:?}");
        assert_eq!(import.other_objects, 0);
        assert!((a[0].angle + 30.0).abs() < 1e-3);
        assert!(matches!(a[0].tail, CurlyEnd::Atom(x) if m.atom_by_id(x).unwrap().pos[0] > 18.0), "{:?}", a[0]);
        let CurlyEnd::Bond { bond, .. } = a[0].head else { panic!("{:?}", a[0]) };
        assert_eq!(m.bond_by_id(bond).unwrap().order, BondOrder::Double);
        // The lone arc graphic: its tail the head turned back 180° about the centre.
        let CurlyEnd::Point(tail) = a[1].tail else { panic!() };
        assert!((tail[0] * 9.6 - 147.28).abs() < 0.05 && (tail[1] * 9.6 - 84.0).abs() < 0.05, "{tail:?}");
        assert!((a[1].angle - 180.0).abs() < 1e-3);
        // Saved into the document and read again: the same arrows.
        let bytes = cdx_codec::write(&crate::molecule::cdx::cdxml_document(xml).unwrap());
        let again = read_cdx(&merge_into_cdx(&bytes, m)).unwrap();
        same_arrows(m, &a, &again.molecule, &curlies(&again.molecule));
        assert_eq!(again.other_objects, 0);
        let _ = PageObject::Curly(a[0].clone());
    }

    /// The user's sketch: a ketone C=O, an H up and to the right, a methoxide down and to the
    /// right; an S from the C=O bond (O's side) to the H, and a two-piece route from the
    /// methoxide's lone pair round the bottom into the carbon from below.
    fn shaped() -> (Molecule, Vec<CurlyArrow>) {
        let mut m = Molecule::default();
        let c = m.add_atom("C".into(), [9.0, 9.0], 0);
        let o = m.add_atom("O".into(), [9.0, 7.5], 0);
        let me1 = m.add_atom("C".into(), [7.7, 9.75], 0);
        let me2 = m.add_atom("C".into(), [10.3, 9.75], 0);
        let h = m.add_atom("H".into(), [11.4, 6.6], 0);
        let nu = m.add_atom("O".into(), [12.5, 10.8], -1);
        let co = m.add_bond(c, o, BondOrder::Double);
        m.add_bond(c, me1, BondOrder::Single);
        m.add_bond(c, me2, BondOrder::Single);
        let s = CurlyArrow { path: vec![[0.3, 0.6], [0.6, -0.6]], ..CurlyArrow::new(CurlyEnd::Bond { bond: co, side: o }, CurlyEnd::Atom(h), 90.0, CurlyKind::Pair) };
        let route = CurlyArrow {
            path: vec![[0.1, 0.4], [0.4, 0.8], [0.6, 0.6], [0.8, 0.4], [1.1, 0.2]],
            ..CurlyArrow::new(CurlyEnd::Atom(nu), CurlyEnd::Atom(c), 90.0, CurlyKind::Pair)
        };
        let hook = CurlyArrow { path: vec![[0.2, -0.5], [0.8, -0.5]], ..CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Between(o, h), 90.0, CurlyKind::Single) };
        let arrows = vec![s, route, hook];
        for a in &arrows {
            m.add_curly_arrow(a.clone());
        }
        (m, arrows)
    }

    #[test]
    fn shaped_arrows_are_chemdraw_curves_and_come_back_exactly() {
        let (m, arrows) = shaped();
        let xml = molecule_to_cdxml(&m);
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert_eq!(xml.matches("<curve").count(), 3, "{xml}");
        assert!(!xml.contains("<arrow"), "no arc for a free shape: {xml}");
        for needle in ["CurvePoints=", "ArrowheadHead=\"Full\"", "ArrowheadType=\"Solid\"", "v=2 tail=bond:", "path=0.3,0.6;0.6,-0.6"] {
            assert!(xml.contains(needle), "{needle}: {xml}");
        }
        assert!(xml.contains("ArrowheadHead=\"HalfLeft\"") || xml.contains("ArrowheadHead=\"HalfRight\""), "{xml}");
        let from_cdx = read_cdx(&molecule_to_cdx_document(&m)).unwrap();
        same_arrows(&m, &arrows, &from_cdx.molecule, &curlies(&from_cdx.molecule));
        assert_eq!(from_cdx.other_objects, 0);
        let from_xml = read_cdxml(&xml).unwrap();
        same_arrows(&m, &arrows, &from_xml.molecule, &curlies(&from_xml.molecule));
        // Saved into the document it came from, each curve keeps its id.
        let bytes = molecule_to_cdx_document(&m);
        let ids: Vec<Option<u32>> = curlies(&from_cdx.molecule).iter().map(|c| c.source).collect();
        let again = read_cdx(&merge_into_cdx(&bytes, &from_cdx.molecule)).unwrap();
        same_arrows(&m, &arrows, &again.molecule, &curlies(&again.molecule));
        assert_eq!(curlies(&again.molecule).iter().map(|c| c.source).collect::<Vec<_>>(), ids);
        assert_eq!(again.other_objects, 0);
    }

    #[test]
    fn the_curve_is_drawn_where_the_editor_draws_it() {
        let (m, arrows) = shaped();
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        let curve = doc.children[0].children.iter().find(|o| o.tag == OBJ_CURVE).unwrap();
        let knots = curve_knots(curve).unwrap();
        let drawn = curly::geometry(&m, &m.curly_arrows().next().unwrap().clone()).unwrap();
        // The document may be shifted onto the page; compare the shape, not the place.
        let shift = [knots[0][1][0] - drawn.tail[0], knots[0][1][1] - drawn.tail[1]];
        let end = knots[knots.len() - 1][1];
        assert!((end[0] - shift[0] - drawn.head[0]).abs() < 0.02 && (end[1] - shift[1] - drawn.head[1]).abs() < 0.02, "{end:?} {shift:?} {drawn:?}");
        let _ = arrows;
    }

    #[test]
    fn without_the_tag_a_curve_keeps_its_shape_and_is_tied_where_clear() {
        let (m, _) = shaped();
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        strip_tags(&mut doc);
        let read = read_cdx(&cdx_codec::write(&doc)).unwrap().molecule;
        let a = curlies(&read);
        assert_eq!(a.len(), 3);
        assert!(a.iter().all(|c| c.is_shaped()));
        // Each is drawn as the file has it, whatever it was tied to.
        let (written, back) = (curly::geometry(&m, &curlies(&m)[1]).unwrap(), curly::geometry(&read, &a[1]).unwrap());
        let shift = [back.points[0][0] - written.points[0][0], back.points[0][1] - written.points[0][1]];
        for p in [0.25f32, 0.5, 0.75] {
            let q = written.points[(p * (written.points.len() - 1) as f32) as usize];
            let nearest = back.points.iter().map(|b| ((b[0] - q[0] - shift[0]).powi(2) + (b[1] - q[1] - shift[1]).powi(2)).sqrt()).fold(f32::INFINITY, f32::min);
            assert!(nearest < 0.25, "{p}: {nearest}");
        }
    }

    #[test]
    fn a_version_1_tag_still_reads() {
        let (m, arrows) = mechanism();
        let xml = molecule_to_cdxml(&m).replace("v=2 ", "v=1 ");
        let read = read_cdxml(&xml).unwrap();
        same_arrows(&m, &arrows, &read.molecule, &curlies(&read.molecule));
        // A later version than this reader knows is not trusted.
        let read = read_cdxml(&molecule_to_cdxml(&m).replace("v=2 ", "v=9 ")).unwrap();
        assert_eq!(curlies(&read.molecule)[1].skew, 0.0, "the skew only the tag carries is gone");
    }

    #[test]
    fn a_chemdraw_bezier_arrow_reads_as_a_shaped_curly_arrow() {
        // A curved arrow drawn with ChemDraw's freehand-curve tool: one piece from the lower
        // right up over to a bromine atom, as CDXML writes it (each knot as control in, knot,
        // control out), and a fish-hook on a closed curve, which is not an arrow.
        let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<CDXML><page id="1">
<fragment id="2"><n id="3" p="100 100" Element="35"/><n id="4" p="114.4 100"/><b id="5" B="3" E="4"/></fragment>
<curve id="10" CurvePoints="140 130 140 130 150 100 120 80 104 94 104 94" FillType="None" ArrowheadHead="Full" ArrowheadType="Solid"/>
<curve id="11" CurvePoints="60 60 60 60 70 50 80 60 80 70 80 70" ArrowheadHead="HalfLeft" ArrowheadType="Solid"/>
<curve id="12" CurveType="1" CurvePoints="10 10 10 10 20 10 20 20 20 20 10 20" ArrowheadHead="Full"/>
</page></CDXML>"#;
        let import = read_cdxml(xml).unwrap();
        let a = curlies(&import.molecule);
        assert_eq!(a.len(), 2, "{a:?}");
        assert_eq!(import.other_objects, 1, "the closed curve stays a shape");
        assert!(a[0].is_shaped() && a[0].kind == CurlyKind::Pair);
        // Its head is next to the bromine: tied to it.
        let CurlyEnd::Atom(br) = a[0].head else { panic!("{:?}", a[0]) };
        assert_eq!(import.molecule.atom_by_id(br).unwrap().element, "Br");
        assert!(matches!(a[0].tail, CurlyEnd::Point(_)));
        assert_eq!(a[1].kind, CurlyKind::Single);
        // Drawn through the file's control points: its middle is where the file's curve's is.
        let path = curly::geometry(&import.molecule, &a[1]).unwrap();
        let mid = bezier_point_pt([[60.0, 60.0], [70.0, 50.0], [80.0, 60.0], [80.0, 70.0]], 0.5);
        let m = path.points[path.points.len() / 2];
        assert!((m[0] * 9.6 - mid[0]).abs() < 0.5 && (m[1] * 9.6 - mid[1]).abs() < 0.5, "{m:?} {mid:?}");
    }

    fn bezier_point_pt(b: [[f32; 2]; 4], t: f32) -> [f32; 2] {
        curly::bezier_point(&b, t)
    }

}
