//! The edit tool's `ops`: the operations as typed JSON, made from `capabilities()`, which reads the same table as the
//! block-text parser and the operations reference, so the tool's schema cannot drift from what a batch accepts.
//!
//! The items are one flat object whose `op` names the operation, with every operation's fields as optional properties,
//! and not a `oneOf` of one object per operation: clients such as Gemini's refuse or flatten unions, and Claude's refuse
//! them at the top of a schema. The `op` property lists each operation's fields, and a field that two operations use with
//! different meanings says which meaning goes with which. The core refuses a field the operation does not have, and names
//! the operation's index. A property's `attrs` is a list of `name=value` texts, since a free-form object has no
//! properties to declare.

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
        "description": "The operations, applied in order as one edit, all or none. Each is an object: `op` names the operation and the other properties are its fields. Not with `batch`.",
        "items": {
            "type": "object",
            "properties": properties,
            "required": ["op"],
        },
    })
}

/// The fields that pick a range inside a paragraph, which several operations share and the description names once.
const RANGE: [&str; 4] = ["find", "occurrence", "start", "end"];

/// Each operation with its fields, a star marking the required ones, then what it does.
fn op_description(ops: &[OpInfo]) -> String {
    let mut text = format!(
        "The operation. Its fields (* required; | one of; range: {}): ",
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
            // The first sentence is enough to pick the operation; the reference has the rest.
            let summary = op
                .summary
                .split_once(". ")
                .map_or(op.summary.as_str(), |(first, _)| first)
                .trim_end_matches('.');
            format!("{} ({}): {summary}.", op.name, fields.join(", "))
        })
        .collect();
    text.push_str(&lines.join(" "));
    text
}

/// The schema of one field, from every operation that has it.
fn field_schema(ops: &[OpInfo], name: &str) -> Value {
    let uses: Vec<(&str, &stetdoc_core::FieldInfo)> = ops
        .iter()
        .flat_map(|op| {
            op.fields
                .iter()
                .filter(|f| f.name == name)
                .map(|f| (op.name.as_str(), f))
        })
        .collect();
    // Operations that give the field the same meaning share one sentence.
    let mut meanings: Vec<(Vec<&str>, &str)> = Vec::new();
    let mut choices: Vec<String> = Vec::new();
    for (op, field) in &uses {
        let doc = field.doc.as_str();
        match meanings.iter_mut().find(|(_, d)| *d == doc) {
            Some((names, _)) => names.push(op),
            None => meanings.push((vec![op], doc)),
        }
        for choice in &field.choices {
            if !choices.contains(choice) {
                choices.push(choice.clone());
            }
        }
    }
    // The meaning most operations share goes first without their names, since `op` already lists each one's fields.
    meanings.sort_by_key(|(names, _)| std::cmp::Reverse(names.len()));
    let leads = match meanings.as_slice() {
        [_] => true,
        [(first, _), (second, _), ..] => first.len() > second.len(),
        [] => false,
    };
    let description = meanings
        .iter()
        .enumerate()
        .map(|(i, (names, doc))| {
            if i == 0 && leads {
                for_json(doc)
            } else {
                format!("{}: {}", names.join(", "), for_json(doc))
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let kind = uses.first().map_or("text", |(_, f)| f.kind.as_str());
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
                    "name": { "type": "string", "description": "The element's local name, such as `jc` or `spacing`." },
                    "val": { "type": "string", "description": "Its `w:val`, for the properties that have only that." },
                    "attrs": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Its other attributes, each `name=value`, such as `before=6pt`."
                    },
                },
                "required": ["name"],
            },
        }),
        _ if !choices.is_empty() => json!({ "type": "string", "enum": choices }),
        _ => json!({ "type": "string" }),
    };
    if let Some(object) = schema.as_object_mut() {
        object.insert("description".into(), Value::String(description));
    }
    schema
}

/// A field's doc without the sentences about block text, which `ops` never is.
fn for_json(doc: &str) -> String {
    doc.split_inclusive(". ")
        .filter(|sentence| !sentence.contains("block text") && !sentence.contains("JSON only"))
        .collect::<String>()
        .trim_end()
        .to_string()
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
                op.insert(key.clone(), value.clone());
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
