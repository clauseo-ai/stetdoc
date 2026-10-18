//! Tests of `Story::table_site`: the innermost table around a paragraph, its style and look, the row
//! and column of the cell, spans, merges and `w:cnfStyle`. Each test says what it proves.

use super::names::W_S;
use super::testing::{document, parse};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

fn scan(body: &str) -> (SpanTree, Story) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (tree, story)
}

fn para(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

/// The entry index of the paragraph whose text is `text`.
fn entry_of(tree: &SpanTree, story: &Story, text: &str) -> usize {
    (0..story.blocks().len())
        .find(|&i| {
            story
                .paragraph(tree, i)
                .is_some_and(|p| p.text(Mode::Final) == text)
        })
        .unwrap_or_else(|| panic!("no paragraph {text}"))
}

fn site(tree: &SpanTree, story: &Story, text: &str) -> TableSite {
    story
        .table_site(tree, entry_of(tree, story, text))
        .unwrap_or_else(|| panic!("{text} is in no table"))
}

#[test]
fn a_plain_table_gives_the_position_of_a_cell() {
    // Proves: the style id, the row index and count, the column and column count (from the grid),
    // the span, the merge state and the two `cnfStyle` values are read for a paragraph in a cell.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid1\"/><w:tblLook w:val=\"04A0\"/></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"1\"/><w:gridCol w:w=\"1\"/><w:gridCol w:w=\"1\"/></w:tblGrid>\
         <w:tr><w:trPr><w:cnfStyle w:val=\"100000000000\"/></w:trPr>\
           <w:tc>{a}</w:tc><w:tc><w:tcPr><w:cnfStyle w:val=\"001000000000\"/></w:tcPr>{b}</w:tc><w:tc>{c}</w:tc></w:tr>\
         <w:tr><w:tc>{d}</w:tc><w:tc>{e}</w:tc><w:tc>{f}</w:tc></w:tr>\
         <w:tr><w:tc>{g}</w:tc><w:tc>{h}</w:tc><w:tc>{i}</w:tc></w:tr>\
         </w:tbl>",
        a = para("a"),
        b = para("b"),
        c = para("c"),
        d = para("d"),
        e = para("e"),
        f = para("f"),
        g = para("g"),
        h = para("h"),
        i = para("i"),
    ));
    let b = site(&tree, &story, "b");
    assert_eq!(b.style_id.as_deref(), Some("Grid1"));
    assert_eq!((b.row, b.row_count), (0, 3));
    assert_eq!((b.column, b.column_count), (1, 3));
    assert_eq!(b.span, 1);
    assert_eq!(b.v_merge, VMerge::None);
    assert_eq!(b.row_cnf.as_deref(), Some("100000000000"));
    assert_eq!(b.cell_cnf.as_deref(), Some("001000000000"));
    let h = site(&tree, &story, "h");
    assert_eq!((h.row, h.column), (2, 1));
    assert_eq!((h.row_cnf, h.cell_cnf), (None, None));
    let c = site(&tree, &story, "c");
    assert_eq!((c.row, c.column, c.cell_cnf), (0, 2, None));
}

#[test]
fn a_paragraph_outside_a_table_or_that_is_not_a_paragraph_has_no_site() {
    // Proves: `None` for a paragraph in no table, for an entry that is a table or a row, and for an
    // index past the end.
    let (tree, story) = scan(&format!(
        "{}<w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>{}",
        para("before"),
        para("in"),
        para("after")
    ));
    assert!(
        story
            .table_site(&tree, entry_of(&tree, &story, "before"))
            .is_none()
    );
    assert!(
        story
            .table_site(&tree, entry_of(&tree, &story, "after"))
            .is_none()
    );
    assert!(
        story
            .table_site(&tree, entry_of(&tree, &story, "in"))
            .is_some()
    );
    for (i, e) in story.blocks().iter().enumerate() {
        if e.kind != BlockKind::Paragraph {
            assert!(story.table_site(&tree, i).is_none(), "entry {i}");
        }
    }
    assert!(story.table_site(&tree, 99).is_none());
}

#[test]
fn a_paragraph_straight_in_a_row_or_table_is_not_in_a_cell() {
    // Proves: a `w:p` standing in a row or table with no cell is not a block (the scan lists it as
    // opaque), so it has no site, and the paragraphs in real cells are unaffected.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tr><w:p/><w:tc>{}</w:tc></w:tr><w:p/></w:tbl>",
        para("cell")
    ));
    let paragraphs = story
        .blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph)
        .count();
    assert_eq!(paragraphs, 1);
    assert_eq!(site(&tree, &story, "cell").column, 0);
}

