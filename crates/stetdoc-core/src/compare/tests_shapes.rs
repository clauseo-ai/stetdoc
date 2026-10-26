//! The shapes of the redline for the scenarios of design `compare.md` (6.5 and 11): one fixture for each case, written by hand,
//! held to the compare laws (`check_pair`) and to a few facts about the XML that the laws do not see.

use super::tests_laws::{Verdict, check_pair};
use super::tests_pkg::Pk;

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

fn body_xml(v: &Verdict) -> String {
    let view = v.redline.parts();
    let id = view.id("word/document.xml").expect("a body");
    String::from_utf8_lossy(&view.tree(id).expect("a tree").serialize()).into_owned()
}

fn run(a: &Pk, b: &Pk) -> (Verdict, String) {
    let v = check_pair(a.bytes(), b.bytes()).expect("compares");
    assert!(v.failures.is_empty(), "{}", v.failures.join("\n"));
    assert_eq!(v.receipt.broken, 0);
    let xml = body_xml(&v);
    (v, xml)
}

fn long(n: usize) -> String {
    format!(
        "Clause {n} says that the supplier shall deliver the goods within thirty days of the order and keep records of it"
    )
}

#[test]
fn an_inserted_paragraph_has_an_inserted_mark_and_inserted_content() {
    let a = Pk::new(&format!("{}{}", p(&long(1)), p(&long(3))));
    let b = Pk::new(&format!("{}{}{}", p(&long(1)), p(&long(2)), p(&long(3))));
    let (_, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:rPr><w:ins ").count(), 1, "{xml}");
    assert!(!xml.contains("<w:del"), "{xml}");
}

#[test]
fn a_deleted_paragraph_has_a_deleted_mark_and_deleted_content() {
    let a = Pk::new(&format!("{}{}{}", p(&long(1)), p(&long(2)), p(&long(3))));
    let b = Pk::new(&format!("{}{}", p(&long(1)), p(&long(3))));
    let (v, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:rPr><w:del ").count(), 1, "{xml}");
    assert!(xml.contains("<w:delText"), "{xml}");
    assert_eq!(v.receipt.declared.imports.len(), 1);
}

#[test]
fn a_replaced_word_is_a_deletion_and_an_insertion_in_one_paragraph() {
    let a = Pk::new(&p(&long(1)));
    let b = Pk::new(&p(&long(1).replace("thirty", "sixty")));
    let (_, xml) = run(&a, &b);
    assert_eq!(
        xml.matches("<w:p>").count() + xml.matches("<w:p ").count(),
        1,
        "{xml}"
    );
    assert!(
        xml.contains("<w:delText xml:space=\"preserve\">thirty") && xml.contains("sixty"),
        "{xml}"
    );
    assert!(
        !xml.contains("<w:rPr><w:ins") && !xml.contains("<w:rPr><w:del"),
        "no mark changes\n{xml}"
    );
}

#[test]
fn a_split_paragraph_has_an_inserted_mark_on_the_first_part() {
    let a = Pk::new(&p(&format!("{} {}", long(1), long(2))));
    let b = Pk::new(&format!("{}{}", p(&long(1)), p(&long(2))));
    let (v, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:rPr><w:ins ").count(), 1, "{xml}");
    assert!(v.receipt.counts.split <= 1);
}

#[test]
fn a_merged_paragraph_is_cut_and_the_mark_between_the_parts_is_deleted() {
    let a = Pk::new(&format!("{}{}", p(&long(1)), p(&long(2))));
    let b = Pk::new(&p(&format!("{} {}", long(1), long(2))));
    let (_, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:rPr><w:del ").count(), 1, "{xml}");
    assert_eq!(
        xml.matches("<w:p>").count() + xml.matches("<w:p ").count(),
        2,
        "{xml}"
    );
}

fn row(cells: &[&str]) -> String {
    format!(
        "<w:tr>{}</w:tr>",
        cells
            .iter()
            .map(|c| format!("<w:tc><w:tcPr/>{}</w:tc>", p(c)))
            .collect::<String>()
    )
}

fn table(rows: &[String]) -> String {
    format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>{}</w:tbl>",
        rows.concat()
    )
}

#[test]
fn an_inserted_row_has_an_inserted_trpr_and_inserted_cells() {
    let (r1, r2, r3) = (
        row(&[&long(1), &long(2)]),
        row(&[&long(3), &long(4)]),
        row(&[&long(5), &long(6)]),
    );
    let a = Pk::new(&format!(
        "{}{}",
        table(&[r1.clone(), r3.clone()]),
        p("after")
    ));
    let b = Pk::new(&format!("{}{}", table(&[r1, r2, r3]), p("after")));
    let (_, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:trPr><w:ins ").count(), 1, "{xml}");
    assert_eq!(xml.matches("<w:rPr><w:ins ").count(), 2, "{xml}");
}

#[test]
fn a_deleted_row_has_a_deleted_trpr_and_every_paragraph_of_it_deleted() {
    let (r1, r2, r3) = (
        row(&[&long(1), &long(2)]),
        row(&[&long(3), &long(4)]),
        row(&[&long(5), &long(6)]),
    );
    let a = Pk::new(&format!(
        "{}{}",
        table(&[r1.clone(), r2, r3.clone()]),
        p("after")
    ));
    let b = Pk::new(&format!("{}{}", table(&[r1, r3]), p("after")));
    let (_, xml) = run(&a, &b);
    assert_eq!(xml.matches("<w:trPr><w:del ").count(), 1, "{xml}");
    assert_eq!(xml.matches("<w:rPr><w:del ").count(), 2, "{xml}");
}

#[test]
fn a_moved_paragraph_is_a_deletion_and_an_insertion_and_one_move_of_the_receipt() {
    let ps: Vec<String> = (1..=6).map(|n| p(&long(n))).collect();
    let a = Pk::new(&ps.concat());
    let order = [0, 2, 3, 4, 1, 5];
    let b = Pk::new(&order.iter().map(|&i| ps[i].clone()).collect::<String>());
    let (v, xml) = run(&a, &b);
    assert_eq!(v.receipt.moves.len(), 1, "{xml}");
    assert!(
        xml.contains("<w:rPr><w:del ") && xml.contains("<w:rPr><w:ins "),
        "{xml}"
    );
}

#[test]
fn formatting_that_changed_over_equal_text_is_a_property_change_and_nothing_else() {
    let a = Pk::new(&p(&long(1)));
    let b = Pk::new(&format!(
        "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        long(1)
    ));
    let (v, xml) = run(&a, &b);
    assert!(xml.contains("<w:rPrChange"), "{xml}");
    assert!(
        !xml.contains("<w:ins ") && !xml.contains("<w:del "),
        "{xml}"
    );
    assert_eq!(v.noise.fmt_spans, 1);
}

#[test]
fn a_field_that_was_replaced_is_deleted_whole_and_the_new_one_inserted() {
    let field = |code: &str, shown: &str| {
        format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>{shown}</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
        )
    };
    let a = Pk::new(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r>{} of the document and the rest of the line</w:p>",
        field("PAGE", "3")
    ));
    let b = Pk::new(&format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">Page </w:t></w:r>{} of the document and the rest of the line</w:p>",
        field("NUMPAGES", "9")
    ));
    let (_, xml) = run(&a, &b);
    assert!(
        xml.contains("<w:delInstrText xml:space=\"preserve\"> PAGE </w:delInstrText>"),
        "{xml}"
    );
    assert!(xml.contains("NUMPAGES"), "{xml}");
}

#[test]
fn a_replaced_picture_is_a_deleted_drawing_and_an_inserted_one_with_its_own_part() {
    let drawing = |rid: &str| {
        format!(
            "<w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"p\"/><a:graphic><a:graphicData><pic:pic><pic:blipFill><a:blip r:embed=\"{rid}\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"
        )
    };
    let a = Pk {
        media: vec![(
            "media/one.png".into(),
            String::new(),
            vec![1, 2, 3],
            "rId40".into(),
        )],
        ..Pk::new(&format!(
            "<w:p><w:r><w:t>Before </w:t></w:r>{}<w:r><w:t> after the picture and more text</w:t></w:r></w:p>",
            drawing("rId40")
        ))
    };
    let b = Pk {
        media: vec![(
            "media/two.png".into(),
            String::new(),
            vec![9, 9, 9],
            "rId41".into(),
        )],
        ..Pk::new(&format!(
            "<w:p><w:r><w:t>Before </w:t></w:r>{}<w:r><w:t> after the picture and more text</w:t></w:r></w:p>",
            drawing("rId41")
        ))
    };
    let (v, xml) = run(&a, &b);
    assert_eq!(v.receipt.stats.parts_copied, 1, "{xml}");
    assert!(xml.matches("<w:drawing>").count() == 2, "{xml}");
}

#[test]
fn a_deleted_footnote_reference_brings_the_note_with_it_and_a_new_note_is_inserted_with_its_reference()
 {
    let note = |id: u32, text: &str| {
        format!(
            "<w:footnote w:id=\"{id}\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> {text}</w:t></w:r></w:p></w:footnote>"
        )
    };
    let with_ref = |id: u32| {
        format!(
            "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r><w:r><w:footnoteReference w:id=\"{id}\"/></w:r></w:p>",
            long(1)
        )
    };
    let a = Pk {
        footnotes: Some(note(1, "the old note")),
        ..Pk::new(&with_ref(1))
    };
    let b = Pk {
        footnotes: Some(String::new()),
        ..Pk::new(&p(&long(1)))
    };
    let (_, xml) = run(&a, &b);
    assert!(
        xml.contains("footnoteReference"),
        "the deleted reference\n{xml}"
    );
    let b2 = Pk {
        footnotes: Some(note(1, "a new note")),
        ..Pk::new(&with_ref(1))
    };
    let a2 = Pk::new(&p(&long(1)));
    run(&a2, &b2);
}

#[test]
fn deleted_paragraphs_at_the_end_of_the_body_and_of_a_cell_keep_a_live_last_mark() {
    let a = Pk::new(&format!("{}{}{}", p(&long(1)), p(&long(2)), p(&long(3))));
    let b = Pk::new(&p(&long(1)));
    let (_, xml) = run(&a, &b);
    // the live paragraph and the first imported one lose their marks, the last imported one keeps its mark and loses its text
    assert_eq!(xml.matches("<w:rPr><w:del ").count(), 2, "{xml}");
    assert_eq!(xml.matches("<w:delText").count(), 2, "{xml}");
    let cell = |inner: &str| {
        format!(
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:tcPr/>{inner}</w:tc></w:tr></w:tbl>{}",
            p("after")
        )
    };
    let a = Pk::new(&cell(&format!(
        "{}{}{}",
        p(&long(1)),
        p(&long(2)),
        p(&long(3))
    )));
    let b = Pk::new(&cell(&p(&long(1))));
    run(&a, &b);
}

#[test]
fn a_list_paragraph_that_is_deleted_keeps_its_list_and_a_new_list_is_imported_when_it_has_no_partner()
 {
    let abs = |id: u32, fmt: &str| {
        format!(
            "<w:abstractNum w:abstractNumId=\"{id}\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{fmt}\"/><w:lvlText w:val=\"%1.\"/></w:lvl></w:abstractNum>"
        )
    };
    let item = |n: u32, t: &str| {
        format!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{n}\"/></w:numPr></w:pPr><w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r></w:p>"
        )
    };
    let a = Pk {
        numbering: Some(format!(
            "{}<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>",
            abs(0, "upperRoman")
        )),
        ..Pk::new(&format!("{}{}", item(1, &long(1)), p(&long(2))))
    };
    let b = Pk {
        numbering: Some(format!(
            "{}<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num>",
            abs(0, "decimal")
        )),
        ..Pk::new(&p(&long(2)))
    };
    let (v, xml) = run(&a, &b);
    assert_eq!(v.receipt.stats.lists_imported, 1, "{xml}");
    assert!(xml.contains("w:numId w:val=\"2\""), "{xml}");
}

