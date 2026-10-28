//! Name fragments: numerical stems, multiplying prefixes, enclosing marks, alphanumerical keys.

/// The largest number [`numerical_term`] spells: the terms run up to "nonalia" (9000).
pub(crate) const MAX_NUMERAL: usize = 9999;

/// The numerical term for `n` (P-14.2.1, Table 1.4), 5 ≤ n ≤ [`MAX_NUMERAL`]: "penta",
/// "undeca", "henicosa", "docosa", "hentriaconta", "hecta", "pentacta", "kilia",
/// "heptahexacontapentactadilia" (2567). The digits are cited from the units up; in
/// combination 1 is "hen" (but 11 is "undeca") and 2 is "do" (but 200 "dicta", 2000 "dilia"),
/// and the "i" of "icosa" is elided after a vowel ("docosa", "tricosa").
fn numerical_term(n: usize) -> Option<String> {
    const UNITS: [&str; 10] = [
        "", "hen", "do", "tri", "tetra", "penta", "hexa", "hepta", "octa", "nona",
    ];
    const TENS: [&str; 10] = [
        "",
        "deca",
        "icosa",
        "triaconta",
        "tetraconta",
        "pentaconta",
        "hexaconta",
        "heptaconta",
        "octaconta",
        "nonaconta",
    ];
    const HUNDREDS: [&str; 10] = [
        "", "hecta", "dicta", "tricta", "tetracta", "pentacta", "hexacta", "heptacta", "octacta",
        "nonacta",
    ];
    const THOUSANDS: [&str; 10] = [
        "", "kilia", "dilia", "trilia", "tetralia", "pentalia", "hexalia", "heptalia", "octalia",
        "nonalia",
    ];
    if !(5..=MAX_NUMERAL).contains(&n) {
        return None;
    }
    let (u, t, h, k) = (n % 10, n / 10 % 10, n / 100 % 10, n / 1000);
    let low = if (t, u) == (1, 1) {
        "undeca".to_string()
    } else {
        let unit = UNITS[u];
        let mut tens = TENS[t];
        if t == 2 && unit.ends_with(['a', 'i', 'o']) {
            tens = &tens[1..];
        }
        format!("{unit}{tens}")
    };
    Some(format!("{low}{}{}", HUNDREDS[h], THOUSANDS[k]))
}

/// The stem of the saturated unbranched hydrocarbon with `n` carbons ("meth", "eth", …,
/// "icos", "henicos", "triacont", "hect", "pentact", "kili"), for 1 ≤ n ≤ [`MAX_NUMERAL`]:
/// the retained names up to butane, then the numerical term without its final "a" (P-21.2.1).
pub(crate) fn stem(n: usize) -> Option<String> {
    match n {
        1..=4 => Some(["meth", "eth", "prop", "but"][n - 1].to_string()),
        _ => numerical_term(n).map(|mut t| {
            t.pop();
            t
        }),
    }
}

/// The multiplying prefix for `n` identical simple features: "", "di", "tri", …
pub(crate) fn multiplier(n: usize) -> String {
    match n {
        0 | 1 => String::new(),
        2 => "di".into(),
        3 => "tri".into(),
        4 => "tetra".into(),
        _ => numerical_term(n).unwrap_or_else(|| n.to_string()),
    }
}

/// The multiplying prefix for `n` identical compound features: "bis", "tris", "tetrakis", …
pub(crate) fn multiplier_complex(n: usize) -> String {
    match n {
        0 | 1 => String::new(),
        2 => "bis".into(),
        3 => "tris".into(),
        _ => format!("{}kis", multiplier(n)),
    }
}

/// Enclose `text` (which contains enclosing marks nested `depth` deep) in the next level of
/// marks: ( ), then [ ], then { }, then ( ) again.
pub(crate) fn enclose(text: &str, depth: usize) -> String {
    let (open, close) = match depth % 3 {
        0 => ('(', ')'),
        1 => ('[', ']'),
        _ => ('{', '}'),
    };
    format!("{open}{text}{close}")
}

/// The alphanumerical-ordering key of a prefix: its letters, without italic parts ("tert-",
/// "sec-", "N-" locants), locants and punctuation.
pub(crate) fn alpha_key(text: &str) -> String {
    let t = text.replace("tert-", "").replace("sec-", "");
    t.chars().filter(|c| c.is_ascii_lowercase()).collect()
}

