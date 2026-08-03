//! The reaction scheme: structures, `+` separators and an arrow with text above and below,
//! laid out on one horizontal reaction line.
//!
//! Structures are drawn by the *editor's own* renderer ([`crate::widget::draw`]) so a
//! structure in a scheme looks exactly like the same structure in the editor: same label
//! anchoring, same H folding, same bond trimming, same double-bond placement. The renderer
//! works off a [`ChemStructEditor`], so a throw-away one is used as a drawing context; the
//! molecule is scaled into it instead of zooming, which keeps line widths and label sizes
//! absolute (see [`ReactionStyle::structure_scale`]).

use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};

use crate::config::StyleConfig;
use crate::molecule::Molecule;
use crate::widget::rich_text::{RichText, RichTextStyle};
use crate::widget::{ChemStructEditor, draw as structure_draw};

use super::style::ReactionStyle;
use super::text::{Run, RunLayout, layout_runs};

/// One element of a reaction scheme, drawn left to right.
#[derive(Clone, Debug)]
pub enum SchemeItem {
    /// A drawn structure, centred on the reaction line.
    Structure(Molecule),
    /// A `+` between two reagents.
    Plus,
    /// The reaction arrow, with its conditions above and below.
    Arrow(Arrow),
    /// Free-standing text on the reaction line (a reagent written out, `Δ`, a label …).
    Text(RichText),
}

/// A reaction arrow and the text riding on it.
#[derive(Clone, Debug, Default)]
pub struct Arrow {
    /// Text above the shaft (reagents).
    pub above: RichText,
    /// Text below the shaft (base, solvent, temperature).
    pub below: RichText,
    /// Explicit shaft length in pixels. `None` uses [`ReactionStyle::arrow_length`], widened
    /// when necessary to fit the text.
    pub length: Option<f32>,
}

impl Arrow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Text above the shaft. `&str` goes through the [rich-text markup](crate::RichText)
    /// parser, so `"K_2CO_3"` gets real subscripts.
    pub fn above(mut self, text: impl Into<RichText>) -> Self {
        self.above = text.into();
        self
    }

    /// Text below the shaft.
    pub fn below(mut self, text: impl Into<RichText>) -> Self {
        self.below = text.into();
        self
    }

    pub fn length(mut self, px: f32) -> Self {
        self.length = Some(px);
        self
    }
}

/// A reaction scheme: a sequence of [`SchemeItem`]s.
///
/// ```no_run
/// use chembuider_rs::reaction::{Arrow, ReactionScheme, demo};
/// let scheme = ReactionScheme::new()
///     .structure(demo::phenol())
///     .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
///     .structure(demo::anisole());
/// ```
#[derive(Clone, Debug, Default)]
pub struct ReactionScheme {
    pub items: Vec<SchemeItem>,
}

