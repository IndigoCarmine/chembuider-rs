//! Laying several [`RichText`] runs of *different sizes* on one baseline.
//!
//! [`RichText`] gives sub/superscripts, bold and italic within a single base font size.
//! The analysis table needs one thing more: a unit set in a smaller face than the number it
//! follows (`1.00`**g**, `10.63`**mmol**) — and it has to sit on the *same baseline*, not be
//! bottom- or centre-aligned, or the descender of `g` would push it out of line.
//!
//! [`layout_runs`] lays each run out on its own and then shifts them onto a common baseline,
//! the same two-pass trick [`RichText::layout_with`] uses for scripts. It also applies
//! letter tracking, which is how the italic block titles (*R e a c t a n t s*) get their
//! spacing without inserting fake space characters into the text.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Align2, Pos2, Rect, Vec2};

use crate::widget::rich_text::{RichGalley, RichText, RichTextStyle, SpanStyle};

/// One run of a [`RunLayout`]: text plus its size relative to the layout's base size.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub text: RichText,
    /// Multiplier on [`RichTextStyle::size`]. `1.0` for body text, ~`0.75` for units.
    pub scale: f32,
}

impl Run {
    pub fn new(text: impl Into<RichText>) -> Self {
        Self { text: text.into(), scale: 1.0 }
    }

    pub fn scaled(text: impl Into<RichText>, scale: f32) -> Self {
        Self { text: text.into(), scale }
    }
}

/// Baseline of a laid-out galley, measured down from its top. epaint stores glyph positions
/// relative to the baseline, so any glyph gives it away; a galley without glyphs falls back
/// to a typographic estimate. (Mirrors the private helper in `widget::rich_text`.)
pub fn galley_baseline(galley: &egui::Galley) -> f32 {
    galley
        .rows
        .first()
        .and_then(|row| Some(row.pos.y + row.row.glyphs.first()?.pos.y))
        .unwrap_or(galley.rect.height() * 0.8)
}

/// Main baseline of a [`RichGalley`], measured down from its top.
///
/// Sub- and superscript parts sit off the main baseline by design, so the reference is the
/// *tallest* part — scripts are laid out at [`SCRIPT_SCALE`](crate::widget::rich_text::SCRIPT_SCALE)
/// of the base size and can never be the tallest one in a run that has normal text.
pub fn rich_baseline(rich: &RichGalley) -> f32 {
    rich.parts
        .iter()
        .max_by(|a, b| {
            a.galley
                .rect
                .height()
                .partial_cmp(&b.galley.rect.height())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|p| p.offset.y + galley_baseline(&p.galley))
        .unwrap_or(0.0)
}

/// Several runs laid out left to right on a shared baseline.
pub struct RunLayout {
    /// Each run's galley and the offset of its top-left from the layout's top-left.
    pub parts: Vec<(RichGalley, Vec2)>,
    /// Tight size of everything together.
    pub size: Vec2,
    /// Shared baseline, measured down from the layout's top.
    pub baseline: f32,
}

impl RunLayout {
    pub const EMPTY: Self = Self { parts: Vec::new(), size: Vec2::ZERO, baseline: 0.0 };

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    pub fn size(&self) -> Vec2 {
        self.size
    }

    /// Paint with the layout's top-left corner at `top_left`.
    pub fn paint(&self, painter: &egui::Painter, top_left: Pos2) {
        for (galley, offset) in &self.parts {
            galley.paint(painter, top_left + *offset);
        }
    }

    /// Paint anchored like [`egui::Painter::text`]; returns the rect covered.
    pub fn paint_anchored(&self, painter: &egui::Painter, anchor: Pos2, align: Align2) -> Rect {
        let rect = align.anchor_size(anchor, self.size);
        self.paint(painter, rect.min);
        rect
    }

    /// Flattened text of every run, in order — the readable oracle for tests.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .flat_map(|(g, _)| g.parts.iter())
            .map(|p| p.galley.job.text.clone())
            .collect()
    }
}

