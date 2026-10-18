//! The cleanup, after the operations that take reference runs away.

use super::add::add;
use super::cleanup::{Reconciled, reconcile};
use super::tests_support::*;
use super::*;
use crate::revisions::compiler::Compiler;
use crate::revisions::index::ChangeIndex;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::Track;
use crate::revisions::types::{Action, Selector, Step};
use crate::state::Tx;
use crate::threads::tests_support::*;
use crate::threads::{Threads, audit};

fn two_paragraphs() -> Pkg {
    Pkg::new(&format!(
        "<w:p>{}</w:p><w:p>{}</w:p>",
        ranged(0, "first"),
        ranged(1, "second")
    ))
    .comments(&[
        comment(0, "Ann", &[Some("0000000A")], "x"),
        comment(1, "Bob", &[Some("0000000B")], "y"),
    ])
    .extended(&[ex("0000000A", None, false), ex("0000000B", None, false)])
    .ids(&[
        id_row("0000000A", "00000001"),
        id_row("0000000B", "00000002"),
    ])
}

fn untracked<'a, 's>(tx: &'a mut Tx<'s>) -> Compiler<'a, 's> {
    Compiler::new(tx, Track::Untracked).unwrap()
}

fn ids(tx: &Tx<'_>) -> Vec<i64> {
    Threads::of(&tx.parts())
        .unwrap()
        .comments()
        .iter()
        .map(|c| c.id)
        .collect()
}

/// Takes the run that holds the reference of comment `id` out of the body, the way a removal that hoists
/// nothing would: the compiler hoists references, so this is how a comment loses its own.
fn strip_reference(tx: &mut Tx<'_>, id: i64) {
    let run = {
        let view = tx.parts();
        let tree = view.tree(view.id("word/document.xml").unwrap()).unwrap();
        let mut stack = vec![tree.root()];
        let mut found = None;
        while let Some(n) = stack.pop() {
            if n.local_name() == "commentReference"
                && n.attr(n.namespace(), "id")
                    .is_some_and(|v| v == id.to_string())
            {
                found = n.parent().map(|p| p.id());
            }
            stack.extend(n.child_elements());
        }
        found.unwrap()
    };
    let tree = tx.part_mut("word/document.xml").unwrap();
    tree.edit().detach(run).unwrap();
}

#[test]
fn the_compiler_hoists_a_reference_so_a_comment_survives_the_deletion_of_its_paragraph() {
    // revisions.md 3.6.5: a range marker and a reference are hoisted and never deleted. So the comment stays,
    // at the place the text was, and the cleanup says so.
    let state = two_paragraphs().open();
    let mut tx = state.begin();
    {
        let mut c = untracked(&mut tx);
        c.delete_block(&para(&state, 0)).unwrap();
        let done = reconcile(&mut c).unwrap();
        assert!(done.removed.is_empty());
        assert_eq!(done.warnings.len(), 1);
        assert_eq!(done.warnings[0].code, "anchor_emptied");
        assert!(done.warnings[0].message.contains("c0"));
        c.finish().unwrap();
    }
    assert_eq!(ids(&tx), vec![0, 1]);
    assert!(audit(&tx.parts()).unwrap().is_empty());
}

#[test]
fn a_rejected_insertion_and_a_deleted_range_across_paragraphs_leave_their_comments_too() {
    let state = threaded().open();
    let mut tx = state.begin();
    {
        let mut c = untracked(&mut tx);
        c.delete_block(&para(&state, 1)).unwrap();
        let done = reconcile(&mut c).unwrap();
        assert!(done.removed.is_empty());
        c.finish().unwrap();
    }
    assert_eq!(ids(&tx), vec![0, 1, 2]);
    assert!(
        audit(&tx.parts()).unwrap().is_empty(),
        "{:?}",
        audit(&tx.parts()).unwrap()
    );
}

#[test]
fn rejecting_an_insertion_that_holds_a_comment_keeps_the_comment_with_an_empty_range() {
    let state = Pkg::new("<w:p><w:r><w:t>Base text.</w:t></w:r></w:p>")
        .comments(&[])
        .extended(&[])
        .open();
    let author = stamp_of("Counsel");
    let mut tx = state.begin();
    {
        let mut c = compiler(&mut tx, &author);
        c.replace_text(&para(&state, 0), 4..5, " inserted ")
            .unwrap();
        c.finish().unwrap();
    }
    let (middle, _) = tx.commit().unwrap();
    let mut tx = middle.begin();
    let agent = stamp();
    {
        let mut c = compiler(&mut tx, &agent);
        add(&mut c, &agent, &para(&middle, 0), 5..13, "Why?", None).unwrap();
        c.finish().unwrap();
    }
    let (commented, _) = tx.commit().unwrap();
    let mut tx = commented.begin();
    let mut index = ChangeIndex::new();
    resolve(
        &mut tx,
        &mut index,
        &[Step {
            action: Action::Reject,
            selector: Selector::All,
        }],
    )
    .unwrap();
    let mut c = compiler(&mut tx, &agent);
    let done = reconcile(&mut c).unwrap();
    assert!(done.removed.is_empty());
    assert_eq!(done.warnings[0].code, "anchor_emptied");
    c.finish().unwrap();
    assert_eq!(ids(&tx), vec![1]);
    assert!(audit(&tx.parts()).unwrap().is_empty());
}

