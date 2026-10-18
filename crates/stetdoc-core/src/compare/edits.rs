//! The word edits of a pair of paragraphs, and the offsets that map through them (design `compare.md`, 5.3).
//!
//! `diff::edits` gives replacements over the original's characters. Compare reads them two ways: as the plan of an edit
//! (the revised range to mark inserted and the original range to import), and as a map from an offset in the original's
//! text to the offset of the same character in the revised text when it is in an unchanged stretch. The note pairing
//! uses the map, and so does the formatting comparison.

use crate::revisions::diff::{Cleanup, Edit, edits};

/// The cleaned word edits that turn `old` into `new`.
pub(super) fn word_edits(old: &str, new: &str) -> Vec<Edit> {
    edits(old, new, Cleanup::Readable)
}

/// The offset in the revised text of the character at `at` in the original text, or `None` if an edit replaces it. A
/// character after a pure insertion is unchanged, and so is the one before it.
pub(super) fn map_unchanged(edits: &[Edit], at: usize) -> Option<usize> {
    let mut delta: isize = 0;
    for e in edits {
        if at < e.range.start {
            break;
        }
        if at < e.range.end {
            return None;
        }
        let with = isize::try_from(e.with.chars().count()).ok()?;
        let cut = isize::try_from(e.range.len()).ok()?;
        delta += with - cut;
    }
    usize::try_from(isize::try_from(at).ok()? + delta).ok()
}

/// The revised range that the edit `e` writes, given the sum of the length differences of the edits before it.
pub(super) fn revised_range(e: &Edit, delta: isize) -> Option<std::ops::Range<usize>> {
    let start = usize::try_from(isize::try_from(e.range.start).ok()? + delta).ok()?;
    Some(start..start + e.with.chars().count())
}

/// One replacement in both coordinates: the original's range that is deleted (empty for a pure insertion) and the revised
/// range that is inserted (empty for a pure deletion). The text between two replacements is equal on both sides, so the
/// distance `a.start - b.start` is the same at the two ends of a stretch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Replace {
    /// The original's characters that are deleted.
    pub a: std::ops::Range<usize>,
    /// The revised characters that are inserted.
    pub b: std::ops::Range<usize>,
}

/// The replacements that turn `old` into `new`, widened so that none cuts a field.
///
/// `spans_a` and `spans_b` are the character ranges of the fields of each side that lie inside the paragraph. An edit that
/// overlaps a field without covering it, or that is an insertion strictly inside one, takes the whole field on its own side
/// and the same stretch on the other (the text outside edits corresponds one for one), and edits that then touch are one.
pub(super) fn replacements(
    old: &str,
    new: &str,
    spans_a: &[std::ops::Range<usize>],
    spans_b: &[std::ops::Range<usize>],
) -> Vec<Replace> {
    let mut delta: isize = 0;
    let mut out: Vec<Replace> = Vec::new();
    for e in word_edits(old, new) {
        let Some(b) = revised_range(&e, delta) else {
            continue;
        };
        let with = isize::try_from(e.with.chars().count()).unwrap_or(0);
        let cut = isize::try_from(e.range.len()).unwrap_or(0);
        delta += with - cut;
        out.push(Replace { a: e.range, b });
    }
    loop {
        let mut changed = false;
        for r in &mut out {
            for s in spans_a {
                if cuts(&r.a, s) {
                    let start = r.a.start.min(s.start);
                    let end = r.a.end.max(s.end);
                    r.b.start = shift(r.b.start, r.a.start, start);
                    r.b.end = shift(r.b.end, r.a.end, end);
                    r.a = start..end;
                    changed = true;
                }
            }
            for s in spans_b {
                if cuts(&r.b, s) {
                    let start = r.b.start.min(s.start);
                    let end = r.b.end.max(s.end);
                    r.a.start = shift(r.a.start, r.b.start, start);
                    r.a.end = shift(r.a.end, r.b.end, end);
                    r.b = start..end;
                    changed = true;
                }
            }
        }
        let before = out.len();
        out = merge_touching(out);
        if !changed && out.len() == before {
            return out;
        }
    }
}

/// True if `r` overlaps `field` without covering all of it, or is an empty range strictly inside it.
fn cuts(r: &std::ops::Range<usize>, field: &std::ops::Range<usize>) -> bool {
    if field.is_empty() {
        return false;
    }
    if r.is_empty() {
        return r.start > field.start && r.start < field.end;
    }
    let overlaps = r.start < field.end && field.start < r.end;
    overlaps && !(r.start <= field.start && r.end >= field.end)
}

/// `at` moved by the distance between `from` and `to`.
fn shift(at: usize, from: usize, to: usize) -> usize {
    if to <= from {
        at.saturating_sub(from - to)
    } else {
        at + (to - from)
    }
}

/// Edits that overlap or touch on either side are one edit, from the first start to the last end.
fn merge_touching(edits: Vec<Replace>) -> Vec<Replace> {
    let mut out: Vec<Replace> = Vec::with_capacity(edits.len());
    for e in edits {
        match out.last_mut() {
            Some(last) if e.a.start <= last.a.end || e.b.start <= last.b.end => {
                last.a.end = last.a.end.max(e.a.end);
                last.b.end = last.b.end.max(e.b.end);
            }
            _ => out.push(e),
        }
    }
    out
}

/// The offset in the revised text of the character at `at` in the original, through `replacements`, or `None` if a
/// replacement covers it.
pub(super) fn map_through(replacements: &[Replace], at: usize) -> Option<usize> {
    let mut delta: isize = 0;
    for r in replacements {
        if at < r.a.start {
            break;
        }
        if at < r.a.end {
            return None;
        }
        delta += isize::try_from(r.b.len()).ok()? - isize::try_from(r.a.len()).ok()?;
    }
    usize::try_from(isize::try_from(at).ok()? + delta).ok()
}
