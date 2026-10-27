//! The compact page of a find: one line for each paragraph that matches, several needles at once, paging, a range,
//! the ledger, and the blocks whole when context is asked for.

use super::build::testing::*;
use super::tests_view::{doc, pages, refs_on, spec, table_doc};
use super::view::view;
use crate::api::error::ErrorCode;
use crate::api::view::{FormatId, RevisionFilter, Scope, ViewSpec};

fn find(needles: &[&str], context: u8) -> Scope {
    Scope::Find {
        needles: needles.iter().map(|n| (*n).to_string()).collect(),
        context,
        from: None,
        to: None,
    }
}

/// A short contract with typed clause numbers, two headings, and one long paragraph.
fn contract() -> Fixture {
    let long = format!(
        "1.2 {} Payment is due within thirty (30) days of the Due Date. {}",
        "The Buyer shall pay every invoice in full and on time, without any deduction or set-off of any kind.".repeat(2),
        "Interest accrues on any late payment at the rate set out in Schedule 1.".repeat(2)
    );
    let body = [
        "CLAUSE 1 — PAYMENT",
        "1.1 The Buyer shall pay the Price.",
        &long,
        "CLAUSE 2 — TERM",
        "2.1 This Agreement lasts thirty years.",
        "2.2 Either party may end it on ninety days notice.",
    ]
    .iter()
    .map(|t| p(t))
    .collect::<String>();
    Fixture::new(&body)
}

#[test]
fn a_find_lists_one_line_for_each_paragraph_with_its_clause_and_the_text_around_the_match() {
    let state = contract().state();
    let v = view(&state, &spec(find(&["days"], 0), 0)).unwrap();
    let lines: Vec<&str> = v.text.lines().collect();
    let hit = lines
        .iter()
        .find(|l| l.starts_with("p3 "))
        .unwrap_or_else(|| panic!("{}", v.text));
    // The text is the paragraph's own, cut at both ends around the match, and the clause is named.
    assert!(hit.contains("{§1.2} …"), "{hit}");
    assert!(
        hit.contains("within thirty (30) days of the Due Date."),
        "{hit}"
    );
    assert!(hit.trim_end().ends_with('…'), "{hit}");
    let inner = hit.split('…').nth(1).unwrap();
    let whole = contract_text(2);
    assert!(whole.contains(inner), "{inner:?} is not in the paragraph");
    assert!(
        inner.chars().count() <= 110,
        "{} characters",
        inner.chars().count()
    );
    // Hits sit under a line that names their outermost clause, and a short paragraph is whole.
    let under = lines
        .iter()
        .position(|l| l.starts_with("{in p4 "))
        .unwrap_or_else(|| panic!("{}", v.text));
    assert!(lines[under].contains("TERM"), "{}", lines[under]);
    assert_eq!(
        lines[under + 1],
        "p6 2.2 Either party may end it on ninety days notice.",
        "{}",
        v.text
    );
    assert_eq!(refs_on(&v.text), vec!["p3", "p6"], "{}", v.text);
    assert_eq!(v.position.scope, "find \"days\": 2 matches in 2 blocks");
    assert!(
        !v.text.contains("{style"),
        "a compact line has no style: {}",
        v.text
    );
}

/// The text of the paragraph at `index` of `contract()`.
fn contract_text(index: usize) -> String {
    let state = contract().state();
    let v = view(&state, &spec(Scope::Auto, 0)).unwrap();
    let line = v
        .text
        .lines()
        .find(|l| l.starts_with(&format!("p{} ", index + 1)))
        .unwrap()
        .to_string();
    line.split_once(' ').unwrap().1.to_string()
}

