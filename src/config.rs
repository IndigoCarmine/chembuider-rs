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
}

// ─── Top-level Config ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub atom_shortcuts: Vec<AtomShortcut>,
    #[serde(default)]
    pub bond_shortcuts: Vec<BondShortcut>,
    #[serde(default)]
    pub selection_shortcuts: Vec<SelectionShortcut>,
    /// Inline fragments (legacy / override). Normally empty; loaded from fragments/ dir.
    #[serde(default)]
    pub fragments: Vec<FragmentDef>,
}

impl Config {
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
        cfg.fragments = load_fragments(inline);
        cfg
    }

    /// Save shortcuts to `chembuilder_config.json` and each fragment to `fragments/<name>.json`.
    pub fn save(&self) -> std::io::Result<()> {
        // Shortcuts only (fragments live in their own files)
        let shortcuts = Config {
            atom_shortcuts: self.atom_shortcuts.clone(),
            bond_shortcuts: self.bond_shortcuts.clone(),
            selection_shortcuts: self.selection_shortcuts.clone(),
            fragments: Vec::new(),
        };
        std::fs::write(
            "chembuilder_config.json",
            serde_json::to_string_pretty(&shortcuts)?,
        )?;

        // Individual fragment files
        std::fs::create_dir_all("fragments")?;
        for frag in &self.fragments {
            let path = format!("fragments/{}.json", frag.name);
            std::fs::write(path, serde_json::to_string_pretty(frag)?)?;
        }
        Ok(())
    }

    /// Resolve a fragment name to a Fragment struct.
    pub fn resolve_fragment(&self, name: &str) -> Option<Fragment> {
        if let Some(def) = self.fragments.iter().find(|f| f.name == name) {
            return Some(def.to_fragment());
        }
        if let Some(size) = name
            .strip_prefix("ring")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if (3..=12).contains(&size) {
                return Some(Fragment::ring(size));
            }
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
                Some(ResolvedAtomAction::InsertFragment(Fragment::ring(*ring)))
            }
            AtomAction::Chain { chain } => Some(ResolvedAtomAction::ExtendChain(*chain, false)),
            AtomAction::Zigzag { zigzag } => Some(ResolvedAtomAction::ExtendChain(*zigzag, true)),
            AtomAction::Charge { charge } => Some(ResolvedAtomAction::ModifyCharge(*charge)),
        }
    }
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
];

/// Load fragments: embedded defaults as base, then merge inline + runtime dir (override by name).
/// This ensures built-in fragments are always available even when a runtime `fragments/` dir exists.
fn load_fragments(inline: Vec<FragmentDef>) -> Vec<FragmentDef> {
    let mut frags: Vec<FragmentDef> = BUILTIN_FRAGMENT_STRS
        .iter()
        .filter_map(|s| serde_json::from_str::<FragmentDef>(s).ok())
        .collect();
    merge_fragments(&mut frags, inline);
    merge_fragments(
        &mut frags,
        load_fragments_from_dir(std::path::Path::new("fragments")),
    );
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
            .and_then(|s| serde_json::from_str::<FragmentDef>(&s).map_err(|e| format!("{e}")))
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
