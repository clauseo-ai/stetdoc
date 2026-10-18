//! Tests of the block views: tables, rows and cells read through transparent wrappers, the grid,
//! merges, and the revisions that are a row or a cell. Each test says what it proves.

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

/// The first table of the story, as a view.
fn table<'a>(tree: &'a SpanTree, story: &'a Story) -> Table<'a> {
    let i = story
        .blocks()
        .iter()
        .position(|e| e.kind == BlockKind::Table)
        .unwrap();
    match story.block(tree, i).unwrap() {
        Block::Table(t) => t,
        _ => panic!("entry {i} is not a table"),
    }
}

fn cell_texts(row: Row<'_>) -> Vec<String> {
    row.cells()
        .map(|c| {
            c.blocks()
                .filter_map(|b| match b {
                    Block::Paragraph(p) => Some(p.text(Mode::Final)),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn para(text: &str) -> String {
    format!("<w:p><w:r><w:t>{text}</w:t></w:r></w:p>")
}

#[test]
fn a_table_gives_its_rows_its_grid_and_its_properties() {
    // Proves: `rows` lists the `w:tr` elements, `grid` the `w:gridCol` widths with a missing or invalid
    // width as `None`, and `props` is a `PropsRef` over `w:tblPr`.
    let (tree, story) = scan(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/><w:tblW w:w=\"5000\" w:type=\"dxa\"/></w:tblPr>\
         <w:tblGrid><w:gridCol w:w=\"2400\"/><w:gridCol/><w:gridCol w:w=\"x\"/><w:gridCol w:w=\"100\"/></w:tblGrid>\
         <w:tr><w:tc><w:p/></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    let t = table(&tree, &story);
    assert_eq!(t.rows().count(), 2);
    assert_eq!(
        t.grid().collect::<Vec<_>>(),
        [Some(2400), None, None, Some(100)]
    );
    let props = t.props().unwrap();
    assert_eq!(props.val("tblStyle").as_deref(), Some("Grid"));
    assert_eq!(props.attr("tblW", "w").as_deref(), Some("5000"));
    assert!(t.grid_element().is_some());
}

#[test]
fn a_table_with_no_grid_or_properties_reads_as_empty() {
    // Proves: reading is total. No `w:tblGrid` is an empty grid and no `w:tblPr` is `None`.
    let (tree, story) = scan("<w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>");
    let t = table(&tree, &story);
    assert_eq!(t.grid().count(), 0);
    assert!(t.props().is_none());
    assert!(t.grid_element().is_none());
}

#[test]
fn rows_and_cells_are_found_through_wrappers_that_stand_among_them() {
    // Proves: a `w:tr` inside a `w:sdt` inside the table, in a `w:customXml`, in a tracked `w:ins`, and
    // a `w:tc` inside a row-level `w:sdt` or `w:smartTag`, are rows and cells, in document order, and
    // property elements and markers beside them are not.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tblPr/><w:tblGrid/>\
         <w:tr><w:trPr/>\
           <w:tc>{a}</w:tc>\
           <w:sdt><w:sdtPr/><w:sdtContent><w:tc>{b}</w:tc></w:sdtContent></w:sdt>\
           <w:smartTag w:element=\"x\"><w:tc>{c}</w:tc></w:smartTag>\
           <w:bookmarkStart w:id=\"1\" w:name=\"n\"/>\
         </w:tr>\
         <w:sdt><w:sdtPr/><w:sdtContent><w:tr><w:tc>{d}</w:tc></w:tr></w:sdtContent></w:sdt>\
         <w:customXml w:element=\"x\"><w:tr><w:tc>{e}</w:tc></w:tr></w:customXml>\
         <w:ins w:id=\"2\" w:author=\"a\"><w:tr><w:tc>{f}</w:tc></w:tr></w:ins>\
         </w:tbl>",
        a = para("a"),
        b = para("b"),
        c = para("c"),
        d = para("d"),
        e = para("e"),
        f = para("f"),
    ));
    let t = table(&tree, &story);
    let rows: Vec<_> = t.rows().collect();
    assert_eq!(rows.len(), 4);
    assert_eq!(cell_texts(rows[0]), ["a", "b", "c"]);
    assert_eq!(cell_texts(rows[1]), ["d"]);
    assert_eq!(cell_texts(rows[2]), ["e"]);
    assert_eq!(cell_texts(rows[3]), ["f"]);
}

#[test]
fn the_blocks_of_a_cell_are_found_through_wrappers_and_the_taken_branch() {
    // Proves: a cell's paragraphs and tables are found through a block-level `w:sdt`, nested ones,
    // `w:customXml`, a revision wrapper and the taken branch of an `mc:AlternateContent` (and not the
    // other branch), and a nested table is a `Block::Table`.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tr><w:tc><w:tcPr/>\
           {p1}\
           <w:sdt><w:sdtPr/><w:sdtContent><w:sdt><w:sdtContent>{p2}</w:sdtContent></w:sdt></w:sdtContent></w:sdt>\
           <w:customXml w:element=\"x\">{p3}</w:customXml>\
           <w:moveFrom w:id=\"1\" w:author=\"a\">{p4}</w:moveFrom>\
           <mc:AlternateContent>\
             <mc:Choice Requires=\"w14\">{p5}</mc:Choice>\
             <mc:Fallback>{p6}</mc:Fallback>\
           </mc:AlternateContent>\
           <w:tbl><w:tr><w:tc>{p7}</w:tc></w:tr></w:tbl>\
         </w:tc></w:tr></w:tbl>",
        p1 = para("1"),
        p2 = para("2"),
        p3 = para("3"),
        p4 = para("4"),
        p5 = para("5"),
        p6 = para("6"),
        p7 = para("7"),
    ));
    let t = table(&tree, &story);
    let row = t.rows().next().unwrap();
    let cell = row.cells().next().unwrap();
    let blocks: Vec<_> = cell.blocks().collect();
    assert_eq!(blocks.len(), 6);
    let texts: Vec<_> = blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.text(Mode::Final)),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["1", "2", "3", "", "5"]);
    assert_eq!(blocks[5].kind(), BlockKind::Table);
}

