//! A find within a range, the counts a find reports, the default budget, the outline that stands for a long body,
//! and a ledger narrowed to the changes whose text holds a needle, with their whole text.

use super::build::testing::*;
use super::tests_view::{doc, pages, refs_on, spec};
use super::view::view;
use crate::api::error::ErrorCode;
use crate::api::view::{Budget, RevisionFilter, Scope, ViewSpec, WarningCode};

fn find(needle: &str, context: u8, from: Option<&str>, to: Option<&str>) -> Scope {
    Scope::Find {
        needle: needle.into(),
        context,
        from: from.map(str::to_string),
        to: to.map(str::to_string),
    }
}

#[test]
fn a_find_within_a_range_shows_only_the_matches_and_context_inside_it() {
    let state = doc(40).state();
    // "number 1" is in p1 and p10 to p19: within p12..p15 only those four, and the context stays inside.
    let v = view(
        &state,
        &spec(find("number 1", 1, Some("p12"), Some("p15")), 0),
    )
    .unwrap();
    assert_eq!(
        refs_on(&v.text),
        vec!["p12", "p13", "p14", "p15"],
        "{}",
        v.text
    );
    assert_eq!(
        v.position.scope,
        "find \"number 1\" in p12..p15: 4 matches in 4 blocks"
    );
    // From a ref to the end of its story.
    let tail = view(&state, &spec(find("number 3", 0, Some("p31"), None), 0)).unwrap();
    assert_eq!(refs_on(&tail.text).first().map(String::as_str), Some("p31"));
    assert!(
        !refs_on(&tail.text).contains(&"p3".to_string()),
        "{}",
        tail.text
    );
    // A range with no match shows nothing, and says so.
    let none = view(
        &state,
        &spec(find("number 7", 0, Some("p20"), Some("p25")), 0),
    )
    .unwrap();
    assert!(refs_on(&none.text).is_empty(), "{}", none.text);
    assert_eq!(
        none.position.scope,
        "find \"number 7\" in p20..p25: no matches"
    );
}

#[test]
fn a_find_within_a_range_refuses_a_backward_range_and_an_end_without_a_start() {
    let state = doc(10).state();
    let back = view(&state, &spec(find("number", 0, Some("p8"), Some("p2")), 0)).unwrap_err();
    assert_eq!(back.code(), ErrorCode::InvalidPayload);
    let lone = view(&state, &spec(find("number", 0, None, Some("p2")), 0)).unwrap_err();
    assert_eq!(lone.code(), ErrorCode::InvalidPayload);
    assert!(lone.next().contains("first ref"), "{}", lone.next());
    let missing = view(&state, &spec(find("number", 0, Some("p99"), None), 0)).unwrap_err();
    assert_eq!(missing.code(), ErrorCode::NotFound);
}

#[test]
fn a_find_pages_and_each_page_says_which_matches_it_shows() {
    let state = doc(60).state();
    let all = pages(&state, find("paragraph", 0, None, None), 200);
    assert!(all.len() > 2, "{} pages", all.len());
    let first = &all[0];
    assert!(
        first
            .position
            .scope
            .starts_with("find \"paragraph\": 60 matches in 60 blocks, showing 1.."),
        "{}",
        first.position.scope
    );
    assert!(first.next.is_some());
    // The pages show each block once, in order, and the last says where it ends.
    let shown: Vec<String> = all.iter().flat_map(|v| refs_on(&v.text)).collect();
    assert_eq!(shown.len(), 60);
    let last = all.last().unwrap();
    assert!(
        last.position.scope.ends_with("..60"),
        "{}",
        last.position.scope
    );
    for v in &all {
        assert!(v.tokens <= 200, "{} tokens", v.tokens);
    }
}

#[test]
fn the_default_budget_keeps_a_page_near_twelve_kilobytes() {
    assert_eq!(Budget::default(), Budget(3_000));
    let state = doc(800).state();
    let v = view(
        &state,
        &ViewSpec {
            scope: find("paragraph", 1, None, None),
            ..ViewSpec::default()
        },
    )
    .unwrap();
    assert!(v.text.len() <= 12_500, "{} bytes", v.text.len());
    assert!(v.next.is_some());
}

