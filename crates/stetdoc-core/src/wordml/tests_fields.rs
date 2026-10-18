//! Tests of fields in the text model: complex and simple fields, nesting, fields with no result,
//! fields across paragraphs, and recovery from fields that are not well formed.

use super::testing::part;
use super::tests_text::{FINAL, ORIGINAL, REDLINE, check_invariants, kinds, maps, para, texts};
use super::*;
use std::sync::Arc;

pub(super) fn begin() -> String {
    "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r>".to_string()
}
pub(super) fn separate() -> String {
    "<w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>".to_string()
}
pub(super) fn end() -> String {
    "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>".to_string()
}
pub(super) fn instr(s: &str) -> String {
    format!("<w:r><w:instrText xml:space=\"preserve\">{s}</w:instrText></w:r>")
}
pub(super) fn text(s: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{s}</w:t></w:r>")
}

/// A complete complex field with a result.
fn field(code: &str, result: &str) -> String {
    format!(
        "{}{}{}{}{}",
        begin(),
        instr(code),
        separate(),
        text(result),
        end()
    )
}

fn ats(m: &TextMap, kind: ItemKind) -> Vec<u32> {
    m.items()
        .iter()
        .filter(|i| i.kind == kind)
        .map(|i| i.at)
        .collect()
}

// ---- a complex field ----------------------------------------------------------------------------

#[test]
fn field_code_is_not_text_and_the_result_is_flagged() {
    let m = para(&format!(
        "{}{}{}",
        text("see section "),
        field(" REF _Ref1 ", "4.2"),
        text(" above")
    ));
    assert_eq!(texts(&m), ["see section 4.2 above"; 3]);
    use ItemKind::*;
    assert_eq!(
        kinds(&m),
        [
            Text,
            FieldBegin,
            FieldCode,
            FieldSeparate,
            Text,
            FieldEnd,
            Text
        ]
    );
    assert_eq!(ats(&m, FieldBegin), [12]);
    assert_eq!(ats(&m, FieldCode), [12]);
    assert_eq!(ats(&m, FieldSeparate), [12]);
    assert_eq!(ats(&m, FieldEnd), [15]);
    let result = &m.items()[4];
    assert_eq!((result.at, result.len), (12, 3));
    assert_eq!(result.flags & FIELD_RESULT, FIELD_RESULT);
    assert_eq!(result.field, 0);
    assert_eq!(
        m.items()[0].flags & FIELD_RESULT,
        0,
        "text before the field is not a result"
    );
    assert_eq!(m.items()[6].field, NO_FIELD);
    assert_eq!(m.items()[6].flags & FIELD_RESULT, 0, "nor is text after it");
    let f = &m.fields()[0];
    assert_eq!(
        (f.id, f.begins_here, f.simple, f.parent),
        (0, true, false, None)
    );
    assert_eq!(f.instr, " REF _Ref1 ");
    assert!(f.separate.is_some() && f.end.is_some());
}

#[test]
fn an_instruction_split_over_runs_and_deleted_instruction_text_is_assembled() {
    let m = para(&format!(
        "{}{}<w:r><w:delInstrText> \\h</w:delInstrText></w:r>{}{}{}",
        begin(),
        instr(" PAGEREF "),
        instr("_Toc12"),
        separate(),
        text("7") + &end()
    ));
    assert_eq!(
        m.fields()[0].instr,
        " PAGEREF _Toc12 \\h".replace("_Toc12 \\h", " \\h_Toc12")
    );
    assert_eq!(m.text(FINAL), "7");
}

