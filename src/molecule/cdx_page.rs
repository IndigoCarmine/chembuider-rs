//! Page objects ([`PageObject`]: reaction arrows and text) in a ChemDraw document.
//!
//! Reading turns the arrows and texts on a document's pages into page objects, each remembering
//! the id of the ChemDraw object it came from (`source`). An arrow is ChemDraw's `arrow` object
//! (ChemDraw 9 and later), usually with the older line `graphic` it supersedes beside it, or a
//! lone line graphic in an older file; both stand for one arrow. Only what the editor can show
//! faithfully is taken — a straight solid reaction arrow or equilibrium arrow, and any text —
//! so curved, hollow, crossed-out, dipole and resonance arrows stay in the document untouched.
//!
//! Writing puts new objects on the page the way ChemDraw 25 writes them (an `arrow` and the line
//! `graphic` it supersedes; a caption `t` in 10 pt Arial). Saving into the document they were
//! read from ([`take_owned`] + [`write_objects`]) first removes every object reading took, then
//! writes the current ones: an object that came from the file goes back as its original
//! ChemDraw object — under the same id, so reaction steps and chemical properties that point at
//! it still do — with only what the editor changed (its position, its text) rewritten. One the
//! user deleted is simply not written again.

use std::collections::{HashMap, HashSet};

use super::cdx::{self, OBJ_GROUP, OBJ_TEXT, PROP_2D_POSITION, PROP_BOUNDING_BOX, PROP_TEXT, PROP_UTF8_TEXT, PT, SCALE};
use super::cdx_codec::CdxObject;
use super::page::{ArrowKind, PageArrow, PageObject, PageText, TextAlign};
use crate::reaction::cdxml::{text_of, utf8_string};
use crate::widget::rich_text::{RichText, Script};
use crate::widget::scene::{
    arrow_primitives, ARROW_HEAD_CENTER, ARROW_HEAD_HALF_WIDTH, ARROW_HEAD_LENGTH, ARROW_SHAFT_SPACING, TEXT_LINE_HEIGHT,
};

const OBJ_GRAPHIC: u16 = 0x8007;
const OBJ_ARROW: u16 = 0x8021;

const PROP_SUPERSEDED_BY: u16 = 0x0013;
const PROP_HEAD_3D: u16 = 0x0207;
const PROP_TAIL_3D: u16 = 0x0208;
const PROP_CENTER_3D: u16 = 0x020D;
const PROP_MAJOR_AXIS_END_3D: u16 = 0x020E;
const PROP_MINOR_AXIS_END_3D: u16 = 0x020F;
const PROP_JUSTIFICATION: u16 = 0x0701;
const PROP_CAPTION_JUSTIFICATION: u16 = 0x080C;
const PROP_GRAPHIC_TYPE: u16 = 0x0A00;
const PROP_LINE_TYPE: u16 = 0x0A01;
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

const GRAPHIC_LINE: i16 = 1;
const ARROW_TYPE_FULL_HEAD: i16 = 2;
const ARROW_TYPE_EQUILIBRIUM: i16 = 8;
const HEAD_NONE: i16 = 1;
const HEAD_FULL: i16 = 2;
const HEAD_HALF_LEFT: i16 = 3;
const ARROWHEAD_SOLID: i16 = 1;
const FILL_NONE: i16 = 1;

/// ChemDraw's line width (0.6 pt, its document style), in CDX units.
const LINE_WIDTH: f64 = 0.6 * PT;
/// Text size ChemDraw writes captions at (10 pt), in points.
const TEXT_PT: f64 = 10.0;
/// From the top of a 10 pt line to its baseline, and from the baseline to its bottom, in points
/// (as `cdx::label_geometry` measures labels).
const ASCENT_PT: f64 = 7.2;
const DESCENT_PT: f64 = 2.1;

fn hundredths(v: f32) -> u16 {
    (v * 100.0).round() as u16
}

// ─── Reading ─────────────────────────────────────────────────────────────────

/// A page object read from a document, and the ids of the ChemDraw objects it stands for
/// (an arrow's `arrow` and the `graphic` it supersedes; a text's `t`).
pub(crate) struct Converted {
    pub object: PageObject,
    pub ids: Vec<u32>,
}

/// Every page-level object of `page` (looking inside groups), in document order.
fn page_level<'a>(obj: &'a CdxObject, out: &mut Vec<&'a CdxObject>) {
    for child in &obj.children {
        if child.tag == OBJ_GROUP {
            page_level(child, out);
        } else {
            out.push(child);
        }
    }
}

fn point_2d(obj: &CdxObject, tag: u16) -> Option<(f64, f64)> {
    let v = obj.prop(tag).filter(|v| v.len() >= 8)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some((n(4), n(0)))
}

fn point_3d(obj: &CdxObject, tag: u16) -> Option<(f64, f64)> {
    let v = obj.prop(tag).filter(|v| v.len() >= 8)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some((n(0), n(4)))
}

