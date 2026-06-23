use serde::{Deserialize, Serialize};
use super::BondOrder;

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
    pub fn chain(n: usize) -> Self {
        assert!(n >= 1);
        let atoms = (0..=n)
            .map(|i| FragAtom { element: "C".into(), pos: [i as f32, 0.0], charge: 0 })
            .collect();
        let bonds = (0..n)
            .map(|i| FragBond { begin: i, end: i + 1, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("chain{n}"), atoms, bonds, attach_idx: 0 }
    }

    /// Zigzag chain of `n` carbons (alternating ±60° from main axis).
    pub fn zigzag(n: usize) -> Self {
        assert!(n >= 1);
        let angle_step = std::f32::consts::PI / 3.0; // 60°
        let mut atoms: Vec<FragAtom> = vec![FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 }];
        let mut x = 0.0_f32;
        let mut y = 0.0_f32;
        for i in 0..n {
            let dy = if i % 2 == 0 { angle_step.sin() } else { -angle_step.sin() };
            x += angle_step.cos();
            y += dy;
            atoms.push(FragAtom { element: "C".into(), pos: [x, y], charge: 0 });
        }
        let bonds = (0..n)
            .map(|i| FragBond { begin: i, end: i + 1, order: BondOrder::Single })
            .collect();
        Fragment { name: format!("zigzag{n}"), atoms, bonds, attach_idx: 0 }
    }

    // --- Linear substituents ---

    fn lin(name: &str, specs: &[(&str, i8)], orders: &[BondOrder]) -> Self {
        let atoms = specs
            .iter()
            .enumerate()
            .map(|(i, &(el, ch))| FragAtom { element: el.into(), pos: [i as f32, 0.0], charge: ch })
            .collect();
        let bonds = orders
            .iter()
            .enumerate()
            .map(|(i, ord)| FragBond { begin: i, end: i + 1, order: ord.clone() })
            .collect();
        Fragment { name: name.into(), atoms, bonds, attach_idx: 0 }
    }

    pub fn oh() -> Self {
        Self::lin("OH", &[("O", 0), ("H", 0)], &[BondOrder::Single])
    }

    pub fn ome() -> Self {
        Self::lin("OMe", &[("O", 0), ("C", 0)], &[BondOrder::Single])
    }

    pub fn nh2() -> Self {
        Self::lin("NH2", &[("N", 0), ("H", 0)], &[BondOrder::Single])
    }

    pub fn no2() -> Self {
        Fragment {
            name: "NO2".into(),
            atoms: vec![
                FragAtom { element: "N".into(), pos: [0.0, 0.0], charge: 1 },
                FragAtom { element: "O".into(), pos: [1.0, 0.0], charge: 0 },
                FragAtom { element: "O".into(), pos: [-0.5, 0.866], charge: -1 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Double },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    pub fn sh() -> Self {
        Self::lin("SH", &[("S", 0), ("H", 0)], &[BondOrder::Single])
    }

    pub fn sih3() -> Self {
        Fragment {
            name: "SiH3".into(),
            atoms: vec![
                FragAtom { element: "Si".into(), pos: [0.0, 0.0], charge: 0 },
                FragAtom { element: "H".into(),  pos: [1.0, 0.0],  charge: 0 },
                FragAtom { element: "H".into(),  pos: [-0.5, 0.866], charge: 0 },
                FragAtom { element: "H".into(),  pos: [-0.5, -0.866], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Single },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
                FragBond { begin: 0, end: 3, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    pub fn ph2() -> Self {
        Fragment {
            name: "PH2".into(),
            atoms: vec![
                FragAtom { element: "P".into(), pos: [0.0, 0.0], charge: 0 },
                FragAtom { element: "H".into(), pos: [1.0, 0.0], charge: 0 },
                FragAtom { element: "H".into(), pos: [-0.5, 0.866], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Single },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    pub fn phenyl() -> Self { Self::benzene() }

    pub fn fluoro() -> Self {
        Self::lin("F", &[("F", 0)], &[])
    }

    pub fn cf3() -> Self {
        Fragment {
            name: "CF3".into(),
            atoms: vec![
                FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 },
                FragAtom { element: "F".into(), pos: [1.0, 0.0], charge: 0 },
                FragAtom { element: "F".into(), pos: [-0.5, 0.866], charge: 0 },
                FragAtom { element: "F".into(), pos: [-0.5, -0.866], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Single },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
                FragBond { begin: 0, end: 3, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    pub fn bromo() -> Self {
        Self::lin("Br", &[("Br", 0)], &[])
    }

    pub fn chloro() -> Self {
        Self::lin("Cl", &[("Cl", 0)], &[])
    }

    pub fn iodo() -> Self {
        Self::lin("I", &[("I", 0)], &[])
    }

    pub fn hydrogen() -> Self {
        Self::lin("H", &[("H", 0)], &[])
    }

    pub fn deuterium() -> Self {
        Self::lin("D", &[("D", 0)], &[])
    }

    pub fn lithium() -> Self {
        Self::lin("Li", &[("Li", 0)], &[])
    }

    pub fn me() -> Self {
        Self::lin("Me", &[("C", 0)], &[])
    }

    pub fn mgbr() -> Self {
        Self::lin("MgBr", &[("Mg", 0), ("Br", 0)], &[BondOrder::Single])
    }

    pub fn bh2() -> Self {
        Fragment {
            name: "BH2".into(),
            atoms: vec![
                FragAtom { element: "B".into(), pos: [0.0, 0.0], charge: 0 },
                FragAtom { element: "H".into(), pos: [1.0, 0.0], charge: 0 },
                FragAtom { element: "H".into(), pos: [-0.5, 0.866], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Single },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    /// Acetyl group: -C(=O)-CH3
    pub fn acetyl() -> Self {
        Fragment {
            name: "Ac".into(),
            atoms: vec![
                FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 },
                FragAtom { element: "O".into(), pos: [0.0, -1.0], charge: 0 },
                FragAtom { element: "C".into(), pos: [1.0, 0.0], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Double },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    /// Ethyl group: -CH2-CH3
    pub fn ethyl() -> Self {
        Self::lin("Et", &[("C", 0), ("C", 0)], &[BondOrder::Single])
    }

    /// Methyl ester: -C(=O)-O-CH3
    pub fn co2me() -> Self {
        Fragment {
            name: "CO2Me".into(),
            atoms: vec![
                FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 },
                FragAtom { element: "O".into(), pos: [0.0, -1.0], charge: 0 },
                FragAtom { element: "O".into(), pos: [1.0, 0.0], charge: 0 },
                FragAtom { element: "C".into(), pos: [2.0, 0.0], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Double },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
                FragBond { begin: 2, end: 3, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    /// Azide group: -N3
    pub fn azide() -> Self {
        Fragment {
            name: "N3".into(),
            atoms: vec![
                FragAtom { element: "N".into(), pos: [0.0, 0.0], charge: 1 },
                FragAtom { element: "N".into(), pos: [1.0, 0.0], charge: -1 },
                FragAtom { element: "N".into(), pos: [2.0, 0.0], charge: 0 },
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Double },
                FragBond { begin: 1, end: 2, order: BondOrder::Double },
            ],
            attach_idx: 0,
        }
    }

    /// Boc group: -C(=O)-O-C(CH3)3
    pub fn boc() -> Self {
        Fragment {
            name: "Boc".into(),
            atoms: vec![
                FragAtom { element: "C".into(), pos: [0.0, 0.0],    charge: 0 }, // 0: carbonyl C
                FragAtom { element: "O".into(), pos: [0.0, -1.0],   charge: 0 }, // 1: =O
                FragAtom { element: "O".into(), pos: [1.0, 0.0],    charge: 0 }, // 2: -O-
                FragAtom { element: "C".into(), pos: [2.0, 0.0],    charge: 0 }, // 3: tBu quaternary C
                FragAtom { element: "C".into(), pos: [3.0, 0.0],    charge: 0 }, // 4: Me
                FragAtom { element: "C".into(), pos: [2.0, -1.0],   charge: 0 }, // 5: Me
                FragAtom { element: "C".into(), pos: [2.0,  1.0],   charge: 0 }, // 6: Me
            ],
            bonds: vec![
                FragBond { begin: 0, end: 1, order: BondOrder::Double },
                FragBond { begin: 0, end: 2, order: BondOrder::Single },
                FragBond { begin: 2, end: 3, order: BondOrder::Single },
                FragBond { begin: 3, end: 4, order: BondOrder::Single },
                FragBond { begin: 3, end: 5, order: BondOrder::Single },
                FragBond { begin: 3, end: 6, order: BondOrder::Single },
            ],
            attach_idx: 0,
        }
    }

    /// Cbz group: -C(=O)-O-CH2-Ph
    pub fn cbz() -> Self {
        let ph = ring_positions(6);
        let mut atoms = vec![
            FragAtom { element: "C".into(), pos: [0.0, 0.0], charge: 0 }, // 0: carbonyl C
            FragAtom { element: "O".into(), pos: [0.0, -1.0], charge: 0 }, // 1: =O
            FragAtom { element: "O".into(), pos: [1.0, 0.0], charge: 0 }, // 2: -O-
            FragAtom { element: "C".into(), pos: [2.0, 0.0], charge: 0 }, // 3: CH2
        ];
        // offset benzene positions by (3, 0)
        for (i, &p) in ph.iter().enumerate() {
            atoms.push(FragAtom { element: "C".into(), pos: [p[0] + 3.0, p[1]], charge: 0 });
            let _ = i;
        }
        let mut bonds = vec![
            FragBond { begin: 0, end: 1, order: BondOrder::Double },
            FragBond { begin: 0, end: 2, order: BondOrder::Single },
            FragBond { begin: 2, end: 3, order: BondOrder::Single },
            FragBond { begin: 3, end: 4, order: BondOrder::Single }, // CH2 → ring atom 0
        ];
        for k in 0..6 {
            bonds.push(FragBond {
                begin: 4 + k,
                end: 4 + (k + 1) % 6,
                order: if k % 2 == 0 { BondOrder::Single } else { BondOrder::Double },
            });
        }
        Fragment { name: "Cbz".into(), atoms, bonds, attach_idx: 0 }
    }
}

/// Regular n-gon positions: atom[0]=(0,0), atom[1]=(1,0), ring below.
pub fn ring_positions(n: usize) -> Vec<[f32; 2]> {
    use std::f32::consts::{PI, TAU};
    let r = 1.0_f32 / (2.0 * (PI / n as f32).sin());
    let center_y = -(r * r - 0.25_f32).sqrt();
    let cx = 0.5_f32;
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
