//! The reaction scheme: structures, `+` separators and arrows with text above and below,
//! laid out left to right on a reaction line. A multi-step scheme runs on over several rows:
//! a [`SchemeItem::LineBreak`] starts a new one, and so does an item that would make a row
//! wider than [`ReactionStyle::max_row_width`]. A structure can carry a compound label
//! (`(I)`, **4**) centred under it.
//!
//! Structures are drawn by the *editor's own* renderer ([`crate::widget::draw`]) so a
//! structure in a scheme looks exactly like the same structure in the editor: same label
//! anchoring, same H folding, same bond trimming, same double-bond placement. The renderer
//! works off a [`ChemStructEditor`], so a throw-away one is used as a drawing context; the
//! molecule is scaled into it instead of zooming, which keeps line widths and label sizes
//! absolute (see [`ReactionStyle::structure_scale`]).

use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};
use serde::{Deserialize, Serialize};

use crate::config::StyleConfig;
use crate::molecule::Molecule;
use crate::widget::rich_text::{RichText, RichTextStyle};
use crate::widget::{ChemStructEditor, draw as structure_draw};

use super::style::ReactionStyle;
use super::text::{Run, RunLayout, layout_runs};

/// A stable identity for a structure in a [`ReactionScheme`].
///
/// Every [`SchemeStructure`] gets a fresh, process-unique id when it is created, so a structure
/// can be told apart from another one carrying an identical molecule, and followed while it
/// moves around the scheme. Cloning a [`SchemeStructure`] keeps its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructureId(u64);

impl StructureId {
    /// A new id, distinct from every id handed out before in this process.
    pub fn fresh() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A structure placed in a [`ReactionScheme`]: a [`Molecule`] plus its [`StructureId`].
///
/// It dereferences to the molecule, so it can be drawn and inspected wherever a `&Molecule`
/// is expected, and compares equal to a bare [`Molecule`] with the same contents (the id is
/// ignored in that comparison).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemeStructure {
    id: StructureId,
    molecule: Molecule,
    /// The compound label drawn under the structure (`(I)`, **4**); empty for none, and then
    /// not written, so a file from before labels reads the same.
    #[serde(default, skip_serializing_if = "RichText::is_empty")]
    label: RichText,
}

impl SchemeStructure {
    /// Wrap `molecule` with a fresh [`StructureId`].
    pub fn new(molecule: Molecule) -> Self {
        Self { id: StructureId::fresh(), molecule, label: RichText::new() }
    }

    /// This structure with a compound label under it.
    pub fn with_label(mut self, label: impl Into<RichText>) -> Self {
        self.label = label.into();
        self
    }

    /// Give the structure a fresh id, returning `(old, new)`. Used when loading a saved
    /// reaction, whose ids were handed out by another process and may collide with ours.
    pub(crate) fn renew_id(&mut self) -> (StructureId, StructureId) {
        let old = self.id;
        self.id = StructureId::fresh();
        (old, self.id)
    }

    /// This structure's id.
    pub fn id(&self) -> StructureId {
        self.id
    }

    /// The molecule.
    pub fn molecule(&self) -> &Molecule {
        &self.molecule
    }

    /// The molecule, mutably. The id stays the same.
    pub fn molecule_mut(&mut self) -> &mut Molecule {
        &mut self.molecule
    }

    /// The compound label under the structure (empty for none).
    pub fn label(&self) -> &RichText {
        &self.label
    }

    /// Replace the compound label; empty text removes it.
    pub fn set_label(&mut self, label: RichText) {
        self.label = label;
    }

    /// Unwrap the molecule, dropping the id.
    pub fn into_molecule(self) -> Molecule {
        self.molecule
    }
}

impl From<Molecule> for SchemeStructure {
    fn from(molecule: Molecule) -> Self {
        Self::new(molecule)
    }
}

impl Deref for SchemeStructure {
    type Target = Molecule;

    fn deref(&self) -> &Molecule {
        &self.molecule
    }
}

impl DerefMut for SchemeStructure {
    fn deref_mut(&mut self) -> &mut Molecule {
        &mut self.molecule
    }
}

impl PartialEq<Molecule> for SchemeStructure {
    fn eq(&self, other: &Molecule) -> bool {
        self.molecule == *other
    }
}

/// One element of a reaction scheme, drawn left to right.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SchemeItem {
    /// A drawn structure, centred on the reaction line.
    Structure(SchemeStructure),
    /// A `+` between two reagents.
    Plus,
    /// The reaction arrow, with its conditions above and below.
    Arrow(Arrow),
    /// Free-standing text on the reaction line (a reagent written out, `Δ`, a label …).
    Text(RichText),
    /// The end of a row: the next item starts a new row under this one.
    LineBreak,
}

/// A label riding above or below a reaction arrow: nothing, some text, or a drawn structure.
///
/// A `Structure` label is the *same* [`Molecule`] a main-line structure carries, so it draws
/// identically (same bond trimming, label anchoring and ring fills) — a reagent or catalyst
/// shown as its structure rather than written out as a name.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum ArrowLabel {
    /// Nothing on this side of the shaft.
    #[default]
    Empty,
    /// Rich text (reagents, a base, a solvent, a temperature).
    Text(RichText),
    /// A drawn structure.
    Structure(SchemeStructure),
}

impl ArrowLabel {
    /// True when there is nothing on this side of the shaft.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// The rich text, when this label is text.
    pub fn as_text(&self) -> Option<&RichText> {
        match self {
            Self::Text(t) => Some(t),
            _ => None,
        }
    }

    /// The drawn structure, when this label is one.
    pub fn as_structure(&self) -> Option<&Molecule> {
        match self {
            Self::Structure(m) => Some(m.molecule()),
            _ => None,
        }
    }

