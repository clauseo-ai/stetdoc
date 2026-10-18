//! Tests of the text model: what each shape of paragraph content becomes. Known answers from
//! hand-written XML, one rule at a time, and the invariants of `text_model.rs` checked on every map.

use super::testing::{document, parse, part};
use super::*;
use crate::xml::SpanTree;
use std::borrow::Cow;
use std::sync::Arc;

pub(super) const FINAL: Mode = Mode::Final;
pub(super) const ORIGINAL: Mode = Mode::Original;
pub(super) const REDLINE: Mode = Mode::Redline;

/// The maps of every paragraph of a body, in scan order, each checked against the invariants.
pub(super) fn maps(body: &str) -> Vec<TextMap> {
    let tree = document(body);
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    (0..story.blocks().len())
        .filter_map(|i| story.paragraph(&tree, i))
        .map(|p| {
            let m = p.map();
            check_invariants(&m);
            m
        })
        .collect()
}

/// The map of the first paragraph of a body.
pub(super) fn map(body: &str) -> TextMap {
    maps(body).into_iter().next().unwrap()
}

/// The map of the paragraph `<w:p>{content}</w:p>`.
pub(super) fn para(content: &str) -> TextMap {
    map(&format!("<w:p>{content}</w:p>"))
}

pub(super) fn texts(m: &TextMap) -> [String; 3] {
    [
        m.text(FINAL).into_owned(),
        m.text(ORIGINAL).into_owned(),
        m.text(REDLINE).into_owned(),
    ]
}

pub(super) fn kinds(m: &TextMap) -> Vec<ItemKind> {
    m.items().iter().map(|i| i.kind).collect()
}

