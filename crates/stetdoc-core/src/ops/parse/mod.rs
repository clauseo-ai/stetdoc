//! `Batch::parse`, `to_json` and `to_block`: the two syntaxes of a batch.
//!
//! The first non-blank character selects the syntax: `{` is JSON, and anything else is block text. A bare JSON array is refused,
//! because it has no base token. Both syntaxes end in the same `Batch`, so they cannot disagree about what an operation is: JSON
//! is serde over `Op`, and block text is read from the table in `api::op_spec` (`block.rs`), which builds a JSON value for each
//! stanza and hands it to the same serde path. A byte limit is checked before anything is allocated.

mod block;
mod fields;
mod json;

use crate::api::batch::Batch;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;

impl Batch {
    /// Reads a batch from JSON or from block text, and validates it against `limits` (the number of operations, the size of
    /// each text, the syntax of every ref). The grammar of block text is in `docs/reference/operations.md`.
    pub fn parse(text: &str, limits: &Limits) -> Result<Batch, StetError> {
        if text.len() as u64 > limits.max_batch_bytes {
            return Err(StetError::new(
                ErrorCode::LimitExceeded,
                format!(
                    "The batch is {} bytes, over the limit of {}.",
                    text.len(),
                    limits.max_batch_bytes
                ),
            )
            .with_next("Apply the batch in parts: fewer operations, then the rest."));
        }
        let batch = match text.trim_start().chars().next() {
            Some('{') => json::parse(text)?,
            Some('[') => {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    "A bare array of operations has no base token. Write {\"base\": \"...\", \"ops\": [...]}.",
                ));
            }
            Some(_) => block::parse(text)?,
            None => {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    "The batch is empty. Write a base token and at least one operation.",
                ));
            }
        };
        super::validate::batch(&batch, limits)?;
        Ok(batch)
    }

    /// The batch as compact JSON.
    pub fn to_json(&self) -> String {
        json::write(self)
    }

    /// The batch as block text. Content that block text cannot say (a newline inside a list entry, a carriage return, a
    /// property with children) is `invalid_payload`: write the batch as JSON instead.
    pub fn to_block(&self) -> Result<String, StetError> {
        block::write(self)
    }
}

#[cfg(test)]
mod tests;
