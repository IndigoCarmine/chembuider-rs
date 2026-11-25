use super::*;
use crate::molecule::cdx::{merge_into_cdx, molecule_to_cdx_document, molecule_to_cdxml, read_cdx, read_cdxml};
use crate::molecule::cdx_codec;
use crate::molecule::BondOrder;

/// The user's sketch, away from the page's edge: acetone's C=O (O above C), an H up and to the
/// right, a methoxide down and to the right, a C–Br bond off to the side; with the four kinds
/// of arrow: the S from C=O through O to H, the methoxide's lone pair to the carbonyl carbon, a
/// fish-hook from the C–Br bond onto Br, and one shaped by hand.
fn mechanism() -> (Molecule, Vec<CurlyArrow>) {
    let mut m = Molecule::default();
    let c = m.add_atom("C".into(), [9.0, 9.0], 0);
    let o = m.add_atom("O".into(), [9.0, 7.5], 0);
    let me1 = m.add_atom("C".into(), [7.7, 9.75], 0);
    let me2 = m.add_atom("C".into(), [10.3, 9.75], 0);
    let h = m.add_atom("H".into(), [11.0, 6.2], 0);
    let nu = m.add_atom("O".into(), [12.2, 11.2], -1);
    let me = m.add_atom("C".into(), [13.5, 10.45], 0);
    let cbr_c = m.add_atom("C".into(), [16.0, 8.0], 0);
    let br = m.add_atom("Br".into(), [17.5, 8.0], 0);
    let co = m.add_bond(c, o, BondOrder::Double);
    m.add_bond(c, me1, BondOrder::Single);
    m.add_bond(c, me2, BondOrder::Single);
    m.add_bond(nu, me, BondOrder::Single);
    let cbr = m.add_bond(cbr_c, br, BondOrder::Single);
    let shaped = CurlyArrow {
        path: vec![[0.1, 0.4], [0.4, 0.8], [0.6, 0.6], [0.8, 0.4], [1.1, 0.2]],
        ..CurlyArrow::new(CurlyEnd::Atom(o), CurlyEnd::Between(o, nu), CurlyKind::Pair)
    };
    let arrows = vec![
        CurlyArrow::new(CurlyEnd::Bond { bond: co, onto: o }, CurlyEnd::Atom(h), CurlyKind::Pair),
        CurlyArrow { flip: true, ..CurlyArrow::new(CurlyEnd::Atom(nu), CurlyEnd::Atom(c), CurlyKind::Pair) },
        CurlyArrow::new(CurlyEnd::Bond { bond: cbr, onto: br }, CurlyEnd::Atom(br), CurlyKind::Single),
        shaped,
    ];
    for a in &arrows {
        m.add_curly_arrow(a.clone());
    }
    (m, arrows)
}

fn curlies(m: &Molecule) -> Vec<CurlyArrow> {
    m.curly_arrows().cloned().collect()
}

/// The same arrows: the same roles (by the atoms' positions, as ids change), kind, dots, flip
/// and hand-set shape.
fn same_arrows(a: &Molecule, xs: &[CurlyArrow], b: &Molecule, ys: &[CurlyArrow]) {
    assert_eq!(xs.len(), ys.len());
    let pos = |m: &Molecule, id: u32| m.atom_by_id(id).map(|a| [(a.pos[0] * 100.0).round() as i32, (a.pos[1] * 100.0).round() as i32]);
    let end = |m: &Molecule, e: &CurlyEnd, source: bool| match *e {
        CurlyEnd::Atom(x) => format!("atom {:?}", pos(m, x)),
        CurlyEnd::Bond { bond, onto } => {
            let b = m.bond_by_id(bond).unwrap();
            let mut ends = [pos(m, b.begin), pos(m, b.end)];
            ends.sort();
            if source { format!("bond {ends:?} onto {:?}", pos(m, onto)) } else { format!("bond {ends:?}") }
        }
        CurlyEnd::Between(x, y) => format!("between {:?} {:?}", pos(m, x), pos(m, y)),
        CurlyEnd::Point(p) => format!("point {:.2} {:.2}", p[0], p[1]),
    };
    for (x, y) in xs.iter().zip(ys) {
        assert_eq!((end(a, &x.from, true), end(a, &x.to, false)), (end(b, &y.from, true), end(b, &y.to, false)), "{x:?} vs {y:?}");
        assert_eq!((x.kind, x.dots, x.flip, x.is_shaped()), (y.kind, y.dots, y.flip, y.is_shaped()), "{x:?} vs {y:?}");
        for (p, q) in x.path.iter().zip(&y.path) {
            assert!((p[0] - q[0]).abs() < 1e-3 && (p[1] - q[1]).abs() < 1e-3, "{x:?} vs {y:?}");
        }
    }
}

