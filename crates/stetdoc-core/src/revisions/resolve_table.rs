//! Resolving rows and cells (design `revisions.md`, 3B and 3.4). Accepting a deleted row or rejecting an
//! inserted one removes the row, and the refs of what was in it go with the reason of the change. A cell
//! that is removed because it was deleted widens the nearest live cell before it, so that the row still
//! covers the grid. A table left with no row is removed.

use super::cleanup::{is_attached, prune_from, remove_hoisting};
use super::names::{is_w, w_attr};
use super::props_edit::apply_edit;
use super::resolve::{Engine, Removed};
use super::tree_ops::{detach, parent_of};
use super::types::{PropEdit, PropElement};
use crate::api::error::StetError;
use crate::state::Retired;
use crate::wordml::BlockKey;
use crate::wordml::{NodeId, RevKind};
use crate::xml::SpanTree;

/// What a queued row revision asks for.
#[derive(Clone, Copy, Debug)]
pub(super) struct RowOp {
    pub marker: NodeId,
    /// Remove the row, and not only the revision marker.
    pub remove: bool,
}

/// What a queued cell revision asks for.
#[derive(Clone, Copy, Debug)]
pub(super) struct CellOp {
    pub marker: NodeId,
    pub kind: RevKind,
}

impl Engine<'_, '_> {
    /// Queues a revision of a row: the `w:ins` or `w:del` in `w:trPr`.
    pub(super) fn resolve_row(&mut self, marker: NodeId, kind: RevKind) {
        let inserted = kind == RevKind::RowIns;
        self.rows.push(RowOp {
            marker,
            remove: inserted != self.accept,
        });
    }

    /// Queues a revision of a cell: `w:cellIns`, `w:cellDel` or `w:cellMerge` in `w:tcPr`.
    pub(super) fn resolve_cell(&mut self, marker: NodeId, kind: RevKind) {
        self.cells.push(CellOp { marker, kind });
    }

    /// Applies the queued row revisions.
    pub(super) fn run_rows(&mut self) -> Result<(), StetError> {
        let ops = std::mem::take(&mut self.rows);
        for op in ops {
            let tree = self.tx.part_mut(&self.part)?;
            if !is_attached(tree, op.marker) {
                continue;
            }
            let Some(trpr) = parent_of(tree, op.marker) else {
                continue;
            };
            let Some(row) = parent_of(tree, trpr) else {
                continue;
            };
            if !op.remove {
                detach(tree, op.marker)?;
                prune_from(tree, trpr)?;
                self.dirty.push(row);
                continue;
            }
            self.remove_row(row)?;
        }
        Ok(())
    }

    /// Removes a row with what it holds, and the table if it was the last row.
    fn remove_row(&mut self, row: NodeId) -> Result<(), StetError> {
        let part = self.part.clone();
        let rejected = !self.accept;
        let tree = self.tx.part_mut(&part)?;
        // A row may stand in a row-level content control or a custom XML element inside its table: the
        // table is the nearest one up, and the wrapper goes when the row was all it held.
        let table = ancestor_table(tree, row);
        let wrapper = table
            .and_then(|t| child_toward(tree, t, row))
            .filter(|w| *w != row);
        let blocks = blocks_within(tree, row);
        let (to, swallowed) = remove_hoisting(tree, row)?;
        self.swallowed.extend(swallowed);
        if is_w(&tree.node_ref(to), "p") {
            self.dirty.push(to);
        }
        for (node, _) in blocks {
            self.record_removed(&BlockKey::new(part.clone(), node), rejected)?;
        }
        if let Some(w) = wrapper {
            let tree = self.tx.part_mut(&part)?;
            if is_attached(tree, w) && !holds_row(tree, w) {
                let (to, swallowed) = remove_hoisting(tree, w)?;
                self.swallowed.extend(swallowed);
                if is_w(&tree.node_ref(to), "p") {
                    self.dirty.push(to);
                }
            }
        }
        if let Some(table) = table {
            let tree = self.tx.part_mut(&part)?;
            let has_row = holds_row(tree, table);
            if !has_row {
                let tkey = BlockKey::new(part.clone(), table);
                self.gap_nexts
                    .extend(super::resolve_mark::next_paragraph(tree, table));
                // The range markers that the rows hoisted, and any of the table's own, move out with it, as
                // they did out of each row: a bookmark that starts in the table and ends after it keeps its
                // start (TC-table-converttotable).
                let (to, swallowed) = remove_hoisting(tree, table)?;
                self.swallowed.extend(swallowed);
                if is_w(&tree.node_ref(to), "p") {
                    self.dirty.push(to);
                }
                self.record_removed(&tkey, rejected)?;
            } else {
                self.dirty.push(table);
            }
        }
        Ok(())
    }

