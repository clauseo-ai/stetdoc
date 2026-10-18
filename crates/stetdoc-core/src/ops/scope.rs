//! The scope: what an edit may change, and the guard that holds it to that (design `document-and-ops.md` 3.3).
//!
//! `Edit::scope` gives refs and ranges. A block is in scope when it or a block that holds it is named, so `t3` covers its rows
//! and the paragraphs in their cells, and a range `p40..p60` covers every block between its ends in the story's order. The guard
//! compares what the batch touched, what each operation named and which parts changed with the scope, and refuses the whole
//! batch with `scope_violation` for anything outside it. A new block has no ref and cannot be named, so it is covered by the
//! block it was made next to, which is a target.

use super::context::Applied;
use super::scope_frame::{FrameChange, frame_changes, in_body_section};
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::refs::BaseToken;
use crate::revisions::changes::{Change, changes};
use crate::state::Tx;
use crate::wordml::BlockKey;
use crate::wordml::{NodeId, SpanTree};
use std::collections::HashSet;
use std::ops::Range;

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

fn violation(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::ScopeViolation, message)
}

/// The blocks an edit may change, resolved at the start of the batch.
#[derive(Clone, Debug)]
pub(crate) struct ScopeSet {
    text: String,
    covered: HashSet<BlockKey>,
}

impl ScopeSet {
    /// Resolves refs and ranges against the base token the batch names. A ref that changed after the batch's version is
    /// `stale_ref`, a reversed range or one with ends in two stories is `invalid_payload`, and more entries than
    /// `max_scope_refs` is `limit_exceeded`.
    pub(crate) fn resolve(
        tx: &Tx<'_>,
        base: &BaseToken,
        entries: &[String],
        limits: &Limits,
    ) -> Result<ScopeSet, StetError> {
        if entries.len() as u64 > u64::from(limits.max_scope_refs) {
            return Err(StetError::new(
                ErrorCode::LimitExceeded,
                format!(
                    "the scope has {} entries, over the limit of {}.",
                    entries.len(),
                    limits.max_scope_refs
                ),
            )
            .with_next("Name a range such as p40..p60 instead of each block."));
        }
        let mut covered = HashSet::new();
        for entry in entries {
            match entry.split_once("..") {
                None => {
                    covered.insert(super::target::block(tx, base, entry.trim())?);
                }
                Some((from, to)) => {
                    let (a, b) = (
                        super::target::block(tx, base, from.trim())?,
                        super::target::block(tx, base, to.trim())?,
                    );
                    covered.extend(range_keys(tx, &a, &b, entry)?);
                }
            }
        }
        Ok(ScopeSet {
            text: entries.join(", "),
            covered,
        })
    }

    /// The scope as the agent gave it, for a message.
    pub(crate) fn text(&self) -> &str {
        &self.text
    }

    /// True when the block, or a block that holds it, is named.
    pub(crate) fn contains(&self, tx: &Tx<'_>, key: &BlockKey) -> bool {
        if self.covered.contains(key) {
            return true;
        }
        let view = tx.parts();
        let found = (|| {
            let part = view.id(key.part())?;
            let story = view.story(part).ok()?;
            let entries = story.blocks();
            let mut at = entries.get(story.index_of(key.node())?)?.parent;
            while let Some(parent) = at {
                let entry = entries.get(parent as usize)?;
                if self.covered.contains(&story.key(entry)) {
                    return Some(true);
                }
                at = entry.parent;
            }
            Some(false)
        })();
        found.unwrap_or(false)
    }
}

/// The blocks from one end of a range to the other, in the story's order.
fn range_keys(
    tx: &Tx<'_>,
    a: &BlockKey,
    b: &BlockKey,
    text: &str,
) -> Result<Vec<BlockKey>, StetError> {
    if a.part() != b.part() {
        return Err(invalid(format!(
            "the range {text} has its ends in two stories."
        )));
    }
    let view = tx.parts();
    let part = view.id(a.part()).ok_or_else(|| {
        StetError::new(
            ErrorCode::Internal,
            "internal: a block's part is not in the document",
        )
    })?;
    let story = view.story(part)?;
    let (Some(from), Some(to)) = (story.index_of(a.node()), story.index_of(b.node())) else {
        return Err(StetError::new(
            ErrorCode::StaleRef,
            format!("an end of the range {text} is not in the document any more."),
        ));
    };
    if from > to {
        return Err(invalid(format!(
            "the range {text} is reversed: its first end comes after its second."
        )));
    }
    Ok(story
        .blocks()
        .iter()
        .skip(from)
        .take(to - from + 1)
        .map(|e| story.key(e))
        .collect())
}

