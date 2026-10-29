//! Stereogenic units of a drawn structure and their CIP descriptors (P-93): R/S for tetrahedral
//! carbon centres from the wedges and hashes drawn at them, E/Z for C=C double bonds from the
//! drawn geometry.
//!
//! A unit gets a descriptor only when the drawing fixes it; otherwise the reason is kept, and
//! the name leaves the unit undescribed rather than guess:
//!
//! - a centre is read only from wedges whose narrow end is at it (IUPAC 2006 graphical
//!   representation, ST-1.2): solid (toward the viewer) or hashed (away). No wedge, a wavy bond
//!   (configuration unspecified), a bold, hollow, dashed or plain hashed bond, or a drawing
//!   whose wedges leave the sense of rotation undecided (all ligands nearly in a line) gives
//!   no descriptor;
//! - a double bond needs its two ends and a ligand on each end drawn off the line through
//!   them; a wavy bond on either end atom, a straight (collinear) drawing or two ligands of an
//!   end drawn on one side gives no descriptor;
//! - double bonds in rings of fewer than eight members are not stereogenic units for naming
//!   (P-93: their geometry is fixed), nor are the Z double bonds of an eight-membered ring with
//!   more than one (P-91.2.2: "cycloocta-1,3,5,7-tetraene");
//! - out of scope, with no descriptor: centres on atoms other than carbon, ligands that tie under
//!   CIP Rule 1a but may differ by Rules 1b–5 (pseudoasymmetric centres, r/s; reported when the
//!   tied ligands carry stereo bonds or stereogenic double bonds of their own, unless the
//!   descriptors found show them alike), allenes (axial chirality, reported),
//!   longer cumulenes, atropisomeric axes and planar chirality (not detected), relative
//!   configuration (rel-, rac-, R*/S*: a drawing's wedges are read as absolute).

use super::cip::{self, Ligand, Unranked};
use super::graph::{El, End, Graph, Wedge};

/// A CIP stereodescriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Descriptor {
    R,
    S,
    E,
    Z,
}

impl std::fmt::Display for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Descriptor::R => "R",
            Descriptor::S => "S",
            Descriptor::E => "E",
            Descriptor::Z => "Z",
        })
    }
}

/// Where a stereogenic unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Site {
    /// A tetrahedral centre.
    Centre(usize),
    /// A double bond between two atoms.
    Double(usize, usize),
    /// The axis of an allene, between its two terminal atoms.
    Axis(usize, usize),
}

/// A stereogenic unit: its descriptor, or why it has none.
#[derive(Clone, Debug)]
pub(crate) struct Unit {
    pub site: Site,
    pub descriptor: Result<Descriptor, String>,
}

/// The atomic number of an element of the namer.
fn atomic_number(e: El) -> u8 {
    match e {
        El::C => 6,
        El::N => 7,
        El::O => 8,
        El::F => 9,
        El::S => 16,
        El::Cl => 17,
        El::Br => 35,
        El::I => 53,
    }
}

/// The stereogenic units of `g`.
pub(crate) fn units(g: &Graph) -> Vec<Unit> {
    let z: Vec<u8> = g.el.iter().map(|&e| atomic_number(e)).collect();
    let atoms = cip::Atoms::new(z, &g.h, &g.adj);
    let mut out = Vec::new();
    for c in 0..g.len() {
        if let Some(u) = centre(g, &atoms, c, None) {
            out.push(u);
        }
    }
    for a in 0..g.len() {
        for &(b, o) in &g.adj[a] {
            if a < b
                && o == 2
                && let Some(u) = double_bond(g, &atoms, a, b)
            {
                out.push(u);
            }
        }
        if let Some(u) = allene(g, &atoms, a) {
            out.push(u);
        }
    }
    // A centre whose tied ligands bear stereogenic units of their own is one only where their
    // configurations differ: decided with the descriptors just found.
    let first = out.clone();
    out.retain(|u| match (u.site, &u.descriptor) {
        (Site::Centre(c), Err(why)) if why == PSEUDOASYMMETRIC => {
            centre(g, &atoms, c, Some(&first)).is_some()
        }
        _ => true,
    });
    out
}

