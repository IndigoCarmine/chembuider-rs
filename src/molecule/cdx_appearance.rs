//! Ring fills ([`RingFill`](super::RingFill)) and drop shadows ([`Shadow`]) in a ChemDraw document, so that both
//! survive saving as CDX or CDXML and opening the file again.
//!
//! **A ring fill** is the shape ChemDraw itself draws for a filled area: a closed, filled
//! freeform curve (kCDXObj_Curve, CDXML `<curve>`, CurveType Closed | Filled and FillType
//! Solid). Its vertices are the ring's atoms in ring order, and each vertex is also its own two
//! Bézier control points, so the edges are straight. Its colour is an index into the colour
//! table, and its alpha is the 16-bit ForegroundAlpha (CDXML `alpha`, left out when opaque).
//! It sits on the page just before its structure's fragment, so ChemDraw draws it behind the
//! bonds. It carries an object tag named [`RING_FILL_TAG`]. That tag makes it a fill: on
//! reading, it is put back onto the atoms its vertices are on. A curve drawn in ChemDraw has no
//! such tag and stays a shape the editor keeps. A tagged curve whose vertices no longer lie on
//! a closed ring of atoms (for example, the structure was moved in ChemDraw without it) is
//! left out. The curve is not grouped with its structure, so moving the structure alone in
//! ChemDraw leaves the fill where it was.
//!
//! **A drop shadow** has no ChemDraw form. A structure's shadow is an object tag on its
//! fragment, named [`SHADOW_TAG`]. Its value is the shadow's colour, offset and blur as text:
//! `r g b a dx dy blur`, with the lengths in molecule units (a bond is 1.5). Object tags are
//! ChemDraw's way to attach data of one's own to an object. ChemDraw keeps the tag, since it is
//! marked persistent, and shows nothing of it, since it holds no text. On reading, every atom
//! of the fragment casts the shadow, which is what [`set_shadow`] gives a structure.
//!
//! A drawing without fills or shadows writes neither, so its file is byte for byte what it was
//! before these existed.
//!
//! [`set_shadow`]: crate::widget::ChemStructEditor::set_shadow

use super::cdx::{self, WrittenIds, PROP_BOUNDING_BOX, SCALE};
use super::cdx_codec::CdxObject;
use super::cdx_colors::{self, ColorTable};
use super::{Molecule, Shadow};
use std::collections::HashMap;

/// kCDXObj_Curve.
pub(crate) const OBJ_CURVE: u16 = 0x8008;
/// kCDXObj_ObjectTag.
const OBJ_OBJECT_TAG: u16 = 0x8011;
/// kCDXProp_Name: an object tag's name, its text without style runs.
const PROP_NAME: u16 = 0x0008;
const PROP_TAG_TYPE: u16 = 0x0D00;
/// kCDXProp_ObjectTag_Persistent: ChemDraw saves the tag again.
const PROP_TAG_PERSISTENT: u16 = 0x0D04;
const PROP_TAG_VALUE: u16 = 0x0D05;
/// kCDXObjectTagType_String: the value is a CDXString.
const TAG_TYPE_STRING: i16 = 3;
const PROP_CURVE_TYPE: u16 = 0x0A08;
const PROP_CURVE_POINTS: u16 = 0x0A23;
const PROP_FILL_TYPE: u16 = 0x0A37;
/// kCDXCurveType Closed | Filled.
const CURVE_CLOSED_FILLED: i16 = 0x0001 | 0x0080;
/// kCDXFillType_Solid.
const FILL_SOLID: i16 = 2;

/// The name of the object tag that makes a curve a ring fill.
pub(crate) const RING_FILL_TAG: &str = "ChemBuilder/RingFill";
/// The name of the object tag that holds a fragment's drop shadow.
pub(crate) const SHADOW_TAG: &str = "ChemBuilder/Shadow";

/// How far a fill's vertex may be from its atom, in molecule units (a bond is 1.5). This covers
/// a coordinate rounded in CDXML, but not a vertex of a neighbouring atom.
const VERTEX_TOLERANCE: f32 = 0.02;

