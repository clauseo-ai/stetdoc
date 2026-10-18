//! Tests of `Story::sections`: one section per `w:sectPr`, the break type, the mode rule, and the
//! headers and footers resolved through `Rels` with inheritance. Each test says what it proves.

use super::names::{R_S, W_S};
use super::sections::PartSlots;
use super::testing::{document, parse, part};
use super::*;
use crate::xml::SpanTree;
use std::sync::Arc;

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/";
const REL_S: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships/";

fn scan(body: &str) -> (SpanTree, Story) {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (tree, story)
}

fn rel(id: &str, kind: &str, target: RelTarget) -> Rel {
    Rel {
        id: id.into(),
        rel_type: format!("{REL}{kind}"),
        target,
    }
}

fn part_rel(id: &str, kind: &str, name: &str) -> Rel {
    rel(id, kind, RelTarget::Part(name.into()))
}

/// Relationships for headers `rId1..rId3` and footers `rId11..rId13` of the usual names.
fn rels() -> Rels {
    Rels::new(vec![
        part_rel("rId1", "header", "word/header1.xml"),
        part_rel("rId2", "header", "word/header2.xml"),
        part_rel("rId3", "header", "word/header3.xml"),
        part_rel("rId11", "footer", "word/footer1.xml"),
        part_rel("rId12", "footer", "word/footer2.xml"),
        part_rel("rId13", "footer", "word/footer3.xml"),
    ])
}

fn name(slot: &Option<Arc<str>>) -> Option<&str> {
    slot.as_deref()
}

fn sect_p(inner: &str) -> String {
    format!("<w:p><w:pPr><w:sectPr>{inner}</w:sectPr></w:pPr></w:p>")
}

#[test]
fn a_section_for_each_sect_pr_with_the_paragraph_that_ends_it() {
    // Proves: one `Section` per `w:sectPr`, in document order, the ones in a paragraph with
    // `ends_at` the entry index of that paragraph and the final one in `w:body` with `None`, and
    // `props` reads the `w:sectPr` itself.
    let (tree, story) = scan(&format!(
        "<w:p/>{}<w:p/><w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl><w:p/><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>",
        sect_p("<w:pgSz w:w=\"100\"/>"),
        sect_p("<w:pgSz w:w=\"200\"/>"),
    ));
    let sections = story.sections(&tree, &Rels::default());
    assert_eq!(sections.len(), 3);
    assert_eq!(
        sections.iter().map(|s| s.ends_at).collect::<Vec<_>>(),
        [Some(1), Some(5), None]
    );
    for s in &sections[..2] {
        let e = &story.blocks()[s.ends_at.unwrap()];
        assert!(e.has(SECTION_BREAK));
        assert_eq!(
            story
                .paragraph(&tree, s.ends_at.unwrap())
                .unwrap()
                .section_break()
                .unwrap()
                .id(),
            s.node
        );
    }
    let widths: Vec<_> = sections
        .iter()
        .map(|s| s.props(&tree).attr("pgSz", "w").unwrap().into_owned())
        .collect();
    assert_eq!(widths, ["100", "200", "12240"]);
}

#[test]
fn a_body_with_no_final_sect_pr_has_only_the_paragraph_sections() {
    // Proves: a missing final `w:sectPr` is not an error: the list holds the paragraph ones only.
    let (tree, story) = scan(&format!("{}<w:p/>", sect_p("")));
    let sections = story.sections(&tree, &Rels::default());
    assert_eq!(sections.len(), 1);
    assert_eq!(sections[0].ends_at, Some(0));
    let (tree, story) = scan("<w:p/>");
    assert!(story.sections(&tree, &Rels::default()).is_empty());
}