fn run(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

/// The invariants of `text_model.rs` that a map can check on itself.
pub(super) fn check_invariants(m: &TextMap) {
    // 1 and 2: items are in order, `at` is the running count of characters, and the Redline text is
    // the concatenation of the items' text.
    let mut at = 0u32;
    let mut rebuilt = String::new();
    for item in m.items() {
        assert_eq!(item.at, at, "{item:?}");
        let text = m.slice(item);
        assert_eq!(text.chars().count() as u32, item.len, "{item:?}");
        if item.kind.is_mark() {
            assert_eq!(item.len, 0);
        }
        if let Some(c) = item.kind.sentinel() {
            assert_eq!(text, c.to_string(), "an atom is its one character");
        }
        rebuilt.push_str(text);
        at += item.len;
    }
    assert_eq!(rebuilt, m.text(REDLINE));
    // 4: a mode's text is the concatenation of the items it includes.
    for mode in [FINAL, ORIGINAL] {
        let expected: String = m
            .items()
            .iter()
            .filter(|i| i.vis.in_mode(mode))
            .map(|i| m.slice(i))
            .collect();
        assert_eq!(m.text(mode), expected, "{mode:?}");
        assert_eq!(m.len(mode), expected.chars().count());
    }
    if !m.has_revisions() {
        assert!(matches!(m.text(FINAL), Cow::Borrowed(_)));
        assert_eq!(m.text(FINAL), m.text(ORIGINAL));
        assert_eq!(m.text(FINAL), m.text(REDLINE));
    }
    // 5: field code is in no text.
    assert!(
        m.items()
            .iter()
            .all(|i| i.kind != ItemKind::FieldCode || i.len == 0)
    );
}

// ---- text, runs and atoms -----------------------------------------------------------------------

#[test]
fn runs_concatenate_into_one_text_that_is_the_same_in_every_mode() {
    let m = para(&format!("{}{}", run("Hello"), run(" world")));
    assert_eq!(texts(&m), ["Hello world", "Hello world", "Hello world"]);
    assert!(!m.has_revisions());
    assert!(matches!(m.text(FINAL), Cow::Borrowed(_)));
    let spans: Vec<(u32, u32, u32)> = m.items().iter().map(|i| (i.at, i.len, i.from)).collect();
    assert_eq!(spans, [(0, 5, 0), (5, 6, 0)]);
    assert_eq!(m.len(FINAL), 11);
    assert!(
        m.items()
            .iter()
            .all(|i| i.kind == ItemKind::Text && i.run.is_some())
    );
}

#[test]
fn an_empty_paragraph_and_the_paragraph_properties_have_no_items() {
    let m = para("");
    assert!(m.items().is_empty() && m.is_empty(FINAL));
    let m = para(
        "<w:pPr><w:pStyle w:val=\"Heading1\"/><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:t></w:t></w:r>",
    );
    assert!(
        m.items().is_empty(),
        "w:pPr is not content and an empty w:t has no text node"
    );
}

#[test]
fn each_atom_is_one_character_of_its_kind() {
    let m = para(
        "<w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:br w:type=\"textWrapping\"/><w:cr/><w:br w:type=\"page\"/>\
         <w:br w:type=\"column\"/><w:noBreakHyphen/><w:softHyphen/><w:ptab w:alignment=\"left\"/><w:drawing/><w:pict/>\
         <w:object/><w:footnoteReference w:id=\"2\"/><w:endnoteReference w:id=\"3\"/><w:footnoteRef/><w:pgNum/>\
         <w:dayShort/></w:r><m:oMathPara/><m:oMath/>",
    );
    use ItemKind::*;
    assert_eq!(
        kinds(&m),
        [
            Text,
            Tab,
            Text,
            LineBreak,
            LineBreak,
            LineBreak,
            PageBreak,
            ColumnBreak,
            Hyphen,
            SoftHyphen,
            Tab,
            Object,
            Object,
            Object,
            NoteRef,
            NoteRef,
            NoteMark,
            Auto,
            Auto,
            Math,
            Math
        ]
    );
    assert_eq!(
        m.text(REDLINE),
        "a\tb\n\n\n\u{c}\u{e}\u{2011}\u{ad}\t\u{1}\u{1}\u{1}\u{2}\u{2}\u{3}\u{5}\u{5}\u{4}\u{4}"
    );
    assert!(
        m.items()
            .iter()
            .filter(|i| i.kind != Text)
            .all(|i| i.len == 1)
    );
}

#[test]
fn a_symbol_is_the_code_point_of_its_char_and_a_bad_one_is_the_replacement_character() {
    let m = para(
        "<w:r><w:sym w:font=\"Wingdings\" w:char=\"F0FC\"/><w:sym w:char=\"0x0041\"/><w:sym w:char=\"0001\"/>\
         <w:sym w:char=\"zz\"/><w:sym/><w:sym w:char=\"D800\"/><w:sym w:char=\"FFFF\"/></w:r>",
    );
    assert_eq!(
        m.text(REDLINE),
        "\u{f0fc}A\u{fffd}\u{fffd}\u{fffd}\u{fffd}\u{fffd}"
    );
    assert!(
        m.items()
            .iter()
            .all(|i| i.kind == ItemKind::Symbol && i.len == 1)
    );
    let bad = m
        .anomalies()
        .iter()
        .filter(|a| a.kind == AnomalyKind::InvalidSymbol)
        .count();
    assert_eq!(bad, 5);
}

#[test]
fn text_keeps_references_decoded_line_endings_normalized_and_literal_tabs() {
    let m = para("<w:r><w:t>a&amp;b&#x41;&#xA0;</w:t><w:t>x\ty\nz</w:t></w:r>");
    assert_eq!(m.text(FINAL), "a&bA\u{a0}x\ty\nz");
    let crlf = para("<w:r><w:t>a\r\nb&#13;c</w:t></w:r>");
    assert_eq!(
        crlf.text(FINAL),
        "a\nb\rc",
        "a CRLF pair is one character, a reference stays a carriage return"
    );
    assert_eq!(crlf.items()[0].len, 5);
}

#[test]
fn a_cdata_text_node_is_flagged_and_a_comment_inside_w_t_splits_it_into_two_items() {
    let m = para("<w:r><w:t><![CDATA[a<b]]></w:t><w:t>x<!--c-->y</w:t></w:r>");
    assert_eq!(m.text(FINAL), "a<bxy");
    let flags: Vec<u8> = m.items().iter().map(|i| i.flags & CDATA).collect();
    assert_eq!(flags, [CDATA, 0, 0]);
    assert_eq!(m.items().len(), 3, "the comment separates two text nodes");
    let nodes: Vec<_> = m.items().iter().map(|i| i.node).collect();
    assert_ne!(nodes[1], nodes[2]);
}

#[test]
fn what_is_skipped_silently_is_not_an_anomaly() {
    let m = para(
        "<w:proofErr w:type=\"spellStart\"/><w:r><w:rPr><w:b/></w:rPr><w:lastRenderedPageBreak/><w:t>a</w:t>\
         <w:annotationRef/></w:r><w:proofErr w:type=\"spellEnd\"/>",
    );
    assert_eq!(m.text(FINAL), "a");
    assert!(m.anomalies().is_empty(), "{:?}", m.anomalies());
}

#[test]
fn unknown_and_misplaced_elements_are_skipped_with_their_content_and_counted() {
    let m = para(
        "<w:foo><w:r><w:t>hidden</w:t></w:r></w:foo><x:ext xmlns:x=\"urn:x\"><w:r><w:t>also hidden</w:t></w:r></x:ext>\
         <w:p><w:r><w:t>nested</w:t></w:r></w:p><w:tbl/><w:r><w:t>a</w:t><w:unknownRunChild/></w:r>",
    );
    assert_eq!(m.text(FINAL), "a");
    let kinds: Vec<AnomalyKind> = m.anomalies().iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        [
            AnomalyKind::UnknownElement,
            AnomalyKind::UnknownElement,
            AnomalyKind::NestedBlock,
            AnomalyKind::NestedBlock,
            AnomalyKind::UnknownElement
        ],
        "an element in a foreign namespace is skipped with its content and counted like any other"
    );
}