#[test]
fn a_paragraph_that_ends_a_section_and_is_inserted_is_an_exclusion_and_not_a_failure() {
    let a = Pk::new(&p(&long(1)));
    let b = Pk::new(&format!(
        "{}<w:p><w:pPr><w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/></w:sectPr></w:pPr><w:r><w:t>End of the section</w:t></w:r></w:p>",
        p(&long(1))
    ));
    let v = check_pair(a.bytes(), b.bytes()).expect("compares");
    assert_eq!(v.receipt.exclusions.len(), 1);
    assert!(v.receipt.exclusions[0].reason.contains("section"));
}

#[test]
fn a_paragraph_that_cannot_be_edited_in_place_is_replaced_whole_and_the_comparison_goes_on() {
    // a content control that is locked against editing its content holds the text that changes
    let locked = |t: &str| {
        format!(
            "<w:sdt><w:sdtPr><w:lock w:val=\"sdtContentLocked\"/><w:id w:val=\"7\"/></w:sdtPr><w:sdtContent><w:r><w:t xml:space=\"preserve\">{t}</w:t></w:r></w:sdtContent></w:sdt>"
        )
    };
    let a = Pk::new(&format!(
        "<w:p>{}</w:p>{}",
        locked("the sum of one hundred dollars is due on signing"),
        p(&long(2))
    ));
    let b = Pk::new(&format!(
        "<w:p>{}</w:p>{}",
        locked("the sum of two hundred dollars is due on signing"),
        p(&long(2))
    ));
    let v = check_pair(a.bytes(), b.bytes()).expect("compares");
    assert!(v.failures.is_empty(), "{}", v.failures.join("\n"));
    assert_eq!(v.receipt.broken, 0);
    assert!(
        v.receipt
            .warnings
            .iter()
            .any(|w| w.code == "compare_paragraph_replaced"),
        "{:?}",
        v.receipt.warnings
    );
}

