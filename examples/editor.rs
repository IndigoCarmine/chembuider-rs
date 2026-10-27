//! Example host application for the `chembuider_rs` chemical-structure editor widget.
//!
//! Run natively with: `cargo run --example editor`
//!
//! Also builds for the web (wasm32, via eframe's wgpu/WebGPU backend) — see
//! `examples/editor_web/README.md` for the build steps.
//!
//! This wraps the library's [`ChemStructEditor`] widget in an eframe window with a toolbar
//! (tools, stereo, paste, clean-up, fragments, config, MOL2 export), a status bar and an
//! Analysis side panel (formula, exact mass, MW, ESI m/z, elemental analysis). The
//! widget itself is library code; everything here is host-app glue you'd write per application.

use chembuider_rs::molecule::cdx::molecule_to_cdxml;
use chembuider_rs::molecule::mol2::to_mol2_string;
use chembuider_rs::molecule::smiles::to_smiles;
use chembuider_rs::widget::analysis::AnalysisPanel;
use chembuider_rs::{bond_menu, BondStereo, ChemStructEditor, Config, Molecule, Tool};
use eframe::{egui, App};

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_title("ChemBuilder — MOL2 Editor"),
        ..Default::default()
    };
    eframe::run_native(
        "ChemBuilder",
        options,
        Box::new(|_cc| Ok(Box::new(EditorApp::default()))),
    )
}

/// Web entry point: attaches eframe to `<canvas id="the_canvas_id">` on the host page.
/// wasm-bindgen auto-invokes `main` on module load for a binary crate like this example.
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("failed to find #the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("#the_canvas_id was not a canvas");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_cc| Ok(Box::new(EditorApp::default()))),
            )
            .await;

        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => loading_text.remove(),
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p>The app has crashed. See the developer console for details.</p>",
                    );
                    panic!("failed to start eframe: {e:?}");
                }
            }
        }
    });
}

struct EditorApp {
    editor: ChemStructEditor,
    status: String,
    /// Name field for the "Save Fragment" feature.
    fragment_name: String,
    /// Color for the "Fill Ring" feature.
    ring_color: egui::Color32,
    /// The Analysis side panel (formula, masses, m/z) and whether it is shown.
    analysis: AnalysisPanel,
    show_analysis: bool,
}

impl Default for EditorApp {
    fn default() -> Self {
        // A host app may load user shortcuts/fragments from disk; the widget itself never does.
        let mut editor = ChemStructEditor::default();
        editor.config = Config::load();
        Self {
            editor,
            status: "Ready — Bond tool active. Click to place atoms, drag between atoms to draw bonds.".to_string(),
            fragment_name: String::new(),
            ring_color: egui::Color32::from_rgb(120, 200, 255),
            analysis: AnalysisPanel::default(),
            show_analysis: true,
        }
    }
}

