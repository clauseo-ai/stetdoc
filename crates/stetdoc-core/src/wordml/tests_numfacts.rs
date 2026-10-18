//! Tests of `Story::num_facts` and the property accessors of `Paragraph`.

use super::testing::document;
use super::tests_text::FINAL;
use super::*;
use std::sync::Arc;

fn facts(body: &str) -> Vec<NumFact> {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    story.num_facts(&tree)
}

#[test]
fn the_direct_style_and_numbering_are_read_and_a_paragraph_without_them_has_none() {
    let f = facts(
        "<w:p><w:pPr><w:pStyle w:val=\"ListParagraph\"/><w:numPr><w:ilvl w:val=\"1\"/><w:numId w:val=\"3\"/></w:numPr></w:pPr></w:p>\
         <w:p><w:pPr><w:numPr><w:numId w:val=\"0\"/></w:numPr></w:pPr></w:p><w:p/>",
    );
    assert_eq!(f.len(), 3);
    assert_eq!(f[0].style_id.as_deref(), Some("ListParagraph"));
    assert_eq!(
        f[0].num_pr,
        Some(NumPr {
            num_id: Some(3),
            ilvl: Some(1)
        })
    );
    assert_eq!(
        f[1].num_pr,
        Some(NumPr {
            num_id: Some(0),
            ilvl: None
        })
    );
    assert_eq!(
        (f[2].style_id.as_deref(), f[2].num_pr, f[2].old.is_some()),
        (None, None, false)
    );
    assert!(
        f.iter()
            .all(|x| x.presence == Presence::Always && x.mark_revs.is_none())
    );
    assert_eq!(f.iter().map(|x| x.entry).collect::<Vec<_>>(), [0, 1, 2]);
}

#[test]
fn a_paragraph_property_change_gives_the_old_values_and_tells_absent_from_empty() {
    let f = facts(
        "<w:p><w:pPr><w:pStyle w:val=\"New\"/><w:numPr><w:numId w:val=\"2\"/></w:numPr>\
         <w:pPrChange w:id=\"1\"><w:pPr><w:pStyle w:val=\"Old\"/><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr></w:pPrChange></w:pPr></w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"New\"/><w:pPrChange w:id=\"2\"><w:pPr/></w:pPrChange></w:pPr></w:p>\
         <w:p><w:pPr><w:pStyle w:val=\"Same\"/></w:pPr></w:p>",
    );
    let old = f[0].old.as_ref().unwrap();
    assert_eq!(old.style_id.as_deref(), Some("Old"));
    assert_eq!(
        old.num_pr,
        Some(NumPr {
            num_id: Some(1),
            ilvl: Some(0)
        })
    );
    let none_before = f[1].old.as_ref().unwrap();
    assert_eq!(
        (none_before.style_id.as_deref(), none_before.num_pr),
        (None, None)
    );
    assert!(f[2].old.is_none(), "no change is recorded");
}

#[test]
fn the_mark_and_the_empty_section_break_are_read() {
    let f = facts(
        "<w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr><w:sectPr/></w:pPr></w:p>\
         <w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>text</w:t></w:r></w:p>\
         <w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:tab/></w:r></w:p>\
         <w:p><w:r><w:t></w:t></w:r></w:p>",
    );
    assert_eq!(f[0].mark_revs, MarkRevs::from_flags(MARK_DELETED));
    assert!(f[0].empty_section_break);
    assert!(!f[1].empty_section_break, "it has text");
    assert!(f[2].empty_section_break, "a tab is not a w:t");
    assert!(!f[3].empty_section_break, "no section break");
}

