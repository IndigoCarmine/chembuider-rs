use super::*;
use crate::molecule::BondOrder;

fn close(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

/// The user's sketch: acetone's C=O (O above C), an H up and to the right of O, and a
/// methoxide O⁻ down and to the right.
struct Sketch {
    m: Molecule,
    c: u32,
    o: u32,
    h: u32,
    nu: u32,
    co: u32,
}

fn sketch() -> Sketch {
    let mut m = Molecule::default();
    let c = m.add_atom("C".into(), [0.0, 0.0], 0);
    let o = m.add_atom("O".into(), [0.0, -1.5], 0);
    let me1 = m.add_atom("C".into(), [-1.3, 0.75], 0);
    let me2 = m.add_atom("C".into(), [1.3, 0.75], 0);
    let co = m.add_bond(c, o, BondOrder::Double);
    m.add_bond(c, me1, BondOrder::Single);
    m.add_bond(c, me2, BondOrder::Single);
    let h = m.add_atom("H".into(), [2.0, -2.8], 0);
    let nu = m.add_atom("O".into(), [3.2, 2.2], -1);
    let me = m.add_atom("C".into(), [4.5, 1.45], 0);
    m.add_bond(nu, me, BondOrder::Single);
    Sketch { m, c, o, h, nu, co }
}

/// What the curve runs into: the nearest it comes to an atom that is not its own (over its
/// whole length), and how many bonds it crosses (bonds' ends aside).
fn clearance(m: &Molecule, arrow: &CurlyArrow, path: &CurlyPath) -> (f32, usize) {
    let own = arrow.atoms(m);
    let nearest = m
        .atoms
        .iter()
        .filter(|a| !own.contains(&a.id))
        .flat_map(|a| path.points.iter().map(move |p| len(sub(*p, a.pos))))
        .fold(f32::INFINITY, f32::min);
    let crossings = m
        .bonds
        .iter()
        .map(|b| {
            let (p, q) = (m.atom_by_id(b.begin).unwrap().pos, m.atom_by_id(b.end).unwrap().pos);
            let d = sub(q, p);
            let (a, c) = (add_scaled(p, d, 0.1), add_scaled(p, d, 0.9));
            path.points.windows(2).filter(|w| segments_cross(w[0], w[1], a, c)).count()
        })
        .sum();
    (nearest, crossings)
}

/// The way the curve ends: the direction of its last stretch.
fn arriving(path: &CurlyPath) -> [f32; 2] {
    let n = path.points.len();
    unit(sub(path.points[n - 1], path.points[n - 4])).unwrap()
}

/// The way the curve leaves its tail, by its first cubic piece (tail to first control point),
/// checked against the drawn curve's first step.
fn leaving(path: &CurlyPath) -> [f32; 2] {
    let b = path.pieces[0];
    assert!(close(b[0], path.tail), "{:?} {:?}", b[0], path.tail);
    let d = unit(sub(b[1], b[0])).unwrap();
    let step = unit(sub(path.points[1], path.points[0])).unwrap();
    assert!(dot(d, step) > 0.995, "{d:?} {step:?}");
    d
}

/// The way the curve comes into its head, by its last cubic piece (last control point to head).
fn coming_in(path: &CurlyPath) -> [f32; 2] {
    let b = path.pieces[path.pieces.len() - 1];
    assert!(close(b[3], path.head), "{:?} {:?}", b[3], path.head);
    unit(sub(b[3], b[2])).unwrap()
}

/// The unit direction of `bond`, and its middle.
fn bond_line(m: &Molecule, bond: u32) -> ([f32; 2], [f32; 2]) {
    let b = m.bond_by_id(bond).unwrap();
    let (p, q) = (m.atom_by_id(b.begin).unwrap().pos, m.atom_by_id(b.end).unwrap().pos);
    (unit(sub(q, p)).unwrap(), midpoint(p, q))
}

/// `arrow`, laid out as it is and flipped, leaves its source bond square to it, each on its own
/// side, its tail off the bond on the side it leaves to; and returns the two.
fn square_on_both_sides(m: &Molecule, arrow: &CurlyArrow) -> (CurlyPath, CurlyPath) {
    let CurlyEnd::Bond { bond, .. } = arrow.from else { panic!("{arrow:?}") };
    let (along, mid) = bond_line(m, bond);
    let flipped = CurlyArrow { flip: true, ..arrow.clone() };
    let (a, b) = (layout(m, arrow).unwrap(), layout(m, &flipped).unwrap());
    let mut sides = Vec::new();
    for path in [&a, &b] {
        let d = leaving(path);
        assert!(dot(d, along).abs() < 0.02, "square to the bond: {d:?} vs {along:?}");
        let off = sub(path.tail, mid);
        assert!(dot(off, d) > 0.1, "the tail stands off the bond on the side it leaves to: {off:?} {d:?}");
        sides.push(d);
    }
    assert!(dot(sides[0], sides[1]) < -0.99, "flipped, the other side: {sides:?}");
    (a, b)
}

#[test]
fn an_arrow_leaves_its_bond_square_to_it_arc_s_or_curve() {
    let s = sketch();
    // The S from C=O through O to H, round either side of O.
    let via = CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.o }, CurlyEnd::Atom(s.h), CurlyKind::Pair);
    let (a, b) = square_on_both_sides(&s.m, &via);
    assert!(a.arc.is_none() && b.arc.is_none() && a.pieces.len() == 2, "the S");
    // The C=O's π electrons onto O, and onto C: arcs, coming down beside the atom.
    for onto in [s.o, s.c] {
        let own = CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto }, CurlyEnd::Atom(onto), CurlyKind::Pair);
        let (a, b) = square_on_both_sides(&s.m, &own);
        assert!(a.arc.is_some() && b.arc.is_some(), "{:?} {:?}", a.arc, b.arc);
    }
    // C=O to the methoxide's O, which is no atom of it: an arc square to the bond.
    let far = CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.c }, CurlyEnd::Atom(s.nu), CurlyKind::Pair);
    square_on_both_sides(&s.m, &far);
    // To a free point straight out from the bond: not straight, but curved off it.
    let (along, mid) = bond_line(&s.m, s.co);
    let ahead = add_scaled(mid, right_of(along), 3.0);
    let out = CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.c }, CurlyEnd::Point(ahead), CurlyKind::Pair);
    let (a, _) = square_on_both_sides(&s.m, &out);
    assert!(turning(&a.points) > 45.0, "curly, not straight: {}", turning(&a.points));
}

