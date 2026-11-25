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
//! and `ArrowTarget` hold the id of a node or a bond. Any other curved arrow is a Bézier
//! `curve` (`CurvePoints`: each knot as control-in, knot, control-out) with arrowheads.
//!
//! **Writing** puts each curly arrow on the page as it is drawn here: an arc as an `arrow` (with
//! the graphic it supersedes, and `ArrowSource` / `ArrowTarget` for an end on an atom or a
//! bond), any other shape — the automatic S of a bond carried by an atom to a partner, a shape
//! set by hand — as a `curve`. So ChemDraw shows the arrow this editor shows. What ChemDraw has
//! no word for, the arrow's meaning, goes into an object tag on it, [`CURLY_TAG`], naming its
//! roles by the ids of the nodes and bonds they were written as, and a hand-set shape when it
//! has one:
//!
//! ```text
//! <curve id="30" CurvePoints="..." ArrowheadHead="Full" ArrowheadType="Solid" ...>
//!   <objecttag ... Name="ChemBuilder/CurlyArrow"
//!     Value="v=3 source=bond:8 onto=4 target=atom:12 kind=pair dots=0 flip=0 auto=1"/>
//! </curve>
//! ```
//!
//! (`source` is `lonepair:N`, `bond:B` with `onto=N`, or `free`; `target` is `atom:N`, `bond:B`,
//! `between:N:M` or `free`; `auto=0` comes with `path=` the hand-set shape, as
//! [`CurlyArrow::path`] holds it.) The tags of earlier versions read too: version 1 and 2 named
//! the ends `tail=atom:N`, `tail=bond:B:SIDE` (the `onto` atom), `head=…`, and version 2 a
//! `path`; an arrow of theirs without a path is drawn automatically now.
//!
//! **Reading** takes an arrow with the tag back as its roles say, provided the tag still fits
//! the arrow: ChemDraw keeps a persistent tag, but if the arrow or its atoms were moved there,
//! or ChemDraw gave the objects new ids, the atoms and bonds the tag names are no longer at the
//! arrow's ends, and the tag is not trusted. An arrow without a tag that holds (one drawn in
//! ChemDraw) keeps the shape the file draws, as a hand-set one, its ends tied to what
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
const PROP_CURVE_TYPE: u16 = 0x0A08;
const PROP_HEAD_SIZE: u16 = 0x0A20;
const PROP_ANGULAR_SIZE: u16 = 0x0A21;
const PROP_CURVE_POINTS: u16 = 0x0A23;
const PROP_ARROWHEAD_TYPE: u16 = 0x0A2F;
const PROP_ARROWHEAD_CENTER_SIZE: u16 = 0x0A30;
const PROP_ARROWHEAD_WIDTH: u16 = 0x0A31;
const PROP_ARROW_SHAFT_SPACING: u16 = 0x0A33;
const PROP_ARROWHEAD_HEAD: u16 = 0x0A35;
const PROP_ARROWHEAD_TAIL: u16 = 0x0A36;
const PROP_FILL_TYPE: u16 = 0x0A37;
const PROP_CLOSED: u16 = 0x0A39;
const PROP_DIPOLE: u16 = 0x0A3A;
const PROP_NO_GO: u16 = 0x0A3B;
const PROP_ARROW_SOURCE: u16 = 0x0A3E;
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
/// kCDXCurveType: closed, and the arrowheads an older curve has at its start (whole or half).
const CURVE_CLOSED: i16 = 0x0001;
const CURVE_ARROWS_AT_START: i16 = 0x0010 | 0x0040;
/// kCDXCurveType: an arrowhead at the end, whole or half (an older curve's head).
const CURVE_ARROW_AT_END: i16 = 0x0008;
const CURVE_HALF_ARROW_AT_END: i16 = 0x0020;

/// The name of the object tag that holds a curly arrow's meaning.
pub(crate) const CURLY_TAG: &str = "ChemBuilder/CurlyArrow";
/// The version of the tag's value this writes; a reader takes this one and earlier ones. 1: the
/// ends (`tail`, `head`), angle, skew, kind and dots; 2: and a hand-set `path`; 3: the roles
/// (`source`, `onto`, `target`), `flip`, and whether the arrow is drawn automatically.
const TAG_VERSION: u32 = 3;
/// How far an atom or bond the tag names may lie from the arrow's end in the file (molecule
/// units, a bond is 1.5) for the tag to be trusted: about where an arrow's end is drawn beside
/// what it stands on.
const TAG_REACH: f32 = 1.1;
/// The smallest turn written as an arc, in degrees: ChemDraw's arc needs a centre, and a nearly
/// straight arc's centre is far away (at 2°, 29 chords off).
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

