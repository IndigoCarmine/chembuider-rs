//! Stoichiometry: turn the entered quantities into the numbers ChemDraw's *Analysis* table
//! shows, and format them the way the reference figure does.
//!
//! # Rules
//!
//! * limiting reagent — `moles = sample mass / MW`, `equivalents = 1.00`
//! * every other reactant — `moles = equivalents × limiting moles`, `mass = moles × MW`
//! * product — `expected moles = limiting moles × coefficient`, `expected mass = … × MW`
//! * product — `product moles = measured mass / MW`, `%yield = product moles / expected × 100`
//! * %Weight — `reactant mass = sample mass × %Weight`, so a back-filled sample mass is
//!   `mass / %Weight`
//! * liquids — `volume = sample mass / density`; solutions — `moles = molarity × volume`
//!
//! Each number remembers whether it was [`Origin::Entered`] by the user or [`Origin::Computed`]
//! from the rest, because the table draws entered values in **bold italic**.

use super::formula::MolecularFormula;
use super::model::{Analysis, Product, Reactant};

/// Where a number in the table came from. Entered values are drawn bold + italic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// Typed in by the user.
    Entered,
    /// Derived from the other entries.
    Computed,
}

/// A number plus its [`Origin`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub origin: Origin,
}

impl Quantity {
    pub const fn entered(value: f64) -> Self {
        Self { value, origin: Origin::Entered }
    }
    pub const fn computed(value: f64) -> Self {
        Self { value, origin: Origin::Computed }
    }
    pub const fn is_entered(self) -> bool {
        matches!(self.origin, Origin::Entered)
    }
}

/// Everything the *Reactants* block prints for one column, in SI-ish base units
/// (grams, moles, mL, percent).
#[derive(Clone, Debug, Default)]
pub struct ReactantAnalysis {
    pub name: Option<String>,
    pub formula: MolecularFormula,
    /// g/mol; `None` when the structure carries a group label with no atomic weight.
    pub mw: Option<f64>,
    /// Whether this reagent sets the scale — declared, or picked by the fallback.
    pub limiting: bool,
    /// How it became limiting: [`Origin::Entered`] when the user declared it,
    /// [`Origin::Computed`] when no reagent is declared and the fallback picked this one.
    /// `None` exactly when [`limiting`](Self::limiting) is `false`.
    pub limiting_origin: Option<Origin>,
    pub equivalents: Option<Quantity>,
    /// `Sample Mass`, g — entered when the reagent was weighed out, otherwise the mass of
    /// material its equivalents call for ([`mass`](Self::mass) ÷ %Weight, computed).
    pub sample_mass: Option<Quantity>,
    pub percent_weight: Option<Quantity>,
    /// mol/L.
    pub molarity: Option<Quantity>,
    /// g/mL.
    pub density: Option<Quantity>,
    /// mL.
    pub volume: Option<Quantity>,
    /// `Reactant Moles`, mol.
    pub moles: Option<Quantity>,
    /// `Reactant Mass`, g.
    pub mass: Option<Quantity>,
}

/// Everything the *Products* block prints for one column.
#[derive(Clone, Debug, Default)]
pub struct ProductAnalysis {
    pub name: Option<String>,
    pub formula: MolecularFormula,
    pub mw: Option<f64>,
    pub equivalents: Option<Quantity>,
    pub percent_completion: Option<Quantity>,
    /// g.
    pub expected_mass: Option<Quantity>,
    /// mol.
    pub expected_moles: Option<Quantity>,
    /// g.
    pub measured_mass: Option<Quantity>,
    pub purity: Option<Quantity>,
    /// `Product Mass`, g — the measured mass corrected for purity.
    pub mass: Option<Quantity>,
    /// `Product Moles`, mol.
    pub moles: Option<Quantity>,
    pub percent_yield: Option<Quantity>,
}

