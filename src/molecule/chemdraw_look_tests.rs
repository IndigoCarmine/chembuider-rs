//! A drawing looks the same here and in ChemDraw: what is written is what ChemDraw writes for
//! the same drawing (its `ReactionAnalysis` document is the reference), it reads back as it
//! was written, and the editor draws a ChemDraw document in ChemDraw's proportions.

use std::collections::HashMap;

use super::cdx::{self, cdxml_document, cdxml_of_document, merge_into_cdx, molecule_to_cdxml, read_cdxml, PT, SCALE};
use super::cdx_codec::{self, CdxObject};
use super::reaction_table::{edit_cell, new_table};
use super::{ArrowKind, BondOrder, Molecule, PageObject, PageText, TextAlign};
use crate::config::StyleConfig;
use crate::reaction::edit::CellRef;
use crate::reaction::BlockKind;
use crate::widget::scene::{
    build_object, build_table, RenderTransform, ARROW_HEAD_LENGTH, TABLE_BASELINE, TABLE_CELL_PAD, TABLE_FRAME_TOP,
    TABLE_ROW_HEIGHT,
};
use crate::widget::{DEFAULT_BOND_LENGTH, SCALE_FACTOR};

const OBJ_GRAPHIC: u16 = 0x8007;
const OBJ_ARROW: u16 = 0x8021;
const OBJ_GRID: u16 = 0x8022;
const OBJ_COMPONENT: u16 = 0x8023;
const OBJ_DATUM: u16 = 0x8024;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

/// ChemDraw 25's `ReactionAnalysis` document, as CDXML.
fn chemdraw() -> String {
    fixture("chemdraw25_reaction_analysis.cdxml")
}

/// The reaction of the report: 1-chloro-2-propanol (C3H7ClO, 4.00 g) to methyloxirane with
/// NaH over the arrow, and its reaction table under it.
fn reaction() -> Molecule {
    let mut m = Molecule::default();
    let bond = |m: &mut Molecule, a: u32, b: u32| {
        m.add_bond(a, b, BondOrder::Single);
    };
    let cl = m.add_atom("Cl".into(), [2.0, 5.0], 0);
    let c1 = m.add_atom("C".into(), [3.3, 4.25], 0);
    let c2 = m.add_atom("C".into(), [4.6, 5.0], 0);
    let c3 = m.add_atom("C".into(), [5.9, 4.25], 0);
    let o = m.add_atom("O".into(), [4.6, 6.5], 0);
    bond(&mut m, cl, c1);
    bond(&mut m, c1, c2);
    bond(&mut m, c2, c3);
    bond(&mut m, c2, o);
    m.add_arrow([8.0, 5.0], [12.0, 5.0], ArrowKind::Reaction);
    m.push_object(PageObject::Text(PageText { id: 0, pos: [10.0, 4.4], text: "NaH".into(), align: TextAlign::Center, source: None, color: None }));
    let r1 = m.add_atom("C".into(), [14.0, 5.0], 0);
    let r2 = m.add_atom("C".into(), [15.5, 5.0], 0);
    let ro = m.add_atom("O".into(), [14.75, 3.7], 0);
    let me = m.add_atom("C".into(), [16.25, 6.3], 0);
    bond(&mut m, r1, r2);
    bond(&mut m, r2, ro);
    bond(&mut m, ro, r1);
    bond(&mut m, r2, me);
    let t = new_table(&m, None).unwrap();
    let t = edit_cell(&m, &t, CellRef::new(BlockKind::Reactants, 0, 4), "4.00 g").unwrap().unwrap();
    m.push_object(PageObject::Table(t));
    m
}

fn all(o: &CdxObject, tag: u16) -> Vec<&CdxObject> {
    fn walk<'a>(o: &'a CdxObject, tag: u16, out: &mut Vec<&'a CdxObject>) {
        if o.tag == tag {
            out.push(o);
        }
        o.children.iter().for_each(|c| walk(c, tag, out));
    }
    let mut out = Vec::new();
    walk(o, tag, &mut out);
    out
}

