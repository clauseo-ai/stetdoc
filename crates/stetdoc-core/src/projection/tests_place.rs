//! Where a find hit or an occurrence says it is: the clause it is or whose own text it is, else the nearest heading; a hit
//! in a table with the rest of its row, paged by line; and the changes of a document shown whole with their neighbours.

use super::build::testing::*;
use super::occurrences::occurrences;
use super::tests_view::{pages, refs_on, spec};
use super::view::view;
use crate::api::view::{RevisionFilter, Scope};

fn find(needle: &str) -> Scope {
    Scope::Find {
        needles: vec![needle.to_string()],
        context: 0,
        from: None,
        to: None,
    }
}

/// A paragraph with a direct outline level, as a heading style gives one.
fn heading(text: &str) -> String {
    px(&format!(
        "<w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr>{}",
        r(text)
    ))
}

/// A table row of plain cells.
fn row(cells: &[&str]) -> String {
    let cells: String = cells
        .iter()
        .map(|c| format!("<w:tc><w:tcPr/>{}</w:tc>", p(c)))
        .collect();
    format!("<w:tr>{cells}</w:tr>")
}

/// Two parties numbered (1) and (2), a recital under a heading, a clause with a list and a paragraph after it, a
/// tracked insertion in 1.2, and a table of prices. Refs: p1 to p10, then t1 with p11 to p16, two cells to a row.
fn contract() -> Fixture {
    let rows: String = [
        ["Product", "Price"],
        ["Fuel oil", "USD 8.50"],
        ["Gas oil", "USD 12.75"],
    ]
    .iter()
    .map(|r| row(r))
    .collect();
    let body = [
        p("(1) ACME LTD, a company in England."),
        p("(2) BETA LLC, a company in Delaware."),
        heading("RECITALS"),
        p("WHEREAS, the Parties wish to trade fuel."),
        p("CLAUSE 1 — FORCE MAJEURE"),
        p("1.1 A Force Majeure Event means any of the following:"),
        p("(a) war;"),
        p("(b) fire."),
        p("For the avoidance of doubt, a strike is not a Force Majeure Event."),
        px(&format!(
            "{}{}{}",
            r("1.2 The Parties trade "),
            ins(1, "Ann", "diesel and "),
            r("fuel.")
        )),
        format!("<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1\"/><w:gridCol w:w=\"1\"/></w:tblGrid>{rows}</w:tbl>"),
    ]
    .concat();
    Fixture::new(&body)
}

fn line<'a>(text: &'a str, r#ref: &str) -> &'a str {
    text.lines()
        .find(|l| l.starts_with(&format!("{ref} ")))
        .unwrap_or_else(|| panic!("no line for {ref}:\n{text}"))
}

#[test]
fn a_recital_after_numbered_parties_is_under_its_heading_and_in_no_clause() {
    let state = contract().state();
    let v = view(&state, &spec(find("wish"), 0)).unwrap();
    assert_eq!(
        line(&v.text, "p4"),
        "p4 WHEREAS, the Parties wish to trade fuel.",
        "{}",
        v.text
    );
    assert!(v.text.contains("{in p3 \"RECITALS\"}"), "{}", v.text);
    let found = occurrences(&state, "wish", &Scope::Auto).unwrap();
    assert_eq!(found.hits[0].clause, None, "{found:?}");
    assert_eq!(found.hits[0].heading.as_deref(), Some("RECITALS"));
}

#[test]
fn a_paragraph_after_a_list_is_in_the_clause_the_list_is_in() {
    let state = contract().state();
    let v = view(&state, &spec(find("avoidance"), 0)).unwrap();
    assert!(
        line(&v.text, "p9").starts_with("p9 {§1.1} For the avoidance"),
        "{}",
        v.text
    );
    // A list item still names itself, and a clause holds its own text.
    let war = view(&state, &spec(find("war"), 0)).unwrap();
    assert!(
        line(&war.text, "p7").starts_with("p7 {§1.1(a)} "),
        "{}",
        war.text
    );
    let found = occurrences(&state, "avoidance", &Scope::Auto).unwrap();
    assert_eq!(found.hits[0].clause.as_deref(), Some("1.1"), "{found:?}");
    assert_eq!(found.hits[0].heading, None);
    let party = occurrences(&state, "ACME", &Scope::Auto).unwrap();
    assert_eq!(party.hits[0].clause.as_deref(), Some("(1)"), "{party:?}");
}

#[test]
fn a_hit_in_a_table_shows_the_rest_of_its_row_and_not_the_table() {
    let state = contract().state();
    let v = view(&state, &spec(find("fuel oil"), 0)).unwrap();
    assert!(
        line(&v.text, "p13").ends_with("Fuel oil {row: p14 \"USD 8.50\"}"),
        "{}",
        v.text
    );
    assert_eq!(refs_on(&v.text), vec!["p13"], "{}", v.text);
    assert!(!v.text.contains("Gas oil"), "{}", v.text);
}

#[test]
fn hits_in_one_table_page_by_line_and_count_each_paragraph() {
    let state = contract().state();
    let all = view(&state, &spec(find("USD"), 0)).unwrap();
    assert_eq!(all.position.scope, "find \"USD\": 2 matches in 2 blocks");
    let shown: Vec<Vec<String>> = pages(&state, find("USD"), 70)
        .iter()
        .map(|v| refs_on(&v.text))
        .collect();
    assert_eq!(shown, vec![vec!["p14"], vec!["p16"]], "{shown:?}");
}

#[test]
fn revisions_with_context_show_each_changed_block_whole_with_its_neighbours() {
    let state = contract().state();
    let filter = RevisionFilter {
        context: 1,
        ..RevisionFilter::default()
    };
    let v = view(&state, &spec(Scope::Revisions(filter), 0)).unwrap();
    let refs = refs_on(&v.text);
    assert_eq!(refs.first().map(String::as_str), Some("p9"), "{}", v.text);
    assert!(refs.contains(&"p10".to_string()), "{}", v.text);
    assert!(
        line(&v.text, "p10").contains("{++diesel and ++}"),
        "{}",
        v.text
    );
    assert_eq!(v.position.scope, "changes: 1 change in 1 block");
    // A filter that keeps nothing shows nothing, and says so.
    let none = RevisionFilter {
        context: 1,
        kind: Some("delete".into()),
        ..RevisionFilter::default()
    };
    let empty = view(&state, &spec(Scope::Revisions(none), 0)).unwrap();
    assert!(refs_on(&empty.text).is_empty(), "{}", empty.text);
    assert_eq!(empty.position.scope, "changes of kind delete: no changes");
}
