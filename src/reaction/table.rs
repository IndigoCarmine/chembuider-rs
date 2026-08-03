//! The stoichiometry table: two framed blocks (*Reactants* and *Products*), one column per
//! species, one row per property.
//!
//! Every row of a block always exists — a value that was neither entered nor derivable is
//! simply drawn as an empty cell, exactly as ChemDraw leaves `%Weight` or `Purity` blank.
//!
//! Typography follows the reference figure:
//! * `Formula` is **bold**, with the counts as true subscripts;
//! * a trailing unit (`g`, `mmol`, `%`) is set smaller than the number, on the same baseline;
//! * values the user *entered* are ***bold italic***, computed ones are regular;
//! * blocks are framed with vertical rules between columns and no horizontal rules;
//! * the block title sits outside the frame, above its left corner, italic and letter-spaced.

use egui::{Align2, Color32, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::widget::rich_text::{RichText, RichTextStyle, SpanStyle};

use super::stoich::{
    AnalysisResult, Formatted, Origin, ProductAnalysis, Quantity, ReactantAnalysis, format_density,
    format_mass, format_molarity, format_moles, format_number, format_percent, format_volume,
};
use super::style::ReactionStyle;
use super::text::{Run, RunLayout, layout_runs, tracked_runs, value_runs};

/// Row labels of the *Reactants* block, top to bottom.
pub const REACTANT_ROWS: [&str; 11] = [
    "Formula",
    "MW",
    "Limiting?",
    "Equivalents",
    "Sample Mass",
    "%Weight",
    "Molarity",
    "Density",
    "Volume",
    "Reactant Moles",
    "Reactant Mass",
];

/// Row labels of the *Products* block, top to bottom.
pub const PRODUCT_ROWS: [&str; 11] = [
    "Formula",
    "MW",
    "Equivalents",
    "%Completion",
    "Expected Mass",
    "Expected Moles",
    "Measured Mass",
    "Purity",
    "Product Mass",
    "Product Moles",
    "%Yield",
];

/// One table cell: the styled value plus the unit that trails it in a smaller face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cell {
    pub text: RichText,
    /// `g`, `mmol`, `%` … — empty for dimensionless cells.
    pub unit: String,
}

impl Cell {
    /// A cell with no value: the row exists, the number doesn't.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Flattened text including the unit — `"700.00mg"`. The readable oracle for tests.
    pub fn plain(&self) -> String {
        let mut s: String = self.text.spans.iter().map(|sp| sp.text.as_str()).collect();
        s.push_str(&self.unit);
        s
    }

    /// True when the value was entered by the user (drawn bold + italic).
    pub fn is_entered(&self) -> bool {
        self.text
            .spans
            .first()
            .is_some_and(|s| s.style.bold && s.style.italic)
    }
}

/// Style of a value cell: entered values are bold + italic, computed ones regular.
fn value_style(origin: Origin) -> SpanStyle {
    match origin {
        Origin::Entered => SpanStyle { bold: true, italic: true, ..Default::default() },
        Origin::Computed => SpanStyle::default(),
    }
}

fn cell(q: Option<Quantity>, format: impl Fn(f64) -> Formatted) -> Cell {
    match q {
        None => Cell::empty(),
        Some(q) => {
            let f = format(q.value);
            Cell {
                text: RichText::new().span(f.number, value_style(q.origin)),
                unit: f.unit.to_string(),
            }
        }
    }
}

fn text_cell(text: &str, origin: Origin) -> Cell {
    Cell {
        text: RichText::new().span(text.to_string(), value_style(origin)),
        unit: String::new(),
    }
}

/// The 11 cells of one *Reactants* column, in [`REACTANT_ROWS`] order.
pub fn reactant_cells(r: &ReactantAnalysis) -> Vec<Cell> {
    vec![
        Cell {
            text: r
                .formula
                .rich_styled(SpanStyle { bold: true, ..Default::default() }),
            unit: String::new(),
        },
        cell(r.mw.map(Quantity::computed), format_number),
        // Being the limiting reagent is the user's declaration, so it is styled as entered.
        text_cell(
            if r.limiting { "Yes" } else { "No" },
            if r.limiting { Origin::Entered } else { Origin::Computed },
        ),
        cell(r.equivalents, format_number),
        cell(r.sample_mass, format_mass),
        cell(r.percent_weight, format_percent),
        cell(r.molarity, format_molarity),
        cell(r.density, format_density),
        cell(r.volume, format_volume),
        cell(r.moles, format_moles),
        cell(r.mass, format_mass),
    ]
}