    /// The drawn structure, mutably, when this label is one.
    pub fn as_structure_mut(&mut self) -> Option<&mut Molecule> {
        match self {
            Self::Structure(m) => Some(m.molecule_mut()),
            _ => None,
        }
    }
}

/// Text folds to [`ArrowLabel::Text`], and empty text to [`ArrowLabel::Empty`], so the
/// `.above("…")` / `.below("…")` builders keep taking plain markup.
impl From<RichText> for ArrowLabel {
    fn from(text: RichText) -> Self {
        if text.is_empty() {
            Self::Empty
        } else {
            Self::Text(text)
        }
    }
}

/// A reaction arrow and the labels riding on it.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Arrow {
    /// Label above the shaft (reagents).
    pub above: ArrowLabel,
    /// Label below the shaft (base, solvent, temperature).
    pub below: ArrowLabel,
    /// Explicit shaft length in pixels. `None` uses [`ReactionStyle::arrow_length`], widened
    /// when necessary to fit the label above or below it.
    pub length: Option<f32>,
}

impl Arrow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Text above the shaft. `&str` goes through the [rich-text markup](crate::RichText)
    /// parser, so `"K_2CO_3"` gets real subscripts.
    pub fn above(mut self, text: impl Into<RichText>) -> Self {
        self.above = ArrowLabel::from(text.into());
        self
    }

    /// Text below the shaft.
    pub fn below(mut self, text: impl Into<RichText>) -> Self {
        self.below = ArrowLabel::from(text.into());
        self
    }

    /// A structure above the shaft, drawn like a main-line scheme structure.
    pub fn above_structure(mut self, molecule: Molecule) -> Self {
        self.above = ArrowLabel::Structure(molecule.into());
        self
    }

    /// A structure below the shaft, drawn like a main-line scheme structure.
    pub fn below_structure(mut self, molecule: Molecule) -> Self {
        self.below = ArrowLabel::Structure(molecule.into());
        self
    }

    pub fn length(mut self, px: f32) -> Self {
        self.length = Some(px);
        self
    }
}

/// A reaction scheme: a sequence of [`SchemeItem`]s.
///
/// ```no_run
/// use chembuider_rs::reaction::{Arrow, ReactionScheme, demo};
/// let scheme = ReactionScheme::new()
///     .structure(demo::phenol())
///     .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
///     .structure(demo::anisole());
/// ```
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ReactionScheme {
    pub items: Vec<SchemeItem>,
}

impl ReactionScheme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(mut self, item: SchemeItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn structure(self, molecule: Molecule) -> Self {
        self.item(SchemeItem::Structure(molecule.into()))
    }

    pub fn plus(self) -> Self {
        self.item(SchemeItem::Plus)
    }

    pub fn arrow(self, arrow: Arrow) -> Self {
        self.item(SchemeItem::Arrow(arrow))
    }

    pub fn text(self, text: impl Into<RichText>) -> Self {
        self.item(SchemeItem::Text(text.into()))
    }

    /// A structure with a compound label (`"(I)"`, `"**4**\n(V)"`) under it.
    pub fn labeled_structure(self, molecule: Molecule, label: impl Into<RichText>) -> Self {
        self.item(SchemeItem::Structure(SchemeStructure::new(molecule).with_label(label)))
    }

    /// End the row: what follows starts a new row.
    pub fn line_break(self) -> Self {
        self.item(SchemeItem::LineBreak)
    }

    /// How many arrows (reaction steps) the scheme has.
    pub fn arrow_count(&self) -> usize {
        self.items.iter().filter(|i| matches!(i, SchemeItem::Arrow(_))).count()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    // ── Mutation (the editing UI drives these) ───────────────────────────────

    /// Append an item; returns its index.
    pub fn add(&mut self, item: SchemeItem) -> usize {
        self.items.push(item);
        self.items.len() - 1
    }

    /// Drop an item, returning it. Out-of-range indices are ignored.
    pub fn remove(&mut self, index: usize) -> Option<SchemeItem> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }

    /// Move an item `delta` places along the reaction line, clamped to the ends. Returns the
    /// index it ended up at, or `None` when there was nothing at `index`.
    pub fn move_item(&mut self, index: usize, delta: isize) -> Option<usize> {
        if index >= self.items.len() {
            return None;
        }
        let last = self.items.len() - 1;
        let target = (index as isize + delta).clamp(0, last as isize) as usize;
        let item = self.items.remove(index);
        self.items.insert(target, item);
        Some(target)
    }
}

// ─── Layout ──────────────────────────────────────────────────────────────────

/// A measured scheme: where every item goes, relative to the scheme's top-left.
pub struct SchemeLayout {
    /// One entry per item of the scheme, in order.
    pub items: Vec<PlacedItem>,
    /// The rows the items run on, top to bottom. A scheme without items has none.
    pub rows: Vec<SchemeRow>,
    /// Tight size of the whole scheme.
    pub size: Vec2,
    /// Distance from the top to the first row's reaction line (the arrow shaft).
    pub center_line: f32,
}

impl SchemeLayout {
    /// The reaction line of the row item `index` sits on, from the scheme's top.
    pub fn line_of(&self, index: usize) -> f32 {
        self.items
            .get(index)
            .and_then(|item| self.rows.get(item.row))
            .map_or(self.center_line, |row| row.center_line)
    }

    /// The row whose band (from its top to its bottom, gaps split between neighbours) holds `y`,
    /// measured from the scheme's top. Above the first row is the first, below the last the last.
    pub fn row_at(&self, y: f32) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }
        Some(
            self.rows
                .windows(2)
                .position(|pair| y < (pair[0].bottom + pair[1].top) * 0.5)
                .unwrap_or(self.rows.len() - 1),
        )
    }
}

/// One row of a scheme.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemeRow {
    /// Top of the row, from the scheme's top.
    pub top: f32,
    /// The row's reaction line, from the scheme's top.
    pub center_line: f32,
    /// Bottom of the row, from the scheme's top.
    pub bottom: f32,
    /// The items on the row: indices into [`SchemeLayout::items`].
    pub items: std::ops::Range<usize>,
}

