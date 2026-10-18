//! Blocks: styles, labels, marks, tables, sections, notes, headers and the blocks that are not shown.

use super::testing::*;
use crate::api::view::Mode;
use crate::projection::model::*;

use crate::wordml::StoryKind;

fn only_story(m: &Projection, kind: StoryKind) -> &Story {
    stories(m).iter().find(|s| s.kind == kind).unwrap()
}

#[test]
fn the_common_style_is_the_commonest_name_and_other_styles_are_tagged() {
    let heading = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr></w:style>\
                   <w:style w:type=\"paragraph\" w:styleId=\"BodyText\"><w:name w:val=\"Body Text\"/></w:style>";
    let styled = |id: &str, t: &str| {
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"{id}\"/></w:pPr>{}</w:p>",
            r(t)
        )
    };
    let body = format!(
        "{}{}{}{}",
        styled("Heading1", "Title"),
        styled("BodyText", "one"),
        styled("BodyText", "two"),
        p("three")
    );
    let m = redline(&Fixture::new(&body).styles(heading));
    assert_eq!(m.head.common_style.as_deref(), Some("Body Text"));
    assert_eq!(para(&m, 0).style.as_deref(), Some("heading 1"));
    assert_eq!(para(&m, 1).style, None);
    assert_eq!(para(&m, 2).style, None);
    assert_eq!(para(&m, 3).style.as_deref(), Some("Normal"));
}

#[test]
fn a_document_with_no_styles_part_reads_as_normal() {
    let m = redline(&Fixture::new(&format!("{}{}", p("a"), p("b"))));
    assert_eq!(m.head.common_style.as_deref(), Some("Normal"));
    assert_eq!(para(&m, 0).style, None);
}

#[test]
fn list_paragraphs_have_their_computed_label_and_bullets_a_glyph() {
    let item = |num: u32, lvl: u32, t: &str| {
        format!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{lvl}\"/><w:numId w:val=\"{num}\"/></w:numPr></w:pPr>{}</w:p>",
            r(t)
        )
    };
    let body = format!(
        "{}{}{}{}",
        item(1, 0, "Definitions"),
        item(1, 1, "Affiliate"),
        item(1, 0, "Payment"),
        item(2, 0, "bullet")
    );
    let m = redline(&Fixture::new(&body).numbering());
    let label = |i| para(&m, i).label.clone().unwrap();
    assert_eq!((&*label(0).text, label(0).bullet), ("1.", false));
    assert_eq!(&*label(1).text, "(a)");
    assert_eq!(&*label(2).text, "2.");
    assert!(label(3).bullet);
}

#[test]
fn the_mark_of_an_inserted_paragraph_and_a_deleted_one_are_tagged_in_redline_only() {
    let marked = |kind: &str, id: u32, inner: &str| {
        format!(
            "<w:p><w:pPr><w:rPr><w:{kind} w:id=\"{id}\" w:author=\"JD\" w:date=\"2026-01-01T00:00:00Z\"/></w:rPr></w:pPr>{inner}</w:p>"
        )
    };
    let body = format!(
        "{}{}{}",
        p("first"),
        marked("ins", 5, &ins(6, "JD", "new text")),
        marked("del", 7, &del(8, "JD", "old text"))
    );
    let f = Fixture::new(&body);
    let m = redline(&f);
    assert_eq!(
        para(&m, 1).mark.as_ref().map(|x| x.kind),
        Some(MarkKind::Inserted)
    );
    assert_eq!(
        para(&m, 2).mark.as_ref().map(|x| x.kind),
        Some(MarkKind::Deleted)
    );
    assert_eq!(para(&m, 2).flags.readonly, Some(ReadOnly::DeletedBlock));
    assert_eq!(para(&m, 1).flags.readonly, None);
    assert_eq!(m.head.summary.readonly, 1);
    // In `Final` the deleted paragraph is gone, and in `Original` the inserted one.
    let fin = model(&f.state(), Mode::Final);
    assert_eq!(body_len(&fin), 2);
    assert!(para(&fin, 1).mark.is_none());
    let orig = model(&f.state(), Mode::Original);
    assert_eq!(body_len(&orig), 2);
    assert_eq!(redline_text(&para(&orig, 1).inlines), "old text");
}