/// Why a centre whose tied ligands differ only in their own configuration has no descriptor.
const PSEUDOASYMMETRIC: &str = "two ligands differ only in their own configuration (a \
                                pseudoasymmetric centre, r/s, or CIP Rules 3–5), which is not \
                                supported";

/// The ligands of `x` other than `except`: its neighbours, then its hydrogens.
fn ligands(g: &Graph, x: usize, except: Option<usize>) -> Vec<Ligand> {
    let mut l: Vec<Ligand> = g
        .nbrs(x)
        .filter(|&m| Some(m) != except)
        .map(Ligand::Atom)
        .collect();
    l.extend((0..g.h[x]).map(|_| Ligand::H));
    l
}

/// A tetrahedral carbon centre with four different ligands. Two ligands that tie under Rule 1a
/// but bear stereogenic units of their own may still differ in configuration: with the units
/// `known` from a first pass, the centre is left out where they do not.
fn centre(g: &Graph, atoms: &cip::Atoms, c: usize, known: Option<&[Unit]>) -> Option<Unit> {
    if g.el[c] != El::C
        || g.charge[c] != 0
        || g.free_on(c) > 0
        || g.h[c] > 1
        || g.adj[c].len() + g.h[c] as usize != 4
        || g.adj[c].iter().any(|&(_, o)| o != 1)
    {
        return None;
    }
    let ligs = ligands(g, c, None);
    let descriptor = match cip::rank(atoms, c, &ligs) {
        Ok(order) => centre_descriptor(g, c, &ligs, &order),
        Err(Unranked::Tie(i, j)) => {
            // Alike constitutionally; they may still differ in their own configuration
            // (Rules 3–5).
            let (Ligand::Atom(a), Ligand::Atom(b)) = (ligs[i], ligs[j]) else {
                return None;
            };
            if !(branch_marked(g, atoms, &[c], a) || branch_marked(g, atoms, &[c], b)) {
                return None;
            }
            if known.is_some_and(|k| alike_configurations(g, k, c, a, b)) {
                return None;
            }
            Err(PSEUDOASYMMETRIC.into())
        }
        Err(Unranked::TooLarge) => {
            if !wedged_at(g, c) {
                return None;
            }
            Err("the structure is too large to rank the ligands by the CIP rules".into())
        }
    };
    Some(Unit {
        site: Site::Centre(c),
        descriptor,
    })
}

/// Whether a wedge or hash starts at `c`.
fn wedged_at(g: &Graph, c: usize) -> bool {
    g.drawn
        .marks
        .iter()
        .any(|m| m.narrow == End::Atom(c) && matches!(m.wedge, Wedge::Up | Wedge::Down))
}

/// Whether the branch through `a` away from the atoms `from` has stereo bonds of its own, a
/// double bond that can be stereogenic or an allene.
fn branch_marked(g: &Graph, atoms: &cip::Atoms, from: &[usize], a: usize) -> bool {
    let mut seen = vec![false; g.len()];
    for &c in from {
        seen[c] = true;
    }
    seen[a] = true;
    let mut stack = vec![a];
    while let Some(x) = stack.pop() {
        if wedged_at(g, x)
            || g.adj[x]
                .iter()
                .any(|&(y, o)| o == 2 && stereogenic_double(g, atoms, x, y))
            || allene(g, atoms, x).is_some()
        {
            return true;
        }
        for y in g.nbrs(x) {
            if !seen[y] {
                seen[y] = true;
                stack.push(y);
            }
        }
    }
    false
}

