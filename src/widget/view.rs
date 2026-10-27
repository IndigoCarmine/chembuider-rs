//! The editor's camera: the one place that maps molecule coordinates to screen pixels.
//!
//! A molecule unit is `zoom * SCALE_FACTOR` pixels on screen, and the molecule origin sits at
//! `center + pan` (the canvas center shifted by the editor's pan). `ChemStructEditor`'s
//! `mol_to_screen` / `screen_to_mol` and the draw pass's [`RenderTransform`] are all derived from
//! a [`ViewTransform`], so they cannot drift apart.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Rect, Vec2};

use super::scene::{MoleculeScene, RenderTransform};
use super::SCALE_FACTOR;
use crate::config::StyleConfig;
use crate::molecule::Molecule;

/// Smallest editor zoom reachable by scroll or pinch.
pub const MIN_ZOOM: f32 = 0.1;
/// Largest editor zoom reachable by scroll or pinch.
pub const MAX_ZOOM: f32 = 20.0;

/// A snapshot of the editor camera (zoom, pan) placed on a canvas whose center is `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    /// Editor zoom (1 = the exporter's scale).
    pub zoom: f32,
    /// Screen-pixel offset of the molecule origin from the canvas center.
    pub pan: egui::Vec2,
    /// Canvas center in screen pixels.
    pub center: egui::Pos2,
}

impl ViewTransform {
    pub fn new(zoom: f32, pan: egui::Vec2, center: egui::Pos2) -> Self {
        Self { zoom, pan, center }
    }

    /// Screen pixels per molecule unit.
    pub fn scale(&self) -> f32 {
        self.zoom * SCALE_FACTOR
    }

    pub fn mol_to_screen(&self, pos: [f32; 2]) -> egui::Pos2 {
        let s = self.scale();
        egui::Pos2 {
            x: self.center.x + pos[0] * s + self.pan.x,
            y: self.center.y + pos[1] * s + self.pan.y,
        }
    }

    pub fn screen_to_mol(&self, screen: egui::Pos2) -> [f32; 2] {
        let inv = 1.0 / self.scale();
        [
            (screen.x - self.center.x - self.pan.x) * inv,
            (screen.y - self.center.y - self.pan.y) * inv,
        ]
    }

    /// A screen-pixel displacement (e.g. a drag delta) expressed in molecule units.
    pub fn screen_delta_to_mol(&self, delta: egui::Vec2) -> [f32; 2] {
        let inv = 1.0 / self.scale();
        [delta.x * inv, delta.y * inv]
    }

    /// This camera zoomed by `factor` (clamped to [`MIN_ZOOM`]..=[`MAX_ZOOM`]), with the pan
    /// adjusted so the molecule point under the screen position `anchor` stays under it.
    pub fn zoomed_about(&self, factor: f32, anchor: egui::Pos2) -> Self {
        let before = self.screen_to_mol(anchor);
        let mut zoomed = Self { zoom: (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM), ..*self };
        let after = zoomed.mol_to_screen(before);
        zoomed.pan += anchor - after;
        zoomed
    }

    /// The backend-neutral render mapping for this camera. Positions scale by
    /// `zoom * SCALE_FACTOR`; visual sizes (widths, font) scale by `zoom` alone — so at zoom = 1
    /// the editor and the image exporter (`SCALE_FACTOR`, `visual_scale = 1`) agree exactly.
    pub fn render_transform(&self) -> RenderTransform {
        RenderTransform {
            coord_scale: self.scale(),
            offset: self.center.to_vec2() + self.pan,
            visual_scale: self.zoom,
        }
    }
}

/// How [`ChemStructEditor::fit_view_with`](super::ChemStructEditor::fit_view_with) frames the
/// drawing on a canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitOptions {
    /// Empty space, in screen pixels, kept between the drawing and every canvas edge.
    pub margin: f32,
    /// Smallest zoom the fit may pick. Held to [`MIN_ZOOM`]..=[`MAX_ZOOM`], the range scroll
    /// and pinch zoom reach, so a fitted view can always be zoomed back by hand.
    pub min_zoom: f32,
    /// Largest zoom the fit may pick, held to the same range. Below `min_zoom`, `min_zoom` wins.
    pub max_zoom: f32,
    /// Keep the current zoom and only re-center the drawing.
    pub keep_zoom: bool,
}

