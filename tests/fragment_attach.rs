//! Fragment attachment test: for every fragment the editor ships, attach it to a benzene ring
//! as a pendant substituent and render the result to its own PNG for eyeballing.
//!
//! Run with `cargo test --test fragment_attach -- --nocapture`; the images land in
//! `target/fragment_test/` (one `benzene_<name>.png` per fragment). PNG rendering
//! (`molecule::image`) is Windows-only (resvg), so the image-writing part is gated on Windows;
//! the attachment logic is checked on every platform.
//!
//! **The fragments come from [`Config::embedded`]**, i.e. `assets/fragments/*.json` — the same
//! set the running editor resolves names against. They used to come from a parallel set of
//! hand-written `Fragment` constructors, which is how those constructors sat at a 1.0 bond
//! length for months after the JSON moved to 1.5: this gallery was the only thing looking at
//! them, and it never compared the two. The constructors are gone; what is rendered here is
//! now what the user gets. `ring`/`benzene`/`chain`/`zigzag` stay, because they are formulas
//! rather than coordinate tables and `Config::resolve_fragment` still falls back to the first
//! two for the ring keys.
//!
//! Note on attachment: `Molecule::insert_fragment` *fuses* — it maps the fragment's root
//! (`attach_idx`) onto the target atom, sharing it. That is right for ring fusion / label
//! expansion but wrong for hanging a substituent off a ring carbon (it would embed heteroatoms
//! in the ring and can't place single-atom groups at all). So this test uses `attach_pendant`:
//! the fragment is added whole and its root is *bonded* to the ring carbon, giving a chemically
//! sensible fluorobenzene / phenol / aniline / biphenyl / … for every fragment.

use chembuider_rs::molecule::fragment::Fragment;
use chembuider_rs::{BondOrder, Config, Molecule};

/// Materialize a standalone `Fragment` (e.g. benzene) as its own `Molecule`.
fn fragment_to_molecule(frag: &Fragment) -> Molecule {
    let mut mol = Molecule::default();
    mol.name = frag.name.clone();
    let ids: Vec<u32> = frag
        .atoms
        .iter()
        .map(|a| mol.add_atom(a.element.clone(), a.pos, a.charge))
        .collect();
    for b in &frag.bonds {
        mol.add_bond(ids[b.begin], ids[b.end], b.order.clone());
    }
    mol
}

/// Centroid of all atoms.
fn centroid(mol: &Molecule) -> [f32; 2] {
    let n = mol.atoms.len().max(1) as f32;
    let (sx, sy) = mol
        .atoms
        .iter()
        .fold((0.0, 0.0), |(sx, sy), a| (sx + a.pos[0], sy + a.pos[1]));
    [sx / n, sy / n]
}

/// Add `frag` to `mol` as a pendant substituent bonded to `attach_to`, pointing along `angle`.
///
/// Unlike `insert_fragment`, the fragment's root atom is *not* merged into `attach_to`: the whole
/// fragment is placed one bond-length out along `angle` (rotated so its body points outward), then
/// a single bond joins `attach_to` to the fragment's root. Works for single- and multi-atom
/// fragments alike. Returns the id of the fragment's root atom.
fn attach_pendant(mol: &mut Molecule, frag: &Fragment, attach_to: u32, angle: f32) -> u32 {
    const BOND: f32 = 1.5; // standard bond length, matching the ring edges
    let base = mol.atom_by_id(attach_to).expect("attach atom exists").pos;
    let root_raw = frag.atoms[frag.attach_idx].pos;

    // The fragment's local "outward" axis: from the root toward the centroid of its other atoms
    // (for a lone-atom fragment there is none, so fall back to +x).
    let others = frag.atoms.len().saturating_sub(1).max(1) as f32;
    let (mut lx, mut ly) = (0.0f32, 0.0f32);
    for (i, a) in frag.atoms.iter().enumerate() {
        if i == frag.attach_idx {
            continue;
        }
        lx += a.pos[0] - root_raw[0];
        ly += a.pos[1] - root_raw[1];
    }
    let (lx, ly) = (lx / others, ly / others);
    let local_angle = if lx.hypot(ly) < 1e-6 { 0.0 } else { ly.atan2(lx) };

    // Rotation that turns the local axis to point along `angle`, then translate the root to sit
    // one bond out from the attach atom.
    let rot = angle - local_angle;
    let (s, c) = rot.sin_cos();
    let (dx, dy) = (angle.cos(), angle.sin());
    let root_pos = [base[0] + BOND * dx, base[1] + BOND * dy];

    let ids: Vec<u32> = frag
        .atoms
        .iter()
        .map(|a| {
            let (ox, oy) = (a.pos[0] - root_raw[0], a.pos[1] - root_raw[1]);
            let pos = [root_pos[0] + ox * c - oy * s, root_pos[1] + ox * s + oy * c];
            mol.add_atom(a.element.clone(), pos, a.charge)
        })
        .collect();
    for b in &frag.bonds {
        mol.add_bond(ids[b.begin], ids[b.end], b.order.clone());
    }
    // Pendant bond: ring carbon → fragment root.
    mol.add_bond(attach_to, ids[frag.attach_idx], BondOrder::Single);
    ids[frag.attach_idx]
}

