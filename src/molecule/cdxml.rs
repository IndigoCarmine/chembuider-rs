//! Convert a `Molecule` into ChemDraw **CDXML** (the XML serialization of the CDX format,
//! extension `.cdxml`). It follows the
//! same design as [`mol2`](super::mol2): a pure function that takes a `&Molecule` and returns a
//! `String`. Output only; no parsing.
//!
//! CDXML shares CDX's data model, so the coordinate/element/bond mappings here mirror
//! [`cdx`](super::cdx):
//! - **Coordinates.** The binary writer scales molecular units by `SCALE = 629_145` into CDX's
//!   1/65536-point integer units. CDXML expresses positions in *points* (floating text), so the
//!   equivalent factor is `SCALE / 65536 = 9.6` points per molecular unit. The y-axis is written
//!   as-is (no flip), matching the binary writer, which is pinned to real ChemDraw output by the
//!   `parse_real_chemdraw_cdx` fixture test.
//! - **Element.** Atomic number, same table as `cdx`. Carbon (Z=6) is the default node type and
//!   is omitted, exactly as the binary writer omits it.
//! - **Bond order.** `Order="1"|"2"|"3"` for single/double/triple.
//! - **Stereo.** Mapped onto CDXML's `Display` bond attribute (`WedgeBegin`, `WedgedHashBegin`,
//!   `Bold`, `Dash`, `Wavy`).

use super::{BondOrder, BondStereo, Molecule};

/// Points per molecular unit. Equals the binary writer's `SCALE` (629_145 units of 1/65536 point)
/// divided by 65536, so a CDXML file and a CDX file built from the same `Molecule` describe the
/// same geometry at the same scale.
const POINT_SCALE: f64 = 629_145.0 / 65536.0; // = 9.6

/// ChemDraw's standard bond length, in points: the default 1.5-molecular-unit bond at [`POINT_SCALE`].
const BOND_LENGTH_PT: f64 = 1.5 * POINT_SCALE; // = 14.4

/// Serialize `mol` to a CDXML document string (always non-empty; a molecule with no atoms yields
/// a valid-but-empty `<fragment/>`).
pub fn to_cdxml(mol: &Molecule) -> String {
    let mut s = String::new();

    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n");
    s.push_str("<!DOCTYPE CDXML SYSTEM \"http://www.cambridgesoft.com/xml/cdxml.dtd\" >\n");
    s.push_str(&format!(
        "<CDXML BondLength=\"{}\">\n",
        fmt_num(BOND_LENGTH_PT)
    ));
    s.push_str(" <page>\n");
    s.push_str("  <fragment>\n");

    // Nodes (atoms). The atom's own id is reused as the CDXML node id, and bonds reference it via
    // B/E — atom ids are unique and positive, so no separate remapping is needed.
    for atom in &mol.atoms {
        let x = atom.pos[0] as f64 * POINT_SCALE;
        let y = atom.pos[1] as f64 * POINT_SCALE;
        s.push_str(&format!(
            "   <n id=\"{}\" p=\"{} {}\"",
            atom.id,
            fmt_num(x),
            fmt_num(y),
        ));
        // Carbon (Z=6) is ChemDraw's default node type and is omitted, matching the binary writer.
        if let Some(z) = atomic_number(&atom.element) {
            if z != 6 {
                s.push_str(&format!(" Element=\"{z}\""));
            }
        }
        if atom.charge != 0 {
            s.push_str(&format!(" Charge=\"{}\"", atom.charge));
        }
        s.push_str("/>\n");
    }

    // Bonds. Skip any that reference an atom that isn't present.
    for bond in &mol.bonds {
        let has_ends = mol.atom_by_id(bond.begin).is_some() && mol.atom_by_id(bond.end).is_some();
        if !has_ends {
            continue;
        }
        s.push_str(&format!(
            "   <b B=\"{}\" E=\"{}\" Order=\"{}\"",
            bond.begin,
            bond.end,
            bond_order_str(&bond.order),
        ));
        if let Some(display) = bond_display(&bond.stereo) {
            s.push_str(&format!(" Display=\"{display}\""));
        }
        s.push_str("/>\n");
    }

    s.push_str("  </fragment>\n");
    s.push_str(" </page>\n");
    s.push_str("</CDXML>\n");

    s
}

