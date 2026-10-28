//! The language of the widgets' labels, menus and messages.
//!
//! One process-wide setting, read by every widget when it draws:
//!
//! ```
//! use chembuider_rs::i18n::{self, Language, Msg};
//!
//! // Once at start-up (or whenever the user picks another language):
//! i18n::set_language(Language::De);
//! assert_eq!(i18n::tr(Msg::ExactMass), "Exakte Masse");
//!
//! // From a BCP-47 tag or a POSIX locale:
//! assert_eq!(Language::from_tag("zh-TW"), Some(Language::ZhHant));
//! assert_eq!(Language::from_tag("pt_BR.UTF-8"), Some(Language::PtBr));
//! # i18n::clear_language();
//! ```
//!
//! # Default
//!
//! Until a host calls [`set_language`], the widgets keep their old behaviour: English, or
//! Japanese where the host asked for it with the older per-widget flags
//! ([`AnalysisPanel::ja`](crate::widget::analysis::AnalysisPanel::ja),
//! [`bond_menu`](crate::widget::bond_menu::bond_menu)'s `ja`). Once a language is set it is
//! used everywhere, those flags included. [`language`] is [`Language::En`] while unset.
//!
//! # Catalog
//!
//! The strings are compiled in: [`Msg`] names each one and [`tr`] looks it up, without
//! allocating. A string missing for a language falls back to English. A few have named
//! placeholders (`{reason}`), filled with [`fill`] (or [`trf`]).
//!
//! The IUPAC names the naming engine makes are English in every language (as IUPAC names
//! are); only the panel's words around them are translated.

use std::fmt::Display;
use std::sync::atomic::{AtomicU8, Ordering};

/// A language the widgets can be shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum Language {
    /// Japanese (`ja`).
    Ja,
    /// English (`en`).
    En,
    /// Simplified Chinese (`zh-Hans`).
    ZhHans,
    /// Traditional Chinese (`zh-Hant`).
    ZhHant,
    /// Spanish (`es`).
    Es,
    /// French (`fr`).
    Fr,
    /// German (`de`).
    De,
    /// Brazilian Portuguese (`pt-BR`).
    PtBr,
    /// Korean (`ko`).
    Ko,
    /// Russian (`ru`).
    Ru,
    /// Italian (`it`).
    It,
}

impl Language {
    /// Every language, in catalog order.
    pub const ALL: [Language; 11] = [
        Language::Ja,
        Language::En,
        Language::ZhHans,
        Language::ZhHant,
        Language::Es,
        Language::Fr,
        Language::De,
        Language::PtBr,
        Language::Ko,
        Language::Ru,
        Language::It,
    ];

    /// The language's BCP-47 tag: `ja`, `en`, `zh-Hans`, `zh-Hant`, `es`, `fr`, `de`,
    /// `pt-BR`, `ko`, `ru`, `it`.
    pub fn tag(self) -> &'static str {
        match self {
            Language::Ja => "ja",
            Language::En => "en",
            Language::ZhHans => "zh-Hans",
            Language::ZhHant => "zh-Hant",
            Language::Es => "es",
            Language::Fr => "fr",
            Language::De => "de",
            Language::PtBr => "pt-BR",
            Language::Ko => "ko",
            Language::Ru => "ru",
            Language::It => "it",
        }
    }

    /// The language's name in itself, for a language menu: `日本語`, `English`, `简体中文`, …
    pub fn native_name(self) -> &'static str {
        match self {
            Language::Ja => "日本語",
            Language::En => "English",
            Language::ZhHans => "简体中文",
            Language::ZhHant => "繁體中文",
            Language::Es => "Español",
            Language::Fr => "Français",
            Language::De => "Deutsch",
            Language::PtBr => "Português (Brasil)",
            Language::Ko => "한국어",
            Language::Ru => "Русский",
            Language::It => "Italiano",
        }
    }

    /// The language a BCP-47 tag (`de-CH`, `zh-Hant-TW`) or POSIX locale (`ja_JP.UTF-8`) is
    /// in, case-insensitively; `None` for a language there is no catalog for.
    ///
    /// Chinese is Traditional for `Hant` and for Taiwan, Hong Kong and Macao, Simplified
    /// otherwise. Every Portuguese is Brazilian Portuguese (the one there is).
    pub fn from_tag(tag: &str) -> Option<Language> {
        // `ja_JP.UTF-8@euro` → `ja-jp`
        let tag = tag
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase()
            .replace('_', "-");
        let mut parts = tag.split('-');
        let language = match parts.next()? {
            "ja" => Language::Ja,
            "en" => Language::En,
            "zh" => {
                let traditional = parts.any(|p| matches!(p, "hant" | "tw" | "hk" | "mo"));
                if traditional {
                    Language::ZhHant
                } else {
                    Language::ZhHans
                }
            }
            "es" => Language::Es,
            "fr" => Language::Fr,
            "de" => Language::De,
            "pt" => Language::PtBr,
            "ko" => Language::Ko,
            "ru" => Language::Ru,
            "it" => Language::It,
            _ => return None,
        };
        Some(language)
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

impl std::str::FromStr for Language {
    type Err = UnknownLanguage;

    /// [`Language::from_tag`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Language::from_tag(s).ok_or_else(|| UnknownLanguage(s.to_owned()))
    }
}

