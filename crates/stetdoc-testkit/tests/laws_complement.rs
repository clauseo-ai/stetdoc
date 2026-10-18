//! H7, H10 and H12 through the public interface, over small documents.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use stetdoc_core::{Document, OpenOptions, WriteOptions};
use stetdoc_testkit::fixtures::{parts, zip::Zip};
use stetdoc_testkit::laws::complement::refuses_every_single_byte_change;
use stetdoc_testkit::laws::memdir::MemDir;
use stetdoc_testkit::laws::writes::survives_a_crash_at_every_step;

fn document(body: &str) -> Document {
    let doc = parts::Doc::new().body(body).bytes();
    Document::open(
        Zip::new(parts::entries_doc(&doc)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

const BODY: &str = "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p>";

#[test]
fn a_document_is_send() {
    // H10, at compile time.
    fn assert_send<T: Send>() {}
    assert_send::<Document>();
}

#[test]
fn a_damaged_working_copy_is_always_refused() {
    let doc = document(BODY);
    refuses_every_single_byte_change(&doc.export().unwrap()).unwrap();
}

#[test]
fn a_first_unpack_and_a_rewrite_survive_a_crash_at_every_step() {
    let doc = document(BODY);
    let wc = doc.export().unwrap();
    survives_a_crash_at_every_step(None, (&doc, &wc), &WriteOptions::default()).unwrap();
    survives_a_crash_at_every_step(Some((&doc, &wc)), (&doc, &wc), &WriteOptions::default())
        .unwrap();
    // The directory that the plan writes is the working copy, and reading it back gives it.
    let plan = wc.write_plan(None, &WriteOptions::default()).unwrap();
    let mut dir = MemDir::new();
    dir.apply_all(&plan.steps);
    let (read, marker) = dir.read();
    assert_eq!(marker, None);
    assert_eq!(read.complement.entries.len(), wc.complement.entries.len());
    assert_eq!(
        Document::resume(&read, &OpenOptions::default())
            .unwrap()
            .base(),
        doc.base()
    );
}

/// The contract fixture, opened, and its working copy with one line of `document.md` edited by hand.
fn hand_edited() -> (
    Document,
    stetdoc_core::WorkingCopy,
    stetdoc_core::WorkingCopy,
) {
    let doc = Document::open(
        stetdoc_testkit::fixtures::contract::package(),
        &OpenOptions::default(),
    )
    .unwrap();
    let wc = doc.export().unwrap();
    let mut edited = wc.clone();
    edited.files[0].1 = edited.files[0]
        .1
        .replace("within thirty days", "within sixty days");
    assert_ne!(edited.files, wc.files);
    (doc, wc, edited)
}

#[test]
fn a_hand_edit_that_the_next_export_took_in_survives_a_crash_at_every_step() {
    // H12 with hand edits: the agent edited `document.md`, the edit was applied, and the new export writes over it.
    let (doc0, _, on_disk) = hand_edited();
    let pending = on_disk.pending_files().unwrap().expect("a pending file");
    let mut doc1 = Document::open(
        stetdoc_testkit::fixtures::contract::package(),
        &OpenOptions::default(),
    )
    .unwrap();
    doc1.edit(stetdoc_core::Edit::files(
        pending,
        stetdoc_core::Stamp::new("Clauseo", "2026-10-06T10:00:00"),
    ))
    .unwrap();
    let wc1 = doc1.export().unwrap();
    let options = WriteOptions {
        consumed: vec!["document.md".to_string()],
        refresh: false,
    };
    survives_a_crash_at_every_step(Some((&doc0, &on_disk)), (&doc1, &wc1), &options).unwrap();
    // The same plan with the edit not named as consumed is refused before its first step.
    let disk = MemDir::of(&on_disk).read().0;
    let e = wc1
        .write_plan(Some(&disk), &WriteOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), stetdoc_core::ErrorCode::PendingEdits, "{e}");
}

#[test]
fn an_export_that_did_not_take_a_hand_edit_in_never_overwrites_it() {
    // A refresh export of the same state, over a directory that holds an edit nobody applied.
    let (doc0, wc0, on_disk) = hand_edited();
    let disk = MemDir::of(&on_disk).read().0;
    let again = doc0.export().unwrap();
    assert_eq!(again, wc0);
    let e = again
        .write_plan(Some(&disk), &WriteOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), stetdoc_core::ErrorCode::PendingEdits, "{e}");
    // Asking for a refresh overwrites it, and that survives a crash at every step too.
    let refresh = WriteOptions {
        consumed: Vec::new(),
        refresh: true,
    };
    survives_a_crash_at_every_step(Some((&doc0, &on_disk)), (&doc0, &again), &refresh).unwrap();
}