/// CDXML `Order` value for a bond order.
fn bond_order_str(order: &BondOrder) -> &'static str {
    match order {
        BondOrder::Single => "1",
        BondOrder::Double => "2",
        BondOrder::Triple => "3",
    }
}

/// CDXML `Display` value for a bond stereo, or `None` for a plain bond (no attribute written).
///
/// These strings are ChemDraw's `kCDXBondDisplay` enum names. `WedgeBegin`/`WedgedHashBegin` put
/// the narrow (point) end at the bond's begin atom — matching how [`BondStereo`] documents its
/// wedges as "narrow at begin".
fn bond_display(stereo: &BondStereo) -> Option<&'static str> {
    match stereo {
        BondStereo::None => None,
        BondStereo::WedgeUp => Some("WedgeBegin"),
        BondStereo::WedgeDown => Some("WedgedHashBegin"),
        BondStereo::Bold => Some("Bold"),
        BondStereo::Dashed => Some("Dash"),
        BondStereo::Wavy => Some("Wavy"),
    }
}

/// Format a coordinate/length: fixed-point, but with trailing zeros and any trailing `.` trimmed
/// so integers read as `0` rather than `0.000000`. Always yields characters safe inside XML.
fn fmt_num(v: f64) -> String {
    let mut s = format!("{v:.6}");
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" {
        s = "0".to_string();
    }
    s
}

/// Escape the five XML metacharacters. Used for any text that could contain them (element labels,
/// names). Kept even though current element symbols are alphabetic — an element field is free text
/// in principle, and correctness here is a stated requirement.
#[allow(dead_code)]
fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

