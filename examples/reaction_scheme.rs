//! Reaction scheme + stoichiometry table, as an editor:
//!
//! ```text
//! cargo run --example reaction_scheme                       # open a window
//! cargo run --example reaction_scheme -- --render out.png   # headless render to a PNG
//! cargo run --example reaction_scheme -- --multi-step       # start on the four-step scheme
//! cargo run --example reaction_scheme -- --open scheme.cdxml   # start on a ChemDraw scheme
//! cargo run --example reaction_scheme -- --open a.cdxml --render out.png --save b.cdxml
//! ```
//!
//! The window starts on the ChemDraw *Analysis* figure it has always drawn (phenol + MeI /
//! K₂CO₃ → anisole) and lets you take it apart:
//!
//! * **structures come from the canvas on the left.** Draw one with the ordinary
//!   [`ChemStructEditor`], then *To scheme*. The table is generated from the scheme: every
//!   structure, and every formula written over the arrow, gets its column.
//! * **double-click a scheme structure** to load it back into the canvas, edit it, and write
//!   it back with *Replace selected structure* — the round-trip for editing a drawn molecule.
//! * **drag a scheme item** (structure, `+`, arrow, text) sideways to reorder the reaction
//!   line; a caret shows where it will drop.
//! * **select a scheme item and press Delete/Backspace** to remove it (a structure takes its
//!   table column with it).
//! * **right-click a scheme item** for its full menu: clean up the layout, edit in the canvas,
//!   replace, move, delete — so the figure is fully operable without the side panel.
//! * **click a cell of the table to edit it.** Only the rows that can hold an entered value
//!   open a box (`Limiting?`, `Equivalents`, `Sample Mass`, …); everything else is derived
//!   and recomputed as you type. `Limiting?` is a toggle.
//! * **click above or below the arrow** to write its reagents and conditions; the usual
//!   rich-text markup (`K_2CO_3`) applies.
//! * **💾 Save / 📂 Open** write and read the whole figure — scheme, table and every entered
//!   value — as a `.rxn.json` file ([`Reaction::to_json`]).
//! * **multi-step schemes**: add arrows and structures in a row, and a *line break* to go on
//!   on a new row (or set a row width to wrap at). Click under a structure to give it a
//!   compound label (`(I)`, `**4**`; Shift+Enter starts a second line). A scheme of two or more
//!   steps is a drawing only — its table is not shown.
//! * **open and save ChemDraw schemes** (CDXML or CDX) from the toolbar.
//!
//! `--render` is unaffected by any of this: it runs the plain [`ReactionView`] paint path.
//! See [`chembuider_rs::reaction::edit`] for the editing API this app is a host for.

use chembuider_rs::egui;
use chembuider_rs::reaction::edit::{
    EditTarget, Hit, SchemePart, StructureRef, cleanup_scheme_structure, scheme_arrow_structure,
};
use chembuider_rs::reaction::cdxml::{scheme_from_cdx, scheme_from_cdxml};
use chembuider_rs::reaction::{
    Analysis, Arrow, Mass, MolecularFormula, Product, Reactant, Reaction, ReactionEditor,
    ReactionScheme, SchemeItem, demo,
};
use chembuider_rs::{ChemStructEditor, Molecule, Tool};

