//! [`ReactionView`] — the whole figure (scheme above, stoichiometry table below) as one
//! drawable unit.
//!
//! The view owns the drawing context structures need (a throw-away [`ChemStructEditor`]),
//! so keep one around between frames rather than building it per frame.
//!
//! ```no_run
//! use chembuider_rs::egui;
//! use chembuider_rs::reaction::{ReactionView, demo};
//!
//! struct App { view: ReactionView, reaction: chembuider_rs::reaction::Reaction }
//! impl App {
//!     fn new() -> Self {
//!         Self { view: ReactionView::new(), reaction: demo::williamson_ether_synthesis() }
//!     }
//!     fn ui(&mut self, ui: &mut egui::Ui) {
//!         self.view.show(ui, &self.reaction);
//!     }
//! }
//! ```

use egui::{Color32, Painter, Pos2, Rect, Sense, Vec2};

use crate::config::StyleConfig;
use crate::widget::ChemStructEditor;

use super::model::Reaction;
use super::scheme::{SchemeLayout, measure_scheme, paint_scheme};
use super::style::ReactionStyle;
use super::table::{AnalysisTable, TableLayout, measure_table, paint_table};

/// A measured figure: both halves plus where the table goes relative to the figure's
/// top-left.
pub struct FigureLayout {
    pub scheme: SchemeLayout,
    pub table: AnalysisTable,
    pub table_layout: TableLayout,
    /// Offset of the table's top-left from the figure's top-left.
    pub table_offset: Vec2,
    /// Offset of the scheme's top-left from the figure's top-left.
    pub scheme_offset: Vec2,
    /// Total size, margins included.
    pub size: Vec2,
}

/// Draws a [`Reaction`]: the scheme, then the stoichiometry table underneath.
pub struct ReactionView {
    /// Figure style (arrow metrics, fonts, table padding …).
    pub style: ReactionStyle,
    /// Structure drawing style — the same knobs the editor uses for bonds and labels.
    pub structure_style: StyleConfig,
    /// Drawing context for structures: the editor's renderer works off one of these.
    scratch: ChemStructEditor,
}

impl Default for ReactionView {
    fn default() -> Self {
        Self::new()
    }
}

impl ReactionView {
    pub fn new() -> Self {
        Self {
            style: ReactionStyle::default(),
            structure_style: StyleConfig::default(),
            scratch: ChemStructEditor::default(),
        }
    }