#[test]
fn a_find_that_matches_a_clause_heading_says_how_to_view_that_clause_whole() {
    let state = contract().state();
    let v = view(&state, &spec(find(&["PAYMENT"], 0), 0)).unwrap();
    // The text starts with its label, Clause 1, so the line does not repeat it.
    assert!(v.text.contains("p1 CLAUSE 1 — PAYMENT\n"), "{}", v.text);
    assert!(
        v.text
            .contains("{tip: p1 is the heading of clause 1: view clause 1 to read it whole, or look up clause 1"),
        "{}",
        v.text
    );
    // The clause the tip names is one a clause view resolves.
    let clause = view(&state, &spec(Scope::Clause(vec!["clause 1".into()]), 0)).unwrap();
    assert_eq!(
        refs_on(&clause.text),
        vec!["p1", "p2", "p3"],
        "{}",
        clause.text
    );
    // Prose is no heading, so a find of it gives no tip.
    let prose = view(&state, &spec(find(&["ninety"], 0), 0)).unwrap();
    assert!(!prose.text.contains("{tip:"), "{}", prose.text);
}

#[test]
fn several_needles_find_any_and_each_line_and_the_position_count_each() {
    let state = contract().state();
    let v = view(
        &state,
        &spec(find(&["thirty", "Due Date", "nowhere"], 0), 0),
    )
    .unwrap();
    assert_eq!(
        v.position.scope,
        "find \"thirty\" (2), \"Due Date\" (1), \"nowhere\" (0): 3 matches in 2 blocks"
    );
    let p3 = v.text.lines().find(|l| l.starts_with("p3 ")).unwrap();
    assert!(p3.ends_with("{~ \"thirty\" 1, \"Due Date\" 1}"), "{p3}");
    let p5 = v.text.lines().find(|l| l.starts_with("p5 ")).unwrap();
    assert!(p5.ends_with("{~ \"thirty\" 1}"), "{p5}");
    // A needle given twice counts once, and a blank one is refused.
    let twice = view(&state, &spec(find(&["thirty", "thirty"], 0), 0)).unwrap();
    assert_eq!(
        twice.position.scope,
        "find \"thirty\": 2 matches in 2 blocks"
    );
    let blank = view(&state, &spec(find(&["thirty", " "], 0), 0)).unwrap_err();
    assert_eq!(blank.code(), ErrorCode::InvalidPayload);
    let none = view(&state, &spec(find(&[], 0), 0)).unwrap_err();
    assert_eq!(none.code(), ErrorCode::InvalidPayload);
}

#[test]
fn context_shows_the_blocks_whole_and_says_which_needles_each_holds() {
    let state = contract().state();
    let v = view(&state, &spec(find(&["thirty", "ninety"], 1), 0)).unwrap();
    assert_eq!(
        refs_on(&v.text),
        vec!["p2", "p3", "p4", "p5", "p6"],
        "{}",
        v.text
    );
    assert!(v.text.contains("{~ \"thirty\" 1}"), "{}", v.text);
    assert!(v.text.contains("{~ \"ninety\" 1}"), "{}", v.text);
    // The whole paragraph, with no ellipsis.
    assert!(v.text.contains(&contract_text(2)), "{}", v.text);
    // One needle keeps the margin it always had.
    let one = view(&state, &spec(find(&["ninety"], 1), 0)).unwrap();
    assert!(one.text.contains("{~ 1 match}"), "{}", one.text);
}

#[test]
fn a_compact_find_pages_and_visits_each_hit_once() {
    let state = doc(60).state();
    let all = pages(&state, find(&["paragraph"], 0), 150);
    assert!(all.len() > 2, "{} pages", all.len());
    assert!(
        all[0]
            .position
            .scope
            .starts_with("find \"paragraph\": 60 matches in 60 blocks, showing 1.."),
        "{}",
        all[0].position.scope
    );
    let shown: Vec<String> = all.iter().flat_map(|v| refs_on(&v.text)).collect();
    let wanted: Vec<String> = (1..=60).map(|i| format!("p{i}")).collect();
    assert_eq!(shown, wanted);
    for v in &all {
        assert!(v.tokens <= 150, "{} tokens", v.tokens);
    }
    assert!(all.last().unwrap().next.is_none());
    // A compact page of a long paragraph is far smaller than the paragraph whole.
    let state = contract().state();
    let compact = view(&state, &spec(find(&["Due Date"], 0), 0)).unwrap();
    let whole = view(&state, &spec(find(&["Due Date"], 1), 0)).unwrap();
    assert!(
        compact.text.len() * 2 < whole.text.len(),
        "{} and {}",
        compact.text.len(),
        whole.text.len()
    );
}

