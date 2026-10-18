//! The table that describes every operation: its name, its fields and what kind each is, whether it is trackable and
//! whether it is reversible.
//!
//! One table drives the block-text parser, `capabilities()`, the generated operations reference
//! (`docs/reference/operations.md`), the skill's table and the conformance suite, so none of them lists operations by hand.
//! It is written as one macro invocation of literals so that `cargo xtask docs` can read it with `syn` without compiling the
//! core, as it reads `error_codes!`. `Op::name` is an exhaustive match over the variants, so a variant with no entry here does
//! not compile.
//!
//! **Adding an operation.** Add a payload and a variant in `op.rs`, an entry below, and a file in `ops/`. A test checks that the
//! example of each entry reads as the operation of that name, and that its fields are the fields of the payload.

use super::op::Op;

/// What kind of value a field holds, which is what the block-text parser needs to read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// A ref (`p42`, `t3`, `r17`), a comment (`c3`) or a note (`fn3`): one line.
    Ref,
    /// Text. One line, or several in a heredoc.
    Text,
    /// A whole number.
    Number,
    /// `true` or `false`.
    Bool,
    /// One of a few words, listed in the field's choices.
    Choice,
    /// A list of texts: one for each line of a heredoc, or the key repeated.
    List,
    /// A list of whole numbers.
    NumberList,
    /// A list of property elements, one for each line: `name`, then an optional value, then `attribute=value` pairs.
    Props,
}

impl FieldKind {
    /// The word that names the kind in the reference and in `capabilities()`.
    pub const fn as_str(self) -> &'static str {
        match self {
            FieldKind::Ref => "ref",
            FieldKind::Text => "text",
            FieldKind::Number => "number",
            FieldKind::Bool => "bool",
            FieldKind::Choice => "choice",
            FieldKind::List => "list",
            FieldKind::NumberList => "number list",
            FieldKind::Props => "properties",
        }
    }
}

/// One field of an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldSpec {
    /// The key, as JSON and block text spell it.
    pub name: &'static str,
    /// What kind of value it holds.
    pub kind: FieldKind,
    /// Whether the operation needs it.
    pub required: bool,
    /// What it means. A text field says what a newline in it means.
    pub doc: &'static str,
    /// For a choice, the words it may be.
    pub choices: &'static [&'static str],
}

/// One operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpSpec {
    /// The name, as the `op` field spells it.
    pub name: &'static str,
    /// What it does, in a sentence.
    pub summary: &'static str,
    /// Whether it writes revisions. A comment operation and a resolution do not.
    pub trackable: bool,
    /// Whether rejecting everything the batch wrote restores the original content and structure.
    pub reversible: bool,
    /// Its fields.
    pub fields: &'static [FieldSpec],
    /// A group of optional fields of which exactly one must be given. Empty when there is none.
    pub one_of: &'static [&'static str],
    /// An example, as JSON, that reads as this operation.
    pub example: &'static str,
}

macro_rules! op_specs {
    ($(
        $variant:ident {
            name: $name:literal,
            summary: $summary:literal,
            trackable: $trackable:literal,
            reversible: $reversible:literal,
            one_of: [$($one:literal),* $(,)?],
            example: $example:literal,
            fields: [$( ($fname:literal, $kind:ident, $required:literal, $doc:literal, $choices:expr) ),* $(,)?],
        }
    )*) => {
        // One static for each entry, named after the `Op` variant, so that `Op::spec` can be an exhaustive match and a
        // variant with no entry does not compile.
        #[expect(
            non_upper_case_globals,
            reason = "each static is named after its Op variant, so that the match in Op::spec is exhaustive"
        )]
        mod specs {
            use super::{FieldKind, FieldSpec, OpSpec};
            $(
                pub static $variant: OpSpec = OpSpec {
                    name: $name,
                    summary: $summary,
                    trackable: $trackable,
                    reversible: $reversible,
                    fields: &[$(
                        FieldSpec { name: $fname, kind: FieldKind::$kind, required: $required, doc: $doc, choices: $choices },
                    )*],
                    one_of: &[$($one),*],
                    example: $example,
                };
            )*
        }

        /// Every operation, in the order of the scope.
        pub static OPS: &[&OpSpec] = &[$(&specs::$variant),*];

        impl Op {
            /// The name of the operation, as the `op` field spells it.
            pub const fn name(&self) -> &'static str {
                match self {
                    $(Op::$variant(_) => $name,)*
                }
            }

            /// The entry of the table that describes this operation.
            pub fn spec(&self) -> &'static OpSpec {
                match self {
                    $(Op::$variant(_) => &specs::$variant,)*
                }
            }
        }
    };
}