/// A CDXML element's attributes (the first element so named), as written.
fn element_attrs(xml: &str, element: &str) -> HashMap<String, String> {
    let at = xml.find(&format!("<{element}")).unwrap_or_else(|| panic!("no <{element}"));
    let tag = &xml[at + element.len() + 1..];
    let tag = &tag[..tag.find('>').unwrap()];
    let mut out = HashMap::new();
    let mut rest = tag;
    while let Some(eq) = rest.find("=\"") {
        let name = rest[..eq].split_whitespace().last().unwrap().to_string();
        let value_end = rest[eq + 2..].find('"').unwrap();
        out.insert(name, rest[eq + 2..eq + 2 + value_end].to_string());
        rest = &rest[eq + 2 + value_end + 1..];
    }
    out
}

/// The run sizes and fonts (`font size`) of every `<s>` in a CDXML text.
fn run_sizes(xml: &str) -> Vec<(String, String)> {
    xml.split("<s ")
        .skip(1)
        .map(|s| {
            let tag = &s[..s.find('>').unwrap()];
            let attr = |n: &str| tag.split(&format!("{n}=\"")).nth(1).unwrap().split('"').next().unwrap().to_string();
            (attr("font"), attr("size"))
        })
        .collect()
}

fn point(v: &str) -> Vec<f64> {
    v.split_whitespace().map(|n| n.parse().unwrap()).collect()
}

/// Every attribute that decides how ChemDraw draws an arrow, a text and the document's
/// structures is what ChemDraw itself writes.
#[test]
fn written_attributes_are_chemdraws() {
    let ours = molecule_to_cdxml(&reaction());
    let theirs = chemdraw();
    // The document's drawing style.
    let (doc, cd) = (element_attrs(&ours, "CDXML"), element_attrs(&theirs, "CDXML"));
    for name in ["LabelFont", "LabelSize", "LabelFace", "CaptionFont", "CaptionSize", "BondSpacing", "ChainAngle"] {
        assert_eq!(doc[name], cd[name], "{name}");
    }
    for name in ["LineWidth", "BoldWidth", "BondLength", "HashSpacing", "MarginWidth"] {
        let (a, b) = (point(&doc[name])[0], point(&cd[name])[0]);
        assert!((a - b).abs() < 0.01, "{name}: {a} vs ChemDraw's {b}");
    }
    // Atom labels and the text over the arrow: 10 pt Arial, as every caption ChemDraw writes.
    let fonts = cdx_fonts_named(&ours);
    let runs = run_sizes(&ours[..ours.find("<stoichiometrygrid").unwrap()]);
    assert!(runs.len() >= 3, "Cl, OH, O and NaH");
    for (font, size) in &runs {
        assert_eq!((fonts[font].as_str(), size.as_str()), ("Arial", "10"));
    }
    // The arrow, and the line graphic it supersedes.
    let (a, ca) = (element_attrs(&ours, "arrow"), element_attrs(&theirs, "arrow"));
    for name in ["FillType", "ArrowheadHead", "ArrowheadType", "HeadSize", "ArrowheadCenterSize", "ArrowheadWidth"] {
        assert_eq!(a.get(name), ca.get(name), "arrow {name}");
    }
    assert!(!a.contains_key("LineWidth") && !ca.contains_key("LineWidth"), "both drawn in the document's line width");
    let (g, cg) = (element_attrs(&ours, "graphic"), element_attrs(&theirs, "graphic"));
    for name in ["GraphicType", "ArrowType", "HeadSize"] {
        assert_eq!(g.get(name), cg.get(name), "graphic {name}");
    }
    // The reaction table: the cells where ChemDraw puts them, from the grid's corner.
    let offsets = |xml: &str| -> Vec<(f64, f64)> {
        let doc = cdxml_document(xml).unwrap();
        let grid = all(&doc, OBJ_GRID)[0].clone();
        let corner = point_of(&grid);
        let header = grid.children.iter().find(|c| c.tag == OBJ_COMPONENT).unwrap();
        header
            .children
            .iter()
            .filter(|d| d.tag == OBJ_DATUM)
            .map(|d| point_of(&d.children[0].children[0]))
            .filter(|p| *p != (0.0, 0.0))
            .map(|p| ((p.0 - corner.0) / PT, (p.1 - corner.1) / PT))
            .collect()
    };
    let (ours_cells, theirs_cells) = (offsets(&ours), offsets(&theirs));
    assert_eq!(ours_cells.len(), theirs_cells.len());
    for (a, b) in ours_cells.iter().zip(&theirs_cells) {
        assert!((a.0 - b.0).abs() < 0.02 && (a.1 - b.1).abs() < 0.02, "label at {a:?}, ChemDraw's at {b:?}");
    }
    assert!((theirs_cells[0].0 - TABLE_CELL_PAD as f64).abs() < 0.02);
    assert!((theirs_cells[0].1 - (TABLE_FRAME_TOP + TABLE_BASELINE) as f64).abs() < 0.02);
    assert!((theirs_cells[1].1 - theirs_cells[0].1 - TABLE_ROW_HEIGHT as f64).abs() < 0.02);
    // Column widths in CDX units, as ChemDraw spells them.
    let widths = component_widths(&ours);
    assert_eq!(widths.len(), 4, "two headers, the reactant's column and the product's");
    for w in &widths {
        assert!(w.parse::<i64>().is_ok_and(|w| w > 20 * 65536), "Width=\"{w}\"");
    }
}

