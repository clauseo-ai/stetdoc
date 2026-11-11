//! Merged cells around a row that is inserted or removed (design `revisions.md`, 3.4).
//!
//! A merge is found by grid column, `w:gridBefore` of the row plus the spans of the cells before, never by the
//! index of a cell. A new row continues a vertical merge only where the merge goes on past it: inside the
//! merge, and not below its last row or above its first, as Word's Insert Rows does. A cell of the new row that
//! continues a merge, vertical or a legacy `w:hMerge`, takes no text, because Word shows only a merge's first
//! cell. When the row that holds a vertical merge's `restart` goes, the continuation below it becomes the new
//! `restart`, as Word's Delete Rows does; the content of the old start goes with its row.

use super::errors::invalid;
use super::names::w_attr;
use super::props_edit::{apply_edit, find_chain};
use super::types::{Place, PropEdit, PropElement};
use crate::api::error::StetError;
use crate::wordml::{Cell, NodeId, Row, Table, VMerge};
use crate::xml::SpanTree;

/// What a cell of a new row does with the vertical merge of the anchor's cell it was copied from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NewMerge {
    /// The cell is in no vertical merge: its properties stay as copied.
    Keep,
    /// The cell continues the vertical merge above it.
    Continue,
    /// The cell leaves the merge: its `w:vMerge` goes.
    Clear,
}

/// The plan for one cell of a new row.
#[derive(Clone, Copy, Debug)]
pub(super) struct NewCell {
    /// What happens to its `w:vMerge`.
    pub merge: NewMerge,
    /// It continues a legacy horizontal merge (`w:hMerge` that is not `restart`), copied from the anchor.
    pub h_continue: bool,
}

/// The grid column where each cell of a row starts.
fn cells_with_columns<'a>(row: &Row<'a>) -> Vec<(Cell<'a>, usize)> {
    let mut column = row
        .props()
        .and_then(|p| p.val("gridBefore"))
        .and_then(|v| v.trim().parse::<i64>().ok())
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(0);
    let mut out = Vec::new();
    for c in row.cells() {
        let span = usize::try_from(c.grid_span()).unwrap_or(usize::MAX);
        out.push((c, column));
        column = column.saturating_add(span);
    }
    out
}

/// The row after `row` in its table, if there is one.
fn next_row(tree: &SpanTree, table: NodeId, row: NodeId) -> Option<NodeId> {
    let t = Table::new(tree, table)?;
    let mut rows = t.rows().map(|r| r.id()).skip_while(|r| *r != row);
    rows.next()?;
    rows.next()
}

/// The cell of `row` that starts at grid column `column` and continues a vertical merge.
fn continuation_at(tree: &SpanTree, row: NodeId, column: usize) -> Option<NodeId> {
    let r = Row::new(tree, row)?;
    cells_with_columns(&r)
        .into_iter()
        .find(|(c, col)| *col == column && c.v_merge() == VMerge::Continue)
        .map(|(c, _)| c.id())
}

/// True if the cell continues a legacy horizontal merge.
fn h_continues(cell: &Cell<'_>) -> bool {
    cell.props()
        .and_then(|p| p.get("hMerge"))
        .is_some_and(|m| w_attr(&m, "val").as_deref() != Some("restart"))
}

/// The plan for each cell of a row inserted on `side` of `anchor`, in the anchor's cell order.
pub(super) fn plan_new_cells(
    tree: &SpanTree,
    table: NodeId,
    anchor: NodeId,
    side: Place,
) -> Vec<NewCell> {
    let Some(row) = Row::new(tree, anchor) else {
        return Vec::new();
    };
    let below = match side {
        Place::After => next_row(tree, table, anchor),
        Place::Before => None,
    };
    cells_with_columns(&row)
        .into_iter()
        .map(|(c, column)| {
            let merge = match (side, c.v_merge()) {
                (_, VMerge::None) => NewMerge::Keep,
                (Place::Before, VMerge::Restart) => NewMerge::Clear,
                (Place::Before, VMerge::Continue) => NewMerge::Continue,
                (Place::After, VMerge::Restart | VMerge::Continue) => {
                    match below.and_then(|b| continuation_at(tree, b, column)) {
                        Some(_) => NewMerge::Continue,
                        None => NewMerge::Clear,
                    }
                }
            };
            NewCell {
                merge,
                h_continue: h_continues(&c),
            }
        })
        .collect()
}

/// Refuses text for a cell of the new row that continues a merge, which Word would not show.
pub(super) fn refuse_hidden_text(plan: &[NewCell], cells: &[String]) -> Result<(), StetError> {
    for (i, (cell, text)) in plan.iter().zip(cells).enumerate() {
        if text.is_empty() {
            continue;
        }
        let merge = if cell.merge == NewMerge::Continue {
            "the vertical merge from the row above"
        } else if cell.h_continue {
            "the horizontal merge from the cell before it"
        } else {
            continue;
        };
        let n = i.saturating_add(1);
        return Err(invalid(format!(
            "column {n} of the new row continues {merge}, and Word shows only the first cell of a merge, \
             so its text would never be seen. Pass \"\" for column {n}."
        )));
    }
    Ok(())
}

/// Writes the plan's `w:vMerge` into the new cell's `w:tcPr`.
pub(super) fn apply_new_merge(
    tree: &mut SpanTree,
    tcpr: NodeId,
    ns: &str,
    merge: NewMerge,
) -> Result<(), StetError> {
    let edit = match merge {
        NewMerge::Keep => return Ok(()),
        NewMerge::Continue => PropEdit {
            set: vec![PropElement {
                ns: ns.to_owned(),
                local: "vMerge".to_owned(),
                attrs: Vec::new(),
                children: Vec::new(),
            }],
            clear: Vec::new(),
        },
        NewMerge::Clear => PropEdit {
            set: Vec::new(),
            clear: vec![(ns.to_owned(), "vMerge".to_owned())],
        },
    };
    apply_edit(tree, tcpr, &edit)
}

/// Before `row` leaves `table`: each vertical merge that starts in it and goes on in the next row starts
/// there instead, with `w:vMerge w:val="restart"`. Returns the cells that became a start.
pub(super) fn promote_below(
    tree: &mut SpanTree,
    table: NodeId,
    row: NodeId,
) -> Result<Vec<NodeId>, StetError> {
    let Some(below) = next_row(tree, table, row) else {
        return Ok(Vec::new());
    };
    let starts: Vec<usize> = match Row::new(tree, row) {
        Some(r) => cells_with_columns(&r)
            .into_iter()
            .filter(|(c, _)| c.v_merge() == VMerge::Restart)
            .map(|(_, col)| col)
            .collect(),
        None => Vec::new(),
    };
    let mut promoted = Vec::new();
    for column in starts {
        let Some(cell) = continuation_at(tree, below, column) else {
            continue;
        };
        let Some(tcpr) = find_chain(tree, cell, &["tcPr"]) else {
            continue;
        };
        let ns = tree.node_ref(tcpr).namespace().to_owned();
        let edit = PropEdit {
            set: vec![PropElement {
                ns: ns.clone(),
                local: "vMerge".to_owned(),
                attrs: vec![(ns, "val".to_owned(), "restart".to_owned())],
                children: Vec::new(),
            }],
            clear: Vec::new(),
        };
        apply_edit(tree, tcpr, &edit)?;
        promoted.push(cell);
    }
    Ok(promoted)
}
