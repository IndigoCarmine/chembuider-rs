use super::Molecule;
use std::collections::{HashMap, HashSet};

const L: f32 = 1.5;
const FORCE_THRESHOLD: f32 = 1e-3;

// FIRE (Fast Inertial Relaxation Engine) hyperparameters — Bitzek et al. 2006
// Using Velocity Verlet (VV) integration: energy-conserving, fewer spurious P<0 events.
const N_MIN: usize = 5;      // consecutive positive-power steps before increasing dt
const F_INC: f32 = 1.1;     // dt growth factor
const F_DEC: f32 = 0.5;     // dt shrink factor on genuine overshoot
const ALPHA_START: f32 = 0.1; // initial velocity-mixing weight
const F_ALPHA: f32 = 0.99;    // alpha decay factor when making progress

struct RelaxParams {
    k_bond: f32,
    k_angle: f32,
    k_rep: f32,
    dt_init: f32,
    dt_max: f32,
    bond_cap: f32, // max |deviation| for bond spring (min-potential soft cap)
    rep_cap: f32,  // max repulsion force magnitude
}

// Maximum per-atom speed: prevents velocity runaway when initial forces are large.
// With dt_max=0.1, this limits per-step displacement to V_MAX*dt_max = 0.5 units/step.
const V_MAX: f32 = 5.0;

// Velocity friction per step: v *= FRICTION.  Strong overdamping (β=0.9) keeps v in the
// friction-controlled regime (v_ss = F·dt/(1-β)) rather than V_MAX-clamped, so ring
// oscillations damp rapidly (~5 half-periods → 1/200 amplitude) without butterfly-effect
// divergence from near-zero friction.
const FRICTION: f32 = 0.9;

const PARAMS: RelaxParams = RelaxParams {
    k_bond: 8.0,
    k_angle: 3.0,
    k_rep: 1.5,
    dt_init: 0.01,
    dt_max: 0.1,
    bond_cap: 3.0,
    rep_cap: 50.0,
};

// ─── Ring detection ───────────────────────────────────────────────────────────

/// Find fundamental cycles via iterative DFS.  One cycle per back-edge.
fn find_simple_cycles(adj: &HashMap<u32, Vec<u32>>, atom_ids: &[u32]) -> Vec<Vec<u32>> {
    if atom_ids.is_empty() {
        return vec![];
    }
    let mut cycles = Vec::new();
    // 0 = unvisited, 1 = on stack (gray), 2 = finished (black)
    let mut color: HashMap<u32, u8> = atom_ids.iter().map(|&id| (id, 0u8)).collect();
    let mut parent: HashMap<u32, u32> = HashMap::new();

    for &start in atom_ids {
        if color[&start] != 0 {
            continue;
        }
        // Stack: (node, next-neighbor-index)
        let mut stack: Vec<(u32, usize)> = vec![(start, 0)];
        *color.get_mut(&start).unwrap() = 1;

        loop {
            let (u, idx) = match stack.last() {
                Some(&x) => x,
                None => break,
            };
            let neighbors = &adj[&u];
            if idx >= neighbors.len() {
                *color.get_mut(&u).unwrap() = 2;
                stack.pop();
                continue;
            }
            stack.last_mut().unwrap().1 += 1;
            let v = neighbors[idx];
            let par = parent.get(&u).copied().unwrap_or(u32::MAX);
            if v == par {
                continue;
            }
            match color.get(&v).copied().unwrap_or(0) {
                1 => {
                    // Back edge u→v: trace parent chain to extract cycle
                    let mut cycle = vec![u];
                    let mut cur = u;
                    while cur != v {
                        cur = parent[&cur];
                        cycle.push(cur);
                    }
                    cycle.reverse();
                    cycles.push(cycle);
                }
                0 => {
                    *color.get_mut(&v).unwrap() = 1;
                    parent.insert(v, u);
                    stack.push((v, 0));
                }
                _ => {}
            }
        }
    }
    cycles
}

