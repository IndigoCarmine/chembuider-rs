//! Formatting page text: fonts, sizes, bold/italic, sub/superscripts and ChemDraw's Symbol font,
//! for a host's format bar and the text box's own shortcut (Ctrl+G toggles Symbol).
//!
//! A change ([`FormatChange`]) goes to what the user is working on, in this order:
//! - the text box, when it is open: the characters selected in it, or the whole text when
//!   nothing is selected. With nothing selected, toggling Symbol instead switches *Symbol
//!   typing*: what is typed from then on is set in Symbol (typing `p` gives π);
//! - otherwise every selected page text, as a whole, in one undo step.
//!
//! Every font, size, bold and italic change also becomes the style new texts are given
//! ([`ChemStructEditor::new_text_style`]), as a word processor's current format does.
//!
//! The text box holds markup, so formatting rewrites it (see [`RichText::to_compact_markup`]),
//! line by line so no group spans a line break, and puts the selection back over the same
//! characters.

use std::ops::Range;

use super::fonts::{self, FontName};
use super::rich_text::{RichText, Script, SpanStyle};
use super::ChemStructEditor;
use crate::molecule::PageObject;

/// The format of the text the user is working on: the first selected character's style (or the
/// one before the cursor), or the style of new text when nothing is being edited.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextFormat {
    pub style: SpanStyle,
    /// Whether Symbol typing is on in the text box.
    pub symbol_typing: bool,
    /// What a change would apply to.
    pub target: FormatTarget,
}

/// What [`ChemStructEditor::apply_text_format`] would change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FormatTarget {
    /// Only the style of new texts.
    #[default]
    NewText,
    /// The text box's selection (or its whole text when nothing is selected).
    Editing { selection: bool },
    /// The selected page texts (this many).
    Selected(usize),
}

/// One formatting change.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FormatChange {
    /// Set in this font (`None`: the base font).
    Font(Option<FontName>),
    /// At this size in points (`None`: the base size).
    Size(Option<f32>),
    Bold(bool),
    Italic(bool),
    Script(Script),
    /// In Symbol (true) or back in the base font (false); with nothing selected in the text box,
    /// Symbol typing on or off.
    Symbol(bool),
}

/// The characters `text` holds once its font goes from `from` to `to`, keeping how they look:
/// Greek typed as Unicode becomes Symbol's ASCII going into Symbol, and Symbol's ASCII becomes
/// the Greek it showed coming out of it.
fn convert_font_text(text: &str, from: Option<FontName>, to: Option<FontName>) -> String {
    let (was, is) = (from.is_some_and(|f| f.is_symbol()), to.is_some_and(|f| f.is_symbol()));
    match (was, is) {
        (false, true) => text.chars().map(|c| fonts::unicode_to_symbol(c).unwrap_or(c)).collect(),
        (true, false) => text.chars().map(fonts::symbol_to_unicode).collect(),
        _ => text.to_owned(),
    }
}

/// `rich` with `change` applied to the characters in `range`. A font change leaves Symbol
/// text in Symbol (as ChemDraw keeps Greek Greek when a caption's font changes); only
/// [`FormatChange::Symbol`] or choosing Symbol itself moves text in or out of it.
pub fn apply_to(rich: &RichText, range: Range<usize>, change: FormatChange) -> RichText {
    rich.map_range(range, |text, style| {
        let font = match change {
            FormatChange::Font(f) if style.is_symbol() && !f.is_some_and(|f| f.is_symbol()) => None,
            FormatChange::Font(f) => Some(f),
            FormatChange::Symbol(true) => Some(Some(FontName::symbol())),
            FormatChange::Symbol(false) if style.is_symbol() => Some(None),
            _ => None,
        };
        match change {
            FormatChange::Size(s) => style.size = s,
            FormatChange::Bold(b) => style.bold = b,
            FormatChange::Italic(i) => style.italic = i,
            FormatChange::Script(s) => style.script = s,
            FormatChange::Font(_) | FormatChange::Symbol(_) => {}
        }
        match font {
            Some(to) => {
                let out = convert_font_text(text, style.font, to);
                style.font = to;
                out
            }
            None => text.to_owned(),
        }
    })
}

/// Markup for rich text that may hold line breaks: one line at a time, so no group spans one.
pub fn markup_of(rich: &RichText) -> String {
    let mut lines = vec![RichText::new()];
    for span in &rich.spans {
        for (k, part) in span.text.split('\n').enumerate() {
            if k > 0 {
                lines.push(RichText::new());
            }
            let line = lines.last_mut().expect("one line at least");
            *line = std::mem::take(line).span(part, span.style);
        }
    }
    lines.iter().map(RichText::to_compact_markup).collect::<Vec<_>>().join("\n")
}

