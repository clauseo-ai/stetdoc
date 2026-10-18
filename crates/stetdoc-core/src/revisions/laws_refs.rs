//! Law R7: refs are right. After a commit every ref that was issued either still names its block, or was
//! retired, merged or rebound, and the commit's report says which, with the reason the identity model gives.
//! The model is the rules of `refs-and-state`: a ref follows its element, so a block that is still in the
//! document keeps its ref on the same element, and a block that left it is gone for a reason that a batch
//! that tracks its changes never makes `deleted` for an element that a tracked deletion kept, because that
//! element is still there.

use super::tests_support::MAIN;
use crate::state::{Committed, State};
use crate::wordml::{BlockKey, Mode};

/// What a resolution or a batch does to the blocks, for the reasons it may give.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Reasons {
    /// A tracked batch: removed blocks are `Deleted` (an own pending insertion that was taken out) or
    /// merged into another.
    Batch,
    /// An untracked batch: removed blocks are `Deleted` or merged into another, and what they held is
    /// gone with them.
    Untracked,
    /// Accept: a removed block is `Deleted` or merged into another, never `Rejected`.
    Accept,
    /// Reject: a removed block is `Rejected` or merged into another, never `Deleted`.
    Reject,
}

/// The blocks of the main part of a state with their refs.
fn blocks_with_refs(state: &State) -> Vec<(BlockKey, String)> {
    let view = state.parts();
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    story
        .blocks()
        .iter()
        .filter_map(|e| {
            let key = story.key(e);
            view.ref_of(&key).map(|r| (key, r.to_string()))
        })
        .collect()
}

/// The text of the paragraph at a key in a mode, empty for a block that is not a paragraph or is not there.
fn own_text(state: &State, key: &BlockKey, mode: Mode) -> String {
    let view = state.parts();
    let Some(id) = view.id(MAIN) else {
        return String::new();
    };
    let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
        return String::new();
    };
    story
        .index_of(key.node())
        .and_then(|i| story.paragraph(tree, i))
        .map(|p| p.text(mode))
        .unwrap_or_default()
}

/// The text that must still be somewhere after what the batch or the resolution did, and the mode that
/// reads it before and after. Original text is never lost by a batch, `Final` text is never lost by an
/// accept, and `Original` text is never lost by a reject (the resolved document reads the same in either
/// mode).
fn survives(reasons: Reasons) -> (Mode, Mode) {
    match reasons {
        Reasons::Batch | Reasons::Untracked => (Mode::Original, Mode::Original),
        Reasons::Accept => (Mode::Final, Mode::Final),
        Reasons::Reject => (Mode::Original, Mode::Final),
    }
}

/// The block at the end of a chain of merges that starts at `key`: the survivor of the survivor, as far as
/// the merges go. `None` if `key` was not absorbed.
fn last_survivor(key: &BlockKey, merged: &[(BlockKey, BlockKey)]) -> Option<BlockKey> {
    let mut at = key.clone();
    let mut found = None;
    for _ in 0..=merged.len() {
        let Some((_, kept)) = merged.iter().find(|(gone, _)| *gone == at) else {
            break;
        };
        found = Some(kept.clone());
        at = kept.clone();
    }
    found
}

