//! Reaction tables ([`ReactionTable`]) as ChemDraw's stoichiometry grid.
//!
//! ChemDraw (its *Reaction Analysis* table) writes a `stoichiometrygrid` (0x8022) placed at
//! its top-left corner (`p`), holding `sgcomponent`s (0x8023): first a header component for the
//! *Reactants* block and one for the *Products* block (`ComponentIsHeader`; the reactant one
//! also `ComponentIsReactant`), then one component per reactant and per product, each naming
//! the fragment it describes (`ComponentReferenceID`). A component's `Width` is its column's
//! width; the columns are laid side by side in that order within each block.
//!
//! A component holds one `sgdatum` (0x8024) per row: `SGPropertyType` says which row (1
//! Formula, 2 MW, 4 Limiting?, 5 the hidden "Limit Moles", 6 Equivalents, 7 Sample Mass,
//! 8 %Weight, 9 Volume, 10 Molarity, 12 Density, 13/14 Reactant Moles/Mass, 15/16 Expected
//! Mass/Moles, 17 Measured Mass, 18 Purity, 19/20 Product Mass/Moles, 21 %Yield, 22
//! %Completion), `SGDataType` how the value is kept (4 a string, 3 a number, 1 a yes/no), and
//! `SGDataValue` the value — a double in grams, moles, millilitres, mol/L and g/mL, with
//! percentages as fractions (100 % is 1). A value the user typed is `IsEdited`; one the table
//! computes is `IsReadOnly`; an empty cell is `Visible="no"`. The datum's id is
//! `(SGPropertyType << 16) | (component id << 8)`, the component ids counting from 1 — ids local
//! to the grid. Each datum carries the text ChemDraw shows for it, in an `objecttag` (named by
//! the datum's id) holding a `t`: 10 pt Arial in the formula face, bold for the formula, bold
//! italic for an entered value, the unit a 7.5 pt run after the number. The block titles,
//! frames and rules are not in the file: ChemDraw draws them.
//!
//! Reading takes the entered quantities (and each column's fragment and shown formula);
//! everything else is recomputed. Writing builds the whole grid the way ChemDraw does.

use std::collections::HashMap;

use super::cdx::{self, OBJ_TEXT, PROP_2D_POSITION, PROP_BOUNDING_BOX, PROP_TEXT, PROP_UTF8_TEXT, PT, SCALE};
use super::cdx_codec::CdxObject;
use super::page::{ReactionTable, TableEntry};
use super::reaction_table::TableState;
use super::Molecule;
use crate::reaction::stoich::{format_moles, format_number, format_percent, AnalysisResult, Formatted, Quantity};
use crate::reaction::table::Cell;
use crate::widget::scene::{
    TABLE_BASELINE, TABLE_BLOCK_GAP, TABLE_CELL_PAD, TABLE_FRAME_TOP, TABLE_MIN_COLUMN, TABLE_ROW_HEIGHT,
    TABLE_UNIT_SCALE,
};

pub(crate) const OBJ_STOICHIOMETRY_GRID: u16 = 0x8022;
pub(crate) const OBJ_SG_COMPONENT: u16 = 0x8023;
pub(crate) const OBJ_SG_DATUM: u16 = 0x8024;
const OBJ_OBJECT_TAG: u16 = 0x8011;

const PROP_NAME: u16 = 0x0008;
const PROP_IGNORE_WARNINGS: u16 = 0x000F;
const PROP_VISIBLE: u16 = 0x0011;
const PROP_LINE_HEIGHT: u16 = 0x0702;
const PROP_WIDTH: u16 = 0x0812;
const PROP_TAG_TYPE: u16 = 0x0D00;
const PROP_SG_DATA_TYPE: u16 = 0x1200;
const PROP_SG_PROPERTY_TYPE: u16 = 0x1201;
const PROP_SG_DATA_VALUE: u16 = 0x1202;
const PROP_COMPONENT_IS_REACTANT: u16 = 0x1203;
const PROP_COMPONENT_IS_HEADER: u16 = 0x1204;
const PROP_IS_HIDDEN: u16 = 0x1205;
const PROP_IS_READ_ONLY: u16 = 0x1206;
const PROP_IS_EDITED: u16 = 0x1207;
const PROP_COMPONENT_REFERENCE_ID: u16 = 0x1208;

const TAG_TYPE_STRING: i16 = 3;
const DATA_BOOL: i16 = 1;
const DATA_NUMBER: i16 = 3;
const DATA_STRING: i16 = 4;

const FONT: u16 = 3;
const SIZE: u16 = 200; // 10 pt, in twentieths
const FACE_PLAIN: u16 = 0x60; // ChemDraw's formula face, as it writes every cell
const FACE_BOLD: u16 = 0x01;
const FACE_ITALIC: u16 = 0x02;
/// From a cell's baseline up to its text box's top, and down to its bottom (points).
const ASCENT: f64 = 8.95;
const DESCENT: f64 = 2.55;
/// Rows a block shows.
const ROWS: f64 = 11.0;

/// The rows of each block as ChemDraw keeps them, in its order: `SGPropertyType` and label.
/// "Limit Moles" (5) is in the file but never shown.
const REACTANT_PROPS: [(i16, &str); 12] = [
    (1, "Formula"),
    (2, "MW"),
    (5, "Limit Moles"),
    (4, "Limiting?"),
    (6, "Equivalents"),
    (7, "Sample Mass"),
    (8, "%Weight"),
    (10, "Molarity"),
    (12, "Density"),
    (9, "Volume"),
    (13, "Reactant Moles"),
    (14, "Reactant Mass"),
];
const PRODUCT_PROPS: [(i16, &str); 12] = [
    (1, "Formula"),
    (2, "MW"),
    (5, "Limit Moles"),
    (6, "Equivalents"),
    (22, "%Completion"),
    (15, "Expected Mass"),
    (16, "Expected Moles"),
    (17, "Measured Mass"),
    (18, "Purity"),
    (19, "Product Mass"),
    (20, "Product Moles"),
    (21, "%Yield"),
];
const LIMIT_MOLES: i16 = 5;

/// The properties a user enters (no `IsReadOnly`).
fn is_entry(prop: i16) -> bool {
    matches!(prop, 6 | 7 | 8 | 9 | 10 | 12 | 17 | 18 | 22)
}

fn is_percent(prop: i16) -> bool {
    matches!(prop, 8 | 18 | 21 | 22)
}

/// A yes/no property: absent is no; present and empty (CDX) or non-zero is yes.
fn flag(o: &CdxObject, tag: u16) -> bool {
    o.prop(tag).is_some_and(|v| v.first().is_none_or(|&b| b != 0))
}

fn number(d: &CdxObject) -> Option<f64> {
    let v = d.prop(PROP_SG_DATA_VALUE)?;
    let bytes: [u8; 8] = v.try_into().ok()?;
    Some(f64::from_le_bytes(bytes)).filter(|x| x.is_finite())
}

fn point_2d(obj: &CdxObject) -> Option<(f64, f64)> {
    let v = obj.prop(PROP_2D_POSITION).filter(|v| v.len() >= 8)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some((n(4), n(0)))
}

/// The text a datum shows (in its object tag), as plain characters.
fn shown_text(d: &CdxObject) -> Option<String> {
    let tag = d.children.iter().find(|c| c.tag == OBJ_OBJECT_TAG)?;
    let t = tag.children.iter().find(|c| c.tag == OBJ_TEXT)?;
    let (_, text) = cdx::parse_cdx_string(t.prop(PROP_TEXT)?)?;
    Some(text)
}

fn reference_value(fragment: u32) -> Vec<u8> {
    // ChemDraw writes the reference in two bytes; an id past that range needs four.
    match i16::try_from(fragment) {
        Ok(f) => f.to_le_bytes().to_vec(),
        Err(_) => fragment.to_le_bytes().to_vec(),
    }
}

// ─── Reading ─────────────────────────────────────────────────────────────────

