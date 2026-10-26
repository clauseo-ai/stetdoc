//! Resolving paragraph marks (design `revisions.md`, 3.2 and 6.2). A mark revision that does not remove
//! the mark (an accepted insertion, a rejected deletion) is just removed. One that removes it, an
//! accepted deletion or a rejected insertion, joins the paragraph with the next one, last to first, so
//! that a run of marks collapses into the first paragraph of the run.

use super::cleanup::{is_attached, prune_from};
use super::join::{Joined, MARKERS, Why, join, leftovers};
use super::names::{is_blank, is_w};
use super::resolve::{Engine, Removed};
use super::tree_ops::{detach, parent_of};
use crate::api::error::StetError;
use crate::state::Retired;
use crate::wordml::BlockKey;
use crate::wordml::NodeId;
use crate::xml::SpanTree;

/// The paragraph after `para` in its container, if the next block is one. Range markers between blocks
/// are passed.
pub(super) fn next_paragraph(tree: &SpanTree, para: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(para).next_sibling();
    while let Some(c) = cur {
        if !c.is_element() || is_blank(&c) {
            cur = c.next_sibling();
            continue;
        }
        if is_w(&c, "p") {
            return Some(c.id());
        }
        if MARKERS.iter().any(|m| is_w(&c, m)) || is_w(&c, "proofErr") {
            cur = c.next_sibling();
            continue;
        }
        return None;
    }
    None
}

/// The paragraph before `para` in its container, if the block before it is one. Range markers between
/// blocks are passed.
pub(super) fn previous_paragraph(tree: &SpanTree, para: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(para).prev_sibling();
    while let Some(c) = cur {
        if !c.is_element() || is_blank(&c) {
            cur = c.prev_sibling();
            continue;
        }
        if is_w(&c, "p") {
            return Some(c.id());
        }
        if MARKERS.iter().any(|m| is_w(&c, m)) || is_w(&c, "proofErr") {
            cur = c.prev_sibling();
            continue;
        }
        return None;
    }
    None
}

impl Engine<'_, '_> {
    /// A revision of a paragraph's mark: `el` is the `w:ins`, `w:del`, `w:moveFrom` or `w:moveTo` in
    /// `w:pPr/w:rPr`.
    pub(super) fn resolve_mark(&mut self, el: NodeId, inserted: bool) -> Result<(), StetError> {
        let removes_mark = inserted != self.accept;
        let tree = self.tx.part_mut(&self.part)?;
        let Some(rpr) = parent_of(tree, el) else {
            return Ok(());
        };
        let para = parent_of(tree, rpr).and_then(|ppr| parent_of(tree, ppr));
        let Some(para) = para else { return Ok(()) };
        if removes_mark {
            if !self.joins.contains(&para) {
                self.joins.push(para);
            }
        } else {
            detach(tree, el)?;
            prune_from(tree, rpr)?;
            self.dirty.push(para);
        }
        Ok(())
    }

    /// Joins every paragraph whose mark is to go with the next one, in the order they were queued, which
    /// is last to first.
    pub(super) fn run_joins(&mut self) -> Result<(), StetError> {
        let joins = std::mem::take(&mut self.joins);
        for para in joins {
            self.join_mark(para)?;
        }
        Ok(())
    }

