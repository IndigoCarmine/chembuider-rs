//! Whether a ring can be fused onto a bond at a given place.
//!
//! Regular hexagons tile the plane, so on a polycyclic system a ring drawn on one bond routinely
//! lands on more of what is already there: the next bond round, an atom of a neighbouring ring.
//! That is fusion, and it is how pyrene and coronene grow. The same is true of the side a ring
//! already occupies, though, where every vertex lands on an atom and there is nothing to draw.
//! Telling these apart takes more than how close the new atoms come to the old ones, which is
//! zero both ways; it takes asking what the placement would actually add, and whether that fits.

use super::{Molecule, charged_max_valence, fragment};

/// How close a new atom or bond may come to what is drawn, as a fraction of the bond length.
///
/// Out in the open this only refuses a real overlap, so a ring beside a strained four-ring stays
/// drawable (its vertex comes within about half a bond of the neighbour).
const CLEARANCE: f32 = 0.45;

/// The same, inside a ring. The hexagon on a bond of an eight-ring fits inside it with no atom
/// on another, yet is no structure; on a hexagonal lattice, where a hole a ring belongs in has
/// everything a full bond length away, this is still met.
const CLEARANCE_INSIDE_A_RING: f32 = 0.75;

/// Distance from `p` to the segment `a`–`b`.
fn distance_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

