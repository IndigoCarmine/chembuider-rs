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

/// Persistent state for continuous cleanup.  Create with `CleanupState::new` (or
/// `new_partial` to clean up a selection), advance with `step`, and write back to the molecule
/// with `apply`.
pub struct CleanupState {
    pos: HashMap<u32, [f32; 2]>,
    vel: HashMap<u32, [f32; 2]>,
    atom_ids: Vec<u32>,
    // The atoms the relaxation moves, in `atom_ids` order: every atom for a whole-page
    // cleanup.  For a selection cleanup only the selected ones; the others still pull on them
    // through bonds, angles and repulsion but never move themselves.
    movable: Vec<u32>,
    // `None` for a whole-page cleanup, which keeps the structure centred on the origin.  For a
    // selection cleanup, the groups of selected atoms that no bond ties to an unselected atom,
    // each held at the centroid it was drawn at in place of that recentring.
    free_groups: Option<Vec<FreeGroup>>,
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

/// Selection cleanup: selected atoms bonded, through selected atoms, to no unselected one
/// (a whole fragment).  Nothing anchors them, so like a whole-page cleanup they are kept from
/// drifting or spinning, about the centroid they were drawn at.
struct FreeGroup {
    ids: Vec<u32>,
    centroid: [f32; 2],
}

impl CleanupState {
    pub fn new(mol: &Molecule) -> Self {
        Self::build(mol, None)
    }

    /// Clean up only the atoms in `movable`: every other atom keeps its exact position, and the
    /// selected atoms are laid out against them (a bond from a selected atom to an unselected
    /// one gets the standard length, the angles at the unselected atom the usual spread).  A
    /// selected fragment bonded to nothing unselected is cleaned up as a whole and stays
    /// centred where it was drawn.  Ids not in the molecule are ignored.
    ///
    /// Like the whole-page cleanup this is a local relaxation: selected atoms drawn far on the
    /// wrong side of the fixed ones can settle with a bond left stretched.
    pub fn new_partial(mol: &Molecule, movable: &HashSet<u32>) -> Self {
        Self::build(mol, Some(movable))
    }

    fn build(mol: &Molecule, selection: Option<&HashSet<u32>>) -> Self {
        let atom_ids: Vec<u32> = mol.atoms.iter().map(|a| a.id).collect();
        let mut pos: HashMap<u32, [f32; 2]> = mol.atoms.iter().map(|a| (a.id, a.pos)).collect();
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

        let mut cycles = find_simple_cycles(&adj, &atom_ids);
        let has_rings;
        let movable: Vec<u32>;
        let free_groups;
        match selection {
            None => {
                has_rings = !cycles.is_empty();
                movable = atom_ids.clone();
                free_groups = None;
            }
            Some(sel) => {
                // Friction damps the oscillation of any ring a moving atom sits in; the ring
                // rescue only permutes rings whose atoms may all move.
                has_rings = cycles.iter().any(|ring| ring.iter().any(|id| sel.contains(id)));
                cycles.retain(|ring| ring.iter().all(|id| sel.contains(id)));
                movable = atom_ids.iter().copied().filter(|id| sel.contains(id)).collect();
                free_groups = Some(free_groups_of(&adj, &movable, sel, &pos));
                separate_coincident(&mut pos, &adj, &atom_ids, &movable);
            }
        }

        // Pre-compute initial forces so the first VV half-kick uses real forces, not zeros.
        compute_forces(&pos, &bonds, &adj, &atom_ids, &bonded, &linear_atoms, &PARAMS, &mut forces);

        let ref_pos: Vec<[f32; 2]> = movable.iter().map(|id| pos[id]).collect();

        CleanupState {
            pos, vel, atom_ids, movable, free_groups, forces, bonds, adj, bonded,
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
        for (k, &id) in self.movable.iter().enumerate() {
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
        self.movable.iter()
            .map(|id| { let f = self.forces[id]; f[0] * f[0] + f[1] * f[1] })
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
            for &id in &self.movable {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] += f[0] * dt * 0.5;
                v[1] += f[1] * dt * 0.5;
            }

            // 2. Drift.
            for &id in &self.movable {
                let v = self.vel[&id];
                let q = self.pos.get_mut(&id).unwrap();
                q[0] += v[0] * dt;
                q[1] += v[1] * dt;
            }
            match &self.free_groups {
                None => recenter(&mut self.pos, &self.atom_ids, [0.0, 0.0]),
                Some(groups) => {
                    for g in groups {
                        recenter(&mut self.pos, &g.ids, g.centroid);
                    }
                }
            }

            // 3. Compute forces at the new positions (overwrites self.forces).
            compute_forces(
                &self.pos, &self.bonds, &self.adj, &self.atom_ids,
                &self.bonded, &self.linear_atoms, p, &mut self.forces,
            );

            // 4. Second half-kick — velocity is now the full VV velocity at the new position.
            for &id in &self.movable {
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
                let n = self.movable.len().max(1) as f32;
                let msd: f32 = self.movable.iter().enumerate()
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
                for (k, &id) in self.movable.iter().enumerate() {
                    self.ref_pos[k] = self.pos[&id];
                }
                self.steps_since_ref = 0;
            }

            // 6. Power P = F · v (full velocity, new forces).
            let power: f32 = self.movable.iter()
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
            let v_norm: f32 = self.movable.iter()
                .map(|id| { let v = self.vel[id]; v[0] * v[0] + v[1] * v[1] })
                .sum::<f32>()
                .sqrt();
            let f_norm: f32 = self.movable.iter()
                .map(|id| { let f = self.forces[id]; f[0] * f[0] + f[1] * f[1] })
                .sum::<f32>()
                .sqrt()
                .max(1e-10);
            let ratio = v_norm / f_norm;
            let alpha = self.alpha;
            for &id in &self.movable {
                let f = self.forces[&id];
                let v = self.vel.get_mut(&id).unwrap();
                v[0] = (1.0 - alpha) * v[0] + alpha * ratio * f[0];
                v[1] = (1.0 - alpha) * v[1] + alpha * ratio * f[1];
            }

            // 9. Clamp per-atom speed to prevent velocity runaway with large initial forces.
            for &id in &self.movable {
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
                for &id in &self.movable {
                    let v = self.vel.get_mut(&id).unwrap();
                    v[0] *= FRICTION;
                    v[1] *= FRICTION;
                }
            }

            // 11. Drain whole-structure translation/rotation so the next drift only relaxes
            //     internal geometry (otherwise residual spin never lets positions converge).
            match &self.free_groups {
                None => remove_rigid_motion(&self.pos, &mut self.vel, &self.atom_ids, [0.0, 0.0]),
                Some(groups) => {
                    for g in groups {
                        remove_rigid_motion(&self.pos, &mut self.vel, &g.ids, g.centroid);
                    }
                }
            }

            // self.forces already holds F(x_new), ready for the next step's first half-kick.
        }
        false
    }

    /// Write the current relaxed positions of the atoms that move back to the molecule (every
    /// atom for a whole-page cleanup, the selected ones for a selection cleanup).
    /// Used through `CleanupSchedule::apply` by the synchronous `cleanup_2d`; the interactive
    /// path streams positions through `CleanupJob` instead.
    pub fn apply(&self, mol: &mut Molecule) {
        for &id in &self.movable {
            if let Some(atom) = mol.atom_by_id_mut(id) {
                atom.pos = self.pos[&id];
            }
        }
    }

