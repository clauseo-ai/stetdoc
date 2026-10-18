//! The checks of a batch that need no document: its size, its base token, and every operation's fields.
//!
//! They run before a `Tx` is begun, so a malformed batch costs no staging. Everything found is `invalid_payload`, except a
//! limit, which is `limit_exceeded`. The checks that need the document (a ref that does not exist, a find that matches
//! nothing, a style that is not defined) run as each operation compiles. The checks are driven by the table in
//! `api::op_spec` and by the JSON form of each operation, so there is no code for each operation.

use crate::api::batch::Batch;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::api::op::Op;
use crate::api::op_spec::{FieldKind, FieldSpec};
use crate::refs::{BaseToken, BlockRef};
use serde_json::{Map, Value};

fn invalid(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

fn too_big(message: impl Into<String>) -> StetError {
    StetError::new(ErrorCode::LimitExceeded, message)
        .with_next("Apply the batch in parts: fewer operations or shorter texts, then the rest.")
}

/// Checks a whole batch.
pub(crate) fn batch(batch: &Batch, limits: &Limits) -> Result<(), StetError> {
    if batch.ops.is_empty() {
        return Err(
            invalid("The batch holds no operations.").with_next("Write at least one operation.")
        );
    }
    if batch.ops.len() as u64 > u64::from(limits.max_batch_ops) {
        return Err(too_big(format!(
            "The batch holds {} operations, over the limit of {}.",
            batch.ops.len(),
            limits.max_batch_ops
        )));
    }
    // Each paragraph of an `insert` costs what an operation does (a tracked insertion, its checks and its place in the
    // receipt), and on the files path every new paragraph is a changed block, so each counts as one operation.
    let counted: u64 = batch
        .ops
        .iter()
        .map(|op| match op {
            Op::Insert(insert) => (insert.paragraphs.len() as u64).max(1),
            _ => 1,
        })
        .sum();
    if counted > u64::from(limits.max_batch_ops) {
        return Err(too_big(format!(
            "The batch holds {} operations and makes {counted} paragraphs and operations together, over the limit of {}: \
             each paragraph of an `insert` counts as one operation.",
            batch.ops.len(),
            limits.max_batch_ops
        ))
        .with_next("Apply the batch in parts: insert fewer paragraphs in each batch, then the rest."));
    }
    BaseToken::parse(&batch.base).map_err(|e| {
        invalid(format!(
            "The base token `{}` is not one: {}",
            batch.base,
            e.message()
        ))
        .with_next("Copy the base token from the last view or receipt, and write the batch again.")
    })?;
    let mut bytes: u64 = batch.base.len() as u64;
    for (index, op) in batch.ops.iter().enumerate() {
        bytes += operation(op, limits).map_err(|e| e.at_op(index))?;
        if bytes > limits.max_batch_bytes {
            return Err(too_big(format!(
                "The batch is over {} bytes of text.",
                limits.max_batch_bytes
            ))
            .at_op(index));
        }
    }
    Ok(())
}

/// Checks one operation, and returns the bytes of text it holds.
fn operation(op: &Op, limits: &Limits) -> Result<u64, StetError> {
    let spec = op.spec();
    let name = spec.name;
    let value =
        serde_json::to_value(op).map_err(|e| StetError::new(ErrorCode::Internal, e.to_string()))?;
    let Some(object) = value.as_object() else {
        return Err(StetError::new(
            ErrorCode::Internal,
            "an operation is not an object",
        ));
    };
    let has = |key: &str| object.get(key).is_some();
    // One of a group.
    if !spec.one_of.is_empty() {
        let given: Vec<&str> = spec.one_of.iter().copied().filter(|k| has(k)).collect();
        if given.len() != 1 {
            return Err(invalid(format!(
                "`{name}` takes exactly one of {}, and this gives {}.",
                list(spec.one_of),
                if given.is_empty() {
                    "none".to_string()
                } else {
                    list(&given)
                }
            )));
        }
    }
    // A range in a paragraph.
    if has("find") && (has("start") || has("end")) {
        return Err(invalid(format!(
            "`{name}` names its range with `find` or with `start` and `end`, not both."
        )));
    }
    if has("occurrence") && !has("find") {
        return Err(invalid(format!(
            "`occurrence` of `{name}` says which match of `find`, and there is no `find`."
        )));
    }
    if has("start") != has("end")
        && !has("find")
        && spec.fields.iter().any(|f| f.name == "start")
        && name != "delete"
    {
        return Err(invalid(format!(
            "`{name}` takes `start` and `end` together."
        )));
    }
    // In one paragraph the end of a range cannot come before its start. A `delete` with `to` has `start` in `ref` and `end`
    // in `to`, two paragraphs whose order only the document knows, so its points are compared when it compiles.
    if let (Some(s), Some(e)) = (number(object, "start"), number(object, "end"))
        && s > e
        && !has("to")
    {
        return Err(invalid(format!("`start` ({s}) is after `end` ({e}).")));
    }
    if number(object, "occurrence") == Some(0) {
        return Err(invalid("`occurrence` counts from 1."));
    }
    if let Some(level) = number(object, "level")
        && level > 8
    {
        return Err(invalid(format!(
            "A list has levels 0 to 8, and `level` is {level}."
        )));
    }
    if number(object, "section") == Some(0) || number(object, "cell") == Some(0) {
        return Err(invalid("`section` and `cell` count from 1."));
    }
    // Fields.
    let mut bytes = 0u64;
    for field in spec.fields {
        let Some(v) = object.get(field.name) else {
            continue;
        };
        bytes += field_ok(name, field, v, limits)?;
    }
    if name == "set" && !has("ref") && !has("section") {
        return Err(invalid(
            "`set` names its target with `ref`, or with `section` for a section.",
        ));
    }
    if matches!(name, "accept" | "reject")
        && object
            .get("ids")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    {
        return Err(invalid(format!("`ids` of `{name}` is empty.")));
    }
    Ok(bytes)
}

fn number(object: &Map<String, Value>, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn list(names: &[&str]) -> String {
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Checks one field's value, and returns the bytes of text in it.
fn field_ok(op: &str, field: &FieldSpec, value: &Value, limits: &Limits) -> Result<u64, StetError> {
    let key = field.name;
    match (field.kind, value) {
        (FieldKind::Ref, Value::String(s)) => {
            reference(op, key, s)?;
            Ok(s.len() as u64)
        }
        (FieldKind::Choice, Value::String(s)) => {
            if !field.choices.is_empty() && !field.choices.contains(&s.as_str()) {
                return Err(invalid(format!(
                    "`{key}` of `{op}` is {}, and `{s}` is not one.",
                    list(field.choices)
                )));
            }
            Ok(s.len() as u64)
        }
        (FieldKind::Text, Value::String(s)) => {
            text(op, key, s, limits)?;
            if matches!(
                key,
                "find" | "before" | "after" | "with" | "text" | "style" | "author"
            ) && s.is_empty()
            {
                return Err(invalid(format!(
                    "`{key}` of `{op}` is empty.{}",
                    if key == "with" {
                        " To remove text, use `delete`."
                    } else {
                        ""
                    }
                )));
            }
            Ok(s.len() as u64)
        }
        (FieldKind::List, Value::Array(items)) => {
            let mut bytes = 0;
            for item in items {
                match item {
                    Value::String(s) => {
                        text(op, key, s, limits)?;
                        bytes += s.len() as u64;
                    }
                    Value::Object(o) => {
                        if let Some(Value::String(s)) = o.get("text") {
                            text(op, key, s, limits)?;
                            bytes += s.len() as u64;
                        }
                    }
                    _ => {}
                }
            }
            if key == "paragraphs" && items.is_empty() {
                return Err(invalid("`paragraphs` of `insert` is empty."));
            }
            Ok(bytes)
        }
        (FieldKind::Props, Value::Array(items)) => {
            if items.is_empty() {
                return Err(invalid("`props` of `set` is empty."));
            }
            let mut bytes = 0;
            for item in items {
                bytes += prop(item, 0, limits)?;
            }
            Ok(bytes)
        }
        _ => Ok(0),
    }
}

/// A property and its children, at most eight deep.
fn prop(value: &Value, depth: usize, limits: &Limits) -> Result<u64, StetError> {
    if depth > 8 {
        return Err(invalid(
            "A property has children nested more than eight deep.",
        ));
    }
    let Some(object) = value.as_object() else {
        return Ok(0);
    };
    let mut bytes = 0;
    if let Some(Value::String(name)) = object.get("name") {
        if name.is_empty() || name.contains(char::is_whitespace) {
            return Err(invalid(
                "The name of a property is a local name, such as `spacing`, with no spaces.",
            ));
        }
        bytes += name.len() as u64;
    }
    if let Some(Value::String(v)) = object.get("val") {
        text("set", "val", v, limits)?;
        bytes += v.len() as u64;
    }
    if let Some(Value::Object(attrs)) = object.get("attrs") {
        for (k, v) in attrs {
            if let Value::String(v) = v {
                text("set", k, v, limits)?;
                bytes += (k.len() + v.len()) as u64;
            }
        }
    }
    if let Some(Value::Array(children)) = object.get("children") {
        for child in children {
            bytes += prop(child, depth + 1, limits)?;
        }
    }
    Ok(bytes)
}

/// The syntax of a ref, a comment id or a note id.
fn reference(op: &str, key: &str, text: &str) -> Result<(), StetError> {
    match key {
        "comment" => {
            let ok = text
                .strip_prefix('c')
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
            if !ok {
                return Err(invalid(format!(
                    "`{key}` of `{op}` is a comment such as `c3`, not `{text}`."
                )));
            }
        }
        "note" => {
            let ok = ["fn", "en"].iter().any(|p| {
                text.strip_prefix(p)
                    .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
            });
            if !ok {
                return Err(invalid(format!(
                    "`{key}` of `{op}` is a note such as `fn3` or `en2`, not `{text}`."
                )));
            }
        }
        _ => {
            BlockRef::parse(text)
                .map_err(|e| invalid(format!("`{key}` of `{op}`: {}", e.message())))?;
        }
    }
    Ok(())
}

/// A text is within the size limit and holds only characters that XML can carry: a tab, a newline and printable text.
fn text(op: &str, key: &str, text: &str, limits: &Limits) -> Result<(), StetError> {
    if text.len() as u64 > u64::from(limits.max_op_text_bytes) {
        return Err(too_big(format!(
            "`{key}` of `{op}` is {} bytes, over the limit of {}.",
            text.len(),
            limits.max_op_text_bytes
        )));
    }
    if let Some(c) = text
        .chars()
        .find(|c| c.is_control() && !matches!(c, '\t' | '\n'))
    {
        return Err(invalid(format!(
            "`{key}` of `{op}` holds the control character U+{:04X}, which a document cannot carry. Tabs and newlines are allowed.",
            u32::from(c)
        )));
    }
    Ok(())
}
