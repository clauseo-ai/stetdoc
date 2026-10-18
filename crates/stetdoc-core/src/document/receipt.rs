//! The receipt of an edit: one builder over a view of the state (design `document-and-ops.md` 4).
//!
//! A real edit builds it from the committed state, where a block the batch made has its ref, and a dry run builds it from the
//! staged state, where it has none, so the two receipts differ in the refs of new blocks and nowhere else. What only the commit
//! knows (the refs retired, the number of blocks added) is added by `complete`, after the commit, for both.

use super::blocks;
use super::changes::{self, element_family};
use super::receipt_text;
use crate::api::error::StetError;
use crate::api::files::Origin;
use crate::api::receipt::{
    AuthorRevisions, CommentEffect as CommentEntry, FoldedEntry, OpResult, Receipt,
    RemovedOwnEntry, Resolution, ResolvedCount, Retired, Validation,
};
use crate::api::stamp::Stamp;
use crate::api::view::{FormatId, Warning, WarningCode};
use crate::ops::Applied;
use crate::refs::{BaseToken, Reason};
use crate::revisions::outcome::RemovedKind;
use crate::revisions::resolve::Resolution as Resolved;
use crate::revisions::types::Action;
use crate::state::{Committed, PartsView};
use crate::wordml::BlockKey;
use std::collections::{HashMap, HashSet};

/// What the builder reads.
pub(super) struct Inputs<'a> {
    /// The state the receipt describes: the next one for an edit, the staged one for a dry run.
    pub parts: PartsView<'a>,
    /// The token the receipt shows: the next state's for an edit, the current one for a dry run.
    pub base: BaseToken,
    /// True for a dry run.
    pub dry_run: bool,
    /// False for an untracked edit.
    pub tracked: bool,
    /// The author, and the date as written.
    pub stamp: Stamp,
    /// What the batch did.
    pub applied: &'a Applied,
    /// The format that writes the blocks.
    pub format: &'a FormatId,
    /// Warnings of the plan of an edit of files.
    pub notes: &'a [Warning],
    /// Where each operation came from, for an edit of files.
    pub origins: &'a [Origin],
    /// The refs, read before the commit, of the existing blocks that the batch touched, removed or joined. A later operation of
    /// the batch can retire a ref (a rejected insertion), and the commit then takes it away, so a receipt that read it afterwards
    /// would list fewer blocks than a dry run, which reads the staged state.
    pub names: &'a HashMap<BlockKey, String>,
    /// What `check` verified, the same for a dry run and for the real edit.
    pub validation: &'a Validation,
}

/// The refs, before the commit, of the existing blocks that the batch touched, and of the blocks that a resolution removed or
/// joined, which no view shows afterwards.
pub(super) fn capture_names(parts: PartsView<'_>, applied: &Applied) -> HashMap<BlockKey, String> {
    let mut out = HashMap::new();
    let mut note = |key: &BlockKey| {
        if let Some(shown) = parts.ref_of(key) {
            out.insert(key.clone(), shown.to_string());
        }
    };
    for (_, resolution) in &applied.resolutions {
        resolution.removed.iter().for_each(|r| note(&r.key));
        resolution.joined.iter().for_each(|(a, b)| {
            note(a);
            note(b);
        });
    }
    for op in &applied.ops {
        op.effects
            .iter()
            .flat_map(|e| e.touched.iter())
            .for_each(&mut note);
    }
    for (_, effect) in &applied.comments {
        effect.blocks.iter().for_each(&mut note);
    }
    out
}

/// What a comment operation did, as the receipt says it: the ids as `c3`, the blocks as refs.
fn comment_entry(
    effect: &crate::comments::CommentEffect,
    names: &HashMap<BlockKey, String>,
) -> CommentEntry {
    use crate::comments::CommentAction as A;
    CommentEntry {
        action: match effect.action {
            A::Added => "added",
            A::Replied => "replied",
            A::Resolved => "resolved",
            A::Reopened => "reopened",
            A::Edited => "edited",
            A::Deleted => "deleted",
            A::Removed => "removed",
        }
        .to_string(),
        id: format!("c{}", effect.id),
        thread: format!("c{}", effect.thread),
        author: effect.author.clone(),
        blocks: effect
            .blocks
            .iter()
            .filter_map(|k| names.get(k).cloned())
            .collect(),
        warnings: effect
            .warnings
            .iter()
            .map(crate::ops::comment_warning)
            .collect(),
    }
}

