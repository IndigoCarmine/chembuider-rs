//! Named fonts for rich text: the font a span is set in ([`FontName`]), ChemDraw's Symbol font,
//! and the system fonts those names are drawn with.
//!
//! # Font names
//!
//! A span names its font by family ("Times New Roman", "Arial", "Symbol"); `None` is the text's
//! base font. Names are interned ([`FontName`] is `Copy`, like the rest of a span's style), so a
//! name costs nothing to pass around; only the few names a document actually uses are interned.
//!
//! # Drawing a named font
//!
//! egui draws a named font through [`FontFamily::Name`]. [`SystemFontProvider`] is an egui
//! [`FontProvider`] that, asked for such a family, installs the system font of that name (found
//! with `fontdb`), followed by egui's own proportional fonts so characters the font lacks still
//! show. A name the system has no font for falls back to those fonts alone: the text keeps its
//! font name (so it goes back into a file unchanged) and is drawn in the default font.
//! [`install`] adds the provider to an egui context (the editor does so itself), and
//! [`standalone_fonts`] makes a font stack with it for the image exports.
//!
//! # Symbol
//!
//! ChemDraw writes Greek letters as plain ASCII in the Symbol font: "p" in Symbol is π. Its
//! documents keep it that way, and so does the span (font `Symbol`, text `p`) so the text goes
//! back to ChemDraw as it came. Only drawing maps the characters, with [`symbol_to_unicode`]
//! (the Adobe Symbol encoding), and draws the result in the text's base font; nothing depends on
//! a legacy Symbol font being installed.
//!
//! # CJK labels
//!
//! egui's own fonts have no Chinese, Japanese or Korean. [`add_cjk_ui_font`] adds a system font
//! that has, picked for the [UI language](crate::i18n): a Simplified Chinese, Traditional
//! Chinese or Korean font first for those languages, a Japanese font first otherwise.

use std::borrow::Cow;
use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock};

use egui::epaint::text::{FontInsert, FontProvider, InsertFontFamily};
use egui::{FontData, FontFamily, FontPriority};

use crate::i18n::Language;

/// The name of ChemDraw's Greek font.
pub const SYMBOL: &str = "Symbol";

/// The family of a font a span is set in, interned so it is `Copy`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontName(&'static str);

impl FontName {
    /// The (interned) name `name`, trimmed.
    pub fn new(name: &str) -> Self {
        static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
        let name = name.trim();
        let mut names = NAMES.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
        if let Some(&known) = names.get(name) {
            return FontName(known);
        }
        let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
        names.insert(leaked);
        FontName(leaked)
    }

    /// ChemDraw's Symbol font.
    pub fn symbol() -> Self {
        Self::new(SYMBOL)
    }

    pub fn as_str(&self) -> &'static str {
        self.0
    }

    /// Whether this is the Symbol font (whose ASCII is Greek).
    pub fn is_symbol(&self) -> bool {
        self.0.eq_ignore_ascii_case(SYMBOL)
    }

    /// The egui family this font is drawn with: the font's own family, or `base` for Symbol
    /// (whose characters are drawn as Unicode in the base font; see the [module docs](self)).
    pub fn family(&self, base: &FontFamily) -> FontFamily {
        if self.is_symbol() { base.clone() } else { FontFamily::Name(self.0.into()) }
    }
}

impl std::fmt::Debug for FontName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl std::fmt::Display for FontName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl serde::Serialize for FontName {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

impl<'de> serde::Deserialize<'de> for FontName {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Ok(FontName::new(&name))
    }
}

// ─── Symbol encoding ─────────────────────────────────────────────────────────

