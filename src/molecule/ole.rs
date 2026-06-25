//! Build an OLE compound document ("Embed Source") wrapping the molecule's CDX, like the
//! `oleObject1.bin` ChemDraw embeds into Office documents. The CDX is stored in a `CONTENTS`
//! stream (the location extraction tools read), alongside a minimal `\x01Ole` stream.
//!
//! LIMITATION: full OLE *activation* (double-click in Word/PowerPoint opens ChemDraw) also
//! needs the root storage CLSID set to ChemDraw's class id — an installation-specific registry
//! value not available here — and ideally delivery via OleSetClipboard/IDataObject
//! (TYMED_ISTORAGE). This builds a structurally valid compound document with the CDX payload;
//! it has not been validated against ChemDraw/Office.

use super::Molecule;
use std::io::{Cursor, Write};

/// ChemDraw's OLE class id, in raw CFBF directory-entry byte order
/// ({41BA6D21-A02E-11CE-8FD9-0020AFD1F20C}, captured from ChemDraw's clipboard).
const CHEMDRAW_CLSID: [u8; 16] = [
    0x21, 0x6d, 0xba, 0x41, 0x2e, 0xa0, 0xce, 0x11, 0x8f, 0xd9, 0x00, 0x20, 0xaf, 0xd1, 0xf2, 0x0c,
];
const CHEMDRAW_USER_TYPE: &str = "CS ChemDraw 64-bit Drawing";

/// ChemDraw's `\x01CompObj` stream verbatim (identifies the embedded object's server: CLSID,
/// user-type "CS ChemDraw 64-bit Drawing", clipboard format, ProgID "ChemDraw_x64.Document.6.0").
const CHEMDRAW_COMPOBJ_HEX: &str = "0100feff030a0000ffffffff216dba412ea0ce118fd90020afd1f20c1b0000004353204368656d447261772036342d6269742044726177696e67001c0000004368656d4472617720496e7465726368616e676520466f726d6174001a0000004368656d447261775f7836342e446f63756d656e742e362e3000f439b271000000000000000000000000";

/// HIMETRIC (0.01 mm) units per molecule unit (CDX 629145 units/mol scaled to HIMETRIC).
const HIMETRIC_PER_MOL: f32 = 338.7;

fn hex_to_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

/// Build the compound-document ("Embed Source") bytes for `mol`, matching ChemDraw's layout:
/// root CLSID + `\x01CompObj` + `CONTENTS` (= the CDX). None if empty.
pub fn molecule_to_ole_embed(mol: &Molecule) -> Option<Vec<u8>> {
    let cdx = super::cdx::molecule_to_cdx_bytes(mol)?;

    let mut comp = cfb::CompoundFile::create(Cursor::new(Vec::new())).ok()?;
    // cfb writes the CLSID as a GUID (Data1/2/3 little-endian), so build it from the GUID
    // fields of {41BA6D21-A02E-11CE-8FD9-0020AFD1F20C} — passing raw bytes byte-swaps Data1
    // and ChemDraw can't bind the server on double-click.
    let clsid = uuid::Uuid::from_fields(
        0x41BA_6D21, 0xA02E, 0x11CE, &[0x8F, 0xD9, 0x00, 0x20, 0xAF, 0xD1, 0xF2, 0x0C],
    );
    comp.set_storage_clsid("/", clsid).ok()?;
    comp.create_stream("/\u{1}CompObj").ok()?.write_all(&hex_to_bytes(CHEMDRAW_COMPOBJ_HEX)).ok()?;
    comp.create_stream("/CONTENTS").ok()?.write_all(&cdx).ok()?;
    comp.flush().ok()?;

    Some(comp.into_inner().into_inner())
}

