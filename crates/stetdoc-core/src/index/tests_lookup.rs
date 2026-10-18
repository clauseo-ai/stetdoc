//! `lookup`: every hit, in the order block, clause, term, and a message for a miss that helps (design 7.5).

use super::lookup;
use super::testing::*;
use crate::api::error::ErrorCode;
use crate::api::lookup::{Hit, Lookup};
use crate::state::State;
use crate::wordml::BlockKey;

fn numbered(level: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>{}</w:p>",
        r(text)
    )
}

/// The block that a ref names, found by walking the stories: `index` has no business with the syntax of a ref.
fn block_of(doc: &State, text: &str) -> Option<BlockKey> {
    let parts = doc.parts();
    for spec in parts.story_specs().ok()?.0.iter() {
        let story = parts.story(parts.id(&spec.part)?).ok()?;
        for entry in story.blocks() {
            let key = story.key(entry);
            if parts.ref_of(&key).is_some_and(|r| r.to_string() == text) {
                return Some(key);
            }
        }
    }
    None
}

fn ask(f: &Fixture, query: &str) -> Result<Lookup, crate::api::error::StetError> {
    let doc = f.state();
    let block = block_of(&doc, query);
    lookup(doc.parts(), &doc.base_token().to_string(), query, block)
}

fn contract() -> Fixture {
    let body = format!(
        "{}{}{}{}{}{}",
        p("This Agreement is between Acme Inc. (the \"Seller\") and Beta LLC (the \"Buyer\")."),
        numbered(0, "Definitions"),
        p("\"Affiliate\" means any person under common control with the Seller."),
        numbered(0, "Payment"),
        numbered(
            1,
            "The Buyer pays the Seller within 30 days, subject to clause 1."
        ),
        p("Schedule 1. The Seller and Buyer agree. The Affiliates are bound.")
    );
    Fixture::new(&body).numbering()
}

#[test]
fn a_term_hit_has_every_definition_and_every_use_with_the_exact_count() {
    let got = ask(&contract(), "Seller").unwrap();
    assert_eq!(got.hits.len(), 1);
    let Hit::Term {
        term,
        matched,
        definitions,
        uses,
        use_count,
        multiple_definitions,
    } = &got.hits[0]
    else {
        panic!("{:?}", got.hits[0]);
    };
    assert_eq!(
        (term.as_str(), matched.as_str(), *multiple_definitions),
        ("Seller", "exact", false)
    );
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        (definitions[0].block.as_str(), definitions[0].kind.as_str()),
        ("p1", "parenthetical")
    );
    let at: Vec<&str> = uses.iter().map(|u| u.block.as_str()).collect();
    assert_eq!(
        at,
        vec!["p3", "p5", "p6"],
        "the definition's own quotes are not a use, and p1 has no other"
    );
    assert_eq!(*use_count, 3);
}

#[test]
fn a_quoted_query_is_a_term_and_a_miss_in_case_falls_back_with_a_flag() {
    let f = contract();
    let quoted = ask(&f, "\"Buyer\"").unwrap();
    assert!(
        matches!(&quoted.hits[0], Hit::Term { term, matched, .. } if term == "Buyer" && matched == "exact")
    );
    let folded = ask(&f, "buyer").unwrap();
    assert!(
        matches!(&folded.hits[0], Hit::Term { term, matched, .. } if term == "Buyer" && matched == "case_insensitive")
    );
}

#[test]
fn a_clause_hit_has_the_composed_label_and_how_it_matched() {
    let got = ask(&contract(), "clause 2(a)").unwrap();
    assert_eq!(got.hits.len(), 1);
    let Hit::Clause {
        block,
        label,
        full,
        source,
        composed,
        noun,
        matched_by,
        ..
    } = &got.hits[0]
    else {
        panic!("{:?}", got.hits[0]);
    };
    assert_eq!(
        (block.as_str(), label.as_str(), full.as_str()),
        ("p5", "(a)", "2(a)")
    );
    assert_eq!(
        (source.as_str(), *composed, noun.is_none()),
        ("computed", false, true)
    );
    assert_eq!(matched_by, "the full label");
}

#[test]
fn a_ref_is_a_block_with_what_refers_to_it_and_what_it_refers_to() {
    let got = ask(&contract(), "p4").unwrap();
    // p4 is the "Payment" heading (clause 2), p5 is 2(a) which cites clause 1.
    let Hit::Block {
        block,
        label,
        full,
        inbound,
        outbound,
        defines,
        ..
    } = &got.hits[0]
    else {
        panic!("{:?}", got.hits[0]);
    };
    assert_eq!(
        (block.as_str(), label.as_deref(), full.as_deref()),
        ("p4", Some("2."), Some("2"))
    );
    assert!(inbound.is_empty() && outbound.is_empty() && defines.is_empty());
    let first = ask(&contract(), "p2").unwrap();
    let Hit::Block { inbound, .. } = &first.hits[0] else {
        panic!()
    };
    assert_eq!(inbound.len(), 1);
    assert_eq!(
        (
            inbound[0].from.as_str(),
            inbound[0].state.as_str(),
            inbound[0].to.as_deref()
        ),
        ("p5", "resolved", Some("p2"))
    );
    let citing = ask(&contract(), "p5").unwrap();
    let Hit::Block { outbound, .. } = &citing.hits[0] else {
        panic!()
    };
    assert_eq!(outbound[0].text, "clause 1");
}

#[test]
fn a_block_that_defines_terms_says_which() {
    let got = ask(&contract(), "p3").unwrap();
    let Hit::Block { defines, .. } = &got.hits[0] else {
        panic!()
    };
    assert_eq!(defines, &vec!["Affiliate".to_string()]);
}

#[test]
fn a_query_that_is_a_clause_and_a_term_gets_both_in_the_fixed_order() {
    let body = format!(
        "{}{}",
        p("Schedule 1. Fees"),
        p("\"Schedule 1\" means the fee schedule attached.")
    );
    let got = ask(&Fixture::new(&body), "Schedule 1").unwrap();
    let kinds: Vec<&str> = got
        .hits
        .iter()
        .map(|h| match h {
            Hit::Block { .. } => "block",
            Hit::Clause { .. } => "clause",
            Hit::Term { .. } => "term",
        })
        .collect();
    assert_eq!(kinds, vec!["clause", "term"]);
}

#[test]
fn a_clause_that_two_paragraphs_have_returns_both_and_chooses_neither() {
    let body = format!("{}{}", p("1. First"), p("1. Second"));
    let got = ask(&Fixture::new(&body), "clause 1").unwrap();
    assert_eq!(got.hits.len(), 2);
}

#[test]
fn a_miss_is_not_found_and_lists_what_starts_the_same_way() {
    let err = ask(&contract(), "Affil").unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
    assert!(err.message().contains("Affiliate"), "{}", err.message());
    let none = ask(&contract(), "Zebra").unwrap_err();
    assert!(
        none.message()
            .contains("Nothing in the document starts the same way"),
        "{}",
        none.message()
    );
    assert_eq!(
        ask(&contract(), "  ").unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
}

#[test]
fn a_lookup_carries_its_query_and_the_base_token_it_was_made_at() {
    let f = contract();
    let doc = f.state();
    let got = ask(&f, "Buyer").unwrap();
    assert_eq!(got.query, "Buyer");
    assert_eq!(got.base, doc.base_token().to_string());
}
