use super::Molecule;
use std::collections::{HashMap, HashSet};

const L: f32 = 1.5;
const FORCE_THRESHOLD: f32 = 1e-3;

struct RelaxParams {
    k_bond: f32,
    k_angle: f32,
    k_rep: f32,
    dt: f32,
    damping: f32,
}

const PARAMS: RelaxParams = RelaxParams {
    k_bond: 8.0,
    k_angle: 3.0,
    k_rep: 1.5,
    dt: 0.001,
    damping: 0.05,
};

// ─── Incremental cleanup state ────────────────────────────────────────────────

/// Persistent state for continuous cleanup.  Create with `CleanupState::new`,
/// advance with `step`, and write back to the molecule with `apply`.
pub struct CleanupState {
    pos: HashMap<u32, [f32; 2]>,
    vel: HashMap<u32, [f32; 2]>,
    atom_ids: Vec<u32>,
    forces: HashMap<u32, [f32; 2]>, // reused buffer — avoids per-step allocation
    bonds: Vec<(u32, u32)>,         // cached from mol (immutable during relaxation)
    adj: HashMap<u32, Vec<u32>>,    // cached adjacency list
    bonded: HashSet<(u32, u32)>,    // both orderings, for O(1) repulsion filter
}

impl CleanupState {
    pub fn new(mol: &Molecule) -> Self {
        let atom_ids: Vec<u32> = mol.atoms.iter().map(|a| a.id).collect();
        let pos: HashMap<u32, [f32; 2]> = mol.atoms.iter().map(|a| (a.id, a.pos)).collect();
        let vel: HashMap<u32, [f32; 2]> = atom_ids.iter().map(|&id| (id, [0.0_f32; 2])).collect();
        let forces: HashMap<u32, [f32; 2]> = atom_ids.iter().map(|&id| (id, [0.0_f32; 2])).collect();

        let bonds: Vec<(u32, u32)> = mol.bonds.iter().map(|b| (b.begin, b.end)).collect();

        let mut adj: HashMap<u32, Vec<u32>> = atom_ids.iter().map(|&id| (id, Vec::new())).collect();
        for &(a, b) in &bonds {
            adj.get_mut(&a).unwrap().push(b);
            adj.get_mut(&b).unwrap().push(a);
        }

        let mut bonded: HashSet<(u32, u32)> = HashSet::with_capacity(bonds.len() * 2);
        for &(a, b) in &bonds {
            bonded.insert((a, b));
            bonded.insert((b, a));
        }

        CleanupState { pos, vel, atom_ids, forces, bonds, adj, bonded }
    }

    /// Run `n` force-field steps.  Returns `true` when the simulation has
    /// converged (max force < FORCE_THRESHOLD) and no further steps are needed.
    pub fn step(&mut self, n: usize) -> bool {
        let p = &PARAMS;
        for _ in 0..n {
            compute_forces(
                &self.pos,
                &self.bonds,
                &self.adj,
                &self.atom_ids,
                &self.bonded,
                p,
                &mut self.forces,
            );

            let max_f2 = self.forces
                .values()
                .map(|f| f[0] * f[0] + f[1] * f[1])
                .fold(0.0_f32, f32::max);
            if max_f2 < FORCE_THRESHOLD * FORCE_THRESHOLD {
                return true;
            }

            for &id in &self.atom_ids {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] = v[0] * p.damping + f[0] * p.dt;
                v[1] = v[1] * p.damping + f[1] * p.dt;
                let q = self.pos.get_mut(&id).unwrap();
                q[0] += v[0] * p.dt;
                q[1] += v[1] * p.dt;
            }
            recenter(&mut self.pos, &self.atom_ids);
        }
        false
    }

    /// Write the current relaxed positions back to the molecule.
    pub fn apply(&self, mol: &mut Molecule) {
        for (&id, &p) in &self.pos {
            if let Some(atom) = mol.atom_by_id_mut(id) {
                atom.pos = p;
            }
        }
    }
}

// ─── One-shot entry point (used by interact.rs button etc.) ──────────────────

pub fn cleanup_2d(mol: &mut Molecule) {
    if mol.atoms.is_empty() {
        return;
    }
    let mut state = CleanupState::new(mol);
    state.step(200_000);
    state.apply(mol);
}

// ─── Force field ──────────────────────────────────────────────────────────────

