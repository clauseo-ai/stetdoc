//! What the block primitives share (design `revisions.md`, 3.2 and 3.4): the neighbours of a block in
//! its container, what a paragraph mark holds, the copy of a `w:pPr` by the copy rule, and the making
//! of a new paragraph element.

use super::compiler::{Compiler, ParaView};
use super::errors::{cross_block, internal, stale};
use super::names::{is_w, is_word_ns};
use super::props_edit::{find_chain, place};
use super::tree_ops::{create_w, detach, next_sibling_of, parent_of};
use super::types::PropElement;
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{
    BlockEntry, BlockKind, IdKind, MARK_DELETED, MARK_MOVED_FROM, Mode, NodeId, Paragraph, Story,
};
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

/// The text id that Word writes on most paragraphs and that a new paragraph gets (design 5.4).
pub(super) const TEXT_ID: &str = "77777777";

/// True for the content of a text box, whatever namespace it is in: a Strict document writes it in the one
/// that Word 2006 used (`wne:txbxContent`), and a Transitional one in `w`.
pub(super) fn is_text_box_content(n: &NodeRef<'_>) -> bool {
    n.is_element() && n.local_name() == "txbxContent"
}

/// The nearest ancestor of `node` that holds blocks: a cell, the body, a header, a footer, a note, a
/// comment or a text box.
pub(super) fn container_of(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(node).parent();
    while let Some(n) = cur {
        if is_word_ns(n.namespace())
            && matches!(
                n.local_name(),
                "tc" | "body" | "hdr" | "ftr" | "footnote" | "endnote" | "comment"
            )
            || is_text_box_content(&n)
        {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

/// True if two blocks are children of one element. A paragraph whose mark goes joins the paragraph after
/// it, and the resolver joins only paragraphs that are siblings: a paragraph at the end of a content
/// control or a revision wrapper does not join the one after the control, whatever `container_of` says.
pub(super) fn same_parent(tree: &SpanTree, a: NodeId, b: NodeId) -> bool {
    tree.node_ref(a)
        .parent()
        .zip(tree.node_ref(b).parent())
        .is_some_and(|(x, y)| x.id() == y.id())
}

/// True if content of the story that is not a block (`Story::opaque`, such as a `w:altChunk`) follows
/// `node` among its siblings before `until`, or, with no `until`, before the next sibling that is a block.
/// The text is read across such content, as if it were not there, while accepting a deleted mark joins a
/// paragraph only with its next sibling, so a mark is never deleted, nor a merge made, across it.
pub(super) fn opaque_after(
    tree: &SpanTree,
    story: &Story,
    node: NodeId,
    until: Option<NodeId>,
) -> bool {
    let mut cur = tree.node_ref(node).next_sibling();
    while let Some(n) = cur {
        let id = n.id();
        if Some(id) == until || (until.is_none() && story.index_of(id).is_some()) {
            return false;
        }
        if story.opaque().iter().any(|o| o.node == id) {
            return true;
        }
        cur = n.next_sibling();
    }
    false
}

/// The revisions that a paragraph's mark holds, by kind.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Mark {
    pub ins: Option<NodeId>,
    pub del: Option<NodeId>,
    pub move_from: Option<NodeId>,
    pub move_to: Option<NodeId>,
}

impl Mark {
    /// True if the mark holds no revision.
    pub(super) fn is_clean(&self) -> bool {
        self.ins.is_none()
            && self.del.is_none()
            && self.move_from.is_none()
            && self.move_to.is_none()
    }

    /// True if the mark is deleted or moved away.
    pub(super) fn is_deleted(&self) -> bool {
        self.del.is_some() || self.move_from.is_some()
    }
}

/// What the mark of the paragraph holds.
pub(super) fn mark_of(tree: &SpanTree, para: NodeId) -> Mark {
    let mut mark = Mark::default();
    if let Some(rpr) = find_chain(tree, para, &["pPr", "rPr"]) {
        for c in tree.node_ref(rpr).child_elements() {
            if !is_word_ns(c.namespace()) {
                continue;
            }
            match c.local_name() {
                "ins" => mark.ins = Some(c.id()),
                "del" => mark.del = Some(c.id()),
                "moveFrom" => mark.move_from = Some(c.id()),
                "moveTo" => mark.move_to = Some(c.id()),
                _ => {}
            }
        }
    }
    mark
}

/// The sibling blocks of a block: the entries before and after it in the same container.
pub(super) struct Sibs {
    pub story: Arc<Story>,
    pub index: usize,
    pub prev: Option<usize>,
    pub next: Option<usize>,
}

impl Sibs {
    /// The entry at an index.
    pub(super) fn entry(&self, i: usize) -> Option<BlockEntry> {
        self.story.blocks().get(i).copied()
    }
}

/// True if `node` is `container` or lies inside it.
fn within(tree: &SpanTree, container: NodeId, node: NodeId) -> bool {
    super::names::contains(tree, container, node)
}

impl Compiler<'_, '_> {
    /// The neighbours of the block `key` in its container. A block that is not in the document any more
    /// is `stale_ref`.
    pub(super) fn sibs(&self, key: &BlockKey) -> Result<Sibs, StetError> {
        let view = self.tx.parts();
        let part = view
            .id(key.part())
            .ok_or_else(|| internal(format!("{} is not a part", key.part())))?;
        let story = view.story(part)?;
        let tree = view.tree(part)?;
        let index = story
            .index_of(key.node())
            .ok_or_else(|| stale("the block is not in the document any more"))?;
        let Some(container) = container_of(tree, key.node()) else {
            return Err(internal("a block has no container"));
        };
        let is_sibling = |e: &BlockEntry| {
            matches!(e.kind, BlockKind::Paragraph | BlockKind::Table)
                && container_of(tree, e.node) == Some(container)
        };
        // An entry that is neither a sibling nor inside a sibling table ends the scan: it is outside the
        // container.
        let leaves = |e: &BlockEntry| {
            container_of(tree, e.node).is_none_or(|c| c != container && !within(tree, container, c))
        };
        let entries = story.blocks();
        let mut next = None;
        for (j, e) in entries.iter().enumerate().skip(index + 1) {
            if is_sibling(e) {
                next = Some(j);
                break;
            }
            if e.kind != BlockKind::Row && leaves(e) {
                break;
            }
        }
        let mut prev = None;
        for (j, e) in entries.iter().enumerate().take(index).rev() {
            if is_sibling(e) {
                prev = Some(j);
                break;
            }
            if e.kind != BlockKind::Row && leaves(e) {
                break;
            }
        }
        Ok(Sibs {
            story,
            index,
            prev,
            next,
        })
    }

    /// True if the entry is a deleted block: a paragraph whose mark is deleted (or that sits in a deleted
    /// row or cell) and whose `Final` text is empty.
    pub(super) fn is_deleted_entry(
        &self,
        part: &Arc<str>,
        e: &BlockEntry,
    ) -> Result<bool, StetError> {
        if e.kind != BlockKind::Paragraph {
            return Ok(false);
        }
        let tree = self.tree_ro(part)?;
        let marked = e.flags & (MARK_DELETED | MARK_MOVED_FROM) != 0
            || super::compiler::in_deleted_row_or_cell(tree, e.node);
        if !marked {
            return Ok(false);
        }
        let Some(p) = Paragraph::new(tree, e.node) else {
            return Ok(false);
        };
        Ok(p.map().len(Mode::Final) == 0)
    }

    /// The block entry of a table or paragraph key, as a `cross_block` error if it is a row.
    pub(super) fn expect_block(&self, s: &Sibs) -> Result<BlockEntry, StetError> {
        let e = s
            .entry(s.index)
            .ok_or_else(|| internal("a block index is out of range"))?;
        if e.kind == BlockKind::Row {
            return Err(cross_block(
                "this block is a table row. Use the row operations for rows, or name a paragraph or a table.",
            ));
        }
        Ok(e)
    }
}

/// A copy of a `w:pPr` made by the copy rule: every change-tracking element and the section break are
/// left out, wherever they are in it. A mark's `w:rPr` that held only a revision is left out too, and so
/// is the whole copy when nothing is left in it, since an empty `w:rPr` or `w:pPr` is a violation (an
/// untracked edit adds no mark that would fill it). Returns the detached copy, or `None` if it is empty.
pub(super) fn copy_ppr(tree: &mut SpanTree, ppr: NodeId) -> Result<Option<NodeId>, StetError> {
    let copy = tree.edit().clone_subtree(ppr)?;
    strip_deep(tree, copy)?;
    let doomed: Vec<NodeId> = tree
        .node_ref(copy)
        .child_elements()
        .filter(|c| is_w(c, "sectPr") || (is_w(c, "rPr") && c.child_elements().next().is_none()))
        .map(|c| c.id())
        .collect();
    for s in doomed {
        detach(tree, s)?;
    }
    if tree.node_ref(copy).child_elements().next().is_none() {
        return Ok(None);
    }
    Ok(Some(copy))
}

/// Removes the change-tracking elements below `node`, at any depth.
fn strip_deep(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
    let mut doomed = Vec::new();
    let mut stack = vec![node];
    while let Some(id) = stack.pop() {
        for c in tree.node_ref(id).child_elements() {
            if is_word_ns(c.namespace()) && super::policy::is_change_tracking(c.local_name()) {
                doomed.push(c.id());
            } else {
                stack.push(c.id());
            }
        }
    }
    for id in doomed {
        detach(tree, id)?;
    }
    Ok(())
}

/// The description of an element: its name, attributes and children, for the edits that copy properties.
pub(super) fn prop_element_of(n: &NodeRef<'_>) -> PropElement {
    PropElement {
        ns: n.namespace().to_owned(),
        local: n.local_name().to_owned(),
        attrs: n
            .attrs()
            .filter(|a| !a.is_namespace_declaration())
            .map(|a| {
                (
                    a.namespace().to_owned(),
                    a.local_name().to_owned(),
                    a.value().into_owned(),
                )
            })
            .collect(),
        children: n.child_elements().map(|c| prop_element_of(&c)).collect(),
    }
}

/// The `w14` attributes of a paragraph that a new paragraph modelled on it copies the pattern of:
/// the namespace of its `paraId`, if it has one in the Word 2010 namespace. The beta namespace of Word 2007
/// (`.../office/word/2007/5/30/wordml`, complex1_NOR) has a `paraId` but no `textId`, so a model with that
/// one gives the new paragraph neither.
pub(super) fn para_id_ns(tree: &SpanTree, para: NodeId) -> Option<String> {
    tree.node_ref(para)
        .attrs()
        .find(|a| a.local_name() == "paraId" && a.namespace() == W14_URI)
        .map(|a| a.namespace().to_owned())
}

/// The Word 2010 namespace, the only one `paraId` and `textId` are written in.
const W14_URI: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

impl Compiler<'_, '_> {
    /// A new, detached `w:p`. If the model paragraph has a `w14:paraId`, it has a new one and the usual
    /// `w14:textId` (design 5.4); no other attribute is copied.
    pub(super) fn new_paragraph_element(
        &mut self,
        part: &Arc<str>,
        model: NodeId,
    ) -> Result<NodeId, StetError> {
        let ids = match para_id_ns(self.tree_ro(part)?, model) {
            Some(ns) => Some((ns, self.tx.mint_id(IdKind::ParaId, part)?)),
            None => None,
        };
        let uri = super::names::wns(self.tree_ro(part)?);
        let tree = self.tx.part_mut(part)?;
        let p = create_w(tree, &uri, "p")?;
        if let Some((ns, id)) = ids {
            tree.edit()
                .set_attr(p, &ns, "paraId", &format!("{id:08X}"))?;
            tree.edit().set_attr(p, &ns, "textId", TEXT_ID)?;
        }
        Ok(p)
    }

    /// Adds our revision element (`ins` or `del`) to the paragraph's mark, making the `w:pPr` and its
    /// `w:rPr` if they are missing. The schema puts it first, `ins` before `del`.
    pub(super) fn add_mark_revision(
        &mut self,
        part: &Arc<str>,
        para: NodeId,
        local: &str,
    ) -> Result<(), StetError> {
        let id = self.next_id()?;
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked mark has no stamp"))?;
        let tree = self.tx.part_mut(part)?;
        let (rpr, _) = super::props_edit::ensure_chain(tree, para, &["pPr", "rPr"])?;
        let uri = super::names::wns(tree);
        let el = create_w(tree, &uri, local)?;
        super::insert_run::write_revision_attrs(tree, el, &uri, id, &stamp)?;
        place(tree, rpr, el)?;
        Ok(())
    }

    /// The paragraph view of a block that must be a paragraph that can be edited.
    pub(super) fn editable_paragraph(&self, key: &BlockKey) -> Result<ParaView, StetError> {
        let p = self.paragraph(key)?;
        self.check_editable(&p)?;
        self.check_not_wrapped_away(&p)?;
        Ok(p)
    }

    /// The paragraph after `node` in `parent`, as the next sibling node, if any.
    pub(super) fn after_node(
        &self,
        part: &Arc<str>,
        node: NodeId,
    ) -> Result<Option<NodeId>, StetError> {
        Ok(next_sibling_of(self.tree_ro(part)?, node))
    }

    /// The parent of a node.
    pub(super) fn parent_node(&self, part: &Arc<str>, node: NodeId) -> Result<NodeId, StetError> {
        parent_of(self.tree_ro(part)?, node).ok_or_else(|| internal("a block has no parent"))
    }
}
