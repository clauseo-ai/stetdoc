//! Tests of `blocks.rs`: L2 by block.

use super::*;
use crate::fixtures::parts::{Doc, entries_doc};
use crate::fixtures::zip::Zip;
use crate::laws::edits::run_batch;
use crate::laws::generate::GenOp;
use stetdoc_core::{Document, OpenOptions};

fn generated(ops: &[(&'static str, &str)], tracked: bool) -> Generated {
    let ops = ops
        .iter()
        .map(|(name, json)| GenOp {
            name,
            json: (*json).to_string(),
            refused: None,
            sticks: None,
        })
        .collect();
    Generated { ops, tracked }
}

/// The bytes after an untracked batch of `ops`.
fn edited(bytes: &[u8], ops: &str) -> Vec<u8> {
    run_batch(bytes, &[], ops, false).unwrap().unwrap().saved
}

#[test]
fn an_insertion_after_a_paragraph_whose_mark_is_gone_touches_the_paragraph_it_joins() {
    // exhibit_table_of_contents.docx, seed 7024836520723365829, batch 8: a prelude deleted p2 to the end, so p1's mark
    // went and p2 is the emptied last paragraph; an insertion after p1 goes after p2, as a Y that gives p2 an inserted
    // mark.
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let body = format!("{}{}", para("Lead."), para("Gone."));
    let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
    let prelude = generated(&[("delete", r#"{"op":"delete","ref":"p2"}"#)], true);
    let preluded = run_batch(&bytes, &[], &prelude.json(), true)
        .unwrap()
        .unwrap()
        .saved;
    let facts =
        Facts::of(&Document::open(preluded.clone(), &OpenOptions::default()).unwrap()).unwrap();
    let op = r#"{"op":"insert","ref":"p1","place":"after","paragraphs":["New."]}"#;
    let g = generated(&[("insert", op)], true);
    assert!(touched(&facts, &g).tops.contains("p2"));
    let after = run_batch(&preluded, &[], &g.json(), true)
        .unwrap()
        .unwrap()
        .saved;
    per_block(&preluded, &after, &facts, &g).unwrap();
}

#[test]
fn a_tracked_range_to_the_end_of_the_body_touches_the_paragraph_before_it() {
    // fdo64256.docx, batch 42 of seed 787681986111147656: the range takes p4 and the empty p5 whole, so the mark that goes
    // is p3's, the paragraph with text left before them.
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let body = format!("{}{}{}{}<w:p/>", para("S"), para("s"), para("t"), para("u"));
    let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
    let facts =
        Facts::of(&Document::open(bytes.clone(), &OpenOptions::default()).unwrap()).unwrap();
    let op = r#"{"op":"delete","ref":"p4","to":"p5","start":0,"end":0}"#;
    let g = generated(&[("delete", op)], true);
    let after = run_batch(&bytes, &[], &g.json(), true)
        .unwrap()
        .unwrap()
        .saved;
    per_block(&bytes, &after, &facts, &g).unwrap();
    assert!(touched(&facts, &g).tops.contains("p3"));
    // Offsets count characters: a last paragraph of non-ASCII text taken to its end is taken whole.
    let body = format!("{}{}{}", para("S"), para("s"), para("Ünïcödé"));
    let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
    let wide = Facts::of(&Document::open(bytes, &OpenOptions::default()).unwrap()).unwrap();
    let op = r#"{"op":"delete","ref":"p2","to":"p3","start":0,"end":7}"#;
    assert!(
        touched(&wide, &generated(&[("delete", op)], true))
            .tops
            .contains("p1")
    );
    let op = r#"{"op":"delete","ref":"p2","to":"p3","start":0,"end":6}"#;
    assert!(
        !touched(&wide, &generated(&[("delete", op)], true))
            .tops
            .contains("p1")
    );
    // From inside the first paragraph, its text takes the last one's place, and the one before is not touched.
    let op = r#"{"op":"delete","ref":"p4","to":"p5","start":1,"end":0}"#;
    assert!(
        !touched(&facts, &generated(&[("delete", op)], true))
            .tops
            .contains("p3")
    );
}

#[test]
fn a_tracked_delete_of_the_last_paragraph_touches_the_paragraph_before_only_when_it_has_text() {
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let table = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>"#;
    for (before, touched_before) in [
        (para("Lead."), true),
        (table.to_string(), false),
        ("<w:p/>".to_string(), false),
    ] {
        let body = format!("{before}{}", para("Last."));
        let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
        let facts = Facts::of(&Document::open(bytes, &OpenOptions::default()).unwrap()).unwrap();
        let last = facts.top.last().cloned().unwrap();
        let first = facts.top.first().cloned().unwrap();
        let g = generated(
            &[("delete", &format!(r#"{{"op":"delete","ref":"{last}"}}"#))],
            true,
        );
        let t = touched(&facts, &g);
        assert_eq!(t.tops.contains(&first), touched_before, "{body}");
    }
}

#[test]
fn a_merge_after_the_last_row_of_a_table_is_deleted_names_the_paragraph_after_the_table() {
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let body = format!(
        r#"{}<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr>{}</w:tc></w:tr></w:tbl>{}{}"#,
        para("Before."),
        para("Cell."),
        para("After."),
        para("Far.")
    );
    let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
    let facts =
        Facts::of(&Document::open(bytes.clone(), &OpenOptions::default()).unwrap()).unwrap();
    assert_eq!(facts.top, ["p1", "t1", "p3", "p4"]);
    let (delete, merge) = (
        r#"{"op":"row","ref":"r1","action":"delete"}"#,
        r#"{"op":"merge","ref":"p1"}"#,
    );
    // Untracked, the row's deletion removes the table, and the merge joins p1 with p3, which the batch names.
    let g = generated(&[("row", delete), ("merge", merge)], false);
    let after = edited(&bytes, &g.json());
    per_block(&bytes, &after, &facts, &g).unwrap();
    // A paragraph that the batch does not name and that changes is still caught.
    let changed = edited(
        &bytes,
        &format!(
            r#"{},{{"op":"replace","ref":"p4","find":"Far","with":"Near"}}"#,
            g.json()
        ),
    );
    let e = per_block(&bytes, &changed, &facts, &g).unwrap_err();
    assert!(
        e.starts_with("L2: p4, which the batch does not name"),
        "{e}"
    );
}

#[test]
fn a_table_after_a_removed_table_may_gain_its_comment_in_its_first_paragraph() {
    // floattable-anchorpos.docx, seed 10998282968869546347: removing t1 hoists its comment into p2, in t2.
    let table = |t: &str| {
        format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#
        )
    };
    let body = format!(
        "{}{}<w:p><w:r><w:t>D</w:t></w:r></w:p>",
        table("C"),
        table("A")
    );
    let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
    let facts =
        Facts::of(&Document::open(bytes.clone(), &OpenOptions::default()).unwrap()).unwrap();
    assert_eq!(facts.top, ["t1", "t2", "p3"]);
    let comment = r#"{"op":"comment","ref":"p1","start":0,"end":0,"text":"Note"}"#;
    let g = generated(
        &[
            ("comment", comment),
            ("delete", r#"{"op":"delete","ref":"t1"}"#),
        ],
        false,
    );
    let after = edited(&bytes, &g.json());
    let xml = part(&after, "word/document.xml").unwrap();
    assert!(
        String::from_utf8_lossy(&xml).contains("commentReference"),
        "the comment is gone"
    );
    per_block(&bytes, &after, &facts, &g).unwrap();
    // The table's text changing is still caught.
    let changed = edited(
        &bytes,
        &format!(
            r#"{},{{"op":"replace","ref":"p2","find":"A","with":"B"}}"#,
            g.json()
        ),
    );
    let e = per_block(&bytes, &changed, &facts, &g).unwrap_err();
    assert!(
        e.starts_with("L2: t2, which the batch does not name"),
        "{e}"
    );
}

#[test]
fn deleting_to_the_end_past_an_emptied_paragraph_touches_the_one_before_it() {
    // exhibit_table_of_contents.docx, seed 110, batch 4: an earlier batch deleted p3 with its mark, so deleting p4 to the end
    // makes p2 lose its mark in p3's place.
    let para = |t: &str| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>");
    let body = format!(
        "{}{}{}{}",
        para("Lead."),
        para("Two."),
        para("Three."),
        para("End.")
    );
    let bytes = Zip::new(entries_doc(&Doc::new().body(&body).bytes())).build();
    let prelude = generated(&[("delete", r#"{"op":"delete","ref":"p3"}"#)], true);
    let preluded = run_batch(&bytes, &[], &prelude.json(), true)
        .unwrap()
        .unwrap()
        .saved;
    let facts =
        Facts::of(&Document::open(preluded.clone(), &OpenOptions::default()).unwrap()).unwrap();
    let g = generated(&[("delete", r#"{"op":"delete","ref":"p4"}"#)], true);
    assert!(touched(&facts, &g).tops.contains("p2"));
    let after = run_batch(&preluded, &[], &g.json(), true)
        .unwrap()
        .unwrap()
        .saved;
    per_block(&preluded, &after, &facts, &g).unwrap();
}
