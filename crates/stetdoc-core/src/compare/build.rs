//! The redline: the plan turned into calls of the compiler (design `compare.md`, 6).
//!
//! One tracked compiler writes the whole redline on the revised state's transaction, entry by entry in the order of the plan:
//! an inserted run of blocks is marked inserted, a deleted run is imported and placed beside the revised block it goes next to,
//! a paired paragraph gets its edits, and a split or a merge is cut first and edited after. The compiler's refusals
//! never fail the comparison: a paragraph that cannot be edited in place is replaced whole (its original deleted, the revised
//! one inserted), and a block that cannot be written at all is left as the revised document has it and listed in the receipt as an
//! exclusion from the compare laws.

use super::import::{Cx, Importer};
use super::plan::{Entry, Plan, StoryPlan};
use super::prepare::Prepared;
use super::receipt::{CompareReceipt, CompareWarning, MoveKeys};
use crate::api::error::{ErrorCode, StetError};
use crate::revisions::compiler::Compiler;
use crate::revisions::outcome::Effect;
use crate::revisions::redline_types::Imported;
use crate::revisions::stamp::{RevStamp, Track};
use crate::revisions::types::Place;
use crate::state::Tx;
use crate::wordml::{BlockKey, Mode, NodeId, Paragraph};
use std::collections::HashMap;
use std::sync::Arc;

/// A redline on an open transaction, and what to tell about it.
pub(crate) struct Redline<'p> {
    /// The transaction on the revised state, with every revision written.
    pub tx: Tx<'p>,
    /// What the build did.
    pub receipt: CompareReceipt,
}

/// The writer of one redline.
pub(super) struct Builder<'a, 'o, 's> {
    pub(super) c: Compiler<'a, 's>,
    pub(super) imp: Importer<'o>,
    pub(super) rc: CompareReceipt,
    /// The deleted copy in the redline of each original block that was placed, by (story index, original node).
    copied: HashMap<(usize, NodeId), BlockKey>,
    /// The blocks of the redline that were marked inserted, by (story index, revised node).
    inserted: HashMap<(usize, NodeId), BlockKey>,
    /// The tables of this story that took deleted copies of original rows (`build_tables`).
    pub(super) copied_rows: Vec<super::build_tables::CopiedRows>,
    /// The original document.
    pub(super) original: &'o crate::state::State,
    /// The story being written.
    story: usize,
}

/// Builds the redline of a prepared pair under `stamp`.
pub(crate) fn redline<'p>(
    prepared: &'p Prepared,
    stamp: &RevStamp,
) -> Result<Redline<'p>, StetError> {
    let mut atoms = super::model::Atoms::default();
    let pairings =
        super::stories::pair_documents(&prepared.original, &prepared.revised, &mut atoms)?;
    let lists = super::import::ListMatch::new(&prepared.original, &prepared.revised)?;
    let plan = super::plan::plan(&pairings, &prepared.original, &prepared.revised, &|a, b| {
        lists.same(a, b)
    })?;
    let mut tx = prepared.revised.begin();
    let receipt = {
        let c = Compiler::new(&mut tx, Track::Tracked(stamp.clone()))?;
        let mut b = Builder {
            c,
            imp: Importer::new(&prepared.original),
            rc: CompareReceipt {
                accepted: prepared.accepted,
                ..CompareReceipt::default()
            },
            copied: HashMap::new(),
            inserted: HashMap::new(),
            copied_rows: Vec::new(),
            original: &prepared.original,
            story: 0,
        };
        for (i, sp) in plan.stories.iter().enumerate() {
            b.story = i;
            b.write_story(sp)?;
        }
        b.finish_moves(&plan, &prepared.original);
        b.finish_exclusions(&prepared.original);
        b.into_receipt(&plan)?
    };
    // a call that failed after it had written leaves a transaction no one can vouch for: it is never committed
    if receipt.broken > 0 {
        let first = receipt
            .exclusions
            .iter()
            .find(|e| e.reason.starts_with("a call failed"))
            .map(|e| e.reason.clone())
            .unwrap_or_default();
        return Err(StetError::new(
            ErrorCode::Internal,
            format!(
                "{} calls of the compiler failed after they had written: {first}",
                receipt.broken
            ),
        ));
    }
    Ok(Redline { tx, receipt })
}

