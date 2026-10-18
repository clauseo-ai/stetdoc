//! `add_deleted_note`: a note of the other package, whose reference is in deleted content, added to the notes part with its
//! paragraphs deleted (design `compare.md`, 7.5).
//!
//! The note's paragraphs arrive detached, copied by `compare::import`. The call checks everything it would refuse before it
//! writes: the part is a notes part of the right kind, the id is free, and every block is a detached paragraph or table. It then
//! makes the `w:footnote` or `w:endnote` element, puts the blocks in it, appends it to the part and marks every block deleted,
//! the last paragraph's mark too (`RP050`), so that accepting leaves a note with no reference, which the unreferenced-note rule
//! removes, and rejecting gives the note back.

use super::compiler::Compiler;
use super::errors::invalid;
use super::names::{is_w, wns};
use super::outcome::Effect;
use super::redline_types::Imported;
use super::tree_ops::create_w;
use crate::api::error::StetError;
use std::sync::Arc;

impl Compiler<'_, '_> {
    /// What `add_deleted_note` would refuse, found without writing: a part that is not a notes part of the kind, an id that is
    /// taken, no block, or a block that is not a detached paragraph or table.
    pub(crate) fn check_deleted_note(
        &self,
        part: &Arc<str>,
        endnote: bool,
        id: i64,
        blocks: &[Imported],
    ) -> Result<(), StetError> {
        let local = if endnote { "endnote" } else { "footnote" };
        {
            let tree = self.tree_ro(part)?;
            let root = tree.root();
            if !is_w(&root, if endnote { "endnotes" } else { "footnotes" }) {
                return Err(invalid("a note is added only to a notes part of its kind"));
            }
            let taken = root.child_elements().any(|c| {
                is_w(&c, local)
                    && c.attrs().any(|a| {
                        a.local_name() == "id" && a.value().parse::<i64>().ok() == Some(id)
                    })
            });
            if taken {
                return Err(invalid("the notes part already has a note with this id"));
            }
            if blocks.is_empty() {
                return Err(invalid("a note has at least one paragraph"));
            }
            for b in blocks {
                let n = tree.node_ref(b.node);
                if n.parent().is_some() || !(is_w(&n, "p") || is_w(&n, "tbl")) {
                    return Err(invalid("a note's blocks are detached paragraphs or tables"));
                }
            }
        }
        Ok(())
    }

    /// Adds the note `id` (an endnote if `endnote`) to `part`, holding `blocks`, each marked deleted.
    pub(crate) fn add_deleted_note(
        &mut self,
        part: &Arc<str>,
        endnote: bool,
        id: i64,
        blocks: &[Imported],
    ) -> Result<Effect, StetError> {
        self.check_deleted_note(part, endnote, id, blocks)?;
        let local = if endnote { "endnote" } else { "footnote" };
        self.begin();
        let res = (|| {
            let tree = self.tx.part_mut(part)?;
            let uri = wns(tree);
            let root = tree.root().id();
            let el = create_w(tree, &uri, local)?;
            let mut ed = tree.edit();
            ed.set_attr(el, &uri, "id", &id.to_string())?;
            for b in blocks {
                ed.append_child(el, b.node)?;
            }
            ed.append_child(root, el)?;
            for b in blocks {
                self.mark_whole_deleted(part, b, false)?;
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
}
