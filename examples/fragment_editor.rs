//! Example host application for authoring reusable chemical **fragments**.
//!
//! Run with: `cargo run --example fragment_editor`
//!
//! A fragment is a small structure plus one *attach atom* (`attach_idx`) — the atom that bonds
//! to the host structure when the fragment is inserted. This example wraps the library's
//! [`ChemStructEditor`] widget and adds host-side glue to:
//!   * draw / edit a structure (Select / Bond / Eraser tools, Clean Up),
//!   * mark one atom as the attach point ("🎯 Set Attach"),
//!   * save the whole canvas as `fragments/<name>.json`, and
//!   * load an existing `fragments/*.json` back for editing.
//!
//! The whole canvas is treated as a single fragment. Saving/loading goes through the
//! `Fragment` JSON/Molecule interop methods (`Fragment::load` / `to_molecule` /
//! `from_molecule` / `save`), so files round-trip with the `fragments/` directory the widget
//! loads at startup.

use chembuider_rs::molecule::fragment::Fragment;
use chembuider_rs::widget::FitOptions;
use chembuider_rs::{ChemStructEditor, Molecule, Tool};
use eframe::{egui, App};

/// How a loaded fragment is framed: fragments are small, so allow up to 2x before stopping,
/// and keep a 60 px margin around the drawing (atoms and their labels).
const LOAD_FIT: FitOptions =
    FitOptions { margin: 60.0, min_zoom: 0.2, max_zoom: 2.0, keep_zoom: false };

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 750.0])
            .with_title("ChemBuilder — Fragment Editor"),
        ..Default::default()
    };
    eframe::run_native(
        "ChemBuilder Fragment Editor",
        options,
        Box::new(|_cc| Ok(Box::new(FragmentEditorApp::default()))),
    )
}

struct FragmentEditorApp {
    editor: ChemStructEditor,
    /// Fragment file name (without extension).
    name: String,
    /// Host-tracked attach atom, by stable atom id (the widget has no attach concept).
    attach_atom: Option<u32>,
    /// Fragment names discovered under `fragments/`, for the Load combo.
    fragments: Vec<String>,
    /// Index into `fragments` currently chosen in the Load combo.
    selected_load: usize,
    status: String,
}

impl Default for FragmentEditorApp {
    fn default() -> Self {
        let mut app = Self {
            editor: ChemStructEditor::default(),
            name: String::new(),
            attach_atom: None,
            fragments: Vec::new(),
            selected_load: 0,
            status: "Draw a fragment (Bond tool), type a name, then 💾 Save. Attach point defaults \
                     to the first atom — override it with 🎯 Set Attach on a selected atom."
                .to_string(),
        };
        app.refresh_list();
        app
    }
}

