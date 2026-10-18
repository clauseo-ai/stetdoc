//! Word's element order and value types.
//!
//! **Owns.** Word's element order and value types, generated into `generated/` by `cargo
//! xtask codegen` from the Open XML SDK's schema data, and the code that uses them: where a
//! new child element goes, and whether an edit added a violation.
//!
//! **Interface.** `Ctx` (a position in the schema: the declaration of a parent, reached from a
//! part root) with `Ctx::order_of`, and `ctx_at`, which finds the `Ctx` of an element of a tree
//! (through `mc:AlternateContent` and `mc:ProcessContent` as insertion and validation read it);
//! `insert_in_order` and `reorder`; `validate_part`,
//! `validate_subtree`, `validate_unit` and `unit_root`; `walk_unit`, `read_children`,
//! `is_unit_boundary` and `in_taken_branch`, which read a unit as the validator does for code
//! that applies other rules to it (`check`); and, for code that holds `NodeRef`s,
//! `taken_branch` and `disposition`. The module reads a document only through two traits in
//! `tree`, so it names no concrete tree type. `Nodes` is the name, parent, children, attributes
//! and prefixes of one element, which is all that insertion and the markup compatibility rules
//! read. `SchemaTree` adds the root and a text test, which validation reads as well. The span tree
//! implements them in `schema/span.rs`, because `xml` may depend only on `api`: `SpanTree` is a
//! `SchemaTree` over `NodeId`s, and `Refs` (`pub(crate)`) is `Nodes` over `NodeRef`s, which carry
//! their tree, so `ctx_at(part, &Refs::new(), node_ref)` works for code that holds only those.
//! Names are expanded (namespace URI and local name): prefixes belong to the document and never
//! enter a table.
//!
//! **Invariants.**
//! - Anything that inserts a child element calls `insert_in_order`. Hand-ordered insertion is
//!   a bug.
//! - `insert_in_order(ctx, tree, parent, new, prefer)` returns an index into
//!   `tree.children(parent)` and a `Fit`. It reads the children as the validator reads them (one
//!   function replaces each `mc:AlternateContent` by the branch Word takes and each element that
//!   `mc:ProcessContent` keeps the children of by those children, so insertion and validation
//!   agree by construction), runs the content model, a DFA, over every position, and
//!   returns a position that keeps the parent valid when one exists (`Fit::Valid`), even when
//!   that repairs a parent that was missing something. It refuses (`Fit::NoValidSlot`) an element
//!   that may occur only once when the parent has one, valid or not, and any element that cannot
//!   be added without adding a violation. For an invalid content parent it picks the cheapest
//!   position that brings no kind of failure the parent did not have (`Fit::BestEffort`).
//! - A **property container** is a declaration the generator marks reorderable: no wildcard, at
//!   least two symbols, at most three symbols sharing a rank, and symbols that never occur
//!   together (the alternatives of a choice) sharing a rank. That is 98 of 832 declarations: the
//!   property elements (`w:pPr`, `w:rPr`, `w:tblPr`, `w:tcPr`, `w:sectPr`, `w:lvl`, `w:style`),
//!   and also `w:settings`, `w:styles`, `w:numbering` and `w:document`. **Any change to the
//!   children of a property container that is already out of order rewrites it in schema order.**
//!   For an insertion, `insert_in_order` returns `Fit::Reorder(order)` with the whole new order.
//!   For a removal, the caller takes the child out and applies `reorder(ctx, tree, parent, None)`.
//!   It is the identity for a container in order, so calling it after every removal is safe. The
//!   container is the element being edited, so the rewrite is inside the edit's scope, and the old
//!   order problem goes away. Content (`w:p`, `w:r`, `w:tbl`, `w:body`) is never rewritten.
//! - **No new violations.** `validate_part`, `validate_subtree` and `validate_unit` report a
//!   `ViolationSet`: the set of `ViolationKey`s, each the kind, the parent's declaration and the
//!   offending element or attribute, or for an invalid value the value. An element that is not
//!   where the content model puts it is one kind (`OutOfOrder`) whether it is after what must
//!   follow it, a second copy or excluded by another element; in content the key names no
//!   element, and a missing child names none anywhere. A key never contains a path, an index or
//!   an offset. Comparison is by key presence, not count.
//!   **The rule is applied per dirty unit.** An edit touches an element when it adds it or
//!   changes its attributes or its children. In a story part the unit of a touched element is
//!   `unit_root(element)`: the innermost paragraph (`w:p`, in a table cell or not) or the table
//!   (`w:tbl`) that holds it, or the element itself when there is none (the `w:body`, a
//!   block-level `w:sdt`). `validate_unit` checks a unit: the element and what is below it, but
//!   not the insides of the paragraphs and tables below it, which are units of their own. So a
//!   paragraph is checked as itself wherever it stands, and a table for its properties, grid,
//!   rows and cells and the order of what each cell holds. In any other part a unit is each
//!   top-level element (a child of the root) an edit touches. After the edit, the keys of each
//!   dirty unit must be a subset of the keys of its origin (`after.added_since(&before)` is
//!   empty). The origin is the unit the edit started from: the unit itself before the edit; for
//!   a block that was split or cloned, the block it came from; for a block that several blocks
//!   were merged into, the union of the keys of those blocks (`ViolationSet::union`). A new
//!   block has no origin, so every violation in it is new. A whole part is never compared as
//!   one set, nor is a whole table: the key names a declaration and not a place, so one
//!   paragraph with a run before its `w:pPr` would hide a new problem of the same kind in every
//!   other paragraph of the table. `check` implements the rule with the origins the edit
//!   pipeline supplies. Two rules follow, and the edit simulation tests them for insertions and
//!   removals on real documents and on random children of every content model:
//!   - *Rule A.* An edit is never refused because of a problem that was already in the input,
//!     with one deliberate exception. Copying an element that has a problem repeats a key.
//!     Changing a property container that is out of order rewrites it, which removes the old
//!     keys. In content the keys are coarse enough that a new element in a parent that already
//!     has a kind of problem adds no key. The exception: in a parent that is already invalid,
//!     `insert_in_order` refuses (`Fit::NoValidSlot`) an element that could only go where it
//!     brings a kind of failure the parent did not have. A `w:tr` into a `w:tbl` that has a
//!     `w:tblPr` but no `w:tblGrid` and no rows is the example: the row can go only before the
//!     missing grid, which is a new kind of problem, so the caller repairs the table first.
//!   - *Rule B.* A parent that was valid before an edit is valid after it.
//!   - *Known limit.* A new instance of a pattern that is already present in the unit is not new:
//!     a second copy of a bad value, or a second misplaced element in a content parent that
//!     already has one, repeats a key. Exactness comes from property tests on inputs with no
//!     violations, where every edit must leave zero.
//! - **Markup compatibility** follows ECMA-376 Part 3, and `schema/effective.rs` is the **single
//!   source** of how Word reads it. An `mc:AlternateContent` is read as the first `mc:Choice`
//!   whose `Requires` names at least one prefix and only prefixes that resolve to namespaces the
//!   tables know, otherwise as `mc:Fallback`, otherwise as nothing: `taken_branch`. An element or
//!   attribute in a namespace listed in `mc:Ignorable`, on the element itself or an ancestor, is
//!   skipped when the tables do not understand the namespace or do not know the name, and a name
//!   they know in a place they do not allow is a violation. `mc:ProcessContent` keeps the
//!   children of a skipped element in its place: `disposition` answers for one element, as
//!   `Mc::{Normal, Branch, NoBranch, ProcessContent, Skip}`. The code that reads a parent's
//!   children for `insert_in_order`, `reorder` and the validator (`effective_children`) is built
//!   on the same two functions, so it and `wordml`, which reads through `NodeRef`s, cannot
//!   disagree. Nothing in the rule limits depth: a branch that holds an `mc:AlternateContent`
//!   that holds another is followed to the end, and a wrapper that declares `mc:Ignorable` or
//!   `mc:ProcessContent` itself makes them apply below it. For example, in
//!   ```text
//!   <mc:AlternateContent>
//!     <mc:Choice Requires="wps"> ... </mc:Choice>
//!     <mc:Fallback> ... </mc:Fallback>
//!   </mc:AlternateContent>
//!   ```
//!   `taken_branch` is the `mc:Choice` when `wps` is a namespace the tables know, and the
//!   `mc:Fallback` when `Requires` names one they do not (or nothing); with no `mc:Fallback` it is
//!   `None`. A namespace in `mc:MustUnderstand` that the tables do not know is a violation. A
//!   part's root can never be ignored.
//! - **Known limits.** A part whose root is in an ISO 29500 Strict namespace is checked for
//!   structure only, because the generated value types are the Transitional ones. Elements of
//!   other vocabularies (DrawingML, VML, math) are known by name where the schema allows them
//!   and their content is not validated. The SDK data is corrected by an explicit list in
//!   `xtask/src/codegen/supplement/rules.rs`, each entry with its reason and reference, and
//!   generation stops when a rule matches nothing. The schema leaves the children of `w:rPr`
//!   unordered, and Word writes them in the SDK's order, so a departure is
//!   `ViolationKind::NonCanonicalOrder`, not a schema violation.
//! - Files under `generated/` carry a header naming their generator and the pinned input, and
//!   are never edited by hand. CI fails if they are stale or if the directory holds anything
//!   else (`cargo xtask codegen --check`).
//!
//! **Depends on.** `api`, `xml`.
//!
//! **Start reading.** `schema/mod.rs`, then `schema/order.rs`, `schema/effective.rs` and
//! `schema/validate/mod.rs`. `schema/tree.rs` and `schema/span.rs` are the seam to the span tree.
//! The tables are read only through `schema/tables.rs`, which also describes their layout. The design and the corpus results are in the research notes.