/// Whether a suffix starting with this text makes the "e" of "-ane"/"-ene"/"-yne" drop.
pub(crate) fn starts_with_vowel(s: &str) -> bool {
    s.starts_with(['a', 'e', 'i', 'o', 'u', 'y'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems() {
        let expect = [
            (1, "meth"),
            (4, "but"),
            (10, "dec"),
            (11, "undec"),
            (12, "dodec"),
            (13, "tridec"),
            (20, "icos"),
            (21, "henicos"),
            (22, "docos"),
            (23, "tricos"),
            (24, "tetracos"),
            (30, "triacont"),
            (31, "hentriacont"),
            (32, "dotriacont"),
            (45, "pentatetracont"),
        ];
        for (n, s) in expect {
            assert_eq!(stem(n).unwrap(), s, "{n}");
        }
        assert_eq!(stem(0), None);
        assert_eq!(stem(10000), None);
    }

    /// The table of numerical terms of Wikipedia (ja) "IUPAC命名法" and IUPAC 2013 Table 1.4,
    /// row by row, with the other examples P-14.2.1.2 gives. Wikipedia's 1 and 2 ("mono"/"hen",
    /// "di"/"do") are the forms used alone and in combination; [`multiplier`] gives the forms
    /// alone (and none for 1, P-14.2.1.1.2), the combined forms are checked through 21, 22, 101.
    #[test]
    fn numerical_terms() {
        let expect = [
            (2, "di"),
            (3, "tri"),
            (4, "tetra"),
            (5, "penta"),
            (6, "hexa"),
            (7, "hepta"),
            (8, "octa"),
            (9, "nona"),
            (10, "deca"),
            (11, "undeca"),
            (12, "dodeca"),
            (14, "tetradeca"),
            (20, "icosa"),
            (21, "henicosa"),
            (22, "docosa"),
            (23, "tricosa"),
            (24, "tetracosa"),
            (30, "triaconta"),
            (40, "tetraconta"),
            (41, "hentetraconta"),
            (50, "pentaconta"),
            (52, "dopentaconta"),
            (60, "hexaconta"),
            (70, "heptaconta"),
            (80, "octaconta"),
            (90, "nonaconta"),
            (100, "hecta"),
            (101, "henhecta"),
            (102, "dohecta"),
            (111, "undecahecta"),
            (200, "dicta"),
            (300, "tricta"),
            (363, "trihexacontatricta"),
            (400, "tetracta"),
            (486, "hexaoctacontatetracta"),
            (500, "pentacta"),
            (600, "hexacta"),
            (700, "heptacta"),
            (800, "octacta"),
            (900, "nonacta"),
            (1000, "kilia"),
            (1001, "henkilia"),
            (2000, "dilia"),
            (2567, "heptahexacontapentactadilia"),
            (3000, "trilia"),
            (4000, "tetralia"),
            (5000, "pentalia"),
            (6000, "hexalia"),
            (7000, "heptalia"),
            (8000, "octalia"),
            (9000, "nonalia"),
            (9999, "nonanonacontanonactanonalia"),
        ];
        for (n, term) in expect {
            assert_eq!(multiplier(n), term, "{n}");
        }
        assert_eq!(multiplier(1), "");
    }

    /// Wikipedia's table of alkane names, as stems (the full names are checked on drawn chains
    /// in `naming::tests`). Wikipedia writes 500 as "pentacane"; IUPAC 2013 (Table 1.4:
    /// "pentacta") makes it "pentactane", like its neighbours "tetractane" and "hexactane".
    #[test]
    fn alkane_stems() {
        let expect = [
            (5, "pent"),
            (9, "non"),
            (19, "nonadec"),
            (25, "pentacos"),
            (29, "nonacos"),
            (40, "tetracont"),
            (100, "hect"),
            (200, "dict"),
            (500, "pentact"),
            (1000, "kili"),
            (2000, "dili"),
            (9000, "nonali"),
            (2567, "heptahexacontapentactadili"),
        ];
        for (n, s) in expect {
            assert_eq!(stem(n).unwrap(), s, "{n}");
        }
    }

    #[test]
    fn multipliers() {
        assert_eq!(multiplier(2), "di");
        assert_eq!(multiplier(5), "penta");
        assert_eq!(multiplier(10), "deca");
        assert_eq!(multiplier(12), "dodeca");
        assert_eq!(multiplier_complex(4), "tetrakis");
    }

    #[test]
    fn alpha() {
        assert_eq!(alpha_key("tert-butyl"), "butyl");
        assert_eq!(alpha_key("2-chloroethyl"), "chloroethyl");
        assert_eq!(alpha_key("ethyl(methyl)amino"), "ethylmethylamino");
    }
}