/// One measured item: its box, plus whatever the paint pass needs to draw it.
pub struct PlacedItem {
    /// Box of this item, relative to the scheme's top-left. A structure's box takes in its
    /// compound label.
    pub rect: Rect,
    /// The row it sits on: an index into [`SchemeLayout::rows`].
    pub row: usize,
    pub kind: PlacedKind,
}

/// Paint-ready form of a measured item.
pub enum PlacedKind {
    /// Where the molecule's coordinate origin lands, relative to the scheme's top-left, and its
    /// compound label, if it has one.
    Structure { origin: Vec2, label: Option<PlacedLabel> },
    /// Pre-laid-out text (`+`, a free text item).
    Text(RunLayout),
    /// Arrow shaft length plus the pre-laid-out label above and below it.
    Arrow {
        length: f32,
        above: PlacedArrowLabel,
        below: PlacedArrowLabel,
    },
    /// The end of a row: nothing to draw.
    LineBreak,
}

/// A compound label, laid out and placed under its structure.
pub struct PlacedLabel {
    /// Its lines, top to bottom, each centred in [`rect`](Self::rect).
    pub lines: Vec<RunLayout>,
    /// The label's box, relative to the scheme's top-left.
    pub rect: Rect,
}

impl PlacedLabel {
    /// Paint the label with the scheme's top-left at `top_left`.
    pub fn paint(&self, painter: &Painter, top_left: Pos2) {
        let rect = self.rect.translate(top_left.to_vec2());
        let mut y = rect.min.y;
        for line in &self.lines {
            line.paint(painter, Pos2::new(rect.center().x - line.size.x * 0.5, y));
            y += line.size.y;
        }
    }
}

/// Paint-ready form of one of an arrow's two labels.
pub enum PlacedArrowLabel {
    /// Nothing on this side of the shaft.
    Empty,
    /// Pre-laid-out text.
    Text(RunLayout),
    /// A structure whose molecule origin lands at `origin`, relative to the scheme's top-left
    /// — the same coordinate a main-line [`PlacedKind::Structure`] uses.
    Structure { origin: Vec2 },
}

impl PlacedArrowLabel {
    /// True when this side of the shaft carries nothing.
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
}

/// A measured arrow label, before its structure origin is placed on the reaction line.
enum ArrowLabelMeasure {
    Empty,
    Text(RunLayout),
    Structure { bounds: Rect },
}

/// A measured scheme item, before the placement pass fixes it to scheme coordinates.
enum Measured {
    Ready(PlacedKind),
    Structure {
        /// The molecule origin inside the item box.
        origin: Vec2,
        /// Width and height of the structure itself (the box above the label).
        size: Vec2,
        /// The label's lines and the size they take together.
        label: Option<(Vec<RunLayout>, Vec2)>,
    },
    Arrow {
        length: f32,
        above: ArrowLabelMeasure,
        below: ArrowLabelMeasure,
    },
}

/// One item's extent: its width, how far it reaches above and below the reaction line.
struct Extent {
    width: f32,
    above: f32,
    below: f32,
    measured: Measured,
}

/// Measure a scheme. `scratch` is a drawing context for structures; its molecule and
/// transform are overwritten.
pub fn measure_scheme(
    scheme: &ReactionScheme,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) -> SchemeLayout {
    // Half the arrow head is the shaft's own extent; a label stacks on top of that.
    let half_shaft = (style.arrow_width * 0.5).max(style.arrow_head_half_width);
    let measured: Vec<Extent> = scheme
        .items
        .iter()
        .map(|item| measure_item(item, painter, style, structure_style, scratch, half_shaft))
        .collect();
    if measured.is_empty() {
        return SchemeLayout { items: Vec::new(), rows: Vec::new(), size: Vec2::ZERO, center_line: 0.0 };
    }

    // Break the items into rows: after a line break, and before an item that would make the
    // row wider than the style allows (a row always takes at least one item).
    let mut row_ranges: Vec<std::ops::Range<usize>> = Vec::new();
    let (mut start, mut pen_x) = (0usize, 0.0_f32);
    for (i, m) in measured.iter().enumerate() {
        let right = if i == start { m.width } else { pen_x + style.item_gap + m.width };
        let too_wide = style.max_row_width.is_some_and(|max| right > max);
        if i > start && too_wide && !matches!(m.measured, Measured::Ready(PlacedKind::LineBreak)) {
            row_ranges.push(start..i);
            start = i;
            pen_x = m.width;
        } else {
            pen_x = right;
        }
        if matches!(m.measured, Measured::Ready(PlacedKind::LineBreak)) {
            row_ranges.push(start..i + 1);
            start = i + 1;
            pen_x = 0.0;
        }
    }
    if start < measured.len() {
        row_ranges.push(start..measured.len());
    }

    let mut items = Vec::with_capacity(measured.len());
    let mut rows = Vec::with_capacity(row_ranges.len());
    let mut measured = measured.into_iter();
    let (mut top, mut width) = (0.0_f32, 0.0_f32);
    for (row, range) in row_ranges.into_iter().enumerate() {
        let row_items: Vec<Extent> = measured.by_ref().take(range.len()).collect();
        if row > 0 {
            top += style.row_gap;
        }
        let center_line = top + row_items.iter().map(|m| m.above).fold(0.0_f32, f32::max);
        let bottom = center_line + row_items.iter().map(|m| m.below).fold(0.0_f32, f32::max);
        let mut pen_x = 0.0_f32;
        let mut placed_any = false;
        for m in row_items {
            // A line break takes no room: it sits at the end of its row.
            if !matches!(m.measured, Measured::Ready(PlacedKind::LineBreak)) {
                if placed_any {
                    pen_x += style.item_gap;
                }
                placed_any = true;
            }
            let rect = Rect::from_min_size(
                Pos2::new(pen_x, center_line - m.above),
                Vec2::new(m.width, m.above + m.below),
            );
            let kind = place(m.measured, rect, center_line, half_shaft, style);
            items.push(PlacedItem { rect, row, kind });
            pen_x += m.width;
        }
        width = width.max(pen_x);
        rows.push(SchemeRow { top, center_line, bottom, items: range });
        top = bottom;
    }

    SchemeLayout { size: Vec2::new(width, top), center_line: rows[0].center_line, items, rows }
}