impl ReactionScheme {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn item(mut self, item: SchemeItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn structure(self, molecule: Molecule) -> Self {
        self.item(SchemeItem::Structure(molecule))
    }

    pub fn plus(self) -> Self {
        self.item(SchemeItem::Plus)
    }

    pub fn arrow(self, arrow: Arrow) -> Self {
        self.item(SchemeItem::Arrow(arrow))
    }

    pub fn text(self, text: impl Into<RichText>) -> Self {
        self.item(SchemeItem::Text(text.into()))
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

// ─── Layout ──────────────────────────────────────────────────────────────────

/// A measured scheme: where every item goes, relative to the scheme's top-left.
pub struct SchemeLayout {
    /// One entry per item of the scheme, in order.
    pub items: Vec<PlacedItem>,
    /// Tight size of the whole scheme.
    pub size: Vec2,
    /// Distance from the top to the reaction line (the arrow shaft).
    pub center_line: f32,
}

/// One measured item: its box, plus whatever the paint pass needs to draw it.
pub struct PlacedItem {
    /// Box of this item, relative to the scheme's top-left.
    pub rect: Rect,
    pub kind: PlacedKind,
}

/// Paint-ready form of a measured item.
pub enum PlacedKind {
    /// Where the molecule's coordinate origin lands, relative to the scheme's top-left.
    Structure { origin: Vec2 },
    /// Pre-laid-out text (`+`, a free text item).
    Text(RunLayout),
    /// Arrow shaft length plus the pre-laid-out text above and below it.
    Arrow {
        length: f32,
        above: RunLayout,
        below: RunLayout,
    },
}

/// Measure a scheme. `scratch` is a drawing context for structures; its molecule and
/// transform are overwritten.
pub fn measure_scheme(
    scheme: &ReactionScheme,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) -> SchemeLayout {
    // (width, extent above the line, extent below the line, paint data)
    let mut measured: Vec<(f32, f32, f32, PlacedKind)> = Vec::with_capacity(scheme.items.len());

    for item in &scheme.items {
        match item {
            SchemeItem::Structure(mol) => {
                let bounds = structure_bounds(mol, painter, style, structure_style, scratch);
                let half = bounds.height() * 0.5;
                // Origin offset inside the item box: the box starts at the bounds' top-left.
                let origin = Vec2::new(-bounds.min.x, -bounds.min.y);
                measured.push((bounds.width(), half, half, PlacedKind::Structure { origin }));
            }
            SchemeItem::Plus => {
                let layout = text_layout(&RichText::plain("+"), style.plus_font_size, painter);
                let half = layout.size.y * 0.5;
                measured.push((layout.size.x, half, half, PlacedKind::Text(layout)));
            }
            SchemeItem::Text(text) => {
                let layout = text_layout(text, style.scheme_font_size, painter);
                let half = layout.size.y * 0.5;
                measured.push((layout.size.x, half, half, PlacedKind::Text(layout)));
            }
            SchemeItem::Arrow(arrow) => {
                let above = text_layout(&arrow.above, style.arrow_font_size, painter);
                let below = text_layout(&arrow.below, style.arrow_font_size, painter);
                let text_width = above.size.x.max(below.size.x);
                let length = arrow
                    .length
                    .unwrap_or_else(|| style.arrow_length.max(text_width + 2.0 * style.item_gap));
                // Half the head is the arrow's own extent; text stacks on top of that.
                let half_shaft = (style.arrow_width * 0.5).max(style.arrow_head_half_width);
                let extent = |t: &RunLayout| {
                    if t.is_empty() {
                        half_shaft
                    } else {
                        half_shaft + style.arrow_text_gap + t.size.y
                    }
                };
                measured.push((
                    length,
                    extent(&above),
                    extent(&below),
                    PlacedKind::Arrow { length, above, below },
                ));
            }
        }
    }

    if measured.is_empty() {
        return SchemeLayout { items: Vec::new(), size: Vec2::ZERO, center_line: 0.0 };
    }

    let center_line = measured.iter().map(|m| m.1).fold(0.0_f32, f32::max);
    let below = measured.iter().map(|m| m.2).fold(0.0_f32, f32::max);
    let mut items = Vec::with_capacity(measured.len());
    let mut pen_x = 0.0_f32;
    for (i, (width, above, item_below, kind)) in measured.into_iter().enumerate() {
        if i > 0 {
            pen_x += style.item_gap;
        }
        let rect = Rect::from_min_size(
            Pos2::new(pen_x, center_line - above),
            Vec2::new(width, above + item_below),
        );
        let kind = match kind {
            // Shift the structure's origin from item-box-relative to scheme-relative.
            PlacedKind::Structure { origin } => PlacedKind::Structure {
                origin: origin + rect.min.to_vec2(),
            },
            other => other,
        };
        items.push(PlacedItem { rect, kind });
        pen_x += width;
    }

    SchemeLayout {
        items,
        size: Vec2::new(pen_x, center_line + below),
        center_line,
    }
}

/// Paint a measured scheme with its top-left corner at `top_left`.
pub fn paint_scheme(
    scheme: &ReactionScheme,
    layout: &SchemeLayout,
    painter: &Painter,
    top_left: Pos2,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) {
    let line_y = top_left.y + layout.center_line;
    for (item, placed) in scheme.items.iter().zip(&layout.items) {
        let rect = placed.rect.translate(top_left.to_vec2());
        match (&placed.kind, item) {
            (PlacedKind::Structure { origin }, SchemeItem::Structure(mol)) => {
                draw_structure(
                    mol,
                    painter,
                    top_left + *origin,
                    style,
                    structure_style,
                    scratch,
                );
            }
            (PlacedKind::Text(text), _) => {
                text.paint(painter, Pos2::new(rect.min.x, line_y - text.size.y * 0.5));
            }
            (PlacedKind::Arrow { length, above, below }, _) => {
                draw_arrow(painter, Pos2::new(rect.min.x, line_y), *length, style);
                let half_shaft = (style.arrow_width * 0.5).max(style.arrow_head_half_width);
                if !above.is_empty() {
                    let x = rect.min.x + (*length - above.size.x) * 0.5;
                    let y = line_y - half_shaft - style.arrow_text_gap - above.size.y;
                    above.paint(painter, Pos2::new(x, y));
                }
                if !below.is_empty() {
                    let x = rect.min.x + (*length - below.size.x) * 0.5;
                    let y = line_y + half_shaft + style.arrow_text_gap;
                    below.paint(painter, Pos2::new(x, y));
                }
            }
            // Measured and painted items are built from the same list, so the pairs always
            // match; a mismatch would mean the layout is stale — draw nothing rather than panic.
            _ => {}
        }
    }
}

/// A right-pointing reaction arrow with a filled head, `tail` sitting on the reaction line.
pub fn draw_arrow(painter: &Painter, tail: Pos2, length: f32, style: &ReactionStyle) {
    let tip = Pos2::new(tail.x + length, tail.y);
    let head_base = tip.x - style.arrow_head_length;
    // Stop the shaft inside the head so the two never disagree at the tip.
    painter.line_segment(
        [tail, Pos2::new(head_base + style.arrow_head_length * 0.5, tail.y)],
        Stroke::new(style.arrow_width, Color32::BLACK),
    );
    painter.add(egui::Shape::convex_polygon(
        vec![
            tip,
            Pos2::new(head_base, tail.y - style.arrow_head_half_width),
            Pos2::new(head_base, tail.y + style.arrow_head_half_width),
        ],
        Color32::BLACK,
        Stroke::NONE,
    ));
}

// ─── Structures ──────────────────────────────────────────────────────────────

/// Load `mol` into the throw-away editor, scaled so one bond is
/// [`ReactionStyle::bond_length`] pixels long at zoom 1.
fn load(mol: &Molecule, style: &ReactionStyle, structure_style: &StyleConfig, scratch: &mut ChemStructEditor) {
    let k = style.structure_scale();
    scratch.molecule = mol.clone();
    for atom in &mut scratch.molecule.atoms {
        atom.pos = [atom.pos[0] * k, atom.pos[1] * k];
    }
    scratch.zoom = 1.0;
    scratch.pan = Vec2::ZERO;
    scratch.config.style = structure_style.clone();
    // No editing state may leak into a static drawing.
    scratch.selected_atoms.clear();
    scratch.hovered_atom = None;
    scratch.hovered_bond = None;
    scratch.hotspot_atom = None;
    scratch.hotspot_bond = None;
}

/// Screen-space extent of a structure — atoms *and* their labels — relative to the
/// molecule's coordinate origin.
pub fn structure_bounds(
    mol: &Molecule,
    painter: &Painter,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) -> Rect {
    load(mol, style, structure_style, scratch);
    let labels = structure_draw::compute_label_layouts(scratch, painter, Pos2::ZERO);
    let mut bounds: Option<Rect> = None;
    let mut grow = |r: Rect| {
        bounds = Some(bounds.map_or(r, |b: Rect| b.union(r)));
    };
    for atom in &scratch.molecule.atoms {
        if structure_draw::is_folded_h(&scratch.molecule, atom.id) {
            continue;
        }
        let p = scratch.mol_to_screen(atom.pos, Pos2::ZERO);
        grow(Rect::from_center_size(p, Vec2::splat(structure_style.bond_width.max(1.0))));
    }
    for label in labels.values() {
        grow(label.rect);
    }
    bounds.unwrap_or(Rect::from_min_size(Pos2::ZERO, Vec2::ZERO))
}

/// Draw a structure with its coordinate origin at `origin`, through the editor's renderer.
pub fn draw_structure(
    mol: &Molecule,
    painter: &Painter,
    origin: Pos2,
    style: &ReactionStyle,
    structure_style: &StyleConfig,
    scratch: &mut ChemStructEditor,
) {
    load(mol, style, structure_style, scratch);
    let labels = structure_draw::compute_label_layouts(scratch, painter, origin);
    structure_draw::draw_bonds(scratch, painter, origin, &labels);
    structure_draw::draw_atom_backgrounds(scratch, painter, origin, &labels);
    structure_draw::draw_atom_labels(scratch, painter, origin, &labels);
}

fn text_layout(text: &RichText, size: f32, painter: &Painter) -> RunLayout {
    if text.is_empty() {
        return RunLayout::EMPTY;
    }
    let style = RichTextStyle::new(size, Color32::BLACK);
    let runs = [Run::new(text.clone())];
    layout_runs(&runs, &style, 0.0, |job| painter.layout_job(job))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;

    /// Run `f` inside a headless egui frame with a real font stack.
    fn with_painter(mut f: impl FnMut(&Painter)) {
        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            ..Default::default()
        };
        let _ = ctx.run_ui(raw, |ui| f(ui.painter()));
    }

    fn scratch() -> ChemStructEditor {
        ChemStructEditor::default()
    }

    /// The reference scheme measures to three items on one reaction line: phenol, the
    /// arrow, anisole — left to right, non-overlapping, all crossing the line.
    #[test]
    fn scheme_lays_items_out_along_the_reaction_line() {
        let scheme = demo::williamson_ether_synthesis().scheme;
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(&scheme, painter, &style, &sstyle, &mut ed);
            assert_eq!(layout.items.len(), 3);

            let mut prev_right = f32::MIN;
            for item in &layout.items {
                assert!(
                    item.rect.min.x >= prev_right - 0.01,
                    "items must not overlap: {:?} after {prev_right}",
                    item.rect
                );
                prev_right = item.rect.max.x;
                assert!(
                    item.rect.min.y <= layout.center_line && item.rect.max.y >= layout.center_line,
                    "every item straddles the reaction line: {:?}",
                    item.rect
                );
            }
            assert!((layout.size.x - prev_right).abs() < 0.01);
            assert!(layout.size.y > 0.0 && layout.center_line > 0.0);

            // The arrow's text stack makes it taller above the line than a bare shaft.
            let arrow = &layout.items[1];
            assert!(
                layout.center_line - arrow.rect.min.y > style.arrow_head_half_width,
                "the MeI label must sit above the shaft"
            );
            assert!(arrow.rect.max.y - layout.center_line > style.arrow_head_half_width);
            match &arrow.kind {
                PlacedKind::Arrow { above, below, length } => {
                    assert_eq!(above.text(), "MeI");
                    assert_eq!(below.text(), "K2CO3", "subscripts flatten to plain digits");
                    assert!(*length >= style.arrow_length);
                }
                _ => panic!("item 1 should be the arrow"),
            }
        });
    }

