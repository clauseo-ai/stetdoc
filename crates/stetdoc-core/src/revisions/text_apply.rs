//! Applying one edit of a replacement to the XML: carve what it deletes, delete it by whose text it is,
//! and put the new text where the gap rule says. This is the body of `replace_text`, for one piece of
//! the difference, on the map that the call built before it changed anything.

use super::carve::groups;
use super::compiler::{Compiler, ParaView};
use super::errors::{internal, readonly};
use super::gap::SettleMode;
use super::names::is_w;
use super::text::Piece;
use super::tree_ops::{detach, parent_of, rpr_of};
use crate::api::error::StetError;
use crate::wordml::{ItemKind, Mode, NodeId};
use crate::xml::NodeKind;

impl Compiler<'_, '_> {
    /// Applies one piece. `starts` are the `Final` offsets at which the map's items start.
    pub(super) fn apply_piece(
        &mut self,
        p: &ParaView,
        starts: &[usize],
        piece: &Piece,
    ) -> Result<(), StetError> {
        if !self.tracked() && self.untracked_in_place(p, starts, piece)? {
            return Ok(());
        }
        let map = &p.map;
        let (format, mut boundary, depth_item) = if piece.start < piece.end {
            let loc = map
                .locate(Mode::Final, piece.start..piece.end)
                .ok_or_else(|| internal("a range that was checked is outside the paragraph"))?;
            let loc = super::text::cover(p, loc)?;
            let before = map.gap(Mode::Final, piece.start);
            let after = map.gap(Mode::Final, piece.end);
            // The formatting of the first text that is deleted, read before the carve moves anything.
            let first_text_run = loc.spans.iter().find_map(|s| {
                let it = map.items().get(s.item)?;
                (it.kind == ItemKind::Text).then_some(it.run).flatten()
            });
            let format = match first_text_run {
                Some(run) => rpr_of(self.tree_ro(&p.part)?, run),
                None => {
                    let left = before.as_ref().and_then(|g| g.left);
                    let right = after.as_ref().and_then(|g| g.right);
                    self.format_source(p, left, right)?
                }
            };
            let last_item = loc.spans.last().map(|s| s.item);
            let carved = self.carve(p, &loc)?;
            let grouped = groups(self.tree_ro(&p.part)?, &carved.units);
            let boundary = self.delete_groups(p, grouped)?;
            if piece.with.is_empty() {
                return Ok(());
            }
            let boundary =
                boundary.ok_or_else(|| internal("a deletion left no place for the new text"))?;
            (format, boundary, last_item)
        } else {
            let gap = map
                .gap(Mode::Final, piece.start)
                .ok_or_else(|| internal("an offset that was checked is outside the paragraph"))?;
            let format = self.format_source(p, gap.left, gap.right)?;
            let boundary = match (gap.left, gap.right) {
                (Some(l), Some(r)) if l == r => {
                    let item = *map
                        .items()
                        .get(l)
                        .ok_or_else(|| internal("a gap names a missing item"))?;
                    let start = starts.get(l).copied().unwrap_or(0);
                    let at = item.from as usize + (piece.start - start);
                    self.boundary_inside_item(p, l, at)?
                }
                (Some(l), _) => self.boundary_after_item(p, l)?,
                (None, _) => self.boundary_at_start(p)?,
            };
            (format, boundary, gap.left)
        };
        let (settled, ends) = self.settle(&p.part, boundary, SettleMode::Insert)?;
        boundary = settled;
        if self.field_depth_after(p, depth_item) - ends > 0 {
            return Err(readonly(
                "this position is inside a field, whose result is read-only",
            ));
        }
        // With no text to copy from, the source is the paragraph mark, whose hidden formatting the new
        // text does not take: `build_mark_run` drops it, as `insert_block` does.
        let from_mark = {
            let tree = self.tree_ro(&p.part)?;
            format
                .and_then(|f| parent_of(tree, f))
                .is_some_and(|ppr| is_w(&tree.node_ref(ppr), "pPr"))
        };
        let run = if from_mark {
            self.build_mark_run(&p.part, &piece.with, format)?
        } else {
            self.build_run(&p.part, &piece.with, format)?
        };
        self.place_run(&p.part, boundary, run)?;
        Ok(())
    }

    /// Untracked, an edit that lies inside one text node is done in the node, so that the runs of the
    /// paragraph stay as they were. Returns false when the edit is not that simple.
    fn untracked_in_place(
        &mut self,
        p: &ParaView,
        starts: &[usize],
        piece: &Piece,
    ) -> Result<bool, StetError> {
        if piece.with.contains(['\t', '\n']) {
            return Ok(false);
        }
        let map = &p.map;
        let (node, from, to) = if piece.start < piece.end {
            let Some(loc) = map.locate(Mode::Final, piece.start..piece.end) else {
                return Ok(false);
            };
            let [span] = loc.spans.as_slice() else {
                return Ok(false);
            };
            let Some(item) = map.items().get(span.item) else {
                return Ok(false);
            };
            if item.kind != ItemKind::Text {
                return Ok(false);
            }
            (span.node, span.start as usize, span.end as usize)
        } else {
            let Some(gap) = map.gap(Mode::Final, piece.start) else {
                return Ok(false);
            };
            let (Some(l), Some(r)) = (gap.left, gap.right) else {
                return Ok(false);
            };
            let Some(item) = map.items().get(l) else {
                return Ok(false);
            };
            if l != r || item.kind != ItemKind::Text {
                return Ok(false);
            }
            let at = item.from as usize + (piece.start - starts.get(l).copied().unwrap_or(0));
            (item.node, at, at)
        };
        {
            let tree = self.tree_ro(&p.part)?;
            if tree.node_ref(node).kind() != NodeKind::Text {
                return Ok(false);
            }
        }
        let tree = self.tx.part_mut(&p.part)?;
        tree.edit()
            .replace_text_range(node, from, to, &piece.with)?;
        let empty = tree.node_ref(node).text().is_empty();
        if empty {
            self.prune_empty_text(&p.part, node)?;
        } else if let Some(holder) = parent_of(self.tree_ro(&p.part)?, node) {
            let tree = self.tx.part_mut(&p.part)?;
            super::tree_ops::ensure_preserve(tree, holder)?;
        }
        Ok(true)
    }

    /// Removes a text node that has become empty, the `w:t` that held it, the run that held that, and
    /// the revision wrapper that held the run, as far as each is left with nothing.
    fn prune_empty_text(
        &mut self,
        part: &std::sync::Arc<str>,
        text: NodeId,
    ) -> Result<(), StetError> {
        let tree = self.tx.part_mut(part)?;
        let Some(holder) = parent_of(tree, text) else {
            return Ok(());
        };
        let only = tree.node_ref(holder).children().count() == 1;
        if !only {
            detach(tree, text)?;
            return Ok(());
        }
        let run = parent_of(tree, holder);
        detach(tree, holder)?;
        let Some(run) = run else { return Ok(()) };
        let run_empty = {
            let n = tree.node_ref(run);
            super::names::is_w(&n, "r")
                && n.children()
                    .all(|c| !c.is_element() || super::names::is_w(&c, "rPr"))
        };
        if run_empty {
            let wrapper = parent_of(tree, run);
            detach(tree, run)?;
            if let Some(w) = wrapper {
                super::wrap::drop_wrapper_if_empty(tree, w)?;
            }
        }
        Ok(())
    }
}