#[test]
fn in_a_nested_table_the_innermost_table_decides() {
    // Proves: a paragraph in a table inside a cell takes the inner table's style, look, rows and
    // columns, and nothing of the outer table's; a paragraph in the outer cell, beside the nested
    // table, takes the outer table's.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Outer\"/><w:tblLook w:firstRow=\"1\"/></w:tblPr>\
         <w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr><w:tc>{o1}</w:tc>\
           <w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>{o2}\
             <w:tbl><w:tblPr><w:tblStyle w:val=\"Inner\"/><w:tblLook w:lastColumn=\"1\"/></w:tblPr>\
               <w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>\
               <w:tr><w:tc>{i1}</w:tc><w:tc>{i2}</w:tc></w:tr>\
               <w:tr><w:tc>{i3}</w:tc><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr>{i4}</w:tc></w:tr>\
             </w:tbl>\
           </w:tc></w:tr>\
         <w:tr><w:tc>{o3}</w:tc></w:tr></w:tbl>",
        o1 = para("o1"),
        o2 = para("o2"),
        o3 = para("o3"),
        i1 = para("i1"),
        i2 = para("i2"),
        i3 = para("i3"),
        i4 = para("i4"),
    ));
    let i4 = site(&tree, &story, "i4");
    assert_eq!(i4.style_id.as_deref(), Some("Inner"));
    assert_eq!(
        i4.look,
        TblLook {
            last_column: true,
            ..TblLook::default()
        }
    );
    assert_eq!(
        (i4.row, i4.row_count, i4.column, i4.column_count),
        (1, 2, 1, 2)
    );
    assert_eq!((i4.span, i4.v_merge), (1, VMerge::Restart));
    assert_eq!(site(&tree, &story, "i1").style_id.as_deref(), Some("Inner"));
    let o2 = site(&tree, &story, "o2");
    assert_eq!(o2.style_id.as_deref(), Some("Outer"));
    assert!(o2.look.first_row && !o2.look.last_column);
    assert_eq!(
        (o2.row, o2.row_count, o2.column, o2.column_count),
        (0, 2, 1, 4)
    );
    assert_eq!(o2.span, 2);
    assert_eq!(site(&tree, &story, "o3").row, 1);
}

#[test]
fn a_nested_table_with_no_style_does_not_take_the_outer_one() {
    // Proves: an inner table with no `w:tblStyle` or `w:tblLook` has none in its site, even when the
    // outer table has both.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Outer\"/><w:tblLook w:val=\"0020\"/></w:tblPr>\
         <w:tr><w:tc><w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>",
        para("x")
    ));
    let x = site(&tree, &story, "x");
    assert_eq!(x.style_id, None);
    assert_eq!(x.look, TblLook::default());
}

#[test]
fn grid_span_and_grid_before_place_the_column() {
    // Proves: the column is `gridBefore` plus the spans of the earlier cells, `span` is the cell's
    // own `gridSpan`, and a missing, negative or invalid `gridBefore` or `gridSpan` counts as 0 and 1.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblGrid><w:gridCol/><w:gridCol/><w:gridCol/><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr><w:trPr><w:gridBefore w:val=\"1\"/></w:trPr>\
           <w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>{a}</w:tc><w:tc>{b}</w:tc><w:tc><w:tcPr><w:gridSpan w:val=\"x\"/></w:tcPr>{c}</w:tc></w:tr>\
         <w:tr><w:trPr><w:gridBefore w:val=\"-1\"/></w:trPr><w:tc>{d}</w:tc></w:tr>\
         <w:tr><w:trPr><w:gridBefore w:val=\"many\"/></w:trPr><w:tc>{e}</w:tc></w:tr>\
         <w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"0\"/></w:tcPr>{f}</w:tc><w:tc>{g}</w:tc></w:tr>\
         </w:tbl>",
        a = para("a"),
        b = para("b"),
        c = para("c"),
        d = para("d"),
        e = para("e"),
        f = para("f"),
        g = para("g"),
    ));
    let cols = |t: &str| {
        let s = site(&tree, &story, t);
        (s.column, s.span)
    };
    assert_eq!(cols("a"), (1, 2));
    assert_eq!(cols("b"), (3, 1));
    assert_eq!(cols("c"), (4, 1));
    assert_eq!(cols("d"), (0, 1));
    assert_eq!(cols("e"), (0, 1));
    assert_eq!(cols("f"), (0, 1));
    assert_eq!(cols("g"), (1, 1));
    assert_eq!(site(&tree, &story, "a").column_count, 5);
}

