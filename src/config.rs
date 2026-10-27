use crate::molecule::fragment::{FragAtom, FragBond, Fragment, ring_positions};
use crate::molecule::{BondOrder, BondStereo};
use serde::{Deserialize, Serialize};

// ─── JSON-serializable shortcut/action types ──────────────────────────────────

/// Action that applies to a hovered atom.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AtomAction {
    Fragment { fragment: String },
    Ring { ring: usize },
    Chain { chain: usize },
    Zigzag { zigzag: usize },
    Charge { charge: i8 },
}

/// Action that applies to a hovered bond.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BondAction {
    Stereo { stereo: BondStereo },
    Order { order: BondOrder },
    Ring { ring: usize },
    Fragment { fragment: String },
}

/// Action for the select tool's arrow-key operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SelectionAction {
    MoveUp,
    MoveDown,
    MoveLeft,
    MoveRight,
    RotateCW,
    RotateCCW,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtomShortcut {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(flatten)]
    pub action: AtomAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BondShortcut {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(flatten)]
    pub action: BondAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionShortcut {
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    pub action: SelectionAction,
}

// ─── Fragment definition (JSON-portable mirror of fragment.rs) ────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentDef {
    pub name: String,
    pub atoms: Vec<FragAtomDef>,
    pub bonds: Vec<FragBondDef>,
    pub attach_idx: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragAtomDef {
    pub element: String,
    pub pos: [f32; 2],
    #[serde(default)]
    pub charge: i8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragBondDef {
    pub begin: usize,
    pub end: usize,
    pub order: BondOrder,
}

impl FragmentDef {
    pub fn to_fragment(&self) -> Fragment {
        Fragment {
            name: self.name.clone(),
            atoms: self
                .atoms
                .iter()
                .map(|a| FragAtom {
                    element: a.element.clone(),
                    pos: a.pos,
                    charge: a.charge,
                })
                .collect(),
            bonds: self
                .bonds
                .iter()
                .map(|b| FragBond {
                    begin: b.begin,
                    end: b.end,
                    order: b.order.clone(),
                })
                .collect(),
            attach_idx: self.attach_idx,
        }
    }

    /// [`Fragment::validate`] for this definition — the same check, not a second copy of it.
    pub fn validate(&self) -> Result<(), String> {
        self.to_fragment().validate()
    }
}

// ─── Drawing style (user-tunable appearance) ──────────────────────────────────

/// All values are in screen pixels. Every field has a `#[serde(default)]`, so a user
/// config may set only the parameters they care about (or omit the whole `style` block).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConfig {
    /// Atom label font size.
    #[serde(default = "def_label_size")]
    pub label_size: f32,
    /// Bond line thickness.
    #[serde(default = "def_bond_width")]
    pub bond_width: f32,
    /// Perpendicular spacing between the lines of a double/triple bond.
    #[serde(default = "def_double_bond_offset")]
    pub double_bond_offset: f32,
    /// Base radius of the hover/selection/hotspot rings drawn around atoms.
    /// (Label backgrounds are now tight rectangles; see `label_bg_pad`.)
    #[serde(default = "def_atom_bg_radius")]
    pub atom_bg_radius: f32,
    /// Half-width of the wide end of a stereo wedge (solid/hash).
    #[serde(default = "def_wedge_width")]
    pub wedge_width: f32,
    /// Thickness of a bold/heavy bond.
    #[serde(default = "def_bold_width")]
    pub bold_width: f32,
    /// Dash length for dashed bonds.
    #[serde(default = "def_dash_len")]
    pub dash_len: f32,
    /// Gap length between dashes for dashed bonds.
    #[serde(default = "def_dash_gap")]
    pub dash_gap: f32,
    /// Amplitude of a wavy bond.
    #[serde(default = "def_wavy_amplitude")]
    pub wavy_amplitude: f32,
    /// Gap left between a trimmed bond tip and the label's bounding box.
    #[serde(default = "def_label_trim_pad")]
    pub label_trim_pad: f32,
    /// Padding of the white rectangle painted behind an atom label. Kept below
    /// `label_trim_pad` so the background never overpaints a trimmed bond tip.
    #[serde(default = "def_label_bg_pad")]
    pub label_bg_pad: f32,
}

