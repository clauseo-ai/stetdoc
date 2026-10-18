//! The validator: reads an element tree through [`SchemaTree`] and reports every place it
//! departs from the generated schema, as a comparable set.
//!
//! **No new violations.** `check` validates a unit of a document (a block of a story part, or a
//! top-level element of another part) before an edit and after it, and refuses the edit when
//! `after.added_since(&before)` is not empty: the keys of the violations after, minus the keys
//! of the violations of the unit it came from. A new unit has no origin, so any violation in it
//! is new. For that, a violation has two parts:
//!
//! * a [`ViolationKey`] that says *what* is wrong and *where in the schema*, never where in
//!   the file: the kind, the parent's declaration, and the offending element or attribute; for
//!   a bad value, the value. An element that is not where the content model puts it is one
//!   problem (`OutOfOrder`) whether it is a second copy, comes after what must follow it, or is
//!   excluded by another element: an edit can change which of these applies to the same element.
//!   In content (a paragraph, a table) an order problem and a missing child are keyed by the
//!   parent alone: an edit cannot rewrite content, so it must not be refused for putting an
//!   element where it adds fewer problems than any other place. Inserting a paragraph moves
//!   every later paragraph, and a key that contained a path or an index would call all their
//!   old problems new. Copying an element that already has a problem (splitting a run copies
//!   its properties) repeats a key, which is not new.
//! * the node, for the report.
//!
//! Counts per key are kept for reports. They take no part in the comparison.
//!
//! **Markup compatibility.** `mc:AlternateContent` is replaced by the branch Word reads (the
//! first `mc:Choice` whose `Requires` namespaces are all known, otherwise `mc:Fallback`)
//! before the parent's content is checked, by the same function `insert_in_order` uses. The
//! other branches are checked below their own elements. Elements and attributes in a namespace
//! listed in `mc:Ignorable`, on the element itself or an ancestor, that the schema does not
//! name are skipped.
//!
//! **Strict.** A part whose root is in an ISO 29500 Strict namespace is checked for structure
//! only. Strict values differ from Transitional ones (units on measures, `start` and `end`),
//! and the generated value types are the Transitional ones.

mod attrs;
mod children;
pub(in crate::schema) use children::KNOWN_BUDGET;
mod unit;

use self::attrs::{AttrCtx, check_attributes};
use self::children::{ChildCtx, check_children};
pub(in crate::schema) use self::unit::is_boundary;
pub use self::unit::{unit_root, validate_unit};
use super::Ctx;
use super::PartKind;
use super::effective::effective_children;
use super::generated::parts;
use super::scope::{Ignorable, scope_at, scope_link};
use super::tables::{self, Model, NONE, display_qname};
use super::tree::SchemaTree;
use std::collections::BTreeMap;

/// The WordprocessingML main namespace (`w:`).
pub const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// The WordprocessingML main namespace in the Strict spelling of ISO 29500.
pub const W_STRICT_NS: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
/// The Markup Compatibility namespace (`mc:`).
pub const MC_NS: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
/// The XML namespace (`xml:space`, `xml:lang`).
pub const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";

/// How many violations a [`Report`] keeps in detail. Counts are kept for all of them.
pub const MAX_DETAILS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// The kinds of departure from the schema the validator reports.
pub enum ViolationKind {
    /// The root element is not the one the part must have.
    WrongRoot,
    /// A known element that the content model does not allow in this parent.
    UnexpectedChild,
    /// An element in a namespace the schema does not know, not listed in `mc:Ignorable`.
    UnknownElement,
    /// The element is allowed in this parent but not after what precedes it.
    OutOfOrder,
    /// The element is allowed here but appears more often than the model permits. It is the
    /// kind of a [`Violation`], and its key says `OutOfOrder`: an edit can change whether the
    /// same surplus element reads as a second copy or as a conflict, and that is no new
    /// violation.
    TooMany,
    /// In a property container that the schema leaves unordered (`w:rPr`, `w:trPr`), a child
    /// is not in the order Word writes, or repeats. Not a schema violation; Word reads any
    /// order, but it only writes this one.
    NonCanonicalOrder,
    /// The content ends before a required element.
    MissingChild,
    /// `mc:MustUnderstand` names a namespace that is not one Word's namespaces.
    MustUnderstand,
    /// Character data in an element that has element-only content.
    UnexpectedText,
    /// An attribute the schema does not define on this element.
    UnknownAttribute,
    /// A required attribute is absent.
    MissingAttribute,
    /// The attribute value does not satisfy its type.
    InvalidAttributeValue,
}

