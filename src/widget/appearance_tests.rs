//! What the canvas paints for the appearance settings (OPE-16): a bond's line colour, an atom
//! label's and a page text's colour — each independent of the others — and the drop shadow,
//! painted before what casts it.

use super::ChemStructEditor;
use crate::molecule::{BondOrder, PageObject, Shadow};
use egui::{Color32, Shape};

const RED: [u8; 4] = [220, 20, 30, 255];
const BLUE: [u8; 4] = [10, 40, 230, 255];
const GREEN: [u8; 4] = [0, 128, 0, 255];

fn c32([r, g, b, a]: [u8; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Every painted shape, groups flattened, in paint order.
fn painted(editor: &mut ChemStructEditor) -> Vec<Shape> {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))),
        ..Default::default()
    };
    let output = crate::testkit::run_ui(&ctx, raw, |ui| {
        editor.ui(ui);
    });
    fn flatten(shape: &Shape, out: &mut Vec<Shape>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
            s => out.push(s.clone()),
        }
    }
    let mut out = Vec::new();
    for clipped in &output.shapes {
        flatten(&clipped.shape, &mut out);
    }
    out
}

/// The colour a text shape draws `text` in: its override, else its first section's.
fn text_color(shape: &Shape, text: &str) -> Option<Color32> {
    match shape {
        Shape::Text(t) if t.galley.job.text == text => {
            Some(t.override_text_color.unwrap_or_else(|| t.galley.job.sections[0].format.color))
        }
        _ => None,
    }
}

fn line_color(shape: &Shape) -> Option<Color32> {
    match shape {
        Shape::LineSegment { stroke, .. } => Some(stroke.color),
        _ => None,
    }
}

/// Ethanol with its C–O bond red, its O blue, and a green text.
fn coloured() -> ChemStructEditor {
    let mut e = ChemStructEditor::default();
    let c1 = e.molecule.add_atom("C".into(), [0.0, 0.0], 0);
    let c2 = e.molecule.add_atom("C".into(), [1.3, 0.75], 0);
    let o = e.molecule.add_atom("O".into(), [2.6, 0.0], 0);
    e.molecule.add_bond(c1, c2, BondOrder::Single);
    let co = e.molecule.add_bond(c2, o, BondOrder::Single);
    e.molecule.bond_by_id_mut(co).unwrap().color = Some(RED);
    e.molecule.atom_by_id_mut(o).unwrap().color = Some(BLUE);
    let t = e.molecule.add_text([0.0, 3.0], "THF");
    if let Some(PageObject::Text(t)) = e.molecule.object_by_id_mut(t) {
        t.color = Some(GREEN);
    }
    e
}

#[test]
fn the_canvas_paints_each_colour_where_it_was_set() {
    let mut e = coloured();
    let shapes = painted(&mut e);
    let lines: Vec<Color32> = shapes.iter().filter_map(line_color).collect();
    assert!(lines.contains(&c32(RED)), "the C–O bond is red: {lines:?}");
    assert!(lines.contains(&Color32::BLACK), "the C–C bond stays black: {lines:?}");
    assert!(!lines.contains(&c32(BLUE)), "the label's colour is not the bond's");
    let o: Vec<Color32> = shapes.iter().filter_map(|s| text_color(s, "OH")).collect();
    assert_eq!(o, vec![c32(BLUE)], "the O's label is blue, its bond notwithstanding");
    let thf: Vec<Color32> = shapes.iter().filter_map(|s| text_color(s, "THF")).collect();
    assert_eq!(thf, vec![c32(GREEN)]);
}

#[test]
fn the_scene_carries_the_colours_for_every_backend() {
    let e = coloured();
    let mut fonts = crate::widget::fonts::standalone_fonts();
    let mut view = fonts.with_pixels_per_point(1.0);
    let xf = super::scene::RenderTransform { coord_scale: super::SCALE_FACTOR, offset: egui::Vec2::ZERO, visual_scale: 1.0 };
    let scene = super::scene::MoleculeScene::build(&e.molecule, &e.config.style, xf, |job| view.layout_job(job));
    let colours: Vec<Option<[u8; 4]>> = scene.bonds.iter().map(|b| b.color).collect();
    assert!(colours.contains(&Some(RED)) && colours.contains(&None), "{colours:?}");
    let o = e.molecule.atoms.iter().find(|a| a.element == "O").unwrap().id;
    assert!(scene.labels[&o].galley.glyph_placements().iter().all(|g| g.color == c32(BLUE)));
    let text = &scene.objects[0].texts[0];
    assert!(text.galley.glyph_placements().iter().all(|g| g.color == c32(GREEN)));
    assert!(scene.shadow.is_none());
}

#[test]
fn the_shadow_is_painted_first_in_its_colour_and_offset() {
    let mut e = coloured();
    let shadow = Shadow { color: [40, 50, 60, 200], offset: [0.2, 0.3], blur: 0.0 };
    e.molecule.shadow = Some(shadow);
    let shapes = painted(&mut e);
    let tint = c32(shadow.color);
    let first_shadow = shapes.iter().position(|s| line_color(s) == Some(tint)).expect("bond shadows");
    let first_bond = shapes.iter().position(|s| line_color(s) == Some(Color32::BLACK)).unwrap();
    assert!(first_shadow < first_bond, "the shadow goes under the bonds");
    // The shadow of the red bond is that bond moved by the offset (zoom 1: SCALE_FACTOR px/unit).
    let segment = |s: &Shape| match s {
        Shape::LineSegment { points, stroke } => Some((points[0], points[1], stroke.color)),
        _ => None,
    };
    let red = shapes.iter().filter_map(segment).find(|(.., c)| *c == c32(RED)).unwrap();
    let d = egui::vec2(shadow.offset[0], shadow.offset[1]) * super::SCALE_FACTOR * e.zoom;
    assert!(
        shapes.iter().filter_map(segment).any(|(a, b, c)| c == tint && (a - (red.0 + d)).length() < 0.01 && (b - (red.1 + d)).length() < 0.01),
        "a shadow of the red bond, offset by {d:?}"
    );
    // The labels cast one too, painted before the labels themselves.
    let label_shadow = shapes.iter().position(|s| text_color(s, "OH") == Some(tint)).expect("a label shadow");
    let label = shapes.iter().position(|s| text_color(s, "OH") == Some(c32(BLUE))).unwrap();
    assert!(label_shadow < label);
    // Page texts cast none.
    assert!(!shapes.iter().any(|s| text_color(s, "THF") == Some(tint)));
}

#[test]
fn a_blurred_shadow_fades_out_in_layers() {
    let mut e = coloured();
    e.molecule.shadow = Some(Shadow { blur: 0.2, ..Shadow::default() });
    let shapes = painted(&mut e);
    let widths: Vec<f32> = shapes
        .iter()
        .filter_map(|s| match s {
            Shape::LineSegment { stroke, .. } if stroke.color != Color32::BLACK && stroke.color != c32(RED) => Some(stroke.width),
            _ => None,
        })
        .collect();
    let (min, max) = widths.iter().fold((f32::MAX, 0.0f32), |(lo, hi), &w| (lo.min(w), hi.max(w)));
    assert!(max > min + 10.0, "layers spread over the blur radius (10 px): {min}..{max}");
}

#[test]
fn no_shadow_paints_nothing_extra() {
    let mut plain = coloured();
    let mut shadowed = coloured();
    shadowed.molecule.shadow = Some(Shadow::default());
    assert!(painted(&mut shadowed).len() > painted(&mut plain).len());
}
