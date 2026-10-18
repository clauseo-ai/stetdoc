//! Names the editor created without a prefix, and the choice of one when their node is inserted.
//!
//! An element made by `create_element`, and an attribute made on a node that is not in the tree,
//! have a namespace and a local name but no prefix. Which prefix is right depends on what is in
//! scope where the node ends up, so it is chosen at insertion (rule in `ns.rs`, `pick_prefix`).

use super::edit::Editor;
use super::errors::invalid;
use super::ns::prefix_length;
use super::tree::{Attr, F_LATE_NAME, F_NEW, F_SUBTREE_DIRTY, F_TAG_DIRTY, NONE, NodeKind, Span};
use crate::api::error::StetError;

impl Editor<'_> {
    /// Gives every unchosen name in the subtree at `sub` a prefix. The subtree is already linked
    /// into the tree, and names are chosen in document order. A subtree that no edit touched has
    /// nothing to choose, and is skipped.
    pub(super) fn resolve_late_names(&mut self, sub: u32) -> Result<(), StetError> {
        let mut stack = vec![sub];
        while let Some(id) = stack.pop() {
            let n = *self.tree.node(id);
            if n.kind != NodeKind::Element || n.flags & (F_NEW | F_SUBTREE_DIRTY) == 0 {
                continue;
            }
            if n.flags & F_LATE_NAME != 0 {
                self.resolve_element_name(id)?;
            }
            while let Some(i) = self.first_late_attr(id) {
                self.resolve_attr_name(id, i)?;
            }
            let mark = stack.len();
            let mut c = n.first;
            while c != NONE {
                stack.push(c);
                c = self.tree.node(c).next;
            }
            if let Some(kids) = stack.get_mut(mark..) {
                kids.reverse();
            }
        }
        Ok(())
    }

    fn first_late_attr(&self, el: u32) -> Option<usize> {
        let n = self.tree.node(el);
        self.tree.node_attrs(n).iter().position(Attr::is_late)
    }

    fn resolve_element_name(&mut self, id: u32) -> Result<(), StetError> {
        let n = *self.tree.node(id);
        let uri = self.tree.uri(n.ns).to_owned();
        let local = self.tree.str_of(n.name).to_owned();
        if uri.is_empty() {
            if self
                .tree
                .prefix_uri_at(id, "")
                .is_some_and(|default| !default.is_empty())
            {
                return Err(invalid(
                    "an element with no namespace would be in the default namespace here",
                ));
            }
            if let Some(m) = self.tree.node_mut(id) {
                m.flags &= !F_LATE_NAME;
            }
            return Ok(());
        }
        let prefix = self.pick_prefix(id, &uri, true, "ns")?;
        let qname = if prefix.is_empty() {
            local
        } else {
            format!("{prefix}:{local}")
        };
        let prefix_len = prefix_length(&prefix)?;
        let name = self.tree.add_bytes(qname.as_bytes())?;
        if let Some(m) = self.tree.node_mut(id) {
            m.name = name;
            m.prefix_len = prefix_len;
            m.flags &= !F_LATE_NAME;
        }
        Ok(())
    }

    fn resolve_attr_name(&mut self, el: u32, index: usize) -> Result<(), StetError> {
        let n = *self.tree.node(el);
        let Some(a) = self.tree.node_attrs(&n).get(index).copied() else {
            return Ok(());
        };
        let uri = self.tree.uri(a.ns).to_owned();
        let local = self.tree.str_of(a.name).to_owned();
        let escaped = self.tree.str_of(a.value).to_owned();
        let prefix = self.pick_prefix(el, &uri, false, "ns")?;
        let prefix_len = prefix_length(&prefix)?;
        let qname = format!("{prefix}:{local}");
        let full = self
            .tree
            .add_bytes(format!(" {qname}=\"{escaped}\"").as_bytes())?;
        let name_start = full.start as usize + 1;
        let value_start = name_start + qname.len() + 2;
        let fixed = Attr {
            full,
            value: Span::new(value_start, value_start + escaped.len())?,
            name: Span::new(name_start, name_start + qname.len())?,
            prefix_len,
            ns: a.ns,
        };
        // Choosing the prefix may have declared a namespace on this element and moved the list.
        let n = *self.tree.node(el);
        let mut list = self.tree.node_attrs(&n).to_vec();
        if let Some(slot) = list
            .iter_mut()
            .find(|b| b.is_late() && b.name == a.name && b.ns == a.ns)
        {
            *slot = fixed;
        }
        self.replace_attrs(el, list)?;
        self.flag(el, F_TAG_DIRTY);
        Ok(())
    }
}
