//! Drawing style for the reaction figure. Same shape as [`StyleConfig`](crate::config::StyleConfig):
//! every field has a `#[serde(default)]`, so a user config may set only what it cares about.
//!
//! All lengths are screen pixels.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionStyle {
    // ── Scheme ───────────────────────────────────────────────────────────────
    /// On-screen length of one bond in a scheme structure. Structures are scaled to this
    /// (the editor's own zoom is not involved), so label size and line widths stay absolute.
    #[serde(default = "def_bond_length")]
    pub bond_length: f32,
    /// Horizontal gap between scheme items (structure, `+`, arrow).
    #[serde(default = "def_item_gap")]
    pub item_gap: f32,
    /// Shaft length of a reaction arrow, before it is widened to fit its labels.
    #[serde(default = "def_arrow_length")]
    pub arrow_length: f32,
    /// Thickness of the arrow shaft.
    #[serde(default = "def_arrow_width")]
    pub arrow_width: f32,
    /// Length of the (filled) arrow head.
    #[serde(default = "def_arrow_head_length")]
    pub arrow_head_length: f32,
    /// Half-height of the arrow head.
    #[serde(default = "def_arrow_head_half_width")]
    pub arrow_head_half_width: f32,
    /// Gap between the shaft and the text above / below it.
    #[serde(default = "def_arrow_text_gap")]
    pub arrow_text_gap: f32,
    /// Font size of the text above / below the arrow.
    #[serde(default = "def_arrow_font_size")]
    pub arrow_font_size: f32,
    /// Font size of the `+` between reagents.
    #[serde(default = "def_plus_font_size")]
    pub plus_font_size: f32,
    /// Font size of a free-standing text item in the scheme.
    #[serde(default = "def_scheme_font_size")]
    pub scheme_font_size: f32,
    /// Font size of a compound label under a structure (`(I)`, **4**).
    #[serde(default = "def_compound_label_font_size")]
    pub compound_label_font_size: f32,
    /// Gap between a structure and its compound label.
    #[serde(default = "def_compound_label_gap")]
    pub compound_label_gap: f32,
    /// Vertical gap between two rows of a scheme.
    #[serde(default = "def_row_gap")]
    pub row_gap: f32,
    /// Widest a scheme row may grow before the next item starts a new row. `None` wraps only
    /// at the scheme's line breaks.
    #[serde(default)]
    pub max_row_width: Option<f32>,

    // ── Table ────────────────────────────────────────────────────────────────
    /// Font size of the table cells.
    #[serde(default = "def_table_font_size")]
    pub table_font_size: f32,
    /// Size of a trailing unit (`g`, `mmol`, `%`) relative to the cell font.
    #[serde(default = "def_unit_scale")]
    pub unit_scale: f32,
    /// Horizontal padding inside a table cell.
    #[serde(default = "def_cell_pad_x")]
    pub cell_pad_x: f32,
    /// Vertical padding inside a table cell.
    #[serde(default = "def_cell_pad_y")]
    pub cell_pad_y: f32,
    /// Minimum width of a value column, so narrow columns still read as columns.
    #[serde(default = "def_min_column_width")]
    pub min_column_width: f32,
    /// Gap between the *Reactants* and *Products* blocks.
    #[serde(default = "def_block_gap")]
    pub block_gap: f32,
    /// Font size of a block title (*R e a c t a n t s*).
    #[serde(default = "def_title_font_size")]
    pub title_font_size: f32,
    /// Extra tracking inserted between the letters of a block title, as a fraction of the
    /// title font size. Rendered as literal spacing between per-character runs.
    #[serde(default = "def_title_tracking")]
    pub title_tracking: f32,
    /// Gap between a block title and the frame below it.
    #[serde(default = "def_title_gap")]
    pub title_gap: f32,
    /// Font family for block titles. `None` uses [`FontFamily::Proportional`](egui::FontFamily).
    /// ChemDraw sets the titles in an italic serif; egui ships no serif face, so a host that
    /// wants one must install it with [`egui::Context::set_fonts`] and name it here. Not part
    /// of the serialized config.
    #[serde(skip)]
    pub title_family: Option<egui::FontFamily>,
    /// Thickness of the table frame and column rules.
    #[serde(default = "def_rule_width")]
    pub rule_width: f32,

    // ── Figure ───────────────────────────────────────────────────────────────
    /// Vertical gap between the scheme and the table.
    #[serde(default = "def_scheme_table_gap")]
    pub scheme_table_gap: f32,
    /// Paint an opaque white sheet behind the figure (as the editor canvas does). Turn it
    /// off to draw the figure over an existing background.
    #[serde(default = "def_white_background")]
    pub white_background: bool,
    /// Margin between the figure content and the allocated rect.
    #[serde(default = "def_margin")]
    pub margin: f32,
}

