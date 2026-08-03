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
use crate::widget::draw;

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

// ═══════════════════════════════════════════════════════════════════════════════
// Ring geometry
// ═══════════════════════════════════════════════════════════════════════════════

/// Every bond length in the molecule, in molecular units.
fn bond_lengths(mol: &Molecule) -> Vec<f32> {
    mol.bonds
        .iter()
        .map(|b| {
            let a = mol.atom_by_id(b.begin).expect("bond begin exists").pos;
            let c = mol.atom_by_id(b.end).expect("bond end exists").pos;
            (a[0] - c[0]).hypot(a[1] - c[1])
        })
        .collect()
}

/// A ring key must draw its ring at the same scale as everything else the editor draws.
///
/// `Fragment::ring` was built from a **unit** n-gon (side 1.0) while chains, the JSON
/// fragments and `fuse_ring_onto_bond` all work in `DEFAULT_BOND_LENGTH` (1.5) — so `6`
/// produced a cyclohexane two thirds the size of a `a`-built benzene sitting right next to it.
#[test]
fn ring_keys_use_the_standard_bond_length() {
    const L: f32 = crate::widget::DEFAULT_BOND_LENGTH;
    for (key, n) in [("v", 3), ("4", 4), ("5", 5), ("6", 6), ("7", 7), ("8", 8)] {
        let e = type_keys(&format!("c {key}"));
        assert_eq!(e.molecule.atoms.len(), n, "`c {key}` should build a {n}-membered ring");
        assert_eq!(e.molecule.bonds.len(), n, "`c {key}` should close the ring");
        for len in bond_lengths(&e.molecule) {
            assert!(
                (len - L).abs() < 0.01,
                "`c {key}`: ring edge is {len:.3}, expected the standard bond length {L}",
            );
        }
    }
}

