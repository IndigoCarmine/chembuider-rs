//! Rich text for chemical structure labels: superscript / subscript / bold / italic.
//!
//! egui's [`LayoutJob`](egui::text::LayoutJob) can only fake sub/superscripts by
//! aligning a smaller font to the top or bottom of the tallest row, which drifts as the
//! label content changes. This module lays every styled span out as its own galley and
//! places it with explicit *baseline* arithmetic, so `CH₃`, `SO₄²⁻`, *N*-methyl or
//! **bold** fragments all sit exactly where typography says they should — at any zoom.
//!
//! Two entry points:
//! - [`RichText`]: the span model. Build it with the fluent methods ([`RichText::text`],
//!   [`RichText::subscript`], [`RichText::superscript`], …) or parse it from markup with
//!   [`RichText::parse`]. Paint it anywhere with [`RichText::paint`] (used by the
//!   structure renderer for atom labels).
//! - [`RichLabel`]: an [`egui::Widget`] wrapper so the same text can be dropped into
//!   ordinary UI (`ui.add(RichLabel::new("SO_4^{2-}"))`).
//!
//! # Markup
//!
//! | syntax          | result                                   |
//! |-----------------|------------------------------------------|
//! | `_2` / `_{aq}`  | subscript (one char / braced group)      |
//! | `^+` / `^{2-}`  | superscript (one char / braced group)    |
//! | `**text**`      | toggle bold                              |
//! | `//text//`      | toggle italic                            |
//! | `\x`            | literal `x` (escapes `_ ^ * / { } \`)    |
//!
//! Groups nest (`^{a_b}`), and bold/italic toggles apply inside groups. A marker that
//! cannot start a valid construct (e.g. a trailing `^`, a single `*`) stays literal, so
//! plain labels like `C*` or `N/A` render unchanged.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Vec2};

/// Size of sub/superscript glyphs relative to the base font size.
pub const SCRIPT_SCALE: f32 = 0.7;
/// How far (in fractions of the base font size) the superscript baseline is raised.
pub const SUPERSCRIPT_RISE: f32 = 0.38;
/// How far (in fractions of the base font size) the subscript baseline is lowered.
pub const SUBSCRIPT_DROP: f32 = 0.16;
/// Horizontal offset (in fractions of the base font size) of the faux-bold double-strike.
const BOLD_STRIKE: f32 = 0.04;

/// Vertical placement of a span relative to the main baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Script {
    #[default]
    Normal,
    /// Raised, smaller (charges: `Na⁺`, `SO₄²⁻`).
    Super,
    /// Lowered, smaller (formula counts: `H₂O`, `CH₃`).
    Sub,
}

/// Style of one contiguous run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub script: Script,
    /// `None` inherits the color given at paint time.
    pub color: Option<Color32>,
}

/// A run of text with a single [`SpanStyle`].
#[derive(Clone, Debug, PartialEq)]
pub struct RichSpan {
    pub text: String,
    pub style: SpanStyle,
}

/// Styled text as a list of spans. See the [module docs](self) for the markup syntax.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RichText {
    pub spans: Vec<RichSpan>,
}