impl Builder<'_, '_, '_> {
    /// Records what a call declared.
    pub(super) fn record(&mut self, fx: &Effect) {
        self.rc.declared.origins.extend(fx.origins.iter().cloned());
        self.rc.declared.imports.extend(fx.imported.iter().cloned());
        self.rc.declared.touched.extend(fx.touched.iter().cloned());
    }

    /// Notes a call that failed after it had written something: a defect, which makes the comparison fail.
    #[track_caller]
    pub(super) fn broken(&mut self, block: BlockKey, e: &StetError) {
        let at = std::panic::Location::caller();
        self.rc.broken += 1;
        self.exclude(
            vec![block],
            Vec::new(),
            "unredlined",
            format!("a call failed half way ({}:{}): {e}", at.file(), at.line()),
        );
    }

    /// What the import could not carry so far: each is a block that rejecting cannot give back as it was.
    fn losses(&self) -> [usize; 4] {
        let s = &self.imp.stats;
        [
            s.math_dropped,
            s.objects_dropped,
            s.notes_dropped,
            s.lists_dropped,
        ]
    }

    /// The blocks of the redline and of the original that an entry is about, for an exclusion: the revised blocks, the copies
    /// that were placed for a deletion, and the original blocks.
    fn entry_blocks(
        &self,
        sp: &StoryPlan,
        e: &Entry,
    ) -> (Vec<BlockKey>, Vec<BlockKey>, Vec<BlockKey>) {
        let (rev, orig): (Vec<NodeId>, Vec<NodeId>) = match e {
            Entry::Edit(pe) => (pe.b.into_iter().collect(), pe.a.into_iter().collect()),
            Entry::Insert { blocks } => (blocks.clone(), Vec::new()),
            Entry::Delete { blocks, .. } => (Vec::new(), blocks.clone()),
            Entry::Split(s) => (s.parts.iter().filter_map(|p| p.b).collect(), vec![s.a]),
            Entry::Merge(m) => (vec![m.b], m.parts.iter().filter_map(|p| p.a).collect()),
        };
        let redline: Vec<BlockKey> = rev.iter().map(|&n| Self::key(&sp.part_b, n)).collect();
        let copies: Vec<BlockKey> = orig
            .iter()
            .filter_map(|n| self.copied.get(&(self.story, *n)).cloned())
            .collect();
        (
            redline,
            copies,
            orig.iter().map(|&n| Self::key(&sp.part_a, n)).collect(),
        )
    }

    fn write_story(&mut self, sp: &StoryPlan) -> Result<(), StetError> {
        for e in &sp.entries {
            let before = self.losses();
            match e {
                Entry::Edit(pe) => self.edit(sp, pe)?,
                Entry::Insert { blocks } => self.insert(sp, blocks)?,
                Entry::Delete {
                    blocks,
                    at,
                    trailing,
                } => self.delete(sp, blocks, at.anchor, at.before, *trailing)?,
                Entry::Split(s) => self.split(sp, s)?,
                Entry::Merge(m) => self.merge(sp, m)?,
            }
            self.flush_notes()?;
            let after = self.losses();
            if after != before {
                let (redline, copies, original) = self.entry_blocks(sp, e);
                let names = [
                    "an equation",
                    "a picture or object whose part has parts of its own",
                    "a note",
                    "a list",
                ];
                let what: Vec<&str> = names
                    .iter()
                    .zip(before.iter().zip(&after))
                    .filter(|(_, (b, a))| a > b)
                    .map(|(n, _)| *n)
                    .collect();
                self.exclude_all(
                    redline,
                    copies,
                    original,
                    "not_restored",
                    format!(
                        "{} could not be carried into the deleted text",
                        what.join(" and ")
                    ),
                );
            }
        }
        // The rows copied from an original table count their columns in its grid (`fit_grid`).
        self.fit_grids();
        // A container that ends with inserted paragraphs gets a live last mark, as Word needs (`settle_end_marks`).
        match self.c.settle_end_marks(&sp.part_b) {
            Ok(done) => {
                self.record(&done.effect);
                self.follow_end_marks(&sp.part_b, &done.given);
                for z in done.orphans {
                    self.exclude(
                        vec![Self::key(&sp.part_b, z)],
                        Vec::new(),
                        "not_restored",
                        "the paragraph ends its container with an inserted mark that the block before it cannot take: Word keeps it, emptied, when its insertion is rejected",
                    );
                }
            }
            Err(e) => {
                let at = sp.entries.iter().rev().find_map(|en| match en {
                    Entry::Insert { blocks } => blocks.last().copied(),
                    _ => None,
                });
                match at {
                    Some(n) => self.broken(Self::key(&sp.part_b, n), &e),
                    None => return Err(e),
                }
            }
        }
        Ok(())
    }

    pub(super) fn key(part: &Arc<str>, n: NodeId) -> BlockKey {
        BlockKey::new(Arc::clone(part), n)
    }

    fn insert(&mut self, sp: &StoryPlan, blocks: &[NodeId]) -> Result<(), StetError> {
        let keys: Vec<BlockKey> = blocks.iter().map(|&n| Self::key(&sp.part_b, n)).collect();
        for (i, k) in keys.iter().enumerate() {
            // rejecting the insertion of the paragraphs before it joins them to the one that stays
            let before: Vec<BlockKey> = keys.iter().take(i).cloned().collect();
            if self.ends_section(k) {
                self.exclude_all(vec![k.clone()], before.clone(), Vec::new(), "not_restored", "the paragraph ends a section: rejecting its insertion leaves an empty paragraph with the break");
            }
            if self.trailing_with_markers(k) {
                self.exclude_all(vec![k.clone()], before, Vec::new(), "not_restored", "the paragraph ends its container and holds a comment's reference, which stays when its insertion is rejected and keeps it");
            }
            if self.holds_nested_paragraph(k) {
                self.exclude(
                    vec![k.clone()],
                    Vec::new(),
                    "not_restored",
                    "the paragraph holds another paragraph, which no tracked change can carry",
                );
            }
        }
        self.mark_inserted_blocks(blocks.iter().copied().zip(keys).collect());
        Ok(())
    }

    /// Marks revised blocks inserted, consecutive ones in one call. A paragraph with an equation has its content marked
    /// inserted and keeps its mark: the equation stays when the insertion is rejected, and a mark inserted would join it to the
    /// paragraph after it. Returns false if a call broke.
    fn mark_inserted_blocks(&mut self, blocks: Vec<(NodeId, BlockKey)>) -> bool {
        let mut runs: Vec<Vec<(NodeId, BlockKey)>> = vec![Vec::new()];
        let mut ok = true;
        for (n, k) in blocks {
            if self.holds_math(&k) {
                self.exclude(
                    vec![k.clone()],
                    Vec::new(),
                    "not_restored",
                    "the paragraph holds an equation, which a tracked change cannot carry, and stays when its insertion is rejected",
                );
                let done = if self.is_paragraph(&k) {
                    self.c.mark_content_inserted(std::slice::from_ref(&k))
                } else {
                    self.c.mark_inserted(std::slice::from_ref(&k))
                };
                match done {
                    Ok(fx) => {
                        self.record(&fx);
                        self.rc.counts.inserted += 1;
                        self.inserted.insert((self.story, n), k);
                    }
                    Err(e) => {
                        ok = false;
                        self.broken(k, &e);
                    }
                }
                runs.push(Vec::new());
            } else if let Some(last) = runs.last_mut() {
                last.push((n, k));
            }
        }
        for run in runs.into_iter().filter(|r| !r.is_empty()) {
            let keys: Vec<BlockKey> = run.iter().map(|(_, k)| k.clone()).collect();
            match self.c.mark_inserted(&keys) {
                Ok(fx) => {
                    self.record(&fx);
                    self.rc.counts.inserted += keys.len();
                    for (n, k) in run {
                        self.inserted.insert((self.story, n), k);
                    }
                }
                Err(e) => {
                    ok = false;
                    for k in keys {
                        self.broken(k, &e);
                    }
                }
            }
        }
        ok
    }

    fn delete(
        &mut self,
        sp: &StoryPlan,
        blocks: &[NodeId],
        anchor: NodeId,
        before: bool,
        trailing: Option<NodeId>,
    ) -> Result<(), StetError> {
        let cx = Cx {
            src: &sp.part_a,
            dst: &sp.part_b,
        };
        let anchor_key = Self::key(&sp.part_b, anchor);
        let mut placed: Vec<Imported> = Vec::new();
        let mut originals: Vec<NodeId> = Vec::new();
        for &n in blocks {
            match self.imp.block(&mut self.c.import_writer(), cx, n) {
                Ok(copy) => {
                    placed.push(Imported {
                        node: copy.node,
                        source: copy.source,
                    });
                    originals.push(n);
                }
                Err(e) => self.exclude_beside(
                    anchor_key.clone(),
                    vec![Self::key(&sp.part_a, n)],
                    "not_restored",
                    format!("a block of the original could not be copied: {e}"),
                ),
            }
        }
        if placed.is_empty() {
            return Ok(());
        }
        let place = if before { Place::Before } else { Place::After };
        let trailing_key = trailing.map(|t| Self::key(&sp.part_b, t));
        match self
            .c
            .place_deleted(&anchor_key, place, &placed, trailing_key.as_ref())
        {
            Ok(fx) => {
                self.record(&fx);
                self.rc.counts.deleted += placed.len();
                for (n, k) in originals.iter().zip(&fx.new_blocks) {
                    self.copied.insert((self.story, *n), k.clone());
                }
                let pairs: Vec<(NodeId, BlockKey)> = originals
                    .iter()
                    .copied()
                    .zip(fx.new_blocks.iter().cloned())
                    .collect();
                self.note_copied_rows(sp, &pairs);
            }
            Err(e) => self.broken(anchor_key, &e),
        }
        Ok(())
    }

    /// The comparison text of the original block, for a move's receipt.
    pub(super) fn text_of(original: &crate::state::State, part: &Arc<str>, node: NodeId) -> String {
        let view = original.parts();
        let Some(id) = view.id(part) else {
            return String::new();
        };
        let Ok(tree) = view.tree(id) else {
            return String::new();
        };
        Paragraph::new(tree, node)
            .map(|p| p.text(Mode::Final).chars().take(80).collect())
            .unwrap_or_default()
    }

    fn finish_moves(&mut self, plan: &Plan, original: &crate::state::State) {
        for (i, sp) in plan.stories.iter().enumerate() {
            for &(a, b) in &sp.moves {
                if let (Some(from), Some(to)) =
                    (self.copied.get(&(i, a)), self.inserted.get(&(i, b)))
                {
                    self.rc.moves.push(MoveKeys {
                        from: from.clone(),
                        to: to.clone(),
                        text: Self::text_of(original, &sp.part_a, a),
                    });
                }
            }
        }
    }

    fn into_receipt(self, plan: &Plan) -> Result<CompareReceipt, StetError> {
        let stats = self.imp.stats.clone();
        let outcome = self.c.finish()?;
        let mut rc = self.rc;
        rc.declared.definitions = outcome
            .effects
            .iter()
            .flat_map(|e| e.imported_defs.iter().cloned())
            .collect();
        rc.counts.kept = plan.stories.iter().map(|s| s.kept).sum();
        let mut warn = |code: &'static str, message: &str, count: usize| {
            if count > 0 {
                rc.warnings.push(CompareWarning {
                    code,
                    message: message.to_owned(),
                    count,
                });
            }
        };
        warn(
            "compare_story_unmatched",
            "headers and footers that exist on one side only were not compared",
            plan.unmatched_slots,
        );
        warn(
            "compare_table_replaced",
            "rows whose cells changed shape were written as a deleted row and an inserted row",
            plan.table_replaced,
        );
        warn(
            "compare_comments_dropped",
            "comments in deleted content were not carried",
            stats.comments_dropped,
        );
        warn(
            "compare_bookmarks_dropped",
            "a bookmark of deleted content has a name the revised document uses",
            stats.bookmarks_dropped,
        );
        warn(
            "compare_hyperlinks_unlinked",
            "a hyperlink in deleted content has an address the revised document has no relationship for, so only its text is shown",
            stats.hyperlinks_unlinked,
        );
        warn(
            "compare_objects_dropped",
            "a picture or object in deleted content could not be carried",
            stats.objects_dropped,
        );
        warn(
            "compare_notes_dropped",
            "a note of deleted content could not be carried",
            stats.notes_dropped,
        );
        warn(
            "compare_lists_dropped",
            "a list of deleted content has no numbering part to go in",
            stats.lists_dropped,
        );
        let replaced = rc.exclusions.len();
        warn(
            "compare_not_redlined",
            "blocks that could not be written as changes are shown as the revised document has them",
            replaced,
        );
        rc.stats = stats;
        Ok(rc)
    }

    /// Splits and merges are counted by the entries that write them.
    pub(super) fn count_split(&mut self) {
        self.rc.counts.split += 1;
    }

    /// As above.
    pub(super) fn count_merge(&mut self) {
        self.rc.counts.merged += 1;
    }

    /// Edited paragraphs are counted by the entries that write them.
    pub(super) fn count_edit(&mut self) {
        self.rc.counts.edited += 1;
    }

    /// Writes a deleted copy of each original block in front of the first revised block and marks the revised ones inserted: the
    /// way a paragraph that cannot be edited in place is shown.
    pub(super) fn replace_whole(
        &mut self,
        sp: &StoryPlan,
        originals: &[NodeId],
        revised: &[NodeId],
        reason: &str,
    ) {
        let Some(&first) = revised.first() else {
            return;
        };
        let cx = Cx {
            src: &sp.part_a,
            dst: &sp.part_b,
        };
        let keys: Vec<BlockKey> = revised.iter().map(|&n| Self::key(&sp.part_b, n)).collect();
        let first_key = Self::key(&sp.part_b, first);
        let original_keys: Vec<BlockKey> = originals
            .iter()
            .map(|&n| Self::key(&sp.part_a, n))
            .collect();
        let mut copies: Vec<Imported> = Vec::new();
        for &a in originals {
            match self.imp.block(&mut self.c.import_writer(), cx, a) {
                Ok(c) => copies.push(Imported {
                    node: c.node,
                    source: c.source,
                }),
                Err(e) => {
                    self.exclude(
                        keys,
                        original_keys,
                        "unredlined",
                        format!("{reason}; and the original could not be copied: {e}"),
                    );
                    return;
                }
            }
        }
        // the deletion first: `place_deleted` refuses before it writes, and then the revised paragraph is left as it is
        if !copies.is_empty() {
            match self
                .c
                .place_deleted(&first_key, Place::Before, &copies, None)
            {
                Ok(fx) => self.record(&fx),
                Err(e) => {
                    self.exclude(
                        keys,
                        original_keys,
                        "unredlined",
                        format!("{reason}; and the original was not placed: {e}"),
                    );
                    return;
                }
            }
        }
        if !self.mark_inserted_blocks(revised.iter().copied().zip(keys).collect()) {
            return;
        }
        self.rc.counts.deleted += copies.len();
        self.rc.warnings_replaced(reason);
    }
}

impl CompareReceipt {
    fn warnings_replaced(&mut self, reason: &str) {
        match self
            .warnings
            .iter_mut()
            .find(|w| w.code == "compare_paragraph_replaced")
        {
            Some(w) => w.count += 1,
            None => self.warnings.push(CompareWarning {
                code: "compare_paragraph_replaced",
                message: format!("a paragraph that could not be edited in place is shown as deleted and inserted: {reason}"),
                count: 1,
            }),
        }
    }
}
