//! Recovery in the field machine and in the wrapper rule, from the review of the text model:
//! instruction text in a result, a simple field nested deeper than the limit, and a row or a cell
//! that a tracked change marks as both inserted and deleted.

use super::testing::document;
use super::tests_fields::{begin, end, instr, separate, text};
use super::tests_text::{FINAL, ORIGINAL, REDLINE, para, texts};
use super::*;
use std::sync::Arc;

fn count(m: &TextMap, kind: AnomalyKind) -> usize {
    m.anomalies().iter().filter(|a| a.kind == kind).count()
}

#[test]
fn instruction_text_in_a_result_is_stray_and_does_not_join_the_instruction() {
    let m = para(&format!(
        "{}{}{}{}",
        begin() + &instr(" REF a ") + &separate(),
        text("one"),
        instr(" stray ") + &text("two"),
        end()
    ));
    assert_eq!(m.text(FINAL), "onetwo");
    assert_eq!(m.fields()[0].instr, " REF a ");
    assert_eq!(count(&m, AnomalyKind::StrayFieldChar), 1);
    let codes = m
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::FieldCode)
        .count();
    assert_eq!(codes, 2, "it is still field code: a mark, in no text");
}

#[test]
fn instruction_text_of_a_field_ignored_for_its_depth_is_dropped_with_it() {
    // 33 nested fields: the 33rd is ignored, and its instruction is not the 32nd's
    let mut s = String::new();
    for _ in 0..33 {
        s.push_str(&(begin() + &instr(" A ")));
    }
    s.push_str(&instr(" TOODEEP "));
    for _ in 0..33 {
        s.push_str(&(separate() + &end()));
    }
    let m = para(&s);
    assert_eq!(m.fields().len(), 32);
    assert!(m.fields().iter().all(|f| !f.instr.contains("TOODEEP")));
    assert_eq!(count(&m, AnomalyKind::FieldTooDeep), 1);
}

#[test]
fn a_simple_field_ignored_for_its_depth_does_not_swallow_the_next_field_end() {
    let mut deep = String::from("<w:r><w:t>core</w:t></w:r>");
    for _ in 0..34 {
        deep = format!("<w:fldSimple w:instr=\" X \">{deep}</w:fldSimple>");
    }
    // a complex field after it must still open, hold its result and close
    let m = para(&format!(
        "{deep}{}",
        begin() + &instr(" REF b ") + &separate() + &text("after") + &end() + &text("outside")
    ));
    assert_eq!(m.text(FINAL), "coreafteroutside");
    assert_eq!(count(&m, AnomalyKind::FieldTooDeep), 2);
    assert_eq!(count(&m, AnomalyKind::StrayFieldChar), 0);
    let outside = m
        .items()
        .iter()
        .rev()
        .find(|i| i.kind == ItemKind::Text)
        .unwrap();
    assert_eq!(
        outside.flags & FIELD_RESULT,
        0,
        "the end closed the field, so what follows is not a result"
    );
    let after = m.items().iter().find(|i| m.slice(i) == "after").unwrap();
    assert_eq!(after.flags & FIELD_RESULT, FIELD_RESULT);
}

#[test]
fn a_row_marked_as_inserted_and_deleted_is_in_neither_mode() {
    let tree = document(
        "<w:tbl><w:tr><w:trPr><w:ins w:id=\"1\"/><w:del w:id=\"2\"/></w:trPr>\
         <w:tc><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr>\
         <w:tr><w:trPr><w:del w:id=\"3\"/><w:ins w:id=\"4\"/></w:trPr><w:tc><w:p><w:r><w:t>y</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    for (entry, word) in [(2, "x"), (4, "y")] {
        let m = story.paragraph(&tree, entry).unwrap().map();
        assert_eq!(texts(&m), ["", "", word], "entry {entry}");
        let chain: Vec<WrapperKind> = m.context(m.items()[0].ctx).map(|c| c.kind).collect();
        assert_eq!(chain.len(), 2);
        assert!(chain.contains(&WrapperKind::RowIns) && chain.contains(&WrapperKind::RowDel));
    }
    let facts = story.num_facts(&tree);
    assert!(facts.iter().all(|f| f.presence == Presence::Never));
}

#[test]
fn a_cell_marked_as_inserted_and_deleted_is_in_neither_mode() {
    let ms = super::tests_text::maps(
        "<w:tbl><w:tr><w:tc><w:tcPr><w:cellDel w:id=\"1\"/><w:cellIns w:id=\"2\"/></w:tcPr>\
         <w:p><w:r><w:t>z</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    assert_eq!(texts(&ms[0]), ["", "", "z"]);
    assert_eq!(ms[0].text(REDLINE), "z");
    assert_eq!(ms[0].text(ORIGINAL), "");
}

#[test]
fn a_row_with_a_huge_property_container_and_many_cells_is_read_in_linear_time() {
    // every paragraph of the row used to read the whole of `w:trPr` and, with no `w:trPr`, every cell
    let cells: String = (0..20_000)
        .map(|_| "<w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc>")
        .collect();
    let tree = document(&format!("<w:tbl><w:tr>{cells}</w:tr></w:tbl>"));
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let mut total = 0;
    for i in 0..story.blocks().len() {
        if let Some(p) = story.paragraph(&tree, i) {
            total += p.map().len(FINAL);
        }
    }
    assert_eq!(total, 20_000);
    let marks: String = (0..20_000).map(|_| "<w:cantSplit/>").collect();
    let tree = document(&format!(
        "<w:tbl><w:tr><w:trPr>{marks}<w:del w:id=\"1\"/></w:trPr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"
    ));
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let m = story.paragraph(&tree, 2).unwrap().map();
    assert_eq!(
        m.text(FINAL),
        "c",
        "a revision beyond the first 64 properties of a row is not read"
    );
}
