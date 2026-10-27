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

    // ─── Characterization: what the figure paints ────────────────────────────────
    //
    // These pin the figure's layout (every placed rect, origin and offset) and the shape list
    // the view paints — paint order, coordinates, colours and label text — so restructuring
    // how structures are drawn (scratch editor → scene → renderer) can be shown not to change
    // the picture. The expected values record current behaviour, not a spec. In particular
    // the reaction figure deliberately paints *no* ring fills, but does paint the over-valence
    // warning ring.

    use crate::molecule::BondOrder;
    use crate::reaction::{Analysis, Arrow, ReactionScheme};
    use crate::reaction::scheme::{PlacedArrowLabel, PlacedKind};
    use crate::testkit::shape_fingerprint;

    /// Fill colour given to the benzene's ring fill in [`decorated_reaction`]; it must never
    /// appear in the painted shapes.
    const RING_FILL: [u8; 4] = [0, 200, 0, 128];

    fn fp_num(v: f32) -> String {
        let r = (v * 1000.0).round() / 1000.0;
        format!("{:.3}", if r == 0.0 { 0.0 } else { r })
    }

    fn fp_vec(v: Vec2) -> String {
        format!("({},{})", fp_num(v.x), fp_num(v.y))
    }

    fn fp_rect(r: Rect) -> String {
        format!("{}-{}", fp_vec(r.min.to_vec2()), fp_vec(r.max.to_vec2()))
    }

    /// A line-per-entry dump of everything [`ReactionView::measure`] decides.
    fn layout_fingerprint(layout: &FigureLayout) -> String {
        let label = |l: &PlacedArrowLabel| match l {
            PlacedArrowLabel::Empty => "empty".to_string(),
            PlacedArrowLabel::Text(t) => format!("text size={}", fp_vec(t.size)),
            PlacedArrowLabel::Structure { origin } => format!("structure origin={}", fp_vec(*origin)),
        };
        let mut lines = vec![
            format!("figure size={}", fp_vec(layout.size)),
            format!("scheme offset={}", fp_vec(layout.scheme_offset)),
            format!(
                "scheme size={} center_line={}",
                fp_vec(layout.scheme.size),
                fp_num(layout.scheme.center_line)
            ),
        ];
        for item in &layout.scheme.items {
            let kind = match &item.kind {
                PlacedKind::Structure { origin } => format!("structure origin={}", fp_vec(*origin)),
                PlacedKind::Text(t) => format!("text size={}", fp_vec(t.size)),
                PlacedKind::Arrow { length, above, below } => format!(
                    "arrow length={} above=[{}] below=[{}]",
                    fp_num(*length),
                    label(above),
                    label(below)
                ),
            };
            lines.push(format!("  item {} {kind}", fp_rect(item.rect)));
        }
        lines.push(format!("table offset={}", fp_vec(layout.table_offset)));
        lines.push(format!("table size={}", fp_vec(layout.table_layout.size)));
        for block in &layout.table_layout.blocks {
            let cols: Vec<String> = block.column_x.iter().map(|x| fp_num(*x)).collect();
            lines.push(format!(
                "  block frame={} title={} columns=[{}] row_height={} baseline={} rows={}",
                fp_rect(block.frame),
                fp_vec(block.title.size),
                cols.join(" "),
                fp_num(block.row_height),
                fp_num(block.baseline),
                block.cells.first().map_or(0, Vec::len),
            ));
        }
        lines.join("\n")
    }

    /// Measure and paint `reaction` with a default view at a fixed offset, returning the
    /// layout fingerprint and the painted shapes' fingerprint.
    fn figure_fingerprints(reaction: &Reaction) -> (String, String) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
        let mut view = ReactionView::new();
        let mut layout_fp = String::new();
        let output = ctx.run_ui(raw, |ui| {
            let painter = ui.painter().clone();
            let layout = view.measure(&painter, reaction);
            layout_fp = layout_fingerprint(&layout);
            view.paint_layout(&layout, &painter, Pos2::new(30.0, 40.0), reaction);
        });
        (layout_fp, shape_fingerprint(&output.shapes))
    }

    /// A scheme that reaches every structure path the figure has: a ring-filled benzene, a
    /// `+`, an over-valence C≡O, an arrow carrying a structure on *both* sides, free text, and
    /// phenol (a labelled product). The table is empty, so the figure is the scheme alone.
    fn decorated_reaction() -> Reaction {
        let mut filled = demo::benzene();
        let ring: Vec<u32> = filled.atoms.iter().map(|a| a.id).collect();
        filled.set_ring_fill(ring, RING_FILL);
        assert_eq!(filled.ring_fills.len(), 1, "the benzene must carry a ring fill");

        let mut co = crate::molecule::Molecule::default();
        let c = co.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = co.add_atom("O".to_string(), [1.0, 0.0], 0);
        co.add_bond(c, o, BondOrder::Triple);
        assert!(
            crate::widget::draw::is_valence_invalid(&co, o),
            "C≡O must flag the oxygen as over-valence"
        );

        let scheme = ReactionScheme::new()
            .structure(filled)
            .plus()
            .structure(co)
            .arrow(
                Arrow::new()
                    .above_structure(demo::biphenyl())
                    .below_structure(demo::methyl_iodide()),
            )
            .structure(demo::phenol())
            .text("rt");
        Reaction::new(scheme, Analysis::new())
    }

    fn williamson_fingerprints() -> (String, String) {
        let reaction = demo::williamson_ether_synthesis();
        let (layout, shapes) = figure_fingerprints(&reaction);
        assert!(!reaction.scheme.is_empty());
        assert!(
            layout.contains("above=[text") && layout.contains("below=[text"),
            "both arrow labels of the demo are text:\n{layout}"
        );
        assert!(layout.contains("  block "), "the demo has a table:\n{layout}");
        (layout, shapes)
    }

    fn decorated_fingerprints() -> (String, String) {
        let reaction = decorated_reaction();
        let (layout, shapes) = figure_fingerprints(&reaction);
        assert!(
            layout.contains("above=[structure") && layout.contains("below=[structure"),
            "the arrow must carry a structure on both sides:\n{layout}"
        );
        assert!(!layout.contains("  block "), "the analysis is empty, so no table:\n{layout}");
        (layout, shapes)
    }

    #[track_caller]
    fn assert_pinned(name: &str, actual: &str, expected: &str) {
        let expected = expected.trim();
        if actual != expected {
            let first = actual
                .lines()
                .zip(expected.lines())
                .position(|(a, e)| a != e)
                .unwrap_or_else(|| actual.lines().count().min(expected.lines().count()));
            panic!(
                "{name}: differs from the pinned value (first difference at line {})\n\
                 --- actual ---\n{actual}\n--- expected ---\n{expected}",
                first + 1,
            );
        }
    }

    #[test]
    fn williamson_figure_layout_is_pinned() {
        assert_pinned("williamson layout", &williamson_fingerprints().0, WILLIAMSON_LAYOUT);
    }

    #[test]
    fn williamson_figure_shapes_are_pinned() {
        assert_pinned("williamson shapes", &williamson_fingerprints().1, WILLIAMSON_SHAPES);
    }

    #[test]
    fn decorated_figure_layout_is_pinned() {
        assert_pinned("decorated layout", &decorated_fingerprints().0, DECORATED_LAYOUT);
    }

    #[test]
    fn decorated_figure_shapes_are_pinned() {
        assert_pinned("decorated shapes", &decorated_fingerprints().1, DECORATED_SHAPES);
    }

    /// The reaction figure paints no ring fills (the benzene's fill colour appears nowhere),
    /// yet it does paint the over-valence warning ring (red dashes) — independently of the
    /// pinned text, so this property survives a deliberate re-pin.
    #[test]
    fn figure_skips_ring_fills_but_warns_on_valence() {
        let (_, shapes) = decorated_fingerprints();
        let [r, g, b, a] = RING_FILL;
        let fill = format!("fill=#{r:02x}{g:02x}{b:02x}{a:02x}");
        assert!(!shapes.contains(&fill), "a ring fill was painted:\n{shapes}");
        // The only filled path is the arrow head.
        let filled_paths = shapes.lines().filter(|l| l.starts_with("path ")).count();
        assert_eq!(filled_paths, 1, "only the arrow head is a path:\n{shapes}");
        assert!(
            shapes.lines().any(|l| l.starts_with("line ") && l.ends_with("/#dc1e1eff")),
            "the over-valence ring must be painted:\n{shapes}"
        );
    }

    /// Building the same figures again in fresh contexts gives the same fingerprints.
    #[test]
    fn figure_fingerprints_are_reproducible() {
        assert_eq!(williamson_fingerprints(), williamson_fingerprints());
        assert_eq!(decorated_fingerprints(), decorated_fingerprints());
    }

    const WILLIAMSON_LAYOUT: &str = r#"
