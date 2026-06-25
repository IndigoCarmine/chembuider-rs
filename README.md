# chembuider-rs

*[日本語版 README →](README.ja.md)*

An [egui](https://github.com/emilk/egui) widget library for drawing and editing **2D chemical
structure diagrams** in Rust — skeletal formulas, stereo bonds, ring & functional-group
templates, automatic 2D clean-up, MOL2 export, and (on Windows) ChemDraw-compatible
clipboard interop.

The core type is [`ChemStructEditor`](src/widget/mod.rs): an immediate-mode widget you hold in
your app and render each frame with `editor.ui(ui)`. Everything app-specific (toolbar, menus,
file dialogs) lives in your host application — see [`examples/editor.rs`](examples/editor.rs)
for a complete one.

## Features

- **Drawing tools** — Select, Bond, and Eraser, with hover highlighting and snapping.
- **Atoms & elements** — single-key placement of heteroatoms and charges; carbons are drawn
  implicitly with implicit hydrogens (CH₃, OH, NH₂ …) and subscripts.
- **Bonds** — single / double / triple (cycle by clicking), plus wedge, hash, bold, dashed,
  and wavy stereo bonds.
- **Templates** — rings (3–10 membered), zig-zag chains, benzene, and a library of built-in
  functional-group **fragments** (Boc, Cbz, CF₃, CO₂Me, TMS, BPin, …) you can extend.
- **Edit gestures** — lasso / double-click selection, drag to move, `Alt`-drag to rotate,
  drag-onto-atom to merge, `Delete` to remove.
- **2D clean-up** — force-directed layout relaxation that runs on a background thread (never
  freezes the UI) and can be cancelled.
- **Valence hints** — over-bonded atoms are flagged with a red dashed circle.
- **Export** — Tripos **MOL2** via `molecule::mol2::to_mol2_string`.
- **Clipboard (Windows)** — copy/paste structures as ChemDraw-compatible **CDX**, a
  transparent **image**, and an **OLE** object, so a paste into PowerPoint embeds an editable
  ChemDraw drawing.

## Quick start

Try the bundled editor application:

```sh
cargo run --example editor
```

## Using the widget in your own app

Add the crate and a matching egui/eframe:

```toml
[dependencies]
chembuider-rs = { git = "https://github.com/IndigoCarmine/chembuider-rs" }
eframe = "0.34"
```

The widget depends only on `egui` (re-exported as `chembuider_rs::egui` so you always get a
version-matched API). Drop it into any `egui::Ui`:

```rust
use chembuider_rs::{egui, ChemStructEditor};

#[derive(Default)]
struct MyApp {
    editor: ChemStructEditor,
}

impl eframe::App for MyApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Draw the editor and process this frame's input.
        // Returns `true` on frames where the molecule was modified.
        let _changed = self.editor.ui(ui);
    }
}
```

`ChemStructEditor::default()` performs **no file I/O** — it ships with built-in shortcuts and
fragments (`Config::embedded()`). To load user shortcuts/fragments from disk instead, assign
`editor.config = Config::load();` (this is what the example app does).

## Controls

### Tools

| Tool | Actions |
| --- | --- |
| **Select** | Click / lasso-drag to select · double-click for the whole molecule · drag to move · `Alt`-drag to rotate · `Delete` to remove |
| **Bond** | Click an atom to extend a chain · drag atom→atom to draw a bond · click a bond to cycle single/double/triple |
| **Eraser** | Click an atom or bond to remove it |

### Keyboard

While hovering the canvas, single keys place elements, fragments, and templates. A few examples:

| Key | Result |
| --- | --- |
| `o` / `n` / `s` / `f` | OH · NH₂ · SH · F |
| `O` / `N` | bare O · bare N atom |
| `2` | carbonyl (=O) |
| `a` | benzene ring |
| `4`–`8` | 4- to 8-membered ring |
| `z` / `Z` | zig-zag chain |
| `+` / `-` | increase / decrease charge |
| `Ctrl`+`K` | clean up layout |
| `Ctrl`+`C` / `Ctrl`+`V` | copy / paste (Windows) |

Selected atoms can be nudged with `Shift`+arrows (or `Ctrl`+arrows) and rotated with
`Alt`+arrows. The full, user-editable key map lives in
[`assets/default_config.json`](assets/default_config.json).

## Configuration

`Config` holds the key shortcuts, drawing style (label size, bond widths, dash spacing, …),
and fragment library. Defaults are compiled into the crate; `Config::load()` additionally
reads `chembuilder_config.json` and a `fragments/` directory from the working directory, and
`Config::save()` writes them back. New fragments can be saved straight from the example app's
toolbar.

## Platform support

The editor is cross-platform. The CDX / image / OLE **clipboard** features are Windows-only
(gated behind `#[cfg(windows)]`); on other platforms copy/paste falls back to plain text.

The example uses eframe's **glow** (OpenGL) backend rather than wgpu, to sidestep a wgpu-hal
Direct3D12 build issue on current Windows toolchains.

## Project layout

```
src/lib.rs            library root & public API
src/widget/           the ChemStructEditor widget (drawing + interaction)
src/molecule/         data model, MOL2, 2D clean-up, CDX/image/OLE (Windows)
src/config.rs         shortcuts, style, fragment library
src/bin/clipdump.rs   diagnostic tool: dump clipboard formats (Windows)
examples/editor.rs    full host application
assets/               default config + built-in fragment JSON
```

## Status

Pre-1.0 and evolving; the public API may change between minor versions.

## License

Copyright © 2026 IndigoCarmine. All rights reserved. See [LICENSE](LICENSE).
The licensing terms are still to be determined.
