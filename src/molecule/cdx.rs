//! A `Molecule` as ChemDraw's binary CDX and back: the "ChemDraw Interchange Format" on the
//! clipboard and the `CONTENTS` of a ChemDraw object embedded in an Office document.
//!
//! The layout follows what ChemDraw 26 itself writes (pinned by the fixtures in
//! `tests/fixtures/chemdraw26_*.cdx`, saved by ChemDraw):
//! - one fragment per connected structure, on one page, with ChemDraw's own document style
//!   (bond length 14.4 pt, Arial 10 pt labels, line widths);
//! - an atom with a label carries its hydrogen count and a text object ("NH2", "O-") in
//!   ChemDraw's formula face; carbons without a charge carry neither;
//! - hydrogens folded into a label ("NH2") are not nodes of their own, only the count;
//! - a label that is no element ("Ph", "CO_2Et") is a node of no element with its text;
//! - wedges, hashes, bold, dashed and wavy bonds are the bond's display.
//!
//! Positions are in 1/65536 pt, y down, as on screen: [`SCALE`] maps the editor's 1.5-unit bond
//! onto ChemDraw's 14.4 pt one.

use super::cdx_codec::{self, CdxObject};
use super::lookup::{AtomLookup, MolIndex};
use super::{elements, BondOrder, BondStereo, DoublePosition, Molecule};
use super::depict;
use crate::widget::rich_text::{RichSpan, RichText, Script, SpanStyle};
use std::collections::{HashMap, HashSet};

// Objects (kCDXObj_*).
pub(crate) const OBJ_DOCUMENT: u16 = 0x8000;
pub(crate) const OBJ_PAGE: u16 = 0x8001;
pub(crate) const OBJ_GROUP: u16 = 0x8002;
pub(crate) const OBJ_FRAGMENT: u16 = 0x8003;
pub(crate) const OBJ_NODE: u16 = 0x8004;
pub(crate) const OBJ_BOND: u16 = 0x8005;
pub(crate) const OBJ_TEXT: u16 = 0x8006;

// Properties (kCDXProp_*).
pub(crate) const PROP_CREATION_PROGRAM: u16 = 0x0003;
pub(crate) const PROP_2D_POSITION: u16 = 0x0200;
pub(crate) const PROP_BOUNDING_BOX: u16 = 0x0204;
const PROP_NODE_TYPE: u16 = 0x0400;
const PROP_NODE_ELEMENT: u16 = 0x0402;
const PROP_ATOM_CHARGE: u16 = 0x0421;
const PROP_ATOM_NUM_HYDROGENS: u16 = 0x042B;
pub(crate) const PROP_TEXT: u16 = 0x0700;
const PROP_LABEL_ALIGNMENT: u16 = 0x0705;
pub(crate) const PROP_UTF8_TEXT: u16 = 0x0709;
const PROP_BOND_ORDER: u16 = 0x0600;
const PROP_BOND_DISPLAY: u16 = 0x0601;
const PROP_BOND_DOUBLE_POSITION: u16 = 0x0602;
const PROP_BOND_BEGIN: u16 = 0x0604;
const PROP_BOND_END: u16 = 0x0605;

/// kCDXBondOrder_Dative: a dative bond is an order of its own in CDX.
const ORDER_DATIVE: i16 = 0x1000;

/// Node types (kCDXNodeType_*) this module writes or reads specially.
const NODE_UNSPECIFIED: i16 = 0;
const NODE_ELEMENT: i16 = 1;
const NODE_EXTERNAL_CONNECTION_POINT: i16 = 12;

/// Label alignment (kCDXLabelAlignment_*): the first character on the atom, or the last.
const ALIGN_LEFT: u8 = 1;
const ALIGN_RIGHT: u8 = 3;

/// The most atoms, and the most bonds, a document's structures may make for [`read_cdx`] to
/// read it: its nodes and bonds, and the hydrogens (with their bonds) that the nodes' hydrogen
/// counts add, counted as if every node got them. The largest real drawings (a long polymer, a
/// big reaction scheme in ChemDraw) have a few thousand atoms and bonds; these leave a margin
/// of more than twenty times that. A CDX beyond them is no drawing to edit but bytes that would
/// keep the reader, and an Office host waiting on a Render or Import, busy (a bare node and
/// bond take about 30 bytes, so a pipe frame of 256 MiB could hold millions), and the editor's
/// own work on a molecule (drawing, looking atoms and bonds up) is not made for such sizes
/// either.
pub(crate) const MAX_READ_ATOMS: usize = 100_000;
/// See [`MAX_READ_ATOMS`].
pub(crate) const MAX_READ_BONDS: usize = 100_000;

/// ChemDraw's document style, as ChemDraw 26 writes it: bond length (14.4 pt), spacing and line
/// widths, the label and caption styles (Arial 10 pt, formula face), the colour and font tables.
/// Copying them makes a structure from here look exactly like one drawn in ChemDraw.
const DOCUMENT_STYLE: [(u16, &str); 14] = [
    (0x080a, "03006000c8000300"),
    (0x080b, "03000000c8000300"),
    (0x0809, "967e0200"),
    (0x0808, "5f960100"),
    (0x0807, "99990000"),
    (0x0806, "39030200"),
    (0x0805, "66660e00"),
    (0x0804, "b400"),
    (0x0803, "00007800"),
    (0x0802, "ffff2300ffff2300ffff2300ffff2300"),
    (0x0301, "0000"),
    (0x0302, "0100"),
    (0x0300, "0800ffffffffffff000000000000ffff00000000ffffffff00000000ffff00000000ffffffff00000000ffffffff0000ffff"),
    (0x0100, "000001000300e4040500417269616c"),
];

/// Font id of Arial in [`DOCUMENT_STYLE`]'s font table.
pub(crate) const LABEL_FONT: u16 = 3;
/// Label size in 1/20 pt (10 pt).
pub(super) const LABEL_SIZE: u16 = 200;
/// Formula face: digits become subscripts and a trailing charge a superscript.
const FACE_FORMULA: u16 = 0x60;
const FACE_BOLD: u16 = 0x01;
const FACE_ITALIC: u16 = 0x02;
const FACE_SUBSCRIPT: u16 = 0x20;
const FACE_SUPERSCRIPT: u16 = 0x40;

/// Molecule units → CDX units (1/65536 pt): the editor's 1.5-unit bond is ChemDraw's 14.4 pt.
pub const SCALE: f64 = 14.4 / 1.5 * 65536.0;

/// One point in CDX units.
pub(crate) const PT: f64 = 65536.0;
/// A structure starting above or left of the page edge is moved to this margin.
const PAGE_MARGIN: f64 = 36.0 * PT;
/// From an atom to its label's baseline, as ChemDraw places it (half the cap height of 10 pt Arial).
const LABEL_BASELINE: f64 = 3.9 * PT;

/// What a CDX file holds, as far as the editor goes.
#[derive(Debug, Clone)]
pub struct CdxImport {
    pub molecule: Molecule,
    /// Objects on the page other than structures (arrows, captions, shapes, …): kept by
    /// [`merge_into_cdx`] but not shown in the editor.
    pub other_objects: usize,
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// A CDXString: its style runs (start, font, face, size, colour), then the characters.
pub(crate) fn cdx_string(runs: &[(u16, u16)], text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(runs.len() as u16).to_le_bytes());
    for &(start, face) in runs {
        for v in [start, LABEL_FONT, face, LABEL_SIZE, 0] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend(text.chars().map(|c| if (c as u32) < 0x100 { c as u8 } else { b'?' }));
    out
}

/// The text and style runs (start, face) of a CDXString.
pub(crate) fn parse_cdx_string(v: &[u8]) -> Option<(Vec<(usize, u16)>, String)> {
    let n = u16::from_le_bytes([*v.first()?, *v.get(1)?]) as usize;
    let mut runs = Vec::with_capacity(n);
    for i in 0..n {
        let s = v.get(2 + i * 10..2 + i * 10 + 10)?;
        runs.push((u16::from_le_bytes([s[0], s[1]]) as usize, u16::from_le_bytes([s[4], s[5]])));
    }
    // Windows-1252 in practice; the ASCII part is what labels use.
    let text = v.get(2 + n * 10..)?.iter().map(|&b| b as char).collect();
    Some((runs, text))
}

pub(crate) fn point(x: f64, y: f64) -> Vec<u8> {
    let mut v = Vec::with_capacity(8);
    v.extend_from_slice(&(y.round() as i32).to_le_bytes());
    v.extend_from_slice(&(x.round() as i32).to_le_bytes());
    v
}

/// A CDXRectangle: top, left, bottom, right.
pub(crate) fn rect(r: [f64; 4]) -> Vec<u8> {
    let [left, top, right, bottom] = r;
    [top, left, bottom, right].iter().flat_map(|v| (v.round() as i32).to_le_bytes()).collect()
}

pub(crate) fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
    match a {
        None => b,
        Some(a) => [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])],
    }
}

