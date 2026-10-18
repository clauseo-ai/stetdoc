//! Matched paragraphs (design 6.4, step 7): a line whose ref is in the baseline is compared with it.
//!
//! The tags are compared first, then the seams, then the slots. A tag or a seam that differs is
//! `readonly_region` and the message names it. A paragraph that is read-only as a whole refuses any slot
//! change. A slot that differs is one `replace` with the range of the whole old slot in the `Final` text,
//! not trimmed: the compiler tokenizes the range and the new text and writes a word-level redline, and a
//! trim of ours that cut a word would turn "cat" into "cart" as an insertion of one letter. Within a
//! paragraph the operations go from the last slot to the first, so an earlier offset is never moved by a
//! later change.

use super::refuse;
use crate::api::error::StetError;
use crate::api::files::PlannedOp;
use crate::projection::draft::DraftBlock;
use crate::projection::keys::Key;
use crate::projection::model::count;
use crate::refs::BlockRef;

/// The first place where two lists of keys differ: `(was, now)`.
fn first_difference<'a>(
    was: &'a [Key],
    now: &'a [Key],
) -> Option<(Option<&'a Key>, Option<&'a Key>)> {
    let longest = was.len().max(now.len());
    (0..longest).find_map(|i| {
        let (w, n) = (was.get(i), now.get(i));
        (w != n).then_some((w, n))
    })
}

/// True if the edited line is the baseline's paragraph, tag for tag, seam for seam and slot for slot.
pub(super) fn same(old: &DraftBlock, new: &DraftBlock) -> bool {
    old == new
}

/// The operations that turn the old paragraph into the edited line, or why they cannot.
pub(super) fn compare(
    file: &str,
    r: BlockRef,
    old: &DraftBlock,
    new: &DraftBlock,
) -> Result<Vec<PlannedOp>, StetError> {
    if let Some((was, now)) = first_difference(&old.tags, &new.tags) {
        return Err(refuse::tag_changed(file, new.line, r, was, now));
    }
    let old_seams: Vec<Key> = old.seams.iter().map(|s| s.key.clone()).collect();
    let new_seams: Vec<Key> = new.seams.iter().map(|s| s.key.clone()).collect();
    if let Some((was, now)) = first_difference(&old_seams, &new_seams) {
        return Err(refuse::seam_changed(file, new.line, r, was, now));
    }
    let starts = old.slot_starts();
    let mut ops = Vec::new();
    for (index, (was, now)) in old.texts.iter().zip(&new.texts).enumerate().rev() {
        if was == now {
            continue;
        }
        if let Some(reason) = old.facts.readonly {
            return Err(refuse::slot_of_readonly(file, new.line, r, reason));
        }
        let start = starts.get(index).copied().unwrap_or(0);
        ops.push(PlannedOp::Replace {
            block: r.to_string(),
            start,
            end: start.saturating_add(count(was)),
            with: now.clone(),
        });
    }
    Ok(ops)
}
