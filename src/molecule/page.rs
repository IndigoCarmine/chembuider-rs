//! Free-form objects on the page beside the structures: reaction arrows, text (reaction
//! conditions, captions) and reaction (stoichiometry) tables.
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

use super::bracket::Bracket;
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

/// What the user entered for one column of a [`ReactionTable`]: a reactant's or a product's
/// quantities. Everything else the column shows (formula, MW, moles, masses, yield) is
/// computed from these and from the structure the column stands for.
///
/// Units are the ones [`reaction`](crate::reaction) works in: grams, millilitres, mol/L, g/mL
/// and percent (0–100). A reactant uses `limiting` … `volume`; a product `equivalents`,
/// `percent_completion`, `measured_mass` and `purity`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TableEntry {
    /// An atom of the structure this column stands for, so the entries stay with that
    /// structure when others are added, removed or moved. `None` for a column read from a
    /// file before it was matched to a structure: it takes the first structure no other entry
    /// claims.
    pub atom: Option<u32>,
    pub limiting: bool,
    pub equivalents: Option<f64>,
    pub sample_mass: Option<f64>,
    pub percent_weight: Option<f64>,
    pub molarity: Option<f64>,
    pub density: Option<f64>,
    pub volume: Option<f64>,
    pub percent_completion: Option<f64>,
    pub measured_mass: Option<f64>,
    pub purity: Option<f64>,
    /// The ChemDraw fragment this column referred to (`ComponentReferenceID`), when read from
    /// a file. Reading resolves it to [`atom`](Self::atom).
    pub fragment: Option<u32>,
    /// The formula the file showed for this column, when read from one: saving compares it
    /// with the structure's current formula to tell whether ChemDraw's table is still right.
    pub formula: Option<String>,
}

impl TableEntry {
    /// Whether the two entries hold the same quantities (ignoring which structure they are
    /// attached to and what a file said about them).
    pub fn same_quantities(&self, other: &TableEntry) -> bool {
        let strip = |e: &TableEntry| TableEntry { atom: None, fragment: None, formula: None, ..e.clone() };
        strip(self) == strip(other)
    }
}

/// A stoichiometry table (ChemDraw's "Reaction Analysis" stoichiometry grid) placed on the
/// page: a *Reactants* block and a *Products* block, one column per structure, describing the
/// reaction of one arrow — the structures before the arrow (and the reagents drawn over it)
/// are its reactants, the ones after it its products. Which structures those are is worked out
/// from the drawing every time (see [`Molecule::reaction_table_columns`](super::Molecule::reaction_table_columns)),
/// so the table follows the scheme as it is edited; only the entered quantities are stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReactionTable {
    pub id: u32,
    /// The table's top-left corner (where ChemDraw places the grid), in molecule units.
    pub pos: [f32; 2],
    /// The arrow ([`PageArrow::id`]) whose reaction the table describes. `None`, or an arrow
    /// that is gone, means the reaction arrow nearest to the table.
    #[serde(default)]
    pub arrow: Option<u32>,
    #[serde(default)]
    pub reactants: Vec<TableEntry>,
    #[serde(default)]
    pub products: Vec<TableEntry>,
    /// The id of the ChemDraw stoichiometry grid this was read from; see [`PageArrow::source`].
    #[serde(default)]
    pub source: Option<u32>,
}

/// One free-form object on the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PageObject {
    Arrow(PageArrow),
    Text(PageText),
    Table(ReactionTable),
    /// Polymer / repeating-unit brackets around some atoms ([`super::bracket`]). They have no
    /// position of their own: they stand where their atoms are.
    Bracket(Bracket),
}

impl PageObject {
    pub fn id(&self) -> u32 {
        match self {
            PageObject::Arrow(a) => a.id,
            PageObject::Text(t) => t.id,
            PageObject::Table(t) => t.id,
            PageObject::Bracket(b) => b.id,
        }
    }

    /// Give the object a new id.
    pub fn set_id(&mut self, id: u32) {
        match self {
            PageObject::Arrow(a) => a.id = id,
            PageObject::Text(t) => t.id = id,
            PageObject::Table(t) => t.id = id,
            PageObject::Bracket(b) => b.id = id,
        }
    }

    /// The id of the ChemDraw object this was read from, if any.
    pub fn source(&self) -> Option<u32> {
        match self {
            PageObject::Arrow(a) => a.source,
            PageObject::Text(t) => t.source,
            PageObject::Table(t) => t.source,
            PageObject::Bracket(b) => b.source,
        }
    }

    /// Move the whole object by `delta` (molecule units). Brackets go where their atoms go, so
    /// moving them alone does nothing.
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
            PageObject::Table(t) => shift(&mut t.pos),
            PageObject::Bracket(_) => {}
        }
    }

    /// Rotate the object's reference points by `angle` (radians) about `center`. A text keeps
    /// its orientation, as does a table; only their anchors move.
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
            PageObject::Table(t) => rot(&mut t.pos),
            PageObject::Bracket(_) => {}
        }
    }

    /// The points that stand for the object in a lasso or a bounding box: an arrow's two ends,
    /// a text's anchor, a table's top-left corner. Brackets have none (their atoms stand for
    /// them).
    pub fn key_points(&self) -> Vec<[f32; 2]> {
        match self {
            PageObject::Arrow(a) => vec![a.tail, a.head],
            PageObject::Text(t) => vec![t.pos],
            PageObject::Table(t) => vec![t.pos],
            PageObject::Bracket(_) => Vec::new(),
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

    pub fn as_table(&self) -> Option<&ReactionTable> {
        match self {
            PageObject::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_bracket(&self) -> Option<&Bracket> {
        match self {
            PageObject::Bracket(b) => Some(b),
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
