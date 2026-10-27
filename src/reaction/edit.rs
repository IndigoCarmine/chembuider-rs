//! Editing a [`Reaction`] from the UI: addressing the drawn figure, parsing typed
//! quantities, and the [`ReactionEditor`] widget that ties the two together.
//!
//! [`ReactionView`] stays a pure renderer — [`ReactionView::paint`] still draws a figure with
//! no editing chrome whatsoever, which is what the headless `--render` path uses. Everything
//! interactive lives here, one layer up:
//!
//! * **addressing** — [`CellRef`] names a table cell, [`SchemePart`] a piece of a scheme item.
//!   [`cell_rect`] and [`scheme_item_rect`] turn those into screen rects off a measured
//!   [`FigureLayout`], and [`hit_test`] goes the other way. All four are plain functions over
//!   the layout, so the geometry can be tested without a window.
//! * **what is editable** — [`field_at`] maps a table row to the [`Field`] it edits. Only the
//!   rows that can become [`Origin::Entered`](crate::reaction::Origin) have one; every
//!   derived row is read-only, and re-deriving happens by itself because the table is rebuilt
//!   from [`Analysis::compute`](crate::reaction::Analysis) every frame.
//! * **typed values** — [`parse_value`] reads `1.00g`, `700.00mg`, `10.63mmol`-style text
//!   back into base units, and blank text clears the entry.
//!
//! ```no_run
//! use chembuider_rs::reaction::{ReactionEditor, demo};
//!
//! struct App { editor: ReactionEditor, reaction: chembuider_rs::reaction::Reaction }
//! impl App {
//!     fn ui(&mut self, ui: &mut chembuider_rs::egui::Ui) {
//!         // Click a cell to type into it, click `Limiting?` to move the limiting reagent.
//!         self.editor.show(ui, &mut self.reaction);
//!     }
//! }
//! ```

use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind};

use crate::molecule::Molecule;
use crate::widget::rich_text::RichText;

use super::model::{Analysis, Mass, Reaction};
use super::scheme::{ArrowLabel, PlacedKind, ReactionScheme, SchemeItem, SchemeStructure};
use super::table::BlockKind;
use super::view::{FigureLayout, ReactionView};

// ─── Addressing ──────────────────────────────────────────────────────────────

/// One value cell of the stoichiometry table: which block, which species column (0-based —
/// the row-label column is not addressable), which row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRef {
    pub block: BlockKind,
    pub column: usize,
    pub row: usize,
}

impl CellRef {
    pub const fn new(block: BlockKind, column: usize, row: usize) -> Self {
        Self { block, column, row }
    }

    /// The input this cell edits, or `None` for a computed (read-only) cell.
    pub fn field(self) -> Option<Field> {
        field_at(self.block, self.row)
    }
}

/// Which part of a scheme item a point fell on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemePart {
    /// The drawn structure.
    Structure,
    /// Reagents above an arrow's shaft.
    ArrowAbove,
    /// Conditions below an arrow's shaft.
    ArrowBelow,
    /// A free-standing text item.
    Text,
    /// A `+` separator — selectable (so it can be deleted) but not editable.
    Plus,
}

impl SchemePart {
    /// True for the parts that carry a [`RichText`] the user can type into.
    pub const fn is_text(self) -> bool {
        matches!(self, Self::ArrowAbove | Self::ArrowBelow | Self::Text)
    }
}

/// What a point in the figure lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A table cell. Ask [`CellRef::field`] whether it can be typed into.
    Cell(CellRef),
    /// A scheme item, and the part of it under the point.
    Scheme { index: usize, part: SchemePart },
}

/// Half-height of an arrow's two clickable bands. An arrow carrying no text is only a few
/// pixels tall, and an *empty* label still needs a target — that is how text gets added to a
/// bare arrow in the first place.
pub const ARROW_HIT_HALF_HEIGHT: f32 = 11.0;

// ─── Editable fields ─────────────────────────────────────────────────────────

/// An input of the analysis — the cells that can become
/// [`Origin::Entered`](crate::reaction::Origin). Everything else in the table is derived and
/// read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    /// `Limiting?` — a toggle rather than a typed value, see [`Analysis::set_limiting`].
    Limiting,
    /// `Equivalents`, in either block.
    Equivalents,
    SampleMass,
    PercentWeight,
    Molarity,
    Density,
    Volume,
    PercentCompletion,
    MeasuredMass,
    Purity,
}

impl Field {
    /// The unit family this field is entered in — what [`parse_value`] will accept after the
    /// number, and the base unit the parsed value comes back in.
    pub const fn unit(self) -> Unit {
        match self {
            Self::Limiting => Unit::Number,
            Self::Equivalents => Unit::Number,
            Self::SampleMass | Self::MeasuredMass => Unit::Mass,
            Self::PercentWeight | Self::PercentCompletion | Self::Purity => Unit::Percent,
            Self::Molarity => Unit::Molarity,
            Self::Density => Unit::Density,
            Self::Volume => Unit::Volume,
        }
    }

    /// True for the fields an edit box can be opened on. `Limiting?` is a toggle, so it is
    /// the one field that is editable without being typeable.
    pub const fn is_typed(self) -> bool {
        !matches!(self, Self::Limiting)
    }
}

/// The field a table row edits, by row index within its block. Rows with no entry of their
/// own (`Formula`, `MW`, `Reactant Moles`, `%Yield`, …) return `None`.
///
/// The indices follow [`REACTANT_ROWS`](crate::reaction::REACTANT_ROWS) and
/// [`PRODUCT_ROWS`](crate::reaction::PRODUCT_ROWS); `rows_and_fields_agree` pins them to the
/// row labels so the two lists cannot drift apart.
pub const fn field_at(block: BlockKind, row: usize) -> Option<Field> {
    match block {
        BlockKind::Reactants => match row {
            2 => Some(Field::Limiting),
            3 => Some(Field::Equivalents),
            4 => Some(Field::SampleMass),
            5 => Some(Field::PercentWeight),
            6 => Some(Field::Molarity),
            7 => Some(Field::Density),
            8 => Some(Field::Volume),
            _ => None,
        },
        BlockKind::Products => match row {
            2 => Some(Field::Equivalents),
            3 => Some(Field::PercentCompletion),
            6 => Some(Field::MeasuredMass),
            7 => Some(Field::Purity),
            _ => None,
        },
    }
}

// ─── Typed values ────────────────────────────────────────────────────────────

/// The unit family of a field: which suffixes [`parse_value`] accepts, and the base unit it
/// returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Grams. `g` (default), `mg`, `kg`, `µg`/`ug`.
    Mass,
    /// Millilitres. `mL` (default), `L`, `µL`/`uL`.
    Volume,
    /// Percent. A trailing `%` is optional.
    Percent,
    /// mol/L. A trailing `M` is optional; `mM` and `µM` scale.
    Molarity,
    /// g/mL. A trailing `g/mL` (or `g/cm3`) is optional.
    Density,
    /// Dimensionless (equivalents, coefficients). A trailing `eq` is tolerated.
    Number,
}

/// Why a typed cell value was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The text does not even start with a number.
    NotANumber,
    /// A number, followed by a unit this field does not take.
    UnknownUnit(String),
    /// A number outside the range the quantity can physically take.
    OutOfRange,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotANumber => write!(f, "not a number"),
            Self::UnknownUnit(u) => write!(f, "unknown unit '{u}'"),
            Self::OutOfRange => write!(f, "value out of range"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse a typed cell value into base units (g, mL, %, mol/L, g/mL, dimensionless).
///
/// The accepted spelling is exactly what the table *prints* — `1.00g`, `700.00mg`, `2.28g/mL`
/// — so a value can be read off the figure and typed straight back in. A unit-less number is
/// read in the field's base unit.
///
/// Blank input is not an error: it clears the entry (`Ok(None)`), which sends the cell back
/// to whatever the stoichiometry computes for it.
pub fn parse_value(unit: Unit, text: &str) -> Result<Option<f64>, ParseError> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }

    // Longest numeric prefix, so `1e-3g` reads as one milligram while `1eq` reads as a bare
    // 1 — scanning forward for the first non-digit would split both in the wrong place.
    let mut split = None;
    for i in (1..=text.len()).rev() {
        if !text.is_char_boundary(i) {
            continue;
        }
        if let Ok(v) = text[..i].parse::<f64>() {
            split = Some((v, text[i..].trim()));
            break;
        }
    }
    let Some((number, suffix)) = split else {
        return Err(ParseError::NotANumber);
    };

    // `µ` is not ASCII, so it survives the fold and both spellings can be matched.
    let key = suffix.to_ascii_lowercase();
    let scale = match (unit, key.as_str()) {
        (Unit::Mass, "" | "g") => 1.0,
        (Unit::Mass, "mg") => 1e-3,
        (Unit::Mass, "kg") => 1e3,
        (Unit::Mass, "µg" | "ug") => 1e-6,
        (Unit::Volume, "" | "ml") => 1.0,
        (Unit::Volume, "l") => 1e3,
        (Unit::Volume, "µl" | "ul") => 1e-3,
        (Unit::Percent, "" | "%") => 1.0,
        (Unit::Molarity, "" | "m" | "mol/l") => 1.0,
        (Unit::Molarity, "mm") => 1e-3,
        (Unit::Molarity, "µm" | "um") => 1e-6,
        (Unit::Density, "" | "g/ml" | "g/cm3" | "g/cm^3") => 1.0,
        (Unit::Number, "" | "eq" | "equiv") => 1.0,
        _ => return Err(ParseError::UnknownUnit(suffix.to_string())),
    };

    let value = number * scale;
    // `"inf"` and `"nan"` parse as f64, and no amount can be negative.
    if !value.is_finite() || value < 0.0 {
        return Err(ParseError::OutOfRange);
    }
    // A zero here would be a silent divide-by-zero in the stoichiometry rather than an amount.
    if value == 0.0 && matches!(unit, Unit::Density | Unit::Molarity) {
        return Err(ParseError::OutOfRange);
    }
    Ok(Some(value))
}

/// Write a parsed value into the analysis; `None` clears the entry.
///
/// Returns `false` when the cell is out of range or read-only — including `Limiting?`, which
/// is a toggle and goes through [`Analysis::set_limiting`] instead.
pub fn set_field(analysis: &mut Analysis, cell: CellRef, value: Option<f64>) -> bool {
    let Some(field) = cell.field() else {
        return false;
    };
    match cell.block {
        BlockKind::Reactants => {
            let Some(r) = analysis.reactants.get_mut(cell.column) else {
                return false;
            };
            match field {
                Field::Equivalents => r.equivalents = value,
                Field::SampleMass => r.sample_mass = value.map(Mass::grams),
                Field::PercentWeight => r.percent_weight = value,
                Field::Molarity => r.molarity = value,
                Field::Density => r.density = value,
                Field::Volume => r.volume = value,
                _ => return false,
            }
        }
        BlockKind::Products => {
            let Some(p) = analysis.products.get_mut(cell.column) else {
                return false;
            };
            match field {
                Field::Equivalents => p.equivalents = value,
                Field::PercentCompletion => p.percent_completion = value,
                Field::MeasuredMass => p.measured_mass = value.map(Mass::grams),
                Field::Purity => p.purity = value,
                _ => return false,
            }
        }
    }
    true
}

/// The *entered* value of a cell, as text [`parse_value`] reads back to exactly the same
/// number (`1.2345g`, `0.004`) — the text an edit box starts with.
///
/// The number is written in the field's base unit with Rust's shortest round-trip `f64`
/// spelling, not with the table's two-decimal display formatting: the box is the entry
/// itself, and seeding it with a rounded figure would overwrite `0.004` eq with `0` the
/// moment the box is committed untouched.
///
/// Empty when nothing was entered there, even if the cell currently shows a computed number:
/// the box starts blank so that opening it and pressing Enter changes nothing, and the
/// computed value is offered as hint text instead of being frozen into an entry by accident.
pub fn field_text(analysis: &Analysis, cell: CellRef) -> String {
    let Some(field) = cell.field() else {
        return String::new();
    };
    // Scale 1 in `parse_value`, so the round trip is exact.
    let suffix = match field.unit() {
        Unit::Mass => "g",
        Unit::Volume => "mL",
        Unit::Percent => "%",
        Unit::Molarity => "M",
        Unit::Density => "g/mL",
        Unit::Number => "",
    };
    let exact = |q: Option<f64>| q.map(|v| format!("{v}{suffix}")).unwrap_or_default();
    match cell.block {
        BlockKind::Reactants => {
            let Some(r) = analysis.reactants.get(cell.column) else {
                return String::new();
            };
            match field {
                Field::Limiting => if r.limiting { "Yes" } else { "No" }.to_string(),
                Field::Equivalents => exact(r.equivalents),
                Field::SampleMass => exact(r.sample_mass.map(Mass::as_grams)),
                Field::PercentWeight => exact(r.percent_weight),
                Field::Molarity => exact(r.molarity),
                Field::Density => exact(r.density),
                Field::Volume => exact(r.volume),
                _ => String::new(),
            }
        }
        BlockKind::Products => {
            let Some(p) = analysis.products.get(cell.column) else {
                return String::new();
            };
            match field {
                Field::Equivalents => exact(p.equivalents),
                Field::PercentCompletion => exact(p.percent_completion),
                Field::MeasuredMass => exact(p.measured_mass.map(Mass::as_grams)),
                Field::Purity => exact(p.purity),
                _ => String::new(),
            }
        }
    }
}

/// The markup of a scheme item's text — the text an edit box starts with. Empty for parts
/// that carry no text (a structure, a `+`, an arrow label that has not been written yet).
pub fn scheme_text(scheme: &ReactionScheme, index: usize, part: SchemePart) -> String {
    let text_markup = |l: &ArrowLabel| l.as_text().map(|t| t.to_markup()).unwrap_or_default();
    match (scheme.items.get(index), part) {
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowAbove) => text_markup(&a.above),
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowBelow) => text_markup(&a.below),
        (Some(SchemeItem::Text(t)), SchemePart::Text) => t.to_markup(),
        _ => String::new(),
    }
}

/// The structure drawn on an arrow's above/below label, if that label holds one.
pub fn scheme_arrow_structure(
    scheme: &ReactionScheme,
    index: usize,
    part: SchemePart,
) -> Option<&Molecule> {
    match (scheme.items.get(index), part) {
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowAbove) => a.above.as_structure(),
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowBelow) => a.below.as_structure(),
        _ => None,
    }
}

/// Draw `molecule` on an arrow's above/below label. Returns `false` (changing nothing) when
/// the item is not an arrow or the part is not an arrow band.
///
/// When the band already holds a structure, this redraws *that* structure: it keeps its
/// [`StructureId`](super::scheme::StructureId), the same as editing it in place. Text or
/// nothing on the band is replaced by a new structure with a fresh id. To put an existing
/// [`SchemeStructure`] (id and all) on a band, use [`place_scheme_arrow_structure`].
pub fn set_scheme_arrow_structure(
    scheme: &mut ReactionScheme,
    index: usize,
    part: SchemePart,
    molecule: Molecule,
) -> bool {
    let Some(slot) = arrow_label_mut(scheme, index, part) else {
        return false;
    };
    match slot {
        ArrowLabel::Structure(s) => *s.molecule_mut() = molecule,
        other => *other = ArrowLabel::Structure(SchemeStructure::new(molecule)),
    }
    true
}

/// Put `structure` on an arrow's above/below label as it is, keeping its
/// [`StructureId`](super::scheme::StructureId), and replacing whatever the band held. Returns
/// `false` (changing nothing, and dropping `structure`) when the item is not an arrow or the
/// part is not an arrow band — check the target first when the structure must not be lost.
///
/// This is how a structure *moves* onto a band (from the line, or from another band), as
/// opposed to [`set_scheme_arrow_structure`], which draws a molecule there.
pub fn place_scheme_arrow_structure(
    scheme: &mut ReactionScheme,
    index: usize,
    part: SchemePart,
    structure: SchemeStructure,
) -> bool {
    let Some(slot) = arrow_label_mut(scheme, index, part) else {
        return false;
    };
    *slot = ArrowLabel::Structure(structure);
    true
}

/// Clear an arrow's above/below label back to empty, returning the structure it held (if any),
/// with its [`StructureId`](super::scheme::StructureId). A text label is cleared too but comes
/// back as `None`.
pub fn take_scheme_arrow_structure(
    scheme: &mut ReactionScheme,
    index: usize,
    part: SchemePart,
) -> Option<SchemeStructure> {
    let slot = arrow_label_mut(scheme, index, part)?;
    match std::mem::replace(slot, ArrowLabel::Empty) {
        ArrowLabel::Structure(s) => Some(s),
        // Text (or nothing) leaves the slot empty and reports no structure taken.
        _ => None,
    }
}

/// Move the main-line structure at `from` onto an arrow's above/below slot, taking it off the
/// reaction line. The target slot must be empty and the two items must be a structure and an
/// arrow; otherwise nothing changes and `None` comes back. On success the arrow's (possibly
/// shifted, since a line item was removed) index is returned.
///
/// The caller is expected to follow up with
/// [`sync_structures_from_scheme`](Reaction::sync_structures_from_scheme) so the table columns
/// follow the moved structure (by its id) back into scheme order.
pub fn move_line_structure_to_arrow_slot(
    scheme: &mut ReactionScheme,
    from: usize,
    arrow_index: usize,
    part: SchemePart,
) -> Option<usize> {
    match arrow_label(scheme, arrow_index, part) {
        None => return None, // not an arrow band: nowhere to put the structure
        Some(ArrowLabel::Structure(_)) => return None, // don't clobber a structure already there
        Some(_) => {}
    }
    if !matches!(scheme.items.get(from), Some(SchemeItem::Structure(_))) {
        return None;
    }
    // Both ends were checked above, so the structure always lands: it moves whole, id included.
    let Some(SchemeItem::Structure(structure)) = scheme.remove(from) else {
        unreachable!("item {from} was checked to be a structure");
    };
    let target = if from < arrow_index { arrow_index - 1 } else { arrow_index };
    place_scheme_arrow_structure(scheme, target, part, structure).then_some(target)
}

