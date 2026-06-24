//! Windows clipboard publishing. Like ChemDraw, a copy puts several formats on the clipboard
//! at once: Unicode text (our own JSON structure format) plus the registered
//! "ChemDraw Interchange Format" carrying CDX bytes, so the structure can be pasted into
//! ChemDraw (and our own format round-trips for in-app paste).

#![cfg(windows)]

use clipboard_win::{raw, register_format, Clipboard};

/// Publish `text` (Unicode) and, when present, `cdx` bytes under the ChemDraw clipboard format.
/// Both formats share a single clipboard session so neither clobbers the other.
pub fn set_text_and_cdx(text: &str, cdx: Option<&[u8]>) -> Result<(), String> {
    let _clip = Clipboard::new_attempts(10).map_err(|e| format!("open clipboard: {e}"))?;
    raw::empty().map_err(|e| format!("empty clipboard: {e}"))?;
    raw::set_string(text).map_err(|e| format!("set text: {e}"))?;
    if let Some(bytes) = cdx {
        if let Some(format) = register_format("ChemDraw Interchange Format") {
            raw::set_without_clear(format.get(), bytes)
                .map_err(|e| format!("set CDX: {e}"))?;
        }
    }
    Ok(())
}
