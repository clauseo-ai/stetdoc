//! Rows and tables: `insert_row` and the row side of `delete_block` (design `revisions.md`, 3.4).
//!
//! A new row is a copy of the anchor row made by the copy rule, with each cell emptied and given one
//! paragraph. Tracked, the row has `w:trPr/w:ins` and each paragraph an inserted mark with its text in
//! an insertion; there is no `w:cellIns`, as in Word's own output. A deleted row has `w:trPr/w:del` and
//! every paragraph of every cell deleted with its mark, the last of a cell included, because the row goes
//! as a whole. A row that is the author's own insertion with content that is all hers is removed outright.

use super::blocks::{TEXT_ID, copy_ppr};
use super::compiler::Compiler;
use super::errors::{cross_block, internal, invalid, readonly, stale};
use super::gap::Boundary;
use super::insert_run::check_new_text;
use super::names::{is_w, is_word_ns, w_attr, w_child};
use super::outcome::{Effect, Origin, RemovedKind, RemovedOwn};
use super::props::Direct;
use super::props_edit::{apply_edit, ensure_chain, find_chain, place};
use super::text::check_carriable;
use super::tree_ops::{create_w, detach, insert, next_sibling_of, parent_of};
use super::types::{Place, PropEdit, PropElement};
use crate::api::error::StetError;
use crate::wordml::BlockKey;
use crate::wordml::{Block, Cell, IdKind, NodeId, Row, Table, VMerge, revision_at};
use crate::xml::SpanTree;
use std::sync::Arc;

/// Gives the content controls that a copied row is wrapped in, around its cells, ids of their own: two
/// controls with one id are not what Word writes. The new id is above every id in the part. A copy also
/// loses its `dataBinding`: Word would show the bound value in place of the new text, and write the same
/// data from two controls.
fn renew_sdt_ids(tree: &mut SpanTree, row: NodeId) -> Result<(), StetError> {
    let id_element = |sdt: &crate::xml::NodeRef<'_>| {
        w_child(sdt, "sdtPr")
            .and_then(|pr| w_child(&pr, "id"))
            .map(|id| id.id())
    };
    // The controls around cells: the walk does not go into a cell, whose content is gone.
    let mut copied: Vec<NodeId> = Vec::new();
    let mut bindings: Vec<NodeId> = Vec::new();
    let mut stack = vec![tree.node_ref(row)];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if is_w(&c, "tc") {
                continue;
            }
            if is_w(&c, "sdt") {
                if let Some(id) = id_element(&c) {
                    copied.push(id);
                }
                if let Some(pr) = w_child(&c, "sdtPr") {
                    // `w:dataBinding`, and `w15:dataBinding` of Word 2013.
                    bindings.extend(
                        pr.child_elements()
                            .filter(|b| b.local_name() == "dataBinding")
                            .map(|b| b.id()),
                    );
                }
            }
            stack.push(c);
        }
    }
    for b in bindings {
        super::tree_ops::detach(tree, b)?;
    }
    if copied.is_empty() {
        return Ok(());
    }
    let mut highest = 0i64;
    let mut stack = vec![tree.root()];
    while let Some(n) = stack.pop() {
        if is_w(&n, "sdt")
            && let Some(id) = id_element(&n)
            && let Some(v) = w_attr(&tree.node_ref(id), "val").and_then(|v| v.parse::<i64>().ok())
        {
            highest = highest.max(v);
        }
        stack.extend(n.child_elements());
    }
    let uri = super::names::wns(tree);
    for id in copied {
        highest = highest.saturating_add(1).min(i64::from(i32::MAX));
        super::tree_ops::set_w_attr(tree, id, &uri, "val", &highest.to_string())?;
    }
    Ok(())
}

/// The enclosing table of a row.
fn table_of(tree: &SpanTree, row: NodeId) -> Option<NodeId> {
    let mut cur = tree.node_ref(row).parent();
    while let Some(n) = cur {
        if is_w(&n, "tbl") {
            return Some(n.id());
        }
        cur = n.parent();
    }
    None
}

impl Compiler<'_, '_> {
    /// Inserts a row before or after the row `anchor`, a copy of it with the cells' texts.
    pub(crate) fn insert_row(
        &mut self,
        anchor: &BlockKey,
        side: Place,
        cells: &[String],
    ) -> Result<Effect, StetError> {
        self.begin();
        match self.insert_row_inner(anchor, side, cells) {
            Ok(()) => Ok(self.end()),
            Err(e) => {
                self.fx = Effect::default();
                Err(e)
            }
        }
    }

