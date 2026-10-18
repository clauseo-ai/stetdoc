//! Every refusal of the alignment, with its message.
//!
//! A message names the file and the line, says what was found and why it is not allowed, and says the
//! way out: the operation to use instead, or what to write. Problems of one kind are independent, so a
//! step collects up to five of them and reports them together, in file order, with the code of the
//! first, so that an agent can fix them in one go and not one a turn.

use crate::api::error::{ErrorCode, StetError};
use crate::projection::keys::{Key, UnitKey};
use crate::projection::model::ReadOnly;
use crate::refs::BlockRef;

/// At most this many problems are reported together.
const MOST: usize = 5;

/// Problems of one step, in file order.
#[derive(Default)]
pub(super) struct Problems {
    first: Option<ErrorCode>,
    /// The file and the line of the first problem, which the error carries in its own fields.
    at: Option<(String, u32)>,
    items: Vec<String>,
    next: Option<String>,
}

impl Problems {
    pub(super) fn push(
        &mut self,
        (code, message): (ErrorCode, String),
        next: &str,
        at: (&str, u32),
    ) {
        if self.first.is_none() {
            self.first = Some(code);
            self.at = Some((at.0.to_string(), at.1));
            self.next = Some(next.to_string());
        }
        self.items.push(message);
    }

    /// The error, or `Ok` when there was no problem.
    pub(super) fn finish(self) -> Result<(), StetError> {
        let Some(code) = self.first else {
            return Ok(());
        };
        let mut message = self
            .items
            .iter()
            .take(MOST)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        if self.items.len() > MOST {
            message.push_str(&format!("\n(and {} more)", self.items.len() - MOST));
        }
        let mut error = StetError::new(code, message);
        if let Some(next) = self.next {
            error = error.with_next(next);
        }
        if let Some((file, line)) = self.at {
            error = error.in_file(file, line);
        }
        Err(error)
    }
}

fn at(file: &str, line: u32) -> String {
    if line == 0 {
        file.to_string()
    } else {
        format!("{file} line {line}")
    }
}

fn why(reason: ReadOnly) -> &'static str {
    match reason {
        ReadOnly::DeletedBlock => "it is a deleted paragraph (another author's tracked deletion)",
        ReadOnly::LockedControl => "it is inside a locked content control",
        ReadOnly::DataBound => "it is inside a content control bound to data",
        ReadOnly::Placeholder => "it is a control's placeholder",
    }
}

pub(super) fn stale_header(file: &str, header: &str, current: &str) -> StetError {
    StetError::new(
        ErrorCode::StaleFiles,
        format!("{file} was written at base {header}, and the document is at {current}. The files are from another version."),
    )
    .with_next("Export the files again and redo the edit.")
    .in_file(file, 0)
}

pub(super) fn head_changed(file: &str, was: Option<&str>, now: Option<&str>) -> StetError {
    let show = |style: Option<&str>| match style {
        Some(s) => format!("common-style={s:?}"),
        None => "no common-style".to_string(),
    };
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{file}: the head line was changed: it had {} and now has {}. The head is read-only: the style of every line with no style tag follows from it.",
            show(was),
            show(now)
        ),
    )
    .with_next("Put the head back as it was exported. To change the style of a paragraph use the `style` operation.")
    .in_file(file, 0)
}

pub(super) fn file_twice(file: &str) -> StetError {
    StetError::new(
        ErrorCode::InvalidPayload,
        format!("{file} was given twice, and the two copies may not agree about what the edit is."),
    )
    .with_next("Give each file once, with all of its edits.")
    .in_file(file, 0)
}

pub(super) fn no_such_file(file: &str) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!("{file} does not exist for this document, and a file cannot add a note, a header or a comment."),
    )
    .with_next("Use the `comment` operations for comments. A footnote, a header or a footer is added in Word.")
    .in_file(file, 0)
}

pub(super) fn duplicate_ref(
    file: &str,
    r: BlockRef,
    first: u32,
    second: u32,
) -> (ErrorCode, String) {
    (
        ErrorCode::InvalidPayload,
        format!(
            "{}: {r} is on lines {first} and {second}. To add a paragraph write a line with no ref.",
            file
        ),
    )
}

pub(super) fn other_file(file: &str, line: u32, r: BlockRef, other: &str) -> (ErrorCode, String) {
    (
        ErrorCode::InvalidPayload,
        format!(
            "{}: {r} belongs to {other}, not to {file}. Put the line back in {other}.",
            at(file, line)
        ),
    )
}

pub(super) fn unknown_ref(file: &str, line: u32, r: BlockRef, range: &str) -> (ErrorCode, String) {
    (
        ErrorCode::NotFound,
        format!(
            "{}: {r} was never a paragraph of this document. {range}",
            at(file, line)
        ),
    )
}