fn measure_item(
    item: &SchemeItem,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
    half_shaft: f32,
) -> Extent {
    match item {
        SchemeItem::Structure(mol) => {
            let bounds = structure_bounds(mol, painter, style, structure_style, scratch);
            let half = bounds.height() * 0.5;
            // Origin offset inside the item box: the box starts at the bounds' top-left.
            let origin = Vec2::new(-bounds.min.x, -bounds.min.y);
            let lines: Vec<RunLayout> = split_lines(mol.label())
                .iter()
                .map(|line| text_layout(line, style.compound_label_font_size, painter))
                .collect();
            let label_size = lines.iter().fold(Vec2::ZERO, |acc, l| Vec2::new(acc.x.max(l.size.x), acc.y + l.size.y));
            let (label, width, below) = if mol.label().is_empty() {
                (None, bounds.width(), half)
            } else {
                let width = bounds.width().max(label_size.x);
                (Some((lines, label_size)), width, half + style.compound_label_gap + label_size.y)
            };
            Extent {
                width,
                above: half,
                below,
                measured: Measured::Structure { origin, size: bounds.size(), label },
            }
        }
        SchemeItem::Plus => {
            let layout = text_layout(&RichText::plain("+"), style.plus_font_size, painter);
            let half = layout.size.y * 0.5;
            Extent { width: layout.size.x, above: half, below: half, measured: Measured::Ready(PlacedKind::Text(layout)) }
        }
        SchemeItem::Text(text) => {
            let layout = text_layout(text, style.scheme_font_size, painter);
            let half = layout.size.y * 0.5;
            Extent { width: layout.size.x, above: half, below: half, measured: Measured::Ready(PlacedKind::Text(layout)) }
        }
        SchemeItem::LineBreak => {
            Extent { width: 0.0, above: 0.0, below: 0.0, measured: Measured::Ready(PlacedKind::LineBreak) }
        }
        SchemeItem::Arrow(arrow) => {
            let above = measure_arrow_label(&arrow.above, painter, style, structure_style, scratch);
            let below = measure_arrow_label(&arrow.below, painter, style, structure_style, scratch);
            let width = |m: &ArrowLabelMeasure| match m {
                ArrowLabelMeasure::Empty => 0.0,
                ArrowLabelMeasure::Text(t) => t.size.x,
                ArrowLabelMeasure::Structure { bounds } => bounds.width(),
            };
            let label_width = width(&above).max(width(&below));
            let length =
                arrow.length.unwrap_or_else(|| style.arrow_length.max(label_width + 2.0 * style.item_gap));
            let extent = |m: &ArrowLabelMeasure| match m {
                ArrowLabelMeasure::Empty => half_shaft,
                ArrowLabelMeasure::Text(t) => half_shaft + style.arrow_text_gap + t.size.y,
                ArrowLabelMeasure::Structure { bounds } => half_shaft + style.arrow_text_gap + bounds.height(),
            };
            Extent {
                width: length,
                above: extent(&above),
                below: extent(&below),
                measured: Measured::Arrow { length, above, below },
            }
        }
    }
}

/// Fix a measured item to its box on a row whose reaction line is at `center_line`.
fn place(measured: Measured, rect: Rect, center_line: f32, half_shaft: f32, style: &ReactionStyle) -> PlacedKind {
    match measured {
        Measured::Ready(kind) => kind,
        // The structure is centred in its box (which a wider label widens), the label under it.
        Measured::Structure { origin, size, label } => {
            let left = rect.center().x - size.x * 0.5;
            let label = label.map(|(lines, label_size)| {
                let min = Pos2::new(rect.center().x - label_size.x * 0.5, rect.min.y + size.y + style.compound_label_gap);
                PlacedLabel { rect: Rect::from_min_size(min, label_size), lines }
            });
            PlacedKind::Structure { origin: origin + Vec2::new(left, rect.min.y), label }
        }
        // Place each arrow-label structure so it is centred over the shaft and sits its own
        // height clear of it, above or below the reaction line.
        Measured::Arrow { length, above, below } => {
            let shaft_center_x = rect.min.x + length * 0.5;
            let place = |m: ArrowLabelMeasure, is_above: bool| -> PlacedArrowLabel {
                match m {
                    ArrowLabelMeasure::Empty => PlacedArrowLabel::Empty,
                    ArrowLabelMeasure::Text(t) => PlacedArrowLabel::Text(t),
                    ArrowLabelMeasure::Structure { bounds } => {
                        let ox = shaft_center_x - bounds.center().x;
                        let oy = if is_above {
                            (center_line - half_shaft - style.arrow_text_gap) - bounds.max.y
                        } else {
                            (center_line + half_shaft + style.arrow_text_gap) - bounds.min.y
                        };
                        PlacedArrowLabel::Structure { origin: Vec2::new(ox, oy) }
                    }
                }
            };
            PlacedKind::Arrow { length, above: place(above, true), below: place(below, false) }
        }
    }
}