    fn insert_row_inner(
        &mut self,
        anchor: &BlockKey,
        side: Place,
        cells: &[String],
    ) -> Result<(), StetError> {
        let part = anchor.part().clone();
        let (table, n_cells, vmerge, models) = {
            let tree = self.tree_ro(&part)?;
            let row = Row::new(tree, anchor.node())
                .ok_or_else(|| cross_block("the anchor of a new row must be a row"))?;
            let table =
                table_of(tree, anchor.node()).ok_or_else(|| internal("a row has no table"))?;
            let vm: Vec<VMerge> = row.cells().map(|c| c.v_merge()).collect();
            let models: Vec<Option<NodeId>> = row
                .cells()
                .map(|c| {
                    c.blocks().find_map(|b| match b {
                        Block::Paragraph(p) => Some(p.id()),
                        _ => None,
                    })
                })
                .collect();
            (table, vm.len(), vm, models)
        };
        if cells.len() > n_cells {
            return Err(invalid(format!(
                "the row has {n_cells} cells and {} texts were given",
                cells.len()
            )));
        }
        for t in cells {
            check_carriable(t)?;
            check_new_text(t)?;
        }
        // The copy, with every cell emptied but its properties.
        let tr_ids = match super::blocks::para_id_ns(self.tree_ro(&part)?, anchor.node()) {
            Some(ns) => Some((ns, self.tx.mint_id(IdKind::ParaId, &part)?)),
            None => None,
        };
        let tree = self.tx.part_mut(&part)?;
        let new_tr = tree.edit().clone_subtree(anchor.node())?;
        let attrs: Vec<(String, String)> = tree
            .node_ref(new_tr)
            .attrs()
            .filter(|a| {
                a.local_name().starts_with("rsid")
                    || a.local_name() == "textId"
                    || a.local_name() == "paraId"
            })
            .map(|a| (a.namespace().to_owned(), a.local_name().to_owned()))
            .collect();
        for (ns, local) in attrs {
            tree.edit().remove_attr(new_tr, &ns, &local)?;
        }
        if let Some((ns, id)) = tr_ids {
            // As Word writes a new row: a paraId of its own and the text id that stands for "no text".
            tree.edit()
                .set_attr(new_tr, &ns, "paraId", &format!("{id:08X}"))?;
            tree.edit().set_attr(new_tr, &ns, "textId", TEXT_ID)?;
        }
        for pr in ["tblPrEx", "trPr"] {
            if let Some(c) = find_chain(tree, new_tr, &[pr]) {
                strip_all(tree, c)?;
            }
        }
        // Range markers between the cells (a bookmark Word put at row level) are the model row's, not
        // the copy's: a copy would give two bookmarks one id.
        let row_markers: Vec<NodeId> = {
            let mut out = Vec::new();
            let mut stack = vec![tree.node_ref(new_tr)];
            while let Some(n) = stack.pop() {
                for c in n.child_elements() {
                    if super::join::MARKERS.iter().any(|m| is_w(&c, m)) {
                        out.push(c.id());
                    } else if !is_w(&c, "tc") {
                        stack.push(c);
                    }
                }
            }
            out
        };
        for m in row_markers {
            detach(tree, m)?;
        }
        // The cells, as the model rows lists them: through the wrappers that Word puts around cells.
        let new_cells: Vec<NodeId> = Row::new(tree, new_tr)
            .map(|r| r.cells().map(|c| c.id()).collect())
            .unwrap_or_default();
        for (i, &tc) in new_cells.iter().enumerate() {
            let kids: Vec<NodeId> = tree.node_ref(tc).children().map(|c| c.id()).collect();
            for k in kids {
                let is_tcpr = {
                    let n = tree.node_ref(k);
                    n.is_element() && is_w(&n, "tcPr")
                };
                if !is_tcpr {
                    detach(tree, k)?;
                }
            }
            if let Some(tcpr) = find_chain(tree, tc, &["tcPr"]) {
                strip_all(tree, tcpr)?;
                let state = vmerge.get(i).copied().unwrap_or(VMerge::None);
                let ns = tree.node_ref(tc).namespace().to_owned();
                let vm = PropElement {
                    ns: ns.clone(),
                    local: "vMerge".to_owned(),
                    attrs: Vec::new(),
                    children: Vec::new(),
                };
                let edit = match (side, state) {
                    (Place::After, VMerge::Restart | VMerge::Continue) => Some(PropEdit {
                        set: vec![vm],
                        clear: Vec::new(),
                    }),
                    (Place::Before, VMerge::Restart) => Some(PropEdit {
                        set: Vec::new(),
                        clear: vec![(ns, "vMerge".to_owned())],
                    }),
                    _ => None,
                };
                if let Some(edit) = edit {
                    apply_edit(tree, tcpr, &edit)?;
                }
            }
        }
        renew_sdt_ids(tree, new_tr)?;
        let parent =
            parent_of(tree, anchor.node()).ok_or_else(|| internal("a row has no parent"))?;
        let at = match side {
            Place::Before => Some(anchor.node()),
            Place::After => next_sibling_of(tree, anchor.node()),
        };
        insert(tree, parent, at, new_tr)?;
        if self.tracked() {
            let id = self.next_id()?;
            let stamp = self
                .stamp()
                .cloned()
                .ok_or_else(|| internal("a tracked row has no stamp"))?;
            let tree = self.tx.part_mut(&part)?;
            let (trpr, _) = ensure_chain(tree, new_tr, &["trPr"])?;
            let uri = super::names::wns(tree);
            let ins = create_w(tree, &uri, "ins")?;
            super::insert_run::write_revision_attrs(tree, ins, &uri, id, &stamp)?;
            place(tree, trpr, ins)?;
        }
        // Each cell gets one paragraph.
        let mut new_blocks = vec![BlockKey::new(part.clone(), new_tr)];
        let mut origins = vec![Origin {
            block: BlockKey::new(part.clone(), new_tr),
            from: vec![anchor.clone()],
        }];
        for (i, &tc) in new_cells.iter().enumerate() {
            let model = models.get(i).copied().flatten();
            let p = match model {
                Some(m) => self.new_paragraph_element(&part, m)?,
                None => {
                    let uri = super::names::wns(self.tree_ro(&part)?);
                    create_w(self.tx.part_mut(&part)?, &uri, "p")?
                }
            };
            {
                let tree = self.tx.part_mut(&part)?;
                tree.edit().append_child(tc, p)?;
                if let Some(ppr) = model.and_then(|m| find_chain(tree, m, &["pPr"]))
                    && let Some(copy) = copy_ppr(tree, ppr)?
                {
                    insert(tree, p, None, copy)?;
                }
            }
            if self.tracked() {
                self.add_mark_revision(&part, p, "ins")?;
            }
            let text = cells.get(i).map_or("", String::as_str);
            if !text.is_empty() {
                let format = find_chain(self.tree_ro(&part)?, p, &["pPr", "rPr"]);
                let run = self.build_mark_run(&part, text, format)?;
                self.place_run(
                    &part,
                    Boundary {
                        parent: p,
                        before: None,
                    },
                    run,
                )?;
            }
            new_blocks.push(BlockKey::new(part.clone(), p));
            // A paragraph with no model was built from nothing, and says so with an empty list.
            origins.push(Origin {
                block: BlockKey::new(part.clone(), p),
                from: model
                    .map(|m| BlockKey::new(part.clone(), m))
                    .into_iter()
                    .collect(),
            });
        }
        self.fx.new_blocks.extend(new_blocks);
        self.fx.origins.extend(origins);
        self.touch(&BlockKey::new(part, table));
        Ok(())
    }

