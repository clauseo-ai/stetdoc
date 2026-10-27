//! What `apply` takes and what it gives back (design `document-and-ops.md` 3.1.1).
//!
//! `Context` is the read-only half of an edit: the stamp, whether it is tracked, the quote policy, the offset unit, the author
//! codes and the scope. `Applied` is what the batch did, kept with the operation that did it, which is how the receipt, the
//! scope guard and an error that names an operation find the operation of a block.

use super::scope::ScopeSet;
use crate::api::files::Origin as FileOrigin;
use crate::api::options::{Markup, OffsetUnit, Quotes};
use crate::api::view::Warning;
use crate::comments::CommentEffect;
use crate::revisions::outcome::{Effect, Origin, Outcome};
use crate::revisions::resolve::Resolution;
use crate::revisions::stamp::RevStamp;
use crate::wordml::BlockKey;
use std::collections::HashSet;

/// What one operation hands back to the loop: the effects of its calls, the blocks it named, and what to say about it.
#[derive(Default)]
pub(super) struct Done {
    /// The effect of each compiler call, in order.
    pub effects: Vec<Effect>,
    /// The blocks the operation named.
    pub targets: Vec<BlockKey>,
    /// The blocks it changes because of its target.
    pub owned: Vec<BlockKey>,
    /// Things worth saying.
    pub warnings: Vec<Warning>,
    /// Parts it changes outside any block.
    pub side_effects: Vec<SideEffect>,
    /// What a comment operation did to comments.
    pub comments: Vec<CommentEffect>,
    /// With manual markup, what it struck through and underlined, in the order of the text.
    pub marked: Vec<Marked>,
    /// True when the operation changed the document through calls whose effects it does not hold: a comment written, a note
    /// deleted.
    pub changed: bool,
}

impl Done {
    /// A result that names these blocks.
    pub(super) fn naming(keys: &[&BlockKey]) -> Done {
        Done {
            targets: keys.iter().map(|k| (*k).clone()).collect(),
            ..Done::default()
        }
    }
}

/// One change of manual markup: the paragraph, the old text struck through and the new text underlined after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Marked {
    /// The paragraph.
    pub block: BlockKey,
    /// The old text, kept and struck through.
    pub struck: String,
    /// The new text, underlined.
    pub underlined: String,
}

/// What an edit asks of `apply`, besides the batch.
pub(crate) struct Context<'a> {
    /// The author and the date. Tracked edits write them into every revision.
    pub stamp: &'a RevStamp,
    /// False for an untracked edit: the same calls, on a compiler that writes no revision.
    pub tracked: bool,
    /// How text changes show. Manual markup comes with `tracked` false.
    pub markup: Markup,
    /// What to do with the quotes of typed text.
    pub quotes: Quotes,
    /// The unit that the offsets of the batch count.
    pub unit: OffsetUnit,
    /// `(code, name)` of each revision author, for `accept` and `reject` by author.
    pub authors: &'a [(String, String)],
    /// The blocks the edit may change, if it has a scope.
    pub scope: Option<&'a ScopeSet>,
    /// For an edit of readable files, where each operation came from, so that a message can name a file and a line and not an
    /// operation number the agent never wrote. Empty for a batch.
    pub origins: &'a [FileOrigin],
    /// `(endnote, ordinal, w:id)` of each note the body refers to, for `delete-note`, which names a note by its ordinal as the
    /// view shows it. `document` reads them from `projection` when the batch has a `delete-note`.
    pub notes: &'a [(bool, u32, i64)],
}

/// A part that an operation changes outside any block, and why. The scope guard accepts the change of a part that an operation
/// declared and refuses every other part that no block covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SideEffect {
    /// The part's name.
    pub part: String,
    /// What was written, for the message that refuses it.
    pub why: String,
    /// For `styles.xml`, the ids of the styles the operation appended: the part may differ from the base only by those `w:style`
    /// elements. `None` declares the whole part.
    pub styles: Option<Vec<String>>,
}

/// What one operation did.
#[derive(Clone, Debug, Default)]
pub(crate) struct OpApplied {
    /// The name of the operation, as the `op` field spells it.
    pub name: &'static str,
    /// Who would be said to change a block, for the message that refuses it: `Operation 1 (replace p57) would change it.`, or
    /// `The edit at document.md line 57 would change it.` for an edit of files.
    pub by: String,
    /// The effect of each compiler call the operation made, in order.
    pub effects: Vec<Effect>,
    /// The blocks the operation named, which the scope guard checks beside the blocks that were touched: a paragraph inserted
    /// before another touches nothing in the anchor, so the touch list alone would miss it.
    pub targets: Vec<BlockKey>,
    /// Blocks the operation changes because of its target and no scope can name: not checked.
    pub owned: Vec<BlockKey>,
    /// False when it changed nothing.
    pub changed: bool,
    /// Things worth saying about this operation.
    pub warnings: Vec<Warning>,
    /// For the first operation of a run of `accept` and `reject`, the index into `Applied::resolutions`.
    pub resolution: Option<usize>,
    /// With manual markup, what it struck through and underlined.
    pub marked: Vec<Marked>,
}

/// What a whole batch did.
#[derive(Debug, Default)]
pub(crate) struct Applied {
    /// One entry for each operation, in order.
    pub ops: Vec<OpApplied>,
    /// The parts the operations declared they change outside any block.
    pub side_effects: Vec<SideEffect>,
    /// One for each compiler. A batch with accepts and rejects between its edits has several.
    pub outcomes: Vec<Outcome>,
    /// The resolutions, by the index of the first operation of the run that made them.
    pub resolutions: Vec<(usize, Resolution)>,
    /// What the compiler and the resolver declared about where blocks came from, in order, for `check`.
    pub origins: Vec<Origin>,
    /// How many of `Tx::touched()` came from the operations themselves.
    pub touched_by_ops: usize,
    /// Warnings of the batch that belong to no one operation, by operation when they do.
    pub warnings: Vec<(Option<usize>, Warning)>,
    /// What the comment operations did, by operation, and what `reconcile` removed (`None`).
    pub comments: Vec<(Option<usize>, CommentEffect)>,
}

impl Applied {
    /// The first operation whose effects touched or created the block, or that named it.
    pub(crate) fn op_touching(&self, block: &BlockKey) -> Option<usize> {
        self.ops.iter().position(|op| {
            op.targets.contains(block)
                || op.effects.iter().any(|e| {
                    e.touched.contains(block)
                        || e.new_blocks.contains(block)
                        || e.survivor.as_ref() == Some(block)
                })
        })
    }

    /// Every block that an operation created, in order.
    pub(crate) fn new_blocks(&self) -> Vec<BlockKey> {
        let mut out: Vec<BlockKey> = Vec::new();
        let mut seen: HashSet<&BlockKey> = HashSet::new();
        for op in &self.ops {
            for effect in &op.effects {
                for key in &effect.new_blocks {
                    if seen.insert(key) {
                        out.push(key.clone());
                    }
                }
            }
        }
        out
    }
}
