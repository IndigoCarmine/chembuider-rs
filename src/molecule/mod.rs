pub mod mol2;

#[derive(Debug, Clone, PartialEq)]
pub enum BondOrder {
    Single,
    Double,
    Triple,
}

impl BondOrder {
    pub fn cycle(&self) -> Self {
        match self {
            BondOrder::Single => BondOrder::Double,
            BondOrder::Double => BondOrder::Triple,
            BondOrder::Triple => BondOrder::Single,
        }
    }

    pub fn mol2_type(&self) -> &'static str {
        match self {
            BondOrder::Single => "1",
            BondOrder::Double => "2",
            BondOrder::Triple => "3",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Atom {
    pub id: u32,
    pub element: String,
    pub pos: [f32; 2],
    pub charge: i8,
}

#[derive(Debug, Clone)]
pub struct Bond {
    pub id: u32,
    pub begin: u32,
    pub end: u32,
    pub order: BondOrder,
}

#[derive(Debug, Clone)]
pub struct Molecule {
    pub name: String,
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    next_id: u32,
}

impl Default for Molecule {
    fn default() -> Self {
        Self {
            name: "MOL".to_string(),
            atoms: Vec::new(),
            bonds: Vec::new(),
            next_id: 1,
        }
    }
}

impl Molecule {
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add_atom(&mut self, element: String, pos: [f32; 2], charge: i8) -> u32 {
        let id = self.alloc_id();
        self.atoms.push(Atom { id, element, pos, charge });
        id
    }

    pub fn add_bond(&mut self, begin: u32, end: u32, order: BondOrder) -> u32 {
        if self.bond_between(begin, end).is_some() {
            return 0;
        }
        let id = self.alloc_id();
        self.bonds.push(Bond { id, begin, end, order });
        id
    }

    pub fn remove_atom(&mut self, id: u32) {
        self.atoms.retain(|a| a.id != id);
        self.bonds.retain(|b| b.begin != id && b.end != id);
    }

    pub fn remove_bond(&mut self, id: u32) {
        self.bonds.retain(|b| b.id != id);
    }

    pub fn atom_by_id(&self, id: u32) -> Option<&Atom> {
        self.atoms.iter().find(|a| a.id == id)
    }

    pub fn atom_by_id_mut(&mut self, id: u32) -> Option<&mut Atom> {
        self.atoms.iter_mut().find(|a| a.id == id)
    }

    pub fn bond_by_id_mut(&mut self, id: u32) -> Option<&mut Bond> {
        self.bonds.iter_mut().find(|b| b.id == id)
    }

    pub fn bonds_for_atom(&self, atom_id: u32) -> Vec<&Bond> {
        self.bonds
            .iter()
            .filter(|b| b.begin == atom_id || b.end == atom_id)
            .collect()
    }

    pub fn bond_between(&self, a: u32, b: u32) -> Option<&Bond> {
        self.bonds.iter().find(|bond| {
            (bond.begin == a && bond.end == b) || (bond.begin == b && bond.end == a)
        })
    }

    pub fn neighbor_atom_ids(&self, atom_id: u32) -> Vec<u32> {
        self.bonds_for_atom(atom_id)
            .iter()
            .map(|b| if b.begin == atom_id { b.end } else { b.begin })
            .collect()
    }
}
