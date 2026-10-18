//! The author and the date of an edit.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Who is editing, and when. It has no default, so it cannot be forgotten: every edit and every compare takes one.
///
/// The date is the wall-clock time of the date the caller gives, `YYYY-MM-DDTHH:MM:SS` with an optional fraction and an
/// optional `Z` or offset. What is written into a document is its first 19 characters and `Z`, which is Word's own
/// convention (its `w:date` is the author's local time labelled `Z`). A caller that passes UTC gets UTC, and the CLI and
/// a typical host application pass local time. The core reads no clock, and it validates the stamp when the edit runs: an empty or
/// over-long author, or a date that is not a real date and time, is `invalid_payload`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stamp {
    /// The author of the revisions and comments, as Word shows it. At most 255 characters.
    pub author: String,
    /// The wall-clock date and time, such as `2026-10-06T14:03:27`.
    pub date: String,
}

impl Stamp {
    /// A stamp from an author and a date. Nothing is checked until an edit uses it.
    pub fn new(author: impl Into<String>, date: impl Into<String>) -> Stamp {
        Stamp {
            author: author.into(),
            date: date.into(),
        }
    }
}