impl App for FragmentEditorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Set by a successful Load in the toolbar; the canvas below fits the view this frame.
        let mut loaded = false;
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                // ── Tools ──────────────────────────────────────────────────────
                ui.label("Tool:");
                if ui.selectable_label(self.editor.tool == Tool::Select, "⬚ Select").clicked() {
                    self.editor.tool = Tool::Select;
                }
                if ui.selectable_label(self.editor.tool == Tool::Bond, "✏ Bond").clicked() {
                    self.editor.tool = Tool::Bond;
                }
                if ui.selectable_label(self.editor.tool == Tool::Eraser, "✖ Eraser").clicked() {
                    self.editor.tool = Tool::Eraser;
                }

                ui.separator();

                // ── Clean Up ───────────────────────────────────────────────────
                let cleaning = self.editor.is_cleaning();
                let button = if cleaning {
                    egui::Button::new(egui::RichText::new("⛔ Stop").color(egui::Color32::WHITE))
                        .fill(egui::Color32::from_rgb(200, 50, 50))
                } else {
                    egui::Button::new("✨ Clean Up")
                };
                if ui.add(button).clicked() {
                    self.editor.toggle_cleanup();
                }

                ui.separator();

                // ── Attach point ───────────────────────────────────────────────
                if ui.button("🎯 Set Attach").clicked() {
                    self.set_attach_from_selection();
                }
                match self.attach_atom {
                    Some(id) => ui.label(format!("attach = atom #{id}")),
                    None => ui.label(egui::RichText::new("attach = —").weak()),
                };

                ui.separator();

                // ── Save ───────────────────────────────────────────────────────
                ui.label("Name:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.name)
                        .hint_text("fragment name")
                        .desired_width(90.0),
                );
                if ui.button("💾 Save Fragment").clicked() {
                    self.status = match self.save_fragment() {
                        Ok(msg) => msg,
                        Err(e) => format!("Save failed: {e}"),
                    };
                }
            });

            ui.horizontal_wrapped(|ui| {
                // ── Load ───────────────────────────────────────────────────────
                ui.label("Load:");
                let current = self.fragments.get(self.selected_load).cloned().unwrap_or_default();
                egui::ComboBox::from_id_salt("load_fragment")
                    .selected_text(if current.is_empty() { "—".to_string() } else { current })
                    .width(140.0)
                    .show_ui(ui, |ui| {
                        for (i, name) in self.fragments.iter().enumerate() {
                            ui.selectable_value(&mut self.selected_load, i, name);
                        }
                    });
                if ui.button("📂 Load").clicked() {
                    if let Some(name) = self.fragments.get(self.selected_load).cloned() {
                        self.status = match self.load_fragment(&name) {
                            Ok(msg) => {
                                loaded = true;
                                msg
                            }
                            Err(e) => format!("Load failed: {e}"),
                        };
                    } else {
                        self.status = "No fragment to load — the fragments/ folder is empty.".to_string();
                    }
                }
                if ui.button("🔄 Refresh").clicked() {
                    self.refresh_list();
                    self.status = format!("Found {} fragment(s) in fragments/.", self.fragments.len());
                }

                ui.separator();

                if ui.button("🗑 Clear").clicked() {
                    // A running cleanup would go on writing into the cleared canvas.
                    self.editor.cancel_cleanup();
                    self.editor.molecule = Molecule::default();
                    self.editor.selected_atoms.clear();
                    self.editor.hotspot_atom = None;
                    self.attach_atom = None;
                    self.name.clear();
                    self.status = "Canvas cleared.".to_string();
                }
            });
        });

        egui::Panel::bottom("status").show(ui, |ui| {
            let attach = self.attach_atom.map_or("—".to_string(), |id| format!("#{id}"));
            ui.label(format!(
                "Atoms: {}  Bonds: {}  Tool: {}  Attach: {}  |  {}",
                self.editor.molecule.atoms.len(),
                self.editor.molecule.bonds.len(),
                self.editor.tool,
                attach,
                self.status,
            ));
        });

        egui::CentralPanel::default().show(ui, |ui| {
            // The toolbar and status panels are laid out by now, so the rect `editor.ui` is
            // about to fill is known: fit on it in the same frame as the load.
            if loaded {
                self.editor.fit_view_with(ui.ctx(), ui.available_rect_before_wrap(), LOAD_FIT);
            }
            self.editor.ui(ui);
        });
    }
}

