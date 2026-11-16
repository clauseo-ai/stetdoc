//! The tracked delete of a container's last paragraph next to insertions (issue #25), against what 0.9.3 (commit 8a0fb6e)
//! gave for the same operations: accept all and reject all must read the same, in the same batch or a later one, by the same
//! author or another, in the body or a cell, with equal or different properties. `last_mark_0_9_3.tsv` holds 0.9.3's
//! results, made with its release binary: one line per case, the name, then the body after accept all and after reject
//! all. 0.9.3 refused an insertion after the deleted last paragraph (`readonly_region`), since its mark was deleted; that
//! now works, and reads as an insertion after the paragraph before it.

use super::tests_edit::batch;
use super::*;
use crate::api::edit::Edit;
use crate::api::stamp::Stamp;
use stetdoc_testkit::fixtures::parts;
use stetdoc_testkit::fixtures::zip::Zip;

const EXPECTED: &str = include_str!("last_mark_0_9_3.tsv");

fn para(text: &str, ppr: &str) -> String {
    format!(r#"<w:p>{ppr}<w:r><w:t xml:space="preserve">{text}</w:t></w:r></w:p>"#)
}

fn source(container: &str, props: &str) -> String {
    let (lead, gone) = match props {
        "different" => (
            r#"<w:pPr><w:jc w:val="center"/><w:rPr><w:b/></w:rPr></w:pPr>"#,
            r#"<w:pPr><w:ind w:left="720"/><w:rPr><w:i/></w:rPr></w:pPr>"#,
        ),
        _ => ("", ""),
    };
    let two = format!("{}{}", para("Lead.", lead), para("Gone.", gone));
    match container {
        "cell" => format!(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="2400"/></w:tblGrid><w:tr><w:tc>{two}</w:tc></w:tr></w:tbl>{}"#,
            para("After.", "")
        ),
        _ => two,
    }
}

fn inserts(kind: &str) -> Vec<&'static str> {
    let before = r#"{"op":"insert","ref":"p2","place":"before","paragraphs":["New."]}"#;
    match kind {
        "before" => vec![before],
        "after1" => vec![r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#],
        "after2" => vec![r#"{"op":"insert","ref":"p2","place":"after","paragraphs":["New."]}"#],
        _ => vec![
            before,
            r#"{"op":"insert","ref":"p2","place":"before","paragraphs":["Two."]}"#,
        ],
    }
}

fn apply(doc: &mut Document, ops: &[&str], author: &str, day: &str) {
    let e = Edit::ops(
        batch(doc, &ops.join(",")),
        Stamp::new(author, format!("2026-10-{day}T10:00:00")),
    );
    doc.edit(e).unwrap_or_else(|err| panic!("{ops:?}: {err:?}"));
    assert!(doc.check().ok, "{ops:?}: {:?}", doc.check());
}

fn body(doc: &Document) -> String {
    let view = doc.state.parts();
    let id = view.id("word/document.xml").unwrap();
    let xml = String::from_utf8(view.tree(id).unwrap().serialize()).unwrap();
    let start = xml.find("<w:body>").unwrap() + "<w:body>".len();
    let end = xml.rfind("<w:sectPr").unwrap();
    xml[start..end].to_string()
}

/// The body with adjacent runs of the same formatting joined, which a cut leaves split.
fn normalized(xml: &str) -> String {
    let xml = xml.replace("<w:t>", r#"<w:t xml:space="preserve">"#);
    let mut out = String::new();
    let mut rest = xml.as_str();
    // The run being built: its formatting and text, while the next run follows it directly.
    let mut open: Option<(String, String)> = None;
    let flush = |out: &mut String, open: &mut Option<(String, String)>| {
        if let Some((rpr, text)) = open.take() {
            out.push_str(&format!(
                r#"<w:r>{rpr}<w:t xml:space="preserve">{text}</w:t></w:r>"#
            ));
        }
    };
    while let Some(at) = rest.find("<w:r>") {
        let (before, run) = rest.split_at(at);
        if !before.is_empty() {
            flush(&mut out, &mut open);
            out.push_str(before);
        }
        let end = run.find("</w:r>").unwrap() + "</w:r>".len();
        let inner = &run["<w:r>".len()..end - "</w:r>".len()];
        let simple = inner.find("<w:t xml:space=\"preserve\">").map(|t| {
            let rpr = &inner[..t];
            let text =
                &inner[t + r#"<w:t xml:space="preserve">"#.len()..inner.len() - "</w:t>".len()];
            (rpr.to_string(), text.to_string())
        });
        match (simple, open.as_mut()) {
            (Some((rpr, text)), Some((o, t))) if *o == rpr && inner.ends_with("</w:t>") => {
                t.push_str(&text)
            }
            (Some(run), _) if inner.ends_with("</w:t>") => {
                flush(&mut out, &mut open);
                open = Some(run);
            }
            _ => {
                flush(&mut out, &mut open);
                out.push_str(&run[..end]);
            }
        }
        rest = &run[end..];
    }
    flush(&mut out, &mut open);
    out.push_str(rest);
    out
}

#[test]
fn insertions_beside_a_deleted_last_paragraph_read_as_they_did_in_0_9_3() {
    let expected: std::collections::HashMap<&str, (&str, &str)> = EXPECTED
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?, (f.next()?, f.next()?)))
        })
        .collect();
    let mut checked = 0;
    for (name, (accept, reject)) in &expected {
        let parts_of: Vec<&str> = name.split('-').collect();
        let [container, props, timing, author, kind] = parts_of[..] else {
            panic!("{name}")
        };
        // 0.9.3 refused an insertion after the deleted last paragraph; it now reads as one after the paragraph before it.
        let (accept, reject) = if accept.starts_with("refused") {
            let other = name.replace("after2", "after1");
            expected[other.as_str()]
        } else {
            (*accept, *reject)
        };
        let xml = parts::Doc::new().body(&source(container, props)).bytes();
        let open = || {
            Document::open(
                Zip::new(parts::entries_doc(&xml)).build(),
                &OpenOptions::default(),
            )
            .unwrap()
        };
        let ops = inserts(kind);
        let mut results = Vec::new();
        for action in ["accept", "reject"] {
            let mut doc = open();
            if timing == "same" {
                let mut all = vec![r#"{"op":"delete","ref":"p2"}"#];
                all.extend(&ops);
                apply(&mut doc, &all, "Alice", "06");
            } else {
                apply(&mut doc, &[r#"{"op":"delete","ref":"p2"}"#], "Alice", "06");
                apply(
                    &mut doc,
                    &ops,
                    if author == "same" { "Alice" } else { "Bob" },
                    "07",
                );
            }
            apply(
                &mut doc,
                &[&format!(r#"{{"op":"{action}","all":true}}"#)],
                "Alice",
                "08",
            );
            results.push(normalized(&body(&doc)));
        }
        assert_eq!(results[0], normalized(accept), "{name}: accept all");
        assert_eq!(results[1], normalized(reject), "{name}: reject all");
        checked += 1;
    }
    assert_eq!(checked, 48);
}
