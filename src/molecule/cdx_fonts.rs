//! Fonts in a ChemDraw document: its font table, and text whose style runs name fonts and sizes.
//!
//! A CDX document lists its fonts once, in the document's FontTable property (`0x0100`; CDXML's
//! `<fonttable>`): a platform word, then each font's id, character set and name. A text's style
//! runs (the CDXString of its Text property) each give the font id, face, size (in 1/20 pt) and
//! colour of the characters from their start on.
//!
//! Reading turns a run's font and size into the span's [`font`](SpanStyle::font) and
//! [`size`](SpanStyle::size); the document default, 10 pt Arial, is left as `None` so text in
//! ChemDraw's own style reads as plain markup. Writing does the reverse, adding to the font table
//! the fonts it does not list yet (and only then rewriting it, so a document whose texts are not
//! touched keeps its table byte for byte). A Symbol run is written as it is held, ASCII in the
//! Symbol font (character set Symbol, 42), which is what ChemDraw shows as Greek.

use super::cdx;
use super::cdx_codec::CdxObject;
use crate::widget::fonts::FontName;
use crate::widget::rich_text::{RichText, SpanStyle, BASE_POINTS};

/// kCDXProp_FontTable.
pub(crate) const PROP_FONT_TABLE: u16 = 0x0100;
/// The character set ChemDraw gives the Symbol font.
pub(crate) const CHARSET_SYMBOL: u16 = 42;
/// Windows Latin-1, what ChemDraw gives text fonts.
pub(crate) const CHARSET_LATIN1: u16 = 1252;
/// The font ChemDraw sets text in unless told otherwise, read as no font at all.
pub(crate) const DEFAULT_FONT: &str = "Arial";
/// ChemDraw's text size (10 pt), in 1/20 pt.
pub(crate) const DEFAULT_SIZE: u16 = 200;

/// A document's font table.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FontTable {
    platform: u16,
    /// (id, character set, name), in the table's order.
    fonts: Vec<(u16, u16, String)>,
    /// Whether fonts were added since it was read.
    changed: bool,
}

impl FontTable {
    /// The font table of `doc` (empty when it has none).
    pub fn of(doc: &CdxObject) -> Self {
        doc.prop(PROP_FONT_TABLE).map(Self::parse).unwrap_or_default()
    }

    fn parse(v: &[u8]) -> Self {
        let u = |i: usize| v.get(i..i + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
        let mut table = FontTable { platform: u(0).unwrap_or(0), ..Default::default() };
        let mut at = 4;
        for _ in 0..u(2).unwrap_or(0) {
            let (Some(id), Some(charset), Some(len)) = (u(at), u(at + 2), u(at + 4)) else { break };
            let Some(name) = v.get(at + 6..at + 6 + len as usize) else { break };
            table.fonts.push((id, charset, name.iter().map(|&b| b as char).collect()));
            at += 6 + len as usize;
        }
        table
    }

    fn encode(&self) -> Vec<u8> {
        let mut v = self.platform.to_le_bytes().to_vec();
        v.extend_from_slice(&(self.fonts.len() as u16).to_le_bytes());
        for (id, charset, name) in &self.fonts {
            let name: Vec<u8> = name.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }).collect();
            v.extend_from_slice(&id.to_le_bytes());
            v.extend_from_slice(&charset.to_le_bytes());
            v.extend_from_slice(&(name.len() as u16).to_le_bytes());
            v.extend_from_slice(&name);
        }
        v
    }

    /// The name of font `id`.
    pub fn name(&self, id: u16) -> Option<&str> {
        self.fonts.iter().find(|(i, _, _)| *i == id).map(|(_, _, n)| n.as_str())
    }

    /// The id of the font named `name` (case-insensitively), added to the table if it is not
    /// there yet.
    pub fn id_for(&mut self, name: &str) -> u16 {
        if let Some((id, _, _)) = self.fonts.iter().find(|(_, _, n)| n.eq_ignore_ascii_case(name)) {
            return *id;
        }
        // ChemDraw numbers its fonts from 3 (Arial in a new document).
        let id = self.fonts.iter().map(|(i, _, _)| *i + 1).max().unwrap_or(3).max(3);
        let charset = if name.eq_ignore_ascii_case(crate::widget::fonts::SYMBOL) { CHARSET_SYMBOL } else { CHARSET_LATIN1 };
        self.fonts.push((id, charset, name.to_owned()));
        self.changed = true;
        id
    }

    /// Write the table back into `doc` if fonts were added to it.
    pub fn store(&self, doc: &mut CdxObject) {
        if self.changed {
            doc.set(PROP_FONT_TABLE, self.encode());
        }
    }
}