#[test]
fn curly_arrows_come_back_from_cdx_and_cdxml_with_their_roles() {
    let (m, arrows) = mechanism();
    let from_cdx = read_cdx(&molecule_to_cdx_document(&m)).unwrap();
    same_arrows(&m, &arrows, &from_cdx.molecule, &curlies(&from_cdx.molecule));
    assert_eq!(from_cdx.other_objects, 0, "the graphics go with their arrows");
    let xml = molecule_to_cdxml(&m);
    let from_xml = read_cdxml(&xml).unwrap();
    same_arrows(&m, &arrows, &from_xml.molecule, &curlies(&from_xml.molecule));
    // Saved again, still the same; and drawn as before.
    let again = read_cdx(&molecule_to_cdx_document(&from_cdx.molecule)).unwrap();
    same_arrows(&m, &arrows, &again.molecule, &curlies(&again.molecule));
    // Saved into the document it came from, each object keeps its id.
    let bytes = molecule_to_cdx_document(&m);
    let ids: Vec<Option<u32>> = curlies(&from_cdx.molecule).iter().map(|c| c.source).collect();
    let merged = read_cdx(&merge_into_cdx(&bytes, &from_cdx.molecule)).unwrap();
    same_arrows(&m, &arrows, &merged.molecule, &curlies(&merged.molecule));
    assert_eq!(curlies(&merged.molecule).iter().map(|c| c.source).collect::<Vec<_>>(), ids);
}

#[test]
fn chemdraw_gets_the_drawn_arrows_and_an_inert_tag_with_the_roles() {
    let (m, _) = mechanism();
    let xml = molecule_to_cdxml(&m);
    let read = cdx_file_rs::cdxml::parse_cdxml_with_warnings(&xml).unwrap();
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    assert!(cdx_file_rs::file::CdxFile::from_bytes(&molecule_to_cdx_document(&m)).is_ok());
    // The S and the hand-set shape are curves; the lone pair's and the fish-hook's arcs, arrows.
    assert_eq!(xml.matches("<curve").count(), 2, "{xml}");
    assert_eq!(xml.matches("<arrow").count(), 2, "{xml}");
    for needle in [
        "v=3 source=bond:",
        " onto=",
        "target=atom:",
        "auto=1",
        "source=lonepair:",
        "flip=1",
        "target=between:",
        "auto=0 path=0.1,0.4;",
        "kind=single",
        "ArrowheadHead=\"Full\"",
        "AngularSize=",
        "ArrowSource=",
        "ArrowTarget=",
        "Name=\"ChemBuilder/CurlyArrow\"",
    ] {
        assert!(xml.contains(needle), "{needle}: {xml}");
    }
    assert!(xml.contains("ArrowheadHead=\"HalfLeft\"") || xml.contains("ArrowheadHead=\"HalfRight\""), "{xml}");
    // The S's curve is where the editor draws it.
    let doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
    let curve = doc.children[0].children.iter().find(|o| o.tag == OBJ_CURVE).unwrap();
    let knots = curve_knots(curve).unwrap();
    let drawn = curly::geometry(&m, &curlies(&m)[0]).unwrap();
    let shift = curly::sub(knots[0][1], drawn.tail);
    let end = curly::sub(knots[knots.len() - 1][1], shift);
    assert!(curly::len(curly::sub(end, drawn.head)) < 0.01, "{end:?} {:?}", drawn.head);
}

/// Every tag stripped: what a reader that ignores it (or a ChemDraw that lost it) sees.
fn strip_tags(o: &mut CdxObject) {
    o.children.retain(|c| !cdx_appearance::is_tag_named(c, CURLY_TAG));
    o.children.iter_mut().for_each(strip_tags);
}

#[test]
fn without_the_tag_the_file_shapes_are_kept_and_tied_where_clear() {
    let (m, _) = mechanism();
    let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
    strip_tags(&mut doc);
    let read = read_cdx(&cdx_codec::write(&doc)).unwrap().molecule;
    let a = curlies(&read);
    assert_eq!(a.len(), 4);
    assert!(a.iter().all(|c| c.is_shaped()), "ChemDraw's shapes, kept as they are");
    // The lone pair's arc: ChemDraw's own source and target tie it.
    assert!(matches!(a[1].from, CurlyEnd::Atom(_)) && matches!(a[1].to, CurlyEnd::Atom(_)), "{:?}", a[1]);
    assert_eq!(a[2].kind, CurlyKind::Single);
    // Each is drawn as the file has it.
    let (written, back) = (curly::geometry(&m, &curlies(&m)[0]).unwrap(), curly::geometry(&read, &a[0]).unwrap());
    let shift = curly::sub(back.points[0], written.points[0]);
    for p in [0.25f32, 0.5, 0.75] {
        let q = written.points[(p * (written.points.len() - 1) as f32) as usize];
        let nearest = back.points.iter().map(|b| curly::len(curly::sub(curly::sub(*b, shift), q))).fold(f32::INFINITY, f32::min);
        assert!(nearest < 0.25, "{p}: {nearest}");
    }
}

