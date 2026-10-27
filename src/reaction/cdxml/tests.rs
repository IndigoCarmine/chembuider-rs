use super::*;
use crate::reaction::demo;

fn fixture() -> String {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scheme_two_rows.cdxml")).unwrap()
}

/// `scheme` written as a ChemDraw document, inside a headless egui frame.
fn document(scheme: &ReactionScheme) -> CdxObject {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
        ..Default::default()
    };
    let mut out = None;
    let _ = ctx.run_ui(raw, |ui| {
        out = Some(scheme_document(
            scheme,
            ui.painter(),
            &ReactionStyle::default(),
            &StyleConfig::default(),
            &mut ChemStructEditor::default(),
        ));
    });
    out.unwrap()
}

/// A scheme as one line per item: what it is, and its text.
fn outline(scheme: &ReactionScheme) -> Vec<String> {
    let label = |l: &ArrowLabel| match l {
        ArrowLabel::Empty => "-".to_string(),
        ArrowLabel::Text(t) => t.to_markup(),
        ArrowLabel::Structure(s) => format!("structure of {} atoms", s.atoms.len()),
    };
    scheme
        .items
        .iter()
        .map(|item| match item {
            SchemeItem::Structure(s) => format!("structure {:?}", s.label().to_markup()),
            SchemeItem::Arrow(a) => format!("arrow {} / {}", label(&a.above), label(&a.below)),
            SchemeItem::Plus => "+".to_string(),
            SchemeItem::Text(t) => format!("text {}", t.to_markup()),
            SchemeItem::LineBreak => "break".to_string(),
        })
        .collect()
}

fn atom_counts(scheme: &ReactionScheme) -> Vec<usize> {
    scheme
        .items
        .iter()
        .filter_map(|i| match i {
            SchemeItem::Structure(s) => Some(s.atoms.len()),
            _ => None,
        })
        .collect()
}

fn count(o: &CdxObject, tag: u16) -> usize {
    o.children.iter().map(|c| count(c, tag)).sum::<usize>() + usize::from(o.tag == tag)
}

#[test]
fn a_chemdraw_scheme_reads_as_two_rows_of_labelled_steps() {
    let scheme = scheme_from_cdxml(&fixture()).expect("a scheme");
    assert_eq!(
        outline(&scheme),
        [
            "structure \"(I)\"",
            "arrow i) / -",
            "structure \"(II)\"",
            "break",
            "structure \"(II)\"",
            // No step for this arrow: its text is found over the shaft.
            "arrow ii) / -",
            // A free bold text right above a structure's label is more of that label.
            "structure \"**4**\\n(III)\"",
        ]
    );
    // The nickname is one labelled atom, its own fragment left out.
    let SchemeItem::Structure(second) = &scheme.items[2] else { panic!("a structure") };
    assert_eq!(second.atoms.iter().filter(|a| a.element == "TEG").count(), 1);
    // The intermediate (II) is the same compound at the end of one row and the start of the next
    // (heavy atoms; the second one is drawn without its nickname).
    assert_eq!(atom_counts(&scheme)[0], 2);
}

#[test]
fn a_multi_step_scheme_survives_cdxml() {
    let scheme = demo::multi_step_scheme().scheme;
    let doc = document(&scheme);
    // One reaction scheme per row, one step per arrow; every label bound to its structure.
    assert_eq!(count(&doc, OBJ_REACTION_SCHEME), 2);
    assert_eq!(count(&doc, OBJ_REACTION_STEP), 4);
    assert_eq!(count(&doc, OBJ_CHEMICAL_PROPERTY), 6);
    assert_eq!(count(&doc, OBJ_GRAPHIC), 4);

    let xml = cdxml_of(&doc);
    assert!(xml.contains("<step") && xml.contains("<chemicalproperty"), "{xml}");
    let read = scheme_from_cdxml(&xml).expect("a scheme");
    assert_eq!(outline(&read), outline(&scheme));
    assert_eq!(atom_counts(&read), atom_counts(&scheme));
    // And through binary CDX, the same.
    let read = scheme_from_cdx(&cdx_codec::write(&doc)).expect("a scheme");
    assert_eq!(outline(&read), outline(&scheme));
}

#[test]
fn a_step_names_the_structures_on_either_side_of_its_arrow() {
    let doc = document(&demo::multi_step_scheme().scheme);
    let page = &doc.children[0];
    let fragments: Vec<u32> = page.children.iter().filter(|c| c.tag == OBJ_FRAGMENT).map(|c| c.id).collect();
    let steps: Vec<&CdxObject> = page
        .children
        .iter()
        .filter(|c| c.tag == OBJ_REACTION_SCHEME)
        .flat_map(|s| s.children.iter())
        .collect();
    // Structures (I)…(V) with (III) twice: fragments 0..6.
    assert_eq!(fragments.len(), 6);
    let sides: Vec<(Vec<u32>, Vec<u32>)> =
        steps.iter().map(|s| (ids(s, PROP_STEP_REACTANTS), ids(s, PROP_STEP_PRODUCTS))).collect();
    let f = |i: usize| vec![fragments[i]];
    assert_eq!(sides, [(f(0), f(1)), (f(1), f(2)), (f(3), f(4)), (f(4), f(5))]);
    // The step label rides above its arrow.
    let above = ids(steps[0], PROP_STEP_ABOVE_ARROW);
    let text = page.children.iter().find(|c| c.id == above[0]).unwrap();
    assert_eq!(text_of(text).unwrap().to_markup(), "i)");
}

#[test]
fn reagents_keep_their_scripts_and_a_structure_on_the_arrow_stays_there() {
    let scheme = ReactionScheme::new()
        .structure(demo::phenol())
        .plus()
        .structure(demo::methyl_iodide())
        .arrow(Arrow::new().above_structure(demo::biphenyl()).below("K_2CO_3, Δ"))
        .structure(demo::anisole());
    let read = scheme_from_cdxml(&cdxml_of(&document(&scheme))).expect("a scheme");
    assert_eq!(outline(&read), outline(&scheme));
}

/// Without the steps and chemical properties ChemDraw writes, what sits on an arrow and under
/// a structure is still found where it is drawn.
#[test]
fn a_document_without_steps_is_read_by_where_things_are() {
    let mut doc = document(&demo::multi_step_scheme().scheme);
    doc.children[0].children.retain(|c| !matches!(c.tag, OBJ_REACTION_SCHEME | OBJ_CHEMICAL_PROPERTY));
    let read = scheme_from_document(&doc).expect("a scheme");
    assert_eq!(outline(&read), outline(&demo::multi_step_scheme().scheme));
}

#[test]
fn what_is_not_a_scheme_reads_as_nothing() {
    assert!(scheme_from_cdxml("not xml at all <").is_none());
    assert!(scheme_from_cdxml("<CDXML><page id=\"1\"/></CDXML>").is_none());
    assert!(scheme_from_cdx(b"VjCD0100").is_none());
}

#[test]
fn chemdraw_units_keep_the_row_limit_in_proportion() {
    let style = ReactionStyle { max_row_width: Some(400.0), ..Default::default() };
    let pt = style.chemdraw();
    assert_eq!(pt.bond_length, 14.4);
    assert!((pt.max_row_width.unwrap() - 400.0 * 14.4 / style.bond_length).abs() < 1e-3);
}

/// What is written is all CDXML the codec knows: nothing is left out reading it back.
#[test]
fn written_cdxml_reads_back_without_warnings() {
    for scheme in [demo::multi_step_scheme().scheme, demo::williamson_ether_synthesis().scheme] {
        let xml = cdxml_of(&document(&scheme));
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    }
}
