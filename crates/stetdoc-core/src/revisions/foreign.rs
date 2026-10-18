//! Another author's pending change, which no edit rewrites or hides (design `revisions.md` 4.5 and 5.5, decision 4).
//!
//! Tracked, the compiler records what it does to someone else's change: it nests a deletion inside their insertion, splits their
//! wrapper around new text, and refuses to edit properties under their `*PrChange` (4.3). Untracked, there is no record, so an
//! edit that would reach into another author's pending change would rewrite it silently: text typed inside their insertion would
//! be theirs, a run they inserted could be hidden, properties under their formatting change would keep their snapshot, and a
//! paragraph with their changes could go. So an untracked edit refuses all of that with `readonly_region`, and says to accept or
//! reject the change first. The author's own pending changes are not another author's: untracked edits them in place. An
//! untracked compiler that was given no author (`Track::Untracked`) treats every pending change as another author's.

use super::compiler::{Compiler, ParaView};
use super::errors::readonly;
use super::names::is_w;
use super::props_edit::find_chain;
use super::stamp::Track;
use crate::api::error::StetError;
use crate::wordml::{NodeId, Paragraph, RevKind, Revision, revision_at};
use std::ops::Range;

/// What a revision is, in the words of a refusal.
fn what(kind: RevKind) -> &'static str {
    match kind {
        RevKind::Ins | RevKind::MarkIns | RevKind::NumPrIns => "a pending insertion",
        RevKind::Del | RevKind::MarkDel => "a pending deletion",
        RevKind::MoveFrom | RevKind::MoveTo | RevKind::MarkMoveFrom | RevKind::MarkMoveTo => {
            "a pending move"
        }
        RevKind::RowIns => "a pending row insertion",
        RevKind::RowDel => "a pending row deletion",
        RevKind::CellIns => "a pending cell insertion",
        RevKind::CellDel => "a pending cell deletion",
        RevKind::CellMerge => "a pending cell merge",
        RevKind::NumberingChange => "a pending numbering change",
        RevKind::CustomXmlInsStart
        | RevKind::CustomXmlInsEnd
        | RevKind::CustomXmlDelStart
        | RevKind::CustomXmlDelEnd
        | RevKind::CustomXmlMoveFromStart
        | RevKind::CustomXmlMoveFromEnd
        | RevKind::CustomXmlMoveToStart
        | RevKind::CustomXmlMoveToEnd => "a pending change of a custom XML element",
        RevKind::RunProps
        | RevKind::ParagraphProps
        | RevKind::SectionProps
        | RevKind::TableProps
        | RevKind::TableGrid
        | RevKind::RowProps
        | RevKind::CellProps
        | RevKind::TableExProps => "a pending formatting change",
    }
}

/// The refusal of an untracked edit that would reach into `rev`. `lead` says where it is: "this text is in".
fn refusal(lead: &str, rev: &Revision) -> StetError {
    let by = rev
        .author
        .as_deref()
        .filter(|a| !a.is_empty())
        .map_or_else(|| "an unnamed author".to_string(), str::to_string);
    readonly(format!(
        "{lead} {} by {by}. An untracked edit does not rewrite or hide another author's pending change: accept or reject that change first, or edit something else.",
        what(rev.kind)
    ))
}

/// True for a revision element that names no author because its start does: the end of a custom XML range.
fn is_range_end(kind: RevKind) -> bool {
    matches!(
        kind,
        RevKind::CustomXmlInsEnd
            | RevKind::CustomXmlDelEnd
            | RevKind::CustomXmlMoveFromEnd
            | RevKind::CustomXmlMoveToEnd
    )
}

impl Compiler<'_, '_> {
    /// The author whose changes are the edit's own: the stamp's when tracked, the one an untracked compiler was given, or none.
    pub(super) fn author(&self) -> Option<&str> {
        match &self.track {
            Track::Tracked(stamp) => Some(stamp.author()),
            Track::UntrackedBy(author) => Some(author.as_str()),
            Track::Untracked => None,
        }
    }

    /// True if a revision with this author is the edit's own. A revision with no author is nobody's.
    pub(super) fn is_own_author(&self, author: Option<&str>) -> bool {
        matches!((self.author(), author), (Some(a), Some(b)) if a == b)
    }

