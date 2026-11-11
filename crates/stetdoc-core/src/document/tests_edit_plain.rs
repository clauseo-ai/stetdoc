//! Untracked edits through `Document::edit` take a note out with its reference, as accepting a tracked deletion of it does
//! (issue 15).

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use stetdoc_testkit::fixtures::parts::{self, DECL, NS_W};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

/// Runs `ops` on `doc`, tracked or not.
fn edit(doc: &mut Document, ops: &str, tracked: bool) -> Result<Receipt, StetError> {
    let b = batch(doc, ops);
    let e = if tracked {
        Edit::ops(b, stamp())
    } else {
        Edit::ops(b, stamp()).untracked()
    };
    doc.edit(e)
}

fn part_xml(doc: &Document, name: &str) -> String {
    let view = doc.state.parts();
    let id = view.id(name).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A body of `body` and a footnotes part with the separators and the notes `notes`, each `(id, text)`.
fn with_notes(body: &str, notes: &[(i64, &str)]) -> Document {
    let doc = parts::Doc::new().body(body).bytes();
    let types = parts::content_types().replace(
        "</Types>",
        "<Override PartName=\"/word/footnotes.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml\"/></Types>",
    );
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"{NS_R}/footnotes\" Target=\"footnotes.xml\"/></Relationships>"
    );
    let bodies: String = notes
        .iter()
        .map(|(id, text)| {
            format!(
                r#"<w:footnote w:id="{id}"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> {text}</w:t></w:r></w:p></w:footnote>"#
            )
        })
        .collect();
    let notes = format!(
        "{DECL}<w:footnotes xmlns:w=\"{NS_W}\"><w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>{bodies}</w:footnotes>"
    );
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new("word/footnotes.xml", notes));
    Document::open(Zip::new(entries).build(), &OpenOptions::default()).unwrap()
}

/// The reporter's fixture: `Alpha {fn1} beta`, then two plain paragraphs.
fn footnoted() -> Document {
    with_notes(
        concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">Alpha </w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:t xml:space="preserve"> beta</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>Unrelated paragraph.</w:t></w:r></w:p>"#,
        ),
        &[(1, "Note body.")],
    )
}

/// The same paragraph in the one cell of a table.
fn footnoted_in_a_cell() -> Document {
    with_notes(
        concat!(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr/>"#,
            r#"<w:p><w:r><w:t xml:space="preserve">Alpha </w:t></w:r><w:r><w:footnoteReference w:id="1"/></w:r><w:r><w:t xml:space="preserve"> beta</w:t></w:r></w:p>"#,
            r#"</w:tc></w:tr></w:tbl><w:p><w:r><w:t>Unrelated paragraph.</w:t></w:r></w:p>"#,
        ),
        &[(1, "Note body.")],
    )
}

/// The `notes_removed` of every operation of a receipt.
fn notes_removed(receipt: &Receipt) -> Vec<String> {
    receipt
        .operations
        .iter()
        .flat_map(|o| o.notes_removed.iter().cloned())
        .collect()
}

#[test]
fn an_untracked_replace_over_a_note_reference_takes_the_note_out_and_says_so() {
    let mut doc = footnoted();
    let receipt = edit(
        &mut doc,
        r#"{"op":"replace","ref":"p1","start":0,"end":12,"with":"Gamma"}"#,
        false,
    )
    .unwrap();
    assert_eq!(final_text(&doc, "p1"), "Gamma");
    let notes = part_xml(&doc, "word/footnotes.xml");
    assert!(!notes.contains("Note body."), "{notes}");
    // The separators stay.
    assert!(notes.contains("w:separator"), "{notes}");
    assert_eq!(notes_removed(&receipt), vec!["footnote (id 1)".to_string()]);
    assert!(receipt.text.contains("footnote (id 1)"), "{}", receipt.text);
    let report = doc.check();
    assert!(report.ok && report.findings.is_empty(), "{report:?}");
}

#[test]
fn an_untracked_rewrite_of_a_cell_with_a_note_reference_takes_the_note_out() {
    let mut doc = footnoted_in_a_cell();
    let receipt = edit(
        &mut doc,
        r#"{"op":"replace","ref":"p1","start":0,"end":12,"with":"Gamma"}"#,
        false,
    )
    .unwrap();
    assert!(!part_xml(&doc, "word/footnotes.xml").contains("Note body."));
    assert_eq!(notes_removed(&receipt), vec!["footnote (id 1)".to_string()]);
    let report = doc.check();
    assert!(report.findings.is_empty(), "{report:?}");
}

#[test]
fn an_untracked_edit_that_keeps_the_reference_keeps_the_note() {
    let mut doc = footnoted();
    let receipt = edit(
        &mut doc,
        r#"{"op":"replace","ref":"p1","find":"beta","with":"gamma"}"#,
        false,
    )
    .unwrap();
    assert!(part_xml(&doc, "word/footnotes.xml").contains("Note body."));
    assert!(notes_removed(&receipt).is_empty());
    // Tracked, the note stays until the deletion of its reference is accepted.
    let mut doc = footnoted();
    let receipt = edit(
        &mut doc,
        r#"{"op":"replace","ref":"p1","start":0,"end":12,"with":"Gamma"}"#,
        true,
    )
    .unwrap();
    assert!(part_xml(&doc, "word/footnotes.xml").contains("Note body."));
    assert!(notes_removed(&receipt).is_empty());
    assert!(doc.check().findings.is_empty(), "{:?}", doc.check());
}

#[test]
fn check_reports_a_note_that_nothing_references_and_edits_still_go_through() {
    // Note 2 is referenced by nothing; note 1 only from inside a tracked deletion, which still counts.
    let mut doc = with_notes(
        concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">Alpha </w:t></w:r><w:del w:id="9" w:author="Other" w:date="2026-01-01T00:00:00Z"><w:r><w:footnoteReference w:id="1"/></w:r></w:del></w:p>"#,
            r#"<w:p><w:r><w:t>Unrelated paragraph.</w:t></w:r></w:p>"#,
        ),
        &[(1, "Referenced."), (2, "Orphaned.")],
    );
    let report = doc.check();
    let codes: Vec<_> = report.findings.iter().map(|f| f.code.as_str()).collect();
    assert_eq!(codes, vec!["unreferenced_note"], "{report:?}");
    let finding = report.findings.first().unwrap();
    assert_eq!(finding.class, crate::api::check::Class::Note);
    assert_eq!(finding.part, "word/footnotes.xml");
    assert!(
        finding.message.contains("footnote") && finding.message.contains('2'),
        "{}",
        finding.message
    );
    assert!(report.ok, "a note is not a violation: {report:?}");
    // The orphan blocks no edit, tracked or not.
    for (tracked, word) in [(true, "Unrelated"), (false, "paragraph")] {
        let ops = format!(r#"{{"op":"replace","ref":"p2","find":"{word}","with":"changed"}}"#);
        edit(&mut doc, &ops, tracked).unwrap();
    }
}

#[test]
fn an_untracked_removal_of_a_row_or_a_table_with_a_note_reference_takes_the_note_out() {
    for ops in [
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
        r#"{"op":"delete","ref":"t1"}"#,
    ] {
        let mut doc = footnoted_in_a_cell();
        let receipt = edit(&mut doc, ops, false).unwrap();
        assert!(
            !part_xml(&doc, "word/footnotes.xml").contains("Note body."),
            "{ops}"
        );
        assert_eq!(
            notes_removed(&receipt),
            vec!["footnote (id 1)".to_string()],
            "{ops}"
        );
        assert!(doc.check().findings.is_empty(), "{ops}: {:?}", doc.check());
    }
}
