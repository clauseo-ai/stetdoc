//! A view of several clauses (decision 0012): the union of their blocks in document order, each once, with each clause
//! citing what cites it, paged as one clause is, and the JSON forms of a clause scope.

use super::build::testing::*;
use super::tests_view::{pages, refs_on, spec};
use super::view::view;
use crate::api::error::{ErrorCode, StetError};
use crate::api::view::{Scope, View};

/// Four clauses, the second with two sub-clauses. p4 cites clause 1, p8 cites 2(a) and p10 cites clause 3.
fn contract() -> Fixture {
    let body = [
        p("1. Definitions"),
        p("\"Affiliate\" means a related person."),
        p("2. Payment"),
        p("The Buyer pays the Seller within 30 days, subject to clause 1."),
        p("(a) in cash"),
        p("(b) by wire"),
        p("3. Notices"),
        p("Notices under clause 2(a) are in writing."),
        p("4. Term"),
        p("This Agreement lasts a year, and clause 3 survives it."),
    ]
    .concat();
    Fixture::new(&body)
}

fn clauses(state: &crate::state::State, texts: &[&str]) -> Result<View, StetError> {
    let scope = Scope::Clause(texts.iter().map(|t| (*t).to_string()).collect());
    view(state, &spec(scope, 0))
}

#[test]
fn several_clauses_are_their_blocks_in_document_order_whatever_order_they_are_named_in() {
    let state = contract().state();
    let v = clauses(&state, &["3", "clause 1"]).unwrap();
    assert_eq!(refs_on(&v.text), vec!["p1", "p2", "p7", "p8"], "{}", v.text);
    assert_eq!(v.position.scope, "clauses 1, 3");
    assert_eq!(v.position.total, 4);
    // Each clause named is a focus, so each says what cites it.
    assert!(v.text.contains("{~ cited by p4}"), "{}", v.text);
    assert!(v.text.contains("{~ cited by p10}"), "{}", v.text);
}

#[test]
fn a_clause_and_one_inside_it_or_the_same_clause_twice_show_each_block_once() {
    let state = contract().state();
    let one = clauses(&state, &["2"]).unwrap();
    let nested = clauses(&state, &["2(a)", "2", "clause 2"]).unwrap();
    assert_eq!(refs_on(&nested.text), vec!["p3", "p4", "p5", "p6"]);
    assert_eq!(refs_on(&nested.text), refs_on(&one.text));
    assert_eq!(nested.position.scope, "clauses 2, 2(a)");
    // 2(a) is a focus too, and what cites it shows under it.
    assert!(nested.text.contains("{~ cited by p8}"), "{}", nested.text);
    let twice = clauses(&state, &["1", "clause 1"]).unwrap();
    assert_eq!(refs_on(&twice.text), vec!["p1", "p2"]);
    assert_eq!(twice.position.scope, "clause 1");
}

#[test]
fn one_clause_in_a_list_is_the_view_of_that_clause() {
    let state = contract().state();
    let v = clauses(&state, &["clause 2"]).unwrap();
    assert_eq!(v.position.scope, "clause 2");
    assert_eq!(refs_on(&v.text), vec!["p3", "p4", "p5", "p6"]);
}

#[test]
fn a_clause_that_is_not_there_fails_the_view_and_is_named() {
    let state = contract().state();
    let miss = clauses(&state, &["1", "9"]).unwrap_err();
    assert_eq!(miss.code(), ErrorCode::NotFound);
    assert!(miss.message().contains("\"9\""), "{}", miss.message());
    let junk = clauses(&state, &["pay the price", "1"]).unwrap_err();
    assert_eq!(junk.code(), ErrorCode::InvalidPayload);
    assert!(
        junk.message().contains("pay the price"),
        "{}",
        junk.message()
    );
    let none = clauses(&state, &[]).unwrap_err();
    assert_eq!(none.code(), ErrorCode::InvalidPayload);
}

#[test]
fn several_clauses_page_as_one_does_and_every_block_shows_once() {
    let state = contract().state();
    let scope = Scope::Clause(vec!["4".into(), "1".into(), "2(b)".into()]);
    let all = view(&state, &spec(scope.clone(), 0)).unwrap();
    let whole = refs_on(&all.text);
    assert_eq!(whole, vec!["p1", "p2", "p6", "p9", "p10"]);
    let paged = pages(&state, scope, 1);
    assert!(paged.len() > 1, "a budget of 1 pages");
    let mut seen = Vec::new();
    for page in &paged {
        assert!(
            page.position.scope.starts_with("clauses 1, 2(b), 4"),
            "{}",
            page.position.scope
        );
        seen.extend(refs_on(&page.text));
    }
    assert_eq!(seen, whole);
}

#[test]
fn json_takes_one_clause_as_a_string_or_several_as_a_list_and_writes_one_as_a_string() {
    let one: Scope = serde_json::from_str(r#"{"clause": "11.1"}"#).unwrap();
    assert_eq!(one, Scope::Clause(vec!["11.1".into()]));
    let many: Scope = serde_json::from_str(r#"{"clause": ["11.1", "12.3"]}"#).unwrap();
    assert_eq!(many, Scope::Clause(vec!["11.1".into(), "12.3".into()]));
    assert_eq!(serde_json::to_string(&one).unwrap(), r#"{"clause":"11.1"}"#);
    assert_eq!(
        serde_json::to_string(&many).unwrap(),
        r#"{"clause":["11.1","12.3"]}"#
    );
    assert!(serde_json::from_str::<Scope>(r#"{"clause": 11}"#).is_err());
}