    /// Snapshot of the current positions of the atoms that move (atom_ids order) for live
    /// preview / publishing.  A selection cleanup leaves the unselected atoms out: the editor
    /// copies these into the live molecule every frame, so listing an atom the run holds still
    /// would snap it back to where it was when the run started — undoing, say, a drag of it
    /// made while the run goes on.
    pub fn positions(&self) -> Vec<(u32, [f32; 2])> {
        self.movable.iter().map(|&id| (id, self.pos[&id])).collect()
    }
}

// ─── One-shot synchronous entry point ────────────────────────────────────────
// Blocking version, for callers that need the result right away: the reaction-scheme
// layout (`reaction::edit`), tests and the fragment-cleanup script. The interactive editor
// uses `CleanupJob` (background thread + cancel) instead so it never freezes.

pub fn cleanup_2d(mol: &mut Molecule) {
    let mut schedule = CleanupSchedule::new(mol);
    // One call per relaxation pass; `advance` never runs more than one pass's budget.
    while !schedule.advance(JOB_PHASE_BUDGET) {}
    schedule.apply(mol);
}

/// `cleanup_2d` of a selection: only the atoms in `movable` move (see
/// [`CleanupState::new_partial`]); every other atom keeps its exact coordinates.
pub fn cleanup_2d_partial(mol: &mut Molecule, movable: &HashSet<u32>) {
    let mut schedule = CleanupSchedule::new_partial(mol, movable);
    while !schedule.advance(JOB_PHASE_BUDGET) {}
    schedule.apply(mol);
}

/// The two-phase cleanup as a resumable state machine, so a caller can run it to the end in
/// one go, between cancel checks on a worker thread, or one chunk per UI frame where nothing
/// may block.
///
/// Phase 1 relaxes the drawing.  Relaxation is local, so it happily settles into a minimum
/// where a ring is drawn inside-out with its own bonds crossing; phase 2, the ring rescue, then
/// runs `apply_ring_fix` (which permutes those ring atoms back into a valid angular order) and
/// relaxes again from there.
///
/// A rescue pass is kept only when it strictly reduced the number of crossed rings, and passes
/// repeat while they keep helping.  Both halves of that rule are needed to leave `cleanup_2d`
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
///
/// Each relaxation pass gets `JOB_PHASE_BUDGET` steps however it is chunked; `step` keeps all
/// of its state between calls, so the coordinates do not depend on the chunk size.
pub struct CleanupSchedule {
    state: CleanupState,
    stage: Stage,
    /// Steps allowed per relaxation pass (`JOB_PHASE_BUDGET` outside tests).
    pass_budget: usize,
    /// Steps run so far in the current relaxation pass.
    steps_in_pass: usize,
    /// Rescue passes started so far (capped at `cycles.len().max(1)`).
    rescue_passes: usize,
    /// Positions to roll back to if the current rescue pass does not help, and the crossing
    /// count it has to beat.
    rescue_snapshot: Vec<(u32, [f32; 2])>,
    crossings_before: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Stage {
    Relax,
    Rescue,
    Done,
}

impl CleanupSchedule {
    pub fn new(mol: &Molecule) -> Self {
        Self::with_pass_budget(mol, JOB_PHASE_BUDGET)
    }

    /// The cleanup of a selection: only the atoms in `movable` move
    /// (see [`CleanupState::new_partial`]).
    pub fn new_partial(mol: &Molecule, movable: &HashSet<u32>) -> Self {
        Self::from_state(CleanupState::new_partial(mol, movable), JOB_PHASE_BUDGET)
    }

    /// `new` with another per-pass step budget, so tests can reach the end of a pass that ran
    /// out of steps without spending `JOB_PHASE_BUDGET` of them.
    fn with_pass_budget(mol: &Molecule, pass_budget: usize) -> Self {
        Self::from_state(CleanupState::new(mol), pass_budget)
    }

    fn from_state(state: CleanupState, pass_budget: usize) -> Self {
        debug_assert!(pass_budget > 0, "a zero pass budget would never finish a pass");
        CleanupSchedule {
            stage: if state.movable.is_empty() { Stage::Done } else { Stage::Relax },
            state,
            pass_budget,
            steps_in_pass: 0,
            rescue_passes: 0,
            rescue_snapshot: Vec::new(),
            crossings_before: 0,
        }
    }

    /// Run at most `max_steps` relaxation steps (never past the end of the current pass) and
    /// take any phase transition that follows.  Returns `true` once the schedule is finished.
    ///
    /// On a finished schedule this does nothing and returns `true`.  Otherwise `max_steps`
    /// must be positive: `advance(0)` runs no step and so never makes progress.
    pub fn advance(&mut self, max_steps: usize) -> bool {
        if self.stage == Stage::Done {
            return true;
        }
        debug_assert!(max_steps > 0, "advance(0) never makes progress");
        let chunk = max_steps.min(self.pass_budget - self.steps_in_pass);
        let converged = self.state.step(chunk);
        self.steps_in_pass += chunk;
        if converged || self.steps_in_pass >= self.pass_budget {
            self.finish_pass();
        }
        self.stage == Stage::Done
    }

    /// A relaxation pass ended: score a rescue pass (keep it only if it strictly reduced the
    /// crossed rings), then start another rescue pass while rings are still crossed.
    fn finish_pass(&mut self) {
        if self.stage == Stage::Rescue
            && self.state.ring_crossing_count() >= self.crossings_before
        {
            self.state.restore(&self.rescue_snapshot);
            self.stage = Stage::Done;
            return;
        }
        let crossings = self.state.ring_crossing_count();
        if crossings == 0 || self.rescue_passes >= self.state.cycles.len().max(1) {
            self.stage = Stage::Done;
            return;
        }
        self.crossings_before = crossings;
        self.rescue_snapshot = self.state.positions();
        self.state.apply_ring_fix();
        self.rescue_passes += 1;
        self.steps_in_pass = 0;
        self.stage = Stage::Rescue;
    }

    /// Stop early.  Mid-rescue, roll back to the positions from before that rescue pass (an
    /// unfinished pass is never kept); mid-relaxation, keep the atoms where they are now.
    /// Does nothing once the schedule is finished.
    pub fn cancel(&mut self) {
        if self.stage == Stage::Rescue {
            self.state.restore(&self.rescue_snapshot);
        }
        self.stage = Stage::Done;
    }

    pub fn is_done(&self) -> bool {
        self.stage == Stage::Done
    }

    /// Current positions (atom_ids order), for live preview while the schedule runs.
    pub fn positions(&self) -> Vec<(u32, [f32; 2])> {
        self.state.positions()
    }

    /// Write the current positions back to the molecule.
    pub fn apply(&self, mol: &mut Molecule) {
        self.state.apply(mol);
    }
}

// ─── Background, cancellable cleanup job ──────────────────────────────────────

/// Steps run per chunk between cancel checks and position publishes.
const JOB_CHUNK: usize = 500;
const JOB_PHASE_BUDGET: usize = 100_000;

/// One running cleanup computation, as the editor drives it: once per UI frame
/// `ChemStructEditor::poll_cleanup` copies `latest()` into the molecule for a live preview and
/// checks `is_done()`; once that is `true` it calls `join()`.  `cancel()` may come at any time.
///
/// Contract for implementors:
///   * `latest` returns `(atom id, position)` pairs for atoms of the molecule the run was
///     started from — for a run from `CleanupEngine::start_partial`, only the atoms it may
///     move, since the editor writes every listed position into the live molecule.  It may
///     also advance the computation (the wasm run does all its work there, one chunk per
///     call).
///   * Once `is_done` returns `true`, `latest` returns the final positions.
///   * After `cancel`, the run must still reach `is_done` (soon), so the editor can finish.
///   * The run may be dropped without `join` (e.g. when the editor holding it is dropped, or
///     `ChemStructEditor::cancel_cleanup` discards it, possibly before `is_done`); dropping it
///     must stop any background work.
///
/// Methods take `&self` because the editor holds the job behind a shared reference while it
/// polls; a run that needs to mutate uses interior mutability.
pub trait CleanupRun {
    /// Ask the run to stop early.  Must not block.
    fn cancel(&self);
    /// Whether the run has finished (normally or after `cancel`).
    fn is_done(&self) -> bool;
    /// The current positions, for the live preview (final once `is_done`).
    fn latest(&self) -> Vec<(u32, [f32; 2])>;
    /// Release the run's resources; called once, after `is_done` returned `true`.
    fn join(self: Box<Self>);
}

/// Starts cleanup runs: the swappable part behind the editor's Cleanup command.
pub trait CleanupEngine {
    fn start(&self, mol: &Molecule) -> CleanupJob;

    /// Start a cleanup of the selected atoms `movable` only (the editor's Cleanup with part of
    /// the drawing selected): the run must leave every other atom where it is.  Defaults to the
    /// built-in selection cleanup ([`CleanupJob::start_partial`]), so an engine that only
    /// knows how to redraw a whole page still never moves an unselected atom.
    fn start_partial(&self, mol: &Molecule, movable: &HashSet<u32>) -> CleanupJob {
        CleanupJob::start_partial(mol, movable)
    }
}

/// The built-in cleanup: the force-field relaxation plus ring rescue of `CleanupSchedule`,
/// on a background thread natively and one chunk per poll on wasm32 (`CleanupJob::start`).
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultCleanupEngine;

impl CleanupEngine for DefaultCleanupEngine {
    fn start(&self, mol: &Molecule) -> CleanupJob {
        CleanupJob::start(mol)
    }
}

/// A running cleanup computation: a thin wrapper around any [`CleanupRun`], so the editor's
/// `cleanup_job: Option<CleanupJob>` can hold the built-in run or another engine's.
///
/// The run must be `Send + Sync` so that `CleanupJob` — and with it `ChemStructEditor` —
/// stays `Send + Sync` as it was when `CleanupJob` was the concrete threaded job.
pub struct CleanupJob {
    run: Box<dyn CleanupRun + Send + Sync>,
}

impl CleanupJob {
    /// Start the built-in cleanup of a snapshot of `mol` (see [`DefaultCleanupEngine`]).
    pub fn start(mol: &Molecule) -> Self {
        Self::from_run(DefaultRun::start(mol, None))
    }

