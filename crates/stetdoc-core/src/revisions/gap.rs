//! Where new text goes (design `revisions.md`, 4.2): the gap rule. A place in the XML is a [`Boundary`]:
//! a parent and the sibling the new node goes before. It starts right after some node and is
//! **settled**: it moves past the range-end markers and the deleted text that follow, and out of any
//! hyperlink, control or smart tag that it is at the end or the start of, and out of a revision wrapper,
//! splitting the wrapper if the boundary is inside it. The text of a new insertion is never put in a
//! hyperlink at its edge, in a deletion, or in another insertion.

use super::compiler::{Compiler, ParaView};
use super::errors::{internal, readonly, unsupported};
use super::names::{first_content, is_blank, is_w, next_content, prev_content, rev_id, wns};
use super::policy::is_created;
use super::tree_ops::{child_ids, detach, insert, move_to, next_sibling_of, parent_of, set_w_attr};
use crate::api::error::StetError;
use crate::wordml::{ItemKind, NodeId, revision_at};
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

/// A place between two siblings: a node goes into `parent` before `before`, or last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Boundary {
    /// The element the node goes into.
    pub parent: NodeId,
    /// The sibling it goes before.
    pub before: Option<NodeId>,
}

/// What a boundary is settled for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettleMode {
    /// New text goes there: it stays inside a container that both neighbours share, and inside an
    /// insertion that this transaction made.
    Insert,
    /// A paragraph is cut there: every container and wrapper that the boundary is strictly inside is cut
    /// in two.
    Split,
}

/// What can be skipped over when a boundary is settled.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Skip {
    /// Not skippable: the boundary stops before it.
    No,
    /// A range-end marker, a proofing mark, white space, or deleted content.
    Yes,
    /// A run that only ends a field: skipped, and counted, so that the boundary is outside the field.
    FieldEnd,
}

/// What an element is when a boundary is inside it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Inside {
    /// The paragraph: the boundary stays.
    Paragraph,
    /// `w:ins` or `w:moveTo`.
    Insertion,
    /// `w:del` or `w:moveFrom`.
    Deletion,
    /// A hyperlink, smart tag, custom XML element, directional wrapper or the content of a control.
    Container,
    /// A simple field: the boundary is inside a field.
    SimpleField,
    /// Something that this release does not edit in: ruby, an alternate content branch.
    Unsupported,
}

fn skip_of(n: &NodeRef<'_>) -> Skip {
    if is_blank(n) {
        return Skip::Yes;
    }
    if !n.is_element() {
        return Skip::Yes;
    }
    if super::names::is_word_ns(n.namespace()) {
        match n.local_name() {
            "bookmarkEnd" | "commentRangeEnd" | "permEnd" | "moveFromRangeEnd"
            | "moveToRangeEnd" | "proofErr" | "del" | "moveFrom" => return Skip::Yes,
            "r" => {
                let mut content = n.child_elements().filter(|c| !is_w(c, "rPr"));
                if let (Some(only), None) = (content.next(), content.next()) {
                    if is_w(&only, "fldChar")
                        && super::names::w_attr(&only, "fldCharType").is_some_and(|v| v == "end")
                    {
                        return Skip::FieldEnd;
                    }
                    // The run that holds a comment's reference follows the range end, and is passed with it.
                    if is_w(&only, "commentReference") {
                        return Skip::Yes;
                    }
                }
            }
            _ => {}
        }
    }
    Skip::No
}

/// True for a range end or a proofing mark, which a new insertion goes after.
fn is_end_marker(n: &NodeRef<'_>) -> bool {
    n.is_element()
        && super::names::is_word_ns(n.namespace())
        && matches!(
            n.local_name(),
            "bookmarkEnd"
                | "commentRangeEnd"
                | "permEnd"
                | "moveFromRangeEnd"
                | "moveToRangeEnd"
                | "proofErr"
        )
}

fn inside_of(n: &NodeRef<'_>) -> Inside {
    if super::names::is_word_ns(n.namespace()) {
        return match n.local_name() {
            "p" => Inside::Paragraph,
            "ins" | "moveTo" => Inside::Insertion,
            "del" | "moveFrom" => Inside::Deletion,
            "fldSimple" => Inside::SimpleField,
            "ruby" | "rubyBase" => Inside::Unsupported,
            _ => Inside::Container,
        };
    }
    match n.local_name() {
        "Choice" | "Fallback" => Inside::Unsupported,
        _ => Inside::Container,
    }
}

