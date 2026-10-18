//! Tests of `insert_row` and of deleting rows and tables (design 3.4).

use super::stamp::Track;
use super::tests_support::*;
use super::types::Place;
use crate::api::error::ErrorCode;

fn cell(w: u32, extra: &str, text: &str) -> String {
    format!(
        "<w:tc><w:tcPr><w:tcW w:w=\"{w}\" w:type=\"dxa\"/>{extra}</w:tcPr><w:p>{}</w:p></w:tc>",
        if text.is_empty() {
            String::new()
        } else {
            r(text)
        }
    )
}

fn table(rows: &[String]) -> String {
    format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid>{}</w:tbl><w:p>{}</w:p>",
        rows.concat(),
        r("after")
    )
}

fn row(a: &str, b: &str) -> String {
    format!("<w:tr>{}{}</w:tr>", cell(3000, "", a), cell(3000, "", b))
}

fn texts_in(xml: &str) -> Vec<String> {
    xml.split("<w:t>")
        .skip(1)
        .map(|s| s.split('<').next().unwrap().to_owned())
        .collect()
}

#[test]
fn a_new_row_is_a_copy_of_the_anchor_with_one_inserted_paragraph_in_each_cell() {
    let body = table(&[row("a", "b")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::After, &["x".to_owned(), "y".to_owned()])]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:tr>").count(), 2, "{}", s.xml);
    assert!(s.xml.contains(&format!("<w:tr><w:trPr><w:ins w:id=\"0\" {AT}/></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/></w:tcPr><w:p><w:pPr><w:rPr><w:ins w:id=\"1\"")), "{}", s.xml);
    assert_eq!(texts_in(&s.xml), ["x", "y"]);
    assert_eq!(s.fin, ["a", "b", "x", "y", "after"]);
    let fx = s.effect(0);
    assert_eq!(fx.new_blocks.len(), 3, "the row and its two paragraphs");
}

#[test]
fn a_new_row_before_goes_before_and_fewer_texts_leave_the_rest_empty() {
    let body = table(&[row("a", "b")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::Before, &["x".to_owned()])]
    });
    s.assert_clean();
    assert!(
        s.xml.find("<w:trPr><w:ins").unwrap() < s.xml.find(">a<").unwrap(),
        "{}",
        s.xml
    );
    assert_eq!(s.fin, ["x", "", "a", "b", "after"]);
}

#[test]
fn too_many_texts_for_a_row_are_refused() {
    let body = table(&[row("a", "b")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(
            &all[1],
            Place::After,
            &["x".to_owned(), "y".to_owned(), "z".to_owned()],
        )]
    });
    assert_eq!(s.code(0), Some(ErrorCode::InvalidPayload));
}

#[test]
fn the_anchor_of_a_row_must_be_a_row() {
    let body = table(&[row("a", "b")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[0], Place::After, &[])]
    });
    assert_eq!(s.code(0), Some(ErrorCode::CrossBlock));
}

#[test]
fn a_row_copy_leaves_out_the_anchors_revisions() {
    let anchor = format!(
        "<w:tr><w:trPr><w:ins {}/><w:trPrChange {}><w:trPr/></w:trPrChange></w:trPr><w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:cellIns {}/><w:tcPrChange {}><w:tcPr/></w:tcPrChange></w:tcPr><w:p>{}</w:p></w:tc><w:tc><w:p>{}</w:p></w:tc></w:tr>",
        eric(800),
        eric(801),
        eric(802),
        eric(803),
        r("a"),
        r("b")
    );
    let s = with_compiler(&table(&[anchor]), Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::After, &[])]
    });
    s.assert_clean();
    let at = s.xml.rfind("<w:tr>").unwrap();
    let copy = &s.xml[at..];
    assert!(
        !copy.contains("Eric") && !copy.contains("trPrChange") && !copy.contains("cellIns"),
        "{copy}"
    );
}

