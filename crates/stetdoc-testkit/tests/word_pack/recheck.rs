//! The second recheck in Word for the web (2026-10-10): the files that settle whether the gaps after 17 and 39 are stetdoc's,
//! and the new shape of 26. They go to `~/Desktop/stetdoc-word-recheck2/` (or `STETDOC_WORD_RECHECK_DIR`) with an `r2-` prefix,
//! with a CHECKLIST.md that says which file to compare with which. Run it with
//! `cargo test -p stetdoc-testkit --test word_pack -- --ignored write_the_second_recheck`.

use crate::build::{AUTHOR, DATE, FN_REF, edited, open, p, package, run, table};
use std::path::PathBuf;
use stetdoc_core::{CompareOptions, Document, Limits, Stamp, compare};

fn dir() -> PathBuf {
    if let Ok(d) = std::env::var("STETDOC_WORD_RECHECK_DIR") {
        return PathBuf::from(d);
    }
    PathBuf::from(std::env::var("HOME").expect("HOME is set"))
        .join("Desktop")
        .join("stetdoc-word-recheck2")
}

fn saved(doc: &Document) -> Vec<u8> {
    doc.save().expect("saves").bytes
}

/// The package after stetdoc accepts or rejects every revision of it: a file with no tracked change.
fn resolved(bytes: &[u8], op: &str) -> Vec<u8> {
    let mut doc = open(bytes.to_vec());
    run(
        &mut doc,
        AUTHOR,
        DATE,
        &format!(r#"{{"op":"{op}","all":true}}"#),
        true,
    );
    saved(&doc)
}

fn compared(original: &[u8], revised: &[u8]) -> Vec<u8> {
    let c = compare(
        original.to_vec(),
        revised.to_vec(),
        &CompareOptions {
            limits: Limits::default(),
            stamp: Stamp::new(AUTHOR, DATE),
            format: Default::default(),
        },
    )
    .expect("compare");
    saved(&c.document)
}

/// The old shape of 26: the note's own number left out of the deletion of its text.
fn number_left_live(bytes: &[u8]) -> Vec<u8> {
    let notes =
        stetdoc_testkit::laws::locality::part(bytes, "word/footnotes.xml").expect("footnotes");
    let notes = String::from_utf8(notes).expect("utf-8");
    let run = "<w:r><w:rPr><w:rStyle w:val=\"FootnoteReference\"/></w:rPr><w:footnoteRef/></w:r>";
    let at = notes.find(run).expect("the note's number");
    let del = notes[..at].rfind("<w:del ").expect("the deletion");
    let old = format!(
        "{}{run}{}{}",
        &notes[..del],
        &notes[del..at],
        &notes[at + run.len()..]
    );
    let names: Vec<String> = stetdoc_testkit::laws::locality::entries(bytes)
        .expect("a zip")
        .into_keys()
        .collect();
    let entries: Vec<stetdoc_testkit::fixtures::zip::Entry> = names
        .iter()
        .map(|n| {
            let data = if n == "word/footnotes.xml" {
                old.clone().into_bytes()
            } else {
                stetdoc_testkit::laws::locality::part(bytes, n).expect("an entry")
            };
            stetdoc_testkit::fixtures::zip::Entry::new(n, data)
        })
        .collect();
    stetdoc_testkit::fixtures::zip::Zip::new(entries).build()
}

#[test]
#[ignore = "writes the second recheck files to the desktop; run with --ignored"]
fn write_the_second_recheck() {
    let dir = dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut add = |name: &str, bytes: Vec<u8>| files.push((format!("r2-{name}.docx"), bytes));

    // 17 and 39: the redlines, their two inputs with no tracked change, and stetdoc's own Accept All and Reject All of them.
    let old_table = package(
        &format!(
            "{}{}{}",
            p("Prices."),
            table(&[&["Item", "Price"], &["Apples", "10"]]),
            p("End.")
        ),
        &[],
    );
    let new_17 = package(
        &format!(
            "{}{}{}",
            p("Prices."),
            table(&[
                &["Code", "Name", "Weight"],
                &["X1", "Bolts", "2 kg"],
                &["X2", "Nuts", "1 kg"]
            ]),
            p("End.")
        ),
        &[],
    );
    let new_39 = package(
        &format!(
            "{}{}{}",
            p("Prices."),
            table(&[&["Code", "Name"], &["X1", "Bolts"], &["X2", "Nuts"]]),
            p("End.")
        ),
        &[],
    );
    // 17 the other way round: a three-column table replaced by a two-column one, where the grid has to hold the deleted rows.
    add("17-narrower-redline", compared(&new_17, &old_table));
    for (n, new) in [("17", &new_17), ("39", &new_39)] {
        let red = compared(&old_table, new);
        add(&format!("{n}-redline"), red.clone());
        add(&format!("{n}-control-old"), old_table.clone());
        add(&format!("{n}-control-new"), new.clone());
        add(
            &format!("{n}-accepted-by-stetdoc"),
            resolved(&red, "accept"),
        );
        add(
            &format!("{n}-rejected-by-stetdoc"),
            resolved(&red, "reject"),
        );
    }

    // Rows that Word itself would mark: one deleted or inserted, last in the table or in the middle of it.
    let tbl = || {
        package(
            &format!(
                "{}{}{}",
                p("Before the table."),
                table(&[&["Item", "Price"], &["Apples", "10"], &["Pears", "12"]]),
                p("After the table.")
            ),
            &[],
        )
    };
    let row = |ops: &str| saved(&edited(tbl(), &[(AUTHOR, ops)]));
    add(
        "row-delete-last",
        row(r#"{"op":"row","ref":"r3","action":"delete"}"#),
    );
    add(
        "row-delete-middle",
        row(r#"{"op":"row","ref":"r2","action":"delete"}"#),
    );
    add(
        "row-insert-last",
        row(r#"{"op":"row","ref":"r3","action":"insert","cells":["Plums","14"]}"#),
    );
    add(
        "row-insert-middle",
        row(r#"{"op":"row","ref":"r2","action":"insert","cells":["Plums","14"]}"#),
    );

    // 26: the note deleted with its reference, now with its own number inside the deletion as Word writes it (RP050), and the
    // old shape with the number left live.
    let noted = format!(
        "{}<w:p><w:r><w:t>This Agreement is governed by English law.</w:t></w:r>{FN_REF}</w:p>{}",
        p("The Supplier shall deliver the goods."),
        p("End.")
    );
    let deleted = saved(&edited(
        package(&noted, &[]),
        &[(AUTHOR, r#"{"op":"delete-note","note":"fn1"}"#)],
    ));
    add("26-footnote-delete", deleted.clone());
    add("26-old-number-left-live", number_left_live(&deleted));
    // The same edit in a package with the settings part that Word always writes, which names the separator notes.
    let settings = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:settings xmlns:w=\"{}\"><w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr></w:settings>",
        "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
    );
    add(
        "26-footnote-delete-with-settings",
        saved(&edited(
            package(
                &noted,
                &[("settings.xml", "settings", "settings", settings)],
            ),
            &[(AUTHOR, r#"{"op":"delete-note","note":"fn1"}"#)],
        )),
    );

    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    std::fs::write(dir.join("CHECKLIST.md"), CHECKLIST).unwrap();
    eprintln!(
        "wrote {} files and CHECKLIST.md to {}",
        files.len(),
        dir.display()
    );
}

const CHECKLIST: &str = "# Second recheck in Word for the web (r2)

Open each file as before (Review, All Markup; Accept All, Undo, Reject All). Where a gap shows after Accept All or Reject All,
reload the page (or close the file and open it again) and note whether the gap is still there.

## 17: a table replaced by one of another grid, now one table

| File | What to do | Compare with |
|---|---|---|
| `r2-17-control-old.docx`, `r2-17-control-new.docx` | open, no tracked changes | each other: the normal space after a table |
| `r2-17-redline.docx` | the review shows one table: two deleted rows of two cells, then three inserted rows of three. Accept All; Undo; Reject All | after Accept All with `r2-17-control-new`, after Reject All with `r2-17-control-old` |
| `r2-17-accepted-by-stetdoc.docx`, `r2-17-rejected-by-stetdoc.docx` | open: stetdoc's own Accept All and Reject All | `r2-17-control-new` and `r2-17-control-old`: any difference here is stetdoc's |
| `r2-17-narrower-redline.docx` | the other way round (three columns replaced by two): one table, every cell of the deleted rows shown. Accept All; Undo; Reject All | after Accept All with `r2-17-control-old`, after Reject All with `r2-17-control-new` |
| `r2-39-*` | unchanged from the last check (one table, same grid) | as before |
| `r2-row-*` | unchanged from the last check | as before |

The deleted rows of `r2-17-redline` end short of the three-column grid, with no `w:gridAfter` (LibreOffice draws one as a
border past the row), as Word leaves rows when it rejects inserted cells. A gap below the table after Reject All that the
rejected-by-stetdoc file does not have is Word for the web keeping removed rows' height, as in 39.

## 26: a note deleted with its reference

| File | What to do | Expected |
|---|---|---|
| `r2-RP050-word.docx` | Word's own file (Open-Xml-PowerTools `RP050`): \"Video\" and the reference deleted. Look at the reference in the review; Accept All, then look at the footnotes; Undo, Reject All | tells whether Word for the web shows a deleted reference as deleted and removes the note on Accept All in Word's own file |
| `r2-word-revision-footnotes.docx` | Word 2019 (Aspose's `Revision footnotes.docx`): a space and the reference deleted, the note's number inside the deletion, as stetdoc writes it; many other changes too. Footnote 4 | as above |
| `r2-word-tdf141660.docx`, `r2-word-fdo65632.docx` | Word files from LibreOffice's tests: a deleted reference with its note deleted (tdf141660) and with its note left live (fdo65632) | as above |
| `r2-26-footnote-delete.docx` | stetdoc's: the reference alone deleted, the note's number and text deleted, as in Word's files | Accept All: no footnote. Reject All: footnote 1 reads Scots law is excluded. |
| `r2-26-footnote-delete-with-settings.docx` | the same, in a package with the settings part Word always writes (`w:footnotePr` naming the separator notes); the files above all have one, the plain one does not | as above |
| `r2-26-old-number-left-live.docx` | the shape before: the note's number left live | for contrast |
";
