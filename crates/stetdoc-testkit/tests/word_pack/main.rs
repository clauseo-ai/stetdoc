//! The hand-check pack for the Word check: about 40 redlines that a person opens in Word for the web, with a checklist.
//!
//! Every file is built with stetdoc's own interface on a document from the fixture builder, so the pack holds no third-party
//! text. It is written to `~/Desktop/stetdoc-word-check/` (or `STETDOC_WORD_PACK_DIR`) and never committed. Run it with
//! `cargo test -p stetdoc-testkit --test word_pack -- --ignored`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod build;
mod more;
mod recheck;

use build::{
    AUTHOR, DATE, FN_REF, OTHER, Tap, cell, edited, edited_files, item, open, out_dir, p, package,
    resolved, run, styled, table,
};
use std::fmt::Write as _;
use stetdoc_core::{CompareOptions, Document, Limits, Stamp, compare};
use stetdoc_testkit::fixtures::parts::{DECL, NS_W};
use stetdoc_testkit::fixtures::word;

pub(crate) struct Case {
    name: String,
    tests: String,
    author: String,
    doc: Document,
    note: String,
}

pub(crate) fn case(name: &str, tests: &str, author: &str, doc: Document) -> Case {
    Case {
        name: name.into(),
        tests: tests.into(),
        author: author.into(),
        doc,
        note: String::new(),
    }
}

impl Case {
    pub(crate) fn note(mut self, note: &str) -> Case {
        self.note = note.into();
        self
    }
}

