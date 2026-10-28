//! Monocyclic heterocycles: the rings the namer knows by name, and how to recognise them.
//!
//! A heterocycle is named from a fixed table (IUPAC 2013, P-22.2): the retained names that are
//! preferred IUPAC names (pyridine, furan, thiophene, pyrrole, imidazole, pyrazole, the
//! diazines, piperidine, pyrrolidine, piperazine, morpholine) and Hantzsch–Widman names
//! (oxirane, oxolane, oxane, thiolane, 1,3-oxazole, 1,3,5-triazine, 1,4-dioxane, …). Each entry
//! fixes the numbering of the ring: the heteroatom at each locant, from 1 up. A ring is either
//! fully unsaturated (mancude: as many non-cumulative double bonds as it can have, drawn in
//! Kekulé form) or saturated; partly hydrogenated rings ("2,3-dihydrofuran"), rings with
//! indicated hydrogen on carbon ("2H-pyran") and any ring not in the table are refused.

use super::graph::{El, Graph, Unsupported};

/// A heterocycle with a name and a fixed numbering.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct HeteroRing {
    pub name: &'static str,
    /// The element at each locant, from 1.
    pub atoms: &'static [El],
    /// Fully unsaturated (pyridine, furan) rather than saturated (piperidine, oxolane).
    pub mancude: bool,
    /// The name carries the indicated hydrogen "1H-" (1H-pyrrole): the atom at locant 1 is a
    /// nitrogen with a hydrogen or a substituent.
    pub indicated: bool,
}

impl HeteroRing {
    /// The locants of the heteroatoms.
    pub(crate) fn hetero_locants(&self) -> Vec<usize> {
        (1..=self.atoms.len())
            .filter(|&l| self.atoms[l - 1] != El::C)
            .collect()
    }
}

const fn ring(name: &'static str, atoms: &'static [El], mancude: bool) -> HeteroRing {
    HeteroRing {
        name,
        atoms,
        mancude,
        indicated: false,
    }
}

const fn ring_h(name: &'static str, atoms: &'static [El]) -> HeteroRing {
    HeteroRing {
        name,
        atoms,
        mancude: true,
        indicated: true,
    }
}

use El::{C, N, O, S};

/// The heterocycles the namer knows.
pub(crate) const RINGS: &[HeteroRing] = &[
    // Six-membered, fully unsaturated.
    ring("pyridine", &[N, C, C, C, C, C], true),
    ring("pyridazine", &[N, N, C, C, C, C], true),
    ring("pyrimidine", &[N, C, N, C, C, C], true),
    ring("pyrazine", &[N, C, C, N, C, C], true),
    ring("1,3,5-triazine", &[N, C, N, C, N, C], true),
    ring("1,2,4-triazine", &[N, N, C, N, C, C], true),
    ring("1,2,3-triazine", &[N, N, N, C, C, C], true),
    // Five-membered, fully unsaturated.
    ring("furan", &[O, C, C, C, C], true),
    ring("thiophene", &[S, C, C, C, C], true),
    ring_h("pyrrole", &[N, C, C, C, C]),
    ring_h("imidazole", &[N, C, N, C, C]),
    ring_h("pyrazole", &[N, N, C, C, C]),
    ring("1,3-oxazole", &[O, C, N, C, C], true),
    ring("1,2-oxazole", &[O, N, C, C, C], true),
    ring("1,3-thiazole", &[S, C, N, C, C], true),
    ring("1,2-thiazole", &[S, N, C, C, C], true),
    // Saturated.
    ring("oxirane", &[O, C, C], false),
    ring("aziridine", &[N, C, C], false),
    ring("thiirane", &[S, C, C], false),
    ring("oxetane", &[O, C, C, C], false),
    ring("azetidine", &[N, C, C, C], false),
    ring("thietane", &[S, C, C, C], false),
    ring("oxolane", &[O, C, C, C, C], false),
    ring("pyrrolidine", &[N, C, C, C, C], false),
    ring("thiolane", &[S, C, C, C, C], false),
    ring("1,3-dioxolane", &[O, C, O, C, C], false),
    ring("oxane", &[O, C, C, C, C, C], false),
    ring("piperidine", &[N, C, C, C, C, C], false),
    ring("thiane", &[S, C, C, C, C, C], false),
    ring("1,3-dioxane", &[O, C, O, C, C, C], false),
    ring("1,4-dioxane", &[O, C, C, O, C, C], false),
    ring("morpholine", &[O, C, C, N, C, C], false),
    ring("piperazine", &[N, C, C, N, C, C], false),
    ring("oxepane", &[O, C, C, C, C, C, C], false),
    ring("azepane", &[N, C, C, C, C, C, C], false),
    ring("thiepane", &[S, C, C, C, C, C, C], false),
];

/// A recognised heterocycle of a structure.
pub(crate) struct Hetero {
    pub kind: &'static HeteroRing,
    /// Every numbering the ring allows, each its atoms in the order of their locants.
    pub numberings: Vec<Vec<usize>>,
}

/// Recognise the heterocycle whose atoms, in cyclic order, are `ring`.
pub(crate) fn recognise(g: &Graph, ring: &[usize]) -> Result<Hetero, Unsupported> {
    let n = ring.len();
    let orders: Vec<u8> = (0..n)
        .map(|i| g.order(ring[i], ring[(i + 1) % n]))
        .collect();
    // Ring double bonds at each atom.
    let doubles: Vec<usize> = (0..n)
        .map(|i| {
            [orders[i], orders[(i + n - 1) % n]]
                .iter()
                .filter(|&&o| o == 2)
                .count()
        })
        .collect();
    let saturated = orders.iter().all(|&o| o == 1);
    let mancude = orders.iter().all(|&o| o <= 2)
        && doubles.iter().all(|&d| d <= 1)
        && doubles.iter().filter(|&&d| d == 0).count() == n % 2;
    if !saturated && !mancude {
        return Err(
            "partly hydrogenated heterocycle, or one with indicated hydrogen or an \
             exocyclic double bond (dihydro…, 2H-pyran, pyridin-2(1H)-one…)"
                .into(),
        );
    }
    // In a five-membered mancude ring the atom without a double bond must take locant 1.
    let sp3 = (0..n).find(|&i| doubles[i] == 0);
    for kind in RINGS
        .iter()
        .filter(|k| k.atoms.len() == n && k.mancude == mancude)
    {
        let mut numberings = Vec::new();
        for s in 0..n {
            for step in [1, n - 1] {
                let idx: Vec<usize> = (0..n).map(|k| (s + k * step) % n).collect();
                let fits = idx
                    .iter()
                    .zip(kind.atoms)
                    .all(|(&i, &e)| g.el[ring[i]] == e);
                if fits && (saturated || sp3.is_none_or(|a| a == idx[0])) {
                    numberings.push(idx.iter().map(|&i| ring[i]).collect());
                }
            }
        }
        if !numberings.is_empty() {
            return Ok(Hetero { kind, numberings });
        }
    }
    if let Some(a) = sp3
        && mancude
        && g.el[ring[a]] == El::C
    {
        return Err("heterocycle with indicated hydrogen on a ring carbon (2H-pyrrole…)".into());
    }
    Err(
        "heterocycle not among the supported monocycles (azoles, azines, furan, thiophene and \
         their saturated forms such as oxolane, piperidine, morpholine)"
            .into(),
    )
}