/// The 11 cells of one *Products* column, in [`PRODUCT_ROWS`] order.
pub fn product_cells(p: &ProductAnalysis) -> Vec<Cell> {
    vec![
        Cell {
            text: p
                .formula
                .rich_styled(SpanStyle { bold: true, ..Default::default() }),
            unit: String::new(),
        },
        cell(p.mw.map(Quantity::computed), format_number),
        cell(p.equivalents, format_number),
        cell(p.percent_completion, format_percent),
        cell(p.expected_mass, format_mass),
        cell(p.expected_moles, format_moles),
        cell(p.measured_mass, format_mass),
        cell(p.purity, format_percent),
        cell(p.mass, format_mass),
        cell(p.moles, format_moles),
        cell(p.percent_yield, format_percent),
    ]
}

/// One framed block of the table.
#[derive(Clone, Debug)]
pub struct TableBlock {
    /// Drawn italic and letter-spaced above the frame's top-left corner.
    pub title: String,
    pub row_labels: Vec<String>,
    /// `columns[species][row]`.
    pub columns: Vec<Vec<Cell>>,
}

/// The whole stoichiometry table: the *Reactants* block and the *Products* block.
#[derive(Clone, Debug, Default)]
pub struct AnalysisTable {
    pub blocks: Vec<TableBlock>,
}