    /// Deletes a row, or every row of a table.
    pub(super) fn delete_table_block(&mut self, block: &BlockKey) -> Result<(), StetError> {
        let part = block.part().clone();
        let (rows, table) = {
            let tree = self.tree_ro(&part)?;
            let n = tree.node_ref(block.node());
            if is_w(&n, "tbl") {
                let rows: Vec<NodeId> = Table::new(tree, block.node())
                    .map(|t| t.rows().map(|r| r.id()).collect())
                    .unwrap_or_default();
                (rows, block.node())
            } else if is_w(&n, "tr") {
                let t =
                    table_of(tree, block.node()).ok_or_else(|| internal("a row has no table"))?;
                (vec![block.node()], t)
            } else {
                return Err(stale("the block is not a table or a row any more"));
            }
        };
        // Untracked, a row goes with everything in it: check them all before the first goes, so a refusal changes nothing.
        for row in &rows {
            self.refuse_foreign_within(&part, *row, "this row holds")?;
        }
        let mut removed = 0usize;
        let total = rows.len();
        for row in rows {
            if self.delete_row(&part, row)? {
                removed += 1;
            }
        }
        let table_key = BlockKey::new(part.clone(), table);
        if removed > 0 {
            if removed == total && self.row_count(&part, table)? == 0 {
                // What the rows hoisted, and any markers of the table's own, are hoisted out of it in
                // turn, and a story that the table was all of keeps a paragraph.
                let tree = self.tx.part_mut(&part)?;
                let parent = parent_of(tree, table);
                let hoisted = super::cleanup::hoist_out(tree, table)?;
                self.note_hoisted(&part, hoisted.to, hoisted.made)?;
                let tree = self.tx.part_mut(&part)?;
                let made = match parent {
                    Some(p) => super::cleanup::keep_a_paragraph(tree, p)?,
                    None => None,
                };
                if let Some(p) = made {
                    self.note_hoisted(&part, p, true)?;
                }
            } else {
                self.touch(&table_key);
            }
        }
        Ok(())
    }

