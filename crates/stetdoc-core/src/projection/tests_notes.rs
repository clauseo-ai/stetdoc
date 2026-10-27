//! A view of one clause, and the margin notes that come from the index: what a citation came to, and what
//! cites the clause (design 4.2 and 4.6).

use super::build::testing::*;
use super::tests_view::{pages, refs_on, spec};
use super::view::view;
use crate::api::error::ErrorCode;
use crate::api::view::{Mode, Scope, StoryName, ViewSpec};

fn contract() -> Fixture {
    let body = format!(
        "{}{}{}{}{}{}{}{}",
        p("1. Definitions"),
        p("\"Affiliate\" means a related person."),
        p("2. Payment"),
        p("The Buyer pays the Seller within 30 days, subject to clause 1 and to Section 99."),
        p("(a) in cash, as in paragraph (b) below"),
        p("(b) by wire"),
        p("3. Notices"),
        p("Notices under clause 2(a) are in writing. Section 501 of the Code applies.")
    );
    Fixture::new(&body)
}

fn clause(
    state: &crate::state::State,
    text: &str,
) -> Result<crate::api::view::View, crate::api::error::StetError> {
    view(state, &spec(Scope::Clause(text.to_string()), 0))
}

#[test]
fn a_clause_view_is_the_clause_and_what_is_under_it_up_to_the_next_of_its_depth() {
    let state = contract().state();
    let v = clause(&state, "clause 2").unwrap();
    assert_eq!(refs_on(&v.text), vec!["p3", "p4", "p5", "p6"], "{}", v.text);
    assert!(
        v.text.contains("clause 2"),
        "the position says what it is a page of: {}",
        v.text
    );
    let sub = clause(&state, "2(a)").unwrap();
    assert_eq!(refs_on(&sub.text), vec!["p5"], "{}", sub.text);
    let last = clause(&state, "3").unwrap();
    assert_eq!(refs_on(&last.text), vec!["p7", "p8"]);
}

#[test]
fn a_clause_view_says_which_blocks_cite_the_clause() {
    let state = contract().state();
    let v = clause(&state, "1").unwrap();
    assert!(v.text.contains("{~ cited by p4}"), "{}", v.text);
    let two_a = clause(&state, "2(a)").unwrap();
    assert!(two_a.text.contains("{~ cited by p8}"), "{}", two_a.text);
    let none = clause(&state, "3").unwrap();
    assert!(!none.text.contains("cited by"), "{}", none.text);
}

#[test]
fn a_clause_that_is_not_there_or_not_unique_is_refused_with_the_candidates() {
    let state = contract().state();
    let miss = clause(&state, "clause 9").unwrap_err();
    assert_eq!(miss.code(), ErrorCode::NotFound);
    let junk = clause(&state, "pay the price").unwrap_err();
    assert_eq!(junk.code(), ErrorCode::InvalidPayload);
    let twice = Fixture::new(&format!("{}{}", p("1. First"), p("1. Second"))).state();
    let e = clause(&twice, "1").unwrap_err();
    assert_eq!(e.code(), ErrorCode::AmbiguousMatch);
    assert!(
        e.message().contains("p1") && e.message().contains("p2"),
        "{}",
        e.message()
    );
}

#[test]
fn a_mention_gets_a_note_with_where_it_points_and_how_sure_that_is() {
    let state = contract().state();
    let v = view(&state, &spec(Scope::Auto, 0)).unwrap();
    let lines: Vec<&str> = v.text.lines().filter(|l| l.starts_with("{~")).collect();
    assert!(
        lines.contains(&"{~ \"clause 1\" -> p1 (1 Definitions)}"),
        "resolved: {lines:#?}"
    );
    assert!(
        lines.contains(&"{~ \"Section 99\" -> unresolved}"),
        "unresolved: {lines:#?}"
    );
    assert!(
        lines.contains(&"{~ \"paragraph (b)\" -> p6 (2(b) by wire) relative to this block}"),
        "relative: {lines:#?}"
    );
    assert!(
        lines.contains(&"{~ \"clause 2(a)\" -> p5 (2(a) in cash, as in paragraph (b) below)}"),
        "{lines:#?}"
    );
    assert!(
        lines.contains(&"{~ \"Section 501\" -> outside the document}"),
        "outside: {lines:#?}"
    );
}

