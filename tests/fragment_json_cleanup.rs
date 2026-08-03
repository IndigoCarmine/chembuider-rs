//! Checks on the shipped fragment JSON: that the two copies of the set agree, that their bond
//! lengths are the one the editor draws at, and that they survive a cleanup round-trip.
//!
//! The round-trip is JSON → `Fragment` → `Molecule` → `cleanup_2d` → `Fragment` → JSON, since
//! `cleanup_2d` operates on a `Molecule`. Cleanup only relaxes atom positions; topology (atoms,
//! bonds, attach point) must survive unchanged. Cleaned copies are written to
//! `target/fragment_test/cleaned/<name>.json`.
//!
//! Run with `cargo test --test fragment_json_cleanup -- --nocapture`.

use chembuider_rs::molecule::cleanup::cleanup_2d;
use chembuider_rs::molecule::fragment::Fragment;
use chembuider_rs::Config;

/// Every `fragments/<name>.json` must also be embedded (`assets/fragments/` +
/// `BUILTIN_FRAGMENT_STRS` in `src/config.rs`), or it silently disappears on the web build —
/// `fragments/` is read from disk at runtime, which doesn't exist in a browser (see
/// `Config::load`/`load_fragments_from_dir`). Catches a fragment saved via the app's "Save
/// Fragment" button but never promoted to the embedded set.
#[test]
fn every_fragment_on_disk_is_embedded() {
    let embedded_names: std::collections::HashSet<String> = Config::embedded()
        .fragments
        .into_iter()
        .map(|f| f.name)
        .collect();

    let mut missing = Vec::new();
    for entry in std::fs::read_dir("fragments").expect("run from the repo root; `fragments/` must exist") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        if !embedded_names.contains(&name) {
            missing.push(name);
        }
    }

    assert!(
        missing.is_empty(),
        "fragments/ has JSON files not embedded for the web build (copy into assets/fragments/ \
         and add an include_str! line in src/config.rs's BUILTIN_FRAGMENT_STRS): {missing:?}"
    );
}

/// Present under the same name is not enough — the two copies must be the same *fragment*.
///
/// `fragments/` and `assets/fragments/` are two spellings of one set: the first is read from
/// disk and wins at runtime (`Config::load`), the second is compiled in by `include_str!` and is
/// all the web build and every test has. When they disagree, the geometry the tests check is not
/// the geometry the user gets, and there is nothing to notice it — which is exactly what
/// happened. `Ac` and `BPin` were edited through the app's fragment editor, which writes to
/// `fragments/` only; the commit that promoted the *new* fragments to `assets/` copied the files
/// that were missing and left the two that had merely *changed*, so the embedded BPin stayed a
/// version behind (nine atoms rooted on the boron, against the ten-atom form with an explicit
/// attachment carbon that every other editor-authored fragment uses).
///
/// Direction of truth: `fragments/` is where the fragment editor saves, so it is the newer copy
/// and `assets/` is the one to bring forward.
#[test]
fn every_fragment_on_disk_matches_its_embedded_copy() {
    let embedded: std::collections::HashMap<String, chembuider_rs::config::FragmentDef> =
        Config::embedded().fragments.into_iter().map(|f| (f.name.clone(), f)).collect();

    let mut differ = Vec::new();
    for entry in std::fs::read_dir("fragments").expect("run from the repo root") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let Some(want) = embedded.get(&name) else { continue }; // the test above reports these
        let got = Fragment::load(&path).unwrap_or_else(|e| panic!("load {name}: {e}"));
        let want = want.to_fragment();

        let same = got.attach_idx == want.attach_idx
            && got.atoms.len() == want.atoms.len()
            && got.bonds.len() == want.bonds.len()
            && got.atoms.iter().zip(&want.atoms).all(|(a, b)| {
                a.element == b.element
                    && a.charge == b.charge
                    && (a.pos[0] - b.pos[0]).abs() < 1e-4
                    && (a.pos[1] - b.pos[1]).abs() < 1e-4
            })
            && got.bonds.iter().zip(&want.bonds).all(|(a, b)| {
                a.begin == b.begin && a.end == b.end && a.order == b.order
            });
        if !same {
            differ.push(format!(
                "  {name}: fragments/ has {} atoms/{} bonds/attach {}, assets/ has {} atoms/{} \
                 bonds/attach {}",
                got.atoms.len(), got.bonds.len(), got.attach_idx,
                want.atoms.len(), want.bonds.len(), want.attach_idx,
            ));
        }
    }

    assert!(
        differ.is_empty(),
        "fragments/ and assets/fragments/ disagree — copy fragments/<name>.json over \
         assets/fragments/<name>.json:\n{}",
        differ.join("\n"),
    );
}

