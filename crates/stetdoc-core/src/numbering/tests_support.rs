//! Builders for the numbering tests: definitions, styles and paragraphs from small XML strings.

use super::counters::StoryNumbers;
use crate::api::view::Mode;
use crate::styles::Cascade;
use crate::wordml::testing::{document, part};
use crate::wordml::{NumFact, NumberingDefs, Story, StoryKind, StyleDefs};
use std::sync::Arc;

/// A `w:lvl` with start 1, a format and a text, and anything extra inside it.
pub(super) fn lvl(ilvl: u8, fmt: &str, text: &str, extra: &str) -> String {
    format!(
        "<w:lvl w:ilvl=\"{ilvl}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/>{extra}\
         <w:lvlText w:val=\"{text}\"/></w:lvl>"
    )
}

/// A `w:abstractNum`.
pub(super) fn abs(id: i64, inner: &str) -> String {
    format!("<w:abstractNum w:abstractNumId=\"{id}\">{inner}</w:abstractNum>")
}

/// A `w:num` over an abstract definition, with overrides.
pub(super) fn num(id: i64, abstract_id: i64, inner: &str) -> String {
    format!("<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{abstract_id}\"/>{inner}</w:num>")
}

/// A `w:lvlOverride` that only sets the start.
pub(super) fn start_override(ilvl: u8, value: i64) -> String {
    format!("<w:lvlOverride w:ilvl=\"{ilvl}\"><w:startOverride w:val=\"{value}\"/></w:lvlOverride>")
}

/// A paragraph numbered directly.
pub(super) fn p(num_id: i64, ilvl: u8) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{ilvl}\"/><w:numId w:val=\"{num_id}\"/></w:numPr></w:pPr>\
         <w:r><w:t>x</w:t></w:r></w:p>"
    )
}

/// A paragraph with a style and nothing else.
pub(super) fn ps(style: &str) -> String {
    format!("<w:p><w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>")
}

/// A paragraph with arbitrary `pPr` content and arbitrary content after it.
pub(super) fn pp(ppr: &str, inner: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr>{inner}</w:p>")
}

/// A paragraph style with `numPr` content.
pub(super) fn style_num(id: &str, based_on: Option<&str>, num_pr: &str) -> String {
    let based = based_on.map_or(String::new(), |b| format!("<w:basedOn w:val=\"{b}\"/>"));
    format!(
        "<w:style w:type=\"paragraph\" w:styleId=\"{id}\"><w:name w:val=\"{id}\"/>{based}\
         <w:pPr><w:numPr>{num_pr}</w:numPr></w:pPr></w:style>"
    )
}

/// The numbering of a body, as clause numbers, one entry per paragraph in document order.
pub(super) fn run(numbering: &str, styles: &str, body: &str, mode: Mode) -> Vec<Option<String>> {
    detail(numbering, styles, body, mode).0
}

/// The clause numbers and the numbers table itself.
pub(super) fn detail(
    numbering: &str,
    styles: &str,
    body: &str,
    mode: Mode,
) -> (Vec<Option<String>>, StoryNumbers, Vec<NumFact>) {
    let defs = NumberingDefs::read(&part("word/numbering.xml", "numbering", numbering));
    let style_tree = part("word/styles.xml", "styles", styles);
    let cascade = Cascade::build(&StyleDefs::read(&style_tree), None, None);
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree)
        .expect("test body scans");
    let facts = story.num_facts(&tree);
    let numbers = StoryNumbers::build(&facts, &defs, &cascade, mode);
    let labels = facts
        .iter()
        .map(|f| numbers.clause_number(f.node).map(str::to_string))
        .collect();
    (labels, numbers, facts)
}

/// The labels as `&str` options, for comparing with an expected list.
pub(super) fn labels(v: &[Option<String>]) -> Vec<Option<&str>> {
    v.iter().map(|s| s.as_deref()).collect()
}

/// A `w:lvl` with an optional start.
pub(super) fn lvl_start(
    ilvl: u8,
    start: Option<i64>,
    fmt: &str,
    text: &str,
    extra: &str,
) -> String {
    let start = start.map_or(String::new(), |s| format!("<w:start w:val=\"{s}\"/>"));
    format!(
        "<w:lvl w:ilvl=\"{ilvl}\">{start}<w:numFmt w:val=\"{fmt}\"/>{extra}<w:lvlText w:val=\"{text}\"/></w:lvl>"
    )
}

/// The three-level legal list used by many tests: `1.`, `1.1`, `(a)` under a `1.1` prefix.
pub(super) fn legal_list(id: i64) -> String {
    abs(
        id,
        &format!(
            "{}{}{}",
            lvl(0, "decimal", "%1.", ""),
            lvl(1, "decimal", "%1.%2", ""),
            lvl(2, "lowerLetter", "(%3)", "")
        ),
    )
}

/// A body of numbered paragraphs, given as (num id, level).
pub(super) fn body(items: &[(i64, u8)]) -> String {
    items.iter().map(|(n, l)| p(*n, *l)).collect()
}
