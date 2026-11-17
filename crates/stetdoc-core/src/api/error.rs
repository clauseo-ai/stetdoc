//! Error codes: the one place they live.
//!
//! Every failure in stetdoc is a [`StetError`]: a stable [`ErrorCode`], a one-line
//! message, a `next` hint and, for batches, the index of the failing operation. The
//! CLI, `--json` output, exit codes and the napi binding all map from this file.
//!
//! **Exit classes** live here too, although only the CLI uses them, so there is one
//! table. CLI argument errors are also exit class 2, but they belong to the CLI and
//! are not codes.
//!
//! **Policy.** From 1.0, codes and their exit classes are frozen. New codes may be
//! added. Existing codes are never repurposed or removed outside a major version.
//!
//! **Hints.** The default `next` hint of a code is surface neutral, so it reads correctly
//! for a library caller, the napi binding and the editor as well as the CLI. It names no
//! command. The CLI overrides it with [`StetError::with_next`] where a
//! command is the better answer.
//!
//! **Adding a code.** Add one entry to the `error_codes!` list below (variant, wire
//! name, exit class, documentation, `next` hint), add its row to the Errors table in
//! `ARCHITECTURE.md`, run `cargo xtask docs`, and raise it where it belongs.
//! `cargo xtask check` fails while the table, this list and the generated reference
//! docs disagree.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt;

/// The process exit status a failure maps to. Only the CLI uses it, but it is defined
/// beside the codes so there is a single table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ExitClass {
    /// Exit 1: a valid request that could not be carried out.
    OperationFailed,
    /// Exit 2: the request itself is malformed.
    Usage,
    /// Exit 3: the request names something that is missing or not unique.
    Target,
    /// Exit 4: the request was written against an out-of-date state.
    Stale,
    /// Exit 5: the edit would add schema or Word-rule violations.
    Validation,
    /// Exit 6: the input is unsafe, malformed or unreadable.
    Input,
    /// Exit 7: a bug in stetdoc. It is never the caller's mistake, so the right answer is to
    /// report it, not to rephrase the request.
    Internal,
}

impl ExitClass {
    /// Every exit class, in exit-code order.
    pub const ALL: [ExitClass; 7] = [
        ExitClass::OperationFailed,
        ExitClass::Usage,
        ExitClass::Target,
        ExitClass::Stale,
        ExitClass::Validation,
        ExitClass::Input,
        ExitClass::Internal,
    ];

    /// The process exit status, from 1 to 7. Exit status 0 is success and is not a
    /// class.
    pub const fn number(self) -> u8 {
        match self {
            ExitClass::OperationFailed => 1,
            ExitClass::Usage => 2,
            ExitClass::Target => 3,
            ExitClass::Stale => 4,
            ExitClass::Validation => 5,
            ExitClass::Input => 6,
            ExitClass::Internal => 7,
        }
    }

    /// The short name used in the Errors table of `ARCHITECTURE.md`.
    pub const fn label(self) -> &'static str {
        match self {
            ExitClass::OperationFailed => "operation failed",
            ExitClass::Usage => "usage",
            ExitClass::Target => "target",
            ExitClass::Stale => "stale",
            ExitClass::Validation => "validation",
            ExitClass::Input => "input",
            ExitClass::Internal => "internal",
        }
    }
}

macro_rules! error_codes {
    ($(
        $variant:ident {
            wire: $wire:literal,
            exit: $exit:ident,
            doc: $doc:literal,
            next: $next:literal $(,)?
        }
    )*) => {
        /// A stable, documented identifier for a failure.
        ///
        /// The wire name (see [`ErrorCode::as_str`]) is what appears in CLI output,
        /// JSON and the library. Codes are frozen from 1.0 and new ones may be added,
        /// so match with a wildcard arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum ErrorCode {
            $(
                #[doc = $doc]
                $variant,
            )*
        }

        impl ErrorCode {
            /// Every code, in the order of the Errors table.
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant),*];

            /// The snake_case name used on the wire.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(ErrorCode::$variant => $wire,)*
                }
            }

            /// The exit class this code maps to.
            pub const fn exit_class(self) -> ExitClass {
                match self {
                    $(ErrorCode::$variant => ExitClass::$exit,)*
                }
            }

            /// What the code means, in a sentence or two.
            pub const fn description(self) -> &'static str {
                match self {
                    $(ErrorCode::$variant => $doc,)*
                }
            }

            /// The default hint for the next step, written as an instruction.
            pub const fn next_hint(self) -> &'static str {
                match self {
                    $(ErrorCode::$variant => $next,)*
                }
            }
        }
    };
}

