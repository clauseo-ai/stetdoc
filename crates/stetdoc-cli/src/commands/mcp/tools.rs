//! The MCP tools, made from the `clap` tree that also gives `--help`, the skill's command table and the CLI reference, so a
//! tool and its command cannot drift apart. Each command the skill teaches is a tool (a nested one such as `skills get`
//! is `skills_get`), each of its flags and positionals a property, and a call becomes the command line the CLI would parse
//! (`call.rs`). The words of each description come from `describe.rs`, short, since they are sent on every turn.
//! The edit tool's list of operations comes from `capabilities()`, like the `capabilities` command, which is not a tool.

use super::{describe, ops};
use crate::cli::Cli;
use clap::{Arg, ArgAction, CommandFactory, ValueHint};
use serde_json::{Map, Value, json};
use std::any::TypeId;

/// The commands that are not tools: `init` writes into agents' folders, `mcp` is this server, and `capabilities` lists
/// what `skills_get` teaches (the operations and the error codes), for programs rather than agents.
const NOT_TOOLS: [&str; 3] = ["init", "mcp", "capabilities"];

/// The arguments that are not properties: help, the version, and `--json`, which a tool always gets.
const NOT_PROPERTIES: [&str; 3] = ["help", "version", "json"];

/// The edit tool's property for the batch as text, which the CLI reads from stdin.
pub(super) const BATCH: &str = "batch";

/// The edit tool's property for the operations as typed objects.
pub(super) const OPS: &str = "ops";

/// The edit tool's property for the base token of `ops`.
pub(super) const BASE: &str = "base";

/// The edit tool's property for `--ops FILE`, renamed so that `ops` can be the typed operations.
const OPS_FILE: &str = "ops_file";

/// The replace tool's property for its OLD NEW pairs, an array of `{old, new}` objects that the CLI takes as positionals.
pub(super) const PAIRS: &str = "pairs";

/// The property of the tools that write for a receipt in full, not cut to `RECEIPT_BYTES`.
pub(super) const DETAIL: &str = "detail";

/// The tools that write a receipt.
pub(super) const WRITERS: [&str; 4] = ["edit", "replace", "comment", "pack"];

/// About how many bytes a receipt's text takes without `detail`.
pub(super) const RECEIPT_BYTES: usize = 2_000;

/// The comment tool's property for its ANCHOR TEXT pairs, an array of `{anchor, text}` objects.
pub(super) const COMMENTS: &str = "comments";

/// One tool.
pub struct Tool {
    /// The tool's name: the command's words joined with `_`.
    pub name: String,
    /// The command's words, such as `["skills", "get"]`.
    pub(super) path: Vec<String>,
    /// The description and the input schema, as `tools/list` shows them.
    pub listing: Value,
    pub(super) args: Vec<Spec>,
}

pub(super) struct Spec {
    pub(super) id: String,
    pub(super) long: Option<String>,
    pub(super) positional: bool,
    pub(super) kind: Kind,
    pub(super) path: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Kind {
    Flag,
    Text,
    Integer,
    List,
    /// A flag that may be given more than once: one string or a list of them.
    Repeat,
}

/// Every tool, in the order of the command tree.
pub fn tools() -> Vec<Tool> {
    let cli = Cli::command();
    let mut tools = Vec::new();
    for sub in cli.get_subcommands() {
        if NOT_TOOLS.contains(&sub.get_name()) {
            continue;
        }
        if sub.has_subcommands() {
            for leaf in sub.get_subcommands() {
                tools.push(tool(&[sub.get_name(), leaf.get_name()], leaf));
            }
        } else {
            tools.push(tool(&[sub.get_name()], sub));
        }
    }
    tools
}

fn tool(words: &[&str], command: &clap::Command) -> Tool {
    let name = words.join("_");
    let mut properties = Map::new();
    let mut required = Vec::new();
    let mut args = Vec::new();
    let apart = conflicts(command);
    for arg in command.get_arguments() {
        let mut id = arg.get_id().as_str().to_string();
        if NOT_PROPERTIES.contains(&id.as_str()) || describe::hidden(&name, &id) {
            continue;
        }
        if name == "edit" && id == OPS {
            id = OPS_FILE.to_string();
        }
        let spec = Spec {
            id: id.clone(),
            long: arg.get_long().map(str::to_string),
            positional: arg.is_positional(),
            kind: kind(arg),
            path: matches!(
                arg.get_value_hint(),
                ValueHint::AnyPath | ValueHint::FilePath | ValueHint::DirPath
            ),
        };
        let mut listed = property(arg, &spec);
        let said = describe::property(&name, &id);
        if let (Some(said), Some(object)) = (said, listed.as_object_mut()) {
            object.insert("description".into(), Value::String(said.to_string()));
        } else if let Some(others) = apart.get(&id)
            && let Some(Value::String(text)) = listed.get_mut("description")
        {
            let names: Vec<String> = others.iter().map(|o| format!("`{o}`")).collect();
            text.push_str(&format!(" Cannot be used with {}.", names.join(", ")));
        }
        properties.insert(id.clone(), listed);
        if arg.is_required_set() {
            required.push(id);
        }
        args.push(spec);
    }
    let description = describe::tool(&name).map_or_else(
        || {
            let about = command
                .get_about()
                .map(ToString::to_string)
                .unwrap_or_default();
            format!("{}.", about.trim_end_matches('.'))
        },
        str::to_string,
    );
    if name == "edit" {
        properties.insert(OPS.into(), ops::schema());
        properties.insert(
            BASE.into(),
            json!({
                "type": "string",
                "description": "The base token of the view or receipt the refs come from: for a working copy, not a .docx."
            }),
        );
        properties.insert(
            BATCH.into(),
            json!({
                "type": "string",
                "description": "The operations as block text or JSON, instead of ops."
            }),
        );
    }
    if WRITERS.contains(&name.as_str()) {
        properties.insert(
            DETAIL.into(),
            json!({ "type": "boolean", "description": "The whole receipt, not a summary of about 2 KB." }),
        );
    }
    if name == "comment" {
        // One comment in `anchor` and `text`, or several in `comments`, which the CLI takes as ANCHOR TEXT pairs.
        properties.remove(PAIRS);
        required.retain(|r| r != PAIRS);
        properties.insert(
            "anchor".into(),
            json!({ "type": "string", "description": "The text to comment on." }),
        );
        properties.insert(
            "text".into(),
            json!({ "type": "string", "description": "The comment." }),
        );
        properties.insert(
            COMMENTS.into(),
            json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "anchor": { "type": "string" },
                        "text": { "type": "string" }
                    },
                    "required": ["anchor", "text"],
                    "additionalProperties": false
                },
                "description": "Several comments, each an anchor and a text."
            }),
        );
    }
    if name == "replace" {
        properties.insert(
            PAIRS.into(),
            json!({
                "type": "array",
                "minItems": 1,
                "items": {
                    "type": "object",
                    "properties": {
                        "old": { "type": "string" },
                        "new": { "type": "string" }
                    },
                    "required": ["old", "new"],
                    "additionalProperties": false
                },
                "description": "Each old text and the new text it becomes."
            }),
        );
    }
    let listing = json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
    });
    Tool {
        name,
        path: words.iter().map(|w| (*w).to_string()).collect(),
        listing,
        args,
    }
}

