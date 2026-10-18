//! Law T4: one test for each row of the table of design 6.5, with what the message says.

use super::testing::*;
use crate::api::error::{ErrorCode, StetError};
use crate::projection::formats::Format;
use crate::projection::formats::stet::StetFormat;
use crate::projection::model::*;
use crate::projection::testing::*;
use crate::wordml::StoryKind;

fn with_notes_and_comments() -> Projection {
    let mut p = sample();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    stories.push(Story {
        kind: StoryKind::Comments,
        part: "word/comments.xml".into(),
        uses: Vec::new(),
        units: vec![Unit {
            kind: UnitKind::Comment,
            id: Some(3),
            ordinal: 0,
            mark: None,
            meta: UnitMeta {
                author: Some(0),
                ..UnitMeta::default()
            },
            blocks: vec![Block::Paragraph(Paragraph {
                r#ref: None,
                ..para(1, vec![text("Capitalise?")])
            })],
        }],
    });
    p
}

fn edit_file(
    p: &Projection,
    file: &str,
    edit: impl Fn(&str) -> String,
) -> Result<super::Plan, StetError> {
    plan_with(p, &StetFormat, &styles(), |name, text| {
        if name == file {
            edit(text)
        } else {
            text.to_string()
        }
    })
}

/// A row of the table: the file, the edit, the code, and words the message must hold.
struct Row {
    what: &'static str,
    file: &'static str,
    edit: fn(&str) -> String,
    code: ErrorCode,
    says: &'static [&'static str],
}

fn rows() -> Vec<Row> {
    vec![
        Row {
            what: "a duplicated ref",
            file: "document.md",
            edit: |t| format!("{t}p2 again\n"),
            code: ErrorCode::InvalidPayload,
            says: &["document.md", "p2 is on lines", "no ref"],
        },
        Row {
            what: "a ref that belongs to another file",
            file: "document.md",
            edit: |t| format!("{t}p7 again\n"),
            code: ErrorCode::InvalidPayload,
            says: &["p7 belongs to notes.md"],
        },
        Row {
            what: "an unbalanced table",
            file: "document.md",
            edit: |t| t.replacen("{/table}\n", "", 1),
            code: ErrorCode::InvalidPayload,
            says: &["end of a table"],
        },
        Row {
            what: "a tag or a marker on a new line",
            file: "document.md",
            edit: |t| format!("{t}text {{fn5}}\n"),
            code: ErrorCode::InvalidPayload,
            says: &["text only", "Markers belong to the document"],
        },
        Row {
            what: "a ref that never existed",
            file: "document.md",
            edit: |t| format!("{t}p77 text\n"),
            code: ErrorCode::NotFound,
            says: &["p77", "never"],
        },
        Row {
            what: "a ref that is gone",
            file: "document.md",
            edit: |t| format!("{t}p977 text\n"),
            code: ErrorCode::StaleRef,
            says: &["p977 is gone", "merged away"],
        },
        Row {
            what: "a changed common style in the head",
            file: "document.md",
            edit: |t| {
                t.replacen(
                    "common-style=\"Body Text\"",
                    "common-style=\"Heading 1\"",
                    1,
                )
            },
            code: ErrorCode::ReadonlyRegion,
            says: &["document.md", "Heading 1", "Body Text", "read-only"],
        },
        Row {
            what: "a removed common style in the head",
            file: "document.md",
            edit: |t| t.replacen(" common-style=\"Body Text\"", "", 1),
            code: ErrorCode::ReadonlyRegion,
            says: &["no common-style", "Body Text"],
        },
        Row {
            what: "a changed tag",
            file: "document.md",
            edit: |t| t.replacen("{§\"1.\"}", "{§\"2.\"}", 1),
            code: ErrorCode::ReadonlyRegion,
            says: &["p1", "number", "read-only"],
        },
        Row {
            what: "a changed seam",
            file: "document.md",
            edit: |t| t.replacen("{++\"Loss\"++}{#5 JD}", "{++\"Loss\"++}{#6 JD}", 1),
            code: ErrorCode::ReadonlyRegion,
            says: &["p2", "insertion", "read-only"],
        },
        Row {
            what: "a removed structure line",
            file: "document.md",
            edit: |t| t.replacen("{section 1 ends: next-page}\n", "", 1),
            code: ErrorCode::ReadonlyRegion,
            says: &["end of section 1", "removed"],
        },
        Row {
            what: "a changed line of comments.md",
            file: "comments.md",
            edit: |t| t.replacen("Capitalise?", "Capitalize?", 1),
            code: ErrorCode::ReadonlyRegion,
            says: &["comments.md is read-only"],
        },
        Row {
            what: "a moved block",
            file: "document.md",
            edit: |t| {
                let t = t.replacen(
                    "p6 {link \"https://example.com\"}the notices page{/link}\n",
                    "",
                    1,
                );
                t.replacen(
                    "p1 {style",
                    "p6 {link \"https://example.com\"}the notices page{/link}\np1 {style",
                    1,
                )
            },
            code: ErrorCode::Unsupported,
            says: &["cannot be moved"],
        },
        Row {
            what: "a new table",
            file: "document.md",
            edit: |t| t.replacen("{/table}\n", "{/table}\n{table cols=1}\n", 1),
            code: ErrorCode::Unsupported,
            says: &["cannot be created"],
        },
        Row {
            what: "a removed note",
            file: "notes.md",
            edit: |t| t.replacen("{footnote 1}\np7 {fn.} The price excludes taxes.\n", "", 1),
            code: ErrorCode::Unsupported,
            says: &["footnote 1", "is not removed"],
        },
        Row {
            what: "a new line whose anchor is ambiguous",
            file: "document.md",
            edit: |t| t.replacen("p5 {+} per unit\n", "p5 {+} per unit\nA line\n", 1),
            code: ErrorCode::Unsupported,
            says: &["ambiguous", "insert"],
        },
    ]
}

