//! What a call did, and what a transaction wrote: `Effect`, `Outcome` and the small records in them
//! (design `revisions.md`, 2.1).
//!
//! The totals of a `Compiler` are an `Outcome`. `created` is not a list the compiler keeps: `finish`
//! derives it from the ids the `Tx` minted (`verify.rs`).

use crate::wordml::BlockKey;
use crate::wordml::{NodeId, RevKind};
use std::ops::Range;
use std::sync::Arc;

/// What kind of own pending content a call removed outright.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RemovedKind {
    /// Text of the author's own pending insertion.
    Text,
    /// The author's own pending inserted paragraph mark.
    Mark,
    /// A paragraph that the author inserted and that was removed whole.
    Paragraph,
    /// A row that the author inserted and that was removed whole.
    Row,
}

/// Own pending content that a call removed outright.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemovedOwn {
    /// The `w:id` of the own revision it was removed from, if it has one.
    pub id: Option<i64>,
    /// What was removed.
    pub kind: RemovedKind,
}

/// An own pending change that an edit went into instead of a new revision: the id of the `*PrChange`,
/// or the id of the inserted mark of a paragraph that was edited directly. `cancelled`: the edit
/// brought the properties back to the snapshot, so the change element was removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Folded {
    /// The id of the change, or of the inserted mark.
    pub id: i64,
    /// True if the change element was removed.
    pub cancelled: bool,
}

/// Where a new or surviving block came from: `from` are the blocks whose state before the call it was
/// copied or built from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Origin {
    /// A new element, or the element that survives a merge.
    pub block: BlockKey,
    /// The blocks it was built from.
    pub from: Vec<BlockKey>,
}

/// What one call did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Effect {
    /// The annotation ids this call took from the transaction's counter.
    pub minted: Range<u32>,
    /// `(old id, new id)`: a pending revision cut in two.
    pub split: Vec<(i64, i64)>,
    /// Own pending text, marks or rows removed outright.
    pub removed_own: Vec<RemovedOwn>,
    /// Property edits folded into the author's pending change.
    pub folded: Vec<Folded>,
    /// Ids of revisions that went with an element removed outright.
    pub swallowed: Vec<i64>,
    /// Elements created. Their refs are minted at commit.
    pub new_blocks: Vec<BlockKey>,
    /// Where each new or surviving block came from.
    pub origins: Vec<Origin>,
    /// For a merge, the element that remains.
    pub survivor: Option<BlockKey>,
    /// The blocks whose own bytes this call changed, whether or not an earlier call of the transaction
    /// had changed them too: the transaction's list of touched blocks has each once.
    pub touched: Vec<BlockKey>,
    /// Compare only: each block that received content from another package, with the block in that package
    /// it came from. It lists the blocks placed whole and the paragraphs that received fragments, so that `check`
    /// is told the origin of each (`Declared::foreign`).
    pub imported: Vec<(BlockKey, BlockKey)>,
    /// Compare only: each definition (a style, a numbering definition) and each part that the import added, with what it was
    /// copied from, so that `check` keys it as foreign.
    pub imported_defs: Vec<super::redline_types::ImportedDef>,
}

impl Effect {
    /// True if the call changed nothing that this record can say.
    pub(crate) fn is_empty(&self) -> bool {
        self.minted.is_empty()
            && self.split.is_empty()
            && self.removed_own.is_empty()
            && self.folded.is_empty()
            && self.swallowed.is_empty()
            && self.new_blocks.is_empty()
            && self.origins.is_empty()
            && self.survivor.is_none()
            && self.touched.is_empty()
            && self.imported.is_empty()
            && self.imported_defs.is_empty()
    }
}

/// A revision element that a transaction wrote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Created {
    /// What it records.
    pub kind: RevKind,
    /// Its `w:id`.
    pub id: i64,
    /// The part it is in.
    pub part: Arc<str>,
    /// The element.
    pub node: NodeId,
}

/// What a `Compiler` did, as `finish` reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Outcome {
    /// Every annotation id the transaction minted.
    pub minted: Range<u32>,
    /// The effect of each call, in order.
    pub effects: Vec<Effect>,
    /// The revision elements the transaction created, in part and document order, in the blocks it touched
    /// and the blocks this compiler made. A transaction with several compilers has the blocks that the
    /// earlier ones made in their outcomes, so the receipt of a batch takes the union of the lists.
    pub created: Vec<Created>,
    /// Things worth telling the agent that are not errors.
    pub warnings: Vec<String>,
}

impl Outcome {
    /// The origins that the calls declared, in order, which `document` converts into `check`'s `Declared`.
    /// Each `from` is made of blocks that were in the document before the compiler began: a block that an
    /// earlier call made is replaced by the blocks it was made from (a tail that a split made and a merge
    /// joined back is the paragraph it was cut from), and a block that was made from nothing is left out, since
    /// it has no keys to give. A block that an earlier compiler of the same transaction made is not in this
    /// outcome, and `document` follows it through that compiler's origins in the same way.
    pub(crate) fn origins(&self) -> Vec<Origin> {
        let mut made: std::collections::HashMap<&BlockKey, Vec<BlockKey>> =
            std::collections::HashMap::new();
        let mut out: Vec<Origin> = Vec::new();
        for effect in &self.effects {
            for new in &effect.new_blocks {
                made.entry(new).or_default();
            }
            for origin in &effect.origins {
                let mut from: Vec<BlockKey> = Vec::new();
                for key in &origin.from {
                    match made.get(key) {
                        Some(base) => from.extend(base.iter().cloned()),
                        None => from.push(key.clone()),
                    }
                }
                let mut seen = std::collections::HashSet::new();
                from.retain(|k| seen.insert(k.clone()));
                if let Some(slot) = made.get_mut(&origin.block) {
                    slot.clone_from(&from);
                }
                out.push(Origin {
                    block: origin.block.clone(),
                    from,
                });
            }
        }
        out
    }

    /// The own pending content that the calls removed outright, in order, with the id of the revision it was
    /// removed from: structured, for a receipt, and not only a warning.
    pub(crate) fn removed_own(&self) -> Vec<RemovedOwn> {
        self.effects
            .iter()
            .flat_map(|e| e.removed_own.iter().cloned())
            .collect()
    }

    /// The edits that the calls folded into the author's own pending change, in order.
    pub(crate) fn folded(&self) -> Vec<Folded> {
        self.effects
            .iter()
            .flat_map(|e| e.folded.iter().cloned())
            .collect()
    }

    /// The ids of the revisions of other authors that went with an element the calls removed, in order, each
    /// once.
    pub(crate) fn swallowed(&self) -> Vec<i64> {
        let mut seen = std::collections::HashSet::new();
        self.effects
            .iter()
            .flat_map(|e| e.swallowed.iter().copied())
            .filter(|id| seen.insert(*id))
            .collect()
    }

    /// Every `(old, new)` pair of revision ids that a call split.
    pub(crate) fn splits(&self) -> Vec<(i64, i64)> {
        self.effects
            .iter()
            .flat_map(|e| e.split.iter().copied())
            .collect()
    }
}
