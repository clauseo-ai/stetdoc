//! Cutting runs and finding what a range of text covers (design `revisions.md`, 2.2 step 3).
//!
//! A run is cut by making a new run after it and moving the later children into it. The **original run
//! is always the left part**, and the original text node keeps the start of its text, so everything
//! to the left of a cut, which the one text map of a call still points at, stays valid. A cut also
//! copies the run's properties by the copy rule, with one exception: a pending `w:rPrChange` is copied
//! with it under a new id, because the two halves are still one revision, and the pair of ids is
//! reported as a split.

use super::compiler::{Compiler, ParaView};
use super::errors::{internal, unsupported};
use super::names::{contains, is_blank, is_w, next_content, rev_id, wns};
use super::tree_ops::{
    child_ids, detach, ensure_preserve, insert, move_to, next_sibling_of, parent_of, rpr_of,
    set_w_attr,
};
use crate::api::error::StetError;
use crate::wordml::{ItemKind, Located, Mode, NodeId, Span, TextMap, Vis};
use crate::xml::{NodeKind, SpanTree};
use std::sync::Arc;

/// Where a run is cut.
#[derive(Clone, Copy, Debug)]
pub(super) enum RunCut {
    /// Inside a text node: the characters from `at` on move to the new run.
    InText {
        /// The text node, a child of a `w:t` or a `w:delText`.
        text: NodeId,
        /// How many characters stay.
        at: usize,
    },
    /// Before this child of the run: it and every later child move to the new run.
    Before(NodeId),
}

/// What a range of text covers once the runs at its ends are cut: the units to delete, and where each
/// came from.
pub(super) struct Carved {
    /// The covered units in document order, each with the index of the item it came from. A unit is a
    /// run, or an element that stands outside any run (a range marker, an equation).
    pub units: Vec<(NodeId, usize)>,
}

impl Compiler<'_, '_> {
    /// Cuts the run `run` of `part`: what is before the cut stays, and the rest moves to a new run that
    /// is placed right after it. Returns the new run.
    pub(super) fn split_run(
        &mut self,
        part: &Arc<str>,
        run: NodeId,
        cut: RunCut,
    ) -> Result<NodeId, StetError> {
        // What the cut needs to know, read before anything changes.
        let (wns, change_id) = {
            let tree = self.tree_ro(part)?;
            let rpr = rpr_of(tree, run);
            let change = rpr.and_then(|r| {
                tree.node_ref(r)
                    .child_elements()
                    .find(|c| is_w(c, "rPrChange"))
            });
            (wns(tree), change.and_then(|c| rev_id(&c)))
        };
        let new_change_id = if change_id.is_some() {
            Some(self.next_id()?)
        } else {
            None
        };
        let tree = self.tx.part_mut(part)?;
        let first_moved = match cut {
            RunCut::Before(child) => child,
            RunCut::InText { text, at } => cut_text(tree, run, text, at)?,
        };
        // The new run: a copy of the run with its properties and no other child.
        let right = tree.edit().clone_subtree(run)?;
        let rpr_copy = rpr_of(tree, right);
        for child in child_ids(tree, right) {
            if Some(child) != rpr_copy {
                detach(tree, child)?;
            }
        }
        let moved: Vec<NodeId> = {
            let n = tree.node_ref(run);
            let mut found = Vec::new();
            let mut on = false;
            for c in n.children() {
                if c.id() == first_moved {
                    on = true;
                }
                if on {
                    found.push(c.id());
                }
            }
            if !on {
                return Err(internal("a run was cut before a child it does not have"));
            }
            found
        };
        for child in moved {
            move_to(tree, child, right, None)?;
        }
        let parent = parent_of(tree, run).ok_or_else(|| internal("a run to cut has no parent"))?;
        let next = next_sibling_of(tree, run);
        insert(tree, parent, next, right)?;
        if let (Some(new_id), Some(old_id)) = (new_change_id, change_id) {
            if let Some(change) = rpr_copy.and_then(|r| {
                tree.node_ref(r)
                    .child_elements()
                    .find(|c| is_w(c, "rPrChange"))
                    .map(|c| c.id())
            }) {
                set_w_attr(tree, change, &wns, "id", &new_id.to_string())?;
            }
            self.note_split(old_id, i64::from(new_id))?;
        }
        Ok(right)
    }

