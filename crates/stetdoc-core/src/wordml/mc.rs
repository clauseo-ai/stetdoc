//! Which children Word reads under an `mc:AlternateContent`, and under an element that
//! `mc:ProcessContent` names.
//!
//! Word reads the first `mc:Choice` whose `Requires` prefixes all resolve to namespaces it
//! understands, else `mc:Fallback`, else nothing; and for an element in an ignorable namespace that
//! it does not know, it reads the children in its place if `mc:ProcessContent` names it and skips the
//! element with its content if not. The rule lives once, in `schema` (`schema/effective.rs`), so that
//! reading and validating cannot disagree, and this file is the one place `wordml` calls it from.

use super::names::{MC, is_in};
use crate::xml::NodeRef;

pub(super) use crate::schema::Mc;

/// True if `n` is an `mc:AlternateContent` element.
pub(super) fn is_alternate_content(n: &NodeRef<'_>) -> bool {
    is_in(n, &[MC], "AlternateContent")
}

/// True if `n` is an `mc:Choice` or `mc:Fallback` element.
pub(super) fn is_branch(n: &NodeRef<'_>) -> bool {
    is_in(n, &[MC], "Choice") || is_in(n, &[MC], "Fallback")
}

/// The branch Word reads for an `mc:AlternateContent`: the `mc:Choice` or `mc:Fallback` element
/// whose children stand in its place, if any.
pub(super) fn taken_branch<'a>(ac: &NodeRef<'a>) -> Option<NodeRef<'a>> {
    crate::schema::taken_branch(ac)
}

/// How Word reads an element that is not in the Word namespace: as it stands, skipped with its
/// content, replaced by its children, or, for an `mc:AlternateContent`, by its branch.
pub(super) fn disposition<'a>(n: &NodeRef<'a>) -> Mc<NodeRef<'a>> {
    crate::schema::disposition(n)
}
