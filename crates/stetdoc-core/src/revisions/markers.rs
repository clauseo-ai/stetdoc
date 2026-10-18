//! `place_markers` and `remove_markers`: range markers for comment anchors and the like (design
//! `revisions.md`, 2.2). They write no revision, whatever the `Track` is, and take no id: the caller
//! mints the annotation ids.
//!
//! The ends of a range are found as for text: the paragraph's runs are cut at both ends (the same
//! code), the start marker goes right before the first covered unit and the end marker right after the
//! last. A marker inside a revision wrapper is a sibling of the runs in it, so no wrapper is split.

use super::compiler::{Compiler, ParaView};
use super::errors::{internal, invalid, readonly};
use super::gap::Boundary;
use super::names::{is_w, rev_id};
use super::outcome::Effect;
use super::text::cover;
use super::tree_ops::{build_element, detach, insert, next_sibling_of, parent_of};
use super::types::MarkerSpec;
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{Mode, NodeId};
use std::ops::Range;
use std::sync::Arc;

/// The local names of the range markers whose ids `remove_markers` matches.
const MARKERS: [&str; 6] = [
    "commentRangeStart",
    "commentRangeEnd",
    "bookmarkStart",
    "bookmarkEnd",
    "permStart",
    "permEnd",
];

/// The elements below `root` that `remove_markers` takes out for `ids`, with the paragraph that holds each:
/// a range marker as it is, and the run that holds a comment reference, whole if it holds nothing else.
fn markers_below(root: &crate::xml::NodeRef<'_>, ids: &[i64]) -> Vec<(NodeId, Option<NodeId>)> {
    let mut found = Vec::new();
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        let wanted = n.id() != root.id() && rev_id(&n).is_some_and(|id| ids.contains(&id));
        if wanted && MARKERS.iter().any(|m| is_w(&n, m)) {
            found.push((n.id(), enclosing_paragraph(&n)));
        } else if wanted && is_w(&n, "commentReference") {
            // The run that holds it goes whole if it holds nothing else.
            let alone = n.parent().is_some_and(|run| {
                is_w(&run, "r")
                    && run
                        .child_elements()
                        .all(|x| x.id() == n.id() || is_w(&x, "rPr"))
            });
            let target = if alone {
                n.parent().map_or_else(|| n.id(), |r| r.id())
            } else {
                n.id()
            };
            found.push((target, enclosing_paragraph(&n)));
        } else {
            // Children in document order, so that what is found is.
            let kids: Vec<_> = n.child_elements().collect();
            stack.extend(kids.into_iter().rev());
        }
    }
    found
}

/// The paragraph that holds an element, if one does.
fn enclosing_paragraph(n: &crate::xml::NodeRef<'_>) -> Option<NodeId> {
    let mut cur = n.parent();
    while let Some(a) = cur {
        if is_w(&a, "p") {
            return Some(a.id());
        }
        cur = a.parent();
    }
    None
}

