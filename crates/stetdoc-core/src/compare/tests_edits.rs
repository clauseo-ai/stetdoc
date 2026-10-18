//! Tests of the replacements and the widening to whole fields.

#![expect(
    clippy::single_range_in_vec_init,
    reason = "the arguments are lists of field ranges, and a list of one is a case"
)]

use super::edits::{Replace, map_through, replacements};

fn r(a: std::ops::Range<usize>, b: std::ops::Range<usize>) -> Replace {
    Replace { a, b }
}

#[test]
fn equal_texts_have_no_replacement() {
    assert!(replacements("same text", "same text", &[], &[]).is_empty());
}

#[test]
fn a_word_replaced_is_one_replacement_in_both_coordinates() {
    let rs = replacements("the old term applies", "the new term applies", &[], &[]);
    assert_eq!(rs, vec![r(4..7, 4..7)]);
}

#[test]
fn an_insertion_and_a_deletion_have_an_empty_side() {
    assert_eq!(
        replacements("a b c", "a b c d e", &[], &[]),
        vec![r(5..5, 5..9)]
    );
    assert_eq!(
        replacements("a b c d e", "a b c", &[], &[]),
        vec![r(5..9, 5..5)]
    );
}

#[test]
fn an_edit_that_touches_a_field_takes_the_whole_field_on_both_sides() {
    // "see 4.2 for" -> "see 4.3 for", the "4.2" is the result of a field of 3 characters at 4..7
    let rs = replacements("see 4.2 for", "see 4.3 for", &[4..7], &[4..7]);
    assert_eq!(rs, vec![r(4..7, 4..7)]);
    // the word diff changes only the last digit; with the field it is the whole result
    let plain = replacements("see 4.2 for", "see 4.3 for", &[], &[]);
    assert_ne!(plain, rs);
}

#[test]
fn a_widened_edit_that_meets_its_neighbour_is_one() {
    let rs = replacements(
        "x 1.2 and 3.4 y",
        "x 1.3 and 3.5 y",
        &[2..5, 10..13],
        &[2..5, 10..13],
    );
    assert_eq!(rs.len(), 2);
    let rs = replacements("x 1.2 3.4 y", "x 1.3 3.5 y", &[2..5, 6..9], &[2..5, 6..9]);
    assert_eq!(rs.len(), 1);
    assert_eq!(rs[0], r(2..9, 2..9));
}

#[test]
fn an_insertion_inside_a_field_widens_to_the_field() {
    let rs = replacements("page 12 here", "page 125 here", &[5..7], &[5..8]);
    assert_eq!(rs, vec![r(5..7, 5..8)]);
}

#[test]
fn offsets_map_through_the_unchanged_stretches_only() {
    let rs = replacements(
        "the old term applies",
        "the brand new term applies",
        &[],
        &[],
    );
    assert_eq!(map_through(&rs, 0), Some(0));
    assert_eq!(map_through(&rs, 5), None);
    assert_eq!(map_through(&rs, 12), Some(18));
}