/// The computed table: one entry per column, in the order they were added.
#[derive(Clone, Debug, Default)]
pub struct AnalysisResult {
    pub reactants: Vec<ReactantAnalysis>,
    pub products: Vec<ProductAnalysis>,
    /// Moles of the limiting reagent — the scale everything else is derived from.
    pub limiting_moles: Option<f64>,
    /// Index of the limiting reagent in [`reactants`](Self::reactants).
    pub limiting_index: Option<usize>,
}

impl Analysis {
    /// Run the stoichiometry. Missing inputs simply leave the corresponding cells empty;
    /// nothing here can fail.
    pub fn compute(&self) -> AnalysisResult {
        let mws: Vec<Option<f64>> = self
            .reactants
            .iter()
            .map(|r| r.formula().weight())
            .collect();

        // The limiting reagent: the one flagged, else the first whose amount is known
        // outright (a sample mass, or a concentration × volume).
        let declared = self.reactants.iter().position(|r| r.limiting);
        let limiting_index = declared.or_else(|| {
            (0..self.reactants.len()).find(|&i| entered_moles(&self.reactants[i], mws[i]).is_some())
        });
        let limiting_origin = |i: usize| match (Some(i) == limiting_index, declared.is_some()) {
            (false, _) => None,
            (true, true) => Some(Origin::Entered),
            (true, false) => Some(Origin::Computed),
        };
        let limiting_moles =
            limiting_index.and_then(|i| entered_moles(&self.reactants[i], mws[i]));

        let reactants = self
            .reactants
            .iter()
            .enumerate()
            .map(|(i, r)| {
                analyse_reactant(r, mws[i], limiting_moles, limiting_origin(i))
            })
            .collect();

        let products = self
            .products
            .iter()
            .map(|p| analyse_product(p, limiting_moles))
            .collect();

        AnalysisResult { reactants, products, limiting_moles, limiting_index }
    }
}

/// Moles a reactant's *own* entries pin down, independently of the rest of the table:
/// a weighed sample, a volume of stock solution, or a volume of neat liquid.
fn entered_moles(r: &Reactant, mw: Option<f64>) -> Option<f64> {
    let assay = r.percent_weight.unwrap_or(100.0) / 100.0;
    if let (Some(mass), Some(mw)) = (r.sample_mass, mw) {
        if mw > 0.0 {
            return Some(mass.as_grams() * assay / mw);
        }
    }
    if let (Some(molarity), Some(volume)) = (r.molarity, r.volume) {
        return Some(molarity * volume / 1000.0); // mol/L × mL
    }
    if let (Some(density), Some(volume), Some(mw)) = (r.density, r.volume, mw) {
        if mw > 0.0 {
            return Some(density * volume * assay / mw);
        }
    }
    None
}