/// What the guard says of a block outside the scope: the block, the scope and the operation.
fn outside(tx: &Tx<'_>, scope: &ScopeSet, applied: &Applied, key: &BlockKey) -> StetError {
    let shown = tx
        .parts()
        .ref_of(key)
        .map_or_else(|| "a block".to_string(), |r| r.to_string());
    let op = applied.op_touching(key);
    let by = match op.and_then(|i| applied.ops.get(i).map(|o| (i, o))) {
        Some((_, o)) => format!(" {}", o.by),
        None => String::new(),
    };
    let error = violation(format!(
        "{shown} is outside the scope ({}).{by}",
        scope.text()
    ))
    .with_next("Widen the scope to hold it, or leave that change out of the batch.");
    match op {
        Some(i) => error.at_op(i),
        None => error,
    }
}

/// The first pass, before `check`: the blocks the operations touched and named, and the parts that changed.
pub(crate) fn guard(tx: &Tx<'_>, scope: &ScopeSet, applied: &Applied) -> Result<(), StetError> {
    let new_blocks: HashSet<BlockKey> = applied.new_blocks().into_iter().collect();
    let owned: HashSet<&BlockKey> = applied.ops.iter().flat_map(|o| o.owned.iter()).collect();
    let touched = tx
        .touched()
        .get(..applied.touched_by_ops)
        .unwrap_or_default();
    // In operation order, so that the first failure is the first operation's.
    let mut first: Option<(usize, StetError)> = None;
    let mut note = |op: Option<usize>, error: StetError| {
        let at = op.unwrap_or(usize::MAX);
        if first.as_ref().is_none_or(|(i, _)| at < *i) {
            first = Some((at, error));
        }
    };
    // The comments story is ignored for touches: `comments` creates and rewrites its paragraphs, and no scope can name them.
    let comment_parts = super::comments::comment_parts(tx);
    for key in touched {
        if comment_parts.contains(key.part())
            || new_blocks.contains(key)
            || owned.contains(key)
            || scope.contains(tx, key)
        {
            continue;
        }
        note(applied.op_touching(key), outside(tx, scope, applied, key));
    }
    for (i, op) in applied.ops.iter().enumerate() {
        for key in &op.targets {
            if !scope.contains(tx, key) {
                note(Some(i), outside(tx, scope, applied, key));
            }
        }
    }
    if let Some((_, error)) = first {
        return Err(error);
    }
    // A part that no block covers changes only as an operation declared.
    let view = tx.parts();
    let stories: Vec<std::sync::Arc<str>> = view
        .story_specs()?
        .0
        .iter()
        .map(|s| s.part.clone())
        .collect();
    for part in tx.changed_parts() {
        let declared: Vec<_> = applied
            .side_effects
            .iter()
            .filter(|s| *s.part == *part)
            .collect();
        if stories.contains(&part) {
            continue;
        }
        if !declared.is_empty() {
            // A declaration of the whole part, or of the styles appended to it, which must be all that differs.
            if declared.iter().any(|s| s.styles.is_none()) {
                continue;
            }
            let ids: Vec<&str> = declared
                .iter()
                .flat_map(|s| s.styles.iter().flatten().map(String::as_str))
                .collect();
            if only_appended_styles(tx, &part, &ids) {
                continue;
            }
            return Err(violation(format!(
                "the batch would change {part} beyond the styles its operations appended ({}), and no block of the scope ({}) covers it.",
                if ids.is_empty() { "none".to_string() } else { ids.join(", ") },
                scope.text()
            ))
            .with_next("Run the batch without a scope, or leave out the operation that changes it."));
        }
        return Err(violation(format!(
            "the batch would change {part}, which no block of the scope ({}) covers and no operation declared.",
            scope.text()
        ))
        .with_next("Run the batch without a scope, or leave out the operation that changes it."));
    }
    Ok(())
}