/// The Adobe Symbol encoding (Adobe's `symbol.txt`, as Windows and macOS map their Symbol fonts)
/// for the codes that differ from Latin-1: (code, Unicode). The Greek capitals Delta, Omega and
/// small mu are the Greek letters rather than the math signs Adobe's table lists beside them
/// (U+2206, U+2126, U+00B5), as a chemist typing D, W or m in Symbol means them.
const SYMBOL_TABLE: &[(u8, char)] = &[
    (0x22, '\u{2200}'), // ∀
    (0x24, '\u{2203}'), // ∃
    (0x27, '\u{220B}'), // ∋
    (0x2A, '\u{2217}'), // ∗
    (0x2D, '\u{2212}'), // −
    (0x40, '\u{2245}'), // ≅
    (b'A', 'Α'),
    (b'B', 'Β'),
    (b'C', 'Χ'),
    (b'D', 'Δ'),
    (b'E', 'Ε'),
    (b'F', 'Φ'),
    (b'G', 'Γ'),
    (b'H', 'Η'),
    (b'I', 'Ι'),
    (b'J', 'ϑ'),
    (b'K', 'Κ'),
    (b'L', 'Λ'),
    (b'M', 'Μ'),
    (b'N', 'Ν'),
    (b'O', 'Ο'),
    (b'P', 'Π'),
    (b'Q', 'Θ'),
    (b'R', 'Ρ'),
    (b'S', 'Σ'),
    (b'T', 'Τ'),
    (b'U', 'Υ'),
    (b'V', 'ς'),
    (b'W', 'Ω'),
    (b'X', 'Ξ'),
    (b'Y', 'Ψ'),
    (b'Z', 'Ζ'),
    (0x5C, '\u{2234}'), // ∴
    (0x5E, '\u{22A5}'), // ⊥
    (b'a', 'α'),
    (b'b', 'β'),
    (b'c', 'χ'),
    (b'd', 'δ'),
    (b'e', 'ε'),
    (b'f', 'φ'),
    (b'g', 'γ'),
    (b'h', 'η'),
    (b'i', 'ι'),
    (b'j', 'ϕ'),
    (b'k', 'κ'),
    (b'l', 'λ'),
    (b'm', 'μ'),
    (b'n', 'ν'),
    (b'o', 'ο'),
    (b'p', 'π'),
    (b'q', 'θ'),
    (b'r', 'ρ'),
    (b's', 'σ'),
    (b't', 'τ'),
    (b'u', 'υ'),
    (b'v', 'ϖ'),
    (b'w', 'ω'),
    (b'x', 'ξ'),
    (b'y', 'ψ'),
    (b'z', 'ζ'),
    (0x7E, '\u{223C}'), // ∼
    (0xA1, 'ϒ'),
    (0xA2, '\u{2032}'), // ′
    (0xA3, '\u{2264}'), // ≤
    (0xA4, '\u{2044}'), // ⁄
    (0xA5, '\u{221E}'), // ∞
    (0xA6, '\u{0192}'), // ƒ
    (0xA7, '\u{2663}'), // ♣
    (0xA8, '\u{2666}'), // ♦
    (0xA9, '\u{2665}'), // ♥
    (0xAA, '\u{2660}'), // ♠
    (0xAB, '\u{2194}'), // ↔
    (0xAC, '\u{2190}'), // ←
    (0xAD, '\u{2191}'), // ↑
    (0xAE, '\u{2192}'), // →
    (0xAF, '\u{2193}'), // ↓
    (0xB2, '\u{2033}'), // ″
    (0xB3, '\u{2265}'), // ≥
    (0xB4, '\u{00D7}'), // ×
    (0xB5, '\u{221D}'), // ∝
    (0xB6, '\u{2202}'), // ∂
    (0xB7, '\u{2022}'), // •
    (0xB8, '\u{00F7}'), // ÷
    (0xB9, '\u{2260}'), // ≠
    (0xBA, '\u{2261}'), // ≡
    (0xBB, '\u{2248}'), // ≈
    (0xBC, '\u{2026}'), // …
    (0xBE, '\u{23AF}'), // ⎯
    (0xBF, '\u{21B5}'), // ↵
    (0xC0, '\u{2135}'), // ℵ
    (0xC1, '\u{2111}'), // ℑ
    (0xC2, '\u{211C}'), // ℜ
    (0xC3, '\u{2118}'), // ℘
    (0xC4, '\u{2297}'), // ⊗
    (0xC5, '\u{2295}'), // ⊕
    (0xC6, '\u{2205}'), // ∅
    (0xC7, '\u{2229}'), // ∩
    (0xC8, '\u{222A}'), // ∪
    (0xC9, '\u{2283}'), // ⊃
    (0xCA, '\u{2287}'), // ⊇
    (0xCB, '\u{2284}'), // ⊄
    (0xCC, '\u{2282}'), // ⊂
    (0xCD, '\u{2286}'), // ⊆
    (0xCE, '\u{2208}'), // ∈
    (0xCF, '\u{2209}'), // ∉
    (0xD0, '\u{2220}'), // ∠
    (0xD1, '\u{2207}'), // ∇
    (0xD2, '\u{00AE}'), // ®
    (0xD3, '\u{00A9}'), // ©
    (0xD4, '\u{2122}'), // ™
    (0xD5, '\u{220F}'), // ∏
    (0xD6, '\u{221A}'), // √
    (0xD7, '\u{22C5}'), // ⋅
    (0xD8, '\u{00AC}'), // ¬
    (0xD9, '\u{2227}'), // ∧
    (0xDA, '\u{2228}'), // ∨
    (0xDB, '\u{21D4}'), // ⇔
    (0xDC, '\u{21D0}'), // ⇐
    (0xDD, '\u{21D1}'), // ⇑
    (0xDE, '\u{21D2}'), // ⇒
    (0xDF, '\u{21D3}'), // ⇓
    (0xE0, '\u{25CA}'), // ◊
    (0xE1, '\u{2329}'), // 〈
    (0xE5, '\u{2211}'), // ∑
    (0xF1, '\u{232A}'), // 〉
    (0xF2, '\u{222B}'), // ∫
];

