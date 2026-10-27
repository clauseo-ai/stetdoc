//! `Document::inspect` of a block lists its tracked changes and comments, and says so when it has none, so that text which
//! only looks like a revision (`[DELETED: ...]`, or stetdoc's own `{++ ++}`) can be told from one. A view escapes the
//! braces of such text, so it never reads as a marker.

use super::*;
use crate::api::view::{Mode, Scope, ViewSpec};
use crate::index::testing::{Fixture, del, ins, p, px, r};

/// p1 holds a real insertion and deletion, p2 only literal look-alikes, and p3 a comment.
fn marked() -> Document {
    let body = format!(
        "{}{}{}",
        px(&format!(
            "{}{}{}",
            r("Pay "),
            del(7, "Eric White", "USD 500"),
            ins(8, "Jane Doe", "USD 900")
        )),
        p("[DELETED: old text] [ADDED: new text] {++ not inserted ++} {-- not deleted --}"),
        px(concat!(
            "<w:commentRangeStart w:id=\"3\"/>",
            "<w:r><w:t>Liability is capped.</w:t></w:r>",
            "<w:commentRangeEnd w:id=\"3\"/>",
            "<w:r><w:commentReference w:id=\"3\"/></w:r>"
        )),
    );
    let comments = concat!(
        "<w:comment w:id=\"3\" w:author=\"Ann Lee\" w:date=\"2026-02-01T09:00:00Z\">",
        "<w:p><w:r><w:t>Too low?</w:t></w:r></w:p></w:comment>"
    );
    let bytes = Fixture::new(&body).comments(comments).bytes();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

fn find<'a>(items: &'a [crate::api::inspect::InspectItem], label: &str) -> Option<&'a str> {
    items
        .iter()
        .find(|i| i.label == label)
        .map(|i| i.value.as_str())
}

#[test]
fn a_block_with_tracked_changes_lists_each_with_its_id_kind_and_author() {
    let doc = marked();
    let got = doc.inspect("p1").unwrap();
    let deleted = find(&got.items, "change #7").unwrap_or_else(|| panic!("{}", got.text));
    assert!(deleted.starts_with("delete by Eric White"), "{deleted}");
    assert!(deleted.contains("\"USD 500\""), "{deleted}");
    let inserted = find(&got.items, "change #8").unwrap_or_else(|| panic!("{}", got.text));
    assert!(inserted.starts_with("insert by Jane Doe"), "{inserted}");
    assert!(
        find(&got.items, "tracked changes").is_none(),
        "{}",
        got.text
    );
    assert_eq!(find(&got.items, "comments"), Some("none on this block"));
}

#[test]
fn a_block_of_literal_look_alikes_says_it_has_no_tracked_changes() {
    let doc = marked();
    let got = doc.inspect("p2").unwrap();
    let none = find(&got.items, "tracked changes").unwrap_or_else(|| panic!("{}", got.text));
    assert!(none.starts_with("none in this block"), "{none}");
    assert!(
        !got.items.iter().any(|i| i.label.starts_with("change ")),
        "{}",
        got.text
    );
    // The redline view escapes stetdoc's own marker syntax in the text, and shows the brackets as they are.
    let view = doc
        .view(&ViewSpec {
            scope: Scope::Range {
                from: "p2".into(),
                to: Some("p2".into()),
            },
            mode: Mode::Redline,
            budget: crate::api::view::Budget(0),
            ..ViewSpec::default()
        })
        .unwrap();
    assert!(
        view.text.contains("\\{++ not inserted ++\\}"),
        "{}",
        view.text
    );
    assert!(
        view.text.contains("\\{-- not deleted --\\}"),
        "{}",
        view.text
    );
    assert!(view.text.contains("[DELETED: old text]"), "{}", view.text);
}

#[test]
fn a_block_with_a_comment_lists_it_with_its_author() {
    let doc = marked();
    let got = doc.inspect("p3").unwrap();
    assert_eq!(
        find(&got.items, "comment c3"),
        Some("by Ann Lee"),
        "{}",
        got.text
    );
    assert!(got.text.contains("comment c3: by Ann Lee"), "{}", got.text);
}
