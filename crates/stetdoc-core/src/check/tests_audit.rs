//! V10: the comment audit as verdicts of `check::edit`. After an edit the audit has no error that
//! the input lacked (C3 of `docs/design/comments-and-notes.md`): a new error is refused, a new note
//! is reported and blocks nothing, and what was there already is carried.

use super::tests::{W, state_with};
use super::tests_edit::{
    Said, accepted, append, detach, el_with, in_part, nth, refused, retext, run_of, say,
};
use super::{Declared, document};
use crate::api::check::FindingCode as C;
use crate::state::State;
use crate::xml::SpanTree;

/// A paragraph with a comment's range and reference, and one without.
const BODY: &str = r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id="0"/><w:r><w:commentReference w:id="0"/></w:r></w:p><w:p><w:r><w:t>other</w:t></w:r></w:p>"#;

fn comments(inner: &str) -> String {
    format!(
        r#"<w:comments xmlns:w="{W}" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml">{inner}</w:comments>"#
    )
}

const COMMENT: &str = r#"<w:comment w:id="0" w:author="a" w:date="2024-01-01T00:00:00Z"><w:p w14:paraId="00000001"><w:r><w:t>note</w:t></w:r></w:p></w:comment>"#;

fn commented(body: &str) -> State {
    state_with(
        body,
        &[(
            "word/comments.xml",
            "comments",
            "comments",
            &comments(COMMENT),
        )],
    )
}

fn say_in_comments(s: &State, change: impl FnOnce(&mut SpanTree)) -> Said {
    say(
        &in_part(s, "word/comments.xml", change),
        &Declared::default(),
    )
}

#[test]
fn a_document_with_a_comment_that_is_whole_has_no_finding() {
    let s = commented(BODY);
    let report = document(&s.parts());
    assert!(report.ok, "{report:?}");
    assert_eq!((report.violations, report.notes), (0, 0), "{report:?}");
}

#[test]
fn an_edit_that_breaks_a_comment_is_refused_with_the_code_of_the_audit() {
    let s = commented(BODY);
    let lost_reference = super::tests_edit::said(&s, |t| {
        let r = nth(t, "commentReference", 0);
        let run = t.node_ref(r).parent().map(|p| p.id()).expect("a run");
        detach(t, run);
    });
    refused(&lost_reference);
    assert_eq!(lost_reference.new, [C::ReferenceMissing]);
    let twice = super::tests_edit::said(&s, |t| {
        let r = nth(t, "commentReference", 0);
        let run = t.node_ref(r).parent().map(|p| p.id()).expect("a run");
        let copy = t.edit().clone_subtree(run).expect("clones");
        let p = nth(t, "p", 1);
        append(t, p, copy);
    });
    refused(&twice);
    assert_eq!(twice.new, [C::ReferenceDuplicate]);
    let open = super::tests_edit::said(&s, |t| {
        let end = nth(t, "commentRangeEnd", 0);
        detach(t, end);
    });
    refused(&open);
    assert_eq!(open.new, [C::MarkerUnpaired]);
    // the comment itself is taken out and its markers are left
    let orphan = say_in_comments(&s, |t| {
        let comment = nth(t, "comment", 0);
        detach(t, comment);
    });
    refused(&orphan);
    assert_eq!(orphan.new, [C::MarkerWithoutComment]);
}