/// Paint a measured scheme with its top-left corner at `top_left`.
pub fn paint_scheme(
    scheme: &ReactionScheme,
    layout: &SchemeLayout,
    painter: &Painter,
    top_left: Pos2,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) {
    for (item, placed) in scheme.items.iter().zip(&layout.items) {
        let rect = placed.rect.translate(top_left.to_vec2());
        let line_y = top_left.y + layout.rows.get(placed.row).map_or(layout.center_line, |r| r.center_line);
        match (&placed.kind, item) {
            (PlacedKind::Structure { origin, label }, SchemeItem::Structure(mol)) => {
                draw_structure(mol, painter, top_left + *origin, style, structure_style, scratch);
                if let Some(label) = label {
                    label.paint(painter, top_left);
                }
            }
            (PlacedKind::Text(text), _) => {
                text.paint(painter, Pos2::new(rect.min.x, line_y - text.size.y * 0.5));
            }
            (PlacedKind::Arrow { length, above, below }, SchemeItem::Arrow(arrow)) => {
                draw_arrow(painter, Pos2::new(rect.min.x, line_y), *length, style);
                let half_shaft = (style.arrow_width * 0.5).max(style.arrow_head_half_width);
                // Text sides paint here; a structure side paints through the molecule renderer
                // at the scheme-relative origin the layout placed it at.
                match above {
                    PlacedArrowLabel::Empty => {}
                    PlacedArrowLabel::Text(t) => {
                        let x = rect.min.x + (*length - t.size.x) * 0.5;
                        let y = line_y - half_shaft - style.arrow_text_gap - t.size.y;
                        t.paint(painter, Pos2::new(x, y));
                    }
                    PlacedArrowLabel::Structure { origin } => {
                        if let Some(mol) = arrow.above.as_structure() {
                            draw_structure(mol, painter, top_left + *origin, style, structure_style, scratch);
                        }
                    }
                }
                match below {
                    PlacedArrowLabel::Empty => {}
                    PlacedArrowLabel::Text(t) => {
                        let x = rect.min.x + (*length - t.size.x) * 0.5;
                        let y = line_y + half_shaft + style.arrow_text_gap;
                        t.paint(painter, Pos2::new(x, y));
                    }
                    PlacedArrowLabel::Structure { origin } => {
                        if let Some(mol) = arrow.below.as_structure() {
                            draw_structure(mol, painter, top_left + *origin, style, structure_style, scratch);
                        }
                    }
                }
            }
            // Measured and painted items are built from the same list, so the pairs always
            // match; a mismatch would mean the layout is stale — draw nothing rather than panic.
            _ => {}
        }
    }
}

/// A right-pointing reaction arrow with a filled head, `tail` sitting on the reaction line.
pub fn draw_arrow(painter: &Painter, tail: Pos2, length: f32, style: &ReactionStyle) {
    let tip = Pos2::new(tail.x + length, tail.y);
    let head_base = tip.x - style.arrow_head_length;
    // Stop the shaft inside the head so the two never disagree at the tip.
    painter.line_segment(
        [tail, Pos2::new(head_base + style.arrow_head_length * 0.5, tail.y)],
        Stroke::new(style.arrow_width, Color32::BLACK),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            Pos2::new(head_base, tail.y - style.arrow_head_half_width),
            Pos2::new(head_base, tail.y + style.arrow_head_half_width),
        ],
        Color32::BLACK,
        Stroke::NONE,
    ));
}

// ─── Structures ──────────────────────────────────────────────────────────────

/// Load `mol` into the throw-away editor, scaled so one bond is
/// [`ReactionStyle::bond_length`] pixels long at zoom 1.
fn load(mol: &Molecule, style: &ReactionStyle, structure_style: &StyleConfig, scratch: &mut ChemStructEditor) {
    let k = style.structure_scale();
    scratch.molecule = mol.clone();
    for atom in &mut scratch.molecule.atoms {
        atom.pos = [atom.pos[0] * k, atom.pos[1] * k];
    }
    scratch.zoom = 1.0;
    scratch.pan = Vec2::ZERO;
    scratch.config.style = structure_style.clone();
    // No editing state may leak into a static drawing.
    scratch.selected_atoms.clear();
    scratch.selected_bonds.clear();
    scratch.hovered_atom = None;
    scratch.hovered_bond = None;
    scratch.hotspot_atom = None;
    scratch.hotspot_bond = None;
}

/// Screen-space extent of a structure — atoms *and* their labels — relative to the
/// molecule's coordinate origin.
pub fn structure_bounds(
    mol: &Molecule,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) -> Rect {
    load(mol, style, structure_style, scratch);
    let labels = structure_draw::compute_label_layouts(scratch, painter, Pos2::ZERO);
    let mut bounds: Option<Rect> = None;
    let mut grow = |r: Rect| {
        bounds = Some(bounds.map_or(r, |b: Rect| b.union(r)));
    };
    for atom in &scratch.molecule.atoms {
        if structure_draw::is_folded_h(&scratch.molecule, atom.id) {
            continue;
        }
        let p = scratch.mol_to_screen(atom.pos, Pos2::ZERO);
        grow(Rect::from_center_size(p, Vec2::splat(structure_style.bond_width.max(1.0))));
    }
    for label in labels.values() {
        grow(label.rect);
    }
    bounds.unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO))
}

/// Draw a structure with its coordinate origin at `origin`, through the editor's renderer.
pub fn draw_structure(
    mol: &Molecule,
    painter: &Painter,
    origin: Pos2,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) {
    load(mol, style, structure_style, scratch);
    let labels = structure_draw::compute_label_layouts(scratch, painter, origin);
    structure_draw::draw_bonds(scratch, painter, origin, &labels);
    structure_draw::draw_atom_backgrounds(scratch, painter, origin, &labels);
    structure_draw::draw_atom_labels(scratch, painter, origin, &labels);
}

