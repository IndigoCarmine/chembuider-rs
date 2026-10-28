//! Name fragments: numerical stems, multiplying prefixes, enclosing marks, alphanumerical keys.

/// The stem of the saturated unbranched hydrocarbon with `n` carbons ("meth", "eth", …,
/// "icos", "henicos", "triacont"), for 1 ≤ n ≤ 99.
pub(crate) fn stem(n: usize) -> Option<String> {
    const FIRST: [&str; 10] = [
        "", "meth", "eth", "prop", "but", "pent", "hex", "hept", "oct", "non",
    ];
    const UNITS: [&str; 10] = [
        "", "hen", "do", "tri", "tetra", "penta", "hexa", "hepta", "octa", "nona",
    ];
    const TENS: [&str; 10] = [
        "",
        "dec",
        "icos",
        "triacont",
        "tetracont",
        "pentacont",
        "hexacont",
        "heptacont",
        "octacont",
        "nonacont",
    ];
    Some(match n {
        0 | 100.. => return None,
        1..=9 => FIRST[n].to_string(),
        10 => "dec".into(),
        11 => "undec".into(),
        _ => {
            let (t, u) = (n / 10, n % 10);
            let unit = UNITS[u];
            let mut tens = TENS[t].to_string();
            // "icos" loses its "i" after a unit ending in a vowel: docos, tricos, tetracos.
            if t == 2 && unit.ends_with(['a', 'i', 'o']) {
                tens.remove(0);
            }
            format!("{unit}{tens}")
        }
    })
}

/// The multiplying prefix for `n` identical simple features: "", "di", "tri", …
pub(crate) fn multiplier(n: usize) -> String {
    match n {
        0 | 1 => String::new(),
        2 => "di".into(),
        3 => "tri".into(),
        4 => "tetra".into(),
        11 => "undeca".into(),
        // "pent" → "penta", "dec" → "deca", "dodec" → "dodeca".
        5..=9 => ["penta", "hexa", "hepta", "octa", "nona"][n - 5].to_string(),
        _ => stem(n).map_or_else(|| n.to_string(), |s| format!("{s}a")),
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
        assert_eq!(stem(100), None);
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