/// True when the part differs from the base's only by `w:style` elements appended to its root with these ids: the root's own
/// attributes are the base's (a namespace declaration aside), and every child the base had is there, in order and unchanged.
fn only_appended_styles(tx: &Tx<'_>, part: &str, ids: &[&str]) -> bool {
    let (after, before) = (tx.parts(), tx.base().parts());
    let Some(now) = after.id(part).and_then(|a| after.tree(a).ok()) else {
        return false;
    };
    let declared = |id: NodeId| {
        let child = now.node_ref(id);
        child.local_name() == "style"
            && child
                .attrs()
                .find(|x| x.local_name() == "styleId")
                .is_some_and(|x| ids.contains(&x.value().as_ref()))
    };
    // A styles part that the operation added holds nothing but what it declared.
    let Some(was) = before.id(part).and_then(|b| before.tree(b).ok()) else {
        return now.root().child_elements().all(|c| declared(c.id()));
    };
    let (root, old) = (now.root(), was.root());
    let own = |tree: &SpanTree, id: NodeId| -> Vec<(String, String, Vec<u8>)> {
        let mut list: Vec<_> = tree
            .node_ref(id)
            .attrs()
            .filter(|x| x.local_name() != "xmlns" && !x.name().starts_with("xmlns"))
            .map(|x| {
                (
                    x.namespace().to_string(),
                    x.local_name().to_string(),
                    x.raw_value().to_vec(),
                )
            })
            .collect();
        list.sort_unstable();
        list
    };
    if root.id() != old.id() || own(now, root.id()) != own(was, old.id()) {
        return false;
    }
    let kept: Vec<NodeId> = old.children().map(|c| c.id()).collect();
    let mut at = kept.iter().peekable();
    for child in root.children() {
        if at.peek() == Some(&&child.id()) {
            at.next();
            if child.is_element() && child.is_dirty() {
                return false;
            }
            continue;
        }
        let ok = if child.is_element() {
            declared(child.id())
        } else {
            child.text().chars().all(char::is_whitespace)
        };
        if !ok {
            return false;
        }
    }
    at.next().is_none()
}

/// The attributes of an element as a sorted list, so that the order they were written in does not matter.
fn attributes(tree: &SpanTree, id: NodeId) -> Vec<(&str, &str, &[u8])> {
    let mut list: Vec<_> = tree
        .node_ref(id)
        .attrs()
        .map(|x| (x.namespace(), x.local_name(), x.raw_value()))
        .collect();
    list.sort_unstable();
    list
}

/// The children of an element that are not rows.
fn other_children(tree: &SpanTree, id: NodeId) -> Vec<NodeId> {
    tree.node_ref(id)
        .child_elements()
        .filter(|c| c.local_name() != "tr")
        .map(|c| c.id())
        .collect()
}

/// True when the table differs from the base's only in the rows it holds: its own attributes are the base's, and so are its other
/// children (the properties and the grid), none of which was edited in this transaction. Rows added or removed are blocks that
/// the operations named and the first pass checked, so such a table is covered by them.
fn rows_only(tx: &Tx<'_>, key: &BlockKey) -> bool {
    let (after, before) = (tx.parts(), tx.base().parts());
    let (Some(a), Some(b)) = (after.id(key.part()), before.id(key.part())) else {
        return false;
    };
    let (Ok(now), Ok(was)) = (after.tree(a), before.tree(b)) else {
        return false;
    };
    let node = key.node();
    let (na, nb) = (now.node_ref(node), was.node_ref(node));
    if na.local_name() != "tbl" || !nb.is_element() || na.namespace() != nb.namespace() {
        return false;
    }
    let kept = other_children(now, node);
    attributes(now, node) == attributes(was, node)
        && kept == other_children(was, node)
        && kept
            .iter()
            .all(|c| !now.node_ref(*c).is_dirty() || was.node_ref(*c).is_dirty())
}