#[test]
fn a_table_of_contents_whose_page_numbers_changed_is_one_group_of_paragraphs() {
    let toc = |pages: [&str; 3]| {
        let line = |t: &str, pg: &str, first: bool, last: bool| {
            let begin = if first {
                "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> TOC \\o \"1-3\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>"
            } else {
                ""
            };
            let end = if last {
                "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
            } else {
                ""
            };
            format!("<w:p>{begin}<w:r><w:t xml:space=\"preserve\">{t}\t{pg}</w:t></w:r>{end}</w:p>")
        };
        format!(
            "{}{}{}",
            line("Definitions of the terms", pages[0], true, false),
            line("Payment of the price", pages[1], false, false),
            line("Termination of the agreement", pages[2], false, true)
        )
    };
    let a = Pk::new(&format!("{}{}", toc(["3", "5", "8"]), p(&long(1))));
    let b = Pk::new(&format!("{}{}", toc(["3", "6", "9"]), p(&long(1))));
    run(&a, &b);
}

#[test]
fn a_paragraph_deleted_after_a_split_stays_after_both_parts() {
    // rejecting must give the original's order: the long paragraph and then the other one
    let first = format!("{} {}", long(1), long(2));
    let a = Pk::new(&format!(
        "{}{}{}",
        p(&first),
        p("An unrelated paragraph about the governing law of the agreement and its venue"),
        p(&long(9))
    ));
    let b = Pk::new(&format!("{}{}{}", p(&long(1)), p(&long(2)), p(&long(9))));
    run(&a, &b);
}

