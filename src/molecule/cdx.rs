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
use super::{elements, BondOrder, BondStereo, Molecule};
use crate::widget::draw;
use crate::widget::rich_text::{RichSpan, RichText, Script, SpanStyle};
use std::collections::HashMap;

// Objects (kCDXObj_*).
const OBJ_DOCUMENT: u16 = 0x8000;
const OBJ_PAGE: u16 = 0x8001;
const OBJ_GROUP: u16 = 0x8002;
const OBJ_FRAGMENT: u16 = 0x8003;
const OBJ_NODE: u16 = 0x8004;
const OBJ_BOND: u16 = 0x8005;
const OBJ_TEXT: u16 = 0x8006;

// Properties (kCDXProp_*).
const PROP_CREATION_PROGRAM: u16 = 0x0003;
const PROP_2D_POSITION: u16 = 0x0200;
const PROP_BOUNDING_BOX: u16 = 0x0204;
const PROP_NODE_TYPE: u16 = 0x0400;
const PROP_NODE_ELEMENT: u16 = 0x0402;
const PROP_ATOM_CHARGE: u16 = 0x0421;
const PROP_ATOM_NUM_HYDROGENS: u16 = 0x042B;
const PROP_TEXT: u16 = 0x0700;
const PROP_LABEL_ALIGNMENT: u16 = 0x0705;
const PROP_UTF8_TEXT: u16 = 0x0709;
const PROP_BOND_ORDER: u16 = 0x0600;
const PROP_BOND_DISPLAY: u16 = 0x0601;
const PROP_BOND_BEGIN: u16 = 0x0604;
const PROP_BOND_END: u16 = 0x0605;

/// Node types (kCDXNodeType_*) this module writes or reads specially.
const NODE_UNSPECIFIED: i16 = 0;
const NODE_ELEMENT: i16 = 1;
const NODE_EXTERNAL_CONNECTION_POINT: i16 = 12;

/// Label alignment (kCDXLabelAlignment_*): the first character on the atom, or the last.
const ALIGN_LEFT: u8 = 1;
const ALIGN_RIGHT: u8 = 3;

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
const LABEL_FONT: u16 = 3;
/// Label size in 1/20 pt (10 pt).
const LABEL_SIZE: u16 = 200;
/// Formula face: digits become subscripts and a trailing charge a superscript.
const FACE_FORMULA: u16 = 0x60;
const FACE_BOLD: u16 = 0x01;
const FACE_ITALIC: u16 = 0x02;
const FACE_SUBSCRIPT: u16 = 0x20;
const FACE_SUPERSCRIPT: u16 = 0x40;

/// Molecule units → CDX units (1/65536 pt): the editor's 1.5-unit bond is ChemDraw's 14.4 pt.
pub const SCALE: f64 = 14.4 / 1.5 * 65536.0;