/// The second pass, after `check`: the blocks that the trees say changed, whatever the operations declared (`Verified::changed`,
/// which `check` finds from the dirty flags and an equality test with the base and does not read a declaration). Each must be in the
/// scope, owned by an operation, new, or a table that differs from the base only in its rows. A compiler that forgot to touch a
/// block, or a resolver that changed one it did not report, is caught here, and the failure names the block.
pub(crate) fn guard_changed(
    tx: &Tx<'_>,
    scope: &ScopeSet,
    applied: &Applied,
    changed: &[BlockKey],
) -> Result<(), StetError> {
    let new_blocks: HashSet<BlockKey> = applied.new_blocks().into_iter().collect();
    let owned: HashSet<&BlockKey> = applied.ops.iter().flat_map(|o| o.owned.iter()).collect();
    // What `reconcile` touched after the operations is exempt, as in the first pass: the text edit that caused it was checked.
    let reconciled: HashSet<&BlockKey> = tx
        .touched()
        .get(applied.touched_by_ops..)
        .unwrap_or_default()
        .iter()
        .collect();
    let allowed = |key: &BlockKey| -> bool {
        new_blocks.contains(key)
            || owned.contains(key)
            || reconciled.contains(key)
            || scope.contains(tx, key)
    };
    let (after, before) = (tx.parts(), tx.base().parts());
    let mut first: Option<(usize, StetError)> = None;
    let mut note = |key: Option<&BlockKey>, error: StetError| {
        let at = key
            .and_then(|k| applied.op_touching(k))
            .unwrap_or(usize::MAX);
        if first.as_ref().is_none_or(|(i, _)| at < *i) {
            first = Some((at, error));
        }
    };
    let comment_parts = super::comments::comment_parts(tx);
    for key in changed {
        if comment_parts.contains(key.part()) {
            continue;
        }
        let (Some(now_id), Some(was_id)) = (after.id(key.part()), before.id(key.part())) else {
            continue;
        };
        let (now, was) = (after.tree(now_id)?, before.tree(was_id)?);
        if key.node() == now.root().id() {
            // A header or a footer that no section names any more (a later operation accepted the deletion of the section
            // break that named it) has no frame left in the document.
            if !after.is_story(now_id)? {
                continue;
            }
            // The frame: each difference belongs to the block after it, or to no block at all.
            let story = after.story(now_id)?;
            for change in frame_changes(&story, now, was) {
                match change {
                    FrameChange::Before(node) => {
                        let block = BlockKey::new(key.part().clone(), node);
                        if !allowed(&block) {
                            note(Some(&block), outside(tx, scope, applied, &block));
                        }
                    }
                    FrameChange::Unowned => note(
                        None,
                        violation(format!(
                            "the batch would change the frame of {}: the body's last section properties, or what lies \
                             after the last block, which no block of the scope ({}) holds.",
                            key.part(),
                            scope.text()
                        ))
                        .with_next("Run the batch without a scope."),
                    ),
                }
            }
            continue;
        }
        if allowed(key) {
            continue;
        }
        if !was.node_ref(key.node()).is_element() {
            // A block that is not in the base and that no operation reported making.
            return Err(StetError::new(
                ErrorCode::Internal,
                format!(
                    "internal: the batch made a block in {} that no operation reported.",
                    key.part()
                ),
            ));
        }
        if !rows_only(tx, key) {
            note(Some(key), outside(tx, scope, applied, key));
        }
    }
    first.map_or(Ok(()), |(_, error)| Err(error))
}

/// The changes in the blocks of the scope, whole, and the ids of the changes left out of them because they change the body's
/// last section properties.
pub(crate) struct InScope {
    /// What `accept` and `reject` by `all` or by author are narrowed to.
    pub(crate) changes: Vec<Change>,
    /// The ids of a change that D gives to the last block of the body because it is in the body's own `w:sectPr`, which no scope
    /// can name.
    pub(crate) section: Vec<i64>,
}

/// The changes in the blocks of the scope, whole: what `accept` and `reject` by `all` or by author are narrowed to. A change
/// that reaches out of the scope is taken whole, touches a block outside, and is refused by the block check. A change in the
/// body's last section properties belongs to no block, so it is never in a scope.
pub(crate) fn changes_in_scope(tx: &Tx<'_>, scope: &ScopeSet) -> Result<InScope, StetError> {
    let view = tx.parts();
    let mut out: Vec<Change> = Vec::new();
    let mut section: Vec<i64> = Vec::new();
    for spec in &view.story_specs()?.0 {
        let Some(part) = view.id(&spec.part) else {
            continue;
        };
        let story = view.story(part)?;
        let tree = view.tree(part)?;
        // Runs of consecutive entries in scope, one `changes` call for each.
        let mut runs: Vec<Range<usize>> = Vec::new();
        for (i, entry) in story.blocks().iter().enumerate() {
            if !scope.contains(tx, &story.key(entry)) {
                continue;
            }
            match runs.last_mut() {
                Some(run) if run.end == i => run.end = i + 1,
                _ => runs.push(i..i + 1),
            }
        }
        for run in runs {
            for change in changes(&story, tree, Some(run)) {
                if change.nodes.iter().any(|n| in_body_section(tree, *n)) {
                    section.extend(change.ids.iter().copied());
                    continue;
                }
                if !out
                    .iter()
                    .any(|c| c.ids.first() == change.ids.first() && c.kind == change.kind)
                {
                    out.push(change);
                }
            }
        }
    }
    Ok(InScope {
        changes: out,
        section,
    })
}
