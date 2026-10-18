//! The rows of the extension parts: add one, find one by the `paraId` of the comment it is about, set its
//! done flag or its parent, and remove the rows of removed comments.
//!
//! A new row takes the namespace of its part's root, which is what the rows already in it use, so a part
//! written in the namespace of Word 2013's preview stays in it.

use super::xmlw::{self, el};
use crate::api::error::StetError;
use crate::revisions::handle::PartWriter;
use crate::threads::{is_w15, para_id, w15_attr};
use crate::xml::{NodeId, SpanTree};
use std::collections::{HashMap, HashSet};

/// The elements of a part that are rows named `local`, with the `paraId` each is about.
fn rows(tree: &SpanTree, local: &str) -> Vec<(NodeId, Option<u32>)> {
    tree.root()
        .child_elements()
        .filter(|n| n.local_name() == local)
        .map(|n| (n.id(), para_id(&n)))
        .collect()
}

/// `paraId` formatted as Word writes it.
pub(super) fn hex(id: u32) -> String {
    format!("{id:08X}")
}

/// Appends a `commentEx` row.
pub(super) fn add_extended(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    para: u32,
    parent: Option<u32>,
    done: bool,
) -> Result<(), StetError> {
    let tree = tx.part_mut(part)?;
    let ns = tree.root().namespace().to_owned();
    let (p, parent_hex, d) = (hex(para), parent.map(hex), if done { "1" } else { "0" });
    let mut attrs: Vec<xmlw::Attr<'_>> = vec![(&ns, "paraId", &p)];
    if let Some(h) = &parent_hex {
        attrs.push((&ns, "paraIdParent", h));
    }
    attrs.push((&ns, "done", d));
    let row = el(tree, &ns, "commentEx", &attrs, &[])?;
    let root = tree.root().id();
    xmlw::append(tree, root, row)
}

/// Appends a `commentId` row.
pub(super) fn add_id(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    para: u32,
    durable: u32,
) -> Result<(), StetError> {
    let tree = tx.part_mut(part)?;
    let ns = tree.root().namespace().to_owned();
    let (p, d) = (hex(para), hex(durable));
    let row = el(
        tree,
        &ns,
        "commentId",
        &[(&ns, "paraId", &p), (&ns, "durableId", &d)],
        &[],
    )?;
    let root = tree.root().id();
    xmlw::append(tree, root, row)
}

/// Appends a `commentExtensible` row. It has a durable id and no UTC date: the compiler reads no clock
/// and a date is the caller's wall-clock time.
pub(super) fn add_extensible(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    durable: u32,
) -> Result<(), StetError> {
    let tree = tx.part_mut(part)?;
    let ns = tree.root().namespace().to_owned();
    let d = hex(durable);
    let row = el(
        tree,
        &ns,
        "commentExtensible",
        &[(&ns, "durableId", &d)],
        &[],
    )?;
    let root = tree.root().id();
    xmlw::append(tree, root, row)
}

/// Appends a `person` for `author` unless the part has one.
pub(super) fn add_person(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    author: &str,
) -> Result<(), StetError> {
    let tree = tx.part_mut(part)?;
    let known = tree
        .root()
        .child_elements()
        .any(|n| is_w15(&n, "person") && w15_attr(&n, "author").is_some_and(|a| a == author));
    if known {
        return Ok(());
    }
    let ns = tree.root().namespace().to_owned();
    let info = el(
        tree,
        &ns,
        "presenceInfo",
        &[(&ns, "providerId", "None"), (&ns, "userId", author)],
        &[],
    )?;
    let person = el(tree, &ns, "person", &[(&ns, "author", author)], &[info])?;
    let root = tree.root().id();
    xmlw::append(tree, root, person)
}

