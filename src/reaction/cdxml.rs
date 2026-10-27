//! A reaction scheme as a ChemDraw document — CDXML, or binary CDX, which hold the same object
//! tree — read and written.
//!
//! **Writing.** The scheme is laid out again in ChemDraw's own units
//! ([`ReactionStyle::chemdraw`]: 14.4 pt bonds, 10 pt text), so it opens in ChemDraw looking
//! like a scheme drawn there. Every structure is a fragment (its compound label a text bound to
//! it by a chemical property, as ChemDraw binds one), every arrow a line graphic with a full
//! head, the text over and under it texts, and each row a reaction scheme whose steps name, per
//! arrow, the structures before and after it on the row, the arrow and what rides on it.
//!
//! **Reading** takes what ChemDraw writes back apart. The steps say which texts and
//! structures ride on which arrow; where a document has no steps, a text or structure over or
//! under an arrow's shaft rides on it. A text a chemical property displays for a structure is
//! that structure's compound label, and so is a free text right under the structure or its
//! label (a bold **4** over `(V)`). Arrows a little apart in height make rows; every other
//! object goes to the row whose reaction line is nearest, left to right along it, and rows are
//! separated by [`SchemeItem::LineBreak`].

use std::collections::{HashMap, HashSet};

use egui::{Painter, Pos2, Rect, Vec2};

use crate::config::StyleConfig;
use crate::molecule::Molecule;
use crate::molecule::cdx::{
    self, LABEL_FONT, MAX_READ_ATOMS, MAX_READ_BONDS, OBJ_BOND, OBJ_FRAGMENT, OBJ_GROUP, OBJ_NODE, OBJ_PAGE,
    OBJ_TEXT, PROP_2D_POSITION, PROP_BOUNDING_BOX, PROP_TEXT, PROP_UTF8_TEXT, PT,
};
use crate::molecule::cdx_codec::{self, CdxObject};
use crate::widget::ChemStructEditor;
use crate::widget::rich_text::RichText;

use super::scheme::{
    Arrow, ArrowLabel, PlacedArrowLabel, PlacedKind, ReactionScheme, SchemeItem, SchemeLayout, SchemeStructure,
    measure_scheme,
};
use super::style::ReactionStyle;

const OBJ_GRAPHIC: u16 = 0x8007;
const OBJ_REACTION_SCHEME: u16 = 0x800D;
const OBJ_REACTION_STEP: u16 = 0x800E;
const OBJ_CHEMICAL_PROPERTY: u16 = 0x8020;
const OBJ_ARROW: u16 = 0x8021;

const PROP_SUPERSEDED_BY: u16 = 0x0013;
const PROP_HEAD_3D: u16 = 0x0207;
const PROP_TAIL_3D: u16 = 0x0208;
const PROP_JUSTIFICATION: u16 = 0x0701;
const PROP_GRAPHIC_TYPE: u16 = 0x0A00;
const PROP_ARROW_TYPE: u16 = 0x0A02;
const PROP_ARROWHEAD_SIZE: u16 = 0x0A20;
const PROP_BASIS_OBJECTS: u16 = 0x0B82;
const PROP_CHEMICAL_PROPERTY_TYPE: u16 = 0x0BB0;
const PROP_CHEMICAL_PROPERTY_DISPLAY_ID: u16 = 0x0BB1;
const PROP_CHEMICAL_PROPERTY_IS_ACTIVE: u16 = 0x0BB2;
const PROP_STEP_REACTANTS: u16 = 0x0C01;
const PROP_STEP_PRODUCTS: u16 = 0x0C02;
const PROP_STEP_ARROWS: u16 = 0x0C04;
const PROP_STEP_ABOVE_ARROW: u16 = 0x0C05;
const PROP_STEP_BELOW_ARROW: u16 = 0x0C06;

const GRAPHIC_LINE: i16 = 1;
const ARROW_FULL_HEAD: i16 = 2;
/// `Justification="Center"`.
const JUSTIFY_CENTER: u8 = 1;
/// The chemical property type ChemDraw 26 gives a structure's displayed label.
const CHEMICAL_PROPERTY_LABEL: u32 = 22;
/// The arrow head size ChemDraw writes (`HeadSize="2250"`).
const ARROWHEAD_SIZE: i16 = 2250;
/// Where the figure starts on the page, in points.
const PAGE_MARGIN_PT: f32 = 36.0;
/// From the top of 10 pt text to its baseline, in points (as [`cdx::label_geometry`] has it).
const ASCENT_PT: f32 = 7.2;

