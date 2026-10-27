//! The data model a caller builds: reagents with their entered quantities, plus the
//! reaction scheme to draw above the table.
//!
//! Everything here is plain data — no egui, no interaction. Values the *user* supplies are
//! `Option`s: whatever is left `None` is either computed from the rest (see
//! [`Analysis::compute`](crate::reaction::stoich)) or drawn as an empty cell.

use crate::molecule::Molecule;

use super::edit::SchemePart;
use super::scheme::{ArrowLabel, ReactionScheme, SchemeItem, SchemeStructure, StructureId};

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
    /// The scheme structure this column stands for, or `None` when no drawn structure does
    /// (a reagent written as arrow text, say). [`Reaction::new`] fills it in for columns
    /// built without one.
    pub structure: Option<StructureId>,
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
            structure: None,
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
    /// The scheme structure this column stands for, or `None` when no drawn structure does.
    /// [`Reaction::new`] fills it in for columns built without one.
    pub structure: Option<StructureId>,
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
            structure: None,
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

/// The analysis column a drawn scheme structure is linked to: the reactant or product column
/// whose [`structure`](Reactant::structure) names the structure's [`StructureId`]. See
/// [`Reaction::structure_role`] and [`Reaction::arrow_structure_role`].
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
/// whole stoichiometry with it. That link is by id: a column whose
/// [`structure`](Reactant::structure) names a [`StructureId`] stands for the scheme structure
/// carrying that id, wherever it is drawn — see [`structure_role`]. Moving a structure along
/// the line or between the line and an arrow band keeps its id, so its column (and the
/// limiting flag, masses and equivalents entered there) stays with the compound. Only
/// [`Reaction::new`] links by position, once, for columns built without an id.
///
/// The [`set_scheme_structure`] / [`sync_structures_from_scheme`] methods carry the change
/// from the scheme into the linked [`Analysis`] column so the table redraws in step; the bulk
/// sync also unlinks columns whose structure left the scheme, turns a column whose structure
/// crossed the arrow into a column of the other kind (Reactant ↔ Product), and puts the linked
/// columns back in scheme order.
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
    /// Put a scheme and its table together, linking each column that has no
    /// [`structure`](Reactant::structure) yet to the scheme structure at its position.
    ///
    /// This is the only place the positional rule decides a link: the n-th reactant structure
    /// before the first arrow is reactant column n, the n-th structure after it product
    /// column n, and arrow-label structures take the reactant columns after the main-line
    /// ones (see [`structure_role`](Reaction::structure_role) and
    /// [`arrow_structure_role`](Reaction::arrow_structure_role)). A column that already
    /// names a structure is left as it is, and a structure some column already names is not
    /// linked a second time. Columns past the drawn structures (reagents written as arrow
    /// text) stay `None`.
    pub fn new(scheme: ReactionScheme, analysis: Analysis) -> Self {
        let mut reaction = Self { scheme, analysis };
        reaction.link_columns_by_position();
        reaction
    }

    /// A scheme of two or more steps (arrows). Its table is not drawn, and the columns are
    /// left as they are while it stays one: a column follows a compound across *the* arrow,
    /// and a multi-step scheme has no single arrow to split its structures at. Once it is back
    /// to one arrow, [`sync_structures_from_scheme`](Reaction::sync_structures_from_scheme)
    /// lines the table up with it again.
    pub fn is_multi_step(&self) -> bool {
        self.scheme.arrow_count() >= 2
    }

    /// Link every unlinked column to the structure at its position; see [`Reaction::new`].
    fn link_columns_by_position(&mut self) {
        if self.is_multi_step() {
            return;
        }
        let mut links: Vec<(StructureRole, StructureId)> = Vec::new();
        for (i, item) in self.scheme.items.iter().enumerate() {
            match item {
                SchemeItem::Structure(s) => {
                    if let Some(role) = self.positional_structure_role(i) {
                        links.push((role, s.id()));
                    }
                }
                SchemeItem::Arrow(arrow) => {
                    for (part, label) in [
                        (SchemePart::ArrowAbove, &arrow.above),
                        (SchemePart::ArrowBelow, &arrow.below),
                    ] {
                        if let ArrowLabel::Structure(s) = label
                            && let Some(role) = self.positional_arrow_structure_role(i, part)
                        {
                            links.push((role, s.id()));
                        }
                    }
                }
                _ => {}
            }
        }
        for (role, id) in links {
            let claimed = self.analysis.reactants.iter().any(|r| r.structure == Some(id))
                || self.analysis.products.iter().any(|p| p.structure == Some(id));
            if claimed {
                continue;
            }
            let slot = match role {
                StructureRole::Reactant(n) => {
                    self.analysis.reactants.get_mut(n).map(|r| &mut r.structure)
                }
                StructureRole::Product(n) => {
                    self.analysis.products.get_mut(n).map(|p| &mut p.structure)
                }
            };
            if let Some(slot @ None) = slot {
                *slot = Some(id);
            }
        }
    }

    /// Which analysis column the structure drawn at scheme item `scheme_index` stands for: the
    /// column linked to its [`StructureId`] (see [`Reactant::structure`]), as an index into
    /// [`Analysis::reactants`] or [`Analysis::products`].
    ///
    /// `None` when the item is not a structure (an arrow, a `+`, free text, or out of range),
    /// or when no column is linked to it (a structure added straight to the scheme, say).
    pub fn structure_role(&self, scheme_index: usize) -> Option<StructureRole> {
        match self.scheme.items.get(scheme_index) {
            Some(SchemeItem::Structure(s)) => self.column_linked_to(s.id()),
            _ => None,
        }
    }

    /// The column whose `structure` names `id`, if any.
    fn column_linked_to(&self, id: StructureId) -> Option<StructureRole> {
        if let Some(n) = self.analysis.reactants.iter().position(|r| r.structure == Some(id)) {
            return Some(StructureRole::Reactant(n));
        }
        self.analysis
            .products
            .iter()
            .position(|p| p.structure == Some(id))
            .map(StructureRole::Product)
    }

    /// Copy `molecule` into the column `role` names. `false` when there is no such column.
    fn copy_into_column(&mut self, role: StructureRole, molecule: Molecule) -> bool {
        let slot = match role {
            StructureRole::Reactant(n) => self.analysis.reactants.get_mut(n).map(|r| &mut r.molecule),
            StructureRole::Product(n) => self.analysis.products.get_mut(n).map(|p| &mut p.molecule),
        };
        match slot {
            Some(slot) => {
                *slot = molecule;
                true
            }
            None => false,
        }
    }

    /// The positional rule [`Reaction::new`] links by: the ordinal of the structure at
    /// `scheme_index` among the structures on its side of the first arrow.
    fn positional_structure_role(&self, scheme_index: usize) -> Option<StructureRole> {
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

    /// Copy the molecule drawn at scheme item `scheme_index` into the analysis column
    /// [linked to it](Reaction::structure_role), so the table's formula, MW and stoichiometry
    /// follow the drawn structure.
    ///
    /// Returns `true` when a linked column existed and was updated. `false` means the item is
    /// not a structure, or no column is linked to it (it stands for nothing in the table).
    pub fn sync_structure_from_scheme(&mut self, scheme_index: usize) -> bool {
        let Some(role) = self.structure_role(scheme_index) else {
            return false;
        };
        let Some(SchemeItem::Structure(structure)) = self.scheme.items.get(scheme_index) else {
            return false;
        };
        let molecule = structure.molecule().clone();
        self.copy_into_column(role, molecule)
    }

    /// Replace the structure drawn at scheme item `scheme_index`, carrying the change into the
    /// matching analysis column so the table follows. Returns `false` (and changes nothing)
    /// when the item is not a structure.
    ///
    /// This is the structure-editing counterpart of typing into a table cell: both leave the
    /// figure re-derivable from what is drawn. The structure is redrawn, not swapped for a new
    /// one, so it keeps its [`StructureId`](super::scheme::StructureId).
    pub fn set_scheme_structure(&mut self, scheme_index: usize, molecule: Molecule) -> bool {
        let Some(SchemeItem::Structure(structure)) = self.scheme.items.get_mut(scheme_index) else {
            return false;
        };
        *structure.molecule_mut() = molecule;
        self.sync_structure_from_scheme(scheme_index);
        true
    }

    /// Bring the table back in line with the scheme after any change to it — a redraw, a
    /// reorder, a move onto or off an arrow band, a removal. Use after a bulk change, when
    /// individual [`sync_structure_from_scheme`](Reaction::sync_structure_from_scheme) calls
    /// were not made. Columns follow their structure by [`StructureId`]:
    ///
    /// * every linked column takes the molecule its structure now draws (main-line structures
    ///   and structures on arrow bands alike);
    /// * a column whose structure is no longer anywhere in the scheme is **unlinked**
    ///   (`structure = None`) but kept, with its molecule and every entered value — removing a
    ///   drawing never deletes what was typed into the table;
    /// * the positions the linked columns occupy are refilled with those same columns in
    ///   scheme order (item by item, an arrow's above band before its below band), so the
    ///   table reads like the scheme again after a reorder. Unlinked columns keep their
    ///   positions. A column moves whole, so its limiting flag, masses and equivalents stay
    ///   with its compound;
    /// * a linked column whose structure now sits on the **other side of the arrow** is
    ///   converted, Reactant ↔ Product (see below).
    ///
    /// **Crossing the arrow.** A structure's side is where it is drawn now: a main-line
    /// structure before the first arrow is a reactant, one after it a product, and a structure
    /// on an arrow band is a reagent, so a reactant — the same split [`Reaction::new`] links by.
    /// With no arrow at all, every structure is a reactant. Dragging a structure across the
    /// arrow, moving the arrow past structures or removing the arrow can therefore leave a
    /// linked column on the wrong side; it is then converted:
    ///
    /// * carried over: molecule, name, coefficient, equivalents and the [`StructureId`] link;
    /// * dropped: whatever the other side has no field for — a reactant's limiting flag,
    ///   sample mass, percent weight, molarity, density and volume, or a product's percent
    ///   completion, measured mass and purity. A converted limiting reagent leaves the
    ///   reaction **without** a limiting reagent (no neighbour is promoted, as with
    ///   [`Analysis::remove_reactant`]); a product turned into a reactant is not limiting;
    /// * dropped values are not brought back if the structure later crosses back;
    /// * the converted column joins its new side right after that side's last linked column
    ///   still drawn (at the front when there is none), then takes its scheme-order place
    ///   among the linked columns like any other. Unlinked columns keep their relative order.
    ///
    /// A structure added straight to the scheme gets no column, and an unchanged figure is
    /// left exactly as it was. A [multi-step](Reaction::is_multi_step) scheme leaves the table
    /// untouched.
    pub fn sync_structures_from_scheme(&mut self) {
        if self.is_multi_step() {
            return;
        }
        // Every drawn structure, in scheme order, with the side it is drawn on. A cloned
        // structure keeps its id, so one id can be drawn twice; its first appearance decides
        // where its column goes.
        let mut drawn: Vec<(StructureId, &Molecule, Side)> = Vec::new();
        let mut line_side = Side::Reactant;
        for item in &self.scheme.items {
            let labels = match item {
                SchemeItem::Structure(s) => [Some((s, line_side)), None],
                SchemeItem::Arrow(a) => {
                    line_side = Side::Product;
                    [
                        band_structure(&a.above).map(|s| (s, Side::Reactant)),
                        band_structure(&a.below).map(|s| (s, Side::Reactant)),
                    ]
                }
                _ => [None, None],
            };
            for (s, side) in labels.into_iter().flatten() {
                if !drawn.iter().any(|(id, _, _)| *id == s.id()) {
                    drawn.push((s.id(), s.molecule(), side));
                }
            }
        }
        let side_of = |link: Option<StructureId>| {
            let id = link?;
            drawn.iter().find(|(d, _, _)| *d == id).map(|(_, _, side)| *side)
        };

        // Columns now on the wrong side of the arrow move across, converted.
        let analysis = &mut self.analysis;
        let (reactants, to_products): (Vec<_>, Vec<_>) = std::mem::take(&mut analysis.reactants)
            .into_iter()
            .partition(|r| side_of(r.structure) != Some(Side::Product));
        let (products, to_reactants): (Vec<_>, Vec<_>) = std::mem::take(&mut analysis.products)
            .into_iter()
            .partition(|p| side_of(p.structure) != Some(Side::Reactant));
        analysis.reactants = reactants;
        analysis.products = products;
        insert_after_last_linked(
            &mut analysis.reactants,
            |r| side_of(r.structure).is_some(),
            to_reactants.into_iter().map(product_to_reactant),
        );
        insert_after_last_linked(
            &mut analysis.products,
            |p| side_of(p.structure).is_some(),
            to_products.into_iter().map(reactant_to_product),
        );

        // Each column's place in scheme order, re-deriving the linked ones and unlinking those
        // whose structure left the scheme.
        let rank = |link: &mut Option<StructureId>, molecule: &mut Molecule| -> Option<usize> {
            let id = (*link)?;
            let found = drawn.iter().position(|(d, _, _)| *d == id);
            match found {
                Some(rank) => *molecule = drawn[rank].1.clone(),
                None => *link = None,
            }
            found
        };
        let reactant_ranks: Vec<_> =
            analysis.reactants.iter_mut().map(|r| rank(&mut r.structure, &mut r.molecule)).collect();
        let product_ranks: Vec<_> =
            analysis.products.iter_mut().map(|p| rank(&mut p.structure, &mut p.molecule)).collect();
        reorder_linked(&mut analysis.reactants, &reactant_ranks);
        reorder_linked(&mut analysis.products, &product_ranks);
    }

    // ── Arrow-label structures ↔ reactant columns ────────────────────────────

    /// How many main-line structures sit before the first arrow — the reactant structures whose
    /// columns [`Reaction::new`] links first. Arrow-label reagent columns are numbered *after*
    /// these.
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

    /// Which analysis column the structure on an arrow's above/below label stands for: the
    /// column linked to its [`StructureId`], as with
    /// [`structure_role`](Reaction::structure_role).
    ///
    /// `None` when the item is not an arrow, the part is not a band, the band carries nothing
    /// or text, or no column is linked to its structure.
    pub fn arrow_structure_role(&self, index: usize, part: SchemePart) -> Option<StructureRole> {
        let Some(SchemeItem::Arrow(arrow)) = self.scheme.items.get(index) else {
            return None;
        };
        let label = match part {
            SchemePart::ArrowAbove => &arrow.above,
            SchemePart::ArrowBelow => &arrow.below,
            _ => return None,
        };
        self.column_linked_to(band_structure(label)?.id())
    }

    /// The positional rule [`Reaction::new`] links arrow-label structures by: they are
    /// reagents, so they take **reactant** columns after the main-line reactant structures
    /// ([`main_line_reactant_count`](Reaction::main_line_reactant_count)). The k-th arrow-label
    /// structure — counted above-before-below, in scheme order, over every arrow — is
    /// `Reactant(main_line_reactant_count + k)`; empty/text bands consume no ordinal.
    fn positional_arrow_structure_role(
        &self,
        index: usize,
        part: SchemePart,
    ) -> Option<StructureRole> {
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

    /// Copy the structure on an arrow band into the column
    /// [linked to it](Reaction::arrow_structure_role), so the table follows the drawn reagent.
    ///
    /// Returns `true` when a linked column existed and was updated. `false` means the band
    /// carries no structure, or no column is linked to it (the arrow reagent then stands for
    /// nothing in the table, exactly as an unlinked main-line structure does).
    pub fn sync_arrow_structure_from_scheme(&mut self, index: usize, part: SchemePart) -> bool {
        let Some(role) = self.arrow_structure_role(index, part) else {
            return false;
        };
        let Some(molecule) = super::edit::scheme_arrow_structure(&self.scheme, index, part).cloned()
        else {
            return false;
        };
        self.copy_into_column(role, molecule)
    }

    /// Put a structure on an arrow's above/below band and carry it into the reactant column it
    /// stands for, so the table's formula, MW and stoichiometry follow — the arrow-label
    /// counterpart of [`set_scheme_structure`](Reaction::set_scheme_structure). A structure
    /// already on the band is redrawn and keeps its id; see [`super::edit::set_scheme_arrow_structure`].
    ///
    /// Returns `false` (changing nothing) when the item is not an arrow or the part is not a
    /// band. Drawing over text or an empty band makes a new structure no column is linked to
    /// yet. Writing over a band can drop the structure that was there, so this does a full
    /// [`sync_structures_from_scheme`](Reaction::sync_structures_from_scheme).
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

/// The structure on an arrow band, id and all, when the band carries one.
fn band_structure(label: &ArrowLabel) -> Option<&SchemeStructure> {
    match label {
        ArrowLabel::Structure(s) => Some(s),
        _ => None,
    }
}

/// Which side of the arrow a drawn structure stands on; see
/// [`Reaction::sync_structures_from_scheme`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Reactant,
    Product,
}

/// A reactant column whose structure crossed to the product side. Keeps what a product column
/// can hold (molecule, name, coefficient, equivalents, link); the reactant-only entries — the
/// limiting flag among them — are dropped.
fn reactant_to_product(r: Reactant) -> Product {
    Product {
        name: r.name,
        coefficient: r.coefficient,
        equivalents: r.equivalents,
        structure: r.structure,
        ..Product::new(r.molecule)
    }
}

/// A product column whose structure crossed to the reactant side. Keeps what both column kinds
/// share (molecule, name, coefficient, equivalents, link); it is not limiting.
fn product_to_reactant(p: Product) -> Reactant {
    Reactant {
        name: p.name,
        coefficient: p.coefficient,
        equivalents: p.equivalents,
        structure: p.structure,
        ..Reactant::new(p.molecule)
    }
}

/// Insert `incoming` right after the last column `is_linked` accepts (at the front when none
/// does), keeping the order of both.
fn insert_after_last_linked<T>(
    columns: &mut Vec<T>,
    is_linked: impl Fn(&T) -> bool,
    incoming: impl IntoIterator<Item = T>,
) {
    let at = columns.iter().rposition(is_linked).map_or(0, |i| i + 1);
    columns.splice(at..at, incoming);
}

/// Refill the positions of the linked columns (`ranks[i]` is `Some`) with those same columns
/// sorted by rank, leaving every unlinked column (`None`) where it is. Equal ranks keep their
/// relative order.
fn reorder_linked<T>(columns: &mut Vec<T>, ranks: &[Option<usize>]) {
    let slots: Vec<usize> = (0..ranks.len()).filter(|&i| ranks[i].is_some()).collect();
    let mut sorted = slots.clone();
    sorted.sort_by_key(|&i| ranks[i]);
    if sorted == slots {
        return; // already in scheme order
    }
    // `source[p]` is the old position of the column that ends up at `p`.
    let mut source: Vec<usize> = (0..columns.len()).collect();
    for (slot, from) in slots.into_iter().zip(sorted) {
        source[slot] = from;
    }
    let mut old: Vec<Option<T>> = std::mem::take(columns).into_iter().map(Some).collect();
    *columns = source
        .into_iter()
        .map(|from| old[from].take().expect("a permutation moves each column exactly once"))
        .collect();
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
        //
        // G-005: the role is now the column linked by id, so the scheme needs columns to be
        // linked to. This used to pass an empty `Analysis` and read the positional ordinals
        // straight off the scheme; `Reaction::new` now links these columns by that same rule.
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(Arrow::new())
            .structure(demo::anisole())
            .plus()
            .structure(demo::potassium_carbonate());
        let analysis = Analysis::new()
            .reactant(Reactant::new(demo::phenol()))
            .reactant(Reactant::new(demo::methyl_iodide()))
            .product(Product::new(demo::anisole()))
            .product(Product::new(demo::potassium_carbonate()));
        let reaction = Reaction::new(scheme, analysis);
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
        // Redraw the structures straight in the scheme, bypassing the syncing setter …
        //
        // G-005: this used to assign brand-new structures (`demo::anisole().into()`), which
        // now carry fresh ids: the drawn phenol and anisole would be *removed* and their
        // columns unlinked (see `removing_a_structure_unlinks_its_column`). Redrawing the
        // molecules in place keeps the ids, which is what this test is about.
        let redraw = |item: &mut SchemeItem, molecule| match item {
            SchemeItem::Structure(s) => *s.molecule_mut() = molecule,
            other => panic!("not a structure: {other:?}"),
        };
        redraw(&mut reaction.scheme.items[0], demo::anisole());
        redraw(&mut reaction.scheme.items[2], demo::phenol());
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
        // G-005: roles are looked up by id, so this now gives `Reaction::new` four reactant
        // columns to link (it used to pass an empty `Analysis` and read positional ordinals).
        let analysis = Analysis::new()
            .reactant(Reactant::new(demo::phenol()))
            .reactant(Reactant::new(demo::methyl_iodide()))
            .reactant(Reactant::new(demo::biphenyl()))
            .reactant(Reactant::new(demo::benzene()));
        let reaction = Reaction::new(scheme, analysis);
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
        reaction.scheme.add(SchemeItem::Structure(demo::benzene().into()));
        reaction.sync_structures_from_scheme();

        assert_eq!(reaction.analysis.reactants.len(), 3);
        assert_eq!(reaction.analysis.products.len(), 1);
        assert_eq!(product_formulas(&reaction), ["C7H8O"]);
        assert_eq!(entered_values(&reaction.analysis), values);
    }

    // ── Column ↔ structure ids (G-005) ───────────────────────────────────────

    fn line_id(reaction: &Reaction, index: usize) -> StructureId {
        match &reaction.scheme.items[index] {
            SchemeItem::Structure(s) => s.id(),
            other => panic!("item {index} is not a structure: {other:?}"),
        }
    }

    fn arrow_label_id(reaction: &Reaction, index: usize, part: SchemePart) -> StructureId {
        let SchemeItem::Arrow(arrow) = &reaction.scheme.items[index] else {
            panic!("item {index} is not an arrow");
        };
        let label = if part == SchemePart::ArrowAbove { &arrow.above } else { &arrow.below };
        match label {
            ArrowLabel::Structure(s) => s.id(),
            other => panic!("the {part:?} band carries no structure: {other:?}"),
        }
    }

    /// `Reaction::new` links the demo's drawn phenol and anisole to their columns; MeI and
    /// K₂CO₃, written as arrow text, stand for no structure.
    #[test]
    fn new_links_demo_columns_by_position() {
        let reaction = demo::williamson_ether_synthesis();
        let a = &reaction.analysis;
        assert_eq!(a.reactants[0].structure, Some(line_id(&reaction, 0)), "phenol");
        assert_eq!(a.reactants[1].structure, None, "MeI is arrow text");
        assert_eq!(a.reactants[2].structure, None, "K2CO3 is arrow text");
        assert_eq!(a.products[0].structure, Some(line_id(&reaction, 2)), "anisole");
    }

    /// An arrow-label structure is linked to the reactant column after the main-line ones.
    #[test]
    fn new_links_arrow_label_structures_after_main_line_reactants() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new().above_structure(demo::biphenyl()).below("cat."))
            .structure(demo::anisole());
        let analysis = Analysis::new()
            .reactant(Reactant::new(demo::phenol()).limiting().sample_mass(Mass::grams(1.0)))
            .reactant(Reactant::new(demo::benzene()).equivalents(1.0))
            .reactant(Reactant::new(demo::methyl_iodide()).equivalents(1.0))
            .product(Product::new(demo::anisole()));
        let reaction = Reaction::new(scheme, analysis);
        let a = &reaction.analysis;
        assert_eq!(a.reactants[0].structure, Some(line_id(&reaction, 0)));
        assert_eq!(
            a.reactants[1].structure,
            Some(arrow_label_id(&reaction, 1, SchemePart::ArrowAbove)),
            "the biphenyl above the arrow"
        );
        assert_eq!(a.reactants[2].structure, None, "no structure is left for it");
        assert_eq!(a.products[0].structure, Some(line_id(&reaction, 2)));
    }

    /// A column that already names a structure keeps it, even when the structure at its
    /// position is a different one.
    #[test]
    fn new_keeps_existing_links() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(Arrow::new())
            .structure(demo::anisole());
        let SchemeItem::Structure(mei) = &scheme.items[2] else {
            panic!("item 2 is the MeI structure");
        };
        let mei = mei.id();
        let elsewhere = StructureId::fresh();
        // Column 0 is linked to the MeI drawn at item 2; column 1 to an id the scheme lacks.
        let mut first = Reactant::new(demo::methyl_iodide());
        first.structure = Some(mei);
        let mut second = Reactant::new(demo::phenol());
        second.structure = Some(elsewhere);
        let analysis = Analysis::new()
            .reactant(first)
            .reactant(second)
            .product(Product::new(demo::anisole()));
        let reaction = Reaction::new(scheme, analysis);
        let a = &reaction.analysis;
        assert_eq!(a.reactants[0].structure, Some(mei), "not overwritten by phenol's id");
        assert_eq!(a.reactants[1].structure, Some(elsewhere), "not overwritten by MeI's id");
        // The phenol at item 0 had no free column at its position, so nothing names it.
        let phenol = line_id(&reaction, 0);
        assert!(a.reactants.iter().all(|r| r.structure != Some(phenol)));
        assert_eq!(a.products[0].structure, Some(line_id(&reaction, 4)));
    }

    /// A structure some column already names is not linked to a second column, even when the
    /// column at its position is free.
    #[test]
    fn new_does_not_link_a_structure_twice() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(Arrow::new())
            .structure(demo::anisole());
        let SchemeItem::Structure(mei) = &scheme.items[2] else {
            panic!("item 2 is the MeI structure");
        };
        let mei = mei.id();
        // Column 0 already names the MeI drawn at item 2; column 1 — MeI's positional column —
        // is unlinked.
        let mut first = Reactant::new(demo::methyl_iodide());
        first.structure = Some(mei);
        let analysis = Analysis::new()
            .reactant(first)
            .reactant(Reactant::new(demo::phenol()))
            .product(Product::new(demo::anisole()));
        let reaction = Reaction::new(scheme, analysis);
        let a = &reaction.analysis;
        assert_eq!(a.reactants[0].structure, Some(mei));
        assert_eq!(a.reactants[1].structure, None, "MeI is already named by column 0");
        let naming_mei = a.reactants.iter().filter(|r| r.structure == Some(mei)).count()
            + a.products.iter().filter(|p| p.structure == Some(mei)).count();
        assert_eq!(naming_mei, 1, "MeI's id appears in exactly one column");
    }

    // ── Sync by id: values stay with their compound (G-005) ──────────────────

    /// `[phenol, +, MeI, →(above biphenyl, below empty), anisole]` over the columns
    /// `[phenol, K₂CO₃, MeI, biphenyl | anisole, by-product]`. K₂CO₃ and the by-product are
    /// unlinked and sit *between* linked columns, so a reorder must step around them. Every
    /// column carries its own entered values, so a test can tell whether they followed.
    fn id_fixture() -> Reaction {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .plus()
            .structure(demo::methyl_iodide())
            .arrow(Arrow::new().above_structure(demo::biphenyl()))
            .structure(demo::anisole());
        let linked = |r: Reactant, id| Reactant { structure: Some(id), ..r };
        let ids = (
            match &scheme.items[0] { SchemeItem::Structure(s) => s.id(), _ => unreachable!() },
            match &scheme.items[2] { SchemeItem::Structure(s) => s.id(), _ => unreachable!() },
            match &scheme.items[3] {
                SchemeItem::Arrow(a) => match &a.above { ArrowLabel::Structure(s) => s.id(), _ => unreachable!() },
                _ => unreachable!(),
            },
        );
        let analysis = Analysis::new()
            .reactant(linked(Reactant::new(demo::phenol()).limiting().sample_mass(Mass::grams(1.0)), ids.0))
            .reactant(Reactant::new(demo::potassium_carbonate()).equivalents(2.0))
            .reactant(linked(Reactant::new(demo::methyl_iodide()).equivalents(1.1), ids.1))
            .reactant(linked(
                Reactant::new(demo::biphenyl()).equivalents(0.1).sample_mass(Mass::milligrams(150.0)),
                ids.2,
            ))
            .product(Product::new(demo::anisole()).measured_mass(Mass::grams(0.9)))
            .product(Product::new(demo::benzene()).measured_mass(Mass::milligrams(20.0)));
        let reaction = Reaction::new(scheme, analysis);
        // `new` links the anisole to product column 0 by position; nothing else is free.
        assert_eq!(reaction.analysis.reactants[1].structure, None);
        assert!(reaction.analysis.products[0].structure.is_some());
        assert_eq!(reaction.analysis.products[1].structure, None);
        reaction
    }

    /// `(limiting, equivalents, sample_mass)` as entered in [`id_fixture`], keyed by formula.
    fn fixture_reactant_values(formula: &str) -> (bool, Option<f64>, Option<Mass>) {
        match formula {
            "C6H6O" => (true, None, Some(Mass::grams(1.0))),
            "CK2O3" => (false, Some(2.0), None),
            "CH3I" => (false, Some(1.1), None),
            "C12H10" => (false, Some(0.1), Some(Mass::milligrams(150.0))),
            other => panic!("no reactant {other} in the fixture"),
        }
    }

    /// The reactant columns read `formulas`, in that order, and every column still carries the
    /// values entered for its compound; the products are untouched.
    fn assert_columns_follow_compounds(reaction: &Reaction, formulas: [&str; 4]) {
        assert_eq!(reactant_formulas(reaction), formulas, "column order");
        for (r, formula) in reaction.analysis.reactants.iter().zip(formulas) {
            assert_eq!(
                (r.limiting, r.equivalents, r.sample_mass),
                fixture_reactant_values(formula),
                "the values entered for {formula} stayed with it"
            );
        }
        assert_eq!(reaction.analysis.limiting(), formulas.iter().position(|f| *f == "C6H6O"));
        assert_eq!(product_formulas(reaction), ["C7H8O", "C6H6"], "products unchanged");
        let measured: Vec<_> = reaction.analysis.products.iter().map(|p| p.measured_mass).collect();
        assert_eq!(measured, [Some(Mass::grams(0.9)), Some(Mass::milligrams(20.0))]);
    }

    /// The column each drawn structure is linked to reads that structure's molecule.
    fn assert_links_by_id(reaction: &Reaction) {
        for (i, item) in reaction.scheme.items.iter().enumerate() {
            match item {
                SchemeItem::Structure(s) => {
                    if let Some(StructureRole::Reactant(n)) = reaction.structure_role(i) {
                        assert_eq!(reaction.analysis.reactants[n].structure, Some(s.id()));
                        assert!(*s == reaction.analysis.reactants[n].molecule);
                    }
                }
                SchemeItem::Arrow(a) => {
                    for (part, label) in [(SchemePart::ArrowAbove, &a.above), (SchemePart::ArrowBelow, &a.below)] {
                        if let ArrowLabel::Structure(s) = label {
                            let Some(StructureRole::Reactant(n)) = reaction.arrow_structure_role(i, part) else {
                                panic!("the arrow structure at ({i}, {part:?}) lost its column");
                            };
                            assert_eq!(reaction.analysis.reactants[n].structure, Some(s.id()));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Reordering on the same side: the linked columns are put in scheme order and take their
    /// values with them; the unlinked K₂CO₃ column keeps its position.
    #[test]
    fn a_same_side_reorder_moves_values_with_their_compound() {
        let mut reaction = id_fixture();
        assert_eq!(reaction.scheme.move_item(2, -2), Some(0)); // MeI to the front
        reaction.sync_structures_from_scheme();
        assert_columns_follow_compounds(&reaction, ["CH3I", "CK2O3", "C6H6O", "C12H10"]);
        assert_links_by_id(&reaction);
    }

    /// The roles are looked up by id, not counted by position: after a reorder each structure
    /// still names the column its compound sits in.
    #[test]
    fn roles_are_the_columns_linked_by_id() {
        let mut reaction = id_fixture();
        assert_eq!(reaction.structure_role(0), Some(StructureRole::Reactant(0)));
        assert_eq!(reaction.structure_role(2), Some(StructureRole::Reactant(2)), "not 1: K2CO3 sits between");
        assert_eq!(reaction.arrow_structure_role(3, SchemePart::ArrowAbove), Some(StructureRole::Reactant(3)));
        assert_eq!(reaction.arrow_structure_role(3, SchemePart::ArrowBelow), None, "empty band");
        assert_eq!(reaction.structure_role(4), Some(StructureRole::Product(0)));
        assert_eq!(reaction.structure_role(1), None, "the plus");

        // A structure no column names has no role, wherever it sits.
        reaction.scheme.add(SchemeItem::Structure(demo::benzene().into()));
        assert_eq!(reaction.structure_role(5), None);
    }

    /// Removing a structure keeps its column — molecule and values — and only clears the link.
    #[test]
    fn removing_a_structure_unlinks_its_column() {
        let mut reaction = id_fixture();
        assert!(reaction.scheme.remove(2).is_some()); // MeI
        reaction.sync_structures_from_scheme();
        let mei = &reaction.analysis.reactants[2];
        assert_eq!(mei.structure, None, "the link is gone");
        assert_columns_follow_compounds(&reaction, ["C6H6O", "CK2O3", "CH3I", "C12H10"]);
        assert_links_by_id(&reaction);

        // The arrow reagent too; then a reorder of what is left steps around both.
        let SchemeItem::Arrow(arrow) = &mut reaction.scheme.items[2] else { panic!("the arrow") };
        arrow.above = ArrowLabel::Empty;
        reaction.sync_structures_from_scheme();
        assert_eq!(reaction.analysis.reactants[3].structure, None);
        assert_columns_follow_compounds(&reaction, ["C6H6O", "CK2O3", "CH3I", "C12H10"]);

        // Removing the product unlinks its column the same way.
        assert!(reaction.scheme.remove(3).is_some());
        reaction.sync_structures_from_scheme();
        assert_eq!(reaction.analysis.products[0].structure, None);
        assert_columns_follow_compounds(&reaction, ["C6H6O", "CK2O3", "CH3I", "C12H10"]);
    }

    /// Redrawing a structure (on the line or on an arrow band) carries the new molecule into
    /// the column linked to it, which keeps its place and its values.
    #[test]
    fn replacing_a_structure_redraws_its_own_column() {
        let mut reaction = id_fixture();
        assert!(reaction.set_scheme_structure(2, demo::benzene())); // MeI → benzene
        assert_eq!(reactant_formulas(&reaction), ["C6H6O", "CK2O3", "C6H6", "C12H10"]);
        assert_eq!(reaction.analysis.reactants[2].equivalents, Some(1.1), "MeI's eq kept");

        assert!(reaction.set_scheme_arrow_structure(3, SchemePart::ArrowAbove, demo::anisole()));
        assert_eq!(reactant_formulas(&reaction), ["C6H6O", "CK2O3", "C6H6", "C7H8O"]);
        let r = &reaction.analysis.reactants[3];
        assert_eq!((r.equivalents, r.sample_mass), (Some(0.1), Some(Mass::milligrams(150.0))));
        assert_eq!(reaction.analysis.limiting(), Some(0));
        assert_links_by_id(&reaction);
    }

    /// Moving a line structure onto an empty arrow band, swapping one with a band, trading two
    /// bands and dropping a band structure back onto the line: after each resync the linked
    /// columns read in scheme order with their own values, and K₂CO₃ stays put.
    #[test]
    fn arrow_moves_keep_values_with_their_compound() {
        use crate::reaction::edit::{
            move_arrow_structure_to_line, move_line_structure_to_arrow_slot,
            place_scheme_arrow_structure, swap_line_structure_with_arrow_slot,
            take_scheme_arrow_structure,
        };
        const ABOVE: SchemePart = SchemePart::ArrowAbove;
        const BELOW: SchemePart = SchemePart::ArrowBelow;

        // MeI onto the empty band below the arrow: [phenol, +, →(biphenyl / MeI), anisole].
        let mut reaction = id_fixture();
        assert_eq!(move_line_structure_to_arrow_slot(&mut reaction.scheme, 2, 3, BELOW), Some(2));
        reaction.sync_structures_from_scheme();
        assert_columns_follow_compounds(&reaction, ["C6H6O", "CK2O3", "C12H10", "CH3I"]);
        assert_links_by_id(&reaction);

        // Phenol swaps with the biphenyl over the arrow: [biphenyl, +, MeI, →(phenol), …].
        let mut reaction = id_fixture();
        assert!(swap_line_structure_with_arrow_slot(&mut reaction.scheme, 0, 3, ABOVE));
        reaction.sync_structures_from_scheme();
        assert_columns_follow_compounds(&reaction, ["C12H10", "CK2O3", "CH3I", "C6H6O"]);
        assert_links_by_id(&reaction);

        // Band to band, as the widget's slot drop does it: MeI first goes below, then the two
        // bands trade structures.
        let mut reaction = id_fixture();
        move_line_structure_to_arrow_slot(&mut reaction.scheme, 2, 3, BELOW).unwrap();
        reaction.sync_structures_from_scheme();
        let moving = take_scheme_arrow_structure(&mut reaction.scheme, 2, BELOW).unwrap();
        let displaced = take_scheme_arrow_structure(&mut reaction.scheme, 2, ABOVE).unwrap();
        assert!(place_scheme_arrow_structure(&mut reaction.scheme, 2, ABOVE, moving));
        assert!(place_scheme_arrow_structure(&mut reaction.scheme, 2, BELOW, displaced));
        reaction.sync_structures_from_scheme();
        assert_columns_follow_compounds(&reaction, ["C6H6O", "CK2O3", "CH3I", "C12H10"]);
        assert_links_by_id(&reaction);

        // The biphenyl back onto the line, in front: [biphenyl, phenol, +, MeI, →, anisole].
        let mut reaction = id_fixture();
        assert_eq!(move_arrow_structure_to_line(&mut reaction.scheme, 3, ABOVE, 0), Some(0));
        reaction.sync_structures_from_scheme();
        assert_columns_follow_compounds(&reaction, ["C12H10", "CK2O3", "C6H6O", "CH3I"]);
        assert_links_by_id(&reaction);
    }

    // ── Crossing the arrow: Reactant ↔ Product (G-005) ───────────────────────

    /// The column linked to `id`, which must exist.
    fn linked(reaction: &Reaction, id: StructureId) -> StructureRole {
        reaction.column_linked_to(id).expect("the structure still has a column")
    }

    /// Phenol (the limiting reagent) dragged past the arrow becomes a product column carrying
    /// its molecule, name, coefficient, equivalents and id; the reaction is left without a
    /// limiting reagent. Dragged back, it is a reactant again — not limiting, its weighed mass
    /// not restored.
    #[test]
    fn a_structure_dragged_across_the_arrow_changes_column_kind() {
        let mut reaction = id_fixture();
        let phenol = line_id(&reaction, 0);
        {
            let r = &mut reaction.analysis.reactants[0];
            r.name = Some("phenol".into());
            r.coefficient = 2.0;
            r.equivalents = Some(1.0);
        }

        // [+, MeI, →(biphenyl), anisole, phenol]
        assert_eq!(reaction.scheme.move_item(0, 4), Some(4));
        reaction.sync_structures_from_scheme();
        assert_eq!(reactant_formulas(&reaction), ["CK2O3", "CH3I", "C12H10"]);
        assert_eq!(product_formulas(&reaction), ["C7H8O", "C6H6O", "C6H6"]);
        assert_eq!(linked(&reaction, phenol), StructureRole::Product(1));
        assert_eq!(reaction.structure_role(4), Some(StructureRole::Product(1)));
        let p = &reaction.analysis.products[1];
        assert_eq!(p.name.as_deref(), Some("phenol"));
        assert_eq!((p.coefficient, p.equivalents), (2.0, Some(1.0)));
        assert_eq!(p.measured_mass, None);
        assert_eq!(reaction.analysis.limiting(), None, "the limiting reagent left the reactants");
        // The other columns keep their own values.
        assert_eq!(reaction.analysis.reactants[1].equivalents, Some(1.1));
        assert_eq!(reaction.analysis.products[0].measured_mass, Some(Mass::grams(0.9)));
        assert_links_by_id(&reaction);

        // Back to the front: [phenol, +, MeI, →(biphenyl), anisole].
        assert_eq!(reaction.scheme.move_item(4, -4), Some(0));
        reaction.sync_structures_from_scheme();
        assert_eq!(reactant_formulas(&reaction), ["CK2O3", "C6H6O", "CH3I", "C12H10"]);
        assert_eq!(product_formulas(&reaction), ["C7H8O", "C6H6"]);
        assert_eq!(linked(&reaction, phenol), StructureRole::Reactant(1));
        let r = &reaction.analysis.reactants[1];
        assert_eq!(r.name.as_deref(), Some("phenol"));
        assert_eq!((r.coefficient, r.equivalents), (2.0, Some(1.0)));
        assert!(!r.limiting, "limiting is not restored");
        assert_eq!(r.sample_mass, None, "the weighed mass dropped on crossing is not restored");
        assert_eq!(reaction.analysis.limiting(), None);
        assert_links_by_id(&reaction);
    }

    /// Moving the arrow to the front puts every main-line structure after it: phenol and MeI
    /// become products; the biphenyl on the arrow band stays a reactant.
    #[test]
    fn moving_the_arrow_to_the_front_makes_line_structures_products() {
        let mut reaction = id_fixture();
        // [→(biphenyl), phenol, +, MeI, anisole]
        assert_eq!(reaction.scheme.move_item(3, -3), Some(0));
        reaction.sync_structures_from_scheme();
        assert_eq!(reactant_formulas(&reaction), ["CK2O3", "C12H10"]);
        assert_eq!(product_formulas(&reaction), ["C6H6O", "CH3I", "C7H8O", "C6H6"]);
        assert_eq!(reaction.analysis.limiting(), None);
        let eq: Vec<_> = reaction.analysis.products.iter().map(|p| p.equivalents).collect();
        assert_eq!(eq, [None, Some(1.1), None, None], "MeI's equivalents crossed with it");
        let measured: Vec<_> = reaction.analysis.products.iter().map(|p| p.measured_mass).collect();
        assert_eq!(measured, [None, None, Some(Mass::grams(0.9)), Some(Mass::milligrams(20.0))]);
        let r = &reaction.analysis.reactants[1];
        assert_eq!((r.equivalents, r.sample_mass), (Some(0.1), Some(Mass::milligrams(150.0))));
        for i in [1, 3, 4] {
            assert!(matches!(reaction.structure_role(i), Some(StructureRole::Product(_))));
        }
        assert_links_by_id(&reaction);
    }

    /// Removing the arrow leaves no product side: the anisole becomes a reactant column (its
    /// measured mass dropped), the biphenyl that rode on the arrow is unlinked, and phenol
    /// stays the limiting reagent.
    #[test]
    fn removing_the_arrow_makes_every_structure_a_reactant() {
        let mut reaction = id_fixture();
        let anisole = line_id(&reaction, 4);
        // [phenol, +, MeI, anisole]
        assert!(reaction.scheme.remove(3).is_some());
        reaction.sync_structures_from_scheme();
        assert_eq!(reactant_formulas(&reaction), ["C6H6O", "CK2O3", "CH3I", "C7H8O", "C12H10"]);
        assert_eq!(product_formulas(&reaction), ["C6H6"]);
        assert_eq!(linked(&reaction, anisole), StructureRole::Reactant(3));
        let a = &reaction.analysis;
        assert!(!a.reactants[3].limiting);
        assert_eq!(a.reactants[3].sample_mass, None);
        assert_eq!(a.reactants[4].structure, None, "the biphenyl left with the arrow");
        assert_eq!(a.reactants[4].sample_mass, Some(Mass::milligrams(150.0)));
        assert_eq!(a.limiting(), Some(0));
        assert_eq!(a.reactants[0].sample_mass, Some(Mass::grams(1.0)));
        assert_eq!(a.products[0].measured_mass, Some(Mass::milligrams(20.0)));
        for i in [0, 2, 3] {
            assert!(matches!(reaction.structure_role(i), Some(StructureRole::Reactant(_))));
        }
    }

    /// An arrow band is the reagent side: a reactant moved onto it is not converted and keeps
    /// every value (the limiting flag too), while a product moved onto it becomes a reactant.
    #[test]
    fn arrow_bands_count_as_the_reactant_side() {
        use crate::reaction::edit::move_line_structure_to_arrow_slot;
        let mut reaction = id_fixture();
        let phenol = line_id(&reaction, 0);
        // [+, MeI, →(biphenyl / phenol), anisole]
        assert_eq!(
            move_line_structure_to_arrow_slot(&mut reaction.scheme, 0, 3, SchemePart::ArrowBelow),
            Some(2)
        );
        reaction.sync_structures_from_scheme();
        assert_eq!(linked(&reaction, phenol), StructureRole::Reactant(3));
        let r = &reaction.analysis.reactants[3];
        assert!(r.limiting);
        assert_eq!(r.sample_mass, Some(Mass::grams(1.0)));
        assert_eq!(reaction.analysis.limiting(), Some(3));
        assert_eq!(product_formulas(&reaction), ["C7H8O", "C6H6"]);

        // The anisole onto the band above, emptied by dropping the biphenyl (now unlinked).
        let mut reaction = id_fixture();
        let anisole = line_id(&reaction, 4);
        let SchemeItem::Arrow(arrow) = &mut reaction.scheme.items[3] else { panic!("the arrow") };
        arrow.above = ArrowLabel::Empty;
        assert_eq!(
            move_line_structure_to_arrow_slot(&mut reaction.scheme, 4, 3, SchemePart::ArrowAbove),
            Some(3)
        );
        reaction.sync_structures_from_scheme();
        assert_eq!(linked(&reaction, anisole), StructureRole::Reactant(3));
        assert_eq!(reaction.arrow_structure_role(3, SchemePart::ArrowAbove), Some(StructureRole::Reactant(3)));
        assert_eq!(reactant_formulas(&reaction), ["C6H6O", "CK2O3", "CH3I", "C7H8O", "C12H10"]);
        assert_eq!(product_formulas(&reaction), ["C6H6"]);
        assert_eq!(reaction.analysis.limiting(), Some(0));
    }

    /// A second arrow makes the scheme multi-step: the table is left alone, whatever moves,
    /// until the scheme is back to one arrow, when the columns line up with it again.
    #[test]
    fn a_multi_step_scheme_leaves_the_table_alone() {
        let mut reaction = demo::williamson_ether_synthesis();
        assert!(!reaction.is_multi_step());
        let (reactants, products) = (reactant_formulas(&reaction), product_formulas(&reaction));

        let second = reaction.scheme.add(SchemeItem::Arrow(Arrow::new()));
        assert!(reaction.is_multi_step());
        // Phenol to the far end: across both arrows. Nothing changes in the table.
        reaction.scheme.move_item(0, 10);
        reaction.sync_structures_from_scheme();
        assert_eq!((reactant_formulas(&reaction), product_formulas(&reaction)), (reactants.clone(), products.clone()));

        // Back to one arrow (the original one, now first): phenol is after it, a product.
        reaction.scheme.remove(second - 1);
        assert!(!reaction.is_multi_step());
        reaction.sync_structures_from_scheme();
        assert!(product_formulas(&reaction).contains(&"C6H6O".to_string()));
    }

    /// Columns are not linked by position across a multi-step scheme: there is no one arrow to
    /// tell reactants from products.
    #[test]
    fn a_multi_step_scheme_links_no_columns() {
        let scheme = ReactionScheme::new()
            .structure(demo::phenol())
            .arrow(Arrow::new())
            .structure(demo::anisole())
            .arrow(Arrow::new())
            .structure(demo::benzene());
        let analysis = Analysis::new().reactant(Reactant::new(demo::phenol())).product(Product::new(demo::benzene()));
        let reaction = Reaction::new(scheme, analysis);
        assert!(reaction.analysis.reactants[0].structure.is_none());
        assert!(reaction.analysis.products[0].structure.is_none());
    }
}