    fn join_mark(&mut self, para: NodeId) -> Result<(), StetError> {
        let part = self.part.clone();
        let tree = self.tx.part_mut(&part)?;
        if !is_attached(tree, para) {
            return Ok(());
        }
        let key = BlockKey::new(part.clone(), para);
        let next = next_paragraph(tree, para).filter(|n| !self.gap_nexts.contains(n));
        let Some(next) = next else {
            // A rejected insertion of the mark that ends its container (a cell, a note, a comment, the body): Word
            // keeps the paragraph, emptied of what the rejection takes out, since the container must end with one,
            // and so does LibreOffice (design 3.2.1). `wordml`'s `Original` reads it so.
            // When only such paragraphs stand between it and a table (or another block that is not a paragraph), the
            // container ends with that block once they go, as it did before they were inserted, and Word gives a story a
            // last paragraph of its own after a table anyway: it goes as below, when it is empty.
            if !self.accept
                && crate::wordml::ends_container(tree, para)
                && !run_after_block(tree, para)
                && !goes_whole(tree, para)
            {
                strip_mark_revisions(tree, para)?;
                self.dirty.push(para);
                return Ok(());
            }
            // Nothing to join into. A paragraph with nothing but range markers and comment references
            // goes, and these move to the end of the paragraph before it, if there is one. One with
            // content keeps its place and loses the revision of its mark.
            match (leftovers(tree, para), previous_paragraph(tree, para)) {
                (Some(left), Some(prev)) => {
                    // What stands between the two goes before what was in the paragraph, as it was.
                    let gap = super::join::markers_between(tree, prev, para);
                    for n in gap.into_iter().chain(left) {
                        super::tree_ops::move_to(tree, n, prev, None)?;
                    }
                    detach(tree, para)?;
                    self.dirty.push(prev);
                    self.retire_removed(&key)?;
                }
                (Some(left), None)
                    if left.is_empty() && (self.accept || !only_block_of_note(tree, para)) =>
                {
                    let parent = parent_of(tree, para);
                    detach(tree, para)?;
                    self.dirty.extend(parent);
                    self.retire_removed(&key)?;
                }
                // Markers with no paragraph before them go to the start of the paragraph after, past a gap that a removed
                // table left, rather than keep an empty paragraph for themselves.
                (Some(left), None) if next_paragraph(tree, para).is_some() => {
                    let after = next_paragraph(tree, para).unwrap_or(para);
                    let gap = super::join::markers_between(tree, para, after);
                    super::join::place_leftovers(tree, para, left, after, gap)?;
                    detach(tree, para)?;
                    self.dirty.push(after);
                    self.retire_removed(&key)?;
                }
                _ => {
                    strip_mark_revisions(tree, para)?;
                    self.dirty.push(para);
                }
            }
            return Ok(());
        };
        let second_empty = leftovers(tree, next).is_some();
        let why = if !self.accept && second_empty {
            Why::SecondRejected
        } else {
            Why::Merge
        };
        let second = BlockKey::new(part.clone(), next);
        match join(self.tx, &part, para, next, why)? {
            Joined::FirstRemoved => {
                if why == Why::SecondRejected {
                    self.inherited.push((key.clone(), second.clone()));
                }
                self.retire_removed(&key)?;
            }
            Joined::Merged => {
                self.joined.push((key.clone(), second.clone()));
                if why == Why::SecondRejected {
                    self.removed.push(Removed {
                        key: second,
                        rejected: true,
                    });
                }
                self.dirty.push(para);
            }
        }
        Ok(())
    }

    /// A paragraph that left the tree: a rejected insertion declares it, an accepted deletion does not.
    fn retire_removed(&mut self, key: &BlockKey) -> Result<(), StetError> {
        self.removed.push(Removed {
            key: key.clone(),
            rejected: !self.accept,
        });
        if !self.accept && self.tx.parts().ref_of(key).is_some() {
            self.tx.retire(key, Retired::Rejected)?;
        }
        Ok(())
    }
}

/// True if the block before `para` and the paragraphs right before it whose marks are inserted, which rejecting joins into it,
/// is not a paragraph (a table, a content control, a wrapper of blocks).
fn run_after_block(tree: &SpanTree, para: NodeId) -> bool {
    let mut cur = para;
    loop {
        let Some(prev) = previous_paragraph(tree, cur) else {
            // No paragraph right before: a block that is not one, or nothing (then the container keeps this one).
            return follows_a_block(tree, cur);
        };
        let mark = super::blocks::mark_of(tree, prev);
        if mark.ins.is_none() && mark.move_to.is_none() {
            return false;
        }
        cur = prev;
    }
}

/// True if the block right before `para` in its parent is not a paragraph (a table, a content control, a wrapper of
/// blocks). With nothing before it, false.
pub(super) fn follows_a_block(tree: &SpanTree, para: NodeId) -> bool {
    previous_paragraph(tree, para).is_none() && previous_block(tree, para)
}

