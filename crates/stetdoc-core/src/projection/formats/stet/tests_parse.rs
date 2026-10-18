//! Laws F5 (edited input), F7 (messages) and F8 (totality) for the reader of `stet/1`.

use super::StetFormat;
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;
use crate::projection::draft::{Draft, DraftLine};
use crate::projection::formats::Format;
use crate::projection::keys::Key;
use crate::projection::testing::*;
use crate::projection::testing_gen::Rng;
use crate::projection::testing_gen_blocks::generate;

fn parse(name: &str, text: &str) -> Result<Draft, crate::api::error::StetError> {
    StetFormat.parse(&[(name.to_string(), text.to_string())], &Limits::default())
}

fn doc() -> String {
    StetFormat.render(&sample()).files.remove(0).1
}

fn draft() -> Draft {
    let p = sample();
    let mut d = p.draft(&StetFormat.policy());
    d.files.truncate(1);
    d
}

fn assert_same(text: &str) {
    assert_eq!(parse("document.md", text).unwrap(), draft());
}

// F5: the five tolerated changes parse to the same draft as the unedited file.

#[test]
fn trailing_spaces_and_tabs_are_not_an_edit() {
    let text: String = doc().lines().map(|l| format!("{l}  \t \n")).collect();
    assert_same(&text);
}

#[test]
fn blank_lines_and_the_final_newline_are_not_an_edit() {
    let mut text = String::from("\n   \n");
    for line in doc().lines() {
        text.push_str(line);
        text.push_str("\n\n \t\n");
    }
    assert_same(&text);
    assert_same(doc().trim_end_matches('\n'));
    assert_same(&format!("{}\n\n\n", doc()));
}

#[test]
fn crlf_is_accepted() {
    assert_same(&doc().replace('\n', "\r\n"));
}

#[test]
fn a_byte_order_mark_is_dropped() {
    assert_same(&format!("\u{feff}{}", doc()));
}

#[test]
fn the_head_block_after_the_header_is_ignored() {
    let text = doc();
    let mut lines: Vec<&str> = text.lines().collect();
    lines.remove(1); // the legend
    let edited = format!(
        "{}\n{{warning looks_hidden p9: anything {{ at all\n{{summary: nothing\n{{rules: changed\n{}\n",
        lines[0],
        lines[1..].join("\n")
    );
    assert_same(&edited);
}

// F5: a generated edit parses to the expected change of the draft.

fn replace_line(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "{from}");
    text.replacen(from, to, 1)
}

#[test]
fn a_changed_slot_is_a_changed_slot_and_nothing_else() {
    let edited = parse(
        "document.md",
        &replace_line(&doc(), "means any {fn1}", "means each {fn1}"),
    )
    .unwrap();
    let want = draft();
    let (DraftLine::Block(a), DraftLine::Block(b)) =
        (&edited.files[0].lines[1], &want.files[0].lines[1])
    else {
        panic!("blocks")
    };
    assert_eq!(a.tags, b.tags);
    assert_eq!(a.seams, b.seams);
    assert_eq!(a.texts[3], " means each ");
    assert_eq!(b.texts[3], " means any ");
    let mut others = edited.clone();
    others.files[0].lines[1] = want.files[0].lines[1].clone();
    assert_eq!(others, want);
}

