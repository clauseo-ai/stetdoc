//! Finding the units a batch changed.
//!
//! The units are the ones `schema::validate_unit` checks (`docs/design/check.md` 2.1). A batch
//! changed a unit when the unit is not what the base has: the span tree marks every element an
//! edit touched and every element above it, so a walk that enters only marked elements finds the
//! edits, and an equality test against the base drops what only earlier commits marked, because a
//! mark is never cleared. A table or a frame is looked at only when an element of its own shell
//! changed: editing the text of a cell marks the table and the body and changes neither.
//!
//! Everything here compares a tree with the tree of the base by `NodeId`. That is sound because
//! `Tx::part_mut` clones the base's tree, a clone keeps the ids of the original, and the base is
//! never written. A part that is not a clone is validated whole by the caller.

use super::rules::is_word;
use crate::schema::{PartKind, is_unit_boundary};
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::collections::HashSet;

/// How a part is divided into units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Layout {
    /// A story: paragraphs, tables, and the frame that is left. `Document`, `Header`, `Footer`
    /// and `Glossary`.
    Story,
    /// Each note or comment is a unit, and so are the paragraphs and tables below it. `Footnotes`,
    /// `Endnotes` and `Comments`.
    Notes,
    /// Each top-level element is a unit. Every other part.
    Elements,
}

/// The layout of a part of this kind.
pub(super) fn layout_of(kind: PartKind) -> Layout {
    match kind {
        PartKind::Document | PartKind::Header | PartKind::Footer | PartKind::Glossary => {
            Layout::Story
        }
        PartKind::Footnotes | PartKind::Endnotes | PartKind::Comments => Layout::Notes,
        _ => Layout::Elements,
    }
}

/// What a unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UnitKind {
    /// A `w:p`.
    Paragraph,
    /// A `w:tbl`.
    Table,
    /// A `w:footnote`, `w:endnote` or `w:comment`, a child of the root of its part.
    Note,
    /// A child of the root of a part that is not a story.
    TopLevel,
    /// What is left of a part: the root, `w:body`, block-level content controls and markers. In
    /// the notes parts and the parts that are not stories, the root alone.
    Frame,
}

/// A unit: its root, and what kind it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Unit {
    pub(super) root: NodeId,
    pub(super) kind: UnitKind,
}

/// What the walk found in one part.
#[derive(Debug, Default)]
pub(super) struct Changed {
    /// The units to validate, in document order of the first element that marked each.
    pub(super) units: Vec<Unit>,
    /// Paragraphs, notes, top-level elements and tables the walk met.
    pub(super) dirty: usize,
    /// Of those, the ones dropped because they are the base's, element for element.
    pub(super) equal: usize,
    /// Tables and frames that were dirty and whose own shell did not change.
    pub(super) shells_skipped: usize,
}

/// Whether the element is a paragraph or a table.
fn is_block(el: &NodeRef<'_>) -> bool {
    is_word(el, "p") || is_word(el, "tbl")
}

/// The attributes of an element as a sorted list, so that the order they were written in does not
/// matter.
fn attributes<'a>(el: &NodeRef<'a>) -> Vec<(&'a str, &'a str, &'a [u8])> {
    let mut out: Vec<_> = el
        .attrs()
        .map(|a| (a.namespace(), a.local_name(), a.raw_value()))
        .collect();
    out.sort_unstable();
    out
}

/// Whether the element of the staged tree is not the base's element of the same id: it is new, or
/// its name, its attributes or the ids of its children (in order) are different. It looks at the
/// element and at nothing below it.
fn changed_itself(base: &SpanTree, el: &NodeRef<'_>) -> bool {
    let b = base.node_ref(el.id());
    !b.is_element()
        || b.namespace() != el.namespace()
        || b.local_name() != el.local_name()
        || attributes(&b) != attributes(el)
        || !b
            .children()
            .map(|c| c.id())
            .eq(el.children().map(|c| c.id()))
}

/// Whether the unit that starts at `root` is, element for element, the base's: the same names,
/// attributes, text and children, in the same place (its parent has the same id). The paragraphs
/// and tables below `root` are units of their own and are compared by id only, as a child of the
/// element that holds them. Only the elements the edits marked are looked into, because an
/// element that was never marked is the one that was parsed, in the base as well.
fn same_unit(base: &SpanTree, tree: &SpanTree, root: NodeId) -> bool {
    let (t, b) = (tree.node_ref(root), base.node_ref(root));
    if !b.is_element() || t.parent().map(|p| p.id()) != b.parent().map(|p| p.id()) {
        return false;
    }
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let el = tree.node_ref(id);
        if changed_itself(base, &el) {
            return false;
        }
        for child in el.children() {
            if child.is_element() {
                if child.is_dirty() && !is_block(&child) {
                    stack.push(child.id());
                }
            } else {
                let old = base.node_ref(child.id());
                if old.kind() != child.kind() || old.text() != child.text() {
                    return false;
                }
            }
        }
    }
    true
}

/// What a dirty element belongs to.
#[derive(Clone, Copy)]
enum Owner {
    /// The frame of the part.
    Frame,
    /// A table: its shell.
    Table(NodeId),
    /// A paragraph or a note, whose own comparison covers it.
    Covered,
}