/// The character a Symbol-font code shows: `p` is π, `D` is Δ. Codes the encoding shares with
/// Latin-1 (digits, most punctuation) are themselves; so is anything beyond Latin-1, except the
/// private-use copy U+F020–U+F0FF that some programs write Symbol text as.
pub fn symbol_to_unicode(c: char) -> char {
    let code = match c as u32 {
        code @ 0xF020..=0xF0FF => code - 0xF000,
        code => code,
    };
    if code > 0xFF {
        return c;
    }
    SYMBOL_TABLE.iter().find(|(k, _)| *k as u32 == code).map_or(char::from_u32(code).unwrap_or(c), |&(_, u)| u)
}

/// The Symbol-font code that shows `c` (π → `p`), if the encoding has one that is not `c` itself.
pub fn unicode_to_symbol(c: char) -> Option<char> {
    // Δ, Ω and μ also have the sign forms Adobe's table maps them from.
    let c = match c {
        '\u{2206}' => 'Δ',
        '\u{2126}' => 'Ω',
        '\u{00B5}' => 'μ',
        c => c,
    };
    SYMBOL_TABLE.iter().find(|(_, u)| *u == c).map(|&(k, _)| k as char)
}

/// The text a span in `font` shows: Symbol text mapped to Unicode, anything else as it is.
pub fn display_text(font: Option<FontName>, text: &str) -> Cow<'_, str> {
    match font {
        Some(f) if f.is_symbol() => Cow::Owned(text.chars().map(symbol_to_unicode).collect()),
        _ => Cow::Borrowed(text),
    }
}

// ─── System fonts ────────────────────────────────────────────────────────────

/// The system's fonts, read once (on the first call; [`install`] starts it in the background).
#[cfg(not(target_arch = "wasm32"))]
pub fn system_database() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
}

