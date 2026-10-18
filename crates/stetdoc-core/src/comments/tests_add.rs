//! Adding comments and replying to them.

use super::add::{add, reply};
use super::tests_support::*;
use super::*;
use crate::api::error::ErrorCode;
use crate::api::view::Mode;
use crate::threads::tests_support::*;
use crate::threads::{Threads, audit};

fn plain() -> Pkg {
    Pkg::new(
        "<w:p><w:r><w:t>The Purchase Price is payable.</w:t></w:r></w:p><w:p><w:r><w:t>Second.</w:t></w:r></w:p>",
    )
}

/// A document that Word 365 wrote: all five parts, and the two styles under another language's ids.
fn word() -> Pkg {
    let italian = "<w:style w:type=\"character\" w:styleId=\"Rimandocommento\"><w:name w:val=\"annotation reference\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Testocommento\"><w:name w:val=\"annotation text\"/></w:style>";
    plain()
        .styles(italian)
        .comments(&[])
        .extended(&[])
        .ids(&[])
        .extensible(&[])
        .people(&["Ann"])
}

#[test]
fn a_comment_on_a_range_of_a_document_with_no_comment_parts() {
    let state = plain().open();
    let mut tx = state.begin();
    let block = para(&state, 0);
    let fx = {
        let mut c = compiler(&mut tx, &stamp());
        let fx = add(&mut c, &stamp(), &block, 4..18, "Check the price.", None).unwrap();
        c.finish().unwrap();
        fx
    };
    assert_eq!((fx.action, fx.id, fx.thread), (CommentAction::Added, 0, 0));
    assert_eq!(fx.author, "Clauseo");
    assert_eq!(fx.blocks, vec![block.clone()]);
    let parts: Vec<&str> = fx.parts.iter().map(|p| &**p).collect();
    assert_eq!(
        parts,
        [
            "word/document.xml",
            "word/comments.xml",
            "word/commentsExtended.xml"
        ]
    );
    let doc = text(&tx, "word/document.xml");
    assert!(
        doc.contains("<w:commentRangeStart w:id=\"0\"/><w:r><w:t>Purchase Price</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r>"),
        "{doc}"
    );
    let xml = text(&tx, "word/comments.xml");
    assert!(
        xml.contains("w:author=\"Clauseo\" w:date=\"2026-10-06T10:00:00Z\" w:initials=\"C\""),
        "{xml}"
    );
    assert!(xml.contains("<w:annotationRef/>"));
    assert!(xml.contains("<w:t>Check the price.</w:t>"));
    assert!(text(&tx, "word/commentsExtended.xml").contains("w15:done=\"0\""));
    // It reads back, anchored, and nothing is wrong with the parts.
    let view = tx.parts();
    let t = Threads::of(&view).unwrap();
    let c = t.comment(0).unwrap();
    assert_eq!(c.author.as_deref(), Some("Clauseo"));
    assert_eq!(
        t.covered_text(&view, 0, Mode::Final, 100)
            .unwrap()
            .as_deref(),
        Some("Purchase Price")
    );
    assert!(audit(&view).unwrap().is_empty());
    // The paragraph's text in every mode is what it was.
    for mode in [Mode::Final, Mode::Original, Mode::Redline] {
        let story = view.story(view.id("word/document.xml").unwrap()).unwrap();
        let tree = view.tree(view.id("word/document.xml").unwrap()).unwrap();
        let p = story.paragraph(tree, 0).unwrap();
        assert_eq!(p.text(mode), "The Purchase Price is payable.");
    }
    let (next, _) = tx.commit().unwrap();
    assert_eq!(Threads::of(&next.parts()).unwrap().counts().comments, 1);
}

