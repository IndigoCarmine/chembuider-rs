//! A fragment's name becomes the file `fragments/<name>.json`. A name that is not a plain file
//! name (`../x`, `a/b`, `C:x`, ...) must be refused before anything is written, never turned
//! into a path outside `fragments/`.

use chembuider_rs::ChemStructEditor;

#[test]
fn save_selection_as_fragment_refuses_a_name_that_leaves_fragments_dir() {
    let mut editor = ChemStructEditor::default();
    let a = editor.molecule.add_atom("C".into(), [0.0, 0.0], 0);
    let b = editor.molecule.add_atom("O".into(), [1.5, 0.0], 0);
    editor.selected_atoms.extend([a, b]);

    // `cargo test` runs in the crate root, so `fragments/../<stem>.json` is `./<stem>.json`.
    let stem = format!("chembuilder-evil-{}", std::process::id());
    let outside = std::path::PathBuf::from(format!("{stem}.json"));
    let result = editor.save_selection_as_fragment(&format!("../{stem}"));
    let escaped = outside.exists();
    if escaped {
        std::fs::remove_file(&outside).unwrap();
    }

    assert!(result.is_err(), "a name with `..` and `/` must be refused: {result:?}");
    assert!(!escaped, "wrote {outside:?}, outside fragments/");
    assert!(
        editor.config.fragments.iter().all(|f| f.name != format!("../{stem}")),
        "a refused fragment must not be registered either"
    );
}
