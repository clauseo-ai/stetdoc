//! The shapes the redline writes so that Word resolves it as stetdoc does (design `compare.md` 4.5): an inserted run
//! of paragraphs at the end of a container gives the live paragraph before it the inserted mark, and a table replaced by another
//! is one table with it, on one grid. Each fixture is held to the compare laws (`check_pair`).

use super::tests_laws::{Verdict, check_pair};
use super::tests_pkg::Pk;

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

fn run(a: &Pk, b: &Pk) -> (Verdict, String) {
    let v = check_pair(a.bytes(), b.bytes()).expect("compares");
    assert!(v.failures.is_empty(), "{}", v.failures.join("\n"));
    assert_eq!(v.receipt.broken, 0);
    let view = v.redline.parts();
    let id = view.id("word/document.xml").expect("a body");
    let xml = String::from_utf8_lossy(&view.tree(id).expect("a tree").serialize()).into_owned();
    (v, xml)
}

fn long(n: usize) -> String {
    format!(
        "Clause {n} says that the supplier shall deliver the goods within thirty days of the order and keep records of it"
    )
}

fn table(cols: usize, rows: &[&[String]]) -> String {
    let grid: String = (0..cols).map(|_| "<w:gridCol w:w=\"2400\"/>").collect();
    let rows: String = rows
        .iter()
        .map(|cells| {
            let cells: String = cells
                .iter()
                .map(|c| format!("<w:tc><w:tcPr/>{}</w:tc>", p(c)))
                .collect();
            format!("<w:tr>{cells}</w:tr>")
        })
        .collect();
    format!("<w:tbl><w:tblPr/><w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl>")
}

#[test]
fn paragraphs_appended_to_the_body_give_the_last_live_paragraph_the_inserted_mark() {
    let a = Pk::new(&format!("{}{}", p(&long(1)), p(&long(2))));
    let b = Pk::new(&format!(
        "{}{}{}{}",
        p(&long(1)),
        p(&long(2)),
        p(&long(3)),
        p(&long(4))
    ));
    let (_, xml) = run(&a, &b);
    // the second paragraph and the first new one have inserted marks, and the last paragraph's mark, the body's end, is live
    assert_eq!(xml.matches("<w:rPr><w:ins ").count(), 2, "{xml}");
    let last = xml.rfind("<w:p>").expect("a last paragraph");
    assert!(!xml[last..].contains("<w:rPr><w:ins "), "{xml}");
}

#[test]
fn a_paragraph_appended_to_a_cell_keeps_the_cells_last_mark_live() {
    let a = Pk::new(&format!("{}{}", table(1, &[&[long(1)]]), p("after")));
    let b = Pk::new(&format!(
        "{}{}",
        table(
            1,
            &[&[format!(
                "{}</w:t></w:r></w:p><w:p><w:r><w:t>{}",
                long(1),
                long(2)
            )]]
        ),
        p("after")
    ));
    let (_, xml) = run(&a, &b);
    let cell = &xml[xml.find("<w:tc>").expect("a cell")..xml.find("</w:tc>").expect("its end")];
    assert_eq!(cell.matches("<w:rPr><w:ins ").count(), 1, "{xml}");
    assert!(cell.ends_with("</w:ins></w:p>"), "{xml}");
    let second = cell.rfind("<w:p>").expect("the new paragraph");
    assert!(!cell[second..].contains("<w:rPr><w:ins "), "{xml}");
}

#[test]
fn a_table_replaced_by_one_of_the_same_frame_is_one_table_of_deleted_rows_and_inserted_rows() {
    let a = Pk::new(&format!(
        "{}{}{}",
        p("Prices."),
        table(
            2,
            &[
                &["Item".into(), "Price".into()],
                &["Apples".into(), "10".into()]
            ]
        ),
        p("End.")
    ));
    let b = Pk::new(&format!(
        "{}{}{}",
        p("Prices."),
        table(
            2,
            &[
                &["Code".into(), "Name".into()],
                &["X1".into(), "Bolts".into()]
            ]
        ),
        p("End.")
    ));
    let (v, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:tbl>").count(), 1, "{xml}");
    assert_eq!(xml.matches("<w:trPr><w:del ").count(), 2, "{xml}");
    assert_eq!(xml.matches("<w:trPr><w:ins ").count(), 2, "{xml}");
    assert!(
        xml.find("<w:trPr><w:del ") < xml.find("<w:trPr><w:ins "),
        "{xml}"
    );
    assert!(
        v.receipt.exclusions.is_empty(),
        "{:?}",
        v.receipt.exclusions
    );
}

/// A table of the given column widths, one row per list of cell texts.
fn table_w(widths: &[u32], rows: &[&[&str]]) -> String {
    let grid: String = widths
        .iter()
        .map(|w| format!("<w:gridCol w:w=\"{w}\"/>"))
        .collect();
    let rows: String = rows
        .iter()
        .map(|cells| {
            let cells: String = cells
                .iter()
                .zip(widths)
                .map(|(c, w)| {
                    format!(
                        "<w:tc><w:tcPr><w:tcW w:w=\"{w}\" w:type=\"dxa\"/></w:tcPr>{}</w:tc>",
                        p(c)
                    )
                })
                .collect();
            format!("<w:tr>{cells}</w:tr>")
        })
        .collect();
    format!("<w:tbl><w:tblPr/><w:tblGrid>{grid}</w:tblGrid>{rows}</w:tbl>")
}

/// The redline of a table of `old` widths replaced by one of `new` widths, between two paragraphs.
fn replaced(old: &[u32], new: &[u32]) -> (Verdict, String) {
    let texts = |w: &[u32], tag: &str| -> Vec<Vec<String>> {
        (0..2)
            .map(|r| (0..w.len()).map(|c| format!("{tag}{r}{c}")).collect())
            .collect()
    };
    let doc = |w: &[u32], tag: &str| {
        let rows = texts(w, tag);
        let rows: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(String::as_str).collect())
            .collect();
        let rows: Vec<&[&str]> = rows.iter().map(Vec::as_slice).collect();
        Pk::new(&format!(
            "{}{}{}",
            p("Prices."),
            table_w(w, &rows),
            p("End.")
        ))
    };
    run(&doc(old, "old"), &doc(new, "new"))
}

