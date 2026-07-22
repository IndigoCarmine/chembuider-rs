//! Keyboard-only reachability: can every structure be drawn without touching the mouse?
//!
//! The objective this file measures is concrete: **every structure
//! `scripts/gen_test_structures.py` generates must be buildable with the keyboard alone.**
//! That is the 123 fixtures in `tests/fixtures/rendering/` (the other two,
//! [`NON_SCRIPT_FIXTURES`](crate::testkit::NON_SCRIPT_FIXTURES), are hand-made geometry cases).
//!
//! Every test here drives the real widget through [`crate::testkit`], which sends key events
//! and *no pointer events at all*, and asserts after each frame that no hover was fabricated.

use crate::molecule::canonical::is_isomorphic;
use crate::molecule::{BondOrder, Molecule};
use crate::testkit::{load_fixture, type_keys};

/// Count bonds of a given order — the cheap check for "did the bond key land?".
fn count_order(mol: &Molecule, order: BondOrder) -> usize {
    mol.bonds.iter().filter(|b| b.order == order).count()
}

// ═══════════════════════════════════════════════════════════════════════════════
// Bootstrap
// ═══════════════════════════════════════════════════════════════════════════════

/// On an empty canvas only keys that `place_element_at_cursor` accepts can start a structure:
/// with no atom to hover and no hotspot to build from, everything else has no target.
///
/// This is the single most important thing to know when writing a key sequence, so it is
/// pinned here rather than left to be rediscovered.
#[test]
fn only_single_atom_keys_can_start_a_structure() {
    for key in ["c", "o", "n", "s", "f", "b"] {
        let editor = type_keys(key);
        assert!(
            !editor.molecule.atoms.is_empty(),
            "{key:?} should seed a structure on an empty canvas",
        );
        assert!(
            editor.hotspot_atom.is_some(),
            "{key:?} must leave a hotspot, or nothing can be built from it",
        );
    }

    for key in ["a", "1", "6"] {
        let editor = type_keys(key);
        assert!(
            editor.molecule.atoms.is_empty(),
            "{key:?} (benzene/chain/ring) has no target on an empty canvas and must do nothing",
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Bond targeting (Shift+arrow)
// ═══════════════════════════════════════════════════════════════════════════════

/// No atom may carry more than one double bond — that would be a pentavalent carbon.
///
/// This is a chemistry invariant, not a keyboard one: it caught `fuse_fragment_onto_bond`
/// adopting a free-standing ring's Kekulé pattern verbatim when fusing onto a bond whose
/// atoms already had double bonds, which produced impossible ring-fusion carbons.
fn assert_no_overvalent_atom(mol: &Molecule, what: &str) {
    for a in &mol.atoms {
        let doubles = mol.bonds.iter()
            .filter(|b| (b.begin == a.id || b.end == a.id) && b.order == BondOrder::Double)
            .count();
        assert!(
            doubles <= 1,
            "{what}: atom {} ({}) carries {doubles} double bonds",
            a.id, a.element,
        );
    }
}

/// Shift+arrow selects the bond towards the neighbour in that direction.
#[test]
fn shift_arrow_targets_a_bond() {
    // `c` seeds a carbon, `1` extends a one-carbon chain, so there is a bond to target.
    let editor = type_keys("c 1");
    assert_eq!(editor.molecule.bonds.len(), 1, "`c 1` should build one bond");
    assert!(editor.hotspot_bond.is_none(), "no bond targeted yet");

    // The chain grows up-and-right and the hotspot advances to the new atom, so the bond
    // now lies back to the left.
    let editor = type_keys("c 1 Shift+ArrowLeft");
    assert!(
        editor.hotspot_bond.is_some(),
        "Shift+arrow along a bond must target it; without this, no bond action is reachable",
    );

    // Pointing away from every neighbour targets nothing rather than guessing.
    let editor = type_keys("c 1 Shift+ArrowRight");
    assert!(editor.hotspot_bond.is_none(), "no neighbour in that direction");
}

/// A plain arrow moves the atom hotspot and must drop any bond target, which would otherwise
/// point at a bond nowhere near the new position.
#[test]
fn plain_arrow_clears_the_bond_target() {
    let editor = type_keys("c 1 Shift+ArrowLeft ArrowLeft");
    assert!(editor.hotspot_bond.is_none(), "moving the atom hotspot must clear the bond target");
}

#[test]
fn space_clears_the_bond_target() {
    let editor = type_keys("c 1 Shift+ArrowLeft Space");
    assert!(editor.hotspot_bond.is_none());
    assert!(editor.hotspot_atom.is_none());
}

// ═══════════════════════════════════════════════════════════════════════════════
// The two capabilities the goal actually requires
// ═══════════════════════════════════════════════════════════════════════════════

/// Bond order. 271 of the goal set's bonds are double and 7 are triple, spread across most
/// of the 123 structures, so this is the single most load-bearing capability.
#[test]
fn bond_order_is_reachable_by_keyboard() {
    let editor = type_keys("c 1 Shift+ArrowLeft 2");
    assert_eq!(
        count_order(&editor.molecule, BondOrder::Double), 1,
        "`2` on a targeted bond should make it double",
    );

    let editor = type_keys("c 1 Shift+ArrowLeft 3");
    assert_eq!(
        count_order(&editor.molecule, BondOrder::Triple), 1,
        "`3` on a targeted bond should make it triple",
    );
}

/// Ring fusion. 20 of the goal set are polycyclic (naphthalene, indole, caffeine, purine,
/// quinoline, …) and every one of them needs a ring fused onto an existing bond.
#[test]
fn ring_fusion_is_reachable_by_keyboard() {
    // Benzene from a seed carbon, then fuse a second benzene onto one of its bonds.
    let one_ring = type_keys("c a");
    assert_eq!(one_ring.molecule.atoms.len(), 6, "`c a` should build benzene");

    let fused = type_keys("c a Shift+ArrowRight a");
    assert_eq!(
        fused.molecule.atoms.len(), 10,
        "fusing benzene onto a benzene bond should give naphthalene's 10 atoms",
    );
    assert_no_overvalent_atom(&fused.molecule, "benzene fused onto benzene");
    assert!(
        is_isomorphic(&fused.molecule, &load_fixture("naphthalene")),
        "keyboard-built fused rings should match the naphthalene fixture",
    );
}

/// Fusing a *saturated* ring must leave the fragment's own orders alone — the Kekulé repair
/// is a fallback for invalid patterns, not a licence to invent double bonds.
#[test]
fn fusing_a_saturated_ring_adds_no_double_bonds() {
    let fused = type_keys("c 6 Shift+ArrowRight 6");
    assert_eq!(
        count_order(&fused.molecule, BondOrder::Double), 0,
        "fusing cyclohexane onto cyclohexane must stay fully saturated",
    );
}

/// `c` seeds a carbon on an empty canvas but *branches* once a hotspot exists.
///
/// These two behaviours share one key, and the cursor-placement path runs first, so an
/// unconditional carbon there made a methyl branch unreachable by keyboard: `c` dropped a
/// disconnected atom next to the structure instead.
/// A heteroatom must be able to *join* the structure, not just land beside it.
///
/// With no pointer there is no cursor to aim at, so dropping the atom at `last_mouse_mol`
/// left it as its own component: `c 1 o` produced a loose oxygen rather than an alcohol.
/// Since almost every structure in the goal set hangs a heteroatom off a carbon skeleton,
/// this single behaviour gated most of the objective.
#[test]
fn heteroatom_keys_join_the_structure() {
    let e = type_keys("c 1 O");
    assert_eq!(e.molecule.atoms.len(), 2, "`O` should retarget the hotspot, not add an atom");
    assert_eq!(e.molecule.bonds.len(), 1, "the oxygen must stay bonded to the carbon");
    assert!(
        e.molecule.atoms.iter().any(|a| a.element == "O"),
        "the hotspot atom should have become oxygen",
    );

    // Clearing the hotspot restores deliberate placement of a separate atom.
    let loose = type_keys("c 1 Space ArrowRight ArrowRight ArrowRight O");
    assert_eq!(loose.molecule.atoms.len(), 3);
    assert_eq!(loose.molecule.bonds.len(), 1, "with no hotspot, `O` seeds a separate component");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Parity gaps: reachable without a pointer, but not needed by the goal set
// ═══════════════════════════════════════════════════════════════════════════════

/// A second, *disconnected* component needs the insertion cursor to move — otherwise every
/// seeded atom lands on the same coordinate and snaps into the first component.
///
/// `ammonium_acetate` is the only structure in the goal set that needs this, but "all 123"
/// means it is required.
#[test]
fn a_disconnected_component_can_be_placed_apart() {
    // Seed an oxygen, drop the hotspot, walk the insertion cursor away, seed a nitrogen.
    let editor = type_keys("o Space ArrowRight ArrowRight ArrowRight n");
    assert_eq!(editor.molecule.atoms.len(), 2, "both atoms should exist");
    assert!(
        editor.molecule.bonds.is_empty(),
        "the two atoms must stay unbonded — they are separate components",
    );

    let (a, b) = (&editor.molecule.atoms[0], &editor.molecule.atoms[1]);
    let dist = ((a.pos[0] - b.pos[0]).powi(2) + (a.pos[1] - b.pos[1]).powi(2)).sqrt();
    assert!(
        dist > crate::molecule::Molecule::SNAP_RADIUS,
        "components placed {dist} apart would snap together",
    );
}

/// Deletion must work with no pointer, falling back to the keyboard target.
#[test]
fn delete_is_reachable_by_keyboard() {
    let built = type_keys("c 1");
    assert_eq!(built.molecule.atoms.len(), 2);

    // Delete acts on the hotspot atom, which `1` left on the newly added carbon.
    let after = type_keys("c 1 Delete");
    assert_eq!(after.molecule.atoms.len(), 1, "Delete should remove the hotspot atom");

    // An explicitly targeted bond outranks the lingering atom hotspot.
    let after = type_keys("c 1 Shift+ArrowLeft Delete");
    assert_eq!(after.molecule.atoms.len(), 2, "atoms should survive a bond delete");
    assert!(after.molecule.bonds.is_empty(), "the targeted bond should be gone");
}

/// The label editor, driven entirely from the keyboard.
///
/// This is the app's escape hatch for any element with no shortcut of its own — bare P and Si
/// have none, so it is the *only* route to `triphenylphosphine` or `tms_ether`. It had no test
/// at all until the harness learned to emit [`egui::Event::Text`]; before that the feature
/// looked like a product gap when it was really a gap in what the tests could drive.
///
/// `` ` `` opens the editor on the hotspot atom with its current element pre-selected, typing
/// replaces that selection, and Enter commits it straight into `atom.element`.
#[test]
fn label_editing_is_reachable_by_keyboard() {
    let e = type_keys("c 1 ` text:P Enter");
    assert_eq!(e.molecule.atoms.len(), 2, "editing a label must not add or remove atoms");
    assert_eq!(e.molecule.bonds.len(), 1, "nor disturb its bonds");
    assert!(
        e.molecule.atoms.iter().any(|a| a.element == "P"),
        "`` ` `` + text + Enter should write the typed element, got {:?}",
        e.molecule.atoms.iter().map(|a| &a.element).collect::<Vec<_>>(),
    );
    assert!(e.label_edit.is_none(), "Enter must close the editor");

    // Typing replaces the pre-selected element rather than appending to it — otherwise every
    // label would come out as "CP".
    assert!(
        !e.molecule.atoms.iter().any(|a| a.element.len() > 2),
        "the pre-filled element must be selected, so typing overwrites it",
    );

    // The hotspot survives the round trip, so a structure can keep growing afterwards. Without
    // this, a label edit would be a one-way door for a keyboard user.
    let grown = type_keys("c 1 ` text:P Enter 1");
    assert_eq!(grown.molecule.atoms.len(), 3, "building must continue after a label edit");
    assert!(grown.molecule.atoms.iter().any(|a| a.element == "P"), "and keep the new label");
}

/// A label the editor cannot resolve is still just text, so multi-character symbols work —
/// which is what `tms_ether` needs.
#[test]
fn label_editing_accepts_a_two_letter_element() {
    let e = type_keys("c 1 ` text:Si Enter");
    assert!(
        e.molecule.atoms.iter().any(|a| a.element == "Si"),
        "got {:?}",
        e.molecule.atoms.iter().map(|a| &a.element).collect::<Vec<_>>(),
    );
}

/// Ctrl+1/2/3 switch tools; without this the keyboard cannot leave the Bond tool.
#[test]
fn tool_switching_is_reachable_by_keyboard() {
    use crate::Tool;
    assert_eq!(type_keys("").tool, Tool::Bond, "Bond is the default tool");
    assert_eq!(type_keys("Ctrl+1").tool, Tool::Select);
    assert_eq!(type_keys("Ctrl+3").tool, Tool::Eraser);
    assert_eq!(type_keys("Ctrl+1 Ctrl+2").tool, Tool::Bond);
}

/// `g` grows the selection from the hotspot, which is the only route to selection — and so
/// to move and rotate — without a pointer.
#[test]
fn selection_is_reachable_by_keyboard() {
    // Ctrl+1 switches to the Select tool, where `g` adds the hotspot to the selection.
    let editor = type_keys("c 1 Ctrl+1 g");
    assert!(
        !editor.selected_atoms.is_empty(),
        "`g` must add the hotspot atom to the selection with no pointer",
    );
}

// ═══════════════════════════════════════════════════════════════════════════════
// Golden key sequences
// ═══════════════════════════════════════════════════════════════════════════════

/// A structure from the goal set, and a key sequence that builds it.
struct Golden {
    fixture: &'static str,
    keys: &'static str,
}

/// Executable witnesses that a structure really is keyboard-reachable.
///
/// Each entry is a genuine proof for one structure, and together they document the keyboard
/// language. **All 123 of the goal set are covered** — that is the objective, met.
///
/// Note the case distinction on heteroatom keys: lowercase `o`/`n` insert the *group* (OH,
/// NH2, with explicit hydrogens) while uppercase `O`/`N` place the bare atom. The fixtures
/// carry no explicit hydrogens, so goldens use the uppercase forms.
///
/// Two idioms are worth recognising when reading these:
///
/// - `s Delete` is how a sulfur is placed. There is no bare-S key, so `s` inserts S–H and
///   `Delete` strips the hydrogen; the hotspot lands back on the sulfur, so building continues
///   normally (see `dimethyl_disulfide`, which does it twice).
/// - `` ` text:P Enter `` sets an element through the label editor. P and Si have no binding at
///   all, and this is the app's general escape hatch for exactly that.
///
/// The first five are hand-authored. The rest were found by
/// [`keyboard_solver`](super::keyboard_solver), which searches key sequences against the real
/// widget; they are kept verbatim rather than tidied, because their value is as a regression
/// record of what the editor accepted, not as pretty prose. Regenerate with
/// `cargo test --lib solve_all_fixtures -- --ignored --nocapture`.
const GOLDENS: &[Golden] = &[
    Golden { fixture: "benzene",      keys: "c a" },
    Golden { fixture: "cyclohexane",  keys: "c 6" },
    Golden { fixture: "cyclopropane", keys: "c v" },
    Golden { fixture: "naphthalene",  keys: "c a Shift+ArrowRight a" },
    Golden { fixture: "ethanol",      keys: "c 1 1 O" },

    Golden { fixture: "acetaldehyde",         keys: "O 2 ArrowLeft 1" },
    Golden { fixture: "acetamide",            keys: "N 1 1 ArrowLeft 2" },
    Golden { fixture: "acetanilide",          keys: "N 1 a ArrowRight ArrowUp 1 1 ArrowRight 2" },
    Golden { fixture: "acetate",              keys: "O 1 1 ArrowLeft 2 ArrowRight ArrowUp -" },
    Golden { fixture: "acetic_acid",          keys: "O 1 1 ArrowLeft 2" },
    Golden { fixture: "acetone",              keys: "c 1 1 ArrowLeft 2" },
    Golden { fixture: "acetonitrile",         keys: "N 1 1 ArrowLeft ArrowLeft Shift+ArrowDown 3 ArrowDown" },
    Golden { fixture: "acetylene_diol",       keys: "O 1 1 1 O ArrowLeft ArrowLeft Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "alanine",              keys: "N 1 1 ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "ammonium_acetate",     keys: "N + Space ArrowRight ArrowRight ArrowRight O 1 1 ArrowLeft 2 ArrowRight ArrowUp -" },
    Golden { fixture: "aniline",              keys: "N 1 a" },
    Golden { fixture: "anisole",              keys: "O 1 ArrowLeft 1 a" },
    Golden { fixture: "anthracene",           keys: "c a ArrowRight ArrowRight ArrowDown Shift+ArrowRight a ArrowRight ArrowRight ArrowRight Shift+ArrowDown a ArrowDown" },
    Golden { fixture: "aspirin",              keys: "O a ArrowLeft 1 1 1 ArrowLeft 2 ArrowRight ArrowRight ArrowRight ArrowRight 1 1 O ArrowRight 2 ArrowLeft ArrowRight ArrowUp ArrowLeft O" },
    Golden { fixture: "benzaldehyde",         keys: "O a ArrowLeft 1 2" },
    Golden { fixture: "benzenesulfonamide",   keys: "N 1 1 a ArrowLeft ArrowLeft 2 ArrowRight 2 ArrowLeft s Delete" },
    Golden { fixture: "benzimidazole",        keys: "N a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 5 ArrowRight ArrowRight Shift+ArrowLeft 2 ArrowLeft N ArrowRight ArrowRight N" },
    Golden { fixture: "benzofuran",           keys: "O a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 5 ArrowRight ArrowRight ArrowRight O ArrowUp ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "benzoic_acid",         keys: "O 1 1 a ArrowLeft ArrowLeft 2" },
    Golden { fixture: "benzonitrile",         keys: "N 1 1 a ArrowLeft ArrowLeft ArrowLeft Shift+ArrowDown 3 ArrowDown" },
    Golden { fixture: "benzothiophene",       keys: "c a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 5 ArrowRight ArrowRight ArrowRight s Delete ArrowUp ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "bicarbonate",          keys: "O 1 2 ArrowLeft 1 O -" },
    Golden { fixture: "biphenyl",             keys: "c a ArrowRight ArrowRight 1 a" },
    Golden { fixture: "caffeine",             keys: "N 6 1 ArrowRight N ArrowDown 2 ArrowRight ArrowRight 1 ArrowRight N ArrowRight 5 ArrowRight 1 ArrowLeft N ArrowDown ArrowLeft N ArrowRight Shift+ArrowDown 2 ArrowDown ArrowLeft ArrowUp Shift+ArrowDown 2 ArrowDown ArrowUp ArrowLeft 2" },
    Golden { fixture: "carbamate",            keys: "N 1 1 1 ArrowLeft O ArrowLeft 2" },
    Golden { fixture: "carbazole",            keys: "N a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 5 ArrowRight ArrowRight ArrowRight Shift+ArrowUp a ArrowUp ArrowLeft N" },
    Golden { fixture: "carbon_tetrachloride", keys: "l 1 1 l ArrowLeft 1 l ArrowRight 1 l" },
    Golden { fixture: "carbonate",            keys: "O 1 2 ArrowLeft 2 - ArrowRight ArrowUp - ArrowRight ArrowLeft Shift+ArrowRight 1 ArrowRight" },
    Golden { fixture: "catechol",             keys: "O 1 a ArrowRight ArrowRight 1 O" },
    Golden { fixture: "cf3_chlorobenzene",    keys: "f 1 1 a ArrowLeft ArrowLeft 1 f ArrowRight 1 f ArrowLeft ArrowRight ArrowRight ArrowRight ArrowRight 1 l" },
    Golden { fixture: "chloroform",           keys: "l 1 1 l ArrowLeft 1 l" },
    Golden { fixture: "citric_acid",          keys: "O 1 1 1 1 1 1 O ArrowRight ArrowDown ArrowRight 1 1 O ArrowLeft 2 ArrowLeft ArrowLeft 1 O ArrowLeft ArrowDown ArrowLeft 2 ArrowRight ArrowRight ArrowRight ArrowLeft ArrowUp 2" },
    Golden { fixture: "coumarin",             keys: "O 1 2 ArrowLeft ArrowLeft Shift+ArrowDown a ArrowDown ArrowLeft ArrowLeft ArrowLeft Shift+ArrowUp a ArrowUp" },
    Golden { fixture: "cyclohexene",          keys: "c 6 ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "cyclopentanone",       keys: "O 5 ArrowLeft 2" },
    Golden { fixture: "cysteine",             keys: "N 1 1 1 s Delete ArrowLeft ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "dibenzofuran",         keys: "O a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 5 ArrowRight ArrowRight ArrowRight Shift+ArrowUp a ArrowUp ArrowLeft O" },
    Golden { fixture: "diethyl_ether",        keys: "O 1 1 ArrowLeft ArrowLeft 1 1" },
    Golden { fixture: "dimethyl_carbonate",   keys: "O 1 ArrowLeft 1 1 1 ArrowLeft O ArrowRight 2" },
    Golden { fixture: "dimethyl_disulfide",   keys: "c 1 1 1 ArrowLeft ArrowLeft s Delete ArrowRight s Delete" },
    Golden { fixture: "dimethyl_ether",       keys: "O 1 ArrowLeft 1" },
    Golden { fixture: "dimethyl_sulfone",     keys: "c 1 1 ArrowLeft 2 ArrowRight 2 ArrowLeft s Delete" },
    Golden { fixture: "dimethyl_sulfoxide",   keys: "c 1 1 ArrowLeft 2 ArrowRight s Delete" },
    Golden { fixture: "dioxane",              keys: "O 6 ArrowRight ArrowRight ArrowDown O ArrowRight ArrowLeft ArrowLeft O" },
    Golden { fixture: "diphenylacetylene",    keys: "c a ArrowRight ArrowRight 1 1 1 a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "dopamine",             keys: "N 1 1 1 a ArrowRight 1 O ArrowLeft ArrowLeft 1 O" },
    Golden { fixture: "epoxide",              keys: "O v O" },
    Golden { fixture: "ethanethiol",          keys: "c 1 1 s Delete" },
    Golden { fixture: "ethyl_benzoate",       keys: "O 1 1 ArrowLeft ArrowLeft 1 1 a ArrowRight ArrowRight 2" },
    Golden { fixture: "ethylene_glycol",      keys: "O 1 1 1 O" },
    Golden { fixture: "formamide",            keys: "N 1 2" },
    Golden { fixture: "fructose",             keys: "O 1 1 1 1 1 1 1 O ArrowLeft ArrowRight 1 O ArrowLeft ArrowDown 1 O ArrowRight ArrowRight 1 O ArrowLeft ArrowDown 2" },
    Golden { fixture: "furan",                keys: "O 5 ArrowRight O ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "gamma_butyrolactone",  keys: "O 5 O ArrowLeft 2" },
    Golden { fixture: "glucose_open",         keys: "O 1 1 1 1 1 1 2 ArrowLeft ArrowRight 1 O ArrowLeft ArrowDown 1 O ArrowRight ArrowRight 1 O ArrowLeft ArrowDown 1 O" },
    Golden { fixture: "glucose_ring",         keys: "O 1 1 6 1 O ArrowRight ArrowRight 1 O ArrowLeft ArrowUp 1 O ArrowLeft ArrowLeft 1 O ArrowLeft ArrowLeft O" },
    Golden { fixture: "glycine",              keys: "N 1 1 1 O ArrowLeft 2" },
    Golden { fixture: "guanidine",            keys: "N 1 1 N ArrowLeft 1 N ArrowRight ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "guanidinium",          keys: "N 1 1 N ArrowLeft 1 N ArrowRight ArrowRight + ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "histamine",            keys: "N 1 1 1 5 N ArrowRight ArrowLeft N ArrowLeft ArrowDown Shift+ArrowLeft 2 ArrowLeft ArrowDown ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "ibuprofen",            keys: "O 1 1 1 a ArrowRight ArrowLeft 1 1 1 ArrowLeft 1 ArrowRight ArrowDown ArrowLeft ArrowLeft ArrowLeft ArrowRight ArrowDown 1 ArrowLeft ArrowLeft 2" },
    Golden { fixture: "imidazole",            keys: "N 5 ArrowRight ArrowDown N ArrowRight ArrowLeft N ArrowRight Shift+ArrowLeft 2 ArrowLeft ArrowLeft ArrowDown Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "indole",               keys: "N a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 5 ArrowRight ArrowRight ArrowRight N ArrowUp ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "indomethacin_core",    keys: "l 1 a ArrowDown ArrowRight 1 1 5 ArrowLeft ArrowUp Shift+ArrowRight a ArrowRight ArrowDown 1 1 1 O ArrowLeft 2 ArrowLeft ArrowLeft ArrowLeft ArrowLeft 1 ArrowRight ArrowLeft N ArrowLeft 2 ArrowRight ArrowRight ArrowRight ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "isoquinoline",         keys: "N a ArrowRight ArrowRight ArrowDown Shift+ArrowRight a ArrowRight ArrowRight ArrowRight N" },
    Golden { fixture: "lactic_acid",          keys: "O 1 1 ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "malonic_acid",         keys: "O 1 1 1 1 O ArrowRight 2 ArrowLeft ArrowDown ArrowLeft 2" },
    Golden { fixture: "mesitylene",           keys: "c a ArrowLeft 1 ArrowRight ArrowRight ArrowRight 1 ArrowLeft ArrowUp ArrowLeft 1" },
    Golden { fixture: "methionine",           keys: "O 1 1 1 ArrowLeft 1 N ArrowLeft ArrowUp 1 1 1 ArrowRight s Delete ArrowDown ArrowRight ArrowDown ArrowLeft 2" },
    Golden { fixture: "methoxide",            keys: "O - 1" },
    Golden { fixture: "methylamine",          keys: "N 1" },
    Golden { fixture: "morpholine",           keys: "N 6 N ArrowRight ArrowRight ArrowDown O" },
    Golden { fixture: "nicotine",             keys: "N a ArrowLeft 1 5 ArrowRight ArrowUp 1 ArrowLeft N ArrowRight ArrowRight ArrowRight ArrowRight N" },
    Golden { fixture: "nitrate",              keys: "O 1 2 ArrowLeft 2 - ArrowRight N + ArrowUp - ArrowRight ArrowLeft Shift+ArrowRight 1 ArrowRight" },
    Golden { fixture: "nitrobenzene",         keys: "O 1 1 a ArrowLeft ArrowLeft 2 ArrowRight N + ArrowUp -" },
    Golden { fixture: "nitromethane",         keys: "O 1 2 ArrowLeft N + 1 ArrowRight ArrowUp -" },
    Golden { fixture: "oxalic_acid",          keys: "O 1 1 1 O ArrowLeft 2 ArrowLeft ArrowLeft 2" },
    Golden { fixture: "oxazole",              keys: "N 5 ArrowRight N ArrowDown ArrowLeft O ArrowLeft ArrowRight Shift+ArrowDown 2 ArrowDown ArrowDown ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "paracetamol",          keys: "O 1 a ArrowDown ArrowRight 1 1 1 ArrowLeft 2 ArrowLeft ArrowLeft N" },
    Golden { fixture: "phenol",               keys: "O 1 a" },
    Golden { fixture: "phenylalanine",        keys: "N 1 1 1 a ArrowLeft ArrowDown ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "phosphate",            keys: "O 1 2 ArrowLeft 1 O - ArrowRight 1 O - ArrowLeft ` text:P Enter" },
    Golden { fixture: "piperazine",           keys: "N 6 ArrowRight ArrowRight ArrowDown N ArrowRight ArrowLeft ArrowLeft N" },
    Golden { fixture: "piperidine",           keys: "N 6 ArrowRight ArrowRight N" },
    Golden { fixture: "proline",              keys: "O 1 1 5 N ArrowLeft ArrowLeft 2" },
    Golden { fixture: "propargyl_alcohol",    keys: "O 1 1 1 ArrowLeft Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "propyne",              keys: "c 1 1 ArrowLeft Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "purine",               keys: "N a ArrowLeft ArrowRight N ArrowRight ArrowRight Shift+ArrowDown 5 ArrowDown ArrowRight N ArrowRight ArrowRight N ArrowLeft ArrowLeft ArrowLeft N ArrowRight ArrowRight ArrowRight ArrowUp Shift+ArrowDown 2 ArrowDown" },
    Golden { fixture: "pyrazine",             keys: "N a ArrowLeft ArrowRight N ArrowRight ArrowRight ArrowLeft N" },
    Golden { fixture: "pyrazole",             keys: "N 5 ArrowRight N ArrowLeft N ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowDown Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "pyridine",             keys: "N a ArrowRight ArrowRight N" },
    Golden { fixture: "pyrimidine",           keys: "N a ArrowRight ArrowRight ArrowDown N ArrowRight ArrowLeft N" },
    Golden { fixture: "pyrrole",              keys: "N 5 ArrowRight N ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "quinoline",            keys: "N a ArrowRight ArrowRight ArrowDown Shift+ArrowRight a ArrowRight ArrowRight ArrowRight ArrowLeft N" },
    Golden { fixture: "resorcinol",           keys: "O 1 a ArrowDown 1 O" },
    Golden { fixture: "salicylic_acid",       keys: "O 1 1 a ArrowLeft ArrowRight 1 O ArrowRight ArrowDown ArrowLeft 2" },
    Golden { fixture: "serine",               keys: "N 1 1 1 O ArrowLeft ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "serotonin",            keys: "O 1 a ArrowDown ArrowRight Shift+ArrowLeft 5 ArrowLeft ArrowLeft ArrowRight 1 1 1 N ArrowLeft ArrowDown ArrowLeft ArrowLeft ArrowDown N ArrowLeft ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "styrene",              keys: "c a ArrowLeft 1 1 ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "succinic_acid",        keys: "O 1 1 1 1 1 O ArrowLeft 2 ArrowRight ArrowRight ArrowDown ArrowLeft 2" },
    Golden { fixture: "sulfanilamide",        keys: "N 1 a ArrowDown ArrowRight 1 2 ArrowLeft 2 ArrowRight 1 N ArrowLeft s Delete" },
    Golden { fixture: "sulfate",              keys: "O 1 2 ArrowLeft 2 ArrowRight ArrowUp - ArrowRight 1 O - ArrowLeft s Delete" },
    Golden { fixture: "tert_butanol",         keys: "O 1 1 ArrowLeft 1 ArrowRight 1" },
    Golden { fixture: "tetrahydrofuran",      keys: "O 5 ArrowRight O" },
    Golden { fixture: "tetramethylammonium",  keys: "N + 1 ArrowLeft 1 ArrowLeft 1 ArrowRight 1" },
    Golden { fixture: "tetrazole",            keys: "N 5 ArrowLeft ArrowDown N ArrowRight N ArrowRight N ArrowLeft N ArrowLeft ArrowDown Shift+ArrowLeft 2 ArrowLeft ArrowDown ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "tfa",                  keys: "O 1 1 1 f ArrowLeft 1 f ArrowLeft 1 f ArrowLeft ArrowLeft 2" },
    Golden { fixture: "thiazole",             keys: "N 5 N ArrowRight ArrowDown s Delete ArrowLeft ArrowLeft Shift+ArrowDown 2 ArrowDown ArrowLeft ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "thiophene",            keys: "c 5 ArrowRight s Delete ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "tms_ether",            keys: "O 1 1 ArrowLeft ArrowLeft 1 1 ArrowRight ArrowDown ArrowDown 1 ArrowRight 1 ArrowLeft ` text:Si Enter" },
    Golden { fixture: "toluene",              keys: "c a ArrowLeft 1" },
    Golden { fixture: "triazole",             keys: "N 5 ArrowLeft ArrowDown N ArrowLeft ArrowRight N ArrowRight N ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "triethyl_phosphate",   keys: "O 1 1 ArrowLeft ArrowLeft 1 1 O ArrowRight 1 1 1 ArrowRight ArrowDown O ArrowLeft ArrowLeft 1 1 ArrowRight ArrowDown ArrowRight 2 ArrowLeft ` text:P Enter" },
    Golden { fixture: "trimethylamine",       keys: "N 1 ArrowLeft 1 ArrowLeft 1" },
    Golden { fixture: "triphenylphosphine",   keys: "c a ArrowRight ArrowRight 1 1 a ArrowLeft ArrowDown 1 a ArrowUp ArrowUp ` text:P Enter" },
    Golden { fixture: "tryptophan",           keys: "N 1 1 1 5 ArrowRight Shift+ArrowDown a ArrowDown ArrowLeft ArrowDown ArrowLeft 1 1 O ArrowRight 2 ArrowLeft ArrowRight ArrowRight ArrowRight ArrowLeft ArrowRight N ArrowLeft ArrowDown Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "urea",                 keys: "N 1 1 N ArrowLeft 2" },
    Golden { fixture: "vanillin",             keys: "O 1 ArrowLeft 1 a 1 O ArrowLeft ArrowLeft ArrowLeft ArrowUp 1 2" },
];

/// The goal set must be *entirely* witnessed — that is the acceptance criterion, and a
/// structure that quietly went unexamined would look exactly like one that passed.
#[test]
fn every_goal_structure_has_a_golden() {
    use std::collections::HashSet;

    let covered: HashSet<&str> = GOLDENS.iter().map(|g| g.fixture).collect();
    let missing: Vec<String> = crate::testkit::script_fixtures()
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| !covered.contains(name.as_str()))
        .collect();

    assert!(missing.is_empty(), "goal structures with no golden: {missing:?}");
    assert_eq!(covered.len(), 123, "every structure the generator makes is keyboard-reachable");
}

#[test]
fn golden_key_sequences_build_their_structures() {
    let mut failures = Vec::new();
    for g in GOLDENS {
        let editor = type_keys(g.keys);
        let want = load_fixture(g.fixture);
        if !is_isomorphic(&editor.molecule, &want) {
            failures.push(format!(
                "  {:<14} keys {:<28} built {} atoms/{} bonds, want {} atoms/{} bonds",
                g.fixture, g.keys,
                editor.molecule.atoms.len(), editor.molecule.bonds.len(),
                want.atoms.len(), want.bonds.len(),
            ));
        }
    }
    assert!(failures.is_empty(), "golden sequences failed:\n{}", failures.join("\n"));
}
