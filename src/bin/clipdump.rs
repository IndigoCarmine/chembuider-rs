//! Clipboard inspector — a small tool to see exactly what is on the Windows clipboard.
//!
//! Usage: copy something (e.g. select a structure in ChemDraw and Ctrl+C), then run:
//!     cargo run --bin clipdump
//!
//! It lists every clipboard format (numeric id + name + byte size), writes each format's raw
//! bytes to `clipboard_dump/`, and special-cases the chemistry ones:
//!   * CDX (starts with "VjCD0100")          → saved as `structure.cdx`
//!   * OLE compound document (D0 CF 11 E0 …) → lists its streams and extracts a CONTENTS CDX
//!
//! Share the printed list and the `clipboard_dump/` files (especially the .cdx) and we can
//! verify/fix the CDX read/write against what ChemDraw actually produces.

fn main() {
    #[cfg(windows)]
    {
        if let Err(e) = imp::run() {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("clipdump is Windows-only.");
    }
}

#[cfg(windows)]
mod imp {
    use clipboard_win::{raw, Clipboard, EnumFormats};
    use std::io::Read;
    use std::path::{Path, PathBuf};

    pub fn run() -> Result<(), String> {
        let _clip = Clipboard::new_attempts(10).map_err(|e| format!("open clipboard: {e}"))?;
        let dir = Path::new("clipboard_dump");
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;

        let formats: Vec<u32> = EnumFormats::new().collect();
        println!("{} clipboard format(s) present:\n", formats.len());

        for fmt in formats {
            let name = format_name(fmt);
            // Only read HGLOBAL-backed formats: registered (>= 0xC000) or known global predefined.
            // Handle-based ones (CF_BITMAP, CF_METAFILEPICT, CF_ENHMETAFILE, …) can corrupt
            // clipboard-win's get_vec, so we skip them.
            let readable = fmt >= 0xC000 || matches!(fmt, 1 | 7 | 8 | 13 | 16 | 17);
            let mut data = Vec::new();
            if readable {
                let _ = raw::get_vec(fmt, &mut data);
            }

            println!("─ format {fmt} (0x{fmt:04X})  \"{name}\"  — {} bytes", data.len());
            if data.is_empty() {
                if fmt == 14 {
                    capture_enhmetafile(dir); // CF_ENHMETAFILE → vector image (Office's choice)
                }
                println!("   (handle-based or empty — skipped)\n");
                continue;
            }
            println!("   first bytes: {}", hex_preview(&data, 32));

            let path = dir.join(format!("{fmt:#06x}_{}.bin", sanitize(&name)));
            std::fs::write(&path, &data).map_err(|e| e.to_string())?;
            println!("   saved: {}", path.display());

            classify(&data, dir);
            println!();
        }

        println!("Done. Inspect the files in {}", dir.display());
        Ok(())
    }

    /// Recognise and unpack chemistry payloads.
    fn classify(data: &[u8], dir: &Path) {
        const CDX_MAGIC: &[u8] = b"VjCD0100";
        const CFBF_MAGIC: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

        if data.len() >= 8 && &data[0..8] == CDX_MAGIC {
            println!("   ** CDX detected → saved structure.cdx **");
            let _ = std::fs::write(dir.join("structure.cdx"), data);
        }

        if data.len() >= 8 && data[0..8] == CFBF_MAGIC {
            println!("   ** OLE compound document — streams: **");
            let Ok(mut comp) = cfb::CompoundFile::open(std::io::Cursor::new(data.to_vec())) else {
                println!("      (failed to open as compound file)");
                return;
            };
            let entries: Vec<(PathBuf, bool, u64)> = comp
                .walk()
                .map(|e| (e.path().to_path_buf(), e.is_stream(), e.len()))
                .collect();
            for (path, is_stream, len) in entries {
                let kind = if is_stream { "stream " } else { "storage" };
                println!("      {kind} {}  ({len} bytes)", path.display());
                if is_stream {
                    if let Ok(mut s) = comp.open_stream(&path) {
                        let mut buf = Vec::new();
                        if s.read_to_end(&mut buf).is_ok()
                            && buf.len() >= 8
                            && &buf[0..8] == CDX_MAGIC
                        {
                            println!("         ** holds CDX → saved contents.cdx **");
                            let _ = std::fs::write(dir.join("contents.cdx"), &buf);
                        }
                    }
                }
            }
        }
    }

    fn format_name(fmt: u32) -> String {
        let predefined = match fmt {
            1 => Some("CF_TEXT"),
            2 => Some("CF_BITMAP"),
            3 => Some("CF_METAFILEPICT"),
            8 => Some("CF_DIB"),
            13 => Some("CF_UNICODETEXT"),
            14 => Some("CF_ENHMETAFILE"),
            15 => Some("CF_HDROP"),
            17 => Some("CF_DIBV5"),
            _ => None,
        };
        if let Some(n) = predefined {
            return n.to_string();
        }
        // Only query names for registered formats; some predefined ids upset the API.
        if fmt >= 0xC000 {
            return raw::format_name_big(fmt).unwrap_or_else(|| format!("#{fmt}"));
        }
        format!("#{fmt}")
    }

    /// Save the clipboard's enhanced metafile (CF_ENHMETAFILE) as image.emf via GDI — this is
    /// the vector image PowerPoint/Word usually paste from ChemDraw.
    fn capture_enhmetafile(dir: &Path) {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetClipboardData(format: u32) -> isize;
        }
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetEnhMetaFileBits(hemf: isize, n: u32, buf: *mut u8) -> u32;
        }
        unsafe {
            let hemf = GetClipboardData(14);
            if hemf == 0 {
                return;
            }
            let size = GetEnhMetaFileBits(hemf, 0, std::ptr::null_mut());
            if size == 0 {
                return;
            }
            let mut buf = vec![0u8; size as usize];
            if GetEnhMetaFileBits(hemf, size, buf.as_mut_ptr()) == size {
                let _ = std::fs::write(dir.join("image.emf"), &buf);
                println!("   ** CF_ENHMETAFILE → saved image.emf ({size} bytes) **");
            }
        }
    }

    fn hex_preview(data: &[u8], n: usize) -> String {
        data.iter()
            .take(n)
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn sanitize(name: &str) -> String {
        name.chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect()
    }
}
