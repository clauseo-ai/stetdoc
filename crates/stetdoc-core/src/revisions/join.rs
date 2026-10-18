//! The join of two paragraphs (design `revisions.md`, 6.2): one paragraph's mark is gone, so its content
//! and the next paragraph's become one. The compiler does it for a merge that is real (the author's own
//! inserted mark, or untracked), and the resolver does it for every mark that is accepted or rejected.
//!
//! The first element survives with its attributes, its ref and its place. The later paragraph's
//! properties, mark and section break win, and its content follows the first's. A first paragraph with
//! no content left is removed instead, and the second is untouched.

use super::names::{is_blank, is_w};
use super::tree_ops::{detach, move_to};
use crate::api::error::StetError;
use crate::state::{Retired, Tx};
use crate::wordml::BlockKey;
use crate::wordml::NodeId;
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

/// What the join did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Joined {
    /// `first` had no content, and its element is gone. `second` is as it was.
    FirstRemoved,
    /// `second`'s content and properties are in `first`, and `second`'s element is gone.
    Merged,
}

/// Why the second paragraph's ref goes: it was merged into the first, or it was emptied by the rejection
/// of its inserted content (the second half of a Y paragraph), which the receipt says as "rejected".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Why {
    /// A mark was accepted or rejected, or a paragraph was removed: a first paragraph with no content
    /// left is removed and the second stays as it is.
    Merge,
    /// A merge that a caller asked for: the first element survives with its ref even if it holds
    /// nothing, because the caller named it as the block that remains.
    Keep,
    SecondRejected,
}

/// The range markers that survive the removal of what they were in.
pub(super) const MARKERS: [&str; 10] = [
    "bookmarkStart",
    "bookmarkEnd",
    "commentRangeStart",
    "commentRangeEnd",
    "permStart",
    "permEnd",
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
];

/// True for something a paragraph's removal must keep: a range marker, or a run that holds only a comment
/// reference.
pub(super) fn is_leftover(n: &NodeRef<'_>) -> bool {
    if MARKERS.iter().any(|m| is_w(n, m)) {
        return true;
    }
    is_w(n, "r") && {
        let mut content = n.child_elements().filter(|c| !is_w(c, "rPr"));
        let mut any = false;
        content.all(|c| {
            any = true;
            is_w(&c, "commentReference")
        }) && any
    }
}

/// True for something that is no content and goes with its paragraph: a `w:proofErr`, and a run that holds
/// nothing but its properties and a `w:lastRenderedPageBreak` (Word leaves both behind when text goes).
fn is_hollow(n: &NodeRef<'_>) -> bool {
    is_w(n, "proofErr")
        || is_w(n, "r")
            && n.children().all(|c| {
                is_blank(&c)
                    || c.is_element() && (is_w(&c, "rPr") || is_w(&c, "lastRenderedPageBreak"))
            })
}

/// Puts the leftovers of a paragraph that goes, `first`, where they keep their place without changing the
/// paragraph after it, `second`: range markers stand between blocks, before `first`, so `second` is
/// untouched (design 6.2). A run cannot stand there, so when a comment reference run is among them, they
/// all go to the start of `second`, with `gap`, the markers between the two, after them, as they were.
/// Returns true if `second` changed. The caller removes `first`.
pub(super) fn place_leftovers(
    tree: &mut SpanTree,
    first: NodeId,
    left: Vec<NodeId>,
    second: NodeId,
    gap: Vec<NodeId>,
) -> Result<bool, StetError> {
    let has_run = left.iter().any(|n| is_w(&tree.node_ref(*n), "r"));
    if !has_run {
        let parent = super::tree_ops::parent_of(tree, first)
            .ok_or_else(|| super::errors::internal("a paragraph has no parent"))?;
        for n in left {
            move_to(tree, n, parent, Some(first))?;
        }
        return Ok(false);
    }
    let at = tree
        .node_ref(second)
        .child_elements()
        .find(|c| !is_w(c, "pPr"))
        .map(|c| c.id());
    for n in left.into_iter().chain(gap) {
        move_to(tree, n, second, at)?;
    }
    Ok(true)
}

