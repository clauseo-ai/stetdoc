//! `Document::inspect`: the effective formatting of a block with its sources, and the listings.

use super::tests_edit::run;
use super::*;
use crate::index::testing::Fixture;

/// A document with styles, a two-level list and a section: a heading, two numbered paragraphs with runs formatted three ways,
/// and a table.
fn styled() -> Document {
    let body = concat!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Definitions</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr><w:spacing w:after=\"120\"/></w:pPr>",
        "<w:r><w:t xml:space=\"preserve\">The </w:t></w:r>",
        "<w:r><w:rPr><w:b/></w:rPr><w:t>Supplier</w:t></w:r>",
        "<w:r><w:rPr><w:i/><w:color w:val=\"FF0000\"/></w:rPr><w:t xml:space=\"preserve\"> shall pay.</w:t></w:r></w:p>",
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"1\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr><w:r><w:t>on demand</w:t></w:r></w:p>",
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"Grid\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid>",
        "<w:tr><w:tc><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    );
    let styles = concat!(
        "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/>",
        "<w:next w:val=\"Normal\"/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"60\"/><w:jc w:val=\"center\"/></w:pPr>",
        "<w:rPr><w:b/><w:sz w:val=\"32\"/></w:rPr></w:style>",
        "<w:style w:type=\"table\" w:styleId=\"Grid\"><w:name w:val=\"Table Grid\"/></w:style>",
    );
    let sect = "<w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1800\" w:bottom=\"1440\" w:left=\"1800\"/><w:cols w:num=\"2\"/>";
    let state = Fixture::new(body)
        .styles(styles)
        .numbering()
        .sect(sect)
        .state();
    let bytes = state.to_package().unwrap().write().unwrap();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

fn line<'a>(text: &'a str, start: &str) -> &'a str {
    text.lines()
        .find(|l| l.starts_with(start))
        .unwrap_or_else(|| panic!("no line starts with {start:?} in\n{text}"))
}

#[test]
fn a_heading_shows_its_style_and_where_each_property_comes_from() {
    let doc = styled();
    let i = doc.inspect("p1").unwrap();
    assert_eq!(
        (i.base.as_str(), i.target.as_str()),
        (doc.base().as_str(), "p1")
    );
    assert_eq!(
        line(&i.text, "block:"),
        "block: p1 is a paragraph in the body story"
    );
    assert_eq!(line(&i.text, "style:"), "style: heading 1 (Heading1)");
    assert_eq!(
        line(&i.text, "jc:"),
        "jc: val=center (paragraph style Heading1)"
    );
    assert_eq!(
        line(&i.text, "spacing:"),
        "spacing: before=240 after=60 (paragraph style Heading1)"
    );
    assert_eq!(
        line(&i.text, "keepNext:"),
        "keepNext: on (paragraph style Heading1)"
    );
    // The style and the list have their own lines, and not one as a property.
    assert!(
        !i.text.contains("pStyle:") && !i.text.contains("numPr:"),
        "{}",
        i.text
    );
    // The run formatting comes from the style too, and covers the characters of the text.
    assert_eq!(
        line(&i.text, "characters 0..11:"),
        "characters 0..11: sz val=32, b on"
    );
    // The same lines as data.
    let jc = i.items.iter().find(|x| x.label == "jc").unwrap();
    assert_eq!(
        (jc.value.as_str(), jc.source.as_deref()),
        ("val=center", Some("paragraph style Heading1"))
    );
}

