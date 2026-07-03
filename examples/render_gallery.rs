//! Headless rendering gallery: draws every fixture in `tests/fixtures/rendering/` with
//! the real editor widget (no window needed) and writes BMP images for visual review.
//!
//! Each structure is rendered twice — raw fixture coordinates and after `cleanup_2d` —
//! side by side, plus everything combined into one contact sheet:
//!
//! ```text
//! cargo run --example render_gallery                 # pairs (raw | cleanup), per-cell BMPs
//! cargo run --example render_gallery -- --single     # one cleaned cell per fixture (compact)
//! cargo run --example render_gallery -- out_dir      # custom output directory
//! ```
//!
//! Rendering goes through the full production path (`ChemStructEditor::ui` in an
//! `egui::Context::run` frame), then the emitted meshes are rasterized on the CPU:
//! barycentric triangle fill with 2×2 supersampling and font-atlas texture sampling.
//! Use this to eyeball label anchoring, H-side flips, bond trimming and overlaps
//! whenever the drawing code changes.

use chembuider_rs::{ChemStructEditor, egui};
use std::collections::HashMap;

const CELL_W: usize = 380;
const CELL_H: usize = 280;

fn main() {
    // Args: `--single` picks the compact one-cell-per-fixture mode; the first
    // non-flag argument is the output directory.
    let args: Vec<String> = std::env::args().skip(1).collect();
    let single = args.iter().any(|a| a == "--single");
    let out_dir = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .cloned()
        .unwrap_or_else(|| "target/render_gallery".to_string());
    std::fs::create_dir_all(&out_dir).unwrap();

    let fixture_dir = std::path::Path::new("tests/fixtures/rendering");
    let mut names: Vec<String> = std::fs::read_dir(fixture_dir)
        .expect("run from the repo root; fixtures come from scripts/gen_test_structures.py")
        .filter_map(|e| {
            let p = e.ok()?.path();
            (p.extension()? == "json").then(|| p.file_stem().unwrap().to_string_lossy().into_owned())
        })
        .collect();
    names.sort();

    // Per fixture, which variants to render: single = cleanup only; default = both.
    let variants: &[bool] = if single { &[true] } else { &[false, true] };

    let mut renderer = CellRenderer::new();
    let mut cells: Vec<(String, Vec<u8>)> = Vec::new(); // (label, RGB cell buffer)
    for name in &names {
        let json = std::fs::read_to_string(fixture_dir.join(format!("{name}.json"))).unwrap();
        for &cleaned in variants {
            let mut editor = ChemStructEditor::default();
            assert!(
                editor.paste_from_string(&format!("chembuilder-mol:{json}")),
                "fixture {name} failed to load"
            );
            editor.selected_atoms.clear(); // paste selects; no selection rings in renders
            if cleaned {
                chembuider_rs::molecule::cleanup::cleanup_2d(&mut editor.molecule);
            }
            fit_view(&mut editor);
            let label = if single {
                name.clone()
            } else {
                format!("{name}{}", if cleaned { " (cleanup)" } else { "" })
            };
            let cell = renderer.render(&mut editor, &label);
            if !single {
                write_bmp(
                    &format!("{out_dir}/{name}{}.bmp", if cleaned { "_cleanup" } else { "" }),
                    &cell,
                    CELL_W,
                    CELL_H,
                );
            }
            cells.push((label, cell));
        }
    }

    // Contact sheet: keep raw|cleanup pairs adjacent in default mode (even columns),
    // widen to 5 columns in the compact single mode.
    let cols = if single { 5 } else { 4 };
    let rows = cells.len().div_ceil(cols);
    let (sheet_w, sheet_h) = (cols * CELL_W, rows * CELL_H);
    let mut sheet = vec![220u8; sheet_w * sheet_h * 3];
    for (i, (_, cell)) in cells.iter().enumerate() {
        let (cx, cy) = ((i % cols) * CELL_W, (i / cols) * CELL_H);
        for y in 0..CELL_H {
            let src = y * CELL_W * 3;
            let dst = ((cy + y) * sheet_w + cx) * 3;
            sheet[dst..dst + CELL_W * 3].copy_from_slice(&cell[src..src + CELL_W * 3]);
        }
    }
    let sheet_path = format!("{out_dir}/contact_sheet.bmp");
    write_bmp(&sheet_path, &sheet, sheet_w, sheet_h);
    println!("wrote {} cells + {sheet_path}", cells.len());
}