fn analyse_reactant(
    r: &Reactant,
    mw: Option<f64>,
    limiting_moles: Option<f64>,
    limiting_origin: Option<Origin>,
) -> ReactantAnalysis {
    let is_limiting = limiting_origin.is_some();
    let own = entered_moles(r, mw);

    // Moles: what this reagent's own entries say, otherwise equivalents × the limiting scale.
    let moles = match (own, limiting_moles, r.equivalents) {
        (Some(m), _, _) => Some(Quantity::computed(m)),
        (None, Some(scale), eq) => Some(Quantity::computed(eq.unwrap_or(1.0) * scale)),
        _ => None,
    };

    // Equivalents: the entered value wins unless this reagent's own amount was pinned down
    // independently, in which case the ratio to the limiting reagent is the honest answer.
    let equivalents = match (r.equivalents, own, limiting_moles) {
        (Some(eq), None, _) => Some(Quantity::entered(eq)),
        (Some(eq), Some(_), _) if is_limiting => Some(Quantity::entered(eq)),
        (_, Some(m), Some(scale)) if scale > 0.0 => Some(Quantity::computed(m / scale)),
        (Some(eq), _, _) => Some(Quantity::entered(eq)),
        (None, _, _) if is_limiting => Some(Quantity::computed(1.0)),
        _ => None,
    };

    // Reactant Mass is the pure reagent; Reactant Mass = Sample Mass × %Weight, so the
    // material to weigh out or measure is the pure mass divided by the assay — the inverse of
    // the conversion in `entered_moles`. A 0 % assay supplies no reagent at any amount.
    let mass = match (moles, mw) {
        (Some(n), Some(mw)) => Some(Quantity::computed(n.value * mw)),
        _ => None,
    };
    let assay = r.percent_weight.unwrap_or(100.0) / 100.0;
    let material_mass = mass.filter(|_| assay > 0.0).map(|m| Quantity::computed(m.value / assay));

    // Volume: entered wins; else derive it from a density (neat liquid) or a molarity (stock).
    let volume = match (r.volume, r.density, r.molarity, material_mass, moles) {
        (Some(v), ..) => Some(Quantity::entered(v)),
        (None, Some(d), _, Some(m), _) if d > 0.0 => Some(Quantity::computed(m.value / d)),
        (None, None, Some(c), _, Some(n)) if c > 0.0 => {
            Some(Quantity::computed(n.value / c * 1000.0))
        }
        _ => None,
    };

    ReactantAnalysis {
        name: r.name.clone(),
        formula: r.formula(),
        mw,
        limiting: is_limiting,
        limiting_origin,
        equivalents,
        // What you weigh out: the entered mass, or — for a reagent whose amount follows from
        // its equivalents — the mass you *would* weigh out, back-filled like ChemDraw does.
        sample_mass: match r.sample_mass {
            Some(m) => Some(Quantity::entered(m.as_grams())),
            None => material_mass,
        },
        percent_weight: r.percent_weight.map(Quantity::entered),
        molarity: r.molarity.map(Quantity::entered),
        density: r.density.map(Quantity::entered),
        volume,
        moles,
        mass,
    }
}

fn analyse_product(p: &Product, limiting_moles: Option<f64>) -> ProductAnalysis {
    let formula = MolecularFormula::of(&p.molecule);
    let mw = formula.weight();
    let completion = p.percent_completion.unwrap_or(100.0) / 100.0;

    let expected_moles = limiting_moles.map(|n| Quantity::computed(n * p.coefficient * completion));
    let expected_mass = match (expected_moles, mw) {
        (Some(n), Some(mw)) => Some(Quantity::computed(n.value * mw)),
        _ => None,
    };

    let purity_factor = p.purity.unwrap_or(100.0) / 100.0;
    let mass = p
        .measured_mass
        .map(|m| Quantity::computed(m.as_grams() * purity_factor));
    let moles = match (mass, mw) {
        (Some(m), Some(mw)) if mw > 0.0 => Some(Quantity::computed(m.value / mw)),
        _ => None,
    };
    let percent_yield = match (moles, expected_moles) {
        (Some(got), Some(want)) if want.value > 0.0 => {
            Some(Quantity::computed(got.value / want.value * 100.0))
        }
        _ => None,
    };

    ProductAnalysis {
        name: p.name.clone(),
        formula,
        mw,
        equivalents: p.equivalents.map(Quantity::entered),
        percent_completion: p.percent_completion.map(Quantity::entered),
        expected_mass,
        expected_moles,
        measured_mass: p.measured_mass.map(|m| Quantity::entered(m.as_grams())),
        purity: p.purity.map(Quantity::entered),
        mass,
        moles,
        percent_yield,
    }
}

// ─── Display formatting ──────────────────────────────────────────────────────

/// A number formatted for the table: the digits, and the unit that follows them in a
/// smaller font (empty when the value is dimensionless).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Formatted {
    pub number: String,
    pub unit: &'static str,
}

impl Formatted {
    fn new(value: f64, unit: &'static str) -> Self {
        Self { number: format!("{value:.2}"), unit }
    }

    /// `"700.00mg"` — how the value reads as one string (used by tests and `Debug` output).
    pub fn joined(&self) -> String {
        format!("{}{}", self.number, self.unit)
    }
}