    /// A paragraph that took what a removal hoisted changed: an existing one is touched, and one made for it
    /// (the story had no other) is a new block, built from nothing.
    fn note_hoisted(&mut self, part: &Arc<str>, to: NodeId, made: bool) -> Result<(), StetError> {
        if !is_w(&self.tree_ro(part)?.node_ref(to), "p") {
            return Ok(());
        }
        let key = BlockKey::new(part.clone(), to);
        if made {
            self.fx.new_blocks.push(key.clone());
            self.fx.origins.push(Origin {
                block: key,
                from: Vec::new(),
            });
        } else {
            self.touch(&key);
        }
        Ok(())
    }

    fn row_count(&self, part: &Arc<str>, table: NodeId) -> Result<usize, StetError> {
        let tree = self.tree_ro(part)?;
        Ok(Table::new(tree, table).map_or(0, |t| t.rows().count()))
    }

    /// Deletes one row. Returns true if the row element was removed outright.
    fn delete_row(&mut self, part: &Arc<str>, row: NodeId) -> Result<bool, StetError> {
        let (deleted, ins_node) = {
            let tree = self.tree_ro(part)?;
            if Row::new(tree, row).is_none() {
                return Err(stale("the row is not in the document any more"));
            }
            let pr = find_chain(tree, row, &["trPr"]);
            // Both can be there, from two authors, so the children are looked at, not the first.
            let child = |name: &str| {
                pr.and_then(|pr| {
                    tree.node_ref(pr)
                        .child_elements()
                        .find(|c| is_w(c, name))
                        .map(|c| c.id())
                })
            };
            (child("del").is_some(), child("ins"))
        };
        if deleted {
            return Err(readonly(
                "this row is already deleted by a tracked change. Accept or reject that change first.",
            ));
        }
        if table_of(self.tree_ro(part)?, row).is_none() {
            return Err(internal("a row has no table"));
        }
        // Untracked, or the author's own insertion whose content is all hers: the row goes.
        let own = match ins_node {
            Some(n) if self.tracked() => self.own_insertion(self.tree_ro(part)?, n) != Direct::No,
            _ => false,
        };
        let ours = own && self.row_is_ours(part, row)?;
        if !self.tracked() || ours {
            let id = {
                let tree = self.tree_ro(part)?;
                ins_node
                    .and_then(|n| revision_at(&tree.node_ref(n)))
                    .and_then(|r| r.id)
            };
            // Its range markers and comment references are hoisted, as an accepted deletion of the row
            // hoists them (design 3.6.5), so that no start loses its end and a comment keeps its anchor.
            let tree = self.tx.part_mut(part)?;
            let wrapper = table_of(tree, row)
                .and_then(|t| super::resolve_table::child_toward(tree, t, row))
                .filter(|w| *w != row);
            let hoisted = super::cleanup::hoist_out(tree, row)?;
            let mut swallowed = hoisted.swallowed.clone();
            let mut gainers = vec![hoisted];
            // A row-level content control or custom XML element that held only this row goes with it.
            if let Some(w) = wrapper
                && !super::resolve_table::holds_row(tree, w)
            {
                let h = super::cleanup::hoist_out(tree, w)?;
                swallowed.extend(h.swallowed.iter().copied());
                gainers.push(h);
            }
            for h in &gainers {
                self.note_hoisted(part, h.to, h.made)?;
            }
            if ours {
                self.fx.removed_own.push(RemovedOwn {
                    id,
                    kind: RemovedKind::Row,
                });
                self.fx
                    .swallowed
                    .extend(swallowed.into_iter().filter(|i| Some(*i) != id));
            }
            return Ok(true);
        }
        // Tracked: every paragraph of every cell, and then our `del` in the row's properties. The
        // order matters: once the row is deleted, the text model reads all its text as deleted already.
        let blocks: Vec<(NodeId, bool)> = {
            let tree = self.tree_ro(part)?;
            let r = Row::new(tree, row).ok_or_else(|| internal("a row went away"))?;
            r.cells()
                .flat_map(|c: Cell<'_>| c.blocks().collect::<Vec<_>>())
                .map(|b| (b.id(), matches!(b, Block::Table(_))))
                .collect()
        };
        // Last to first: a paragraph of the author's own insertion is joined into the one after it, and
        // that one has been deleted already, so no paragraph is joined into before its turn.
        for (node, is_table) in blocks.into_iter().rev() {
            let key = BlockKey::new(part.clone(), node);
            if is_table {
                self.delete_table_block(&key)?;
            } else {
                self.delete_paragraph(&key, true, true)?;
            }
        }
        self.add_row_del(part, row)?;
        self.touch(&BlockKey::new(part.clone(), row));
        Ok(false)
    }

    /// Adds our `w:del` to the row's `w:trPr`, after another author's `w:ins` if there is one.
    fn add_row_del(&mut self, part: &Arc<str>, row: NodeId) -> Result<(), StetError> {
        let id = self.next_id()?;
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked row has no stamp"))?;
        let tree = self.tx.part_mut(part)?;
        let (trpr, _) = ensure_chain(tree, row, &["trPr"])?;
        let uri = super::names::wns(tree);
        let del = create_w(tree, &uri, "del")?;
        super::insert_run::write_revision_attrs(tree, del, &uri, id, &stamp)?;
        place(tree, trpr, del)
    }

    /// True if everything in the row is the author's own insertion: each paragraph's mark, and its
    /// content, which is inside her own insertions or is a range marker.
    fn row_is_ours(&self, part: &Arc<str>, row: NodeId) -> Result<bool, StetError> {
        let tree = self.tree_ro(part)?;
        let mut stack = vec![tree.node_ref(row)];
        while let Some(n) = stack.pop() {
            for c in n.child_elements() {
                if is_w(&c, "p") {
                    if !self.paragraph_is_ours(tree, c.id()) {
                        return Ok(false);
                    }
                } else if is_w(&c, "tbl")
                    || is_w(&c, "tr")
                    || is_w(&c, "tc")
                    || is_w(&c, "sdt")
                    || is_w(&c, "sdtContent")
                {
                    stack.push(c);
                }
            }
        }
        Ok(true)
    }

    fn paragraph_is_ours(&self, tree: &SpanTree, para: NodeId) -> bool {
        let mark = super::blocks::mark_of(tree, para);
        let own_mark = mark
            .ins
            .is_some_and(|i| self.own_insertion(tree, i) != Direct::No);
        if !own_mark || mark.del.is_some() {
            return false;
        }
        tree.node_ref(para).child_elements().all(|c| {
            if is_w(&c, "pPr") {
                return true;
            }
            if is_w(&c, "ins") {
                return self.own_insertion(tree, c.id()) != Direct::No
                    && c.child_elements()
                        .all(|x| is_w(&x, "r") || super::carve::is_transparent(&x));
            }
            super::carve::is_transparent(&c)
        })
    }
}

/// The ids of the revision elements below a node.
fn revision_ids_below(tree: &SpanTree, node: NodeId) -> Vec<i64> {
    let mut out = Vec::new();
    let mut stack = vec![tree.node_ref(node)];
    while let Some(n) = stack.pop() {
        for c in n.child_elements() {
            if let Some(r) = revision_at(&c)
                && let Some(id) = r.id
            {
                out.push(id);
            }
            stack.push(c);
        }
    }
    out
}

/// Removes every change-tracking element below a property container of a copied row.
fn strip_all(tree: &mut SpanTree, node: NodeId) -> Result<(), StetError> {
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
