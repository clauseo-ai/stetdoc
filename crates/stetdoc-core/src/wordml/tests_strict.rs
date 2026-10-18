//! The text model in the Strict dialect and with both dialects mixed in one part: names are matched
//! in both `w` namespaces always and by URI, so nothing is chosen from the root.

use super::testing::parse;
use super::tests_text::{FINAL, ORIGINAL, REDLINE, check_invariants, kinds};
use super::*;
use std::sync::Arc;

const STRICT: &str = "http://purl.oclc.org/ooxml/wordprocessingml/main";
const TRANSITIONAL: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The body of a document whose `w` namespace is `w_uri`, with the Transitional namespace bound to
/// `t` so a test can mix the two.
fn doc(w_uri: &str, body: &str) -> crate::xml::SpanTree {
    parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{w_uri}\" xmlns:t=\"{TRANSITIONAL}\" xmlns:s=\"{STRICT}\"><w:body>{body}</w:body></w:document>"
        ),
    )
}

fn maps(tree: &crate::xml::SpanTree) -> Vec<TextMap> {
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, tree).unwrap();
    (0..story.blocks().len())
        .filter_map(|i| story.paragraph(tree, i))
        .map(|p| {
            let m = p.map();
            check_invariants(&m);
            m
        })
        .collect()
}

const BODY: &str = "<w:p><w:r><w:t xml:space=\"preserve\">A </w:t></w:r><w:ins w:id=\"1\"><w:r><w:t>B</w:t></w:r></w:ins>\
     <w:del w:id=\"2\"><w:r><w:delText xml:space=\"preserve\"> C</w:delText></w:r></w:del>\
     <w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> REF x </w:instrText></w:r>\
     <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>4</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>\
     <w:fldSimple w:instr=\" PAGE \"/><w:hyperlink w:anchor=\"q\"><w:r><w:t>h</w:t></w:r></w:hyperlink>\
     <w:sdt><w:sdtContent><w:r><w:t>c</w:t></w:r></w:sdtContent></w:sdt><w:r><w:tab/><w:br w:type=\"page\"/></w:r></w:p>";

#[test]
fn a_strict_paragraph_reads_the_same_as_a_transitional_one() {
    let t = maps(&doc(TRANSITIONAL, BODY));
    let s = maps(&doc(STRICT, BODY));
    assert_eq!(t.len(), 1);
    for mode in [FINAL, ORIGINAL, REDLINE] {
        assert_eq!(s[0].text(mode), t[0].text(mode), "{mode:?}");
    }
    assert_eq!(s[0].text(FINAL), "A B4\u{6}hc\t\u{c}");
    assert_eq!(s[0].text(ORIGINAL), "A  C4\u{6}hc\t\u{c}");
    assert_eq!(kinds(&s[0]), kinds(&t[0]));
    assert!(s[0].has_revisions());
    assert_eq!(s[0].fields().len(), 2);
}

#[test]
fn a_part_that_mixes_both_dialects_reads_each_element_by_its_own_uri() {
    // the root is Transitional, and the insertion, the field and the control are Strict
    let body = "<w:p><w:r><w:t>one </w:t></w:r><s:ins s:id=\"1\"><s:r><s:t>two </s:t></s:r></s:ins>\
         <t:del t:id=\"2\"><s:r><s:delText>gone </s:delText></s:r></t:del>\
         <s:sdt><s:sdtContent><t:r><t:t>three</t:t></t:r></s:sdtContent></s:sdt></w:p>";
    let ms = maps(&doc(TRANSITIONAL, body));
    assert_eq!(ms[0].text(FINAL), "one two three");
    assert_eq!(ms[0].text(ORIGINAL), "one gone three");
    assert!(
        ms[0]
            .anomalies()
            .iter()
            .all(|a| a.kind == AnomalyKind::EdgeSpaceWithoutPreserve),
        "{:?}",
        ms[0].anomalies()
    );
}

#[test]
fn a_strict_body_scans_with_tables_and_revision_marks() {
    let tree = doc(
        STRICT,
        "<w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr></w:pPr><w:r><w:t>a</w:t></w:r></w:p>\
         <w:tbl><w:tr><w:trPr><w:ins w:id=\"2\"/></w:trPr><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    assert_eq!(story.blocks().len(), 4);
    assert!(story.blocks()[0].has(MARK_DELETED));
    assert_eq!(
        story.paragraph(&tree, 0).unwrap().mark_revs(),
        MarkRevs::from_flags(MARK_DELETED)
    );
    let cell = story.paragraph(&tree, 3).unwrap().map();
    assert_eq!(cell.text(FINAL), "b");
    assert_eq!(cell.text(ORIGINAL), "", "the row is a tracked insertion");
    assert_eq!(
        cell.context(cell.items()[0].ctx).next().unwrap().kind,
        WrapperKind::RowIns
    );
}