/// The characters of `markup`'s text a markup range covers (those whose markup lies in it).
pub fn text_range(map: &[usize], markup: Range<usize>) -> Range<usize> {
    let count = |at: usize| map.iter().filter(|&&m| m < at).count();
    count(markup.start)..count(markup.end)
}

/// The markup range that covers the text characters `range` (`map` from
/// [`RichText::parse_mapped`] of the markup, `len` its length in chars).
pub fn markup_range(map: &[usize], len: usize, range: Range<usize>) -> Range<usize> {
    let at = |k: usize| map.get(k).copied().unwrap_or(len);
    if range.is_empty() {
        let p = if range.start == 0 { at(0).min(len) } else { map.get(range.start - 1).map_or(len, |m| m + 1) };
        return p..p;
    }
    at(range.start)..map.get(range.end - 1).map_or(len, |m| m + 1)
}

/// `style` over `base`: each of `style`'s font, size, bold and italic that is not set comes
/// from `base`. What new text is given.
fn fill_style(style: &mut SpanStyle, base: &SpanStyle) {
    if style.font.is_none() {
        style.font = base.font;
    }
    if style.size.is_none() {
        style.size = base.size;
    }
    style.bold |= base.bold;
    style.italic |= base.italic;
}

impl ChemStructEditor {
    /// The format of what the user is working on; see [`TextFormat`].
    pub fn text_format(&self) -> TextFormat {
        if let Some(edit) = &self.text_edit {
            let (rich, map) = RichText::parse_mapped(&edit.text);
            let sel = edit.selection.clone().map(|s| text_range(&map, s));
            let selection = sel.as_ref().is_some_and(|s| !s.is_empty());
            let at = match &sel {
                Some(s) if !s.is_empty() => s.start..s.start + 1,
                Some(s) if s.start > 0 => s.start - 1..s.start,
                _ => 0..1,
            };
            let mut style = rich.styles_in(at).first().copied().unwrap_or_default();
            if edit.id.is_none() {
                fill_style(&mut style, &self.new_text_style);
            }
            return TextFormat { style, symbol_typing: edit.symbol, target: FormatTarget::Editing { selection } };
        }
        let selected = self.selected_texts();
        if let Some(first) = selected.first().and_then(|id| self.molecule.object_by_id(*id)).and_then(|o| o.as_text()) {
            let style = first.lines().iter().find_map(|l| l.spans.first().map(|s| s.style)).unwrap_or_default();
            return TextFormat { style, symbol_typing: false, target: FormatTarget::Selected(selected.len()) };
        }
        TextFormat { style: self.new_text_style, ..Default::default() }
    }

    /// The selected page texts, by id, in page order.
    fn selected_texts(&self) -> Vec<u32> {
        self.molecule
            .objects
            .iter()
            .filter(|o| o.as_text().is_some() && self.selected_objects.contains(&o.id()))
            .map(|o| o.id())
            .collect()
    }

