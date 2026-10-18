//! The scenario table of design 8.2a, without Word's files: for each row, the document before, the
//! operations of the row under a fixed stamp, and the shape that Word writes for the same edit, written
//! out by hand from section 1.3 of the design. The comparison is `normalize` of the result with `normalize`
//! of Word's shape, ids renamed, author and date as the stamp writes them. There are no known divergences:
//! a row that differs is a bug in the compiler, or a decision to change the design.

use super::stamp::Track;
use super::tests_support::*;
use super::types::{NewParagraph, Place, PropTarget};

fn at(id: u32) -> String {
    format!("w:id=\"{id}\" {AT}")
}

/// The attributes of a revision by the second author of the table (`stamp_of`).
fn bob(id: u32) -> String {
    format!("w:id=\"{id}\" w:author=\"Bob\" w:date=\"2026-10-06T09:00:00Z\"")
}

fn mark(kind: &str, id: u32) -> String {
    format!("<w:rPr><w:{kind} {}/></w:rPr>", at(id))
}

fn dtext(text: &str) -> String {
    format!("<w:r><w:delText>{text}</w:delText></w:r>")
}

fn itext(text: &str) -> String {
    format!("<w:r><w:t>{text}</w:t></w:r>")
}

fn tracked(
    body: &str,
    f: impl FnOnce(
        &mut super::compiler::Compiler<'_, '_>,
        &[crate::wordml::BlockKey],
        &[crate::wordml::BlockKey],
    ) -> Vec<Result<super::outcome::Effect, crate::api::error::StetError>>,
) -> Snap {
    let s = with_compiler(body, Track::Tracked(stamp()), f);
    s.assert_clean();
    s
}

#[test]
fn delete_a_word_wraps_the_run_in_del_with_deltext_and_leaves_the_neighbours() {
    // FA/001-DeletedRun, RP002-Deleted-Text
    let s = tracked(&p(&r("one two three")), |c, _, ks| {
        vec![c.replace_text(&ks[0], 4..7, "")]
    });
    s.assert_word_shape(&p(&format!(
        "{}<w:del {}>{}</w:del>{}",
        itext("one "),
        at(1),
        dtext("two"),
        itext(" three")
    )));
}

#[test]
fn type_new_text_is_an_ins_with_a_run_that_copies_the_neighbours_formatting() {
    // FA/010-InsertedRun, RP003-Inserted-Text
    let bold = |t: &str| format!("<w:r><w:rPr><w:b/></w:rPr><w:t>{t}</w:t></w:r>");
    let s = tracked(&p(&bold("bold text")), |c, _, ks| {
        vec![c.replace_text(&ks[0], 4..4, "X")]
    });
    s.assert_word_shape(&p(&format!(
        "{}<w:ins {}>{}</w:ins>{}",
        bold("bold"),
        at(1),
        bold("X"),
        bold(" text")
    )));
}

#[test]
fn replace_a_word_is_a_del_then_an_ins_next_to_each_other_with_ids_of_their_own() {
    let s = tracked(&p(&r("one two three")), |c, _, ks| {
        vec![c.replace_text(&ks[0], 4..7, "2")]
    });
    s.assert_word_shape(&p(&format!(
        "{}<w:del {}>{}</w:del><w:ins {}>{}</w:ins>{}",
        itext("one "),
        at(1),
        dtext("two"),
        at(2),
        itext("2"),
        itext(" three")
    )));
}

#[test]
fn delete_a_paragraph_in_the_middle_deletes_its_content_and_its_mark_and_touches_no_other() {
    // FA/013-DeletedParagraph
    let s = tracked(
        &format!("{}{}{}", p(&r("A")), p(&r("B")), p(&r("C"))),
        |c, _, ks| vec![c.delete_block(&ks[1])],
    );
    s.assert_word_shape(&format!(
        "{}<w:p><w:pPr>{}</w:pPr><w:del {}>{}</w:del></w:p>{}",
        p(&itext("A")),
        mark("del", 1),
        at(2),
        dtext("B"),
        p(&itext("C"))
    ));
}