/// Mass in the largest unit that keeps the number ≥ 1: `1.00g`, `700.00mg`, `1.20kg`.
pub fn format_mass(grams: f64) -> Formatted {
    let g = grams.abs();
    if g >= 1000.0 {
        Formatted::new(grams / 1000.0, "kg")
    } else if g >= 1.0 || g == 0.0 {
        Formatted::new(grams, "g")
    } else {
        Formatted::new(grams * 1000.0, "mg")
    }
}

/// Amount of substance, defaulting to mmol — the working unit of a bench reaction:
/// `10.63mmol`, `1.50mol`, `850.00µmol`.
pub fn format_moles(moles: f64) -> Formatted {
    let n = moles.abs();
    if n >= 1.0 {
        Formatted::new(moles, "mol")
    } else if n >= 1e-3 || n == 0.0 {
        Formatted::new(moles * 1e3, "mmol")
    } else {
        Formatted::new(moles * 1e6, "µmol")
    }
}

/// `60.92%`.
pub fn format_percent(value: f64) -> Formatted {
    Formatted::new(value, "%")
}

/// A bare number: equivalents, molecular weight.
pub fn format_number(value: f64) -> Formatted {
    Formatted::new(value, "")
}

/// Volume in mL (or L above 1000 mL).
pub fn format_volume(ml: f64) -> Formatted {
    if ml.abs() >= 1000.0 {
        Formatted::new(ml / 1000.0, "L")
    } else {
        Formatted::new(ml, "mL")
    }
}

/// Concentration in mol/L.
pub fn format_molarity(molar: f64) -> Formatted {
    Formatted::new(molar, "M")
}