/// Approximate advance of a character of 10 pt Arial, in CDX units (enough to centre a label's
/// first or last letter on its atom the way ChemDraw does).
fn arial_width(c: char) -> f64 {
    let per_mille = match c {
        'i' | 'j' | 'l' => 222,
        'I' | 'f' | 't' | '/' => 278,
        'r' | '(' | ')' | '-' => 333,
        'm' | 'M' => 833,
        'w' => 722,
        'W' => 944,
        'O' | 'Q' | 'G' => 778,
        'H' | 'N' | 'C' | 'D' | 'R' | 'U' | 'A' | 'K' | 'X' => 722,
        '+' => 584,
        c if c.is_ascii_uppercase() => 667,
        _ => 556,
    };
    per_mille as f64 * 10.0 * PT / 1000.0
}

/// Where the label `text` of an atom at `at` (CDX units) goes, aligned on its first character or,
/// with `right`, its last: the text's origin on the baseline and its bounding box (left, top,
/// right, bottom), in CDX units.
pub(super) fn label_geometry(at: (f64, f64), text: &str, right: bool) -> ((f64, f64), [f64; 4]) {
    let (x, y) = at;
    let first = text.chars().next().map_or(0.0, arial_width);
    let last = text.chars().last().map_or(0.0, arial_width);
    let width: f64 = text.chars().map(arial_width).sum();
    let (origin_x, left) = if right { (x + last / 2.0, x + last / 2.0 - width) } else { (x - first / 2.0, x - first / 2.0) };
    let baseline = y + LABEL_BASELINE;
    ((origin_x, baseline), [left, baseline - 7.2 * PT, left + width, baseline + 2.1 * PT])
}

/// The text object labelling an atom at `at` (CDX units): `label` with its style runs, aligned on
/// the first character or the last.
fn label_object(id: u32, at: (f64, f64), label: &NodeLabel) -> CdxObject {
    let (origin, bbox) = label_geometry(at, &label.text, label.right);
    let mut t = CdxObject::new(OBJ_TEXT, id);
    t.set(PROP_2D_POSITION, point(origin.0, origin.1));
    t.set(PROP_BOUNDING_BOX, rect(bbox));
    t.set(PROP_LABEL_ALIGNMENT, vec![if label.right { ALIGN_RIGHT } else { ALIGN_LEFT }]);
    t.set(PROP_TEXT, cdx_string(&label.runs, &label.text));
    t.set(PROP_UTF8_TEXT, cdx_string(&label.runs, &label.text));
    t
}

/// What an atom is written as: the node's type and element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NodeKind {
    /// Carbon, ChemDraw's default node: no type and no element written.
    Carbon,
    /// Any other element, by atomic number.
    Element(u16),
    /// A label that is no element ("Ph", "CO_2Et", "D"): a node of no element, its text all
    /// there is of it.
    Unspecified,
}

/// The label a node carries: its text with its style runs (start char, face), aligned on its
/// first character or, with `right`, its last (the hydrogens of "H2N" before the atom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NodeLabel {
    pub runs: Vec<(u16, u16)>,
    pub text: String,
    pub right: bool,
}

/// How an atom is written as a node of a ChemDraw document. One set of rules for both of
/// ChemDraw's formats, the binary CDX here and its XML form ([`super::cdxml`]), so the two
/// describe the same structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NodeRules {
    pub kind: NodeKind,
    /// The hydrogens the label shows, for an element with a label (they are not nodes of their own).
    pub num_hydrogens: Option<i16>,
    pub charge: i8,
    pub label: Option<NodeLabel>,
}

/// Whether `atom_id` is a node of its own: a hydrogen folded into its neighbour's label ("NH2")
/// is only that label's hydrogen count, and its bond is not written either.
pub(super) fn is_node(mol: &impl AtomLookup, atom_id: u32) -> bool {
    !depict::folded_h(mol, atom_id)
}

/// The node `atom` is written as (see [`NodeRules`]). A writer asks this of every atom, so it
/// passes a [`MolIndex`] built once rather than the molecule, whose lookups scan.
pub(super) fn node_rules(mol: &impl AtomLookup, atom: &super::Atom) -> NodeRules {
    let z = elements::element_of_label(&atom.element);
    let kind = match z {
        Some(6) => NodeKind::Carbon,
        Some(z) => NodeKind::Element(z),
        None => NodeKind::Unspecified,
    };
    let shown = depict::should_show_label(mol, atom.id);
    let num_hydrogens = (z.is_some() && shown).then(|| depict::displayed_h_count(mol, atom.id) as i16);
    let label = shown.then(|| {
        let (mut runs, text) = runs_of(&depict::atom_label(mol, atom).rich);
        // An element's label is a formula, written the way ChemDraw writes its own.
        if z.is_some() {
            runs = vec![(0, FACE_FORMULA)];
        }
        let right = depict::displayed_h_count(mol, atom.id) > 0 && depict::h_side(mol, atom.id) == depict::HSide::Left;
        NodeLabel { runs, text, right }
    });
    NodeRules { kind, num_hydrogens, charge: atom.charge, label }
}

/// Style runs (start char, face) for rich text, and its characters.
pub(crate) fn runs_of(rich: &RichText) -> (Vec<(u16, u16)>, String) {
    let mut runs: Vec<(u16, u16)> = Vec::new();
    let mut text = String::new();
    for span in &rich.spans {
        let mut face = match span.style.script {
            Script::Normal => 0,
            Script::Sub => FACE_SUBSCRIPT,
            Script::Super => FACE_SUPERSCRIPT,
        };
        if span.style.bold {
            face |= FACE_BOLD;
        }
        if span.style.italic {
            face |= FACE_ITALIC;
        }
        if runs.last().is_none_or(|r| r.1 != face) {
            runs.push((text.chars().count() as u16, face));
        }
        text.push_str(&span.text);
    }
    (runs, text)
}

/// Rich text from a CDXString's runs: formula face turns digits into subscripts and the
/// trailing charge into a superscript, as ChemDraw draws it.
pub(crate) fn rich_of(runs: &[(usize, u16)], text: &str) -> RichText {
    let chars: Vec<char> = text.chars().collect();
    let mut rich = RichText::new();
    let runs: Vec<(usize, u16)> = if runs.is_empty() { vec![(0, 0)] } else { runs.to_vec() };
    for (i, &(start, face)) in runs.iter().enumerate() {
        let end = runs.get(i + 1).map_or(chars.len(), |r| r.0).min(chars.len());
        let Some(part) = chars.get(start.min(end)..end) else { continue };
        let base = SpanStyle { bold: face & FACE_BOLD != 0, italic: face & FACE_ITALIC != 0, ..Default::default() };
        if face & FACE_FORMULA == FACE_FORMULA {
            let charge_from = part.iter().rposition(|c| !matches!(c, '+' | '-')).map_or(0, |p| p + 1);
            for (k, &c) in part.iter().enumerate() {
                let script = if k >= charge_from {
                    Script::Super
                } else if c.is_ascii_digit() && k > 0 {
                    Script::Sub
                } else {
                    Script::Normal
                };
                rich = rich.span(c.to_string(), SpanStyle { script, ..base });
            }
        } else {
            let script = match face & FACE_FORMULA {
                FACE_SUBSCRIPT => Script::Sub,
                FACE_SUPERSCRIPT => Script::Super,
                _ => Script::Normal,
            };
            rich = rich.span(part.iter().collect::<String>(), SpanStyle { script, ..base });
        }
    }
    rich
}

/// Markup for `rich` as someone would type it: a one-character script without braces
/// (`CO_2Et`, `NH_4^+`), longer ones braced.
fn compact_markup(rich: &RichText) -> String {
    let mut out = String::new();
    let (mut bold, mut italic) = (false, false);
    let reserved = |c: char| matches!(c, '\\' | '*' | '/' | '^' | '_' | '{' | '}');
    for span in rich.spans.iter().filter(|s| !s.text.is_empty()) {
        if span.style.bold != bold {
            out.push_str("**");
            bold = span.style.bold;
        }
        if span.style.italic != italic {
            out.push_str("//");
            italic = span.style.italic;
        }
        let escaped: String = span.text.chars().flat_map(|c| if reserved(c) { vec!['\\', c] } else { vec![c] }).collect();
        let marker = match span.style.script {
            Script::Normal => None,
            Script::Sub => Some('_'),
            Script::Super => Some('^'),
        };
        let mut chars = span.text.chars();
        match (marker, chars.next(), chars.next()) {
            (None, _, _) => out.push_str(&escaped),
            (Some(m), Some(c), None) if !c.is_whitespace() => {
                out.push(m);
                out.push_str(&escaped);
            }
            (Some(m), _, _) => {
                out.push(m);
                out.push('{');
                out.push_str(&escaped);
                out.push('}');
            }
        }
    }
    if bold {
        out.push_str("**");
    }
    if italic {
        out.push_str("//");
    }
    out
}