/// A tag [`Language::from_tag`] knows no language for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownLanguage(pub String);

impl std::fmt::Display for UnknownLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no translation for the language {:?}", self.0)
    }
}

impl std::error::Error for UnknownLanguage {}

// ─── The setting ─────────────────────────────────────────────────────────────

/// The language set, as its index plus one; `0` while none is.
static LANGUAGE: AtomicU8 = AtomicU8::new(0);

/// Show every widget in `language` from the next frame on.
pub fn set_language(language: Language) {
    LANGUAGE.store(language as u8 + 1, Ordering::Relaxed);
}

/// Forget the language set: the widgets go back to their defaults (see the
/// [module docs](self#default)).
pub fn clear_language() {
    LANGUAGE.store(0, Ordering::Relaxed);
}

/// The language set with [`set_language`], if any.
pub fn explicit_language() -> Option<Language> {
    match LANGUAGE.load(Ordering::Relaxed) {
        0 => None,
        n => Language::ALL.get(usize::from(n) - 1).copied(),
    }
}

/// The language the widgets are shown in: the one set, or English.
pub fn language() -> Language {
    explicit_language().unwrap_or(Language::En)
}

/// The language set, or `fallback` while none is: what a widget with its own older language
/// flag shows (`fallback` being what the flag says).
pub fn language_or(fallback: Language) -> Language {
    explicit_language().unwrap_or(fallback)
}

/// Message `msg` in the current [`language`].
pub fn tr(msg: Msg) -> &'static str {
    tr_in(language(), msg)
}

/// Message `msg` in `language`, or in English when it has no translation.
pub fn tr_in(language: Language, msg: Msg) -> &'static str {
    pick(&CATALOG[msg as usize], language)
}

/// `language`'s column of a catalog row, or English's when it is empty.
fn pick(row: &[&'static str; 11], language: Language) -> &'static str {
    match row[language as usize] {
        "" => row[Language::En as usize],
        text => text,
    }
}

/// Message `msg` in `language` with its placeholders filled ([`fill`]).
pub fn trf(language: Language, msg: Msg, args: &[(&str, &dyn Display)]) -> String {
    fill(tr_in(language, msg), args)
}

/// `template` with each `{name}` replaced by the value given for `name`; a placeholder with
/// no value is left as it is.
pub fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            args.iter()
                .find(|(n, _)| *n == name)
                .map(|(_, v)| (close, *v))
        });
        match value {
            Some((close, v)) => {
                use std::fmt::Write;
                let _ = write!(out, "{v}");
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The names of the placeholders in `template` (`{reason}` → `reason`), in order.
pub fn placeholders(template: &str) -> Vec<&str> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        names.push(&after[..close]);
        rest = &after[close + 1..];
    }
    names
}

// ─── The catalog ─────────────────────────────────────────────────────────────