/// The reference figure: PhOH + MeI / K₂CO₃ → PhOMe, 700 mg isolated (60.92 %).
fn reference_reaction() -> Reaction {
    // The arrow carries a *structure* above the shaft — MeI, drawn as the same kind of object
    // as the main-line structures — with the base written out below it, to show the two kinds
    // of arrow label side by side. Double-click the MeI to edit it on the canvas.
    let scheme = ReactionScheme::new()
        .structure(demo::phenol())
        .arrow(Arrow::new().above_structure(demo::methyl_iodide()).below("K_2CO_3"))
        .structure(demo::anisole());

    // The reactant columns follow the scheme in order: the main-line phenol first, then the
    // MeI riding on the arrow (a reagent shown as its structure), then the K₂CO₃ written out
    // below it. Every column is a compound the scheme shows, and editing the MeI on the canvas
    // flows into its column, formula/MW and all.
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
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let reaction = match value("--open") {
        Some(path) => match open_scheme(std::path::Path::new(&path)) {
            Ok(reaction) => reaction,
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        },
        None if args.iter().any(|a| a == "--multi-step") => demo::multi_step_scheme(),
        None => reference_reaction(),
    };
    if let Some(path) = value("--save") {
        headless::save(&path, &reaction);
    }
    if args.iter().any(|a| a == "--render") {
        let path = value("--render").unwrap_or_else(|| "target/reaction_scheme.png".to_string());
        headless::render(&path, &reaction);
    }
    if args.iter().any(|a| a == "--render" || a == "--save") {
        return Ok(());
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1500.0, 860.0])
            .with_title("ChemBuilder — Reaction Scheme & Analysis"),
        ..Default::default()
    };
    eframe::run_native(
        "ChemBuilder Reaction",
        options,
        Box::new(|cc| {
            // The figure is drawn on white paper; a light theme keeps the page around it light.
            cc.egui_ctx.set_visuals(egui::Visuals::light());
            Ok(Box::new(ReactionApp { reaction, ..Default::default() }))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {}

/// A ChemDraw scheme (CDXML, or binary CDX) from a file, as a reaction with an empty table.
#[cfg(not(target_arch = "wasm32"))]
fn open_scheme(path: &std::path::Path) -> Result<Reaction, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let scheme = if bytes.starts_with(b"VjCD") {
        scheme_from_cdx(&bytes)
    } else {
        scheme_from_cdxml(&String::from_utf8_lossy(&bytes))
    };
    scheme
        .map(Reaction::from_scheme)
        .ok_or_else(|| format!("{}: no reaction scheme could be read", path.display()))
}

struct ReactionApp {
    reaction: Reaction,
    /// Keep the editor between frames: it owns the view, which owns the drawing context.
    editor: ReactionEditor,
    /// The one place structures are drawn. Everything in the figure was taken from here.
    canvas: ChemStructEditor,
    /// The scheme structure the canvas is currently editing, if any (a main-line structure or
    /// one on an arrow label). While set, canvas edits are written straight back into this slot
    /// every frame so the table below follows live.
    linked_structure: Option<StructureRef>,
    status: String,
}

impl Default for ReactionApp {
    fn default() -> Self {
        let mut canvas = ChemStructEditor::default();
        // Something to take into the figure straight away, so the flow is discoverable.
        canvas.molecule = demo::benzene();
        Self {
            reaction: reference_reaction(),
            editor: ReactionEditor::new(),
            canvas,
            linked_structure: None,
            status: "Draw on the canvas, then add it to the figure. Double-click a structure to \
                     edit it — canvas edits then flow into the table live. Drag scheme items to \
                     reorder, right-click for more."
                .to_string(),
        }
    }
}

/// A structural edit queued while the context menu is being drawn, applied once it is closed.
enum Action {
    RemoveSchemeItem(usize),
    MoveSchemeItem(usize, isize),
    ReplaceSchemeStructure(usize),
    /// Relax the layout of a scheme structure with the automatic 2-D cleanup (課題A).
    CleanupSchemeStructure(usize),
    /// Load a scheme structure into the canvas so it can be edited, then written back (課題B).
    /// The target is a main-line structure or one riding on an arrow label.
    EditSchemeStructureInCanvas(StructureRef),
    /// Put the canvas structure on an arrow's above/below label.
    PlaceArrowStructure(usize, SchemePart),
    EditSchemeText(usize, SchemePart),
}

impl eframe::App for ReactionApp {
    // eframe 0.34+ hands us the root viewport Ui; panels are shown inside it.
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(&self.status);
                // Clicks on the figure are handled inside the editor, so the selection is
                // what says where they landed.
                if let Some(hit) = self.editor.selection() {
                    ui.separator();
                    ui.weak(describe(hit, &self.reaction));
                }
                if let Some(error) = self.editor.error() {
                    ui.separator();
                    ui.colored_label(egui::Color32::from_rgb(180, 40, 40), error);
                }
            });
        });

        egui::Panel::left("canvas")
            .default_size(420.0)
            .show(ui, |ui| {
                ui.label("Structure canvas — draw here, then add it to the figure above.");
                self.canvas.ui(ui);
            });

        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                // Carry this frame's canvas edits into the linked scheme structure before the
                // figure is drawn, so the table below re-derives live as the structure changes.
                self.sync_linked_structure();
                let response = self.editor.show(ui, &mut self.reaction);
                // Clicking the figure moves the user's attention off the canvas: drop the
                // canvas selection so Delete/Backspace (which now remove the selected scheme
                // item) cannot also delete atoms still selected over there.
                if response.clicked() {
                    self.canvas.selected_atoms.clear();
                }
                // A drag on the figure moved structures to other positions. The link names its
                // slot by position, so it may now point at a different structure — drop it
                // before the next frame's sync writes the canvas over that one (as `apply` does
                // for the context menu's reorders).
                if self.editor.take_scheme_rearranged() && self.linked_structure.take().is_some() {
                    self.status =
                        "The figure was rearranged — the canvas is no longer linked.".to_string();
                }
                // Right-click a scheme item for its actions, so the figure is fully operable on
                // its own (課題D).
                self.scheme_context_menu(response);
                // A double-click on a structure (main-line or on an arrow label) loads it into
                // the canvas for editing (課題B).
                if let Some(target) = self.editor.take_edit_request() {
                    self.apply(Action::EditSchemeStructureInCanvas(target));
                }
            });
        });
    }
}

