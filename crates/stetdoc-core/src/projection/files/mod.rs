//! Readable files into operations: the alignment of an edited draft with its baseline (design 6).
//!
//! `align` takes the baseline draft (what the unedited files say) and the edited draft (what a format's
//! parse made of the agent's files) and returns the operations that the difference comes to, one origin
//! for each, and notes. The steps run in this order, and each can end the plan: refs, order, structure,
//! splits and merges, matched paragraphs, removed blocks, new lines (`align.rs`). Everything the agent
//! wrote is either applied exactly as written or refused with the file, the line and the way out. There
//! is no fuzzy matching anywhere: two lines are the same paragraph when they carry the same ref.
//!
//! The files are independent of any syntax, because the drafts are. `to_batch` (the one entry point
//! `document` calls, which renders the baseline from the state and parses the edited files with the
//! format they were written in) wraps `align` in step 7 of the plan.

mod align;
mod anchor;
mod batch;
mod checks;
mod matched;
mod plan;
mod recognize;
mod refuse;
mod split;
mod style;

#[cfg(test)]
pub(crate) mod testing;
#[cfg(test)]
mod tests_align;
#[cfg(test)]
mod tests_anchor;
#[cfg(test)]
mod tests_batch;
#[cfg(test)]
mod tests_laws;
#[cfg(test)]
mod tests_merge;
#[cfg(test)]
mod tests_refusals;
#[cfg(test)]
mod tests_split;
#[cfg(test)]
mod tests_style;

pub(crate) use align::align;
pub(crate) use batch::to_batch;
#[cfg(test)]
pub(crate) use batch::to_batch_in;

use super::draft::Draft;
use super::formats::Wrap;
use crate::api::files::{Origin, PlannedOp};
use crate::api::view::Warning;
use crate::refs::BlockRef;

/// The draft that the unedited files must parse to, and where the renderer wrapped long paragraphs.
pub(crate) struct Baseline {
    /// What the unedited files say.
    pub draft: Draft,
    /// Where a long paragraph was cut, so that a split at that place can be warned about.
    pub wraps: Vec<Wrap>,
}

/// What became of a ref that no file of the baseline holds.
pub(crate) enum Fate {
    /// It never existed: `not_found`.
    Unknown {
        /// What the refs of the files are, for the message.
        range: String,
    },
    /// It was deleted or merged away: `stale_ref`.
    Gone {
        /// What became of it, from the ref table.
        what: String,
    },
}

/// A paragraph style, by the name the files show and the id an operation takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StyleRef {
    /// The style's name.
    pub name: String,
    /// The style's `w:styleId`.
    pub id: String,
}

/// What `align` needs to know that the drafts do not say.
pub(crate) trait Outside {
    /// What became of a ref that is in no file of the baseline.
    fn fate(&self, r: BlockRef) -> Fate;

    /// The style that Enter gives after a paragraph in this style: its `w:next` when that names another
    /// paragraph style that exists. `None` when the next style is the style itself, or there is none.
    fn next_style(&self, name: &str) -> Option<StyleRef>;
}

/// What an edit of the files comes to.
#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// The operations, in the order they must be applied.
    pub ops: Vec<PlannedOp>,
    /// One origin for each operation.
    pub origins: Vec<Origin>,
    /// Warnings that are not errors.
    pub notes: Vec<Warning>,
}
