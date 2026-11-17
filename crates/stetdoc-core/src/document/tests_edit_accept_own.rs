//! `accept` and `reject` by author at the end of the batch that made the revisions (issue 28): the author is looked up in the
//! document as the batch's earlier operations left it, so a caller can resolve exactly its own revisions in one call, and the
//! one call writes the same package as the edit followed by a second edit that accepts by author. A code keeps the meaning the
//! view of the input gave it, and a scope narrows `all` as it did before.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::error::ErrorCode;
use crate::api::receipt::Receipt;
use crate::api::stamp::Stamp;
use crate::api::view::Mode;

/// The edits of the reporter's first case, on the contract: a replacement and a new paragraph.
const EDITS: &str = r#"{"op":"replace","ref":"p1","find":"Supplier","with":"Vendor"},
    {"op":"insert","ref":"p2","place":"after","paragraphs":["Inserted paragraph."]}"#;

fn as_who(doc: &mut Document, author: &str, ops: &str) -> Result<Receipt, StetError> {
    let b = batch(doc, ops);
    doc.edit(Edit::ops(b, Stamp::new(author, "2026-10-06T10:00:00")))
}

/// Every part of the saved package, by name, as text.
fn bytes(doc: &Document) -> Vec<(String, String)> {
    let saved = doc.save().unwrap().bytes;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(saved)).unwrap();
    (0..archive.len())
        .map(|i| {
            let mut file = archive.by_index(i).unwrap();
            let mut text = String::new();
            std::io::Read::read_to_string(&mut file, &mut text).unwrap();
            (file.name().unwrap().to_string(), text)
        })
        .collect()
}

/// What each paragraph says in the `Final` and the `Original` view, and the pending authors: the document as a reader sees
/// it, whatever runs a reject left split.
fn shown(doc: &Document) -> (Vec<(String, String)>, Vec<String>) {
    let texts = (1..=12)
        .map(|n| format!("p{n}"))
        .filter(|t| doc.state.resolve(crate::refs::Claim::Implied, t).is_ok())
        .map(|t| (final_text(doc, &t), text(doc, &t, Mode::Original)))
        .collect();
    (texts, pending_authors(doc))
}

/// The authors of the pending revisions, each once, sorted.
fn pending_authors(doc: &Document) -> Vec<String> {
    let mut authors: Vec<String> = doc
        .revisions(&RevisionFilter::default())
        .unwrap()
        .into_iter()
        .map(|r| r.author.unwrap_or_default())
        .collect();
    authors.sort();
    authors.dedup();
    authors
}

/// The contract with a change by another author in `p4`, the paragraph none of `EDITS` touches.
fn with_other() -> Document {
    let mut doc = rich();
    as_who(
        &mut doc,
        "Other Person",
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    )
    .unwrap();
    doc
}

