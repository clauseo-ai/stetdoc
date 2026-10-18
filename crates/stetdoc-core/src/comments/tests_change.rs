//! Resolving, editing and deleting.

use super::add::{add, reply};
use super::delete::delete;
use super::edit::edit;
use super::resolve::set_resolved;
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

fn clean(tx: &crate::state::Tx<'_>) {
    let found = audit(&tx.parts()).unwrap();
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn resolving_writes_done_on_every_comment_of_the_thread_and_reopening_clears_it() {
    let state = threaded().open();
    let mut tx = state.begin();
    let who = stamp();
    let mut c = compiler(&mut tx, &who);
    // Comment 2 is a thread of one, open.
    let fx = set_resolved(&mut c, 2, true).unwrap();
    assert_eq!((fx.action, fx.thread), (CommentAction::Resolved, 2));
    assert_eq!(fx.parts.len(), 1);
    assert_eq!(&*fx.parts[0], "word/commentsExtended.xml");
    assert!(
        !fx.blocks.is_empty(),
        "the scope guard reads the anchor blocks"
    );
    // Thread 0 is resolved, with a reply: reopen it.
    let fx = set_resolved(&mut c, 1, false).unwrap();
    assert_eq!(
        (fx.action, fx.id, fx.thread),
        (CommentAction::Reopened, 1, 0)
    );
    c.finish().unwrap();
    let t = Threads::of(&tx.parts()).unwrap();
    assert!(t.comment(2).unwrap().resolved);
    assert!(t.threads().iter().find(|t| t.root == 2).unwrap().resolved);
    assert!(!t.comment(0).unwrap().resolved && !t.comment(1).unwrap().resolved);
    clean(&tx);
}

#[test]
fn resolving_twice_is_resolving_once_and_says_so() {
    let state = threaded().open();
    let mut tx = state.begin();
    let who = stamp();
    let mut c = compiler(&mut tx, &who);
    let fx = set_resolved(&mut c, 0, true).unwrap();
    assert_eq!(fx.warnings[0].code, "already_resolved");
    assert!(fx.parts.is_empty());
    let fx = set_resolved(&mut c, 2, false).unwrap();
    assert_eq!(fx.warnings[0].code, "already_open");
    drop(c);
    let ex = text(&tx, "word/commentsExtended.xml");
    let first = {
        let mut c = compiler(&mut tx, &who);
        set_resolved(&mut c, 2, true).unwrap();
        text(c.tx(), "word/commentsExtended.xml")
    };
    assert_ne!(ex, first);
    let second = {
        let mut c = compiler(&mut tx, &who);
        let fx = set_resolved(&mut c, 2, true).unwrap();
        assert_eq!(fx.warnings[0].code, "already_resolved");
        text(c.tx(), "word/commentsExtended.xml")
    };
    assert_eq!(first, second);
}

#[test]
fn resolving_a_comment_with_no_extension_part_makes_one_and_gives_the_comment_a_paragraph_id() {
    let state = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "text")))
        .comments(&[comment(0, "Libre", &[None], "x")])
        .open();
    let mut tx = state.begin();
    let who = stamp();
    let mut c = compiler(&mut tx, &who);
    set_resolved(&mut c, 0, true).unwrap();
    c.finish().unwrap();
    let t = Threads::of(&tx.parts()).unwrap();
    assert!(t.comment(0).unwrap().resolved);
    assert!(t.comment(0).unwrap().para_id.is_some());
    clean(&tx);
}

#[test]
fn a_thread_whose_rows_disagree_is_made_to_agree() {
    let body = format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b"));
    let state = Pkg::new(&body)
        .comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .extended(&[
            ex("0000000A", None, true),
            ex("0000000B", Some("0000000A"), false),
        ])
        .open();
    let mut tx = state.begin();
    let who = stamp();
    let mut c = compiler(&mut tx, &who);
    set_resolved(&mut c, 0, true).unwrap();
    let t = Threads::of(&c.tx().parts()).unwrap();
    assert!(t.comment(0).unwrap().resolved && t.comment(1).unwrap().resolved);
}

#[test]
fn editing_rewrites_the_text_and_keeps_the_id_the_date_and_the_rows() {
    let state = word().open();
    let mut tx = state.begin();
    let block = para(&state, 0);
    let who = stamp();
    let mut c = compiler(&mut tx, &who);
    add(&mut c, &who, &block, 4..12, "First draft.", None).unwrap();
    let before = {
        let t = Threads::of(&c.tx().parts()).unwrap();
        let c0 = t.comment(0).unwrap();
        (c0.date.clone(), c0.para_id, c0.durable_id)
    };
    let later = stamp_of("Clauseo");
    let fx = edit(&mut c, &later, 0, "Second draft.\nWith a second paragraph.").unwrap();
    assert_eq!((fx.action, fx.id), (CommentAction::Edited, 0));
    c.finish().unwrap();
    let t = Threads::of(&tx.parts()).unwrap();
    let c0 = t.comment(0).unwrap();
    assert_eq!((c0.date.clone(), c0.para_id, c0.durable_id), before);
    let xml = text(&tx, "word/comments.xml");
    assert!(xml.contains("Second draft.") && xml.contains("With a second paragraph."));
    assert!(!xml.contains("First draft."));
    assert_eq!(xml.matches("<w:annotationRef/>").count(), 1);
    assert!(
        xml.contains("<w:pStyle w:val=\"Testocommento\"/>"),
        "the comment's own styles are kept"
    );
    clean(&tx);
}

