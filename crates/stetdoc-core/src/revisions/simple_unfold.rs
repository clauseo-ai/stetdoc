//! A tracked deletion of a whole simple field, written as a complex field (issue #24, design `revisions.md`, 3.6.3).
//!
//! A `w:fldSimple` cannot stand in a `w:del`, so a tracked deletion first wraps the field's runs in a `w:del` inside the
//! element. Word does not read that as a deleted field: its Accept All removes the runs and leaves an empty field. Word does
//! delete a complex field whose runs are all in deletions, so once a tracked deletion has deleted all of a simple field's
//! content, the element becomes the runs of a complex field, all deleted: begin, code, separate, the result as it was, end.
//! Rejecting then gives back a complex field where the source had a `w:fldSimple`, with the same code, result and formatting.

use super::cleanup::{is_attached, unwrap_element};
use super::compiler::{Compiler, ParaView};
use super::errors::internal;
use super::insert_run::write_revision_attrs;
use super::names::{XML_NS, is_w, wns};
use super::tree_ops::{copy_attrs, create_w, detach, insert, set_w_attr};
use crate::api::error::StetError;
use crate::wordml::{ItemKind, Mode, NodeId, Vis, simple_field_vis};
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

impl Compiler<'_, '_> {
    /// Writes as complex fields the simple fields of `fields` that the paragraph `p` showed in `Final` before the call and
    /// that a tracked deletion of the call has now deleted whole. A simple field that holds another is deleted whole only once
    /// the inner one is a complex field, so the passes repeat until one writes nothing.
    pub(super) fn unfold_deleted_simple_fields(
        &mut self,
        p: &ParaView,
        fields: &[NodeId],
    ) -> Result<(), StetError> {
        // Each field that was in `Final`, with whether it was in `Original` too.
        let mut left: Vec<(NodeId, bool)> = fields
            .iter()
            .filter_map(|f| {
                p.map
                    .items()
                    .iter()
                    .find(|i| {
                        i.kind == ItemKind::FieldBegin && i.node == *f && i.vis.in_mode(Mode::Final)
                    })
                    .map(|i| (*f, i.vis.in_mode(Mode::Original)))
            })
            .collect();
        loop {
            let before = left.len();
            let mut kept = Vec::with_capacity(left.len());
            for (field, original) in left {
                let gone = {
                    let tree = self.tree_ro(&p.part)?;
                    let n = tree.node_ref(field);
                    is_attached(tree, field)
                        && is_w(&n, "fldSimple")
                        && !simple_field_vis(&n, Vis::BOTH).in_mode(Mode::Final)
                };
                if gone {
                    self.unfold(&p.part, field, !original)?;
                } else {
                    kept.push((field, original));
                }
            }
            if kept.len() == before {
                return Ok(());
            }
            left = kept;
        }
    }