    /// Cuts the paragraph's runs at both ends of the items that `loc` covers, so that whole runs cover
    /// them, and returns the units that do. The cut at the end is made first, so the cut at the start
    /// still finds the text it counted.
    pub(super) fn carve(&mut self, p: &ParaView, loc: &Located) -> Result<Carved, StetError> {
        if loc.spans.is_empty() && loc.items.is_empty() {
            return Ok(Carved { units: Vec::new() });
        }
        let items = p.map.items();
        let (i0, i1) = (loc.items.start, loc.items.end.saturating_sub(1));
        let (Some(first_item), Some(last_item)) = (items.get(i0).copied(), items.get(i1).copied())
        else {
            return Err(internal(
                "a covered range names an item that the map does not have",
            ));
        };
        let span_of = |i: usize| loc.spans.iter().find(|s| s.item == i).copied();
        self.check_cuttable(
            p,
            [
                (&first_item, span_of(i0), true),
                (&last_item, span_of(i1), false),
            ],
        )?;
        // The end first: what follows the covered part moves out of the run.
        if let Some(run) = last_item.run {
            let cut = self.end_cut(&p.part, &last_item, span_of(i1), run)?;
            if let Some(cut) = cut {
                self.split_run(&p.part, run, cut)?;
            }
        }
        // Then the start: what precedes the covered part stays, and the covered part is a new run.
        let mut first_unit: Option<NodeId> = None;
        if let Some(run) = first_item.run {
            let cut = self.start_cut(&p.part, &first_item, span_of(i0), run)?;
            if let Some(cut) = cut {
                first_unit = Some(self.split_run(&p.part, run, cut)?);
            }
        }
        // The units, from the items between the two ends. An item the mode excludes is not covered: it is
        // already deleted, and what wraps it is its own revision.
        let tree = self.tree_ro(&p.part)?;
        let mut units: Vec<(NodeId, usize)> = Vec::new();
        for i in loc.items.clone() {
            let Some(item) = items.get(i) else { continue };
            if !item.vis.in_mode(Mode::Final) {
                continue;
            }
            let unit = match (i == i0, first_unit) {
                (true, Some(unit)) => unit,
                _ => unit_of(tree, item.node),
            };
            if units.last().map(|u| u.0) != Some(unit) {
                units.push((unit, i));
            }
        }
        Ok(Carved { units })
    }

    /// The checks that `carve` makes before it cuts anything, for a caller that must know that the call will go through.
    pub(super) fn check_carve(&self, p: &ParaView, loc: &Located) -> Result<(), StetError> {
        if loc.spans.is_empty() && loc.items.is_empty() {
            return Ok(());
        }
        let items = p.map.items();
        let (i0, i1) = (loc.items.start, loc.items.end.saturating_sub(1));
        let (Some(first_item), Some(last_item)) = (items.get(i0).copied(), items.get(i1).copied())
        else {
            return Err(internal(
                "a covered range names an item that the map does not have",
            ));
        };
        let span_of = |i: usize| loc.spans.iter().find(|s| s.item == i).copied();
        self.check_cuttable(
            p,
            [
                (&first_item, span_of(i0), true),
                (&last_item, span_of(i1), false),
            ],
        )
    }

    /// The text nodes that a cut would divide must be plain text, and the `w:t` that holds each must
    /// hold only it. A node that is covered from its first character to its last is not cut.
    fn check_cuttable(
        &self,
        p: &ParaView,
        ends: [(&crate::wordml::Item, Option<Span>, bool); 2],
    ) -> Result<(), StetError> {
        let tree = self.tree_ro(&p.part)?;
        for (item, span, is_start) in ends {
            let Some(span) = span.filter(|_| item.kind == ItemKind::Text) else {
                continue;
            };
            let n = tree.node_ref(item.node);
            let cut_needed = if is_start {
                span.start > 0
            } else {
                (span.end as usize) < n.text().chars().count()
            };
            if !cut_needed {
                continue;
            }
            if n.kind() == NodeKind::CData {
                return Err(unsupported(
                    "this text is in a CDATA section, which cannot be cut in place",
                ));
            }
        }
        Ok(())
    }