/// For each argument, the arguments it cannot be used with, both ways round: those it or they name as conflicts, and the
/// others of a group that takes one. The input schema says so in each property's description, and not with `oneOf`,
/// which clients such as the Claude API refuse at the top of a tool's schema.
fn conflicts(command: &clap::Command) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut apart: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    let mut add = |a: &str, b: &str| {
        if a == b {
            return;
        }
        for (x, y) in [(a, b), (b, a)] {
            let list = apart.entry(x.to_string()).or_default();
            if !list.iter().any(|n| n == y) {
                list.push(y.to_string());
            }
        }
    };
    for arg in command.get_arguments() {
        for other in command.get_arg_conflicts_with(arg) {
            add(arg.get_id().as_str(), other.get_id().as_str());
        }
    }
    for group in command.get_groups() {
        if group.clone().is_multiple() {
            continue;
        }
        let members: Vec<&str> = group.get_args().map(clap::Id::as_str).collect();
        for a in &members {
            for b in &members {
                add(a, b);
            }
        }
    }
    let order: Vec<String> = command
        .get_arguments()
        .map(|a| a.get_id().as_str().to_string())
        .collect();
    for list in apart.values_mut() {
        list.sort_by_key(|n| order.iter().position(|o| o == n));
    }
    apart
}

fn kind(arg: &Arg) -> Kind {
    if matches!(arg.get_action(), ArgAction::SetTrue) {
        return Kind::Flag;
    }
    if arg.get_value_delimiter().is_some() {
        return Kind::List;
    }
    if matches!(arg.get_action(), ArgAction::Append) {
        return Kind::Repeat;
    }
    let type_id = arg.get_value_parser().type_id();
    let integers = [
        TypeId::of::<u8>(),
        TypeId::of::<u16>(),
        TypeId::of::<u32>(),
        TypeId::of::<u64>(),
        TypeId::of::<usize>(),
    ];
    if integers.iter().any(|t| type_id == *t) {
        Kind::Integer
    } else {
        Kind::Text
    }
}

fn property(arg: &Arg, spec: &Spec) -> Value {
    let help = arg.get_help().map(ToString::to_string).unwrap_or_default();
    let mut property = match spec.kind {
        Kind::Flag => json!({ "type": "boolean" }),
        Kind::Integer => json!({ "type": "integer" }),
        Kind::List => json!({ "type": "array", "items": { "type": "string" } }),
        // A list, not a union of a string and a list, which clients such as Gemini's do not take. A lone string still works.
        Kind::Repeat => json!({ "type": "array", "items": { "type": "string" } }),
        Kind::Text => {
            let choices: Vec<String> = arg
                .get_possible_values()
                .iter()
                .filter(|v| !v.is_hide_set())
                .map(|v| v.get_name().to_string())
                .collect();
            if choices.is_empty() {
                json!({ "type": "string" })
            } else {
                json!({ "type": "string", "enum": choices })
            }
        }
    };
    let mut description = help;
    if !description.is_empty() && !description.ends_with(['.', '?']) {
        description.push('.');
    }
    if spec.path {
        description.push_str(" A path.");
    }
    if let Some(object) = property.as_object_mut() {
        object.insert("description".into(), Value::String(description));
        // An integer's default is said in words where it matters (a page of about 3000 tokens); a choice's default
        // says what happens when it is left out.
        if let Some(default) = arg.get_default_values().first()
            && spec.kind != Kind::Integer
        {
            let default = default.to_string_lossy().into_owned();
            let value = match spec.kind {
                Kind::Integer => default
                    .parse::<u64>()
                    .map_or(Value::String(default), Value::from),
                _ => Value::String(default),
            };
            object.insert("default".into(), value);
        }
    }
    property
}