/// An object tag named `name`, persistent, with the text `value` when there is one.
fn object_tag(id: u32, name: &str, value: Option<&str>) -> CdxObject {
    let mut tag = CdxObject::new(OBJ_OBJECT_TAG, id);
    tag.set(PROP_NAME, name.as_bytes().to_vec());
    tag.set(PROP_TAG_PERSISTENT, vec![1]);
    if let Some(value) = value {
        tag.set(PROP_TAG_TYPE, TAG_TYPE_STRING.to_le_bytes().to_vec());
        tag.set(PROP_TAG_VALUE, cdx::cdx_string(&[], value));
    }
    tag
}

/// `obj`'s first object tag named `name`. The name is taken as plain text, as ChemDraw writes
/// it, or as a CDXString without style runs.
fn tag_named<'a>(obj: &'a CdxObject, name: &str) -> Option<&'a CdxObject> {
    obj.children.iter().find(|c| {
        c.tag == OBJ_OBJECT_TAG
            && c.prop(PROP_NAME).is_some_and(|v| v == name.as_bytes() || v.strip_prefix(&[0, 0][..]) == Some(name.as_bytes()))
    })
}

/// Whether `obj` is the curve of a ring fill written here.
pub(crate) fn is_ring_fill(obj: &CdxObject) -> bool {
    obj.tag == OBJ_CURVE && tag_named(obj, RING_FILL_TAG).is_some()
}

// ─── Writing ─────────────────────────────────────────────────────────────────

/// The object tag of a fragment that casts `shadow`.
pub(crate) fn shadow_tag(id: u32, shadow: &Shadow) -> CdxObject {
    let [r, g, b, a] = shadow.color;
    let [dx, dy] = shadow.offset;
    object_tag(id, SHADOW_TAG, Some(&format!("{r} {g} {b} {a} {dx} {dy} {}", shadow.blur)))
}

/// The curve of each of `mol`'s ring fills, put into `objects` just before the fragment its
/// ring was written into (`ids`), in the order of the fills. A fill is written when it has
/// three atoms or more and all of them were written as nodes, the same fills the editor draws.
/// Its colour joins the colour table. `shift` is the one the fragments were written with.
pub(crate) fn insert_ring_fills(
    mol: &Molecule,
    shift: (f64, f64),
    next: &mut u32,
    ids: &WrittenIds,
    colors: &mut ColorTable,
    objects: &mut Vec<CdxObject>,
) {
    if mol.ring_fills.is_empty() {
        return;
    }
    let pos: HashMap<u32, [f32; 2]> = mol.atoms.iter().map(|a| (a.id, a.pos)).collect();
    for rf in &mol.ring_fills {
        if rf.atoms.len() < 3 || !rf.atoms.iter().all(|a| ids.node_of.contains_key(a)) {
            continue;
        }
        let Some(&fragment) = ids.fragment_of.get(&rf.atoms[0]) else { continue };
        let Some(at) = objects.iter().position(|o| o.id == fragment) else { continue };
        let vertices: Vec<(f64, f64)> = rf
            .atoms
            .iter()
            .map(|a| {
                let p = pos[a];
                (p[0] as f64 * SCALE + shift.0, p[1] as f64 * SCALE + shift.1)
            })
            .collect();
        *next += 1;
        let mut curve = CdxObject::new(OBJ_CURVE, *next);
        let bbox = vertices.iter().fold(None, |b, &(x, y)| Some(cdx::union(b, [x, y, x, y])));
        curve.set(PROP_BOUNDING_BOX, cdx::rect(bbox.expect("three vertices or more")));
        curve.set(cdx_colors::PROP_FOREGROUND_COLOR, colors.index_of(Some(rf.color)).to_le_bytes().to_vec());
        cdx_colors::set_alpha(&mut curve, rf.color[3]);
        curve.set(PROP_CURVE_TYPE, CURVE_CLOSED_FILLED.to_le_bytes().to_vec());
        curve.set(PROP_FILL_TYPE, FILL_SOLID.to_le_bytes().to_vec());
        // Each vertex as (control in, vertex, control out), all three the same point.
        let mut points = ((vertices.len() * 3) as u16).to_le_bytes().to_vec();
        for &(x, y) in &vertices {
            for _ in 0..3 {
                points.extend(cdx::point(x, y));
            }
        }
        curve.set(PROP_CURVE_POINTS, points);
        *next += 1;
        curve.children.push(object_tag(*next, RING_FILL_TAG, None));
        objects.insert(at, curve);
    }
}

// ─── Reading ─────────────────────────────────────────────────────────────────