/// Returns true when the two line segments (p1-p2) and (p3-p4) properly intersect
/// (strictly in their interiors — shared endpoints are not a crossing).
fn segments_cross(p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], p4: [f32; 2]) -> bool {
    let cross2d = |a: [f32; 2], b: [f32; 2]| a[0] * b[1] - a[1] * b[0];
    let sub = |a: [f32; 2], b: [f32; 2]| [a[0] - b[0], a[1] - b[1]];
    let d1 = cross2d(sub(p2, p1), sub(p3, p1));
    let d2 = cross2d(sub(p2, p1), sub(p4, p1));
    let d3 = cross2d(sub(p4, p3), sub(p1, p3));
    let d4 = cross2d(sub(p4, p3), sub(p2, p3));
    // Strictly opposite signs on both tests ⟹ proper intersection.
    (d1 > 0.0 && d2 < 0.0 || d1 < 0.0 && d2 > 0.0)
        && (d3 > 0.0 && d4 < 0.0 || d3 < 0.0 && d4 > 0.0)
}

/// True when at least one pair of non-adjacent ring bonds cross each other.
fn ring_has_crossing(pos: &HashMap<u32, [f32; 2]>, ring: &[u32]) -> bool {
    let n = ring.len();
    for i in 0..n {
        let pa = pos[&ring[i]];
        let pb = pos[&ring[(i + 1) % n]];
        // j starts at i+2 to skip adjacent bond; the last bond (n-1) is adjacent to bond 0.
        let j_end = if i == 0 { n - 1 } else { n };
        for j in (i + 2)..j_end {
            let pc = pos[&ring[j]];
            let pd = pos[&ring[(j + 1) % n]];
            if segments_cross(pa, pb, pc, pd) {
                return true;
            }
        }
    }
    false
}

/// For each ring that has actual bond crossings, apply the minimum-displacement
/// permutation of positions within the ring that restores a valid (non-crossing)
/// angular traversal order.  Rings that are already valid are left untouched.
fn fix_ring_crossings(pos: &mut HashMap<u32, [f32; 2]>, cycles: &[Vec<u32>]) {
    // Process smallest rings first so fused-ring corrections don't propagate upward.
    let mut rings: Vec<&Vec<u32>> = cycles.iter().collect();
    rings.sort_by_key(|r| r.len());

    for ring in rings {
        let n = ring.len();
        if n < 3 {
            continue;
        }

        // Only fix rings that actually have crossing bonds.
        if !ring_has_crossing(pos, ring) {
            continue;
        }

        // Centroid of ring atoms.
        let (cx, cy) = ring.iter().fold((0.0_f32, 0.0_f32), |(ax, ay), &id| {
            let p = pos[&id];
            (ax + p[0], ay + p[1])
        });
        let (cx, cy) = (cx / n as f32, cy / n as f32);

        // Angle of each ring atom (in topological order) from centroid.
        let angles: Vec<f32> = ring.iter().map(|&id| {
            let p = pos[&id];
            (p[1] - cy).atan2(p[0] - cx)
        }).collect();

        // Sort indices by angle → CCW angular order.
        let mut sorted_idx: Vec<usize> = (0..n).collect();
        sorted_idx.sort_by(|&a, &b| angles[a].partial_cmp(&angles[b]).unwrap());

        // The n angular-slot positions (sorted CCW).
        let angular_pos: Vec<[f32; 2]> = sorted_idx.iter().map(|&i| pos[&ring[i]]).collect();
        let current_pos: Vec<[f32; 2]> = ring.iter().map(|&id| pos[&id]).collect();

        // Try all 2·n assignments (n cyclic rotations × 2 directions).
        //   direction=false (CCW): ring[i] ← angular_pos[(i+offset) % n]
        //   direction=true  (CW):  ring[i] ← angular_pos[(n-1-i+offset) % n]
        // Pick the assignment with minimum total squared displacement (closest to original).
        let mut best_cost = f32::INFINITY;
        let mut best_new_pos: Vec<[f32; 2]> = current_pos.clone();

        for direction in [false, true] {
            for offset in 0..n {
                let mut cost = 0.0_f32;
                let mut candidate: Vec<[f32; 2]> = Vec::with_capacity(n);
                for i in 0..n {
                    let slot = if direction {
                        (2 * n - 1 - i + offset) % n
                    } else {
                        (i + offset) % n
                    };
                    let new_p = angular_pos[slot];
                    let old_p = current_pos[i];
                    let dx = new_p[0] - old_p[0];
                    let dy = new_p[1] - old_p[1];
                    cost += dx * dx + dy * dy;
                    candidate.push(new_p);
                }
                if cost < best_cost {
                    best_cost = cost;
                    best_new_pos = candidate;
                }
            }
        }

        for (i, &id) in ring.iter().enumerate() {
            *pos.get_mut(&id).unwrap() = best_new_pos[i];
        }
    }
}