impl Default for FitOptions {
    /// A 20 px margin; the zoom may shrink as far as scroll zoom does ([`MIN_ZOOM`]) but grows
    /// no further than 1, the editor's starting zoom and the exporter's scale — a small
    /// drawing is shown at its natural size rather than blown up to fill the canvas.
    fn default() -> Self {
        Self { margin: 20.0, min_zoom: MIN_ZOOM, max_zoom: 1.0, keep_zoom: false }
    }
}

impl FitOptions {
    /// `min_zoom..=max_zoom` held to [`MIN_ZOOM`]..=[`MAX_ZOOM`]; an empty range collapses to
    /// its lower end, and a NaN bound falls back to the editor's own bound on that side.
    fn zoom_range(&self) -> (f32, f32) {
        // Not `clamp`: that would carry a NaN `min_zoom` through and lose the lower bound.
        let lo = if self.min_zoom >= MIN_ZOOM { self.min_zoom.min(MAX_ZOOM) } else { MIN_ZOOM };
        (lo, self.max_zoom.min(MAX_ZOOM).max(lo))
    }
}

/// Most re-measurements [`fit_transform`] makes after its prediction.
const FIT_CORRECTIONS: usize = 4;
/// Overshoot, in pixels, that [`fit_transform`] accepts as fitting: f32 noise on canvas
/// coordinates of a few thousand pixels, far below anything visible.
const FIT_SLACK_PX: f32 = 0.01;

