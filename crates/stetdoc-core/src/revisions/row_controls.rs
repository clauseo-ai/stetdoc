//! Content controls around rows (design `revisions.md`, 3.4): the ids of a copied row's controls, the
//! repeating-section item that a new row gets, and the wrappers that go with a removed row.
//!
//! A row can stand in a row-level content control or custom XML element inside its table. A repeating
//! section is the `w:sdt` with `w15:repeatingSection`, and each of its items is a `w:sdt` with
//! `w15:repeatingSectionItem` around one copy of the section's content, in a table one row. A new row next
//! to an item's row gets an item of its own, a copy of the item's properties with a new id, before or after
//! that item, as Word's "Insert item before" and "Insert item after" do. Its merged cells follow the row it lands
//! next to, the item's first row or its last, not the anchor row. A removed row takes with it every
//! wrapper between it and its table that holds no other row: an item whose only row it was, and the section
//! too when that was its last item. A tracked new row before the first item goes above the section instead and
//! is not an item, because Word's Reject All of a section's first row removes the section (decision 0020).

use super::names::{is_w, w_attr, w_child};
use super::tree_ops::{create_w, detach};
use super::types::Place;
use crate::api::error::StetError;
use crate::state::PartsView;
use crate::wordml::{BlockKey, NodeId};
use crate::xml::{NodeRef, SpanTree};

/// The id of a content control, its `w:sdtPr/w:id` element.
fn id_element(sdt: &NodeRef<'_>) -> Option<NodeId> {
    w_child(sdt, "sdtPr")
        .and_then(|pr| w_child(&pr, "id"))
        .map(|id| id.id())
}

/// The highest content control id in the package: in every story part, the body, the headers and footers, the
/// footnotes and endnotes and the comments, as they stand in the transaction. Zero when there is none.
pub(super) fn highest_sdt_id(parts: PartsView<'_>) -> Result<i64, StetError> {
    let mut highest = 0i64;
    for spec in &parts.story_specs()?.0 {
        let Some(tree) = parts.tree_opt(&spec.part)? else {
            continue;
        };
        let mut stack = vec![tree.root()];
        while let Some(n) = stack.pop() {
            if is_w(&n, "sdt")
                && let Some(id) = id_element(&n)
                && let Some(v) =
                    w_attr(&tree.node_ref(id), "val").and_then(|v| v.trim().parse::<i64>().ok())
            {
                highest = highest.max(v);
            }
            stack.extend(n.child_elements());
        }
    }
    Ok(highest)
}

/// Gives the content controls in `top`, `top` itself included, ids of their own, counting up from `highest`, the
/// highest id in the package (`highest_sdt_id`): two controls with one id are not what Word writes, in one part or
/// across parts. The walk does not go into a cell, whose content a new row does not copy. A copy also loses its
/// `dataBinding`, since Word would show the bound value in place of the new text and write the same data from two
/// controls, and its `w:showingPlcHdr`, since what it holds is new content and not the placeholder.
pub(super) fn renew_sdt_ids(
    tree: &mut SpanTree,
    top: NodeId,
    mut highest: i64,
) -> Result<(), StetError> {
    let mut copied: Vec<NodeId> = Vec::new();
    let mut dropped: Vec<NodeId> = Vec::new();
    let mut stack = vec![tree.node_ref(top)];
    while let Some(n) = stack.pop() {
        if is_w(&n, "tc") {
            continue;
        }
        if is_w(&n, "sdt") {
            if let Some(id) = id_element(&n) {
                copied.push(id);
            }
            if let Some(pr) = w_child(&n, "sdtPr") {
                // `w:dataBinding`, and `w15:dataBinding` of Word 2013.
                dropped.extend(
                    pr.child_elements()
                        .filter(|b| b.local_name() == "dataBinding" || is_w(b, "showingPlcHdr"))
                        .map(|b| b.id()),
                );
            }
        }
        stack.extend(n.child_elements());
    }
    for b in dropped {
        detach(tree, b)?;
    }
    let uri = super::names::wns(tree);
    for id in copied {
        highest = highest.saturating_add(1).min(i64::from(i32::MAX));
        super::tree_ops::set_w_attr(tree, id, &uri, "val", &highest.to_string())?;
    }
    Ok(())
}

/// The rows of `node`, in document order. A row of a table nested in a cell does not count.
fn rows_of(tree: &SpanTree, node: NodeId) -> Vec<NodeId> {
    let mut rows = Vec::new();
    let mut stack = vec![tree.node_ref(node)];
    while let Some(n) = stack.pop() {
        if is_w(&n, "tr") {
            rows.push(n.id());
            continue;
        }
        if is_w(&n, "tbl") {
            continue;
        }
        // Children in reverse, so that the stack gives them back in document order.
        let kids: Vec<NodeRef<'_>> = n.child_elements().collect();
        stack.extend(kids.into_iter().rev());
    }
    rows
}

/// The row of `item` that a new row placed on `side` of it lands next to: its first row before it, its last row after
/// it. A row of a table nested in a cell does not count.
pub(super) fn edge_row(tree: &SpanTree, item: NodeId, side: Place) -> Option<NodeId> {
    let rows = rows_of(tree, item);
    match side {
        Place::Before => rows.first().copied(),
        Place::After => rows.last().copied(),
    }
}

