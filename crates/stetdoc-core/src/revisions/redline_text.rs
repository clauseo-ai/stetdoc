//! `redline_paragraph`: the replacements, the changed run formatting and the old properties of one paragraph of the
//! revised document, in one call and over one text map (design `compare.md`, 6.1).
//!
//! The paragraph is the revised one, so the inserted text is its own runs, wrapped in `w:ins` where they stand. The deleted text
//! is not in it: it arrives as detached runs (copied from the original with their own formatting) and is placed in `w:del`
//! immediately before the insertion, or at the gap that the edit names. The edits go from the last to the first, as
//! `replace_text` applies its pieces, so what is still to come keeps its offsets, and the formatting changes (which lie in
//! the stretches that no replacement touches) are mixed in by position.

use super::carve::groups;
use super::compiler::{Compiler, ParaView};
use super::errors::{internal, invalid};
use super::gap::{Boundary, SettleMode};
use super::names::is_w;
use super::outcome::Effect;
use super::redline_types::{ParaRedline, RedlineEdit, RunSnapshot};
use super::redline_wrap::{Rev, wrappable};
use super::text::{cover, final_starts};
use super::tree_ops::{insert, parent_of};
use crate::api::error::StetError;
use crate::wordml::{BlockKey, Mode, NodeId};
use std::sync::Arc;

/// One thing to do to the paragraph, with where it starts.
enum Op<'a> {
    Edit(&'a RedlineEdit),
    Fmt(&'a RunSnapshot),
}

impl Op<'_> {
    fn start(&self) -> usize {
        match self {
            Op::Edit(e) => e.insert.start,
            Op::Fmt(f) => f.range.start,
        }
    }
}

