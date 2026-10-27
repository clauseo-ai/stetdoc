//! The scopes of a view: the outline and its sources, find, range, the stories, the appendix and the threads of a
//! document with none.

use super::build::testing::*;
use super::tests_outline::styled;
use super::tests_view::{doc, pages, refs_on, spec};
use super::view::view;
use crate::api::error::ErrorCode;
use crate::api::view::{Budget, Cursor, FormatId, Mode, Scope, StoryName, ViewSpec, WarningCode};

#[test]
fn find_shows_the_blocks_that_hold_the_needle_with_context_gaps_and_counts() {
    let state = doc(40).state();
    let v = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["number 7 has".into()],
                context: 1,
                from: None,
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    assert_eq!(refs_on(&v.text), vec!["p6", "p7", "p8"]);
    assert!(v.text.contains("{~ 1 match}"), "{}", v.text);
    assert_eq!(
        v.position.scope,
        "find \"number 7 has\": 1 match in 1 block"
    );
    let two = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["number 1".into()],
                context: 1,
                from: None,
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    // number 1 and 10 to 19, each with a block on each side: the gap between them is counted.
    assert_eq!(refs_on(&two.text).first().map(String::as_str), Some("p1"));
    assert!(two.text.contains("{~ 6 blocks not shown}"), "{}", two.text);
    // Smart case: a capital makes it exact.
    let loose = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["paragraph number 3 ".into()],
                context: 0,
                from: None,
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    assert_eq!(refs_on(&loose.text), vec!["p3"]);
    let exact = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["paragraph Number 3 ".into()],
                context: 0,
                from: None,
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    assert!(refs_on(&exact.text).is_empty());
    assert_eq!(exact.position.shown, 0);
    let none = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["   ".into()],
                context: 0,
                from: None,
                to: None,
            },
            0,
        ),
    )
    .unwrap_err();
    assert_eq!(none.code(), ErrorCode::InvalidPayload);
}

