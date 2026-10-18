//! `set_props`: the one primitive that edits properties (design `revisions.md`, 2.2 and 3.3).
//!
//! Every target is a container of properties (a `w:rPr`, `w:pPr`, `w:sectPr`, `w:tblPr`, `w:trPr` or
//! `w:tcPr`). Tracked, an edit takes a snapshot of the container first and writes a `*PrChange` that holds
//! it after, unless the container already holds a change that this transaction made, or one of the
//! author's own from before (the edit goes into it and is reported), or something that is going to
//! disappear anyway: a paragraph mark, a row, a cell or a run that the author inserted is edited
//! directly. Another author's pending change is never touched, and the edit is refused.

use super::compiler::Compiler;
use super::errors::{internal, invalid, readonly, stale};
use super::insert_run::write_revision_attrs;
use super::names::{is_w, wns};
use super::outcome::{Effect, Folded};
use super::props_edit::{
    apply_edit, ensure_chain, find_chain, held, held_by_snapshot, place, snapshot,
};
use super::resolve_mark::next_paragraph;
use super::text::cover;
use super::tree_ops::{create_w, detach};
use super::types::{PropEdit, PropTarget};
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{Mode, NodeId, revision_at};
use crate::xml::SpanTree;
use std::ops::Range;
use std::sync::Arc;

/// Why an edit of a container is direct.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Direct {
    /// It is not: a change is written.
    No,
    /// The container belongs to something this transaction inserted.
    Created,
    /// The container belongs to something the author inserted before, with this revision id.
    Earlier(i64),
}

/// The change that a container already holds.
enum Pending {
    None,
    /// Made by this transaction.
    Created(NodeId),
    /// The author's own, from before.
    Earlier(NodeId, i64),
    /// Another author's: the name and the id.
    Other(String, i64),
}