#[test]
fn a_run_inside_a_run_is_not_read_and_the_ruby_base_is() {
    let m = para("<w:r><w:r><w:t>inner</w:t></w:r></w:r>");
    assert!(m.text(FINAL).is_empty());
    let ruby = para(
        "<w:r><w:ruby><w:rt><w:r><w:t>kana</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>base</w:t></w:r></w:rubyBase></w:ruby></w:r>",
    );
    assert_eq!(ruby.text(FINAL), "base");
    assert_eq!(
        ruby.context(ruby.items()[0].ctx).next().unwrap().kind,
        WrapperKind::Ruby
    );
}

#[test]
fn whitespace_without_xml_space_is_read_as_written_and_counted() {
    let m = para(
        "<w:r><w:t> a </w:t></w:r><w:r><w:t xml:space=\"preserve\"> b </w:t></w:r><w:r><w:t>\u{a0}c</w:t></w:r>",
    );
    assert_eq!(m.text(FINAL), " a  b \u{a0}c");
    let edge = m
        .anomalies()
        .iter()
        .filter(|a| a.kind == AnomalyKind::EdgeSpaceWithoutPreserve)
        .count();
    assert_eq!(
        edge, 1,
        "only the first: preserve is set on the second, and a no-break space is not XML whitespace"
    );
}

// ---- marks --------------------------------------------------------------------------------------

#[test]
fn marks_have_a_position_and_no_width() {
    let m = para(&format!(
        "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}<w:commentRangeStart w:id=\"2\"/>{}<w:commentRangeEnd w:id=\"2\"/>\
         <w:r><w:commentReference w:id=\"2\"/></w:r><w:bookmarkEnd w:id=\"1\"/><w:permStart w:id=\"9\"/><w:permEnd w:id=\"9\"/>\
         <w:moveFromRangeStart w:id=\"3\"/><w:moveToRangeStart w:id=\"4\"/><w:moveFromRangeEnd w:id=\"3\"/><w:moveToRangeEnd w:id=\"4\"/>",
        run("ab"),
        run("cd")
    ));
    use ItemKind::*;
    assert_eq!(
        kinds(&m),
        [
            BookmarkStart,
            Text,
            CommentStart,
            Text,
            CommentEnd,
            CommentRef,
            BookmarkEnd,
            PermStart,
            PermEnd,
            MoveRangeStart,
            MoveRangeStart,
            MoveRangeEnd,
            MoveRangeEnd
        ]
    );
    let at: Vec<u32> = m.items().iter().map(|i| i.at).collect();
    assert_eq!(at, [0, 0, 2, 2, 4, 4, 4, 4, 4, 4, 4, 4, 4]);
    assert_eq!(m.text(FINAL), "abcd");
}

// ---- wrappers -----------------------------------------------------------------------------------

fn chain(m: &TextMap, item: usize) -> Vec<WrapperKind> {
    m.context(m.items()[item].ctx).map(|c| c.kind).collect()
}