    /// Notes that a block left the tree, and declares it if its insertion was rejected.
    pub(super) fn record_removed(
        &mut self,
        key: &BlockKey,
        rejected: bool,
    ) -> Result<(), StetError> {
        self.removed.push(Removed {
            key: key.clone(),
            rejected,
        });
        if rejected && self.tx.parts().ref_of(key).is_some() {
            self.tx.retire(key, Retired::Rejected)?;
        }
        Ok(())
    }

    /// Applies the queued cell revisions.
    pub(super) fn run_cells(&mut self) -> Result<(), StetError> {
        let ops = std::mem::take(&mut self.cells);
        for op in ops {
            let part = self.part.clone();
            let tree = self.tx.part_mut(&part)?;
            if !is_attached(tree, op.marker) {
                continue;
            }
            let Some(tcpr) = parent_of(tree, op.marker) else {
                continue;
            };
            let Some(cell) = parent_of(tree, tcpr) else {
                continue;
            };
            let accept = self.accept;
            let remove_cell = match op.kind {
                RevKind::CellIns => !accept,
                RevKind::CellDel => accept,
                _ => false,
            };
            if remove_cell {
                let span = grid_span(tree, cell);
                let blocks = blocks_within(tree, cell);
                let widen = op.kind == RevKind::CellDel;
                let target = if widen {
                    neighbour_cell(tree, cell)
                } else {
                    None
                };
                let (to, swallowed) = remove_hoisting(tree, cell)?;
                self.swallowed.extend(swallowed);
                if is_w(&self.tx.part_mut(&part)?.node_ref(to), "p") {
                    self.dirty.push(to);
                }
                if let Some(t) = target {
                    set_grid_span(self.tx.part_mut(&part)?, t, span)?;
                    self.dirty.push(t);
                }
                for (node, _) in blocks {
                    self.record_removed(&BlockKey::new(part.clone(), node), !accept)?;
                }
                continue;
            }
            if op.kind == RevKind::CellMerge && accept {
                let value = w_attr(&tree.node_ref(op.marker), "vMerge").map(|v| v.into_owned());
                let ns = tree.node_ref(tcpr).namespace().to_owned();
                let attrs = match value.as_deref() {
                    Some("rest") => vec![(ns.clone(), "val".to_owned(), "restart".to_owned())],
                    _ => Vec::new(),
                };
                let edit = PropEdit {
                    set: vec![PropElement {
                        ns,
                        local: "vMerge".to_owned(),
                        attrs,
                        children: Vec::new(),
                    }],
                    clear: Vec::new(),
                };
                apply_edit(tree, tcpr, &edit)?;
            }
            detach(tree, op.marker)?;
            prune_from(tree, tcpr)?;
            self.dirty.push(cell);
        }
        Ok(())
    }

    /// Gives a story that a removal left with no block (the body, a note, a comment, a header or a footer)
    /// the empty paragraph it must have. A story is never left with no paragraph; a note that no reference
    /// names any more is removed elsewhere, and one that stays keeps one. A reject keeps the last paragraph of a
    /// story itself (`resolve_mark`), so on a reject this fills only a story whose last block was an inserted table.
    pub(super) fn fill_empty_stories(&mut self) -> Result<(), StetError> {
        let part = self.part.clone();
        let tree = self.tx.part_mut(&part)?;
        let root = tree.root();
        let mut stories: Vec<NodeId> = Vec::new();
        if matches!(root.local_name(), "hdr" | "ftr") {
            stories.push(root.id());
        }
        for c in root.child_elements() {
            if is_w(&c, "body")
                || is_w(&c, "footnote")
                || is_w(&c, "endnote")
                || is_w(&c, "comment")
            {
                stories.push(c.id());
            }
        }
        for s in stories {
            let (empty, sect) = {
                let n = tree.node_ref(s);
                let empty = !n.child_elements().any(|x| {
                    is_w(&x, "p") || is_w(&x, "tbl") || is_w(&x, "sdt") || is_w(&x, "customXml")
                });
                let sect = n
                    .child_elements()
                    .find(|x| is_w(x, "sectPr"))
                    .map(|x| x.id());
                (empty, sect)
            };
            if !empty {
                continue;
            }
            let uri = super::names::wns(tree);
            let p = super::tree_ops::create_w(tree, &uri, "p")?;
            // Range markers and comment references left between blocks go into it, in order.
            let loose: Vec<NodeId> = tree
                .node_ref(s)
                .child_elements()
                .filter(|x| super::join::is_leftover(x))
                .map(|x| x.id())
                .collect();
            match sect {
                Some(sp) => super::tree_ops::insert(tree, s, Some(sp), p)?,
                None => tree.edit().append_child(s, p)?,
            }
            for l in loose {
                super::tree_ops::move_to(tree, l, p, None)?;
            }
        }
        Ok(())
    }