    /// A structure's box is the union of its atoms and labels, and the molecule origin is
    /// placed so the drawing lands inside that box.
    #[test]
    fn structure_bounds_cover_atoms_and_labels() {
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();
        with_painter(|painter| {
            let mut ed = scratch();
            let bounds = structure_bounds(&demo::phenol(), painter, &style, &sstyle, &mut ed);
            // A benzene ring plus OH is roughly two bond lengths across in each direction.
            assert!(bounds.width() > style.bond_length, "{bounds:?}");
            assert!(bounds.height() > style.bond_length, "{bounds:?}");

            // Every atom of the (scaled) molecule sits inside the bounds.
            for atom in &ed.molecule.atoms {
                let p = ed.mol_to_screen(atom.pos, Pos2::ZERO);
                assert!(bounds.contains(p), "atom {p:?} outside {bounds:?}");
            }
            // The hydroxyl label extends the box to the left of the ring's leftmost atom.
            let min_atom_x = ed
                .molecule
                .atoms
                .iter()
                .map(|a| ed.mol_to_screen(a.pos, Pos2::ZERO).x)
                .fold(f32::MAX, f32::min);
            assert!(bounds.min.x < min_atom_x, "the HO label widens the box");
        });
    }

    /// `+` separators and free text are ordinary scheme items.
    #[test]
    fn plus_and_text_items_measure() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .text("MeI")
            .arrow(Arrow::new())
            .structure(demo::anisole());
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(
                &scheme,
                painter,
                &ReactionStyle::default(),
                &StyleConfig::default(),
                &mut ed,
            );
            assert_eq!(layout.items.len(), 5);
            match &layout.items[1].kind {
                PlacedKind::Text(t) => assert_eq!(t.text(), "+"),
                _ => panic!("item 1 should be the plus"),
            }
            match &layout.items[2].kind {
                PlacedKind::Text(t) => assert_eq!(t.text(), "MeI"),
                _ => panic!("item 2 should be the text"),
            }
        });
    }

    /// Painting draws the arrow (shaft + filled head), its labels, and the `+` separators.
    #[test]
    fn painting_emits_arrow_and_text() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .text("MeI")
            .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
            .structure(demo::anisole());
        let style = ReactionStyle::default();
        let sstyle = StyleConfig::default();

        let ctx = egui::Context::default();
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1200.0, 800.0))),
            ..Default::default()
        };
        let mut ed = scratch();
        let output = ctx.run_ui(raw, |ui| {
            let layout = measure_scheme(&scheme, ui.painter(), &style, &sstyle, &mut ed);
            paint_scheme(
                &scheme,
                &layout,
                ui.painter(),
                Pos2::new(20.0, 20.0),
                &style,
                &sstyle,
                &mut ed,
            );
        });

        let mut texts = Vec::new();
        let mut polygons = 0usize;
        fn walk(shape: &egui::Shape, texts: &mut Vec<String>, polygons: &mut usize) {
            match shape {
                egui::Shape::Text(t) => texts.push(t.galley.job.text.clone()),
                egui::Shape::Path(_) => *polygons += 1,
                egui::Shape::Vec(v) => {
                    for s in v {
                        walk(s, texts, polygons);
                    }
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut texts, &mut polygons);
        }

        assert_eq!(polygons, 1, "exactly one filled arrow head");
        for expected in ["+", "MeI", "K", "2", "CO", "3", "OH"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "expected a '{expected}' galley among {texts:?}"
            );
        }
    }

    /// An empty scheme measures to nothing instead of panicking.
    #[test]
    fn empty_scheme_is_empty() {
        with_painter(|painter| {
            let mut ed = scratch();
            let layout = measure_scheme(
                &ReactionScheme::new(),
                painter,
                &ReactionStyle::default(),
                &StyleConfig::default(),
                &mut ed,
            );
            assert!(layout.items.is_empty());
            assert_eq!(layout.size, Vec2::ZERO);
        });
    }
}
