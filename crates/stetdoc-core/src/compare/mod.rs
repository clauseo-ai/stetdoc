//! Original and revised into a redline state.
//!
//! **Owns.** Block alignment, the diff into revisions, and `import`, which owns cross-package transplants (relationships, styles,
//! numbering, notes, bookmarks, drawing ids).
//!
//! **Interface.** `compare_states(original, revised, &Limits, &RevStamp) -> (State, CompareReceipt)`, and under it `prepare`
//! (open both, accept every revision), `build::redline` (the plan written as calls of the compiler, returning the transaction still
//! open so that `document` can run `check` on it and commit) and the `CompareReceipt` (moves, warnings, exclusions, what `check`
//! must be told). The public `compare` is defined in `document`.
//!
//! **Invariants.**
//! - The redline is built on a State from the *revised* package, after every revision of both inputs was accepted.
//! - Accepting all revisions in the redline gives the revised document, and rejecting all gives the original (the compare laws N1 and N2), except for
//!   the blocks that the receipt lists as exclusions with their reasons.
//! - Clause numbers are excluded from comparison text, and alignment uses unique-hash anchors first, then similarity.
//! - A paragraph that the compiler refuses is replaced whole; one that cannot be written at all is left as the revised document has
//!   it and listed: one paragraph never fails a comparison. A call that fails after it has written something is a defect, and
//!   `build::redline` fails with `internal` rather than return a transaction it cannot vouch for.
//! - The import writes only through `Compiler::import_writer` (detached nodes in a story, definitions and parts elsewhere), and
//!   the compiler places what it made (rule 2 of `ARCHITECTURE.md`).
//!
//! **Depends on.** `api`, `package`, `xml`, `schema`, `wordml`, `styles`, `numbering`, `state`, `revisions`.
//!
//! **Start reading.** `compare/mod.rs`, then `compare/build.rs`. The design is [`docs/design/compare.md`](../../../../docs/design/compare.md).

#![expect(
    dead_code,
    reason = "`compare_states` and some fields of the receipt serve the module's own tests, and `document` calls `prepare` and `redline`"
)]

mod align;
mod align_gap;
mod build;
mod build_checks;
mod build_edit;
mod build_exclusions;
mod build_notes;
mod build_tables;
mod edits;
mod fmt;
mod import;
mod model;
mod plan;
mod plan_fields;
mod plan_para;
mod prepare;
mod receipt;
mod regroup;
mod similarity;
mod stories;
mod tables;
mod text;

use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::revisions::stamp::RevStamp;
use crate::state::State;
pub(crate) use build::{Redline, redline};
#[cfg(test)]
pub(crate) use import::failpoint;
pub(crate) use prepare::{Prepared, prepare};
pub(crate) use receipt::{CompareReceipt, Exclusion, Site};

/// Compares two packages: the redline, as a state on the revised package, and the receipt. `document` runs `check` on the
/// transaction between the build and the commit (design 2.2), and this is the whole of it without that step.
pub(crate) fn compare_states(
    original: Vec<u8>,
    revised: Vec<u8>,
    limits: &Limits,
    stamp: &RevStamp,
) -> Result<(State, CompareReceipt), StetError> {
    let prepared = prepare::prepare(original, revised, limits, stamp)?;
    let build::Redline { tx, receipt } = build::redline(&prepared, stamp)?;
    let (state, _) = tx.commit()?;
    Ok((state, receipt))
}

#[cfg(test)]
mod tests_align;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_dump;
#[cfg(test)]
mod tests_edits;
#[cfg(test)]
mod tests_generated;
#[cfg(test)]
mod tests_import;
#[cfg(test)]
mod tests_laws;
#[cfg(test)]
mod tests_pairing;
#[cfg(test)]
mod tests_pairs;
#[cfg(test)]
mod tests_pkg;
#[cfg(test)]
mod tests_plan;
#[cfg(test)]
mod tests_shapes;
#[cfg(test)]
mod tests_word_shapes;