impl RichText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Text taken verbatim — no markup interpretation.
    pub fn plain(text: impl Into<String>) -> Self {
        Self::new().text(text)
    }

    /// True if there is nothing to draw (no spans, or only empty ones).
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// Append a run with an explicit style.
    pub fn span(mut self, text: impl Into<String>, style: SpanStyle) -> Self {
        let text = text.into();
        if !text.is_empty() {
            // Merge with the previous span when the style is identical, so builders and
            // the parser can call this freely without fragmenting the span list.
            if let Some(last) = self.spans.last_mut()
                && last.style == style
            {
                last.text.push_str(&text);
                return self;
            }
            self.spans.push(RichSpan { text, style });
        }
        self
    }

    /// Append normal (base-line, regular) text.
    pub fn text(self, text: impl Into<String>) -> Self {
        self.span(text, SpanStyle::default())
    }

    /// Append a subscript run (e.g. the 2 in `H₂O`).
    pub fn subscript(self, text: impl Into<String>) -> Self {
        self.span(text, SpanStyle { script: Script::Sub, ..Default::default() })
    }

    /// Append a superscript run (e.g. the + in `Na⁺`).
    pub fn superscript(self, text: impl Into<String>) -> Self {
        self.span(text, SpanStyle { script: Script::Super, ..Default::default() })
    }

    /// Append a bold run.
    pub fn bold(self, text: impl Into<String>) -> Self {
        self.span(text, SpanStyle { bold: true, ..Default::default() })
    }

    /// Append an italic run (e.g. the *N* in *N*-methyl).
    pub fn italic(self, text: impl Into<String>) -> Self {
        self.span(text, SpanStyle { italic: true, ..Default::default() })
    }

    /// Parse markup (see the [module docs](self) for the grammar).
    pub fn parse(markup: &str) -> Self {
        let mut out = Self::new();
        let mut cur = String::new();
        let mut bold = false;
        let mut italic = false;
        // Innermost braced script group; bottom entry is the implicit Normal level.
        let mut scripts: Vec<Script> = vec![Script::Normal];

        let style = |bold: bool, italic: bool, script: Script| SpanStyle {
            bold,
            italic,
            script,
            color: None,
        };
        // Flush the accumulated literal run under the *current* style.
        macro_rules! flush {
            () => {
                if !cur.is_empty() {
                    out = out.span(std::mem::take(&mut cur), style(bold, italic, *scripts.last().unwrap()));
                }
            };
        }

        let mut chars = markup.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some(next) => cur.push(next),
                    None => cur.push('\\'), // trailing backslash stays literal
                },
                '*' if chars.peek() == Some(&'*') => {
                    chars.next();
                    flush!();
                    bold = !bold;
                }
                '/' if chars.peek() == Some(&'/') => {
                    chars.next();
                    flush!();
                    italic = !italic;
                }
                '^' | '_' => {
                    let script = if c == '^' { Script::Super } else { Script::Sub };
                    match chars.peek() {
                        Some('{') => {
                            chars.next();
                            flush!();
                            scripts.push(script);
                        }
                        // One-character shortcut: `_2`, `^+`, `_\{` (escaped char).
                        Some(&next) if !next.is_whitespace() && next != '{' && next != '}' => {
                            chars.next();
                            let ch = if next == '\\' { chars.next().unwrap_or('\\') } else { next };
                            flush!();
                            out = out.span(ch.to_string(), style(bold, italic, script));
                        }
                        // Nothing it can apply to: keep the marker literal.
                        _ => cur.push(c),
                    }
                }
                '}' if scripts.len() > 1 => {
                    flush!();
                    scripts.pop();
                }
                other => cur.push(other),
            }
        }
        flush!();
        out
    }

    /// The markup that would [`parse`](Self::parse) back to this text — the inverse of the
    /// parser, so an already-built [`RichText`] can be handed to a text box and edited.
    ///
    /// Scripts always come out braced (`K_{2}CO_{3}` rather than `K_2CO_3`) and every
    /// character the grammar reserves is escaped, which makes the output unambiguous rather
    /// than shortest. Parsing it again reproduces the same spans, which is the property that
    /// matters; a caller keeping the user's original spelling should keep the string.
    ///
    /// Colours are a paint-time concern the markup has no syntax for and are dropped.
    pub fn to_markup(&self) -> String {
        let mut out = String::new();
        let (mut bold, mut italic) = (false, false);
        for span in &self.spans {
            if span.text.is_empty() {
                continue;
            }
            if span.style.bold != bold {
                out.push_str("**");
                bold = span.style.bold;
            }
            if span.style.italic != italic {
                out.push_str("//");
                italic = span.style.italic;
            }
            let open = match span.style.script {
                Script::Normal => "",
                Script::Sub => "_{",
                Script::Super => "^{",
            };
            out.push_str(open);
            for c in span.text.chars() {
                if matches!(c, '\\' | '*' | '/' | '^' | '_' | '{' | '}') {
                    out.push('\\');
                }
                out.push(c);
            }
            if !open.is_empty() {
                out.push('}');
            }
        }
        // Close whatever is still open, so the string stands on its own.
        if bold {
            out.push_str("**");
        }
        if italic {
            out.push_str("//");
        }
        out
    }

    /// Lay the text out with `layout` (any source of galleys: a [`egui::Painter`], a
    /// `FontsView`, …). `style` provides the base font size / color / family.
    pub fn layout_with(
        &self,
        style: &RichTextStyle,
        mut layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
    ) -> RichGalley {
        // A span laid out on its own, before the common baseline is known.
        struct Pending {
            galley: Arc<egui::Galley>,
            x: f32,
            baseline: f32,
            shift: f32,
            bold_offset: Option<f32>,
        }
        let mut parts: Vec<RichGalleyPart> = Vec::with_capacity(self.spans.len());
        let mut pen_x = 0.0_f32;
        // Per-part vertical placement is resolved in two steps: first collect each part
        // with its own baseline and desired baseline *shift*, then translate everything
        // so the topmost part starts at y = 0.
        let mut placed: Vec<Pending> = Vec::new();
        for span in &self.spans {
            if span.text.is_empty() {
                continue;
            }
            let (scale, shift) = match span.style.script {
                Script::Normal => (1.0, 0.0),
                Script::Super => (SCRIPT_SCALE, -SUPERSCRIPT_RISE * style.size),
                Script::Sub => (SCRIPT_SCALE, SUBSCRIPT_DROP * style.size),
            };
            let format = TextFormat {
                font_id: FontId::new(style.size * scale, style.family.clone()),
                color: span.style.color.unwrap_or(style.color),
                italics: span.style.italic,
                ..Default::default()
            };
            let mut job = LayoutJob::default();
            job.append(&span.text, 0.0, format);
            let galley = layout(job);

            let baseline = galley_baseline(&galley);
            let bold_offset = span.style.bold.then(|| (style.size * BOLD_STRIKE).max(0.3));
            let advance = galley.rect.width() + bold_offset.unwrap_or(0.0);
            placed.push(Pending { galley, x: pen_x, baseline, shift, bold_offset });
            pen_x += advance;
        }

        if placed.is_empty() {
            return RichGalley { parts, size: Vec2::ZERO };
        }

        // Common main baseline: low enough that no raised part pokes above y = 0.
        let main_baseline = placed
            .iter()
            .map(|p| p.baseline - p.shift)
            .fold(f32::MIN, f32::max);
        let mut height = 0.0_f32;
        for p in placed {
            let top = main_baseline + p.shift - p.baseline;
            height = height.max(top + p.galley.rect.height());
            parts.push(RichGalleyPart {
                galley: p.galley,
                offset: Vec2::new(p.x, top),
                bold_offset: p.bold_offset,
            });
        }
        RichGalley { parts, size: Vec2::new(pen_x, height) }
    }

    /// Lay out and paint in one call, anchored like [`egui::Painter::text`].
    /// Returns the rect the text occupies.
    pub fn paint(
        &self,
        painter: &egui::Painter,
        anchor: Pos2,
        align: Align2,
        style: &RichTextStyle,
    ) -> Rect {
        let galley = self.layout_with(style, |job| painter.layout_job(job));
        galley.paint_anchored(painter, anchor, align)
    }
}

