//! JSON in and out of the addon, the requests that only the binding has, and how a failure is thrown.

use napi::Error;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::panic::{AssertUnwindSafe, catch_unwind};
use stetdoc_core::{ErrorCode, Markup, OffsetUnit, Quotes, ReadableFiles, StetError};

/// The prefix of the message of a thrown `StetError`, which `npm/index.js` looks for.
const PREFIX: &str = "stetdoc:";

/// Runs one call of the binding: a panic becomes `internal`, and a `StetError` becomes a thrown error.
pub(crate) fn guard<T>(call: impl FnOnce() -> Result<T, StetError>) -> napi::Result<T> {
    let outcome = catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| {
        Err(StetError::new(
            ErrorCode::Internal,
            "stetdoc panicked while handling this call.",
        ))
    });
    outcome.map_err(|error| thrown(&error))
}

/// The JavaScript error that carries a `StetError`.
pub(crate) fn thrown(error: &StetError) -> Error {
    let json = serde_json::to_string(error).unwrap_or_else(|_| {
        format!(
            "{{\"code\":\"{}\",\"message\":\"(unserializable)\",\"next\":\"\"}}",
            error.code()
        )
    });
    Error::from_reason(format!("{PREFIX}{json}"))
}

/// Reads a JSON argument into a core type. A bad shape is `invalid_payload`, naming what was being read.
pub(crate) fn parse<T: DeserializeOwned>(what: &str, json: &str) -> Result<T, StetError> {
    serde_json::from_str(json).map_err(|error| describe(what, &error))
}

/// What is wrong with a JSON argument. A JavaScript string can hold half of a surrogate pair, which `JSON.stringify` writes as
/// an escape that is not a character, so that case is named in words and not as a JSON syntax error.
fn describe(what: &str, error: &serde_json::Error) -> StetError {
    let text = error.to_string();
    if text.contains("surrogate") || text.contains("hex escape") {
        StetError::new(
            ErrorCode::InvalidPayload,
            format!(
                "The {what} has text with an unpaired surrogate (half of a UTF-16 character, such as one half of an emoji) \
                 at line {} column {}, which is not text.",
                error.line(),
                error.column()
            ),
        )
        .with_next("Pass whole characters: a string sliced through the middle of an emoji leaves half of it behind.")
    } else {
        StetError::new(
            ErrorCode::InvalidPayload,
            format!("The {what} is not valid: {text}."),
        )
    }
}

/// Writes a result as JSON. The core's types always serialize, so a failure is `internal`.
pub(crate) fn json<T: Serialize>(value: &T) -> Result<String, StetError> {
    serde_json::to_string(value).map_err(|error| {
        StetError::new(
            ErrorCode::Internal,
            format!("A result did not serialize: {error}."),
        )
    })
}

fn utf16() -> OffsetUnit {
    OffsetUnit::Utf16
}

/// What `Document.edit` sends: the source of the edit, the stamp, and the options of the `Edit` builder.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditRequest {
    /// The base token of `ops`.
    pub base: Option<String>,
    /// Operations as JSON values, read with `base` by `Batch::parse`.
    pub ops: Option<serde_json::Value>,
    /// A whole batch, in JSON or block text.
    pub batch: Option<String>,
    /// Hand-edited readable files.
    pub files: Option<ReadableFiles>,
    /// The author of the revisions.
    pub author: String,
    /// The date of the revisions.
    pub date: String,
    /// Write no revisions.
    #[serde(default)]
    pub untracked: bool,
    /// How text changes show: revisions, or manual markup (strikethrough and underline, no revision).
    #[serde(default)]
    pub markup: Markup,
    /// The scope guard.
    #[serde(default)]
    pub scope: Vec<String>,
    /// Run the edit and drop the result.
    #[serde(default)]
    pub dry_run: bool,
    /// What to do with typed quotes.
    #[serde(default)]
    pub quotes: Quotes,
    /// The unit of the offsets of `ops` and `batch`. JavaScript counts UTF-16 code units, so that is the default.
    #[serde(default = "utf16")]
    pub offsets: OffsetUnit,
}

/// What `Document.checkPending` sends.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CheckPendingRequest {
    /// The pending files, if any.
    pub pending: Option<ReadableFiles>,
    /// The author of the dry run's revisions.
    pub author: Option<String>,
    /// The date of the dry run's revisions.
    pub date: Option<String>,
    /// Whether an existing violation fails the check.
    #[serde(default)]
    pub strict: bool,
}

/// What `Document.textEditor` sends besides the tool's input.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditorOptions {
    /// The author of the revisions.
    pub author: String,
    /// The date of the revisions.
    pub date: String,
    /// The virtual directory of the files, such as `/documents/acme/`. Empty puts them at the root.
    #[serde(default)]
    pub path_prefix: String,
}
