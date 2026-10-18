//! `mc:Ignorable` and `mc:ProcessContent`: which namespaces a part tells a consumer to skip, and
//! which ignored elements keep their children. What is in scope at a place is read from the
//! ancestors ([`Nodes::parent`]), so the validator, which carries the chain down as it
//! walks, and `insert_in_order`, which starts from one parent, see the same thing. The branch of
//! an `mc:AlternateContent` that Word reads, and the replacement of a processed element by its
//! children, are applied in `schema/effective.rs`.

use super::tables;
use super::tree::{Attr, Nodes};
use super::validate::MC_NS;

/// Namespaces made ignorable by `mc:Ignorable`, as a chain from the innermost element out.
pub(super) struct Ignorable {
    pub(super) parent: Option<usize>,
    pub(super) uris: Vec<String>,
    /// `mc:ProcessContent`: (namespace URI, local name or `*`) whose children are kept when
    /// the element itself is ignored.
    pub(super) process: Vec<(String, String)>,
}

/// The `mc:Ignorable` and `mc:ProcessContent` in scope at a place: a chain of the elements that
/// declare them, innermost last, and the index of the innermost.
pub(super) struct Scope<N> {
    pub(super) chain: Vec<Ignorable>,
    pub(super) ign: Option<usize>,
    /// The topmost element above the place (the place itself when it has no parent).
    pub(super) root: N,
}

/// How many ancestors a walk up the tree follows. A tree is finite and the engine limits depth far
/// below this; the cap only keeps a broken `Nodes` from looping.
pub(super) const MAX_ANCESTORS: usize = 100_000;

/// The scope at `node`, read from its ancestors. With `with_self`, the element's own
/// declarations count too: those are in scope for its children.
pub(super) fn scope_at<T: Nodes>(tree: &T, node: T::Node, with_self: bool) -> Scope<T::Node> {
    let mut path = vec![node];
    let mut cur = node;
    while let Some(p) = tree.parent(cur) {
        path.push(p);
        cur = p;
        if path.len() > MAX_ANCESTORS {
            break;
        }
    }
    path.reverse();
    let root = path.first().copied().unwrap_or(node);
    if !with_self {
        path.pop();
    }
    let mut chain: Vec<Ignorable> = Vec::new();
    let mut ign = None;
    for a in path {
        if let Some(link) = scope_link(tree, a, &tree.attributes(a), ign) {
            chain.push(link);
            ign = Some(chain.len() - 1);
        }
    }
    Scope { chain, ign, root }
}

/// Whether an element in an ignorable, not understood namespace is listed in
/// `mc:ProcessContent` in scope.
pub(super) fn is_processed(
    chain: &[Ignorable],
    mut at: Option<usize>,
    uri: &str,
    local: &str,
) -> bool {
    while let Some(i) = at {
        let Some(link) = chain.get(i) else {
            return false;
        };
        if link
            .process
            .iter()
            .any(|(u, l)| u == uri && (l == "*" || l == local))
        {
            return true;
        }
        at = link.parent;
    }
    false
}

/// Whether an element or attribute is skipped: the document lists its namespace in
/// `mc:Ignorable`, and either the tables do not understand the namespace or they do not know
/// the name at all. A name the tables know in a position they do not allow is a violation even
/// when its namespace is ignorable: ECMA-376 Part 3, M.2.2, lets only a consumer that does not
/// understand a namespace skip it, and an understood name in the wrong place is not a name the
/// consumer is missing. The second clause keeps the tables' gaps (an extension attribute they
/// never listed) from becoming false violations.
pub(super) fn is_skipped(chain: &[Ignorable], at: Option<usize>, uri: &str, local: &str) -> bool {
    !uri.is_empty()
        && is_ignorable(chain, at, uri)
        && (!tables::is_understood(uri) || !tables::is_known_name(uri, local))
}

pub(super) fn is_ignorable(chain: &[Ignorable], mut at: Option<usize>, uri: &str) -> bool {
    while let Some(i) = at {
        let Some(link) = chain.get(i) else {
            return false;
        };
        if link.uris.iter().any(|u| u == uri) {
            return true;
        }
        at = link.parent;
    }
    false
}

/// The `mc:Ignorable` and `mc:ProcessContent` declared on one element, if any.
pub(super) fn scope_link<T: Nodes>(
    tree: &T,
    node: T::Node,
    attrs: &[Attr<'_>],
    parent: Option<usize>,
) -> Option<Ignorable> {
    let mut uris: Vec<String> = Vec::new();
    let mut process: Vec<(String, String)> = Vec::new();
    for a in attrs.iter().filter(|a| a.namespace == MC_NS) {
        match a.local {
            "Ignorable" => uris.extend(
                a.value
                    .split_whitespace()
                    .filter_map(|p| tree.resolve_prefix(node, p)),
            ),
            "ProcessContent" => {
                for q in a.value.split_whitespace() {
                    let (prefix, local) = q.split_once(':').unwrap_or(("", q));
                    if let Some(uri) = tree.resolve_prefix(node, prefix) {
                        process.push((uri, local.to_string()));
                    }
                }
            }
            _ => {}
        }
    }
    (!uris.is_empty() || !process.is_empty()).then_some(Ignorable {
        parent,
        uris,
        process,
    })
}

/// Whether the tables settle an element at once: it has no namespace, or it is a name in a
/// namespace the tables understand that they know. Such an element is never skipped, whatever the
/// document lists as ignorable, so nothing about the scope needs to be read.
pub(super) fn is_plain(ns: &str, local: &str) -> bool {
    ns.is_empty() || (tables::is_understood(ns) && tables::is_known_name(ns, local))
}

/// How an element that may not belong in its parent's content is treated under Part 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Skip {
    /// Not ignorable: the content model decides.
    No,
    /// Ignorable: skipped, with everything in it.
    Skipped,
    /// Ignorable and listed in `mc:ProcessContent`: skipped, but its children stay.
    Processed,
}

/// Whether `node` is skipped. An element is in the scope of the `mc:Ignorable` and
/// `mc:ProcessContent` it carries itself as well as of those of its ancestors (ECMA-376 Part 3),
/// so `<x:foo mc:Ignorable="x"/>` is skipped without any ancestor listing `x`. The element's
/// own attributes are read only for an element in a namespace the tables do not understand, or
/// with a name they do not know, which is rare.
pub(super) fn skip_of<T: Nodes>(
    tree: &T,
    node: T::Node,
    chain: &[Ignorable],
    ign: Option<usize>,
) -> Skip {
    let (ns, local) = tree.name(node);
    // An element is skipped only if an ancestor lists its namespace as ignorable or it lists some
    // itself, and almost none does either, so the tables are asked only then.
    if ns.is_empty()
        || (!is_ignorable(chain, ign, ns) && !tree.has_attribute_in(node, MC_NS))
        || is_plain(ns, local)
    {
        return Skip::No;
    }
    let own = scope_link(tree, node, &tree.attributes(node), None);
    let ignorable = is_ignorable(chain, ign, ns)
        || own.as_ref().is_some_and(|l| l.uris.iter().any(|u| u == ns));
    if !ignorable {
        return Skip::No;
    }
    let processed = is_processed(chain, ign, ns, local)
        || own.as_ref().is_some_and(|l| {
            l.process
                .iter()
                .any(|(u, n)| u == ns && (n == "*" || n == local))
        });
    if processed {
        Skip::Processed
    } else {
        Skip::Skipped
    }
}
