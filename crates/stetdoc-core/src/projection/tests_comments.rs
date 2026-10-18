//! The `Comments` scope, comment threads in the comments story, and the `Final` text of the JSON view.

use super::tests_view::spec;
use super::view::view;
use crate::api::error::ErrorCode;
use crate::api::view::{CommentFilter, Cursor, FormatId, Scope, StoryName, View, ViewSpec};
use crate::state::State;
use crate::threads::tests_support::threaded;

fn threads(state: &State, filter: CommentFilter, budget: u32) -> View {
    view(state, &spec(Scope::Comments(filter), budget)).unwrap()
}

/// The lines of a page between its head and its position line.
fn body_lines(v: &View) -> Vec<&str> {
    v.text
        .lines()
        .filter(|l| {
            !l.starts_with("{stet ")
                && !l.starts_with("{authors:")
                && !l.starts_with("{summary:")
                && !l.starts_with("{position:")
        })
        .collect()
}

#[test]
fn the_comments_scope_lists_each_thread_with_its_anchor_its_replies_and_whether_it_is_resolved() {
    let state = threaded().open();
    let v = threads(&state, CommentFilter::default(), 0);
    assert_eq!(
        body_lines(&v),
        vec![
            "{threads: 2 threads · 1 resolved}",
            "{thread c0 resolved · on p1 \"target one\"}",
            "  {c0 A 2026-01-01} parent",
            "  {c1 B 2026-01-02} reply",
            "{thread c2 open · on p1 \"target two\"}",
            "  {c2 A 2026-01-03} other",
        ],
        "{}",
        v.text
    );
    assert!(v.text.starts_with("{stet 1 view=threads "), "{}", v.text);
    assert!(
        v.text.contains("{authors: A \"Ann\" · B \"Bob\"}"),
        "{}",
        v.text
    );
    assert!(
        v.text.contains("{summary: 3 comments, 1 resolved"),
        "{}",
        v.text
    );
    assert_eq!(v.position.scope, "threads");
    assert_eq!(
        (v.position.first.as_deref(), v.position.last.as_deref()),
        (Some("c0"), Some("c2"))
    );
    assert_eq!((v.position.shown, v.position.total), (2, 2));
    assert!(v.next.is_none());
}

#[test]
fn the_filter_keeps_threads_by_whether_they_are_resolved_and_by_the_author_of_the_root() {
    let state = threaded().open();
    let roots = |filter: CommentFilter| -> Vec<String> {
        body_lines(&threads(&state, filter, 0))
            .into_iter()
            .filter_map(|l| l.strip_prefix("{thread "))
            .map(|l| l.split(' ').next().unwrap_or("").to_string())
            .collect()
    };
    let open = CommentFilter {
        resolved: Some(false),
        ..CommentFilter::default()
    };
    assert_eq!(roots(open), vec!["c2"]);
    let resolved = CommentFilter {
        resolved: Some(true),
        ..CommentFilter::default()
    };
    assert_eq!(roots(resolved), vec!["c0"]);
    let by = |author: &str| CommentFilter {
        author: Some(author.to_string()),
        ..CommentFilter::default()
    };
    assert_eq!(roots(by("ann")), vec!["c0", "c2"], "a name, in any case");
    assert_eq!(roots(by("A")), vec!["c0", "c2"], "a code");
    assert!(
        roots(by("Bob")).is_empty(),
        "Bob only replies, and a thread is the root's"
    );
    let none = threads(&state, by("Nobody"), 0);
    assert!(none.text.contains("{threads: 0 threads}"), "{}", none.text);
}

#[test]
fn threads_page_by_the_root_of_the_last_thread_shown_and_a_bad_cursor_is_refused() {
    let state = threaded().open();
    let first = threads(&state, CommentFilter::default(), 1);
    assert_eq!(first.position.shown, 1, "{}", first.text);
    assert_eq!(first.next, Some(Cursor::new("c0")));
    for cursor in ["c0", "comment:0", "c1"] {
        let next = view(
            &state,
            &ViewSpec {
                after: Some(Cursor::new(cursor)),
                ..spec(Scope::Comments(CommentFilter::default()), 1)
            },
        )
        .unwrap();
        assert_eq!(
            next.position.first.as_deref(),
            Some("c2"),
            "{cursor}: a reply continues after its thread"
        );
        assert!(next.next.is_none());
    }
    for (cursor, code) in [
        ("p1", ErrorCode::InvalidPayload),
        ("c99", ErrorCode::NotFound),
    ] {
        let e = view(
            &state,
            &ViewSpec {
                after: Some(Cursor::new(cursor)),
                ..spec(Scope::Comments(CommentFilter::default()), 0)
            },
        )
        .unwrap_err();
        assert_eq!(e.code(), code, "{cursor}: {e}");
    }
}

#[test]
fn the_comments_story_says_which_comment_a_reply_answers_and_that_its_thread_is_resolved() {
    let state = threaded().open();
    let v = view(&state, &spec(Scope::Story(StoryName::Comments), 0)).unwrap();
    for line in [
        "{comment c0 by A at \"2026-01-01T10:00:00Z\" on p1 resolved}",
        "{reply c1 to c0 by B at \"2026-01-02T10:00:00Z\" on p1 resolved}",
        "{comment c2 by A at \"2026-01-03T10:00:00Z\" on p1}",
    ] {
        assert!(v.text.lines().any(|l| l == line), "{line}\n{}", v.text);
    }
}

#[test]
fn a_json_view_of_the_threads_has_them_and_a_json_view_of_text_has_each_paragraphs_final_text() {
    let state = threaded().open();
    let json = |scope: Scope| {
        view(
            &state,
            &ViewSpec {
                format: FormatId::new(FormatId::JSON),
                ..spec(scope, 0)
            },
        )
        .unwrap()
        .text
    };
    let t = json(Scope::Comments(CommentFilter::default()));
    assert!(
        t.contains(
            "\"threads\":{\"total\":2,\"resolved\":1,\"threads\":[{\"root\":0,\"resolved\":true"
        ),
        "{t}"
    );
    let body = json(Scope::Story(StoryName::Body));
    // The comment marks are not text, so the inlines alone would not say where offsets count.
    assert!(body.contains("\"final_text\":\"target one.\""), "{body}");
    assert!(body.contains("\"final_text\":\"target two\""), "{body}");
    let page = view(&state, &spec(Scope::Story(StoryName::Body), 0)).unwrap();
    assert!(
        !page.text.contains("final_text"),
        "the text format has no use for it"
    );
}