/// Checks the refs of `before` against `after` and the report of the commit between them. `merged` lists
/// the merges that were declared, as the block that was absorbed and the block that survived: a ref may
/// follow a merge to the survivor, because a merge keeps the ref of whichever block had one. Returns what
/// is wrong, if anything.
pub(super) fn check_refs(
    before: &State,
    after: &State,
    committed: &Committed,
    reasons: Reasons,
    merged: &[(BlockKey, BlockKey)],
) -> Result<(), String> {
    let view = after.parts();
    let before_refs = blocks_with_refs(before);
    // A merge that the batch reports with refs on both sides is declared at commit: the absorbed ref is
    // merged away, or rebound when the survivor had none.
    for (gone, kept) in merged {
        let (Some((_, gone_ref)), Some((_, kept_ref))) = (
            before_refs.iter().find(|(k, _)| k == gone),
            before_refs.iter().find(|(k, _)| k == kept),
        ) else {
            continue;
        };
        // Or, when a rejection emptied the second half of a split, it is retired as rejected.
        let declared = committed
            .merged
            .iter()
            .any(|(s, a)| s.to_string() == *kept_ref && a.to_string() == *gone_ref)
            || committed.retired.iter().any(|(r, why)| {
                r.to_string() == *gone_ref && format!("{why:?}").contains("Rejected")
            });
        if !declared {
            return Err(format!(
                "{gone_ref} was merged into {kept_ref} and the commit did not say so"
            ));
        }
    }
    for (key, rf) in before_refs {
        let attached = view.is_attached(&key).unwrap_or(false);
        let retired = committed.retired.iter().find(|(r, _)| r.to_string() == rf);
        let rebound = committed.rebound.iter().find(|(r, _)| r.to_string() == rf);
        let absorbed = committed
            .merged
            .iter()
            .any(|(_, gone)| gone.to_string() == rf);
        if let Some((_, reason)) = retired {
            if attached {
                return Err(format!(
                    "{rf} was retired, but its block is still in the document"
                ));
            }
            let why = format!("{reason:?}");
            let ok = match reasons {
                Reasons::Batch | Reasons::Accept | Reasons::Untracked => {
                    why.contains("Deleted") || why.contains("MergedInto")
                }
                Reasons::Reject => why.contains("Rejected") || why.contains("MergedInto"),
            };
            if !ok {
                return Err(format!(
                    "{rf} was retired with the wrong reason for {reasons:?}: {why}"
                ));
            }
            // A ref is retired only when nothing of its block is left to name: the text that must survive
            // is not in a block that is gone.
            let (mode, _) = survives(reasons);
            let left = own_text(before, &key, mode);
            if reasons != Reasons::Untracked && !why.contains("MergedInto") && !left.is_empty() {
                // Text may leave a block by a split of the batch, into a block the batch made. It may not
                // be taken in by a block that was there before, which keeps its own ref: the retired
                // block's text would then be in an element that another ref names.
                let taken_in = |other: &BlockKey| {
                    view.is_attached(other).unwrap_or(false)
                        && own_text(after, other, mode).contains(&left)
                        && !own_text(before, other, mode).contains(&left)
                };
                let holder = blocks_with_refs(before)
                    .into_iter()
                    .find(|(other, _)| *other != key && taken_in(other));
                if !matches!(reasons, Reasons::Batch | Reasons::Untracked) || holder.is_some() {
                    let at = holder.map_or_else(
                        || "the document".to_owned(),
                        |(_, other_ref)| format!("the block of {other_ref}"),
                    );
                    return Err(format!(
                        "{rf} was retired ({why}) while its text {left:?} is still in {at}"
                    ));
                }
            }
            continue;
        }
        // A ref that a batch rebound to a new element and then merged away (the element was joined into
        // another) is reported as both, and it is merged.
        if absorbed {
            if attached {
                return Err(format!(
                    "{rf} was merged away, but its block is still in the document"
                ));
            }
            let (before_mode, after_mode) = survives(reasons);
            let moved = own_text(before, &key, before_mode);
            // A survivor may be absorbed in its turn by the join before it, so the text is looked for in
            // the block at the end of the chain.
            if let Some(kept) = last_survivor(&key, merged)
                && !matches!(reasons, Reasons::Batch | Reasons::Untracked)
                && !own_text(after, &kept, after_mode).contains(&moved)
            {
                return Err(format!(
                    "{rf} was merged into a block that does not hold its text {moved:?}"
                ));
            }
            continue;
        }
        if let Some((_, now)) = rebound {
            // A merge may have been absorbed by another in the same batch, so the ref lands at the end of
            // the chain of survivors.
            if last_survivor(&key, merged).as_ref() != Some(now) {
                return Err(format!(
                    "{rf} was rebound from {:?} to {:?}, and no merge says so: the merges are {:?}",
                    key.node(),
                    now.node(),
                    merged
                        .iter()
                        .map(|(a, b)| (a.node(), b.node()))
                        .collect::<Vec<_>>()
                ));
            }
            if !view.is_attached(now).unwrap_or(false) {
                return Err(format!(
                    "{rf} was rebound to a block that is not in the document"
                ));
            }
            let (before_mode, after_mode) = survives(reasons);
            let moved = own_text(before, &key, before_mode);
            let end = last_survivor(&key, merged).unwrap_or_else(|| now.clone());
            // A batch may split a block and join another into its first half, so a batch is held to R3's
            // model for the text, and a resolution, which splits nothing, to this.
            if !matches!(reasons, Reasons::Batch | Reasons::Untracked)
                && !own_text(after, &end, after_mode).contains(&moved)
            {
                return Err(format!(
                    "{rf} was rebound to a block that does not hold its text {moved:?}"
                ));
            }
            continue;
        }
        // Nothing happened to the ref: it is on the same block, which is in the document.
        if !attached {
            return Err(format!(
                "{rf} is on a block that left the document and was not retired"
            ));
        }
        if view.ref_of(&key).map(|r| r.to_string()).as_deref() != Some(rf.as_str()) {
            return Err(format!("{rf} is not on its block any more"));
        }
    }
    // Every block in the document has a ref.
    let id = view.id(MAIN).unwrap();
    let story = view.story(id).unwrap();
    for e in story.blocks() {
        let key = story.key(e);
        if view.ref_of(&key).is_none() {
            return Err(format!(
                "the block {:?} has no ref after the commit",
                key.node()
            ));
        }
    }
    Ok(())
}
