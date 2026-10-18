//! Deleting notes, in the shape of `RP050-Deleted-Footnote`.

use super::notes::delete_note;
use super::tests_support::*;
use crate::api::error::ErrorCode;
use crate::api::view::Mode;
use crate::revisions::compiler::Compiler;
use crate::revisions::index::ChangeIndex;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::Track;
use crate::revisions::types::{Action, Selector, Step};
use crate::state::Tx;
use crate::threads::tests_support::*;

const REF: &str = "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteReference w:id=\"2\"/></w:r>";

fn note(id: i64, text: &str) -> String {
    format!(
        "<w:footnote w:id=\"{id}\"><w:p><w:pPr><w:pStyle w:val=\"FootnoteText\"/></w:pPr><w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> {text}</w:t></w:r></w:p></w:footnote>"
    )
}

fn with_note() -> Pkg {
    Pkg::new(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Video </w:t></w:r>{REF}<w:r><w:t xml:space=\"preserve\">provides a way.</w:t></w:r></w:p>"
    ))
    .footnotes(&note(2, "This is a test."))
}

fn final_text(tx: &Tx<'_>, part: &str, n: usize) -> String {
    let view = tx.parts();
    let id = view.id(part).unwrap();
    let (story, tree) = (view.story(id).unwrap(), view.tree(id).unwrap());
    story
        .paragraph(tree, n)
        .map_or_else(String::new, |p| p.text(Mode::Final))
}

#[test]
fn a_tracked_deletion_is_the_shape_word_writes_and_accept_and_reject_do_what_they_should() {
    let state = with_note().open();
    let mut tx = state.begin();
    let counsel = stamp_of("Counsel");
    let fx = {
        let mut c = compiler(&mut tx, &counsel);
        let fx = delete_note(&mut c, false, 2).unwrap();
        c.finish().unwrap();
        fx
    };
    assert_eq!((fx.endnote, fx.id), (false, 2));
    assert!(fx.removed.is_empty());
    assert_eq!(
        fx.parts.iter().map(|p| &**p).collect::<Vec<_>>(),
        ["word/document.xml", "word/footnotes.xml"]
    );
    assert_eq!(
        fx.blocks.len(),
        2,
        "the paragraph with the reference, and the note's paragraph"
    );
    let doc = text(&tx, "word/document.xml");
    // RP050: the reference run inside a deletion, with the text around it left alone.
    let del = doc.find("<w:del ").unwrap();
    assert!(doc.find("footnoteReference").unwrap() > del, "{doc}");
    assert!(doc.contains("provides a way."));
    let notes = text(&tx, "word/footnotes.xml");
    // The note keeps its element, its paragraph's mark is deleted (the last one too) and its runs are in a deletion.
    assert!(notes.contains("<w:footnote w:id=\"2\">"), "{notes}");
    assert!(
        notes.contains("<w:rPr><w:del "),
        "the paragraph mark is deleted: {notes}"
    );
    assert!(notes.contains("<w:delText"), "{notes}");
    assert!(notes.contains("<w:footnoteRef/>"));
    assert_eq!(
        final_text(&tx, "word/document.xml", 0),
        "Video provides a way."
    );
    // Accepting everything removes the note, and the separators stay.
    let (done, _) = tx.commit().unwrap();
    let mut accept = done.begin();
    resolve(
        &mut accept,
        &mut ChangeIndex::new(),
        &[Step {
            action: Action::Accept,
            selector: Selector::All,
        }],
    )
    .unwrap();
    let after = text(&accept, "word/footnotes.xml");
    assert!(!after.contains("w:id=\"2\""), "{after}");
    assert!(after.contains("w:type=\"separator\"") && after.contains("continuationSeparator"));
    assert!(!text(&accept, "word/document.xml").contains("footnoteReference"));
    // Rejecting everything gives the document back.
    let mut reject = done.begin();
    resolve(
        &mut reject,
        &mut ChangeIndex::new(),
        &[Step {
            action: Action::Reject,
            selector: Selector::All,
        }],
    )
    .unwrap();
    assert!(text(&reject, "word/document.xml").contains("footnoteReference w:id=\"2\""));
    assert!(text(&reject, "word/footnotes.xml").contains("This is a test."));
    assert!(final_text(&reject, "word/footnotes.xml", 0).contains("This is a test."));
}