/// `RichText::parse` for `&str`, verbatim for `String`-less call sites like
/// `ui.add(RichLabel::new("H_2O"))`.
impl From<&str> for RichText {
    fn from(markup: &str) -> Self {
        Self::parse(markup)
    }
}

/// Base style for laying out a [`RichText`]: everything else (script sizes, offsets)
/// derives from these.
#[derive(Clone, Debug, PartialEq)]
pub struct RichTextStyle {
    /// Font size of normal spans, in points. Scripts use `size * SCRIPT_SCALE`.
    pub size: f32,
    /// Color of spans that don't specify their own.
    pub color: Color32,
    pub family: FontFamily,
}

impl Default for RichTextStyle {
    fn default() -> Self {
        Self {
            size: 16.0,
            color: Color32::BLACK,
            family: FontFamily::Proportional,
        }
    }
}

impl RichTextStyle {
    pub fn new(size: f32, color: Color32) -> Self {
        Self { size, color, ..Default::default() }
    }
}

/// One laid-out span: a galley plus where it goes relative to the text's top-left.
pub struct RichGalleyPart {
    pub galley: Arc<egui::Galley>,
    /// Offset of the galley's top-left from the [`RichGalley`]'s top-left.
    pub offset: Vec2,
    /// `Some(dx)` for faux-bold: the galley is painted twice, the second time shifted
    /// right by `dx` (double-strike emboldening — egui's default fonts have no bold face).
    pub bold_offset: Option<f32>,
}