/// The families of the installed fonts, sorted case-insensitively, without duplicates. Always
/// includes [`SYMBOL`] (drawn from its encoding, installed or not). Empty on the web except for it.
pub fn system_families() -> &'static [String] {
    static FAMILIES: OnceLock<Vec<String>> = OnceLock::new();
    FAMILIES.get_or_init(|| {
        #[allow(unused_mut)]
        let mut names: Vec<String> = vec![SYMBOL.to_owned()];
        #[cfg(not(target_arch = "wasm32"))]
        for face in system_database().faces() {
            // The first family name is the English one, which is what documents store.
            if let Some((name, _)) = face.families.first() {
                names.push(name.clone());
            }
        }
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    })
}

/// Whether the system has a font of family `name` (case-insensitive). Symbol always counts.
pub fn is_installed(name: &str) -> bool {
    name.eq_ignore_ascii_case(SYMBOL) || system_families().iter().any(|f| f.eq_ignore_ascii_case(name))
}

// ─── CJK UI font ─────────────────────────────────────────────────────────────

/// Simplified Chinese UI fonts: Windows, macOS, Linux.
const CJK_SC: [&str; 5] = ["Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", "Noto Sans SC", "Source Han Sans SC"];
/// Traditional Chinese UI fonts.
const CJK_TC: [&str; 5] = ["Microsoft JhengHei", "PingFang TC", "Noto Sans CJK TC", "Noto Sans TC", "Source Han Sans TC"];
/// Korean UI fonts.
const CJK_KR: [&str; 5] = ["Malgun Gothic", "Apple SD Gothic Neo", "Noto Sans CJK KR", "Noto Sans KR", "Source Han Sans KR"];
/// Japanese UI fonts.
const CJK_JP: [&str; 8] = [
    "Yu Gothic",
    "Meiryo",
    "MS Gothic",
    "Hiragino Sans",
    "Hiragino Kaku Gothic ProN",
    "Noto Sans CJK JP",
    "Noto Sans JP",
    "Source Han Sans JP",
];

/// The font data key [`add_cjk_ui_font`] installs its font under.
pub const CJK_UI_FONT: &str = "chembuilder-cjk-ui";

/// System font families with CJK glyphs, in the order to try for labels in `language`: its
/// own fonts first for Simplified Chinese, Traditional Chinese and Korean (Microsoft YaHei /
/// JhengHei / Malgun Gothic on Windows, PingFang SC / TC and Apple SD Gothic Neo on macOS,
/// Noto Sans CJK SC / TC / KR on Linux), then Japanese fonts, then the other CJK ones.
/// Japanese fonts come first for every other language.
pub fn cjk_ui_font_families(language: Language) -> Vec<&'static str> {
    let groups: [&[&'static str]; 4] = match language {
        Language::ZhHans => [&CJK_SC, &CJK_JP, &CJK_TC, &CJK_KR],
        Language::ZhHant => [&CJK_TC, &CJK_JP, &CJK_SC, &CJK_KR],
        Language::Ko => [&CJK_KR, &CJK_JP, &CJK_SC, &CJK_TC],
        _ => [&CJK_JP, &CJK_SC, &CJK_TC, &CJK_KR],
    };
    groups.concat()
}