#[test]
fn a_whole_paragraph_replace_leaves_the_comment_on_it() {
    let state = two_paragraphs().open();
    let mut tx = state.begin();
    {
        let mut c = untracked(&mut tx);
        c.replace_text(&para(&state, 0), 0..5, "changed text")
            .unwrap();
        let done = reconcile(&mut c).unwrap();
        assert!(done.removed.is_empty() && done.warnings.is_empty());
        c.finish().unwrap();
    }
    assert_eq!(ids(&tx), vec![0, 1]);
    assert!(
        Threads::of(&tx.parts())
            .unwrap()
            .comment(0)
            .unwrap()
            .anchor
            .reference
            .is_some()
    );
    assert!(audit(&tx.parts()).unwrap().is_empty());
}

#[test]
fn a_comment_that_lost_its_reference_run_is_removed_with_its_own_markers_and_rows() {
    let state = two_paragraphs().open();
    let mut tx = state.begin();
    strip_reference(&mut tx, 0);
    let mut c = untracked(&mut tx);
    let done = reconcile(&mut c).unwrap();
    assert_eq!(done.removed.len(), 1);
    let fx = &done.removed[0];
    assert_eq!(
        (fx.action, fx.id, fx.author.as_str()),
        (CommentAction::Removed, 0, "Ann")
    );
    assert_eq!(fx.warnings[0].code, "removed_with_text");
    assert!(fx.parts.iter().any(|p| &**p == "word/comments.xml"));
    c.finish().unwrap();
    assert_eq!(ids(&tx), vec![1]);
    let doc = text(&tx, "word/document.xml");
    assert!(!doc.contains("w:id=\"0\""), "its markers went: {doc}");
    assert!(
        doc.contains("commentRangeStart w:id=\"1\"") && doc.contains("commentReference w:id=\"1\"")
    );
    for (part, tag) in [
        ("word/commentsExtended.xml", "<w15:commentEx "),
        ("word/commentsIds.xml", "<w16cid:commentId "),
    ] {
        assert_eq!(text(&tx, part).matches(tag).count(), 1, "{part}");
    }
    assert!(audit(&tx.parts()).unwrap().is_empty());
    let (next, _) = tx.commit().unwrap();
    assert_eq!(Threads::of(&next.parts()).unwrap().comments().len(), 1);
}

#[test]
fn a_comment_stranded_in_the_input_is_not_touched() {
    let state =
        Pkg::new("<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>")
            .comments(&[comment(5, "A", &[Some("0000000A")], "x")])
            .open();
    let mut tx = state.begin();
    let mut c = untracked(&mut tx);
    c.delete_block(&para(&state, 1)).unwrap();
    let done = reconcile(&mut c).unwrap();
    assert!(done.removed.is_empty() && done.warnings.is_empty());
    drop(c);
    assert_eq!(ids(&tx), vec![5]);
}

#[test]
fn a_comment_added_in_the_batch_is_examined_with_the_rest() {
    let state = two_paragraphs().open();
    let mut tx = state.begin();
    let who = stamp();
    let first = para(&state, 0);
    {
        let mut c = compiler(&mut tx, &who);
        let added = add(&mut c, &who, &first, 0..3, "New.", None).unwrap();
        assert_eq!(added.id, 2);
    }
    strip_reference(&mut tx, 2);
    let mut c = untracked(&mut tx);
    let done = reconcile(&mut c).unwrap();
    assert_eq!(
        done.removed.iter().map(|f| f.id).collect::<Vec<_>>(),
        vec![2]
    );
    drop(c);
    assert_eq!(ids(&tx), vec![0, 1]);
}

#[test]
fn a_reply_whose_root_goes_becomes_a_root() {
    let body = format!("<w:p>{}</w:p><w:p>{}</w:p>", ranged(0, "a"), ranged(1, "b"));
    let state = Pkg::new(&body)
        .comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .extended(&[
            ex("0000000A", None, false),
            ex("0000000B", Some("0000000A"), false),
        ])
        .open();
    let mut tx = state.begin();
    strip_reference(&mut tx, 0);
    let mut c = untracked(&mut tx);
    reconcile(&mut c).unwrap();
    drop(c);
    let t = Threads::of(&tx.parts()).unwrap();
    assert_eq!(t.comments().len(), 1);
    assert_eq!(t.comment(1).unwrap().parent, None);
    assert_eq!(t.threads().len(), 1);
    assert!(
        audit(&tx.parts()).unwrap().is_empty(),
        "{:?}",
        audit(&tx.parts()).unwrap()
    );
}

#[test]
fn a_comment_whose_text_was_deleted_in_a_tracked_change_stays_and_says_so() {
    let state = two_paragraphs().open();
    let mut tx = state.begin();
    let who = stamp_of("Counsel");
    let mut c = compiler(&mut tx, &who);
    c.replace_text(&para(&state, 0), 0..5, "").unwrap();
    let done = reconcile(&mut c).unwrap();
    assert!(done.removed.is_empty());
    assert_eq!(done.warnings.len(), 1);
    assert_eq!(done.warnings[0].code, "anchor_emptied");
    c.finish().unwrap();
    assert_eq!(ids(&tx), vec![0, 1]);
}

#[test]
fn nothing_to_do_in_a_document_with_no_comment() {
    let state = Pkg::new("<w:p><w:r><w:t>x</w:t></w:r></w:p>").open();
    let mut tx = state.begin();
    let mut c = untracked(&mut tx);
    assert_eq!(reconcile(&mut c).unwrap(), Reconciled::default());
}