/// Zoom/pan the editor so the molecule fills the cell with a comfortable margin.
/// Label extents aren't known before a frame runs, so leave generous headroom here;
/// `refine_fit` tightens it from the actual label rects after a warm-up frame.
fn fit_view(editor: &mut ChemStructEditor) {
    use chembuider_rs::widget::SCALE_FACTOR;
    let xs: Vec<f32> = editor.molecule.atoms.iter().map(|a| a.pos[0]).collect();
    let ys: Vec<f32> = editor.molecule.atoms.iter().map(|a| a.pos[1]).collect();
    if xs.is_empty() {
        return;
    }
    let (min_x, max_x) = (xs.iter().cloned().fold(f32::MAX, f32::min), xs.iter().cloned().fold(f32::MIN, f32::max));
    let (min_y, max_y) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
    let w = (max_x - min_x).max(1.0) * SCALE_FACTOR;
    let h = (max_y - min_y).max(1.0) * SCALE_FACTOR;
    let zoom = ((CELL_W as f32 - 120.0) / w).min((CELL_H as f32 - 100.0) / h);
    editor.zoom = zoom.clamp(0.2, 1.3);
    let cx = (min_x + max_x) * 0.5;
    let cy = (min_y + max_y) * 0.5;
    editor.pan = egui::vec2(
        -cx * editor.zoom * SCALE_FACTOR,
        -cy * editor.zoom * SCALE_FACTOR,
    );
}

/// After a warm-up frame the editor knows every label's screen rect; shrink the zoom and
/// recenter so the union of atoms + labels sits inside the cell with a fixed margin.
fn refine_fit(editor: &mut ChemStructEditor, canvas: egui::Rect) {
    const MARGIN: f32 = 14.0;
    let mut bounds: Option<egui::Rect> = None;
    let center = canvas.center();
    for atom in &editor.molecule.atoms {
        let sp = editor.mol_to_screen(atom.pos, center);
        let r = egui::Rect::from_center_size(sp, egui::vec2(8.0, 8.0));
        bounds = Some(bounds.map_or(r, |b| b.union(r)));
    }
    for rect in editor.label_rects().values() {
        bounds = Some(bounds.map_or(*rect, |b| b.union(*rect)));
    }
    let Some(bounds) = bounds else { return };
    let avail = canvas.shrink(MARGIN);
    let scale = (avail.width() / bounds.width())
        .min(avail.height() / bounds.height())
        .min(1.0); // only shrink, never blow tiny structures up further
    // Screen-space transform is affine in (zoom, pan): scaling zoom by `s` around the
    // canvas center and fixing pan keeps molecule points at
    // p' = center + (p - center - pan)*s + pan... simpler to recompute pan directly.
    editor.zoom *= scale;
    // Recenter: the bounds center corresponds to a molecule-space point; after the zoom
    // change, choose pan so that point lands on the canvas center.
    let bc = bounds.center();
    // Point in mol coords that was at bc (using the OLD zoom / pan via inverse transform
    // captured before mutation would be cleaner; recompute from atoms instead):
    // just recompute the label-aware bounds center in mol coords approximately from the
    // pre-scale transform: mol = (bc - center - pan_old) / (zoom_old * SCALE)
    // Since we already overwrote zoom, reconstruct zoom_old:
    let zoom_old = editor.zoom / scale;
    use chembuider_rs::widget::SCALE_FACTOR;
    let mol_cx = (bc.x - center.x - editor.pan.x) / (zoom_old * SCALE_FACTOR);
    let mol_cy = (bc.y - center.y - editor.pan.y) / (zoom_old * SCALE_FACTOR);
    editor.pan = egui::vec2(
        -mol_cx * editor.zoom * SCALE_FACTOR,
        -mol_cy * editor.zoom * SCALE_FACTOR,
    );
}

/// Renders one editor frame into an RGB cell buffer. Owns a persistent egui context so
/// the font atlas builds once and texture deltas accumulate across cells.
struct CellRenderer {
    ctx: egui::Context,
    textures: HashMap<u64, (usize, usize, Vec<egui::Color32>)>,
}

