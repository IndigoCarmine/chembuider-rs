#!/usr/bin/env python3
"""Generate rendering-test fixtures for chembuider-rs from SMILES via RDKit.

For each entry: SMILES -> kekulized 2D depiction -> uniform scale so the median
bond length is 1.5 (the editor's DEFAULT_BOND_LENGTH) -> y flipped (RDKit is
y-up, the widget's molecule/screen space is y-down) -> ClipMol-shaped JSON in
tests/fixtures/rendering/<name>.json:

    {"atoms": [{"element": str, "pos": [x, y], "charge": int}, ...],
     "bonds": [{"begin": i, "end": i, "order": "Single|Double|Triple",
                "stereo": "None"}, ...]}

`begin`/`end` are indices into `atoms` (the paste-clipboard format of
src/widget/mod.rs). Load in Rust with:
    editor.paste_from_string(&format!("chembuilder-mol:{json}"))

Usage: python3 scripts/gen_test_structures.py   (writes/overwrites the fixtures)
"""

import json
import statistics
from pathlib import Path

from rdkit import Chem
from rdkit.Chem import rdDepictor

TARGET_BOND_LEN = 1.5

# Chosen to exercise: terminal OH/NH2 pointing left AND right (glucose has
# several of each), charges (+/-/2-), wide-ish hetero labels, fused rings,
# hetero-rich rings, and halogens.
STRUCTURES = [
    ("ethanol", "CCO"),
    ("tert_butanol", "OC(C)(C)C"),
    ("methylamine", "CN"),
    ("glycine", "NCC(=O)O"),
    ("acetate", "CC(=O)[O-]"),
    ("tetramethylammonium", "C[N+](C)(C)C"),
    ("sulfate", "[O-]S([O-])(=O)=O"),
    ("ethyl_benzoate", "CCOC(=O)c1ccccc1"),
    ("caffeine", "Cn1cnc2c1c(=O)n(C)c(=O)n2C"),
    ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
    ("glucose_open", "OCC(O)C(O)C(O)C(O)C=O"),
    ("benzenesulfonamide", "NS(=O)(=O)c1ccccc1"),
    ("naphthalene", "c1ccc2ccccc2c1"),
    ("cf3_chlorobenzene", "FC(F)(F)c1ccc(Cl)cc1"),
    ("tms_ether", "C[Si](C)(C)OCC"),
]

ORDER_NAMES = {
    Chem.BondType.SINGLE: "Single",
    Chem.BondType.DOUBLE: "Double",
    Chem.BondType.TRIPLE: "Triple",
}


def to_clipmol(smiles: str) -> dict:
    mol = Chem.MolFromSmiles(smiles)
    if mol is None:
        raise ValueError(f"bad SMILES: {smiles}")
    Chem.Kekulize(mol, clearAromaticFlags=True)
    rdDepictor.Compute2DCoords(mol)
    conf = mol.GetConformer()

    lengths = []
    for b in mol.GetBonds():
        p = conf.GetAtomPosition(b.GetBeginAtomIdx())
        q = conf.GetAtomPosition(b.GetEndAtomIdx())
        lengths.append(((p.x - q.x) ** 2 + (p.y - q.y) ** 2) ** 0.5)
    scale = TARGET_BOND_LEN / statistics.median(lengths) if lengths else 1.0

    atoms = []
    for a in mol.GetAtoms():
        p = conf.GetAtomPosition(a.GetIdx())
        atoms.append({
            "element": a.GetSymbol(),
            # y flipped: RDKit depicts y-up, the widget draws y-down.
            "pos": [round(p.x * scale, 4), round(-p.y * scale, 4)],
            "charge": a.GetFormalCharge(),
        })
    bonds = [
        {
            "begin": b.GetBeginAtomIdx(),
            "end": b.GetEndAtomIdx(),
            "order": ORDER_NAMES[b.GetBondType()],
            "stereo": "None",
        }
        for b in mol.GetBonds()
    ]
    return {"atoms": atoms, "bonds": bonds}


def main() -> None:
    out_dir = Path(__file__).resolve().parent.parent / "tests" / "fixtures" / "rendering"
    out_dir.mkdir(parents=True, exist_ok=True)
    for name, smiles in STRUCTURES:
        data = to_clipmol(smiles)
        (out_dir / f"{name}.json").write_text(json.dumps(data, indent=1) + "\n")
        print(f"{name}: {len(data['atoms'])} atoms, {len(data['bonds'])} bonds")

    # Hand-made degenerate case: two charged, labeled atoms far closer than a
    # normal bond, so the trimmed bond is fully swallowed by the label rects.
    short = {
        "atoms": [
            {"element": "O", "pos": [0.0, 0.0], "charge": -1},
            {"element": "O", "pos": [0.35, 0.0], "charge": -1},
        ],
        "bonds": [{"begin": 0, "end": 1, "order": "Single", "stereo": "None"}],
    }
    (out_dir / "short_bond.json").write_text(json.dumps(short, indent=1) + "\n")
    print("short_bond: hand-made degenerate fixture")


if __name__ == "__main__":
    main()
