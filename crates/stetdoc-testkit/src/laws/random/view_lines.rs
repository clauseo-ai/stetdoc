//! The block lines of the body that L5 compares, and the comparison itself.
//!
//! Two things in a view depend on more than the block a line is about, and are taken out of the comparison:
//! - **The common style.** A paragraph in the document's commonest style carries no style tag, and which style is commonest is
//!   counted over the whole document, so one paragraph more or less can change it and the tag of every other line. Each line
//!   gets its style tag back from its own page's `common-style`, so a real change of a paragraph's style still shows.
//! - **The paragraph that W9 asks for.** A cell must end with a paragraph, so a reject that removes the paragraph inserted after
//!   a table that ended its cell leaves an empty one there (design `revisions.md`, step 3 of the resolver). After a reject, one
//!   such paragraph more than the original had is no difference, and nothing else is.

use super::{without_comment_marks, without_refs};
use crate::laws::files::{cell_end_stubs, without_tags};

/// A block line as L5 compares it, and whether it is the empty paragraph that ends a cell right after a table in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ViewLine {
    /// The line without refs and comment markers, with its style tag.
    pub text: String,
    /// True for the empty last paragraph of a cell, right after a table.
    pub filler: bool,
}

/// The block lines of a page of the view, without refs and without comment markers, each with its style tag. A long block is
/// one logical line that the view writes as several physical ones (a continuation ends in an odd number of backslashes, design
/// 5.3), and where it breaks depends on the length of the block's ref, so the continuations are joined before the refs are
/// taken out: two views that differ only in a ref compare equal.
pub(super) fn block_lines(page: &str) -> Vec<ViewLine> {
    let text = with_style_tags(&joined(page));
    text.lines()
        .zip(cell_end_stubs(&text))
        .filter(|(l, _)| l.split(' ').next().is_some_and(super::is_ref))
        .map(|(l, filler)| ViewLine {
            text: without_refs(&without_comment_marks(l)),
            filler,
        })
        .collect()
}

/// A page with the style tag written on every paragraph line that has none: the page's `common-style`, which a paragraph in
/// that style does not repeat.
pub(super) fn with_style_tags(page: &str) -> String {
    let common = page.lines().next().and_then(|head| {
        let start = head.find(" common-style=\"")? + " common-style=\"".len();
        let rest = head.get(start..)?;
        Some(rest.get(..rest.find('"')?)?.to_string())
    });
    let Some(common) = common else {
        return page.to_string();
    };
    let tag = format!("{{style \"{common}\"}}");
    page.split('\n')
        .map(|line| {
            let (r, rest) = line.split_once(' ').unwrap_or((line, ""));
            let paragraph = r
                .strip_prefix('p')
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            if !paragraph || rest.starts_with("{style \"") {
                line.to_string()
            } else if rest.is_empty() {
                format!("{r} {tag}")
            } else {
                format!("{r} {tag} {rest}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A text with its continuations joined: a physical line that ends in an odd number of backslashes loses the last one and runs
/// on into the next line.
pub(super) fn joined(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut runs_on = false;
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 && !runs_on {
            out.push('\n');
        }
        runs_on = line.chars().rev().take_while(|c| *c == '\\').count() % 2 == 1;
        out.push_str(if runs_on {
            line.get(..line.len() - 1).unwrap_or_default()
        } else {
            line
        });
    }
    out
}

/// A story whose only paragraph is empty has no lines: a story is never left with no paragraph, so removing everything in it
/// (accepting the deletion of the body's only table) keeps one empty paragraph, which Word shows as well.
pub(super) fn lone_empty_is_none(lines: Vec<ViewLine>) -> Vec<ViewLine> {
    match lines.as_slice() {
        [only] if only.text.trim().is_empty() || without_tags(only.text.trim()).trim() == "#" => {
            Vec::new()
        }
        _ => lines,
    }
}

/// Compares the lines of the original with those after a reject. They are the same, or the same but for one empty paragraph
/// that ends a cell right after a table, which the reject added where the original's cell ended with the table. Otherwise the
/// index of the first line that differs.
pub(super) fn restored(original: &[ViewLine], after: &[ViewLine]) -> Result<(), usize> {
    let texts = |v: &[ViewLine]| v.iter().map(|l| l.text.clone()).collect::<Vec<_>>();
    let a = texts(original);
    let b = texts(after);
    if a == b {
        return Ok(());
    }
    let first = a
        .iter()
        .zip(&b)
        .position(|(p, q)| p != q)
        .unwrap_or(a.len().min(b.len()));
    if b.len() == a.len() + 1
        && after.get(first).is_some_and(|l| l.filler)
        && a.get(first..) == b.get(first + 1..)
    {
        return Ok(());
    }
    Err(first)
}
