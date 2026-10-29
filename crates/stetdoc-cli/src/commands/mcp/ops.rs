//! The edit tool's `ops`: the operations as typed JSON, made from `capabilities()`, which reads the same table as the
//! block-text parser and the operations reference, so the tool's schema cannot drift from what a batch accepts.
//!
//! The items are one flat object whose `op` names the operation, with every operation's fields as optional properties,
//! and not a `oneOf` of one object per operation: clients such as Gemini's refuse or flatten unions, and Claude's refuse
//! them at the top of a schema. The `op` property lists each operation's fields and what it does in a few words. Only
//! the fields whose meaning their name does not give carry a description (`NOTES`): the schema is sent on every turn,
//! and `skills_get` with `name: operations` has every field in full. The core refuses a field the operation does not
//! have, and names the operation's index. A property's `attrs` is a list of `name=value` texts, since a free-form object
//! has no properties to declare.

use crate::output::Failure;
use serde_json::{Map, Value, json};
use stetdoc_core::{OpInfo, capabilities};

/// The schema of the `ops` property.
pub fn schema() -> Value {
    let ops = capabilities().ops;
    let names: Vec<&str> = ops.iter().map(|op| op.name.as_str()).collect();
    let mut properties = Map::new();
    properties.insert(
        "op".into(),
        json!({
            "type": "string",
            "enum": names,
            "description": op_description(&ops),
        }),
    );
    let mut order: Vec<&str> = Vec::new();
    for op in &ops {
        for field in &op.fields {
            if !order.contains(&field.name.as_str()) {
                order.push(&field.name);
            }
        }
    }
    for name in order {
        properties.insert(name.into(), field_schema(&ops, name));
    }
    json!({
        "type": "array",
        "minItems": 1,
        "description": "The operations, in order, all applied or none.",
        "items": {
            "type": "object",
            "properties": properties,
            "required": ["op"],
        },
    })
}

/// The fields that pick a range inside a paragraph, which several operations share and the description names once.
const RANGE: [&str; 4] = ["find", "occurrence", "start", "end"];

/// The fields whose name does not say enough, with what they hold. Every other field has no description.
const NOTES: [(&str, &str); 7] = [
    (
        "ref",
        "The block: a paragraph (p4), a table (t1) or a row (r2).",
    ),
    (
        "find",
        "Text of the paragraph, matched once unless occurrence says which.",
    ),
    ("occurrence", "Which match of find, from 1."),
    ("to", "The last paragraph of a range of blocks."),
    ("comment", "A comment of the thread, such as c3."),
    (
        "ids",
        "Revision numbers, as view revisions shows them after #.",
    ),
    (
        "text",
        "The text of the comment or reply; a newline starts a paragraph.",
    ),
];

/// Each operation with its fields, a star marking the required ones, then what it does.
fn op_description(ops: &[OpInfo]) -> String {
    let mut text = format!(
        "Fields (* required, | one of, range: {}): ",
        RANGE.join(", ")
    );
    let lines: Vec<String> = ops
        .iter()
        .map(|op| {
            let ranged = RANGE.iter().all(|r| op.fields.iter().any(|f| f.name == *r));
            let mut fields: Vec<String> = op
                .fields
                .iter()
                .filter(|f| !op.one_of.contains(&f.name))
                .filter(|f| !(ranged && RANGE.contains(&f.name.as_str())))
                .map(|f| {
                    if f.required {
                        format!("{}*", f.name)
                    } else {
                        f.name.clone()
                    }
                })
                .collect();
            if !op.one_of.is_empty() {
                fields.push(op.one_of.join("|"));
            }
            if ranged {
                fields.push("range".into());
            }
            // The words up to the first comma, colon or full stop are enough to pick the operation; the reference has the
            // rest.
            let summary = op
                .summary
                .split([',', ':', '.'])
                .next()
                .unwrap_or(op.summary.as_str())
                .trim();
            format!("{} ({}): {summary}.", op.name, fields.join(", "))
        })
        .collect();
    text.push_str(&lines.join(" "));
    text
}

