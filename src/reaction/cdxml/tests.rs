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
    let _ = crate::testkit::run_ui(&ctx, raw, |ui| {
        out = Some(scheme_document(
            scheme,
            ui.painter(),
            &ReactionStyle::default(),
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

    let xml = cdx::cdxml_of_document(&doc);
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
    let read = scheme_from_cdxml(&cdx::cdxml_of_document(&document(&scheme))).expect("a scheme");
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
        let xml = cdx::cdxml_of_document(&document(&scheme));
        let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    }
}

/// Line breaks and labels are part of the figure the `.rxn.json` file keeps, and a structure
/// with no label writes none, as files from before labels have none.
#[test]
fn line_breaks_and_labels_survive_the_reaction_file() {
    use crate::reaction::Reaction;
    let reaction = demo::multi_step_scheme();
    let read = Reaction::from_json(&reaction.to_json()).unwrap();
    assert_eq!(outline(&read.scheme), outline(&reaction.scheme));
    let plain = demo::williamson_ether_synthesis().to_json();
    assert!(!plain.contains("\"label\""), "{plain}");
    assert!(Reaction::from_json(&plain).is_ok());
}

/// `scheme` written as the reaction figure writes it: a [`ReactionView`](crate::reaction::ReactionView)
/// with its default styles.
fn figure_document(scheme: &ReactionScheme) -> CdxObject {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
        ..Default::default()
    };
    let mut out = None;
    let _ = crate::testkit::run_ui(&ctx, raw, |ui| {
        out = Some(crate::reaction::ReactionView::new().scheme_document(ui.painter(), scheme));
    });
    out.unwrap()
}

/// The x range (points) each object on a row covers as ChemDraw draws it: a fragment its atoms
/// and their 10 pt labels, an arrow its shaft, a text its box. Riders over and under arrows are
/// left out (they sit off the line).
fn row_extents(doc: &CdxObject) -> Vec<(f64, f64, f64, String)> {
    let page = &doc.children[0];
    let riders: HashSet<u32> = page
        .children
        .iter()
        .filter(|c| c.tag == OBJ_REACTION_SCHEME)
        .flat_map(|s| s.children.iter())
        .flat_map(|step| [ids(step, PROP_STEP_ABOVE_ARROW), ids(step, PROP_STEP_BELOW_ARROW)].concat())
        .collect();
    let labels: HashSet<u32> = page.children.iter().filter(|c| c.tag == OBJ_CHEMICAL_PROPERTY).flat_map(|p| ids(p, PROP_CHEMICAL_PROPERTY_DISPLAY_ID)).collect();
    let mut out = Vec::new();
    for c in &page.children {
        if riders.contains(&c.id) || labels.contains(&c.id) {
            continue;
        }
        let b = match c.tag {
            OBJ_FRAGMENT => {
                let mut b: Option<B> = None;
                for n in c.children.iter().filter(|n| n.tag == OBJ_NODE) {
                    let (x, y) = position(n).unwrap();
                    b = Some(cdx::union(b, [x, y, x, y]));
                    for t in n.children.iter().filter(|t| t.tag == OBJ_TEXT) {
                        b = Some(cdx::union(b, cdx::bounding_box(t).unwrap()));
                    }
                }
                b.unwrap()
            }
            OBJ_GRAPHIC | OBJ_TEXT => cdx::bounding_box(c).unwrap(),
            _ => continue,
        };
        let (l, r) = (b[0].min(b[2]) / PT, b[0].max(b[2]) / PT);
        out.push((l, r, (b[1] + b[3]) * 0.5 / PT, format!("{:04x}#{}", c.tag, c.id)));
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// The figure lays a scheme out for ChemDraw with the atom labels ChemDraw draws (10 pt, as
/// they are written), so a structure's labels keep the whole gap to the `+` or the arrow beside
/// it, as everything else on the row does.
#[test]
fn written_row_items_keep_their_gaps_as_chemdraw_draws_them() {
    let gap_pt = ReactionStyle::default().chemdraw().item_gap as f64;
    for scheme in [demo::williamson_ether_synthesis().scheme, demo::multi_step_scheme().scheme] {
        let doc = figure_document(&scheme);
        // Rows: the arrows' heights.
        let mut lines: Vec<f64> = Vec::new();
        for c in doc.children[0].children.iter().filter(|c| c.tag == OBJ_GRAPHIC) {
            let y = cdx::bounding_box(c).unwrap()[1] / PT;
            if lines.iter().all(|l| (l - y).abs() > 20.0) {
                lines.push(y);
            }
        }
        let row_of = |y: f64| lines.iter().enumerate().min_by(|a, b| (a.1 - y).abs().total_cmp(&(b.1 - y).abs())).unwrap().0;
        let extents = row_extents(&doc);
        for row in 0..lines.len() {
            let on_row: Vec<_> = extents.iter().filter(|e| row_of(e.2) == row).collect();
            assert!(on_row.len() >= 3, "{on_row:?}");
            for w in on_row.windows(2) {
                let gap = w[1].0 - w[0].1;
                // Within a point: the layout measures labels in egui's font, the written boxes
                // are Arial's widths.
                assert!(gap >= gap_pt - 1.0, "{} to {}: {gap:.2} pt, not the {gap_pt} pt gap", w[0].3, w[1].3);
            }
        }
    }
}
