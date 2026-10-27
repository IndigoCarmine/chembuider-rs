//! Brackets ([`Bracket`]) in a ChemDraw document.
//!
//! ChemDraw writes a pair of polymer brackets as:
//!
//! - two bracket `graphic`s (`GraphicType="Bracket"`, `BracketType="Square"`, `LipSize`) inside
//!   the fragment of the enclosed atoms. A graphic's "bounding box" holds the bracket's two ends;
//!   its lips turn left of the line from the first to the second (on the y-down page). The
//!   closing graphic also carries the `BracketUsage` and an `objecttag` named `bracketusage`
//!   whose value is the label (`n`, `2`), with the text showing it below and right of the
//!   bracket's lower end;
//! - a page-level `bracketedgroup` listing the enclosed nodes (`BracketedObjectIDs`), with its
//!   `BracketUsage`, `PolymerRepeatPattern`, `SRULabel` (repeat units) or `RepeatCount` (multiple
//!   groups), and one `bracketattachment` per graphic (`GraphicID`), each holding the
//!   `crossingbond` it stands across (`BondID`, `InnerAtomID`).
//!
//! Reading takes the bracketed group (which [`cdx_page::convert_page`](super::cdx_page) marks as
//! read), with the shape and label of its graphics, into a [`Bracket`] around the atoms its
//! nodes became. Writing puts the whole set back as ChemDraw does — the graphics where the
//! brackets stand now, the crossing bonds as the drawing has them now. A bracket read from a
//! document keeps its group's id and the properties the editor does not know (`PolymerFlipType`,
//! …); everything that refers to atoms, bonds or positions is rewritten, since the structures
//! themselves are written anew.

use std::collections::HashMap;

use super::bracket::{self, Bracket, BracketShape, BracketUsage, RepeatPattern};
use super::cdx::{self, OBJ_GROUP, OBJ_TEXT, PROP_2D_POSITION, PROP_BOUNDING_BOX, PROP_TEXT, PT, SCALE};
use super::cdx_codec::CdxObject;
use super::cdx_page::Owned;
use super::{Molecule, PageObject};

pub(crate) const OBJ_BRACKETED_GROUP: u16 = 0x8017;
const OBJ_BRACKET_ATTACHMENT: u16 = 0x8018;
const OBJ_CROSSING_BOND: u16 = 0x8019;
const OBJ_GRAPHIC: u16 = 0x8007;
const OBJ_FRAGMENT: u16 = 0x8003;
const OBJ_OBJECT_TAG: u16 = 0x8011;

const PROP_NAME: u16 = 0x0008;
const PROP_GRAPHIC_TYPE: u16 = 0x0A00;
const PROP_BRACKET_TYPE: u16 = 0x0A06;
const PROP_LIP_SIZE: u16 = 0x0A22;
const PROP_BRACKET_USAGE: u16 = 0x0A24;
const PROP_REPEAT_PATTERN: u16 = 0x0A25;
const PROP_BRACKETED_OBJECTS: u16 = 0x0A27;
const PROP_REPEAT_COUNT: u16 = 0x0A28;
const PROP_SRU_LABEL: u16 = 0x0A2A;
const PROP_GRAPHIC_ID: u16 = 0x0A2B;
const PROP_BOND_ID: u16 = 0x0A2C;
const PROP_INNER_ATOM_ID: u16 = 0x0A2D;
const PROP_TAG_TYPE: u16 = 0x0D00;
const PROP_TAG_VALUE: u16 = 0x0D05;
const PROP_CAPTION_LINE_HEIGHT: u16 = 0x0707;

const GRAPHIC_BRACKET: i16 = 6;
const TAG_TYPE_STRING: i16 = 3;
/// What ChemDraw writes as a bracket's `LipSize` (it does not read it back).
const LIP_SIZE: i16 = 60;
/// The objecttag ChemDraw labels the closing bracket with.
const LABEL_TAG: &str = "bracketusage";
/// The label's size: 7.5 pt, in twentieths of a point.
const LABEL_SIZE: u16 = 150;
const LABEL_FONT: u16 = cdx::LABEL_FONT;
/// The label's ascent and descent at 7.5 pt, in points.
const LABEL_ASCENT_PT: f64 = 5.4;
const LABEL_DESCENT_PT: f64 = 1.6;