#[test]
fn delete_from_the_end_of_one_paragraph_through_the_end_of_the_last_keeps_the_last_mark() {
    // RP040-Deleted-Paras-at-End
    let s = tracked(
        &format!("{}{}{}", p(&r("A")), p(&r("B")), p(&r("C"))),
        |c, _, ks| {
            vec![
                c.delete_block(&ks[1]),
                c.delete_block(&ks[2]),
                c.merge_blocks(&ks[0], &ks[2]),
            ]
        },
    );
    s.assert_word_shape(&format!(
        "<w:p><w:pPr>{}</w:pPr>{}</w:p><w:p><w:pPr>{}</w:pPr><w:del {}>{}</w:del></w:p><w:p><w:del {}>{}</w:del></w:p>",
        mark("del", 1),
        itext("A"),
        mark("del", 2),
        at(3),
        dtext("B"),
        at(4),
        dtext("C")
    ));
}

#[test]
fn delete_numbered_paragraphs_keeps_the_numbering_properties_and_adds_the_deleted_mark() {
    // FA/002-DeletedNumberedParagraphs
    let num = "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>";
    let item = |t: &str| format!("<w:p><w:pPr>{num}</w:pPr>{}</w:p>", r(t));
    let body = format!("{}{}{}{}", item("a"), item("b"), item("c"), p(&r("tail")));
    let s = tracked(&body, |c, _, ks| {
        (0..3).map(|i| c.delete_block(&ks[i])).collect()
    });
    let gone = |t: &str, base: u32| {
        format!(
            "<w:p><w:pPr>{num}{}</w:pPr><w:del {}>{}</w:del></w:p>",
            mark("del", base),
            at(base + 1),
            dtext(t)
        )
    };
    s.assert_word_shape(&format!(
        "{}{}{}{}",
        gone("a", 1),
        gone("b", 3),
        gone("c", 5),
        p(&itext("tail"))
    ));
}

#[test]
fn insert_a_paragraph_before_another_is_a_new_paragraph_with_an_inserted_mark_and_inserted_content()
{
    // FA/009-InsertedParagraph
    let s = tracked(&format!("{}{}", p(&r("A")), p(&r("B"))), |c, _, ks| {
        vec![c.insert_block(
            &ks[1],
            Place::Before,
            &NewParagraph {
                text: "new".into(),
                ..Default::default()
            },
        )]
    });
    s.assert_word_shape(&format!(
        "{}<w:p><w:pPr>{}</w:pPr><w:ins {}>{}</w:ins></w:p>{}",
        p(&itext("A")),
        mark("ins", 1),
        at(2),
        itext("new"),
        p(&itext("B"))
    ));
}

#[test]
fn append_paragraphs_leaves_the_first_plain_and_gives_each_new_one_an_inserted_mark_and_content() {
    // RP039-Inserted-Paras-at-End
    let s = tracked(&p(&r("A")), |c, _, ks| {
        let first = c.insert_block(
            &ks[0],
            Place::After,
            &NewParagraph {
                text: "B".into(),
                ..Default::default()
            },
        );
        let anchor = first
            .as_ref()
            .ok()
            .and_then(|e| e.new_blocks.first().cloned());
        let second = anchor.map(|a| {
            c.insert_block(
                &a,
                Place::After,
                &NewParagraph {
                    text: "C".into(),
                    ..Default::default()
                },
            )
        });
        let mut out = vec![first];
        out.extend(second);
        out
    });
    s.assert_word_shape(&format!(
        "{}<w:p><w:pPr>{}</w:pPr><w:ins {}>{}</w:ins></w:p><w:p><w:pPr>{}</w:pPr><w:ins {}>{}</w:ins></w:p>",
        p(&itext("A")),
        mark("ins", 1),
        at(2),
        itext("B"),
        mark("ins", 3),
        at(4),
        itext("C")
    ));
}

fn numbered() -> super::types::PropEdit {
    set_edit(vec![pel(
        "numPr",
        &[],
        vec![
            pel("ilvl", &[("val", "0")], vec![]),
            pel("numId", &[("val", "1")], vec![]),
        ],
    )])
}

