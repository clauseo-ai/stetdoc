//! What `Document::occurrences` returns: every place a text is in the document as it reads now, so that a caller can
//! name it with an operation's `ref`, `find` and `occurrence` against the same state.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One place a text is in a paragraph's Final text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Occurrence {
    /// The ref of the paragraph.
    pub block: String,
    /// The story: `body`, `notes`, `headers` or `comments`.
    pub story: String,
    /// The full label of the clause the paragraph is in, such as `11.1(a)`, when it is in one.
    pub clause: Option<String>,
    /// Which match of the text in the paragraph it is, counted from 1: the `occurrence` an operation names it with.
    pub occurrence: u32,
    /// The first character of the match, as an offset in the paragraph's Final text.
    pub start: u32,
    /// One past the last character of the match.
    pub end: u32,
    /// The match with up to 30 characters on each side, the match in square brackets, on one line.
    pub snippet: String,
    /// True when only the second try matched: curly and straight quotes taken as the same, as an operation's `find` does.
    pub loose: bool,
}

/// Every place a text is, at one state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Occurrences {
    /// The base token of the state the answer describes.
    pub base: String,
    /// The text, as asked.
    pub needle: String,
    /// Every match in the Final text, in document order. Text that a tracked insertion added counts, and text that a
    /// tracked deletion removed does not.
    pub hits: Vec<Occurrence>,
    /// The paragraphs where the text is only in text a tracked deletion removes, one entry each with its first match in
    /// the Original text. They are not targets, and are listed so that a caller can say why the text was not found.
    pub deleted: Vec<Occurrence>,
}