fn def_label_size() -> f32 { 13.0 }
fn def_bond_width() -> f32 { 1.5 }
fn def_double_bond_offset() -> f32 { 3.5 }
fn def_atom_bg_radius() -> f32 { 9.0 }
fn def_wedge_width() -> f32 { 5.0 }
fn def_bold_width() -> f32 { 4.0 }
fn def_dash_len() -> f32 { 5.0 }
fn def_dash_gap() -> f32 { 4.0 }
fn def_wavy_amplitude() -> f32 { 3.0 }
fn def_label_trim_pad() -> f32 { 2.0 }
fn def_label_bg_pad() -> f32 { 1.0 }

impl StyleConfig {
    /// ChemDraw's drawing style (its ACS-like default: 10 pt labels and 0.6 pt lines on a
    /// 14.4 pt bond), in the editor's pixels: how a structure is drawn for a document, so it
    /// sits beside ones drawn in ChemDraw.
    pub fn chemdraw() -> Self {
        let pt = crate::widget::SCALE_FACTOR * 1.5 / 14.4;
        Self {
            label_size: 10.0 * pt,
            bond_width: 0.6 * pt,
            double_bond_offset: 2.6 * pt,
            atom_bg_radius: def_atom_bg_radius(),
            wedge_width: 1.3 * pt,
            bold_width: 2.0 * pt,
            dash_len: 0.8 * pt,
            dash_gap: 1.2 * pt,
            wavy_amplitude: 1.2 * pt,
            label_trim_pad: 1.2 * pt,
            label_bg_pad: 0.8 * pt,
        }
    }
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            label_size: def_label_size(),
            bond_width: def_bond_width(),
            double_bond_offset: def_double_bond_offset(),
            atom_bg_radius: def_atom_bg_radius(),
            wedge_width: def_wedge_width(),
            bold_width: def_bold_width(),
            dash_len: def_dash_len(),
            dash_gap: def_dash_gap(),
            wavy_amplitude: def_wavy_amplitude(),
            label_trim_pad: def_label_trim_pad(),
            label_bg_pad: def_label_bg_pad(),
        }
    }
}

// ─── Top-level Config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub atom_shortcuts: Vec<AtomShortcut>,
    #[serde(default)]
    pub bond_shortcuts: Vec<BondShortcut>,
    #[serde(default)]
    pub selection_shortcuts: Vec<SelectionShortcut>,
    /// User-tunable drawing appearance (label size, bond width, …).
    #[serde(default)]
    pub style: StyleConfig,
    /// Inline fragments (legacy / override). Normally empty; loaded from fragments/ dir.
    #[serde(default)]
    pub fragments: Vec<FragmentDef>,
}

impl Default for Config {
    /// The built-in defaults compiled into the crate — shortcuts and fragments, but **no file
    /// I/O**. This is what [`ChemStructEditor`](crate::ChemStructEditor) uses unless you assign
    /// it a config of your own. Use [`Config::load`] to additionally read user files from disk.
    fn default() -> Self {
        Self::embedded()
    }
}

impl Config {
    /// Built-in shortcuts and fragments compiled into the crate. Does not touch the filesystem.
    pub fn embedded() -> Self {
        let mut cfg: Config =
            serde_json::from_str(DEFAULT_SHORTCUTS).expect("embedded shortcuts valid");
        cfg.fragments = builtin_fragments();
        cfg
    }

    /// Load shortcuts from `chembuilder_config.json` (or embedded default),
    /// then load fragments from the `fragments/` directory (or embedded defaults).
    pub fn load() -> Self {
        // ── Shortcuts ────────────────────────────────────────────────────────
        let mut cfg: Config = if let Ok(text) = std::fs::read_to_string("chembuilder_config.json") {
            serde_json::from_str::<Config>(&text).unwrap_or_else(|e| {
                eprintln!("Warning: failed to parse chembuilder_config.json: {e}");
                serde_json::from_str(DEFAULT_SHORTCUTS).expect("embedded shortcuts valid")
            })
        } else {
            serde_json::from_str(DEFAULT_SHORTCUTS).expect("embedded shortcuts valid")
        };

        // ── Fragments ────────────────────────────────────────────────────────
        // Priority: fragments/ directory > inline fragments in JSON > embedded defaults
        let inline = std::mem::take(&mut cfg.fragments);
        cfg.fragments = load_fragments(inline, std::path::Path::new("fragments"));
        cfg
    }

    /// Save shortcuts to `chembuilder_config.json` and each fragment to `fragments/<name>.json`.
    ///
    /// Fails with [`InvalidInput`](std::io::ErrorKind::InvalidInput), before writing anything,
    /// if a fragment's name is not a plain file name ([`Fragment::validate_name`]): a name like
    /// `../x` would otherwise write outside `fragments/`. Refusing the whole save (rather than
    /// skipping that fragment) tells the caller, and leaves the files on disk as they were.
    pub fn save(&self) -> std::io::Result<()> {
        self.save_to(std::path::Path::new("."))
    }

