//! The dirty unit: what `check` compares before and after an edit.
//!
//! A unit is an innermost paragraph (`w:p`, in a table cell or not) or a table's own element
//! (`w:tbl`: its properties, grid, rows and cells, and the order of what each cell holds). The
//! insides of the paragraphs and tables below a unit are units of their own. Any other element
//! an edit touches (the `w:body`, a block-level `w:sdt`) is a unit without the paragraphs and
//! tables below it.
//!
//! The violation keys name a declaration and an element, not a place. Compared as one set, an
//! old problem in one paragraph hides a new problem of the same kind in another. One unit per
//! paragraph keeps them apart.

use super::{Report, W_NS, walk};
use crate::schema::Ctx;
use crate::schema::scope::MAX_ANCESTORS;
use crate::schema::tables;
use crate::schema::tree::{Nodes, SchemaTree};

/// Whether the element is a paragraph (`w:p`) or a table (`w:tbl`), in the Transitional or the
/// Strict namespace. Each one is a unit of its own.
pub(in crate::schema) fn is_boundary<T: Nodes>(tree: &T, node: T::Node) -> bool {
    let (ns, local) = tree.name(node);
    (local == "p" || local == "tbl") && {
        let id = tables::ns_by_uri(ns);
        id.is_some() && id == tables::ns_by_uri(W_NS)
    }
}

/// The root of the unit that contains `node`: the nearest paragraph or table at or above it, or
/// `node` itself when there is none (the `w:body`, a block-level `w:sdt`). It is the unit of a
/// story part. In any other part (`styles`, `numbering`, `settings`) a unit is a top-level
/// element, a child of the root, which the caller names.
pub fn unit_root<T: Nodes>(tree: &T, node: T::Node) -> T::Node {
    let mut cur = node;
    for _ in 0..MAX_ANCESTORS {
        if is_boundary(tree, cur) {
            return cur;
        }
        let Some(parent) = tree.parent(cur) else {
            break;
        };
        cur = parent;
    }
    node
}

/// Validates one unit: `node` and what is below it, except the insides of the paragraphs and
/// tables below it. `ctx` is the context of `node`, which `unit_root` returned. The keys are the
/// ones [`validate_part`](super::validate_part) gives, and the keys of all the units of a part
/// together are the keys of the part.
///
/// A paragraph checks itself (its properties, runs and attributes). A table checks its own
/// properties, its grid, its rows and cells, and the order of the children of each cell, a
/// paragraph among them, but not what is inside the paragraphs. One cell paragraph with a run
/// before its `w:pPr` therefore cannot hide a new order problem in the other paragraphs of the
/// table.
pub fn validate_unit<T: SchemaTree>(ctx: Ctx, tree: &T, node: T::Node) -> Report<T::Node> {
    walk(ctx, tree, node, true)
}