#[test]
fn a_vertical_merge_continues_below_and_is_not_restarted() {
    let merged = |first: &str, second: &str| {
        format!(
            "<w:tr>{}{}</w:tr>",
            cell(3000, first, "a"),
            cell(3000, second, "b")
        )
    };
    let body = table(&[
        merged("<w:vMerge w:val=\"restart\"/>", ""),
        merged("<w:vMerge/>", ""),
    ]);
    let after_restart = with_compiler(&body, Track::Tracked(stamp()), |c, _, _| {
        let rows = keys_of(c.tx(), crate::wordml::BlockKind::Row);
        vec![c.insert_row(&rows[0], Place::After, &[])]
    });
    after_restart.assert_clean();
    let new_row = &after_restart.xml[after_restart.xml.find("<w:trPr><w:ins").unwrap()..];
    assert!(
        new_row.contains("<w:vMerge/>") && !new_row.contains("restart"),
        "{}",
        after_restart.xml
    );
    let before_restart = with_compiler(&body, Track::Tracked(stamp()), |c, _, _| {
        let rows = keys_of(c.tx(), crate::wordml::BlockKind::Row);
        vec![c.insert_row(&rows[0], Place::Before, &[])]
    });
    before_restart.assert_clean();
    let new_row = &before_restart.xml[before_restart.xml.find("<w:trPr><w:ins").unwrap()..];
    let end = new_row.find("</w:tr>").unwrap();
    assert!(!new_row[..end].contains("vMerge"), "{}", before_restart.xml);
    let before_continue = with_compiler(&body, Track::Tracked(stamp()), |c, _, _| {
        let rows = keys_of(c.tx(), crate::wordml::BlockKind::Row);
        vec![c.insert_row(&rows[1], Place::Before, &[])]
    });
    before_continue.assert_clean();
    assert_eq!(
        before_continue.xml.matches("<w:vMerge/>").count(),
        2,
        "{}",
        before_continue.xml
    );
}

#[test]
fn deleting_a_row_deletes_each_paragraph_with_its_mark_and_marks_the_row() {
    let body = table(&[row("a", "b"), row("c", "d")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.delete_block(&all[1])]
    });
    s.assert_clean();
    assert!(
        s.xml
            .contains(&format!("<w:tr><w:trPr><w:del w:id=\"4\" {AT}/></w:trPr>")),
        "{}",
        s.xml
    );
    assert_eq!(
        s.xml.matches("<w:rPr><w:del ").count(),
        2,
        "the last paragraph of each cell has its mark deleted too: {}",
        s.xml
    );
    assert_eq!(s.fin, ["", "", "c", "d", "after"]);
}