    /// The cut that ends the covered part at the end of the last item, if the run holds anything after it.
    fn end_cut(
        &self,
        part: &Arc<str>,
        item: &crate::wordml::Item,
        span: Option<Span>,
        run: NodeId,
    ) -> Result<Option<RunCut>, StetError> {
        let tree = self.tree_ro(part)?;
        let n = tree.node_ref(item.node);
        if item.kind == ItemKind::Text {
            let Some(span) = span else { return Ok(None) };
            let end = span.end as usize;
            if end < n.text().chars().count() || more_text_beside(&n, true) {
                return Ok(Some(RunCut::InText {
                    text: item.node,
                    at: end,
                }));
            }
            let holder = n
                .parent()
                .ok_or_else(|| internal("a text node has no parent"))?;
            return Ok(next_content(&holder).map(|c| RunCut::Before(c.id())));
        }
        if n.parent().map(|x| x.id()) != Some(run) {
            return Ok(None);
        }
        Ok(next_content(&n).map(|c| RunCut::Before(c.id())))
    }

    /// The cut that begins the covered part at the start of the first item, if the run holds anything
    /// before it.
    fn start_cut(
        &self,
        part: &Arc<str>,
        item: &crate::wordml::Item,
        span: Option<Span>,
        run: NodeId,
    ) -> Result<Option<RunCut>, StetError> {
        let tree = self.tree_ro(part)?;
        let n = tree.node_ref(item.node);
        if item.kind == ItemKind::Text {
            let Some(span) = span else { return Ok(None) };
            if span.start > 0 || more_text_beside(&n, false) {
                return Ok(Some(RunCut::InText {
                    text: item.node,
                    at: span.start as usize,
                }));
            }
            let holder = n
                .parent()
                .ok_or_else(|| internal("a text node has no parent"))?;
            return Ok(before_content(&holder).then_some(RunCut::Before(holder.id())));
        }
        if n.parent().map(|x| x.id()) != Some(run) {
            return Ok(None);
        }
        Ok(before_content(&n).then_some(RunCut::Before(n.id())))
    }
}

/// True if a run holds something before `n` other than its properties.
fn before_content(n: &crate::xml::NodeRef<'_>) -> bool {
    let mut cur = n.prev_sibling();
    while let Some(c) = cur {
        if !is_blank(&c) && !is_w(&c, "rPr") {
            return true;
        }
        cur = c.prev_sibling();
    }
    false
}

/// True if the `w:t` that holds the text node `text` holds text after it (`after`) or before it: a `w:t`
/// may hold several text nodes and CDATA sections (`a]]&gt;b<![CDATA[c]]>d`).
pub(super) fn more_text_beside(n: &crate::xml::NodeRef<'_>, after: bool) -> bool {
    let mut cur = if after {
        n.next_sibling()
    } else {
        n.prev_sibling()
    };
    while let Some(c) = cur {
        if matches!(c.kind(), NodeKind::Text | NodeKind::CData) && !c.text().is_empty() {
            return true;
        }
        cur = if after {
            c.next_sibling()
        } else {
            c.prev_sibling()
        };
    }
    false
}

/// Cuts a text node in two inside its run's child, and returns the child that begins the moved part.
/// The original `w:t` and text node keep the start of the text, a new `w:t` holds the rest and every text
/// node or CDATA section after it in the `w:t`, and both say `xml:space="preserve"` where they now need it.
/// The cut may be at the start or the end of the text node when the `w:t` holds text before or after it.
fn cut_text(
    tree: &mut SpanTree,
    run: NodeId,
    text: NodeId,
    at: usize,
) -> Result<NodeId, StetError> {
    let (holder, tail, len, later) = {
        let n = tree.node_ref(text);
        let s = n.text();
        let len = s.chars().count();
        if n.kind() != NodeKind::Text && at > 0 && at < len {
            return Err(unsupported(
                "this text is in a CDATA section, which cannot be cut in place",
            ));
        }
        let holder = n
            .parent()
            .ok_or_else(|| internal("a text node has no parent"))?
            .id();
        let mut later = Vec::new();
        let mut cur = n.next_sibling();
        while let Some(c) = cur {
            later.push(c.id());
            cur = c.next_sibling();
        }
        let edge =
            (at == 0 && !more_text_beside(&n, false)) || (at >= len && !more_text_beside(&n, true));
        if edge {
            return Err(internal("a text element was cut at its start or its end"));
        }
        (holder, s.chars().skip(at).collect::<String>(), len, later)
    };
    if parent_of(tree, holder) != Some(run) {
        return Err(internal(
            "a text element is not a child of the run being cut",
        ));
    }
    let holder2 = tree.edit().clone_subtree(holder)?;
    for child in child_ids(tree, holder2) {
        detach(tree, child)?;
    }
    if at == 0 {
        // The whole node moves, with what follows it.
        move_to(tree, text, holder2, None)?;
    } else if at < len {
        tree.edit().replace_text_range(text, at, len, "")?;
        let tail_node = tree.edit().create_text(&tail)?;
        tree.edit().append_child(holder2, tail_node)?;
    }
    for c in later {
        move_to(tree, c, holder2, None)?;
    }
    let next = next_sibling_of(tree, holder);
    insert(tree, run, next, holder2)?;
    ensure_preserve(tree, holder)?;
    ensure_preserve(tree, holder2)?;
    Ok(holder2)
}

