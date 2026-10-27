//! The editor's camera: the one place that maps molecule coordinates to screen pixels.
//!
//! A molecule unit is `zoom * SCALE_FACTOR` pixels on screen, and the molecule origin sits at
//! `center + pan` (the canvas center shifted by the editor's pan). `ChemStructEditor`'s
//! `mol_to_screen` / `screen_to_mol` and the draw pass's [`RenderTransform`] are all derived from
//! a [`ViewTransform`], so they cannot drift apart.

use super::scene::RenderTransform;
use super::SCALE_FACTOR;

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
    fn render_transform_matches_mol_to_screen() {
        let v = ViewTransform::new(0.75, egui::vec2(-42.0, 17.5), egui::pos2(123.0, 456.0));
        let t = v.render_transform();
        assert_eq!(t.visual_scale, 0.75);
        for p in [[0.0, 0.0], [3.0, -1.5], [-8.25, 4.0]] {
            let (a, b) = (t.apply(p), v.mol_to_screen(p));
            assert!((a - b).length() <= 1e-3, "{p:?}: {a:?} != {b:?}");
        }
    }
}
