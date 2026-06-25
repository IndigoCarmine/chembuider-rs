pub mod mol2;
pub mod fragment;
pub mod cleanup;
#[cfg(windows)]
pub mod cdx;
#[cfg(windows)]
pub mod image;
#[cfg(windows)]
pub mod ole;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum BondStereo {
    #[default]
    None,
    WedgeUp,   // w: solid wedge (toward viewer), narrow at begin
    WedgeDown, // W/h: hash wedge (away from viewer), narrow at begin
    Bold,      // H/b: thick/heavy bond
    Dashed,    // d: dashed line
    Wavy,      // y: wavy bond (undefined stereo)
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BondOrder {
    Single,
    Double,
    Triple,
}

impl BondOrder {
    pub fn cycle(&self) -> Self {
        match self {
            BondOrder::Single => BondOrder::Double,
            BondOrder::Double => BondOrder::Triple,
            BondOrder::Triple => BondOrder::Single,
        }
    }

    pub fn mol2_type(&self) -> &'static str {
        match self {
            BondOrder::Single => "1",
            BondOrder::Double => "2",
            BondOrder::Triple => "3",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub id: u32,
    pub element: String,
    pub pos: [f32; 2],
    pub charge: i8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bond {
    pub id: u32,
    pub begin: u32,
    pub end: u32,
    pub order: BondOrder,
    pub stereo: BondStereo,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Molecule {
    pub name: String,
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    next_id: u32,
}

impl Default for Molecule {
    fn default() -> Self {
        Self {
            name: "MOL".to_string(),
            atoms: Vec::new(),
            bonds: Vec::new(),
            next_id: 1,
        }
    }
}

impl Molecule {
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_atom(&mut self, element: String, pos: [f32; 2], charge: i8) -> u32 {
        let id = self.alloc_id();
        self.atoms.push(Atom { id, element, pos, charge });
        id
    }

    pub fn add_bond(&mut self, begin: u32, end: u32, order: BondOrder) -> u32 {
        if self.bond_between(begin, end).is_some() {
            return 0;
        }
        let id = self.alloc_id();
        self.bonds.push(Bond { id, begin, end, order, stereo: BondStereo::None });
        id
    }

    pub fn remove_atom(&mut self, id: u32) {
        self.atoms.retain(|a| a.id != id);
        self.bonds.retain(|b| b.begin != id && b.end != id);
    }

    pub fn remove_bond(&mut self, id: u32) {
        self.bonds.retain(|b| b.id != id);
    }

    pub fn atom_by_id(&self, id: u32) -> Option<&Atom> {
        self.atoms.iter().find(|a| a.id == id)
    }

    pub fn atom_by_id_mut(&mut self, id: u32) -> Option<&mut Atom> {
        self.atoms.iter_mut().find(|a| a.id == id)
    }

    pub fn bond_by_id_mut(&mut self, id: u32) -> Option<&mut Bond> {
        self.bonds.iter_mut().find(|b| b.id == id)
    }

    pub fn bonds_for_atom(&self, atom_id: u32) -> Vec<&Bond> {
        self.bonds
            .iter()
            .filter(|b| b.begin == atom_id || b.end == atom_id)
            .collect()
    }

    pub fn bond_between(&self, a: u32, b: u32) -> Option<&Bond> {
        self.bonds.iter().find(|bond| {
            (bond.begin == a && bond.end == b) || (bond.begin == b && bond.end == a)
        })
    }

    /// Find the closest atom within `radius` (molecular units) of `pos`.
    /// Used to snap new atoms onto existing ones instead of creating duplicates.
    pub fn find_atom_near(&self, pos: [f32; 2], radius: f32) -> Option<u32> {
        let r2 = radius * radius;
        self.atoms.iter()
            .filter(|a| {
                let dx = a.pos[0] - pos[0];
                let dy = a.pos[1] - pos[1];
                dx * dx + dy * dy < r2
            })
            .min_by(|a, b| {
                let da = (a.pos[0]-pos[0]).powi(2) + (a.pos[1]-pos[1]).powi(2);
                let db = (b.pos[0]-pos[0]).powi(2) + (b.pos[1]-pos[1]).powi(2);
                da.partial_cmp(&db).unwrap()
            })
            .map(|a| a.id)
    }

    pub fn neighbor_atom_ids(&self, atom_id: u32) -> Vec<u32> {
        self.bonds_for_atom(atom_id)
            .iter()
            .map(|b| if b.begin == atom_id { b.end } else { b.begin })
            .collect()
    }

    /// All atom ids in the same connected molecule as `start` (graph traversal over bonds,
    /// includes `start`).
    pub fn connected_atoms(&self, start: u32) -> Vec<u32> {
        if self.atom_by_id(start).is_none() {
            return vec![];
        }
        let mut seen = std::collections::HashSet::from([start]);
        let mut queue = vec![start];
        while let Some(id) = queue.pop() {
            for n in self.neighbor_atom_ids(id) {
                if seen.insert(n) {
                    queue.push(n);
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Snap radius for merging overlapping atoms (molecular units).
    pub const SNAP_RADIUS: f32 = 0.4;

    /// Fuse a Fragment onto an existing atom.
    /// `attach_to` becomes fragment.atoms[attach_idx].
    /// `angle` = direction (radians) from attach_to toward the next atom.
    /// `flip`  = mirror the fragment across the attach→next axis.
    /// Returns IDs of all newly created atoms.
    pub fn insert_fragment(
        &mut self,
        frag: &fragment::Fragment,
        attach_to: u32,
        angle: f32,
        flip: bool,
    ) -> Vec<u32> {
        let base_pos = match self.atom_by_id(attach_to) {
            Some(a) => a.pos,
            None => return vec![],
        };

        let next_idx = if frag.atoms.len() > 1 {
            (frag.attach_idx + 1) % frag.atoms.len()
        } else {
            return vec![];
        };
        let attach_raw = frag.atoms[frag.attach_idx].pos;
        let next_raw   = frag.atoms[next_idx].pos;
        let raw_angle  = (next_raw[1] - attach_raw[1]).atan2(next_raw[0] - attach_raw[0]);

        // Canonicalize: rotate by -raw_angle, then optionally flip y, then rotate by angle.
        let (sin_neg, cos_neg) = (-raw_angle).sin_cos();
        let (sin_a,   cos_a  ) = angle.sin_cos();

        let mut id_map: Vec<Option<u32>> = vec![None; frag.atoms.len()];
        id_map[frag.attach_idx] = Some(attach_to);

        let mut new_ids: Vec<u32> = Vec::new();
        for (i, fa) in frag.atoms.iter().enumerate() {
            if i == frag.attach_idx { continue; }
            let dx = fa.pos[0] - attach_raw[0];
            let dy = fa.pos[1] - attach_raw[1];
            // Step 1: canonicalize (rotate so raw_angle → 0)
            let dx_c =  dx * cos_neg - dy * sin_neg;
            let dy_c = (dx * sin_neg + dy * cos_neg) * if flip { -1.0 } else { 1.0 };
            // Step 2: rotate to target angle
            let rx = dx_c * cos_a - dy_c * sin_a;
            let ry = dx_c * sin_a + dy_c * cos_a;
            let new_pos = [base_pos[0] + rx, base_pos[1] + ry];
            // Snap: if an existing atom is within SNAP_RADIUS, reuse it instead of creating a duplicate
            let new_id = if let Some(existing) = self.find_atom_near(new_pos, Self::SNAP_RADIUS) {
                existing
            } else {
                self.add_atom(fa.element.to_string(), new_pos, fa.charge)
            };
            id_map[i]   = Some(new_id);
            if !new_ids.contains(&new_id) { new_ids.push(new_id); }
        }

        for fb in &frag.bonds {
            if let (Some(a), Some(b)) = (id_map[fb.begin], id_map[fb.end]) {
                self.add_bond(a, b, fb.order.clone());
            }
        }

        new_ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connected_atoms_finds_one_molecule() {
        let mut m = Molecule::default();
        // Molecule A: a–b–c
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("O".to_string(), [2.0, 0.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        m.add_bond(b, c, BondOrder::Single);
        // Molecule B: a separate, unbonded atom
        let d = m.add_atom("N".to_string(), [5.0, 0.0], 0);

        let mut got = m.connected_atoms(b);
        got.sort();
        let mut want = vec![a, b, c];
        want.sort();
        assert_eq!(got, want, "reaches the whole A component from any of its atoms");
        assert_eq!(m.connected_atoms(d), vec![d], "isolated atom is its own molecule");
    }
}