#[test]
fn heterolysis_leaves_square_and_stays_a_chemdraw_arc() {
    let mut m = Molecule::default();
    let c = m.add_atom("C".into(), [0.0, 0.0], 0);
    let br = m.add_atom("Br".into(), [1.5, 0.0], 0);
    m.add_atom("C".into(), [-0.75, 1.3], 0);
    let cbr = m.add_bond(c, br, BondOrder::Single);
    let arrow = CurlyArrow::new(CurlyEnd::Bond { bond: cbr, onto: br }, CurlyEnd::Atom(br), CurlyKind::Single);
    let (a, b) = square_on_both_sides(&m, &arrow);
    for path in [&a, &b] {
        assert!(path.arc.is_some_and(|t| t.abs() > 60.0 && t.abs() < 180.0), "{:?}", path.arc);
        // Down beside the bromine, on the side it left the bond to.
        assert!(len(sub(path.head, [1.5, 0.0])) < 1.0 && path.head[1] * path.tail[1] > 0.0, "{:?} {:?}", path.tail, path.head);
    }
}

#[test]
fn an_arrow_comes_square_into_a_bond_or_a_gap() {
    // The enolate again: O⁻–C1=C2, C2–C3; and a Br above C3.
    let mut m = Molecule::default();
    let o = m.add_atom("O".into(), [0.0, -1.5], -1);
    let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
    let c2 = m.add_atom("C".into(), [1.3, 0.75], 0);
    let c3 = m.add_atom("C".into(), [2.6, 0.0], 0);
    let br = m.add_atom("Br".into(), [3.6, -2.0], 0);
    let oc = m.add_bond(o, c1, BondOrder::Single);
    let cc = m.add_bond(c1, c2, BondOrder::Double);
    let c23 = m.add_bond(c2, c3, BondOrder::Single);
    let into = |path: &CurlyPath, line: [f32; 2], mid: [f32; 2]| {
        let d = coming_in(path);
        assert!(dot(d, line).abs() < 0.02, "square into it: {d:?} vs {line:?}");
        assert!(dot(sub(mid, path.head), d) > 0.05, "short of its middle, pointing at it: {:?}", path.head);
    };
    // A lone pair into a bond: an arc.
    let lp = CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Bond { bond: oc, onto: o }, CurlyKind::Pair);
    let path = layout(&m, &lp).unwrap();
    let (line, mid) = bond_line(&m, oc);
    into(&path, line, mid);
    assert!(path.arc.is_some() && path.dots.is_some());
    // A bond into a bond, square at both ends, on either side.
    let shift = CurlyArrow::new(CurlyEnd::Bond { bond: cc, onto: c2 }, CurlyEnd::Bond { bond: c23, onto: c2 }, CurlyKind::Pair);
    let (a, b) = square_on_both_sides(&m, &shift);
    let (line, mid) = bond_line(&m, c23);
    into(&a, line, mid);
    into(&b, line, mid);
    let (_, crossings) = clearance(&m, &shift, &a);
    assert_eq!(crossings, 0);
    // A bond into the gap where a bond forms.
    let gap = CurlyArrow::new(CurlyEnd::Bond { bond: c23, onto: c3 }, CurlyEnd::Between(c3, br), CurlyKind::Pair);
    let (a, _) = square_on_both_sides(&m, &gap);
    let (p, q) = (m.atom_by_id(c3).unwrap().pos, m.atom_by_id(br).unwrap().pos);
    into(&a, unit(sub(q, p)).unwrap(), midpoint(p, q));
}

