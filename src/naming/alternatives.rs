//! The alternative names of one structure that come from its structure rather than from the
//! engine's styles ([`Style`](super::engine::Style)): functional class names, the retained names
//! keyed by structure, additive, conjunctive and [n]annulene names, the amine as a prefix, and
//! cis/trans (OPE-24). Each recogniser matches a narrow, exactly described structure and gives
//! nothing otherwise: an alternative is left out rather than guessed.

use super::NameKind;
use super::engine::Namer;
use super::graph::{self, Class, El, Graph, Perceived};
use super::stereo::{Descriptor, Site, Unit};
use super::words::{alpha_key, enclose, multiplier, multiplier_complex, stem};

/// The alternatives of a neutral structure named `Namer::name` by `namer`; `cited`: whether
/// that name cites a descriptor for every one of `units`.
pub(crate) fn of_structure(
    g: &Graph,
    p: &Perceived,
    units: &[Unit],
    namer: &Namer,
    cited: bool,
) -> Vec<(NameKind, String)> {
    let mut out = Vec::new();
    // A name written without the descriptors would name the compound without its
    // configuration: only cis/trans, which states it, is given for a structure with any.
    if units.iter().any(|u| u.descriptor.is_ok()) {
        if cited && let Some(n) = cis_trans(g, p, units) {
            out.push((NameKind::Systematic, n));
        }
        return out;
    }
    if g.charge.iter().any(|&q| q != 0) {
        return out;
    }
    if !g.free.is_empty() {
        if let Some(n) = substituent_group(g) {
            out.push((NameKind::Retained, n));
        }
        return out;
    }
    let found = [
        radicofunctional(g, p, namer).map(|n| (NameKind::Radicofunctional, n)),
        glycerol(g).map(|n| (NameKind::Retained, n)),
        lactone(g, p, true).map(|n| (NameKind::Retained, n)),
        oxolane(g, p).map(|n| (NameKind::Additive, n)),
        conjunctive(g, p).map(|n| (NameKind::Conjunctive, n)),
        lactone(g, p, false).map(|n| (NameKind::Systematic, n)),
        amino_prefix(g, p).map(|n| (NameKind::Systematic, n)),
        annulene(g, p).map(|n| (NameKind::Systematic, n)),
    ];
    out.extend(found.into_iter().flatten());
    out
}

/// Whether every bond of `a` is single.
fn saturated(g: &Graph, a: usize) -> bool {
    g.adj[a].iter().all(|&(_, o)| o == 1)
}

/// The atoms other than carbon.
fn heteroatoms(g: &Graph) -> Vec<usize> {
    (0..g.len()).filter(|&a| g.el[a] != El::C).collect()
}

/// Functional class names (P-63.1.5 alcohols, P-63.2.1.1 ethers, P-61.3.2 halides: the group,
/// then the class name) and the amine named after its group (P-62.2.1.1.2): "methyl alcohol",
/// "ethyl methyl ether", "butyl chloride", "methylamine". Only for a hydrocarbyl group (the one
/// heteroatom of the structure is the group's), on a saturated carbon for the alcohol, halide
/// and amine (a phenol is not an alcohol), and for the amine only a group whose name needs no
/// enclosing marks ("cyclohexylamine"; "propan-2-ylamine" or "(propan-2-yl)amine"? not decided).
fn radicofunctional(g: &Graph, p: &Perceived, namer: &Namer) -> Option<String> {
    let [x] = heteroatoms(g)[..] else {
        return None;
    };
    if p.in_ring[x] || !saturated(g, x) {
        return None;
    }
    let group = |c: usize| namer.sub(c, x).ok();
    match (g.el[x], &g.adj[x][..]) {
        (El::O, &[(c, _)]) if g.h[x] == 1 && saturated(g, c) => {
            Some(format!("{} alcohol", group(c)?.text))
        }
        (El::O, &[(c, _), (d, _)]) => {
            let (r, s) = (group(c)?, group(d)?);
            if r.text == s.text {
                let two = if r.compound {
                    format!("{}{}", multiplier_complex(2), enclose(&r.text, r.depth))
                } else if r.needs_enclosure() {
                    format!("{}{}", multiplier(2), enclose(&r.text, r.depth))
                } else if r.text.starts_with("tert-") {
                    format!("{}-{}", multiplier(2), r.text)
                } else {
                    format!("{}{}", multiplier(2), r.text)
                };
                return Some(format!("{two} ether"));
            }
            let mut words = [r.text, s.text];
            words.sort_by_key(|w| (alpha_key(w), w.clone()));
            Some(format!("{} {} ether", words[0], words[1]))
        }
        (El::N, &[(c, _)]) if g.h[x] == 2 && saturated(g, c) => {
            let r = group(c)?;
            (!r.needs_enclosure()).then(|| format!("{}amine", r.text))
        }
        (e, &[(c, _)]) if e.is_halogen() && saturated(g, c) => {
            Some(format!("{} {}", group(c)?.text, e.halide_word()))
        }
        _ => None,
    }
}