#[test]
fn the_break_type_and_title_page_are_read() {
    // Proves: `w:type` gives each break type, no `w:type` and an unknown one are the next page, and
    // `w:titlePg` is an on/off value that defaults to false.
    let (tree, story) = scan(&format!(
        "{}{}{}{}{}{}<w:sectPr/>",
        sect_p("<w:type w:val=\"continuous\"/>"),
        sect_p("<w:type w:val=\"evenPage\"/><w:titlePg/>"),
        sect_p("<w:type w:val=\"oddPage\"/><w:titlePg w:val=\"false\"/>"),
        sect_p("<w:type w:val=\"nextColumn\"/><w:titlePg w:val=\"1\"/>"),
        sect_p("<w:type w:val=\"nextPage\"/>"),
        sect_p("<w:type w:val=\"sideways\"/>"),
    ));
    let sections = story.sections(&tree, &Rels::default());
    let types: Vec<_> = sections.iter().map(|s| s.break_type).collect();
    assert_eq!(
        types,
        [
            SectionBreak::Continuous,
            SectionBreak::EvenPage,
            SectionBreak::OddPage,
            SectionBreak::NextColumn,
            SectionBreak::NextPage,
            SectionBreak::NextPage,
            SectionBreak::NextPage,
        ]
    );
    let title: Vec<_> = sections.iter().map(|s| s.title_pg).collect();
    assert_eq!(title, [false, true, false, true, false, false, false]);
}

#[test]
fn headers_and_footers_resolve_through_the_relationships() {
    // Proves: each reference of a `w:type` is resolved by its `r:id` to a part name, headers and
    // footers separately, and the first reference of a type wins when one is repeated.
    let (tree, story) = scan(
        "<w:sectPr>\
         <w:headerReference w:type=\"default\" r:id=\"rId1\"/>\
         <w:headerReference w:type=\"first\" r:id=\"rId2\"/>\
         <w:headerReference w:type=\"even\" r:id=\"rId3\"/>\
         <w:headerReference w:type=\"default\" r:id=\"rId3\"/>\
         <w:footerReference w:type=\"default\" r:id=\"rId11\"/>\
         <w:footerReference w:type=\"first\" r:id=\"rId12\"/>\
         <w:titlePg/></w:sectPr>",
    );
    let s = &story.sections(&tree, &rels())[0];
    assert_eq!(name(&s.headers.default), Some("word/header1.xml"));
    assert_eq!(name(&s.headers.first), Some("word/header2.xml"));
    assert_eq!(name(&s.headers.even), Some("word/header3.xml"));
    assert_eq!(name(&s.footers.default), Some("word/footer1.xml"));
    assert_eq!(name(&s.footers.first), Some("word/footer2.xml"));
    assert_eq!(name(&s.footers.even), None);
}

#[test]
fn a_section_that_names_no_reference_of_a_kind_takes_the_previous_ones() {
    // Proves: inheritance, kind by kind and separately for headers and footers. The first section
    // with none has none; a later one takes the nearest earlier one; one that names a kind keeps its
    // own and still inherits the others; `own_headers` keeps what it wrote.
    let (tree, story) = scan(&format!(
        "{}{}{}\
         <w:sectPr><w:footerReference w:type=\"default\" r:id=\"rId12\"/></w:sectPr>",
        sect_p(""),
        sect_p(
            "<w:headerReference w:type=\"default\" r:id=\"rId1\"/>\
             <w:headerReference w:type=\"first\" r:id=\"rId2\"/>\
             <w:footerReference w:type=\"default\" r:id=\"rId11\"/>"
        ),
        sect_p("<w:headerReference w:type=\"default\" r:id=\"rId3\"/>"),
    ));
    let s = story.sections(&tree, &rels());
    assert_eq!(s.len(), 4);
    assert_eq!(s[0].headers, PartSlots::default());
    assert_eq!(s[0].footers, PartSlots::default());
    assert_eq!(name(&s[1].headers.default), Some("word/header1.xml"));
    assert_eq!(name(&s[1].headers.first), Some("word/header2.xml"));
    assert_eq!(name(&s[2].headers.default), Some("word/header3.xml"));
    assert_eq!(name(&s[2].headers.first), Some("word/header2.xml"));
    assert_eq!(name(&s[2].footers.default), Some("word/footer1.xml"));
    assert_eq!(name(&s[3].headers.default), Some("word/header3.xml"));
    assert_eq!(name(&s[3].footers.default), Some("word/footer2.xml"));
    assert_eq!(name(&s[3].headers.first), Some("word/header2.xml"));
    assert_eq!(s[2].own_headers.first, None);
    assert_eq!(name(&s[2].own_headers.default), Some("word/header3.xml"));
}