impl Compiler<'_, '_> {
    /// Applies a property edit to the container that `target` names. See the design for what is written.
    pub(crate) fn set_props(
        &mut self,
        target: &PropTarget,
        edit: &PropEdit,
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.set_props_inner(target, edit) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn set_props_inner(&mut self, target: &PropTarget, edit: &PropEdit) -> Result<(), StetError> {
        if edit.is_empty() {
            return Ok(());
        }
        match target {
            PropTarget::Run { block, range } => self.props_run(block, range.clone(), edit),
            PropTarget::ParagraphMark { block } => {
                self.props_paragraph(block, &["pPr", "rPr"], true, edit)
            }
            PropTarget::Paragraph { block } => self.props_paragraph(block, &["pPr"], true, edit),
            PropTarget::Section { block } => {
                self.props_paragraph(block, &["pPr", "sectPr"], false, edit)
            }
            PropTarget::BodySection { part } => self.props_body_section(part, edit),
            PropTarget::Table { table } => self.props_table(table, edit),
            PropTarget::Row { row } => self.props_row(row, None, edit),
            PropTarget::Cell { row, cell } => self.props_row(row, Some(*cell), edit),
        }
    }

    /// The paragraph's own container: `w:pPr`, the mark's `w:rPr`, or the `w:sectPr`.
    fn props_paragraph(
        &mut self,
        block: &BlockKey,
        path: &[&str],
        create: bool,
        edit: &PropEdit,
    ) -> Result<(), StetError> {
        let p = self.paragraph(block)?;
        self.check_editable(&p)?;
        self.check_not_wrapped_away(&p)?;
        if create {
            // Untracked, the properties of a paragraph whose mark another author inserted, deleted or moved are theirs too.
            self.refuse_foreign_container(
                &p.part,
                p.node,
                &["pPr", "rPr"],
                "this paragraph's mark records",
            )?;
        }
        let direct = if create {
            self.own_mark(&p.part, p.node)?
        } else {
            Direct::No
        };
        if !create && find_chain(self.tree_ro(&p.part)?, p.node, path).is_none() {
            return Err(invalid(
                "this paragraph carries no section break, so there are no section properties to change",
            ));
        }
        self.edit_container(&p.part, p.node, path, direct, edit)?;
        self.touch(block);
        Ok(())
    }

    /// Whether the paragraph's mark is an insertion of the author's, which makes its properties direct.
    pub(super) fn own_mark(&self, part: &Arc<str>, paragraph: NodeId) -> Result<Direct, StetError> {
        let tree = self.tree_ro(part)?;
        let Some(ins) = find_chain(tree, paragraph, &["pPr", "rPr"])
            .and_then(|rpr| tree.node_ref(rpr).child_elements().find(|c| is_w(c, "ins")))
        else {
            return Ok(Direct::No);
        };
        let own = self.own_insertion(tree, ins.id());
        // The properties of a paragraph that an earlier insertion of the author's made are edited in
        // place only if they go when the author's insertions are rejected: the paragraph joins a later
        // one, whose properties are kept. Where nothing like that follows, the paragraph can stay, with
        // the properties it has then, so the edit is a tracked change like any other.
        if matches!(own, Direct::Earlier(_))
            && let Some(stamp) = self.stamp()
            && !survivor_follows(tree, paragraph, stamp.author())
        {
            return Ok(Direct::No);
        }
        Ok(own)
    }

    /// `Created` or `Earlier` if the revision element is an insertion of the author's.
    pub(super) fn own_insertion(&self, tree: &SpanTree, node: NodeId) -> Direct {
        let Some(stamp) = self.stamp() else {
            return Direct::No;
        };
        let Some(rev) = revision_at(&tree.node_ref(node)) else {
            return Direct::No;
        };
        if self.is_created_node(tree, node) {
            Direct::Created
        } else if rev.author.as_deref() == Some(stamp.author()) {
            Direct::Earlier(rev.id.unwrap_or(-1))
        } else {
            Direct::No
        }
    }

    fn props_body_section(&mut self, part: &Arc<str>, edit: &PropEdit) -> Result<(), StetError> {
        let owner = {
            let tree = self.tree_ro(part)?;
            let body = tree
                .root()
                .child_elements()
                .find(|c| is_w(c, "body"))
                .ok_or_else(|| invalid("this part has no body"))?;
            if !body.child_elements().any(|c| is_w(&c, "sectPr")) {
                return Err(invalid("the body of this part has no section properties"));
            }
            body.id()
        };
        self.edit_container(part, owner, &["sectPr"], Direct::No, edit)
    }

    fn props_table(&mut self, table: &BlockKey, edit: &PropEdit) -> Result<(), StetError> {
        let part = table.part().clone();
        self.expect_element(&part, table.node(), "tbl")?;
        self.edit_container(&part, table.node(), &["tblPr"], Direct::No, edit)?;
        self.touch(table);
        Ok(())
    }

    fn props_row(
        &mut self,
        row: &BlockKey,
        cell: Option<usize>,
        edit: &PropEdit,
    ) -> Result<(), StetError> {
        let part = row.part().clone();
        self.expect_element(&part, row.node(), "tr")?;
        let (owner, path, marker): (NodeId, &[&str], (&str, &str)) = match cell {
            None => (row.node(), &["trPr"], ("trPr", "ins")),
            Some(i) => {
                let tree = self.tree_ro(&part)?;
                let tc = tree
                    .node_ref(row.node())
                    .child_elements()
                    .filter(|c| is_w(c, "tc"))
                    .nth(i)
                    .ok_or_else(|| invalid(format!("the row has no cell {i}")))?;
                (tc.id(), &["tcPr"], ("tcPr", "cellIns"))
            }
        };
        let direct = {
            let tree = self.tree_ro(&part)?;
            find_chain(tree, owner, &[marker.0])
                .and_then(|pr| {
                    tree.node_ref(pr)
                        .child_elements()
                        .find(|c| is_w(c, marker.1))
                })
                .map_or(Direct::No, |ins| self.own_insertion(tree, ins.id()))
        };
        self.edit_container(&part, owner, path, direct, edit)?;
        self.touch(row);
        Ok(())
    }

    /// The node must be the element `local` of the `w` vocabulary.
    fn expect_element(&self, part: &Arc<str>, node: NodeId, local: &str) -> Result<(), StetError> {
        let tree = self.tree_ro(part)?;
        if !is_w(&tree.node_ref(node), local) {
            return Err(stale(format!("the block is not a {local} any more")));
        }
        Ok(())
    }

    /// Formats the runs that hold `range`: each run's `w:rPr`, after cutting the runs at the ends.
    fn props_run(
        &mut self,
        block: &BlockKey,
        range: Range<usize>,
        edit: &PropEdit,
    ) -> Result<(), StetError> {
        let p = self.paragraph(block)?;
        // before the range, which a paragraph that is gone in `Final` has none of
        self.check_not_wrapped_away(&p)?;
        let len = p.map.len(Mode::Final);
        if range.start > range.end || range.end > len {
            return Err(invalid(format!(
                "the range {}..{} is outside the paragraph, which has {len} characters",
                range.start, range.end
            )));
        }
        self.check_editable(&p)?;
        if range.is_empty() {
            return Ok(());
        }
        let loc = p
            .map
            .locate(Mode::Final, range)
            .ok_or_else(|| invalid("a range lies outside the paragraph"))?;
        let loc = cover(&p, loc)?;
        self.check_items(&p, loc.items.clone())?;
        self.refuse_foreign_run_changes(&p, loc.items.clone())?;
        self.refuse_foreign_items(&p, loc.items.clone())?;
        let carved = self.carve(&p, &loc)?;
        for (unit, _) in carved.units {
            let (is_run, direct) = {
                let tree = self.tree_ro(&p.part)?;
                let n = tree.node_ref(unit);
                (is_w(&n, "r"), self.run_direct(tree, unit))
            };
            if is_run {
                self.edit_container(&p.part, unit, &["rPr"], direct, edit)?;
            }
        }
        self.touch(block);
        Ok(())
    }

    /// A run inside an insertion that this transaction made is formatted directly: rejecting the
    /// insertion removes the run, so a change would only be noise.
    fn run_direct(&self, tree: &SpanTree, run: NodeId) -> Direct {
        let mut cur = tree.node_ref(run).parent();
        while let Some(a) = cur {
            if is_w(&a, "p") {
                break;
            }
            if is_w(&a, "ins") && self.is_created_node(tree, a.id()) {
                return Direct::Created;
            }
            cur = a.parent();
        }
        Direct::No
    }

    /// Before anything changes: the runs of the range must not hold another author's pending change.
    fn refuse_foreign_run_changes(
        &self,
        p: &super::compiler::ParaView,
        items: Range<usize>,
    ) -> Result<(), StetError> {
        let Some(stamp) = self.stamp() else {
            return Ok(());
        };
        let tree = self.tree_ro(&p.part)?;
        for i in items {
            let Some(run) = p.map.items().get(i).and_then(|it| it.run) else {
                continue;
            };
            let change = find_chain(tree, run, &["rPr"]).and_then(|rpr| {
                tree.node_ref(rpr)
                    .child_elements()
                    .find(|c| is_w(c, "rPrChange"))
            });
            if let Some(change) = change
                && let Some(rev) = revision_at(&change)
                && rev.author.as_deref() != Some(stamp.author())
            {
                return Err(readonly(format!(
                    "this text has a pending formatting change by {}. Accept or reject it first, or format other text.",
                    rev.author.unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    /// The change that a container holds, and whose it is.
    fn pending(&self, tree: &SpanTree, container: NodeId, change_local: &str) -> Pending {
        let Some(stamp) = self.stamp() else {
            return Pending::None;
        };
        let Some(change) = tree
            .node_ref(container)
            .child_elements()
            .find(|c| is_w(c, change_local))
        else {
            return Pending::None;
        };
        let rev = revision_at(&change);
        let id = rev.as_ref().and_then(|r| r.id).unwrap_or(-1);
        let author = rev.and_then(|r| r.author);
        if self.is_created_node(tree, change.id()) {
            Pending::Created(change.id())
        } else if author.as_deref() == Some(stamp.author()) {
            Pending::Earlier(change.id(), id)
        } else {
            Pending::Other(author.unwrap_or_default(), id)
        }
    }

    /// Edits the container at `path` below `owner`, in the way that `direct` and the track allow.
    pub(super) fn edit_container(
        &mut self,
        part: &Arc<str>,
        owner: NodeId,
        path: &[&str],
        direct: Direct,
        edit: &PropEdit,
    ) -> Result<(), StetError> {
        let local = *path
            .last()
            .ok_or_else(|| internal("a container has no name"))?;
        let change_local = format!("{local}Change");
        self.refuse_foreign_container(part, owner, path, "these properties hold")?;
        let pending = {
            let tree = self.tree_ro(part)?;
            match find_chain(tree, owner, path) {
                Some(c) if self.tracked() => self.pending(tree, c, &change_local),
                _ => Pending::None,
            }
        };
        if let Pending::Other(author, id) = &pending {
            return Err(readonly(format!(
                "these properties have a pending change by {author} (id {id}). Accept or reject it first, or edit something else."
            )));
        }
        if direct != Direct::No || !self.tracked() {
            let tree = self.tx.part_mut(part)?;
            let (container, made) = ensure_chain(tree, owner, path)?;
            let was_empty = is_empty(tree, container);
            apply_edit(tree, container, edit)?;
            drop_if_empty(tree, owner, container, made, was_empty)?;
            if let Direct::Earlier(id) = direct {
                self.fx.folded.push(Folded {
                    id,
                    cancelled: false,
                });
            }
            return Ok(());
        }
        // A change is written, or the one that is there takes the edit.
        let tree = self.tx.part_mut(part)?;
        let (container, made) = ensure_chain(tree, owner, path)?;
        let was_empty = is_empty(tree, container);
        let snap = match &pending {
            Pending::None => Some(snapshot(tree, container, &change_local)?),
            _ => None,
        };
        apply_edit(tree, container, edit)?;
        let write = match (&pending, snap) {
            (Pending::None, Some(snap)) => {
                if held(tree, container, &change_local) == held_by_snapshot(tree, snap) {
                    None
                } else {
                    Some(snap)
                }
            }
            (Pending::Created(change) | Pending::Earlier(change, _), _) => {
                let old = tree
                    .node_ref(*change)
                    .child_elements()
                    .next()
                    .map(|s| s.id());
                let same = old.is_some_and(|s| {
                    held(tree, container, &change_local) == held_by_snapshot(tree, s)
                });
                if same {
                    detach(tree, *change)?;
                }
                if let Pending::Earlier(_, id) = &pending {
                    self.fx.folded.push(Folded {
                        id: *id,
                        cancelled: same,
                    });
                }
                None
            }
            _ => None,
        };
        let Some(snap) = write else {
            let tree = self.tx.part_mut(part)?;
            drop_if_empty(tree, owner, container, made, was_empty)?;
            return Ok(());
        };
        let id = self.next_id()?;
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked edit has no stamp"))?;
        let tree = self.tx.part_mut(part)?;
        let wns_uri = wns(tree);
        let change = create_w(tree, &wns_uri, &change_local)?;
        write_revision_attrs(tree, change, &wns_uri, id, &stamp)?;
        tree.edit().append_child(change, snap)?;
        place(tree, container, change)?;
        Ok(())
    }
}

/// Whether a container has no element child.
fn is_empty(tree: &SpanTree, container: NodeId) -> bool {
    tree.node_ref(container).child_elements().next().is_none()
}

/// Removes a container that the edit left with nothing in it: one that this edit made, or a `w:rPr` or
/// `w:pPr` that had properties before it (an edit that clears the last property of a run takes its `w:rPr`
/// with it, since an empty one is W10). Then each `w:rPr` or `w:pPr` on the path up to `owner` that is
/// left empty goes too. A container that was empty before the edit is the document's, and stays, and so does
/// any other container (a `w:tblPr` is required).
fn drop_if_empty(
    tree: &mut SpanTree,
    owner: NodeId,
    container: NodeId,
    made: Option<NodeId>,
    was_empty: bool,
) -> Result<(), StetError> {
    if !is_empty(tree, container) {
        return Ok(());
    }
    if let Some(root) = made {
        detach(tree, root)?;
        return Ok(());
    }
    let w10 = |tree: &SpanTree, n: NodeId| {
        let n = tree.node_ref(n);
        is_w(&n, "rPr") || is_w(&n, "pPr")
    };
    if was_empty || !w10(tree, container) {
        return Ok(());
    }
    let mut doomed = container;
    loop {
        let parent = tree.node_ref(doomed).parent().map(|p| p.id());
        detach(tree, doomed)?;
        match parent {
            Some(p) if p != owner && w10(tree, p) && is_empty(tree, p) => doomed = p,
            _ => break,
        }
    }
    Ok(())
}

/// True if a paragraph after `paragraph` in its container has a mark that is not an insertion by
/// `author`: rejecting the author's insertions joins `paragraph` into it.
fn survivor_follows(tree: &SpanTree, paragraph: NodeId, author: &str) -> bool {
    let mut cur = next_paragraph(tree, paragraph);
    while let Some(p) = cur {
        let own_ins = find_chain(tree, p, &["pPr", "rPr"]).is_some_and(|rpr| {
            tree.node_ref(rpr).child_elements().any(|c| {
                is_w(&c, "ins")
                    && revision_at(&c).is_some_and(|r| r.author.as_deref() == Some(author))
            })
        });
        if !own_ins {
            return true;
        }
        cur = next_paragraph(tree, p);
    }
    false
}
