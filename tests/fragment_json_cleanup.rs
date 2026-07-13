//! Fragment JSON round-trip test: load every `fragments/<name>.json`, run the 2D cleanup on it,
//! and save the cleaned fragment back out as JSON.
//!
//! `cleanup_2d` operates on a `Molecule`, so the pipeline is
//!   JSON → `Fragment` → `Molecule` → `cleanup_2d` → `Fragment` → JSON.
//! Cleanup only relaxes atom positions; topology (atoms, bonds, attach point) must survive
//! unchanged. Cleaned copies are written to `target/fragment_test/cleaned/<name>.json`.
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