impl Compiler<'_, '_> {
    /// Puts the start marker of `spec` right before the first character of `range` (a range of the
    /// paragraph's `Final` text), the end marker right after the last, and the reference run, if there is
    /// one, right after the end marker. An empty range puts both at one place.
    pub(crate) fn place_markers(
        &mut self,
        block: &BlockKey,
        range: Range<usize>,
        spec: &MarkerSpec,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.place_markers_inner(block, range, spec) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn place_markers_inner(
        &mut self,
        block: &BlockKey,
        range: Range<usize>,
        spec: &MarkerSpec,
    ) -> Result<(), StetError> {
        let p = self.paragraph(block)?;
        let len = p.map.len(Mode::Final);
        if range.start > range.end || range.end > len {
            return Err(invalid(format!(
                "the range {}..{} is outside the paragraph, which has {len} characters",
                range.start, range.end
            )));
        }
        self.check_editable(&p)?;
        let (start, end) = if range.start < range.end {
            let loc = p
                .map
                .locate(Mode::Final, range)
                .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
            let loc = cover(&p, loc)?;
            self.check_items(&p, loc.items.clone())?;
            let carved = self.carve(&p, &loc)?;
            let (Some(&(first, _)), Some(&(last, _))) = (carved.units.first(), carved.units.last())
            else {
                return Err(internal("a range of text covers nothing"));
            };
            let tree = self.tree_ro(&p.part)?;
            let start = Boundary {
                parent: parent_of(tree, first).ok_or_else(|| internal("a unit has no parent"))?,
                before: Some(first),
            };
            let end = Boundary {
                parent: parent_of(tree, last).ok_or_else(|| internal("a unit has no parent"))?,
                before: next_sibling_of(tree, last),
            };
            (start, end)
        } else {
            let at = self.point_boundary(&p, range.start)?;
            (at, at)
        };
        let part = p.part.clone();
        let tree = self.tx.part_mut(&part)?;
        let end_el = build_element(tree, &spec.end)?;
        let reference = spec
            .reference
            .as_ref()
            .map(|r| build_element(tree, r))
            .transpose()?;
        let start_el = build_element(tree, &spec.start)?;
        if start == end {
            insert(tree, start.parent, start.before, start_el)?;
            insert(tree, end.parent, end.before, end_el)?;
        } else {
            insert(tree, end.parent, end.before, end_el)?;
            insert(tree, start.parent, start.before, start_el)?;
        }
        if let Some(run) = reference {
            insert(tree, end.parent, end.before, run)?;
        }
        self.touch(block);
        Ok(())
    }

    /// The place between the characters at `at - 1` and `at`, for markers: after the left neighbour (the
    /// run is cut if the place is inside it), out of a field that the place is at the end of, and
    /// refused inside a field.
    fn point_boundary(&mut self, p: &ParaView, at: usize) -> Result<Boundary, StetError> {
        let gap = p
            .map
            .gap(Mode::Final, at)
            .ok_or_else(|| invalid("an offset lies outside the paragraph"))?;
        let boundary = match (gap.left, gap.right) {
            (Some(l), Some(r)) if l == r => {
                let item = *p
                    .map
                    .items()
                    .get(l)
                    .ok_or_else(|| internal("a gap names a missing item"))?;
                let start = super::text::final_starts(&p.map)
                    .get(l)
                    .copied()
                    .unwrap_or(0);
                let in_node = item.from as usize + (at - start);
                self.boundary_inside_item(p, l, in_node)?
            }
            (Some(l), _) => self.boundary_after_item(p, l)?,
            (None, _) => self.boundary_at_start(p)?,
        };
        let mut b = boundary;
        let mut ends = 0i64;
        {
            let tree = self.tree_ro(&p.part)?;
            while let Some(next) = b.before {
                let n = tree.node_ref(next);
                let only_field_end = {
                    let mut content = n.child_elements().filter(|c| !is_w(c, "rPr"));
                    is_w(&n, "r")
                        && matches!((content.next(), content.next()), (Some(f), None)
                            if is_w(&f, "fldChar")
                                && super::names::w_attr(&f, "fldCharType").is_some_and(|v| v == "end"))
                };
                if !only_field_end {
                    break;
                }
                ends += 1;
                b.before = n.next_sibling().map(|s| s.id());
            }
        }
        if self.field_depth_after(p, gap.left) - ends > 0 {
            return Err(readonly(
                "this position is inside a field, whose result is read-only",
            ));
        }
        Ok(b)
    }

    /// Removes the range markers and the comment reference runs whose ids are in `ids` from the
    /// paragraph, and drops a run that the removal leaves empty.
    pub(crate) fn remove_markers(
        &mut self,
        block: &BlockKey,
        ids: &[i64],
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.remove_markers_inner(block, ids) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn remove_markers_inner(&mut self, block: &BlockKey, ids: &[i64]) -> Result<(), StetError> {
        let part = block.part().clone();
        // Found first, removed after: the walk reads the tree and the removal writes it.
        let doomed: Vec<(NodeId, Option<NodeId>)> = {
            let tree = self.tree_ro(&part)?;
            if !tree.node_ref(block.node()).is_element() {
                return Err(super::errors::stale("the paragraph is not in the document"));
            }
            markers_below(&tree.node_ref(block.node()), ids)
        };
        if doomed.is_empty() {
            return Ok(());
        }
        let tree = self.tx.part_mut(&part)?;
        for (node, _) in doomed {
            let parent = parent_of(tree, node);
            detach(tree, node)?;
            if let Some(parent) = parent {
                super::wrap::drop_wrapper_if_empty(tree, parent)?;
            }
        }
        self.touch(block);
        Ok(())
    }

    /// Removes the range markers and the comment reference runs whose ids are in `ids` from a whole part, in
    /// whatever paragraphs and between whatever blocks they are: the two ends of a comment that spans
    /// paragraphs are in two of them, and a marker can stand between blocks. Every paragraph that held one
    /// is touched.
    pub(crate) fn remove_markers_in_part(
        &mut self,
        part: &Arc<str>,
        ids: &[i64],
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.remove_markers_in_part_inner(part, ids) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn remove_markers_in_part_inner(
        &mut self,
        part: &Arc<str>,
        ids: &[i64],
    ) -> Result<(), StetError> {
        let doomed = {
            let tree = self.tree_ro(part)?;
            markers_below(&tree.root(), ids)
        };
        if doomed.is_empty() {
            return Ok(());
        }
        let tree = self.tx.part_mut(part)?;
        for (node, _) in &doomed {
            let parent = parent_of(tree, *node);
            detach(tree, *node)?;
            if let Some(parent) = parent {
                super::wrap::drop_wrapper_if_empty(tree, parent)?;
            }
        }
        for owner in doomed.iter().filter_map(|(_, owner)| *owner) {
            self.touch(&BlockKey::new(part.clone(), owner));
        }
        Ok(())
    }

    /// Puts the markers of `spec` beside the markers of the comment `comment_id` that is already there: the
    /// start right after its start marker, the end right after its reference run (or its end marker if it
    /// has none), and the reference run of `spec` after that. This is where Word puts a reply, whatever the
    /// range of the comment it answers: it may span paragraphs, and then the start and the end are in two of
    /// them. Nothing is written but the markers.
    pub(crate) fn place_markers_beside(
        &mut self,
        part: &Arc<str>,
        comment_id: i64,
        spec: &MarkerSpec,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.place_markers_beside_inner(part, comment_id, spec) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn place_markers_beside_inner(
        &mut self,
        part: &Arc<str>,
        comment_id: i64,
        spec: &MarkerSpec,
    ) -> Result<(), StetError> {
        // The three places in the comment that is there: found first, placed after.
        let (start, end, reference) = {
            let tree = self.tree_ro(part)?;
            let (mut start, mut end, mut reference) = (None, None, None);
            let mut stack = vec![tree.root()];
            while let Some(n) = stack.pop() {
                if rev_id(&n) == Some(comment_id) {
                    if is_w(&n, "commentRangeStart") {
                        start = Some(n.id());
                    } else if is_w(&n, "commentRangeEnd") {
                        end = Some(n.id());
                    } else if is_w(&n, "commentReference") {
                        // The run that holds it, even when it holds more (Word puts a
                        // `w:lastRenderedPageBreak` there): the markers that follow are
                        // siblings of runs, never children of one.
                        reference = Some(match n.parent() {
                            Some(run) if is_w(&run, "r") => run.id(),
                            _ => n.id(),
                        });
                    }
                }
                stack.extend(n.child_elements());
            }
            (start, end, reference)
        };
        let (Some(start), Some(after_end)) = (start, reference.or(end)) else {
            return Err(invalid(format!(
                "the comment {comment_id} has no range in this part, so there is nothing to put a reply beside"
            )));
        };
        let tree = self.tx.part_mut(part)?;
        let end_el = build_element(tree, &spec.end)?;
        let reference_el = spec
            .reference
            .as_ref()
            .map(|r| build_element(tree, r))
            .transpose()?;
        let start_el = build_element(tree, &spec.start)?;
        let (sp, ep) = (
            parent_of(tree, start).ok_or_else(|| internal("a start marker has no parent"))?,
            parent_of(tree, after_end).ok_or_else(|| internal("an end marker has no parent"))?,
        );
        let before_start = next_sibling_of(tree, start);
        insert(tree, sp, before_start, start_el)?;
        let before_end = next_sibling_of(tree, after_end);
        insert(tree, ep, before_end, end_el)?;
        if let Some(run) = reference_el {
            let next = next_sibling_of(tree, end_el);
            insert(tree, ep, next, run)?;
        }
        for node in [start, after_end] {
            let owner = {
                let tree = self.tree_ro(part)?;
                enclosing_paragraph(&tree.node_ref(node))
            };
            if let Some(owner) = owner {
                self.touch(&BlockKey::new(part.clone(), owner));
            }
        }
        Ok(())
    }
}
