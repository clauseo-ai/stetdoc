//! The ids a document already uses, so `state` can mint ones that collide with none of them.
//!
//! `wordml` only reports. `state` mints, deterministically, and bumps past the collisions this
//! reports. `docs/design/wordml.md` 3.5 says which kinds read which attributes, and in which parts.

use super::names::{W, W14, W15, W15_PREVIEW, W16CEX, W16CID, attr_in, is_in};
use super::rels::{RelKind, Rels, StorySpec};
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

/// Which ids to read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[expect(
    clippy::enum_variant_names,
    reason = "the variants are named for the attributes callers know (paraId, textId, durableId) and agreed with state"
)]
pub enum IdKind {
    /// `w14:paraId`, and the `paraId` that `w15` (in either of its namespaces) and `w16cid` use to name
    /// a paragraph.
    ParaId,
    /// `w14:textId`.
    TextId,
    /// `w16cid:durableId` on a comment id, and `w16cex:durableId` on a comment.
    CommentDurableId,
    /// `w:id` on any element: revisions, bookmarks, comment ranges, comments, move ranges.
    AnnotationId,
}

/// Eight hexadecimal digits, as a number.
fn hex8(value: &str) -> Option<u32> {
    // `from_str_radix` takes a leading `+`, which is not a digit
    if value.len() == 8 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        u32::from_str_radix(value, 16).ok()
    } else {
        None
    }
}

/// A non-negative whole number in decimal that fits in a `u32`.
fn decimal(value: &str) -> Option<u32> {
    let v = value.trim();
    if v.is_empty() || !v.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    v.parse::<u32>().ok()
}

fn read(kind: IdKind, n: &NodeRef<'_>, out: &mut Vec<u32>) {
    match kind {
        IdKind::ParaId => {
            if let Some(v) = attr_in(n, &[W14, W15, W15_PREVIEW, super::names::W16CID], "paraId") {
                out.extend(hex8(&v));
            }
        }
        IdKind::TextId => {
            if let Some(v) = attr_in(n, &[W14], "textId") {
                out.extend(hex8(&v));
            }
        }
        IdKind::CommentDurableId => {
            let holder =
                is_in(n, &[W16CID], "commentId") || is_in(n, &[W16CEX], "commentExtensible");
            if holder && let Some(v) = attr_in(n, &[W16CID, W16CEX], "durableId") {
                out.extend(hex8(&v));
            }
        }
        IdKind::AnnotationId => {
            if let Some(v) = attr_in(n, &W, "id") {
                out.extend(decimal(&v));
            }
        }
    }
}

/// Appends every value of `kind` found in `tree`, in document order, skipping values that are not
/// well formed. It reads both branches of an `mc:AlternateContent`, so a value used in either is
/// reported. One read-only pass over the elements and their attributes, with no allocation per
/// element.
pub fn existing_ids(kind: IdKind, tree: &SpanTree, out: &mut Vec<u32>) {
    let mut stack = vec![tree.root().children()];
    read(kind, &tree.root(), out);
    while let Some(top) = stack.last_mut() {
        match top.next() {
            None => {
                stack.pop();
            }
            Some(n) if n.is_element() => {
                read(kind, &n, out);
                stack.push(n.children());
            }
            Some(_) => {}
        }
    }
}

/// The parts to read with [`existing_ids`] for `kind`: part names in a fixed order, each once.
///
/// Paragraph ids are read from every story and from the comment extension parts that name
/// paragraphs. Text ids come from every story. Annotation ids come from every story and from the styles
/// and numbering parts, whose `w:pPrChange` and `w:rPrChange` carry ids of their own. Durable comment
/// ids come from the two comment id parts. The parts that are not stories are found by relationship
/// type in the main part's relationships.
pub fn id_parts(kind: IdKind, stories: &[StorySpec], main_rels: &Rels) -> Vec<Arc<str>> {
    let mut out: Vec<Arc<str>> = Vec::new();
    let mut add = |name: &str| {
        if !out.iter().any(|p| &**p == name) {
            out.push(Arc::from(name));
        }
    };
    let story_parts = |add: &mut dyn FnMut(&str)| {
        for s in stories {
            add(&s.part);
        }
    };
    match kind {
        IdKind::ParaId => {
            story_parts(&mut add);
            for k in [RelKind::CommentsExtended, RelKind::CommentsIds] {
                if let Some(name) = main_rels.first_part(k) {
                    add(name);
                }
            }
        }
        IdKind::TextId => story_parts(&mut add),
        IdKind::AnnotationId => {
            story_parts(&mut add);
            for k in [RelKind::Styles, RelKind::Numbering] {
                if let Some(name) = main_rels.first_part(k) {
                    add(name);
                }
            }
        }
        IdKind::CommentDurableId => {
            for k in [RelKind::CommentsIds, RelKind::CommentsExtensible] {
                if let Some(name) = main_rels.first_part(k) {
                    add(name);
                }
            }
        }
    }
    out
}
