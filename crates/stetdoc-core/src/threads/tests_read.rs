//! The reader against small packages written the way Word writes comments.

use super::tests_support::*;
use super::*;
use crate::api::view::Mode;
use std::sync::Arc;

fn read(pkg: &Pkg) -> Arc<Threads> {
    let state = pkg.open();
    Threads::of(&state.parts()).unwrap()
}

#[test]
fn a_reply_joins_its_parents_thread_and_the_root_decides_resolved() {
    let t = read(&threaded());
    assert_eq!(
        t.counts(),
        Counts {
            comments: 3,
            threads: 2,
            resolved_threads: 1,
            unanchored: 0
        }
    );
    assert_eq!(
        t.threads()[0],
        Thread {
            root: 0,
            replies: vec![1],
            resolved: true
        }
    );
    assert_eq!(
        t.threads()[1],
        Thread {
            root: 2,
            replies: vec![],
            resolved: false
        }
    );
    assert_eq!(t.comment(1).unwrap().parent, Some(0));
    assert_eq!(t.thread_of(1).unwrap().root, 0);
    assert_eq!(t.thread_of(2).unwrap().root, 2);
    assert!(t.thread_of(9).is_none());
    let c = t.comment(0).unwrap();
    assert_eq!(
        (c.author.as_deref(), c.initials.as_deref()),
        (Some("Ann"), Some("X"))
    );
    assert_eq!(c.para_id, Some(0x317E22E5));
    assert_eq!(c.durable_id, Some(0x385D7929));
    assert_eq!(t.comment(1).unwrap().durable_id, Some(0x0430_989E));
    assert!(c.resolved && !t.comment(2).unwrap().resolved);
}

#[test]
fn a_range_that_spans_paragraphs_lists_both_blocks_and_covers_the_text_between() {
    let state = threaded().open();
    let view = state.parts();
    let t = Threads::of(&view).unwrap();
    assert_eq!(t.comment(0).unwrap().anchor.blocks.len(), 1);
    assert_eq!(t.comment(2).unwrap().anchor.blocks.len(), 2);
    let text = |id| t.covered_text(&view, id, Mode::Final, 100).unwrap();
    assert_eq!(text(0).as_deref(), Some("target one"));
    assert_eq!(text(1).as_deref(), Some("target one"));
    assert_eq!(text(2).as_deref(), Some("\ntarget two"));
    assert_eq!(
        t.covered_text(&view, 0, Mode::Final, 6).unwrap().as_deref(),
        Some("target")
    );
    assert_eq!(t.covered_text(&view, 9, Mode::Final, 6).unwrap(), None);
}

#[test]
fn a_point_comment_covers_nothing_and_still_has_a_block() {
    let body = "<w:p><w:r><w:t>a</w:t></w:r><w:commentRangeStart w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r><w:r><w:t>b</w:t></w:r></w:p>";
    let pkg = Pkg::new(body).comments(&[comment(0, "Ann", &[Some("11111111")], "c")]);
    let state = pkg.open();
    let view = state.parts();
    let t = Threads::of(&view).unwrap();
    assert_eq!(
        t.covered_text(&view, 0, Mode::Final, 10)
            .unwrap()
            .as_deref(),
        Some("")
    );
    assert_eq!(t.comment(0).unwrap().anchor.blocks.len(), 1);
}

#[test]
fn a_chain_of_replies_is_one_thread_whose_root_is_the_top() {
    let pkg = Pkg::new(&format!(
        "<w:p>{}{}{}</w:p>",
        ranged(10, "a"),
        ranged(11, "b"),
        ranged(12, "c")
    ))
    .comments(&[
        comment(10, "A", &[Some("0000000A")], "x"),
        comment(11, "B", &[Some("0000000B")], "x"),
        comment(12, "C", &[Some("0000000C")], "x"),
    ])
    .extended(&[
        ex("0000000A", None, false),
        ex("0000000B", Some("0000000A"), false),
        ex("0000000C", Some("0000000B"), false),
    ]);
    let t = read(&pkg);
    assert_eq!(t.threads().len(), 1);
    assert_eq!(t.threads()[0].replies, vec![11, 12]);
    assert_eq!(t.thread_of(12).unwrap().root, 10);
    assert_eq!(t.comment(12).unwrap().parent, Some(11));
}

#[test]
fn comments_with_no_paragraph_ids_and_no_extension_parts_are_each_a_thread() {
    // What LibreOffice and the libraries write: no paraId, no rows.
    let pkg = Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b")))
        .comments(&[comment(0, "A", &[None], "x"), comment(1, "B", &[None], "y")]);
    let t = read(&pkg);
    assert_eq!(t.counts().threads, 2);
    assert!(
        t.comments()
            .iter()
            .all(|c| c.para_id.is_none() && c.parent.is_none() && !c.resolved)
    );
}

