//! The options of `open`, `resume`, `compare` and `edit`.

use super::limits::Limits;
use super::stamp::Stamp;
use super::view::FormatId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What `Document::open` and `Document::resume` take. Every field has a default, and a partial JSON object keeps the
/// default of each field it leaves out.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct OpenOptions {
    /// The bounds on untrusted input, which apply to `open`, `resume`, `compare`, readable files and batches.
    pub limits: Limits,
    /// The format of the working copy that `export` writes, that receipts show blocks in and that `lookup` writes its text
    /// in. A resumed document keeps the format recorded in its complement, and this applies to `open` only.
    pub format: FormatId,
}

/// What `compare` takes. The stamp is required, because a redline has an author and a date.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CompareOptions {
    /// The bounds on untrusted input, applied to both packages.
    #[serde(default)]
    pub limits: Limits,
    /// The author and the date of the revisions of the redline.
    pub stamp: Stamp,
    /// The format of the working copy of the redline.
    #[serde(default)]
    pub format: FormatId,
}

/// What an edit does with the quotes of the text it writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Quotes {
    /// Follow the paragraph: straight quotes typed into a paragraph that uses curly ones become curly, curly ones typed into a
    /// straight paragraph become straight, and a quote after a digit stays as typed. Each change is reported.
    #[default]
    Match,
    /// Keep every character as typed.
    AsTyped,
}

/// How an edit shows the text it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Markup {
    /// Word revisions: tracked changes, or no marks at all for an untracked edit.
    #[default]
    Revisions,
    /// Manual markup, for a reader who asked for a redline without Track Changes. Deleted text stays in the document, struck
    /// through, and new text goes in underlined, as direct run formatting, and the edit writes no revision. The rest of the
    /// edit is untracked.
    Manual,
    /// Manual markup in colour: deleted text is also red and new text also blue, the colours of a law firm's blackline.
    ManualColor,
}

impl Markup {
    /// True for `Revisions`, the default, which a receipt leaves out of its JSON.
    pub fn is_revisions(&self) -> bool {
        *self == Markup::Revisions
    }

    /// True for either kind of manual markup.
    pub fn is_manual(&self) -> bool {
        !self.is_revisions()
    }
}

/// The unit that the offsets of a batch count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OffsetUnit {
    /// Unicode scalar values in the block's Final text: what the compiler counts.
    #[default]
    Scalar,
    /// UTF-16 code units in the block's Final text, as a JavaScript string counts them.
    Utf16,
}
