//! Display rules for a molecule drawing, independent of any UI or rendering backend: which atoms
//! get a label, what the label says (folded hydrogens, implicit-H count, charge, the side the
//! hydrogens go on), and which atoms are over-bonded. The editor, the reaction figure and the
//! file writers (CDX, images) all draw from these rules, so they live with the molecule model.
//! The widget's `draw` module re-exports them under their old paths.

use super::lookup::AtomLookup;
use super::{charged_max_valence, charged_valence};
use crate::widget::rich_text::RichText;

/// Which side of the element symbol the displayed hydrogens go on.
/// `Right` is the normal reading order ("OH", "NH2"); `Left` mirrors it ("HO", "H2N")
/// so the hydrogens point into free space when the bonds come from the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HSide {
    Right,
    Left,
}

/// Averages the unit directions from the atom toward its (non-folded) neighbors: when
/// they point right (x-sum above a small deadzone) the free side is the left, so the
/// hydrogens flip there. Near-vertical sums inside the deadzone and unbonded atoms keep
/// the default `Right`. (Vertical H placement — H above/below for straight-up bonds, as
/// ChemDraw does in crowded rings — is a possible future refinement.)
pub(crate) fn h_side(mol: &impl AtomLookup, atom_id: u32) -> HSide {
    const H_FLIP_DEADZONE: f32 = 0.1;
    let Some(atom) = mol.atom_by_id(atom_id) else { return HSide::Right };
    let mut sum_x = 0.0_f32;
    for nid in mol.neighbor_atom_ids(atom_id) {
        if folded_h(mol, nid) {
            continue;
        }
        let Some(n) = mol.atom_by_id(nid) else { continue };
        let dx = n.pos[0] - atom.pos[0];
        let dy = n.pos[1] - atom.pos[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1e-6 {
            sum_x += dx / len;
        }
    }
    if sum_x > H_FLIP_DEADZONE { HSide::Left } else { HSide::Right }
}

/// A styled atom label plus the char range (over the flattened text) of the element
/// symbol — the part that gets centered on the atom position.
pub struct AtomLabel {
    pub rich: RichText,
    pub anchor_chars: std::ops::Range<usize>,
}

/// Length in chars of the leading element token of a parsed label's flat text: the first
/// `[A-Z][a-z]*` run ("CO2Et" → "C", "Cl" → "Cl", "OTf" → "O"), falling back to a single
/// char for labels that start otherwise (e.g. italic "tBu").
fn element_token_len(flat: &str) -> usize {
    let mut chars = flat.chars();
    let Some(first) = chars.next() else { return 0 };
    if !first.is_ascii_uppercase() {
        return 1;
    }
    1 + chars.take_while(|c| c.is_ascii_lowercase()).count()
}

/// Build the styled atom label: hydrogen count as a true subscript (CH₃, NH₂), the charge
/// as a trailing superscript (N⁺, SO₄²⁻), and the hydrogens flipped to the atom's free
/// side ("HO", "H₂N" when the bonds come from the right). The element text itself goes
/// through the rich-text markup parser, so custom labels typed into the editor can use
/// `_`/`^` scripts, `**bold**` and `//italic//` (e.g. `CO_2Et`, `//t//Bu`).
pub(crate) fn atom_label(
    mol: &impl AtomLookup,
    atom: &crate::molecule::Atom,
) -> AtomLabel {
    let element = RichText::parse(&atom.element);
    let element_flat: String = element.spans.iter().map(|s| s.text.as_str()).collect();
    let h = displayed_h_count(mol, atom.id);
    let side = if h >= 1 { h_side(mol, atom.id) } else { HSide::Right };

    let mut rich = RichText::new();
    let mut anchor_start = 0usize;
    if side == HSide::Left {
        // Hydrogens first: "H", then the count subscript ("H₂N").
        rich = rich.text("H");
        anchor_start = 1;
        if h > 1 {
            let count = h.to_string();
            anchor_start += count.chars().count();
            rich = rich.subscript(count);
        }
    }
    for span in element.spans {
        rich = rich.span(span.text, span.style);
    }
    if side == HSide::Right && h >= 1 {
        rich = rich.text("H");
        if h > 1 {
            rich = rich.subscript(h.to_string());
        }
    }
    // The charge stays a trailing superscript in both orders (top-right, ChemDraw-style;
    // the strict-ChemDraw leading variant "⁺H₃N" is a possible future refinement).
    match atom.charge {
        0 => {}
        1 => rich = rich.superscript("+"),
        -1 => rich = rich.superscript("-"),
        c if c > 0 => rich = rich.superscript(format!("{c}+")),
        c => rich = rich.superscript(format!("{}-", -c)),
    }

    // Anchor on the element token; degenerate labels fall back to the first char.
    let token_len = element_token_len(&element_flat);
    let (start, len) = if token_len == 0 { (0, 1) } else { (anchor_start, token_len) };
    AtomLabel { rich, anchor_chars: start..start + len }
}

/// A hydrogen that is drawn inside its heavy neighbour's label instead of on the canvas,
/// so it gets no vertex of its own and its bond is suppressed (N–H renders as "NH2").
///
/// Three conditions, and the last two are what keep the folding honest:
///
/// - Exactly one bond, to a heavy (non-H) atom. Deuterium ("D") is a normal atom, never folded.
/// - It came in with a group ([`Atom::group_h`](crate::molecule::Atom::group_h)) — `n` inserts
///   NH₂, whose hydrogen is really just the "2" in the label. A hydrogen the user asked for by
///   name is an atom like any other; folding it made it vanish outright.
/// - Its neighbour's label still has room for it. [`displayed_h_count`] derives the label's
///   hydrogens from the *heavy* bond sum, so once those fill the valence the label shows no
///   hydrogen at all and there is nothing left to fold into. An invisible hydrogen was the
///   sole cause of the over-valence ring on both nitrogens of a Ph–N=N–Ph drawn from two `n`
///   keys: a warning the user could neither see the reason for nor delete.
pub fn is_folded_h(mol: &crate::molecule::Molecule, atom_id: u32) -> bool {
    folded_h(mol, atom_id)
}

/// [`is_folded_h`] over any [`AtomLookup`], so a per-atom pass can answer it from an index.
pub(crate) fn folded_h(mol: &impl AtomLookup, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    if atom.element != "H" || !atom.group_h { return false; }
    let bonds = mol.bonds_for_atom(atom_id);
    if bonds.len() != 1 { return false; }
    let b = bonds[0];
    let other = if b.begin == atom_id { b.end } else { b.begin };
    let Some(heavy) = mol.atom_by_id(other) else { return false };
    if heavy.element == "H" { return false; }
    // The same valence `displayed_h_count` fills, so "room" means room in the label.
    let Some(valence) = charged_valence(&heavy.element, heavy.charge) else { return false };

    // Room left on the neighbour: its valence, minus every bond that is not a group hydrogen,
    // minus the group hydrogens folded ahead of this one. Ordering those by id makes the
    // choice stable when a group brings more hydrogens than the atom can show (SiH₃ on a
    // silicon that has since grown a second bond).
    let used: i16 = mol.bonds_for_atom(other).iter().map(|nb| {
        let n = if nb.begin == other { nb.end } else { nb.begin };
        match mol.atom_by_id(n) {
            Some(a) if a.element == "H" && a.group_h => (n < atom_id) as i16,
            _ => nb.order.valence(),
        }
    }).sum();
    used < valence
}

pub(crate) fn should_show_label(mol: &impl AtomLookup, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    // Folded hydrogens are merged into their heavy neighbor's label.
    if folded_h(mol, atom_id) { return false; }
    if atom.element == "C" {
        // Carbon: only show when completely isolated (no bonds) so the atom stays visible
        return atom.charge != 0 || mol.bonds_for_atom(atom_id).is_empty();
    }
    true
}

/// True when an atom is over-bonded: its total bond order exceeds what its element/charge
/// allows (e.g. a carbon with more than 4 bond-orders, an oxygen with more than 2). Such
/// atoms are marked with a red dashed warning ring. Under-bonded atoms are valid — the
/// missing valence is shown as implicit H — and elements with no standard valence aren't checked.
///
/// The warning threshold is the element's *highest* common valence, moved by its charge the
/// same way the label's hydrogens are ([`charged_max_valence`]), so hypervalent-but-ordinary
/// S and P (sulfones, sulfates, phosphates) are not flagged, a borate's four bonds are not
/// either, and a carbocation's fourth bond is; implicit-H counting keeps using the lowest
/// ([`charged_valence`]).
pub fn is_valence_invalid(mol: &crate::molecule::Molecule, atom_id: u32) -> bool {
    let Some(atom) = mol.atom_by_id(atom_id) else { return false };
    let Some(limit) = charged_max_valence(&atom.element, atom.charge) else { return false };
    mol.bond_order_sum(atom_id) > limit
}

/// Total hydrogens to display on a heavy atom = explicit folded-H neighbors +
/// implicit H needed to satisfy valence. Computed as the [`charged_valence`] minus the
/// bond-order sum to *non-folded* neighbors, which naturally includes both.
pub(crate) fn displayed_h_count(mol: &impl AtomLookup, atom_id: u32) -> u8 {
    let Some(atom) = mol.atom_by_id(atom_id) else { return 0 };
    let Some(effective) = charged_valence(&atom.element, atom.charge) else { return 0 };
    // A lone atom (no bonds yet) shows just its symbol — placing "O"/"N"/"S" at the cursor
    // should read as O/N/S, not OH2/NH3/SH2. Implicit H appears once it gets a bond.
    if mol.bonds_for_atom(atom_id).is_empty() {
        return 0;
    }
    let bond_sum_heavy: i16 = mol.bonds_for_atom(atom_id).iter()
        .filter(|b| {
            let other = if b.begin == atom_id { b.end } else { b.begin };
            !folded_h(mol, other)
        })
        .map(|b| b.order.valence())
        .sum();
    (effective - bond_sum_heavy).max(0) as u8
}