impl App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // eframe 0.34 hands us the root viewport Ui; panels are shown inside it.
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Tool:");
                if ui.selectable_label(self.editor.tool == Tool::Select, "⬚ Select").clicked() {
                    self.editor.tool = Tool::Select;
                    self.status = tool_status(Tool::Select);
                }
                if ui.selectable_label(self.editor.tool == Tool::Bond, "✏ Bond").clicked() {
                    self.editor.tool = Tool::Bond;
                    self.status = tool_status(Tool::Bond);
                }
                if ui.selectable_label(self.editor.tool == Tool::Eraser, "✖ Eraser").clicked() {
                    self.editor.tool = Tool::Eraser;
                    self.status = tool_status(Tool::Eraser);
                }

                ui.separator();

                ui.label("Stereo:");
                if ui.selectable_label(self.editor.current_bond_stereo == BondStereo::None, "─ None").clicked() {
                    self.editor.current_bond_stereo = BondStereo::None;
                }
                if ui.selectable_label(self.editor.current_bond_stereo == BondStereo::WedgeUp, "▲ Wedge").clicked() {
                    self.editor.current_bond_stereo = BondStereo::WedgeUp;
                }
                if ui.selectable_label(self.editor.current_bond_stereo == BondStereo::WedgeDown, "┄ Hash").clicked() {
                    self.editor.current_bond_stereo = BondStereo::WedgeDown;
                }

                ui.separator();

                if ui.button("🧬 Copy SMILES").clicked() {
                    let smiles = to_smiles(&self.editor.molecule);
                    self.status = if smiles.is_empty() {
                        "Nothing drawn, so no SMILES to copy.".to_string()
                    } else {
                        ui.ctx().copy_text(smiles.clone());
                        format!("Copied SMILES: {smiles}")
                    };
                }

                if ui.button("📋 Paste").clicked() {
                    self.status = if self.editor.paste_clipboard() {
                        "Pasted structure from clipboard.".to_string()
                    } else {
                        "No structure on the clipboard to paste.".to_string()
                    };
                }

                ui.separator();

                let cleaning = self.editor.is_cleaning();
                let button = if cleaning {
                    egui::Button::new(egui::RichText::new("⛔ Stop").color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(200, 50, 50))
                } else {
                    egui::Button::new("✨ Clean Up")
                };
                if ui.add(button).clicked() {
                    self.editor.toggle_cleanup();
                    self.status = if cleaning {
                        "Cleanup stopped.".to_string()
                    } else {
                        "Cleaning up… (click Stop to cancel)".to_string()
                    };
                }

                ui.separator();

                ui.label("Fragment:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.fragment_name)
                        .hint_text("name")
                        .desired_width(70.0),
                );
                if ui.button("➕ Save Fragment").clicked() {
                    match self.editor.save_selection_as_fragment(&self.fragment_name) {
                        Ok(n) => {
                            self.status = format!(
                                "Saved fragment '{}' ({} atoms) to fragments/{}.json",
                                self.fragment_name.trim(), n, self.fragment_name.trim()
                            );
                            self.fragment_name.clear();
                        }
                        Err(e) => self.status = format!("Save fragment failed: {e}"),
                    }
                }

                ui.separator();

                if ui.button("⚙ Save Config").clicked() {
                    match self.editor.config.save() {
                        Ok(_) => self.status = "Config saved to chembuilder_config.json".to_string(),
                        Err(e) => self.status = format!("Config save failed: {e}"),
                    }
                }

                ui.separator();

                // Ring fill: pick a color, then fill the ring the selection/hotspot points at.
                // (Select an atom on the ring with the Select tool, or hover it, then click.)
                ui.label("Ring:");
                ui.color_edit_button_srgba(&mut self.ring_color);
                if ui.button("🎨 Fill Ring").clicked() {
                    let c = self.ring_color;
                    let rgba = [c.r(), c.g(), c.b(), c.a()];
                    self.status = if self.editor.fill_selected_ring(rgba) {
                        "Filled the ring through the selected/hotspot atom.".to_string()
                    } else {
                        "Select (or hover) an atom that sits on a ring first.".to_string()
                    };
                }
                if ui.button("🧹 Unfill Ring").clicked() {
                    self.status = if self.editor.clear_selected_ring_fill() {
                        "Removed the ring fill.".to_string()
                    } else {
                        "No ring fill on the selected/hotspot atom.".to_string()
                    };
                }

                ui.separator();

                if ui.button("🗑 Clear").clicked() {
                    // A running cleanup would go on writing into the cleared canvas.
                    self.editor.cancel_cleanup();
                    // An empty canvas has nothing to clear: an undo step there would only wipe
                    // the redo history. Otherwise snapshot first so Ctrl+Z brings it back.
                    if !self.editor.molecule.atoms.is_empty() {
                        self.editor.push_undo();
                        self.editor.molecule = Molecule::default();
                        self.editor.selected_atoms.clear();
                    }
                    self.status = "Canvas cleared.".to_string();
                }

                ui.separator();

                if ui.button("💾 Save MOL2").clicked() {
                    self.save_mol2();
                }

                if ui.button("💾 Export CDXML").clicked() {
                    self.save_cdxml();
                }

                ui.separator();

                ui.toggle_value(&mut self.show_analysis, "🔬 Analysis");
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Atoms: {}  Bonds: {}  Tool: {}  Element: {}  {}",
                    self.editor.molecule.atoms.len(),
                    self.editor.molecule.bonds.len(),
                    self.editor.tool,
                    self.editor.current_element,
                    self.status,
                ));
            });
        });

        if self.show_analysis {
            egui::Panel::right("analysis").resizable(true).default_size(230.0).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.analysis.ui(ui, &self.editor.molecule, &self.editor.selected_atoms) {
                        self.status = "Copied the analysis to the clipboard.".to_string();
                    }
                });
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            // Refresh the status line once a canvas edit makes the last toolbar message (e.g.
            // "Canvas cleared.") stale. Takes effect next repaint, since this panel draws after
            // the status bar above — imperceptible, as canvas edits already trigger a repaint.
            let out = self.editor.show(ui);
            // Right-click on a bond: ChemDraw's bond menu (order, line style, double-bond
            // side, delete). The choice is applied after the menu closure lets go of the editor.
            let mut chosen = None;
            let menu = out.response.context_menu(|ui| {
                match self.editor.context_bond().and_then(|id| self.editor.molecule.bond_by_id(id)) {
                    Some(bond) => chosen = bond_menu(ui, bond, false),
                    None => {
                        ui.label("Right-click a bond to change it.");
                    }
                }
            });
            if menu.is_none() {
                self.editor.clear_context_bond();
            }
            let menu_modified = match (chosen, self.editor.context_bond()) {
                (Some(action), Some(id)) => self.editor.apply_bond_menu(id, action),
                _ => false,
            };
            if out.modified || menu_modified {
                self.status = tool_status(self.editor.tool.clone());
            }
        });
    }
}