op_specs! {
    Replace {
        name: "replace",
        summary: "Replaces a text range, or the whole paragraph, with new text. The difference is made word by word.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"replace\",\"ref\":\"p42\",\"find\":\"shall\",\"with\":\"must\"}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("with", Text, true, "The new text. A newline is a line break inside the paragraph, and a tab is written as a tab.", &[]),
            ("find", Text, false, "The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. A newline matches a line break of the paragraph.", &[]),
            ("occurrence", Number, false, "Which match of `find` to use, counted from 1.", &[]),
            ("start", Number, false, "The first character of the range, as an offset in the Final text. Not with `find`.", &[]),
            ("end", Number, false, "One past the last character of the range, as an offset in the Final text. Not with `find`.", &[]),
        ],
    }
    Insert {
        name: "insert",
        summary: "Inserts new paragraphs before or after a paragraph or a table, in order.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"insert\",\"ref\":\"p42\",\"place\":\"after\",\"paragraphs\":[\"A new paragraph.\"]}",
        fields: [
            ("ref", Ref, true, "The paragraph or table the new paragraphs go next to.", &[]),
            ("place", Choice, true, "Whether they go before or after it.", &["before", "after"]),
            ("paragraphs", List, true, "The paragraphs, in order. Each is one paragraph of text, and a newline inside one is a line break of that paragraph. In JSON an entry may be an object with a text and a style, and in block text one style applies to all.", &[]),
        ],
    }
    Delete {
        name: "delete",
        summary: "Deletes a whole paragraph or table, a text range, or the blocks from one paragraph through another.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"delete\",\"ref\":\"p42\"}",
        fields: [
            ("ref", Ref, true, "The paragraph, table or first paragraph of the range.", &[]),
            ("to", Ref, false, "The last paragraph of a range that spans paragraphs. Then `start` is an offset in `ref` and `end` an offset in `to`.", &[]),
            ("find", Text, false, "The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. A newline matches a line break of the paragraph.", &[]),
            ("occurrence", Number, false, "Which match of `find` to use, counted from 1.", &[]),
            ("start", Number, false, "The first character of the range, as an offset in the Final text. Not with `find`.", &[]),
            ("end", Number, false, "One past the last character of the range, as an offset in the Final text. Not with `find`.", &[]),
        ],
    }
    Split {
        name: "split",
        summary: "Cuts a paragraph in two, at an offset or before or after some text.",
        trackable: true,
        reversible: true,
        one_of: ["at", "before", "after"],
        example: "{\"op\":\"split\",\"ref\":\"p42\",\"before\":\"Notwithstanding\"}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("at", Number, false, "The offset of the cut in the Final text.", &[]),
            ("before", Text, false, "Cut just before this text, which must match once. A newline matches a line break.", &[]),
            ("after", Text, false, "Cut just after this text, which must match once. A newline matches a line break.", &[]),
        ],
    }
    Merge {
        name: "merge",
        summary: "Joins a paragraph and the one after it.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"merge\",\"ref\":\"p42\"}",
        fields: [
            ("ref", Ref, true, "The paragraph that stays.", &[]),
            ("second", Ref, false, "The paragraph that is joined into it. By default the next paragraph that is not a deleted block.", &[]),
        ],
    }
    Format {
        name: "format",
        summary: "Changes the formatting of the runs in a range, or of the whole paragraph.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"format\",\"ref\":\"p42\",\"find\":\"Losses\",\"bold\":true}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("bold", Bool, false, "Bold on or off.", &[]),
            ("italic", Bool, false, "Italic on or off.", &[]),
            ("underline", Text, false, "The underline style: `single`, `double`, `none` and the others Word has.", &[]),
            ("strike", Bool, false, "Strike through on or off.", &[]),
            ("color", Text, false, "The text colour, `RRGGBB` or `auto`.", &[]),
            ("highlight", Text, false, "The highlight colour, by Word's name.", &[]),
            ("size", Text, false, "The font size with a unit, such as `12pt`.", &[]),
            ("font", Text, false, "The font name.", &[]),
            ("char_style", Text, false, "The id or name of a character style.", &[]),
            ("clear", List, false, "The names of formatting to remove, one for each entry.", &[]),
            ("find", Text, false, "The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. A newline matches a line break of the paragraph.", &[]),
            ("occurrence", Number, false, "Which match of `find` to use, counted from 1.", &[]),
            ("start", Number, false, "The first character of the range, as an offset in the Final text. Not with `find`.", &[]),
            ("end", Number, false, "One past the last character of the range, as an offset in the Final text. Not with `find`.", &[]),
        ],
    }
    Paragraph {
        name: "paragraph",
        summary: "Changes the formatting of a paragraph. Values take units such as 0.5in, 6pt and 1.5cm.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"paragraph\",\"ref\":\"p42\",\"align\":\"center\",\"space_after\":\"6pt\"}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("align", Choice, false, "The alignment.", &["left", "center", "right", "justify"]),
            ("indent_left", Text, false, "The left indent.", &[]),
            ("indent_right", Text, false, "The right indent.", &[]),
            ("first_line", Text, false, "The indent of the first line.", &[]),
            ("hanging", Text, false, "The hanging indent.", &[]),
            ("space_before", Text, false, "The space before the paragraph.", &[]),
            ("space_after", Text, false, "The space after the paragraph.", &[]),
            ("line_spacing", Text, false, "The line spacing.", &[]),
            ("keep_next", Bool, false, "Keep with the next paragraph.", &[]),
            ("keep_lines", Bool, false, "Keep the lines together.", &[]),
            ("page_break_before", Bool, false, "Start a new page before the paragraph.", &[]),
            ("clear", List, false, "The names of properties to remove, one for each entry.", &[]),
        ],
    }
    Style {
        name: "style",
        summary: "Sets the paragraph style.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"style\",\"ref\":\"p42\",\"style\":\"Heading 2\"}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("style", Text, true, "The style, by id or by name as the view shows it. One line.", &[]),
        ],
    }
    List {
        name: "list",
        summary: "Changes the level of a list item, restarts its numbering at a number, or takes it out of the list.",
        trackable: true,
        reversible: true,
        one_of: ["level", "restart", "remove"],
        example: "{\"op\":\"list\",\"ref\":\"p42\",\"level\":1}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("level", Number, false, "The new level, from 0.", &[]),
            ("restart", Number, false, "Restart the numbering at this number.", &[]),
            ("remove", Bool, false, "Take the paragraph out of its list.", &[]),
        ],
    }
    Set {
        name: "set",
        summary: "Sets any schema property of a paragraph, run, paragraph mark, section, table, row or cell.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"set\",\"ref\":\"p42\",\"props\":[{\"name\":\"jc\",\"val\":\"center\"}]}",
        fields: [
            ("props", Props, true, "The properties to set. In block text each line is a name, an optional value and `attribute=value` pairs, with no spaces inside a value. A property with children is JSON only.", &[]),
            ("target", Choice, false, "What to set it on. By default from the kind of `ref`.", &["paragraph", "run", "mark", "section", "table", "row", "cell"]),
            ("ref", Ref, false, "The paragraph, table or row. A section is named by `section` instead.", &[]),
            ("section", Number, false, "The number of the section, as `inspect sections` shows it, from 1.", &[]),
            ("cell", Number, false, "The cell of a row, from 1.", &[]),
            ("clear", List, false, "The names of properties to remove, one for each entry.", &[]),
            ("find", Text, false, "The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. A newline matches a line break of the paragraph.", &[]),
            ("occurrence", Number, false, "Which match of `find` to use, counted from 1.", &[]),
            ("start", Number, false, "The first character of the range, as an offset in the Final text. Not with `find`.", &[]),
            ("end", Number, false, "One past the last character of the range, as an offset in the Final text. Not with `find`.", &[]),
        ],
    }
    Row {
        name: "row",
        summary: "Inserts a table row, or deletes one.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"row\",\"ref\":\"r17\",\"action\":\"delete\"}",
        fields: [
            ("ref", Ref, true, "The row.", &[]),
            ("action", Choice, true, "Whether to insert a row next to this one or delete it.", &["insert", "delete"]),
            ("place", Choice, false, "For an insert, whether the new row goes before or after. After by default.", &["before", "after"]),
            ("cells", List, false, "For an insert, the text of each cell, in order. A newline inside one is a line break of the cell.", &[]),
        ],
    }
    Comment {
        name: "comment",
        summary: "Adds a comment anchored to a range, or to the whole paragraph.",
        trackable: false,
        reversible: false,
        one_of: [],
        example: "{\"op\":\"comment\",\"ref\":\"p42\",\"find\":\"Losses\",\"text\":\"Check this against schedule 2.\"}",
        fields: [
            ("ref", Ref, true, "The paragraph.", &[]),
            ("text", Text, true, "The text of the comment. A newline is a paragraph break of the comment, which has no line break inside a paragraph.", &[]),
            ("initials", Text, false, "The initials shown with the comment. By default the first letters of the author's words.", &[]),
            ("find", Text, false, "The text to find in the paragraph's Final text. It must match once, or `occurrence` says which match. A newline matches a line break of the paragraph.", &[]),
            ("occurrence", Number, false, "Which match of `find` to use, counted from 1.", &[]),
            ("start", Number, false, "The first character of the range, as an offset in the Final text. Not with `find`.", &[]),
            ("end", Number, false, "One past the last character of the range, as an offset in the Final text. Not with `find`.", &[]),
        ],
    }
    Reply {
        name: "reply",
        summary: "Replies to a comment thread.",
        trackable: false,
        reversible: false,
        one_of: [],
        example: "{\"op\":\"reply\",\"comment\":\"c3\",\"text\":\"Agreed.\"}",
        fields: [
            ("comment", Ref, true, "Any comment of the thread, such as `c3`.", &[]),
            ("text", Text, true, "The text of the reply. A newline is a paragraph break.", &[]),
        ],
    }
    Resolve {
        name: "resolve",
        summary: "Resolves a comment thread, or reopens it.",
        trackable: false,
        reversible: false,
        one_of: [],
        example: "{\"op\":\"resolve\",\"comment\":\"c3\"}",
        fields: [
            ("comment", Ref, true, "Any comment of the thread.", &[]),
            ("reopen", Bool, false, "Reopen the thread instead of resolving it.", &[]),
        ],
    }
    EditComment {
        name: "edit-comment",
        summary: "Replaces the text of one of the author's own comments.",
        trackable: false,
        reversible: false,
        one_of: [],
        example: "{\"op\":\"edit-comment\",\"comment\":\"c3\",\"text\":\"Checked.\"}",
        fields: [
            ("comment", Ref, true, "The comment.", &[]),
            ("text", Text, true, "The new text. A newline is a paragraph break.", &[]),
        ],
    }
    DeleteComment {
        name: "delete-comment",
        summary: "Deletes one of the author's own comments, with its replies.",
        trackable: false,
        reversible: false,
        one_of: [],
        example: "{\"op\":\"delete-comment\",\"comment\":\"c3\"}",
        fields: [
            ("comment", Ref, true, "The comment.", &[]),
        ],
    }
    DeleteNote {
        name: "delete-note",
        summary: "Deletes a footnote or endnote, with its reference.",
        trackable: true,
        reversible: true,
        one_of: [],
        example: "{\"op\":\"delete-note\",\"note\":\"fn3\"}",
        fields: [
            ("note", Ref, true, "The note, such as `fn3` or `en2`: its kind and its ordinal.", &[]),
        ],
    }
    Accept {
        name: "accept",
        summary: "Accepts revisions: applies them.",
        trackable: false,
        reversible: false,
        one_of: ["all", "author", "ids"],
        example: "{\"op\":\"accept\",\"author\":\"JD\"}",
        fields: [
            ("all", Bool, false, "Every revision.", &[]),
            ("author", Text, false, "Every revision of this author, by name or by the short code the view shows.", &[]),
            ("ids", NumberList, false, "The revisions with these ids, the numbers after `#` in the view. A change is resolved whole.", &[]),
        ],
    }
    Reject {
        name: "reject",
        summary: "Rejects revisions: undoes them.",
        trackable: false,
        reversible: false,
        one_of: ["all", "author", "ids"],
        example: "{\"op\":\"reject\",\"ids\":[7,8]}",
        fields: [
            ("all", Bool, false, "Every revision.", &[]),
            ("author", Text, false, "Every revision of this author, by name or by the short code the view shows.", &[]),
            ("ids", NumberList, false, "The revisions with these ids, the numbers after `#` in the view. A change is resolved whole.", &[]),
        ],
    }
}
