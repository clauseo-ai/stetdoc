//! The cases of the pack about how Word reads a shape: what a file holds and Word does not show, and the shapes stetdoc wrote
//! before Word's checks settled them (kept as `-old` files, for contrast with the shapes it writes now).

use crate::build::{AUTHOR, DATE, edited, p, package, table};
use crate::{Case, a, case};
use stetdoc_core::{CompareOptions, Limits, Stamp, compare};

/// The revision attributes of the old shapes, as stetdoc wrote them.
fn rev(id: u32) -> String {
    format!(r#"w:id="{id}" w:author="{AUTHOR}" w:date="{DATE}Z""#)
}

/// A table of the pack's frame whose rows are all deleted (`del`) or all inserted (`ins`), each cell's paragraph with its mark
/// and its content tracked, as stetdoc's compare wrote a replaced table.
fn tracked_table(rows: &[&[&str]], kind: &str, id: &mut u32) -> String {
    let frame = table(rows);
    let head = frame.split("<w:tr>").next().unwrap_or_default().to_string();
    let mut s = head;
    for row in rows {
        let mut take = || {
            *id += 1;
            rev(*id)
        };
        s.push_str(&format!("<w:tr><w:trPr><w:{kind} {}/></w:trPr>", take()));
        for text in *row {
            let (mark, wrap) = (take(), take());
            let content = if kind == "del" {
                format!(
                    r#"<w:del {wrap}><w:r><w:delText xml:space="preserve">{text}</w:delText></w:r></w:del>"#
                )
            } else {
                format!(
                    r#"<w:ins {wrap}><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:ins>"#
                )
            };
            s.push_str(&format!(
                r#"<w:tc><w:tcPr><w:tcW w:w="2400" w:type="dxa"/></w:tcPr><w:p><w:pPr><w:rPr><w:{kind} {mark}/></w:rPr></w:pPr>{content}</w:p></w:tc>"#
            ));
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
    s
}

fn compared(original: &str, revised: &str) -> stetdoc_core::Document {
    compare(
        package(original, &[]),
        package(revised, &[]),
        &CompareOptions {
            limits: Limits::default(),
            stamp: Stamp::new(AUTHOR, DATE),
            format: Default::default(),
        },
    )
    .expect("compare")
    .document
}

pub(crate) fn cases(out: &mut Vec<Case>) {
    // How Word reads what the file holds and does not show (the LibreOffice oracle's findings, settled for Word).
    {
        let body = format!(
            "<w:p><w:r><w:t>Unshown </w:t></w:r><w:r><w:t>space</w:t></w:r></w:p><w:p><w:r><w:t>one two </w:t></w:r><w:r><w:t>three</w:t></w:r></w:p>{}",
            p("This Agreement is governed by English law.")
        );
        out.push(
            case(
                "37-white-space-without-preserve",
                "white space at the ends of a w:t without xml:space=\"preserve\", which Word shows, and a tracked edit beside it that keeps the space",
                AUTHOR,
                edited(
                    package(&body, &[]),
                    &[a(r#"{"op":"replace","ref":"p2","find":"two","with":"four"}"#)],
                ),
            )
            .note("Word showed Unshown space with the space (2026-10-10), and stetdoc reads it so now: the first line must read Unshown space, and the second one four three, with a space before three, in every view"),
        );
    }
    {
        let body = crate::build::merged_table();
        out.push(
            case(
                "38-merged-cell-continuation-text",
                "text in a vertically merged continuation cell (w:vMerge without restart), and a tracked edit beside it",
                AUTHOR,
                edited(
                    package(&body, &[]),
                    &[a(r#"{"op":"replace","ref":"p2","find":"Right","with":"Right edited"}"#)],
                ),
            )
            .note("Word should show Merged alone in the merged cell, and not Continuation text: stetdoc flags that text hidden. If Word shows it, stetdoc's reading is wrong. The texts in this row list it, as the file holds it"),
        );
    }
    // The shapes before Word's check of 2026-10-09, which Word resolved otherwise than stetdoc did.
    {
        let old = |x: &str| format!(r#"<w:p><w:r><w:t xml:space="preserve">{x}</w:t></w:r></w:p>"#);
        let body = format!(
            "{}{}{}",
            p("Before the table."),
            table(&[&["Item", "Price"], &["Apples", "10"], &["Pears", "12"]]),
            p("After the table.")
        )
        .replace(&old("10"), &format!(r#"<w:p><w:del {}><w:r><w:delText xml:space="preserve">10</w:delText></w:r></w:del><w:ins {}><w:r><w:t>11</w:t></w:r></w:ins></w:p>"#, rev(1), rev(2)))
        .replace(&old("Pears"), &format!(r#"<w:p><w:del {}><w:r><w:delText xml:space="preserve">Pears</w:delText></w:r></w:del><w:ins {}><w:r><w:t>Green pears</w:t></w:r></w:ins></w:p>"#, rev(3), rev(4)))
        .replace(&old("12"), &format!(r#"{}<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}><w:r><w:t>Price per kilo.</w:t></w:r></w:ins></w:p>"#, old("12"), rev(5), rev(6)));
        out.push(
            case(
                "15-old-table-cell-edits",
                "the old shape of 15: the paragraph after the last paragraph of a cell has an inserted mark of its own (C11)",
                AUTHOR,
                crate::build::open(package(&body, &[])),
            )
            .note("For contrast with 15. Word for the web left the cell an empty paragraph after Reject All, so the 12 cell stays two lines tall; 15 must not"),
        );
    }
    {
        let mut id = 0;
        let body = format!(
            "{}{}{}{}",
            p("Prices."),
            tracked_table(&[&["Item", "Price"], &["Apples", "10"]], "del", &mut id),
            tracked_table(
                &[
                    &["Code", "Name", "Weight"],
                    &["X1", "Bolts", "2 kg"],
                    &["X2", "Nuts", "1 kg"]
                ],
                "ins",
                &mut id
            ),
            p("End.")
        );
        out.push(
            case(
                "17-old-compare-table-replaced",
                "the old shape of 17: a deleted table directly against an inserted table, with nothing between them",
                AUTHOR,
                crate::build::open(package(&body, &[])),
            )
            .note("For contrast with 17. Word for the web showed untracked space between the tables that stayed after Accept All and after Reject All; 17 must show no gap either way"),
        );
    }
    out.push(
        case(
            "39-compare-table-replaced-same-grid",
            "a compare redline where a table is replaced by one with the same properties and grid: one table, its old rows deleted and its new rows inserted",
            AUTHOR,
            compared(
                &format!("{}{}{}", p("Prices."), table(&[&["Item", "Price"], &["Apples", "10"]]), p("End.")),
                &format!("{}{}{}", p("Prices."), table(&[&["Code", "Name"], &["X1", "Bolts"], &["X2", "Nuts"]]), p("End.")),
            ),
        )
        .note("No gap after the table after Accept All or Reject All"),
    );
}
