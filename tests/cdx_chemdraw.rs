//! What the installed ChemDraw reads from CDX written here: each structure is written to
//! `target/chemdraw_check/`, opened in ChemDraw over COM (`scripts/e2e/chemdraw-check.ps1`), and
//! ChemDraw's own SMILES of it compared with the chemistry drawn.
//!
//! Needs ChemDraw (Windows), so it is ignored by default:
//!   cargo test --test cdx_chemdraw -- --ignored --nocapture

use chembuider_rs::molecule::cdx::molecule_to_cdx_document;
use chembuider_rs::{BondOrder, BondStereo, Molecule};
use std::path::PathBuf;

fn bond(mol: &mut Molecule, a: u32, b: u32, order: BondOrder, stereo: BondStereo) {
    let id = mol.add_bond(a, b, order);
    mol.bond_by_id_mut(id).unwrap().stereo = stereo;
}

/// D-alanine zwitterion: the nitrogen on a wedge above the stereocentre, the methyl lower left and
/// the carboxylate lower right make it (R); a charge on each end, and the hydrogens of NH3+ only
/// in its label.
fn alanine() -> Molecule {
    let mut mol = Molecule::default();
    let ca = mol.add_atom("C".into(), [0.0, 0.0], 0);
    let me = mol.add_atom("C".into(), [-1.3, 0.75], 0);
    let n = mol.add_atom("N".into(), [0.0, -1.5], 1);
    let c = mol.add_atom("C".into(), [1.3, 0.75], 0);
    let o1 = mol.add_atom("O".into(), [2.6, 0.0], 0);
    let o2 = mol.add_atom("O".into(), [1.3, 2.25], -1);
    bond(&mut mol, ca, me, BondOrder::Single, BondStereo::None);
    bond(&mut mol, ca, n, BondOrder::Single, BondStereo::WedgeUp);
    bond(&mut mol, ca, c, BondOrder::Single, BondStereo::None);
    bond(&mut mol, c, o1, BondOrder::Double, BondStereo::None);
    bond(&mut mol, c, o2, BondOrder::Single, BondStereo::None);
    mol
}

/// Elements past calcium, a triple bond, two structures, and a hashed wedge.
fn assorted() -> Molecule {
    let mut mol = Molecule::default();
    let c = mol.add_atom("C".into(), [0.0, 0.0], 0);
    let n = mol.add_atom("N".into(), [1.5, 0.0], 0);
    bond(&mut mol, c, n, BondOrder::Triple, BondStereo::None);
    let se = mol.add_atom("Se".into(), [-1.3, 0.75], 0);
    bond(&mut mol, c, se, BondOrder::Single, BondStereo::None);
    let me = mol.add_atom("C".into(), [-2.6, 0.0], 0);
    bond(&mut mol, se, me, BondOrder::Single, BondStereo::None);
    // Ethyl bromide with a hashed bond to the bromine, apart from the first structure.
    let c1 = mol.add_atom("C".into(), [0.0, 4.0], 0);
    let c2 = mol.add_atom("C".into(), [1.3, 4.75], 0);
    let br = mol.add_atom("Br".into(), [2.6, 4.0], 0);
    bond(&mut mol, c1, c2, BondOrder::Single, BondStereo::None);
    bond(&mut mol, c2, br, BondOrder::Single, BondStereo::WedgeDown);
    mol
}

/// Labels that are no element: abbreviations ChemDraw knows (Ph, CO2Et, tBu, with their
/// markup) and one it does not.
fn labels() -> Molecule {
    let mut mol = Molecule::default();
    let c = mol.add_atom("C".into(), [0.0, 0.0], 0);
    let ph = mol.add_atom("Ph".into(), [-1.3, 0.75], 0);
    let ester = mol.add_atom("CO_2Et".into(), [1.3, 0.75], 0);
    let tbu = mol.add_atom("//t//Bu".into(), [0.0, -1.5], 0);
    let x = mol.add_atom("Xyz".into(), [0.0, 3.0], 0);
    let c2 = mol.add_atom("C".into(), [1.3, 3.75], 0);
    bond(&mut mol, c, ph, BondOrder::Single, BondStereo::None);
    bond(&mut mol, c, ester, BondOrder::Single, BondStereo::None);
    bond(&mut mol, c, tbu, BondOrder::Single, BondStereo::None);
    bond(&mut mol, x, c2, BondOrder::Single, BondStereo::None);
    mol
}

fn chemdraw_reads(files: &[PathBuf]) -> Vec<(usize, usize, String)> {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/e2e/chemdraw-check.ps1");
    let out = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-File", script])
        .args(files)
        .arg("-Cdxml")
        .output()
        .expect("pwsh runs");
    let text = String::from_utf8_lossy(&out.stdout);
    eprintln!("{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "ChemDraw check failed");
    text.lines()
        .map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f[1].parse().unwrap(), f[2].parse().unwrap(), f.get(3).unwrap_or(&"").to_string())
        })
        .collect()
}

#[test]
#[ignore = "needs ChemDraw installed (Windows COM)"]
fn chemdraw_reads_what_we_write() {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("chemdraw_check");
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [("alanine", alanine()), ("assorted", assorted()), ("labels", labels())];
    let files: Vec<PathBuf> = cases
        .iter()
        .map(|(name, mol)| {
            let path = dir.join(format!("{name}.cdx"));
            std::fs::write(&path, molecule_to_cdx_document(mol)).unwrap();
            path
        })
        .collect();
    let read = chemdraw_reads(&files);

    // ChemDraw counts the atoms it shows: the hydrogens inside labels are not among them.
    let (atoms, bonds, smiles) = &read[0];
    assert_eq!((*atoms, *bonds), (6, 5));
    assert_eq!(smiles, "C[C@H](C([O-])=O)[NH3+]", "charges, hydrogens and the (R) centre as drawn");

    let (atoms, bonds, smiles) = &read[1];
    assert_eq!((*atoms, *bonds), (7, 5));
    let mut parts: Vec<&str> = smiles.split('.').collect();
    parts.sort();
    assert_eq!(parts, ["CCBr", "N#C[Se]C"]);

    // ChemDraw knows the abbreviations as what they stand for; an unknown label stays a label.
    let (_, _, smiles) = &read[2];
    let mut parts: Vec<&str> = smiles.split('.').collect();
    parts.sort();
    assert_eq!(parts, ["O=C(C(C(C)(C)C)C1=CC=CC=C1)OCC", "[Xyz]C"]);
}