#[test]
fn a_note_follows_the_block_it_is_about() {
    let state = contract().state();
    let v = view(&state, &spec(Scope::Auto, 0)).unwrap();
    let lines: Vec<&str> = v.text.lines().collect();
    let at = lines.iter().position(|l| l.starts_with("p4 ")).unwrap();
    assert!(lines[at + 1].starts_with("{~ \"clause 1\""), "{}", v.text);
    assert!(lines[at + 2].starts_with("{~ \"Section 99\""), "{}", v.text);
}

#[test]
fn notes_false_and_the_original_mode_have_no_notes_and_neither_has_a_file() {
    let state = contract().state();
    let off = view(
        &state,
        &ViewSpec {
            notes: false,
            ..spec(Scope::Auto, 0)
        },
    )
    .unwrap();
    assert!(!off.text.contains("{~"), "{}", off.text);
    let original = view(
        &state,
        &ViewSpec {
            mode: Mode::Original,
            ..spec(Scope::Auto, 0)
        },
    )
    .unwrap();
    assert!(!original.text.contains("{~"), "{}", original.text);
    let files = model(&state, Mode::Redline);
    let text = crate::projection::formats::by_id("stet/1")
        .unwrap()
        .render(&files)
        .files;
    assert!(text.iter().all(|(_, t)| !t.contains("{~")));
}

#[test]
fn an_ambiguous_mention_lists_the_nearest_candidates_and_counts_the_rest() {
    let mut body = String::new();
    for i in 0..6 {
        body.push_str(&p(&format!("1. Repeated heading {i}")));
    }
    body.push_str(&p("See clause 1."));
    let state = Fixture::new(&body).state();
    let v = view(&state, &spec(Scope::Auto, 0)).unwrap();
    let line = v
        .text
        .lines()
        .find(|l| l.starts_with("{~ \"clause 1\""))
        .unwrap();
    assert!(line.contains("ambiguous: p6 (1 Repeated heading 5), p5 (1 Repeated heading 4), p4 (1 Repeated heading 3) and 3 more"), "{line}");
}

#[test]
fn a_find_and_a_clause_view_page_with_their_notes_and_still_follow_every_block_once() {
    let state = contract().state();
    for scope in [
        Scope::Story(StoryName::Body),
        Scope::Clause("clause 2".into()),
        Scope::Find {
            needles: vec!["clause".into()],
            context: 1,
            from: None,
            to: None,
        },
    ] {
        let whole = view(&state, &spec(scope.clone(), 0)).unwrap();
        let paged = pages(&state, scope.clone(), 90);
        let mut got: Vec<String> = Vec::new();
        for page in &paged {
            got.extend(refs_on(&page.text));
        }
        assert_eq!(got, refs_on(&whole.text), "{scope:?}");
        assert!(
            paged.len() > 1,
            "{scope:?} should need several pages at this budget"
        );
    }
}

#[test]
fn a_ref_field_carries_the_ref_of_the_paragraph_its_bookmark_is_in() {
    let body = format!(
        "{}{}",
        px(&format!(
            "<w:bookmarkStart w:id=\"1\" w:name=\"_Ref100\"/>{}<w:bookmarkEnd w:id=\"1\"/>",
            r("Payment")
        )),
        px(&format!(
            "{}<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> REF _Ref100 \\r \\h </w:instrText></w:r>\
             <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>\
             <w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> REF Gone </w:instrText></w:r>\
             <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            r("See "),
            r("4.2"),
            r("Error!")
        ))
    );
    let v = view(&Fixture::new(&body).state(), &spec(Scope::Auto, 0)).unwrap();
    assert!(v.text.contains("{field REF \"4.2\" ->p1}"), "{}", v.text);
    assert!(
        v.text.contains("{field REF \"Error!\"}"),
        "a bookmark that is gone has no target: {}",
        v.text
    );
}

#[test]
fn a_relative_note_lists_the_candidates_it_was_chosen_over_after_the_target() {
    let body = format!(
        "{}{}{}{}{}",
        p("(b) a recital that is also called b"),
        p("4. Payment"),
        p("(a) in cash"),
        p("(b) by wire"),
        p("(c) or as in clause (b) above")
    );
    let state = Fixture::new(&body).state();
    let v = view(&state, &spec(Scope::Auto, 0)).unwrap();
    let lines: Vec<&str> = v.text.lines().filter(|l| l.starts_with("{~")).collect();
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("{~ \"clause (b)\" -> p4 (")
                && l.contains(") relative to this block, or else p1 (")),
        "{lines:#?}"
    );
}
