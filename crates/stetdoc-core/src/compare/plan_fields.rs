//! Fields that span paragraphs in the plan (design `compare.md`, 4.6 and 5.3): a table of contents is one unit, so neither side of a
//! comparison ever has half of one.

use super::model::{Body, Container};
use super::plan_para::Reader;
use super::tables::ItemPair;
use std::collections::{HashMap, HashSet};

/// A field that spans paragraphs (a TOC) is one unit. If any paragraph of a group differs, or a group has a partner that is not
/// a whole group of the other side, every paragraph of the groups concerned is left unpaired, so neither side has half a field.
pub(super) fn dissolve_field_groups(
    ca: &Container,
    cb: &Container,
    pairs: &[ItemPair],
    reader: &mut Reader<'_>,
) -> Vec<ItemPair> {
    let group_of = |c: &Container, i: usize| match c.blocks.get(i).map(|n| &n.body) {
        Some(Body::Para(p)) => p.group,
        _ => None,
    };
    let any = ca
        .blocks
        .iter()
        .any(|n| matches!(&n.body, Body::Para(p) if p.group.is_some()))
        || cb
            .blocks
            .iter()
            .any(|n| matches!(&n.body, Body::Para(p) if p.group.is_some()));
    if !any {
        return pairs.to_vec();
    }
    let size = |c: &Container, g: u32| {
        (0..c.blocks.len())
            .filter(|&i| group_of(c, i) == Some(g))
            .count()
    };
    let mut live: Vec<bool> = vec![true; pairs.len()];
    let mut bad_a: HashSet<u32> = HashSet::new();
    let mut bad_b: HashSet<u32> = HashSet::new();
    loop {
        let mut changed = false;
        let mut seen_a: HashMap<u32, usize> = HashMap::new();
        let mut seen_b: HashMap<u32, usize> = HashMap::new();
        for (k, p) in pairs.iter().enumerate() {
            if !live.get(k).copied().unwrap_or(false) {
                continue;
            }
            let (ga, gb) = (group_of(ca, p.a), group_of(cb, p.b));
            if ga.is_none() && gb.is_none() {
                continue;
            }
            let equal = match (ga, gb, ca.blocks.get(p.a), cb.blocks.get(p.b)) {
                (Some(_), Some(_), Some(x), Some(y)) => reader.pair(x, y).is_empty(),
                _ => false,
            };
            let mut kill = !equal;
            if let Some(g) = ga {
                *seen_a.entry(g).or_default() += 1;
                kill |= bad_a.contains(&g);
            }
            if let Some(g) = gb {
                *seen_b.entry(g).or_default() += 1;
                kill |= bad_b.contains(&g);
            }
            if kill {
                if let Some(g) = ga {
                    bad_a.insert(g);
                }
                if let Some(g) = gb {
                    bad_b.insert(g);
                }
                if let Some(l) = live.get_mut(k) {
                    *l = false;
                }
                changed = true;
            }
        }
        // a group with a member that is not paired is not whole
        for (g, n) in &seen_a {
            if *n != size(ca, *g) {
                changed |= bad_a.insert(*g);
            }
        }
        for (g, n) in &seen_b {
            if *n != size(cb, *g) {
                changed |= bad_b.insert(*g);
            }
        }
        for i in 0..ca.blocks.len() {
            if let Some(g) = group_of(ca, i)
                && !pairs.iter().any(|p| p.a == i)
            {
                changed |= bad_a.insert(g);
            }
        }
        for i in 0..cb.blocks.len() {
            if let Some(g) = group_of(cb, i)
                && !pairs.iter().any(|p| p.b == i)
            {
                changed |= bad_b.insert(g);
            }
        }
        if !changed {
            break;
        }
        for (k, p) in pairs.iter().enumerate() {
            let hit = group_of(ca, p.a).is_some_and(|g| bad_a.contains(&g))
                || group_of(cb, p.b).is_some_and(|g| bad_b.contains(&g));
            if hit && let Some(l) = live.get_mut(k) {
                *l = false;
            }
        }
    }
    pairs
        .iter()
        .zip(live)
        .filter(|(_, l)| *l)
        .map(|(p, _)| p.clone())
        .collect()
}