#[test]
fn a_tail_on_a_double_bond_starts_clear_of_its_lines() {
    let s = sketch();
    let (along, mid) = bond_line(&s.m, s.co);
    for flip in [false, true] {
        let arrow = CurlyArrow { flip, ..CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.o }, CurlyEnd::Atom(s.h), CurlyKind::Pair) };
        let path = layout(&s.m, &arrow).unwrap();
        // The C=O is drawn as two lines either side of its axis.
        let off = dot(sub(path.tail, mid), right_of(along)).abs();
        assert!(off > BOND_LINE_SPACING / 2.0 + 0.08, "{off}");
    }
}

#[test]
fn a_hand_set_shape_keeps_its_own_way_off_the_bond() {
    let s = sketch();
    // Leaving the C=O at a slant, as drawn by hand.
    let arrow = CurlyArrow { path: vec![[0.5, -0.1], [0.9, -0.3]], ..CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.o }, CurlyEnd::Atom(s.h), CurlyKind::Pair) };
    let before = arrow.path.clone();
    let path = geometry(&s.m, &arrow).unwrap();
    assert_eq!(arrow.path, before);
    let (along, _) = bond_line(&s.m, s.co);
    assert!(dot(leaving(&path), along).abs() > 0.2, "not squared: {:?}", leaving(&path));
    let (at, ah) = anchors(&s.m, &arrow).unwrap();
    let whole = sample(&pieces_between(at, ah, &arrow.path));
    let nearest = whole.iter().map(|p| len(sub(*p, path.tail))).fold(f32::INFINITY, f32::min);
    assert!(nearest < 0.02, "its tail is on the curve set by hand: {nearest}");
}