#[test]
fn paragraphs_that_were_merged_around_text_that_was_deleted_are_planned_as_paragraphs_and_not_as_a_merge()
 {
    // the second paragraph is gone altogether: there is nothing to cut the revised paragraph at
    let a = Pk::new(&format!(
        "{}{}{}",
        p(&long(1)),
        p("A second paragraph that the revised document does not have at all anymore"),
        p(&long(9))
    ));
    let b = Pk::new(&format!("{}{}", p(&long(1)), p(&long(9))));
    let (v, _) = run(&a, &b);
    assert_eq!(v.receipt.counts.merged, 0);
}

#[test]
fn a_paragraph_that_gained_a_neighbour_is_an_insertion_and_not_a_split() {
    let a = Pk::new(&format!("{}{}", p(&long(1)), p(&long(3))));
    let b = Pk::new(&format!(
        "{}{}{}",
        p(&long(1)),
        p("A new paragraph about the same clause number three, which uses its words"),
        p(&long(3))
    ));
    let (v, xml) = run(&a, &b);
    assert_eq!(v.receipt.counts.split, 0, "{xml}");
}

#[test]
fn deletions_that_end_the_body_go_after_an_inserted_table_so_rejecting_restores_the_last_paragraph()
{
    // DB010: the original ends with a picture, the revised with new text, a new table and an empty last paragraph. The picture's
    // paragraph cannot share the last mark (it holds an atom), so it takes the inserted end mark: placed before the table, its
    // join would have to reach across the table that rejecting removes, which it does not, and an empty paragraph was left.
    let pic = "<w:p><w:r><w:drawing><wp:inline><wp:docPr id=\"7\" name=\"q\"/></wp:inline></w:drawing></w:r></w:p>";
    let table = "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"4788\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>";
    let a = Pk::new(&format!("{}{pic}", p(&long(1))));
    let b = Pk::new(&format!("{}{}{table}<w:p/>", p(&long(2)), p(&long(3))));
    let (v, xml) = run(&a, &b);
    assert!(v.receipt.exclusions.is_empty(), "{xml}");
    let (table_at, picture_at) = (
        xml.find("<w:tbl>").unwrap(),
        xml.find("<w:drawing>").unwrap(),
    );
    assert!(table_at < picture_at, "{xml}");
}
