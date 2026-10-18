//! Notes: a note that no reference names goes when a resolution takes out its last reference, and the
//! function that does it is the one that operations on notes call (design 3.6.4). And the deletion of a
//! whole note body, whose last paragraph loses its mark too (`RP050`).

use super::index::ChangeIndex;
use super::notes::remove_unreferenced_notes;
use super::resolve::resolve;
use super::stamp::Track;
use super::tests_support::*;
use super::types::{Action, Selector, Step};
use crate::api::limits::Limits;
use crate::state::State;
use crate::wordml::BlockKey;
use stetdoc_testkit::fixtures::{
    parts::{self, Doc},
    zip::{Entry, Zip},
};

const NOTES: &str = "word/footnotes.xml";

fn note(id: i32, kind: &str, inner: &str) -> String {
    format!("<w:footnote{kind} w:id=\"{id}\">{inner}</w:footnote>")
}

/// A state with a body and a footnotes part: the two separators and the notes given.
fn with_notes(body: &str, notes: &[String]) -> State {
    let doc = Doc::new().body(body).root_attrs(EXTRA_NS).bytes();
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/></Types>",
    );
    let rels = format!(
        "{}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes\" Target=\"footnotes.xml\"/></Relationships>",
        parts::DECL
    );
    let footnotes = format!(
        "{}<w:footnotes xmlns:w=\"{}\">{}{}{}</w:footnotes>",
        parts::DECL,
        parts::NS_W,
        note(
            -1,
            " w:type=\"separator\"",
            "<w:p><w:r><w:separator/></w:r></w:p>"
        ),
        note(
            0,
            " w:type=\"continuationSeparator\"",
            "<w:p><w:r><w:continuationSeparator/></w:r></w:p>"
        ),
        notes.concat()
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new(NOTES, footnotes));
    State::open(Zip::new(entries).build(), &Limits::default())
        .unwrap_or_else(|e| panic!("the test notes do not open: {e}"))
}

fn notes_xml(tx: &crate::state::Tx<'_>) -> String {
    let view = tx.parts();
    let id = view.id(NOTES).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

fn ids_of_notes(xml: &str) -> Vec<String> {
    xml.split("<w:footnote ")
        .skip(1)
        .filter_map(|s| s.split("w:id=\"").nth(1))
        .filter_map(|s| s.split('"').next())
        .map(str::to_owned)
        .collect()
}

fn accept_all() -> Step {
    Step {
        action: Action::Accept,
        selector: Selector::All,
    }
}

fn reject_all() -> Step {
    Step {
        action: Action::Reject,
        selector: Selector::All,
    }
}

/// The body of a deleted footnote as RP050 has it: the reference in a deletion, and in the note the
/// paragraph's mark and its content deleted.
fn deleted_note() -> (String, Vec<String>) {
    let body = format!(
        "<w:p>{}<w:del w:id=\"1\" {AT}><w:r><w:footnoteReference w:id=\"1\"/></w:r></w:del></w:p>",
        r("text")
    );
    let inner = format!(
        "<w:p><w:pPr><w:rPr><w:del w:id=\"2\" {AT}/></w:rPr></w:pPr><w:del w:id=\"3\" {AT}><w:r><w:footnoteRef/></w:r><w:r><w:delText>the note</w:delText></w:r></w:del></w:p>"
    );
    (body, vec![note(1, "", &inner)])
}

#[test]
fn accepting_the_deletion_of_a_footnote_reference_removes_the_note_and_keeps_the_separators() {
    let (body, notes) = deleted_note();
    let s = with_notes(&body, &notes);
    let mut tx = s.begin();
    let res = resolve(&mut tx, &mut ChangeIndex::new(), &[accept_all()]).unwrap();
    assert_eq!(
        ids_of_notes(&notes_xml(&tx)),
        ["-1", "0"],
        "{}",
        notes_xml(&tx)
    );
    assert_eq!(res[0].notes.len(), 1);
    assert_eq!((res[0].notes[0].id, res[0].notes[0].endnote), (1, false));
    assert!(
        !body_xml(&tx).contains("footnoteReference"),
        "{}",
        body_xml(&tx)
    );
}

#[test]
fn rejecting_the_insertion_of_a_footnote_removes_the_reference_and_the_note_and_retires_its_paragraph_as_rejected()
 {
    let body = format!(
        "<w:p>{}<w:ins w:id=\"1\" {AT}><w:r><w:footnoteReference w:id=\"1\"/></w:r></w:ins></w:p>",
        r("text")
    );
    let inner = format!(
        "<w:p><w:pPr><w:rPr><w:ins w:id=\"2\" {AT}/></w:rPr></w:pPr><w:ins w:id=\"3\" {AT}><w:r><w:footnoteRef/></w:r><w:r><w:t>the note</w:t></w:r></w:ins></w:p>"
    );
    let s = with_notes(&body, &[note(1, "", &inner)]);
    let mut tx = s.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &[reject_all()]).unwrap();
    assert_eq!(ids_of_notes(&notes_xml(&tx)), ["-1", "0"]);
    let (_, committed) = tx.commit().unwrap();
    assert!(
        committed
            .retired
            .iter()
            .any(|(_, why)| format!("{why:?}").contains("Rejected")),
        "{:?}",
        committed.retired
    );
}

