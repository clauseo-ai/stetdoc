//! `Edit::files` and the readable surface through `Document`: render, edit the text, apply, render again.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::files::ReadableFiles;
use crate::api::limits::Limits;
use crate::api::options::OffsetUnit;
use crate::api::receipt::Receipt;
use crate::api::revision::ChangeKind;
use crate::api::view::{Mode, Scope, ViewSpec, WarningCode};
use crate::refs::{hex, sha256};

/// The files of a document with one line of `document.md` changed.
fn with_line(doc: &Document, from: &str, to: &str) -> ReadableFiles {
    let mut files = doc.files().unwrap();
    let text = &mut files.files[0].1;
    assert!(text.contains(from), "{from} is not in\n{text}");
    *text = text.replacen(from, to, 1);
    files
}

fn apply(doc: &mut Document, files: ReadableFiles) -> Receipt {
    doc.edit(Edit::files(files, stamp())).unwrap()
}

fn fails_files(doc: &mut Document, files: ReadableFiles) -> StetError {
    doc.edit(Edit::files(files, stamp())).err().unwrap()
}

#[test]
fn the_files_are_the_view_of_the_state_and_carry_its_token() {
    let doc = rich();
    let files = doc.files().unwrap();
    assert_eq!(files.base, doc.base());
    assert_eq!(files.format.as_str(), "stet/1");
    assert!(files.rendered.is_empty());
    assert_eq!(files.files.len(), 1);
    let (name, text) = &files.files[0];
    assert_eq!(name, "document.md");
    assert!(text.starts_with("{stet 1 file=document.md base="), "{text}");
    assert!(
        text.contains("\np4 Payment is due within thirty days.\n"),
        "{text}"
    );
    // The working copy holds the same files, and their hashes are in its manifest.
    let wc = doc.export().unwrap();
    assert_eq!(wc.files, files.files);
    let manifest = wc.manifest().unwrap();
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(manifest.files[0].0, "document.md");
    assert_eq!(manifest.files[0].1, sha256(text.as_bytes()));
    assert_eq!(wc.pending_files().unwrap(), None);
}

#[test]
fn a_view_and_a_lookup_read_the_current_state() {
    let mut doc = rich();
    let view = doc.view(&ViewSpec::default()).unwrap();
    assert_eq!(view.base, doc.base());
    assert!(
        view.text.contains("p4 Payment is due within thirty days."),
        "{}",
        view.text
    );
    let range = ViewSpec {
        scope: Scope::Range {
            from: "p4".into(),
            to: Some("p5".into()),
        },
        mode: Mode::Final,
        ..ViewSpec::default()
    };
    assert_eq!(doc.view(&range).unwrap().position.shown, 2);
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let after = doc.view(&range).unwrap();
    assert_eq!(after.base, doc.base());
    assert!(
        after.text.contains("p4 Payment is due within sixty days."),
        "{}",
        after.text
    );
    // `lookup` of a ref names the block, and its text is what a surface prints.
    let found = doc.lookup("p4").unwrap();
    assert_eq!(found.base, doc.base());
    assert!(!found.text.is_empty());
    assert!(found.text.contains("p4"), "{}", found.text);
    let e = doc.lookup("p999").err().unwrap();
    assert_eq!(e.code(), ErrorCode::NotFound);
}

