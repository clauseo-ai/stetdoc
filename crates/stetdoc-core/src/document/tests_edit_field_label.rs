//! A field label that takes no offsets is marked in the view (issue #17, the follow-up in pull request #22). A `MACROBUTTON` or
//! `GOTOBUTTON` with no stored result shows the label its field code holds, which has no characters in the paragraph, and the
//! view prints it as `{field MACROBUTTON "Click here" label}`. The same field with a stored result `Click here` takes ten
//! offsets and prints without the word. A working copy written before the word existed has the field without it, and still
//! packs: the word is not part of the equality of a field's key.

use super::tests_edit::*;
use super::*;
use crate::api::edit::Edit;
use crate::api::view::{FormatId, ViewSpec};
use crate::refs::{hex, sha256};
use stetdoc_testkit::fixtures::parts;
use stetdoc_testkit::fixtures::zip::Zip;

/// The bytes of a document of four paragraphs, each `Alpha {field}beta.`: `p1` a `MACROBUTTON` with no `separate`, `p2` a
/// `GOTOBUTTON` with no `separate`, `p3` a `MACROBUTTON` whose stored result is `Click here`, and `p4` a `DATE` whose stored
/// result is `2026`.
fn package() -> Vec<u8> {
    let run = |t: &str| format!(r#"<w:r><w:t xml:space="preserve">{t}</w:t></w:r>"#);
    let fld = |kind: &str| format!(r#"<w:r><w:fldChar w:fldCharType="{kind}"/></w:r>"#);
    let instr = |code: &str| {
        format!(r#"<w:r><w:instrText xml:space="preserve">{code}</w:instrText></w:r>"#)
    };
    let p = |field: String| format!("<w:p>{}{field}{}</w:p>", run("Alpha "), run("beta."));
    let body = [
        p(fld("begin") + &instr(" MACROBUTTON NoMacro Click here ") + &fld("end")),
        p(fld("begin") + &instr(" GOTOBUTTON Top Go up ") + &fld("end")),
        p(fld("begin")
            + &instr(" MACROBUTTON NoMacro Click here ")
            + &fld("separate")
            + &run("Click here")
            + &fld("end")),
        p(fld("begin")
            + &instr(r#" DATE \@ "yyyy" "#)
            + &fld("separate")
            + &run("2026")
            + &fld("end")),
    ]
    .concat();
    let doc = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

fn open() -> Document {
    Document::open(package(), &OpenOptions::default()).unwrap()
}

/// The paragraph lines of the stet/1 view, without the head, whose base token changes with every edit.
fn body(doc: &Document) -> Vec<String> {
    let view = doc.view(&ViewSpec::default()).unwrap();
    view.text
        .lines()
        .filter(|l| l.starts_with('p'))
        .map(str::to_string)
        .collect()
}

const LINES: [(&str, &str); 4] = [
    (
        "p1",
        r#"p1 Alpha {field MACROBUTTON "Click here" label}beta."#,
    ),
    ("p2", r#"p2 Alpha {field GOTOBUTTON "Go up" label}beta."#),
    ("p3", r#"p3 Alpha {field MACROBUTTON "Click here"}beta."#),
    ("p4", r#"p4 Alpha {field DATE "2026"}beta."#),
];

#[test]
fn a_label_shown_from_the_field_code_is_marked_and_a_stored_result_is_not() {
    let doc = open();
    let want: Vec<String> = LINES.iter().map(|(_, l)| l.to_string()).collect();
    assert_eq!(body(&doc), want);
    // The working copy's file says the same.
    let files = doc.files().unwrap();
    for (_, l) in LINES {
        assert!(files.files[0].1.contains(l), "{}", files.files[0].1);
    }
    // json/1 carries the flag next to the width.
    let spec = ViewSpec {
        format: FormatId::new("json/1"),
        ..ViewSpec::default()
    };
    let json = doc.view(&spec).unwrap().text;
    let compact: String = json.split_whitespace().collect();
    for (keyword, text, width, label) in [
        ("MACROBUTTON", "Clickhere", 0, true),
        ("GOTOBUTTON", "Goup", 0, true),
        ("MACROBUTTON", "Clickhere", 10, false),
        ("DATE", "2026", 4, false),
    ] {
        let member =
            format!(r#""keyword":"{keyword}","text":"{text}","width":{width},"label":{label}"#);
        assert!(compact.contains(&member), "{member} is not in {json}");
    }
}

/// Offsets count a label as nothing and a stored result as its characters: deleting the `b` of `beta` in each paragraph, tracked
/// and untracked. A tracked edit is rejected back to the source and accepted to the intended text, and every result checks clean.
#[test]
fn offsets_count_a_label_as_nothing_and_a_stored_result_as_its_characters() {
    let source = body(&open());
    for (k, (token, start)) in [("p1", 6), ("p2", 6), ("p3", 16), ("p4", 10)]
        .into_iter()
        .enumerate()
    {
        let mut want = source.clone();
        want[k] = want[k].replace("}beta.", "}eta.");
        let ops = format!(
            r#"{{"op":"delete","ref":"{token}","start":{start},"end":{}}}"#,
            start + 1
        );
        let mut untracked = open();
        let b = batch(&untracked, &ops);
        untracked.edit(Edit::ops(b, stamp()).untracked()).unwrap();
        assert_eq!(body(&untracked), want, "{token} untracked");
        assert!(untracked.check().ok, "{token}: {:?}", untracked.check());

        let mut tracked = open();
        run(&mut tracked, &ops);
        assert!(tracked.check().ok, "{token}: {:?}", tracked.check());
        let mut accepted =
            Document::open(tracked.save().unwrap().bytes, &OpenOptions::default()).unwrap();
        run(&mut accepted, r#"{"op":"accept","all":true}"#);
        assert_eq!(body(&accepted), want, "{token} accepted");
        assert!(accepted.check().ok, "{token}: {:?}", accepted.check());
        run(&mut tracked, r#"{"op":"reject","all":true}"#);
        assert_eq!(body(&tracked), source, "{token} rejected");
        assert!(tracked.check().ok, "{token}: {:?}", tracked.check());
    }
}

/// Removes every ` label` word from a stet/1 file: the working copy as 0.9.3 wrote it.
fn as_written_before(text: &str) -> String {
    text.replace(" label}", "}")
}

/// A working copy written before the word existed, and one written now, both pack: an edit of the text beside the field
/// applies, tracked and untracked, accept-all and reject-all give the intended text and the source, and an unedited copy in the
/// old form changes nothing.
#[test]
fn a_working_copy_with_or_without_the_word_packs() {
    let source = body(&open());
    for old in [true, false] {
        for tracked in [true, false] {
            let mut wc = open().export().unwrap();
            let text = &mut wc.files[0].1;
            if old {
                *text = as_written_before(text);
                assert!(!text.contains(" label}"), "{text}");
            }
            for token in ["p1", "p2"] {
                *text = text.replacen(&format!("{token} Alpha "), &format!("{token} Gamma "), 1);
            }
            let pending = wc.pending_files().unwrap().expect("one file is pending");
            let mut resumed = Document::resume(&wc, &OpenOptions::default()).unwrap();
            let edit = Edit::files(pending, stamp());
            let edit = if tracked { edit } else { edit.untracked() };
            resumed
                .edit(edit)
                .unwrap_or_else(|e| panic!("old={old} tracked={tracked}: {e}"));
            assert!(resumed.check().ok, "{:?}", resumed.check());
            let mut want = source.clone();
            want[0] = want[0].replace("Alpha", "Gamma");
            want[1] = want[1].replace("Alpha", "Gamma");
            if tracked {
                let bytes = resumed.save().unwrap().bytes;
                let mut accepted = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
                run(&mut accepted, r#"{"op":"accept","all":true}"#);
                assert_eq!(body(&accepted), want, "old={old} accepted");
                let mut rejected = Document::open(bytes, &OpenOptions::default()).unwrap();
                run(&mut rejected, r#"{"op":"reject","all":true}"#);
                assert_eq!(body(&rejected), source, "old={old} rejected");
            } else {
                assert_eq!(body(&resumed), want, "old={old} untracked");
            }
        }
    }
    // The old form alone is no edit.
    let mut wc = open().export().unwrap();
    wc.files[0].1 = as_written_before(&wc.files[0].1);
    let pending = wc
        .pending_files()
        .unwrap()
        .expect("the file differs from its render");
    let mut resumed = Document::resume(&wc, &OpenOptions::default()).unwrap();
    let receipt = resumed.edit(Edit::files(pending, stamp())).unwrap();
    assert!(receipt.operations.is_empty(), "{}", receipt.text);
    assert_eq!(body(&resumed), source);
}

/// A working copy that 0.9.3 wrote has no `label` word and recorded the hash of that text as the files' rendering. The new
/// build renders the same state with the word, so the recorded hash matches only the rendering as an earlier release made it,
/// and the pack takes that. A rendering that neither this release nor an earlier one wrote is still release skew.
#[test]
fn a_working_copy_that_an_earlier_release_wrote_packs_and_one_that_renders_otherwise_is_stale() {
    for tracked in [true, false] {
        let mut doc = open();
        let mut files = doc.files().unwrap();
        files.files[0].1 = as_written_before(&files.files[0].1);
        files.rendered = vec![(
            "document.md".to_owned(),
            hex(&sha256(files.files[0].1.as_bytes())),
        )];
        files.files[0].1 = files.files[0].1.replacen("p4 Alpha ", "p4 Gamma ", 1);
        let edit = Edit::files(files, stamp());
        let edit = if tracked { edit } else { edit.untracked() };
        let receipt = doc.edit(edit).unwrap();
        assert_eq!(receipt.operations.len(), 1, "{}", receipt.text);
        assert_eq!(receipt.operations[0].op, "replace");
        assert_eq!(final_text(&doc, "p4"), "Gamma 2026beta.");
        assert!(doc.check().ok, "{:?}", doc.check());
        if tracked {
            let bytes = doc.save().unwrap().bytes;
            let mut rejected = Document::open(bytes, &OpenOptions::default()).unwrap();
            run(&mut rejected, r#"{"op":"reject","all":true}"#);
            assert_eq!(body(&rejected), body(&open()));
            run(&mut doc, r#"{"op":"accept","all":true}"#);
            assert_eq!(final_text(&doc, "p4"), "Gamma 2026beta.");
            assert!(doc.check().ok, "{:?}", doc.check());
        }
    }
    let mut other = open();
    let mut files = other.files().unwrap();
    files.rendered = vec![("document.md".to_owned(), hex(&sha256(b"something else")))];
    files.files[0].1 = files.files[0].1.replacen("p4 Alpha ", "p4 Gamma ", 1);
    let e = other.edit(Edit::files(files, stamp())).err().unwrap();
    assert_eq!(e.code(), ErrorCode::StaleFiles, "{}", e.message());
}
