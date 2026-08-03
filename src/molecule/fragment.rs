use serde::{Deserialize, Serialize};
use super::{BondOrder, Molecule};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragAtom {
    pub element: String,
    pub pos: [f32; 2],
    #[serde(default)]
    pub charge: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragBond {
    pub begin: usize,
    pub end: usize,
    pub order: BondOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    #[allow(dead_code)]
    pub name: String,
    pub atoms: Vec<FragAtom>,
    pub bonds: Vec<FragBond>,
    pub attach_idx: usize,
}

/// Fragments the crate builds in code rather than reading from JSON.
///
/// These are the *parametric* ones — an n-gon or an n-carbon chain is a formula, not a table of
/// coordinates, so there is nothing here for a JSON file to duplicate. Every named group
/// (OH, NH₂, CF₃, SiH₃, Boc, Cbz, …) lives in `assets/fragments/*.json` and reaches the editor
/// through [`Config::resolve_fragment`](crate::Config::resolve_fragment), which consults its
/// fragment list *first* and only falls through to [`Fragment::ring`] and [`Fragment::benzene`].
///
/// Hand-written copies of those groups used to sit here as well, and being unreachable is
/// exactly how they went wrong: they were still spaced 1.0 apart long after the JSON moved to
/// [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH), and nothing failed, because
/// nothing called them. They are gone; `assets/fragments/` is the single source.
#[allow(dead_code)]
impl Fragment {
    /// Regular n-membered carbocycle.
    pub fn ring(n: usize) -> Self {
        assert!(n >= 3);
        let positions = ring_positions(n);
        let atoms = positions
            .iter()
            .map(|&pos| FragAtom { element: "C".into(), pos, charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|k| FragBond { begin: k, end: (k + 1) % n, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("ring{n}"), atoms, bonds, attach_idx: 0 }
    }

    /// Benzene with alternating single/double bonds.
    pub fn benzene() -> Self {
        let n = 6;
        let positions = ring_positions(n);
        let atoms = positions
            .iter()
            .map(|&pos| FragAtom { element: "C".into(), pos, charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|k| FragBond {
                begin: k,
                end: (k + 1) % n,
                order: if k % 2 == 0 { BondOrder::Single } else { BondOrder::Double },
            })
            .collect();
        Fragment { name: "benzene".into(), atoms, bonds, attach_idx: 0 }
    }

    /// Linear chain of `n` carbons. atom[0] is attach point at origin.
    ///
    /// Spaced by [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH), like everything
    /// else the editor draws — fragment coordinates are absolute molecular units and are never
    /// rescaled on insertion, so a unit step here would draw a chain two thirds the size of the
    /// ring beside it.
    pub fn chain(n: usize) -> Self {
        assert!(n >= 1);
        let l = crate::widget::DEFAULT_BOND_LENGTH;
        let atoms = (0..=n)
            .map(|i| FragAtom { element: "C".into(), pos: [i as f32 * l, 0.0], charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|i| FragBond { begin: i, end: i + 1, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("chain{n}"), atoms, bonds, attach_idx: 0 }
    }

    /// Zigzag chain of `n` carbons (alternating ±60° from main axis), at
    /// [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH) per bond.
    pub fn zigzag(n: usize) -> Self {
        assert!(n >= 1);
        let l = crate::widget::DEFAULT_BOND_LENGTH;
        let angle_step = std::f32::consts::PI / 3.0; // 60°
        let mut atoms: Vec<FragAtom> = vec![FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 }];
        let mut x = 0.0_f32;
        let mut y = 0.0_f32;
        for i in 0..n {
            let dy = if i % 2 == 0 { angle_step.sin() } else { -angle_step.sin() };
            x += angle_step.cos() * l;
            y += dy * l;
            atoms.push(FragAtom { element: "C".into(), pos: [x, y], charge: 0 });
        }
        let bonds = (0..n)
            .map(|i| FragBond { begin: i, end: i + 1, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("zigzag{n}"), atoms, bonds, attach_idx: 0 }
    }
}

// ─── JSON I/O and Molecule interop ────────────────────────────────────────────

impl Fragment {
    /// Parse a fragment from its JSON representation (the on-disk `fragments/<name>.json` format).
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        serde_json::from_str(s)
    }

    /// Pretty-print the fragment as JSON, matching the on-disk fragment format.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Load a fragment from a JSON file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save the fragment to a JSON file.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Materialize this fragment as a standalone [`Molecule`]. Atoms are added in fragment order,
    /// so `molecule.atoms[i]` corresponds to `self.atoms[i]` (and the attach atom is at
    /// `self.attach_idx`).
    pub fn to_molecule(&self) -> Molecule {
        let mut mol = Molecule::default();
        mol.name = self.name.clone();
        let ids: Vec<u32> = self
            .atoms
            .iter()
            .map(|a| mol.add_atom(a.element.clone(), a.pos, a.charge))
            .collect();
        for b in &self.bonds {
            mol.add_bond(ids[b.begin], ids[b.end], b.order.clone());
        }
        mol
    }

    /// Rebuild a fragment from a molecule (e.g. after `cleanup_2d`), taking each atom's element,
    /// position and charge and re-indexing bond endpoints by atom order. `attach_idx` is carried
    /// over unchanged, so pass the same value used to build the molecule.
    pub fn from_molecule(mol: &Molecule, attach_idx: usize) -> Self {
        let index_of: std::collections::HashMap<u32, usize> =
            mol.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        let atoms = mol
            .atoms
            .iter()
            .map(|a| FragAtom { element: a.element.clone(), pos: a.pos, charge: a.charge })
            .collect();
        let bonds = mol
            .bonds
            .iter()
            .filter_map(|b| {
                Some(FragBond {
                    begin: *index_of.get(&b.begin)?,
                    end: *index_of.get(&b.end)?,
                    order: b.order.clone(),
                })
            })
            .collect();
        Fragment { name: mol.name.clone(), atoms, bonds, attach_idx }
    }
}

/// Which of a fragment's atoms sit on one of its rings.
///
/// Leaves are pruned until none are left; whatever survives is on a cycle. Used by
/// [`Molecule::fragment_atoms_lost_to_snap`](crate::Molecule::fragment_atoms_lost_to_snap) to
/// tell a wanted ring fusion from a substituent that would be silently re-rooted.
pub fn ring_membership(frag: &Fragment) -> Vec<bool> {
    let n = frag.atoms.len();
    let mut alive = vec![true; n];
    let mut degree = vec![0usize; n];
    for b in &frag.bonds {
        if b.begin < n && b.end < n && b.begin != b.end {
            degree[b.begin] += 1;
            degree[b.end] += 1;
        }
    }
    loop {
        let Some(leaf) = (0..n).find(|&i| alive[i] && degree[i] <= 1) else { break };
        alive[leaf] = false;
        degree[leaf] = 0;
        for b in &frag.bonds {
            if b.begin >= n || b.end >= n || b.begin == b.end {
                continue;
            }
            let other = if b.begin == leaf {
                b.end
            } else if b.end == leaf {
                b.begin
            } else {
                continue;
            };
            if alive[other] {
                degree[other] = degree[other].saturating_sub(1);
            }
        }
    }
    alive
}

/// Regular n-gon positions: atom[0]=(0,0), atom[1]=(L,0), ring below, where L is
/// [`DEFAULT_BOND_LENGTH`].
///
/// The edge length is not decorative. Fragment coordinates are *absolute* molecular units —
/// [`Molecule::insert_fragment`] places a fragment on an atom by translation and rotation
/// only, with no scaling — so whatever this returns is the size the ring appears on screen.
/// Every hand-authored fragment (`assets/fragments/*.json`) spaces its bonds by 1.5, and so
/// do chains and [`crate::widget::interact`]'s ring fusion; a **unit** n-gon here therefore
/// drew ring-key rings at two thirds the size of everything beside them.
pub fn ring_positions(n: usize) -> Vec<[f32; 2]> {
    use std::f32::consts::{PI, TAU};
    let l = crate::widget::DEFAULT_BOND_LENGTH;
    let r = l / (2.0 * (PI / n as f32).sin());
    let center_y = -(r * r - l * l * 0.25).sqrt();
    let cx = l * 0.5;
    let cy = center_y;
    let alpha_0 = (0.0_f32 - cy).atan2(0.0_f32 - cx);
    (0..n)
        .map(|k| {
            let angle = alpha_0 - k as f32 * TAU / n as f32;
            [cx + r * angle.cos(), cy + r * angle.sin()]
        })
        .collect()
}

#[allow(dead_code)]
impl BondOrder {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Double" => BondOrder::Double,
            "Triple" => BondOrder::Triple,
            _ => BondOrder::Single,
        }
    }
}