#[test]
fn a_field_nested_in_another_fields_code_is_part_of_that_code() {
    // IF { DOCVARIABLE x } = 1 "yes" "no": the inner result is code of the outer field
    let m = para(&format!(
        "{}{}{}{}{}{}{}{}{}",
        begin(),
        instr(" IF "),
        field(" DOCVARIABLE x ", "\u{ab}x\u{bb}"),
        instr(" = 1 "),
        separate(),
        text("RESULT"),
        end(),
        text("."),
        ""
    ));
    assert_eq!(
        m.text(FINAL),
        "RESULT.",
        "the inner result is field code, and code is in no text"
    );
    let fields = m.fields();
    assert_eq!(fields.len(), 2);
    assert_eq!((fields[0].id, fields[0].parent), (0, None));
    assert_eq!((fields[1].id, fields[1].parent), (1, Some(0)));
    assert_eq!(fields[0].instr, " IF  = 1 ");
    assert_eq!(
        m.anomalies()
            .iter()
            .filter(|a| a.kind == AnomalyKind::TextInFieldCode)
            .count(),
        1
    );
}

#[test]
fn a_field_in_a_result_keeps_the_innermost_id_and_both_are_results() {
    // a table of contents entry: PAGEREF inside the result of TOC
    let m = para(&format!(
        "{}{}{}{}{}{}{}",
        begin(),
        instr(" TOC "),
        separate(),
        text("Heading "),
        field(" PAGEREF h ", "3"),
        text("!"),
        end()
    ));
    assert_eq!(m.text(FINAL), "Heading 3!");
    let texts: Vec<(String, u32, u8)> = m
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::Text)
        .map(|i| (m.slice(i).to_string(), i.field, i.flags & FIELD_RESULT))
        .collect();
    assert_eq!(
        texts,
        [
            ("Heading ".into(), 0, FIELD_RESULT),
            ("3".into(), 1, FIELD_RESULT),
            ("!".into(), 0, FIELD_RESULT)
        ]
    );
    assert_eq!(m.fields()[1].parent, Some(0));
}

// ---- fields with no result ----------------------------------------------------------------------

#[test]
fn a_field_with_no_result_that_word_shows_is_one_atom_and_the_others_are_empty() {
    let shown = |code: &str| {
        para(&format!(
            "{}{}{}{}",
            text("["),
            begin() + &instr(code),
            end(),
            text("]")
        ))
    };
    for code in [
        " PAGE ",
        " EQ \\f(1,2) ",
        " SYMBOL 65 ",
        " LISTNUM x ",
        " MACROBUTTON NoMacro [Click] ",
        " GOTOBUTTON a [b] ",
        " QUOTE \"q\" ",
        " BARCODE x ",
    ] {
        let m = shown(code);
        assert_eq!(m.text(FINAL), "[\u{6}]", "{code}");
        let atom = m
            .items()
            .iter()
            .find(|i| i.kind == ItemKind::FieldValue)
            .unwrap();
        assert_eq!(atom.len, 1);
        assert_eq!(atom.flags & FIELD_RESULT, FIELD_RESULT);
        assert_eq!(atom.field, 0);
    }
    for code in [
        " IF x ",
        " REF y ",
        " XE \"z\" ",
        " TA \\l \"a\" ",
        " NOSUCH ",
    ] {
        let m = shown(code);
        assert_eq!(m.text(FINAL), "[]", "{code}");
        assert!(m.items().iter().all(|i| i.kind != ItemKind::FieldValue));
    }
}

#[test]
fn a_field_that_is_shown_inside_another_fields_code_stays_code() {
    let m = para(&format!(
        "{}{}{}{}{}",
        begin(),
        instr(" IF "),
        begin() + &instr(" PAGE ") + &end(),
        separate(),
        text("r") + &end()
    ));
    assert_eq!(m.text(FINAL), "r");
}

#[test]
fn display_text_comes_from_the_code_of_a_macro_button() {
    let tree = super::testing::document(&format!(
        "<w:p>{}{}{}</w:p>",
        begin(),
        instr(" MACROBUTTON NoMacro [Name - jump to next field with F11] "),
        end()
    ));
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let fields = story.fields(&tree);
    assert_eq!(fields.len(), 1);
    assert_eq!(&*fields[0].keyword, "MACROBUTTON");
    assert_eq!(
        fields[0].display_text().as_deref(),
        Some("[Name - jump to next field with F11]")
    );
    let page = FieldInfo {
        instr: " PAGE ".into(),
        keyword: "PAGE".into(),
        ..fields[0].clone()
    };
    assert_eq!(page.display_text(), None);
}

