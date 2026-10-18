//! A batch of operations, written against one base token.

use super::op::Op;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A list of operations that applies completely or not at all.
///
/// It names the base token of the state it was written against, always, in a working copy as well, so that an old `p42`
/// can never silently hit a different paragraph. A batch is written blind: no operation can name a block that an earlier
/// operation of the batch created, and each operation sees the effect of the earlier ones. `Batch::parse` reads JSON or
/// block text, and `to_json` and `to_block` write them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    /// The base token that the batch was written against: the one the last view or receipt showed.
    pub base: String,
    /// The operations, in order. A batch with none is `invalid_payload`.
    pub ops: Vec<Op>,
}

impl Batch {
    /// A batch of operations against a base token.
    pub fn new(base: impl Into<String>, ops: Vec<Op>) -> Batch {
        Batch {
            base: base.into(),
            ops,
        }
    }
}