/// The camera that frames `mol` (drawn with `style`) on `canvas` as `opts` asks — the zoom
/// from the drawing's extent, then the pan that puts the center of its bounding rect (atoms,
/// bonds, ring fills and label rects: [`MoleculeScene::bounds`]) on the canvas center. `zoom`
/// is the current zoom, used only by [`FitOptions::keep_zoom`]. `layout` is the galley source
/// the frame will draw with. `None` when there is nothing to fit.
///
/// No warm-up frame is needed: every primitive's offset from the molecule origin — atom
/// positions, bond and wedge geometry, and the label font — scales with zoom, so the drawing
/// laid out once at zoom 1 predicts the zoom that fits, up to the pixel rounding of the label
/// text, which one layout at the predicted zoom corrects.
pub(super) fn fit_transform(
    mol: &Molecule,
    style: &StyleConfig,
    canvas: Rect,
    zoom: f32,
    opts: FitOptions,
    mut layout: impl FnMut(LayoutJob) -> Arc<egui::Galley>,
) -> Option<ViewTransform> {
    let center = canvas.center();
    // What is drawn at `zoom` with the molecule origin on the canvas center (pan 0).
    let mut measure = |zoom: f32| {
        let xf = ViewTransform::new(zoom, Vec2::ZERO, center).render_transform();
        MoleculeScene::build(mol, style, xf, &mut layout).bounds(mol, xf)
    };
    let avail = canvas.shrink(opts.margin);
    // Zoom factor that makes `b` fit `avail` (infinite along a zero-extent axis).
    let fit_ratio = |b: Rect| {
        (avail.width().max(0.0) / b.width()).min(avail.height().max(0.0) / b.height())
    };
    let (lo, hi) = opts.zoom_range();
    // `max`/`min` rather than `clamp`, so a NaN ratio (zero extent on a zero canvas) yields `lo`.
    let clamp = |z: f32| z.max(lo).min(hi);

    let (zoom, bounds) = if opts.keep_zoom {
        (zoom, measure(zoom)?)
    } else {
        let mut zoom = clamp(fit_ratio(measure(1.0)?));
        let mut bounds = measure(zoom)?;
        // Label glyph positions and row heights are rounded to whole pixels, so the prediction
        // can overshoot by a fraction of a pixel. Shrink by what the layout at that zoom
        // actually measures until it fits (one pass has sufficed in the tests' sweep; the cap
        // only bounds the work).
        for _ in 0..FIT_CORRECTIONS {
            let fits = bounds.width() <= avail.width() + FIT_SLACK_PX
                && bounds.height() <= avail.height() + FIT_SLACK_PX;
            if fits || zoom <= lo {
                break;
            }
            zoom = clamp(zoom * fit_ratio(bounds));
            bounds = measure(zoom)?;
        }
        (zoom, bounds)
    };
    // The drawing moves rigidly with the pan, so this centers it exactly.
    Some(ViewTransform::new(zoom, center - bounds.center(), center))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_maps_to_center_plus_pan_and_units_scale_by_zoom() {
        let v = ViewTransform::new(2.0, egui::vec2(10.0, -5.0), egui::pos2(300.0, 200.0));
        assert_eq!(v.scale(), 2.0 * SCALE_FACTOR);
        assert_eq!(v.mol_to_screen([0.0, 0.0]), egui::pos2(310.0, 195.0));
        assert_eq!(
            v.mol_to_screen([1.0, -1.0]),
            egui::pos2(310.0 + 2.0 * SCALE_FACTOR, 195.0 - 2.0 * SCALE_FACTOR)
        );
        assert_eq!(v.screen_to_mol(egui::pos2(310.0, 195.0)), [0.0, 0.0]);
    }

    #[test]
    fn screen_delta_to_mol_divides_by_the_scale() {
        let v = ViewTransform::new(1.7, egui::vec2(3.0, 4.0), egui::pos2(50.0, 60.0));
        let d = v.screen_delta_to_mol(egui::vec2(17.0, -34.0));
        assert_eq!(d, [17.0 / v.scale(), -34.0 / v.scale()]);
    }

    #[test]
    fn zoomed_about_keeps_the_anchor_fixed_and_clamps() {
        let v = ViewTransform::new(1.3, egui::vec2(25.0, -15.0), egui::pos2(320.0, 240.0));
        let anchor = egui::pos2(500.0, 120.0);
        let z = v.zoomed_about(1.5, anchor);
        assert!((z.zoom - 1.95).abs() <= 1e-6);
        assert_eq!(z.center, v.center);
        let (a, b) = (z.screen_to_mol(anchor), v.screen_to_mol(anchor));
        assert!((a[0] - b[0]).abs() <= 1e-4 && (a[1] - b[1]).abs() <= 1e-4, "{a:?} != {b:?}");
        assert_eq!(v.zoomed_about(1.0e4, anchor).zoom, MAX_ZOOM);
        assert_eq!(v.zoomed_about(1.0e-4, anchor).zoom, MIN_ZOOM);
    }

    #[test]
    fn render_transform_matches_mol_to_screen() {
        let v = ViewTransform::new(0.75, egui::vec2(-42.0, 17.5), egui::pos2(123.0, 456.0));
        let t = v.render_transform();
        assert_eq!(t.visual_scale, 0.75);
        for p in [[0.0, 0.0], [3.0, -1.5], [-8.25, 4.0]] {
            let (a, b) = (t.apply(p), v.mol_to_screen(p));
            assert!((a - b).length() <= 1e-3, "{p:?}: {a:?} != {b:?}");
        }
    }

    // ─── fit_transform ──────────────────────────────────────────────────────────

    use crate::molecule::{BondOrder, Molecule};

    /// The standalone font stack the image exporter uses, so a test lays labels out as shipped.
    fn fonts() -> egui::epaint::text::Fonts {
        egui::epaint::text::Fonts::new(
            egui::epaint::text::TextOptions::default(),
            egui::FontDefinitions::default(),
        )
    }

    /// Ethane, away from the origin: no labels, one short bond.
    fn ethane() -> Molecule {
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [3.0, 2.0], 0);
        let b = m.add_atom("C".to_string(), [4.5, 2.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        m
    }

    /// Two benzene-sized rings 130 units apart (each 65 from the drawing's center), an H2N on
    /// the far left and an OH on the far right, so the labels are the extremes of the drawing.
    fn wide() -> Molecule {
        let mut m = Molecule::default();
        for (cx, end) in [(40.0_f32, "N"), (170.0, "O")] {
            let ids: Vec<u32> = (0..6)
                .map(|i| {
                    let a = std::f32::consts::FRAC_PI_3 * i as f32;
                    m.add_atom("C".to_string(), [cx + 1.5 * a.cos(), 10.0 + 1.5 * a.sin()], 0)
                })
                .collect();
            for i in 0..6 {
                m.add_bond(ids[i], ids[(i + 1) % 6], BondOrder::Single);
            }
            // Vertex 0 points right, vertex 3 left.
            let (vertex, dx) = if end == "N" { (ids[3], -1.5) } else { (ids[0], 1.5) };
            let x = cx + dx * 2.0;
            let t = m.add_atom(end.to_string(), [x, 10.0], 0);
            m.add_bond(vertex, t, BondOrder::Single);
        }
        m
    }

    /// Canvases of several shapes, each big enough for [`wide`] at [`MIN_ZOOM`].
    fn canvases() -> [Rect; 3] {
        [
            Rect::from_min_size(egui::pos2(30.0, 50.0), egui::vec2(1280.0, 720.0)),
            Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(760.0, 900.0)),
            Rect::from_min_size(egui::pos2(-200.0, 75.0), egui::vec2(1600.0, 300.0)),
        ]
    }

    fn fit(mol: &Molecule, canvas: Rect, zoom: f32, opts: FitOptions) -> Option<ViewTransform> {
        let mut fonts = fonts();
        let mut view = fonts.with_pixels_per_point(1.0);
        fit_transform(mol, &StyleConfig::default(), canvas, zoom, opts, |j| view.layout_job(j))
    }

    /// The drawing under `v`: every atom position, every label rect and the union of both.
    fn drawn(mol: &Molecule, v: ViewTransform) -> (Vec<egui::Pos2>, Vec<Rect>, Rect) {
        let mut fonts = fonts();
        let mut view = fonts.with_pixels_per_point(1.0);
        let xf = v.render_transform();
        let scene =
            MoleculeScene::build(mol, &StyleConfig::default(), xf, |j| view.layout_job(j));
        let bounds = scene.bounds(mol, xf).expect("non-empty drawing");
        let atoms = mol.atoms.iter().map(|a| xf.apply(a.pos)).collect();
        (atoms, scene.labels.values().map(|l| l.rect).collect(), bounds)
    }

    fn assert_centered(bounds: Rect, canvas: Rect, what: &str) {
        let off = bounds.center() - canvas.center();
        assert!(off.x.abs() <= 1.0 && off.y.abs() <= 1.0, "{what}: drawing center off by {off:?}");
    }

    #[test]
    fn fit_puts_every_atom_and_label_inside_the_margin_and_centers_the_drawing() {
        let opts = FitOptions::default();
        let tiny = Rect::from_min_size(egui::pos2(5.0, 5.0), egui::vec2(137.0, 91.0));
        let narrow = Rect::from_min_size(egui::pos2(-40.0, 10.0), egui::vec2(90.0, 600.0));
        let mut ethane_canvases = canvases().to_vec();
        ethane_canvases.extend([tiny, narrow]);
        let cases = [
            ("ethane", ethane(), 0, ethane_canvases),
            ("wide", wide(), 2, canvases().to_vec()),
        ];
        for (name, mol, labels, canvases) in cases {
            for canvas in canvases {
                let what = format!("{name} in {canvas:?}");
                // The current camera must not matter: no warm-up frame, no dependence on it.
                let v = fit(&mol, canvas, 1.0, opts).expect("a drawing to fit");
                let w = fit(&mol, canvas, 7.5, opts).unwrap();
                assert_eq!(v, w, "{what}: fit depends on the current zoom");
                assert_eq!(v.center, canvas.center(), "{what}");

                // The binding edge lands on the margin, up to float noise.
                let avail = canvas.shrink(opts.margin).expand(FIT_SLACK_PX);
                let (atoms, rects, bounds) = drawn(&mol, v);
                assert_eq!(rects.len(), labels, "{what}: label count");
                for p in atoms {
                    assert!(avail.contains(p), "{what}: atom {p:?} outside {avail:?}");
                }
                for r in rects {
                    assert!(avail.contains_rect(r), "{what}: label {r:?} outside {avail:?}");
                }
                assert_centered(bounds, canvas, &what);
            }
        }
        // The wide drawing is shrunk below the natural scale to fit; the tiny one stops at the
        // default upper bound instead of being blown up to fill the canvas.
        assert!(fit(&wide(), canvases()[0], 1.0, opts).unwrap().zoom < 1.0);
        assert_eq!(fit(&ethane(), canvases()[0], 0.3, opts).unwrap().zoom, opts.max_zoom);
    }

    /// Long labels (CO2Et, NHBoc) at the drawing's extremes, side by side and stacked, laid out
    /// at fractional zooms and display scales. Glyph positions and row heights are rounded to
    /// pixels, so for some of these the zoom predicted from the zoom-1 layout overshoots by a
    /// fraction of a pixel, and the correction pass must take it back.
    #[test]
    fn fit_holds_for_long_labels_across_canvas_sizes_and_display_scales() {
        let mut across = wide();
        across.atoms[6].element = "CO2Et".to_string(); // the left end (was N)
        across.atoms[13].element = "NHBoc".to_string(); // the right end (was O)
        let mut stacked = across.clone();
        for a in &mut stacked.atoms {
            a.pos = [a.pos[1], a.pos[0]];
        }
        let opts = FitOptions::default();
        let style = StyleConfig::default();
        for ppp in [1.0_f32, 1.25, 1.5, 2.0] {
            let mut fonts = fonts();
            let mut view = fonts.with_pixels_per_point(ppp);
            // From the smallest canvas that holds the drawing above MIN_ZOOM.
            for w in (804..2400).step_by(13) {
                let (long, short) = (w as f32, w as f32 * 0.9);
                for (mol, size) in [(&across, egui::vec2(long, short)), (&stacked, egui::vec2(short, long))] {
                    let canvas = Rect::from_min_size(egui::pos2(3.0, 7.0), size);
                    let what = format!("ppp {ppp} canvas {size:?}");
                    let v = fit_transform(mol, &style, canvas, 1.0, opts, |j| view.layout_job(j))
                        .unwrap();
                    assert!(v.zoom > MIN_ZOOM, "{what}: zoom {}", v.zoom);
                    let xf = v.render_transform();
                    let scene = MoleculeScene::build(mol, &style, xf, |j| view.layout_job(j));
                    let avail = canvas.shrink(opts.margin).expand(FIT_SLACK_PX);
                    for l in scene.labels.values() {
                        let r = l.rect;
                        assert!(avail.contains_rect(r), "{what}: {r:?} outside {avail:?}");
                    }
                    assert_centered(scene.bounds(mol, xf).unwrap(), canvas, &what);
                }
            }
        }
    }

    #[test]
    fn fit_zoom_is_clamped_to_the_options_and_to_the_editor_range() {
        let small = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(120.0, 80.0));
        let big = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(4000.0, 3000.0));

        let floor = FitOptions { min_zoom: 0.5, ..Default::default() };
        let v = fit(&wide(), small, 1.0, floor).unwrap();
        assert_eq!(v.zoom, 0.5, "too wide to fit: stops at min_zoom");
        assert_centered(drawn(&wide(), v).2, small, "clamped low");

        let ceiling = FitOptions { max_zoom: 3.0, ..Default::default() };
        let v = fit(&ethane(), big, 1.0, ceiling).unwrap();
        assert_eq!(v.zoom, 3.0, "tiny drawing: stops at max_zoom");
        assert_centered(drawn(&ethane(), v).2, big, "clamped high");

        // Options outside the scroll-zoom range are held to it.
        let wild = FitOptions { min_zoom: 0.0, max_zoom: 1.0e6, ..Default::default() };
        assert_eq!(fit(&ethane(), big, 1.0, wild).unwrap().zoom, MAX_ZOOM);
        assert_eq!(fit(&wide(), small, 1.0, wild).unwrap().zoom, MIN_ZOOM);
        // An inverted range collapses to its lower end.
        let inverted = FitOptions { min_zoom: 2.0, max_zoom: 0.5, ..Default::default() };
        assert_eq!(fit(&ethane(), big, 1.0, inverted).unwrap().zoom, 2.0);
        // A NaN bound falls back to the editor's bound on that side.
        let nan_floor = FitOptions { min_zoom: f32::NAN, ..Default::default() };
        assert_eq!(fit(&wide(), small, 1.0, nan_floor).unwrap().zoom, MIN_ZOOM);
        let nan_ceiling = FitOptions { max_zoom: f32::NAN, ..Default::default() };
        assert_eq!(fit(&ethane(), big, 1.0, nan_ceiling).unwrap().zoom, MAX_ZOOM);
    }

    #[test]
    fn keep_zoom_only_recenters() {
        let opts = FitOptions { keep_zoom: true, ..Default::default() };
        for zoom in [0.37, 2.5] {
            for canvas in canvases() {
                let v = fit(&wide(), canvas, zoom, opts).unwrap();
                assert_eq!(v.zoom, zoom);
                assert_centered(drawn(&wide(), v).2, canvas, &format!("zoom {zoom} {canvas:?}"));
            }
        }
    }

    #[test]
    fn fit_of_an_empty_molecule_is_none() {
        for opts in [FitOptions::default(), FitOptions { keep_zoom: true, ..Default::default() }] {
            assert_eq!(fit(&Molecule::default(), canvases()[0], 1.0, opts), None);
        }
    }
}
