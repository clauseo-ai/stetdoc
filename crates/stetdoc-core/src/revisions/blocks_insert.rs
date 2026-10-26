//! `insert_block` and `split_block`: new paragraphs in the two shapes Word writes (design
//! `revisions.md`, 3.2.1).
//!
//! **Y** (Enter in a paragraph that has a successor or ends its container, and every split but one before the text): the paragraph that was there keeps
//! its element, its text before the cut and a copy of its properties with an inserted mark. A new
//! paragraph after it holds the rest and the **original** mark and properties. Rejecting the mark joins the
//! two halves again, and the later paragraph's properties are the original's.
//!
//! **X** (a paragraph that stands alone: inserted before another, cut off before a paragraph's text, or where Y cannot go): a new
//! paragraph with an inserted mark and all its content inserted. It has a **twin**, the paragraph after it,
//! which rejecting its mark joins it into. Its properties are a copy of the twin's, and when an edit makes
//! them differ, a `pPrChange` aligns it to the twin, so that Word gets the same answer under either rule
//! for which paragraph's properties survive a join (3.2.4).
//!
//! Untracked, a new paragraph has no marks of ours.

use super::blocks::{copy_ppr, mark_of};
use super::compiler::Compiler;
use super::errors::{internal, invalid, readonly};
use super::gap::{Boundary, SettleMode};
use super::insert_run::check_new_text;
use super::names::{is_w, wns};
use super::outcome::{Effect, Origin};
use super::props::Direct;
use super::props_edit::find_chain;
use super::text::{check_carriable, final_starts};
use super::tree_ops::{create_w, insert, move_to, next_sibling_of, parent_of};
use super::types::{InsertRun, NewParagraph, Place, PropEdit, PropElement, RunStep};
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{BlockKind, Mode, NodeId, revision_at};
use crate::xml::SpanTree;
use std::sync::Arc;

/// How the new paragraph of an insertion is built.
enum Shape {
    /// The paragraph that was there is cut after its content: the new paragraph is the second half.
    Y { anchor: NodeId },
    /// A self-contained paragraph before `before`, or after `after` when nothing follows.
    X {
        twin: Option<NodeId>,
        model: Option<NodeId>,
        before: Option<NodeId>,
        after: Option<NodeId>,
    },
}

/// The block an insertion goes next to, with the blocks of its container before and after it.
struct Around {
    node: NodeId,
    kind: BlockKind,
    next: Option<(NodeId, BlockKind)>,
    prev: Option<(NodeId, BlockKind)>,
    /// The next block of the container, in whatever parent.
    next_any: Option<(NodeId, BlockKind)>,
}

/// True for a revision element that can hold whole blocks: an insertion, a deletion or either side of a move.
fn is_block_wrapper(n: &crate::xml::NodeRef<'_>) -> bool {
    ["ins", "del", "moveFrom", "moveTo"]
        .iter()
        .any(|name| is_w(n, name))
}

/// True for a sibling that is content of a block wrapper, not a range marker beside it.
fn is_wrapped_content(n: &crate::xml::NodeRef<'_>) -> bool {
    n.is_element()
        && (is_block_wrapper(n)
            || ["p", "tbl", "sdt", "customXml", "altChunk"]
                .iter()
                .any(|name| is_w(n, name)))
}

