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

# A broad set (~100) exercising: terminal OH/NH2 pointing both ways, charges
# (+/-/2-), wide hetero labels, fused/hetero rings, halogens, triple bonds,
# sulfur/phosphorus, and real drug scaffolds. Any SMILES that fails to parse is
# skipped with a warning rather than aborting the whole run.
STRUCTURES = [
    # ── small molecules & functional groups ──
    ("ethanol", "CCO"),
    ("tert_butanol", "OC(C)(C)C"),
    ("methylamine", "CN"),
    ("ethylene_glycol", "OCCO"),
    ("acetone", "CC(=O)C"),
    ("acetaldehyde", "CC=O"),
    ("acetic_acid", "CC(=O)O"),
    ("acetamide", "CC(=O)N"),
    ("acetonitrile", "CC#N"),
    ("propyne", "CC#C"),
    ("dimethyl_ether", "COC"),
    ("dimethyl_sulfoxide", "CS(=O)C"),
    ("dimethyl_sulfone", "CS(=O)(=O)C"),
    ("nitromethane", "C[N+](=O)[O-]"),
    ("urea", "NC(=O)N"),
    ("guanidine", "NC(=N)N"),
    ("formamide", "O=CN"),
    ("carbamate", "NC(=O)OC"),
    ("trimethylamine", "CN(C)C"),
    ("diethyl_ether", "CCOCC"),
    ("ethanethiol", "CCS"),
    ("dimethyl_disulfide", "CSSC"),
    ("chloroform", "C(Cl)(Cl)Cl"),
    ("carbon_tetrachloride", "ClC(Cl)(Cl)Cl"),
    ("tfa", "OC(=O)C(F)(F)F"),
    ("glycine", "NCC(=O)O"),
    ("alanine", "CC(N)C(=O)O"),
    ("serine", "OCC(N)C(=O)O"),
    ("lactic_acid", "CC(O)C(=O)O"),
    ("oxalic_acid", "OC(=O)C(=O)O"),
    ("malonic_acid", "OC(=O)CC(=O)O"),
    ("succinic_acid", "OC(=O)CCC(=O)O"),
    ("dimethyl_carbonate", "COC(=O)OC"),
    # ── charged species ──
    ("acetate", "CC(=O)[O-]"),
    ("tetramethylammonium", "C[N+](C)(C)C"),
    ("sulfate", "[O-]S([O-])(=O)=O"),
    ("phosphate", "OP(=O)([O-])[O-]"),
    ("nitrate", "[O-][N+](=O)[O-]"),
    ("carbonate", "[O-]C(=O)[O-]"),
    ("guanidinium", "NC(=[NH2+])N"),
    ("ammonium_acetate", "CC(=O)[O-].[NH4+]"),
    ("bicarbonate", "OC(=O)[O-]"),
    ("methoxide", "C[O-]"),
    # ── aromatic & heteroaromatic ──
    ("benzene", "c1ccccc1"),
    ("toluene", "Cc1ccccc1"),
    ("phenol", "Oc1ccccc1"),
    ("aniline", "Nc1ccccc1"),
    ("benzoic_acid", "OC(=O)c1ccccc1"),
    ("nitrobenzene", "O=[N+]([O-])c1ccccc1"),
    ("styrene", "C=Cc1ccccc1"),
    ("benzaldehyde", "O=Cc1ccccc1"),
    ("anisole", "COc1ccccc1"),
    ("ethyl_benzoate", "CCOC(=O)c1ccccc1"),
    ("benzenesulfonamide", "NS(=O)(=O)c1ccccc1"),
    ("benzonitrile", "N#Cc1ccccc1"),
    ("cf3_chlorobenzene", "FC(F)(F)c1ccc(Cl)cc1"),
    ("catechol", "Oc1ccccc1O"),
    ("resorcinol", "Oc1cccc(O)c1"),
    ("mesitylene", "Cc1cc(C)cc(C)c1"),
    ("pyridine", "c1ccncc1"),
    ("pyrimidine", "c1cncnc1"),
    ("pyrazine", "c1cnccn1"),
    ("pyrrole", "c1cc[nH]c1"),
    ("furan", "c1ccoc1"),
    ("thiophene", "c1ccsc1"),
    ("imidazole", "c1c[nH]cn1"),
    ("oxazole", "c1ocnc1"),
    ("thiazole", "c1cscn1"),
    ("pyrazole", "c1cc[nH]n1"),
    ("triazole", "c1nc[nH]n1"),
    ("tetrazole", "c1nnn[nH]1"),
    ("indole", "c1ccc2[nH]ccc2c1"),
    ("quinoline", "c1ccc2ncccc2c1"),
    ("isoquinoline", "c1ccc2cnccc2c1"),
    ("benzofuran", "c1ccc2occc2c1"),
    ("benzothiophene", "c1ccc2sccc2c1"),
    ("benzimidazole", "c1ccc2[nH]cnc2c1"),
    ("purine", "c1nc2[nH]cnc2cn1"),
    ("naphthalene", "c1ccc2ccccc2c1"),
    ("anthracene", "c1ccc2cc3ccccc3cc2c1"),
    ("biphenyl", "c1ccc(-c2ccccc2)cc1"),
    ("carbazole", "c1ccc2c(c1)[nH]c1ccccc12"),
    ("dibenzofuran", "c1ccc2c(c1)oc1ccccc12"),
    # ── rings, aliphatic ──
    ("cyclohexane", "C1CCCCC1"),
    ("cyclopropane", "C1CC1"),
    ("cyclopentanone", "O=C1CCCC1"),
    ("piperidine", "C1CCNCC1"),
    ("piperazine", "C1CNCCN1"),
    ("morpholine", "C1COCCN1"),
    ("tetrahydrofuran", "C1CCOC1"),
    ("gamma_butyrolactone", "O=C1CCCO1"),
    ("epoxide", "C1CO1"),
    ("dioxane", "C1COCCO1"),
    ("cyclohexene", "C1CCCC=C1"),
    # ── drugs & natural products ──
    ("aspirin", "CC(=O)Oc1ccccc1C(=O)O"),
    ("paracetamol", "CC(=O)Nc1ccc(O)cc1"),
    ("ibuprofen", "CC(C)Cc1ccc(C(C)C(=O)O)cc1"),
    ("caffeine", "Cn1cnc2c1c(=O)n(C)c(=O)n2C"),
    ("nicotine", "CN1CCCC1c1cccnc1"),
    ("dopamine", "NCCc1ccc(O)c(O)c1"),
    ("serotonin", "NCCc1c[nH]c2ccc(O)cc12"),
    ("histamine", "NCCc1c[nH]cn1"),
    ("salicylic_acid", "OC(=O)c1ccccc1O"),
    ("phenylalanine", "NC(Cc1ccccc1)C(=O)O"),
    ("tryptophan", "NC(Cc1c[nH]c2ccccc12)C(=O)O"),
    ("proline", "OC(=O)C1CCCN1"),
    ("cysteine", "NC(CS)C(=O)O"),
    ("methionine", "CSCCC(N)C(=O)O"),
    ("glucose_open", "OCC(O)C(O)C(O)C(O)C=O"),
    ("glucose_ring", "OCC1OC(O)C(O)C(O)C1O"),
    ("fructose", "OCC(=O)C(O)C(O)C(O)CO"),
    ("citric_acid", "OC(=O)CC(O)(C(=O)O)CC(=O)O"),
    ("sulfanilamide", "Nc1ccc(S(N)(=O)=O)cc1"),
    ("acetanilide", "CC(=O)Nc1ccccc1"),
    ("vanillin", "COc1cc(C=O)ccc1O"),
    ("coumarin", "O=c1ccc2ccccc2o1"),
    ("indomethacin_core", "Cc1c(CC(=O)O)c2ccccc2n1C(=O)c1ccc(Cl)cc1"),
    # ── organosilicon / phosphorus / triple bonds ──
    ("tms_ether", "C[Si](C)(C)OCC"),
    ("triethyl_phosphate", "CCOP(=O)(OCC)OCC"),
    ("triphenylphosphine", "c1ccc(P(c2ccccc2)c2ccccc2)cc1"),
    ("acetylene_diol", "OC#CO"),
    ("propargyl_alcohol", "OCC#C"),
    ("diphenylacetylene", "c1ccc(C#Cc2ccccc2)cc1"),
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
    ok = 0
    for name, smiles in STRUCTURES:
        try:
            data = to_clipmol(smiles)
        except Exception as e:  # noqa: BLE001 - skip a bad SMILES, keep going
            print(f"SKIP {name}: {e}")
            continue
        (out_dir / f"{name}.json").write_text(json.dumps(data, indent=1) + "\n")
        ok += 1
    print(f"wrote {ok}/{len(STRUCTURES)} structures")

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

    # Hand-made bent alkyne: 2-butyne drawn with a ~90 deg kink at each sp carbon.
    # Cleanup should straighten both triple-bond carbons back to ~180 deg (before the
    # sp-linear fix, the degree-2 angle term pulled them toward 120 deg instead).
    bent = {
        "atoms": [
            {"element": "C", "pos": [0.0, 0.0], "charge": 0},
            {"element": "C", "pos": [1.5, 0.0], "charge": 0},
            {"element": "C", "pos": [1.5, 1.5], "charge": 0},
            {"element": "C", "pos": [3.0, 1.5], "charge": 0},
        ],
        "bonds": [
            {"begin": 0, "end": 1, "order": "Single", "stereo": "None"},
            {"begin": 1, "end": 2, "order": "Triple", "stereo": "None"},
            {"begin": 2, "end": 3, "order": "Single", "stereo": "None"},
        ],
    }
    (out_dir / "bent_alkyne.json").write_text(json.dumps(bent, indent=1) + "\n")
    print("bent_alkyne: hand-made bent triple-bond fixture")


if __name__ == "__main__":
    main()