    /// [`Config::save`] into `dir` instead of the current directory.
    fn save_to(&self, dir: &std::path::Path) -> std::io::Result<()> {
        for frag in &self.fragments {
            Fragment::validate_name(&frag.name)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        }

        // Shortcuts + style (fragments live in their own files)
        let shortcuts = Config {
            atom_shortcuts: self.atom_shortcuts.clone(),
            bond_shortcuts: self.bond_shortcuts.clone(),
            selection_shortcuts: self.selection_shortcuts.clone(),
            style: self.style.clone(),
            fragments: Vec::new(),
        };
        std::fs::write(
            dir.join("chembuilder_config.json"),
            serde_json::to_string_pretty(&shortcuts)?,
        )?;

        // Individual fragment files (names checked above)
        let frag_dir = dir.join("fragments");
        std::fs::create_dir_all(&frag_dir)?;
        for frag in &self.fragments {
            let path = frag_dir.join(format!("{}.json", frag.name));
            std::fs::write(path, serde_json::to_string_pretty(frag)?)?;
        }
        Ok(())
    }

    /// Resolve a fragment name to a Fragment struct.
    pub fn resolve_fragment(&self, name: &str) -> Option<Fragment> {
        if let Some(def) = self.fragments.iter().find(|f| f.name == name) {
            return Some(def.to_fragment());
        }
        if let Some(ring) = name
            .strip_prefix("ring")
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(ring_of_size)
        {
            return Some(ring);
        }
        if name == "benzene" {
            return Some(Fragment::benzene());
        }
        None
    }

    pub fn atom_action_to_fragment(&self, action: &AtomAction) -> Option<ResolvedAtomAction> {
        match action {
            AtomAction::Fragment { fragment } => self
                .resolve_fragment(fragment)
                .map(ResolvedAtomAction::InsertFragment),
            AtomAction::Ring { ring } => {
                ring_of_size(*ring).map(ResolvedAtomAction::InsertFragment)
            }
            AtomAction::Chain { chain } => Some(ResolvedAtomAction::ExtendChain(*chain, false)),
            AtomAction::Zigzag { zigzag } => Some(ResolvedAtomAction::ExtendChain(*zigzag, true)),
            AtomAction::Charge { charge } => Some(ResolvedAtomAction::ModifyCharge(*charge)),
        }
    }
}

/// Ring sizes a config value may ask for, whether as a `"ringN"` fragment name or a
/// `{"ring": N}` atom shortcut. Both come from user-editable files, so anything outside this
/// range resolves to nothing instead of reaching [`Fragment::ring`], which asserts `n >= 3`.
const RING_SIZES: std::ops::RangeInclusive<usize> = 3..=12;

/// The n-membered carbocycle, if `n` is a ring size config values may ask for.
fn ring_of_size(n: usize) -> Option<Fragment> {
    RING_SIZES.contains(&n).then(|| Fragment::ring(n))
}

/// Resolved atom action ready for the editor to execute.
pub enum ResolvedAtomAction {
    InsertFragment(Fragment),
    ExtendChain(usize, bool),
    ModifyCharge(i8),
}

// ─── Fragment loading ─────────────────────────────────────────────────────────

