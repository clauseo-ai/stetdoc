//! The `Revisions` scope: the ledger of a document, one line per change (design 4.6), its filters, its cursor and what
//! it says when a document has no changes.

use super::build::testing::*;
use super::tests_view::{pages, spec};
use super::view::view;
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::{Cursor, FormatId, RevisionFilter, Scope, View, ViewSpec};
use crate::state::State;

/// Four changes by two authors, one in each paragraph: an insertion by JD (`#1`), a deletion by EW (`#2`), a format
/// change by JD (`#3`) and an insertion by EW (`#4`). Each change is dated on its id's day in January 2026.
fn tracked() -> State {
    Fixture::new(&format!(
        "{}{}{}{}",
        px(&format!("{}{}", r("The Seller "), ins(1, "Jane Doe", "shall pay "))),
        px(&format!(
            "{}{}{}",
            r("Price "),
            del(2, "Eric White", "gross"),
            r(" net")
        )),
        px(
            "<w:r><w:rPr><w:b/><w:rPrChange w:id=\"3\" w:author=\"Jane Doe\" w:date=\"2026-01-04T10:00:00Z\"><w:rPr/></w:rPrChange></w:rPr><w:t xml:space=\"preserve\">Bold now</w:t></w:r>"
        ),
        px(&format!(
            "{}{}",
            r("Governing law: "),
            ins(4, "Eric White", "English law")
        )),
    ))
    .state()
}

/// The ledger of `state` for a filter, from a cursor if there is one.
fn ledger(
    state: &State,
    filter: RevisionFilter,
    budget: u32,
    after: Option<&str>,
) -> Result<View, StetError> {
    view(
        state,
        &ViewSpec {
            after: after.map(Cursor::new),
            ..spec(Scope::Revisions(filter), budget)
        },
    )
}

/// A filter that names an author and a kind, either or both.
fn by(author: Option<&str>, kind: Option<&str>) -> RevisionFilter {
    RevisionFilter {
        author: author.map(str::to_string),
        kind: kind.map(str::to_string),
        ..RevisionFilter::default()
    }
}

/// The lines of a page that are changes: the ones that start with `#`.
fn lines(v: &View) -> Vec<&str> {
    v.text.lines().filter(|l| l.starts_with('#')).collect()
}

#[test]
fn the_ledger_has_one_line_per_change_in_document_order() {
    let state = tracked();
    let v = ledger(&state, by(None, None), 0, None).unwrap();
    assert_eq!(
        lines(&v),
        vec![
            "#1 insert JD 2026-01-02 p1 \"shall pay\"",
            "#2 delete EW 2026-01-03 p2 \"gross\"",
            "#3 format JD 2026-01-04 p3",
            "#4 insert EW 2026-01-05 p4 \"English law\"",
        ],
        "{}",
        v.text
    );
    assert!(v.text.starts_with("{stet 1 view=ledger "), "{}", v.text);
    assert!(v.text.contains("{changes: 4 changes}"), "{}", v.text);
    assert!(
        v.text
            .contains("{authors: JD \"Jane Doe\" · EW \"Eric White\"}"),
        "{}",
        v.text
    );
    assert_eq!(
        (
            v.position.scope.as_str(),
            v.position.shown,
            v.position.total
        ),
        ("changes", 4, 4)
    );
    assert_eq!(
        (v.position.first.as_deref(), v.position.last.as_deref()),
        (Some("change:1"), Some("change:4"))
    );
    assert!(v.next.is_none());
}

#[test]
fn the_author_filter_keeps_one_authors_changes_by_code_or_by_name() {
    let state = tracked();
    for who in ["EW", "Eric White"] {
        let v = ledger(&state, by(Some(who), None), 0, None).unwrap();
        assert_eq!(
            lines(&v),
            vec![
                "#2 delete EW 2026-01-03 p2 \"gross\"",
                "#4 insert EW 2026-01-05 p4 \"English law\"",
            ],
            "{who}"
        );
        assert_eq!(v.position.scope, format!("changes by {who}"));
        assert!(v.text.contains("{changes: 2 changes}"), "{}", v.text);
    }
    let none = ledger(&state, by(Some("Nobody"), None), 0, None).unwrap();
    assert!(lines(&none).is_empty(), "{}", none.text);
    assert!(none.text.contains("{changes: 0 changes}"), "{}", none.text);
    assert_eq!(none.position.total, 0);
}