/// One style run of a CDXString: from character `start` on, this font, face, size (1/20 pt)
/// and colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StyleRun {
    pub start: u16,
    pub font: u16,
    pub face: u16,
    pub size: u16,
    pub color: u16,
}

/// The style runs and characters of a CDXString (Latin-1, as the Text property holds them).
pub(crate) fn parse_string(v: &[u8]) -> Option<(Vec<StyleRun>, String)> {
    let n = u16::from_le_bytes([*v.first()?, *v.get(1)?]) as usize;
    let mut runs = Vec::with_capacity(n);
    for i in 0..n {
        let s = v.get(2 + i * 10..2 + i * 10 + 10)?;
        let w = |k: usize| u16::from_le_bytes([s[k], s[k + 1]]);
        runs.push(StyleRun { start: w(0), font: w(2), face: w(4), size: w(6), color: w(8) });
    }
    let text = v.get(2 + n * 10..)?.iter().map(|&b| b as char).collect();
    Some((runs, text))
}

/// A CDXString of `runs` over `text`: Latin-1 (`?` for what it cannot hold) for the Text
/// property, or UTF-8 for the UTF-8 Text property.
pub(crate) fn string_of(runs: &[StyleRun], text: &str, utf8: bool) -> Vec<u8> {
    let mut out = (runs.len() as u16).to_le_bytes().to_vec();
    for r in runs {
        for v in [r.start, r.font, r.face, r.size, r.color] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    if utf8 {
        out.extend_from_slice(text.as_bytes());
    } else {
        out.extend(text.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }));
    }
    out
}

/// Rich text from style runs over `text`, the fonts named through `table`: each run's face read
/// as [`cdx::rich_of`] reads it, and its font and size set on its spans unless they are the
/// document default (10 pt Arial).
pub(crate) fn rich_of(runs: &[StyleRun], text: &str, table: &FontTable) -> RichText {
    let chars: Vec<char> = text.chars().collect();
    if runs.is_empty() {
        return cdx::rich_of(&[], text);
    }
    let mut rich = RichText::new();
    for (i, run) in runs.iter().enumerate() {
        let start = (run.start as usize).min(chars.len());
        let end = runs.get(i + 1).map_or(chars.len(), |r| (r.start as usize).min(chars.len())).max(start);
        let part: String = chars[start..end].iter().collect();
        let font = table.name(run.font).filter(|n| !n.eq_ignore_ascii_case(DEFAULT_FONT)).map(FontName::new);
        let size = (run.size != DEFAULT_SIZE && run.size > 0).then(|| run.size as f32 / 20.0);
        for span in cdx::rich_of(&[(0, run.face)], &part).spans {
            rich = rich.span(span.text, SpanStyle { font, size, ..span.style });
        }
    }
    rich
}

/// Style runs for `rich` and its characters, its fonts given ids in `table` (added when new).
pub(crate) fn runs_of(rich: &RichText, table: &mut FontTable) -> (Vec<StyleRun>, String) {
    let (faces, text) = cdx::runs_of(rich);
    // `cdx::runs_of` starts a run where the face changes; start one also where the font or size
    // does, from each span's style.
    let mut runs: Vec<StyleRun> = Vec::new();
    let mut at = 0u16;
    let mut faces = faces.into_iter().peekable();
    let mut face = 0;
    for span in rich.spans.iter().filter(|s| !s.text.is_empty()) {
        let n = span.text.chars().count() as u16;
        let font = table.id_for(span.style.font.map_or(DEFAULT_FONT, |f| f.as_str()));
        let size = span.style.size.map_or(DEFAULT_SIZE, |pt| (pt * 20.0).round().clamp(1.0, u16::MAX as f32) as u16);
        for k in at..at + n {
            while let Some(&(start, f)) = faces.peek() {
                if start > k {
                    break;
                }
                face = f;
                faces.next();
            }
            let run = StyleRun { start: k, font, face, size, color: 0 };
            match runs.last() {
                Some(r) if (r.font, r.face, r.size, r.color) == (font, face, size, 0) => {}
                _ => runs.push(run),
            }
        }
        at += n;
    }
    (runs, text)
}