/// Glycerol (P-63.1.1.2: general nomenclature), propane-1,2,3-triol itself: three carbons in a
/// chain, each with one hydroxy group, and nothing else.
fn glycerol(g: &Graph) -> Option<String> {
    let carbons: Vec<usize> = (0..g.len()).filter(|&a| g.el[a] == El::C).collect();
    let oxygens: Vec<usize> = (0..g.len()).filter(|&a| g.el[a] == El::O).collect();
    let hydroxy = |o: usize| g.h[o] == 1 && matches!(g.adj[o][..], [(c, 1)] if g.el[c] == El::C);
    let one_oh = |c: usize| g.nbrs(c).filter(|&o| g.el[o] == El::O).count() == 1;
    (carbons.len() == 3
        && oxygens.len() == 3
        && g.len() == 6
        && oxygens.iter().all(|&o| hydroxy(o))
        && carbons.iter().all(|&c| one_oh(c) && saturated(g, c)))
    .then(|| "glycerol".to_string())
}

/// The one ring of a structure whose other atoms are `outside` in number: a monocycle (not
/// part of a von Baeyer system), its atoms in cyclic order.
fn only_ring<'p>(g: &Graph, p: &'p Perceived, outside: usize) -> Option<&'p [usize]> {
    match &p.rings[..] {
        [r] if r.poly.is_none() && g.len() == r.atoms.len() + outside => Some(&r.atoms),
        _ => None,
    }
}

/// An unsubstituted saturated lactone or lactam of four to eight ring members (oxetan-2-one …
/// oxocan-2-one): retained, only ε-caprolactone and ε-caprolactam (the seven-membered ones);
/// systematic, the name from the hydroxy or amino acid (P-65.6.3.5.1, P-66.1.5.1):
/// "6-hexanelactone", "6-hexanelactam", "4-butanelactone".
fn lactone(g: &Graph, p: &Perceived, retained: bool) -> Option<String> {
    let ring = only_ring(g, p, 1)?;
    let m = ring.len();
    if !(4..=8).contains(&m) {
        return None;
    }
    let [x] = ring.iter().copied().filter(|&a| g.el[a] != El::C).collect::<Vec<_>>()[..] else {
        return None;
    };
    let lactam = match g.el[x] {
        El::O => false,
        El::N if g.h[x] == 1 => true,
        _ => return None,
    };
    // One ring carbon, next to the heteroatom, is C=O; the others are CH2.
    let oxo = |c: usize| {
        g.adj[c]
            .iter()
            .any(|&(o, k)| k == 2 && g.el[o] == El::O && g.h[o] == 0 && g.adj[o].len() == 1)
    };
    let carbonyls: Vec<usize> = ring.iter().copied().filter(|&c| oxo(c)).collect();
    let [c] = carbonyls[..] else {
        return None;
    };
    let ring_bonds_single = ring
        .iter()
        .all(|&a| g.adj[a].iter().all(|&(b, k)| k == 1 || !ring.contains(&b)));
    if g.order(c, x) != 1
        || !ring_bonds_single
        || !ring
            .iter()
            .all(|&a| a == x || a == c || (g.el[a] == El::C && g.h[a] == 2))
    {
        return None;
    }
    let word = if lactam { "lactam" } else { "lactone" };
    if retained {
        return (m == 7).then(|| format!("ε-capro{word}"));
    }
    let k = m - 1;
    Some(format!("{k}-{}ane{word}", stem(k)?))
}

/// Tetrahydrofuran (P-31.1.4.2.4, an additive name: furan with four added hydrogens), oxolane
/// itself.
fn oxolane(g: &Graph, p: &Perceived) -> Option<String> {
    let ring = only_ring(g, p, 0)?;
    let oxygens = ring.iter().filter(|&&a| g.el[a] == El::O).count();
    let ch2 = ring
        .iter()
        .filter(|&&a| g.el[a] == El::C && g.h[a] == 2 && saturated(g, a))
        .count();
    (ring.len() == 5 && oxygens == 1 && ch2 == 4).then(|| "tetrahydrofuran".to_string())
}