/// Measure one arrow label: nothing, laid-out text, or a structure (whose screen bounds are
/// kept so the placement pass can centre it over the shaft).
fn measure_arrow_label(
    label: &ArrowLabel,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) -> ArrowLabelMeasure {
    match label {
        ArrowLabel::Empty => ArrowLabelMeasure::Empty,
        ArrowLabel::Text(t) => {
            let layout = text_layout(t, style.arrow_font_size, painter);
            if layout.is_empty() {
                ArrowLabelMeasure::Empty
            } else {
                ArrowLabelMeasure::Text(layout)
            }
        }
        ArrowLabel::Structure(mol) => {
            let bounds = structure_bounds(mol, painter, style, structure_style, scratch);
            ArrowLabelMeasure::Structure { bounds }
        }
    }
}

/// Rich text cut at its line breaks, each line keeping its spans' styles. An empty line (two
/// breaks in a row) stays, as a blank line.
fn split_lines(text: &RichText) -> Vec<RichText> {
    let mut lines = vec![RichText::new()];
    for span in &text.spans {
        for (k, part) in span.text.split('\n').enumerate() {
            if k > 0 {
                lines.push(RichText::new());
            }
            let last = lines.last_mut().expect("starts with one line");
            *last = std::mem::take(last).span(part, span.style);
        }
    }
    lines
}