#[test]
fn a_compact_find_lists_the_cells_of_a_table_and_goes_on_after_it() {
    let state = table_doc().state();
    let all = pages(&state, find(&["table"], 0), 100);
    let shown: Vec<String> = all.iter().flat_map(|v| refs_on(&v.text)).collect();
    // Before the table, each of its 30 cells, and after it: each once, in order.
    assert_eq!(shown.len(), 32, "{shown:?}");
    let mut unique = shown.clone();
    unique.dedup();
    assert_eq!(unique, shown);
    assert!(all.len() >= 3, "{} pages", all.len());
    assert!(all.last().unwrap().next.is_none());
}

#[test]
fn a_compact_find_within_a_range_lists_only_the_hits_inside_it() {
    let state = doc(40).state();
    let v = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["number 1".into(), "number 3".into()],
                context: 0,
                from: Some("p12".into()),
                to: Some("p31".into()),
            },
            0,
        ),
    )
    .unwrap();
    let mut wanted: Vec<String> = (12..=19).map(|i| format!("p{i}")).collect();
    wanted.extend((30..=31).map(|i| format!("p{i}")));
    assert_eq!(refs_on(&v.text), wanted, "{}", v.text);
    assert_eq!(
        v.position.scope,
        "find \"number 1\" (8), \"number 3\" (2) in p12..p31: 10 matches in 10 blocks"
    );
}

#[test]
fn a_ledger_find_with_several_texts_keeps_the_changes_that_hold_any() {
    let body = format!(
        "{}{}{}",
        px(&ins(1, "JD", "the sum of USD 100")),
        px(&ins(2, "JD", "within ninety days")),
        px(&ins(3, "JD", "under English law")),
    );
    let state = Fixture::new(&body).state();
    let v = view(
        &state,
        &spec(
            Scope::Revisions(RevisionFilter {
                find: vec!["usd".into(), "law".into()],
                ..RevisionFilter::default()
            }),
            0,
        ),
    )
    .unwrap();
    let lines: Vec<&str> = v.text.lines().filter(|l| l.starts_with('#')).collect();
    assert_eq!(lines.len(), 2, "{}", v.text);
    assert_eq!(v.position.scope, "changes with text \"usd\" or \"law\"");
}

#[test]
fn a_find_reads_one_needle_or_a_list_from_json_and_a_compact_page_has_a_json_form() {
    let one: ViewSpec =
        serde_json::from_str(r#"{"scope":{"find":{"needle":"days","context":0}}}"#).unwrap();
    assert_eq!(one.scope, find(&["days"], 0));
    let many: ViewSpec =
        serde_json::from_str(r#"{"scope":{"find":{"needles":["days","law"],"context":0}}}"#)
            .unwrap();
    assert_eq!(many.scope, find(&["days", "law"], 0));
    let ledger: RevisionFilter = serde_json::from_str(r#"{"find":"usd"}"#).unwrap();
    assert_eq!(ledger.find, vec!["usd".to_string()]);
    let none: RevisionFilter = serde_json::from_str(r#"{"find":null}"#).unwrap();
    assert!(none.find.is_empty());
    let state = contract().state();
    let v = view(
        &state,
        &ViewSpec {
            format: FormatId::new(FormatId::JSON),
            ..spec(find(&["days", "PAYMENT"], 0), 0)
        },
    )
    .unwrap();
    let json: serde_json::Value = serde_json::from_str(&v.text).unwrap();
    let hits = &json["body"]["hits"];
    assert_eq!(hits["needles"], 2, "{json}");
    assert_eq!(hits["hits"][0]["ref"], "p1", "{json}");
    assert_eq!(hits["tips"][0]["clause"], "clause 1", "{json}");
}
