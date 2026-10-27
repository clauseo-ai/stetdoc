//! `Document::occurrences`: where a text is, matched as an operation's `find` matches it, in the scope a view selects, and
//! each hit a target that a batch against the same base applies to.

use super::*;
use crate::api::batch::Batch;
use crate::api::edit::Edit;
use crate::api::op::{Op, ReplaceOp};
use crate::api::stamp::Stamp;
use crate::api::view::Scope;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

const DATE: &str = r#"w:date="2026-01-01T00:00:00Z""#;

/// p1 has the text across runs and formatting, p2 an insertion and a deletion by Opposing, p3 curly quotes, and p4 and p5
/// typed clause numbers.
fn doc() -> Document {
    let body = format!(
        concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">Payment is due within thi</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve">rty days</w:t></w:r><w:r><w:t>.</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Price is </w:t></w:r><w:del w:id="1" w:author="Opposing" {d}><w:r><w:delText>net 30</w:delText></w:r></w:del><w:ins w:id="2" w:author="Opposing" {d}><w:r><w:t>thirty days</w:t></w:r></w:ins><w:r><w:t xml:space="preserve"> after delivery.</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>The “Seller” sells.</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>1. Term</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>2. Notices within thirty days.</w:t></w:r></w:p>"#,
        ),
        d = DATE
    );
    let xml = parts::Doc::new().body(&body).bytes();
    Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

#[test]
fn a_text_is_found_across_runs_and_in_another_authors_insertion() {
    let doc = doc();
    let found = doc.occurrences("thirty days", &Scope::Auto).unwrap();
    assert_eq!(found.base, doc.base());
    let blocks: Vec<&str> = found.hits.iter().map(|h| h.block.as_str()).collect();
    assert_eq!(blocks, ["p1", "p2", "p5"], "{found:?}");
    let first = &found.hits[0];
    assert_eq!((first.start, first.end, first.occurrence), (22, 33, 1));
    assert_eq!(first.snippet, "Payment is due within [thirty days].");
    assert!(!first.loose);
}

#[test]
fn text_only_in_a_deletion_is_listed_apart() {
    let found = doc().occurrences("net 30", &Scope::Auto).unwrap();
    assert!(found.hits.is_empty(), "{found:?}");
    assert_eq!(found.deleted.len(), 1);
    assert_eq!(found.deleted[0].block, "p2");
}

#[test]
fn straight_quotes_find_curly_ones_loosely() {
    let found = doc().occurrences("\"Seller\"", &Scope::Auto).unwrap();
    assert_eq!(found.hits.len(), 1, "{found:?}");
    assert!(found.hits[0].loose);
    assert_eq!(found.hits[0].block, "p3");
}

#[test]
fn a_range_or_a_clause_limits_the_search() {
    let doc = doc();
    let range = Scope::Range {
        from: "p2".into(),
        to: Some("p3".into()),
    };
    let found = doc.occurrences("thirty days", &range).unwrap();
    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].block, "p2");
    let clause = doc
        .occurrences("thirty days", &Scope::Clause(vec!["2".into()]))
        .unwrap();
    let blocks: Vec<&str> = clause.hits.iter().map(|h| h.block.as_str()).collect();
    assert_eq!(blocks, ["p5"], "{clause:?}");
    assert_eq!(clause.hits[0].clause.as_deref(), Some("2"));
    // Several clauses are the union of their blocks.
    let both = doc
        .occurrences("e", &Scope::Clause(vec!["2".into(), "1".into()]))
        .unwrap();
    let mut blocks: Vec<&str> = both.hits.iter().map(|h| h.block.as_str()).collect();
    blocks.dedup();
    assert_eq!(blocks, ["p4", "p5"], "{both:?}");
}

#[test]
fn an_empty_text_or_a_view_only_scope_is_refused() {
    let doc = doc();
    let empty = doc.occurrences("", &Scope::Auto).unwrap_err();
    assert_eq!(empty.code(), ErrorCode::InvalidPayload);
    let outline = doc
        .occurrences("thirty", &Scope::Outline { depth: None })
        .unwrap_err();
    assert_eq!(outline.code(), ErrorCode::InvalidPayload);
}

#[test]
fn every_hit_is_a_target_of_a_batch_against_the_same_base() {
    let mut doc = doc();
    let found = doc.occurrences("thirty days", &Scope::Auto).unwrap();
    // From the end of each paragraph to its start, as the CLI orders them; here one hit per paragraph.
    let ops = found
        .hits
        .iter()
        .map(|h| {
            Op::Replace(ReplaceOp {
                r#ref: h.block.clone(),
                with: "sixty days".into(),
                find: None,
                occurrence: None,
                start: Some(h.start),
                end: Some(h.end),
            })
        })
        .collect();
    let stamp = Stamp::new("Clauseo", "2026-10-06T09:30:00Z");
    doc.edit(Edit::ops(Batch::new(doc.base(), ops), stamp))
        .unwrap();
    let after = doc.occurrences("sixty days", &Scope::Auto).unwrap();
    assert_eq!(after.hits.len(), 3, "{after:?}");
    assert!(
        doc.occurrences("thirty days", &Scope::Auto)
            .unwrap()
            .hits
            .is_empty()
    );
}

#[test]
fn ignoring_case_finds_any_case_at_the_offsets_of_the_final_text() {
    let doc = doc();
    assert!(
        doc.occurrences("PAYMENT is", &Scope::Auto)
            .unwrap()
            .hits
            .is_empty()
    );
    let found = doc
        .occurrences_ignoring_case("PAYMENT is due within THIRTY days", &Scope::Auto)
        .unwrap();
    assert_eq!(found.hits.len(), 1, "{found:?}");
    assert_eq!(
        (
            found.hits[0].block.as_str(),
            found.hits[0].start,
            found.hits[0].end
        ),
        ("p1", 0, 33)
    );
    // curly quotes still match straight ones, and text only in a deletion is still listed apart
    let quoted = doc
        .occurrences_ignoring_case("\"seller\"", &Scope::Auto)
        .unwrap();
    assert!(quoted.hits.len() == 1 && quoted.hits[0].loose, "{quoted:?}");
    let deleted = doc
        .occurrences_ignoring_case("NET 30", &Scope::Auto)
        .unwrap();
    assert!(
        deleted.hits.is_empty() && deleted.deleted.len() == 1,
        "{deleted:?}"
    );
    // the hit is a target a batch applies to
    let mut doc = doc;
    let op = Op::Replace(ReplaceOp {
        r#ref: "p1".into(),
        with: "Payment is due in".into(),
        find: None,
        occurrence: None,
        start: Some(found.hits[0].start),
        end: Some(21),
    });
    doc.edit(Edit::ops(
        Batch::new(found.base.clone(), vec![op]),
        Stamp::new("Clauseo", "2026-10-06T09:30:00Z"),
    ))
    .unwrap();
}
