//! `mark_inserted` and `place_deleted`: whole blocks of the revised document marked inserted, and whole blocks from the
//! original placed as deleted (design `compare.md`, 6.1 and 6.2).
//!
//! An inserted paragraph is written as one shape: a mark inserted and the content in `w:ins`, self-contained, so that rejecting removes
//! it whatever its neighbours. An inserted row has `w:ins` in its `w:trPr` and the same in each paragraph of its cells (Word
//! writes no `cellIns` for a new row, `FA/007`). A deleted block is a copy of the original's, put beside a revised block, whose
//! content is in `w:del` and whose mark is deleted; a deleted row has `w:del` in its `w:trPr` and every paragraph deleted,
//! each cell's last one included (`FA/006`).
//!
//! A container that ends with inserted paragraphs is given a live last mark by `redline_ends`.
//!
//! A run of deleted paragraphs that ends its container, with nothing live after it, is `RP040`'s shape: the last live
//! paragraph before the run takes a deleted mark, and the last imported paragraph keeps a live mark with its content deleted.
//! That paragraph is given the live paragraph's properties and mark properties, with its own in a `pPrChange` and an
//! `rPrChange`, so that accepting joins them into a paragraph that has the live one's properties, and rejecting restores the
//! original paragraphs and their marks.

use super::compiler::Compiler;
use super::errors::{internal, invalid, stale};
use super::insert_run::write_revision_attrs;
use super::names::is_w;
use super::names::wns;
use super::outcome::{Effect, Origin};
use super::props_edit::{
    ensure_chain, find_chain, held, held_by_snapshot, place, snapshot as snapshot_of,
};
use super::redline_types::Imported;
use super::redline_wrap::{Rev, blocks_below};
use super::tree_ops::{child_element_ids, create_w, detach, insert, next_sibling_of, parent_of};
use super::types::Place;
use crate::api::error::StetError;
use crate::wordml::{BlockKey, NodeId};
use std::sync::Arc;

/// A container whose properties are replaced: the path to it, the change element, the children that stay, and the ones of the
/// live paragraph that are not copied.
type Step<'a> = (&'a [&'a str], &'a str, &'a [&'a str], &'a [&'a str]);

