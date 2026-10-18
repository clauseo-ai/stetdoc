//! One grid for the rows of two tables (design `compare.md` 4.5).
//!
//! The redline of a table compare pairs keeps the revised table, with the original's deleted rows copied into it. A copied row
//! counts its columns in the original's grid. When that grid starts with the same widths as the revised one, the row fits as it
//! is, short of the grid's end if the revised table is wider, as Word itself leaves rows when it rejects inserted cells
//! (`RP035`). Otherwise the table takes the grid of every column edge of both grids, and each row's cells span the columns their
//! own edges cover there, so that every reader sees each row at its own widths and none drops a cell that would stand past the
//! grid's end. A row that ends short stays short, with no `w:gridAfter`, which LibreOffice draws as a border past the row.

use super::compiler::Compiler;
use super::names::{is_w, wns};
use super::outcome::Effect;
use super::props_edit::{ensure_chain, place};
use super::tree_ops::{child_element_ids, create_w, detach, insert, set_w_attr};
use crate::api::error::StetError;
use crate::wordml::{BlockKey, NodeId, Row, Table};
use crate::xml::SpanTree;

/// How one row takes up its grid: the columns before its first cell, each cell's span and node, and the columns after its last.
struct Layout {
    row: NodeId,
    before: usize,
    cells: Vec<(NodeId, usize)>,
    after: usize,
    copied: bool,
}

/// A whole number of grid columns written in a row's property, zero if it is missing or not one.
fn count(row: &Row<'_>, name: &str) -> usize {
    row.props()
        .and_then(|p| p.val(name))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
}

/// The edges of a grid: 0, then the running sum of the widths.
fn edges(grid: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = vec![0];
    for w in grid {
        let last = out.last().copied().unwrap_or(0);
        out.push(last.saturating_add(*w));
    }
    out
}

/// The grid of `table` and how its rows take it up, or `None` when a width is missing or not positive, or a row takes more
/// columns than its own grid has: a table that this rule leaves as it is.
fn layouts(
    tree: &SpanTree,
    table: NodeId,
    old: &[i64],
    copies: &[NodeId],
) -> Option<(Vec<i64>, Vec<Layout>)> {
    let t = Table::new(tree, table)?;
    let new: Vec<i64> = t.grid().collect::<Option<Vec<_>>>()?;
    if new.iter().chain(old).any(|w| *w <= 0) {
        return None;
    }
    let mut out = Vec::new();
    for row in t.rows() {
        let copied = copies.contains(&row.id());
        let cells: Vec<(NodeId, usize)> = row
            .cells()
            .map(|c| {
                (
                    c.element().id(),
                    usize::try_from(c.grid_span()).unwrap_or(1),
                )
            })
            .collect();
        let l = Layout {
            row: row.id(),
            before: count(&row, "gridBefore"),
            after: count(&row, "gridAfter"),
            cells,
            copied,
        };
        let used = l.before + l.cells.iter().map(|c| c.1).sum::<usize>() + l.after;
        if used > if copied { old.len() } else { new.len() } {
            return None;
        }
        out.push(l);
    }
    Some((new, out))
}

