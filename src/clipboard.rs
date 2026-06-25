//! Windows clipboard publishing. Like ChemDraw, a copy puts several formats on the clipboard
//! at once: Unicode text (our own JSON structure format) plus the registered
//! "ChemDraw Interchange Format" carrying CDX bytes, so the structure can be pasted into
//! ChemDraw (and our own format round-trips for in-app paste).

#![cfg(windows)]

use clipboard_win::{raw, register_format, Clipboard};

/// Standard Windows clipboard format ids.
const CF_DIB: u32 = 8;
const CF_UNICODETEXT: u32 = 13;
const CF_ENHMETAFILE: u32 = 14;

#[link(name = "user32")]
unsafe extern "system" {
    fn SetClipboardData(format: u32, hmem: isize) -> isize;
}

/// Publish a copy with several flavors in one clipboard session (none clobbers another).
/// ORDER MATTERS: apps pick by clipboard-enumeration order, so the rich flavors go first and
/// plain `text` LAST — otherwise PowerPoint/Word paste our JSON text instead of the object/image.
/// For OLE embedding, "Object Descriptor" + "Embed Source" must both be present.
pub fn set_clipboard(
    text: &str,
    emf: Option<isize>,
    png: Option<&[u8]>,
    dib: Option<&[u8]>,
    cdx: Option<&[u8]>,
    embed: Option<&[u8]>,
    object_descriptor: Option<&[u8]>,
) -> Result<(), String> {
    let _clip = Clipboard::new_attempts(10).map_err(|e| format!("open clipboard: {e}"))?;
    raw::empty().map_err(|e| format!("empty clipboard: {e}"))?;

    let set_named = |name: &str, bytes: &[u8]| -> Result<(), String> {
        if let Some(format) = register_format(name) {
            raw::set_without_clear(format.get(), bytes).map_err(|e| format!("set {name}: {e}"))?;
        }
        Ok(())
    };

    // 1) OLE object FIRST so a plain paste embeds an editable ChemDraw object: Object Descriptor
    //    + Embed Source storage, plus CF_ENHMETAFILE as the object's on-page presentation.
    if let (Some(od), Some(em)) = (object_descriptor, embed) {
        set_named("Object Descriptor", od)?;
        set_named("Embed Source", em)?;
    }
    // 2) Enhanced metafile (vector picture / OLE presentation). The clipboard takes ownership
    //    of the HENHMETAFILE handle.
    if let Some(hemf) = emf {
        if unsafe { SetClipboardData(CF_ENHMETAFILE, hemf) } == 0 {
            return Err("set CF_ENHMETAFILE failed".to_string());
        }
    }
    // 3) Raster images.
    if let Some(bytes) = png {
        set_named("PNG", bytes)?;
    }
    if let Some(bytes) = dib {
        raw::set_without_clear(CF_DIB, bytes).map_err(|e| format!("set image: {e}"))?;
    }
    // 4) ChemDraw native CDX.
    if let Some(bytes) = cdx {
        set_named("ChemDraw Interchange Format", bytes)?;
    }
    // 5) Our own structure JSON under a PRIVATE format name — deliberately NOT CF_UNICODETEXT.
    //    If any "text" format is on the clipboard, PowerPoint/Word default-paste it instead of
    //    the picture. With no text format, a plain paste becomes the picture; we still read this
    //    private format back for full-fidelity in-app paste.
    set_named(APP_FORMAT, text.as_bytes())?;
    let _ = CF_UNICODETEXT;
    Ok(())
}

/// Private clipboard format carrying our own JSON structure (in-app copy/paste fidelity).
pub const APP_FORMAT: &str = "ChemBuilder Molecule";

/// Read CDX bytes from the clipboard's "ChemDraw Interchange Format", if present
/// (e.g. after copying a structure in ChemDraw).
pub fn read_cdx() -> Option<Vec<u8>> {
    let _clip = Clipboard::new_attempts(10).ok()?;
    let format = register_format("ChemDraw Interchange Format")?;
    let mut out = Vec::new();
    raw::get_vec(format.get(), &mut out).ok()?;
    (!out.is_empty()).then_some(out)
}

/// Read Unicode text (CF_UNICODETEXT) from the clipboard, if any.
pub fn read_text() -> Option<String> {
    clipboard_win::get_clipboard_string().ok()
}

/// Read our private "ChemBuilder Molecule" JSON, if present (for in-app paste).
pub fn read_app_format() -> Option<String> {
    let _clip = Clipboard::new_attempts(10).ok()?;
    let format = register_format(APP_FORMAT)?;
    let mut out = Vec::new();
    raw::get_vec(format.get(), &mut out).ok()?;
    String::from_utf8(out).ok().filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    /// Diagnostic: does set_clipboard actually publish all formats? (overwrites the clipboard)
    ///   cargo test set_then_read_clipboard -- --ignored --nocapture
    #[test]
    #[ignore = "overwrites the clipboard"]
    fn set_then_read_clipboard() {
        let cdx = b"VjCD0100example-cdx".to_vec();
        let r = super::set_clipboard(
            "chembuilder-mol:{}",
            None,
            Some(b"pngdata"),
            Some(b"dibdata"),
            Some(&cdx),
            Some(b"embeddata"),
            Some(b"oddata"),
        );
        eprintln!("set_clipboard -> {r:?}");
        eprintln!("read_cdx -> {:?} bytes", super::read_cdx().map(|b| b.len()));
        eprintln!("read_app_format -> {:?}", super::read_app_format());
        eprintln!("read_text -> {:?}", super::read_text());
    }

    /// Diagnostic: read the LIVE clipboard. Copy in ChemDraw first, then run:
    ///   cargo test read_live_clipboard -- --ignored --nocapture
    #[test]
    #[ignore = "reads the live clipboard"]
    fn read_live_clipboard() {
        match super::read_cdx() {
            Some(b) => eprintln!("read_cdx: {} bytes, magic={:?}", b.len(), b.get(0..8)),
            None => eprintln!("read_cdx: None"),
        }
        match super::read_text() {
            Some(t) => eprintln!("read_text: {} chars: {:?}", t.len(), &t[..t.len().min(60)]),
            None => eprintln!("read_text: None"),
        }
    }
}
