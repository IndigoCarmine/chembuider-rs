use super::ChemStructEditor;
use super::scene::{self, RenderTransform};
use std::collections::HashMap;

// Display rules (label text, folded hydrogens, valence warnings) live in `molecule::depict`;
// re-exported so the old `widget::draw::*` paths keep compiling.
pub use crate::molecule::depict::{is_folded_h, is_valence_invalid, AtomLabel};
// The crate-internal rules are only read by this file's label oracle and tests now; every other
// caller (CDX, images, scene, formula, label chain) takes them from `molecule::depict` directly.
#[cfg(test)]
use crate::molecule::depict::{atom_label, displayed_h_count, h_side, HSide};

// Geometry helpers moved to `scene.rs` (the backend-neutral primitive builder). Re-exported so
// draw.rs's own unit tests keep reaching them by their old names.
#[cfg(test)]
pub(crate) use scene::{
    double_bond_placement, perp_normal, segment_rect_exit, trimmed_bond_endpoints,
    DoubleBondPlacement,
};

/// A laid-out atom label (galley + element-anchored screen rect). Alias of the backend-neutral
/// [`SceneLabel`] so callers (the reaction figure, tests) keep the old type name.
pub use scene::SceneLabel as AtomLabelLayout;

/// The mapping from editor pan/zoom to the backend-neutral [`RenderTransform`], taken from the
/// editor's [`ViewTransform`](super::ViewTransform) so drawing and hit-testing share one camera.
fn editor_transform(editor: &ChemStructEditor, center: egui::Pos2) -> RenderTransform {
    editor.view(center).render_transform()
}

/// Test-only window onto [`editor_transform`], so `widget::view_tests` can check that the
/// draw-side transform agrees with `ChemStructEditor::mol_to_screen`.
#[cfg(test)]
pub(super) fn editor_transform_for_tests(
    editor: &ChemStructEditor,
    center: egui::Pos2,
) -> RenderTransform {
    editor_transform(editor, center)
}

// ─── Per-frame label layout pass ─────────────────────────────────────────────

/// Lay out every visible atom label once per frame, via the shared [`scene::build_labels`]. The
/// map is shared by the bond pass (endpoint trimming), the background pass (white mask rects) and
/// the label pass (painting), so the baseline math runs once.
pub fn compute_label_layouts(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
) -> HashMap<u32, AtomLabelLayout> {
    scene::build_labels(
        &editor.molecule,
        &editor.config.style,
        editor_transform(editor, center),
        |job| painter.layout_job(job),
    )
}

// ─── Layer -1: ring fills ─────────────────────────────────────────────────────

/// Colored ring interiors, drawn behind the bonds (ChemDraw-style ring shading). Same shared
/// fill polygons the image exporter draws, so the canvas and the copied picture match.
pub fn draw_ring_fills(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    for poly in scene::build_ring_fills(&editor.molecule, editor_transform(editor, center)) {
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: poly.points,
            closed: true,
            fill: poly.color,
            stroke: egui::epaint::PathStroke::NONE,
        }));
    }
}

// ─── Layer 0: bonds ──────────────────────────────────────────────────────────

pub fn draw_bonds(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
    labels: &HashMap<u32, AtomLabelLayout>,
) {
    let bonds = scene::build_bonds(
        &editor.molecule,
        &editor.config.style,
        editor_transform(editor, center),
        labels,
    );
    for bond in bonds {
        // A hovered bond is tinted green, and so is the bond a context menu is open on; the
        // geometry is identical to the exported picture.
        let color = if editor.hovered_bond == Some(bond.bond_id) || editor.context_bond == Some(bond.bond_id) {
            egui::Color32::from_rgb(50, 180, 50)
        } else {
            egui::Color32::BLACK
        };
        for s in &bond.strokes {
            painter.line_segment([s.a, s.b], egui::Stroke::new(s.width, color));
        }
        for fill in bond.fills {
            painter.add(egui::Shape::convex_polygon(fill, color, egui::Stroke::NONE));
        }
    }
}

// ─── Layer 1: atom label backgrounds ─────────────────────────────────────────

/// White rectangles behind labels: incident bonds are already trimmed at the label rect,
/// so this only masks NON-incident bonds that happen to cross behind the text.
pub fn draw_atom_backgrounds(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    _center: egui::Pos2,
    labels: &HashMap<u32, AtomLabelLayout>,
) {
    let pad = editor.config.style.label_bg_pad * editor.zoom;
    for layout in labels.values() {
        painter.rect_filled(layout.rect.expand(pad), 0.0, egui::Color32::WHITE);
    }
}

// ─── Layer 2: atom labels + selection/hover/hotspot rings ────────────────────