/// Swap a main-line structure with a structure already riding on an arrow slot, losslessly:
/// the arrow slot ends up holding the (former) main-line structure and the main-line position
/// holds what the slot had. Neither leaves the figure, so this is the *occupied*-slot
/// counterpart of [`move_line_structure_to_arrow_slot`] (which only accepts an empty slot).
///
/// Returns `false` (changing nothing) unless `from` is a main-line structure **and** the slot
/// holds a structure; a text- or empty-occupied slot is not swappable. The caller should follow
/// up with [`sync_structures_from_scheme`](Reaction::sync_structures_from_scheme) so the
/// columns, which follow their structures by id, are put back in scheme order.
pub fn swap_line_structure_with_arrow_slot(
    scheme: &mut ReactionScheme,
    from: usize,
    arrow_index: usize,
    part: SchemePart,
) -> bool {
    if scheme_arrow_structure(scheme, arrow_index, part).is_none() {
        return false; // only a structure-occupied slot is swappable
    }
    if !matches!(scheme.items.get(from), Some(SchemeItem::Structure(_))) {
        return false;
    }
    // Trade the two structures whole, so each keeps its id at its new place.
    let Some(mut moving) = take_scheme_arrow_structure(scheme, arrow_index, part) else {
        unreachable!("the slot was checked to hold a structure");
    };
    if let Some(SchemeItem::Structure(line)) = scheme.items.get_mut(from) {
        std::mem::swap(line, &mut moving);
    }
    place_scheme_arrow_structure(scheme, arrow_index, part, moving)
}

/// Move the structure on an arrow band back onto the reaction line: insert it as a new
/// [`SchemeItem::Structure`] at `insert_at` (a slot in `0..=items.len()`, as
/// [`scheme_drop_index`] reports) and clear the band to [`ArrowLabel::Empty`]. Returns the
/// index the structure landed at, or `None` when the band carries no structure.
///
/// Nothing is removed, so `insert_at` is used directly. The caller should follow up with
/// [`sync_structures_from_scheme`](Reaction::sync_structures_from_scheme) so the columns
/// re-derive around the structure that rejoined the line.
pub fn move_arrow_structure_to_line(
    scheme: &mut ReactionScheme,
    index: usize,
    part: SchemePart,
    insert_at: usize,
) -> Option<usize> {
    let structure = take_scheme_arrow_structure(scheme, index, part)?;
    let at = insert_at.min(scheme.items.len());
    scheme.items.insert(at, SchemeItem::Structure(structure));
    Some(at)
}

/// The arrow band's label, mutably, when the item is an arrow and the part names a band.
fn arrow_label_mut(
    scheme: &mut ReactionScheme,
    index: usize,
    part: SchemePart,
) -> Option<&mut ArrowLabel> {
    match (scheme.items.get_mut(index), part) {
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowAbove) => Some(&mut a.above),
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowBelow) => Some(&mut a.below),
        _ => None,
    }
}

/// The arrow band's label, when the item is an arrow and the part names a band.
fn arrow_label(scheme: &ReactionScheme, index: usize, part: SchemePart) -> Option<&ArrowLabel> {
    match (scheme.items.get(index), part) {
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowAbove) => Some(&a.above),
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowBelow) => Some(&a.below),
        _ => None,
    }
}

/// Whether an arrow band currently holds *text* — a target a dragged structure must not
/// clobber, so the drag routing steers clear of it (a line structure dropped there reorders
/// along the line instead).
fn arrow_slot_holds_text(scheme: &ReactionScheme, index: usize, part: SchemePart) -> bool {
    arrow_label(scheme, index, part).is_some_and(|l| l.as_text().is_some())
}

/// Whether an arrow's above/below slot currently holds a *structure* (rather than text or
/// nothing) — the parts a text edit box must refuse to open on.
fn scheme_part_holds_structure(scheme: &ReactionScheme, index: usize, part: SchemePart) -> bool {
    scheme_arrow_structure(scheme, index, part).is_some()
}

/// Write markup back into a scheme item's text. Returns `false` when the part carries no
/// text. The markup goes through [`RichText::parse`], so `K_2CO_3` gets real subscripts.
pub fn set_scheme_text(
    scheme: &mut ReactionScheme,
    index: usize,
    part: SchemePart,
    markup: &str,
) -> bool {
    match (scheme.items.get_mut(index), part) {
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowAbove) => {
            a.above = ArrowLabel::from(RichText::parse(markup))
        }
        (Some(SchemeItem::Arrow(a)), SchemePart::ArrowBelow) => {
            a.below = ArrowLabel::from(RichText::parse(markup))
        }
        (Some(SchemeItem::Text(t)), SchemePart::Text) => *t = RichText::parse(markup),
        _ => return false,
    }
    true
}

/// Run the automatic 2-D layout cleanup ([`crate::molecule::cleanup`]) on the structure at
/// `index`, rewriting its atom coordinates in place. Returns `false` when the item at that
/// index is not a structure (or the index is out of range), leaving the scheme untouched.
///
/// This is the synchronous, one-shot relaxation — fine for the small structures a scheme
/// carries. The interactive canvas editor streams the same computation off a background
/// thread instead (see [`ChemStructEditor::toggle_cleanup`](crate::widget::ChemStructEditor));
/// a scheme structure is redrawn from scratch every frame, so it just takes the final result.
pub fn cleanup_scheme_structure(scheme: &mut ReactionScheme, index: usize) -> bool {
    match scheme.items.get_mut(index) {
        Some(SchemeItem::Structure(mol)) => {
            crate::molecule::cleanup::cleanup_2d(mol);
            true
        }
        _ => false,
    }
}

// ─── Geometry ────────────────────────────────────────────────────────────────

/// Screen rect of a table cell, for a figure whose top-left corner is at `top_left`.
/// `None` when the block is absent or the row/column is out of range.
pub fn cell_rect(layout: &FigureLayout, top_left: Pos2, cell: CellRef) -> Option<Rect> {
    let i = layout.table.block_index(cell.block)?;
    let block = layout.table_layout.blocks.get(i)?;
    if cell.row >= layout.table.blocks[i].row_labels.len() {
        return None;
    }
    // Column 0 of the layout is the row labels, so a species column is one further along.
    let x0 = *block.column_x.get(cell.column + 1)?;
    let x1 = *block.column_x.get(cell.column + 2)?;
    let frame = block.frame.translate((top_left + layout.table_offset).to_vec2());
    let y0 = frame.min.y + cell.row as f32 * block.row_height;
    Some(Rect::from_min_max(
        Pos2::new(frame.min.x + x0, y0),
        Pos2::new(frame.min.x + x1, y0 + block.row_height),
    ))
}

/// Screen rect of one part of a scheme item, for a figure whose top-left corner is at
/// `top_left`.
///
/// An arrow's two text bands are the halves of its box split at the reaction line, each grown
/// to at least [`ARROW_HIT_HALF_HEIGHT`]; every other part is the item's own box.
pub fn scheme_item_rect(
    layout: &FigureLayout,
    top_left: Pos2,
    index: usize,
    part: SchemePart,
) -> Option<Rect> {
    let origin = top_left + layout.scheme_offset;
    let placed = layout.scheme.items.get(index)?;
    let rect = placed.rect.translate(origin.to_vec2());
    let line_y = origin.y + layout.scheme.center_line;
    Some(match part {
        SchemePart::ArrowAbove => Rect::from_min_max(
            Pos2::new(rect.min.x, rect.min.y.min(line_y - ARROW_HIT_HALF_HEIGHT)),
            Pos2::new(rect.max.x, line_y),
        ),
        SchemePart::ArrowBelow => Rect::from_min_max(
            Pos2::new(rect.min.x, line_y),
            Pos2::new(rect.max.x, rect.max.y.max(line_y + ARROW_HIT_HALF_HEIGHT)),
        ),
        _ => rect,
    })
}

/// Where a scheme item dragged to `pointer` would drop: the number of items whose horizontal
/// centre sits left of the pointer, i.e. an insertion slot in `0..=items.len()`. Independent
/// of which item is being dragged, so the caller converts it to a final index itself.
pub fn scheme_drop_index(layout: &FigureLayout, top_left: Pos2, pointer: Pos2) -> usize {
    let origin = top_left + layout.scheme_offset;
    layout
        .scheme
        .items
        .iter()
        .filter(|placed| placed.rect.translate(origin.to_vec2()).center().x < pointer.x)
        .count()
}

/// The arrow above/below band under `pointer`, if any — a drop slot for a dragged structure.
pub fn arrow_slot_at(
    scheme: &ReactionScheme,
    layout: &FigureLayout,
    top_left: Pos2,
    pointer: Pos2,
) -> Option<(usize, SchemePart)> {
    match hit_test(scheme, layout, top_left, pointer) {
        Some(Hit::Scheme { index, part })
            if matches!(part, SchemePart::ArrowAbove | SchemePart::ArrowBelow) =>
        {
            Some((index, part))
        }
        _ => None,
    }
}

/// A vertical caret marking where a dragged item will drop, spanning the scheme band.
fn draw_drop_caret(
    painter: &egui::Painter,
    layout: &FigureLayout,
    top_left: Pos2,
    insert_at: usize,
    color: Color32,
) {
    let origin = top_left + layout.scheme_offset;
    let scheme = &layout.scheme;
    let Some(last) = scheme.items.last() else { return };
    let x = match scheme.items.get(insert_at) {
        Some(placed) => placed.rect.translate(origin.to_vec2()).min.x,
        None => last.rect.translate(origin.to_vec2()).max.x,
    };
    painter.line_segment(
        [Pos2::new(x, origin.y), Pos2::new(x, origin.y + scheme.size.y)],
        Stroke::new(2.5, color),
    );
}

/// The parts of a scheme item that [`hit_test`] can answer with, in the order it tries them.
/// The first one is what selecting the item as a whole names.
fn scheme_item_parts(item: &SchemeItem) -> &'static [SchemePart] {
    match item {
        SchemeItem::Structure(_) => &[SchemePart::Structure],
        SchemeItem::Plus => &[SchemePart::Plus],
        SchemeItem::Arrow(_) => &[SchemePart::ArrowAbove, SchemePart::ArrowBelow],
        SchemeItem::Text(_) => &[SchemePart::Text],
    }
}

/// What is under `pos` in a figure drawn with its top-left corner at `top_left`.
///
/// The scheme is tested before the table; the two halves are stacked and never overlap, so
/// the order only settles which one claims the gap between them (neither does).
pub fn hit_test(
    scheme: &ReactionScheme,
    layout: &FigureLayout,
    top_left: Pos2,
    pos: Pos2,
) -> Option<Hit> {
    for (index, placed) in layout.scheme.items.iter().enumerate() {
        let parts = match scheme.items.get(index) {
            Some(item)
                if matches!(placed.kind, PlacedKind::Arrow { .. })
                    == matches!(item, SchemeItem::Arrow(_)) =>
            {
                scheme_item_parts(item)
            }
            // Layout and scheme are built from the same list; a mismatch means a stale
            // layout, and hitting nothing is better than answering with the wrong item.
            _ => &[],
        };
        for &part in parts {
            if scheme_item_rect(layout, top_left, index, part).is_some_and(|r| r.contains(pos)) {
                return Some(Hit::Scheme { index, part });
            }
        }
    }

    let origin = top_left + layout.table_offset;
    for (i, block) in layout.table_layout.blocks.iter().enumerate() {
        let frame = block.frame.translate(origin.to_vec2());
        if !frame.contains(pos) || block.row_height <= 0.0 {
            continue;
        }
        let kind = layout.table.blocks[i].kind;
        let row = ((pos.y - frame.min.y) / block.row_height).floor() as usize;
        if row >= layout.table.blocks[i].row_labels.len() {
            continue;
        }
        let x = pos.x - frame.min.x;
        // `column_x` is the left edge of every column plus the closing edge; column 0 is the
        // row labels, which nothing can be typed into.
        let column = block.column_x.windows(2).position(|w| x >= w[0] && x < w[1])?;
        if column == 0 {
            return None;
        }
        return Some(Hit::Cell(CellRef::new(kind, column - 1, row)));
    }
    None
}

// ─── The widget ──────────────────────────────────────────────────────────────

/// What the edit box is open on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTarget {
    Cell(CellRef),
    SchemeText { index: usize, part: SchemePart },
}

/// A drawn structure in the scheme the host can be asked to open in a canvas editor — either a
/// main-line structure item or one riding on an arrow's above/below label. This is what a
/// double-click leaves for the host to pick up with [`ReactionEditor::take_edit_request`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureRef {
    /// A main-line [`SchemeItem::Structure`] at this scheme index.
    Item(usize),
    /// A structure riding on an arrow's above/below label.
    ArrowLabel { index: usize, part: SchemePart },
}

/// The open edit box: what it edits and what has been typed into it so far.
struct Editing {
    target: EditTarget,
    text: String,
}

/// What a drag picked up.
#[derive(Clone, Copy)]
enum DragSource {
    /// A main-line scheme item at this index (reorders along the line).
    Item(usize),
    /// A structure riding on an arrow's above/below label (moves between slots).
    ArrowLabel { index: usize, part: SchemePart },
}

impl DragSource {
    /// True when this source *is* the given arrow slot, so the pointer resting on its own slot
    /// is not treated as a drop target.
    fn is_slot(self, index: usize, part: SchemePart) -> bool {
        matches!(self, Self::ArrowLabel { index: i, part: p } if i == index && p == part)
    }

    /// True when what was picked up is a structure — the only thing an arrow slot can take.
    /// A `+`, a text item or an arrow (including one picked up by its own empty band) is not,
    /// so passing over a band leaves it reordering along the line.
    fn carries_structure(self, scheme: &ReactionScheme) -> bool {
        match self {
            Self::Item(i) => matches!(scheme.items.get(i), Some(SchemeItem::Structure(_))),
            Self::ArrowLabel { index, part } => scheme_arrow_structure(scheme, index, part).is_some(),
        }
    }
}

/// A scheme item being dragged: either reordered along the line, or dropped onto an arrow slot.
#[derive(Clone, Copy)]
struct DragReorder {
    /// What was picked up.
    source: DragSource,
    /// Reorder slot for a main-line drag, and the insertion slot an arrow-label structure would
    /// rejoin the line at: the number of items that will sit to its left (`0..=items.len()`).
    insert_at: usize,
    /// The arrow above/below slot under the pointer, when the drop would land on one. Only while
    /// a structure is being dragged, only slots that can *accept* it — empty or
    /// structure-occupied, never text — and never the source's own slot.
    to_slot: Option<(usize, SchemePart)>,
    /// Whether the pointer is over any arrow band. For an arrow-label source this is what tells
    /// "resting on a band" (its own, or a text one — do nothing) apart from "over the reaction
    /// line" (drop the structure back onto the line).
    over_band: bool,
}

/// Interactive front end for [`ReactionView`]: the same figure, plus click-to-edit.
///
/// Hold one between frames — it owns the view, which owns the structure drawing context.
/// Adding, deleting and reordering columns and scheme items are ordinary edits of the
/// [`Reaction`] ([`Analysis::add_reactant`], [`ReactionScheme::remove`], …); this widget
/// covers the parts that need the drawn figure to point at.
pub struct ReactionEditor {
    /// The figure renderer. Style knobs live on it.
    pub view: ReactionView,
    /// Outline colour for the hovered and selected cell.
    pub highlight: Color32,
    edit: Option<Editing>,
    selection: Option<Hit>,
    error: Option<String>,
    /// Set when a scheme structure is double-clicked: the host reads it with
    /// [`take_edit_request`](Self::take_edit_request) and opens that structure in a molecule
    /// editor of its own. Names either a main-line structure or one on an arrow label.
    edit_request: Option<StructureRef>,
    /// What the pointer was over when the context menu (secondary click) opened; the host
    /// reads it with [`context_target`](Self::context_target) to build the menu.
    context_target: Option<Hit>,
    /// A scheme item being dragged to reorder, live for the duration of the drag.
    drag: Option<DragReorder>,
    /// Set when a drag moved scheme structures to other positions; read and cleared by the host
    /// with [`take_scheme_rearranged`](Self::take_scheme_rearranged).
    scheme_rearranged: bool,
}

impl Default for ReactionEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactionEditor {
    pub fn new() -> Self {
        Self {
            view: ReactionView::new(),
            highlight: Color32::from_rgb(60, 120, 220),
            edit: None,
            selection: None,
            error: None,
            edit_request: None,
            context_target: None,
            drag: None,
            scheme_rearranged: false,
        }
    }

    /// What was clicked last — the column or scheme item a host command such as "replace with
    /// the structure on the canvas" applies to.
    pub fn selection(&self) -> Option<Hit> {
        self.selection
    }

    pub fn select(&mut self, hit: Option<Hit>) {
        self.selection = hit;
    }

    /// Take the pending "edit this structure" request, set when a scheme structure is
    /// double-clicked. The host loads that structure — [`StructureRef::Item`] reads
    /// `reaction.scheme.items[index]`, [`StructureRef::ArrowLabel`] reads it with
    /// [`scheme_arrow_structure`] — into a [`ChemStructEditor`](crate::widget::ChemStructEditor)
    /// and, once edited, writes it back through the selection this same double-click leaves.
    pub fn take_edit_request(&mut self) -> Option<StructureRef> {
        self.edit_request.take()
    }

    /// What the pointer was over when the context menu was opened (secondary click). Read this
    /// inside a [`Response::context_menu`](egui::Response::context_menu) closure to decide what
    /// the menu offers.
    pub fn context_target(&self) -> Option<Hit> {
        self.context_target
    }

    /// Forget the context-menu target (e.g. after the menu closes).
    pub fn clear_context_target(&mut self) {
        self.context_target = None;
    }

    /// True while a scheme item is being dragged to reorder it.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Take whether [`show`](Self::show) has moved scheme structures to other positions since
    /// the last call: a drag that reordered the line, moved a structure onto or off an arrow
    /// slot, or swapped two of them. The host must re-check anything it holds by scheme
    /// position — a [`StructureRef`] from [`take_edit_request`](Self::take_edit_request) kept
    /// as a canvas link, say — since it may now name a different structure.
    pub fn take_scheme_rearranged(&mut self) -> bool {
        std::mem::take(&mut self.scheme_rearranged)
    }

