//! What a transaction created, and the post-conditions on it (design `revisions.md`, 2.2 and 7.3).
//!
//! The compiler keeps no list of what it wrote. Every revision id comes from the `Tx`'s counter, so
//! `finish` walks the touched and the new blocks for revision elements that this transaction created: its
//! author, a minted id, and not the new id of a split. Then it checks the Word rules on them with
//! `check::scan_unit`. A failure is `internal`: it is a bug in the compiler, and the batch is discarded.

use super::compiler::Compiler;
use super::errors::internal;
use super::outcome::Created;
use super::policy::is_created;
use crate::api::error::StetError;
use crate::schema::ViolationKey;
use crate::schema::is_unit_boundary;
use crate::wordml::BlockKey;
use crate::wordml::revision_at;
use crate::xml::{NodeId, SpanTree};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The elements that the compiler and the resolver rename, as `(in text, in a deletion)`: a deletion makes
/// `w:t` into `w:delText` and `w:instrText` into `w:delInstrText`, and rejecting it makes them back. They are
/// the only renames there are. The element keeps its attributes and its children, so a violation it carries
/// (python-docx writes `w:space` where `xml:space` belongs, in 1 of 33 documents of one corpus tier) goes
/// with it, and the edit did not add it.
pub(crate) const RENAMES: [(&str, &str); 2] = [("t", "delText"), ("instrText", "delInstrText")];

/// The key of a violation with the names of renamed elements put back to the one they have in text (see
/// [`RENAMES`]), so that a problem that was in the document is not a new problem because the compiler or the
/// resolver renamed the element that carries it. The rule for any comparison of keys before and after an
/// edit: compare these.
pub(crate) fn key_through_renames(key: &ViolationKey) -> ViolationKey {
    let put_back = |text: &str| {
        let mut out = text.to_owned();
        for (plain, deleted) in RENAMES {
            out = out.replace(&format!("w:{deleted}"), &format!("w:{plain}"));
        }
        out
    };
    ViolationKey {
        kind: key.kind,
        parent: put_back(&key.parent),
        subject: put_back(&key.subject),
        extra: key.extra.clone(),
    }
}

/// The revision elements this transaction created, in part order and document order of the blocks.
pub(super) fn created(c: &Compiler<'_, '_>) -> Result<Vec<Created>, StetError> {
    let Some(stamp) = c.stamp() else {
        return Ok(Vec::new());
    };
    let minted = c.tx.minted_annotation_ids();
    if minted.is_empty() {
        return Ok(Vec::new());
    }
    let view = c.tx.parts();
    let mut blocks: Vec<BlockKey> = c.tx.touched().to_vec();
    for fx in c.effects_so_far() {
        blocks.extend(fx.new_blocks.iter().cloned());
    }
    let mut seen_blocks: HashSet<BlockKey> = HashSet::new();
    let mut seen_nodes = HashSet::new();
    let mut out = Vec::new();
    for key in blocks {
        if !seen_blocks.insert(key.clone()) {
            continue;
        }
        let Some(part) = view.id(key.part()) else {
            return Err(internal(format!("{} is not a part", key.part())));
        };
        // A block that was removed again leaves nothing behind.
        if !view.is_attached(&key)? {
            continue;
        }
        let tree = view.tree(part)?;
        let mut stack = vec![tree.node_ref(key.node())];
        while let Some(n) = stack.pop() {
            if !n.is_element() {
                continue;
            }
            if seen_nodes.insert((key.part().clone(), n.id()))
                && let Some(r) = revision_at(&n)
                && let Some(id) = r.id
                && is_created(
                    r.author.as_deref(),
                    id,
                    stamp,
                    &minted,
                    c.tx.copied_annotation_ids(),
                )
            {
                out.push(Created {
                    kind: r.kind,
                    id,
                    part: key.part().clone(),
                    node: n.id(),
                });
            }
            let mark = stack.len();
            stack.extend(n.child_elements());
            if let Some(tail) = stack.get_mut(mark..) {
                tail.reverse();
            }
        }
    }
    Ok(out)
}