fn grid_of(xml: &str) -> Vec<String> {
    let g =
        &xml[xml.find("<w:tblGrid>").expect("a grid")..xml.find("</w:tblGrid>").expect("its end")];
    g.split("w:w=\"")
        .skip(1)
        .map(|s| s[..s.find('"').unwrap_or(0)].to_string())
        .collect()
}

#[test]
fn a_table_replaced_by_a_wider_one_is_one_table_whose_deleted_rows_end_short() {
    let (v, xml) = replaced(&[2400, 2400], &[2400, 2400, 2400]);
    assert_eq!(xml.matches("<w:tbl>").count(), 1, "{xml}");
    assert!(!xml.contains("vanish"), "{xml}");
    // the revised grid, and the deleted rows as they were, two cells and no gridAfter, which LibreOffice draws as a border
    assert_eq!(grid_of(&xml), ["2400", "2400", "2400"]);
    assert!(
        !xml.contains("gridAfter") && !xml.contains("gridSpan"),
        "{xml}"
    );
    assert_eq!(xml.matches("<w:trPr><w:del ").count(), 2, "{xml}");
    assert!(
        v.receipt.exclusions.is_empty(),
        "{:?}",
        v.receipt.exclusions
    );
}

#[test]
fn a_table_replaced_by_a_narrower_one_keeps_a_grid_that_holds_the_deleted_rows() {
    // Past the revised grid's end, LibreOffice drops the deleted rows' last cells.
    let (_, xml) = replaced(&[2400, 2400, 2400], &[2400, 2400]);
    assert_eq!(xml.matches("<w:tbl>").count(), 1, "{xml}");
    assert_eq!(grid_of(&xml), ["2400", "2400", "2400"]);
    assert!(
        !xml.contains("gridAfter") && !xml.contains("gridSpan"),
        "{xml}"
    );
}

#[test]
fn a_table_replaced_by_one_of_other_widths_takes_every_column_edge_of_both() {
    let (_, xml) = replaced(&[3600, 3600], &[2400, 2400, 2400]);
    assert_eq!(xml.matches("<w:tbl>").count(), 1, "{xml}");
    assert_eq!(grid_of(&xml), ["2400", "1200", "1200", "2400"]);
    // each deleted cell spans two columns, the inserted rows' middle cell two
    assert_eq!(
        xml.matches("<w:gridSpan w:val=\"2\"/>").count(),
        4 + 2,
        "{xml}"
    );
    assert!(!xml.contains("gridAfter"), "{xml}");
}

#[test]
fn a_last_paragraph_replaced_by_another_shares_the_containers_last_mark() {
    // The last paragraphs of the body have nothing in common: they are one paragraph whose text is replaced, so the body's last
    // mark is never an inserted one.
    let a = Pk::new(&format!(
        "{}{}",
        p(&long(1)),
        p("Alpha beta gamma delta epsilon")
    ));
    let b = Pk::new(&format!(
        "{}{}",
        p(&long(1)),
        p("Nothing of the original survives here")
    ));
    let (_, xml) = run(&a, &b);
    let body = &xml[xml.find("<w:body>").expect("a body")..];
    assert_eq!(
        body.matches("<w:p>").count() + body.matches("<w:p ").count(),
        2,
        "{xml}"
    );
    assert!(!body.contains("<w:rPr><w:ins "), "{xml}");
    assert!(
        body.contains("<w:delText") && body.contains("<w:ins "),
        "{xml}"
    );
}
