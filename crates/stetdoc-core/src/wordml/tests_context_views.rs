//! What a caller can read from an item and from a wrapper of a paragraph's map without naming the XML:
//! the id of a marker (`Paragraph::mark_id`), and the revision, content control or hyperlink a
//! wrapper is (`revision_of`, `control_of`, `hyperlink_of`). Each test says what it proves.

use super::names::W_T;
use super::testing::{document, parse};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

fn scan(tree: &SpanTree) -> Story {
    Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap()
}

const REV: &str = "w:author=\"Ann\" w:date=\"2024-02-03T04:05:06Z\"";

/// The item of the first paragraph with the first text `t`, and the wrappers of it, innermost first.
fn wrappers_of_text(tree: &SpanTree, para: usize, t: &str) -> Vec<(WrapperKind, Option<Revision>)> {
    let story = scan(tree);
    let p = story.paragraph(tree, para).unwrap();
    let m = p.map();
    let item = m
        .items()
        .iter()
        .find(|i| i.kind == ItemKind::Text && m.slice(i) == t)
        .unwrap();
    m.context(item.ctx)
        .map(|c| (c.kind, p.revision_of(c)))
        .collect()
}

#[test]
fn the_id_of_a_marker_is_read_from_the_item() {
    // Proves: comment ranges and references, note references, bookmarks and move ranges give their
    // `w:id`; text, tabs and permissions do not; an id that is not a number gives none.
    let tree = document(
        "<w:p><w:commentRangeStart w:id=\"7\"/><w:bookmarkStart w:id=\"9\" w:name=\"b\"/><w:r><w:t>a</w:t></w:r>\
         <w:bookmarkEnd w:id=\"9\"/><w:commentRangeEnd w:id=\"7\"/>\
         <w:r><w:commentReference w:id=\"7\"/></w:r><w:r><w:footnoteReference w:id=\"3\"/></w:r>\
         <w:r><w:endnoteReference w:id=\"4\"/></w:r><w:moveFromRangeStart w:id=\"5\" w:name=\"m\"/>\
         <w:moveFromRangeEnd w:id=\"5\"/><w:permStart w:id=\"x\" w:edGrp=\"everyone\"/>\
         <w:bookmarkStart w:id=\"seven\" w:name=\"c\"/><w:r><w:tab/></w:r></w:p>",
    );
    let story = scan(&tree);
    let p = story.paragraph(&tree, 0).unwrap();
    let m = p.map();
    let ids: Vec<(ItemKind, Option<i64>)> =
        m.items().iter().map(|i| (i.kind, p.mark_id(i))).collect();
    assert_eq!(
        ids,
        [
            (ItemKind::CommentStart, Some(7)),
            (ItemKind::BookmarkStart, Some(9)),
            (ItemKind::Text, None),
            (ItemKind::BookmarkEnd, Some(9)),
            (ItemKind::CommentEnd, Some(7)),
            (ItemKind::CommentRef, Some(7)),
            (ItemKind::NoteRef, Some(3)),
            (ItemKind::NoteRef, Some(4)),
            (ItemKind::MoveRangeStart, Some(5)),
            (ItemKind::MoveRangeEnd, Some(5)),
            (ItemKind::PermStart, None),
            (ItemKind::BookmarkStart, None),
            (ItemKind::Tab, None),
        ]
    );
}

#[test]
fn the_automatic_number_in_a_note_has_the_id_of_the_note() {
    let tree = parse(
        "word/footnotes.xml",
        &format!(
            "<w:footnotes xmlns:w=\"{W_T}\"><w:footnote w:id=\"-1\" w:type=\"separator\"><w:p/></w:footnote>\
             <w:footnote w:id=\"2\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t> text</w:t></w:r></w:p></w:footnote>\
             <w:footnote w:id=\"5\"><w:p><w:r><w:footnoteRef/></w:r></w:p></w:footnote></w:footnotes>"
        ),
    );
    let story = Story::scan(
        &Arc::from("word/footnotes.xml"),
        StoryKind::Footnotes,
        &tree,
    )
    .unwrap();
    let marks: Vec<Option<i64>> = (0..story.blocks().len())
        .filter_map(|i| story.paragraph(&tree, i))
        .flat_map(|p| {
            let m = p.map();
            m.items()
                .iter()
                .filter(|i| i.kind == ItemKind::NoteMark)
                .map(|i| p.mark_id(i))
                .collect::<Vec<_>>()
        })
        .collect();
    assert_eq!(marks, [Some(2), Some(5)]);
}