/// Embedded fragment JSON strings (compile-time, one per file in assets/fragments/).
const BUILTIN_FRAGMENT_STRS: &[&str] = &[
    include_str!("../assets/fragments/Oxo.json"),
    include_str!("../assets/fragments/N.json"),
    include_str!("../assets/fragments/O.json"),
    include_str!("../assets/fragments/OH.json"),
    include_str!("../assets/fragments/OMe.json"),
    include_str!("../assets/fragments/NH2.json"),
    include_str!("../assets/fragments/NO2.json"),
    include_str!("../assets/fragments/SH.json"),
    include_str!("../assets/fragments/SiH3.json"),
    include_str!("../assets/fragments/PH2.json"),
    include_str!("../assets/fragments/F.json"),
    include_str!("../assets/fragments/CF3.json"),
    include_str!("../assets/fragments/Cl.json"),
    include_str!("../assets/fragments/Br.json"),
    include_str!("../assets/fragments/I.json"),
    include_str!("../assets/fragments/H.json"),
    include_str!("../assets/fragments/D.json"),
    include_str!("../assets/fragments/Li.json"),
    include_str!("../assets/fragments/Me.json"),
    include_str!("../assets/fragments/Et.json"),
    include_str!("../assets/fragments/MgBr.json"),
    include_str!("../assets/fragments/BH2.json"),
    include_str!("../assets/fragments/Ac.json"),
    include_str!("../assets/fragments/CO2Me.json"),
    include_str!("../assets/fragments/N3.json"),
    include_str!("../assets/fragments/Boc.json"),
    include_str!("../assets/fragments/Cbz.json"),
    include_str!("../assets/fragments/benzene.json"),
    include_str!("../assets/fragments/BPin.json"),
    include_str!("../assets/fragments/TMS.json"),
    include_str!("../assets/fragments/TBS.json"),
    include_str!("../assets/fragments/Ts.json"),
    include_str!("../assets/fragments/Benz.json"),
    include_str!("../assets/fragments/TEG.json"),
    include_str!("../assets/fragments/C4H9.json"),
    include_str!("../assets/fragments/C5H11.json"),
    include_str!("../assets/fragments/C6H13.json"),
    include_str!("../assets/fragments/C7H15.json"),
    include_str!("../assets/fragments/C8H17.json"),
    include_str!("../assets/fragments/C9H19.json"),
    include_str!("../assets/fragments/C10H21.json"),
    include_str!("../assets/fragments/C11H23.json"),
    include_str!("../assets/fragments/C12H25.json"),
    include_str!("../assets/fragments/Ph.json"),
];

/// Parse one fragment's JSON and reject it if [`check_loaded`] fails. Used for the built-in
/// strings and `fragments/*.json`; inline fragments arrive already parsed with the config and
/// are checked in [`load_fragments`].
fn parse_fragment_def(text: &str) -> Result<FragmentDef, String> {
    let def = serde_json::from_str::<FragmentDef>(text).map_err(|e| format!("{e}"))?;
    check_loaded(&def)?;
    Ok(def)
}

/// What a loaded fragment must pass to enter the config: [`FragmentDef::validate`] (its
/// indices fit), and a name that [`Config::save`] can write as `fragments/<name>.json`
/// ([`Fragment::validate_name`]) — so a shared file with `"name": "../x"` is turned away here
/// instead of being written back outside `fragments/` on the next save.
fn check_loaded(def: &FragmentDef) -> Result<(), String> {
    def.validate()?;
    Fragment::validate_name(&def.name)
}

/// Parse the compile-time built-in fragments. No file I/O.
fn builtin_fragments() -> Vec<FragmentDef> {
    BUILTIN_FRAGMENT_STRS
        .iter()
        .filter_map(|s| parse_fragment_def(s).ok())
        .collect()
}

/// Load fragments: embedded defaults as base, then merge inline + runtime dir (override by name).
/// This ensures built-in fragments are always available even when a runtime `fragments/` dir exists.
fn load_fragments(inline: Vec<FragmentDef>, dir: &std::path::Path) -> Vec<FragmentDef> {
    let mut frags = builtin_fragments();
    let inline = inline
        .into_iter()
        .filter(|f| match check_loaded(f) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("Warning: skip fragment {:?} in chembuilder_config.json: {e}", f.name);
                false
            }
        })
        .collect();
    merge_fragments(&mut frags, inline);
    merge_fragments(&mut frags, load_fragments_from_dir(dir));
    frags
}

fn merge_fragments(base: &mut Vec<FragmentDef>, additions: Vec<FragmentDef>) {
    for frag in additions {
        match base.iter_mut().find(|f| f.name == frag.name) {
            Some(slot) => *slot = frag,
            None => base.push(frag),
        }
    }
}

/// Scan a directory for `*.json` fragment files, sorted alphabetically.
fn load_fragments_from_dir(dir: &std::path::Path) -> Vec<FragmentDef> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .flatten()
        .filter(|e| e.path().extension().map_or(false, |x| x == "json"))
        .map(|e| e.path())
        .collect();
    paths.sort();
    let mut out = Vec::with_capacity(paths.len());
    for path in paths {
        match std::fs::read_to_string(&path)
            .map_err(|e| format!("{e}"))
            .and_then(|s| parse_fragment_def(&s))
        {
            Ok(frag) => out.push(frag),
            Err(e) => eprintln!("Warning: skip {:?}: {e}", path),
        }
    }
    out
}

// ─── Embedded default shortcuts (no fragments section) ────────────────────────

const DEFAULT_SHORTCUTS: &str = include_str!("../assets/default_config.json");