/// The drop shadow `frag` casts, if it has a [`SHADOW_TAG`] that reads as one.
pub(crate) fn read_shadow(frag: &CdxObject) -> Option<Shadow> {
    let (_, text) = cdx::parse_cdx_string(tag_named(frag, SHADOW_TAG)?.prop(PROP_TAG_VALUE)?)?;
    let v: Vec<&str> = text.split_whitespace().collect();
    let [r, g, b, a, dx, dy, blur] = v[..] else { return None };
    let channel = |s: &str| s.parse::<u8>().ok();
    let length = |s: &str| s.parse::<f32>().ok().filter(|v| v.is_finite());
    Some(Shadow {
        color: [channel(r)?, channel(g)?, channel(b)?, channel(a)?],
        offset: [length(dx)?, length(dy)?],
        blur: length(blur)?.max(0.0),
    })
}

/// A ring fill as read from its curve: its vertices (molecule units) and colour, not yet put
/// onto atoms ([`link_ring_fills`]).
#[derive(Debug, Clone)]
pub(crate) struct ReadFill {
    vertices: Vec<[f32; 2]>,
    color: [u8; 4],
}

/// The ring fills inside `obj` (a page, and the groups in it), added to `out` with their
/// curve's id. `colors` is the colour table as read in `obj`.
pub(crate) fn read_ring_fills(obj: &CdxObject, colors: &ColorTable, out: &mut Vec<(u32, ReadFill)>) {
    let colors = &colors.inside(obj);
    for c in &obj.children {
        if c.tag == cdx::OBJ_GROUP {
            read_ring_fills(c, colors, out);
        } else if is_ring_fill(c) {
            // Black, the editor's default colour, reads as none; a fill is always in one.
            let [r, g, b, _] = cdx_colors::foreground(c, colors).unwrap_or([0, 0, 0, 255]);
            let vertices = curve_vertices(c).unwrap_or_default();
            out.push((c.id, ReadFill { vertices, color: [r, g, b, cdx_colors::alpha(c)] }));
        }
    }
}

/// The vertices of a curve whose points are (control in, vertex, control out) triples, in
/// molecule units as the reader places atoms.
fn curve_vertices(curve: &CdxObject) -> Option<Vec<[f32; 2]>> {
    let v = curve.prop(PROP_CURVE_POINTS)?;
    let n = u16::from_le_bytes([*v.first()?, *v.get(1)?]) as usize;
    if !n.is_multiple_of(3) {
        return None;
    }
    let int = |at: usize| v.get(at..at + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f64 / SCALE);
    (0..n / 3)
        .map(|k| {
            let at = 2 + (k * 3 + 1) * 8;
            Some([int(at + 4)? as f32, int(at)? as f32])
        })
        .collect()
}

/// Atoms (id, position) by the grid cell they are in.
type AtomGrid = HashMap<(i64, i64), Vec<(u32, [f32; 2])>>;