#[test]
fn deleting_a_table_deletes_every_row() {
    let body = table(&[row("a", "b"), row("c", "d")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.delete_block(&all[0])]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:trPr><w:del ").count(), 2, "{}", s.xml);
    assert_eq!(s.fin, ["", "", "", "", "after"]);
}

#[test]
fn a_row_another_author_inserted_gets_our_del_after_their_ins() {
    let theirs = format!(
        "<w:tr><w:trPr><w:ins {}/></w:trPr><w:tc><w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr><w:ins {}><w:r><w:t>a</w:t></w:r></w:ins></w:p></w:tc><w:tc><w:p/></w:tc></w:tr>",
        eric(810),
        eric(811),
        eric(812)
    );
    let s = with_compiler(&table(&[theirs]), Track::Tracked(stamp()), |c, all, _| {
        vec![c.delete_block(&all[1])]
    });
    s.assert_clean();
    assert!(
        s.xml
            .contains(&format!("<w:trPr><w:ins {}/><w:del w:id=", eric(810))),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains(&format!("<w:ins {}><w:del ", eric(812))),
        "{}",
        s.xml
    );
}

#[test]
fn deleting_a_row_that_is_already_deleted_is_refused() {
    let gone = format!(
        "<w:tr><w:trPr><w:del {}/></w:trPr><w:tc><w:p/></w:tc></w:tr>",
        eric(820)
    );
    let s = with_compiler(&table(&[gone]), Track::Tracked(stamp()), |c, all, _| {
        vec![c.delete_block(&all[1])]
    });
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn a_row_the_author_inserted_with_content_that_is_hers_is_removed_outright_and_the_table_is_touched()
 {
    let body = table(&[row("a", "b")]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        let e = c.insert_row(&all[1], Place::After, &["x".to_owned(), "y".to_owned()]);
        let k = e.as_ref().unwrap().new_blocks[0].clone();
        vec![e, c.delete_block(&k)]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:tr>").count(), 1, "{}", s.xml);
    assert_eq!(s.fin, ["a", "b", "after"]);
    assert_eq!(s.effect(1).removed_own.len(), 1);
    assert!(s.outcome.as_ref().unwrap().created.is_empty());
}

#[test]
fn untracked_a_row_is_removed_and_a_table_with_no_row_goes() {
    let body = table(&[row("a", "b"), row("c", "d")]);
    let s = with_compiler(&body, Track::Untracked, |c, all, _| {
        vec![c.delete_block(&all[1])]
    });
    assert!(s.effects[0].is_ok());
    assert_eq!(s.xml.matches("<w:tr>").count(), 1, "{}", s.xml);
    let t = with_compiler(&body, Track::Untracked, |c, all, _| {
        vec![c.delete_block(&all[0])]
    });
    assert!(t.effects[0].is_ok());
    assert!(!t.xml.contains("<w:tbl>"), "{}", t.xml);
    assert_eq!(t.fin, ["after"]);
}

#[test]
fn a_new_row_gets_a_para_id_of_its_own_and_the_text_id_word_writes_when_the_model_row_has_ids() {
    // FA/007: Word writes `w14:paraId` and `w14:textId="77777777"` on a row it adds.
    let body = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid>\
                <w:tr w:rsidR=\"00AA11BB\" w14:paraId=\"11111111\" w14:textId=\"22222222\"><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let s = with_compiler(body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::After, &["x".to_owned()])]
    });
    s.assert_clean();
    let rows: Vec<&str> = s.xml.split("<w:tr ").skip(1).collect();
    assert_eq!(rows.len(), 2, "{}", s.xml);
    assert!(
        rows[0]
            .starts_with("w:rsidR=\"00AA11BB\" w14:paraId=\"11111111\" w14:textId=\"22222222\">"),
        "the row that was there keeps its ids: {}",
        s.xml
    );
    let new = rows.get(1).copied().unwrap_or_default();
    let head = new.split('>').next().unwrap_or_default();
    assert!(head.contains("w14:textId=\"77777777\""), "{head}");
    assert!(
        head.contains("w14:paraId=\"") && !head.contains("11111111"),
        "{head}"
    );
    assert!(!head.contains("rsid"), "{head}");
}

#[test]
fn a_cell_inside_a_content_control_gets_its_text_and_the_control_a_new_id() {
    // coverpage2: the second cell of the row is wrapped in a w:sdt. The new row has the control too, with an
    // id of its own, and no text of the anchor's is copied or lost.
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>{}<w:sdt><w:sdtPr><w:alias w:val=\"Year\"/><w:id w:val=\"15676118\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt></w:tr></w:tbl><w:p>{}</w:p>",
        cell(3000, "", "a"),
        cell(3000, "", "b"),
        r("after")
    );
    for track in [Track::Tracked(stamp()), Track::Untracked] {
        let s = with_compiler(&body, track, |c, all, _| {
            vec![c.insert_row(&all[1], Place::After, &["x".to_owned(), "y".to_owned()])]
        });
        assert!(s.effects[0].is_ok(), "{:?}", s.effects);
        assert_eq!(s.fin, ["a", "b", "x", "y", "after"], "{}", s.xml);
        assert_eq!(s.xml.matches("<w:sdt>").count(), 2, "{}", s.xml);
        assert_eq!(
            s.xml.matches("<w:id w:val=\"15676118\"/>").count(),
            1,
            "{}",
            s.xml
        );
        assert_eq!(
            s.xml.matches("<w:id w:val=\"15676119\"/>").count(),
            1,
            "{}",
            s.xml
        );
        // Every new paragraph of the row has an origin: the copy of each cell's first paragraph.
        assert_eq!(
            s.effect(0).new_blocks.len(),
            3,
            "the row and two paragraphs"
        );
        assert!(s.schema.is_empty(), "{:?}", s.schema);
    }
}