impl ReactionApp {
    /// Start over on `reaction`: nothing selected, nothing linked to the canvas (they all name
    /// items of the old figure).
    fn load(&mut self, reaction: Reaction, status: &str) {
        self.editor.select(None);
        self.editor.cancel_edit();
        self.linked_structure = None;
        self.reaction = reaction;
        self.status = status.to_string();
    }

    /// Replace the figure with a ChemDraw scheme (CDXML or CDX) the user picks.
    fn open_file(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = rfd::FileDialog::new().add_filter("ChemDraw", &["cdxml", "cdx"]).pick_file() {
            match open_scheme(&path) {
                Ok(reaction) => self.load(reaction, &format!("Opened {}.", path.display())),
                Err(e) => self.status = e,
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = "Opening files is not available in the browser build.".to_string();
        }
    }

    /// Save the scheme as CDXML (or binary CDX). The table is not part of a ChemDraw scheme.
    fn save_file(&mut self, painter: &egui::Painter, binary: bool) {
        if self.reaction.scheme.is_empty() {
            self.status = "The scheme is empty — nothing to save.".to_string();
            return;
        }
        let (bytes, ext) = if binary {
            (self.editor.view.scheme_cdx(painter, &self.reaction.scheme), "cdx")
        } else {
            (self.editor.view.scheme_cdxml(painter, &self.reaction.scheme).into_bytes(), "cdxml")
        };
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("ChemDraw", &[ext])
            .set_file_name(format!("scheme.{ext}"))
            .save_file()
        {
            self.status = match std::fs::write(&path, bytes) {
                Ok(()) => format!("Saved {}.", path.display()),
                Err(e) => format!("Save failed: {e}"),
            };
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (bytes, ext);
            self.status = "Saving is not available in the browser build.".to_string();
        }
    }

    /// Write the whole figure to a `.rxn.json` file the user picks.
    fn save_reaction(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("ChemBuilder reaction", &["rxn.json", "json"])
                .set_file_name("reaction.rxn.json")
                .save_file()
            else {
                return;
            };
            self.status = match std::fs::write(&path, self.reaction.to_json()) {
                Ok(()) => format!("Saved: {}", path.display()),
                Err(e) => format!("Save failed: {e}"),
            };
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = "Saving is not available in the browser build.".to_string();
        }
    }

    /// Replace the figure with one read from a `.rxn.json` file the user picks.
    fn open_reaction(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("ChemBuilder reaction", &["rxn.json", "json"])
                .pick_file()
            else {
                return;
            };
            let read = std::fs::read_to_string(&path).map_err(|e| e.to_string());
            match read.and_then(|text| Reaction::from_json(&text)) {
                Ok(reaction) => self.load(reaction, &format!("Opened: {}", path.display())),
                Err(e) => self.status = format!("Could not open {}: {e}", path.display()),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = "Opening files is not available in the browser build.".to_string();
        }
    }

    /// The structure currently on the canvas, or `None` (with a complaint in the status bar)
    /// when there is nothing drawn.
    fn canvas_structure(&mut self) -> Option<Molecule> {
        if self.canvas.molecule.atoms.is_empty() {
            self.status = "Nothing on the canvas — draw a structure first.".to_string();
            return None;
        }
        Some(self.canvas.molecule.clone())
    }