#[test]
fn views_agree_with_the_scan_on_which_blocks_a_row_holds() {
    // Proves: for every row entry, the blocks of its cells through the views are exactly the entries
    // whose parent is that row, in the same order. One reading of the structure, not two.
    let (tree, story) = scan(&format!(
        "<w:tbl><w:tr>\
           <w:tc>{a}<w:sdt><w:sdtContent>{b}</w:sdtContent></w:sdt></w:tc>\
           <w:sdt><w:sdtContent><w:tc><w:tbl><w:tr><w:tc>{c}</w:tc></w:tr></w:tbl>{d}</w:tc></w:sdtContent></w:sdt>\
         </w:tr><w:tr><w:tc/></w:tr></w:tbl>",
        a = para("a"),
        b = para("b"),
        c = para("c"),
        d = para("d"),
    ));
    let mut checked = 0;
    for (i, e) in story.blocks().iter().enumerate() {
        if e.kind != BlockKind::Row {
            continue;
        }
        let Some(Block::Row(row)) = story.block(&tree, i) else {
            panic!("entry {i} is not a row");
        };
        let from_views: Vec<NodeId> = row
            .cells()
            .flat_map(|c| c.blocks())
            .map(|b| b.id())
            .collect();
        let from_scan: Vec<NodeId> = story
            .blocks()
            .iter()
            .filter(|c| c.parent == u32::try_from(i).ok())
            .map(|c| c.node)
            .collect();
        assert_eq!(from_views, from_scan, "row entry {i}");
        checked += 1;
    }
    assert_eq!(checked, 3);
}

#[test]
fn grid_span_defaults_to_one_and_reads_a_valid_count() {
    // Proves: `w:gridSpan` gives the span, and a missing, zero, negative or non-numeric value is one.
    let (tree, story) = scan(
        "<w:tbl><w:tr>\
         <w:tc><w:tcPr><w:gridSpan w:val=\"3\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:p/></w:tc>\
         <w:tc><w:tcPr><w:gridSpan w:val=\"0\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:gridSpan w:val=\"-2\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:gridSpan w:val=\"two\"/></w:tcPr><w:p/></w:tc>\
         </w:tr></w:tbl>",
    );
    let spans: Vec<u32> = table(&tree, &story)
        .rows()
        .flat_map(|r| r.cells())
        .map(|c| c.grid_span())
        .collect();
    assert_eq!(spans, [3, 1, 1, 1, 1]);
}

#[test]
fn vertical_merge_is_restart_continue_or_none() {
    // Proves: `w:vMerge` with `restart` starts a region, one with `continue` or with no value
    // continues it, and a cell with none, or no `w:tcPr`, is not merged.
    let (tree, story) = scan(
        "<w:tbl><w:tr>\
         <w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:vMerge w:val=\"continue\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:tcW w:w=\"1\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:p/></w:tc>\
         </w:tr></w:tbl>",
    );
    let merges: Vec<VMerge> = table(&tree, &story)
        .rows()
        .flat_map(|r| r.cells())
        .map(|c| c.v_merge())
        .collect();
    assert_eq!(
        merges,
        [
            VMerge::Restart,
            VMerge::Continue,
            VMerge::Continue,
            VMerge::None,
            VMerge::None
        ]
    );
}