/// Whether the branches through `a` and `b` away from the centre `c`, alike under Rule 1a, have
/// the same configuration: they share no atom (no ring), and their stereogenic units in `known`,
/// by distance from `c`, have the same descriptors. Only one unit at each distance, all of them
/// described, is decided; anything else is taken to differ.
fn alike_configurations(g: &Graph, known: &[Unit], c: usize, a: usize, b: usize) -> bool {
    // The distance from `c` of each atom of the branch through `top`.
    let distances = |top: usize| {
        let mut dist = vec![usize::MAX; g.len()];
        dist[c] = 0;
        dist[top] = 1;
        let mut queue = std::collections::VecDeque::from([top]);
        while let Some(x) = queue.pop_front() {
            for y in g.nbrs(x) {
                if dist[y] == usize::MAX {
                    dist[y] = dist[x] + 1;
                    queue.push_back(y);
                }
            }
        }
        dist
    };
    let (da, db) = (distances(a), distances(b));
    if (0..g.len()).any(|x| x != c && da[x] != usize::MAX && db[x] != usize::MAX) {
        return false;
    }
    let units_of = |dist: &[usize]| -> Option<Vec<(usize, Descriptor)>> {
        let mut found = Vec::new();
        for u in known {
            let at = match u.site {
                Site::Centre(x) => vec![x],
                Site::Double(x, y) | Site::Axis(x, y) => vec![x, y],
            };
            let inside = at
                .iter()
                .filter(|&&x| x != c && dist[x] != usize::MAX)
                .count();
            if inside == 0 {
                continue;
            }
            if inside < at.len() {
                return None;
            }
            let d = at
                .iter()
                .map(|&x| dist[x])
                .min()
                .expect("a unit has an atom");
            found.push((d, *u.descriptor.as_ref().ok()?));
        }
        found.sort_by_key(|u| u.0);
        found.windows(2).all(|w| w[0].0 != w[1].0).then_some(found)
    };
    units_of(&da).is_some_and(|ua| units_of(&db) == Some(ua))
}

/// A drawn position as a vector in a right-handed frame (y upward), relative to `from`.
fn rel(p: [f32; 2], from: [f32; 2]) -> [f32; 2] {
    [p[0] - from[0], from[1] - p[1]]
}

fn unit2(v: [f32; 2]) -> Option<[f32; 2]> {
    let n = v[0].hypot(v[1]);
    (n > 1e-6).then(|| [v[0] / n, v[1] / n])
}

fn wavy_at(g: &Graph, x: usize) -> bool {
    g.drawn
        .marks
        .iter()
        .any(|m| m.wedge == Wedge::Wavy && (m.narrow == End::Atom(x) || m.wide == End::Atom(x)))
}

/// R or S for the centre `c` with `ligs` ranked `order` (highest first), from the wedges
/// drawn at it.
fn centre_descriptor(
    g: &Graph,
    c: usize,
    ligs: &[Ligand],
    order: &[usize],
) -> Result<Descriptor, String> {
    let d = &g.drawn;
    if wavy_at(g, c) {
        return Err("drawn with a wavy bond: the configuration is not specified".into());
    }
    let at_c: Vec<_> = d
        .marks
        .iter()
        .filter(|m| m.narrow == End::Atom(c))
        .collect();
    if at_c.iter().any(|m| m.wedge == Wedge::Unclear) {
        return Err(
            "drawn with a bold, hollow, dashed or plain hashed bond, which does not fix \
                    the configuration"
                .into(),
        );
    }
    if !at_c
        .iter()
        .any(|m| matches!(m.wedge, Wedge::Up | Wedge::Down))
    {
        return Err("stereocentre drawn without a wedge or hash bond starting at it".into());
    }
    let inside = || "inside an abbreviation, which is not drawn".to_string();
    let p0 = d.pos[c].ok_or_else(inside)?;
    // Each ligand as a 3D vector: its drawn direction, with the wedge's depth.
    let mut v: Vec<Option<[f32; 3]>> = Vec::new();
    for &k in order {
        let (end, pos) = match ligs[k] {
            Ligand::Atom(a) => (End::Atom(a), Some(d.pos[a].ok_or_else(inside)?)),
            Ligand::H => match d.h.iter().position(|&(on, p)| on == c && p.is_some()) {
                Some(i) => (End::H(i), d.h[i].1),
                None => (End::Free, None),
            },
        };
        let Some(pos) = pos else {
            v.push(None);
            continue;
        };
        let [x, y] = unit2(rel(pos, p0)).ok_or_else(inside)?;
        let depth = at_c
            .iter()
            .find(|m| m.wide == end)
            .map_or(0.0, |m| if m.wedge == Wedge::Up { 1.0 } else { -1.0 });
        v.push(Some([x, y, depth]));
    }
    // An implicit hydrogen points away from the other three.
    let sum = v
        .iter()
        .flatten()
        .fold([0.0; 3], |s, a| [s[0] + a[0], s[1] + a[1], s[2] + a[2]]);
    let v: Vec<[f32; 3]> = v
        .into_iter()
        .map(|a| a.unwrap_or([-sum[0], -sum[1], -sum[2]]))
        .collect();
    let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let (a, b, e) = (sub(v[0], v[3]), sub(v[1], v[3]), sub(v[2], v[3]));
    let det = a[0] * (b[1] * e[2] - b[2] * e[1]) - a[1] * (b[0] * e[2] - b[2] * e[0])
        + a[2] * (b[0] * e[1] - b[1] * e[0]);
    // Positive: 1 → 2 → 3 counterclockwise seen with 4 away from the viewer.
    if det.abs() < 0.1 {
        Err("the wedges drawn leave the configuration undecided".into())
    } else if det > 0.0 {
        Ok(Descriptor::S)
    } else {
        Ok(Descriptor::R)
    }
}