    /// Write the canvas straight into the scheme structure it is linked to (set by
    /// double-clicking a structure), so the table below follows the edit with no extra click.
    /// Only pushes when the molecule actually changed, and drops the link if the canvas is
    /// emptied or the linked slot is gone.
    fn sync_linked_structure(&mut self) {
        let Some(target) = self.linked_structure else {
            return;
        };
        // An emptied canvas breaks the link rather than wiping the linked structure.
        if self.canvas.molecule.atoms.is_empty() {
            self.linked_structure = None;
            self.status = "Canvas cleared — the structure is no longer linked.".to_string();
            return;
        }
        // The molecule currently in the linked slot, whichever kind of slot it is.
        let current = match target {
            StructureRef::Item(i) => match self.reaction.scheme.items.get(i) {
                Some(SchemeItem::Structure(m)) => Some(m.molecule()),
                _ => None,
            },
            StructureRef::ArrowLabel { index, part } => {
                scheme_arrow_structure(&self.reaction.scheme, index, part)
            }
        };
        let Some(current) = current else {
            // The linked slot was deleted or is no longer a structure: forget the link.
            self.linked_structure = None;
            return;
        };
        if *current != self.canvas.molecule {
            match target {
                // set_scheme_structure carries the edit into its analysis column, so the table's
                // formula, MW and stoichiometry redraw with the structure.
                StructureRef::Item(i) => {
                    self.reaction.set_scheme_structure(i, self.canvas.molecule.clone());
                }
                // An arrow-label structure feeds a reactant column too, so the syncing setter
                // carries the edit into it — the table follows the reagent live (課題C).
                StructureRef::ArrowLabel { index, part } => {
                    self.reaction.set_scheme_arrow_structure(
                        index,
                        part,
                        self.canvas.molecule.clone(),
                    );
                }
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("📂 Open").clicked() {
                self.open_reaction();
            }
            if ui.button("💾 Save").clicked() {
                self.save_reaction();
            }

            ui.separator();

            ui.label("Canvas:");
            for (tool, label) in [
                (Tool::Select, "⬚ Select"),
                (Tool::Bond, "✏ Bond"),
                (Tool::Eraser, "✖ Eraser"),
            ] {
                if ui
                    .selectable_label(self.canvas.tool == tool, label)
                    .clicked()
                {
                    self.canvas.tool = tool;
                }
            }
            if ui.button("🗑 Clear").clicked() {
                // A running cleanup would go on writing into the cleared canvas.
                self.canvas.cancel_cleanup();
                // An empty canvas has nothing to clear: an undo step there would only wipe
                // the redo history. Otherwise snapshot first so Ctrl+Z brings it back.
                if !self.canvas.molecule.atoms.is_empty() {
                    self.canvas.push_undo();
                    self.canvas.molecule = Molecule::default();
                    self.canvas.selected_atoms.clear();
                }
            }
            if ui.button("📋 Paste").clicked() {
                self.canvas.paste_clipboard();
            }

            ui.separator();

            ui.label("Scheme:");
            if ui.button("📂 Open CDXML/CDX…").clicked() {
                self.open_file();
            }
            if ui.button("💾 Save CDXML…").clicked() {
                self.save_file(ui.painter(), false);
            }
            if ui.button("💾 Save CDX…").clicked() {
                self.save_file(ui.painter(), true);
            }
            if ui.button("Analysis demo").clicked() {
                self.load(reference_reaction(), "The Analysis figure.");
            }
            if ui.button("Multi-step demo").clicked() {
                self.load(demo::multi_step_scheme(), "A four-step scheme on two rows.");
            }
            let style = &mut self.editor.view.style;
            let mut wrap = style.max_row_width.is_some();
            if ui.checkbox(&mut wrap, "Wrap rows at").changed() {
                style.max_row_width = wrap.then_some(700.0);
            }
            if let Some(width) = style.max_row_width.as_mut() {
                ui.add(egui::DragValue::new(width).range(100.0..=4000.0).suffix(" px"));
            }
        });

        ui.horizontal_wrapped(|ui| {
            ui.label("Add from canvas:");
            if ui.button("➕ To scheme").clicked()
                && let Some(mol) = self.canvas_structure()
            {
                self.reaction.scheme.add(SchemeItem::Structure(mol.into()));
                // The table is generated from the scheme: the new structure gets its column.
                self.reaction.sync_structures_from_scheme();
                self.status = "Added the structure to the scheme.".to_string();
            }

            ui.separator();

            ui.label("Insert into scheme:");
            if ui.button("➕ plus").clicked() {
                self.reaction.scheme.add(SchemeItem::Plus);
            }
            if ui.button("➕ arrow").clicked() {
                let i = self.reaction.scheme.add(SchemeItem::Arrow(Arrow::new()));
                // Structures past the new arrow become products.
                self.reaction.sync_structures_from_scheme();
                self.editor.begin_edit(
                    EditTarget::SchemeText { index: i, part: SchemePart::ArrowAbove },
                    &self.reaction,
                );
                self.status = "Added an arrow — type its reagents, e.g. K_2CO_3.".to_string();
            }
            if ui.button("➕ text").clicked() {
                let i = self.reaction.scheme.add(SchemeItem::Text("text".into()));
                self.editor
                    .begin_edit(EditTarget::SchemeText { index: i, part: SchemePart::Text }, &self.reaction);
            }
            if ui.button("➕ line break").clicked() {
                self.reaction.scheme.add(SchemeItem::LineBreak);
                self.status = "Added a line break — what follows goes on a new row.".to_string();
            }

            ui.separator();

            let selected = self.editor.selection();
            let structure_selected =
                matches!(selected, Some(Hit::Scheme { part: SchemePart::Structure, .. }));
            if ui
                .add_enabled(structure_selected, egui::Button::new("⟳ Replace selected structure"))
                .on_disabled_hover_text("Select a structure in the scheme first")
                .clicked()
                && let (Some(Hit::Scheme { index, .. }), Some(mol)) =
                    (selected, self.canvas_structure())
            {
                // The syncing setter carries the new structure into its analysis column, so
                // the table's formula, MW and stoichiometry redraw with the scheme.
                self.status = if self.reaction.set_scheme_structure(index, mol) {
                    "Replaced the selected structure — the table below followed it.".to_string()
                } else {
                    "The selection is not a structure on the reaction line — nothing replaced."
                        .to_string()
                };
            }
            if ui
                .add_enabled(structure_selected, egui::Button::new("🧹 Clean up selected"))
                .on_disabled_hover_text("Select a structure in the scheme first")
                .clicked()
                && let Some(i) = self.editor.cleanup_selected_structure(&mut self.reaction)
            {
                self.status = format!("Cleaned up the layout of scheme structure {i}.");
            }

            ui.separator();

            // Put the canvas structure over (or under) the selected arrow, so the drawn figure
            // can show a reagent as its structure rather than a name.
            let arrow_selected = matches!(
                selected,
                Some(Hit::Scheme { part: SchemePart::ArrowAbove | SchemePart::ArrowBelow, .. })
            );
            ui.label("On selected arrow:");
            if ui
                .add_enabled(arrow_selected, egui::Button::new("▲ structure above"))
                .on_disabled_hover_text("Select an arrow (above/below) in the scheme first")
                .clicked()
                && let Some(Hit::Scheme { index, .. }) = selected
            {
                self.apply(Action::PlaceArrowStructure(index, SchemePart::ArrowAbove));
            }
            if ui
                .add_enabled(arrow_selected, egui::Button::new("▼ structure below"))
                .on_disabled_hover_text("Select an arrow (above/below) in the scheme first")
                .clicked()
                && let Some(Hit::Scheme { index, .. }) = selected
            {
                self.apply(Action::PlaceArrowStructure(index, SchemePart::ArrowBelow));
            }
        });
    }

