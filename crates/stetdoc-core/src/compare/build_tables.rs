//! A table of the original replaced by one of the revised document (design `compare.md` 4.5).
//!
//! The deletions of a gap go right before its insertions, so a table replaced by another would follow it with nothing between,
//! which Word does not keep as two tables: it shows them apart with untracked space that stays after accepting or rejecting. So
//! the pairing makes the two one table of deleted rows and inserted rows (`tables::join_replaced_tables`), whatever their grids.
//! The original's rows count their columns in the original's grid, and once a story is written `Compiler::fit_grid` gives every
//! row of such a table the same grid.

use super::build::Builder;
use super::plan::StoryPlan;
use crate::wordml::{BlockKey, NodeId, Table};
use std::sync::Arc;

/// A table of the redline that holds deleted copies of rows of an original table: the table, the original's grid and the copies.
pub(super) struct CopiedRows {
    pub table: BlockKey,
    pub grid: Vec<i64>,
    pub rows: Vec<NodeId>,
}

impl Builder<'_, '_, '_> {
    /// Notes the deleted copies of original rows that `delete` placed: `pairs` holds each original row with its copy.
    pub(super) fn note_copied_rows(&mut self, sp: &StoryPlan, pairs: &[(NodeId, BlockKey)]) {
        for (orig, copy) in pairs {
            let Some((table, grid)) = self.grid_of_row(&sp.part_a, *orig) else {
                continue;
            };
            let _ = table;
            let Some(red) = self.table_of_row(copy) else {
                continue;
            };
            match self.copied_rows.iter_mut().find(|c| c.table == red) {
                Some(c) => c.rows.push(copy.node()),
                None => self.copied_rows.push(CopiedRows {
                    table: red,
                    grid,
                    rows: vec![copy.node()],
                }),
            }
        }
    }

    /// Fits the grid of every table that took copied rows in this story (`Compiler::fit_grid`).
    pub(super) fn fit_grids(&mut self) {
        for c in std::mem::take(&mut self.copied_rows) {
            match self.c.fit_grid(&c.table, &c.grid, &c.rows) {
                Ok(fx) => self.record(&fx),
                Err(e) => self.broken(c.table.clone(), &e),
            }
        }
    }

    /// The table of an original row and its grid, if every column of the grid has a width.
    fn grid_of_row(&self, part: &Arc<str>, row: NodeId) -> Option<(NodeId, Vec<i64>)> {
        let view = self.original.parts();
        let tree = view.tree(view.id(part)?).ok()?;
        let table = table_above(tree, row)?;
        let grid = Table::new(tree, table)?
            .grid()
            .collect::<Option<Vec<_>>>()?;
        Some((table, grid))
    }

    /// The table of a row of the redline.
    fn table_of_row(&self, row: &BlockKey) -> Option<BlockKey> {
        let view = self.c.tx().parts();
        let tree = view.tree(view.id(row.part())?).ok()?;
        if tree.node_ref(row.node()).local_name() != "tr" {
            return None;
        }
        table_above(tree, row.node()).map(|t| BlockKey::new(row.part().clone(), t))
    }
}

/// The nearest `w:tbl` above `node`.
fn table_above(tree: &crate::xml::SpanTree, node: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(node).parent();
    while let Some(n) = cur {
        if n.local_name() == "tbl" {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}