impl Compiler<'_, '_> {
    /// Marks existing blocks inserted: a paragraph (its mark and all its content), a row (its `w:trPr` and the paragraphs of
    /// its cells), or a table (all its rows). A paragraph that carries a section break keeps its mark and the break, and only
    /// its content is inserted.
    pub(crate) fn mark_inserted(&mut self, blocks: &[BlockKey]) -> Result<Effect, StetError> {
        self.begin();
        let res = (|| {
            for block in blocks {
                let part = block.part().clone();
                let kind = {
                    let tree = self.tree_ro(&part)?;
                    let n = tree.node_ref(block.node());
                    if is_w(&n, "p") {
                        'p'
                    } else if is_w(&n, "tr") {
                        'r'
                    } else if is_w(&n, "tbl") {
                        't'
                    } else {
                        return Err(stale("the block is not a paragraph, row or table any more"));
                    }
                };
                match kind {
                    'p' => {
                        let sect =
                            find_chain(self.tree_ro(&part)?, block.node(), &["pPr", "sectPr"])
                                .is_some();
                        self.mark_paragraph(&part, block.node(), Rev::Ins, !sect)?;
                    }
                    'r' => self.mark_row(&part, block.node(), Rev::Ins)?,
                    _ => {
                        let (rows, paragraphs) = blocks_below(self.tree_ro(&part)?, block.node());
                        for r in rows {
                            self.add_row_rev(&part, r, Rev::Ins)?;
                            self.touch(&BlockKey::new(part.clone(), r));
                        }
                        for p in paragraphs {
                            let sect =
                                find_chain(self.tree_ro(&part)?, p, &["pPr", "sectPr"]).is_some();
                            self.mark_paragraph(&part, p, Rev::Ins, !sect)?;
                        }
                        self.touch(block);
                    }
                }
            }
            Ok(())
        })();
        match res {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    /// Marks the content of existing paragraphs inserted and leaves their marks alone, as `mark_inserted` does for a paragraph
    /// that carries a section break: rejecting removes the content and keeps the paragraph, so that what no tracked change can
    /// carry (an equation) stays in a paragraph of its own and is not joined to the next one.
    pub(crate) fn mark_content_inserted(
        &mut self,
        blocks: &[BlockKey],
    ) -> Result<Effect, StetError> {
        for block in blocks {
            let tree = self.tree_ro(block.part())?;
            if parent_of(tree, block.node()).is_none() || !is_w(&tree.node_ref(block.node()), "p") {
                return Err(invalid(
                    "only a paragraph in the document has its content marked inserted",
                ));
            }
        }
        self.begin();
        let res = (|| {
            for block in blocks {
                self.mark_paragraph(block.part(), block.node(), Rev::Ins, false)?;
            }
            Ok(())
        })();
        match res {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    /// Places detached blocks that were copied from another package beside `anchor`, each as a tracked deletion, in order.
    /// `trailing` is the last live paragraph before the run when nothing live follows it in its container.
    pub(crate) fn place_deleted(
        &mut self,
        anchor: &BlockKey,
        place: Place,
        blocks: &[Imported],
        trailing: Option<&BlockKey>,
    ) -> Result<Effect, StetError> {
        self.check_placeable(anchor, blocks, trailing)?;
        self.begin();
        match self.place_deleted_inner(anchor, place, blocks, trailing) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    /// What `place_deleted` would refuse, found before it writes: the anchor is in the story, each block is a detached paragraph,
    /// row or table that fits the anchor's container (a row in a table, anything else outside one), and `trailing` is a
    /// paragraph of the same container.
    fn check_placeable(
        &self,
        anchor: &BlockKey,
        blocks: &[Imported],
        trailing: Option<&BlockKey>,
    ) -> Result<(), StetError> {
        let tree = self.tree_ro(anchor.part())?;
        let parent = parent_of(tree, anchor.node())
            .ok_or_else(|| stale("the anchor is not in the document any more"))?;
        let in_table = is_w(&tree.node_ref(parent), "tbl");
        if in_table != is_w(&tree.node_ref(anchor.node()), "tr") {
            return Err(invalid("the anchor is not a block of its container"));
        }
        if blocks.is_empty() {
            return Err(invalid("nothing to place"));
        }
        for (i, b) in blocks.iter().enumerate() {
            let n = tree.node_ref(b.node);
            if n.parent().is_some() || n.id() == tree.root().id() {
                return Err(invalid("a block to place is not detached"));
            }
            if blocks.iter().take(i).any(|x| x.node == b.node) {
                return Err(invalid("a block is placed twice"));
            }
            let fits = if in_table {
                is_w(&n, "tr")
            } else {
                is_w(&n, "p") || is_w(&n, "tbl")
            };
            if !fits {
                return Err(invalid(
                    "a placed block does not fit the container of its anchor",
                ));
            }
        }
        if let Some(t) = trailing
            && (t.part() != anchor.part()
                || parent_of(tree, t.node()) != Some(parent)
                || !is_w(&tree.node_ref(t.node()), "p"))
        {
            return Err(invalid(
                "the trailing paragraph is not a paragraph beside the anchor",
            ));
        }
        Ok(())
    }

    fn place_deleted_inner(
        &mut self,
        anchor: &BlockKey,
        place: Place,
        blocks: &[Imported],
        trailing: Option<&BlockKey>,
    ) -> Result<(), StetError> {
        let part = anchor.part().clone();
        let (parent, before) = {
            let tree = self.tree_ro(&part)?;
            let parent = parent_of(tree, anchor.node())
                .ok_or_else(|| stale("the anchor is not in the document any more"))?;
            let before = match place {
                Place::Before => Some(anchor.node()),
                Place::After => next_sibling_of(tree, anchor.node()),
            };
            (parent, before)
        };
        for imp in blocks {
            let tree = self.tx.part_mut(&part)?;
            strip_section(tree, imp.node)?;
            insert(tree, parent, before, imp.node)?;
        }
        let all_paragraphs = {
            let tree = self.tree_ro(&part)?;
            blocks.iter().all(|b| is_w(&tree.node_ref(b.node), "p"))
        };
        let live = trailing.filter(|_| all_paragraphs && !blocks.is_empty());
        for (i, imp) in blocks.iter().enumerate() {
            let last = i + 1 == blocks.len();
            self.mark_whole_deleted(&part, imp, last && live.is_some())?;
        }
        if let (Some(live_key), Some(last)) = (live, blocks.last()) {
            self.take_the_mark(&part, live_key, last.node)?;
        }
        Ok(())
    }

    /// Marks a block that is in the tree as deleted whole, and records it as new, with the block of the other package it is a copy
    /// of. `keep_mark` leaves a paragraph's mark live: the last paragraph of a run that ends its container.
    pub(super) fn mark_whole_deleted(
        &mut self,
        part: &Arc<str>,
        imp: &Imported,
        keep_mark: bool,
    ) -> Result<(), StetError> {
        let key = BlockKey::new(Arc::clone(part), imp.node);
        let kind = {
            let tree = self.tree_ro(part)?;
            let n = tree.node_ref(imp.node);
            if is_w(&n, "p") {
                'p'
            } else if is_w(&n, "tr") {
                'r'
            } else if is_w(&n, "tbl") {
                't'
            } else {
                return Err(invalid(
                    "only a paragraph, a row or a table is placed as deleted",
                ));
            }
        };
        match kind {
            'p' if keep_mark => self.wrap_content(part, imp.node, Rev::Del)?,
            'p' => self.mark_paragraph(part, imp.node, Rev::Del, true)?,
            'r' => {
                self.mark_row(part, imp.node, Rev::Del)?;
                self.touch_table_of(part, imp.node);
            }
            _ => {
                let (rows, paragraphs) = blocks_below(self.tree_ro(part)?, imp.node);
                for r in rows {
                    self.add_row_rev(part, r, Rev::Del)?;
                }
                for p in paragraphs {
                    self.mark_paragraph(part, p, Rev::Del, true)?;
                }
            }
        }
        self.fx.new_blocks.push(key.clone());
        self.fx.origins.push(Origin {
            block: key.clone(),
            from: Vec::new(),
        });
        self.fx.imported.push((key, imp.source.clone()));
        Ok(())
    }

    /// The table that a placed row is in is a changed block: its list of rows changed.
    fn touch_table_of(&mut self, part: &Arc<str>, row: NodeId) {
        let table = self.tree_ro(part).ok().and_then(|t| parent_of(t, row));
        if let Some(t) = table {
            self.touch(&BlockKey::new(Arc::clone(part), t));
        }
    }

    /// The `RP040` shape: `live` takes a deleted mark, and `last` (an imported paragraph whose content is deleted) gets the
    /// properties of `live`, with its own in a `pPrChange` and an `rPrChange` in the mark. A part that is already equal is
    /// not written, because a change whose snapshot equals the properties is refused.
    fn take_the_mark(
        &mut self,
        part: &Arc<str>,
        live: &BlockKey,
        last: NodeId,
    ) -> Result<(), StetError> {
        self.add_mark_revision(part, live.node(), "del")?;
        self.touch(live);
        // (path, change element, children of the container that stay, children of the live container that are not copied)
        let steps: [Step; 2] = [
            (
                &["pPr"],
                "pPrChange",
                &["rPr", "sectPr", "pPrChange"],
                &["rPr", "sectPr", "pPrChange"],
            ),
            (
                &["pPr", "rPr"],
                "rPrChange",
                &[],
                &["ins", "del", "moveFrom", "moveTo", "rPrChange"],
            ),
        ];
        for (path, change, stay, not_copied) in steps {
            let id = self.next_id()?;
            let stamp = self
                .stamp()
                .cloned()
                .ok_or_else(|| internal("a tracked change has no stamp"))?;
            let tree = self.tx.part_mut(part)?;
            let source = find_chain(tree, live.node(), path);
            let (own, _) = ensure_chain(tree, last, path)?;
            let old = snapshot_of(tree, own, change)?;
            for k in child_element_ids(tree, own) {
                if !stay.contains(&tree.node_ref(k).local_name()) {
                    detach(tree, k)?;
                }
            }
            if let Some(src) = source {
                for k in child_element_ids(tree, src) {
                    if not_copied.contains(&tree.node_ref(k).local_name()) {
                        continue;
                    }
                    // a property that the live paragraph lists twice (a tool's slip) is copied once
                    let name = tree.node_ref(k).local_name().to_owned();
                    if tree
                        .node_ref(own)
                        .child_elements()
                        .any(|x| x.local_name() == name)
                    {
                        continue;
                    }
                    let copy = tree.edit().clone_subtree(k)?;
                    place(tree, own, copy)?;
                }
            }
            if held(tree, own, change) == held_by_snapshot(tree, old) {
                // nothing differs: no change is written, and the container that was made for it is not left empty
                if tree.node_ref(own).child_elements().next().is_none() {
                    detach(tree, own)?;
                }
                continue;
            }
            let uri = wns(tree);
            let ch = create_w(tree, &uri, change)?;
            write_revision_attrs(tree, ch, &uri, id, &stamp)?;
            tree.edit().append_child(ch, old)?;
            place(tree, own, ch)?;
        }
        let tree = self.tx.part_mut(part)?;
        if let Some(ppr) = find_chain(tree, last, &["pPr"])
            && tree.node_ref(ppr).child_elements().next().is_none()
        {
            detach(tree, ppr)?;
        }
        Ok(())
    }
}

/// Removes a section break from the pPr of an imported paragraph: a copy never carries one.
fn strip_section(tree: &mut crate::xml::SpanTree, node: NodeId) -> Result<(), StetError> {
    let (_, paragraphs) = blocks_below(tree, node);
    for p in paragraphs {
        if let Some(sect) = find_chain(tree, p, &["pPr", "sectPr"]) {
            detach(tree, sect)?;
        }
    }
    Ok(())
}
