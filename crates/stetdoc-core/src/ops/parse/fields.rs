//! Reading and writing one field of a block-text stanza, by the kind that `op_spec` gives it.

use crate::api::error::{ErrorCode, StetError};
use crate::api::op_spec::{FieldKind, FieldSpec};
use serde_json::{Map, Number, Value};

fn bad(message: String) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message)
}

/// What the lines of a field were: one line after the colon, or the lines of a heredoc.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Raw {
    /// The text after `key:`, with one optional space dropped.
    Line(String),
    /// The lines between `key: <<TAG` and `TAG`.
    Heredoc(Vec<String>),
}

/// Adds the field `spec` with value `raw` to the object of a stanza.
pub(super) fn add(
    map: &mut Map<String, Value>,
    spec: &FieldSpec,
    raw: Raw,
) -> Result<(), StetError> {
    let name = spec.name;
    let twice = || {
        bad(format!(
            "`{name}` is given twice. Only a list may repeat its key."
        ))
    };
    match spec.kind {
        FieldKind::Ref | FieldKind::Choice | FieldKind::Number | FieldKind::Bool => {
            let Raw::Line(line) = raw else {
                return Err(bad(format!(
                    "`{name}` is one line, and a heredoc cannot give it."
                )));
            };
            if map.contains_key(name) {
                return Err(twice());
            }
            let value = line.trim();
            let value = match spec.kind {
                FieldKind::Number => number(name, value)?,
                FieldKind::Bool => match value {
                    "true" => Value::Bool(true),
                    "false" => Value::Bool(false),
                    other => {
                        return Err(bad(format!(
                            "`{name}` is `true` or `false`, not `{other}`."
                        )));
                    }
                },
                _ => Value::String(value.to_string()),
            };
            map.insert(name.to_string(), value);
        }
        FieldKind::Text => {
            if map.contains_key(name) {
                return Err(twice());
            }
            let text = match raw {
                Raw::Line(line) => line,
                Raw::Heredoc(lines) => lines.join("\n"),
            };
            map.insert(name.to_string(), Value::String(text));
        }
        FieldKind::List => {
            let entries = match raw {
                Raw::Line(line) => vec![line],
                Raw::Heredoc(lines) => lines,
            };
            extend(map, name, entries.into_iter().map(Value::String));
        }
        FieldKind::NumberList => {
            let lines = match raw {
                Raw::Line(line) => vec![line],
                Raw::Heredoc(lines) => lines,
            };
            let mut numbers = Vec::new();
            for line in &lines {
                for token in line.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
                    numbers.push(number(name, token)?);
                }
            }
            extend(map, name, numbers.into_iter());
        }
        FieldKind::Props => {
            let lines = match raw {
                Raw::Line(line) => vec![line],
                Raw::Heredoc(lines) => lines,
            };
            let mut props = Vec::new();
            for line in &lines {
                props.push(prop(line)?);
            }
            extend(map, name, props.into_iter());
        }
    }
    Ok(())
}

fn extend(map: &mut Map<String, Value>, name: &str, items: impl Iterator<Item = Value>) {
    let entry = map
        .entry(name.to_string())
        .or_insert_with(|| Value::Array(Vec::new()));
    if let Value::Array(list) = entry {
        list.extend(items);
    }
}

fn number(name: &str, text: &str) -> Result<Value, StetError> {
    if let Ok(n) = text.parse::<u64>() {
        return Ok(Value::Number(Number::from(n)));
    }
    if let Ok(n) = text.parse::<i64>() {
        return Ok(Value::Number(Number::from(n)));
    }
    Err(bad(format!("`{name}` is a whole number, not `{text}`.")))
}

/// One line of a `props` field: `name`, then an optional value, then `attribute=value` pairs.
fn prop(line: &str) -> Result<Value, StetError> {
    let mut tokens = line.split_whitespace();
    let Some(name) = tokens.next() else {
        return Err(bad(
            "A property line is empty. Write a name, an optional value and attribute=value pairs."
                .to_string(),
        ));
    };
    let mut object = Map::new();
    object.insert("name".to_string(), Value::String(name.to_string()));
    let mut attrs = Map::new();
    for token in tokens {
        match token.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                attrs.insert(key.to_string(), Value::String(value.to_string()));
            }
            _ => {
                if object.contains_key("val") {
                    return Err(bad(format!(
                        "The property `{name}` has two values. Write one value, then attribute=value pairs."
                    )));
                }
                object.insert("val".to_string(), Value::String(token.to_string()));
            }
        }
    }
    if !attrs.is_empty() {
        object.insert("attrs".to_string(), Value::Object(attrs));
    }
    Ok(Value::Object(object))
}

