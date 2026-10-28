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
//! | `{@Font:text}`  | `text` in the font family `Font`         |
//! | `{@Font,12:text}` | … at 12 pt; `{@,12:text}` size only    |
//!
//! Groups nest (`^{a_b}`, `{@Symbol:a_{2}}`), and bold/italic toggles apply inside groups. A
//! marker that cannot start a valid construct (e.g. a trailing `^`, a single `*`, a `{@` with
//! no `:`) stays literal, so plain labels like `C*` or `N/A` render unchanged.
//!
//! # Fonts
//!
//! A font group names a font family and/or a size in points (the base size of the text counts
//! as [`BASE_POINTS`], ChemDraw's 10 pt). Inside the family name `\` escapes `: , { } \`.
//! In the `Symbol` font, ChemDraw's Greek font, the text is kept as typed (`{@Symbol:p}`) and
//! shown as Greek (π); see [`fonts`](super::fonts). Fonts are drawn through egui's
//! [`FontFamily::Name`], which [`fonts::install`](super::fonts::install) makes draw the system
//! font of that name.

use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Vec2};
use serde::{Deserialize, Serialize};

use super::fonts::{self, FontName};

/// The size, in points, that a text's base size stands for: a span of [`SpanStyle::size`] `s`
/// is drawn `s / BASE_POINTS` times as large as the base. ChemDraw's captions and labels are
/// 10 pt, and the editor's label size is taken as that.
pub const BASE_POINTS: f32 = 10.0;

/// Size of sub/superscript glyphs relative to the base font size.
pub const SCRIPT_SCALE: f32 = 0.7;
/// How far (in fractions of the base font size) the superscript baseline is raised.
pub const SUPERSCRIPT_RISE: f32 = 0.38;
/// How far (in fractions of the base font size) the subscript baseline is lowered.
pub const SUBSCRIPT_DROP: f32 = 0.16;
/// Horizontal offset (in fractions of the base font size) of the faux-bold double-strike.
const BOLD_STRIKE: f32 = 0.04;

/// Vertical placement of a span relative to the main baseline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Script {
    #[default]
    Normal,
    /// Raised, smaller (charges: `Na⁺`, `SO₄²⁻`).
    Super,
    /// Lowered, smaller (formula counts: `H₂O`, `CH₃`).
    Sub,
}

/// Style of one contiguous run of text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SpanStyle {
    pub bold: bool,
    pub italic: bool,
    pub script: Script,
    /// `None` inherits the color given at paint time.
    #[serde(default, with = "color_rgba")]
    pub color: Option<Color32>,
    /// The font family; `None` is the text's base font. See the [module docs](self#fonts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<FontName>,
    /// The size in points ([`BASE_POINTS`] is the base size); `None` is the base size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
}

impl SpanStyle {
    /// Whether the span is set in ChemDraw's Symbol font (its ASCII shows as Greek).
    pub fn is_symbol(&self) -> bool {
        self.font.is_some_and(|f| f.is_symbol())
    }
}

/// (De)serialize an optional [`Color32`] as `[r, g, b, a]` (straight alpha), so styled text
/// can be saved without depending on egui's own serde support.
mod color_rgba {
    use egui::Color32;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(color: &Option<Color32>, s: S) -> Result<S::Ok, S::Error> {
        color.map(|c| c.to_srgba_unmultiplied()).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Color32>, D::Error> {
        let rgba = Option::<[u8; 4]>::deserialize(d)?;
        Ok(rgba.map(|[r, g, b, a]| Color32::from_rgba_unmultiplied(r, g, b, a)))
    }
}

/// A run of text with a single [`SpanStyle`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RichSpan {
    pub text: String,
    pub style: SpanStyle,
}