    /// Untracked: refuses when an item of `items` lies in another author's pending change, a wrapper around it (an insertion,
    /// a deletion, a move, an inserted or deleted row or cell) or a formatting change of its run. Tracked edits record what
    /// they do, and pass.
    pub(super) fn refuse_foreign_items(
        &self,
        p: &ParaView,
        items: Range<usize>,
    ) -> Result<(), StetError> {
        if self.tracked() {
            return Ok(());
        }
        let tree = self.tree_ro(&p.part)?;
        let Some(para) = Paragraph::new(tree, p.node) else {
            return Ok(());
        };
        let lead = "this text is in";
        for i in items {
            let Some(item) = p.map.items().get(i) else {
                continue;
            };
            for ctx in p.map.context(item.ctx) {
                if let Some(rev) = para.revision_of(ctx)
                    && !self.is_own_author(rev.author.as_deref())
                {
                    return Err(refusal(lead, &rev));
                }
            }
            if let Some(run) = item.run
                && let Some(rev) = run_change(tree, run)
                && !self.is_own_author(rev.author.as_deref())
            {
                return Err(refusal(lead, &rev));
            }
        }
        Ok(())
    }

    /// Untracked: refuses new text at a gap inside another author's wrapper, where it would be theirs. At the edge of their
    /// wrapper the text goes beside it, so only a gap with the same wrapper on both sides is refused.
    pub(super) fn refuse_foreign_gap(
        &self,
        p: &ParaView,
        left: Option<usize>,
        right: Option<usize>,
    ) -> Result<(), StetError> {
        if self.tracked() {
            return Ok(());
        }
        let (Some(l), Some(r)) = (left, right) else {
            return Ok(());
        };
        let tree = self.tree_ro(&p.part)?;
        let Some(para) = Paragraph::new(tree, p.node) else {
            return Ok(());
        };
        let (Some(li), Some(ri)) = (p.map.items().get(l), p.map.items().get(r)) else {
            return Ok(());
        };
        let right_wrappers: Vec<NodeId> = p.map.context(ri.ctx).map(|c| c.node).collect();
        for ctx in p.map.context(li.ctx) {
            let inside = l == r || right_wrappers.contains(&ctx.node);
            if inside
                && let Some(rev) = para.revision_of(ctx)
                && !self.is_own_author(rev.author.as_deref())
            {
                return Err(refusal("this position is inside", &rev));
            }
        }
        Ok(())
    }

    /// Untracked: refuses when the element `node` or anything inside it is a revision of another author's. For an edit that
    /// removes or rewrites the whole element: a paragraph that goes, a mark that is joined away, a row, properties.
    pub(super) fn refuse_foreign_within(
        &self,
        part: &str,
        node: NodeId,
        lead: &str,
    ) -> Result<(), StetError> {
        if self.tracked() {
            return Ok(());
        }
        let tree = self.tree_ro(part)?;
        let mut stack = vec![tree.node_ref(node)];
        while let Some(n) = stack.pop() {
            if let Some(rev) = revision_at(&n)
                && !is_range_end(rev.kind)
                && !self.is_own_author(rev.author.as_deref())
            {
                return Err(refusal(lead, &rev));
            }
            stack.extend(n.child_elements());
        }
        Ok(())
    }

    /// Untracked: refuses when the container at `path` below `owner` holds another author's revision, a `*PrChange` or a mark,
    /// row or cell revision among its children.
    pub(super) fn refuse_foreign_container(
        &self,
        part: &str,
        owner: NodeId,
        path: &[&str],
        lead: &str,
    ) -> Result<(), StetError> {
        if self.tracked() {
            return Ok(());
        }
        let tree = self.tree_ro(part)?;
        let Some(container) = find_chain(tree, owner, path) else {
            return Ok(());
        };
        for child in tree.node_ref(container).child_elements() {
            if let Some(rev) = revision_at(&child)
                && !is_range_end(rev.kind)
                && !self.is_own_author(rev.author.as_deref())
            {
                return Err(refusal(lead, &rev));
            }
        }
        Ok(())
    }
}

/// The formatting change of a run, if its properties hold one.
fn run_change(tree: &crate::xml::SpanTree, run: NodeId) -> Option<Revision> {
    let rpr = find_chain(tree, run, &["rPr"])?;
    let change = tree
        .node_ref(rpr)
        .child_elements()
        .find(|c| is_w(c, "rPrChange"))?;
    revision_at(&change)
}