/// Sets `w15:done` on the rows of the comments whose last paragraphs are given, adding the row of a comment
/// that has none, in one pass over the part. Returns true if anything changed.
pub(super) fn set_done_many(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    wanted: &[(u32, bool)],
) -> Result<bool, StetError> {
    // The first row of each paragraph, with its done flag as written.
    let found: HashMap<u32, (NodeId, Option<String>)> = {
        let view = tx.parts();
        let tree = view.id(part).map(|id| view.tree(id)).transpose()?;
        let mut map = HashMap::new();
        if let Some(t) = tree {
            for (node, para) in rows(t, "commentEx") {
                if let Some(p) = para {
                    map.entry(p).or_insert_with(|| {
                        (
                            node,
                            w15_attr(&t.node_ref(node), "done").map(|v| v.into_owned()),
                        )
                    });
                }
            }
        }
        map
    };
    let mut changed = false;
    let mut writes: Vec<(NodeId, &str)> = Vec::new();
    let mut adds: Vec<(u32, bool)> = Vec::new();
    for (para, done) in wanted {
        let value = if *done { "1" } else { "0" };
        match found.get(para) {
            Some((_, Some(current))) if current == value => {}
            Some((node, _)) => writes.push((*node, value)),
            None => adds.push((*para, *done)),
        }
    }
    if !writes.is_empty() {
        let tree = tx.part_mut(part)?;
        let ns = tree.root().namespace().to_owned();
        for (node, value) in writes {
            xmlw::set(tree, node, &ns, "done", value)?;
        }
        changed = true;
    }
    for (para, done) in adds {
        add_extended(tx, part, para, None, done)?;
        changed = true;
    }
    Ok(changed)
}

/// Sets `w15:done` on the row of one comment. See `set_done_many`.
pub(super) fn set_done(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    para: u32,
    done: bool,
) -> Result<bool, StetError> {
    set_done_many(tx, part, &[(para, done)])
}

/// Sets or clears `w15:paraIdParent` on the rows of the comments whose last paragraphs are given, in one
/// pass over the part. A comment with no row is left alone.
pub(super) fn set_parents(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    wanted: &[(u32, Option<u32>)],
) -> Result<(), StetError> {
    if wanted.is_empty() {
        return Ok(());
    }
    let nodes: HashMap<u32, NodeId> = {
        let view = tx.parts();
        let tree = view.id(part).map(|id| view.tree(id)).transpose()?;
        let mut map = HashMap::new();
        if let Some(t) = tree {
            for (node, para) in rows(t, "commentEx") {
                if let Some(p) = para {
                    map.entry(p).or_insert(node);
                }
            }
        }
        map
    };
    let targets: Vec<(NodeId, Option<u32>)> = wanted
        .iter()
        .filter_map(|(para, parent)| nodes.get(para).map(|n| (*n, *parent)))
        .collect();
    if targets.is_empty() {
        return Ok(());
    }
    let tree = tx.part_mut(part)?;
    let ns = tree.root().namespace().to_owned();
    for (node, parent) in targets {
        match parent {
            Some(p) => xmlw::set(tree, node, &ns, "paraIdParent", &hex(p))?,
            None => {
                tree.edit().remove_attr(node, &ns, "paraIdParent")?;
            }
        }
    }
    Ok(())
}

/// Removes the rows of the comments whose last paragraphs are `paras` and whose durable ids are
/// `durables`, from the part that holds rows named `local`.
pub(super) fn remove(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    local: &str,
    paras: &HashSet<u32>,
    durables: &HashSet<u32>,
) -> Result<(), StetError> {
    let doomed: Vec<NodeId> = {
        let view = tx.parts();
        let tree = view.id(part).map(|id| view.tree(id)).transpose()?;
        tree.map(|t| {
            t.root()
                .child_elements()
                .filter(|n| n.local_name() == local)
                .filter(|n| {
                    let by_para = para_id(n).is_some_and(|p| paras.contains(&p));
                    let by_durable = durable(n).is_some_and(|d| durables.contains(&d));
                    by_para || by_durable
                })
                .map(|n| n.id())
                .collect()
        })
        .unwrap_or_default()
    };
    if doomed.is_empty() {
        return Ok(());
    }
    let tree = tx.part_mut(part)?;
    for node in doomed {
        xmlw::detach(tree, node)?;
    }
    Ok(())
}

/// The durable id of a row, if it has one. `commentId` rows are matched by paragraph as well, and
/// `commentExtensible` rows only by this.
fn durable(n: &crate::xml::NodeRef<'_>) -> Option<u32> {
    [crate::threads::NS_CID, crate::threads::NS_CEX]
        .iter()
        .find_map(|ns| n.attr(ns, "durableId"))
        .and_then(|v| {
            (v.len() == 8 && v.bytes().all(|b| b.is_ascii_hexdigit()))
                .then(|| u32::from_str_radix(&v, 16).ok())
                .flatten()
        })
}