/// A ChemDraw stoichiometry grid as a reaction table: its corner, and per column what the
/// user entered, the fragment it describes and the formula it showed.
pub(crate) fn read_grid(grid: &CdxObject) -> Option<ReactionTable> {
    let at = point_2d(grid)?;
    let mut table = ReactionTable {
        id: 0,
        pos: [(at.0 / SCALE) as f32, (at.1 / SCALE) as f32],
        arrow: None,
        reactants: Vec::new(),
        products: Vec::new(),
        source: Some(grid.id),
    };
    for component in grid.children.iter().filter(|c| c.tag == OBJ_SG_COMPONENT) {
        if flag(component, PROP_COMPONENT_IS_HEADER) {
            continue;
        }
        let mut entry = TableEntry {
            fragment: component.prop_int(PROP_COMPONENT_REFERENCE_ID).map(|v| v as u32),
            ..TableEntry::default()
        };
        for d in component.children.iter().filter(|c| c.tag == OBJ_SG_DATUM) {
            let Some(prop) = d.prop_i16(PROP_SG_PROPERTY_TYPE) else { continue };
            if prop == 1 {
                entry.formula = shown_text(d).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                continue;
            }
            if !flag(d, PROP_IS_EDITED) {
                continue;
            }
            let Some(v) = number(d) else { continue };
            let v = if is_percent(prop) { v * 100.0 } else { v };
            match prop {
                4 => entry.limiting = v != 0.0,
                6 => entry.equivalents = Some(v),
                7 => entry.sample_mass = Some(v),
                8 => entry.percent_weight = Some(v),
                9 => entry.volume = Some(v),
                10 => entry.molarity = Some(v),
                12 => entry.density = Some(v),
                17 => entry.measured_mass = Some(v),
                18 => entry.purity = Some(v),
                22 => entry.percent_completion = Some(v),
                _ => {}
            }
        }
        if flag(component, PROP_COMPONENT_IS_REACTANT) {
            table.reactants.push(entry);
        } else {
            table.products.push(entry);
        }
    }
    Some(table)
}

/// The largest id a document uses for objects of its own: the component and datum ids inside
/// stoichiometry grids are local to their grid (ChemDraw numbers them from 1 in every grid,
/// and builds the datum ids from them), so they are left out, or new ids would jump past a
/// million for no reason.
pub(crate) fn document_max_id(obj: &CdxObject) -> u32 {
    let own = if matches!(obj.tag, OBJ_SG_COMPONENT | OBJ_SG_DATUM) { 0 } else { obj.id };
    obj.children.iter().map(document_max_id).fold(own, u32::max)
}

/// The box a grid covers on the page (CDX units): its corner to the bottom of its frames and
/// the right edge of its last column. A grid has no bounding box of its own.
pub(crate) fn grid_extent(grid: &CdxObject) -> Option<[f64; 4]> {
    if grid.tag != OBJ_STOICHIOMETRY_GRID {
        return None;
    }
    let (x, y) = point_2d(grid)?;
    let components: Vec<&CdxObject> = grid.children.iter().filter(|c| c.tag == OBJ_SG_COMPONENT).collect();
    let block = |reactant: bool| -> f64 {
        components
            .iter()
            .filter(|c| flag(c, PROP_COMPONENT_IS_REACTANT) == reactant)
            .map(|c| c.prop_int(PROP_WIDTH).unwrap_or(0) as f64)
            .sum()
    };
    let right = x + block(true) + TABLE_BLOCK_GAP as f64 * PT + block(false);
    Some([x, y, right, y + (TABLE_FRAME_TOP as f64 + ROWS * TABLE_ROW_HEIGHT as f64) * PT])
}

// ─── CDXML ───────────────────────────────────────────────────────────────────

/// CDXML spells a component's `Width` its own way: ChemDraw writes it as an integer in CDX
/// units (`Width="5534515"`, the 84.45 pt label column of its `ReactionAnalysis` document),
/// where every other coordinate, a page's `Width` among them, is written in points. The CDXML
/// codec knows `Width` only as a coordinate, so a component's width is turned from points
/// into CDX units after writing, and back before reading. ChemDraw reads a width in points as
/// CDX units — nearly nothing — and stacks every column on the first.
const COMPONENT_ELEMENT: &str = "sgcomponent";
const WIDTH_ATTRIBUTE: &str = "Width";

/// `xml` written by the codec with each component's width in CDX units, as ChemDraw writes it.
pub(crate) fn widths_to_cdxml(xml: String) -> String {
    rewrite_component_widths(&xml, |v| {
        let pt: f64 = v.trim().parse().ok()?;
        Some(((pt * PT).round() as i64).to_string())
    })
    .unwrap_or(xml)
}

/// A component's `Width` whose size is below this may be in points, from it up it is in CDX
/// units. ChemDraw writes CDX units; earlier versions of this crate wrote points, as the codec
/// writes every coordinate (trailing zeros trimmed: the minimum column went out as
/// `Width="40"`), and a coordinate is an `i32` of CDX units, so those stay below 32768 pt. A
/// width from here up read as points would run past the `i32` and be cut to its end, so it is
/// read as CDX units, the one way it holds. Below, the file does not say which it is: a width
/// is taken for points, as every earlier file's is, and a CDX width of 1 to 32767 units (under
/// half a point) is read as that many points. Zero, a hidden column, is zero either way.
const POINTS_BELOW: f64 = 32768.0;

/// `xml` with each component's width in points, for the codec to read: a width in CDX units
/// is turned into points, one in points already ([`POINTS_BELOW`]) stays as it is.
pub(crate) fn widths_from_cdxml(xml: &str) -> std::borrow::Cow<'_, str> {
    match rewrite_component_widths(xml, |v| {
        let units: f64 = v.trim().parse().ok()?;
        (units.is_finite() && units.abs() >= POINTS_BELOW).then(|| format!("{}", units / PT))
    }) {
        Some(s) => std::borrow::Cow::Owned(s),
        None => std::borrow::Cow::Borrowed(xml),
    }
}

/// `xml` with the `Width` of every `sgcomponent` start tag replaced by what `f` makes of its
/// value as XML reads it ([`attribute_value`]), left as it is where `f` gives nothing; `None`
/// when nothing changed. Tags are found as XML has them: not inside comments, CDATA sections,
/// processing instructions or declarations, and their attributes with whitespace around `=`
/// and either quote.
fn rewrite_component_widths(xml: &str, f: impl Fn(&str) -> Option<String>) -> Option<String> {
    let mut out = String::with_capacity(xml.len() + 64);
    let mut copied = 0;
    let mut i = 0;
    while let Some(n) = xml[i..].find('<') {
        let at = i + n;
        let markup = &xml[at..];
        if let Some(end) = non_element_end(markup) {
            i = at + end;
            continue;
        }
        let name_end = markup[1..].find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>').map_or(markup.len(), |n| 1 + n);
        let is_component = &markup[1..name_end] == COMPONENT_ELEMENT;
        let end = start_tag_attributes(markup, name_end, |name, value| {
            if !is_component || name != WIDTH_ATTRIBUTE {
                return;
            }
            if let Some(v) = attribute_value(&markup[value.clone()]).and_then(|v| f(&v)) {
                out.push_str(&xml[copied..at + value.start]);
                out.push_str(&v);
                copied = at + value.end;
            }
        });
        i = at + end;
    }
    if copied == 0 {
        return None;
    }
    out.push_str(&xml[copied..]);
    Some(out)
}

/// Where `markup` (from its `<`) ends, past its `>`, when it holds no element: a comment, a
/// CDATA section, a processing instruction or a declaration (`<!DOCTYPE …>`, its internal
/// subset in brackets among it). `None` for a tag.
fn non_element_end(markup: &str) -> Option<usize> {
    let to = |open: &str, close: &str| markup.strip_prefix(open).map(|r| r.find(close).map_or(markup.len(), |n| open.len() + n + close.len()));
    if let Some(end) = to("<!--", "-->").or_else(|| to("<![CDATA[", "]]>")).or_else(|| to("<?", "?>")) {
        return Some(end);
    }
    if !markup.starts_with("<!") {
        return None;
    }
    // A declaration: its `>` outside quotes, brackets and the comments of an internal subset.
    let mut depth = 0usize;
    let mut i = 2;
    while i < markup.len() {
        let rest = &markup[i..];
        if rest.starts_with("<!--") {
            i += rest.find("-->").map_or(rest.len(), |n| n + 3);
            continue;
        }
        match rest.as_bytes()[0] {
            q @ (b'"' | b'\'') => i +=1 + rest[1..].find(q as char).map_or(rest.len() - 1, |n| n + 1),
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'>' if depth == 0 => return Some(i + 1),
            _ => i += rest.chars().next().map_or(1, char::len_utf8),
        }
    }
    Some(markup.len())
}