impl ReactionStyle {
    /// This style in ChemDraw's units: one figure unit is a point, a bond 14.4 pt, text 10 pt,
    /// arrows and gaps as ChemDraw draws them. A row limit keeps its proportion to the bond.
    pub fn chemdraw(&self) -> ReactionStyle {
        let to_pt = 14.4 / self.bond_length;
        ReactionStyle {
            bond_length: 14.4,
            item_gap: 10.0,
            arrow_length: 72.0,
            arrow_width: 0.6,
            arrow_head_length: 6.0,
            arrow_head_half_width: 2.5,
            arrow_text_gap: 4.0,
            arrow_font_size: 10.0,
            plus_font_size: 10.0,
            scheme_font_size: 10.0,
            compound_label_font_size: 10.0,
            compound_label_gap: 6.0,
            row_gap: 30.0,
            max_row_width: self.max_row_width.map(|w| w * to_pt),
            ..self.clone()
        }
    }
}

// ─── Writing ─────────────────────────────────────────────────────────────────

/// `scheme` as a ChemDraw document (write it with [`cdx_codec::write`] or [`cdxml_of`]). The
/// painter measures text; `scratch` is a drawing context for structures, as for
/// [`measure_scheme`].
pub fn scheme_document(
    scheme: &ReactionScheme,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) -> CdxObject {
    let pt_style = style.chemdraw();
    // Atom labels in proportion to the bonds, as in the figure.
    let mut structure_style = structure_style.clone();
    let to_pt = 14.4 / style.bond_length;
    structure_style.label_size *= to_pt;
    structure_style.label_bg_pad *= to_pt;
    structure_style.label_trim_pad *= to_pt;
    let layout = measure_scheme(scheme, painter, &pt_style, &structure_style, scratch);
    Writer::new(&pt_style).document(scheme, &layout)
}

/// A ChemDraw document as CDXML text.
pub fn cdxml_of(doc: &CdxObject) -> String {
    cdx::cdxml_of_document(doc)
}

/// What one scheme item became: the objects that stand for it in a step.
#[derive(Default, Clone)]
struct Written {
    /// The fragments of a structure.
    fragments: Vec<u32>,
    /// An arrow's graphic, and the objects over and under it.
    arrow: Option<u32>,
    above: Vec<u32>,
    below: Vec<u32>,
}

struct Writer<'a> {
    style: &'a ReactionStyle,
    next: u32,
    page: CdxObject,
}

impl<'a> Writer<'a> {
    fn new(style: &'a ReactionStyle) -> Self {
        Self { style, next: 1, page: CdxObject::new(OBJ_PAGE, 1) }
    }

    fn id(&mut self) -> u32 {
        self.next += 1;
        self.next
    }

