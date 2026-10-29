//! Namespaces in the tree: what a prefix means at a node, binding a namespace on the root,
//! choosing a prefix for new names, and the declarations that keep the meaning of a moved node's
//! prefixes. See `docs/design/package-and-xml.md` section 2.5.

use super::NodeId;
use super::edit::{Editor, MC_NS, no_node};
use super::errors::{invalid, limit, unsupported};
use super::scope::ScopeStack;
use super::text::{escape_attr, is_ncname, unescape_attr};
use super::tree::{
    Attr, F_LATE_NAME, F_NS_DECL, F_TAG_DIRTY, NONE, Node, NodeKind, Span, SpanTree, XML_NS,
    XMLNS_NS,
};
use crate::api::error::StetError;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

impl SpanTree {
    /// The namespace declarations of an element, as (prefix, attribute). The default namespace
    /// has the empty prefix.
    pub(super) fn declarations<'s>(
        &'s self,
        n: &'s Node,
    ) -> impl Iterator<Item = (&'s str, &'s Attr)> + 's {
        self.node_attrs(n).iter().filter_map(move |a| {
            if a.ns == 0 || self.uri(a.ns) != XMLNS_NS {
                return None;
            }
            let name = self.str_of(a.name);
            Some((name.strip_prefix("xmlns:").unwrap_or(""), a))
        })
    }

    /// The namespace URI a declaration binds.
    pub(super) fn declared_uri(&self, a: &Attr) -> Cow<'_, str> {
        unescape_attr(self.str_of(a.value))
    }

    /// What `prefix` means at `at`: the value of the nearest declaration of it, found by walking
    /// up from `at`, which counts. An empty prefix is the default namespace, and an empty result
    /// is a default namespace that was undeclared. `None` if the prefix is not bound.
    pub(super) fn prefix_uri_at(&self, at: u32, prefix: &str) -> Option<Cow<'_, str>> {
        let mut cur = at;
        while cur != NONE {
            let n = self.node(cur);
            if n.flags & F_NS_DECL != 0
                && let Some((_, a)) = self.declarations(n).find(|(p, _)| *p == prefix)
            {
                return Some(self.declared_uri(a));
            }
            cur = n.parent;
        }
        (prefix == "xml").then_some(Cow::Borrowed(XML_NS))
    }

    /// A prefix that means `uri` at `at`, nearest declaration first. A binding that a closer
    /// declaration of the same prefix hides does not count. The empty prefix (the default
    /// namespace) counts only if `allow_default`.
    pub(super) fn find_prefix_at(&self, at: u32, uri: &str, allow_default: bool) -> Option<String> {
        let mut hidden: HashSet<&str> = HashSet::new();
        let mut cur = at;
        while cur != NONE {
            let n = self.node(cur);
            if n.flags & F_NS_DECL != 0 {
                let here: Vec<(&str, Cow<'_, str>)> = self
                    .declarations(n)
                    .map(|(p, a)| (p, self.declared_uri(a)))
                    .collect();
                for (p, v) in &here {
                    if (allow_default || !p.is_empty()) && !hidden.contains(p) && v == uri {
                        return Some((*p).to_owned());
                    }
                }
                hidden.extend(here.iter().map(|(p, _)| *p));
            }
            cur = n.parent;
        }
        None
    }

    /// Every prefix in scope at `at`, with what it means.
    pub(super) fn scope_at(&self, at: u32) -> HashMap<&str, Cow<'_, str>> {
        let mut scope: HashMap<&str, Cow<'_, str>> = HashMap::new();
        let mut cur = at;
        while cur != NONE {
            let n = self.node(cur);
            if n.flags & F_NS_DECL != 0 {
                for (p, a) in self.declarations(n) {
                    scope.entry(p).or_insert_with(|| self.declared_uri(a));
                }
            }
            cur = n.parent;
        }
        scope
    }

    /// True if the node is in the document: following its parents ends at the document node.
    pub(super) fn in_document(&self, mut id: u32) -> bool {
        loop {
            let parent = self.node(id).parent;
            if parent == NONE {
                return id == 0;
            }
            id = parent;
        }
    }
}

