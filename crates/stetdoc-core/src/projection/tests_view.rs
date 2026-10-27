//! The views: paging, the budget, position and determinism (laws P1 to P4 and P7, design 9.1).

use super::build::testing::*;
use super::page::estimate;
use super::view::view;
use crate::api::view::{Budget, Cursor, Scope, View, ViewSpec};
use crate::state::State;

pub(super) fn spec(scope: Scope, budget: u32) -> ViewSpec {
    ViewSpec {
        scope,
        budget: Budget(budget),
        ..ViewSpec::default()
    }
}

/// A body of `n` paragraphs of a size that makes a page of a few hundred tokens hold a handful.
pub(super) fn doc(n: usize) -> Fixture {
    let body: String = (1..=n)
        .map(|i| {
            p(&format!(
                "Paragraph number {i} has some words in it to give it a size."
            ))
        })
        .collect();
    Fixture::new(&body)
}

/// The refs that start a line of a page: the blocks it shows.
pub(super) fn refs_on(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let tok = l.split_whitespace().next()?;
            let (kind, digits) = tok.split_at(1);
            (matches!(kind, "p" | "t" | "r")
                && !digits.is_empty()
                && digits.chars().all(|c| c.is_ascii_digit()))
            .then(|| tok.to_string())
        })
        .collect()
}

/// Every page of a scope, following the cursors.
pub(super) fn pages(state: &State, scope: Scope, budget: u32) -> Vec<View> {
    let mut out: Vec<View> = Vec::new();
    let mut after: Option<Cursor> = None;
    loop {
        let v = view(
            state,
            &ViewSpec {
                after: after.clone(),
                ..spec(scope.clone(), budget)
            },
        )
        .unwrap();
        let next = v.next.clone();
        out.push(v);
        assert!(out.len() < 400, "paging does not end");
        match next {
            Some(c) => after = Some(c),
            None => return out,
        }
    }
}

#[test]
fn a_document_that_fits_is_one_page_with_the_whole_story() {
    let state = doc(5).state();
    let v = view(&state, &ViewSpec::default()).unwrap();
    assert_eq!(refs_on(&v.text), vec!["p1", "p2", "p3", "p4", "p5"]);
    assert!(v.next.is_none());
    assert_eq!(
        (
            v.position.shown,
            v.position.total,
            v.position.scope.as_str()
        ),
        (5, 5, "p1..p5")
    );
    assert!(
        v.text
            .contains("{position: p1..p5 of p1..p5 (5 of 5 blocks)}")
    );
    assert!(!v.text.contains("{rules:"), "a page is read, not edited");
    assert_eq!(v.base, state.base_token().to_string());
}

#[test]
fn p2_to_p4_and_p7_following_the_cursor_visits_every_block_once_within_the_budget() {
    let state = doc(60).state();
    for budget in [150u32, 220, 400, 1000, 5000] {
        let all = pages(
            &state,
            Scope::Story(crate::api::view::StoryName::Body),
            budget,
        );
        let mut seen: Vec<String> = Vec::new();
        for v in &all {
            let refs = refs_on(&v.text);
            // P3: within the budget, or one block.
            assert!(
                v.tokens <= budget || v.position.shown == 1,
                "budget {budget}: {} tokens",
                v.tokens
            );
            // P7: the estimate is the formula.
            assert_eq!(v.tokens, estimate(&v.text));
            // P4: the position is what the page shows.
            assert_eq!(v.position.shown as usize, refs.len());
            assert_eq!(
                v.position.first.as_deref(),
                refs.first().map(String::as_str)
            );
            assert_eq!(v.position.last.as_deref(), refs.last().map(String::as_str));
            assert_eq!(v.position.total, 60);
            assert_eq!(
                v.next.as_ref().map(Cursor::as_str),
                v.next.as_ref().and(refs.last().map(String::as_str))
            );
            seen.extend(refs);
        }
        let want: Vec<String> = (1..=60).map(|i| format!("p{i}")).collect();
        assert_eq!(seen, want, "P2 at budget {budget}");
        if budget == 5000 {
            assert_eq!(all.len(), 1);
        } else {
            assert!(all.len() > 1, "budget {budget} gave one page");
        }
    }
}