pub fn draw_atom_labels(
    editor: &ChemStructEditor,
    painter: &egui::Painter,
    center: egui::Pos2,
    labels: &HashMap<u32, AtomLabelLayout>,
) {
    // Visual sizes scale with zoom so they track the panzoom transform (z == 1.0 = baseline).
    let z = editor.zoom;
    let bg = editor.config.style.atom_bg_radius * z;
    for atom in &editor.molecule.atoms {
        // A folded hydrogen is drawn nowhere, so a ring around it is a marker hovering in
        // empty space — which is exactly how a lasso-selected NH2 used to look.
        if is_folded_h(&editor.molecule, atom.id) {
            continue;
        }
        let sp = editor.mol_to_screen(atom.pos, center);
        let is_hovered  = editor.hovered_atom  == Some(atom.id);
        let is_selected = editor.selected_atoms.contains(&atom.id);
        let is_hotspot  = editor.hotspot_atom  == Some(atom.id);

        // Hotspot ring (orange, outermost)
        if is_hotspot {
            painter.circle_stroke(
                sp,
                bg + 9.0 * z,
                egui::Stroke::new(2.0 * z, egui::Color32::from_rgb(255, 140, 0)),
            );
        }
        // Selection ring (blue)
        if is_selected {
            painter.circle_stroke(
                sp,
                bg + 5.0 * z,
                egui::Stroke::new(2.0 * z, egui::Color32::from_rgb(80, 130, 255)),
            );
        }
        // Hover ring (green)
        if is_hovered {
            painter.circle_stroke(
                sp,
                bg + 2.0 * z,
                egui::Stroke::new(1.5 * z, egui::Color32::from_rgb(50, 180, 50)),
            );
        }
        // Over-valence warning (red dashed) — applies to all atoms, even unlabeled carbons.
        if is_valence_invalid(&editor.molecule, atom.id) {
            draw_dashed_circle(painter, sp, bg + 7.0 * z, egui::Color32::from_rgb(220, 30, 30), 1.5 * z);
        }

        if let Some(layout) = labels.get(&atom.id) {
            // Pre-laid-out with the element symbol anchored on the atom position.
            layout.galley.paint(painter, layout.rect.min);
        }
    }
}

// ─── Layer 2b: page objects (arrows, texts) ──────────────────────────────────

const SELECTED: egui::Color32 = egui::Color32::from_rgb(80, 130, 255);
const HOVERED: egui::Color32 = egui::Color32::from_rgb(50, 180, 50);

/// Paint the page objects from the shared scene (the geometry the image export draws), tinted
/// when hovered or selected, with the end handles of a lone selected arrow and the arrow being
/// drawn. Returns the screen rect of every text and table painted, and every table's cells,
/// for next frame's hit testing.
pub fn draw_page_objects(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) -> PageObjectRects {
    use crate::molecule::PageObject;
    let xf = editor_transform(editor, center);
    let style = &editor.config.style;
    let editing = editor.text_edit.as_ref().and_then(|e| e.id);
    let mut rects = HashMap::new();
    let mut cells = HashMap::new();
    let editing_cell = editor.table_edit.as_ref().map(|e| (e.id, e.cell));
    let paint = |obj: &scene::SceneObject, color: egui::Color32| {
        for s in &obj.strokes {
            painter.line_segment([s.a, s.b], egui::Stroke::new(s.width, color));
        }
        for fill in &obj.fills {
            painter.add(egui::Shape::convex_polygon(fill.clone(), color, egui::Stroke::NONE));
        }
        for line in &obj.texts {
            line.galley.paint(painter, line.rect.min);
        }
    };
    for object in &editor.molecule.objects {
        let id = object.id();
        if editing == Some(id) {
            continue;
        }
        let selected = editor.selected_objects.contains(&id);
        let hovered = editor.hovered_object == Some(id);
        let color = if selected { SELECTED } else if hovered { HOVERED } else { egui::Color32::BLACK };
        if let PageObject::Table(t) = object {
            let mut table = scene::build_table(t, &editor.molecule, style, xf, |job| painter.layout_job(job));
            // The cell being typed into shows the edit box instead of its value.
            if let Some((_, cell)) = editing_cell.filter(|(eid, _)| *eid == id)
                && let Some(&(_, r)) = table.cells.iter().find(|(c, _)| *c == cell)
            {
                table.object.texts.retain(|line| !r.contains(line.rect.center()));
            }
            paint(&table.object, color);
            rects.insert(id, table.object.rect);
            cells.insert(id, table.cells);
            if selected || hovered {
                painter.rect_stroke(table.object.rect.expand(2.0), 2.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Outside);
            }
            continue;
        }
        let obj = scene::build_object(object, &editor.molecule, style, xf, |job| painter.layout_job(job));
        paint(&obj, color);
        if matches!(object, PageObject::Text(_)) {
            rects.insert(id, obj.rect);
            if selected || hovered {
                painter.rect_stroke(obj.rect.expand(2.0), 2.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Outside);
            }
        }
    }
    // The ends of a lone selected arrow can be dragged: mark them.
    if let Some(a) = super::interact::lone_selected_arrow(editor) {
        for p in [a.tail, a.head] {
            let sp = xf.apply(p);
            painter.circle(sp, 4.0, egui::Color32::WHITE, egui::Stroke::new(1.5, SELECTED));
        }
    }
    // The arrow being drawn with the Arrow tool.
    if let Some(super::interact::ObjectDrag::NewArrow { tail, head, kind }) = editor.object_drag {
        let preview = PageObject::Arrow(crate::molecule::PageArrow { id: 0, tail, head, kind, source: None });
        let obj = scene::build_object(&preview, &editor.molecule, style, xf, |job| painter.layout_job(job));
        paint(&obj, egui::Color32::from_rgba_unmultiplied(80, 140, 255, 180));
    }
    PageObjectRects { rects, cells }
}

