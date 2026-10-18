//! Namespaces and the small reads every other file of `wordml` shares.
//!
//! WordprocessingML has two dialects, Transitional (the one Word writes) and ISO 29500 Strict, with
//! different namespace URIs for the same elements. `wordml` accepts both everywhere, by matching the
//! URI against both, so no caller names a dialect. Names are matched on the local name first, because
//! that is a short comparison, and the namespace second.

use crate::xml::NodeRef;
use std::borrow::Cow;

/// The `w` namespace, Transitional.
pub(super) const W_T: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// The `w` namespace, Strict.
pub(super) const W_S: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
/// The relationships namespace (`r:id`), Transitional.
pub(super) const R_T: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// The relationships namespace, Strict.
pub(super) const R_S: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
/// Office Math, Transitional.
pub(super) const M_T: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";
/// Office Math, Strict.
pub(super) const M_S: &str = "http://purl.oclc.org/ooxml/officeDocument/math";
/// The XML namespace, for `xml:space`.
pub(super) const XML: &str = "http://www.w3.org/XML/1998/namespace";
/// Markup compatibility (`mc:AlternateContent`), the same in both dialects.
pub(super) const MC: &str = crate::xml::MC_NS;
/// Word 2010 extensions (`w14:paraId`, `w14:textId`, `w14:checkbox`).
pub(super) const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
/// Word 2012 extensions (`w15:commentEx`).
pub(super) const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
/// The namespace Word 2013's preview wrote `w15` in, which Word reads as `w15`.
pub(super) const W15_PREVIEW: &str = "http://schemas.microsoft.com/office/word/2010/11/wordml";
/// Comment ids (`w16cid:commentId`, `w16cid:durableId`).
pub(super) const W16CID: &str = "http://schemas.microsoft.com/office/word/2016/wordml/cid";
/// Comment extensibility (`w16cex:commentExtensible`).
pub(super) const W16CEX: &str = "http://schemas.microsoft.com/office/word/2018/wordml/cex";
/// The Word 2006 namespace that two Strict documents in the corpus use for a text box's content.
pub(super) const W2006: &str = "http://schemas.microsoft.com/office/word/2006/wordml";

/// The `w` namespaces of both dialects.
pub(super) const W: [&str; 2] = [W_T, W_S];
/// The relationships namespaces of both dialects.
pub(super) const R: [&str; 2] = [R_T, R_S];
/// The math namespaces of both dialects.
pub(super) const M: [&str; 2] = [M_T, M_S];

/// True for a namespace URI of the `w` vocabulary in either dialect.
pub(super) fn is_word_ns(uri: &str) -> bool {
    uri == W_T || uri == W_S
}

/// True if `n` is the element `local` of the `w` vocabulary, in either dialect.
pub(super) fn is_w(n: &NodeRef<'_>, local: &str) -> bool {
    n.is_element() && n.local_name() == local && is_word_ns(n.namespace())
}

/// True if `n` is the element `local` in one of `namespaces`.
pub(super) fn is_in(n: &NodeRef<'_>, namespaces: &[&str], local: &str) -> bool {
    n.is_element() && n.local_name() == local && namespaces.contains(&n.namespace())
}

/// The decoded value of the attribute `local` of `n` in one of `namespaces`.
pub(super) fn attr_in<'a>(
    n: &NodeRef<'a>,
    namespaces: &[&str],
    local: &str,
) -> Option<Cow<'a, str>> {
    n.attrs()
        .find(|a| a.local_name() == local && namespaces.contains(&a.namespace()))
        .map(|a| a.value())
}

/// The attribute `local` in the `w` namespace of either dialect, such as `w:val`.
pub(super) fn w_attr<'a>(n: &NodeRef<'a>, local: &str) -> Option<Cow<'a, str>> {
    attr_in(n, &W, local)
}

/// The attribute `local` in the relationships namespace of either dialect, such as `r:id`.
pub(super) fn r_attr<'a>(n: &NodeRef<'a>, local: &str) -> Option<Cow<'a, str>> {
    attr_in(n, &R, local)
}

