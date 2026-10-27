# chembuider-rs

*[日本語版 README →](README.ja.md)*

An [egui](https://github.com/emilk/egui) widget library for drawing and editing **2D chemical
structure diagrams** in Rust — skeletal formulas, stereo bonds, ring & functional-group
templates, automatic 2D clean-up, MOL2 / SMILES export, and (on Windows) ChemDraw-compatible
clipboard interop.

The core type is [`ChemStructEditor`](src/widget/mod.rs): an immediate-mode widget you hold in
your app and render each frame with `editor.ui(ui)`. Everything app-specific (toolbar, menus,
file dialogs) lives in your host application — see [`examples/editor.rs`](examples/editor.rs)
for a complete one.

## Features

- **Drawing tools** — Select, Bond, and Eraser, with hover highlighting and snapping.
- **Atoms & elements** — single-key placement of heteroatoms and charges; carbons are drawn
  implicitly with implicit hydrogens (CH₃, OH, NH₂ …) and subscripts.
- **Rich labels** — atom labels render with true baseline-aligned subscripts and
  superscripts (NH₃⁺, SO₄²⁻); custom labels support a lightweight markup for scripts,
  bold, and italics (see [Rich text labels](#rich-text-labels)).
- **Bonds** — single / double / triple (cycle by clicking), plus wedge, hash, bold, dashed,
  and wavy stereo bonds.
- **Templates** — rings (3–10 membered), zig-zag chains, benzene, and a library of built-in
  functional-group **fragments** (Boc, Cbz, CF₃, CO₂Me, TMS, BPin, …) you can extend.
- **Edit gestures** — lasso / double-click selection, drag to move, `Alt`-drag to rotate,
  drag-onto-atom to merge, `Delete` to remove.
- **2D clean-up** — force-directed layout relaxation that runs on a background thread (never
  freezes the UI) and can be cancelled.
- **Valence hints** — over-bonded atoms are flagged with a red dashed circle.
- **Export** — Tripos **MOL2** via `molecule::mol2::to_mol2_string`, and canonical **SMILES**
  via `molecule::smiles::to_smiles` (Kekulé, deterministic, no stereochemistry).
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
| `Ctrl`+`Z` | undo |
| `Ctrl`+`Y` / `Ctrl`+`Shift`+`Z` | redo |
| `Ctrl`+`C` / `Ctrl`+`V` | copy / paste (Windows) |
| `Ctrl`+`0` | fit the whole structure to the view |

Selected atoms can be nudged with `Shift`+arrows (or `Ctrl`+arrows) and rotated with
`Alt`+arrows. The full, user-editable key map lives in
[`assets/default_config.json`](assets/default_config.json).

egui's default `zoom_with_keyboard` option also treats `Ctrl`+`0` as "reset GUI zoom", so if
the GUI scale was changed with `Ctrl`+`+`/`-`, the same key resets it too. A host app can turn
that off with `ctx.options_mut(|o| o.zoom_with_keyboard = false)`.

## Rich text labels

Atom labels are drawn by a dedicated rich-text renderer (`widget::rich_text`) that lays each
styled run out as its own galley and places it with real baseline arithmetic, so subscripts,
superscripts, bold, and italics stay crisp at any zoom. Hydrogen counts become subscripts and
charges become superscripts automatically (NH₃⁺, SO₄²⁻).

Labels follow ChemDraw drawing conventions: the element symbol itself (not the label's
bounding box) is centered on the atom position, hydrogens flip to the atom's free side when
its bonds come from the right (HO–, H₂N–), and bonds are trimmed at the label boundary
instead of striking through the text. Rendering across many structures can be checked with
`cargo run --example render_gallery`, which draws the RDKit-generated fixtures in
`tests/fixtures/rendering/` headlessly (raw and after 2D clean-up) into BMP contact sheets.

Custom labels (press `` ` `` on an atom to edit) and the standalone `RichLabel` widget accept
a lightweight markup:

| syntax          | result                                |
|-----------------|---------------------------------------|
| `_2` / `_{aq}`  | subscript (one char / braced group)   |
| `^+` / `^{2-}`  | superscript (one char / braced group) |
| `**text**`      | bold                                  |
| `//text//`      | italic                                |
| `\x`            | literal `x` (escape)                  |

So `CO_2Et` renders as CO₂Et and `//t//Bu` as *t*Bu. The renderer is exported for host apps:

```rust
use chembuider_rs::{RichLabel, RichText};

ui.add(RichLabel::new("[Cu(NH_3)_4]^{2+}").size(20.0)); // as an egui widget
RichText::parse("SO_4^{2-}")                            // or paint directly
    .paint(&painter, pos, egui::Align2::CENTER_CENTER, &style);
```

## Configuration

`Config` holds the key shortcuts, drawing style (label size, bond widths, dash spacing, …),
and fragment library. Defaults are compiled into the crate; `Config::load()` additionally
reads `chembuilder_config.json` and a `fragments/` directory from the working directory, and
`Config::save()` writes them back. New fragments can be saved straight from the example app's
toolbar.

## Editing ChemDraw objects in ibisOffice

`chembuilder-office` (feature `office`, Windows) is an external editor for
[ibisOffice](https://github.com/IndigoCarmine/ibis-office): its embedded structures are ChemDraw
objects (`ChemDraw_x64.Document.6.0`, the CDX in `CONTENTS`), so a file saved in ibisOffice opens
in PowerPoint/Word/Excel with ChemDraw, and the other way round.

```sh
cargo build --release --features office --bin chembuilder-office
target/release/chembuilder-office --install-ibis-handler   # writes %APPDATA%\ibisOffice\embed-handlers\chembuilder.toml
```

In ibisOffice, then:

- double-click a ChemDraw object to edit it here; each edit goes back to the document;
- Ctrl+V a structure copied in ChemDraw (or ChemBuilder) to paste it as a ChemDraw object;
- Insert ▸ Object ▸ ChemBuilder for a new one, or ▸ From File for a `.cdx`;
- Ctrl+C an object to paste it into Office or ChemDraw.

ChemDraw's own pictures of objects (vector EMF) are drawn by ChemBuilder on the screen; the
file keeps them until an object is edited. Run by hand, `chembuilder-office file.cdx` edits a
CDX file. The protocol is implemented from ibisOffice's published description and shares no
code with it.

## Platform support

The editor is cross-platform. The CDX / image / OLE **clipboard** features are Windows-only
(gated behind `#[cfg(windows)]`); on other platforms copy/paste falls back to plain text.

The example uses eframe's **glow** (OpenGL) backend rather than wgpu, to sidestep a wgpu-hal
Direct3D12 build issue on current Windows toolchains.

## Project layout

```
src/lib.rs            library root & public API
src/widget/           the ChemStructEditor widget (drawing + interaction)
src/molecule/         data model, MOL2, SMILES, 2D clean-up, CDX/image/OLE (Windows)
src/config.rs         shortcuts, style, fragment library
src/bin/clipdump.rs   diagnostic tool: dump clipboard formats (Windows)
examples/editor.rs    full host application
examples/render_gallery.rs  headless BMP gallery of the rendering fixtures
scripts/gen_test_structures.py  RDKit generator for tests/fixtures/rendering/
assets/               default config + built-in fragment JSON
```

## Status

Pre-1.0 and evolving; the public API may change between minor versions.

## License

Copyright © 2026 IndigoCarmine. All rights reserved. See [LICENSE](LICENSE).
The licensing terms are still to be determined.
