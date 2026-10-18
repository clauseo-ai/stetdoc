//! Building the run that holds new text and putting it at a settled boundary (design `revisions.md`,
//! 4.2): the formatting source, the `w:ins` that it goes in, and coalescing with an insertion that this
//! transaction already made.

use super::compiler::{Compiler, ParaView};
use super::errors::{internal, invalid};
use super::gap::Boundary;
use super::names::{is_blank, is_w, wns};
use super::tree_ops::{
    copy_attrs, create_w, ensure_preserve, insert, move_to, rpr_of, set_w_attr,
    strip_change_tracking,
};
use crate::api::error::StetError;
use crate::wordml::{ItemKind, NodeId};
use crate::xml::SpanTree;
use std::sync::Arc;

impl Compiler<'_, '_> {
    /// Builds a run holding `text`: a `w:t` for each stretch of text, a `w:tab` for each tab and a
    /// `w:br` for each line break. `format` is the `w:rPr` to copy, by the copy rule.
    pub(super) fn build_run(
        &mut self,
        part: &Arc<str>,
        text: &str,
        format: Option<NodeId>,
    ) -> Result<NodeId, StetError> {
        let wns_uri = wns(self.tree_ro(part)?);
        let tree = self.tx.part_mut(part)?;
        let run = create_w(tree, &wns_uri, "r")?;
        if let Some(src) = format {
            let rpr = tree.edit().clone_subtree(src)?;
            strip_change_tracking(tree, rpr)?;
            if tree.node_ref(rpr).first_child().is_some() {
                tree.edit().append_child(run, rpr)?;
            }
        }
        let mut stretch = String::new();
        let flush = |tree: &mut SpanTree, stretch: &mut String| -> Result<(), StetError> {
            if stretch.is_empty() {
                return Ok(());
            }
            let t = create_w(tree, &wns_uri, "t")?;
            let node = tree.edit().create_text(stretch)?;
            tree.edit().append_child(t, node)?;
            tree.edit().append_child(run, t)?;
            ensure_preserve(tree, t)?;
            stretch.clear();
            Ok(())
        };
        for c in text.chars() {
            match c {
                '\t' => {
                    flush(tree, &mut stretch)?;
                    let e = create_w(tree, &wns_uri, "tab")?;
                    tree.edit().append_child(run, e)?;
                }
                '\n' => {
                    flush(tree, &mut stretch)?;
                    let e = create_w(tree, &wns_uri, "br")?;
                    tree.edit().append_child(run, e)?;
                }
                other => stretch.push(other),
            }
        }
        flush(tree, &mut stretch)?;
        Ok(run)
    }

    /// Builds a run holding `text` in a new paragraph, formatted like `mark`, the `w:rPr` of a paragraph
    /// mark. A mark is often hidden (`w:vanish`, `w:specVanish`) where the paragraph's text is not, so the
    /// copy drops them: a line added after a paragraph with a hidden mark is not hidden text. It drops
    /// `w:oMath` too, which says that the mark is an equation's and is a property of a mark only: Word's
    /// schema has no place for it in a run's properties (tdf171433).
    pub(super) fn build_mark_run(
        &mut self,
        part: &Arc<str>,
        text: &str,
        mark: Option<NodeId>,
    ) -> Result<NodeId, StetError> {
        let run = self.build_run(part, text, mark)?;
        let tree = self.tx.part_mut(part)?;
        if let Some(rpr) = rpr_of(tree, run) {
            let hidden: Vec<NodeId> = tree
                .node_ref(rpr)
                .child_elements()
                .filter(|c| is_w(c, "vanish") || is_w(c, "specVanish") || is_w(c, "oMath"))
                .map(|c| c.id())
                .collect();
            for id in hidden {
                super::tree_ops::detach(tree, id)?;
            }
            if tree.node_ref(rpr).child_elements().next().is_none() {
                super::tree_ops::detach(tree, rpr)?;
            }
        }
        Ok(run)
    }