    pub fn with_style(mut self, style: ReactionStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_structure_style(mut self, style: StyleConfig) -> Self {
        self.structure_style = style;
        self
    }

    /// Measure the figure without drawing anything.
    pub fn measure(&mut self, painter: &Painter, reaction: &Reaction) -> FigureLayout {
        let scheme = measure_scheme(
            &reaction.scheme,
            painter,
            &self.style,
            &self.structure_style,
            &mut self.scratch,
        );
        let table = AnalysisTable::from_result(&reaction.analysis.compute());
        let table_layout = measure_table(&table, painter, &self.style);

        let m = self.style.margin;
        let gap = if reaction.scheme.is_empty() || table.is_empty() {
            0.0
        } else {
            self.style.scheme_table_gap
        };
        // The two halves are centred on each other: a narrow scheme sits over the middle of
        // a wide table, as in the reference figure.
        let content = scheme.size.x.max(table_layout.size.x);
        let scheme_offset = Vec2::new(m + (content - scheme.size.x) * 0.5, m);
        let table_offset = Vec2::new(
            m + (content - table_layout.size.x) * 0.5,
            m + scheme.size.y + gap,
        );
        let size = Vec2::new(
            content + 2.0 * m,
            scheme.size.y + gap + table_layout.size.y + 2.0 * m,
        );

        FigureLayout {
            scheme,
            table,
            table_layout,
            table_offset,
            scheme_offset,
            size,
        }
    }

    /// Paint a figure that has already been measured.
    pub fn paint_layout(
        &mut self,
        layout: &FigureLayout,
        painter: &Painter,
        top_left: Pos2,
        reaction: &Reaction,
    ) {
        if self.style.white_background {
            painter.rect_filled(
                Rect::from_min_size(top_left, layout.size),
                0.0,
                Color32::WHITE,
            );
        }
        paint_scheme(
            &reaction.scheme,
            &layout.scheme,
            painter,
            top_left + layout.scheme_offset,
            &self.style,
            &self.structure_style,
            &mut self.scratch,
        );
        paint_table(
            &layout.table_layout,
            painter,
            top_left + layout.table_offset,
            &self.style,
        );
    }

    /// Measure and paint in one call; returns the rect the figure occupies.
    pub fn paint(&mut self, painter: &Painter, top_left: Pos2, reaction: &Reaction) -> Rect {
        let layout = self.measure(painter, reaction);
        self.paint_layout(&layout, painter, top_left, reaction);
        Rect::from_min_size(top_left, layout.size)
    }

    /// Draw the figure as an egui widget, allocating exactly the space it needs.
    pub fn show(&mut self, ui: &mut egui::Ui, reaction: &Reaction) -> egui::Response {
        let painter = ui.painter().clone();
        let layout = self.measure(&painter, reaction);
        let (rect, response) = ui.allocate_exact_size(layout.size, Sense::hover());
        if ui.is_rect_visible(rect) {
            let painter = ui.painter().clone();
            self.paint_layout(&layout, &painter, rect.min, reaction);
        }
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;

    /// The figure stacks the scheme above the table, both inside the allocated rect.
    #[test]
    fn figure_stacks_scheme_above_table() {
        let reaction = demo::williamson_ether_synthesis();
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
        let mut view = ReactionView::new();
        let _ = ctx.run_ui(raw, |ui| {
            let layout = view.measure(ui.painter(), &reaction);
            let m = view.style.margin;

            assert!(layout.scheme.size.y > 0.0);
            assert!(layout.table_layout.size.y > 0.0);

            // The table starts below the scheme, separated by the configured gap.
            assert!(
                (layout.table_offset.y - (m + layout.scheme.size.y + view.style.scheme_table_gap))
                    .abs()
                    < 0.01
            );
            // Both halves fit inside the figure, margins included.
            assert!(layout.size.x >= layout.table_layout.size.x + 2.0 * m - 0.01);
            assert!(layout.size.x >= layout.scheme.size.x + 2.0 * m - 0.01);

            // The scheme is narrower than the table, and centred on it.
            assert!(layout.scheme.size.x < layout.table_layout.size.x);
            let scheme_mid = layout.scheme_offset.x + layout.scheme.size.x * 0.5;
            let table_mid = layout.table_offset.x + layout.table_layout.size.x * 0.5;
            assert!(
                (scheme_mid - table_mid).abs() < 0.01,
                "scheme centre {scheme_mid} should sit over the table centre {table_mid}"
            );
            assert!(
                (layout.size.y - (layout.table_offset.y + layout.table_layout.size.y + m)).abs()
                    < 0.01
            );
        });
    }

    /// Through the widget path: the allocated rect matches the measured size, and the
    /// frame paints both the structures (bond lines) and the table text.
    #[test]
    fn widget_paints_scheme_and_table() {
        let reaction = demo::williamson_ether_synthesis();
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
        let mut view = ReactionView::new();
        let mut allocated = Rect::NOTHING;
        let mut measured = Vec2::ZERO;
        let output = ctx.run_ui(raw, |ui| {
            measured = view.measure(ui.painter(), &reaction).size;
            allocated = view.show(ui, &reaction).rect;
        });
        // egui snaps an allocation to the pixel grid, so allow a sub-pixel difference.
        assert!(
            (allocated.size() - measured).length() < 0.1,
            "allocated {:?} vs measured {measured:?}",
            allocated.size()
        );

        let mut texts = Vec::new();
        let mut lines = 0usize;
        fn walk(shape: &egui::Shape, texts: &mut Vec<String>, lines: &mut usize) {
            match shape {
                egui::Shape::Text(t) => texts.push(t.galley.job.text.clone()),
                egui::Shape::LineSegment { .. } => *lines += 1,
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, texts, lines);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts, &mut lines);
        }

        // Scheme: the "HO" of phenol, the arrow labels; table: the yield.
        for expected in ["OH", "MeI", "K", "60.92"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "expected a '{expected}' galley among {texts:?}"
            );
        }
        // Two benzene rings (6 bonds each, doubles drawn as 2 lines) plus the arrow shaft
        // and the column rules — a comfortable lower bound.
        assert!(lines > 20, "expected the structures to paint their bonds, got {lines}");
    }
}
