//! `insert` with `place: after` through `Document::edit` (issue 18): the new paragraph goes directly after its anchor, before
//! whatever follows it (a paragraph, a table, a content control, range markers, an empty control), in the body, a cell and a
//! header, tracked and untracked. The anchor keeps its `w:pPr` byte for byte, its section break included, so accepting
//! everything leaves the section boundary where it was, and rejecting everything gives back the part as it was.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::error::ErrorCode;
use crate::refs::Claim;
use stetdoc_testkit::fixtures::parts::{self, DECL, NS_R, NS_W};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

/// A paragraph with this text and this `w:pPr` (none when empty).
fn para(text: &str, ppr: &str) -> String {
    let ppr = if ppr.is_empty() {
        String::new()
    } else {
        format!("<w:pPr>{ppr}</w:pPr>")
    };
    format!(r#"<w:p>{ppr}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

const TABLE: &str = r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Cell.</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
const CONTROL: &str = r#"<w:sdt><w:sdtPr><w:id w:val="9"/></w:sdtPr><w:sdtContent><w:p><w:r><w:t>Inside.</w:t></w:r></w:p></w:sdtContent></w:sdt>"#;
const EMPTY_CONTROL: &str = r#"<w:sdt><w:sdtPr><w:id w:val="8"/></w:sdtPr><w:sdtContent/></w:sdt>"#;
const EMPTY_CUSTOM: &str = r#"<w:customXml w:element="x"/>"#;
/// The properties of the anchor that ends a section.
const BREAK: &str = r#"<w:jc w:val="left"/><w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr>"#;

/// A package whose body is `body`, with a header part `header` (a `w:hdr`'s content) when it is not empty, and an
/// imported HTML part for `w:altChunk r:id="rIdA"`.
fn package(body: &str, header: &str) -> Vec<u8> {
    let doc = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:r="{NS_R}""#))
        .body(body)
        .bytes();
    let types = parts::content_types().replace(
        "</Types>",
        "<Default Extension=\"html\" ContentType=\"text/html\"/><Override PartName=\"/word/header1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml\"/></Types>",
    );
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdH\" Type=\"{NS_R}/header\" Target=\"header1.xml\"/><Relationship Id=\"rIdA\" Type=\"{NS_R}/aFChunk\" Target=\"chunk.html\"/></Relationships>"
    );
    let header = format!("{DECL}<w:hdr xmlns:w=\"{NS_W}\">{header}</w:hdr>");
    let mut entries = parts::entries_with(&doc, &parts::root_rels(), &types);
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new("word/header1.xml", header));
    entries.push(Entry::new(
        "word/chunk.html",
        "<html><body><p>Imported.</p></body></html>",
    ));
    Zip::new(entries).build()
}

fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &OpenOptions::default()).unwrap()
}

fn part_xml(doc: &Document, part: &str) -> String {
    let view = doc.state.parts();
    let id = view.id(part).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

/// The ref of the paragraph whose `Final` text is `text`.
fn token_of(doc: &Document, text: &str) -> String {
    (1..64)
        .map(|n| format!("p{n}"))
        .find(|t| doc.state.resolve(Claim::Implied, t).is_ok() && final_text(doc, t) == text)
        .unwrap_or_else(|| panic!("no paragraph says {text}"))
}

fn edit(doc: &mut Document, ops: &str, tracked: bool) -> Result<(), crate::api::error::StetError> {
    let b = batch(doc, ops);
    let e = Edit::ops(b, stamp());
    doc.edit(if tracked { e } else { e.untracked() })
        .map(|_| ())
}

/// What follows the anchor's element in `xml`, which must hold it exactly as `anchor` says.
fn after_anchor<'x>(xml: &'x str, anchor: &str, label: &str) -> &'x str {
    let at = xml
        .find(anchor)
        .unwrap_or_else(|| panic!("{label}: the anchor changed or is gone:\n{anchor}\nin\n{xml}"));
    &xml[at + anchor.len()..]
}

/// What follows the paragraph that says `text` in `xml`, whatever its properties are.
fn after_text<'x>(xml: &'x str, text: &str, label: &str) -> &'x str {
    let at = xml
        .find(&format!(">{text}</w:t>"))
        .unwrap_or_else(|| panic!("{label}: no paragraph says {text}:\n{xml}"));
    let end = at + xml[at..].find("</w:p>").unwrap() + "</w:p>".len();
    &xml[end..]
}