#[test]
fn a_numbered_paragraph_shows_its_number_its_clause_and_each_run_formatting() {
    let doc = styled();
    let i = doc.inspect("p2").unwrap();
    assert_eq!(
        line(&i.text, "style:"),
        "style: Normal (the default paragraph style)"
    );
    assert_eq!(
        line(&i.text, "list:"),
        "list: numId 1, level 0, number 1. (direct)"
    );
    assert_eq!(line(&i.text, "clause:"), "clause: 1 (as shown)");
    // Direct formatting says so, and a style's value is overridden by it where both speak.
    assert_eq!(line(&i.text, "spacing:"), "spacing: after=120 (direct)");
    // The distinct run formattings, with the characters of the Final text each covers.
    let runs: Vec<&str> = i
        .text
        .lines()
        .filter(|l| l.starts_with("characters"))
        .collect();
    assert_eq!(
        runs,
        [
            "characters 0..4: plain",
            "characters 4..12: b on",
            "characters 12..23: i on, color val=FF0000",
        ]
    );
    let deeper = doc.inspect("p3").unwrap();
    assert_eq!(
        line(&deeper.text, "list:"),
        "list: numId 1, level 1, number (a) (direct)"
    );
    assert_eq!(line(&deeper.text, "clause:"), "clause: 1(a) (as shown)");
}

#[test]
fn run_ranges_count_the_final_text_after_an_edit() {
    // A deleted word is not in the Final text, so the ranges after it move up.
    let mut doc = styled();
    run(&mut doc, r#"{"op":"delete","ref":"p2","find":"The "}"#);
    let i = doc.inspect("p2").unwrap();
    let runs: Vec<&str> = i
        .text
        .lines()
        .filter(|l| l.starts_with("characters"))
        .collect();
    assert_eq!(
        runs,
        [
            "characters 0..8: b on",
            "characters 8..19: i on, color val=FF0000"
        ]
    );
}

#[test]
fn a_table_and_a_row_show_their_style_and_what_they_hold() {
    let doc = styled();
    let table = doc.inspect("t1").unwrap();
    assert_eq!(
        line(&table.text, "block:"),
        "block: t1 is a table in the body story"
    );
    assert_eq!(line(&table.text, "style:"), "style: Table Grid (Grid)");
    assert_eq!(line(&table.text, "rows:"), "rows: 1");
    let row = doc.inspect("r1").unwrap();
    assert_eq!(
        line(&row.text, "block:"),
        "block: r1 is a row in the body story"
    );
    assert_eq!(
        line(&row.text, "blocks in its cells:"),
        "blocks in its cells: 1"
    );
}

#[test]
fn the_listings_name_each_style_list_and_section() {
    let doc = styled();
    let styles = doc.inspect("styles").unwrap();
    assert_eq!(
        line(&styles.text, "style: Normal"),
        "style: Normal: paragraph, the default, 3 paragraphs"
    );
    assert_eq!(
        line(&styles.text, "style: heading 1"),
        "style: heading 1 (Heading1): paragraph, based on Normal, next Normal, 1 paragraph"
    );
    assert_eq!(
        line(&styles.text, "style: Table Grid"),
        "style: Table Grid (Grid): table"
    );
    let lists = doc.inspect("numbering").unwrap();
    assert_eq!(
        line(&lists.text, "list: numId 1"),
        "list: numId 1, abstract list 0, levels: 0 decimal \"%1.\" from 1; 1 lowerletter \"(%2)\" from 1"
    );
    assert!(
        line(&lists.text, "list: numId 2").contains("bullet"),
        "{}",
        lists.text
    );
    let sections = doc.inspect("sections").unwrap();
    assert_eq!(
        line(&sections.text, "section 1:"),
        "section 1: the last section; starts on the next page; page 12240 by 15840 twips; margins top 1440 right 1800 bottom 1440 left 1800; 2 columns"
    );
}

#[test]
fn a_target_that_is_nothing_to_inspect_is_refused_with_what_to_name() {
    let doc = styled();
    let e = doc.inspect("banana").err().unwrap();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.next().contains("`styles`"), "{}", e.next());
    assert_eq!(
        doc.inspect("p999").err().unwrap().code(),
        ErrorCode::NotFound
    );
    // An unedited minimal document has no styles part and says nothing of styles, and still answers.
    let plain = crate::document::tests::opened();
    assert!(plain.inspect("styles").unwrap().items.is_empty());
    assert!(
        plain
            .inspect("p1")
            .unwrap()
            .text
            .contains("p1 is a paragraph")
    );
}