fn removed_word(kind: RemovedKind) -> &'static str {
    match kind {
        RemovedKind::Text => "text",
        RemovedKind::Mark => "mark",
        RemovedKind::Paragraph => "paragraph",
        RemovedKind::Row => "row",
    }
}

fn warning(code: WarningCode, message: String) -> Warning {
    Warning {
        code,
        block: None,
        story: None,
        message,
        next: None,
    }
}

fn ids_text(ids: &[i64]) -> String {
    ids.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The public form of what a resolution did.
fn resolution(res: &Resolved, names: &HashMap<BlockKey, String>) -> Resolution {
    let shown = |key: &BlockKey| {
        names
            .get(key)
            .cloned()
            .unwrap_or_else(|| "a block".to_string())
    };
    let mut counts: Vec<ResolvedCount> = Vec::new();
    for r in &res.resolved {
        let author = r
            .author
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .unwrap_or("Unknown")
            .to_string();
        let kind = element_family(r.kind).to_string();
        match counts
            .iter_mut()
            .find(|c| c.author == author && c.kind == kind)
        {
            Some(c) => c.count += 1,
            None => counts.push(ResolvedCount {
                author,
                kind,
                count: 1,
            }),
        }
    }
    Resolution {
        action: match res.action {
            Action::Accept => "accept",
            Action::Reject => "reject",
        }
        .to_string(),
        resolved: counts,
        swallowed: res.swallowed.clone(),
        removed: res
            .removed
            .iter()
            .map(|r| Retired {
                r#ref: shown(&r.key),
                reason: if r.rejected { "rejected" } else { "deleted" }.to_string(),
            })
            .collect(),
        joined: res
            .joined
            .iter()
            .map(|(a, b)| (shown(a), shown(b)))
            .collect(),
        notes_removed: res
            .notes
            .iter()
            .map(|n| {
                format!(
                    "{} (id {})",
                    if n.endnote { "endnote" } else { "footnote" },
                    n.id
                )
            })
            .collect(),
        warnings: res
            .warnings
            .iter()
            .map(|m| warning(WarningCode::ReadAnomaly, m.clone()))
            .collect(),
    }
}

/// Builds the receipt of what the batch did, up to what only the commit knows.
pub(super) fn build(i: &Inputs<'_>) -> Result<Receipt, StetError> {
    let parts = i.parts;
    let applied = i.applied;
    let written = changes::written(&parts, applied)?;
    let made: HashSet<BlockKey> = applied.new_blocks().into_iter().collect();
    let mut operations = Vec::with_capacity(applied.ops.len());
    for (index, op) in applied.ops.iter().enumerate() {
        let mut blocks: Vec<String> = Vec::new();
        let mut added: Vec<String> = Vec::new();
        let mut added_count = 0u32;
        let mut removed_own = Vec::new();
        let mut folded = Vec::new();
        let mut swallowed: Vec<i64> = Vec::new();
        for effect in &op.effects {
            for key in &effect.touched {
                if made.contains(key) {
                    continue;
                }
                if let Some(shown) = i.names.get(key)
                    && !blocks.contains(shown)
                {
                    blocks.push(shown.clone());
                }
            }
            for key in &effect.new_blocks {
                if parts.is_attached(key)? {
                    added_count += 1;
                    if let Some(shown) = parts.ref_of(key) {
                        added.push(shown.to_string());
                    }
                }
            }
            removed_own.extend(effect.removed_own.iter().map(|r| RemovedOwnEntry {
                kind: removed_word(r.kind).to_string(),
                id: r.id,
            }));
            folded.extend(effect.folded.iter().map(|f| FoldedEntry {
                id: f.id,
                cancelled: f.cancelled,
            }));
            for id in &effect.swallowed {
                if !swallowed.contains(id) {
                    swallowed.push(*id);
                }
            }
        }
        let mut warnings = op.warnings.clone();
        if !removed_own.is_empty() {
            warnings.push(warning(
                WarningCode::OwnTextRemoved,
                format!(
                    "{} of your own pending {} removed outright, not written as a deletion.",
                    removed_own.len(),
                    if removed_own.len() == 1 {
                        "change was"
                    } else {
                        "changes were"
                    }
                ),
            ));
        }
        if !folded.is_empty() {
            warnings.push(warning(
                WarningCode::PropertyChangeFolded,
                format!(
                    "your own pending property change {} was edited in place.",
                    ids_text(&folded.iter().map(|f| f.id).collect::<Vec<_>>())
                ),
            ));
        }
        if !swallowed.is_empty() {
            warnings.push(warning(
                WarningCode::RevisionRemovedWithText,
                format!(
                    "the revisions {} of other authors went with text that this operation removed.",
                    ids_text(&swallowed)
                ),
            ));
        }
        operations.push(OpResult {
            index,
            op: op.name.to_string(),
            changed: op.changed,
            origin: i.origins.get(index).cloned(),
            blocks,
            added,
            added_count,
            revisions: written
                .iter()
                .filter(|w| w.op == Some(index))
                .map(|w| changes::entry(&w.info))
                .collect(),
            removed_own,
            folded,
            swallowed,
            resolution: op
                .resolution
                .and_then(|n| applied.resolutions.get(n))
                .map(|(_, r)| resolution(r, i.names)),
            comment: applied
                .comments
                .iter()
                .find(|(at, _)| *at == Some(index))
                .map(|(_, e)| format!("c{}", e.id)),
            warnings,
        });
    }
    let infos: Vec<_> = written.iter().map(|w| w.info.clone()).collect();
    let shown = blocks::shown(parts, i.base, i.format, applied)?;
    // One list, in order, with each warning once.
    let mut seen: HashSet<(WarningCode, Option<String>, String)> = HashSet::new();
    let mut warnings = Vec::new();
    let every = operations
        .iter()
        .flat_map(|o| o.warnings.iter())
        .chain(applied.warnings.iter().map(|(_, w)| w))
        .chain(shown.warnings.iter())
        .chain(i.notes.iter());
    for w in every {
        if seen.insert((w.code, w.block.clone(), w.message.clone())) {
            warnings.push(w.clone());
        }
    }
    Ok(Receipt {
        base: i.base.to_string(),
        dry_run: i.dry_run,
        tracked: i.tracked,
        stamp: i.stamp.clone(),
        operations,
        revisions: AuthorRevisions::group(&infos),
        comments: applied
            .comments
            .iter()
            .map(|(_, e)| comment_entry(e, i.names))
            .collect(),
        retired: Vec::new(),
        added: Vec::new(),
        added_count: 0,
        moves: Vec::new(),
        blocks: shown.blocks,
        blocks_omitted: shown.omitted,
        warnings,
        validation: i.validation.clone(),
        text: String::new(),
    })
}

/// Adds what only the commit knows, and writes the receipt's text. `retired` holds every ref that went stale: those whose
/// element left the document, and those a merge absorbed, as `merged into p5` (or the survivor's reason, when the batch removed
/// the survivor too).
pub(super) fn complete(receipt: &mut Receipt, committed: &Committed) {
    receipt.retired = committed
        .retired
        .iter()
        .chain(&committed.absorbed)
        .map(|(shown, reason)| Retired {
            r#ref: shown.to_string(),
            reason: match reason {
                Reason::Deleted => "deleted".to_string(),
                Reason::Rejected => "rejected".to_string(),
                Reason::MergedInto(into) => format!("merged into {into}"),
            },
        })
        .collect();
    receipt.added_count = u32::try_from(committed.minted.len()).unwrap_or(u32::MAX);
    receipt.added = if receipt.dry_run {
        Vec::new()
    } else {
        committed
            .minted
            .iter()
            .map(|(_, shown)| shown.to_string())
            .collect()
    };
    receipt.text = receipt_text::compose(receipt);
}