#[test]
fn a_row_inserted_by_one_author_and_deleted_by_another_carries_both() {
    // Proves: both revisions are read, in order, and the row is gone in `Final` and in `Original`.
    let (tree, story) = scan(
        "<w:tbl><w:tr><w:trPr><w:ins w:id=\"7\" w:author=\"ann\"/><w:del w:id=\"8\" w:author=\"bob\"/></w:trPr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    let row = table(&tree, &story).rows().next().unwrap();
    let revs: Vec<_> = row.revisions().map(|r| (r.kind, r.id)).collect();
    assert_eq!(
        revs,
        [(RevKind::RowIns, Some(7)), (RevKind::RowDel, Some(8))]
    );
    let marks = row.revs();
    assert!(marks.inserted && marks.deleted);
    assert!(marks.gone_in(Mode::Final) && marks.gone_in(Mode::Original));
    assert!(!marks.gone_in(Mode::Redline));
}

#[test]
fn a_row_that_is_a_tracked_insertion_or_deletion_says_so() {
    // Proves: `w:trPr/w:ins` is `RowIns` and `w:trPr/w:del` is `RowDel`, with the author and id read
    // by `revision_at`, and a row with neither (or a `w:ins` that is not in `w:trPr`) is no revision.
    let (tree, story) = scan(
        "<w:tbl>\
         <w:tr><w:trPr><w:ins w:id=\"7\" w:author=\"ann\" w:date=\"2024-01-01T00:00:00Z\"/></w:trPr><w:tc><w:p/></w:tc></w:tr>\
         <w:tr><w:trPr><w:cantSplit/><w:del w:id=\"8\" w:author=\"bob\"/></w:trPr><w:tc><w:p/></w:tc></w:tr>\
         <w:tr><w:trPr><w:cantSplit/></w:trPr><w:tc><w:p/></w:tc></w:tr>\
         <w:tr><w:tc><w:p/></w:tc></w:tr>\
         </w:tbl>",
    );
    let rows: Vec<_> = table(&tree, &story).rows().collect();
    let kinds = |i: usize| rows[i].revisions().map(|r| r.kind).collect::<Vec<_>>();
    assert_eq!(kinds(0), [RevKind::RowIns]);
    assert_eq!(kinds(1), [RevKind::RowDel]);
    assert!(kinds(2).is_empty());
    assert!(kinds(3).is_empty());
    let ins = rows[0].revisions().next().unwrap();
    assert_eq!((ins.id, ins.author.as_deref()), (Some(7), Some("ann")));
    assert_eq!(rows[1].revisions().next().unwrap().id, Some(8));
    assert!(rows[0].revs().inserted && !rows[0].revs().deleted);
    assert!(rows[2].revs().is_none());
    assert!(rows[2].props().unwrap().get("cantSplit").is_some());
    assert!(rows[3].props().is_none());
}

#[test]
fn a_row_exposes_its_table_property_exceptions() {
    // Proves: `w:tblPrEx` is a `PropsRef` of its own, apart from `w:trPr`.
    let (tree, story) = scan(
        "<w:tbl><w:tr><w:tblPrEx><w:tblW w:w=\"900\"/></w:tblPrEx><w:trPr><w:jc w:val=\"center\"/></w:trPr>\
         <w:tc><w:p/></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
    );
    let rows: Vec<_> = table(&tree, &story).rows().collect();
    let ex = rows[0].table_prop_ex().unwrap();
    assert_eq!(ex.attr("tblW", "w").as_deref(), Some("900"));
    assert!(ex.get("jc").is_none());
    assert_eq!(
        rows[0].props().unwrap().val("jc").as_deref(),
        Some("center")
    );
    assert!(rows[1].table_prop_ex().is_none());
}

#[test]
fn a_cell_lists_its_own_revisions() {
    // Proves: `w:cellIns`, `w:cellDel` and `w:cellMerge` in `w:tcPr` are the cell's revisions, in
    // order, and `w:tcPrChange` is not one of them.
    let (tree, story) = scan(
        "<w:tbl><w:tr>\
         <w:tc><w:tcPr><w:cellIns w:id=\"1\" w:author=\"a\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:cellDel w:id=\"2\" w:author=\"a\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:cellMerge w:id=\"3\" w:author=\"a\" w:vMerge=\"rest\"/></w:tcPr><w:p/></w:tc>\
         <w:tc><w:tcPr><w:tcW w:w=\"1\"/><w:tcPrChange w:id=\"4\" w:author=\"a\"><w:tcPr/></w:tcPrChange></w:tcPr><w:p/></w:tc>\
         <w:tc><w:p/></w:tc>\
         </w:tr></w:tbl>",
    );
    let kinds: Vec<Vec<RevKind>> = table(&tree, &story)
        .rows()
        .flat_map(|r| r.cells())
        .map(|c| c.revisions().map(|r| r.kind).collect())
        .collect();
    assert_eq!(
        kinds,
        [
            vec![RevKind::CellIns],
            vec![RevKind::CellDel],
            vec![RevKind::CellMerge],
            vec![],
            vec![]
        ]
    );
}

#[test]
fn story_block_gives_each_kind_and_none_past_the_end() {
    // Proves: `Story::block` returns a paragraph, a table or a row view by entry kind, the view's
    // element is the entry's node, and an index past the end is `None`.
    let (tree, story) = scan("<w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>");
    let kinds: Vec<_> = (0..story.blocks().len())
        .map(|i| story.block(&tree, i).unwrap().kind())
        .collect();
    assert_eq!(
        kinds,
        [
            BlockKind::Paragraph,
            BlockKind::Table,
            BlockKind::Row,
            BlockKind::Paragraph
        ]
    );
    for (i, e) in story.blocks().iter().enumerate() {
        assert_eq!(story.block(&tree, i).unwrap().id(), e.node);
    }
    assert!(story.block(&tree, 4).is_none());
}

#[test]
fn a_paragraph_gives_its_direct_properties() {
    // Proves: `Paragraph::ppr` is a `PropsRef` over `w:pPr`, `None` when there is none.
    let (tree, story) = scan("<w:p><w:pPr><w:jc w:val=\"right\"/></w:pPr></w:p><w:p/>");
    let first = story.paragraph(&tree, 0).unwrap();
    assert_eq!(first.ppr().unwrap().val("jc").as_deref(), Some("right"));
    assert!(story.paragraph(&tree, 1).unwrap().ppr().is_none());
}

#[test]
fn a_strict_table_reads_like_a_transitional_one() {
    // Proves: names are matched in the Strict `w` namespace as well, for rows, cells, the grid,
    // properties, merges and revisions.
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\"><w:body><w:tbl><w:tblPr><w:tblW w:w=\"10\"/></w:tblPr>\
             <w:tblGrid><w:gridCol w:w=\"5\"/><w:gridCol w:w=\"6\"/></w:tblGrid>\
             <w:tr><w:trPr><w:ins w:id=\"1\" w:author=\"a\"/></w:trPr>\
             <w:tc><w:tcPr><w:gridSpan w:val=\"2\"/><w:vMerge w:val=\"restart\"/><w:cellIns w:id=\"2\" w:author=\"a\"/></w:tcPr>\
             <w:sdt><w:sdtContent><w:p/></w:sdtContent></w:sdt></w:tc></w:tr></w:tbl></w:body></w:document>"
        ),
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let t = table(&tree, &story);
    assert_eq!(t.grid().collect::<Vec<_>>(), [Some(5), Some(6)]);
    assert!(t.props().unwrap().get("tblW").is_some());
    let row = t.rows().next().unwrap();
    assert_eq!(
        row.revisions().map(|r| r.kind).collect::<Vec<_>>(),
        [RevKind::RowIns]
    );
    let cell = row.cells().next().unwrap();
    assert_eq!(cell.grid_span(), 2);
    assert_eq!(cell.v_merge(), VMerge::Restart);
    assert_eq!(cell.revisions().count(), 1);
    assert_eq!(cell.blocks().count(), 1);
}

