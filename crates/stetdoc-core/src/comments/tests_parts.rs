//! The comment parts: creating them, in either flavour, and the rows, styles and paragraphs that go in them.

use super::body::{self, NewComment, ParaSpec, Styles};
use super::*;
use crate::revisions::handle::PartWriter;
use crate::threads::Threads;
use crate::threads::tests_support::*;

/// The staged text of a part.
fn text(tx: &crate::state::Tx<'_>, part: &str) -> String {
    let view = tx.parts();
    let id = view.id(part).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

fn plain() -> Pkg {
    Pkg::new("<w:p><w:r><w:t>text</w:t></w:r></w:p>")
}

#[test]
fn a_missing_comments_part_is_made_and_related_and_read_back() {
    let state = plain().open();
    let mut tx = state.begin();
    let name = parts::ensure_comments(&mut PartWriter::over(&mut tx)).unwrap();
    assert_eq!(&*name, "word/comments.xml");
    // The part is a story now, and the second call finds it.
    let view = tx.parts();
    assert!(Threads::of(&view).unwrap().comments().is_empty());
    assert_eq!(
        parts::names(&view).unwrap().comments.as_deref(),
        Some("word/comments.xml")
    );
    assert_eq!(
        &*parts::ensure_comments(&mut PartWriter::over(&mut tx)).unwrap(),
        "word/comments.xml"
    );
    assert!(text(&tx, "word/comments.xml").contains("mc:Ignorable=\"w14\""));
    let (next, _) = tx.commit().unwrap();
    let again = Threads::of(&next.parts()).unwrap();
    assert!(again.parts().comments.is_some());
}

#[test]
fn a_strict_document_gets_a_strict_part_and_the_strict_relationship_type() {
    let state = plain().strict().open();
    let mut tx = state.begin();
    parts::ensure_comments(&mut PartWriter::over(&mut tx)).unwrap();
    parts::ensure_extended(&mut PartWriter::over(&mut tx)).unwrap();
    let rels = text(&tx, "word/_rels/document.xml.rels");
    assert!(
        rels.contains("http://purl.oclc.org/ooxml/officeDocument/relationships/comments\""),
        "{rels}"
    );
    // The Microsoft extension type has one spelling.
    assert!(
        rels.contains("http://schemas.microsoft.com/office/2011/relationships/commentsExtended"),
        "{rels}"
    );
    assert!(
        text(&tx, "word/comments.xml").contains("http://purl.oclc.org/ooxml/wordprocessingml/main")
    );
    assert!(Threads::of(&tx.parts()).unwrap().parts().comments.is_some());
}

#[test]
fn an_existing_unrelated_part_keeps_its_name_and_the_new_one_is_numbered() {
    let state = plain()
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")])
        .open_related(|k| k != crate::schema::PartKind::Comments);
    let mut tx = state.begin();
    let name = parts::ensure_comments(&mut PartWriter::over(&mut tx)).unwrap();
    assert_eq!(&*name, "word/comments2.xml");
}

#[test]
fn a_new_extended_part_has_an_open_row_for_each_comment_that_has_a_paragraph_id() {
    let pkg = plain().comments(&[
        comment(0, "A", &[Some("0000000A")], "x"),
        comment(1, "B", &[None], "y"),
        comment(2, "C", &[Some("0000000C")], "z"),
    ]);
    let state = pkg.open();
    let mut tx = state.begin();
    let name = parts::ensure_extended(&mut PartWriter::over(&mut tx)).unwrap();
    let xml = text(&tx, &name);
    assert_eq!(xml.matches("<w15:commentEx ").count(), 2, "{xml}");
    assert!(xml.contains("w15:paraId=\"0000000A\" w15:done=\"0\""));
    assert_eq!(
        &*parts::ensure_extended(&mut PartWriter::over(&mut tx)).unwrap(),
        &*name
    );
}

#[test]
fn rows_are_added_found_changed_and_removed() {
    let pkg = plain()
        .comments(&[comment(0, "A", &[Some("0000000A")], "x")])
        .extended(&[ex("0000000A", None, false)])
        .ids(&[id_row("0000000A", "00000001")])
        .extensible(&[ext_row("00000001")])
        .people(&["A"]);
    let state = pkg.open();
    let mut tx = state.begin();
    let ex_part = "word/commentsExtended.xml";
    assert!(rows::set_done(&mut PartWriter::over(&mut tx), ex_part, 0xA, true).unwrap());
    assert!(
        !rows::set_done(&mut PartWriter::over(&mut tx), ex_part, 0xA, true).unwrap(),
        "no change the second time"
    );
    assert!(
        rows::set_done(&mut PartWriter::over(&mut tx), ex_part, 0xB, true).unwrap(),
        "a missing row is added"
    );
    rows::add_extended(
        &mut PartWriter::over(&mut tx),
        ex_part,
        0xC,
        Some(0xA),
        false,
    )
    .unwrap();
    rows::set_parents(
        &mut PartWriter::over(&mut tx),
        ex_part,
        &[(0xC, None), (0xB, Some(0xA))],
    )
    .unwrap();
    let xml = text(&tx, ex_part);
    assert!(
        xml.contains("w15:paraId=\"0000000A\" w15:done=\"1\""),
        "{xml}"
    );
    assert!(
        xml.contains("w15:paraId=\"0000000B\" w15:done=\"1\" w15:paraIdParent=\"0000000A\""),
        "{xml}"
    );
    assert!(
        xml.contains("w15:paraId=\"0000000C\" w15:done=\"0\""),
        "{xml}"
    );
    rows::add_id(
        &mut PartWriter::over(&mut tx),
        "word/commentsIds.xml",
        0xB,
        0x77,
    )
    .unwrap();
    rows::add_extensible(
        &mut PartWriter::over(&mut tx),
        "word/commentsExtensible.xml",
        0x77,
    )
    .unwrap();
    rows::add_person(&mut PartWriter::over(&mut tx), "word/people.xml", "A").unwrap();
    rows::add_person(&mut PartWriter::over(&mut tx), "word/people.xml", "B").unwrap();
    assert_eq!(
        text(&tx, "word/people.xml").matches("<w15:person ").count(),
        2
    );
    let paras = std::collections::HashSet::from([0xA_u32]);
    let durables = std::collections::HashSet::from([1_u32]);
    rows::remove(
        &mut PartWriter::over(&mut tx),
        ex_part,
        "commentEx",
        &paras,
        &durables,
    )
    .unwrap();
    rows::remove(
        &mut PartWriter::over(&mut tx),
        "word/commentsIds.xml",
        "commentId",
        &paras,
        &durables,
    )
    .unwrap();
    rows::remove(
        &mut PartWriter::over(&mut tx),
        "word/commentsExtensible.xml",
        "commentExtensible",
        &paras,
        &durables,
    )
    .unwrap();
    assert!(!text(&tx, ex_part).contains("0000000A\" w15:done"));
    assert!(!text(&tx, "word/commentsIds.xml").contains("00000001"));
    assert!(!text(&tx, "word/commentsExtensible.xml").contains("00000001"));
    assert!(text(&tx, "word/commentsIds.xml").contains("0000000B"));
}

#[test]
fn a_comment_is_built_the_way_word_writes_it() {
    let state = plain().comments(&[]).open();
    let mut tx = state.begin();
    let ids = body::mint(&mut PartWriter::over(&mut tx), "word/comments.xml", 2, None).unwrap();
    assert_eq!(ids.len(), 2);
    let styles = Styles {
        reference: Some("CommentReference".into()),
        text: Some("CommentText".into()),
    };
    let specs = [
        ParaSpec {
            text: "first\tsecond".into(),
            para_id: Some(ids[0]),
        },
        ParaSpec {
            text: " padded ".into(),
            para_id: Some(ids[1]),
        },
    ];
    let mut writer = PartWriter::over(&mut tx);
    let tree = writer.part_mut("word/comments.xml").unwrap();
    let w = tree.root().namespace().to_owned();
    let root = tree.root().id();
    let c = body::comment(
        tree,
        &w,
        &NewComment {
            id: 4,
            author: "Ann",
            date: "2026-01-02T03:04:05Z",
            initials: "A",
        },
        &specs,
        &styles,
    )
    .unwrap();
    super::xmlw::append(tree, root, c).unwrap();
    let xml = text(&tx, "word/comments.xml");
    assert!(xml.contains("<w:comment w:id=\"4\" w:author=\"Ann\" w:date=\"2026-01-02T03:04:05Z\" w:initials=\"A\">"), "{xml}");
    assert!(xml.contains("<w:pPr><w:pStyle w:val=\"CommentText\"/></w:pPr><w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:annotationRef/></w:r><w:r><w:t>first</w:t><w:tab/><w:t>second</w:t></w:r>"), "{xml}");
    assert!(
        xml.contains("<w:t xml:space=\"preserve\"> padded </w:t>"),
        "{xml}"
    );
    assert_eq!(xml.matches("w14:textId=\"77777777\"").count(), 2);
    assert_eq!(
        xml.matches("annotationRef").count(),
        1,
        "only the first paragraph"
    );
    // It reads back as a comment, keyed by its last paragraph.
    let t = Threads::of(&tx.parts()).unwrap();
    assert_eq!(t.comment(4).unwrap().para_id, Some(ids[1]));
    assert_eq!(body::initials("Clauseo Counsel"), "CC");
    assert_eq!(body::initials("clauseo"), "C");
}

#[test]
fn the_two_styles_are_found_by_name_or_added_the_way_word_adds_them() {
    // A document with Italian ids has them.
    let italian = "<w:style w:type=\"character\" w:styleId=\"Rimandocommento\"><w:name w:val=\"annotation reference\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Testocommento\"><w:name w:val=\"annotation text\"/></w:style>";
    let state = plain().styles(italian).open();
    let plan = styles::plan(&state.parts(), "word/document.xml").unwrap();
    assert!(!plan.adds());
    assert_eq!(plan.styles.reference.as_deref(), Some("Rimandocommento"));
    assert_eq!(plan.styles.text.as_deref(), Some("Testocommento"));
    // A document with neither gets both, with fresh ids when the usual ones are taken by other styles.
    let bare = "<w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style><w:style w:type=\"character\" w:default=\"1\" w:styleId=\"DefaultParagraphFont\"><w:name w:val=\"Default Paragraph Font\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"CommentText\"><w:name w:val=\"Mine\"/></w:style>";
    let state = plain().styles(bare).open();
    let plan = styles::plan(&state.parts(), "word/document.xml").unwrap();
    assert!(plan.adds());
    assert_eq!(plan.styles.reference.as_deref(), Some("CommentReference"));
    assert_eq!(plan.styles.text.as_deref(), Some("CommentText2"));
    let mut tx = state.begin();
    assert!(styles::apply(&mut PartWriter::over(&mut tx), &plan).unwrap());
    let xml = text(&tx, "word/styles.xml");
    assert!(xml.contains("<w:style w:type=\"character\" w:styleId=\"CommentReference\"><w:name w:val=\"annotation reference\"/><w:basedOn w:val=\"DefaultParagraphFont\"/><w:uiPriority w:val=\"99\"/><w:semiHidden/><w:unhideWhenUsed/><w:rPr><w:sz w:val=\"16\"/><w:szCs w:val=\"16\"/></w:rPr></w:style>"), "{xml}");
    assert!(xml.contains("w:styleId=\"CommentText2\"><w:name w:val=\"annotation text\"/><w:basedOn w:val=\"Normal\"/>"), "{xml}");
    assert!(!xml.contains("w:link"), "no linked character style");
    // Applying again changes nothing: the plan now finds both.
    let again = styles::plan(&tx.parts(), "word/document.xml").unwrap();
    assert!(!again.adds());
    // A document with no styles part has no style to name and nothing to add.
    let state = plain().open();
    let plan = styles::plan(&state.parts(), "word/document.xml").unwrap();
    assert!(plan.part.is_none() && plan.styles.reference.is_none() && !plan.adds());
}