    fn document(mut self, scheme: &ReactionScheme, layout: &SchemeLayout) -> CdxObject {
        let page_at = |p: Pos2| ((p.x + PAGE_MARGIN_PT) as f64 * PT, (p.y + PAGE_MARGIN_PT) as f64 * PT);
        let mut written: Vec<Written> = Vec::with_capacity(scheme.items.len());
        for (item, placed) in scheme.items.iter().zip(&layout.items) {
            let line = layout.line_of(written.len());
            let mut w = Written::default();
            match (item, &placed.kind) {
                (SchemeItem::Structure(s), PlacedKind::Structure { origin, label }) => {
                    let frags = self.structure(s.molecule(), *origin);
                    w.fragments = frags.iter().map(|f| f.0).collect();
                    if let Some(label) = label {
                        let text = self.text(s.label(), label.rect, true);
                        self.bind_label(text, &frags);
                    }
                }
                (SchemeItem::Plus, PlacedKind::Text(t)) => {
                    let rect = Rect::from_center_size(Pos2::new(placed.rect.center().x, line), t.size);
                    self.text(&RichText::plain("+"), rect, true);
                }
                (SchemeItem::Text(text), PlacedKind::Text(t)) => {
                    let rect = Rect::from_center_size(Pos2::new(placed.rect.center().x, line), t.size);
                    self.text(text, rect, true);
                }
                (SchemeItem::Arrow(arrow), PlacedKind::Arrow { length, above, below }) => {
                    let tail = page_at(Pos2::new(placed.rect.min.x, line));
                    let head = page_at(Pos2::new(placed.rect.min.x + length, line));
                    let id = self.id();
                    let mut g = CdxObject::new(OBJ_GRAPHIC, id);
                    g.set(PROP_BOUNDING_BOX, cdx::rect([head.0, head.1, tail.0, tail.1]));
                    g.set(PROP_GRAPHIC_TYPE, GRAPHIC_LINE.to_le_bytes().to_vec());
                    g.set(PROP_ARROW_TYPE, ARROW_FULL_HEAD.to_le_bytes().to_vec());
                    g.set(PROP_ARROWHEAD_SIZE, ARROWHEAD_SIZE.to_le_bytes().to_vec());
                    self.page.children.push(g);
                    w.arrow = Some(id);
                    let half_shaft = (self.style.arrow_width * 0.5).max(self.style.arrow_head_half_width);
                    let centre_x = placed.rect.min.x + length * 0.5;
                    for (label, placed_label, is_above) in [(&arrow.above, above, true), (&arrow.below, below, false)] {
                        let ids = match (label, placed_label) {
                            (ArrowLabel::Text(text), PlacedArrowLabel::Text(t)) => {
                                let top = if is_above {
                                    line - half_shaft - self.style.arrow_text_gap - t.size.y
                                } else {
                                    line + half_shaft + self.style.arrow_text_gap
                                };
                                let rect = Rect::from_min_size(Pos2::new(centre_x - t.size.x * 0.5, top), t.size);
                                vec![self.text(text, rect, true)]
                            }
                            (ArrowLabel::Structure(s), PlacedArrowLabel::Structure { origin }) => {
                                self.structure(s.molecule(), *origin).iter().map(|f| f.0).collect()
                            }
                            _ => Vec::new(),
                        };
                        if is_above {
                            w.above = ids;
                        } else {
                            w.below = ids;
                        }
                    }
                }
                _ => {}
            }
            written.push(w);
        }
        self.steps(layout, &written);

        let mut doc = cdx::new_document();
        let mut page = std::mem::take(&mut self.page);
        if let Some(bbox) = page.children.iter().filter_map(cdx::bounding_box).reduce(|a, b| cdx::union(Some(a), b)) {
            page.props.insert(0, (PROP_BOUNDING_BOX, cdx::rect(bbox)));
            doc.props.insert(1, (PROP_BOUNDING_BOX, cdx::rect(bbox)));
        }
        doc.children.push(page);
        doc
    }

    /// Fragments for a molecule whose origin the layout put at `origin`; each with the ids of
    /// its nodes and bonds.
    fn structure(&mut self, mol: &Molecule, origin: Vec2) -> Vec<(u32, Vec<u32>)> {
        let shift = ((origin.x + PAGE_MARGIN_PT) as f64 * PT, (origin.y + PAGE_MARGIN_PT) as f64 * PT);
        let frags = cdx::fragments(mol, shift, &mut self.next);
        let out = frags
            .iter()
            .map(|f| (f.id, f.children.iter().filter(|c| matches!(c.tag, OBJ_NODE | OBJ_BOND)).map(|c| c.id).collect()))
            .collect();
        self.page.children.extend(frags);
        out
    }