/// The schema of one field, from every operation that has it: its type, its choices, and a description from `NOTES`.
fn field_schema(ops: &[OpInfo], name: &str) -> Value {
    let uses: Vec<&stetdoc_core::FieldInfo> = ops
        .iter()
        .flat_map(|op| op.fields.iter().filter(|f| f.name == name))
        .collect();
    let mut choices: Vec<String> = Vec::new();
    for field in &uses {
        for choice in &field.choices {
            if !choices.contains(choice) {
                choices.push(choice.clone());
            }
        }
    }
    let kind = uses.first().map_or("text", |f| f.kind.as_str());
    let mut schema = match kind {
        "number" => json!({ "type": "integer" }),
        "bool" => json!({ "type": "boolean" }),
        "list" => json!({ "type": "array", "items": { "type": "string" } }),
        "number list" => json!({ "type": "array", "items": { "type": "integer" } }),
        "properties" => json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Element name, such as jc." },
                    "val": { "type": "string" },
                    "attrs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Other attributes, each name=value."
                    },
                },
                "required": ["name"],
            },
        }),
        _ if !choices.is_empty() => json!({ "type": "string", "enum": choices }),
        _ => json!({ "type": "string" }),
    };
    if let (Some((_, note)), Some(object)) = (
        NOTES.iter().find(|(field, _)| *field == name),
        schema.as_object_mut(),
    ) {
        object.insert("description".into(), Value::String((*note).to_string()));
    }
    schema
}

/// The batch, as JSON text, that the `ops` of a call stand for. Without a base, the batch has none, and the edit reads its
/// refs against a .docx as it is now.
pub fn batch(ops: &Value, base: Option<&str>) -> Result<String, Failure> {
    let wrong = |why: String| {
        Failure::usage(
            why,
            "Pass ops as a list of objects, such as [{\"op\": \"replace\", \"ref\": \"p4\", \"find\": \"thirty\", \"with\": \"sixty\"}].",
        )
    };
    let Value::Array(items) = ops else {
        return Err(wrong("ops is a list of operations.".into()));
    };
    let kinds = field_kinds();
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let Value::Object(fields) = item else {
            return Err(wrong(format!("Operation {index} of ops is not an object.")));
        };
        let mut op = Map::new();
        for (key, value) in fields {
            if value.is_null() {
                continue;
            }
            if key == "props" {
                op.insert(key.clone(), props(value, index)?);
            } else {
                let kind = kinds
                    .iter()
                    .find(|(name, _)| name == key)
                    .map(|(_, kind)| kind.as_str());
                op.insert(key.clone(), typed(value, kind));
            }
        }
        out.push(Value::Object(op));
    }
    let mut batch = Map::new();
    if let Some(base) = base {
        batch.insert("base".into(), Value::String(base.to_string()));
    }
    batch.insert("ops".into(), Value::Array(out));
    Ok(Value::Object(batch).to_string())
}

/// The kind of each field of any operation, by its name, as `capabilities()` gives it.
fn field_kinds() -> Vec<(String, String)> {
    let mut kinds: Vec<(String, String)> = Vec::new();
    for op in capabilities().ops {
        for field in op.fields {
            if !kinds.iter().any(|(name, _)| *name == field.name) {
                kinds.push((field.name, field.kind));
            }
        }
    }
    kinds
}

/// A field's value with a boolean or a number that came as text made the type the field takes, since some clients send
/// `"true"` or `"2"`. Anything else is passed on as it came, for the core to judge.
fn typed(value: &Value, kind: Option<&str>) -> Value {
    match (kind, value) {
        (Some("bool"), Value::String(_)) => {
            super::call::flag(value).map_or_else(|| value.clone(), Value::Bool)
        }
        (Some("number"), Value::String(s)) => s
            .trim()
            .parse::<u64>()
            .map_or_else(|_| value.clone(), Value::from),
        _ => value.clone(),
    }
}

/// A `set` operation's properties, with each `attrs` list of `name=value` texts made the object the core reads.
fn props(value: &Value, index: usize) -> Result<Value, Failure> {
    let Value::Array(list) = value else {
        return Ok(value.clone());
    };
    let mut out = Vec::with_capacity(list.len());
    for prop in list {
        let mut prop = prop.clone();
        if let Some(Value::Array(attrs)) = prop.get("attrs") {
            let mut object = Map::new();
            for attr in attrs {
                let pair = attr.as_str().and_then(|a| a.split_once('='));
                let Some((name, val)) = pair else {
                    return Err(Failure::usage(
                        format!(
                            "An attribute of operation {index}'s props is {attr}, not `name=value`."
                        ),
                        "Write each attribute as name=value, such as before=6pt.",
                    ));
                };
                object.insert(
                    name.trim().to_string(),
                    Value::String(val.trim().to_string()),
                );
            }
            prop["attrs"] = Value::Object(object);
        }
        out.push(prop);
    }
    Ok(Value::Array(out))
}