pub(crate) fn a(ops: &str) -> (&str, &str) {
    (AUTHOR, ops)
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let three = || {
        format!(
            "{}{}{}",
            p("The Supplier shall deliver the goods."),
            p("Payment is due within thirty days."),
            p("This Agreement is governed by English law.")
        )
    };

    // Text.
    out.push(case("01-insert-text", "tracked insertion inside a paragraph", AUTHOR,
        edited(package(&three(), &[]), &[a(r#"{"op":"replace","ref":"p1","find":"the goods","with":"the goods and the software"}"#)])));
    out.push(case(
        "02-delete-text-edge-spaces",
        "tracked deletions, one starting and one ending with a space (xml:space preserve on w:delText)",
        AUTHOR,
        edited(
            package(&three(), &[]),
            &[
                a(r#"{"op":"delete","ref":"p2","find":" within thirty days"}"#),
                a(r#"{"op":"delete","ref":"p3","find":"governed "}"#),
            ],
        ),
    ));
    out.push(case(
        "03-replace-text",
        "tracked replacement, a deletion next to an insertion",
        AUTHOR,
        edited(
            package(&three(), &[]),
            &[a(
                r#"{"op":"replace","ref":"p2","find":"thirty","with":"sixty"}"#,
            )],
        ),
    ));
    out.push(case("04-insert-paragraphs-before-after", "inserted paragraphs before a paragraph, and after the last paragraph of the body in Word's Enter shape (C9, C11)", AUTHOR,
        edited(package(&three(), &[]), &[a(r#"{"op":"insert","ref":"p2","place":"before","paragraphs":["A paragraph inserted before."]}"#), a(r#"{"op":"insert","ref":"p3","place":"after","paragraphs":["A last paragraph inserted after the body."]}"#)])));
    out.push(case(
        "05-delete-paragraph",
        "a whole paragraph deleted, and the last paragraph of the body deleted (C3)",
        AUTHOR,
        edited(
            package(&three(), &[]),
            &[
                a(r#"{"op":"delete","ref":"p2"}"#),
                a(r#"{"op":"delete","ref":"p3"}"#),
            ],
        ),
    ));

    // Formatting.
    out.push(case("06-format-run", "a run formatting change (bold, size, colour) as rPrChange", AUTHOR,
        edited(package(&three(), &[]), &[a(r#"{"op":"format","ref":"p2","find":"thirty days","bold":true,"size":"14pt","color":"C00000"}"#)])).note("Accept All: thirty days stays bold, 14 pt, red. Reject All: plain again"));
    out.push(case("07-paragraph-properties", "a paragraph property change (alignment, indent, spacing) as pPrChange", AUTHOR,
        edited(package(&three(), &[]), &[a(r#"{"op":"paragraph","ref":"p2","align":"center","indent_left":"0.5in","space_after":"12pt"}"#)])).note("Accept All: paragraph 2 centred, indented, space after. Reject All: left aligned"));
    out.push(
        case(
            "08-style-change",
            "a paragraph style change from Normal to Heading 1",
            AUTHOR,
            edited(
                package(&three(), &[]),
                &[a(r#"{"op":"style","ref":"p1","style":"Heading 1"}"#)],
            ),
        )
        .note("Accept All: paragraph 1 is Heading 1. Reject All: Normal"),
    );

    // Split and merge.
    out.push(case(
        "09-split",
        "a paragraph split in two (inserted paragraph mark, C9)",
        AUTHOR,
        edited(
            package(&three(), &[]),
            &[a(r#"{"op":"split","ref":"p2","before":"within"}"#)],
        ),
    ));
    out.push(case(
        "10-merge",
        "two paragraphs merged (deleted paragraph mark)",
        AUTHOR,
        edited(
            package(&three(), &[]),
            &[a(r#"{"op":"merge","ref":"p2","second":"p3"}"#)],
        ),
    ));
    let heading = format!(
        "{}{}{}",
        styled("Heading1", "Delivery"),
        p("The Supplier shall deliver the goods."),
        p("Payment is due within thirty days.")
    );
    out.push(case("11-merge-heading-normal", "merge of a Heading 1 paragraph and a Normal paragraph, with the alignment pPrChange (C1, C10)", AUTHOR,
        edited(package(&heading, &[]), &[a(r#"{"op":"merge","ref":"p1","second":"p2"}"#)])).note("record which paragraph style the merged paragraph keeps after Accept All (C1)"));
    out.push(case(
        "12-split-heading-restyle",
        "a Heading 1 split, the second half set to Normal (C2)",
        AUTHOR,
        edited(
            package(
                &format!(
                    "{}{}",
                    styled("Heading1", "Delivery and acceptance of the goods"),
                    p("Body text.")
                ),
                &[],
            ),
            &[
                a(r#"{"op":"split","ref":"p1","before":"acceptance"}"#),
                a(r#"{"op":"style","ref":"p2","style":"Normal"}"#),
            ],
        ),
    ));

    // Tables.
    let tbl = format!(
        "{}{}{}",
        p("Before the table."),
        table(&[&["Item", "Price"], &["Apples", "10"], &["Pears", "12"]]),
        p("After the table.")
    );
    out.push(case(
        "13-table-row-insert",
        "a tracked row insertion (C15)",
        AUTHOR,
        edited(
            package(&tbl, &[]),
            &[a(
                r#"{"op":"row","ref":"r2","action":"insert","cells":["Plums","14"]}"#,
            )],
        ),
    ));
    out.push(case(
        "14-table-row-delete",
        "a tracked row deletion (C15)",
        AUTHOR,
        edited(
            package(&tbl, &[]),
            &[a(r#"{"op":"row","ref":"r3","action":"delete"}"#)],
        ),
    ));
    out.push(case("15-table-cell-edits", "text replaced and inserted inside table cells, and a paragraph after the last paragraph of a cell in Word's Enter shape: the 12 paragraph's mark is inserted, the new one keeps the cell's (C11)", AUTHOR,
        edited(package(&tbl, &[]), &[a(r#"{"op":"replace","ref":"p5","find":"10","with":"11"}"#), a(r#"{"op":"replace","ref":"p6","with":"Green pears"}"#), a(r#"{"op":"insert","ref":"p7","place":"after","paragraphs":["Price per kilo."]}"#)])));
    out.push(case(
        "16-table-delete",
        "a whole table deleted, and the paragraph before it deleted (C3, before a table)",
        AUTHOR,
        edited(
            package(&tbl, &[]),
            &[
                a(r#"{"op":"delete","ref":"t1"}"#),
                a(r#"{"op":"delete","ref":"p1"}"#),
            ],
        ),
    ));
    {
        let original = package(
            &format!(
                "{}{}{}",
                p("Prices."),
                table(&[&["Item", "Price"], &["Apples", "10"]]),
                p("End.")
            ),
            &[],
        );
        let revised = package(
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
        let c = compare(
            original,
            revised,
            &CompareOptions {
                limits: Limits::default(),
                stamp: Stamp::new(AUTHOR, DATE),
                format: Default::default(),
            },
        )
        .expect("compare");
        out.push(
            case(
                "17-compare-table-replaced",
                "a compare redline where a table is replaced by one of another grid: one table, the deleted rows of two cells ending short of the three-column grid, then the inserted rows",
                AUTHOR,
                c.document,
            )
            .note("One table in the review. After Accept All the three-column table, after Reject All the two-column one, each with no gap before End. beyond what Word for the web keeps of removed rows until the file is opened again (39, 14)"),
        );
    }

    // Comments.
    let base = || package(&three(), &[]);
    out.push(case("18-comment-add", "a new comment on a range, with a new comments part, commentsExtended without commentsIds, people.xml and the annotation styles (comment items 1, 5, 10)", AUTHOR,
        edited(base(), &[a(r#"{"op":"comment","ref":"p2","find":"thirty days","text":"Is thirty days enough?"}"#)])));
    out.push(case(
        "19-comment-reply",
        "a reply to a comment, shown as one thread",
        AUTHOR,
        edited(
            base(),
            &[
                (
                    OTHER,
                    r#"{"op":"comment","ref":"p2","find":"thirty","text":"Too short."}"#,
                ),
                a(r#"{"op":"reply","comment":"c0","text":"Agreed, sixty."}"#),
            ],
        ),
    ));
    out.push(case("20-comment-resolved-then-reply", "a resolved thread, then a reply added to it with done=1 (comment item 2: record whether Word reopens it)", AUTHOR,
        edited(base(), &[a(r#"{"op":"comment","ref":"p1","text":"Check the goods."}"#), a(r#"{"op":"resolve","comment":"c0"}"#), a(r#"{"op":"reply","comment":"c0","text":"A late reply."}"#)])).note("record whether Word shows the thread resolved or reopened"));
    out.push(case(
        "21-comment-delete-last",
        "the only comment deleted, leaving an empty comments part (comment item 3)",
        AUTHOR,
        edited(
            base(),
            &[a(r#"{"op":"comment","ref":"p1","text":"Temporary."}"#)],
        )
        .tap(|d| {
            run(
                d,
                AUTHOR,
                DATE,
                r#"{"op":"delete-comment","comment":"c0"}"#,
                true,
            )
        }),
    ));
    let runs = format!(
        "{}{}",
        "<w:p><w:r><w:t xml:space=\"preserve\">The </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Supplier shall</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t xml:space=\"preserve\"> deliver the goods.</w:t></w:r></w:p>",
        p("Payment is due within thirty days.")
    );
    out.push(case("22-comment-across-runs-point-twopara", "a comment over a range across three runs, a point comment (empty range), and a two-paragraph comment (comment items 7, 12)", AUTHOR,
        edited(package(&runs, &[]), &[a(r#"{"op":"comment","ref":"p1","find":"The Supplier shall deliver","text":"Range across runs."}"#), a(r#"{"op":"comment","ref":"p2","start":7,"end":7,"text":"A point comment."}"#), a(r#"{"op":"comment","ref":"p2","find":"thirty","text":"First paragraph of the comment.\nSecond paragraph of the comment."}"#)])).note("three comments: the first covers The Supplier shall deliver; the second is a point after Payment; the third has two paragraphs"));
    out.push(case(
        "23-comment-edited",
        "a comment whose text stetdoc rewrote (comment item 11)",
        AUTHOR,
        edited(
            base(),
            &[a(r#"{"op":"comment","ref":"p3","text":"First wording."}"#)],
        )
        .tap(|d| {
            run(
                d,
                AUTHOR,
                DATE,
                r#"{"op":"edit-comment","comment":"c0","text":"Second wording."}"#,
                true,
            )
        }),
    ));
    {
        // A LibreOffice-style comments part: no w14:paraId, no mc:Ignorable, no commentsExtended.
        let body = format!(
            "<w:p><w:commentRangeStart w:id=\"0\"/><w:r><w:t>The Supplier shall deliver the goods.</w:t></w:r><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>{}",
            p("Payment is due within thirty days.")
        );
        let comments = format!(
            "{DECL}<w:comments xmlns:w=\"{NS_W}\"><w:comment w:id=\"0\" w:author=\"LibreOffice User\" w:date=\"2026-01-02T10:00:00Z\" w:initials=\"LU\"><w:p><w:r><w:t>A comment written without paraId.</w:t></w:r></w:p></w:comment></w:comments>"
        );
        let bytes = package(&body, &[("comments.xml", "comments", "comments", comments)]);
        out.push(case("24-comment-reply-to-libreoffice", "a reply to a comment that has no paraId, so comments.xml gains w14:paraId (comment item 4)", AUTHOR,
            edited(bytes, &[a(r#"{"op":"reply","comment":"c0","text":"A reply to a LibreOffice comment."}"#)])));
    }
    out.push(case(
        "25-comment-strict-document",
        "a comment added to an ISO Strict document (comment item 6)",
        AUTHOR,
        edited(
            word::smart_tag(true),
            &[a(
                r#"{"op":"comment","ref":"p1","text":"A comment in a Strict file."}"#,
            )],
        ),
    ));

    // Notes, lists, moves.
    let noted = format!(
        "{}<w:p><w:r><w:t>This Agreement is governed by English law.</w:t></w:r>{FN_REF}</w:p>{}",
        p("The Supplier shall deliver the goods."),
        p("End.")
    );
    out.push(
        case(
            "26-footnote-delete",
            "a deleted footnote with its reference (RP050 shape, comment item 13, C14)",
            AUTHOR,
            edited(
                package(&noted, &[]),
                &[a(r#"{"op":"delete-note","note":"fn1"}"#)],
            ),
        )
        .note("Accept All: footnote 1 is gone. Reject All: footnote 1 reads Scots law is excluded"),
    );
    out.push(
        case(
            "27-footnote-text-edit",
            "an edit to the text of a footnote",
            AUTHOR,
            edited_files(
                package(&noted, &[]),
                "Scots law is excluded.",
                "Scots and Irish law are excluded.",
            ),
        )
        .note("footnote 1 reads Scots and Irish law are excluded after Accept All, and Scots law is excluded after Reject All"),
    );
    let list = format!(
        "{}{}{}{}{}",
        p("The Supplier shall:"),
        item(0, "deliver the goods;"),
        item(0, "install them;"),
        item(0, "train the staff."),
        p("End.")
    );
    out.push(case("28-list-items-numbering", "list items: one demoted to level 2, one restarted at 5, and text inserted in a numbered item", AUTHOR,
        edited(package(&list, &[]), &[a(r#"{"op":"list","ref":"p3","level":1}"#), a(r#"{"op":"list","ref":"p4","restart":5}"#), a(r#"{"op":"replace","ref":"p2","find":"goods","with":"goods and spares"}"#)]))
        .note("numbering after Accept All: 1. (a) 5.; after Reject All: 1. 2. 3."));
    {
        // Word-written move markup: the first paragraph moved after the second. stetdoc then edits the moved text.
        let body = concat!(
            "<w:moveFromRangeStart w:id=\"1\" w:author=\"Other Author\" w:date=\"2026-10-01T09:00:00Z\" w:name=\"move1\"/>",
            "<w:p><w:pPr><w:rPr><w:moveFrom w:id=\"2\" w:author=\"Other Author\" w:date=\"2026-10-01T09:00:00Z\"/></w:rPr></w:pPr>",
            "<w:moveFrom w:id=\"3\" w:author=\"Other Author\" w:date=\"2026-10-01T09:00:00Z\"><w:r><w:t>Notices must be in writing.</w:t></w:r></w:moveFrom></w:p>",
            "<w:moveFromRangeEnd w:id=\"1\"/>",
            "<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p>",
            "<w:moveToRangeStart w:id=\"4\" w:author=\"Other Author\" w:date=\"2026-10-01T09:00:00Z\" w:name=\"move1\"/>",
            "<w:p><w:pPr><w:rPr><w:moveTo w:id=\"5\" w:author=\"Other Author\" w:date=\"2026-10-01T09:00:00Z\"/></w:rPr></w:pPr>",
            "<w:moveTo w:id=\"6\" w:author=\"Other Author\" w:date=\"2026-10-01T09:00:00Z\"><w:r><w:t>Notices must be in writing.</w:t></w:r></w:moveTo></w:p>",
            "<w:moveToRangeEnd w:id=\"4\"/>",
            "<w:p><w:r><w:t>End.</w:t></w:r></w:p>",
        );
        let bytes = package(body, &[]);
        let to = "p3";
        out.push(case("29-move-with-edit", "a Word move (moveFrom, moveTo by Other Author) with stetdoc text inserted inside the moved-to text (C8)", "Other Author and Stet Reviewer",
            edited(bytes, &[a(&format!(r#"{{"op":"replace","ref":"{to}","find":"writing","with":"writing and signed"}}"#))])));
    }

    // Authors and dates.
    out.push(case("30-other-author-nested", "author A inserts, author B inserts in the middle and deletes part of A's insertion (C6, C7)", "Other Author and Stet Reviewer",
        edited(base(), &[(OTHER, r#"{"op":"replace","ref":"p2","find":"thirty days","with":"thirty days of the invoice date"}"#), a(r#"{"op":"replace","ref":"p2","find":"invoice","with":"undisputed invoice"}"#), a(r#"{"op":"delete","ref":"p2","find":" date"}"#)])));
    {
        let mut doc = open(base());
        run(
            &mut doc,
            AUTHOR,
            "2026-10-06T14:00:00",
            r#"{"op":"replace","ref":"p1","find":"goods","with":"products"}"#,
            true,
        );
        run(
            &mut doc,
            OTHER,
            "2026-10-06T14:03:27",
            r#"{"op":"replace","ref":"p3","find":"English","with":"Welsh"}"#,
            true,
        );
        out.push(
            case(
                "31-dates-with-and-without-seconds",
                "two stamps, one on the minute and one with seconds (C4)",
                "Stet Reviewer and Other Author",
                doc,
            )
            .note("Review pane dates: 6 Oct 2026 14:00 and 14:03"),
        );
    }

    {
        let body = format!(
            "{}<w:p><w:r><w:t xml:space=\"preserve\">See page </w:t></w:r><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r><w:r><w:t>.</w:t></w:r></w:p>{}",
            p("The Supplier shall deliver the goods."),
            p("End.")
        );
        out.push(case("32-field-deleted", "a paragraph holding a PAGE field deleted; on Reject All the field code survives (C13)", AUTHOR,
            edited(package(&body, &[]), &[a(r#"{"op":"delete","ref":"p2"}"#)]))
            .note("Reject All reads See page 1. (a PAGE field; the view omits its result)"));
    }

    // Compare, accept all, reject all, untracked.
    {
        let original = package(&three(), &[]);
        let revised = package(
            &format!(
                "{}{}{}",
                p("The Supplier shall deliver the goods on time."),
                p("A new clause on warranties."),
                p("This Agreement is governed by Scots law.")
            ),
            &[],
        );
        let c = compare(
            original,
            revised,
            &CompareOptions {
                limits: Limits::default(),
                stamp: Stamp::new(AUTHOR, DATE),
                format: Default::default(),
            },
        )
        .expect("compare");
        out.push(case(
            "33-compare-two-versions",
            "a compare redline of two versions of a document",
            AUTHOR,
            c.document,
        ));
    }
    let busy = || {
        edited(
            base(),
            &[
                a(r#"{"op":"replace","ref":"p2","find":"thirty","with":"sixty"}"#),
                a(
                    r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["Risk passes on delivery."]}"#,
                ),
                a(r#"{"op":"delete","ref":"p3"}"#),
            ],
        )
    };
    out.push(
        case(
            "34-accept-all-output",
            "the output of stetdoc's accept all on a redline: no revisions remain",
            "(none)",
            busy().tap(|d| run(d, AUTHOR, DATE, r#"{"op":"accept","all":true}"#, true)),
        )
        .note("Review shows no changes; text is the accepted text"),
    );
    out.push(
        case(
            "35-reject-all-output",
            "the output of stetdoc's reject all on a redline: no revisions remain",
            "(none)",
            busy().tap(|d| run(d, AUTHOR, DATE, r#"{"op":"reject","all":true}"#, true)),
        )
        .note("Review shows no changes; text is the original text"),
    );
    {
        let mut doc = open(base());
        run(
            &mut doc,
            AUTHOR,
            DATE,
            r#"{"op":"replace","ref":"p2","find":"thirty","with":"forty-five"}"#,
            false,
        );
        out.push(
            case(
                "36-untracked-edit",
                "an untracked edit: the text changes with no revision",
                "(none)",
                doc,
            )
            .note("Review shows no changes"),
        );
    }
    more::cases(&mut out);
    out
}

/// Edits a story through the readable files: replaces one string in every file that holds it.
#[test]
#[ignore = "writes the Word hand-check pack to the desktop; run with --ignored"]
fn write_the_word_hand_check_pack() {
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let mut md = String::from("# stetdoc hand check in Word for the web\n\n");
    md.push_str("To open a file: go to onedrive.live.com, upload the folder, click a file, and it opens in Word for the web.\n");
    md.push_str("Then pick the Review tab: Tracking, then All Markup, shows the changes, and Accept or Reject, then Accept All Changes or Reject All Changes, resolves them (use Undo, or reopen the file, between the two).\n\n");
    md.push_str("Tick each box. Opens: no error and no repair message. Review: the changes show, by the author named. Accept All and Reject All: the body text reads as given (paragraphs separated by /).\n\n");
    md.push_str("| File | Tests | Opens | Review shows author | After Accept All | Ok | After Reject All | Ok | Notes |\n");
    md.push_str("|---|---|---|---|---|---|---|---|---|\n");
    let mut count = 0;
    let mut all = cases();
    all.sort_by(|x, y| x.name.cmp(&y.name));
    for c in all {
        let saved = c
            .doc
            .save()
            .unwrap_or_else(|e| panic!("{} does not save: {e}", c.name));
        let name = format!("{}.docx", c.name);
        std::fs::write(dir.join(&name), &saved.bytes).unwrap();
        let accepted = resolved(&saved.bytes, "accept");
        let rejected = resolved(&saved.bytes, "reject");
        let _ = writeln!(
            md,
            "| `{name}` | {} | [ ] | [ ] {} | {} | [ ] | {} | [ ] | {} |",
            cell(&c.tests),
            cell(&c.author),
            cell(&accepted),
            cell(&rejected),
            cell(&c.note)
        );
        count += 1;
    }
    md.push_str("\nRecord any repair message, a change shown by the wrong author, or text that differs, in the Notes column with the file name.\n");
    std::fs::write(dir.join("CHECKLIST.md"), md).unwrap();
    eprintln!("wrote {count} files and CHECKLIST.md to {}", dir.display());
}