#[test]
fn wrappers_are_transparent_and_each_item_knows_its_chain_innermost_first() {
    let m = para(&format!(
        "<w:hyperlink w:anchor=\"a\">{}</w:hyperlink><w:smartTag w:element=\"x\"><w:smartTag w:element=\"y\">{}</w:smartTag></w:smartTag>\
         <w:customXml w:element=\"c\">{}</w:customXml><w:sdt><w:sdtPr><w:alias w:val=\"t\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>\
         <w:dir w:val=\"rtl\"><w:bdo w:val=\"ltr\">{}</w:bdo></w:dir>{}",
        run("1"),
        run("2"),
        run("3"),
        run("4"),
        run("5"),
        run("6")
    ));
    assert_eq!(m.text(FINAL), "123456");
    use WrapperKind::*;
    assert_eq!(chain(&m, 0), [Hyperlink]);
    assert_eq!(chain(&m, 1), [SmartTag, SmartTag]);
    assert_eq!(chain(&m, 2), [CustomXml]);
    assert_eq!(chain(&m, 3), [Control]);
    assert_eq!(chain(&m, 4), [Bdo, Dir]);
    assert!(chain(&m, 5).is_empty());
    assert!(!m.has_revisions());
}

#[test]
fn a_paragraph_knows_the_wrappers_above_it_up_to_the_story_root() {
    let ms = maps(
        "<w:sdt><w:sdtContent><w:p><w:r><w:t>a</w:t></w:r></w:p></w:sdtContent></w:sdt>\
         <w:moveFrom><w:p><w:r><w:delText>b</w:delText></w:r></w:p></w:moveFrom>\
         <w:sdt><w:sdtContent><w:tbl><w:tr><w:tc><w:p><w:r><w:t>c</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:sdtContent></w:sdt>",
    );
    assert_eq!(ms.len(), 3);
    assert_eq!(chain(&ms[0], 0), [WrapperKind::Control]);
    assert_eq!(
        texts(&ms[1]),
        ["", "b", "b"],
        "a paragraph in a body-level moveFrom is deleted text"
    );
    assert_eq!(chain(&ms[1], 0), [WrapperKind::MoveFrom]);
    assert_eq!(
        chain(&ms[2], 0),
        [WrapperKind::Control],
        "a control around a whole table reaches its paragraphs"
    );
}

// ---- revisions ----------------------------------------------------------------------------------

#[test]
fn an_insertion_and_a_deletion_make_the_three_texts_differ() {
    let m = para(&format!(
        "{}<w:ins w:id=\"1\">{}</w:ins>{}<w:del w:id=\"2\"><w:r><w:delText>C</w:delText></w:r></w:del>{}",
        run("A "),
        run("B"),
        run(" "),
        run("D")
    ));
    assert_eq!(texts(&m), ["A B D", "A  CD", "A B CD"]);
    assert!(m.has_revisions());
    assert_eq!(m.len(FINAL), 5);
    assert_eq!(m.len(ORIGINAL), 5);
    assert_eq!(m.len(REDLINE), 6);
    let vis: Vec<(u8, usize)> = m
        .items()
        .iter()
        .enumerate()
        .map(|(i, it)| (it.vis.0, i))
        .collect();
    assert_eq!(vis.iter().map(|v| v.0).collect::<Vec<_>>(), [3, 1, 3, 2, 3]);
}

#[test]
fn moves_follow_the_same_rule_as_insertions_and_deletions() {
    let m = para(&format!(
        "<w:moveFrom w:id=\"1\"><w:r><w:delText>gone</w:delText></w:r></w:moveFrom><w:moveTo w:id=\"2\">{}</w:moveTo>",
        run("here")
    ));
    assert_eq!(texts(&m), ["here", "gone", "gonehere"]);
}

#[test]
fn a_revision_inside_a_revision_is_in_neither_mode() {
    let m = para(&format!(
        "<w:ins w:id=\"1\"><w:del w:id=\"2\"><w:r><w:delText>x</w:delText></w:r></w:del>{}</w:ins>\
         <w:moveFrom w:id=\"3\"><w:ins w:id=\"4\">{}</w:ins></w:moveFrom><w:moveTo w:id=\"5\"><w:del w:id=\"6\"><w:r><w:delText>z</w:delText></w:r></w:del></w:moveTo>",
        run("y"),
        run("w")
    ));
    assert_eq!(texts(&m), ["y", "", "xywz"]);
    let vis: Vec<u8> = m.items().iter().map(|i| i.vis.0).collect();
    assert_eq!(
        vis,
        [0, 1, 0, 0],
        "ins then del, ins, moveFrom then ins, moveTo then del"
    );
    assert_eq!(chain(&m, 0), [WrapperKind::Del, WrapperKind::Ins]);
}

