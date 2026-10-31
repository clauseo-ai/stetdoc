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
    toc_after("", tail, next)
}

/// `toc`, after the body XML `lead`.
fn toc_after(lead: &str, tail: &str, next: &str) -> Document {
    let run = |t: &str| format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    let para = |inner: String| format!("<w:p>{inner}</w:p>");
    let body = [
        lead.to_string(),
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
    lines_in(doc, Mode::Final)
}

/// `final_lines`, in any mode.
fn lines_in(doc: &Document, mode: Mode) -> Vec<String> {
    shown(doc, mode)
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

#[test]
fn deleting_a_paragraph_before_a_table_of_contents_shows_in_final_what_accepting_leaves() {
    // The paragraph holds text and a field with an empty result (a TA entry), and its mark joins it to the paragraph where the
    // table begins. In Final only the empty field is left, which is no text, so the field takes it in, as it does once the
    // deletion is accepted (Complex 3.docx, seed 107).
    let lead = r#"<w:p><w:r><w:t>Quote</w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TA \l "Quote" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let mut doc = toc_after(lead, "", "Heading");
    assert_eq!(final_lines(&doc), ["Quote", "toc", "Heading", "After"]);
    run(&mut doc, r#"{"op":"delete","ref":"p1"}"#);
    let intended = final_lines(&doc);
    assert_eq!(
        intended,
        ["toc", "Heading", "After"],
        "{}",
        shown(&doc, Mode::Final)
    );
    accept(&mut doc);
    assert_eq!(final_lines(&doc), intended, "{}", shown(&doc, Mode::Final));
}

/// A table of contents over three paragraphs, the first with the paragraph mark `mark` (`""`, or a tracked `ins` or `del`), the
/// second with `entry` as its runs, and the third only the field's end, followed by "After".
fn toc_of_three(mark: &str, entry: &str) -> Document {
    let p_pr = if mark.is_empty() {
        String::new()
    } else {
        format!(
            r#"<w:pPr><w:rPr><w:{mark} w:id="90" w:author="JD" w:date="2026-01-01T00:00:00Z"/></w:rPr></w:pPr>"#
        )
    };
    let body = [
        format!(
            r#"<w:p>{p_pr}<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> TOC \o "1-3" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t xml:space="preserve">Heading one	1</w:t></w:r></w:p>"#
        ),
        format!("<w:p>{entry}</w:p>"),
        r#"<w:p><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#.to_string(),
        r#"<w:p><w:r><w:t>After</w:t></w:r></w:p>"#.to_string(),
    ]
    .concat();
    let xml = parts::Doc::new().body(&body).bytes();
    Document::open(
        Zip::new(parts::entries_doc(&xml)).build(),
        &OpenOptions::default(),
    )
    .unwrap()
}

const ENTRY: &str = r#"<w:r><w:t xml:space="preserve">Heading two	1</w:t></w:r>"#;

/// The text of a line: its markers left out, but for a field marker, which reads as its quoted text.
fn line_text(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(at) = rest.find('{') {
        out.push_str(&rest[..at]);
        let end = rest[at..].find('}').map_or(rest.len(), |e| at + e);
        let marker = &rest[at..end];
        if marker.starts_with("{field ")
            && let (Some(a), Some(b)) = (marker.find('"'), marker.rfind('"'))
        {
            out.push_str(&marker[a + 1..b.max(a + 1)]);
        }
        rest = rest.get(end + 1..).unwrap_or_default();
    }
    out.push_str(rest);
    out.trim().to_string()
}

/// The body's lines in a mode as a review would leave them, as L5 compares them: each line's text without its ref, a collapsed
/// field as `toc`, and a paragraph that joins the next one in one line with it.
fn joined_lines(doc: &Document, mode: Mode) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut join = false;
    for l in shown(doc, mode).lines() {
        let text = if l.starts_with("{opaque toc") {
            "toc".to_string()
        } else {
            let (r, rest) = l.split_once(' ').unwrap_or((l, ""));
            if !(r.len() > 1 && r.starts_with('p') && r[1..].chars().all(|c| c.is_ascii_digit())) {
                continue;
            }
            rest.to_string()
        };
        let joins = text.contains("{joins-next}");
        let text = line_text(&text);
        match out.last_mut() {
            Some(last) if join => last.push_str(&text),
            _ => out.push(text),
        }
        join = joins;
    }
    out
}

/// The document's Final and Original lines, and those of the document that accepting and rejecting every revision make.
fn reviewed(make: impl Fn() -> Document) -> [(Vec<String>, Vec<String>); 2] {
    let doc = &make();
    let mut accepted = make();
    accept(&mut accepted);
    let mut rejected = make();
    run(&mut rejected, r#"{"op":"reject","all":true}"#);
    [
        (
            joined_lines(doc, Mode::Final),
            joined_lines(&accepted, Mode::Final),
        ),
        (
            joined_lines(doc, Mode::Original),
            joined_lines(&rejected, Mode::Original),
        ),
    ]
}

#[test]
fn a_table_of_contents_that_a_deleted_mark_brings_under_three_paragraphs_stays_open_in_final() {
    // In Final the first paragraph joins the second, so the field spans two paragraphs, as once accepted, and is not collapsed
    // (fdo77129.docx, seed 9351215736669558703). In Original the mark is there, three paragraphs, as once rejected.
    let doc = toc_of_three("del", ENTRY);
    let [(fin, accepted), (orig, rejected)] = reviewed(|| toc_of_three("del", ENTRY));
    assert!(
        !fin.contains(&"toc".to_string()),
        "{}",
        shown(&doc, Mode::Final)
    );
    assert_eq!(fin, accepted, "{}", shown(&doc, Mode::Final));
    assert_eq!(orig, ["toc", "After"], "{}", shown(&doc, Mode::Original));
    assert_eq!(orig, rejected);
    // The redline counts the paragraphs as they are.
    assert!(shown(&doc, Mode::Redline).contains("{opaque toc"));
}

#[test]
fn a_table_of_contents_that_an_inserted_mark_brings_to_three_paragraphs_stays_open_in_original() {
    // In Original the inserted mark is gone and the first paragraph joins the second, as once rejected; in Final it is there.
    let doc = toc_of_three("ins", ENTRY);
    let [(fin, accepted), (orig, rejected)] = reviewed(|| toc_of_three("ins", ENTRY));
    assert!(
        !orig.contains(&"toc".to_string()),
        "{}",
        shown(&doc, Mode::Original)
    );
    assert_eq!(orig, rejected, "{}", shown(&doc, Mode::Original));
    assert_eq!(fin, ["toc", "After"], "{}", shown(&doc, Mode::Final));
    assert_eq!(fin, accepted);
    assert!(shown(&doc, Mode::Redline).contains("{opaque toc"));
}

#[test]
fn a_table_of_contents_whose_paragraphs_no_revision_joins_collapses_in_every_mode() {
    // A pending insertion of text in an entry leaves the three paragraphs as they are.
    let entry = format!(
        r#"{ENTRY}<w:ins w:id="91" w:author="JD" w:date="2026-01-01T00:00:00Z"><w:r><w:t>new</w:t></w:r></w:ins>"#
    );
    let doc = toc_of_three("", &entry);
    let [(fin, accepted), (orig, rejected)] = reviewed(|| toc_of_three("", &entry));
    assert_eq!(fin, ["toc", "After"], "{}", shown(&doc, Mode::Final));
    assert_eq!((&fin, &orig), (&accepted, &rejected));
    assert_eq!(orig, fin);
    assert!(shown(&doc, Mode::Redline).contains("{opaque toc"));
}