/// The top-level unit an item's node belongs to: the run that holds it, or the node itself when no run
/// does (a range marker, an equation). The climb stops at the paragraph.
pub(super) fn unit_of(tree: &SpanTree, node: NodeId) -> NodeId {
    let mut cur = tree.node_ref(node);
    loop {
        if is_w(&cur, "r") {
            return cur.id();
        }
        match cur.parent() {
            Some(p) if !is_w(&p, "p") => cur = p,
            _ => return node,
        }
    }
}

/// True if a sibling that stands between two units of one group may be wrapped with them: a range
/// marker, a proofing mark, or white space.
pub(super) fn is_transparent(n: &crate::xml::NodeRef<'_>) -> bool {
    if is_blank(n) {
        return true;
    }
    n.is_element()
        && super::names::is_word_ns(n.namespace())
        && matches!(
            n.local_name(),
            "bookmarkStart"
                | "bookmarkEnd"
                | "commentRangeStart"
                | "commentRangeEnd"
                | "permStart"
                | "permEnd"
                | "moveFromRangeStart"
                | "moveFromRangeEnd"
                | "moveToRangeStart"
                | "moveToRangeEnd"
                | "proofErr"
        )
}

/// Groups units into runs of consecutive siblings of one parent, with the transparent siblings
/// between them. Each group is the nodes to wrap or to remove together, in order, and the item index
/// of its first unit.
pub(super) fn groups(tree: &SpanTree, units: &[(NodeId, usize)]) -> Vec<(Vec<NodeId>, usize)> {
    let mut out: Vec<(Vec<NodeId>, usize)> = Vec::new();
    let mut last_unit: Option<NodeId> = None;
    for &(unit, item) in units {
        let n = tree.node_ref(unit);
        let mut joined = false;
        if let (Some(prev), Some(group)) = (last_unit, out.last_mut()) {
            let same_parent =
                tree.node_ref(prev).parent().map(|p| p.id()) == n.parent().map(|p| p.id());
            if same_parent {
                let mut between = Vec::new();
                let mut cur = tree.node_ref(prev).next_sibling();
                let mut ok = false;
                while let Some(c) = cur {
                    if c.id() == unit {
                        ok = true;
                        break;
                    }
                    if !is_transparent(&c) {
                        break;
                    }
                    between.push(c.id());
                    cur = c.next_sibling();
                }
                if ok {
                    group.0.extend(between);
                    group.0.push(unit);
                    joined = true;
                }
            }
        }
        if !joined {
            out.push((vec![unit], item));
        }
        last_unit = Some(unit);
    }
    out
}

/// True if `tree` holds `node` inside `ancestor`.
pub(super) fn inside(tree: &SpanTree, ancestor: NodeId, node: NodeId) -> bool {
    contains(tree, ancestor, node)
}

/// The visibility of an item in the Final text, for the callers that only need to know.
pub(super) fn in_final(v: Vis) -> bool {
    v.in_mode(Mode::Final)
}

/// The map's item at an index, if there is one.
pub(super) fn item_at(map: &TextMap, i: usize) -> Option<crate::wordml::Item> {
    map.items().get(i).copied()
}
