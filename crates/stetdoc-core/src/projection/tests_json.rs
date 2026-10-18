//! The `json/1` adapter: its values, its shape, and the round trip of every model.

use super::draft::Policy;
use super::formats::json::JsonFormat;
use super::formats::json_value::{FromJson, Json, ToJson};
use super::formats::{Format, by_id, registry};
use super::model::*;
use super::testing::*;
use super::testing_gen_blocks::generate;
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;

#[test]
fn json_values_round_trip_and_refuse_what_the_model_never_writes() {
    let value = Json::Obj(vec![
        (
            "a".to_string(),
            Json::Arr(vec![Json::Int(-5), Json::Null, Json::Bool(true)]),
        ),
        (
            "b\"\\".to_string(),
            Json::Str("tab\t nl\n cr\r nul\u{0} del\u{7f} é \u{1f600} \u{2028}".to_string()),
        ),
    ]);
    let mut text = String::new();
    value.write(&mut text);
    assert_eq!(Json::parse(&text, 8).unwrap(), value);
    assert!(Json::parse("1.5", 8).is_err(), "no fractions");
    assert!(Json::parse("1e3", 8).is_err(), "no exponents");
    assert!(Json::parse("[1] x", 8).is_err(), "text after the value");
    assert!(
        Json::parse("\"a\nb\"", 8).is_err(),
        "a raw line feed in a string"
    );
    assert!(Json::parse("\"\\ud800\"", 8).is_err(), "a lone surrogate");
    assert!(Json::parse("\"\\q\"", 8).is_err());
    assert!(
        Json::parse("99999999999999999999", 8).is_err(),
        "out of range"
    );
    assert_eq!(
        Json::parse("\"\\ud83d\\ude00\"", 8).unwrap(),
        Json::Str("\u{1f600}".to_string())
    );
    let deep = "[".repeat(100) + &"]".repeat(100);
    assert!(Json::parse(&deep, 20).is_err(), "nested too deeply");
    assert!(Json::parse(&deep, 200).is_ok());
}

#[test]
fn a_generated_model_is_the_same_after_json() {
    for seed in 0..150 {
        let p = generate(seed);
        let json = p.to_json();
        let mut text = String::new();
        json.write(&mut text);
        let back = Projection::from_json(&Json::parse(&text, 192).unwrap()).unwrap();
        assert_eq!(back, p, "seed {seed}");
    }
}

#[test]
fn the_json_format_parses_to_the_draft_of_the_model() {
    let format = JsonFormat;
    let policy: Policy = format.policy();
    let limits = Limits::default();
    for seed in 0..150 {
        let p = generate(seed);
        let rendered = format.render(&p);
        let parsed = format.parse(&rendered.files, &limits).unwrap();
        assert_eq!(parsed, p.draft(&policy), "seed {seed}");
        assert!(rendered.wraps.is_empty());
    }
    let p = sample();
    let rendered = format.render(&p);
    let names: Vec<&str> = rendered.files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["document.json", "notes.json"]);
    assert_eq!(
        format.parse(&rendered.files, &limits).unwrap(),
        p.draft(&policy)
    );
}

#[test]
fn rendering_is_deterministic() {
    let format = JsonFormat;
    assert_eq!(format.render(&generate(5)), format.render(&generate(5)));
}

#[test]
fn an_edit_to_the_model_text_is_an_edit_of_the_draft() {
    let format = JsonFormat;
    let policy = format.policy();
    let p = sample();
    let mut edited = p.clone();
    let Body::Stories(stories) = &mut edited.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.inlines = vec![text("DEFINITIONS AND INTERPRETATION")];
    }
    let rendered = format.render(&edited);
    let parsed = format.parse(&rendered.files, &Limits::default()).unwrap();
    assert_ne!(parsed, p.draft(&policy));
    assert_eq!(parsed, edited.draft(&policy));
}