#[test]
fn a_bond_carried_by_its_atom_to_a_partner_is_an_s_round_that_atom() {
    let s = sketch();
    let arrow = CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.o }, CurlyEnd::Atom(s.h), CurlyKind::Pair);
    assert_eq!(arrow.case(&s.m), CurlyCase::BondViaAtomToPartner);
    assert_eq!(describe(&s.m, &arrow), "C=O → (O) → H");
    let path = layout(&s.m, &arrow).unwrap();
    // It starts just off the bond's middle, clear of its lines, on the side the H is...
    assert!(len(sub(path.tail, [0.0, -0.75])) < 0.3 && path.tail[0] > 0.0, "{:?}", path.tail);
    // ... leaving it square, across the bond...
    assert!(leaving(&path)[0] > 0.99, "{:?}", leaving(&path));
    // ... then past O on the outside, to its right, clear of its label...
    let o = s.m.atom_by_id(s.o).unwrap().pos;
    let beside = path.points.iter().filter(|p| (p[1] - o[1]).abs() < 0.3).map(|p| p[0] - o[0]).fold(f32::NEG_INFINITY, f32::max);
    assert!(beside > 0.5, "passes O on its right: {beside}");
    assert!(path.points.iter().all(|p| len(sub(*p, o)) > 0.4), "outside O's label");
    // ... into the H along the O···H bond being made, its head short of H.
    let h = s.m.atom_by_id(s.h).unwrap().pos;
    let along_oh = unit(sub(h, o)).unwrap();
    assert!(dot(arriving(&path), along_oh) > 0.8, "{:?}", arriving(&path));
    let short = len(sub(h, path.head));
    assert!(short > 0.3 && short < 0.7, "{short}");
    let (nearest, crossings) = clearance(&s.m, &arrow, &path);
    assert!(nearest > 0.4 && crossings == 0, "{nearest} {crossings}");
    assert!(path.pieces.len() == 2 && path.arc.is_none());
}

#[test]
fn a_lone_pair_to_an_atom_is_an_arc_from_dots_facing_the_target() {
    let s = sketch();
    let arrow = CurlyArrow::new(CurlyEnd::Atom(s.nu), CurlyEnd::Atom(s.c), CurlyKind::Pair);
    assert_eq!(arrow.case(&s.m), CurlyCase::LonePairToAtom);
    assert_eq!(describe(&s.m, &arrow), "O: → C");
    let path = layout(&s.m, &arrow).unwrap();
    assert!(path.arc.is_some());
    let (nu, c) = (s.m.atom_by_id(s.nu).unwrap().pos, s.m.atom_by_id(s.c).unwrap().pos);
    let (dots, _) = path.dots.expect("its lone pair");
    assert!(dot(sub(dots, nu), sub(c, nu)) > 0.0, "the dots face the carbon");
    assert!(len(sub(path.head, c)) < 0.5);
    let (nearest, crossings) = clearance(&s.m, &arrow, &path);
    assert!(nearest > 0.3 && crossings == 0, "{nearest} {crossings}");
}

#[test]
fn a_bond_onto_its_own_atom_is_a_short_arc_from_its_middle() {
    let mut m = Molecule::default();
    let c = m.add_atom("C".into(), [0.0, 0.0], 0);
    let br = m.add_atom("Br".into(), [1.5, 0.0], 0);
    let h1 = m.add_atom("C".into(), [-0.75, 1.3], 0);
    let cbr = m.add_bond(c, br, BondOrder::Single);
    m.add_bond(c, h1, BondOrder::Single);
    let arrow = CurlyArrow::new(CurlyEnd::Bond { bond: cbr, onto: br }, CurlyEnd::Atom(br), CurlyKind::Pair);
    assert_eq!(arrow.case(&m), CurlyCase::BondOntoOwnAtom);
    assert_eq!(describe(&m, &arrow), "C–Br → (Br)");
    let path = layout(&m, &arrow).unwrap();
    assert!(len(sub(path.tail, [0.75, 0.0])) < 0.2, "{:?}", path.tail);
    assert!(len(sub(path.head, [1.5, 0.0])) < 1.0, "beside the bromine: {:?}", path.head);
    let (_, crossings) = clearance(&m, &arrow, &path);
    assert_eq!(crossings, 0);
}

