use serde::{Deserialize, Serialize};
use super::{BondOrder, Molecule};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragAtom {
    pub element: String,
    pub pos: [f32; 2],
    #[serde(default)]
    pub charge: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragBond {
    pub begin: usize,
    pub end: usize,
    pub order: BondOrder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    #[allow(dead_code)]
    pub name: String,
    pub atoms: Vec<FragAtom>,
    pub bonds: Vec<FragBond>,
    pub attach_idx: usize,
}

/// Fragments the crate builds in code rather than reading from JSON.
///
/// These are the *parametric* ones — an n-gon or an n-carbon chain is a formula, not a table of
/// coordinates, so there is nothing here for a JSON file to duplicate. Every named group
/// (OH, NH₂, CF₃, SiH₃, Boc, Cbz, …) lives in `assets/fragments/*.json` and reaches the editor
/// through [`Config::resolve_fragment`](crate::Config::resolve_fragment), which consults its
/// fragment list *first* and only falls through to [`Fragment::ring`] and [`Fragment::benzene`].
///
/// Hand-written copies of those groups used to sit here as well, and being unreachable is
/// exactly how they went wrong: they were still spaced 1.0 apart long after the JSON moved to
/// [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH), and nothing failed, because
/// nothing called them. They are gone; `assets/fragments/` is the single source.
#[allow(dead_code)]
impl Fragment {
    /// Regular n-membered carbocycle.
    pub fn ring(n: usize) -> Self {
        assert!(n >= 3);
        let positions = ring_positions(n);
        let atoms = positions
            .iter()
            .map(|&pos| FragAtom { element: "C".into(), pos, charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|k| FragBond { begin: k, end: (k + 1) % n, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("ring{n}"), atoms, bonds, attach_idx: 0 }
    }

    /// Benzene with alternating single/double bonds.
    pub fn benzene() -> Self {
        let n = 6;
        let positions = ring_positions(n);
        let atoms = positions
            .iter()
            .map(|&pos| FragAtom { element: "C".into(), pos, charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|k| FragBond {
                begin: k,
                end: (k + 1) % n,
                order: if k % 2 == 0 { BondOrder::Single } else { BondOrder::Double },
            })
            .collect();
        Fragment { name: "benzene".into(), atoms, bonds, attach_idx: 0 }
    }

    /// Linear chain of `n` carbons. atom[0] is attach point at origin.
    ///
    /// Spaced by [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH), like everything
    /// else the editor draws — fragment coordinates are absolute molecular units and are never
    /// rescaled on insertion, so a unit step here would draw a chain two thirds the size of the
    /// ring beside it.
    pub fn chain(n: usize) -> Self {
        assert!(n >= 1);
        let l = crate::widget::DEFAULT_BOND_LENGTH;
        let atoms = (0..=n)
            .map(|i| FragAtom { element: "C".into(), pos: [i as f32 * l, 0.0], charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|i| FragBond { begin: i, end: i + 1, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("chain{n}"), atoms, bonds, attach_idx: 0 }
    }

    /// Zigzag chain of `n` carbons (alternating ±60° from main axis), at
    /// [`DEFAULT_BOND_LENGTH`](crate::widget::DEFAULT_BOND_LENGTH) per bond.
    pub fn zigzag(n: usize) -> Self {
        assert!(n >= 1);
        let l = crate::widget::DEFAULT_BOND_LENGTH;
        let angle_step = std::f32::consts::PI / 3.0; // 60°
        let mut atoms: Vec<FragAtom> = vec![FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 }];
        let mut x = 0.0_f32;
        let mut y = 0.0_f32;
        for i in 0..n {
            let dy = if i % 2 == 0 { angle_step.sin() } else { -angle_step.sin() };
            x += angle_step.cos() * l;
            y += dy * l;
            atoms.push(FragAtom { element: "C".into(), pos: [x, y], charge: 0 });
        }
        let bonds = (0..n)
            .map(|i| FragBond { begin: i, end: i + 1, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("zigzag{n}"), atoms, bonds, attach_idx: 0 }
    }
}

// ─── JSON I/O and Molecule interop ────────────────────────────────────────────

impl Fragment {
    /// Check that every index in the fragment points at one of its atoms: there is at least one
    /// atom, `attach_idx` is one of them, and each bond joins two distinct atoms that exist.
    ///
    /// Fragment JSON is user-editable, and the code that places a fragment
    /// ([`Molecule::insert_fragment`], [`Fragment::to_molecule`], the editor) indexes `atoms` by
    /// these values directly. They are checked once, here, where the JSON is read
    /// ([`Fragment::from_json`] and [`Config`](crate::Config)'s fragment loading), so that a
    /// fragment which got past loading is safe to index.
    pub fn validate(&self) -> Result<(), String> {
        let n = self.atoms.len();
        if n == 0 {
            return Err("fragment has no atoms".into());
        }
        if self.attach_idx >= n {
            return Err(format!("attach_idx {} is out of range for {n} atoms", self.attach_idx));
        }
        for (k, b) in self.bonds.iter().enumerate() {
            if b.begin >= n || b.end >= n {
                return Err(format!(
                    "bond {k} ({} -> {}) refers to an atom out of range for {n} atoms",
                    b.begin, b.end
                ));
            }
            if b.begin == b.end {
                return Err(format!("bond {k} joins atom {} to itself", b.begin));
            }
        }
        Ok(())
    }

    /// Check that `name` can be used as is for the file `fragments/<name>.json` on every
    /// platform, so the file lands inside `fragments/` and nowhere else.
    ///
    /// The name is user data (typed into the editor, or the `"name"` field of a shared fragment
    /// JSON), and the code that saves fragments builds their path from it. Refused: the empty
    /// name; path separators (`/`, `\`), `:` (a Windows drive or stream) and `..`; control
    /// characters; a trailing `.` or whitespace (Windows drops them, so the file would not be
    /// the one named); and the Windows device names `CON`, `PRN`, `AUX`, `NUL`, `COM1`–`COM9`,
    /// `LPT1`–`LPT9`, also with an extension (`NUL.x`), in any case.
    ///
    /// Every place that writes `fragments/<name>.json` calls this first, and fragments whose
    /// name fails it are skipped when loading.
    pub fn validate_name(name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("fragment name is empty".into());
        }
        if let Some(c) = name.chars().find(|&c| matches!(c, '/' | '\\' | ':') || c.is_control()) {
            return Err(format!("fragment name {name:?} contains {c:?}, which is not allowed in a file name"));
        }
        if name.contains("..") {
            return Err(format!("fragment name {name:?} contains \"..\""));
        }
        if name.ends_with(|c: char| c == '.' || c.is_whitespace()) {
            return Err(format!("fragment name {name:?} ends with a dot or whitespace"));
        }
        let device = name.split('.').next().unwrap_or(name).trim_end().to_ascii_uppercase();
        let numbered = |prefix: &str| {
            device.strip_prefix(prefix).is_some_and(|d| matches!(d.as_bytes(), [b'1'..=b'9']))
        };
        if matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL") || numbered("COM") || numbered("LPT") {
            return Err(format!("fragment name {name:?} is a reserved device name on Windows"));
        }
        Ok(())
    }

    /// Parse a fragment from its JSON representation (the on-disk `fragments/<name>.json` format).
    /// A fragment that parses but fails [`Fragment::validate`] is an error.
    pub fn from_json(s: &str) -> serde_json::Result<Self> {
        let frag: Self = serde_json::from_str(s)?;
        frag.validate().map_err(<serde_json::Error as serde::de::Error>::custom)?;
        Ok(frag)
    }

    /// Pretty-print the fragment as JSON, matching the on-disk fragment format.
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Load a fragment from a JSON file.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        Self::from_json(&text).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save the fragment to a JSON file.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }

    /// Materialize this fragment as a standalone [`Molecule`]. Atoms are added in fragment order,
    /// so `molecule.atoms[i]` corresponds to `self.atoms[i]` (and the attach atom is at
    /// `self.attach_idx`).
    pub fn to_molecule(&self) -> Molecule {
        let mut mol = Molecule::default();
        mol.name = self.name.clone();
        let ids: Vec<u32> = self
            .atoms
            .iter()
            .map(|a| mol.add_atom(a.element.clone(), a.pos, a.charge))
            .collect();
        for b in &self.bonds {
            mol.add_bond(ids[b.begin], ids[b.end], b.order.clone());
        }
        mol
    }

    /// Rebuild a fragment from a molecule (e.g. after `cleanup_2d`), taking each atom's element,
    /// position and charge and re-indexing bond endpoints by atom order. `attach_idx` is carried
    /// over unchanged, so pass the same value used to build the molecule.
    pub fn from_molecule(mol: &Molecule, attach_idx: usize) -> Self {
        let index_of: std::collections::HashMap<u32, usize> =
            mol.atoms.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        let atoms = mol
            .atoms
            .iter()
            .map(|a| FragAtom { element: a.element.clone(), pos: a.pos, charge: a.charge })
            .collect();
        let bonds = mol
            .bonds
            .iter()
            .filter_map(|b| {
                Some(FragBond {
                    begin: *index_of.get(&b.begin)?,
                    end: *index_of.get(&b.end)?,
                    order: b.order.clone(),
                })
            })
            .collect();
        Fragment { name: mol.name.clone(), atoms, bonds, attach_idx }
    }
}

/// Which of a fragment's atoms sit on one of its rings.
///
/// Leaves are pruned until none are left; whatever survives is on a cycle. Used by
/// [`Molecule::fragment_atoms_lost_to_snap`](crate::Molecule::fragment_atoms_lost_to_snap) to
/// tell a wanted ring fusion from a substituent that would be silently re-rooted.
pub fn ring_membership(frag: &Fragment) -> Vec<bool> {
    let n = frag.atoms.len();
    let mut alive = vec![true; n];
    let mut degree = vec![0usize; n];
    for b in &frag.bonds {
        if b.begin < n && b.end < n && b.begin != b.end {
            degree[b.begin] += 1;
            degree[b.end] += 1;
        }
    }
    loop {
        let Some(leaf) = (0..n).find(|&i| alive[i] && degree[i] <= 1) else { break };
        alive[leaf] = false;
        degree[leaf] = 0;
        for b in &frag.bonds {
            if b.begin >= n || b.end >= n || b.begin == b.end {
                continue;
            }
            let other = if b.begin == leaf {
                b.end
            } else if b.end == leaf {
                b.begin
            } else {
                continue;
            };
            if alive[other] {
                degree[other] = degree[other].saturating_sub(1);
            }
        }
    }
    alive
}

/// Regular n-gon positions: atom[0]=(0,0), atom[1]=(L,0), ring below, where L is
/// [`DEFAULT_BOND_LENGTH`].
///
/// The edge length is not decorative. Fragment coordinates are *absolute* molecular units —
/// [`Molecule::insert_fragment`] places a fragment on an atom by translation and rotation
/// only, with no scaling — so whatever this returns is the size the ring appears on screen.
/// Every hand-authored fragment (`assets/fragments/*.json`) spaces its bonds by 1.5, and so
/// do chains and [`crate::widget::interact`]'s ring fusion; a **unit** n-gon here therefore
/// drew ring-key rings at two thirds the size of everything beside them.
pub fn ring_positions(n: usize) -> Vec<[f32; 2]> {
    use std::f32::consts::{PI, TAU};
    let l = crate::widget::DEFAULT_BOND_LENGTH;
    let r = l / (2.0 * (PI / n as f32).sin());
    let center_y = -(r * r - l * l * 0.25).sqrt();
    let cx = l * 0.5;
    let cy = center_y;
    let alpha_0 = (0.0_f32 - cy).atan2(0.0_f32 - cx);
    (0..n)
        .map(|k| {
            let angle = alpha_0 - k as f32 * TAU / n as f32;
            [cx + r * angle.cos(), cy + r * angle.sin()]
        })
        .collect()
}

#[allow(dead_code)]
impl BondOrder {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Double" => BondOrder::Double,
            "Triple" => BondOrder::Triple,
            _ => BondOrder::Single,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A C–O fragment whose attach index and single bond's endpoints are the caller's.
    fn two_atom_json(attach_idx: usize, begin: usize, end: usize) -> String {
        format!(
            r#"{{"name":"bad","atoms":[{{"element":"C","pos":[0.0,0.0]}},{{"element":"O","pos":[1.5,0.0]}}],"bonds":[{{"begin":{begin},"end":{end},"order":"Single"}}],"attach_idx":{attach_idx}}}"#
        )
    }

    /// Every index in fragment JSON is user data; one that points past `atoms` must be refused
    /// when the JSON is read, not panic later in `to_molecule` / `insert_fragment`.
    #[test]
    fn from_json_rejects_indices_that_do_not_fit_the_atoms() {
        let cases = [
            ("bond end out of range", two_atom_json(0, 0, 99)),
            ("bond begin out of range", two_atom_json(0, 99, 1)),
            ("attach_idx out of range", two_atom_json(5, 0, 1)),
            ("bond joins an atom to itself", two_atom_json(0, 1, 1)),
            ("no atoms", r#"{"name":"empty","atoms":[],"bonds":[],"attach_idx":0}"#.to_string()),
        ];
        for (what, text) in cases {
            assert!(Fragment::from_json(&text).is_err(), "{what}");
        }
        assert!(Fragment::from_json(&two_atom_json(1, 0, 1)).is_ok());
    }

    #[test]
    fn load_reports_a_bad_fragment_file_as_invalid_data() {
        let path = std::env::temp_dir()
            .join(format!("chembuilder-bad-fragment-{}.json", std::process::id()));
        std::fs::write(&path, two_atom_json(0, 0, 99)).unwrap();
        let result = Fragment::load(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    /// The validation must not turn away any fragment the crate ships.
    #[test]
    fn every_bundled_fragment_passes_validation() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fragments");
        let mut count = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|x| x == "json") {
                let frag = Fragment::load(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                assert_eq!(frag.to_molecule().atoms.len(), frag.atoms.len(), "{path:?}");
                count += 1;
            }
        }
        assert!(count > 0, "no fragments found in {dir:?}");
    }

    #[test]
    fn validate_name_refuses_names_that_are_not_a_plain_file_name() {
        let bad = [
            "", "../evil", "..", "a..b", "a/b", "/abs", "a\\b", "C:x", "x:stream", "tab\there",
            "nl\n", "nul\0", "trailing.", "trailing ", "trailing\t", "CON", "con", "Prn", "AUX",
            "NUL", "NUL.x", "nul.tar.gz", "CON .x", "COM1", "com9", "LPT1", "lpt9.json",
        ];
        for name in bad {
            assert!(Fragment::validate_name(name).is_err(), "{name:?} must be refused");
        }
        let good = [
            "OH", "CO2Me", "C12H25", "benzene", "ring6", "my group", "N-Boc", "a.b", "COM",
            "COM0", "COM10", "LPT", "CONx", "Nul2", ".hidden", "Ph",
        ];
        for name in good {
            assert!(Fragment::validate_name(name).is_ok(), "{name:?} must be accepted");
        }
    }

    /// Every fragment the crate ships (and the parametric ones it builds) has a name that the
    /// savers accept, so saving the config never refuses a built-in.
    #[test]
    fn every_shipped_fragment_name_is_a_valid_file_name() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut count = 0;
        for dir in ["assets/fragments", "fragments"] {
            for entry in std::fs::read_dir(root.join(dir)).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().is_some_and(|x| x == "json") {
                    let frag = Fragment::load(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                    Fragment::validate_name(&frag.name).unwrap_or_else(|e| panic!("{path:?}: {e}"));
                    count += 1;
                }
            }
        }
        assert!(count > 0, "no fragments found");
        for frag in [Fragment::ring(3), Fragment::ring(12), Fragment::benzene(), Fragment::chain(4), Fragment::zigzag(4)] {
            Fragment::validate_name(&frag.name).unwrap_or_else(|e| panic!("{e}"));
        }
    }
}