/// Styled text as a list of spans. See the [module docs](self) for the markup syntax.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
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
        Self::parse_mapped(markup).0
    }

    /// [`parse`](Self::parse), and for every character of the parsed text (in the order of
    /// [`chars`](Self::chars)) the index, in chars, of the markup character it came from. An
    /// editor uses it to turn a cursor or selection in the markup into one in the text.
    pub fn parse_mapped(markup: &str) -> (Self, Vec<usize>) {
        /// An open braced group: a script, or a font and/or size.
        enum Frame {
            Script(Script),
            Font(Option<FontName>, Option<f32>),
        }
        let mut out = Self::new();
        let mut map: Vec<usize> = Vec::new();
        let mut cur = String::new();
        let mut bold = false;
        let mut italic = false;
        let mut frames: Vec<Frame> = Vec::new();

        let style = |bold: bool, italic: bool, script: Option<Script>, frames: &[Frame]| {
            let script = script
                .or_else(|| frames.iter().rev().find_map(|f| if let Frame::Script(s) = f { Some(*s) } else { None }))
                .unwrap_or_default();
            let font = frames.iter().rev().find_map(|f| if let Frame::Font(Some(n), _) = f { Some(*n) } else { None });
            let size = frames.iter().rev().find_map(|f| if let Frame::Font(_, s) = f { *s } else { None });
            SpanStyle { bold, italic, script, color: None, font, size }
        };
        // Flush the accumulated literal run under the *current* style.
        macro_rules! flush {
            () => {
                if !cur.is_empty() {
                    out = out.span(std::mem::take(&mut cur), style(bold, italic, None, &frames));
                }
            };
        }

        let chars: Vec<char> = markup.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();
            match c {
                '\\' => {
                    match next {
                        Some(n) => {
                            cur.push(n);
                            map.push(i + 1);
                            i += 1;
                        }
                        None => {
                            cur.push('\\'); // trailing backslash stays literal
                            map.push(i);
                        }
                    }
                }
                '*' if next == Some('*') => {
                    i += 1;
                    flush!();
                    bold = !bold;
                }
                '/' if next == Some('/') => {
                    i += 1;
                    flush!();
                    italic = !italic;
                }
                '^' | '_' => {
                    let script = if c == '^' { Script::Super } else { Script::Sub };
                    match next {
                        Some('{') => {
                            i += 1;
                            flush!();
                            frames.push(Frame::Script(script));
                        }
                        // One-character shortcut: `_2`, `^+`, `_\{` (escaped char).
                        Some(n) if !n.is_whitespace() && n != '}' => {
                            let (ch, at) = match (n, chars.get(i + 2)) {
                                ('\\', Some(&e)) => (e, i + 2),
                                _ => (n, i + 1),
                            };
                            flush!();
                            out = out.span(ch.to_string(), style(bold, italic, Some(script), &frames));
                            map.push(at);
                            i = at;
                        }
                        // Nothing it can apply to: keep the marker literal.
                        _ => {
                            cur.push(c);
                            map.push(i);
                        }
                    }
                }
                '{' if next == Some('@') => match parse_font_spec(&chars, i + 2) {
                    Some((font, size, end)) => {
                        flush!();
                        frames.push(Frame::Font(font, size));
                        i = end;
                    }
                    None => {
                        cur.push(c);
                        map.push(i);
                    }
                },
                '}' if !frames.is_empty() => {
                    flush!();
                    frames.pop();
                }
                other => {
                    cur.push(other);
                    map.push(i);
                }
            }
            i += 1;
        }
        flush!();
        (out, map)
    }

    /// Every character of the text, span after span.
    pub fn chars(&self) -> impl Iterator<Item = char> + '_ {
        self.spans.iter().flat_map(|s| s.text.chars())
    }

    /// The number of characters of the text.
    pub fn char_count(&self) -> usize {
        self.spans.iter().map(|s| s.text.chars().count()).sum()
    }

    /// The style of every character in `range` (character indices of [`chars`](Self::chars)).
    pub fn styles_in(&self, range: std::ops::Range<usize>) -> Vec<SpanStyle> {
        let mut out = Vec::new();
        let mut at = 0;
        for span in &self.spans {
            for _ in span.text.chars() {
                if range.contains(&at) {
                    out.push(span.style);
                }
                at += 1;
            }
        }
        out
    }

    /// The text with the characters in `range` rewritten by `f`, which is given each run of
    /// them that shares a style and may change both the run's text and its style. Everything
    /// outside `range` stays as it was.
    pub fn map_range(&self, range: std::ops::Range<usize>, mut f: impl FnMut(&str, &mut SpanStyle) -> String) -> Self {
        let mut out = Self::new();
        let mut at = 0;
        for span in &self.spans {
            let n = span.text.chars().count();
            let (lo, hi) = (range.start.clamp(at, at + n) - at, range.end.clamp(at, at + n) - at);
            let part = |a: usize, b: usize| span.text.chars().skip(a).take(b - a).collect::<String>();
            out = out.span(part(0, lo), span.style);
            if hi > lo {
                let mut style = span.style;
                let text = f(&part(lo, hi), &mut style);
                out = out.span(text, style);
            }
            out = out.span(part(hi.max(lo), n), span.style);
            at += n;
        }
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
        self.write_markup(false)
    }

    /// Markup as someone would type it: like [`to_markup`](Self::to_markup), but a
    /// one-character script goes without braces (`CO_2Et`, `NH_4^+`).
    pub fn to_compact_markup(&self) -> String {
        self.write_markup(true)
    }

    fn write_markup(&self, compact: bool) -> String {
        let mut out = String::new();
        let (mut bold, mut italic) = (false, false);
        let reserved = |c: char| matches!(c, '\\' | '*' | '/' | '^' | '_' | '{' | '}');
        // The font group open around the current span, if any.
        let mut group: Option<(Option<FontName>, Option<f32>)> = None;
        for span in self.spans.iter().filter(|s| !s.text.is_empty()) {
            let style = &span.style;
            let want = (style.font.is_some() || style.size.is_some()).then_some((style.font, style.size));
            if want != group {
                if group.is_some() {
                    out.push('}');
                }
                if let Some((font, size)) = want {
                    push_font_spec(&mut out, font, size);
                }
                group = want;
            }
            if style.bold != bold {
                out.push_str("**");
                bold = style.bold;
            }
            if style.italic != italic {
                out.push_str("//");
                italic = style.italic;
            }
            let escaped: String =
                span.text.chars().flat_map(|c| if reserved(c) { vec!['\\', c] } else { vec![c] }).collect();
            let marker = match style.script {
                Script::Normal => None,
                Script::Sub => Some('_'),
                Script::Super => Some('^'),
            };
            let mut chars = span.text.chars();
            match (marker, chars.next(), chars.next()) {
                (None, _, _) => out.push_str(&escaped),
                (Some(m), Some(c), None) if compact && !c.is_whitespace() => {
                    out.push(m);
                    out.push_str(&escaped);
                }
                (Some(m), _, _) => {
                    out.push(m);
                    out.push('{');
                    out.push_str(&escaped);
                    out.push('}');
                }
            }
        }
        // Close whatever is still open, so the string stands on its own.
        if group.is_some() {
            out.push('}');
        }
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
            size: f32,
            italic: bool,
            font: Option<FontName>,
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
            // A span's own size (in points) scales the base size.
            let base = span.style.size.map_or(style.size, |pt| style.size * pt / BASE_POINTS);
            let (scale, shift) = match span.style.script {
                Script::Normal => (1.0, 0.0),
                Script::Super => (SCRIPT_SCALE, -SUPERSCRIPT_RISE * base),
                Script::Sub => (SCRIPT_SCALE, SUBSCRIPT_DROP * base),
            };
            let family = span.style.font.map_or_else(|| style.family.clone(), |f| f.family(&style.family));
            let format = TextFormat {
                font_id: FontId::new(base * scale, family),
                color: span.style.color.unwrap_or(style.color),
                italics: span.style.italic,
                ..Default::default()
            };
            let mut job = LayoutJob::default();
            job.append(&fonts::display_text(span.style.font, &span.text), 0.0, format);
            let galley = layout(job);

            let baseline = galley_baseline(&galley);
            let bold_offset = span.style.bold.then(|| (base * BOLD_STRIKE).max(0.3));
            let advance = galley.rect.width() + bold_offset.unwrap_or(0.0);
            placed.push(Pending {
                galley,
                x: pen_x,
                baseline,
                shift,
                bold_offset,
                size: base * scale,
                italic: span.style.italic,
                font: span.style.font.filter(|f| !f.is_symbol()),
            });
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
                size: p.size,
                italic: p.italic,
                font: p.font,
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

/// The font spec of a `{@Font,size:` group whose spec starts at `chars[start]`: the font, the
/// size, and the index of the `:` ending it. `None` when it is not one (no `:` before a brace
/// or a line break, or nothing in it).
fn parse_font_spec(chars: &[char], start: usize) -> Option<(Option<FontName>, Option<f32>, usize)> {
    let mut name = String::new();
    let mut size: Option<String> = None;
    let mut i = start;
    loop {
        let c = *chars.get(i)?;
        match c {
            '\\' => {
                let e = *chars.get(i + 1)?;
                size.as_mut().unwrap_or(&mut name).push(e);
                i += 1;
            }
            ':' => break,
            ',' if size.is_none() => size = Some(String::new()),
            '{' | '}' | '\n' => return None,
            c => size.as_mut().unwrap_or(&mut name).push(c),
        }
        i += 1;
    }
    let size = match size {
        // A size that is no number belongs to the name ("Font, Inc").
        Some(s) => match s.trim().parse::<f32>() {
            Ok(v) if v.is_finite() && v > 0.0 => Some(v),
            _ => {
                name.push(',');
                name.push_str(&s);
                None
            }
        },
        None => None,
    };
    let font = (!name.trim().is_empty()).then(|| FontName::new(&name));
    (font.is_some() || size.is_some()).then_some((font, size, i))
}

/// Escape the characters a font group's spec reserves.
fn push_font_spec(out: &mut String, font: Option<FontName>, size: Option<f32>) {
    out.push_str("{@");
    if let Some(font) = font {
        for c in font.as_str().chars() {
            if matches!(c, '\\' | ':' | ',' | '{' | '}') {
                out.push('\\');
            }
            out.push(c);
        }
    }
    if let Some(size) = size {
        out.push_str(&format!(",{size}"));
    }
    out.push(':');
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
    /// Font size this span was laid out at (base size, or `size * SCRIPT_SCALE` for scripts).
    /// Backends that re-emit the text (e.g. SVG export) need it to match the glyph size.
    pub size: f32,
    /// Whether this span is italic (shape differs; SVG export needs it).
    pub italic: bool,
    /// The named font the span is drawn in, `None` for the base font. Symbol text is drawn
    /// as Unicode in the base font, so it has none here.
    pub font: Option<FontName>,
}

impl RichGalleyPart {
    /// From the part's top to its baseline.
    pub fn baseline(&self) -> f32 {
        galley_baseline(&self.galley)
    }
}

/// One glyph of a laid-out [`RichGalley`], positioned so a non-egui backend (the SVG image
/// export) can place the exact same character at the exact same spot egui would. `x`/`baseline_y`
/// are relative to the galley's top-left; `baseline_y` is the text baseline (SVG's default text
/// anchor point), so the two renderers share one label geometry.
pub struct PlacedGlyph {
    pub chr: char,
    pub x: f32,
    pub baseline_y: f32,
    pub size: f32,
    pub italic: bool,
    /// Faux-bold double-strike offset (paint the glyph twice, second shifted by this).
    pub bold_offset: Option<f32>,
    /// The named font of the glyph's span ([`RichGalleyPart::font`]).
    pub font: Option<FontName>,
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

    /// Distance from the top of the laid-out text to its main (normal-size) baseline; 0 for
    /// empty text. Lets a caller put the baseline where it wants it, as a page text does.
    pub fn baseline(&self) -> f32 {
        // Normal-size spans sit on the main baseline, so the largest part's own glyph
        // baseline gives it away.
        self.parts
            .iter()
            .max_by(|a, b| a.size.total_cmp(&b.size))
            .map_or(0.0, |p| p.offset.y + galley_baseline(&p.galley))
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

    /// Every glyph of the laid-out text, positioned (relative to the galley's top-left) so a
    /// non-egui backend can reproduce egui's exact layout. `baseline_y` uses egui's own glyph
    /// baseline (`row.pos.y + glyph.pos.y`), which is what SVG's `<text>` anchors to.
    pub fn glyph_placements(&self) -> Vec<PlacedGlyph> {
        let mut out = Vec::new();
        for part in &self.parts {
            for row in &part.galley.rows {
                for glyph in &row.row.glyphs {
                    out.push(PlacedGlyph {
                        chr: glyph.chr,
                        x: part.offset.x + row.pos.x + glyph.pos.x,
                        baseline_y: part.offset.y + row.pos.y + glyph.pos.y,
                        size: part.size,
                        italic: part.italic,
                        bold_offset: part.bold_offset,
                        font: part.font,
                    });
                }
            }
        }
        out
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

    fn font(name: &str) -> SpanStyle {
        SpanStyle { font: Some(FontName::new(name)), ..Default::default() }
    }

    #[test]
    fn parses_font_groups() {
        assert_eq!(spans("{@Symbol:p}-stacking"), vec![("p".into(), font("Symbol")), ("-stacking".into(), normal())]);
        // Font and size; size alone; nested groups and scripts inside.
        assert_eq!(
            spans("{@Times New Roman,12:a}"),
            vec![("a".into(), SpanStyle { size: Some(12.0), ..font("Times New Roman") })]
        );
        assert_eq!(spans("{@,14:big}"), vec![("big".into(), SpanStyle { size: Some(14.0), ..normal() })]);
        assert_eq!(
            spans("{@Arial,8:x{@Symbol:a_2}y}"),
            vec![
                ("x".into(), SpanStyle { size: Some(8.0), ..font("Arial") }),
                ("a".into(), SpanStyle { size: Some(8.0), ..font("Symbol") }),
                ("2".into(), SpanStyle { size: Some(8.0), script: Script::Sub, ..font("Symbol") }),
                ("y".into(), SpanStyle { size: Some(8.0), ..font("Arial") }),
            ]
        );
        // Escapes in the name; a size that is no number is part of the name.
        assert_eq!(spans(r"{@A\:B:x}"), vec![("x".into(), font("A:B"))]);
        assert_eq!(spans("{@Font, Inc:x}"), vec![("x".into(), font("Font, Inc"))]);
        // Not a group: no colon, nothing named, or a brace in the spec.
        assert_eq!(spans("{@abc}"), vec![("{@abc}".into(), normal())]);
        assert_eq!(spans("{@:x}"), vec![("{@:x}".into(), normal())]);
        assert_eq!(spans("a{b}"), vec![("a{b}".into(), normal())]);
    }

    #[test]
    fn font_groups_round_trip_through_markup() {
        for markup in [
            "{@Symbol:p}-stacking",
            "{@Times New Roman,12:**a**_2}b",
            "x{@Symbol:a}{@Arial:b}",
            "{@,14://big//}",
            r"{@A\:B\,C:x}",
        ] {
            let parsed = RichText::parse(markup);
            for out in [parsed.to_markup(), parsed.to_compact_markup()] {
                assert_eq!(RichText::parse(&out).spans, parsed.spans, "'{markup}' → '{out}'");
            }
        }
        assert_eq!(RichText::parse("{@Symbol:a_2b}").to_compact_markup(), "{@Symbol:a_2b}");
        assert_eq!(RichText::parse("{@Symbol:a_2b}").to_markup(), "{@Symbol:a_{2}b}");
    }

    #[test]
    fn parse_mapped_points_each_char_at_its_markup() {
        let (rt, map) = RichText::parse_mapped("C_2{@Symbol:p}\\_");
        assert_eq!(rt.chars().collect::<String>(), "C2p_");
        assert_eq!(map, vec![0, 2, 12, 15]);
    }

    #[test]
    fn map_range_restyles_part_of_a_span() {
        let rt = RichText::parse("abcd");
        let out = rt.map_range(1..3, |text, style| {
            style.font = Some(FontName::symbol());
            text.to_string()
        });
        assert_eq!(out.to_markup(), "a{@Symbol:bc}d");
        assert_eq!(out.styles_in(0..4).iter().filter(|s| s.is_symbol()).count(), 2);
        assert_eq!(out.char_count(), 4);
    }

    #[test]
    fn symbol_spans_lay_out_as_greek() {
        let style = RichTextStyle::new(20.0, Color32::BLACK);
        let g = layout(&RichText::parse("{@Symbol:p}x"), &style);
        let chars: String = g.glyph_placements().iter().map(|g| g.chr).collect();
        assert_eq!(chars, "πx");
        assert!(g.parts.iter().all(|p| p.font.is_none()), "Symbol is drawn in the base font");
        // A span's point size scales it against the base 10 pt.
        let big = layout(&RichText::parse("{@,20:M}"), &style);
        let normal = layout(&RichText::parse("M"), &style);
        assert!((big.parts[0].size - 2.0 * normal.parts[0].size).abs() < 1e-3);
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
        let output = crate::testkit::run_ui(&ctx, raw, |ui| {
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