/// The structures of `mol` as fragments (one per connected structure), with ids from `next`.
/// Positions are moved by `shift` (CDX units).
pub(crate) fn fragments(mol: &Molecule, shift: (f64, f64), next: &mut u32) -> Vec<CdxObject> {
    let mut id = || {
        *next += 1;
        *next
    };
    // Every atom is looked up here, and its neighbours with it: from an index built once, as
    // the molecule's own lookups scan all atoms and bonds each time.
    let index = MolIndex::new(mol);
    let visible: Vec<&super::Atom> = mol.atoms.iter().filter(|a| is_node(&index, a.id)).collect();
    // The structures, in the order of their first node: each one's nodes in atom order and its
    // bonds in bond order. A structure is everything bonded to its first node, through any atom.
    let mut structure_of: HashMap<u32, usize> = HashMap::new();
    let mut structures: Vec<(Vec<&super::Atom>, Vec<&super::Bond>)> = Vec::new();
    for &atom in &visible {
        let k = match structure_of.get(&atom.id) {
            Some(&k) => k,
            None => {
                let k = structures.len();
                structures.push((Vec::new(), Vec::new()));
                structure_of.insert(atom.id, k);
                let mut queue = vec![atom.id];
                while let Some(a) = queue.pop() {
                    for n in index.neighbor_atom_ids(a) {
                        if let std::collections::hash_map::Entry::Vacant(e) = structure_of.entry(n) {
                            e.insert(k);
                            queue.push(n);
                        }
                    }
                }
                k
            }
        };
        structures[k].0.push(atom);
    }
    let visible_ids: HashSet<u32> = visible.iter().map(|a| a.id).collect();
    for bond in &mol.bonds {
        if visible_ids.contains(&bond.begin) && visible_ids.contains(&bond.end) {
            structures[structure_of[&bond.begin]].1.push(bond);
        }
    }
    let mut out = Vec::new();
    for (members, bonds) in structures {
        let mut frag = CdxObject::new(OBJ_FRAGMENT, id());
        let mut bbox = None;
        let mut nodes: HashMap<u32, u32> = HashMap::new();
        for atom in members {
            let node_id = id();
            nodes.insert(atom.id, node_id);
            let at = (atom.pos[0] as f64 * SCALE + shift.0, atom.pos[1] as f64 * SCALE + shift.1);
            bbox = Some(union(bbox, [at.0, at.1, at.0, at.1]));
            let mut node = CdxObject::new(OBJ_NODE, node_id);
            node.set(PROP_2D_POSITION, point(at.0, at.1));
            let rules = node_rules(&index, atom);
            match rules.kind {
                NodeKind::Carbon => {}
                NodeKind::Element(z) => node.set(PROP_NODE_ELEMENT, (z as i16).to_le_bytes().to_vec()),
                NodeKind::Unspecified => node.set(PROP_NODE_TYPE, NODE_UNSPECIFIED.to_le_bytes().to_vec()),
            }
            if let Some(h) = rules.num_hydrogens {
                node.set(PROP_ATOM_NUM_HYDROGENS, h.to_le_bytes().to_vec());
            }
            if rules.charge != 0 {
                node.set(PROP_ATOM_CHARGE, vec![rules.charge as u8]);
            }
            if let Some(label) = &rules.label {
                node.children.push(label_object(id(), at, label));
            }
            frag.children.push(node);
        }
        for bond in bonds {
            let (Some(&begin), Some(&end)) = (nodes.get(&bond.begin), nodes.get(&bond.end)) else { continue };
            let mut b = CdxObject::new(OBJ_BOND, id());
            b.set(PROP_BOND_BEGIN, begin.to_le_bytes().to_vec());
            b.set(PROP_BOND_END, end.to_le_bytes().to_vec());
            // kCDXBondOrder flags. ChemDraw keeps a dative bond as an order of its own (0x1000),
            // not as a display.
            let order: i16 = match (&bond.order, &bond.stereo) {
                (_, BondStereo::Dative) => ORDER_DATIVE,
                (BondOrder::Single, _) => 1,
                (BondOrder::Double, _) => 2,
                (BondOrder::Triple, _) => 4,
                (BondOrder::Quadruple, _) => 8,
            };
            if order != 1 {
                b.set(PROP_BOND_ORDER, order.to_le_bytes().to_vec());
            }
            let display: i16 = match bond.stereo {
                BondStereo::None | BondStereo::Dative => 0,
                BondStereo::Dashed => 1,
                BondStereo::Hashed => 2,
                BondStereo::WedgeDown => 3,
                BondStereo::Bold => 5,
                BondStereo::WedgeUp => 6,
                BondStereo::Wavy => 8,
                BondStereo::HollowWedge => 9,
            };
            if display != 0 {
                b.set(PROP_BOND_DISPLAY, display.to_le_bytes().to_vec());
            }
            // kCDXBondDoublePosition: only a side the user chose (the "User…" values); an
            // automatic one is ChemDraw's default and left unwritten.
            let position: Option<i16> = match bond.double_position {
                _ if bond.order != BondOrder::Double => None,
                DoublePosition::Auto => None,
                DoublePosition::Center => Some(0x0100),
                DoublePosition::Right => Some(0x0101),
                DoublePosition::Left => Some(0x0102),
            };
            if let Some(position) = position {
                b.set(PROP_BOND_DOUBLE_POSITION, position.to_le_bytes().to_vec());
            }
            frag.children.push(b);
        }
        if let Some(bbox) = bbox {
            frag.props.insert(0, (PROP_BOUNDING_BOX, rect(bbox)));
        }
        out.push(frag);
    }
    out
}

/// How far to move `mol` so it does not start above or left of the page.
fn page_shift(mol: &Molecule) -> (f64, f64) {
    let min = |i: usize| mol.atoms.iter().map(|a| a.pos[i] as f64 * SCALE).fold(f64::INFINITY, f64::min);
    let fix = |m: f64| if m.is_finite() && m < PAGE_MARGIN { (PAGE_MARGIN - m).ceil() } else { 0.0 };
    (fix(min(0)), fix(min(1)))
}

pub(crate) fn bounding_box(obj: &CdxObject) -> Option<[f64; 4]> {
    let v = obj.prop(PROP_BOUNDING_BOX).filter(|v| v.len() >= 16)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some([n(4), n(0), n(12), n(8)])
}

/// An empty ChemDraw document in ChemDraw's own style ([`DOCUMENT_STYLE`]), naming this
/// program as its creator.
pub(crate) fn new_document() -> CdxObject {
    let mut doc = CdxObject::new(OBJ_DOCUMENT, 0);
    doc.set(PROP_CREATION_PROGRAM, cdx_string(&[], &format!("ChemBuilder {}", env!("CARGO_PKG_VERSION"))));
    for (tag, value) in DOCUMENT_STYLE {
        doc.set(tag, hex(value));
    }
    doc
}

/// A new ChemDraw document holding `mol` (an empty page when it has no atoms).
pub fn molecule_to_cdx_document(mol: &Molecule) -> Vec<u8> {
    let mut doc = new_document();
    let mut next = 0;
    let mut page = CdxObject::new(OBJ_PAGE, {
        next += 1;
        next
    });
    page.children = fragments(mol, page_shift(mol), &mut next);
    if let Some(bbox) = page.children.iter().filter_map(bounding_box).reduce(|a, b| union(Some(a), b)) {
        doc.props.insert(1, (PROP_BOUNDING_BOX, rect(bbox)));
    }
    doc.children.push(page);
    cdx_codec::write(&doc)
}

/// Serialize `mol` to CDX bytes, or None if empty (nothing to put on the clipboard).
pub fn molecule_to_cdx_bytes(mol: &Molecule) -> Option<Vec<u8>> {
    (!mol.atoms.is_empty()).then(|| molecule_to_cdx_document(mol))
}

/// Drop every fragment inside `obj` (and groups left empty), counting what else stays.
fn strip_structures(obj: &mut CdxObject) {
    obj.children.retain_mut(|c| match c.tag {
        OBJ_FRAGMENT => false,
        OBJ_GROUP => {
            strip_structures(c);
            !c.children.is_empty()
        }
        _ => true,
    });
}

