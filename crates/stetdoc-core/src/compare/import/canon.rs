//! The canonical form of a definition, for telling two definitions that mean the same from two that do not.

use crate::xml::{NodeKind, NodeRef};

/// The form of the element `n`: names, attributes in a fixed order and children, with the attributes that identify a revision
/// session or an id (`rsid*`, `paraId`, `textId`) left out, white space between elements left out, and the attributes named in
/// `skip` (by local name) left out too.
pub(super) fn canon(n: &NodeRef<'_>, skip: &[&str]) -> String {
    let mut out = String::new();
    write(n, skip, &mut out, 0);
    out
}

fn write(n: &NodeRef<'_>, skip: &[&str], out: &mut String, depth: usize) {
    if depth > 64 {
        return;
    }
    out.push('<');
    out.push_str(n.namespace());
    out.push('|');
    out.push_str(n.local_name());
    let mut attrs: Vec<(String, String, String)> = n
        .attrs()
        .filter(|a| !a.is_namespace_declaration())
        .filter(|a| {
            let l = a.local_name();
            !(l.starts_with("rsid") || l == "paraId" || l == "textId" || skip.contains(&l))
        })
        .map(|a| {
            (
                a.namespace().to_owned(),
                a.local_name().to_owned(),
                a.value().into_owned(),
            )
        })
        .collect();
    attrs.sort();
    for (ns, l, v) in attrs {
        out.push(' ');
        out.push_str(&ns);
        out.push('|');
        out.push_str(&l);
        out.push('=');
        out.push_str(&v);
    }
    out.push('>');
    for c in n.children() {
        match c.kind() {
            NodeKind::Element => write(&c, skip, out, depth + 1),
            NodeKind::Text | NodeKind::CData => {
                let t = c.text();
                let t = t.trim();
                if !t.is_empty() {
                    out.push_str(t);
                }
            }
            _ => {}
        }
    }
    out.push_str("</>");
}