/// Add the first installed font of [`cjk_ui_font_families`]`(language)` to `fonts` (as
/// [`CJK_UI_FONT`]), behind egui's own proportional and monospace fonts so it draws only what
/// they lack. Calling it again (with another language) replaces the font. Returns the family
/// used, `None` when the system has none of them (or on the web) and `fonts` is unchanged.
///
/// It reads the system's font list the first time, which can take a moment; then
/// `ctx.set_fonts(fonts)`. Optional: the widgets work without it, but CJK labels need a font
/// that has them.
pub fn add_cjk_ui_font(fonts: &mut egui::FontDefinitions, language: Language) -> Option<&'static str> {
    #[cfg(not(target_arch = "wasm32"))]
    for family in cjk_ui_font_families(language) {
        if let Some(data) = system_font_data(family) {
            fonts.font_data.insert(CJK_UI_FONT.to_owned(), Arc::new(data));
            for list in [FontFamily::Proportional, FontFamily::Monospace] {
                let list = fonts.families.entry(list).or_default();
                list.retain(|f| f != CJK_UI_FONT);
                list.push(CJK_UI_FONT.to_owned());
            }
            return Some(family);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (fonts, language);
    None
}

/// The regular face of the system font `name`, loaded once and shared.
#[cfg(not(target_arch = "wasm32"))]
fn system_font_data(name: &str) -> Option<FontData> {
    type Cache = Mutex<std::collections::HashMap<String, Option<FontData>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let key = name.to_lowercase();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(found) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
        return found.clone();
    }
    let db = system_database();
    let families = [fontdb::Family::Name(name)];
    let query = fontdb::Query { families: &families, ..Default::default() };
    let data = db.query(&query).and_then(|id| {
        // `query` matches families loosely; take the face only when it is the one asked for.
        let face = db.face(id)?;
        if !face.families.iter().any(|(f, _)| f.eq_ignore_ascii_case(name)) {
            return None;
        }
        db.with_face_data(id, |bytes, index| {
            let mut data = FontData::from_owned(bytes.to_vec());
            data.index = index;
            data
        })
    });
    cache.lock().unwrap_or_else(|e| e.into_inner()).insert(key, data.clone());
    data
}

/// An egui [`FontProvider`] for [`FontFamily::Name`] families: the system font of that name,
/// then egui's default proportional fonts as fallbacks. See the [module docs](self).
#[derive(Debug, Default)]
pub struct SystemFontProvider;

impl FontProvider for SystemFontProvider {
    fn fonts_for_family(&self, family: &FontFamily) -> Vec<FontInsert> {
        let FontFamily::Name(name) = family else { return Vec::new() };
        let to_family = |priority| vec![InsertFontFamily { family: family.clone(), priority }];
        let mut out = Vec::new();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(data) = system_font_data(name) {
            out.push(FontInsert::new(&format!("system:{}", name.to_lowercase()), data, to_family(FontPriority::Highest)));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = name;
        // egui's own fonts after it (or instead of it), under the names egui installs them by,
        // so they are the faces already loaded.
        let defaults = egui::FontDefinitions::default();
        for font in defaults.families.get(&FontFamily::Proportional).into_iter().flatten() {
            if let Some(data) = defaults.font_data.get(font) {
                out.push(FontInsert::new(font, (**data).clone(), to_family(FontPriority::Lowest)));
            }
        }
        out
    }
}

/// Add [`SystemFontProvider`] to `ctx` (once; later calls do nothing), and start reading the
/// system's font list in the background so the first named font or font menu does not wait.
pub fn install(ctx: &egui::Context) {
    let id = egui::Id::unique("chembuilder_system_font_provider");
    if ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, true));
    ctx.add_font_provider(Arc::new(SystemFontProvider));
    #[cfg(not(target_arch = "wasm32"))]
    std::thread::spawn(|| {
        system_families();
    });
}

