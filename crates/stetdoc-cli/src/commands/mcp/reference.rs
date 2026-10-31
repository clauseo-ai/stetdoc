//! `docs/reference/mcp-tools.md`: every tool `stetdoc mcp` serves, with its description and its arguments, rendered from
//! the same listings `tools/list` sends, so the page cannot drift from the server. `cargo xtask docs` writes it through
//! the test below, and without `STETDOC_BLESS` that test is its drift check, like the CLI reference's.

use super::tools::tools;
use serde_json::Value;
use std::fmt::Write;

/// The page: a table of the tools, then each tool's arguments.
fn reference() -> String {
    let tools = tools();
    let mut out = String::from(
        "<!-- Generated from the tool definitions of `stetdoc mcp` by `cargo xtask docs`: do not edit it by hand. -->\n\n\
         # MCP tools\n\n\
         Each tool is a \
         command of the [CLI](cli.md), and each description is the one `tools/list` sends. Every tool also returns its \
         result as JSON in `structuredContent`.\n\n\
         | Tool | Description | Arguments |\n|---|---|---|\n",
    );
    let mut sections = String::new();
    for tool in &tools {
        let listing = &tool.listing;
        let description = cell(listing["description"].as_str().unwrap_or_default());
        let args = arguments(listing);
        let names: Vec<String> = args
            .iter()
            .map(|a| {
                if a.required {
                    format!("`{}` (required)", a.name)
                } else {
                    format!("`{}`", a.name)
                }
            })
            .collect();
        let _ = writeln!(
            out,
            "| [`{name}`](#{name}) | {description} | {} |",
            names.join(", "),
            name = tool.name
        );
        let _ = write!(sections, "\n## `{}`\n\n{description}\n\n", tool.name);
        if args.is_empty() {
            sections.push_str("No arguments.\n");
            continue;
        }
        sections.push_str("| Argument | Type | Required | Meaning |\n|---|---|---|---|\n");
        for a in args {
            let _ = writeln!(
                sections,
                "| `{}` | {} | {} | {} |",
                a.name,
                a.kind,
                if a.required { "yes" } else { "no" },
                cell(&a.meaning)
            );
        }
    }
    out.push_str(&sections);
    out
}

/// One argument of a tool, as the page shows it.
struct Argument {
    name: String,
    kind: String,
    required: bool,
    meaning: String,
}

/// A tool's arguments, the required ones first, each group in the order of the input schema.
fn arguments(listing: &Value) -> Vec<Argument> {
    let schema = &listing["inputSchema"];
    let required: Vec<&str> = schema["required"]
        .as_array()
        .map(|r| r.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let mut args: Vec<Argument> = schema["properties"]
        .as_object()
        .map(|p| {
            p.iter()
                .map(|(name, property)| {
                    let mut meaning = property["description"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    if let Some(default) = property.get("default") {
                        let default = default
                            .as_str()
                            .map_or_else(|| default.to_string(), str::to_string);
                        let _ = write!(meaning, " Default `{default}`.");
                    }
                    Argument {
                        name: name.clone(),
                        kind: kind(property),
                        required: required.contains(&name.as_str()),
                        meaning: meaning.trim().to_string(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    args.sort_by_key(|a| !a.required);
    args
}

/// A property's JSON type in words: `string`, `one of `a`, `b``, `array of strings`, `array of `{old, new}``.
fn kind(property: &Value) -> String {
    let choices: Vec<String> = property["enum"]
        .as_array()
        .map(|e| {
            e.iter()
                .filter_map(Value::as_str)
                .map(|c| format!("`{c}`"))
                .collect()
        })
        .unwrap_or_default();
    if !choices.is_empty() {
        return format!("one of {}", choices.join(", "));
    }
    match property["type"].as_str().unwrap_or_default() {
        "array" => {
            let items = &property["items"];
            match items["type"].as_str().unwrap_or_default() {
                "string" => "array of strings".into(),
                "integer" => "array of integers".into(),
                "object" => {
                    let keys: Vec<&str> = items["properties"]
                        .as_object()
                        .map(|p| p.keys().map(String::as_str).collect())
                        .unwrap_or_default();
                    if !keys.is_empty() && keys.len() <= 4 {
                        format!("array of `{{{}}}`", keys.join(", "))
                    } else {
                        "array of objects".into()
                    }
                }
                _ => "array".into(),
            }
        }
        other => other.to_string(),
    }
}

/// Text that goes in a table cell: one line, with its pipes escaped.
fn cell(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "\\|")
}

#[test]
fn the_mcp_tool_reference_is_generated_from_the_server() {
    crate::generated::check("docs/reference/mcp-tools.md", &reference());
}