/// Checks the one-call form against the two-call form on `source`, by the author's name and by its code, and that the edits
/// alone reject back to `source` and accept to the same text as the one call.
fn one_call_is_two_calls(source: fn() -> Document, others: &[&str]) {
    let mut two = source();
    run(&mut two, EDITS);
    // Rejecting the edits gives back the source: all of them where the source holds no revision, the author's where it does.
    let mut rejected = source();
    run(&mut rejected, EDITS);
    let reject = if others.is_empty() {
        r#"{"op":"reject","all":true}"#
    } else {
        r#"{"op":"reject","author":"Clauseo"}"#
    };
    run(&mut rejected, reject);
    assert_eq!(shown(&rejected), shown(&source()), "{reject} of the edits");
    assert!(rejected.check().ok, "{:?}", rejected.check());
    run(&mut two, r#"{"op":"accept","author":"Clauseo"}"#);
    for who in ["Clauseo", "C"] {
        let mut one = source();
        let receipt = run(
            &mut one,
            &format!(r#"{EDITS},{{"op":"accept","author":"{who}"}}"#),
        );
        assert!(receipt.operations.iter().all(|o| o.changed), "{who}");
        assert_eq!(bytes(&one), bytes(&two), "one call by {who} and two calls");
        assert_eq!(pending_authors(&one), others, "{who}");
        assert!(one.check().ok, "{who}: {:?}", one.check());
        assert_eq!(final_text(&one, "p1"), final_text(&two, "p1"));
    }
    if others.is_empty() {
        let mut all = source();
        run(
            &mut all,
            &format!(r#"{EDITS},{{"op":"accept","all":true}}"#),
        );
        assert_eq!(bytes(&all), bytes(&two), "accept all in one call");
    }
}

#[test]
fn accept_by_author_resolves_the_revisions_the_same_batch_made() {
    one_call_is_two_calls(rich, &[]);
}

#[test]
fn accept_by_author_in_the_batch_leaves_another_authors_revisions_pending() {
    one_call_is_two_calls(with_other, &["Other Person"]);
    // `all` in the same place takes the other author's revisions too, which is why the author matters.
    let mut all = with_other();
    run(
        &mut all,
        &format!(r#"{EDITS},{{"op":"accept","all":true}}"#),
    );
    assert!(pending_authors(&all).is_empty());
}

#[test]
fn reject_by_author_undoes_the_revisions_the_same_batch_made() {
    for source in [rich as fn() -> Document, with_other] {
        let mut doc = source();
        let receipt = run(
            &mut doc,
            &format!(r#"{EDITS},{{"op":"reject","author":"Clauseo"}}"#),
        );
        assert!(receipt.operations.iter().all(|o| o.changed));
        assert_eq!(shown(&doc), shown(&source()));
        assert!(doc.check().ok, "{:?}", doc.check());
    }
}

#[test]
fn an_untracked_batch_accepts_the_authors_earlier_revisions_as_before() {
    // Untracked edits make no revision, so by author the batch resolves what the input already held, as it always did.
    let mut doc = rich();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let b = batch(
        &doc,
        &format!(r#"{EDITS},{{"op":"accept","author":"Clauseo"}}"#),
    );
    doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(pending_authors(&doc).is_empty());
    assert!(final_text(&doc, "p4").contains("sixty"));
    assert!(final_text(&doc, "p1").contains("Vendor"));
    assert!(doc.check().ok, "{:?}", doc.check());
}

#[test]
fn a_code_means_what_the_view_of_the_input_showed() {
    // Jane Doe's change is late in the body, and the batch's author, Jane Dunn, writes before it, so in the edited document
    // Jane Dunn comes first and would take the code `JD`. The caller read `JD` as Jane Doe, and that is what it still means.
    let with_doe = || {
        let mut doc = rich();
        as_who(
            &mut doc,
            "Jane Doe",
            r#"{"op":"replace","ref":"p10","find":"After","with":"Below"}"#,
        )
        .unwrap();
        doc
    };
    let insert = r#"{"op":"replace","ref":"p1","find":"Supplier","with":"Vendor"}"#;
    let mut codes = with_doe();
    as_who(&mut codes, "Jane Dunn", insert).unwrap();
    let table = crate::projection::authors(codes.state.parts()).unwrap();
    assert_eq!(
        table.first(),
        Some(&("JD".to_string(), "Jane Dunn".to_string()))
    );
    let mut by_code = with_doe();
    as_who(
        &mut by_code,
        "Jane Dunn",
        &format!(r#"{insert},{{"op":"accept","author":"JD"}}"#),
    )
    .unwrap();
    assert_eq!(pending_authors(&by_code), ["Jane Dunn"]);
    // By name, the batch's own author is found.
    let mut doc = with_doe();
    as_who(
        &mut doc,
        "Jane Dunn",
        &format!(r#"{insert},{{"op":"accept","author":"Jane Dunn"}}"#),
    )
    .unwrap();
    assert_eq!(pending_authors(&doc), ["Jane Doe"]);
}

#[test]
fn a_name_the_batch_wrote_wins_over_an_equal_code_from_the_view() {
    // The view of the input shows `OP "Other Person"`, and the batch's author is named `OP`: a full name wins over a code.
    let mut doc = with_other();
    as_who(
        &mut doc,
        "OP",
        &format!(r#"{EDITS},{{"op":"accept","author":"OP"}}"#),
    )
    .unwrap();
    assert_eq!(pending_authors(&doc), ["Other Person"]);
    assert!(final_text(&doc, "p1").contains("Vendor"));
}

#[test]
fn an_author_with_no_revision_is_still_not_found_and_the_message_lists_the_batchs_own() {
    let mut doc = rich();
    let e = fails(
        &mut doc,
        &format!(r#"{EDITS},{{"op":"accept","author":"Nobody"}}"#),
    );
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.message().contains("C (Clauseo)"), "{}", e.message());
    assert_eq!(e.op_index(), Some(2));
    assert_eq!(bytes(&doc), bytes(&rich()));
}

#[test]
fn a_scope_narrows_accept_in_the_batch_as_before() {
    // Issue 29's case: deleting p2 and accepting under `--scope p2` is refused, and `p2..p3` lets it through. By author the
    // same now holds.
    for accept in [
        r#"{"op":"accept","all":true}"#,
        r#"{"op":"accept","author":"Clauseo"}"#,
    ] {
        let ops = format!(r#"{{"op":"delete","ref":"p2"}},{accept}"#);
        let mut doc = rich();
        let b = batch(&doc, &ops);
        let e = doc
            .edit(Edit::ops(b, stamp()).scope(vec!["p2".to_string()]))
            .unwrap_err();
        assert_eq!(
            e.code(),
            ErrorCode::ScopeViolation,
            "{accept}: {}",
            e.message()
        );
        let b = batch(&doc, &ops);
        doc.edit(Edit::ops(b, stamp()).scope(vec!["p2..p3".to_string()]))
            .unwrap();
        assert!(pending_authors(&doc).is_empty(), "{accept}");
    }
}