fn id_array(obj: &CdxObject, tag: u16) -> Vec<u32> {
    obj.prop(tag).map(|v| v.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)).collect()).unwrap_or_default()
}

fn cdx_text(v: &[u8]) -> Option<String> {
    cdx::parse_cdx_string(v).map(|(_, text)| text.replace('\r', "\n"))
}

/// Every graphic inside `obj` (fragments and groups included), by id.
pub(crate) fn graphics(obj: &CdxObject) -> HashMap<u32, &CdxObject> {
    fn walk<'a>(o: &'a CdxObject, out: &mut HashMap<u32, &'a CdxObject>) {
        for c in &o.children {
            if c.tag == OBJ_GRAPHIC {
                out.insert(c.id, c);
            }
            if matches!(c.tag, OBJ_FRAGMENT | OBJ_GROUP | OBJ_BRACKETED_GROUP) {
                walk(c, out);
            }
        }
    }
    let mut out = HashMap::new();
    walk(obj, &mut out);
    out
}

/// The bracketed groups inside `group` (ChemDraw nests a set of brackets inside one that
/// encloses it), in document order.
pub(crate) fn nested_groups(group: &CdxObject) -> Vec<&CdxObject> {
    let mut out = Vec::new();
    for c in group.children.iter().filter(|c| c.tag == OBJ_BRACKETED_GROUP) {
        out.push(c);
        out.extend(nested_groups(c));
    }
    out
}

/// The label an objecttag of a bracket graphic shows (`n`, `2`).
fn tag_label(graphic: &CdxObject) -> Option<String> {
    let tag = graphic.children.iter().find(|c| {
        c.tag == OBJ_OBJECT_TAG && c.prop(PROP_NAME).is_some_and(|n| n.eq_ignore_ascii_case(LABEL_TAG.as_bytes()))
    })?;
    tag.prop(PROP_TAG_VALUE)
        .and_then(cdx_text)
        .or_else(|| tag.children.iter().find(|t| t.tag == OBJ_TEXT).and_then(|t| t.prop(PROP_TEXT)).and_then(cdx_text))
}

/// A whole number written without decimals, any other with as many as it needs.
fn count_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e9 { format!("{}", v as i64) } else { format!("{v}") }
}

/// A bracketed group read as a [`Bracket`] whose `atoms` are still the ChemDraw ids it lists
/// ([`link`] turns them into atoms), with the ids of the graphics it is drawn with. `None` for
/// a group that encloses nothing or whose usage ChemDraw does not define.
pub(crate) fn read_group(group: &CdxObject, graphics: &HashMap<u32, &CdxObject>) -> Option<(Bracket, Vec<u32>)> {
    let objects = id_array(group, PROP_BRACKETED_OBJECTS);
    if objects.is_empty() {
        return None;
    }
    let drawn: Vec<&CdxObject> = group
        .children
        .iter()
        .filter(|c| c.tag == OBJ_BRACKET_ATTACHMENT)
        .filter_map(|a| a.prop_u32(PROP_GRAPHIC_ID))
        .filter_map(|g| graphics.get(&g).copied())
        .collect();
    // ChemDraw 7 reads the usage from the graphics; it writes it to both.
    let usage = match group.prop_int(PROP_BRACKET_USAGE).or_else(|| drawn.iter().find_map(|g| g.prop_int(PROP_BRACKET_USAGE))) {
        Some(v) => BracketUsage::from_cdx(v)?,
        None => BracketUsage::Unspecified,
    };
    let shape = BracketShape::from_cdx(drawn.first().and_then(|g| g.prop_int(PROP_BRACKET_TYPE)));
    let repeat_pattern = RepeatPattern::from_cdx(
        group.prop_int(PROP_REPEAT_PATTERN).or_else(|| drawn.iter().find_map(|g| g.prop_int(PROP_REPEAT_PATTERN))).unwrap_or(0),
    );
    let shown = drawn.iter().find_map(|g| tag_label(g));
    let label = match usage {
        BracketUsage::Sru => group.prop(PROP_SRU_LABEL).and_then(cdx_text).or(shown),
        BracketUsage::MultipleGroup => group
            .prop(PROP_REPEAT_COUNT)
            .and_then(|v| <[u8; 8]>::try_from(v).ok())
            .map(f64::from_le_bytes)
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(count_text)
            .or(shown),
        _ => shown,
    }
    .unwrap_or_else(|| usage.default_label().to_string());
    let bracket =
        Bracket { id: 0, atoms: objects, usage, label: label.trim().to_string(), repeat_pattern, shape, source: Some(group.id) };
    Some((bracket, drawn.iter().map(|g| g.id).collect()))
}