#[test]
fn p1_the_same_bytes_and_spec_give_the_same_page() {
    for scope in [
        Scope::Auto,
        Scope::Outline { depth: None },
        Scope::Story(crate::api::view::StoryName::Body),
        Scope::Find {
            needles: vec!["number 7".into()],
            context: 1,
            from: None,
            to: None,
        },
    ] {
        let a = view(&doc(40).state(), &spec(scope.clone(), 400)).unwrap();
        let b = view(&doc(40).state(), &spec(scope.clone(), 400)).unwrap();
        assert_eq!(a, b, "{scope:?}");
    }
}

pub(super) fn table_doc() -> Fixture {
    let row = |i: u32| {
        format!(
            "<w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>",
            p(&format!(
                "Row {i} of the table has a cell with enough words in it to take some room."
            ))
        )
    };
    let rows: String = (1..=30).map(row).collect();
    Fixture::new(&format!(
        "{}<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1\"/></w:tblGrid>{rows}</w:tbl>{}",
        p("before the table"),
        p("after the table")
    ))
}

#[test]
fn a_table_that_does_not_fit_is_cut_between_rows_and_the_cursor_is_a_row() {
    let state = table_doc().state();
    let all = pages(&state, Scope::Story(crate::api::view::StoryName::Body), 300);
    assert!(all.len() >= 4, "{} pages", all.len());
    let mut rows: Vec<String> = Vec::new();
    for (i, v) in all.iter().enumerate() {
        let refs = refs_on(&v.text);
        rows.extend(refs.iter().filter(|r| r.starts_with('r')).cloned());
        let cut = v.text.contains("{~ table ");
        let has_table_line = refs.iter().any(|r| r.starts_with('t'));
        if i > 0 && has_table_line && refs.iter().any(|r| r.starts_with('r')) {
            // A page that goes on with a table says where it goes on.
            assert!(
                cut || v.text.contains("{table cols=1}"),
                "page {i}: {}",
                v.text
            );
        }
        if let Some(next) = &v.next {
            assert!(
                next.as_str().starts_with(['p', 'r']),
                "a cursor is a ref of the last block shown: {next:?}"
            );
        }
        assert!(
            v.tokens <= 300 || v.position.shown == 1,
            "page {i}: {} tokens",
            v.tokens
        );
    }
    let want: Vec<String> = (1..=30).map(|i| format!("r{i}")).collect();
    assert_eq!(rows, want, "every row once, in order");
    let all_text: String = all.iter().map(|v| v.text.as_str()).collect();
    assert!(
        all_text.contains("{~ table t1 goes on at row r"),
        "{all_text}"
    );
    assert!(
        refs_on(&all.last().unwrap().text).contains(&"p32".to_string()),
        "the paragraph after the table is last"
    );
}

#[test]
fn a_cursor_after_the_last_block_is_an_empty_page_and_a_foreign_one_is_refused() {
    let state = doc(3).state();
    let v = view(
        &state,
        &ViewSpec {
            after: Some(Cursor::new("p3")),
            ..spec(Scope::Story(crate::api::view::StoryName::Body), 0)
        },
    )
    .unwrap();
    assert!(refs_on(&v.text).is_empty());
    assert_eq!(v.position.shown, 0);
    let bad = |after: &str| {
        view(
            &state,
            &ViewSpec {
                after: Some(Cursor::new(after)),
                ..spec(Scope::Story(crate::api::view::StoryName::Body), 0)
            },
        )
        .unwrap_err()
    };
    assert_eq!(bad("p999").code(), crate::api::error::ErrorCode::NotFound);
    assert_eq!(
        bad("zzz").code(),
        crate::api::error::ErrorCode::InvalidPayload
    );
}

#[test]
fn a_budget_of_zero_is_no_limit() {
    let state = doc(300).state();
    let v = view(
        &state,
        &spec(Scope::Story(crate::api::view::StoryName::Body), 0),
    )
    .unwrap();
    assert_eq!(refs_on(&v.text).len(), 300);
    assert!(v.next.is_none());
}
