//! Properties of the word difference over many random texts. They are the ones the compiler relies
//! on: the edits turn the old text into the new, they do not overlap or touch, they begin and end
//! between tokens (so never inside a grapheme), cleaning only merges, and the result is a function of
//! the inputs.

use super::diff::*;
use super::tests_diff::*;

#[test]
fn tokens_are_contiguous_and_cover_the_text() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..3000 {
        let s = text(&mut rng, 14);
        let c = chars(&s);
        let toks = tokenize(&c);
        let mut at = 0;
        for (r, _) in &toks {
            assert_eq!(r.start, at, "{s:?}");
            assert!(r.end > r.start);
            at = r.end;
        }
        assert_eq!(at, c.len(), "{s:?}");
    }
}

#[test]
fn a_token_never_begins_with_a_joiner_or_follows_a_zero_width_joiner() {
    let mut rng = Rng(0x0F0F_1234_5678_9ABD);
    for _ in 0..3000 {
        let s = text(&mut rng, 14);
        let c = chars(&s);
        for (r, _) in tokenize(&c).into_iter().skip(1) {
            let first = c[r.start];
            assert!(
                !('\u{300}'..='\u{36f}').contains(&first) && first != '\u{200d}',
                "{s:?}"
            );
            assert_ne!(c[r.start - 1], '\u{200d}', "{s:?}");
        }
    }
}

#[test]
fn the_edits_turn_the_old_text_into_the_new_for_related_texts() {
    let mut rng = Rng(0xDEAD_BEEF_0000_0001);
    for _ in 0..6000 {
        let old = text(&mut rng, 16);
        let new = mutate(&mut rng, &old);
        let (min, read) = both(&old, &new);
        check_script(&old, &new, &min);
        check_script(&old, &new, &read);
    }
}

#[test]
fn the_edits_turn_the_old_text_into_the_new_for_unrelated_texts() {
    let mut rng = Rng(0xABCD_EF01_2345_6789);
    for _ in 0..6000 {
        let old = text(&mut rng, 14);
        let new = text(&mut rng, 14);
        let (min, read) = both(&old, &new);
        check_script(&old, &new, &min);
        check_script(&old, &new, &read);
    }
}

#[test]
fn the_same_input_gives_the_same_edits() {
    let mut rng = Rng(0x5555_AAAA_1111_2222);
    for _ in 0..500 {
        let old = text(&mut rng, 20);
        let new = mutate(&mut rng, &old);
        for c in [Cleanup::Minimal, Cleanup::Readable] {
            assert_eq!(edits(&old, &new, c), edits(&old, &new, c));
        }
    }
}

#[test]
fn applying_the_edits_to_the_result_leaves_nothing_to_change() {
    let mut rng = Rng(0x7777_1313_2424_3535);
    for _ in 0..1500 {
        let old = text(&mut rng, 14);
        let new = mutate(&mut rng, &old);
        let done = apply(&old, &edits(&old, &new, Cleanup::Readable));
        assert_eq!(edits(&done, &new, Cleanup::Readable), []);
    }
}

#[test]
fn the_minimal_difference_never_keeps_more_than_the_longest_common_subsequence() {
    let mut rng = Rng(0x2468_ACE0_1357_9BDF);
    for _ in 0..1500 {
        let old = text(&mut rng, 12);
        let new = mutate(&mut rng, &old);
        let min = edits(&old, &new, Cleanup::Minimal);
        assert!(
            kept_tokens(&old, &min) <= lcs_tokens(&old, &new),
            "{old:?} {new:?}"
        );
    }
}

#[test]
fn with_distinct_words_in_the_same_order_the_minimal_difference_is_the_longest_common_subsequence()
{
    let mut rng = Rng(0x1357_9BDF_2468_ACE1);
    for _ in 0..1500 {
        let n = 2 + rng.below(14);
        let old_words: Vec<String> = (0..n).map(|i| format!("w{i}x")).collect();
        let mut new_words: Vec<String> = Vec::new();
        for (i, w) in old_words.iter().enumerate() {
            match rng.below(5) {
                0 => {}
                1 => new_words.push(format!("n{i}y")),
                2 => {
                    new_words.push(w.clone());
                    new_words.push(format!("m{i}z"));
                }
                _ => new_words.push(w.clone()),
            }
        }
        let (old, new) = (old_words.join(" "), new_words.join(" "));
        let min = edits(&old, &new, Cleanup::Minimal);
        check_script(&old, &new, &min);
        assert_eq!(
            kept_tokens(&old, &min),
            lcs_tokens(&old, &new),
            "{old:?} {new:?} {min:?}"
        );
    }
}

#[test]
fn cleaning_only_merges_edits_and_never_keeps_text_the_minimal_difference_changed() {
    let mut rng = Rng(0x9999_8888_7777_6666);
    for _ in 0..4000 {
        let old = text(&mut rng, 16);
        let new = mutate(&mut rng, &old);
        let (min, read) = both(&old, &new);
        assert!(read.len() <= min.len(), "{old:?} {new:?}");
        for m in &min {
            let covered = read
                .iter()
                .any(|r| r.range.start <= m.range.start && m.range.end <= r.range.end);
            assert!(covered, "{old:?} {new:?} min {min:?} read {read:?}");
        }
    }
}