// ─── Incremental cleanup state ────────────────────────────────────────────────

/// Persistent state for continuous cleanup.  Create with `CleanupState::new`,
/// advance with `step`, and write back to the molecule with `apply`.
pub struct CleanupState {
    pos: HashMap<u32, [f32; 2]>,
    vel: HashMap<u32, [f32; 2]>,
    atom_ids: Vec<u32>,
    // forces holds F(x_current) — used for the first half-kick of the next VV step.
    forces: HashMap<u32, [f32; 2]>,
    bonds: Vec<(u32, u32)>,         // cached from mol (immutable during relaxation)
    adj: HashMap<u32, Vec<u32>>,    // cached adjacency list
    bonded: HashSet<(u32, u32)>,    // both orderings, for O(1) repulsion filter
    // Fundamental cycles (for deferred ring-crossing fix in phase 2).
    cycles: Vec<Vec<u32>>,
    // FIRE adaptive state
    dt: f32,
    alpha: f32,
    n_pos: usize,
    // True when the molecular graph contains at least one cycle.
    // Friction is applied only for cyclic molecules (ring oscillations need damping);
    // acyclic spanning trees converge faster without friction.
    has_rings: bool,
}

impl CleanupState {
    pub fn new(mol: &Molecule) -> Self {
        let atom_ids: Vec<u32> = mol.atoms.iter().map(|a| a.id).collect();
        let pos: HashMap<u32, [f32; 2]> = mol.atoms.iter().map(|a| (a.id, a.pos)).collect();
        let vel: HashMap<u32, [f32; 2]> = atom_ids.iter().map(|&id| (id, [0.0_f32; 2])).collect();
        let mut forces: HashMap<u32, [f32; 2]> =
            atom_ids.iter().map(|&id| (id, [0.0_f32; 2])).collect();

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

        let cycles = find_simple_cycles(&adj, &atom_ids);
        let has_rings = !cycles.is_empty();

        // Pre-compute initial forces so the first VV half-kick uses real forces, not zeros.
        compute_forces(&pos, &bonds, &adj, &atom_ids, &bonded, &PARAMS, &mut forces);

        CleanupState {
            pos, vel, atom_ids, forces, bonds, adj, bonded,
            cycles,
            dt: PARAMS.dt_init,
            alpha: ALPHA_START,
            n_pos: 0,
            has_rings,
        }
    }

    /// Apply ring-crossing fix (phase-2 rescue): permute ring-atom positions into the
    /// correct angular order for any ring that has actual bond crossings, then reset
    /// velocities and FIRE state so the next step() starts fresh from the new positions.
    /// Only called when phase 1 (normal FIRE) has failed to converge.
    pub fn apply_ring_fix(&mut self) {
        fix_ring_crossings(&mut self.pos, &self.cycles);
        for v in self.vel.values_mut() {
            *v = [0.0, 0.0];
        }
        self.dt = PARAMS.dt_init;
        self.alpha = ALPHA_START;
        self.n_pos = 0;
        compute_forces(
            &self.pos, &self.bonds, &self.adj, &self.atom_ids,
            &self.bonded, &PARAMS, &mut self.forces,
        );
    }