#[test]
fn every_row_of_the_table_is_refused_with_its_code_and_a_message_that_says_why() {
    let p = with_notes_and_comments();
    for row in rows() {
        let e = edit_file(&p, row.file, row.edit).expect_err(row.what);
        assert_eq!(e.code(), row.code, "{}: {}", row.what, e.message());
        for word in row.says {
            assert!(
                e.message().contains(word),
                "{}: the message must say {word:?}:\n{}",
                row.what,
                e.message()
            );
        }
        assert!(
            !e.next().is_empty(),
            "{}: the error must say what to do",
            row.what
        );
        // The file, and the line when there is one, are in the error's own fields, as the message says them.
        assert_eq!(e.file(), Some(row.file), "{}: {}", row.what, e.message());
        if let Some(line) = e.line() {
            assert!(
                e.message().contains(&format!("line {line}"))
                    || e.message().contains(&format!("and {line}")),
                "{}: line {line} is not the message's:\n{}",
                row.what,
                e.message()
            );
        }
        if row.what == "a removed note" {
            assert!(
                e.next().contains("delete-note") && e.next().contains("`footnote`"),
                "{}",
                e.next()
            );
        }
    }
}

#[test]
fn the_files_of_another_version_and_a_file_the_document_does_not_have_have_their_own_codes() {
    let p = sample();
    let format = StetFormat;
    let baseline = baseline_of(&p, &format);
    let mut draft = format
        .parse(
            &format.render(&p).files,
            &crate::api::limits::Limits::default(),
        )
        .unwrap();
    draft.files[0].base = Some("0000000000000000:3:0000000000000000".to_string());
    let e = super::align(&baseline, &draft, &styles()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleFiles);
    assert!(
        e.message().contains("document.md") && e.next().contains("Export"),
        "{}",
        e.message()
    );
    let mut p = sample();
    let Body::Stories(stories) = &mut p.body else {
        panic!("a model of files has stories");
    };
    stories.retain(|s| s.kind != StoryKind::Footnotes);
    let baseline = baseline_of(&p, &format);
    let files = vec![(
        "notes.md".to_string(),
        format.render(&sample()).files.remove(1).1,
    )];
    let draft = format
        .parse(&files, &crate::api::limits::Limits::default())
        .unwrap();
    let e = super::align(&baseline, &draft, &styles()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::Unsupported);
    assert!(
        e.message().contains("notes.md does not exist"),
        "{}",
        e.message()
    );
}

#[test]
fn a_file_given_twice_is_refused_and_so_is_a_head_that_differs_even_when_the_lines_are_the_same() {
    let p = sample();
    let format = StetFormat;
    let baseline = baseline_of(&p, &format);
    let mut draft = format
        .parse(
            &format.render(&p).files,
            &crate::api::limits::Limits::default(),
        )
        .unwrap();
    let again = draft.files[0].clone();
    draft.files.push(again);
    let e = super::align(&baseline, &draft, &styles()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(
        e.message().contains("document.md was given twice"),
        "{}",
        e.message()
    );
    draft.files.pop();
    draft.files[0].common_style = Some("Heading 1".into());
    let e = super::align(&baseline, &draft, &styles()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::ReadonlyRegion);
}
