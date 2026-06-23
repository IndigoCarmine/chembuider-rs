use std::collections::{HashMap, HashSet, VecDeque};
use super::Molecule;

const L: f32 = 1.5;

pub fn cleanup_2d(mol: &mut Molecule) {
    if mol.atoms.is_empty() {
        return;
    }

    let rings = find_rings(mol);
    let mut placed: HashMap<u32, [f32; 2]> = HashMap::new();
    let mut queue: VecDeque<u32> = VecDeque::new();

    if let Some(first_ring) = rings.first() {
        // Place the first ring centered at origin
        place_ring_polygon(first_ring, [0.0, 0.0], 0.0, &mut placed);
        for &id in first_ring {
            queue.push_back(id);
        }
    } else {
        // No rings: start from a terminal atom (degree 1) or atom[0]
        let start = terminal_atom(mol).unwrap_or(mol.atoms[0].id);
        placed.insert(start, [0.0, 0.0]);
        queue.push_back(start);
    }

    let mut in_queue: HashSet<u32> = queue.iter().cloned().collect();

    while let Some(atom_id) = queue.pop_front() {
        for neighbor in mol.neighbor_atom_ids(atom_id) {
            if placed.contains_key(&neighbor) {
                continue;
            }

            if let Some(ring) = ring_containing(neighbor, &rings) {
                // Only place the ring if most of it is unplaced
                let unplaced_count = ring.iter().filter(|&&id| !placed.contains_key(&id)).count();
                if unplaced_count == 0 {
                    continue;
                }
                // The bond from atom_id → neighbor defines the entry direction
                let entry_pos = placed[&atom_id];
                let entry_angle = best_chain_angle(atom_id, neighbor, &placed, mol);
                // Rotate ring so that the entry neighbor is first and entry_pos is its position
                let rotated = rotate_ring_to_entry(&ring, neighbor);
                place_ring_from_entry(&rotated, entry_pos, entry_angle, &mut placed);
                for &id in &rotated {
                    if !in_queue.contains(&id) {
                        queue.push_back(id);
                        in_queue.insert(id);
                    }
                }
            } else {
                let entry_pos = placed[&atom_id];
                let angle = best_chain_angle(atom_id, neighbor, &placed, mol);
                let pos = [
                    entry_pos[0] + angle.cos() * L,
                    entry_pos[1] + angle.sin() * L,
                ];
                placed.insert(neighbor, pos);
                if !in_queue.contains(&neighbor) {
                    queue.push_back(neighbor);
                    in_queue.insert(neighbor);
                }
            }
        }
    }

    // Apply positions (atoms not reached keep their current position)
    for (id, pos) in placed {
        if let Some(atom) = mol.atom_by_id_mut(id) {
            atom.pos = pos;
        }
    }
}

/// Place an n-membered ring at a standard polygon geometry.
/// `entry_pos` = position of ring[0], `entry_angle` = direction ring[0]→ring[1].
fn place_ring_from_entry(ring: &[u32], entry_pos: [f32; 2], entry_angle: f32, placed: &mut HashMap<u32, [f32; 2]>) {
    use std::f32::consts::{PI, TAU};
    let n = ring.len();
    let r = L / (2.0 * (PI / n as f32).sin());

    // Center is perpendicular to entry bond, offset inward
    // ring[0] at entry_pos, ring[1] at entry_pos + L*(cos θ, sin θ)
    // center: midpoint of ring[0]-ring[1] + perpendicular offset of magnitude sqrt(r²-L²/4)
    let perp_len = (r * r - L * L * 0.25).max(0.0).sqrt();
    // Choose the "below" side: rotate entry_angle by -90°
    let perp_angle = entry_angle - PI * 0.5;
    let mid = [
        entry_pos[0] + (entry_angle.cos() * L * 0.5),
        entry_pos[1] + (entry_angle.sin() * L * 0.5),
    ];
    let cx = mid[0] + perp_angle.cos() * perp_len;
    let cy = mid[1] + perp_angle.sin() * perp_len;

    // Angle from center to ring[0]
    let alpha_0 = (entry_pos[1] - cy).atan2(entry_pos[0] - cx);

    for (k, &id) in ring.iter().enumerate() {
        if placed.contains_key(&id) {
            continue;
        }
        let angle = alpha_0 - k as f32 * TAU / n as f32;
        placed.insert(id, [cx + r * angle.cos(), cy + r * angle.sin()]);
    }
    // Ensure atom[0] is placed (may already be there)
    placed.entry(ring[0]).or_insert(entry_pos);
}

/// Place the first ring centered at `center_pos` with ring[0] pointing at angle `start_angle`.
fn place_ring_polygon(ring: &[u32], center_pos: [f32; 2], start_angle: f32, placed: &mut HashMap<u32, [f32; 2]>) {
    use std::f32::consts::{PI, TAU};
    let n = ring.len();
    let r = L / (2.0 * (PI / n as f32).sin());
    for (k, &id) in ring.iter().enumerate() {
        let angle = start_angle + k as f32 * TAU / n as f32;
        placed.insert(id, [center_pos[0] + r * angle.cos(), center_pos[1] + r * angle.sin()]);
    }
}

/// Rotate the ring slice so that `entry_atom` is first.
fn rotate_ring_to_entry(ring: &[u32], entry_atom: u32) -> Vec<u32> {
    if let Some(idx) = ring.iter().position(|&id| id == entry_atom) {
        let mut v = ring[idx..].to_vec();
        v.extend_from_slice(&ring[..idx]);
        v
    } else {
        ring.to_vec()
    }
}