/// Lay `runs` out left to right, aligning their baselines and inserting `tracking` pixels
/// after every run but the last.
///
/// `style` gives the *base* size/colour/family; each run's own [`Run::scale`] multiplies the
/// size. `layout` is any galley source (an [`egui::Painter`], a `FontsView`, …).
pub fn layout_runs(
    runs: &[Run],
    style: &RichTextStyle,
    tracking: f32,
    mut layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> RunLayout {
    let mut laid: Vec<(RichGalley, f32)> = Vec::with_capacity(runs.len());
    for run in runs {
        if run.text.is_empty() {
            continue;
        }
        let run_style = RichTextStyle {
            size: style.size * run.scale,
            color: style.color,
            family: style.family.clone(),
        };
        let galley = run.text.layout_with(&run_style, &mut layout);
        let baseline = rich_baseline(&galley);
        laid.push((galley, baseline));
    }
    if laid.is_empty() {
        return RunLayout::EMPTY;
    }

    // Common baseline: deep enough that no run's own baseline sits below the top.
    let baseline = laid.iter().map(|(_, b)| *b).fold(f32::MIN, f32::max);
    let mut parts = Vec::with_capacity(laid.len());
    let mut pen_x = 0.0_f32;
    let mut height = 0.0_f32;
    let last = laid.len() - 1;
    for (i, (galley, own_baseline)) in laid.into_iter().enumerate() {
        let top = baseline - own_baseline;
        let advance = galley.size.x + if i == last { 0.0 } else { tracking };
        height = height.max(top + galley.size.y);
        parts.push((galley, Vec2::new(pen_x, top)));
        pen_x += advance;
    }
    RunLayout { parts, size: Vec2::new(pen_x, height), baseline }
}

/// Convenience: one run, no tracking.
pub fn layout_text(
    text: &RichText,
    style: &RichTextStyle,
    layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> RunLayout {
    let runs = [Run::new(text.clone())];
    layout_runs(&runs, style, 0.0, layout)
}

/// A number followed by a smaller unit: `1.00` + `g`. An empty unit yields a single run.
///
/// The unit inherits the value's weight and slant — an entered value reads *`700.00mg`* as
/// one styled quantity — and differs only in size.
pub fn value_runs(value: RichText, unit: &str, unit_scale: f32) -> Vec<Run> {
    if unit.is_empty() {
        return vec![Run::new(value)];
    }
    let style = value
        .spans
        .first()
        .map(|s| SpanStyle { script: crate::widget::rich_text::Script::Normal, ..s.style })
        .unwrap_or_default();
    let unit = RichText::new().span(unit.to_string(), style);
    vec![Run::new(value), Run::scaled(unit, unit_scale)]
}

/// One run per character, so [`layout_runs`]'s tracking can space them out — the
/// *R e a c t a n t s* treatment. The text itself stays intact (no injected spaces).
pub fn tracked_runs(text: &str, style: SpanStyle) -> Vec<Run> {
    text.chars()
        .map(|c| Run::new(RichText::new().span(c.to_string(), style)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Color32;

    fn fonts() -> egui::epaint::text::Fonts {
        egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            egui::FontDefinitions::default(),
        )
    }

    fn lay(runs: &[Run], size: f32, tracking: f32) -> RunLayout {
        let mut fonts = fonts();
        let mut view = fonts.with_pixels_per_point(1.0);
        let style = RichTextStyle::new(size, Color32::BLACK);
        layout_runs(runs, &style, tracking, |job| view.layout_job(job))
    }

    /// A smaller unit run sits on the *same* baseline as the number, not on the same top
    /// or bottom edge — the whole point of this module.
    #[test]
    fn unit_run_shares_the_number_baseline() {
        let runs = value_runs(RichText::plain("1.00"), "g", 0.75);
        let layout = lay(&runs, 16.0, 0.0);
        assert_eq!(layout.parts.len(), 2);

        let (number, n_off) = &layout.parts[0];
        let (unit, u_off) = &layout.parts[1];

        // Same baseline, in absolute layout coordinates.
        let n_base = n_off.y + rich_baseline(number);
        let u_base = u_off.y + rich_baseline(unit);
        assert!(
            (n_base - u_base).abs() < 0.01,
            "baselines differ: {n_base} vs {u_base}"
        );

        // The unit really is smaller, and starts lower down than the number.
        assert!(unit.size.y < number.size.y, "unit must use a smaller face");
        assert!(u_off.y > n_off.y, "smaller text starts lower when baseline-aligned");

        // Runs advance left to right without overlapping, and the layout is tight.
        assert!(u_off.x >= n_off.x + number.size.x - 0.01);
        assert!((layout.size.x - (number.size.x + unit.size.x)).abs() < 0.01);
        assert_eq!(layout.text(), "1.00g");
    }

    /// A descender in the unit (`g`) grows the box downward but must not move the baseline.
    #[test]
    fn descenders_do_not_shift_the_baseline() {
        let flat = lay(&value_runs(RichText::plain("1.00"), "L", 0.75), 16.0, 0.0);
        let deep = lay(&value_runs(RichText::plain("1.00"), "g", 0.75), 16.0, 0.0);
        assert!((flat.baseline - deep.baseline).abs() < 0.01);
        assert!(deep.size.y >= flat.size.y);
    }

    /// Tracking widens the run sequence by exactly (n − 1) × tracking, and the text stays
    /// free of injected spaces.
    #[test]
    fn tracking_spaces_letters_without_touching_the_text() {
        let runs = tracked_runs("Reactants", SpanStyle { italic: true, ..Default::default() });
        assert_eq!(runs.len(), 9);
        let tight = lay(&runs, 13.0, 0.0);
        let spaced = lay(&runs, 13.0, 4.0);
        assert_eq!(spaced.text(), "Reactants", "no fake spaces in the text");
        assert!(
            (spaced.size.x - tight.size.x - 8.0 * 4.0).abs() < 0.01,
            "9 letters → 8 gaps: {} vs {}",
            spaced.size.x,
            tight.size.x
        );
        // Every character is italic.
        assert!(runs.iter().all(|r| r.text.spans[0].style.italic));
    }

    /// Subscripts inside a run don't confuse the baseline: the formula and the plain text
    /// next to it still line up.
    #[test]
    fn subscripts_keep_the_main_baseline() {
        let formula = RichText::new().text("C").subscript("6").text("H");
        let layout = lay(&[Run::new(formula), Run::new(RichText::plain("X"))], 16.0, 0.0);
        let (a, a_off) = &layout.parts[0];
        let (b, b_off) = &layout.parts[1];
        let a_base = a_off.y + rich_baseline(a);
        let b_base = b_off.y + rich_baseline(b);
        assert!((a_base - b_base).abs() < 0.01, "{a_base} vs {b_base}");
        // The subscript hangs below the shared baseline.
        assert!(layout.size.y > b.size.y);
    }

    /// The unit follows the value's style — an entered value's unit is bold italic too —
    /// but always at the smaller size.
    #[test]
    fn units_inherit_the_value_style() {
        let entered = SpanStyle { bold: true, italic: true, ..Default::default() };
        let runs = value_runs(RichText::new().span("700.00", entered), "mg", 0.75);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[1].text.spans[0].text, "mg");
        assert!(runs[1].text.spans[0].style.bold && runs[1].text.spans[0].style.italic);
        assert_eq!(runs[1].scale, 0.75);

        // A computed value keeps a regular unit.
        let runs = value_runs(RichText::plain("1.66"), "g", 0.75);
        assert!(!runs[1].text.spans[0].style.bold);

        // No unit, no second run.
        assert_eq!(value_runs(RichText::plain("94.11"), "", 0.75).len(), 1);
    }

    #[test]
    fn empty_runs_lay_out_to_nothing() {
        let layout = lay(&[Run::new(RichText::new())], 16.0, 0.0);
        assert!(layout.is_empty());
        assert_eq!(layout.size, Vec2::ZERO);
    }
}