#[test]
fn a_comment_is_keyed_by_the_last_of_its_paragraphs() {
    let pkg = Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b")))
        .comments(&[
            comment(0, "A", &[Some("0000000A"), Some("0000000B")], "x"),
            comment(1, "B", &[Some("0000000C")], "y"),
        ])
        .extended(&[
            ex("0000000B", None, true),
            ex("0000000C", Some("0000000B"), false),
        ]);
    let t = read(&pkg);
    assert_eq!(t.comment(0).unwrap().para_id, Some(0x0B));
    assert!(t.comment(0).unwrap().resolved);
    assert_eq!(t.comment(1).unwrap().parent, Some(0));
}

#[test]
fn a_loop_of_parents_is_cut_at_the_first_comment_and_a_missing_parent_is_ignored() {
    let pkg = Pkg::new(&format!(
        "<w:p>{}{}{}</w:p>",
        ranged(0, "a"),
        ranged(1, "b"),
        ranged(2, "c")
    ))
    .comments(&[
        comment(0, "A", &[Some("0000000A")], "x"),
        comment(1, "B", &[Some("0000000B")], "x"),
        comment(2, "C", &[Some("0000000C")], "x"),
    ])
    .extended(&[
        ex("0000000A", Some("0000000B"), false),
        ex("0000000B", Some("0000000A"), false),
        ex("0000000C", Some("0000FFFF"), false),
    ]);
    let t = read(&pkg);
    assert_eq!(t.comment(0).unwrap().parent, None);
    assert_eq!(t.comment(1).unwrap().parent, Some(0));
    assert_eq!(t.comment(2).unwrap().parent, None);
    assert_eq!(
        t.threads().iter().map(|t| t.root).collect::<Vec<_>>(),
        vec![0, 2]
    );
}

#[test]
fn a_comment_with_no_marker_is_unanchored_and_kept() {
    let pkg = Pkg::new("<w:p><w:r><w:t>text</w:t></w:r></w:p>").comments(&[comment(
        5,
        "A",
        &[Some("0000000A")],
        "x",
    )]);
    let t = read(&pkg);
    assert_eq!(t.counts().unanchored, 1);
    assert!(t.comment(5).unwrap().anchor.is_unanchored());
    assert!(t.comment(5).unwrap().anchor.blocks.is_empty());
}

#[test]
fn a_document_with_no_comments_part_has_no_comments_and_the_answer_is_cached() {
    let pkg = Pkg::new("<w:p><w:r><w:t>text</w:t></w:r></w:p>");
    let state = pkg.open();
    let view = state.parts();
    let a = Threads::of(&view).unwrap();
    assert_eq!(a.counts(), Counts::default());
    assert!(Arc::ptr_eq(&a, &Threads::of(&view).unwrap()));
}

#[test]
fn rows_in_the_namespace_of_word_2013s_preview_are_read() {
    // `Comment042` and `Invalid_Word15Comments` of the corpus were written with it.
    let pkg = Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b")))
        .comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .extended_preview(&[
            ex("0000000A", None, true),
            ex("0000000B", Some("0000000A"), false),
        ]);
    let t = read(&pkg);
    assert_eq!(t.threads().len(), 1);
    assert!(t.threads()[0].resolved);
    assert_eq!(t.comment(1).unwrap().parent, Some(0));
}

/// Runs `f` on another thread and fails if it does not finish in 30 seconds: a walk that never ends is
/// a hang, and a test that hangs says nothing.
fn finishes<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(30))
        .unwrap_or_else(|_| panic!("the call did not finish in 30 seconds"))
}

#[test]
fn two_comments_with_one_id_and_a_parent_between_them_do_not_hang_the_thread_lookup() {
    // review_f's probe: the first comment names the second as parent, and both are `w:id` 1.
    let (root, replies, parent) = finishes(|| {
        let pkg = Pkg::new(&format!("<w:p>{}</w:p>", ranged(1, "a")))
            .comments(&[
                comment(1, "A", &[Some("0000000A")], "x"),
                comment(1, "B", &[Some("0000000B")], "y"),
            ])
            .extended(&[
                ex("0000000A", Some("0000000B"), false),
                ex("0000000B", None, false),
            ]);
        let t = read(&pkg);
        let thread = t.thread_of(1).unwrap();
        (
            thread.root,
            thread.replies.len(),
            t.comment(1).unwrap().parent,
        )
    });
    assert_eq!((root, replies), (1, 1));
    // The parent's id is two comments', so it is not given: no climb by id can loop or go astray.
    assert_eq!(parent, None);
}

#[test]
fn a_comment_that_names_itself_as_parent_is_a_root_and_a_loop_of_one() {
    let pkg = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "a")))
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")])
        .extended(&[ex("0000000A", Some("0000000A"), false)]);
    let t = read(&pkg);
    assert_eq!(t.comment(0).unwrap().parent, None);
    assert_eq!(t.threads().len(), 1);
    assert_eq!(t.loops.len(), 1);
}