#[test]
fn enter_at_the_end_of_a_paragraph_and_then_numbering_all_three_is_the_shape_of_fa_005() {
    // FA/005-InsertedNumberedParagraph: the first paragraph has the list pPr and no pPrChange, the second
    // and the third have it with a pPrChange whose snapshot is the original, empty pPr.
    let body = format!("{}{}", p(&r("one")), p(&r("two")));
    let s = tracked(&body, |c, _, ks| {
        let ins = c.insert_block(
            &ks[0],
            Place::After,
            &NewParagraph {
                text: "new".into(),
                props: numbered(),
                style: None,
            },
        );
        let first = c.set_props(
            &PropTarget::Paragraph {
                block: ks[0].clone(),
            },
            &numbered(),
        );
        let third = c.set_props(
            &PropTarget::Paragraph {
                block: ks[1].clone(),
            },
            &numbered(),
        );
        vec![ins, first, third]
    });
    let num = "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>";
    s.assert_word_shape(&format!(
        "<w:p><w:pPr>{num}{}</w:pPr>{}</w:p>\
         <w:p><w:pPr>{num}<w:pPrChange {}><w:pPr/></w:pPrChange></w:pPr><w:ins {}>{}</w:ins></w:p>\
         <w:p><w:pPr>{num}<w:pPrChange {}><w:pPr/></w:pPrChange></w:pPr>{}</w:p>",
        mark("ins", 1),
        itext("one"),
        at(3),
        at(2),
        itext("new"),
        at(4),
        itext("two")
    ));
}

#[test]
fn split_a_paragraph_gives_the_first_half_an_inserted_mark_and_leaves_the_second_a_plain_mark() {
    // RP006-Inserted-Paragraph-Mark
    let s = tracked(&p(&r("hello world")), |c, _, ks| {
        vec![c.split_block(&ks[0], 5)]
    });
    s.assert_word_shape(&format!(
        "<w:p><w:pPr>{}</w:pPr>{}</w:p><w:p>{}</w:p>",
        mark("ins", 1),
        itext("hello"),
        itext(" world")
    ));
}

#[test]
fn merge_two_paragraphs_deletes_the_first_mark_and_leaves_both_texts() {
    // RP005-Deleted-Paragraph-Mark
    let s = tracked(&format!("{}{}", p(&r("one")), p(&r("two"))), |c, _, ks| {
        vec![c.merge_blocks(&ks[0], &ks[1])]
    });
    s.assert_word_shape(&format!(
        "<w:p><w:pPr>{}</w:pPr>{}</w:p>{}",
        mark("del", 1),
        itext("one"),
        p(&itext("two"))
    ));
}

#[test]
fn merge_a_run_of_paragraphs_deletes_each_mark_but_the_last() {
    // RP007-Multiple-Deleted-Para-Mark
    let body = format!("{}{}{}", p(&r("one")), p(&r("two")), p(&r("three")));
    let s = tracked(&body, |c, _, ks| {
        vec![
            c.merge_blocks(&ks[0], &ks[1]),
            c.merge_blocks(&ks[1], &ks[2]),
        ]
    });
    s.assert_word_shape(&format!(
        "<w:p><w:pPr>{}</w:pPr>{}</w:p><w:p><w:pPr>{}</w:pPr>{}</w:p>{}",
        mark("del", 1),
        itext("one"),
        mark("del", 2),
        itext("two"),
        p(&itext("three"))
    ));
}

#[test]
fn change_paragraph_formatting_is_a_ppr_change_last_in_ppr_with_the_old_properties() {
    // FA/018, RP025, and the style case of n830205
    let body = "<w:p><w:pPr><w:pStyle w:val=\"Old\"/><w:jc w:val=\"left\"/></w:pPr><w:r><w:t>t</w:t></w:r></w:p>";
    let s = tracked(body, |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Paragraph {
                block: ks[0].clone(),
            },
            &set_edit(vec![
                pel("pStyle", &[("val", "New")], vec![]),
                pel("jc", &[("val", "center")], vec![]),
            ]),
        )]
    });
    s.assert_word_shape(&format!(
        "<w:p><w:pPr><w:pStyle w:val=\"New\"/><w:jc w:val=\"center\"/><w:pPrChange {}><w:pPr><w:pStyle w:val=\"Old\"/><w:jc w:val=\"left\"/></w:pPr></w:pPrChange></w:pPr>{}</w:p>",
        at(1),
        itext("t")
    ));
}