// ---- wrappers and revisions ---------------------------------------------------------------------

#[test]
fn a_field_begun_in_an_insertion_and_ended_outside_it_still_closes() {
    let m = para(&format!(
        "<w:ins w:id=\"1\">{}{}{}{}</w:ins>{}{}",
        begin(),
        instr(" REF a "),
        separate(),
        text("R"),
        end(),
        text("after")
    ));
    assert_eq!(texts(&m), ["Rafter", "after", "Rafter"]);
    let r = m.items().iter().find(|i| m.slice(i) == "R").unwrap();
    let a = m.items().iter().find(|i| m.slice(i) == "after").unwrap();
    assert_eq!(
        (r.flags & FIELD_RESULT, a.flags & FIELD_RESULT),
        (FIELD_RESULT, 0)
    );
}

#[test]
fn a_field_deleted_whole_is_in_original_and_not_in_final() {
    let m = para(&format!(
        "<w:del w:id=\"1\">{}<w:r><w:delInstrText> REF a </w:delInstrText></w:r>{}<w:r><w:delText>gone</w:delText></w:r>{}</w:del>",
        begin(),
        separate(),
        end()
    ));
    assert_eq!(texts(&m), ["", "gone", "gone"]);
    assert_eq!(m.fields()[0].instr, " REF a ");
    assert!(
        m.items()
            .iter()
            .any(|i| i.kind == ItemKind::Text && i.flags & FIELD_RESULT != 0)
    );
}

// ---- simple fields ------------------------------------------------------------------------------

#[test]
fn a_simple_field_is_a_wrapper_whose_content_is_the_result() {
    let m = para(&format!(
        "{}<w:fldSimple w:instr=\" PAGE \">{}</w:fldSimple>{}",
        text("page "),
        text("5"),
        text(" of 9")
    ));
    assert_eq!(m.text(FINAL), "page 5 of 9");
    use ItemKind::*;
    assert_eq!(kinds(&m), [Text, FieldBegin, Text, FieldEnd, Text]);
    let five = &m.items()[2];
    assert_eq!((five.flags & FIELD_RESULT, five.field), (FIELD_RESULT, 0));
    assert_eq!(m.items()[4].field, NO_FIELD);
    assert!(m.fields()[0].simple);
    assert_eq!(m.fields()[0].instr, " PAGE ");
    assert_eq!(
        m.context(five.ctx).next().map(|c| c.kind),
        Some(WrapperKind::FieldSimple)
    );
}

#[test]
fn an_empty_simple_field_shows_a_value_when_word_does_and_nothing_otherwise() {
    let m = para("<w:fldSimple w:instr=\" PAGE \"/><w:fldSimple w:instr=\" REF x \"/>");
    assert_eq!(m.text(FINAL), "\u{6}");
}

#[test]
fn simple_and_complex_fields_nest() {
    let m = para(&format!(
        "<w:fldSimple w:instr=\" IF a \">{}{}</w:fldSimple>",
        text("x"),
        field(" REF b ", "y")
    ));
    assert_eq!(m.text(FINAL), "xy");
    let ids: Vec<u32> = m
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::Text)
        .map(|i| i.field)
        .collect();
    assert_eq!(ids, [0, 1]);
    assert_eq!(m.fields()[1].parent, Some(0));
}

// ---- recovery -----------------------------------------------------------------------------------

