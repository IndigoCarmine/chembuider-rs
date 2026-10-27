pub mod mol2;
pub mod cdxml;
pub mod fragment;
pub mod canonical;
pub mod cleanup;
pub mod smiles;
pub mod cdx;
pub mod cdx_codec;
pub mod elements;
pub(crate) mod lookup;
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

    /// What this bond contributes to each of its two atoms' valence.
    pub fn valence(&self) -> i16 {
        match self {
            BondOrder::Single => 1,
            BondOrder::Double => 2,
            BondOrder::Triple => 3,
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

/// Standard organic valence for implicit H calculation (the element's lowest
/// common valence: S counts hydrogens like H₂S, not like a sulfone).
///
/// `None` for anything with no standard valence — a custom label, a metal, a group name —
/// which is the signal that no hydrogen or over-valence conclusion may be drawn about it.
pub fn normal_valence(element: &str) -> Option<u8> {
    match element {
        "C"  => Some(4),
        "N"  => Some(3),
        "O"  => Some(2),
        "S"  => Some(2),
        "P"  => Some(3),
        "F" | "Cl" | "Br" | "I" => Some(1),
        "B"  => Some(3),
        "Si" => Some(4),
        "H"  => Some(1),
        _    => None,
    }
}

/// Highest common valence: the threshold above which an atom is genuinely over-bonded.
///
/// Used by the over-valence warning and by [`Molecule::consume_excess_group_h`], so that
/// hypervalent-but-normal S(VI) and P(V) centers (sulfones, sulfonamides, sulfates,
/// phosphates) are neither flagged nor stripped of hydrogens.
pub fn max_valence(element: &str) -> Option<u8> {
    match element {
        "S" => Some(6),
        "P" => Some(5),
        other => normal_valence(other),
    }
}

/// Valence electrons of the neutral atom, for the elements [`normal_valence`] knows.
fn valence_electrons(element: &str) -> Option<i16> {
    match element {
        "H" => Some(1),
        "B" => Some(3),
        "C" | "Si" => Some(4),
        "N" | "P" => Some(5),
        "O" | "S" => Some(6),
        "F" | "Cl" | "Br" | "I" => Some(7),
        _ => None,
    }
}

/// The valence a charged atom fills with hydrogen: the number of bonds it makes once its
/// charge has added or removed electrons — i.e. the valence of the element it is then
/// isoelectronic with. `None` exactly where [`normal_valence`] is `None`.
///
/// With `e` = valence electrons − charge, the atom makes `min(e, 8 − e)` bonds (`min(e, 2 − e)`
/// for hydrogen, whose shell holds two): up to four electrons each one pairs with a bond
/// partner; past four the extra ones stay as lone pairs. Simply adding the charge to the
/// valence is the same thing only while `e ≥ 4` on the far side of carbon, which is why it
/// held for the onium ions and went wrong for carbon, silicon, boron and hydrogen:
///
/// | atom | e | like | valence | as ion      | `valence + charge` |
/// |------|---|------|---------|-------------|--------------------|
/// | C⁺   | 3 | B    | 3       | CH₃⁺        | 5 (wrong)          |
/// | C⁻   | 5 | N    | 3       | CH₃⁻        | 3                  |
/// | C²⁺  | 2 | Be   | 2       | CH₂²⁺       | 6 (wrong)          |
/// | Si⁺  | 3 | Al   | 3       | SiH₃⁺       | 5 (wrong)          |
/// | Si⁻  | 5 | P    | 3       | SiH₃⁻       | 3                  |
/// | N⁺   | 4 | C    | 4       | NH₄⁺        | 4                  |
/// | N⁻   | 6 | O    | 2       | NH₂⁻        | 2                  |
/// | P⁺   | 4 | Si   | 4       | PH₄⁺        | 4                  |
/// | O⁺   | 5 | N    | 3       | H₃O⁺        | 3                  |
/// | O⁻   | 7 | F    | 1       | HO⁻         | 1                  |
/// | S⁺   | 5 | P    | 3       | H₃S⁺        | 3                  |
/// | F⁻   | 8 | Ne   | 0       | F⁻          | 0                  |
/// | B⁻   | 4 | C    | 4       | BH₄⁻        | 2 (wrong)          |
/// | B⁺   | 2 | Be   | 2       | BH₂⁺        | 4 (wrong)          |
/// | H⁺   | 0 | —    | 0       | H⁺          | 2 (wrong)          |
/// | H⁻   | 2 | He   | 0       | H⁻          | 0                  |
///
/// ChemDraw counts a charged atom's implicit hydrogens this way (a "+" on the end carbon
/// of ethane gives CH₂⁺, on the nitrogen of methylamine NH₃⁺), as do RDKit and the
/// OpenSMILES/MDL readers. It leaves out radicals and the expanded valences of S and P, as
/// `normal_valence` does for the neutral atom. Charges that strip every valence electron or
/// overfill the shell give 0, never a negative valence.
pub(crate) fn charged_valence(element: &str, charge: i8) -> Option<i16> {
    let e = valence_electrons(element)? - charge as i16;
    let shell = if element == "H" { 2 } else { 8 };
    Some(e.min(shell - e).max(0))
}

/// The over-valence threshold of a charged atom: [`charged_valence`] plus whatever expanded
/// valence the element has on top of its lowest one ([`max_valence`] − [`normal_valence`]:
/// +4 for S, +2 for P, 0 for everything else).
///
/// This is the limit the warning ring and [`Molecule::consume_excess_group_h`] measure
/// against, so it moves with a charge exactly as the label's hydrogen count does: wherever
/// the label has no room left for a hydrogen, a bond past this limit is an excess. For
/// `e ≥ 4` (N⁺, O⁺, C⁻, S±, P±, halogens) it equals the old `max_valence + charge`; it differs
/// only where `charged_valence` does — C⁺ and Si⁺ 3, C²⁺ 2, N²⁺ 3, B⁻ 4 (BF₄⁻ is not
/// over-bonded), B⁺ 2, H⁺ 0.
pub(crate) fn charged_max_valence(element: &str, charge: i8) -> Option<i16> {
    let expanded = max_valence(element)? as i16 - normal_valence(element)? as i16;
    Some(charged_valence(element, charge)? + expanded)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Atom {
    pub id: u32,
    pub element: String,
    pub pos: [f32; 2],
    pub charge: i8,
    /// A hydrogen that arrived as part of a group (OH, NH₂, SiH₃, …) rather than being drawn
    /// on purpose. Such a hydrogen is the materialisation of a *label*: it is folded into its
    /// heavy neighbour's text ("NH2") instead of getting a vertex of its own, and the label
    /// re-derives it from valence. A hydrogen the user placed is an ordinary atom and is
    /// always drawn. Meaningless on anything that is not a hydrogen.
    pub group_h: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bond {
    pub id: u32,
    pub begin: u32,
    pub end: u32,
    pub order: BondOrder,
    pub stereo: BondStereo,
}

/// A colored fill painted inside a ring, *behind* the bond lines (ChemDraw's ring shading).
///
/// The ring is stored as the ordered list of the atom **ids** on its cycle (the polygon is
/// their positions connected in order) plus a straight (un-premultiplied) RGBA color. Storing
/// ids — not coordinates — keeps the fill attached to the atoms as they move, and makes the
/// value serde-portable and backend-agnostic: the drawing code turns it into the same fill
/// polygon primitive for both the on-screen (egui) and exported (SVG/EMF) renderers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RingFill {
    /// Atom ids forming the ring, in cyclic (polygon) order.
    pub atoms: Vec<u32>,
    /// Straight (un-premultiplied) RGBA fill color.
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Molecule {
    pub name: String,
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    /// Colored ring interiors (drawn behind bonds). Empty for a plain structure.
    pub ring_fills: Vec<RingFill>,
    next_id: u32,
}

impl Default for Molecule {
    fn default() -> Self {
        Self {
            name: "MOL".to_string(),
            atoms: Vec::new(),
            bonds: Vec::new(),
            ring_fills: Vec::new(),
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
        self.atoms.push(Atom { id, element, pos, charge, group_h: false });
        id
    }

    /// Add an atom that a fragment brings with it. A fragment's hydrogens belong to the group
    /// it draws (see [`Atom::group_h`]); everything else is an ordinary atom.
    fn add_fragment_atom(&mut self, element: String, pos: [f32; 2], charge: i8) -> u32 {
        let id = self.alloc_id();
        let group_h = element == "H";
        self.atoms.push(Atom { id, element, pos, charge, group_h });
        id
    }

    pub fn add_bond(&mut self, begin: u32, end: u32, order: BondOrder) -> u32 {
        if self.bond_between(begin, end).is_some() {
            return 0;
        }
        self.push_bond(begin, end, order, BondStereo::None)
    }

    /// Add a bond the caller knows is not there yet (a reader that tracks its own bonds, where
    /// [`add_bond`](Self::add_bond)'s search through every bond would make reading quadratic).
    fn push_bond(&mut self, begin: u32, end: u32, order: BondOrder, stereo: BondStereo) -> u32 {
        let id = self.alloc_id();
        self.bonds.push(Bond { id, begin, end, order, stereo });
        id
    }

    pub fn remove_atom(&mut self, id: u32) {
        self.atoms.retain(|a| a.id != id);
        self.bonds.retain(|b| b.begin != id && b.end != id);
        self.drop_broken_ring_fills();
    }

    pub fn remove_bond(&mut self, id: u32) {
        self.bonds.retain(|b| b.id != id);
        self.drop_broken_ring_fills();
    }

    /// Drop every ring fill whose cycle is no longer closed. A fill describes a ring only while
    /// all of its atoms exist and each consecutive pair (wrapping around) is still bonded;
    /// once an atom or a ring bond is deleted the polygon would shade an open chain.
    fn drop_broken_ring_fills(&mut self) {
        let mut fills = std::mem::take(&mut self.ring_fills);
        fills.retain(|rf| self.ring_is_closed(&rf.atoms));
        self.ring_fills = fills;
    }

    fn ring_is_closed(&self, cycle: &[u32]) -> bool {
        let n = cycle.len();
        cycle.iter().all(|&id| self.atom_by_id(id).is_some())
            && (0..n).all(|k| self.bond_between(cycle[k], cycle[(k + 1) % n]).is_some())
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

    /// Total bond order carried by `atom_id`.
    pub fn bond_order_sum(&self, atom_id: u32) -> i16 {
        self.bonds_for_atom(atom_id).iter().map(|b| b.order.valence()).sum()
    }

    // ─── Rings ────────────────────────────────────────────────────────────────

    /// Shortest path (inclusive vertex list `[src, …, dst]`) over the bond graph, never
    /// traversing the single undirected edge `forbid`. `None` if `dst` is unreachable.
    fn shortest_path(&self, src: u32, dst: u32, forbid: (u32, u32)) -> Option<Vec<u32>> {
        use std::collections::VecDeque;
        let mut prev: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut visited: std::collections::HashSet<u32> = std::collections::HashSet::from([src]);
        let mut queue: VecDeque<u32> = VecDeque::from([src]);
        while let Some(u) = queue.pop_front() {
            if u == dst {
                let mut path = vec![dst];
                let mut cur = dst;
                while cur != src {
                    cur = prev[&cur];
                    path.push(cur);
                }
                path.reverse();
                return Some(path);
            }
            for n in self.neighbor_atom_ids(u) {
                let is_forbidden = (u == forbid.0 && n == forbid.1) || (u == forbid.1 && n == forbid.0);
                if is_forbidden {
                    continue;
                }
                if visited.insert(n) {
                    prev.insert(n, u);
                    queue.push_back(n);
                }
            }
        }
        None
    }

    /// The smallest ring (cycle) that passes through `atom_id`, as its atom ids in cyclic
    /// order, or `None` when the atom is on no ring. Found by, for each neighbour `n`, taking
    /// the shortest path from `n` back to the atom that does *not* reuse the direct `atom—n`
    /// bond: that path plus the atom closes a cycle, and the shortest such cycle is the ring.
    pub fn smallest_ring_containing(&self, atom_id: u32) -> Option<Vec<u32>> {
        self.atom_by_id(atom_id)?;
        let mut best: Option<Vec<u32>> = None;
        for n in self.neighbor_atom_ids(atom_id) {
            if let Some(path) = self.shortest_path(n, atom_id, (atom_id, n)) {
                // path = [n, …, atom_id]; ring = [atom_id, n, …] (drop the trailing atom_id).
                if path.len() >= 3 {
                    let mut ring = Vec::with_capacity(path.len());
                    ring.push(atom_id);
                    ring.extend_from_slice(&path[..path.len() - 1]);
                    if best.as_ref().map_or(true, |b| ring.len() < b.len()) {
                        best = Some(ring);
                    }
                }
            }
        }
        best
    }

    /// Shortest path like [`shortest_path`](Self::shortest_path), but confined to the vertex
    /// set `allowed`: BFS never steps onto an atom outside it. Both `src` and `dst` are assumed
    /// to be in `allowed`.
    fn shortest_path_within(
        &self,
        src: u32,
        dst: u32,
        forbid: (u32, u32),
        allowed: &std::collections::HashSet<u32>,
    ) -> Option<Vec<u32>> {
        use std::collections::VecDeque;
        let mut prev: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
        let mut visited: std::collections::HashSet<u32> = std::collections::HashSet::from([src]);
        let mut queue: VecDeque<u32> = VecDeque::from([src]);
        while let Some(u) = queue.pop_front() {
            if u == dst {
                let mut path = vec![dst];
                let mut cur = dst;
                while cur != src {
                    cur = prev[&cur];
                    path.push(cur);
                }
                path.reverse();
                return Some(path);
            }
            for n in self.neighbor_atom_ids(u) {
                if !allowed.contains(&n) {
                    continue;
                }
                let is_forbidden =
                    (u == forbid.0 && n == forbid.1) || (u == forbid.1 && n == forbid.0);
                if is_forbidden {
                    continue;
                }
                if visited.insert(n) {
                    prev.insert(n, u);
                    queue.push_back(n);
                }
            }
        }
        None
    }

    /// The smallest ring through `atom_id` whose every vertex lies within `allowed`, in cyclic
    /// order — [`smallest_ring_containing`](Self::smallest_ring_containing) restricted to a
    /// vertex set. `None` when no such ring stays inside the set.
    fn smallest_ring_within(
        &self,
        atom_id: u32,
        allowed: &std::collections::HashSet<u32>,
    ) -> Option<Vec<u32>> {
        self.atom_by_id(atom_id)?;
        let mut best: Option<Vec<u32>> = None;
        for n in self.neighbor_atom_ids(atom_id) {
            if !allowed.contains(&n) {
                continue;
            }
            if let Some(path) = self.shortest_path_within(n, atom_id, (atom_id, n), allowed) {
                if path.len() >= 3 {
                    let mut ring = Vec::with_capacity(path.len());
                    ring.push(atom_id);
                    ring.extend_from_slice(&path[..path.len() - 1]);
                    if best.as_ref().map_or(true, |b| ring.len() < b.len()) {
                        best = Some(ring);
                    }
                }
            }
        }
        best
    }

    /// The ring a *selection* of atoms means, in cyclic (polygon) order, or `None` if the
    /// selected atoms enclose no ring.
    ///
    /// The search is confined to `atoms`, so on a fused ring the caller's selection — not a
    /// shared vertex's globally smallest ring — decides which cycle is filled: selecting exactly
    /// the four atoms of a square fused onto benzene yields the square, and selecting benzene's
    /// six yields the hexagon, even though the two share an edge. When the selection encloses
    /// more than one ring (e.g. both fused rings selected whole) the smallest enclosed ring
    /// wins. Returns the smallest ring found over all selected atoms as seeds.
    pub fn ring_from_atoms(&self, atoms: &[u32]) -> Option<Vec<u32>> {
        let allowed: std::collections::HashSet<u32> = atoms.iter().copied().collect();
        if allowed.len() < 3 {
            return None;
        }
        let mut best: Option<Vec<u32>> = None;
        for &start in &allowed {
            if let Some(ring) = self.smallest_ring_within(start, &allowed) {
                if best.as_ref().map_or(true, |b| ring.len() < b.len()) {
                    best = Some(ring);
                }
            }
        }
        best
    }

    /// Add (or replace) a ring fill on the given cyclic atom-id list with `color` (straight
    /// RGBA). An existing fill covering the same set of atoms is replaced, so re-filling a ring
    /// recolors it rather than stacking translucent layers.
    pub fn set_ring_fill(&mut self, atoms: Vec<u32>, color: [u8; 4]) {
        let key = {
            let mut s = atoms.clone();
            s.sort_unstable();
            s
        };
        self.ring_fills.retain(|rf| {
            let mut s = rf.atoms.clone();
            s.sort_unstable();
            s != key
        });
        self.ring_fills.push(RingFill { atoms, color });
    }

    /// Remove any ring fill whose cycle contains `atom_id`. Returns true if one was removed.
    pub fn clear_ring_fill_containing(&mut self, atom_id: u32) -> bool {
        let before = self.ring_fills.len();
        self.ring_fills.retain(|rf| !rf.atoms.contains(&atom_id));
        self.ring_fills.len() != before
    }

    /// Drop the group hydrogens that `atom_id` no longer has room for, and only those.
    ///
    /// A fragment's hydrogens ([`Atom::group_h`]) are the materialisation of a *label*: the "2"
    /// in NH₂ is a statement about the nitrogen's spare valence, not an atom the user asked
    /// for. So when a bond arrives and that spare valence is gone, the honest thing is for the
    /// hydrogens to go with it — Ph–N=N–Ph is what the two `n` keys meant, not a pair of
    /// pentavalent nitrogens wearing a warning ring. **Call this from editing paths only**;
    /// import, layout and fragment→molecule conversion all build graphs through
    /// [`add_bond`](Self::add_bond) and must not have hydrogens quietly removed underneath them.
    ///
    /// Three things bound what it will take:
    ///
    /// - **Only group hydrogens.** One the user placed by hand (`h`) is an ordinary atom; it
    ///   stays, and the over-valence warning it causes is correct and points at something the
    ///   user can see and delete.
    /// - **Only terminal ones**, so removing a hydrogen can never split the molecule.
    /// - **Only the excess**, measured against [`charged_max_valence`] ([`max_valence`] moved
    ///   by the charge) — the same threshold the warning ring uses, so this removes exactly
    ///   what would have been flagged and stops there. (The lower [`normal_valence`] would
    ///   strip the hydrogen off every S the moment it picked up a second bond, which is wrong
    ///   for sulfoxides and would break `s`+`Delete`, the app's only route to a bare sulfur.)
    ///
    /// Hydrogens go **highest id first**, i.e. most recently added first, which both makes the
    /// choice independent of iteration order and agrees with
    /// [`is_folded_h`](crate::widget::draw::is_folded_h): that folds the *lowest* ids into the
    /// label, so the one this removes is the one that had already stopped being folded.
    ///
    /// Returns the ids removed.
    pub fn consume_excess_group_h(&mut self, atom_id: u32) -> Vec<u32> {
        let Some(atom) = self.atom_by_id(atom_id) else { return Vec::new() };
        let Some(limit) = charged_max_valence(&atom.element, atom.charge) else { return Vec::new() };
        let mut sum = self.bond_order_sum(atom_id);
        if sum <= limit {
            return Vec::new();
        }

        let mut candidates: Vec<(u32, i16)> = self
            .bonds_for_atom(atom_id)
            .iter()
            .map(|b| (if b.begin == atom_id { b.end } else { b.begin }, b.order.valence()))
            .filter(|&(other, _)| {
                self.atom_by_id(other).is_some_and(|a| a.element == "H" && a.group_h)
                    && self.bonds_for_atom(other).len() == 1
            })
            .collect();
        candidates.sort_by_key(|&(h, _)| std::cmp::Reverse(h));

        let mut removed = Vec::new();
        for (h, contribution) in candidates {
            if sum <= limit {
                break;
            }
            self.remove_atom(h);
            sum -= contribution;
            removed.push(h);
        }
        removed
    }

    /// Snap radius for merging overlapping atoms (molecular units).
    pub const SNAP_RADIUS: f32 = 0.4;

    /// Where every fragment atom would land if the fragment were inserted at `attach_to` with
    /// this `angle`/`flip` — the same transform [`insert_fragment`](Self::insert_fragment) uses,
    /// exposed so a caller can inspect a placement *before* committing to it.
    ///
    /// The entry at `frag.attach_idx` is the attach atom's own position. Returns an empty vec
    /// when there is nothing to place (missing atom, or a fragment of fewer than two atoms —
    /// `insert_fragment` declines those too, so the two agree by construction).
    pub fn fragment_placement(
        &self,
        frag: &fragment::Fragment,
        attach_to: u32,
        angle: f32,
        flip: bool,
    ) -> Vec<[f32; 2]> {
        let Some(base_pos) = self.atom_by_id(attach_to).map(|a| a.pos) else {
            return Vec::new();
        };
        if frag.atoms.len() < 2 {
            return Vec::new();
        }
        let next_idx = (frag.attach_idx + 1) % frag.atoms.len();
        let attach_raw = frag.atoms[frag.attach_idx].pos;
        let next_raw = frag.atoms[next_idx].pos;
        let raw_angle = (next_raw[1] - attach_raw[1]).atan2(next_raw[0] - attach_raw[0]);

        // Canonicalize: rotate by -raw_angle, then optionally flip y, then rotate by angle.
        let (sin_neg, cos_neg) = (-raw_angle).sin_cos();
        let (sin_a, cos_a) = angle.sin_cos();

        frag.atoms
            .iter()
            .map(|fa| {
                let dx = fa.pos[0] - attach_raw[0];
                let dy = fa.pos[1] - attach_raw[1];
                // Step 1: canonicalize (rotate so raw_angle → 0)
                let dx_c = dx * cos_neg - dy * sin_neg;
                let dy_c = (dx * sin_neg + dy * cos_neg) * if flip { -1.0 } else { 1.0 };
                // Step 2: rotate to target angle
                let rx = dx_c * cos_a - dy_c * sin_a;
                let ry = dx_c * sin_a + dy_c * cos_a;
                [base_pos[0] + rx, base_pos[1] + ry]
            })
            .collect()
    }

    /// How many atoms the fragment would *lose* to `insert_fragment`'s snap at this placement.
    ///
    /// Not every snap is a loss, and telling the two apart is the whole job here.
    ///
    /// - A vertex **on one of the fragment's own rings** may land on an existing atom: the two
    ///   rings then share the bond between them, which is ring fusion and exactly what `5` on a
    ///   benzene carbon is for (indole, purine, the benzothiophene golden…). The fragment
    ///   arrives whole; it just overlaps what was already drawn.
    /// - Any **branch** atom that lands on one is pure loss. It is a substituent, so merging
    ///   does not fuse anything — it re-roots that branch onto the neighbour (a *tert*-butyl
    ///   that jumps off the silicon and onto the chain) or, for a terminal atom, drops it
    ///   entirely against a bond that already exists. `c 1 S` drew SiH₂ this way.
    ///
    /// Ring membership is decided by pruning leaves until none are left; whatever survives is
    /// on a cycle. [`best_fragment_placement`](crate::ChemStructEditor::best_fragment_placement)
    /// rotates the placement until this count is zero.
    pub fn fragment_atoms_lost_to_snap(
        &self,
        frag: &fragment::Fragment,
        attach_to: u32,
        angle: f32,
        flip: bool,
    ) -> usize {
        let on_ring = fragment::ring_membership(frag);
        self.fragment_placement(frag, attach_to, angle, flip)
            .into_iter()
            .enumerate()
            .filter(|&(i, pos)| {
                i != frag.attach_idx
                    && !on_ring[i]
                    && self.find_atom_near(pos, Self::SNAP_RADIUS).is_some()
            })
            .count()
    }

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
        let positions = self.fragment_placement(frag, attach_to, angle, flip);
        if positions.is_empty() {
            return vec![];
        }

        let mut id_map: Vec<Option<u32>> = vec![None; frag.atoms.len()];
        id_map[frag.attach_idx] = Some(attach_to);

        let mut new_ids: Vec<u32> = Vec::new();
        for (i, fa) in frag.atoms.iter().enumerate() {
            if i == frag.attach_idx { continue; }
            let new_pos = positions[i];
            // Snap: if an existing atom is within SNAP_RADIUS, reuse it instead of creating a duplicate
            let new_id = if let Some(existing) = self.find_atom_near(new_pos, Self::SNAP_RADIUS) {
                existing
            } else {
                self.add_fragment_atom(fa.element.to_string(), new_pos, fa.charge)
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

    /// The valence a charged atom fills with hydrogen, for the elements the table in
    /// [`charged_valence`] covers — including the doubly charged and the hydrogen ions.
    #[test]
    fn charged_valence_follows_the_isoelectronic_element() {
        for (el, charge, want) in [
            ("C", 0, Some(4)), ("C", 1, Some(3)), ("C", -1, Some(3)), ("C", 2, Some(2)),
            ("N", 0, Some(3)), ("N", 1, Some(4)), ("N", -1, Some(2)), ("N", 2, Some(3)),
            ("O", 0, Some(2)), ("O", 1, Some(3)), ("O", -1, Some(1)), ("O", 2, Some(4)),
            ("B", 0, Some(3)), ("B", -1, Some(4)), ("B", 1, Some(2)),
            ("F", 0, Some(1)), ("F", -1, Some(0)), ("Cl", 1, Some(2)),
            ("H", 0, Some(1)), ("H", 1, Some(0)), ("H", -1, Some(0)),
            ("Si", -1, Some(3)), ("S", -1, Some(1)), ("P", 1, Some(4)),
            ("Fe", 2, None), ("Xx", 0, None),
        ] {
            assert_eq!(charged_valence(el, charge), want, "{el} with charge {charge:+}");
        }
        // Neutral, the table agrees with `normal_valence` element for element.
        for el in ["C", "N", "O", "S", "P", "F", "Cl", "Br", "I", "B", "Si", "H"] {
            assert_eq!(
                charged_valence(el, 0),
                normal_valence(el).map(i16::from),
                "{el}: the electron table disagrees with normal_valence",
            );
        }
    }

    /// The over-valence threshold moves with a charge exactly as the hydrogen count does, on
    /// top of the expanded valence of S and P. Where the electron count stays at four or more
    /// it is the old `max_valence + charge`; the seven ions listed last are where it is not.
    #[test]
    fn charged_max_valence_moves_with_the_label() {
        for el in ["C", "N", "O", "S", "P", "F", "Cl", "Br", "I", "B", "Si", "H"] {
            assert_eq!(charged_max_valence(el, 0), max_valence(el).map(i16::from), "{el}");
        }
        for (el, charge) in [
            ("N", 1), ("N", -1), ("O", 1), ("O", -1), ("C", -1), ("S", 1), ("S", -1), ("S", 2),
            ("P", 1), ("P", -1), ("Cl", 1), ("Br", -1), ("Si", -1),
        ] {
            assert_eq!(
                charged_max_valence(el, charge),
                max_valence(el).map(|v| v as i16 + charge as i16),
                "{el}{charge:+} keeps the old threshold",
            );
        }
        for (el, charge, want) in [
            ("C", 1, 3), ("Si", 1, 3), ("C", 2, 2), ("N", 2, 3), ("B", -1, 4), ("B", 1, 2),
            ("H", 1, 0),
        ] {
            assert_eq!(charged_max_valence(el, charge), Some(want), "{el}{charge:+}");
        }
        assert_eq!(charged_max_valence("Fe", 2), None);
    }

    /// A heavy atom carrying `n` group hydrogens, plus one heavy neighbour to bond onto.
    fn with_group_h(element: &str, n: usize) -> (Molecule, u32, u32, Vec<u32>) {
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let x = m.add_atom(element.to_string(), [1.5, 0.0], 0);
        m.add_bond(c, x, BondOrder::Single);
        let hs = (0..n)
            .map(|i| {
                let h = m.add_fragment_atom("H".to_string(), [3.0, i as f32], 0);
                m.add_bond(x, h, BondOrder::Single);
                h
            })
            .collect();
        (m, c, x, hs)
    }

    /// Nothing is consumed while the atom still has room, and exactly the excess goes when it
    /// runs out. The count is what matters: an amine that lost *both* hydrogens the moment it
    /// picked up a second bond would be as wrong as one that lost neither.
    #[test]
    fn only_the_excess_group_hydrogens_are_consumed() {
        // N with two group hydrogens and one heavy bond: full, but not over.
        let (mut m, _c, n, hs) = with_group_h("N", 2);
        assert!(m.consume_excess_group_h(n).is_empty(), "a full valence is not an excess");
        assert_eq!(m.atoms.len(), 4);

        // A second heavy neighbour takes one of the three: exactly one hydrogen goes.
        let c2 = m.add_atom("C".to_string(), [1.5, 1.5], 0);
        m.add_bond(n, c2, BondOrder::Single);
        assert_eq!(
            m.consume_excess_group_h(n), vec![hs[1]],
            "the excess is one hydrogen, and it is the most recently added",
        );
        assert!(m.atom_by_id(hs[0]).is_some(), "the other hydrogen is still needed");
        assert_eq!(m.bond_order_sum(n), 3, "and the nitrogen is back at its valence");

        // Raising that bond to a double takes the last one too.
        let bid = m.bond_between(n, c2).expect("the bond exists").id;
        m.bond_by_id_mut(bid).expect("the bond exists").order = BondOrder::Double;
        assert_eq!(m.consume_excess_group_h(n), vec![hs[0]]);
        assert_eq!(m.bond_order_sum(n), 3);
    }

    /// A hydrogen the user drew is an atom, not a label decoration: it is never taken, and the
    /// over-valence it causes is the correct, visible answer.
    #[test]
    fn a_hand_placed_hydrogen_is_never_consumed() {
        let mut m = Molecule::default();
        let c = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let n = m.add_atom("N".to_string(), [1.5, 0.0], 0);
        m.add_bond(c, n, BondOrder::Single);
        let explicit = m.add_atom("H".to_string(), [3.0, 0.0], 0);
        m.add_bond(n, explicit, BondOrder::Single);
        let c2 = m.add_atom("C".to_string(), [1.5, 1.5], 0);
        m.add_bond(n, c2, BondOrder::Double);

        assert!(m.consume_excess_group_h(n).is_empty(), "nothing here is a group's hydrogen");
        assert!(m.atom_by_id(explicit).is_some());
        assert_eq!(m.bond_order_sum(n), 4, "the nitrogen stays over-valence, visibly so");
    }

    /// The threshold is the element's *highest* common valence, the same one the warning ring
    /// uses. Sulfur is where the two differ: an S–H on a sulfoxide sulfur is ordinary chemistry
    /// and must survive, and only the seventh bond-order takes it.
    #[test]
    fn sulfur_is_measured_against_its_hypervalent_limit() {
        let (mut m, _c, s, hs) = with_group_h("S", 1);
        for (i, order) in [BondOrder::Double, BondOrder::Double].into_iter().enumerate() {
            let o = m.add_atom("O".to_string(), [1.5, -(i as f32) - 1.0], 0);
            m.add_bond(s, o, order);
            assert!(
                m.consume_excess_group_h(s).is_empty(),
                "S(IV)/S(VI) is not over-valence, so its hydrogen stays",
            );
        }
        assert_eq!(m.bond_order_sum(s), 6);

        let c2 = m.add_atom("C".to_string(), [3.0, -3.0], 0);
        m.add_bond(s, c2, BondOrder::Single);
        assert_eq!(m.consume_excess_group_h(s), vec![hs[0]], "the seventh bond-order takes it");
    }

    /// Only *terminal* group hydrogens are candidates, so consuming one can never split the
    /// molecule in two — a bridging hydrogen is left alone however over-valent its neighbour is.
    #[test]
    fn a_bridging_group_hydrogen_is_left_alone() {
        let mut m = Molecule::default();
        let o = m.add_atom("O".to_string(), [0.0, 0.0], 0);
        let h = m.add_fragment_atom("H".to_string(), [1.5, 0.0], 0);
        let far = m.add_atom("C".to_string(), [3.0, 0.0], 0);
        let c1 = m.add_atom("C".to_string(), [0.0, 1.5], 0);
        let c2 = m.add_atom("C".to_string(), [0.0, -1.5], 0);
        for (a, b) in [(o, h), (h, far), (o, c1), (o, c2)] {
            m.add_bond(a, b, BondOrder::Single);
        }
        assert!(m.consume_excess_group_h(o).is_empty(), "removing it would disconnect `far`");
        assert_eq!(m.atoms.len(), 5);
    }

    /// Unknown elements have no valence to exceed, so nothing is ever taken from them.
    #[test]
    fn an_element_with_no_standard_valence_keeps_its_hydrogens() {
        let (mut m, _c, x, _hs) = with_group_h("Zz", 3);
        assert!(m.consume_excess_group_h(x).is_empty());
        assert!(m.consume_excess_group_h(9999).is_empty(), "and a missing atom is not a panic");
    }

    /// A ring of `n` carbons, returned with the atom ids in order.
    fn carbon_ring(n: usize) -> (Molecule, Vec<u32>) {
        let mut m = Molecule::default();
        let ids: Vec<u32> = (0..n)
            .map(|k| {
                let a = (k as f32) * std::f32::consts::TAU / n as f32;
                m.add_atom("C".to_string(), [a.cos(), a.sin()], 0)
            })
            .collect();
        for k in 0..n {
            m.add_bond(ids[k], ids[(k + 1) % n], BondOrder::Single);
        }
        (m, ids)
    }

    #[test]
    fn smallest_ring_finds_the_cycle_through_an_atom() {
        let (m, ids) = carbon_ring(6);
        let ring = m.smallest_ring_containing(ids[0]).expect("benzene atom is on a ring");
        assert_eq!(ring.len(), 6, "the six-membered ring");
        assert_eq!(ring[0], ids[0], "the queried atom leads the cycle");
        // Every ring atom appears exactly once, and they are all the ring's atoms.
        let mut got = ring.clone();
        got.sort_unstable();
        let mut want = ids.clone();
        want.sort_unstable();
        assert_eq!(got, want);
    }

    #[test]
    fn smallest_ring_prefers_the_smaller_of_fused_rings() {
        // A cyclohexane fused to a cyclopropane sharing an edge (ids[0]-ids[1]).
        let (mut m, ids) = carbon_ring(6);
        let bridge = m.add_atom("C".to_string(), [2.0, 0.0], 0);
        m.add_bond(ids[0], bridge, BondOrder::Single);
        m.add_bond(ids[1], bridge, BondOrder::Single);
        let ring = m.smallest_ring_containing(ids[0]).unwrap();
        assert_eq!(ring.len(), 3, "the 3-membered ring is smaller than the 6-membered one");
        assert!(ring.contains(&bridge) && ring.contains(&ids[1]));
    }

    #[test]
    fn a_chain_atom_is_on_no_ring() {
        let mut m = Molecule::default();
        let a = m.add_atom("C".to_string(), [0.0, 0.0], 0);
        let b = m.add_atom("C".to_string(), [1.0, 0.0], 0);
        m.add_bond(a, b, BondOrder::Single);
        assert!(m.smallest_ring_containing(a).is_none());
    }

    #[test]
    fn ring_fill_is_set_replaced_and_pruned() {
        let (mut m, ids) = carbon_ring(6);
        let ring = m.smallest_ring_containing(ids[0]).unwrap();
        m.set_ring_fill(ring.clone(), [1, 2, 3, 4]);
        assert_eq!(m.ring_fills.len(), 1);

        // Re-filling the same ring (any rotation) replaces rather than stacks.
        let mut rotated = ring.clone();
        rotated.rotate_left(2);
        m.set_ring_fill(rotated, [9, 9, 9, 255]);
        assert_eq!(m.ring_fills.len(), 1);
        assert_eq!(m.ring_fills[0].color, [9, 9, 9, 255]);

        // Deleting a ring atom drops the now-meaningless fill.
        m.remove_atom(ids[0]);
        assert!(m.ring_fills.is_empty(), "a fill whose ring lost an atom is removed");
    }

    #[test]
    fn removing_a_ring_bond_drops_the_fill_but_other_bonds_do_not() {
        let (mut m, ids) = carbon_ring(6);
        let sub = m.add_atom("C".to_string(), [2.0, 0.0], 0);
        let sub_bond = m.add_bond(ids[0], sub, BondOrder::Single);
        let ring = m.smallest_ring_containing(ids[0]).unwrap();
        m.set_ring_fill(ring, [1, 2, 3, 4]);

        // A bond that is not on the filled cycle leaves the ring intact.
        m.remove_bond(sub_bond);
        assert_eq!(m.ring_fills.len(), 1, "an unrelated bond's removal keeps the fill");

        // Opening the ring leaves a chain: the fill no longer describes a ring.
        let ring_bond = m.bond_between(ids[2], ids[3]).unwrap().id;
        m.remove_bond(ring_bond);
        assert!(m.ring_fills.is_empty(), "a fill whose ring lost a bond is removed");
    }

    #[test]
    fn removing_a_bond_of_the_other_fused_ring_keeps_the_fill() {
        // A cyclohexane fused to a cyclopropane on the ids[0]-ids[1] edge; only the
        // six-membered ring is filled.
        let (mut m, ids) = carbon_ring(6);
        let bridge = m.add_atom("C".to_string(), [2.0, 0.0], 0);
        let b0 = m.add_bond(ids[0], bridge, BondOrder::Single);
        m.add_bond(ids[1], bridge, BondOrder::Single);
        m.set_ring_fill(ids.clone(), [1, 2, 3, 4]);

        // Breaking the three-membered ring does not touch the filled six-ring.
        m.remove_bond(b0);
        assert_eq!(m.ring_fills.len(), 1);

        // Removing the shared edge does open the six-ring.
        let shared = m.bond_between(ids[0], ids[1]).unwrap().id;
        m.remove_bond(shared);
        assert!(m.ring_fills.is_empty(), "the shared edge was on the filled cycle");
    }
}