/// The leftovers in document order, if a paragraph holds nothing else besides its `w:pPr`. Wrappers
/// that hold nothing but leftovers are looked through.
pub(super) fn leftovers(tree: &SpanTree, para: NodeId) -> Option<Vec<NodeId>> {
    fn walk(n: &NodeRef<'_>, out: &mut Vec<NodeId>) -> bool {
        for c in n.children() {
            if is_blank(&c) {
                continue;
            }
            if !c.is_element() {
                return false;
            }
            if is_leftover(&c) {
                out.push(c.id());
            } else if is_w(&c, "pPr") && n.local_name() == "p" || is_hollow(&c) {
                continue;
            } else if is_w(&c, "ins")
                || is_w(&c, "del")
                || is_w(&c, "moveTo")
                || is_w(&c, "moveFrom")
                || is_w(&c, "hyperlink")
                || is_w(&c, "smartTag")
            {
                if !walk(&c, out) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }
    let mut out = Vec::new();
    walk(&tree.node_ref(para), &mut out).then_some(out)
}

/// Moves what is left in `first` besides its `w:pPr` (empty runs, proofing marks) to the start of `second`, in order, and copies the
/// attributes of `first` that `second` does not have.
fn keep_first_in_second(
    tree: &mut SpanTree,
    first: NodeId,
    second: NodeId,
) -> Result<(), StetError> {
    let kids: Vec<NodeId> = tree
        .node_ref(first)
        .children()
        .filter(|c| c.is_element() && !is_w(c, "pPr"))
        .map(|c| c.id())
        .collect();
    let at = tree
        .node_ref(second)
        .child_elements()
        .find(|c| !is_w(c, "pPr"))
        .map(|c| c.id());
    for k in kids {
        move_to(tree, k, second, at)?;
    }
    let attrs: Vec<(String, String, String)> = {
        let s = tree.node_ref(second);
        tree.node_ref(first)
            .attrs()
            .filter(|a| !a.is_namespace_declaration())
            .filter(|a| s.attr(a.namespace(), a.local_name()).is_none())
            .map(|a| {
                (
                    a.namespace().to_owned(),
                    a.local_name().to_owned(),
                    a.value().into_owned(),
                )
            })
            .collect()
    };
    for (ns, local, value) in attrs {
        tree.edit().set_attr(second, &ns, &local, &value)?;
    }
    Ok(())
}

/// The range markers that stand between two sibling blocks (`from` before `to`), in order. A marker can sit
/// between paragraphs, as Word writes `w:bookmarkEnd` after the paragraph that a bookmark was in. When the
/// leftovers of a paragraph that goes are moved into a neighbour, these have to move with them and stay on
/// the same side of them, or a range whose start was in the paragraph and whose end was in the gap would end
/// up with its end before its start. Empty if anything else is between the two.
pub(super) fn markers_between(tree: &SpanTree, from: NodeId, to: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut cur = tree.node_ref(from).next_sibling();
    while let Some(c) = cur {
        if c.id() == to {
            return out;
        }
        if !c.is_element() || is_blank(&c) {
            cur = c.next_sibling();
            continue;
        }
        if MARKERS.iter().any(|m| is_w(&c, m)) || is_w(&c, "proofErr") {
            out.push(c.id());
            cur = c.next_sibling();
            continue;
        }
        return Vec::new();
    }
    Vec::new()
}

/// True if the paragraph holds anything besides its `w:pPr` and white space.
pub(super) fn has_content(tree: &crate::xml::SpanTree, para: NodeId) -> bool {
    tree.node_ref(para)
        .children()
        .any(|c| !(is_blank(&c) || c.is_element() && is_w(&c, "pPr")))
}

/// Joins `second` into `first`, which are neighbours in one container. Declares the identity of the
/// result to the transaction, for the blocks that have a ref. `first_removed_reason` is what the
/// declaration says when `first` is removed and had a ref: nothing, since an element that leaves the
/// tree is `Deleted` by default.
pub(super) fn join(
    tx: &mut Tx<'_>,
    part: &Arc<str>,
    first: NodeId,
    second: NodeId,
    why: Why,
) -> Result<Joined, StetError> {
    let first_key = BlockKey::new(part.clone(), first);
    let second_key = BlockKey::new(part.clone(), second);
    let (first_ref, second_ref) = {
        let view = tx.parts();
        (view.ref_of(&first_key), view.ref_of(&second_key))
    };
    let tree = tx.part_mut(part)?;
    // A first paragraph with nothing left but range markers and comment references is removed, and
    // those stay where it was (or go to the start of the second if a reference run is among them).
    if why != Why::Keep
        && let Some(left) = leftovers(tree, first)
    {
        // What stands between the two comes after what was in the first, as it did.
        let gap = markers_between(tree, first, second);
        place_leftovers(tree, first, left, second, gap)?;
        if why == Why::SecondRejected {
            // The second is the new half of a Y paragraph, emptied by the rejection, and the first held the original's content
            // and element: the empty runs left in the first go to the start of the second, and the attributes of the first that
            // the second lacks go with them, so that the paragraph reads and is written as it was before the split.
            keep_first_in_second(tree, first, second)?;
        }
        detach(tree, first)?;
        return Ok(Joined::FirstRemoved);
    }
    // The properties: the later paragraph's, in the first element.
    let old_ppr = tree
        .node_ref(first)
        .child_elements()
        .find(|c| is_w(c, "pPr"))
        .map(|c| c.id());
    let new_ppr = tree
        .node_ref(second)
        .child_elements()
        .find(|c| is_w(c, "pPr"))
        .map(|c| c.id());
    if let Some(old) = old_ppr {
        detach(tree, old)?;
    }
    if let Some(new) = new_ppr {
        detach(tree, new)?;
        // An empty one says nothing, and is a violation wherever it stands; so is an empty mark w:rPr in it
        // (Word and LibreOffice write both). They would be new violations under the survivor's ref.
        let empty_rpr: Vec<NodeId> = tree
            .node_ref(new)
            .child_elements()
            .filter(|c| is_w(c, "rPr") && c.child_elements().next().is_none())
            .map(|c| c.id())
            .collect();
        for r in empty_rpr {
            detach(tree, r)?;
        }
        if tree.node_ref(new).child_elements().next().is_some() {
            let before = tree.node_ref(first).first_child().map(|c| c.id());
            super::tree_ops::insert(tree, first, before, new)?;
        }
    }
    // The content: after the first's, in order.
    let moving: Vec<NodeId> = tree
        .node_ref(second)
        .children()
        .filter(|c| !is_blank(c))
        .map(|c| c.id())
        .collect();
    // The last thing in the first paragraph and the first thing in the second, which may be the two
    // halves of a hyperlink or a smart tag that a split cut.
    let last_before = tree
        .node_ref(first)
        .children()
        .filter(|c| !is_blank(c))
        .last()
        .map(|c| c.id());
    let first_after = moving.first().copied();
    for c in moving {
        move_to(tree, c, first, None)?;
    }
    detach(tree, second)?;
    if let (Some(a), Some(b)) = (last_before, first_after) {
        merge_shells(tree, a, b)?;
    }
    match (first_ref, second_ref, why) {
        (Some(_), Some(_), Why::Merge | Why::Keep) => {
            tx.merge(&first_key, &second_key, first_key.clone())?
        }
        (Some(_), Some(_), Why::SecondRejected) => tx.retire(&second_key, Retired::Rejected)?,
        (None, Some(_), _) => tx.rebind(&second_key, first_key.clone())?,
        _ => {}
    }
    Ok(Joined::Merged)
}

/// The `w:id`s of the revision elements below a paragraph's `w:pPr`, which a join drops with it.
pub(super) fn ppr_revision_ids(
    tree: &crate::xml::SpanTree,
    para: NodeId,
) -> Vec<(crate::wordml::RevKind, i64)> {
    let mut out = Vec::new();
    let Some(ppr) = tree
        .node_ref(para)
        .child_elements()
        .find(|c| is_w(c, "pPr"))
    else {
        return out;
    };
    let mut stack = vec![ppr];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if let Some(r) = crate::wordml::revision_at(&c)
                && let Some(id) = r.id
            {
                out.push((r.kind, id));
            }
            stack.push(c);
        }
    }
    out
}

