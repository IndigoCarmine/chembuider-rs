//! Reaction schemes and ChemDraw-style stoichiometry ("Analysis") tables.
//!
//! A [`Reaction`] is a [`ReactionScheme`] — structures, `+` separators and an arrow carrying
//! its conditions — plus an [`Analysis`], the reagents with the quantities the chemist
//! actually entered. [`ReactionView`] draws both: the scheme on top, the table underneath.
//!
//! Everything in the table is derived from the drawn structures. `Formula` and `MW` come
//! from counting the atoms the renderer displays (implicit hydrogens included, see
//! [`formula`]), and the amounts fall out of the limiting reagent (see [`stoich`]).
//!
//! A figure can be built from code, as below, or edited from the UI with
//! [`ReactionEditor`] — the same figure, with the entered cells and the arrow labels
//! clickable. See [`edit`] for what is editable and how typed quantities are read.
//!
//! ```no_run
//! use chembuider_rs::reaction::{
//!     Analysis, Arrow, Mass, Product, Reactant, Reaction, ReactionScheme, ReactionView, demo,
//! };
//!
//! let scheme = ReactionScheme::new()
//!     .structure(demo::phenol())
//!     .arrow(Arrow::new().above("MeI").below("K_2CO_3"))
//!     .structure(demo::anisole());
//!
//! let analysis = Analysis::new()
//!     .reactant(Reactant::new(demo::phenol()).limiting().equivalents(1.0)
//!                                            .sample_mass(Mass::grams(1.0)))
//!     .reactant(Reactant::new(demo::methyl_iodide()).equivalents(1.10))
//!     .reactant(Reactant::new(demo::potassium_carbonate()).equivalents(3.00))
//!     .product(Product::new(demo::anisole()).measured_mass(Mass::milligrams(700.0)));
//!
//! let reaction = Reaction::new(scheme, analysis);
//! let mut view = ReactionView::new(); // keep this between frames, it owns a drawing context
//! # let ui: &mut chembuider_rs::egui::Ui = unimplemented!();
//! view.show(ui, &reaction);
//! ```
//!
//! See `examples/reaction_scheme.rs` for a complete host application.

pub mod demo;
pub mod edit;
pub mod formula;
pub mod model;
pub mod scheme;
pub mod stoich;
pub mod style;
pub mod table;
pub mod text;
pub mod view;

pub use edit::{
    CellRef, EditTarget, Field, Hit, ParseError, ReactionEditor, SchemePart, StructureRef, Unit,
    arrow_slot_at, cell_rect, cleanup_scheme_structure, field_at, hit_test,
    move_arrow_structure_to_line, move_line_structure_to_arrow_slot, parse_value,
    place_scheme_arrow_structure, scheme_arrow_structure, scheme_drop_index, scheme_item_rect,
    set_scheme_arrow_structure, swap_line_structure_with_arrow_slot, take_scheme_arrow_structure,
};
pub use formula::{MolecularFormula, atomic_weight, formula_of, molecular_weight};
pub use model::{
    Analysis, FILE_FORMAT, FILE_VERSION, Mass, Product, Reactant, Reaction, StructureRole,
};
pub use scheme::{
    Arrow, ArrowLabel, ReactionScheme, SchemeItem, SchemeLayout, SchemeStructure, StructureId,
};
pub use stoich::{
    AnalysisResult, Formatted, Origin, ProductAnalysis, Quantity, ReactantAnalysis, format_mass,
    format_moles, format_number, format_percent,
};
pub use style::ReactionStyle;
pub use table::{AnalysisTable, BlockKind, Cell, PRODUCT_ROWS, REACTANT_ROWS, TableBlock};
pub use text::{Run, RunLayout};
pub use view::{FigureLayout, ReactionView};