/// `original` (a ChemDraw document) with its structures replaced by `mol`. The document's
/// settings and everything on the page that is not a structure — arrows, captions, shapes —
/// stay as they were, so editing an object made in ChemDraw loses nothing the editor cannot
/// show. Not a CDX document: a new one, as [`molecule_to_cdx_document`].
pub fn merge_into_cdx(original: &[u8], mol: &Molecule) -> Vec<u8> {
    let Some(mut doc) = cdx_codec::parse(original).filter(|d| d.tag == OBJ_DOCUMENT) else {
        return molecule_to_cdx_document(mol);
    };
    for page in doc.children.iter_mut().filter(|c| c.tag == OBJ_PAGE) {
        strip_structures(page);
    }
    let mut next = doc.max_id();
    if !doc.children.iter().any(|c| c.tag == OBJ_PAGE) {
        next += 1;
        doc.children.push(CdxObject::new(OBJ_PAGE, next));
    }
    let new = fragments(mol, page_shift(mol), &mut next);
    let page = doc.children.iter_mut().find(|c| c.tag == OBJ_PAGE).expect("a page");
    page.children.extend(new);
    let bbox = doc
        .children
        .iter()
        .filter(|c| c.tag == OBJ_PAGE)
        .flat_map(|p| p.children.iter())
        .filter_map(bounding_box)
        .reduce(|a, b| union(Some(a), b));
    match bbox {
        Some(bbox) => doc.set(PROP_BOUNDING_BOX, rect(bbox)),
        None => doc.props.retain(|(t, _)| *t != PROP_BOUNDING_BOX),
    }
    cdx_codec::write(&doc)
}

/// Read one fragment's own nodes and bonds into `mol` (not the hidden insides of an
/// abbreviation, which ChemDraw keeps as a fragment inside its node).
pub(crate) fn read_fragment(frag: &CdxObject, mol: &mut Molecule) {
    let mut atoms: HashMap<u32, u32> = HashMap::new();
    // Atoms ChemDraw gives a hydrogen count (NH4+, H2O on their own), with their position.
    let mut hydrogens: Vec<(u32, [f32; 2], usize)> = Vec::new();
    // The fragment's bonds so far (atom pair, lower id first) and the atoms they join. The atoms
    // are new to `mol`, so no bond read before can join them: these are all a repeated bond
    // can repeat, and looking them up here keeps reading linear in the fragment's size.
    let mut bonds: HashSet<(u32, u32)> = HashSet::new();
    let mut bonded: HashSet<u32> = HashSet::new();
    for node in frag.children.iter().filter(|c| c.tag == OBJ_NODE) {
        let node_type = node.prop_i16(PROP_NODE_TYPE).unwrap_or(NODE_ELEMENT);
        if node_type == NODE_EXTERNAL_CONNECTION_POINT {
            continue;
        }
        let (x, y) = match node.prop(PROP_2D_POSITION).filter(|p| p.len() >= 8) {
            Some(p) => (
                i32::from_le_bytes([p[4], p[5], p[6], p[7]]) as f64 / SCALE,
                i32::from_le_bytes([p[0], p[1], p[2], p[3]]) as f64 / SCALE,
            ),
            None => (0.0, 0.0),
        };
        let charge = node.prop_int(PROP_ATOM_CHARGE).unwrap_or(0).clamp(-128, 127) as i8;
        let label = node
            .children
            .iter()
            .find(|c| c.tag == OBJ_TEXT)
            .and_then(|t| t.prop(PROP_TEXT))
            .and_then(parse_cdx_string)
            .map(|(runs, text)| rich_of(&runs, &text));
        let element = match node.prop_i16(PROP_NODE_ELEMENT) {
            Some(z) if node_type == NODE_ELEMENT => elements::symbol(z as u16).unwrap_or("C").to_string(),
            None if node_type == NODE_ELEMENT => "C".to_string(),
            // A label of its own (Ph, CO2Et, R): the text, without the charge the editor adds back.
            _ => match label {
                Some(mut rich) => {
                    if charge != 0 {
                        while rich.spans.last().is_some_and(|s| s.style.script == Script::Super) {
                            rich.spans.pop();
                        }
                    }
                    rich.spans.retain(|s: &RichSpan| !s.text.is_empty());
                    let markup = compact_markup(&rich);
                    if markup.is_empty() { "C".to_string() } else { markup }
                }
                None => match node.prop_i16(PROP_NODE_ELEMENT).and_then(|z| elements::symbol(z as u16)) {
                    Some(symbol) => symbol.to_string(),
                    None => "C".to_string(),
                },
            },
        };
        let pos = [x as f32, y as f32];
        let atom = mol.add_atom(element, pos, charge);
        atoms.insert(node.id, atom);
        if let Some(h) = hydrogen_count(node) {
            hydrogens.push((atom, pos, h));
        }
    }
    for bond in frag.children.iter().filter(|c| c.tag == OBJ_BOND) {
        let (Some(begin), Some(end)) = (bond.prop_u32(PROP_BOND_BEGIN), bond.prop_u32(PROP_BOND_END)) else { continue };
        let (Some(&a), Some(&b)) = (atoms.get(&begin), atoms.get(&end)) else { continue };
        let order_flags = bond.prop_i16(PROP_BOND_ORDER);
        let order = match order_flags {
            Some(2) => BondOrder::Double,
            Some(4) => BondOrder::Triple,
            Some(8) => BondOrder::Quadruple,
            _ => BondOrder::Single,
        };
        // The "…End" displays have the narrow end at the bond's end: turned around.
        let (stereo, a, b) = match bond.prop_i16(PROP_BOND_DISPLAY).unwrap_or(0) {
            1 => (BondStereo::Dashed, a, b),
            2 => (BondStereo::Hashed, a, b),
            3 => (BondStereo::WedgeDown, a, b),
            4 => (BondStereo::WedgeDown, b, a),
            5 => (BondStereo::Bold, a, b),
            6 => (BondStereo::WedgeUp, a, b),
            7 => (BondStereo::WedgeUp, b, a),
            8 => (BondStereo::Wavy, a, b),
            9 => (BondStereo::HollowWedge, a, b),
            10 => (BondStereo::HollowWedge, b, a),
            _ if order_flags == Some(ORDER_DATIVE) => (BondStereo::Dative, a, b),
            _ => (BondStereo::None, a, b),
        };
        let double_position = match bond.prop_i16(PROP_BOND_DOUBLE_POSITION) {
            Some(0x0100) => DoublePosition::Center,
            Some(0x0101) => DoublePosition::Right,
            Some(0x0102) => DoublePosition::Left,
            _ => DoublePosition::Auto,
        };
        // A bond given twice is one bond, the first, as `Molecule::add_bond` has it.
        if bonds.insert((a.min(b), a.max(b))) {
            mol.push_bond(a, b, order, stereo);
            if let Some(last) = mol.bonds.last_mut() {
                last.double_position = double_position;
            }
            bonded.extend([a, b]);
        }
    }
    // An atom on its own shows no hydrogens here unless it has them: ChemDraw's NH4+ would
    // come back as N+. They are the label's (group) hydrogens, as an `n` key's NH2 has.
    for (atom, at, count) in hydrogens {
        if bonded.contains(&atom) {
            continue;
        }
        for k in 0..count {
            let angle = k as f32 * std::f32::consts::TAU / count as f32;
            // A group's hydrogen (`group_h`), new and so not bonded yet.
            let h = mol.add_fragment_atom("H".to_string(), [at[0] + angle.cos(), at[1] + angle.sin()], 0);
            mol.push_bond(atom, h, BondOrder::Single, BondStereo::None);
        }
    }
}

/// The hydrogens an element node's count (NH4+) gives it, as many as [`read_fragment`] adds
/// for them: at most 8 (and only to a node that ends up with no bonds).
fn hydrogen_count(node: &CdxObject) -> Option<usize> {
    if node.prop_i16(PROP_NODE_TYPE).unwrap_or(NODE_ELEMENT) != NODE_ELEMENT {
        return None;
    }
    node.prop_int(PROP_ATOM_NUM_HYDROGENS).filter(|&h| h > 0).map(|h| h.min(8) as usize)
}

/// Add up the most atoms and bonds [`read_container`] can make of `obj`'s structures: a node
/// and a bond each, and each node's hydrogens with their bonds, whether or not the node ends up
/// with bonds of its own (which would leave its hydrogens out).
pub(crate) fn count_structures(obj: &CdxObject, atoms: &mut usize, bonds: &mut usize) {
    for child in &obj.children {
        match child.tag {
            OBJ_FRAGMENT => {
                for c in &child.children {
                    match c.tag {
                        OBJ_NODE => {
                            let h = hydrogen_count(c).unwrap_or(0);
                            *atoms += 1 + h;
                            *bonds += h;
                        }
                        OBJ_BOND => *bonds += 1,
                        _ => {}
                    }
                }
            }
            OBJ_GROUP => count_structures(child, atoms, bonds),
            _ => {}
        }
    }
}

