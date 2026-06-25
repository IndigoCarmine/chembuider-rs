//! `chembuider` — an [egui](https://github.com/emilk/egui) widget for drawing and editing
//! 2D chemical structure diagrams.
//!
//! The core type is [`ChemStructEditor`]: an immediate-mode widget you hold in your app and
//! render each frame with [`ChemStructEditor::ui`]. It manages a [`Molecule`] and exposes
//! tools for placing atoms, drawing bonds, fragments, ring templates, stereo, cleanup, and
//! (on Windows) ChemDraw-compatible clipboard interop.
//!
//! ```no_run
//! use chembuider_rs::{egui, ChemStructEditor};
//! struct MyApp { editor: ChemStructEditor }
//! impl MyApp {
//!     fn ui(&mut self, ui: &mut egui::Ui) {
//!         self.editor.ui(ui); // draw the editor and process input for this frame
//!     }
//! }
//! ```
//!
//! See `examples/editor.rs` for a complete host application (toolbar, status bar, MOL2 export).

pub mod config;
pub mod molecule;
pub mod widget;

/// Windows clipboard plumbing (CDX / image / OLE), used internally by the widget.
#[cfg(windows)]
mod clipboard;

/// Re-export of the exact [`egui`] version this widget is built against, so host apps can
/// depend on a matching API without pinning the version themselves.
pub use egui;

pub use config::Config;
pub use molecule::{Atom, Bond, BondOrder, BondStereo, Molecule};
pub use widget::{ChemStructEditor, Tool};