#[test]
fn a_copied_content_control_loses_its_data_binding() {
    // coverpage2: the control around a cell is bound to a custom XML part. The new row's copy keeps the
    // control but not the binding, so Word shows the inserted text and not the bound value; the anchor's
    // control keeps its binding.
    let binding = "<w:dataBinding w:prefixMappings=\"xmlns:ns0='http://example.com/'\" w:xpath=\"/ns0:Y[1]\" w:storeItemID=\"{55AF091B-3C7A-41E3-B477-F2FDAA23CFDA}\"/>";
    let body = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr>{}<w:sdt><w:sdtPr><w:alias w:val=\"Year\"/><w:id w:val=\"15676118\"/>{binding}</w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt></w:tr></w:tbl><w:p>{}</w:p>",
        cell(3000, "", "a"),
        cell(3000, "", "b"),
        r("after")
    );
    for track in [Track::Tracked(stamp()), Track::Untracked] {
        let s = with_compiler(&body, track, |c, all, _| {
            vec![c.insert_row(&all[1], Place::After, &["x".to_owned(), "y".to_owned()])]
        });
        assert!(s.effects[0].is_ok(), "{:?}", s.effects);
        assert!(s.outcome.is_ok(), "{:?}", s.outcome);
        assert_eq!(s.fin, ["a", "b", "x", "y", "after"], "{}", s.xml);
        assert_eq!(s.xml.matches("<w:sdt>").count(), 2, "{}", s.xml);
        assert_eq!(s.xml.matches("<w:dataBinding ").count(), 1, "{}", s.xml);
        let first = s.xml.find("<w:dataBinding ").unwrap();
        assert!(first < s.xml.find("15676119").unwrap(), "{}", s.xml);
        assert!(s.schema.is_empty(), "{:?}", s.schema);
    }
}

#[test]
fn a_continued_vertical_merge_keeps_the_cell_properties_in_order() {
    // The new row is built before it is placed, so its cells have no context from the root: their properties keep
    // the order of a cell anyway, w:vMerge before w:vAlign.
    let body = table(&[format!(
        "<w:tr>{}{}</w:tr>",
        cell(
            3000,
            "<w:vMerge w:val=\"restart\"/><w:vAlign w:val=\"center\"/>",
            "a"
        ),
        cell(3000, "", "b")
    )]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::After, &["x".to_owned()])]
    });
    s.assert_clean();
    assert!(
        s.xml.contains(
            "<w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:vMerge/><w:vAlign w:val=\"center\"/>"
        ),
        "{}",
        s.xml
    );
}

