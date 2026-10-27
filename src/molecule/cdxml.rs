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
//! - **Nodes.** Written by the binary writer's own rules ([`cdx::node_rules`]), so both formats
//!   describe the same structure: the atomic number as `Element` (carbon, the default node, is
//!   omitted); a label that is no element ("Ph", "CO_2Et", "D") as `NodeType="Unspecified"`;
//!   `NumHydrogens` and `Charge`; a label as `<t>` with one `<s>` per style run (`face` as in CDX:
//!   96 formula, 32 subscript, 2 italic, …); a hydrogen folded into its neighbour's label ("NH2")
//!   not as a node, nor its bond.
//! - **Bond order.** `Order="1"|"2"|"3"` for single/double/triple.
//! - **Stereo.** Mapped onto CDXML's `Display` bond attribute (`WedgeBegin`, `WedgedHashBegin`,
//!   `Bold`, `Dash`, `Wavy`).

use super::cdx::{self, NodeKind, NodeLabel, NodeRules};
use super::{Atom, BondOrder, BondStereo, Molecule};

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
    // The nodes, as the binary writer writes them (one set of rules, `cdx::node_rules`). A
    // hydrogen folded into its neighbour's label is not one of them, only that label's count.
    let nodes: Vec<(&Atom, NodeRules)> =
        mol.atoms.iter().filter(|a| cdx::is_node(mol, a.id)).map(|a| (a, cdx::node_rules(mol, a))).collect();

    // The font the labels name (the binary writer's document font table has the same).
    if nodes.iter().any(|(_, rules)| rules.label.is_some()) {
        s.push_str(&format!(
            " <fonttable>\n  <font id=\"{}\" charset=\"iso-8859-1\" name=\"Arial\"/>\n </fonttable>\n",
            cdx::LABEL_FONT
        ));
    }
    s.push_str(" <page>\n");
    s.push_str("  <fragment>\n");

    // Nodes (atoms). The atom's own id is reused as the CDXML node id, and bonds reference it via
    // B/E — atom ids are unique and positive, so no separate remapping is needed.
    for (atom, rules) in &nodes {
        let x = atom.pos[0] as f64 * POINT_SCALE;
        let y = atom.pos[1] as f64 * POINT_SCALE;
        s.push_str(&format!(
            "   <n id=\"{}\" p=\"{} {}\"",
            atom.id,
            fmt_num(x),
            fmt_num(y),
        ));
        // Carbon (Z=6) is ChemDraw's default node type and is omitted, matching the binary writer.
        match rules.kind {
            NodeKind::Carbon => {}
            NodeKind::Element(z) => s.push_str(&format!(" Element=\"{z}\"")),
            NodeKind::Unspecified => s.push_str(" NodeType=\"Unspecified\""),
        }
        if let Some(h) = rules.num_hydrogens {
            s.push_str(&format!(" NumHydrogens=\"{h}\""));
        }
        if rules.charge != 0 {
            s.push_str(&format!(" Charge=\"{}\"", rules.charge));
        }
        match &rules.label {
            None => s.push_str("/>\n"),
            Some(label) => {
                s.push('>');
                push_label(&mut s, (x, y), label);
                s.push_str("</n>\n");
            }
        }
    }

    // Bonds. Skip any that reference an atom that isn't a node (missing, or a folded hydrogen).
    for bond in &mol.bonds {
        let is_node = |id: u32| mol.atom_by_id(id).is_some() && cdx::is_node(mol, id);
        if !(is_node(bond.begin) && is_node(bond.end)) {
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

/// A node's label as CDXML's `<t>`: placed and aligned as the binary writer places its text
/// object, one `<s>` per style run (with its face, as CDX's style runs have it), the text escaped.
fn push_label(s: &mut String, at: (f64, f64), label: &NodeLabel) {
    // The binary writer's geometry is in CDX units (1/65536 pt), CDXML's in points.
    let pt = |v: f64| fmt_num(v / cdx::PT);
    let (origin, [left, top, right, bottom]) =
        cdx::label_geometry((at.0 * cdx::PT, at.1 * cdx::PT), &label.text, label.right);
    s.push_str(&format!(
        "<t p=\"{} {}\" BoundingBox=\"{} {} {} {}\" LabelAlignment=\"{}\">",
        pt(origin.0),
        pt(origin.1),
        pt(left),
        pt(top),
        pt(right),
        pt(bottom),
        if label.right { "Right" } else { "Left" },
    ));
    let chars: Vec<char> = label.text.chars().collect();
    let runs: &[(u16, u16)] = if label.runs.is_empty() { &[(0, 0)] } else { &label.runs };
    for (i, &(start, face)) in runs.iter().enumerate() {
        let end = runs.get(i + 1).map_or(chars.len(), |r| r.0 as usize).min(chars.len());
        let text: String = chars[(start as usize).min(end)..end].iter().collect();
        s.push_str(&format!(
            "<s font=\"{}\" size=\"{}\"",
            cdx::LABEL_FONT,
            fmt_num(cdx::LABEL_SIZE as f64 / 20.0)
        ));
        if face != 0 {
            s.push_str(&format!(" face=\"{face}\""));
        }
        s.push('>');
        s.push_str(&xml_escape(&text));
        s.push_str("</s>");
    }
    s.push_str("</t>");
}

/// Escape the five XML metacharacters. Used for any text that could contain them (labels are free
/// text: "R&D", "<1>").
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Count non-overlapping occurrences of `needle` in `hay`.
    fn count(hay: &str, needle: &str) -> usize {
        hay.matches(needle).count()
    }

    /// A minimal well-formedness check sufficient for these outputs: every container tag opens and
    /// closes exactly once, every `<b>`/`<font>` self-closes, every `<n>` self-closes or (holding
    /// its label) has its `</n>`, and every label's `<t>`/`<s>` is closed. We assert the specific
    /// tag pairing instead of pulling in a full XML parser.
    fn assert_well_formed(xml: &str) {
        assert!(xml.starts_with("<?xml "), "has XML declaration");
        assert!(xml.contains("<!DOCTYPE CDXML"), "has CDXML DOCTYPE");
        for tag in ["CDXML", "page", "fragment"] {
            assert_eq!(count(xml, &format!("<{tag}")), 1, "one <{tag}");
            assert_eq!(count(xml, &format!("</{tag}>")), 1, "one </{tag}>");
        }
        assert_eq!(count(xml, "<fonttable>"), count(xml, "</fonttable>"), "<fonttable> is closed");
        let self_closing = count(xml, "/>");
        let n_open = count(xml, "<n ");
        let n_closed = count(xml, "</n>");
        let b_open = count(xml, "<b ");
        let font_open = count(xml, "<font ");
        assert_eq!(self_closing + n_closed, n_open + b_open + font_open, "every <n>/<b>/<font> is closed");
        for tag in ["t", "s"] {
            assert_eq!(count(xml, &format!("<{tag} ")), count(xml, &format!("</{tag}>")), "every <{tag}> is closed");
        }
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

    /// A chain of neutral carbons with a wedge and a double bond: nothing on it shows a label.
    fn plain_chain() -> Molecule {
        let mut mol = Molecule::default();
        let a = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [1.3, 0.75], 0);
        let c = mol.add_atom("C".to_string(), [2.6, 0.0], 0);
        let d = mol.add_atom("C".to_string(), [1.3, 2.25], 0);
        let w = mol.add_bond(a, b, BondOrder::Single);
        mol.bond_by_id_mut(w).unwrap().stereo = BondStereo::WedgeUp;
        mol.add_bond(b, c, BondOrder::Double);
        mol.add_bond(b, d, BondOrder::Single);
        mol
    }

    /// A molecule with no label at all is written exactly as before labels were written.
    #[test]
    fn a_structure_without_labels_is_written_as_before() {
        let expected = "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>
<!DOCTYPE CDXML SYSTEM \"http://www.cambridgesoft.com/xml/cdxml.dtd\" >
<CDXML BondLength=\"14.399986\">
 <page>
  <fragment>
   <n id=\"1\" p=\"0 0\"/>
   <n id=\"2\" p=\"12.479988 7.199993\"/>
   <n id=\"3\" p=\"24.959975 0\"/>
   <n id=\"4\" p=\"12.479988 21.599979\"/>
   <b B=\"1\" E=\"2\" Order=\"1\" Display=\"WedgeBegin\"/>
   <b B=\"2\" E=\"3\" Order=\"2\"/>
   <b B=\"2\" E=\"4\" Order=\"1\"/>
  </fragment>
 </page>
</CDXML>
";
        assert_eq!(to_cdxml(&plain_chain()), expected);
    }

    /// The `<n …>` start tag of the node with id `id`, and what it holds up to its `</n>` (empty
    /// for a node that self-closes).
    fn node(xml: &str, id: u32) -> (&str, &str) {
        let start = xml.find(&format!("<n id=\"{id}\"")).unwrap_or_else(|| panic!("node {id}"));
        let rest = &xml[start..];
        let tag_end = rest.find('>').unwrap() + 1;
        let tag = &rest[..tag_end];
        if tag.ends_with("/>") {
            (tag, "")
        } else {
            (tag, &rest[tag_end..rest.find("</n>").unwrap()])
        }
    }

    #[test]
    fn abbreviations_are_unspecified_nodes_with_their_label() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let ome = mol.add_atom("OMe".to_string(), [1.3, 0.75], 0);
        let ph = mol.add_atom("Ph".to_string(), [-1.3, 0.75], 0);
        let ester = mol.add_atom("CO_2Et".to_string(), [0.0, -1.5], 0);
        for x in [ome, ph, ester] {
            mol.add_bond(c, x, BondOrder::Single);
        }
        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        for (id, text) in [(ome, "OMe"), (ph, "Ph")] {
            let (tag, inner) = node(&xml, id);
            assert!(tag.contains("NodeType=\"Unspecified\""), "{text}: {tag}");
            assert!(!tag.contains("Element="), "{text} is no element: {tag}");
            assert!(inner.starts_with("<t "), "{text} has its label: {inner}");
            assert!(inner.contains(&format!(">{text}</s></t>")), "{text}: {inner}");
        }
        // The markup's subscript is the label's own style run.
        let (_, inner) = node(&xml, ester);
        assert!(inner.contains(">CO</s><s font=\"3\" size=\"10\" face=\"32\">2</s><s font=\"3\" size=\"10\">Et</s>"), "{inner}");
        // The font the labels name is in the document's font table.
        assert!(xml.contains("<fonttable>"), "{xml}");
        assert!(xml.contains("<font id=\"3\" charset=\"iso-8859-1\" name=\"Arial\"/>"), "{xml}");
        // The carbon they hang on shows nothing.
        assert_eq!(node(&xml, c).1, "");
    }

    #[test]
    fn label_text_is_escaped() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let x = mol.add_atom("R&D<1>".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, x, BondOrder::Single);
        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        let (_, inner) = node(&xml, x);
        assert!(inner.contains("R&amp;D&lt;1&gt;"), "{inner}");
        assert!(!xml.contains("R&D"), "no raw metacharacters");
    }

    #[test]
    fn deuterium_is_a_label_as_in_cdx() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let d = mol.add_atom("D".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, d, BondOrder::Single);
        let xml = to_cdxml(&mol);
        let (tag, inner) = node(&xml, d);
        assert!(tag.contains("NodeType=\"Unspecified\""), "{tag}");
        assert!(inner.contains(">D</s></t>"), "{inner}");
    }

    #[test]
    fn group_hydrogens_are_the_labels_count_not_nodes() {
        // H2N–CH3, with one hydrogen of the NH2 an atom brought in by the group.
        let mut mol = Molecule::default();
        let n = mol.add_atom("N".to_string(), [0.0, 0.0], 0);
        let c = mol.add_atom("C".to_string(), [1.3, 0.75], 0);
        mol.add_bond(n, c, BondOrder::Single);
        let h = mol.add_atom("H".to_string(), [-1.0, 0.5], 0);
        mol.atom_by_id_mut(h).unwrap().group_h = true;
        mol.add_bond(n, h, BondOrder::Single);
        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        assert_eq!(count(&xml, "<n "), 2, "the group's hydrogen is not a node: {xml}");
        assert_eq!(count(&xml, "<b "), 1, "nor is its bond: {xml}");
        assert!(!xml.contains(&format!("<n id=\"{h}\"")));
        let (tag, inner) = node(&xml, n);
        assert!(tag.contains("Element=\"7\"") && tag.contains("NumHydrogens=\"2\""), "{tag}");
        // As the CDX writer: the formula face, the hydrogens on the side away from the bond.
        assert!(inner.contains("LabelAlignment=\"Right\""), "{inner}");
        assert!(inner.contains("<s font=\"3\" size=\"10\" face=\"96\">H2N</s>"), "{inner}");
    }

    /// ChemDraw's own file with abbreviations (Ph and tBu as nicknames, CO2Et as a fragment, and
    /// a label it cannot interpret), read and written as CDXML: every label is kept.
    #[test]
    fn chemdraw_abbreviations_keep_their_labels() {
        let bytes = include_bytes!("../../tests/fixtures/chemdraw26_abbreviations.cdx");
        let mol = super::super::cdx::read_cdx(bytes).expect("reads").molecule;
        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        assert_eq!(count(&xml, "NodeType=\"Unspecified\""), 4, "{xml}");
        for label in [">Ph</s>", ">CO</s>", "face=\"32\">2</s>", ">Et</s>", "face=\"2\">t</s>", ">Bu</s>", ">Xyz</s>"] {
            assert!(xml.contains(label), "{label} in {xml}");
        }
        // No node of no element goes without its text.
        for (i, _) in xml.match_indices("NodeType=\"Unspecified\"") {
            assert!(xml[i..].starts_with("NodeType=\"Unspecified\"><t "), "{}", &xml[i..]);
        }
    }

    /// The fixture's ammonium (NH4+ on its own) keeps its hydrogens as a count, not as nodes.
    #[test]
    fn chemdraw_ammonium_writes_its_hydrogen_count() {
        let bytes = include_bytes!("../../tests/fixtures/chemdraw26_charge_stereo.cdx");
        let mol = super::super::cdx::read_cdx(bytes).expect("reads").molecule;
        let xml = to_cdxml(&mol);
        assert_well_formed(&xml);
        assert_eq!(count(&xml, "<n "), 7, "heavy atoms only: {xml}");
        assert!(xml.contains("Element=\"7\" NumHydrogens=\"4\" Charge=\"1\""), "{xml}");
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