#[test]
fn deep_wrappers_are_walked_without_recursion() {
    // Proves: a cell with 100 nested content controls, 200 elements deep (the parser's limit is 256), is
    // read with an explicit stack and still finds the paragraph at the bottom.
    let depth = 100;
    let body = format!(
        "<w:tbl><w:tr><w:tc>{}{}{}</w:tc></w:tr></w:tbl>",
        "<w:sdt><w:sdtContent>".repeat(depth),
        para("deep"),
        "</w:sdtContent></w:sdt>".repeat(depth),
    );
    let (tree, story) = scan(&body);
    let cell = table(&tree, &story)
        .rows()
        .next()
        .unwrap()
        .cells()
        .next()
        .unwrap();
    assert_eq!(cell.blocks().count(), 1);
    assert_eq!(story.blocks().len(), 3);
}

#[test]
fn block_within_reads_any_container_of_blocks() {
    // Proves: `Block::within` on an element that is not a cell (here a `w:body`) lists its paragraphs
    // and tables, and a cell that holds no paragraph has no blocks.
    let (tree, story) = scan("<w:p/><w:tbl><w:tr><w:tc/></w:tr></w:tbl><w:p/>");
    let body = tree.root().first_child().unwrap();
    let kinds: Vec<_> = Block::within(&tree, body).map(|b| b.kind()).collect();
    assert_eq!(
        kinds,
        [BlockKind::Paragraph, BlockKind::Table, BlockKind::Paragraph]
    );
    let cell = table(&tree, &story)
        .rows()
        .next()
        .unwrap()
        .cells()
        .next()
        .unwrap();
    assert_eq!(cell.blocks().count(), 0);
}
