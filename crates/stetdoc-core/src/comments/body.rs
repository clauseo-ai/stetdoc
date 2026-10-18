//! The `w:comment` element and its paragraphs, built whole.
//!
//! A comment's paragraphs are never edited in place by a difference: they are made from the text. The
//! first paragraph starts with the `annotationRef` run Word writes, every paragraph has a `paraId` and
//! the `textId` Word writes on most paragraphs, and the styles are the document's (found by name).

use super::xmlw::{self, XML_NS, el, text_el};
use crate::api::error::StetError;
use crate::revisions::handle::PartWriter;
use crate::threads::NS_W14;
use crate::wordml::IdKind;
use crate::xml::{NodeId, SpanTree};

/// The `textId` Word writes on 83% of paragraphs, and D writes on new ones.
const TEXT_ID: &str = "77777777";

/// The paragraph styles of a comment, by id. A document with no styles part has none.
#[derive(Clone, Debug, Default)]
pub(super) struct Styles {
    /// The character style of the `annotationRef` run and of the reference run.
    pub(super) reference: Option<String>,
    /// The paragraph style of the comment's paragraphs.
    pub(super) text: Option<String>,
}

/// One paragraph to write: its text, and the `paraId` it keeps, if it has one.
#[derive(Clone, Debug)]
pub(super) struct ParaSpec {
    /// The text. A tab is `\t`.
    pub(super) text: String,
    /// The `paraId`.
    pub(super) para_id: Option<u32>,
}

/// The paragraphs of a comment text: one per line. An empty text is `None`.
pub(super) fn lines(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// `paraId`s for the lines of a comment: one new id each, and `keep` for the last line if it is given.
pub(super) fn mint(
    tx: &mut PartWriter<'_, '_>,
    part: &str,
    count: usize,
    keep: Option<u32>,
) -> Result<Vec<u32>, StetError> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        if i + 1 == count && keep.is_some() {
            ids.extend(keep);
        } else {
            ids.push(tx.mint_id(IdKind::ParaId, part)?);
        }
    }
    Ok(ids)
}

/// One `w:p`: the style, the `annotationRef` run if it is the first, and the text as runs.
fn paragraph(
    tree: &mut SpanTree,
    w: &str,
    first: bool,
    spec: &ParaSpec,
    styles: &Styles,
) -> Result<NodeId, StetError> {
    let mut kids = Vec::new();
    if let Some(style) = &styles.text {
        let s = el(tree, w, "pStyle", &[(w, "val", style)], &[])?;
        kids.push(el(tree, w, "pPr", &[], &[s])?);
    }
    if first {
        let mut run = Vec::new();
        if let Some(style) = &styles.reference {
            let s = el(tree, w, "rStyle", &[(w, "val", style)], &[])?;
            run.push(el(tree, w, "rPr", &[], &[s])?);
        }
        run.push(el(tree, w, "annotationRef", &[], &[])?);
        kids.push(el(tree, w, "r", &[], &run)?);
    }
    let mut content = Vec::new();
    for (i, piece) in spec.text.split('\t').enumerate() {
        if i > 0 {
            content.push(el(tree, w, "tab", &[], &[])?);
        }
        if !piece.is_empty() {
            let keep = piece.starts_with(' ') || piece.ends_with(' ');
            let attrs: &[xmlw::Attr<'_>] = if keep {
                &[(XML_NS, "space", "preserve")]
            } else {
                &[]
            };
            content.push(text_el(tree, w, "t", attrs, piece)?);
        }
    }
    if !content.is_empty() {
        kids.push(el(tree, w, "r", &[], &content)?);
    }
    let mut attrs: Vec<xmlw::Attr<'_>> = Vec::new();
    let id = spec.para_id.map(super::rows::hex);
    if let Some(id) = &id {
        attrs.push((NS_W14, "paraId", id));
        attrs.push((NS_W14, "textId", TEXT_ID));
    }
    el(tree, w, "p", &attrs, &kids)
}

/// The paragraphs of a comment, in order.
pub(super) fn paragraphs(
    tree: &mut SpanTree,
    w: &str,
    specs: &[ParaSpec],
    styles: &Styles,
) -> Result<Vec<NodeId>, StetError> {
    specs
        .iter()
        .enumerate()
        .map(|(i, s)| paragraph(tree, w, i == 0, s, styles))
        .collect()
}

/// A new `w:comment`, detached, with its paragraphs.
pub(super) struct NewComment<'a> {
    /// The `w:id`.
    pub(super) id: i64,
    /// The `w:author`.
    pub(super) author: &'a str,
    /// The `w:date`.
    pub(super) date: &'a str,
    /// The `w:initials`.
    pub(super) initials: &'a str,
}

/// Builds the comment element.
pub(super) fn comment(
    tree: &mut SpanTree,
    w: &str,
    new: &NewComment<'_>,
    specs: &[ParaSpec],
    styles: &Styles,
) -> Result<NodeId, StetError> {
    let paras = paragraphs(tree, w, specs, styles)?;
    let id = new.id.to_string();
    el(
        tree,
        w,
        "comment",
        &[
            (w, "id", &id),
            (w, "author", new.author),
            (w, "date", new.date),
            (w, "initials", new.initials),
        ],
        &paras,
    )
}

/// The initials Word makes of an author's name: the first letter of each word, in upper case, at most four.
pub(super) fn initials(author: &str) -> String {
    author
        .split_whitespace()
        .filter_map(|w| w.chars().next())
        .flat_map(char::to_uppercase)
        .take(4)
        .collect()
}