    /// Gives a cell that has no block, or that the resolution left ending with a table, the empty paragraph
    /// that it must end with. A cell that ended with a table before (Word and LibreOffice write such cells)
    /// is left as it is, or accepting a change near it would add a paragraph to the Final view: a cell is
    /// named here itself only when what it held was removed, and a paragraph, row or table that changed
    /// inside it leaves its last block what it was.
    pub(super) fn fill_empty_cells(&mut self) -> Result<(), StetError> {
        let part = self.part.clone();
        let dirty = self.dirty.clone();
        for n in dirty {
            let tree = self.tx.part_mut(&part)?;
            if !is_attached(tree, n) {
                continue;
            }
            let mut cur = Some(tree.node_ref(n));
            while let Some(c) = cur {
                // A row or a table that changed says nothing of the cell around its table.
                if is_w(&c, "tr") || is_w(&c, "tbl") {
                    break;
                }
                if is_w(&c, "tc") {
                    let last_block = c
                        .child_elements()
                        .filter(|x| is_w(x, "p") || is_w(x, "tbl") || is_w(x, "sdt"))
                        .last();
                    let fill = match last_block {
                        None => true,
                        Some(b) => is_w(&b, "tbl") && c.id() == n,
                    };
                    if fill {
                        let tc = c.id();
                        let uri = super::names::wns(tree);
                        let p = super::tree_ops::create_w(tree, &uri, "p")?;
                        tree.edit().append_child(tc, p)?;
                    }
                    break;
                }
                cur = c.parent();
            }
        }
        Ok(())
    }
}

/// The nearest table above a row.
fn ancestor_table(tree: &SpanTree, row: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(row).parent();
    while let Some(n) = cur {
        if is_w(&n, "tbl") {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

/// The child of `ancestor` that holds `node` (or is it).
pub(super) fn child_toward(tree: &SpanTree, ancestor: NodeId, node: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(node);
    loop {
        let parent = cur.parent()?;
        if parent.id() == ancestor {
            return Some(cur.id());
        }
        cur = parent;
    }
}

/// True if a row of this table stands in `el`, directly or in a wrapper, but not in a nested table.
pub(super) fn holds_row(tree: &SpanTree, el: NodeId) -> bool {
    let mut stack: Vec<_> = tree.node_ref(el).child_elements().collect();
    while let Some(n) = stack.pop() {
        if is_w(&n, "tr") {
            return true;
        }
        if !is_w(&n, "tbl") {
            stack.extend(n.child_elements());
        }
    }
    false
}

/// The paragraphs, tables and rows inside an element, with their kind letters, outermost first.
fn blocks_within(tree: &SpanTree, node: NodeId) -> Vec<(NodeId, char)> {
    let mut out = vec![];
    let mut stack = vec![tree.node_ref(node)];
    while let Some(n) = stack.pop() {
        if is_w(&n, "p") {
            out.push((n.id(), 'p'));
            continue;
        }
        if is_w(&n, "tbl") {
            out.push((n.id(), 't'));
        }
        if is_w(&n, "tr") {
            out.push((n.id(), 'r'));
        }
        stack.extend(n.child_elements());
    }
    out
}

/// The grid span of a cell.
fn grid_span(tree: &SpanTree, cell: NodeId) -> u32 {
    tree.node_ref(cell)
        .child_elements()
        .find(|c| is_w(c, "tcPr"))
        .and_then(|pr| pr.child_elements().find(|c| is_w(c, "gridSpan")))
        .and_then(|g| w_attr(&g, "val").and_then(|v| v.trim().parse::<u32>().ok()))
        .filter(|n| *n >= 1)
        .unwrap_or(1)
}

/// The cell that takes a removed cell's width: the nearest cell before it, else the next one.
fn neighbour_cell(tree: &SpanTree, cell: NodeId) -> Option<NodeId> {
    let n = tree.node_ref(cell);
    let mut prev = n.prev_sibling();
    while let Some(p) = prev {
        if is_w(&p, "tc") {
            return Some(p.id());
        }
        prev = p.prev_sibling();
    }
    let mut next = n.next_sibling();
    while let Some(x) = next {
        if is_w(&x, "tc") {
            return Some(x.id());
        }
        next = x.next_sibling();
    }
    None
}

/// Widens a cell by `extra` grid columns.
fn set_grid_span(tree: &mut SpanTree, cell: NodeId, extra: u32) -> Result<(), StetError> {
    let now = grid_span(tree, cell);
    let (tcpr, _) = super::props_edit::ensure_chain(tree, cell, &["tcPr"])?;
    let ns = tree.node_ref(tcpr).namespace().to_owned();
    let edit = PropEdit {
        set: vec![PropElement {
            ns: ns.clone(),
            local: "gridSpan".to_owned(),
            attrs: vec![(ns, "val".to_owned(), (now + extra).to_string())],
            children: Vec::new(),
        }],
        clear: Vec::new(),
    };
    apply_edit(tree, tcpr, &edit)
}
