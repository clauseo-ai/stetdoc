//! The operations: one typed, validated edit instruction each.
//!
//! `Op` is a closed enum. JSON is serde over it, and the block-text parser builds the same value from the table in
//! `op_spec`, so a new operation is a variant here, an entry there and a file in `ops/`. Every payload refuses unknown
//! fields, so a typo is an `invalid_payload` and never a dropped instruction. A range inside a paragraph (`Within`) is the
//! four fields `find`, `occurrence`, `start` and `end`, written out on each payload by a macro and not flattened, because serde
//! cannot refuse an unknown field through `flatten`.
//!
//! Text offsets count Unicode scalar values in the block's Final text, or UTF-16 code units when the edit says so
//! (`Edit::offsets`). An agent names a range with `find`, and offsets are for programs.

use super::files::{NewParagraph, Place};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Defines a payload struct. Required fields come first, then `; opt` and the optional ones, which are `Option` and are
/// left out of the JSON when they are `None`.
macro_rules! payload {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$rm:meta])* $rf:ident : $rt:ty, )*
            ; opt
            $( $(#[$om:meta])* $of:ident : $ot:ty, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            $( $(#[$rm])* pub $rf: $rt, )*
            $(
                $(#[$om])*
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $of: Option<$ot>,
            )*
        }
    };
}

