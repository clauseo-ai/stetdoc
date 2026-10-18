//! Maps `PlannedOp` into `Op`: the one-to-one mapping that `Edit::files` makes (design `document-and-ops.md` 2.4).
//!
//! `Planned.ops` is in the order the operations must be applied, and `Planned.origins[i]` stays attached to operation `i`. The
//! batch carries the base token of the files, which `to_batch` has just checked against the state.

use crate::api::batch::Batch;
use crate::api::files::{Planned, PlannedOp};
use crate::api::op::{DeleteOp, InsertOp, MergeOp, Op, ReplaceOp, RowAction, RowOp, SplitOp};

/// The batch that a plan comes to, naming the base token of the files.
pub(crate) fn batch(base: &str, planned: &Planned) -> Batch {
    Batch {
        base: base.to_string(),
        ops: planned.ops.iter().map(op_of).collect(),
    }
}

fn op_of(planned: &PlannedOp) -> Op {
    match planned {
        PlannedOp::Replace {
            block,
            start,
            end,
            with,
        } => Op::Replace(ReplaceOp {
            r#ref: block.clone(),
            with: with.clone(),
            find: None,
            occurrence: None,
            start: Some(*start),
            end: Some(*end),
        }),
        PlannedOp::Insert {
            anchor,
            place,
            paragraphs,
        } => Op::Insert(InsertOp {
            r#ref: anchor.clone(),
            place: *place,
            paragraphs: paragraphs.clone(),
        }),
        PlannedOp::Delete { block } => Op::Delete(DeleteOp {
            r#ref: block.clone(),
            to: None,
            find: None,
            occurrence: None,
            start: None,
            end: None,
        }),
        PlannedOp::RowDelete { row } => Op::Row(RowOp {
            r#ref: row.clone(),
            action: RowAction::Delete,
            place: None,
            cells: None,
        }),
        PlannedOp::Split { block, at } => Op::Split(SplitOp {
            r#ref: block.clone(),
            at: Some(*at),
            before: None,
            after: None,
        }),
        PlannedOp::Merge { first, second } => Op::Merge(MergeOp {
            r#ref: first.clone(),
            second: Some(second.clone()),
        }),
    }
}
