//! The data model a caller builds: reagents with their entered quantities, plus the
//! reaction scheme to draw above the table.
//!
//! Everything here is plain data — no egui, no interaction. Values the *user* supplies are
//! `Option`s: whatever is left `None` is either computed from the rest (see
//! [`Analysis::compute`](crate::reaction::stoich)) or drawn as an empty cell.

use crate::molecule::Molecule;

use super::edit::SchemePart;
use super::scheme::{ReactionScheme, SchemeItem};

/// A mass. Stored in grams; the constructors say which unit the caller had in mind.
///
/// Display picks the unit back automatically (`< 1 g` prints as mg), which is what the
/// reference figure does: an entered `700.00 mg` prints as `700.00mg`, a computed `1.1491 g`
/// prints as `1.15g`.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct Mass(f64);

impl Mass {
    pub const fn grams(g: f64) -> Self {
        Self(g)
    }
    pub const fn milligrams(mg: f64) -> Self {
        Self(mg / 1000.0)
    }
    pub const fn kilograms(kg: f64) -> Self {
        Self(kg * 1000.0)
    }
    pub const fn as_grams(self) -> f64 {
        self.0
    }
    pub const fn as_milligrams(self) -> f64 {
        self.0 * 1000.0
    }
}

/// One reactant column of the *Reactants* block.
///
/// The molecule is the source of truth for `Formula` and `MW`; everything else is either
/// entered here or derived from the limiting reagent.
#[derive(Clone, Debug)]
pub struct Reactant {
    /// Structure the formula and molecular weight are computed from.
    pub molecule: Molecule,
    /// Optional display name (unused by the table, handy for callers keeping their own UI).
    pub name: Option<String>,
    /// `Limiting? = Yes`. Exactly one reactant should carry it; the others' amounts are
    /// derived from this one.
    pub limiting: bool,
    /// Molar equivalents relative to the limiting reagent.
    pub equivalents: Option<f64>,
    /// Mass actually weighed out. Left `None` for a reagent measured by equivalents — the
    /// table then shows the mass those equivalents call for, as a computed value.
    pub sample_mass: Option<Mass>,
    /// Assay/purity of the sample, in percent (a 60 % NaH dispersion → `60.0`). Scales the
    /// sample mass when converting to moles. Absent means 100 %.
    pub percent_weight: Option<f64>,
    /// Concentration of a stock solution, mol/L.
    pub molarity: Option<f64>,
    /// Density of a neat liquid, g/mL.
    pub density: Option<f64>,
    /// Volume in mL. Entered for a solution/liquid, otherwise computed from density or
    /// molarity when one of those is known.
    pub volume: Option<f64>,
    /// Stoichiometric coefficient in the balanced equation (default 1).
    pub coefficient: f64,
}

