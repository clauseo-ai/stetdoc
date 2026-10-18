//! `npm/types.d.ts` is generated from the core's `json_schema()`, and this test fails when it is stale. `cargo xtask codegen`
//! runs it with `STETDOC_BLESS=1`, which writes the file instead. The emitter reads the keywords that schemars writes
//! for the core's types: `type`, `properties`, `required`, `additionalProperties`, `items`, `prefixItems`, `oneOf`, `anyOf`,
//! `const`, `enum`, `$ref` and `description`. Any other shape is `unknown`, so a new kind of type shows up in review.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Map, Value};
use std::fmt::Write as _;
use std::path::PathBuf;

const HEADER: &str = "// The data types of stetdoc, as they cross the library in JSON.\n\
// Generated from the core's json_schema() by `cargo xtask codegen`. Do not edit it by hand.\n";

/// A JSDoc comment for a description, at an indent.
fn doc(out: &mut String, description: Option<&Value>, indent: &str) {
    let Some(text) = description.and_then(Value::as_str) else {
        return;
    };
    let text = text.replace("*/", "*\\/");
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() == 1 {
        writeln!(out, "{indent}/** {} */", lines[0]).unwrap();
        return;
    }
    writeln!(out, "{indent}/**").unwrap();
    for line in lines {
        if line.is_empty() {
            writeln!(out, "{indent} *").unwrap();
        } else {
            writeln!(out, "{indent} * {line}").unwrap();
        }
    }
    writeln!(out, "{indent} */").unwrap();
}

fn primitive(name: &str) -> &'static str {
    match name {
        "string" => "string",
        "integer" | "number" => "number",
        "boolean" => "boolean",
        "null" => "null",
        "array" => "unknown[]",
        "object" => "{ [key: string]: unknown }",
        _ => "unknown",
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Joins alternatives into a union, without repeats.
fn union(parts: Vec<String>) -> String {
    let mut seen: Vec<String> = Vec::new();
    for part in parts {
        if !seen.contains(&part) {
            seen.push(part);
        }
    }
    seen.join(" | ")
}

/// The TypeScript of one schema, at an indent (for the fields of an object written over several lines).
fn ty(schema: &Value, indent: &str) -> String {
    let Some(object) = schema.as_object() else {
        return if schema == &Value::Bool(false) {
            "never".into()
        } else {
            "unknown".into()
        };
    };
    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
        return reference
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_string();
    }
    if let Some(value) = object.get("const") {
        return value.to_string();
    }
    if let Some(values) = object.get("enum").and_then(Value::as_array) {
        return union(values.iter().map(Value::to_string).collect());
    }
    for key in ["oneOf", "anyOf"] {
        if let Some(alternatives) = object.get(key).and_then(Value::as_array) {
            return union(
                alternatives
                    .iter()
                    .map(|alt| {
                        let t = ty(alt, indent);
                        if t.contains(" | ") && !t.starts_with('{') {
                            format!("({t})")
                        } else {
                            t
                        }
                    })
                    .collect(),
            );
        }
    }
    match object.get("type") {
        Some(Value::Array(types)) => union(
            types
                .iter()
                .filter_map(Value::as_str)
                .map(|name| typed(object, name, indent))
                .collect(),
        ),
        Some(Value::String(name)) => typed(object, name, indent),
        _ => "unknown".into(),
    }
}

/// The TypeScript of a schema whose `type` is `name`.
fn typed(object: &Map<String, Value>, name: &str, indent: &str) -> String {
    match name {
        "array" => {
            if let Some(items) = object.get("prefixItems").and_then(Value::as_array) {
                let items: Vec<String> = items.iter().map(|item| ty(item, indent)).collect();
                return format!("[{}]", items.join(", "));
            }
            match object.get("items") {
                Some(items) => {
                    let t = ty(items, indent);
                    if t.contains(' ') && !t.starts_with('{') && !t.starts_with('[') {
                        format!("({t})[]")
                    } else {
                        format!("{t}[]")
                    }
                }
                None => "unknown[]".into(),
            }
        }
        "object" => object_type(object, indent),
        other => primitive(other).into(),
    }
}