#![expect(
    dead_code,
    unused_imports,
    reason = "wordml, revisions, ops and check call into schema; they are built after it"
)]

mod ctx;
mod effective;
mod generated;
mod order;
mod scope;
mod span;
mod tables;
mod tree;
mod validate;
mod value;
mod walk;

pub use ctx::{Ctx, ctx_at};
pub(crate) use effective::{Mc, disposition, taken_branch};
pub use generated::parts::PartKind;
pub use order::{Fit, Placement, Prefer, insert_in_order, reorder};
pub(crate) use span::Refs;
pub use tables::display_qname;
pub use tree::{Attr, Nodes, QName, SchemaTree};
pub use validate::{
    MAX_DETAILS, MC_NS, Report, Violation, ViolationKey, ViolationKind, ViolationSet, XML_NS,
    unit_root, validate_part, validate_subtree, validate_unit,
};
pub use walk::{in_taken_branch, is_unit_boundary, is_word_namespace, read_children, walk_unit};

use generated::parts;

impl PartKind {
    /// The part kind for an OPC content type, if the schema covers it.
    pub fn from_content_type(content_type: &str) -> Option<PartKind> {
        let i = parts::CONTENT_TYPES
            .binary_search_by(|(c, _)| (*c).cmp(content_type))
            .ok()
            .and_then(|i| parts::CONTENT_TYPES.get(i))
            .map(|(_, p)| usize::from(*p))?;
        PartKind::ALL.get(i).copied()
    }