fn mol_point(p: (f64, f64)) -> [f32; 2] {
    [(p.0 / SCALE) as f32, (p.1 / SCALE) as f32]
}

/// The kind of a ChemDraw `arrow` object, when it is one the editor draws: straight, solid,
/// neither crossed out nor a dipole, and either a reaction arrow (a full head, no tail) or an
/// equilibrium arrow (half heads at both ends of two shafts).
fn arrow_object_kind(a: &CdxObject) -> Option<ArrowKind> {
    if a.prop_int(PROP_ANGULAR_SIZE).is_some_and(|v| v != 0)
        || a.prop_int(PROP_NO_GO).is_some_and(|v| v > 1)
        || a.prop_int(PROP_DIPOLE).is_some_and(|v| v != 0)
        || a.prop_i16(PROP_ARROWHEAD_TYPE).is_some_and(|t| t != 0 && t != ARROWHEAD_SOLID)
    {
        return None;
    }
    let head = a.prop_i16(PROP_ARROWHEAD_HEAD).unwrap_or(0);
    let tail = a.prop_i16(PROP_ARROWHEAD_TAIL).unwrap_or(0);
    let spacing = a.prop_int(PROP_ARROW_SHAFT_SPACING).unwrap_or(0);
    match (head, tail) {
        (HEAD_FULL, 0 | HEAD_NONE) if spacing == 0 => Some(ArrowKind::Reaction),
        (HEAD_HALF_LEFT, HEAD_HALF_LEFT) if spacing > 0 => Some(ArrowKind::Equilibrium),
        _ => None,
    }
}

/// The kind of a legacy line `graphic` arrow, when it is one the editor draws.
fn graphic_kind(g: &CdxObject) -> Option<ArrowKind> {
    if g.prop_i16(PROP_GRAPHIC_TYPE) != Some(GRAPHIC_LINE) || g.prop_i16(PROP_LINE_TYPE).is_some_and(|t| t != 0) {
        return None;
    }
    match g.prop_i16(PROP_ARROW_TYPE)? {
        ARROW_TYPE_FULL_HEAD => Some(ArrowKind::Reaction),
        ARROW_TYPE_EQUILIBRIUM => Some(ArrowKind::Equilibrium),
        _ => None,
    }
}

/// A line graphic's ends: its "bounding box" holds the head, then the tail.
fn graphic_ends(g: &CdxObject) -> Option<((f64, f64), (f64, f64))> {
    let b = cdx::bounding_box(g)?;
    Some(((b[0], b[1]), (b[2], b[3])))
}

fn superseded_by(g: &CdxObject) -> Option<u32> {
    g.prop_u32(PROP_SUPERSEDED_BY)
}

fn justification(t: &CdxObject) -> TextAlign {
    match t.prop_int(PROP_JUSTIFICATION).or_else(|| t.prop_int(PROP_CAPTION_JUSTIFICATION)) {
        Some(1) => TextAlign::Center,
        Some(-1) => TextAlign::Right,
        _ => TextAlign::Left,
    }
}

/// A ChemDraw text as page text: its markup (lines joined by `\n`), alignment and anchor.
fn read_text(t: &CdxObject) -> Option<PageText> {
    let rich = text_of(t)?;
    let markup = markup_lines(&rich);
    if markup.trim().is_empty() {
        return None;
    }
    let pos = point_2d(t, PROP_2D_POSITION)
        .or_else(|| cdx::bounding_box(t).map(|b| (b[0], b[1] + ASCENT_PT * PT)))?;
    Some(PageText { id: 0, pos: mol_point(pos), text: markup, align: justification(t), source: Some(t.id) })
}

/// Markup for rich text that may hold line breaks: each line written on its own, so a script
/// never spans a break.
fn markup_lines(rich: &RichText) -> String {
    let mut lines: Vec<RichText> = vec![RichText::new()];
    for span in &rich.spans {
        for (k, part) in span.text.split('\n').enumerate() {
            if k > 0 {
                lines.push(RichText::new());
            }
            let line = lines.last_mut().expect("one line at least");
            *line = std::mem::take(line).span(part, span.style);
        }
    }
    lines.iter().map(cdx::compact_markup).collect::<Vec<_>>().join("\n")
}