    /// Apply `change` to what the user is working on (see the [module docs](self)). Returns
    /// whether the molecule changed (the text box's text is not the molecule until committed).
    pub fn apply_text_format(&mut self, change: FormatChange) -> bool {
        match change {
            FormatChange::Font(f) => self.new_text_style.font = f.filter(|f| !f.is_symbol()),
            FormatChange::Size(s) => self.new_text_style.size = s,
            FormatChange::Bold(b) => self.new_text_style.bold = b,
            FormatChange::Italic(i) => self.new_text_style.italic = i,
            FormatChange::Script(_) | FormatChange::Symbol(_) => {}
        }
        if let Some(edit) = self.text_edit.as_mut() {
            let (rich, map) = RichText::parse_mapped(&edit.text);
            let sel = edit.selection.clone().map(|s| text_range(&map, s)).filter(|s| !s.is_empty());
            let range = match (&sel, change) {
                (None, FormatChange::Symbol(on)) => {
                    edit.symbol = on;
                    // Typing on after Symbol text that was just typed would go into its group:
                    // step the cursor out of the groups closing there.
                    if let (false, Some(cursor)) = (on, edit.selection.as_ref().map(|s| s.end)) {
                        let chars: Vec<char> = edit.text.chars().collect();
                        let k = text_range(&map, 0..cursor).end;
                        if k > 0 && rich.styles_in(k - 1..k).first().is_some_and(|s| s.is_symbol()) {
                            let mut p = cursor;
                            while chars.get(p) == Some(&'}') {
                                p += 1;
                            }
                            if p != cursor {
                                edit.pending_selection = Some(p..p);
                                edit.selection = Some(p..p);
                            }
                        }
                    }
                    return false;
                }
                (Some(s), _) => s.clone(),
                (None, _) => 0..rich.char_count(),
            };
            let out = apply_to(&rich, range.clone(), change);
            let text = markup_of(&out);
            if text != edit.text {
                let (_, new_map) = RichText::parse_mapped(&text);
                let new_len = text.chars().count();
                edit.text = text;
                edit.pending_selection = sel.map(|s| markup_range(&new_map, new_len, s));
                if edit.pending_selection.is_none() {
                    edit.pending_selection = Some(new_len..new_len);
                }
                edit.selection = edit.pending_selection.clone();
            }
            return false;
        }
        let ids = self.selected_texts();
        let mut changed: Vec<(u32, String)> = Vec::new();
        for id in ids {
            let Some(PageObject::Text(t)) = self.molecule.object_by_id(id) else { continue };
            // Lines are parsed apart, as the text is drawn.
            let mut rich = RichText::new();
            for (k, line) in t.lines().into_iter().enumerate() {
                if k > 0 {
                    rich = rich.text("\n");
                }
                for span in line.spans {
                    rich = rich.span(span.text, span.style);
                }
            }
            let out = markup_of(&apply_to(&rich, 0..rich.char_count(), change));
            if out != t.text {
                changed.push((id, out));
            }
        }
        if changed.is_empty() {
            return false;
        }
        self.push_undo();
        for (id, text) in changed {
            if let Some(PageObject::Text(t)) = self.molecule.object_by_id_mut(id) {
                t.text = text;
            }
        }
        true
    }

    /// Markup for a new text typed as `markup`: [`new_text_style`](Self::new_text_style) fills
    /// in what its characters do not set themselves.
    pub fn styled_new_text(&self, markup: &str) -> String {
        if self.new_text_style == SpanStyle::default() {
            return markup.to_owned();
        }
        let lines: Vec<String> = markup
            .split('\n')
            .map(|line| {
                let mut out = RichText::new();
                for span in RichText::parse(line).spans {
                    let mut style = span.style;
                    fill_style(&mut style, &self.new_text_style);
                    out = out.span(span.text, style);
                }
                out.to_compact_markup()
            })
            .collect();
        lines.join("\n")
    }

    /// Keep the text box open when the pointer is pressed inside `rect` (a host's format bar).
    /// Call it every frame the bar is shown; a press anywhere else on the screen still commits
    /// the text, as before.
    pub fn keep_text_edit_open_over(&mut self, rect: egui::Rect) {
        self.text_ui_rects.push(rect);
    }