#[test]
fn change_the_paragraph_mark_formatting_is_an_rpr_change_inside_the_marks_rpr() {
    // RP024-ParagraphMark-rPr-Change
    let s = tracked(&p(&r("t")), |c, _, ks| {
        vec![c.set_props(
            &PropTarget::ParagraphMark {
                block: ks[0].clone(),
            },
            &bold(),
        )]
    });
    s.assert_word_shape(&format!(
        "<w:p><w:pPr><w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr></w:pPr>{}</w:p>",
        at(1),
        itext("t")
    ));
}

#[test]
fn format_text_is_an_rpr_change_as_the_last_child_of_each_runs_rpr() {
    // FA/019-RunPropertiesChange
    let s = tracked(&p(&r("one two three")), |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Run {
                block: ks[0].clone(),
                range: 4..7,
            },
            &bold(),
        )]
    });
    s.assert_word_shape(&p(&format!(
        "{}<w:r><w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr><w:t>two</w:t></w:r>{}",
        itext("one "),
        at(1),
        itext(" three")
    )));
}

const TABLE_HEAD: &str =
    "<w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>";

fn row(a: &str, b: &str) -> String {
    format!(
        "<w:tr><w:tc><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>",
        r(a),
        r(b)
    )
}

#[test]
fn insert_a_row_marks_the_row_and_every_paragraph_and_wraps_the_content_without_cell_ins() {
    // FA/007, RP010
    let body = format!(
        "<w:tbl>{TABLE_HEAD}{}</w:tbl>{}",
        row("a1", "b1"),
        p(&r("after"))
    );
    let s = tracked(&body, |c, all, _| {
        // The row is the second block: table, row, then the cells' paragraphs.
        vec![c.insert_row(&all[1], Place::After, &["x".to_owned(), "y".to_owned()])]
    });
    s.assert_word_shape(&format!(
        "<w:tbl>{TABLE_HEAD}{}<w:tr><w:trPr><w:ins {}/></w:trPr>\
         <w:tc><w:p><w:pPr>{}</w:pPr><w:ins {}>{}</w:ins></w:p></w:tc>\
         <w:tc><w:p><w:pPr>{}</w:pPr><w:ins {}>{}</w:ins></w:p></w:tc></w:tr></w:tbl>{}",
        row("a1", "b1"),
        at(1),
        mark("ins", 2),
        at(3),
        itext("x"),
        mark("ins", 4),
        at(5),
        itext("y"),
        p(&itext("after"))
    ));
}

#[test]
fn delete_a_row_marks_the_row_and_every_paragraph_and_wraps_the_content_in_del() {
    // FA/006, RP009: the last paragraph of each cell too
    let body = format!(
        "<w:tbl>{TABLE_HEAD}{}{}</w:tbl>{}",
        row("a1", "b1"),
        row("a2", "b2"),
        p(&r("after"))
    );
    // Blocks: the table, the first row and its two paragraphs, then the second row.
    let s = tracked(&body, |c, all, _| vec![c.delete_block(&all[4])]);
    s.assert_word_shape(&format!(
        "<w:tbl>{TABLE_HEAD}{}<w:tr><w:trPr><w:del {}/></w:trPr>\
         <w:tc><w:p><w:pPr>{}</w:pPr><w:del {}>{}</w:del></w:p></w:tc>\
         <w:tc><w:p><w:pPr>{}</w:pPr><w:del {}>{}</w:del></w:p></w:tc></w:tr></w:tbl>{}",
        row("a1", "b1"),
        at(1),
        mark("del", 2),
        at(3),
        dtext("a2"),
        mark("del", 4),
        at(5),
        dtext("b2"),
        p(&itext("after"))
    ));
}