fn compute_forces(
    pos: &HashMap<u32, [f32; 2]>,
    bonds: &[(u32, u32)],
    adj: &HashMap<u32, Vec<u32>>,
    atom_ids: &[u32],
    bonded: &HashSet<(u32, u32)>,
    p: &RelaxParams,
    forces: &mut HashMap<u32, [f32; 2]>,
) {
    for f in forces.values_mut() {
        *f = [0.0, 0.0];
    }

    // Bond stretching: harmonic spring toward target length L
    for &(begin, end) in bonds {
        let pi = pos[&begin];
        let pj = pos[&end];
        let dx = pj[0] - pi[0];
        let dy = pj[1] - pi[1];
        let d = (dx * dx + dy * dy).sqrt().max(0.001);
        let mag = p.k_bond * (d - L);
        let fx = mag * dx / d;
        let fy = mag * dy / d;
        {
            let f = forces.get_mut(&begin).unwrap();
            f[0] += fx;
            f[1] += fy;
        }
        {
            let f = forces.get_mut(&end).unwrap();
            f[0] -= fx;
            f[1] -= fy;
        }
    }

    // Angle bending: tangential forces on consecutive neighbor pairs sorted by angle.
    // degree 2: small gap → 120°, large (reflex) gap → 240°
    // degree 3: all gaps → 120°
    // degree ≥4: equal spacing 360°/n
    for &c_id in atom_ids {
        let neighbors = &adj[&c_id];
        let n = neighbors.len();
        if n < 2 {
            continue;
        }

        let cp = pos[&c_id];
        let mut sorted: Vec<(u32, f32)> = neighbors
            .iter()
            .map(|&nid| {
                let np = pos[&nid];
                (nid, (np[1] - cp[1]).atan2(np[0] - cp[0]))
            })
            .collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        for i in 0..n {
            let (id_a, ang_a) = sorted[i];
            let (id_b, ang_b) = sorted[(i + 1) % n];

            let mut gap = ang_b - ang_a;
            if gap <= 0.0 {
                gap += std::f32::consts::TAU;
            }

            let theta0 = if n == 2 {
                if gap < std::f32::consts::PI {
                    std::f32::consts::TAU / 3.0
                } else {
                    std::f32::consts::TAU * 2.0 / 3.0
                }
            } else if n == 3 {
                std::f32::consts::TAU / 3.0
            } else {
                std::f32::consts::TAU / n as f32
            };

            let err = gap - theta0;
            let na = pos[&id_a];
            let nb = pos[&id_b];
            let ra = ((na[0] - cp[0]).powi(2) + (na[1] - cp[1]).powi(2))
                .sqrt()
                .max(0.001);
            let rb = ((nb[0] - cp[0]).powi(2) + (nb[1] - cp[1]).powi(2))
                .sqrt()
                .max(0.001);
            let ta = [-(na[1] - cp[1]) / ra, (na[0] - cp[0]) / ra];
            let tb = [-(nb[1] - cp[1]) / rb, (nb[0] - cp[0]) / rb];

            let f = p.k_angle * err;
            {
                let fa = forces.get_mut(&id_a).unwrap();
                fa[0] += f * ta[0];
                fa[1] += f * ta[1];
            }
            {
                let fb = forces.get_mut(&id_b).unwrap();
                fb[0] -= f * tb[0];
                fb[1] -= f * tb[1];
            }
        }
    }

    // Non-bonded repulsion: soft 1/r² within half a bond length
    for i in 0..atom_ids.len() {
        for j in (i + 1)..atom_ids.len() {
            let id_i = atom_ids[i];
            let id_j = atom_ids[j];
            if bonded.contains(&(id_i, id_j)) {
                continue;
            }
            let pi = pos[&id_i];
            let pj = pos[&id_j];
            let dx = pj[0] - pi[0];
            let dy = pj[1] - pi[1];
            let d2 = (dx * dx + dy * dy).max(0.0001);
            let d = d2.sqrt();
            if d >= L * 0.5 {
                continue;
            }
            let mag = p.k_rep / d2;
            let fx = mag * dx / d;
            let fy = mag * dy / d;
            {
                let f = forces.get_mut(&id_i).unwrap();
                f[0] -= fx;
                f[1] -= fy;
            }
            {
                let f = forces.get_mut(&id_j).unwrap();
                f[0] += fx;
                f[1] += fy;
            }
        }
    }
}

fn recenter(pos: &mut HashMap<u32, [f32; 2]>, ids: &[u32]) {
    if ids.is_empty() {
        return;
    }
    let (sx, sy) = ids.iter().fold((0.0_f32, 0.0_f32), |(ax, ay), id| {
        let p = pos[id];
        (ax + p[0], ay + p[1])
    });
    let n = ids.len() as f32;
    let (cx, cy) = (sx / n, sy / n);
    for id in ids {
        let p = pos.get_mut(id).unwrap();
        p[0] -= cx;
        p[1] -= cy;
    }
}