/// The word tokens inside a range of `s`, counted by tokenizing that range alone, which is right
/// because every boundary of an edit is a token boundary.
fn words(s: &[char]) -> usize {
    tokenize(s)
        .into_iter()
        .filter(|(_, k)| *k == Kind::Word)
        .count()
}

#[test]
fn the_cleaned_edits_are_a_fixed_point_of_the_cleaning_rules() {
    let mut rng = Rng(0x4242_4242_4242_4243);
    for _ in 0..4000 {
        let old = text(&mut rng, 16);
        let new = mutate(&mut rng, &old);
        let read = edits(&old, &new, Cleanup::Readable);
        let oc = chars(&old);
        let size = |e: &Edit| e.range.len() + e.with.chars().count();
        for w in read.windows(2) {
            let island = &oc[w[0].range.end..w[1].range.start];
            assert!(!island.is_empty(), "{read:?}");
            let island_words = words(island);
            assert!(
                island_words >= 1,
                "an island of no word is left: {old:?} {new:?} {read:?}"
            );
            if island_words <= 2 {
                assert!(
                    island.len() > size(&w[0]).max(size(&w[1])),
                    "{old:?} {new:?} {read:?}"
                );
            }
        }
        if read.len() > 1 {
            let (first, last) = (&read[0], &read[read.len() - 1]);
            let inside: usize = read
                .iter()
                .map(|e| words(&oc[e.range.clone()]) + words(&chars(&e.with)))
                .sum();
            let delta: isize = read
                .iter()
                .map(|e| e.with.chars().count() as isize - e.range.len() as isize)
                .sum();
            let new_end = (last.range.end as isize + delta) as usize;
            let new_span = &chars(&new)[first.range.start..new_end];
            let span = words(&oc[first.range.start..last.range.end]) + words(new_span);
            assert!(inside * 2 <= span, "{old:?} {new:?} {read:?}");
        }
    }
}

#[test]
fn an_atom_in_the_old_text_is_always_inside_an_edit_when_the_new_text_has_none() {
    let mut rng = Rng(0x3131_4141_5151_6161);
    for _ in 0..2000 {
        let mut old = text(&mut rng, 12);
        for _ in 0..rng.below(3) {
            let at = old
                .char_indices()
                .map(|(i, _)| i)
                .nth(rng.below(old.chars().count() + 1))
                .unwrap_or(old.len());
            old.insert(at, ['\u{1}', '\u{2}', '\u{5}', '\u{c}'][rng.below(4)]);
        }
        let new = mutate(
            &mut rng,
            &old.replace(['\u{1}', '\u{2}', '\u{5}', '\u{c}'], ""),
        );
        for c in [Cleanup::Minimal, Cleanup::Readable] {
            let list = edits(&old, &new, c);
            check_script(&old, &new, &list);
            for (i, ch) in old.chars().enumerate() {
                if ('\u{1}'..='\u{6}').contains(&ch) || ch == '\u{c}' {
                    assert!(
                        list.iter().any(|e| e.range.contains(&i)),
                        "atom at {i} kept: {old:?} {new:?} {list:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_difference_with_no_anchor_and_a_huge_middle_is_one_edit_and_finishes() {
    // Two words alternate, so no token is unique, and the middle is too big for the table.
    let old: String = (0..4000)
        .map(|i| if i % 3 == 0 { "ab " } else { "cd " })
        .collect();
    let new: String = (0..4000)
        .map(|i| if i % 5 == 0 { "cd " } else { "ab " })
        .collect();
    let list = edits(&old, &new, Cleanup::Readable);
    check_script(&old, &new, &list);
    assert!(list.len() <= 2, "{} edits", list.len());
}

#[test]
fn many_stretches_stay_inside_the_work_budget_and_the_result_is_still_valid() {
    // Unique anchors every few tokens leave many stretches, each small: all of them are matched.
    let mut rng = Rng(0x1111_2222_3333_4445);
    let mut old = String::new();
    let mut new = String::new();
    for i in 0..3000 {
        old.push_str(&format!(
            "anchor{i} {} {} ",
            WORDS[rng.below(8)],
            WORDS[rng.below(8)]
        ));
        new.push_str(&format!(
            "anchor{i} {} {} ",
            WORDS[rng.below(8)],
            WORDS[rng.below(8)]
        ));
    }
    for c in [Cleanup::Minimal, Cleanup::Readable] {
        let list = edits(&old, &new, c);
        check_script(&old, &new, &list);
    }
}

#[test]
fn long_texts_with_one_change_are_cheap_and_exact() {
    let base: String = (0..20_000).map(|i| format!("w{i} ")).collect();
    let changed = base.replacen("w10000 ", "w10000x ", 1);
    let list = edits(&base, &changed, Cleanup::Readable);
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].with, "w10000x");
    check_script(&base, &changed, &list);
}