    /// After the text box changed `before` into the current text with Symbol typing on: set
    /// what was typed in Symbol, and put the cursor after it.
    pub(super) fn symbol_typed(&mut self, before: &str) {
        let Some(edit) = self.text_edit.as_mut() else { return };
        let (old, new): (Vec<char>, Vec<char>) = (before.chars().collect(), edit.text.chars().collect());
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
        let inserted = prefix..new.len() - suffix;
        if inserted.is_empty() {
            return;
        }
        let (rich, map) = RichText::parse_mapped(&edit.text);
        let range = text_range(&map, inserted);
        if range.is_empty() || rich.styles_in(range.clone()).iter().all(|s| s.is_symbol()) {
            return;
        }
        let text = markup_of(&apply_to(&rich, range.clone(), FormatChange::Symbol(true)));
        let (_, new_map) = RichText::parse_mapped(&text);
        let len = text.chars().count();
        let cursor = markup_range(&new_map, len, range.end..range.end);
        edit.text = text;
        edit.selection = Some(cursor.clone());
        edit.pending_selection = Some(cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::TextEditState;

    fn editing(text: &str, selection: Option<Range<usize>>) -> ChemStructEditor {
        let mut e = ChemStructEditor::default();
        e.text_edit = Some(TextEditState { text: text.into(), selection, ..TextEditState::new([0.0, 0.0]) });
        e
    }

    #[test]
    fn symbol_on_a_selection_keeps_its_look() {
        // "αb" with "αb" selected: α (Unicode) becomes Symbol's a, b becomes β.
        let mut e = editing("αb c", Some(0..2));
        e.apply_text_format(FormatChange::Symbol(true));
        let edit = e.text_edit.as_ref().unwrap();
        assert_eq!(edit.text, "{@Symbol:ab} c");
        assert_eq!(edit.pending_selection, Some(9..11));
        // And out of it again: shown the same, now as Unicode in the base font.
        let mut e = editing("{@Symbol:ab} c", Some(9..11));
        e.apply_text_format(FormatChange::Symbol(false));
        assert_eq!(e.text_edit.as_ref().unwrap().text, "αβ c");
    }

    #[test]
    fn with_nothing_selected_a_change_takes_the_whole_text_and_symbol_switches_typing() {
        let mut e = editing("a\nb", None);
        e.apply_text_format(FormatChange::Font(Some(FontName::new("Georgia"))));
        assert_eq!(e.text_edit.as_ref().unwrap().text, "{@Georgia:a}\n{@Georgia:b}");
        assert_eq!(e.new_text_style.font, Some(FontName::new("Georgia")), "new text follows");
        e.apply_text_format(FormatChange::Symbol(true));
        assert!(e.text_edit.as_ref().unwrap().symbol);
        assert!(e.text_format().symbol_typing);
    }

    #[test]
    fn a_font_change_keeps_symbol_text_in_symbol() {
        let rich = RichText::parse("heat {@Symbol:D}");
        let out = apply_to(&rich, 0..rich.char_count(), FormatChange::Font(Some(FontName::new("Georgia"))));
        assert_eq!(out.to_compact_markup(), "{@Georgia:heat }{@Symbol:D}");
    }

    #[test]
    fn typing_in_symbol_sets_the_typed_letters_in_symbol() {
        let mut e = editing("x", None);
        e.text_edit.as_mut().unwrap().symbol = true;
        e.text_edit.as_mut().unwrap().text = "xp".into();
        e.symbol_typed("x");
        let edit = e.text_edit.as_ref().unwrap();
        assert_eq!(edit.text, "x{@Symbol:p}");
        assert_eq!(edit.pending_selection, Some(11..11), "the cursor stays after the π");
        // Typing on inside the group changes nothing more.
        let mut e = editing("x{@Symbol:p}", None);
        e.text_edit.as_mut().unwrap().symbol = true;
        e.text_edit.as_mut().unwrap().text = "x{@Symbol:pa}".into();
        e.symbol_typed("x{@Symbol:p}");
        assert_eq!(e.text_edit.as_ref().unwrap().text, "x{@Symbol:pa}");
    }

    #[test]
    fn selected_texts_are_formatted_whole_in_one_undo_step() {
        let mut e = ChemStructEditor::default();
        let a = e.molecule.add_text([0.0, 0.0], "K_2CO_3");
        let b = e.molecule.add_text([0.0, 3.0], "THF");
        e.selected_objects.extend([a, b]);
        assert_eq!(e.text_format().target, FormatTarget::Selected(2));
        assert!(e.apply_text_format(FormatChange::Size(Some(12.0))));
        assert_eq!(e.undo_stack.len(), 1);
        let texts: Vec<&str> = e.molecule.objects.iter().filter_map(|o| o.as_text()).map(|t| t.text.as_str()).collect();
        assert_eq!(texts, ["{@,12:K_2CO_3}", "{@,12:THF}"]);
        assert!(!e.apply_text_format(FormatChange::Size(Some(12.0))), "nothing more to change");
    }

    #[test]
    fn new_text_takes_the_current_style() {
        let mut e = ChemStructEditor::default();
        e.new_text_style.font = Some(FontName::new("Georgia"));
        assert_eq!(e.styled_new_text("a{@Arial:b}\nc"), "{@Georgia:a}{@Arial:b}\n{@Georgia:c}");
        e.new_text_style = SpanStyle::default();
        assert_eq!(e.styled_new_text("a_2"), "a_2");
    }

    #[test]
    fn ranges_map_between_markup_and_text() {
        let (_, map) = RichText::parse_mapped("a{@Symbol:b}c");
        assert_eq!(text_range(&map, 0..13), 0..3);
        assert_eq!(text_range(&map, 10..11), 1..2);
        assert_eq!(markup_range(&map, 13, 1..2), 10..11);
        assert_eq!(markup_range(&map, 13, 3..3), 13..13);
        assert_eq!(markup_range(&map, 13, 2..2), 11..11);
    }
}