/// The arrows and texts on `page` that become page objects, in document order (an arrow where
/// its `arrow` object, or its lone graphic, stands).
pub(crate) fn convert_page(page: &CdxObject) -> Vec<Converted> {
    let mut objects = Vec::new();
    page_level(page, &mut objects);
    // Arrow objects the editor draws, and the graphics superseded by them.
    let arrows: HashMap<u32, ArrowKind> =
        objects.iter().filter(|o| o.tag == OBJ_ARROW).filter_map(|a| Some((a.id, arrow_object_kind(a)?))).collect();
    let mut graphic_of: HashMap<u32, u32> = HashMap::new();
    for g in objects.iter().filter(|o| o.tag == OBJ_GRAPHIC) {
        if let Some(a) = superseded_by(g) {
            graphic_of.insert(a, g.id);
        }
    }
    let all_ids: HashSet<u32> = objects.iter().map(|o| o.id).collect();
    let mut out = Vec::new();
    for obj in objects {
        match obj.tag {
            OBJ_ARROW => {
                let Some(&kind) = arrows.get(&obj.id) else { continue };
                let (Some(head), Some(tail)) = (point_3d(obj, PROP_HEAD_3D), point_3d(obj, PROP_TAIL_3D)) else { continue };
                let mut ids = vec![obj.id];
                ids.extend(graphic_of.get(&obj.id));
                let arrow = PageArrow { id: 0, tail: mol_point(tail), head: mol_point(head), kind, source: Some(obj.id) };
                out.push(Converted { object: PageObject::Arrow(arrow), ids });
            }
            OBJ_GRAPHIC => {
                // A graphic superseded by an arrow object is that arrow (taken with it, or left
                // with it); one whose arrow object is missing stands on its own.
                if superseded_by(obj).is_some_and(|a| all_ids.contains(&a)) {
                    continue;
                }
                let Some(kind) = graphic_kind(obj) else { continue };
                let Some((head, tail)) = graphic_ends(obj) else { continue };
                let arrow = PageArrow { id: 0, tail: mol_point(tail), head: mol_point(head), kind, source: Some(obj.id) };
                out.push(Converted { object: PageObject::Arrow(arrow), ids: vec![obj.id] });
            }
            OBJ_TEXT => {
                if let Some(text) = read_text(obj) {
                    out.push(Converted { object: PageObject::Text(text), ids: vec![obj.id] });
                }
            }
            _ => {}
        }
    }
    out
}

// ─── Writing ─────────────────────────────────────────────────────────────────

fn point_3d_value(p: (f64, f64)) -> Vec<u8> {
    [p.0, p.1, 0.0].iter().flat_map(|v| (v.round() as i32).to_le_bytes()).collect()
}

fn cdx_point(p: [f32; 2], shift: (f64, f64)) -> (f64, f64) {
    (p[0] as f64 * SCALE + shift.0, p[1] as f64 * SCALE + shift.1)
}

/// The box an arrow from `tail` to `head` (CDX units) covers when ChemDraw draws it.
fn arrow_bbox(tail: (f64, f64), head: (f64, f64), kind: ArrowKind) -> [f64; 4] {
    // In points, so f32 keeps its precision.
    let pt = |p: (f64, f64)| egui::pos2((p.0 / PT) as f32, (p.1 / PT) as f32);
    let (strokes, fills) = arrow_primitives(pt(tail), pt(head), kind, (LINE_WIDTH / PT) as f32);
    let mut b: Option<[f64; 4]> = None;
    let points = strokes.iter().flat_map(|s| [s.a, s.b]).chain(fills.iter().flatten().copied());
    for p in points {
        let (x, y) = (p.x as f64 * PT, p.y as f64 * PT);
        b = Some(cdx::union(b, [x, y, x, y]));
    }
    let half = LINE_WIDTH / 2.0;
    let [l, t, r, bt] = b.unwrap_or([tail.0, tail.1, tail.0, tail.1]);
    [l - half, t - half, r + half, bt + half]
}

/// Set an arrow object's and its graphic's geometry to run from `tail` to `head` (CDX units).
fn set_arrow_geometry(obj: &mut CdxObject, tail: (f64, f64), head: (f64, f64), kind: ArrowKind) {
    if obj.tag == OBJ_GRAPHIC {
        obj.set(PROP_BOUNDING_BOX, cdx::rect([head.0, head.1, tail.0, tail.1]));
        return;
    }
    let center = ((tail.0 + head.0) / 2.0, (tail.1 + head.1) / 2.0);
    let d = (head.0 - tail.0, head.1 - tail.1);
    obj.set(PROP_BOUNDING_BOX, cdx::rect(arrow_bbox(tail, head, kind)));
    obj.set(PROP_HEAD_3D, point_3d_value(head));
    obj.set(PROP_TAIL_3D, point_3d_value(tail));
    // ChemDraw also keeps the frame an arrow would bend in: its centre, and the two axes from
    // there (the shaft's direction, and that turned a quarter).
    obj.set(PROP_CENTER_3D, point_3d_value(center));
    obj.set(PROP_MAJOR_AXIS_END_3D, point_3d_value((center.0 + d.0, center.1 + d.1)));
    obj.set(PROP_MINOR_AXIS_END_3D, point_3d_value((center.0 - d.1, center.1 + d.0)));
}