#[test]
fn stray_field_characters_are_ignored_and_counted() {
    let m = para(&format!(
        "{}{}{}{}",
        end(),
        separate(),
        text("a"),
        instr(" lonely ")
    ));
    assert_eq!(m.text(FINAL), "a");
    let n = m
        .anomalies()
        .iter()
        .filter(|a| a.kind == AnomalyKind::StrayFieldChar)
        .count();
    assert_eq!(n, 3);
    let twice = para(&format!(
        "{}{}{}{}{}{}",
        begin(),
        instr(" X "),
        separate(),
        separate(),
        text("r"),
        end()
    ));
    assert_eq!(twice.text(FINAL), "r");
    assert_eq!(
        twice
            .anomalies()
            .iter()
            .filter(|a| a.kind == AnomalyKind::StrayFieldChar)
            .count(),
        1
    );
}

#[test]
fn field_code_that_is_never_closed_ends_with_its_paragraph() {
    let ms = maps(&format!(
        "<w:p>{}{}{}</w:p><w:p>{}</w:p>",
        text("a"),
        begin(),
        instr(" REF unclosed ") + &text("hidden in the code"),
        text("b")
    ));
    assert_eq!(ms[0].text(FINAL), "a");
    assert_eq!(
        ms[0]
            .anomalies()
            .iter()
            .filter(|a| a.kind == AnomalyKind::UnterminatedField)
            .count(),
        1
    );
    assert_eq!(
        ms[1].text(FINAL),
        "b",
        "code cannot cross a paragraph, so the next one is not code"
    );
    assert_eq!(ms[1].items()[0].flags & FIELD_RESULT, 0);
}

#[test]
fn nesting_deeper_than_thirty_two_is_ignored_and_the_ends_still_balance() {
    let mut s = String::new();
    for _ in 0..40 {
        s.push_str(&(begin().to_string() + &instr(" X ") + &separate()));
    }
    s.push_str(&text("core"));
    for _ in 0..40 {
        s.push_str(&end());
    }
    s.push_str(&text("after"));
    let m = para(&s);
    assert_eq!(m.text(FINAL), "coreafter");
    assert_eq!(
        m.anomalies()
            .iter()
            .filter(|a| a.kind == AnomalyKind::FieldTooDeep)
            .count(),
        8
    );
    assert_eq!(m.fields().len(), 32);
    let after = m
        .items()
        .iter()
        .rev()
        .find(|i| i.kind == ItemKind::Text)
        .unwrap();
    assert_eq!(after.flags & FIELD_RESULT, 0);
}

// ---- across paragraphs --------------------------------------------------------------------------

#[test]
fn a_field_crosses_paragraphs_and_the_story_knows_where_it_starts_and_ends() {
    let ms = maps(&format!(
        "<w:p>{}{}{}{}</w:p><w:p>{}</w:p><w:p>{}{}{}{}</w:p>",
        text("a "),
        begin() + &instr(" TOC \\o \"1-3\" ") + &separate(),
        text("first "),
        "",
        text("middle"),
        text("last"),
        end(),
        text(" after "),
        field(" REF q ", "q")
    ));
    assert_eq!(ms[0].text(FINAL), "a first ");
    assert_eq!(ms[1].text(FINAL), "middle");
    assert_eq!(ms[1].items()[0].flags & FIELD_RESULT, FIELD_RESULT);
    assert_eq!(ms[1].items()[0].field, 0);
    assert!(!ms[1].fields()[0].begins_here);
    assert_eq!(ms[2].text(FINAL), "last after q");
    let flags: Vec<u8> = ms[2]
        .items()
        .iter()
        .filter(|i| i.kind == ItemKind::Text)
        .map(|i| i.flags & FIELD_RESULT)
        .collect();
    assert_eq!(
        flags,
        [FIELD_RESULT, 0, FIELD_RESULT],
        "last is the end of the TOC, after is outside, q is a result"
    );
    let q = ms[2]
        .items()
        .iter()
        .rev()
        .find(|i| i.kind == ItemKind::Text)
        .unwrap();
    assert_eq!(
        q.field, 1,
        "ids are story-wide: the second field of the story is 1"
    );
    for m in &ms {
        check_invariants(m);
    }
}

