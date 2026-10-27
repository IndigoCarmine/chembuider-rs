//! Free-form objects on the page beside the structures: reaction arrows and text (reaction
//! conditions, captions).
//!
//! They live in [`Molecule::objects`](super::Molecule::objects), next to the atoms and bonds,
//! so everything that already handles a drawing as one value — the editor's undo/redo
//! snapshots, a host's "is the document dirty" comparison, JSON saving, the shared scene that
//! the canvas and the image exports draw — covers them without a second code path. Ring fills
//! set the precedent: page decoration that is not chemistry, kept on the molecule.
//!
//! Coordinates are molecule units (a bond is 1.5, y grows downward), the same as atoms, so a
//! CDX file's page coordinates map onto them with the same scale (see `molecule::cdx`).

use serde::{Deserialize, Serialize};

use crate::widget::rich_text::RichText;

/// What an arrow means, which decides how it is drawn and written to ChemDraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ArrowKind {
    /// A plain reaction arrow: one shaft, a full head at the head end.
    #[default]
    Reaction,
    /// An equilibrium arrow: two parallel shafts pointing opposite ways, each with a half
    /// head on its outer side (ChemDraw's "HalfLeft" head and tail).
    Equilibrium,
}

/// Which point of a text's first line [`PageText::pos`] is: ChemDraw's text justification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextAlign {
    /// `pos` is the left end of the baseline (ChemDraw's default for typed text).
    #[default]
    Left,
    /// `pos` is the middle of the baseline; every line is centred on it.
    Center,
    /// `pos` is the right end of the baseline.
    Right,
}

/// A straight arrow from `tail` to `head`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageArrow {
    pub id: u32,
    pub tail: [f32; 2],
    pub head: [f32; 2],
    pub kind: ArrowKind,
    /// The id of the ChemDraw object this arrow was read from (the `arrow` object, or a legacy
    /// line `graphic` when the file has no arrow object for it). Saving back into that file
    /// rewrites that object instead of adding a second arrow. `None` for an arrow drawn here.
    #[serde(default)]
    pub source: Option<u32>,
}

/// A block of text: one line per `\n`, each line in the rich-text markup the atom labels use
/// (`CO_2Et`, `^{13}C`, `**bold**`, `//italic//`; see [`RichText::parse`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageText {
    pub id: u32,
    /// The anchor on the first line's baseline; which point it is depends on [`align`](Self::align).
    pub pos: [f32; 2],
    /// Markup, lines separated by `\n`.
    pub text: String,
    #[serde(default)]
    pub align: TextAlign,
    /// The id of the ChemDraw text object this was read from; see [`PageArrow::source`].
    #[serde(default)]
    pub source: Option<u32>,
}

impl PageText {
    /// Each line of the text as rich text, in order (an empty text has one empty line).
    pub fn lines(&self) -> Vec<RichText> {
        self.text.split('\n').map(RichText::parse).collect()
    }

    /// True when there is nothing to draw: every line is blank.
    pub fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }
}

/// One free-form object on the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PageObject {
    Arrow(PageArrow),
    Text(PageText),
}

impl PageObject {
    pub fn id(&self) -> u32 {
        match self {
            PageObject::Arrow(a) => a.id,
            PageObject::Text(t) => t.id,
        }
    }

    /// The id of the ChemDraw object this was read from, if any.
    pub fn source(&self) -> Option<u32> {
        match self {
            PageObject::Arrow(a) => a.source,
            PageObject::Text(t) => t.source,
        }
    }

    /// Move the whole object by `delta` (molecule units).
    pub fn translate(&mut self, delta: [f32; 2]) {
        let shift = |p: &mut [f32; 2]| {
            p[0] += delta[0];
            p[1] += delta[1];
        };
        match self {
            PageObject::Arrow(a) => {
                shift(&mut a.tail);
                shift(&mut a.head);
            }
            PageObject::Text(t) => shift(&mut t.pos),
        }
    }

    /// Rotate the object's reference points by `angle` (radians) about `center`. A text keeps
    /// its orientation; only its anchor moves.
    pub fn rotate_about(&mut self, center: [f32; 2], angle: f32) {
        let (s, c) = angle.sin_cos();
        let rot = |p: &mut [f32; 2]| {
            let (dx, dy) = (p[0] - center[0], p[1] - center[1]);
            *p = [center[0] + dx * c - dy * s, center[1] + dx * s + dy * c];
        };
        match self {
            PageObject::Arrow(a) => {
                rot(&mut a.tail);
                rot(&mut a.head);
            }
            PageObject::Text(t) => rot(&mut t.pos),
        }
    }

    /// The points that stand for the object in a lasso or a bounding box: an arrow's two ends,
    /// a text's anchor.
    pub fn key_points(&self) -> Vec<[f32; 2]> {
        match self {
            PageObject::Arrow(a) => vec![a.tail, a.head],
            PageObject::Text(t) => vec![t.pos],
        }
    }

    pub fn as_arrow(&self) -> Option<&PageArrow> {
        match self {
            PageObject::Arrow(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&PageText> {
        match self {
            PageObject::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// The default length of an arrow placed with a click: three bonds.
pub const DEFAULT_ARROW_LENGTH: f32 = 4.5;

/// `head` moved so the direction `tail → head` is a multiple of `step_deg` degrees, keeping
/// its length.
pub fn snap_direction(tail: [f32; 2], head: [f32; 2], step_deg: f32) -> [f32; 2] {
    let (dx, dy) = (head[0] - tail[0], head[1] - tail[1]);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return head;
    }
    let step = step_deg.to_radians();
    let angle = (dy.atan2(dx) / step).round() * step;
    [tail[0] + len * angle.cos(), tail[1] + len * angle.sin()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapping_keeps_length_and_rounds_the_angle() {
        let head = snap_direction([0.0, 0.0], [3.0, 0.3], 15.0);
        assert!((head[1]).abs() < 1e-5, "5.7° snaps to 0°: {head:?}");
        assert!((head[0] - (9.0f32 + 0.09).sqrt()).abs() < 1e-5);
        let head = snap_direction([1.0, 1.0], [2.0, 2.1], 15.0);
        let angle = (head[1] - 1.0).atan2(head[0] - 1.0).to_degrees();
        assert!((angle - 45.0).abs() < 1e-3, "{angle}");
    }

    #[test]
    fn text_lines_split_on_newlines() {
        let t = PageText { id: 1, pos: [0.0, 0.0], text: "Pd(PPh_3)_4\nTHF, 60 °C".into(), align: TextAlign::Left, source: None };
        let lines = t.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].spans.len(), 4, "Pd(PPh, 3, )_, 4");
        assert!(!t.is_blank());
        assert!(PageText { text: " \n ".into(), ..t }.is_blank());
    }

    #[test]
    fn translate_and_rotate_move_the_reference_points() {
        let mut a = PageObject::Arrow(PageArrow { id: 1, tail: [0.0, 0.0], head: [2.0, 0.0], kind: ArrowKind::Reaction, source: None });
        a.translate([1.0, 1.0]);
        assert_eq!(a.key_points(), vec![[1.0, 1.0], [3.0, 1.0]]);
        a.rotate_about([1.0, 1.0], std::f32::consts::FRAC_PI_2);
        let head = a.as_arrow().unwrap().head;
        assert!((head[0] - 1.0).abs() < 1e-5 && (head[1] - 3.0).abs() < 1e-5, "{head:?}");
    }
}