#[test]
fn a_reference_that_resolves_to_nothing_is_none_and_does_not_inherit() {
    // Proves: a relationship id that is missing, one of another type, one that points outside the
    // package and one with an invalid target all give `None` for that slot; the reference was
    // written, so the section does not take the previous one's, and the next section inherits the
    // `None`.
    let mut all = rels();
    all = Rels::new(
        all.iter()
            .cloned()
            .chain([
                rel(
                    "rExt",
                    "header",
                    RelTarget::External("http://example.com/h".into()),
                ),
                rel("rBad", "header", RelTarget::Invalid),
                part_rel("rWrong", "footer", "word/footer9.xml"),
            ])
            .collect(),
    );
    let (tree, story) = scan(&format!(
        "{}{}{}<w:sectPr/>",
        sect_p(
            "<w:headerReference w:type=\"default\" r:id=\"rId1\"/>\
             <w:headerReference w:type=\"first\" r:id=\"rId2\"/>\
             <w:headerReference w:type=\"even\" r:id=\"rId3\"/>"
        ),
        sect_p(
            "<w:headerReference w:type=\"default\" r:id=\"rId404\"/>\
             <w:headerReference w:type=\"first\" r:id=\"rExt\"/>\
             <w:headerReference w:type=\"even\" r:id=\"rWrong\"/>"
        ),
        sect_p("<w:headerReference w:type=\"default\" r:id=\"rBad\"/>"),
    ));
    let s = story.sections(&tree, &all);
    assert_eq!(name(&s[0].headers.default), Some("word/header1.xml"));
    assert_eq!(s[1].headers, PartSlots::default());
    assert_eq!(s[2].headers.default, None);
    assert_eq!(s[2].headers.first, None);
    assert_eq!(s[3].headers, s[2].headers);
}

#[test]
fn a_reference_with_no_id_is_written_and_broken_and_one_with_no_type_is_skipped() {
    // Proves: a reference that has a `w:type` and no `r:id` counts as written and gives `None`
    // without inheriting; a reference with no `w:type` (or an unknown one) is skipped as if absent,
    // so that kind is inherited.
    let (tree, story) = scan(&format!(
        "{}<w:sectPr><w:headerReference w:type=\"first\"/>\
         <w:headerReference r:id=\"rId3\"/><w:headerReference w:type=\"odd\" r:id=\"rId3\"/></w:sectPr>",
        sect_p(
            "<w:headerReference w:type=\"default\" r:id=\"rId1\"/>\
             <w:headerReference w:type=\"first\" r:id=\"rId2\"/>"
        ),
    ));
    let s = story.sections(&tree, &rels());
    assert_eq!(name(&s[1].headers.default), Some("word/header1.xml"));
    assert_eq!(s[1].headers.first, None);
    assert_eq!(s[1].headers.even, None);
}

#[test]
fn a_deleted_or_moved_away_mark_removes_the_section_in_final() {
    // Proves: the mode rule. A `w:sectPr` on a paragraph whose mark is deleted or moved away is gone
    // in `Final` and there in `Original` and `Redline`; one whose mark is inserted or moved in is
    // gone in `Original`; a plain one and the final one exist in every mode; `mark` is the
    // paragraph's mark revision.
    let mark = |kind: &str| {
        format!(
            "<w:p><w:pPr><w:sectPr/><w:rPr><w:{kind} w:id=\"1\" w:author=\"a\"/></w:rPr></w:pPr></w:p>"
        )
    };
    let (tree, story) = scan(&format!(
        "{}{}{}{}{}<w:sectPr/>",
        sect_p(""),
        mark("del"),
        mark("ins"),
        mark("moveFrom"),
        mark("moveTo"),
    ));
    let s = story.sections(&tree, &Rels::default());
    assert_eq!(
        s.iter().map(|s| s.mark_revs).collect::<Vec<_>>(),
        [
            MarkRevs::default(),
            MarkRevs::from_flags(MARK_DELETED),
            MarkRevs::from_flags(MARK_INSERTED),
            MarkRevs::from_flags(MARK_MOVED_FROM),
            MarkRevs::from_flags(MARK_MOVED_TO),
            MarkRevs::default()
        ]
    );
    let exists = |mode| s.iter().map(|s| s.exists_in(mode)).collect::<Vec<_>>();
    assert_eq!(exists(Mode::Final), [true, false, true, false, true, true]);
    assert_eq!(
        exists(Mode::Original),
        [true, true, false, true, false, true]
    );
    assert_eq!(exists(Mode::Redline), [true; 6]);
}