/// Checks the Word rules on what the transaction created, with `check::scan_unit`, so the rules have one
/// implementation. Each unit that holds a created revision is scanned, and a finding counts when it is at a
/// created revision or below one and the part had no finding with its key before the transaction: a problem
/// elsewhere in the unit, or one the input had that a new revision now wraps, is `check`'s to carry. A new
/// problem whose key the part already had elsewhere is `check::edit`'s to find, unit by unit, at commit. The schema's rules (W4, W6, half of W5) and those read over a part or a story (W3, W7, W8) are
/// `check::edit`'s, at commit. A failure is a bug in the compiler: the message names the rule and the
/// revision, and the batch is discarded.
pub(super) fn check_created(
    c: &Compiler<'_, '_>,
    created: &[super::outcome::Created],
) -> Result<(), StetError> {
    let view = c.tx.parts();
    let mut problems: Vec<String> = Vec::new();
    let mut by_part: BTreeMap<&str, HashMap<NodeId, &Created>> = BTreeMap::new();
    for item in created {
        by_part
            .entry(&item.part)
            .or_default()
            .insert(item.node, item);
    }
    for (name, mine) in by_part {
        let Some(part) = view.id(name) else {
            return Err(internal(format!("{name} is not a part")));
        };
        let tree = view.tree(part)?;
        let mut units: Vec<NodeId> = Vec::new();
        let mut seen: HashSet<NodeId> = HashSet::new();
        // In document order of the created revisions, so the message is the same each time.
        for item in created.iter().filter(|x| &*x.part == name) {
            let unit = unit_of(tree, item.node);
            if seen.insert(unit) {
                units.push(unit);
            }
        }
        // The keys the part had before the transaction, read only when a finding needs them: first those of
        // the same unit as the base had it, which is where a defect that a new revision wraps or a copy
        // carries almost always comes from (Word leaves an empty `w:rPr` on runs), and which costs the unit;
        // then, only for a key the unit did not have, those of the whole part.
        let mut unit_before: HashMap<NodeId, HashSet<crate::check::Key>> = HashMap::new();
        let mut before: Option<HashSet<crate::check::Key>> = None;
        for unit in units {
            for found in crate::check::scan_unit(tree, unit) {
                let Some(item) = found
                    .node
                    .and_then(|n| created_at_or_above(tree, n, unit, &mine))
                else {
                    continue;
                };
                let in_unit = match unit_before.entry(unit) {
                    std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(base_unit_keys(c, name, unit)?)
                    }
                };
                if in_unit.contains(&found.key) {
                    continue;
                }
                let had = match &before {
                    Some(keys) => keys,
                    None => before.insert(base_keys(c, name)?),
                };
                if had.contains(&found.key) {
                    continue;
                }
                problems.push(format!(
                    "{} (revision {}, {:?})",
                    found.message, item.id, item.kind
                ));
            }
        }
    }
    // The compiler's own ids: `Tx::next_annotation_id` never gives one twice.
    let mut ids: Vec<i64> = created.iter().map(|x| x.id).collect();
    ids.sort_unstable();
    if ids.windows(2).any(|w| w.first() == w.get(1)) {
        problems.push("two revisions created by one transaction share an id".to_owned());
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(internal(format!(
            "the compiler wrote a revision that breaks a Word rule: {}",
            problems.join("; ")
        )))
    }
}

/// The keys of the Word rules in a part as the transaction's base has it: a problem the input had is carried
/// into a revision the compiler wraps around it (LibreOffice writes empty `w:rPr` on every run), and is not
/// the compiler's. A part the base does not have has none.
fn base_keys(c: &Compiler<'_, '_>, name: &str) -> Result<HashSet<crate::check::Key>, StetError> {
    let view = c.tx.base().parts();
    let Some(part) = view.id(name) else {
        return Ok(HashSet::new());
    };
    let tree = view.tree(part)?;
    Ok(crate::check::scan_part(tree)
        .into_iter()
        .map(|f| f.key)
        .collect())
}

/// The keys of the Word rules in one unit as the transaction's base has it, or none when the base did not have
/// the unit (a block the transaction made: node ids are never reused, so its id is no node of the base).
fn base_unit_keys(
    c: &Compiler<'_, '_>,
    name: &str,
    unit: NodeId,
) -> Result<HashSet<crate::check::Key>, StetError> {
    let view = c.tx.base().parts();
    let Some(part) = view.id(name) else {
        return Ok(HashSet::new());
    };
    if !view.is_attached(&BlockKey::new(name.into(), unit))? {
        return Ok(HashSet::new());
    }
    let tree = view.tree(part)?;
    Ok(crate::check::scan_unit(tree, unit)
        .into_iter()
        .map(|f| f.key)
        .collect())
}

/// The root of the unit that holds `node`: its nearest paragraph or table, or the root of the part.
fn unit_of(tree: &SpanTree, node: NodeId) -> NodeId {
    let mut cur = tree.node_ref(node);
    loop {
        if is_unit_boundary(tree, cur.id()) {
            return cur.id();
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => return cur.id(),
        }
    }
}

/// The created revision that is `node` or above it, up to the root of its unit.
fn created_at_or_above<'a>(
    tree: &SpanTree,
    node: NodeId,
    unit: NodeId,
    mine: &HashMap<NodeId, &'a Created>,
) -> Option<&'a Created> {
    let mut cur = Some(tree.node_ref(node));
    while let Some(n) = cur {
        if let Some(item) = mine.get(&n.id()) {
            return Some(item);
        }
        if n.id() == unit {
            return None;
        }
        cur = n.parent();
    }
    None
}