/// A text object's text: its style runs over the UTF-8 text when it has one, the Latin-1 text
/// otherwise, its fonts named by `table`. Line breaks (carriage returns) become `\n`.
pub(crate) fn text_of(t: &CdxObject, table: &FontTable) -> Option<RichText> {
    let (runs, latin1) = parse_string(t.prop(cdx::PROP_TEXT)?)?;
    let utf8 = t
        .prop(cdx::PROP_UTF8_TEXT)
        .and_then(|v| parse_string(v).map(|(_, s)| s))
        .and_then(|s| String::from_utf8(s.chars().map(|c| c as u8).collect()).ok());
    let text = utf8.filter(|u| u.chars().count() == latin1.chars().count()).unwrap_or(latin1).replace('\r', "\n");
    let rich = rich_of(&runs, &text, table);
    (!rich.is_empty()).then_some(rich)
}

/// Rough width of `rich` in CDX units, from Arial's widths at each span's size.
pub(crate) fn estimated_width(rich: &RichText) -> f64 {
    rich.spans
        .iter()
        .map(|s| {
            let script = if s.style.script == crate::widget::rich_text::Script::Normal { 1.0 } else { 0.7 };
            let size = s.style.size.unwrap_or(BASE_POINTS) as f64 / 10.0;
            s.text.chars().map(cdx::arial_width).sum::<f64>() * script * size
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn a_truncated_table_reads_what_it_can() {
        // Two fonts announced, the first one's name cut short.
        assert!(FontTable::parse(&hex("000002000300e404050041726961")).fonts.is_empty());
    }

    #[test]
    fn the_table_round_trips_and_grows_only_when_asked() {
        let v = hex("000001000300e4040500417269616c");
        let mut t = FontTable::parse(&v);
        assert_eq!(t.name(3), Some("Arial"));
        assert_eq!(t.encode(), v);
        assert_eq!(t.id_for("arial"), 3);
        let mut doc = CdxObject::new(cdx::OBJ_DOCUMENT, 0);
        doc.set(PROP_FONT_TABLE, v.clone());
        t.store(&mut doc);
        assert_eq!(doc.prop(PROP_FONT_TABLE), Some(&v[..]), "unchanged tables are not rewritten");
        assert_eq!(t.id_for("Symbol"), 4);
        assert_eq!(t.fonts[1], (4, CHARSET_SYMBOL, "Symbol".to_owned()));
        assert_eq!(t.id_for("Times New Roman"), 5);
        assert_eq!(t.fonts[2].1, CHARSET_LATIN1);
        t.store(&mut doc);
        assert_eq!(FontTable::of(&doc).fonts.len(), 3);
    }

    #[test]
    fn runs_carry_fonts_and_sizes_both_ways() {
        let mut t = FontTable::parse(&hex("000001000300e4040500417269616c"));
        let rich = RichText::parse("CH_2{@Symbol:p}{@Times New Roman,12:**x**}");
        let (runs, text) = runs_of(&rich, &mut t);
        assert_eq!(text, "CH2px");
        let symbol = t.id_for("Symbol");
        let times = t.id_for("Times New Roman");
        assert_eq!(
            runs,
            vec![
                StyleRun { start: 0, font: 3, face: 0, size: 200, color: 0 },
                StyleRun { start: 2, font: 3, face: 0x20, size: 200, color: 0 },
                StyleRun { start: 3, font: symbol, face: 0, size: 200, color: 0 },
                StyleRun { start: 4, font: times, face: 1, size: 240, color: 0 },
            ]
        );
        let back = rich_of(&runs, &text, &t);
        assert_eq!(back.spans, rich.spans);
        let bytes = string_of(&runs, &text, false);
        assert_eq!(parse_string(&bytes), Some((runs, text)));
    }
}