pub(super) fn gone_ref(file: &str, line: u32, r: BlockRef, what: &str) -> (ErrorCode, String) {
    (
        ErrorCode::StaleRef,
        format!("{}: {r} is gone. {what}", at(file, line)),
    )
}

pub(super) fn moved(file: &str, line: u32, r: &str, before: &str) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{}: {r} is before {before}, and it was after it. Paragraphs cannot be moved by editing a file.",
            at(file, line)
        ),
    )
    .with_next("Delete the line and insert the text again, or use an operation.")
    .in_file(file, line)
}

pub(super) fn moved_across(file: &str, line: u32, what: &str) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{}: {what} moved across a line that is structure (the end of a table, a row, a section or a note). Paragraphs cannot be moved by editing a file.",
            at(file, line)
        ),
    )
    .with_next("Delete the line and insert the text again, or use an operation.")
    .in_file(file, line)
}

pub(super) fn structure_unbalanced(file: &str, line: u32, what: &str) -> StetError {
    StetError::new(
        ErrorCode::InvalidPayload,
        format!("{}: {what}", at(file, line)),
    )
    .with_next("Remove a table or a row whole, with its paragraphs and its end line, or leave it as it is.")
    .in_file(file, line)
}

pub(super) fn new_structure(file: &str, line: u32, what: &str) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{}: {what} A table, a row, a section or a note cannot be created by editing a file.",
            at(file, line)
        ),
    )
    .with_next("Use the `row` operation to add a row, and the operations for tables and sections.")
    .in_file(file, line)
}

pub(super) fn structure_changed(file: &str, line: u32, was: &Key, now: &Key) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{}: {} is read-only, and it was changed (it was {}). Structure lines belong to the document.",
            at(file, line),
            now.describe(),
            was.describe()
        ),
    )
    .with_next("Put the line back as it was. Tables, rows and sections change with operations.")
    .in_file(file, line)
}

pub(super) fn structure_missing(file: &str, was: &Key) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{file}: {} was removed, and structure lines are read-only.",
            was.describe()
        ),
    )
    .with_next("Put the line back as it was.")
    .in_file(file, 0)
}

pub(super) fn note_or_comment_removed(file: &str, was: &Key) -> StetError {
    if let Key::Unit(UnitKey::Note {
        endnote, ordinal, ..
    }) = was
    {
        let kind = if *endnote { "endnote" } else { "footnote" };
        return StetError::new(
            ErrorCode::Unsupported,
            format!(
                "{file}: {kind} {ordinal} was removed. A note is not removed by taking its lines out of a file."
            ),
        )
        .with_next(format!(
            "Use the `delete-note` operation with the kind `{kind}` and the ordinal {ordinal}."
        ))
        .in_file(file, 0);
    }
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{file}: {} was removed. A note, a header or a comment cannot be added or removed by editing a file.",
            was.describe()
        ),
    )
    .with_next("Use the `comment` operations for comments. Headers and footers are changed in Word.")
    .in_file(file, 0)
}

pub(super) fn tag_changed(
    file: &str,
    line: u32,
    r: BlockRef,
    was: Option<&Key>,
    now: Option<&Key>,
) -> StetError {
    let what = match (was, now) {
        (Some(w), Some(n)) => format!("{} was changed to {}", w.describe(), n.describe()),
        (Some(w), None) => format!("{} was removed", w.describe()),
        (None, Some(n)) => format!("{} was added", n.describe()),
        (None, None) => "a tag changed".to_string(),
    };
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!("{} ({r}): {what}, and tags are read-only.", at(file, line)),
    )
    .with_next("Edit the text after the tags. Styles, numbers and tracked-change marks change with operations (`style`, `list`, `accept`, `reject`).")
    .in_file(file, line)
}

pub(super) fn seam_changed(
    file: &str,
    line: u32,
    r: BlockRef,
    was: Option<&Key>,
    now: Option<&Key>,
) -> StetError {
    let what = match (was, now) {
        (Some(w), Some(n)) => format!("{} was changed to {}", w.describe(), n.describe()),
        (Some(w), None) => format!("{} was removed", w.describe()),
        (None, Some(n)) => format!("{} was added", n.describe()),
        (None, None) => "a marker changed".to_string(),
    };
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!("{} ({r}): {what}. Markers in braces are read-only: they are the document's, not text.", at(file, line)),
    )
    .with_next("To undo someone else's tracked change use `reject`, to keep it use `accept`. Write the text around the marker, and leave the marker as it is.")
    .in_file(file, line)
}