impl Compiler<'_, '_> {
    /// Fits the rows of `table`, some of which (`copies`) are deleted copies of rows of a table whose grid was `old`, to one
    /// grid, as the module says. Does nothing when the copies fit the table's grid as they are.
    pub(crate) fn fit_grid(
        &mut self,
        table: &BlockKey,
        old: &[i64],
        copies: &[NodeId],
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.fit_grid_inner(table, old, copies) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn fit_grid_inner(
        &mut self,
        table: &BlockKey,
        old: &[i64],
        copies: &[NodeId],
    ) -> Result<(), StetError> {
        let part = table.part().clone();
        let Some((new, rows)) = layouts(self.tree_ro(&part)?, table.node(), old, copies) else {
            return Ok(());
        };
        let fits = rows.iter().filter(|r| r.copied).all(|r| {
            let used = r.before + r.cells.iter().map(|c| c.1).sum::<usize>() + r.after;
            old.get(..used) == new.get(..used)
        });
        if old == new.as_slice() || fits {
            return Ok(());
        }
        let (old_edges, new_edges) = (edges(old), edges(&new));
        let mut union: Vec<i64> = old_edges.iter().chain(&new_edges).copied().collect();
        union.sort_unstable();
        union.dedup();
        let at = |e: i64| union.binary_search(&e).unwrap_or(0);
        let tree = self.tx.part_mut(&part)?;
        let uri = wns(tree);
        for r in &rows {
            let own = if r.copied { &old_edges } else { &new_edges };
            let edge = |col: usize| own.get(col).copied().unwrap_or(0);
            let mut col = r.before;
            let first = at(edge(col));
            for &(cell, span) in &r.cells {
                let spans = at(edge(col + span)) - at(edge(col));
                set_span(tree, &uri, cell, span, spans)?;
                col += span;
            }
            let after = at(edge(col + r.after)) - at(edge(col));
            set_count(tree, &uri, r.row, "gridBefore", r.before, first)?;
            set_count(tree, &uri, r.row, "gridAfter", r.after, after)?;
        }
        let widths: Vec<i64> = union
            .iter()
            .zip(union.iter().skip(1))
            .map(|(a, b)| b - a)
            .collect();
        set_grid(tree, &uri, table.node(), &widths)?;
        self.touch(table);
        Ok(())
    }
}

/// Writes a cell's `w:gridSpan` when it changes: none for one column.
fn set_span(
    tree: &mut SpanTree,
    uri: &str,
    cell: NodeId,
    was: usize,
    now: usize,
) -> Result<(), StetError> {
    if was == now {
        return Ok(());
    }
    let (tcpr, _) = ensure_chain(tree, cell, &["tcPr"])?;
    let found = child_element_ids(tree, tcpr)
        .into_iter()
        .find(|&c| is_w(&tree.node_ref(c), "gridSpan"));
    if now <= 1 {
        if let Some(s) = found {
            detach(tree, s)?;
        }
        return Ok(());
    }
    let span = match found {
        Some(s) => s,
        None => {
            let s = create_w(tree, uri, "gridSpan")?;
            place(tree, tcpr, s)?;
            s
        }
    };
    set_w_attr(tree, span, uri, "val", &now.to_string())
}

/// Writes a row's `w:gridBefore` or `w:gridAfter` when it changes: none for zero.
fn set_count(
    tree: &mut SpanTree,
    uri: &str,
    row: NodeId,
    name: &str,
    was: usize,
    now: usize,
) -> Result<(), StetError> {
    if was == now {
        return Ok(());
    }
    let (trpr, _) = ensure_chain(tree, row, &["trPr"])?;
    let found = child_element_ids(tree, trpr)
        .into_iter()
        .find(|&c| is_w(&tree.node_ref(c), name));
    if now == 0 {
        if let Some(s) = found {
            detach(tree, s)?;
        }
        return Ok(());
    }
    let el = match found {
        Some(s) => s,
        None => {
            let s = create_w(tree, uri, name)?;
            place(tree, trpr, s)?;
            s
        }
    };
    set_w_attr(tree, el, uri, "val", &now.to_string())
}

/// Replaces the `w:gridCol` children of the table's `w:tblGrid` with one per width.
fn set_grid(
    tree: &mut SpanTree,
    uri: &str,
    table: NodeId,
    widths: &[i64],
) -> Result<(), StetError> {
    let Some(grid) = child_element_ids(tree, table)
        .into_iter()
        .find(|&c| is_w(&tree.node_ref(c), "tblGrid"))
    else {
        return Ok(());
    };
    let cols: Vec<NodeId> = child_element_ids(tree, grid)
        .into_iter()
        .filter(|&c| is_w(&tree.node_ref(c), "gridCol"))
        .collect();
    let next = cols
        .last()
        .and_then(|&c| super::tree_ops::next_sibling_of(tree, c));
    for c in cols {
        detach(tree, c)?;
    }
    for w in widths {
        let col = create_w(tree, uri, "gridCol")?;
        set_w_attr(tree, col, uri, "w", &w.to_string())?;
        insert(tree, grid, next, col)?;
    }
    Ok(())
}