/// The result of laying out a [`RichText`]: ready to paint, knows its size.
pub struct RichGalley {
    pub parts: Vec<RichGalleyPart>,
    /// Tight size of the whole laid-out text.
    pub size: Vec2,
}

impl RichGalley {
    pub fn size(&self) -> Vec2 {
        self.size
    }

    /// Paint with the text's top-left corner at `top_left`.
    pub fn paint(&self, painter: &egui::Painter, top_left: Pos2) {
        for part in &self.parts {
            let pos = top_left + part.offset;
            painter.galley(pos, part.galley.clone(), Color32::BLACK);
            if let Some(dx) = part.bold_offset {
                painter.galley(pos + Vec2::new(dx, 0.0), part.galley.clone(), Color32::BLACK);
            }
        }
    }

    /// Paint anchored to `anchor` (e.g. `Align2::CENTER_CENTER` to center on an atom).
    /// Returns the rect the text occupies.
    pub fn paint_anchored(&self, painter: &egui::Painter, anchor: Pos2, align: Align2) -> Rect {
        let rect = align.anchor_size(anchor, self.size);
        self.paint(painter, rect.min);
        rect
    }

    /// Bounding rect (relative to this galley's top-left) of a range of characters of the
    /// *flattened* text — the concatenation of every span's text, in order. Used to anchor
    /// a chemical label on its element symbol rather than the whole bounding box.
    ///
    /// Char-based (not span-based) because [`RichText::span`] merges adjacent same-style
    /// runs, so "the element symbol" is in general a prefix/slice of a larger part.
    ///
    /// The vertical extent is each covered part's own line box (not the whole galley), so
    /// the anchor stays on the base text even when a superscript grows the galley upward.
    /// Returns `None` when the range covers no glyphs.
    pub fn char_range_rect(&self, chars: std::ops::Range<usize>) -> Option<Rect> {
        let mut rect: Option<Rect> = None;
        let mut idx = 0usize; // running char index over the flattened text
        for part in &self.parts {
            for row in &part.galley.rows {
                for glyph in &row.row.glyphs {
                    if chars.contains(&idx) {
                        let x0 = part.offset.x + row.pos.x + glyph.pos.x;
                        let x1 = x0 + glyph.advance_width + part.bold_offset.unwrap_or(0.0);
                        let g = Rect::from_min_max(
                            Pos2::new(x0, part.offset.y),
                            Pos2::new(x1, part.offset.y + part.galley.rect.height()),
                        );
                        rect = Some(rect.map_or(g, |r| r.union(g)));
                    }
                    idx += 1;
                }
            }
        }
        rect
    }
}

/// Baseline of a single-row galley, measured from the galley's top. epaint stores glyph
/// positions relative to the baseline, so any glyph gives it away; an empty galley
/// (whitespace-only text can still produce glyphs, but e.g. a stripped-out control char
/// may not) falls back to a typographic estimate.
fn galley_baseline(galley: &egui::Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| {
            let glyph = row.row.glyphs.first()?;
            Some(row.pos.y + glyph.pos.y)
        })
        .unwrap_or(galley.rect.height() * 0.8)
}

// ─── egui widget ─────────────────────────────────────────────────────────────

