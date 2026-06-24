use crate::molecule::{mol2::to_mol2_string, BondOrder, BondStereo};
use crate::widget::{ChemStructEditor, Tool};
use eframe::{egui, App};

pub struct Mol2App {
    editor: ChemStructEditor,
    status: String,
    /// Name field for the "Save Fragment" feature.
    fragment_name: String,
}

impl Default for Mol2App {
    fn default() -> Self {
        Self {
            editor: ChemStructEditor::default(),
            status: "Ready — Bond tool active. Click to place atoms, drag between atoms to draw bonds.".to_string(),
            fragment_name: String::new(),
        }
    }
}

impl App for Mol2App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                // Tool selection
                ui.label("Tool:");
                if ui
                    .selectable_label(self.editor.tool == Tool::Select, "⬚ Select")
                    .clicked()
                {
                    self.editor.tool = Tool::Select;
                    self.status = "Select: click atoms, lasso-drag to multi-select, Delete to remove.".to_string();
                }
                if ui
                    .selectable_label(self.editor.tool == Tool::Bond, "✏ Bond")
                    .clicked()
                {
                    self.editor.tool = Tool::Bond;
                    self.status = "Bond: click atom to extend chain, drag atom→atom to draw bond, click bond to cycle order.".to_string();
                }
                if ui
                    .selectable_label(self.editor.tool == Tool::Eraser, "✖ Eraser")
                    .clicked()
                {
                    self.editor.tool = Tool::Eraser;
                    self.status = "Eraser: click an atom or bond to remove it.".to_string();
                }

                ui.separator();

                // Element palette
                ui.label("Element:");
                for el in &["C", "N", "O", "S", "H", "P", "F", "Cl", "Br", "I"] {
                    if ui
                        .selectable_label(self.editor.current_element == *el, *el)
                        .clicked()
                    {
                        self.editor.current_element = el.to_string();
                    }
                }

                ui.separator();

                // Bond order
                ui.label("Bond:");
                if ui
                    .selectable_label(self.editor.current_bond_order == BondOrder::Single, "─ 1")
                    .clicked()
                {
                    self.editor.current_bond_order = BondOrder::Single;
                }
                if ui
                    .selectable_label(self.editor.current_bond_order == BondOrder::Double, "═ 2")
                    .clicked()
                {
                    self.editor.current_bond_order = BondOrder::Double;
                }
                if ui
                    .selectable_label(self.editor.current_bond_order == BondOrder::Triple, "≡ 3")
                    .clicked()
                {
                    self.editor.current_bond_order = BondOrder::Triple;
                }

                ui.separator();

                // Bond stereo
                ui.label("Stereo:");
                if ui.selectable_label(
                    self.editor.current_bond_stereo == BondStereo::None, "─ None"
                ).clicked() {
                    self.editor.current_bond_stereo = BondStereo::None;
                }
                if ui.selectable_label(
                    self.editor.current_bond_stereo == BondStereo::WedgeUp, "▲ Wedge"
                ).clicked() {
                    self.editor.current_bond_stereo = BondStereo::WedgeUp;
                }
                if ui.selectable_label(
                    self.editor.current_bond_stereo == BondStereo::WedgeDown, "┄ Hash"
                ).clicked() {
                    self.editor.current_bond_stereo = BondStereo::WedgeDown;
                }

                ui.separator();

                // Clean Up button (also Ctrl+K in the canvas).
                // Runs on a background thread; turns red ("Stop") while computing so it can
                // be cancelled and never freezes the UI.
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

                // Save selection as a reusable fragment
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

                // Save Config
                if ui.button("⚙ Save Config").clicked() {
                    match self.editor.config.save() {
                        Ok(_)  => self.status = "Config saved to chembuilder_config.json".to_string(),
                        Err(e) => self.status = format!("Config save failed: {e}"),
                    }
                }

                ui.separator();

                // Clear button
                if ui.button("🗑 Clear").clicked() {
                    self.editor.molecule = crate::molecule::Molecule::default();
                    self.editor.selected_atoms.clear();
                    self.status = "Canvas cleared.".to_string();
                }

                ui.separator();

                // Save MOL2
                if ui.button("💾 Save MOL2").clicked() {
                    self.save_mol2(ctx);
                }
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
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

        egui::CentralPanel::default().show(ctx, |ui| {
            self.editor.ui(ui);
        });
    }
}

impl Mol2App {
    fn save_mol2(&mut self, _ctx: &egui::Context) {
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