    /// A text filling `rect` (figure points): centred on it, or from its left edge.
    fn text(&mut self, text: &RichText, rect: Rect, centred: bool) -> u32 {
        let (runs, chars) = cdx::runs_of(text);
        let chars = chars.replace('\n', "\r");
        let x = if centred { rect.center().x } else { rect.min.x };
        let at = |x: f32, y: f32| ((x + PAGE_MARGIN_PT) as f64 * PT, (y + PAGE_MARGIN_PT) as f64 * PT);
        let (px, py) = at(x, rect.min.y + ASCENT_PT);
        let (l, t) = at(rect.min.x, rect.min.y);
        let (r, b) = at(rect.max.x, rect.max.y);
        let id = self.id();
        let mut t_obj = CdxObject::new(OBJ_TEXT, id);
        t_obj.set(PROP_2D_POSITION, cdx::point(px, py));
        t_obj.set(PROP_BOUNDING_BOX, cdx::rect([l, t, r, b]));
        if centred {
            t_obj.set(PROP_JUSTIFICATION, vec![JUSTIFY_CENTER]);
        }
        t_obj.set(PROP_TEXT, cdx::cdx_string(&runs, &chars));
        if chars.chars().any(|c| c as u32 >= 0x100) {
            t_obj.set(PROP_UTF8_TEXT, utf8_string(&runs, &chars));
        }
        self.page.children.push(t_obj);
        id
    }

    /// Bind a compound label to its structure, as ChemDraw does: a chemical property whose
    /// basis is the structure's nodes and bonds and whose display is the label.
    fn bind_label(&mut self, text: u32, frags: &[(u32, Vec<u32>)]) {
        let basis: Vec<u32> = frags.iter().flat_map(|f| f.1.iter().copied()).collect();
        let id = self.id();
        let mut p = CdxObject::new(OBJ_CHEMICAL_PROPERTY, id);
        p.set(PROP_CHEMICAL_PROPERTY_DISPLAY_ID, text.to_le_bytes().to_vec());
        p.set(PROP_CHEMICAL_PROPERTY_IS_ACTIVE, vec![1]);
        p.set(PROP_CHEMICAL_PROPERTY_TYPE, CHEMICAL_PROPERTY_LABEL.to_le_bytes().to_vec());
        p.set(PROP_BASIS_OBJECTS, ids_value(&basis));
        self.page.children.push(p);
    }

    /// A reaction scheme per row, a step per arrow: the structures between the arrow and the
    /// one before it (or the row's start) react to those up to the next arrow (or the row's end).
    fn steps(&mut self, layout: &SchemeLayout, written: &[Written]) {
        for row in &layout.rows {
            let items = &written[row.items.clone()];
            let arrows: Vec<usize> = items.iter().enumerate().filter(|(_, w)| w.arrow.is_some()).map(|(i, _)| i).collect();
            if arrows.is_empty() {
                continue;
            }
            let scheme_id = self.id();
            let mut scheme = CdxObject::new(OBJ_REACTION_SCHEME, scheme_id);
            for (k, &a) in arrows.iter().enumerate() {
                let from = if k == 0 { 0 } else { arrows[k - 1] + 1 };
                let to = arrows.get(k + 1).copied().unwrap_or(items.len());
                let fragments = |range: std::ops::Range<usize>| -> Vec<u32> {
                    items[range].iter().flat_map(|w| w.fragments.iter().copied()).collect()
                };
                let id = self.id();
                let mut step = CdxObject::new(OBJ_REACTION_STEP, id);
                let w = &items[a];
                for (tag, ids) in [
                    (PROP_STEP_REACTANTS, fragments(from..a)),
                    (PROP_STEP_PRODUCTS, fragments(a + 1..to)),
                    (PROP_STEP_ARROWS, w.arrow.into_iter().collect()),
                    (PROP_STEP_ABOVE_ARROW, w.above.clone()),
                    (PROP_STEP_BELOW_ARROW, w.below.clone()),
                ] {
                    if !ids.is_empty() {
                        step.set(tag, ids_value(&ids));
                    }
                }
                scheme.children.push(step);
            }
            self.page.children.push(scheme);
        }
    }
}

fn ids_value(ids: &[u32]) -> Vec<u8> {
    ids.iter().flat_map(|id| id.to_le_bytes()).collect()
}