/// A new arrow as ChemDraw 25 writes one: the `arrow` object, and the line `graphic` it
/// supersedes (read by older programs and named by reaction steps).
fn new_arrow(arrow: &PageArrow, shift: (f64, f64), next: &mut u32) -> Vec<CdxObject> {
    let tail = cdx_point(arrow.tail, shift);
    let head = cdx_point(arrow.head, shift);
    *next += 1;
    let graphic_id = *next;
    *next += 1;
    let arrow_id = *next;

    let mut g = CdxObject::new(OBJ_GRAPHIC, graphic_id);
    g.set(PROP_SUPERSEDED_BY, arrow_id.to_le_bytes().to_vec());
    g.set(PROP_BOUNDING_BOX, cdx::rect([head.0, head.1, tail.0, tail.1]));
    g.set(PROP_GRAPHIC_TYPE, GRAPHIC_LINE.to_le_bytes().to_vec());
    let arrow_type = match arrow.kind {
        ArrowKind::Reaction => ARROW_TYPE_FULL_HEAD,
        ArrowKind::Equilibrium => ARROW_TYPE_EQUILIBRIUM,
    };
    g.set(PROP_ARROW_TYPE, arrow_type.to_le_bytes().to_vec());
    g.set(PROP_HEAD_SIZE, (hundredths(ARROW_HEAD_LENGTH) as i16).to_le_bytes().to_vec());

    let mut a = CdxObject::new(OBJ_ARROW, arrow_id);
    a.set(PROP_BOUNDING_BOX, Vec::new()); // placed first, as ChemDraw orders it; set below
    a.set(PROP_FILL_TYPE, FILL_NONE.to_le_bytes().to_vec());
    let (head_end, tail_end) = match arrow.kind {
        ArrowKind::Reaction => (HEAD_FULL, None),
        ArrowKind::Equilibrium => (HEAD_HALF_LEFT, Some(HEAD_HALF_LEFT)),
    };
    a.set(PROP_ARROWHEAD_HEAD, head_end.to_le_bytes().to_vec());
    if let Some(t) = tail_end {
        a.set(PROP_ARROWHEAD_TAIL, t.to_le_bytes().to_vec());
    }
    a.set(PROP_ARROWHEAD_TYPE, ARROWHEAD_SOLID.to_le_bytes().to_vec());
    a.set(PROP_HEAD_SIZE, (hundredths(ARROW_HEAD_LENGTH) as i16).to_le_bytes().to_vec());
    a.set(PROP_ARROWHEAD_CENTER_SIZE, hundredths(ARROW_HEAD_CENTER).to_le_bytes().to_vec());
    a.set(PROP_ARROWHEAD_WIDTH, hundredths(ARROW_HEAD_HALF_WIDTH).to_le_bytes().to_vec());
    if arrow.kind == ArrowKind::Equilibrium {
        a.set(PROP_ARROW_SHAFT_SPACING, hundredths(ARROW_SHAFT_SPACING).to_le_bytes().to_vec());
    }
    set_arrow_geometry(&mut a, tail, head, arrow.kind);
    vec![g, a]
}

/// A text's characters (lines joined by ChemDraw's `\r`) with their style runs.
fn text_runs(text: &PageText) -> (Vec<(u16, u16)>, String) {
    let mut rich = RichText::new();
    for (k, line) in text.lines().into_iter().enumerate() {
        if k > 0 {
            rich = rich.text("\r");
        }
        for span in line.spans {
            rich = rich.span(span.text, span.style);
        }
    }
    cdx::runs_of(&rich)
}

/// Where ChemDraw will draw `text` anchored at `at` (CDX units): an estimate from Arial's
/// widths at 10 pt, as for atom labels.
fn text_bbox(text: &PageText, at: (f64, f64)) -> [f64; 4] {
    let lines = text.lines();
    let scale = TEXT_PT / 10.0;
    let width = |line: &RichText| -> f64 {
        line.spans
            .iter()
            .map(|s| {
                let k = if s.style.script == Script::Normal { 1.0 } else { 0.7 };
                s.text.chars().map(cdx::arial_width).sum::<f64>() * k * scale
            })
            .sum()
    };
    let widest = lines.iter().map(width).fold(0.0, f64::max);
    let left = match text.align {
        TextAlign::Left => at.0,
        TextAlign::Center => at.0 - widest / 2.0,
        TextAlign::Right => at.0 - widest,
    };
    let line_height = TEXT_PT * TEXT_LINE_HEIGHT as f64 * PT;
    let top = at.1 - ASCENT_PT * PT;
    let bottom = at.1 + (lines.len().max(1) - 1) as f64 * line_height + DESCENT_PT * PT;
    [left, top, left + widest, bottom]
}

fn justification_value(align: TextAlign) -> Option<Vec<u8>> {
    match align {
        TextAlign::Left => None,
        TextAlign::Center => Some(vec![1]),
        TextAlign::Right => Some(vec![(-1i8) as u8]),
    }
}

/// Write `text`'s characters, alignment and box into `t`, anchored at `at` (CDX units).
fn set_text_content(t: &mut CdxObject, text: &PageText, at: (f64, f64)) {
    let (runs, chars) = text_runs(text);
    t.set(PROP_2D_POSITION, cdx::point(at.0, at.1));
    t.set(PROP_BOUNDING_BOX, cdx::rect(text_bbox(text, at)));
    match justification_value(text.align) {
        Some(v) => t.set(PROP_JUSTIFICATION, v),
        None => t.props.retain(|(tag, _)| *tag != PROP_JUSTIFICATION),
    }
    t.props.retain(|(tag, _)| *tag != PROP_CAPTION_JUSTIFICATION);
    t.set(PROP_TEXT, cdx::cdx_string(&runs, &chars));
    if chars.chars().any(|c| c as u32 >= 0x100) {
        t.set(PROP_UTF8_TEXT, utf8_string(&runs, &chars));
    } else {
        t.props.retain(|(tag, _)| *tag != PROP_UTF8_TEXT);
    }
}