#[test]
fn only_the_author_may_edit_or_delete() {
    let state = threaded().open();
    let mut tx = state.begin();
    let outsider = stamp();
    let mut c = compiler(&mut tx, &outsider);
    assert_eq!(
        edit(&mut c, &outsider, 0, "Mine now.").unwrap_err().code(),
        ErrorCode::ReadonlyRegion
    );
    assert_eq!(
        delete(&mut c, &outsider, 0).unwrap_err().code(),
        ErrorCode::ReadonlyRegion
    );
    // Ann's comment 0 has Bob's reply, so Ann cannot delete the thread either.
    let ann = stamp_of("Ann");
    let e = delete(&mut c, &ann, 0).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("Bob"), "{}", e.message());
    drop(c);
    let before = text(&tx, "word/comments.xml");
    assert_eq!(
        before,
        text(&state.begin(), "word/comments.xml"),
        "a refusal changed nothing"
    );
}

#[test]
fn deleting_a_comment_takes_its_markers_its_rows_and_its_replies() {
    let state = word().open();
    let mut tx = state.begin();
    let block = para(&state, 0);
    let who = stamp();
    let original_doc = text(&tx, "word/document.xml");
    let mut c = compiler(&mut tx, &who);
    add(&mut c, &who, &block, 4..12, "One.", None).unwrap();
    reply(&mut c, &who, 0, "Two.", None).unwrap();
    add(&mut c, &who, &block, 0..3, "Three.", None).unwrap();
    let fx = delete(&mut c, &who, 0).unwrap();
    assert_eq!(fx.iter().map(|f| f.id).collect::<Vec<_>>(), vec![0, 1]);
    assert!(
        fx.iter()
            .all(|f| f.action == CommentAction::Deleted && f.thread == 0)
    );
    c.finish().unwrap();
    let t = Threads::of(&tx.parts()).unwrap();
    assert_eq!(t.comments().len(), 1);
    assert_eq!(t.comment(2).unwrap().author.as_deref(), Some("Clauseo"));
    for (part, tag) in [
        ("word/commentsExtended.xml", "<w15:commentEx "),
        ("word/commentsIds.xml", "<w16cid:commentId "),
        ("word/commentsExtensible.xml", "<w16cex:commentExtensible "),
    ] {
        assert_eq!(text(&tx, part).matches(tag).count(), 1, "{part}");
    }
    let doc = text(&tx, "word/document.xml");
    assert!(
        !doc.contains("w:id=\"0\"") && !doc.contains("w:id=\"1\""),
        "{doc}"
    );
    assert_eq!(
        text(&tx, "word/people.xml").matches("<w15:person ").count(),
        2,
        "people stay"
    );
    clean(&tx);
    // Delete the last one: text and comments are as they were, but for what C7 allows.
    let mut c = compiler(&mut tx, &who);
    delete(&mut c, &who, 2).unwrap();
    let t = Threads::of(&tx.parts()).unwrap();
    assert!(t.comments().is_empty());
    let view = tx.parts();
    let id = view.id("word/document.xml").unwrap();
    let (story, tree) = (view.story(id).unwrap(), view.tree(id).unwrap());
    for (i, want) in ["The Purchase Price is payable.", "Second."]
        .iter()
        .enumerate()
    {
        assert_eq!(story.paragraph(tree, i).unwrap().text(Mode::Final), *want);
    }
    assert!(!text(&tx, "word/document.xml").contains("commentRange"));
    assert!(original_doc.contains("Second."));
    clean(&tx);
}

#[test]
fn a_comment_that_spans_paragraphs_is_deleted_from_both() {
    let state = threaded().open();
    let mut tx = state.begin();
    let ann = stamp_of("Ann");
    let mut c = compiler(&mut tx, &ann);
    delete(&mut c, &ann, 2).unwrap();
    c.finish().unwrap();
    let doc = text(&tx, "word/document.xml");
    assert!(!doc.contains("w:id=\"2\""), "{doc}");
    assert!(doc.contains("w:id=\"0\"") && doc.contains("w:id=\"1\""));
    clean(&tx);
}

#[test]
fn deleting_a_reply_leaves_the_root_and_deleting_what_does_not_exist_is_not_found() {
    let state = threaded().open();
    let mut tx = state.begin();
    let bob = stamp_of("Bob");
    let mut c = compiler(&mut tx, &bob);
    let fx = delete(&mut c, &bob, 1).unwrap();
    assert_eq!(fx.len(), 1);
    assert_eq!(
        delete(&mut c, &bob, 7).unwrap_err().code(),
        ErrorCode::NotFound
    );
    c.finish().unwrap();
    let t = Threads::of(&tx.parts()).unwrap();
    assert_eq!(t.thread_of(0).unwrap().replies.len(), 0);
    clean(&tx);
}
