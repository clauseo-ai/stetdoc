//! Tests for what tracked changes do to numbering, by mode, and the random totality test. Each test says what it proves.

use super::tests_support::*;
use crate::api::view::Mode::{Final, Original, Redline};

fn deleted_mark(num_id: i64) -> String {
    pp(
        &format!(
            "<w:numPr><w:numId w:val=\"{num_id}\"/></w:numPr>\
             <w:rPr><w:del w:id=\"1\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"/></w:rPr>"
        ),
        "",
    )
}

fn inserted_mark(num_id: i64) -> String {
    pp(
        &format!(
            "<w:numPr><w:numId w:val=\"{num_id}\"/></w:numPr>\
             <w:rPr><w:ins w:id=\"2\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"/></w:rPr>"
        ),
        "",
    )
}

/// A mark that one author inserted and another deleted: `w:ins` and `w:del`, in the order given.
fn inserted_and_deleted_mark(num_id: i64, first: &str, second: &str) -> String {
    pp(
        &format!(
            "<w:numPr><w:numId w:val=\"{num_id}\"/></w:numPr><w:rPr>\
             <w:{first} w:id=\"1\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"/>\
             <w:{second} w:id=\"2\" w:author=\"b\" w:date=\"2024-01-02T00:00:00Z\"/></w:rPr>"
        ),
        "",
    )
}

fn simple_list() -> String {
    format!("{}{}", abs(1, &lvl(0, "decimal", "%1.", "")), num(1, 1, ""))
}

