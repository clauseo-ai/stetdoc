//! What the model reads: numbered lines, the listing of an ambiguous match, the blocks an edit touched, and an error.

use crate::run::{Failure, Output};
use stetdoc_core::{ReadableFiles, Receipt};

/// The most matches an `ambiguous_match` lists.
const MAX_LISTED: usize = 10;

/// One numbered line, as `cat -n` writes it: the number right-aligned in six columns, a tab, the line.
fn line(out: &mut String, number: usize, text: &str) {
    out.push_str(&format!("{number:>6}\t{text}\n"));
}

/// Lines `start` to `end` of a text, from 1 and inclusive, numbered.
pub(crate) fn numbered(text: &str, (start, end): (usize, usize)) -> String {
    let mut out = String::new();
    for (index, content) in text.lines().enumerate() {
        let number = index + 1;
        if number >= start && number <= end {
            line(&mut out, number, content);
        }
    }
    out
}

/// Each match of an ambiguous `old_str`, by line, with a line of context on each side.
pub(crate) fn matches_listing(text: &str, lines: &[usize]) -> String {
    let count = text.lines().count();
    let mut out = String::new();
    for (index, at) in lines.iter().take(MAX_LISTED).enumerate() {
        out.push_str(&format!("Match {} at line {at}:\n", index + 1));
        out.push_str(&numbered(
            text,
            (at.saturating_sub(1).max(1), (at + 1).min(count)),
        ));
    }
    if lines.len() > MAX_LISTED {
        out.push_str(&format!("and {} more matches.\n", lines.len() - MAX_LISTED));
    }
    out
}

/// After an edit: what changed, each touched block numbered where it now is in the file, and the receipt's warnings. `name`
/// is the file's name and `shown` its path as the model sees it.
pub(crate) fn snippets(
    name: &str,
    shown: &str,
    receipt: &Receipt,
    after: &ReadableFiles,
) -> String {
    let changes: usize = receipt
        .revisions
        .iter()
        .map(|author| author.changes.len())
        .sum();
    let mut out = if receipt.tracked {
        format!(
            "{shown} was edited: {changes} tracked change{} by {}.\n",
            if changes == 1 { "" } else { "s" },
            receipt.stamp.author
        )
    } else {
        format!("{shown} was edited, untracked.\n")
    };
    let file: Vec<&str> = after
        .files
        .iter()
        .find(|(file, _)| file == name)
        .map(|(_, text)| text.lines().collect())
        .unwrap_or_default();
    if !receipt.blocks.is_empty() {
        out.push_str("The touched blocks now read:\n");
    }
    for block in &receipt.blocks {
        let lines: Vec<&str> = block.text.lines().collect();
        match locate(&file, &lines) {
            Some(start) => {
                for (offset, text) in lines.iter().enumerate() {
                    line(&mut out, start + offset + 1, text);
                }
            }
            None => {
                for text in &lines {
                    out.push_str(&format!("      \t{text}\n"));
                }
            }
        }
    }
    if receipt.blocks_omitted > 0 {
        out.push_str(&format!(
            "and {} more blocks; view the file to see them.\n",
            receipt.blocks_omitted
        ));
    }
    for warning in &receipt.warnings {
        out.push_str(&format!(
            "warning {}: {}\n",
            warning.code.as_str(),
            warning.message
        ));
    }
    out
}

/// The zero-based line of the file where a block's lines start.
fn locate(file: &[&str], block: &[&str]) -> Option<usize> {
    if block.is_empty() || block.len() > file.len() {
        return None;
    }
    (0..=file.len() - block.len()).find(|&start| {
        file.get(start..start + block.len())
            .is_some_and(|window| window == block)
    })
}

/// The text and the `is_error` flag of a `tool_result` block.
pub fn tool_result(result: &Result<Output, Failure>) -> (String, bool) {
    match result {
        Ok(output) => (output.text.clone(), false),
        Err(failure) => {
            let error = &failure.error;
            let mut text = format!("Error {error}\n");
            if let Some(detail) = &failure.detail {
                text.push_str(detail);
            }
            text.push_str(&format!("Next: {}\n", error.next()));
            (text, true)
        }
    }
}