/// The atoms of the smallest ring through the bond `a`–`b`, if it is in one, from `a` around to
/// `b`.
fn smallest_ring(g: &Graph, a: usize, b: usize) -> Option<Vec<usize>> {
    let mut up = vec![usize::MAX; g.len()];
    up[a] = a;
    let mut queue = std::collections::VecDeque::from([a]);
    while let Some(x) = queue.pop_front() {
        for y in g.nbrs(x) {
            if (x, y) == (a, b) || up[y] != usize::MAX {
                continue;
            }
            up[y] = x;
            if y == b {
                let mut ring = vec![b];
                while *ring.last().expect("not empty") != a {
                    ring.push(up[*ring.last().expect("not empty")]);
                }
                ring.reverse();
                return Some(ring);
            }
            queue.push_back(y);
        }
    }
    None
}

/// The double bonds of `ring` (its atoms in order around it).
fn ring_doubles(g: &Graph, ring: &[usize]) -> usize {
    (0..ring.len())
        .filter(|&i| {
            let (x, y) = (ring[i], ring[(i + 1) % ring.len()]);
            g.adj[x].iter().any(|&(m, o)| m == y && o == 2)
        })
        .count()
}

/// Whether `x` has a double bond to an atom other than `except`.
fn other_double(g: &Graph, x: usize, except: usize) -> bool {
    g.adj[x].iter().any(|&(m, o)| m != except && o >= 2)
}

/// Whether the double bond `a`–`b` (in the smallest ring `ring`, if any) can be a stereogenic
/// unit by its constitution: between two carbons, not cumulated, not in a ring of fewer than
/// eight members (P-93).
fn double_bond_possible(g: &Graph, a: usize, b: usize, ring: Option<&[usize]>) -> bool {
    g.el[a] == El::C
        && g.el[b] == El::C
        && g.free_on(a) + g.free_on(b) == 0
        && !other_double(g, a, b)
        && !other_double(g, b, a)
        && !ring.is_some_and(|r| r.len() < 8)
}

/// Whether the double bond `a`–`b` can be stereogenic: possible by its constitution, with two
/// ligands on each end that Rule 1a does not tie.
fn stereogenic_double(g: &Graph, atoms: &cip::Atoms, a: usize, b: usize) -> bool {
    double_bond_possible(g, a, b, smallest_ring(g, a, b).as_deref())
        && [(a, b), (b, a)].into_iter().all(|(x, y)| {
            let ligs = ligands(g, x, Some(y));
            ligs.len() == 2 && !matches!(cip::rank(atoms, x, &ligs), Err(Unranked::Tie(..)))
        })
}

