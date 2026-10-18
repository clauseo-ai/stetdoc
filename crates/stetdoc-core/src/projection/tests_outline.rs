//! The outline of a view: its sources, its depth, its paging, and `Auto`'s choice between text and outline.

use super::build::testing::*;
use super::tests_view::{doc, pages, refs_on, spec};
use super::view::view;
use crate::api::view::Scope;

const HEADINGS: &str = "<w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:pPr><w:outlineLvl w:val=\"0\"/></w:pPr></w:style>\
                        <w:style w:type=\"paragraph\" w:styleId=\"Heading2\"><w:name w:val=\"heading 2\"/><w:pPr><w:outlineLvl w:val=\"1\"/></w:pPr></w:style>";

fn heading(level: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Heading{level}\"/></w:pPr>{}</w:p>",
        r(text)
    )
}

/// Three chapters, each with two sections and some body text.
pub(super) fn styled() -> Fixture {
    let mut body = String::new();
    for c in 1..=3 {
        body.push_str(&heading(1, &format!("Chapter {c}")));
        body.push_str(&p("Some introduction to the chapter."));
        for s in 1..=2 {
            body.push_str(&heading(2, &format!("Section {c}.{s}")));
            body.push_str(&p(
                "Some text under the section, long enough to be counted.",
            ));
            body.push_str(&p("More text under the section."));
        }
    }
    Fixture::new(&body).styles(HEADINGS)
}

#[test]
fn the_outline_of_a_styled_document_lists_its_headings_with_what_each_covers() {
    let state = styled().state();
    let v = view(&state, &spec(Scope::Outline { depth: None }, 0)).unwrap();
    assert!(
        v.text
            .contains("{outline: headings from styles · to level 2 · 9 entries}"),
        "{}",
        v.text
    );
    assert!(v.text.contains("p1 Chapter 1  [8 blocks"), "{}", v.text);
    assert!(v.text.contains("  p3 Section 1.1  [3 blocks"), "{}", v.text);
    assert_eq!(v.position.scope, "outline");
    assert_eq!((v.position.shown, v.position.total), (9, 9));
    assert!(v.text.contains("(9 of 9 entries)"));
    // A depth cap hides the sections and says how many.
    let v = view(&state, &spec(Scope::Outline { depth: Some(1) }, 0)).unwrap();
    assert!(
        v.text.contains("· to level 1 · 3 of 9 entries}"),
        "{}",
        v.text
    );
    assert!(v.text.contains("p1 Chapter 1 (+2 entries)"), "{}", v.text);
    assert!(!v.text.contains("Section 1.1"));
}

#[test]
fn with_no_depth_the_deepest_level_that_fits_is_shown() {
    let state = styled().state();
    let full = view(&state, &spec(Scope::Outline { depth: None }, 0)).unwrap();
    let tight = view(
        &state,
        &spec(Scope::Outline { depth: None }, full.tokens - 5),
    )
    .unwrap();
    assert!(tight.text.contains("· to level 1 ·"), "{}", tight.text);
    assert!(tight.tokens <= full.tokens - 5 || tight.position.shown == 1);
}

#[test]
fn an_outline_too_big_for_one_page_is_paged_by_entries() {
    let state = styled().state();
    let all = pages(&state, Scope::Outline { depth: Some(2) }, 105);
    assert!(all.len() > 1, "{} pages", all.len());
    let mut seen = Vec::new();
    for v in &all {
        assert!(v.tokens <= 105 || v.position.shown == 1, "{}", v.tokens);
        seen.extend(refs_on(&v.text.replace("  ", "")));
    }
    let want = view(&state, &spec(Scope::Outline { depth: Some(2) }, 0)).unwrap();
    assert_eq!(seen, refs_on(&want.text.replace("  ", "")));
}

#[test]
fn a_document_with_no_headings_gets_a_page_index_of_chunks() {
    let state = doc(120).state();
    let v = view(&state, &spec(Scope::Outline { depth: None }, 0)).unwrap();
    assert!(
        v.text
            .contains("{outline: no headings found, so runs of blocks"),
        "{}",
        v.text
    );
    assert!(v.text.contains("p1 Paragraph number 1"), "{}", v.text);
    let entries: Vec<String> = refs_on(&v.text);
    assert!(entries.len() >= 2, "{entries:?}");
}

#[test]
fn bold_lines_that_look_like_headings_are_the_outline_when_nothing_is_styled() {
    let bold = |t: &str| format!("<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>{t}</w:t></w:r></w:p>");
    let body = format!(
        "{}{}{}{}{}",
        bold("ARTICLE I: DEFINITIONS"),
        p("The terms have the meanings given here."),
        bold("Section 1.01 Defined Terms"),
        p("Affiliate means a person under common control."),
        bold("ARTICLE II: PAYMENT")
    );
    let v = view(
        &Fixture::new(&body).state(),
        &spec(Scope::Outline { depth: None }, 0),
    )
    .unwrap();
    assert!(
        v.text
            .contains("{outline: headings from bold lines · to level 2 · 3 entries}"),
        "{}",
        v.text
    );
    assert!(
        v.text.contains("p1 ARTICLE I: DEFINITIONS  [4 blocks"),
        "{}",
        v.text
    );
    assert!(
        v.text
            .contains("  p3 Section 1.01 Defined Terms  [2 blocks"),
        "{}",
        v.text
    );
}

#[test]
fn auto_is_the_text_when_it_fits_and_the_outline_when_it_does_not() {
    let state = styled().state();
    let whole = view(&state, &spec(Scope::Auto, 0)).unwrap();
    assert!(!whole.text.contains("{outline:"));
    let small = view(&state, &spec(Scope::Auto, whole.tokens / 2)).unwrap();
    assert!(
        small.text.contains("{outline: headings from styles"),
        "{}",
        small.text
    );
    let fits = view(&state, &spec(Scope::Auto, whole.tokens + 10)).unwrap();
    assert!(!fits.text.contains("{outline:"), "{}", fits.text);
}

#[test]
fn p5_every_heading_is_in_the_outline_at_its_level_or_counted_under_its_parent() {
    let state = styled().state();
    let at = |depth: u32| {
        view(
            &state,
            &spec(
                Scope::Outline {
                    depth: Some(depth as u8),
                },
                0,
            ),
        )
        .unwrap()
    };
    let shown = |text: &str| {
        text.lines()
            .filter(|l| l.trim_start().starts_with('p'))
            .count()
    };
    let hidden = |text: &str| -> usize {
        text.lines()
            .filter_map(|l| l.split("(+").nth(1))
            .filter_map(|r| r.split(' ').next()?.parse::<usize>().ok())
            .sum()
    };
    let (one, two) = (at(1), at(2));
    assert_eq!(shown(&two.text), 9);
    assert_eq!(shown(&one.text) + hidden(&one.text), 9, "{}", one.text);
    assert_eq!(hidden(&two.text), 0);
}