/// Checks that `rest`, what follows the anchor, starts with the new paragraph, which says "New." and holds no section break,
/// and then `next`.
fn new_then(rest: &str, next: &str, xml: &str, label: &str) {
    assert!(
        rest.starts_with("<w:p"),
        "{label}: no paragraph right after the anchor:\n{xml}"
    );
    let end = rest.find("</w:p>").unwrap() + "</w:p>".len();
    let new = &rest[..end];
    assert!(
        new.contains("New."),
        "{label}: the paragraph after the anchor is not the new one:\n{xml}"
    );
    assert!(
        !new.contains("sectPr"),
        "{label}: the new paragraph took a section break:\n{xml}"
    );
    assert!(
        rest[end..].starts_with(next),
        "{label}: the new paragraph is not right before what followed the anchor ({next}):\n{xml}"
    );
}

/// Checks that the new paragraph stands directly after the anchor, which is exactly `anchor`, and before `next`.
fn new_right_after(xml: &str, anchor: &str, next: &str, label: &str) {
    new_then(after_anchor(xml, anchor, label), next, xml, label);
}

/// What one row checks after the insertion is resolved: the anchor after accept all, and after reject all, as the source
/// has it unless one of the limits of Y (design `revisions.md`, 12, decision 2) applies.
struct Resolved<'a> {
    accepted: &'a str,
    rejected: &'a str,
}