    /// The part kind that a relationship type points at, in the Transitional or the Strict
    /// spelling of the type, if the schema covers it.
    pub fn from_relationship_type(rel_type: &str) -> Option<PartKind> {
        PartKind::ALL
            .iter()
            .copied()
            .find(|kind| same_relationship_type(kind.relationship_type(), rel_type))
    }

    /// The relationship type that points at a part of this kind.
    pub fn relationship_type(self) -> &'static str {
        parts::PART_REL_TYPES
            .get(self as usize)
            .copied()
            .unwrap_or("")
    }

    /// The usual package path of the first part of this kind.
    pub fn default_path(self) -> &'static str {
        parts::PART_PATHS.get(self as usize).copied().unwrap_or("")
    }
}

const TRANSITIONAL_RELATIONSHIPS: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const STRICT_RELATIONSHIPS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

/// Whether `other` is the relationship type `transitional`, or its Strict spelling.
fn same_relationship_type(transitional: &str, other: &str) -> bool {
    if transitional == other {
        return true;
    }
    match (
        other.strip_prefix(STRICT_RELATIONSHIPS),
        transitional.strip_prefix(TRANSITIONAL_RELATIONSHIPS),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Whether a namespace is an Office extension namespace (w14, w15, w16cid, ...). A part that
/// uses one must declare it and list its prefix in `mc:Ignorable`, so that consumers that do
/// not know it can skip it.
pub fn is_extension_namespace(uri: &str) -> bool {
    tables::ns_by_uri(uri)
        .and_then(tables::namespace_row)
        .is_some_and(|r| r.2)
}

/// The prefix Word uses for a namespace (`w`, `w14`, ...), for declaring it in a new part.
pub fn preferred_prefix(uri: &str) -> Option<&'static str> {
    tables::ns_by_uri(uri)
        .and_then(tables::namespace_row)
        .map(|r| r.1)
        .filter(|p| !p.is_empty())
}

#[cfg(test)]
mod sim;
#[cfg(test)]
mod tests;
