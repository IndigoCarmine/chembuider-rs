use crate::molecule::{BondOrder, Molecule};

use super::DEFAULT_BOND_LENGTH;

// ── Geometric helpers ─────────────────────────────────────────────────────────

/// Move one bond-length from `pos` in the direction given by `angle_deg`.
fn step(pos: [f32; 2], angle_deg: f32, len: f32) -> [f32; 2] {
    let r = angle_deg.to_radians();
    [pos[0] + r.cos() * len, pos[1] + r.sin() * len]
}

/// Walk `n_steps` bonds starting from `start`.
/// Each step goes at the current angle, then the angle is incremented by `turn_deg`.
/// Returns all n_steps+1 positions (including the start).
fn walk(start: [f32; 2], start_angle_deg: f32, turn_deg: f32, n_steps: usize, len: f32) -> Vec<[f32; 2]> {
    let mut positions = vec![start];
    let mut pos = start;
    let mut angle = start_angle_deg;
    for _ in 0..n_steps {
        pos = step(pos, angle, len);
        positions.push(pos);
        angle += turn_deg;
    }
    positions
}

// ── AtomFragment ─────────────────────────────────────────────────────────────

/// Fragment that attaches to an existing atom.
///
/// Local coordinate convention:
/// - `atoms[connection]` is placed at `origin`
/// - the fragment's main axis points along +x (rotated by `dir_angle` on insert)
pub struct AtomFragment {
    atoms: Vec<(String, [f32; 2], i8)>, // (element, local_pos, charge)
    bonds: Vec<(usize, usize, BondOrder)>,
    connection: usize,
}

impl AtomFragment {
    /// Insert this fragment.
    ///
    /// - `origin`: world position of the connection atom
    /// - `anchor_id`: if `Some`, reuse that existing atom as the connection point
    /// - `dir_angle`: rotation for the fragment's +x axis
    pub fn insert(
        &self,
        mol: &mut Molecule,
        origin: [f32; 2],
        anchor_id: Option<u32>,
        dir_angle: f32,
    ) {
        let cos = dir_angle.cos();
        let sin = dir_angle.sin();
        let conn_local = self.atoms[self.connection].1;

        let world: Vec<[f32; 2]> = self
            .atoms
            .iter()
            .map(|(_, local, _)| {
                let dx = local[0] - conn_local[0];
                let dy = local[1] - conn_local[1];
                [
                    origin[0] + dx * cos - dy * sin,
                    origin[1] + dx * sin + dy * cos,
                ]
            })
            .collect();

        let ids: Vec<u32> = self
            .atoms
            .iter()
            .enumerate()
            .map(|(i, (element, _, charge))| {
                if i == self.connection {
                    if let Some(id) = anchor_id {
                        return id;
                    }
                }
                mol.add_atom(element.clone(), world[i], *charge)
            })
            .collect();

        for (from, to, order) in &self.bonds {
            mol.add_bond(ids[*from], ids[*to], order.clone());
        }
    }

    // ── factory methods ───────────────────────────────────────────────────

    /// C=O: new carbon (connection) double-bonded to oxygen.
    /// O is placed one bond-length directly ahead (0°).
    pub fn carbonyl() -> Self {
        let l = DEFAULT_BOND_LENGTH;
        Self {
            atoms: vec![
                ("C".into(), [0.0, 0.0], 0),
                ("O".into(), step([0.0, 0.0], 0.0, l), 0),
            ],
            bonds: vec![(0, 1, BondOrder::Double)],
            connection: 0,
        }
    }

    /// -C(=O)-N: carbonyl carbon (connection) with O branching at −60° and N ahead at 0°.
    pub fn amide() -> Self {
        let l = DEFAULT_BOND_LENGTH;
        Self {
            atoms: vec![
                ("C".into(), [0.0, 0.0], 0),
                ("O".into(), step([0.0, 0.0], -120.0, l), 0),
                ("N".into(), step([0.0, 0.0],   0.0, l), 0),
            ],
            bonds: vec![
                (0, 1, BondOrder::Double),
                (0, 2, BondOrder::Single),
            ],
            connection: 0,
        }
    }

    /// -C(=O)-O: carbonyl carbon (connection) with O=C branch at −60° and single O ahead at 0°.
    pub fn ester() -> Self {
        let l = DEFAULT_BOND_LENGTH;
        Self {
            atoms: vec![
                ("C".into(), [0.0, 0.0], 0),
                ("O".into(), step([0.0, 0.0], -120.0, l), 0),
                ("O".into(), step([0.0, 0.0],   0.0, l), 0),
            ],
            bonds: vec![
                (0, 1, BondOrder::Double),
                (0, 2, BondOrder::Single),
            ],
            connection: 0,
        }
    }

    /// Benzene ring: atom 0 is the connection vertex, ring extends along +x.
    ///
    /// Walks the hexagon starting at atom 0, first bond at −60° (up-right on screen),
    /// turning +60° each step to traverse clockwise visually.
    ///
    /// ```text
    ///   1   2
    ///  / \ / \
    /// 0       3
    ///  \ / \ /
    ///   5   4
    /// ```
    pub fn benzene() -> Self {
        let l = DEFAULT_BOND_LENGTH;
        // start_angle=-60°, turn=+60°: walks the upper half first then lower half
        let pts = walk([0.0, 0.0], -60.0, 120.0, 5, l);
        Self {
            atoms: pts.into_iter().map(|p| ("C".into(), p, 0)).collect(),
            bonds: vec![
                (0, 1, BondOrder::Single),
                (1, 2, BondOrder::Double),
                (2, 3, BondOrder::Single),
                (3, 4, BondOrder::Double),
                (4, 5, BondOrder::Single),
                (5, 0, BondOrder::Double),
            ],
            connection: 0,
        }
    }
}