/// Density in g/mL.
pub fn format_density(g_per_ml: f64) -> Formatted {
    Formatted::new(g_per_ml, "g/mL")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::demo;
    use crate::reaction::model::Mass;

    fn figure() -> AnalysisResult {
        demo::williamson_ether_synthesis().analysis.compute()
    }

    /// Every number of the reference figure, formatted exactly as it is printed there.
    #[test]
    fn reference_figure_numbers() {
        let a = figure();
        assert_eq!(a.limiting_index, Some(0));

        let names = ["phenol", "MeI", "K2CO3"];
        let formulas = ["C6H6O", "CH3I", "CK2O3"];
        let mws = ["94.11", "141.94", "138.20"];
        let equivs = ["1.00", "1.10", "3.00"];
        let moles = ["10.63mmol", "11.69mmol", "31.88mmol"];
        let masses = ["1.00g", "1.66g", "4.41g"];

        for (i, r) in a.reactants.iter().enumerate() {
            let who = names[i];
            assert_eq!(r.formula.plain(), formulas[i], "{who} formula");
            assert_eq!(format!("{:.2}", r.mw.unwrap()), mws[i], "{who} MW");
            assert_eq!(
                format_number(r.equivalents.unwrap().value).number,
                equivs[i],
                "{who} equivalents"
            );
            assert_eq!(
                format_moles(r.moles.unwrap().value).joined(),
                moles[i],
                "{who} reactant moles"
            );
            assert_eq!(
                format_mass(r.mass.unwrap().value).joined(),
                masses[i],
                "{who} reactant mass"
            );
        }

        // Sample Mass is filled for every reactant: entered for the weighed-out phenol,
        // back-filled with the required amount for the two derived from equivalents.
        for (i, want) in masses.iter().enumerate() {
            let sample = a.reactants[i].sample_mass.expect("sample mass is always shown");
            assert_eq!(format_mass(sample.value).joined(), *want, "{} sample mass", names[i]);
            assert_eq!(sample.is_entered(), i == 0, "{} sample mass origin", names[i]);
        }

        let p = &a.products[0];
        assert_eq!(p.formula.plain(), "C7H8O");
        assert_eq!(format!("{:.2}", p.mw.unwrap()), "108.14");
        assert_eq!(format_mass(p.expected_mass.unwrap().value).joined(), "1.15g");
        assert_eq!(
            format_moles(p.expected_moles.unwrap().value).joined(),
            "10.63mmol"
        );
        assert_eq!(
            format_mass(p.measured_mass.unwrap().value).joined(),
            "700.00mg"
        );
        assert_eq!(format_mass(p.mass.unwrap().value).joined(), "700.00mg");
        assert_eq!(format_moles(p.moles.unwrap().value).joined(), "6.47mmol");
        assert_eq!(
            format_percent(p.percent_yield.unwrap().value).joined(),
            "60.92%"
        );
    }

    /// The cells that are blank in the figure stay `None` — the rows exist, the values don't.
    #[test]
    fn unused_entries_stay_empty() {
        let a = figure();
        for r in &a.reactants {
            assert!(r.percent_weight.is_none());
            assert!(r.molarity.is_none());
            assert!(r.density.is_none());
            assert!(r.volume.is_none());
        }
        let p = &a.products[0];
        assert!(p.equivalents.is_none());
        assert!(p.percent_completion.is_none());
        assert!(p.purity.is_none());
    }

    /// Entered vs computed drives the bold-italic styling, so pin it down.
    #[test]
    fn origins_match_the_figure() {
        let a = figure();
        assert!(a.reactants[0].limiting);
        assert!(!a.reactants[1].limiting);
        for r in &a.reactants {
            assert!(r.equivalents.unwrap().is_entered(), "equivalents are entered");
            assert!(!r.moles.unwrap().is_entered(), "moles are computed");
            assert!(!r.mass.unwrap().is_entered(), "reactant mass is computed");
        }
        assert!(a.reactants[0].sample_mass.unwrap().is_entered(), "phenol was weighed out");
        assert!(
            !a.reactants[1].sample_mass.unwrap().is_entered(),
            "a back-filled sample mass is a computed value"
        );

        let p = &a.products[0];
        assert!(p.measured_mass.unwrap().is_entered());
        assert!(!p.mass.unwrap().is_entered());
        assert!(!p.expected_mass.unwrap().is_entered());
        assert!(!p.percent_yield.unwrap().is_entered());
    }

    /// Liquids: `volume = mass / density`. Solutions: `moles = molarity × volume`.
    #[test]
    fn liquid_and_solution_reagents() {
        use crate::reaction::model::{Analysis, Reactant};

        // A neat liquid co-reagent: 1.10 eq of MeI (d = 2.28 g/mL) → 1.66 g → 0.73 mL.
        let a = Analysis::new()
            .reactant(
                Reactant::new(demo::phenol())
                    .limiting()
                    .sample_mass(Mass::grams(1.0)),
            )
            .reactant(
                Reactant::new(demo::methyl_iodide())
                    .equivalents(1.10)
                    .density(2.28),
            )
            .compute();
        let mei = &a.reactants[1];
        assert_eq!(format_mass(mei.mass.unwrap().value).joined(), "1.66g");
        let volume = mei.volume.unwrap();
        assert!(!volume.is_entered());
        assert_eq!(format_volume(volume.value).joined(), "0.73mL");

        // A stock solution defines its own amount: 5.00 mL of 2.00 M is 10.00 mmol, and it
        // becomes the limiting reagent when nothing else is flagged.
        let a = Analysis::new()
            .reactant(Reactant::new(demo::methyl_iodide()).molarity(2.0).volume(5.0))
            .compute();
        assert_eq!(a.limiting_index, Some(0));
        assert_eq!(
            format_moles(a.reactants[0].moles.unwrap().value).joined(),
            "10.00mmol"
        );

        // %Weight is an assay: 2.00 g of 50 % material is 1.00 g of reagent.
        let a = Analysis::new()
            .reactant(
                Reactant::new(demo::phenol())
                    .limiting()
                    .sample_mass(Mass::grams(2.0))
                    .percent_weight(50.0),
            )
            .compute();
        assert_eq!(
            format_mass(a.reactants[0].mass.unwrap().value).joined(),
            "1.00g"
        );
        assert_eq!(
            format_moles(a.reactants[0].moles.unwrap().value).joined(),
            "10.63mmol"
        );
    }

    /// %Weight on a reagent measured by equivalents: Reactant Mass stays the pure reagent,
    /// while the back-filled Sample Mass and the density-derived Volume are of the impure
    /// material — the inverse of how an entered sample is turned into moles.
    #[test]
    fn assay_scales_back_filled_sample_mass_and_volume() {
        use crate::reaction::model::{Analysis, Reactant};

        let a = Analysis::new()
            .reactant(
                Reactant::new(demo::phenol())
                    .limiting()
                    .sample_mass(Mass::grams(1.0)),
            )
            .reactant(Reactant::new(demo::methyl_iodide()).equivalents(1.0).percent_weight(50.0))
            .reactant(
                Reactant::new(demo::methyl_iodide())
                    .equivalents(1.0)
                    .percent_weight(50.0)
                    .density(2.28),
            )
            .compute();

        for r in &a.reactants[1..] {
            let pure = r.mass.unwrap().value;
            let sample = r.sample_mass.unwrap();
            assert!(!sample.is_entered());
            // 1.00 eq of MeI is 1.51 g of MeI, so 3.02 g of 50 % material is weighed out.
            assert_eq!(format_mass(pure).joined(), "1.51g");
            assert!((sample.value - 2.0 * pure).abs() < 1e-9, "sample mass = mass / assay");
        }

        // Feeding the derived volume back through `density × volume × assay / MW` gives the
        // same moles.
        let mei = &a.reactants[2];
        let volume = mei.volume.unwrap();
        assert!(!volume.is_entered());
        let moles = 2.28 * volume.value * 0.5 / mei.mw.unwrap();
        assert!((moles - mei.moles.unwrap().value).abs() < 1e-12);
        assert_eq!(format_volume(volume.value).joined(), "1.32mL");

        // And the round trip holds the other way: entering the back-filled sample mass as a
        // weighed amount reproduces the same moles.
        let back = Analysis::new()
            .reactant(
                Reactant::new(demo::methyl_iodide())
                    .sample_mass(Mass::grams(a.reactants[1].sample_mass.unwrap().value))
                    .percent_weight(50.0),
            )
            .compute();
        assert!(
            (back.reactants[0].moles.unwrap().value - a.reactants[1].moles.unwrap().value).abs()
                < 1e-12
        );
    }

    /// %Completion scales the expectation, purity scales what was actually isolated.
    #[test]
    fn completion_and_purity_scale_the_product() {
        use crate::reaction::model::{Analysis, Product, Reactant};
        let a = Analysis::new()
            .reactant(
                Reactant::new(demo::phenol())
                    .limiting()
                    .sample_mass(Mass::grams(1.0)),
            )
            .product(
                Product::new(demo::anisole())
                    .percent_completion(50.0)
                    .measured_mass(Mass::milligrams(700.0))
                    .purity(90.0),
            )
            .compute();
        let p = &a.products[0];
        // Half the limiting moles are expected …
        assert_eq!(
            format_moles(p.expected_moles.unwrap().value).joined(),
            "5.31mmol"
        );
        // … and only 90 % of the 700 mg is product.
        assert_eq!(format_mass(p.mass.unwrap().value).joined(), "630.00mg");
        assert_eq!(format_moles(p.moles.unwrap().value).joined(), "5.83mmol");
        assert_eq!(
            format_percent(p.percent_yield.unwrap().value).joined(),
            "109.66%"
        );
    }

    #[test]
    fn units_switch_at_the_right_magnitudes() {
        assert_eq!(format_mass(1.0).joined(), "1.00g");
        assert_eq!(format_mass(0.7).joined(), "700.00mg");
        assert_eq!(format_mass(0.9999).joined(), "999.90mg");
        assert_eq!(format_mass(1200.0).joined(), "1.20kg");
        assert_eq!(format_mass(0.0).joined(), "0.00g");
        assert_eq!(format_moles(0.010_625_6).joined(), "10.63mmol");
        assert_eq!(format_moles(1.5).joined(), "1.50mol");
        assert_eq!(format_moles(8.5e-4).joined(), "850.00µmol");
        assert_eq!(format_percent(60.9199).joined(), "60.92%");
        assert_eq!(format_number(1.0).joined(), "1.00");
    }
}
