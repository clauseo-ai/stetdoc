//! `set_snapshot`: a `*PrChange` whose old properties are given, and nothing else changed (design `compare.md`, 6.1).
//!
//! `set_props` reads the snapshot from the container, which is right when the new value is what an edit has just written.
//! Compare is the other way round: the revised document's properties are already in the container and the original's are
//! known, so the old value is passed in. Everything else about the change is as `set_props` writes it: the element goes last
//! in the container, in schema order, with an id, our author and the date, and the snapshot is a copy that the copy rule
//! has stripped of every change-tracking element.

use super::compiler::Compiler;
use super::errors::{internal, invalid};
use super::insert_run::write_revision_attrs;
use super::names::{is_w, wns};
use super::outcome::Effect;
use super::props_edit::{ensure_chain, place};
use super::redline_types::Snapshot;
use super::tree_ops::{build_element, create_w};
use super::types::PropElement;
use crate::api::error::StetError;
use crate::wordml::{BlockKey, NodeId};
use std::sync::Arc;

impl Compiler<'_, '_> {
    /// Writes a `w:pPrChange` and, when `snapshot` has one, an `rPrChange` in the mark, with the old properties that
    /// `snapshot` holds. The paragraph is not otherwise changed.
    pub(crate) fn set_snapshot(
        &mut self,
        block: &BlockKey,
        snapshot: &Snapshot,
    ) -> Result<Effect, StetError> {
        self.begin();
        let res = (|| {
            let part = block.part().clone();
            if !is_w(&self.tree_ro(&part)?.node_ref(block.node()), "p") {
                return Err(invalid("a snapshot is written to a paragraph"));
            }
            self.write_snapshot(&part, block.node(), snapshot)?;
            self.touch(block);
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

    /// The body of `set_snapshot` for a paragraph element.
    pub(super) fn write_snapshot(
        &mut self,
        part: &Arc<str>,
        para: NodeId,
        snapshot: &Snapshot,
    ) -> Result<(), StetError> {
        if let Some(items) = &snapshot.ppr {
            self.write_change(part, para, &["pPr"], "pPrChange", "pPr", items)?;
        }
        if let Some(items) = &snapshot.mark {
            self.write_change(part, para, &["pPr", "rPr"], "rPrChange", "rPr", items)?;
        }
        Ok(())
    }

    /// Writes `change_local` last in the container at `path` below `owner`, holding a `snapshot_local` element with `items`.
    /// A container that already holds that change is left as it is.
    pub(super) fn write_change(
        &mut self,
        part: &Arc<str>,
        owner: NodeId,
        path: &[&str],
        change_local: &str,
        snapshot_local: &str,
        items: &[PropElement],
    ) -> Result<(), StetError> {
        let id = self.next_id()?;
        let stamp = self
            .stamp()
            .cloned()
            .ok_or_else(|| internal("a tracked change has no stamp"))?;
        let tree = self.tx.part_mut(part)?;
        let (container, _) = ensure_chain(tree, owner, path)?;
        if tree
            .node_ref(container)
            .child_elements()
            .any(|c| is_w(&c, change_local))
        {
            return Ok(());
        }
        let uri = wns(tree);
        let change = create_w(tree, &uri, change_local)?;
        write_revision_attrs(tree, change, &uri, id, &stamp)?;
        let snap = create_w(tree, &uri, snapshot_local)?;
        for item in items {
            let child = build_element(tree, item)?;
            tree.edit().append_child(snap, child)?;
        }
        tree.edit().append_child(change, snap)?;
        place(tree, container, change)
    }
}
