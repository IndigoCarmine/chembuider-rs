//! Render a `Molecule` to a bitmap (Windows DIB) for the clipboard, like ChemDraw's image
//! flavor. The molecule is drawn to an SVG, rasterized with resvg, and packed as a top-down
//! 32bpp CF_DIB. Windows-only (depends on resvg).

use super::{BondOrder, Molecule};
use std::sync::{Arc, OnceLock};

/// System fonts are loaded once and reused (loading them on every copy is slow).
fn font_db() -> Arc<resvg::usvg::fontdb::Database> {
    static DB: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Render the molecule to an opaque RGBA pixmap (white background).
fn render_pixmap(mol: &Molecule) -> Option<resvg::tiny_skia::Pixmap> {
    if mol.atoms.is_empty() {
        return None;
    }
    let svg = molecule_to_svg(mol);
    let mut opt = resvg::usvg::Options::default();
    opt.fontdb = font_db();
    let tree = resvg::usvg::Tree::from_str(&svg, &opt).ok()?;

    let size = tree.size();
    let w = size.width().ceil() as u32;
    let h = size.height().ceil() as u32;
    if w == 0 || h == 0 || w > 4000 || h > 4000 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
    pixmap.fill(resvg::tiny_skia::Color::WHITE); // opaque background
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Some(pixmap)
}

/// PNG bytes of the structure (modern Office pastes the registered "PNG" format as an image).
pub fn molecule_to_png(mol: &Molecule) -> Option<Vec<u8>> {
    render_pixmap(mol)?.encode_png().ok()
}

/// CF_DIB byte blob: BITMAPINFOHEADER + bottom-up 24bpp BI_RGB pixels — the most broadly
/// accepted DIB layout. (32bpp/top-down DIBs are rejected by some apps, incl. PowerPoint, which
/// then falls back to pasting our text.)
pub fn molecule_to_dib(mol: &Molecule) -> Option<Vec<u8>> {
    let pixmap = render_pixmap(mol)?;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let rgba = pixmap.data();
    let stride = (w * 3 + 3) & !3; // each row padded to a 4-byte boundary
    let image_size = stride * h;

    let mut dib = Vec::with_capacity(40 + image_size);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(w as i32).to_le_bytes());
    dib.extend_from_slice(&(h as i32).to_le_bytes()); // positive = bottom-up
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&24u16.to_le_bytes()); // bpp
    dib.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    dib.extend_from_slice(&(image_size as u32).to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    // Bottom-up rows; RGBA → BGR; pad each row to `stride`.
    for row in (0..h).rev() {
        let start = row * w * 4;
        for px in rgba[start..start + w * 4].chunks_exact(4) {
            dib.extend_from_slice(&[px[2], px[1], px[0]]);
        }
        for _ in 0..(stride - w * 3) {
            dib.push(0);
        }
    }
    Some(dib)
}