fn new_text(text: &PageText, shift: (f64, f64), next: &mut u32) -> CdxObject {
    *next += 1;
    let mut t = CdxObject::new(OBJ_TEXT, *next);
    set_text_content(&mut t, text, cdx_point(text.pos, shift));
    t
}

/// Objects on a document's pages that reading made page objects of, taken off the pages to
/// be written again: by the source id of each, what reading made of it and its ChemDraw
/// objects (in document order).
pub(crate) type Owned = HashMap<u32, (PageObject, Vec<CdxObject>)>;

/// Take the objects reading converts ([`convert_page`]) off `page` (and out of its groups).
pub(crate) fn take_owned(page: &mut CdxObject, owned: &mut Owned) {
    let converted = convert_page(page);
    let mut source_of: HashMap<u32, u32> = HashMap::new();
    for c in &converted {
        let source = c.object.source().expect("read objects have a source");
        for id in &c.ids {
            source_of.insert(*id, source);
        }
        owned.insert(source, (c.object.clone(), Vec::new()));
    }
    fn take(obj: &mut CdxObject, source_of: &HashMap<u32, u32>, owned: &mut Owned) {
        let mut kept = Vec::with_capacity(obj.children.len());
        for mut child in std::mem::take(&mut obj.children) {
            if child.tag == OBJ_GROUP {
                take(&mut child, source_of, owned);
                kept.push(child);
            } else if let Some(source) = source_of.get(&child.id) {
                owned.get_mut(source).expect("registered above").1.push(child);
            } else {
                kept.push(child);
            }
        }
        obj.children = kept;
    }
    take(page, &source_of, owned);
    // A group left empty goes too.
    fn prune(obj: &mut CdxObject) {
        obj.children.retain_mut(|c| {
            if c.tag == OBJ_GROUP {
                prune(c);
                !c.children.is_empty()
            } else {
                true
            }
        });
    }
    prune(page);
}

/// Whether two CDX points are the same, give or take the rounding of a round trip.
fn same_point(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 2.0 && (a.1 - b.1).abs() < 2.0
}