/// The repeating section of `item`: the nearest `w:sdt` above it, below its table.
fn section_of(tree: &SpanTree, item: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(item).parent();
    while let Some(n) = cur {
        if is_w(&n, "tbl") {
            return None;
        }
        if is_w(&n, "sdt") {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

/// What a tracked new row before `item` goes before when `item` is the first item of its repeating section: the
/// section, or the outermost row-level wrapper above it that starts with the same row. None when another item comes
/// first. Word keeps the start of a control that wraps rows in its first row, so a rejected row at the top of a section
/// takes the start with it, and Word's Reject All then removes the section and every item, keeping their rows.
pub(super) fn before_first_item(tree: &SpanTree, item: NodeId) -> Option<NodeId> {
    let first = edge_row(tree, item, Place::Before)?;
    let section = section_of(tree, item)?;
    let wrappers = wrappers_of(tree, first);
    let at = wrappers.iter().position(|&w| w == section)?;
    let mut top = None;
    for &w in wrappers.iter().skip(at) {
        if edge_row(tree, w, Place::Before) != Some(first) {
            break;
        }
        top = Some(w);
    }
    top
}

/// The repeating section whose start `row` holds: the section, when `row` is the first row of the section's first
/// item (`wordml::repeating_item`, which sees an item through plain controls around its row). Word keeps a control's
/// start in its first row, so accepting a tracked deletion of this row in Word removes the section's control and keeps
/// the rows that remain (decision 0020).
pub(super) fn section_started_by(tree: &SpanTree, row: NodeId) -> Option<NodeId> {
    let item = item_of(tree, row)?;
    before_first_item(tree, item)?;
    (edge_row(tree, item, Place::Before) == Some(row))
        .then(|| section_of(tree, item))
        .flatten()
}

/// True if `section` holds a row other than `row` that no tracked change deletes: a row that stays when the deletions
/// are accepted.
pub(super) fn holds_other_row(tree: &SpanTree, section: NodeId, row: NodeId) -> bool {
    rows_of(tree, section).into_iter().any(|r| {
        r != row
            && !w_child(&tree.node_ref(r), "trPr")
                .is_some_and(|pr| pr.child_elements().any(|c| is_w(&c, "del")))
    })
}

impl super::compiler::Compiler<'_, '_> {
    /// True if Word's Accept All would remove the repeating-section control around rows that stay: `row`, deleted
    /// tracked, holds the start of its section, and the section holds a row that no tracked change deletes. Asked after
    /// the last operation of a run, so a batch that deletes every item's row is not warned.
    pub(crate) fn section_unwrapped_by_word(&self, row: &BlockKey) -> Result<bool, StetError> {
        let tree = self.tree_ro(row.part())?;
        if crate::wordml::Row::new(tree, row.node()).is_none() {
            return Ok(false);
        }
        Ok(section_started_by(tree, row.node())
            .is_some_and(|section| holds_other_row(tree, section, row.node())))
    }
}

/// The repeating-section item that `row` is the content of (`wordml::repeating_item`).
pub(super) fn item_of(tree: &SpanTree, row: NodeId) -> Option<NodeId> {
    crate::wordml::repeating_item(tree.node_ref(row)).map(|(item, _)| item.id())
}

/// Puts the new row `row`, not yet in the tree, in a new item made from `item`: a copy of its `w:sdtPr` and
/// `w:sdtEndPr` around a `w:sdtContent` that holds the row. Returns the new item, which is not in the tree
/// either. The ids of the new item and of the controls in the row are renewed by [`renew_sdt_ids`], which the
/// caller runs on what this returns, and which also drops the copy's `dataBinding` and `w:showingPlcHdr`.
pub(super) fn new_item_around(
    tree: &mut SpanTree,
    item: NodeId,
    row: NodeId,
) -> Result<NodeId, StetError> {
    let uri = tree.node_ref(item).namespace().to_owned();
    let props: Vec<NodeId> = tree
        .node_ref(item)
        .child_elements()
        .filter(|c| is_w(c, "sdtPr") || is_w(c, "sdtEndPr"))
        .map(|c| c.id())
        .collect();
    let sdt = create_w(tree, &uri, "sdt")?;
    for p in props {
        let copy = tree.edit().clone_subtree(p)?;
        tree.edit().append_child(sdt, copy)?;
    }
    let content = create_w(tree, &uri, "sdtContent")?;
    tree.edit().append_child(sdt, content)?;
    tree.edit().append_child(content, row)?;
    Ok(sdt)
}

/// The wrappers between `row` and its table, innermost first: the row-level content controls and custom XML
/// elements that hold it. A control's `w:sdtContent` is not among them; the control is.
pub(super) fn wrappers_of(tree: &SpanTree, row: NodeId) -> Vec<NodeId> {
    below_table(tree, row)
        .into_iter()
        .filter(|n| !is_w(n, "sdtContent"))
        .map(|n| n.id())
        .collect()
}

/// The elements above `row` and below its table, innermost first; none if no table holds the row.
fn below_table(tree: &SpanTree, row: NodeId) -> Vec<NodeRef<'_>> {
    let mut out = Vec::new();
    let mut cur = tree.node_ref(row).parent();
    while let Some(n) = cur {
        if is_w(&n, "tbl") {
            return out;
        }
        out.push(n);
        cur = n.parent();
    }
    Vec::new()
}