/// The outermost revision wrapper that holds `block` as a whole block, when the new block can go next to it: `block` is
/// its first block (for `Before`) or its last (for `After`), and so at each level of nested wrappers. A position anywhere
/// else inside a wrapper would put a new paragraph inside another author's change, or inside a move whose text would then
/// change, and is `readonly_region`. `None` when the block is in no such wrapper.
fn wrapper_exit(tree: &SpanTree, block: NodeId, place: Place) -> Result<Option<NodeId>, StetError> {
    let mut out = None;
    let mut cur = tree.node_ref(block);
    while let Some(parent) = cur.parent() {
        if !is_block_wrapper(&parent) {
            break;
        }
        let mut beyond = match place {
            Place::Before => cur.prev_sibling(),
            Place::After => cur.next_sibling(),
        };
        while let Some(s) = beyond {
            if is_wrapped_content(&s) {
                return Err(readonly(format!(
                    "this position is inside a tracked {} that holds whole paragraphs. Insert before its first block or after its last, or resolve the change first.",
                    match parent.local_name() {
                        "moveFrom" | "moveTo" => "move",
                        "del" => "deletion",
                        _ => "insertion",
                    }
                )));
            }
            beyond = match place {
                Place::Before => s.prev_sibling(),
                Place::After => s.next_sibling(),
            };
        }
        out = Some(parent.id());
        cur = parent;
    }
    Ok(out)
}

/// The edit that makes a paragraph's base properties exactly `w:pStyle` with this id: it sets the style
/// and clears every other base property that the paragraph has.
fn style_edit(tree: &SpanTree, para: NodeId, style: &str, extra: &PropEdit) -> PropEdit {
    let ns = tree.node_ref(para).namespace().to_owned();
    let mut clear: Vec<(String, String)> = Vec::new();
    if let Some(ppr) = find_chain(tree, para, &["pPr"]) {
        for c in tree.node_ref(ppr).child_elements() {
            let name = c.local_name();
            if matches!(name, "rPr" | "sectPr" | "pPrChange" | "pStyle") {
                continue;
            }
            clear.push((c.namespace().to_owned(), name.to_owned()));
        }
    }
    clear.extend(extra.clear.iter().cloned());
    let mut set: Vec<PropElement> = vec![PropElement {
        ns: ns.clone(),
        local: "pStyle".to_owned(),
        attrs: vec![(ns, "val".to_owned(), style.to_owned())],
        children: Vec::new(),
    }];
    set.extend(extra.set.iter().cloned());
    PropEdit { set, clear }
}

/// True if the paragraph's `w:pPr` holds a pending `w:pPrChange` that is not the author's.
fn foreign_ppr_change(tree: &SpanTree, para: NodeId, author: &str) -> bool {
    find_chain(tree, para, &["pPr"])
        .and_then(|ppr| {
            tree.node_ref(ppr)
                .child_elements()
                .find(|c| is_w(c, "pPrChange"))
        })
        .and_then(|c| revision_at(&c))
        .is_some_and(|r| r.author.as_deref() != Some(author))
}

impl Compiler<'_, '_> {
    /// Inserts a paragraph before or after `anchor`, a paragraph or a table.
    pub(crate) fn insert_block(
        &mut self,
        anchor: &BlockKey,
        place: Place,
        spec: &NewParagraph,
    ) -> Result<Effect, StetError> {
        self.insert_block_in(&mut InsertRun::default(), anchor, place, spec)
    }