fn read_container(obj: &CdxObject, mol: &mut Molecule, others: &mut usize) {
    for child in &obj.children {
        match child.tag {
            OBJ_FRAGMENT => read_fragment(child, mol),
            OBJ_GROUP => read_container(child, mol, others),
            _ => *others += 1,
        }
    }
}

/// The structures of a CDX document (empty when it has none), or `None` when the bytes are
/// not CDX, or its structures are larger than [`MAX_READ_ATOMS`] / [`MAX_READ_BONDS`] allow.
pub fn read_cdx(bytes: &[u8]) -> Option<CdxImport> {
    read_document(&cdx_codec::parse(bytes)?)
}

/// The structures of a CDXML document, read as [`read_cdx`] reads CDX: both are the same
/// object tree, written as XML. `None` when the text is not CDXML, or its structures are too
/// large.
pub fn read_cdxml(xml: &str) -> Option<CdxImport> {
    read_document(&cdxml_document(xml)?)
}

/// A CDXML document as the object tree a CDX file holds.
pub fn cdxml_document(xml: &str) -> Option<CdxObject> {
    cdx_file_rs::cdxml::parse_cdxml(xml).ok().map(CdxObject::from)
}

/// A CDX object tree written as CDXML.
pub fn cdxml_of_document(doc: &CdxObject) -> String {
    cdx_file_rs::cdxml::write_cdxml(&doc.clone().into())
}

/// The structures of a document object tree; see [`read_cdx`].
fn read_document(doc: &CdxObject) -> Option<CdxImport> {
    if doc.tag != OBJ_DOCUMENT {
        return None;
    }
    let pages = || doc.children.iter().filter(|c| c.tag == OBJ_PAGE);
    let (mut atoms, mut bonds) = (0, 0);
    for page in pages() {
        count_structures(page, &mut atoms, &mut bonds);
    }
    // Checked before anything is built, on an upper bound of what reading makes (hydrogens
    // included): the molecule read never has more atoms or bonds than the limits.
    if atoms > MAX_READ_ATOMS || bonds > MAX_READ_BONDS {
        return None;
    }
    let mut molecule = Molecule::default();
    let mut other_objects = 0;
    for page in pages() {
        read_container(page, &mut molecule, &mut other_objects);
    }
    Some(CdxImport { molecule, other_objects })
}