/// Where page objects were painted, for hit testing on the next frame: the rect of each text
/// and table, and each table's value cells.
#[derive(Default)]
pub struct PageObjectRects {
    pub rects: HashMap<u32, egui::Rect>,
    pub cells: HashMap<u32, Vec<(crate::reaction::edit::CellRef, egui::Rect)>>,
}

// ─── Layer 3: overlays ───────────────────────────────────────────────────────

pub fn draw_overlays(editor: &ChemStructEditor, painter: &egui::Painter, center: egui::Pos2) {
    // Bond preview line
    if let (Some(src_id), Some(end)) = (editor.bond_start, editor.preview_end_screen) {
        if let Some(src) = editor.molecule.atom_by_id(src_id) {
            let sp = editor.mol_to_screen(src.pos, center);
            painter.line_segment(
                [sp, end],
                egui::Stroke::new(1.5 * editor.zoom, egui::Color32::from_rgba_unmultiplied(80, 140, 255, 180)),
            );
        }
    }

    // Lasso
    if editor.lasso_path.len() > 1 {
        let pts: Vec<egui::Pos2> = editor.lasso_path.clone();
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: pts,
            // Must be closed: epaint panics when filling an open path.
            closed: true,
            fill: egui::Color32::from_rgba_unmultiplied(100, 150, 255, 30),
            stroke: egui::epaint::PathStroke::new(1.5 * editor.zoom, egui::Color32::from_rgb(80, 130, 255)),
        }));
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Draw a dashed circle (used as the over-valence warning ring).
fn draw_dashed_circle(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: egui::Color32, stroke_width: f32) {
    const SEGMENTS: usize = 20;
    let stroke = egui::Stroke::new(stroke_width, color);
    for i in (0..SEGMENTS).step_by(2) {
        let a0 = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        painter.line_segment(
            [
                egui::pos2(center.x + radius * a0.cos(), center.y + radius * a0.sin()),
                egui::pos2(center.x + radius * a1.cos(), center.y + radius * a1.sin()),
            ],
            stroke,
        );
    }
}

