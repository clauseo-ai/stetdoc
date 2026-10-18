//! Shapes built to be slow (`docs/design/wordml.md` 9): each must be read, never panic and stay
//! linear. Each runs on another thread with a generous time limit, as `xml/tests_ns.rs` does, because
//! the core reads no clock. A hostile size is also a hostile depth, and `xml` bounds depth before
//! `wordml` runs, so each deep shape is read twice: once with the depth limit raised, to hold
//! `wordml` to its explicit stacks, and once at the default limit, at the deepest tree that parses and
//! one level past it.

use super::tests_hostile::{exercise, parse_with, within, wrap};
use super::tests_text::{FINAL, ORIGINAL, REDLINE, check_invariants};
use super::*;
use crate::api::error::ErrorCode;
use crate::xml::SpanTree;
use std::sync::Arc;

/// The body story of a body with these children, parsed with this depth limit.
fn read(body: &str, depth: u32) -> (SpanTree, Story) {
    let tree = parse_with(
        depth,
        &wrap("document", &format!("<w:body>{body}</w:body>")),
    )
    .unwrap();
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (tree, story)
}

/// Everything that reads a whole story, once.
fn whole(tree: &SpanTree, story: &Story) -> usize {
    assert_eq!(story.fingerprints(tree).len(), story.blocks().len());
    let _ = (
        story.anchors(tree),
        story.revisions(tree),
        story.fields(tree),
    );
    let refreshed = Story::refresh(story, tree, &[]).unwrap();
    assert!(refreshed.story.same_as(story));
    (0..story.blocks().len())
        .filter_map(|i| story.paragraph(tree, i))
        .map(|p| p.map().len(REDLINE))
        .sum()
}

fn anomalies(m: &TextMap, kind: AnomalyKind) -> usize {
    m.anomalies().iter().filter(|a| a.kind == kind).count()
}

fn run(inner: &str) -> String {
    format!("<w:r>{inner}</w:r>")
}