#[test]
fn a_lone_pair_into_a_bond_and_a_bond_into_a_bond_end_at_the_bonds_middle() {
    // An enolate: O⁻–C1=C2, C2–C3; the O's lone pair into O–C1, the C1=C2 π bond onto C2.
    let mut m = Molecule::default();
    let o = m.add_atom("O".into(), [0.0, -1.5], -1);
    let c1 = m.add_atom("C".into(), [0.0, 0.0], 0);
    let c2 = m.add_atom("C".into(), [1.3, 0.75], 0);
    let c3 = m.add_atom("C".into(), [2.6, 0.0], 0);
    let oc = m.add_bond(o, c1, BondOrder::Single);
    let cc = m.add_bond(c1, c2, BondOrder::Double);
    let c23 = m.add_bond(c2, c3, BondOrder::Single);
    let lp = CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Bond { bond: oc, onto: o }, CurlyKind::Pair);
    assert_eq!(lp.case(&m), CurlyCase::LonePairToBond);
    let path = layout(&m, &lp).unwrap();
    assert!(len(sub(path.head, [0.0, -0.75])) < 0.3, "{:?}", path.head);
    let shift = CurlyArrow::new(CurlyEnd::Bond { bond: cc, onto: c2 }, CurlyEnd::Bond { bond: c23, onto: c2 }, CurlyKind::Pair);
    assert_eq!(shift.case(&m), CurlyCase::BondToBond);
    let path = layout(&m, &shift).unwrap();
    assert!(len(sub(path.head, [1.95, 0.375])) < 0.3, "{:?}", path.head);
    let (_, crossings) = clearance(&m, &shift, &path);
    assert_eq!(crossings, 0);
}

#[test]
fn the_layout_follows_the_atoms_and_flips_to_the_other_side() {
    let mut s = sketch();
    let arrow = CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.o }, CurlyEnd::Atom(s.h), CurlyKind::Pair);
    let before = layout(&s.m, &arrow).unwrap();
    for a in &mut s.m.atoms {
        a.pos = [a.pos[0] + 2.0, a.pos[1] + 1.0];
    }
    let after = layout(&s.m, &arrow).unwrap();
    for (p, q) in before.points.iter().zip(&after.points) {
        assert!(close(*q, [p[0] + 2.0, p[1] + 1.0]), "{p:?} {q:?}");
    }
    // Flipped, it goes round O on its other side.
    let mut flipped = arrow.clone();
    flipped.flip();
    assert!(flipped.flip && !flipped.is_shaped());
    let other = layout(&s.m, &flipped).unwrap();
    let o = s.m.atom_by_id(s.o).unwrap().pos;
    assert!(other.points.iter().any(|p| p[0] < o[0] - 0.4), "round the left of O");
}

