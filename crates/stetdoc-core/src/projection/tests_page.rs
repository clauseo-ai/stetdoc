//! The estimate and the search of `page.rs`.

use super::page::{estimate, fit, greedy};
use crate::api::error::StetError;
use crate::api::view::Mode;

#[test]
fn the_estimate_is_a_quarter_of_the_ascii_and_a_token_for_the_rest() {
    assert_eq!(estimate(""), 0);
    assert_eq!(estimate("abcd"), 1);
    assert_eq!(estimate("abcde"), 2);
    assert_eq!(estimate("§"), 1);
    assert_eq!(estimate("abcd§§"), 3);
    assert_eq!(estimate(&"x".repeat(4000)), 1000);
    assert_eq!(estimate("नमस्ते"), 6);
}

fn run(most: usize, budget: u32, guess: usize, cost: impl Fn(usize) -> u32) -> (usize, usize) {
    let mut probes = 0usize;
    let mut tokens = |n: usize| -> Result<u32, StetError> {
        probes += 1;
        Ok(cost(n))
    };
    let n = fit(most, budget, guess, &mut tokens).unwrap();
    (n, probes)
}

#[test]
fn fit_finds_the_largest_page_within_the_budget_from_any_guess() {
    // `n` blocks cost `40 + 7 n` tokens, so with 500 the largest is 65.
    let cost = |n: usize| 40 + 7 * u32::try_from(n).unwrap();
    for guess in [1, 2, 30, 65, 66, 200, 1000] {
        let (n, probes) = run(1000, 500, guess, cost);
        assert_eq!(n, 65, "guess {guess}");
        assert!(probes <= 20, "{probes} probes from {guess}");
    }
}

#[test]
fn fit_always_takes_one_block_and_never_more_than_there_are() {
    assert_eq!(run(10, 5, 4, |n| 100 * u32::try_from(n).unwrap()).0, 1);
    assert_eq!(run(10, 1_000_000, 3, |n| u32::try_from(n).unwrap()).0, 10);
    assert_eq!(run(1, 5, 1, |_| 1_000).0, 1);
    assert_eq!(run(0, 5, 0, |_| 0).0, 0);
    assert_eq!(
        run(7, 0, 3, |_| u32::MAX).0,
        7,
        "a budget of zero has no limit"
    );
}

#[test]
fn fit_searches_a_page_that_is_every_size_in_a_few_probes() {
    for budget in 1..60u32 {
        for most in [1usize, 2, 5, 13, 64] {
            let cost = |n: usize| u32::try_from(n * n).unwrap();
            let (n, _) = run(most, budget, most / 2 + 1, cost);
            let want = (1..=most).rev().find(|k| cost(*k) <= budget).unwrap_or(1);
            assert_eq!(n, want, "budget {budget}, {most} blocks");
        }
    }
}

#[test]
fn the_greedy_guess_takes_blocks_while_their_weights_fit() {
    assert_eq!(greedy([10, 10, 10, 10].into_iter(), 35, 0), 3);
    assert_eq!(greedy([10, 10, 10, 10].into_iter(), 35, 10), 2);
    assert_eq!(greedy([50, 10].into_iter(), 20, 0), 1);
    assert_eq!(greedy([50, 10].into_iter(), 0, 0), 2);
    assert_eq!(greedy(std::iter::empty(), 20, 0), 0);
}

/// P9: the sizes made from the lengths the index read are the sizes the scan counts, in every mode, with the
/// collapsed fields or without them, for a body that has a tracked insertion, a tracked deletion, a paragraph that
/// is only a deletion, a TOC field and a table with an inserted and a deleted row.
#[test]
fn p9_the_sizes_the_index_gives_are_the_sizes_of_the_scan() {
    use super::build::testing::*;
    let row = |kind: &str, id: u32, t: &str| {
        format!(
            "<w:tr><w:trPr><w:{kind} w:id=\"{id}\" w:author=\"JD\" w:date=\"2026-01-01T00:00:00Z\"/></w:trPr><w:tc><w:tcPr/>{}</w:tc></w:tr>",
            p(t)
        )
    };
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>{}{}</w:tbl>",
        p("kept"),
        row("ins", 3, "added"),
        row("del", 4, "removed")
    );
    let toc = format!(
        "{}{}{}",
        px(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC \\o \"1-3\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>1 Heading</w:t></w:r>"
        ),
        p("2 Another"),
        px("<w:r><w:t>3 Last</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"),
    );
    let body = format!(
        "{}{}{}{}{}{table}",
        p("A plain paragraph of some length, with a few words."),
        px(&format!(
            "{}{}{}",
            r("kept "),
            ins(1, "JD", "added "),
            del(2, "JD", "removed ")
        )),
        px(&del(5, "EW", "all of this is deleted")),
        toc,
        p("The last paragraph."),
    );
    let state = Fixture::new(&body).state();
    let parts = state.parts();
    let part = parts.id("word/document.xml").unwrap();
    for mode in [Mode::Final, Mode::Original, Mode::Redline] {
        for expanded in [false, true] {
            let sizes = parts.top_sizes(part, mode, expanded).unwrap();
            let scan = parts.scan(part, mode, expanded).unwrap();
            assert_eq!(
                sizes.as_slice(),
                scan.tops.as_slice(),
                "{mode:?} {expanded}"
            );
            assert!(sizes.iter().any(|s| s.chars > 0), "{mode:?} {expanded}");
        }
    }
}
