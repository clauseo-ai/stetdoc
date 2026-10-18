//! The children Word reads, and the one place that says how Word reads markup compatibility.
//!
//! **The rule.** Word reads an `mc:AlternateContent` as the first `mc:Choice` whose `Requires`
//! names only namespaces the tables know, otherwise as `mc:Fallback`, otherwise as nothing
//! ([`taken_branch`]). An element in a namespace that the document lists in `mc:Ignorable`, and
//! that the tables do not understand or do not know the name in, is skipped with everything in it;
//! if `mc:ProcessContent` lists it, Word reads its children in its place instead ([`disposition`]).
//! `mc:Ignorable` and `mc:ProcessContent` apply to the element that carries them and everything
//! below it.
//!
//! **One source.** [`effective_children`], which `insert_in_order`, `reorder` and the validator
//! read a parent's children through, and [`taken_branch`] and [`disposition`], which code that
//! holds `NodeRef`s (`wordml`) calls, are the same rule over different handles: both go through
//! `taken_branch_in` and `disposition_in`. Reading and validating therefore cannot disagree, and
//! nothing in the rule limits depth: a branch that holds an `mc:AlternateContent` that holds
//! another is followed to the end, with an explicit stack, because the span tree does not recurse
//! on input depth either.

use super::scope::{Ignorable, Skip, is_plain, scope_at, scope_link, skip_of};
use super::span::Refs;
use super::tables;
use super::tree::{Nodes, QName};
use super::validate::MC_NS;
use crate::xml::NodeRef;

/// Whether an expanded name is `mc:<local>`.
pub(super) fn is_mc(name: QName<'_>, local: &str) -> bool {
    name.0 == MC_NS && name.1 == local
}

/// How Word reads one element, as far as markup compatibility goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mc<N> {
    /// As it stands: the element has no namespace, or is a name the tables know in a namespace
    /// they understand, or the document does not make its namespace ignorable.
    Normal,
    /// An `mc:AlternateContent`: the children of this `mc:Choice` or `mc:Fallback` stand in its
    /// place.
    Branch(N),
    /// An `mc:AlternateContent` with no branch Word can read: it stands for nothing.
    NoBranch,
    /// Ignorable, and listed in `mc:ProcessContent`: Word reads its children where it stands.
    ProcessContent,
    /// Ignorable: skipped with everything in it.
    Skip,
}

/// The branch of the `mc:AlternateContent` `ac` that Word reads: the first `mc:Choice` whose
/// `Requires` lists at least one prefix and only prefixes that resolve, at the choice, to
/// namespaces the tables know (the table of namespaces that `is_extension_namespace` reads),
/// otherwise the first `mc:Fallback`, otherwise none.
pub(super) fn taken_branch_in<T: Nodes>(tree: &T, ac: T::Node) -> Option<T::Node> {
    tree.find_child(ac, &mut |b| {
        is_mc(tree.name(b), "Choice") && requires_understood(tree, b)
    })
    .or_else(|| tree.find_child(ac, &mut |b| is_mc(tree.name(b), "Fallback")))
}

/// Whether the `Requires` of a `mc:Choice` names a namespace, and every prefix in it resolves to
/// one the tables know. A `Requires` with no prefix in it requires nothing that can be checked, so
/// the choice is not taken.
fn requires_understood<T: Nodes>(tree: &T, choice: T::Node) -> bool {
    let Some(req) = tree.attribute(choice, "", "Requires") else {
        return false;
    };
    let mut named = false;
    for prefix in req.split_whitespace() {
        named = true;
        let known = tree
            .resolve_prefix(choice, prefix)
            .is_some_and(|uri| tables::ns_by_uri(&uri).is_some());
        if !known {
            return false;
        }
    }
    named
}

/// How Word reads `node`, given the scope at it (`chain` and `ign`: what `mc:Ignorable` and
/// `mc:ProcessContent` say at the element's parent).
pub(super) fn disposition_in<T: Nodes>(
    tree: &T,
    node: T::Node,
    chain: &[Ignorable],
    ign: Option<usize>,
) -> Mc<T::Node> {
    if is_mc(tree.name(node), "AlternateContent") {
        return taken_branch_in(tree, node).map_or(Mc::NoBranch, Mc::Branch);
    }
    match skip_of(tree, node, chain, ign) {
        Skip::No => Mc::Normal,
        Skip::Skipped => Mc::Skip,
        Skip::Processed => Mc::ProcessContent,
    }
}

