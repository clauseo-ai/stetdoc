//! Tests of `required.rs` through `delete_block`: which paragraphs a container needs, and the reason a kept one gives.

use super::required::KeptReason;
use super::stamp::Track;
use super::tests_support::*;

const TABLE: &str = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>";

fn delete(body: &str, para: usize) -> Snap {
    with_compiler(body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.delete_block(&ks[para])]
    })
}

#[test]
fn the_only_paragraph_of_a_container_keeps_its_mark_and_says_why() {
    let s = delete(&format!("<w:p>{}</w:p>", r("Only")), 0);
    s.assert_clean();
    assert!(!s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
    let kept = &s.effect(0).kept;
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].reason, KeptReason::Only("the body"));
}

#[test]
fn the_last_paragraph_keeps_its_mark_when_the_paragraphs_before_it_are_deleted_too() {
    // The paragraph before it has a deleted mark, so it joins this one, and the body would be left with no paragraph.
    let s = with_compiler(
        &format!("<w:p>{}</w:p><w:p>{}</w:p>", r("A"), r("B")),
        Track::Tracked(stamp()),
        |c, _, ks| vec![c.delete_block(&ks[0]), c.delete_block(&ks[1])],
    );
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPr><w:del").count(), 1, "{}", s.xml);
    assert_eq!(s.effect(1).kept[0].reason, KeptReason::Only("the body"));
}

#[test]
fn the_first_paragraph_before_a_table_loses_its_mark() {
    let body = format!("<w:p>{}</w:p>{TABLE}<w:p>{}</w:p>", r("before"), r("tail"));
    let s = delete(&body, 0);
    s.assert_clean();
    assert!(s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
    assert!(s.effect(0).kept.is_empty());
}

#[test]
fn a_paragraph_between_two_tables_or_after_the_last_keeps_its_mark() {
    let between = format!(
        "{TABLE}<w:p>{}</w:p>{TABLE}<w:p>{}</w:p>",
        r("between"),
        r("tail")
    );
    let after = format!("<w:p>{}</w:p>{TABLE}<w:p>{}</w:p>", r("lead"), r("after"));
    for (body, at, reason) in [
        (between, 1, KeptReason::BetweenTables),
        (after, 2, KeptReason::AfterBlock("the body", "a table")),
    ] {
        let s = delete(&body, at);
        s.assert_clean();
        assert!(!s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
        assert_eq!(s.effect(0).kept[0].reason, reason);
    }
}

#[test]
fn untracked_the_only_paragraph_is_emptied_and_kept() {
    let only = with_compiler(
        &format!("<w:p>{}</w:p>", r("Only")),
        Track::Untracked,
        |c, _, ks| vec![c.delete_block(&ks[0])],
    );
    assert_eq!(only.fin, [""]);
    assert_eq!(only.effect(0).kept[0].reason, KeptReason::Only("the body"));
}