impl ViolationKind {
    /// The kind in a [`ViolationKey`]: an element that is not where the content model puts it
    /// is one problem whether it is a second copy, after what must follow it, or excluded by
    /// another element.
    pub fn keyed(self) -> ViolationKind {
        match self {
            ViolationKind::TooMany => ViolationKind::OutOfOrder,
            other => other,
        }
    }

    /// A stable snake_case name, for reports.
    pub fn name(self) -> &'static str {
        match self {
            ViolationKind::WrongRoot => "wrong_root",
            ViolationKind::UnexpectedChild => "unexpected_child",
            ViolationKind::UnknownElement => "unknown_element",
            ViolationKind::OutOfOrder => "out_of_order",
            ViolationKind::TooMany => "too_many",
            ViolationKind::NonCanonicalOrder => "non_canonical_order",
            ViolationKind::MissingChild => "missing_child",
            ViolationKind::MustUnderstand => "must_understand",
            ViolationKind::UnexpectedText => "unexpected_text",
            ViolationKind::UnknownAttribute => "unknown_attribute",
            ViolationKind::MissingAttribute => "missing_attribute",
            ViolationKind::InvalidAttributeValue => "invalid_attribute_value",
        }
    }
}

/// What is wrong, independent of where in the file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ViolationKey {
    /// What is wrong.
    pub kind: ViolationKind,
    /// The parent's declaration, `w:CT_PPr/w:pPr`.
    pub parent: String,
    /// The offending element or attribute, `w:foo`, or `#text`. Empty for an order problem in
    /// content (a parent that is not a property container) and for a missing child.
    pub subject: String,
    /// For an invalid value, the value (clipped). Empty otherwise.
    pub extra: String,
}

#[derive(Clone, Debug)]
/// One violation in a part, with the node it was found at.
pub struct Violation<N> {
    /// What is wrong, as a person reads it. The key's kind can be coarser ([`ViolationKind::keyed`]).
    pub kind: ViolationKind,
    /// The comparable identity of the violation.
    pub key: ViolationKey,
    /// The offending node (the parent, for a missing child).
    pub node: N,
    /// A message for people. Not part of the identity.
    pub detail: String,
}

/// Violations counted by key.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViolationSet {
    counts: BTreeMap<ViolationKey, u32>,
}

impl ViolationSet {
    /// Violations, counting repeats.
    pub fn total(&self) -> u32 {
        self.counts.values().sum()
    }
    /// Whether there are no violations.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
    /// How many times the key occurs.
    pub fn count(&self, key: &ViolationKey) -> u32 {
        self.counts.get(key).copied().unwrap_or(0)
    }
    /// Whether the key occurs at least once.
    pub fn contains(&self, key: &ViolationKey) -> bool {
        self.counts.contains_key(key)
    }
    /// Keys with their counts, in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&ViolationKey, u32)> {
        self.counts.iter().map(|(k, v)| (k, *v))
    }
    pub(crate) fn add(&mut self, key: ViolationKey) {
        *self.counts.entry(key).or_insert(0) += 1;
    }
    /// The keys present here and not in `before`. This is the "no new violations" test: an
    /// edit is acceptable when the result is empty.
    pub fn added_since(&self, before: &ViolationSet) -> Vec<ViolationKey> {
        self.counts
            .keys()
            .filter(|k| !before.contains(k))
            .cloned()
            .collect()
    }
    /// The keys of both sets, counts added. The origin of a block that several blocks were merged
    /// into (two paragraphs joined by deleting the mark between them) is the union of their sets:
    /// a problem that either had is not new in the merged block.
    pub fn union(&self, other: &ViolationSet) -> ViolationSet {
        let mut counts = self.counts.clone();
        for (key, n) in &other.counts {
            *counts.entry(key.clone()).or_insert(0) += n;
        }
        ViolationSet { counts }
    }
}

