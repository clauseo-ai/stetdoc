//! The tree the edit simulation works on: a real span tree, edited through its `Editor`, and
//! the violation keys of a parent. Nothing here is a stand-in for the engine's tree: an
//! insertion is `create_element` and `insert_before`, a rewrite of a property container is
//! `detach` and `append_child`, and what is validated afterwards is the edited span tree.

use super::super::tests::{W, W14};
use super::super::validate::is_boundary;
use super::*;
use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeId, SpanTree};
use std::collections::{BTreeSet, HashSet};

/// An element to build: its name, attributes and children.
pub(super) struct Spec {
    pub ns: String,
    pub local: String,
    pub attrs: Vec<(String, String, String)>,
    pub kids: Vec<Spec>,
}

/// A span tree under edit.
pub(super) struct SimTree {
    pub(super) tree: SpanTree,
    /// Edits the editor refused, which the simulation should never ask for.
    pub(super) refused_by_editor: usize,
}

/// What `SimTree::save` remembers so that an edit can be undone.
pub(super) struct Saved {
    parent: NodeId,
    children: Vec<NodeId>,
}

impl SimTree {
    /// Parses one part. `None` when the span tree refuses it.
    pub(super) fn parse(bytes: &[u8]) -> Option<SimTree> {
        let tree = SpanTree::parse("part.xml", Bytes::from(bytes), &Limits::default()).ok()?;
        Some(SimTree {
            tree,
            refused_by_editor: 0,
        })
    }

    pub(super) fn root(&self) -> NodeId {
        self.tree.root().id()
    }

    /// The elements of the tree in document order.
    pub(super) fn elements(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![self.root()];
        while let Some(id) = stack.pop() {
            out.push(id);
            let mut kids = self.children(id);
            kids.reverse();
            stack.extend(kids);
        }
        out
    }

    pub(super) fn children(&self, id: NodeId) -> Vec<NodeId> {
        self.tree.children(id)
    }

    pub(super) fn qname(&self, id: NodeId) -> (String, String) {
        let (ns, local) = self.tree.name(id);
        (ns.to_string(), local.to_string())
    }

    /// The first element with the local name, in document order.
    pub(super) fn find(&self, local: &str) -> Option<NodeId> {
        self.elements()
            .into_iter()
            .find(|e| self.tree.name(*e).1 == local)
    }

    /// The schema context of an element.
    pub(super) fn ctx_of(&self, part: PartKind, id: NodeId) -> Option<Ctx> {
        ctx_at(part, &self.tree, id)
    }

    /// The children of `id` as names, with what is inside an element in brackets, for messages.
    pub(super) fn shape(&self, id: NodeId) -> String {
        let kids: Vec<String> = self
            .children(id)
            .iter()
            .map(|c| {
                let name = self.qname(*c).1;
                if self.children(*c).is_empty() {
                    name
                } else {
                    format!("{name}[{}]", self.shape(*c))
                }
            })
            .collect();
        kids.join(" ")
    }

    /// Remembers the children of `parent`.
    pub(super) fn save(&self, parent: NodeId) -> Saved {
        Saved {
            parent,
            children: self.children(parent),
        }
    }

    /// Puts the children of the parent back as `saved` has them. An element that an edit added
    /// is taken out and one that it removed is put back where it was, which is all that
    /// insertion and removal do; a parent whose children were rearranged (a rewrite) is rebuilt.
    pub(super) fn restore(&mut self, saved: Saved) {
        let now = self.children(saved.parent);
        let wanted: HashSet<NodeId> = saved.children.iter().copied().collect();
        let mut ed = self.tree.edit();
        let mut failed = false;
        for c in now.iter().filter(|c| !wanted.contains(c)) {
            failed |= ed.detach(*c).is_err();
        }
        let kept: Vec<NodeId> = now.iter().copied().filter(|c| wanted.contains(c)).collect();
        let present: HashSet<NodeId> = kept.iter().copied().collect();
        let in_order = kept
            .iter()
            .eq(saved.children.iter().filter(|c| present.contains(c)));
        if in_order {
            // Put back what is missing, from the end, before the element that follows it.
            let mut next: Option<NodeId> = None;
            for c in saved.children.iter().rev() {
                if !present.contains(c) {
                    failed |= ed.insert_before(saved.parent, next, *c).is_err();
                }
                next = Some(*c);
            }
        } else {
            for c in &kept {
                failed |= ed.detach(*c).is_err();
            }
            for c in &saved.children {
                failed |= ed.append_child(saved.parent, *c).is_err();
            }
        }
        self.refused_by_editor += usize::from(failed);
    }

    /// Inserts a new empty element into `parent` where the schema says, rewriting the parent's
    /// children when the fit is `Reorder`. Returns the fit and the new element, or `None` when
    /// the editor refuses (counted in `refused_by_editor`).
    pub(super) fn insert(
        &mut self,
        ctx: Option<Ctx>,
        parent: NodeId,
        name: (&str, &str),
        prefer: Prefer,
    ) -> Option<(Fit, NodeId)> {
        let kids = self.children(parent);
        let placement = insert_in_order(ctx, &self.tree, parent, name, prefer);
        let done = (|| {
            let mut ed = self.tree.edit();
            let new = ed.create_element(name.0, name.1).ok()?;
            match &placement.fit {
                Fit::Reorder(order) => {
                    for k in &kids {
                        ed.detach(*k).ok()?;
                    }
                    for slot in order {
                        let id = match slot {
                            Some(i) => *kids.get(*i)?,
                            None => new,
                        };
                        ed.append_child(parent, id).ok()?;
                    }
                }
                _ => ed
                    .insert_before(parent, kids.get(placement.index).copied(), new)
                    .ok()?,
            }
            Some(new)
        })();
        let Some(new) = done else {
            self.refused_by_editor += 1;
            return None;
        };
        Some((placement.fit, new))
    }

