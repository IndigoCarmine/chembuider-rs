//! Convert a `Molecule` into ChemDraw CDX binary bytes, for publishing on the clipboard as
//! the "ChemDraw Interchange Format". Built on the cdx-file-rs low-level `RawCdxObject` tree +
//! `CdxWriter` (which emits the correct CDX header/framing).
//!
//! NOTE: the produced bytes have not been validated against ChemDraw in this environment —
//! coordinate scale, axis direction, and the minimal property set may need real-world tuning.

use super::{BondOrder, Molecule};
use cdx_file_rs::cdx_parse_impl::raw_nodes::RawCdxObject;
use cdx_file_rs::cdx_parse_impl::writer::CdxWriter;
use std::collections::HashMap;

// CDX object tags (kCDXObj_*).
const OBJ_DOCUMENT: u16 = 0x8000;
const OBJ_PAGE: u16 = 0x8001;
const OBJ_FRAGMENT: u16 = 0x8003;
const OBJ_NODE: u16 = 0x8004;
const OBJ_BOND: u16 = 0x8005;

// CDX property tags (kCDXProp_*).
const PROP_BOND_LENGTH: u16 = 0x0501;
const PROP_2D_POSITION: u16 = 0x0200;
const PROP_NODE_ELEMENT: u16 = 0x0402;
const PROP_BOND_ORDER: u16 = 0x0600;
const PROP_BOND_BEGIN: u16 = 0x0604;
const PROP_BOND_END: u16 = 0x0605;

/// Molecule-units → CDX coordinate units (1/65536 point); chosen so a default bond
/// (1.5 mol units) is roughly ChemDraw's standard bond length.
const SCALE: f64 = 629_145.0;

fn next_id(n: &mut u32) -> u32 {
    *n += 1;
    *n
}

/// Serialize `mol` to CDX bytes, or None if empty.
pub fn molecule_to_cdx_bytes(mol: &Molecule) -> Option<Vec<u8>> {
    if mol.atoms.is_empty() {
        return None;
    }
    let mut ids: u32 = 0;

    let mut doc = RawCdxObject::new(OBJ_DOCUMENT, next_id(&mut ids));
    doc.add_property(PROP_BOND_LENGTH, ((1.5 * SCALE) as i32).to_le_bytes().to_vec());

    let mut page = RawCdxObject::new(OBJ_PAGE, next_id(&mut ids));
    let mut frag = RawCdxObject::new(OBJ_FRAGMENT, next_id(&mut ids));

    // atom.id → CDX node id (bonds reference these)
    let mut node_ids: HashMap<u32, u32> = HashMap::new();
    for a in &mol.atoms {
        let id = next_id(&mut ids);
        node_ids.insert(a.id, id);
        let mut node = RawCdxObject::new(OBJ_NODE, id);

        // CDXPoint2D: i32 y then i32 x, little-endian.
        let x = (a.pos[0] as f64 * SCALE) as i32;
        let y = (a.pos[1] as f64 * SCALE) as i32;
        let mut pos = Vec::with_capacity(8);
        pos.extend_from_slice(&y.to_le_bytes());
        pos.extend_from_slice(&x.to_le_bytes());
        node.add_property(PROP_2D_POSITION, pos);

        // Element as atomic number; carbon is omitted (ChemDraw's default node type).
        if let Some(z) = atomic_number(&a.element) {
            if z != 6 {
                node.add_property(PROP_NODE_ELEMENT, (z as i16).to_le_bytes().to_vec());
            }
        }
        frag.children.push(node);
    }

    for b in &mol.bonds {
        let (Some(&begin), Some(&end)) = (node_ids.get(&b.begin), node_ids.get(&b.end)) else {
            continue;
        };
        let mut bond = RawCdxObject::new(OBJ_BOND, next_id(&mut ids));
        bond.add_property(PROP_BOND_BEGIN, begin.to_le_bytes().to_vec());
        bond.add_property(PROP_BOND_END, end.to_le_bytes().to_vec());
        let order: i16 = match b.order {
            BondOrder::Single => 0x0001,
            BondOrder::Double => 0x0002,
            BondOrder::Triple => 0x0004,
        };
        bond.add_property(PROP_BOND_ORDER, order.to_le_bytes().to_vec());
        frag.children.push(bond);
    }

    page.children.push(frag);
    doc.children.push(page);

    let mut writer = CdxWriter::new(std::io::Cursor::new(Vec::new()));
    writer.write(&doc).ok()?;
    Some(writer.into_inner().into_inner())
}

/// Atomic number for common element symbols (None → treated as carbon by ChemDraw).
fn atomic_number(el: &str) -> Option<u16> {
    Some(match el {
        "H" => 1, "He" => 2, "Li" => 3, "Be" => 4, "B" => 5, "C" => 6, "N" => 7, "O" => 8,
        "F" => 9, "Ne" => 10, "Na" => 11, "Mg" => 12, "Al" => 13, "Si" => 14, "P" => 15,
        "S" => 16, "Cl" => 17, "Ar" => 18, "K" => 19, "Ca" => 20, "Br" => 35, "I" => 53,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_cdx_with_magic_header() {
        let mut mol = Molecule::default();
        let a = mol.add_atom("O".to_string(), [0.0, 0.0], 0);
        let b = mol.add_atom("C".to_string(), [1.5, 0.0], 0);
        mol.add_bond(a, b, BondOrder::Double);
        let bytes = molecule_to_cdx_bytes(&mol).expect("should produce bytes");
        assert!(bytes.len() > 8, "non-trivial output");
        assert_eq!(&bytes[0..8], b"VjCD0100", "CDX files start with the magic header");
    }

    #[test]
    fn empty_molecule_yields_none() {
        assert!(molecule_to_cdx_bytes(&Molecule::default()).is_none());
    }
}
