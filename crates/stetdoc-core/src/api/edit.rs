//! The request that `Document::edit` takes.

use super::batch::Batch;
use super::files::ReadableFiles;
use super::options::{OffsetUnit, Quotes};
use super::stamp::Stamp;

/// Where the operations of an edit come from.
#[derive(Clone, Debug)]
pub(crate) enum EditSource {
    /// A batch of operations.
    Ops(Batch),
    /// Readable files that an agent edited by hand, which `projection::files` turns into operations.
    Files(ReadableFiles),
}

/// One edit: operations or hand-edited files, with the stamp that signs it and the options that shape it.
///
/// It is **tracked by default**: every change to text, formatting or structure becomes a Word revision with the stamp's
/// author and date. `untracked` is the explicit exception.
#[derive(Clone, Debug)]
pub struct Edit {
    pub(crate) source: EditSource,
    pub(crate) stamp: Stamp,
    pub(crate) tracked: bool,
    pub(crate) scope: Vec<String>,
    pub(crate) dry_run: bool,
    pub(crate) quotes: Quotes,
    pub(crate) unit: OffsetUnit,
}

impl Edit {
    fn new(source: EditSource, stamp: Stamp) -> Edit {
        Edit {
            source,
            stamp,
            tracked: true,
            scope: Vec::new(),
            dry_run: false,
            quotes: Quotes::default(),
            unit: OffsetUnit::default(),
        }
    }

    /// An edit of operations, signed by `stamp`.
    pub fn ops(batch: Batch, stamp: Stamp) -> Edit {
        Edit::new(EditSource::Ops(batch), stamp)
    }

    /// An edit of readable files that an agent changed by hand, signed by `stamp`. The files name their own base token.
    pub fn files(files: ReadableFiles, stamp: Stamp) -> Edit {
        Edit::new(EditSource::Files(files), stamp)
    }

    /// Writes no revisions: the changes are applied directly. For documents that are not under review.
    #[must_use]
    pub fn untracked(mut self) -> Edit {
        self.tracked = false;
        self
    }

    /// Limits the edit to these blocks: refs, and ranges such as `p40..p60`. A change anywhere else rejects the whole batch
    /// with `scope_violation`.
    #[must_use]
    pub fn scope(mut self, refs: Vec<String>) -> Edit {
        self.scope = refs;
        self
    }

    /// Runs the whole edit and drops the result. It fails exactly when the real edit would, and its receipt differs from the
    /// real one only in `dry_run`, the base token (the current one) and the refs of new blocks (a dry run issues none).
    #[must_use]
    pub fn dry_run(mut self) -> Edit {
        self.dry_run = true;
        self
    }

    /// What to do with the quotes of typed text. The default follows the paragraph.
    #[must_use]
    pub fn quotes(mut self, quotes: Quotes) -> Edit {
        self.quotes = quotes;
        self
    }

    /// The unit that the offsets of an operation batch count. The default is Unicode scalar values. It applies to `Edit::ops`
    /// and not to `Edit::files`, whose operations carry scalar offsets.
    #[must_use]
    pub fn offsets(mut self, unit: OffsetUnit) -> Edit {
        self.unit = unit;
        self
    }
}
