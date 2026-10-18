//! The rows of the extension parts: threads and done flags, durable ids, UTC dates and people.
//!
//! Rows are matched by namespace URI and local name, so a prefix other than Word's does not matter.
//! A value that is not eight hexadecimal digits reads as absent, as `wordml::existing_ids` reads ids.

use super::parts::CommentParts;
use crate::api::error::StetError;
use crate::state::PartsView;
use crate::xml::NodeRef;

/// The `w14` namespace: `paraId` on a comment's paragraph.
pub(crate) const NS_W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
/// The `w15` namespace: `commentEx`, `person` and the `done` and `paraIdParent` attributes.
pub(crate) const NS_W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
/// The namespace that Word 2013's preview wrote `w15` in: `Comment042` and `Invalid_Word15Comments` of the
/// corpus use it, and Word reads it as `w15`.
pub(crate) const NS_W15_PREVIEW: &str = "http://schemas.microsoft.com/office/word/2010/11/wordml";
/// The `w16cid` namespace: `commentId`.
pub(crate) const NS_CID: &str = "http://schemas.microsoft.com/office/word/2016/wordml/cid";
/// The `w16cex` namespace: `commentExtensible`.
pub(crate) const NS_CEX: &str = "http://schemas.microsoft.com/office/word/2018/wordml/cex";

/// Eight hexadecimal digits as a number.
fn hex8(text: &str) -> Option<u32> {
    if text.len() == 8 && text.bytes().all(|b| b.is_ascii_hexdigit()) {
        u32::from_str_radix(text, 16).ok()
    } else {
        None
    }
}

/// The `paraId` of an element, in any of the three namespaces Word writes it in.
pub(crate) fn para_id(node: &NodeRef<'_>) -> Option<u32> {
    [NS_W14, NS_W15, NS_W15_PREVIEW, NS_CID]
        .iter()
        .find_map(|ns| node.attr(ns, "paraId"))
        .and_then(|v| hex8(&v))
}

/// An attribute of the `w15` namespace, in either spelling.
pub(crate) fn w15_attr<'a>(node: &NodeRef<'a>, local: &str) -> Option<std::borrow::Cow<'a, str>> {
    node.attr(NS_W15, local)
        .or_else(|| node.attr(NS_W15_PREVIEW, local))
}

/// True if the element is a `w15` element of this name, in either spelling.
pub(crate) fn is_w15(node: &NodeRef<'_>, local: &str) -> bool {
    node.is(NS_W15, local) || node.is(NS_W15_PREVIEW, local)
}

fn durable_id(node: &NodeRef<'_>, ns: &str) -> Option<u32> {
    node.attr(ns, "durableId").and_then(|v| hex8(&v))
}

/// An on/off attribute: `1`, `true` and `on` are set, and anything else is not.
fn on(value: &str) -> bool {
    matches!(value, "1" | "true" | "on")
}

/// A `w15:commentEx`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExRow {
    /// The last paragraph of the comment this row is about.
    pub(super) para: Option<u32>,
    /// The last paragraph of the parent comment, if the row has a parent.
    pub(super) parent: Option<u32>,
    /// True if the row names a parent, whether or not it reads. `parent_missing` needs it.
    pub(super) has_parent: bool,
    /// `w15:done`.
    pub(super) done: bool,
}

/// A `w16cid:commentId`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IdRow {
    /// The last paragraph of the comment.
    pub(super) para: Option<u32>,
    /// The durable id.
    pub(super) durable: Option<u32>,
}

/// A `w16cex:commentExtensible`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExtRow {
    /// The durable id.
    pub(super) durable: Option<u32>,
}

/// The rows of every extension part that exists, and whether each part does.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Rows {
    /// `commentsExtended.xml` exists.
    pub(super) has_extended: bool,
    /// `commentsIds.xml` exists.
    pub(super) has_ids: bool,
    /// `commentsExtensible.xml` exists.
    pub(super) has_extensible: bool,
    /// `people.xml` exists.
    pub(super) has_people: bool,
    /// The rows of `commentsExtended.xml`, in file order.
    pub(super) extended: Vec<ExRow>,
    /// The rows of `commentsIds.xml`.
    pub(super) ids: Vec<IdRow>,
    /// The rows of `commentsExtensible.xml`.
    pub(super) extensible: Vec<ExtRow>,
    /// The authors of `people.xml`.
    pub(super) people: Vec<String>,
}

impl Rows {
    /// Reads every extension part that exists.
    pub(super) fn read(view: &PartsView<'_>, parts: &CommentParts) -> Result<Rows, StetError> {
        let mut rows = Rows {
            has_extended: parts.extended.is_some(),
            has_ids: parts.ids.is_some(),
            has_extensible: parts.extensible.is_some(),
            has_people: parts.people.is_some(),
            ..Rows::default()
        };
        if let Some(part) = parts.extended {
            for n in view.tree(part)?.root().child_elements() {
                if is_w15(&n, "commentEx") {
                    let parent = w15_attr(&n, "paraIdParent");
                    rows.extended.push(ExRow {
                        para: para_id(&n),
                        parent: parent.as_deref().and_then(hex8),
                        has_parent: parent.is_some(),
                        done: w15_attr(&n, "done").is_some_and(|v| on(&v)),
                    });
                }
            }
        }
        if let Some(part) = parts.ids {
            for n in view.tree(part)?.root().child_elements() {
                if n.is(NS_CID, "commentId") {
                    rows.ids.push(IdRow {
                        para: para_id(&n),
                        durable: durable_id(&n, NS_CID),
                    });
                }
            }
        }
        if let Some(part) = parts.extensible {
            for n in view.tree(part)?.root().child_elements() {
                if n.is(NS_CEX, "commentExtensible") {
                    rows.extensible.push(ExtRow {
                        durable: durable_id(&n, NS_CEX),
                    });
                }
            }
        }
        if let Some(part) = parts.people {
            for n in view.tree(part)?.root().child_elements() {
                if is_w15(&n, "person") {
                    rows.people
                        .push(w15_attr(&n, "author").map_or_else(String::new, |a| a.into_owned()));
                }
            }
        }
        Ok(rows)
    }
}