    /// Puts a new run at a settled boundary. Tracked, it goes in a `w:ins`: the one just before it, if
    /// this transaction made it and nothing but markers stands between, otherwise a new one. The
    /// markers between move into the wrapper with the run, so each keeps its place between the old
    /// text and the new. Returns the wrapper, if there is one.
    pub(super) fn place_run(
        &mut self,
        part: &Arc<str>,
        b: Boundary,
        run: NodeId,
    ) -> Result<Option<NodeId>, StetError> {
        if !self.tracked() {
            let tree = self.tx.part_mut(part)?;
            insert(tree, b.parent, b.before, run)?;
            return Ok(None);
        }
        // Inside an insertion this transaction made, the run is just a child of it.
        let inside_created = {
            let tree = self.tree_ro(part)?;
            let parent = tree.node_ref(b.parent);
            is_w(&parent, "ins") && self.is_created_node(tree, b.parent)
        };
        if inside_created {
            let tree = self.tx.part_mut(part)?;
            insert(tree, b.parent, b.before, run)?;
            return Ok(None);
        }
        // The previous sibling that is not a marker, and the markers between.
        let (previous, markers) = {
            let tree = self.tree_ro(part)?;
            let mut markers = Vec::new();
            let mut cur = match b.before {
                Some(id) => tree.node_ref(id).prev_sibling(),
                None => tree.node_ref(b.parent).last_child(),
            };
            let mut found = None;
            while let Some(c) = cur {
                if super::carve::is_transparent(&c) {
                    if !is_blank(&c) {
                        markers.push(c.id());
                    }
                    cur = c.prev_sibling();
                } else {
                    found = Some(c.id());
                    break;
                }
            }
            markers.reverse();
            let reusable =
                found.filter(|&f| is_w(&tree.node_ref(f), "ins") && self.is_created_node(tree, f));
            (reusable, markers)
        };
        if let Some(prev) = previous {
            let tree = self.tx.part_mut(part)?;
            for m in markers {
                move_to(tree, m, prev, None)?;
            }
            tree.edit().append_child(prev, run)?;
            return Ok(Some(prev));
        }
        // Right before an insertion that this transaction made: the run becomes its first child.
        let next_created = {
            let tree = self.tree_ro(part)?;
            b.before
                .filter(|&n| is_w(&tree.node_ref(n), "ins") && self.is_created_node(tree, n))
        };
        if let Some(next) = next_created {
            let tree = self.tx.part_mut(part)?;
            let first = tree.node_ref(next).first_child().map(|c| c.id());
            insert(tree, next, first, run)?;
            return Ok(Some(next));
        }
        let id = self.next_id()?;
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked insertion has no stamp"))?;
        let wns_uri = wns(self.tree_ro(part)?);
        let tree = self.tx.part_mut(part)?;
        let ins = create_w(tree, &wns_uri, "ins")?;
        write_revision_attrs(tree, ins, &wns_uri, id, &stamp)?;
        tree.edit().append_child(ins, run)?;
        insert(tree, b.parent, b.before, ins)?;
        Ok(Some(ins))
    }

    /// The `w:rPr` to copy for new text next to item `near`: the nearest text on the left in `Final`,
    /// then on the right, then the paragraph mark's. Atoms give no formatting.
    pub(super) fn format_source(
        &self,
        p: &ParaView,
        left: Option<usize>,
        right: Option<usize>,
    ) -> Result<Option<NodeId>, StetError> {
        let tree = self.tree_ro(&p.part)?;
        let items = p.map.items();
        let text_run = |i: usize| -> Option<NodeId> {
            let it = items.get(i)?;
            (it.kind == ItemKind::Text && it.vis.in_mode(crate::wordml::Mode::Final))
                .then_some(it.run)
                .flatten()
        };
        let pick = |from: Option<usize>, step: isize| -> Option<NodeId> {
            let mut i = from? as isize;
            while i >= 0 && (i as usize) < items.len() {
                if let Some(run) = text_run(i as usize) {
                    return Some(run);
                }
                i += step;
            }
            None
        };
        let run = pick(left, -1).or_else(|| pick(right, 1));
        if let Some(run) = run {
            return Ok(rpr_of(tree, run));
        }
        let para = tree.node_ref(p.node);
        let mark = para
            .child_elements()
            .find(|c| is_w(c, "pPr"))
            .and_then(|ppr| ppr.child_elements().find(|c| is_w(c, "rPr")))
            .map(|r| r.id());
        Ok(mark)
    }
}

/// Writes `w:id`, `w:author` and `w:date`, in that order, on a revision element.
pub(super) fn write_revision_attrs(
    tree: &mut SpanTree,
    el: NodeId,
    wns_uri: &str,
    id: u32,
    stamp: &super::stamp::RevStamp,
) -> Result<(), StetError> {
    set_w_attr(tree, el, wns_uri, "id", &id.to_string())?;
    set_w_attr(tree, el, wns_uri, "author", stamp.author())?;
    set_w_attr(tree, el, wns_uri, "date", stamp.date())
}

/// The characters a new text may hold, after the diff: nothing that XML cannot carry and nothing that
/// stands for an atom.
pub(super) fn check_new_text(text: &str) -> Result<(), StetError> {
    for c in text.chars() {
        let atom = matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}');
        let carriable = matches!(c, '\t' | '\n' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}');
        if atom {
            return Err(invalid(format!(
                "the new text holds U+{:04X}, which stands for a drawing, a note reference, an equation or a break that this edit would have to create",
                u32::from(c)
            )));
        }
        if !carriable {
            return Err(invalid(format!(
                "the new text holds U+{:04X}, which cannot be written in a Word document",
                u32::from(c)
            )));
        }
    }
    Ok(())
}

/// Copies the attributes of a revision wrapper to a new one, without its id.
pub(super) fn copy_wrapper(tree: &mut SpanTree, from: NodeId, to: NodeId) -> Result<(), StetError> {
    copy_attrs(tree, from, to)
}