/// One point in CDX units.
const PT: f64 = 65536.0;
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
fn cdx_string(runs: &[(u16, u16)], text: &str) -> Vec<u8> {
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
fn parse_cdx_string(v: &[u8]) -> Option<(Vec<(usize, u16)>, String)> {
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

fn point(x: f64, y: f64) -> Vec<u8> {
    let mut v = Vec::with_capacity(8);
    v.extend_from_slice(&(y.round() as i32).to_le_bytes());
    v.extend_from_slice(&(x.round() as i32).to_le_bytes());
    v
}

/// A CDXRectangle: top, left, bottom, right.
fn rect(r: [f64; 4]) -> Vec<u8> {
    let [left, top, right, bottom] = r;
    [top, left, bottom, right].iter().flat_map(|v| (v.round() as i32).to_le_bytes()).collect()
}

fn union(a: Option<[f64; 4]>, b: [f64; 4]) -> [f64; 4] {
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

/// The text object labelling an atom at `at` (CDX units): `label` with its style runs, aligned on
/// the first character (`ALIGN_LEFT`) or the last.
fn label_object(id: u32, at: (f64, f64), runs: &[(u16, u16)], text: &str, align: u8) -> CdxObject {
    let (x, y) = at;
    let first = text.chars().next().map_or(0.0, arial_width);
    let last = text.chars().last().map_or(0.0, arial_width);
    let width: f64 = text.chars().map(arial_width).sum();
    let (origin_x, left) = if align == ALIGN_RIGHT { (x + last / 2.0, x + last / 2.0 - width) } else { (x - first / 2.0, x - first / 2.0) };
    let baseline = y + LABEL_BASELINE;
    let mut t = CdxObject::new(OBJ_TEXT, id);
    t.set(PROP_2D_POSITION, point(origin_x, baseline));
    t.set(PROP_BOUNDING_BOX, rect([left, baseline - 7.2 * PT, left + width, baseline + 2.1 * PT]));
    t.set(PROP_LABEL_ALIGNMENT, vec![align]);
    t.set(PROP_TEXT, cdx_string(runs, text));
    t.set(PROP_UTF8_TEXT, cdx_string(runs, text));
    t
}

/// Style runs (start char, face) for rich text, and its characters.
fn runs_of(rich: &RichText) -> (Vec<(u16, u16)>, String) {
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
fn rich_of(runs: &[(usize, u16)], text: &str) -> RichText {
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
fn fragments(mol: &Molecule, shift: (f64, f64), next: &mut u32) -> Vec<CdxObject> {
    let mut id = || {
        *next += 1;
        *next
    };
    let visible: Vec<&super::Atom> = mol.atoms.iter().filter(|a| !draw::is_folded_h(mol, a.id)).collect();
    let mut placed: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for start in &visible {
        if placed.contains(&start.id) {
            continue;
        }
        let members: Vec<u32> = mol.connected_atoms(start.id).into_iter().filter(|a| !draw::is_folded_h(mol, *a)).collect();
        placed.extend(&members);
        let mut frag = CdxObject::new(OBJ_FRAGMENT, id());
        let mut bbox = None;
        let mut nodes: HashMap<u32, u32> = HashMap::new();
        for atom in visible.iter().filter(|a| members.contains(&a.id)) {
            let node_id = id();
            nodes.insert(atom.id, node_id);
            let at = (atom.pos[0] as f64 * SCALE + shift.0, atom.pos[1] as f64 * SCALE + shift.1);
            bbox = Some(union(bbox, [at.0, at.1, at.0, at.1]));
            let mut node = CdxObject::new(OBJ_NODE, node_id);
            node.set(PROP_2D_POSITION, point(at.0, at.1));
            let z = elements::atomic_number(&atom.element);
            match z {
                Some(6) => {}
                Some(z) => node.set(PROP_NODE_ELEMENT, (z as i16).to_le_bytes().to_vec()),
                None => node.set(PROP_NODE_TYPE, NODE_UNSPECIFIED.to_le_bytes().to_vec()),
            }
            if z.is_some() && draw::should_show_label(mol, atom.id) {
                node.set(PROP_ATOM_NUM_HYDROGENS, (draw::displayed_h_count(mol, atom.id) as i16).to_le_bytes().to_vec());
            }
            if atom.charge != 0 {
                node.set(PROP_ATOM_CHARGE, vec![atom.charge as u8]);
            }
            if draw::should_show_label(mol, atom.id) {
                let label = draw::atom_label(mol, atom);
                let (mut runs, text) = runs_of(&label.rich);
                // An element's label is a formula, written the way ChemDraw writes its own.
                if z.is_some() {
                    runs = vec![(0, FACE_FORMULA)];
                }
                let h_left = draw::displayed_h_count(mol, atom.id) > 0 && draw::h_side(mol, atom.id) == draw::HSide::Left;
                let align = if h_left { ALIGN_RIGHT } else { ALIGN_LEFT };
                node.children.push(label_object(id(), at, &runs, &text, align));
            }
            frag.children.push(node);
        }
        for bond in &mol.bonds {
            let (Some(&begin), Some(&end)) = (nodes.get(&bond.begin), nodes.get(&bond.end)) else { continue };
            let mut b = CdxObject::new(OBJ_BOND, id());
            b.set(PROP_BOND_BEGIN, begin.to_le_bytes().to_vec());
            b.set(PROP_BOND_END, end.to_le_bytes().to_vec());
            match bond.order {
                BondOrder::Single => {}
                BondOrder::Double => b.set(PROP_BOND_ORDER, 2i16.to_le_bytes().to_vec()),
                BondOrder::Triple => b.set(PROP_BOND_ORDER, 4i16.to_le_bytes().to_vec()),
            }
            let display: i16 = match bond.stereo {
                BondStereo::None => 0,
                BondStereo::Dashed => 1,
                BondStereo::WedgeDown => 3,
                BondStereo::Bold => 5,
                BondStereo::WedgeUp => 6,
                BondStereo::Wavy => 8,
            };
            if display != 0 {
                b.set(PROP_BOND_DISPLAY, display.to_le_bytes().to_vec());
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

fn bounding_box(obj: &CdxObject) -> Option<[f64; 4]> {
    let v = obj.prop(PROP_BOUNDING_BOX).filter(|v| v.len() >= 16)?;
    let n = |i: usize| i32::from_le_bytes([v[i], v[i + 1], v[i + 2], v[i + 3]]) as f64;
    Some([n(4), n(0), n(12), n(8)])
}

/// A new ChemDraw document holding `mol` (an empty page when it has no atoms).
pub fn molecule_to_cdx_document(mol: &Molecule) -> Vec<u8> {
    let mut doc = CdxObject::new(OBJ_DOCUMENT, 0);
    doc.set(PROP_CREATION_PROGRAM, cdx_string(&[], &format!("ChemBuilder {}", env!("CARGO_PKG_VERSION"))));
    for (tag, value) in DOCUMENT_STYLE {
        doc.set(tag, hex(value));
    }
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
fn read_fragment(frag: &CdxObject, mol: &mut Molecule) {
    let mut atoms: HashMap<u32, u32> = HashMap::new();
    // Atoms ChemDraw gives a hydrogen count (NH4+, H2O on their own).
    let mut hydrogens: Vec<(u32, usize)> = Vec::new();
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
        let atom = mol.add_atom(element, [x as f32, y as f32], charge);
        atoms.insert(node.id, atom);
        if node_type == NODE_ELEMENT
            && let Some(h) = node.prop_int(PROP_ATOM_NUM_HYDROGENS).filter(|&h| h > 0)
        {
            hydrogens.push((atom, h.min(8) as usize));
        }
    }
    for bond in frag.children.iter().filter(|c| c.tag == OBJ_BOND) {
        let (Some(begin), Some(end)) = (bond.prop_u32(PROP_BOND_BEGIN), bond.prop_u32(PROP_BOND_END)) else { continue };
        let (Some(&a), Some(&b)) = (atoms.get(&begin), atoms.get(&end)) else { continue };
        let order = match bond.prop_i16(PROP_BOND_ORDER) {
            Some(2) => BondOrder::Double,
            Some(4) => BondOrder::Triple,
            _ => BondOrder::Single,
        };
        // The "…End" displays have the narrow end at the bond's end: turned around.
        let (stereo, a, b) = match bond.prop_i16(PROP_BOND_DISPLAY).unwrap_or(0) {
            1 | 2 => (BondStereo::Dashed, a, b),
            3 => (BondStereo::WedgeDown, a, b),
            4 => (BondStereo::WedgeDown, b, a),
            5 => (BondStereo::Bold, a, b),
            6 => (BondStereo::WedgeUp, a, b),
            7 => (BondStereo::WedgeUp, b, a),
            8 => (BondStereo::Wavy, a, b),
            _ => (BondStereo::None, a, b),
        };
        let id = mol.add_bond(a, b, order);
        if let Some(bond) = mol.bond_by_id_mut(id) {
            bond.stereo = stereo;
        }
    }
    // An atom on its own shows no hydrogens here unless it has them: ChemDraw's NH4+ would
    // come back as N+. They are the label's (group) hydrogens, as an `n` key's NH2 has.
    for (atom, count) in hydrogens {
        if !mol.bonds_for_atom(atom).is_empty() {
            continue;
        }
        let at = mol.atom_by_id(atom).map(|a| a.pos).unwrap_or_default();
        for k in 0..count {
            let angle = k as f32 * std::f32::consts::TAU / count as f32;
            let h = mol.add_atom("H".to_string(), [at[0] + angle.cos(), at[1] + angle.sin()], 0);
            if let Some(hydrogen) = mol.atom_by_id_mut(h) {
                hydrogen.group_h = true;
            }
            mol.add_bond(atom, h, BondOrder::Single);
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
/// not CDX.
pub fn read_cdx(bytes: &[u8]) -> Option<CdxImport> {
    let doc = cdx_codec::parse(bytes).filter(|d| d.tag == OBJ_DOCUMENT)?;
    let mut molecule = Molecule::default();
    let mut other_objects = 0;
    for page in doc.children.iter().filter(|c| c.tag == OBJ_PAGE) {
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
        assert_eq!(crate::widget::draw::displayed_h_count(&mol, ammonium.id), 4);
        assert!(mol.atoms.iter().filter(|a| a.element == "H").all(|h| h.group_h && draw::is_folded_h(&mol, h.id)));
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
}