/// z of (`b` − `a`) × (`c` − `a`): positive when `c` is to the left of `a`→`b`.
fn cross(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether the open segments `p`–`q` and `r`–`s` cross.
fn segments_cross(p: [f32; 2], q: [f32; 2], r: [f32; 2], s: [f32; 2]) -> bool {
    cross(p, q, r) * cross(p, q, s) < 0.0 && cross(r, s, p) * cross(r, s, q) < 0.0
}

/// Whether `p` lies inside the polygon.
fn inside_polygon(p: [f32; 2], polygon: &[[f32; 2]]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for (i, &a) in polygon.iter().enumerate() {
        let b = polygon[j];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl Molecule {
    /// How many bonds atom `id` has past what it can hold ([`charged_max_valence`]), negative with
    /// room to spare; `None` for an element with no standard valence. Group hydrogens are not
    /// counted: they make way for a bond ([`consume_excess_group_h`](Self::consume_excess_group_h)).
    pub(crate) fn valence_excess(&self, id: u32) -> Option<i16> {
        let atom = self.atom_by_id(id)?;
        let limit = charged_max_valence(&atom.element, atom.charge)?;
        let sum: i16 = self
            .bonds_for_atom(id)
            .iter()
            .filter(|b| {
                let other = if b.begin == id { b.end } else { b.begin };
                !self.atom_by_id(other).is_some_and(|a| a.group_h)
            })
            .map(|b| b.order.valence())
            .sum();
        Some(sum - limit)
    }

    /// The atom a fragment's vertex at `pos` merges with, if any.
    ///
    /// A group hydrogen ([`Atom::group_h`](super::Atom::group_h)) is a label's hydrogen with
    /// no vertex of its own to speak of, so a ring takes no notice of where it happens to sit.
    pub(crate) fn fusion_snap(&self, pos: [f32; 2]) -> Option<u32> {
        self.atoms
            .iter()
            .filter(|a| !a.group_h)
            .map(|a| (a.id, (a.pos[0] - pos[0]).hypot(a.pos[1] - pos[1])))
            .filter(|&(_, d)| d < Self::SNAP_RADIUS)
            .min_by(|x, y| x.1.total_cmp(&y.1))
            .map(|(id, _)| id)
    }

    /// Whether `frag`, placed at `positions` with the bond `shared` as its edge from `edge.0` to
    /// `edge.1`, can be drawn: it adds something, and what it adds overlaps nothing.
    ///
    /// Vertices within [`SNAP_RADIUS`](Self::SNAP_RADIUS) of an atom merge with it and bonds
    /// already there are reused, so those are never in the way. Everything else has to keep clear
    /// of what is drawn. A placement that adds nothing is the side of the bond a ring is already
    /// on. Valence is a separate question, since it depends on the bond orders that result.
    pub(crate) fn fusion_fits(
        &self,
        frag: &fragment::Fragment,
        positions: &[[f32; 2]],
        edge: (usize, usize),
        shared: (u32, u32),
    ) -> bool {
        let mut ids: Vec<Option<u32>> = positions.iter().map(|&p| self.fusion_snap(p)).collect();
        ids[edge.0] = Some(shared.0);
        ids[edge.1] = Some(shared.1);

        for (i, id) in ids.iter().enumerate() {
            let Some(id) = *id else { continue };
            // Two vertices on one atom would collapse the ring, and a drawn hydrogen is a cap.
            if ids[..i].contains(&Some(id)) || self.atom_by_id(id).is_some_and(|a| a.element == "H")
            {
                return false;
            }
        }

        let new_atoms: Vec<usize> = (0..ids.len()).filter(|&i| ids[i].is_none()).collect();
        let new_bonds: Vec<(usize, usize)> = frag
            .bonds
            .iter()
            .map(|b| (b.begin, b.end))
            .filter(|&(u, v)| match (ids[u], ids[v]) {
                (Some(a), Some(b)) => self.bond_between(a, b).is_none(),
                _ => true,
            })
            .collect();
        if new_atoms.is_empty() && new_bonds.is_empty() {
            return false;
        }

        let cycle = fragment::ring_through_bond(frag, edge.0, edge.1);
        let middle = match &cycle {
            Some(c) => centroid(c.iter().map(|&i| positions[i])),
            None => centroid(positions.iter().copied()),
        };
        let inside_a_ring = self.ring_beside(shared, middle);
        let bond_length = {
            let (p, q) = (positions[edge.0], positions[edge.1]);
            (p[0] - q[0]).hypot(p[1] - q[1])
        };
        let clearance = bond_length
            * if inside_a_ring {
                CLEARANCE_INSIDE_A_RING
            } else {
                CLEARANCE
            };

        // The drawing, less what no vertex stands for: a folded hydrogen is part of a label.
        let atoms: Vec<(u32, [f32; 2])> = self
            .atoms
            .iter()
            .filter(|a| !a.group_h)
            .map(|a| (a.id, a.pos))
            .collect();
        let position = |id: u32| atoms.iter().find(|a| a.0 == id).map(|a| a.1);
        let bonds: Vec<(u32, u32, [f32; 2], [f32; 2])> = self
            .bonds
            .iter()
            .filter_map(|b| Some((b.begin, b.end, position(b.begin)?, position(b.end)?)))
            .collect();

        for &i in &new_atoms {
            let p = positions[i];
            let near_atom = atoms
                .iter()
                .any(|a| (a.1[0] - p[0]).hypot(a.1[1] - p[1]) < clearance);
            let near_bond = bonds
                .iter()
                .any(|b| distance_to_segment(p, b.2, b.3) < clearance);
            if near_atom || near_bond {
                return false;
            }
        }
        for &(u, v) in &new_bonds {
            let (p, q) = (positions[u], positions[v]);
            let ends = [ids[u], ids[v]];
            let on_it = atoms
                .iter()
                .any(|a| !ends.contains(&Some(a.0)) && distance_to_segment(a.1, p, q) < clearance);
            let crosses = bonds.iter().any(|b| {
                !ends.contains(&Some(b.0))
                    && !ends.contains(&Some(b.1))
                    && segments_cross(p, q, b.2, b.3)
            });
            if on_it || crosses {
                return false;
            }
        }

        match cycle {
            Some(cycle) => {
                let polygon: Vec<[f32; 2]> = cycle.iter().map(|&i| positions[i]).collect();
                let vertices: Vec<Option<u32>> = cycle.iter().map(|&i| ids[i]).collect();
                let enclosed = atoms
                    .iter()
                    .any(|a| !vertices.contains(&Some(a.0)) && inside_polygon(a.1, &polygon));
                let n = vertices.len();
                let chord = (0..n).any(|i| {
                    (i + 2..n).any(|j| {
                        (j + 1) % n != i
                            && matches!(
                                (vertices[i], vertices[j]),
                                (Some(a), Some(b)) if self.bond_between(a, b).is_some()
                            )
                    })
                });
                !enclosed && !chord
            }
            None => true,
        }
    }

    /// Whether the side of the bond `shared` that `point` lies on is the inside of a ring: walking
    /// the bonds round that side comes back to the bond having enclosed an area.
    ///
    /// Each step takes the sharpest turn towards that side, which is what traces a face of a
    /// planar drawing. On the outside of a molecule the same walk goes round it the other way,
    /// and the area comes out negative.
    fn ring_beside(&self, shared: (u32, u32), point: [f32; 2]) -> bool {
        let at = |id: u32| self.atom_by_id(id).map(|a| a.pos);
        let (Some(p), Some(q)) = (at(shared.0), at(shared.1)) else {
            return false;
        };
        let start = if cross(p, q, point) > 0.0 {
            shared
        } else {
            (shared.1, shared.0)
        };

        let (mut u, mut v) = start;
        let mut area = 0.0;
        for _ in 0..2 * self.bonds.len() + 2 {
            let (Some(from), Some(to)) = (at(u), at(v)) else {
                return false;
            };
            area += from[0] * to[1] - to[0] * from[1];
            // The neighbour first reached turning clockwise from the bond just walked; `u`
            // itself comes last, a full turn round, so a dead end is walked back along.
            let back = (from[1] - to[1]).atan2(from[0] - to[0]);
            let turn = |n: u32| {
                let Some(pos) = at(n) else { return f32::MAX };
                let angle = (pos[1] - to[1]).atan2(pos[0] - to[0]);
                let clockwise = (back - angle).rem_euclid(std::f32::consts::TAU);
                if n == u {
                    std::f32::consts::TAU
                } else {
                    clockwise
                }
            };
            let Some(next) = self
                .neighbor_atom_ids(v)
                .into_iter()
                .min_by(|&x, &y| turn(x).total_cmp(&turn(y)))
            else {
                return false;
            };
            (u, v) = (v, next);
            if (u, v) == start {
                return area > 1e-3;
            }
        }
        false
    }
}

fn centroid(points: impl Iterator<Item = [f32; 2]>) -> [f32; 2] {
    let (mut x, mut y, mut n) = (0.0, 0.0, 0.0);
    for p in points {
        x += p[0];
        y += p[1];
        n += 1.0;
    }
    [x / n, y / n]
}