    /// The figure's own right-click menu: delete, reorder, replace, clean up and "edit in
    /// canvas", reachable straight from the drawing (課題D).
    fn scheme_context_menu(&mut self, response: egui::Response) {
        let target = self.editor.context_target();
        let mut action = None;
        response.context_menu(|ui| {
            let Some(Hit::Scheme { index, part }) = target else {
                ui.label("Right-click a scheme item for actions");
                return;
            };
            let is_structure =
                matches!(self.reaction.scheme.items.get(index), Some(SchemeItem::Structure(_)));
            if is_structure {
                if ui.button("✎ Edit in canvas").clicked() {
                    action = Some(Action::EditSchemeStructureInCanvas(StructureRef::Item(index)));
                    ui.close();
                }
                if ui.button("🧹 Clean up layout").clicked() {
                    action = Some(Action::CleanupSchemeStructure(index));
                    ui.close();
                }
                if ui.button("⟳ Replace with canvas").clicked() {
                    action = Some(Action::ReplaceSchemeStructure(index));
                    ui.close();
                }
                if ui.button("✎ Edit label").clicked() {
                    action = Some(Action::EditSchemeText(index, SchemePart::CompoundLabel));
                    ui.close();
                }
                ui.separator();
            }
            match part {
                // An arrow band either holds a structure (edit it on the canvas, or replace it)
                // or is a text label (edit its markup); either way, the canvas structure can be
                // dropped onto it.
                SchemePart::ArrowAbove | SchemePart::ArrowBelow => {
                    let has_structure =
                        scheme_arrow_structure(&self.reaction.scheme, index, part).is_some();
                    if has_structure {
                        if ui.button("✎ Edit structure in canvas").clicked() {
                            action = Some(Action::EditSchemeStructureInCanvas(
                                StructureRef::ArrowLabel { index, part },
                            ));
                            ui.close();
                        }
                    } else if ui.button("✎ Edit text").clicked() {
                        action = Some(Action::EditSchemeText(index, part));
                        ui.close();
                    }
                    let verb = if has_structure { "⟳ Replace with canvas" } else { "＋ Structure from canvas" };
                    if ui.button(verb).clicked() {
                        action = Some(Action::PlaceArrowStructure(index, part));
                        ui.close();
                    }
                    ui.separator();
                }
                SchemePart::Text => {
                    if ui.button("✎ Edit text").clicked() {
                        action = Some(Action::EditSchemeText(index, part));
                        ui.close();
                    }
                    ui.separator();
                }
                _ => {}
            }
            if ui.button("◀ Move left").clicked() {
                action = Some(Action::MoveSchemeItem(index, -1));
                ui.close();
            }
            if ui.button("▶ Move right").clicked() {
                action = Some(Action::MoveSchemeItem(index, 1));
                ui.close();
            }
            if ui.button("🗑 Delete").clicked() {
                action = Some(Action::RemoveSchemeItem(index));
                ui.close();
            }
        });
        if let Some(action) = action {
            self.editor.clear_context_target();
            self.apply(action);
        }
    }

