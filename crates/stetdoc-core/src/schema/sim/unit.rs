//! The edit simulation on small documents.

use super::super::tests::{W, doc_xml, violations};
use super::*;

const CLEAN: &str = r#"<w:p w14:paraId="1A2B3C4D"><w:pPr><w:pStyle w:val="Heading1"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:spacing w:before="0"/><w:jc w:val="left"/><w:rPr><w:b/></w:rPr></w:pPr>
           <w:r><w:rPr><w:rFonts w:ascii="A"/><w:b/><w:color w:val="FF0000"/><w:sz w:val="24"/></w:rPr><w:t>hi</w:t></w:r></w:p>
           <w:tbl><w:tblPr><w:tblStyle w:val="T"/><w:tblW w:w="0" w:type="auto"/><w:jc w:val="left"/><w:tblBorders><w:top w:val="single"/></w:tblBorders><w:tblLook w:val="04A0"/></w:tblPr>
             <w:tblGrid><w:gridCol w:w="100"/></w:tblGrid>
             <w:tr><w:trPr><w:cantSplit/></w:trPr><w:tc><w:tcPr><w:tcW w:w="100" w:type="dxa"/><w:shd w:val="clear"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>
           <w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1" w:right="1" w:bottom="1" w:left="1" w:header="1" w:footer="1" w:gutter="0"/></w:sectPr>"#;

#[test]
fn a_clean_document_stays_clean_after_every_insert_and_removal() {
    let xml = doc_xml(CLEAN);
    assert!(violations(PartKind::Document, &xml).is_empty());
    let mut tally = Tally::default();
    simulate(xml.as_bytes(), PartKind::Document, 1, 1000, &mut tally);
    assert!(tally.inserts > 300, "only {} inserts ran", tally.inserts);
    assert!(tally.removals > 30, "only {} removals ran", tally.removals);
    assert!(
        tally.rule_b_failures.is_empty(),
        "{:#?}",
        tally.rule_b_failures
    );
    assert!(
        tally.rule_a_failures.is_empty(),
        "{:#?}",
        tally.rule_a_failures
    );
    assert_eq!(tally.parent_clean, tally.inserts + tally.removals);
    assert_eq!(tally.editor_refusals, 0);
    assert_eq!(tally.reordered, 0, "a valid parent is never rewritten");
}

#[test]
fn a_table_whose_properties_are_out_of_order_is_edited_without_a_refusal() {
    // the python-docx shape: tblBorders after tblLook, in a document with Word's own order
    // problems elsewhere
    let xml = doc_xml(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:jc w:val="left"/><w:tblLook w:val="04A0"/><w:tblBorders><w:top w:val="single"/></w:tblBorders></w:tblPr>
           <w:tblGrid><w:gridCol w:w="100"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>
           <w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr></w:p>"#,
    );
    let before = violations(PartKind::Document, &xml);
    assert!(!before.is_empty());
    let mut tally = Tally::default();
    simulate(xml.as_bytes(), PartKind::Document, 1, 1000, &mut tally);
    assert!(tally.reordered > 10, "{tally:?}");
    assert!(tally.rewritten_removals >= 2, "{tally:?}");
    assert!(
        tally.rule_a_failures.is_empty(),
        "{:#?}",
        tally.rule_a_failures
    );
}

#[test]
fn rewriting_removes_the_old_violation() {
    let xml = doc_xml(
        r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/><w:tblLook w:val="04A0"/><w:tblBorders><w:top w:val="single"/></w:tblBorders></w:tblPr>
           <w:tblGrid><w:gridCol w:w="100"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#,
    );
    let mut tree = SimTree::parse(xml.as_bytes()).expect("parses");
    let tbl_pr = tree.find("tblPr").expect("tblPr");
    let ctx = tree.ctx_of(PartKind::Document, tbl_pr).expect("ctx");
    let before = keys_of(&tree, ctx, tbl_pr, None);
    assert_eq!(before.total(), 1);
    let (fit, new) = tree
        .insert(Some(ctx), tbl_pr, (W, "jc"), Prefer::Last)
        .expect("inserts");
    assert!(matches!(fit, Fit::Reorder(_)), "{fit:?}");
    // the new w:jc has no w:val yet; that is its own problem, not the container's
    let after = keys_of(&tree, ctx, tbl_pr, Some(new));
    assert!(
        after.is_empty(),
        "the rewrite fixes the container: {after:?}"
    );
    let names: Vec<String> = tree
        .children(tbl_pr)
        .iter()
        .map(|c| tree.qname(*c).1)
        .collect();
    assert_eq!(names, ["tblW", "jc", "tblBorders", "tblLook"]);
}

#[test]
fn taking_a_child_out_of_an_out_of_order_container_adds_no_key() {
    // jc, ind, pStyle: ind and pStyle are out of order. Whatever is taken out, nothing new is
    // wrong, and when the container is rewritten, as the contract asks, it is in order after.
    let xml = doc_xml(
        r#"<w:p><w:pPr><w:jc w:val="left"/><w:ind w:left="0"/><w:pStyle w:val="A"/></w:pPr></w:p>"#,
    );
    let mut tree = SimTree::parse(xml.as_bytes()).expect("parses");
    let ppr = tree.find("pPr").expect("pPr");
    let ctx = tree.ctx_of(PartKind::Document, ppr).expect("ctx");
    let before = keys_of(&tree, ctx, ppr, None);
    assert_eq!(before.total(), 2, "{before:?}");
    for index in 0..3 {
        // without the rewrite the old problems stay, and no new one appears
        let saved = tree.save(ppr);
        assert!(!tree.remove(None, ppr, index));
        let bare = keys_of(&tree, ctx, ppr, None);
        tree.restore(saved);
        assert!(bare.added_since(&before).is_empty(), "{index}: {bare:?}");
        // with it the container is in order
        let saved = tree.save(ppr);
        assert!(tree.remove(Some(ctx), ppr, index));
        let rewritten = keys_of(&tree, ctx, ppr, None);
        let names: Vec<String> = tree
            .children(ppr)
            .iter()
            .map(|c| tree.qname(*c).1)
            .collect();
        tree.restore(saved);
        assert!(rewritten.is_empty(), "{index}: {rewritten:?} {names:?}");
    }
}

#[test]
fn the_rules_hold_with_random_wrappers_and_markup_compatibility() {
    // Alternate contents nested in each other, ignorable wrappers that are processed or skipped,
    // and the attributes that decide it on the root, on the properties and on the wrappers: what
    // insertion reads and what the validator reads must stay the same through every edit.
    let mut rng = super::super::tests::Gen(0x9E37_79B9_7F4A_7C15);
    let mut tally = Tally::default();
    for _ in 0..150 {
        let xml = rng.part();
        simulate(xml.as_bytes(), PartKind::Document, 1, 50, &mut tally);
    }
    assert!(tally.inserts > 1500, "only {} inserts ran", tally.inserts);
    assert!(tally.removals > 200, "only {} removals ran", tally.removals);
    assert_eq!(tally.editor_refusals, 0);
    assert!(
        tally.rule_a_failures.is_empty(),
        "{:#?}",
        &tally.rule_a_failures[..tally.rule_a_failures.len().min(8)]
    );
    assert!(
        tally.rule_b_failures.is_empty(),
        "{:#?}",
        &tally.rule_b_failures[..tally.rule_b_failures.len().min(8)]
    );
}