fn body_len(m: &Projection) -> usize {
    super::testing::body(m).len()
}

#[test]
fn a_table_has_rows_cells_spans_merges_and_nested_tables() {
    let cell = |inner: &str, props: &str| format!("<w:tc><w:tcPr>{props}</w:tcPr>{inner}</w:tc>");
    let nested = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr>{}</w:tr></w:tbl>",
        cell(&p("inner"), "")
    );
    let table = format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"100\"/><w:gridCol w:w=\"100\"/><w:gridCol w:w=\"100\"/></w:tblGrid>\
         <w:tr><w:trPr><w:tblHeader/></w:trPr>{}{}</w:tr>\
         <w:tr>{}{}{}</w:tr></w:tbl>",
        cell(&p("A"), "<w:gridSpan w:val=\"2\"/>"),
        cell(&p("B"), "<w:vMerge w:val=\"restart\"/>"),
        cell(&format!("{}{}", p("c1"), p("c2")), ""),
        cell(&nested, ""),
        cell(&p("e"), "<w:vMerge/>")
    );
    let f = Fixture::new(&format!("{}{}{}", p("before"), table, p("after"))).styles(
        "<w:style w:type=\"table\" w:styleId=\"Grid\"><w:name w:val=\"Table Grid\"/></w:style>",
    );
    let m = redline(&f);
    assert_eq!(body_len(&m), 3);
    let Block::Table(t) = &body(&m)[1] else {
        panic!()
    };
    assert_eq!(t.cols, 3);
    assert_eq!(t.style.as_deref(), Some("Table Grid"));
    assert_eq!(t.rows.len(), 2);
    assert!(t.rows[0].header && !t.rows[1].header);
    assert_eq!(
        t.rows[0]
            .cells
            .iter()
            .map(|c| (c.col, c.span, c.v_merge))
            .collect::<Vec<_>>(),
        vec![
            (0, 2, crate::wordml::VMerge::None),
            (2, 1, crate::wordml::VMerge::Restart)
        ]
    );
    assert_eq!(t.rows[1].cells[0].blocks.len(), 2);
    assert!(matches!(&t.rows[1].cells[1].blocks[0], Block::Table(inner) if inner.rows.len() == 1));
    assert_eq!(t.rows[1].cells[2].v_merge, crate::wordml::VMerge::Continue);
    // Every ref is minted and unique.
    let mut refs = Vec::new();
    collect_refs(body(&m), &mut refs);
    let n = refs.len();
    refs.sort_by_key(|r| (r.kind() as u8, r.number()));
    refs.dedup();
    assert_eq!(refs.len(), n);
    assert_eq!(n, 3 + 1 + 2 + 1 + 5 + 1, "paragraphs, tables and rows: {n}");
}

fn collect_refs(blocks: &[Block], out: &mut Vec<crate::refs::BlockRef>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.extend(p.r#ref),
            Block::Table(t) => {
                out.extend(t.r#ref);
                for row in &t.rows {
                    out.extend(row.r#ref);
                    for cell in &row.cells {
                        collect_refs(&cell.blocks, out);
                    }
                }
            }
            _ => {}
        }
    }
}

#[test]
fn an_inserted_row_is_tagged_in_redline_and_absent_from_original_and_a_deleted_one_from_final() {
    let row = |kind: &str, id: u32, t: &str| {
        format!(
            "<w:tr><w:trPr><w:{kind} w:id=\"{id}\" w:author=\"JD\" w:date=\"2026-01-01T00:00:00Z\"/></w:trPr><w:tc><w:tcPr/>{}</w:tc></w:tr>",
            p(t)
        )
    };
    let plain = format!("<w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>", p("kept"));
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid>{plain}{}{}</w:tbl>",
        row("ins", 3, "added"),
        row("del", 4, "removed")
    );
    let f = Fixture::new(&table);
    let m = redline(&f);
    let Block::Table(t) = &body(&m)[0] else {
        panic!()
    };
    assert_eq!(t.rows.len(), 3);
    assert_eq!(
        t.rows[1].change.as_ref().map(|c| c.kind),
        Some(RowChange::Inserted)
    );
    assert_eq!(
        t.rows[2].change.as_ref().map(|c| c.kind),
        Some(RowChange::Deleted)
    );
    let Block::Paragraph(deleted) = &t.rows[2].cells[0].blocks[0] else {
        panic!()
    };
    assert_eq!(deleted.flags.readonly, Some(ReadOnly::DeletedBlock));
    assert!(deleted.mark.is_none(), "the row owns the revision");
    assert_eq!(redline_text(&deleted.inlines), "removed");
    let rows = |mode| match &body(&model(&f.state(), mode))[0] {
        Block::Table(t) => t.rows.len(),
        _ => 0,
    };
    assert_eq!(rows(Mode::Final), 2);
    assert_eq!(rows(Mode::Original), 2);
}