fn text_layout(text: &RichText, size: f32, painter: &Painter) -> RunLayout {
    if text.is_empty() {
        return RunLayout::EMPTY;
    }
    let style = RichTextStyle::new(size, Color32::BLACK);
    let runs = [Run::new(text.clone())];
    layout_runs(&runs, &style, 0.0, |job| painter.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;

    /// Run `f` inside a headless egui frame with a real font stack.
    fn with_painter(mut f: impl FnMut(&Painter)) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            ..Default::default()
        };
        let _ = crate::testkit::run_ui(&ctx, raw, |ui| f(ui.painter()));
    }

    fn scratch() -> ChemStructEditor {
        ChemStructEditor::default()
    }

    /// The reference scheme measures to three items on one reaction line: phenol, the
    /// arrow, anisole — left to right, non-overlapping, all crossing the line.
    #[test]
    fn scheme_lays_items_out_along_the_reaction_line() {
        let scheme = demo::williamson_ether_synthesis().scheme;
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(&scheme, painter, &style, &sstyle, &mut ed);
            assert_eq!(layout.items.len(), 3);

            let mut prev_right = f32::MIN;
            for item in &layout.items {
                assert!(
                    item.rect.min.x >= prev_right - 0.01,
                    "items must not overlap: {:?} after {prev_right}",
                    item.rect
                );
                prev_right = item.rect.max.x;
                assert!(
                    item.rect.min.y <= layout.center_line && item.rect.max.y >= layout.center_line,
                    "every item straddles the reaction line: {:?}",
                    item.rect
                );
            }
            assert!((layout.size.x - prev_right).abs() < 0.01);
            assert!(layout.size.y > 0.0 && layout.center_line > 0.0);

            // The arrow's text stack makes it taller above the line than a bare shaft.
            let arrow = &layout.items[1];
            assert!(
                layout.center_line - arrow.rect.min.y > style.arrow_head_half_width,
                "the MeI label must sit above the shaft"
            );
            assert!(arrow.rect.max.y - layout.center_line > style.arrow_head_half_width);
            match &arrow.kind {
                PlacedKind::Arrow { above, below, length } => {
                    match above {
                        PlacedArrowLabel::Text(t) => assert_eq!(t.text(), "MeI"),
                        _ => panic!("MeI is text above the arrow"),
                    }
                    match below {
                        PlacedArrowLabel::Text(t) => {
                            assert_eq!(t.text(), "K2CO3", "subscripts flatten to plain digits")
                        }
                        _ => panic!("K2CO3 is text below the arrow"),
                    }
                    assert!(*length >= style.arrow_length);
                }
                _ => panic!("item 1 should be the arrow"),
            }
        });
    }

    /// A structure's box is the union of its atoms and labels, and the molecule origin is
    /// placed so the drawing lands inside that box.
    #[test]
    fn structure_bounds_cover_atoms_and_labels() {
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();
        with_painter(|painter| {
            let mut ed = scratch();
            let bounds = structure_bounds(&demo::phenol(), painter, &style, &sstyle, &mut ed);
            // A benzene ring plus OH is roughly two bond lengths across in each direction.
            assert!(bounds.width() > style.bond_length, "{bounds:?}");
            assert!(bounds.height() > style.bond_length, "{bounds:?}");

            // Every atom of the (scaled) molecule sits inside the bounds.
            for atom in &ed.molecule.atoms {
                let p = ed.mol_to_screen(atom.pos, Pos2::ZERO);
                assert!(bounds.contains(p), "atom {p:?} outside {bounds:?}");
            }
            // The hydroxyl label extends the box to the left of the ring's leftmost atom.
            let min_atom_x = ed
                .molecule
                .atoms
                .iter()
                .map(|a| ed.mol_to_screen(a.pos, Pos2::ZERO).x)
                .fold(f32::MAX, f32::min);
            assert!(bounds.min.x < min_atom_x, "the HO label widens the box");
        });
    }

    /// `+` separators and free text are ordinary scheme items.
    #[test]
    fn plus_and_text_items_measure() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .text("MeI")
            .arrow(Arrow::new())
            .structure(demo::anisole());
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(
                &scheme,
                painter,
                &ReactionStyle::default(),
                &StyleConfig::default(),
                &mut ed,
            );
            assert_eq!(layout.items.len(), 5);
            match &layout.items[1].kind {
                PlacedKind::Text(t) => assert_eq!(t.text(), "+"),
                _ => panic!("item 1 should be the plus"),
            }
            match &layout.items[2].kind {
                PlacedKind::Text(t) => assert_eq!(t.text(), "MeI"),
                _ => panic!("item 2 should be the text"),
            }
        });
    }

    /// Painting draws the arrow (shaft + filled head), its labels, and the `+` separators.
    #[test]
    fn painting_emits_arrow_and_text() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .text("MeI")
            .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
            .structure(demo::anisole());
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();

        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            ..Default::default()
        };
        let mut ed = scratch();
        let output = crate::testkit::run_ui(&ctx, raw, |ui| {
            let layout = measure_scheme(&scheme, ui.painter(), &style, &sstyle, &mut ed);
            paint_scheme(
                &scheme,
                &layout,
                ui.painter(),
                Pos2::new(20.0, 20.0),
                &style,
                &sstyle,
                &mut ed,
            );
        });

        let mut texts = Vec::new();
        let mut polygons = 0usize;
        fn walk(shape: &egui::Shape, texts: &mut Vec<String>, polygons: &mut usize) {
            match shape {
                egui::Shape::Text(t) => texts.push(t.galley.job.text.clone()),
                egui::Shape::Path(_) => *polygons += 1,
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, texts, polygons);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts, &mut polygons);
        }

        assert_eq!(polygons, 1, "exactly one filled arrow head");
        for expected in ["+", "MeI", "K", "2", "CO", "3", "OH"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "expected a '{expected}' galley among {texts:?}"
            );
        }
    }

    /// An arrow can carry a *structure* above (or below) its shaft, measured and placed as a
    /// molecule — the same object a main-line structure is — with the arrow widened to fit it.
    #[test]
    fn arrow_carries_a_structure_over_the_shaft() {
        let scheme = ReactionScheme::new()
            .structure(demo::benzene())
            .arrow(Arrow::new().above_structure(demo::biphenyl()).below("cat."))
            .structure(demo::benzene());
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(&scheme, painter, &style, &sstyle, &mut ed);
            let arrow = &layout.items[1];
            match &arrow.kind {
                PlacedKind::Arrow { above, below, length } => {
                    let PlacedArrowLabel::Structure { origin } = above else {
                        panic!("the biphenyl above the arrow is a structure");
                    };
                    assert!(matches!(below, PlacedArrowLabel::Text(_)), "'cat.' is text below");
                    // Biphenyl is wider than a bare arrow, so the shaft grew to fit it.
                    assert!(*length > style.arrow_length, "the arrow widened for the structure");
                    // The structure sits above the reaction line, clear of the shaft.
                    assert!(origin.y < layout.center_line);
                }
                _ => panic!("item 1 should be the arrow"),
            }
            // The arrow's box reaches a structure's height above the line.
            assert!(
                layout.center_line - arrow.rect.min.y > style.bond_length,
                "the box must contain the structure above the shaft"
            );
        });
    }

    /// Painting an arrow that carries a structure draws that structure's bonds, not just the
    /// shaft.
    #[test]
    fn painting_draws_an_arrow_structure() {
        let scheme = ReactionScheme::new()
            .structure(demo::benzene())
            .arrow(Arrow::new().above_structure(demo::biphenyl()))
            .structure(demo::benzene());
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            ..Default::default()
        };
        let mut ed = scratch();
        let output = crate::testkit::run_ui(&ctx, raw, |ui| {
            let layout = measure_scheme(&scheme, ui.painter(), &style, &sstyle, &mut ed);
            paint_scheme(&scheme, &layout, ui.painter(), Pos2::new(20.0, 20.0), &style, &sstyle, &mut ed);
        });
        let mut lines = 0usize;
        fn walk(shape: &egui::Shape, lines: &mut usize) {
            match shape {
                egui::Shape::LineSegment { .. } => *lines += 1,
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, lines)),
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut lines);
        }
        // Two main-line benzenes plus the biphenyl (two rings) above the arrow, and the shaft:
        // comfortably more bond lines than two rings alone would give.
        assert!(lines > 24, "expected the arrow structure's bonds to paint, got {lines}");
    }

    /// An empty scheme measures to nothing instead of panicking.
    #[test]
    fn empty_scheme_is_empty() {
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(
                &ReactionScheme::new(),
                painter,
                &ReactionStyle::default(),
                &StyleConfig::default(),
                &mut ed,
            );
            assert!(layout.items.is_empty());
            assert_eq!(layout.size, Vec2::ZERO);
        });
    }

    fn structure_ids(scheme: &ReactionScheme) -> Vec<StructureId> {
        let mut ids = Vec::new();
        for item in &scheme.items {
            match item {
                SchemeItem::Structure(s) => ids.push(s.id()),
                SchemeItem::Arrow(a) => {
                    for label in [&a.above, &a.below] {
                        if let ArrowLabel::Structure(s) = label {
                            ids.push(s.id());
                        }
                    }
                }
                _ => {}
            }
        }
        ids
    }

    #[test]
    fn builder_gives_every_structure_its_own_id() {
        // The same molecule twice still gets two ids: identity is not the molecule's content.
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::phenol())
            .arrow(
                Arrow::new()
                    .above_structure(demo::phenol())
                    .below_structure(demo::anisole()),
            )
            .structure(demo::anisole());
        let ids = structure_ids(&scheme);
        assert_eq!(ids.len(), 5);
        let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len(), "ids must be distinct: {ids:?}");
    }

    #[test]
    fn scheme_structure_derefs_to_and_compares_with_its_molecule() {
        let s = SchemeStructure::new(demo::phenol());
        let as_ref: &Molecule = &s;
        assert_eq!(*as_ref, demo::phenol());
        assert_eq!(s.molecule(), &demo::phenol());
        assert!(s == demo::phenol());
        assert!(s != demo::anisole());
        // A clone is the same structure (same id); a new wrapper is a different one.
        assert_eq!(s.clone().id(), s.id());
        assert_ne!(SchemeStructure::from(demo::phenol()).id(), s.id());
        assert_eq!(s.into_molecule(), demo::phenol());
    }

    #[test]
    fn editing_through_deref_mut_keeps_the_id() {
        let mut s = SchemeStructure::new(demo::phenol());
        let id = s.id();
        *s.molecule_mut() = demo::anisole();
        assert_eq!(s.id(), id);
        assert!(s == demo::anisole());
    }

    fn measure(scheme: &ReactionScheme, style: &ReactionStyle) -> SchemeLayout {
        let mut out = None;
        with_painter(|painter| {
            out = Some(measure_scheme(scheme, painter, style, &StyleConfig::default(), &mut scratch()));
        });
        out.unwrap()
    }

    /// The rows of a layout, as the items each holds.
    fn row_items(layout: &SchemeLayout) -> Vec<std::ops::Range<usize>> {
        layout.rows.iter().map(|r| r.items.clone()).collect()
    }

    #[test]
    fn a_line_break_starts_a_new_row_under_the_first() {
        let scheme = demo::multi_step_scheme().scheme;
        let layout = measure(&scheme, &ReactionStyle::default());
        assert_eq!(row_items(&layout), [0..6, 6..11], "the break ends the first row");
        let [first, second] = &layout.rows[..] else { panic!("two rows") };
        assert!(second.top >= first.bottom + ReactionStyle::default().row_gap - 0.01);
        assert_eq!(layout.center_line, first.center_line);
        assert!((layout.size.y - second.bottom).abs() < 0.01);
        for (i, item) in layout.items.iter().enumerate() {
            let row = &layout.rows[item.row];
            assert!(row.items.contains(&i));
            assert_eq!(layout.line_of(i), row.center_line);
            if !matches!(item.kind, PlacedKind::LineBreak) {
                assert!(item.rect.min.y <= row.center_line && item.rect.max.y >= row.center_line);
                assert!(item.rect.min.y >= row.top - 0.01 && item.rect.max.y <= row.bottom + 0.01);
            }
        }
        // Both rows start at the left edge.
        assert_eq!(layout.items[0].rect.min.x, 0.0);
        assert_eq!(layout.items[6].rect.min.x, 0.0);
        // The break sits at the end of its row, taking no room.
        let brk = &layout.items[5];
        assert_eq!(brk.rect.width(), 0.0);
        assert_eq!(brk.rect.min.x, layout.items[4].rect.max.x);
    }

    #[test]
    fn rows_wrap_at_the_widest_a_row_may_be() {
        let scheme = demo::williamson_ether_synthesis().scheme;
        let wide = measure(&scheme, &ReactionStyle::default());
        assert_eq!(wide.rows.len(), 1, "no limit, one row");
        // Room for the phenol and the arrow, not the anisole too.
        let limit = wide.items[1].rect.max.x + 1.0;
        let narrow = measure(&scheme, &ReactionStyle { max_row_width: Some(limit), ..Default::default() });
        assert_eq!(row_items(&narrow), [0..2, 2..3]);
        assert!(narrow.size.x <= limit);
        assert_eq!(narrow.items[2].rect.min.x, 0.0, "the wrapped item starts its row");
        // A limit narrower than any item still puts one item on each row.
        let tiny = measure(&scheme, &ReactionStyle { max_row_width: Some(1.0), ..Default::default() });
        assert_eq!(row_items(&tiny), [0..1, 1..2, 2..3]);
    }

    #[test]
    fn a_compound_label_is_centred_under_its_structure() {
        let plain = ReactionScheme::new().structure(demo::phenol());
        let labeled = ReactionScheme::new().labeled_structure(demo::phenol(), "(I)");
        let two_lines = ReactionScheme::new().labeled_structure(demo::phenol(), "**4**\n(V)");
        let style = ReactionStyle::default();
        let (plain, labeled, two_lines) = (measure(&plain, &style), measure(&labeled, &style), measure(&two_lines, &style));

        let PlacedKind::Structure { origin, label: None } = &plain.items[0].kind else { panic!("no label") };
        let PlacedKind::Structure { origin: o1, label: Some(l1) } = &labeled.items[0].kind else { panic!("a label") };
        let PlacedKind::Structure { label: Some(l2), .. } = &two_lines.items[0].kind else { panic!("a label") };
        // Same structure, same place on the line; the label hangs below it.
        assert_eq!(origin, o1);
        assert_eq!(plain.center_line, labeled.center_line);
        assert!(l1.rect.min.y >= plain.items[0].rect.max.y + style.compound_label_gap - 0.01);
        assert!((l1.rect.center().x - plain.items[0].rect.center().x).abs() < 0.5);
        assert!((labeled.items[0].rect.max.y - l1.rect.max.y).abs() < 0.01, "the item box takes the label in");
        // Two lines: one per line, stacked.
        assert_eq!(l2.lines.len(), 2);
        assert_eq!(l2.lines[0].text(), "4");
        assert_eq!(l2.lines[1].text(), "(V)");
        assert!(l2.rect.height() > l1.rect.height() * 1.5);
    }

    #[test]
    fn a_label_wider_than_its_structure_widens_the_item() {
        let scheme = ReactionScheme::new().labeled_structure(demo::methyl_iodide(), "a very long compound name");
        let layout = measure(&scheme, &ReactionStyle::default());
        let PlacedKind::Structure { label: Some(l), .. } = &layout.items[0].kind else { panic!("a label") };
        assert!((layout.items[0].rect.width() - l.rect.width()).abs() < 0.01);
        assert!(l.rect.min.x >= -0.01);
    }

    #[test]
    fn row_at_splits_the_gap_between_rows() {
        let layout = measure(&demo::multi_step_scheme().scheme, &ReactionStyle::default());
        let [first, second] = &layout.rows[..] else { panic!("two rows") };
        let mid = (first.bottom + second.top) * 0.5;
        assert_eq!(layout.row_at(-100.0), Some(0));
        assert_eq!(layout.row_at(mid - 1.0), Some(0));
        assert_eq!(layout.row_at(mid + 1.0), Some(1));
        assert_eq!(layout.row_at(1e6), Some(1));
        assert_eq!(measure(&ReactionScheme::new(), &ReactionStyle::default()).row_at(0.0), None);
    }
}