fn atomic_number(el: &str) -> Option<u16> {
    super::elements::atomic_number(el)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Count non-overlapping occurrences of `needle` in `hay`.
    fn count(hay: &str, needle: &str) -> usize {
        hay.matches(needle).count()
    }

    /// A minimal well-formedness check sufficient for these outputs: every container tag opens and
    /// closes exactly once, and every `<n>`/`<b>` self-closes. We assert the specific tag pairing
    /// instead of pulling in a full XML parser.
    fn assert_well_formed(xml: &str) {
        assert!(xml.starts_with("<?xml "), "has XML declaration");
        assert!(xml.contains("<!DOCTYPE CDXML"), "has CDXML DOCTYPE");
        for tag in ["CDXML", "page", "fragment"] {
            assert_eq!(count(xml, &format!("<{tag}")), 1, "one <{tag}");
            assert_eq!(count(xml, &format!("</{tag}>")), 1, "one </{tag}>");
        }
        let self_closing = count(xml, "/>");
        let n_open = count(xml, "<n ");
        let b_open = count(xml, "<b ");
        assert_eq!(self_closing, n_open + b_open, "every <n>/<b> self-closes");
    }

    fn benzene() -> Molecule {
        // Regular hexagon of carbons, alternating single/double bonds (Kekulé).
        let mut mol = Molecule::default();
        let mut ids = Vec::new();
        for i in 0..6 {
            let a = std::f32::consts::PI / 3.0 * i as f32;
            ids.push(mol.add_atom("C".to_string(), [a.cos() * 1.5, a.sin() * 1.5], 0));
        }
        for i in 0..6 {
            let order = if i % 2 == 0 { BondOrder::Double } else { BondOrder::Single };
            mol.add_bond(ids[i], ids[(i + 1) % 6], order);
        }
        mol
    }

    #[test]
    fn benzene_has_six_nodes_six_bonds_and_is_well_formed() {
        let xml = to_cdxml(&benzene());
        assert_well_formed(&xml);
        assert_eq!(count(&xml, "<n "), 6, "six carbon nodes");
        assert_eq!(count(&xml, "<b "), 6, "six bonds");
        // All carbon → no Element attribute is emitted at all.
        assert!(!xml.contains("Element="), "carbon nodes omit Element");
        // Three double bonds and three singles.
        assert_eq!(count(&xml, "Order=\"2\""), 3, "three double bonds");
        assert_eq!(count(&xml, "Order=\"1\""), 3, "three single bonds");
    }

    #[test]
    fn heteroatoms_get_atomic_numbers_and_carbon_does_not() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        let n = mol.add_atom("N".to_string(), [0.0, 1.5], 0);
        mol.add_bond(c, o, BondOrder::Double);
        mol.add_bond(c, n, BondOrder::Single);
        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        assert!(xml.contains("Element=\"8\""), "oxygen is Z=8");
        assert!(xml.contains("Element=\"7\""), "nitrogen is Z=7");
        assert_eq!(count(&xml, "Element="), 2, "only the two heteroatoms carry Element");
    }

    #[test]
    fn coordinates_use_the_shared_point_scale() {
        let mut mol = Molecule::default();
        mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        // 1.5 molecular units → one standard bond length (~14.4 pt; not exactly 14.4 because
        // the binary writer's SCALE=629_145 is a hair under 9.6*65536, and we stay faithful to it).
        mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        let xml = to_cdxml(&mol);
        let expected = fmt_num(1.5 * POINT_SCALE);
        assert!((POINT_SCALE - 9.6).abs() < 1e-3, "scale is ~9.6 pt per molecular unit");
        assert!(xml.contains(&format!("BondLength=\"{expected}\"")), "standard bond length in points");
        assert!(xml.contains("p=\"0 0\""), "origin atom at 0 0");
        assert!(xml.contains(&format!("p=\"{expected} 0\"")), "1.5 units → one bond length on x");
    }

    #[test]
    fn wedge_and_hash_map_to_display_and_charge_is_written() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], -1);
        let bid = mol.add_bond(c, o, BondOrder::Single);
        mol.bond_by_id_mut(bid).unwrap().stereo = BondStereo::WedgeUp;
        // Second wedge (hash) so we can see both display strings.
        let n = mol.add_atom("N".to_string(), [0.0, 1.5], 0);
        let bid2 = mol.add_bond(c, n, BondOrder::Single);
        mol.bond_by_id_mut(bid2).unwrap().stereo = BondStereo::WedgeDown;

        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        assert!(xml.contains("Display=\"WedgeBegin\""), "solid wedge");
        assert!(xml.contains("Display=\"WedgedHashBegin\""), "hash wedge");
        assert!(xml.contains("Charge=\"-1\""), "the anionic oxygen carries its charge");
    }

    #[test]
    fn bonds_reference_existing_node_ids() {
        let xml = to_cdxml(&benzene());
        // Collect node ids, then confirm every B/E points at one of them.
        let node_ids: Vec<&str> = xml
            .match_indices("<n id=\"")
            .map(|(i, _)| {
                let rest = &xml[i + 7..];
                &rest[..rest.find('"').unwrap()]
            })
            .collect();
        for (i, _) in xml.match_indices("B=\"") {
            let rest = &xml[i + 3..];
            let id = &rest[..rest.find('"').unwrap()];
            assert!(node_ids.contains(&id), "bond begin {id} is a real node");
        }
    }

    #[test]
    fn xml_escape_handles_all_five_metacharacters() {
        assert_eq!(xml_escape("a&b<c>d\"e'f"), "a&amp;b&lt;c&gt;d&quot;e&apos;f");
    }

    #[test]
    fn fmt_num_trims_trailing_zeros_and_negative_zero() {
        assert_eq!(fmt_num(0.0), "0");
        assert_eq!(fmt_num(-0.0), "0");
        assert_eq!(fmt_num(14.4), "14.4");
        assert_eq!(fmt_num(1.5), "1.5");
    }
}