/// The `Width` of every `sgcomponent` of a CDXML text, as written.
fn component_widths(xml: &str) -> Vec<String> {
    xml.split("<sgcomponent").skip(1).map(|s| element_attrs(&format!("<sgcomponent{s}"), "sgcomponent")["Width"].clone()).collect()
}

fn point_of(o: &CdxObject) -> (f64, f64) {
    let v = o.prop(cdx::PROP_2D_POSITION).unwrap();
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    (n(4), n(0))
}

/// The font table of a CDXML text: id to name.
fn cdx_fonts_named(xml: &str) -> HashMap<String, String> {
    xml.split("<font ")
        .skip(1)
        .map(|f| {
            let attr = |n: &str| f.split(&format!("{n}=\"")).nth(1).unwrap().split('"').next().unwrap().to_string();
            (attr("id"), attr("name"))
        })
        .collect()
}

/// What decides how one object of a document looks: what must be the same exactly (its runs,
/// justification, arrowhead, a column's width), its anchor points and its box, in points.
#[derive(Debug, Clone)]
struct Look {
    tag: u16,
    /// Whether it is a structure's: a fragment, or an atom's label.
    structure: bool,
    exact: String,
    points: Vec<f64>,
    bbox: Vec<f64>,
}

/// Every object of a CDXML document that draws something, in a stable order. Ids and the order
/// of objects in the file are left out.
fn look(xml: &str) -> Vec<Look> {
    fn coords(v: &[u8]) -> Vec<f64> {
        v.as_chunks::<4>().0.iter().map(|b| i32::from_le_bytes(*b) as f64 / PT).collect()
    }
    fn walk(o: &CdxObject, structure: bool, out: &mut Vec<Look>) {
        // Text and its runs, justification, the arrowhead's shape and size, a column's width.
        const EXACT: [u16; 8] = [0x0700, 0x0701, 0x0A20, 0x0A2F, 0x0A30, 0x0A31, 0x0A35, 0x0812];
        // Position, head and tail.
        const POINTS: [u16; 3] = [0x0200, 0x0207, 0x0208];
        let exact: Vec<String> =
            o.props.iter().filter(|(t, _)| EXACT.contains(t)).map(|(t, v)| format!("{t:04x}={v:?}")).collect();
        let points: Vec<f64> = o.props.iter().filter(|(t, _)| POINTS.contains(t)).flat_map(|(_, v)| coords(v)).collect();
        let bbox = o.prop(cdx::PROP_BOUNDING_BOX).map(coords).unwrap_or_default();
        let structure = structure || o.tag == 0x8003;
        if (!exact.is_empty() || !points.is_empty()) && o.tag != 0x8000 && o.tag != 0x8001 {
            out.push(Look { tag: o.tag, structure, exact: exact.join(" "), points, bbox });
        }
        o.children.iter().for_each(|c| walk(c, structure, out));
    }
    let mut out = Vec::new();
    walk(&cdxml_document(xml).unwrap(), false, &mut out);
    out.sort_by_key(|l| (l.tag, l.exact.clone(), l.points.iter().map(|v| (v * 2.0).round() as i64).collect::<Vec<_>>()));
    out
}

