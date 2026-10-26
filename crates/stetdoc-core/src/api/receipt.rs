//! The receipt of an edit, and the result of `check_pending`.
//!
//! A receipt shows what an edit did: one result for each operation, the revisions it wrote by author, resolutions, comment
//! effects, the refs that went stale, the post-edit text of every block it touched or added, warnings, and what `check`
//! verified. Its `text` is the form an agent reads, and the structure is the `--json` form. A **dry run** has the same receipt
//! as the real edit, except that `dry_run` is true, `base` is the document's current token and a new block has no ref: it
//! is shown as new, and `added_count` says how many there are.

use super::check::{CheckReport, Finding};
use super::files::{Origin, RenderedBlock};
use super::revision::{ChangeKind, RevisionInfo};
use super::stamp::Stamp;
use super::view::Warning;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One revision, or one change, that an edit wrote.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChangeEntry {
    /// What the change is.
    pub kind: ChangeKind,
    /// Every revision id of the change.
    pub ids: Vec<i64>,
    /// The date as written into the document.
    pub date: String,
    /// The ref of the block, or none for a block that a dry run shows as new.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
}

/// The changes of one author, with a count of each kind.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthorRevisions {
    /// The author, as the document writes it. A change with no author is `Unknown`.
    pub author: String,
    /// How many changes of each kind, in the order of the kinds.
    pub counts: Vec<(ChangeKind, u32)>,
    /// The changes, in the order given.
    pub changes: Vec<ChangeEntry>,
}

impl AuthorRevisions {
    /// Groups a list of changes by author, in the order the authors first appear.
    pub fn group(infos: &[RevisionInfo]) -> Vec<AuthorRevisions> {
        let mut authors: Vec<String> = Vec::new();
        let mut counts: Vec<BTreeMap<ChangeKind, u32>> = Vec::new();
        let mut changes: Vec<Vec<ChangeEntry>> = Vec::new();
        for info in infos {
            let author = info.author.clone().unwrap_or_else(|| "Unknown".to_string());
            let at = match authors.iter().position(|a| *a == author) {
                Some(at) => at,
                None => {
                    authors.push(author);
                    counts.push(BTreeMap::new());
                    changes.push(Vec::new());
                    authors.len() - 1
                }
            };
            if let Some(map) = counts.get_mut(at) {
                *map.entry(info.kind).or_insert(0) += 1;
            }
            if let Some(list) = changes.get_mut(at) {
                list.push(ChangeEntry {
                    kind: info.kind,
                    ids: info.ids.clone(),
                    date: info.date.clone().unwrap_or_default(),
                    block: info.block.clone(),
                });
            }
        }
        authors
            .into_iter()
            .zip(counts)
            .zip(changes)
            .map(|((author, counts), changes)| AuthorRevisions {
                author,
                counts: counts.into_iter().collect(),
                changes,
            })
            .collect()
    }
}

/// Own pending content that an operation removed outright: the author's earlier insertion, say.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RemovedOwnEntry {
    /// What was removed: `text`, `mark`, `paragraph` or `row`.
    pub kind: String,
    /// The id of the revision it was, when it had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
}

/// An own pending property change that an operation edited in place instead of writing a new one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FoldedEntry {
    /// The id of the change.
    pub id: i64,
    /// True when the edit brought the properties back to what they were and the change element was removed.
    pub cancelled: bool,
}

/// A paragraph that compare wrote as a deletion and an insertion, and lists as a move.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MoveEntry {
    /// The ref where it was.
    pub from: String,
    /// The ref where it went.
    pub to: String,
    /// The start of its text.
    pub text: String,
}

/// A ref that is stale now, and why.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Retired {
    /// The ref.
    pub r#ref: String,
    /// `deleted`, `rejected` or `merged into p5`.
    pub reason: String,
}

/// How many revisions of one kind an `accept` or `reject` resolved, for one author.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedCount {
    /// The author.
    pub author: String,
    /// `insertion`, `deletion`, `move` or `format`.
    pub kind: String,
    /// How many revision elements.
    pub count: u32,
}

/// What an `accept` or `reject` did.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Resolution {
    /// `accept` or `reject`.
    pub action: String,
    /// What was resolved, by author and kind.
    pub resolved: Vec<ResolvedCount>,
    /// The ids of revisions that went with an element that was removed.
    pub swallowed: Vec<i64>,
    /// The refs that went stale.
    pub removed: Vec<Retired>,
    /// Paragraphs that were joined: the one that stays and the one that went.
    pub joined: Vec<(String, String)>,
    /// The notes that were removed because their last reference went, such as `fn3`.
    pub notes_removed: Vec<String>,
    /// Things worth saying that are not errors.
    pub warnings: Vec<Warning>,
}

