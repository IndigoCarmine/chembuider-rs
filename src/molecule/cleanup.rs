use super::Molecule;
use std::collections::{HashMap, HashSet};

const L: f32 = 1.5;

// Convergence is based on motion, not force: stop once atoms have effectively stopped
// moving.  Every MSD_WINDOW steps the mean squared displacement (MSD) of all atoms relative
// to a reference snapshot taken MSD_WINDOW steps earlier is measured; when it drops below
// MSD_THRESHOLD the structure is considered stationary.  Using a window rather than a single
// step avoids false positives from momentary stalls (e.g. just after a FIRE velocity reset,
// when dt is small and one step's drift is tiny regardless of the remaining force).
// MSD_THRESHOLD = (1e-3)^2 ⇒ atoms move < ~1e-3 length-units over the whole window.
const MSD_WINDOW: usize = 50;
const MSD_THRESHOLD: f32 = 1e-6;

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
    bond_cap: f32,  // max |deviation| for bond spring (min-potential soft cap)
    rep_cap: f32,   // max repulsion force magnitude
    // Upper bound on the bend term's L/r moment-arm factor, so a neighbour collapsing onto
    // its centre cannot make the tangential force diverge.
    angle_amp_max: f32,
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
    // The L/r arm factor is exact physics, but only meaningful while the neighbour sits at
    // a chemically sensible distance.  Below r = L/2 — the repulsion cutoff, i.e. the atoms
    // are already on top of each other — the bond spring is the term that has to win, so
    // the arm factor is frozen at its value there: L/(L/2) = 2.
    angle_amp_max: 2.0,
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
    // sp-hybridized centers that want linear (180°) geometry: a degree-2 atom with a
    // triple bond, or the central atom of two cumulated double bonds (allene).
    linear_atoms: HashSet<u32>,
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
    // Motion-based convergence: reference snapshot (in atom_ids order) and steps elapsed
    // since it was taken.  See MSD_WINDOW / MSD_THRESHOLD.
    ref_pos: Vec<[f32; 2]>,
    steps_since_ref: usize,
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

        let linear_atoms = linear_centers(mol);

        let cycles = find_simple_cycles(&adj, &atom_ids);
        let has_rings = !cycles.is_empty();

        // Pre-compute initial forces so the first VV half-kick uses real forces, not zeros.
        compute_forces(&pos, &bonds, &adj, &atom_ids, &bonded, &linear_atoms, &PARAMS, &mut forces);

        let ref_pos: Vec<[f32; 2]> = atom_ids.iter().map(|id| pos[id]).collect();

        CleanupState {
            pos, vel, atom_ids, forces, bonds, adj, bonded,
            linear_atoms,
            cycles,
            dt: PARAMS.dt_init,
            alpha: ALPHA_START,
            n_pos: 0,
            has_rings,
            ref_pos,
            steps_since_ref: 0,
        }
    }

    /// Apply ring-crossing fix (phase-2 rescue): permute ring-atom positions into the
    /// correct angular order for any ring that has actual bond crossings, then reset
    /// velocities and FIRE state so the next step() starts fresh from the new positions.
    pub fn apply_ring_fix(&mut self) {
        fix_ring_crossings(&mut self.pos, &self.cycles);
        self.restart_from_current_positions();
    }

    /// Put the atoms back at a snapshot taken with `positions()` and restart from rest.
    pub fn restore(&mut self, snapshot: &[(u32, [f32; 2])]) {
        for &(id, p) in snapshot {
            if let Some(q) = self.pos.get_mut(&id) {
                *q = p;
            }
        }
        self.restart_from_current_positions();
    }

    /// Positions jumped discontinuously: drop the accumulated velocity, put FIRE back to its
    /// initial timestep/mixing, re-baseline the motion-convergence window, and recompute the
    /// forces so the next `step` starts from a consistent state.
    fn restart_from_current_positions(&mut self) {
        for v in self.vel.values_mut() {
            *v = [0.0, 0.0];
        }
        self.dt = PARAMS.dt_init;
        self.alpha = ALPHA_START;
        self.n_pos = 0;
        for (k, &id) in self.atom_ids.iter().enumerate() {
            self.ref_pos[k] = self.pos[&id];
        }
        self.steps_since_ref = 0;
        compute_forces(
            &self.pos, &self.bonds, &self.adj, &self.atom_ids,
            &self.bonded, &self.linear_atoms, &PARAMS, &mut self.forces,
        );
    }

    /// How many rings are currently drawn with their own bonds crossing each other — the one
    /// defect `apply_ring_fix` repairs, and the score the rescue is accepted or rejected on.
    ///
    /// Whether to attempt the rescue used to be approximated by `has_rings && max_force() > 0.1`,
    /// which only worked by accident: the bend term was not torque-free, so a relaxed cyclic
    /// molecule always retained a residual force and the test was effectively always true.
    /// Once the force field became conservative that proxy started reading "nothing wrong" for
    /// tangled rings and the rescue stopped running.  Asking about the actual defect is both
    /// correct and cheaper.
    pub fn ring_crossing_count(&self) -> usize {
        self.cycles
            .iter()
            .filter(|ring| ring.len() >= 3 && ring_has_crossing(&self.pos, ring))
            .count()
    }

    pub fn max_force(&self) -> f32 {
        self.forces.values()
            .map(|f| f[0] * f[0] + f[1] * f[1])
            .fold(0.0_f32, f32::max)
            .sqrt()
    }

    /// Run up to `n` FIRE steps using Velocity Verlet integration.
    /// Returns `true` when converged — i.e. atoms have stopped moving (windowed MSD < MSD_THRESHOLD).
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
                &self.bonded, &self.linear_atoms, p, &mut self.forces,
            );

            // 4. Second half-kick — velocity is now the full VV velocity at the new position.
            for &id in &self.atom_ids {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] += f[0] * dt * 0.5;
                v[1] += f[1] * dt * 0.5;
            }

            // 5. Convergence check (motion-based): every MSD_WINDOW steps, measure how far
            //    atoms have moved from the reference snapshot.  Positions are already final
            //    for this step (the second half-kick only updates velocity).
            self.steps_since_ref += 1;
            if self.steps_since_ref >= MSD_WINDOW {
                let n = self.atom_ids.len().max(1) as f32;
                let msd: f32 = self.atom_ids.iter().enumerate()
                    .map(|(k, id)| {
                        let np = self.pos[id];
                        let dx = np[0] - self.ref_pos[k][0];
                        let dy = np[1] - self.ref_pos[k][1];
                        dx * dx + dy * dy
                    })
                    .sum::<f32>() / n;
                if msd < MSD_THRESHOLD {
                    return true;
                }
                // Not stationary yet: advance the reference window to the current positions.
                for (k, &id) in self.atom_ids.iter().enumerate() {
                    self.ref_pos[k] = self.pos[&id];
                }
                self.steps_since_ref = 0;
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

            // 11. Drain whole-structure translation/rotation so the next drift only relaxes
            //     internal geometry (otherwise residual spin never lets positions converge).
            remove_rigid_motion(&self.pos, &mut self.vel, &self.atom_ids);

            // self.forces already holds F(x_new), ready for the next step's first half-kick.
        }
        false
    }

    /// Write the current relaxed positions back to the molecule.
    /// Used by the synchronous `cleanup_2d` (tests / fragment-cleanup script); the interactive
    /// path streams positions through `CleanupJob` instead.
    #[allow(dead_code)]
    pub fn apply(&self, mol: &mut Molecule) {
        for (&id, &p) in &self.pos {
            if let Some(atom) = mol.atom_by_id_mut(id) {
                atom.pos = p;
            }
        }
    }

    /// Snapshot of the current positions (atom_ids order) for live preview / publishing.
    pub fn positions(&self) -> Vec<(u32, [f32; 2])> {
        self.atom_ids.iter().map(|&id| (id, self.pos[&id])).collect()
    }
}

