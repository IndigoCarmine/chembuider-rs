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

/// Minimal embedded-object `\x01Ole` stream ([MS-OLEDS]): version 0x02000001, then zeroed
/// flags / link-update / reserved / moniker-size.
const OLE_STREAM: [u8; 20] = [
    0x01, 0x00, 0x00, 0x02, // version 0x02000001
    0, 0, 0, 0, // flags
    0, 0, 0, 0, // link update option
    0, 0, 0, 0, // reserved1
    0, 0, 0, 0, // reserved moniker stream size
];

/// Build the compound-document ("Embed Source") bytes for `mol`, or None if empty.
pub fn molecule_to_ole_embed(mol: &Molecule) -> Option<Vec<u8>> {
    let cdx = super::cdx::molecule_to_cdx_bytes(mol)?;

    let mut comp = cfb::CompoundFile::create(Cursor::new(Vec::new())).ok()?;
    comp.create_stream("/CONTENTS").ok()?.write_all(&cdx).ok()?;
    comp.create_stream("/\u{1}Ole").ok()?.write_all(&OLE_STREAM).ok()?;
    comp.flush().ok()?;

    Some(comp.into_inner().into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::BondOrder;
    use std::io::Read;

    #[test]
    fn embed_has_contents_stream_with_cdx() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Double);

        let bytes = molecule_to_ole_embed(&mol).expect("embed");
        // Re-open the compound document and read CONTENTS back.
        let mut comp = cfb::CompoundFile::open(Cursor::new(bytes)).expect("valid CFBF");
        let mut contents = Vec::new();
        comp.open_stream("/CONTENTS").unwrap().read_to_end(&mut contents).unwrap();
        assert_eq!(&contents[0..8], b"VjCD0100", "CONTENTS holds the CDX payload");
    }
}