#[test]
fn a_tag_that_no_longer_fits_the_arrow_is_not_trusted() {
    let (m, _) = mechanism();
    let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
    // ChemDraw moved the lone pair's arrow far away (its tag says it stands on the methoxide).
    let page = &mut doc.children[0];
    let arrow = page.children.iter_mut().find(|o| o.tag == OBJ_ARROW).unwrap();
    for tag in [PROP_TAIL_3D, PROP_HEAD_3D] {
        let p = point_3d(arrow, tag).unwrap();
        arrow.set(tag, point_3d_value((p.0 + 200.0 * PT, p.1 + 200.0 * PT)));
    }
    arrow.props.retain(|(t, _)| *t != PROP_ARROW_SOURCE && *t != PROP_ARROW_TARGET);
    let read = read_cdx(&cdx_codec::write(&doc)).unwrap().molecule;
    let a = &curlies(&read)[1];
    assert!(matches!((a.from, a.to), (CurlyEnd::Point(_), CurlyEnd::Point(_))), "{a:?}");
}

/// `value` (a version 3 tag) as a version 1 or 2 tag says the same: `tail=` and `head=`, the
/// bond's `onto` as its side.
fn older(value: &str, version: u32) -> String {
    let f: HashMap<&str, &str> = value.split_whitespace().filter_map(|x| x.split_once('=')).collect();
    let tail = match f["source"].split_once(':') {
        Some(("lonepair", n)) => format!("atom:{n}"),
        Some(("bond", b)) => format!("bond:{b}:{}", f["onto"]),
        _ => "free".into(),
    };
    let head = match f["target"].split_once(':') {
        Some(("bond", b)) => format!("bond:{b}:0"),
        _ => f["target"].to_string(),
    };
    let mut out = format!("v={version} tail={tail} head={head} angle=120 skew=0 kind={} dots={}", f["kind"], f["dots"]);
    if version >= 2
        && let Some(p) = f.get("path")
    {
        out.push_str(&format!(" path={p}"));
    }
    out
}

#[test]
fn version_1_and_2_tags_read_into_the_roles() {
    let (m, arrows) = mechanism();
    for version in [1, 2] {
        let mut doc = cdx_codec::parse(&molecule_to_cdx_document(&m)).unwrap();
        fn retag(o: &mut CdxObject, version: u32) {
            for c in &mut o.children {
                if cdx_appearance::is_tag_named(c, CURLY_TAG) {
                    let v = cdx_appearance::tag_text(c).unwrap();
                    *c = cdx_appearance::object_tag(c.id, CURLY_TAG, Some(&older(&v, version)));
                }
                retag(c, version);
            }
        }
        retag(&mut doc, version);
        let read = read_cdx(&cdx_codec::write(&doc)).unwrap().molecule;
        let a = curlies(&read);
        // The roles as they were; no flip in those versions, and only version 2 had a path.
        let expect: Vec<CurlyArrow> = arrows
            .iter()
            .map(|x| CurlyArrow { flip: false, path: if version == 2 { x.path.clone() } else { Vec::new() }, ..x.clone() })
            .collect();
        same_arrows(&m, &expect, &read, &a);
    }
}