    /// Inserts a paragraph as `insert_block` does, as part of `run`. A paragraph that goes after the one the
    /// previous insertion of the run made, which was made next to a paragraph, takes the next block of its
    /// container from the run instead of the story, and needs no check of the fields open there: none are,
    /// since a new paragraph holds no field markup and the depth after its anchor was checked.
    pub(crate) fn insert_block_in(
        &mut self,
        run: &mut InsertRun,
        anchor: &BlockKey,
        place: Place,
        spec: &NewParagraph,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.insert_block_inner(run, anchor, place, spec) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    /// The block `anchor` and the blocks before and after it in its container, read from the story. The next
    /// one only when it is in the same parent.
    fn around(&self, anchor: &BlockKey) -> Result<Around, StetError> {
        let sibs = self.sibs(anchor)?;
        let entry = self.expect_block(&sibs)?;
        let node_of = |i: Option<usize>| i.and_then(|i| sibs.entry(i));
        // After the last block of a content control, the next block of the story is in another container:
        // the new paragraph goes right after the anchor in the anchor's own, with no twin to join.
        let next = {
            let tree = self.tree_ro(anchor.part())?;
            node_of(sibs.next).filter(|e| super::blocks::same_parent(tree, entry.node, e.node))
        };
        Ok(Around {
            node: entry.node,
            kind: entry.kind,
            next: next.map(|e| (e.node, e.kind)),
            prev: node_of(sibs.prev).map(|e| (e.node, e.kind)),
            next_any: node_of(sibs.next).map(|e| (e.node, e.kind)),
        })
    }

    fn insert_block_inner(
        &mut self,
        run: &mut InsertRun,
        anchor: &BlockKey,
        place: Place,
        spec: &NewParagraph,
    ) -> Result<(), StetError> {
        check_carriable(&spec.text)?;
        check_new_text(&spec.text)?;
        let part = anchor.part().clone();
        let carried = run
            .last
            .take()
            .filter(|step| step.settled && place == Place::After && step.made == *anchor);
        let entry = match &carried {
            Some(step) => Around {
                node: anchor.node(),
                kind: BlockKind::Paragraph,
                next: step.follows,
                prev: None,
                next_any: step.follows,
            },
            None => self.around(anchor)?,
        };
        // An anchor inside a revision wrapper that holds whole blocks: the new paragraph goes outside the wrapper, next to
        // it, when the anchor is the wrapper's first block (before) or its last (after), and is refused elsewhere.
        let exit = wrapper_exit(self.tree_ro(&part)?, anchor.node(), place)?;
        if super::compiler::in_deleted_row_or_cell(self.tree_ro(&part)?, anchor.node()) {
            return Err(readonly(
                "this block is in a row that a tracked change deletes, so nothing can be inserted next to it. Reject that change first.",
            ));
        }
        // Out of a wrapper, the block that follows is the one after the wrapper, in the wrapper's own parent.
        let next = match (exit, place) {
            (Some(w), Place::After) => {
                let tree = self.tree_ro(&part)?;
                entry
                    .next_any
                    .filter(|e| super::blocks::same_parent(tree, w, e.0))
            }
            _ => entry.next,
        };
        let next_para = next.filter(|e| e.1 == BlockKind::Paragraph).map(|e| e.0);
        let prev_para = entry
            .prev
            .filter(|e| e.1 == BlockKind::Paragraph)
            .map(|e| e.0);
        let is_para = entry.kind == BlockKind::Paragraph;
        // A paragraph that a field's result runs into, or out of, has the field around its start or its end:
        // a paragraph inserted there would be inside the field, whose result is read-only (as for a split).
        if is_para && carried.is_none() {
            let p = self.paragraph(anchor)?;
            let depth = match place {
                Place::Before => self.field_depth_after(&p, None),
                Place::After => self.field_depth_after(&p, p.map.items().len().checked_sub(1)),
            };
            if depth > 0 {
                return Err(readonly(
                    "this position is inside a field, whose result is read-only. Insert before or after the paragraphs that the field spans.",
                ));
            }
        }
        let edit_wanted = !spec.props.is_empty() || spec.style.is_some();
        let foreign = match self.stamp() {
            Some(stamp) => foreign_ppr_change(self.tree_ro(&part)?, entry.node, stamp.author()),
            None => false,
        };
        let anchor_mark = mark_of(self.tree_ro(&part)?, entry.node);
        let anchor_mark_clean = is_para && anchor_mark.is_clean();
        // After the last paragraph of its container (a cell, a note, a comment, the body) the Enter shape too, as Word
        // has it: the anchor's mark is the container's end, which no rejection can remove, so a paragraph with an
        // inserted mark of its own after it would be left empty by Word's Reject All. An inserted mark may move with
        // the original properties to the new paragraph, which is then the one that ends the container.
        let at_end = is_para
            && exit.is_none()
            && crate::wordml::ends_container(self.tree_ro(&part)?, entry.node);
        let movable = anchor_mark.del.is_none()
            && anchor_mark.move_from.is_none()
            && anchor_mark.move_to.is_none();
        let shape = match (self.tracked(), place) {
            (true, Place::After)
                if is_para
                    && exit.is_none()
                    && ((next_para.is_some() && anchor_mark_clean) || (at_end && movable))
                    && !(edit_wanted && foreign) =>
            {
                Shape::Y { anchor: entry.node }
            }
            // Where Y cannot go at the end, a paragraph of its own would be the one Reject All leaves empty.
            (true, Place::After) if at_end => {
                return Err(readonly(if movable {
                    "the last paragraph of this container holds another author's pending change of paragraph properties, which a new paragraph after it would have to take over. Insert it without properties, or resolve that change first."
                } else {
                    "the mark of the last paragraph of this container is a pending deletion or move, so nothing can follow it. Resolve that change first."
                }));
            }
            (_, Place::Before) => Shape::X {
                twin: if is_para { Some(entry.node) } else { None },
                model: if is_para {
                    Some(entry.node)
                } else {
                    prev_para.or(next_para)
                },
                before: Some(exit.unwrap_or(entry.node)),
                after: None,
            },
            (tracked, Place::After) => Shape::X {
                twin: if tracked { next_para } else { None },
                // Untracked there is no twin to join into, and the new paragraph follows the anchor.
                model: if tracked {
                    next_para.or(if is_para { Some(entry.node) } else { prev_para })
                } else if is_para {
                    Some(entry.node)
                } else {
                    prev_para.or(next_para)
                },
                before: next.map(|e| e.0),
                after: Some(exit.unwrap_or(entry.node)),
            },
        };
        let (new, from, cut_anchor) = match shape {
            Shape::Y { anchor } => (self.make_y(&part, anchor, None, spec)?, Some(anchor), true),
            Shape::X {
                twin,
                model,
                before,
                after,
            } => {
                let n = self.make_x(&part, twin, model, before, after, spec)?;
                (n, twin.or(model), false)
            }
        };
        // A paragraph made with no model was built from nothing, and says so with an empty list.
        let key = BlockKey::new(part.clone(), new);
        // The new paragraph sits right before the block that followed its anchor, or right before the anchor.
        // Before a wrapper, the anchor inside it is in another parent, so nothing follows in the same one.
        let follows = match place {
            Place::After => next,
            Place::Before if exit.is_some() => None,
            Place::Before => Some((entry.node, entry.kind)),
        };
        run.last = Some(RunStep {
            made: key.clone(),
            follows,
            settled: is_para,
        });
        self.fx.new_blocks.push(key.clone());
        self.fx.origins.push(Origin {
            block: key,
            from: from.map(|n| BlockKey::new(part, n)).into_iter().collect(),
        });
        if cut_anchor {
            self.touch(anchor);
        }
        Ok(())
    }

    /// Cuts the paragraph `anchor` at the boundary `cut` (the end of its content if `None`) and returns the
    /// new second paragraph, which holds what follows the cut, the original `w:pPr` and `text` as new
    /// content. The first keeps its element and gets a copy of the properties, with our mark if tracked.
    fn make_y(
        &mut self,
        part: &Arc<str>,
        anchor: NodeId,
        cut: Option<Boundary>,
        spec: &NewParagraph,
    ) -> Result<NodeId, StetError> {
        let p2 = self.new_paragraph_element(part, anchor)?;
        let tree = self.tx.part_mut(part)?;
        // The properties: the original goes to the second paragraph, a copy stays.
        let ppr = find_chain(tree, anchor, &["pPr"]);
        if let Some(ppr) = ppr {
            let copy = copy_ppr(tree, ppr)?;
            super::tree_ops::detach(tree, ppr)?;
            insert(tree, p2, None, ppr)?;
            if let Some(copy) = copy {
                let first = tree.node_ref(anchor).first_child().map(|c| c.id());
                insert(tree, anchor, first, copy)?;
            }
        }
        // What follows the cut moves to the second paragraph.
        if let Some(b) = cut {
            let moving: Vec<NodeId> = {
                let n = tree.node_ref(b.parent);
                let mut found = Vec::new();
                let mut on = false;
                for c in n.children() {
                    if Some(c.id()) == b.before {
                        on = true;
                    }
                    if on && !(c.is_element() && is_w(&c, "pPr")) {
                        found.push(c.id());
                    }
                }
                found
            };
            for c in moving {
                move_to(tree, c, p2, None)?;
            }
        }
        let parent =
            parent_of(tree, anchor).ok_or_else(|| internal("a paragraph has no parent"))?;
        let next = next_sibling_of(tree, anchor);
        insert(tree, parent, next, p2)?;
        if self.tracked() {
            self.add_mark_revision(part, anchor, "ins")?;
        }
        if !spec.text.is_empty() {
            let format = find_chain(self.tree_ro(part)?, p2, &["pPr", "rPr"]);
            let run = self.build_mark_run(part, &spec.text, format)?;
            self.place_run(
                part,
                Boundary {
                    parent: p2,
                    before: None,
                },
                run,
            )?;
        }
        let edit = self.new_paragraph_edit(part, p2, spec)?;
        if let Some(edit) = edit {
            self.edit_container(part, p2, &["pPr"], Direct::No, &edit)?;
        }
        Ok(p2)
    }

    /// The property edit of a new paragraph: the style that replaces its properties, then the edit.
    fn new_paragraph_edit(
        &self,
        part: &Arc<str>,
        para: NodeId,
        spec: &NewParagraph,
    ) -> Result<Option<PropEdit>, StetError> {
        let tree = self.tree_ro(part)?;
        Ok(match &spec.style {
            Some(style) => Some(style_edit(tree, para, style, &spec.props)),
            None if spec.props.is_empty() => None,
            None => Some(spec.props.clone()),
        })
    }

    /// A self-contained paragraph with a copy of the model's properties, placed before `before`, or after
    /// `after` when `before` is `None`.
    fn make_x(
        &mut self,
        part: &Arc<str>,
        twin: Option<NodeId>,
        model: Option<NodeId>,
        before: Option<NodeId>,
        after: Option<NodeId>,
        spec: &NewParagraph,
    ) -> Result<NodeId, StetError> {
        let n = match model {
            Some(m) => self.new_paragraph_element(part, m)?,
            None => {
                let uri = wns(self.tree_ro(part)?);
                create_w(self.tx.part_mut(part)?, &uri, "p")?
            }
        };
        {
            let tree = self.tx.part_mut(part)?;
            if let Some(ppr) = model.and_then(|m| find_chain(tree, m, &["pPr"]))
                && let Some(copy) = copy_ppr(tree, ppr)?
            {
                insert(tree, n, None, copy)?;
            }
            let (parent, at) = match (before, after) {
                (Some(b), _) => (
                    parent_of(tree, b).ok_or_else(|| internal("a block has no parent"))?,
                    Some(b),
                ),
                (None, Some(a)) => (
                    parent_of(tree, a).ok_or_else(|| internal("a block has no parent"))?,
                    next_sibling_of(tree, a),
                ),
                (None, None) => return Err(internal("a new paragraph has no place")),
            };
            insert(tree, parent, at, n)?;
        }
        if self.tracked() {
            self.add_mark_revision(part, n, "ins")?;
        }
        if !spec.text.is_empty() {
            let format = find_chain(self.tree_ro(part)?, n, &["pPr", "rPr"]);
            let run = self.build_mark_run(part, &spec.text, format)?;
            self.place_run(
                part,
                Boundary {
                    parent: n,
                    before: None,
                },
                run,
            )?;
        }
        if let Some(edit) = self.new_paragraph_edit(part, n, spec)? {
            // With a twin the change is written whenever the properties differ from the twin's, so that
            // rejecting gives the twin's properties under either rule. With none the paragraph is ours
            // and edited directly.
            let direct = if twin.is_some() {
                Direct::No
            } else {
                Direct::Created
            };
            self.edit_container(part, n, &["pPr"], direct, &edit)?;
        }
        Ok(n)
    }

    /// Cuts a paragraph in two at `at`, a character offset in its `Final` text. The first paragraph keeps
    /// its element and its ref, the second is new and holds the original mark and properties. A cut before
    /// the first character (that ends no field) keeps the element and the ref for the text instead, and the
    /// new paragraph is an empty X before it, so that rejecting gives back the very `w:p` (design 3.2.1).
    pub(crate) fn split_block(&mut self, block: &BlockKey, at: usize) -> Result<Effect, StetError> {
        self.split_block_as(block, at, true)
    }

    /// `split_block`, where `x_at_start` says whether a cut at the start may be an X. `compare` cuts its
    /// paragraphs with Y alone, since it takes the new block for the part after the cut.
    pub(super) fn split_block_as(
        &mut self,
        block: &BlockKey,
        at: usize,
        x_at_start: bool,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.split_block_inner(block, at, x_at_start) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn split_block_inner(
        &mut self,
        block: &BlockKey,
        at: usize,
        x_at_start: bool,
    ) -> Result<(), StetError> {
        let p = self.editable_paragraph(block)?;
        let len = p.map.len(Mode::Final);
        if at > len {
            return Err(invalid(format!(
                "the offset {at} is outside the paragraph, which has {len} characters"
            )));
        }
        let gap = p
            .map
            .gap(Mode::Final, at)
            .ok_or_else(|| invalid("an offset lies outside the paragraph"))?;
        for i in [gap.left, gap.right].into_iter().flatten() {
            self.check_items(&p, i..i + 1)?;
        }
        self.refuse_foreign_gap(&p, gap.left, gap.right)?;
        let raw = match (gap.left, gap.right) {
            (Some(l), Some(r)) if l == r => {
                let item = *p
                    .map
                    .items()
                    .get(l)
                    .ok_or_else(|| internal("a gap names a missing item"))?;
                let start = final_starts(&p.map).get(l).copied().unwrap_or(0);
                self.boundary_inside_item(&p, l, item.from as usize + (at - start))?
            }
            (Some(l), _) => self.boundary_after_item(&p, l)?,
            (None, _) => self.boundary_at_start(&p)?,
        };
        let (b, ends) = self.settle(&p.part, raw, SettleMode::Split)?;
        if self.field_depth_after(&p, gap.left) - ends > 0 {
            return Err(readonly(
                "this position is inside a field, whose result is read-only. Split the paragraph before or after the field.",
            ));
        }
        // A cut before everything the paragraph shows, with no field ending there, would leave a Y's first half empty, and
        // rejecting its mark would remove that element, its attributes and its ref with it. So the paragraph keeps them and
        // its text, and the new one is an empty X before it. A field's end before the text stays in a Y's first half.
        let at_start = x_at_start
            && gap.left.is_none()
            && ends == 0
            && wrapper_exit(self.tree_ro(&p.part)?, p.node, Place::Before)?.is_none();
        let new = if at_start {
            let n = Some(p.node);
            self.make_x(&p.part, n, n, n, None, &NewParagraph::default())?
        } else {
            self.make_y(&p.part, p.node, Some(b), &NewParagraph::default())?
        };
        let key = BlockKey::new(p.part.clone(), new);
        self.fx.new_blocks.push(key.clone());
        self.fx.origins.push(Origin {
            block: key,
            from: vec![block.clone()],
        });
        // An X leaves the paragraph as it was, as an insertion before it does.
        if !at_start {
            self.touch(block);
        }
        Ok(())
    }
}
