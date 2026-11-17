//! Decision 0021 through `compare`: a nested table that ended its cell in the original is deleted with an empty paragraph after
//! it, and a change at the edge of a field or a read-only content control is written, while one inside a read-only region
//! replaces its paragraph whole. None of them refuses the comparison.

use super::tests_edit::stamp;
use super::*;
use crate::api::edit::Edit;
use crate::api::limits::Limits;
use crate::api::options::CompareOptions;
use crate::api::view::{Mode, ViewSpec, WarningCode};
use crate::threads::tests_support::Pkg;

fn r(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn p(text: &str) -> String {
    format!("<w:p>{}</w:p>", r(text))
}

fn options() -> CompareOptions {
    CompareOptions {
        limits: Limits::default(),
        stamp: stamp(),
        format: crate::api::view::FormatId::stet(),
    }
}

/// The blocks of the body as the final view shows them, without their refs.
fn blocks(doc: &Document) -> Vec<String> {
    let spec = ViewSpec {
        mode: Mode::Final,
        ..ViewSpec::default()
    };
    doc.view(&spec)
        .unwrap()
        .text
        .lines()
        .filter(|l| {
            let mut c = l.chars();
            matches!(c.next(), Some('p' | 't' | 'r'))
                && c.next().is_some_and(|d| d.is_ascii_digit())
        })
        .filter_map(|l| l.split_once(' ').map(|(_, t)| t.to_string()))
        .collect()
}

/// The document after accepting, or rejecting, every revision.
fn resolved(doc: &Document, op: &str) -> Document {
    let mut copy = Document::open(doc.save().unwrap().bytes, &OpenOptions::default()).unwrap();
    let b = super::tests_edit::batch(&copy, &format!(r#"{{"op":"{op}","all":true}}"#));
    copy.edit(Edit::ops(b, stamp())).unwrap();
    copy
}

fn open(bytes: &[u8]) -> Document {
    Document::open(bytes.to_vec(), &OpenOptions::default()).unwrap()
}

fn codes(c: &Compared) -> Vec<WarningCode> {
    c.receipt.warnings.iter().map(|w| w.code).collect()
}

fn body_xml(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    String::from_utf8_lossy(&view.tree(id).unwrap().serialize()).into_owned()
}

fn nested(inner: bool) -> Vec<u8> {
    let grid = "<w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>";
    let table = |rows: &[[String; 2]]| {
        let rows: String = rows
            .iter()
            .map(|[a, b]| {
                format!("<w:tr><w:tc><w:tcPr/>{a}</w:tc><w:tc><w:tcPr/>{b}</w:tc></w:tr>")
            })
            .collect();
        format!("<w:tbl><w:tblPr/>{grid}{rows}</w:tbl>")
    };
    let inner_table = table(&[
        [p("Inner A1 original wording"), p("Inner B1 stays")],
        [p("Inner A2 stays"), p("Inner B2 stays")],
    ]);
    // as some generators write it: the nested table is the cell's last block, with no paragraph after it
    let cell = if inner {
        format!(
            "{}{inner_table}",
            p("Outer cell text with several words here")
        )
    } else {
        p("Outer cell text with several words here")
    };
    let outer = table(&[
        [cell, p("Outer B1 stays the same")],
        [p("Outer A2"), p("Outer B2")],
    ]);
    Pkg::new(&format!(
        "{}{outer}{}",
        p("Intro paragraph that does not change at all."),
        p("Closing paragraph that stays the same.")
    ))
    .bytes()
}

#[test]
fn a_nested_table_that_ended_its_cell_is_deleted_with_an_empty_paragraph_after_it() {
    let (a, b) = (nested(true), nested(false));
    assert!(
        !open(&a).check().ok,
        "the original's cell ends with a table"
    );
    let compared = compare(a.clone(), b.clone(), &options()).expect("compares");
    let doc = &compared.document;
    assert!(doc.check().ok, "{:?}", doc.check());
    let xml = body_xml(doc);
    assert!(xml.contains("</w:tbl><w:p/></w:tc>"), "{xml}");
    let warned: Vec<_> = compared
        .receipt
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::CompareNotRestored)
        .collect();
    assert_eq!(warned.len(), 1, "{:?}", compared.receipt.warnings);
    assert!(warned[0].block.is_some() && warned[0].message.contains("empty paragraph"));
    // accepting gives the revised document and rejecting the original, each with the empty paragraph at the cell's end
    let with_end = |d: &Document, after: &str| {
        let mut out = blocks(d);
        let at = out.iter().position(|l| l == after).unwrap() + 1;
        out.insert(at, "{+}".to_string());
        out
    };
    let accepted = blocks(&resolved(doc, "accept"));
    assert_eq!(
        accepted,
        with_end(&open(&b), "Outer cell text with several words here")
    );
    let rejected = blocks(&resolved(doc, "reject"));
    assert_eq!(rejected, with_end(&open(&a), "Inner B2 stays"));
}

/// A paragraph with `thing` in the middle, as an element of its own.
fn around(before: &str, thing: &str, after: &str) -> Vec<u8> {
    Pkg::new(&format!(
        "{}<w:p>{}{thing}{}</w:p>{}",
        p("Intro stays the same."),
        r(before),
        r(after),
        p("Closing stays.")
    ))
    .bytes()
}

fn simple(result: &str) -> String {
    format!(
        "<w:fldSimple w:instr=\" DATE \">{}</w:fldSimple>",
        r(result)
    )
}

fn complex(result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> DATE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r(result)
    )
}

fn locked(result: &str) -> String {
    format!(
        "<w:sdt><w:sdtPr><w:lock w:val=\"contentLocked\"/><w:id w:val=\"77\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>",
        r(result)
    )
}

/// Compares `a` with `b` and holds the redline to accepting and rejecting, and to `check`.
fn holds(a: &[u8], b: &[u8]) -> Compared {
    let compared = compare(a.to_vec(), b.to_vec(), &options()).expect("compares");
    let doc = &compared.document;
    assert!(doc.check().ok, "{:?}", doc.check());
    assert_eq!(blocks(&resolved(doc, "accept")), blocks(&open(b)));
    assert_eq!(blocks(&resolved(doc, "reject")), blocks(&open(a)));
    compared
}

#[test]
fn a_change_at_the_edge_of_a_field_or_a_locked_control_is_written_in_place() {
    for (name, thing) in [
        ("a simple field", simple as fn(&str) -> String),
        ("a complex field", complex),
        ("a locked control", locked),
    ] {
        let a = around(
            "Text before the thing ",
            &thing("RESULT"),
            " text after the thing.",
        );
        // a word deleted right before it, a word deleted right after it, and words inserted on each side
        for b in [
            around(
                "Text before the ",
                &thing("RESULT"),
                " text after the thing.",
            ),
            around(
                "Text before the thing ",
                &thing("RESULT"),
                " after the thing.",
            ),
            around(
                "Text before the thing NEW ",
                &thing("RESULT"),
                " text after the thing.",
            ),
            around(
                "Text before the thing ",
                &thing("RESULT"),
                " NEW text after the thing.",
            ),
        ] {
            for (x, y) in [(&a, &b), (&b, &a)] {
                let c = holds(x, y);
                assert!(
                    !codes(&c).contains(&WarningCode::CompareParagraphReplaced),
                    "{name}: {:?}",
                    c.receipt.warnings
                );
            }
        }
    }
}

#[test]
fn paragraphs_joined_or_split_at_the_end_of_a_field_or_a_locked_control_are_a_merge_or_a_split() {
    for (name, thing) in [
        ("a simple field", simple as fn(&str) -> String),
        ("a complex field", complex),
        ("a locked control", locked),
    ] {
        let one = around(
            "Text before the thing ",
            &thing("RESULT"),
            " text after the thing.",
        );
        let two = Pkg::new(&format!(
            "{}<w:p>{}{}</w:p>{}{}",
            p("Intro stays the same."),
            r("Text before the thing "),
            thing("RESULT"),
            p(" text after the thing."),
            p("Closing stays.")
        ))
        .bytes();
        for (x, y) in [(&one, &two), (&two, &one)] {
            let c = holds(x, y);
            assert!(
                !codes(&c).contains(&WarningCode::CompareParagraphReplaced),
                "{name}: {:?}",
                c.receipt.warnings
            );
        }
    }
}

#[test]
fn a_change_inside_a_locked_control_or_a_simple_fields_result_replaces_the_paragraph_whole() {
    for thing in [simple as fn(&str) -> String, locked] {
        let a = around(
            "Text before the thing ",
            &thing("RESULT"),
            " text after the thing.",
        );
        let b = around(
            "Text before the thing ",
            &thing("OTHER"),
            " text after the thing.",
        );
        let compared = compare(a, b, &options()).expect("compares, never refuses");
        assert!(compared.document.check().ok);
        assert!(
            codes(&compared).contains(&WarningCode::CompareParagraphReplaced),
            "{:?}",
            compared.receipt.warnings
        );
    }
}