#[test]
fn the_look_is_read_from_attributes_when_any_is_written() {
    // Proves: if any of the six `w:tblLook` attributes is written the six are read with the on/off
    // rule and an absent one is false, even when `w:val` says otherwise.
    let look = |attrs: &str| {
        let (tree, story) = scan(&format!(
            "<w:tbl><w:tblPr><w:tblLook {attrs}/></w:tblPr><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
            para("x")
        ));
        site(&tree, &story, "x").look
    };
    assert_eq!(
        look(
            "w:firstRow=\"1\" w:lastRow=\"true\" w:firstColumn=\"on\" w:lastColumn=\"1\" w:noHBand=\"1\" w:noVBand=\"1\""
        ),
        TblLook {
            first_row: true,
            last_row: true,
            first_column: true,
            last_column: true,
            no_h_band: true,
            no_v_band: true
        }
    );
    assert_eq!(
        look("w:firstRow=\"0\" w:noHBand=\"1\" w:lastRow=\"off\" w:noVBand=\"false\""),
        TblLook {
            no_h_band: true,
            ..TblLook::default()
        }
    );
    // The attributes win over `w:val`, and an absent attribute is false.
    assert_eq!(
        look("w:val=\"07E0\" w:lastColumn=\"1\""),
        TblLook {
            last_column: true,
            ..TblLook::default()
        }
    );
}

#[test]
fn the_look_is_decoded_from_the_hex_value_otherwise() {
    // Proves: with no attribute, `w:val` is hexadecimal with 0x0020 first row, 0x0040 last row, 0x0080
    // first column, 0x0100 last column, 0x0200 no horizontal band and 0x0400 no vertical band, and a
    // bad or missing value, or no `w:tblLook`, gives all off.
    let look = |tbl_pr: &str| {
        let (tree, story) = scan(&format!(
            "<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
            para("x")
        ));
        site(&tree, &story, "x").look
    };
    let flags = |l: TblLook| {
        [
            l.first_row,
            l.last_row,
            l.first_column,
            l.last_column,
            l.no_h_band,
            l.no_v_band,
        ]
    };
    let val = |v: &str| flags(look(&format!("<w:tblLook w:val=\"{v}\"/>")));
    assert_eq!(val("04A0"), [true, false, true, false, false, true]);
    assert_eq!(val("0020"), [true, false, false, false, false, false]);
    assert_eq!(val("0040"), [false, true, false, false, false, false]);
    assert_eq!(val("0080"), [false, false, true, false, false, false]);
    assert_eq!(val("0100"), [false, false, false, true, false, false]);
    assert_eq!(val("0200"), [false, false, false, false, true, false]);
    assert_eq!(val("0400"), [false, false, false, false, false, true]);
    assert_eq!(val("0660"), [true, true, false, false, true, true]);
    assert_eq!(val("04a0"), [true, false, true, false, false, true]);
    assert_eq!(val("0000"), [false; 6]);
    for bad in ["zz", "", "0x04A0", "-1", "04 A0", "FFFFFFFFFF"] {
        assert_eq!(val(bad), [false; 6], "value {bad:?}");
    }
    assert_eq!(flags(look("<w:tblLook/>")), [false; 6]);
    assert_eq!(flags(look("")), [false; 6]);
}

