//! Example host application for the `chembuider_rs` chemical-structure editor widget.
//!
//! Run with: `cargo run --example editor`
//!
//! This wraps the library's [`ChemStructEditor`] widget in an eframe window with a toolbar
//! (tools, stereo, paste, clean-up, fragments, config, MOL2 export) and a status bar. The
//! widget itself is library code; everything here is host-app glue you'd write per application.

use chembuider_rs::molecule::mol2::to_mol2_string;
use chembuider_rs::{BondStereo, ChemStructEditor, Config, Molecule, Tool};
use eframe::{egui, App};

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

struct EditorApp {
    editor: ChemStructEditor,
    status: String,
    /// Name field for the "Save Fragment" feature.
    fragment_name: String,
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
        }
    }
}

impl App for EditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // eframe 0.34 hands us the root viewport Ui; panels are shown inside it.
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("Tool:");
                if ui.selectable_label(self.editor.tool == Tool::Select, "⬚ Select").clicked() {
                    self.editor.tool = Tool::Select;
                    self.status = "Select: click/lasso to select, double-click for whole molecule, Alt+drag to rotate, Delete to remove.".to_string();
                }
                if ui.selectable_label(self.editor.tool == Tool::Bond, "✏ Bond").clicked() {
                    self.editor.tool = Tool::Bond;
                    self.status = "Bond: click atom to extend chain, drag atom→atom to draw bond, click bond to cycle order.".to_string();
                }
                if ui.selectable_label(self.editor.tool == Tool::Eraser, "✖ Eraser").clicked() {
                    self.editor.tool = Tool::Eraser;
                    self.status = "Eraser: click an atom or bond to remove it.".to_string();
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

                if ui.button("🗑 Clear").clicked() {
                    self.editor.molecule = Molecule::default();
                    self.editor.selected_atoms.clear();
                    self.status = "Canvas cleared.".to_string();
                }

                ui.separator();

                if ui.button("💾 Save MOL2").clicked() {
                    self.save_mol2();
                }
            });
        });

        egui::Panel::bottom("status").show_inside(ui, |ui| {
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

        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.editor.ui(ui);
        });
    }
}

impl EditorApp {
    fn save_mol2(&mut self) {
        if self.editor.molecule.atoms.is_empty() {
            self.status = "Nothing to save — draw a molecule first.".to_string();
            return;
        }
        let mol2_text = to_mol2_string(&self.editor.molecule);
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
}