/// Fragment coordinates are absolute molecular units and are never rescaled on insertion, so a
/// fragment drawn at the wrong bond length is drawn at the wrong bond length forever — beside
/// rings and chains built at `DEFAULT_BOND_LENGTH` (1.5). This is the invariant the deleted
/// in-code fragment tables quietly broke by staying at 1.0, and it is cheap to keep.
///
/// **BPin is the deliberate exception**, and the numbers say why. It is a five-membered
/// dioxaborolane carrying two *gem*-dimethyl pairs, and on a regular pentagon of side 1.5 the
/// two inner methyls come within 1.12 of each other. The relaxed ring in the JSON — B–O pulled
/// in to 1.38, the C–C pushed out to 1.65, hence the slack below — opens that to 1.46. Those
/// coordinates are `cleanup_2d`'s own output on the ideal pentagon, so squaring them up to a
/// uniform 1.5 would draw a more crowded molecule *and* be undone by the next cleanup.
#[test]
fn fragment_bonds_use_the_standard_bond_length() {
    const L: f32 = 1.5; // chembuider_rs::widget::DEFAULT_BOND_LENGTH
    /// The one fragment whose ring is deliberately relaxed, and how far it may stray.
    const RELAXED: (&str, f32) = ("BPin", 0.16);

    for def in Config::embedded().fragments {
        let frag = def.to_fragment();
        let tolerance = if def.name == RELAXED.0 { RELAXED.1 } else { 0.01 };
        for b in &frag.bonds {
            let (p, q) = (frag.atoms[b.begin].pos, frag.atoms[b.end].pos);
            let len = (p[0] - q[0]).hypot(p[1] - q[1]);
            assert!(
                (len - L).abs() <= tolerance,
                "{}: bond {}–{} is {len:.4}, expected {L} ± {tolerance}",
                def.name, b.begin, b.end,
            );
        }
    }
}

#[test]
fn load_cleanup_save_roundtrip() {
    let src_dir = std::path::Path::new("fragments");
    let out_dir = std::path::Path::new("target/fragment_test/cleaned");
    std::fs::create_dir_all(out_dir).unwrap();

    let mut count = 0;
    for entry in std::fs::read_dir(src_dir).expect("run from the repo root; `fragments/` must exist") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();

        // ── Load ──────────────────────────────────────────────────────────────
        let frag = Fragment::load(&path).unwrap_or_else(|e| panic!("load {name}: {e}"));
        assert!(!frag.atoms.is_empty(), "{name}: fragment has atoms");
        assert!(
            frag.attach_idx < frag.atoms.len(),
            "{name}: attach_idx in range"
        );

        // ── Cleanup (via Molecule) ────────────────────────────────────────────
        let mut mol = frag.to_molecule();
        cleanup_2d(&mut mol);
        let cleaned = Fragment::from_molecule(&mol, frag.attach_idx);

        // Cleanup relaxes coordinates but must preserve topology exactly.
        assert_eq!(cleaned.atoms.len(), frag.atoms.len(), "{name}: atom count preserved");
        assert_eq!(cleaned.bonds.len(), frag.bonds.len(), "{name}: bond count preserved");
        for (a, b) in cleaned.atoms.iter().zip(&frag.atoms) {
            assert_eq!(a.element, b.element, "{name}: elements preserved in order");
        }

        // ── Save + verify it reloads ──────────────────────────────────────────
        let out_path = out_dir.join(format!("{name}.json"));
        cleaned.save(&out_path).unwrap_or_else(|e| panic!("save {name}: {e}"));
        let reloaded = Fragment::load(&out_path).expect("cleaned JSON reloads");
        assert_eq!(reloaded.atoms.len(), cleaned.atoms.len(), "{name}: saved JSON round-trips");

        // Report how much cleanup moved things (max atom displacement).
        let moved = cleaned
            .atoms
            .iter()
            .zip(&frag.atoms)
            .map(|(c, o)| ((c.pos[0] - o.pos[0]).powi(2) + (c.pos[1] - o.pos[1]).powi(2)).sqrt())
            .fold(0.0_f32, f32::max);
        println!("{name}: cleaned, max atom shift {moved:.3} → {}", out_path.display());
        count += 1;
    }

    assert!(count > 0, "expected to process at least one fragment JSON");
    println!("processed {count} fragment JSON files");
}