#[test]
fn a_paragraph_in_a_cell_through_wrappers_has_a_site() {
    // Proves: the cell may be reached through a `w:sdt` in the row, a `w:sdt`, `w:customXml` and
    // `w:smartTag` in the cell, the row itself may be in a `w:sdt` in the table, a revision wrapper
    // around a row still counts as a row, and the whole table may be in a control.
    let (tree, story) = scan(&format!(
        "<w:sdt><w:sdtContent><w:tbl><w:tblPr><w:tblStyle w:val=\"S\"/></w:tblPr>\
         <w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr><w:tc>{a}</w:tc>\
           <w:sdt><w:sdtContent><w:tc><w:sdt><w:sdtContent>{b}</w:sdtContent></w:sdt></w:tc></w:sdtContent></w:sdt></w:tr>\
         <w:sdt><w:sdtContent><w:tr><w:tc><w:customXml w:element=\"x\"><w:smartTag w:element=\"y\">{c}</w:smartTag></w:customXml></w:tc></w:tr></w:sdtContent></w:sdt>\
         <w:ins w:id=\"1\" w:author=\"a\"><w:tr><w:tc>{d}</w:tc></w:tr></w:ins>\
         </w:tbl></w:sdtContent></w:sdt>",
        a = para("a"),
        b = para("b"),
        c = para("c"),
        d = para("d"),
    ));
    let b = site(&tree, &story, "b");
    assert_eq!((b.row, b.column, b.row_count, b.column_count), (0, 1, 3, 2));
    assert_eq!(b.style_id.as_deref(), Some("S"));
    let c = site(&tree, &story, "c");
    assert_eq!((c.row, c.column), (1, 0));
    let d = site(&tree, &story, "d");
    assert_eq!((d.row, d.row_count), (2, 3));
}

#[test]
fn a_missing_grid_is_measured_from_the_widest_row() {
    // Proves: with no `w:tblGrid`, or an empty one, the column count is the largest of `gridBefore`
    // plus the spans plus `gridAfter` over the rows, and a grid that is there wins over the rows.
    let body = |grid: &str| {
        format!(
            "<w:tbl>{grid}\
             <w:tr><w:tc>{}</w:tc><w:tc/></w:tr>\
             <w:tr><w:trPr><w:gridBefore w:val=\"1\"/><w:gridAfter w:val=\"2\"/></w:trPr>\
               <w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr>{}</w:tc></w:tr></w:tbl>",
            para("a"),
            para("b")
        )
    };
    for grid in ["", "<w:tblGrid/>"] {
        let (tree, story) = scan(&body(grid));
        assert_eq!(site(&tree, &story, "a").column_count, 5, "grid {grid:?}");
        assert_eq!(site(&tree, &story, "b").column_count, 5);
    }
    let (tree, story) = scan(&body("<w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>"));
    assert_eq!(site(&tree, &story, "a").column_count, 2);
}

#[test]
fn a_vertical_merge_is_carried_to_the_site() {
    // Proves: `v_merge` and `span` are those of the cell holding the paragraph.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tr><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr>{a}</w:tc></w:tr>\
         <w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/><w:vMerge/></w:tcPr>{b}</w:tc></w:tr></w:tbl>",
        a = para("a"),
        b = para("b")
    ));
    assert_eq!(site(&tree, &story, "a").v_merge, VMerge::Restart);
    let b = site(&tree, &story, "b");
    assert_eq!((b.v_merge, b.span), (VMerge::Continue, 2));
}

#[test]
fn every_paragraph_of_a_cell_has_the_same_site() {
    // Proves: two paragraphs in one cell give the same site, and the paragraph after a nested table in
    // the same cell is still in the outer table.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Outer\"/></w:tblPr><w:tr><w:tc>{a}\
         <w:tbl><w:tblPr><w:tblStyle w:val=\"Inner\"/></w:tblPr><w:tr><w:tc>{b}</w:tc></w:tr></w:tbl>{c}</w:tc></w:tr></w:tbl>",
        a = para("a"),
        b = para("b"),
        c = para("c"),
    ));
    assert_eq!(site(&tree, &story, "a"), site(&tree, &story, "c"));
    assert_eq!(site(&tree, &story, "c").style_id.as_deref(), Some("Outer"));
    assert_eq!(site(&tree, &story, "b").style_id.as_deref(), Some("Inner"));
}

