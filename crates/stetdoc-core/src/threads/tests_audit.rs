//! The audit: one rule per test, and the clean case that has none.

use super::tests_support::*;
use super::*;
use crate::schema::PartKind;

fn codes(pkg: &Pkg) -> Vec<(&'static str, Class)> {
    let state = pkg.open();
    let found = audit(&state.parts()).unwrap();
    found.iter().map(|f| (f.code, f.class)).collect()
}

fn one(pkg: &Pkg, code: &'static str, class: Class) {
    assert_eq!(codes(pkg), vec![(code, class)], "{code}");
}

/// A body with comments 0 and 1, each ranged, and rows that agree.
fn pair(extra_rows: &[String]) -> Pkg {
    let mut rows = vec![ex("0000000A", None, false), ex("0000000B", None, false)];
    rows.extend_from_slice(extra_rows);
    Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b")))
        .comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .extended(&rows)
}

#[test]
fn a_clean_document_and_a_document_with_no_comments_have_no_findings() {
    assert!(codes(&threaded()).is_empty());
    assert!(codes(&pair(&[])).is_empty());
    assert!(codes(&Pkg::new("<w:p><w:r><w:t>text</w:t></w:r></w:p>")).is_empty());
}

#[test]
fn a_duplicated_comment_id_is_an_error_once() {
    let pkg = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "a"))).comments(&[
        comment(0, "A", &[Some("0000000A")], "x"),
        comment(0, "B", &[Some("0000000B")], "y"),
    ]);
    one(&pkg, "comment_id_duplicate", Class::Error);
}

#[test]
fn markers_that_do_not_pair_or_name_no_comment_are_errors() {
    let unpaired = Pkg::new("<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>a</w:t></w:r><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>")
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    one(&unpaired, "marker_unpaired", Class::Error);
    let nameless = Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(9, "b")))
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    one(&nameless, "marker_without_comment", Class::Error);
    let no_reference = Pkg::new("<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>a</w:t></w:r><w:commentRangeEnd w:id=\"0\"/></w:p>")
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    one(&no_reference, "reference_missing", Class::Error);
    let twice = Pkg::new(&format!(
        "<w:p>{}<w:r><w:commentReference w:id=\"0\"/></w:r></w:p>",
        ranged(0, "a")
    ))
    .comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    one(&twice, "reference_duplicate", Class::Error);
}

#[test]
fn a_comment_with_no_marker_is_a_note() {
    let pkg = Pkg::new("<w:p><w:r><w:t>a</w:t></w:r></w:p>").comments(&[comment(
        0,
        "A",
        &[Some("0000000A")],
        "x",
    )]);
    one(&pkg, "unanchored", Class::Note);
}

#[test]
fn rows_that_name_no_comment_are_errors_in_each_part() {
    one(
        &pair(&[ex("0000000F", None, false)]),
        "row_without_comment",
        Class::Error,
    );
    let ids = pair(&[]).ids(&[
        id_row("0000000A", "00000001"),
        id_row("0000000B", "00000002"),
        id_row("0000000F", "00000003"),
    ]);
    one(&ids, "row_without_comment", Class::Error);
    let ext = pair(&[])
        .ids(&[
            id_row("0000000A", "00000001"),
            id_row("0000000B", "00000002"),
        ])
        .extensible(&[
            ext_row("00000001"),
            ext_row("00000002"),
            ext_row("00000009"),
        ]);
    one(&ext, "row_without_comment", Class::Error);
}

#[test]
fn a_comment_with_no_row_in_a_part_that_exists_is_a_note() {
    let pkg = Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b")))
        .comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .extended(&[ex("0000000A", None, false)]);
    one(&pkg, "row_missing", Class::Note);
    let ids = pair(&[]).ids(&[id_row("0000000A", "00000001")]);
    one(&ids, "row_missing", Class::Note);
}

#[test]
fn paragraph_ids_that_are_missing_repeated_or_out_of_range_are_found() {
    let missing = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "a"))).comments(&[comment(
        0,
        "A",
        &[None],
        "x",
    )]);
    one(&missing, "para_id_missing", Class::Note);
    let repeated =
        Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b"))).comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000A")], "y"),
        ]);
    one(&repeated, "para_id_duplicate", Class::Error);
    let high = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "a"))).comments(&[comment(
        0,
        "A",
        &[Some("97CB30E2")],
        "x",
    )]);
    one(&high, "para_id_range", Class::Note);
    let shared = Pkg::new(&format!(
        "<w:p xmlns:w14=\"{W14}\" w14:paraId=\"0000000A\">{}</w:p>",
        ranged(0, "a")
    ))
    .comments(&[comment(0, "A", &[Some("0000000A")], "x")]);
    one(&shared, "para_id_duplicate", Class::Error);
}

#[test]
fn a_parent_that_is_missing_is_a_note_and_a_loop_is_an_error() {
    let missing = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "a")))
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")])
        .extended(&[ex("0000000A", Some("0000FFFF"), false)]);
    one(&missing, "parent_missing", Class::Note);
    let looped = Pkg::new(&format!("<w:p>{}{}</w:p>", ranged(0, "a"), ranged(1, "b")))
        .comments(&[
            comment(0, "A", &[Some("0000000A")], "x"),
            comment(1, "B", &[Some("0000000B")], "y"),
        ])
        .extended(&[
            ex("0000000A", Some("0000000B"), false),
            ex("0000000B", Some("0000000A"), false),
        ]);
    one(&looped, "thread_cycle", Class::Error);
}

#[test]
fn a_durable_id_used_by_two_rows_of_a_part_is_an_error() {
    let pkg = pair(&[]).ids(&[
        id_row("0000000A", "00000001"),
        id_row("0000000B", "00000001"),
    ]);
    one(&pkg, "durable_mismatch", Class::Error);
}

#[test]
fn a_comment_part_that_nothing_relates_is_an_error() {
    let pkg = pair(&[]);
    let state = pkg.open_related(|k| k != PartKind::CommentsExtended);
    let found = audit(&state.parts()).unwrap();
    assert_eq!(
        found.iter().map(|f| f.code).collect::<Vec<_>>(),
        vec!["part_unrelated"]
    );
    assert_eq!(found[0].part.as_deref(), Some("word/commentsExtended.xml"));
}

#[test]
fn findings_name_the_comment_and_say_what_is_wrong() {
    let pkg = Pkg::new("<w:p><w:r><w:t>a</w:t></w:r></w:p>").comments(&[comment(
        7,
        "A",
        &[Some("0000000A")],
        "x",
    )]);
    let state = pkg.open();
    let found = audit(&state.parts()).unwrap();
    assert_eq!(found[0].comment, Some(7));
    assert!(found[0].message.contains("c7"), "{}", found[0].message);
}

#[test]
fn a_comment_that_names_itself_as_parent_is_a_loop() {
    let pkg = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "a")))
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")])
        .extended(&[ex("0000000A", Some("0000000A"), false)]);
    one(&pkg, "thread_cycle", Class::Error);
}