/// What block text writes for a field value, or why it cannot.
pub(super) fn write(spec: &FieldSpec, value: &Value, out: &mut String) -> Result<(), StetError> {
    let name = spec.name;
    let cannot = |why: &str| {
        bad(format!(
            "`{name}` cannot be written as block text: {why}. Write the batch as JSON."
        ))
    };
    match (spec.kind, value) {
        (FieldKind::Ref | FieldKind::Choice, Value::String(s)) => {
            if s.contains('\n') || s.is_empty() || s.trim() != s {
                return Err(cannot("it is not a plain word"));
            }
            out.push_str(&format!("{name}: {s}\n"));
        }
        (FieldKind::Number, Value::Number(n)) => out.push_str(&format!("{name}: {n}\n")),
        (FieldKind::Bool, Value::Bool(b)) => out.push_str(&format!("{name}: {b}\n")),
        (FieldKind::Text, Value::String(s)) => text(name, s, out)?,
        (FieldKind::List, Value::Array(items)) => {
            let mut texts = Vec::new();
            for item in items {
                match item {
                    Value::String(s) if !s.contains('\n') => texts.push(s.as_str()),
                    Value::String(_) => return Err(cannot("an entry has a newline")),
                    _ => return Err(cannot("an entry has a style or another field of its own")),
                }
            }
            heredoc(name, &texts, out);
        }
        (FieldKind::NumberList, Value::Array(items)) => {
            let numbers: Vec<String> = items.iter().map(Value::to_string).collect();
            out.push_str(&format!("{name}: {}\n", numbers.join(", ")));
        }
        (FieldKind::Props, Value::Array(items)) => {
            let mut lines = Vec::new();
            for item in items {
                lines.push(prop_line(item).ok_or_else(|| {
                    cannot("a property has children or text this form cannot say")
                })?);
            }
            let texts: Vec<&str> = lines.iter().map(String::as_str).collect();
            heredoc(name, &texts, out);
        }
        _ => return Err(cannot("it has an unexpected shape")),
    }
    Ok(())
}

fn text(name: &str, s: &str, out: &mut String) -> Result<(), StetError> {
    if s.contains('\r') {
        return Err(bad(format!(
            "`{name}` has a carriage return, which block text cannot say. Write the batch as JSON."
        )));
    }
    if s.contains('\n')
        || s.starts_with(char::is_whitespace)
        || s.ends_with(char::is_whitespace)
        || s.starts_with("<<")
    {
        let lines: Vec<&str> = s.split('\n').collect();
        heredoc(name, &lines, out);
    } else {
        out.push_str(&format!("{name}: {s}\n"));
    }
    Ok(())
}

/// `key: <<TAG`, the lines, `TAG`, with a tag that no line equals.
fn heredoc(name: &str, lines: &[&str], out: &mut String) {
    let mut tag = "END".to_string();
    let mut n = 0u32;
    while lines.iter().any(|line| *line == tag) {
        n += 1;
        tag = format!("END{n}");
    }
    out.push_str(&format!("{name}: <<{tag}\n"));
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&tag);
    out.push('\n');
}

fn prop_line(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    if object.contains_key("children") {
        return None;
    }
    let mut line = object.get("name")?.as_str()?.to_string();
    let plain = |s: &str| !s.is_empty() && !s.contains(char::is_whitespace);
    if !plain(&line) {
        return None;
    }
    if let Some(val) = object.get("val") {
        let val = val.as_str()?;
        if !plain(val) || val.contains('=') {
            return None;
        }
        line.push(' ');
        line.push_str(val);
    }
    if let Some(attrs) = object.get("attrs") {
        for (key, v) in attrs.as_object()? {
            let v = v.as_str()?;
            if !plain(key) || key.contains('=') || v.contains(char::is_whitespace) {
                return None;
            }
            line.push_str(&format!(" {key}={v}"));
        }
    }
    Some(line)
}