#[test]
fn delete_a_field_is_one_del_around_every_run_from_begin_to_end() {
    // FA/003-DeletedFieldCode, RP019
    let fld = "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>3</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
    let body = p(&format!("{}{fld}{}", r("a "), r(" b")));
    let s = tracked(&body, |c, _, ks| vec![c.replace_text(&ks[0], 2..3, "")]);
    s.assert_word_shape(&p(&format!(
        "{}<w:del {}><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:delInstrText xml:space=\"preserve\"> PAGE </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:delText>3</w:delText></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:del>{}",
        itext("a "),
        at(1),
        itext(" b")
    )));
}

#[test]
fn a_second_author_deleting_inside_an_insertion_nests_the_del_in_the_ins() {
    // cross-author-slicing/delete-interior-pending: ins{r del{r}}
    let body = p(&format!(
        "<w:ins {}>{}</w:ins>",
        at(900),
        itext("hello world")
    ));
    let s = with_compiler(&body, Track::Tracked(stamp_of("Bob")), |c, _, ks| {
        vec![c.replace_text(&ks[0], 6..11, "")]
    });
    s.assert_clean();
    s.assert_word_shape(&p(&format!(
        "<w:ins {}>{}<w:del {}>{}</w:del></w:ins>",
        at(900),
        itext("hello "),
        bob(901),
        dtext("world")
    )));
}

#[test]
fn a_second_author_inserting_inside_an_insertion_splits_it_and_the_tail_gets_a_new_id() {
    // cross-author-slicing/insert-interior-pending
    let body = p(&format!("<w:ins {}>{}</w:ins>", at(900), itext("hello")));
    let s = with_compiler(&body, Track::Tracked(stamp_of("Bob")), |c, _, ks| {
        vec![c.replace_text(&ks[0], 2..2, "X")]
    });
    s.assert_clean();
    s.assert_word_shape(&p(&format!(
        "<w:ins {}>{}</w:ins><w:ins {}>{}</w:ins><w:ins {}>{}</w:ins>",
        at(900),
        itext("he"),
        bob(901),
        itext("X"),
        at(902),
        itext("llo")
    )));
}

#[test]
fn change_table_row_and_cell_properties_each_get_their_own_change_with_the_old_properties() {
    // FA/021 to FA/024, RP028 to RP033: tblPrChange, trPrChange, tcPrChange
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr>{}</w:tc></w:tr></w:tbl>{}",
        p(&r("cell")),
        p(&r("after"))
    );
    let s = tracked(&body, |c, all, _| {
        vec![
            c.set_props(
                &PropTarget::Table {
                    table: all[0].clone(),
                },
                &set_edit(vec![pel("jc", &[("val", "center")], vec![])]),
            ),
            c.set_props(
                &PropTarget::Row {
                    row: all[1].clone(),
                },
                &set_edit(vec![pel("cantSplit", &[], vec![])]),
            ),
            c.set_props(
                &PropTarget::Cell {
                    row: all[1].clone(),
                    cell: 0,
                },
                &set_edit(vec![pel("noWrap", &[], vec![])]),
            ),
        ]
    });
    s.assert_word_shape(&format!(
        "<w:tbl><w:tblPr><w:jc w:val=\"center\"/><w:tblPrChange {}><w:tblPr/></w:tblPrChange></w:tblPr><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid>\
         <w:tr><w:trPr><w:cantSplit/><w:trPrChange {}><w:trPr/></w:trPrChange></w:trPr>\
         <w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:noWrap/><w:tcPrChange {}><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr></w:tcPrChange></w:tcPr>{}</w:tc></w:tr></w:tbl>{}",
        at(1),
        at(2),
        at(3),
        p(&itext("cell")),
        p(&itext("after"))
    ));
}

#[test]
fn change_section_properties_is_a_sect_pr_change_that_holds_the_old_ones_without_the_references() {
    // FA/020, RP027
    let body = "<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId9\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:pPr><w:r><w:t>end</w:t></w:r></w:p>";
    let s = tracked(body, |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Section {
                block: ks[0].clone(),
            },
            &set_edit(vec![pel("pgSz", &[("w", "11906"), ("h", "16838")], vec![])]),
        )]
    });
    s.assert_word_shape(&format!(
        "<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId9\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"/><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:sectPrChange {}><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:sectPrChange></w:sectPr></w:pPr>{}</w:p>",
        at(1),
        itext("end")
    ));
}