#[test]
fn element_names_do_not_decide_a_deleted_w_t_is_deleted_and_a_stray_delete_text_is_plain() {
    let m = para(
        "<w:del w:id=\"1\"><w:r><w:t>a</w:t></w:r></w:del><w:r><w:delText>b</w:delText></w:r>",
    );
    assert_eq!(texts(&m), ["b", "ab", "ab"]);
    let kinds: Vec<AnomalyKind> = m.anomalies().iter().map(|a| a.kind).collect();
    assert_eq!(
        kinds,
        [
            AnomalyKind::TextInDeletion,
            AnomalyKind::DelTextOutsideDeletion
        ]
    );
}

#[test]
fn a_paragraph_in_a_row_or_a_cell_marked_by_a_revision_takes_the_row_or_cell_wrapper() {
    let ms = maps(
        "<w:tbl><w:tr><w:trPr><w:ins w:id=\"1\"/></w:trPr><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc></w:tr>\
         <w:tr><w:trPr><w:del w:id=\"2\"/></w:trPr><w:tc><w:p><w:r><w:delText>3</w:delText></w:r></w:p></w:tc></w:tr>\
         <w:tr><w:tc><w:tcPr><w:cellIns w:id=\"3\"/></w:tcPr><w:p><w:r><w:t>4</w:t></w:r></w:p></w:tc>\
         <w:tc><w:tcPr><w:cellDel w:id=\"4\"/></w:tcPr><w:p><w:r><w:t>5</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    assert_eq!(
        texts(&ms[0]),
        ["2", "", "2"],
        "the runs carry no wrapper of their own"
    );
    assert_eq!(chain(&ms[0], 0), [WrapperKind::RowIns]);
    assert_eq!(texts(&ms[1]), ["", "3", "3"]);
    assert_eq!(chain(&ms[1], 0), [WrapperKind::RowDel]);
    assert_eq!(texts(&ms[2]), ["4", "", "4"]);
    assert_eq!(texts(&ms[3]), ["", "5", "5"]);
}

// ---- alternate content, math, text boxes --------------------------------------------------------

#[test]
fn an_alternate_content_in_a_run_is_one_object_and_its_text_box_is_not_paragraph_text() {
    let m = para(&format!(
        "{}<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wps:txbx><w:txbxContent><w:p><w:r><w:t>in the box</w:t></w:r></w:p></w:txbxContent></wps:txbx></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><w:txbxContent><w:p><w:r><w:t>in the box</w:t></w:r></w:p></w:txbxContent></w:pict><w:pict/></mc:Fallback></mc:AlternateContent></w:r>",
        run("a")
    ));
    assert_eq!(m.text(FINAL), "a\u{1}");
    assert_eq!(kinds(&m), [ItemKind::Text, ItemKind::Object]);
    assert_eq!(chain(&m, 1), [WrapperKind::Alternate]);
}

#[test]
fn an_alternate_content_with_no_usable_branch_contributes_nothing_and_is_counted() {
    let m = para(&format!(
        "{}<w:r><mc:AlternateContent><mc:Choice Requires=\"unbound\"><w:drawing/></mc:Choice></mc:AlternateContent></w:r>",
        run("a")
    ));
    assert_eq!(m.text(FINAL), "a");
    assert_eq!(m.anomalies().len(), 1);
    assert_eq!(m.anomalies()[0].kind, AnomalyKind::NoBranch);
}

#[test]
fn an_alternate_content_between_runs_reads_the_taken_branch() {
    let m = para(&format!(
        "<mc:AlternateContent><mc:Choice Requires=\"w14\">{}</mc:Choice><mc:Fallback>{}</mc:Fallback></mc:AlternateContent>",
        run("choice"),
        run("fallback")
    ));
    assert_eq!(m.text(FINAL), "choice");
}

// ---- paragraph accessors ------------------------------------------------------------------------

fn paragraph_of<'a>(story: &'a Story, tree: &'a SpanTree, i: usize) -> Paragraph<'a> {
    story.paragraph(tree, i).unwrap()
}