/// Build the OBJECTDESCRIPTOR for the "Object Descriptor" clipboard format, which is REQUIRED
/// for an app like PowerPoint to paste the structure as an editable OLE object (not text).
pub fn object_descriptor(mol: &Molecule) -> Vec<u8> {
    // Display extent (HIMETRIC) from the molecule's bounding box.
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for a in &mol.atoms {
        min_x = min_x.min(a.pos[0]);
        max_x = max_x.max(a.pos[0]);
        min_y = min_y.min(a.pos[1]);
        max_y = max_y.max(a.pos[1]);
    }
    if mol.atoms.is_empty() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 1.0, 1.0);
    }
    let cx = (((max_x - min_x) + 1.5) * HIMETRIC_PER_MOL).max(100.0) as i32;
    let cy = (((max_y - min_y) + 1.5) * HIMETRIC_PER_MOL).max(100.0) as i32;

    let utf16z = |s: &str| -> Vec<u8> {
        s.encode_utf16().chain([0]).flat_map(|u| u.to_le_bytes()).collect()
    };
    let full = utf16z(CHEMDRAW_USER_TYPE);
    let src = utf16z(CHEMDRAW_USER_TYPE);
    const FIXED: u32 = 52; // cbSize+clsid+aspect+sizel+pointl+status+offFull+offSrc
    let off_full = FIXED;
    let off_src = FIXED + full.len() as u32;
    let cb_size = FIXED as usize + full.len() + src.len();

    let mut od = Vec::with_capacity(cb_size);
    od.extend_from_slice(&(cb_size as u32).to_le_bytes());
    od.extend_from_slice(&CHEMDRAW_CLSID);
    od.extend_from_slice(&1u32.to_le_bytes()); // dwDrawAspect = DVASPECT_CONTENT
    od.extend_from_slice(&cx.to_le_bytes()); // sizel.cx
    od.extend_from_slice(&cy.to_le_bytes()); // sizel.cy
    od.extend_from_slice(&0i32.to_le_bytes()); // pointl.x
    od.extend_from_slice(&0i32.to_le_bytes()); // pointl.y
    od.extend_from_slice(&1u32.to_le_bytes()); // dwStatus = OLEMISC_RECOMPOSEONRESIZE
    od.extend_from_slice(&off_full.to_le_bytes());
    od.extend_from_slice(&off_src.to_le_bytes());
    od.extend_from_slice(&full);
    od.extend_from_slice(&src);
    od
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;
    use std::io::Read;

    #[test]
    #[ignore = "dumps ChemDraw's captured CompObj for replication"]
    fn dump_chemdraw_embed() {
        let bytes = std::fs::read("clipboard_dump/0xc00b_Embed_Source.bin").unwrap();
        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).unwrap();
        for p in comp.walk().map(|e| format!("{:?} stream={}", e.path(), e.is_stream())).collect::<Vec<_>>() {
            eprintln!("entry: {p}");
        }
        let mut compobj = Vec::new();
        comp.open_stream("/\u{1}CompObj").unwrap().read_to_end(&mut compobj).unwrap();
        eprintln!("CompObj {} bytes:", compobj.len());
        eprintln!("{}", compobj.iter().map(|b| format!("{b:02x}")).collect::<String>());
    }

    #[test]
    fn embed_matches_chemdraw_layout() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Double);

        let bytes = molecule_to_ole_embed(&mol).expect("embed");
        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).expect("valid CFBF");

        // Root storage CLSID = ChemDraw's, written in correct GUID byte order.
        let expected = uuid::Uuid::from_fields(
            0x41BA_6D21, 0xA02E, 0x11CE, &[0x8F, 0xD9, 0x00, 0x20, 0xAF, 0xD1, 0xF2, 0x0C],
        );
        assert_eq!(comp.root_entry().clsid(), &expected);

        // CONTENTS holds the CDX; \x01CompObj is present.
        let mut contents = Vec::new();
        comp.open_stream("/CONTENTS").unwrap().read_to_end(&mut contents).unwrap();
        assert_eq!(&contents[0..8], b"VjCD0100");
        let mut compobj = Vec::new();
        comp.open_stream("/\u{1}CompObj").unwrap().read_to_end(&mut compobj).unwrap();
        assert_eq!(compobj, hex_to_bytes(CHEMDRAW_COMPOBJ_HEX));
    }

    #[test]
    fn embed_clsid_bytes_are_chemdraws_raw_order() {
        let mut mol = Molecule::default();
        mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let bytes = molecule_to_ole_embed(&mol).unwrap();
        // ChemDraw's CLSID (raw CFBF byte order) must appear verbatim in the directory entry.
        let raw = CHEMDRAW_CLSID;
        let reordered = [ // what we'd get if cfb wrote a field-swapped GUID
            0x41, 0xba, 0x6d, 0x21, 0xa0, 0x2e, 0x11, 0xce,
            0x8f, 0xd9, 0x00, 0x20, 0xaf, 0xd1, 0xf2, 0x0c,
        ];
        let has_raw = bytes.windows(16).any(|w| w == raw);
        let has_reordered = bytes.windows(16).any(|w| w == reordered);
        eprintln!("raw={has_raw} reordered={has_reordered}");
        assert!(has_raw, "CLSID should be ChemDraw's raw CFBF bytes");
        assert!(!has_reordered, "no byte-swapped CLSID should remain (root storage must be correct)");
    }

    #[test]
    fn object_descriptor_is_well_formed() {
        let mut mol = Molecule::default();
        mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        mol.add_atom("O".to_string(), [3.0, 1.5], 0);
        let od = object_descriptor(&mol);
        // cbSize field matches the actual length.
        let cb = u32::from_le_bytes([od[0], od[1], od[2], od[3]]) as usize;
        assert_eq!(cb, od.len());
        // CLSID is ChemDraw's, aspect = content, non-zero extent.
        assert_eq!(&od[4..20], &CHEMDRAW_CLSID);
        assert_eq!(u32::from_le_bytes([od[20], od[21], od[22], od[23]]), 1);
        let cx = i32::from_le_bytes([od[24], od[25], od[26], od[27]]);
        let cy = i32::from_le_bytes([od[28], od[29], od[30], od[31]]);
        assert!(cx > 0 && cy > 0);
    }
}