error_codes! {
    Unsupported {
        wire: "unsupported",
        exit: OperationFailed,
        doc: "The request is well formed but asks for something this release does not \
              do, such as reordering refs in a readable file or creating a file in the \
              editor.",
        next: "Use a supported operation (the capabilities list names them), or \
               leave this content untouched.",
    }
    CrossBlock {
        wire: "cross_block",
        exit: OperationFailed,
        doc: "A text range or find match spans more than one block. Operations address \
              one block at a time.",
        next: "Split the edit into one operation per block, or merge the blocks \
               first.",
    }
    ReadonlyRegion {
        wire: "readonly_region",
        exit: OperationFailed,
        doc: "The edit changes read-only content: a clause number, a field result, a \
              placeholder or someone else's deletion.",
        next: "Edit the surrounding text instead and leave the read-only region \
               as it is.",
    }
    ScopeViolation {
        wire: "scope_violation",
        exit: OperationFailed,
        doc: "The batch would change a block outside the refs named by the scope guard. \
              Nothing was applied.",
        next: "Add the block's ref to the scope, or remove the operation that \
               reaches outside it.",
    }
    IoError {
        wire: "io_error",
        exit: OperationFailed,
        doc: "The file system refused a read or a write that the request needs: a folder \
              that is not writable, a full disk, or a working copy that another process \
              holds. Only surfaces that do file I/O raise it; the core never does.",
        next: "Fix the cause the message names (permissions, space, another process), \
               then run the same command again.",
    }
    InvalidPayload {
        wire: "invalid_payload",
        exit: Usage,
        doc: "An operation batch or readable file could not be parsed or is internally \
              inconsistent: an unknown operation, a missing field, a bad value or a \
              duplicated ref.",
        next: "Fix the payload and retry. The capabilities list names the valid \
               operations and their fields.",
    }
    NotFound {
        wire: "not_found",
        exit: Target,
        doc: "A ref, revision id, comment id or find text matches nothing.",
        next: "View the document to list the current refs, then retry with one \
               that exists.",
    }
    AmbiguousMatch {
        wire: "ambiguous_match",
        exit: Target,
        doc: "A find text or string matches more than one place, so the target is not \
              unique.",
        next: "Add surrounding text to make the match unique, or target one \
               block by its ref.",
    }
    CountMismatch {
        wire: "count_mismatch",
        exit: Target,
        doc: "A find text matches a different number of times from the count the \
              request expects, so nothing was applied.",
        next: "Look at the places listed, then give the count they show or a \
               narrower scope.",
    }
    StaleRef {
        wire: "stale_ref",
        exit: Stale,
        doc: "A ref names a block that was deleted, or belongs to a different ref \
              namespace than the document, for example after reopening saved bytes.",
        next: "View the document again to get fresh refs, then rewrite the \
               batch.",
    }
    StaleFiles {
        wire: "stale_files",
        exit: Stale,
        doc: "Readable files were rendered from an older state version, or from a \
              different ref namespace, than the document now has.",
        next: "Export the working copy again to render fresh readable files, \
               then redo the hand edits on them.",
    }
    PendingEdits {
        wire: "pending_edits",
        exit: Stale,
        doc: "The working copy has hand edits to readable files that have not been \
              packed, so applying operations would lose or mix them.",
        next: "Apply the pending hand edits as a files edit, or discard them, \
               then retry the operation.",
    }
    StaleTarget {
        wire: "stale_target",
        exit: Stale,
        doc: "The file a command would write holds a version that the command has not \
              seen, so writing it could overwrite a newer version: a pack's output that is \
              neither the document the working copy came from nor one it packed, or a .docx \
              edited in place that someone wrote during the edit. Only surfaces that write \
              files raise it.",
        next: "Read the file at that path again and redo the edits on what it holds now, \
               or write to another path.",
    }
    ValidationFailed {
        wire: "validation_failed",
        exit: Validation,
        doc: "The edit would add schema or Word-rule violations. Nothing was applied. \
              Problems already present in the input are reported by check, not blocked.",
        next: "Change the edit so it does not add the violation the message \
               describes, then retry. A check of the document lists problems \
               that were already there.",
    }
    MalformedInput {
        wire: "malformed_input",
        exit: Input,
        doc: "The input is not a readable .docx package: a bad ZIP structure, missing \
              required parts, or XML that does not parse.",
        next: "Check that the input is a valid .docx. Opening and re-saving it \
               in Word or LibreOffice often repairs it.",
    }
    LimitExceeded {
        wire: "limit_exceeded",
        exit: Input,
        doc: "The input or the request exceeds a configured limit, such as entry count, \
              size, compression ratio, nesting depth or batch size. When the limit is one \
              of the fields of `Limits`, the error names it in `limit`, with what the input \
              had in `value` and the limit in `max`.",
        next: "Use a smaller input. A Node or Rust caller can raise the limit that the \
               error names, in its open options, if the source is trusted.",
    }
    CorruptComplement {
        wire: "corrupt_complement",
        exit: Input,
        doc: "The complement of a working copy failed verification: its content hashes, \
              ref map or format did not match.",
        next: "Do not edit the complement. Restore the working copy, or start \
               again from the original .docx.",
    }
    UnsupportedVersion {
        wire: "unsupported_version",
        exit: Input,
        doc: "The complement was written by a newer major version of stetdoc, or in a \
              format version this release cannot read.",
        next: "Upgrade stetdoc to a version that reads this complement.",
    }
    Internal {
        wire: "internal",
        exit: Internal,
        doc: "A bug in stetdoc: an internal consistency check failed, so the request was \
              refused and the document was not changed. The content of the request never \
              causes this.",
        next: "Report this as a bug in stetdoc, with the input and the request that \
               caused it. Do not try a different edit to get around it.",
    }
}