/// A curly arrow of shape `pieces` (molecule units, from its tail to its head) with free ends
/// where they start and end: what a ChemDraw arrow is read as before it is tied to atoms.
fn free_arrow(pieces: &[[[f32; 2]; 4]], kind: CurlyKind, source: u32) -> Option<CurlyArrow> {
    let (tail, head) = (pieces[0][0], pieces[pieces.len() - 1][3]);
    let mut arrow = CurlyArrow::new(CurlyEnd::Point(tail), CurlyEnd::Point(head), kind);
    arrow.path = curly::path_of(tail, head, pieces)?;
    arrow.source = Some(source);
    Some(arrow)
}

fn tag_of(obj: &CdxObject) -> Option<String> {
    cdx_appearance::tag_named(obj, CURLY_TAG).and_then(cdx_appearance::tag_text).filter(|t| !t.is_empty())
}

/// A ChemDraw `curve` arrow as a curly arrow of its shape, and what may tie its ends to atoms
/// (only its tag: a curve has no source or target).
pub(crate) fn read_curve(c: &CdxObject) -> Option<(CurlyArrow, Links)> {
    let knots = curve_knots(c)?;
    let pieces: Vec<[[f32; 2]; 4]> = knots.windows(2).map(|w| [w[0][1], w[0][2], w[1][0], w[1][1]]).collect();
    let curve_type = c.prop_i16(PROP_CURVE_TYPE).unwrap_or(0);
    let kind = match c.prop_i16(PROP_ARROWHEAD_HEAD) {
        Some(HEAD_FULL) => CurlyKind::Pair,
        Some(_) => CurlyKind::Single,
        None if curve_type & CURVE_HALF_ARROW_AT_END != 0 => CurlyKind::Single,
        None => CurlyKind::Pair,
    };
    let arrow = free_arrow(&pieces, kind, c.id)?;
    Some((arrow, Links { tag: tag_of(c), source: None, target: None }))
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

/// A ChemDraw curved `arrow` as a curly arrow of its arc, and what may tie its ends to atoms.
pub(crate) fn read_arrow(a: &CdxObject) -> Option<(CurlyArrow, Links)> {
    let tail = to_mol(point_3d(a, PROP_TAIL_3D)?);
    let head = to_mol(point_3d(a, PROP_HEAD_3D)?);
    // Tenths of a degree in CDX (CDXML's degrees, scaled on reading).
    let angle = a.prop_int(PROP_ANGULAR_SIZE)? as f32 / 10.0;
    let kind = match a.prop_i16(PROP_ARROWHEAD_HEAD)? {
        HEAD_FULL => CurlyKind::Pair,
        _ => CurlyKind::Single,
    };
    if curly::len(curly::sub(head, tail)) < 1e-4 {
        return None;
    }
    let arrow = free_arrow(&curly::arc_pieces(tail, head, angle), kind, a.id)?;
    let id = |tag: u16| a.prop_u32(tag).or_else(|| a.prop_int(tag).map(|v| v as u32)).filter(|&v| v != 0);
    Some((arrow, Links { tag: tag_of(a), source: id(PROP_ARROW_SOURCE), target: id(PROP_ARROW_TARGET) }))
}

/// A legacy arc `graphic` arrow as a curly arrow of its arc. Its box holds the head and the arc's
/// centre; the tail is the head turned back about the centre by the (rounded) angle.
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
    free_arrow(&curly::arc_pieces(to_mol(tail), to_mol(head), angle), kind, g.id)
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

/// An end as a version 1 or 2 tag names it: `atom:N`, `bond:B:SIDE`, `between:N:M`.
fn old_end(mol: &Molecule, ids: &ReadIds, text: &str) -> Option<CurlyEnd> {
    let mut parts = text.split(':');
    let kind = parts.next()?;
    let mut num = || parts.next().and_then(|v| v.parse::<u32>().ok());
    Some(match kind {
        "atom" => CurlyEnd::Atom(ids.atom(num()?)?),
        "bond" => {
            let bond = ids.bond(mol, num()?)?;
            let onto = ids.atom(num()?)?;
            bond_end(mol, bond, onto)?
        }
        "between" => CurlyEnd::Between(ids.atom(num()?)?, ids.atom(num()?)?),
        _ => return None,
    })
}

/// Bond `bond` as an end, its electrons onto `onto`, when that is one of its atoms.
fn bond_end(mol: &Molecule, bond: u32, onto: u32) -> Option<CurlyEnd> {
    let b = mol.bond_by_id(bond)?;
    (onto == b.begin || onto == b.end).then_some(CurlyEnd::Bond { bond, onto })
}

/// `arrow` (read with free ends) as its tag says: its roles, kind, dots, flip and hand-set
/// shape. `None` when the tag does not read, is of a later version, or no longer fits the arrow
/// (what it names is not where the arrow's ends are in the file).
fn from_tag(mol: &Molecule, ids: &ReadIds, arrow: &CurlyArrow, tag: &str) -> Option<CurlyArrow> {
    let fields: HashMap<&str, &str> = tag.split_whitespace().filter_map(|f| f.split_once('=')).collect();
    let version: u32 = fields.get("v")?.parse().ok()?;
    if version == 0 || version > TAG_VERSION {
        return None;
    }
    let mut out = arrow.clone();
    let node = |text: &str| text.parse::<u32>().ok().and_then(|n| ids.atom(n));
    if version >= 3 {
        out.from = match *fields.get("source")? {
            "free" => arrow.from,
            text => match text.split_once(':')? {
                ("lonepair", n) => CurlyEnd::Atom(node(n)?),
                ("bond", b) => bond_end(mol, ids.bond(mol, b.parse().ok()?)?, node(fields.get("onto")?)?)?,
                _ => return None,
            },
        };
        out.to = match *fields.get("target")? {
            "free" => arrow.to,
            text => old_end(mol, ids, text).or_else(|| {
                // `bond:B` names no onto.
                let b = text.strip_prefix("bond:")?.parse().ok()?;
                let bond = mol.bond_by_id(ids.bond(mol, b)?)?;
                Some(CurlyEnd::Bond { bond: bond.id, onto: bond.begin })
            })?,
        };
        out.flip = fields.get("flip").is_some_and(|f| *f == "1");
    } else {
        let end = |key: &str, free: CurlyEnd| -> Option<CurlyEnd> {
            match *fields.get(key)? {
                "free" => Some(free),
                text => old_end(mol, ids, text),
            }
        };
        out.from = end("tail", arrow.from)?;
        out.to = end("head", arrow.to)?;
    }
    out.kind = match fields.get("kind").copied() {
        Some("single") => CurlyKind::Single,
        Some("pair") | None => CurlyKind::Pair,
        Some(_) => return None,
    };
    out.dots = fields.get("dots").is_some_and(|d| *d == "1");
    out.path = match fields.get("path") {
        Some(text) if fields.get("auto") != Some(&"1") => text
            .split(';')
            .map(|p| {
                let (s, t) = p.split_once(',')?;
                Some([s.parse::<f32>().ok().filter(|v| v.is_finite())?, t.parse::<f32>().ok().filter(|v| v.is_finite())?])
            })
            .collect::<Option<Vec<_>>>()?,
        // An earlier arc, or an automatic arrow: drawn automatically.
        _ => Vec::new(),
    };
    if !out.path.is_empty() && !out.is_shaped() {
        return None;
    }
    // Still the arrow the file draws: what the tag names stands at its ends.
    let (CurlyEnd::Point(tail), CurlyEnd::Point(head)) = (arrow.from, arrow.to) else { return None };
    let (at, ah) = curly::anchors(mol, &CurlyArrow { path: Vec::new(), ..out.clone() })?;
    let near = |p: [f32; 2], q: [f32; 2]| curly::len(curly::sub(p, q)) <= TAG_REACH;
    (near(at, tail) && near(ah, head)).then_some(out)
}

/// What ChemDraw's `ArrowSource` / `ArrowTarget` id names: a node's atom, or a bond (its
/// electrons onto the atom nearer the drawn point `at`).
fn native_end(mol: &Molecule, ids: &ReadIds, id: u32, at: [f32; 2]) -> Option<CurlyEnd> {
    if let Some(atom) = ids.atom(id) {
        return Some(CurlyEnd::Atom(atom));
    }
    let bond = mol.bond_by_id(ids.bond(mol, id)?)?;
    let dist = |a: u32| mol.atom_by_id(a).map_or(f32::INFINITY, |a| curly::len(curly::sub(a.pos, at)));
    let onto = if dist(bond.end) < dist(bond.begin) { bond.end } else { bond.begin };
    Some(CurlyEnd::Bond { bond: bond.id, onto })
}

/// Tie `arrow`, read with free ends and the file's shape, to the atoms and bonds of `mol` (the
/// structures read with it): as its tag says, when the tag holds; else as ChemDraw's own source
/// and target say, or where its ends clearly stand, keeping the file's shape.
pub(crate) fn link(arrow: &mut CurlyArrow, links: &Links, mol: &Molecule, ids: &ReadIds) {
    if let Some(tagged) = links.tag.as_deref().and_then(|t| from_tag(mol, ids, arrow, t)) {
        *arrow = tagged;
        return;
    }
    let (CurlyEnd::Point(tail), CurlyEnd::Point(head)) = (arrow.from, arrow.to) else { return };
    let tie = |id: Option<u32>, at: [f32; 2], source: bool| {
        id.and_then(|id| native_end(mol, ids, id, at))
            .or_else(|| curly::end_near(mol, at, source))
            .unwrap_or(CurlyEnd::Point(at))
    };
    let drawn = curly::pieces(mol, arrow);
    let read = arrow.clone();
    arrow.from = tie(links.source, tail, true);
    arrow.to = tie(links.target, head, false);
    arrow.dots = false;
    // A source and a target on one atom is no arrow to redraw: keep it as ChemDraw drew it.
    if arrow.from == arrow.to || matches!((arrow.from, arrow.to), (CurlyEnd::Atom(a), CurlyEnd::Atom(b)) if a == b) {
        *arrow = read;
        return;
    }
    // The file's shape, its control points where the file has them, now between the anchors
    // of the ends it is tied to.
    if let (Some(pieces), Some((at, ah))) = (drawn, curly::anchors(mol, arrow))
        && let Some(path) = curly::path_of(at, ah, &pieces)
    {
        arrow.path = path;
    }
}

// ─── Writing ─────────────────────────────────────────────────────────────────

/// The id ChemDraw's `ArrowSource` / `ArrowTarget` takes for an end: a node, or a bond.
fn native_id(end: &CurlyEnd, ids: &WrittenIds) -> Option<u32> {
    match *end {
        CurlyEnd::Atom(a) => ids.node_of.get(&a).copied(),
        CurlyEnd::Bond { bond, .. } => ids.bond_of.get(&bond).copied(),
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => None,
    }
}

/// The tag's value for `arrow`: its roles by the ids of the nodes and bonds they were written as
/// (an end on something not written as one is written free).
fn tag_value(arrow: &CurlyArrow, ids: &WrittenIds) -> String {
    let node = |a: u32| ids.node_of.get(&a).copied();
    let (source, onto) = match arrow.from {
        CurlyEnd::Atom(a) => (node(a).map(|n| format!("lonepair:{n}")), None),
        CurlyEnd::Bond { bond, onto } => match (ids.bond_of.get(&bond), node(onto)) {
            (Some(b), Some(n)) => (Some(format!("bond:{b}")), Some(n)),
            _ => (None, None),
        },
        CurlyEnd::Between(..) | CurlyEnd::Point(_) => (None, None),
    };
    let target = match arrow.to {
        CurlyEnd::Atom(a) => node(a).map(|n| format!("atom:{n}")),
        CurlyEnd::Bond { bond, .. } => ids.bond_of.get(&bond).map(|b| format!("bond:{b}")),
        CurlyEnd::Between(a, b) => node(a).zip(node(b)).map(|(a, b)| format!("between:{a}:{b}")),
        CurlyEnd::Point(_) => None,
    };
    let mut value = format!("v={TAG_VERSION} source={}", source.unwrap_or_else(|| "free".into()));
    if let Some(n) = onto {
        value.push_str(&format!(" onto={n}"));
    }
    value.push_str(&format!(
        " target={} kind={} dots={} flip={} auto={}",
        target.unwrap_or_else(|| "free".into()),
        match arrow.kind {
            CurlyKind::Pair => "pair",
            CurlyKind::Single => "single",
        },
        u8::from(arrow.dots),
        u8::from(arrow.flip),
        u8::from(!arrow.is_shaped()),
    ));
    if arrow.is_shaped() {
        let points: Vec<String> = arrow.path.iter().map(|p| format!("{},{}", p[0], p[1])).collect();
        value.push_str(&format!(" path={}", points.join(";")));
    }
    value
}

/// The arc ChemDraw draws for an arrow drawn as one: its tail, head and centre (CDX units), its
/// radius and the angle written.
struct Arc {
    tail: (f64, f64),
    head: (f64, f64),
    center: (f64, f64),
    radius: f64,
    angle: f32,
}

fn arc_of(path: &curly::CurlyPath, shift: (f64, f64)) -> Option<Arc> {
    let angle = path.arc?;
    let angle = if angle.abs() < MIN_WRITTEN_ANGLE { MIN_WRITTEN_ANGLE.copysign(angle) } else { angle };
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
    Some(Arc { tail, head, center, radius, angle })
}

/// `ArrowheadHead` for `arrow` drawn along `path`: a fish-hook's barb on the outer side of its
/// last turn, as the editor draws it ([`curly::barb_left`]).
fn head_value(arrow: &CurlyArrow, path: &curly::CurlyPath) -> i16 {
    match arrow.kind {
        CurlyKind::Pair => HEAD_FULL,
        CurlyKind::Single if curly::barb_left(path) => HEAD_HALF_LEFT,
        CurlyKind::Single => HEAD_HALF_RIGHT,
    }
}

/// ChemDraw's rounding of an arc graphic's angle: 90, 120, 180 or 270°, keeping the sign.
fn graphic_angle(angle: f32) -> i16 {
    let a = angle.abs();
    let nearest = [90.0f32, 120.0, 180.0, 270.0].into_iter().min_by(|x, y| (x - a).abs().total_cmp(&(y - a).abs())).unwrap();
    (nearest.copysign(angle) * 10.0) as i16
}

/// The box `path` covers on the page, moved by `shift`, padded by half a head's width.
fn bbox(path: &curly::CurlyPath, shift: (f64, f64)) -> Vec<u8> {
    let mut b: Option<[f64; 4]> = None;
    for p in &path.points {
        let (x, y) = to_cdx(*p, shift);
        b = Some(cdx::union(b, [x, y, x, y]));
    }
    let line = 0.6 * PT;
    let pad = line / 2.0 + CURLY_HEAD_HALF_WIDTH as f64 * line;
    let [l, t, r, bt] = b.expect("a curve has points");
    cdx::rect([l - pad, t - pad, r + pad, bt + pad])
}

/// The arrowheads and the tag, on an `arrow` or a `curve`.
fn set_head_and_tag(o: &mut CdxObject, arrow: &CurlyArrow, path: &curly::CurlyPath, ids: &WrittenIds, next: &mut u32) {
    o.set(PROP_FILL_TYPE, i16_bytes(FILL_NONE));
    o.set(PROP_ARROWHEAD_HEAD, i16_bytes(head_value(arrow, path)));
    o.props.retain(|(t, _)| *t != PROP_ARROWHEAD_TAIL && *t != PROP_ARROW_SHAFT_SPACING);
    o.set(PROP_ARROWHEAD_TYPE, i16_bytes(ARROWHEAD_SOLID));
    o.set(PROP_HEAD_SIZE, i16_bytes(hundredths(CURLY_HEAD_LENGTH)));
    o.set(PROP_ARROWHEAD_CENTER_SIZE, i16_bytes(hundredths(CURLY_HEAD_CENTER)));
    o.set(PROP_ARROWHEAD_WIDTH, i16_bytes(hundredths(CURLY_HEAD_HALF_WIDTH)));
    // The tag: the one read is replaced.
    o.children.retain(|c| !cdx_appearance::is_tag_named(c, CURLY_TAG));
    *next += 1;
    o.children.push(cdx_appearance::object_tag(*next, CURLY_TAG, Some(&tag_value(arrow, ids))));
}

/// Write an arrow drawn as an arc onto `a` (a new `arrow` object or the one it was read from)
/// and `g`, the graphic it supersedes.
#[allow(clippy::too_many_arguments)]
fn set_arc(a: &mut CdxObject, g: Option<&mut CdxObject>, arrow: &CurlyArrow, path: &curly::CurlyPath, arc: &Arc, shift: (f64, f64), ids: &WrittenIds, next: &mut u32) {
    a.set(PROP_BOUNDING_BOX, bbox(path, shift));
    set_head_and_tag(a, arrow, path, ids, next);
    a.set(PROP_ANGULAR_SIZE, i16_bytes((arc.angle * 10.0).round() as i16));
    match native_id(&arrow.from, ids) {
        Some(id) => a.set(PROP_ARROW_SOURCE, id.to_le_bytes().to_vec()),
        None => a.props.retain(|(t, _)| *t != PROP_ARROW_SOURCE),
    }
    match native_id(&arrow.to, ids) {
        Some(id) => a.set(PROP_ARROW_TARGET, id.to_le_bytes().to_vec()),
        None => a.props.retain(|(t, _)| *t != PROP_ARROW_TARGET),
    }
    a.set(PROP_HEAD_3D, point_3d_value(arc.head));
    a.set(PROP_TAIL_3D, point_3d_value(arc.tail));
    a.set(PROP_CENTER_3D, point_3d_value(arc.center));
    a.set(PROP_MAJOR_AXIS_END_3D, point_3d_value((arc.center.0 + arc.radius, arc.center.1)));
    a.set(PROP_MINOR_AXIS_END_3D, point_3d_value((arc.center.0, arc.center.1 + arc.radius)));
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

/// Write an arrow drawn as any other curve onto `c` (a new `curve` or the one it was read from):
/// its cubic pieces as drawn, moved by `shift`.
fn set_curve(c: &mut CdxObject, arrow: &CurlyArrow, path: &curly::CurlyPath, shift: (f64, f64), ids: &WrittenIds, next: &mut u32) {
    let pieces = &path.pieces;
    // Each knot as (control in, knot, control out); the ends' outer controls are the ends.
    let mut knots: Vec<[[f32; 2]; 3]> = vec![[pieces[0][0], pieces[0][0], pieces[0][1]]];
    for (i, b) in pieces.iter().enumerate() {
        let out = pieces.get(i + 1).map_or(b[3], |n| n[1]);
        knots.push([b[2], b[3], out]);
    }
    let mut points = ((knots.len() * 3) as u16).to_le_bytes().to_vec();
    for k in &knots {
        for p in k {
            let (x, y) = to_cdx(*p, shift);
            points.extend(cdx::point(x, y));
        }
    }
    c.set(PROP_BOUNDING_BOX, bbox(path, shift));
    c.set(PROP_CURVE_POINTS, points);
    // An open curve: what made the one read closed or headed at its start is dropped.
    if let Some(t) = c.prop_i16(PROP_CURVE_TYPE) {
        c.set(PROP_CURVE_TYPE, i16_bytes(t & !(CURVE_CLOSED | CURVE_ARROWS_AT_START | CURVE_ARROW_AT_END | CURVE_HALF_ARROW_AT_END)));
    }
    c.props.retain(|(t, _)| *t != PROP_CLOSED);
    set_head_and_tag(c, arrow, path, ids, next);
}

/// The ChemDraw objects for `arrow` on `mol`, moved by `shift`: the objects it was read from
/// (`original`, rewritten), or new ones with new ids from `next` — an arc graphic and the
/// `arrow` superseding it for an arrow drawn as an arc, a `curve` for any other. An arrow whose
/// shape changed kind since it was read is written as its new kind (a ChemDraw arc kept as the
/// shape it was, which this editor holds as a hand-set curve, comes back a `curve` under the
/// arc's id). Nothing when the arrow cannot be placed.
pub(crate) fn write(
    mol: &Molecule,
    arrow: &CurlyArrow,
    original: Option<Vec<CdxObject>>,
    ids: &WrittenIds,
    shift: (f64, f64),
    next: &mut u32,
) -> Vec<CdxObject> {
    let Some(path) = curly::geometry(mol, arrow) else { return Vec::new() };
    let original = original.unwrap_or_default();
    let read_as_curve = original.iter().any(|o| o.tag == OBJ_CURVE);
    let Some(arc) = arc_of(&path, shift) else {
        // An arrow read as an arc and now drawn otherwise keeps the id it had.
        let arrow_id = original.iter().find(|o| o.tag == OBJ_ARROW || o.tag == OBJ_GRAPHIC).map(|o| o.id);
        let mut c = match original.into_iter().find(|o| o.tag == OBJ_CURVE) {
            Some(c) => c,
            None => {
                let id = arrow_id.unwrap_or_else(|| {
                    *next += 1;
                    *next
                });
                let mut c = CdxObject::new(OBJ_CURVE, id);
                c.set(PROP_BOUNDING_BOX, Vec::new()); // first, as ChemDraw orders it; set below
                c
            }
        };
        set_curve(&mut c, arrow, &path, shift, ids, next);
        return vec![c];
    };
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
    set_arc(&mut a, objects.iter_mut().find(|o| o.tag == OBJ_GRAPHIC), arrow, &path, &arc, shift, ids, next);
    objects.insert(arrow_at, a);
    objects
}

#[cfg(test)]
mod tests;