/// `a` and `b` look the same: the same objects, each with the same text, runs and arrowheads,
/// and its anchor points within 0.02 pt (a coordinate kept in molecule units comes back within
/// a few 1/65536 pt). A box is ChemDraw's cache of what it draws; a structure's is estimated
/// when it is written anew (Arial's widths over a 7.2 pt ascent and 2.1 pt descent, where
/// ChemDraw caches the glyphs' own extent), so it is held to 2.5 pt, every other to 0.02.
#[track_caller]
fn assert_same_look(a: &[Look], b: &[Look], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: as many objects");
    let near = |p: &[f64], q: &[f64], tol: f64| p.len() == q.len() && p.iter().zip(q).all(|(u, v)| (u - v).abs() <= tol);
    for (x, y) in a.iter().zip(b) {
        assert_eq!((x.tag, &x.exact), (y.tag, &y.exact), "{what}: {x:?} vs {y:?}");
        assert!(near(&x.points, &y.points, 0.02), "{what}: {x:?} vs {y:?}");
        let tol = if x.structure { 2.5 } else { 0.02 };
        assert!(near(&x.bbox, &y.bbox, tol), "{what}: box {x:?} vs {y:?}");
    }
}

/// Written, read back and written again, the drawing is written the same.
#[test]
fn a_drawing_written_and_read_back_is_written_the_same() {
    let first = molecule_to_cdxml(&reaction());
    let read = read_cdxml(&first).unwrap().molecule;
    let second = molecule_to_cdxml(&read);
    assert_same_look(&look(&second), &look(&first), "written again");
    // And saved into the document it was read from, as the app saves an opened file.
    let original = cdx_codec::write(&cdxml_document(&first).unwrap());
    let merged = cdxml_of_document(&cdx_codec::parse(&merge_into_cdx(&original, &read)).unwrap());
    assert_same_look(&look(&merged), &look(&first), "saved into its document");
}

/// A ChemDraw document opened and saved as the app does it is the document ChemDraw wrote, in
/// everything that decides its look.
#[test]
fn a_chemdraw_document_opened_and_saved_keeps_its_look() {
    let theirs = chemdraw();
    let original = cdx_codec::write(&cdxml_document(&theirs).unwrap());
    let read = read_cdxml(&theirs).unwrap().molecule;
    let saved = cdxml_of_document(&cdx_codec::parse(&merge_into_cdx(&original, &read)).unwrap());
    assert_same_look(&look(&saved), &look(&cdxml_of_document(&cdxml_document(&theirs).unwrap())), "opened and saved");
    assert_eq!(component_widths(&saved), component_widths(&theirs));
    assert_eq!(component_widths(&theirs), ["5534515", "5639372", "3968204", "3968204"]);
}

/// Editor pixels per point of a document: its coordinates are read at 14.4 pt to a
/// [`DEFAULT_BOND_LENGTH`] bond, drawn [`SCALE_FACTOR`] pixels to the unit.
fn px_per_pt() -> f32 {
    (SCALE_FACTOR as f64 * 65536.0 / SCALE) as f32
}

fn fonts() -> egui::epaint::text::Fonts {
    egui::epaint::text::Fonts::new(egui::epaint::text::TextOptions::default(), egui::FontDefinitions::default())
}

