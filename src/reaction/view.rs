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
use crate::molecule::cdx_codec::CdxObject;
use crate::widget::ChemStructEditor;

use super::model::Reaction;
use super::scheme::{ReactionScheme, SchemeLayout, measure_scheme, paint_scheme};
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
        // A multi-step scheme is a drawing only: its table would describe one step of several.
        let table = if reaction.is_multi_step() {
            AnalysisTable::default()
        } else {
            AnalysisTable::from_result(&reaction.analysis.compute())
        };
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

    /// The figure's scheme as a ChemDraw document, laid out in ChemDraw's units with this
    /// view's style (see [`cdxml`](super::cdxml)). Write it with [`ReactionView::scheme_cdxml`]
    /// or [`ReactionView::scheme_cdx`]; the table is not part of it.
    pub fn scheme_document(&mut self, painter: &Painter, scheme: &ReactionScheme) -> CdxObject {
        super::cdxml::scheme_document(scheme, painter, &self.style, &self.structure_style, &mut self.scratch)
    }

    /// The figure's scheme as CDXML text.
    pub fn scheme_cdxml(&mut self, painter: &Painter, scheme: &ReactionScheme) -> String {
        super::cdxml::cdxml_of(&self.scheme_document(painter, scheme))
    }

    /// The figure's scheme as a binary CDX file.
    pub fn scheme_cdx(&mut self, painter: &Painter, scheme: &ReactionScheme) -> Vec<u8> {
        crate::molecule::cdx_codec::write(&self.scheme_document(painter, scheme))
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
                PlacedKind::Structure { origin, label: None } => format!("structure origin={}", fp_vec(*origin)),
                PlacedKind::Structure { origin, label: Some(l) } => {
                    format!("structure origin={} label={}", fp_vec(*origin), fp_rect(l.rect))
                }
                PlacedKind::Text(t) => format!("text size={}", fp_vec(t.size)),
                PlacedKind::LineBreak => "line break".to_string(),
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
        styled_figure_fingerprints(ReactionView::new(), reaction)
    }

    /// As [`figure_fingerprints`], with a view whose styles the caller has set.
    fn styled_figure_fingerprints(mut view: ReactionView, reaction: &Reaction) -> (String, String) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1400.0, 900.0))),
            ..Default::default()
        };
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

    /// A structure-drawing style with *every* field moved off its default, so a figure that
    /// ignored [`ReactionView::structure_style`] would paint differently.
    fn non_default_structure_style() -> StyleConfig {
        let d = StyleConfig::default();
        let s = StyleConfig {
            label_size: 17.0,
            bond_width: 2.25,
            double_bond_offset: 5.0,
            atom_bg_radius: 12.0,
            wedge_width: 7.5,
            bold_width: 6.0,
            dash_len: 3.0,
            dash_gap: 2.5,
            wavy_amplitude: 4.5,
            label_trim_pad: 3.5,
            label_bg_pad: 2.0,
        };
        for (name, value, default) in [
            ("label_size", s.label_size, d.label_size),
            ("bond_width", s.bond_width, d.bond_width),
            ("double_bond_offset", s.double_bond_offset, d.double_bond_offset),
            ("atom_bg_radius", s.atom_bg_radius, d.atom_bg_radius),
            ("wedge_width", s.wedge_width, d.wedge_width),
            ("bold_width", s.bold_width, d.bold_width),
            ("dash_len", s.dash_len, d.dash_len),
            ("dash_gap", s.dash_gap, d.dash_gap),
            ("wavy_amplitude", s.wavy_amplitude, d.wavy_amplitude),
            ("label_trim_pad", s.label_trim_pad, d.label_trim_pad),
            ("label_bg_pad", s.label_bg_pad, d.label_bg_pad),
        ] {
            assert_ne!(value, default, "{name} must differ from its default");
        }
        s
    }

    /// One structure carrying every special bond kind — solid wedge, hashed wedge, bold,
    /// dashed and wavy — next to a double bond, labelled atoms (OH, NH2) and an over-valence
    /// C≡O, so each bond kind's widths, dashes and fills are pinned under a non-default style.
    fn every_bond_kind() -> crate::molecule::Molecule {
        use crate::molecule::BondStereo;
        let mut m = crate::molecule::Molecule::default();
        let c0 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c1 = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        let c2 = m.add_atom("C".to_string(), [-0.75, 1.3], 0);
        let o3 = m.add_atom("O".to_string(), [-0.75, -1.3], 0);
        let n4 = m.add_atom("N".to_string(), [2.25, 1.3], 0);
        let c5 = m.add_atom("C".to_string(), [2.25, -1.3], 0);
        let c6 = m.add_atom("C".to_string(), [-2.25, 1.3], 0);
        let o7 = m.add_atom("O".to_string(), [-3.0, 2.6], 0);
        let stereo = [
            (m.add_bond(c0, c1, BondOrder::Single), BondStereo::WedgeUp),
            (m.add_bond(c0, c2, BondOrder::Single), BondStereo::WedgeDown),
            (m.add_bond(c0, o3, BondOrder::Single), BondStereo::Bold),
            (m.add_bond(c1, n4, BondOrder::Single), BondStereo::Dashed),
            (m.add_bond(c1, c5, BondOrder::Single), BondStereo::Wavy),
        ];
        m.add_bond(c2, c6, BondOrder::Double);
        m.add_bond(c6, o7, BondOrder::Triple);
        for (id, kind) in stereo {
            m.bonds.iter_mut().find(|b| b.id == id).expect("bond exists").stereo = kind;
        }
        for kind in [
            BondStereo::WedgeUp,
            BondStereo::WedgeDown,
            BondStereo::Bold,
            BondStereo::Dashed,
            BondStereo::Wavy,
        ] {
            assert!(m.bonds.iter().any(|b| b.stereo == kind), "missing a {kind:?} bond");
        }
        assert!(
            crate::widget::draw::is_valence_invalid(&m, o7),
            "the C≡O oxygen must be over-valence"
        );
        m
    }

    /// The bond-kind structure on the main line *and* above an arrow, drawn through a view
    /// with a non-default structure style and a non-default bond length and arrow text gap.
    fn styled_fingerprints() -> (String, String) {
        let structure_style = non_default_structure_style();
        let style = ReactionStyle {
            bond_length: 30.0,
            arrow_text_gap: 9.0,
            ..ReactionStyle::default()
        };
        assert_ne!(style.bond_length, ReactionStyle::default().bond_length);
        assert_ne!(style.arrow_text_gap, ReactionStyle::default().arrow_text_gap);

        let scheme = ReactionScheme::new()
            .structure(every_bond_kind())
            .arrow(Arrow::new().above_structure(every_bond_kind()).below("heat"))
            .structure(demo::phenol());
        let reaction = Reaction::new(scheme, Analysis::new());
        let view = ReactionView::new().with_style(style).with_structure_style(structure_style);
        let (layout, shapes) = styled_figure_fingerprints(view, &reaction);
        assert!(
            layout.contains("above=[structure") && layout.contains("below=[text"),
            "the arrow carries the bond-kind structure above and text below:\n{layout}"
        );
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
        assert_eq!(styled_fingerprints(), styled_fingerprints());
    }

    #[test]
    fn styled_figure_layout_is_pinned() {
        assert_pinned("styled layout", &styled_fingerprints().0, STYLED_LAYOUT);
    }

    #[test]
    fn styled_figure_shapes_are_pinned() {
        assert_pinned("styled shapes", &styled_fingerprints().1, STYLED_SHAPES);
    }

    const STYLED_LAYOUT: &str = r#"