#[test]
fn a_section_that_disappears_leaves_the_chain_to_those_that_remain() {
    // Proves: `own_headers` and `own_footers` let a caller rebuild inheritance for a mode. Here the
    // section with the deleted mark names a header, the section after it names none, and the static
    // reading gives the deleted section's header while the rebuilt `Final` chain gives the first's.
    let (tree, story) = scan(&format!(
        "{}<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId2\"/></w:sectPr>\
         <w:rPr><w:del w:id=\"1\" w:author=\"a\"/></w:rPr></w:pPr></w:p>\
         <w:sectPr/>",
        sect_p("<w:headerReference w:type=\"default\" r:id=\"rId1\"/>"),
    ));
    let s = story.sections(&tree, &rels());
    assert_eq!(name(&s[2].headers.default), Some("word/header2.xml"));
    let mut current = PartSlots::default();
    for sec in s.iter().filter(|s| s.exists_in(Mode::Final)) {
        if sec.own_headers.default.is_some() {
            current.default = sec.own_headers.default.clone();
        }
    }
    assert_eq!(name(&current.default), Some("word/header1.xml"));
}

#[test]
fn only_the_body_has_sections() {
    // Proves: a header story has no sections, even when its paragraphs hold a `w:sectPr`.
    let tree = part("word/header1.xml", "hdr", &sect_p(""));
    let story = Story::scan(&Arc::from("word/header1.xml"), StoryKind::Header, &tree).unwrap();
    assert!(story.sections(&tree, &rels()).is_empty());
}

#[test]
fn strict_sections_resolve_strict_relationship_types() {
    // Proves: a section in the Strict `w` and `r` namespaces, with Strict relationship types, reads
    // like a Transitional one: the break type, `w:titlePg`, the mark, and the references.
    let tree = parse(
        "word/document.xml",
        &format!(
            "<w:document xmlns:w=\"{W_S}\" xmlns:r=\"{R_S}\"><w:body>\
             <w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"rId1\"/>\
             <w:type w:val=\"continuous\"/><w:titlePg/></w:sectPr>\
             <w:rPr><w:del w:id=\"1\" w:author=\"a\"/></w:rPr></w:pPr></w:p>\
             <w:sectPr><w:footerReference w:type=\"default\" r:id=\"rId2\"/></w:sectPr>\
             </w:body></w:document>"
        ),
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let strict = |id: &str, kind: &str, name: &str| Rel {
        id: id.into(),
        rel_type: format!("{REL_S}{kind}"),
        target: RelTarget::Part(name.into()),
    };
    let rels = Rels::new(vec![
        strict("rId1", "header", "word/h.xml"),
        strict("rId2", "footer", "word/f.xml"),
    ]);
    let s = story.sections(&tree, &rels);
    assert_eq!(s.len(), 2);
    assert_eq!(s[0].break_type, SectionBreak::Continuous);
    assert!(s[0].title_pg);
    assert_eq!(s[0].mark_revs, MarkRevs::from_flags(MARK_DELETED));
    assert!(!s[0].exists_in(Mode::Final));
    assert_eq!(name(&s[0].headers.default), Some("word/h.xml"));
    assert_eq!(name(&s[1].headers.default), Some("word/h.xml"));
    assert_eq!(name(&s[1].footers.default), Some("word/f.xml"));
}
