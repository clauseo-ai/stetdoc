//! The MCP tools, made from the `clap` tree that also gives `--help`, the skill's command table and the CLI reference, so a
//! tool and its command cannot drift apart. Each command the skill teaches is a tool (a nested one such as `skills get`
//! is `skills_get`), each of its flags and positionals a property, and a call becomes the command line the CLI would parse.
//! The edit tool's list of operations comes from `capabilities()`, like the `capabilities` command.

use super::confine::Root;
use crate::cli::Cli;
use crate::output::Failure;
use clap::{Arg, ArgAction, CommandFactory, ValueHint};
use serde_json::{Map, Value, json};
use std::any::TypeId;
use std::ffi::OsString;

/// The commands that are not tools: `init` writes into agents' folders and `mcp` is this server.
const NOT_TOOLS: [&str; 2] = ["init", "mcp"];

/// The arguments that are not properties: help, the version, and `--json`, which a tool always gets.
const NOT_PROPERTIES: [&str; 3] = ["help", "version", "json"];

/// The edit tool's property for the batch, which the CLI reads from stdin.
pub const BATCH: &str = "batch";

/// One tool.
pub struct Tool {
    /// The tool's name: the command's words joined with `_`.
    pub name: String,
    /// The command's words, such as `["skills", "get"]`.
    path: Vec<String>,
    /// The description and the input schema, as `tools/list` shows them.
    pub listing: Value,
    args: Vec<Spec>,
}

struct Spec {
    id: String,
    long: Option<String>,
    positional: bool,
    kind: Kind,
    path: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Flag,
    Text,
    Integer,
    List,
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
    for arg in command.get_arguments() {
        let id = arg.get_id().as_str().to_string();
        if NOT_PROPERTIES.contains(&id.as_str()) {
            continue;
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
        properties.insert(id.clone(), property(arg, &spec));
        if arg.is_required_set() {
            required.push(id);
        }
        args.push(spec);
    }
    let about = command
        .get_about()
        .map(ToString::to_string)
        .unwrap_or_default();
    let mut description = format!(
        "{}. Takes the arguments of `stetdoc {}` and returns the JSON it prints with --json.",
        about.trim_end_matches('.'),
        words.join(" ")
    );
    if name == "edit" {
        let ops: Vec<String> = stetdoc_core::capabilities()
            .ops
            .into_iter()
            .map(|op| op.name)
            .collect();
        description.push_str(&format!(
            " Give the batch, as block text or JSON, in `batch`. The operations: {}. The skills_get tool with name \
             operations has each one's fields.",
            ops.join(", ")
        ));
        properties.insert(
            BATCH.into(),
            json!({
                "type": "string",
                "description": "The batch, as block text or JSON: what `stetdoc edit` reads from stdin. Its first line is `base:` with the token of the last view or receipt."
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

fn kind(arg: &Arg) -> Kind {
    if matches!(arg.get_action(), ArgAction::SetTrue) {
        return Kind::Flag;
    }
    if arg.get_value_delimiter().is_some() {
        return Kind::List;
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
        Kind::Integer => json!({ "type": "integer", "minimum": 0 }),
        Kind::List => json!({ "type": "array", "items": { "type": "string" } }),
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
    if spec.path {
        description.push_str(" A path.");
    }
    if let Some(object) = property.as_object_mut() {
        object.insert("description".into(), Value::String(description));
        if let Some(default) = arg.get_default_values().first() {
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

/// The command line a call stands for, and the edit tool's batch. With a root, every path is resolved and confined.
pub fn argv(
    tool: &Tool,
    arguments: &Map<String, Value>,
    root: Option<&Root>,
) -> Result<(Vec<OsString>, Option<String>), Failure> {
    let mut argv: Vec<OsString> = vec!["stetdoc".into()];
    argv.extend(tool.path.iter().map(OsString::from));
    let mut positionals: Vec<(usize, OsString)> = Vec::new();
    let mut batch = None;
    for (key, value) in arguments {
        if value.is_null() {
            continue;
        }
        if tool.name == "edit" && key == BATCH {
            batch = Some(scalar(key, value)?);
            continue;
        }
        let Some((index, spec)) = tool.args.iter().enumerate().find(|(_, s)| s.id == *key) else {
            let known: Vec<&str> = tool.args.iter().map(|s| s.id.as_str()).collect();
            return Err(Failure::usage(
                format!("{} takes no argument named {key}.", tool.name),
                format!("Use only: {}.", known.join(", ")),
            ));
        };
        let values: Vec<String> = match (spec.kind, value) {
            (Kind::Flag, Value::Bool(true)) => {
                argv.push(format!("--{}", spec.long.as_deref().unwrap_or(&spec.id)).into());
                continue;
            }
            (Kind::Flag, Value::Bool(false)) => continue,
            (Kind::Flag, _) => {
                return Err(Failure::usage(
                    format!("{key} is true or false."),
                    "Pass a boolean, or leave the argument out.",
                ));
            }
            (Kind::List, Value::Array(items)) => items
                .iter()
                .map(|v| scalar(key, v))
                .collect::<Result<_, _>>()?,
            _ => vec![scalar(key, value)?],
        };
        for value in values {
            let value: OsString = match root {
                Some(root) if spec.path => root.resolve(&value)?.into_os_string(),
                _ => value.into(),
            };
            match &spec.long {
                Some(long) if !spec.positional => {
                    let mut flag = OsString::from(format!("--{long}="));
                    flag.push(value);
                    argv.push(flag);
                }
                _ => positionals.push((index, value)),
            }
        }
    }
    positionals.sort_by_key(|(index, _)| *index);
    argv.push("--".into());
    argv.extend(positionals.into_iter().map(|(_, v)| v));
    Ok((argv, batch))
}

fn scalar(key: &str, value: &Value) -> Result<String, Failure> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(Failure::usage(
            format!("{key} is a string, not {value}."),
            "Pass the argument as the tool's input schema says.",
        )),
    }
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
                "compare",
                "check",
                "lookup",
                "skills_get",
                "capabilities"
            ]
        );
    }

    #[test]
    fn a_schema_follows_the_flags() {
        let all = tools();
        let view = all.iter().find(|t| t.name == "view").unwrap();
        let schema = &view.listing["inputSchema"];
        assert_eq!(schema["required"], serde_json::json!(["input"]));
        assert_eq!(schema["properties"]["budget"]["type"], "integer");
        assert_eq!(schema["properties"]["budget"]["default"], 8000);
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
    }
}
