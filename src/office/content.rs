//! A ChemDraw object's content as ibisOffice hands it over: the CDX in its `CONTENTS` stream,
//! under ChemDraw's class, so Office and ChemDraw open what the editor saves.

use super::message::{Clsid, OleContent, OleStream, Preview};
use crate::molecule::cdx;
use crate::molecule::image;
use crate::molecule::ole::{CHEMDRAW_CLSID, CHEMDRAW_PROG_ID, CHEMDRAW_USER_TYPE};
use crate::Molecule;

/// The stream ChemDraw keeps its drawing in (a CDX document).
pub const CONTENTS: &str = "CONTENTS";

/// EMU (Office's unit) per point.
const EMU_PER_PT: f32 = 12_700.0;
/// No side of a preview is larger: sharp on a slide, small in the document.
const PREVIEW_MAX_SIDE: u32 = 2400;
/// The picture of an object with nothing drawn yet, in points.
const BLANK_PT: [f32; 2] = [72.0, 36.0];

/// A new ChemDraw object holding `cdx`.
pub fn new_content(cdx: Vec<u8>) -> OleContent {
    OleContent {
        clsid: Clsid(CHEMDRAW_CLSID),
        prog_id: CHEMDRAW_PROG_ID.to_string(),
        user_type: CHEMDRAW_USER_TYPE.to_string(),
        streams: vec![OleStream { path: CONTENTS.to_string(), bytes: cdx }],
    }
}

/// `original` with its drawing replaced by `cdx`: the class and every other stream as they came.
pub fn with_cdx(original: &OleContent, cdx: Vec<u8>) -> OleContent {
    let mut content = original.clone();
    content.set_stream(CONTENTS, cdx);
    content
}

/// The drawing in `content` (empty when it has no structure), or why it cannot be read.
pub fn read(content: &OleContent) -> Result<cdx::CdxImport, String> {
    let bytes = content.stream(CONTENTS).ok_or("the object has no CONTENTS stream")?;
    cdx::read_cdx(bytes).ok_or_else(|| "CONTENTS is not a ChemDraw (CDX) document".to_string())
}

/// The picture of `mol` and its size in EMU (a blank one when nothing is drawn).
pub fn preview(mol: &Molecule) -> (Preview, (i64, i64)) {
    let (png, pt) = image::preview_png(mol, PREVIEW_MAX_SIDE).unwrap_or_else(|| (image::blank_png(96, 48), BLANK_PT));
    let emu = ((pt[0] * EMU_PER_PT).round() as i64, (pt[1] * EMU_PER_PT).round() as i64);
    (Preview { png: Some(png), svg: None }, emu)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_objects_are_chemdraw_objects() {
        let content = new_content(cdx::molecule_to_cdx_document(&Molecule::default()));
        assert_eq!(content.prog_id, "ChemDraw_x64.Document.6.0");
        assert_eq!(content.clsid.0, CHEMDRAW_CLSID);
        assert!(read(&content).unwrap().molecule.atoms.is_empty());
    }

    #[test]
    fn edits_keep_the_class_and_other_streams() {
        let mut original = new_content(b"old".to_vec());
        original.prog_id = "ChemDraw.Document.6.0".into();
        original.streams.push(OleStream { path: "Other/Data".into(), bytes: vec![1, 2] });
        let edited = with_cdx(&original, b"new".to_vec());
        assert_eq!(edited.prog_id, "ChemDraw.Document.6.0");
        assert_eq!(edited.stream("Other/Data"), Some(&[1u8, 2][..]));
        assert_eq!(edited.stream(CONTENTS), Some(&b"new"[..]));
        assert!(read(&edited).is_err(), "not CDX");
    }

    #[test]
    fn previews_are_never_empty() {
        let (blank, size) = preview(&Molecule::default());
        assert!(blank.png.is_some());
        assert_eq!(size, (914_400, 457_200), "an inch by half an inch");
    }
}