    /// Run the automatic 2-D layout cleanup on the *selected* scheme structure, if the current
    /// selection is one. Returns the index that was cleaned up. A convenience wrapper over
    /// [`cleanup_scheme_structure`].
    pub fn cleanup_selected_structure(&mut self, reaction: &mut Reaction) -> Option<usize> {
        match self.selection {
            Some(Hit::Scheme { index, part: SchemePart::Structure })
                if cleanup_scheme_structure(&mut reaction.scheme, index) =>
            {
                Some(index)
            }
            _ => None,
        }
    }

    /// The open edit box: what it edits, and the text in it.
    pub fn editing(&self) -> Option<(EditTarget, &str)> {
        self.edit.as_ref().map(|e| (e.target, e.text.as_str()))
    }

    /// Why the last edit was rejected, if it was. Cleared by the next accepted one.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Open the edit box on `target`, seeded with whatever is entered there.
    ///
    /// Returns `false` — and leaves any open box alone — when the target carries nothing
    /// typeable: a computed cell, `Limiting?` (a toggle), a structure or a `+`.
    pub fn begin_edit(&mut self, target: EditTarget, reaction: &Reaction) -> bool {
        let text = match target {
            EditTarget::Cell(cell) => match cell.field() {
                Some(f) if f.is_typed() => field_text(&reaction.analysis, cell),
                _ => return false,
            },
            EditTarget::SchemeText { index, part } => {
                // A structure on an arrow label is edited on the canvas, not through the text
                // box — refuse to open one on it.
                if !part.is_text()
                    || index >= reaction.scheme.items.len()
                    || scheme_part_holds_structure(&reaction.scheme, index, part)
                {
                    return false;
                }
                scheme_text(&reaction.scheme, index, part)
            }
        };
        self.edit = Some(Editing { target, text });
        self.error = None;
        true
    }

    /// Replace the text in the open box. No-op when no box is open.
    pub fn set_edit_text(&mut self, text: impl Into<String>) {
        if let Some(e) = self.edit.as_mut() {
            e.text = text.into();
        }
    }

    /// Apply the open box to `reaction`.
    ///
    /// On success the box closes. On a parse error it stays open with the offending text, so
    /// the user can fix it rather than lose it; the message is also kept in [`error`](Self::error).
    pub fn commit_edit(&mut self, reaction: &mut Reaction) -> Result<(), ParseError> {
        let Some(editing) = self.edit.as_ref() else {
            return Ok(());
        };
        match editing.target {
            EditTarget::Cell(cell) => {
                let unit = cell.field().map_or(Unit::Number, Field::unit);
                match parse_value(unit, &editing.text) {
                    Ok(value) => {
                        set_field(&mut reaction.analysis, cell, value);
                    }
                    Err(e) => {
                        self.error = Some(e.to_string());
                        return Err(e);
                    }
                }
            }
            EditTarget::SchemeText { index, part } => {
                set_scheme_text(&mut reaction.scheme, index, part, &editing.text);
            }
        }
        self.edit = None;
        self.error = None;
        Ok(())
    }

    /// Close the edit box, discarding what was typed.
    pub fn cancel_edit(&mut self) {
        self.edit = None;
    }

    /// Act on a click that landed on `hit`: it becomes the selection, `Limiting?` toggles,
    /// and any other editable target opens the edit box.
    ///
    /// Clicking the reagent that is already limiting *clears* the flag rather than doing
    /// nothing, so "this reaction has no declared limiting reagent" stays reachable from the
    /// figure alone.
    pub fn click(&mut self, hit: Hit, reaction: &mut Reaction) {
        self.selection = Some(hit);
        match hit {
            // `field_at` only ever names `Limiting` in the Reactants block, so the column
            // index is a reactant.
            Hit::Cell(cell) => match cell.field() {
                Some(Field::Limiting) => {
                    let was = reaction.analysis.reactants.get(cell.column).is_some_and(|r| r.limiting);
                    reaction.analysis.set_limiting((!was).then_some(cell.column));
                    self.edit = None;
                }
                _ => {
                    if !self.begin_edit(EditTarget::Cell(cell), reaction) {
                        self.edit = None;
                    }
                }
            },
            Hit::Scheme { index, part } => {
                if !self.begin_edit(EditTarget::SchemeText { index, part }, reaction) {
                    self.edit = None;
                }
            }
        }
    }

    /// Draw the figure and process a frame of interaction: hover and selection outlines, the
    /// edit-box overlay, and clicks.
    pub fn show(&mut self, ui: &mut egui::Ui, reaction: &mut Reaction) -> egui::Response {
        let painter = ui.painter().clone();
        let layout = self.view.measure(&painter, reaction);
        // `click_and_drag` so scheme items can be picked up and reordered; a press-release
        // without movement is still a click, so cell/label editing is unchanged.
        let (rect, response) = ui.allocate_exact_size(layout.size, Sense::click_and_drag());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter().clone();
            self.view.paint_layout(&layout, &painter, rect.min, reaction);
        }

        let hover = response
            .hover_pos()
            .and_then(|p| hit_test(&reaction.scheme, &layout, rect.min, p));

        // ── Drag a scheme item ─────────────────────────────────────────────────
        // A main-line item reorders; a structure on an arrow label moves between slots. A drag
        // that starts on the table does nothing.
        if response.drag_started() {
            let start = response
                .interact_pointer_pos()
                .and_then(|p| hit_test(&reaction.scheme, &layout, rect.min, p));
            let source = match start {
                // A structure riding on an arrow band is picked up as a slot source.
                Some(Hit::Scheme { index, part })
                    if matches!(part, SchemePart::ArrowAbove | SchemePart::ArrowBelow)
                        && scheme_arrow_structure(&reaction.scheme, index, part).is_some() =>
                {
                    Some(DragSource::ArrowLabel { index, part })
                }
                // Anything else on the reaction line reorders along it.
                Some(Hit::Scheme { index, .. }) => Some(DragSource::Item(index)),
                _ => None,
            };
            if let Some(source) = source {
                let insert_at = match source {
                    DragSource::Item(i) => i,
                    DragSource::ArrowLabel { index, .. } => index,
                };
                // Picking an item up is moving on from the open box, just like clicking away.
                // Settle it now, while the indices it names still point where they did.
                self.leave_edit(ui, reaction);
                self.drag =
                    Some(DragReorder { source, insert_at, to_slot: None, over_band: false });
            }
        }
        if response.dragged() {
            if let (Some(drag), Some(p)) = (self.drag.as_mut(), response.interact_pointer_pos()) {
                let source = drag.source;
                drag.insert_at = scheme_drop_index(&layout, rect.min, p);
                // A drop onto an arrow band takes priority over reordering: the structure lands
                // in that slot. Only a dragged *structure* is offered a slot, and only an
                // *acceptable* one — not the source's own, and not a text label (dropping a
                // structure there would clobber the text, so it is left to reorder along the
                // line instead). Anything else stays a reorder.
                let carries_structure = source.carries_structure(&reaction.scheme);
                drag.to_slot = arrow_slot_at(&reaction.scheme, &layout, rect.min, p).filter(
                    |&(ti, tp)| {
                        carries_structure
                            && !source.is_slot(ti, tp)
                            && !arrow_slot_holds_text(&reaction.scheme, ti, tp)
                    },
                );
                drag.over_band = matches!(
                    hit_test(&reaction.scheme, &layout, rect.min, p),
                    Some(Hit::Scheme { part: SchemePart::ArrowAbove | SchemePart::ArrowBelow, .. })
                );
            }
        }

        // Outlines over the finished figure. Nothing is drawn when the pointer is elsewhere
        // and nothing is selected, so an idle figure is byte-for-byte the plain rendering.
        let painter = ui.painter().clone();
        let outline = |hit: Hit, stroke: Stroke| {
            let r = match hit {
                Hit::Cell(cell) => cell_rect(&layout, rect.min, cell),
                Hit::Scheme { index, part } => scheme_item_rect(&layout, rect.min, index, part),
            };
            if let Some(r) = r {
                painter.rect_stroke(r, 2.0, stroke, StrokeKind::Inside);
            }
        };
        if let Some(hit) = self.selection {
            outline(hit, Stroke::new(1.5, self.highlight));
        }
        // While dragging, the hover outline would just chase the pointer — the drop caret says
        // where the item lands instead.
        if self.drag.is_none() {
            if let Some(hit) = hover.filter(|h| Some(*h) != self.selection) {
                outline(hit, Stroke::new(1.0, self.highlight.gamma_multiply(0.5)));
            }
        } else if let Some(drag) = self.drag {
            let source_hit = match drag.source {
                // Only drawn, never kept as a selection: `Structure` is how `scheme_item_rect`
                // gives a main-line item's whole box — for an arrow, both bands at once.
                DragSource::Item(index) => Hit::Scheme { index, part: SchemePart::Structure },
                DragSource::ArrowLabel { index, part } => Hit::Scheme { index, part },
            };
            outline(source_hit, Stroke::new(1.5, self.highlight.gamma_multiply(0.7)));
            // Landing on an arrow slot highlights that slot; otherwise a caret shows where the
            // item drops on the reaction line — for a main-line reorder, or an arrow-label
            // structure rejoining the line (which only happens when the pointer left the bands).
            if let Some((ti, tp)) = drag.to_slot {
                outline(Hit::Scheme { index: ti, part: tp }, Stroke::new(2.0, self.highlight));
            } else {
                let on_line = match drag.source {
                    DragSource::Item(_) => true,
                    DragSource::ArrowLabel { .. } => !drag.over_band,
                };
                if on_line {
                    draw_drop_caret(&painter, &layout, rect.min, drag.insert_at, self.highlight);
                }
            }
        }

        if self.drag.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if hover.is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        // Commit the drag when it ends.
        if response.drag_stopped() {
            if let Some(drag) = self.drag.take() {
                if let Some((ti, tp)) = drag.to_slot {
                    // A structure was dropped onto an arrow slot (move, or a lossless swap).
                    self.drop_into_arrow_slot(reaction, drag.source, ti, tp);
                } else {
                    match drag.source {
                        DragSource::Item(from) => {
                            // Reorder along the line. `insert_at` counts items left of the
                            // pointer, so once the moved item is pulled out its final index
                            // shifts down by one whenever it was dropped to the right of where
                            // it started.
                            let target =
                                if drag.insert_at > from { drag.insert_at - 1 } else { drag.insert_at };
                            let delta = target as isize - from as isize;
                            if let Some(landed) = reaction.scheme.move_item(from, delta) {
                                // The columns follow their structures by id; resync so they
                                // are put back in the scheme's new order, values and all.
                                reaction.sync_structures_from_scheme();
                                // Dropped back in place moves nothing; any other landing does.
                                self.scheme_rearranged |= landed != from;
                                // Select what landed by the part a click on it would give — a
                                // `+`, text or arrow is not a structure.
                                self.selection = reaction
                                    .scheme
                                    .items
                                    .get(landed)
                                    .and_then(|item| scheme_item_parts(item).first())
                                    .map(|&part| Hit::Scheme { index: landed, part });
                            }
                        }
                        DragSource::ArrowLabel { index, part } => {
                            // Off the bands and onto the reaction line: drop the arrow structure
                            // back onto the line as a main-line structure. Resting on any band
                            // (its own slot, or a text one) leaves it where it was.
                            if !drag.over_band {
                                self.drop_arrow_structure_onto_line(
                                    reaction,
                                    index,
                                    part,
                                    drag.insert_at,
                                );
                            }
                        }
                    }
                }
            }
        }

        // The overlay runs before the click below, so clicking straight from one cell to the
        // next commits the first (the box loses focus) and then opens the second.
        self.edit_overlay(ui, reaction, &layout, rect.min);

        // Double-click a structure to ask the host to open it in a molecule editor — whether
        // it is a main-line structure or one riding on an arrow label.
        if response.double_clicked() {
            match hover {
                Some(Hit::Scheme { index, part: SchemePart::Structure }) => {
                    self.selection = Some(Hit::Scheme { index, part: SchemePart::Structure });
                    self.edit_request = Some(StructureRef::Item(index));
                    self.edit = None;
                }
                Some(Hit::Scheme { index, part })
                    if matches!(part, SchemePart::ArrowAbove | SchemePart::ArrowBelow)
                        && scheme_arrow_structure(&reaction.scheme, index, part).is_some() =>
                {
                    self.selection = Some(Hit::Scheme { index, part });
                    self.edit_request = Some(StructureRef::ArrowLabel { index, part });
                    self.edit = None;
                }
                _ => {}
            }
        } else if response.clicked() {
            match hover {
                Some(hit) => self.click(hit, reaction),
                None => self.selection = None,
            }
        }

        // Secondary click records what the pointer is over so the host's context menu (attached
        // to the returned response) knows what it was opened on.
        if response.secondary_clicked() {
            self.context_target = hover;
            if let Some(hit) = hover {
                self.selection = Some(hit);
            }
        }