    pub fn max_force(&self) -> f32 {
        self.forces.values()
            .map(|f| f[0] * f[0] + f[1] * f[1])
            .fold(0.0_f32, f32::max)
            .sqrt()
    }

    /// Run up to `n` FIRE steps using Velocity Verlet integration.
    /// Returns `true` when converged (max force < FORCE_THRESHOLD).
    ///
    /// VV layout per step (one force evaluation):
    ///   1. half-kick:  v += F_stored * dt/2    (F_stored = forces from previous step)
    ///   2. drift:      x += v * dt
    ///   3. new forces: F_new = compute_forces(x)
    ///   4. half-kick:  v += F_new * dt/2       (full velocity at new position)
    ///   5. P = F_new · v  →  FIRE adaptation
    ///   6. FIRE velocity mixing toward F_new
    ///   7. Clamp per-atom |v| ≤ V_MAX
    ///   8. Friction: v *= FRICTION  (damps ring oscillations)
    ///   (F_new stored in self.forces for next iteration)
    pub fn step(&mut self, n: usize) -> bool {
        let p = &PARAMS;
        for _ in 0..n {
            let dt = self.dt;

            // 1. First half-kick with forces stored from the previous step.
            for &id in &self.atom_ids {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] += f[0] * dt * 0.5;
                v[1] += f[1] * dt * 0.5;
            }

            // 2. Drift.
            for &id in &self.atom_ids {
                let v = self.vel[&id];
                let q = self.pos.get_mut(&id).unwrap();
                q[0] += v[0] * dt;
                q[1] += v[1] * dt;
            }
            recenter(&mut self.pos, &self.atom_ids);

            // 3. Compute forces at the new positions (overwrites self.forces).
            compute_forces(
                &self.pos, &self.bonds, &self.adj, &self.atom_ids,
                &self.bonded, p, &mut self.forces,
            );

            // 4. Second half-kick — velocity is now the full VV velocity at the new position.
            for &id in &self.atom_ids {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] += f[0] * dt * 0.5;
                v[1] += f[1] * dt * 0.5;
            }

            // 5. Convergence check (uses new forces).
            let max_f2 = self.forces.values()
                .map(|f| f[0] * f[0] + f[1] * f[1])
                .fold(0.0_f32, f32::max);
            if max_f2 < FORCE_THRESHOLD * FORCE_THRESHOLD {
                return true;
            }

            // 6. Power P = F · v (full velocity, new forces).
            let power: f32 = self.atom_ids.iter()
                .map(|id| {
                    let f = self.forces[id];
                    let v = self.vel[id];
                    f[0] * v[0] + f[1] * v[1]
                })
                .sum();

            // 7. FIRE dt/alpha adaptation.
            if power > 0.0 {
                self.n_pos += 1;
                if self.n_pos >= N_MIN {
                    self.dt = (self.dt * F_INC).min(p.dt_max);
                    self.alpha *= F_ALPHA;
                }
            } else if power < 0.0 {
                // Genuine overshoot: remove kinetic energy, shrink dt to reduce next overshoot.
                // Floor at dt_init (not dt_init*0.1) for fast recovery — VV has few genuine P<0.
                for v in self.vel.values_mut() {
                    *v = [0.0, 0.0];
                }
                self.dt = (self.dt * F_DEC).max(p.dt_init);
                self.alpha = ALPHA_START;
                self.n_pos = 0;
            }
            // power == 0.0: v is already zero, no state change.