#[test]
fn the_kind_filter_keeps_one_family_of_changes() {
    let state = tracked();
    let cases = [
        ("insert", vec!["#1", "#4"]),
        ("delete", vec!["#2"]),
        ("format", vec!["#3"]),
        ("move", vec![]),
    ];
    for (kind, ids) in cases {
        let v = ledger(&state, by(None, Some(kind)), 0, None).unwrap();
        let got: Vec<&str> = lines(&v)
            .iter()
            .map(|l| l.split(' ').next().unwrap_or_default())
            .collect();
        assert_eq!(got, ids, "{kind}");
        assert_eq!(v.position.scope, format!("changes of kind {kind}"));
    }
    let bad = ledger(&state, by(None, Some("bogus")), 0, None).unwrap_err();
    assert_eq!(bad.code(), ErrorCode::InvalidPayload);
    assert!(bad.message().contains("bogus"), "{}", bad.message());
}

#[test]
fn the_author_and_the_kind_filters_combine() {
    let state = tracked();
    let v = ledger(&state, by(Some("JD"), Some("insert")), 0, None).unwrap();
    assert_eq!(lines(&v), vec!["#1 insert JD 2026-01-02 p1 \"shall pay\""]);
    assert_eq!(v.position.scope, "changes by JD of kind insert");
}

#[test]
fn an_empty_ledger_says_it_has_no_changes_and_lists_none() {
    let state = Fixture::new(&p("A plain paragraph with no changes.")).state();
    let v = ledger(&state, by(None, None), 0, None).unwrap();
    assert!(lines(&v).is_empty(), "{}", v.text);
    assert!(v.text.contains("{changes: 0 changes}"), "{}", v.text);
    assert!(v.next.is_none());
    assert_eq!(v.position.total, 0);
    let filtered = ledger(&state, by(Some("JD"), Some("delete")), 0, None).unwrap();
    assert!(filtered.text.contains("{changes: 0 changes}"));
}

#[test]
fn the_cursor_continues_after_the_change_it_names_whatever_the_filter() {
    let state = tracked();
    let after_two = ledger(&state, by(None, None), 0, Some("change:2")).unwrap();
    assert_eq!(
        lines(&after_two),
        vec![
            "#3 format JD 2026-01-04 p3",
            "#4 insert EW 2026-01-05 p4 \"English law\"",
        ]
    );
    assert_eq!(
        ledger(&state, by(None, None), 0, Some("change:4"))
            .unwrap()
            .text
            .matches("\n#")
            .count(),
        0
    );
    // The filter keeps EW's changes, and the cursor names a change that JD made: the page goes on at the next one of EW's.
    let kept = ledger(&state, by(Some("EW"), None), 0, Some("change:1")).unwrap();
    assert_eq!(lines(&kept).len(), 2, "{}", kept.text);
    let last = ledger(&state, by(Some("EW"), None), 0, Some("change:3")).unwrap();
    assert_eq!(
        lines(&last),
        vec!["#4 insert EW 2026-01-05 p4 \"English law\""]
    );
    let missing = ledger(&state, by(None, None), 0, Some("change:99")).unwrap_err();
    assert_eq!(missing.code(), ErrorCode::NotFound);
    let wrong = ledger(&state, by(None, None), 0, Some("p3")).unwrap_err();
    assert_eq!(wrong.code(), ErrorCode::InvalidPayload);
}

#[test]
fn pages_hold_every_change_once_and_in_order() {
    let state = tracked();
    let all: Vec<String> = lines(&ledger(&state, by(None, None), 0, None).unwrap())
        .into_iter()
        .map(str::to_string)
        .collect();
    // A budget too small for any page makes each page hold one change, so the cursor is what moves the list on.
    let paged = pages(&state, Scope::Revisions(by(None, None)), 1);
    assert_eq!(paged.len(), 4);
    let got: Vec<String> = paged
        .iter()
        .flat_map(|v| lines(v).into_iter().map(str::to_string))
        .collect();
    assert_eq!(got, all);
    assert_eq!(paged[0].next.as_ref().map(Cursor::as_str), Some("change:1"));
    assert!(paged[3].next.is_none());
}

#[test]
fn the_ledger_is_also_json_and_reads_back_as_the_same_model() {
    use super::formats::json_value::{FromJson, Json};
    use super::model::{Body, Projection};
    let state = tracked();
    let v = view(
        &state,
        &ViewSpec {
            format: FormatId::new(FormatId::JSON),
            ..spec(Scope::Revisions(by(Some("EW"), None)), 0)
        },
    )
    .unwrap();
    assert!(v.text.contains("\"ledger\":{\"total\":2,"), "{}", v.text);
    assert!(v.text.contains("\"kind\":\"run_format\"") || v.text.contains("\"kind\":\"deletion\""));
    assert!(v.text.contains("\"author\":\"Eric White\""), "{}", v.text);
    let back = Projection::from_json(&Json::parse(&v.text, 192).unwrap()).unwrap();
    assert!(
        matches!(&back.body, Body::Ledger(list) if list.total == 2 && list.changes.len() == 2),
        "{back:?}"
    );
}
