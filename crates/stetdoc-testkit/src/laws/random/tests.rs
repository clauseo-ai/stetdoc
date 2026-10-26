//! The comparison of view lines that L5 makes, on pages written as the view writes them.

use super::view_lines::*;
use super::*;

/// A line written as the view writes it: broken after the last space from column 1,000 to 1,498, else at 1,498, each piece but the last
/// ending in a backslash.
fn wrapped(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut start = 0;
    while chars.len() - start > 1498 {
        let window = &chars[start..start + 1498];
        let at = window[1000..]
            .iter()
            .rposition(|c| *c == ' ')
            .map_or(1498, |i| 1000 + i + 1);
        out.extend(&window[..at]);
        out.push_str("\\\n");
        start += at;
    }
    out.extend(&chars[start..]);
    out
}

#[test]
fn a_long_block_under_refs_of_two_lengths_gives_the_same_lines() {
    // Words, and then a long run without a space, where the view breaks at column 1,498 whatever the ref.
    let words: String = (0..100).map(|i| format!("word{i} ")).collect();
    let text = format!("{words}{}", "abcdefghij".repeat(300));
    assert!(text.chars().count() > 1498 * 2);
    let page = |r: &str| {
        format!(
            "p1 Before.\n{}\np2 After.\n",
            wrapped(&format!("{r} {text}"))
        )
    };
    let (short, long) = (page("p86"), page("p301"));
    // The view breaks the two in different places, so the physical lines differ beyond their refs.
    assert_ne!(
        short.lines().nth(1).map(without_refs),
        long.lines().nth(1).map(without_refs)
    );
    let lines = block_lines(&short);
    let text = text.as_str();
    assert_eq!(lines, block_lines(&long));
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert_eq!(lines[1].text, format!("# {text}"));
}

#[test]
fn an_even_run_of_backslashes_ends_a_line_and_an_odd_one_continues_it() {
    assert_eq!(joined("p1 a\\\\\np2 b"), "p1 a\\\\\np2 b");
    assert_eq!(joined("p1 a\\\nb\np2 c\n"), "p1 ab\np2 c\n");
    assert_eq!(joined("p1 a\\\\\\\nb"), "p1 a\\\\b");
}

/// The lines of a page of the body under a header with this common style.
fn page(common: &str, body: &str) -> Vec<ViewLine> {
    block_lines(&format!(
        "{{stet 1 view=body base=x mode=original common-style=\"{common}\" range=t1..t1}}\n{body}"
    ))
}

/// A cell of t1 that holds a paragraph and then a table, t2, and a paragraph after the table.
const CELL: &str = "t1 {table cols=1}\nr1 {row}\np1 Before.\nt2 {+} {table cols=1}\nr2 {row}\np2 Inner.\n{/table}\n";

#[test]
fn one_empty_paragraph_after_a_table_that_ended_its_cell_is_restored_enough() {
    let original = page("Normal", &format!("{CELL}{{/table}}\np3 After.\n"));
    let after = page(
        "Normal",
        &format!("{CELL}p9 {{+}}\n{{/table}}\np3 After.\n"),
    );
    assert!(after.iter().any(|l| l.filler), "{after:?}");
    assert_eq!(restored(&original, &after), Ok(()));
    // Two of them are one too many.
    let two = page(
        "Normal",
        &format!("{CELL}p9 {{+}}\np10 {{+}}\n{{/table}}\np3 After.\n"),
    );
    assert!(restored(&original, &two).is_err(), "{two:?}");
    // An empty paragraph that does not end a cell after a table is a difference: here in the body, after the outer table.
    let outside = page("Normal", &format!("{CELL}{{/table}}\np9\np3 After.\n"));
    assert!(restored(&original, &outside).is_err(), "{outside:?}");
    // And so is one whose text is not empty.
    let full = page(
        "Normal",
        &format!("{CELL}p9 {{+}} Text.\n{{/table}}\np3 After.\n"),
    );
    assert!(restored(&original, &full).is_err(), "{full:?}");
}

#[test]
fn a_flip_of_the_common_style_is_no_difference_and_a_change_of_one_paragraph_style_is() {
    let original = page(
        "Table Space",
        "p1 {style \"Normal\"} {joins-next}\np2 {style \"Normal\"} Text.\np3 Other.\n",
    );
    let flipped = page(
        "Normal",
        "p1 {joins-next}\np2 Text.\np3 {style \"Table Space\"} Other.\n",
    );
    assert_eq!(restored(&original, &flipped), Ok(()));
    let changed = page(
        "Normal",
        "p1 {joins-next}\np2 {style \"Heading 1\"} Text.\np3 {style \"Table Space\"} Other.\n",
    );
    assert_eq!(restored(&original, &changed), Err(1));
    // A paragraph that lost its own style for the common one is a change too.
    let lost = page(
        "Table Space",
        "p1 {style \"Normal\"} {joins-next}\np2 Text.\np3 Other.\n",
    );
    assert_eq!(restored(&original, &lost), Err(1));
}
