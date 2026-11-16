//! Deleting a paragraph that its container needs, and one it does not (issue #25), through `Document::edit`, tracked and
//! untracked. A paragraph the container does not need goes: tracked, its mark is deleted and accepting removes it; untracked,
//! it is removed at once. One the container needs is emptied and kept, and the receipt says so with a
//! `kept_required_paragraph` warning naming the ref and the reason, and an operation line that says it was emptied. Rejecting
//! everything gives the source back, and `check` stays clean.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::receipt::Receipt;
use crate::api::view::WarningCode;
use stetdoc_testkit::fixtures::parts::{self, DECL, NS_R};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

fn para(text: &str) -> String {
    format!(r#"<w:p><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

fn table(cell: &str) -> String {
    format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc>{cell}</w:tc></w:tr></w:tbl>"#
    )
}

/// The paragraph that ends a landscape section.
fn section_end(text: &str) -> String {
    format!(
        r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/></w:sectPr></w:pPr><w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#
    )
}

/// A package whose body is `body`, with an imported HTML part for `w:altChunk r:id="rIdA"`.
fn open(body: &str) -> Document {
    let xml = parts::Doc::new()
        .root_attrs(&format!(r#" xmlns:r="{NS_R}""#))
        .body(body)
        .bytes();
    let types = parts::content_types().replace(
        "</Types>",
        "<Default Extension=\"html\" ContentType=\"text/html\"/></Types>",
    );
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rIdA\" Type=\"{NS_R}/aFChunk\" Target=\"chunk.html\"/></Relationships>"
    );
    let mut entries = parts::entries_with(&xml, &parts::root_rels(), &types);
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new(
        "word/chunk.html",
        "<html><body><p>Imported.</p></body></html>",
    ));
    Document::open(Zip::new(entries).build(), &OpenOptions::default()).unwrap()
}

/// The body's content, without its last `w:sectPr`.
fn body(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let xml = String::from_utf8(view.tree(id).unwrap().serialize()).unwrap();
    let start = xml.find("<w:body>").unwrap() + "<w:body>".len();
    let end = xml.rfind("<w:sectPr/></w:body>").unwrap();
    xml[start..end].to_string()
}

fn edit(doc: &mut Document, ops: &str, tracked: bool) -> Receipt {
    let e = Edit::ops(batch(doc, ops), stamp());
    let receipt = doc
        .edit(if tracked { e } else { e.untracked() })
        .unwrap_or_else(|err| panic!("{ops}: {err:?}"));
    assert!(doc.check().ok, "{ops}: {:?}", doc.check());
    receipt
}

fn kept(receipt: &Receipt) -> Vec<(String, String)> {
    receipt
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::KeptRequiredParagraph)
        .map(|w| (w.block.clone().unwrap_or_default(), w.message.clone()))
        .collect()
}

/// A case: the source body, the batch, what the body is once the batch is applied untracked or accepted, and the warning
/// that names the kept paragraph, if one is kept.
struct Case {
    name: &'static str,
    source: String,
    ops: &'static str,
    result: String,
    warning: Option<(&'static str, &'static str)>,
}