/// True for the containers whose two halves, with the same attributes, are one container again.
fn is_shell(n: &NodeRef<'_>) -> bool {
    n.is_element()
        && super::names::is_word_ns(n.namespace())
        && matches!(
            n.local_name(),
            "hyperlink" | "smartTag" | "customXml" | "dir" | "bdo"
        )
}

/// Joins two neighbouring containers that are the same element with the same attributes into one, and
/// does the same where their contents meet.
fn merge_shells(tree: &mut SpanTree, a: NodeId, b: NodeId) -> Result<(), StetError> {
    let same = {
        let (x, y) = (tree.node_ref(a), tree.node_ref(b));
        let attrs = |n: &NodeRef<'_>| -> Vec<(String, String, String)> {
            let mut v: Vec<_> = n
                .attrs()
                .filter(|a| !a.is_namespace_declaration())
                .map(|a| {
                    (
                        a.namespace().to_owned(),
                        a.local_name().to_owned(),
                        a.value().into_owned(),
                    )
                })
                .collect();
            v.sort();
            v
        };
        is_shell(&x)
            && x.local_name() == y.local_name()
            && x.namespace() == y.namespace()
            && attrs(&x) == attrs(&y)
    };
    if !same {
        return Ok(());
    }
    let a_last = tree
        .node_ref(a)
        .children()
        .filter(|c| !is_blank(c))
        .last()
        .map(|c| c.id());
    let moving: Vec<NodeId> = tree
        .node_ref(b)
        .children()
        .filter(|c| !is_blank(c))
        .map(|c| c.id())
        .collect();
    let b_first = moving.first().copied();
    for c in moving {
        move_to(tree, c, a, None)?;
    }
    detach(tree, b)?;
    if let (Some(x), Some(y)) = (a_last, b_first) {
        merge_shells(tree, x, y)?;
    }
    Ok(())
}
