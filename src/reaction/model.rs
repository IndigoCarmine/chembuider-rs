//! The data model a caller builds: reagents with their entered quantities, plus the
//! reaction scheme to draw above the table.
//!
//! Everything here is plain data — no egui, no interaction. Values the *user* supplies are
//! `Option`s: whatever is left `None` is either computed from the rest (see
//! [`Analysis::compute`](crate::reaction::stoich)) or drawn as an empty cell.

use crate::molecule::Molecule;

use super::scheme::ReactionScheme;

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

/// A reaction scheme plus its stoichiometry table — the whole figure.
///
/// The two halves are deliberately independent: the scheme in the reference figure draws
/// MeI and K₂CO₃ as text over and under the arrow, while the table still gives them a full
/// column each.
#[derive(Clone, Debug, Default)]
pub struct Reaction {
    pub scheme: ReactionScheme,
    pub analysis: Analysis,
}

impl Reaction {
    pub fn new(scheme: ReactionScheme, analysis: Analysis) -> Self {
        Self { scheme, analysis }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mass_units_round_trip() {
        assert_eq!(Mass::milligrams(700.0).as_grams(), 0.7);
        assert_eq!(Mass::grams(1.0).as_milligrams(), 1000.0);
        assert_eq!(Mass::kilograms(0.5).as_grams(), 500.0);
    }
}