    fn apply(&mut self, action: Action) {
        // Any selection or open box may be pointing at an index that is about to move.
        self.editor.select(None);
        self.editor.cancel_edit();
        // Drop the live edit link too — a delete/reorder can invalidate its index. The
        // EditSchemeStructureInCanvas arm re-establishes it for the structure it opens.
        self.linked_structure = None;
        match action {
            Action::RemoveSchemeItem(i) => {
                self.reaction.scheme.remove(i);
                // The removed structure (or the arrow's reagents) takes its column with it.
                self.reaction.sync_structures_from_scheme();
                self.status = "Removed a scheme item.".to_string();
            }
            Action::MoveSchemeItem(i, delta) => {
                self.reaction.scheme.move_item(i, delta);
                // Keep the table's columns aligned with the reordered structures.
                self.reaction.sync_structures_from_scheme();
            }
            Action::ReplaceSchemeStructure(i) => {
                if let Some(mol) = self.canvas_structure() {
                    // Redraw the structure and carry it into its table column in one step.
                    self.status = if self.reaction.set_scheme_structure(i, mol) {
                        "Replaced a scheme structure — the table column followed it.".to_string()
                    } else {
                        "That item is not a structure on the reaction line — nothing replaced."
                            .to_string()
                    };
                }
            }
            Action::CleanupSchemeStructure(i) => {
                if cleanup_scheme_structure(&mut self.reaction.scheme, i) {
                    self.editor.select(Some(Hit::Scheme { index: i, part: SchemePart::Structure }));
                    self.status = "Cleaned up the structure's layout.".to_string();
                }
            }
            Action::EditSchemeStructureInCanvas(target) => {
                // Read the molecule from whichever kind of slot the request names.
                let (mol, sel) = match target {
                    StructureRef::Item(i) => match self.reaction.scheme.items.get(i) {
                        Some(SchemeItem::Structure(m)) => (
                            Some(m.molecule().clone()),
                            Hit::Scheme { index: i, part: SchemePart::Structure },
                        ),
                        _ => (None, Hit::Scheme { index: i, part: SchemePart::Structure }),
                    },
                    StructureRef::ArrowLabel { index, part } => (
                        scheme_arrow_structure(&self.reaction.scheme, index, part).cloned(),
                        Hit::Scheme { index, part },
                    ),
                };
                if let Some(mol) = mol {
                    // A running cleanup would go on writing into the loaded structure.
                    self.canvas.cancel_cleanup();
                    self.canvas.push_undo();
                    self.canvas.molecule = mol;
                    self.canvas.selected_atoms.clear();
                    self.canvas.tool = Tool::Select;
                    // Link this slot to the canvas: every edit now flows back into it (and the
                    // table, for a main-line structure) automatically, no Replace click needed.
                    self.linked_structure = Some(target);
                    self.editor.select(Some(sel));
                    self.status =
                        "Editing this structure on the canvas — your changes update the figure live."
                            .to_string();
                }
            }
            Action::PlaceArrowStructure(index, part) => {
                if let Some(mol) = self.canvas_structure() {
                    // The syncing setter also re-derives the reagent's table column (課題C).
                    if self.reaction.set_scheme_arrow_structure(index, part, mol) {
                        self.editor.select(Some(Hit::Scheme { index, part }));
                        let where_ = if part == SchemePart::ArrowAbove { "above" } else { "below" };
                        self.status =
                            format!("Placed the canvas structure {where_} the arrow.");
                    } else {
                        self.status = "That scheme item is not an arrow.".to_string();
                    }
                }
            }
            Action::EditSchemeText(index, part) => {
                self.editor
                    .begin_edit(EditTarget::SchemeText { index, part }, &self.reaction);
                self.editor.select(Some(Hit::Scheme { index, part }));
            }
        }
    }
}