impl Compiler<'_, '_> {
    /// True if the revision element `node` was created by this transaction: its author is the stamp's,
    /// its id was minted, and it is not the new id of a split.
    pub(super) fn is_created_node(&self, tree: &SpanTree, node: NodeId) -> bool {
        let Some(stamp) = self.stamp() else {
            return false;
        };
        let n = tree.node_ref(node);
        let Some(r) = revision_at(&n) else {
            return false;
        };
        let Some(id) = r.id else { return false };
        is_created(
            r.author.as_deref(),
            id,
            stamp,
            &self.tx.minted_annotation_ids(),
            self.tx.copied_annotation_ids(),
        )
    }

    /// The boundary right after item `index` of the paragraph's map: after its run, or inside the run
    /// if the run holds more after the item, in which case the run is cut there.
    pub(super) fn boundary_after_item(
        &mut self,
        p: &ParaView,
        index: usize,
    ) -> Result<Boundary, StetError> {
        let item = *p
            .map
            .items()
            .get(index)
            .ok_or_else(|| internal("a gap names an item that the map does not have"))?;
        let (holder, parent, after_run) = {
            let tree = self.tree_ro(&p.part)?;
            let node = tree.node_ref(item.node);
            let holder = if item.kind == ItemKind::Text {
                node.parent()
                    .ok_or_else(|| internal("a text node has no parent"))?
                    .id()
            } else {
                item.node
            };
            match item.run {
                Some(run) => (
                    Some(holder),
                    parent_of(tree, run),
                    Some((run, next_sibling_of(tree, run))),
                ),
                None => (None, parent_of(tree, item.node), None),
            }
        };
        let Some(parent) = parent else {
            return Err(internal("an item has no parent"));
        };
        match (item.run, holder, after_run) {
            (Some(run), Some(holder), Some((_, next_of_run))) => {
                // Text after this node in the same `w:t`: the `w:t` is cut after it.
                let split_here = item.kind == ItemKind::Text && {
                    let tree = self.tree_ro(&p.part)?;
                    super::carve::more_text_beside(&tree.node_ref(item.node), true)
                };
                if split_here {
                    let at = {
                        let tree = self.tree_ro(&p.part)?;
                        tree.node_ref(item.node).text().chars().count()
                    };
                    let right = self.split_run(
                        &p.part,
                        run,
                        super::carve::RunCut::InText {
                            text: item.node,
                            at,
                        },
                    )?;
                    return Ok(Boundary {
                        parent,
                        before: Some(right),
                    });
                }
                let more = {
                    let tree = self.tree_ro(&p.part)?;
                    next_content(&tree.node_ref(holder)).map(|n| n.id())
                };
                match more {
                    Some(child) => {
                        let right =
                            self.split_run(&p.part, run, super::carve::RunCut::Before(child))?;
                        Ok(Boundary {
                            parent,
                            before: Some(right),
                        })
                    }
                    None => Ok(Boundary {
                        parent,
                        before: next_of_run,
                    }),
                }
            }
            _ => {
                let tree = self.tree_ro(&p.part)?;
                Ok(Boundary {
                    parent,
                    before: next_sibling_of(tree, item.node),
                })
            }
        }
    }

    /// The boundary at the very start of the paragraph's content, after `w:pPr`.
    pub(super) fn boundary_at_start(&self, p: &ParaView) -> Result<Boundary, StetError> {
        let tree = self.tree_ro(&p.part)?;
        let para = tree.node_ref(p.node);
        let mut cur = first_content(&para);
        while let Some(c) = cur {
            if is_w(&c, "pPr") {
                cur = next_content(&c);
            } else {
                break;
            }
        }
        Ok(Boundary {
            parent: p.node,
            before: cur.map(|c| c.id()),
        })
    }

    /// The boundary inside the text of item `index`, `at` characters into its node: the run is cut.
    pub(super) fn boundary_inside_item(
        &mut self,
        p: &ParaView,
        index: usize,
        at: usize,
    ) -> Result<Boundary, StetError> {
        let item = *p
            .map
            .items()
            .get(index)
            .ok_or_else(|| internal("a gap names an item that the map does not have"))?;
        let run = item.run.ok_or_else(|| internal("a text item has no run"))?;
        let right = self.split_run(
            &p.part,
            run,
            super::carve::RunCut::InText {
                text: item.node,
                at,
            },
        )?;
        let tree = self.tree_ro(&p.part)?;
        let parent = parent_of(tree, run).ok_or_else(|| internal("a run has no parent"))?;
        Ok(Boundary {
            parent,
            before: Some(right),
        })
    }