/// Replace the ChemDraw ids a read bracket lists with the atoms they became (`atoms_of`: a
/// node's atom, every atom of a fragment). Ids of anything else (bonds) are dropped.
pub(crate) fn link(bracket: &mut Bracket, atoms_of: &HashMap<u32, Vec<u32>>) {
    let mut atoms: Vec<u32> = Vec::new();
    for id in &bracket.atoms {
        for &a in atoms_of.get(id).into_iter().flatten() {
            if !atoms.contains(&a) {
                atoms.push(a);
            }
        }
    }
    bracket.atoms = atoms;
}

// ─── Writing ─────────────────────────────────────────────────────────────────

fn cdx_point(p: [f32; 2], shift: (f64, f64)) -> (f64, f64) {
    (p[0] as f64 * SCALE + shift.0, p[1] as f64 * SCALE + shift.1)
}

/// A CDXString of `text` in one run of the label style (Arial 7.5 pt, plain).
fn label_string(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&1u16.to_le_bytes());
    for v in [0u16, LABEL_FONT, 0, LABEL_SIZE, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend(text.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }));
    out
}

/// The objecttag ChemDraw puts on the closing bracket: the label, and the text showing it with
/// its baseline starting at `at` (CDX units).
fn label_tag(id: u32, label: &str, at: (f64, f64)) -> CdxObject {
    let mut tag = CdxObject::new(OBJ_OBJECT_TAG, id);
    tag.set(PROP_TAG_TYPE, TAG_TYPE_STRING.to_le_bytes().to_vec());
    tag.set(PROP_NAME, LABEL_TAG.as_bytes().to_vec());
    tag.set(PROP_TAG_VALUE, cdx::cdx_string(&[], label));
    let width: f64 = label.chars().map(cdx::arial_width).sum::<f64>() * LABEL_SIZE as f64 / 200.0;
    let mut t = CdxObject::new(OBJ_TEXT, 0);
    t.set(PROP_2D_POSITION, cdx::point(at.0, at.1));
    t.set(PROP_BOUNDING_BOX, cdx::rect([at.0, at.1 - LABEL_ASCENT_PT * PT, at.0 + width, at.1 + LABEL_DESCENT_PT * PT]));
    // "variable": ChemDraw's line height that follows the text's size.
    t.set(PROP_CAPTION_LINE_HEIGHT, 0u16.to_le_bytes().to_vec());
    t.set(PROP_TEXT, label_string(label));
    tag.children.push(t);
    tag
}

/// Ids for a bracket's group, attachments, crossing bonds and graphics: the document's own
/// group id when the bracket came from it, new ones otherwise.
fn new_id(next: &mut u32) -> u32 {
    *next += 1;
    *next
}