        response
    }

    /// Land a dragged structure on the arrow slot `(ti, tp)`. The slot is always one the drag
    /// routing already vetted as *acceptable* — empty or structure-occupied, never text.
    ///
    /// * From a **main-line** structure onto an **empty** slot: move it off the line into the
    ///   slot, then re-derive the table columns (delete-from-line plus place-on-arrow).
    /// * From a **main-line** structure onto a **structure-occupied** slot: swap the two
    ///   losslessly — the line position takes the slot's structure and the slot takes the
    ///   dragged one — then re-derive (課題A).
    /// * From **another arrow slot**: swap the two, so above/below (or two arrows) trade
    ///   structures.
    fn drop_into_arrow_slot(
        &mut self,
        reaction: &mut Reaction,
        source: DragSource,
        ti: usize,
        tp: SchemePart,
    ) {
        match source {
            DragSource::Item(from) => {
                if scheme_arrow_structure(&reaction.scheme, ti, tp).is_some() {
                    // Occupied by a structure: trade places rather than refuse (課題A).
                    if swap_line_structure_with_arrow_slot(&mut reaction.scheme, from, ti, tp) {
                        reaction.sync_structures_from_scheme();
                        self.scheme_rearranged = true;
                        self.selection =
                            Some(Hit::Scheme { index: from, part: SchemePart::Structure });
                    }
                } else if let Some(landed) =
                    move_line_structure_to_arrow_slot(&mut reaction.scheme, from, ti, tp)
                {
                    reaction.sync_structures_from_scheme();
                    self.scheme_rearranged = true;
                    self.selection = Some(Hit::Scheme { index: landed, part: tp });
                }
            }
            DragSource::ArrowLabel { index, part } => {
                // Check the target band before taking anything off, so a structure never
                // leaves the figure for a target that cannot hold it.
                if arrow_label(&reaction.scheme, ti, tp).is_none()
                    || scheme_arrow_structure(&reaction.scheme, index, part).is_none()
                {
                    return;
                }
                // The structures move whole, so each keeps its id. Whatever sits in the target
                // slot goes back to the source (a swap); an empty target leaves the source empty.
                let Some(moving) = take_scheme_arrow_structure(&mut reaction.scheme, index, part)
                else {
                    return;
                };
                let displaced = take_scheme_arrow_structure(&mut reaction.scheme, ti, tp);
                place_scheme_arrow_structure(&mut reaction.scheme, ti, tp, moving);
                if let Some(d) = displaced {
                    place_scheme_arrow_structure(&mut reaction.scheme, index, part, d);
                }
                // Arrow-label structures feed reactant columns, and a swap can move which slot
                // holds which — re-derive so the columns follow.
                reaction.sync_structures_from_scheme();
                self.scheme_rearranged = true;
                self.selection = Some(Hit::Scheme { index: ti, part: tp });
            }
        }
    }

    /// Drop the structure riding on an arrow band back onto the reaction line, inserting it at
    /// `insert_at`, then re-derive the table columns (課題B). The band is left empty.
    fn drop_arrow_structure_onto_line(
        &mut self,
        reaction: &mut Reaction,
        index: usize,
        part: SchemePart,
        insert_at: usize,
    ) {
        if let Some(at) = move_arrow_structure_to_line(&mut reaction.scheme, index, part, insert_at)
        {
            reaction.sync_structures_from_scheme();
            self.scheme_rearranged = true;
            self.selection = Some(Hit::Scheme { index: at, part: SchemePart::Structure });
        }
    }

    /// The edit box's widget id, fixed (rather than left to egui) so its keyboard focus can be
    /// handed back from outside the overlay — see [`leave_edit`](Self::leave_edit).
    fn edit_box_id(ui: &egui::Ui) -> egui::Id {
        ui.id().with("reaction_cell_edit_text")
    }

    /// The user moved on from the open box — clicked away from it, or picked up a scheme item
    /// to drag. What was typed is committed. Moving on must never trap the user in a box it
    /// refuses to close, so a bad value is dropped rather than kept — [`error`](Self::error)
    /// says what happened. Either way the box closes and gives up the keyboard. No-op when no
    /// box is open.
    fn leave_edit(&mut self, ui: &egui::Ui, reaction: &mut Reaction) {
        if self.edit.is_none() {
            return;
        }
        if self.commit_edit(reaction).is_err() {
            self.edit = None;
        }
        let id = Self::edit_box_id(ui);
        ui.ctx().memory_mut(|m| m.surrender_focus(id));
    }

    /// The floating text box, in the style of the structure editor's own label editor: an
    /// [`egui::Area`] over the cell, Enter to commit, Escape to abandon, and a commit on
    /// focus loss so clicking away is not a trap.
    fn edit_overlay(
        &mut self,
        ui: &mut egui::Ui,
        reaction: &mut Reaction,
        layout: &FigureLayout,
        top_left: Pos2,
    ) {
        let Some(target) = self.edit.as_ref().map(|e| e.target) else {
            return;
        };
        let anchor = match target {
            EditTarget::Cell(cell) => cell_rect(layout, top_left, cell),
            EditTarget::SchemeText { index, part } => {
                scheme_item_rect(layout, top_left, index, part)
            }
        };
        // The target can vanish under the box (a column deleted from the host's panel);
        // close rather than float over nothing.
        let Some(anchor) = anchor else {
            self.edit = None;
            return;
        };

        // The buffer comes out of `self` for the duration of the frame, so the commit below
        // can take `&mut self` while the text box still holds a `&mut String`.
        let mut text = self.edit.as_ref().map_or(String::new(), |e| e.text.clone());

        // Live check, so a bad unit is flagged as it is typed rather than on Enter.
        let live_error = match target {
            EditTarget::Cell(cell) => {
                let unit = cell.field().map_or(Unit::Number, Field::unit);
                parse_value(unit, &text).err()
            }
            EditTarget::SchemeText { .. } => None,
        };
        // What the cell shows now, offered as a hint: an empty box means "leave it computed".
        let hint = match target {
            EditTarget::Cell(cell) => layout
                .table
                .block_index(cell.block)
                .and_then(|i| layout.table.blocks[i].columns.get(cell.column))
                .and_then(|col| col.get(cell.row))
                .map(|c| c.plain())
                .unwrap_or_default(),
            EditTarget::SchemeText { .. } => String::new(),
        };

        let width = anchor.width().max(88.0);
        let box_id = Self::edit_box_id(ui);
        let area = egui::Area::new(ui.id().with("reaction_cell_edit"))
            .order(egui::Order::Foreground)
            // egui sizes a new Area with `spacing.default_area_size` (600 x 400) for its
            // first, invisible pass, and that oversized rect is what decides hit-testing on
            // the following frame — a click a third of the figure away would be swallowed.
            // A realistic guess costs nothing and keeps the figure clickable throughout.
            .default_size(egui::vec2(width + 24.0, 40.0))
            .fixed_pos(anchor.min)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .show(ui, |ui| {
                        let mut output = egui::TextEdit::singleline(&mut text)
                            .id(box_id)
                            .desired_width(width)
                            .hint_text(hint)
                            .show(ui);
                        // Select the seeded text on the first frame so typing replaces it —
                        // `has_focus()` is false only before `request_focus()` below lands.
                        if !output.response.has_focus() {
                            let range = egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(text.chars().count()),
                            );
                            output.state.cursor.set_char_range(Some(range));
                            output.state.store(ui.ctx(), output.response.id);
                        }
                        if let Some(e) = &live_error {
                            ui.colored_label(Color32::from_rgb(190, 40, 40), e.to_string());
                        }
                        output.response
                    })
                    .inner
            });
        let response = area.inner;
        response.request_focus();
        if let Some(e) = self.edit.as_mut() {
            e.text = text;
        }

        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
        let lost = response.lost_focus() && !enter;

        if escape {
            self.cancel_edit();
        } else if enter {
            // A rejected Enter keeps the box open with the offending text.
            let _ = self.commit_edit(reaction);
        } else if lost {
            self.leave_edit(ui, reaction);
        }
        if self.edit.is_none() {
            // Hand the keyboard back the moment the box closes; a TextEdit that simply stops
            // being shown keeps its focus until egui reaps it.
            response.surrender_focus();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;
    use crate::reaction::formula::MolecularFormula;
    use crate::reaction::model::{Product, Reactant};
    use crate::reaction::stoich::{format_mass, format_moles, format_percent};
    use crate::reaction::table::{AnalysisTable, PRODUCT_ROWS, REACTANT_ROWS};
    use crate::reaction::{Arrow, Origin};
    use egui::Vec2;

    /// Run `f` inside a headless egui frame with a real font stack, handing it a measured
    /// figure at a known top-left corner.
    fn with_figure(reaction: &Reaction, mut f: impl FnMut(&FigureLayout, Pos2)) {
        const TOP_LEFT: Pos2 = Pos2::new(30.0, 20.0);
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
        let mut view = ReactionView::new();
        let _ = ctx.run_ui(raw, |ui| {
            let layout = view.measure(ui.painter(), reaction);
            f(&layout, TOP_LEFT);
        });
    }

    // ── Which cells are editable ────────────────────────────────────────────

    /// The row indices in [`field_at`] are pinned to the row *labels*, so the two lists
    /// cannot drift apart, and every row named as editable in the design is one.
    #[test]
    fn rows_and_fields_agree() {
        let editable_reactant = [
            ("Limiting?", Field::Limiting),
            ("Equivalents", Field::Equivalents),
            ("Sample Mass", Field::SampleMass),
            ("%Weight", Field::PercentWeight),
            ("Molarity", Field::Molarity),
            ("Density", Field::Density),
            ("Volume", Field::Volume),
        ];
        for (label, field) in editable_reactant {
            let row = REACTANT_ROWS.iter().position(|r| *r == label).unwrap();
            assert_eq!(field_at(BlockKind::Reactants, row), Some(field), "{label}");
        }
        for (label, field) in [
            ("Equivalents", Field::Equivalents),
            ("%Completion", Field::PercentCompletion),
            ("Measured Mass", Field::MeasuredMass),
            ("Purity", Field::Purity),
        ] {
            let row = PRODUCT_ROWS.iter().position(|r| *r == label).unwrap();
            assert_eq!(field_at(BlockKind::Products, row), Some(field), "{label}");
        }

        // Everything derived is read-only.
        for label in ["Formula", "MW", "Reactant Moles", "Reactant Mass"] {
            let row = REACTANT_ROWS.iter().position(|r| *r == label).unwrap();
            assert_eq!(field_at(BlockKind::Reactants, row), None, "{label}");
        }
        for label in [
            "Formula",
            "MW",
            "Expected Mass",
            "Expected Moles",
            "Product Mass",
            "Product Moles",
            "%Yield",
        ] {
            let row = PRODUCT_ROWS.iter().position(|r| *r == label).unwrap();
            assert_eq!(field_at(BlockKind::Products, row), None, "{label}");
        }
        // Out of range is not editable either.
        assert_eq!(field_at(BlockKind::Reactants, 99), None);
    }

    // ── Parsing ─────────────────────────────────────────────────────────────

    /// The units the table prints are the units it reads back.
    #[test]
    fn values_parse_in_the_units_the_table_prints() {
        // The unit scaling is a float multiply, so compare to the last significant digit.
        let ok = |u, s: &str, want: f64| {
            let got = parse_value(u, s).unwrap().unwrap();
            assert!((got - want).abs() < 1e-9, "'{s}' → {got}, expected {want}");
        };
        ok(Unit::Mass, "1.00g", 1.0);
        ok(Unit::Mass, "700.00mg", 0.7);
        ok(Unit::Mass, "1.20kg", 1200.0);
        ok(Unit::Mass, "2.5", 2.5); // a bare number is the base unit
        ok(Unit::Mass, " 700 MG ", 0.7); // case and space are forgiven
        ok(Unit::Volume, "5.00mL", 5.0);
        ok(Unit::Volume, "1.5L", 1500.0);
        ok(Unit::Percent, "60.00%", 60.0);
        ok(Unit::Molarity, "2.00M", 2.0);
        ok(Unit::Molarity, "50mM", 0.05);
        ok(Unit::Density, "2.28g/mL", 2.28);
        ok(Unit::Number, "1.10", 1.10);
        ok(Unit::Number, "3eq", 3.0);
        // The numeric prefix is taken as long as possible, so an exponent is not mistaken
        // for the start of a unit.
        ok(Unit::Mass, "1e-3", 1e-3);
    }

    /// Blank means "not entered": the entry is cleared and the cell goes back to computed.
    #[test]
    fn blank_input_clears_the_entry() {
        assert_eq!(parse_value(Unit::Mass, ""), Ok(None));
        assert_eq!(parse_value(Unit::Mass, "   "), Ok(None));

        let mut reaction = demo::williamson_ether_synthesis();
        let cell = CellRef::new(BlockKind::Reactants, 0, 4); // phenol, Sample Mass
        assert!(reaction.analysis.reactants[0].sample_mass.is_some());
        assert!(set_field(&mut reaction.analysis, cell, None));
        assert!(reaction.analysis.reactants[0].sample_mass.is_none());

        // With no weighed sample left, the whole table loses its scale.
        let a = reaction.analysis.compute();
        assert!(a.limiting_moles.is_none());
        assert!(a.reactants[0].moles.is_none());
        assert!(a.products[0].percent_yield.is_none());
    }

    /// Bad input is rejected rather than guessed at.
    #[test]
    fn invalid_input_is_rejected() {
        assert_eq!(parse_value(Unit::Mass, "abc"), Err(ParseError::NotANumber));
        assert_eq!(parse_value(Unit::Mass, "-1g"), Err(ParseError::OutOfRange));
        assert_eq!(parse_value(Unit::Mass, "inf"), Err(ParseError::OutOfRange));
        assert_eq!(parse_value(Unit::Mass, "nan"), Err(ParseError::OutOfRange));
        assert_eq!(
            parse_value(Unit::Mass, "1.0mL"),
            Err(ParseError::UnknownUnit("mL".into())),
            "a volume unit is not a mass"
        );
        assert_eq!(
            parse_value(Unit::Number, "1.0g"),
            Err(ParseError::UnknownUnit("g".into())),
            "equivalents are dimensionless"
        );
        // A density or concentration of zero would be a divide-by-zero, not an amount.
        assert_eq!(parse_value(Unit::Density, "0"), Err(ParseError::OutOfRange));
        assert_eq!(parse_value(Unit::Molarity, "0M"), Err(ParseError::OutOfRange));
        // A zero mass is a legitimate (if odd) entry.
        assert_eq!(parse_value(Unit::Mass, "0g"), Ok(Some(0.0)));
    }

    /// Every entered value round-trips: what the box seeds is what the parser reads back.
    #[test]
    fn entered_values_round_trip_through_the_edit_box() {
        let analysis = Analysis::new()
            .reactant(
                Reactant::new(demo::phenol())
                    .limiting()
                    .equivalents(1.0)
                    .sample_mass(Mass::milligrams(700.0))
                    .percent_weight(60.0)
                    .molarity(2.0)
                    .density(2.28)
                    .volume(5.0),
            )
            .product(
                Product::new(demo::anisole())
                    .equivalents(1.0)
                    .percent_completion(50.0)
                    .measured_mass(Mass::grams(1.0))
                    .purity(95.0),
            );
        let expect = [
            (BlockKind::Reactants, 3, 1.0),
            (BlockKind::Reactants, 4, 0.7),
            (BlockKind::Reactants, 5, 60.0),
            (BlockKind::Reactants, 6, 2.0),
            (BlockKind::Reactants, 7, 2.28),
            (BlockKind::Reactants, 8, 5.0),
            (BlockKind::Products, 2, 1.0),
            (BlockKind::Products, 3, 50.0),
            (BlockKind::Products, 6, 1.0),
            (BlockKind::Products, 7, 95.0),
        ];
        for (block, row, value) in expect {
            let cell = CellRef::new(block, 0, row);
            let text = field_text(&analysis, cell);
            assert!(!text.is_empty(), "{block:?} row {row} should seed the box");
            let field = cell.field().unwrap();
            let back = parse_value(field.unit(), &text)
                .unwrap_or_else(|e| panic!("{block:?} row {row}: '{text}' rejected: {e}"))
                .unwrap();
            assert!(
                (back - value).abs() < 1e-6,
                "{block:?} row {row}: '{text}' → {back}, expected {value}"
            );
        }

        // A cell with nothing entered seeds an empty box, even where the table prints a
        // computed number.
        let plain = demo::williamson_ether_synthesis().analysis;
        assert_eq!(field_text(&plain, CellRef::new(BlockKind::Reactants, 1, 4)), "");
        assert_eq!(field_text(&plain, CellRef::new(BlockKind::Reactants, 0, 4)), "1g");
    }

    // ── Editing recomputes ──────────────────────────────────────────────────

    /// Changing an input immediately changes every value derived from it.
    #[test]
    fn editing_a_cell_recomputes_the_derived_cells() {
        let mut reaction = demo::williamson_ether_synthesis();
        let before = reaction.analysis.compute();
        assert_eq!(format_mass(before.reactants[1].mass.unwrap().value).joined(), "1.66g");
        assert_eq!(format_percent(before.products[0].percent_yield.unwrap().value).joined(), "60.92%");

        // Double the limiting reagent: everything scaled from it doubles.
        let cell = CellRef::new(BlockKind::Reactants, 0, 4);
        assert!(set_field(&mut reaction.analysis, cell, Some(2.0)));
        let after = reaction.analysis.compute();
        assert_eq!(format_mass(after.reactants[1].mass.unwrap().value).joined(), "3.32g");
        assert_eq!(format_moles(after.limiting_moles.unwrap()).joined(), "21.25mmol");
        // …and half as much of it was converted.
        assert_eq!(format_percent(after.products[0].percent_yield.unwrap().value).joined(), "30.46%");

        // The edited cell is now an entered value; the ones derived from it are not.
        assert_eq!(after.reactants[0].sample_mass.unwrap().origin, Origin::Entered);
        assert_eq!(after.reactants[1].sample_mass.unwrap().origin, Origin::Computed);

        // Equivalents of a co-reagent rescale only that column.
        let cell = CellRef::new(BlockKind::Reactants, 1, 3);
        assert!(set_field(&mut reaction.analysis, cell, Some(2.0)));
        let after = reaction.analysis.compute();
        assert_eq!(format_moles(after.reactants[1].moles.unwrap().value).joined(), "42.50mmol");
        assert_eq!(format_moles(after.reactants[2].moles.unwrap().value).joined(), "63.75mmol");
    }

    /// A computed cell cannot be written to, and neither can a column that is not there.
    #[test]
    fn read_only_and_missing_cells_reject_writes() {
        let mut reaction = demo::williamson_ether_synthesis();
        for row in [0, 1, 9, 10] {
            assert!(
                !set_field(&mut reaction.analysis, CellRef::new(BlockKind::Reactants, 0, row), Some(1.0)),
                "{} must be read-only",
                REACTANT_ROWS[row]
            );
        }
        // `Limiting?` is editable, but as a toggle — not by writing a number.
        assert!(!set_field(&mut reaction.analysis, CellRef::new(BlockKind::Reactants, 0, 2), Some(1.0)));
        assert!(!set_field(&mut reaction.analysis, CellRef::new(BlockKind::Reactants, 9, 4), Some(1.0)));
        assert!(!set_field(&mut reaction.analysis, CellRef::new(BlockKind::Products, 9, 6), Some(1.0)));
    }

    // ── Limiting reagent ────────────────────────────────────────────────────

    /// Exactly one reagent is limiting, and moving the flag rescales the whole table.
    #[test]
    fn limiting_is_exclusive_and_rescales_everything() {
        let mut reaction = demo::williamson_ether_synthesis();
        // Give MeI a weighed mass so it can carry the scale once it is limiting.
        set_field(&mut reaction.analysis, CellRef::new(BlockKind::Reactants, 1, 4), Some(1.0));

        reaction.analysis.set_limiting(Some(1));
        let flags: Vec<bool> = reaction.analysis.reactants.iter().map(|r| r.limiting).collect();
        assert_eq!(flags, [false, true, false], "only one reagent may be limiting");
        assert_eq!(reaction.analysis.limiting(), Some(1));

        let a = reaction.analysis.compute();
        assert_eq!(a.limiting_index, Some(1));
        // 1.00 g of MeI is 7.05 mmol, and phenol now follows from it.
        assert_eq!(format_moles(a.limiting_moles.unwrap()).joined(), "7.05mmol");
        assert_eq!(format_moles(a.reactants[0].moles.unwrap().value).joined(), "10.63mmol");

        // Clearing the flag is a legal state: the fallback finds the first reagent that
        // pins down an amount of its own, which is phenol again.
        reaction.analysis.set_limiting(None);
        assert!(reaction.analysis.limiting().is_none());
        let a = reaction.analysis.compute();
        assert_eq!(a.limiting_index, Some(0));
        assert_eq!(format_moles(a.limiting_moles.unwrap()).joined(), "10.63mmol");
    }

    /// Clicking `Limiting?` moves the flag; clicking the reagent that already has it clears it.
    #[test]
    fn clicking_limiting_toggles_it() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();

        editor.click(Hit::Cell(CellRef::new(BlockKind::Reactants, 2, 2)), &mut reaction);
        assert_eq!(reaction.analysis.limiting(), Some(2));
        assert!(editor.editing().is_none(), "a toggle must not open a text box");

        editor.click(Hit::Cell(CellRef::new(BlockKind::Reactants, 2, 2)), &mut reaction);
        assert_eq!(reaction.analysis.limiting(), None, "clicking it again clears it");
    }

    /// Clearing the declaration on phenol lets the fallback pick phenol again. The cell must
    /// still show that the click did something: the declared bold-italic "Yes" becomes an
    /// inferred regular "Yes", matching the "No" that the entered state now holds.
    #[test]
    fn clearing_a_declared_limiting_reagent_shows_the_inferred_one() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();
        let cell = CellRef::new(BlockKind::Reactants, 0, 2);
        let limiting_cell = |reaction: &Reaction| {
            AnalysisTable::from_result(&reaction.analysis.compute()).blocks[0].columns[0][2].clone()
        };

        let before = limiting_cell(&reaction);
        assert_eq!((before.plain(), before.is_entered()), ("Yes".to_string(), true));
        assert_eq!(field_text(&reaction.analysis, cell), "Yes");

        editor.click(Hit::Cell(cell), &mut reaction);
        assert_eq!(reaction.analysis.limiting(), None);
        let after = limiting_cell(&reaction);
        assert_eq!((after.plain(), after.is_entered()), ("Yes".to_string(), false));
        assert_eq!(field_text(&reaction.analysis, cell), "No");

        // Clicking the inferred "Yes" declares it again.
        editor.click(Hit::Cell(cell), &mut reaction);
        assert_eq!(reaction.analysis.limiting(), Some(0));
        assert!(limiting_cell(&reaction).is_entered());
    }

    /// Deleting the limiting reagent leaves the reaction without one; the derived amounts go
    /// blank instead of quietly rescaling to a reagent that has no amount of its own.
    #[test]
    fn deleting_the_limiting_reagent_blanks_the_derived_amounts() {
        let mut reaction = demo::williamson_ether_synthesis();
        let removed = reaction.analysis.remove_reactant(0).expect("phenol is column 0");
        assert!(removed.limiting);
        assert_eq!(reaction.analysis.reactants.len(), 2);
        assert!(reaction.analysis.limiting().is_none(), "no neighbour is promoted");

        let a = reaction.analysis.compute();
        assert!(a.limiting_moles.is_none());
        for r in &a.reactants {
            assert!(r.moles.is_none(), "no scale, no amounts");
            assert!(r.mass.is_none());
            assert!(r.sample_mass.is_none());
            // What the user actually typed survives.
            assert!(r.equivalents.unwrap().is_entered());
        }
        assert!(a.products[0].expected_moles.is_none());
        assert!(a.products[0].percent_yield.is_none());
        // The measured mass is still the user's, and still says how much material there is.
        assert_eq!(format_mass(a.products[0].mass.unwrap().value).joined(), "700.00mg");

        // Naming a new limiting reagent and weighing it brings the table back.
        reaction.analysis.set_limiting(Some(0));
        set_field(&mut reaction.analysis, CellRef::new(BlockKind::Reactants, 0, 4), Some(1.0));
        let a = reaction.analysis.compute();
        assert_eq!(format_moles(a.limiting_moles.unwrap()).joined(), "7.05mmol");
    }

    // ── Adding and deleting columns ─────────────────────────────────────────

    /// Adding a column adds a column; deleting one deletes exactly that one.
    #[test]
    fn columns_can_be_added_and_deleted() {
        let mut reaction = demo::williamson_ether_synthesis();
        let table = AnalysisTable::from_result(&reaction.analysis.compute());
        assert_eq!(table.blocks[0].columns.len(), 3);

        let i = reaction.analysis.add_reactant(Reactant::new(demo::methyl_iodide()).equivalents(2.0));
        assert_eq!(i, 3);
        let j = reaction.analysis.add_product(Product::new(demo::phenol()));
        assert_eq!(j, 1);
        let table = AnalysisTable::from_result(&reaction.analysis.compute());
        assert_eq!(table.blocks[0].columns.len(), 4);
        assert_eq!(table.blocks[1].columns.len(), 2);
        // The new reactant is derived from the limiting reagent straight away.
        assert_eq!(table.blocks[0].columns[3][0].plain(), "CH3I");
        assert_eq!(table.blocks[0].columns[3][9].plain(), "21.25mmol");

        // Delete a middle reactant: the ones after it shuffle down.
        let gone = reaction.analysis.remove_reactant(1).unwrap();
        assert_eq!(MolecularFormula::of(&gone.molecule).plain(), "CH3I");
        let table = AnalysisTable::from_result(&reaction.analysis.compute());
        let formulas: Vec<String> = table.blocks[0].columns.iter().map(|c| c[0].plain()).collect();
        assert_eq!(formulas, ["C6H6O", "CK2O3", "CH3I"]);

        assert!(reaction.analysis.remove_product(1).is_some());
        assert!(reaction.analysis.remove_product(9).is_none(), "out of range is ignored");
        assert!(reaction.analysis.remove_reactant(9).is_none());

        // Emptying a block drops the block, not just its columns.
        while reaction.analysis.remove_product(0).is_some() {}
        let table = AnalysisTable::from_result(&reaction.analysis.compute());
        assert_eq!(table.blocks.len(), 1);
        assert_eq!(table.block_index(BlockKind::Products), None);
        assert_eq!(table.block_index(BlockKind::Reactants), Some(0));
    }

    /// Scheme items can be appended, removed and reordered.
    #[test]
    fn scheme_items_can_be_added_and_deleted() {
        let mut scheme = demo::williamson_ether_synthesis().scheme;
        assert_eq!(scheme.items.len(), 3);

        let i = scheme.add(SchemeItem::Plus);
        assert_eq!(i, 3);
        scheme.add(SchemeItem::Text(RichText::parse("Δ")));
        assert_eq!(scheme.items.len(), 5);

        assert!(matches!(scheme.remove(3), Some(SchemeItem::Plus)));
        assert!(scheme.remove(99).is_none());
        assert_eq!(scheme.items.len(), 4);

        // Reordering clamps at the ends rather than wrapping or failing.
        assert_eq!(scheme.move_item(3, -1), Some(2));
        assert!(matches!(scheme.items[2], SchemeItem::Text(_)));
        assert_eq!(scheme.move_item(2, -9), Some(0));
        assert_eq!(scheme.move_item(0, 9), Some(3));
        assert_eq!(scheme.move_item(9, 1), None);
    }

    // ── Scheme text ─────────────────────────────────────────────────────────

    /// Arrow labels and free text round-trip through the box as markup, subscripts included.
    #[test]
    fn scheme_text_round_trips_as_markup() {
        let mut scheme = demo::williamson_ether_synthesis().scheme;
        assert_eq!(scheme_text(&scheme, 1, SchemePart::ArrowAbove), "MeI");
        assert_eq!(scheme_text(&scheme, 1, SchemePart::ArrowBelow), "K_{2}CO_{3}");
        // A structure carries no text, and neither does a label that was never written.
        assert_eq!(scheme_text(&scheme, 0, SchemePart::Structure), "");
        assert_eq!(scheme_text(&scheme, 99, SchemePart::Text), "");

        assert!(set_scheme_text(&mut scheme, 1, SchemePart::ArrowBelow, "Cs_2CO_3, DMF"));
        assert_eq!(scheme_text(&scheme, 1, SchemePart::ArrowBelow), "Cs_{2}CO_{3}, DMF");
        let SchemeItem::Arrow(arrow) = &scheme.items[1] else {
            panic!("item 1 is the arrow")
        };
        let below = arrow.below.as_text().expect("the label below is text");
        assert_eq!(
            below.spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            "Cs2CO3, DMF"
        );
        assert!(below.spans.iter().any(|s| s.text == "2"
            && s.style.script == crate::widget::rich_text::Script::Sub));

        // Nothing to write to is reported rather than silently dropped.
        assert!(!set_scheme_text(&mut scheme, 0, SchemePart::ArrowAbove, "x"));
        assert!(!set_scheme_text(&mut scheme, 99, SchemePart::Text, "x"));
    }

    // ── Geometry ────────────────────────────────────────────────────────────

    /// Every cell of the drawn table maps back to itself: the rect the layout gives for a
    /// cell hit-tests to that same cell, columns and rows line up, and the row-label column
    /// is not addressable.
    #[test]
    fn cells_hit_test_to_themselves() {
        let reaction = demo::williamson_ether_synthesis();
        with_figure(&reaction, |layout, top_left| {
            for (block, columns) in [(BlockKind::Reactants, 3), (BlockKind::Products, 1)] {
                for column in 0..columns {
                    for row in 0..REACTANT_ROWS.len() {
                        let cell = CellRef::new(block, column, row);
                        let rect = cell_rect(layout, top_left, cell)
                            .unwrap_or_else(|| panic!("no rect for {cell:?}"));
                        assert!(rect.width() > 0.0 && rect.height() > 0.0);
                        assert_eq!(
                            hit_test(&reaction.scheme, layout, top_left, rect.center()),
                            Some(Hit::Cell(cell)),
                            "{cell:?} at {rect:?}"
                        );
                    }
                }
            }

            // Out of range asks for nothing.
            assert!(cell_rect(layout, top_left, CellRef::new(BlockKind::Reactants, 3, 0)).is_none());
            assert!(cell_rect(layout, top_left, CellRef::new(BlockKind::Reactants, 0, 11)).is_none());

            // The row-label column is inside the frame but is not a cell.
            let first = cell_rect(layout, top_left, CellRef::new(BlockKind::Reactants, 0, 0)).unwrap();
            let labels = Pos2::new(first.min.x - 10.0, first.center().y);
            assert_eq!(hit_test(&reaction.scheme, layout, top_left, labels), None);
            // So is everything outside the figure.
            assert_eq!(
                hit_test(&reaction.scheme, layout, top_left, top_left - Vec2::splat(50.0)),
                None
            );
        });
    }

    /// The `Limiting?` cell of each column is the one the row labels say it is — the hit test
    /// and the drawn table agree on what the user is clicking.
    #[test]
    fn hit_test_finds_the_row_the_label_names() {
        let reaction = demo::williamson_ether_synthesis();
        let table = AnalysisTable::from_result(&reaction.analysis.compute());
        with_figure(&reaction, |layout, top_left| {
            let row = REACTANT_ROWS.iter().position(|r| *r == "Limiting?").unwrap();
            for column in 0..3 {
                let cell = CellRef::new(BlockKind::Reactants, column, row);
                let rect = cell_rect(layout, top_left, cell).unwrap();
                assert_eq!(
                    hit_test(&reaction.scheme, layout, top_left, rect.center()),
                    Some(Hit::Cell(cell))
                );
                assert_eq!(
                    table.blocks[0].columns[column][row].plain(),
                    if column == 0 { "Yes" } else { "No" }
                );
            }
        });
    }

    /// Scheme items answer for themselves, and the arrow splits into two text bands at the
    /// reaction line — even when it carries no text at all.
    #[test]
    fn scheme_items_hit_test_by_part() {
        let reaction = demo::williamson_ether_synthesis();
        with_figure(&reaction, |layout, top_left| {
            let line_y = top_left.y + layout.scheme_offset.y + layout.scheme.center_line;

            for (index, part) in [(0, SchemePart::Structure), (2, SchemePart::Structure)] {
                let rect = scheme_item_rect(layout, top_left, index, part).unwrap();
                assert_eq!(
                    hit_test(&reaction.scheme, layout, top_left, rect.center()),
                    Some(Hit::Scheme { index, part })
                );
            }

            let above = scheme_item_rect(layout, top_left, 1, SchemePart::ArrowAbove).unwrap();
            let below = scheme_item_rect(layout, top_left, 1, SchemePart::ArrowBelow).unwrap();
            assert!((above.max.y - line_y).abs() < 0.01, "the bands meet at the shaft");
            assert!((below.min.y - line_y).abs() < 0.01);
            assert_eq!(
                hit_test(&reaction.scheme, layout, top_left, above.center()),
                Some(Hit::Scheme { index: 1, part: SchemePart::ArrowAbove })
            );
            assert_eq!(
                hit_test(&reaction.scheme, layout, top_left, below.center()),
                Some(Hit::Scheme { index: 1, part: SchemePart::ArrowBelow })
            );
        });

        // A bare arrow's labels are empty, so its box is only a few pixels tall — the bands
        // are grown so there is still something to click.
        let bare = Reaction::new(
            ReactionScheme::new()
                .structure(demo::phenol())
                .plus()
                .arrow(Arrow::new())
                .text("Δ"),
            Analysis::new(),
        );
        with_figure(&bare, |layout, top_left| {
            let above = scheme_item_rect(layout, top_left, 2, SchemePart::ArrowAbove).unwrap();
            assert!(above.height() >= ARROW_HIT_HALF_HEIGHT - 0.01, "{above:?}");
            assert_eq!(
                hit_test(&bare.scheme, layout, top_left, above.center()),
                Some(Hit::Scheme { index: 2, part: SchemePart::ArrowAbove })
            );
            // `+` and free text answer as themselves.
            for (index, part) in [(1, SchemePart::Plus), (3, SchemePart::Text)] {
                let rect = scheme_item_rect(layout, top_left, index, part).unwrap();
                assert_eq!(
                    hit_test(&bare.scheme, layout, top_left, rect.center()),
                    Some(Hit::Scheme { index, part })
                );
            }
        });
    }

    // ── Cleanup ─────────────────────────────────────────────────────────────

    /// Cleanup relaxes a scheme structure's coordinates in place, and leaves every other
    /// kind of scheme item (and out-of-range indices) untouched.
    #[test]
    fn cleanup_scheme_structure_relaxes_only_structures() {
        use crate::molecule::{BondOrder, Molecule};

        let mut mol = Molecule::default();
        let a = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [5.0, 0.0], 0); // bond far too long
        mol.add_bond(a, b, BondOrder::Single);
        let mut scheme = ReactionScheme::new().structure(mol).plus();

        assert!(cleanup_scheme_structure(&mut scheme, 0), "index 0 is a structure");
        let SchemeItem::Structure(relaxed) = &scheme.items[0] else {
            panic!("item 0 is a structure");
        };
        let (p, q) = (relaxed.atoms[0].pos, relaxed.atoms[1].pos);
        let d = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        assert!((d - 1.5).abs() < 0.2, "the bond relaxes toward L = 1.5, got {d}");

        // A `+` (and an out-of-range index) is not a structure — reported, not cleaned.
        assert!(!cleanup_scheme_structure(&mut scheme, 1), "index 1 is a plus");
        assert!(!cleanup_scheme_structure(&mut scheme, 99), "out of range");
    }

    // ── Drag to reorder ─────────────────────────────────────────────────────

    /// The drop slot is the number of items whose centre sits left of the pointer, so it runs
    /// `0..=items.len()` across the reaction line.
    #[test]
    fn scheme_drop_index_counts_items_left_of_the_pointer() {
        let reaction = demo::williamson_ether_synthesis();
        with_figure(&reaction, |layout, top_left| {
            let origin = top_left + layout.scheme_offset;
            let centre: Vec<f32> = layout
                .scheme
                .items
                .iter()
                .map(|p| p.rect.translate(origin.to_vec2()).center().x)
                .collect();
            let y = origin.y + layout.scheme.center_line;

            assert_eq!(scheme_drop_index(layout, top_left, Pos2::new(origin.x - 50.0, y)), 0);
            assert_eq!(scheme_drop_index(layout, top_left, Pos2::new(centre[2] + 500.0, y)), 3);
            assert_eq!(scheme_drop_index(layout, top_left, Pos2::new(centre[0] + 1.0, y)), 1);
            assert_eq!(scheme_drop_index(layout, top_left, Pos2::new(centre[1] + 1.0, y)), 2);
        });
    }

    /// The drop-slot → final-index conversion in `show`'s `drag_stopped` handler moves an item
    /// to exactly the gap the caret pointed at, whichever direction it travelled.
    #[test]
    fn dragging_reorders_scheme_items() {
        // Mirror the index math in `ReactionEditor::show`.
        fn drop(scheme: &mut ReactionScheme, from: usize, insert_at: usize) -> Option<usize> {
            let target = if insert_at > from { insert_at - 1 } else { insert_at };
            scheme.move_item(from, target as isize - from as isize)
        }

        // [Structure, Arrow, Structure]
        let mut scheme = demo::williamson_ether_synthesis().scheme;
        // Drag the first structure past the end (slot 3): it lands last.
        assert_eq!(drop(&mut scheme, 0, 3), Some(2));
        assert!(matches!(scheme.items[0], SchemeItem::Arrow(_)));
        assert!(matches!(scheme.items[2], SchemeItem::Structure(_)));
        // Drag it back to the front (slot 0).
        assert_eq!(drop(&mut scheme, 2, 0), Some(0));
        assert!(matches!(scheme.items[0], SchemeItem::Structure(_)));
        assert!(matches!(scheme.items[1], SchemeItem::Arrow(_)));
    }

    // ── Structures on arrow labels ──────────────────────────────────────────

    /// A structure can be placed on an arrow's above/below label, read back, hit-tested and
    /// taken off again — and a text box refuses to open on a structure label.
    #[test]
    fn arrow_label_structures_place_hit_and_refuse_the_text_box() {
        let mut scheme = ReactionScheme::new()
            .structure(demo::benzene())
            .arrow(Arrow::new().below("cat."))
            .structure(demo::benzene());

        // Place biphenyl above the arrow (item 1).
        assert!(set_scheme_arrow_structure(&mut scheme, 1, SchemePart::ArrowAbove, demo::biphenyl()));
        assert!(scheme_arrow_structure(&scheme, 1, SchemePart::ArrowAbove).is_some());
        // A non-arrow item, or a non-arrow part, refuses.
        assert!(!set_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowAbove, demo::benzene()));
        assert!(!set_scheme_arrow_structure(&mut scheme, 1, SchemePart::Text, demo::benzene()));

        let reaction = Reaction::new(scheme, Analysis::new());
        with_figure(&reaction, |layout, top_left| {
            // The above band now covers the drawn structure, and hit-tests to it.
            let rect = scheme_item_rect(layout, top_left, 1, SchemePart::ArrowAbove).unwrap();
            assert_eq!(
                hit_test(&reaction.scheme, layout, top_left, rect.center()),
                Some(Hit::Scheme { index: 1, part: SchemePart::ArrowAbove })
            );
            // And it is a drop slot the drag code can find.
            assert_eq!(
                arrow_slot_at(&reaction.scheme, layout, top_left, rect.center()),
                Some((1, SchemePart::ArrowAbove))
            );
        });

        // A text edit box opens on the (text) label below, but not on the structure above.
        let mut editor = ReactionEditor::new();
        assert!(!editor.begin_edit(
            EditTarget::SchemeText { index: 1, part: SchemePart::ArrowAbove },
            &reaction
        ));
        assert!(editor.begin_edit(
            EditTarget::SchemeText { index: 1, part: SchemePart::ArrowBelow },
            &reaction
        ));

        // Taking the structure off leaves the slot empty.
        let mut scheme = reaction.scheme;
        let taken = take_scheme_arrow_structure(&mut scheme, 1, SchemePart::ArrowAbove);
        assert!(taken.is_some());
        assert!(scheme_arrow_structure(&scheme, 1, SchemePart::ArrowAbove).is_none());
    }

    /// Dropping a *main-line* structure onto an empty arrow slot moves it off the line and
    /// re-derives the columns; an occupied slot is left untouched.
    #[test]
    fn dragging_a_line_structure_onto_an_arrow_slot_moves_it() {
        // [Structure(phenol), Arrow, Structure(anisole)]
        let mut scheme = demo::williamson_ether_synthesis().scheme;
        // Move the product (item 2) up onto the arrow's above slot.
        let landed = move_line_structure_to_arrow_slot(&mut scheme, 2, 1, SchemePart::ArrowAbove);
        assert_eq!(landed, Some(1), "the arrow keeps index 1 (removed item was to its right)");
        assert_eq!(scheme.items.len(), 2, "the structure left the reaction line");
        assert_eq!(
            MolecularFormula::of(scheme_arrow_structure(&scheme, 1, SchemePart::ArrowAbove).unwrap())
                .plain(),
            "C7H8O",
            "anisole now rides above the arrow"
        );
        // The slot is occupied now, so a second drop is refused rather than clobbering it.
        assert!(move_line_structure_to_arrow_slot(&mut scheme, 0, 1, SchemePart::ArrowAbove).is_none());
        // A non-structure source, or a non-arrow target, is refused too.
        assert!(move_line_structure_to_arrow_slot(&mut scheme, 1, 1, SchemePart::ArrowBelow).is_none());
    }

    /// Two arrow-label structures swap when one is dropped on the other (the primitives the
    /// drag handler composes for an arrow-label source).
    #[test]
    fn arrow_label_structures_swap_between_slots() {
        let mut scheme = ReactionScheme::new().arrow(Arrow::new());
        set_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowAbove, demo::benzene());
        set_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowBelow, demo::phenol());

        // Swap above <-> below with the same primitives `drop_into_arrow_slot` uses.
        let src = take_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowAbove).unwrap();
        let displaced = take_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowBelow);
        place_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowBelow, src);
        place_scheme_arrow_structure(&mut scheme, 0, SchemePart::ArrowAbove, displaced.unwrap());

        assert_eq!(
            MolecularFormula::of(scheme_arrow_structure(&scheme, 0, SchemePart::ArrowAbove).unwrap())
                .plain(),
            "C6H6O",
            "phenol moved up"
        );
        assert_eq!(
            MolecularFormula::of(scheme_arrow_structure(&scheme, 0, SchemePart::ArrowBelow).unwrap())
                .plain(),
            "C6H6",
            "benzene moved down"
        );
    }

    /// Dropping a main-line structure onto a slot that already holds one swaps them, losslessly
    /// — neither leaves the figure, and the line keeps its length (課題A).
    #[test]
    fn dragging_a_line_structure_onto_an_occupied_slot_swaps() {
        // [Structure(phenol), Arrow(above = biphenyl), Structure(anisole)]
        let mut scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new().above_structure(demo::biphenyl()))
            .structure(demo::anisole());

        assert!(swap_line_structure_with_arrow_slot(&mut scheme, 0, 1, SchemePart::ArrowAbove));
        assert_eq!(scheme.items.len(), 3, "a swap keeps the reaction line's length");
        // The line position now holds biphenyl; the slot holds the (former line) phenol.
        let SchemeItem::Structure(m) = &scheme.items[0] else { panic!("item 0 is a structure") };
        assert_eq!(MolecularFormula::of(m).plain(), "C12H10", "biphenyl came down onto the line");
        assert_eq!(
            MolecularFormula::of(scheme_arrow_structure(&scheme, 1, SchemePart::ArrowAbove).unwrap())
                .plain(),
            "C6H6O",
            "phenol went up onto the arrow"
        );

        // A text- or empty-occupied slot is not swappable (no structure to trade back).
        let mut text_slot = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new().above("cat."));
        assert!(!swap_line_structure_with_arrow_slot(&mut text_slot, 0, 1, SchemePart::ArrowAbove));
        let mut empty_slot = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new());
        assert!(!swap_line_structure_with_arrow_slot(&mut empty_slot, 0, 1, SchemePart::ArrowAbove));
        // A non-structure source is refused too.
        assert!(!swap_line_structure_with_arrow_slot(&mut scheme, 1, 1, SchemePart::ArrowAbove));
    }

    /// Dragging a structure off an arrow band back onto the reaction line inserts it as a
    /// main-line structure and empties the band (課題B).
    #[test]
    fn dragging_an_arrow_structure_onto_the_line_inserts_it() {
        // [Structure(phenol), Arrow(above = biphenyl), Structure(anisole)]
        let mut scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new().above_structure(demo::biphenyl()))
            .structure(demo::anisole());

        // Drop biphenyl back onto the line at slot 3 (after anisole).
        let landed = move_arrow_structure_to_line(&mut scheme, 1, SchemePart::ArrowAbove, 3);
        assert_eq!(landed, Some(3));
        assert_eq!(scheme.items.len(), 4, "the structure rejoined the line");
        let SchemeItem::Structure(m) = &scheme.items[3] else { panic!("item 3 is a structure") };
        assert_eq!(MolecularFormula::of(m).plain(), "C12H10");
        // The band is empty now.
        assert!(scheme_arrow_structure(&scheme, 1, SchemePart::ArrowAbove).is_none());
        let SchemeItem::Arrow(a) = &scheme.items[1] else { panic!("item 1 is the arrow") };
        assert!(a.above.is_empty(), "the arrow band is back to empty");

        // An empty (or text) band yields nothing to move.
        assert!(move_arrow_structure_to_line(&mut scheme, 1, SchemePart::ArrowAbove, 0).is_none());
    }

    /// The swap keeps the reactant→column mapping aligned: after biphenyl and phenol trade
    /// places, re-syncing re-derives each column from the structure now standing there (課題A+C).
    #[test]
    fn swapping_keeps_reactant_columns_aligned() {
        use crate::reaction::model::{Product, Reactant};
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new().above_structure(demo::biphenyl()))
            .structure(demo::anisole());
        let analysis = Analysis::new()
            .reactant(Reactant::new(demo::phenol()).limiting().sample_mass(Mass::grams(1.0)))
            .reactant(Reactant::new(demo::benzene()).equivalents(1.0)) // the arrow reagent's column
            .product(Product::new(demo::anisole()));
        let mut reaction = Reaction::new(scheme, analysis);
        reaction.sync_structures_from_scheme();
        assert_eq!(reaction.analysis.compute().reactants[1].formula.plain(), "C12H10");

        assert!(swap_line_structure_with_arrow_slot(
            &mut reaction.scheme,
            0,
            1,
            SchemePart::ArrowAbove
        ));
        reaction.sync_structures_from_scheme();
        let a = reaction.analysis.compute();
        assert_eq!(a.reactants[0].formula.plain(), "C12H10", "biphenyl now on the line, column 0");
        assert_eq!(a.reactants[1].formula.plain(), "C6H6O", "phenol now over the arrow, column 1");
    }

    // ── Structure ids travel with their structure ───────────────────────────

    use crate::reaction::StructureId;
    const ABOVE: SchemePart = SchemePart::ArrowAbove;
    const BELOW: SchemePart = SchemePart::ArrowBelow;

    /// Where the structure with `id` sits: a main-line item (`part: Structure`) or an arrow band.
    fn find_scheme_structure(
        scheme: &ReactionScheme,
        id: StructureId,
    ) -> Option<Hit> {
        scheme.items.iter().enumerate().find_map(|(index, item)| match item {
            SchemeItem::Structure(s) if s.id() == id => {
                Some(Hit::Scheme { index, part: SchemePart::Structure })
            }
            SchemeItem::Arrow(a) => [(ABOVE, &a.above), (BELOW, &a.below)]
                .into_iter()
                .find(|(_, l)| matches!(l, ArrowLabel::Structure(s) if s.id() == id))
                .map(|(part, _)| Hit::Scheme { index, part }),
            _ => None,
        })
    }

    fn on_line(index: usize) -> Option<Hit> {
        Some(Hit::Scheme { index, part: SchemePart::Structure })
    }

    fn on_band(index: usize, part: SchemePart) -> Option<Hit> {
        Some(Hit::Scheme { index, part })
    }

    fn line_id(scheme: &ReactionScheme, index: usize) -> StructureId {
        match &scheme.items[index] {
            SchemeItem::Structure(s) => s.id(),
            other => panic!("item {index} is not a structure: {other:?}"),
        }
    }

    fn slot_id(scheme: &ReactionScheme, index: usize, part: SchemePart) -> StructureId {
        match arrow_label(scheme, index, part) {
            Some(ArrowLabel::Structure(s)) => s.id(),
            other => panic!("({index}, {part:?}) holds no structure: {other:?}"),
        }
    }

    /// [Structure(phenol), Arrow(above = biphenyl, below = benzene), Structure(anisole)]
    fn id_scheme() -> ReactionScheme {
        ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(
                Arrow::new()
                    .above_structure(demo::biphenyl())
                    .below_structure(demo::benzene()),
            )
            .structure(demo::anisole())
    }

    #[test]
    fn ids_follow_a_reorder_along_the_line() {
        let mut scheme = id_scheme();
        let (phenol, anisole) = (line_id(&scheme, 0), line_id(&scheme, 2));
        assert_eq!(scheme.move_item(2, -2), Some(0));
        assert_eq!(find_scheme_structure(&scheme, anisole), on_line(0));
        assert_eq!(find_scheme_structure(&scheme, phenol), on_line(1));
    }

    #[test]
    fn ids_follow_a_line_structure_onto_an_empty_slot() {
        let mut scheme = id_scheme();
        let anisole = line_id(&scheme, 2);
        take_scheme_arrow_structure(&mut scheme, 1, ABOVE);
        assert_eq!(move_line_structure_to_arrow_slot(&mut scheme, 2, 1, ABOVE), Some(1));
        assert_eq!(find_scheme_structure(&scheme, anisole), on_band(1, ABOVE));
        assert!(scheme_arrow_structure(&scheme, 1, ABOVE).unwrap() == &demo::anisole());
    }

    /// A target that is not an arrow band is refused before the structure leaves the line, so
    /// it is never lost.
    #[test]
    fn a_line_structure_is_not_lost_on_a_non_band_target() {
        let mut scheme = id_scheme();
        let phenol = line_id(&scheme, 0);
        assert_eq!(move_line_structure_to_arrow_slot(&mut scheme, 0, 1, SchemePart::Text), None);
        assert_eq!(find_scheme_structure(&scheme, phenol), on_line(0));
        assert_eq!(scheme.items.len(), 3);
    }

    #[test]
    fn ids_follow_a_line_slot_swap() {
        let mut scheme = id_scheme();
        let (phenol, biphenyl) = (line_id(&scheme, 0), slot_id(&scheme, 1, ABOVE));
        assert!(swap_line_structure_with_arrow_slot(&mut scheme, 0, 1, ABOVE));
        assert_eq!(find_scheme_structure(&scheme, phenol), on_band(1, ABOVE));
        assert_eq!(find_scheme_structure(&scheme, biphenyl), on_line(0));
    }

    #[test]
    fn ids_follow_an_arrow_structure_back_onto_the_line() {
        let mut scheme = id_scheme();
        let benzene = slot_id(&scheme, 1, BELOW);
        assert_eq!(move_arrow_structure_to_line(&mut scheme, 1, BELOW, 3), Some(3));
        assert_eq!(find_scheme_structure(&scheme, benzene), on_line(3));
        // `take` hands the structure back with its id.
        let biphenyl = slot_id(&scheme, 1, ABOVE);
        let taken = take_scheme_arrow_structure(&mut scheme, 1, ABOVE).unwrap();
        assert_eq!(taken.id(), biphenyl);
        assert!(place_scheme_arrow_structure(&mut scheme, 1, BELOW, taken));
        assert_eq!(find_scheme_structure(&scheme, biphenyl), on_band(1, BELOW));
    }

    /// Slot to slot through the drag handler: a swap of two occupied bands, then a move onto an
    /// empty one.
    #[test]
    fn ids_follow_a_slot_to_slot_drop() {
        let mut reaction = Reaction::new(id_scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let above = slot_id(&reaction.scheme, 1, ABOVE);
        let below = slot_id(&reaction.scheme, 1, BELOW);
        let source = DragSource::ArrowLabel { index: 1, part: ABOVE };

        editor.drop_into_arrow_slot(&mut reaction, source, 1, BELOW);
        assert_eq!(find_scheme_structure(&reaction.scheme, above), on_band(1, BELOW));
        assert_eq!(find_scheme_structure(&reaction.scheme, below), on_band(1, ABOVE));

        // Empty the above band, then drop the below structure onto it.
        let taken = take_scheme_arrow_structure(&mut reaction.scheme, 1, ABOVE).unwrap();
        assert_eq!(taken.id(), below);
        let source = DragSource::ArrowLabel { index: 1, part: BELOW };
        editor.drop_into_arrow_slot(&mut reaction, source, 1, ABOVE);
        assert_eq!(find_scheme_structure(&reaction.scheme, above), on_band(1, ABOVE));
        assert!(arrow_label(&reaction.scheme, 1, BELOW).unwrap().is_empty());

        // A target that is not a band leaves the source where it was.
        let source = DragSource::ArrowLabel { index: 1, part: ABOVE };
        editor.drop_into_arrow_slot(&mut reaction, source, 0, BELOW);
        assert_eq!(find_scheme_structure(&reaction.scheme, above), on_band(1, ABOVE));
    }

    /// The drag handler's line-to-slot paths keep ids too (move onto an empty band, swap with an
    /// occupied one).
    #[test]
    fn ids_follow_a_line_to_slot_drop() {
        let mut reaction = Reaction::new(id_scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let phenol = line_id(&reaction.scheme, 0);
        let biphenyl = slot_id(&reaction.scheme, 1, ABOVE);
        editor.drop_into_arrow_slot(&mut reaction, DragSource::Item(0), 1, ABOVE);
        assert_eq!(find_scheme_structure(&reaction.scheme, phenol), on_band(1, ABOVE));
        assert_eq!(find_scheme_structure(&reaction.scheme, biphenyl), on_line(0));

        take_scheme_arrow_structure(&mut reaction.scheme, 1, BELOW);
        let anisole = line_id(&reaction.scheme, 2);
        editor.drop_into_arrow_slot(&mut reaction, DragSource::Item(2), 1, BELOW);
        assert_eq!(find_scheme_structure(&reaction.scheme, anisole), on_band(1, BELOW));
    }

    /// Redrawing a structure (the canvas edit, or Replace) is an edit of the same structure: its
    /// id stays. A band that held text or nothing gets a new structure with a fresh id.
    #[test]
    fn replacing_a_structure_keeps_its_id() {
        let mut reaction = Reaction::new(id_scheme(), Analysis::new());
        let phenol = line_id(&reaction.scheme, 0);
        assert!(reaction.set_scheme_structure(0, demo::anisole()));
        assert_eq!(line_id(&reaction.scheme, 0), phenol);
        assert!(matches!(&reaction.scheme.items[0], SchemeItem::Structure(s) if *s == demo::anisole()));

        let biphenyl = slot_id(&reaction.scheme, 1, ABOVE);
        assert!(reaction.set_scheme_arrow_structure(1, ABOVE, demo::phenol()));
        assert_eq!(slot_id(&reaction.scheme, 1, ABOVE), biphenyl);
        assert!(scheme_arrow_structure(&reaction.scheme, 1, ABOVE).unwrap() == &demo::phenol());

        // An empty band gets a brand-new structure.
        take_scheme_arrow_structure(&mut reaction.scheme, 1, BELOW);
        assert!(set_scheme_arrow_structure(&mut reaction.scheme, 1, BELOW, demo::benzene()));
        let fresh = slot_id(&reaction.scheme, 1, BELOW);
        assert_ne!(fresh, biphenyl);
        assert_ne!(fresh, phenol);
    }

    #[test]
    fn adding_the_same_molecule_twice_gives_two_ids() {
        let mut scheme = ReactionScheme::new();
        let a = scheme.add(SchemeItem::Structure(demo::phenol().into()));
        let b = scheme.add(SchemeItem::Structure(demo::phenol().into()));
        assert_ne!(line_id(&scheme, a), line_id(&scheme, b));
    }

    // ── The widget ──────────────────────────────────────────────────────────

    /// The whole edit cycle without a window: open a box on a cell, type, commit — and the
    /// derived cells follow.
    #[test]
    fn the_edit_box_commits_and_recomputes() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();
        let cell = CellRef::new(BlockKind::Products, 0, 6); // Measured Mass

        assert!(editor.begin_edit(EditTarget::Cell(cell), &reaction));
        assert_eq!(editor.editing().map(|(_, t)| t), Some("0.7g"));

        editor.set_edit_text("1.00g");
        editor.commit_edit(&mut reaction).unwrap();
        assert!(editor.editing().is_none(), "a good value closes the box");
        assert_eq!(
            reaction.analysis.products[0].measured_mass.map(Mass::as_grams),
            Some(1.0)
        );
        let a = reaction.analysis.compute();
        assert_eq!(format_percent(a.products[0].percent_yield.unwrap().value).joined(), "87.03%");

        // Blank sends the cell back to "not entered".
        assert!(editor.begin_edit(EditTarget::Cell(cell), &reaction));
        editor.set_edit_text("");
        editor.commit_edit(&mut reaction).unwrap();
        assert!(reaction.analysis.products[0].measured_mass.is_none());
        assert!(reaction.analysis.compute().products[0].percent_yield.is_none());

        // Read-only cells refuse to open.
        assert!(!editor.begin_edit(EditTarget::Cell(CellRef::new(BlockKind::Products, 0, 10)), &reaction));
        assert!(!editor.begin_edit(EditTarget::Cell(CellRef::new(BlockKind::Reactants, 0, 2)), &reaction));
        assert!(!editor.begin_edit(EditTarget::SchemeText { index: 0, part: SchemePart::Structure }, &reaction));
    }

    /// Opening a box on an entered cell and committing it untouched writes back exactly the
    /// value that was there — the table rounds to two decimals, the entry must not.
    #[test]
    fn an_untouched_box_leaves_the_entry_exactly_as_it_was() {
        let mut reaction = demo::williamson_ether_synthesis();
        reaction.analysis.reactants[0].equivalents = Some(0.004);
        reaction.analysis.reactants[0].sample_mass = Some(Mass::grams(1.2345));
        reaction.analysis.reactants[0].molarity = Some(0.004);
        reaction.analysis.reactants[0].density = Some(0.789);
        reaction.analysis.reactants[0].volume = Some(0.125);
        reaction.analysis.reactants[0].percent_weight = Some(99.995);
        reaction.analysis.products[0].equivalents = Some(1.125);
        reaction.analysis.products[0].percent_completion = Some(87.125);
        reaction.analysis.products[0].measured_mass = Some(Mass::grams(0.7));
        reaction.analysis.products[0].purity = Some(1.0 / 3.0);
        let before = reaction.analysis.clone();

        let mut editor = ReactionEditor::new();
        for (block, row) in [
            (BlockKind::Reactants, 3),
            (BlockKind::Reactants, 4),
            (BlockKind::Reactants, 5),
            (BlockKind::Reactants, 6),
            (BlockKind::Reactants, 7),
            (BlockKind::Reactants, 8),
            (BlockKind::Products, 2),
            (BlockKind::Products, 3),
            (BlockKind::Products, 6),
            (BlockKind::Products, 7),
        ] {
            let cell = CellRef::new(block, 0, row);
            assert!(editor.begin_edit(EditTarget::Cell(cell), &reaction));
            editor
                .commit_edit(&mut reaction)
                .unwrap_or_else(|e| panic!("{block:?} row {row}: untouched box rejected: {e}"));
        }
        assert_eq!(reaction.analysis.reactants[0].equivalents, before.reactants[0].equivalents);
        assert_eq!(reaction.analysis.reactants[0].sample_mass, before.reactants[0].sample_mass);
        assert_eq!(reaction.analysis.reactants[0].percent_weight, before.reactants[0].percent_weight);
        assert_eq!(reaction.analysis.reactants[0].molarity, before.reactants[0].molarity);
        assert_eq!(reaction.analysis.reactants[0].density, before.reactants[0].density);
        assert_eq!(reaction.analysis.reactants[0].volume, before.reactants[0].volume);
        assert_eq!(reaction.analysis.products[0].equivalents, before.products[0].equivalents);
        assert_eq!(
            reaction.analysis.products[0].percent_completion,
            before.products[0].percent_completion
        );
        assert_eq!(reaction.analysis.products[0].measured_mass, before.products[0].measured_mass);
        assert_eq!(reaction.analysis.products[0].purity, before.products[0].purity);
    }

    /// A rejected value keeps the box open with the offending text, so nothing is lost and
    /// nothing wrong is written.
    #[test]
    fn a_rejected_value_keeps_the_box_open() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();
        let cell = CellRef::new(BlockKind::Reactants, 0, 4);

        editor.begin_edit(EditTarget::Cell(cell), &reaction);
        editor.set_edit_text("two grams");
        assert_eq!(editor.commit_edit(&mut reaction), Err(ParseError::NotANumber));
        assert_eq!(editor.editing().map(|(_, t)| t), Some("two grams"));
        assert_eq!(editor.error(), Some("not a number"));
        assert_eq!(
            reaction.analysis.reactants[0].sample_mass.map(Mass::as_grams),
            Some(1.0),
            "the old value is untouched"
        );

        // Fixing it commits and clears the message.
        editor.set_edit_text("2g");
        editor.commit_edit(&mut reaction).unwrap();
        assert!(editor.editing().is_none());
        assert_eq!(editor.error(), None);
        assert_eq!(reaction.analysis.reactants[0].sample_mass.map(Mass::as_grams), Some(2.0));

        // Escape abandons a bad value without writing it.
        editor.begin_edit(EditTarget::Cell(cell), &reaction);
        editor.set_edit_text("-5g");
        editor.cancel_edit();
        assert_eq!(reaction.analysis.reactants[0].sample_mass.map(Mass::as_grams), Some(2.0));
    }

    /// Editing an arrow label through the box parses its markup.
    #[test]
    fn the_edit_box_writes_scheme_text() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();
        let target = EditTarget::SchemeText { index: 1, part: SchemePart::ArrowAbove };

        assert!(editor.begin_edit(target, &reaction));
        assert_eq!(editor.editing().map(|(_, t)| t), Some("MeI"));
        editor.set_edit_text("Me_2SO_4");
        editor.commit_edit(&mut reaction).unwrap();
        let SchemeItem::Arrow(arrow) = &reaction.scheme.items[1] else {
            panic!("item 1 is the arrow")
        };
        let above = arrow.above.as_text().expect("the label above is text");
        assert_eq!(
            above.spans.iter().map(|s| s.text.as_str()).collect::<String>(),
            "Me2SO4"
        );
    }

    /// Through the widget, with a real pointer: clicking a cell of the drawn table opens the
    /// box on *that* cell, and clicking a computed one only selects it.
    #[test]
    fn clicking_the_drawn_figure_opens_the_box_on_that_cell() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();

        /// One frame of the editor, returning the figure it drew: the measured layout and
        /// the corner it was drawn at, which is what turns a [`CellRef`] into a click target.
        fn frame(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            events: Vec<egui::Event>,
        ) -> (FigureLayout, Pos2) {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            };
            let mut view = ReactionView::new();
            let mut figure = None;
            let _ = ctx.run_ui(raw, |ui| {
                // Measure the same figure the editor is about to draw, with the same fonts.
                let layout = view.measure(ui.painter(), reaction);
                let origin = editor.show(ui, reaction).rect.min;
                figure = Some((layout, origin));
            });
            figure.expect("the closure runs exactly once")
        }

        /// Move onto `target`, then press and release there. egui resolves a click against a
        /// widget rect it has already seen, so the move needs a frame of its own.
        fn click_at(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            target: Pos2,
        ) {
            let button = |pressed| egui::Event::PointerButton {
                pos: target,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(target)]);
            frame(ctx, editor, reaction, vec![button(true), button(false)]);
        }

        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let cell = CellRef::new(BlockKind::Reactants, 1, 3); // MeI, Equivalents
        click_at(
            &ctx,
            &mut editor,
            &mut reaction,
            cell_rect(&layout, origin, cell).unwrap().center(),
        );
        assert_eq!(editor.selection(), Some(Hit::Cell(cell)));
        assert_eq!(
            editor.editing().map(|(t, text)| (t, text.to_string())),
            Some((EditTarget::Cell(cell), "1.1".to_string())),
            "clicking Equivalents opens it seeded with what is entered there"
        );

        // Straight on to a computed cell — the open box must not swallow the click, and the
        // cell it lands on selects without opening anything.
        let moles = CellRef::new(BlockKind::Reactants, 1, 9);
        click_at(
            &ctx,
            &mut editor,
            &mut reaction,
            cell_rect(&layout, origin, moles).unwrap().center(),
        );
        assert_eq!(editor.selection(), Some(Hit::Cell(moles)));
        assert!(editor.editing().is_none(), "Reactant Moles is computed");
    }

    /// Through the widget, with a real pointer: a drag that reorders the scheme is reported to
    /// the host exactly once, so a host holding a positional [`StructureRef`] (e.g. a canvas
    /// link) can tell its index went stale. A plain click reports nothing.
    #[test]
    fn a_drag_reorder_is_reported_to_the_host() {
        let mut reaction = demo::williamson_ether_synthesis();
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();

        fn frame(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            events: Vec<egui::Event>,
        ) -> (FigureLayout, Pos2) {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            };
            let mut view = ReactionView::new();
            let mut figure = None;
            let _ = ctx.run_ui(raw, |ui| {
                let layout = view.measure(ui.painter(), reaction);
                let origin = editor.show(ui, reaction).rect.min;
                figure = Some((layout, origin));
            });
            figure.expect("the closure runs exactly once")
        }
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };

        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let first = scheme_item_rect(&layout, origin, 0, SchemePart::Structure).unwrap();
        let last = scheme_item_rect(&layout, origin, 2, SchemePart::Structure).unwrap();
        let (phenol, anisole) = (line_id(&reaction.scheme, 0), line_id(&reaction.scheme, 2));

        // A click on a structure selects it but leaves the scheme as it was.
        let start = last.center();
        frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(start)]);
        frame(&ctx, &mut editor, &mut reaction, vec![button(start, true), button(start, false)]);
        assert!(!editor.take_scheme_rearranged(), "a click does not rearrange the scheme");

        // Drag the product (anisole) to the far left, ahead of phenol.
        let end = Pos2::new(first.min.x + 2.0, first.center().y);
        // egui picks the item up at wherever the pointer is when it first crosses the drag
        // threshold, so nudge it off the press point while still over the product.
        let nudge = start + Vec2::new(-12.0, 0.0);
        frame(&ctx, &mut editor, &mut reaction, vec![button(start, true)]);
        frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(nudge)]);
        frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(end)]);
        assert!(editor.is_dragging(), "the press-and-move picked the product up");
        frame(&ctx, &mut editor, &mut reaction, vec![button(end, false)]);

        assert!(!editor.is_dragging());
        assert!(
            matches!(&reaction.scheme.items[0], SchemeItem::Structure(m) if *m == demo::anisole()),
            "the drag moved anisole to the front"
        );
        // Each structure took its id along.
        assert_eq!(line_id(&reaction.scheme, 0), anisole);
        assert_eq!(find_scheme_structure(&reaction.scheme, phenol), on_line(1));
        assert!(editor.take_scheme_rearranged(), "the reorder is reported");
        assert!(!editor.take_scheme_rearranged(), "and only once");
    }

    /// Through the widget, with a real pointer: only a dragged *structure* is offered an arrow
    /// slot. A `+` (or the arrow itself, picked up by one of its own empty bands) passing over a
    /// band is still a reorder — no slot is targeted, and letting go moves it along the line.
    /// A structure over the same band still targets it and lands there.
    #[test]
    fn only_a_dragged_structure_targets_an_arrow_slot() {
        fn frame(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            events: Vec<egui::Event>,
        ) -> (FigureLayout, Pos2) {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            };
            let mut view = ReactionView::new();
            let mut figure = None;
            let _ = ctx.run_ui(raw, |ui| {
                let layout = view.measure(ui.painter(), reaction);
                let origin = editor.show(ui, reaction).rect.min;
                figure = Some((layout, origin));
            });
            figure.expect("the closure runs exactly once")
        }
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        /// Press at `start`, cross the drag threshold at `nudge`, move to `end`, and return
        /// the slot the drag is targeting there (before letting go).
        fn drag_to(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            start: Pos2,
            nudge: Pos2,
            end: Pos2,
        ) -> Option<(usize, SchemePart)> {
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(start)]);
            frame(ctx, editor, reaction, vec![button(start, true)]);
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(nudge)]);
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(end)]);
            assert!(editor.is_dragging(), "the press-and-move picked the item up");
            editor.drag.and_then(|d| d.to_slot)
        }
        let scheme = || {
            ReactionScheme::new()
                .structure(demo::phenol())
                .plus()
                .structure(demo::benzene())
                .arrow(Arrow::new())
                .structure(demo::anisole())
        };

        // A `+` dragged over the arrow's empty above band: no slot, and the drop reorders.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let plus = scheme_item_rect(&layout, origin, 1, SchemePart::Plus).unwrap();
        let band = scheme_item_rect(&layout, origin, 3, SchemePart::ArrowAbove).unwrap();
        // Just right of the arrow's centre, so four items sit left of the pointer.
        let end = band.center() + Vec2::new(4.0, 0.0);
        assert_eq!(scheme_drop_index(&layout, origin, end), 4);
        let start = plus.left_center() + Vec2::new(1.0, 0.0);
        let nudge = plus.right_center() - Vec2::new(1.0, 0.0);
        let to_slot = drag_to(&ctx, &mut editor, &mut reaction, start, nudge, end);
        assert_eq!(to_slot, None, "a `+` is not offered the arrow slot");
        frame(&ctx, &mut editor, &mut reaction, vec![button(end, false)]);
        assert!(!editor.is_dragging());
        assert!(
            matches!(reaction.scheme.items[3], SchemeItem::Plus),
            "the `+` reordered to just after the arrow"
        );
        let SchemeItem::Arrow(a) = &reaction.scheme.items[2] else { panic!("item 2 is the arrow") };
        assert!(a.above.is_empty(), "the band was left alone");
        assert!(editor.take_scheme_rearranged(), "the reorder is reported");

        // The arrow itself, picked up by its own empty above band and held there: its own band
        // is not a drop target, and letting go in place moves nothing.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let band = scheme_item_rect(&layout, origin, 3, SchemePart::ArrowAbove).unwrap();
        let start = band.center() - Vec2::new(10.0, 0.0);
        let end = band.center() - Vec2::new(1.0, 0.0);
        let to_slot = drag_to(&ctx, &mut editor, &mut reaction, start, end, end);
        assert_eq!(to_slot, None, "the arrow is not offered its own band");
        frame(&ctx, &mut editor, &mut reaction, vec![button(end, false)]);
        assert_eq!(reaction.scheme.items.len(), 5);
        assert!(matches!(reaction.scheme.items[1], SchemeItem::Plus), "nothing moved");
        let SchemeItem::Arrow(a) = &reaction.scheme.items[3] else { panic!("item 3 is the arrow") };
        assert!(a.above.is_empty() && a.below.is_empty(), "the bands were left alone");
        assert!(!editor.take_scheme_rearranged(), "dropped back in place");

        // A structure over the same band still targets it, and lands on it.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let benzene = scheme_item_rect(&layout, origin, 2, SchemePart::Structure).unwrap();
        let band = scheme_item_rect(&layout, origin, 3, SchemePart::ArrowAbove).unwrap();
        let start = benzene.center();
        let to_slot = drag_to(
            &ctx,
            &mut editor,
            &mut reaction,
            start,
            start + Vec2::new(12.0, 0.0),
            band.center(),
        );
        assert_eq!(to_slot, Some((3, SchemePart::ArrowAbove)));
        frame(&ctx, &mut editor, &mut reaction, vec![button(band.center(), false)]);
        assert!(
            scheme_arrow_structure(&reaction.scheme, 2, SchemePart::ArrowAbove)
                .is_some_and(|m| *m == demo::benzene()),
            "benzene now rides above the arrow"
        );
        assert!(editor.take_scheme_rearranged());
    }

    /// Through the widget, with a real pointer: after a drag reorders a scheme item, the
    /// selection names the landed item with the part [`hit_test`] gives that kind of item — a
    /// `+` is selected as `Plus`, a text item as `Text`, an arrow as its above band — not as a
    /// `Structure`, which would turn on the host's structure-only actions for a non-structure.
    #[test]
    fn a_drag_reorder_selects_the_landed_item_by_its_kind() {
        fn frame(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            events: Vec<egui::Event>,
        ) -> (FigureLayout, Pos2) {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            };
            let mut view = ReactionView::new();
            let mut figure = None;
            let _ = ctx.run_ui(raw, |ui| {
                let layout = view.measure(ui.painter(), reaction);
                let origin = editor.show(ui, reaction).rect.min;
                figure = Some((layout, origin));
            });
            figure.expect("the closure runs exactly once")
        }
        /// Press at `start`, cross the drag threshold at `nudge`, let go at `end`.
        fn drag(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            start: Pos2,
            nudge: Pos2,
            end: Pos2,
        ) {
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(start)]);
            frame(ctx, editor, reaction, vec![button(start, true)]);
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(nudge)]);
            frame(ctx, editor, reaction, vec![egui::Event::PointerMoved(end)]);
            assert!(editor.is_dragging(), "the press-and-move picked the item up");
            frame(ctx, editor, reaction, vec![button(end, false)]);
            assert!(!editor.is_dragging());
        }
        // phenol + benzene → anisole  heat
        let scheme = || {
            ReactionScheme::new()
                .structure(demo::phenol())
                .plus()
                .structure(demo::benzene())
                .arrow(Arrow::new())
                .structure(demo::anisole())
                .text("heat")
        };

        // The `+` dragged to the front of the line.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let first = scheme_item_rect(&layout, origin, 0, SchemePart::Structure).unwrap();
        let front = Pos2::new(first.min.x + 2.0, first.center().y);
        let plus = scheme_item_rect(&layout, origin, 1, SchemePart::Plus).unwrap();
        let start = plus.right_center() - Vec2::new(1.0, 0.0);
        let nudge = plus.left_center() + Vec2::new(1.0, 0.0);
        drag(&ctx, &mut editor, &mut reaction, start, nudge, front);
        assert!(matches!(reaction.scheme.items[0], SchemeItem::Plus), "the `+` moved to the front");
        assert_eq!(editor.selection(), Some(Hit::Scheme { index: 0, part: SchemePart::Plus }));

        // The text item dragged to the front of the line.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let text = scheme_item_rect(&layout, origin, 5, SchemePart::Text).unwrap();
        let start = text.right_center() - Vec2::new(1.0, 0.0);
        let nudge = text.left_center() + Vec2::new(1.0, 0.0);
        drag(&ctx, &mut editor, &mut reaction, start, nudge, front);
        assert!(
            matches!(reaction.scheme.items[0], SchemeItem::Text(_)),
            "the text moved to the front"
        );
        assert_eq!(editor.selection(), Some(Hit::Scheme { index: 0, part: SchemePart::Text }));

        // The arrow, picked up by its empty above band, dragged to the front of the line.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let band = scheme_item_rect(&layout, origin, 3, SchemePart::ArrowAbove).unwrap();
        let start = band.center();
        let nudge = start - Vec2::new(12.0, 0.0);
        drag(&ctx, &mut editor, &mut reaction, start, nudge, front);
        assert!(
            matches!(reaction.scheme.items[0], SchemeItem::Arrow(_)),
            "the arrow moved to the front"
        );
        assert_eq!(
            editor.selection(),
            Some(Hit::Scheme { index: 0, part: SchemePart::ArrowAbove })
        );
        // Nothing structure-only acts on it.
        assert_eq!(editor.cleanup_selected_structure(&mut reaction), None);

        // A structure still lands selected as a structure.
        let mut reaction = Reaction::new(scheme(), Analysis::new());
        let mut editor = ReactionEditor::new();
        let ctx = egui::Context::default();
        let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
        let anisole = scheme_item_rect(&layout, origin, 4, SchemePart::Structure).unwrap();
        let start = anisole.center();
        let nudge = start - Vec2::new(12.0, 0.0);
        drag(&ctx, &mut editor, &mut reaction, start, nudge, front);
        assert!(
            matches!(&reaction.scheme.items[0], SchemeItem::Structure(m) if *m == demo::anisole()),
            "anisole moved to the front"
        );
        assert_eq!(editor.selection(), Some(Hit::Scheme { index: 0, part: SchemePart::Structure }));
    }

    /// Through the widget, with a real pointer and keyboard: picking up a scheme item while the
    /// edit box is open leaves the box the same way clicking away does — what was typed is
    /// committed (a bad value is dropped with its message kept), the box closes and hands the
    /// keyboard back. Nothing typed is silently thrown away.
    #[test]
    fn dragging_a_scheme_item_commits_the_open_box() {
        fn frame(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            events: Vec<egui::Event>,
        ) -> (FigureLayout, Pos2) {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            };
            let mut view = ReactionView::new();
            let mut figure = None;
            let _ = ctx.run_ui(raw, |ui| {
                let layout = view.measure(ui.painter(), reaction);
                let origin = editor.show(ui, reaction).rect.min;
                figure = Some((layout, origin));
            });
            figure.expect("the closure runs exactly once")
        }

        /// On a fresh demo reaction: click `cell`, type `typed` into the box that opens, then
        /// drag the product to the front of the line.
        fn type_then_drag(cell: CellRef, typed: &str) -> (Reaction, ReactionEditor, egui::Context) {
            let mut reaction = demo::williamson_ether_synthesis();
            let mut editor = ReactionEditor::new();
            let ctx = egui::Context::default();
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };

            let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
            let at = cell_rect(&layout, origin, cell).unwrap().center();
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(at)]);
            frame(&ctx, &mut editor, &mut reaction, vec![button(at, true), button(at, false)]);
            assert_eq!(editor.editing().map(|(t, _)| t), Some(EditTarget::Cell(cell)));
            // A frame for the box to take focus (and select its seeded text), then type over it.
            frame(&ctx, &mut editor, &mut reaction, Vec::new());
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::Text(typed.to_string())]);
            assert_eq!(editor.editing().map(|(_, t)| t), Some(typed), "the typing landed");
            assert!(ctx.memory(|m| m.focused()).is_some(), "the box has the keyboard");

            let first = scheme_item_rect(&layout, origin, 0, SchemePart::Structure).unwrap();
            let last = scheme_item_rect(&layout, origin, 2, SchemePart::Structure).unwrap();
            let start = last.center();
            let nudge = start + Vec2::new(-12.0, 0.0);
            let end = Pos2::new(first.min.x + 2.0, first.center().y);
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(start)]);
            frame(&ctx, &mut editor, &mut reaction, vec![button(start, true)]);
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(nudge)]);
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(end)]);
            assert!(editor.is_dragging(), "the press-and-move picked the product up");
            frame(&ctx, &mut editor, &mut reaction, vec![button(end, false)]);
            assert!(!editor.is_dragging());
            assert!(editor.take_scheme_rearranged(), "the drag reordered the line");
            (reaction, editor, ctx)
        }

        // A good value is written, exactly as clicking away would write it.
        let sample_mass = CellRef::new(BlockKind::Reactants, 0, 4);
        let (reaction, editor, ctx) = type_then_drag(sample_mass, "2.5g");
        assert_eq!(
            reaction.analysis.reactants[0].sample_mass.map(Mass::as_grams),
            Some(2.5),
            "the typed Sample Mass reached the analysis"
        );
        assert!(editor.editing().is_none(), "the box closed");
        assert_eq!(editor.error(), None);
        assert_eq!(ctx.memory(|m| m.focused()), None, "the box handed the keyboard back");

        // A value that does not parse closes the box too, leaving the old value and saying why.
        let (reaction, editor, ctx) = type_then_drag(sample_mass, "two grams");
        assert_eq!(reaction.analysis.reactants[0].sample_mass.map(Mass::as_grams), Some(1.0));
        assert!(editor.editing().is_none(), "a drag never leaves the box open behind it");
        assert_eq!(editor.error(), Some("not a number"));
        assert_eq!(ctx.memory(|m| m.focused()), None);
    }

    /// Through the widget: the interactive path allocates and paints exactly what the plain
    /// view does, so switching to it cannot change the figure.
    #[test]
    fn the_widget_paints_the_same_figure_as_the_plain_view() {
        fn texts(output: &egui::FullOutput) -> Vec<String> {
            fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                    egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                    _ => {}
                }
            }
            let mut out = Vec::new();
            for clipped in &output.shapes {
                walk(&clipped.shape, &mut out);
            }
            out
        }
        let raw = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };

        let reaction = demo::williamson_ether_synthesis();
        let ctx = egui::Context::default();
        let mut view = ReactionView::new();
        let mut plain_rect = Rect::NOTHING;
        let plain = ctx.run_ui(raw(), |ui| plain_rect = view.show(ui, &reaction).rect);

        let mut reaction = reaction;
        let ctx = egui::Context::default();
        let mut editor = ReactionEditor::new();
        let mut edit_rect = Rect::NOTHING;
        let edited = ctx.run_ui(raw(), |ui| {
            edit_rect = editor.show(ui, &mut reaction).rect;
        });

        assert!((plain_rect.size() - edit_rect.size()).length() < 0.1);
        assert_eq!(
            texts(&plain),
            texts(&edited),
            "an idle editor must draw exactly the plain figure"
        );
    }

    // ── Entered values stay with their compound through every move ──────────
    //
    // Each compound below carries values of its own, so a column that ends up beside the wrong
    // structure (or a column left in place while its structure moved) shows up as a mismatch
    // between the column's name, its molecule, its values and the structure its id points at.
    // None of these moves crosses the arrow: every reactant stays a reactant.

    /// One reactant column as it was entered, keyed by its name.
    struct Entered {
        name: &'static str,
        molecule: fn() -> Molecule,
        limiting: bool,
        sample_mass: Option<f64>,
        equivalents: Option<f64>,
        /// Whether a scheme structure draws it (K₂CO₃ is only a table column).
        drawn: bool,
    }

    const ENTERED: [Entered; 5] = [
        Entered {
            name: "phenol",
            molecule: demo::phenol,
            limiting: true,
            sample_mass: Some(1.0),
            equivalents: None,
            drawn: true,
        },
        Entered {
            name: "benzene",
            molecule: demo::benzene,
            limiting: false,
            sample_mass: Some(3.0),
            equivalents: Some(2.0),
            drawn: true,
        },
        Entered {
            name: "biphenyl",
            molecule: demo::biphenyl,
            limiting: false,
            sample_mass: None,
            equivalents: Some(0.1),
            drawn: true,
        },
        Entered {
            name: "MeI",
            molecule: demo::methyl_iodide,
            limiting: false,
            sample_mass: None,
            equivalents: Some(1.5),
            drawn: true,
        },
        Entered {
            name: "K2CO3",
            molecule: demo::potassium_carbonate,
            limiting: false,
            sample_mass: None,
            equivalents: Some(3.0),
            drawn: false,
        },
    ];

    /// `phenol + benzene →(above: biphenyl, below: MeI or nothing) anisole`, with a column for
    /// each structure (linked by [`Reaction::new`]) plus an undrawn K₂CO₃ column at the end.
    fn valued_reaction(mei_below: bool) -> Reaction {
        let mut arrow = Arrow::new().above_structure(demo::biphenyl());
        if mei_below {
            arrow = arrow.below_structure(demo::methyl_iodide());
        }
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::benzene())
            .arrow(arrow)
            .structure(demo::anisole());
        let mut analysis = Analysis::new();
        for e in ENTERED.iter().filter(|e| mei_below || e.name != "MeI") {
            let mut r = Reactant::new((e.molecule)()).name(e.name);
            r.limiting = e.limiting;
            r.sample_mass = e.sample_mass.map(Mass::grams);
            r.equivalents = e.equivalents;
            analysis.add_reactant(r);
        }
        analysis.add_product(
            Product::new(demo::anisole()).name("anisole").measured_mass(Mass::grams(0.7)),
        );
        Reaction::new(scheme, analysis)
    }

    /// The molecule the scheme draws for `id`, on the line or on an arrow band.
    fn drawn_molecule(scheme: &ReactionScheme, id: StructureId) -> Option<&Molecule> {
        match find_scheme_structure(scheme, id)? {
            Hit::Scheme { index, part: SchemePart::Structure } => match &scheme.items[index] {
                SchemeItem::Structure(s) => Some(s.molecule()),
                _ => None,
            },
            Hit::Scheme { index, part } => scheme_arrow_structure(scheme, index, part),
            Hit::Cell(_) => None,
        }
    }

    /// The reactant columns read `order` (by name), and every column still holds its own
    /// compound's molecule and entered values, linked to the structure that draws it.
    fn assert_values_follow(reaction: &Reaction, order: &[&str]) {
        let names: Vec<_> =
            reaction.analysis.reactants.iter().map(|r| r.name.as_deref().unwrap_or("?")).collect();
        assert_eq!(names, order, "the reactant columns are in scheme order");
        for r in &reaction.analysis.reactants {
            let name = r.name.as_deref().unwrap();
            let e = ENTERED.iter().find(|e| e.name == name).unwrap();
            assert!(r.molecule == (e.molecule)(), "{name}: the column kept its molecule");
            assert_eq!(r.limiting, e.limiting, "{name}: limiting stayed with the compound");
            assert_eq!(
                r.sample_mass.map(Mass::as_grams),
                e.sample_mass,
                "{name}: sample mass stayed with the compound"
            );
            assert_eq!(r.equivalents, e.equivalents, "{name}: equivalents stayed with the compound");
            match r.structure {
                Some(id) => {
                    assert!(e.drawn, "{name}: an undrawn column was never linked");
                    assert!(
                        drawn_molecule(&reaction.scheme, id) == Some(&r.molecule),
                        "{name}: the column is linked to the structure that draws it"
                    );
                }
                None => assert!(!e.drawn, "{name}: the column lost its structure"),
            }
        }
        let [p] = reaction.analysis.products.as_slice() else { panic!("one product column") };
        assert_eq!(p.name.as_deref(), Some("anisole"));
        assert!(p.molecule == demo::anisole());
        assert_eq!(p.measured_mass.map(Mass::as_grams), Some(0.7));
        let id = p.structure.expect("the product stays linked");
        assert!(drawn_molecule(&reaction.scheme, id) == Some(&demo::anisole()));
    }

    /// The fixture itself: [`Reaction::new`] links by position, in scheme order.
    #[test]
    fn valued_reaction_starts_linked_in_scheme_order() {
        assert_values_follow(
            &valued_reaction(true),
            &["phenol", "benzene", "biphenyl", "MeI", "K2CO3"],
        );
        assert_values_follow(&valued_reaction(false), &["phenol", "benzene", "biphenyl", "K2CO3"]);
    }

    /// The edit-layer moves, followed by the resync the widget does after each of them.
    #[test]
    fn values_follow_their_compound_through_the_edit_layer_moves() {
        // Along the line: benzene to the front.
        let mut reaction = valued_reaction(true);
        assert_eq!(reaction.scheme.move_item(2, -2), Some(0));
        reaction.sync_structures_from_scheme();
        assert_values_follow(&reaction, &["benzene", "phenol", "biphenyl", "MeI", "K2CO3"]);

        // Line → empty slot: phenol onto the empty below band (the arrow shifts to index 2).
        let mut reaction = valued_reaction(false);
        assert_eq!(move_line_structure_to_arrow_slot(&mut reaction.scheme, 0, 3, BELOW), Some(2));
        reaction.sync_structures_from_scheme();
        assert_values_follow(&reaction, &["benzene", "biphenyl", "phenol", "K2CO3"]);

        // Line ↔ occupied slot: phenol and biphenyl trade places.
        let mut reaction = valued_reaction(true);
        assert!(swap_line_structure_with_arrow_slot(&mut reaction.scheme, 0, 3, ABOVE));
        reaction.sync_structures_from_scheme();
        assert_values_follow(&reaction, &["biphenyl", "benzene", "phenol", "MeI", "K2CO3"]);

        // Slot → line: MeI to the front of the line.
        let mut reaction = valued_reaction(true);
        assert_eq!(move_arrow_structure_to_line(&mut reaction.scheme, 3, BELOW, 0), Some(0));
        reaction.sync_structures_from_scheme();
        assert_values_follow(&reaction, &["MeI", "phenol", "benzene", "biphenyl", "K2CO3"]);
    }

    /// The drop handler's own paths: line onto an empty slot, line swapped with an occupied
    /// slot, slot swapped with slot, slot onto an empty slot, and slot back onto the line.
    #[test]
    fn values_follow_their_compound_through_drop_into_arrow_slot() {
        let mut editor = ReactionEditor::new();

        let mut reaction = valued_reaction(false);
        editor.drop_into_arrow_slot(&mut reaction, DragSource::Item(0), 3, BELOW);
        assert_values_follow(&reaction, &["benzene", "biphenyl", "phenol", "K2CO3"]);

        let mut reaction = valued_reaction(true);
        editor.drop_into_arrow_slot(&mut reaction, DragSource::Item(2), 3, ABOVE);
        assert_values_follow(&reaction, &["phenol", "biphenyl", "benzene", "MeI", "K2CO3"]);

        let mut reaction = valued_reaction(true);
        let above = DragSource::ArrowLabel { index: 3, part: ABOVE };
        editor.drop_into_arrow_slot(&mut reaction, above, 3, BELOW);
        assert_values_follow(&reaction, &["phenol", "benzene", "MeI", "biphenyl", "K2CO3"]);

        let mut reaction = valued_reaction(false);
        editor.drop_into_arrow_slot(&mut reaction, above, 3, BELOW);
        assert!(arrow_label(&reaction.scheme, 3, ABOVE).unwrap().is_empty());
        assert_values_follow(&reaction, &["phenol", "benzene", "biphenyl", "K2CO3"]);

        let mut reaction = valued_reaction(true);
        editor.drop_arrow_structure_onto_line(&mut reaction, 3, BELOW, 0);
        assert_values_follow(&reaction, &["MeI", "phenol", "benzene", "biphenyl", "K2CO3"]);
    }

    /// Through the widget, with a real pointer: every kind of drag — a reorder along the line,
    /// a line structure onto an empty or an occupied arrow slot, slot to slot, and slot back
    /// onto the line — leaves limiting, sample mass and equivalents with the same compound.
    #[test]
    fn values_follow_their_compound_through_a_widget_drag() {
        fn frame(
            ctx: &egui::Context,
            editor: &mut ReactionEditor,
            reaction: &mut Reaction,
            events: Vec<egui::Event>,
        ) -> (FigureLayout, Pos2) {
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
                events,
                focused: true,
                ..Default::default()
            };
            let mut view = ReactionView::new();
            let mut figure = None;
            let _ = ctx.run_ui(raw, |ui| {
                let layout = view.measure(ui.painter(), reaction);
                let origin = editor.show(ui, reaction).rect.min;
                figure = Some((layout, origin));
            });
            figure.expect("the closure runs exactly once")
        }
        /// On a fresh [`valued_reaction`]: press where `from` says, cross the drag threshold
        /// `nudge` away, let go where `to` says, and hand back the reaction.
        fn drag(
            mei_below: bool,
            from: impl Fn(&FigureLayout, Pos2) -> Pos2,
            nudge: Vec2,
            to: impl Fn(&FigureLayout, Pos2) -> Pos2,
        ) -> Reaction {
            let mut reaction = valued_reaction(mei_below);
            let mut editor = ReactionEditor::new();
            let ctx = egui::Context::default();
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            let (layout, origin) = frame(&ctx, &mut editor, &mut reaction, Vec::new());
            let (start, end) = (from(&layout, origin), to(&layout, origin));
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(start)]);
            frame(&ctx, &mut editor, &mut reaction, vec![button(start, true)]);
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(start + nudge)]);
            frame(&ctx, &mut editor, &mut reaction, vec![egui::Event::PointerMoved(end)]);
            assert!(editor.is_dragging(), "the press-and-move picked the item up");
            frame(&ctx, &mut editor, &mut reaction, vec![button(end, false)]);
            assert!(!editor.is_dragging());
            assert!(editor.take_scheme_rearranged(), "the drag changed the scheme");
            reaction
        }
        let center = |index, part| {
            move |l: &FigureLayout, o: Pos2| scheme_item_rect(l, o, index, part).unwrap().center()
        };
        // The front of the line, over phenol (not over an arrow band).
        let front = |l: &FigureLayout, o: Pos2| {
            let first = scheme_item_rect(l, o, 0, SchemePart::Structure).unwrap();
            Pos2::new(first.min.x + 2.0, first.center().y)
        };
        let line_draws = |r: &Reaction, index: usize, m: Molecule| {
            matches!(&r.scheme.items[index], SchemeItem::Structure(s) if *s == m)
        };
        let band_draws = |r: &Reaction, index: usize, part: SchemePart, m: Molecule| {
            scheme_arrow_structure(&r.scheme, index, part) == Some(&m)
        };
        let left = Vec2::new(-12.0, 0.0);
        let right = Vec2::new(12.0, 0.0);

        // Reorder along the line: benzene to the front.
        let reaction = drag(true, center(2, SchemePart::Structure), left, front);
        assert!(line_draws(&reaction, 0, demo::benzene()));
        assert_values_follow(&reaction, &["benzene", "phenol", "biphenyl", "MeI", "K2CO3"]);

        // Line → empty slot: phenol onto the arrow's empty below band.
        let reaction = drag(false, center(0, SchemePart::Structure), right, center(3, BELOW));
        assert!(band_draws(&reaction, 2, BELOW, demo::phenol()));
        assert_values_follow(&reaction, &["benzene", "biphenyl", "phenol", "K2CO3"]);

        // Line ↔ occupied slot: phenol onto biphenyl's band, biphenyl back onto the line.
        let reaction = drag(true, center(0, SchemePart::Structure), right, center(3, ABOVE));
        assert!(band_draws(&reaction, 3, ABOVE, demo::phenol()));
        assert_values_follow(&reaction, &["biphenyl", "benzene", "phenol", "MeI", "K2CO3"]);

        // Slot ↔ slot: biphenyl (above) dropped on MeI (below); the two trade bands.
        let reaction = drag(true, center(3, ABOVE), left, center(3, BELOW));
        assert!(band_draws(&reaction, 3, ABOVE, demo::methyl_iodide()));
        assert_values_follow(&reaction, &["phenol", "benzene", "MeI", "biphenyl", "K2CO3"]);

        // Slot → line: MeI off its band to the front of the line.
        let reaction = drag(true, center(3, BELOW), left, front);
        assert!(line_draws(&reaction, 0, demo::methyl_iodide()));
        assert_values_follow(&reaction, &["MeI", "phenol", "benzene", "biphenyl", "K2CO3"]);
    }
}