            // 8. FIRE: mix velocity toward force direction.
            //    v ← (1-α)v + α·(|v|/|F|)·F
            let v_norm: f32 = self.atom_ids.iter()
                .map(|id| { let v = self.vel[id]; v[0] * v[0] + v[1] * v[1] })
                .sum::<f32>()
                .sqrt();
            let f_norm: f32 = self.atom_ids.iter()
                .map(|id| { let f = self.forces[id]; f[0] * f[0] + f[1] * f[1] })
                .sum::<f32>()
                .sqrt()
                .max(1e-10);
            let ratio = v_norm / f_norm;
            let alpha = self.alpha;
            for &id in &self.atom_ids {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] = (1.0 - alpha) * v[0] + alpha * ratio * f[0];
                v[1] = (1.0 - alpha) * v[1] + alpha * ratio * f[1];
            }

            // 9. Clamp per-atom speed to prevent velocity runaway with large initial forces.
            for &id in &self.atom_ids {
                let v = self.vel.get_mut(&id).unwrap();
                let spd2 = v[0] * v[0] + v[1] * v[1];
                if spd2 > V_MAX * V_MAX {
                    let scale = V_MAX / spd2.sqrt();
                    v[0] *= scale;
                    v[1] *= scale;
                }
            }

            // 10. Friction: damp ring oscillations (ring molecules only).
            if self.has_rings {
                for &id in &self.atom_ids {
                    let v = self.vel.get_mut(&id).unwrap();
                    v[0] *= FRICTION;
                    v[1] *= FRICTION;
                }
            }
            // self.forces already holds F(x_new), ready for the next step's first half-kick.
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
    // Phase 1: normal FIRE.  Converges most molecules without any position permutation.
    if state.step(100_000) {
        state.apply(mol);
        return;
    }
    // Phase 2: if significantly stuck and the molecule has rings, apply the ring-crossing
    // fix (permute ring-atom positions to correct angular order) and continue FIRE.
    // Only triggered when phase 1 fails; leaves trial=00-style molecules untouched.
    if state.has_rings && state.max_force() > 0.1 {
        state.apply_ring_fix();
    }
    state.step(100_000);
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

    // Bond stretching: soft potential — harmonic near L, constant-force cap beyond bond_cap
    for &(begin, end) in bonds {
        let pi = pos[&begin];
        let pj = pos[&end];
        let dx = pj[0] - pi[0];
        let dy = pj[1] - pi[1];
        let d = (dx * dx + dy * dy).sqrt().max(0.001);
        let mag = p.k_bond * (d - L).clamp(-p.bond_cap, p.bond_cap);
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
            // Reaction on center atom: Newton's 3rd law — ensures zero net force
            {
                let fc = forces.get_mut(&c_id).unwrap();
                fc[0] += f * (tb[0] - ta[0]);
                fc[1] += f * (tb[1] - ta[1]);
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
            let mag = (p.k_rep / d2).min(p.rep_cap);
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

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::molecule::{BondOrder, Molecule};

    /// Minimal LCG — no external deps, reproducible seed.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self.0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            self.0
        }
        fn f32_range(&mut self, lo: f32, hi: f32) -> f32 {
            let t = ((self.next() >> 33) as f32) / (u32::MAX as f32);
            lo + t * (hi - lo)
        }
        fn usize_range(&mut self, lo: usize, hi: usize) -> usize {
            lo + (self.next() as usize % (hi - lo))
        }
    }

    /// Random connected molecule: random spanning tree + `extra_bonds` ring-closures.
    /// Atom positions are fully random within [-range, range].
    fn random_molecule(rng: &mut Lcg, n_atoms: usize, extra_bonds: usize, range: f32) -> Molecule {
        assert!(n_atoms >= 2);
        let mut mol = Molecule::default();
        for _ in 0..n_atoms {
            let x = rng.f32_range(-range, range);
            let y = rng.f32_range(-range, range);
            mol.add_atom("C".to_string(), [x, y], 0);
        }
        let ids: Vec<u32> = mol.atoms.iter().map(|a| a.id).collect();
        // Random spanning tree — guarantees connectivity
        for i in 1..ids.len() {
            let j = rng.usize_range(0, i);
            mol.add_bond(ids[j], ids[i], BondOrder::Single);
        }
        // Extra bonds (may form rings)
        for _ in 0..extra_bonds {
            let a = rng.usize_range(0, ids.len());
            let b = rng.usize_range(0, ids.len());
            if a != b {
                mol.add_bond(ids[a], ids[b], BondOrder::Single);
            }
        }
        mol
    }

    /// Max per-atom displacement between two molecule snapshots (same atom order assumed).
    fn max_displacement(before: &Molecule, after: &Molecule) -> f32 {
        before.atoms.iter().zip(after.atoms.iter())
            .map(|(a, b)| {
                let dx = b.pos[0] - a.pos[0];
                let dy = b.pos[1] - a.pos[1];
                (dx * dx + dy * dy).sqrt()
            })
            .fold(0.0_f32, f32::max)
    }

    /// Mirror the two-phase logic of cleanup_2d; return (converged, steps, final_max_force).
    fn run_steps_capped(mol: &Molecule, max_steps: usize) -> (bool, usize, f32) {
        let phase1 = max_steps / 2;
        let phase2 = max_steps - phase1;
        let mut state = CleanupState::new(mol);
        for step in 0..phase1 {
            if state.step(1) {
                return (true, step + 1, state.max_force());
            }
        }
        // Phase 2: apply ring fix if stuck, then continue.
        if state.has_rings && state.max_force() > 0.1 {
            state.apply_ring_fix();
        }
        for step in 0..phase2 {
            if state.step(1) {
                return (true, phase1 + step + 1, state.max_force());
            }
        }
        (false, max_steps, state.max_force())
    }

    #[test]
    fn diagnose_convergence_50_random() {
        let mut rng = Lcg(0xDEAD_BEEF_CAFE_1234);
        let cap = 200_000_usize;
        let mut n_failed = 0usize;

        for trial in 0..50_usize {
            let n_atoms     = rng.usize_range(2, 21);
            let extra_bonds = rng.usize_range(0, n_atoms / 2 + 1);
            let range       = rng.f32_range(5.0, 15.0);

            let mol = random_molecule(&mut rng, n_atoms, extra_bonds, range);

            let (converged, steps, final_force) = run_steps_capped(&mol, cap);

            println!(
                "trial={trial:02}  atoms={n_atoms:2}  extra={extra_bonds}  range={range:.1}  \
                 converged={converged}  steps={steps:>7}  final_force={final_force:.4e}",
            );

            if !converged {
                n_failed += 1;
            }
        }

        println!("\n{n_failed}/50 trials did NOT converge within {cap} steps");
        // Not asserting — diagnostic only
    }

    #[test]
    fn convergence_50_random() {
        let mut rng = Lcg(0xDEAD_BEEF_CAFE_1234);
        let mut failures: Vec<String> = Vec::new();
        let threshold = 0.05_f32;

        for trial in 0..50_usize {
            let n_atoms     = rng.usize_range(2, 21);
            let extra_bonds = rng.usize_range(0, n_atoms / 2 + 1);
            let range       = rng.f32_range(5.0, 15.0);

            let mut mol = random_molecule(&mut rng, n_atoms, extra_bonds, range);

            cleanup_2d(&mut mol);
            let after_first = mol.clone();
            cleanup_2d(&mut mol);

            let disp = max_displacement(&after_first, &mol);

            println!(
                "trial={trial:02}  atoms={n_atoms:2}  extra={extra_bonds}  range={range:.1}  \
                 max_disp_2nd={disp:.6}  {}",
                if disp > threshold { "FAIL" } else { "ok" }
            );

            if disp > threshold {
                failures.push(format!(
                    "trial={trial:02} atoms={n_atoms} extra={extra_bonds} range={range:.1} disp={disp:.4}"
                ));
            }
        }

        if !failures.is_empty() {
            panic!(
                "\n{} / 50 structures did not converge:\n{}",
                failures.len(),
                failures.join("\n")
            );
        }
    }
}