/// Compute the best angle to place `target` from `source`, considering already-placed neighbors.
fn best_chain_angle(source: u32, target: u32, placed: &HashMap<u32, [f32; 2]>, mol: &Molecule) -> f32 {
    let _ = target;
    let src_pos = placed[&source];

    let occupied_angles: Vec<f32> = mol
        .neighbor_atom_ids(source)
        .into_iter()
        .filter_map(|nid| {
            let pos = placed.get(&nid)?;
            let dx = pos[0] - src_pos[0];
            let dy = pos[1] - src_pos[1];
            Some(dy.atan2(dx))
        })
        .collect();

    if occupied_angles.is_empty() {
        return std::f32::consts::PI / 3.0;
    }

    if occupied_angles.len() == 1 {
        let a = occupied_angles[0];
        let opt1 = a + std::f32::consts::PI * 2.0 / 3.0;
        let opt2 = a - std::f32::consts::PI * 2.0 / 3.0;
        return if opt1.sin() <= opt2.sin() {
            normalize(opt1)
        } else {
            normalize(opt2)
        };
    }

    let mut angles = occupied_angles;
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut max_gap = 0.0_f32;
    let mut best = 0.0_f32;
    let n = angles.len();
    for i in 0..n {
        let cur = angles[i];
        let next = if i + 1 < n { angles[i + 1] } else { angles[0] + std::f32::consts::TAU };
        let gap = next - cur;
        if gap > max_gap {
            max_gap = gap;
            best = cur + gap * 0.5;
        }
    }
    normalize(best)
}

fn normalize(a: f32) -> f32 {
    let mut a = a;
    while a > std::f32::consts::PI { a -= std::f32::consts::TAU; }
    while a < -std::f32::consts::PI { a += std::f32::consts::TAU; }
    a
}

/// Find all atoms with degree 1 (terminal).
fn terminal_atom(mol: &Molecule) -> Option<u32> {
    mol.atoms.iter().find(|a| mol.neighbor_atom_ids(a.id).len() <= 1).map(|a| a.id)
}

/// Find which ring (if any) contains `atom_id`.
fn ring_containing(atom_id: u32, rings: &[Vec<u32>]) -> Option<Vec<u32>> {
    rings.iter().find(|r| r.contains(&atom_id)).cloned()
}

/// DFS-based ring detection. Returns minimum cycles (one per back-edge).
pub fn find_rings(mol: &Molecule) -> Vec<Vec<u32>> {
    let mut visited: HashMap<u32, u32> = HashMap::new(); // id → parent id (u32::MAX = root)
    let mut depth: HashMap<u32, usize> = HashMap::new();
    let mut rings: Vec<Vec<u32>> = Vec::new();
    let mut stack: Vec<(u32, u32, usize)> = Vec::new(); // (node, parent, depth)

    for seed in mol.atoms.iter().map(|a| a.id) {
        if visited.contains_key(&seed) {
            continue;
        }
        stack.push((seed, u32::MAX, 0));

        while let Some((node, parent, d)) = stack.pop() {
            if visited.contains_key(&node) {
                continue;
            }
            visited.insert(node, parent);
            depth.insert(node, d);

            for neighbor in mol.neighbor_atom_ids(node) {
                if neighbor == parent {
                    continue;
                }
                if visited.contains_key(&neighbor) {
                    // Back edge: extract cycle
                    let ring = extract_cycle(node, neighbor, &visited, &depth);
                    if ring.len() >= 3 {
                        // Deduplicate by canonical form
                        let canonical = canonical_ring(&ring);
                        if !rings.iter().any(|r| canonical_ring(r) == canonical) {
                            rings.push(ring);
                        }
                    }
                } else {
                    stack.push((neighbor, node, d + 1));
                }
            }
        }
    }

    rings
}

/// Trace from `low` back through DFS tree parents until `high` is reached.
fn extract_cycle(low: u32, high: u32, parent: &HashMap<u32, u32>, depth: &HashMap<u32, usize>) -> Vec<u32> {
    let d_low = *depth.get(&low).unwrap_or(&0);
    let d_high = *depth.get(&high).unwrap_or(&0);

    // Walk the deeper node up to the same depth, then walk both up together
    let mut a = low;
    let mut b = high;
    let mut path_a = vec![a];
    let mut path_b = vec![b];

    let mut da = d_low;
    let mut db = d_high;

    while da > db {
        a = *parent.get(&a).unwrap_or(&a);
        path_a.push(a);
        da -= 1;
    }
    while db > da {
        b = *parent.get(&b).unwrap_or(&b);
        path_b.push(b);
        db -= 1;
    }
    while a != b {
        a = *parent.get(&a).unwrap_or(&a);
        b = *parent.get(&b).unwrap_or(&b);
        path_a.push(a);
        path_b.push(b);
    }
    // path_a and path_b meet at LCA = a = b; build the ring
    path_b.pop(); // remove LCA from path_b (already in path_a)
    path_b.reverse();
    path_a.extend(path_b);
    path_a
}

fn canonical_ring(ring: &[u32]) -> Vec<u32> {
    let mut min_start = 0;
    for i in 1..ring.len() {
        if ring[i] < ring[min_start] {
            min_start = i;
        }
    }
    let mut v: Vec<u32> = ring[min_start..].iter().chain(ring[..min_start].iter()).cloned().collect();
    // also try reversed and pick lexicographically smaller
    let mut rev = v.clone();
    rev.reverse();
    if rev < v { v = rev; }
    v
}
