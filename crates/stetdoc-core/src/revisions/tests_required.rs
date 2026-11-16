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

#[test]
fn every_container_but_a_wrapper_joins_its_last_paragraph_to_the_one_before() {
    // The rule holds in every container that must end with a paragraph (Word for Mac, issue #25): the last paragraph never
    // gets a deleted mark, and the paragraph before it is the one whose mark goes. A block-level content control's last
    // paragraph is kept instead.
    use crate::xml::SpanTree;
    for (container, joins) in [
        ("body", true),
        ("tc", true),
        ("hdr", true),
        ("ftr", true),
        ("footnote", true),
        ("endnote", true),
        ("comment", true),
        ("txbxContent", true),
        ("sdtContent", false),
    ] {
        let xml = format!(
            r#"<w:root xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:{container}><w:p/><w:p/></w:{container}></w:root>"#
        );
        let tree = SpanTree::parse(
            "word/document.xml",
            crate::xml::Bytes::from(xml.into_bytes()),
            &crate::api::limits::Limits::default(),
        )
        .unwrap();
        let parent = tree.root().child_elements().next().unwrap();
        let paras: Vec<_> = parent.child_elements().map(|p| p.id()).collect();
        let found = super::required::previous_to_join(&tree, paras[1]);
        assert_eq!(found, joins.then_some(paras[0]), "{container}");
    }
}

#[test]
fn accept_all_reads_as_final_after_deleting_a_paragraph_before_another_authors_deleted_last_one() {
    // The soak's case (document 7, sequence 88, R8): an empty last paragraph whose mark another author deleted, a paragraph
    // inserted before the one in front of it, that one deleted, accepts by id in between, and a paragraph inserted after
    // the new one. Accept all must read as Final, with no empty paragraph left at the end.
    use super::compiler::Compiler;
    use super::index::ChangeIndex;
    use super::laws_util::final_runs;
    use super::resolve::resolve;
    use super::types::{Action, NewParagraph, Place, Selector, Step};
    const Z: &str = r#"w:author="Zed" w:date="2026-10-02T10:00:00Z""#;
    const F: &str = r#"w:author="Fay" w:date="2026-10-01T10:00:00Z""#;
    let body = format!(
        r#"<w:p><w:pPr><w:pStyle w:val="List"/><w:rPr><w:ins w:id="4" {Z}/></w:rPr><w:pPrChange w:id="6" {Z}><w:pPr/></w:pPrChange></w:pPr><w:ins w:id="5" {Z}><w:r><w:t>within</w:t></w:r></w:ins></w:p><w:p><w:pPr><w:rPr><w:ins w:id="7" {Z}/></w:rPr></w:pPr><w:r><w:t xml:space="preserve">lead </w:t></w:r><w:sdt><w:sdtPr><w:alias w:val="c"/></w:sdtPr><w:sdtContent><w:r><w:t xml:space="preserve">control tex</w:t></w:r><w:r><w:rPr><w:i/><w:rPrChange w:id="2" {F}><w:rPr/></w:rPrChange></w:rPr><w:t xml:space="preserve">t</w:t></w:r></w:sdtContent></w:sdt><w:r><w:rPr><w:i/><w:rPrChange w:id="3" {F}><w:rPr/></w:rPrChange></w:rPr><w:t xml:space="preserve"> t</w:t></w:r><w:del w:id="0" {F}><w:r><w:delText xml:space="preserve">rai</w:delText></w:r></w:del><w:ins w:id="1" {F}><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">rai</w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t xml:space="preserve">ty </w:t></w:r></w:ins><w:r><w:t xml:space="preserve">l</w:t></w:r></w:p><w:p><w:pPr><w:rPr><w:del w:id="9" {Z}/></w:rPr></w:pPr></w:p>"#
    );
    let s = state(&body);
    let mut tx = s.begin();
    let ks = para_keys(&tx);
    let track = || Track::Tracked(stamp_of("Clauseo"));
    let mut index = ChangeIndex::new();
    let accept = |tx: &mut crate::state::Tx<'_>, index: &mut ChangeIndex, id: i64| {
        let step = Step {
            action: Action::Accept,
            selector: Selector::Ids(vec![id]),
        };
        resolve(tx, index, &[step]).unwrap();
    };
    {
        let mut c = Compiler::new(&mut tx, track()).unwrap();
        c.delete_block(&ks[2]).unwrap();
        c.finish().unwrap();
    }
    accept(&mut tx, &mut index, 0);
    let heading = {
        let mut c = Compiler::new(&mut tx, track()).unwrap();
        let spec = NewParagraph {
            style: Some("Heading1".to_owned()),
            ..NewParagraph::default()
        };
        let fx = c.insert_block(&ks[1], Place::Before, &spec).unwrap();
        c.finish().unwrap();
        fx.new_blocks[0].clone()
    };
    accept(&mut tx, &mut index, 6);
    {
        let mut c = Compiler::new(&mut tx, track()).unwrap();
        c.delete_block(&ks[1]).unwrap();
        c.finish().unwrap();
    }
    accept(&mut tx, &mut index, 2);
    {
        let mut c = Compiler::new(&mut tx, track()).unwrap();
        let spec = NewParagraph {
            text: "Party".to_owned(),
            ..NewParagraph::default()
        };
        c.insert_block(&heading, Place::After, &spec).unwrap();
        c.finish().unwrap();
    }
    let (next, _) = tx.commit().unwrap();
    let mut tx = next.begin();
    let all = Step {
        action: Action::Accept,
        selector: Selector::All,
    };
    resolve(&mut tx, &mut ChangeIndex::new(), &[all]).unwrap();
    assert_eq!(
        final_runs(&tx.parts()),
        final_runs(&next.parts()),
        "{}",
        body_xml(&tx)
    );
}