/// [`RichText`] as an ordinary [`egui::Widget`]:
///
/// ```no_run
/// # use chembuider_rs::{egui, RichLabel};
/// # fn demo(ui: &mut egui::Ui) {
/// ui.add(RichLabel::new("SO_4^{2-}"));                 // markup
/// ui.add(RichLabel::new("//J// = 7.2 Hz").size(18.0)); // italic, custom size
/// # }
/// ```
pub struct RichLabel {
    text: RichText,
    size: Option<f32>,
    color: Option<Color32>,
    family: FontFamily,
}

impl RichLabel {
    /// `text` can be a [`RichText`] built by hand, or a `&str` of markup.
    pub fn new(text: impl Into<RichText>) -> Self {
        Self {
            text: text.into(),
            size: None,
            color: None,
            family: FontFamily::Proportional,
        }
    }

    /// Base font size in points (default: the current body text size).
    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    /// Base text color (default: the theme's text color).
    pub fn color(mut self, color: Color32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn family(mut self, family: FontFamily) -> Self {
        self.family = family;
        self
    }
}

impl egui::Widget for RichLabel {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let size = self
            .size
            .unwrap_or_else(|| egui::TextStyle::Body.resolve(ui.style()).size);
        let color = self.color.unwrap_or_else(|| ui.visuals().text_color());
        let style = RichTextStyle { size, color, family: self.family };
        let galley = self.text.layout_with(&style, |job| ui.painter().layout_job(job));
        let (rect, response) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            galley.paint(ui.painter(), rect.min);
        }
        response
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn spans(markup: &str) -> Vec<(String, SpanStyle)> {
        RichText::parse(markup)
            .spans
            .into_iter()
            .map(|s| (s.text, s.style))
            .collect()
    }

    fn normal() -> SpanStyle {
        SpanStyle::default()
    }
    fn sub() -> SpanStyle {
        SpanStyle { script: Script::Sub, ..Default::default() }
    }
    fn sup() -> SpanStyle {
        SpanStyle { script: Script::Super, ..Default::default() }
    }

    #[test]
    fn parses_plain_text() {
        assert_eq!(spans("OTf"), vec![("OTf".into(), normal())]);
        assert!(RichText::parse("").is_empty());
    }

    #[test]
    fn parses_single_char_scripts() {
        assert_eq!(
            spans("CH_3"),
            vec![("CH".into(), normal()), ("3".into(), sub())]
        );
        assert_eq!(
            spans("Na^+"),
            vec![("Na".into(), normal()), ("+".into(), sup())]
        );
    }

    #[test]
    fn parses_braced_groups() {
        assert_eq!(
            spans("SO_4^{2-}"),
            vec![
                ("SO".into(), normal()),
                ("4".into(), sub()),
                ("2-".into(), sup()),
            ]
        );
        // Nested: group content can itself contain script markers.
        assert_eq!(
            spans("^{a_b}c"),
            vec![("a".into(), sup()), ("b".into(), sub()), ("c".into(), normal())]
        );
    }

    /// [`RichText::to_markup`] is the parser's inverse: whatever the editing UI hands to a
    /// text box has to parse back to the very same spans, or an edit would silently rewrite
    /// the text it was only meant to show.
    #[test]
    fn markup_round_trips_through_to_markup() {
        for markup in [
            "MeI",
            "K_2CO_3",
            "SO_4^{2-}",
            "**bold** and //italic//",
            "**b_2**",
            "N/A",
            "50 °C, 12 h",
            "",
        ] {
            let parsed = RichText::parse(markup);
            let round = RichText::parse(&parsed.to_markup());
            assert_eq!(
                round.spans, parsed.spans,
                "'{markup}' → '{}' did not survive re-parsing",
                parsed.to_markup()
            );
        }
        // Scripts always come back braced, and the reserved characters are escaped.
        assert_eq!(RichText::parse("K_2CO_3").to_markup(), "K_{2}CO_{3}");
        assert_eq!(RichText::plain("a_b^c*d").to_markup(), r"a\_b\^c\*d");
        // A style left open at the end is closed, so the string stands on its own.
        assert_eq!(RichText::parse("a**b").to_markup(), "a**b**");
    }