// ─── One-shot synchronous entry point ────────────────────────────────────────
// Blocking version, retained for tests and the fragment-cleanup script. The interactive
// UI uses `CleanupJob` (background thread + cancel) instead so it never freezes.

#[allow(dead_code)]
pub fn cleanup_2d(mol: &mut Molecule) {
    if mol.atoms.is_empty() {
        return;
    }
    let mut state = CleanupState::new(mol);
    // Phase 1: normal FIRE.  Converges most molecules without any position permutation.
    state.step(JOB_PHASE_BUDGET);
    // Phase 2: the ring-crossing rescue, if phase 1 left a ring drawn inside-out.
    ring_rescue(&mut state, |s| {
        s.step(JOB_PHASE_BUDGET);
        true
    });
    state.apply(mol);
}

/// Phase 2, shared by every entry point.
///
/// Relaxation is local, so it happily settles into a minimum where a ring is drawn inside-out
/// with its own bonds crossing.  `apply_ring_fix` permutes those ring atoms back into a valid
/// angular order; `relax` then re-relaxes from there and returns false if it was cancelled.
///
/// A pass is kept only when it strictly reduced the number of crossed rings, and passes repeat
/// while they keep helping.  Both halves of that rule are needed to leave `cleanup_2d`
/// idempotent — the property `convergence_50_random` checks by cleaning twice:
///
///   * Without the accept/reject, a molecule that simply cannot be drawn crossing-free (a
///     random 19-atom graph with 9 extra bonds is essentially never planar) would re-trigger
///     the permutation on every cleanup and shuffle its ring atoms somewhere new each time.
///   * Without the repeat, a molecule the rescue can still improve would be improved a bit
///     more on each successive cleanup, which moves atoms just the same.
///
/// Repeating terminates quickly: an accepted pass strictly lowers the crossing count, so there
/// can be at most one accepted pass per ring, and the run ends on the first rejected pass.
fn ring_rescue(state: &mut CleanupState, mut relax: impl FnMut(&mut CleanupState) -> bool) {
    for _ in 0..state.cycles.len().max(1) {
        let before = state.ring_crossing_count();
        if before == 0 {
            return;
        }
        let snapshot = state.positions();
        state.apply_ring_fix();
        let completed = relax(state);
        if !completed || state.ring_crossing_count() >= before {
            state.restore(&snapshot);
            return;
        }
    }
}

// ─── Background, cancellable cleanup job ──────────────────────────────────────

/// Steps run per chunk between cancel checks and position publishes.
const JOB_CHUNK: usize = 500;
const JOB_PHASE_BUDGET: usize = 100_000;