    /// Replaces the `w:fldSimple` `field`, whose content is all deleted, by the runs of a complex field around that content.
    /// The begin, code and separate runs go at the start of the first deletion inside the element when this call made it,
    /// and the end run at the end of the last, so the field is one revision with its result; otherwise each gets a deletion
    /// of its own. The begin carries the element's `w:fldLock`, `w:dirty` and `w:fldData`, the code is its `w:instr`, and the
    /// field's runs take the properties of the result's first run, the end those of its last, with a copy of a pending change
    /// of them under a new id, so that accepting or rejecting that change formats the field as it formats the result.
    ///
    /// A field that was not in `Original` (`inserted`: all of its content is in insertions) must stay out of it, so its runs go in
    /// a deletion of their own inside a copy of the first insertion and of the last, under new ids that the outcome records as
    /// copies. That needs its content to be insertions of one change (`one_insertion`): with two, rejecting one would take a
    /// marker and leave the other's text without a field, while the source keeps the field. Such a field, and one whose content
    /// has another shape, is left as it is (design `revisions.md`, 3.6.3).
    fn unfold(&mut self, part: &Arc<str>, field: NodeId, inserted: bool) -> Result<(), StetError> {
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked deletion has no stamp"))?;
        let (wns_uri, instr, lock, dirty, data, first_rpr, last_rpr, head, tail, wraps) = {
            let tree = self.tree_ro(part)?;
            let n = tree.node_ref(field);
            let attr = |local: &str| {
                n.attrs()
                    .find(|a| a.local_name() == local && is_w_attr_ns(a.namespace(), &n))
                    .map(|a| a.value().into_owned())
            };
            let runs = runs_below(&n);
            let ours = |c: Option<NodeRef<'_>>| {
                c.filter(|c| !inserted && is_w(c, "del") && self.is_created_node(tree, c.id()))
                    .map(|c| c.id())
            };
            let ins = |c: Option<&NodeRef<'_>>| c.filter(|c| is_w(c, "ins")).map(|c| c.id());
            let content: Vec<NodeRef<'_>> = n
                .child_elements()
                .filter(|c| !is_w(c, "fldData") && !super::carve::is_transparent(c))
                .collect();
            (
                wns(tree),
                attr("instr").unwrap_or_default(),
                attr("fldLock"),
                attr("dirty"),
                n.child_elements()
                    .filter(|c| is_w(c, "fldData"))
                    .map(|c| c.id())
                    .collect::<Vec<_>>(),
                runs.first().and_then(|r| rpr_of(r)),
                runs.last().and_then(|r| rpr_of(r)),
                ours(content.first().cloned()),
                ours(content.last().cloned()),
                one_insertion(&content)
                    .then(|| ins(content.first()).zip(ins(content.last())))
                    .flatten(),
            )
        };
        if inserted && wraps.is_none() {
            return Ok(());
        }
        let tree = self.tx.part_mut(part)?;
        let begin = field_char(tree, &wns_uri, "begin")?;
        for (local, value) in [("fldLock", &lock), ("dirty", &dirty)] {
            if let Some(v) = value {
                set_w_attr(tree, begin, &wns_uri, local, v)?;
            }
        }
        for d in data {
            detach(tree, d)?;
            insert(tree, begin, None, d)?;
        }
        let code = create_w(tree, &wns_uri, "delInstrText")?;
        tree.edit().set_attr(code, XML_NS, "space", "preserve")?;
        let text = tree.edit().create_text(&instr)?;
        insert(tree, code, None, text)?;
        let separate = field_char(tree, &wns_uri, "separate")?;
        let end = field_char(tree, &wns_uri, "end")?;
        let mut opening = Vec::new();
        for c in [begin, code, separate] {
            opening.push(field_run(tree, &wns_uri, first_rpr, c)?);
        }
        let closing = field_run(tree, &wns_uri, last_rpr, end)?;
        let new_runs: Vec<NodeId> = opening.iter().copied().chain([closing]).collect();
        let first_child = tree.node_ref(field).first_child().map(|c| c.id());
        if let Some((first_ins, last_ins)) = wraps {
            let mut dels = Vec::new();
            let mut copies = Vec::new();
            for (runs, from, before) in [
                (opening, first_ins, first_child),
                (vec![closing], last_ins, None),
            ] {
                let copy = create_w(tree, &wns_uri, "ins")?;
                copy_attrs(tree, from, copy)?;
                let del = create_w(tree, &wns_uri, "del")?;
                insert(tree, copy, None, del)?;
                for r in runs {
                    insert(tree, del, None, r)?;
                }
                insert(tree, field, before, copy)?;
                dels.push(del);
                copies.push((copy, from));
            }
            unwrap_element(tree, field)?;
            return self.finish_inserted(part, &dels, &copies, &new_runs, field);
        }
        let open_del = match head {
            Some(del) => del,
            None => {
                let del = create_w(tree, &wns_uri, "del")?;
                insert(tree, field, first_child, del)?;
                del
            }
        };
        let open_before = tree.node_ref(open_del).first_child().map(|c| c.id());
        for r in opening {
            insert(tree, open_del, open_before, r)?;
        }
        let close_del = match tail {
            Some(del) => del,
            None => {
                let del = create_w(tree, &wns_uri, "del")?;
                insert(tree, field, None, del)?;
                del
            }
        };
        insert(tree, close_del, None, closing)?;
        unwrap_element(tree, field)?;
        // A deletion this call made just before or after the field takes the field's, as Word writes a deleted paragraph
        // with its field in one `w:del`.
        let opened = self.join_created(part, open_del, true)?;
        let close_del = if close_del == open_del {
            opened
        } else {
            close_del
        };
        self.join_created(part, close_del, false)?;
        // The ids last, as `next_id` needs the transaction that the tree borrows.
        let mut made: Vec<NodeId> = Vec::new();
        for (new, del) in [(head.is_none(), open_del), (tail.is_none(), close_del)] {
            if new && !made.contains(&del) {
                made.push(del);
            }
        }
        for del in made {
            if !is_attached(self.tree_ro(part)?, del) {
                continue;
            }
            let id = self.next_id()?;
            let tree = self.tx.part_mut(part)?;
            write_revision_attrs(tree, del, &wns_uri, id, &stamp)?;
        }
        self.renumber_copied_changes(part, &new_runs, &wns_uri)?;
        self.fx.unfolded.push((part.clone(), field));
        Ok(())
    }

    /// The ids of what `unfold` wrote for a field that was in insertions: each new deletion gets one, and each copy of an
    /// insertion gets a new one that the outcome records as a copy of the insertion's.
    fn finish_inserted(
        &mut self,
        part: &Arc<str>,
        dels: &[NodeId],
        copies: &[(NodeId, NodeId)],
        runs: &[NodeId],
        field: NodeId,
    ) -> Result<(), StetError> {
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked deletion has no stamp"))?;
        let wns_uri = wns(self.tree_ro(part)?);
        let wns_uri = wns_uri.as_str();
        for &del in dels {
            let id = self.next_id()?;
            let tree = self.tx.part_mut(part)?;
            write_revision_attrs(tree, del, wns_uri, id, &stamp)?;
        }
        for &(copy, from) in copies {
            let old = id_of(&self.tree_ro(part)?.node_ref(from));
            let new = self.next_id()?;
            let tree = self.tx.part_mut(part)?;
            set_w_attr(tree, copy, wns_uri, "id", &new.to_string())?;
            if let Some(old) = old {
                self.note_split(old, i64::from(new))?;
            }
        }
        self.renumber_copied_changes(part, runs, wns_uri)?;
        self.fx.unfolded.push((part.clone(), field));
        Ok(())
    }

    /// Gives the copy of a pending `w:rPrChange` in each of `runs` a new id, which the outcome records as a copy of the old one,
    /// as one id names one element.
    fn renumber_copied_changes(
        &mut self,
        part: &Arc<str>,
        runs: &[NodeId],
        wns_uri: &str,
    ) -> Result<(), StetError> {
        for &run in runs {
            let change = {
                let tree = self.tree_ro(part)?;
                let rpr = tree.node_ref(run).child_elements().find(|c| is_w(c, "rPr"));
                rpr.and_then(|r| r.child_elements().find(|c| is_w(c, "rPrChange")))
                    .map(|c| (c.id(), id_of(&c)))
            };
            let Some((change, old)) = change else {
                continue;
            };
            let new = self.next_id()?;
            let tree = self.tx.part_mut(part)?;
            set_w_attr(tree, change, wns_uri, "id", &new.to_string())?;
            if let Some(old) = old {
                self.note_split(old, i64::from(new))?;
            }
        }
        Ok(())
    }

    /// Joins the deletion `del` to the one this call made just before it (`before`) or just after it, with only range
    /// markers between, which go into the deletion too. Returns the deletion that holds `del`'s content now.
    fn join_created(
        &mut self,
        part: &Arc<str>,
        del: NodeId,
        before: bool,
    ) -> Result<NodeId, StetError> {
        let (other, between) = {
            let tree = self.tree_ro(part)?;
            let step = |n: &NodeRef<'_>| -> Option<NodeId> {
                if before {
                    n.prev_sibling().map(|s| s.id())
                } else {
                    n.next_sibling().map(|s| s.id())
                }
            };
            let mut between = Vec::new();
            let mut cur = step(&tree.node_ref(del));
            let mut other = None;
            while let Some(c) = cur.map(|id| tree.node_ref(id)) {
                if super::carve::is_transparent(&c) {
                    between.push(c.id());
                    cur = step(&c);
                } else {
                    if is_w(&c, "del") && self.is_created_node(tree, c.id()) {
                        other = Some(c.id());
                    }
                    break;
                }
            }
            (other, between)
        };
        let Some(other) = other else {
            return Ok(del);
        };
        let tree = self.tx.part_mut(part)?;
        let kids = super::tree_ops::child_ids(tree, del);
        if before {
            // `between` runs from `del` back to `other`.
            for n in between.into_iter().rev().chain(kids) {
                super::tree_ops::move_to(tree, n, other, None)?;
            }
        } else {
            let first = tree.node_ref(other).first_child().map(|c| c.id());
            for n in kids.into_iter().chain(between) {
                super::tree_ops::move_to(tree, n, other, first)?;
            }
        }
        detach(tree, del)?;
        Ok(other)
    }
}