    #[test]
    fn parses_bold_and_italic_toggles() {
        assert_eq!(
            spans("**B**old"),
            vec![
                ("B".into(), SpanStyle { bold: true, ..Default::default() }),
                ("old".into(), normal()),
            ]
        );
        assert_eq!(
            spans("//N//-methyl"),
            vec![
                ("N".into(), SpanStyle { italic: true, ..Default::default() }),
                ("-methyl".into(), normal()),
            ]
        );
        // Styles combine inside a script group.
        assert_eq!(
            spans("_{**2**}"),
            vec![("2".into(), SpanStyle { bold: true, script: Script::Sub, ..Default::default() })]
        );
    }

    #[test]
    fn escapes_and_lone_markers_stay_literal() {
        assert_eq!(spans(r"a\_b"), vec![("a_b".into(), normal())]);
        assert_eq!(spans(r"5\^2"), vec![("5^2".into(), normal())]);
        // Trailing marker has nothing to apply to.
        assert_eq!(spans("x^"), vec![("x^".into(), normal())]);
        assert_eq!(spans("x_ y"), vec![("x_ y".into(), normal())]);
        // Single `*` and `/` are ordinary characters (C* isotope labels, N/A, …).
        assert_eq!(spans("C*"), vec![("C*".into(), normal())]);
        assert_eq!(spans("N/A"), vec![("N/A".into(), normal())]);
        // Stray closing brace without an open group.
        assert_eq!(spans("a}b"), vec![("a}b".into(), normal())]);
        assert_eq!(spans(r"\\"), vec![("\\".into(), normal())]);
    }

    #[test]
    fn adjacent_same_style_runs_merge() {
        let rt = RichText::new().text("C").text("H").subscript("3");
        assert_eq!(
            rt.spans,
            vec![
                RichSpan { text: "CH".into(), style: normal() },
                RichSpan { text: "3".into(), style: sub() },
            ]
        );
    }

    // ── Layout tests use a real font stack (no GPU needed) ──────────────────