    /// Start the built-in cleanup of the atoms `movable` only, the others held still (see
    /// [`CleanupState::new_partial`]).
    pub fn start_partial(mol: &Molecule, movable: &HashSet<u32>) -> Self {
        Self::from_run(DefaultRun::start(mol, Some(movable)))
    }

    /// Wrap another engine's run.
    pub fn from_run(run: impl CleanupRun + Send + Sync + 'static) -> Self {
        CleanupJob { run: Box::new(run) }
    }

    /// Request the run to stop early.
    pub fn cancel(&self) {
        self.run.cancel();
    }

    pub fn is_done(&self) -> bool {
        self.run.is_done()
    }

    /// Latest relaxed positions (for live preview each frame).
    pub fn latest(&self) -> Vec<(u32, [f32; 2])> {
        self.run.latest()
    }

    /// Finish the run (call once `is_done`).
    pub fn join(self) {
        self.run.join();
    }
}

#[cfg(not(target_arch = "wasm32"))]
use job_native::ThreadedRun as DefaultRun;
#[cfg(target_arch = "wasm32")]
use job_wasm::PolledRun as DefaultRun;

#[cfg(not(target_arch = "wasm32"))]
mod job_native {
    use super::{run_cleanup_job, CleanupRun, CleanupSchedule, Molecule};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;

    /// The built-in cleanup running on a background thread. The UI thread polls
    /// `latest()` for a live preview and `is_done()` for completion, and may `cancel()`
    /// at any time.
    pub struct ThreadedRun {
        cancel: Arc<AtomicBool>,
        done: Arc<AtomicBool>,
        shared: Arc<Mutex<Vec<(u32, [f32; 2])>>>,
        handle: Option<JoinHandle<()>>,
    }

    impl ThreadedRun {
        /// Spawn a worker that relaxes a snapshot of `mol` off the UI thread: all of it, or
        /// only the atoms in `movable`.
        pub fn start(mol: &Molecule, movable: Option<&HashSet<u32>>) -> Self {
            let cancel = Arc::new(AtomicBool::new(false));
            let done = Arc::new(AtomicBool::new(false));
            // Until the worker publishes, the preview is the atoms it will move, where they are.
            let shared = Arc::new(Mutex::new(
                mol.atoms
                    .iter()
                    .filter(|a| movable.is_none_or(|m| m.contains(&a.id)))
                    .map(|a| (a.id, a.pos))
                    .collect::<Vec<_>>(),
            ));

            let snapshot = mol.clone();
            let movable = movable.cloned();
            let (c, d, s) = (cancel.clone(), done.clone(), shared.clone());
            let handle = std::thread::spawn(move || {
                let schedule = match &movable {
                    None => CleanupSchedule::new(&snapshot),
                    Some(movable) => CleanupSchedule::new_partial(&snapshot, movable),
                };
                run_cleanup_job(schedule, &c, &d, &s);
            });

            ThreadedRun { cancel, done, shared, handle: Some(handle) }
        }
    }

    impl CleanupRun for ThreadedRun {
        /// Request the worker to stop at the next chunk boundary.
        fn cancel(&self) {
            self.cancel.store(true, Ordering::Relaxed);
        }

        fn is_done(&self) -> bool {
            // Acquire pairs with the worker's Release store, so once this reads `true` the
            // final positions it published are what `latest()` returns.
            self.done.load(Ordering::Acquire)
        }

        /// Latest relaxed positions (for live preview each frame).
        fn latest(&self) -> Vec<(u32, [f32; 2])> {
            self.shared.lock().unwrap().clone()
        }

        /// Join the worker thread (call once `is_done`).
        fn join(mut self: Box<Self>) {
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    impl Drop for ThreadedRun {
        fn drop(&mut self) {
            // Make sure a still-running worker stops if the job is dropped without joining.
            self.cancel.store(true, Ordering::Relaxed);
        }
    }
}

// wasm32-unknown-unknown has no `std::thread::spawn` (it always returns an "unsupported"
// error, and `thread::spawn` panics on that) — there's no Web Worker / SharedArrayBuffer
// setup here for real background threads. Instead, `latest()` — already polled once per UI
// frame by `ChemStructEditor::poll_cleanup` — advances the same `CleanupSchedule` as
// `cleanup_2d` and the native worker by one `JOB_CHUNK` on each call, so the tab still
// repaints between chunks instead of blocking on one huge synchronous relaxation.
// wasm32-unknown-unknown is single-threaded, so the `Mutex` here is never contended; it (and
// the `AtomicBool`) are used instead of `RefCell`/`Cell` only because `CleanupJob` requires its
// run to be `Sync`, and a `RefCell` is not.
#[cfg(target_arch = "wasm32")]
mod job_wasm {
    use super::{CleanupRun, CleanupSchedule, Molecule, JOB_CHUNK};
    use std::collections::HashSet;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;

    pub struct PolledRun {
        schedule: Mutex<CleanupSchedule>,
        /// Set by `cancel`; the next `latest` call stops the schedule, as the native worker
        /// does at its next chunk boundary.
        cancelled: AtomicBool,
    }

    impl PolledRun {
        pub fn start(mol: &Molecule, movable: Option<&HashSet<u32>>) -> Self {
            let schedule = match movable {
                None => CleanupSchedule::new(mol),
                Some(movable) => CleanupSchedule::new_partial(mol, movable),
            };
            PolledRun {
                schedule: Mutex::new(schedule),
                cancelled: AtomicBool::new(false),
            }
        }
    }

    impl CleanupRun for PolledRun {
        fn cancel(&self) {
            self.cancelled.store(true, Ordering::Relaxed);
        }

        fn is_done(&self) -> bool {
            self.schedule.lock().unwrap().is_done()
        }

        /// Advance the schedule by one chunk (or stop it, once cancelled), then return the
        /// positions.  On a finished schedule both `advance` and `cancel` do nothing, so this
        /// then just returns the final positions.
        fn latest(&self) -> Vec<(u32, [f32; 2])> {
            let mut schedule = self.schedule.lock().unwrap();
            if self.cancelled.load(Ordering::Relaxed) {
                schedule.cancel();
            } else {
                schedule.advance(JOB_CHUNK);
            }
            schedule.positions()
        }