#[test]
fn an_edit_of_a_line_is_the_replace_it_comes_to_and_the_receipt_says_where_from() {
    let mut files_doc = rich();
    let files = with_line(&files_doc, "within thirty days", "within sixty days");
    let receipt = apply(&mut files_doc, files);
    assert_eq!(
        final_text(&files_doc, "p4"),
        "Payment is due within sixty days."
    );
    assert_eq!(
        text(&files_doc, "p4", Mode::Original),
        "Payment is due within thirty days."
    );
    let op = &receipt.operations[0];
    assert_eq!((op.op.as_str(), op.changed), ("replace", true));
    let origin = op.origin.as_ref().expect("an origin");
    assert_eq!(
        (origin.file.as_str(), origin.block.as_deref()),
        ("document.md", Some("p4"))
    );
    assert!(origin.line > 0);
    assert!(
        receipt
            .text
            .contains(&format!("0 replace p4 (document.md line {}):", origin.line)),
        "{}",
        receipt.text
    );
    // It is the same edit as the operation, byte for byte.
    let mut ops_doc = rich();
    run(
        &mut ops_doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    assert_eq!(
        files_doc.save().unwrap().bytes,
        ops_doc.save().unwrap().bytes
    );
    assert_eq!(files_doc.base(), ops_doc.base());
}

#[test]
fn new_lines_are_paragraphs_and_a_removed_line_is_a_deleted_paragraph() {
    let mut doc = rich();
    let files = with_line(
        &doc,
        "p4 Payment is due within thirty days.\n",
        "p4 Payment is due within thirty days.\nA new paragraph.\nAnother new one.\n",
    );
    let receipt = apply(&mut doc, files);
    let op = &receipt.operations[0];
    assert_eq!(op.op, "insert");
    assert_eq!(op.added.len(), 2);
    assert_eq!(final_text(&doc, &op.added[0]), "A new paragraph.");
    assert_eq!(final_text(&doc, &op.added[1]), "Another new one.");
    assert_eq!(final_text(&doc, "p4"), "Payment is due within thirty days.");
    // A removed line.
    let mut doc = rich();
    let files = with_line(&doc, "p5 This Agreement is governed by English law.\n", "");
    let receipt = apply(&mut doc, files);
    assert_eq!(receipt.operations[0].op, "delete");
    assert_eq!(final_text(&doc, "p5"), "");
    assert_eq!(
        text(&doc, "p5", Mode::Original),
        "This Agreement is governed by English law."
    );
    // A removed table, whole.
    let mut doc = rich();
    let mut files = doc.files().unwrap();
    let before = files.files[0].1.clone();
    let start = before.find("t1 {table").expect("a table");
    let end = before.find("{/table}\n").expect("its end") + "{/table}\n".len();
    files.files[0].1 = format!("{}{}", &before[..start], &before[end..]);
    let receipt = apply(&mut doc, files);
    assert_eq!(receipt.operations[0].op, "delete");
    for cell in ["p6", "p7", "p8", "p9"] {
        assert_eq!(final_text(&doc, cell), "", "{cell}");
    }
}

#[test]
fn a_cut_and_a_join_are_a_split_and_a_merge() {
    let mut doc = rich();
    let files = with_line(
        &doc,
        "p4 Payment is due within thirty days.\n",
        "p4 Payment is due \nwithin thirty days.\n",
    );
    let receipt = apply(&mut doc, files);
    assert_eq!(receipt.operations.len(), 1, "{}", receipt.text);
    assert_eq!(receipt.operations[0].op, "split");
    assert_eq!(final_text(&doc, "p4"), "Payment is due ");
    assert_eq!(
        final_text(&doc, &receipt.operations[0].added[0]),
        "within thirty days."
    );
    let mut doc = rich();
    let files = with_line(
        &doc,
        "p4 Payment is due within thirty days.\np5 This Agreement",
        "p4 Payment is due within thirty days. This Agreement",
    );
    let receipt = apply(&mut doc, files);
    assert_eq!(receipt.operations[0].op, "merge", "{}", receipt.text);
    assert!(
        receipt.operations[0]
            .revisions
            .iter()
            .any(|r| r.kind == ChangeKind::MarkDeleted)
    );
}

#[test]
fn files_that_say_what_the_document_says_come_to_nothing() {
    let mut doc = rich();
    let before = (doc.base(), doc.save().unwrap().bytes);
    let same = doc.files().unwrap();
    let dry = doc
        .edit(Edit::files(same.clone(), stamp()).dry_run())
        .unwrap();
    assert!(dry.dry_run && dry.operations.is_empty() && dry.added_count == 0);
    assert!(
        dry.text.starts_with("dry run: nothing was applied."),
        "{}",
        dry.text
    );
    let receipt = apply(&mut doc, same);
    assert!(receipt.operations.is_empty());
    assert!(receipt.revisions.is_empty() && receipt.blocks.is_empty());
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    assert!(
        receipt.text.starts_with("applied 0 operations"),
        "{}",
        receipt.text
    );
    // And after an edit, the files that were rendered again say what the document says.
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let token = doc.base();
    let same = doc.files().unwrap();
    let receipt = apply(&mut doc, same);
    assert!(receipt.operations.is_empty());
    assert_eq!(doc.base(), token);
}

#[test]
fn files_from_another_version_or_document_are_stale() {
    let mut doc = rich();
    let old = doc.files().unwrap();
    run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    let before = (doc.base(), doc.save().unwrap().bytes);
    let mut edited = old.clone();
    edited.files[0].1 = edited.files[0]
        .1
        .replace("within thirty days", "within ninety days");
    let e = fails_files(&mut doc, edited);
    assert_eq!(e.code(), ErrorCode::StaleFiles);
    assert!(e.message().contains("version"), "{e}");
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    // From another document.
    let other = crate::document::tests::opened().files().unwrap();
    assert_eq!(fails_files(&mut doc, other).code(), ErrorCode::StaleFiles);
    // A format that is not registered.
    let mut odd = doc.files().unwrap();
    odd.format = crate::api::view::FormatId::new("stet/9");
    assert_eq!(fails_files(&mut doc, odd).code(), ErrorCode::InvalidPayload);
    assert_eq!(doc.base(), before.0);
}

#[test]
fn files_written_by_another_release_are_stale_whether_or_not_the_plan_succeeds() {
    // H8. `rendered` holds the hashes the writing release recorded, and a release that renders another text records others.
    let wrong = hex(&sha256(b"what another release wrote"));
    // The plan succeeds: an edit of a line.
    let mut doc = rich();
    let before = doc.base();
    let mut files = with_line(&doc, "within thirty days", "within sixty days");
    files.rendered = vec![("document.md".to_string(), wrong.clone())];
    let e = fails_files(&mut doc, files);
    assert_eq!(e.code(), ErrorCode::StaleFiles);
    assert!(e.message().contains("another release"), "{e}");
    assert_eq!(doc.base(), before);
    // The plan fails: the head is read-only.
    let mut files = doc.files().unwrap();
    files.files[0].1 = files.files[0]
        .1
        .replace("common-style=\"Normal\"", "common-style=\"Heading1\"");
    let plain = fails_files(&mut doc, files.clone());
    assert_ne!(plain.code(), ErrorCode::StaleFiles, "{plain}");
    // With the hashes of this release's render the refusal is the plan's own, and with other hashes it is skew.
    let current = hex(&sha256(doc.files().unwrap().files[0].1.as_bytes()));
    let mut same = files.clone();
    same.rendered = vec![("document.md".to_string(), current)];
    let own = fails_files(&mut doc, same);
    assert_eq!((own.code(), own.message()), (plain.code(), plain.message()));
    files.rendered = vec![("document.md".to_string(), wrong.clone())];
    let e = fails_files(&mut doc, files);
    assert_eq!(e.code(), ErrorCode::StaleFiles, "{e}");
    // The hashes of this release's render are no skew, and neither are none.
    let mut files = with_line(&doc, "within thirty days", "within sixty days");
    let rendered = doc.files().unwrap();
    files.rendered = vec![(
        "document.md".to_string(),
        hex(&sha256(rendered.files[0].1.as_bytes())),
    )];
    apply(&mut doc, files);
    assert_eq!(final_text(&doc, "p4"), "Payment is due within sixty days.");
    // A recorded file that does not render at all is skew too.
    let mut doc = rich();
    let mut files = with_line(&doc, "within thirty days", "within sixty days");
    files.rendered = vec![("notes.md".to_string(), wrong)];
    assert_eq!(fails_files(&mut doc, files).code(), ErrorCode::StaleFiles);
}

#[test]
fn a_hand_edit_in_a_working_copy_goes_through_pending_files() {
    // What the CLI does: export, edit a file in the directory, read the pending files, resume, edit.
    let doc = rich();
    let mut wc = doc.export().unwrap();
    wc.files[0].1 = wc.files[0]
        .1
        .replace("within thirty days", "within sixty days");
    let pending = wc.pending_files().unwrap().expect("one file is pending");
    assert_eq!(pending.rendered.len(), 1);
    let mut resumed = Document::resume(&wc, &OpenOptions::default()).unwrap();
    let receipt = apply(&mut resumed, pending);
    assert_eq!(receipt.operations[0].op, "replace");
    assert_eq!(
        final_text(&resumed, "p4"),
        "Payment is due within sixty days."
    );
    // The unchanged file is not sent, and the edit still applies.
    assert_eq!(resumed.save().unwrap().receipt.revisions.len(), 2);
}

#[test]
fn a_failure_names_the_file_and_the_line_and_never_an_operation() {
    // A scope that holds p4 and an edit that also changes p5.
    let mut doc = rich();
    let files = with_line(
        &doc,
        "within thirty days.\np5 This Agreement is governed by English law.",
        "within sixty days.\np5 This Agreement is governed by Scots law.",
    );
    let before = (doc.base(), doc.save().unwrap().bytes);
    let e = doc
        .edit(Edit::files(files, stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation);
    assert_eq!(e.op_index(), None, "{e}");
    assert_eq!(e.file(), Some("document.md"));
    assert!(e.line().is_some_and(|l| l > 0));
    assert!(
        e.message().starts_with("p5 is outside the scope (p4)."),
        "{e}"
    );
    assert!(e.message().contains("The edit at document.md line"), "{e}");
    assert!(!e.message().contains("Operation"), "{e}");
    assert!(format!("{e}").contains("(document.md line"), "{e}");
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    // Files from before a delete are stale: the document has moved on, and a line of theirs may name what is gone.
    let mut doc = rich();
    let files = doc.files().unwrap();
    run(&mut doc, r#"{"op":"delete","ref":"p4"}"#);
    let mut gone = doc.files().unwrap();
    gone.files[0].1 = files.files[0].1.clone();
    assert_eq!(fails_files(&mut doc, gone).code(), ErrorCode::StaleFiles);
}

#[test]
fn an_edit_that_changes_too_many_blocks_is_refused_with_the_count() {
    let limits = Limits {
        max_batch_ops: 2,
        ..Limits::default()
    };
    let options = OpenOptions {
        limits,
        ..OpenOptions::default()
    };
    let mut doc = Document::open(rich_package(), &options).unwrap();
    let mut files = doc.files().unwrap();
    files.files[0].1 = files.files[0]
        .1
        .replace("thirty days", "sixty days")
        .replace("English law", "Scots law")
        .replace("After the table", "After the chart");
    let e = fails_files(&mut doc, files);
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(
        e.message()
            .contains("changes 3 blocks, over the limit of 2"),
        "{e}"
    );
    assert!(e.next().contains("in parts"), "{}", e.next());
    // Two blocks are at the limit.
    let mut files = doc.files().unwrap();
    files.files[0].1 = files.files[0]
        .1
        .replace("thirty days", "sixty days")
        .replace("English law", "Scots law");
    apply(&mut doc, files);
}

#[test]
fn a_utf16_count_on_the_edit_does_not_change_what_files_mean() {
    // H11: `Edit::files` is not converted. p3 has an emoji, which is two UTF-16 units and one scalar value.
    let mut a = rich();
    let mut b = rich();
    let files = |doc: &Document| {
        with_line(
            doc,
            "then a tab\\tand the end.",
            "then a tab\\tand the finish.",
        )
    };
    let fa = files(&a);
    let fb = files(&b);
    apply(&mut a, fa);
    b.edit(Edit::files(fb, stamp()).offsets(OffsetUnit::Utf16))
        .unwrap();
    assert_eq!(
        final_text(&a, "p3"),
        "Emoji \u{1F600} then a tab\tand the finish."
    );
    assert_eq!(a.save().unwrap().bytes, b.save().unwrap().bytes);
}

#[test]
fn a_dry_run_of_files_is_the_real_edit_without_its_effect() {
    let files = |doc: &Document| {
        with_line(
            doc,
            "p4 Payment is due within thirty days.\n",
            "p4 Payment is due within sixty days.\nA new paragraph.\n",
        )
    };
    let mut dry = rich();
    let mut real = rich();
    let state = (dry.base(), dry.export().unwrap());
    let f = files(&dry);
    let d = dry.edit(Edit::files(f, stamp()).dry_run()).unwrap();
    assert_eq!((dry.base(), dry.export().unwrap()), state);
    let f = files(&real);
    let r = real.edit(Edit::files(f, stamp())).unwrap();
    assert!(d.dry_run && !r.dry_run);
    assert_eq!(d.base, state.0);
    assert_eq!(d.operations.len(), r.operations.len());
    for (x, y) in d.operations.iter().zip(&r.operations) {
        assert_eq!(
            (&x.op, x.changed, &x.origin, &x.blocks),
            (&y.op, y.changed, &y.origin, &y.blocks)
        );
    }
    assert_eq!(d.added, Vec::<String>::new());
    assert_eq!(d.added_count, 1);
    assert_eq!(r.added.len(), 1);
    assert!(
        d.blocks
            .iter()
            .any(|b| b.r#ref.is_none() && b.anchor.as_deref() == Some("p4")),
        "{:?}",
        d.blocks
    );
    assert!(d.warnings.iter().all(|w| w.code != WarningCode::NoChange));
}

#[test]
fn what_the_plan_says_is_in_the_receipt_beside_what_the_edit_says() {
    // Note: a line was removed and its text written again elsewhere, which is a deletion and an insertion and not a move.
    let mut doc = rich();
    let mut files = doc.files().unwrap();
    let text = files.files[0].1.clone();
    files.files[0].1 = text
        .replace("p5 This Agreement is governed by English law.\n", "")
        .replace(
            "p2 \u{201C}Losses\u{201D} means any loss, and the Supplier\u{2019}s liability.\n",
            "p2 \u{201C}Losses\u{201D} means any loss, and the Supplier\u{2019}s liability.\nThis Agreement is governed by English law.\n",
        );
    let dry = doc
        .edit(Edit::files(files.clone(), stamp()).dry_run())
        .unwrap();
    let receipt = apply(&mut doc, files);
    let note = receipt
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::LooksLikeRewrite)
        .expect("a note from the plan");
    assert_eq!(note.block.as_deref(), Some("p5"));
    assert!(
        receipt.text.contains("warning looks_like_rewrite p5:"),
        "{}",
        receipt.text
    );
    assert_eq!(dry.warnings, receipt.warnings);
    // Both operations name the line they came from, in the order of the plan.
    let lines: Vec<u32> = receipt
        .operations
        .iter()
        .filter_map(|o| o.origin.as_ref().map(|x| x.line))
        .collect();
    assert_eq!(lines.len(), 2);
    assert!(lines.iter().all(|l| *l > 0));
    assert_eq!(receipt.operations[0].op, "insert");
    assert_eq!(receipt.operations[1].op, "delete");
}

#[test]
fn a_line_added_after_a_paragraph_with_a_hidden_mark_is_not_hidden() {
    // A new run copies the formatting of the paragraph mark it is modelled on, except w:vanish and w:specVanish
    // (revisions' build_mark_run), so a line added after a paragraph with a hidden mark is visible and no hidden_text
    // warning is needed for it.
    let body = "<w:p><w:pPr><w:rPr><w:vanish/></w:rPr></w:pPr><w:r><w:t>Alpha beta gamma</w:t></w:r></w:p><w:p><w:r><w:t>Second one</w:t></w:r></w:p>";
    let doc = stetdoc_testkit::fixtures::parts::Doc::new()
        .body(body)
        .bytes();
    let bytes = stetdoc_testkit::fixtures::zip::Zip::new(
        stetdoc_testkit::fixtures::parts::entries_doc(&doc),
    )
    .build();
    let mut doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    let files = with_line(
        &doc,
        "p1 Alpha beta gamma\n",
        "p1 Alpha beta gamma\nA new line.\n",
    );
    let receipt = apply(&mut doc, files);
    let added = receipt.operations[0].added[0].clone();
    assert!(
        !receipt.warnings.iter().any(
            |w| w.code == WarningCode::HiddenText && w.block.as_deref() == Some(added.as_str())
        ),
        "a hidden_text warning for {added}:\n{}",
        receipt.text
    );
}

#[test]
fn a_ref_on_two_lines_is_refused_with_the_file_and_the_line_in_their_fields() {
    let mut doc = rich();
    let files = with_line(
        &doc,
        "p5 This Agreement",
        "p4 A second line for p4.\np5 This Agreement",
    );
    let second = files.files[0]
        .1
        .lines()
        .position(|l| l == "p4 A second line for p4.")
        .map(|i| u32::try_from(i + 1).unwrap())
        .unwrap();
    let e = fails_files(&mut doc, files);
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(e.message().contains("p4 is on lines"), "{e}");
    assert_eq!(e.file(), Some("document.md"));
    assert_eq!(e.line(), Some(second), "{e}");
    assert_eq!(e.op_index(), None);
}
