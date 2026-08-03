//! Reaction scheme + stoichiometry table, reproducing the ChemDraw *Analysis* figure:
//!
//! ```text
//! cargo run --example reaction_scheme                       # open a window
//! cargo run --example reaction_scheme -- --render out.png   # headless render to a PNG
//! ```
//!
//! The figure is built entirely from code — there is no editing UI for schemes. Everything
//! in the table is derived: `Formula` and `MW` are counted off the drawn structures, and the
//! amounts follow from the limiting reagent (phenol, 1.00 g) and the entered equivalents.
//! See [`chembuider_rs::reaction`] for the API, and `demo::williamson_ether_synthesis` for
//! the exact reaction assembled below.

use chembuider_rs::egui;
use chembuider_rs::reaction::{
    Analysis, Arrow, Mass, Product, Reactant, Reaction, ReactionScheme, ReactionView, demo,
};

/// The reference figure: PhOH + MeI / K₂CO₃ → PhOMe, 700 mg isolated (60.92 %).
fn reference_reaction() -> Reaction {
    let scheme = ReactionScheme::new()
        .structure(demo::phenol())
        .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
        .structure(demo::anisole());

    let analysis = Analysis::new()
        .reactant(
            Reactant::new(demo::phenol())
                .name("phenol")
                .limiting()
                .equivalents(1.00)
                .sample_mass(Mass::grams(1.00)),
        )
        .reactant(Reactant::new(demo::methyl_iodide()).name("MeI").equivalents(1.10))
        .reactant(
            Reactant::new(demo::potassium_carbonate())
                .name("K2CO3")
                .equivalents(3.00),
        )
        .product(
            Product::new(demo::anisole())
                .name("anisole")
                .measured_mass(Mass::milligrams(700.00)),
        );

    Reaction::new(scheme, analysis)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(i) = args.iter().position(|a| a == "--render") {
        let path = args
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| "target/reaction_scheme.png".to_string());
        headless::render(&path, &reference_reaction());
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 620.0])
            .with_title("ChemBuilder — Reaction Scheme & Analysis"),
        ..Default::default()
    };
    eframe::run_native(
        "ChemBuilder Reaction",
        options,
        Box::new(|cc| {
            // The figure is drawn on white paper; a light theme keeps the page around it light.
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            Ok(Box::new(ReactionApp::default()))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}

struct ReactionApp {
    reaction: Reaction,
    view: ReactionView,
}

impl Default for ReactionApp {
    fn default() -> Self {
        Self {
            reaction: reference_reaction(),
            // Keep the view between frames: it owns the structure drawing context.
            view: ReactionView::new(),
        }
    }
}

impl eframe::App for ReactionApp {
    // eframe 0.34+ hands us the root viewport Ui; panels are shown inside it.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                self.view.show(ui, &self.reaction);
            });
        });
    }
}

// ─── Headless rendering ──────────────────────────────────────────────────────

/// Renders the figure without a window: run the real paint path into an egui frame,
/// tessellate it, rasterize the meshes on the CPU and write a PNG. Same idea as
/// `examples/render_gallery.rs`, which writes BMPs of the editor widget.
#[cfg(not(target_arch = "wasm32"))]
mod headless {
    use super::*;
    use chembuider_rs::reaction::ReactionView;
    use std::collections::HashMap;

    type Atlas = HashMap<u64, (usize, usize, Vec<egui::Color32>)>;

    pub fn render(path: &str, reaction: &Reaction) {
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::light());
        let mut view = ReactionView::new();
        let mut atlas: Atlas = HashMap::new();