pub(super) fn slot_of_readonly(file: &str, line: u32, r: BlockRef, reason: ReadOnly) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{} ({r}): the text cannot be changed, because {}.",
            at(file, line),
            why(reason)
        ),
    )
    .with_next("Leave the paragraph as it is.")
    .in_file(file, line)
}

pub(super) fn removed_readonly(file: &str, r: BlockRef, reason: ReadOnly) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{file}: the line of {r} was removed, and it cannot be, because {}.",
            why(reason)
        ),
    )
    .with_next("Put the line back. To remove a deleted paragraph for good, `accept` the deletion.")
    .in_file(file, 0)
}

pub(super) fn marker_in_new_line(file: &str, line: u32) -> (ErrorCode, String) {
    (
        ErrorCode::InvalidPayload,
        format!(
            "{}: a new line holds text only. Markers belong to the document. If you copied a line, write its text without the markers.",
            at(file, line)
        ),
    )
}

pub(super) fn tag_on_new_line(file: &str, line: u32) -> (ErrorCode, String) {
    (
        ErrorCode::InvalidPayload,
        format!(
            "{}: a new line holds text only: a style, a number or a tag cannot be written on it. A new paragraph takes the style Word gives after the line before it; change it with the `style` operation.",
            at(file, line)
        ),
    )
}

pub(super) fn new_line_in_comments(file: &str, line: u32) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{}: comments.md is read-only, so a line cannot be added to it.",
            at(file, line)
        ),
    )
    .with_next("Use the `comment` operations to add, reply to, resolve or delete a comment.")
    .in_file(file, line)
}

pub(super) fn comments_changed(file: &str, line: u32) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{}: comments.md is read-only, and this line differs from what was written.",
            at(file, line)
        ),
    )
    .with_next("Put the file back as it was. Use the `comment` operations to change a comment.")
    .in_file(file, line)
}

pub(super) fn anchor_ambiguous(file: &str, line: u32, why: &str) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{}: a new line here is ambiguous: {why}. Use the `insert` operation, which says exactly where the paragraph goes.",
            at(file, line)
        ),
    )
    .with_next(
        "Use the `insert` operation with an anchor, which says exactly where the paragraph goes.",
    )
    .in_file(file, line)
}

pub(super) fn new_line_next_to_edit(file: &str, line: u32, r: BlockRef) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{}: a new line is next to {r}, which this edit splits or joins. Where the new paragraph goes would depend on the order of the two.",
            at(file, line)
        ),
    )
    .with_next("Make the split or the join first, then add the line in a second edit.")
    .in_file(file, line)
}

pub(super) fn ref_dropped_text_kept(file: &str, line: u32, r: BlockRef) -> StetError {
    StetError::new(
        ErrorCode::ReadonlyRegion,
        format!(
            "{}: the ref {r} was taken off this line and its text left in place. That would delete {r} and insert a new paragraph with the same text, which has none of its style, numbering or tracked changes.",
            at(file, line)
        ),
    )
    .with_next(format!(
        "To edit {r} put `{r}` back at the start of the line. To delete it, remove the whole line."
    ))
    .in_file(file, line)
}

pub(super) fn split_ref_on_later_line(file: &str, line: u32, r: BlockRef) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{}: the text of {r} was cut into lines, and {r} is on a later line than the first. A split keeps the ref on the first part.",
            at(file, line)
        ),
    )
    .with_next(format!("Put {r} on the first line and write the other parts as new lines with no ref."))
    .in_file(file, line)
}

pub(super) fn split_position_ambiguous(file: &str, line: u32, r: BlockRef, key: &Key) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{} ({r}): the cut is next to {}, which takes no room in the text, so which side of it the cut falls on cannot be told from the text.",
            at(file, line),
            key.describe()
        ),
    )
    .with_next("Cut somewhere else, or use the `split` operation at an offset and check the result.")
    .in_file(file, line)
}

pub(super) fn merge_ambiguous(file: &str, line: u32, r: BlockRef, key: &Key) -> StetError {
    StetError::new(
        ErrorCode::Unsupported,
        format!(
            "{} ({r}): the lines were joined with a space next to {}, which takes no room in the text, so where the space goes cannot be told.",
            at(file, line),
            key.describe()
        ),
    )
    .with_next("Join the lines without adding a space, or use the `merge` and `replace` operations.")
    .in_file(file, line)
}

/// A recognized split or merge that the compiler would refuse, found from the baseline.
pub(super) fn edit_refused(
    file: &str,
    line: u32,
    code: ErrorCode,
    message: String,
    next: &str,
) -> StetError {
    StetError::new(code, format!("{}: {message}", at(file, line)))
        .with_next(next)
        .in_file(file, line)
}

pub(super) fn readonly_why(reason: ReadOnly) -> &'static str {
    why(reason)
}
