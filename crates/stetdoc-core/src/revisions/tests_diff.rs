//! Tests for the word difference: a table of known answers, and the helpers that
//! `tests_diff_props` uses for the properties over many random texts.

use super::diff::*;
use std::ops::Range;

pub(super) struct Rng(pub(super) u64);

impl Rng {
    pub(super) fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % n.max(1) as u64).unwrap_or(0)
    }
}

pub(super) const WORDS: [&str; 16] = [
    "the", "Party", "shall", "pay", "within", "30", "days", "Section", "3", "Services", "fox",
    "Customer", "Supplier", "of", "a", "and",
];

pub(super) const MISC: [&str; 18] = [
    " ",
    " ",
    " ",
    "  ",
    ",",
    ".",
    "(",
    ")",
    "'",
    "-",
    "\t",
    "\n",
    "\u{a0}",
    "e\u{301}",
    "\u{e9}",
    "\u{1f600}",
    "👨\u{200d}👩\u{200d}👧",
    "合同",
];

/// A text of words and the things that make trouble: accents, a combining mark, an emoji sequence, a
/// tab, a line break, a no-break space, characters with no case.
pub(super) fn text(rng: &mut Rng, pieces: usize) -> String {
    let mut s = String::new();
    for _ in 0..rng.below(pieces + 1) {
        if rng.below(3) == 0 {
            s.push_str(MISC[rng.below(MISC.len())]);
        } else {
            s.push_str(WORDS[rng.below(WORDS.len())]);
            if rng.below(2) == 0 {
                s.push(' ');
            }
        }
    }
    s
}

/// A text that resembles `old`: words changed, inserted and deleted.
pub(super) fn mutate(rng: &mut Rng, old: &str) -> String {
    let parts: Vec<&str> = old.split_inclusive(' ').collect();
    let mut out = String::new();
    for p in parts {
        match rng.below(8) {
            0 => {}
            1 => out.push_str(WORDS[rng.below(WORDS.len())]),
            2 => {
                out.push_str(p);
                out.push_str(WORDS[rng.below(WORDS.len())]);
                out.push(' ');
            }
            3 => {
                out.push_str(p);
                out.push_str(MISC[rng.below(MISC.len())]);
            }
            _ => out.push_str(p),
        }
    }
    out
}

pub(super) fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

/// The positions where a token begins or ends in `s`.
pub(super) fn boundaries(s: &str) -> Vec<usize> {
    let c = chars(s);
    let mut b = vec![0];
    b.extend(tokenize(&c).into_iter().map(|(r, _)| r.end));
    b
}

pub(super) fn kept_tokens(old: &str, edits: &[Edit]) -> usize {
    let c = chars(old);
    tokenize(&c)
        .into_iter()
        .filter(|(r, _)| {
            !edits
                .iter()
                .any(|e| e.range.start < r.end && r.start < e.range.end)
        })
        .filter(|(r, _)| {
            !edits
                .iter()
                .any(|e| e.range.is_empty() && e.range.start > r.start && e.range.start < r.end)
        })
        .count()
}

