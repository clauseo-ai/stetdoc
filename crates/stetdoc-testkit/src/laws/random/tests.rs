//! The comparison of view lines that L5 makes, on pages written as the view writes them.

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
    assert_eq!(lines[1], format!("# {text}"));
}

#[test]
fn an_even_run_of_backslashes_ends_a_line_and_an_odd_one_continues_it() {
    assert_eq!(joined("p1 a\\\\\np2 b"), "p1 a\\\\\np2 b");
    assert_eq!(joined("p1 a\\\nb\np2 c\n"), "p1 ab\np2 c\n");
    assert_eq!(joined("p1 a\\\\\\\nb"), "p1 a\\\\b");
}