/// The two routes to a six-ring — the ring key and the benzene fragment — must agree.
///
/// They are drawn side by side all the time (a cyclohexane hung off a benzene), so any
/// mismatch is immediately visible as one ring being smaller than the other.
#[test]
fn ring_key_and_benzene_fragment_build_the_same_hexagon() {
    let ring = bond_lengths(&type_keys("c 6").molecule);
    let benz = bond_lengths(&type_keys("c a").molecule);
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let (r, b) = (mean(&ring), mean(&benz));
    assert!(
        (r - b).abs() < 0.01,
        "`6` builds edges of {r:.3} but `a` builds {b:.3} — the same ring at two scales",
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
// Heteroatom survival
// ═══════════════════════════════════════════════════════════════════════════════

/// How many atoms of each element the molecule has.
fn elements(mol: &Molecule) -> std::collections::BTreeMap<&str, usize> {
    let mut out = std::collections::BTreeMap::new();
    for a in &mol.atoms {
        *out.entry(a.element.as_str()).or_insert(0) += 1;
    }
    out
}

/// A skeleton fragment grows *from* the hotspot atom; it does not replace it.
///
/// Every carbon-rooted fragment used to overwrite the atom it was grown from, so `a` on a
/// nitrogen erased the nitrogen and built plain benzene. The whole point of standing on an
/// N and pressing the benzene key is pyridine.
#[test]
fn a_carbon_rooted_fragment_keeps_the_heteroatom_it_grows_from() {
    for (keys, elem) in [("N a", "N"), ("O a", "O"), ("s a", "S")] {
        let e = type_keys(keys);
        assert_eq!(
            e.molecule.atoms.len(), 6,
            "`{keys}` should build one six-ring, got {:?}", elements(&e.molecule),
        );
        assert_eq!(
            elements(&e.molecule).get(elem), Some(&1),
            "`{keys}` must keep its {elem}, got {:?}", elements(&e.molecule),
        );
        assert_eq!(
            elements(&e.molecule).get("C"), Some(&5),
            "`{keys}`: the other five ring atoms are carbon, got {:?}", elements(&e.molecule),
        );
    }

    // The headline case, against the real structure.
    assert!(
        is_isomorphic(&type_keys("N a").molecule, &load_fixture("pyridine")),
        "`N a` should be pyridine",
    );

    // Ring keys go through the same path, so they behave the same way.
    let e = type_keys("N 6");
    assert_eq!(elements(&e.molecule).get("N"), Some(&1), "`N 6` should be a piperidine ring");
    assert_eq!(elements(&e.molecule).get("C"), Some(&5));
}

/// Charge belongs to the atom, not to the fragment grown off it.
///
/// Element and charge are set together, so preserving one while resetting the other would
/// silently neutralise an N⁺ the moment a ring was grown from it.
#[test]
fn growing_a_skeleton_keeps_the_atoms_charge() {
    let e = type_keys("N + a");
    let n = e.molecule.atoms.iter().find(|a| a.element == "N").expect("the N survives");
    assert_eq!(n.charge, 1, "the charge must survive the ring too");
}

/// On a plain carbon nothing changes: `a` is still benzene.
#[test]
fn a_carbon_rooted_fragment_on_a_carbon_is_unchanged() {
    let e = type_keys("c a");
    assert_eq!(elements(&e.molecule), std::collections::BTreeMap::from([("C", 6)]));
}

/// A *heteroatom-rooted* fragment still retargets the atom — that is what the key means.
///
/// `o` is "make this an OH", not "hang an OH off this carbon"; the same for `n` (NH2) and
/// the halogens. Keeping the old element here would give C–O–H where the user asked for a
/// two-atom alcohol.
#[test]
fn a_heteroatom_rooted_fragment_still_retargets_the_atom() {
    let e = type_keys("c 1 o");
    assert_eq!(e.molecule.atoms.len(), 3, "`o` inserts O–H onto the hotspot carbon");
    assert_eq!(
        elements(&e.molecule).get("C"), Some(&1),
        "the hotspot carbon must have *become* the O, got {:?}", elements(&e.molecule),
    );
    assert_eq!(elements(&e.molecule).get("O"), Some(&1));

    // Retargeting one heteroatom to another is still allowed.
    let e = type_keys("O 1 1 n");
    assert_eq!(
        elements(&e.molecule).get("N"), Some(&1),
        "`n` on a carbon must give the N of an NH2, got {:?}", elements(&e.molecule),
    );
}

/// Hydrogen is a cap, not a skeleton atom, so it is not preserved: a ring vertex or chain
/// carbon can never be an H, and leaving one there produces a structure nothing can fix.
#[test]
fn hydrogen_is_not_preserved_by_a_skeleton_fragment() {
    let e = type_keys("c 1 h a");
    assert_eq!(
        elements(&e.molecule).get("H"), None,
        "the H should have become the ring's carbon, got {:?}", elements(&e.molecule),
    );
}

/// Fusing onto a bond never touched the shared atoms' elements — this pins that, because the
/// atom path's fix would be pointless if the bond path still flattened heteroatoms.
#[test]
fn fusing_onto_a_bond_keeps_both_shared_atoms() {
    let e = type_keys("N 1 Shift+ArrowLeft a");
    assert_eq!(e.molecule.atoms.len(), 6, "benzene fused onto the N–C bond is a six-ring");
    assert_eq!(
        elements(&e.molecule).get("N"), Some(&1),
        "the fused ring must keep the nitrogen, got {:?}", elements(&e.molecule),
    );
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
// Hydrogens
// ═══════════════════════════════════════════════════════════════════════════════

/// Nothing the user cannot see may change what the user *can* see.
///
/// A folded hydrogen is drawn nowhere — no vertex, no bond line, no label — so it must
/// never be the reason its heavy neighbour wears the red over-valence ring. When it was,
/// the warning had no visible cause and no reachable fix: the offending atom could only be
/// found by lassoing empty space.
fn assert_no_invisible_atom_inflates_a_valence(mol: &Molecule, what: &str) {
    for a in &mol.atoms {
        if !draw::is_folded_h(mol, a.id) {
            continue;
        }
        for n in mol.neighbor_atom_ids(a.id) {
            assert!(
                !draw::is_valence_invalid(mol, n),
                "{what}: atom {n} is flagged over-valence by hydrogen {}, which is not drawn",
                a.id,
            );
        }
    }
}

/// A hydrogen the user asked for by name must appear on the canvas.
///
/// `h` retargets the hotspot atom to hydrogen. Folding *every* terminal hydrogen into its
/// neighbour's label made that atom vanish outright — the key looked like it did nothing at
/// all, leaving only the orange hotspot ring floating a bond length from the structure.
#[test]
fn a_hydrogen_placed_on_purpose_is_drawn() {
    let e = type_keys("c 1 h");
    let h = e.molecule.atoms.iter()
        .find(|a| a.element == "H")
        .expect("`h` should put a hydrogen on the canvas");
    assert!(
        !draw::is_folded_h(&e.molecule, h.id),
        "a hydrogen the user placed is an atom like any other and must be drawn",
    );
}

/// Nothing may be over-valence, and no atom may be sitting somewhere the user cannot see it.
///
/// Together with [`assert_no_invisible_atom_inflates_a_valence`] this is what "the structure
/// came out clean" means: no red ring anywhere, and every atom either drawn on the canvas or
/// legitimately folded into a neighbour's label.
fn assert_no_valence_warning(mol: &Molecule, what: &str) {
    for a in &mol.atoms {
        assert!(
            !draw::is_valence_invalid(mol, a.id),
            "{what}: atom {} ({}) wears the over-valence ring",
            a.id, a.element,
        );
    }
}

/// Azobenzene, Ph–N=N–Ph, drawn the way the `n` key invites: each nitrogen arrives as an
/// NH₂ group and then grows the bonds that leave its label no hydrogen to show.
///
/// A group's hydrogen stands in for spare valence (see [`crate::molecule::Atom::group_h`]),
/// so once the azo double bond and the second aryl bond have taken that valence there is
/// nothing left for it to stand in for, and it goes. Before that, this sequence produced two
/// pentavalent nitrogens: two red rings, and two hydrogens that appeared nowhere on the
/// canvas — the structure from the bug report. What has to come out is plain azobenzene:
/// twelve carbons, two nitrogens, no hydrogen atoms at all and no warning.
#[test]
fn azobenzene_from_the_n_key_needs_no_hand_cleanup() {
    let e = type_keys(
        "c a ArrowLeft 1 n ArrowDown 1 n ArrowRight Shift+ArrowRight 2 ArrowRight ArrowDown 1 a",
    );

    // The skeleton really is Ph–N=N–Ph: two nitrogens, joined to each other by a double bond.
    let ns: Vec<u32> = e.molecule.atoms.iter()
        .filter(|a| a.element == "N")
        .map(|a| a.id)
        .collect();
    assert_eq!(ns.len(), 2, "azobenzene has two nitrogens");
    assert_eq!(
        e.molecule.bond_between(ns[0], ns[1]).map(|b| b.order.clone()),
        Some(BondOrder::Double),
        "the two nitrogens should be joined by the azo double bond",
    );

    assert_eq!(
        elements(&e.molecule),
        std::collections::BTreeMap::from([("C", 12), ("N", 2)]),
        "azobenzene is C12H10N2 — every hydrogen here is implicit, so none is an atom",
    );
    assert_eq!(e.molecule.bonds.len(), 15, "two rings (12) plus C–N, N=N, N–C");
    assert_no_valence_warning(&e.molecule, "azobenzene built with `n`");
    assert_no_invisible_atom_inflates_a_valence(&e.molecule, "azobenzene built with `n`");
}

/// The same rule reached through a fragment rather than a bond key: growing a ring off an
/// `n`-inserted nitrogen is pyridine.
///
/// The benzene fragment gives the nitrogen two ring bonds (one of them the Kekulé double), so
/// with the group hydrogen still attached it would stand at 1 + 1 + 2 = 4 — a pentavalent N
/// wearing a warning ring, over a hydrogen that had already stopped being drawn.
#[test]
fn a_ring_grown_on_a_group_nitrogen_is_pyridine() {
    let e = type_keys("c n ArrowLeft a");
    assert_eq!(
        elements(&e.molecule),
        std::collections::BTreeMap::from([("C", 5), ("N", 1)]),
        "the NH₂ hydrogen has no room in a pyridine ring and must be gone",
    );
    assert!(is_isomorphic(&e.molecule, &load_fixture("pyridine")), "`c n ArrowLeft a` is pyridine");
    assert_no_valence_warning(&e.molecule, "pyridine grown on an NH2 nitrogen");
}

/// Inserting a group is not itself a reason to lose a hydrogen: nothing is consumed until a
/// bond actually arrives.
///
/// This is the other half of the rule, and the half a too-eager version breaks — an `s` that
/// dropped its hydrogen on sight would make S–H unreachable, and with it every thiol.
///
/// `S` (SiH₃) used to be listed here with two hydrogens rather than three. That was never this
/// rule's doing, nor the fragment's: see
/// [`a_fragment_never_lands_an_atom_on_an_existing_one`].
#[test]
fn an_untouched_group_keeps_every_hydrogen_it_brought() {
    for (key, heavy, hydrogens) in
        [("n", "N", 1), ("o", "O", 1), ("s", "S", 1), ("p", "P", 2), ("S", "Si", 3)]
    {
        let e = type_keys(&format!("c 1 {key}"));
        let got = elements(&e.molecule);
        assert_eq!(
            got.get("H"), Some(&hydrogens),
            "`c 1 {key}` should keep all {hydrogens} of its hydrogens, got {got:?}",
        );
        assert_eq!(got.get(heavy), Some(&1), "`c 1 {key}` should place one {heavy}, got {got:?}");
        for h in e.molecule.atoms.iter().filter(|a| a.element == "H") {
            assert!(
                draw::is_folded_h(&e.molecule, h.id),
                "`c 1 {key}`: hydrogen {} belongs inside the label, not on the canvas", h.id,
            );
        }
        assert_no_valence_warning(&e.molecule, &format!("`c 1 {key}`"));
    }
}

/// A fragment must not drop one of its own atoms on top of an atom that is already there.
///
/// `best_new_bond_angle` puts a new bond 120° away from the one the atom already carries, and a
/// three-fold group (SiH₃, CF₃, TMS, TBS) branches ±120° off its own axis — so one branch points
/// straight back down the existing bond and lands *exactly* on the neighbour, one bond-length
/// out. `insert_fragment`'s snap then merges the two and the group silently arrives an atom
/// short: `c 1 S` drew SiH₂, with the missing hydrogen sitting 0.0001 from the carbon.
///
/// Note what this is *not*. The geometry only became coincident when the JSON fragments moved
/// to the editor's 1.5 bond length: at the old 1.0 spacing the stray branch landed 0.5 from the
/// neighbour, outside `SNAP_RADIUS` (0.4), and nothing merged. Widening the fragment is what
/// exposed the placement bug, so "the fragment is drawn too small" is exactly backwards.
///
/// Checked over every built-in fragment rather than just the four that trip it, because the
/// rule is about placement and not about any one group's shape.
///
/// A chain end is the case to check because it is the one with nothing to fuse to. Landing a
/// *ring* vertex on an existing atom is a wanted move elsewhere — it is how `5` on a benzene
/// carbon draws an indole — so the editor allows it; here there is no second ring in sight and
/// every fragment must arrive whole.
#[test]
fn a_fragment_never_lands_an_atom_on_an_existing_one() {
    let config = crate::Config::embedded();
    let mut failures = Vec::new();

    for def in &config.fragments {
        let frag = def.to_fragment();
        if frag.atoms.len() < 2 {
            continue; // single-atom fragments relabel the atom; there is nothing to place
        }
        // A chain end: one existing neighbour, which is the case that goes wrong.
        let mut e = type_keys("c 1");
        let hot = e.hotspot_atom.expect("`c 1` leaves the hotspot on the new atom");
        let before = e.molecule.atoms.len();

        let (angle, flip) = e.best_fragment_placement(hot, &frag);
        let new_ids = e.molecule.insert_fragment(&frag, hot, angle, flip);

        let want = frag.atoms.len() - 1; // every atom but the root, which fuses onto `hot`
        if new_ids.len() != want || e.molecule.atoms.len() != before + want {
            failures.push(format!(
                "  {:<8} placed {} of {want} atoms ({} → {} in the molecule)",
                def.name,
                new_ids.len(),
                before,
                e.molecule.atoms.len(),
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "fragments lost atoms to the insertion snap:\n{}",
        failures.join("\n"),
    );
}

/// The same rule seen from the keyboard, on the group that actually shipped broken.
#[test]
fn sih3_arrives_with_three_hydrogens() {
    let e = type_keys("c 1 S");
    assert_eq!(
        elements(&e.molecule),
        std::collections::BTreeMap::from([("C", 1), ("Si", 1), ("H", 3)]),
        "`c 1 S` is CH₃–SiH₃; the group brings three hydrogens and keeps all three",
    );

    // And they are three *distinct* places, not three ids sharing a coordinate.
    let snap = crate::molecule::Molecule::SNAP_RADIUS;
    for (i, a) in e.molecule.atoms.iter().enumerate() {
        for b in e.molecule.atoms.iter().skip(i + 1) {
            let d = (a.pos[0] - b.pos[0]).hypot(a.pos[1] - b.pos[1]);
            assert!(
                d >= snap,
                "atoms {} ({}) and {} ({}) are {d:.4} apart — closer than the snap radius {snap}",
                a.id, a.element, b.id, b.element,
            );
        }
    }
}

/// A hydrogen the user placed by hand is an ordinary atom, and no amount of bonding removes it.
///
/// `c 1 h ArrowLeft N a` puts an H on an atom, turns that atom into a nitrogen and then grows
/// a benzene from it — the exact shape of the pyridine case above, except that the hydrogen
/// was typed rather than delivered by a group. The nitrogen therefore really is over-valence,
/// and that is the correct outcome: the warning is about an atom the user can see and delete.
#[test]
fn a_hydrogen_placed_on_purpose_survives_any_amount_of_bonding() {
    let e = type_keys("c 1 h ArrowLeft N a");
    let h = e.molecule.atoms.iter()
        .find(|a| a.element == "H")
        .expect("a hand-placed hydrogen is never consumed");
    assert!(!h.group_h, "`h` places an ordinary atom, not a group's decoration");
    assert!(!draw::is_folded_h(&e.molecule, h.id), "and it is drawn on the canvas");

    let n = e.molecule.atoms.iter().find(|a| a.element == "N").expect("the N survives the ring");
    assert_eq!(
        e.molecule.bond_order_sum(n.id), 4,
        "N–H plus the ring's single and double bond is four bond-orders",
    );
    assert!(
        draw::is_valence_invalid(&e.molecule, n.id),
        "an explicit hydrogen that over-bonds its neighbour must still say so",
    );
    assert_no_invisible_atom_inflates_a_valence(&e.molecule, "a hand-placed hydrogen on a ring N");
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
/// Three idioms are worth recognising when reading these:
///
/// - `s Delete` is how a sulfur is placed. There is no bare-S key, so `s` inserts S–H and
///   `Delete` strips the hydrogen; the hotspot lands back on the sulfur, so building continues
///   normally (see `dimethyl_disulfide`, which does it twice). On a centre that is *already*
///   full — the S(VI) of a sulfone, sulfonamide or sulfate — plain `s` is enough and the
///   `Delete` must be left off: the group hydrogen has nowhere to sit, so it is consumed the
///   moment it arrives and the `Delete` would fall through onto the sulfur itself.
/// - `` ` text:P Enter `` sets an element through the label editor. P and Si have no binding at
///   all, and this is the app's general escape hatch for exactly that.
/// - **A heteroatom seed survives the ring grown from it**, so a heterocycle is seeded rather
///   than retrofitted: `N a` is pyridine, `N 6` piperidine, `O 5` tetrahydrofuran. Every one of
///   these used to lay down an all-carbon ring and walk back to retarget an atom, which is why
///   so many of these sequences are shorter than they once were.
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

    Golden { fixture: "acetaldehyde",         keys: "c 1 2" },
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
    Golden { fixture: "aspirin",              keys: "c a ArrowLeft 1 1 1 ArrowLeft 2 ArrowRight ArrowRight ArrowRight ArrowRight 1 1 O ArrowRight 2 ArrowLeft ArrowRight ArrowUp ArrowLeft O" },
    Golden { fixture: "benzaldehyde",         keys: "c a ArrowLeft 1 2" },
    Golden { fixture: "benzenesulfonamide",   keys: "N 1 1 a ArrowLeft ArrowLeft 2 ArrowRight 2 ArrowLeft s" },
    Golden { fixture: "benzimidazole",        keys: "N 5 ArrowLeft ArrowDown Shift+ArrowRight a ArrowRight ArrowUp N ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "benzofuran",           keys: "O 5 ArrowLeft ArrowDown Shift+ArrowRight a ArrowRight ArrowUp ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "benzoic_acid",         keys: "O 1 1 a ArrowLeft ArrowLeft 2" },
    Golden { fixture: "benzonitrile",         keys: "N 1 1 a ArrowLeft ArrowLeft ArrowLeft Shift+ArrowDown 3 ArrowDown" },
    Golden { fixture: "benzothiophene",       keys: "c a ArrowRight ArrowRight ArrowDown 5 ArrowRight s Delete ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "bicarbonate",          keys: "O 1 2 ArrowLeft 1 O -" },
    Golden { fixture: "biphenyl",             keys: "c a ArrowRight ArrowRight 1 a" },
    Golden { fixture: "caffeine",             keys: "N 6 5 ArrowUp 1 ArrowLeft N ArrowDown ArrowRight 2 ArrowLeft ArrowDown 1 ArrowLeft N ArrowLeft 2 ArrowRight ArrowUp 1 ArrowRight ArrowUp ArrowLeft N ArrowRight Shift+ArrowDown 2 ArrowDown ArrowDown ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "carbamate",            keys: "N 1 1 1 ArrowLeft O ArrowLeft 2" },
    Golden { fixture: "carbazole",            keys: "N 5 ArrowLeft ArrowDown Shift+ArrowRight a ArrowRight ArrowUp ArrowLeft Shift+ArrowRight a ArrowRight" },
    Golden { fixture: "carbon_tetrachloride", keys: "l 1 1 l ArrowLeft 1 l ArrowRight 1 l" },
    Golden { fixture: "carbonate",            keys: "O 1 2 ArrowLeft 2 - ArrowRight ArrowUp - ArrowRight ArrowLeft Shift+ArrowRight 1 ArrowRight" },
    Golden { fixture: "catechol",             keys: "O 1 a ArrowRight ArrowRight 1 O" },
    Golden { fixture: "cf3_chlorobenzene",    keys: "f 1 1 a ArrowLeft ArrowLeft 1 f ArrowRight 1 f ArrowLeft ArrowRight ArrowRight ArrowRight ArrowRight 1 l" },
    Golden { fixture: "chloroform",           keys: "l 1 1 l ArrowLeft 1 l" },
    Golden { fixture: "citric_acid",          keys: "O 1 1 1 1 1 1 O ArrowRight ArrowDown ArrowRight 1 1 O ArrowLeft 2 ArrowLeft ArrowLeft 1 O ArrowLeft ArrowDown ArrowLeft 2 ArrowRight ArrowRight ArrowRight ArrowLeft ArrowUp 2" },
    Golden { fixture: "coumarin",             keys: "O 1 2 ArrowLeft ArrowLeft Shift+ArrowDown a ArrowDown ArrowLeft ArrowLeft ArrowLeft Shift+ArrowUp a ArrowUp" },
    Golden { fixture: "cyclohexene",          keys: "c 6 ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "cyclopentanone",       keys: "c 5 ArrowLeft 2" },
    Golden { fixture: "cysteine",             keys: "N 1 1 1 s Delete ArrowLeft ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "dibenzofuran",         keys: "O 5 ArrowLeft ArrowDown Shift+ArrowRight a ArrowRight ArrowUp ArrowLeft Shift+ArrowRight a ArrowRight" },
    Golden { fixture: "diethyl_ether",        keys: "O 1 1 ArrowLeft ArrowLeft 1 1" },
    Golden { fixture: "dimethyl_carbonate",   keys: "O 1 ArrowLeft 1 1 1 ArrowLeft O ArrowRight 2" },
    Golden { fixture: "dimethyl_disulfide",   keys: "c 1 1 1 ArrowLeft ArrowLeft s Delete ArrowRight s Delete" },
    Golden { fixture: "dimethyl_ether",       keys: "O 1 ArrowLeft 1" },
    Golden { fixture: "dimethyl_sulfone",     keys: "c 1 1 ArrowLeft 2 ArrowRight 2 ArrowLeft s" },
    Golden { fixture: "dimethyl_sulfoxide",   keys: "c 1 1 ArrowLeft 2 ArrowRight s Delete" },
    Golden { fixture: "dioxane",              keys: "O 6 ArrowRight ArrowRight O" },
    Golden { fixture: "diphenylacetylene",    keys: "c a ArrowRight ArrowRight 1 1 1 a ArrowRight ArrowRight ArrowDown Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "dopamine",             keys: "N 1 1 1 a ArrowRight 1 O ArrowLeft ArrowLeft 1 O" },
    Golden { fixture: "epoxide",              keys: "O v" },
    Golden { fixture: "ethanethiol",          keys: "c 1 1 s Delete" },
    Golden { fixture: "ethyl_benzoate",       keys: "O 1 1 ArrowLeft ArrowLeft 1 1 a ArrowRight ArrowRight 2" },
    Golden { fixture: "ethylene_glycol",      keys: "O 1 1 1 O" },
    Golden { fixture: "formamide",            keys: "N 1 2" },
    Golden { fixture: "fructose",             keys: "O 1 1 1 1 1 1 1 O ArrowLeft ArrowRight 1 O ArrowLeft ArrowDown 1 O ArrowRight ArrowRight 1 O ArrowLeft ArrowDown 2" },
    Golden { fixture: "furan",                keys: "O 5 ArrowLeft ArrowDown Shift+ArrowRight 2 ArrowRight ArrowRight ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "gamma_butyrolactone",  keys: "O 5 ArrowLeft ArrowDown 2" },
    Golden { fixture: "glucose_open",         keys: "O 1 1 1 1 1 1 2 ArrowLeft ArrowRight 1 O ArrowLeft ArrowDown 1 O ArrowRight ArrowRight 1 O ArrowLeft ArrowDown 1 O" },
    Golden { fixture: "glucose_ring",         keys: "O 1 1 6 1 O ArrowRight ArrowRight 1 O ArrowLeft ArrowUp 1 O ArrowLeft ArrowLeft 1 O ArrowLeft ArrowLeft O" },
    Golden { fixture: "glycine",              keys: "N 1 1 1 O ArrowLeft 2" },
    Golden { fixture: "guanidine",            keys: "N 1 1 N ArrowLeft 1 N ArrowRight ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "guanidinium",          keys: "N 1 1 N ArrowLeft 1 N ArrowRight ArrowRight + ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "histamine",            keys: "N 1 1 1 5 N ArrowRight ArrowLeft N ArrowLeft ArrowDown Shift+ArrowLeft 2 ArrowLeft ArrowDown ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "ibuprofen",            keys: "O 1 1 1 a ArrowRight ArrowLeft 1 1 1 ArrowLeft 1 ArrowRight ArrowDown ArrowLeft ArrowLeft ArrowLeft ArrowRight ArrowDown 1 ArrowLeft ArrowLeft 2" },
    Golden { fixture: "imidazole",            keys: "N 5 ArrowRight N ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "indole",               keys: "N 5 ArrowLeft ArrowDown Shift+ArrowRight a ArrowRight ArrowUp ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "indomethacin_core",    keys: "l 1 a ArrowDown ArrowRight 1 1 5 ArrowLeft ArrowUp Shift+ArrowRight a ArrowRight ArrowDown 1 1 1 O ArrowLeft 2 ArrowLeft ArrowLeft ArrowLeft ArrowLeft 1 ArrowRight ArrowLeft N ArrowLeft 2 ArrowRight ArrowRight ArrowRight ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "isoquinoline",         keys: "N a ArrowRight ArrowRight Shift+ArrowLeft a ArrowLeft" },
    Golden { fixture: "lactic_acid",          keys: "O 1 1 ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "malonic_acid",         keys: "O 1 1 1 1 O ArrowRight 2 ArrowLeft ArrowDown ArrowLeft 2" },
    Golden { fixture: "mesitylene",           keys: "c a ArrowLeft 1 ArrowRight ArrowRight ArrowRight 1 ArrowLeft ArrowUp ArrowLeft 1" },
    Golden { fixture: "methionine",           keys: "O 1 1 1 ArrowLeft 1 N ArrowLeft ArrowUp 1 1 1 ArrowRight s Delete ArrowDown ArrowRight ArrowDown ArrowLeft 2" },
    Golden { fixture: "methoxide",            keys: "O - 1" },
    Golden { fixture: "methylamine",          keys: "N 1" },
    Golden { fixture: "morpholine",           keys: "N 6 ArrowRight ArrowRight O" },
    Golden { fixture: "nicotine",             keys: "N 5 1 a ArrowRight ArrowDown ArrowLeft 1 ArrowRight ArrowRight ArrowUp ArrowRight ArrowUp N" },
    Golden { fixture: "nitrate",              keys: "O 1 2 ArrowLeft 2 - ArrowRight N + ArrowUp - ArrowRight ArrowLeft Shift+ArrowRight 1 ArrowRight" },
    Golden { fixture: "nitrobenzene",         keys: "O 1 1 a ArrowLeft ArrowLeft 2 ArrowRight N + ArrowUp -" },
    Golden { fixture: "nitromethane",         keys: "O 1 2 ArrowLeft N + 1 ArrowRight ArrowUp -" },
    Golden { fixture: "oxalic_acid",          keys: "O 1 1 1 O ArrowLeft 2 ArrowLeft ArrowLeft 2" },
    Golden { fixture: "oxazole",              keys: "N 5 ArrowRight ArrowDown O ArrowLeft ArrowLeft Shift+ArrowDown 2 ArrowDown ArrowLeft ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "paracetamol",          keys: "O 1 a ArrowDown ArrowRight 1 1 1 ArrowLeft 2 ArrowLeft ArrowLeft N" },
    Golden { fixture: "phenol",               keys: "O 1 a" },
    Golden { fixture: "phenylalanine",        keys: "N 1 1 1 a ArrowLeft ArrowDown ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "phosphate",            keys: "O 1 2 ArrowLeft 1 O - ArrowRight 1 O - ArrowLeft ` text:P Enter" },
    Golden { fixture: "piperazine",           keys: "N 6 ArrowRight ArrowRight N" },
    Golden { fixture: "piperidine",           keys: "N 6" },
    Golden { fixture: "proline",              keys: "O 1 1 5 N ArrowLeft ArrowLeft 2" },
    Golden { fixture: "propargyl_alcohol",    keys: "O 1 1 1 ArrowLeft Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "propyne",              keys: "c 1 1 ArrowLeft Shift+ArrowRight 3 ArrowRight" },
    Golden { fixture: "purine",               keys: "N a 5 ArrowUp N ArrowLeft ArrowLeft N ArrowDown ArrowRight ArrowRight ArrowDown N ArrowRight ArrowLeft ArrowUp ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "pyrazine",             keys: "N a ArrowRight ArrowRight N" },
    Golden { fixture: "pyrazole",             keys: "N 5 N ArrowRight Shift+ArrowLeft 2 ArrowLeft ArrowLeft ArrowDown Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "pyridine",             keys: "N a" },
    Golden { fixture: "pyrimidine",           keys: "N a ArrowRight N" },
    Golden { fixture: "pyrrole",              keys: "N 5 ArrowLeft ArrowDown Shift+ArrowRight 2 ArrowRight ArrowRight ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "quinoline",            keys: "N a ArrowLeft ArrowRight Shift+ArrowDown a ArrowDown" },
    Golden { fixture: "resorcinol",           keys: "O 1 a ArrowDown 1 O" },
    Golden { fixture: "salicylic_acid",       keys: "O 1 1 a ArrowLeft ArrowRight 1 O ArrowRight ArrowDown ArrowLeft 2" },
    Golden { fixture: "serine",               keys: "N 1 1 1 O ArrowLeft ArrowLeft 1 1 O ArrowRight 2" },
    Golden { fixture: "serotonin",            keys: "O 1 a ArrowDown 5 ArrowLeft ArrowDown 1 1 1 N ArrowLeft ArrowRight ArrowRight ArrowRight ArrowRight N ArrowLeft ArrowLeft Shift+ArrowDown 2 ArrowDown" },
    Golden { fixture: "styrene",              keys: "c a ArrowLeft 1 1 ArrowRight Shift+ArrowLeft 2 ArrowLeft" },
    Golden { fixture: "succinic_acid",        keys: "O 1 1 1 1 1 O ArrowLeft 2 ArrowRight ArrowRight ArrowDown ArrowLeft 2" },
    Golden { fixture: "sulfanilamide",        keys: "N 1 a ArrowDown ArrowRight 1 2 ArrowLeft 2 ArrowRight 1 N ArrowLeft s" },
    Golden { fixture: "sulfate",              keys: "O 1 2 ArrowLeft 2 ArrowRight ArrowUp - ArrowRight 1 O - ArrowLeft s" },
    Golden { fixture: "tert_butanol",         keys: "O 1 1 ArrowLeft 1 ArrowRight 1" },
    Golden { fixture: "tetrahydrofuran",      keys: "O 5" },
    Golden { fixture: "tetramethylammonium",  keys: "N + 1 ArrowLeft 1 ArrowLeft 1 ArrowRight 1" },
    Golden { fixture: "tetrazole",            keys: "N 5 N ArrowRight N ArrowDown N ArrowLeft ArrowLeft Shift+ArrowDown 2 ArrowDown ArrowLeft ArrowRight Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "tfa",                  keys: "O 1 1 1 f ArrowLeft 1 f ArrowLeft 1 f ArrowLeft ArrowLeft 2" },
    Golden { fixture: "thiazole",             keys: "N 5 ArrowRight s Delete ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "thiophene",            keys: "c 5 ArrowRight s Delete ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
    Golden { fixture: "tms_ether",            keys: "O 1 1 ArrowLeft ArrowLeft 1 1 ArrowRight ArrowDown ArrowDown 1 ArrowRight 1 ArrowLeft ` text:Si Enter" },
    Golden { fixture: "toluene",              keys: "c a ArrowLeft 1" },
    Golden { fixture: "triazole",             keys: "N 5 ArrowRight ArrowDown N ArrowRight N ArrowDown ArrowLeft Shift+ArrowRight 2 ArrowRight ArrowLeft ArrowLeft Shift+ArrowRight 2 ArrowRight" },
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