#[derive(Debug)]
/// The result of validating one part.
pub struct Report<N> {
    /// Up to [`MAX_DETAILS`] violations, in document order.
    pub violations: Vec<Violation<N>>,
    /// Every violation, counted by key.
    pub set: ViolationSet,
    /// Whether there were more violations than `violations` holds.
    pub truncated: bool,
}

pub(super) struct Sink<N> {
    pub(super) report: Report<N>,
}

impl<N> Sink<N> {
    pub(super) fn push(&mut self, key: ViolationKey, node: N, detail: String) {
        let kind = key.kind;
        let key = ViolationKey {
            kind: kind.keyed(),
            ..key
        };
        self.report.set.add(key.clone());
        if self.report.violations.len() < MAX_DETAILS {
            self.report.violations.push(Violation {
                kind,
                key,
                node,
                detail,
            });
        } else {
            self.report.truncated = true;
        }
    }
}

pub(super) fn key(
    kind: ViolationKind,
    parent: &str,
    subject: String,
    extra: String,
) -> ViolationKey {
    ViolationKey {
        kind,
        parent: parent.to_string(),
        subject,
        extra,
    }
}

pub(super) struct Frame<N> {
    pub(super) node: N,
    pub(super) decl: u16,
    pub(super) ign: Option<usize>,
}

/// Validates one part. `kind` selects the root declaration the tree must have.
pub fn validate_part<T: SchemaTree>(kind: PartKind, tree: &T) -> Report<T::Node> {
    let root_decl = parts::PART_ROOTS
        .get(kind as usize)
        .copied()
        .unwrap_or(NONE);
    validate_from(root_decl, tree)
}

/// Validates the subtree of one element, for a check that only needs the part of a document an
/// edit touched. `ctx` is the element's context. The `mc:Ignorable` scope and the Strict test are
/// read from its ancestors. The keys are the same ones [`validate_part`] gives for the same
/// problems, so a subtree before and after an edit can be compared like whole parts.
pub fn validate_subtree<T: SchemaTree>(ctx: Ctx, tree: &T, node: T::Node) -> Report<T::Node> {
    walk(ctx, tree, node, false)
}

/// Checks `node` and what is below it. With `stop_at_boundaries`, a paragraph or a table below
/// `node` is not entered (see [`validate_unit`]).
pub(super) fn walk<T: SchemaTree>(
    ctx: Ctx,
    tree: &T,
    node: T::Node,
    stop_at_boundaries: bool,
) -> Report<T::Node> {
    let scope = scope_at(tree, node, false);
    let strict = tables::is_strict_uri(tree.name(scope.root).0);
    let frame = Frame {
        node,
        decl: ctx.decl(),
        ign: scope.ign,
    };
    walk_from(tree, frame, strict, scope.chain, stop_at_boundaries)
}

fn walk_from<T: SchemaTree>(
    tree: &T,
    start: Frame<T::Node>,
    strict: bool,
    mut chain: Vec<Ignorable>,
    stop_at_boundaries: bool,
) -> Report<T::Node> {
    let mut sink = Sink {
        report: Report {
            violations: Vec::new(),
            set: ViolationSet::default(),
            truncated: false,
        },
    };
    let first = start.node;
    let mut stack = vec![start];
    while let Some(frame) = stack.pop() {
        if stop_at_boundaries && frame.node != first && unit::is_boundary(tree, frame.node) {
            continue;
        }
        check_element(tree, frame, strict, &mut chain, &mut stack, &mut sink);
    }
    sink.report
}