#[test]
fn a_removed_line_an_added_line_and_a_changed_tag_are_what_they_are() {
    let text = doc();
    let removed = parse(
        "document.md",
        &text.replace(
            "p6 {link \"https://example.com\"}the notices page{/link}\n",
            "",
        ),
    )
    .unwrap();
    assert_eq!(
        removed.files[0].lines.len(),
        draft().files[0].lines.len() - 1
    );
    let added = parse(
        "document.md",
        &replace_line(&text, "p1 {style", "A new line\np1 {style"),
    )
    .unwrap();
    let DraftLine::Block(new) = &added.files[0].lines[0] else {
        panic!()
    };
    assert_eq!(new.r#ref, None);
    assert_eq!(new.texts, vec!["A new line"]);
    assert_eq!(
        new.line, 4,
        "the line in the file, after the header, the legend and the authors"
    );
    let retagged = parse(
        "document.md",
        &replace_line(&text, "{style \"Heading 1\"}", "{style \"Heading 2\"}"),
    )
    .unwrap();
    let DraftLine::Block(b) = &retagged.files[0].lines[0] else {
        panic!()
    };
    assert_eq!(b.tags[0], Key::Style("Heading 2".into()));
    let reseamed = parse(
        "document.md",
        &replace_line(&text, "{--\"Losses\"--}{#4 JD}", "{--\"Losses\"--}{#9 JD}"),
    )
    .unwrap();
    assert_ne!(reseamed, draft());
}

#[test]
fn a_duplicated_ref_and_a_reordered_pair_parse_and_are_left_to_the_alignment() {
    let text = doc();
    assert!(parse("document.md", &format!("{text}p1 again\n")).is_ok());
    let swapped = replace_line(
        &replace_line(&text, "p3 Item", "pXX"),
        "p4 Price",
        "p3 Item\np4 Price",
    );
    assert!(parse("document.md", &swapped.replace("pXX", "p4 Price")).is_ok());
}

#[test]
fn a_line_with_no_ref_may_use_markers_because_a_split_may_need_them() {
    let d = parse(
        "document.md",
        &replace_line(
            &doc(),
            "p1 {style",
            "{--\"x\"--}{#3 EW}text {fn2}\np1 {style",
        ),
    )
    .unwrap();
    let DraftLine::Block(new) = &d.files[0].lines[0] else {
        panic!()
    };
    assert_eq!(new.r#ref, None);
    assert_eq!(new.seams.len(), 2);
}

// F7: every error names the file, the line and the way out.

fn error_of(name: &str, text: &str) -> crate::api::error::StetError {
    parse(name, text).expect_err("must be refused")
}

fn with_line(line: &str) -> String {
    let doc = doc();
    let mut lines: Vec<&str> = doc.lines().collect();
    lines.push(line);
    lines.join("\n")
}

#[test]
fn every_syntax_error_is_invalid_payload_and_says_where() {
    let cases: &[(&str, &str)] = &[
        ("p9 a \\q b", "not an escape"),
        ("p9 ends with a backslash \\\\\\", "continuation"),
        ("p9 \\u{zz} x", "hexadecimal"),
        ("p9 {++never closed", "split"),
        ("p9 {--deleted--} no tag", "tag"),
        ("p9 {--deleted--}{#x JD}", "change id"),
        ("p9 {nonsense} here", "not a marker"),
        ("p9 a } b", "does not end a marker"),
        ("p9 {++a++}{#1 JD move to #2}", "move from"),
        ("p9 {--a--}{#1 JD move from #2}", "move to"),
        ("p9 {link \"u\"}never closed", "split"),
        ("p9 {/link} stray", "no {link"),
        ("p9 {ctl wat}x{/ctl}", "kind of control"),
        ("p9 {field REF}", "quoted"),
        ("p9 {hidden text}", "quoted"),
        ("p9 {looks-hidden pink \"x\"}", "white"),
        ("p9 {c3 extra}", "unexpected"),
        ("p9 {fn3x}", "not a marker"),
        ("p07 text", "not a ref"),
        ("p0 text", "not a ref"),
        ("p9{style \"x\"}", "not a marker"),
        ("p9 {style \"x\"}text", "one space"),
        ("p9 {style x}", "quotes"),
        ("p9 {span two}", "columns"),
        ("t9", "table"),
        ("t9 {row}", "table"),
        ("r9 {table cols=1}", "row"),
        ("t9 {table}", "cols"),
        ("{section x ends: next-page}", "number"),
        ("{section 2 ends: sideways}", "section break"),
        ("{opaque}", "not shown"),
        ("{opaque toc 5}", "paragraphs"),
        ("{header sometimes sections=1}", "default"),
        ("{comment c3 by}", "author"),
        ("{reply c4}", "answers"),
        ("{rules: after the head block}", "head block"),
        ("{table cols=1} trailing", "only its marker"),
    ];
    for (line, why) in cases {
        let text = with_line(line);
        let error = parse("document.md", &text).expect_err(line);
        assert_eq!(
            error.code(),
            ErrorCode::InvalidPayload,
            "{line}: {}",
            error.message()
        );
        let message = error.message();
        assert!(message.contains("document.md line "), "{line}: {message}");
        assert!(
            message.to_lowercase().contains(why),
            "{line} should mention {why:?}: {message}"
        );
        assert!(!error.next().is_empty());
    }
}

#[test]
fn the_line_in_the_message_is_the_line_where_the_logical_line_starts() {
    let doc = doc();
    let count = doc.lines().count();
    let text = format!("{doc}p9 a long line \\\nthat goes on \\\nand has a bad \\q escape\n");
    let error = error_of("document.md", &text);
    assert!(
        error.message().contains(&format!("line {}", count + 1)),
        "{}",
        error.message()
    );
}

#[test]
fn a_header_that_is_missing_or_wrong_is_refused() {
    let doc = doc();
    let body: Vec<&str> = doc.lines().skip(1).collect();
    assert_eq!(
        error_of("document.md", "").code(),
        ErrorCode::InvalidPayload
    );
    assert_eq!(
        error_of("document.md", &body.join("\n")).code(),
        ErrorCode::InvalidPayload
    );
    let swapped = doc.replacen("file=document.md", "file=notes.md", 1);
    let e = error_of("document.md", &swapped);
    assert!(
        e.message().contains("notes.md") && e.message().contains("swapped"),
        "{}",
        e.message()
    );
    let future = doc.replacen("{stet 1 ", "{stet 2 ", 1);
    let e = error_of("document.md", &future);
    assert_eq!(e.code(), ErrorCode::UnsupportedVersion);
    assert!(e.message().contains("stet/2"));
    let final_mode = doc.replacen("mode=redline", "mode=final", 1);
    assert!(
        error_of("document.md", &final_mode)
            .message()
            .contains("redline")
    );
    let view = doc.replacen("file=document.md", "view=body", 1);
    assert!(error_of("document.md", &view).message().contains("view"));
    let nobase = doc.replacen(&format!("base={} ", base()), "", 1);
    assert!(error_of("document.md", &nobase).message().contains("base="));
    let unknown = doc.replacen("mode=redline", "mode=redline colour=red", 1);
    assert!(
        error_of("document.md", &unknown)
            .message()
            .contains("colour=")
    );
}

#[test]
fn a_lone_carriage_return_and_a_file_that_ends_in_a_continuation_are_refused() {
    let e = error_of("document.md", &format!("{}p9 a\rb\n", doc()));
    assert!(e.message().contains("carriage return"), "{}", e.message());
    // The file and the line are in the error's fields too, not only in its message.
    assert_eq!(e.file(), Some("document.md"));
    let line = e.line().expect("a line");
    assert!(
        e.message()
            .starts_with(&format!("document.md line {line}: ")),
        "{}",
        e.message()
    );
    let e = error_of("document.md", &format!("{}p9 never ends \\", doc()));
    assert!(e.message().contains("continuation"), "{}", e.message());
}

#[test]
fn a_file_that_is_not_the_formats_is_ignored() {
    let d = StetFormat
        .parse(
            &[("readme.txt".to_string(), "not stet at all".to_string())],
            &Limits::default(),
        )
        .unwrap();
    assert!(d.files.is_empty());
}

#[test]
fn limits_are_loud_and_name_the_limit() {
    let deep = format!("{}p9 {}", doc(), "{++".repeat(40));
    let e = error_of("document.md", &deep);
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(e.message().contains("32"), "{}", e.message());
    let big = "x".repeat(70_000);
    let e = error_of(
        "document.md",
        &format!("{}p9 {{--{big}--}}{{#1 JD}}\n", doc()),
    );
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    let e = error_of(
        "document.md",
        &format!("{}p9 {{hidden \"{big}\"}}\n", doc()),
    );
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(e.message().contains("65536"), "{}", e.message());
    let tiny = Limits {
        max_entry_bytes: 50,
        ..Limits::default()
    };
    let e = StetFormat
        .parse(&[("document.md".to_string(), doc())], &tiny)
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    let few = Limits {
        max_xml_nodes: 3,
        ..Limits::default()
    };
    let e = StetFormat
        .parse(&[("document.md".to_string(), doc())], &few)
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
}

// F8: totality.

#[test]
fn arbitrary_and_mutated_files_give_ok_or_an_error() {
    let mut rng = Rng::new(99);
    let pieces = [
        "{", "}", "\\", "\"", "{++", "++}", "--}", "{--", "p1 ", "\n", "\r", " ", "\t", "{#1 JD}",
        "{field", "=", "→",
    ];
    let mut outcomes = [0usize; 2];
    for seed in 0..300u64 {
        let rendered = StetFormat.render(&generate(seed));
        for (name, text) in rendered.files {
            let mut bytes: Vec<char> = text.chars().collect();
            for _ in 0..1 + rng.below(6) {
                let at = rng.below(bytes.len().max(1));
                match rng.below(4) {
                    0 => {
                        if at < bytes.len() {
                            bytes.remove(at);
                        }
                    }
                    1 => {
                        for c in rng.word(&pieces).chars().rev() {
                            bytes.insert(at, c);
                        }
                    }
                    2 => bytes.truncate(at),
                    _ => {
                        let dup: Vec<char> = bytes.iter().skip(at).take(40).copied().collect();
                        for (i, c) in dup.into_iter().enumerate() {
                            bytes.insert(at + i, c);
                        }
                    }
                }
            }
            let mutated: String = bytes.into_iter().collect();
            match StetFormat.parse(&[(name, mutated)], &Limits::default()) {
                Ok(_) => outcomes[0] += 1,
                Err(_) => outcomes[1] += 1,
            }
        }
    }
    assert!(outcomes[0] > 0 && outcomes[1] > 0, "{outcomes:?}");
}

#[test]
fn hostile_inputs_are_refused_or_read_in_linear_time() {
    let header = doc().lines().next().unwrap().to_string();
    let hostile = [
        "{".repeat(200_000),
        "}".repeat(200_000),
        "\\".repeat(200_001),
        "\\".repeat(200_000),
        "{++".repeat(100_000),
        "{--".repeat(100_000),
        "p1 ".repeat(100_000),
        "{field REF \"".repeat(50_000),
        "\"".repeat(200_000),
        "{span ".repeat(50_000),
        "a\\\n".repeat(100_000),
        "\n".repeat(300_000),
        "{c1}".repeat(50_000),
    ];
    for body in hostile {
        let _ = parse("document.md", &format!("{header}\n{body}"));
    }
}