/// A standalone egui font stack (egui's default fonts) that also draws named system fonts, for
/// laying text out away from a context (the image exports).
pub fn standalone_fonts() -> egui::epaint::text::Fonts {
    egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), egui::FontDefinitions::default())
        .with_font_providers(vec![Arc::new(SystemFontProvider)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_fonts_follow_the_language() {
        assert_eq!(cjk_ui_font_families(Language::ZhHans)[0], "Microsoft YaHei");
        assert_eq!(cjk_ui_font_families(Language::ZhHant)[0], "Microsoft JhengHei");
        assert_eq!(cjk_ui_font_families(Language::Ko)[0], "Malgun Gothic");
        for language in [Language::Ja, Language::En, Language::De] {
            assert_eq!(cjk_ui_font_families(language)[0], "Yu Gothic", "Japanese first, as before");
        }
        // Every language falls back to a Japanese font after its own.
        let sc = cjk_ui_font_families(Language::ZhHans);
        assert_eq!(sc[CJK_SC.len()], "Yu Gothic");
        assert!(sc.contains(&"PingFang SC") && sc.contains(&"Noto Sans CJK SC"));
        assert_eq!(sc.len(), CJK_SC.len() + CJK_TC.len() + CJK_KR.len() + CJK_JP.len());
    }

    #[test]
    fn a_cjk_font_is_added_behind_eguis_once() {
        let mut fonts = egui::FontDefinitions::default();
        let before = fonts.families[&FontFamily::Proportional].clone();
        match add_cjk_ui_font(&mut fonts, Language::Ko) {
            Some(_) => {
                add_cjk_ui_font(&mut fonts, Language::ZhHans);
                let after = &fonts.families[&FontFamily::Proportional];
                assert_eq!(after[..before.len()], before[..], "egui's fonts first");
                assert_eq!(after.iter().filter(|f| *f == CJK_UI_FONT).count(), 1);
                assert_eq!(after.last().map(String::as_str), Some(CJK_UI_FONT));
            }
            // A system without CJK fonts: nothing changes.
            None => assert_eq!(fonts.families[&FontFamily::Proportional], before),
        }
    }

    #[test]
    fn names_are_interned_and_compare_by_content() {
        let a = FontName::new("Times New Roman");
        let b = FontName::new(" Times New Roman ");
        assert_eq!(a, b);
        assert!(std::ptr::eq(a.as_str(), b.as_str()));
        assert!(FontName::new("symbol").is_symbol());
        assert!(!a.is_symbol());
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, "\"Times New Roman\"");
        assert_eq!(serde_json::from_str::<FontName>(&json).unwrap(), a);
    }

    #[test]
    fn symbol_ascii_is_greek() {
        let shown: String = "pabgDmWS".chars().map(symbol_to_unicode).collect();
        assert_eq!(shown, "παβγΔμΩΣ");
        // Digits, brackets and spaces are themselves; so is anything already Unicode.
        assert_eq!(symbol_to_unicode('2'), '2');
        assert_eq!(symbol_to_unicode('('), '(');
        assert_eq!(symbol_to_unicode('π'), 'π');
        // The private-use copy some programs write.
        assert_eq!(symbol_to_unicode('\u{F070}'), 'π');
        // Latin-1 codes of the upper half.
        assert_eq!(symbol_to_unicode('\u{B0}'), '°');
        assert_eq!(symbol_to_unicode('\u{AE}'), '→');
        assert_eq!(display_text(Some(FontName::symbol()), "a-D"), "α\u{2212}Δ");
        assert_eq!(display_text(Some(FontName::new("Arial")), "a"), "a");
    }

    #[test]
    fn unicode_maps_back_to_symbol_codes() {
        for (code, _) in SYMBOL_TABLE {
            let c = *code as char;
            assert_eq!(unicode_to_symbol(symbol_to_unicode(c)), Some(c), "{c:?}");
        }
        assert_eq!(unicode_to_symbol('µ'), Some('m'));
        assert_eq!(unicode_to_symbol('x'), None);
    }

    #[test]
    fn provider_falls_back_to_egui_fonts_for_unknown_names() {
        let fonts = SystemFontProvider.fonts_for_family(&FontFamily::Name("No Such Font 12345".into()));
        assert!(!fonts.is_empty(), "the default fonts stand in");
        assert!(fonts.iter().all(|f| !f.name.starts_with("system:")));
        assert!(SystemFontProvider.fonts_for_family(&FontFamily::Proportional).is_empty());
    }

    #[test]
    fn standalone_fonts_lay_out_a_named_family() {
        let mut fonts = standalone_fonts();
        let mut view = fonts.with_pixels_per_point(1.0);
        let galley = view.layout_no_wrap(
            "Abc".into(),
            egui::FontId::new(14.0, FontFamily::Name("No Such Font 12345".into())),
            egui::Color32::BLACK,
        );
        assert!(galley.rect.width() > 10.0);
    }
}