/// The ChemDraw objects for `objects`, moved by `shift` (CDX units), with new ids from `next`.
/// An object whose source is in `owned` is written as its original objects with what changed
/// rewritten; any other is written new.
pub(crate) fn write_objects(objects: &[PageObject], owned: &mut Owned, shift: (f64, f64), next: &mut u32) -> Vec<CdxObject> {
    let mut out = Vec::new();
    for object in objects {
        let original = object.source().and_then(|s| owned.remove(&s));
        match (object, original) {
            (PageObject::Arrow(a), Some((PageObject::Arrow(read), mut cdx_objects))) if read.kind == a.kind => {
                let (tail, head) = (cdx_point(a.tail, shift), cdx_point(a.head, shift));
                let moved = !same_point(tail, cdx_point(read.tail, (0.0, 0.0))) || !same_point(head, cdx_point(read.head, (0.0, 0.0)));
                if moved {
                    for o in &mut cdx_objects {
                        set_arrow_geometry(o, tail, head, a.kind);
                    }
                }
                out.extend(cdx_objects);
            }
            (PageObject::Text(t), Some((PageObject::Text(read), mut cdx_objects))) if cdx_objects.len() == 1 => {
                let at = cdx_point(t.pos, shift);
                let was = cdx_point(read.pos, (0.0, 0.0));
                let obj = &mut cdx_objects[0];
                if t.text != read.text || t.align != read.align {
                    set_text_content(obj, t, at);
                } else if !same_point(at, was) {
                    // Only moved: keep ChemDraw's own text (fonts, colours) and shift its box.
                    let (dx, dy) = (at.0 - was.0, at.1 - was.1);
                    obj.set(PROP_2D_POSITION, cdx::point(at.0, at.1));
                    if let Some(b) = cdx::bounding_box(obj) {
                        obj.set(PROP_BOUNDING_BOX, cdx::rect([b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy]));
                    }
                }
                out.extend(cdx_objects);
            }
            (PageObject::Arrow(a), _) => out.extend(new_arrow(a, shift, next)),
            (PageObject::Text(t), _) => {
                if !t.is_blank() {
                    out.push(new_text(t, shift, next));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx::{merge_into_cdx, molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml, CdxImport, OBJ_PAGE};
    use crate::molecule::cdx_codec;
    use crate::molecule::Molecule;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
    }

    /// The ChemDraw 25 reaction (`Reaction.cdx` of Open DrawChemistry): two structures, one
    /// reaction arrow (an `arrow` 92 superseding the line `graphic` 89; 89 and 81 in the CDXML
    /// save of the same drawing) and the compound labels
    /// "(I)" under each structure (texts 83 and 85, centred), plus a reaction scheme and two
    /// chemical properties.
    const ARROW: u32 = 92;
    const GRAPHIC: u32 = 89;

    fn reaction_cdx() -> Vec<u8> {
        fixture("chemdraw25_reaction.cdx")
    }

    fn arrows(m: &Molecule) -> Vec<&PageArrow> {
        m.objects.iter().filter_map(PageObject::as_arrow).collect()
    }

    fn texts(m: &Molecule) -> Vec<&PageText> {
        m.objects.iter().filter_map(PageObject::as_text).collect()
    }

    /// Molecule units to points.
    fn pt(v: f32) -> f32 {
        v * 14.4 / 1.5
    }

    fn assert_reaction(import: &CdxImport, what: &str, arrow_id: u32) {
        let m = &import.molecule;
        let a = arrows(m);
        assert_eq!(a.len(), 1, "{what}: one arrow, not one per graphic and arrow object");
        assert_eq!(a[0].kind, ArrowKind::Reaction);
        assert_eq!(a[0].source, Some(arrow_id), "{what}: the arrow object is the source");
        assert!((pt(a[0].tail[0]) - 130.95).abs() < 0.02 && (pt(a[0].head[0]) - 161.15).abs() < 0.02, "{what}: {:?}", a[0]);
        assert!((pt(a[0].tail[1]) - 47.02).abs() < 0.02 && (pt(a[0].head[1]) - 47.02).abs() < 0.02, "{what}: {:?}", a[0]);
        let t = texts(m);
        assert_eq!(t.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["(I)", "(I)"], "{what}");
        assert!(t.iter().all(|t| t.align == TextAlign::Center), "{what}");
        assert_eq!(t.iter().map(|t| t.source).collect::<Vec<_>>(), [Some(83), Some(85)], "{what}");
        assert!((pt(t[0].pos[0]) - 71.98).abs() < 0.02 && (pt(t[0].pos[1]) - 98.32).abs() < 0.02, "{what}: {:?}", t[0]);
        // The reaction scheme and the two chemical properties stay unshown.
        assert_eq!(import.other_objects, 3, "{what}");
        assert!(!m.atoms.is_empty());
    }

    #[test]
    fn a_chemdraw_reaction_reads_its_arrow_and_texts() {
        assert_reaction(&read_cdx(&reaction_cdx()).unwrap(), "CDX", ARROW);
        let xml = String::from_utf8(fixture("chemdraw25_reaction.cdxml")).unwrap();
        assert_reaction(&read_cdxml(&xml).unwrap(), "CDXML", 89);
    }

    fn all_objects(doc: &CdxObject) -> Vec<&CdxObject> {
        fn walk<'a>(o: &'a CdxObject, out: &mut Vec<&'a CdxObject>) {
            out.push(o);
            o.children.iter().for_each(|c| walk(c, out));
        }
        let mut out = Vec::new();
        walk(doc, &mut out);
        out
    }

    fn find(doc: &CdxObject, id: u32, tag: u16) -> Option<CdxObject> {
        all_objects(doc).into_iter().find(|o| o.id == id && o.tag == tag).cloned()
    }

    #[test]
    fn saving_unedited_keeps_chemdraws_arrow_and_texts_as_they_were() {
        let original = reaction_cdx();
        let import = read_cdx(&original).unwrap();
        let saved = merge_into_cdx(&original, &import.molecule);
        let again = read_cdx(&saved).unwrap();
        assert_reaction(&again, "saved", ARROW);
        let (before, after) = (cdx_codec::parse(&original).unwrap(), cdx_codec::parse(&saved).unwrap());
        for (id, tag) in [(GRAPHIC, OBJ_GRAPHIC), (ARROW, OBJ_ARROW), (83, OBJ_TEXT), (85, OBJ_TEXT)] {
            assert_eq!(find(&after, id, tag), find(&before, id, tag), "object {id} untouched");
        }
        // Nothing doubled: one graphic and one arrow object, under their ids.
        let all = all_objects(&after);
        assert_eq!(all.iter().filter(|o| o.tag == OBJ_ARROW).count(), 1);
        assert_eq!(all.iter().filter(|o| o.tag == OBJ_GRAPHIC).count(), 1);
        // Saved twice, still the same.
        let twice = merge_into_cdx(&saved, &again.molecule);
        assert_reaction(&read_cdx(&twice).unwrap(), "saved twice", ARROW);
    }

    #[test]
    fn a_moved_arrow_and_text_are_saved_where_they_are_now() {
        let original = reaction_cdx();
        let mut m = read_cdx(&original).unwrap().molecule;
        for o in &mut m.objects {
            if o.source() == Some(ARROW) || o.source() == Some(83) {
                o.translate([0.0, 1.5]);
            }
        }
        let saved = merge_into_cdx(&original, &m);
        let again = read_cdx(&saved).unwrap();
        let a = arrows(&again.molecule)[0];
        assert!((pt(a.tail[1]) - (47.02 + 14.4)).abs() < 0.02, "{a:?}");
        assert!((pt(a.head[0]) - 161.15).abs() < 0.02, "{a:?}");
        let t = texts(&again.molecule);
        assert!((pt(t[0].pos[1]) - (98.32 + 14.4)).abs() < 0.02, "{:?}", t[0]);
        assert!((pt(t[1].pos[1]) - 98.32).abs() < 0.02, "the other label stays: {:?}", t[1]);
        assert_eq!(again.other_objects, 3);
        // The graphic moved with the arrow object, so old readers see the same arrow; the
        // moved text keeps ChemDraw's own string.
        let doc = cdx_codec::parse(&saved).unwrap();
        let (head, tail) = graphic_ends(&find(&doc, GRAPHIC, OBJ_GRAPHIC).unwrap()).unwrap();
        assert!(((tail.1 / PT) - (47.02 + 14.4)).abs() < 0.02 && ((head.1 / PT) - (47.02 + 14.4)).abs() < 0.02);
        let before = cdx_codec::parse(&original).unwrap();
        assert_eq!(find(&doc, 83, OBJ_TEXT).unwrap().prop(PROP_TEXT), find(&before, 83, OBJ_TEXT).unwrap().prop(PROP_TEXT));
    }

    #[test]
    fn deleted_and_edited_objects_are_saved_as_they_are_now() {
        let original = reaction_cdx();
        let mut m = read_cdx(&original).unwrap().molecule;
        let arrow = m.objects.iter().find(|o| o.source() == Some(ARROW)).unwrap().id();
        assert!(m.remove_object(arrow));
        if let Some(PageObject::Text(t)) = m.objects.iter_mut().find(|o| o.source() == Some(85)) {
            t.text = "(II)".into();
        }
        let saved = merge_into_cdx(&original, &m);
        let again = read_cdx(&saved).unwrap();
        assert!(arrows(&again.molecule).is_empty(), "the deleted arrow is gone");
        let doc = cdx_codec::parse(&saved).unwrap();
        assert!(find(&doc, GRAPHIC, OBJ_GRAPHIC).is_none() && find(&doc, ARROW, OBJ_ARROW).is_none());
        assert_eq!(texts(&again.molecule).iter().map(|t| t.text.as_str()).collect::<Vec<_>>(), ["(I)", "(II)"]);
        // Still the chemical property's display: the same id.
        assert_eq!(texts(&again.molecule)[1].source, Some(85));
        assert_eq!(again.other_objects, 3);
    }

    #[test]
    fn new_arrows_and_text_join_a_chemdraw_document() {
        let original = reaction_cdx();
        let mut m = read_cdx(&original).unwrap().molecule;
        m.add_arrow([20.0, 10.0], [24.0, 10.0], ArrowKind::Equilibrium);
        m.add_text([20.0, 9.0], "H_2O, 25 \u{b0}C");
        let saved = merge_into_cdx(&original, &m);
        let again = read_cdx(&saved).unwrap();
        assert_eq!(arrows(&again.molecule).len(), 2);
        assert_eq!(texts(&again.molecule).len(), 3);
        assert_eq!(texts(&again.molecule)[2].text, "H_2O, 25 \u{b0}C");
        let max_before = cdx_codec::parse(&original).unwrap().max_id();
        let new_ids: Vec<u32> = again.molecule.objects.iter().filter_map(|o| o.source()).filter(|&s| s > max_before).collect();
        assert_eq!(new_ids.len(), 2, "new objects get ids the document never used");
        assert_eq!(again.other_objects, 3);
    }

    /// A new drawing with both arrow kinds, a two-line text and a centred one.
    fn drawn() -> Molecule {
        let mut m = Molecule::default();
        // Away from the page's top-left margin, so writing moves nothing.
        m.add_atom("C".into(), [5.0, 5.0], 0);
        m.add_arrow([7.0, 5.0], [11.5, 5.0], ArrowKind::Reaction);
        m.add_arrow([7.0, 8.0], [11.5, 6.0], ArrowKind::Equilibrium);
        m.add_text([7.5, 4.2], "Pd(PPh_3)_4\nTHF, 60 \u{b0}C");
        m.push_object(PageObject::Text(PageText { id: 0, pos: [9.0, 7.0], text: "K_2CO_3".into(), align: TextAlign::Center, source: None }));
        m
    }

    fn same_objects(a: &Molecule, b: &Molecule) {
        assert_eq!(a.objects.len(), b.objects.len());
        for (x, y) in a.objects.iter().zip(&b.objects) {
            match (x, y) {
                (PageObject::Arrow(x), PageObject::Arrow(y)) => {
                    assert_eq!(x.kind, y.kind);
                    for (p, q) in [(x.tail, y.tail), (x.head, y.head)] {
                        assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3, "{x:?} vs {y:?}");
                    }
                }
                (PageObject::Text(x), PageObject::Text(y)) => {
                    assert_eq!((&x.text, x.align), (&y.text, y.align));
                    assert!((x.pos[0] - y.pos[0]).abs() < 1e-3 && (x.pos[1] - y.pos[1]).abs() < 1e-3, "{x:?} vs {y:?}");
                }
                _ => panic!("{x:?} vs {y:?}"),
            }
        }
    }

    #[test]
    fn drawn_arrows_and_text_survive_cdx_and_cdxml() {
        let m = drawn();
        let from_cdx = read_cdx(&molecule_to_cdx_document(&m)).unwrap();
        same_objects(&m, &from_cdx.molecule);
        assert_eq!(from_cdx.other_objects, 0);
        let xml = molecule_to_cdxml(&m);
        let from_xml = read_cdxml(&xml).unwrap();
        same_objects(&m, &from_xml.molecule);
        // Everything written is CDXML that ChemDraw's format knows.
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert!(xml.contains("ArrowheadHead=\"Full\"") && xml.contains("ArrowheadHead=\"HalfLeft\""), "{xml}");
        assert!(xml.contains("ArrowType=\"Equilibrium\"") && xml.contains("Justification=\"Center\""), "{xml}");
        assert!(cdx_file_rs::file::CdxFile::from_bytes(&molecule_to_cdx_document(&m)).is_ok());
        // Merged into ChemDraw's document, too.
        let merged = merge_into_cdx(&reaction_cdx(), &m);
        let doc = cdx_codec::parse(&merged).unwrap();
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&crate::molecule::cdx::cdxml_of_document(&doc)).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    }

    #[test]
    fn an_arrow_alone_is_still_a_document() {
        let mut m = Molecule::default();
        m.add_arrow([0.0, 0.0], [3.0, 0.0], ArrowKind::Reaction);
        let bytes = crate::molecule::cdx::molecule_to_cdx_bytes(&m).expect("an arrow is something to copy");
        assert_eq!(arrows(&read_cdx(&bytes).unwrap().molecule).len(), 1);
    }

    /// Arrows the editor cannot draw (a curved one, a hollow retrosynthetic head, a resonance
    /// arrow) stay ChemDraw's: counted as other objects and written back unchanged.
    #[test]
    fn arrows_the_editor_cannot_draw_are_left_alone() {
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&drawn())).unwrap();
        let page = doc.children.iter_mut().find(|c| c.tag == OBJ_PAGE).unwrap();
        let mut curved = CdxObject::new(OBJ_ARROW, 900);
        curved.set(PROP_ARROWHEAD_HEAD, HEAD_FULL.to_le_bytes().to_vec());
        curved.set(PROP_ANGULAR_SIZE, 900i16.to_le_bytes().to_vec());
        set_arrow_geometry(&mut curved, (0.0, 0.0), (100.0 * PT, 0.0), ArrowKind::Reaction);
        let mut hollow = curved.clone();
        hollow.id = 901;
        hollow.props.retain(|(t, _)| *t != PROP_ANGULAR_SIZE);
        hollow.set(PROP_ARROWHEAD_TYPE, 2i16.to_le_bytes().to_vec());
        let mut resonance = CdxObject::new(OBJ_GRAPHIC, 902);
        resonance.set(PROP_GRAPHIC_TYPE, GRAPHIC_LINE.to_le_bytes().to_vec());
        resonance.set(PROP_ARROW_TYPE, 4i16.to_le_bytes().to_vec());
        resonance.set(PROP_BOUNDING_BOX, cdx::rect([0.0, 0.0, 50.0 * PT, 0.0]));
        page.children.extend([curved.clone(), hollow.clone(), resonance.clone()]);
        let bytes = cdx_codec::write(&doc);
        let import = read_cdx(&bytes).unwrap();
        assert_eq!(arrows(&import.molecule).len(), 2, "only the two drawn ones");
        assert_eq!(import.other_objects, 3);
        let saved = cdx_codec::parse(&merge_into_cdx(&bytes, &import.molecule)).unwrap();
        for o in [curved, hollow, resonance] {
            assert_eq!(find(&saved, o.id, o.tag), Some(o));
        }
    }

    #[test]
    fn multi_line_markup_reads_back_line_by_line() {
        let rich = RichText::parse("a_2").text("\nb").superscript("+");
        assert_eq!(markup_lines(&rich), "a_2\nb^+");
    }

    /// Every ChemDraw fixture reads, saves and reads again with the same page objects and the
    /// same number of other objects.
    #[test]
    fn chemdraw_fixtures_round_trip_their_page_objects() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("cdx") {
                continue;
            }
            let bytes = std::fs::read(&path).unwrap();
            let Some(first) = read_cdx(&bytes) else { continue };
            let again = read_cdx(&merge_into_cdx(&bytes, &first.molecule)).unwrap();
            assert_eq!(again.other_objects, first.other_objects, "{path:?}");
            same_objects(&first.molecule, &again.molecule);
        }
    }
}