#[test]
fn a_new_note_is_reported_and_blocks_nothing() {
    let s = commented(BODY);
    // a comment that no range and no reference belongs to, with a paragraph id
    let added = say_in_comments(&s, |t| {
        let root = t.root().id();
        let comment = el_with(
            t,
            "comment",
            &[
                ("id", "1"),
                ("author", "a"),
                ("date", "2024-01-01T00:00:00Z"),
            ],
        );
        let p = super::tests_edit::el(t, "p");
        t.edit()
            .set_attr(
                p,
                "http://schemas.microsoft.com/office/word/2010/wordml",
                "paraId",
                "00000002",
            )
            .expect("sets");
        let run = run_of(t, "unanchored");
        append(t, p, run);
        append(t, comment, p);
        append(t, root, comment);
    });
    let verified = accepted(&added);
    let codes: Vec<C> = verified.notes.iter().map(|f| f.code).collect();
    assert_eq!(codes, [C::Unanchored], "{:?}", verified.notes);
    assert_eq!(verified.notes[0].class, crate::api::check::Class::Note);
    assert_eq!(verified.notes[0].part, "word/comments.xml");
}

#[test]
fn a_problem_the_comments_already_had_does_not_block_and_is_not_the_edits() {
    // the reference run is missing from the input
    let body = r#"<w:p><w:commentRangeStart w:id="0"/><w:r><w:t>text</w:t></w:r><w:commentRangeEnd w:id="0"/></w:p><w:p><w:r><w:t>other</w:t></w:r></w:p>"#;
    let s = commented(body);
    let report = document(&s.parts());
    assert!(!report.ok, "{report:?}");
    let elsewhere = super::tests_edit::said(&s, |t| retext(t, 1, "edited"));
    let v = accepted(&elsewhere);
    // the audit speaks of the comments of the document, and not of the blocks the edit changed
    assert_eq!((v.carried_total, v.notes.len()), (0, 0), "{v:?}");
    // the same edit could not have added it
    let s = commented(BODY);
    let lost = super::tests_edit::said(&s, |t| {
        let r = nth(t, "commentReference", 0);
        let run = t.node_ref(r).parent().map(|p| p.id()).expect("a run");
        detach(t, run);
    });
    refused(&lost);
}

#[test]
fn an_edit_far_from_any_comment_is_not_read_by_the_audit_when_there_is_none() {
    let s = state_with("<w:p><w:r><w:t>text</w:t></w:r></w:p>", &[]);
    let v = accepted(&super::tests_edit::said(&s, |t| retext(t, 0, "edited"))).checked;
    assert_eq!(v.validated, 1);
    // a marker with no comment is found although the document has no comments part
    let marked = super::tests_edit::said(&s, |t| {
        let p = nth(t, "p", 0);
        let m = el_with(t, "commentRangeStart", &[("id", "5")]);
        append(t, p, m);
    });
    refused(&marked);
    assert!(
        marked.new.contains(&C::MarkerWithoutComment),
        "{:?}",
        marked.new
    );
}

#[test]
fn every_code_the_audit_can_give_is_a_finding_code_of_the_same_class() {
    // the codes are the literals the audit hands to `finding`, and the class follows them
    let source = include_str!("../threads/audit.rs");
    let mut codes = std::collections::BTreeMap::new();
    let mut rest = source;
    while let Some(i) = rest.find("finding(") {
        let (before, from) = rest.split_at(i);
        let call = from.get("finding(".len()..).unwrap_or_default();
        rest = call;
        if before.ends_with("fn ") {
            continue;
        }
        let Some(start) = call.find('"') else { break };
        let tail = call.get(start + 1..).unwrap_or_default();
        let Some(end) = tail.find('"') else { break };
        let after = tail.get(end..).unwrap_or_default();
        let (error, note) = (after.find("Class::Error"), after.find("Class::Note"));
        let class = match (error, note) {
            (Some(e), Some(n)) if e < n => crate::api::check::Class::Violation,
            (Some(_), None) => crate::api::check::Class::Violation,
            _ => crate::api::check::Class::Note,
        };
        codes.insert(tail.get(..end).unwrap_or_default().to_owned(), class);
    }
    assert!(codes.len() >= 15, "{codes:?}");
    for (name, class) in &codes {
        let code = C::from_name(name).unwrap_or_else(|| panic!("{name} is not a finding code"));
        assert_eq!(code.class(), *class, "{name}");
    }
}
