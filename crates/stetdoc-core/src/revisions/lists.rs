//! `new_list_instance`: a new `w:num` that restarts an existing list (design `revisions.md`, 3.3).
//!
//! Word does not track edits of numbering definitions, and neither does stetdoc: the new instance is
//! written to `numbering.xml` with no revision. What is tracked is the paragraph's own `w:numPr`, which
//! `set_props` changes. The instance points at the same `w:abstractNum` as the one it restarts, with a
//! `w:lvlOverride` for one level that holds a `w:startOverride`, as Word writes "restart numbering".

use super::compiler::Compiler;
use super::errors::{internal, invalid, limit, not_found};
use super::names::wns;
use super::props_edit::place;
use super::tree_ops::create_w;
use crate::api::error::StetError;
use crate::wordml::{NumberingDefs, StoryKind};

impl Compiler<'_, '_> {
    /// Adds a list instance that uses the same definition as `num_id` and starts level `ilvl` at
    /// `restart_at`. Returns its `w:numId`, the one after the largest in the document.
    pub(crate) fn new_list_instance(
        &mut self,
        num_id: i64,
        ilvl: u8,
        restart_at: i64,
    ) -> Result<i64, StetError> {
        if ilvl > 8 {
            return Err(invalid(format!(
                "a list has levels 0 to 8, and {ilvl} is not one"
            )));
        }
        if !(0..=i64::from(i32::MAX)).contains(&restart_at) {
            return Err(invalid(format!(
                "a list cannot restart at {restart_at}: the number must be from 0 to 2147483647"
            )));
        }
        let (part, abstract_id, new_id) = {
            let view = self.tx.parts();
            let specs = view.story_specs()?;
            let body = specs
                .0
                .iter()
                .find(|s| s.kind == StoryKind::Body)
                .ok_or_else(|| internal("the document has no body"))?;
            let main = view
                .id(&body.part)
                .ok_or_else(|| internal("the body is not a part"))?;
            let numbering = view
                .related_part(main, &|t| t.ends_with("/relationships/numbering"))?
                .ok_or_else(|| not_found("this document has no numbering definitions, so there is no list to restart"))?;
            let name = view
                .name(numbering)
                .ok_or_else(|| internal("the numbering part has no name"))?
                .clone();
            let defs = NumberingDefs::read(view.tree(numbering)?);
            let num = defs
                .num(num_id)
                .ok_or_else(|| not_found(format!("the list {num_id} does not exist")))?;
            (name, num.abstract_id, defs.max_num_id() + 1)
        };
        if new_id > i64::from(i32::MAX) {
            return Err(limit("the document has no list numbers left"));
        }
        let tree = self.tx.part_mut(&part)?;
        let uri = wns(tree);
        let num = create_w(tree, &uri, "num")?;
        tree.edit()
            .set_attr(num, &uri, "numId", &new_id.to_string())?;
        let abs = create_w(tree, &uri, "abstractNumId")?;
        tree.edit()
            .set_attr(abs, &uri, "val", &abstract_id.to_string())?;
        tree.edit().append_child(num, abs)?;
        let over = create_w(tree, &uri, "lvlOverride")?;
        tree.edit()
            .set_attr(over, &uri, "ilvl", &ilvl.to_string())?;
        let start = create_w(tree, &uri, "startOverride")?;
        tree.edit()
            .set_attr(start, &uri, "val", &restart_at.to_string())?;
        tree.edit().append_child(over, start)?;
        tree.edit().append_child(num, over)?;
        let root = tree.root().id();
        place(tree, root, num)?;
        Ok(new_id)
    }
}