/// Put each fill onto the atoms of `mol` its vertices are on, in their order, when those make
/// a closed ring of three atoms or more. Any other fill is left out.
pub(crate) fn link_ring_fills(mol: &mut Molecule, fills: Vec<ReadFill>) {
    if fills.is_empty() {
        return;
    }
    // The atoms by the cell of a grid of the tolerance's size they are in: a vertex's atom is in
    // its cell or a neighbouring one.
    let cell = |p: [f32; 2]| ((p[0] / VERTEX_TOLERANCE).floor() as i64, (p[1] / VERTEX_TOLERANCE).floor() as i64);
    let mut grid: AtomGrid = HashMap::new();
    for a in mol.atoms.iter().filter(|a| !a.group_h) {
        grid.entry(cell(a.pos)).or_default().push((a.id, a.pos));
    }
    let atom_at = |p: [f32; 2]| {
        let (cx, cy) = cell(p);
        let near = (cx - 1..=cx + 1).flat_map(|x| (cy - 1..=cy + 1).map(move |y| (x, y)));
        near.filter_map(|k| grid.get(&k))
            .flatten()
            .map(|&(id, q)| (id, (q[0] - p[0]).hypot(q[1] - p[1])))
            .filter(|&(_, d)| d <= VERTEX_TOLERANCE)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    };
    for fill in fills {
        let Some(atoms) = fill.vertices.iter().map(|&p| atom_at(p)).collect::<Option<Vec<u32>>>() else { continue };
        let mut distinct = atoms.clone();
        distinct.sort_unstable();
        distinct.dedup();
        if atoms.len() >= 3 && distinct.len() == atoms.len() && mol.ring_is_closed(&atoms) {
            mol.set_ring_fill(atoms, fill.color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx::{molecule_to_cdx_document, read_cdx};
    use crate::molecule::{cdx_codec, BondOrder, RingFill};

    /// A triangle of carbons, filled.
    fn triangle() -> Molecule {
        let mut m = Molecule::default();
        let ids: Vec<u32> =
            [[1.5, 0.0], [-0.75, 1.25], [-0.75, -1.25]].into_iter().map(|p| m.add_atom("C".into(), p, 0)).collect();
        for k in 0..3 {
            m.add_bond(ids[k], ids[(k + 1) % 3], BondOrder::Single);
        }
        m.set_ring_fill(ids, [1, 2, 3, 4]);
        m
    }

    #[test]
    fn a_shadow_tag_reads_back_exactly() {
        let shadow = Shadow { color: [1, 2, 3, 4], offset: [0.1, -1e-3], blur: 0.25 };
        let mut frag = CdxObject::new(cdx::OBJ_FRAGMENT, 1);
        frag.children.push(shadow_tag(2, &shadow));
        assert_eq!(read_shadow(&frag), Some(shadow));
    }

    #[test]
    fn a_shadow_tag_that_does_not_read_is_no_shadow() {
        for value in ["", "1 2 3", "1 2 3 300 0 0 0", "1 2 3 4 NaN 0 0", "1 2 3 4 0 0 0 9"] {
            let mut frag = CdxObject::new(cdx::OBJ_FRAGMENT, 1);
            frag.children.push(object_tag(2, SHADOW_TAG, Some(value)));
            assert_eq!(read_shadow(&frag), None, "{value:?}");
        }
        assert_eq!(read_shadow(&CdxObject::new(cdx::OBJ_FRAGMENT, 1)), None);
    }

    /// A fill whose curve no longer lies on its ring (moved away in ChemDraw) is left out, and
    /// so is one on atoms that make no ring.
    #[test]
    fn a_fill_off_its_ring_is_left_out() {
        let m = triangle();
        let bytes = molecule_to_cdx_document(&m);
        let mut doc = cdx_codec::parse(&bytes).unwrap();
        let curve = doc.children[0].children.iter_mut().find(|c| c.tag == OBJ_CURVE).unwrap();
        let points = curve.prop(PROP_CURVE_POINTS).unwrap().to_vec();
        let mut moved = points.clone();
        // The y of every point, 1 pt further down.
        for k in 0..(points.len() - 2) / 8 {
            let at = 2 + k * 8;
            let y = i32::from_le_bytes(points[at..at + 4].try_into().unwrap()) + 65536;
            moved[at..at + 4].copy_from_slice(&y.to_le_bytes());
        }
        curve.set(PROP_CURVE_POINTS, moved);
        let read = read_cdx(&cdx_codec::write(&doc)).unwrap();
        assert!(read.molecule.ring_fills.is_empty());
        assert_eq!(read.other_objects, 0, "not a shape either");

        // The triangle's fill over the chain C–C–C it becomes without a bond: no ring.
        let doc = cdx_codec::parse(&bytes).unwrap();
        let mut fills = Vec::new();
        read_ring_fills(&doc.children[0], &ColorTable::of(&doc), &mut fills);
        assert_eq!(fills.len(), 1);
        let mut chain = read_cdx(&bytes).unwrap().molecule;
        chain.ring_fills.clear();
        let b = chain.bonds[2].id;
        chain.bonds.retain(|x| x.id != b);
        link_ring_fills(&mut chain, fills.into_iter().map(|(_, f)| f).collect());
        assert!(chain.ring_fills.is_empty());
    }

    #[test]
    fn a_fill_is_read_onto_its_atoms_in_ring_order() {
        let m = triangle();
        let read = read_cdx(&molecule_to_cdx_document(&m)).unwrap().molecule;
        let ids: Vec<u32> = read.atoms.iter().map(|a| a.id).collect();
        assert_eq!(read.ring_fills, [RingFill { atoms: ids, color: [1, 2, 3, 4] }]);
    }
}