/// The branch of the `mc:AlternateContent` `ac` that Word reads: the `mc:Choice` or `mc:Fallback`
/// whose children stand in its place, or `None` if no branch is usable. This is the rule of
/// `schema/effective.rs` for code that holds `NodeRef`s. It looks at the branches of `ac` and the
/// namespace declarations in scope at them, nothing else, and allocates only what resolving the
/// prefixes of a `Requires` costs (`NodeRef::namespace_of_prefix` returns a `String`).
///
/// ```text
/// <mc:AlternateContent>
///   <mc:Choice Requires="wps"> text box, the shape Word 2010 writes </mc:Choice>
///   <mc:Fallback> the same text box as VML, for Word 2007 </mc:Fallback>
/// </mc:AlternateContent>
/// ```
/// `wps` is a namespace the tables know, so the `mc:Choice` is the branch. With
/// `Requires="x"` where `x` is a namespace they do not know, it is the `mc:Fallback`.
pub(crate) fn taken_branch<'a>(ac: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    taken_branch_in(&Refs::new(), *ac)
}

/// How Word reads `node`, from the markup compatibility attributes on it and its ancestors. For an
/// `mc:AlternateContent` it is the branch ([`taken_branch`]). For any other element it says
/// whether the element is read as it stands, skipped with its content, or replaced by its
/// children. An element the tables settle at once (no namespace, or a name they know in a
/// namespace they understand, which includes every element of the Word namespace) is answered
/// without reading any attribute; the others cost a walk up their ancestors.
///
/// ```text
/// <w:p mc:Ignorable="x" mc:ProcessContent="x:wrap">
///   <x:wrap><w:r>..</w:r></x:wrap>   ProcessContent: Word reads the run in its place
///   <x:note>..</x:note>              Skip: ignorable and not listed
///   <w:r>..</w:r>                    Normal
/// </w:p>
/// ```
pub(crate) fn disposition<'a>(node: &NodeRef<'a>) -> Mc<NodeRef<'a>> {
    let refs = Refs::new();
    let node = *node;
    let (ns, local) = refs.name(node);
    if is_mc((ns, local), "AlternateContent") {
        // No scope is needed to read a wrapper.
        return taken_branch_in(&refs, node).map_or(Mc::NoBranch, Mc::Branch);
    }
    if is_plain(ns, local) {
        return Mc::Normal;
    }
    let scope = scope_at(&refs, node, false);
    disposition_in(&refs, node, &scope.chain, scope.ign)
}

/// One child Word reads, and the child of the parent it comes from. An `mc:AlternateContent`
/// stands for the elements of its branch, and a processed element for its children; they all
/// share its index. `ign` is the scope the element is read in: what the markup compatibility
/// attributes of its parent, and of the wrappers it came through, say.
pub(super) struct Item<N> {
    pub(super) node: N,
    pub(super) from: usize,
    pub(super) ign: Option<usize>,
    /// Whether Word reads the element: false for one in an ignorable namespace that the tables do
    /// not know, which is skipped with everything in it. It is what `skip_of` says of `node` in the
    /// scope `ign`, kept so that a caller need not ask again.
    pub(super) read: bool,
}

/// A parent's children as Word reads them.
pub(super) struct Effective<N> {
    /// How many element children the parent has. An index into them is what
    /// `insert_in_order` returns.
    pub(super) raw: usize,
    /// The elements Word reads, in order. A wrapper that stands for nothing adds none.
    pub(super) items: Vec<Item<N>>,
    /// Elements of branches that were not taken, with their scope. They are checked below
    /// themselves.
    pub(super) skipped: Vec<(N, Option<usize>)>,
}

