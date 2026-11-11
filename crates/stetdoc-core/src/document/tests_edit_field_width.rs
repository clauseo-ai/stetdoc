//! A field with no stored result has no width (issue #17). A complex field written as `begin`, instruction, `end`, with no
//! `separate`, and a `w:fldSimple` with no content, are as wide as a field whose result is empty, which is nothing, whatever
//! the instruction. Every offset after the field means the same character as in the text `view` shows, so an edit by offset
//! changes what it names, and an offset past the visible text is refused. A range across a simple field stays read-only.

use super::tests_edit::*;
use super::*;
use stetdoc_testkit::fixtures::parts;
use stetdoc_testkit::fixtures::zip::Zip;

/// The four ways to write `Alpha {field}beta.` whose field has nothing to show: `PAGE` with no `separate`, `XE` with no
/// `separate`, `PAGE` with a `separate` and no result, and an empty simple `PAGE` field.
fn document() -> Document {
    let run = |t: &str| format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    let fld = |kind: &str| format!(r#"<w:r><w:fldChar w:fldCharType="{kind}"/></w:r>"#);
    let instr = |code: &str| {
        format!(r#"<w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r>"#)
    };
    let p = |field: String| format!("<w:p>{}{field}{}</w:p>", run("Alpha "), run("beta."));
    let body = [
        p(fld("begin") + &instr(" PAGE ") + &fld("end")),
        p(fld("begin") + &instr(r#" XE "term" "#) + &fld("end")),
        p(fld("begin") + &instr(" PAGE ") + &fld("separate") + &fld("end")),
        p(r#"<w:fldSimple w:instr=" PAGE "/>"#.to_string()),
    ]
    .concat();
    let doc = parts::Doc::new().body(&body).bytes();
    let bytes = Zip::new(parts::entries_doc(&doc)).build();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

/// The line of a paragraph in the readable file.
fn line(doc: &Document, token: &str) -> String {
    let wc = doc.export().unwrap();
    wc.files
        .iter()
        .flat_map(|(_, text)| text.lines())
        .find(|l| *l == token || l.starts_with(&format!("{token} ")))
        .unwrap_or_else(|| panic!("no line for {token}"))
        .to_string()
}

#[test]
fn a_field_with_no_result_is_zero_width_in_offsets() {
    // (operation, the accepted text, whether the field is still there)
    let rows: [(&str, &str, bool); 7] = [
        (r#""op":"delete","start":0,"end":11"#, "", false),
        (
            r#""op":"replace","start":0,"end":11,"with":"New text.""#,
            "New text.",
            false,
        ),
        (r#""op":"delete","start":4,"end":8"#, "Alphta.", false),
        (r#""op":"delete","start":6,"end":7"#, "Alpha eta.", true),
        (r#""op":"delete","start":8,"end":9"#, "Alpha bea.", true),
        (r#""op":"delete","start":10,"end":11"#, "Alpha beta", true),
        (
            r#""op":"replace","find":"beta","with":"gamma""#,
            "Alpha gamma.",
            true,
        ),
    ];
    for token in ["p1", "p2", "p3", "p4"] {
        let doc = document();
        assert_eq!(final_text(&doc, token), "Alpha beta.", "{token}");
        for (op, accepted, field) in rows {
            let mut doc = document();
            if token == "p4" && !field {
                // A range across a simple field is refused whatever its width, so the rows that take the field away do not
                // apply to it.
                let e = fails(&mut doc, &format!(r#"{{"ref":"{token}",{op}}}"#));
                assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{token} {op}: {e}");
                continue;
            }
            run(&mut doc, &format!(r#"{{"ref":"{token}",{op}}}"#));
            run(&mut doc, r#"{"op":"accept","all":true}"#);
            assert_eq!(final_text(&doc, token), accepted, "{token} {op}");
            assert_eq!(
                line(&doc, token).contains("{field"),
                field,
                "{token} {op}: {}",
                line(&doc, token)
            );
        }
        // The offset past the visible text is refused, as on any paragraph of eleven characters.
        let mut doc = document();
        let e = fails(
            &mut doc,
            &format!(r#"{{"op":"delete","ref":"{token}","start":11,"end":12}}"#),
        );
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{token}: {e}");
        assert!(
            e.message().contains("which has 11 characters"),
            "{token}: {e}"
        );
    }
}