impl CellRenderer {
    fn new() -> Self {
        Self { ctx: egui::Context::default(), textures: HashMap::new() }
    }

    fn render(&mut self, editor: &mut ChemStructEditor, caption: &str) -> Vec<u8> {
        let cell = egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(CELL_W as f32, CELL_H as f32),
        );
        let raw = egui::RawInput { screen_rect: Some(cell), ..Default::default() };
        let frame = |editor: &mut ChemStructEditor, ui: &mut egui::Ui| {
            editor.ui(ui);
            // Caption + cell border on top of the editor's white canvas.
            let painter = ui.painter();
            painter.text(
                cell.min + egui::vec2(6.0, 4.0),
                egui::Align2::LEFT_TOP,
                caption,
                egui::FontId::proportional(13.0),
                egui::Color32::from_gray(90),
            );
            painter.rect_stroke(
                cell.shrink(0.5),
                0.0,
                egui::Stroke::new(1.0, egui::Color32::from_gray(180)),
                egui::StrokeKind::Inside,
            );
        };
        // Warm-up frame: teaches the editor its label rects, then tighten the fit so
        // wide edge labels (H2N, OH...) don't spill out of the cell.
        let warm = self.ctx.run_ui(raw.clone(), |ui| frame(editor, ui));
        self.fold_textures(&warm.textures_delta);
        refine_fit(editor, cell);
        let output = self.ctx.run_ui(raw, |ui| frame(editor, ui));
        self.fold_textures(&output.textures_delta);

        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
        rasterize(&primitives, &self.textures, CELL_W, CELL_H)
    }

    /// Fold a frame's texture uploads (full images and sub-rect patches) into the
    /// persistent atlas copies.
    fn fold_textures(&mut self, deltas: &egui::TexturesDelta) {
        for (id, delta) in &deltas.set {
            let egui::TextureId::Managed(mid) = id else { continue };
            let egui::epaint::image::ImageData::Color(img) = &delta.image;
            match delta.pos {
                None => {
                    self.textures
                        .insert(*mid, (img.size[0], img.size[1], img.pixels.clone()));
                }
                Some([px, py]) => {
                    if let Some((w, _h, pixels)) = self.textures.get_mut(mid) {
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
}

/// CPU-rasterize tessellated meshes onto a white background; returns an RGB buffer.
fn rasterize(
    primitives: &[egui::epaint::ClippedPrimitive],
    textures: &HashMap<u64, (usize, usize, Vec<egui::Color32>)>,
    w: usize,
    h: usize,
) -> Vec<u8> {
    // Premultiplied RGBA accumulation canvas, white background.
    let mut canvas = vec![[255.0_f32; 4]; w * h];

    for prim in primitives {
        let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive else { continue };
        let tex = match mesh.texture_id {
            egui::TextureId::Managed(mid) => textures.get(&mid),
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
            let max_x = (xs.iter().cloned().fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(w - 1);
            let min_y = ys.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize;
            let max_y = (ys.iter().cloned().fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(h - 1);
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
                        let l0 = ((ys[1] - ys[2]) * (x - xs[2]) + (xs[2] - xs[1]) * (y - ys[2])) / denom;
                        let l1 = ((ys[2] - ys[0]) * (x - xs[2]) + (xs[0] - xs[2]) * (y - ys[2])) / denom;
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

/// Minimal dependency-free 24-bit BMP writer (BGR, bottom-up rows).
fn write_bmp(path: &str, rgb: &[u8], w: usize, h: usize) {
    let row_pad = (4 - (w * 3) % 4) % 4;
    let img_size = (w * 3 + row_pad) * h;
    let mut bmp: Vec<u8> = Vec::with_capacity(54 + img_size);
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&(54u32 + img_size as u32).to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&(w as i32).to_le_bytes());
    bmp.extend_from_slice(&(h as i32).to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&24u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let i = (y * w + x) * 3;
            bmp.extend_from_slice(&[rgb[i + 2], rgb[i + 1], rgb[i]]);
        }
        bmp.extend(std::iter::repeat_n(0u8, row_pad));
    }
    std::fs::write(path, bmp).unwrap();
}