#[cfg(test)]
mod tests {
    use super::tools;

    #[test]
    fn every_command_the_skill_teaches_is_a_tool() {
        let names: Vec<String> = tools().into_iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "unpack",
                "pack",
                "view",
                "inspect",
                "edit",
                "replace",
                "comment",
                "compare",
                "check",
                "lookup",
                "skills_get"
            ]
        );
    }

    #[test]
    fn clause_is_a_list_and_a_lone_string_still_works() {
        let all = tools();
        for name in ["view", "replace", "comment"] {
            let tool = all.iter().find(|t| t.name == name).unwrap();
            let clause = &tool.listing["inputSchema"]["properties"]["clause"];
            assert_eq!(clause["type"], "array", "{name}: {clause}");
            assert_eq!(clause["items"]["type"], "string", "{name}: {clause}");
        }
        let view = all.iter().find(|t| t.name == "view").unwrap();
        let argv_of = |arguments: serde_json::Value| -> Vec<String> {
            let call =
                super::super::call::argv(view, arguments.as_object().unwrap(), None, &[]).unwrap();
            call.argv
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect()
        };
        let many = argv_of(serde_json::json!({ "input": "a.docx", "clause": ["4.01", "4.02"] }));
        assert!(
            many.contains(&"--clause=4.01".to_string())
                && many.contains(&"--clause=4.02".to_string()),
            "{many:?}"
        );
        let one = argv_of(serde_json::json!({ "input": "a.docx", "clause": "4.01" }));
        assert!(one.contains(&"--clause=4.01".to_string()), "{one:?}");
    }

    #[test]
    fn a_schema_follows_the_flags() {
        let all = tools();
        let view = all.iter().find(|t| t.name == "view").unwrap();
        let schema = &view.listing["inputSchema"];
        assert_eq!(schema["required"], serde_json::json!(["input"]));
        assert_eq!(schema["properties"]["budget"]["type"], "integer");
        assert_eq!(schema["properties"]["all"]["type"], "boolean");
        // The pairs the CLI refuses are said once, in the tool's description, and not in every property's.
        let said = |name: &str| {
            schema["properties"][name]["description"]
                .as_str()
                .unwrap()
                .to_string()
        };
        let about = view.listing["description"].as_str().unwrap();
        assert!(
            about.contains("Use one of outline, range, clause, revisions, comments, story"),
            "{about}"
        );
        assert!(!said("find").contains("Cannot"), "{}", said("find"));
        assert_eq!(schema["properties"]["outline"]["type"], "boolean");
        assert_eq!(
            schema["properties"]["mode"]["enum"],
            serde_json::json!(["final", "original", "redline"])
        );
        assert!(schema["properties"].get("json").is_none());
        let edit = all.iter().find(|t| t.name == "edit").unwrap();
        assert_eq!(
            edit.listing["inputSchema"]["properties"]["scope"]["type"],
            "array"
        );
        assert!(edit.listing["inputSchema"]["properties"]["batch"].is_object());
        assert!(
            edit.listing["description"]
                .as_str()
                .unwrap()
                .contains("replace")
        );
        // The edit tool takes typed operations, generated from the operations table, beside the text batch.
        let ops = &edit.listing["inputSchema"]["properties"]["ops"];
        assert_eq!(ops["items"]["required"], serde_json::json!(["op"]));
        assert_eq!(
            ops["items"]["properties"]["op"]["enum"]
                .as_array()
                .unwrap()
                .len(),
            stetdoc_core::capabilities().ops.len()
        );
        assert_eq!(ops["items"]["properties"]["occurrence"]["type"], "integer");
        assert!(edit.listing["inputSchema"]["properties"]["ops_file"].is_object());
        // offsets serves programs that count UTF-16 units, and is the CLI's only.
        assert!(edit.listing["inputSchema"]["properties"]["offsets"].is_null());
        // Each editing tool carries its rules, so a client that drops the server's instructions still has them.
        let replace = all.iter().find(|t| t.name == "replace").unwrap();
        assert!(
            replace.listing["description"]
                .as_str()
                .unwrap()
                .contains("all: true"),
            "{}",
            replace.listing
        );
        assert!(
            edit.listing["description"]
                .as_str()
                .unwrap()
                .contains("never dry-run first"),
            "{}",
            edit.listing
        );
    }
}