impl Reactant {
    pub fn new(molecule: Molecule) -> Self {
        Self {
            molecule,
            name: None,
            limiting: false,
            equivalents: None,
            sample_mass: None,
            percent_weight: None,
            molarity: None,
            density: None,
            volume: None,
            coefficient: 1.0,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    /// Mark as the limiting reagent (`Limiting? = Yes`).
    pub fn limiting(mut self) -> Self {
        self.limiting = true;
        self
    }
    pub fn equivalents(mut self, eq: f64) -> Self {
        self.equivalents = Some(eq);
        self
    }
    pub fn sample_mass(mut self, mass: Mass) -> Self {
        self.sample_mass = Some(mass);
        self
    }
    pub fn percent_weight(mut self, percent: f64) -> Self {
        self.percent_weight = Some(percent);
        self
    }
    /// Stock concentration in mol/L.
    pub fn molarity(mut self, molarity: f64) -> Self {
        self.molarity = Some(molarity);
        self
    }
    /// Density in g/mL.
    pub fn density(mut self, density: f64) -> Self {
        self.density = Some(density);
        self
    }
    /// Volume in mL.
    pub fn volume(mut self, ml: f64) -> Self {
        self.volume = Some(ml);
        self
    }
    /// Stoichiometric coefficient in the balanced equation.
    pub fn coefficient(mut self, n: f64) -> Self {
        self.coefficient = n;
        self
    }
}

/// One product column of the *Products* block.
#[derive(Clone, Debug)]
pub struct Product {
    /// Structure the formula and molecular weight are computed from.
    pub molecule: Molecule,
    pub name: Option<String>,
    /// Stoichiometric coefficient: moles of product per mole of limiting reagent.
    pub coefficient: f64,
    /// Molar equivalents (informational; left empty in the reference figure).
    pub equivalents: Option<f64>,
    /// Reaction conversion in percent; scales the expected amounts. Absent means 100 %.
    pub percent_completion: Option<f64>,
    /// Mass of product actually isolated.
    pub measured_mass: Option<Mass>,
    /// Purity of the isolated material, in percent. Absent means 100 %.
    pub purity: Option<f64>,
}

impl Product {
    pub fn new(molecule: Molecule) -> Self {
        Self {
            molecule,
            name: None,
            coefficient: 1.0,
            equivalents: None,
            percent_completion: None,
            measured_mass: None,
            purity: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    pub fn coefficient(mut self, n: f64) -> Self {
        self.coefficient = n;
        self
    }
    pub fn equivalents(mut self, eq: f64) -> Self {
        self.equivalents = Some(eq);
        self
    }
    pub fn percent_completion(mut self, percent: f64) -> Self {
        self.percent_completion = Some(percent);
        self
    }
    pub fn measured_mass(mut self, mass: Mass) -> Self {
        self.measured_mass = Some(mass);
        self
    }
    pub fn purity(mut self, percent: f64) -> Self {
        self.purity = Some(percent);
        self
    }
}

/// The inputs of the stoichiometry table: reactants and products, in column order.
#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub reactants: Vec<Reactant>,
    pub products: Vec<Product>,
}

impl Analysis {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reactant(mut self, r: Reactant) -> Self {
        self.reactants.push(r);
        self
    }

    pub fn product(mut self, p: Product) -> Self {
        self.products.push(p);
        self
    }

    // ── Mutation (the editing UI drives these) ───────────────────────────────

    /// Append a reactant column; returns its index.
    pub fn add_reactant(&mut self, r: Reactant) -> usize {
        self.reactants.push(r);
        self.reactants.len() - 1
    }

    /// Append a product column; returns its index.
    pub fn add_product(&mut self, p: Product) -> usize {
        self.products.push(p);
        self.products.len() - 1
    }

    /// Make `index` *the* limiting reagent, clearing the flag on every other column — a
    /// reaction has at most one, and the whole table is scaled from it.
    ///
    /// `None` leaves the reaction without a declared limiting reagent. That is a legal
    /// state, not an error: [`compute`](Analysis::compute) then falls back on the first
    /// reagent whose own entries pin an amount down (a weighed sample, or a stock
    /// concentration × volume), and if none does, every derived amount is simply blank.
    pub fn set_limiting(&mut self, index: Option<usize>) {
        for (i, r) in self.reactants.iter_mut().enumerate() {
            r.limiting = Some(i) == index;
        }
    }

    /// Index of the declared limiting reagent, if there is one.
    pub fn limiting(&self) -> Option<usize> {
        self.reactants.iter().position(|r| r.limiting)
    }

    /// Drop a reactant column, returning it. Out-of-range indices are ignored.
    ///
    /// Deleting the limiting reagent leaves the reaction *without* one rather than promoting
    /// a neighbour. A promoted neighbour would carry no amount of its own — it was entered
    /// in equivalents — so the numbers would not actually come back, they would only look as
    /// if they had, silently rescaled to nothing. Blank derived cells until the user names a
    /// new limiting reagent and gives it a mass is the honest state to show.
    pub fn remove_reactant(&mut self, index: usize) -> Option<Reactant> {
        (index < self.reactants.len()).then(|| self.reactants.remove(index))
    }

    /// Drop a product column, returning it. Out-of-range indices are ignored.
    pub fn remove_product(&mut self, index: usize) -> Option<Product> {
        (index < self.products.len()).then(|| self.products.remove(index))
    }
}

/// The role a drawn scheme structure plays in the analysis, decided by where it sits
/// relative to the reaction arrow.
///
/// Structures to the left of the first arrow are reactants, those to its right are products;
/// the ordinal counts structures of that role from the left, so it lines up with the column
/// order of the matching [`Analysis`] block. A scheme with no arrow treats every structure as
/// a reactant.
///
/// Structures riding on an arrow's above/below label are reagents; they too map to reactant
/// columns, numbered *after* the main-line reactant structures — see
/// [`Reaction::arrow_structure_role`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructureRole {
    /// The n-th reactant structure of the scheme — matches `analysis.reactants[n]`.
    Reactant(usize),
    /// The n-th product structure of the scheme — matches `analysis.products[n]`.
    Product(usize),
}

/// A reaction scheme plus its stoichiometry table — the whole figure.
///
/// The two halves are deliberately independent: the scheme in the reference figure draws
/// MeI and K₂CO₃ as text over and under the arrow, while the table still gives them a full
/// column each.
///
/// The drawn structures are none the less the source of truth for the columns they *do*
/// stand for: editing a structure in the scheme is meant to move its formula, weight and the
/// whole stoichiometry with it. That link is positional — see [`structure_role`] — and the
/// [`set_scheme_structure`] / [`sync_structures_from_scheme`] methods carry the change from
/// the scheme into the matching [`Analysis`] column so the table redraws in step.
///
/// [`structure_role`]: Reaction::structure_role
/// [`set_scheme_structure`]: Reaction::set_scheme_structure
/// [`sync_structures_from_scheme`]: Reaction::sync_structures_from_scheme
#[derive(Clone, Debug, Default)]
pub struct Reaction {
    pub scheme: ReactionScheme,
    pub analysis: Analysis,
}

impl Reaction {
    pub fn new(scheme: ReactionScheme, analysis: Analysis) -> Self {
        Self { scheme, analysis }
    }

    /// Which analysis column the structure drawn at scheme item `scheme_index` stands for, or
    /// `None` when that item is not a structure (an arrow, a `+`, free text, or out of range).
    ///
    /// Structures before the first arrow are reactants, those after it products; the ordinal
    /// counts structures of that role left to right, so it indexes straight into
    /// [`Analysis::reactants`] or [`Analysis::products`].
    pub fn structure_role(&self, scheme_index: usize) -> Option<StructureRole> {
        if !matches!(self.scheme.items.get(scheme_index), Some(SchemeItem::Structure(_))) {
            return None;
        }
        let mut seen_arrow = false;
        let mut reactants = 0usize;
        let mut products = 0usize;
        for (i, item) in self.scheme.items.iter().enumerate() {
            match item {
                SchemeItem::Arrow(_) => seen_arrow = true,
                SchemeItem::Structure(_) => {
                    let role = if seen_arrow {
                        StructureRole::Product(products)
                    } else {
                        StructureRole::Reactant(reactants)
                    };
                    if i == scheme_index {
                        return Some(role);
                    }
                    if seen_arrow {
                        products += 1;
                    } else {
                        reactants += 1;
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Copy the molecule drawn at scheme item `scheme_index` into the analysis column that
    /// plays the same [role](Reaction::structure_role), so the table's formula, MW and
    /// stoichiometry follow the drawn structure.
    ///
    /// Returns `true` when a matching column existed and was updated. `false` means the item
    /// is not a structure, or the scheme draws more structures of that role than the analysis
    /// has columns for (the extra ones stand for nothing in the table).
    pub fn sync_structure_from_scheme(&mut self, scheme_index: usize) -> bool {
        let Some(role) = self.structure_role(scheme_index) else {
            return false;
        };
        let Some(SchemeItem::Structure(molecule)) = self.scheme.items.get(scheme_index) else {
            return false;
        };
        let molecule = molecule.clone();
        match role {
            StructureRole::Reactant(n) => match self.analysis.reactants.get_mut(n) {
                Some(r) => {
                    r.molecule = molecule;
                    true
                }
                None => false,
            },
            StructureRole::Product(n) => match self.analysis.products.get_mut(n) {
                Some(p) => {
                    p.molecule = molecule;
                    true
                }
                None => false,
            },
        }
    }

    /// Replace the structure drawn at scheme item `scheme_index`, carrying the change into the
    /// matching analysis column so the table follows. Returns `false` (and changes nothing)
    /// when the item is not a structure.
    ///
    /// This is the structure-editing counterpart of typing into a table cell: both leave the
    /// figure re-derivable from what is drawn.
    pub fn set_scheme_structure(&mut self, scheme_index: usize, molecule: Molecule) -> bool {
        match self.scheme.items.get_mut(scheme_index) {
            Some(item) if matches!(item, SchemeItem::Structure(_)) => {
                *item = SchemeItem::Structure(molecule);
            }
            _ => return false,
        }
        self.sync_structure_from_scheme(scheme_index);
        true
    }

    /// Re-derive every analysis column that a scheme structure stands for, from the structure
    /// currently drawn there — main-line structures *and* structures riding on arrow labels.
    /// Use after a bulk change or a reorder, when individual
    /// [`sync_structure_from_scheme`](Reaction::sync_structure_from_scheme) calls were not made.
    pub fn sync_structures_from_scheme(&mut self) {
        for i in 0..self.scheme.items.len() {
            self.sync_structure_from_scheme(i);
        }
        // Arrow-label structures feed reactant columns appended after the main-line ones; a
        // second pass keeps their formula/MW/stoichiometry in step too.
        for i in 0..self.scheme.items.len() {
            self.sync_arrow_structure_from_scheme(i, SchemePart::ArrowAbove);
            self.sync_arrow_structure_from_scheme(i, SchemePart::ArrowBelow);
        }
    }

    // ── Arrow-label structures ↔ reactant columns ────────────────────────────

    /// How many main-line structures sit before the first arrow — the reactant structures whose
    /// columns come first. Arrow-label reagent columns are numbered *after* these.
    fn main_line_reactant_count(&self) -> usize {
        let mut count = 0;
        for item in &self.scheme.items {
            match item {
                SchemeItem::Arrow(_) => break,
                SchemeItem::Structure(_) => count += 1,
                _ => {}
            }
        }
        count
    }

    /// Which analysis column the structure on an arrow's above/below label stands for.
    ///
    /// Arrow-label structures are reagents, so they map to **reactant** columns, appended after
    /// the main-line reactant structures ([`main_line_reactant_count`](Reaction::main_line_reactant_count)):
    /// the k-th arrow-label structure — counted above-before-below, in scheme order, over every
    /// arrow — is `Reactant(main_line_reactant_count + k)`. A band carrying nothing, or text,
    /// holds no column and returns `None`, so empty/text labels never consume a column ordinal.
    ///
    /// This keeps the positional main-line mapping untouched: a scheme with no arrow-label
    /// structures numbers its reactants exactly as before.
    pub fn arrow_structure_role(&self, index: usize, part: SchemePart) -> Option<StructureRole> {
        let base = self.main_line_reactant_count();
        let mut k = 0usize;
        for (i, item) in self.scheme.items.iter().enumerate() {
            let SchemeItem::Arrow(arrow) = item else { continue };
            for (p, label) in [
                (SchemePart::ArrowAbove, &arrow.above),
                (SchemePart::ArrowBelow, &arrow.below),
            ] {
                if label.as_structure().is_some() {
                    if i == index && p == part {
                        return Some(StructureRole::Reactant(base + k));
                    }
                    k += 1;
                }
            }
        }
        None
    }

    /// Copy the structure on an arrow band into the reactant column it
    /// [stands for](Reaction::arrow_structure_role), so the table follows the drawn reagent.
    ///
    /// Returns `true` when a matching column existed and was updated. `false` means the band
    /// carries no structure, or the analysis has no column for it (the arrow reagent then
    /// stands for nothing in the table, exactly as a surplus main-line structure does).
    pub fn sync_arrow_structure_from_scheme(&mut self, index: usize, part: SchemePart) -> bool {
        let Some(StructureRole::Reactant(col)) = self.arrow_structure_role(index, part) else {
            return false;
        };
        let Some(SchemeItem::Arrow(arrow)) = self.scheme.items.get(index) else {
            return false;
        };
        let label = match part {
            SchemePart::ArrowAbove => &arrow.above,
            SchemePart::ArrowBelow => &arrow.below,
            _ => return false,
        };
        let Some(molecule) = label.as_structure().cloned() else {
            return false;
        };
        match self.analysis.reactants.get_mut(col) {
            Some(r) => {
                r.molecule = molecule;
                true
            }
            None => false,
        }
    }

    /// Put a structure on an arrow's above/below band and carry it into the reactant column it
    /// stands for, so the table's formula, MW and stoichiometry follow — the arrow-label
    /// counterpart of [`set_scheme_structure`](Reaction::set_scheme_structure).
    ///
    /// Returns `false` (changing nothing) when the item is not an arrow or the part is not a
    /// band. Placing a structure can shift the ordinal of later arrow reagents, so this does a
    /// full [`sync_structures_from_scheme`](Reaction::sync_structures_from_scheme).
    pub fn set_scheme_arrow_structure(
        &mut self,
        index: usize,
        part: SchemePart,
        molecule: Molecule,
    ) -> bool {
        if !super::edit::set_scheme_arrow_structure(&mut self.scheme, index, part, molecule) {
            return false;
        }
        self.sync_structures_from_scheme();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;
    use crate::reaction::scheme::Arrow;

    #[test]
    fn mass_units_round_trip() {
        assert_eq!(Mass::milligrams(700.0).as_grams(), 0.7);
        assert_eq!(Mass::grams(1.0).as_milligrams(), 1000.0);
        assert_eq!(Mass::kilograms(0.5).as_grams(), 500.0);
    }

    /// A scheme structure is a reactant before the arrow and a product after it; the ordinal
    /// counts structures of that role, so it indexes straight into the analysis columns.
    #[test]
    fn structure_roles_split_at_the_arrow() {
        let reaction = demo::williamson_ether_synthesis();
        assert_eq!(reaction.structure_role(0), Some(StructureRole::Reactant(0)));
        assert_eq!(reaction.structure_role(1), None, "the arrow is not a structure");
        assert_eq!(reaction.structure_role(2), Some(StructureRole::Product(0)));
        assert_eq!(reaction.structure_role(9), None, "out of range");

        // Two reagents drawn before the arrow, two products after it: the ordinals run in
        // drawn order within each block.
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(Arrow::new())
            .structure(demo::anisole())
            .plus()
            .structure(demo::potassium_carbonate());
        let reaction = Reaction::new(scheme, Analysis::new());
        assert_eq!(reaction.structure_role(0), Some(StructureRole::Reactant(0)));
        assert_eq!(reaction.structure_role(2), Some(StructureRole::Reactant(1)));
        assert_eq!(reaction.structure_role(4), Some(StructureRole::Product(0)));
        assert_eq!(reaction.structure_role(6), Some(StructureRole::Product(1)));
    }

    /// Replacing a drawn scheme structure carries its formula, weight and stoichiometry into
    /// the matching analysis column, so the table below the scheme follows.
    #[test]
    fn editing_a_scheme_structure_updates_the_table() {
        let mut reaction = demo::williamson_ether_synthesis();

        // Baseline: the drawn phenol is reactant column 0, anisole is product column 0.
        let before = reaction.analysis.compute();
        assert_eq!(before.reactants[0].formula.plain(), "C6H6O");
        assert!((before.reactants[0].mw.unwrap() - 94.11).abs() < 0.01);
        assert_eq!(before.products[0].formula.plain(), "C7H8O");

        // Redraw the reactant (scheme item 0) as anisole: the table's reactant column follows,
        // formula and MW re-derived, and the whole column's stoichiometry with it.
        assert!(reaction.set_scheme_structure(0, demo::anisole()));
        let after = reaction.analysis.compute();
        assert_eq!(after.reactants[0].formula.plain(), "C7H8O", "formula followed the drawing");
        assert!((after.reactants[0].mw.unwrap() - 108.14).abs() < 0.01, "MW re-derived");
        // 1.00 g of MW 108.14 is 9.25 mmol, down from phenol's 10.63 mmol — the scale moved.
        assert!((after.reactants[0].moles.unwrap().value - 1.0 / 108.14).abs() < 1e-6);

        // Redraw the product (scheme item 2) as phenol: the product column follows too.
        assert!(reaction.set_scheme_structure(2, demo::phenol()));
        assert_eq!(
            reaction.analysis.compute().products[0].formula.plain(),
            "C6H6O",
            "the product column followed the drawn structure"
        );
    }

    /// A structure the table has no column for, and a non-structure item, are left alone.
    #[test]
    fn syncing_a_structure_without_a_column_is_a_no_op() {
        // A scheme structure but an empty analysis: nothing to sync into.
        let mut reaction = Reaction::new(
            ReactionScheme::new().structure(demo::phenol()),
            Analysis::new(),
        );
        assert!(!reaction.sync_structure_from_scheme(0), "no reactant column exists");

        // Replacing the arrow's slot (not a structure) does nothing and reports it.
        let mut reaction = demo::williamson_ether_synthesis();
        assert!(!reaction.set_scheme_structure(1, demo::phenol()));
        let SchemeItem::Arrow(_) = &reaction.scheme.items[1] else {
            panic!("item 1 must still be the arrow, untouched");
        };
    }

    /// A bulk resync re-derives every column that a scheme structure stands for.
    #[test]
    fn bulk_sync_redraws_every_linked_column() {
        let mut reaction = demo::williamson_ether_synthesis();
        // Poke new structures straight into the scheme, bypassing the syncing setter …
        reaction.scheme.items[0] = SchemeItem::Structure(demo::anisole());
        reaction.scheme.items[2] = SchemeItem::Structure(demo::phenol());
        // … the table is stale until a resync.
        assert_eq!(reaction.analysis.compute().reactants[0].formula.plain(), "C6H6O");

        reaction.sync_structures_from_scheme();
        let a = reaction.analysis.compute();
        assert_eq!(a.reactants[0].formula.plain(), "C7H8O");
        assert_eq!(a.products[0].formula.plain(), "C6H6O");
        // The co-reagents that are only arrow text keep their own molecules.
        assert_eq!(a.reactants[1].formula.plain(), "CH3I");
        assert_eq!(a.reactants[2].formula.plain(), "CK2O3");
    }

    /// A structure riding on an arrow label feeds a reactant column, numbered after the
    /// main-line reactant structures; empty/text bands hold no column (課題C).
    #[test]
    fn arrow_label_structures_feed_reactant_columns() {
        // phenol + [biphenyl above / "cat." below the arrow] → anisole.
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new().above_structure(demo::biphenyl()).below("cat."))
            .structure(demo::anisole());
        // One main-line reactant (phenol), so the arrow reagent takes reactant column 1.
        let analysis = Analysis::new()
            .reactant(Reactant::new(demo::phenol()).limiting().sample_mass(Mass::grams(1.0)))
            .reactant(Reactant::new(demo::benzene()).equivalents(1.0)) // placeholder for the reagent
            .product(Product::new(demo::anisole()));
        let mut reaction = Reaction::new(scheme, analysis);

        // The biphenyl above the arrow resolves to reactant column 1…
        assert_eq!(
            reaction.arrow_structure_role(1, SchemePart::ArrowAbove),
            Some(StructureRole::Reactant(1))
        );
        // …and the text ("cat.") band below holds no column.
        assert_eq!(reaction.arrow_structure_role(1, SchemePart::ArrowBelow), None);
        // A non-arrow item has no arrow role either.
        assert_eq!(reaction.arrow_structure_role(0, SchemePart::ArrowAbove), None);

        // Syncing re-derives column 1 from the biphenyl drawn over the arrow.
        reaction.sync_structures_from_scheme();
        let a = reaction.analysis.compute();
        assert_eq!(a.reactants[0].formula.plain(), "C6H6O", "the main-line phenol keeps column 0");
        assert_eq!(a.reactants[1].formula.plain(), "C12H10", "biphenyl feeds column 1");
        assert!((a.reactants[1].mw.unwrap() - 154.21).abs() < 0.1, "MW re-derived from the reagent");

        // Changing the arrow structure follows into the column, entered stoichiometry kept.
        assert!(reaction.set_scheme_arrow_structure(1, SchemePart::ArrowAbove, demo::phenol()));
        let a = reaction.analysis.compute();
        assert_eq!(a.reactants[1].formula.plain(), "C6H6O", "the reagent column followed the redraw");
        assert_eq!(reaction.analysis.reactants[1].equivalents, Some(1.0), "its equivalents survived");

        // A non-arrow target refuses.
        assert!(!reaction.set_scheme_arrow_structure(0, SchemePart::ArrowAbove, demo::benzene()));
    }

    /// Two arrow reagents number in scheme order (above before below), so their columns are
    /// stable and distinct (課題C).
    #[test]
    fn arrow_reagents_number_in_scheme_order() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(
                Arrow::new()
                    .above_structure(demo::biphenyl())
                    .below_structure(demo::benzene()),
            )
            .structure(demo::anisole());
        let reaction = Reaction::new(scheme, Analysis::new());
        // Two main-line reactant structures (phenol, MeI), so arrow reagents start at column 2.
        assert_eq!(
            reaction.arrow_structure_role(3, SchemePart::ArrowAbove),
            Some(StructureRole::Reactant(2))
        );
        assert_eq!(
            reaction.arrow_structure_role(3, SchemePart::ArrowBelow),
            Some(StructureRole::Reactant(3))
        );
    }

    // ── Characterization (G-005) ─────────────────────────────────────────────
    //
    // The scheme ↔ table link is about to move from position to a stable structure ID. These
    // pin the parts of today's behaviour that the ID-based link keeps. They deliberately do
    // *not* pin which column an entered value stays in after a reorder or a removal: that is
    // exactly what changes (values will follow their compound instead of the column index).

    /// The entered numbers of every column, for comparing a table before and after an edit.
    #[derive(Debug, PartialEq)]
    struct EnteredValues {
        /// `(limiting, equivalents, sample_mass)` per reactant column.
        reactants: Vec<(bool, Option<f64>, Option<Mass>)>,
        /// `measured_mass` per product column.
        products: Vec<Option<Mass>>,
    }

    fn entered_values(a: &Analysis) -> EnteredValues {
        EnteredValues {
            reactants: a.reactants.iter().map(|r| (r.limiting, r.equivalents, r.sample_mass)).collect(),
            products: a.products.iter().map(|p| p.measured_mass).collect(),
        }
    }

    fn reactant_formulas(reaction: &Reaction) -> Vec<String> {
        reaction.analysis.compute().reactants.iter().map(|r| r.formula.plain()).collect()
    }

    fn product_formulas(reaction: &Reaction) -> Vec<String> {
        reaction.analysis.compute().products.iter().map(|p| p.formula.plain()).collect()
    }

    /// After a reorder and a resync, the linked columns read in scheme order, and the number of
    /// columns does not change.
    #[test]
    fn resync_after_a_reorder_follows_scheme_order() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(Arrow::new())
            .structure(demo::anisole());
        let analysis = Analysis::new()
            .reactant(Reactant::new(demo::phenol()).limiting().sample_mass(Mass::grams(1.0)))
            .reactant(Reactant::new(demo::methyl_iodide()).equivalents(1.1))
            .product(Product::new(demo::anisole()));
        let mut reaction = Reaction::new(scheme, analysis);

        // MeI to the front: [MeI, phenol, +, →, anisole].
        assert_eq!(reaction.scheme.move_item(2, -2), Some(0));
        reaction.sync_structures_from_scheme();

        assert_eq!(reactant_formulas(&reaction), ["CH3I", "C6H6O"], "table follows the scheme");
        assert_eq!(product_formulas(&reaction), ["C7H8O"]);
        assert_eq!(reaction.analysis.reactants.len(), 2);
        assert_eq!(reaction.analysis.products.len(), 1);
    }

    /// A resync of an unchanged figure changes nothing: no column moves, appears or loses a
    /// value, however often it runs.
    #[test]
    fn resync_of_an_unchanged_figure_is_a_no_op() {
        let mut reaction = demo::williamson_ether_synthesis();
        let formulas = (reactant_formulas(&reaction), product_formulas(&reaction));
        let values = entered_values(&reaction.analysis);

        reaction.sync_structures_from_scheme();
        reaction.sync_structures_from_scheme();

        assert_eq!((reactant_formulas(&reaction), product_formulas(&reaction)), formulas);
        assert_eq!(entered_values(&reaction.analysis), values);
    }

    /// Redrawing a structure in place keeps every entered value of its column, and leaves the
    /// columns no structure stands for (MeI, K₂CO₃ — arrow text in the demo) untouched.
    #[test]
    fn redrawing_keeps_entered_values_and_leaves_unlinked_columns_alone() {
        let mut reaction = demo::williamson_ether_synthesis();
        let values = entered_values(&reaction.analysis);

        assert!(reaction.set_scheme_structure(0, demo::anisole()));
        assert!(reaction.set_scheme_structure(2, demo::phenol()));
        reaction.sync_structures_from_scheme();

        assert_eq!(entered_values(&reaction.analysis), values, "limiting/masses/eq kept");
        assert_eq!(reactant_formulas(&reaction), ["C7H8O", "CH3I", "CK2O3"]);
        assert_eq!(product_formulas(&reaction), ["C6H6O"]);
    }

    /// Removing a drawn structure keeps its column — molecule and entered values — so nothing
    /// typed into the table is lost.
    #[test]
    fn removing_a_structure_keeps_its_column() {
        let mut reaction = demo::williamson_ether_synthesis();
        let values = entered_values(&reaction.analysis);

        // Drop the product (item 2), then the reactant (item 0), resyncing after each.
        assert!(reaction.scheme.remove(2).is_some());
        reaction.sync_structures_from_scheme();
        assert!(reaction.scheme.remove(0).is_some());
        reaction.sync_structures_from_scheme();

        assert_eq!(reactant_formulas(&reaction), ["C6H6O", "CH3I", "CK2O3"]);
        assert_eq!(product_formulas(&reaction), ["C7H8O"]);
        assert_eq!(entered_values(&reaction.analysis), values);
    }

    /// Adding a structure straight to the scheme does not create a column, and a resync does
    /// not either; existing columns are untouched.
    #[test]
    fn a_raw_scheme_add_creates_no_column() {
        let mut reaction = demo::williamson_ether_synthesis();
        let values = entered_values(&reaction.analysis);

        // A second product structure after the anisole, added through the raw scheme API.
        reaction.scheme.add(SchemeItem::Structure(demo::benzene()));
        reaction.sync_structures_from_scheme();

        assert_eq!(reaction.analysis.reactants.len(), 3);
        assert_eq!(reaction.analysis.products.len(), 1);
        assert_eq!(product_formulas(&reaction), ["C7H8O"]);
        assert_eq!(entered_values(&reaction.analysis), values);
    }
}