/// Reads `ST_OnOff`: `false`, `0` and `off` are false, anything else is true. An element that is
/// present without a value is true, so callers pass `None` for a missing attribute.
pub(super) fn on_off(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("false" | "0" | "off"))
}

/// Reads a whole number written in decimal, ignoring surrounding whitespace.
pub(super) fn int(value: &str) -> Option<i64> {
    value.trim().parse::<i64>().ok()
}

/// The elements that are the content of a table, a row and a cell, where the property container of
/// each (`w:tblPr`, `w:trPr`, `w:tcPr`) stops being looked for: it comes before them.
pub(super) const TABLE_CONTENT: [&str; 3] = ["tr", "sdt", "customXml"];
/// See [`TABLE_CONTENT`].
pub(super) const ROW_CONTENT: [&str; 3] = ["tc", "sdt", "customXml"];
/// See [`TABLE_CONTENT`].
pub(super) const CELL_CONTENT: [&str; 4] = ["p", "tbl", "sdt", "customXml"];

/// How many element children of a table, row or cell are looked at for its property container, if no
/// content comes first. Word writes the container first, so this only bounds input built to make
/// every paragraph of a row read 100,000 elements before its first cell.
const LEADING_CAP: usize = 64;

/// The first `w:` child of `n` named `local` among the children before its first `content` element,
/// which is where a table, a row or a cell keeps its properties, and among at most [`LEADING_CAP`] of
/// them.
pub(super) fn leading_child<'a>(
    n: &NodeRef<'a>,
    local: &str,
    content: &[&str],
) -> Option<NodeRef<'a>> {
    n.child_elements()
        .take(LEADING_CAP)
        .take_while(|c| !content.iter().any(|name| is_w(c, name)))
        .find(|c| is_w(c, local))
}

/// The first child element of `n` that is `local` of the `w` vocabulary.
pub(super) fn w_child<'a>(n: &NodeRef<'a>, local: &str) -> Option<NodeRef<'a>> {
    n.child_elements().find(|c| is_w(c, local))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::limits::Limits;
    use crate::xml::{Bytes, SpanTree};

    fn tree(xml: &str) -> SpanTree {
        SpanTree::parse("t.xml", Bytes::from(xml.as_bytes()), &Limits::default()).unwrap()
    }

    #[test]
    fn both_dialects_are_the_w_vocabulary() {
        for ns in [W_T, W_S] {
            let t = tree(&format!(
                "<w:p xmlns:w=\"{ns}\" xmlns:r=\"{R_T}\"><w:r w:val=\"x\" r:id=\"rId3\"/></w:p>"
            ));
            let r = t.root().first_child().unwrap();
            assert!(is_w(&t.root(), "p"));
            assert!(is_w(&r, "r"));
            assert!(!is_w(&r, "p"));
            assert_eq!(w_attr(&r, "val").as_deref(), Some("x"));
            assert_eq!(r_attr(&r, "id").as_deref(), Some("rId3"));
        }
    }

    #[test]
    fn another_namespace_is_not_w() {
        let t = tree("<w:p xmlns:w=\"urn:other\"/>");
        assert!(!is_w(&t.root(), "p"));
        assert!(is_in(&t.root(), &["urn:other"], "p"));
    }

    #[test]
    fn on_off_follows_st_onoff() {
        assert!(on_off(None));
        assert!(on_off(Some("true")));
        assert!(on_off(Some("1")));
        assert!(on_off(Some("on")));
        assert!(!on_off(Some("false")));
        assert!(!on_off(Some("0")));
        assert!(!on_off(Some(" off ")));
        assert!(on_off(Some("nonsense")));
    }

    #[test]
    fn int_reads_decimal_only() {
        assert_eq!(int(" 12 "), Some(12));
        assert_eq!(int("-1"), Some(-1));
        assert_eq!(int("1.5"), None);
        assert_eq!(int(""), None);
    }
}
