//! Windows clipboard publishing. Like ChemDraw, a copy puts several formats on the clipboard
//! at once: Unicode text (our own JSON structure format) plus the registered
//! "ChemDraw Interchange Format" carrying CDX bytes, so the structure can be pasted into
//! ChemDraw (and our own format round-trips for in-app paste).

#![cfg(windows)]

use clipboard_win::{raw, register_format, Clipboard};

/// Standard Windows DIB clipboard format (CF_DIB).
const CF_DIB: u32 = 8;

/// Publish a copy with up to three flavors in one clipboard session (so none clobbers another):
/// Unicode `text` (our JSON), `cdx` bytes under "ChemDraw Interchange Format", and a `dib` image.
pub fn set_clipboard(text: &str, cdx: Option<&[u8]>, dib: Option<&[u8]>) -> Result<(), String> {
    let _clip = Clipboard::new_attempts(10).map_err(|e| format!("open clipboard: {e}"))?;
    raw::empty().map_err(|e| format!("empty clipboard: {e}"))?;
    raw::set_string(text).map_err(|e| format!("set text: {e}"))?;
    if let Some(bytes) = cdx {
        if let Some(format) = register_format("ChemDraw Interchange Format") {
            raw::set_without_clear(format.get(), bytes)
                .map_err(|e| format!("set CDX: {e}"))?;
        }
    }
    if let Some(bytes) = dib {
        raw::set_without_clear(CF_DIB, bytes).map_err(|e| format!("set image: {e}"))?;
    }
    Ok(())
}

/// Read CDX bytes from the clipboard's "ChemDraw Interchange Format", if present
/// (e.g. after copying a structure in ChemDraw).
pub fn read_cdx() -> Option<Vec<u8>> {
    let _clip = Clipboard::new_attempts(10).ok()?;
    let format = register_format("ChemDraw Interchange Format")?;
    let mut out = Vec::new();
    raw::get_vec(format.get(), &mut out).ok()?;
    (!out.is_empty()).then_some(out)
}