/// ChemDraw's document drawn by the editor in its default style: the arrowhead and the table's
/// rows and columns where ChemDraw draws them, and text at ChemDraw's size.
#[test]
fn the_editor_draws_a_chemdraw_document_in_chemdraws_proportions() {
    let m = read_cdxml(&chemdraw()).unwrap().molecule;
    let style = StyleConfig::default();
    let xf = RenderTransform { coord_scale: SCALE_FACTOR, offset: egui::Vec2::ZERO, visual_scale: 1.0 };
    let mut fonts = fonts();
    let mut view = fonts.with_pixels_per_point(1.0);
    let pt = px_per_pt();
    assert!((pt - SCALE_FACTOR * DEFAULT_BOND_LENGTH / 14.4).abs() < 1e-4);
    assert!((style.label_size / pt - 10.0).abs() < 1e-4, "atom labels and texts at 10 pt");

    // The arrowhead: HeadSize="2250" hundredths of the 0.6 pt line, 13.5 pt long.
    let arrow = m.objects.iter().find(|o| o.as_arrow().is_some()).unwrap();
    let a = arrow.as_arrow().unwrap();
    let scene = build_object(arrow, &m, &style, xf, |job| view.layout_job(job));
    let head = &scene.fills[0];
    let tip = xf.apply(a.head);
    let back = head.iter().map(|p| (tip.x - p.x).abs()).fold(0.0, f32::max);
    assert!((back / pt - 13.5).abs() < 0.01, "head {} pt long", back / pt);
    assert!((back / pt - ARROW_HEAD_LENGTH * 0.6).abs() < 0.01);
    assert!((scene.strokes[0].width / pt - 0.6).abs() < 1e-4, "a 0.6 pt shaft");

    // The table: each value cell's row as ChemDraw's (a 15 pt row, the first baseline at
    // 125.46 pt), its value column as ChemDraw's — within a point or two, as egui's font is not
    // Arial.
    let table = m.objects.iter().find_map(PageObject::as_table).unwrap();
    let t = build_table(table, &m, &style, xf, |job| view.layout_job(job));
    let at = |kind: BlockKind, row: usize| {
        t.cells.iter().find(|(c, _)| *c == CellRef::new(kind, 0, row)).map(|(_, r)| *r).unwrap()
    };
    for row in 0..11 {
        let r = at(BlockKind::Reactants, row);
        let baseline = (r.min.y + TABLE_BASELINE * pt) / pt;
        assert!((baseline - (125.46 + 15.0 * row as f32)).abs() < 0.02, "row {row}: baseline {baseline} pt");
        assert!((r.height() / pt - 15.0).abs() < 1e-3);
    }
    // ChemDraw's value columns: the reactant's from 134.81 pt (its text at 142.31), the
    // product's from 283.29 pt (its text at 290.84).
    let reactant = at(BlockKind::Reactants, 0);
    let product = at(BlockKind::Products, 0);
    assert!((reactant.min.x / pt - 134.81).abs() < 2.5, "reactant column at {} pt", reactant.min.x / pt);
    assert!((product.min.x / pt - 283.29).abs() < 5.0, "product column at {} pt", product.min.x / pt);
}

/// Writes the files to open in ChemDraw for a look at the result (not run by default):
/// `cargo test --lib write_chemdraw_check_files -- --ignored`, then open
/// `target/ope7-check/*.cdxml` in ChemDraw.
#[test]
#[ignore]
fn write_chemdraw_check_files() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/ope7-check");
    std::fs::create_dir_all(&dir).unwrap();
    // Drawn here: the reaction of the report, with NaH over the arrow and its table.
    std::fs::write(dir.join("drawn_here.cdxml"), molecule_to_cdxml(&reaction())).unwrap();
    // ChemDraw's own document, opened and saved as the app does it.
    let theirs = chemdraw();
    let original = cdx_codec::write(&cdxml_document(&theirs).unwrap());
    let read = read_cdxml(&theirs).unwrap().molecule;
    let saved = cdxml_of_document(&cdx_codec::parse(&merge_into_cdx(&original, &read)).unwrap());
    std::fs::write(dir.join("chemdraw_resaved.cdxml"), saved).unwrap();
    // The same reaction as a picture drawn by the editor's renderer, to set beside ChemDraw.
    #[cfg(windows)]
    if let Some(png) = super::image::molecule_to_png(&reaction()) {
        std::fs::write(dir.join("drawn_here.png"), png).unwrap();
    }
}

/// Every arrow, graphic and grid of the written reaction is there once.
#[test]
fn the_reaction_is_written_once() {
    let doc = cdx_codec::parse(&cdx::molecule_to_cdx_document(&reaction())).unwrap();
    assert_eq!((all(&doc, OBJ_ARROW).len(), all(&doc, OBJ_GRAPHIC).len(), all(&doc, OBJ_GRID).len()), (1, 1, 1));
}