macro_rules! catalog {
    ($(
        $(#[$meta:meta])*
        $key:ident {
            ja: $ja:expr, en: $en:expr, zh_hans: $zh_hans:expr, zh_hant: $zh_hant:expr,
            es: $es:expr, fr: $fr:expr, de: $de:expr, pt_br: $pt_br:expr,
            ko: $ko:expr, ru: $ru:expr, it: $it:expr $(,)?
        }
    )*) => {
        /// A message the widgets show, for [`tr`].
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Msg {
            $($(#[$meta])* $key,)*
        }

        impl Msg {
            /// Every message.
            pub const ALL: &'static [Msg] = &[$(Msg::$key,)*];
        }

        /// One row per [`Msg`], one column per [`Language`] (in [`Language::ALL`]'s order).
        const CATALOG: &[[&str; 11]] = &[
            $([$ja, $en, $zh_hans, $zh_hant, $es, $fr, $de, $pt_br, $ko, $ru, $it],)*
        ];
    };
}

catalog! {
    // ── Analysis panel ──
    /// The analysis panel's heading.
    Analysis {
        ja: "解析", en: "Analysis", zh_hans: "分析", zh_hant: "分析",
        es: "Análisis", fr: "Analyse", de: "Analyse", pt_br: "Análise",
        ko: "분석", ru: "Анализ", it: "Analisi",
    }
    /// What is analysed, with nothing selected.
    WholeDrawing {
        ja: "図全体", en: "Whole drawing", zh_hans: "整个绘图", zh_hant: "整個繪圖",
        es: "Todo el dibujo", fr: "Dessin entier", de: "Gesamte Zeichnung", pt_br: "Desenho inteiro",
        ko: "전체 그림", ru: "Весь рисунок", it: "Intero disegno",
    }
    /// What is analysed, with atoms selected.
    SelectedStructures {
        ja: "選択した構造", en: "Selected structures", zh_hans: "所选结构", zh_hant: "所選結構",
        es: "Estructuras seleccionadas", fr: "Structures sélectionnées", de: "Ausgewählte Strukturen",
        pt_br: "Estruturas selecionadas", ko: "선택한 구조", ru: "Выбранные структуры",
        it: "Strutture selezionate",
    }
    /// The panel of an empty drawing.
    NothingDrawn {
        ja: "まだ何も描かれていません", en: "Nothing drawn yet.", zh_hans: "尚未绘制任何内容。",
        zh_hant: "尚未繪製任何內容。", es: "Aún no hay nada dibujado.", fr: "Rien n’est encore dessiné.",
        de: "Noch nichts gezeichnet.", pt_br: "Nada desenhado ainda.", ko: "아직 그린 것이 없습니다.",
        ru: "Пока ничего не нарисовано.", it: "Ancora nulla di disegnato.",
    }
    /// The button copying the analysis (after a 📋).
    Copy {
        ja: "コピー", en: "Copy", zh_hans: "复制", zh_hant: "複製",
        es: "Copiar", fr: "Copier", de: "Kopieren", pt_br: "Copiar",
        ko: "복사", ru: "Копировать", it: "Copia",
    }
    /// The copy button's tooltip.
    CopyAnalysis {
        ja: "解析結果をテキストでコピー", en: "Copy the analysis as text",
        zh_hans: "将分析结果复制为文本", zh_hant: "將分析結果複製為文字",
        es: "Copiar el análisis como texto", fr: "Copier l’analyse sous forme de texte",
        de: "Analyse als Text kopieren", pt_br: "Copiar a análise como texto",
        ko: "분석 결과를 텍스트로 복사", ru: "Копировать анализ как текст",
        it: "Copia l’analisi come testo",
    }
    /// The ion (the m/z species).
    Ion {
        ja: "イオン", en: "Ion", zh_hans: "离子", zh_hant: "離子",
        es: "Ion", fr: "Ion", de: "Ion", pt_br: "Íon",
        ko: "이온", ru: "Ион", it: "Ione",
    }
    /// The adduct choice for the neutral molecule.
    Neutral {
        ja: "中性 (M)", en: "Neutral (M)", zh_hans: "中性 (M)", zh_hant: "中性 (M)",
        es: "Neutra (M)", fr: "Neutre (M)", de: "Neutral (M)", pt_br: "Neutra (M)",
        ko: "중성 (M)", ru: "Нейтральная (M)", it: "Neutra (M)",
    }
    /// The adduct choice for a species typed in.
    CustomAdduct {
        ja: "カスタム…", en: "Custom…", zh_hans: "自定义…", zh_hant: "自訂…",
        es: "Personalizado…", fr: "Personnalisé…", de: "Benutzerdefiniert…", pt_br: "Personalizado…",
        ko: "사용자 지정…", ru: "Другой…", it: "Personalizzato…",
    }
    /// The adduct (the species added to or taken from the molecule).
    Adduct {
        ja: "付加イオン", en: "Adduct", zh_hans: "加合离子", zh_hant: "加合離子",
        es: "Aducto", fr: "Adduit", de: "Addukt", pt_br: "Aduto",
        ko: "부가 이온", ru: "Аддукт", it: "Addotto",
    }
    /// A custom adduct's name.
    AdductName {
        ja: "名前", en: "Name", zh_hans: "名称", zh_hant: "名稱",
        es: "Nombre", fr: "Nom", de: "Name", pt_br: "Nome",
        ko: "이름", ru: "Название", it: "Nome",
    }
    /// A custom adduct's formula.
    AdductFormula {
        ja: "組成式", en: "Formula", zh_hans: "化学式", zh_hant: "化學式",
        es: "Fórmula", fr: "Formule", de: "Formel", pt_br: "Fórmula",
        ko: "화학식", ru: "Формула", it: "Formula",
    }
    /// A custom adduct's charge per unit.
    ChargePerUnit {
        ja: "1 単位の電荷", en: "Charge / unit", zh_hans: "单位电荷", zh_hant: "單位電荷",
        es: "Carga por unidad", fr: "Charge par unité", de: "Ladung pro Einheit", pt_br: "Carga por unidade",
        ko: "단위당 전하", ru: "Заряд на единицу", it: "Carica per unità",
    }
    /// The ion's net charge.
    Charge {
        ja: "電荷", en: "Charge", zh_hans: "电荷", zh_hant: "電荷",
        es: "Carga", fr: "Charge", de: "Ladung", pt_br: "Carga",
        ko: "전하", ru: "Заряд", it: "Carica",
    }
    /// How many molecules the ion has (`2` for `[2M+Na]+`).
    Molecules {
        ja: "分子数", en: "Molecules", zh_hans: "分子数", zh_hant: "分子數",
        es: "Moléculas", fr: "Molécules", de: "Moleküle", pt_br: "Moléculas",
        ko: "분자 수", ru: "Число молекул", it: "Molecole",
    }
    /// A value that cannot be given for unknown elements. `{elements}`: their symbols.
    UnknownElements {
        ja: "不明: {elements}", en: "unknown: {elements}", zh_hans: "未知：{elements}",
        zh_hant: "未知：{elements}", es: "desconocido: {elements}", fr: "inconnu : {elements}",
        de: "unbekannt: {elements}", pt_br: "desconhecido: {elements}", ko: "알 수 없음: {elements}",
        ru: "неизвестно: {elements}", it: "sconosciuto: {elements}",
    }
    /// The (molecular) formula row.
    ChemicalFormula {
        ja: "分子式", en: "Chemical formula", zh_hans: "分子式", zh_hant: "分子式",
        es: "Fórmula molecular", fr: "Formule brute", de: "Summenformel", pt_br: "Fórmula molecular",
        ko: "분자식", ru: "Молекулярная формула", it: "Formula molecolare",
    }
    /// Under a polymer's formula: the values are those of one repeat unit.
    ValuesForN1 {
        ja: "数値は n = 1 の値", en: "Values for n = 1", zh_hans: "数值为 n = 1 时的值",
        zh_hant: "數值為 n = 1 時的值", es: "Valores para n = 1", fr: "Valeurs pour n = 1",
        de: "Werte für n = 1", pt_br: "Valores para n = 1", ko: "n = 1일 때의 값",
        ru: "Значения для n = 1", it: "Valori per n = 1",
    }
    /// The monoisotopic mass row.
    ExactMass {
        ja: "精密質量", en: "Exact mass", zh_hans: "精确质量", zh_hant: "精確質量",
        es: "Masa exacta", fr: "Masse exacte", de: "Exakte Masse", pt_br: "Massa exata",
        ko: "정확한 질량", ru: "Точная масса", it: "Massa esatta",
    }
    /// The (average) molecular weight row.
    MolecularWeight {
        ja: "分子量", en: "Molecular weight", zh_hans: "分子量", zh_hant: "分子量",
        es: "Peso molecular", fr: "Masse molaire", de: "Molekulargewicht", pt_br: "Massa molecular",
        ko: "분자량", ru: "Молекулярная масса", it: "Peso molecolare",
    }
    /// The elemental analysis (mass percentages) row.
    ElementalAnalysis {
        ja: "元素分析", en: "Elemental analysis", zh_hans: "元素分析", zh_hant: "元素分析",
        es: "Análisis elemental", fr: "Analyse élémentaire", de: "Elementaranalyse",
        pt_br: "Análise elementar", ko: "원소 분석", ru: "Элементный анализ", it: "Analisi elementare",
    }
    /// The IUPAC name row.
    IupacName {
        ja: "IUPAC 名", en: "IUPAC name", zh_hans: "IUPAC 名称", zh_hant: "IUPAC 名稱",
        es: "Nombre IUPAC", fr: "Nom IUPAC", de: "IUPAC-Name", pt_br: "Nome IUPAC",
        ko: "IUPAC 이름", ru: "Название по IUPAC", it: "Nome IUPAC",
    }
    /// The tooltip of the button copying the IUPAC name.
    CopyName {
        ja: "名前をコピー", en: "Copy the name", zh_hans: "复制名称", zh_hant: "複製名稱",
        es: "Copiar el nombre", fr: "Copier le nom", de: "Namen kopieren", pt_br: "Copiar o nome",
        ko: "이름 복사", ru: "Копировать название", it: "Copia il nome",
    }
    /// Under an IUPAC name of a drawing with stereo bonds.
    StereoNotIncluded {
        ja: "（立体表記は含まれません）", en: "(stereodescriptors not included)",
        zh_hans: "（不含立体描述符）", zh_hant: "（不含立體描述符）",
        es: "(sin descriptores estereoquímicos)", fr: "(descripteurs stéréochimiques non inclus)",
        de: "(ohne Stereodeskriptoren)", pt_br: "(sem descritores estereoquímicos)",
        ko: "(입체 표기는 포함되지 않음)", ru: "(без стереодескрипторов)",
        it: "(descrittori stereochimici non inclusi)",
    }
    /// Instead of an IUPAC name the engine cannot make. `{reason}`: why (in English).
    Unsupported {
        ja: "未対応: {reason}", en: "unsupported: {reason}", zh_hans: "不支持：{reason}",
        zh_hant: "不支援：{reason}", es: "no compatible: {reason}", fr: "non pris en charge : {reason}",
        de: "nicht unterstützt: {reason}", pt_br: "não suportado: {reason}", ko: "지원되지 않음: {reason}",
        ru: "не поддерживается: {reason}", it: "non supportato: {reason}",
    }

    // ── Bond menu ──
    /// Bond order: single (a submenu of line styles).
    BondSingle {
        ja: "一重結合", en: "Single", zh_hans: "单键", zh_hant: "單鍵",
        es: "Enlace sencillo", fr: "Liaison simple", de: "Einfachbindung", pt_br: "Ligação simples",
        ko: "단일 결합", ru: "Одинарная связь", it: "Legame singolo",
    }
    /// Bond order: double.
    BondDouble {
        ja: "二重結合", en: "Double", zh_hans: "双键", zh_hant: "雙鍵",
        es: "Enlace doble", fr: "Liaison double", de: "Doppelbindung", pt_br: "Ligação dupla",
        ko: "이중 결합", ru: "Двойная связь", it: "Legame doppio",
    }
    /// Bond order: triple.
    BondTriple {
        ja: "三重結合", en: "Triple", zh_hans: "三键", zh_hant: "三鍵",
        es: "Enlace triple", fr: "Liaison triple", de: "Dreifachbindung", pt_br: "Ligação tripla",
        ko: "삼중 결합", ru: "Тройная связь", it: "Legame triplo",
    }
    /// Bond order: quadruple.
    BondQuadruple {
        ja: "四重結合", en: "Quadruple", zh_hans: "四重键", zh_hant: "四重鍵",
        es: "Enlace cuádruple", fr: "Liaison quadruple", de: "Vierfachbindung", pt_br: "Ligação quádrupla",
        ko: "사중 결합", ru: "Четверная связь", it: "Legame quadruplo",
    }
    /// Single-bond style: a plain line.
    StylePlain {
        ja: "実線", en: "Plain", zh_hans: "实线", zh_hant: "實線",
        es: "Normal", fr: "Normale", de: "Normal", pt_br: "Normal",
        ko: "실선", ru: "Обычная", it: "Normale",
    }
    /// Single-bond style: dashed.
    StyleDashed {
        ja: "破線", en: "Dashed", zh_hans: "虚线", zh_hant: "虛線",
        es: "Discontinua", fr: "Pointillée", de: "Gestrichelt", pt_br: "Tracejada",
        ko: "파선", ru: "Пунктирная", it: "Tratteggiato",
    }
    /// Single-bond style: hashed.
    StyleHashed {
        ja: "ハッシュ", en: "Hashed", zh_hans: "阴影线", zh_hant: "陰影線",
        es: "Rayada", fr: "Hachurée", de: "Schraffiert", pt_br: "Hachurada",
        ko: "해시", ru: "Штрихованная", it: "A righe",
    }
    /// Single-bond style: hashed wedge.
    StyleHashedWedged {
        ja: "ハッシュくさび", en: "Hashed Wedged", zh_hans: "阴影楔形", zh_hant: "陰影楔形",
        es: "Cuña rayada", fr: "Coin hachuré", de: "Schraffierter Keil", pt_br: "Cunha hachurada",
        ko: "해시 쐐기", ru: "Штрихованный клин", it: "Cuneo a righe",
    }
    /// Single-bond style: bold.
    StyleBold {
        ja: "太線", en: "Bold", zh_hans: "粗线", zh_hant: "粗線",
        es: "Gruesa", fr: "Épaisse", de: "Fett", pt_br: "Espessa",
        ko: "굵은 선", ru: "Жирная", it: "Spesso",
    }
    /// Single-bond style: bold (solid) wedge.
    StyleBoldWedged {
        ja: "太線くさび", en: "Bold Wedged", zh_hans: "实心楔形", zh_hant: "實心楔形",
        es: "Cuña sólida", fr: "Coin plein", de: "Fetter Keil", pt_br: "Cunha cheia",
        ko: "굵은 쐐기", ru: "Жирный клин", it: "Cuneo pieno",
    }
    /// Single-bond style: hollow wedge.
    StyleHollowWedged {
        ja: "中抜きくさび", en: "Hollow Wedged", zh_hans: "空心楔形", zh_hant: "空心楔形",
        es: "Cuña hueca", fr: "Coin creux", de: "Hohler Keil", pt_br: "Cunha vazada",
        ko: "속이 빈 쐐기", ru: "Полый клин", it: "Cuneo vuoto",
    }
    /// Single-bond style: dative (an arrow).
    StyleDative {
        ja: "配位結合", en: "Dative", zh_hans: "配位键", zh_hant: "配位鍵",
        es: "Dativa", fr: "Dative", de: "Koordinativ", pt_br: "Dativa",
        ko: "배위 결합", ru: "Донорно-акцепторная", it: "Dativo",
    }
    /// Single-bond style: wavy.
    StyleWavy {
        ja: "波線", en: "Wavy", zh_hans: "波浪线", zh_hant: "波浪線",
        es: "Ondulada", fr: "Ondulée", de: "Wellenlinie", pt_br: "Ondulada",
        ko: "물결선", ru: "Волнистая", it: "Ondulato",
    }
    /// The submenu placing a double bond's second line.
    BondPosition {
        ja: "二重結合の位置", en: "Bond Position", zh_hans: "双键位置", zh_hant: "雙鍵位置",
        es: "Posición del doble enlace", fr: "Position de la double liaison",
        de: "Lage der Doppelbindung", pt_br: "Posição da ligação dupla", ko: "이중 결합 위치",
        ru: "Положение двойной связи", it: "Posizione del doppio legame",
    }
    /// Double-bond position: chosen automatically.
    PositionAuto {
        ja: "自動", en: "Auto", zh_hans: "自动", zh_hant: "自動",
        es: "Automática", fr: "Automatique", de: "Automatisch", pt_br: "Automática",
        ko: "자동", ru: "Авто", it: "Automatica",
    }
    /// Double-bond position: left.
    PositionLeft {
        ja: "左", en: "Left", zh_hans: "左", zh_hant: "左",
        es: "Izquierda", fr: "Gauche", de: "Links", pt_br: "Esquerda",
        ko: "왼쪽", ru: "Слева", it: "Sinistra",
    }
    /// Double-bond position: centred.
    PositionCenter {
        ja: "中央", en: "Center", zh_hans: "居中", zh_hant: "置中",
        es: "Centro", fr: "Centre", de: "Mitte", pt_br: "Centro",
        ko: "가운데", ru: "По центру", it: "Centro",
    }
    /// Double-bond position: right.
    PositionRight {
        ja: "右", en: "Right", zh_hans: "右", zh_hant: "右",
        es: "Derecha", fr: "Droite", de: "Rechts", pt_br: "Direita",
        ko: "오른쪽", ru: "Справа", it: "Destra",
    }
    /// Remove the bond.
    DeleteBond {
        ja: "結合を削除", en: "Delete Bond", zh_hans: "删除键", zh_hant: "刪除鍵",
        es: "Eliminar enlace", fr: "Supprimer la liaison", de: "Bindung löschen", pt_br: "Excluir ligação",
        ko: "결합 삭제", ru: "Удалить связь", it: "Elimina legame",
    }
    /// A context menu opened away from any bond.
    RightClickBond {
        ja: "結合を右クリックすると変更できます", en: "Right-click a bond to change it.",
        zh_hans: "右键单击键即可更改。", zh_hant: "在鍵上按右鍵即可變更。",
        es: "Haga clic derecho en un enlace para cambiarlo.",
        fr: "Cliquez droit sur une liaison pour la modifier.",
        de: "Klicken Sie mit der rechten Maustaste auf eine Bindung, um sie zu ändern.",
        pt_br: "Clique com o botão direito em uma ligação para alterá-la.",
        ko: "결합을 마우스 오른쪽 버튼으로 클릭하면 변경할 수 있습니다.",
        ru: "Щёлкните связь правой кнопкой мыши, чтобы изменить её.",
        it: "Fai clic con il tasto destro su un legame per modificarlo.",
    }

    // ── Tools ──
    /// The Select tool.
    ToolSelect {
        ja: "選択", en: "Select", zh_hans: "选择", zh_hant: "選取",
        es: "Seleccionar", fr: "Sélection", de: "Auswahl", pt_br: "Selecionar",
        ko: "선택", ru: "Выделение", it: "Seleziona",
    }
    /// The Bond tool.
    ToolBond {
        ja: "結合", en: "Bond", zh_hans: "键", zh_hant: "鍵",
        es: "Enlace", fr: "Liaison", de: "Bindung", pt_br: "Ligação",
        ko: "결합", ru: "Связь", it: "Legame",
    }
    /// The Eraser tool.
    ToolEraser {
        ja: "消しゴム", en: "Eraser", zh_hans: "橡皮擦", zh_hant: "橡皮擦",
        es: "Borrador", fr: "Gomme", de: "Radiergummi", pt_br: "Borracha",
        ko: "지우개", ru: "Ластик", it: "Gomma",
    }
    /// New bonds drawn plain.
    NewBondPlain {
        ja: "通常", en: "Plain", zh_hans: "普通", zh_hant: "普通",
        es: "Normal", fr: "Normale", de: "Normal", pt_br: "Normal",
        ko: "일반", ru: "Обычная", it: "Normale",
    }
    /// New bonds drawn as wedges.
    NewBondWedge {
        ja: "くさび", en: "Wedge", zh_hans: "楔形", zh_hant: "楔形",
        es: "Cuña", fr: "Coin", de: "Keil", pt_br: "Cunha",
        ko: "쐐기", ru: "Клин", it: "Cuneo",
    }
    /// New bonds drawn as hashed wedges.
    NewBondHash {
        ja: "破線くさび", en: "Hash", zh_hans: "阴影楔形", zh_hant: "陰影楔形",
        es: "Cuña rayada", fr: "Coin hachuré", de: "Schraffierter Keil", pt_br: "Cunha hachurada",
        ko: "해시 쐐기", ru: "Штрихованный клин", it: "Cuneo a righe",
    }
    /// Undo the last edit.
    Undo {
        ja: "元に戻す", en: "Undo", zh_hans: "撤销", zh_hant: "復原",
        es: "Deshacer", fr: "Annuler", de: "Rückgängig", pt_br: "Desfazer",
        ko: "실행 취소", ru: "Отменить", it: "Annulla",
    }
    /// Redo the edit undone.
    Redo {
        ja: "やり直し", en: "Redo", zh_hans: "重做", zh_hant: "重做",
        es: "Rehacer", fr: "Rétablir", de: "Wiederholen", pt_br: "Refazer",
        ko: "다시 실행", ru: "Повторить", it: "Ripeti",
    }
    /// Paste from the clipboard.
    Paste {
        ja: "貼り付け", en: "Paste", zh_hans: "粘贴", zh_hant: "貼上",
        es: "Pegar", fr: "Coller", de: "Einfügen", pt_br: "Colar",
        ko: "붙여넣기", ru: "Вставить", it: "Incolla",
    }
    /// Tidy the structure's 2D layout.
    CleanUp {
        ja: "整形", en: "Clean Up", zh_hans: "整理结构", zh_hant: "整理結構",
        es: "Limpiar estructura", fr: "Nettoyer la structure", de: "Struktur bereinigen",
        pt_br: "Limpar estrutura", ko: "구조 정리", ru: "Упорядочить структуру", it: "Riordina struttura",
    }
    /// Tidy a reaction scheme around its arrows (the arrows stay where they are).
    CleanUpReaction {
        ja: "反応式を整形", en: "Clean Up Reaction", zh_hans: "整理反应式", zh_hant: "整理反應式",
        es: "Limpiar reacción", fr: "Nettoyer la réaction", de: "Reaktion bereinigen",
        pt_br: "Limpar reação", ko: "반응식 정리", ru: "Упорядочить реакцию", it: "Riordina reazione",
    }

    // ── ibisOffice editor ──
    /// A ChemDraw object's other objects kept untouched. `{n}`: how many.
    OtherObjectsKept {
        ja: "構造式以外の {n} 個のオブジェクト（矢印・文字など）はそのまま保持されます",
        en: "{n} other objects (arrows, text, …) are kept as they are",
        zh_hans: "结构式以外的 {n} 个对象（箭头、文字等）将保持原样",
        zh_hant: "結構式以外的 {n} 個物件（箭頭、文字等）將保持原樣",
        es: "Los otros {n} objetos (flechas, texto…) se conservan tal cual",
        fr: "Les {n} autres objets (flèches, texte…) sont conservés tels quels",
        de: "{n} weitere Objekte (Pfeile, Text …) bleiben unverändert erhalten",
        pt_br: "Os outros {n} objetos (setas, texto…) são mantidos como estão",
        ko: "구조식 이외의 객체 {n}개(화살표, 텍스트 등)는 그대로 유지됩니다",
        ru: "Прочие объекты ({n}: стрелки, текст…) сохраняются без изменений",
        it: "Gli altri {n} oggetti (frecce, testo…) vengono mantenuti invariati",
    }
    /// Data that is no ChemDraw drawing.
    NotCdx {
        ja: "ChemDraw (CDX) の図として読めません", en: "Not a ChemDraw (CDX) drawing",
        zh_hans: "不是 ChemDraw (CDX) 图形", zh_hant: "不是 ChemDraw (CDX) 圖形",
        es: "No es un dibujo de ChemDraw (CDX)", fr: "Ce n’est pas un dessin ChemDraw (CDX)",
        de: "Keine ChemDraw-Zeichnung (CDX)", pt_br: "Não é um desenho do ChemDraw (CDX)",
        ko: "ChemDraw(CDX) 그림이 아닙니다", ru: "Это не рисунок ChemDraw (CDX)",
        it: "Non è un disegno ChemDraw (CDX)",
    }
    /// A file that cannot be read. `{error}`: why.
    CannotReadFile {
        ja: "ファイルを読めません: {error}", en: "Cannot read the file: {error}",
        zh_hans: "无法读取文件：{error}", zh_hant: "無法讀取檔案：{error}",
        es: "No se puede leer el archivo: {error}", fr: "Impossible de lire le fichier : {error}",
        de: "Datei kann nicht gelesen werden: {error}", pt_br: "Não é possível ler o arquivo: {error}",
        ko: "파일을 읽을 수 없습니다: {error}", ru: "Не удаётся прочитать файл: {error}",
        it: "Impossibile leggere il file: {error}",
    }
    /// A file another program changed, so no longer written.
    ChangedOutside {
        ja: "ファイルが別のアプリで変更されたため上書きしません",
        en: "The file was changed by another program and is not written over",
        zh_hans: "文件已被其他程序修改，因此不会覆盖", zh_hant: "檔案已被其他程式修改，因此不會覆寫",
        es: "Otro programa modificó el archivo, por lo que no se sobrescribe",
        fr: "Le fichier a été modifié par un autre programme et n’est pas écrasé",
        de: "Die Datei wurde von einem anderen Programm geändert und wird nicht überschrieben",
        pt_br: "O arquivo foi alterado por outro programa e não será sobrescrito",
        ko: "다른 프로그램이 파일을 변경했으므로 덮어쓰지 않습니다",
        ru: "Файл изменён другой программой и не будет перезаписан",
        it: "Il file è stato modificato da un altro programma e non viene sovrascritto",
    }
    /// Edits go back to the host document as they are made.
    EditsSentBack {
        ja: "編集は自動的に反映されます", en: "Edits are sent back automatically",
        zh_hans: "编辑会自动回传", zh_hant: "編輯會自動回傳",
        es: "Los cambios se envían automáticamente", fr: "Les modifications sont renvoyées automatiquement",
        de: "Änderungen werden automatisch übernommen", pt_br: "As edições são enviadas de volta automaticamente",
        ko: "편집 내용이 자동으로 반영됩니다", ru: "Изменения передаются автоматически",
        it: "Le modifiche vengono rinviate automaticamente",
    }
    /// Edits do not go back to the host document.
    EditsNotSentBack {
        ja: "編集は反映されません", en: "Edits are not sent back",
        zh_hans: "编辑不会回传", zh_hant: "編輯不會回傳",
        es: "Los cambios no se envían", fr: "Les modifications ne sont pas renvoyées",
        de: "Änderungen werden nicht übernommen", pt_br: "As edições não são enviadas de volta",
        ko: "편집 내용이 반영되지 않습니다", ru: "Изменения не передаются",
        it: "Le modifiche non vengono rinviate",
    }
    /// Edits are written to the file as they are made.
    EditsSaved {
        ja: "編集は自動的に保存されます", en: "Edits are saved automatically",
        zh_hans: "编辑会自动保存", zh_hant: "編輯會自動儲存",
        es: "Los cambios se guardan automáticamente", fr: "Les modifications sont enregistrées automatiquement",
        de: "Änderungen werden automatisch gespeichert", pt_br: "As edições são salvas automaticamente",
        ko: "편집 내용이 자동으로 저장됩니다", ru: "Изменения сохраняются автоматически",
        it: "Le modifiche vengono salvate automaticamente",
    }
    /// Edits are not written to the file.
    EditsNotSaved {
        ja: "編集は保存されません", en: "Edits are not saved",
        zh_hans: "编辑不会保存", zh_hant: "編輯不會儲存",
        es: "Los cambios no se guardan", fr: "Les modifications ne sont pas enregistrées",
        de: "Änderungen werden nicht gespeichert", pt_br: "As edições não são salvas",
        ko: "편집 내용이 저장되지 않습니다", ru: "Изменения не сохраняются",
        it: "Le modifiche non vengono salvate",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_every_message() {
        assert_eq!(CATALOG.len(), Msg::ALL.len());
        for &msg in Msg::ALL {
            for language in Language::ALL {
                let text = CATALOG[msg as usize][language as usize];
                assert!(!text.trim().is_empty(), "{msg:?} has no {language} text");
            }
        }
    }

    #[test]
    fn placeholders_are_the_same_in_every_language() {
        for &msg in Msg::ALL {
            let mut english = placeholders(tr_in(Language::En, msg));
            english.sort_unstable();
            for language in Language::ALL {
                let mut names = placeholders(tr_in(language, msg));
                names.sort_unstable();
                assert_eq!(names, english, "{msg:?} in {language}");
            }
        }
    }

    #[test]
    fn languages_are_in_catalog_order() {
        for (i, language) in Language::ALL.into_iter().enumerate() {
            assert_eq!(language as usize, i);
            assert_eq!(Language::from_tag(language.tag()), Some(language));
            assert_eq!(language.tag().parse::<Language>(), Ok(language));
        }
    }

    #[test]
    fn tags_and_locales_are_parsed() {
        assert_eq!(Language::from_tag("ja"), Some(Language::Ja));
        assert_eq!(Language::from_tag("ja_JP.UTF-8"), Some(Language::Ja));
        assert_eq!(Language::from_tag("EN-us"), Some(Language::En));
        assert_eq!(Language::from_tag("zh"), Some(Language::ZhHans));
        assert_eq!(Language::from_tag("zh-CN"), Some(Language::ZhHans));
        assert_eq!(Language::from_tag("zh-Hans-SG"), Some(Language::ZhHans));
        assert_eq!(Language::from_tag("zh-Hant"), Some(Language::ZhHant));
        assert_eq!(Language::from_tag("zh_TW"), Some(Language::ZhHant));
        assert_eq!(Language::from_tag("zh-HK"), Some(Language::ZhHant));
        assert_eq!(Language::from_tag("pt-BR"), Some(Language::PtBr));
        assert_eq!(Language::from_tag("pt"), Some(Language::PtBr));
        assert_eq!(Language::from_tag("de_CH@euro"), Some(Language::De));
        assert_eq!(Language::from_tag("ko-KR"), Some(Language::Ko));
        assert_eq!(Language::from_tag("nl"), None);
        assert_eq!(Language::from_tag(""), None);
        assert!("xx".parse::<Language>().is_err());
    }

    #[test]
    fn a_missing_translation_falls_back_to_english() {
        let mut row = [""; 11];
        row[Language::En as usize] = "English";
        row[Language::Ja as usize] = "日本語";
        assert_eq!(pick(&row, Language::Ja), "日本語");
        assert_eq!(pick(&row, Language::Ko), "English");
    }

    #[test]
    fn lookups_translate() {
        assert_eq!(tr_in(Language::Ja, Msg::ChemicalFormula), "分子式");
        assert_eq!(tr_in(Language::Fr, Msg::ChemicalFormula), "Formule brute");
        assert_eq!(tr_in(Language::De, Msg::ChemicalFormula), "Summenformel");
        assert_eq!(tr_in(Language::Ko, Msg::ElementalAnalysis), "원소 분석");
        assert_eq!(tr_in(Language::En, Msg::ExactMass), "Exact mass");
    }

    #[test]
    fn placeholders_are_filled() {
        assert_eq!(
            trf(
                Language::Ja,
                Msg::Unsupported,
                &[("reason", &"bridged rings")]
            ),
            "未対応: bridged rings"
        );
        assert_eq!(
            trf(Language::En, Msg::OtherObjectsKept, &[("n", &3)]),
            "3 other objects (arrows, text, …) are kept as they are"
        );
        assert_eq!(fill("{a} {b} {a}", &[("a", &1)]), "1 {b} 1");
        assert_eq!(fill("{ unclosed", &[]), "{ unclosed");
        assert_eq!(placeholders("x {a} y {b}"), ["a", "b"]);
    }
}