/// The ChemDraw objects of every bracket of `mol`: each bracket's graphics go into
/// `fragments` (into the fragment its first enclosed atom was written in, or onto the page
/// when there is none), and the bracketed groups are returned, for the page. `ids` maps atoms
/// and bonds to the nodes and bonds they were written as; `owned` holds what reading took from
/// the document being saved into (see [`cdx_page::take_owned`](super::cdx_page::take_owned)).
pub(crate) fn write_brackets(
    mol: &Molecule,
    ids: &cdx::WrittenIds,
    owned: &mut Owned,
    shift: (f64, f64),
    next: &mut u32,
    fragments: &mut Vec<CdxObject>,
) -> Vec<CdxObject> {
    let mut groups = Vec::new();
    for object in &mol.objects {
        let PageObject::Bracket(b) = object else { continue };
        let original = b.source.and_then(|s| owned.remove(&s)).and_then(|(_, objects)| {
            objects.into_iter().find(|o| o.tag == OBJ_BRACKETED_GROUP)
        });
        let nodes: Vec<u32> = b.atoms.iter().filter_map(|a| ids.node_of.get(a).copied()).collect();
        let Some(geometry) = bracket::geometry(mol, b).filter(|_| !nodes.is_empty()) else { continue };
        // The group: the document's own (its other properties kept), or a new one.
        let mut group = match original {
            Some(mut g) => {
                g.children.clear();
                g.props.retain(|(t, _)| {
                    !matches!(*t, PROP_BRACKETED_OBJECTS | PROP_BRACKET_USAGE | PROP_REPEAT_PATTERN | PROP_REPEAT_COUNT | PROP_SRU_LABEL)
                });
                g
            }
            None => CdxObject::new(OBJ_BRACKETED_GROUP, new_id(next)),
        };
        group.set(PROP_BRACKETED_OBJECTS, nodes.iter().flat_map(|n| n.to_le_bytes()).collect());
        group.set(PROP_BRACKET_USAGE, vec![b.usage.cdx_value() as u8]);
        if b.usage == BracketUsage::Sru && b.repeat_pattern != RepeatPattern::HeadToTail {
            group.set(PROP_REPEAT_PATTERN, vec![b.repeat_pattern.cdx_value() as u8]);
        }
        match b.usage {
            BracketUsage::MultipleGroup => {
                if let Some(n) = b.label.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0) {
                    group.set(PROP_REPEAT_COUNT, n.to_le_bytes().to_vec());
                }
            }
            BracketUsage::Sru => group.set(PROP_SRU_LABEL, cdx::cdx_string(&[], &b.label)),
            _ => {}
        }
        let home = b.atoms.iter().find_map(|a| ids.fragment_of.get(a).copied());
        let last = geometry.sides.len() - 1;
        let mut drawn = Vec::new();
        for (k, side) in geometry.sides.iter().enumerate() {
            let (start, end) = (cdx_point(side.start, shift), cdx_point(side.end, shift));
            let mut g = CdxObject::new(OBJ_GRAPHIC, new_id(next));
            g.set(PROP_BOUNDING_BOX, cdx::rect([start.0, start.1, end.0, end.1]));
            g.set(PROP_GRAPHIC_TYPE, GRAPHIC_BRACKET.to_le_bytes().to_vec());
            g.set(PROP_BRACKET_TYPE, b.shape.cdx_value().to_le_bytes().to_vec());
            g.set(PROP_LIP_SIZE, LIP_SIZE.to_le_bytes().to_vec());
            if k == last {
                g.set(PROP_BRACKET_USAGE, vec![b.usage.cdx_value() as u8]);
                if b.usage == BracketUsage::Sru && b.repeat_pattern != RepeatPattern::HeadToTail {
                    g.set(PROP_REPEAT_PATTERN, vec![b.repeat_pattern.cdx_value() as u8]);
                }
                if !b.label.trim().is_empty() {
                    let tag_id = new_id(next);
                    g.children.push(label_tag(tag_id, b.label.trim(), cdx_point(geometry.label_at, shift)));
                }
            }
            let mut attachment = CdxObject::new(OBJ_BRACKET_ATTACHMENT, new_id(next));
            attachment.set(PROP_GRAPHIC_ID, g.id.to_le_bytes().to_vec());
            if let Some(c) = side.crossing
                && let (Some(bond), Some(inner)) = (ids.bond_of.get(&c.bond), ids.node_of.get(&c.inner))
            {
                let mut crossing = CdxObject::new(OBJ_CROSSING_BOND, new_id(next));
                crossing.set(PROP_BOND_ID, bond.to_le_bytes().to_vec());
                crossing.set(PROP_INNER_ATOM_ID, inner.to_le_bytes().to_vec());
                attachment.children.push(crossing);
            }
            group.children.push(attachment);
            drawn.push(g);
        }
        match home.and_then(|f| fragments.iter_mut().find(|o| o.tag == OBJ_FRAGMENT && o.id == f)) {
            Some(fragment) => fragment.children.extend(drawn),
            None => groups.extend(drawn),
        }
        groups.push(group);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx::{merge_into_cdx, molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml};
    use crate::molecule::{cdx_codec, BondOrder};

    /// `sgroups_and_remove_atoms_4.cdxml` of RDKit's test data (ChemDraw 21): an N with a
    /// multiple group ×2 around three atoms, written by ChemDraw, trimmed to the structure.
    const CHEMDRAW_MULTIPLE_GROUP: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<!DOCTYPE CDXML SYSTEM "http://www.cambridgesoft.com/xml/cdxml.dtd" >
<CDXML CreationProgram="ChemDraw 21.0.0.28" BoundingBox="215.57 314.93 324.43 405.07" LabelFont="21" LabelSize="10" BondLength="30"><fonttable><font id="21" charset="x-mac-roman" name="Helvetica"/></fonttable><page id="20" BoundingBox="0 0 540 720"><fragment id="1" BoundingBox="215.57 314.93 324.43 405.07" Z="1"><n id="2" p="215.82 351.43" Z="2" AS="N"/><n id="3" p="241.80 336.43" Z="3" AS="N"/><n id="4" p="267.78 351.43" Z="4" Element="7" NumHydrogens="0" NeedsClean="yes" AS="N"><t p="264.17 355.02" BoundingBox="264.93 347.85 270.62 355.02" LabelJustification="Left"><s font="21" size="10" color="0" face="96">N</s></t></n><n id="5" p="293.76 336.43" Z="5" AS="N"/><n id="6" p="324.18 354" Z="6" AS="N"/><n id="7" p="267.78 381.43" Z="7" AS="N"/><b id="8" Z="8" B="2" E="3" BS="N"/><b id="9" Z="9" B="4" E="5" BS="N"/><b id="10" Z="10" B="4" E="7" BS="N"/><b id="11" Z="11" B="3" E="4" BS="N"/><b id="12" Z="12" B="6" E="5" BS="N"/><graphic id="14" BoundingBox="252.78 402.43 252.78 315.43" Z="13" GraphicType="Bracket" BracketType="Square" LipSize="60"/><graphic id="18" BoundingBox="308.59 315.43 308.59 402.43" Z="14" GraphicType="Bracket" BracketType="Square" LipSize="60" BracketUsage="MultipleGroup" PolymerFlipType="NoFlip"><objecttag TagType="String" Name="bracketusage" Value="2"><t p="312.10 405.07" BoundingBox="312.34 399.80 315.96 405.07" CaptionLineHeight="variable"><s font="21" size="7.5" color="0">2</s></t></objecttag></graphic></fragment><bracketedgroup id="21" BracketedObjectIDs="4 5 7" BracketUsage="MultipleGroup" PolymerFlipType="NoFlip" RepeatCount="2"><bracketattachment id="22" GraphicID="14"><crossingbond id="23" BondID="11" InnerAtomID="4"/></bracketattachment><bracketattachment id="24" GraphicID="18"><crossingbond id="25" BondID="12" InnerAtomID="5"/></bracketattachment></bracketedgroup></page></CDXML>"#;

    /// The same file's sibling `sgroups_and_remove_atoms_6.cdxml`, cut down to one repeat unit:
    /// `SRULabel="n"`.
    const CHEMDRAW_SRU: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<CDXML CreationProgram="ChemDraw 21.0.0.28" BondLength="30"><page id="20"><fragment id="1"><n id="9" p="296.49 349.53"/><n id="10" p="322.47 334.53"/><n id="11" p="348.45 349.53"/><n id="12" p="374.43 334.53"/><b id="20" B="9" E="10"/><b id="21" B="10" E="11"/><b id="25" B="11" E="12"/><graphic id="28" BoundingBox="308.96 364.53 308.96 334.53" GraphicType="Bracket" BracketType="Square" LipSize="60"/><graphic id="32" BoundingBox="360.97 334.53 360.97 364.53" GraphicType="Bracket" BracketType="Square" LipSize="60" BracketUsage="SRU" PolymerFlipType="NoFlip"><objecttag TagType="String" Name="bracketusage" Value="n"><t p="364.24 366.53" BoundingBox="364.72 362.52 367.92 366.53" CaptionLineHeight="variable"><s font="3" size="7.5" color="0">n</s></t></objecttag></graphic></fragment><bracketedgroup id="57" BracketedObjectIDs="10 11" BracketUsage="SRU" PolymerFlipType="NoFlip" SRULabel="n"><bracketattachment id="58" GraphicID="32"><crossingbond id="59" BondID="25" InnerAtomID="11"/></bracketattachment><bracketattachment id="60" GraphicID="28"><crossingbond id="61" BondID="20" InnerAtomID="10"/></bracketattachment></bracketedgroup></page></CDXML>"#;

    fn brackets(m: &Molecule) -> Vec<&Bracket> {
        m.brackets().collect()
    }

    fn element_of(m: &Molecule, id: u32) -> String {
        m.atom_by_id(id).unwrap().element.clone()
    }

    #[test]
    fn chemdraws_multiple_group_reads_as_a_bracket() {
        let import = read_cdxml(CHEMDRAW_MULTIPLE_GROUP).unwrap();
        let m = &import.molecule;
        let b = brackets(m);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].usage, BracketUsage::MultipleGroup);
        assert_eq!(b[0].label, "2");
        assert_eq!(b[0].repeat_count(), Some(2));
        assert_eq!(b[0].shape, BracketShape::Square);
        assert_eq!(b[0].source, Some(21));
        assert_eq!(b[0].atoms.len(), 3);
        assert!(b[0].atoms.iter().any(|&a| element_of(m, a) == "N"));
        assert_eq!(bracket::crossing_bonds(m, b[0]).len(), 2);
        assert_eq!(import.other_objects, 0, "the bracketed group is read, not hidden");
    }

    #[test]
    fn chemdraws_repeat_unit_reads_as_a_bracket() {
        let import = read_cdxml(CHEMDRAW_SRU).unwrap();
        let b = brackets(&import.molecule);
        assert_eq!(b.len(), 1);
        assert_eq!((b[0].usage, b[0].label.as_str()), (BracketUsage::Sru, "n"));
        assert_eq!(b[0].atoms.len(), 2);
        assert_eq!(import.other_objects, 0);
        // The brackets ChemDraw drew and the ones placed here stand in the same places
        // (give or take a point).
        let g = bracket::geometry(&import.molecule, b[0]).unwrap();
        let pt = |v: f32| v as f64 * SCALE / PT;
        let xs: Vec<f64> = g.sides.iter().map(|s| pt(s.start[0])).collect();
        assert!((xs[0] - 309.47).abs() < 1.5 && (xs[1] - 361.46).abs() < 1.5, "{xs:?}");
    }

    fn polyethylene() -> Molecule {
        let mut m = Molecule::default();
        let ids: Vec<u32> =
            (0..4).map(|k| m.add_atom("C".into(), [5.0 + k as f32 * 1.3, if k % 2 == 0 { 5.0 } else { 4.25 }], 0)).collect();
        for w in ids.windows(2) {
            m.add_bond(w[0], w[1], BondOrder::Single);
        }
        m.add_bracket(&[ids[1], ids[2]].into(), BracketUsage::Sru);
        m
    }

    fn assert_same_brackets(a: &Molecule, b: &Molecule) {
        let (x, y) = (brackets(a), brackets(b));
        assert_eq!(x.len(), y.len());
        for (p, q) in x.iter().zip(&y) {
            assert_eq!((p.usage, &p.label, p.shape, p.repeat_pattern), (q.usage, &q.label, q.shape, q.repeat_pattern));
            let atoms = |m: &Molecule, br: &Bracket| {
                let mut v: Vec<[i32; 2]> = br
                    .atoms
                    .iter()
                    .map(|&id| m.atom_by_id(id).unwrap().pos)
                    .map(|p| [(p[0] * 100.0).round() as i32, (p[1] * 100.0).round() as i32])
                    .collect();
                v.sort();
                v
            };
            assert_eq!(atoms(a, p), atoms(b, q));
        }
    }

    #[test]
    fn drawn_brackets_survive_cdx_and_cdxml() {
        let mut m = polyethylene();
        let o = m.add_atom("O".into(), [12.0, 5.0], 0);
        let c = m.add_atom("C".into(), [13.5, 5.0], 0);
        m.add_bond(o, c, BondOrder::Single);
        let id = m.add_bracket(&[o, c].into(), BracketUsage::MultipleGroup).unwrap();
        if let Some(PageObject::Bracket(b)) = m.object_by_id_mut(id) {
            b.label = "3".into();
            b.shape = BracketShape::Round;
        }
        let from_cdx = read_cdx(&molecule_to_cdx_document(&m)).unwrap();
        assert_same_brackets(&m, &from_cdx.molecule);
        assert_eq!(from_cdx.other_objects, 0);
        let xml = molecule_to_cdxml(&m);
        let from_xml = read_cdxml(&xml).unwrap();
        assert_same_brackets(&m, &from_xml.molecule);
        assert_eq!(from_xml.other_objects, 0);
        // As ChemDraw writes it, and nothing cdx-file-rs does not know.
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        for needle in [
            "GraphicType=\"Bracket\"",
            "BracketType=\"Square\"",
            "BracketType=\"Round\"",
            "BracketUsage=\"SRU\"",
            "SRULabel=\"n\"",
            "BracketUsage=\"MultipleGroup\"",
            "RepeatCount=\"3\"",
            "Name=\"bracketusage\"",
            "<crossingbond",
        ] {
            assert!(xml.contains(needle), "{needle}: {xml}");
        }
        assert!(cdx_file_rs::file::CdxFile::from_bytes(&molecule_to_cdx_document(&m)).is_ok());
    }

    #[test]
    fn chemdraw_brackets_round_trip_through_a_merge() {
        let doc = cdx::cdxml_document(CHEMDRAW_MULTIPLE_GROUP).unwrap();
        let original = cdx_codec::write(&doc);
        let first = read_cdx(&original).unwrap();
        let saved = merge_into_cdx(&original, &first.molecule);
        let again = read_cdx(&saved).unwrap();
        assert_same_brackets(&first.molecule, &again.molecule);
        assert_eq!(again.other_objects, 0);
        assert_eq!(brackets(&again.molecule)[0].source, Some(21), "the group keeps its id");
        // ChemDraw's other properties stay; one group, two graphics, no stale ones.
        let doc = cdx_codec::parse(&saved).unwrap();
        let xml = cdx::cdxml_of_document(&doc);
        assert!(xml.contains("PolymerFlipType=\"NoFlip\""), "{xml}");
        assert_eq!(xml.matches("<bracketedgroup").count(), 1, "{xml}");
        assert_eq!(xml.matches("GraphicType=\"Bracket\"").count(), 2, "{xml}");
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        // Saved twice, still the same.
        let twice = read_cdx(&merge_into_cdx(&saved, &again.molecule)).unwrap();
        assert_same_brackets(&first.molecule, &twice.molecule);
    }

    #[test]
    fn deleted_and_edited_brackets_are_saved_as_they_are_now() {
        let doc = cdx::cdxml_document(CHEMDRAW_MULTIPLE_GROUP).unwrap();
        let original = cdx_codec::write(&doc);
        let mut m = read_cdx(&original).unwrap().molecule;
        if let Some(PageObject::Bracket(b)) = m.objects.iter_mut().find(|o| o.as_bracket().is_some()) {
            b.label = "5".into();
        }
        let again = read_cdx(&merge_into_cdx(&original, &m)).unwrap();
        assert_eq!(brackets(&again.molecule)[0].repeat_count(), Some(5));
        let id = m.brackets().next().unwrap().id;
        m.remove_object(id);
        let saved = merge_into_cdx(&original, &m);
        let again = read_cdx(&saved).unwrap();
        assert!(brackets(&again.molecule).is_empty());
        let xml = cdx::cdxml_of_document(&cdx_codec::parse(&saved).unwrap());
        assert!(!xml.contains("bracketedgroup") && !xml.contains("GraphicType=\"Bracket\""), "{xml}");
    }

    /// Every RDKit CDXML test file that has brackets (when fetched beside the repository, see
    /// cdx-file-rs's `scripts/fetch_rdkit_testdata`), read, saved and read again.
    #[test]
    fn brackets_in_chemdraw_files_round_trip() {
        let Ok(dir) = std::env::var("CHEMBUILDER_CDXML_CORPUS") else { return };
        let mut seen = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let Ok(xml) = std::fs::read_to_string(&path) else { continue };
            if !xml.contains("bracketedgroup") {
                continue;
            }
            let Some(first) = read_cdxml(&xml) else { continue };
            let Some(doc) = cdx::cdxml_document(&xml) else { continue };
            let original = cdx_codec::write(&doc);
            let saved = merge_into_cdx(&original, &first.molecule);
            let again = read_cdx(&saved).unwrap();
            assert_same_brackets(&first.molecule, &again.molecule);
            assert_eq!(again.other_objects, first.other_objects, "{path:?}");
            seen += first.molecule.brackets().count();
        }
        assert!(seen > 0, "no brackets read from the corpus");
        eprintln!("{seen} brackets round-tripped");
    }
}