impl FragmentEditorApp {
    /// Rescan `fragments/` for `*.json` and refresh the Load combo (names sorted).
    fn refresh_list(&mut self) {
        let mut names: Vec<String> = std::fs::read_dir("fragments")
            .into_iter()
            .flatten()
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.extension()?.to_str()? == "json")
                    .then(|| p.file_stem()?.to_str().map(str::to_owned))
                    .flatten()
            })
            .collect();
        names.sort();
        if self.selected_load >= names.len() {
            self.selected_load = 0;
        }
        self.fragments = names;
    }

    /// Mark the single selected atom as the fragment's attach point.
    fn set_attach_from_selection(&mut self) {
        let mut ids = self.editor.selected_atoms.iter().copied();
        match (ids.next(), ids.next()) {
            (Some(id), None) => {
                self.attach_atom = Some(id);
                self.editor.hotspot_atom = Some(id); // widget highlights it → visual attach marker
                self.status = format!("Attach point set to atom #{id}.");
            }
            (None, _) => self.status = "Select a single atom (Select tool), then 🎯 Set Attach.".to_string(),
            _ => self.status = "Select exactly one atom for the attach point.".to_string(),
        }
    }

    /// Load `fragments/<name>.json` into the editor and mark its attach atom.
    fn load_fragment(&mut self, name: &str) -> Result<String, String> {
        let frag = Fragment::load(format!("fragments/{name}.json")).map_err(|e| e.to_string())?;
        if frag.attach_idx >= frag.atoms.len() {
            return Err(format!("{name}: attach_idx out of range"));
        }
        // A running cleanup would go on writing into the loaded fragment.
        self.editor.cancel_cleanup();
        self.editor.molecule = frag.to_molecule();
        self.editor.selected_atoms.clear();
        // `to_molecule` adds atoms in fragment order, so atoms[attach_idx] is the attach atom.
        let attach_id = self.editor.molecule.atoms[frag.attach_idx].id;
        self.attach_atom = Some(attach_id);
        self.editor.hotspot_atom = Some(attach_id);
        self.name = frag.name.clone();
        Ok(format!(
            "Loaded '{}' ({} atoms); attach = atom #{attach_id}.",
            frag.name,
            frag.atoms.len()
        ))
    }

    /// Save the whole canvas as `fragments/<name>.json`, recentered on the attach atom.
    ///
    /// The attach atom is optional: it defaults to the explicit 🎯 choice if still present,
    /// else the hotspot, else the lowest-id atom — the same fallback the library's
    /// `save_selection_as_fragment` uses — so a freshly drawn fragment saves without an extra step.
    fn save_fragment(&mut self) -> Result<String, String> {
        let name = self.name.trim().to_string();
        if name.is_empty() {
            return Err("name the fragment first".into());
        }
        // The name becomes `fragments/<name>.json`; refuse one that is not a plain file name.
        Fragment::validate_name(&name)?;
        if self.editor.molecule.atoms.is_empty() {
            return Err("draw a structure first".into());
        }
        let present = |id: u32, m: &Molecule| m.atom_by_id(id).is_some();
        let id = self
            .attach_atom
            .filter(|&i| present(i, &self.editor.molecule))
            .or_else(|| self.editor.hotspot_atom.filter(|&i| present(i, &self.editor.molecule)))
            .or_else(|| self.editor.molecule.atoms.iter().map(|a| a.id).min())
            .ok_or("draw a structure first")?;
        // The editor tracks atoms by stable id; `from_molecule` indexes positionally.
        let attach_idx = self
            .editor
            .molecule
            .atoms
            .iter()
            .position(|a| a.id == id)
            .expect("resolved attach id exists in the molecule");

        let mut frag = Fragment::from_molecule(&self.editor.molecule, attach_idx);
        frag.name = name.clone();
        // Recenter on the attach atom so saved files match the fragments/ convention (attach
        // near origin). Cosmetic — `insert_fragment` orients relative to the attach atom anyway.
        let base = frag.atoms[attach_idx].pos;
        for a in &mut frag.atoms {
            a.pos[0] -= base[0];
            a.pos[1] -= base[1];
        }

        std::fs::create_dir_all("fragments").map_err(|e| e.to_string())?;
        frag.save(format!("fragments/{name}.json")).map_err(|e| e.to_string())?;
        // Reflect the resolved attach atom in the UI (in case it fell back to a default).
        self.attach_atom = Some(id);
        self.editor.hotspot_atom = Some(id);
        self.refresh_list();
        Ok(format!(
            "Saved '{name}' ({} atoms, attach = atom #{id}) to fragments/{name}.json",
            frag.atoms.len()
        ))
    }
}
