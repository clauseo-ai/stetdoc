//! The commit-time audit: it keeps the ref table true to the trees.
//!
//! Commit runs it over the affected parts, on a clone of the table, so that it can fail without
//! having changed anything. It applies the operations' declarations, checks their composition against
//! the trees, retires the refs of elements that left, mints refs for elements that are new, and
//! stamps every block that changed. Whatever the operations did, "the live refs are exactly the
//! blocks in the trees, one to one" holds afterward. See `docs/design/refs-and-state.md` section 7.

use super::errors::internal;
use super::failpoint;
use super::intents::Intent;
use super::parts::PartId;
use super::stories::StoryList;
use super::tx::Tx;
use super::view::PartsView;
use crate::api::error::StetError;
use crate::refs::{BlockKey, BlockRef, Merged, Reason, RefKind, RefTable};
use crate::wordml::{
    BlockKind, MARK_DELETED, MARK_INSERTED, MARK_MOVED_FROM, MARK_MOVED_TO, NodeId, Story,
    StorySpec,
};
use crate::xml::SpanTree;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The four bits of `BlockEntry::flags` that say what revision a paragraph mark carries.
const MARK_BITS: u8 = MARK_INSERTED | MARK_DELETED | MARK_MOVED_FROM | MARK_MOVED_TO;

/// One affected part, scanned.
pub(super) struct Scan {
    /// The part's place in the new story list.
    pub(super) index: usize,
    pub(super) part: PartId,
    pub(super) spec: StorySpec,
    pub(super) story: Arc<Story>,
    /// The base's story for the part, when it was a story there.
    prev: Option<Arc<Story>>,
    /// Blocks that left the order they had, or changed parent, found by the refresh.
    moved: Vec<NodeId>,
    /// Blocks whose wrappers changed with neither their bytes nor their place changing: wrapped into
    /// or out of a content control or a revision, or their row or cell gained or lost a revision.
    /// Found by the refresh.
    context_changed: Vec<NodeId>,
    /// Tables and rows that gained, lost or reordered a row or a cell, or one of the nodes that make
    /// up their own properties. Found by comparing what each owns in the two trees.
    own_changed: Vec<NodeId>,
    /// Paragraphs that start in another field state, found by the refresh.
    field_starts: Vec<NodeId>,
}

/// What the audit leaves for commit to publish.
pub(super) struct Audited {
    /// The table with everything applied, and not yet advanced to the next version.
    pub(super) table: RefTable,
    pub(super) scans: Vec<Scan>,
    pub(super) minted: Vec<(BlockKey, BlockRef)>,
    pub(super) retired: Vec<(BlockRef, Reason)>,
    pub(super) rebound: Vec<(BlockRef, BlockKey)>,
    pub(super) merged: Vec<(BlockRef, BlockRef)>,
    /// The canonical text of the events, which goes into the commit's digest.
    pub(super) events: String,
}

/// The final scans, and the question every check asks of them: is this element a block of a story.
struct Finals<'a> {
    scans: &'a [Scan],
    after: &'a StoryList,
    view: &'a PartsView<'a>,
}

impl Finals<'_> {
    /// The kind of block the element is in the final document, or `None` if it is not a block of a
    /// story.
    fn kind_of(&self, key: &BlockKey) -> Result<Option<BlockKind>, StetError> {
        let story = match self.scans.iter().find(|s| *s.spec.part == **key.part()) {
            Some(scan) => Arc::clone(&scan.story),
            None => {
                if !self.after.0.iter().any(|s| *s.part == **key.part()) {
                    return Ok(None);
                }
                let id = self.view.id(key.part()).ok_or_else(|| {
                    internal(format!("{} is a story and has no part number", key.part()))
                })?;
                self.view.story(id)?
            }
        };
        Ok(story
            .index_of(key.node())
            .and_then(|i| story.blocks().get(i))
            .map(|e| e.kind))
    }

    /// The place of an element for the commit's digest: the story's index in the story list and
    /// the block's ordinal, or `None` if it is not a block of a story.
    fn position(&self, key: &BlockKey) -> Result<Option<(usize, usize)>, StetError> {
        let Some(index) = self.after.0.iter().position(|s| *s.part == **key.part()) else {
            return Ok(None);
        };
        let story = match self.scans.iter().find(|s| *s.spec.part == **key.part()) {
            Some(scan) => Arc::clone(&scan.story),
            None => {
                let id = self.view.id(key.part()).ok_or_else(|| {
                    internal(format!("{} is a story and has no part number", key.part()))
                })?;
                self.view.story(id)?
            }
        };
        Ok(story.index_of(key.node()).map(|ordinal| (index, ordinal)))
    }
}