        /// Nothing to join — there is no worker thread on wasm32.
        fn join(self: Box<Self>) {}
    }
}

#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

/// Worker body: the same `CleanupSchedule` as `cleanup_2d` (or `cleanup_2d_partial`),
/// advanced `JOB_CHUNK` steps at a time so the worker can check the cancel flag and publish
/// intermediate positions for the live preview in between.
#[cfg(not(target_arch = "wasm32"))]
fn run_cleanup_job(
    mut schedule: CleanupSchedule,
    cancel: &AtomicBool,
    done: &AtomicBool,
    shared: &Mutex<Vec<(u32, [f32; 2])>>,
) {
    while !schedule.is_done() {
        if cancel.load(Ordering::Relaxed) {
            schedule.cancel();
            break;
        }
        schedule.advance(JOB_CHUNK);
        *shared.lock().unwrap() = schedule.positions();
    }

    *shared.lock().unwrap() = schedule.positions();
    // Release: publishes the final positions above to whoever sees `done` (see `is_done`).
    done.store(true, Ordering::Release);
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
                // A quadruple bond (a metal–metal bond) implies no linear geometry.
                BondOrder::Single | BondOrder::Quadruple => {}
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

/// Move the atoms `ids` together so their centroid is at `target`.  (Subtracting a zero
/// `target` is exact, so recentring on the origin is unchanged by the parameter.)
fn recenter(pos: &mut HashMap<u32, [f32; 2]>, ids: &[u32], target: [f32; 2]) {
    if ids.is_empty() {
        return;
    }
    let (sx, sy) = ids.iter().fold((0.0_f32, 0.0_f32), |(ax, ay), id| {
        let p = pos[id];
        (ax + p[0], ay + p[1])
    });
    let n = ids.len() as f32;
    let (cx, cy) = (sx / n - target[0], sy / n - target[1]);
    for id in ids {
        let p = pos.get_mut(id).unwrap();
        p[0] -= cx;
        p[1] -= cy;
    }
}

/// The free groups of a selection cleanup: the connected pieces of the selected atoms
/// (`movable`, in atom order) that have no bond to an unselected atom, each with its current
/// centroid.
fn free_groups_of(
    adj: &HashMap<u32, Vec<u32>>,
    movable: &[u32],
    sel: &HashSet<u32>,
    pos: &HashMap<u32, [f32; 2]>,
) -> Vec<FreeGroup> {
    let mut seen: HashSet<u32> = HashSet::new();
    let mut groups = Vec::new();
    for &start in movable {
        if !seen.insert(start) {
            continue;
        }
        let mut ids = vec![start];
        let mut anchored = false;
        let mut k = 0;
        while k < ids.len() {
            for &nb in &adj[&ids[k]] {
                if !sel.contains(&nb) {
                    anchored = true;
                } else if seen.insert(nb) {
                    ids.push(nb);
                }
            }
            k += 1;
        }
        if !anchored {
            let n = ids.len() as f32;
            let (sx, sy) = ids.iter().fold((0.0_f32, 0.0_f32), |(ax, ay), id| (ax + pos[id][0], ay + pos[id][1]));
            groups.push(FreeGroup { ids, centroid: [sx / n, sy / n] });
        }
    }
    groups
}

/// Two atoms closer than this (in each coordinate) count as drawn on top of each other.
const COINCIDENT: f32 = 1e-4;

/// Selection cleanup: move each selected atom drawn on top of another atom a short way off it,
/// so the relaxation can pull the two apart.  At zero separation every force between them has
/// no direction (the bond's, the repulsion's and the angle terms' tangents all vanish); in a
/// whole-page cleanup the other bonds of the pair shift them off each other, but an unselected
/// atom never moves, so a selected one sitting exactly on it would stay stuck there.  The
/// selected atom is moved away from the other atom's remaining neighbours (where a new bond
/// would be drawn), or, with none to go by, along a direction set by its place in `movable`:
/// deterministic, so the job and `cleanup_2d_partial` agree.
fn separate_coincident(
    pos: &mut HashMap<u32, [f32; 2]>,
    adj: &HashMap<u32, Vec<u32>>,
    atom_ids: &[u32],
    movable: &[u32],
) {
    for (k, &id) in movable.iter().enumerate() {
        let p = pos[&id];
        let Some(&other) = atom_ids.iter().find(|&&o| {
            let q = pos[&o];
            o != id && (q[0] - p[0]).abs() < COINCIDENT && (q[1] - p[1]).abs() < COINCIDENT
        }) else {
            continue;
        };
        let q = pos[&other];
        let (sx, sy) = adj[&other].iter().filter(|&&nb| nb != id).fold((0.0_f32, 0.0_f32), |(x, y), nb| {
            (x + pos[nb][0] - q[0], y + pos[nb][1] - q[1])
        });
        let len = (sx * sx + sy * sy).sqrt();
        let dir = if len > COINCIDENT {
            [-sx / len, -sy / len]
        } else {
            // The golden angle spreads several such atoms apart.
            let a = 2.399_963 * (k + 1) as f32;
            [a.cos(), a.sin()]
        };
        pos.insert(id, [q[0] + dir[0] * L * 0.1, q[1] + dir[1] * L * 0.1]);
    }
}

/// Project out rigid-body motion (net translation + net rotation) from the velocities.
/// Whole-structure translation and rotation are zero-energy modes with no restoring force;
/// the angle forces are not perfectly torque-free, so they keep pumping angular momentum into
/// the rotation mode and the structure spins forever — which, with motion-based convergence,
/// never settles.  Draining that net motion each step lets only the internal geometry relax,
/// and keeps the result at the same orientation the user drew.  Assumes `ids` are centred on
/// `center`.
fn remove_rigid_motion(
    pos: &HashMap<u32, [f32; 2]>,
    vel: &mut HashMap<u32, [f32; 2]>,
    ids: &[u32],
    center: [f32; 2],
) {
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

    // Angular velocity ω = L / I about the centroid, using the de-translated velocity.
    let arm = |id: &u32| [pos[id][0] - center[0], pos[id][1] - center[1]];
    let (mut l, mut inertia) = (0.0_f32, 0.0_f32);
    for id in ids {
        let r = arm(id);
        let v = vel[id];
        l += r[0] * (v[1] - vy) - r[1] * (v[0] - vx);
        inertia += r[0] * r[0] + r[1] * r[1];
    }
    let omega = if inertia > 1e-9 { l / inertia } else { 0.0 };

    // Subtract both the linear and rotational (v_rot = (-ω·y, ω·x)) components.
    for id in ids {
        let r = arm(id);
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

    /// What phase 2 (the ring rescue in `CleanupSchedule`) ended up doing for a fixture.
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
    /// takes the recorded route through phase 2 — so every branch of the ring rescue (skipped,
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
            assert_eq!(rescue, fx.rescue, "{}: route through the ring rescue changed", fx.name);

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

    /// Drive a job the way `poll_cleanup` does (call `latest` until `is_done`), on any target.
    fn run_polled(job: CleanupJob) -> Vec<(u32, [f32; 2])> {
        while !job.is_done() {
            job.latest();
            std::thread::yield_now();
        }
        let positions = job.latest();
        job.join();
        positions
    }

    /// `DefaultCleanupEngine` is the built-in cleanup: same coordinates as `cleanup_2d`.
    #[test]
    fn default_engine_matches_cleanup_2d() {
        for fx in characterization_fixtures() {
            let mut mol = fx.mol.clone();
            cleanup_2d(&mut mol);
            let from_engine = position_bits(&run_polled(DefaultCleanupEngine.start(&fx.mol)));
            assert_eq!(from_engine, coord_bits(&mol), "{}: DefaultCleanupEngine differs from cleanup_2d", fx.name);
        }
    }

    /// A run that records what was forwarded to it.
    struct FakeRun {
        positions: Vec<(u32, [f32; 2])>,
        done: bool,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        joined: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl CleanupRun for FakeRun {
        fn cancel(&self) {
            self.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        fn is_done(&self) -> bool {
            self.done
        }
        fn latest(&self) -> Vec<(u32, [f32; 2])> {
            self.positions.clone()
        }
        fn join(self: Box<Self>) {
            self.joined.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// `CleanupJob::from_run` wraps any run and forwards every call to it.
    #[test]
    fn from_run_forwards_to_the_run() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        for done in [false, true] {
            let cancelled = Arc::new(AtomicBool::new(false));
            let joined = Arc::new(AtomicBool::new(false));
            let positions = vec![(3, [1.5, -2.0]), (7, [0.25, 4.0])];
            let job = CleanupJob::from_run(FakeRun {
                positions: positions.clone(),
                done,
                cancelled: cancelled.clone(),
                joined: joined.clone(),
            });

            assert_eq!(job.latest(), positions);
            assert_eq!(job.is_done(), done);
            assert!(!cancelled.load(Ordering::Relaxed));
            job.cancel();
            assert!(cancelled.load(Ordering::Relaxed));
            assert!(!joined.load(Ordering::Relaxed));
            job.join();
            assert!(joined.load(Ordering::Relaxed));
        }
    }

    /// An engine whose runs report fixed positions, standing in for the force field.
    struct FakeEngine {
        positions: Vec<(u32, [f32; 2])>,
        done: bool,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        joined: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl CleanupEngine for FakeEngine {
        fn start(&self, _mol: &Molecule) -> CleanupJob {
            CleanupJob::from_run(FakeRun {
                positions: self.positions.clone(),
                done: self.done,
                cancelled: self.cancelled.clone(),
                joined: self.joined.clone(),
            })
        }
    }

    /// An editor set up by [`editor_with_fake_engine`], and what the tests check it against.
    struct FakeEngineFixture {
        editor: crate::widget::ChemStructEditor,
        /// The molecule before any cleanup.
        before: Molecule,
        /// The positions the fake engine's runs report.
        positions: Vec<(u32, [f32; 2])>,
        /// Set once the run is cancelled.
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        /// Set once the run is joined.
        joined: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    /// An editor with two atoms, cleaned up by a `FakeEngine` that moves both of them.
    fn editor_with_fake_engine(done: bool) -> FakeEngineFixture {
        let mut editor = crate::widget::ChemStructEditor::default();
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [5.0, 5.0], 0);
        let before = editor.molecule.clone();
        let positions = vec![(a, [1.25, -0.5]), (b, [2.5, 0.75])];
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let joined = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        editor.set_cleanup_engine(FakeEngine {
            positions: positions.clone(),
            done,
            cancelled: cancelled.clone(),
            joined: joined.clone(),
        });
        FakeEngineFixture { editor, before, positions, cancelled, joined }
    }

    fn atom_positions(mol: &Molecule) -> Vec<(u32, [f32; 2])> {
        mol.atoms.iter().map(|a| (a.id, a.pos)).collect()
    }

    /// The editor's Cleanup runs the engine set with `set_cleanup_engine`: `poll_cleanup`
    /// copies that engine's positions in, and on completion joins the run, clears
    /// `cleanup_job` and leaves exactly one undo step back to the pre-cleanup molecule.
    #[test]
    fn set_cleanup_engine_drives_toggle_and_poll() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, before, positions, joined, .. } = editor_with_fake_engine(true);

        editor.toggle_cleanup();
        assert!(editor.cleanup_job.is_some());
        assert_eq!(editor.undo_stack.len(), 1);
        assert_eq!(editor.undo_stack.back(), Some(&before));

        assert!(!editor.poll_cleanup(), "a finished run ends the cleanup");
        assert_eq!(atom_positions(&editor.molecule), positions);
        assert!(editor.cleanup_job.is_none());
        assert!(joined.load(Ordering::Relaxed));
        assert_eq!(editor.undo_stack.len(), 1);

        assert!(editor.undo());
        assert_eq!(editor.molecule, before);
    }

    /// While the engine's run is not done, `poll_cleanup` previews its positions and keeps
    /// the job.
    #[test]
    fn unfinished_engine_run_keeps_the_job() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, positions, joined, .. } = editor_with_fake_engine(false);

        editor.toggle_cleanup();
        assert!(editor.poll_cleanup(), "an unfinished run keeps the cleanup active");
        assert_eq!(atom_positions(&editor.molecule), positions);
        assert!(editor.cleanup_job.is_some());
        assert!(!joined.load(Ordering::Relaxed));
        assert_eq!(editor.undo_stack.len(), 1);
    }

    /// Undo while a cleanup runs cancels it: the run is cancelled and dropped, the molecule is
    /// back to its pre-cleanup structure, a later poll writes nothing over it, and neither
    /// stack keeps the half-relaxed preview (the redo history is the one from before Cleanup).
    #[test]
    fn undo_during_cleanup_cancels_it_and_restores_the_pre_cleanup_molecule() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, before, positions, cancelled, joined } =
            editor_with_fake_engine(false);

        editor.toggle_cleanup();
        let redo_at_start = editor.redo_stack.clone();
        assert!(editor.poll_cleanup());
        assert_eq!(atom_positions(&editor.molecule), positions, "preview applied");

        assert!(editor.undo());
        assert_eq!(editor.molecule, before);
        assert!(editor.cleanup_job.is_none());
        assert!(!editor.is_cleaning());
        assert!(cancelled.load(Ordering::Relaxed), "the run was cancelled");
        assert!(!joined.load(Ordering::Relaxed), "the run was dropped, not joined");
        assert_eq!(std::sync::Arc::strong_count(&cancelled), 2, "only the test and the engine hold the flag: the run was dropped");
        assert!(editor.undo_stack.is_empty(), "the pre-cleanup snapshot was consumed");
        assert_eq!(editor.redo_stack, redo_at_start, "no half-relaxed state on the redo stack");

        assert!(!editor.poll_cleanup());
        assert_eq!(editor.molecule, before, "nothing overwrites the restored molecule");
        assert!(!editor.redo(), "nothing to redo: the cleanup was cancelled, not undone");
        assert_eq!(editor.molecule, before);
    }

    /// The redo history from before Cleanup survives an undo that cancels the cleanup.
    #[test]
    fn undo_during_cleanup_keeps_the_redo_history_from_before_it() {
        let FakeEngineFixture { mut editor, before, .. } = editor_with_fake_engine(false);
        // An undo entry equal to the current molecule makes Cleanup's own snapshot a
        // duplicate, so `push_undo` skips it and leaves the redo stack alone.
        let mut later = before.clone();
        later.add_atom("O".to_string(), [9.0, 9.0], 0);
        editor.undo_stack.push_back(before.clone());
        editor.redo_stack.push_back(later.clone());

        editor.toggle_cleanup();
        assert!(editor.poll_cleanup());
        assert!(editor.undo());
        assert_eq!(editor.molecule, before);
        assert_eq!(editor.redo_stack, std::collections::VecDeque::from(vec![later.clone()]));
        assert!(editor.redo());
        assert_eq!(editor.molecule, later);
    }

    /// Redo while a cleanup runs stops it at once: the run is cancelled and dropped and a later
    /// poll writes nothing more.  With nothing to redo the stopped layout stays (as a Ctrl+K
    /// stop would leave it) and Undo still returns to the pre-cleanup molecule.
    #[test]
    fn redo_during_cleanup_stops_it() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, before, positions, cancelled, joined } =
            editor_with_fake_engine(false);