/// Parse CDX bytes (e.g. pasted from ChemDraw, or our own copy) into a Molecule.
/// Returns None if the bytes aren't a parseable CDX with at least one atom.
pub fn cdx_bytes_to_molecule(bytes: &[u8]) -> Option<Molecule> {
    read_cdx(bytes).map(|i| i.molecule).filter(|m| !m.atoms.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elements_of(mol: &Molecule) -> Vec<String> {
        let mut e: Vec<String> = mol.atoms.iter().map(|a| a.element.clone()).collect();
        e.sort();
        e
    }

    #[test]
    fn produces_cdx_with_magic_header() {
        let mut mol = Molecule::default();
        let a = mol.add_atom("O".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [1.5, 0.0], 0);
        mol.add_bond(a, b, BondOrder::Double);
        let bytes = molecule_to_cdx_bytes(&mol).expect("should produce bytes");
        assert_eq!(&bytes[0..8], b"VjCD0100", "CDX files start with the magic header");
    }

    #[test]
    fn empty_molecule_yields_none_but_an_empty_document_reads_back_empty() {
        assert!(molecule_to_cdx_bytes(&Molecule::default()).is_none());
        let doc = molecule_to_cdx_document(&Molecule::default());
        let back = read_cdx(&doc).expect("a document");
        assert!(back.molecule.atoms.is_empty());
    }

    /// Verify our reader against a REAL ChemDraw CDX (a brominated naphthol captured from
    /// ChemDraw's clipboard via the `clipdump` tool). This pins our parsing to ChemDraw's
    /// actual byte layout — coordinate order, element/bond property tags, u32 bond refs.
    #[test]
    fn parse_real_chemdraw_cdx() {
        let bytes = include_bytes!("../../tests/fixtures/chemdraw_real.cdx");
        let mol = cdx_bytes_to_molecule(bytes).expect("parse real ChemDraw CDX");
        assert_eq!(mol.atoms.len(), 12);
        assert_eq!(mol.bonds.len(), 13);
        assert!(mol.atoms.iter().any(|a| a.element == "O"), "finds the oxygen");
        assert!(mol.atoms.iter().any(|a| a.element == "Br"), "finds the bromine");
        assert!(mol.bonds.iter().filter(|b| b.order == BondOrder::Double).count() >= 4,
            "finds the aromatic double bonds");
    }

    /// ChemDraw 26's own file (saved from SMILES `C[C@H](N)C(=O)[O-].[NH4+]`): charges, the
    /// wedge, and two structures inside a group.
    #[test]
    fn reads_chemdraw_charges_wedges_and_groups() {
        let bytes = include_bytes!("../../tests/fixtures/chemdraw26_charge_stereo.cdx");
        let mol = cdx_bytes_to_molecule(bytes).expect("parses");
        let heavy = Molecule { atoms: mol.atoms.iter().filter(|a| a.element != "H").cloned().collect(), ..mol.clone() };
        assert_eq!(elements_of(&heavy), ["C", "C", "C", "N", "N", "O", "O"]);
        // The ammonium on its own keeps its four hydrogens (in its label, "NH4+").
        let ammonium = mol.atoms.iter().find(|a| a.element == "N" && a.charge == 1).unwrap();
        assert_eq!(depict::displayed_h_count(&mol, ammonium.id), 4);
        assert!(mol.atoms.iter().filter(|a| a.element == "H").all(|h| h.group_h && depict::is_folded_h(&mol, h.id)));
        // And they go back to ChemDraw as its hydrogen count, not as atoms.
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        let mut nodes = Vec::new();
        find(&doc, OBJ_NODE, &mut nodes);
        assert_eq!(nodes.len(), 7);
        let n = nodes.iter().find(|n| n.prop_int(PROP_ATOM_CHARGE) == Some(1)).unwrap();
        assert_eq!(n.prop_i16(PROP_ATOM_NUM_HYDROGENS), Some(4));
        let charges: Vec<(String, i8)> = mol.atoms.iter().filter(|a| a.charge != 0).map(|a| (a.element.clone(), a.charge)).collect();
        assert_eq!(charges.len(), 2);
        assert!(charges.contains(&("O".to_string(), -1)) && charges.contains(&("N".to_string(), 1)));
        let wedge = mol.bonds.iter().find(|b| b.stereo == BondStereo::WedgeUp).expect("the wedge");
        // Narrow end on the stereocentre (a carbon), wide end on the nitrogen.
        assert_eq!(mol.atom_by_id(wedge.begin).unwrap().element, "C");
        assert_eq!(mol.atom_by_id(wedge.end).unwrap().element, "N");
    }

    #[test]
    fn reads_the_powerpoint_sample() {
        let bytes = include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx");
        let mol = cdx_bytes_to_molecule(bytes).expect("parses");
        assert_eq!((mol.atoms.len(), mol.bonds.len()), (6, 6));
        // ChemDraw's 14.4 pt bond is the editor's 1.5 units.
        let b = &mol.bonds[0];
        let (p, q) = (mol.atom_by_id(b.begin).unwrap().pos, mol.atom_by_id(b.end).unwrap().pos);
        let len = ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt();
        assert!((len - 1.5).abs() < 0.01, "bond length {len}");
    }

    /// The document style is ChemDraw's: the bond length property is the one ChemDraw reads.
    #[test]
    fn writes_chemdraws_document_style() {
        let mut mol = Molecule::default();
        mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        assert_eq!(doc.prop_u32(0x0805), Some(943_718), "14.4 pt");
        assert!(doc.prop(0x0100).is_some(), "font table");
    }

    fn find<'a>(obj: &'a CdxObject, tag: u16, out: &mut Vec<&'a CdxObject>) {
        for c in &obj.children {
            if c.tag == tag {
                out.push(c);
            }
            find(c, tag, out);
        }
    }

    /// The ethyl cation CH3–CH2+: ChemDraw reads NumHydrogens as the cation's own count, so it
    /// must be the carbocation's two, not the four a "valence + charge" count would give.
    #[test]
    fn writes_a_carbocations_hydrogen_count() {
        let mut mol = Molecule::default();
        let c1 = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c2 = mol.add_atom("C".to_string(), [1.3, 0.0], 1);
        mol.add_bond(c1, c2, BondOrder::Single);

        let doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        let mut nodes = Vec::new();
        find(&doc, OBJ_NODE, &mut nodes);
        let cation = nodes.iter().find(|n| n.prop_int(PROP_ATOM_CHARGE) == Some(1)).unwrap();
        assert_eq!(cation.prop_i16(PROP_ATOM_NUM_HYDROGENS), Some(2));
        let (_, label) = parse_cdx_string(cation.children[0].prop(PROP_TEXT).unwrap()).unwrap();
        assert_eq!(label, "CH2+");
    }

    #[test]
    fn writes_labels_charges_and_folded_hydrogens_like_chemdraw() {
        // H2N–CH2–C(=O)–O(-)
        let mut mol = Molecule::default();
        let n = mol.add_atom("N".to_string(), [0.0, 0.0], 0);
        let c1 = mol.add_atom("C".to_string(), [1.3, 0.75], 0);
        let c2 = mol.add_atom("C".to_string(), [2.6, 0.0], 0);
        let o1 = mol.add_atom("O".to_string(), [2.6, -1.5], 0);
        let o2 = mol.add_atom("O".to_string(), [3.9, 0.75], -1);
        mol.add_bond(n, c1, BondOrder::Single);
        mol.add_bond(c1, c2, BondOrder::Single);
        mol.add_bond(c2, o1, BondOrder::Double);
        mol.add_bond(c2, o2, BondOrder::Single);
        // A hydrogen of the NH2 group: folded into the label, not a node.
        let h = mol.add_atom("H".to_string(), [-1.0, 0.5], 0);
        mol.atom_by_id_mut(h).unwrap().group_h = true;
        mol.add_bond(n, h, BondOrder::Single);

        let doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        let mut nodes = Vec::new();
        find(&doc, OBJ_NODE, &mut nodes);
        assert_eq!(nodes.len(), 5, "the folded hydrogen is not a node");
        let nitrogen = nodes.iter().find(|n| n.prop_i16(PROP_NODE_ELEMENT) == Some(7)).unwrap();
        assert_eq!(nitrogen.prop_i16(PROP_ATOM_NUM_HYDROGENS), Some(2));
        let text = &nitrogen.children[0];
        let (runs, label) = parse_cdx_string(text.prop(PROP_TEXT).unwrap()).unwrap();
        assert_eq!(runs, [(0, FACE_FORMULA)]);
        // The bond comes from the right, so the hydrogens are on the left: "H2N", right-aligned.
        assert_eq!(label, "H2N");
        assert_eq!(text.prop(PROP_LABEL_ALIGNMENT), Some(&[ALIGN_RIGHT][..]));
        let anion = nodes.iter().find(|n| n.prop_int(PROP_ATOM_CHARGE) == Some(-1)).unwrap();
        let (_, label) = parse_cdx_string(anion.children[0].prop(PROP_TEXT).unwrap()).unwrap();
        assert_eq!(label, "O-");
        // Carbons carry no label.
        assert_eq!(nodes.iter().filter(|n| n.children.is_empty()).count(), 2);
        let mut bonds = Vec::new();
        find(&doc, OBJ_BOND, &mut bonds);
        assert_eq!(bonds.len(), 4, "no bond to the folded hydrogen");
        let ids: std::collections::HashSet<u32> = nodes.iter().map(|n| n.id).collect();
        assert!(bonds.iter().all(|b| ids.contains(&b.prop_u32(PROP_BOND_BEGIN).unwrap()) && ids.contains(&b.prop_u32(PROP_BOND_END).unwrap())));
    }

    /// "Ac" and "Ts" on an atom are the acetyl and tosyl labels (`assets/fragments/Ac.json`,
    /// `Ts.json`), not actinium and tennessine: they are written as labels, as "Ph" is, and
    /// come back as the same labels.
    #[test]
    fn acetyl_and_tosyl_labels_are_written_as_labels_not_elements() {
        let mut mol = Molecule::default();
        let n = mol.add_atom("N".to_string(), [0.0, 0.0], 0);
        let ac = mol.add_atom("Ac".to_string(), [1.3, 0.75], 0);
        let ts = mol.add_atom("Ts".to_string(), [-1.3, 0.75], 0);
        mol.add_bond(n, ac, BondOrder::Single);
        mol.add_bond(n, ts, BondOrder::Single);

        let doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        let mut nodes = Vec::new();
        find(&doc, OBJ_NODE, &mut nodes);
        for text in ["Ac", "Ts"] {
            let node = nodes
                .iter()
                .find(|n| n.children.first().and_then(|t| t.prop(PROP_TEXT)).and_then(parse_cdx_string).is_some_and(|(_, l)| l == text))
                .unwrap_or_else(|| panic!("{text} has its label"));
            assert_eq!(node.prop_i16(PROP_NODE_TYPE), Some(NODE_UNSPECIFIED), "{text}");
            assert_eq!(node.prop_i16(PROP_NODE_ELEMENT), None, "{text} is no element");
            assert_eq!(node.prop_i16(PROP_ATOM_NUM_HYDROGENS), None, "{text}");
        }
        let back = cdx_bytes_to_molecule(&molecule_to_cdx_document(&mol)).expect("reads back");
        assert_eq!(elements_of(&back), ["Ac", "N", "Ts"]);
    }

    /// Actinium from ChemDraw (a node of element 89) still reads as "Ac".
    #[test]
    fn reads_an_actinium_node_as_ac() {
        let mut mol = Molecule::default();
        mol.add_atom("Pt".to_string(), [0.0, 0.0], 0);
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        fn set_element(obj: &mut CdxObject) {
            for c in &mut obj.children {
                if c.tag == OBJ_NODE && c.prop_i16(PROP_NODE_ELEMENT) == Some(78) {
                    c.set(PROP_NODE_ELEMENT, 89i16.to_le_bytes().to_vec());
                }
                set_element(c);
            }
        }
        set_element(&mut doc);
        let back = cdx_bytes_to_molecule(&cdx_codec::write(&doc)).expect("reads");
        assert_eq!(elements_of(&back), ["Ac"]);
    }

    /// A chain of carbons `C0–C1–…`, bond k from Ck to Ck+1 with the given order and stereo.
    fn chain_of(bonds: &[(BondOrder, BondStereo, DoublePosition)]) -> Molecule {
        let mut mol = Molecule::default();
        let atoms: Vec<u32> = (0..=bonds.len())
            .map(|k| mol.add_atom("C".to_string(), [k as f32 * 1.5, (k % 2) as f32], 0))
            .collect();
        for (k, (order, stereo, position)) in bonds.iter().enumerate() {
            let id = mol.add_bond(atoms[k], atoms[k + 1], order.clone());
            let bond = mol.bond_by_id_mut(id).unwrap();
            bond.stereo = stereo.clone();
            bond.double_position = *position;
        }
        mol
    }

    /// The bond styles of the bond context menu that the first CDX writer had no value for —
    /// quadruple, plain hashed, hollow wedge, dative — and a chosen double-bond side all
    /// survive a write and read, with each bond's direction kept.
    #[test]
    fn round_trips_the_bond_menus_orders_styles_and_double_positions() {
        use DoublePosition::*;
        let bonds = [
            (BondOrder::Quadruple, BondStereo::None, Auto),
            (BondOrder::Single, BondStereo::Hashed, Auto),
            (BondOrder::Single, BondStereo::HollowWedge, Auto),
            (BondOrder::Single, BondStereo::Dative, Auto),
            (BondOrder::Double, BondStereo::None, Left),
            (BondOrder::Double, BondStereo::None, Right),
            (BondOrder::Double, BondStereo::None, Center),
            (BondOrder::Double, BondStereo::None, Auto),
        ];
        let mol = chain_of(&bonds);
        let back = cdx_bytes_to_molecule(&molecule_to_cdx_document(&mol)).expect("reads back");
        assert_eq!(back.bonds.len(), bonds.len());
        for (k, (bond, (order, stereo, position))) in back.bonds.iter().zip(&bonds).enumerate() {
            assert_eq!((&bond.order, &bond.stereo, bond.double_position), (order, stereo, *position), "bond {k}");
            assert_eq!((bond.begin, bond.end), (back.atoms[k].id, back.atoms[k + 1].id), "bond {k} keeps its direction");
        }
    }

    /// ChemDraw's "…End" hollow wedge has its narrow end at the bond's end atom: read turned around.
    #[test]
    fn a_hollow_wedge_drawn_from_the_end_is_read_from_its_narrow_end() {
        let mol = chain_of(&[(BondOrder::Single, BondStereo::HollowWedge, DoublePosition::Auto)]);
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&mol)).unwrap();
        fn flip(obj: &mut CdxObject) {
            if obj.tag == OBJ_BOND {
                obj.set(PROP_BOND_DISPLAY, 10i16.to_le_bytes().to_vec());
            }
            obj.children.iter_mut().for_each(flip);
        }
        flip(&mut doc);
        let back = cdx_bytes_to_molecule(&cdx_codec::write(&doc)).unwrap();
        let bond = &back.bonds[0];
        assert_eq!(bond.stereo, BondStereo::HollowWedge);
        assert_eq!((bond.begin, bond.end), (back.atoms[1].id, back.atoms[0].id));
    }

    #[test]
    fn round_trips_elements_charges_stereo_and_labels() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let pt = mol.add_atom("Pt".to_string(), [1.5, 0.0], 2);
        let se = mol.add_atom("Se".to_string(), [0.0, 1.5], 0);
        let ph = mol.add_atom("Ph".to_string(), [-1.5, 0.0], 0);
        let ester = mol.add_atom("CO_2Et".to_string(), [0.0, -1.5], 0);
        let tbu = mol.add_atom("//t//Bu".to_string(), [1.5, 1.5], 0);
        let n = mol.add_atom("N".to_string(), [3.0, 0.0], 1);
        let stereo = [
            (c, pt, BondStereo::WedgeUp),
            (c, se, BondStereo::WedgeDown),
            (c, ph, BondStereo::Bold),
            (c, ester, BondStereo::Dashed),
            (se, tbu, BondStereo::Wavy),
            (pt, n, BondStereo::None),
        ];
        for (a, b, s) in &stereo {
            let id = mol.add_bond(*a, *b, BondOrder::Single);
            mol.bond_by_id_mut(id).unwrap().stereo = s.clone();
        }
        let id = mol.bond_between(pt, n).unwrap().id;
        mol.bond_by_id_mut(id).unwrap().order = BondOrder::Triple;

        let back = cdx_bytes_to_molecule(&molecule_to_cdx_document(&mol)).expect("reads back");
        assert_eq!(back.atoms.len(), mol.atoms.len());
        let by_element = |m: &Molecule, e: &str| m.atoms.iter().find(|a| a.element == e).cloned();
        for atom in &mol.atoms {
            let other = by_element(&back, &atom.element).unwrap_or_else(|| panic!("{} came back", atom.element));
            assert_eq!(other.charge, atom.charge, "{}", atom.element);
        }
        // Positions keep their shape (the whole structure may move onto the page).
        let d = |m: &Molecule, a: &str, b: &str| {
            let (p, q) = (by_element(m, a).unwrap().pos, by_element(m, b).unwrap().pos);
            ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2)).sqrt()
        };
        assert!((d(&back, "Pt", "Se") - d(&mol, "Pt", "Se")).abs() < 1e-3);
        for (a, b, s) in &stereo {
            let (ea, eb) = (&mol.atom_by_id(*a).unwrap().element, &mol.atom_by_id(*b).unwrap().element);
            let (ia, ib) = (by_element(&back, ea).unwrap().id, by_element(&back, eb).unwrap().id);
            let bond = back.bond_between(ia, ib).expect("bond");
            assert_eq!(&bond.stereo, s, "{ea}-{eb}");
            if *s != BondStereo::None {
                assert_eq!(bond.begin, ia, "narrow end kept on {ea}");
            }
        }
        let triple = back.bonds.iter().filter(|b| b.order == BondOrder::Triple).count();
        assert_eq!(triple, 1);
    }

    #[test]
    fn writing_moves_structures_onto_the_page_once() {
        let mut mol = Molecule::default();
        mol.add_atom("C".to_string(), [-10.0, -4.0], 0);
        mol.add_atom("O".to_string(), [-8.5, -4.0], 0);
        let once = cdx_bytes_to_molecule(&molecule_to_cdx_document(&mol)).unwrap();
        assert!(once.atoms.iter().all(|a| a.pos[0] > 0.0 && a.pos[1] > 0.0));
        let twice = cdx_bytes_to_molecule(&molecule_to_cdx_document(&once)).unwrap();
        for (a, b) in once.atoms.iter().zip(&twice.atoms) {
            assert!((a.pos[0] - b.pos[0]).abs() < 1e-4 && (a.pos[1] - b.pos[1]).abs() < 1e-4, "stable once on the page");
        }
    }

    #[test]
    fn merging_keeps_chemdraws_settings_and_other_objects() {
        let original = include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx");
        let mut doc = cdx_codec::parse(original).unwrap();
        // A caption next to the structure, as ChemDraw would have it on the page.
        let mut caption = CdxObject::new(OBJ_TEXT, 500);
        caption.set(PROP_TEXT, cdx_string(&[(0, 0)], "compound 1"));
        doc.children[0].children.push(caption);
        let original = cdx_codec::write(&doc);

        let mut mol = cdx_bytes_to_molecule(&original).unwrap();
        let first = mol.atoms[0].id;
        let pos = mol.atoms[0].pos;
        let o = mol.add_atom("O".to_string(), [pos[0], pos[1] - 1.5], 0);
        mol.add_bond(first, o, BondOrder::Single);

        let merged = merge_into_cdx(&original, &mol);
        let import = read_cdx(&merged).unwrap();
        assert_eq!(import.molecule.atoms.len(), 7, "the structure is replaced, not doubled");
        assert_eq!(import.other_objects, 1, "the caption stays");
        let doc = cdx_codec::parse(&merged).unwrap();
        assert_eq!(doc.prop(PROP_CREATION_PROGRAM), cdx_codec::parse(&original).unwrap().prop(PROP_CREATION_PROGRAM));
        let mut ids: Vec<u32> = Vec::new();
        fn all_ids(o: &CdxObject, out: &mut Vec<u32>) {
            if o.tag != OBJ_TEXT {
                out.push(o.id);
            }
            o.children.iter().for_each(|c| all_ids(c, out));
        }
        all_ids(&doc, &mut ids);
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "no id is used twice");
    }

    /// A CDX document with one fragment per entry of `chains`: a chain of that many carbons,
    /// each with a hydrogen count of `hydrogens` when that is not 0, and the chain's bonds, each
    /// from one carbon to the next, given `passes` times (none at all when 0).
    fn chain_document(chains: &[usize], passes: usize, hydrogens: i16) -> Vec<u8> {
        let mut next = 1;
        let mut id = || {
            next += 1;
            next
        };
        let mut page = CdxObject::new(OBJ_PAGE, id());
        for &n in chains {
            let mut frag = CdxObject::new(OBJ_FRAGMENT, id());
            let nodes: Vec<u32> = (0..n).map(|_| id()).collect();
            for (k, &node) in nodes.iter().enumerate() {
                let mut obj = CdxObject::new(OBJ_NODE, node);
                obj.set(PROP_2D_POSITION, point(0.0, k as f64 * SCALE));
                if hydrogens != 0 {
                    obj.set(PROP_ATOM_NUM_HYDROGENS, hydrogens.to_le_bytes().to_vec());
                }
                frag.children.push(obj);
            }
            for _ in 0..passes {
                for pair in nodes.windows(2) {
                    let mut bond = CdxObject::new(OBJ_BOND, id());
                    bond.set(PROP_BOND_BEGIN, pair[0].to_le_bytes().to_vec());
                    bond.set(PROP_BOND_END, pair[1].to_le_bytes().to_vec());
                    frag.children.push(bond);
                }
            }
            page.children.push(frag);
        }
        let mut doc = CdxObject::new(OBJ_DOCUMENT, 1);
        doc.children.push(page);
        cdx_codec::write(&doc)
    }

    #[test]
    fn reads_a_chain_as_large_as_allowed_and_drops_repeated_bonds() {
        let import = read_cdx(&chain_document(&[MAX_READ_ATOMS / 2], 2, 0)).expect("within the limits");
        assert_eq!(import.molecule.atoms.len(), MAX_READ_ATOMS / 2);
        assert_eq!(import.molecule.bonds.len(), MAX_READ_ATOMS / 2 - 1, "a bond given twice is one bond");
        let import = read_cdx(&chain_document(&[MAX_READ_ATOMS / 2, MAX_READ_ATOMS / 2], 1, 0)).expect("within the limits");
        assert_eq!(import.molecule.atoms.len(), MAX_READ_ATOMS);
        assert_eq!(import.molecule.bonds.len(), MAX_READ_ATOMS - 2);
    }

    #[test]
    fn refuses_documents_with_more_atoms_or_bonds_than_allowed() {
        // In one fragment, and across fragments of one document.
        assert!(read_cdx(&chain_document(&[MAX_READ_ATOMS + 1], 1, 0)).is_none());
        assert!(read_cdx(&chain_document(&[MAX_READ_ATOMS / 2 + 1, MAX_READ_ATOMS / 2], 1, 0)).is_none());
        // Bonds over the limit, even when they repeat bonds already there.
        assert!(read_cdx(&chain_document(&[MAX_READ_BONDS / 2 + 2], 2, 0)).is_none());
        assert!(cdx_bytes_to_molecule(&chain_document(&[MAX_READ_ATOMS + 1], 1, 0)).is_none());
    }

    #[test]
    fn counts_the_hydrogens_nodes_bring_against_the_limits() {
        // Carbons on their own, each with eight hydrogens (more are read as eight): nine atoms
        // and eight bonds a node.
        let n = MAX_READ_ATOMS / 9;
        for h in [8, 100] {
            let mol = read_cdx(&chain_document(&[n], 0, h)).expect("within the limits").molecule;
            assert_eq!(mol.atoms.len(), 9 * n);
            assert_eq!(mol.bonds.len(), 8 * n);
            assert!(mol.atoms.iter().filter(|a| a.element == "H").all(|a| a.group_h));
        }
        assert!(read_cdx(&chain_document(&[n + 1], 0, 8)).is_none(), "too many atoms with the hydrogens");
        // Hydrogens counted, and their bonds, even on a node whose bonds leave them out.
        let n = MAX_READ_BONDS / 3 + 2;
        assert!(2 * n <= MAX_READ_ATOMS && 2 * (n - 1) <= MAX_READ_BONDS);
        assert!(read_cdx(&chain_document(&[n], 2, 0)).is_some());
        assert!(read_cdx(&chain_document(&[n], 2, 1)).is_none(), "too many bonds with the hydrogens");
    }

    #[test]
    fn formula_runs_become_scripts() {
        assert_eq!(compact_markup(&rich_of(&[(0, FACE_FORMULA)], "NH4+")), "NH_4^+");
        assert_eq!(compact_markup(&rich_of(&[(0, 0), (2, FACE_SUBSCRIPT), (3, 0)], "CO2Et")), "CO_2Et");
        assert_eq!(compact_markup(&rich_of(&[(0, FACE_ITALIC), (1, 0)], "tBu")), "//t//Bu");
        assert_eq!(compact_markup(&rich_of(&[(0, 0), (2, FACE_SUBSCRIPT)], "SO42")), "SO_{42}");
        for markup in ["CO_2Et", "//t//Bu", "**Bn**", "SO_{42}", "N_3"] {
            let rich = RichText::parse(markup);
            assert_eq!(compact_markup(&rich), markup);
        }
    }

    /// A molecule touching every rule the writers have: labels (elements, abbreviations with
    /// markup, deuterium), charges, hydrogens folded into a label and one drawn on purpose, every
    /// bond order and display, a ring fill, a lone ion, and structures whose atoms and bonds are
    /// interleaved in the lists.
    fn writer_fixture() -> Molecule {
        let mut m = Molecule::default();
        let group_h = |m: &mut Molecule, heavy: u32, pos: [f32; 2]| {
            let h = m.add_atom("H".to_string(), pos, 0);
            m.atom_by_id_mut(h).unwrap().group_h = true;
            m.add_bond(heavy, h, BondOrder::Single);
        };
        let bond = |m: &mut Molecule, a: u32, b: u32, stereo: BondStereo| {
            let id = m.add_bond(a, b, BondOrder::Single);
            m.bond_by_id_mut(id).unwrap().stereo = stereo;
        };
        // A filled benzene ring, its atoms and bonds interleaved with a second structure's.
        let mut ring = Vec::new();
        let mut other = Vec::new();
        for i in 0..6 {
            let t = i as f32 * std::f32::consts::PI / 3.0;
            ring.push(m.add_atom("C".to_string(), [1.5 * t.cos(), 1.5 * t.sin()], 0));
            if i % 2 == 0 {
                let k = i / 2;
                other.push(m.add_atom(["N", "C", "S"][k].to_string(), [8.0 + i as f32, 0.0], [1, 0, -1][k]));
            }
        }
        for i in 0..6 {
            let order = if i % 2 == 0 { BondOrder::Double } else { BondOrder::Single };
            m.add_bond(ring[i], ring[(i + 1) % 6], order);
            if i == 1 {
                m.add_bond(other[0], other[1], BondOrder::Single);
            }
        }
        m.add_bond(other[1], other[2], BondOrder::Triple);
        m.set_ring_fill(ring.clone(), [255, 200, 0, 128]);
        // NH2 on the ring's right, and H3N+ (its hydrogens before the N) on the second structure.
        let n = m.add_atom("N".to_string(), [3.0, 0.0], 0);
        m.add_bond(ring[0], n, BondOrder::Single);
        group_h(&mut m, n, [3.8, 0.5]);
        group_h(&mut m, n, [3.8, -0.5]);
        for k in 0..3 {
            group_h(&mut m, other[0], [7.0, k as f32]);
        }
        // An alkoxide on a bold bond, a wedge to an ester label, a hash to Ph, dashed and wavy bonds.
        let o = m.add_atom("O".to_string(), [-3.0, 0.0], -1);
        bond(&mut m, ring[3], o, BondStereo::Bold);
        let ester = m.add_atom("CO_2Et".to_string(), [0.75, 3.0], 0);
        bond(&mut m, ring[1], ester, BondStereo::WedgeUp);
        let ph = m.add_atom("Ph".to_string(), [-0.75, 3.0], 0);
        bond(&mut m, ring[2], ph, BondStereo::WedgeDown);
        let tbu = m.add_atom("//t//Bu".to_string(), [-0.75, -3.0], 0);
        bond(&mut m, ring[4], tbu, BondStereo::Dashed);
        let d = m.add_atom("D".to_string(), [0.75, -3.0], 0);
        bond(&mut m, ring[5], d, BondStereo::Wavy);
        // A hydrogen drawn on purpose stays an atom; a carbanion; a lone chloride.
        let h = m.add_atom("H".to_string(), [-1.0, 5.0], 0);
        m.add_bond(ph, h, BondOrder::Single);
        let anion = m.add_atom("C".to_string(), [12.0, 2.0], -1);
        m.add_bond(other[2], anion, BondOrder::Single);
        m.add_atom("Cl".to_string(), [-5.0, -5.0], -1);
        m
    }

    /// `molecule_to_cdx_document` without its creation program (which names the version).
    fn cdx_without_version(mol: &Molecule) -> Vec<u8> {
        let mut bytes = molecule_to_cdx_document(mol);
        let program = cdx_string(&[], &format!("ChemBuilder {}", env!("CARGO_PKG_VERSION")));
        let mut prop = PROP_CREATION_PROGRAM.to_le_bytes().to_vec();
        prop.extend_from_slice(&(program.len() as u16).to_le_bytes());
        prop.extend_from_slice(&program);
        let at = bytes.windows(prop.len()).position(|w| w == prop).expect("a creation program");
        bytes.drain(at..at + prop.len());
        bytes
    }

    /// Both writers write [`writer_fixture`] byte for byte as they did when the golden files were
    /// saved (before the writers looked atoms up from an index), so a change to how they find
    /// their way around the molecule changes nothing they write.
    #[test]
    fn writers_keep_their_output() {
        let m = writer_fixture();
        assert_eq!(cdx_without_version(&m), include_bytes!("../../tests/fixtures/writer_golden.cdx"));
        let golden = include_str!("../../tests/fixtures/writer_golden.cdxml").replace("\r\n", "\n");
        assert_eq!(super::super::cdxml::to_cdxml(&m), golden);
    }

    /// A zigzag chain of `n` atoms, every tenth an NH whose hydrogen is folded into its label.
    fn chain(n: usize) -> Molecule {
        let mut m = Molecule::default();
        let mut prev = None;
        for i in 0..n {
            let at = [i as f32 * 1.3, if i % 2 == 0 { 0.0 } else { 0.75 }];
            let element = if i % 10 == 5 { "N" } else { "C" };
            let a = m.add_atom(element.to_string(), at, 0);
            if element == "N" {
                let h = m.add_atom("H".to_string(), [at[0], at[1] + 1.0], 0);
                m.atom_by_id_mut(h).unwrap().group_h = true;
                m.add_bond(a, h, BondOrder::Single);
            }
            if let Some(p) = prev {
                m.add_bond(p, a, BondOrder::Single);
            }
            prev = Some(a);
        }
        m
    }

    /// How long both writers take on long chains: run in release with `--ignored --nocapture`.
    #[test]
    #[ignore]
    fn writer_time_on_long_chains() {
        for n in [25_000, 50_000, 100_000] {
            let m = chain(n);
            let t = std::time::Instant::now();
            let cdx = molecule_to_cdx_document(&m);
            let t_cdx = t.elapsed();
            let t = std::time::Instant::now();
            let xml = super::super::cdxml::to_cdxml(&m);
            let t_xml = t.elapsed();
            println!("{n:>7} atoms: cdx {t_cdx:?} ({} bytes), cdxml {t_xml:?} ({} bytes)", cdx.len(), xml.len());
        }
    }
}