/// Inserts "New." after the paragraph that says `anchor_text`, tracked and untracked, and checks the place, the anchor,
/// accept all and reject all. `anchor` is the anchor's element as the source has it, `next` what follows it there.
/// Untracked the anchor keeps its bytes. Tracked it may carry our inserted mark (Y), and after accept all it is
/// `resolved.accepted`, after reject all the part is the source with the anchor `resolved.rejected`.
#[expect(clippy::too_many_arguments, reason = "one row of the issue's tables")]
fn inserts_right_after_then(
    label: &str,
    part: &str,
    body: &str,
    header: &str,
    anchor_text: &str,
    anchor: &str,
    next: &str,
    resolved: &Resolved<'_>,
) {
    let bytes = package(body, header);
    let source = part_xml(&open(&bytes), part);
    assert!(
        source.contains(&format!("{anchor}{next}")),
        "{label}: the fixture is not as described:\n{source}"
    );
    for tracked in [true, false] {
        let label = format!("{label}, {}", if tracked { "tracked" } else { "untracked" });
        let inserted = || {
            let mut doc = open(&bytes);
            let token = token_of(&doc, anchor_text);
            let op = format!(
                r#"{{"op":"insert","ref":"{token}","place":"after","paragraphs":["New."]}}"#
            );
            edit(&mut doc, &op, tracked).unwrap_or_else(|e| panic!("{label}: {e:?}"));
            doc
        };
        let mut doc = inserted();
        let xml = part_xml(&doc, part);
        if !tracked {
            new_right_after(&xml, anchor, next, &label);
            continue;
        }
        new_then(after_text(&xml, anchor_text, &label), next, &xml, &label);
        let mut accepted = inserted();
        run(&mut accepted, r#"{"op":"accept","all":true}"#);
        new_right_after(
            &part_xml(&accepted, part),
            resolved.accepted,
            next,
            &format!("{label}, accepted"),
        );
        run(&mut doc, r#"{"op":"reject","all":true}"#);
        assert_eq!(
            part_xml(&doc, part),
            source.replace(anchor, resolved.rejected),
            "{label}: reject all does not give back the source"
        );
    }
}

/// `inserts_right_after_then` for a row where accept all keeps the anchor byte for byte and reject all gives back the source.
fn inserts_right_after(
    label: &str,
    part: &str,
    body: &str,
    header: &str,
    anchor_text: &str,
    anchor: &str,
    next: &str,
) {
    let resolved = Resolved {
        accepted: anchor,
        rejected: anchor,
    };
    inserts_right_after_then(
        label,
        part,
        body,
        header,
        anchor_text,
        anchor,
        next,
        &resolved,
    );
}

#[test]
fn after_a_paragraph_that_ends_a_section_the_break_stays_on_it() {
    // Issue 18, case 1: every row but the last block of the body, which is refused below.
    let anchor = para("Section.", BREAK);
    let rows = [
        ("a paragraph", para("Next.", r#"<w:jc w:val="center"/>"#)),
        ("a table", TABLE.to_owned()),
        ("a content control holding a paragraph", CONTROL.to_owned()),
        (
            "an empty content control, then a paragraph",
            format!("{EMPTY_CONTROL}{}", para("After.", "")),
        ),
        (
            "an empty content control, then a table",
            format!("{EMPTY_CONTROL}{TABLE}"),
        ),
        (
            "an empty customXml, then a paragraph",
            format!("{EMPTY_CUSTOM}{}", para("After.", "")),
        ),
    ];
    for (label, following) in rows {
        let body = format!("{anchor}{following}{}", para("Tail.", ""));
        inserts_right_after(
            label,
            "word/document.xml",
            &body,
            "",
            "Section.",
            &anchor,
            &following,
        );
    }
}

#[test]
fn after_the_last_paragraph_of_the_body_that_ends_a_section_a_tracked_insert_is_refused_and_an_untracked_one_keeps_the_break()
 {
    // Issue 18, case 1, "nothing": Y is the only tracked shape that rejects to the source here, and it would move the break.
    let anchor = para("Section.", BREAK);
    let bytes = package(&anchor, "");
    let op = r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#;
    let mut doc = open(&bytes);
    let e = edit(&mut doc, op, true).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
    assert!(e.message().contains("section"), "{}", e.message());
    let mut doc = open(&bytes);
    edit(&mut doc, op, false).unwrap();
    new_right_after(
        &part_xml(&doc, "word/document.xml"),
        &anchor,
        "<w:sectPr/>",
        "untracked, last block",
    );
}

/// The markup of case 2 that stands between the anchor and the next block, with what the anchor holds to open a range that
/// ends there, and what the tail closes.
fn markups() -> Vec<(&'static str, &'static str, String, &'static str)> {
    vec![
        ("a point bookmark", "", r#"<w:bookmarkStart w:id="1" w:name="Mark"/><w:bookmarkEnd w:id="1"/>"#.to_owned(), ""),
        ("a permission start", "", r#"<w:permStart w:id="2" w:edGrp="everyone"/>"#.to_owned(), r#"<w:permEnd w:id="2"/>"#),
        ("a proofing mark", "", r#"<w:proofErr w:type="spellStart"/>"#.to_owned(), ""),
        ("an empty content control", "", EMPTY_CONTROL.to_owned(), ""),
        ("an empty customXml", "", EMPTY_CUSTOM.to_owned(), ""),
        ("a bookmark that ends there", r#"<w:bookmarkStart w:id="4" w:name="Range"/>"#, r#"<w:bookmarkEnd w:id="4"/>"#.to_owned(), ""),
        ("a permission range that ends there", r#"<w:permStart w:id="5" w:edGrp="everyone"/>"#, r#"<w:permEnd w:id="5"/>"#.to_owned(), ""),
        ("a comment range", r#"<w:commentRangeStart w:id="6"/>"#, r#"<w:commentRangeEnd w:id="6"/>"#.to_owned(), ""),
        (
            "several markers",
            "",
            r#"<w:bookmarkStart w:id="1" w:name="Mark"/><w:bookmarkEnd w:id="1"/><w:proofErr w:type="spellStart"/><w:permStart w:id="2" w:edGrp="everyone"/>"#.to_owned(),
            r#"<w:permEnd w:id="2"/>"#,
        ),
        ("an imported part", "", r#"<w:altChunk r:id="rIdA"/>"#.to_owned(), ""),
    ]
}

/// The anchor of case 2, holding `inner` after its text.
fn plain(inner: &str) -> String {
    format!(r#"<w:p><w:r><w:t xml:space="preserve">Plain.</w:t></w:r>{inner}</w:p>"#)
}

#[test]
fn with_markup_between_the_anchor_and_the_next_block_the_new_paragraph_goes_before_the_markup() {
    // Issue 18, case 2, in the body: a table next and a paragraph next.
    for (label, inner, between, close) in markups() {
        let anchor = plain(inner);
        for (neighbour, block) in [
            ("table", TABLE.to_owned()),
            ("paragraph", para("Next.", "")),
        ] {
            let tail = format!(r#"<w:p><w:r><w:t>Tail.</w:t></w:r>{close}</w:p>"#);
            let body = format!("{anchor}{between}{block}{tail}");
            let label = format!("{label}, then a {neighbour}");
            inserts_right_after(
                &label,
                "word/document.xml",
                &body,
                "",
                "Plain.",
                &anchor,
                &between,
            );
        }
    }
}

#[test]
fn with_markup_after_the_anchor_in_a_cell_or_a_header_the_new_paragraph_goes_before_the_markup() {
    let between = r#"<w:bookmarkStart w:id="1" w:name="Mark"/><w:bookmarkEnd w:id="1"/>"#;
    let anchor = plain("");
    // In a cell, before a nested table.
    let cell = format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="4800"/></w:tblGrid><w:tr><w:tc>{anchor}{between}{TABLE}{}</w:tc></w:tr></w:tbl>{}"#,
        para("Cell tail.", ""),
        para("Body.", "")
    );
    inserts_right_after(
        "a cell",
        "word/document.xml",
        &cell,
        "",
        "Plain.",
        &anchor,
        between,
    );
    // In a header, before a table, with the body's section naming the header.
    let header = format!("{anchor}{between}{TABLE}{}", para("Header tail.", ""));
    let body = format!(
        r#"{}<w:p><w:pPr><w:sectPr><w:headerReference w:type="default" r:id="rIdH"/></w:sectPr></w:pPr></w:p>{}"#,
        para("Body.", ""),
        para("Last.", "")
    );
    inserts_right_after(
        "a header",
        "word/header1.xml",
        &body,
        &header,
        "Plain.",
        &anchor,
        between,
    );
}

#[test]
fn the_anchors_properties_stay_as_they_were_within_the_limits_of_y() {
    // Issue 18, case 3. A paragraph follows, so the shape is Y, whose copy on the anchor is faithful: the anchor keeps its
    // children, empty ones included, with our inserted mark. Its two limits (design `revisions.md`, 12, decision 2): a copy
    // is put in schema order, and resolving the mark cannot tell an empty container from one the mark emptied. Untracked,
    // the anchor keeps its bytes.
    let mark = r#"<w:ins w:id="0" w:author="Clauseo" w:date="2026-10-06T10:00:00Z"/>"#;
    let bare = "<w:p><w:r><w:t>Anchor.</w:t></w:r></w:p>";
    let ordered = r#"<w:p><w:pPr><w:spacing w:after="240"/><w:jc w:val="center"/></w:pPr><w:r><w:t>Anchor.</w:t></w:r></w:p>"#;
    let rows = [
        (
            "an empty pPr",
            "<w:pPr/>".to_owned(),
            format!("<w:pPr><w:rPr>{mark}</w:rPr></w:pPr>"),
            bare,
            bare,
        ),
        (
            "an empty mark",
            "<w:pPr><w:rPr/></w:pPr>".to_owned(),
            format!("<w:pPr><w:rPr>{mark}</w:rPr></w:pPr>"),
            bare,
            bare,
        ),
        (
            "out of order",
            r#"<w:pPr><w:jc w:val="center"/><w:spacing w:after="240"/></w:pPr>"#.to_owned(),
            format!(
                r#"<w:pPr><w:spacing w:after="240"/><w:jc w:val="center"/><w:rPr>{mark}</w:rPr></w:pPr>"#
            ),
            ordered,
            "",
        ),
        (
            "a mark with a language",
            r#"<w:pPr><w:jc w:val="center"/><w:rPr><w:lang w:val="fr"/></w:rPr></w:pPr>"#
                .to_owned(),
            format!(
                r#"<w:pPr><w:jc w:val="center"/><w:rPr>{mark}<w:lang w:val="fr"/></w:rPr></w:pPr>"#
            ),
            "",
            "",
        ),
    ];
    for (label, ppr, tracked_ppr, accepted, rejected) in &rows {
        let anchor = format!("<w:p>{ppr}<w:r><w:t>Anchor.</w:t></w:r></w:p>");
        let next = para("End.", "");
        let body = format!("{anchor}{next}");
        let pick = |s: &'static str| if s.is_empty() { anchor.as_str() } else { s };
        let resolved = Resolved {
            accepted: pick(accepted),
            rejected: pick(rejected),
        };
        inserts_right_after_then(
            label,
            "word/document.xml",
            &body,
            "",
            "Anchor.",
            &anchor,
            &next,
            &resolved,
        );
        // The tracked edit: the anchor's copy, with the new paragraph after it holding the original properties.
        let mut doc = open(&package(&body, ""));
        edit(
            &mut doc,
            r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#,
            true,
        )
        .unwrap();
        let xml = part_xml(&doc, "word/document.xml");
        let copy = format!("<w:p>{tracked_ppr}<w:r><w:t>Anchor.</w:t></w:r></w:p><w:p>{ppr}");
        assert!(
            xml.contains(&copy),
            "{label}: the anchor's copy is not faithful:\n{xml}"
        );
    }
}

#[test]
fn after_the_last_paragraph_of_a_container_the_anchor_keeps_its_properties_through_accept_and_reject()
 {
    // At a container's end the shape is Y, which Reject All needs: the anchor gets the inserted mark and a faithful copy of
    // its properties (the original moves to the new paragraph, which then ends the container). Accepting leaves the anchor's
    // properties as they were, and rejecting gives back the part.
    // Each with the anchor's properties in the tracked edit: the same children in the same order, and our mark.
    let mark = r#"<w:ins w:id="0" w:author="Clauseo" w:date="2026-10-06T10:00:00Z"/>"#;
    let pprs = [
        (
            r#"<w:pPr><w:spacing w:after="240"/><w:jc w:val="center"/></w:pPr>"#,
            format!(
                r#"<w:pPr><w:spacing w:after="240"/><w:jc w:val="center"/><w:rPr>{mark}</w:rPr></w:pPr>"#
            ),
        ),
        (
            r#"<w:pPr><w:jc w:val="center"/><w:rPr><w:lang w:val="fr"/></w:rPr></w:pPr>"#,
            format!(
                r#"<w:pPr><w:jc w:val="center"/><w:rPr>{mark}<w:lang w:val="fr"/></w:rPr></w:pPr>"#
            ),
        ),
    ];
    for (ppr, copy) in &pprs {
        let anchor = format!("<w:p>{ppr}<w:r><w:t>Anchor.</w:t></w:r></w:p>");
        let in_body = format!("{}{anchor}", para("Lead.", ""));
        let in_cell = format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc>{in_body}</w:tc></w:tr></w:tbl>{}"#,
            para("Body.", "")
        );
        for (label, body, next) in [
            ("the body", &in_body, "<w:sectPr/>"),
            ("a cell", &in_cell, "</w:tc>"),
        ] {
            let label = format!("{label}, {ppr}");
            let bytes = package(body, "");
            let source = part_xml(&open(&bytes), "word/document.xml");
            let op = r#"{"op":"insert","ref":"p2","place":"after","paragraphs":["New."]}"#;
            let mut doc = open(&bytes);
            edit(&mut doc, op, true).unwrap();
            let tracked = part_xml(&doc, "word/document.xml");
            let copy = format!("<w:p>{copy}<w:r><w:t>Anchor.</w:t></w:r></w:p>");
            assert!(
                tracked.contains(&copy),
                "{label}: the anchor's copy is not faithful:\n{tracked}"
            );
            let mut accepted = open(&bytes);
            edit(&mut accepted, op, true).unwrap();
            run(&mut accepted, r#"{"op":"accept","all":true}"#);
            new_right_after(
                &part_xml(&accepted, "word/document.xml"),
                &anchor,
                next,
                &format!("{label}, accepted"),
            );
            run(&mut doc, r#"{"op":"reject","all":true}"#);
            assert_eq!(
                part_xml(&doc, "word/document.xml"),
                source,
                "{label}: reject all"
            );
        }
    }
}

#[test]
fn an_untracked_split_gives_the_first_half_a_faithful_copy_of_the_properties() {
    // The first half of a Y keeps the children the original had, empty ones included. Tracked, our mark fills the mark's
    // w:rPr anyway; untracked, an empty w:pPr or w:rPr is kept as it was rather than dropped.
    for ppr in ["<w:pPr/>", "<w:pPr><w:rPr/></w:pPr>"] {
        let body = format!("<w:p>{ppr}<w:r><w:t>OneTwo</w:t></w:r></w:p>");
        let mut doc = open(&package(&body, ""));
        edit(&mut doc, r#"{"op":"split","ref":"p1","at":3}"#, false).unwrap();
        let xml = part_xml(&doc, "word/document.xml");
        let halves = format!("<w:p>{ppr}<w:r><w:t>One</w:t></w:r></w:p><w:p>{ppr}");
        assert!(xml.contains(&halves), "{ppr}:\n{xml}");
    }
}

/// The body of a tracked insert of "New." after `p1`, and the body after accepting everything, both with the stamp's
/// attributes left out.
fn tracked_then_accepted(body: &str) -> (String, String) {
    let strip = |xml: String| {
        let at = xml.find("<w:body>").unwrap();
        xml[at..].replace(r#" w:author="Clauseo" w:date="2026-10-06T10:00:00Z""#, "")
    };
    let mut doc = open(&package(body, ""));
    edit(
        &mut doc,
        r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#,
        true,
    )
    .unwrap();
    let tracked = strip(part_xml(&doc, "word/document.xml"));
    run(&mut doc, r#"{"op":"accept","all":true}"#);
    (tracked, strip(part_xml(&doc, "word/document.xml")))
}

#[test]
fn after_a_paragraph_that_ends_a_section_the_new_one_copies_the_next_and_needs_no_change_of_properties()
 {
    // The new paragraph starts the next section, so it takes the next paragraph's properties, as its twin, and no
    // `pPrChange` (no Formatted balloon in Word).
    let anchor = para("Section.", BREAK);
    let body = format!("{anchor}{}", para("Next.", r#"<w:jc w:val="center"/>"#));
    let (tracked, accepted) = tracked_then_accepted(&body);
    let new = r#"<w:p><w:pPr><w:jc w:val="center"/><w:rPr><w:ins w:id="0"/></w:rPr></w:pPr><w:ins w:id="1"><w:r><w:t>New.</w:t></w:r></w:ins></w:p>"#;
    assert!(tracked.contains(&format!("{anchor}{new}")), "{tracked}");
    assert!(!tracked.contains("pPrChange"), "{tracked}");
    let new = r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>New.</w:t></w:r></w:p>"#;
    assert!(accepted.contains(&format!("{anchor}{new}")), "{accepted}");
}

#[test]
fn after_a_paragraph_whose_mark_another_author_inserted_the_new_one_copies_the_next() {
    // Y cannot put a second insertion on the mark, so the new paragraph is an X with the next paragraph's properties, as in
    // 0.9.3: no `pPrChange`, and centred after accepting everything.
    let other = r#"<w:ins w:id="5" w:author="Other" w:date="2020-01-01T00:00:00Z"/>"#;
    let anchor = format!(
        r#"<w:p><w:pPr><w:jc w:val="left"/><w:rPr>{other}</w:rPr></w:pPr><w:r><w:t>Anchor.</w:t></w:r></w:p>"#
    );
    let body = format!("{anchor}{}", para("Next.", r#"<w:jc w:val="center"/>"#));
    let (tracked, accepted) = tracked_then_accepted(&body);
    assert!(tracked.contains(&anchor), "{tracked}");
    assert!(!tracked.contains("pPrChange"), "{tracked}");
    let new = r#"<w:p><w:pPr><w:jc w:val="center"/><w:rPr><w:ins w:id="6"/></w:rPr></w:pPr><w:ins w:id="7"><w:r><w:t>New.</w:t></w:r></w:ins></w:p>"#;
    assert!(tracked.contains(&format!("{anchor}{new}")), "{tracked}");
    let new = r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>New.</w:t></w:r></w:p>"#;
    assert!(accepted.contains(new), "{accepted}");
}

#[test]
fn after_a_paragraph_whose_mark_another_author_deleted_the_new_one_copies_the_next() {
    // Accepting the deleted mark joins the anchor into the new paragraph, whose properties are the next paragraph's, so the
    // joined paragraph is centred, as in 0.9.3.
    let other = r#"<w:del w:id="5" w:author="Other" w:date="2020-01-01T00:00:00Z"/>"#;
    let anchor = format!(
        r#"<w:p><w:pPr><w:jc w:val="left"/><w:rPr>{other}</w:rPr></w:pPr><w:r><w:t>Anchor.</w:t></w:r></w:p>"#
    );
    let body = format!("{anchor}{}", para("Next.", r#"<w:jc w:val="center"/>"#));
    let (tracked, accepted) = tracked_then_accepted(&body);
    assert!(tracked.contains(&anchor), "{tracked}");
    assert!(!tracked.contains("pPrChange"), "{tracked}");
    let joined = r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Anchor.</w:t></w:r><w:r><w:t>New.</w:t></w:r></w:p>"#;
    assert!(accepted.contains(joined), "{accepted}");
}