#[test]
fn a_note_that_a_reference_still_names_stays() {
    // The deletion of the reference is pending, so the reference is in the document still: rejecting
    // something else leaves the note where it is.
    let (body, notes) = deleted_note();
    let s = with_notes(
        &format!(
            "{body}<w:p><w:ins w:id=\"9\" {AT}>{}</w:ins></w:p>",
            r("more")
        ),
        &notes,
    );
    let mut tx = s.begin();
    let res = resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[Step {
            action: Action::Reject,
            selector: Selector::Ids(vec![9]),
        }],
    )
    .unwrap();
    assert!(res[0].notes.is_empty());
    assert_eq!(ids_of_notes(&notes_xml(&tx)), ["-1", "0", "1"]);
}

#[test]
fn the_cleanup_that_operations_call_removes_only_the_note_of_a_reference_taken_out_in_this_transaction()
 {
    let body = format!(
        "<w:p>{}<w:r><w:footnoteReference w:id=\"1\"/></w:r></w:p>",
        r("text")
    );
    let inner = "<w:p><w:r><w:footnoteRef/></w:r><w:r><w:t>the note</w:t></w:r></w:p>";
    // Note 2 is named by no reference when the transaction begins, and it is not this transaction's to
    // remove. Note 1 is named by the reference that the transaction takes out.
    let s = with_notes(&body, &[note(1, "", inner), note(2, "", inner)]);
    let mut tx = s.begin();
    let gone = remove_unreferenced_notes(&mut tx, false).unwrap();
    assert!(gone.is_empty(), "{gone:?}");
    assert_eq!(ids_of_notes(&notes_xml(&tx)), ["-1", "0", "1", "2"]);
    let ks = para_keys(&tx);
    {
        let mut c = super::compiler::Compiler::new(&mut tx, Track::Untracked).unwrap();
        c.replace_text(&ks[0], 4..5, "").unwrap();
        c.finish().unwrap();
    }
    let gone = remove_unreferenced_notes(&mut tx, false).unwrap();
    assert_eq!(gone.iter().map(|n| n.id).collect::<Vec<_>>(), [1]);
    assert_eq!(ids_of_notes(&notes_xml(&tx)), ["-1", "0", "2"]);
}

#[test]
fn deleting_a_whole_body_deletes_the_mark_of_the_last_paragraph_too_only_when_asked() {
    // `delete_block` leaves the mark of the last paragraph of a container, which nothing follows.
    // `delete_block_with_mark` deletes it, as the whole deletion of a note does (RP050).
    let body = format!("{}{}", p(&r("one")), p(&r("two")));
    for (with_mark, want) in [(false, 1usize), (true, 2usize)] {
        let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
            let delete = |c: &mut super::compiler::Compiler<'_, '_>, k: &BlockKey| {
                if with_mark {
                    c.delete_block_with_mark(k)
                } else {
                    c.delete_block(k)
                }
            };
            vec![delete(c, &ks[0]), delete(c, &ks[1])]
        });
        s.assert_clean();
        assert_eq!(s.xml.matches("<w:rPr><w:del ").count(), want, "{}", s.xml);
        assert_eq!(s.fin, ["", ""]);
    }
}

#[test]
fn a_resolution_removes_only_the_notes_that_lost_their_last_reference() {
    // A note that nothing named before the step is not the step's to remove, though the cleanup that an
    // operation calls would take it.
    let (body, mut notes) = deleted_note();
    notes.push(note(
        2,
        "",
        "<w:p><w:r><w:footnoteRef/></w:r><w:r><w:t>nobody names it</w:t></w:r></w:p>",
    ));
    let s = with_notes(&body, &notes);
    let mut tx = s.begin();
    let res = resolve(&mut tx, &mut ChangeIndex::new(), &[accept_all()]).unwrap();
    assert_eq!(res[0].notes.len(), 1);
    assert_eq!(res[0].notes[0].id, 1);
    assert_eq!(ids_of_notes(&notes_xml(&tx)), ["-1", "0", "2"]);
    // And the other entry point, which compares with the start of the transaction, leaves it too.
    let gone = remove_unreferenced_notes(&mut tx, false).unwrap();
    assert!(gone.is_empty(), "{gone:?}");
}

#[test]
fn a_note_that_stays_keeps_a_paragraph_on_accept_and_a_reject_gives_back_what_was_there() {
    // The reference stays, so the note stays: accepting the deletion of all its content leaves it one empty
    // paragraph. A reject gives back what was there before the insertions (WC063-Footnote-Mod has an empty note),
    // so it adds none.
    let body = format!(
        "<w:p>{}<w:r><w:footnoteReference w:id=\"1\"/></w:r></w:p>",
        r("text")
    );
    let deleted = format!(
        "<w:p><w:pPr><w:rPr><w:del w:id=\"2\" {AT}/></w:rPr></w:pPr><w:del w:id=\"3\" {AT}><w:r><w:footnoteRef/></w:r><w:r><w:delText>the note</w:delText></w:r></w:del></w:p>"
    );
    let s = with_notes(&body, &[note(1, "", &deleted)]);
    let mut tx = s.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &[accept_all()]).unwrap();
    let xml = notes_xml(&tx);
    assert!(
        xml.contains("<w:footnote w:id=\"1\"><w:p/></w:footnote>"),
        "{xml}"
    );
    let inserted = format!(
        "<w:p><w:pPr><w:rPr><w:ins w:id=\"2\" {AT}/></w:rPr></w:pPr><w:ins w:id=\"3\" {AT}><w:r><w:footnoteRef/></w:r><w:r><w:t>the note</w:t></w:r></w:ins></w:p>"
    );
    let s = with_notes(&body, &[note(1, "", &inserted)]);
    let mut tx = s.begin();
    resolve(&mut tx, &mut ChangeIndex::new(), &[reject_all()]).unwrap();
    let xml = notes_xml(&tx);
    assert!(!xml.contains("<w:footnote w:id=\"1\"><w:p/>"), "{xml}");
}
