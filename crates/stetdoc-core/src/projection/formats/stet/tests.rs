//! The default format on the models of the module: examples, determinism and the round trip.

use super::StetFormat;
use crate::api::limits::Limits;
use crate::projection::draft::Draft;
use crate::projection::formats::Format;
use crate::projection::testing::*;
use crate::projection::testing_gen_blocks::generate;

fn limits() -> Limits {
    Limits::default()
}

/// The first line at which two drafts differ, for a failure message.
fn first_difference(parsed: &Draft, want: &Draft) -> String {
    if parsed.files.len() != want.files.len() {
        return format!("{} files, want {}", parsed.files.len(), want.files.len());
    }
    for (a, b) in parsed.files.iter().zip(&want.files) {
        if a.name != b.name || a.base != b.base || a.common_style != b.common_style {
            return format!(
                "file header: {:?} {:?} {:?} vs {:?} {:?} {:?}",
                a.name, a.base, a.common_style, b.name, b.base, b.common_style
            );
        }
        if a.lines.len() != b.lines.len() {
            return format!(
                "{}: {} lines, want {}",
                a.name,
                a.lines.len(),
                b.lines.len()
            );
        }
        for (i, (x, y)) in a.lines.iter().zip(&b.lines).enumerate() {
            if x != y {
                return format!("{} line index {i}:\n parsed {x:?}\n  want {y:?}", a.name);
            }
        }
    }
    "no difference".to_string()
}

#[test]
fn the_sample_renders_as_the_design_shows_it() {
    let rendered = StetFormat.render(&sample());
    let (name, text) = &rendered.files[0];
    assert_eq!(name, "document.md");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        format!(
            "{{stet 1 file=document.md base={} mode=redline common-style=\"Body Text\"}}",
            base()
        )
    );
    assert!(lines[1].starts_with("{rules: edit the text after a ref"));
    assert_eq!(lines[2], "{authors: JD \"Jane Doe\"}");
    assert_eq!(
        &lines[3..],
        [
            "p1 {style \"Heading 1\"} {§\"1.\"} DEFINITIONS",
            "p2 In this Agreement, {--\"Losses\"--}{#4 JD}{++\"Loss\"++}{#5 JD} means any {fn1}\\tliability",
            "t1 {table cols=2 style=\"Table Grid\"}",
            "r1 {row header}",
            "p3 Item",
            "p4 Price",
            "p5 {+} per unit",
            "{/table}",
            "{section 1 ends: next-page}",
            "p6 {link \"https://example.com\"}the notices page{/link}",
        ]
    );
    let (name, text) = &rendered.files[1];
    assert_eq!(name, "notes.md");
    assert!(text.lines().any(|l| l == "{footnote 1}"));
    assert!(
        text.lines()
            .any(|l| l == "p7 {fn.} The price excludes taxes.")
    );
}

#[test]
fn rendering_is_deterministic() {
    for seed in 0..40 {
        assert_eq!(
            StetFormat.render(&generate(seed)),
            StetFormat.render(&generate(seed)),
            "{seed}"
        );
    }
}

#[test]
fn the_sample_parses_back_to_its_draft() {
    let p = sample();
    let rendered = StetFormat.render(&p);
    let parsed = StetFormat.parse(&rendered.files, &limits()).unwrap();
    assert_eq!(parsed, p.draft(&StetFormat.policy()));
}

#[test]
fn every_generated_model_parses_back_to_its_draft() {
    let policy = StetFormat.policy();
    let mut failures = Vec::new();
    for seed in 0..400 {
        let p = generate(seed);
        let rendered = StetFormat.render(&p);
        match StetFormat.parse(&rendered.files, &limits()) {
            Ok(parsed) => {
                let want: Draft = p.draft(&policy);
                if parsed != want {
                    failures.push(format!("seed {seed}: {}", first_difference(&parsed, &want)));
                }
            }
            Err(e) => failures.push(format!("seed {seed}: {}", e.message())),
        }
    }
    assert!(
        failures.is_empty(),
        "{}",
        failures
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn the_tag_words_the_structure_words_and_the_inline_markers_start_differently() {
    use super::parse_structure::{STRUCTURE_WORDS, TAG_WORDS};
    // the first word of every inline marker, as it is written after the brace
    const INLINE: &[&str] = &[
        "++",
        "--",
        "link",
        "/link",
        "ctl",
        "/ctl",
        "field",
        "hidden",
        "looks-hidden",
        "math",
        "pagebreak",
        "colbreak",
        "page",
        "date",
        "value",
        "img",
        "shape",
        "chart",
        "object",
        "textbox",
    ];
    // a comment or note marker starts with these letters and then a number or a dot
    const NUMBERED: &[&str] = &["c", "/c", "fn", "en"];
    let mut all: Vec<&str> = Vec::new();
    all.extend(TAG_WORDS);
    all.extend(STRUCTURE_WORDS);
    all.extend(INLINE);
    let mut sorted = all.clone();
    sorted.sort_unstable();
    let before = sorted.len();
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        before,
        "a word is both a tag, a structure line and a marker: {all:?}"
    );
    for word in all {
        for prefix in NUMBERED {
            let rest = word.strip_prefix(prefix);
            assert!(
                rest.is_none_or(|r| !(r.is_empty()
                    || r.starts_with('.')
                    || r.bytes().all(|b| b.is_ascii_digit()))),
                "{word} reads as a numbered marker"
            );
        }
    }
}
