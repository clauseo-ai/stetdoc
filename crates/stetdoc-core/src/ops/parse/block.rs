//! The block-text syntax of a batch. **Provisional**: the scope leaves the grammar to experiment A, so it is confined to this
//! file and to `FieldKind`, and changing a spelling touches nothing else.
//!
//! ```text
//! base: 3f9a0c11d2b4e577:7:51ac09f2e8b34d11
//! ---
//! @replace
//! ref: p42
//! find: the Supplier shall
//! with: the Supplier must
//! ---
//! @insert
//! ref: p42
//! place: after
//! paragraphs: <<END
//! A first new paragraph.
//! A second one.
//! END
//! ```
//!
//! A batch is the header `base: <token>`, then stanzas. A stanza starts with `@name` after a line of `---`. A field is
//! `key: value` on one line, or `key: <<TAG` and the lines up to a line that is exactly `TAG`, which carries text with quotes,
//! braces or backslashes without escaping. What a newline means depends on the kind of the field (`fields.rs`). A key that is not
//! a field of the operation is refused, with the fields listed. A carriage return is refused.

use super::fields::{self, Raw};
use crate::api::batch::Batch;
use crate::api::error::{ErrorCode, StetError};
use crate::api::op::Op;
use crate::api::op_spec::{OPS, OpSpec};
use serde_json::{Map, Value};

fn bad(line: usize, message: impl Into<String>) -> StetError {
    StetError::new(
        ErrorCode::InvalidPayload,
        format!("Line {line}: {}", message.into()),
    )
}

fn names() -> String {
    OPS.iter().map(|s| s.name).collect::<Vec<_>>().join(", ")
}

fn spec_named(name: &str) -> Option<&'static OpSpec> {
    OPS.iter().copied().find(|spec| spec.name == name)
}

/// A heredoc tag: letters, digits and underscores.
fn tag_of(rest: &str) -> Option<&str> {
    let tag = rest.strip_prefix("<<")?;
    (!tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then_some(tag)
}

struct Stanza {
    spec: &'static OpSpec,
    line: usize,
    map: Map<String, Value>,
}

/// Reads block text.
pub(super) fn parse(text: &str) -> Result<Batch, StetError> {
    if text.contains('\r') {
        return Err(StetError::new(
            ErrorCode::InvalidPayload,
            "The batch has a carriage return. Block text uses line feeds only.",
        ));
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut at = 0usize;
    let mut base: Option<String> = None;
    let mut ops: Vec<Op> = Vec::new();
    let mut current: Option<Stanza> = None;
    while let Some(raw_line) = lines.get(at) {
        let number = at + 1;
        at += 1;
        let line = raw_line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        if line == "---" {
            finish(current.take(), &mut ops)?;
            continue;
        }
        if let Some(name) = line.strip_prefix('@') {
            finish(current.take(), &mut ops)?;
            let name = name.trim();
            let spec = spec_named(name).ok_or_else(|| {
                bad(
                    number,
                    format!(
                        "`@{name}` is not an operation. The operations are {}.",
                        names()
                    ),
                )
            })?;
            current = Some(Stanza {
                spec,
                line: number,
                map: Map::new(),
            });
            continue;
        }
        let Some((key, rest)) = raw_line.split_once(':') else {
            return Err(bad(
                number,
                "expected `key: value`, a line `@operation` or a line `---`.",
            ));
        };
        let key = key.trim();
        let rest = rest.strip_prefix(' ').unwrap_or(rest);
        let raw = match tag_of(rest.trim_end()) {
            Some(tag) => {
                let mut body = Vec::new();
                loop {
                    let Some(next) = lines.get(at) else {
                        return Err(bad(
                            number,
                            format!("the heredoc `<<{tag}` has no closing line `{tag}`."),
                        ));
                    };
                    at += 1;
                    if *next == tag {
                        break;
                    }
                    body.push((*next).to_string());
                }
                Raw::Heredoc(body)
            }
            None => Raw::Line(rest.to_string()),
        };
        match current.as_mut() {
            None if key == "base" => {
                if base.is_some() {
                    return Err(bad(number, "`base` is given twice."));
                }
                match raw {
                    Raw::Line(token) => base = Some(token.trim().to_string()),
                    Raw::Heredoc(_) => return Err(bad(number, "`base` is one line.")),
                }
            }
            None => {
                return Err(bad(
                    number,
                    "a field before the first `@operation`. Write the base token first, then `---` and `@operation`.",
                ));
            }
            Some(stanza) => {
                let field = stanza
                    .spec
                    .fields
                    .iter()
                    .find(|f| f.name == key)
                    .ok_or_else(|| {
                        let fields: Vec<&str> = stanza.spec.fields.iter().map(|f| f.name).collect();
                        bad(
                            number,
                            format!(
                                "`{key}` is not a field of `{}`. Its fields are {}.",
                                stanza.spec.name,
                                fields.join(", ")
                            ),
                        )
                    })?;
                fields::add(&mut stanza.map, field, raw)
                    .map_err(|e| bad(number, e.message().to_string()))?;
            }
        }
    }
    finish(current.take(), &mut ops)?;
    let Some(base) = base else {
        return Err(StetError::new(
            ErrorCode::InvalidPayload,
            "The batch has no base token. Its first line is `base: <token>`, the token the last view or receipt showed.",
        ));
    };
    Ok(Batch { base, ops })
}

/// Turns a finished stanza into an operation through the same serde path as JSON.
fn finish(stanza: Option<Stanza>, ops: &mut Vec<Op>) -> Result<(), StetError> {
    let Some(mut stanza) = stanza else {
        return Ok(());
    };
    stanza.map.insert(
        "op".to_string(),
        Value::String(stanza.spec.name.to_string()),
    );
    let index = ops.len();
    let op = serde_json::from_value::<Op>(Value::Object(stanza.map)).map_err(|e| {
        bad(
            stanza.line,
            format!("the `{}` operation is not complete: {e}.", stanza.spec.name),
        )
        .at_op(index)
    })?;
    ops.push(op);
    Ok(())
}

/// Writes block text, or says why the batch cannot be written that way.
pub(super) fn write(batch: &Batch) -> Result<String, StetError> {
    if batch.base.contains('\n') {
        return Err(StetError::new(
            ErrorCode::InvalidPayload,
            "The base token has a newline.",
        ));
    }
    let mut out = format!("base: {}\n", batch.base);
    for (index, op) in batch.ops.iter().enumerate() {
        let spec = op.spec();
        let value = serde_json::to_value(op)
            .map_err(|e| StetError::new(ErrorCode::Internal, e.to_string()))?;
        let Some(object) = value.as_object() else {
            return Err(StetError::new(
                ErrorCode::Internal,
                "an operation is not an object",
            ));
        };
        out.push_str("---\n");
        out.push_str(&format!("@{}\n", spec.name));
        for field in spec.fields {
            if let Some(v) = object.get(field.name) {
                fields::write(field, v, &mut out).map_err(|e| e.at_op(index))?;
            }
        }
    }
    Ok(out)
}