    /// How many fields are open at the end of item `index`, or at the start of the paragraph if there is
    /// none: the fields carried in from an earlier paragraph, plus the begins, minus the ends.
    pub(super) fn field_depth_after(&self, p: &ParaView, index: Option<usize>) -> i64 {
        let carried = p.map.fields().iter().filter(|f| !f.begins_here).count() as i64;
        let Some(index) = index else { return carried };
        let mut depth = carried;
        for item in p.map.items().iter().take(index + 1) {
            match item.kind {
                ItemKind::FieldBegin => depth += 1,
                ItemKind::FieldEnd => depth -= 1,
                _ => {}
            }
        }
        depth
    }

    /// Settles a boundary for a new insertion: past the end markers and the deleted text that follow it,
    /// out of a hyperlink or the like at its edge, and out of a revision wrapper, which is cut if the
    /// boundary is inside it. Returns the boundary and how many field ends it passed.
    pub(super) fn settle(
        &mut self,
        part: &Arc<str>,
        mut b: Boundary,
        mode: SettleMode,
    ) -> Result<(Boundary, i64), StetError> {
        let mut ends = 0i64;
        for _ in 0..100_000 {
            // 1. Skip what the boundary passes.
            {
                let tree = self.tree_ro(part)?;
                while let Some(next) = b.before {
                    let n = tree.node_ref(next);
                    match skip_of(&n) {
                        Skip::No => break,
                        Skip::Yes => {}
                        Skip::FieldEnd => ends += 1,
                    }
                    b.before = n.next_sibling().map(|s| s.id());
                }
            }
            // 2. Where is it?
            let (inside, content_before, wrapper_parent, wrapper_next, container) = {
                let tree = self.tree_ro(part)?;
                let parent = tree.node_ref(b.parent);
                let inside = inside_of(&parent);
                let before_node = match b.before {
                    Some(id) => prev_content(&tree.node_ref(id)),
                    None => parent.last_child().and_then(|l| {
                        if is_blank(&l) {
                            prev_content(&l)
                        } else {
                            Some(l)
                        }
                    }),
                };
                // Something that holds content: a wrapper's children are runs and markers.
                let content_before =
                    before_node.is_some_and(|n| !is_w(&n, "pPr") && !is_w(&n, "sdtPr"));
                // A control's content is in w:sdtContent, and the node that goes out is the control.
                let container = if parent.local_name() == "sdtContent" {
                    parent.parent().map(|c| c.id()).unwrap_or(b.parent)
                } else {
                    b.parent
                };
                let outer = tree.node_ref(container);
                (
                    inside,
                    content_before,
                    outer.parent().map(|x| x.id()),
                    outer.next_sibling().map(|x| x.id()),
                    container,
                )
            };
            let content_after = b.before.is_some();
            match inside {
                Inside::Paragraph => return Ok((b, ends)),
                Inside::SimpleField => {
                    return Err(readonly(
                        "this position is inside a field, whose result is read-only",
                    ));
                }
                Inside::Unsupported => {
                    return Err(unsupported(
                        "this position is inside ruby text or alternate content, which this release does not edit",
                    ));
                }
                Inside::Container => {
                    let Some(grand) = wrapper_parent else {
                        return Err(internal("a wrapper has no parent"));
                    };
                    if content_before && content_after {
                        if mode == SettleMode::Insert {
                            return Ok((b, ends));
                        }
                        // A paragraph is cut inside the container: the container is cut too, and each
                        // paragraph keeps a copy of it. A content control is one inline thing, and is not.
                        if container != b.parent {
                            return Err(readonly(
                                "a paragraph cannot be split inside a content control. Split it before or after the control.",
                            ));
                        }
                        let tail = self.split_container(part, b.parent, b.before)?;
                        b = Boundary {
                            parent: grand,
                            before: Some(tail),
                        };
                        continue;
                    }
                    b = if content_after {
                        Boundary {
                            parent: grand,
                            before: Some(container),
                        }
                    } else {
                        Boundary {
                            parent: grand,
                            before: wrapper_next,
                        }
                    };
                }
                Inside::Insertion | Inside::Deletion => {
                    if inside == Inside::Insertion && mode == SettleMode::Insert {
                        let (stay, climb) = {
                            let tree = self.tree_ro(part)?;
                            // Untracked, new text stays only in an insertion of the author's own: at the edge of
                            // another author's it goes beside it, and inside one it was refused before.
                            let created = if self.tracked() {
                                self.is_created_node(tree, b.parent)
                            } else {
                                crate::wordml::revision_at(&tree.node_ref(b.parent))
                                    .is_some_and(|r| self.is_own_author(r.author.as_deref()))
                            };
                            // At the end of our own insertion, a range end that follows it comes first:
                            // the gap rule puts the new text after it, and the insertion then takes the
                            // marker in with the text.
                            let end_marker = self.tracked()
                                && !content_after
                                && next_content(&tree.node_ref(b.parent))
                                    .is_some_and(|n| is_end_marker(&n));
                            (created, end_marker)
                        };
                        if stay && !climb {
                            return Ok((b, ends));
                        }
                    }
                    let Some(grand) = wrapper_parent else {
                        return Err(internal("a wrapper has no parent"));
                    };
                    if !content_after {
                        b = Boundary {
                            parent: grand,
                            before: wrapper_next,
                        };
                    } else if !content_before {
                        b = Boundary {
                            parent: grand,
                            before: Some(b.parent),
                        };
                    } else {
                        let tail = self.split_wrapper(part, b.parent, b.before)?;
                        b = Boundary {
                            parent: grand,
                            before: Some(tail),
                        };
                    }
                }
            }
        }
        Err(internal("a boundary did not settle"))
    }