#[test]
fn a_strict_table_has_a_site() {
    // Proves: every name is read in the Strict `w` namespace: style, look attributes, row and cell
    // properties, grid, spans, merges and `cnfStyle`, and a nested Strict table decides for itself.
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\"><w:body>\
             <w:tbl><w:tblPr><w:tblStyle w:val=\"StrictOuter\"/><w:tblLook w:firstRow=\"1\"/></w:tblPr>\
             <w:tblGrid><w:gridCol w:w=\"1\"/><w:gridCol w:w=\"1\"/><w:gridCol w:w=\"1\"/></w:tblGrid>\
             <w:tr><w:trPr><w:gridBefore w:val=\"1\"/><w:cnfStyle w:val=\"100000000000\"/></w:trPr>\
             <w:tc><w:tcPr><w:gridSpan w:val=\"2\"/><w:vMerge w:val=\"restart\"/><w:cnfStyle w:val=\"000100000000\"/></w:tcPr>\
             <w:sdt><w:sdtContent><w:p><w:r><w:t>s</w:t></w:r></w:p></w:sdtContent></w:sdt>\
             <w:tbl><w:tblPr><w:tblStyle w:val=\"StrictInner\"/><w:tblLook w:val=\"0080\"/></w:tblPr>\
             <w:tr><w:tc><w:p><w:r><w:t>n</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc></w:tr></w:tbl>\
             </w:body></w:document>"
        ),
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let s = site(&tree, &story, "s");
    assert_eq!(s.style_id.as_deref(), Some("StrictOuter"));
    assert!(s.look.first_row);
    assert_eq!((s.row, s.row_count, s.column, s.column_count), (0, 1, 1, 3));
    assert_eq!((s.span, s.v_merge), (2, VMerge::Restart));
    assert_eq!(s.row_cnf.as_deref(), Some("100000000000"));
    assert_eq!(s.cell_cnf.as_deref(), Some("000100000000"));
    let n = site(&tree, &story, "n");
    assert_eq!(n.style_id.as_deref(), Some("StrictInner"));
    assert!(n.look.first_column && !n.look.first_row);
}

#[test]
fn a_header_story_has_sites_too() {
    // Proves: `table_site` works in any story, here a header, because it reads the entry list and the
    // tree and not the body.
    let tree = super::testing::part(
        "word/header1.xml",
        "hdr",
        &format!(
            "<w:tbl><w:tblPr><w:tblStyle w:val=\"H\"/></w:tblPr><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
            para("h")
        ),
    );
    let story = Story::scan(&Arc::from("word/header1.xml"), StoryKind::Header, &tree).unwrap();
    assert_eq!(site(&tree, &story, "h").style_id.as_deref(), Some("H"));
}

#[test]
fn table_sites_gives_every_entry_the_same_answer_as_table_site() {
    // Proves: the bulk call agrees with the single call for every entry, including entries that are
    // not paragraphs, paragraphs outside tables, and nested tables.
    let (tree, story) = scan(
        "<w:p/><w:tbl><w:tblPr><w:tblStyle w:val=\"Outer\"/></w:tblPr><w:tblGrid><w:gridCol/><w:gridCol/></w:tblGrid>\
         <w:tr><w:tc><w:p/></w:tc><w:tc><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:tc></w:tr>\
         <w:tr><w:trPr><w:gridBefore w:val=\"1\"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>",
    );
    let all = story.table_sites(&tree);
    assert_eq!(all.len(), story.blocks().len());
    for (i, site) in all.iter().enumerate() {
        assert_eq!(site, &story.table_site(&tree, i), "entry {i}");
    }
    assert!(all.iter().filter(|s| s.is_some()).count() >= 3);
}

#[test]
fn a_table_of_five_thousand_rows_is_read_in_one_pass() {
    // Proves: `table_sites` does the per-table work once, so a large table is linear. A call per
    // paragraph would walk 5,000 rows 5,000 times.
    let rows: String = (0..5_000)
        .map(|_| "<w:tr><w:tc><w:p/></w:tc></w:tr>")
        .collect();
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblGrid><w:gridCol/></w:tblGrid>{rows}</w:tbl>"
    ));
    let all = story.table_sites(&tree);
    let last = all.last().unwrap().as_ref().unwrap();
    assert_eq!((last.row, last.row_count), (4_999, 5_000));
}

#[test]
fn a_row_of_twenty_thousand_cells_is_read_in_one_pass() {
    // Proves: `table_sites` reads each row once, so a row with 20,000 cells, each holding a
    // paragraph, is linear. Asking for each cell's start by walking the row would be quadratic.
    let cells: String = (0..20_000).map(|_| "<w:tc><w:p/></w:tc>").collect();
    let (tree, story) = scan(&format!("<w:tbl><w:tr>{cells}</w:tr></w:tbl>"));
    let all = story.table_sites(&tree);
    let last = all.last().unwrap().as_ref().unwrap();
    assert_eq!(
        (last.row, last.column, last.column_count),
        (0, 19_999, 20_000)
    );
    assert_eq!(story.blocks().len(), 20_002);
}