#[test]
fn a_revision_wrapper_gives_its_revision_with_author_date_and_branch() {
    // Proves: the contexts of an item, innermost first, give the `w:ins` and the `w:del` it sits in as
    // revisions with their ids and authors, and an untaken branch is marked.
    let tree = document(&format!(
        "<w:p><w:ins w:id=\"1\" {REV}><w:del w:id=\"2\" w:author=\"Bob\"><w:r><w:delText>x</w:delText></w:r></w:del></w:ins>\
         <mc:AlternateContent><mc:Choice Requires=\"wps\"><w:ins w:id=\"4\" w:author=\"Di\"><w:r><w:t>t</w:t></w:r></w:ins></mc:Choice>\
         <mc:Fallback><w:ins w:id=\"3\" w:author=\"Cy\"><w:r><w:t>f</w:t></w:r></w:ins></mc:Fallback>\
         </mc:AlternateContent></w:p>"
    ));
    let chain = wrappers_of_text(&tree, 0, "x");
    let kinds: Vec<WrapperKind> = chain.iter().map(|c| c.0).collect();
    assert_eq!(kinds, [WrapperKind::Del, WrapperKind::Ins]);
    let del = chain[0].1.clone().unwrap();
    assert_eq!(
        (del.kind, del.id, del.author.as_deref()),
        (RevKind::Del, Some(2), Some("Bob"))
    );
    assert_eq!(del.date, None);
    let ins = chain[1].1.clone().unwrap();
    assert_eq!(
        (ins.kind, ins.id, ins.author.as_deref(), ins.date.as_deref()),
        (
            RevKind::Ins,
            Some(1),
            Some("Ann"),
            Some("2024-02-03T04:05:06Z")
        )
    );
    assert_eq!(del.branch, Branch::Outside);
    // in the taken branch the revision says so, and the wrapper of the branch is not a revision
    let branch = wrappers_of_text(&tree, 0, "t");
    assert_eq!(
        branch.iter().map(|c| c.0).collect::<Vec<_>>(),
        [WrapperKind::Ins, WrapperKind::Alternate]
    );
    assert_eq!(branch[0].1.as_ref().unwrap().branch, Branch::Taken);
    assert_eq!(branch[0].1.as_ref().unwrap().author.as_deref(), Some("Di"));
    assert_eq!(branch[1].1, None);
}

#[test]
fn a_row_or_a_cell_marked_by_a_revision_gives_the_revision_in_its_properties() {
    // Proves: the wrapper of a row or a cell is the row or the cell, and its revision is the element
    // in the properties; a row marked both ways has two wrappers, each with its own revision.
    let tree = document(&format!(
        "<w:tbl><w:tr><w:trPr><w:ins w:id=\"1\" {REV}/><w:del w:id=\"2\" w:author=\"Bob\"/></w:trPr>\
         <w:tc><w:tcPr><w:cellIns w:id=\"3\" w:author=\"Cy\"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>\
         <w:tc><w:tcPr><w:cellDel w:id=\"4\" w:author=\"Di\"/></w:tcPr><w:p><w:r><w:delText>b</w:delText></w:r></w:p></w:tc>\
         </w:tr></w:tbl>"
    ));
    let story = scan(&tree);
    let first = (0..story.blocks().len())
        .find(|&i| story.paragraph(&tree, i).is_some())
        .unwrap();
    let a = wrappers_of_text(&tree, first, "a");
    let kinds: Vec<WrapperKind> = a.iter().map(|c| c.0).collect();
    assert_eq!(
        kinds,
        [
            WrapperKind::CellIns,
            WrapperKind::RowIns,
            WrapperKind::RowDel
        ]
    );
    let revs: Vec<(RevKind, Option<i64>, Option<String>)> = a
        .into_iter()
        .map(|c| c.1.unwrap())
        .map(|r| (r.kind, r.id, r.author))
        .collect();
    assert_eq!(
        revs,
        [
            (RevKind::CellIns, Some(3), Some("Cy".into())),
            (RevKind::RowIns, Some(1), Some("Ann".into())),
            (RevKind::RowDel, Some(2), Some("Bob".into())),
        ]
    );
}

#[test]
fn a_content_control_and_a_hyperlink_give_their_properties_and_other_wrappers_give_none() {
    let tree = document(
        "<w:p><w:sdt><w:sdtPr><w:alias w:val=\"Client\"/><w:tag w:val=\"c1\"/><w:id w:val=\"42\"/><w:text/></w:sdtPr>\
         <w:sdtContent><w:hyperlink w:anchor=\"top\" w:tooltip=\"Up\"><w:r><w:t>link</w:t></w:r></w:hyperlink>\
         </w:sdtContent></w:sdt></w:p>",
    );
    let story = scan(&tree);
    let p = story.paragraph(&tree, 0).unwrap();
    let m = p.map();
    let item = m.items().iter().find(|i| i.kind == ItemKind::Text).unwrap();
    let ctxs: Vec<&Context> = m.context(item.ctx).collect();
    assert_eq!(
        ctxs.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [WrapperKind::Hyperlink, WrapperKind::Control]
    );
    let link = p.hyperlink_of(ctxs[0]).unwrap();
    assert_eq!(
        (link.anchor.as_deref(), link.tooltip.as_deref(), link.r_id),
        (Some("top"), Some("Up"), None)
    );
    assert!(p.control_of(ctxs[0]).is_none());
    assert!(p.revision_of(ctxs[0]).is_none());
    let sdt = p.control_of(ctxs[1]).unwrap();
    assert_eq!(
        (sdt.alias.as_deref(), sdt.tag.as_deref(), sdt.id, sdt.kind),
        (Some("Client"), Some("c1"), Some(42), ControlKind::PlainText)
    );
    assert!(p.hyperlink_of(ctxs[1]).is_none());
    assert!(p.revision_of(ctxs[1]).is_none());
}