#[test]
fn a_deleted_mark_advances_nothing_in_final_and_is_a_ghost_in_redline() {
    // Register: U3, U15.
    // Proves 4.6: Final skips it, Original counts it, Redline shows a ghost number that leaves the
    // counters alone, so the next live paragraph repeats it.
    let n = simple_list();
    let b = format!("{}{}{}", p(1, 0), deleted_mark(1), p(1, 0));
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![Some("1."), None, Some("2.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Original)),
        vec![Some("1."), Some("2."), Some("3.")]
    );
    let (got, numbers, facts) = detail(&n, "", &b, Redline);
    assert_eq!(labels(&got), vec![Some("1."), Some("2."), Some("2.")]);
    assert!(numbers.item(facts[1].node).unwrap().ghost);
    assert!(!numbers.item(facts[2].node).unwrap().ghost);
}

#[test]
fn an_inserted_mark_advances_nothing_in_original() {
    // Register: U3.
    // Proves the mirror: an inserted mark counts in Final and Redline, and not in Original.
    let n = simple_list();
    let b = format!("{}{}{}", p(1, 0), inserted_mark(1), p(1, 0));
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![Some("1."), Some("2."), Some("3.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Original)),
        vec![Some("1."), None, Some("2.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Redline)),
        vec![Some("1."), Some("2."), Some("3.")]
    );
}

#[test]
fn a_deleted_mark_on_the_last_paragraph_is_ignored() {
    // Register: U3.
    // Proves the join rule: with no paragraph after it the mark does not remove the paragraph.
    let n = simple_list();
    let b = format!("{}{}", p(1, 0), deleted_mark(1));
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![Some("1."), Some("2.")]
    );
}

#[test]
fn a_paragraph_in_a_deleted_row_is_absent_in_final() {
    // Register: U3, U15.
    // Proves row revisions: a paragraph in a row with a tracked deletion exists only in Original, and is
    // a ghost in Redline.
    let n = simple_list();
    let row = format!(
        "<w:tbl><w:tr><w:trPr><w:del w:id=\"3\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"/></w:trPr>\
         <w:tc>{}</w:tc></w:tr></w:tbl>",
        p(1, 0)
    );
    let b = format!("{}{row}{}", p(1, 0), p(1, 0));
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![Some("1."), None, Some("2.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Original)),
        vec![Some("1."), Some("2."), Some("3.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Redline)),
        vec![Some("1."), Some("2."), Some("2.")]
    );
}

#[test]
fn a_paragraph_property_change_is_read_from_the_old_properties_in_original() {
    // Register: U3.
    // Proves pPrChange: Original numbers from the old numPr (here another list), Final from the new.
    let n = format!(
        "{}{}{}{}",
        abs(1, &lvl(0, "decimal", "%1.", "")),
        abs(2, &lvl(0, "upperLetter", "%1)", "")),
        num(1, 1, ""),
        num(2, 2, "")
    );
    let changed = pp(
        "<w:numPr><w:numId w:val=\"1\"/></w:numPr>\
         <w:pPrChange w:id=\"4\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\">\
         <w:pPr><w:numPr><w:numId w:val=\"2\"/></w:numPr></w:pPr></w:pPrChange>",
        "",
    );
    let removed = pp(
        "<w:numPr><w:numId w:val=\"1\"/></w:numPr>\
         <w:pPrChange w:id=\"5\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"><w:pPr/></w:pPrChange>",
        "",
    );
    let b = format!("{changed}{removed}{}", p(1, 0));
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![Some("1."), Some("2."), Some("3.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Original)),
        vec![Some("A)"), None, Some("1.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Redline)),
        vec![Some("1."), Some("2."), Some("3.")]
    );
}

/// A tiny deterministic generator, so the property tests need no dependency.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound.max(1)
    }
}

#[test]
fn random_lists_never_panic_and_are_deterministic_and_prefix_stable() {
    // Proves totality and determinism over random definitions and paragraphs, and that appending a
    // paragraph never changes the numbers of the paragraphs before it.
    let fmts = [
        "decimal",
        "lowerLetter",
        "upperRoman",
        "bullet",
        "none",
        "ordinal",
        "cardinalText",
        "hindiVowels",
    ];
    let mut rng = Lcg(7);
    for round in 0..40 {
        let mut levels = String::new();
        for l in 0..9u8 {
            let fmt = fmts[rng.next(fmts.len() as u64) as usize];
            let extra = match rng.next(6) {
                0 => "<w:isLgl/>".to_string(),
                1 => format!("<w:lvlRestart w:val=\"{}\"/>", rng.next(10)),
                _ => String::new(),
            };
            let text: String = (0..=l).map(|k| format!("%{}.", k + 1)).collect();
            let start = if rng.next(4) == 0 {
                None
            } else {
                Some(rng.next(5) as i64)
            };
            levels.push_str(&lvl_start(l, start, fmt, &text, &extra));
        }
        let mut overrides = String::new();
        if rng.next(2) == 0 {
            overrides.push_str(&start_override(rng.next(9) as u8, rng.next(9) as i64));
        }
        let n = format!(
            "{}{}{}",
            abs(1, &levels),
            num(1, 1, ""),
            num(2, 1, &overrides)
        );
        let items: Vec<(i64, u8)> = (0..30)
            .map(|_| (1 + rng.next(2) as i64, rng.next(9) as u8))
            .collect();
        let full = run(&n, "", &body(&items), Final);
        assert_eq!(full, run(&n, "", &body(&items), Final), "round {round}");
        let shorter = run(&n, "", &body(&items[..20]), Final);
        assert_eq!(&full[..20], &shorter[..], "prefix stability, round {round}");
        for mode in [Original, Redline] {
            // No revisions: every mode agrees.
            assert_eq!(
                full,
                run(&n, "", &body(&items), mode),
                "mode, round {round}"
            );
        }
    }
}

#[test]
fn a_ghost_at_the_start_of_a_list_leaves_the_counters_empty() {
    // U15 (ghost_at_start): a deleted first paragraph shows the start value in Redline, and the first
    // live paragraph shows it again, because the ghost advanced nothing.
    let n = simple_list();
    let b = format!("{}{}{}", deleted_mark(1), p(1, 0), p(1, 0));
    assert_eq!(
        labels(&run(&n, "", &b, Redline)),
        vec![Some("1."), Some("1."), Some("2.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![None, Some("1."), Some("2.")]
    );
}

#[test]
fn two_ghosts_in_a_row_show_the_same_number() {
    // U15 (two ghosts in a row): neither advances anything, so both show the number the next live
    // paragraph is about to take, and that paragraph repeats it.
    let n = simple_list();
    let b = format!(
        "{}{}{}{}",
        p(1, 0),
        deleted_mark(1),
        deleted_mark(1),
        p(1, 0)
    );
    assert_eq!(
        labels(&run(&n, "", &b, Redline)),
        vec![Some("1."), Some("2."), Some("2."), Some("2.")]
    );
    assert_eq!(
        labels(&run(&n, "", &b, Final)),
        vec![Some("1."), None, None, Some("2.")]
    );
}

#[test]
fn a_mark_that_was_inserted_and_deleted_advances_nothing_in_either_mode() {
    // Proves: a mark that holds an insertion and a deletion is gone in Final (the deletion) and in
    // Original (the insertion), whichever revision is written first, so the paragraph joins the next
    // one in both and the next one carries the number. Reading one revision per mark numbered it in
    // Final when the insertion came first.
    let n = simple_list();
    for (first, second) in [("ins", "del"), ("del", "ins")] {
        let b = format!(
            "{}{}{}",
            p(1, 0),
            inserted_and_deleted_mark(1, first, second),
            p(1, 0)
        );
        for mode in [Final, Original] {
            assert_eq!(
                labels(&run(&n, "", &b, mode)),
                vec![Some("1."), None, Some("2.")],
                "{first} then {second}, {mode:?}"
            );
        }
    }
}

#[test]
fn a_ghost_between_two_levels_does_not_disturb_the_levels_below() {
    // U15 (ghost_between): a deleted level-0 paragraph between a level-0 and a level-1 paragraph
    // leaves the level-1 counter running under the live parent.
    let levels = format!(
        "{}{}",
        lvl(0, "decimal", "%1.", ""),
        lvl(1, "decimal", "%1.%2", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let del0 = pp(
        "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/></w:numPr>\
         <w:rPr><w:del w:id=\"1\" w:author=\"a\" w:date=\"2024-01-01T00:00:00Z\"/></w:rPr>",
        "",
    );
    let b = format!("{}{}{del0}{}{}", p(1, 0), p(1, 1), p(1, 1), p(1, 0));
    assert_eq!(
        labels(&run(&n, "", &b, Redline)),
        vec![Some("1."), Some("1.1"), Some("2."), Some("1.2"), Some("2.")]
    );
}