/// A CDXString in UTF-8 (the UTF-8 Text property): the style runs, then the text.
fn utf8_string(runs: &[(u16, u16)], text: &str) -> Vec<u8> {
    let mut out = (runs.len() as u16).to_le_bytes().to_vec();
    for &(start, face) in runs {
        for v in [start, LABEL_FONT, face, 200, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(text.as_bytes());
    out
}

// ─── Reading ─────────────────────────────────────────────────────────────────

/// The reaction scheme a CDXML document draws; `None` when it is no CDXML, draws nothing, or
/// is too large to read.
pub fn scheme_from_cdxml(xml: &str) -> Option<ReactionScheme> {
    scheme_from_document(&cdx::cdxml_document(xml)?)
}

/// The reaction scheme a CDX document draws; see [`scheme_from_cdxml`].
pub fn scheme_from_cdx(bytes: &[u8]) -> Option<ReactionScheme> {
    scheme_from_document(&cdx_codec::parse(bytes)?)
}

/// A point, in CDX units (1/65536 pt).
type P = (f64, f64);

/// A box in CDX units: left, top, right, bottom.
type B = [f64; 4];

struct ReadFragment {
    id: u32,
    bbox: B,
    molecule: Molecule,
    /// Its nodes and bonds, which a chemical property's basis names.
    members: HashSet<u32>,
}

struct ReadText {
    id: u32,
    bbox: B,
    text: RichText,
}

struct ReadArrow {
    /// The graphic, and the arrow object superseding it (a step can name either).
    ids: Vec<u32>,
    tail: P,
    head: P,
}

impl ReadArrow {
    fn y(&self) -> f64 {
        (self.tail.1 + self.head.1) * 0.5
    }
    fn x_range(&self) -> (f64, f64) {
        (self.tail.0.min(self.head.0), self.tail.0.max(self.head.0))
    }
}

#[derive(Default)]
struct ReadStep {
    arrows: Vec<u32>,
    above: Vec<u32>,
    below: Vec<u32>,
}

/// Everything on a document's pages that a scheme is made of.
#[derive(Default)]
struct Objects {
    fragments: Vec<ReadFragment>,
    texts: Vec<ReadText>,
    arrows: Vec<ReadArrow>,
    steps: Vec<ReadStep>,
    /// Chemical properties: the text displayed, and the objects it is about.
    properties: Vec<(u32, Vec<u32>)>,
}

/// The reaction scheme a CDX object tree draws.
pub fn scheme_from_document(doc: &CdxObject) -> Option<ReactionScheme> {
    if doc.tag != cdx::OBJ_DOCUMENT {
        return None;
    }
    let (mut atoms, mut bonds) = (0, 0);
    for page in doc.children.iter().filter(|c| c.tag == OBJ_PAGE) {
        cdx::count_structures(page, &mut atoms, &mut bonds);
    }
    if atoms > MAX_READ_ATOMS || bonds > MAX_READ_BONDS {
        return None;
    }
    let mut objects = Objects::default();
    let mut superseded: HashMap<u32, u32> = HashMap::new();
    let mut arrow_objects: Vec<ReadArrow> = Vec::new();
    for page in doc.children.iter().filter(|c| c.tag == OBJ_PAGE) {
        collect(page, &mut objects, &mut superseded, &mut arrow_objects);
    }
    // A graphic superseded by an arrow object is the same arrow: keep the arrow's geometry,
    // answer to both ids.
    for arrow in arrow_objects {
        let graphic = superseded.iter().find(|(_, a)| **a == arrow.ids[0]).map(|(g, _)| *g);
        match graphic.and_then(|g| objects.arrows.iter_mut().find(|a| a.ids.contains(&g))) {
            Some(existing) => {
                existing.ids.push(arrow.ids[0]);
                existing.tail = arrow.tail;
                existing.head = arrow.head;
            }
            None => objects.arrows.push(arrow),
        }
    }
    let scheme = assemble(objects);
    (!scheme.is_empty()).then_some(scheme)
}

fn collect(obj: &CdxObject, out: &mut Objects, superseded: &mut HashMap<u32, u32>, arrow_objects: &mut Vec<ReadArrow>) {
    for child in &obj.children {
        match child.tag {
            OBJ_GROUP => collect(child, out, superseded, arrow_objects),
            OBJ_FRAGMENT => {
                let mut molecule = Molecule::default();
                cdx::read_fragment(child, &mut molecule);
                if molecule.atoms.is_empty() {
                    continue;
                }
                let nodes = || child.children.iter().filter(|c| c.tag == OBJ_NODE);
                let bbox = cdx::bounding_box(child).or_else(|| {
                    nodes().filter_map(position).map(|(x, y)| [x, y, x, y]).reduce(|a, b| cdx::union(Some(a), b))
                });
                let Some(bbox) = bbox else { continue };
                let members = child.children.iter().filter(|c| matches!(c.tag, OBJ_NODE | OBJ_BOND)).map(|c| c.id).collect();
                out.fragments.push(ReadFragment { id: child.id, bbox, molecule, members });
            }
            OBJ_TEXT => {
                let Some(text) = text_of(child) else { continue };
                let bbox = cdx::bounding_box(child).or_else(|| position(child).map(|(x, y)| [x, y - 7.2 * PT, x, y + 2.1 * PT]));
                if let Some(bbox) = bbox {
                    out.texts.push(ReadText { id: child.id, bbox, text });
                }
            }
            OBJ_GRAPHIC => {
                if let Some(a) = child.prop(PROP_SUPERSEDED_BY).filter(|v| v.len() >= 4) {
                    superseded.insert(child.id, u32::from_le_bytes([a[0], a[1], a[2], a[3]]));
                }
                let is_arrow = child.prop_i16(PROP_GRAPHIC_TYPE) == Some(GRAPHIC_LINE)
                    && child.prop_i16(PROP_ARROW_TYPE).is_some_and(|t| t != 0);
                if let (true, Some(b)) = (is_arrow, cdx::bounding_box(child)) {
                    // A line's "bounding box" is its two ends: the head, then the tail.
                    out.arrows.push(ReadArrow { ids: vec![child.id], head: (b[0], b[1]), tail: (b[2], b[3]) });
                }
            }
            OBJ_ARROW => {
                if let (Some(head), Some(tail)) = (point_3d(child, PROP_HEAD_3D), point_3d(child, PROP_TAIL_3D)) {
                    arrow_objects.push(ReadArrow { ids: vec![child.id], head, tail });
                }
            }
            OBJ_REACTION_SCHEME => {
                for step in child.children.iter().filter(|c| c.tag == OBJ_REACTION_STEP) {
                    out.steps.push(ReadStep {
                        arrows: ids(step, PROP_STEP_ARROWS),
                        above: ids(step, PROP_STEP_ABOVE_ARROW),
                        below: ids(step, PROP_STEP_BELOW_ARROW),
                    });
                }
            }
            OBJ_CHEMICAL_PROPERTY => {
                if let Some(display) = ids(child, PROP_CHEMICAL_PROPERTY_DISPLAY_ID).first() {
                    out.properties.push((*display, ids(child, PROP_BASIS_OBJECTS)));
                }
            }
            _ => {}
        }
    }
}

fn position(obj: &CdxObject) -> Option<P> {
    let v = obj.prop(PROP_2D_POSITION).filter(|v| v.len() >= 8)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some((n(4), n(0)))
}

fn point_3d(obj: &CdxObject, tag: u16) -> Option<P> {
    let v = obj.prop(tag).filter(|v| v.len() >= 8)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some((n(0), n(4)))
}

fn ids(obj: &CdxObject, tag: u16) -> Vec<u32> {
    obj.prop(tag)
        .map(|v| v.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect())
        .unwrap_or_default()
}

/// A text object's text: its style runs over the UTF-8 text when it has one, the Latin-1 text
/// otherwise. Line breaks (carriage returns) become `\n`.
fn text_of(t: &CdxObject) -> Option<RichText> {
    let (runs, latin1) = cdx::parse_cdx_string(t.prop(PROP_TEXT)?)?;
    let utf8 = t
        .prop(PROP_UTF8_TEXT)
        .and_then(|v| cdx::parse_cdx_string(v).map(|(_, s)| s))
        .and_then(|s| String::from_utf8(s.chars().map(|c| c as u8).collect()).ok());
    let text = utf8.filter(|u| u.chars().count() == latin1.chars().count()).unwrap_or(latin1).replace('\r', "\n");
    let rich = cdx::rich_of(&runs, &text);
    (!rich.is_empty()).then_some(rich)
}

fn centre(b: &B) -> P {
    ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5)
}

/// Join texts read top to bottom into one, a line each.
fn join_lines(mut texts: Vec<&ReadText>) -> RichText {
    texts.sort_by(|a, b| a.bbox[1].total_cmp(&b.bbox[1]));
    let mut out = RichText::new();
    for (k, t) in texts.iter().enumerate() {
        if k > 0 {
            out = out.text("\n");
        }
        for span in &t.text.spans {
            out = out.span(span.text.clone(), span.style);
        }
    }
    out
}

/// The texts over and under an arrow, then the structures over and under it (indices).
type Riders = (Vec<usize>, Vec<usize>, Vec<usize>, Vec<usize>);

/// What a scheme item read from the page is, before it takes its place on a row.
enum Placed {
    Structure(usize),
    Arrow(usize),
    Text(usize),
}

fn assemble(objects: Objects) -> ReactionScheme {
    let Objects { fragments, texts, arrows, steps, properties } = objects;
    let text_index: HashMap<u32, usize> = texts.iter().enumerate().map(|(i, t)| (t.id, i)).collect();
    let fragment_index: HashMap<u32, usize> = fragments.iter().enumerate().map(|(i, f)| (f.id, i)).collect();
    let mut claimed_texts: HashSet<usize> = HashSet::new();
    let mut claimed_fragments: HashSet<usize> = HashSet::new();

    // Compound labels a chemical property displays.
    let mut labels: HashMap<usize, Vec<usize>> = HashMap::new();
    for (display, basis) in &properties {
        let Some(&t) = text_index.get(display) else { continue };
        let basis: HashSet<u32> = basis.iter().copied().collect();
        if let Some(f) = fragments.iter().position(|f| f.members.iter().any(|m| basis.contains(m)) || basis.contains(&f.id)) {
            labels.entry(f).or_default().push(t);
            claimed_texts.insert(t);
        }
    }

    // What rides on each arrow: what the steps say, else what sits over or under its shaft.
    let mut riders: Vec<Riders> = vec![Default::default(); arrows.len()];
    for step in &steps {
        let Some(a) = arrows.iter().position(|a| step.arrows.iter().any(|id| a.ids.contains(id))) else { continue };
        for (ids, above) in [(&step.above, true), (&step.below, false)] {
            for id in ids {
                let r = &mut riders[a];
                if let Some(&t) = text_index.get(id) {
                    if claimed_texts.insert(t) {
                        if above { r.0.push(t) } else { r.1.push(t) }
                    }
                } else if let Some(&f) = fragment_index.get(id)
                    && claimed_fragments.insert(f)
                {
                    if above { r.2.push(f) } else { r.3.push(f) }
                }
            }
        }
    }
    let near_shaft = |a: &ReadArrow, b: &B| {
        let (x0, x1) = a.x_range();
        let (cx, cy) = centre(b);
        cx >= x0 - 4.0 * PT && cx <= x1 + 4.0 * PT && (cy - a.y()).abs() < 30.0 * PT
    };
    for (a, arrow) in arrows.iter().enumerate() {
        if steps.iter().any(|s| s.arrows.iter().any(|id| arrow.ids.contains(id))) {
            continue;
        }
        for (t, text) in texts.iter().enumerate() {
            if !claimed_texts.contains(&t) && near_shaft(arrow, &text.bbox) && text.text.to_markup() != "+" {
                claimed_texts.insert(t);
                if centre(&text.bbox).1 < arrow.y() { riders[a].0.push(t) } else { riders[a].1.push(t) }
            }
        }
    }

    // A free text right under a structure, or right above or under its label, is (more of) its
    // label.
    for (t, text) in texts.iter().enumerate() {
        if claimed_texts.contains(&t) || text.text.to_markup() == "+" {
            continue;
        }
        let (cx, cy) = centre(&text.bbox);
        let owner = fragments.iter().enumerate().find(|(f, frag)| {
            if claimed_fragments.contains(f) {
                return false;
            }
            match labels.get(f) {
                Some(ls) => ls.iter().any(|&l| {
                    let lb = &texts[l].bbox;
                    cx >= lb[0] - 20.0 * PT && cx <= lb[2] + 20.0 * PT && (cy - centre(lb).1).abs() < 30.0 * PT
                }),
                None => cx >= frag.bbox[0] && cx <= frag.bbox[2] && text.bbox[1] >= frag.bbox[3] - 4.0 * PT && text.bbox[1] - frag.bbox[3] < 20.0 * PT,
            }
        });
        if let Some((f, _)) = owner {
            labels.entry(f).or_default().push(t);
            claimed_texts.insert(t);
        }
    }

    // Rows: arrows a little apart in height make one each; everything else joins the row whose
    // line is nearest its centre.
    let mut lines: Vec<f64> = Vec::new();
    let mut by_y: Vec<usize> = (0..arrows.len()).collect();
    by_y.sort_by(|&a, &b| arrows[a].y().total_cmp(&arrows[b].y()));
    for &a in &by_y {
        let y = arrows[a].y();
        if lines.last().is_none_or(|l| (y - l).abs() > 20.0 * PT) {
            lines.push(y);
        }
    }
    let row_of = |y: f64| -> usize {
        lines.iter().enumerate().min_by(|a, b| (a.1 - y).abs().total_cmp(&(b.1 - y).abs())).map_or(0, |(i, _)| i)
    };
    let mut rows: Vec<Vec<(f64, Placed)>> = (0..lines.len().max(1)).map(|_| Vec::new()).collect();
    for (f, frag) in fragments.iter().enumerate() {
        if !claimed_fragments.contains(&f) {
            let (x, y) = centre(&frag.bbox);
            rows[row_of(y)].push((x, Placed::Structure(f)));
        }
    }
    for (a, arrow) in arrows.iter().enumerate() {
        let (x0, x1) = arrow.x_range();
        rows[row_of(arrow.y())].push(((x0 + x1) * 0.5, Placed::Arrow(a)));
    }
    for (t, text) in texts.iter().enumerate() {
        if !claimed_texts.contains(&t) {
            let (x, y) = centre(&text.bbox);
            rows[row_of(y)].push((x, Placed::Text(t)));
        }
    }

    let mut scheme = ReactionScheme::new();
    let mut fragments: Vec<Option<ReadFragment>> = fragments.into_iter().map(Some).collect();
    let mut take_molecule = |f: usize| centred(fragments[f].take().map(|f| f.molecule).unwrap_or_default());
    let rider_label = |ts: &[usize], fs: &[usize], take: &mut dyn FnMut(usize) -> Molecule| -> ArrowLabel {
        match (fs.first(), ts.is_empty()) {
            (Some(&f), _) => ArrowLabel::Structure(take(f).into()),
            (None, false) => ArrowLabel::from(join_lines(ts.iter().map(|&t| &texts[t]).collect())),
            (None, true) => ArrowLabel::Empty,
        }
    };
    let row_count = rows.len();
    for (r, mut row) in rows.into_iter().enumerate() {
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (_, placed) in row {
            let item = match placed {
                Placed::Structure(f) => {
                    let label = labels.get(&f).map(|ls| join_lines(ls.iter().map(|&t| &texts[t]).collect()));
                    let s = SchemeStructure::new(take_molecule(f));
                    SchemeItem::Structure(match label {
                        Some(label) => s.with_label(label),
                        None => s,
                    })
                }
                Placed::Arrow(a) => {
                    let (above_t, below_t, above_f, below_f) = &riders[a];
                    let above = rider_label(above_t, above_f, &mut take_molecule);
                    let below = rider_label(below_t, below_f, &mut take_molecule);
                    SchemeItem::Arrow(Arrow { above, below, length: None })
                }
                Placed::Text(t) if texts[t].text.to_markup() == "+" => SchemeItem::Plus,
                Placed::Text(t) => SchemeItem::Text(texts[t].text.clone()),
            };
            scheme.add(item);
        }
        if r + 1 < row_count {
            scheme.add(SchemeItem::LineBreak);
        }
    }
    scheme
}

/// The molecule moved so its atoms are centred on the origin, as a structure drawn here is.
fn centred(mut mol: Molecule) -> Molecule {
    let (mut min, mut max) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for a in &mol.atoms {
        for i in 0..2 {
            min[i] = min[i].min(a.pos[i]);
            max[i] = max[i].max(a.pos[i]);
        }
    }
    if min[0].is_finite() {
        let c = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
        for a in &mut mol.atoms {
            a.pos = [a.pos[0] - c[0], a.pos[1] - c[1]];
        }
    }
    mol
}

#[cfg(test)]
mod tests;