        let input = |size: egui::Vec2| egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            ..Default::default()
        };

        // Pass 1: a frame just to measure — the fonts have to exist before text can be laid out.
        let mut size = egui::vec2(1400.0, 900.0);
        let warm = ctx.run_ui(input(size), |ui| {
            size = view.measure(ui.painter(), reaction).size;
        });
        fold_textures(&warm.textures_delta, &mut atlas);

        // Pass 2: paint the figure at its own size, over the whole (unclipped) layer.
        let output = ctx.run_ui(input(size), |ui| {
            let painter = ui.ctx().layer_painter(egui::LayerId::background());
            view.paint(&painter, egui::Pos2::ZERO, reaction);
        });
        fold_textures(&output.textures_delta, &mut atlas);

        let (w, h) = (size.x.ceil() as usize, size.y.ceil() as usize);
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
        let rgb = rasterize(&primitives, &atlas, w, h);
        if let Some(dir) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(dir).ok();
        }
        write_png(path, &rgb, w, h);
        println!("wrote {path} ({w}x{h})");
    }

    /// Fold a frame's texture uploads (full images and sub-rect patches) into the atlas.
    fn fold_textures(deltas: &egui::TexturesDelta, atlas: &mut Atlas) {
        for (id, delta) in &deltas.set {
            let egui::TextureId::Managed(mid) = id else { continue };
            let egui::epaint::image::ImageData::Color(img) = &delta.image;
            match delta.pos {
                None => {
                    atlas.insert(*mid, (img.size[0], img.size[1], img.pixels.clone()));
                }
                Some([px, py]) => {
                    if let Some((w, _h, pixels)) = atlas.get_mut(mid) {
                        for y in 0..img.size[1] {
                            for x in 0..img.size[0] {
                                pixels[(py + y) * *w + px + x] = img.pixels[y * img.size[0] + x];
                            }
                        }
                    }
                }
            }
        }
    }

    /// CPU-rasterize tessellated meshes onto a white page; returns an RGB buffer.
    fn rasterize(
        primitives: &[egui::epaint::ClippedPrimitive],
        atlas: &Atlas,
        w: usize,
        h: usize,
    ) -> Vec<u8> {
        let mut canvas = vec![[255.0_f32; 4]; w * h];
        for prim in primitives {
            let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else { continue };
            let tex = match mesh.texture_id {
                egui::TextureId::Managed(mid) => atlas.get(&mid),
                _ => None,
            };
            for tri in mesh.indices.chunks_exact(3) {
                let v = [
                    &mesh.vertices[tri[0] as usize],
                    &mesh.vertices[tri[1] as usize],
                    &mesh.vertices[tri[2] as usize],
                ];
                let xs = [v[0].pos.x, v[1].pos.x, v[2].pos.x];
                let ys = [v[0].pos.y, v[1].pos.y, v[2].pos.y];
                let min_x = xs.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize;
                let max_x = (xs.iter().cloned().fold(f32::MIN, f32::max).ceil().max(0.0) as usize)
                    .min(w - 1);
                let min_y = ys.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize;
                let max_y = (ys.iter().cloned().fold(f32::MIN, f32::max).ceil().max(0.0) as usize)
                    .min(h - 1);
                let denom = (ys[1] - ys[2]) * (xs[0] - xs[2]) + (xs[2] - xs[1]) * (ys[0] - ys[2]);
                if denom.abs() < 1e-9 {
                    continue;
                }
                for py in min_y..=max_y {
                    for px in min_x..=max_x {
                        // 2x2 supersampling for smoother edges.
                        let mut acc = [0.0_f32; 4];
                        let mut hits = 0u32;
                        for (ox, oy) in [(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] {
                            let x = px as f32 + ox;
                            let y = py as f32 + oy;
                            let l0 = ((ys[1] - ys[2]) * (x - xs[2]) + (xs[2] - xs[1]) * (y - ys[2]))
                                / denom;
                            let l1 = ((ys[2] - ys[0]) * (x - xs[2]) + (xs[0] - xs[2]) * (y - ys[2]))
                                / denom;
                            let l2 = 1.0 - l0 - l1;
                            if l0 < -0.001 || l1 < -0.001 || l2 < -0.001 {
                                continue;
                            }
                            hits += 1;
                            let mut col = [0.0_f32; 4];
                            for i in 0..3 {
                                let c = v[i].color;
                                let li = [l0, l1, l2][i];
                                col[0] += c.r() as f32 * li;
                                col[1] += c.g() as f32 * li;
                                col[2] += c.b() as f32 * li;
                                col[3] += c.a() as f32 * li;
                            }
                            if let Some((tw, th, pixels)) = tex {
                                let uvx = v[0].uv.x * l0 + v[1].uv.x * l1 + v[2].uv.x * l2;
                                let uvy = v[0].uv.y * l0 + v[1].uv.y * l1 + v[2].uv.y * l2;
                                let tx = ((uvx * *tw as f32) as usize).min(tw - 1);
                                let ty = ((uvy * *th as f32) as usize).min(th - 1);
                                let t = pixels[ty * tw + tx];
                                col[0] *= t.r() as f32 / 255.0;
                                col[1] *= t.g() as f32 / 255.0;
                                col[2] *= t.b() as f32 / 255.0;
                                col[3] *= t.a() as f32 / 255.0;
                            }
                            for i in 0..4 {
                                acc[i] += col[i];
                            }
                        }
                        if hits == 0 {
                            continue;
                        }
                        let src = acc.map(|c| c / 4.0);
                        let dst = &mut canvas[py * w + px];
                        let ia = 1.0 - src[3] / 255.0;
                        for i in 0..4 {
                            dst[i] = src[i] + dst[i] * ia;
                        }
                    }
                }
            }
        }
        canvas
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]].map(|c| c.clamp(0.0, 255.0) as u8))
            .collect()
    }

    // ── Minimal dependency-free PNG writer (8-bit RGB, uncompressed deflate) ──

    fn write_png(path: &str, rgb: &[u8], w: usize, h: usize) {
        // Raw scanlines, each prefixed with filter type 0 (None).
        let mut raw = Vec::with_capacity(h * (1 + w * 3));
        for y in 0..h {
            raw.push(0);
            raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
        }
        let mut png: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&(w as u32).to_be_bytes());
        ihdr.extend_from_slice(&(h as u32).to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bits, truecolour RGB, no interlace
        chunk(&mut png, b"IHDR", &ihdr);
        chunk(&mut png, b"IDAT", &zlib_stored(&raw));
        chunk(&mut png, b"IEND", &[]);
        std::fs::write(path, png).expect("write png");
    }

    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }

    /// A zlib stream of *stored* (uncompressed) deflate blocks — no compressor needed.
    fn zlib_stored(data: &[u8]) -> Vec<u8> {
        let mut out = vec![0x78, 0x01]; // deflate, 32k window, no preset dictionary
        let blocks: Vec<&[u8]> = if data.is_empty() {
            vec![&[]]
        } else {
            data.chunks(65_535).collect()
        };
        let last = blocks.len() - 1;
        for (i, block) in blocks.iter().enumerate() {
            out.push(if i == last { 1 } else { 0 }); // BFINAL, BTYPE = 00 (stored)
            let len = block.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(block);
        }
        out.extend_from_slice(&adler32(data).to_be_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, slot) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
            *slot = c;
        }
        let mut crc = 0xFFFF_FFFF_u32;
        for &b in data {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFF_FFFF
    }

    fn adler32(data: &[u8]) -> u32 {
        let (mut a, mut b) = (1_u32, 0_u32);
        for &x in data {
            a = (a + x as u32) % 65_521;
            b = (b + a) % 65_521;
        }
        (b << 16) | a
    }
}