#[test]
fn the_story_lists_every_field_with_its_paragraphs_keyword_and_flags() {
    let tree = super::testing::document(&format!(
        "<w:p>{}</w:p><w:p>{}</w:p><w:p>{}</w:p>",
        begin() + &instr(" HYPERLINK \"http://x\" ") + &separate() + &text("link"),
        text("more") + &end(),
        "<w:fldSimple w:instr=\" date \\@ yyyy \" w:dirty=\"true\" w:fldLock=\"1\"><w:r><w:t>2025</w:t></w:r></w:fldSimple>"
    ));
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let fields = story.fields(&tree);
    assert_eq!(fields.len(), 2);
    let (p0, p1, p2) = (
        story.blocks()[0].node,
        story.blocks()[1].node,
        story.blocks()[2].node,
    );
    assert_eq!(&*fields[0].keyword, "HYPERLINK");
    assert_eq!(
        (fields[0].paragraph_begin, fields[0].paragraph_end),
        (p0, Some(p1))
    );
    assert!(fields[0].end.is_some() && fields[0].separate.is_some());
    assert!(!fields[0].dirty && !fields[0].locked);
    assert_eq!(&*fields[1].keyword, "DATE");
    assert!(fields[1].simple && fields[1].dirty && fields[1].locked);
    assert_eq!(
        (fields[1].paragraph_begin, fields[1].paragraph_end),
        (p2, Some(p2))
    );
    assert!(story.field_anomalies(&tree).is_empty());
    // the map of a paragraph that is in a field starts with it open
    let m = story.paragraph(&tree, 1).unwrap().map();
    assert_eq!(m.items()[0].flags & FIELD_RESULT, FIELD_RESULT);
}

#[test]
fn a_note_starts_afresh_and_a_field_open_at_its_end_is_reported() {
    let tree = part(
        "word/footnotes.xml",
        "footnotes",
        &format!(
            "<w:footnote w:id=\"1\"><w:p>{}</w:p></w:footnote><w:footnote w:id=\"2\"><w:p>{}</w:p></w:footnote>",
            begin() + &instr(" REF a ") + &separate() + &text("open"),
            text("free")
        ),
    );
    let story = Story::scan(
        &Arc::from("word/footnotes.xml"),
        StoryKind::Footnotes,
        &tree,
    )
    .unwrap();
    let free = story.paragraph(&tree, 1).unwrap().map();
    assert_eq!(free.items()[0].flags & FIELD_RESULT, 0);
    assert_eq!(free.items()[0].field, NO_FIELD);
    let a = story.field_anomalies(&tree);
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].kind, AnomalyKind::UnterminatedField);
    assert_eq!(story.fields(&tree)[0].end, None);
}

#[test]
fn a_standalone_paragraph_starts_with_no_open_field() {
    let tree = super::testing::document(&format!(
        "<w:p>{}</w:p><w:p>{}</w:p>",
        begin() + &separate() + &text("a"),
        text("b") + &end()
    ));
    let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
    let second = story.blocks()[1].node;
    let alone = Paragraph::new(&tree, second).unwrap().map();
    assert_eq!(alone.items()[0].flags & FIELD_RESULT, 0);
    assert_eq!(
        alone
            .anomalies()
            .iter()
            .filter(|a| a.kind == AnomalyKind::StrayFieldChar)
            .count(),
        1
    );
    let with_story = story.paragraph(&tree, 1).unwrap().map();
    assert_eq!(with_story.items()[0].flags & FIELD_RESULT, FIELD_RESULT);
}

#[test]
fn modes_apply_to_field_results_like_any_item() {
    let m = para(&format!(
        "{}{}{}<w:ins w:id=\"1\">{}</w:ins>{}",
        begin(),
        instr(" REF a "),
        separate(),
        text("new"),
        end()
    ));
    assert_eq!(texts(&m), ["new", "", "new"]);
    assert_eq!(m.len(ORIGINAL), 0);
    assert_eq!(m.len(REDLINE), 3);
}