/// Replaces each `mc:AlternateContent` among the children of `parent` by the branch Word reads,
/// and each element that `mc:ProcessContent` keeps the children of by those children, however
/// deep they nest. `chain` and `ign` are the scope at the parent: what is in scope for its
/// children. When a wrapper on the way down carries `mc:Ignorable` or `mc:ProcessContent` itself,
/// its declarations are added to `chain`, which only grows, and the items below it are read in
/// the scope they make.
pub(super) fn effective_children<T: Nodes>(
    tree: &T,
    parent: T::Node,
    chain: &mut Vec<Ignorable>,
    ign: Option<usize>,
) -> Effective<T::Node> {
    let children = tree.children(parent);
    let mut out = Effective {
        raw: children.len(),
        items: Vec::with_capacity(children.len()),
        skipped: Vec::new(),
    };
    // What stands in the place of a wrapper, depth first in document order: the stack pops the
    // last pushed first. Most children are not wrappers and never go on it.
    let mut work: Vec<(T::Node, usize, Option<usize>)> = Vec::new();
    for (from, child) in children.into_iter().enumerate() {
        if disposition_in(tree, child, chain, ign) == Mc::Normal {
            out.items.push(Item {
                node: child,
                from,
                ign,
                read: true,
            });
            continue;
        }
        work.push((child, from, ign));
        while let Some((node, from, ign)) = work.pop() {
            match disposition_in(tree, node, chain, ign) {
                Mc::Normal => out.items.push(Item {
                    node,
                    from,
                    ign,
                    read: true,
                }),
                Mc::Skip => out.items.push(Item {
                    node,
                    from,
                    ign,
                    read: false,
                }),
                Mc::Branch(taken) => alternate(
                    tree,
                    chain,
                    node,
                    (from, ign),
                    Some(taken),
                    &mut work,
                    &mut out,
                ),
                Mc::NoBranch => {
                    alternate(tree, chain, node, (from, ign), None, &mut work, &mut out);
                }
                Mc::ProcessContent => {
                    let ign = within(tree, chain, node, ign);
                    work.extend(
                        tree.children(node)
                            .into_iter()
                            .rev()
                            .map(|c| (c, from, ign)),
                    );
                }
            }
        }
    }
    out
}

/// Reads the `mc:AlternateContent` `ac`: the children of the `taken` branch go back on the work
/// list, in the scope the wrapper and the branch make; the children of every other branch are not
/// read, and are left to be checked below themselves.
fn alternate<T: Nodes>(
    tree: &T,
    chain: &mut Vec<Ignorable>,
    ac: T::Node,
    (from, ign): (usize, Option<usize>),
    taken: Option<T::Node>,
    work: &mut Vec<(T::Node, usize, Option<usize>)>,
    out: &mut Effective<T::Node>,
) {
    let ign = within(tree, chain, ac, ign);
    for branch in tree.children(ac) {
        let ign = within(tree, chain, branch, ign);
        let inside = tree.children(branch);
        if Some(branch) == taken {
            work.extend(inside.into_iter().rev().map(|c| (c, from, ign)));
        } else {
            out.skipped.extend(inside.into_iter().map(|c| (c, ign)));
        }
    }
}

/// The scope for the children of `wrapper`: `ign`, plus what `wrapper` declares itself. The
/// declarations go onto `chain`, and the result is their index; with none it is `ign` as it was.
fn within<T: Nodes>(
    tree: &T,
    chain: &mut Vec<Ignorable>,
    wrapper: T::Node,
    ign: Option<usize>,
) -> Option<usize> {
    let attrs = tree.attributes(wrapper);
    if !attrs.iter().any(|a| a.namespace == MC_NS) {
        return ign;
    }
    match scope_link(tree, wrapper, &attrs, ign) {
        Some(link) => {
            chain.push(link);
            Some(chain.len() - 1)
        }
        None => ign,
    }
}

/// For each insertion point among the raw children (`0..=raw`), how many of the elements Word
/// reads come before it. An index into the raw children and a position in the sequence the
/// content model runs over are related by this table.
pub(super) fn boundaries<N>(eff: &Effective<N>) -> Vec<usize> {
    let mut out = Vec::with_capacity(eff.raw + 1);
    let mut before = 0usize;
    for p in 0..=eff.raw {
        while eff.items.get(before).is_some_and(|i| i.from < p) {
            before += 1;
        }
        out.push(before);
    }
    out
}