#[test]
fn find_looks_in_every_story_and_pages_across_them() {
    let body = format!(
        "{}{}",
        px(&format!(
            "{}<w:r><w:footnoteReference w:id=\"1\"/></w:r>",
            r("the price")
        )),
        p("other")
    );
    let f = Fixture::new(&body)
        .footnotes("<w:footnote w:id=\"1\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t> the price excludes taxes</w:t></w:r></w:p></w:footnote>");
    let state = f.state();
    let v = view(
        &state,
        &spec(
            Scope::Find {
                needles: vec!["price".into()],
                context: 0,
                from: None,
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    assert_eq!(refs_on(&v.text), vec!["p1", "p3"]);
    assert_eq!(v.position.scope, "find \"price\": 2 matches in 2 blocks");
}

#[test]
fn a_range_is_the_blocks_from_one_ref_to_another() {
    let state = doc(20).state();
    let v = view(
        &state,
        &spec(
            Scope::Range {
                from: "p5".into(),
                to: Some("p9".into()),
            },
            0,
        ),
    )
    .unwrap();
    assert_eq!(refs_on(&v.text), vec!["p5", "p6", "p7", "p8", "p9"]);
    assert!(v.text.contains("range=p5..p9"), "{}", v.text);
    let open = view(
        &state,
        &spec(
            Scope::Range {
                from: "p18".into(),
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    assert_eq!(refs_on(&open.text), vec!["p18", "p19", "p20"]);
    let back = view(
        &state,
        &spec(
            Scope::Range {
                from: "p9".into(),
                to: Some("p5".into()),
            },
            0,
        ),
    )
    .unwrap_err();
    assert_eq!(back.code(), ErrorCode::InvalidPayload);
    let gone = view(
        &state,
        &spec(
            Scope::Range {
                from: "p99".into(),
                to: None,
            },
            0,
        ),
    )
    .unwrap_err();
    assert_eq!(gone.code(), ErrorCode::NotFound);
    let junk = view(
        &state,
        &spec(
            Scope::Range {
                from: "five".into(),
                to: None,
            },
            0,
        ),
    )
    .unwrap_err();
    assert_eq!(junk.code(), ErrorCode::InvalidPayload);
}

#[test]
fn a_range_that_starts_inside_a_table_of_contents_shows_its_paragraphs() {
    let toc = format!(
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}</w:p>{}{}{}<w:p><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>",
        r("Entry one"),
        p("Entry two"),
        p("Entry three"),
        p("Entry four")
    );
    let state = Fixture::new(&format!("{}{}{}", p("intro"), toc, p("after"))).state();
    let collapsed = view(&state, &spec(Scope::Story(StoryName::Body), 0)).unwrap();
    assert!(collapsed.text.contains("{opaque"), "{}", collapsed.text);
    assert_eq!(
        refs_on(&collapsed.text),
        vec!["p1", "p7"],
        "{}",
        collapsed.text
    );
    let inside = view(
        &state,
        &spec(
            Scope::Range {
                from: "p3".into(),
                to: Some("p4".into()),
            },
            0,
        ),
    )
    .unwrap();
    assert_eq!(refs_on(&inside.text), vec!["p3", "p4"]);
    assert!(inside.text.contains("Entry three"), "{}", inside.text);
}

#[test]
fn the_notes_headers_and_comments_are_stories_a_view_can_ask_for() {
    let f = Fixture::new(&px(&format!("{}<w:r><w:footnoteReference w:id=\"1\"/></w:r>", r("text"))))
        .footnotes("<w:footnote w:id=\"1\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t> a note</w:t></w:r></w:p></w:footnote>")
        .header(1, &p("running head"))
        .sect("<w:headerReference w:type=\"default\" r:id=\"rIdH1\"/>")
        .comments("<w:comment w:id=\"0\" w:author=\"Ann Lee\"><w:p><w:r><w:t>a comment</w:t></w:r></w:p></w:comment>");
    let state = f.state();
    let notes = view(&state, &spec(Scope::Story(StoryName::Notes), 0)).unwrap();
    assert!(
        notes.text.contains("{footnote 1}") && notes.text.contains("a note"),
        "{}",
        notes.text
    );
    let heads = view(&state, &spec(Scope::Story(StoryName::Headers), 0)).unwrap();
    assert!(
        heads.text.contains("{header default sections=1}") || heads.text.contains("running head"),
        "{}",
        heads.text
    );
    let comments = view(&state, &spec(Scope::Story(StoryName::Comments), 0)).unwrap();
    assert!(
        comments.text.contains("{comment c0 by AL}") && comments.text.contains("a comment"),
        "{}",
        comments.text
    );
}

#[test]
fn p6_a_page_cites_its_notes_and_comments_in_an_appendix() {
    let body = format!(
        "{}{}",
        px(&format!(
            "{}<w:commentRangeStart w:id=\"7\"/>{}<w:commentRangeEnd w:id=\"7\"/><w:r><w:footnoteReference w:id=\"2\"/></w:r>",
            r("Pay the "),
            r("Purchase Price")
        )),
        p("a paragraph that cites nothing")
    );
    let f = Fixture::new(&body)
        .footnotes("<w:footnote w:id=\"2\"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t> The price excludes taxes.</w:t></w:r></w:p></w:footnote>")
        .comments("<w:comment w:id=\"7\" w:author=\"Jane Doe\" w:date=\"2026-09-30T10:00:00Z\"><w:p><w:r><w:t>Capitalise?</w:t></w:r></w:p></w:comment>");
    let state = f.state();
    let v = view(&state, &spec(Scope::Story(StoryName::Body), 0)).unwrap();
    assert!(v.text.contains("{appendix}\n{fn1} The price excludes taxes.\n{c7 JD 2026-09-30 on p1 \"Purchase Price\"} Capitalise?"), "{}", v.text);
    let second = view(
        &state,
        &spec(
            Scope::Range {
                from: "p2".into(),
                to: None,
            },
            0,
        ),
    )
    .unwrap();
    assert!(!second.text.contains("{appendix}"), "{}", second.text);
    let off = view(
        &state,
        &ViewSpec {
            notes: false,
            ..spec(Scope::Story(StoryName::Body), 0)
        },
    )
    .unwrap();
    assert!(!off.text.contains("{appendix}"));
}

#[test]
fn an_appendix_entry_is_cut_to_four_hundred_characters_with_a_pointer() {
    let long = "word ".repeat(200);
    let f = Fixture::new(&px(&format!(
        "{}<w:r><w:footnoteReference w:id=\"1\"/></w:r>",
        r("text")
    )))
    .footnotes(&format!(
        "<w:footnote w:id=\"1\"><w:p><w:r><w:t>{long}</w:t></w:r></w:p></w:footnote>"
    ));
    let v = view(&f.state(), &spec(Scope::Story(StoryName::Body), 0)).unwrap();
    let line = v.text.lines().find(|l| l.starts_with("{fn1}")).unwrap();
    assert!(line.ends_with("… {more in notes.md}"), "{line}");
    assert!(line.chars().count() < 460);
}

#[test]
fn a_document_with_no_comments_has_no_threads() {
    let state = doc(2).state();
    let v = view(&state, &spec(Scope::Comments(Default::default()), 0)).unwrap();
    assert!(v.text.contains("{threads: 0 threads}"), "{}", v.text);
    assert!(v.next.is_none());
}

#[test]
fn final_and_original_views_show_no_change_markup() {
    let body = px(&format!(
        "{}{}{}",
        r("The "),
        del(1, "Jane Doe", "Seller"),
        ins(2, "Eric White", "Buyer")
    ));
    let state = Fixture::new(&body).state();
    let redline = view(&state, &spec(Scope::Auto, 0)).unwrap();
    assert!(
        redline.text.contains("{--Seller--}") && redline.text.contains("{++Buyer++}"),
        "{}",
        redline.text
    );
    let fin = view(
        &state,
        &ViewSpec {
            mode: Mode::Final,
            ..spec(Scope::Auto, 0)
        },
    )
    .unwrap();
    assert!(
        fin.text.contains("p1 The Buyer") && !fin.text.contains("{--") && !fin.text.contains("{++"),
        "{}",
        fin.text
    );
    assert!(fin.text.contains("mode=final"));
    let orig = view(
        &state,
        &ViewSpec {
            mode: Mode::Original,
            ..spec(Scope::Auto, 0)
        },
    )
    .unwrap();
    assert!(orig.text.contains("p1 The Seller"), "{}", orig.text);
}

#[test]
fn p8_an_authors_code_is_the_same_on_every_page_and_in_the_files() {
    let paragraph = |i: u32, author: &str| {
        px(&format!(
            "{}{}",
            r(&format!("Paragraph {i} ")),
            ins(i64::from(i), author, "inserted text here")
        ))
    };
    let body: String = (1..=12)
        .map(|i| paragraph(i, if i % 2 == 0 { "Jane Doe" } else { "Eric White" }))
        .collect();
    let state = Fixture::new(&body).state();
    let code_of = |text: &str| -> Vec<(String, String)> {
        text.lines()
            .filter(|l| l.starts_with("{authors:"))
            .flat_map(|l| {
                l.trim_start_matches("{authors:")
                    .trim_end_matches('}')
                    .split(" ·")
                    .map(|a| {
                        let a = a.trim();
                        let (code, name) = a.split_once(' ').unwrap();
                        (name.trim_matches('"').to_string(), code.to_string())
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let m = model(&state, Mode::Redline);
    let file = crate::projection::formats::Format::render(
        &crate::projection::formats::stet::StetFormat,
        &m,
    );
    let in_files = code_of(&file.files[0].1);
    assert_eq!(in_files.len(), 2);
    for v in pages(&state, Scope::Story(StoryName::Body), 200) {
        for (name, code) in code_of(&v.text) {
            assert!(
                in_files.contains(&(name.clone(), code.clone())),
                "{name} is {code} here and not in the files"
            );
        }
    }
}

#[test]
fn json_is_a_format_of_a_view_too() {
    let state = styled().state();
    let json = |scope: Scope| {
        let v = view(
            &state,
            &ViewSpec {
                format: FormatId::new("json/1"),
                ..spec(scope, 0)
            },
        )
        .unwrap();
        assert!(
            v.text.starts_with('{') && v.text.ends_with('}'),
            "{}",
            v.text
        );
        v.text
    };
    assert!(json(Scope::Outline { depth: None }).contains("\"outline\""));
    assert!(json(Scope::Story(StoryName::Body)).contains("\"stories\""));
    let bad = view(
        &state,
        &ViewSpec {
            format: FormatId::new("nope/9"),
            ..spec(Scope::Auto, 0)
        },
    )
    .unwrap_err();
    assert_eq!(bad.code(), ErrorCode::InvalidPayload);
    let _ = (Budget(1), Cursor::new("p1"), WarningCode::ALL);
}