#[test]
fn a_section_break_is_a_line_after_its_paragraph_with_the_next_sections_type() {
    let sect = |inner: &str| {
        format!(
            "<w:p><w:pPr><w:sectPr>{inner}</w:sectPr></w:pPr>{}</w:p>",
            r("end of one")
        )
    };
    let body = format!(
        "{}{}{}",
        p("a"),
        sect("<w:type w:val=\"nextPage\"/>"),
        p("b")
    );
    let f = Fixture::new(&body).sect("<w:type w:val=\"continuous\"/><w:titlePg/>");
    let m = redline(&f);
    let blocks = super::testing::body(&m);
    assert_eq!(blocks.len(), 4);
    let Block::Section(line) = &blocks[2] else {
        panic!("{:?}", blocks[2])
    };
    assert_eq!(line.index, 1);
    assert_eq!(line.break_type, crate::wordml::SectionBreak::Continuous);
    assert!(line.title_pg);
    assert_eq!(m.head.summary.sections, 2);
}

#[test]
fn notes_are_numbered_by_the_order_of_their_references_and_an_unreferenced_one_follows() {
    let note = |id: u32, t: &str| {
        format!(
            "<w:footnote w:id=\"{id}\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> {t}</w:t></w:r></w:p></w:footnote>"
        )
    };
    let reference = |id: u32| format!("<w:r><w:footnoteReference w:id=\"{id}\"/></w:r>");
    let body = format!(
        "{}{}",
        px(&format!("{}{}", r("one"), reference(5))),
        px(&format!("{}{}{}", r("two"), reference(3), r(" tail")))
    );
    let f = Fixture::new(&body).footnotes(&format!(
        "{}{}{}",
        note(3, "second"),
        note(5, "first"),
        note(9, "orphan")
    ));
    let m = redline(&f);
    let Inline::Atom(Atom::NoteRef { kind, ordinal }) = &para(&m, 0).inlines[1] else {
        panic!("{:?}", para(&m, 0).inlines)
    };
    assert_eq!((*kind, *ordinal), (NoteKind::Footnote, 1));
    assert!(matches!(
        &para(&m, 1).inlines[1],
        Inline::Atom(Atom::NoteRef { ordinal: 2, .. })
    ));
    let notes = only_story(&m, StoryKind::Footnotes);
    let got: Vec<(u32, Option<i64>)> = notes.units.iter().map(|u| (u.ordinal, u.id)).collect();
    assert_eq!(got, vec![(1, Some(5)), (2, Some(3)), (3, Some(9))]);
    assert_eq!(m.head.summary.footnotes, 3);
    let Block::Paragraph(first_note) = &notes.units[0].blocks[0] else {
        panic!()
    };
    assert!(matches!(
        &first_note.inlines[0],
        Inline::Atom(Atom::NoteMark {
            kind: NoteKind::Footnote
        })
    ));
    assert!(
        m.head
            .warnings
            .iter()
            .any(|w| w.code == crate::api::view::WarningCode::UnreferencedNote)
    );
}