#[test]
fn a_file_that_is_not_json_1_is_invalid_payload_with_its_name() {
    let format = JsonFormat;
    let limits = Limits::default();
    let good = format.render(&sample());
    let text = good.files[0].1.clone();
    let name = good.files[0].0.clone();
    let cases = [
        ("not json".to_string(), "not a JSON value"),
        (
            text.replacen("{\"head\"", "{\"extra\":1,\"head\"", 1),
            "not known",
        ),
        (text.replacen("\"mode\":\"redline\",", "", 1), "missing"),
        (text.replacen("\"redline\"", "\"sideways\"", 1), "mode"),
        (format!("{text} x"), "after the end"),
    ];
    for (bad, why) in cases {
        let error = format
            .parse(&[(name.clone(), bad)], &limits)
            .expect_err(why);
        assert_eq!(error.code(), ErrorCode::InvalidPayload);
        assert!(error.message().contains(&name), "{}", error.message());
        assert!(error.message().contains(why), "{why}: {}", error.message());
    }
}

#[test]
fn a_file_over_the_limit_is_refused_and_other_names_are_ignored() {
    let format = JsonFormat;
    let good = format.render(&sample());
    let limits = Limits {
        max_entry_bytes: 100,
        ..Limits::default()
    };
    let error = format.parse(&good.files, &limits).unwrap_err();
    assert_eq!(error.code(), ErrorCode::LimitExceeded);
    let ignored = format
        .parse(
            &[("notes.txt".to_string(), "anything".to_string())],
            &Limits::default(),
        )
        .unwrap();
    assert!(ignored.files.is_empty());
}

#[test]
fn the_registry_has_the_json_format_and_an_unknown_id_is_invalid_payload() {
    assert_eq!(by_id("json/1").unwrap().id(), "json/1");
    assert_eq!(
        by_id("stet/9").err().unwrap().code(),
        ErrorCode::InvalidPayload
    );
    let ids: Vec<&str> = registry().iter().map(|f| f.id()).collect();
    assert!(ids.contains(&"json/1"));
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "ids are unique");
}

#[test]
fn the_parts_of_a_view_are_the_same_after_json() {
    use crate::refs::{BlockRef, RefKind};
    let r = |kind, n| BlockRef::new(kind, n).unwrap();
    let mut p = files_projection(vec![body_story(vec![
        plain(1, "a line"),
        Block::Margin(Margin::Matches(2)),
        Block::Margin(Margin::Gap(7)),
        Block::Margin(Margin::Continued {
            table: r(RefKind::Table, 3),
            from: r(RefKind::Row, 12),
        }),
    ])]);
    p.appendix = vec![
        AppendixEntry::Note {
            kind: NoteKind::Endnote,
            ordinal: 2,
            text: "an endnote".into(),
            cut: true,
        },
        AppendixEntry::Comment {
            id: 3,
            author: Some(1),
            date: Some("2026-09-30T10:00:00Z".into()),
            on: Some(r(RefKind::Paragraph, 9)),
            anchored: Some("two words".into()),
            text: "a comment".into(),
            cut: false,
        },
    ];
    let mut text = String::new();
    p.to_json().write(&mut text);
    assert_eq!(
        Projection::from_json(&Json::parse(&text, 192).unwrap()).unwrap(),
        p
    );
    p.body = Body::Outline(Outline {
        source: OutlineSource::BoldLines,
        depth: 2,
        total: 9,
        entries: vec![OutlineEntry {
            r#ref: Some(r(RefKind::Paragraph, 1)),
            level: 1,
            label: Some("1.".into()),
            text: "ARTICLE I".into(),
            blocks: 4,
            chars: 812,
            first: Some(r(RefKind::Paragraph, 1)),
            last: Some(r(RefKind::Paragraph, 4)),
            hidden: 2,
        }],
    });
    let mut text = String::new();
    p.to_json().write(&mut text);
    assert_eq!(
        Projection::from_json(&Json::parse(&text, 192).unwrap()).unwrap(),
        p
    );
}
