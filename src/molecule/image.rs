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

/// Render the molecule to a CF_DIB byte blob (BITMAPINFOHEADER + BGRA pixels), or None.
pub fn molecule_to_dib(mol: &Molecule) -> Option<Vec<u8>> {
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
    pixmap.fill(resvg::tiny_skia::Color::WHITE); // opaque background (no alpha issues in DIB)
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());

    // CF_DIB: BITMAPINFOHEADER (40 bytes) + pixels. Negative height = top-down, 32bpp BI_RGB.
    let mut dib = Vec::with_capacity(40 + (w * h * 4) as usize);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&(w as i32).to_le_bytes());
    dib.extend_from_slice(&(-(h as i32)).to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes()); // planes
    dib.extend_from_slice(&32u16.to_le_bytes()); // bpp
    dib.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    dib.extend_from_slice(&(w * h * 4).to_le_bytes()); // image size
    dib.extend_from_slice(&0i32.to_le_bytes()); // x ppm
    dib.extend_from_slice(&0i32.to_le_bytes()); // y ppm
    dib.extend_from_slice(&0u32.to_le_bytes()); // clr used
    dib.extend_from_slice(&0u32.to_le_bytes()); // clr important
    for px in pixmap.data().chunks_exact(4) {
        dib.extend_from_slice(&[px[2], px[1], px[0], px[3]]); // RGBA → BGRA
    }
    Some(dib)
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
}