#[test]
fn a_hand_set_shape_overrides_the_layout_until_it_is_cleared() {
    let s = sketch();
    let mut arrow = CurlyArrow::new(CurlyEnd::Atom(s.nu), CurlyEnd::Atom(s.c), CurlyKind::Pair);
    let auto = geometry(&s.m, &arrow).unwrap();
    let h = handles(&s.m, &arrow).unwrap();
    assert_eq!(h.controls.len(), 2);
    // Taking the layout's shape changes nothing much...
    let mut same = arrow.clone();
    assert!(shape_from_layout(&s.m, &mut same) && same.is_shaped());
    let kept = geometry(&s.m, &same).unwrap();
    assert!(len(sub(kept.middle(), auto.middle())) < 0.15, "{:?} {:?}", kept.middle(), auto.middle());
    // ... dragging a control point shapes it by hand...
    let (index, at, _) = h.controls[0];
    assert!(move_path_point(&s.m, &mut arrow, index, [at[0] - 1.0, at[1] + 1.0]));
    assert!(arrow.is_shaped());
    assert!(close(handles(&s.m, &arrow).unwrap().controls[0].1, [at[0] - 1.0, at[1] + 1.0]) || arrow.dots);
    // ... and clearing the path gives the layout back.
    arrow.path.clear();
    assert_eq!(geometry(&s.m, &arrow).unwrap(), auto);
}

#[test]
fn arrows_saved_before_roles_had_names_still_read() {
    // The JSON of a first-round arrow: tail / head, a bond's `side`, an angle and a skew.
    let json = r#"{"id":7,"tail":{"Bond":{"bond":4,"side":2}},"head":{"Atom":9},"angle":-120.0,"skew":0.3,"kind":"Single","dots":false,"source":null}"#;
    let a: CurlyArrow = serde_json::from_str(json).unwrap();
    assert_eq!((a.from, a.to, a.kind), (CurlyEnd::Bond { bond: 4, onto: 2 }, CurlyEnd::Atom(9), CurlyKind::Single));
    assert!(!a.is_shaped(), "an arc then: drawn automatically now");
    let json = r#"{"id":7,"tail":{"Atom":1},"head":{"Atom":2},"angle":90.0,"path":[[0.3,0.6],[0.6,-0.6]]}"#;
    let a: CurlyArrow = serde_json::from_str(json).unwrap();
    assert!(a.is_shaped() && !a.dots);
    // And what is written now reads back the same.
    let back: CurlyArrow = serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(back, a);
}

#[test]
fn the_frame_turns_and_stretches_with_its_ends() {
    let p = [0.3, -0.4];
    let abs = from_frame([1.0, 1.0], [3.0, 1.0], p);
    assert!(close(abs, [1.6, 0.2]), "right of a line going right is down: {abs:?}");
    assert!(close(to_frame([1.0, 1.0], [3.0, 1.0], abs).unwrap(), p));
    let turned = from_frame([0.0, 0.0], [0.0, 4.0], p);
    assert!(close(turned, [1.6, 1.2]), "{turned:?}");
}

#[test]
fn the_arc_turns_the_way_chemdraw_counts_and_its_cubics_follow_it() {
    // ChemDraw's 180° arrow from left to right bends below (y down); -180° above.
    let (p, q) = ([0.0, 0.0], [2.0, 0.0]);
    assert!(close(curve_point(p, q, 180.0, 0.5), [1.0, 1.0]));
    assert!(close(curve_point(p, q, -180.0, 0.5), [1.0, -1.0]));
    for angle in [60.0f32, -120.0, 200.0] {
        let pieces = arc_pieces(p, q, angle);
        assert_eq!(pieces.len(), if angle > 150.0 { 2 } else { 1 });
        let half = (angle.abs() / 2.0).to_radians();
        let radius = 1.0 / half.sin();
        let apex = curve_point(p, q, angle, 0.5);
        let centre = [1.0, apex[1] - apex[1].signum() * radius];
        for b in &pieces {
            for k in 0..=8 {
                let x = bezier_point(b, k as f32 / 8.0);
                assert!((len(sub(x, centre)) - radius).abs() < 0.01 * radius, "{angle}: {x:?}");
            }
        }
    }
}