#[test]
fn presence_follows_the_wrappers_above_the_paragraph() {
    let f = facts(
        "<w:p/>\
         <w:tbl><w:tr><w:trPr><w:ins w:id=\"1\"/></w:trPr><w:tc><w:p/>\
           <w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl></w:tc></w:tr>\
         <w:tr><w:trPr><w:del w:id=\"2\"/></w:trPr><w:tc><w:p/></w:tc></w:tr>\
         <w:tr><w:tc><w:tcPr><w:cellDel w:id=\"3\"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:cellIns w:id=\"4\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>\
         <w:ins w:id=\"5\"><w:p/></w:ins><w:del w:id=\"6\"><w:ins w:id=\"7\"><w:p/></w:ins></w:del>",
    );
    let p: Vec<Presence> = f.iter().map(|x| x.presence).collect();
    use Presence::*;
    assert_eq!(
        p,
        [
            Always,
            FinalOnly,
            FinalOnly,
            OriginalOnly,
            OriginalOnly,
            FinalOnly,
            FinalOnly,
            Never
        ],
        "the nested table is in the inserted row too, and an insertion inside a deletion is in neither"
    );
}

#[test]
fn a_story_with_no_paragraph_has_no_facts() {
    assert!(facts("<w:tbl/>").is_empty());
}

#[test]
fn mark_properties_run_properties_and_the_run_style_are_read_from_the_paragraph() {
    let tree = document(
        "<w:p><w:pPr><w:rPr><w:b/><w:sz w:val=\"28\"/></w:rPr></w:pPr>\
         <w:r><w:rPr><w:rStyle w:val=\"Strong\"/><w:i/></w:rPr><w:t>a</w:t></w:r><w:r><w:t>b</w:t></w:r>\
         <w:fldSimple w:instr=\" PAGE \"/></w:p>",
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let p = story.paragraph(&tree, 0).unwrap();
    let mark = p.mark_props().unwrap();
    assert_eq!(
        (mark.on_off("b"), mark.val("sz").as_deref()),
        (Some(true), Some("28"))
    );
    let m = p.map();
    assert_eq!(m.text(FINAL), "ab\u{6}");
    let a = &m.items()[0];
    let b = &m.items()[1];
    assert_eq!(p.run_style(a).as_deref(), Some("Strong"));
    assert_eq!(p.run_props(a).unwrap().on_off("i"), Some(true));
    assert!(p.run_props(b).is_none() && p.run_style(b).is_none());
    let value = m
        .items()
        .iter()
        .find(|i| i.kind == ItemKind::FieldValue)
        .unwrap();
    assert!(
        p.run_props(value).is_none(),
        "an item that no run holds has no run properties"
    );
    assert!(story.paragraph(&tree, 0).unwrap().element().is_element());
}

#[test]
fn a_paragraph_from_a_cell_view_reads_run_properties_too() {
    let tree = document(
        "<w:tbl><w:tr><w:tc><w:p><w:r><w:rPr><w:rStyle w:val=\"Emph\"/></w:rPr><w:t>x</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let Some(Block::Row(row)) = story.block(&tree, 1) else {
        panic!("entry 1 is a row");
    };
    let cell = row.cells().next().unwrap();
    let Some(Block::Paragraph(p)) = cell.blocks().next() else {
        panic!("the cell holds a paragraph");
    };
    let m = p.map();
    assert_eq!(p.run_style(&m.items()[0]).as_deref(), Some("Emph"));
}

#[test]
fn join_applies_is_the_structural_half_of_the_join_rule_and_ignores_the_mode() {
    // Proves: `join_applies` is true only when the next entry is a paragraph in the same container,
    // whatever the mark says: not at the end of a cell, before a table, or at the end of the story.
    let f = facts(
        "<w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr></w:p><w:p/>\
         <w:tbl><w:tr><w:tc><w:p/><w:p/></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl>\
         <w:p/><w:sdt><w:sdtContent><w:p/></w:sdtContent></w:sdt><w:p/>",
    );
    let join: Vec<bool> = f.iter().map(|x| x.join_applies).collect();
    assert_eq!(join, [true, false, true, false, false, true, true, false]);
    assert_eq!(f[0].mark_revs, MarkRevs::from_flags(MARK_DELETED));
}