/// The index that stands for a namespace this table has never seen. It equals no real one.
const UNKNOWN: u16 = u16::MAX;

impl Editor<'_> {
    /// The prefix the root element binds to `uri`, or `None`. The empty string means the
    /// default namespace.
    pub(super) fn root_prefix_for(&self, uri: &str, allow_default: bool) -> Option<String> {
        let root = self.tree.node(self.tree.root);
        self.tree
            .declarations(root)
            .find(|(p, a)| (allow_default || !p.is_empty()) && self.tree.declared_uri(a) == uri)
            .map(|(p, _)| p.to_owned())
    }

    /// Keeps the meaning of every written name in a subtree at its new position. A prefix means what the declarations in the
    /// subtree and above `new_parent` make it mean, and where a written name's prefix (or the default namespace) would mean
    /// something else there, the namespace it had is declared on the highest element of the subtree that needs it, so the
    /// element and everything under it read as before. Only a prefix that cannot be declared (`xml` or `xmlns` bound to
    /// something else), or one element whose name and an attribute need the same prefix for two namespaces, is
    /// `unsupported`; then nothing is declared. Used only when some element below the root declares namespaces, because
    /// otherwise a prefix means the same everywhere.
    pub(super) fn keep_namespaces(&mut self, sub: u32, new_parent: u32) -> Result<(), StetError> {
        let needed = self.declarations_to_keep(sub, new_parent)?;
        for (el, prefix, uri) in needed {
            self.declare_on(el, &uri, &prefix)?;
        }
        Ok(())
    }

    /// The declarations `keep_namespaces` adds, as (element, prefix, namespace URI), outermost first.
    fn declarations_to_keep(
        &self,
        sub: u32,
        new_parent: u32,
    ) -> Result<Vec<(u32, String, String)>, StetError> {
        enum Step {
            Enter(u32),
            Exit(usize),
        }
        let tree = &*self.tree;
        let outer = tree.scope_at(new_parent);
        let index_of = |uri: &str| {
            if uri.is_empty() {
                0
            } else {
                tree.ns.find(uri).unwrap_or(UNKNOWN)
            }
        };
        let refuse = |what: &str| {
            unsupported(format!(
                "{what} would mean something else at the new position, and its prefix cannot be declared there"
            ))
        };
        let mut needed: Vec<(u32, String, String)> = Vec::new();
        let mut inner: ScopeStack<&str, u16> = ScopeStack::new();
        let mut stack = vec![Step::Enter(sub)];
        while let Some(step) = stack.pop() {
            let id = match step {
                Step::Exit(mark) => {
                    inner.close(mark);
                    continue;
                }
                Step::Enter(id) => id,
            };
            let n = tree.node(id);
            if n.kind != NodeKind::Element {
                continue;
            }
            let mark = inner.mark();
            for (prefix, a) in tree.declarations(n) {
                inner.bind(prefix, index_of(&tree.declared_uri(a)));
            }
            // What each prefix this element writes must mean, and the declarations that make it so.
            let mut wanted: Vec<(&str, u16, String)> = Vec::new();
            if n.flags & F_LATE_NAME == 0 {
                let name = tree.str_of(n.name);
                let prefix = name.get(..usize::from(n.prefix_len)).unwrap_or("");
                wanted.push((prefix, n.ns, format!("the name {name:?}")));
            }
            for a in tree.node_attrs(n) {
                let written = a.written_prefix_len();
                if a.is_late() || written == 0 || tree.uri(a.ns) == XMLNS_NS {
                    continue;
                }
                let name = tree.str_of(a.name);
                wanted.push((
                    name.get(..written).unwrap_or(""),
                    a.ns,
                    format!("the attribute {name:?}"),
                ));
            }
            let mut here: Vec<(&str, u16)> = Vec::new();
            for (prefix, ns, what) in wanted {
                let means = here
                    .iter()
                    .find(|(p, _)| *p == prefix)
                    .map(|(_, ns)| *ns)
                    .or_else(|| inner.get(prefix))
                    .or_else(|| outer.get(prefix).map(|u| index_of(u)))
                    .or_else(|| (prefix == "xml").then(|| index_of(XML_NS)))
                    .or_else(|| prefix.is_empty().then_some(0));
                if means == Some(ns) {
                    continue;
                }
                let declared_here = here.iter().any(|(p, _)| *p == prefix);
                if declared_here
                    || prefix == "xml"
                    || prefix == "xmlns"
                    || (!prefix.is_empty() && ns == 0)
                {
                    return Err(refuse(&what));
                }
                here.push((prefix, ns));
                needed.push((id, prefix.to_owned(), tree.uri(ns).to_owned()));
            }
            for (prefix, ns) in here {
                inner.bind(prefix, ns);
            }
            stack.push(Step::Exit(mark));
            let mut kids = Vec::new();
            let mut c = n.first;
            while c != NONE {
                kids.push(c);
                c = tree.node(c).next;
            }
            stack.extend(kids.into_iter().rev().map(Step::Enter));
        }
        Ok(needed)
    }

    /// Makes sure the root element binds `uri` and returns its prefix (empty for the default
    /// namespace). An existing binding is reused. Otherwise `hint` is declared on the root after
    /// its last namespace declaration. With `ignorable`, the prefix is also added to
    /// `mc:Ignorable`, and `mc` is declared if missing.
    ///
    /// New elements and attributes find the namespace on their own when they are inserted, and
    /// declare it on the root if it is missing, so this is for what the caller has to decide: the
    /// prefix a namespace should have, and whether it is ignorable.
    pub fn bind_namespace(
        &mut self,
        uri: &str,
        hint: &str,
        ignorable: bool,
    ) -> Result<String, StetError> {
        if uri.is_empty() || uri == XMLNS_NS || uri == XML_NS {
            return Err(invalid("not a bindable namespace URI"));
        }
        let prefix = match self.root_prefix_for(uri, true) {
            Some(p) => p,
            None => {
                let root = self.tree.root;
                let p = self.fresh_prefix(hint, root);
                self.declare_on(root, uri, &p)?;
                p
            }
        };
        if ignorable && !prefix.is_empty() {
            self.add_ignorable(&prefix)?;
        }
        Ok(prefix)
    }

    /// Declares `prefix` for `uri` on the element `el`, after its last namespace declaration, so the rest of its start tag
    /// keeps its bytes. It is for a prefix that something already names and nothing binds, such as a token of
    /// `mc:Ignorable` (decision 0014), so a prefix that is already bound at `el`, `xml`, `xmlns`, the empty prefix, a name
    /// that is not an NCName and a URI that cannot be bound are refused with `invalid_payload`.
    pub fn declare_prefix(&mut self, el: NodeId, prefix: &str, uri: &str) -> Result<(), StetError> {
        self.element(el)?;
        if prefix.is_empty() || !is_ncname(prefix) || prefix == "xml" || prefix == "xmlns" {
            return Err(invalid("not a prefix that can be declared"));
        }
        if uri.is_empty() || uri == XMLNS_NS || uri == XML_NS {
            return Err(invalid("not a bindable namespace URI"));
        }
        if self.tree.prefix_uri_at(el.0, prefix).is_some() {
            return Err(invalid("the prefix is already bound here"));
        }
        self.declare_on(el.0, uri, prefix)
    }

    /// A prefix named after `base` that nothing in scope at `from` uses.
    pub(super) fn fresh_prefix(&self, base: &str, from: u32) -> String {
        let base = if is_ncname(base) && base != "xmlns" && base != "xml" {
            base
        } else {
            "ns"
        };
        let scope = self.tree.scope_at(from);
        let mut candidate = base.to_owned();
        let mut i = 1usize;
        while scope.contains_key(candidate.as_str()) {
            candidate = format!("{base}{i}");
            i += 1;
        }
        candidate
    }

    /// A prefix that means `uri` at element `from`, declaring one if there is none.
    ///
    /// The nearest binding in scope is used. If there is none, the namespace is declared on the
    /// root, as Word does, under a prefix that no declaration between the root and `from` uses, so
    /// the root's declaration reaches `from`. If the root already binds `uri` but a closer
    /// declaration hides that, the namespace is declared on `from` itself under a fresh prefix.
    /// `allow_default` says whether the default namespace will do, which it does for an element
    /// name and does not for an attribute name.
    pub(super) fn pick_prefix(
        &mut self,
        from: u32,
        uri: &str,
        allow_default: bool,
        hint: &str,
    ) -> Result<String, StetError> {
        if uri == XML_NS {
            return Ok("xml".to_owned());
        }
        if let Some(p) = self.tree.find_prefix_at(from, uri, allow_default) {
            return Ok(p);
        }
        match self.root_prefix_for(uri, allow_default) {
            None => {
                let p = self.fresh_prefix(hint, from);
                self.declare_on(self.tree.root, uri, &p)?;
                Ok(p)
            }
            Some(on_root) => {
                let base = if on_root.is_empty() { hint } else { &on_root };
                let p = self.fresh_prefix(base, from);
                self.declare_on(from, uri, &p)?;
                Ok(p)
            }
        }
    }

    /// Adds ` xmlns:prefix="uri"` to an element, or ` xmlns="uri"` for the empty prefix (the default namespace, where an
    /// empty URI undeclares it), after its last declaration or else before its first attribute, as declarations are usually
    /// written.
    fn declare_on(&mut self, el: u32, uri: &str, prefix: &str) -> Result<(), StetError> {
        self.count_declaration()?;
        let n = *self.tree.node(el);
        let escaped = escape_attr(uri, b'"', self.tree.ascii_only)?;
        let text = if prefix.is_empty() {
            format!(" xmlns=\"{escaped}\"")
        } else {
            format!(" xmlns:{prefix}=\"{escaped}\"")
        };
        let span = self.tree.add_bytes(text.as_bytes())?;
        let name_start = span.start as usize + 1;
        let name_len = if prefix.is_empty() {
            5
        } else {
            6 + prefix.len()
        };
        let value_start = name_start + name_len + 2;
        let xmlns = self.tree.intern(XMLNS_NS)?;
        // Keep the declared URI in the table, so that lookups by URI find it.
        if !uri.is_empty() {
            self.tree.intern(uri)?;
        }
        let attr = Attr {
            full: span,
            value: Span::new(value_start, value_start + escaped.len())?,
            name: Span::new(name_start, name_start + name_len)?,
            prefix_len: if prefix.is_empty() { 0 } else { 5 },
            ns: xmlns,
        };
        let mut list: Vec<Attr> = self.tree.node_attrs(&n).to_vec();
        let at = list
            .iter()
            .rposition(|a| a.ns == xmlns)
            .map_or(0, |i| i + 1);
        list.insert(at, attr);
        self.replace_attrs(el, list)?;
        self.flag(el, F_TAG_DIRTY | F_NS_DECL);
        if el != self.tree.root {
            self.tree.nested_ns_decls = true;
        }
        Ok(())
    }

    fn add_ignorable(&mut self, prefix: &str) -> Result<(), StetError> {
        let root = NodeId(self.tree.root);
        let n = self.node(root)?;
        match self.find_attr(&n, MC_NS, "Ignorable") {
            Some(i) => {
                let a = self
                    .tree
                    .node_attrs(&n)
                    .get(i)
                    .copied()
                    .ok_or_else(no_node)?;
                let cur = unescape_attr(self.tree.str_of(a.value)).into_owned();
                if cur.split_whitespace().any(|t| t == prefix) {
                    return Ok(());
                }
                let next = if cur.trim().is_empty() {
                    prefix.to_owned()
                } else {
                    format!("{cur} {prefix}")
                };
                self.set_attr(root, MC_NS, "Ignorable", &next)
            }
            None => {
                self.bind_namespace(MC_NS, "mc", false)?;
                self.set_attr(root, MC_NS, "Ignorable", prefix)
            }
        }
    }
}

/// Used by `late.rs` to cap the length of a chosen prefix.
pub(super) fn prefix_length(prefix: &str) -> Result<u16, StetError> {
    u16::try_from(prefix.len())
        .ok()
        .filter(|&l| l != Attr::LATE)
        .ok_or_else(|| limit("a prefix is 65535 bytes or more"))
}