fn cases() -> Vec<Case> {
    let delete = |r: &'static str| -> &'static str {
        match r {
            "p1" => r#"{"op":"delete","ref":"p1"}"#,
            "p2" => r#"{"op":"delete","ref":"p2"}"#,
            "p3" => r#"{"op":"delete","ref":"p3"}"#,
            _ => r#"{"op":"delete","ref":"p4"}"#,
        }
    };
    // The issue's fixture: p1, a table whose cell holds p2, p3 that ends a section, and p4 last.
    let a7 =
        |p2: &str, p3: &str, p4: &str| format!("{}{}{p3}{p4}", para("First paragraph."), table(p2));
    let a7_source = a7(
        &para("Only paragraph of the cell."),
        &section_end("Landscape section ends here."),
        &para("Last paragraph."),
    );
    let empty_section_end = r#"<w:p><w:pPr><w:sectPr><w:pgSz w:w="16838" w:h="11906" w:orient="landscape"/></w:sectPr></w:pPr></w:p>"#;
    vec![
        Case {
            name: "the only paragraph of a cell",
            source: a7_source.clone(),
            ops: delete("p2"),
            result: a7(
                "<w:p></w:p>",
                &section_end("Landscape section ends here."),
                &para("Last paragraph."),
            ),
            warning: Some((
                "p2",
                "p2 is the only paragraph of a cell; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "a paragraph that ends a section",
            source: a7_source.clone(),
            ops: delete("p3"),
            result: a7(
                &para("Only paragraph of the cell."),
                empty_section_end,
                &para("Last paragraph."),
            ),
            warning: Some((
                "p3",
                "p3 holds a section break, which stetdoc keeps so that the section keeps its layout, headers and footers; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the body's last paragraph after a section break",
            source: a7_source,
            ops: delete("p4"),
            result: a7(
                &para("Only paragraph of the cell."),
                &section_end("Landscape section ends here."),
                "<w:p></w:p>",
            ),
            warning: Some((
                "p4",
                "p4 is the body's last paragraph and follows a paragraph that ends a section, which would leave the last section empty without it; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the first block, before a table",
            source: format!("{}{}{}", para("Lead."), table(&para("Cell.")), para("Tail.")),
            ops: delete("p1"),
            result: format!("{}{}", table(&para("Cell.")), para("Tail.")),
            warning: None,
        },
        Case {
            name: "a paragraph between two tables",
            source: format!(
                "{}{}{}{}",
                table(&para("Cell.")),
                para("Between."),
                table(&para("Cell.")),
                para("Tail.")
            ),
            ops: delete("p2"),
            result: format!(
                "{}<w:p></w:p>{}{}",
                table(&para("Cell.")),
                table(&para("Cell.")),
                para("Tail.")
            ),
            warning: Some((
                "p2",
                "p2 stands between two tables, which Word would join into one without it; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the paragraph after the final table",
            source: format!("{}{}", table(&para("Cell.")), para("After.")),
            ops: delete("p2"),
            result: format!("{}<w:p></w:p>", table(&para("Cell."))),
            warning: Some((
                "p2",
                "p2 ends the body right after a table, which cannot end the body; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the body's only paragraph",
            source: para("Only."),
            ops: delete("p1"),
            result: "<w:p></w:p>".to_string(),
            warning: Some((
                "p1",
                "p1 is the only paragraph of the body; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "a cell's last paragraph when another is left",
            source: format!(
                "{}{}",
                table(&format!("{}{}", para("Cell one."), para("Cell two."))),
                para("Tail.")
            ),
            ops: delete("p2"),
            result: format!("{}{}", table(&para("Cell one.")), para("Tail.")),
            warning: None,
        },
        Case {
            name: "the body's last paragraph after another paragraph",
            source: format!("{}{}", para("Lead."), para("Gone.")),
            ops: delete("p2"),
            result: para("Lead."),
            warning: None,
        },
        Case {
            name: "the body's last paragraph with bookmarks, after another paragraph",
            source: format!(
                r#"{}<w:p><w:bookmarkStart w:id="7" w:name="b"/><w:r><w:t xml:space="preserve">Gone.</w:t></w:r><w:bookmarkEnd w:id="7"/></w:p>"#,
                para("Lead.")
            ),
            ops: delete("p2"),
            result: r#"<w:p><w:r><w:t xml:space="preserve">Lead.</w:t></w:r><w:bookmarkStart w:id="7" w:name="b"/><w:bookmarkEnd w:id="7"/></w:p>"#.to_string(),
            warning: None,
        },
        Case {
            name: "the body's last paragraph holding a content control, after another paragraph",
            source: format!(
                r#"{}<w:p><w:sdt><w:sdtPr><w:id w:val="5"/></w:sdtPr><w:sdtContent><w:r><w:t xml:space="preserve">Value</w:t></w:r></w:sdtContent></w:sdt></w:p>"#,
                para("Lead.")
            ),
            ops: delete("p2"),
            result: format!(
                r#"{}<w:p><w:sdt><w:sdtPr><w:id w:val="5"/></w:sdtPr><w:sdtContent></w:sdtContent></w:sdt></w:p>"#,
                para("Lead.")
            ),
            warning: Some((
                "p2",
                "p2 holds content that stays when its text is deleted, such as a content control, and no paragraph follows it to take that content; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "a range from inside the first paragraph through the end of the last",
            source: format!("{}{}", para("Lead."), para("Gone.")),
            ops: r#"{"op":"delete","ref":"p1","to":"p2","start":2}"#,
            result: para("Le"),
            warning: None,
        },
        Case {
            name: "a first paragraph with a bookmark before a table",
            source: format!(
                r#"<w:p><w:bookmarkStart w:id="7" w:name="b"/><w:r><w:t xml:space="preserve">Lead.</w:t></w:r><w:bookmarkEnd w:id="7"/></w:p>{}{}"#,
                table(&para("Cell.")),
                para("Tail.")
            ),
            ops: delete("p1"),
            result: format!(
                r#"<w:p><w:bookmarkStart w:id="7" w:name="b"/><w:bookmarkEnd w:id="7"/></w:p>{}{}"#,
                table(&para("Cell.")),
                para("Tail.")
            ),
            warning: Some((
                "p1",
                "p1 holds bookmarks, comment ranges or other markers, and no paragraph stands beside it to take them; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the body's last paragraph before an imported part that ends the body",
            source: format!(r#"{}{}<w:altChunk r:id="rIdA"/>"#, para("Lead."), para("Gone.")),
            ops: delete("p2"),
            result: format!(r#"{}<w:p></w:p><w:altChunk r:id="rIdA"/>"#, para("Lead.")),
            warning: Some((
                "p2",
                "p2 is the last paragraph of the body, and without it the body would end with an imported document part, which cannot end it; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "a paragraph right before an imported part, with a paragraph after it",
            source: format!(
                r#"{}{}<w:altChunk r:id="rIdA"/>{}"#,
                para("Lead."),
                para("Gone."),
                para("Tail.")
            ),
            ops: delete("p2"),
            result: format!(
                r#"{}<w:p></w:p><w:altChunk r:id="rIdA"/>{}"#,
                para("Lead."),
                para("Tail.")
            ),
            warning: Some((
                "p2",
                "p2 stands right before an imported document part, and a deleted paragraph cannot be joined across it; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the body's last paragraph before an empty content control that ends the body",
            source: format!(
                r#"{}{}<w:sdt><w:sdtPr><w:id w:val="8"/></w:sdtPr><w:sdtContent/></w:sdt>"#,
                para("Lead."),
                para("Gone.")
            ),
            ops: delete("p2"),
            result: format!(
                r#"{}<w:p></w:p><w:sdt><w:sdtPr><w:id w:val="8"/></w:sdtPr><w:sdtContent/></w:sdt>"#,
                para("Lead.")
            ),
            warning: Some((
                "p2",
                "p2 is the last paragraph of the body, and without it the body would end with a content control, which cannot end it; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "the body's last paragraph after a content control that ends with a table",
            source: format!(
                r#"{}<w:sdt><w:sdtPr><w:id w:val="9"/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt>{}"#,
                para("Lead."),
                para("In."),
                table(&para("N.")),
                para("Gone.")
            ),
            ops: delete("p4"),
            result: format!(
                r#"{}<w:sdt><w:sdtPr><w:id w:val="9"/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt><w:p></w:p>"#,
                para("Lead."),
                para("In."),
                table(&para("N."))
            ),
            warning: Some((
                "p4",
                "p4 ends the body right after a content control, which cannot end the body; its text was deleted and the empty paragraph kept",
            )),
        },
        Case {
            name: "a whole final paragraph deleted, then an insertion beside it",
            source: format!("{}{}", para("Lead."), para("Gone.")),
            ops: r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#,
            result: format!("{}{}", para("Lead."), para("New.")),
            warning: None,
        },
        Case {
            name: "an insertion, then the whole final paragraph deleted",
            source: format!("{}{}", para("Lead."), para("Gone.")),
            ops: r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]},{"op":"delete","ref":"p2"}"#,
            result: format!("{}{}", para("Lead."), para("New.")),
            warning: None,
        },
    ]
}

/// Asserts what the receipt says about kept paragraphs: the warning, and the operation line that says the paragraph was
/// emptied, or neither.
fn assert_said(case: &Case, receipt: &Receipt, mode: &str) {
    let name = case.name;
    match case.warning {
        Some((shown, message)) => {
            assert_eq!(
                kept(receipt),
                [(shown.to_string(), message.to_string())],
                "{name}, {mode}: {}",
                receipt.text
            );
            let line = format!("0 delete {shown}: {shown} emptied and kept");
            assert!(
                receipt.text.contains(&line),
                "{name}, {mode}: {}",
                receipt.text
            );
            assert!(
                receipt.text.contains(&format!(
                    "warning kept_required_paragraph {shown}: {message}"
                )),
                "{name}, {mode}: {}",
                receipt.text
            );
        }
        None => assert!(kept(receipt).is_empty(), "{name}, {mode}: {}", receipt.text),
    }
}

#[test]
fn a_tracked_delete_removes_a_paragraph_the_container_does_not_need_and_says_when_it_keeps_one() {
    for case in cases() {
        let name = case.name;
        let mut doc = open(&case.source);
        let source = body(&doc);
        let receipt = edit(&mut doc, case.ops, true);
        assert_said(&case, &receipt, "tracked");
        let mut rejected = open(&case.source);
        edit(&mut rejected, case.ops, true);
        edit(&mut doc, r#"{"op":"accept","all":true}"#, true);
        // A new run's text is written without `xml:space`, which the comparison leaves out.
        assert_eq!(
            body(&doc).replace("<w:t>", r#"<w:t xml:space="preserve">"#),
            case.result,
            "{name}: accept all"
        );
        edit(&mut rejected, r#"{"op":"reject","all":true}"#, true);
        // A run that a range cut in two stays in two runs of the same properties.
        let joined =
            |xml: String| xml.replace(r#"</w:t></w:r><w:r><w:t xml:space="preserve">"#, "");
        assert_eq!(
            joined(body(&rejected)),
            joined(source),
            "{name}: reject all"
        );
    }
}

#[test]
fn an_untracked_delete_removes_a_paragraph_the_container_does_not_need_and_says_when_it_keeps_one()
{
    for case in cases() {
        let mut doc = open(&case.source);
        let receipt = edit(&mut doc, case.ops, false);
        assert_said(&case, &receipt, "untracked");
        assert_eq!(
            body(&doc).replace("<w:t>", r#"<w:t xml:space="preserve">"#),
            case.result,
            "{}: untracked",
            case.name
        );
    }
}

#[test]
fn accepting_the_removal_of_a_paragraph_retires_its_ref() {
    let mut doc = open(&format!("{}{}", para("Lead."), para("Gone.")));
    edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#, true);
    let receipt = edit(&mut doc, r#"{"op":"accept","all":true}"#, true);
    let retired: Vec<(&str, &str)> = receipt
        .retired
        .iter()
        .map(|r| (r.r#ref.as_str(), r.reason.as_str()))
        .collect();
    // The paragraph before it lost its mark in its place, so accepting joins the two and the ref of the last goes into it.
    assert_eq!(retired, [("p2", "merged into p1")], "{}", receipt.text);
}

#[test]
fn deleting_the_last_paragraph_and_then_inserting_after_it() {
    // Tracked, the last paragraph keeps its mark (the paragraph before it loses its own), so an insertion after it has a
    // place, and accepting leaves the paragraph before it and the new one. Untracked, it is gone, and the refusal says
    // what to do instead.
    let ops = r#"{"op":"delete","ref":"p2"},{"op":"insert","ref":"p2","place":"after","paragraphs":["New."]}"#;
    let mut doc = open(&format!("{}{}", para("Lead."), para("Gone.")));
    edit(&mut doc, ops, true);
    edit(&mut doc, r#"{"op":"accept","all":true}"#, true);
    assert_eq!(
        body(&doc).replace("<w:t>", r#"<w:t xml:space="preserve">"#),
        format!("{}{}", para("Lead."), para("New."))
    );
    let mut doc = open(&format!("{}{}", para("Lead."), para("Gone.")));
    let err = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).untracked())
        .err()
        .unwrap();
    assert_eq!(
        err.code(),
        crate::api::error::ErrorCode::StaleRef,
        "{err:?}"
    );
    assert!(
        err.message().contains("Name a block that is still there, such as the paragraph before it, or, to keep the paragraph for a later operation, delete only its text."),
        "{err:?}"
    );
}

#[test]
fn a_last_paragraph_that_holds_another_authors_deletion_is_kept_so_accepting_the_authors_revisions_agrees_with_final()
 {
    // Another author commented on and deleted the body's only paragraph, which kept it empty. The author then inserts a
    // paragraph before it and deletes it: a deleted mark would not remove it on accepting the author's revisions alone,
    // since the other author's deletion is still in it, so it stays, and the receipt says why.
    let mut doc = open(&para("Only."));
    let reviewer = Edit::ops(
        batch(
            &doc,
            r#"{"op":"comment","ref":"p1","text":"Note"},{"op":"delete","ref":"p1"}"#,
        ),
        crate::api::stamp::Stamp::new("Reviewer", "2026-10-06T10:00:00"),
    );
    doc.edit(reviewer).unwrap();
    let receipt = edit(
        &mut doc,
        r#"{"op":"insert","ref":"p1","place":"before","paragraphs":["New."]},{"op":"delete","ref":"p1"}"#,
        true,
    );
    assert_eq!(
        kept(&receipt),
        [(
            "p1".to_string(),
            "p1 holds another author's pending deletion, and no paragraph follows it to take that, so it can go only once that change is resolved; its text was deleted and the empty paragraph kept".to_string()
        )],
        "{}",
        receipt.text
    );
    let lines = |doc: &Document| {
        (1..8)
            .map(|n| format!("p{n}"))
            .filter(|t| doc.state.resolve(crate::refs::Claim::Implied, t).is_ok())
            .map(|t| final_text(doc, &t))
            .collect::<Vec<_>>()
    };
    let before = lines(&doc);
    edit(&mut doc, r#"{"op":"accept","author":"Clauseo"}"#, true);
    assert_eq!(lines(&doc), before);
    assert!(!body(&doc).contains("<w:rPr><w:del"), "{}", body(&doc));
}

#[test]
fn a_containers_last_paragraph_keeps_its_mark_and_the_paragraph_before_it_loses_its_own_as_word_needs()
 {
    // Word cannot accept a deleted mark on a container's last paragraph: in the body the revision stays pending, and in a cell
    // Word's Accept All removes the whole table (Word for Mac 16.114.4, batch 2, folders 02 and 19). So the last paragraph
    // keeps its mark, the paragraph before it loses its own, and the last paragraph takes that paragraph's properties in a
    // change of its own, so that accepting keeps the earlier paragraph's properties whichever paragraph a join keeps.
    const AT: &str = r#"w:author="Clauseo" w:date="2026-10-06T10:00:00Z""#;
    let lead = r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t xml:space="preserve">Lead.</w:t></w:r></w:p>"#;
    let gone = r#"<w:p><w:pPr><w:spacing w:after="240"/></w:pPr><w:r><w:t xml:space="preserve">Gone.</w:t></w:r></w:p>"#;
    let cell = table(&format!("{}{}", para("Cell one."), para("Cell two.")));
    let cases = [
        (
            format!("{lead}{gone}"),
            format!(
                r#"<w:p><w:pPr><w:jc w:val="center"/><w:rPr><w:del w:id="2" {AT}/></w:rPr></w:pPr><w:r><w:t xml:space="preserve">Lead.</w:t></w:r></w:p><w:p><w:pPr><w:jc w:val="center"/><w:pPrChange w:id="1" {AT}><w:pPr><w:spacing w:after="240"/></w:pPr></w:pPrChange></w:pPr><w:del w:id="0" {AT}><w:r><w:delText xml:space="preserve">Gone.</w:delText></w:r></w:del></w:p>"#
            ),
            lead.to_string(),
        ),
        (
            format!("{cell}{}", para("After.")),
            format!(
                r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc><w:p><w:pPr><w:rPr><w:del w:id="1" {AT}/></w:rPr></w:pPr><w:r><w:t xml:space="preserve">Cell one.</w:t></w:r></w:p><w:p><w:del w:id="0" {AT}><w:r><w:delText xml:space="preserve">Cell two.</w:delText></w:r></w:del></w:p></w:tc></w:tr></w:tbl>{}"#,
                para("After.")
            ),
            format!("{}{}", table(&para("Cell one.")), para("After.")),
        ),
    ];
    for (source, tracked, accepted) in cases {
        let mut doc = open(&source);
        let receipt = edit(&mut doc, r#"{"op":"delete","ref":"p2"}"#, true);
        assert!(kept(&receipt).is_empty(), "{}", receipt.text);
        assert_eq!(body(&doc), tracked);
        let mut rejected = open(&source);
        edit(&mut rejected, r#"{"op":"delete","ref":"p2"}"#, true);
        edit(&mut doc, r#"{"op":"accept","all":true}"#, true);
        assert_eq!(body(&doc), accepted);
        edit(&mut rejected, r#"{"op":"reject","all":true}"#, true);
        assert_eq!(body(&rejected), source);
    }
}

#[test]
fn rejecting_an_insertion_after_a_last_paragraph_of_markers_keeps_them_in_it() {
    // A tracked delete of a last paragraph can leave it with nothing but range markers, and an insertion after it is then
    // the Y shape. Rejecting it gives the paragraph back with its markers inside (the soak's R2, document 18).
    let source = format!(
        r#"{}<w:p><w:bookmarkStart w:id="1" w:name="b"/><w:bookmarkEnd w:id="1"/></w:p>"#,
        para("Lead.")
    );
    let mut doc = open(&source);
    edit(
        &mut doc,
        r#"{"op":"insert","ref":"p2","place":"after","paragraphs":["days"]}"#,
        true,
    );
    edit(&mut doc, r#"{"op":"reject","all":true}"#, true);
    assert_eq!(body(&doc), source);
}
