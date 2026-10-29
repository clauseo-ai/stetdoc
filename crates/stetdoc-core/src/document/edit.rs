//! `Document::edit`: the pipeline of design `document-and-ops.md` 3.1.
//!
//! Nothing in the document changes until the last step, so every failure before it leaves the document as it was, and **a dry
//! run is the same path** up to the end, which drops the state it committed instead of installing it. The steps: the
//! envelope (the stamp, the batch), the stage (a `Tx`, the scope), the apply, the first pass of the scope guard, the commit,
//! the receipt, and the install. An edit of readable files is the same path after its plan (`edit_files.rs`). Between the guard
//! and the commit `check::edit` validates the units that changed, and the second pass of the guard reads what it found.

use super::declared;
use super::edit_files::Envelope;
use super::receipt::{self, Inputs};
use super::{Document, Original};
use crate::api::edit::{Edit, EditSource};
use crate::api::error::StetError;
use crate::api::op::Op;
use crate::api::options::OffsetUnit;
use crate::api::receipt::{Receipt, Validation};
use crate::api::stamp::Stamp;
use crate::check;
use crate::ops::{self, Context, ScopeSet};
use crate::projection;
use crate::refs::BaseToken;
use crate::revisions::stamp::RevStamp;
use crate::state::State;

/// True for an operation that names an author, whose codes `document` has to resolve.
fn by_author(op: &Op) -> bool {
    matches!(op, Op::Accept(r) | Op::Reject(r) if r.author.is_some())
}

impl Document {
    /// Applies an edit atomically, tracked unless the edit says otherwise. A dry run fails exactly when the real edit would,
    /// and its receipt is the real one apart from `dry_run`, the base token (the current one) and the refs of new blocks, which
    /// a dry run never issues.
    pub fn edit(&mut self, edit: Edit) -> Result<Receipt, StetError> {
        let (receipt, next) = self.run_edit(&edit)?;
        let Some(next) = next else {
            return Ok(receipt);
        };
        // The original is what the document was opened from, and the first edit is the last chance to say so. Law L1 makes the
        // bytes `open` was given what an unedited document writes, and the state holds them shared, so keeping them is no copy.
        if matches!(self.original, Original::Opened) && next.base_token().version() != 0 {
            self.original = Original::Kept(self.state.opened_source());
        }
        self.state = next;
        Ok(receipt)
    }

    /// Everything an edit does short of installing its state: the receipt, and the next state of an edit that is not a dry run. It
    /// reads the document and changes nothing in it, so `check_pending` can run a dry run through a shared reference.
    pub(super) fn run_edit(&self, edit: &Edit) -> Result<(Receipt, Option<State>), StetError> {
        self.check_original()?;
        // 1. The envelope.
        let stamp = RevStamp::new(&edit.stamp.author, &edit.stamp.date)?;
        let envelope = match &edit.source {
            EditSource::Ops(batch) => {
                ops::validate_batch(batch, &self.limits)?;
                Envelope::of_batch(batch)
            }
            EditSource::Files(files) => self.plan_files(files)?,
        };
        let batch = &*envelope.batch;
        let base = BaseToken::parse(&batch.base)?;
        let authors = if batch.ops.iter().any(by_author) {
            projection::authors(self.state.parts())?
        } else {
            Vec::new()
        };
        let notes = if batch.ops.iter().any(|o| matches!(o, Op::DeleteNote(_))) {
            projection::note_ids(self.state.parts())?
        } else {
            Vec::new()
        };
        // 4. Stage.
        let mut tx = self.state.begin();
        let scope = if edit.scope.is_empty() {
            None
        } else {
            Some(ScopeSet::resolve(&tx, &base, &edit.scope, &self.limits)?)
        };
        // 5. Apply.
        let cx = Context {
            stamp: &stamp,
            tracked: edit.tracked && !edit.markup.is_manual(),
            markup: edit.markup,
            quotes: edit.quotes,
            // An edit of files is planned in scalar values, and a UTF-16 count that the caller set is for batches (H11).
            unit: match edit.source {
                EditSource::Ops(_) => edit.unit,
                EditSource::Files(_) => OffsetUnit::Scalar,
            },
            authors: &authors,
            scope: scope.as_ref(),
            origins: &envelope.origins,
            notes: &notes,
        };
        let applied = ops::apply(batch, &mut tx, &cx).map_err(|e| envelope.locate(e))?;
        // Every property container the operations copied is written in the order Word writes, so a copy never repeats a
        // defect of the order of its source.
        crate::revisions::canonical::order_created(&mut tx)?;
        #[cfg(test)]
        super::tests_tamper::tamper(&mut tx);
        // 6. The scope guard, first pass: what the operations declared.
        if let Some(scope) = &scope {
            ops::scope_guard(&tx, scope, &applied).map_err(|e| envelope.locate(e))?;
        }
        // 7. Check: no unit that differs from the base may have a violation that its origin did not have. Where it refuses, the
        // block that holds the first violation says which operation made it.
        let declared = declared::build(self.state.parts(), tx.parts(), &applied.origins)?;
        let verified = check::edit_located(&tx, &declared).map_err(|refusal| {
            let check::Refusal { error, block } = *refusal;
            let at = block.and_then(|b| applied.op_touching(&b));
            envelope.locate(at.map_or(error.clone(), |i| error.at_op(i)))
        })?;
        // The second pass: what the trees say changed, which no declaration can hide.
        if let Some(scope) = &scope {
            ops::scope_guard_changed(&tx, scope, &applied, &verified.changed)
                .map_err(|e| envelope.locate(e))?;
        }
        // A write repairs the prefixes that the input names and never declares, which Word refuses (decision 0014).
        let repairs = super::repair::repair(&mut tx)?;
        let names = receipt::capture_names(tx.parts(), &applied);
        let shown = Stamp::new(stamp.author(), stamp.date());
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        let validation = Validation {
            parts: count(verified.checked.parts),
            units: count(verified.checked.validated),
            carried: verified.carried,
            carried_total: count(verified.carried_total),
            notes: verified.notes,
        };
        let inputs = |parts, base, dry_run| Inputs {
            parts,
            base,
            dry_run,
            tracked: edit.tracked && !edit.markup.is_manual(),
            markup: edit.markup,
            stamp: shown.clone(),
            applied: &applied,
            format: &self.format,
            notes: &envelope.notes,
            repairs: &repairs,
            origins: &envelope.origins,
            names: &names,
            validation: &validation,
        };
        // 8 to 11. The receipt and the commit, in the order that suits each kind. A dry run commits into a state it drops, so that
        // every failure the real edit would meet at the commit is met here too.
        if edit.dry_run {
            let mut receipt = receipt::build(&inputs(tx.parts(), self.state.base_token(), true))?;
            let (_dropped, committed) = tx.commit()?;
            receipt::complete(&mut receipt, &committed);
            return Ok((receipt, None));
        }
        let (next, committed) = tx.commit()?;
        let mut receipt = receipt::build(&inputs(next.parts(), next.base_token(), false))?;
        receipt::complete(&mut receipt, &committed);
        Ok((receipt, Some(next)))
    }
}
