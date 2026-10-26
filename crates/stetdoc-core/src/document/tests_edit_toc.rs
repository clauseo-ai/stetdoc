//! A field whose result spans paragraphs (a table of contents) collapses in a view only when that hides nothing the view would
//! show, through `Document::edit` and `Document::view` (design `projection-and-index.md` 3.3, step 3): a pending revision on the
//! mark of the paragraph where it ends keeps it open in `Redline`, and in `Final` that paragraph joins the next one when its mark
//! is deleted, so the field collapses exactly as it does once the deletion is accepted (law L5).

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::view::{Budget, Mode, Scope, StoryName, ViewSpec};
use stetdoc_testkit::fixtures::{parts, zip::Zip};

/// A table of contents over p1 to p4, where p4 holds the field's end and then `tail`, followed by `next` and "After".
fn toc(tail: &str, next: &str) -> Document {
    let run = |t: &str| format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    let para = |inner: String| format!("<w:p>{inner}</w:p>");
    let body = [
        para(format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TOC \o "1-3" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{}"#,
            run("Heading one\t1")
        )),
        para(run("Heading two\t1")),
        para(run("Heading three\t2")),
        para(format!(
            r#"<w:r><w:fldChar w:fldCharType="end"/></w:r>{}"#,
            if tail.is_empty() { String::new() } else { run(tail) }
        )),
        para(if next.is_empty() { String::new() } else { run(next) }),
        para(run("After")),
    ]
    .concat();
    let xml = parts::Doc::new().body(&body).bytes();
    Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

/// The body's view in a mode, as text.
fn shown(doc: &Document, mode: Mode) -> String {
    let spec = ViewSpec {
        scope: Scope::Story(StoryName::Body),
        mode,
        budget: Budget(u32::MAX),
        ..ViewSpec::default()
    };
    doc.view(&spec).unwrap().text
}

/// The texts the body's Final view shows, in order: each block line without its ref and markers, and a collapsed field as
/// `toc`. A paragraph that the view hides because it joins the next is not a line.
fn final_lines(doc: &Document) -> Vec<String> {
    shown(doc, Mode::Final)
        .lines()
        .filter_map(|l| {
            if l.starts_with("{opaque toc") {
                return Some("toc".to_string());
            }
            let (r, rest) = l.split_once(' ').unwrap_or((l, ""));
            let is_ref =
                r.len() > 1 && r.starts_with('p') && r[1..].chars().all(|c| c.is_ascii_digit());
            is_ref.then(|| {
                let mut out = String::new();
                let mut depth = 0;
                for c in rest.chars() {
                    match c {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ if depth == 0 => out.push(c),
                        _ => {}
                    }
                }
                out.trim().to_string()
            })
        })
        .collect()
}

fn accept(doc: &mut Document) {
    run(doc, r#"{"op":"accept","all":true}"#);
}

#[test]
fn deleting_the_paragraph_where_a_table_of_contents_ends_shows_in_final_what_accepting_leaves() {
    // The deletion takes "Tail" and p4's mark, so p4 joins "Heading" (TOC_field_f.docx, seed 3104574440245829000).
    let mut doc = toc("Tail", "Heading");
    run(&mut doc, r#"{"op":"delete","ref":"p4"}"#);
    let intended = final_lines(&doc);
    assert!(intended.contains(&"Heading".to_string()), "{intended:?}");
    accept(&mut doc);
    assert_eq!(final_lines(&doc), intended, "{}", shown(&doc, Mode::Final));
    // When the paragraph that p4 joins is empty, the field takes it in, before and after the accept.
    let mut doc = toc("Tail", "");
    run(&mut doc, r#"{"op":"delete","ref":"p4"}"#);
    let intended = final_lines(&doc);
    assert_eq!(intended, ["toc", "After"], "{}", shown(&doc, Mode::Final));
    accept(&mut doc);
    assert_eq!(final_lines(&doc), intended, "{}", shown(&doc, Mode::Final));
}

#[test]
fn a_tracked_split_after_the_end_of_a_table_of_contents_shows_its_new_mark_in_redline() {
    // p4 is the field's end and "Tail": a split before "Tail" leaves p4 with only the end, and an inserted mark that the
    // redline must show (toc_doc.docx, seed 9459939230244126257).
    let mut doc = toc("Tail", "Heading");
    let before = shown(&doc, Mode::Redline);
    assert!(!before.contains("{opaque"), "{before}");
    run(&mut doc, r#"{"op":"split","ref":"p4","before":"Tail"}"#);
    let redline = shown(&doc, Mode::Redline);
    assert!(!redline.contains("{opaque"), "{redline}");
    assert!(redline.contains("{¶+"), "{redline}");
    // Untracked, the paragraphs are the field's alone, and it collapses.
    let mut doc = toc("Tail", "Heading");
    let b = batch(&doc, r#"{"op":"split","ref":"p4","before":"Tail"}"#);
    doc.edit(Edit::ops(b, stamp()).untracked()).unwrap();
    assert!(shown(&doc, Mode::Redline).contains("{opaque toc"));
}