/// A place for the commit's digest, written as two numbers, or a dash for an element that is not a
/// block of a story. The digest must not depend on how a Rust type happens to print.
fn place(p: Option<(usize, usize)>) -> String {
    p.map_or_else(
        || "-".to_owned(),
        |(story, ordinal)| format!("{story} {ordinal}"),
    )
}

fn live_ref(table: &RefTable, key: &BlockKey, what: &str) -> Result<BlockRef, StetError> {
    table.ref_of(key).ok_or_else(|| {
        internal(format!(
            "{what} was declared for a block in {} that has no live ref",
            key.part()
        ))
    })
}

/// Runs the audit for a transaction that will become version `at`.
pub(super) fn run(tx: &Tx<'_>, at: u32) -> Result<Audited, StetError> {
    let base = tx.base();
    // The base is read through its own caches (the story list, the story scans and the field pass
    // that `refresh` compares with), which a commit that fails leaves filled. They are functions of
    // the committed parts, and the first read of the base would fill them in the same way, so this
    // changes nothing a caller can see. Keeping them in a store of the commit would repeat the scan
    // of the base's stories for every dry run that comes before the real edit.
    let before_view = base.parts();
    let view = tx.parts();
    let before = before_view.story_specs()?;
    let after = view.story_specs()?;
    let mut table = base.refs.clone();

    // 1. Scan the affected parts: the staged stories, every story whose kind or existence is new, and
    // every story that a declaration names, since a declaration can bind a ref to an element of a
    // part that nothing else changed.
    let named: HashSet<&str> = tx
        .intents
        .iter()
        .flat_map(|intent| match intent {
            Intent::Retire { key, .. } => vec![key],
            Intent::Rebind { from, to } => vec![from, to],
            Intent::Merge {
                survivor,
                absorbed,
                into,
            } => vec![survivor, absorbed, into],
        })
        .map(|key| &**key.part())
        .collect();
    let mut scans = Vec::new();
    for (index, spec) in after.0.iter().enumerate() {
        let part = view
            .id(&spec.part)
            .ok_or_else(|| internal(format!("{} is a story and has no part number", spec.part)))?;
        let was_story = before
            .0
            .iter()
            .any(|b| *b.part == *spec.part && b.kind == spec.kind);
        let edited = tx.staged.get(&part).is_some_and(|s| s.changed());
        if was_story && !edited && !named.contains(&*spec.part) {
            continue;
        }
        failpoint::here("audit:scan")?;
        let tree = view.tree(part)?;
        let name = view
            .name(part)
            .ok_or_else(|| internal(format!("{} has no name", spec.part)))?;
        let (story, prev, moved, context_changed, field_starts, own_changed) = if was_story {
            let prev = before_view.story(part)?;
            // `refresh` compares the field state of the paragraphs of `prev` with the new story's,
            // and when `prev` never ran the field pass it reports nothing. A state that was just
            // opened or resumed has no pass yet, and a stale batch must still be refused for a
            // paragraph whose field start this commit changes, so the base's pass runs first. It runs
            // once per story and session, since the new story carries the index on.
            let _ = prev.fields(before_view.tree(part)?);
            let r = Story::refresh(&prev, tree, &tx.touched_nodes(&spec.part))?;
            let own = own_changes(&prev, before_view.tree(part)?, &r.story, tree);
            (
                Arc::new(r.story),
                Some(prev),
                r.moved,
                r.context_changed,
                r.field_starts_changed,
                own,
            )
        } else {
            (
                Arc::new(Story::scan(name, spec.kind, tree)?),
                None,
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
        };
        scans.push(Scan {
            index,
            part,
            spec: spec.clone(),
            story,
            prev,
            moved,
            context_changed,
            own_changed,
            field_starts,
        });
    }
    let removed_parts: Vec<Arc<str>> = before
        .0
        .iter()
        .filter(|b| !after.0.iter().any(|a| *a.part == *b.part))
        .map(|b| Arc::clone(&b.part))
        .collect();
    let finals = Finals {
        scans: &scans,
        after: &after,
        view: &view,
    };

    // 2. Apply the declarations, in order, to the clone of the table.
    failpoint::here("audit:declarations")?;
    let mut reasons: HashMap<BlockRef, Reason> = HashMap::new();
    let mut retired_by_merges = Merged::default();
    let mut moved_refs: Vec<BlockRef> = Vec::new();
    let mut released: Vec<(BlockKey, BlockRef, &'static str)> = Vec::new();
    let mut rebound = Vec::new();
    let mut merged = Vec::new();
    let mut merges = Vec::new();
    for intent in &tx.intents {
        match intent {
            Intent::Retire { key, reason } => {
                let r = live_ref(&table, key, "retire")?;
                reasons.insert(r, (*reason).into());
                released.push((key.clone(), r, "retire"));
            }
            Intent::Rebind { from, to } => {
                let r = live_ref(&table, from, "rebind")?;
                table.rebind(r, to)?;
                released.retain(|(k, ..)| k != to);
                if !moved_refs.contains(&r) {
                    moved_refs.push(r);
                }
                rebound.push((r, to.clone()));
            }
            Intent::Merge {
                survivor,
                absorbed,
                into,
            } => {
                let rs = live_ref(&table, survivor, "merge")?;
                let ra = live_ref(&table, absorbed, "merge")?;
                table.merge(rs, ra, into, at, &mut retired_by_merges)?;
                released.retain(|(k, ..)| k != into);
                for (key, r) in [(survivor, rs), (absorbed, ra)] {
                    if key != into {
                        released.push((key.clone(), r, "merge"));
                    }
                }
                if !moved_refs.contains(&rs) {
                    moved_refs.push(rs);
                }
                merged.push((rs, ra));
                merges.push((rs, ra, into.clone()));
            }
        }
    }

    // 3. Check the composed result of the declarations, before anything is retired or minted.
    failpoint::here("audit:check")?;
    for r in &moved_refs {
        let Some(key) = table.key_of(*r) else {
            continue;
        };
        match finals.kind_of(&key)? {
            Some(kind) if RefKind::of(kind) == r.kind() => {}
            Some(_) => {
                return Err(internal(format!(
                    "{r} was bound to a block of another kind in {}",
                    key.part()
                )));
            }
            None if !after.0.iter().any(|s| *s.part == **key.part()) => {
                return Err(internal(format!(
                    "{r} was bound to an element of {}, which is not a story",
                    key.part()
                )));
            }
            None => {
                // A ref bound to an element that has since left the document is retired in step 4,
                // like any block a later operation deleted. One bound to an element that is in the
                // document and is not a block is a contradiction.
                if view.is_attached(&key)? {
                    return Err(internal(format!(
                        "{r} was bound to an element of {} that is not a block of a story",
                        key.part()
                    )));
                }
            }
        }
    }
    for (key, r, what) in &released {
        if finals.kind_of(key)?.is_some() {
            return Err(internal(format!(
                "{what} released the element of {r}, and it is still a block of {}",
                key.part()
            )));
        }
    }

    // 4. Retire every live ref whose element is not in the scan of its part.
    failpoint::here("audit:tombstone")?;
    let mut retired = Vec::new();
    for scan in &scans {
        for (r, node) in table.live_in(&scan.spec.part) {
            if scan.story.index_of(node).is_none() {
                let reason = reasons.get(&r).copied().unwrap_or(Reason::Deleted);
                table.tombstone(r, reason, at, &retired_by_merges)?;
                retired.push((r, reason));
            }
        }
    }
    for part in &removed_parts {
        for (r, _) in table.live_in(part) {
            table.tombstone(r, Reason::Deleted, at, &retired_by_merges)?;
            retired.push((r, Reason::Deleted));
        }
    }

    // 5. Mint a ref for every block that has none, in document order and story order.
    failpoint::here("audit:mint")?;
    let mut minted = Vec::new();
    let mut events = String::new();
    for scan in &scans {
        for (ordinal, entry) in scan.story.blocks().iter().enumerate() {
            let key = scan.story.key(entry);
            if table.ref_of(&key).is_none() {
                let r = table.allocate(&key, entry.kind, at)?;
                events.push_str(&format!("mint {r} {} {ordinal}\n", scan.index));
                minted.push((key, r));
            }
        }
    }
    for (r, reason) in &retired {
        events.push_str(&match reason {
            Reason::Deleted => format!("retire {r} deleted\n"),
            Reason::Rejected => format!("retire {r} rejected\n"),
            Reason::MergedInto(into) => format!("retire {r} merged {into}\n"),
        });
    }
    for (r, to) in &rebound {
        events.push_str(&format!("rebind {r} {}\n", place(finals.position(to)?)));
    }
    for (survivor, absorbed, into) in &merges {
        events.push_str(&format!(
            "merge {survivor} {absorbed} {}\n",
            place(finals.position(into)?)
        ));
    }

    // 6. Stamp the blocks that changed.
    failpoint::here("audit:stamp")?;
    for key in &tx.touched {
        table.touch(key, at);
    }
    for (_, to) in &rebound {
        table.touch(to, at);
    }
    for scan in &scans {
        let after_marks = after_mark_changes(scan.prev.as_deref(), &scan.story);
        let stamped = scan
            .moved
            .iter()
            .chain(&scan.context_changed)
            .chain(&scan.own_changed)
            .chain(&scan.field_starts)
            .chain(&after_marks);
        for node in stamped {
            table.touch(&BlockKey::new(Arc::clone(scan.story.part()), *node), at);
        }
    }

    Ok(Audited {
        table,
        scans,
        minted,
        retired,
        rebound,
        merged,
        events,
    })
}

/// The tables and rows whose own parts changed in a way that the two trees show: a row or a cell was
/// inserted, removed or reordered, or a node of the block's own properties was added or removed
/// (`Story::own_parts`, which says what a table or a row owns). An edit inside one of those nodes,
/// a changed attribute of `w:tblPr` or `w:trPr`, is not visible without comparing bytes, and is the
/// operation's `touch`. The audit stamps these anyway, so that a stale batch that names a table
/// whose rows changed is refused though no operation touched the table.
fn own_changes(prev: &Story, before: &SpanTree, now: &Story, after: &SpanTree) -> Vec<NodeId> {
    let mut out = Vec::new();
    for (i, entry) in now.blocks().iter().enumerate() {
        if entry.kind == BlockKind::Paragraph {
            continue;
        }
        let Some(was) = prev.index_of(entry.node) else {
            continue;
        };
        if prev.own_parts(before, was) != now.own_parts(after, i) {
            out.push(entry.node);
        }
    }
    out
}

/// The paragraphs to stamp because the paragraph before them changed its mark revision. In `Final`
/// a paragraph whose mark is deleted joins the next one, and in `Original` one whose mark is
/// inserted does, so a change of the mark changes what the next paragraph's text is part of.
///
/// A paragraph counts as changed when its `MARK_*` bits differ between the base scan and the new
/// scan by `NodeId`, or when it is new or removed and has any of them. For a removed paragraph the
/// next paragraph is the one that followed it in the base scan and is still there.
fn after_mark_changes(prev: Option<&Story>, now: &Story) -> Vec<NodeId> {
    let before: HashMap<NodeId, u8> = prev
        .map(|p| {
            p.blocks()
                .iter()
                .filter(|e| e.kind == BlockKind::Paragraph)
                .map(|e| (e.node, e.flags & MARK_BITS))
                .collect()
        })
        .unwrap_or_default();
    let blocks = now.blocks();
    let mut out = Vec::new();
    for (i, entry) in blocks.iter().enumerate() {
        if entry.kind != BlockKind::Paragraph {
            continue;
        }
        let was = before.get(&entry.node).copied().unwrap_or(0);
        if was != entry.flags & MARK_BITS
            && let Some(next) = blocks
                .iter()
                .skip(i + 1)
                .find(|e| e.kind == BlockKind::Paragraph)
        {
            out.push(next.node);
        }
    }
    if let Some(p) = prev {
        let present: HashSet<NodeId> = blocks.iter().map(|e| e.node).collect();
        let old = p.blocks();
        for (i, entry) in old.iter().enumerate() {
            if entry.kind == BlockKind::Paragraph
                && entry.flags & MARK_BITS != 0
                && !present.contains(&entry.node)
                && let Some(next) = old
                    .iter()
                    .skip(i + 1)
                    .find(|e| e.kind == BlockKind::Paragraph && present.contains(&e.node))
            {
                out.push(next.node);
            }
        }
    }
    out
}