figure size=(494.031,403.293)
scheme offset=(76.875,8.000)
scheme size=(340.282,112.173) center_line=56.087
  item (0.000,0.000)-(94.782,112.173) structure origin=(20.750,111.423)
  item (108.782,32.087)-(204.782,81.167) arrow length=96.000 above=[text size=(21.594,15.000)] below=[text size=(35.906,16.080)]
  item (218.782,0.000)-(340.282,112.173) structure origin=(239.532,111.423)
table offset=(8.000,146.173)
table size=(478.031,249.120)
  block frame=(0.000,19.000)-(290.000,249.120) title=(93.806,15.000) columns=[0.000 94.719 159.813 224.906 290.000] row_height=20.920 baseline=14.000 rows=11
  block frame=(316.000,19.000)-(478.031,249.120) title=(83.537,15.000) columns=[0.000 96.938 162.031] row_height=20.920 baseline=14.000 rows=11
"#;

    const WILLIAMSON_SHAPES: &str = r#"
clip (0.000,0.000)-(1400.000,900.000)
rect (30.000,40.000)-(524.031,443.293) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
line (127.625,159.423) (167.625,159.423) stroke=1.500/#000000ff
line (167.625,159.423) (187.625,124.782) stroke=1.500/#000000ff
line (167.594,152.477) (181.594,128.228) stroke=1.500/#000000ff
line (187.625,124.782) (167.625,90.141) stroke=1.500/#000000ff
line (167.625,90.141) (127.625,90.141) stroke=1.500/#000000ff
line (161.625,93.641) (133.625,93.641) stroke=1.500/#000000ff
line (127.625,90.141) (107.625,124.782) stroke=1.500/#000000ff
line (107.625,124.782) (127.625,159.423) stroke=1.500/#000000ff
line (113.656,128.228) (127.656,152.477) stroke=1.500/#000000ff
line (167.625,90.141) (182.140,65.000) stroke=1.500/#000000ff
rect (181.594,47.000)-(202.656,64.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "OH" at=(182.594,48.000) sections=["OH":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (215.656,104.087) (306.656,104.087) stroke=1.500/#000000ff
path closed=true fill=#000000ff stroke=0.000/#00000000 pts=[(311.656,104.087) (301.656,100.087) (301.656,108.087)] kind=Middle
text "MeI" at=(252.860,80.087) sections=["MeI":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "K" at=(245.703,113.087) sections=["K":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "2" at=(253.485,119.167) sections=["2":#000000ff@9.100] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "CO" at=(258.610,113.087) sections=["CO":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3" at=(276.485,119.167) sections=["3":#000000ff@9.100] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (346.406,159.423) (386.406,159.423) stroke=1.500/#000000ff
line (386.406,159.423) (406.406,124.782) stroke=1.500/#000000ff
line (386.375,152.477) (400.375,128.228) stroke=1.500/#000000ff
line (406.406,124.782) (386.406,90.141) stroke=1.500/#000000ff
line (386.406,90.141) (346.406,90.141) stroke=1.500/#000000ff
line (380.406,93.641) (352.406,93.641) stroke=1.500/#000000ff
line (346.406,90.141) (326.406,124.782) stroke=1.500/#000000ff
line (326.406,124.782) (346.406,159.423) stroke=1.500/#000000ff
line (332.437,128.228) (346.437,152.477) stroke=1.500/#000000ff
line (386.406,90.141) (400.922,65.000) stroke=1.500/#000000ff
line (413.438,55.500) (446.406,55.500) stroke=1.500/#000000ff
rect (400.375,47.000)-(412.438,64.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "O" at=(401.375,48.000) sections=["O":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "R" at=(38.000,186.173) sections=["R":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "e" at=(50.581,186.173) sections=["e":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "a" at=(62.256,186.173) sections=["a":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "c" at=(73.431,186.173) sections=["c":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "t" at=(83.950,186.173) sections=["t":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "a" at=(93.531,186.173) sections=["a":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "n" at=(104.706,186.173) sections=["n":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "t" at=(116.662,186.173) sections=["t":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "s" at=(126.244,186.173) sections=["s":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
rect (38.000,205.173)-(328.000,435.293) fill=#00000000 stroke=1.000/#000000ff corner=0/0/0/0 kind=Inside blur=0.000 round_px=None angle=0.000 brush=none
line (132.719,205.173) (132.719,435.293) stroke=1.000/#000000ff
line (197.813,205.173) (197.813,435.293) stroke=1.000/#000000ff
line (262.906,205.173) (262.906,435.293) stroke=1.000/#000000ff
text "Formula" at=(44.000,208.173) sections=["Formula":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "MW" at=(44.000,229.093) sections=["MW":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Limiting?" at=(44.000,250.013) sections=["Limiting?":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Equivalents" at=(44.000,270.933) sections=["Equivalents":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Sample Mass" at=(44.000,291.853) sections=["Sample Mass":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "%Weight" at=(44.000,312.773) sections=["%Weight":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Molarity" at=(44.000,333.693) sections=["Molarity":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Density" at=(44.000,354.613) sections=["Density":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Volume" at=(44.000,375.533) sections=["Volume":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Reactant Moles" at=(44.000,396.453) sections=["Reactant Moles":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Reactant Mass" at=(44.000,417.373) sections=["Reactant Mass":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "C" at=(138.719,208.173) sections=["C":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "C" at=(139.199,208.173) sections=["C":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "6" at=(146.511,213.093) sections=["6":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "6" at=(146.991,213.093) sections=["6":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "H" at=(151.741,208.173) sections=["H":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "H" at=(152.221,208.173) sections=["H":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "6" at=(160.534,213.093) sections=["6":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "6" at=(161.014,213.093) sections=["6":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(165.764,208.173) sections=["O":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(166.244,208.173) sections=["O":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "94.11" at=(138.719,229.093) sections=["94.11":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Yes" at=(138.719,250.013) sections=["Yes":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Yes" at=(139.199,250.013) sections=["Yes":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.00" at=(138.719,270.933) sections=["1.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.00" at=(139.199,270.933) sections=["1.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.00" at=(138.719,291.853) sections=["1.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.00" at=(139.199,291.853) sections=["1.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(162.449,294.853) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(162.809,294.853) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "10.63" at=(138.719,396.453) sections=["10.63":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mmol" at=(168.750,399.453) sections=["mmol":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.00" at=(138.719,417.373) sections=["1.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(161.969,420.373) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "CH" at=(203.813,208.173) sections=["CH":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "CH" at=(204.293,208.173) sections=["CH":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3" at=(219.918,213.093) sections=["3":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3" at=(220.397,213.093) sections=["3":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "I" at=(225.147,208.173) sections=["I":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "I" at=(225.627,208.173) sections=["I":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "141.94" at=(203.813,229.093) sections=["141.94":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "No" at=(203.813,250.013) sections=["No":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.10" at=(203.813,270.933) sections=["1.10":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.10" at=(204.293,270.933) sections=["1.10":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.66" at=(203.813,291.853) sections=["1.66":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(227.063,294.853) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "11.69" at=(203.813,396.453) sections=["11.69":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mmol" at=(233.844,399.453) sections=["mmol":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.66" at=(203.813,417.373) sections=["1.66":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(227.063,420.373) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "CK" at=(268.906,208.173) sections=["CK":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "CK" at=(269.386,208.173) sections=["CK":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "2" at=(283.886,213.093) sections=["2":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "2" at=(284.366,213.093) sections=["2":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(289.116,208.173) sections=["O":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(289.596,208.173) sections=["O":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3" at=(298.878,213.093) sections=["3":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3" at=(299.358,213.093) sections=["3":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "138.20" at=(268.906,229.093) sections=["138.20":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "No" at=(268.906,250.013) sections=["No":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3.00" at=(268.906,270.933) sections=["3.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "3.00" at=(269.386,270.933) sections=["3.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "4.41" at=(268.906,291.853) sections=["4.41":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(292.156,294.853) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "31.88" at=(268.906,396.453) sections=["31.88":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mmol" at=(298.938,399.453) sections=["mmol":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "4.41" at=(268.906,417.373) sections=["4.41":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(292.156,420.373) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "P" at=(354.000,186.173) sections=["P":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "r" at=(366.238,186.173) sections=["r":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "o" at=(375.663,186.173) sections=["o":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "d" at=(387.775,186.173) sections=["d":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "u" at=(399.919,186.173) sections=["u":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "c" at=(411.875,186.173) sections=["c":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "t" at=(422.394,186.173) sections=["t":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "s" at=(431.975,186.173) sections=["s":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
rect (354.000,205.173)-(516.031,435.293) fill=#00000000 stroke=1.000/#000000ff corner=0/0/0/0 kind=Inside blur=0.000 round_px=None angle=0.000 brush=none
line (450.938,205.173) (450.938,435.293) stroke=1.000/#000000ff
text "Formula" at=(360.000,208.173) sections=["Formula":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "MW" at=(360.000,229.093) sections=["MW":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Equivalents" at=(360.000,250.013) sections=["Equivalents":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "%Completion" at=(360.000,270.933) sections=["%Completion":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Expected Mass" at=(360.000,291.853) sections=["Expected Mass":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Expected Moles" at=(360.000,312.773) sections=["Expected Moles":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Measured Mass" at=(360.000,333.693) sections=["Measured Mass":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Purity" at=(360.000,354.613) sections=["Purity":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Product Mass" at=(360.000,375.533) sections=["Product Mass":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "Product Moles" at=(360.000,396.453) sections=["Product Moles":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "%Yield" at=(360.000,417.373) sections=["%Yield":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "C" at=(456.938,208.173) sections=["C":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "C" at=(457.418,208.173) sections=["C":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "7" at=(464.730,213.093) sections=["7":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "7" at=(465.210,213.093) sections=["7":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "H" at=(469.960,208.173) sections=["H":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "H" at=(470.440,208.173) sections=["H":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "8" at=(478.753,213.093) sections=["8":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "8" at=(479.233,213.093) sections=["8":#000000ff@8.400] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(483.982,208.173) sections=["O":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "O" at=(484.463,208.173) sections=["O":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "108.14" at=(456.938,229.093) sections=["108.14":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "1.15" at=(456.938,291.853) sections=["1.15":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "g" at=(480.188,294.853) sections=["g":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "10.63" at=(456.938,312.773) sections=["10.63":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mmol" at=(486.969,315.773) sections=["mmol":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "700.00" at=(456.938,333.693) sections=["700.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "700.00" at=(457.418,333.693) sections=["700.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mg" at=(494.199,336.693) sections=["mg":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mg" at=(494.559,336.693) sections=["mg":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "700.00" at=(456.938,375.533) sections=["700.00":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mg" at=(493.719,378.533) sections=["mg":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "6.47" at=(456.938,396.453) sections=["6.47":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "mmol" at=(480.188,399.453) sections=["mmol":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "60.92" at=(456.938,417.373) sections=["60.92":#000000ff@12.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "%" at=(486.969,420.373) sections=["%":#000000ff@9.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;

    const DECORATED_LAYOUT: &str = r#"
figure size=(543.730,151.869)
scheme offset=(8.000,8.000)
scheme size=(527.730,135.869) center_line=79.782
  item (0.000,44.391)-(81.500,115.173) structure origin=(20.750,114.423)
  item (95.500,69.782)-(105.094,89.782) text size=(9.594,20.000)
  item (119.094,72.282)-(151.542,87.282) structure origin=(119.844,79.782)
  item (165.542,0.000)-(395.042,103.782) arrow length=229.500 above=[structure origin=(200.292,70.032)] below=[structure origin=(259.864,96.282)]
  item (409.042,23.696)-(503.823,135.869) structure origin=(429.792,135.119)
  item (517.823,72.282)-(527.730,87.282) text size=(9.906,15.000)
table offset=(271.865,143.869)
table size=(0.000,0.000)
"#;

    const DECORATED_SHAPES: &str = r#"
clip (0.000,0.000)-(1400.000,900.000)
rect (30.000,40.000)-(573.730,191.869) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
line (58.750,162.423) (98.750,162.423) stroke=1.500/#000000ff
line (98.750,162.423) (118.750,127.782) stroke=1.500/#000000ff
line (98.719,155.477) (112.719,131.228) stroke=1.500/#000000ff
line (118.750,127.782) (98.750,93.141) stroke=1.500/#000000ff
line (98.750,93.141) (58.750,93.141) stroke=1.500/#000000ff
line (92.750,96.641) (64.750,96.641) stroke=1.500/#000000ff
line (58.750,93.141) (38.750,127.782) stroke=1.500/#000000ff
line (38.750,127.782) (58.750,162.423) stroke=1.500/#000000ff
line (44.781,131.228) (58.781,155.477) stroke=1.500/#000000ff
text "+" at=(133.500,117.782) sections=["+":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (157.844,127.782) (177.479,127.782) stroke=1.500/#000000ff
line (157.844,131.282) (177.479,131.282) stroke=1.500/#000000ff
line (157.844,124.282) (177.479,124.282) stroke=1.500/#000000ff
rect (178.479,119.282)-(190.542,136.282) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
line (200.510,127.782) (199.727,132.726) stroke=1.500/#dc1e1eff
line (197.455,137.187) (193.915,140.726) stroke=1.500/#dc1e1eff
line (189.455,142.999) (184.510,143.782) stroke=1.500/#dc1e1eff
line (179.566,142.999) (175.106,140.726) stroke=1.500/#dc1e1eff
line (171.566,137.187) (169.294,132.726) stroke=1.500/#dc1e1eff
line (168.510,127.782) (169.294,122.838) stroke=1.500/#dc1e1eff
line (171.566,118.377) (175.106,114.838) stroke=1.500/#dc1e1eff
line (179.566,112.565) (184.510,111.782) stroke=1.500/#dc1e1eff
line (189.455,112.565) (193.915,114.838) stroke=1.500/#dc1e1eff
line (197.455,118.377) (199.727,122.838) stroke=1.500/#dc1e1eff
text "O" at=(179.479,120.282) sections=["O":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (203.542,127.782) (428.042,127.782) stroke=1.500/#000000ff
path closed=true fill=#000000ff stroke=0.000/#00000000 pts=[(433.042,127.782) (423.042,123.782) (423.042,131.782)] kind=Middle
line (238.292,118.032) (278.292,118.032) stroke=1.500/#000000ff
line (278.292,118.032) (298.292,83.391) stroke=1.500/#000000ff
line (278.261,111.086) (292.261,86.837) stroke=1.500/#000000ff
line (298.292,83.391) (278.292,48.750) stroke=1.500/#000000ff
line (278.292,48.750) (238.292,48.750) stroke=1.500/#000000ff
line (272.292,52.250) (244.292,52.250) stroke=1.500/#000000ff
line (238.292,48.750) (218.292,83.391) stroke=1.500/#000000ff
line (218.292,83.391) (238.292,118.032) stroke=1.500/#000000ff
line (224.323,86.837) (238.323,111.086) stroke=1.500/#000000ff
line (358.292,118.032) (398.292,118.032) stroke=1.500/#000000ff
line (398.292,118.032) (418.292,83.391) stroke=1.500/#000000ff
line (398.261,111.086) (412.261,86.837) stroke=1.500/#000000ff
line (418.292,83.391) (398.292,48.750) stroke=1.500/#000000ff
line (398.292,48.750) (358.292,48.750) stroke=1.500/#000000ff
line (392.292,52.250) (364.292,52.250) stroke=1.500/#000000ff
line (358.292,48.750) (338.292,83.391) stroke=1.500/#000000ff
line (338.292,83.391) (358.292,118.032) stroke=1.500/#000000ff
line (344.323,86.837) (358.323,111.086) stroke=1.500/#000000ff
line (298.292,83.391) (338.292,83.391) stroke=1.500/#000000ff
line (297.864,144.282) (334.252,144.282) stroke=1.500/#000000ff
rect (335.252,135.782)-(340.470,152.782) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "I" at=(336.252,136.782) sections=["I":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (467.792,183.119) (507.792,183.119) stroke=1.500/#000000ff
line (507.792,183.119) (527.792,148.478) stroke=1.500/#000000ff
line (507.761,176.172) (521.761,151.924) stroke=1.500/#000000ff
line (527.792,148.478) (507.792,113.837) stroke=1.500/#000000ff
line (507.792,113.837) (467.792,113.837) stroke=1.500/#000000ff
line (501.792,117.337) (473.792,117.337) stroke=1.500/#000000ff
line (467.792,113.837) (447.792,148.478) stroke=1.500/#000000ff
line (447.792,148.478) (467.792,183.119) stroke=1.500/#000000ff
line (453.823,151.924) (467.823,176.172) stroke=1.500/#000000ff
line (507.792,113.837) (522.307,88.696) stroke=1.500/#000000ff
rect (521.761,70.696)-(542.823,87.696) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "OH" at=(522.761,71.696) sections=["OH":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "rt" at=(555.823,120.282) sections=["rt":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;
}