/// The elements the validator reaches when it checks the unit at `node`, in the order it reaches
/// them. A paragraph or a table that a unit stops at is among them. The tests of `walk_unit` use it
/// to say that a rule about what Word reads sees every element the validator checks.
#[cfg(test)]
pub(in crate::schema) fn visited<T: SchemaTree>(
    ctx: Ctx,
    tree: &T,
    node: T::Node,
    stop_at_boundaries: bool,
) -> Vec<T::Node> {
    let scope = scope_at(tree, node, false);
    let strict = tables::is_strict_uri(tree.name(scope.root).0);
    let start = Frame {
        node,
        decl: ctx.decl(),
        ign: scope.ign,
    };
    let mut chain = scope.chain;
    let mut sink = Sink {
        report: Report {
            violations: Vec::new(),
            set: ViolationSet::default(),
            truncated: false,
        },
    };
    let mut seen = Vec::new();
    let mut stack = vec![start];
    while let Some(frame) = stack.pop() {
        seen.push(frame.node);
        if stop_at_boundaries && frame.node != node && unit::is_boundary(tree, frame.node) {
            continue;
        }
        check_element(tree, frame, strict, &mut chain, &mut stack, &mut sink);
    }
    seen
}

fn validate_from<T: SchemaTree>(root_decl: u16, tree: &T) -> Report<T::Node> {
    let root = tree.root();
    let (ns, local) = tree.name(root);
    let expected = tables::decl_row(root_decl).map(|r| r.0);
    let actual = tables::ns_by_uri(ns).and_then(|n| tables::name_id(n, local));
    if expected.is_none() || expected != actual {
        let mut sink = Sink {
            report: Report {
                violations: Vec::new(),
                set: ViolationSet::default(),
                truncated: false,
            },
        };
        sink.push(
            key(
                ViolationKind::WrongRoot,
                &tables::decl_display(root_decl),
                display_qname(ns, local),
                String::new(),
            ),
            root,
            "the root element is not the one this part must have".to_string(),
        );
        return sink.report;
    }
    let strict = tables::is_strict_uri(ns);
    let frame = Frame {
        node: root,
        decl: root_decl,
        ign: None,
    };
    walk_from(tree, frame, strict, Vec::new(), false)
}

fn check_element<T: SchemaTree>(
    tree: &T,
    frame: Frame<T::Node>,
    strict: bool,
    chain: &mut Vec<Ignorable>,
    stack: &mut Vec<Frame<T::Node>>,
    sink: &mut Sink<T::Node>,
) {
    let Frame { node, decl, ign } = frame;
    let Some(&(_, _, flags, _, _)) = tables::decl_row(decl) else {
        return;
    };
    if flags & tables::FLAG_OPAQUE != 0 {
        return;
    }
    let parent_label = tables::decl_display(decl);
    let attrs = tree.attributes(node);

    // mc:Ignorable and mc:ProcessContent on this element extend the scope of the subtree.
    let mut ign = ign;
    if let Some(link) = scope_link(tree, node, &attrs, ign) {
        chain.push(link);
        ign = Some(chain.len() - 1);
    }
    for a in attrs
        .iter()
        .filter(|a| a.namespace == MC_NS && a.local == "MustUnderstand")
    {
        for p in a.value.split_whitespace() {
            let known = tree
                .resolve_prefix(node, p)
                .is_some_and(|uri| tables::ns_by_uri(&uri).is_some());
            if !known {
                sink.push(
                    key(
                        ViolationKind::MustUnderstand,
                        &parent_label,
                        p.to_string(),
                        String::new(),
                    ),
                    node,
                    format!("mc:MustUnderstand names a namespace that is not understood: {p}"),
                );
            }
        }
    }

    let ctx = AttrCtx {
        node,
        decl,
        parent_label: &parent_label,
        chain,
        ign,
        strict,
    };
    check_attributes(&ctx, &attrs, sink);

    if flags & tables::FLAG_TEXT == 0 && tree.has_text(node) {
        sink.push(
            key(
                ViolationKind::UnexpectedText,
                &parent_label,
                "#text".into(),
                String::new(),
            ),
            node,
            "character data in an element with element-only content".into(),
        );
    }

    let Some(model) = Model::of_decl(decl) else {
        return;
    };
    // Reading the children can add to the scopes, when a wrapper carries mc:Ignorable itself.
    let read = effective_children(tree, node, chain, ign);
    let cc = ChildCtx {
        model: &model,
        parent_label: &parent_label,
        canonical: flags & tables::FLAG_CANON != 0,
        reorderable: flags & tables::FLAG_REORDER != 0,
        chain,
    };
    check_children(tree, node, read, &cc, stack, sink);
}
