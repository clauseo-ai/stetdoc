//! Reading a unit the way Word reads it: the children of an element, a walk over the elements of
//! a unit, and whether an element is in a branch Word takes.
//!
//! `check` reads a unit twice, once for the schema (`validate_unit`) and once for the Word rules,
//! and the two must see the same document. So the Word rules do not walk the tree themselves:
//! they walk it with [`walk_unit`], which goes through [`effective_children`] as the validator
//! does. An `mc:AlternateContent` stands for the branch Word reads, an element that
//! `mc:ProcessContent` keeps the children of stands for them, and an element in an ignorable
//! namespace that the tables do not know is not there. Two differences from the validator are on
//! purpose. The validator checks the elements of the branches that were not taken below
//! themselves, and a rule about what Word reads must not see them. The validator does not enter
//! an element the tables hold no content model for (the drawing around a text box: DrawingML
//! and the shapes are known by name and not by content), because it has nothing to check there,
//! and a Word rule must reach the paragraphs of a text box, which Word reads.

use super::effective::{effective_children, is_mc, taken_branch_in};
use super::scope::{MAX_ANCESTORS, scope_at, scope_link};
use super::tree::Nodes;
use super::validate::{self, MC_NS, W_NS, W_STRICT_NS};

/// The element children of `parent` that Word reads, in document order. An `mc:AlternateContent`
/// is replaced by the elements of its branch ([`taken_branch`](super::taken_branch)), an element
/// that `mc:ProcessContent` keeps the children of by those children, and an element that is
/// skipped (ignorable, and not a name the tables know) is left out. It is the list the validator
/// runs through the content model, and the one `insert_in_order` counts positions in.
pub fn read_children<T: Nodes>(tree: &T, parent: T::Node) -> Vec<T::Node> {
    let mut scope = scope_at(tree, parent, true);
    let read = effective_children(tree, parent, &mut scope.chain, scope.ign);
    read.items
        .into_iter()
        .filter(|item| item.read)
        .map(|item| item.node)
        .collect()
}

/// Calls `visit(node, depth)` for each element Word reads in the unit that starts at `root`, in
/// document order, with `root` at depth 0. A wrapper that stands for its branch is not visited
/// and does not count as a level, so the depth of an element is the number of elements Word
/// reads between it and `root`.
///
/// With `stop_at_boundaries`, a paragraph or a table below `root` is visited and not entered: it
/// is a unit of its own ([`validate_unit`](super::validate_unit)). Without it the walk covers
/// everything below `root`, which is what a whole part needs.
///
/// Every element that is not skipped is entered, whatever the tables know of it, so the walk
/// reaches the paragraphs inside a text box. An element in an ignorable namespace that the tables
/// do not know is not read, and neither is anything in it.
pub fn walk_unit<T: Nodes>(
    tree: &T,
    root: T::Node,
    stop_at_boundaries: bool,
    visit: &mut dyn FnMut(T::Node, usize),
) {
    let mut scope = scope_at(tree, root, false);
    // The node, its depth, and what `mc:Ignorable` and `mc:ProcessContent` say at its parent.
    let mut stack = vec![(root, 0usize, scope.ign)];
    while let Some((node, depth, ign)) = stack.pop() {
        visit(node, depth);
        if stop_at_boundaries && depth > 0 && validate::is_boundary(tree, node) {
            continue;
        }
        // The element's own declarations are in scope for its children. Almost no element has any,
        // so the attributes are collected only for one that has an attribute of markup
        // compatibility.
        let mut inner = ign;
        if tree.has_attribute_in(node, MC_NS)
            && let Some(link) = scope_link(tree, node, &tree.attributes(node), ign)
        {
            scope.chain.push(link);
            inner = Some(scope.chain.len() - 1);
        }
        let read = effective_children(tree, node, &mut scope.chain, inner);
        // The stack pops the last pushed first, so the first child goes on last.
        for item in read.items.into_iter().rev().filter(|item| item.read) {
            stack.push((item.node, depth + 1, item.ign));
        }
    }
}

/// Whether `node` is the root of a unit of its own: a paragraph or a table (`w:p`, `w:tbl`), in
/// the Transitional or the Strict namespace.
pub fn is_unit_boundary<T: Nodes>(tree: &T, node: T::Node) -> bool {
    validate::is_boundary(tree, node)
}

/// Whether Word reads `node`: false when an element above it (or it) is an `mc:Choice` that is
/// not the branch Word takes, or an `mc:Fallback` of an `mc:AlternateContent` that has one. It
/// is the question `ctx_at` does not ask: `ctx_at` gives a node in a branch that was not taken
/// the context it would have if Word read it, because a context is a fact about structure. Code
/// that must not edit what Word does not read asks this first.
pub fn in_taken_branch<T: Nodes>(tree: &T, node: T::Node) -> bool {
    let mut at = node;
    for _ in 0..MAX_ANCESTORS {
        let Some(parent) = tree.parent(at) else {
            return true;
        };
        let name = tree.name(at);
        if (is_mc(name, "Choice") || is_mc(name, "Fallback"))
            && taken_branch_in(tree, parent) != Some(at)
        {
            return false;
        }
        at = parent;
    }
    true
}

/// Whether a namespace URI is the WordprocessingML main namespace, in the Transitional or the
/// Strict spelling.
pub fn is_word_namespace(uri: &str) -> bool {
    uri == W_NS || uri == W_STRICT_NS
}
