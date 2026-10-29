//! A tool call as the command line the CLI would parse: each property a flag or a positional, the edit tool's typed
//! operations a batch, and each path resolved, confined to the root when there is one, and looked for beside the
//! documents the session has seen when it is not there as given. Also the words of a result, said in the tool's own
//! properties.

use super::confine::Root;
use super::ops;
use super::tools::{BASE, BATCH, COMMENTS, DETAIL, Kind, OPS, PAIRS, Tool, WRITERS};
use crate::output::Failure;
use serde_json::{Map, Value};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What a call stands for: the command line, the edit tool's batch, and the paths it reads.
pub struct Call {
    /// The command line, as the CLI would parse it.
    pub argv: Vec<OsString>,
    /// The edit tool's batch, which the CLI reads from stdin.
    pub batch: Option<String>,
    /// The paths the call names that exist, as resolved.
    pub paths: Vec<PathBuf>,
    /// The receipt is wanted whole.
    pub detail: bool,
}

/// The command line a call stands for, and the edit tool's batch. With a root, every path is resolved and confined. A
/// relative path to read that is not there as given is looked for in `known`, the folders of the documents this server
/// has seen, most recent first: a client that keeps documents in a folder of its own often passes a working copy by
/// the bare name `unpack` printed beside it.
pub fn argv(
    tool: &Tool,
    arguments: &Map<String, Value>,
    root: Option<&Root>,
    known: &[PathBuf],
) -> Result<Call, Failure> {
    let mut paths = Vec::new();
    let mut argv: Vec<OsString> = vec!["stetdoc".into()];
    argv.extend(tool.path.iter().map(OsString::from));
    let mut positionals: Vec<(usize, OsString)> = Vec::new();
    let mut batch = None;
    let mut typed = None;
    let mut base = None;
    let mut first: [Option<String>; 2] = [None, None];
    let mut detail = false;
    let pairs_at = tool
        .args
        .iter()
        .position(|s| s.id == PAIRS)
        .unwrap_or(usize::MAX);
    for (key, value) in arguments {
        if value.is_null() {
            continue;
        }
        if key == DETAIL && WRITERS.contains(&tool.name.as_str()) {
            detail = flag(value).ok_or_else(|| {
                Failure::usage(
                    "detail is true or false.",
                    "Pass true or false, or leave the argument out.",
                )
            })?;
            continue;
        }
        if tool.name == "comment" && (key == "anchor" || key == "text") {
            first[usize::from(key == "text")] = Some(scalar(key, value)?);
            continue;
        }
        if tool.name == "comment" && key == COMMENTS {
            for (anchor, text) in object_pairs(value, ("anchor", "text"))? {
                positionals.push((pairs_at, anchor.into()));
                positionals.push((pairs_at, text.into()));
            }
            continue;
        }
        if tool.name == "edit" && key == BATCH {
            batch = Some(scalar(key, value)?);
            continue;
        }
        if tool.name == "edit" && key == OPS {
            typed = Some(value);
            continue;
        }
        if tool.name == "edit" && key == BASE {
            base = Some(scalar(key, value)?);
            continue;
        }
        if tool.name == "replace" && key == PAIRS {
            for (old, new) in object_pairs(value, ("old", "new"))? {
                positionals.push((pairs_at, old.into()));
                positionals.push((pairs_at, new.into()));
            }
            continue;
        }
        let listed = tool.listing["inputSchema"]["properties"]
            .as_object()
            .is_some_and(|p| p.contains_key(key.as_str()));
        let found = tool.args.iter().enumerate().find(|(_, s)| s.id == *key);
        let Some((index, spec)) = found.filter(|_| listed) else {
            let known: Vec<&str> = tool.listing["inputSchema"]["properties"]
                .as_object()
                .map(|p| p.keys().map(String::as_str).collect())
                .unwrap_or_default();
            return Err(Failure::usage(
                format!("{} takes no argument named {key}.", tool.name),
                format!("Use only: {}.", known.join(", ")),
            ));
        };
        let values: Vec<String> = match (spec.kind, value) {
            (Kind::Flag, _) => match flag(value) {
                Some(true) => {
                    argv.push(format!("--{}", spec.long.as_deref().unwrap_or(&spec.id)).into());
                    continue;
                }
                Some(false) => continue,
                None => {
                    return Err(Failure::usage(
                        format!("{key} is true or false."),
                        "Pass true or false, or leave the argument out.",
                    ));
                }
            },
            (Kind::List | Kind::Repeat, Value::Array(items)) => items
                .iter()
                .map(|v| scalar(key, v))
                .collect::<Result<_, _>>()?,
            _ => vec![scalar(key, value)?],
        };
        for value in values {
            let value: OsString = if spec.path {
                let path = locate(&value, spec.id != "output", root, known)?;
                if path.exists() {
                    paths.push(path.clone());
                }
                path.into_os_string()
            } else {
                value.into()
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
    match (typed, &batch, &base) {
        (Some(_), Some(_), _) => {
            return Err(Failure::usage(
                "The edit tool takes ops or batch, and both were given.",
                "Pass the operations once: in ops as objects, or in batch as text.",
            ));
        }
        (Some(typed), None, _) => batch = Some(ops::batch(typed, base.as_deref())?),
        (None, _, Some(_)) => {
            return Err(Failure::usage(
                "base goes with ops, and no ops were given.",
                "Pass the operations in ops, or put the base line at the top of batch.",
            ));
        }
        (None, _, None) => {}
    }
    match first {
        [Some(anchor), Some(text)] => {
            // The lone comment goes first, before those of `comments`; the sort below keeps the order of equal indexes.
            positionals.insert(0, (pairs_at, text.into()));
            positionals.insert(0, (pairs_at, anchor.into()));
        }
        [None, None] => {}
        _ => {
            return Err(Failure::usage(
                "A comment needs both anchor and text, and only one was given.",
                "Pass anchor and text together, or list the comments in comments as {\"anchor\": ..., \"text\": ...}.",
            ));
        }
    }
    if tool.name == "comment" && !positionals.iter().any(|(i, _)| *i == pairs_at) {
        return Err(Failure::usage(
            "The comment tool needs a comment: anchor and text, or comments.",
            "Pass anchor and text, or comments as [{\"anchor\": \"thirty days\", \"text\": \"Why not sixty?\"}].",
        ));
    }
    positionals.sort_by_key(|(index, _)| *index);
    argv.push("--".into());
    argv.extend(positionals.into_iter().map(|(_, v)| v));
    Ok(Call {
        argv,
        batch,
        paths,
        detail,
    })
}

/// A message or a receipt with the CLI's flags said as the tool's properties, so that an agent can act on its next step
/// as it reads: `--all` becomes `all: true`, `--range` becomes `range` and `-o` becomes `output`. A flag the tool does not
/// have, such as one of another command, stays as it is.
pub fn for_tool(text: &str, tool: &Tool) -> String {
    let detailed;
    let text = if WRITERS.contains(&tool.name.as_str()) {
        detailed = text.replace(
            "the JSON receipt (--json)",
            "the whole receipt (detail: true)",
        );
        detailed.as_str()
    } else {
        text
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('-') {
        let (before, from) = rest.split_at(at);
        out.push_str(before);
        let starts_word = out
            .chars()
            .last()
            .is_none_or(|c| c.is_whitespace() || "(`'\"".contains(c));
        let long = from.strip_prefix("--");
        let name_len = long.map_or(0, |l| {
            l.find(|c: char| !(c.is_ascii_lowercase() || c == '-'))
                .unwrap_or(l.len())
        });
        let spec = long.filter(|_| starts_word && name_len > 0).and_then(|l| {
            let name = l.get(..name_len)?.trim_end_matches('-').replace('-', "_");
            tool.args
                .iter()
                .find(|s| s.id == name && s.long.is_some() && !s.positional)
        });
        if let Some(spec) = spec {
            out.push_str(&spec.id);
            if spec.kind == Kind::Flag {
                out.push_str(": true");
            }
            rest = from.get(2 + name_len..).unwrap_or("");
            continue;
        }
        let short_output = starts_word
            && from.starts_with("-o")
            && from.get(2..3).is_none_or(|c| c == " " || c == ",")
            && tool.args.iter().any(|s| s.id == "output");
        if short_output {
            out.push_str("output");
            rest = from.get(2..).unwrap_or("");
            continue;
        }
        out.push('-');
        rest = from.get(1..).unwrap_or("");
    }
    out.push_str(rest);
    out
}

/// A boolean, or the text `true` or `false`, which some clients send for one; `None` for anything else.
pub fn flag(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
        Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

/// A path argument, resolved against the root when there is one. A relative path to read (`read`) that is not there is
/// looked for in each folder of `known`, and the first that has it wins; a path to write is never moved.
fn locate(
    value: &str,
    read: bool,
    root: Option<&Root>,
    known: &[PathBuf],
) -> Result<PathBuf, Failure> {
    let given = match root {
        Some(root) => root.resolve(value)?,
        None => PathBuf::from(value),
    };
    if !read || given.exists() || Path::new(value).is_absolute() {
        return Ok(given);
    }
    for dir in known {
        let candidate = dir.join(value);
        if candidate.exists() {
            return match root {
                Some(root) => root.check(&candidate),
                None => Ok(candidate),
            };
        }
    }
    Ok(given)
}

/// The pairs of the replace tool's `pairs` or the comment tool's `comments`, from an array of objects with the two
/// `names` as fields.
fn object_pairs(value: &Value, names: (&str, &str)) -> Result<Vec<(String, String)>, Failure> {
    let (a, b) = names;
    let wrong = || {
        let (property, example) = if a == "old" {
            ("pairs", "[{\"old\": \"net 30\", \"new\": \"net 60\"}]")
        } else {
            (
                COMMENTS,
                "[{\"anchor\": \"thirty days\", \"text\": \"Why not sixty?\"}]",
            )
        };
        Failure::usage(
            format!("{property} is an array of objects, each with {a} and {b} as strings."),
            format!("Pass {property} as {example}."),
        )
    };
    let Value::Array(items) = value else {
        return Err(wrong());
    };
    items
        .iter()
        .map(|item| {
            let field = |name: &str| item.get(name).and_then(Value::as_str).map(str::to_string);
            match (item.as_object().map(Map::len), field(a), field(b)) {
                (Some(2), Some(old), Some(new)) => Ok((old, new)),
                _ => Err(wrong()),
            }
        })
        .collect()
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