/// The deepest `n` for which `shape(n)` parses with the default limits. One more is refused with
/// `limit_exceeded`, and the deepest is read.
fn at_the_default_limit(shape: impl Fn(usize) -> String) {
    let parses = |n: usize| {
        let xml = wrap("document", &format!("<w:body>{}</w:body>", shape(n)));
        parse_with(256, &xml)
    };
    let (mut lo, mut hi) = (1usize, 256usize);
    while lo + 1 < hi {
        let mid = (lo + hi) / 2;
        if parses(mid).is_ok() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    assert!(lo > 50, "the deepest shape that parses is only {lo} deep");
    let past = parses(lo + 1).err().expect("one level deeper is refused");
    assert_eq!(past.code(), ErrorCode::LimitExceeded);
    let tree = parses(lo).unwrap();
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    assert!(whole(&tree, &story) > 0 || !story.blocks().is_empty());
    exercise(&tree);
}

// ---- fields -------------------------------------------------------------------------------------

#[test]
fn a_hundred_thousand_field_begins_in_one_paragraph() {
    within(90, || {
        let n = 100_000;
        let begin = run("<w:fldChar w:fldCharType=\"begin\"/>");
        let (tree, story) = read(&format!("<w:p>{}</w:p>", begin.repeat(n)), 256);
        let m = story.paragraph(&tree, 0).unwrap().map();
        // nesting is clamped at 32: the rest are ignored and counted, whatever they would have held
        assert_eq!(story.fields(&tree).len(), 32);
        assert_eq!(anomalies(&m, AnomalyKind::FieldTooDeep), n - 32);
        assert_eq!(anomalies(&m, AnomalyKind::UnterminatedField), 32);
        assert!(m.is_empty(FINAL));
        check_invariants(&m);
        whole(&tree, &story);
    });
}

#[test]
fn fifty_thousand_whole_fields_and_a_hundred_thousand_stray_field_characters() {
    within(90, || {
        let n = 50_000;
        let field = format!(
            "{}{}{}{}{}",
            run("<w:fldChar w:fldCharType=\"begin\"/>"),
            run("<w:instrText> PAGE </w:instrText>"),
            run("<w:fldChar w:fldCharType=\"separate\"/>"),
            run("<w:t>1</w:t>"),
            run("<w:fldChar w:fldCharType=\"end\"/>")
        );
        let (tree, story) = read(&format!("<w:p>{}</w:p>", field.repeat(n)), 256);
        assert_eq!(story.fields(&tree).len(), n);
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(m.len(FINAL), n);
        check_invariants(&m);
        whole(&tree, &story);
        let stray = format!(
            "{}{}",
            run("<w:fldChar w:fldCharType=\"end\"/>"),
            run("<w:fldChar w:fldCharType=\"separate\"/>")
        );
        let (tree, story) = read(&format!("<w:p>{}</w:p>", stray.repeat(n)), 256);
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(anomalies(&m, AnomalyKind::StrayFieldChar), 2 * n);
        assert!(story.fields(&tree).is_empty());
        whole(&tree, &story);
    });
}

#[test]
fn a_field_across_a_hundred_thousand_paragraphs() {
    within(90, || {
        let n = 100_000;
        let mut body = String::from(
            "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/><w:instrText> TOC </w:instrText><w:fldChar w:fldCharType=\"separate\"/></w:r></w:p>",
        );
        body.push_str(&"<w:p><w:r><w:t>entry</w:t></w:r></w:p>".repeat(n));
        body.push_str("<w:p><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>");
        let (tree, story) = read(&body, 256);
        assert_eq!(story.fields(&tree).len(), 1);
        let m = story.paragraph(&tree, n / 2).unwrap().map();
        assert_eq!(m.fields().len(), 1);
        assert!(m.items().iter().all(|i| i.flags & FIELD_RESULT != 0));
        assert_eq!(whole(&tree, &story), 5 * n);
    });
}

// ---- depth --------------------------------------------------------------------------------------

#[test]
fn ten_thousand_nested_content_controls_in_a_paragraph() {
    within(90, || {
        let n = 10_000;
        let open = "<w:sdt><w:sdtContent>".repeat(n);
        let close = "</w:sdtContent></w:sdt>".repeat(n);
        let (tree, story) = read(
            &format!("<w:p>{open}<w:r><w:t>x</w:t></w:r>{close}</w:p>"),
            30_000,
        );
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(m.text(FINAL), "x");
        let chain: Vec<WrapperKind> = m.context(m.items()[0].ctx).map(|c| c.kind).collect();
        assert_eq!(chain.len(), n);
        assert!(chain.iter().all(|k| *k == WrapperKind::Control));
        whole(&tree, &story);
        exercise(&tree);
    });
    at_the_default_limit(|n| {
        let open = "<w:sdt><w:sdtContent>".repeat(n);
        let close = "</w:sdtContent></w:sdt>".repeat(n);
        format!("<w:p>{open}<w:r><w:t>x</w:t></w:r>{close}</w:p>")
    });
}

#[test]
fn ten_thousand_nested_content_controls_around_a_paragraph() {
    within(90, || {
        let n = 10_000;
        let open = "<w:sdt><w:sdtContent>".repeat(n);
        let close = "</w:sdtContent></w:sdt>".repeat(n);
        let (tree, story) = read(
            &format!("{open}<w:p><w:r><w:t>x</w:t></w:r></w:p>{close}"),
            30_000,
        );
        assert_eq!(story.blocks().len(), 1);
        assert!(story.blocks()[0].has(IN_CONTROL));
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(
            m.context(m.items()[0].ctx).count(),
            n,
            "the chain reaches the story root"
        );
        whole(&tree, &story);
    });
    at_the_default_limit(|n| {
        let open = "<w:sdt><w:sdtContent>".repeat(n);
        let close = "</w:sdtContent></w:sdt>".repeat(n);
        format!("{open}<w:p><w:r><w:t>x</w:t></w:r></w:p>{close}")
    });
}

#[test]
fn five_thousand_nested_tables() {
    within(90, || {
        let n = 5_000;
        let open = "<w:tbl><w:tr><w:tc>".repeat(n);
        let close = "</w:tc></w:tr></w:tbl>".repeat(n);
        let (tree, story) = read(
            &format!("{open}<w:p><w:r><w:t>x</w:t></w:r></w:p>{close}"),
            30_000,
        );
        assert_eq!(story.blocks().len(), 2 * n + 1);
        let last = story.blocks().len() - 1;
        assert_eq!(story.blocks()[last].parent, Some(last as u32 - 1));
        assert_eq!(story.join_run(&tree, last, FINAL), last..last + 1);
        assert!(story.own_parts(&tree, 0).is_some() && story.own_parts(&tree, last).is_none());
        assert_eq!(whole(&tree, &story), 1);
        exercise(&tree);
    });
    at_the_default_limit(|n| {
        let open = "<w:tbl><w:tr><w:tc>".repeat(n);
        let close = "</w:tc></w:tr></w:tbl>".repeat(n);
        format!("{open}<w:p><w:r><w:t>x</w:t></w:r></w:p>{close}")
    });
}

#[test]
fn nested_revision_wrappers_and_alternate_content_hundreds_deep() {
    within(90, || {
        // 1,000 levels (4,000 elements) in a release build; 300 levels in a debug build, which is
        // still hundreds deep and keeps `cargo xtask check` inside the guard on a loaded machine.
        let n = if cfg!(debug_assertions) { 300 } else { 1_000 };
        let open =
            "<w:ins w:id=\"1\"><w:del w:id=\"2\"><mc:AlternateContent><mc:Choice Requires=\"wps\">"
                .repeat(n);
        let close = "</mc:Choice></mc:AlternateContent></w:del></w:ins>".repeat(n);
        let (tree, story) = read(
            &format!("<w:p>{open}<w:r><w:delText>x</w:delText></w:r>{close}</w:p>"),
            30_000,
        );
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(
            (m.text(FINAL), m.text(ORIGINAL), m.text(REDLINE)),
            (String::new().into(), String::new().into(), "x".into())
        );
        assert_eq!(m.context(m.items()[0].ctx).count(), 3 * n);
        whole(&tree, &story);
    });
}

// ---- width --------------------------------------------------------------------------------------

#[test]
fn a_hundred_thousand_runs_in_one_paragraph() {
    within(90, || {
        let n = 100_000;
        let (tree, story) = read(
            &format!("<w:p>{}</w:p>", "<w:r><w:t>x</w:t></w:r>".repeat(n)),
            256,
        );
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(m.items().len(), n);
        assert_eq!(m.len(FINAL), n);
        assert_eq!(m.locate(FINAL, 0..n).unwrap().spans.len(), n);
        assert_eq!(m.gap(FINAL, n / 2).unwrap().right, Some(n / 2));
        assert_eq!(m.offset_of(FINAL, m.items()[n - 1].node, 0), Some(n - 1));
        assert_eq!(m.find_all(FINAL, "xx").len(), n / 2);
        assert_eq!(m.char_to_utf16(FINAL, n), Some(n));
        assert_eq!(m.utf16_to_char(FINAL, n), Some(n));
        check_invariants(&m);
        whole(&tree, &story);
    });
}

#[test]
fn a_hundred_thousand_revision_wrappers_in_one_paragraph() {
    within(90, || {
        let n = 50_000;
        let pair = "<w:ins w:id=\"1\"><w:r><w:t>x</w:t></w:r></w:ins><w:del w:id=\"2\"><w:r><w:delText>y</w:delText></w:r></w:del>";
        let (tree, story) = read(&format!("<w:p>{}</w:p>", pair.repeat(n)), 256);
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert!(m.has_revisions());
        assert_eq!(
            (m.len(FINAL), m.len(ORIGINAL), m.len(REDLINE)),
            (n, n, 2 * n)
        );
        assert_eq!(story.revisions(&tree).len(), 2 * n);
        check_invariants(&m);
        whole(&tree, &story);
    });
}

#[test]
fn one_text_of_several_megabytes() {
    within(90, || {
        let n = 4_000_000;
        let (tree, story) = read(
            &format!(
                "<w:p><w:r><w:t>{}</w:t></w:r></w:p>",
                "\u{1F600}a".repeat(n / 2)
            ),
            256,
        );
        let m = story.paragraph(&tree, 0).unwrap().map();
        assert_eq!(m.len(FINAL), n);
        assert_eq!(m.char_to_utf16(FINAL, n), Some(n + n / 2));
        assert_eq!(m.utf16_to_char(FINAL, n + n / 2), Some(n));
        assert_eq!(m.find_all(FINAL, "a\u{1F600}").len(), n / 2 - 1);
        assert_eq!(m.locate(FINAL, 7..n - 7).unwrap().spans.len(), 1);
        whole(&tree, &story);
    });
}

#[test]
fn thirty_thousand_bookmarks_in_every_arrangement() {
    within(90, || {
        let n = 30_000;
        let mut shapes: Vec<String> = Vec::new();
        let pair = |i: usize| {
            format!("<w:bookmarkStart w:id=\"{i}\" w:name=\"b{i}\"/><w:bookmarkEnd w:id=\"{i}\"/>")
        };
        shapes.push((0..n).map(pair).collect()); // side by side
        let starts: String = (0..n)
            .map(|i| format!("<w:bookmarkStart w:id=\"{i}\" w:name=\"b{i}\"/>"))
            .collect();
        let ends: String = (0..n)
            .rev()
            .map(|i| format!("<w:bookmarkEnd w:id=\"{i}\"/>"))
            .collect();
        shapes.push(format!("{starts}{ends}")); // all nested
        shapes.push(format!(
            "{}{}",
            "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>".repeat(n),
            "<w:bookmarkEnd w:id=\"1\"/>".repeat(n)
        )); // one id
        let comments: String = (0..n)
            .map(|i| format!("<w:commentRangeStart w:id=\"{i}\"/><w:commentRangeEnd w:id=\"{i}\"/><w:r><w:commentReference w:id=\"{i}\"/></w:r>"))
            .collect();
        shapes.push(comments);
        for content in shapes {
            let (tree, story) = read(&format!("<w:p>{content}</w:p>"), 256);
            let a = story.anchors(&tree);
            assert!(a.bookmarks.len() + a.comments.len() >= n);
            let m = story.paragraph(&tree, 0).unwrap().map();
            check_invariants(&m);
            // the same markers between blocks, which is where most real ones are
            let (tree, story) = read(&format!("{content}<w:p/>"), 256);
            assert!(
                story.anchors(&tree).bookmarks.len() + story.anchors(&tree).comments.len() >= n
            );
            whole(&tree, &story);
        }
    });
}

// ---- many blocks --------------------------------------------------------------------------------

#[test]
fn a_hundred_thousand_paragraphs() {
    within(90, || {
        let n = 100_000;
        let (tree, story) = read(&"<w:p><w:r><w:t>x</w:t></w:r></w:p>".repeat(n), 256);
        assert_eq!(story.blocks().len(), n);
        assert_eq!(whole(&tree, &story), n);
        assert_eq!(story.fingerprints(&tree).len(), n);
        // every mark deleted: one paragraph by the rule of 3.6, and the join is read once
        let del =
            "<w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr><w:r><w:t>x</w:t></w:r></w:p>";
        let (tree, story) = read(&del.repeat(n), 256);
        let run = story.join_run(&tree, 0, FINAL);
        assert_eq!(run, 0..n - 1 + 1);
        let (range, joined) = story.joined(&tree, 0, FINAL).unwrap();
        assert_eq!((range, joined.len()), (0..n, n));
        assert_eq!(joined.locate(0..n).unwrap().len(), n);
        assert_eq!(story.join_run(&tree, 0, ORIGINAL), 0..1);
        whole(&tree, &story);
    });
}

#[test]
fn twenty_thousand_sections_and_relationships() {
    within(90, || {
        let n = 20_000;
        let t = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
        let body: String = (0..n)
            .map(|i| format!("<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId{i}\"/></w:sectPr></w:pPr></w:p>"))
            .collect();
        let (tree, _) = read(&body, 256);
        let rels = Rels::new(
            (0..n)
                .map(|i| Rel {
                    id: format!("rId{i}"),
                    rel_type: t.to_string(),
                    target: RelTarget::Part(format!("word/header{i}.xml")),
                })
                .collect(),
        );
        let got = stories("word/document.xml", &tree, &rels);
        assert_eq!(got.len(), n + 1);
        assert_eq!(got[n].kind, StoryKind::Header);
    });
}