fn object_type(object: &Map<String, Value>, indent: &str) -> String {
    let properties = object.get("properties").and_then(Value::as_object);
    let extra = match object.get("additionalProperties") {
        Some(Value::Bool(false)) | None => None,
        Some(Value::Bool(true)) => Some("unknown".to_string()),
        Some(schema) => Some(ty(schema, indent)),
    };
    let Some(properties) = properties.filter(|p| !p.is_empty()) else {
        return format!(
            "{{ [key: string]: {} }}",
            extra.unwrap_or_else(|| "unknown".into())
        );
    };
    let required: Vec<&str> = object
        .get("required")
        .and_then(Value::as_array)
        .map(|names| names.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let inner = format!("{indent}  ");
    let mut out = String::from("{\n");
    for (name, schema) in properties {
        doc(&mut out, schema.get("description"), &inner);
        let key = if is_identifier(name) {
            name.clone()
        } else {
            Value::String(name.clone()).to_string()
        };
        let optional = if required.contains(&name.as_str()) {
            ""
        } else {
            "?"
        };
        writeln!(out, "{inner}{key}{optional}: {};", ty(schema, &inner)).unwrap();
    }
    if let Some(extra) = extra {
        writeln!(out, "{inner}[key: string]: {extra};").unwrap();
    }
    out.push_str(indent);
    out.push('}');
    out
}

/// The whole file: one exported type for each definition, in name order.
fn emit(schema: &str) -> String {
    let schema: Value = serde_json::from_str(schema).unwrap();
    let defs = schema["$defs"].as_object().expect("the schema has $defs");
    let mut names: Vec<&String> = defs.keys().collect();
    names.sort();
    let mut out = String::from(HEADER);
    for name in names {
        out.push('\n');
        doc(&mut out, defs[name].get("description"), "");
        writeln!(out, "export type {name} = {};", ty(&defs[name], "")).unwrap();
    }
    out
}

fn path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("npm/types.d.ts")
}

#[test]
fn the_typescript_types_are_generated_from_the_schema() {
    let generated = emit(&stetdoc_core::json_schema());
    let path = path();
    if std::env::var_os("STETDOC_BLESS").is_some() {
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == generated,
        "{} is stale: the core's public types changed. Run `cargo xtask codegen` and commit the result.",
        path.display()
    );
}

#[test]
fn the_emitter_writes_each_shape() {
    let schema = r##"{"$defs": {
        "A": {"description": "An object.", "type": "object", "properties": {
            "x": {"type": "string"}, "y": {"type": ["integer", "null"]}, "ref": {"$ref": "#/$defs/B"},
            "pair": {"type": "array", "prefixItems": [{"type": "string"}, {"type": "string"}]},
            "list": {"type": "array", "items": {"anyOf": [{"$ref": "#/$defs/B"}, {"type": "null"}]}}},
            "required": ["x"]},
        "B": {"oneOf": [{"type": "string", "const": "b"}, {"type": "object", "properties": {"c": {"type": "boolean"}}, "required": ["c"]}]},
        "C": {"type": "string", "enum": ["one", "two"]},
        "D": {"type": "object", "additionalProperties": {"type": "string"}}
    }}"##;
    let out = emit(schema);
    assert!(
        out.contains("/** An object. */\nexport type A = {\n"),
        "{out}"
    );
    assert!(
        out.contains("  x: string;\n  y?: number | null;\n"),
        "{out}"
    );
    assert!(out.contains("  ref?: B;\n"), "{out}");
    assert!(out.contains("  pair?: [string, string];\n"), "{out}");
    assert!(out.contains("  list?: (B | null)[];\n"), "{out}");
    assert!(
        out.contains("export type B = \"b\" | {\n  c: boolean;\n};"),
        "{out}"
    );
    assert!(out.contains("export type C = \"one\" | \"two\";"), "{out}");
    assert!(
        out.contains("export type D = { [key: string]: string };"),
        "{out}"
    );
}