    fn layout(rt: &RichText, style: &RichTextStyle) -> RichGalley {
        let mut fonts = egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            egui::FontDefinitions::default(),
        );
        let mut view = fonts.with_pixels_per_point(1.0);
        rt.layout_with(style, |job| view.layout_job(job))
    }

    #[test]
    fn layout_places_scripts_around_the_baseline() {
        let style = RichTextStyle::new(20.0, Color32::BLACK);
        let g = layout(&RichText::parse("X_2^3"), &style);
        assert_eq!(g.parts.len(), 3);
        let base = &g.parts[0];
        let sub = &g.parts[1];
        let sup = &g.parts[2];

        // Scripts are smaller than the base text.
        assert!(sub.galley.rect.height() < base.galley.rect.height());
        assert!(sup.galley.rect.height() < base.galley.rect.height());

        // Subscript extends below the base span; superscript starts above the base top.
        let base_bottom = base.offset.y + base.galley.rect.height();
        let sub_bottom = sub.offset.y + sub.galley.rect.height();
        assert!(
            sub_bottom > base_bottom,
            "subscript should hang below the base run ({sub_bottom} vs {base_bottom})"
        );
        assert!(
            sup.offset.y < base.offset.y,
            "superscript should rise above the base run's top ({} vs {})",
            sup.offset.y,
            base.offset.y
        );

        // Spans advance left to right without overlap.
        assert!(sub.offset.x >= base.offset.x + base.galley.rect.width() - 0.01);
        assert!(sup.offset.x >= sub.offset.x + sub.galley.rect.width() - 0.01);

        // The layout is tight: something starts at y = 0 and the size covers everything.
        let min_top = g.parts.iter().map(|p| p.offset.y).fold(f32::MAX, f32::min);
        assert!(min_top.abs() < 0.01, "topmost part should sit at y = 0, got {min_top}");
        for p in &g.parts {
            assert!(p.offset.y + p.galley.rect.height() <= g.size.y + 0.01);
            assert!(p.offset.x + p.galley.rect.width() <= g.size.x + 0.01);
        }
    }

    #[test]
    fn layout_of_plain_text_matches_single_galley() {
        let style = RichTextStyle::new(16.0, Color32::BLACK);
        let g = layout(&RichText::plain("NH"), &style);
        assert_eq!(g.parts.len(), 1);
        assert!(g.parts[0].offset == Vec2::ZERO);
        assert_eq!(g.size, g.parts[0].galley.rect.size());
    }

    #[test]
    fn bold_spans_reserve_the_double_strike_width() {
        let style = RichTextStyle::new(16.0, Color32::BLACK);
        let plain = layout(&RichText::plain("Br"), &style);
        let bold = layout(&RichText::new().bold("Br"), &style);
        let dx = bold.parts[0].bold_offset.expect("bold span must carry a strike offset");
        assert!(dx > 0.0);
        assert!((bold.size.x - (plain.size.x + dx)).abs() < 0.01);
    }

    #[test]
    fn empty_text_lays_out_to_nothing() {
        let style = RichTextStyle::default();
        let g = layout(&RichText::new(), &style);
        assert!(g.parts.is_empty());
        assert_eq!(g.size, Vec2::ZERO);
    }

    #[test]
    fn char_range_rect_measures_the_flattened_text() {
        let style = RichTextStyle::new(20.0, Color32::BLACK);

        // "NH" merges into one part; the first char's rect must stop short of the whole.
        let g = layout(&RichText::plain("NH"), &style);
        assert_eq!(g.parts.len(), 1);
        let n = g.char_range_rect(0..1).expect("N rect");
        let all = g.char_range_rect(0..2).expect("NH rect");
        assert!(n.width() > 0.0);
        assert!(n.max.x < all.max.x, "N alone must end before NH ({n:?} vs {all:?})");
        assert!((all.width() - g.size.x).abs() < 0.5, "full range spans the galley width");

        // "H2N": chars are H(0) 2(1) N(2); the N rect sits right of the subscript.
        let g = layout(&RichText::new().text("H").subscript("2").text("N"), &style);
        let two = g.char_range_rect(1..2).expect("2 rect");
        let n = g.char_range_rect(2..3).expect("N rect");
        assert!(n.min.x >= two.max.x - 0.5, "N starts after the subscript 2");

        // "O^-": the O anchor's vertical center is the base part's middle, not the
        // (superscript-extended) whole-galley middle.
        let g = layout(&RichText::new().text("O").superscript("-"), &style);
        let o = g.char_range_rect(0..1).expect("O rect");
        let base = &g.parts[0];
        let base_mid = base.offset.y + base.galley.rect.height() * 0.5;
        assert!((o.center().y - base_mid).abs() < 0.01);
        assert!(g.size.y > base.galley.rect.height() - 0.01);

        // Out-of-range / empty requests.
        assert!(g.char_range_rect(5..9).is_none());
        assert!(g.char_range_rect(1..1).is_none());
    }

    /// The widget path: `ui.add(RichLabel)` in a headless egui frame allocates a
    /// non-empty rect and paints one galley per styled span.
    #[test]
    fn rich_label_widget_paints_in_a_frame() {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 240.0))),
            ..Default::default()
        };
        let output = ctx.run_ui(raw, |ui| {
            let response = ui.add(RichLabel::new("SO_4^{2-}"));
            assert!(response.rect.width() > 0.0 && response.rect.height() > 0.0);
        });
        fn count_text(shape: &egui::Shape) -> usize {
            match shape {
                egui::Shape::Text(_) => 1,
                egui::Shape::Vec(shapes) => shapes.iter().map(count_text).sum(),
                _ => 0,
            }
        }
        let text_shapes: usize = output.shapes.iter().map(|c| count_text(&c.shape)).sum();
        assert!(
            text_shapes >= 3,
            "SO_4^{{2-}} should paint at least 3 galleys (SO / 4 / 2-), got {text_shapes}"
        );
    }
}