#[test]
fn a_comment_in_a_document_word_wrote_keeps_every_part_in_step() {
    let state = word().open();
    let mut tx = state.begin();
    let block = para(&state, 0);
    let who = stamp();
    {
        let mut c = compiler(&mut tx, &who);
        add(&mut c, &who, &block, 0..3, "One.", None).unwrap();
        add(
            &mut c,
            &who,
            &block,
            4..12,
            "Two.\nSecond paragraph.",
            Some("CL"),
        )
        .unwrap();
        c.finish().unwrap();
    }
    let view = tx.parts();
    let t = Threads::of(&view).unwrap();
    assert_eq!(t.counts().comments, 2);
    let (a, b) = (t.comment(0).unwrap(), t.comment(1).unwrap());
    assert_eq!(b.initials.as_deref(), Some("CL"));
    assert!(a.para_id.is_some() && b.para_id.is_some() && a.para_id != b.para_id);
    assert!(a.durable_id.is_some() && b.durable_id.is_some() && a.durable_id != b.durable_id);
    assert!(
        audit(&view).unwrap().is_empty(),
        "{:?}",
        audit(&view).unwrap()
    );
    // The localised styles are used, the author is known once, and every part has two rows.
    let doc = text(&tx, "word/document.xml");
    assert!(
        doc.contains("<w:rStyle w:val=\"Rimandocommento\"/></w:rPr><w:commentReference"),
        "{doc}"
    );
    let comments = text(&tx, "word/comments.xml");
    assert!(
        comments.contains("<w:pStyle w:val=\"Testocommento\"/>"),
        "{comments}"
    );
    assert_eq!(comments.matches("<w:annotationRef/>").count(), 2);
    assert_eq!(
        text(&tx, "word/people.xml").matches("<w15:person ").count(),
        2
    );
    for (part, tag) in [
        ("word/commentsExtended.xml", "<w15:commentEx "),
        ("word/commentsIds.xml", "<w16cid:commentId "),
        ("word/commentsExtensible.xml", "<w16cex:commentExtensible "),
    ] {
        assert_eq!(text(&tx, part).matches(tag).count(), 2, "{part}");
    }
    assert!(!text(&tx, "word/commentsExtensible.xml").contains("dateUtc"));
}

#[test]
fn a_reply_goes_beside_the_roots_markers_and_names_it_as_parent() {
    let state = threaded().open();
    let mut tx = state.begin();
    let who = stamp_of("Clauseo");
    let fx = {
        let mut c = compiler(&mut tx, &who);
        // Comment 1 is already a reply: the new reply still names the root, 0.
        let fx = reply(&mut c, &who, 1, "Agreed, thanks.", None).unwrap();
        c.finish().unwrap();
        fx
    };
    assert_eq!((fx.action, fx.thread), (CommentAction::Replied, 0));
    assert_eq!(fx.warnings[0].code, "reply_to_resolved");
    let view = tx.parts();
    let t = Threads::of(&view).unwrap();
    let new = t.comment(fx.id).unwrap();
    assert_eq!(new.parent, Some(0));
    assert!(new.resolved, "as resolved as the root");
    assert_eq!(t.thread_of(fx.id).unwrap().replies, vec![1, fx.id]);
    assert_eq!(t.thread_of(fx.id).unwrap().root, 0);
    assert_eq!(new.anchor.blocks, t.comment(0).unwrap().anchor.blocks);
    assert_eq!(
        t.covered_text(&view, fx.id, Mode::Final, 100)
            .unwrap()
            .as_deref(),
        Some("target one")
    );
    assert!(
        audit(&view).unwrap().is_empty(),
        "{:?}",
        audit(&view).unwrap()
    );
    let doc = text(&tx, "word/document.xml");
    let at = |needle: &str| {
        doc.find(needle)
            .unwrap_or_else(|| panic!("{needle} in {doc}"))
    };
    // The reply sits right after the root's markers: start0 start3 start1 ... end0 ref0 end3 ref3 end1 ref1.
    assert!(at("commentRangeStart w:id=\"0\"") < at("commentRangeStart w:id=\"3\""));
    assert!(at("commentRangeStart w:id=\"3\"") < at("commentRangeStart w:id=\"1\""));
    assert!(at("commentReference w:id=\"0\"") < at("commentRangeEnd w:id=\"3\""));
    assert!(at("commentRangeEnd w:id=\"3\"") < at("commentReference w:id=\"3\""));
    assert!(at("commentReference w:id=\"3\"") < at("commentRangeEnd w:id=\"1\""));
}

#[test]
fn a_reply_to_a_comment_whose_reference_run_holds_more_goes_after_that_run() {
    // Word puts a `w:lastRenderedPageBreak` in the reference run; the reply's markers must not go into the run.
    let body = "<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:lastRenderedPageBreak/><w:commentReference w:id=\"0\"/></w:r></w:p>";
    let state = Pkg::new(body)
        .comments(&[comment(0, "Libre", &[None], "x")])
        .open();
    let mut tx = state.begin();
    let who = stamp();
    let fx = {
        let mut c = compiler(&mut tx, &who);
        let fx = reply(&mut c, &who, 0, "Reply.", None).unwrap();
        c.finish().unwrap();
        fx
    };
    let doc = text(&tx, "word/document.xml");
    let end = format!("<w:commentRangeEnd w:id=\"{}\"/>", fx.id);
    assert!(
        doc.contains(&format!("<w:commentReference w:id=\"0\"/></w:r>{end}<w:r>")),
        "{doc}"
    );
    let view = tx.parts();
    assert!(
        audit(&view).unwrap().is_empty(),
        "{:?}",
        audit(&view).unwrap()
    );
}

