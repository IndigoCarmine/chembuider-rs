//! Requests answered without a window: a new object, one from a file, a file from an object, and
//! the picture of an object (ibisOffice cannot draw ChemDraw's own vector pictures).

use super::content;
use super::message::{EditorMsg, HostMsg};
use super::session::{Receiver, Sender};
use crate::molecule::{cdx, mol2};
use crate::Molecule;

/// The answer to one request.
pub fn answer(request: HostMsg) -> EditorMsg {
    // A request must always be answered: ibisOffice waits for it.
    std::panic::catch_unwind(|| reply(request)).unwrap_or_else(|_| EditorMsg::Error { message: "ChemBuilder failed on this object".into() })
}

fn reply(request: HostMsg) -> EditorMsg {
    let error = |message: String| EditorMsg::Error { message };
    match request {
        HostMsg::New => {
            let mol = Molecule::default();
            let (preview, size) = content::preview(&mol);
            EditorMsg::Created { content: content::new_content(cdx::molecule_to_cdx_document(&mol)), preview, size_emu: Some(size) }
        }
        HostMsg::Import { extension, data } => match extension.to_ascii_lowercase().as_str() {
            "cdx" => match cdx::read_cdx(&data) {
                // The file goes in as it is: ChemDraw's own content stays whole.
                Some(import) => {
                    let (preview, size) = content::preview(&import.molecule);
                    EditorMsg::Created { content: content::new_content(data), preview, size_emu: Some(size) }
                }
                None => error("the file is not a ChemDraw (CDX) document".into()),
            },
            other => error(format!("ChemBuilder cannot read .{other} files")),
        },
        HostMsg::Export { content, extension } => {
            let import = match content::read(&content) {
                Ok(i) => i,
                Err(e) => return error(e),
            };
            let data = match extension.to_ascii_lowercase().as_str() {
                "cdx" => content.stream(content::CONTENTS).unwrap_or_default().to_vec(),
                "cdxml" => cdx::molecule_to_cdxml(&import.molecule).into_bytes(),
                "mol2" => mol2::to_mol2_string(&import.molecule).into_bytes(),
                other => return error(format!("ChemBuilder cannot write .{other} files")),
            };
            EditorMsg::Exported { data }
        }
        HostMsg::Render { content } => match content::read(&content) {
            Ok(import) if !import.molecule.atoms.is_empty() => {
                let (preview, size) = content::preview(&import.molecule);
                EditorMsg::Rendered { preview, size_emu: Some(size) }
            }
            Ok(_) => error("the object has no structure to draw".into()),
            Err(e) => error(e),
        },
        HostMsg::Open { .. } | HostMsg::Close | HostMsg::Ping => error("not a request without a window".into()),
    }
}

/// Answer the one request on a headless connection.
pub fn serve(mut receiver: Receiver, mut sender: Sender) -> std::io::Result<()> {
    let request = receiver.recv()?;
    sender.send(&answer(request))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = include_bytes!("../../tests/fixtures/chemdraw26_benzene.cdx");

    #[test]
    fn new_objects_are_empty_chemdraw_drawings_with_a_picture() {
        let EditorMsg::Created { content, preview, size_emu } = answer(HostMsg::New) else { panic!() };
        assert!(content::read(&content).unwrap().molecule.atoms.is_empty());
        assert!(preview.png.is_some());
        assert!(size_emu.is_some());
    }

    #[test]
    fn imports_cdx_as_it_is_and_draws_it() {
        let EditorMsg::Created { content, preview, size_emu } = answer(HostMsg::Import { extension: "CDX".into(), data: SAMPLE.to_vec() }) else {
            panic!()
        };
        assert_eq!(content.stream("CONTENTS"), Some(SAMPLE));
        assert!(preview.png.unwrap().len() > 100);
        let (w, h) = size_emu.unwrap();
        assert!(w > 12_700 * 20 && h > 12_700 * 20, "a benzene ring is more than 20 pt across");
        assert!(matches!(answer(HostMsg::Import { extension: "cdx".into(), data: b"junk".to_vec() }), EditorMsg::Error { .. }));
        assert!(matches!(answer(HostMsg::Import { extension: "mol".into(), data: vec![] }), EditorMsg::Error { .. }));
    }

    #[test]
    fn renders_and_exports_an_object() {
        let content = content::new_content(SAMPLE.to_vec());
        assert!(matches!(answer(HostMsg::Render { content: content.clone() }), EditorMsg::Rendered { size_emu: Some(_), .. }));
        let EditorMsg::Exported { data } = answer(HostMsg::Export { content: content.clone(), extension: "cdxml".into() }) else { panic!() };
        assert!(String::from_utf8(data).unwrap().contains("<CDXML"));
        let EditorMsg::Exported { data } = answer(HostMsg::Export { content: content.clone(), extension: "cdx".into() }) else { panic!() };
        assert_eq!(data, SAMPLE);
        let empty = content::new_content(cdx::molecule_to_cdx_document(&Molecule::default()));
        assert!(matches!(answer(HostMsg::Render { content: empty }), EditorMsg::Error { .. }));
    }
}