/// The toolbar message shown when a tool is selected, and reused to refresh the status line
/// after a canvas edit (see the `CentralPanel` above).
fn tool_status(tool: Tool) -> String {
    match tool {
        Tool::Select => "Select: click/lasso to select, double-click for whole molecule, Alt+drag to rotate, Delete to remove.".to_string(),
        Tool::Bond => "Bond: click atom to extend chain, drag atom→atom to draw bond, click bond to cycle order.".to_string(),
        Tool::Eraser => "Eraser: click an atom or bond to remove it.".to_string(),
    }
}

impl EditorApp {
    fn save_mol2(&mut self) {
        if self.editor.molecule.atoms.is_empty() {
            self.status = "Nothing to save — draw a molecule first.".to_string();
            return;
        }
        let mol2_text = to_mol2_string(&self.editor.molecule);

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Tripos MOL2", &["mol2"])
                .set_file_name("molecule.mol2")
                .save_file()
            {
                match std::fs::write(&path, mol2_text) {
                    Ok(_) => self.status = format!("Saved: {}", path.display()),
                    Err(e) => self.status = format!("Save failed: {e}"),
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            match download_text_file("molecule.mol2", "chemical/x-mol2", &mol2_text) {
                Ok(()) => self.status = "Downloaded molecule.mol2".to_string(),
                Err(e) => self.status = format!("Download failed: {e:?}"),
            }
        }
    }

    fn save_cdxml(&mut self) {
        if self.editor.molecule.atoms.is_empty() {
            self.status = "Nothing to export — draw a molecule first.".to_string();
            return;
        }
        let cdxml_text = molecule_to_cdxml(&self.editor.molecule);

        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("ChemDraw CDXML", &["cdxml"])
                .set_file_name("molecule.cdxml")
                .save_file()
            {
                match std::fs::write(&path, cdxml_text) {
                    Ok(_) => self.status = format!("Saved: {}", path.display()),
                    Err(e) => self.status = format!("Save failed: {e}"),
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            match download_text_file("molecule.cdxml", "chemical/x-cdxml", &cdxml_text) {
                Ok(()) => self.status = "Downloaded molecule.cdxml".to_string(),
                Err(e) => self.status = format!("Download failed: {e:?}"),
            }
        }
    }
}

/// Triggers a browser download of `contents` by creating an object URL and clicking a
/// throwaway `<a download>` link — there is no native save dialog to call into on the web.
#[cfg(target_arch = "wasm32")]
fn download_text_file(
    filename: &str,
    mime_type: &str,
    contents: &str,
) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast as _;
    use web_sys::{Blob, BlobPropertyBag, HtmlAnchorElement, Url};

    let parts = js_sys::Array::new();
    parts.push(&wasm_bindgen::JsValue::from_str(contents));
    let bag = BlobPropertyBag::new();
    bag.set_type(mime_type);
    let blob = Blob::new_with_str_sequence_and_options(&parts, &bag)?;
    let url = Url::create_object_url_with_blob(&blob)?;

    let document = web_sys::window().unwrap().document().unwrap();
    let anchor: HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    Url::revoke_object_url(&url)?;
    Ok(())
}