/// What a comment operation did, or what `reconcile` removed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CommentEffect {
    /// `added`, `replied`, `resolved`, `reopened`, `edited`, `deleted` or `removed` (by `reconcile`).
    pub action: String,
    /// The comment, such as `c12`. A new comment is listed so that the next operation can name it.
    pub id: String,
    /// The root of its thread.
    pub thread: String,
    /// The comment's author.
    pub author: String,
    /// The paragraphs that hold its markers.
    pub blocks: Vec<String>,
    /// Things worth saying that are not errors.
    pub warnings: Vec<Warning>,
}

/// What `check` verified for an edit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Validation {
    /// How many parts were examined.
    pub parts: u32,
    /// How many units were validated.
    pub units: u32,
    /// Up to ten problems that were already in the blocks the edit touched, so a later `check` will still show them.
    pub carried: Vec<Finding>,
    /// How many such problems there are in all.
    pub carried_total: u32,
    /// Notes that are not problems.
    pub notes: Vec<Finding>,
}

/// The result of one operation of a batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpResult {
    /// The zero-based index of the operation.
    pub index: usize,
    /// Its name.
    pub op: String,
    /// False when it changed nothing.
    pub changed: bool,
    /// For an edit of files, the file and line it came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    /// The refs of the existing blocks it changed.
    pub blocks: Vec<String>,
    /// The refs minted for the blocks it added. Empty for a dry run, which issues no ref. A block it added that a later
    /// operation joined an older block into carries the older block's ref, which is not listed.
    pub added: Vec<String>,
    /// How many blocks it added: the same for a dry run as for the real edit.
    pub added_count: u32,
    /// The changes it wrote.
    pub revisions: Vec<ChangeEntry>,
    /// The author's own pending text, marks or rows that it removed outright.
    pub removed_own: Vec<RemovedOwnEntry>,
    /// The author's own pending property changes that it edited in place.
    pub folded: Vec<FoldedEntry>,
    /// The ids of other revisions that went with removed text.
    pub swallowed: Vec<i64>,
    /// For `accept` and `reject`, what was resolved. It is shown on the first operation of a run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<Resolution>,
    /// For a comment operation, the comment, such as `c12`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// Things worth saying about this operation.
    pub warnings: Vec<Warning>,
}

/// The receipt of an edit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Receipt {
    /// The base token after the edit. For a dry run, and for a batch that changed nothing, the current one.
    pub base: String,
    /// True for a dry run, which changed nothing.
    pub dry_run: bool,
    /// False for an untracked edit.
    pub tracked: bool,
    /// The author, and the date as written into the document.
    pub stamp: Stamp,
    /// One result for each operation, in order.
    pub operations: Vec<OpResult>,
    /// The revisions the batch wrote, by author.
    pub revisions: Vec<AuthorRevisions>,
    /// What the comment operations did, and what `reconcile` removed.
    pub comments: Vec<CommentEffect>,
    /// The refs that are stale now, with the reason.
    pub retired: Vec<Retired>,
    /// The refs of the blocks the batch added. Empty for a dry run.
    pub added: Vec<String>,
    /// How many blocks the batch added.
    pub added_count: u32,
    /// Moved paragraphs, which only compare lists. Empty for an edit.
    pub moves: Vec<MoveEntry>,
    /// The post-edit text of every block touched or added, at most 200.
    pub blocks: Vec<RenderedBlock>,
    /// How many more blocks there were.
    pub blocks_omitted: u32,
    /// Every warning, deduplicated by code, block and message.
    pub warnings: Vec<Warning>,
    /// What `check` verified.
    pub validation: Validation,
    /// What a surface shows an agent.
    pub text: String,
}

/// The result of `Document::check_pending`: everything `stetdoc check` prints, and its exit status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CheckResult {
    /// The dry run of the pending files, which shows the revisions they would write. None when none are pending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<Receipt>,
    /// The revisions already in the document, by author.
    pub existing: Vec<AuthorRevisions>,
    /// What `Document::check` found in the document as it is.
    pub report: CheckReport,
    /// Whether any violation, new or old, fails the check.
    pub strict: bool,
    /// The exit status: 0, or 5 when `strict` and the report has a violation. A pending edit that would add a violation is the
    /// error `validation_failed`, whose exit class is 5.
    pub exit_status: u8,
}
