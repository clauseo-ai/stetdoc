//! A long thread, which a walk that is quadratic in its length would take minutes over, and the handle that
//! refuses to write a story.

use super::delete::delete;
use super::edit::edit;
use super::resolve::set_resolved;
use super::tests_support::*;
use crate::api::error::ErrorCode;
use crate::revisions::handle::PartWriter;
use crate::threads::tests_support::*;
use crate::threads::{Threads, audit};

/// A root and `n` replies to it, all by Clauseo, with a paragraph id and a row each.
fn long_thread(n: usize) -> Pkg {
    let para = |i: usize| format!("{:08X}", i + 1);
    let comments: Vec<String> = (0..=n)
        .map(|i| comment(i as i64, "Clauseo", &[Some(&para(i))], "x"))
        .collect();
    let rows: Vec<String> = (0..=n)
        .map(|i| ex(&para(i), (i > 0).then(|| para(0)).as_deref(), false))
        .collect();
    Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "text")))
        .comments(&comments)
        .extended(&rows)
}

#[test]
fn resolving_and_reopening_a_flat_thread_of_four_thousand_replies_is_fast() {
    let (resolved, open, findings) = finishes(|| {
        let state = long_thread(4_000).open();
        let mut tx = state.begin();
        let who = stamp();
        let mut c = compiler(&mut tx, &who);
        let fx = set_resolved(&mut c, 0, true).unwrap();
        assert_eq!(fx.parts.len(), 1);
        let again = set_resolved(&mut c, 17, true).unwrap();
        assert_eq!(again.warnings[0].code, "already_resolved");
        let resolved = Threads::of(&c.tx().parts())
            .unwrap()
            .comments()
            .iter()
            .filter(|c| c.resolved)
            .count();
        set_resolved(&mut c, 4_000, false).unwrap();
        let open = Threads::of(&c.tx().parts())
            .unwrap()
            .comments()
            .iter()
            .filter(|c| !c.resolved)
            .count();
        c.finish().unwrap();
        (resolved, open, audit(&tx.parts()).unwrap().len())
    });
    assert_eq!((resolved, open), (4_001, 4_001));
    assert_eq!(
        findings, 4_000,
        "the replies have no range, which is a note, and was before"
    );
}

#[test]
fn deleting_a_long_thread_is_fast() {
    let left = finishes(|| {
        let state = long_thread(3_000).open();
        let mut tx = state.begin();
        let who = stamp();
        let mut c = compiler(&mut tx, &who);
        let gone = delete(&mut c, &who, 0).unwrap();
        assert_eq!(gone.len(), 3_001);
        c.finish().unwrap();
        Threads::of(&tx.parts()).unwrap().comments().len()
    });
    assert_eq!(left, 0);
}

#[test]
fn a_comment_that_holds_more_than_paragraphs_is_not_edited() {
    let table = "<w:comment w:id=\"0\" w:author=\"Clauseo\" w:date=\"2026-01-01T00:00:00Z\" w:initials=\"C\"><w:p><w:r><w:t>intro</w:t></w:r></w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/></w:comment>";
    let state = Pkg::new(&format!("<w:p>{}</w:p>", ranged(0, "text")))
        .comments(&[table.to_owned()])
        .open();
    let mut tx = state.begin();
    let who = stamp();
    let mut c = compiler(&mut tx, &who);
    let e = edit(&mut c, &who, 0, "New text.").unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(e.message().contains("table"), "{}", e.message());
}

#[test]
fn the_handle_writes_the_comment_parts_and_refuses_the_stories() {
    let state = Pkg::new("<w:p><w:r><w:t>text</w:t></w:r></w:p>")
        .comments(&[])
        .footnotes("")
        .open();
    let mut tx = state.begin();
    let mut w = PartWriter::over(&mut tx);
    assert!(w.part_mut("word/comments.xml").is_ok());
    for story in ["word/document.xml", "word/footnotes.xml"] {
        let e = w.part_mut(story).err().unwrap();
        assert_eq!(e.code(), ErrorCode::Internal, "{story}");
        assert!(e.message().contains("only the compiler"), "{}", e.message());
    }
    let new = crate::package::NewPart {
        name: "word/header9.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        bytes: b"<w:hdr/>".to_vec(),
        related_from: None,
    };
    assert_eq!(w.add_part(new).unwrap_err().code(), ErrorCode::Internal);
}

#[test]
fn the_handle_adds_only_comment_parts_and_changes_no_other_part() {
    const COMMENTS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
    const HEADER: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/header";
    const FOOTNOTES: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes";
    const SETTINGS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings";
    const COMMENTS_CT: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml";
    const HEADER_CT: &str =
        "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml";
    fn new(
        name: &'static str,
        content_type: &'static str,
        from: Option<(&'static str, &'static str)>,
    ) -> crate::package::NewPart<'static> {
        crate::package::NewPart {
            name,
            content_type,
            bytes: b"<x/>".to_vec(),
            related_from: from,
        }
    }
    const MAIN: &str = "word/document.xml";
    let state = Pkg::new("<w:p><w:r><w:t>text</w:t></w:r></w:p>")
        .styles("")
        .open();
    let mut tx = state.begin();
    let mut w = PartWriter::over(&mut tx);
    // A story's relationship type is refused whatever the content type says, and so is a comment part whose
    // content type is a story's, one with no relationship, one related from another part and any other kind.
    for refused in [
        new("word/comments9.xml", COMMENTS_CT, Some((MAIN, HEADER))),
        new("word/comments9.xml", COMMENTS_CT, Some((MAIN, FOOTNOTES))),
        new("word/header9.xml", HEADER_CT, Some((MAIN, COMMENTS))),
        new("word/comments9.xml", COMMENTS_CT, None),
        new(
            "word/comments9.xml",
            COMMENTS_CT,
            Some(("word/styles.xml", COMMENTS)),
        ),
        new("word/x.xml", "application/xml", Some((MAIN, SETTINGS))),
    ] {
        let name = refused.name;
        let e = w.add_part(refused).unwrap_err();
        assert_eq!(e.code(), ErrorCode::Internal, "{name}");
        assert!(!w.parts().contains(name), "{name}");
    }
    // A part that is not a comment part, the people part or the style sheet is refused, and so are the
    // content types and the relationships, which change only with a part that is added.
    for other in [
        "[Content_Types].xml",
        "word/_rels/document.xml.rels",
        "word/nothing.xml",
    ] {
        assert!(w.part_mut(other).is_err(), "{other}");
    }
    assert!(w.part_mut("word/styles.xml").is_ok());
    // A comment part with its own relationship type and content type is added, and is then writable.
    w.add_part(new(
        "word/comments.xml",
        COMMENTS_CT,
        Some((MAIN, COMMENTS)),
    ))
    .unwrap();
    assert!(w.part_mut("word/comments.xml").is_ok());
}