        editor.toggle_cleanup();
        assert!(editor.poll_cleanup());
        assert!(!editor.redo(), "nothing to redo");
        assert!(editor.cleanup_job.is_none());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!joined.load(Ordering::Relaxed));
        assert_eq!(std::sync::Arc::strong_count(&cancelled), 2, "only the test and the engine hold the flag: the run was dropped");
        assert_eq!(atom_positions(&editor.molecule), positions);

        assert!(!editor.poll_cleanup());
        assert!(editor.undo());
        assert_eq!(editor.molecule, before);
    }

    /// Pasting while a cleanup runs stops it, so the run can no longer move atoms.
    #[test]
    fn paste_during_cleanup_stops_it() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, cancelled, joined, .. } = editor_with_fake_engine(false);
        let mut source = crate::widget::ChemStructEditor::default();
        let id = source.molecule.add_atom("N".to_string(), [0.0, 0.0], 0);
        source.selected_atoms.insert(id);
        let clip = source.copy_to_string().expect("one selected atom copies");

        editor.toggle_cleanup();
        assert!(editor.poll_cleanup());
        assert!(editor.paste_from_string(&clip));
        assert!(editor.cleanup_job.is_none());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!joined.load(Ordering::Relaxed));
        assert_eq!(std::sync::Arc::strong_count(&cancelled), 2, "only the test and the engine hold the flag: the run was dropped");
        let after_paste = editor.molecule.clone();
        assert!(!editor.poll_cleanup());
        assert_eq!(editor.molecule, after_paste);
    }

    /// `cancel_cleanup` stops a running cleanup without writing any more positions, and
    /// reports whether there was one.
    #[test]
    fn cancel_cleanup_stops_the_run_without_writing() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, before, cancelled, joined, .. } =
            editor_with_fake_engine(false);

        assert!(!editor.cancel_cleanup(), "nothing to cancel while idle");
        editor.toggle_cleanup();
        assert!(editor.cancel_cleanup());
        assert!(editor.cleanup_job.is_none());
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!joined.load(Ordering::Relaxed));
        assert_eq!(std::sync::Arc::strong_count(&cancelled), 2, "only the test and the engine hold the flag: the run was dropped");
        assert_eq!(editor.molecule, before, "no preview was polled, so nothing moved");
        // A host replacing the molecule after cancelling keeps what it put there.
        editor.molecule = Molecule::default();
        assert!(!editor.poll_cleanup());
        assert!(editor.molecule.atoms.is_empty());
    }

    /// A run that already reports done when `cancel_cleanup` comes is joined, not just dropped,
    /// and its final positions are not written either.
    #[test]
    fn cancel_cleanup_joins_a_finished_run() {
        use std::sync::atomic::Ordering;
        let FakeEngineFixture { mut editor, before, joined, .. } = editor_with_fake_engine(true);

        editor.toggle_cleanup();
        assert!(editor.cancel_cleanup());
        assert!(editor.cleanup_job.is_none());
        assert!(joined.load(Ordering::Relaxed), "a finished run is joined");
        assert_eq!(editor.molecule, before);
    }

    /// `cancel_cleanup` discards a running built-in (native, threaded) run without writing its
    /// positions and without waiting for the worker.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cancel_cleanup_discards_a_running_native_job() {
        let mut rng = Lcg(0x1053);
        let mol = random_molecule(&mut rng, 120, 30, 40.0);
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.toggle_cleanup();
        assert!(editor.cancel_cleanup());
        assert!(editor.cleanup_job.is_none());
        assert_eq!(editor.molecule, mol);
    }

    /// A run that finishes between two calls of the editor's poll: its first `latest()` returns
    /// the preview and, as a worker publishing its last chunk and setting `done` would, makes
    /// `is_done()` true from then on; every later `latest()` returns the final positions (e.g.
    /// the restore after a rejected rescue pass, or after a cancel mid-rescue).
    struct FinishesAfterFirstLatestRun {
        preview: Vec<(u32, [f32; 2])>,
        finished: Vec<(u32, [f32; 2])>,
        done: std::sync::atomic::AtomicBool,
        joined: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl CleanupRun for FinishesAfterFirstLatestRun {
        fn cancel(&self) {}
        fn is_done(&self) -> bool {
            self.done.load(std::sync::atomic::Ordering::Relaxed)
        }
        fn latest(&self) -> Vec<(u32, [f32; 2])> {
            if self.done.swap(true, std::sync::atomic::Ordering::Relaxed) {
                self.finished.clone()
            } else {
                self.preview.clone()
            }
        }
        fn join(self: Box<Self>) {
            self.joined.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }

    struct FinishesAfterFirstLatestEngine {
        preview: Vec<(u32, [f32; 2])>,
        finished: Vec<(u32, [f32; 2])>,
        joined: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl CleanupEngine for FinishesAfterFirstLatestEngine {
        fn start(&self, _mol: &Molecule) -> CleanupJob {
            CleanupJob::from_run(FinishesAfterFirstLatestRun {
                preview: self.preview.clone(),
                finished: self.finished.clone(),
                done: Default::default(),
                joined: self.joined.clone(),
            })
        }
    }

    /// A run that becomes done right after `poll_cleanup` read its preview must still end on
    /// the final positions: the editor may only treat what `latest()` returns as final when
    /// `is_done()` was already true before that `latest()` call.
    #[test]
    fn run_finishing_after_the_preview_read_ends_on_the_final_positions() {
        use std::sync::atomic::Ordering;
        let mut editor = crate::widget::ChemStructEditor::default();
        let a = editor.molecule.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = editor.molecule.add_atom("C".to_string(), [5.0, 5.0], 0);
        let before = editor.molecule.clone();
        let preview = vec![(a, [1.25, -0.5]), (b, [2.5, 0.75])];
        let finished = vec![(a, [-3.0, 1.0]), (b, [4.0, -2.0])];
        let joined = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        editor.set_cleanup_engine(FinishesAfterFirstLatestEngine {
            preview: preview.clone(),
            finished: finished.clone(),
            joined: joined.clone(),
        });

        editor.toggle_cleanup();
        assert!(editor.poll_cleanup(), "the run was not done before the preview was read");
        assert_eq!(atom_positions(&editor.molecule), preview);
        assert!(editor.cleanup_job.is_some());
        assert!(!joined.load(Ordering::Relaxed));

        assert!(!editor.poll_cleanup(), "a finished run ends the cleanup");
        assert_eq!(atom_positions(&editor.molecule), finished);
        assert!(editor.cleanup_job.is_none());
        assert!(joined.load(Ordering::Relaxed));
        assert_eq!(editor.undo_stack.len(), 1);
        assert!(editor.undo());
        assert_eq!(editor.molecule, before);
    }

    /// Wrapping the run in a trait object must not cost `CleanupJob` (and so the editor that
    /// holds one) the `Send + Sync` it had as a concrete threaded job.
    #[test]
    fn cleanup_job_and_editor_stay_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CleanupJob>();
        assert_send_sync::<crate::widget::ChemStructEditor>();
    }

    // ─── Cleanup of a selection ──────────────────────────────────────────────

    /// A well-drawn zigzag chain of `n` carbons (bonds of length `L`, 120° angles) starting at
    /// `origin`, returning the atom ids in chain order.
    fn zigzag_chain(mol: &mut Molecule, n: usize, origin: [f32; 2]) -> Vec<u32> {
        let ids: Vec<u32> = (0..n)
            .map(|i| {
                let p = [origin[0] + i as f32 * L * 0.75_f32.sqrt(), origin[1] + (i % 2) as f32 * L * 0.5];
                mol.add_atom("C".to_string(), p, 0)
            })
            .collect();
        for w in ids.windows(2) {
            mol.add_bond(w[0], w[1], BondOrder::Single);
        }
        ids
    }

    fn bond_length(mol: &Molecule, a: u32, b: u32) -> f32 {
        let pa = mol.atom_by_id(a).unwrap().pos;
        let pb = mol.atom_by_id(b).unwrap().pos;
        ((pb[0] - pa[0]).powi(2) + (pb[1] - pa[1]).powi(2)).sqrt()
    }

    fn centroid(mol: &Molecule, ids: &[u32]) -> [f32; 2] {
        let n = ids.len() as f32;
        let (sx, sy) = ids.iter().fold((0.0, 0.0), |(x, y), &id| {
            let p = mol.atom_by_id(id).unwrap().pos;
            (x + p[0], y + p[1])
        });
        [sx / n, sy / n]
    }

    /// Run the editor's Cleanup to the end the way the UI does: toggle, then poll every frame.
    #[cfg(not(target_arch = "wasm32"))]
    fn run_editor_cleanup(editor: &mut crate::widget::ChemStructEditor) {
        editor.toggle_cleanup();
        while editor.poll_cleanup() {
            std::thread::yield_now();
        }
    }

    /// The bit patterns of the atoms in `mol` that are not in `selected`.
    fn unselected_bits(mol: &Molecule, selected: &HashSet<u32>) -> Vec<(u32, [u32; 2])> {
        coord_bits(mol).into_iter().filter(|(id, _)| !selected.contains(id)).collect()
    }

    /// A six-carbon chain whose last two atoms were dragged out of shape: one bond stretched to
    /// 4 units, the next squashed to 0.4.  Returns the molecule, the chain and the two atoms.
    fn chain_with_distorted_end() -> (Molecule, Vec<u32>, HashSet<u32>) {
        let mut mol = Molecule::default();
        let chain = zigzag_chain(&mut mol, 6, [0.0, 0.0]);
        mol.atom_by_id_mut(chain[4]).unwrap().pos = [8.5, -2.0];
        mol.atom_by_id_mut(chain[5]).unwrap().pos = [8.7, -1.7];
        let selected = [chain[4], chain[5]].into_iter().collect();
        (mol, chain, selected)
    }

    /// Cleanup with part of the structure selected moves only the selected atoms: every other
    /// atom keeps its exact coordinates.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_leaves_unselected_atoms_untouched() {
        let (mol, chain, selected) = chain_with_distorted_end();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.selected_atoms = selected.clone();

        run_editor_cleanup(&mut editor);

        assert_eq!(unselected_bits(&editor.molecule, &selected), unselected_bits(&mol, &selected));
        for &id in &selected {
            assert_ne!(editor.molecule.atom_by_id(id).unwrap().pos, mol.atom_by_id(id).unwrap().pos, "atom {id} was cleaned");
        }
        assert_eq!(editor.molecule.bonds, mol.bonds);
        assert!(chain.iter().all(|&id| editor.molecule.atom_by_id(id).is_some()));
    }

    /// The selected atoms are laid out against the unselected ones held still: the bond
    /// crossing the selection boundary, and the bonds inside the selection, come out at the
    /// standard length, and the angle at the fixed boundary atom at 120°.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_puts_boundary_bonds_at_the_standard_length() {
        let (mol, chain, selected) = chain_with_distorted_end();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol;
        editor.selected_atoms = selected;

        run_editor_cleanup(&mut editor);

        let m = &editor.molecule;
        for (a, b) in [(chain[3], chain[4]), (chain[4], chain[5])] {
            let d = bond_length(m, a, b);
            assert!((d - L).abs() < 0.05, "bond {a}-{b} is {d}, want {L}");
        }
        let angle = angle_deg(m, chain[2], chain[3], chain[4]);
        assert!((angle - 120.0).abs() < 3.0, "angle at the boundary atom is {angle}°");
        let angle = angle_deg(m, chain[3], chain[4], chain[5]);
        assert!((angle - 120.0).abs() < 3.0, "angle inside the selection is {angle}°");
    }

    /// A ring atom pulled out of place goes back between its two fixed ring neighbours.
    #[test]
    fn selection_cleanup_puts_a_ring_atom_back_between_fixed_neighbours() {
        let mut mol = fixture("toluene_distorted");
        cleanup_2d(&mut mol);
        let ring: Vec<u32> = mol.atoms.iter().take(6).map(|a| a.id).collect();
        mol.atom_by_id_mut(ring[2]).unwrap().pos[0] += 2.5;
        let before = mol.clone();
        let selected: HashSet<u32> = [ring[2]].into_iter().collect();

        cleanup_2d_partial(&mut mol, &selected);

        assert_eq!(unselected_bits(&mol, &selected), unselected_bits(&before, &selected));
        for n in [ring[1], ring[3]] {
            let d = bond_length(&mol, ring[2], n);
            assert!((d - L).abs() < 0.05, "ring bond to {n} is {d}");
        }
    }

    /// A selected fragment bonded to nothing unselected is cleaned up as a whole and put back
    /// where it was: its centroid does not move, and the rest of the page does not move at all.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_of_a_separate_fragment_keeps_its_centroid() {
        let mut mol = Molecule::default();
        zigzag_chain(&mut mol, 4, [0.0, 0.0]);
        let fragment: Vec<u32> = [[20.0, 10.0], [23.5, 10.4], [22.8, 13.0], [26.0, 9.0]]
            .iter()
            .map(|&p| mol.add_atom("C".to_string(), p, 0))
            .collect();
        for w in fragment.windows(2) {
            mol.add_bond(w[0], w[1], BondOrder::Single);
        }
        let selected: HashSet<u32> = fragment.iter().copied().collect();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.selected_atoms = selected.clone();

        run_editor_cleanup(&mut editor);

        let m = &editor.molecule;
        assert_eq!(unselected_bits(m, &selected), unselected_bits(&mol, &selected));
        let (was, now) = (centroid(&mol, &fragment), centroid(m, &fragment));
        assert!((was[0] - now[0]).abs() < 1e-3 && (was[1] - now[1]).abs() < 1e-3, "centroid moved from {was:?} to {now:?}");
        for w in fragment.windows(2) {
            let d = bond_length(m, w[0], w[1]);
            assert!((d - L).abs() < 0.05, "bond {}-{} is {d}", w[0], w[1]);
        }

        // Its shape is the one a plain cleanup of the fragment alone gives, moved back.
        let mut alone = Molecule::default();
        for &id in &fragment {
            alone.add_atom("C".to_string(), mol.atom_by_id(id).unwrap().pos, 0);
        }
        let alone_ids: Vec<u32> = alone.atoms.iter().map(|a| a.id).collect();
        for w in alone_ids.windows(2) {
            alone.add_bond(w[0], w[1], BondOrder::Single);
        }
        cleanup_2d(&mut alone);
        for (&id, &aid) in fragment.iter().zip(&alone_ids) {
            let p = m.atom_by_id(id).unwrap().pos;
            let q = alone.atom_by_id(aid).unwrap().pos;
            let (dx, dy) = (p[0] - was[0] - q[0], p[1] - was[1] - q[1]);
            assert!(dx.hypot(dy) < 0.02, "atom {id} at {p:?}, plain cleanup puts it at {q:?} + {was:?}");
        }
    }

    /// One Undo goes back to the drawing from before the selection cleanup.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_is_one_undo_step() {
        let (mol, _, selected) = chain_with_distorted_end();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.selected_atoms = selected;

        run_editor_cleanup(&mut editor);
        assert_ne!(editor.molecule, mol);
        assert_eq!(editor.undo_stack.len(), 1);

        assert!(editor.undo());
        assert_eq!(editor.molecule, mol);
    }

    /// With one bond selected and no atom, Cleanup moves just that bond's two atoms.  (The
    /// Select-tool click that selects the bond is tested in `widget::interact`.)
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_of_one_bond_moves_its_two_atoms() {
        let mut mol = Molecule::default();
        let chain = zigzag_chain(&mut mol, 5, [0.0, 0.0]);
        mol.atom_by_id_mut(chain[2]).unwrap().pos = [2.2, -0.9];
        mol.atom_by_id_mut(chain[3]).unwrap().pos = [4.6, 1.6];
        let selected: HashSet<u32> = [chain[2], chain[3]].into_iter().collect();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.selected_bonds = [mol.bond_between(chain[2], chain[3]).unwrap().id].into_iter().collect();

        run_editor_cleanup(&mut editor);

        let m = &editor.molecule;
        assert_eq!(unselected_bits(m, &selected), unselected_bits(&mol, &selected));
        for w in chain.windows(2) {
            let d = bond_length(m, w[0], w[1]);
            assert!((d - L).abs() < 0.05, "bond {}-{} is {d}", w[0], w[1]);
        }
    }

    /// With nothing selected, or everything, Cleanup is the plain whole-page cleanup.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn cleanup_with_no_or_full_selection_is_the_plain_cleanup() {
        for fx in characterization_fixtures() {
            let mut expected = fx.mol.clone();
            cleanup_2d(&mut expected);
            let all: HashSet<u32> = fx.mol.atoms.iter().map(|a| a.id).collect();
            for selected in [HashSet::new(), all] {
                let mut editor = crate::widget::ChemStructEditor::default();
                editor.molecule = fx.mol.clone();
                editor.selected_atoms = selected;
                run_editor_cleanup(&mut editor);
                assert_eq!(coord_bits(&editor.molecule), coord_bits(&expected), "{}", fx.name);
            }
        }
    }

    /// The background job of a selection cleanup lands on the coordinates of the one-shot
    /// `cleanup_2d_partial`, and reports the selected atoms only.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn partial_job_matches_cleanup_2d_partial() {
        let (mol, _, selected) = chain_with_distorted_end();
        let mut expected = mol.clone();
        cleanup_2d_partial(&mut expected, &selected);
        let from_job = position_bits(&run_polled(DefaultCleanupEngine.start_partial(&mol, &selected)));
        let expected_selected: Vec<_> =
            coord_bits(&expected).into_iter().filter(|(id, _)| selected.contains(id)).collect();
        assert_eq!(from_job, expected_selected);
    }

    /// While a selection cleanup runs, the editor writes back only the atoms it moves: an
    /// unselected atom dragged meanwhile stays where it was dragged instead of snapping back to
    /// where it was when Cleanup started.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn partial_job_leaves_an_unselected_atom_moved_during_the_run() {
        let (mol, chain, selected) = chain_with_distorted_end();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol;
        editor.selected_atoms = selected;

        editor.toggle_cleanup();
        let dragged = [-3.0, 2.0];
        editor.molecule.atom_by_id_mut(chain[0]).unwrap().pos = dragged;
        while editor.poll_cleanup() {
            std::thread::yield_now();
        }
        assert_eq!(editor.molecule.atom_by_id(chain[0]).unwrap().pos, dragged);
    }

    /// A selected atom drawn exactly on an unselected neighbour (every force between the two
    /// has no direction there) is moved off it and laid out at bond length, the other atoms
    /// untouched — both when that neighbour has other bonds to steer by and when it has none.
    #[test]
    fn selection_cleanup_separates_an_atom_drawn_on_its_fixed_neighbour() {
        for n in [4, 2] {
            let mut mol = Molecule::default();
            let chain = zigzag_chain(&mut mol, n, [0.0, 0.0]);
            let (on, under) = (chain[n - 1], chain[n - 2]);
            mol.atom_by_id_mut(on).unwrap().pos = mol.atom_by_id(under).unwrap().pos;
            let selected: HashSet<u32> = [on].into_iter().collect();

            let mut m = mol.clone();
            cleanup_2d_partial(&mut m, &selected);

            assert_eq!(unselected_bits(&m, &selected), unselected_bits(&mol, &selected), "{n} atoms");
            let d = bond_length(&m, under, on);
            assert!((d - L).abs() < 0.05, "{n} atoms: the bond is {d}");
            if n == 4 {
                // Steered away from the fixed atom's other neighbour: the chain's 120° angle.
                let p = |id| m.atom_by_id(id).unwrap().pos;
                let (a, b, c) = (p(chain[1]), p(under), p(on));
                let ang = ((a[0] - b[0]) * (c[0] - b[0]) + (a[1] - b[1]) * (c[1] - b[1]))
                    / (bond_length(&m, chain[1], under) * d);
                assert!((ang.acos().to_degrees() - 120.0).abs() < 2.0, "angle {}", ang.acos().to_degrees());
            }
        }
    }

    /// A lone atom selected on its own has nothing to lay it out against: it stays put, and so
    /// does everything else.
    #[test]
    fn selection_cleanup_of_a_single_free_atom_moves_nothing() {
        let mut mol = Molecule::default();
        zigzag_chain(&mut mol, 4, [0.0, 0.0]);
        let lone = mol.add_atom("O".to_string(), [3.0, 4.0], 0);
        let selected: HashSet<u32> = [lone].into_iter().collect();
        let mut m = mol.clone();
        cleanup_2d_partial(&mut m, &selected);
        assert_eq!(unselected_bits(&m, &selected), unselected_bits(&mol, &selected));
        let p = m.atom_by_id(lone).unwrap().pos;
        assert!((p[0] - 3.0).abs() < 1e-5 && (p[1] - 4.0).abs() < 1e-5, "moved to {p:?}");
    }

    /// Undo while a selection cleanup runs drops the run and restores the drawing from before
    /// Cleanup; stopping it with Cleanup again keeps the unselected atoms exactly where they
    /// were.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_can_be_undone_or_stopped_mid_run() {
        let (mol, _, selected) = chain_with_distorted_end();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.selected_atoms = selected.clone();
        editor.toggle_cleanup();
        editor.poll_cleanup();
        assert!(editor.undo());
        assert!(!editor.poll_cleanup(), "undo ends the run");
        assert_eq!(editor.molecule, mol);

        editor.selected_atoms = selected.clone();
        editor.toggle_cleanup();
        editor.poll_cleanup();
        editor.toggle_cleanup();
        while editor.poll_cleanup() {
            std::thread::yield_now();
        }
        assert_eq!(unselected_bits(&editor.molecule, &selected), unselected_bits(&mol, &selected));
    }

    /// An engine that records which of its entry points the editor called, and with what.
    struct RecordingEngine {
        calls: std::sync::Arc<std::sync::Mutex<Vec<Option<HashSet<u32>>>>>,
    }

    impl CleanupEngine for RecordingEngine {
        fn start(&self, _mol: &Molecule) -> CleanupJob {
            self.calls.lock().unwrap().push(None);
            CleanupJob::from_run(FakeRun { positions: vec![], done: true, cancelled: Default::default(), joined: Default::default() })
        }
        fn start_partial(&self, _mol: &Molecule, movable: &HashSet<u32>) -> CleanupJob {
            self.calls.lock().unwrap().push(Some(movable.clone()));
            CleanupJob::from_run(FakeRun { positions: vec![], done: true, cancelled: Default::default(), joined: Default::default() })
        }
    }

    /// The editor hands the engine the selected atoms (only those still on the page) and asks
    /// for a whole-page run when nothing, or everything, is selected.
    #[test]
    fn editor_passes_the_selection_to_the_engine() {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut editor = crate::widget::ChemStructEditor::default();
        let chain = zigzag_chain(&mut editor.molecule, 4, [0.0, 0.0]);
        editor.set_cleanup_engine(RecordingEngine { calls: calls.clone() });

        for selection in [vec![], vec![chain[1], chain[2], 999], chain.clone()] {
            editor.selected_atoms = selection.into_iter().collect();
            editor.toggle_cleanup();
            assert!(!editor.poll_cleanup());
        }
        let partial: HashSet<u32> = [chain[1], chain[2]].into_iter().collect();
        assert_eq!(*calls.lock().unwrap(), vec![None, Some(partial), None]);
    }

    /// On a page holding a reaction, a selection cleanup moves only the selected atoms of one
    /// structure: the arrow and the other structure stay exactly where they are.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn selection_cleanup_on_a_reaction_page_moves_only_the_selection() {
        let mut mol = Molecule::default();
        let reactant = zigzag_chain(&mut mol, 4, [0.0, 0.0]);
        mol.add_arrow([6.0, 0.5], [10.0, 0.5], crate::molecule::ArrowKind::Reaction);
        zigzag_chain(&mut mol, 4, [12.0, 0.0]);
        mol.atom_by_id_mut(reactant[3]).unwrap().pos = [2.0, 3.0];
        let selected: HashSet<u32> = [reactant[3]].into_iter().collect();
        let mut editor = crate::widget::ChemStructEditor::default();
        editor.molecule = mol.clone();
        editor.selected_atoms = selected.clone();

        run_editor_cleanup(&mut editor);

        let m = &editor.molecule;
        assert_eq!(unselected_bits(m, &selected), unselected_bits(&mol, &selected));
        assert_eq!(m.objects, mol.objects);
        let d = bond_length(m, reactant[2], reactant[3]);
        assert!((d - L).abs() < 0.05, "boundary bond is {d}");
    }

    /// `CleanupSchedule` advanced in small chunks (as a worker or a per-frame poll would) lands
    /// on exactly the coordinates of the one-shot `cleanup_2d`.
    #[test]
    fn chunked_schedule_matches_cleanup_2d() {
        for fx in characterization_fixtures() {
            let mut mol = fx.mol.clone();
            cleanup_2d(&mut mol);
            for chunk in [JOB_CHUNK, 777] {
                let mut schedule = CleanupSchedule::new(&fx.mol);
                while !schedule.advance(chunk) {
                    assert!(!schedule.is_done());
                }
                assert!(schedule.is_done());
                assert_eq!(
                    position_bits(&schedule.positions()),
                    coord_bits(&mol),
                    "{}: schedule in chunks of {chunk} differs from cleanup_2d",
                    fx.name
                );
            }
        }
    }

    fn fixture(name: &str) -> Molecule {
        characterization_fixtures().into_iter().find(|f| f.name == name).unwrap().mol
    }

    /// Cancelled during phase 1, the schedule stops where the atoms are.
    #[test]
    fn cancel_mid_relaxation_keeps_current_positions() {
        let mut schedule = CleanupSchedule::new(&fixture("toluene_distorted"));
        assert!(!schedule.advance(10));
        let before = position_bits(&schedule.positions());
        schedule.cancel();
        assert!(schedule.is_done());
        assert!(schedule.advance(10));
        assert_eq!(position_bits(&schedule.positions()), before);
    }

    /// A finished schedule answers `advance` with `true` whatever the step count, including 0
    /// (the positive-step requirement only guards an unfinished schedule against stalling).
    #[test]
    fn advance_on_a_finished_schedule_is_a_no_op() {
        let mut schedule = CleanupSchedule::new(&fixture("toluene_distorted"));
        schedule.cancel();
        let before = position_bits(&schedule.positions());
        assert!(schedule.advance(0));
        assert!(schedule.advance(JOB_CHUNK));
        assert_eq!(position_bits(&schedule.positions()), before);
    }

    /// Cancelled during a rescue pass, the schedule rolls back to the phase-1 result — the same
    /// positions a finished schedule ends on when it rejects that pass.
    #[test]
    fn cancel_mid_rescue_restores_the_phase1_result() {
        let mol = fixture("random_trial_05");
        let mut st = CleanupState::new(&mol);
        st.step(JOB_PHASE_BUDGET);
        let after_phase1 = position_bits(&st.positions());

        let mut schedule = CleanupSchedule::new(&mol);
        while schedule.stage == Stage::Relax {
            schedule.advance(JOB_CHUNK);
        }
        assert!(schedule.stage == Stage::Rescue);
        schedule.advance(1);
        assert!(schedule.stage == Stage::Rescue);
        assert_ne!(position_bits(&schedule.positions()), after_phase1);
        schedule.cancel();
        assert!(schedule.is_done());
        assert_eq!(position_bits(&schedule.positions()), after_phase1);
    }

    /// A pass that runs out of its budget ends there, and a chunk that would cross the end of
    /// the pass is cut short: 3 + 3 + 3 requested against a budget of 7 runs exactly 7 steps.
    #[test]
    fn pass_ends_at_its_budget_and_caps_the_last_chunk() {
        let mol = fixture("toluene_distorted");
        let mut st = CleanupState::new(&mol);
        assert!(!st.step(7), "the fixture must not converge within the test budget");
        assert_eq!(st.ring_crossing_count(), 0, "no rescue may follow the pass");

        let mut schedule = CleanupSchedule::with_pass_budget(&mol, 7);
        assert!(!schedule.advance(3));
        assert!(!schedule.advance(3));
        assert!(schedule.advance(3), "the pass ran out of budget, so the schedule is done");
        assert_eq!(position_bits(&schedule.positions()), position_bits(&st.positions()));
    }

    /// With every pass (phase 1 and the rescue passes) cut off by a small budget, the result
    /// still does not depend on the chunk size.
    #[test]
    fn budget_limited_rescue_passes_do_not_depend_on_chunk_size() {
        let mol = fixture("random_trial_10");
        let run = |chunk: usize| {
            let mut schedule = CleanupSchedule::with_pass_budget(&mol, 50);
            while !schedule.advance(chunk) {}
            assert!(schedule.rescue_passes > 0, "the fixture must reach the rescue");
            position_bits(&schedule.positions())
        };
        let whole = run(50);
        for chunk in [1, 7, 1000] {
            assert_eq!(run(chunk), whole, "chunks of {chunk} differ from whole passes");
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
