//! Copies of properties in the order Word writes: the last step of every transaction before it is
//! checked.
//!
//! A run that the compiler creates (new text, the right half of a cut, a run of a new row or
//! paragraph, a run that compare imports) gets its properties by copying a `w:rPr` the document
//! already has, and a copy is clean, so it writes the bytes it was copied from. When those bytes
//! are out of the order Word writes, every copy repeats the defect, and the Open XML SDK counts each
//! one as a schema error. So every property container that the transaction made and that is in the
//! document at the end is put in schema order (`schema::reorder`), and the containers it did not
//! make keep their bytes. The left half of a cut run is the run the document had, so it keeps them
//! too: a split adds no copy of the defect, and the document has as many as before.
//!
//! A child of a `w:rPr` or `w:trPr` that repeats an earlier one exactly (`<w:i/><w:i/>`, a tool's slip) is
//! the same defect to the validator, so a created container keeps the first of them and drops the rest. Repeats that
//! differ are kept, since which one Word reads is not this module's to decide.

use super::props_edit::{canon, ctx_of, restore_order};
use super::tree_ops::detach;
use crate::api::error::StetError;
use crate::schema::is_word_namespace;
use crate::state::Tx;
use crate::xml::{NodeId, SpanTree};
use std::collections::HashSet;

/// Puts every property container that `tx` created in its changed parts, and that is attached, in
/// schema order. A container the transaction did not create is never touched, and one already in
/// order is left as it is.
pub(crate) fn order_created(tx: &mut Tx<'_>) -> Result<(), StetError> {
    for name in tx.edited_clones() {
        let Some(floor) = tx.base().parts().tree_opt(&name)?.map(SpanTree::node_count) else {
            continue;
        };
        let tree = tx.part_mut(&name)?;
        let created: Vec<NodeId> = tree
            .created_since(floor)
            .filter(|&id| is_container(tree, id) && attached(tree, id))
            .collect();
        for id in created {
            if ctx_of(tree, id).is_some_and(|c| c.is_reorderable()) {
                if is_unordered(tree, id) {
                    drop_exact_repeats(tree, id)?;
                }
                restore_order(tree, id)?;
            }
        }
    }
    Ok(())
}

/// The property containers the schema leaves unordered, where a repeat is `schema_non_canonical_order`, the counted kind:
/// `w:rPr` and `w:trPr`. A repeat elsewhere is not counted, and stays as the source had it.
fn is_unordered(tree: &SpanTree, id: NodeId) -> bool {
    let n = tree.node_ref(id);
    is_word_namespace(n.namespace()) && matches!(n.local_name(), "rPr" | "trPr")
}

/// Detaches every element child that is the same as an earlier one: name, attributes and children.
fn drop_exact_repeats(tree: &mut SpanTree, container: NodeId) -> Result<(), StetError> {
    let mut seen: HashSet<String> = HashSet::new();
    let repeats: Vec<NodeId> = tree
        .node_ref(container)
        .child_elements()
        .filter(|c| !seen.insert(canon(c)))
        .map(|c| c.id())
        .collect();
    for id in repeats {
        detach(tree, id)?;
    }
    Ok(())
}

/// A cheap filter before the schema is asked: an element with at least two element children.
fn is_container(tree: &SpanTree, id: NodeId) -> bool {
    let n = tree.node_ref(id);
    n.is_element() && n.child_elements().nth(1).is_some()
}

/// Whether the node is in the document: its ancestors reach the document node.
fn attached(tree: &SpanTree, id: NodeId) -> bool {
    let top = tree.document().id();
    let mut cur = tree.node_ref(id).parent();
    while let Some(p) = cur {
        if p.id() == top {
            return true;
        }
        cur = p.parent();
    }
    false
}
