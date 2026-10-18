//! Test builders for the modules that depend on `wordml` and may not depend on `xml`.
//!
//! `refs` cannot name `xml`, even in tests, and a `NodeId` cannot be built outside `xml`, so these
//! build a `Story`, or a few distinct `BlockKey`s, from a small XML string.

use super::story::{BlockKey, Story, StoryKind};
use crate::api::limits::Limits;
use crate::xml::{Bytes, SpanTree};
use std::sync::Arc;

/// The namespace declarations the test documents carry.
const DECLS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\" \
xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\" \
xmlns:wps=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"";

/// Parses an XML part held in a string.
pub(crate) fn parse(name: &str, xml: &str) -> SpanTree {
    match SpanTree::parse(name, Bytes::from(xml.as_bytes()), &Limits::default()) {
        Ok(tree) => tree,
        Err(e) => panic!("test XML does not parse: {e:?}"),
    }
}

/// A main document part whose `w:body` holds `body_xml`.
pub(crate) fn document(body_xml: &str) -> SpanTree {
    parse(
        "word/document.xml",
        &format!("<w:document {DECLS}><w:body>{body_xml}</w:body></w:document>"),
    )
}

/// A part with the given root element and content, with the test namespaces declared.
pub(crate) fn part(name: &str, root: &str, content: &str) -> SpanTree {
    parse(name, &format!("<w:{root} {DECLS}>{content}</w:{root}>"))
}

/// The body story of a document whose `w:body` holds `body_xml`.
pub(crate) fn story(body_xml: &str) -> Story {
    let tree = document(body_xml);
    match Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree) {
        Ok(story) => story,
        Err(e) => panic!("test body does not scan: {e:?}"),
    }
}

/// The keys of every block of a story, in order.
pub(crate) fn keys(story: &Story) -> Vec<BlockKey> {
    story.blocks().iter().map(|e| story.key(e)).collect()
}