#[test]
fn a_chemdraw_curved_arrow_reads_as_drawn() {
    // ChemDraw 23 (OrgChemBG's Mg.cdxml): a −30° arrow from a methyl's node to a bond, and
    // ChemDraw 12's 180° arc graphic with no arrow object.
    let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<CDXML><page id="1">
<fragment id="2"><n id="51" p="178.44 138.65"/><n id="52" p="200 150"/><n id="23" p="40 170"/><n id="31" p="70 175"/>
<b id="30" B="51" E="52"/><b id="32" B="23" E="31" Order="2"/></fragment>
<arrow id="69" BoundingBox="57.51 144.68 187.26 166.39" FillType="None" ArrowheadHead="Full" ArrowheadType="Solid" HeadSize="400" ArrowheadCenterSize="400" ArrowheadWidth="200" AngularSize="-30" ArrowSource="51" ArrowTarget="32" Head3D="57.51 164.25 0" Tail3D="186.77 145.17 0" Center3D="86.53 -86.48 0" MajorAxisEnd3D="338.97 -86.48 0" MinorAxisEnd3D="86.53 165.95 0"/>
<graphic id="64" SupersededBy="69" BoundingBox="57.51 164.25 86.53 -86.48" GraphicType="Arc" ArrowType="FullHead" HeadSize="1000" AngularSize="-90"/>
<graphic id="96" BoundingBox="199.02 84 173.15 84" GraphicType="Arc" ArrowType="FullHead" HeadSize="1000" AngularSize="180"/>
</page></CDXML>"#;
    let import = read_cdxml(xml).unwrap();
    let m = &import.molecule;
    let a = curlies(m);
    assert_eq!(a.len(), 2, "{a:?}");
    assert_eq!(import.other_objects, 0);
    assert!(a.iter().all(|c| c.is_shaped()), "drawn as ChemDraw drew them");
    assert!(matches!(a[0].from, CurlyEnd::Atom(x) if m.atom_by_id(x).unwrap().pos[0] > 18.0), "{:?}", a[0]);
    let CurlyEnd::Bond { bond, .. } = a[0].to else { panic!("{:?}", a[0]) };
    assert_eq!(m.bond_by_id(bond).unwrap().order, BondOrder::Double);
    // The lone arc graphic: its tail the head turned back 180° about the centre.
    let CurlyEnd::Point(tail) = a[1].from else { panic!() };
    assert!((tail[0] * 9.6 - 147.28).abs() < 0.05 && (tail[1] * 9.6 - 84.0).abs() < 0.05, "{tail:?}");
    let path = curly::geometry(m, &a[1]).unwrap();
    assert!(path.points.iter().any(|p| (p[1] * 9.6 - 84.0 - 25.9).abs() < 0.5), "bends below, 180°");
    // Saved into the document and read again: the same arrows.
    let bytes = cdx_codec::write(&crate::molecule::cdx::cdxml_document(xml).unwrap());
    let again = read_cdx(&merge_into_cdx(&bytes, m)).unwrap();
    same_arrows(m, &a, &again.molecule, &curlies(&again.molecule));
    assert_eq!(again.other_objects, 0);
}

#[test]
fn a_chemdraw_bezier_arrow_reads_as_a_shaped_curly_arrow() {
    // A curved arrow drawn with ChemDraw's freehand-curve tool, one piece from the lower right
    // up and over to a bromine atom (each knot as control in, knot, control out); a fish-hook
    // curve; and a closed curve with a head, which is no arrow.
    let xml = r#"<?xml version="1.0" encoding="UTF-8" ?>
<CDXML><page id="1">
<fragment id="2"><n id="3" p="100 100" Element="35"/><n id="4" p="114.4 100"/><b id="5" B="3" E="4"/></fragment>
<curve id="10" CurvePoints="140 130 140 130 150 100 120 80 104 94 104 94" FillType="None" ArrowheadHead="Full" ArrowheadType="Solid"/>
<curve id="11" CurvePoints="60 60 60 60 70 50 80 60 80 70 80 70" ArrowheadHead="HalfLeft" ArrowheadType="Solid"/>
<curve id="12" CurveType="1" CurvePoints="10 10 10 10 20 10 20 20 20 20 10 20" ArrowheadHead="Full"/>
</page></CDXML>"#;
    let import = read_cdxml(xml).unwrap();
    let a = curlies(&import.molecule);
    assert_eq!(a.len(), 2, "{a:?}");
    assert_eq!(import.other_objects, 1, "the closed curve stays a shape");
    assert!(a[0].is_shaped() && a[0].kind == CurlyKind::Pair);
    let CurlyEnd::Atom(br) = a[0].to else { panic!("{:?}", a[0]) };
    assert_eq!(import.molecule.atom_by_id(br).unwrap().element, "Br");
    assert!(matches!(a[0].from, CurlyEnd::Point(_)));
    assert_eq!(a[1].kind, CurlyKind::Single);
    let path = curly::geometry(&import.molecule, &a[1]).unwrap();
    let mid = curly::bezier_point(&[[60.0, 60.0], [70.0, 50.0], [80.0, 60.0], [80.0, 70.0]], 0.5);
    let m = path.points[path.points.len() / 2];
    assert!((m[0] * 9.6 - mid[0]).abs() < 0.5 && (m[1] * 9.6 - mid[1]).abs() < 0.5, "{m:?} {mid:?}");
}