#[test]
fn an_untracked_deletion_takes_the_references_and_then_the_note() {
    let state = with_note().open();
    let mut tx = state.begin();
    let mut c = Compiler::new(&mut tx, Track::Untracked).unwrap();
    let fx = delete_note(&mut c, false, 2).unwrap();
    c.finish().unwrap();
    assert_eq!(fx.removed.len(), 1);
    assert_eq!((fx.removed[0].id, fx.removed[0].endnote), (2, false));
    assert!(!text(&tx, "word/document.xml").contains("footnoteReference"));
    let notes = text(&tx, "word/footnotes.xml");
    assert!(
        !notes.contains("w:id=\"2\"") && notes.contains("w:type=\"separator\""),
        "{notes}"
    );
    assert_eq!(
        final_text(&tx, "word/document.xml", 0),
        "Video provides a way."
    );
}

#[test]
fn a_note_with_two_paragraphs_loses_both_and_two_references_in_one_paragraph_both_go() {
    let two = "<w:footnote w:id=\"2\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t>One.</w:t></w:r></w:p><w:p><w:r><w:t>Two.</w:t></w:r></w:p></w:footnote>";
    let body = format!(
        "<w:p><w:r><w:t>a</w:t></w:r>{REF}<w:r><w:t>b</w:t></w:r>{REF}<w:r><w:t>c</w:t></w:r></w:p>"
    );
    let state = Pkg::new(&body).footnotes(two).open();
    let mut tx = state.begin();
    let counsel = stamp_of("Counsel");
    let mut c = compiler(&mut tx, &counsel);
    delete_note(&mut c, false, 2).unwrap();
    c.finish().unwrap();
    assert_eq!(final_text(&tx, "word/document.xml", 0), "abc");
    let notes = text(&tx, "word/footnotes.xml");
    assert_eq!(
        notes.matches("<w:rPr><w:del ").count(),
        2,
        "both marks, the last too: {notes}"
    );
}

#[test]
fn an_endnote_a_missing_note_and_an_unreferenced_note() {
    let body = "<w:p><w:r><w:t>a</w:t></w:r><w:r><w:endnoteReference w:id=\"3\"/></w:r></w:p>";
    let endnote = "<w:endnote w:id=\"3\"><w:p><w:r><w:endnoteRef/></w:r><w:r><w:t>End.</w:t></w:r></w:p></w:endnote>";
    let state = Pkg::new(body)
        .endnotes(endnote)
        .footnotes(&note(2, "Nobody cites me."))
        .open();
    let counsel = stamp_of("Counsel");
    let mut tx = state.begin();
    {
        let mut c = compiler(&mut tx, &counsel);
        delete_note(&mut c, true, 3).unwrap();
        // A footnote that nothing refers to: only its paragraphs, tracked.
        let fx = delete_note(&mut c, false, 2).unwrap();
        assert_eq!(fx.blocks.len(), 1);
        assert_eq!(
            delete_note(&mut c, false, 9).unwrap_err().code(),
            ErrorCode::NotFound
        );
        c.finish().unwrap();
    }
    assert!(text(&tx, "word/endnotes.xml").contains("<w:rPr><w:del "));
    // Untracked, an unreferenced note cannot be deleted without leaving a note with no block.
    let mut tx2 = state.begin();
    let mut c = Compiler::new(&mut tx2, Track::Untracked).unwrap();
    assert_eq!(
        delete_note(&mut c, false, 2).unwrap_err().code(),
        ErrorCode::Unsupported
    );
}