/// Conjunctive names (P-15.6, not used in PINs): an unsubstituted benzene or cycloalkane ring
/// with one unbranched saturated chain ending in the principal group, the ring's name before
/// the chain's: "cyclohexanemethanol", "cyclohexaneethanol", "benzenemethanol" (hydroxy on the
/// first or second carbon of the chain), "cyclohexaneacetic acid".
fn conjunctive(g: &Graph, p: &Perceived) -> Option<String> {
    let [r] = &p.rings[..] else {
        return None;
    };
    if r.poly.is_some() || r.hetero.is_some() {
        return None;
    }
    let ring = &r.atoms;
    let ring_name = if r.benzene {
        "benzene".to_string()
    } else if ring.iter().all(|&a| saturated(g, a)) {
        format!("cyclo{}ane", stem(ring.len())?)
    } else {
        return None;
    };
    // The one ring atom with a neighbour outside the ring, and that neighbour.
    let mut exits = ring.iter().flat_map(|&a| {
        g.nbrs(a)
            .filter(|&b| !p.in_ring[b])
            .map(move |b| (a, b))
    });
    let (a, first) = exits.next()?;
    if exits.next().is_some() || g.el[first] != El::C {
        return None;
    }
    // The chain: carbons from `first`, unbranched.
    let mut chain = vec![first];
    let mut prev = a;
    loop {
        let cur = *chain.last().unwrap();
        let next: Vec<usize> = g
            .nbrs(cur)
            .filter(|&b| b != prev && g.el[b] == El::C)
            .collect();
        match next[..] {
            [] => break,
            [n] if chain.len() < 3 => {
                prev = cur;
                chain.push(n);
            }
            _ => return None,
        }
    }
    let end = *chain.last().unwrap();
    let oxygens: Vec<usize> = g.nbrs(end).filter(|&o| g.el[o] == El::O).collect();
    if g.len() != ring.len() + chain.len() + oxygens.len()
        || chain[..chain.len() - 1].iter().any(|&c| !saturated(g, c) || g.h[c] != 2)
    {
        return None;
    }
    let chain_name = match (chain.len(), &oxygens[..]) {
        (1 | 2, &[o]) if p.hydroxyl[o] && saturated(g, end) && g.h[end] == 2 => {
            ["methanol", "ethanol"][chain.len() - 1]
        }
        (2, &[_, _]) if p.ctype[end] == Some(Class::Acid) => "acetic acid",
        _ => return None,
    };
    Some(format!("{ring_name}{chain_name}"))
}

/// The amines named substitutively with "amino" prefixes on the hydrocarbon (general
/// nomenclature): "aminomethane", "1,2-diaminoethane". Only primary amines (–NH2) on a
/// hydrocarbon; an N-substituted amine is left out.
fn amino_prefix(g: &Graph, p: &Perceived) -> Option<String> {
    let hetero = heteroatoms(g);
    let primary = |n: usize| {
        g.el[n] == El::N && p.amine[n] && g.h[n] == 2 && matches!(g.adj[n][..], [(_, 1)])
    };
    if p.principal != Some(Class::Amine) || hetero.is_empty() || !hetero.iter().all(|&n| primary(n))
    {
        return None;
    }
    let mut q = graph::perceive(g).ok()?;
    q.principal = None;
    let name = Namer::new(g, &q).name().ok()?;
    name.contains("amino").then_some(name)
}

/// [n]annulene (P-22.1.2: general nomenclature) for the unsubstituted fully conjugated
/// monocyclic hydrocarbons CnHn of an even number n ≥ 8 of carbons (benzene is not
/// "[6]annulene"; an odd ring needs indicated hydrogen).
fn annulene(g: &Graph, p: &Perceived) -> Option<String> {
    let ring = only_ring(g, p, 0)?;
    let n = ring.len();
    let conjugated = ring.iter().all(|&a| {
        g.el[a] == El::C && g.h[a] == 1 && g.adj[a].iter().filter(|&&(_, o)| o == 2).count() == 1
    });
    (n >= 8 && n % 2 == 0 && conjugated).then(|| format!("[{n}]annulene"))
}

/// The substituent groups –CH2–CH2– "ethylene" and –CH(CH3)–CH2– "propylene" (P-29.6.2.3:
/// general nomenclature; their PINs ethane-1,2-diyl and propane-1,2-diyl), unsubstituted. Only
/// the groups: propene is not "propylene" here.
fn substituent_group(g: &Graph) -> Option<String> {
    let all_carbon_saturated = (0..g.len()).all(|a| g.el[a] == El::C && saturated(g, a));
    let free: Vec<usize> = g.free.iter().map(|&(a, _)| a).collect();
    if !all_carbon_saturated || g.free.iter().any(|&(_, o)| o != 1) {
        return None;
    }
    let [x, y] = free[..] else {
        return None;
    };
    if x == y || g.order(x, y) != 1 {
        return None;
    }
    match g.len() {
        2 => Some("ethylene".into()),
        // The third carbon on one of the two: –CH(CH3)–CH2–.
        3 => (g.adj[x].len() + g.adj[y].len() == 3).then(|| "propylene".into()),
        _ => None,
    }
}

/// "cis-" or "trans-" for the one stereogenic double bond of a structure whose two atoms each
/// carry one hydrogen: each atom's only other ligand ranks above its hydrogen, so Z is cis and
/// E trans (P-93.5.1.1). Not for esters, whose descriptor stands after the alkyl group.
fn cis_trans(g: &Graph, p: &Perceived, units: &[Unit]) -> Option<String> {
    let [u] = units else {
        return None;
    };
    let Site::Double(a, b) = u.site else {
        return None;
    };
    let one_h = |x: usize| g.h[x] == 1 && g.adj[x].len() == 2;
    if !one_h(a) || !one_h(b) || p.principal == Some(Class::Ester) || g.net_charge() != 0 {
        return None;
    }
    let word = match u.descriptor {
        Ok(Descriptor::Z) => "cis",
        Ok(Descriptor::E) => "trans",
        _ => return None,
    };
    let plain = Namer::new(g, p).name().ok()?;
    Some(format!("{word}-{plain}"))
}