/// A C=C double bond whose ends both bear two different ligands.
fn double_bond(g: &Graph, atoms: &cip::Atoms, a: usize, b: usize) -> Option<Unit> {
    let ring = smallest_ring(g, a, b);
    if !double_bond_possible(g, a, b, ring.as_deref()) {
        return None;
    }
    let mut ends = Vec::new();
    let mut unranked = None;
    for (x, y) in [(a, b), (b, a)] {
        let ligs = ligands(g, x, Some(y));
        if ligs.len() != 2 {
            return None;
        }
        match cip::rank(atoms, x, &ligs) {
            Ok(order) => ends.push((x, [ligs[order[0]], ligs[order[1]]])),
            Err(Unranked::Tie(..)) => {
                // Alike constitutionally (no stereogenic unit), or only in their own
                // configuration (Rules 3–5).
                let marked = ligs.iter().any(|&l| match l {
                    Ligand::Atom(m) => branch_marked(g, atoms, &[a, b], m),
                    Ligand::H => false,
                });
                if !marked {
                    return None;
                }
                unranked = Some(
                    "the two ligands of an end differ only in their own configuration (CIP \
                     Rules 3–5), which is not supported",
                );
            }
            Err(Unranked::TooLarge) => {
                unranked = Some("the structure is too large to rank the ligands by the CIP rules");
            }
        }
    }
    let descriptor = match unranked {
        Some(why) => Err(why.to_string()),
        None => double_descriptor(g, &ends),
    };
    // The Z double bonds of an eight-membered ring need no descriptor when the ring has more
    // than one (P-91.2.2).
    if descriptor == Ok(Descriptor::Z)
        && ring.is_some_and(|r| r.len() == 8 && ring_doubles(g, &r) > 1)
    {
        return None;
    }
    Some(Unit {
        site: Site::Double(a, b),
        descriptor,
    })
}

/// E or Z for a double bond, its ends each with their two ligands, higher-ranked first.
fn double_descriptor(g: &Graph, ends: &[(usize, [Ligand; 2])]) -> Result<Descriptor, String> {
    let d = &g.drawn;
    let (a, b) = (ends[0].0, ends[1].0);
    if wavy_at(g, a) || wavy_at(g, b) {
        return Err("drawn with a wavy bond: the geometry is not specified".into());
    }
    let inside = || "inside an abbreviation, which is not drawn".to_string();
    let (pa, pb) = (d.pos[a].ok_or_else(inside)?, d.pos[b].ok_or_else(inside)?);
    let axis = unit2(rel(pb, pa)).ok_or_else(inside)?;
    let straight = || "drawn straight: a ligand in line with the double bond".to_string();
    let mut sides = Vec::new();
    for &(x, ligs) in ends {
        let px = d.pos[x].ok_or_else(inside)?;
        // Which side of the line through the double bond each drawn ligand is on.
        let side = |l: Ligand| -> Result<Option<f32>, String> {
            let p = match l {
                Ligand::Atom(m) => d.pos[m],
                Ligand::H => {
                    d.h.iter()
                        .find(|&&(on, p)| on == x && p.is_some())
                        .and_then(|h| h.1)
                }
            };
            let Some(p) = p else { return Ok(None) };
            let v = unit2(rel(p, px)).ok_or_else(inside)?;
            let s = axis[0] * v[1] - axis[1] * v[0];
            if s.abs() < 0.1 {
                return Err(straight());
            }
            Ok(Some(s.signum()))
        };
        let (hi, lo) = (side(ligs[0])?, side(ligs[1])?);
        let s = match (hi, lo) {
            (Some(h), Some(l)) if h == l => {
                return Err("both ligands of an end drawn on one side of the double bond".into());
            }
            (Some(h), _) => h,
            (None, Some(l)) => -l,
            (None, None) => return Err(inside()),
        };
        sides.push(s);
    }
    Ok(if sides[0] == sides[1] {
        Descriptor::Z
    } else {
        Descriptor::E
    })
}

/// An allene C=C=C centred on `m` whose terminal carbons each bear two different ligands: axial
/// chirality, not supported.
fn allene(g: &Graph, atoms: &cip::Atoms, m: usize) -> Option<Unit> {
    let [(a, 2), (b, 2)] = g.adj[m][..] else {
        return None;
    };
    if g.el[m] != El::C || other_double(g, a, m) || other_double(g, b, m) {
        return None;
    }
    for (x, y) in [(a, m), (b, m)] {
        let ligs = ligands(g, x, Some(y));
        if ligs.len() != 2 || cip::rank(atoms, x, &ligs).is_err() {
            return None;
        }
    }
    Some(Unit {
        site: Site::Axis(a, b),
        descriptor: Err("axial chirality of an allene (Ra/Sa) is not supported".into()),
    })
}