#[test]
fn a_whole_body_too_long_for_a_page_shows_its_outline_and_says_how_to_read_it() {
    let state = doc(400).state();
    let v = view(&state, &spec(Scope::Auto, 500)).unwrap();
    let warning = v
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::OutlineShown)
        .expect("an outline_shown warning");
    let next = warning.next.as_deref().unwrap_or_default();
    assert!(next.contains("--range p1..p400"), "{next}");
    assert!(next.contains("--all"), "{next}");
    assert!(v.text.contains("{warning outline_shown"), "{}", v.text);
    // A body that fits is shown whole, with no such warning.
    let small = doc(3).state();
    let whole = view(&small, &spec(Scope::Auto, 500)).unwrap();
    assert!(
        whole
            .warnings
            .iter()
            .all(|w| w.code != WarningCode::OutlineShown)
    );
    // An outline asked for has none either.
    let asked = view(&state, &spec(Scope::Outline { depth: None }, 500)).unwrap();
    assert!(
        asked
            .warnings
            .iter()
            .all(|w| w.code != WarningCode::OutlineShown)
    );
}

/// Three insertions: an amount, a date and a long clause with the amount at its end.
fn amounts() -> crate::state::State {
    let long = format!(
        "{} and the threshold is USD 1,000,000",
        "the parties agree that ".repeat(8)
    );
    Fixture::new(&format!(
        "{}{}{}",
        px(&format!(
            "{}{}",
            r("Price: "),
            ins(1, "Jane Doe", "USD 500")
        )),
        px(&format!(
            "{}{}",
            r("Date: "),
            ins(2, "Jane Doe", "31 March 2025")
        )),
        px(&format!("{}{}", r("Clause: "), ins(3, "Eric White", &long))),
    ))
    .state()
}

fn ledger(filter: RevisionFilter) -> crate::api::view::View {
    view(&amounts(), &spec(Scope::Revisions(filter), 0)).unwrap()
}

#[test]
fn a_ledger_find_keeps_only_the_changes_whose_text_holds_the_needle() {
    let usd = ledger(RevisionFilter {
        find: Some("usd".into()),
        ..RevisionFilter::default()
    });
    let lines: Vec<&str> = usd.text.lines().filter(|l| l.starts_with('#')).collect();
    assert_eq!(lines.len(), 2, "{}", usd.text);
    assert!(lines[0].starts_with("#1 insert"), "{}", usd.text);
    // The needle is found past the 80 characters a line shows.
    assert!(lines[1].starts_with("#3 insert"), "{}", usd.text);
    assert_eq!(usd.position.scope, "changes with text \"usd\"");
    // A capital makes it exact, and nothing matches.
    let exact = ledger(RevisionFilter {
        find: Some("Usd".into()),
        ..RevisionFilter::default()
    });
    assert_eq!(exact.position.total, 0, "{}", exact.text);
    let blank = view(
        &amounts(),
        &spec(
            Scope::Revisions(RevisionFilter {
                find: Some(" ".into()),
                ..RevisionFilter::default()
            }),
            0,
        ),
    )
    .unwrap_err();
    assert_eq!(blank.code(), ErrorCode::InvalidPayload);
}

#[test]
fn a_full_ledger_shows_the_whole_text_of_each_change() {
    let cut = ledger(RevisionFilter::default());
    assert!(!cut.text.contains("1,000,000"), "{}", cut.text);
    assert!(cut.text.contains('…'), "{}", cut.text);
    let full = ledger(RevisionFilter {
        full: true,
        ..RevisionFilter::default()
    });
    assert!(full.text.contains("USD 1,000,000\""), "{}", full.text);
    // Still paged to the budget.
    let paged = pages(
        &amounts(),
        Scope::Revisions(RevisionFilter {
            full: true,
            ..RevisionFilter::default()
        }),
        120,
    );
    assert!(paged.len() > 1, "{} pages", paged.len());
}