/// Plain-text label (scripts flattened) — the rendering path uses `atom_label`; this is
/// the readable oracle used by tests. Kept in sync by `label_oracle_matches_rich_label`.
#[cfg(test)]
fn atom_label_text(mol: &crate::molecule::Molecule, atom: &crate::molecule::Atom) -> String {
    let h = displayed_h_count(mol, atom.id);
    let side = if h >= 1 { h_side(mol, atom.id) } else { HSide::Right };
    let h_part = match h {
        0 => String::new(),
        1 => "H".to_string(),
        n => format!("H{n}"),
    };
    let mut s = match side {
        HSide::Right => format!("{}{}", atom.element, h_part),
        HSide::Left => format!("{}{}", h_part, atom.element),
    };
    match atom.charge {
        0          => {}
        1          => s.push('+'),
        -1         => s.push('-'),
        c if c > 0 => s.push_str(&format!("{}+", c)),
        c          => s.push_str(&format!("{}-", -c)),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, Molecule};

    fn label(mol: &Molecule, id: u32) -> String {
        atom_label_text(mol, mol.atom_by_id(id).unwrap())
    }

    /// The real NH₂ group, from the same embedded fragment set the editor uses — so these
    /// tests see the geometry that actually ships rather than a hand-written stand-in.
    fn nh2() -> crate::molecule::fragment::Fragment {
        crate::Config::embedded()
            .resolve_fragment("NH2")
            .expect("NH2 is a built-in fragment")
    }

    /// A lone (unbonded) atom shows just its symbol — no implicit H (so placing O/N/S reads
    /// as O/N/S, not OH2/NH3/SH2).
    #[test]
    fn lone_atoms_show_bare_symbol() {
        let mut m = Molecule::default();
        for el in ["O", "N", "S", "Br", "C", "P", "F"] {
            let id = m.add_atom(el.to_string(), [0.0, 0.0], 0);
            assert_eq!(label(&m, id), el, "lone {el} should render as '{el}'");
        }
    }

    /// Once bonded, valence is filled with implicit H (hydroxyl/amine/thiol, etc.).
    #[test]
    fn bonded_heteroatoms_fill_valence_with_h() {
        for (el, want) in [("O", "OH"), ("N", "NH2"), ("S", "SH"), ("Br", "Br"), ("P", "PH2")] {
            let mut m = Molecule::default();
            let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
            let x = m.add_atom(el.to_string(), [1.0, 0.0], 0);
            m.add_bond(c, x, BondOrder::Single);
            assert_eq!(label(&m, x), want, "C-{el} should render as '{want}'");
        }
        // Carbonyl: a double bond consumes two valences → no H.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = m.add_atom("O".to_string(), [1.0, 0.0], 0);
        m.add_bond(c, o, BondOrder::Double);
        assert_eq!(label(&m, o), "O", "C=O oxygen should render as 'O'");
    }

    /// A charge changes how many hydrogens an atom takes by what it does to the atom's valence
    /// electrons, not by adding itself to the valence: N⁺ and O⁺ gain a bond (onium), but C⁺
    /// loses one (carbocation), C⁻ loses one too (carbanion, a lone pair), and B⁻ gains one
    /// (borate). Each is drawn with one bond to a methyl carbon, so the label shows all the
    /// hydrogens of the named ion but one.
    #[test]
    fn charge_changes_valence_by_the_atoms_electron_count() {
        for (el, charge, want) in [
            ("C", 1, "CH2+"),  // ethyl cation: CH3+ with one H traded for the bond
            ("C", -1, "CH2-"), // CH3- carbanion
            ("N", 1, "NH3+"),  // NH4+
            ("N", -1, "NH-"),  // NH2- amide
            ("O", 1, "OH2+"),  // H3O+
            ("O", -1, "O-"),   // alkoxide
            ("B", -1, "BH3-"), // BH4- borate
            ("B", 1, "BH+"),   // BH2+ borinium
            ("Si", 1, "SiH2+"),
            ("P", 1, "PH3+"),  // PH4+ phosphonium
            ("S", 1, "SH2+"),  // H3S+ sulfonium
        ] {
            let mut m = Molecule::default();
            let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
            let x = m.add_atom(el.to_string(), [1.0, 0.0], charge);
            m.add_bond(c, x, BondOrder::Single);
            assert_eq!(label(&m, x), want, "C-{el}({charge:+}) should render as '{want}'");
        }
    }

    #[test]
    fn over_valence_is_flagged() {
        // Carbon with 5 single bonds → invalid; 4 → valid; 3 → valid (CH).
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let mut ns = Vec::new();
        for i in 0..5 {
            let n = m.add_atom("H".to_string(), [i as f32 + 1.0, 0.0], 0);
            m.add_bond(c, n, BondOrder::Single);
            ns.push(n);
        }
        assert!(is_valence_invalid(&m, c), "pentavalent carbon must be flagged");
        m.remove_atom(ns[4]);
        assert!(!is_valence_invalid(&m, c), "tetravalent carbon is valid");
        m.remove_atom(ns[3]);
        assert!(!is_valence_invalid(&m, c), "under-valence carbon is valid (implicit H)");

        // Oxygen with a triple bond → over-valence.
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = m.add_atom("O".to_string(), [1.0, 0.0], 0);
        m.add_bond(a, o, BondOrder::Triple);
        assert!(is_valence_invalid(&m, o), "O with a triple bond is over-valence");

        // Hypervalent-but-ordinary sulfur: a sulfonamide S (two S=O, S-N, S-C = 6) is
        // valid; seven bond-orders tips it over.
        let mut m = Molecule::default();
        let s = m.add_atom("S".to_string(), [0.0, 0.0], 0);
        let o1 = m.add_atom("O".to_string(), [0.0, 1.0], 0);
        let o2 = m.add_atom("O".to_string(), [0.0, -1.0], 0);
        let n = m.add_atom("N".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [-1.0, 0.0], 0);
        m.add_bond(s, o1, BondOrder::Double);
        m.add_bond(s, o2, BondOrder::Double);
        m.add_bond(s, n, BondOrder::Single);
        m.add_bond(s, c, BondOrder::Single);
        assert!(!is_valence_invalid(&m, s), "sulfonamide S(VI) must not be flagged");
        let x = m.add_atom("C".to_string(), [1.0, 1.0], 0);
        m.add_bond(s, x, BondOrder::Single);
        assert!(is_valence_invalid(&m, s), "seven bond-orders on S is over-valence");
        // But H-filling still uses the low valence: S with one bond shows SH, not SH5.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let s = m.add_atom("S".to_string(), [1.0, 0.0], 0);
        m.add_bond(c, s, BondOrder::Single);
        assert_eq!(label(&m, s), "SH");
    }

    /// C–X with charge `charge` on X, and `n` group hydrogens on X (as a fragment brings them).
    fn charged_with_group_h(el: &str, charge: i8, n: usize) -> (Molecule, u32, Vec<u32>) {
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let x = m.add_atom(el.to_string(), [1.0, 0.0], charge);
        m.add_bond(c, x, BondOrder::Single);
        let hs = (0..n)
            .map(|i| {
                let h = m.add_atom("H".to_string(), [2.0, i as f32], 0);
                m.atom_by_id_mut(h).unwrap().group_h = true;
                m.add_bond(x, h, BondOrder::Single);
                h
            })
            .collect();
        (m, x, hs)
    }

    /// The warning ring and the label agree on what a charge does: a bond the label has room
    /// for is never flagged, and one it has no room for always is. Otherwise the ring would
    /// appear on an atom whose hydrogens are all folded out of sight (a borate), or a
    /// hydrogen would quietly drop out of the label with no ring at all (a silylium).
    #[test]
    fn over_valence_moves_with_the_charge_like_the_label() {
        // Borate: C–BH2 made B⁻ has room for four, so both hydrogens fold into "BH3-" and
        // nothing is over-bonded or taken.
        let (mut m, b, hs) = charged_with_group_h("B", -1, 2);
        assert_eq!(label(&m, b), "BH3-");
        assert!(hs.iter().all(|&h| is_folded_h(&m, h)), "both hydrogens are in the label");
        assert!(!is_valence_invalid(&m, b), "a borate with three bonds is not over-bonded");
        assert!(m.consume_excess_group_h(b).is_empty(), "nothing is in excess");

        // BF4⁻: four bonds on B⁻ is its full valence.
        let mut m = Molecule::default();
        let b = m.add_atom("B".to_string(), [0.0, 0.0], -1);
        for i in 0..4 {
            let f = m.add_atom("F".to_string(), [1.0, i as f32], 0);
            m.add_bond(b, f, BondOrder::Single);
        }
        assert!(!is_valence_invalid(&m, b), "BF4- is not over-bonded");

        // Cations of the carbon group have room for three: C–SiH3 or C–CH3 made X⁺ carries
        // one bond too many, which is flagged and, being a group hydrogen, taken.
        for el in ["Si", "C"] {
            let (mut m, x, hs) = charged_with_group_h(el, 1, 3);
            assert!(is_valence_invalid(&m, x), "{el}+ with four bonds is over-bonded");
            assert_eq!(m.consume_excess_group_h(x), vec![hs[2]], "{el}+: one hydrogen goes");
            assert!(!is_valence_invalid(&m, x), "{el}+ is back at its valence");
            assert_eq!(label(&m, x), format!("{el}H2+"));
            assert!(hs[..2].iter().all(|&h| is_folded_h(&m, h)), "{el}+: the rest are folded");
        }
        // Four heavy neighbours on C⁺ are flagged too — nothing there to take.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 1);
        for i in 0..4 {
            let n = m.add_atom("C".to_string(), [1.0, i as f32], 0);
            m.add_bond(c, n, BondOrder::Single);
        }
        assert!(is_valence_invalid(&m, c), "a carbocation with four bonds is over-bonded");

        // B⁺ has room for two, so three bonds are one too many.
        let mut m = Molecule::default();
        let b = m.add_atom("B".to_string(), [0.0, 0.0], 1);
        for i in 0..3 {
            let n = m.add_atom("C".to_string(), [1.0, i as f32], 0);
            m.add_bond(b, n, BondOrder::Single);
        }
        assert!(is_valence_invalid(&m, b), "B+ with three bonds is over-bonded");
    }

    /// The rich label carries the same characters as the plain-text oracle, with the H
    /// count as a subscript span and the charge as a superscript span.
    #[test]
    fn label_oracle_matches_rich_label() {
        use crate::widget::rich_text::Script;
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 1); // NH3+ once bonded (bond from the left)
        let o = m.add_atom("O".to_string(), [3.0, 0.0], -2); // O 2- (over-reduced, but exercises multi-charge)
        m.add_bond(c, n, BondOrder::Single);

        for id in [n, o] {
            let atom = m.atom_by_id(id).unwrap();
            let rich = atom_label(&m, atom).rich;
            let flat: String = rich.spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(flat, atom_label_text(&m, atom));
        }

        // NH3+ (bond from the left keeps reading order): 3 subscript, + superscript.
        let rich = atom_label(&m, m.atom_by_id(n).unwrap()).rich;
        let scripts: Vec<(String, Script)> = rich.spans.iter()
            .map(|s| (s.text.clone(), s.style.script)).collect();
        assert_eq!(scripts, vec![
            ("NH".to_string(), Script::Normal),
            ("3".to_string(), Script::Sub),
            ("+".to_string(), Script::Super),
        ]);

        // O with charge -2 renders the chemistry-style "2-" superscript.
        let rich = atom_label(&m, m.atom_by_id(o).unwrap()).rich;
        assert_eq!(rich.spans.last().map(|s| (s.text.as_str(), s.style.script)),
                   Some(("2-", Script::Super)));
    }

    /// Only a *group's* hydrogen folds into the label. One the user drew is an ordinary
    /// atom, and folding it made it disappear completely — no vertex, no bond, no label.
    #[test]
    fn only_a_groups_hydrogen_folds_into_the_label() {
        // Drawn on purpose: its own vertex, and it counts against the neighbour's valence
        // like any other bond (C–H with three implicit hydrogens is still methane).
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let h = m.add_atom("H".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, h, BondOrder::Single);
        assert!(!is_folded_h(&m, h), "a hydrogen placed by hand must stay visible");
        assert_eq!(label(&m, h), "H");

        // Arrived inside an NH2 group: label decoration, folded into "NH2".
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, n, BondOrder::Single);
        let group = m.insert_fragment(&nh2(), n, 0.0, false);
        assert!(is_folded_h(&m, group[0]), "a group's hydrogen belongs in its label");
        assert_eq!(label(&m, n), "NH2");
    }

    /// A group hydrogen stays folded only while the label can still show it: once the heavy
    /// bonds fill the valence there is no implicit hydrogen left to hide inside, so it is
    /// drawn — which is what keeps the over-valence warning pointing at something visible.
    #[test]
    fn a_group_hydrogen_unfolds_when_the_label_has_no_room() {
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, n, BondOrder::Single);
        let h = m.insert_fragment(&nh2(), n, 0.0, false)[0];
        assert!(is_folded_h(&m, h), "one heavy bond leaves the label two hydrogens to show");

        // A second heavy neighbour, double-bonded: N now has three bond-orders of its own.
        let n2 = m.add_atom("N".to_string(), [1.5, 1.5], 0);
        m.add_bond(n, n2, BondOrder::Double);
        assert!(!is_folded_h(&m, h), "with no implicit hydrogen left, the group's must be drawn");
        assert_eq!(label(&m, n), "N", "the label shows no hydrogen it is not holding");
        assert!(is_valence_invalid(&m, n), "four bond-orders on N is over-valence, visibly so");
    }

    /// Hydrogens flip to the free side: a terminal O whose bond comes from the RIGHT
    /// renders "HO"; from the LEFT renders "OH". Anchor chars always cover the element.
    #[test]
    fn h_side_flips_terminal_labels() {
        use crate::widget::rich_text::Script;

        // C at the right of O → bond direction +x from O's perspective → flip: "HO".
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        m.add_bond(o, c, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Left);
        let label = atom_label(&m, m.atom_by_id(o).unwrap());
        let flat: String = label.rich.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(flat, "HO");
        assert_eq!(flat, atom_label_text(&m, m.atom_by_id(o).unwrap()));
        assert_eq!(label.anchor_chars, 1..2, "anchor must be the O, after the H");

        // C at the left of O → default order "OH", anchored on the leading O.
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = m.add_atom("O".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, o, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Right);
        let label = atom_label(&m, m.atom_by_id(o).unwrap());
        let flat: String = label.rich.spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(flat, "OH");
        assert_eq!(label.anchor_chars, 0..1);

        // Flipped charged amine: H first, count subscript right after the H, charge last.
        let mut m = Molecule::default();
        let n = m.add_atom("N".to_string(), [0.0, 0.0], 1);
        let c = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        m.add_bond(n, c, BondOrder::Single);
        let label = atom_label(&m, m.atom_by_id(n).unwrap());
        let scripts: Vec<(String, Script)> = label.rich.spans.iter()
            .map(|s| (s.text.clone(), s.style.script)).collect();
        assert_eq!(scripts, vec![
            ("H".to_string(), Script::Normal),
            ("3".to_string(), Script::Sub),
            ("N".to_string(), Script::Normal),
            ("+".to_string(), Script::Super),
        ]);
        assert_eq!(label.anchor_chars, 2..3, "anchor is the N after H and the count");

        // Near-vertical bond stays in the deadzone → no flip.
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [0.0, 1.5], 0);
        m.add_bond(o, c, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Right);

        // Two neighbors straddling the atom cancel out → no flip.
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let a = m.add_atom("C".to_string(), [-1.5, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        m.add_bond(a, o, BondOrder::Single);
        m.add_bond(o, b, BondOrder::Single);
        assert_eq!(h_side(&m, o), HSide::Right);

        // Custom label with an element token: anchor covers the whole leading token.
        let mut m = Molecule::default();
        let x = m.add_atom("Cl".to_string(), [0.0, 0.0], 0);
        let label = atom_label(&m, m.atom_by_id(x).unwrap());
        assert_eq!(label.anchor_chars, 0..2);
        let x2 = m.add_atom("CO2Et".to_string(), [1.0, 0.0], 0);
        let label = atom_label(&m, m.atom_by_id(x2).unwrap());
        assert_eq!(label.anchor_chars, 0..1, "CO2Et anchors on the C only");
    }

    #[test]
    fn segment_rect_exit_finds_the_leaving_point() {
        let rect = egui::Rect::from_min_max(egui::pos2(-10.0, -5.0), egui::pos2(10.0, 5.0));
        let c = egui::pos2(0.0, 0.0);

        // Exit right: t where x reaches 10 on the way to (40, 0).
        let t = segment_rect_exit(c, egui::pos2(40.0, 0.0), rect).unwrap();
        assert!((t - 0.25).abs() < 1e-5);
        // Exit top (y-min side).
        let t = segment_rect_exit(c, egui::pos2(0.0, -20.0), rect).unwrap();
        assert!((t - 0.25).abs() < 1e-5);
        // Diagonal: the y slab (5/20) is hit before the x slab (10/20).
        let t = segment_rect_exit(c, egui::pos2(20.0, 20.0), rect).unwrap();
        assert!((t - 0.25).abs() < 1e-5);
        // Axis-parallel with zero x movement.
        let t = segment_rect_exit(c, egui::pos2(0.0, 50.0), rect).unwrap();
        assert!((t - 0.1).abs() < 1e-5);
        // Never exits: destination inside the rect.
        assert!(segment_rect_exit(c, egui::pos2(5.0, 0.0), rect).is_none());
    }

    #[test]
    fn trimmed_bond_endpoints_clip_at_labels() {
        let p1 = egui::pos2(0.0, 0.0);
        let p2 = egui::pos2(100.0, 0.0);
        let r1 = egui::Rect::from_center_size(p1, egui::vec2(20.0, 10.0));
        let r2 = egui::Rect::from_center_size(p2, egui::vec2(40.0, 10.0));

        // Both ends labeled: clipped at each rect's edge.
        let (a, b) = trimmed_bond_endpoints(p1, p2, Some(r1), Some(r2)).unwrap();
        assert!((a.x - 10.0).abs() < 1e-4 && a.y == 0.0);
        assert!((b.x - 80.0).abs() < 1e-4 && b.y == 0.0);

        // Only one end labeled: the other end stays put.
        let (a, b) = trimmed_bond_endpoints(p1, p2, Some(r1), None).unwrap();
        assert!((a.x - 10.0).abs() < 1e-4);
        assert_eq!(b, p2);

        // Unlabeled: identity.
        assert_eq!(trimmed_bond_endpoints(p1, p2, None, None), Some((p1, p2)));

        // Overlapping label rects swallow the bond entirely.
        let big1 = egui::Rect::from_center_size(p1, egui::vec2(130.0, 10.0));
        let big2 = egui::Rect::from_center_size(p2, egui::vec2(130.0, 10.0));
        assert_eq!(trimmed_bond_endpoints(p1, p2, Some(big1), Some(big2)), None);

        // Whole bond inside one label rect → swallowed too.
        let huge = egui::Rect::from_center_size(p1, egui::vec2(300.0, 30.0));
        assert_eq!(trimmed_bond_endpoints(p1, p2, Some(huge), None), None);
    }

    /// End-to-end: run one headless editor frame (real egui context, real fonts) and check
    /// that a charged amine paints its label as separate galleys — the base "NH", the
    /// subscript count, and the superscript charge — through the full `ui()` paint path.
    #[test]
    fn editor_frame_paints_rich_labels() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let c = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = editor.molecule.add_atom("N".to_string(), [1.5, 0.0], 1);
        editor.molecule.add_bond(c, n, BondOrder::Single);

        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))),
            ..Default::default()
        };
        let output = crate::testkit::run_ui(&ctx, raw, |ui| {
            editor.ui(ui);
        });

        // Collect the text of every painted galley (recursing into shape groups).
        fn collect_text(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.job.text.clone()),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect_text(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut texts = Vec::new();
        for clipped in &output.shapes {
            collect_text(&clipped.shape, &mut texts);
        }
        for expect in ["NH", "3", "+"] {
            assert!(
                texts.iter().any(|t| t == expect),
                "expected a '{expect}' galley among painted text {texts:?}"
            );
        }
    }

    /// Anchoring: the element glyph (not the bounding box) is centered on the atom, so a
    /// flipped "HO" extends left of the atom and the O sits exactly on it. Runs the real
    /// layout path (fonts + galleys) inside a headless frame.
    #[test]
    fn labels_anchor_the_element_glyph_on_the_atom() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let o = editor.molecule.add_atom("O".to_string(), [0.0, 0.0], 0);
        let c = editor.molecule.add_atom("C".to_string(), [1.5, 0.0], 0);
        editor.molecule.add_bond(o, c, BondOrder::Single); // bond from the right → "HO"

        let center = egui::pos2(200.0, 150.0);
        let _ = crate::testkit::run_ui(&ctx, egui::RawInput::default(), |ui| {
            let layouts = compute_label_layouts(&editor, ui.painter(), center);
            let layout = &layouts[&o];
            let sp = editor.mol_to_screen([0.0, 0.0], center);
            assert!(layout.rect.contains(sp), "atom must sit inside its label rect");

            // The anchor char (the O of "HO") is centered on the atom position.
            let label = atom_label(&editor.molecule, editor.molecule.atom_by_id(o).unwrap());
            let flat: String = label.rich.spans.iter().map(|s| s.text.as_str()).collect();
            assert_eq!(flat, "HO");
            let a = layout.galley.char_range_rect(label.anchor_chars).unwrap();
            let anchor_screen = layout.rect.min + a.center().to_vec2();
            assert!((anchor_screen.x - sp.x).abs() < 0.75, "{anchor_screen:?} vs {sp:?}");
            assert!((anchor_screen.y - sp.y).abs() < 0.75, "{anchor_screen:?} vs {sp:?}");

            // "HO" extends to the left of the anchored O.
            assert!(layout.rect.center().x < sp.x);
        });
    }

    /// Bond lines stop at the label boundary instead of striking through the text.
    #[test]
    fn editor_frame_trims_bonds_at_labels() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let c = editor.molecule.add_atom("C".to_string(), [-1.5, 0.0], 0);
        let o = editor.molecule.add_atom("O".to_string(), [0.0, 0.0], 0);
        editor.molecule.add_bond(c, o, BondOrder::Single);

        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0))),
            ..Default::default()
        };
        let output = crate::testkit::run_ui(&ctx, raw, |ui| {
            editor.ui(ui);
        });

        let label_rect = *editor.last_label_rects.get(&o).expect("O label rect recorded");
        fn collect_lines(shape: &egui::Shape, out: &mut Vec<[egui::Pos2; 2]>) {
            match shape {
                egui::Shape::LineSegment { points, .. } => out.push(*points),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        collect_lines(s, out);
                    }
                }
                _ => {}
            }
        }
        let mut lines = Vec::new();
        for clipped in &output.shapes {
            collect_lines(&clipped.shape, &mut lines);
        }
        // Exactly one bond line, and neither endpoint reaches into the label rect.
        assert!(!lines.is_empty(), "the C-O bond must be painted");
        for [a, b] in &lines {
            assert!(
                !label_rect.contains(*a) && !label_rect.contains(*b),
                "bond endpoint inside label rect: {a:?}/{b:?} in {label_rect:?}"
            );
        }
    }

    /// Wide labels are clickable over their whole text via last-frame label rects.
    #[test]
    fn hit_test_covers_the_label_rect() {
        let ctx = egui::Context::default();
        let mut editor = ChemStructEditor::default();
        let a = editor.molecule.add_atom("CO2Et".to_string(), [0.0, 0.0], 0);

        let center = egui::pos2(200.0, 150.0);
        let _ = crate::testkit::run_ui(&ctx, egui::RawInput::default(), |ui| {
            let layouts = compute_label_layouts(&editor, ui.painter(), center);
            editor.last_label_rects =
                layouts.iter().map(|(id, l)| (*id, l.rect)).collect();
        });

        let rect = editor.last_label_rects[&a];
        assert!(rect.width() > 30.0, "CO2Et should lay out wide, got {rect:?}");
        let sp = editor.mol_to_screen([0.0, 0.0], center);
        let far_point = egui::pos2(rect.max.x - 2.0, rect.center().y);
        assert!(sp.distance(far_point) > super::super::NODE_HIT_RADIUS_PX,
            "test point must be outside the base hit circle");
        assert_eq!(editor.hit_test_atom(far_point, center), Some(a));
        // Outside both circle and rect: no hit.
        assert_eq!(editor.hit_test_atom(egui::pos2(rect.max.x + 30.0, rect.max.y + 30.0), center), None);
    }

    /// Side-decision for double-bond line placement. The helper is space-agnostic, so the test
    /// works in plain (molecule-like) coordinates: bond along the x axis with normal (0, 1).
    #[test]
    fn double_bond_placement_picks_substituent_side() {
        let p1 = egui::Pos2::new(0.0, 0.0);
        let p2 = egui::Pos2::new(2.0, 0.0);
        let normal = perp_normal(p1, p2); // (0, 1) for a horizontal bond

        // Ring-like / cis: both neighbors above the axis (same, +normal side) → Centered(+1).
        let cis = [egui::Pos2::new(0.0, 1.0), egui::Pos2::new(2.0, 1.0)];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &cis),
            DoubleBondPlacement::Centered(1.0),
        );

        // Cis on the other side → Centered(-1).
        let cis_neg = [egui::Pos2::new(0.0, -1.0), egui::Pos2::new(2.0, -1.0)];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &cis_neg),
            DoubleBondPlacement::Centered(-1.0),
        );

        // Trans: one neighbor on each side → Symmetric.
        let trans = [egui::Pos2::new(0.0, 1.0), egui::Pos2::new(2.0, -1.0)];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &trans),
            DoubleBondPlacement::Symmetric,
        );

        // Asymmetric: two neighbors above, one below → inset line toward the majority.
        let asym = [
            egui::Pos2::new(0.0, 1.0),
            egui::Pos2::new(2.0, 1.0),
            egui::Pos2::new(2.0, -1.0),
        ];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &asym),
            DoubleBondPlacement::Centered(1.0),
        );
        let asym_neg = [
            egui::Pos2::new(0.0, -1.0),
            egui::Pos2::new(2.0, -1.0),
            egui::Pos2::new(0.0, 1.0),
        ];
        assert_eq!(
            double_bond_placement(p1, p2, normal, &asym_neg),
            DoubleBondPlacement::Centered(-1.0),
        );

        // Terminal / isolated: no neighbors → Symmetric.
        assert_eq!(
            double_bond_placement(p1, p2, normal, &[]),
            DoubleBondPlacement::Symmetric,
        );
    }
}