/// Each attribute of the tag `markup` (from its `<`, its name ending at `from`) given to
/// `visit` as its name and the byte range of its value, as XML spells an attribute: `name`,
/// optional whitespace, `=`, optional whitespace, a value in `"` or `'`. Returns where the tag
/// ends, past its `>`; a quoted `>` does not end it. What is not an attribute is passed over.
fn start_tag_attributes(markup: &str, from: usize, mut visit: impl FnMut(&str, std::ops::Range<usize>)) -> usize {
    let skip_space = |i: usize| markup[i..].find(|c: char| !c.is_ascii_whitespace()).map_or(markup.len(), |n| i + n);
    // Where the value quoted at `i` closes.
    let quoted = |i: usize| -> Option<usize> {
        let q = markup[i..].chars().next().filter(|q| *q == '"' || *q == '\'')?;
        Some(markup[i + 1..].find(q).map_or(markup.len(), |n| i + 1 + n))
    };
    let mut i = skip_space(from);
    while i < markup.len() {
        if markup[i..].starts_with('>') {
            return i + 1;
        }
        if let Some(close) = quoted(i) {
            i = skip_space((close + 1).min(markup.len()));
            continue;
        }
        let name_end = markup[i..].find(|c: char| c.is_ascii_whitespace() || matches!(c, '=' | '>' | '/' | '"' | '\'')).map_or(markup.len(), |n| i + n);
        if name_end == i {
            // A `/` or a stray `=`.
            i = skip_space(i + 1);
            continue;
        }
        let name = &markup[i..name_end];
        let eq = skip_space(name_end);
        i = eq;
        if !markup[eq..].starts_with('=') {
            continue;
        }
        let value_at = skip_space(eq + 1);
        i = value_at;
        let Some(close) = quoted(value_at) else { continue };
        visit(name, value_at + 1..close);
        i = skip_space((close + 1).min(markup.len()));
    }
    markup.len()
}