    /// Takes the child at `index` out of `parent`. When `ctx` is given and a property container,
    /// the rest is rewritten in schema order, as the contract asks of a caller. Returns whether
    /// that moved anything.
    pub(super) fn remove(&mut self, ctx: Option<Ctx>, parent: NodeId, index: usize) -> bool {
        let Some(victim) = self.children(parent).get(index).copied() else {
            return false;
        };
        if self.tree.edit().detach(victim).is_err() {
            self.refused_by_editor += 1;
            return false;
        }
        let Some(order) = reorder(ctx, &self.tree, parent, None) else {
            return false;
        };
        let now = self.children(parent);
        let rewritten: Vec<NodeId> = order
            .into_iter()
            .filter_map(|slot| slot.and_then(|i| now.get(i).copied()))
            .collect();
        if rewritten == now {
            return false;
        }
        let mut ed = self.tree.edit();
        let mut failed = false;
        for c in &now {
            failed |= ed.detach(*c).is_err();
        }
        for c in &rewritten {
            failed |= ed.append_child(parent, *c).is_err();
        }
        self.refused_by_editor += usize::from(failed);
        true
    }

    /// A tree whose root is `root` with the prefixes the tests use bound, holding `kids`. It is
    /// written as XML and parsed, like any other part.
    pub(super) fn from_spec(root: (&str, &str), kids: Vec<Spec>) -> SimTree {
        let mut uris: BTreeSet<String> = BTreeSet::new();
        uris.insert(root.0.to_string());
        collect_uris(&kids, &mut uris);
        let mut prefixes: Vec<(String, String)> = [
            ("w", W),
            ("w14", W14),
            ("mc", MC_NS),
            (
                "wps",
                "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
            ),
            ("x", "urn:example:unknown"),
        ]
        .iter()
        .map(|(p, u)| (p.to_string(), u.to_string()))
        .collect();
        for u in uris.iter().filter(|u| !u.is_empty()) {
            if !prefixes.iter().any(|(_, known)| known == u) {
                prefixes.push((format!("n{}", prefixes.len()), u.clone()));
            }
        }
        let mut xml = String::new();
        xml.push('<');
        xml.push_str(&qualified(&prefixes, root.0, root.1));
        for (p, u) in &prefixes {
            xml.push_str(&format!(" xmlns:{p}=\"{u}\""));
        }
        xml.push('>');
        for k in &kids {
            write_spec(&prefixes, k, &mut xml);
        }
        xml.push_str(&format!("</{}>", qualified(&prefixes, root.0, root.1)));
        SimTree::parse(xml.as_bytes()).expect("the generated XML parses")
    }
}

fn collect_uris(specs: &[Spec], out: &mut BTreeSet<String>) {
    for s in specs {
        out.insert(s.ns.clone());
        out.extend(s.attrs.iter().map(|a| a.0.clone()));
        collect_uris(&s.kids, out);
    }
}

fn qualified(prefixes: &[(String, String)], ns: &str, local: &str) -> String {
    match prefixes.iter().find(|(_, u)| u == ns) {
        Some((p, _)) if !ns.is_empty() => format!("{p}:{local}"),
        _ => local.to_string(),
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('"', "&quot;")
}

fn write_spec(prefixes: &[(String, String)], spec: &Spec, out: &mut String) {
    let name = qualified(prefixes, &spec.ns, &spec.local);
    out.push('<');
    out.push_str(&name);
    for (ns, local, value) in &spec.attrs {
        out.push_str(&format!(
            " {}=\"{}\"",
            qualified(prefixes, ns, local),
            escape(value)
        ));
    }
    out.push('>');
    for k in &spec.kids {
        write_spec(prefixes, k, out);
    }
    out.push_str(&format!("</{name}>"));
}

/// Every violation key of the unit of `parent` (`validate_unit`: `parent` and what is below it,
/// except the insides of paragraphs and tables below it), apart from the problems of the element
/// `own` that the edit put there: an empty `w:tc` lacks its paragraph, which is not what the edit
/// is judged on. A paragraph or table that `own` is has its insides in no unit of `parent`.
pub(super) fn keys_of(
    tree: &SimTree,
    ctx: Ctx,
    parent: NodeId,
    own: Option<NodeId>,
) -> ViolationSet {
    let all = validate_unit(ctx, &tree.tree, parent).set;
    let Some(new) = own else {
        return all;
    };
    if is_boundary(&tree.tree, new) {
        return all;
    }
    let (ns, local) = tree.qname(new);
    let Some(new_ctx) = ctx.child((ns.as_str(), local.as_str())) else {
        return all;
    };
    let theirs = validate_unit(new_ctx, &tree.tree, new).set;
    let mut set = ViolationSet::default();
    for (k, n) in all.iter() {
        for _ in 0..n.saturating_sub(theirs.count(k)) {
            set.add(k.clone());
        }
    }
    set
}