/// The length of the longest common subsequence of the tokens of two texts.
pub(super) fn lcs_tokens(a: &str, b: &str) -> usize {
    let (ca, cb) = (chars(a), chars(b));
    let ta: Vec<Vec<char>> = tokenize(&ca)
        .into_iter()
        .map(|(r, _)| ca[r].to_vec())
        .collect();
    let tb: Vec<Vec<char>> = tokenize(&cb)
        .into_iter()
        .map(|(r, _)| cb[r].to_vec())
        .collect();
    let mut t = vec![vec![0usize; tb.len() + 1]; ta.len() + 1];
    for i in (0..ta.len()).rev() {
        for j in (0..tb.len()).rev() {
            t[i][j] = if ta[i] == tb[j] {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    t[0][0]
}

/// Everything that must hold of a difference, whatever the cleanup.
pub(super) fn check_script(old: &str, new: &str, edits: &[Edit]) {
    assert_eq!(
        apply(old, edits),
        new,
        "old {old:?} new {new:?} edits {edits:?}"
    );
    let len = old.chars().count();
    let old_b = boundaries(old);
    let new_b = boundaries(new);
    let mut new_at = 0usize;
    let mut prev_end: Option<usize> = None;
    for e in edits {
        assert!(
            e.range.start <= e.range.end && e.range.end <= len,
            "{edits:?}"
        );
        assert!(
            !(e.range.is_empty() && e.with.is_empty()),
            "an empty edit: {edits:?}"
        );
        if let Some(p) = prev_end {
            assert!(e.range.start > p, "edits touch or overlap: {edits:?}");
            new_at += e.range.start - p;
        } else {
            new_at += e.range.start;
        }
        assert!(
            old_b.contains(&e.range.start) && old_b.contains(&e.range.end),
            "cut inside a token of {old:?}: {edits:?}"
        );
        assert!(
            new_b.contains(&new_at),
            "new text cut inside a token of {new:?}: {edits:?}"
        );
        new_at += e.with.chars().count();
        assert!(
            new_b.contains(&new_at),
            "new text cut inside a token of {new:?}: {edits:?}"
        );
        prev_end = Some(e.range.end);
    }
}

pub(super) fn e(range: Range<usize>, with: &str) -> Edit {
    Edit {
        range,
        with: with.to_owned(),
    }
}

pub(super) fn both(old: &str, new: &str) -> (Vec<Edit>, Vec<Edit>) {
    (
        edits(old, new, Cleanup::Minimal),
        edits(old, new, Cleanup::Readable),
    )
}

// ---- known answers ------------------------------------------------------------------------------

#[test]
fn identical_texts_have_no_edits() {
    for s in [
        "",
        "a",
        "the Party shall pay",
        "e\u{301} 👨\u{200d}👩\u{200d}👧",
    ] {
        assert_eq!(edits(s, s, Cleanup::Minimal), []);
        assert_eq!(edits(s, s, Cleanup::Readable), []);
    }
}

#[test]
fn an_empty_side_is_one_edit() {
    assert_eq!(
        edits("", "new text", Cleanup::Readable),
        [e(0..0, "new text")]
    );
    assert_eq!(edits("old text", "", Cleanup::Readable), [e(0..8, "")]);
}

#[test]
fn one_word_changed_is_one_small_edit() {
    let old = "Payment is due within 30 days.";
    let new = "Payment is due within 45 days.";
    let (min, read) = both(old, new);
    assert_eq!(min, [e(22..24, "45")]);
    assert_eq!(read, min);
}

#[test]
fn an_insertion_and_a_deletion_are_at_word_boundaries() {
    assert_eq!(
        edits("foo bar", "foo big bar", Cleanup::Readable),
        [e(4..4, "big ")]
    );
    assert_eq!(
        edits("foo big bar", "foo bar", Cleanup::Readable),
        [e(4..8, "")]
    );
    assert_eq!(
        edits("foo", "foo bar", Cleanup::Readable),
        [e(3..3, " bar")]
    );
}

#[test]
fn a_word_that_changes_a_few_letters_is_replaced_whole() {
    assert_eq!(
        edits("the colour red", "the color red", Cleanup::Readable),
        [e(4..10, "color")]
    );
}

#[test]
fn islands_of_no_word_are_dissolved() {
    // The three words differ and only spaces are left between them.
    let (min, read) = both("the quick brown fox", "a slow red fox");
    assert_eq!(min.len(), 3, "{min:?}");
    assert_eq!(read, [e(0..15, "a slow red")]);
}

#[test]
fn a_whole_paragraph_with_nothing_in_common_is_one_edit() {
    let (min, read) = both("alpha beta", "gamma delta");
    assert_eq!(min.len(), 2, "{min:?}");
    assert_eq!(read, [e(0..10, "gamma delta")]);
}

#[test]
fn a_short_island_is_dissolved_when_the_edits_around_it_are_longer() {
    let old = "alpha beta gamma delta epsilon";
    let new = "ALPHA beta GAMMA delta EPSILON";
    let (min, read) = both(old, new);
    assert_eq!(min.len(), 3, "{min:?}");
    assert_eq!(read, [e(0..30, new)]);
}

#[test]
fn a_long_island_is_kept() {
    let old = "Alpha one two three four five Omega";
    let new = "Beta one two three four five Psi";
    let (min, read) = both(old, new);
    assert_eq!(min, [e(0..5, "Beta"), e(30..35, "Psi")]);
    assert_eq!(read, min);
}

#[test]
fn an_island_of_three_words_is_kept_however_short_it_is_next_to_long_edits() {
    // The island is shorter than the edits, but a dissolved island is at most two words.
    let old = "AAAAAAAAAA a b c BBBBBBBBBB";
    let new = "CCCCCCCCCC a b c DDDDDDDDDD";
    assert_eq!(
        edits(old, new, Cleanup::Readable),
        [e(0..10, "CCCCCCCCCC"), e(17..27, "DDDDDDDDDD")]
    );
}

#[test]
fn exactly_half_of_the_words_changed_does_not_make_one_edit() {
    // Four of the eight words of the span are in edits: not more than half.
    let (min, read) = both("A b c D", "X b c Y");
    assert_eq!(min, [e(0..1, "X"), e(6..7, "Y")]);
    assert_eq!(read, min);
}

#[test]
fn a_common_ending_is_matched_even_when_the_middle_is_too_big_to_match() {
    // Nothing is unique, and the middle is over the table's limit, so the middle is one edit.
    // The ending must still be trimmed first and stay out of it.
    let tail = "ab cd ab cd ab cd ab cd";
    let old: String = (0..4000)
        .map(|i| if i % 3 == 0 { "ab " } else { "cd " })
        .collect::<String>()
        + tail;
    let new: String = (0..4000)
        .map(|i| if i % 5 == 0 { "cd " } else { "ab " })
        .collect::<String>()
        + tail;
    let list = edits(&old, &new, Cleanup::Minimal);
    check_script(&old, &new, &list);
    let tail_start = old.chars().count() - tail.chars().count();
    assert!(
        list.iter().all(|e| e.range.end <= tail_start),
        "{:?}",
        list.last()
    );
}

#[test]
fn of_two_crossing_anchors_the_later_one_in_the_new_text_is_kept() {
    // `a` and `b` swap places. Only one of them can stay, and the result is the same every time.
    assert_eq!(
        edits("a b", "b a", Cleanup::Minimal),
        [e(0..2, ""), e(3..3, " a")]
    );
}

#[test]
fn of_equally_long_matches_the_one_that_deletes_from_the_old_text_first_is_taken() {
    // Nothing is unique, so the table decides. "b a b" stays in either answer; this one deletes first.
    assert_eq!(
        edits("a b a b", "b a b a", Cleanup::Minimal),
        [e(0..2, ""), e(7..7, " a")]
    );
}

#[test]
fn the_common_beginning_is_matched_before_a_repeated_word_further_on() {
    assert_eq!(
        edits("the cat the dog", "the dog", Cleanup::Minimal),
        [e(4..12, "")]
    );
}

#[test]
fn edits_with_a_clause_between_them_stay_apart() {
    let old = "We will provide the Services to the Customer";
    let new = "We shall supply the Services to the Client";
    let (_, read) = both(old, new);
    assert_eq!(read, [e(3..15, "shall supply"), e(36..44, "Client")]);
}

#[test]
fn most_words_changed_makes_the_whole_span_one_edit() {
    // Four of the six words of the span changed, so the span is one replacement.
    let old = "a LONGISLAND b";
    let new = "x LONGISLAND y";
    assert_eq!(edits(old, new, Cleanup::Minimal).len(), 2);
    assert_eq!(edits(old, new, Cleanup::Readable), [e(0..14, new)]);
}

/// The one edit of a readable difference, as the deleted and the inserted text.
fn one_edit(old: &str, new: &str) -> (String, String) {
    let list = edits(old, new, Cleanup::Readable);
    check_script(old, new, &list);
    assert_eq!(list.len(), 1, "{old:?} {new:?} {list:?}");
    let deleted = chars(old)[list[0].range.clone()].iter().collect();
    (deleted, list[0].with.clone())
}

#[test]
fn an_edit_never_cuts_a_bracket_pair_in_half() {
    // old, new, the deletion, the insertion: the cases of the report on 0.9.0, and the ones that must not change.
    let table = [
        (
            "due within thirty (30) days",
            "due within forty-five (45) days",
            "thirty (30)",
            "forty-five (45)",
        ),
        (
            "thirty (30)",
            "forty-five (45)",
            "thirty (30)",
            "forty-five (45)",
        ),
        ("within (30) days", "within [30] days", "(30)", "[30]"),
        (
            "interest at 2% per month",
            "interest at 3% per month",
            "2",
            "3",
        ),
        (
            "the Seller's obligations",
            "the Vendor's obligations",
            "Seller",
            "Vendor",
        ),
        ("(see clause 3.3)", "(see clause 7.5(a))", "3.3", "7.5(a)"),
        (
            "Payment is due within thirty days.",
            "Payment is due within sixty days.",
            "thirty",
            "sixty",
        ),
        ("thirty days", "sixty (60) days", "thirty", "sixty (60)"),
        // The mirror: a closer the edit leaves unopened takes the opener before it.
        (
            "under (a) Seller shall",
            "under (b) Buyer shall",
            "(a) Seller",
            "(b) Buyer",
        ),
        // Brackets nested two deep are both taken.
        (
            "see [clause (3",
            "see [section (4",
            "clause (3",
            "section (4",
        ),
        (
            "as in f(g(x)) here",
            "as in h(k(y)) here",
            "f(g(x))",
            "h(k(y))",
        ),
        // Curly double quotes pair; a straight quote has no direction and stays out.
        (
            "the \u{201C}Price list\u{201D} means",
            "the \u{201C}Cost sheet\u{201D} means",
            "Price list",
            "Cost sheet",
        ),
        (
            "the \u{201C}Price (net\u{201D} means",
            "the \u{201C}Cost (gross\u{201D} means",
            "Price (net",
            "Cost (gross",
        ),
        (
            "says \"thirty (30)\" here",
            "says \"sixty (60)\" here",
            "thirty (30)",
            "sixty (60)",
        ),
    ];
    for (old, new, deleted, inserted) in table {
        assert_eq!(
            one_edit(old, new),
            (deleted.to_owned(), inserted.to_owned()),
            "{old:?} -> {new:?}"
        );
    }
}

#[test]
fn a_bracket_that_another_edit_holds_or_that_the_caller_keeps_out_is_not_taken() {
    // The caller keeps the closer out: the edit is what it was before the bracket rule.
    let old = "due within thirty (30) days";
    let new = "due within forty-five (45) days";
    let closer = old.find(')').unwrap();
    let list = edits_keeping(old, new, &|r| r.contains(&closer));
    check_script(old, new, &list);
    assert_eq!(list, [e(11..21, "forty-five (45")]);
    // Kept out elsewhere, it is taken.
    assert_eq!(
        edits_keeping(old, new, &|r| r.start == 0),
        [e(11..22, "forty-five (45)")]
    );
}

#[test]
fn a_minimal_difference_keeps_the_bracket_where_the_matching_put_it() {
    assert_eq!(
        edits("thirty (30) days", "forty-five (45) days", Cleanup::Minimal),
        [e(0..6, "forty-five"), e(8..10, "45")]
    );
}

#[test]
fn a_combining_mark_stays_with_its_letter() {
    let (min, read) = both("a cafe\u{301} b", "a cafe b");
    assert_eq!(min, [e(2..7, "cafe")]);
    assert_eq!(read, min);
}

#[test]
fn an_emoji_sequence_is_one_token() {
    let old = "x 👨\u{200d}👩\u{200d}👧 y";
    let new = "x 👨\u{200d}👩\u{200d}👦 y";
    let (min, _) = both(old, new);
    assert_eq!(min, [e(2..7, "👨\u{200d}👩\u{200d}👦")]);
}

#[test]
fn an_atom_that_the_new_text_lacks_is_always_deleted() {
    let old = "see\u{1} here\u{2}.";
    let new = "see here.";
    let (min, read) = both(old, new);
    assert_eq!(min, [e(3..4, ""), e(9..10, "")]);
    assert_eq!(read.len(), 2, "{read:?}");
}

#[test]
fn the_tokenizer_cuts_words_spaces_and_single_characters() {
    let c = chars("ab1  cd, e\tf");
    let toks: Vec<(Range<usize>, Kind)> = tokenize(&c);
    let want = [
        (0..3, Kind::Word),
        (3..5, Kind::Space),
        (5..7, Kind::Word),
        (7..8, Kind::Other),
        (8..9, Kind::Space),
        (9..10, Kind::Word),
        (10..11, Kind::Other),
        (11..12, Kind::Word),
    ];
    assert_eq!(toks, want);
}