#[test]
fn a_header_part_is_one_story_with_every_section_that_names_it() {
    let ref_h =
        |id: &str, kind: &str| format!("<w:headerReference w:type=\"{kind}\" r:id=\"{id}\"/>");
    let sect = |inner: &str| {
        format!(
            "<w:p><w:pPr><w:sectPr>{inner}</w:sectPr></w:pPr>{}</w:p>",
            r("end")
        )
    };
    let body = format!(
        "{}{}{}{}",
        sect(&ref_h("rIdH1", "default")),
        sect(&format!(
            "{}{}",
            ref_h("rIdH1", "default"),
            ref_h("rIdH2", "first")
        )),
        sect(""),
        p("last")
    );
    let f = Fixture::new(&body)
        .header(1, &p("shared header"))
        .header(2, &p("first page header"))
        .sect(&ref_h("rIdH1", "default"));
    let m = redline(&f);
    let one = stories(&m)
        .iter()
        .find(|s| s.part.as_ref() == "word/header1.xml")
        .unwrap();
    assert_eq!(
        one.uses,
        vec![
            SectionUse {
                section: 1,
                slot: Slot::Default
            },
            SectionUse {
                section: 2,
                slot: Slot::Default
            },
            SectionUse {
                section: 4,
                slot: Slot::Default
            },
        ]
    );
    let two = stories(&m)
        .iter()
        .find(|s| s.part.as_ref() == "word/header2.xml")
        .unwrap();
    assert_eq!(
        two.uses,
        vec![SectionUse {
            section: 2,
            slot: Slot::First
        }]
    );
    assert_eq!(one.kind, StoryKind::Header);
    assert_eq!(one.units[0].kind, UnitKind::Header);
}

#[test]
fn a_field_with_a_stored_result_is_one_read_only_inline_and_a_toc_collapses() {
    let field = |instr: &str, result: &str| {
        format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {instr} </w:instrText></w:r>\
             <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            r(result)
        )
    };
    let body = format!(
        "{}{}",
        px(&format!(
            "{}{}{}",
            r("within "),
            field("REF _Ref1 \\r", "30"),
            r(" days")
        )),
        p("tail")
    );
    let m = redline(&Fixture::new(&body));
    let Inline::Field(f) = &para(&m, 0).inlines[1] else {
        panic!("{:?}", para(&m, 0).inlines)
    };
    assert_eq!(
        (&*f.keyword, &*f.text, f.width, f.opens, f.closes),
        ("REF", "30", 2, true, true)
    );
    assert_eq!(redline_text(&para(&m, 0).inlines), "within 30 days");
    // A table of contents: one field over many paragraphs.
    let toc = format!(
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC \\o \"1-3\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}</w:p>{}{}{}\
         <w:p><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>",
        r("Entry one"),
        p("Entry two"),
        p("Entry three"),
        p("Entry four"),
    );
    let m = redline(&Fixture::new(&format!(
        "{}{}",
        p("intro"),
        toc + &p("after")
    )));
    let blocks = super::testing::body(&m);
    assert_eq!(blocks.len(), 3, "{blocks:?}");
    let Block::Opaque(OpaqueBlock::Collapsed {
        keyword,
        first,
        last,
        blocks: n,
    }) = &blocks[1]
    else {
        panic!("{:?}", blocks[1])
    };
    assert_eq!((&**keyword, *n), ("toc", 5));
    assert!(first.is_some() && last.is_some() && first != last);
}

#[test]
fn a_drawing_is_an_object_with_its_alt_text_and_math_a_placeholder_with_its_text() {
    let drawing = "<w:r><w:drawing><wp:inline><wp:docPr id=\"1\" name=\"Picture 1\" descr=\"A diagram of the deal\"/>\
                   <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic/></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>";
    let math = "<m:oMath><m:r><m:t>x</m:t></m:r><m:r><m:t>+1</m:t></m:r></m:oMath>";
    let m = redline(&Fixture::new(&px(&format!(
        "{}{}{}",
        r("see "),
        drawing,
        math
    ))));
    let inlines = &para(&m, 0).inlines;
    let Inline::Atom(Atom::Object(info)) = &inlines[1] else {
        panic!("{inlines:?}")
    };
    assert_eq!(info.kind, ObjectKind::Image);
    assert_eq!(info.alt.as_deref(), Some("A diagram of the deal"));
    assert!(matches!(&inlines[2], Inline::Atom(Atom::Math(t)) if &**t == "x+1"));
}