#[test]
fn knots_split_without_changing_the_shape_and_join_again() {
    let m = Molecule::default();
    let (at, ah) = ([0.0, 0.0], [3.0, 0.0]);
    let mut a = CurlyArrow::new(CurlyEnd::Point(at), CurlyEnd::Point(ah), CurlyKind::Pair);
    a.path = vec![[0.3, 0.8], [0.7, -0.8]];
    let before = geometry(&m, &a).unwrap();
    let k = insert_knot(&m, &mut a, before.middle()).unwrap();
    assert_eq!((k, a.path.len()), (2, 5));
    let after = geometry(&m, &a).unwrap();
    for p in [0.1, 0.33, 0.5, 0.8] {
        let i = (p * (before.points.len() - 1) as f32) as usize;
        let nearest = after.points.iter().map(|q| len(sub(*q, before.points[i]))).fold(f32::INFINITY, f32::min);
        assert!(nearest < 0.03, "{p}: {nearest}");
    }
    let knot = from_frame(at, ah, a.path[2]);
    let (c2, c1) = (from_frame(at, ah, a.path[1]), from_frame(at, ah, a.path[3]));
    assert!(move_path_point(&m, &mut a, 2, [knot[0], knot[1] + 1.0]));
    assert!(close(from_frame(at, ah, a.path[1]), [c2[0], c2[1] + 1.0]) && close(from_frame(at, ah, a.path[3]), [c1[0], c1[1] + 1.0]));
    let k = from_frame(at, ah, a.path[2]);
    assert!(move_path_point(&m, &mut a, 1, [k[0] - 1.0, k[1]]));
    let other = from_frame(at, ah, a.path[3]);
    assert!(other[0] > k[0] && (other[1] - k[1]).abs() < 1e-4, "smooth through the knot: {other:?}");
    assert!(remove_knot(&mut a, 2));
    assert_eq!(a.path.len(), 2);
    assert!(!remove_knot(&mut a, 1));
}

#[test]
fn a_label_reaches_as_far_as_its_letters() {
    let mut s = sketch();
    let up = label_reach(&s.m, s.o, [0.0, -1.0]).unwrap();
    assert!((up - LABEL_HALF_HEIGHT).abs() < 1e-4);
    let side = label_reach(&s.m, s.o, [1.0, 0.0]).unwrap();
    assert!(side > 0.3 && side < 0.5, "half an O: {side}");
    s.m.atom_by_id_mut(s.o).unwrap().element = "Br".into();
    assert!(label_reach(&s.m, s.o, [1.0, 0.0]).unwrap() > side);
    assert_eq!(label_reach(&s.m, s.c, [1.0, 0.0]), None, "a bare carbon has none");
}

#[test]
fn deleting_an_atom_frees_the_ends_on_it_where_they_were() {
    let mut s = sketch();
    let id = s.m.add_curly_arrow(CurlyArrow::new(CurlyEnd::Bond { bond: s.co, onto: s.o }, CurlyEnd::Atom(s.h), CurlyKind::Single));
    let before = geometry(&s.m, s.m.object_by_id(id).unwrap().as_curly().unwrap()).unwrap();
    s.m.remove_atom(s.h);
    let a = s.m.object_by_id(id).unwrap().as_curly().unwrap();
    assert!(matches!(a.from, CurlyEnd::Bond { .. }));
    assert!(matches!(a.to, CurlyEnd::Point(p) if close(p, before.head)), "{a:?}");
    s.m.remove_bond(s.co);
    let a = s.m.object_by_id(id).unwrap().as_curly().unwrap();
    assert!(matches!(a.from, CurlyEnd::Point(_)), "{a:?}");
    assert!(geometry(&s.m, a).is_some());
}

#[test]
fn ends_are_tied_only_when_that_is_clear() {
    let s = sketch();
    assert_eq!(end_near(&s.m, [0.05, -0.75], true), Some(CurlyEnd::Bond { bond: s.co, onto: s.c }));
    assert_eq!(end_near(&s.m, [-0.6, -1.9], true), Some(CurlyEnd::Atom(s.o)));
    assert_eq!(end_near(&s.m, [8.0, 8.0], false), None);
}
