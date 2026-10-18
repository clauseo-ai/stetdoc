//! What `Document::inspect` returns.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One line of an inspection: a label, its value and where the value comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InspectItem {
    /// What the line is about, such as `style` or `spacing`.
    pub label: String,
    /// Its value.
    pub value: String,
    /// Where the value comes from: `default`, a table style, numbering, a paragraph style or `direct`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The answer to `Document::inspect`: effective formatting with its source for a block, or the structure of the document
/// for `styles`, `numbering` and `sections`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Inspection {
    /// The base token of the state the answer describes.
    pub base: String,
    /// What was inspected, as asked.
    pub target: String,
    /// The lines as text: what a surface prints.
    pub text: String,
    /// The same lines as data.
    pub items: Vec<InspectItem>,
}