impl AnalysisTable {
    /// Build the table from computed results. A block with no columns is left out entirely.
    pub fn from_result(result: &AnalysisResult) -> Self {
        let mut blocks = Vec::new();
        if !result.reactants.is_empty() {
            blocks.push(TableBlock {
                title: "Reactants".into(),
                row_labels: REACTANT_ROWS.iter().map(|s| s.to_string()).collect(),
                columns: result.reactants.iter().map(reactant_cells).collect(),
            });
        }
        if !result.products.is_empty() {
            blocks.push(TableBlock {
                title: "Products".into(),
                row_labels: PRODUCT_ROWS.iter().map(|s| s.to_string()).collect(),
                columns: result.products.iter().map(product_cells).collect(),
            });
        }
        Self { blocks }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

// ─── Layout ──────────────────────────────────────────────────────────────────

/// A measured table, ready to paint. All rects are relative to the table's top-left.
pub struct TableLayout {
    pub blocks: Vec<BlockLayout>,
    pub size: Vec2,
}

/// A measured block.
pub struct BlockLayout {
    pub title: RunLayout,
    /// The framed area (the title sits above it).
    pub frame: Rect,
    /// Left edge of every column plus the frame's right edge: `columns.len() + 2` entries,
    /// the first column being the row labels.
    pub column_x: Vec<f32>,
    pub row_height: f32,
    /// Baseline offset inside a row, from the row's top edge.
    pub baseline: f32,
    /// `cells[column][row]`, column 0 being the row labels.
    pub cells: Vec<Vec<RunLayout>>,
}

impl BlockLayout {
    /// Bounding rect of the block including its title.
    pub fn rect(&self) -> Rect {
        let title_top = self.frame.min.y - self.title.size.y;
        Rect::from_min_max(
            Pos2::new(self.frame.min.x, title_top.min(self.frame.min.y)),
            self.frame.max,
        )
    }
}

/// Measure the table with the fonts of `painter`.
pub fn measure_table(table: &AnalysisTable, painter: &Painter, style: &ReactionStyle) -> TableLayout {
    let cell_style = RichTextStyle::new(style.table_font_size, Color32::BLACK);
    let title_style = RichTextStyle {
        size: style.title_font_size,
        color: Color32::BLACK,
        family: style
            .title_family
            .clone()
            .unwrap_or(egui::FontFamily::Proportional),
    };
    let lay = |runs: &[Run], s: &RichTextStyle, tracking: f32| {
        layout_runs(runs, s, tracking, |job| painter.layout_job(job))
    };

    let mut blocks: Vec<BlockLayout> = Vec::with_capacity(table.blocks.len());
    let mut pen_x = 0.0_f32;
    let mut height = 0.0_f32;

    for block in &table.blocks {
        let title = lay(
            &tracked_runs(&block.title, SpanStyle { italic: true, ..Default::default() }),
            &title_style,
            style.title_tracking * style.title_font_size,
        );

        // Column 0 is the row labels; the rest are the species.
        let mut cells: Vec<Vec<RunLayout>> = Vec::with_capacity(block.columns.len() + 1);
        cells.push(
            block
                .row_labels
                .iter()
                .map(|label| lay(&[Run::new(RichText::plain(label.clone()))], &cell_style, 0.0))
                .collect(),
        );
        for column in &block.columns {
            cells.push(
                column
                    .iter()
                    .map(|c| {
                        lay(
                            &value_runs(c.text.clone(), &c.unit, style.unit_scale),
                            &cell_style,
                            0.0,
                        )
                    })
                    .collect(),
            );
        }

        // One row height and one baseline for the whole block, so every row lines up.
        let mut max_baseline = 0.0_f32;
        let mut max_descent = 0.0_f32;
        for column in &cells {
            for c in column {
                max_baseline = max_baseline.max(c.baseline);
                max_descent = max_descent.max(c.size.y - c.baseline);
            }
        }
        let row_height = max_baseline + max_descent + 2.0 * style.cell_pad_y;
        let baseline = style.cell_pad_y + max_baseline;

        // Column widths: the widest cell plus padding, with a floor for the value columns.
        let mut column_x = Vec::with_capacity(cells.len() + 1);
        let mut x = 0.0_f32;
        for (i, column) in cells.iter().enumerate() {
            column_x.push(x);
            let content = column.iter().map(|c| c.size.x).fold(0.0_f32, f32::max);
            let mut width = content + 2.0 * style.cell_pad_x;
            if i > 0 {
                width = width.max(style.min_column_width);
            }
            x += width;
        }
        column_x.push(x);

        let rows = block.row_labels.len() as f32;
        let top = title.size.y + style.title_gap;
        let frame = Rect::from_min_size(
            Pos2::new(pen_x, top),
            Vec2::new(x, rows * row_height),
        );
        height = height.max(frame.max.y);
        pen_x = frame.max.x + style.block_gap;

        blocks.push(BlockLayout {
            title,
            frame,
            column_x,
            row_height,
            baseline,
            cells,
        });
    }

    let width = blocks
        .last()
        .map(|b| b.frame.max.x)
        .unwrap_or(0.0);
    TableLayout { blocks, size: Vec2::new(width, height) }
}

/// Paint a measured table with its top-left corner at `top_left`.
pub fn paint_table(layout: &TableLayout, painter: &Painter, top_left: Pos2, style: &ReactionStyle) {
    let offset = top_left.to_vec2();
    let stroke = Stroke::new(style.rule_width, Color32::BLACK);

    for block in &layout.blocks {
        let frame = block.frame.translate(offset);

        // Title: outside the frame, above its left corner.
        block.title.paint_anchored(
            painter,
            Pos2::new(frame.min.x, frame.min.y - style.title_gap),
            Align2::LEFT_BOTTOM,
        );

        painter.rect_stroke(frame, 0.0, stroke, StrokeKind::Inside);

        // Vertical rules between columns (the outer edges are the frame itself).
        for x in block.column_x.iter().skip(1).take(block.column_x.len() - 2) {
            let sx = frame.min.x + x;
            painter.line_segment([Pos2::new(sx, frame.min.y), Pos2::new(sx, frame.max.y)], stroke);
        }

        // Cells: left-aligned in their column, all on the row's shared baseline.
        for (c, column) in block.cells.iter().enumerate() {
            let x = frame.min.x + block.column_x[c] + style.cell_pad_x;
            for (r, cell) in column.iter().enumerate() {
                if cell.is_empty() {
                    continue;
                }
                let row_top = frame.min.y + r as f32 * block.row_height;
                cell.paint(painter, Pos2::new(x, row_top + block.baseline - cell.baseline));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;

    fn figure_table() -> AnalysisTable {
        AnalysisTable::from_result(&demo::williamson_ether_synthesis().analysis.compute())
    }

    /// Every cell of the reference figure, as flat text. Empty strings are the blank cells.
    #[test]
    fn reference_figure_cells() {
        let table = figure_table();
        assert_eq!(table.blocks.len(), 2);

        let reactants = &table.blocks[0];
        assert_eq!(reactants.title, "Reactants");
        assert_eq!(reactants.row_labels, REACTANT_ROWS.to_vec());
        assert_eq!(reactants.columns.len(), 3);

        let flat = |col: &[Cell]| col.iter().map(|c| c.plain()).collect::<Vec<_>>();
        assert_eq!(
            flat(&reactants.columns[0]),
            [
                "C6H6O", "94.11", "Yes", "1.00", "1.00g", "", "", "", "", "10.63mmol", "1.00g",
            ]
        );
        assert_eq!(
            flat(&reactants.columns[1]),
            [
                "CH3I", "141.94", "No", "1.10", "1.66g", "", "", "", "", "11.69mmol", "1.66g",
            ]
        );
        assert_eq!(
            flat(&reactants.columns[2]),
            [
                "CK2O3", "138.20", "No", "3.00", "4.41g", "", "", "", "", "31.88mmol", "4.41g",
            ]
        );

        let products = &table.blocks[1];
        assert_eq!(products.title, "Products");
        assert_eq!(products.row_labels, PRODUCT_ROWS.to_vec());
        assert_eq!(
            flat(&products.columns[0]),
            [
                "C7H8O", "108.14", "", "", "1.15g", "10.63mmol", "700.00mg", "", "700.00mg",
                "6.47mmol", "60.92%",
            ]
        );
    }

    /// The typography rules: bold formula with subscripts, bold-italic entered values,
    /// regular computed ones, units carried separately so they can be drawn smaller.
    #[test]
    fn cell_styling_matches_the_figure() {
        use crate::widget::rich_text::Script;
        let table = figure_table();
        let phenol = &table.blocks[0].columns[0];
        let product = &table.blocks[1].columns[0];

        // Formula: every span bold, the counts subscript.
        let formula = &phenol[0];
        assert!(formula.text.spans.iter().all(|s| s.style.bold));
        assert!(!formula.text.spans.iter().any(|s| s.style.italic));
        assert!(
            formula
                .text
                .spans
                .iter()
                .any(|s| s.style.script == Script::Sub && s.text == "6")
        );
        assert!(formula.unit.is_empty());

        // Entered values: Limiting? = Yes, all equivalents, the weighed sample mass and
        // the measured product mass.
        assert!(phenol[2].is_entered(), "Limiting? Yes");
        assert!(phenol[3].is_entered(), "equivalents");
        assert!(phenol[4].is_entered(), "sample mass — phenol was weighed out");
        assert!(
            !table.blocks[0].columns[1][4].is_entered(),
            "MeI's sample mass is back-filled from its equivalents, so it is computed"
        );
        assert!(table.blocks[0].columns[1][3].is_entered(), "MeI equivalents");
        assert!(table.blocks[0].columns[2][3].is_entered(), "K2CO3 equivalents");
        assert!(product[6].is_entered(), "measured mass");

        // Computed values are regular.
        for cell in [&phenol[0], &phenol[1], &phenol[9], &phenol[10], &product[4], &product[10]] {
            assert!(!cell.is_entered(), "{} should be computed", cell.plain());
        }
        // "No" is not a user entry either.
        assert!(!table.blocks[0].columns[1][2].is_entered());

        // Units live outside the number so they can be set smaller.
        assert_eq!(phenol[9].unit, "mmol");
        assert_eq!(phenol[9].text.spans[0].text, "10.63");
        assert_eq!(product[10].unit, "%");
        assert_eq!(phenol[1].unit, "", "MW is dimensionless");
    }

    /// Rows always exist; missing values are empty cells.
    #[test]
    fn every_row_is_present_even_when_blank() {
        let table = figure_table();
        for block in &table.blocks {
            for column in &block.columns {
                assert_eq!(column.len(), block.row_labels.len());
            }
        }
        // The blank rows of the figure.
        let phenol = &table.blocks[0].columns[0];
        for row in 5..=8 {
            assert!(phenol[row].is_empty(), "{} must be blank", REACTANT_ROWS[row]);
        }
        let product = &table.blocks[1].columns[0];
        for row in [2, 3, 7] {
            assert!(product[row].is_empty(), "{} must be blank", PRODUCT_ROWS[row]);
        }
    }

    // ── Layout ──────────────────────────────────────────────────────────────

    fn with_painter(mut f: impl FnMut(&Painter)) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
        let _ = ctx.run_ui(raw, |ui| f(ui.painter()));
    }

    /// Geometry: two separate frames side by side, a label column plus one column per
    /// species, uniform rows, and the title sitting above the frame's left corner.
    #[test]
    fn table_layout_geometry() {
        let table = figure_table();
        let style = ReactionStyle::default();
        with_painter(|painter| {
            let layout = measure_table(&table, painter, &style);
            assert_eq!(layout.blocks.len(), 2);

            let reactants = &layout.blocks[0];
            let products = &layout.blocks[1];

            // 1 label column + 3 species, plus the closing edge.
            assert_eq!(reactants.column_x.len(), 5);
            assert_eq!(products.column_x.len(), 3);
            // Columns are strictly increasing and the last entry is the frame width.
            for w in reactants.column_x.windows(2) {
                assert!(w[1] > w[0], "columns must have positive width: {w:?}");
            }
            assert!((reactants.column_x.last().unwrap() - reactants.frame.width()).abs() < 0.01);

            // Rows: 11 of them, uniform, filling the frame exactly.
            assert!(reactants.row_height > 0.0);
            assert!(
                (reactants.frame.height() - 11.0 * reactants.row_height).abs() < 0.01,
                "frame height must be 11 uniform rows"
            );

            // The two blocks are separate frames with a gap between them.
            assert!(
                products.frame.min.x >= reactants.frame.max.x + style.block_gap - 0.01,
                "blocks must not touch: {:?} / {:?}",
                reactants.frame,
                products.frame
            );

            // The title sits above the frame, outside it, at its left edge.
            assert!(reactants.title.size.x > 0.0);
            assert!(reactants.frame.min.y >= reactants.title.size.y + style.title_gap - 0.01);
            assert_eq!(reactants.title.text(), "Reactants");
            assert_eq!(products.title.text(), "Products");

            // The whole table is as wide as the right edge of the last block.
            assert!((layout.size.x - products.frame.max.x).abs() < 0.01);
            assert!(layout.size.y >= reactants.frame.max.y - 0.01);

            // Every cell fits inside its column, and the baseline sits inside the row.
            assert!(reactants.baseline > 0.0 && reactants.baseline < reactants.row_height);
            for (c, column) in reactants.cells.iter().enumerate() {
                let width = reactants.column_x[c + 1] - reactants.column_x[c];
                for cell in column {
                    assert!(
                        cell.size.x + 2.0 * style.cell_pad_x <= width + 0.01,
                        "cell '{}' overflows its column",
                        cell.text()
                    );
                }
            }
        });
    }

    /// Painting emits one frame per block, the internal column rules, and the cell text.
    #[test]
    fn painting_emits_frames_and_text() {
        let table = figure_table();
        let style = ReactionStyle::default();
        let origin = Pos2::new(10.0, 10.0);
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
        let mut frames: Vec<Rect> = Vec::new();
        let output = ctx.run_ui(raw, |ui| {
            let layout = measure_table(&table, ui.painter(), &style);
            frames = layout
                .blocks
                .iter()
                .map(|b| b.frame.translate(origin.to_vec2()))
                .collect();
            paint_table(&layout, ui.painter(), origin, &style);
        });

        let mut texts: Vec<String> = Vec::new();
        let mut rects: Vec<Rect> = Vec::new();
        let mut lines: Vec<[Pos2; 2]> = Vec::new();
        fn walk(
            shape: &egui::Shape,
            texts: &mut Vec<String>,
            rects: &mut Vec<Rect>,
            lines: &mut Vec<[Pos2; 2]>,
        ) {
            match shape {
                egui::Shape::Text(t) => texts.push(t.galley.job.text.clone()),
                egui::Shape::Rect(r) => rects.push(r.rect),
                egui::Shape::LineSegment { points, .. } => lines.push(*points),
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, texts, rects, lines);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts, &mut rects, &mut lines);
        }

        // One frame per block, drawn where the layout said it would be.
        assert_eq!(frames.len(), 2);
        for frame in &frames {
            assert!(
                rects
                    .iter()
                    .any(|r| (r.min - frame.min).length() < 0.01 && (r.max - frame.max).length() < 0.01),
                "no frame painted at {frame:?} (painted: {rects:?})"
            );
        }

        // Column rules: vertical segments spanning a frame top to bottom — three inside the
        // Reactants block (4 columns) and one inside Products (2 columns). No horizontal rules.
        let rules: Vec<_> = lines
            .iter()
            .filter(|[a, b]| (a.x - b.x).abs() < 0.01 && frames.iter().any(|f| f.contains(*a) && f.contains(*b)))
            .collect();
        assert_eq!(rules.len(), 4, "3 rules in Reactants + 1 in Products, got {rules:?}");
        assert!(
            !lines.iter().any(|[a, b]| (a.y - b.y).abs() < 0.01
                && frames.iter().any(|f| f.contains(*a) && f.contains(*b))),
            "the blocks must have no horizontal rules"
        );

        for expected in [
            "Formula",
            "%Yield",
            "Reactant Moles",
            "10.63",
            "mmol",
            "60.92",
            "700.00",
            "mg",
            "Yes",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "expected a '{expected}' galley among {texts:?}"
            );
        }
    }
}
