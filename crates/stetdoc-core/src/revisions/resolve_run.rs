//! Resolving the revisions that wrap runs (design `revisions.md`, 3B and 3.6): insertions, deletions and
//! moved text, the ones in math, and fields, which are one unit.
//!
//! Accepting an insertion or rejecting a deletion unwraps it. Rejecting an insertion or accepting a
//! deletion removes it and what it holds, and the range markers and comment references in it move to
//! where it was. A revision around any piece of a field is resolved for the whole field.

use super::cleanup::{prune_from, remove_hoisting, rename_text_back, unwrap_element};
use super::names::{is_math, is_w};
use super::resolve::Engine;
use super::tree_ops::{detach, parent_of};
use crate::api::error::StetError;
use crate::wordml::{ItemKind, NodeId, Paragraph, RevKind, Revision};
use crate::xml::SpanTree;

impl Engine<'_, '_> {
    /// The range markers of the moves that `picked` resolves, found before anything is removed: a move that
    /// is resolved has no use for them, and one with its wrappers gone would leave them dangling.
    pub(super) fn move_markers(&mut self, picked: &[Revision]) -> Result<Vec<NodeId>, StetError> {
        let wrappers: Vec<NodeId> = picked
            .iter()
            .filter(|r| matches!(r.kind, RevKind::MoveFrom | RevKind::MoveTo))
            .map(|r| r.node)
            .collect();
        if wrappers.is_empty() {
            return Ok(Vec::new());
        }
        let view = self.tx.parts();
        let Some(id) = view.id(&self.part) else {
            return Ok(Vec::new());
        };
        let Ok(story) = view.story(id) else {
            return Ok(Vec::new());
        };
        let tree = view.tree(id)?;
        let ranges = super::moves::ranges_of(&story, tree, &wrappers);
        let mut out: Vec<NodeId> = Vec::new();
        for m in ranges.values() {
            for n in std::iter::once(m.start).chain(m.end) {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        // Resolution is by move (design 3.5): the ranges of the other side, which share the name, go as well, even when
        // no wrapper is left in them (an earlier step emptied the source, and its markers would dangle).
        let names: Vec<&str> = ranges
            .values()
            .map(|m| m.name.as_str())
            .filter(|n| !n.is_empty())
            .collect();
        for a in story.anchors(tree).moves {
            if !names.contains(&a.name.as_str()) {
                continue;
            }
            for n in a.start.iter().chain(a.end.iter()).map(|l| l.node) {
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        Ok(out)
    }

    /// Removes the move range markers that are still in the tree, and the revision wrappers that held nothing else (a
    /// deletion of the marker alone, which a tracked delete over the move writes).
    pub(super) fn drop_markers(&mut self, markers: Vec<NodeId>) -> Result<(), StetError> {
        for n in markers {
            let tree = self.tx.part_mut(&self.part)?;
            if !super::cleanup::is_attached(tree, n) {
                continue;
            }
            let parent = parent_of(tree, n);
            detach(tree, n)?;
            if let Some(parent) = parent {
                let kept = prune_from(tree, parent)?;
                self.dirty.push(kept);
            }
        }
        Ok(())
    }

    /// Resolves `w:ins`, `w:del`, `w:moveFrom` or `w:moveTo` around runs, or in a math run.
    pub(super) fn resolve_wrapper(&mut self, el: NodeId, kind: RevKind) -> Result<(), StetError> {
        let inserted = matches!(kind, RevKind::Ins | RevKind::MoveTo);
        let remove = inserted != self.accept;
        let math_parent = {
            let tree = self.tx.part_mut(&self.part)?;
            parent_of(tree, el).filter(|p| is_math(&tree.node_ref(*p)))
        };
        if let Some(parent) = math_parent {
            return self.resolve_math(el, parent, inserted);
        }
        // A field that this removal leaves without its start or its end has pieces that mean nothing now.
        let orphaned = if remove {
            self.fields_cut_by(el)?
        } else {
            Vec::new()
        };
        let tree = self.tx.part_mut(&self.part)?;
        let parent = if remove {
            let (parent, swallowed) = remove_hoisting(tree, el)?;
            self.swallowed.extend(swallowed);
            parent
        } else {
            if kind == RevKind::Del {
                rename_text_back(tree, el)?;
            }
            unwrap_element(tree, el)?
        };
        let kept = prune_from(tree, parent)?;
        self.dirty.push(kept);
        self.remove_nodes(orphaned)
    }

    /// Removes nodes, each with what it holds (range markers hoisted), and prunes what they emptied.
    fn remove_nodes(&mut self, nodes: Vec<NodeId>) -> Result<(), StetError> {
        for n in nodes {
            let tree = self.tx.part_mut(&self.part)?;
            if !super::cleanup::is_attached(tree, n) {
                continue;
            }
            let (parent, swallowed) = remove_hoisting(tree, n)?;
            self.swallowed.extend(swallowed);
            let kept = prune_from(tree, parent)?;
            self.dirty.push(kept);
        }
        Ok(())
    }

    /// What a field is left with when `el` goes. A revision removes what it holds and no more: a field whose
    /// code alone was deleted keeps its result, and an inserted field whose result Word left out of the
    /// insertion keeps the result. But a field that loses its `begin` or its `end` to `el` is not a field
    /// any more, and the pieces of its structure that are left (the separator, the other end, an
    /// instruction) would be read as the start of one. They are returned, to go with `el`. The result
    /// stays, as text. A field that goes beyond the paragraph is resolved by its own pieces, and says so.
    fn fields_cut_by(&mut self, el: NodeId) -> Result<Vec<NodeId>, StetError> {
        let tree = self.tx.part_mut(&self.part)?;
        let Some(para) = enclosing_paragraph(tree, el) else {
            return Ok(Vec::new());
        };
        let Some(paragraph) = Paragraph::new(tree, para) else {
            return Ok(Vec::new());
        };
        let map = paragraph.map();
        let items = map.items();
        let inside = |n: NodeId| super::names::contains(tree, el, n);
        let structural = |k: ItemKind| {
            matches!(
                k,
                ItemKind::FieldBegin
                    | ItemKind::FieldSeparate
                    | ItemKind::FieldEnd
                    | ItemKind::FieldCode
            )
        };
        let mut out: Vec<NodeId> = Vec::new();
        let mut spans = false;
        for f in map.fields() {
            if f.simple {
                continue;
            }
            let held = items.iter().any(|i| {
                structural(i.kind) && inside(i.node) && field_of(&map, i.node) == Some(f.id)
            });
            if !held {
                continue;
            }
            let cut = inside(f.begin) || f.end.is_some_and(inside);
            if !cut {
                continue;
            }
            let begin = items
                .iter()
                .position(|i| i.node == f.begin && i.kind == ItemKind::FieldBegin);
            let end = f.end.and_then(|e| {
                items
                    .iter()
                    .position(|i| i.node == e && i.kind == ItemKind::FieldEnd)
            });
            let (Some(b), Some(e), true) = (begin, end, f.begins_here) else {
                spans = true;
                continue;
            };
            for item in items.iter().skip(b).take(e + 1 - b) {
                let unit = item.run.unwrap_or(item.node);
                if structural(item.kind)
                    && field_of(&map, item.node) == Some(f.id)
                    && !out.contains(&unit)
                    && !inside(unit)
                {
                    out.push(unit);
                }
            }
        }
        if spans {
            self.warnings.push(
                "a field that spans paragraphs was resolved piece by piece, not as a whole"
                    .to_owned(),
            );
        }
        Ok(out)
    }

    /// A revision in a math run: `m:r > w:ins`, or `m:ctrlPr > w:ins` of an `m:f`.
    fn resolve_math(
        &mut self,
        el: NodeId,
        parent: NodeId,
        inserted: bool,
    ) -> Result<(), StetError> {
        let tree = self.tx.part_mut(&self.part)?;
        let in_run = tree.node_ref(parent).local_name() == "r";
        // Accepting an insertion or rejecting a deletion keeps what is there.
        let keep = inserted == self.accept;
        let victim = if in_run {
            parent
        } else {
            // `m:ctrlPr` inside `m:fPr` inside `m:f`: the whole fraction goes.
            let mut cur = tree.node_ref(parent).parent();
            while let Some(n) = cur {
                if n.local_name() == "f" && is_math(&n) {
                    break;
                }
                cur = n.parent();
            }
            cur.map_or(parent, |n| n.id())
        };
        if keep {
            if in_run {
                let p = unwrap_element(tree, el)?;
                self.dirty.push(p);
            } else {
                detach(tree, el)?;
                let kept = prune_from(tree, parent)?;
                self.dirty.push(kept);
            }
        } else {
            let (p, swallowed) = remove_hoisting(tree, victim)?;
            self.swallowed.extend(swallowed);
            self.dirty.push(p);
        }
        Ok(())
    }
}

/// The paragraph that holds an element.
fn enclosing_paragraph(tree: &SpanTree, el: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(el).parent();
    while let Some(n) = cur {
        if is_w(&n, "p") {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

/// The id of the field whose structure the element is a piece of.
fn field_of(map: &crate::wordml::TextMap, node: NodeId) -> Option<u32> {
    map.fields()
        .iter()
        .find(|f| f.begin == node || f.separate == Some(node) || f.end == Some(node))
        .map(|f| f.id)
        .or_else(|| {
            map.items()
                .iter()
                .find(|i| i.node == node)
                .map(|i| i.field)
                .filter(|f| *f != crate::wordml::NO_FIELD)
        })
}