/// Build a fresh benzene and hang a single `frag` off its first ring carbon.
fn benzene_with(frag: &Fragment) -> Molecule {
    let mut mol = fragment_to_molecule(&Fragment::benzene());
    let center = centroid(&mol);
    let ring_id = mol.atoms[0].id;
    let pos = mol.atom_by_id(ring_id).unwrap().pos;
    let angle = (pos[1] - center[1]).atan2(pos[0] - center[0]); // radially outward
    attach_pendant(&mut mol, frag, ring_id, angle);
    mol
}

/// Every fragment the editor can resolve, paired with the file-name stem for its rendered
/// image: the shipped JSON set plus the parametric constructors that back the ring keys.
fn all_fragments() -> Vec<(String, Fragment)> {
    let mut out: Vec<(String, Fragment)> = vec![
        ("ring6".into(), Fragment::ring(6)),
        ("chain3".into(), Fragment::chain(3)),
        ("zigzag3".into(), Fragment::zigzag(3)),
    ];
    out.extend(
        Config::embedded()
            .fragments
            .iter()
            .map(|def| (def.name.clone(), def.to_fragment())),
    );
    out
}

#[test]
fn attaches_every_fragment_to_benzene() {
    let frags = all_fragments();
    assert!(
        frags.len() > 40,
        "the gallery should cover the whole shipped fragment set, got {}",
        frags.len(),
    );

    for (name, frag) in &frags {
        let before = 6; // bare benzene
        let mol = benzene_with(frag);

        // The pendant added the whole fragment plus, for multi-atom fragments, kept the ring
        // intact — so the molecule always grew by exactly the fragment's atom count.
        assert_eq!(
            mol.atoms.len(),
            before + frag.atoms.len(),
            "{name}: benzene(6) + {} fragment atoms",
            frag.atoms.len()
        );
        // Benzene's three aromatic double bonds must survive (ring atom ids are 1..=6).
        let ring_doubles = mol
            .bonds
            .iter()
            .filter(|b| b.order == BondOrder::Double && b.begin <= 6 && b.end <= 6)
            .count();
        assert_eq!(ring_doubles, 3, "{name}: aromatic ring keeps three double bonds");

        render_png(name, &mol);
    }
}

/// Render `mol` to `target/fragment_test/benzene_<name>.png`.
#[cfg(windows)]
fn render_png(name: &str, mol: &Molecule) {
    use chembuider_rs::molecule::image::molecule_to_png;

    let png = molecule_to_png(mol).expect("PNG rendering should succeed for a non-empty molecule");
    assert_eq!(
        &png[0..8],
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        "{name}: valid PNG signature"
    );

    let dir = std::path::Path::new("target/fragment_test");
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("benzene_{name}.png"));
    std::fs::write(&path, &png).unwrap();
    println!("wrote {} ({} bytes)", path.display(), png.len());
}

#[cfg(not(windows))]
fn render_png(_name: &str, _mol: &Molecule) {}