// ─── Helper ───────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn ring_fragment_def(n: usize) -> FragmentDef {
    let positions = ring_positions(n);
    let atoms = positions
        .iter()
        .map(|&p| FragAtomDef {
            element: "C".into(),
            pos: p,
            charge: 0,
        })
        .collect();
    let bonds = (0..n)
        .map(|k| FragBondDef {
            begin: k,
            end: (k + 1) % n,
            order: BondOrder::Single,
        })
        .collect();
    FragmentDef {
        name: format!("ring{n}"),
        atoms,
        bonds,
        attach_idx: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        // Never `Config::load` — that reads the developer's own config and `fragments/`.
        Config::embedded()
    }

    fn ring_action(ring: usize) -> Option<ResolvedAtomAction> {
        cfg().atom_action_to_fragment(&AtomAction::Ring { ring })
    }

    /// `ring` comes from the user-editable `chembuilder_config.json`; an out-of-range size
    /// must resolve to no action rather than reach `Fragment::ring`'s `assert!(n >= 3)`.
    #[test]
    fn ring_shortcut_outside_valid_sizes_resolves_to_none() {
        for ring in [0, 1, 2, 13, 1000] {
            assert!(ring_action(ring).is_none(), "ring {ring}");
        }
    }

    /// The `{"ring": N}` shortcut and the `"ringN"` fragment name accept the same sizes.
    #[test]
    fn ring_shortcut_and_ring_name_accept_the_same_sizes() {
        for n in 0..=20 {
            let by_action = match ring_action(n) {
                Some(ResolvedAtomAction::InsertFragment(f)) => Some(f.atoms.len()),
                Some(_) => panic!("ring {n} must insert a fragment"),
                None => None,
            };
            let by_name = cfg()
                .resolve_fragment(&format!("ring{n}"))
                .map(|f| f.atoms.len());
            assert_eq!(by_action, by_name, "ring {n}");
        }
    }

    /// A C–O fragment named `name` whose attach index and bond endpoints are the caller's.
    fn two_atom_def(name: &str, attach_idx: usize, begin: usize, end: usize) -> FragmentDef {
        let atom = |element: &str, x: f32| FragAtomDef { element: element.into(), pos: [x, 0.0], charge: 0 };
        FragmentDef {
            name: name.into(),
            atoms: vec![atom("C", 0.0), atom("O", 1.5)],
            bonds: vec![FragBondDef { begin, end, order: BondOrder::Single }],
            attach_idx,
        }
    }

    /// A fresh `fragments/`-style directory holding `defs`, one `<name>.json` each.
    fn fragment_dir(tag: &str, defs: &[FragmentDef]) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("chembuilder-fragments-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for def in defs {
            let text = serde_json::to_string_pretty(def).unwrap();
            std::fs::write(dir.join(format!("{}.json", def.name)), text).unwrap();
        }
        dir
    }

    /// A hand-edited `fragments/*.json` with an index past its atoms is skipped by the loader.
    #[test]
    fn fragment_dir_loader_skips_out_of_range_indices() {
        let dir = fragment_dir(
            "skip",
            &[
                two_atom_def("BadAttach", 5, 0, 1),
                two_atom_def("BadBond", 0, 0, 99),
                two_atom_def("Good", 0, 0, 1),
                two_atom_def("SelfBond", 0, 1, 1),
            ],
        );
        let names: Vec<String> = load_fragments_from_dir(&dir).into_iter().map(|f| f.name).collect();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(names, ["Good"]);
    }

    /// Whatever the user puts in `fragments/` or inline in `chembuilder_config.json`, every
    /// fragment the config can hand out indexes safely — so `insert_fragment`, `to_molecule`
    /// and the editor's `frag.atoms[frag.attach_idx]` never see an out-of-range index.
    #[test]
    fn invalid_user_fragments_never_reach_insertion() {
        let dir = fragment_dir("insert", &[two_atom_def("BadBond", 0, 0, 99), two_atom_def("OH", 7, 0, 1)]);
        let inline = vec![two_atom_def("InlineBad", 3, 0, 1), two_atom_def("Me", 0, 0, 42)];
        let cfg = Config { fragments: load_fragments(inline, &dir), ..Config::embedded() };
        std::fs::remove_dir_all(&dir).unwrap();

        for name in ["BadBond", "InlineBad"] {
            assert!(cfg.resolve_fragment(name).is_none(), "{name}");
        }
        // A bad override leaves the built-in of the same name in place.
        for name in ["OH", "Me"] {
            assert_eq!(cfg.resolve_fragment(name).unwrap().attach_idx, 0, "{name}");
        }
        for def in &cfg.fragments {
            let action = AtomAction::Fragment { fragment: def.name.clone() };
            let Some(ResolvedAtomAction::InsertFragment(frag)) = cfg.atom_action_to_fragment(&action) else {
                panic!("{} must resolve to a fragment", def.name);
            };
            assert!(frag.atoms.get(frag.attach_idx).is_some(), "{}", def.name);
            assert_eq!(frag.to_molecule().atoms.len(), frag.atoms.len(), "{}", def.name);
            let mut mol = crate::molecule::Molecule::default();
            let anchor = mol.add_atom("C".into(), [0.0, 0.0], 0);
            mol.insert_fragment(&frag, anchor, 0.0, false);
        }
    }

    /// A fragment named `../evil` (e.g. from a shared JSON file) must not make `save` write
    /// `<dir>/evil.json`; the save is refused before anything is written.
    #[test]
    fn save_refuses_a_fragment_name_that_leaves_fragments_dir() {
        let root = fragment_dir("save-escape", &[]);
        let cfg = Config {
            fragments: vec![two_atom_def("Good", 0, 0, 1), two_atom_def("../evil", 0, 0, 1)],
            ..Config::embedded()
        };
        let result = cfg.save_to(&root);
        let written = walk(&root);
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
        assert!(written.is_empty(), "a refused save wrote {written:?}");
    }

    /// Every file under `dir`, recursively.
    fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
        out
    }

    /// With valid names, `save` writes each fragment as `fragments/<name>.json` under `dir`,
    /// and loading that directory back yields the same fragments.
    #[test]
    fn save_writes_each_fragment_into_fragments_dir() {
        let root = fragment_dir("save-ok", &[]);
        let cfg = Config { fragments: vec![two_atom_def("Good", 0, 0, 1)], ..Config::embedded() };
        cfg.save_to(&root).unwrap();
        let names: Vec<String> =
            load_fragments_from_dir(&root.join("fragments")).into_iter().map(|f| f.name).collect();
        let config_written = root.join("chembuilder_config.json").is_file();
        std::fs::remove_dir_all(&root).unwrap();

        assert!(config_written);
        assert_eq!(names, ["Good"]);
    }

    /// A fragment whose name `save` would refuse is skipped on load, from `fragments/*.json`
    /// and inline alike, so it never enters the config.
    #[test]
    fn loader_skips_fragments_whose_name_is_not_a_file_name() {
        let dir = fragment_dir("bad-name", &[two_atom_def("Good", 0, 0, 1)]);
        let text = serde_json::to_string_pretty(&two_atom_def("../evil", 0, 0, 1)).unwrap();
        std::fs::write(dir.join("shared.json"), text).unwrap();
        let inline = vec![two_atom_def("a/b", 0, 0, 1), two_atom_def("NUL", 0, 0, 1)];
        let frags = load_fragments(inline, &dir);
        std::fs::remove_dir_all(&dir).unwrap();

        let names: Vec<&str> = frags.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"Good"));
        for bad in ["../evil", "a/b", "NUL"] {
            assert!(!names.contains(&bad), "{bad} must be skipped");
        }
    }

    /// Every compiled-in fragment survives the same parse-and-validate step as a user file.
    #[test]
    fn every_builtin_fragment_is_valid() {
        for text in BUILTIN_FRAGMENT_STRS {
            let def: FragmentDef = serde_json::from_str(text).unwrap();
            def.validate().unwrap_or_else(|e| panic!("{}: {e}", def.name));
        }
        assert_eq!(builtin_fragments().len(), BUILTIN_FRAGMENT_STRS.len());
    }

    /// The element symbols the writers treat as group labels are exactly the built-in fragments
    /// named by an element symbol that are not that element on its own ("Ac" is acetyl; "Br" is
    /// bromine).
    #[test]
    fn group_labels_are_the_fragments_named_like_an_element() {
        use crate::molecule::elements::{atomic_number, GROUP_LABELS};
        let mut groups: Vec<String> = builtin_fragments()
            .iter()
            .filter(|def| atomic_number(&def.name).is_some())
            .filter(|def| {
                let frag = def.to_fragment();
                !(frag.atoms.len() == 1 && frag.atoms[0].element == def.name)
            })
            .map(|def| def.name.clone())
            .collect();
        groups.sort();
        assert_eq!(groups, GROUP_LABELS);
    }
}