#[test]
fn untracked_a_removed_row_leaves_its_range_markers() {
    // A bookmark that starts in the row and ends after the table keeps its start: it stays between the rows, or before
    // the table when the table goes with its last row.
    let marked = format!(
        "<w:tr>{}{}</w:tr>",
        cell(3000, "", "a").replace("<w:p>", "<w:p><w:bookmarkStart w:id=\"5\" w:name=\"b\"/>"),
        cell(3000, "", "b")
    );
    let ended = |rows: &[String]| {
        table(rows).replace("</w:tbl><w:p>", "</w:tbl><w:p><w:bookmarkEnd w:id=\"5\"/>")
    };
    for (rows, left) in [
        (vec![marked.clone(), row("c", "d")], 1),
        (vec![marked.clone()], 0),
    ] {
        let body = ended(&rows);
        let s = with_compiler(&body, Track::Untracked, |c, all, _| {
            vec![c.delete_block(&all[1])]
        });
        assert!(s.effects[0].is_ok(), "{:?}", s.effects[0]);
        assert!(s.schema.is_empty(), "{:?} {}", s.schema, s.xml);
        assert!(
            s.xml.contains("<w:bookmarkStart w:id=\"5\" w:name=\"b\"/>"),
            "{}",
            s.xml
        );
        assert_eq!(s.xml.matches("<w:tr>").count(), left, "{}", s.xml);
    }
}

#[test]
fn a_new_row_does_not_copy_the_bookmarks_between_the_model_rows_cells() {
    let body = table(&[format!(
        "<w:tr><w:bookmarkStart w:id=\"0\" w:name=\"r\"/>{}<w:bookmarkEnd w:id=\"0\"/>{}</w:tr>",
        cell(3000, "", "a"),
        cell(3000, "", "b")
    )]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_row(&all[1], Place::After, &["x".to_owned()])]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:bookmarkStart").count(), 1, "{}", s.xml);
    assert_eq!(s.xml.matches("<w:bookmarkEnd").count(), 1, "{}", s.xml);
}

#[test]
fn untracked_a_removed_row_or_table_hoists_a_comments_anchor_so_the_comment_survives() {
    // As an accepted deletion does (design 3.6.5): the range markers stand where the row was, and the reference run,
    // which cannot stand between blocks, goes with them into the neighbouring paragraph, the one after the table
    // here, since nothing is before it.
    let anchored = format!(
        "<w:tr>{}{}</w:tr>",
        cell(3000, "", "a")
            .replace("<w:p>", "<w:p><w:commentRangeStart w:id=\"0\"/>")
            .replace(
                "</w:p>",
                "<w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r></w:p>"
            ),
        cell(3000, "", "b")
    );
    let anchor = "<w:commentRangeStart w:id=\"0\"/><w:commentRangeEnd w:id=\"0\"/><w:r><w:commentReference w:id=\"0\"/></w:r>";
    for (rows, left) in [
        (vec![anchored.clone(), row("c", "d")], 1),
        (vec![anchored.clone()], 0),
    ] {
        let body = table(&rows);
        let s = with_compiler(&body, Track::Untracked, |c, all, _| {
            vec![c.delete_block(&all[1])]
        });
        assert!(s.effects[0].is_ok(), "{:?}", s.effects[0]);
        assert!(s.schema.is_empty(), "{:?} {}", s.schema, s.xml);
        assert!(s.xml.contains(&format!("<w:p>{anchor}<w:r>")), "{}", s.xml);
        assert_eq!(s.xml.matches("<w:tr>").count(), left, "{}", s.xml);
        let fx = s.effect(0);
        assert!(
            !fx.touched.is_empty(),
            "the paragraph that took the anchor is touched"
        );
    }
    // A body that was only this table keeps a paragraph, which takes the anchor.
    let only = table(&[anchored]).replace(&format!("<w:p>{}</w:p>", r("after")), "");
    let s = with_compiler(&only, Track::Untracked, |c, all, _| {
        vec![c.delete_block(&all[0])]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects[0]);
    assert_eq!(s.xml, format!("<w:p>{anchor}</w:p>"));
    assert_eq!(s.effect(0).new_blocks.len(), 1);
    // And with no anchor, an empty one.
    let s = with_compiler(
        &table(&[row("a", "b")]).replace(&format!("<w:p>{}</w:p>", r("after")), ""),
        Track::Untracked,
        |c, all, _| vec![c.delete_block(&all[0])],
    );
    assert!(s.effects[0].is_ok(), "{:?}", s.effects[0]);
    assert_eq!(s.xml, "<w:p/>");
}