fn def_bond_length() -> f32 { 40.0 }
fn def_item_gap() -> f32 { 14.0 }
fn def_arrow_length() -> f32 { 96.0 }
fn def_arrow_width() -> f32 { 1.5 }
fn def_arrow_head_length() -> f32 { 10.0 }
fn def_arrow_head_half_width() -> f32 { 4.0 }
fn def_arrow_text_gap() -> f32 { 5.0 }
fn def_arrow_font_size() -> f32 { 13.0 }
fn def_plus_font_size() -> f32 { 17.0 }
fn def_scheme_font_size() -> f32 { 13.0 }
fn def_compound_label_font_size() -> f32 { 13.0 }
fn def_compound_label_gap() -> f32 { 6.0 }
fn def_row_gap() -> f32 { 24.0 }
fn def_table_font_size() -> f32 { 12.0 }
fn def_unit_scale() -> f32 { 0.75 }
fn def_cell_pad_x() -> f32 { 6.0 }
fn def_cell_pad_y() -> f32 { 3.0 }
fn def_min_column_width() -> f32 { 62.0 }
fn def_block_gap() -> f32 { 26.0 }
fn def_title_font_size() -> f32 { 13.0 }
fn def_title_tracking() -> f32 { 0.35 }
fn def_title_gap() -> f32 { 4.0 }
fn def_rule_width() -> f32 { 1.0 }
fn def_scheme_table_gap() -> f32 { 26.0 }
fn def_white_background() -> bool { true }
fn def_margin() -> f32 { 8.0 }

impl Default for ReactionStyle {
    fn default() -> Self {
        Self {
            bond_length: def_bond_length(),
            item_gap: def_item_gap(),
            arrow_length: def_arrow_length(),
            arrow_width: def_arrow_width(),
            arrow_head_length: def_arrow_head_length(),
            arrow_head_half_width: def_arrow_head_half_width(),
            arrow_text_gap: def_arrow_text_gap(),
            arrow_font_size: def_arrow_font_size(),
            plus_font_size: def_plus_font_size(),
            scheme_font_size: def_scheme_font_size(),
            compound_label_font_size: def_compound_label_font_size(),
            compound_label_gap: def_compound_label_gap(),
            row_gap: def_row_gap(),
            max_row_width: None,
            table_font_size: def_table_font_size(),
            unit_scale: def_unit_scale(),
            cell_pad_x: def_cell_pad_x(),
            cell_pad_y: def_cell_pad_y(),
            min_column_width: def_min_column_width(),
            block_gap: def_block_gap(),
            title_font_size: def_title_font_size(),
            title_tracking: def_title_tracking(),
            title_gap: def_title_gap(),
            title_family: None,
            rule_width: def_rule_width(),
            scheme_table_gap: def_scheme_table_gap(),
            white_background: def_white_background(),
            margin: def_margin(),
        }
    }
}

impl ReactionStyle {
    /// Factor that maps molecule coordinates to screen pixels so that one
    /// [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH) bond comes out
    /// [`bond_length`](Self::bond_length) pixels long.
    pub fn structure_scale(&self) -> f32 {
        self.bond_length / (crate::widget::DEFAULT_BOND_LENGTH * crate::widget::SCALE_FACTOR)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structure_scale_maps_a_bond_to_bond_length_pixels() {
        let s = ReactionStyle::default();
        let k = s.structure_scale();
        let px = crate::widget::DEFAULT_BOND_LENGTH * k * crate::widget::SCALE_FACTOR;
        assert!((px - s.bond_length).abs() < 1e-4, "one bond should be {} px, got {px}", s.bond_length);
    }

    /// A partial config only has to name the fields it changes.
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let s: ReactionStyle = serde_json::from_str(r#"{ "bond_length": 40.0 }"#).unwrap();
        assert_eq!(s.bond_length, 40.0);
        assert_eq!(s.table_font_size, def_table_font_size());
        assert!(s.white_background);
    }
}