impl ErrorCode {
    /// Looks a code up by its wire name. The inverse of [`ErrorCode::as_str`].
    pub fn from_wire(name: &str) -> Option<ErrorCode> {
        ErrorCode::ALL
            .iter()
            .copied()
            .find(|code| code.as_str() == name)
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// A code is its wire name in JSON, so these are written by hand and not derived.
impl Serialize for ErrorCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ErrorCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<ErrorCode, D::Error> {
        let name = String::deserialize(deserializer)?;
        ErrorCode::from_wire(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("`{name}` is not an error code")))
    }
}

impl JsonSchema for ErrorCode {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("ErrorCode")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        let names: Vec<&str> = ErrorCode::ALL.iter().map(|c| c.as_str()).collect();
        json_schema!({ "type": "string", "enum": names })
    }
}

/// A failure: a stable code, a message, a `next` hint and, for batches, the index of
/// the failing operation.
///
/// The hint defaults to the code's [`ErrorCode::next_hint`]. Raise errors with a
/// specific hint when the caller can say more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StetError {
    code: ErrorCode,
    message: String,
    next: Option<String>,
    op_index: Option<usize>,
    file: Option<String>,
    line: Option<u32>,
    /// Boxed so that a failure without a limit stays small: `Result` carries the error on every call.
    limit: Option<Box<Exceeded>>,
}

/// The bound a `limit_exceeded` failure is over: the name of its field in `Limits`, the value the input or request had and
/// the bound it passed.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Exceeded {
    name: &'static str,
    value: u64,
    max: u64,
}