#[test]
fn a_reply_to_a_comment_with_no_paragraph_id_gives_it_one() {
    let body = format!("<w:p>{}</w:p>", ranged(0, "text"));
    let state = Pkg::new(&body)
        .comments(&[comment(0, "Libre", &[None], "x")])
        .open();
    let mut tx = state.begin();
    let who = stamp();
    let fx = {
        let mut c = compiler(&mut tx, &who);
        let fx = reply(&mut c, &who, 0, "Reply.", None).unwrap();
        c.finish().unwrap();
        fx
    };
    let view = tx.parts();
    let t = Threads::of(&view).unwrap();
    assert!(t.comment(0).unwrap().para_id.is_some());
    assert_eq!(t.comment(fx.id).unwrap().parent, Some(0));
    assert!(
        audit(&view).unwrap().is_empty(),
        "{:?}",
        audit(&view).unwrap()
    );
    assert!(text(&tx, "word/comments.xml").contains("mc:Ignorable=\"w14\""));
}

#[test]
fn what_is_refused_and_with_which_code() {
    let state = plain().open();
    let who = stamp();
    let block = para(&state, 0);
    let code = |r: Result<CommentEffect, crate::api::error::StetError>| r.unwrap_err().code();
    let mut tx = state.begin();
    let mut c = compiler(&mut tx, &who);
    assert_eq!(
        code(add(&mut c, &who, &block, 0..3, "  ", None)),
        ErrorCode::InvalidPayload
    );
    assert_eq!(
        code(add(&mut c, &who, &block, 0..999, "x", None)),
        ErrorCode::InvalidPayload
    );
    assert_eq!(code(reply(&mut c, &who, 5, "x", None)), ErrorCode::NotFound);
    // A table or a row is not a paragraph.
    let table = Pkg::new("<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>").open();
    let story_part = "word/document.xml";
    let view = table.parts();
    let story = view.story(view.id(story_part).unwrap()).unwrap();
    let tbl = story
        .blocks()
        .iter()
        .find(|e| e.kind == crate::wordml::BlockKind::Table)
        .map(|e| story.key(e))
        .unwrap();
    let mut tx2 = table.begin();
    let mut c2 = compiler(&mut tx2, &who);
    assert_eq!(
        code(add(&mut c2, &who, &tbl, 0..0, "x", None)),
        ErrorCode::Unsupported
    );
    // A reply needs an anchor, and a duplicated id is ambiguous.
    let unanchored = Pkg::new("<w:p/>")
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")])
        .open();
    let mut tx3 = unanchored.begin();
    let mut c3 = compiler(&mut tx3, &who);
    assert_eq!(
        code(reply(&mut c3, &who, 0, "x", None)),
        ErrorCode::Unsupported
    );
    let dup = Pkg::new(&format!("<w:p>{}</w:p>", ranged(1, "t")))
        .comments(&[
            comment(1, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .open();
    let mut tx4 = dup.begin();
    let mut c4 = compiler(&mut tx4, &who);
    assert_eq!(
        code(reply(&mut c4, &who, 1, "x", None)),
        ErrorCode::AmbiguousMatch
    );
}

#[test]
fn the_same_batch_on_the_same_state_writes_the_same_bytes_and_a_dry_run_changes_nothing() {
    let run = |pkg: &Pkg| {
        let state = pkg.open();
        let before = state.export().unwrap().head;
        let block = para(&state, 0);
        let mut tx = state.begin();
        let who = stamp();
        {
            let mut c = compiler(&mut tx, &who);
            add(&mut c, &who, &block, 4..9, "One.", None).unwrap();
            reply(&mut c, &who, 0, "Two.", None).unwrap();
        }
        let after: Vec<String> = [
            "word/document.xml",
            "word/comments.xml",
            "word/commentsExtended.xml",
        ]
        .iter()
        .map(|p| text(&tx, p))
        .collect();
        tx.discard();
        assert_eq!(
            state.export().unwrap().head,
            before,
            "a dry run changes nothing"
        );
        after
    };
    assert_eq!(run(&plain()), run(&plain()));
}