/// Like `payload!`, with the four fields of a text range added to the optional ones.
macro_rules! payload_within {
    (
        $(#[$meta:meta])*
        $name:ident {
            $( $(#[$rm:meta])* $rf:ident : $rt:ty, )*
            ; opt
            $( $(#[$om:meta])* $of:ident : $ot:ty, )*
        }
    ) => {
        payload! {
            $(#[$meta])*
            $name {
                $( $(#[$rm])* $rf: $rt, )*
                ; opt
                $( $(#[$om])* $of: $ot, )*
                /// The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match.
                find: String,
                /// Which match of `find` to use, counted from 1.
                occurrence: u32,
                /// The first character of the range, as an offset in the Final text.
                start: u32,
                /// One past the last character of the range, as an offset in the Final text.
                end: u32,
            }
        }
    };
}

payload_within! {
    /// Replaces a text range, or the whole paragraph, with new text. The compiler diffs by words, so a rewrite is a few
    /// insertions and deletions. A newline in `with` is a line break inside the paragraph, and `\t` is a tab.
    ReplaceOp {
        /// The paragraph.
        r#ref: String,
        /// The new text.
        with: String,
        ; opt
    }
}

payload! {
    /// Inserts new paragraphs before or after a paragraph or a table, in order.
    InsertOp {
        /// The paragraph or table the new paragraphs go next to.
        r#ref: String,
        /// Whether they go before or after it.
        place: Place,
        /// The paragraphs, each a text (and optionally a style) or a bare string.
        paragraphs: Vec<NewParagraph>,
        ; opt
    }
}

payload_within! {
    /// Deletes a whole paragraph or table, a text range of one paragraph, or the blocks from `ref` through `to`.
    DeleteOp {
        /// The paragraph, table or first paragraph of the range.
        r#ref: String,
        ; opt
        /// The last paragraph of a range that spans paragraphs. `start` is then an offset in `ref` and `end` an offset in `to`.
        to: String,
    }
}

payload! {
    /// Cuts a paragraph in two, at an offset or before or after some text. Exactly one of the three says where.
    SplitOp {
        /// The paragraph.
        r#ref: String,
        ; opt
        /// The offset of the cut in the Final text.
        at: u32,
        /// Cut just before this text, which must match once.
        before: String,
        /// Cut just after this text, which must match once.
        after: String,
    }
}

payload! {
    /// Joins a paragraph and the one after it.
    MergeOp {
        /// The paragraph that stays.
        r#ref: String,
        ; opt
        /// The paragraph that is joined into it. By default the next paragraph that is not a deleted block.
        second: String,
    }
}

payload_within! {
    /// Run formatting on a range, or on the whole paragraph.
    FormatOp {
        /// The paragraph.
        r#ref: String,
        ; opt
        /// Bold on or off.
        bold: bool,
        /// Italic on or off.
        italic: bool,
        /// The underline style: `single`, `double`, `none` and the others Word has.
        underline: String,
        /// Strike through on or off.
        strike: bool,
        /// The text colour, `RRGGBB` or `auto`.
        color: String,
        /// The highlight colour, by Word's name.
        highlight: String,
        /// The font size with a unit, such as `12pt`.
        size: String,
        /// The font name.
        font: String,
        /// The id or name of a character style.
        char_style: String,
        /// The names of formatting to remove.
        clear: Vec<String>,
    }
}

payload! {
    /// Paragraph formatting. The values take human units: `0.5in`, `6pt`, `1.5cm`.
    ParagraphOp {
        /// The paragraph.
        r#ref: String,
        ; opt
        /// `left`, `center`, `right` or `justify`.
        align: String,
        /// The left indent.
        indent_left: String,
        /// The right indent.
        indent_right: String,
        /// The indent of the first line.
        first_line: String,
        /// The hanging indent.
        hanging: String,
        /// The space before the paragraph.
        space_before: String,
        /// The space after the paragraph.
        space_after: String,
        /// The line spacing.
        line_spacing: String,
        /// Keep with the next paragraph.
        keep_next: bool,
        /// Keep the lines together.
        keep_lines: bool,
        /// Start a new page before the paragraph.
        page_break_before: bool,
        /// The names of properties to remove.
        clear: Vec<String>,
    }
}

payload! {
    /// Sets the paragraph style.
    StyleOp {
        /// The paragraph.
        r#ref: String,
        /// The style, by id or by name as the view shows it.
        style: String,
        ; opt
    }
}

payload! {
    /// Changes the list level of a list item, restarts its numbering, or takes it out of the list. Exactly one of the three.
    ListOp {
        /// The paragraph.
        r#ref: String,
        ; opt
        /// The new level, from 0.
        level: u32,
        /// Restart the numbering at this number.
        restart: u32,
        /// Take the paragraph out of its list.
        remove: bool,
    }
}

/// One property element for the generic setter: a name in the `w` namespace, an optional `w:val`, attributes and children.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PropSet {
    /// The element's local name, such as `spacing` or `jc`. A `w:` prefix is accepted.
    pub name: String,
    /// The `w:val` attribute, for the properties that have only that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub val: Option<String>,
    /// The other attributes, by local name. Values take human units where the schema has a unit.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attrs: BTreeMap<String, String>,
    /// Child elements, for the properties that have them (borders, tab stops).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PropSet>,
}

payload_within! {
    /// Sets any schema property of a paragraph, run, paragraph mark, section, table, row or cell.
    SetOp {
        /// The properties to set.
        props: Vec<PropSet>,
        ; opt
        /// What to set it on: `paragraph`, `run`, `mark`, `section`, `table`, `row` or `cell`. By default from the kind of `ref`.
        target: String,
        /// The paragraph, table or row. A section is named by `section` instead.
        r#ref: String,
        /// The number of the section, as `inspect sections` shows it, from 1.
        section: u32,
        /// The cell of a row, from 1.
        cell: u32,
        /// The names of properties to remove.
        clear: Vec<String>,
    }
}

/// What a `row` operation does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RowAction {
    /// Insert a row next to the one named.
    Insert,
    /// Delete the row.
    Delete,
}

payload! {
    /// Inserts a table row, or deletes one.
    RowOp {
        /// The row.
        r#ref: String,
        /// Whether to insert a row or delete this one.
        action: RowAction,
        ; opt
        /// For an insert, whether the new row goes before or after `ref`. After by default.
        place: Place,
        /// For an insert, the text of each cell. A newline in a cell is a line break.
        cells: Vec<String>,
    }
}

payload_within! {
    /// Adds a comment anchored to a range, or to the whole paragraph. A newline in `text` is a paragraph break of the comment.
    CommentOp {
        /// The paragraph.
        r#ref: String,
        /// The text of the comment.
        text: String,
        ; opt
        /// The initials shown with the comment. By default the first letters of the author's words.
        initials: String,
    }
}

payload! {
    /// Replies to a comment thread. A newline in `text` is a paragraph break.
    ReplyOp {
        /// Any comment of the thread, such as `c3`.
        comment: String,
        /// The text of the reply.
        text: String,
        ; opt
    }
}

payload! {
    /// Resolves a comment thread, or reopens it.
    ResolveOp {
        /// Any comment of the thread.
        comment: String,
        ; opt
        /// Reopen the thread instead of resolving it.
        reopen: bool,
    }
}

payload! {
    /// Replaces the text of one of the author's own comments. A newline in `text` is a paragraph break.
    EditCommentOp {
        /// The comment.
        comment: String,
        /// The new text.
        text: String,
        ; opt
    }
}

payload! {
    /// Deletes one of the author's own comments and its replies.
    DeleteCommentOp {
        /// The comment.
        comment: String,
        ; opt
    }
}

payload! {
    /// Deletes a footnote or endnote and its reference.
    DeleteNoteOp {
        /// The note, such as `fn3` or `en2`: its kind and its ordinal.
        note: String,
        ; opt
    }
}

payload! {
    /// Selects the revisions that `accept` or `reject` resolves. Exactly one of the three.
    ReviewOp {
        ; opt
        /// Every revision.
        all: bool,
        /// Every revision of this author, by name or by the short code the view shows.
        author: String,
        /// The revisions with these ids, the numbers after `#` in the view. A change is resolved whole.
        ids: Vec<i64>,
    }
}

/// One edit instruction. See `Op::spec` for the table that describes it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "kebab-case")]
#[non_exhaustive]
pub enum Op {
    /// Replace text.
    Replace(ReplaceOp),
    /// Insert paragraphs.
    Insert(InsertOp),
    /// Delete blocks or text.
    Delete(DeleteOp),
    /// Split a paragraph.
    Split(SplitOp),
    /// Merge two paragraphs.
    Merge(MergeOp),
    /// Format runs.
    Format(FormatOp),
    /// Format a paragraph.
    Paragraph(ParagraphOp),
    /// Set the paragraph style.
    Style(StyleOp),
    /// Change list membership.
    List(ListOp),
    /// Set any property.
    Set(SetOp),
    /// Insert or delete a table row.
    Row(RowOp),
    /// Add a comment.
    Comment(CommentOp),
    /// Reply to a comment.
    Reply(ReplyOp),
    /// Resolve or reopen a thread.
    Resolve(ResolveOp),
    /// Edit one's own comment.
    EditComment(EditCommentOp),
    /// Delete one's own comment.
    DeleteComment(DeleteCommentOp),
    /// Delete a note.
    DeleteNote(DeleteNoteOp),
    /// Accept revisions.
    Accept(ReviewOp),
    /// Reject revisions.
    Reject(ReviewOp),
}