// ── BondFragment ──────────────────────────────────────────────────────────────

/// Fragment that fuses onto an existing bond.
///
/// `atoms[begin]` and `atoms[end]` map to the two endpoints of the target bond;
/// they are NOT added as new atoms — the existing atoms are reused.
/// All other atoms are new.
///
/// Local coordinate convention: begin → end lies along +x with length DEFAULT_BOND_LENGTH.
/// The ring extends into the negative-y half-plane (= upward on screen).
pub struct BondFragment {
    atoms: Vec<(String, [f32; 2], i8)>,
    bonds: Vec<(usize, usize, BondOrder)>,
    begin: usize,
    end: usize,
}

impl BondFragment {
    /// Fuse this fragment onto the bond identified by `bond_id`.
    ///
    /// 1. Reads begin/end atom positions from the molecule.
    /// 2. Computes dir_angle and scale from the actual bond geometry.
    /// 3. Places new atoms; reuses existing atoms for `begin`/`end` slots.
    /// 4. Adds new bonds; updates the existing bond's order if needed.
    pub fn insert(&self, mol: &mut Molecule, bond_id: u32) {
        let (begin_id, end_id, begin_pos, end_pos) = {
            let bond = match mol.bond_by_id(bond_id) {
                Some(b) => b,
                None => return,
            };
            let bp = mol.atom_by_id(bond.begin).map(|a| a.pos).unwrap_or([0.0; 2]);
            let ep = mol.atom_by_id(bond.end).map(|a| a.pos).unwrap_or([0.0; 2]);
            (bond.begin, bond.end, bp, ep)
        };

        let dx = end_pos[0] - begin_pos[0];
        let dy = end_pos[1] - begin_pos[1];
        let actual_len = (dx * dx + dy * dy).sqrt().max(1e-6);
        let dir_angle = dy.atan2(dx);
        let scale = actual_len / DEFAULT_BOND_LENGTH;

        let cos = dir_angle.cos();
        let sin = dir_angle.sin();
        let begin_local = self.atoms[self.begin].1;

        let world: Vec<[f32; 2]> = self
            .atoms
            .iter()
            .map(|(_, local, _)| {
                let lx = (local[0] - begin_local[0]) * scale;
                let ly = (local[1] - begin_local[1]) * scale;
                [
                    begin_pos[0] + lx * cos - ly * sin,
                    begin_pos[1] + lx * sin + ly * cos,
                ]
            })
            .collect();

        let ids: Vec<u32> = self
            .atoms
            .iter()
            .enumerate()
            .map(|(i, (element, _, charge))| {
                if i == self.begin {
                    begin_id
                } else if i == self.end {
                    end_id
                } else {
                    mol.add_atom(element.clone(), world[i], *charge)
                }
            })
            .collect();

        for (from, to, order) in &self.bonds {
            let fa = ids[*from];
            let ta = ids[*to];
            if let Some(existing) = mol.bond_between(fa, ta).map(|b| b.id) {
                if let Some(b) = mol.bond_by_id_mut(existing) {
                    b.order = order.clone();
                }
            } else {
                mol.add_bond(fa, ta, order.clone());
            }
        }
    }

    // ── factory methods ───────────────────────────────────────────────────

    /// Benzene ring fused onto an existing bond.
    ///
    /// Walks the hexagon from begin (0°), turning −60° each step so the ring
    /// extends upward (−y = above on screen).
    ///
    /// ```text
    ///  5   4   3
    ///   \ / \ / \
    ///    0   ?   2
    ///     \ / \ /
    ///      begin=0, end=1
    /// ```
    /// Local layout (begin at origin, end at (L, 0)):
    /// ```text
    /// 5(−60°×L)  4(0, −√3·L)  3(L, −√3·L)  2(+60°from 1)
    /// 0(0,0)                                  1(L, 0)
    /// ```
    pub fn benzene() -> Self {
        let l = DEFAULT_BOND_LENGTH;
        // start_angle=0°, turn=−60°: begin→end is the base, ring wraps above (−y)
        let pts = walk([0.0, 0.0], 0.0, -120.0, 5, l);
        Self {
            atoms: pts.into_iter().map(|p| ("C".into(), p, 0)).collect(),
            bonds: vec![
                (0, 1, BondOrder::Single),  // existing bond — update order to Single
                (1, 2, BondOrder::Double),
                (2, 3, BondOrder::Single),
                (3, 4, BondOrder::Double),
                (4, 5, BondOrder::Single),
                (5, 0, BondOrder::Double),
            ],
            begin: 0,
            end: 1,
        }
    }
}

// ── Fragment enum ─────────────────────────────────────────────────────────────

pub enum Fragment {
    Atom(AtomFragment),
    Bond(BondFragment),
}