impl StetError {
    /// An error with a code and a one-line message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> StetError {
        StetError {
            code,
            message: message.into(),
            next: None,
            op_index: None,
            file: None,
            line: None,
            limit: None,
        }
    }

    /// Replaces the code's default `next` hint with a more specific one.
    #[must_use]
    pub fn with_next(mut self, next: impl Into<String>) -> StetError {
        self.next = Some(next.into());
        self
    }

    /// Records the zero-based index of the operation in the batch that failed.
    #[must_use]
    pub fn at_op(mut self, index: usize) -> StetError {
        self.op_index = Some(index);
        self
    }

    /// Records the readable file and the one-based line that the failure came from, for an edit
    /// of readable files, where the agent wrote lines and not operations.
    #[must_use]
    pub fn at_line(mut self, file: impl Into<String>, line: u32) -> StetError {
        self.file = Some(file.into());
        self.line = Some(line);
        self
    }

    /// Records the readable file, and the one-based line when it is not 0 (0 is the file as a whole), for a refusal of readable
    /// files whose message names them too: so that a caller reads them from the fields and not from the text.
    #[must_use]
    pub(crate) fn in_file(mut self, file: impl Into<String>, line: u32) -> StetError {
        self.file = Some(file.into());
        self.line = (line > 0).then_some(line);
        self
    }

    /// Records the bound a `limit_exceeded` failure is over: `name` is the field of `Limits` (such as `max_archive_bytes`),
    /// `value` is what the input or the request had and `max` is the limit it passed. The JSON form carries them as `limit`,
    /// `value` and `max`, so a caller can tell which limit was hit without reading the message.
    #[must_use]
    pub fn with_limit(mut self, name: &'static str, value: u64, max: u64) -> StetError {
        self.limit = Some(Box::new(Exceeded { name, value, max }));
        self
    }

    /// The name of the field of `Limits` that a `limit_exceeded` failure is over, when the failure names one.
    pub fn limit_name(&self) -> Option<&'static str> {
        self.limit.as_ref().map(|e| e.name)
    }

    /// What the input or request had, for the limit that [`StetError::limit_name`] names.
    pub fn limit_value(&self) -> Option<u64> {
        self.limit.as_ref().map(|e| e.value)
    }

    /// The limit that [`StetError::limit_name`] names.
    pub fn limit_max(&self) -> Option<u64> {
        self.limit.as_ref().map(|e| e.max)
    }

    /// Clears the operation index, for a failure that is reported by file and line instead.
    #[must_use]
    pub fn without_op(mut self) -> StetError {
        self.op_index = None;
        self
    }

    /// The stable code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// The one-line message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The hint for the next step: the specific one if set, otherwise the code's
    /// default.
    pub fn next(&self) -> &str {
        self.next
            .as_deref()
            .unwrap_or_else(|| self.code.next_hint())
    }

    /// The index of the failing operation, when the failure came from a batch.
    pub fn op_index(&self) -> Option<usize> {
        self.op_index
    }

    /// The readable file the failure came from, when it came from an edit of files.
    pub fn file(&self) -> Option<&str> {
        self.file.as_deref()
    }

    /// The one-based line in [`StetError::file`].
    pub fn line(&self) -> Option<u32> {
        self.line
    }
}

/// What a failure looks like as data: `--json`, napi's `StetdocError` and the schema.
#[derive(Serialize, JsonSchema)]
struct StetErrorData<'a> {
    /// The stable code.
    code: ErrorCode,
    /// The one-line message.
    message: &'a str,
    /// The hint for the next step.
    next: &'a str,
    /// The zero-based index of the failing operation.
    #[serde(skip_serializing_if = "Option::is_none")]
    op_index: Option<usize>,
    /// The readable file the failure came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<&'a str>,
    /// The one-based line in `file`.
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<u32>,
    /// The field of `Limits` that was exceeded, such as `max_archive_bytes`.
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<&'a str>,
    /// What the input or request had, for `limit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<u64>,
    /// The limit that `value` passed, for `limit`.
    #[serde(skip_serializing_if = "Option::is_none")]
    max: Option<u64>,
}

impl Serialize for StetError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        StetErrorData {
            code: self.code,
            message: &self.message,
            next: self.next(),
            op_index: self.op_index,
            file: self.file.as_deref(),
            line: self.line,
            limit: self.limit_name(),
            value: self.limit_value(),
            max: self.limit_max(),
        }
        .serialize(serializer)
    }
}

impl JsonSchema for StetError {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("StetError")
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        StetErrorData::json_schema(generator)
    }
}

impl fmt::Display for StetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(index) = self.op_index {
            write!(f, " (operation {index})")?;
        }
        if let (Some(file), Some(line)) = (&self.file, self.line) {
            write!(f, " ({file} line {line})")?;
        }
        Ok(())
    }
}

impl std::error::Error for StetError {}
