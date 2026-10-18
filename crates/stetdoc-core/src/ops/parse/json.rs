//! The JSON syntax: serde over `Op`, one operation at a time so that an error names the operation, with the errors of serde
//! said as `invalid_payload`.

use crate::api::batch::Batch;
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::Op;
use serde_json::Value;

fn bad(message: String) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message).with_next(
        "Check the field names against the operations reference, and write the batch again.",
    )
}

/// Reads a JSON batch. Serde refuses unknown fields, unknown operations and wrong types, and its message says what was
/// expected. The batch is read as a value first and each operation after it, so that an error carries the index of the
/// operation that failed.
pub(super) fn parse(text: &str) -> Result<Batch, StetError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|e| bad(format!("The batch is not valid JSON: {e}.")))?;
    let Value::Object(mut object) = value else {
        return Err(bad(
            "A batch is a JSON object with a `base` and a list of `ops`.".to_string(),
        ));
    };
    let base = match object.remove("base") {
        Some(Value::String(base)) => base,
        Some(_) => {
            return Err(bad(
                "`base` of the batch is a string: the base token.".to_string()
            ));
        }
        None => {
            return Err(bad(
                "The batch has no `base`: the base token that the last view or receipt showed."
                    .to_string(),
            ));
        }
    };
    let ops = match object.remove("ops") {
        Some(Value::Array(ops)) => ops,
        Some(_) => {
            return Err(bad(
                "`ops` of the batch is a list of operations.".to_string()
            ));
        }
        None => {
            return Err(bad(
                "The batch has no `ops`: the list of operations.".to_string()
            ));
        }
    };
    if let Some(key) = object.keys().next() {
        return Err(bad(format!(
            "unknown field `{key}` in the batch, expected `base` or `ops`."
        )));
    }
    let mut parsed = Vec::with_capacity(ops.len());
    for (index, op) in ops.into_iter().enumerate() {
        parsed
            .push(serde_json::from_value::<Op>(op).map_err(|e| bad(format!("{e}.")).at_op(index))?);
    }
    Ok(Batch { base, ops: parsed })
}

/// The batch as compact JSON.
pub(super) fn write(batch: &Batch) -> String {
    serde_json::to_string(batch).unwrap_or_default()
}