#[cfg(not(target_arch = "wasm32"))]
mod job_native {
    use super::{run_cleanup_job, Molecule};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// A cleanup computation running on a background thread. The UI thread polls
    /// `latest()` for a live preview and `is_done()` for completion, and may `cancel()`
    /// at any time. Created with `CleanupJob::start`.
    pub struct CleanupJob {
        cancel: Arc<AtomicBool>,
        done: Arc<AtomicBool>,
        shared: Arc<Mutex<Vec<(u32, [f32; 2])>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl CleanupJob {
        /// Spawn a worker that relaxes a snapshot of `mol` off the UI thread.
        pub fn start(mol: &Molecule) -> Self {
            let cancel = Arc::new(AtomicBool::new(false));
            let done = Arc::new(AtomicBool::new(false));
            let shared = Arc::new(Mutex::new(
                mol.atoms.iter().map(|a| (a.id, a.pos)).collect::<Vec<_>>(),
            ));

            let snapshot = mol.clone();
            let (c, d, s) = (cancel.clone(), done.clone(), shared.clone());
            let handle = std::thread::spawn(move || {
                run_cleanup_job(&snapshot, &c, &d, &s);
            });

            CleanupJob { cancel, done, shared, handle: Some(handle) }
        }

        /// Request the worker to stop at the next chunk boundary.
        pub fn cancel(&self) {
            self.cancel.store(true, Ordering::Relaxed);
        }

        pub fn is_done(&self) -> bool {
            self.done.load(Ordering::Relaxed)
        }

        /// Latest relaxed positions (for live preview each frame).
        pub fn latest(&self) -> Vec<(u32, [f32; 2])> {
            self.shared.lock().unwrap().clone()
        }

        /// Join the worker thread (call once `is_done`).
        pub fn join(mut self) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    impl Drop for CleanupJob {
        fn drop(&mut self) {
            // Make sure a still-running worker stops if the job is dropped without joining.
            self.cancel.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use job_native::CleanupJob;

// wasm32-unknown-unknown has no `std::thread::spawn` (it always returns an "unsupported"
// error, and `thread::spawn` panics on that) — there's no Web Worker / SharedArrayBuffer
// setup here for real background threads. Instead, `latest()` — already polled once per UI
// frame by `ChemStructEditor::poll_cleanup` — advances the same chunked relaxation by one
// `JOB_CHUNK` on each call, so the tab still repaints between chunks instead of blocking on
// one huge synchronous relaxation. `Cell`/`RefCell` stand in for the native version's atomics
// and mutex since wasm32-unknown-unknown is single-threaded.
#[cfg(target_arch = "wasm32")]
mod job_wasm {
    use super::{CleanupState, Molecule, JOB_CHUNK, JOB_PHASE_BUDGET};
    use std::cell::{Cell, RefCell};

    pub struct CleanupJob {
        state: RefCell<Option<CleanupState>>,
        phase: Cell<u8>,
        steps_done_in_phase: Cell<usize>,
        cancelled: Cell<bool>,
        done: Cell<bool>,
        positions: RefCell<Vec<(u32, [f32; 2])>>,
        // Phase-2 (ring rescue) bookkeeping: the phase-1 result to fall back to, and the
        // crossing count it has to beat.  See `ring_rescue` for why the retry is scored.
        rescue_snapshot: RefCell<Vec<(u32, [f32; 2])>>,
        crossings_before: Cell<usize>,
    }

    impl CleanupJob {
        pub fn start(mol: &Molecule) -> Self {
            if mol.atoms.is_empty() {
                return Self {
                    state: RefCell::new(None),
                    phase: Cell::new(1),
                    steps_done_in_phase: Cell::new(0),
                    cancelled: Cell::new(false),
                    done: Cell::new(true),
                    positions: RefCell::new(Vec::new()),
                    rescue_snapshot: RefCell::new(Vec::new()),
                    crossings_before: Cell::new(0),
                };
            }
            let state = CleanupState::new(mol);
            let positions = state.positions();
            Self {
                state: RefCell::new(Some(state)),
                phase: Cell::new(1),
                steps_done_in_phase: Cell::new(0),
                cancelled: Cell::new(false),
                done: Cell::new(false),
                positions: RefCell::new(positions),
                rescue_snapshot: RefCell::new(Vec::new()),
                crossings_before: Cell::new(0),
            }
        }

        pub fn cancel(&self) {
            self.cancelled.set(true);
        }

        pub fn is_done(&self) -> bool {
            self.done.get()
        }

        /// Advance one chunk of the relaxation (mirrors `run_phase`'s two-phase logic, just
        /// spread across polls instead of a background-thread loop), then return positions.
        pub fn latest(&self) -> Vec<(u32, [f32; 2])> {
            if !self.done.get() {
                if self.cancelled.get() {
                    self.done.set(true);
                } else if let Some(state) = self.state.borrow_mut().as_mut() {
                    let budget_left = JOB_PHASE_BUDGET - self.steps_done_in_phase.get();
                    let chunk = JOB_CHUNK.min(budget_left);
                    let converged = state.step(chunk);
                    self.steps_done_in_phase.set(self.steps_done_in_phase.get() + chunk);
                    *self.positions.borrow_mut() = state.positions();

                    let phase_exhausted = self.steps_done_in_phase.get() >= JOB_PHASE_BUDGET;
                    if converged || phase_exhausted {
                        // Same schedule as `cleanup_2d` / `ring_rescue`, unrolled across polls:
                        // retry the ring-crossing rescue while each pass keeps removing
                        // crossings, and roll back the first pass that does not.
                        let rejected = self.phase.get() == 2
                            && state.ring_crossing_count() >= self.crossings_before.get();
                        if rejected {
                            state.restore(&self.rescue_snapshot.borrow());
                            *self.positions.borrow_mut() = state.positions();
                            self.done.set(true);
                        } else {
                            let crossings = state.ring_crossing_count();
                            if crossings > 0 {
                                self.crossings_before.set(crossings);
                                *self.rescue_snapshot.borrow_mut() = state.positions();
                                state.apply_ring_fix();
                                self.phase.set(2);
                                self.steps_done_in_phase.set(0);
                            } else {
                                self.done.set(true);
                            }
                        }
                    }
                }
            }
            self.positions.borrow().clone()
        }

        /// Nothing to join — there is no worker thread on wasm32.
        pub fn join(self) {}
    }
}

#[cfg(target_arch = "wasm32")]
pub use job_wasm::CleanupJob;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

/// Worker body: mirrors `cleanup_2d`'s two-phase logic, but chunked so it can check the
/// cancel flag and publish intermediate positions for a live preview.
#[cfg(not(target_arch = "wasm32"))]
fn run_cleanup_job(
    mol: &Molecule,
    cancel: &AtomicBool,
    done: &AtomicBool,
    shared: &Mutex<Vec<(u32, [f32; 2])>>,
) {
    if mol.atoms.is_empty() {
        done.store(true, Ordering::Relaxed);
        return;
    }
    // Same two phases as `cleanup_2d`, just chunked so the worker can be cancelled and can
    // publish intermediate positions for the live preview.
    let mut state = CleanupState::new(mol);
    run_phase(&mut state, cancel, shared);
    if !cancel.load(Ordering::Relaxed) {
        ring_rescue(&mut state, |s| {
            run_phase(s, cancel, shared);
            !cancel.load(Ordering::Relaxed)
        });
    }

    *shared.lock().unwrap() = state.positions();
    done.store(true, Ordering::Relaxed);
}

/// Run one JOB_PHASE_BUDGET phase in chunks; returns true if it converged.
#[cfg(not(target_arch = "wasm32"))]
fn run_phase(
    state: &mut CleanupState,
    cancel: &AtomicBool,
    shared: &Mutex<Vec<(u32, [f32; 2])>>,
) -> bool {
    let mut done_steps = 0;
    while done_steps < JOB_PHASE_BUDGET {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let chunk = JOB_CHUNK.min(JOB_PHASE_BUDGET - done_steps);
        let converged = state.step(chunk);
        done_steps += chunk;
        *shared.lock().unwrap() = state.positions();
        if converged {
            return true;
        }
    }
    false
}

// ─── Force field ──────────────────────────────────────────────────────────────

/// Atoms whose 2D geometry should be linear (180°): sp-hybridized centers. A degree-2
/// atom is linear when it carries a triple bond (alkyne / nitrile carbon) or two
/// cumulated double bonds (allene central carbon). Higher-degree atoms and terminal
/// atoms are excluded — the bend term only acts on degree ≥ 2 anyway.
fn linear_centers(mol: &Molecule) -> HashSet<u32> {
    use crate::molecule::BondOrder;
    let mut triples: HashMap<u32, u32> = HashMap::new();
    let mut doubles: HashMap<u32, u32> = HashMap::new();
    let mut degree: HashMap<u32, u32> = HashMap::new();
    for b in &mol.bonds {
        for id in [b.begin, b.end] {
            *degree.entry(id).or_insert(0) += 1;
            match b.order {
                BondOrder::Triple => *triples.entry(id).or_insert(0) += 1,
                BondOrder::Double => *doubles.entry(id).or_insert(0) += 1,
                BondOrder::Single => {}
            }
        }
    }
    mol.atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| {
            degree.get(id).copied().unwrap_or(0) == 2
                && (triples.get(id).copied().unwrap_or(0) >= 1
                    || doubles.get(id).copied().unwrap_or(0) >= 2)
        })
        .collect()
}

fn compute_forces(
    pos: &HashMap<u32, [f32; 2]>,
    bonds: &[(u32, u32)],
    adj: &HashMap<u32, Vec<u32>>,
    atom_ids: &[u32],
    bonded: &HashSet<(u32, u32)>,
    linear_atoms: &HashSet<u32>,
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

            let theta0 = if n == 2 && linear_atoms.contains(&c_id) {
                // sp center (triple bond / allene): both gaps want 180°.
                std::f32::consts::PI
            } else if n == 2 {
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

            // The bend acts as a torque τ = -∂E/∂gap = -k·err on the a–c–b wedge.  A torque
            // is converted to a tangential force by dividing by the moment arm: |f| = τ/r.
            // That 1/r is what makes the pair's torques about the centre cancel exactly
            // (r_a·τ/r_a - r_b·τ/r_b = 0).  Applying the same |f| to both neighbours instead
            // (the previous code) leaves a residual torque ∝ τ·(r_a - r_b), i.e. a
            // non-conservative field that pumps angular momentum in forever: the molecule
            // reaches its relaxed shape and then spins, so a motion-based convergence test
            // never fires.  With the 1/r factor the angle term is exactly -∇E for
            // E = Σ ½·(k_angle·L)·(gap-θ0)², hence torque-free by rotational invariance.
            //
            // The constant L only sets the scale (it cancels in the torque balance): it keeps
            // the force magnitude at the equilibrium bond length identical to before, so the
            // tuned k_angle/k_bond balance is unchanged for normal geometry.
            //
            // angle_amp_max bounds the 1/r blow-up for a neighbour collapsing onto the centre.
            // The bound must rescale *both* magnitudes by the same factor: the torques cancel
            // only while r_a·|f_a| = r_b·|f_b|, so flooring the two arms individually (or
            // clipping just the larger force) would reintroduce exactly the spurious torque
            // that the 1/r exists to remove.  A common positive rescale keeps the force
            // parallel to the true gradient, so it still descends the energy — the same
            // trust-region trick bond_cap / rep_cap already use.
            let f = p.k_angle * err * L;
            let mut fa_mag = f / ra;
            let mut fb_mag = f / rb;
            let amp = L / ra.min(rb);
            if amp > p.angle_amp_max {
                let s = p.angle_amp_max / amp;
                fa_mag *= s;
                fb_mag *= s;
            }
            {
                let fa = forces.get_mut(&id_a).unwrap();
                fa[0] += fa_mag * ta[0];
                fa[1] += fa_mag * ta[1];
            }
            {
                let fb = forces.get_mut(&id_b).unwrap();
                fb[0] -= fb_mag * tb[0];
                fb[1] -= fb_mag * tb[1];
            }
            // Reaction on center atom: Newton's 3rd law — ensures zero net force
            {
                let fc = forces.get_mut(&c_id).unwrap();
                fc[0] += fb_mag * tb[0] - fa_mag * ta[0];
                fc[1] += fb_mag * tb[1] - fa_mag * ta[1];
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

/// Project out rigid-body motion (net translation + net rotation) from the velocities.
/// Whole-structure translation and rotation are zero-energy modes with no restoring force;
/// the angle forces are not perfectly torque-free, so they keep pumping angular momentum into
/// the rotation mode and the structure spins forever — which, with motion-based convergence,
/// never settles.  Draining that net motion each step lets only the internal geometry relax,
/// and keeps the result at the same orientation the user drew.  Assumes `pos` is centered.
fn remove_rigid_motion(pos: &HashMap<u32, [f32; 2]>, vel: &mut HashMap<u32, [f32; 2]>, ids: &[u32]) {
    if ids.len() < 2 {
        return;
    }
    let n = ids.len() as f32;

    // Net (mean) linear velocity.
    let (mut vx, mut vy) = (0.0_f32, 0.0_f32);
    for id in ids {
        let v = vel[id];
        vx += v[0];
        vy += v[1];
    }
    vx /= n;
    vy /= n;

    // Angular velocity ω = L / I about the centroid (origin), using the de-translated velocity.
    let (mut l, mut inertia) = (0.0_f32, 0.0_f32);
    for id in ids {
        let r = pos[id];
        let v = vel[id];
        l += r[0] * (v[1] - vy) - r[1] * (v[0] - vx);
        inertia += r[0] * r[0] + r[1] * r[1];
    }
    let omega = if inertia > 1e-9 { l / inertia } else { 0.0 };

    // Subtract both the linear and rotational (v_rot = (-ω·y, ω·x)) components.
    for id in ids {
        let r = pos[id];
        let v = vel.get_mut(id).unwrap();
        v[0] -= vx - omega * r[1];
        v[1] -= vy + omega * r[0];
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
        // Phase 2: untangle any inside-out ring, then continue.
        if state.ring_crossing_count() > 0 {
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

    /// Interior angle at atom `b` (in degrees) for the chain a–b–c.
    fn angle_deg(mol: &Molecule, a: u32, b: u32, c: u32) -> f32 {
        let pa = mol.atom_by_id(a).unwrap().pos;
        let pb = mol.atom_by_id(b).unwrap().pos;
        let pc = mol.atom_by_id(c).unwrap().pos;
        let v1 = [pa[0] - pb[0], pa[1] - pb[1]];
        let v2 = [pc[0] - pb[0], pc[1] - pb[1]];
        let dot = v1[0] * v2[0] + v1[1] * v2[1];
        let n1 = (v1[0] * v1[0] + v1[1] * v1[1]).sqrt();
        let n2 = (v2[0] * v2[0] + v2[1] * v2[1]).sqrt();
        (dot / (n1 * n2)).clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// `linear_centers` flags exactly the sp atoms: alkyne/nitrile carbons and allene
    /// centers, but not sp²/sp³ atoms or triple-bond terminals.
    #[test]
    fn linear_centers_flags_sp_atoms() {
        // Propyne  CH3–C≡CH : the two internal alkyne carbons are sp (degree 2 here).
        let mut m = Molecule::default();
        let c0 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c1 = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let c2 = m.add_atom("C".to_string(), [2.0, 0.0], 0);
        m.add_bond(c0, c1, BondOrder::Single);
        m.add_bond(c1, c2, BondOrder::Triple);
        let lin = linear_centers(&m);
        assert!(lin.contains(&c1), "alkyne carbon with the triple bond is linear");
        assert!(!lin.contains(&c0), "sp3 methyl carbon is not linear");
        assert!(!lin.contains(&c2), "triple-bond terminal (degree 1) is not linear");

        // Allene  C=C=C : the central carbon has two double bonds → linear.
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("C".to_string(), [2.0, 0.0], 0);
        m.add_bond(a, b, BondOrder::Double);
        m.add_bond(b, c, BondOrder::Double);
        let lin = linear_centers(&m);
        assert!(lin.contains(&b), "allene central carbon is linear");
        assert!(!lin.contains(&a) && !lin.contains(&c));

        // A single =C< with one double + one single bond (degree 2, sp²) is NOT linear.
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        let c = m.add_atom("O".to_string(), [2.0, 0.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        m.add_bond(b, c, BondOrder::Double);
        assert!(!linear_centers(&m).contains(&b), "vinyl/sp2 carbon is not linear");
    }

    /// Cleanup relaxes a triple-bonded chain toward a 180° angle at the sp carbon, even
    /// when it starts badly bent.
    #[test]
    fn triple_bond_relaxes_to_180() {
        // C0–C1≡C2 with C1 kinked to ~90°.
        let mut m = Molecule::default();
        let c0 = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c1 = m.add_atom("C".to_string(), [1.5, 0.0], 0);
        let c2 = m.add_atom("C".to_string(), [1.5, 1.5], 0); // bent up ~90°
        m.add_bond(c0, c1, BondOrder::Single);
        m.add_bond(c1, c2, BondOrder::Triple);

        let before = angle_deg(&m, c0, c1, c2);
        assert!(before < 120.0, "start should be bent, got {before:.1}");
        cleanup_2d(&mut m);
        let after = angle_deg(&m, c0, c1, c2);
        assert!(after > 172.0, "sp carbon should straighten to ~180°, got {after:.1}");
    }

    /// Net force and net torque (about the origin) of the whole force field, plus the size
    /// of the individual contributions so the check can be a relative one.
    fn net_force_and_torque(st: &CleanupState) -> ([f32; 2], f32, f32) {
        let (mut net, mut torque, mut scale) = ([0.0_f32; 2], 0.0_f32, 0.0_f32);
        for &id in &st.atom_ids {
            let f = st.forces[&id];
            let r = st.pos[&id];
            net[0] += f[0];
            net[1] += f[1];
            torque += r[0] * f[1] - r[1] * f[0];
            scale += (f[0] * f[0] + f[1] * f[1]).sqrt() * (r[0] * r[0] + r[1] * r[1]).sqrt();
        }
        (net, torque, scale)
    }

    /// The relaxation potential depends only on the *shape* of the molecule — bond lengths and
    /// the angles between a centre's neighbours — so it is invariant under rigid motion, and
    /// its gradient must therefore have both zero net force and zero net torque.
    ///
    /// Regression test for a bend term that applied the same tangential force to both
    /// neighbours regardless of how far each sat from the centre.  A torque becomes a force
    /// only after dividing by the moment arm, so dropping that 1/r left a residual torque
    /// ∝ k·err·(r_a − r_b): a non-conservative field that kept pumping angular momentum in.
    /// Molecules reached their relaxed shape and then rotated forever, the motion-based
    /// convergence test never fired, and two consecutive `cleanup_2d` calls returned the same
    /// structure at different angles (`convergence_50_random` trials 22 / 27 / 39, which came
    /// out as pure rotations of 45.9° / 134.2° with zero residual after de-rotation).
    #[test]
    fn force_field_has_no_net_force_or_torque() {
        let mut rng = Lcg(0x0BAD_F00D_1234_5678);
        let mut checked = 0usize;

        for _ in 0..40 {
            let n_atoms = rng.usize_range(3, 21);
            let extra = rng.usize_range(0, n_atoms / 2 + 1);
            let mut mol = random_molecule(&mut rng, n_atoms, extra, 6.0);

            // Relax, then jiggle: a generic geometry with sane bond lengths.  Testing the
            // relaxed structure alone would be vacuous — every force there is already ~0.
            cleanup_2d(&mut mol);
            for a in mol.atoms.iter_mut() {
                a.pos[0] += rng.f32_range(-0.2, 0.2);
                a.pos[1] += rng.f32_range(-0.2, 0.2);
            }

            let st = CleanupState::new(&mol);
            // `angle_amp_max` knowingly trades exactness for a bounded force once an arm is
            // shorter than L/2, i.e. the two atoms already overlap.  Skip those geometries.
            let min_bond = st
                .bonds
                .iter()
                .map(|&(a, b)| {
                    let (p, q) = (st.pos[&a], st.pos[&b]);
                    ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt()
                })
                .fold(f32::INFINITY, f32::min);
            if min_bond < L * 0.5 {
                continue;
            }

            let (net, torque, scale) = net_force_and_torque(&st);
            let f_sum: f32 = st
                .forces
                .values()
                .map(|f| (f[0] * f[0] + f[1] * f[1]).sqrt())
                .sum::<f32>()
                .max(1.0);

            let net_mag = (net[0] * net[0] + net[1] * net[1]).sqrt();
            assert!(
                net_mag < 1e-3 * f_sum,
                "net force {net_mag:.6} is not zero (Σ|F| = {f_sum:.3}): \
                 the force field translates the whole molecule"
            );
            assert!(
                torque.abs() < 1e-3 * scale.max(1.0),
                "net torque {torque:.6} is not zero (Σ|r||F| = {scale:.3}): the force field \
                 is non-conservative and will spin the molecule forever instead of converging"
            );
            checked += 1;
        }

        assert!(checked >= 20, "only {checked} structures were suitable for the check");
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

    // ─── Characterization: today's cleanup result, pinned bit for bit ─────────
    //
    // A safety net for restructuring how the two-phase schedule (relax → ring rescue) is
    // driven: `cleanup_2d` and the native `CleanupJob` must keep producing exactly the
    // coordinates they produce now.  The fingerprints below were recorded from the code as it
    // stood, not derived from any expectation of what a "good" layout is.  The f32 trig calls
    // (`atan2`) come from the platform's libm and 100k relaxation steps amplify a 1-ulp
    // difference, so they were recorded on x86_64-pc-windows-msvc; if they disagree on another
    // platform without any code change, that is the libm, not a regression.

    /// What phase 2 (`ring_rescue`) ended up doing for a fixture.
    #[derive(Debug, PartialEq, Clone, Copy)]
    enum Rescue {
        /// Phase 1 left no crossed ring, so `apply_ring_fix` was never called.
        NotNeeded,
        /// The first rescue pass did not help and was rolled back to the phase-1 result.
        Rejected,
        /// The first pass reduced the crossing count and was kept; nothing further was kept.
        KeptOnePass,
        /// The repeat loop kept a second (or later) pass as well.
        KeptSeveralPasses,
    }

    struct Fixture {
        name: &'static str,
        mol: Molecule,
        rescue: Rescue,
        /// `fingerprint` of the coordinates `cleanup_2d` produces today.
        fingerprint: u64,
    }

    /// Exact coordinates (as f32 bit patterns) in `mol.atoms` order.
    fn coord_bits(mol: &Molecule) -> Vec<(u32, [u32; 2])> {
        mol.atoms.iter().map(|a| (a.id, [a.pos[0].to_bits(), a.pos[1].to_bits()])).collect()
    }

    fn position_bits(positions: &[(u32, [f32; 2])]) -> Vec<(u32, [u32; 2])> {
        positions.iter().map(|&(id, p)| (id, [p[0].to_bits(), p[1].to_bits()])).collect()
    }

    /// FNV-1a over every atom's id and coordinate bits, in order.
    fn fingerprint(bits: &[(u32, [u32; 2])]) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for &(id, [x, y]) in bits {
            for word in [id, x, y] {
                for byte in word.to_le_bytes() {
                    h ^= u64::from(byte);
                    h = h.wrapping_mul(0x0000_0100_0000_01b3);
                }
            }
        }
        h
    }

    /// Molecules `convergence_50_random` draws (same seed and recipe), picked by trial index.
    fn random_trials(picks: &[usize]) -> Vec<Molecule> {
        let last = *picks.iter().max().unwrap();
        let mut rng = Lcg(0xDEAD_BEEF_CAFE_1234);
        let mut out = Vec::new();
        for trial in 0..=last {
            let n_atoms = rng.usize_range(2, 21);
            let extra_bonds = rng.usize_range(0, n_atoms / 2 + 1);
            let range = rng.f32_range(5.0, 15.0);
            let mol = random_molecule(&mut rng, n_atoms, extra_bonds, range);
            if picks.contains(&trial) {
                out.push(mol);
            }
        }
        out
    }

    fn characterization_fixtures() -> Vec<Fixture> {
        let empty = Molecule::default();

        let mut single = Molecule::default();
        single.add_atom("C".to_string(), [0.7, -0.3], 0);

        // C0–C1≡C2 bent to ~90° at the sp carbon (exercises the linear-centre term).
        let mut propyne = Molecule::default();
        let c0 = propyne.add_atom("C".to_string(), [0.0, 0.0], 0);
        let c1 = propyne.add_atom("C".to_string(), [1.5, 0.0], 0);
        let c2 = propyne.add_atom("C".to_string(), [1.5, 1.5], 0);
        propyne.add_bond(c0, c1, BondOrder::Single);
        propyne.add_bond(c1, c2, BondOrder::Triple);

        // A badly drawn but uncrossed benzene ring with a methyl group.
        let mut toluene = Molecule::default();
        let ring: Vec<u32> = [[0.0, 1.2], [1.4, 0.9], [1.1, -0.6], [0.1, -1.7], [-1.3, -0.4], [-1.0, 0.5]]
            .iter()
            .map(|&p| toluene.add_atom("C".to_string(), p, 0))
            .collect();
        for i in 0..6 {
            let order = if i % 2 == 0 { BondOrder::Double } else { BondOrder::Single };
            toluene.add_bond(ring[i], ring[(i + 1) % 6], order);
        }
        let methyl = toluene.add_atom("C".to_string(), [0.3, 2.0], 0);
        toluene.add_bond(ring[0], methyl, BondOrder::Single);

        let mut random = random_trials(&[1, 2, 5, 10, 22, 42]).into_iter();
        let mut next = || random.next().unwrap();

        vec![
            Fixture { name: "empty", mol: empty, rescue: Rescue::NotNeeded, fingerprint: 0xcbf2_9ce4_8422_2325 },
            Fixture { name: "single_atom", mol: single, rescue: Rescue::NotNeeded, fingerprint: 0x5f24_2d39_c242_2be4 },
            Fixture { name: "propyne_bent", mol: propyne, rescue: Rescue::NotNeeded, fingerprint: 0x0dcf_aa10_fc30_b7e9 },
            Fixture { name: "toluene_distorted", mol: toluene, rescue: Rescue::NotNeeded, fingerprint: 0x1d45_d142_bb29_38ca },
            // Random trials, chosen to cover every route through phase 2 (see `Rescue`).
            Fixture { name: "random_trial_01", mol: next(), rescue: Rescue::NotNeeded, fingerprint: 0x7c68_05f3_d142_0581 },
            Fixture { name: "random_trial_02", mol: next(), rescue: Rescue::NotNeeded, fingerprint: 0xf609_5d14_1c2e_9375 },
            Fixture { name: "random_trial_05", mol: next(), rescue: Rescue::KeptOnePass, fingerprint: 0xa05e_aa4f_57dd_f0dd },
            Fixture { name: "random_trial_10", mol: next(), rescue: Rescue::KeptSeveralPasses, fingerprint: 0xf461_4345_0557_b563 },
            Fixture { name: "random_trial_22", mol: next(), rescue: Rescue::Rejected, fingerprint: 0x88b0_afef_6dfb_28b9 },
            Fixture { name: "random_trial_42", mol: next(), rescue: Rescue::KeptOnePass, fingerprint: 0x2f8b_bd1c_9ab9_256a },
        ]
    }

    /// Run the native background job to completion the way `poll_cleanup` consumes it.
    #[cfg(not(target_arch = "wasm32"))]
    fn run_native_job(mol: &Molecule) -> Vec<(u32, [f32; 2])> {
        let job = CleanupJob::start(mol);
        while !job.is_done() {
            std::thread::yield_now();
        }
        let positions = job.latest();
        job.join();
        positions
    }

    /// Which route through phase 2 `cleanup_2d` took to produce `got`, found by replaying
    /// phase 1 and a single rescue pass by hand and comparing.
    fn observed_rescue(name: &str, mol: &Molecule, got: &[(u32, [u32; 2])], cleaned: &Molecule) -> Rescue {
        if mol.atoms.is_empty() {
            return Rescue::NotNeeded;
        }
        let mut st = CleanupState::new(mol);
        st.step(JOB_PHASE_BUDGET);
        let after_phase1 = position_bits(&st.positions());
        let crossings_after_phase1 = st.ring_crossing_count();
        if crossings_after_phase1 == 0 {
            assert_eq!(got, after_phase1, "{name}: no crossed ring, yet the result is not phase 1's");
            return Rescue::NotNeeded;
        }
        if got == after_phase1 {
            return Rescue::Rejected;
        }

        st.apply_ring_fix();
        st.step(JOB_PHASE_BUDGET);
        let after_one_pass = position_bits(&st.positions());
        let crossings_after_one_pass = st.ring_crossing_count();
        assert!(
            crossings_after_one_pass < crossings_after_phase1,
            "{name}: a rescue pass was kept although it did not reduce crossings \
             ({crossings_after_phase1} → {crossings_after_one_pass})"
        );
        let crossings_final = CleanupState::new(cleaned).ring_crossing_count();
        if crossings_final < crossings_after_one_pass {
            Rescue::KeptSeveralPasses
        } else {
            // Any later pass was rolled back, which restores the one-pass result exactly.
            assert_eq!(got, after_one_pass, "{name}: later passes were rejected, yet the result moved");
            Rescue::KeptOnePass
        }
    }

    /// `cleanup_2d` still produces exactly the recorded coordinates, and each fixture still
    /// takes the recorded route through phase 2 — so every branch of `ring_rescue` (skipped,
    /// rolled back, one pass kept, repeated passes kept) is pinned.
    #[test]
    fn cleanup_2d_matches_recorded_coordinates() {
        let fixtures = characterization_fixtures();
        for route in [Rescue::NotNeeded, Rescue::Rejected, Rescue::KeptOnePass, Rescue::KeptSeveralPasses] {
            assert!(fixtures.iter().any(|f| f.rescue == route), "no fixture covers {route:?}");
        }

        let mut mismatches = Vec::new();
        for fx in &fixtures {
            let mut mol = fx.mol.clone();
            cleanup_2d(&mut mol);
            let got = coord_bits(&mol);

            let rescue = observed_rescue(fx.name, &fx.mol, &got, &mol);
            assert_eq!(rescue, fx.rescue, "{}: route through ring_rescue changed", fx.name);

            let fp = fingerprint(&got);
            if fp != fx.fingerprint {
                mismatches.push(format!(
                    "{}: fingerprint 0x{fp:016x} (recorded 0x{:016x}), coordinates {:?}",
                    fx.name,
                    fx.fingerprint,
                    mol.atoms.iter().map(|a| (a.id, a.pos)).collect::<Vec<_>>()
                ));
            }
        }
        assert!(mismatches.is_empty(), "cleanup_2d output changed:\n{}", mismatches.join("\n"));
    }

    /// The native background job and the synchronous `cleanup_2d` agree bit for bit.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn native_job_matches_cleanup_2d() {
        for fx in characterization_fixtures() {
            let mut mol = fx.mol.clone();
            cleanup_2d(&mut mol);
            let from_job = position_bits(&run_native_job(&fx.mol));
            assert_eq!(from_job, coord_bits(&mol), "{}: CleanupJob differs from cleanup_2d", fx.name);
        }
    }

    /// Maintenance script (not a real test — `#[ignore]`d so it never runs in CI):
    /// read every fragment JSON under `assets/fragments/` and `fragments/`, relax its
    /// geometry with `cleanup_2d`, re-center on the attach atom, and write it back.
    /// Run with:
    ///   cargo test cleanup_fragment_geometries -- --ignored --nocapture
    #[test]
    #[ignore]
    fn cleanup_fragment_geometries() {
        use crate::config::FragmentDef;

        fn round4(v: f32) -> f32 {
            (v * 10_000.0).round() / 10_000.0
        }

        fn process_dir(dir: &str) -> usize {
            let path = std::path::Path::new(dir);
            if !path.is_dir() {
                return 0;
            }
            let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(path)
                .unwrap()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().map_or(false, |x| x == "json"))
                .collect();
            files.sort();

            let mut count = 0;
            for file in files {
                let Ok(text) = std::fs::read_to_string(&file) else { continue };
                let mut def: FragmentDef = match serde_json::from_str(&text) {
                    Ok(d) => d,
                    Err(e) => {
                        eprintln!("skip {:?}: {e}", file.file_name().unwrap());
                        continue;
                    }
                };
                if def.atoms.is_empty() {
                    continue;
                }

                // Build a molecule mirroring the fragment topology.
                let mut mol = Molecule::default();
                let ids: Vec<u32> = def
                    .atoms
                    .iter()
                    .map(|a| mol.add_atom(a.element.clone(), a.pos, a.charge))
                    .collect();
                for b in &def.bonds {
                    mol.add_bond(ids[b.begin], ids[b.end], b.order.clone());
                }

                // Relax 2D coordinates.
                cleanup_2d(&mut mol);

                // Re-center so the attach atom sits at the origin (insert_fragment expects
                // attach-relative coordinates).
                let attach = mol.atom_by_id(ids[def.attach_idx]).unwrap().pos;
                for (i, a) in def.atoms.iter_mut().enumerate() {
                    let p = mol.atom_by_id(ids[i]).unwrap().pos;
                    a.pos = [round4(p[0] - attach[0]), round4(p[1] - attach[1])];
                }

                let out = serde_json::to_string_pretty(&def).unwrap();
                std::fs::write(&file, out + "\n").unwrap();
                eprintln!(
                    "cleaned {:?} ({} atoms)",
                    file.file_name().unwrap(),
                    def.atoms.len()
                );
                count += 1;
            }
            count
        }

        let a = process_dir("assets/fragments");
        let b = process_dir("fragments");
        eprintln!("done: {a} files in assets/fragments, {b} in fragments");
        assert!(a + b > 0, "no fragment files were processed");
    }
}
