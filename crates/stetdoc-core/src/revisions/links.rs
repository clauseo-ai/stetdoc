//! The relationships of the hyperlinks an edit removed. A hyperlink names its address through a relationship of its part
//! (`r:id`). When an edit removes the last element that uses a hyperlink relationship (an emptied link that went, a
//! paragraph deleted untracked, a tracked deletion that was accepted), the relationship goes too, so the part keeps no
//! relationship that nothing uses. A relationship that nothing used before the edit is left as it was, and so is every
//! relationship of another type: a hyperlink's address is the only target this cleans up.

use super::names::{R_S, R_T};
use crate::api::error::StetError;
use crate::state::Tx;
use crate::xml::SpanTree;
use std::collections::HashSet;

/// True for the type of a hyperlink relationship, in either dialect.
fn is_hyperlink_type(rel_type: &str) -> bool {
    rel_type == "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink"
        || rel_type == "http://purl.oclc.org/ooxml/officeDocument/relationships/hyperlink"
}

/// The values of every attribute of the relationships namespace in `tree`: the relationship ids the part uses. One pass over
/// the tree, so the cost grows with the size of the part and not with the number of links times the number of attributes.
fn in_use(tree: &SpanTree) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack = vec![tree.root()];
    while let Some(n) = stack.pop() {
        for a in n.attrs() {
            if a.namespace() == R_T || a.namespace() == R_S {
                out.insert(a.value().into_owned());
            }
        }
        stack.extend(n.child_elements());
    }
    out
}

/// Removes the hyperlink relationships of the parts `tx` edited whose last use the transaction removed: the part used them
/// before the transaction and uses them no more. Returns the names of the relationships parts it rewrote.
pub(crate) fn drop_unused_link_rels(tx: &mut Tx<'_>) -> Result<Vec<String>, StetError> {
    let mut rewritten = Vec::new();
    for name in tx.edited_clones() {
        let gone = {
            let view = tx.parts();
            let Some(id) = view.id(&name) else {
                continue;
            };
            let links: Vec<String> = view
                .rels(id)?
                .iter()
                .filter(|r| is_hyperlink_type(&r.rel_type))
                .map(|r| r.id.clone())
                .collect();
            if links.is_empty() {
                continue;
            }
            let now = in_use(view.tree(id)?);
            let unused: Vec<String> = links.into_iter().filter(|l| !now.contains(l)).collect();
            if unused.is_empty() {
                continue;
            }
            let base = tx.base().parts();
            let Some(base_id) = base.id(&name) else {
                continue;
            };
            let before = in_use(base.tree(base_id)?);
            unused
                .into_iter()
                .filter(|l| before.contains(l))
                .collect::<Vec<String>>()
        };
        if gone.is_empty() {
            continue;
        }
        if let Some(rels) = tx.remove_relationships(&name, &gone)? {
            rewritten.push(rels);
        }
    }
    Ok(rewritten)
}