impl Compiler<'_, '_> {
    /// Writes `plan` into the paragraph `block` of the revised document. See the design for what each part becomes. A refused
    /// call has changed nothing.
    pub(crate) fn redline_paragraph(
        &mut self,
        block: &BlockKey,
        plan: &ParaRedline,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.redline_inner(block, plan) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn redline_inner(&mut self, block: &BlockKey, plan: &ParaRedline) -> Result<(), StetError> {
        if !self.tracked() {
            return Err(internal("a redline is written tracked"));
        }
        let part = block.part().clone();
        let p = self.paragraph(block)?;
        let len = p.map.len(Mode::Final);
        let mut ops: Vec<Op<'_>> = Vec::new();
        for e in &plan.edits {
            if e.insert.start > e.insert.end || e.insert.end > len {
                return Err(invalid(format!(
                    "the range {}..{} is outside the paragraph, which has {len} characters",
                    e.insert.start, e.insert.end
                )));
            }
            ops.push(Op::Edit(e));
        }
        for f in &plan.fmt {
            if f.range.start >= f.range.end || f.range.end > len {
                return Err(invalid(format!(
                    "the range {}..{} is outside the paragraph, which has {len} characters",
                    f.range.start, f.range.end
                )));
            }
            ops.push(Op::Fmt(f));
        }
        self.precheck_ops(&p, plan)?;
        // from the last to the first; at one offset the formatting of the text that starts there is to the right of a deletion
        // that ends up there, so it goes first
        ops.sort_by_key(|o| (std::cmp::Reverse(o.start()), matches!(o, Op::Edit(_))));
        let starts = final_starts(&p.map);
        for op in ops {
            match op {
                Op::Edit(e) => self.redline_edit(&p, &starts, e)?,
                Op::Fmt(f) => self.redline_format(&p, f)?,
            }
        }
        if plan.mark_inserted {
            self.add_mark_revision(&part, p.node, "ins")?;
        }
        if plan.mark_deleted {
            self.add_mark_revision(&part, p.node, "del")?;
        }
        if let Some(snapshot) = &plan.snapshot {
            self.write_snapshot(&part, p.node, snapshot)?;
        }
        self.touch(block);
        if let Some(source) = &plan.source
            && plan.edits.iter().any(|e| !e.deleted.is_empty())
        {
            self.fx.imported.push((block.clone(), source.clone()));
        }
        Ok(())
    }

    /// What the calls of the plan would refuse, found before anything is written, so that a refused call has changed nothing.
    fn precheck_ops(&self, p: &ParaView, plan: &ParaRedline) -> Result<(), StetError> {
        for e in &plan.edits {
            if !e.insert.is_empty() {
                self.precheck_range(p, e.insert.clone())?;
            } else if !e.deleted.is_empty() {
                let gap = p
                    .map
                    .gap(Mode::Final, e.insert.start)
                    .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
                for i in [gap.left, gap.right].into_iter().flatten() {
                    self.check_items(p, i..i + 1)?;
                }
            }
        }
        for f in &plan.fmt {
            self.precheck_range(p, f.range.clone())?;
        }
        Ok(())
    }

    fn precheck_range(&self, p: &ParaView, range: std::ops::Range<usize>) -> Result<(), StetError> {
        let loc = p
            .map
            .locate(Mode::Final, range)
            .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
        let loc = cover(p, loc)?;
        self.check_items(p, loc.items.clone())?;
        self.check_carve(p, &loc)
    }

    /// Whether `redline_paragraph` would take this plan, without writing anything.
    pub(crate) fn precheck_redline(
        &self,
        block: &BlockKey,
        plan: &ParaRedline,
    ) -> Result<(), StetError> {
        let p = self.paragraph(block)?;
        let len = p.map.len(Mode::Final);
        for (a, b) in plan
            .edits
            .iter()
            .map(|e| (e.insert.start, e.insert.end))
            .chain(plan.fmt.iter().map(|f| (f.range.start, f.range.end)))
        {
            if a > b || b > len {
                return Err(invalid(format!(
                    "the range {a}..{b} is outside the paragraph, which has {len} characters"
                )));
            }
        }
        self.precheck_ops(&p, plan)
    }

    /// Whether the paragraph can be cut at these offsets and the marks between the parts deleted: it ends no section, no cut is inside
    /// a field or a locked control, and the paragraph can be edited.
    pub(crate) fn precheck_split(&self, block: &BlockKey, cuts: &[usize]) -> Result<(), StetError> {
        let p = self.paragraph(block)?;
        self.check_editable(&p)?;
        if p.flags & crate::wordml::SECTION_BREAK != 0 {
            return Err(super::errors::readonly(
                "this paragraph ends a section, and merging it would move or drop the break",
            ));
        }
        for &cut in cuts {
            let gap = p
                .map
                .gap(Mode::Final, cut)
                .ok_or_else(|| invalid("an offset lies outside the paragraph"))?;
            for i in [gap.left, gap.right].into_iter().flatten() {
                self.check_items(&p, i..i + 1)?;
            }
            if self.field_depth_after(&p, gap.left) > 0 {
                return Err(super::errors::readonly(
                    "this position is inside a field, whose result is read-only",
                ));
            }
        }
        let last = p.map.items().len().checked_sub(1);
        if self.field_depth_after(&p, last) > 0 {
            return Err(super::errors::readonly(
                "this paragraph begins or ends a field that spans paragraphs, so its mark cannot be merged",
            ));
        }
        Ok(())
    }

    /// One replacement: the revised text is wrapped in `w:ins`, and the deleted text goes in front of it.
    fn redline_edit(
        &mut self,
        p: &ParaView,
        starts: &[usize],
        edit: &RedlineEdit,
    ) -> Result<(), StetError> {
        let mut first_ins: Option<NodeId> = None;
        if !edit.insert.is_empty() {
            let loc = p
                .map
                .locate(Mode::Final, edit.insert.clone())
                .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
            let loc = cover(p, loc)?;
            self.check_items(p, loc.items.clone())?;
            let carved = self.carve(p, &loc)?;
            let grouped = groups(self.tree_ro(&p.part)?, &carved.units);
            for (nodes, _) in grouped {
                let pieces = {
                    let tree = self.tree_ro(&p.part)?;
                    wrappable(tree, &nodes)
                };
                for piece in pieces {
                    let w = self.wrap_nodes(&p.part, &piece, Rev::Ins)?;
                    first_ins.get_or_insert(w);
                }
            }
        }
        if edit.deleted.is_empty() {
            return Ok(());
        }
        let boundary = match first_ins {
            Some(w) => {
                let tree = self.tree_ro(&p.part)?;
                Boundary {
                    parent: parent_of(tree, w)
                        .ok_or_else(|| internal("an insertion has no parent"))?,
                    before: Some(w),
                }
            }
            None => self.gap_boundary(p, starts, edit.insert.start)?,
        };
        self.place_fragment(&p.part, boundary, &edit.deleted)
    }

    /// The settled boundary at an offset of the paragraph's `Final` text, where deleted text is put when nothing is inserted.
    fn gap_boundary(
        &mut self,
        p: &ParaView,
        starts: &[usize],
        offset: usize,
    ) -> Result<Boundary, StetError> {
        let gap = p
            .map
            .gap(Mode::Final, offset)
            .ok_or_else(|| internal("an offset that was checked is outside the paragraph"))?;
        let boundary = match (gap.left, gap.right) {
            (Some(l), Some(r)) if l == r => {
                let item = *p
                    .map
                    .items()
                    .get(l)
                    .ok_or_else(|| internal("a gap names a missing item"))?;
                let start = starts.get(l).copied().unwrap_or(0);
                let at = item.from as usize + (offset - start);
                // the formatting of the text just to the right may have cut the node at this very offset
                let cut_already = self
                    .tree_ro(&p.part)?
                    .node_ref(item.node)
                    .text()
                    .chars()
                    .count()
                    <= at;
                if cut_already {
                    self.boundary_after_item(p, l)?
                } else {
                    self.boundary_inside_item(p, l, at)?
                }
            }
            (Some(l), _) => self.boundary_after_item(p, l)?,
            (None, _) => self.boundary_at_start(p)?,
        };
        let (settled, _) = self.settle(&p.part, boundary, SettleMode::Insert)?;
        Ok(settled)
    }

    /// Puts deleted text at a boundary: each stretch of runs in a `w:del` of its own, and a wrapper (a hyperlink) as it is,
    /// with its runs in a `w:del` inside it.
    fn place_fragment(
        &mut self,
        part: &Arc<str>,
        boundary: Boundary,
        nodes: &[NodeId],
    ) -> Result<(), StetError> {
        // a boundary inside an insertion of ours is moved out in front of it: a deletion is never inside our own insertion
        let boundary = {
            let tree = self.tree_ro(part)?;
            let parent = tree.node_ref(boundary.parent);
            if is_w(&parent, "ins") && self.is_created_node(tree, boundary.parent) {
                let grand = parent_of(tree, boundary.parent)
                    .ok_or_else(|| internal("an insertion has no parent"))?;
                Boundary {
                    parent: grand,
                    before: Some(boundary.parent),
                }
            } else {
                boundary
            }
        };
        let mut run: Vec<NodeId> = Vec::new();
        let mut order: Vec<Vec<NodeId>> = Vec::new();
        {
            let tree = self.tree_ro(part)?;
            for &n in nodes {
                if is_w(&tree.node_ref(n), "r") {
                    run.push(n);
                } else {
                    if !run.is_empty() {
                        order.push(std::mem::take(&mut run));
                    }
                    order.push(vec![n]);
                }
            }
            if !run.is_empty() {
                order.push(run);
            }
        }
        for group in order {
            let is_runs = {
                let tree = self.tree_ro(part)?;
                group.iter().all(|&n| is_w(&tree.node_ref(n), "r"))
            };
            if is_runs {
                // the runs go into the tree in front of the boundary, then into their wrapper
                let tree = self.tx.part_mut(part)?;
                for &n in &group {
                    insert(tree, boundary.parent, boundary.before, n)?;
                }
                self.wrap_nodes(part, &group, Rev::Del)?;
            } else if let Some(&holder) = group.first() {
                let tree = self.tx.part_mut(part)?;
                insert(tree, boundary.parent, boundary.before, holder)?;
                self.wrap_content(part, holder, Rev::Del)?;
            }
        }
        Ok(())
    }

    /// Changed formatting over unchanged text: an `rPrChange` with the old properties on each run that holds the range.
    fn redline_format(&mut self, p: &ParaView, fmt: &RunSnapshot) -> Result<(), StetError> {
        let loc = p
            .map
            .locate(Mode::Final, fmt.range.clone())
            .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
        let loc = cover(p, loc)?;
        self.check_items(p, loc.items.clone())?;
        let carved = self.carve(p, &loc)?;
        for (unit, _) in carved.units {
            let is_run = is_w(&self.tree_ro(&p.part)?.node_ref(unit), "r");
            if is_run {
                self.write_change(&p.part, unit, &["rPr"], "rPrChange", "rPr", &fmt.snapshot)?;
            }
        }
        Ok(())
    }
}