figure size=(423.651,176.000)
scheme offset=(8.000,8.000)
scheme size=(407.651,160.000) center_line=111.000
  item (0.000,62.000)-(136.084,160.000) structure origin=(66.579,98.000)
  item (150.084,0.000)-(314.167,139.000) arrow length=164.084 above=[structure origin=(230.663,36.000)] below=[text size=(26.156,15.000)]
  item (328.167,66.466)-(407.651,155.534) structure origin=(344.292,154.409)
table offset=(211.825,168.000)
table size=(0.000,0.000)
"#;

    const STYLED_SHAPES: &str = r#"
clip (0.000,0.000)-(1400.000,900.000)
rect (30.000,40.000)-(453.651,216.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
path closed=true fill=#000000ff stroke=0.000/#00000000 pts=[(104.579,146.000) (134.579,153.500) (134.579,138.500)] kind=Middle
line (100.996,149.709) (103.162,150.958) stroke=2.250/#000000ff
line (97.414,153.417) (101.744,155.916) stroke=2.250/#000000ff
line (93.831,157.126) (100.327,160.874) stroke=2.250/#000000ff
line (90.248,160.835) (98.910,165.832) stroke=2.250/#000000ff
line (86.665,164.543) (97.493,170.790) stroke=2.250/#000000ff
line (83.083,168.252) (96.075,175.748) stroke=2.250/#000000ff
line (104.579,146.000) (97.367,133.500) stroke=6.000/#000000ff
line (134.579,146.000) (136.078,148.599) stroke=2.250/#000000ff
line (137.327,150.764) (138.827,153.363) stroke=2.250/#000000ff
line (140.076,155.528) (141.575,158.127) stroke=2.250/#000000ff
line (134.579,146.000) (138.273,145.965) stroke=2.250/#000000ff
line (138.273,145.965) (140.352,144.999) stroke=2.250/#000000ff
line (140.352,144.999) (140.148,142.715) stroke=2.250/#000000ff
line (140.148,142.715) (138.329,139.500) stroke=2.250/#000000ff
line (138.329,139.500) (136.510,136.285) stroke=2.250/#000000ff
line (136.510,136.285) (136.306,134.001) stroke=2.250/#000000ff
line (136.306,134.001) (138.385,133.035) stroke=2.250/#000000ff
line (138.385,133.035) (142.079,133.000) stroke=2.250/#000000ff
line (142.079,133.000) (145.773,132.965) stroke=2.250/#000000ff
line (145.773,132.965) (147.852,131.999) stroke=2.250/#000000ff
line (147.852,131.999) (147.648,129.715) stroke=2.250/#000000ff
line (147.648,129.715) (145.829,126.500) stroke=2.250/#000000ff
line (145.829,126.500) (144.010,123.285) stroke=2.250/#000000ff
line (144.010,123.285) (143.806,121.001) stroke=2.250/#000000ff
line (143.806,121.001) (145.885,120.035) stroke=2.250/#000000ff
line (145.885,120.035) (149.579,120.000) stroke=2.250/#000000ff
line (89.579,169.500) (59.579,169.500) stroke=2.250/#000000ff
line (89.579,174.500) (59.579,174.500) stroke=2.250/#000000ff
line (59.579,172.000) (52.367,184.500) stroke=2.250/#000000ff
line (55.248,169.501) (48.037,182.001) stroke=2.250/#000000ff
line (63.910,174.499) (56.698,186.999) stroke=2.250/#000000ff
rect (141.553,160.000)-(176.084,185.720) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (36.000,186.000)-(53.156,210.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (70.000,108.000)-(98.938,132.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "HO" at=(72.000,110.000) sections=["HO":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "NH" at=(143.553,162.000) sections=["NH":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "2" at=(167.365,169.720) sections=["2":#000000ff@11.900] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (78.579,172.000) (77.649,177.871) stroke=1.500/#dc1e1eff
line (74.950,183.168) (70.747,187.371) stroke=1.500/#dc1e1eff
line (65.450,190.070) (59.579,191.000) stroke=1.500/#dc1e1eff
line (53.708,190.070) (48.411,187.371) stroke=1.500/#dc1e1eff
line (44.208,183.168) (41.509,177.871) stroke=1.500/#dc1e1eff
line (40.579,172.000) (41.509,166.129) stroke=1.500/#dc1e1eff
line (44.208,160.832) (48.411,156.629) stroke=1.500/#dc1e1eff
line (53.708,153.930) (59.579,153.000) stroke=1.500/#dc1e1eff
line (65.450,153.930) (70.747,156.629) stroke=1.500/#dc1e1eff
line (74.950,160.832) (77.649,166.129) stroke=1.500/#dc1e1eff
line (63.579,198.000) (62.649,203.871) stroke=1.500/#dc1e1eff
line (59.950,209.168) (55.747,213.371) stroke=1.500/#dc1e1eff
line (50.450,216.070) (44.579,217.000) stroke=1.500/#dc1e1eff
line (38.708,216.070) (33.411,213.371) stroke=1.500/#dc1e1eff
line (29.208,209.168) (26.509,203.871) stroke=1.500/#dc1e1eff
line (25.579,198.000) (26.509,192.129) stroke=1.500/#dc1e1eff
line (29.208,186.832) (33.411,182.629) stroke=1.500/#dc1e1eff
line (38.708,179.930) (44.579,179.000) stroke=1.500/#dc1e1eff
line (50.450,179.930) (55.747,182.629) stroke=1.500/#dc1e1eff
line (59.950,186.832) (62.649,192.129) stroke=1.500/#dc1e1eff
text "O" at=(38.000,188.000) sections=["O":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (188.084,159.000) (347.167,159.000) stroke=1.500/#000000ff
path closed=true fill=#000000ff stroke=0.000/#00000000 pts=[(352.167,159.000) (342.167,155.000) (342.167,163.000)] kind=Middle
path closed=true fill=#000000ff stroke=0.000/#00000000 pts=[(268.663,84.000) (298.663,91.500) (298.663,76.500)] kind=Middle
line (265.080,87.709) (267.245,88.958) stroke=2.250/#000000ff
line (261.497,91.417) (265.828,93.916) stroke=2.250/#000000ff
line (257.915,95.126) (264.411,98.874) stroke=2.250/#000000ff
line (254.332,98.835) (262.994,103.832) stroke=2.250/#000000ff
line (250.749,102.543) (261.576,108.790) stroke=2.250/#000000ff
line (247.166,106.252) (260.159,113.748) stroke=2.250/#000000ff
line (268.663,84.000) (261.451,71.500) stroke=6.000/#000000ff
line (298.663,84.000) (300.162,86.599) stroke=2.250/#000000ff
line (301.411,88.764) (302.910,91.363) stroke=2.250/#000000ff
line (304.160,93.528) (305.659,96.127) stroke=2.250/#000000ff
line (298.663,84.000) (302.356,83.965) stroke=2.250/#000000ff
line (302.356,83.965) (304.436,82.999) stroke=2.250/#000000ff
line (304.436,82.999) (304.231,80.715) stroke=2.250/#000000ff
line (304.231,80.715) (302.413,77.500) stroke=2.250/#000000ff
line (302.413,77.500) (300.594,74.285) stroke=2.250/#000000ff
line (300.594,74.285) (300.390,72.001) stroke=2.250/#000000ff
line (300.390,72.001) (302.469,71.035) stroke=2.250/#000000ff
line (302.469,71.035) (306.163,71.000) stroke=2.250/#000000ff
line (306.163,71.000) (309.856,70.965) stroke=2.250/#000000ff
line (309.856,70.965) (311.936,69.999) stroke=2.250/#000000ff
line (311.936,69.999) (311.731,67.715) stroke=2.250/#000000ff
line (311.731,67.715) (309.913,64.500) stroke=2.250/#000000ff
line (309.913,64.500) (308.094,61.285) stroke=2.250/#000000ff
line (308.094,61.285) (307.890,59.001) stroke=2.250/#000000ff
line (307.890,59.001) (309.969,58.035) stroke=2.250/#000000ff
line (309.969,58.035) (313.663,58.000) stroke=2.250/#000000ff
line (253.663,107.500) (223.663,107.500) stroke=2.250/#000000ff
line (253.663,112.500) (223.663,112.500) stroke=2.250/#000000ff
line (223.663,110.000) (216.451,122.500) stroke=2.250/#000000ff
line (219.332,107.501) (212.120,120.001) stroke=2.250/#000000ff
line (227.994,112.499) (220.782,124.999) stroke=2.250/#000000ff
rect (200.084,124.000)-(217.240,148.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (234.084,46.000)-(263.021,70.000) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
rect (305.636,98.000)-(340.167,123.720) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "HO" at=(236.084,48.000) sections=["HO":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "NH" at=(307.636,100.000) sections=["NH":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "2" at=(331.449,107.720) sections=["2":#000000ff@11.900] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (242.663,110.000) (241.733,115.871) stroke=1.500/#dc1e1eff
line (239.034,121.168) (234.831,125.371) stroke=1.500/#dc1e1eff
line (229.534,128.070) (223.663,129.000) stroke=1.500/#dc1e1eff
line (217.791,128.070) (212.495,125.371) stroke=1.500/#dc1e1eff
line (208.291,121.168) (205.593,115.871) stroke=1.500/#dc1e1eff
line (204.663,110.000) (205.593,104.129) stroke=1.500/#dc1e1eff
line (208.291,98.832) (212.495,94.629) stroke=1.500/#dc1e1eff
line (217.791,91.930) (223.663,91.000) stroke=1.500/#dc1e1eff
line (229.534,91.930) (234.831,94.629) stroke=1.500/#dc1e1eff
line (239.034,98.832) (241.733,104.129) stroke=1.500/#dc1e1eff
line (227.663,136.000) (226.733,141.871) stroke=1.500/#dc1e1eff
line (224.034,147.168) (219.831,151.371) stroke=1.500/#dc1e1eff
line (214.534,154.070) (208.663,155.000) stroke=1.500/#dc1e1eff
line (202.791,154.070) (197.495,151.371) stroke=1.500/#dc1e1eff
line (193.291,147.168) (190.593,141.871) stroke=1.500/#dc1e1eff
line (189.663,136.000) (190.593,130.129) stroke=1.500/#dc1e1eff
line (193.291,124.832) (197.495,120.629) stroke=1.500/#dc1e1eff
line (202.791,117.930) (208.663,117.000) stroke=1.500/#dc1e1eff
line (214.534,117.930) (219.831,120.629) stroke=1.500/#dc1e1eff
line (224.034,124.832) (226.733,130.129) stroke=1.500/#dc1e1eff
text "O" at=(202.084,126.000) sections=["O":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
text "heat" at=(257.047,172.000) sections=["heat":#000000ff@13.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
line (382.292,202.409) (412.292,202.409) stroke=2.250/#000000ff
line (412.292,202.409) (427.292,176.428) stroke=2.250/#000000ff
line (410.212,196.012) (420.712,177.825) stroke=2.250/#000000ff
line (427.292,176.428) (412.292,150.447) stroke=2.250/#000000ff
line (412.292,150.447) (382.292,150.447) stroke=2.250/#000000ff
line (407.792,155.447) (386.792,155.447) stroke=2.250/#000000ff
line (382.292,150.447) (367.292,176.428) stroke=2.250/#000000ff
line (367.292,176.428) (382.292,202.409) stroke=2.250/#000000ff
line (373.873,177.825) (384.373,196.012) stroke=2.250/#000000ff
line (412.292,150.447) (419.498,137.966) stroke=2.250/#000000ff
rect (418.713,112.466)-(447.651,136.466) fill=#ffffffff stroke=0.000/#00000000 corner=0/0/0/0 kind=Outside blur=0.000 round_px=None angle=0.000 brush=none
text "OH" at=(420.713,114.466) sections=["OH":#000000ff@17.000] families=[Proportional] override=none opacity=1.000 angle=0.000 fallback=#000000ff underline=0.000/#00000000
"#;

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
