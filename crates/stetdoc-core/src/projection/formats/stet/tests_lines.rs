//! Law F4 (lines): no physical line reaches 2,000 characters, and unwrapping gives back the logical line.

use super::StetFormat;
use super::escape::unescape;
use crate::api::limits::Limits;
use crate::projection::formats::Format;
use crate::projection::model::*;
use crate::projection::testing::*;
use crate::projection::testing_gen_blocks::generate;
use crate::refs::BlockRef;

fn render_body(inlines: Vec<Inline>) -> (String, Vec<crate::projection::formats::Wrap>) {
    let p = files_projection(vec![body_story(vec![Block::Paragraph(para(1, inlines))])]);
    let rendered = StetFormat.render(&p);
    let (_, text) = rendered.files.into_iter().next().unwrap();
    (text, rendered.wraps)
}

/// Joins continuations the way the reader does: independent of the parser.
fn logical(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        let n = line.bytes().rev().take_while(|b| *b == b'\\').count();
        if n % 2 == 1 {
            cur.push_str(&line[..line.len() - 1]);
        } else {
            cur.push_str(line);
            out.push(std::mem::take(&mut cur));
        }
    }
    assert!(cur.is_empty(), "the file ends in a continuation");
    out
}

fn body_lines(text: &str) -> Vec<&str> {
    text.lines()
        .filter(|l| !l.starts_with('{') || l.starts_with("{--"))
        .collect()
}

#[test]
fn a_paragraph_with_no_space_is_broken_at_column_1498() {
    let long = "x".repeat(5000);
    let (text, wraps) = render_body(vec![crate::projection::testing::text(&long)]);
    let lines = body_lines(&text);
    assert!(lines.len() >= 4, "{}", lines.len());
    for line in &lines {
        assert!(line.chars().count() < 2000);
    }
    for line in &lines[..lines.len() - 1] {
        assert!(line.ends_with('\\'));
        assert_eq!(
            line.chars().count(),
            1498 + 1,
            "the break is at the column, and the backslash is one more"
        );
    }
    let joined = logical(&text);
    assert_eq!(joined.last().unwrap(), &format!("p1 {long}"));
    assert_eq!(wraps.len(), lines.len() - 1);
    assert_eq!(wraps[0].slot, 0);
    assert_eq!(
        wraps[0].at,
        1498 - 3,
        "the ref and a space come before the text"
    );
}

#[test]
fn a_paragraph_with_spaces_is_broken_after_the_last_space_in_the_window() {
    let words = "alpha beta gamma delta ".repeat(200);
    let words = words.trim_end().to_string();
    let (text, wraps) = render_body(vec![crate::projection::testing::text(&words)]);
    let lines = body_lines(&text);
    assert!(lines.len() >= 3);
    for line in &lines[..lines.len() - 1] {
        assert!(line.ends_with(" \\"), "{:?}", &line[line.len() - 10..]);
        assert!(line.chars().count() <= 1499 && line.chars().count() >= 1000);
    }
    assert_eq!(logical(&text).last().unwrap(), &format!("p1 {words}"));
    assert!(!wraps.is_empty());
    // the wrap is after the space: the characters before it, unescaped, end in a space
    let first: String = lines[0]
        .strip_prefix("p1 ")
        .unwrap()
        .trim_end_matches('\\')
        .to_string();
    assert_eq!(
        wraps[0].at as usize,
        unescape(&first, false).unwrap().chars().count()
    );
}

#[test]
fn a_break_never_falls_inside_an_escape() {
    let nbsp = "\u{a0}".repeat(1000) + &"{}".repeat(400);
    let (text, _) = render_body(vec![crate::projection::testing::text(&nbsp)]);
    for line in body_lines(&text) {
        assert!(line.chars().count() < 2000);
        let body = line.strip_suffix('\\').unwrap_or(line);
        let body = body.strip_prefix("p1 ").unwrap_or(body);
        // each physical line is a whole number of escapes
        assert!(unescape(body, false).is_ok(), "{line:?}");
    }
    assert_eq!(
        logical(&text).last().unwrap(),
        &format!("p1 {}", super::escape::escape_text(&nbsp))
    );
}

#[test]
fn what_a_marker_shows_is_wrapped_too() {
    let big = "word ".repeat(900);
    let big = big.trim_end().to_string();
    let inlines = vec![
        crate::projection::testing::text("a "),
        del(1, &big),
        Inline::Hidden {
            reason: HiddenReason::Vanish,
            inlines: vec![crate::projection::testing::text(&big)],
        },
        Inline::Atom(Atom::Object(ObjectInfo {
            kind: ObjectKind::TextBox,
            alt: Some(big.as_str().into()),
            text_box: Some(big.as_str().into()),
        })),
        Inline::Field(Field {
            keyword: "REF".into(),
            text: big.as_str().into(),
            width: 10,
            target: BlockRef::parse("p5").ok(),
            opens: true,
            closes: true,
            dirty: false,
        }),
    ];
    let p = files_projection(vec![body_story(vec![Block::Paragraph(para(1, inlines))])]);
    let rendered = StetFormat.render(&p);
    let (_, text) = &rendered.files[0];
    for line in text.lines() {
        assert!(line.chars().count() < 2000, "{}", line.chars().count());
    }
    assert!(text.lines().count() > 10);
    let parsed = StetFormat
        .parse(&rendered.files, &Limits::default())
        .unwrap();
    assert_eq!(parsed, p.draft(&StetFormat.policy()));
}

#[test]
fn a_continuation_inside_a_marker_does_not_hide_the_marker_from_the_reader() {
    // a deleted run of 3000 characters: the wrap falls inside `{--...--}`
    let big = "ab ".repeat(1000);
    let big = big.trim_end().to_string();
    let p = files_projection(vec![body_story(vec![Block::Paragraph(para(
        1,
        vec![
            crate::projection::testing::text("before "),
            del(1, &big),
            crate::projection::testing::text(" after"),
        ],
    ))])]);
    let rendered = StetFormat.render(&p);
    let parsed = StetFormat
        .parse(&rendered.files, &Limits::default())
        .unwrap();
    assert_eq!(parsed, p.draft(&StetFormat.policy()));
}

#[test]
fn no_physical_line_of_a_generated_file_is_too_long_or_ends_in_a_space() {
    let mut longest = 0;
    for seed in 0..120 {
        let p = generate(seed);
        for (name, text) in StetFormat.render(&p).files {
            assert!(!text.contains('\r'), "{name}");
            assert!(text.ends_with('\n'));
            for line in text.lines() {
                let n = line.chars().count();
                longest = longest.max(n);
                assert!(n < 2000, "{name}: {n}");
                assert!(
                    !line.ends_with(' ') && !line.ends_with('\t'),
                    "{name}: {line:?}"
                );
            }
            // unwrapping gives lines that all end without a continuation
            assert!(!logical(&text).is_empty());
        }
    }
    assert!(
        longest > 1400,
        "the generator must make long lines: {longest}"
    );
}