#[test]
fn a_long_chain_of_replies_is_read_at_once() {
    let n = 20_000;
    let (threads, replies, found) = finishes(move || {
        let para = |i: usize| format!("{:08X}", i + 1);
        let comments: Vec<String> = (0..n)
            .map(|i| comment(i as i64, "A", &[Some(&para(i))], "x"))
            .collect();
        let rows: Vec<String> = (0..n)
            .map(|i| ex(&para(i), (i > 0).then(|| para(i - 1)).as_deref(), false))
            .collect();
        let pkg = Pkg::new("<w:p/>").comments(&comments).extended(&rows);
        let state = pkg.open();
        let view = state.parts();
        let t = Threads::of(&view).unwrap();
        let found = audit(&view).unwrap().len();
        (t.threads().len(), t.threads()[0].replies.len(), found)
    });
    assert_eq!((threads, replies), (1, n - 1));
    assert_eq!(found, n, "each comment is unanchored, and that is all");
}

#[test]
fn a_range_with_a_marker_between_blocks_still_lists_the_paragraphs_it_spans() {
    // Both markers are children of the body: 282 of the 1,993 range markers of the curated corpus are. The
    // paragraph that holds the reference run is listed too, since a writer at the markers writes there.
    let both = "<w:commentRangeStart w:id=\"0\"/><w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:commentRangeEnd w:id=\"0\"/><w:p><w:r><w:commentReference w:id=\"0\"/></w:r><w:r><w:t>three</w:t></w:r></w:p>";
    let pkg = Pkg::new(both).comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    assert_eq!(read(&pkg).comment(0).unwrap().anchor.blocks.len(), 3);
    // The start is in a paragraph and the end between blocks.
    let end = "<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:commentRangeEnd w:id=\"0\"/><w:p><w:r><w:commentReference w:id=\"0\"/></w:r><w:r><w:t>three</w:t></w:r></w:p>";
    let pkg = Pkg::new(end).comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    assert_eq!(read(&pkg).comment(0).unwrap().anchor.blocks.len(), 3);
    // The start is between blocks and the end in a paragraph.
    let start = "<w:commentRangeStart w:id=\"0\"/><w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p><w:p/>";
    let pkg = Pkg::new(start).comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    assert_eq!(read(&pkg).comment(0).unwrap().anchor.blocks.len(), 2);
}

#[test]
fn a_parent_with_a_duplicated_id_is_not_named_but_the_thread_is_still_exact() {
    let pkg = Pkg::new(&format!(
        "<w:p>{}{}{}</w:p>",
        ranged(1, "a"),
        ranged(2, "b"),
        ranged(3, "c")
    ))
    .comments(&[
        comment(1, "A", &[Some("0000000A")], "x"),
        comment(2, "B", &[Some("0000000B")], "y"),
        comment(2, "C", &[Some("0000000C")], "z"),
        comment(3, "D", &[Some("0000000D")], "w"),
    ])
    .extended(&[
        ex("0000000A", None, false),
        ex("0000000B", Some("0000000A"), false),
        ex("0000000C", Some("0000000A"), false),
        ex("0000000D", Some("0000000B"), false),
    ]);
    let t = read(&pkg);
    // Comment 3 names a comment with id 2, and two have it, so no parent id is given.
    assert_eq!(t.comment(3).unwrap().parent, None);
    // Both comments with id 2 name comment 1, whose id is unique.
    assert_eq!(t.comment(2).unwrap().parent, Some(1));
    // The thread holds all four, found by index.
    assert_eq!(t.threads().len(), 1);
    assert_eq!(t.threads()[0].replies, vec![2, 2, 3]);
    assert_eq!(t.thread_of(3).unwrap().root, 1);
}

#[test]
fn comments_with_a_marker_between_blocks_have_covered_text() {
    let text = |body: &str| {
        let pkg = Pkg::new(body).comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
        let state = pkg.open();
        let view = state.parts();
        let t = Threads::of(&view).unwrap();
        t.covered_text(&view, 0, Mode::Final, 100).unwrap()
    };
    let reference = "<w:r><w:commentReference w:id=\"0\"/></w:r>";
    let one = "<w:p><w:r><w:t>one</w:t></w:r></w:p>";
    let two = "<w:p><w:r><w:t>two</w:t></w:r></w:p>";
    let start = "<w:commentRangeStart w:id=\"0\"/>";
    let end = "<w:commentRangeEnd w:id=\"0\"/>";
    // Both markers between blocks: the blocks between them, whole.
    let both = format!("{start}{one}{two}{end}<w:p>{reference}<w:r><w:t>x</w:t></w:r></w:p>");
    assert_eq!(text(&both).as_deref(), Some("one\ntwo"));
    // The start in a paragraph, at its beginning, and the end between blocks.
    let in_start = format!(
        "<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>one</w:t></w:r></w:p>{two}{end}<w:p>{reference}</w:p>"
    );
    assert_eq!(text(&in_start).as_deref(), Some("one\ntwo"));
    // The start between blocks and the end in the middle of a paragraph.
    let in_end = format!(
        "{start}{one}<w:p><w:r><w:t>tw</w:t></w:r>{end}{reference}<w:r><w:t>o</w:t></w:r></w:p>"
    );
    assert_eq!(text(&in_end).as_deref(), Some("one\ntw"));
    // Both markers between the same two blocks: a point, which covers nothing.
    let point = format!("{one}{start}{end}<w:p>{reference}<w:r><w:t>b</w:t></w:r></w:p>");
    assert_eq!(text(&point).as_deref(), Some(""));
}