/// The attribute value `raw` as XML reads it, as the codec does: its character references
/// (`&#53;`, `&#x35;`) and predefined entities (`&lt;`, `&gt;`, `&amp;`, `&apos;`, `&quot;`)
/// replaced by what they stand for. `None` where a reference is not one of these or names no
/// character.
fn attribute_value(raw: &str) -> Option<std::borrow::Cow<'_, str>> {
    if !raw.contains('&') {
        return Some(std::borrow::Cow::Borrowed(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let end = at + rest[at..].find(';')?;
        out.push(match &rest[at + 1..end] {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "apos" => '\'',
            "quot" => '"',
            name => {
                let number = name.strip_prefix('#')?;
                let (digits, radix) = number.strip_prefix('x').map_or((number, 10), |hex| (hex, 16));
                if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                    return None;
                }
                char::from_u32(u32::from_str_radix(digits, radix).ok()?)?
            }
        });
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Some(std::borrow::Cow::Owned(out))
}

// ─── Writing ─────────────────────────────────────────────────────────────────

/// Width of `text` in 10 pt Arial (CDX units) times `scale`, a little wider when bold.
fn text_width(text: &str, bold: bool, scale: f64) -> f64 {
    let w: f64 = text
        .chars()
        .map(|c| match c {
            '.' | ' ' | ',' => 278.0 * 10.0 * PT / 1000.0,
            '%' => 889.0 * 10.0 * PT / 1000.0,
            c => cdx::arial_width(c),
        })
        .sum();
    w * scale * if bold { 1.08 } else { 1.0 }
}

/// A CDXString with runs of their own size: (start, face, size in twentieths of a point); its
/// characters in Latin-1, or (`utf8`) in UTF-8.
fn styled_string(runs: &[(u16, u16, u16)], text: &str, utf8: bool) -> Vec<u8> {
    let mut out = (runs.len() as u16).to_le_bytes().to_vec();
    for &(start, face, size) in runs {
        for v in [start, FONT, face, size, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    if utf8 {
        out.extend_from_slice(text.as_bytes());
    } else {
        out.extend(text.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }));
    }
    out
}

/// What one datum shows: its characters, style runs and width (CDX units).
struct Shown {
    text: String,
    runs: Vec<(u16, u16, u16)>,
    width: f64,
}

fn shown_label(label: &str) -> Shown {
    Shown { text: label.to_string(), runs: vec![(0, FACE_PLAIN, SIZE)], width: text_width(label, false, 1.0) }
}

fn shown_formula(formula: &str) -> Shown {
    Shown { text: formula.to_string(), runs: vec![(0, FACE_PLAIN | FACE_BOLD, SIZE)], width: text_width(formula, true, 1.0) }
}

fn shown_cell(cell: &Cell) -> Shown {
    let number: String = cell.text.spans.iter().map(|s| s.text.as_str()).collect();
    let entered = cell.is_entered();
    let face = if entered { FACE_PLAIN | FACE_BOLD | FACE_ITALIC } else { FACE_PLAIN };
    let mut runs = vec![(0, face, SIZE)];
    let mut width = text_width(&number, entered, 1.0);
    let mut text = number;
    if !cell.unit.is_empty() {
        runs.push((text.chars().count() as u16, FACE_PLAIN, (SIZE as f32 * TABLE_UNIT_SCALE).round() as u16));
        width += text_width(&cell.unit, false, TABLE_UNIT_SCALE as f64);
        text.push_str(&cell.unit);
    }
    Shown { text, runs, width }
}

fn shown_value(f: Formatted) -> Shown {
    shown_cell(&Cell { text: crate::RichText::plain(f.number), unit: f.unit.to_string() })
}

/// One datum: its property, how its value is kept, its flags and what it shows.
struct Datum {
    prop: i16,
    data_type: i16,
    value: Vec<u8>,
    read_only: bool,
    hidden: bool,
    edited: bool,
    visible: bool,
    shown: Shown,
}

/// `d` as the `sgdatum` of component `component`, its text's baseline starting at `at` (CDX
/// units; `None` puts it at the origin, where ChemDraw keeps the hidden "Limit Moles").
fn datum_object(d: &Datum, component: u32, at: Option<(f64, f64)>, next: &mut u32) -> CdxObject {
    let id = ((d.prop as u32) << 16) | (component << 8);
    let mut o = CdxObject::new(OBJ_SG_DATUM, id);
    o.set(PROP_SG_DATA_TYPE, d.data_type.to_le_bytes().to_vec());
    o.set(PROP_SG_DATA_VALUE, d.value.clone());
    o.set(PROP_SG_PROPERTY_TYPE, d.prop.to_le_bytes().to_vec());
    if d.read_only {
        o.set(PROP_IS_READ_ONLY, Vec::new());
    }
    if d.hidden {
        o.set(PROP_IS_HIDDEN, Vec::new());
        o.set(PROP_VISIBLE, vec![0]);
    }
    if d.edited {
        o.set(PROP_IS_EDITED, Vec::new());
    }
    *next += 1;
    let mut tag = CdxObject::new(OBJ_OBJECT_TAG, *next);
    tag.set(PROP_TAG_TYPE, TAG_TYPE_STRING.to_le_bytes().to_vec());
    if !d.visible {
        tag.set(PROP_VISIBLE, vec![0]);
    }
    tag.set(PROP_NAME, id.to_string().into_bytes());
    let mut t = CdxObject::new(OBJ_TEXT, 0);
    let (x, y) = at.unwrap_or((0.0, 0.0));
    t.set(PROP_2D_POSITION, cdx::point(x, y));
    t.set(PROP_BOUNDING_BOX, cdx::rect([x, y - ASCENT * PT, x + d.shown.width, y + DESCENT * PT]));
    if !d.visible {
        t.set(PROP_VISIBLE, vec![0]);
        t.set(PROP_IGNORE_WARNINGS, Vec::new());
    }
    t.set(PROP_LINE_HEIGHT, 1u16.to_le_bytes().to_vec());
    t.set(PROP_TEXT, styled_string(&d.shown.runs, &d.shown.text, false));
    if d.shown.text.chars().any(|c| c as u32 >= 0x100) {
        t.set(PROP_UTF8_TEXT, styled_string(&d.shown.runs, &d.shown.text, true));
    }
    tag.children.push(t);
    o.children.push(tag);
    o
}

/// A header component's data: the row labels.
fn header_data(props: &[(i16, &str)]) -> Vec<Datum> {
    props
        .iter()
        .map(|&(prop, label)| Datum {
            prop,
            data_type: DATA_STRING,
            value: label.as_bytes().to_vec(),
            read_only: true,
            hidden: prop == LIMIT_MOLES,
            edited: false,
            visible: prop != LIMIT_MOLES,
            shown: shown_label(label),
        })
        .collect()
}

/// A value column's data: the cells the table shows (`cells`, in the block's row order), the
/// numbers behind them (`values`, by property, percentages in percent) and the formula.
fn column_data(
    props: &[(i16, &str)],
    formula: &str,
    cells: &[Cell],
    values: &HashMap<i16, f64>,
    limit_moles: Option<f64>,
) -> Vec<Datum> {
    let mut row = 0;
    props
        .iter()
        .map(|&(prop, _)| {
            if prop == LIMIT_MOLES {
                let v = limit_moles.unwrap_or(0.0);
                return Datum {
                    prop,
                    data_type: DATA_NUMBER,
                    value: v.to_le_bytes().to_vec(),
                    read_only: true,
                    hidden: true,
                    edited: false,
                    visible: false,
                    shown: shown_value(format_moles(v)),
                };
            }
            let cell = &cells[row];
            row += 1;
            if prop == 1 {
                return Datum {
                    prop,
                    data_type: DATA_STRING,
                    value: Vec::new(),
                    read_only: true,
                    hidden: false,
                    edited: false,
                    visible: !formula.is_empty(),
                    shown: shown_formula(formula),
                };
            }
            let limiting = prop == 4;
            // What ChemDraw keeps in a cell left empty: 100 %, one equivalent, else nothing.
            let default = if is_percent(prop) || prop == 6 { 1.0 } else { 0.0 };
            let stored = match values.get(&prop).copied() {
                Some(v) if is_percent(prop) => v / 100.0,
                Some(v) => v,
                None => default,
            };
            let shown = if !cell.is_empty() {
                shown_cell(cell)
            } else if is_percent(prop) {
                shown_value(format_percent(stored * 100.0))
            } else {
                shown_value(format_number(stored))
            };
            Datum {
                prop,
                data_type: if limiting { DATA_BOOL } else { DATA_NUMBER },
                value: stored.to_le_bytes().to_vec(),
                read_only: limiting || !is_entry(prop),
                hidden: false,
                edited: cell.is_entered(),
                visible: !cell.is_empty(),
                shown,
            }
        })
        .collect()
}

fn q(v: Option<Quantity>) -> Option<f64> {
    v.map(|q| q.value)
}

/// The numbers behind each column, by `SGPropertyType`, in ChemDraw's units (moles in mol,
/// volume in mL; percentages still in percent).
type Values = Vec<HashMap<i16, f64>>;
fn column_values(result: &AnalysisResult) -> (Values, Values) {
    let collect = |pairs: Vec<(i16, Option<f64>)>| pairs.into_iter().filter_map(|(k, v)| Some((k, v?))).collect();
    let reactants = result
        .reactants
        .iter()
        .map(|r| {
            collect(vec![
                (2, r.mw),
                (4, Some(if r.limiting { 1.0 } else { 0.0 })),
                (6, q(r.equivalents)),
                (7, q(r.sample_mass)),
                (8, q(r.percent_weight)),
                (10, q(r.molarity)),
                (12, q(r.density)),
                (9, q(r.volume)),
                (13, q(r.moles)),
                (14, q(r.mass)),
            ])
        })
        .collect();
    let products = result
        .products
        .iter()
        .map(|p| {
            collect(vec![
                (2, p.mw),
                (6, q(p.equivalents)),
                (22, q(p.percent_completion)),
                (15, q(p.expected_mass)),
                (16, q(p.expected_moles)),
                (17, q(p.measured_mass)),
                (18, q(p.purity)),
                (19, q(p.mass)),
                (20, q(p.moles)),
                (21, q(p.percent_yield)),
            ])
        })
        .collect();
    (reactants, products)
}

/// The stoichiometry grid for `table` as ChemDraw writes one, under id `id` with its corner at
/// `at` (CDX units). `fragment_of` gives the fragment each atom was written in, for the columns'
/// `ComponentReferenceID`; new object tags take ids from `next`.
pub(crate) fn grid_object(
    id: u32,
    table: &ReactionTable,
    mol: &Molecule,
    at: (f64, f64),
    fragment_of: &HashMap<u32, u32>,
    next: &mut u32,
) -> CdxObject {
    let state = TableState::of(mol, table);
    let result = state.analysis.compute();
    let [reactant_block, product_block] = state.blocks();
    let (reactant_values, product_values) = column_values(&result);
    let (reactant_formulas, product_formulas) = state.formulas();
    let limit_moles = result.limiting_moles;

    // Every component's data, in ChemDraw's order: the two headers, then the columns.
    struct Component {
        reactant: bool,
        header: bool,
        fragment: Option<u32>,
        data: Vec<Datum>,
    }
    let mut components = vec![
        Component { reactant: true, header: true, fragment: None, data: header_data(&REACTANT_PROPS) },
        Component { reactant: false, header: true, fragment: None, data: header_data(&PRODUCT_PROPS) },
    ];
    let fragment = |atom: Option<u32>| atom.and_then(|a| fragment_of.get(&a).copied());
    for (k, cells) in reactant_block.columns.iter().enumerate() {
        components.push(Component {
            reactant: true,
            header: false,
            fragment: fragment(state.reactants[k].atom),
            data: column_data(&REACTANT_PROPS, &reactant_formulas[k], cells, &reactant_values[k], limit_moles),
        });
    }
    for (k, cells) in product_block.columns.iter().enumerate() {
        components.push(Component {
            reactant: false,
            header: false,
            fragment: fragment(state.products[k].atom),
            data: column_data(&PRODUCT_PROPS, &product_formulas[k], cells, &product_values[k], limit_moles),
        });
    }

    // Column widths and left edges: each block's header column, then its value columns.
    let pad = TABLE_CELL_PAD as f64 * PT;
    let widths: Vec<f64> = components
        .iter()
        .map(|c| {
            let widest = c.data.iter().filter(|d| d.visible).map(|d| d.shown.width).fold(0.0, f64::max);
            let w = widest + 2.0 * pad;
            if c.header { w } else { w.max(TABLE_MIN_COLUMN as f64 * PT) }
        })
        .collect();
    let mut left = vec![0.0; components.len()];
    let mut x = at.0;
    for reactant in [true, false] {
        for header in [true, false] {
            for (k, c) in components.iter().enumerate() {
                if c.reactant == reactant && c.header == header {
                    left[k] = x;
                    x += widths[k];
                }
            }
        }
        x += TABLE_BLOCK_GAP as f64 * PT;
    }

    let mut grid = CdxObject::new(OBJ_STOICHIOMETRY_GRID, id);
    grid.set(PROP_2D_POSITION, cdx::point(at.0, at.1));
    let frame_top = at.1 + TABLE_FRAME_TOP as f64 * PT;
    for (k, c) in components.iter().enumerate() {
        let component_id = k as u32 + 1;
        let mut o = CdxObject::new(OBJ_SG_COMPONENT, component_id);
        o.set(PROP_WIDTH, (widths[k].round() as i32).to_le_bytes().to_vec());
        if let Some(f) = c.fragment {
            o.set(PROP_COMPONENT_REFERENCE_ID, reference_value(f));
        }
        if c.reactant {
            o.set(PROP_COMPONENT_IS_REACTANT, Vec::new());
        }
        if c.header {
            o.set(PROP_COMPONENT_IS_HEADER, Vec::new());
        }
        let mut row = 0;
        for d in &c.data {
            let place = if d.prop == LIMIT_MOLES {
                None
            } else {
                let baseline = frame_top + (row as f64 * TABLE_ROW_HEIGHT as f64 + TABLE_BASELINE as f64) * PT;
                row += 1;
                Some((left[k] + pad, baseline))
            };
            o.children.push(datum_object(d, component_id, place, next));
        }
        grid.children.push(o);
    }
    grid
}

/// Move a grid (its corner and the text of every datum) by `delta` (CDX units). The hidden
/// "Limit Moles" texts ChemDraw keeps at the origin stay there.
pub(crate) fn translate_grid(grid: &mut CdxObject, delta: (f64, f64)) {
    fn walk(o: &mut CdxObject, delta: (f64, f64)) {
        if let Some((x, y)) = point_2d(o)
            && (o.tag != OBJ_TEXT || (x, y) != (0.0, 0.0))
        {
            o.set(PROP_2D_POSITION, cdx::point(x + delta.0, y + delta.1));
            if let Some(b) = cdx::bounding_box(o) {
                o.set(PROP_BOUNDING_BOX, cdx::rect([b[0] + delta.0, b[1] + delta.1, b[2] + delta.0, b[3] + delta.1]));
            }
        }
        for c in &mut o.children {
            walk(c, delta);
        }
    }
    walk(grid, delta);
}

/// Point each column of a grid kept as it was at the fragment its structure is written in now:
/// the entry read from that column (`fragment`) knows the atom it was read into, and
/// `fragment_of` that atom's new fragment. Nothing else changes.
pub(crate) fn relink_grid(grid: &mut CdxObject, entries: &[&TableEntry], fragment_of: &HashMap<u32, u32>) {
    for c in grid.children.iter_mut().filter(|c| c.tag == OBJ_SG_COMPONENT) {
        let Some(old) = c.prop_int(PROP_COMPONENT_REFERENCE_ID).map(|v| v as u32) else { continue };
        let new = entries.iter().find(|e| e.fragment == Some(old)).and_then(|e| e.atom).and_then(|a| fragment_of.get(&a));
        if let Some(&new) = new
            && new != old
        {
            c.set(PROP_COMPONENT_REFERENCE_ID, reference_value(new));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::cdx::{cdxml_of_document, merge_into_cdx, molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml, CdxImport};
    use crate::molecule::cdx_codec;
    use crate::molecule::reaction_table::{edit_cell, new_table};
    use crate::molecule::{ArrowKind, BondOrder, PageObject};
    use crate::reaction::edit::CellRef;
    use crate::reaction::BlockKind;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
    }

    /// ChemDraw 25's `ReactionAnalysis` document: a sulfoxide drawn twice with a reaction arrow
    /// between them (arrow 85 superseding graphic 81, one reaction step), and its stoichiometry
    /// grid 82 at (50.36, 99.21) pt with the reactant marked limiting and nothing else entered.
    const GRID: u32 = 82;

    fn analysis_cdx() -> Vec<u8> {
        fixture("chemdraw25_reaction_analysis.cdx")
    }

    fn tables(m: &Molecule) -> Vec<&ReactionTable> {
        m.objects.iter().filter_map(PageObject::as_table).collect()
    }

    fn find(doc: &CdxObject, id: u32, tag: u16) -> Option<CdxObject> {
        fn walk(o: &CdxObject, id: u32, tag: u16) -> Option<CdxObject> {
            if o.id == id && o.tag == tag {
                return Some(o.clone());
            }
            o.children.iter().find_map(|c| walk(c, id, tag))
        }
        walk(doc, id, tag)
    }

    fn assert_analysis(import: &CdxImport, what: &str) {
        let m = &import.molecule;
        let t = tables(m);
        assert_eq!(t.len(), 1, "{what}");
        let t = t[0];
        assert_eq!(t.source, Some(GRID), "{what}");
        let pt = |v: f32| v * 14.4 / 1.5;
        assert!((pt(t.pos[0]) - 50.36).abs() < 0.02 && (pt(t.pos[1]) - 99.21).abs() < 0.02, "{what}: {t:?}");
        assert_eq!((t.reactants.len(), t.products.len()), (1, 1), "{what}");
        assert!(t.reactants[0].limiting, "{what}: Limiting? = Yes was entered");
        assert_eq!(t.reactants[0].equivalents, None, "{what}: not entered");
        assert_eq!(t.reactants[0].formula.as_deref(), Some("C13H12OS"), "{what}");
        // Tied to the drawing: each column to its structure, the table to the step's arrow.
        let arrow = m.objects.iter().find(|o| o.as_arrow().is_some()).unwrap().id();
        assert_eq!(t.arrow, Some(arrow), "{what}");
        let state = TableState::of(m, t);
        assert_eq!(state.columns.reactants.len(), 1);
        assert!(state.columns.reactants[0].contains(&t.reactants[0].atom.unwrap()), "{what}");
        assert!(state.columns.products[0].contains(&t.products[0].atom.unwrap()), "{what}");
        // We compute what ChemDraw printed.
        assert_eq!(state.formulas(), (vec!["C13H12OS".to_string()], vec!["C13H12OS".to_string()]), "{what}");
        let [r, _] = state.blocks();
        assert_eq!(r.columns[0][1].plain(), "216.30", "{what}");
        assert_eq!(r.columns[0][2].plain(), "Yes", "{what}");
        // The grid is no longer an unshown object: only the reaction scheme is.
        assert_eq!(import.other_objects, 1, "{what}");
    }

    #[test]
    fn chemdraws_reaction_table_reads_from_cdx_and_cdxml() {
        assert_analysis(&read_cdx(&analysis_cdx()).unwrap(), "CDX");
        let xml = String::from_utf8(fixture("chemdraw25_reaction_analysis.cdxml")).unwrap();
        assert_analysis(&read_cdxml(&xml).unwrap(), "CDXML");
    }

    /// The grid with its fragment references left out (they follow the renumbered structures).
    fn without_references(mut grid: CdxObject) -> CdxObject {
        for c in &mut grid.children {
            c.props.retain(|(t, _)| *t != PROP_COMPONENT_REFERENCE_ID);
        }
        grid
    }

    fn references(grid: &CdxObject) -> Vec<u32> {
        grid.children.iter().filter_map(|c| c.prop_int(PROP_COMPONENT_REFERENCE_ID)).map(|v| v as u32).collect()
    }

    fn fragment_ids(doc: &CdxObject) -> Vec<u32> {
        fn walk(o: &CdxObject, out: &mut Vec<u32>) {
            if o.tag == cdx::OBJ_FRAGMENT {
                out.push(o.id);
            }
            o.children.iter().for_each(|c| walk(c, out));
        }
        let mut out = Vec::new();
        walk(doc, &mut out);
        out
    }

    fn assert_analysis_saved(import: &CdxImport) {
        let t = tables(&import.molecule);
        assert_eq!(t.len(), 1);
        assert!(t[0].reactants[0].limiting);
        assert_eq!(import.other_objects, 1);
    }

    #[test]
    fn an_unedited_table_is_saved_as_chemdraw_wrote_it() {
        let original = analysis_cdx();
        let import = read_cdx(&original).unwrap();
        let saved = merge_into_cdx(&original, &import.molecule);
        let (before, after) = (cdx_codec::parse(&original).unwrap(), cdx_codec::parse(&saved).unwrap());
        let was = find(&before, GRID, OBJ_STOICHIOMETRY_GRID).unwrap();
        let now = find(&after, GRID, OBJ_STOICHIOMETRY_GRID).unwrap();
        assert_eq!(without_references(now.clone()), without_references(was), "byte for byte, but for the references");
        // The columns name the fragments the structures are written in now.
        let fragments = fragment_ids(&after);
        assert_eq!(references(&now).len(), 2);
        assert!(references(&now).iter().all(|r| fragments.contains(r)), "{:?} in {fragments:?}", references(&now));
        assert_analysis_saved(&read_cdx(&saved).unwrap());
        // New ids continue from the document's own, not from the grid's local datum ids.
        assert!(fragments.iter().all(|&f| f < 10_000), "{fragments:?}");
        // Saving again keeps it.
        let twice = merge_into_cdx(&saved, &read_cdx(&saved).unwrap().molecule);
        let again = find(&cdx_codec::parse(&twice).unwrap(), GRID, OBJ_STOICHIOMETRY_GRID).unwrap();
        assert_eq!(without_references(again), without_references(now));
    }

    #[test]
    fn a_moved_table_moves_its_grid_and_texts() {
        let original = analysis_cdx();
        let mut m = read_cdx(&original).unwrap().molecule;
        for o in &mut m.objects {
            if o.source() == Some(GRID) {
                o.translate([1.5, 3.0]);
            }
        }
        let saved = merge_into_cdx(&original, &m);
        let doc = cdx_codec::parse(&saved).unwrap();
        let grid = find(&doc, GRID, OBJ_STOICHIOMETRY_GRID).unwrap();
        let (x, y) = point_2d(&grid).unwrap();
        assert!(((x / PT) - (50.36 + 14.4)).abs() < 0.02 && ((y / PT) - (99.21 + 28.8)).abs() < 0.02);
        // The label "Formula" moved with it; the hidden "Limit Moles" stays at the origin.
        let formula = &grid.children[0].children[0].children[0].children[0];
        let (fx, fy) = point_2d(formula).unwrap();
        assert!(((fx / PT) - (57.86 + 14.4)).abs() < 0.02 && ((fy / PT) - (125.46 + 28.8)).abs() < 0.02);
        let hidden = &grid.children[0].children[2].children[0].children[0];
        assert_eq!(point_2d(hidden), Some((0.0, 0.0)));
        let again = read_cdx(&saved).unwrap();
        let t = tables(&again.molecule)[0];
        assert!(((t.pos[1] * 9.6) - (99.21 + 28.8)).abs() < 0.02);
    }

    #[test]
    fn an_edited_table_is_written_anew_with_its_values() {
        let original = analysis_cdx();
        let mut m = read_cdx(&original).unwrap().molecule;
        let id = tables(&m)[0].id;
        let PageObject::Table(t) = m.object_by_id(id).unwrap().clone() else { unreachable!() };
        let t = edit_cell(&m, &t, CellRef::new(BlockKind::Reactants, 0, 4), "216.3 mg").unwrap().unwrap();
        let t = edit_cell(&m, &t, CellRef::new(BlockKind::Products, 0, 6), "150 mg").unwrap().unwrap();
        let t = edit_cell(&m, &t, CellRef::new(BlockKind::Products, 0, 7), "95").unwrap().unwrap();
        if let Some(PageObject::Table(slot)) = m.object_by_id_mut(id) {
            *slot = t;
        }
        let saved = merge_into_cdx(&original, &m);
        let doc = cdx_codec::parse(&saved).unwrap();
        let grid = find(&doc, GRID, OBJ_STOICHIOMETRY_GRID).expect("the same id");
        assert_eq!(grid.children.len(), 4, "two headers and two columns");
        // ChemDraw's units: grams, and a fraction for percent.
        let datum = |component: usize, prop: i16| {
            grid.children[component].children.iter().find(|d| d.prop_i16(PROP_SG_PROPERTY_TYPE) == Some(prop)).unwrap().clone()
        };
        let sample = datum(2, 7);
        assert!((number(&sample).unwrap() - 0.2163).abs() < 1e-9 && flag(&sample, PROP_IS_EDITED));
        assert_eq!(shown_text(&sample).as_deref(), Some("216.30mg"));
        let purity = datum(3, 18);
        assert!((number(&purity).unwrap() - 0.95).abs() < 1e-9 && flag(&purity, PROP_IS_EDITED));
        let moles = datum(2, 13);
        assert!((number(&moles).unwrap() - 0.001).abs() < 1e-5 && flag(&moles, PROP_IS_READ_ONLY));
        let percent_yield = datum(3, 21);
        assert_eq!(shown_text(&percent_yield).as_deref(), Some("65.88%"));
        assert_eq!(datum(2, 7).id, (7 << 16) | (3 << 8), "ChemDraw's datum ids");
        // Read back: the same entries.
        let again = read_cdx(&saved).unwrap();
        let t = tables(&again.molecule)[0];
        assert!((t.reactants[0].sample_mass.unwrap() - 0.2163).abs() < 1e-9);
        assert!((t.products[0].measured_mass.unwrap() - 0.15).abs() < 1e-9);
        assert!((t.products[0].purity.unwrap() - 95.0).abs() < 1e-9);
        assert!(t.reactants[0].limiting);
        // Everything written is CDX and CDXML that ChemDraw's format knows.
        assert!(cdx_file_rs::file::CdxFile::from_bytes(&saved).is_ok());
        let xml = cdxml_of_document(&doc);
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        let from_xml = read_cdxml(&xml).unwrap();
        assert!((tables(&from_xml.molecule)[0].reactants[0].sample_mass.unwrap() - 0.2163).abs() < 1e-9);
    }

    /// A reaction drawn here, with a table inserted under it.
    fn drawn() -> Molecule {
        let mut m = Molecule::default();
        let chain = |m: &mut Molecule, at: [f32; 2], n: usize| {
            let ids: Vec<u32> = (0..n).map(|i| m.add_atom("C".into(), [at[0] + 1.3 * i as f32, at[1]], 0)).collect();
            for w in ids.windows(2) {
                m.add_bond(w[0], w[1], BondOrder::Single);
            }
        };
        chain(&mut m, [5.0, 5.0], 2);
        chain(&mut m, [8.0, 5.0], 3);
        m.add_arrow([12.0, 5.0], [16.0, 5.0], ArrowKind::Reaction);
        chain(&mut m, [17.0, 5.0], 5);
        let t = new_table(&m, None).unwrap();
        let t = edit_cell(&m, &t, CellRef::new(BlockKind::Reactants, 0, 4), "1.00 g").unwrap().unwrap();
        let t = edit_cell(&m, &t, CellRef::new(BlockKind::Reactants, 1, 3), "1.2").unwrap().unwrap();
        m.push_object(PageObject::Table(t));
        m
    }

    #[test]
    fn a_new_table_survives_cdx_and_cdxml() {
        let m = drawn();
        for (what, import) in [
            ("CDX", read_cdx(&molecule_to_cdx_document(&m)).unwrap()),
            ("CDXML", read_cdxml(&molecule_to_cdxml(&m)).unwrap()),
        ] {
            let t = tables(&import.molecule);
            assert_eq!(t.len(), 1, "{what}");
            let state = TableState::of(&import.molecule, t[0]);
            assert_eq!(state.reactants.len(), 2, "{what}");
            assert!(state.reactants[0].limiting && state.reactants[0].sample_mass == Some(1.0), "{what}: {t:?}");
            assert_eq!(state.reactants[1].equivalents, Some(1.2), "{what}");
            assert_eq!(state.products.len(), 1, "{what}");
            assert!(import.molecule.reaction_table_arrow(t[0]).is_some(), "{what}");
            assert_eq!(import.other_objects, 0, "{what}");
            let p = t[0].pos;
            let q = tables(&m)[0].pos;
            assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3, "{what}");
        }
        let xml = molecule_to_cdxml(&m);
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        assert!(xml.contains("<stoichiometrygrid") && xml.contains("ComponentIsHeader=\"yes\""), "{xml}");
        assert!(xml.contains("SGDataValue=\"Reactant Moles\""), "{xml}");
        assert!(cdx_file_rs::file::CdxFile::from_bytes(&molecule_to_cdx_document(&m)).is_ok());
        // The columns name the fragments they describe, and the page's box covers the table.
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        let fragments = fragment_ids(&doc);
        let grid = doc.children[0].children.iter().find(|o| o.tag == OBJ_STOICHIOMETRY_GRID).unwrap();
        assert_eq!(references(grid).len(), 3);
        assert!(references(grid).iter().all(|r| fragments.contains(r)));
        let doc_box = cdx::bounding_box(&doc).unwrap();
        assert!(doc_box[3] >= grid_extent(grid).unwrap()[3] - 1.0);
        // Merged into ChemDraw's document, too.
        let merged = merge_into_cdx(&analysis_cdx(), &m);
        let doc = cdx_codec::parse(&merged).unwrap();
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&cdxml_of_document(&doc)).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    }

    /// Each component's `Width`, in CDX units.
    fn widths(grid: &CdxObject) -> Vec<i32> {
        grid.children.iter().filter(|c| c.tag == OBJ_SG_COMPONENT).map(|c| c.prop_int(PROP_WIDTH).unwrap()).collect()
    }

    fn grid_of(doc: &CdxObject) -> CdxObject {
        fn walk(o: &CdxObject) -> Option<CdxObject> {
            if o.tag == OBJ_STOICHIOMETRY_GRID {
                return Some(o.clone());
            }
            o.children.iter().find_map(walk)
        }
        walk(doc).expect("a grid")
    }

    /// The `Width` attribute of every `sgcomponent` in a CDXML text, as written.
    fn cdxml_widths(xml: &str) -> Vec<String> {
        xml.split("<sgcomponent").skip(1).map(|s| s.split("Width=\"").nth(1).unwrap().split('"').next().unwrap().to_string()).collect()
    }

    #[test]
    fn only_a_components_width_is_rewritten_and_nothing_else_changes() {
        let xml = "<page Width=\"540\"><sgcomponentx Width=\"1\"/><sgcomponent id=\"1\" Name='a>b' Width=\"84.45\" = ComponentIsHeader=\"yes\"/></page>";
        let out = widths_to_cdxml(xml.to_string());
        assert_eq!(out, xml.replace("Width=\"84.45\"", &format!("Width=\"{}\"", (84.45 * PT).round())));
        assert_eq!(widths_from_cdxml(&out), xml.replace("84.45", &format!("{}", (84.45 * PT).round() / PT)));
        // A width in points (with a fraction) is read as it is.
        assert_eq!(widths_from_cdxml(xml), xml);
    }

    #[test]
    fn a_width_is_read_as_points_only_where_points_can_hold_it() {
        // An earlier version wrote points; ChemDraw writes CDX units. Points reach no further
        // than 32768 either way, an `i32` of CDX units; from there on a width is CDX units.
        for (w, pt) in [
            ("40", "40"),
            ("40.02", "40.02"),
            ("-40", "-40"),
            ("32767", "32767"),
            ("-32767", "-32767"),
            ("32768", "0.5"),
            ("-32768", "-0.5"),
            ("65535", "0.9999847412109375"),
            ("65536", "1"),
            ("-65536", "-1"),
            ("5534515", "84.44999694824219"),
        ] {
            let xml = format!("<sgcomponent Width=\"{w}\"/>");
            assert_eq!(widths_from_cdxml(&xml), format!("<sgcomponent Width=\"{pt}\"/>"), "{w}");
        }
    }

    #[test]
    fn a_width_of_a_few_cdx_units_is_taken_for_points() {
        // What stays ambiguous: a width under half a point in CDX units, 1 to 32767, reads as
        // that many points, as an earlier version's would. A hidden column, 0, is 0 either way.
        for (w, units) in [("1", 65536), ("40", 40 * 65536), ("32767", 32767 * 65536), ("0", 0), ("-0", 0)] {
            let grid = grid_of(&cdx::cdxml_document(&analysis_with_first_width(&format!("Width=\"{w}\""))).unwrap());
            assert_eq!(widths(&grid)[0], units, "{w}");
        }
        assert_eq!(widths_from_cdxml("<sgcomponent Width=\"0\"/>"), "<sgcomponent Width=\"0\"/>");
    }

    #[test]
    fn a_width_points_cannot_hold_is_never_saturated() {
        // Read as points, 32768 and up would run past an `i32` of CDX units and stop at its end.
        for w in [32768, 65535, -32768, -40000, -65536, i32::MAX, i32::MIN + 1] {
            let xml = analysis_with_first_width(&format!("Width=\"{w}\""));
            let grid = grid_of(&cdx::cdxml_document(&xml).unwrap());
            assert_eq!(widths(&grid)[0], w, "{w}");
            let again = cdxml_of_document(&cdx::cdxml_document(&xml).unwrap());
            assert_eq!(cdxml_widths(&again)[0], w.to_string(), "{w} written again");
        }
    }

    #[test]
    fn a_widths_character_references_are_read_as_xml_reads_them() {
        for (w, pt) in [
            ("&#53;534515", "84.44999694824219"),
            ("&#x35;534515", "84.44999694824219"),
            ("&#x35;&#53;3451&#x0035;", "84.44999694824219"),
            ("&#32;65536&#x9;", "1"),
            ("&#52;0", "&#52;0"),
            ("&amp;65536", "&amp;65536"),
            ("&unknown;65536", "&unknown;65536"),
        ] {
            let xml = format!("<sgcomponent Width='{w}'/>");
            assert_eq!(widths_from_cdxml(&xml), format!("<sgcomponent Width='{pt}'/>"), "{w}");
        }
        assert_eq!(attribute_value("&lt;&gt;&amp;&apos;&quot;&#60;&#x3E;").as_deref(), Some("<>&'\"<>"));
        for bad in ["&#;", "&#x;", "&#+53;", "&#x+35;", "&#xD800;", "&#1114112;", "&#53", "&nbsp;", "&#X35;"] {
            assert_eq!(attribute_value(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_components_width_written_with_character_references_imports_and_saves_as_chemdraws() {
        let chemdraw = [5534515, 5639372, 3968204, 3968204];
        for spelling in ["Width='&#53;534515'", "Width=\"&#x35;534515\"", "Width = '&#x35;&#53;&#51;4515'"] {
            let xml = analysis_with_first_width(spelling);
            assert_eq!(widths(&grid_of(&cdx::cdxml_document(&xml).unwrap())), chemdraw, "{spelling} read");
            // Opened and saved again, through the document the file was.
            let original = cdx_codec::write(&cdx::cdxml_document(&xml).unwrap());
            let read = read_cdxml(&xml).unwrap().molecule;
            let saved = cdxml_of_document(&cdx_codec::parse(&merge_into_cdx(&original, &read)).unwrap());
            assert_eq!(cdxml_widths(&saved), ["5534515", "5639372", "3968204", "3968204"], "{spelling} saved");
        }
        // An earlier version's points, spelled with references, stay points.
        let grid = grid_of(&cdx::cdxml_document(&analysis_with_first_width("Width='&#52;&#x30;'")).unwrap());
        assert_eq!(widths(&grid)[0], 40 * 65536);
    }

    #[test]
    fn widths_are_found_as_xml_spells_attributes_and_nowhere_else() {
        // Spaces around `=`, either quote, and markup that holds no element.
        let xml = "<?pi <sgcomponent Width=\"65536\"?><!-- <sgcomponent Width=\"65536\"/> --><!DOCTYPE CDXML [<!ENTITY e \"<sgcomponent Width='65536'>\">]>\
                   <page><![CDATA[<sgcomponent Width=\"65536\"/>]]><sgcomponent id = '1' Width = '131072'\n/><sgcomponent\tWidth\n=\t\"196608\">x</sgcomponent></page>";
        let out = widths_from_cdxml(xml);
        assert_eq!(out, xml.replace("'131072'", "'2'").replace("\"196608\"", "\"3\""));
        assert_eq!(widths_to_cdxml(out.into_owned()), xml);
    }

    #[test]
    fn chemdraws_cdxml_grid_widths_are_cdx_units() {
        // ChemDraw writes a component's Width in CDXML as an integer in CDX units (1/65536 pt),
        // not in points as other coordinates: `Width="5534515"` is the 84.45 pt label column.
        let xml = String::from_utf8(fixture("chemdraw25_reaction_analysis.cdxml")).unwrap();
        assert_eq!(cdxml_widths(&xml), ["5534515", "5639372", "3968204", "3968204"]);
        let grid = grid_of(&cdx::cdxml_document(&xml).unwrap());
        assert_eq!(widths(&grid), [5534515, 5639372, 3968204, 3968204]);
        // Read so, the grid covers what ChemDraw drew: from 50.36 pt to the right edge of the
        // product column (the label at 290.84 pt plus its 7.5 pt pad, plus 60.55 pt).
        let extent = grid_extent(&grid).unwrap();
        assert!((extent[2] / PT - 343.89).abs() < 0.2, "{}", extent[2] / PT);
    }

    #[test]
    fn grid_widths_go_to_cdxml_in_cdx_units_and_back() {
        let m = drawn();
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        let written = widths(&grid_of(&doc));
        assert!(written.iter().all(|&w| w > 20 * 65536), "every column is tens of points wide: {written:?}");
        let xml = molecule_to_cdxml(&m);
        let expected: Vec<String> = written.iter().map(|w| w.to_string()).collect();
        assert_eq!(cdxml_widths(&xml), expected, "as ChemDraw spells them");
        assert_eq!(widths(&grid_of(&cdx::cdxml_document(&xml).unwrap())), written, "and read back the same");
        // ChemDraw's own document, read and written again, keeps its widths.
        let chemdraw = String::from_utf8(fixture("chemdraw25_reaction_analysis.cdxml")).unwrap();
        let again = cdxml_of_document(&cdx::cdxml_document(&chemdraw).unwrap());
        assert_eq!(cdxml_widths(&again), ["5534515", "5639372", "3968204", "3968204"]);
    }

    /// [`drawn`] with hydrogen (H2) as a second reactant: its column is the narrowest a
    /// value column gets, [`TABLE_MIN_COLUMN`] points, a whole number.
    fn drawn_with_hydrogen() -> Molecule {
        let mut m = Molecule::default();
        let a = m.add_atom("C".into(), [5.0, 5.0], 0);
        let b = m.add_atom("C".into(), [6.3, 5.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        let h = m.add_atom("H".into(), [8.0, 5.0], 0);
        let h2 = m.add_atom("H".into(), [9.3, 5.0], 0);
        m.add_bond(h, h2, BondOrder::Single);
        m.add_arrow([11.0, 5.0], [15.0, 5.0], ArrowKind::Reaction);
        let c = m.add_atom("C".into(), [16.0, 5.0], 0);
        let d = m.add_atom("C".into(), [17.3, 5.0], 0);
        m.add_bond(c, d, BondOrder::Single);
        let t = new_table(&m, None).unwrap();
        m.push_object(PageObject::Table(t));
        m
    }

    #[test]
    fn grid_widths_saved_in_points_by_earlier_versions_still_read() {
        // Before ChemDraw's spelling was followed, the codec wrote the widths in points as it
        // writes every coordinate, trailing zeros trimmed: the minimum column went out as
        // `Width="40"`. Those files read as they were meant, and are saved as ChemDraw spells
        // them.
        let m = drawn_with_hydrogen();
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        let written = widths(&grid_of(&doc));
        let old = cdx_file_rs::cdxml::write_cdxml(&doc.clone().into());
        let min = (TABLE_MIN_COLUMN as f64).to_string();
        assert!(cdxml_widths(&old).contains(&min), "a column at the minimum, {min} pt: {:?}", cdxml_widths(&old));
        let close = |read: &[i32], what: &str| {
            assert_eq!(read.len(), written.len(), "{what}");
            for (a, b) in read.iter().zip(&written) {
                assert!((a - b).abs() <= 1, "{what}: {read:?} vs {written:?}");
            }
        };
        close(&widths(&grid_of(&cdx::cdxml_document(&old).unwrap())), "read");
        // Opened and saved again, through the document the file was.
        let original = cdx_codec::write(&cdx::cdxml_document(&old).unwrap());
        let read = read_cdxml(&old).unwrap().molecule;
        let saved = cdxml_of_document(&cdx_codec::parse(&merge_into_cdx(&original, &read)).unwrap());
        let units: Vec<i32> = cdxml_widths(&saved).iter().map(|w| w.parse().unwrap()).collect();
        close(&units, "saved");
        close(&widths(&grid_of(&cdx::cdxml_document(&saved).unwrap())), "saved and read");
    }

    /// The ChemDraw document with its first component's `Width="5534515"` written as `width`.
    fn analysis_with_first_width(width: &str) -> String {
        let xml = String::from_utf8(fixture("chemdraw25_reaction_analysis.cdxml")).unwrap();
        assert!(xml.contains("Width=\"5534515\""));
        xml.replacen("Width=\"5534515\"", width, 1)
    }

    #[test]
    fn a_components_width_reads_however_the_xml_spells_the_attribute() {
        let chemdraw = [5534515, 5639372, 3968204, 3968204];
        for spelling in ["Width = \"5534515\"", "Width= '5534515'", "Width\t=\n\"5534515\"", "Width='5534515'", "Width=\" 5534515 \""] {
            let grid = grid_of(&cdx::cdxml_document(&analysis_with_first_width(spelling)).unwrap());
            assert_eq!(widths(&grid), chemdraw, "{spelling:?}");
        }
        // Width first, before the id.
        let xml = analysis_with_first_width("").replacen("<sgcomponent", "<sgcomponent Width = '5534515'", 1);
        assert!(xml.contains("<sgcomponent Width = '5534515'\n id=\"1\"") || xml.contains("<sgcomponent Width = '5534515'\r\n id=\"1\""));
        assert_eq!(widths(&grid_of(&cdx::cdxml_document(&xml).unwrap())), chemdraw, "attribute order");
        // An earlier version's whole-point width, however spelled.
        for spelling in ["Width = \"40\"", "Width='40'", "Width =\t'40.0'"] {
            let grid = grid_of(&cdx::cdxml_document(&analysis_with_first_width(spelling)).unwrap());
            assert_eq!(widths(&grid)[0], 40 * 65536, "{spelling:?}");
        }
    }

    /// The components of a grid in the order ChemDraw lays their columns out: each block's
    /// header column, then its value columns.
    fn in_column_order(grid: &CdxObject) -> Vec<&CdxObject> {
        let components: Vec<&CdxObject> = grid.children.iter().filter(|c| c.tag == OBJ_SG_COMPONENT).collect();
        let mut out = Vec::new();
        for reactant in [true, false] {
            for header in [true, false] {
                out.extend(components.iter().filter(|c| {
                    flag(c, PROP_COMPONENT_IS_REACTANT) == reactant && flag(c, PROP_COMPONENT_IS_HEADER) == header
                }));
            }
        }
        out
    }

    /// The x ranges (points) of the columns ChemDraw lays out from a CDXML text: the widths as
    /// it reads them (integers in CDX units), side by side from the grid's corner, the blocks
    /// [`TABLE_BLOCK_GAP`] apart.
    fn column_ranges(xml: &str, grid: &CdxObject) -> Vec<(f64, f64)> {
        let widths: Vec<f64> = cdxml_widths(xml).iter().map(|w| w.parse::<f64>().unwrap().trunc() / PT).collect();
        let by_id: HashMap<u32, f64> = grid.children.iter().filter(|c| c.tag == OBJ_SG_COMPONENT).map(|c| c.id).zip(widths).collect();
        let (x0, _) = point_2d(grid).unwrap();
        let mut x = x0 / PT;
        let mut out = Vec::new();
        let mut was_reactant = true;
        for c in in_column_order(grid) {
            if was_reactant && !flag(c, PROP_COMPONENT_IS_REACTANT) {
                x += TABLE_BLOCK_GAP as f64;
                was_reactant = false;
            }
            out.push((x, x + by_id[&c.id]));
            x += by_id[&c.id];
        }
        out
    }

    #[test]
    fn cells_stay_inside_their_columns_and_rows() {
        let m = drawn();
        let xml = molecule_to_cdxml(&m);
        let grid = grid_of(&cdx::cdxml_document(&xml).unwrap());
        let columns = column_ranges(&xml, &grid);
        assert_eq!(columns.len(), 5, "two headers, two reactants, one product");
        for w in columns.windows(2) {
            assert!(w[0].1 <= w[1].0 + 1e-6, "columns do not overlap: {columns:?}");
        }
        // Every shown text lies inside its column (right of the pad) and inside its row, and
        // the rows do not overlap.
        let (_, y0) = point_2d(&grid).unwrap();
        let frame_top = y0 / PT + TABLE_FRAME_TOP as f64;
        for (c, &(left, right)) in in_column_order(&grid).into_iter().zip(&columns) {
            let mut row = 0;
            let mut last_bottom = f64::NEG_INFINITY;
            for d in c.children.iter().filter(|d| d.tag == OBJ_SG_DATUM) {
                if d.prop_i16(PROP_SG_PROPERTY_TYPE) == Some(LIMIT_MOLES) {
                    continue;
                }
                let t = &d.children[0].children[0];
                let b = cdx::bounding_box(t).unwrap().map(|v| v / PT);
                let top = frame_top + row as f64 * TABLE_ROW_HEIGHT as f64;
                let bottom = top + TABLE_ROW_HEIGHT as f64;
                assert!(b[0] >= left + TABLE_CELL_PAD as f64 - 0.01 && b[2] <= right + 0.01, "{b:?} in {left}..{right}");
                assert!(b[1] >= top - 0.01 && b[3] <= bottom + 0.01, "{b:?} in row {top}..{bottom}");
                assert!(b[1] >= last_bottom - 0.01, "rows do not overlap");
                last_bottom = b[3];
                row += 1;
            }
        }
    }

    #[test]
    fn a_deleted_table_is_gone_from_the_file() {
        let original = analysis_cdx();
        let mut m = read_cdx(&original).unwrap().molecule;
        let id = tables(&m)[0].id;
        assert!(m.remove_object(id));
        let saved = merge_into_cdx(&original, &m);
        assert!(find(&cdx_codec::parse(&saved).unwrap(), GRID, OBJ_STOICHIOMETRY_GRID).is_none());
        assert!(tables(&read_cdx(&saved).unwrap().molecule).is_empty());
    }

    #[test]
    fn the_analysis_sample_has_no_table() {
        // ChemDraw's `Analysis` document is a structure with chemical properties: nothing to read.
        let import = read_cdx(&fixture("chemdraw25_analysis.cdx")).unwrap();
        assert!(tables(&import.molecule).is_empty());
        let xml = String::from_utf8(fixture("chemdraw25_analysis.cdxml")).unwrap();
        assert!(tables(&read_cdxml(&xml).unwrap().molecule).is_empty());
    }
}