/// True if every element of `content` is a `w:ins` with the author and date of the first, which makes them one change: the
/// copies that hold a field's begin and end are then decided with the rest, and the field goes or stays whole.
fn one_insertion(content: &[NodeRef<'_>]) -> bool {
    let stamp = |c: &NodeRef<'_>| {
        let get = |local: &str| {
            c.attrs()
                .find(|a| a.local_name() == local && a.namespace() == c.namespace())
                .map(|a| a.value().into_owned())
        };
        (get("author"), get("date"))
    };
    let Some(first) = content.first() else {
        return false;
    };
    content
        .iter()
        .all(|c| is_w(c, "ins") && stamp(c) == stamp(first))
}

/// The `w:id` of a revision element, if it has a numeric one.
fn id_of(n: &NodeRef<'_>) -> Option<i64> {
    n.attrs()
        .find(|a| a.local_name() == "id" && a.namespace() == n.namespace())
        .and_then(|a| a.value().parse::<i64>().ok())
}

/// True if an attribute of `n` in namespace `ns` is in the element's own namespace, as the `w:` attributes are.
fn is_w_attr_ns(ns: &str, n: &NodeRef<'_>) -> bool {
    ns == n.namespace()
}

/// The `w:r` elements below `n`, in document order, leaving out what a `w:fldData` holds.
fn runs_below<'t>(n: &NodeRef<'t>) -> Vec<NodeRef<'t>> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeRef<'t>> = n.child_elements().collect();
    stack.reverse();
    while let Some(c) = stack.pop() {
        if is_w(&c, "r") {
            out.push(c);
        } else if !is_w(&c, "fldData") {
            let mut kids: Vec<NodeRef<'t>> = c.child_elements().collect();
            kids.reverse();
            stack.extend(kids);
        }
    }
    out
}

/// The run properties of `run`, with any pending change of them.
fn rpr_of(run: &NodeRef<'_>) -> Option<NodeId> {
    run.child_elements()
        .find(|c| is_w(c, "rPr"))
        .map(|c| c.id())
}

/// A new `w:fldChar` of the type `kind`.
fn field_char(tree: &mut SpanTree, wns_uri: &str, kind: &str) -> Result<NodeId, StetError> {
    let el = create_w(tree, wns_uri, "fldChar")?;
    set_w_attr(tree, el, wns_uri, "fldCharType", kind)?;
    Ok(el)
}

/// A new run holding `content`, with a copy of the properties `rpr` if there are any.
fn field_run(
    tree: &mut SpanTree,
    wns_uri: &str,
    rpr: Option<NodeId>,
    content: NodeId,
) -> Result<NodeId, StetError> {
    let run = create_w(tree, wns_uri, "r")?;
    if let Some(rpr) = rpr {
        let copy = tree.edit().clone_subtree(rpr)?;
        insert(tree, run, None, copy)?;
    }
    insert(tree, run, None, content)?;
    Ok(run)
}