/// What the current selection is, for the status bar — the figure has no other way to say
/// which of its cells a click landed on.
fn describe(hit: Hit, reaction: &Reaction) -> String {
    use chembuider_rs::reaction::{PRODUCT_ROWS, REACTANT_ROWS};
    match hit {
        Hit::Cell(cell) => {
            let (block, rows) = match cell.block {
                chembuider_rs::reaction::BlockKind::Reactants => ("Reactants", &REACTANT_ROWS),
                chembuider_rs::reaction::BlockKind::Products => ("Products", &PRODUCT_ROWS),
            };
            let row = rows.get(cell.row).copied().unwrap_or("?");
            let editable = if cell.field().is_some() { "" } else { " (computed)" };
            format!("{block} column {} — {row}{editable}", cell.column + 1)
        }
        Hit::Scheme { index, part } => {
            let what = reaction
                .scheme
                .items
                .get(index)
                .map(item_label)
                .unwrap_or_default();
            match part {
                SchemePart::ArrowAbove => format!("scheme item {index} — arrow, above"),
                SchemePart::ArrowBelow => format!("scheme item {index} — arrow, below"),
                SchemePart::CompoundLabel => format!("scheme item {index} — {what}, label"),
                _ => format!("scheme item {index} — {what}"),
            }
        }
    }
}

/// A scheme item in a few plain words, for the status bar. Plain words rather than symbols:
/// egui's default font stack has no glyph for `⬡` or `→`, and a missing-glyph box names nothing.
fn item_label(item: &SchemeItem) -> String {
    match item {
        SchemeItem::Structure(m) => MolecularFormula::of(m).plain(),
        SchemeItem::Plus => "plus".to_string(),
        SchemeItem::Arrow(_) => "arrow".to_string(),
        SchemeItem::Text(t) => {
            let flat: String = t.spans.iter().map(|s| s.text.as_str()).collect();
            format!("text \"{flat}\"")
        }
        SchemeItem::LineBreak => "line break".to_string(),
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

    /// Write the scheme as CDXML (or CDX, by the extension) without a window.
    pub fn save(path: &str, reaction: &Reaction) {
        let ctx = egui::Context::default();
        let mut view = ReactionView::new();
        let mut bytes = Vec::new();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 900.0))),
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| {
            bytes = if path.ends_with(".cdx") {
                view.scheme_cdx(ui.painter(), &reaction.scheme)
            } else {
                view.scheme_cdxml(ui.painter(), &reaction.scheme).into_bytes()
            };
        });
        std::fs::write(path, bytes).expect("write the scheme");
        println!("wrote {path}");
    }

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
