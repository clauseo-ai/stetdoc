//! What this release can do, as data: `capabilities()`.

use super::limits::Limits;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A projection format and the extension of its readable files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FormatInfo {
    /// The id, such as `stet/1`.
    pub id: String,
    /// The extension of its readable files, such as `md`. A working copy holds the files of the directory that have it.
    pub extension: String,
}

/// One field of an operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldInfo {
    /// The key.
    pub name: String,
    /// The kind of value: `ref`, `text`, `number`, `bool`, `choice`, `list`, `number list` or `properties`.
    pub kind: String,
    /// Whether the operation needs it.
    pub required: bool,
    /// What it means.
    pub doc: String,
    /// For a choice, the words it may be.
    pub choices: Vec<String>,
}

/// One operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OpInfo {
    /// The name.
    pub name: String,
    /// What it does.
    pub summary: String,
    /// Whether it writes revisions.
    pub trackable: bool,
    /// Whether rejecting what it wrote restores the original.
    pub reversible: bool,
    /// The group of optional fields of which exactly one must be given. Empty when there is none.
    pub one_of: Vec<String>,
    /// An example, as JSON.
    pub example: String,
    /// Its fields.
    pub fields: Vec<FieldInfo>,
}

/// An error code with its exit status.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorInfo {
    /// The code.
    pub code: String,
    /// The exit status of its class.
    pub exit: u8,
    /// What it means.
    pub description: String,
    /// The default hint for the next step.
    pub next: String,
}

/// A finding code with its class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FindingInfo {
    /// The code.
    pub code: String,
    /// `violation` or `note`.
    pub class: String,
    /// What it means.
    pub description: String,
}

/// What this release can do: the operations, the formats, the codes and the default limits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    /// The release.
    pub version: String,
    /// The version of the complement this release writes, such as `1.0`. It reads the versions up to its own.
    pub complement_version: String,
    /// The formats it reads and writes.
    pub formats: Vec<FormatInfo>,
    /// The operations.
    pub ops: Vec<OpInfo>,
    /// The error codes.
    pub error_codes: Vec<ErrorInfo>,
    /// The warning codes.
    pub warning_codes: Vec<String>,
    /// The finding codes of `check`.
    pub finding_codes: Vec<FindingInfo>,
    /// The default limits.
    pub limits: Limits,
}
