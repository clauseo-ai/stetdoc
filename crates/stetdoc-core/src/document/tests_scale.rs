//! The work of an edit that grows with its batch: an `insert` of hundreds of paragraphs, or of lines that each hold a
//! quote. Each new paragraph used to look its neighbours, its quote context or its story's kind up in a story scanned again
//! after every edit, so the cost grew with the square of the paragraphs (the batch fuzz target found it). The tests count the
//! story scans an edit makes (`state::story_builds`) instead of timing it, as `check.md` V9 prefers: each scan walks the
//! whole story, so a number of scans that does not grow with the paragraphs keeps the edit linear.

use super::tests_edit::*;
use super::*;
use crate::api::batch::Batch;
use crate::api::edit::Edit;
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;

/// A block-text batch of one `insert` after p1 of `n` paragraphs of `line`.
fn insert_of(doc: &Document, n: usize, line: &str) -> Batch {
    let mut text = format!(
        "base: {}\n---\n@insert\nref: p1\nplace: after\nparagraphs: <<END\n",
        doc.base()
    );
    for _ in 0..n {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("END\n");
    Batch::parse(&text, &Limits::default()).unwrap()
}

/// The story scans of one edit of `n` paragraphs on a fresh document, tracked or not, with its save.
fn scans_of(n: usize, line: &str, tracked: bool) -> usize {
    let mut doc = rich();
    let b = insert_of(&doc, n, line);
    let mut edit = Edit::ops(b, stamp());
    if !tracked {
        edit = edit.untracked();
    }
    let before = crate::state::story_builds();
    doc.edit(edit).unwrap();
    doc.save().unwrap();
    crate::state::story_builds() - before
}

/// Four and eight times the paragraphs take no more scans than once as many, and far fewer than one for each paragraph:
/// a lookup for each new paragraph made it one scan or more for each.
fn scans_stay_flat(line: &str, tracked: bool) {
    let n = 50;
    let [one, four, eight] = [n, 4 * n, 8 * n].map(|size| scans_of(size, line, tracked));
    assert!(
        four <= one && eight <= one && one < n / 2,
        "{n}, {} and {} paragraphs ({line:?}, tracked {tracked}) scanned a story {one}, {four} and {eight} times",
        4 * n,
        8 * n
    );
}

#[test]
fn inserting_hundreds_of_empty_paragraphs_scans_the_story_a_fixed_number_of_times() {
    scans_stay_flat("", true);
    scans_stay_flat("", false);
}

#[test]
fn inserting_hundreds_of_lines_with_quotes_scans_the_story_a_fixed_number_of_times() {
    scans_stay_flat("He said \"yes\" and 'no'.", true);
    scans_stay_flat("He said \"yes\" and 'no'.", false);
}

#[test]
fn each_paragraph_of_an_insert_counts_as_an_operation() {
    let mut doc = rich();
    let limits = Limits {
        max_batch_ops: 10,
        ..Limits::default()
    };
    let text = |n: usize| {
        format!(
            "base: {}\n---\n@insert\nref: p1\nplace: after\nparagraphs: <<END\n{}END\n---\n@replace\nref: p2\nfind: Losses\nwith: Loss\n",
            doc.base(),
            "x\n".repeat(n)
        )
    };
    // Nine paragraphs and one more operation are ten: at the limit.
    assert!(Batch::parse(&text(9), &limits).is_ok());
    let e = Batch::parse(&text(10), &limits).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert!(
        e.message().contains("2 operations")
            && e.message().contains("11 paragraphs and operations")
            && e.message().contains("limit of 10"),
        "{e}"
    );
    assert!(e.next().contains("fewer paragraphs"), "{}", e.next());
    // The default limit refuses an insert of more paragraphs than it allows operations.
    let b = format!(
        "base: {}\n---\n@insert\nref: p1\nplace: after\nparagraphs: <<END\n{}END\n",
        doc.base(),
        "\n".repeat(5_001)
    );
    let e = Batch::parse(&b, &Limits::default()).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded, "{e}");
    let b = insert_of(&doc, 3, "x");
    doc.edit(Edit::ops(b, stamp())).unwrap();
}