/// The units of a part that this transaction changed, in a tree that is a clone of the base's.
///
/// The walk enters the elements that are marked, and two kinds of element that are not always:
/// an element that is not in the base, whose insides were copied clean by `clone_subtree` and are
/// new all the same, and an element that was moved into a parent that is marked, which an edit
/// does not mark itself. Both are found as the children of a marked element whose list of
/// children is not the base's.
pub(super) fn changed_units(base: &SpanTree, tree: &SpanTree, kind: PartKind) -> Changed {
    let layout = layout_of(kind);
    let mut out = Changed::default();
    let root = tree.root();
    if !root.is_dirty() {
        return out;
    }
    // The candidates in the order the walk meets them. A shell is marked by the first of its
    // elements that changed itself.
    let mut order: Vec<Unit> = Vec::new();
    let mut marked: HashSet<NodeId> = HashSet::new();
    let mut tables: Vec<NodeId> = Vec::new();
    let mut mark = |unit: Unit, order: &mut Vec<Unit>| {
        if marked.insert(unit.root) {
            order.push(unit);
        }
    };
    let frame = Unit {
        root: root.id(),
        kind: UnitKind::Frame,
    };
    // The element, what it belongs to, and whether it is inside an element that is new.
    let mut stack: Vec<(NodeId, Owner, bool)> = vec![(root.id(), Owner::Frame, false)];
    while let Some((id, owner, inside_new)) = stack.pop() {
        let el = tree.node_ref(id);
        let old = base.node_ref(id);
        let is_new = inside_new || !old.is_element();
        let top = layout != Layout::Story && el.parent().is_some_and(|p| p.id() == root.id());
        let mut inner = owner;
        if top {
            out.dirty += 1;
            let unit = Unit {
                root: id,
                kind: if layout == Layout::Notes {
                    UnitKind::Note
                } else {
                    UnitKind::TopLevel
                },
            };
            if same_unit(base, tree, id) {
                out.equal += 1;
            } else {
                mark(unit, &mut order);
            }
            inner = Owner::Covered;
        } else if layout != Layout::Elements && is_word(&el, "p") {
            out.dirty += 1;
            if same_unit(base, tree, id) {
                out.equal += 1;
            } else {
                mark(
                    Unit {
                        root: id,
                        kind: UnitKind::Paragraph,
                    },
                    &mut order,
                );
            }
            inner = Owner::Covered;
        } else if layout != Layout::Elements && is_word(&el, "tbl") {
            out.dirty += 1;
            tables.push(id);
            let moved =
                old.is_element() && old.parent().map(|p| p.id()) != el.parent().map(|p| p.id());
            if is_new || moved || changed_itself(base, &el) {
                mark(
                    Unit {
                        root: id,
                        kind: UnitKind::Table,
                    },
                    &mut order,
                );
            }
            inner = Owner::Table(id);
        } else if is_new || changed_itself(base, &el) {
            match owner {
                Owner::Frame => mark(frame, &mut order),
                Owner::Table(t) => mark(
                    Unit {
                        root: t,
                        kind: UnitKind::Table,
                    },
                    &mut order,
                ),
                Owner::Covered => {}
            }
        }
        if layout == Layout::Elements && top {
            continue;
        }
        let children: Vec<NodeId> = if is_new {
            el.child_elements().map(|c| c.id()).collect()
        } else {
            // The marked children, and, when the list of children is not the base's, the ones that
            // are here and were not here in the base: new elements, and blocks that were moved in.
            let same_list = old.is_element()
                && old
                    .children()
                    .map(|c| c.id())
                    .eq(el.children().map(|c| c.id()));
            let was: HashSet<NodeId> = if same_list {
                HashSet::new()
            } else {
                old.children().map(|c| c.id()).collect()
            };
            el.child_elements()
                .filter(|c| c.is_dirty() || (!same_list && !was.contains(&c.id())))
                .map(|c| c.id())
                .collect()
        };
        for child in children.into_iter().rev() {
            stack.push((child, inner, is_new));
        }
    }
    out.shells_skipped = tables.iter().filter(|t| !marked.contains(t)).count();
    out.units = order;
    out
}

/// Every unit of a part, found without the dirty flags: what the shortcuts must agree with.
#[cfg(test)]
pub(super) fn all_units(tree: &SpanTree, kind: PartKind) -> Vec<Unit> {
    let layout = layout_of(kind);
    let root = tree.root().id();
    let mut out = vec![Unit {
        root,
        kind: UnitKind::Frame,
    }];
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let el = tree.node_ref(id);
        let top = layout != Layout::Story && el.parent().is_some_and(|p| p.id() == root);
        let unit_kind = if top {
            Some(if layout == Layout::Notes {
                UnitKind::Note
            } else {
                UnitKind::TopLevel
            })
        } else if layout != Layout::Elements && is_word(&el, "p") {
            Some(UnitKind::Paragraph)
        } else if layout != Layout::Elements && is_word(&el, "tbl") {
            Some(UnitKind::Table)
        } else {
            None
        };
        if let Some(kind) = unit_kind {
            out.push(Unit { root: id, kind });
        }
        let children: Vec<NodeId> = el.child_elements().map(|c| c.id()).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

/// The kind of unit whose root is `node`, in a part with this layout.
pub(super) fn kind_of(tree: &SpanTree, node: NodeId, layout: Layout) -> UnitKind {
    let el = tree.node_ref(node);
    if layout != Layout::Elements && is_unit_boundary(tree, node) {
        return if is_word(&el, "tbl") {
            UnitKind::Table
        } else {
            UnitKind::Paragraph
        };
    }
    let top = el.parent().is_some_and(|p| p.id() == tree.root().id());
    match (layout, top) {
        (Layout::Notes, true) => UnitKind::Note,
        (Layout::Elements, true) => UnitKind::TopLevel,
        _ => UnitKind::Frame,
    }
}