    /// Cuts a container (a hyperlink, smart tag, custom XML element, `w:dir` or `w:bdo`) before its child
    /// `at`: the children from `at` on move to a copy of the container, with its attributes, which is
    /// placed right after it. Returns the copy.
    pub(super) fn split_container(
        &mut self,
        part: &Arc<str>,
        container: NodeId,
        at: Option<NodeId>,
    ) -> Result<NodeId, StetError> {
        let tree = self.tx.part_mut(part)?;
        let tail = tree.edit().clone_subtree(container)?;
        for child in child_ids(tree, tail) {
            detach(tree, child)?;
        }
        if let Some(at) = at {
            let moved: Vec<NodeId> = {
                let n = tree.node_ref(container);
                let mut found = Vec::new();
                let mut on = false;
                for c in n.children() {
                    if c.id() == at {
                        on = true;
                    }
                    if on {
                        found.push(c.id());
                    }
                }
                found
            };
            for child in moved {
                move_to(tree, child, tail, None)?;
            }
        }
        let parent =
            parent_of(tree, container).ok_or_else(|| internal("a container has no parent"))?;
        let next = next_sibling_of(tree, container);
        insert(tree, parent, next, tail)?;
        Ok(tail)
    }

    /// Cuts a revision wrapper before its child `at`: the children from `at` on move to a new wrapper with
    /// the same author and date and a new id, which is placed right after the wrapper. Returns the new
    /// wrapper.
    pub(super) fn split_wrapper(
        &mut self,
        part: &Arc<str>,
        wrapper: NodeId,
        at: Option<NodeId>,
    ) -> Result<NodeId, StetError> {
        let (old_id, has_id) = {
            let tree = self.tree_ro(part)?;
            let id = rev_id(&tree.node_ref(wrapper));
            (id, id.is_some())
        };
        let new_id = if has_id { Some(self.next_id()?) } else { None };
        let wns_uri = wns(self.tree_ro(part)?);
        let tree = self.tx.part_mut(part)?;
        let tail = tree.edit().clone_subtree(wrapper)?;
        for child in child_ids(tree, tail) {
            detach(tree, child)?;
        }
        if let Some(id) = new_id {
            set_w_attr(tree, tail, &wns_uri, "id", &id.to_string())?;
        }
        if let Some(at) = at {
            let moved: Vec<NodeId> = {
                let n = tree.node_ref(wrapper);
                let mut found = Vec::new();
                let mut on = false;
                for c in n.children() {
                    if c.id() == at {
                        on = true;
                    }
                    if on {
                        found.push(c.id());
                    }
                }
                found
            };
            for child in moved {
                move_to(tree, child, tail, None)?;
            }
        }
        let parent = parent_of(tree, wrapper).ok_or_else(|| internal("a wrapper has no parent"))?;
        let next = next_sibling_of(tree, wrapper);
        insert(tree, parent, next, tail)?;
        if let (Some(old), Some(new)) = (old_id, new_id) {
            self.note_split(old, i64::from(new))?;
        }
        Ok(tail)
    }
}