/// Render the molecule to an Enhanced Metafile (GDI) and return the HENHMETAFILE handle, for
/// the CF_ENHMETAFILE clipboard format. This vector image is what PowerPoint/Word paste as a
/// picture AND what an embedded OLE object uses for its on-page presentation. None if empty.
/// The returned handle is owned by the caller (give it to the clipboard, which then owns it).
pub fn molecule_to_emf(mol: &Molecule) -> Option<isize> {
    #[repr(C)]
    struct Rect_ { left: i32, top: i32, right: i32, bottom: i32 }
    #[link(name = "gdi32")]
    unsafe extern "system" {
        fn CreateEnhMetaFileW(h: isize, f: *const u16, r: *const Rect_, d: *const u16) -> isize;
        fn CloseEnhMetaFile(h: isize) -> isize;
        fn MoveToEx(h: isize, x: i32, y: i32, p: *mut core::ffi::c_void) -> i32;
        fn LineTo(h: isize, x: i32, y: i32) -> i32;
        fn CreatePen(style: i32, width: i32, color: u32) -> isize;
        fn CreateSolidBrush(color: u32) -> isize;
        fn SelectObject(h: isize, o: isize) -> isize;
        fn DeleteObject(o: isize) -> i32;
        fn SetBkMode(h: isize, m: i32) -> i32;
        fn SetTextAlign(h: isize, a: u32) -> u32;
        fn SetTextColor(h: isize, c: u32) -> u32;
        fn TextOutW(h: isize, x: i32, y: i32, s: *const u16, c: i32) -> i32;
        fn Ellipse(h: isize, l: i32, t: i32, r: i32, b: i32) -> i32;
        #[allow(clippy::too_many_arguments)]
        fn CreateFontW(ch: i32, cw: i32, esc: i32, ori: i32, wt: i32, it: u32, un: u32, so: u32,
            cs: u32, op: u32, cp: u32, q: u32, pf: u32, face: *const u16) -> isize;
    }
    if mol.atoms.is_empty() {
        return None;
    }
    const PX: f32 = 40.0;
    const MARGIN: f32 = 0.7;
    const HM_PER_PX: f32 = 8.47; // HIMETRIC per drawing pixel (≈ 338.7/40)
    const BLACK: u32 = 0x0000_0000;
    const WHITE: u32 = 0x00FF_FFFF;

    let min_x = mol.atoms.iter().map(|a| a.pos[0]).fold(f32::INFINITY, f32::min);
    let max_x = mol.atoms.iter().map(|a| a.pos[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = mol.atoms.iter().map(|a| a.pos[1]).fold(f32::INFINITY, f32::min);
    let max_y = mol.atoms.iter().map(|a| a.pos[1]).fold(f32::NEG_INFINITY, f32::max);
    let w_px = (((max_x - min_x) + 2.0 * MARGIN) * PX) as i32;
    let h_px = (((max_y - min_y) + 2.0 * MARGIN) * PX) as i32;
    let tx = |x: f32| ((x - min_x + MARGIN) * PX) as i32;
    let ty = |y: f32| ((y - min_y + MARGIN) * PX) as i32;

    let frame = Rect_ {
        left: 0,
        top: 0,
        right: (w_px as f32 * HM_PER_PX) as i32,
        bottom: (h_px as f32 * HM_PER_PX) as i32,
    };

    unsafe {
        let hdc = CreateEnhMetaFileW(0, std::ptr::null(), &frame, std::ptr::null());
        if hdc == 0 {
            return None;
        }
        SetBkMode(hdc, 1); // TRANSPARENT
        let pen = CreatePen(0, 2, BLACK); // PS_SOLID, 2px
        let old_pen = SelectObject(hdc, pen);

        // Bonds.
        for b in &mol.bonds {
            let (Some(p), Some(q)) = (mol.atom_by_id(b.begin), mol.atom_by_id(b.end)) else { continue };
            let (x1, y1, x2, y2) = (tx(p.pos[0]), ty(p.pos[1]), tx(q.pos[0]), ty(q.pos[1]));
            let (dx, dy) = ((x2 - x1) as f32, (y2 - y1) as f32);
            let len = (dx * dx + dy * dy).sqrt().max(0.01);
            let (nx, ny) = ((-dy / len * 3.0) as i32, (dx / len * 3.0) as i32);
            let line = |a: i32, b: i32, c: i32, d: i32| {
                MoveToEx(hdc, a, b, std::ptr::null_mut());
                LineTo(hdc, c, d);
            };
            match b.order {
                BondOrder::Single => line(x1, y1, x2, y2),
                BondOrder::Double => {
                    line(x1 + nx, y1 + ny, x2 + nx, y2 + ny);
                    line(x1 - nx, y1 - ny, x2 - nx, y2 - ny);
                }
                BondOrder::Triple => {
                    line(x1, y1, x2, y2);
                    line(x1 + 2 * nx, y1 + 2 * ny, x2 + 2 * nx, y2 + 2 * ny);
                    line(x1 - 2 * nx, y1 - 2 * ny, x2 - 2 * nx, y2 - 2 * ny);
                }
            }
        }

        // Heteroatom labels: white disc to mask the bond, then centered black text.
        let brush = CreateSolidBrush(WHITE);
        let no_pen = CreatePen(5, 0, WHITE); // PS_NULL (no outline on the disc)
        let face: Vec<u16> = "Arial\0".encode_utf16().collect();
        let font = CreateFontW(-16, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 0, 0, face.as_ptr());
        let old_font = SelectObject(hdc, font);
        SetTextColor(hdc, BLACK);
        SetTextAlign(hdc, 6); // TA_CENTER
        for a in &mol.atoms {
            if a.element == "C" {
                continue;
            }
            let (cx, cy) = (tx(a.pos[0]), ty(a.pos[1]));
            SelectObject(hdc, brush);
            SelectObject(hdc, no_pen);
            Ellipse(hdc, cx - 10, cy - 10, cx + 10, cy + 10);
            SelectObject(hdc, pen);
            let label: Vec<u16> = a.element.encode_utf16().collect();
            TextOutW(hdc, cx, cy - 9, label.as_ptr(), label.len() as i32);
        }

        SelectObject(hdc, old_font);
        SelectObject(hdc, old_pen);
        DeleteObject(font);
        DeleteObject(brush);
        DeleteObject(no_pen);
        DeleteObject(pen);
        let hemf = CloseEnhMetaFile(hdc);
        (hemf != 0).then_some(hemf)
    }
}

/// Skeletal SVG: bonds as lines (single/double/triple), heteroatoms as labels on a white disc.
fn molecule_to_svg(mol: &Molecule) -> String {
    const PX: f32 = 40.0; // pixels per molecule unit
    const MARGIN: f32 = 0.7;

    let min_x = mol.atoms.iter().map(|a| a.pos[0]).fold(f32::INFINITY, f32::min);
    let max_x = mol.atoms.iter().map(|a| a.pos[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = mol.atoms.iter().map(|a| a.pos[1]).fold(f32::INFINITY, f32::min);
    let max_y = mol.atoms.iter().map(|a| a.pos[1]).fold(f32::NEG_INFINITY, f32::max);
    let w = ((max_x - min_x) + 2.0 * MARGIN) * PX;
    let h = ((max_y - min_y) + 2.0 * MARGIN) * PX;
    let tx = |x: f32| (x - min_x + MARGIN) * PX;
    let ty = |y: f32| (y - min_y + MARGIN) * PX;
    let line = |x1: f32, y1: f32, x2: f32, y2: f32| {
        format!(
            r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" stroke="black" stroke-width="1.6"/>"#
        )
    };

    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}">"#
    );
    for b in &mol.bonds {
        let (Some(p), Some(q)) = (mol.atom_by_id(b.begin), mol.atom_by_id(b.end)) else { continue };
        let (x1, y1, x2, y2) = (tx(p.pos[0]), ty(p.pos[1]), tx(q.pos[0]), ty(q.pos[1]));
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len = (dx * dx + dy * dy).sqrt().max(0.01);
        let (nx, ny) = (-dy / len * 3.0, dx / len * 3.0);
        match b.order {
            BondOrder::Single => s.push_str(&line(x1, y1, x2, y2)),
            BondOrder::Double => {
                s.push_str(&line(x1 + nx, y1 + ny, x2 + nx, y2 + ny));
                s.push_str(&line(x1 - nx, y1 - ny, x2 - nx, y2 - ny));
            }
            BondOrder::Triple => {
                s.push_str(&line(x1, y1, x2, y2));
                s.push_str(&line(x1 + 2.0 * nx, y1 + 2.0 * ny, x2 + 2.0 * nx, y2 + 2.0 * ny));
                s.push_str(&line(x1 - 2.0 * nx, y1 - 2.0 * ny, x2 - 2.0 * nx, y2 - 2.0 * ny));
            }
        }
    }
    for a in &mol.atoms {
        if a.element == "C" {
            continue; // skeletal: carbons are vertices
        }
        let (cx, cy) = (tx(a.pos[0]), ty(a.pos[1]));
        s.push_str(&format!(r#"<circle cx="{cx:.1}" cy="{cy:.1}" r="10" fill="white"/>"#));
        s.push_str(&format!(
            r#"<text x="{cx:.1}" y="{cy:.1}" font-size="16" text-anchor="middle" dominant-baseline="central" font-family="sans-serif" fill="black">{el}</text>"#,
            el = a.element
        ));
    }
    s.push_str("</svg>");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_draws_bonds_and_heteroatom_labels() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Double);
        let svg = molecule_to_svg(&mol);
        assert!(svg.starts_with("<svg"));
        assert!(svg.ends_with("</svg>"));
        assert_eq!(svg.matches("<line").count(), 2, "double bond renders two lines");
        assert!(svg.contains(">O</text>"), "oxygen gets a label");
        assert!(!svg.contains(">C</text>"), "carbon stays a skeletal vertex");
    }

    #[test]
    fn png_and_dib_are_well_formed() {
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Single);

        let png = molecule_to_png(&mol).expect("png");
        assert_eq!(&png[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A], "PNG signature");

        let dib = molecule_to_dib(&mol).expect("dib");
        assert_eq!(u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]), 40, "BITMAPINFOHEADER");
        let h = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
        assert!(h > 0, "bottom-up DIB (positive height)");
    }

    #[test]
    fn emf_is_a_valid_metafile() {
        #[link(name = "gdi32")]
        unsafe extern "system" {
            fn GetEnhMetaFileBits(h: isize, n: u32, b: *mut u8) -> u32;
            fn DeleteEnhMetaFile(h: isize) -> i32;
        }
        let mut mol = Molecule::default();
        let c = mol.add_atom("C".to_string(), [0.0, 0.0], 0);
        let o = mol.add_atom("O".to_string(), [1.5, 0.0], 0);
        mol.add_bond(c, o, BondOrder::Double);

        let hemf = molecule_to_emf(&mol).expect("emf handle");
        unsafe {
            let size = GetEnhMetaFileBits(hemf, 0, std::ptr::null_mut());
            assert!(size > 88, "non-trivial EMF");
            let mut buf = vec![0u8; size as usize];
            GetEnhMetaFileBits(hemf, size, buf.as_mut_ptr());
            // EMR_HEADER: iType == 1 and the " EMF" signature at offset 40.
            assert_eq!(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]), 1, "EMR_HEADER");
            assert_eq!(&buf[40..44], b" EMF", "EMF signature");
            DeleteEnhMetaFile(hemf);
        }
    }
}