/// True if a block element stands before `para` in its parent, past range markers.
fn previous_block(tree: &SpanTree, para: NodeId) -> bool {
    let mut cur = tree.node_ref(para).prev_sibling();
    while let Some(c) = cur {
        if c.is_element()
            && !is_blank(&c)
            && !MARKERS.iter().any(|m| is_w(&c, m))
            && !is_w(&c, "proofErr")
            && !is_w(&c, "pPr")
            && !is_w(&c, "tcPr")
            && !is_w(&c, "sdtPr")
            && !is_w(&c, "sdtEndPr")
        {
            return true;
        }
        cur = c.prev_sibling();
    }
    false
}

/// True if `para` is the only block of a note or a comment. A rejection keeps it, emptied of what it takes out, as it keeps the
/// last paragraph of any container (design 3.2.1), even when all the note's paragraphs were inserted: a note goes only with its
/// reference (3.6.4), so when the reference goes too the note goes with it, and when the reference stays the note must keep a
/// block (`check`'s W9).
fn only_block_of_note(tree: &SpanTree, para: NodeId) -> bool {
    let Some(parent) = tree.node_ref(para).parent() else {
        return false;
    };
    (is_w(&parent, "footnote") || is_w(&parent, "endnote") || is_w(&parent, "comment"))
        && parent
            .child_elements()
            .filter(|c| {
                ["p", "tbl", "sdt", "customXml", "altChunk"]
                    .iter()
                    .any(|b| is_w(c, b))
            })
            .count()
            == 1
}

/// True if rejecting removes the paragraph's whole container with it: it is in an inserted row or cell, or in a note or
/// comment whose paragraphs are all inserted (a new note goes with its reference). The resolver, the writer of compare's end
/// marks and the laws' readings (`laws_util::runs_in`) all ask this one question.
pub(super) fn goes_whole(tree: &SpanTree, para: NodeId) -> bool {
    let mut cur = tree.node_ref(para).parent();
    while let Some(c) = cur {
        // An inserted row (`w:trPr/w:ins`) or an inserted cell (`w:tcPr/w:cellIns`).
        let marked = |holder: &str, props: &str, kind: &str| {
            is_w(&c, holder)
                && c.child_elements()
                    .find(|x| is_w(x, props))
                    .is_some_and(|p| p.child_elements().any(|x| is_w(&x, kind)))
        };
        if marked("tr", "trPr", "ins") || marked("tc", "tcPr", "cellIns") {
            return true;
        }
        if is_w(&c, "footnote") || is_w(&c, "endnote") || is_w(&c, "comment") {
            let mut all = true;
            let mut stack = vec![c];
            while let Some(e) = stack.pop() {
                if is_w(&e, "p") {
                    all &= super::blocks::mark_of(tree, e.id()).ins.is_some();
                    continue;
                }
                stack.extend(e.child_elements());
            }
            return all;
        }
        cur = c.parent();
    }
    false
}

/// Removes the revisions of a paragraph's mark: the `w:ins`, `w:del`, `w:moveFrom` and `w:moveTo` in its
/// `w:pPr/w:rPr`.
fn strip_mark_revisions(tree: &mut SpanTree, para: NodeId) -> Result<(), StetError> {
    let Some(ppr) = tree
        .node_ref(para)
        .child_elements()
        .find(|c| is_w(c, "pPr"))
    else {
        return Ok(());
    };
    let Some(rpr) = ppr.child_elements().find(|c| is_w(c, "rPr")) else {
        return Ok(());
    };
    let rpr = rpr.id();
    let doomed: Vec<NodeId> = tree
        .node_ref(rpr)
        .child_elements()
        .filter(|c| is_w(c, "ins") || is_w(c, "del") || is_w(c, "moveFrom") || is_w(c, "moveTo"))
        .map(|c| c.id())
        .collect();
    for d in doomed {
        detach(tree, d)?;
    }
    prune_from(tree, rpr)?;
    Ok(())
}