#[test]
fn a_paragraph_reads_its_style_numbering_section_break_and_mark() {
    let tree = document(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:numPr><w:ilvl w:val=\"2\"/><w:numId w:val=\"7\"/></w:numPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:del w:id=\"1\"/></w:rPr><w:sectPr/></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:ins w:id=\"2\"/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:moveFrom w:id=\"3\"/></w:rPr></w:pPr></w:p>\
         <w:p><w:pPr><w:rPr><w:moveTo w:id=\"4\"/></w:rPr></w:pPr></w:p>\
         <w:p/>",
    );
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let p = |i| paragraph_of(&story, &tree, i);
    assert_eq!(p(0).style_id().as_deref(), Some("Heading1"));
    assert_eq!(
        p(0).num_pr(),
        Some(NumPr {
            num_id: Some(7),
            ilvl: Some(2)
        })
    );
    assert!(p(0).section_break().is_none() && p(0).mark_revs().is_none());
    assert!(p(1).section_break().is_some());
    assert_eq!(p(1).mark_revs(), MarkRevs::from_flags(MARK_DELETED));
    assert_eq!(p(2).mark_revs(), MarkRevs::from_flags(MARK_INSERTED));
    assert_eq!(p(3).mark_revs(), MarkRevs::from_flags(MARK_MOVED_FROM));
    assert_eq!(p(4).mark_revs(), MarkRevs::from_flags(MARK_MOVED_TO));
    assert_eq!(
        (p(5).style_id(), p(5).num_pr(), p(5).mark_revs()),
        (None, None, MarkRevs::default())
    );
    // a deleted mark joins in Final, an inserted one in Original, and Redline never joins
    let joins = |i, mode| p(i).joins_next(mode);
    assert!(joins(1, FINAL) && !joins(1, ORIGINAL) && !joins(1, REDLINE));
    assert!(joins(2, ORIGINAL) && !joins(2, FINAL));
    assert!(joins(3, FINAL) && joins(4, ORIGINAL));
    assert!(!joins(0, FINAL) && !joins(5, ORIGINAL));
    // entries 1, 2, 3, 4: the entry flags say the same without reading the paragraph
    assert!(story.blocks()[1].has(MARK_DELETED | SECTION_BREAK));
}

#[test]
fn a_paragraph_text_is_the_text_of_its_map_and_new_needs_a_paragraph() {
    let tree = document("<w:p><w:r><w:t>x</w:t></w:r></w:p><w:tbl/>");
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    assert_eq!(paragraph_of(&story, &tree, 0).text(FINAL), "x");
    assert!(story.paragraph(&tree, 1).is_none(), "entry 1 is a table");
    assert!(story.paragraph(&tree, 9).is_none());
    let id = story.blocks()[0].node;
    assert_eq!(Paragraph::new(&tree, id).unwrap().text(REDLINE), "x");
    assert!(Paragraph::new(&tree, story.blocks()[1].node).is_none());
}

// ---- size and shape -----------------------------------------------------------------------------

#[test]
fn a_hundred_thousand_runs_and_deep_wrappers_are_linear() {
    let runs: String = (0..100_000).map(|_| "<w:r><w:t>x</w:t></w:r>").collect();
    let m = para(&runs);
    assert_eq!(m.len(FINAL), 100_000);
    let mut deep = String::from("<w:r><w:t>x</w:t></w:r>");
    for _ in 0..120 {
        deep = format!("<w:hyperlink>{deep}</w:hyperlink>");
    }
    let m = para(&deep);
    assert_eq!(m.text(FINAL), "x");
    assert_eq!(m.context(m.items()[0].ctx).count(), 120);
}

#[test]
fn a_part_other_than_the_body_works_the_same_through_paragraph_new() {
    let tree = part(
        "word/footnotes.xml",
        "footnotes",
        "<w:footnote w:id=\"2\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space=\"preserve\"> Note.</w:t></w:r></w:p></w:footnote>",
    );
    let story = Story::scan(
        &Arc::from("word/footnotes.xml"),
        StoryKind::Footnotes,
        &tree,
    )
    .unwrap();
    let m = story.paragraph(&tree, 0).unwrap().map();
    assert_eq!(m.text(FINAL), "\u{3} Note.");
    let tree = parse(
        "t.xml",
        "<w:p xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:r><w:t>q</w:t></w:r></w:p>",
    );
    assert_eq!(
        Paragraph::new(&tree, tree.root().id()).unwrap().text(FINAL),
        "q"
    );
}
