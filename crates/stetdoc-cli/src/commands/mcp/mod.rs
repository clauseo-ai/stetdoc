//! `stetdoc mcp`: the commands as tools over the Model Context Protocol, on stdin and stdout.
//!
//! JSON-RPC 2.0, one message per line, handled one at a time with `serde_json`. The server speaks the current revision,
//! 2026-07-28, in which every request carries its version in `_meta` and `server/discover` describes the server, and the
//! earlier revisions that open with an `initialize` handshake, so clients of either era work. Each tool call runs the
//! command's own function in this process and answers with the JSON the CLI prints with `--json`; a failure is a result
//! with `isError` and the failure's message and next step. Stdout carries protocol messages only: logs go to stderr.

mod confine;
mod describe;
mod ops;
mod tools;

use crate::cli::{Cli, Command, McpArgs};
use crate::output::{self, Failure};
use clap::Parser;
use confine::Root;
use serde_json::{Map, Value, json};
use std::io::{BufRead, Read, Write};
use stetdoc_core::{ErrorCode, StetError};
use tools::Tool;

/// The revision this server speaks first.
const LATEST: &str = "2026-07-28";
/// Every revision it speaks. The ones after the first open with `initialize`.
/// 2025-03-26 is left out: it allowed JSON-RPC batches, which this server refuses.
const SUPPORTED: [&str; 4] = [LATEST, "2025-11-25", "2025-06-18", "2024-11-05"];
/// The `_meta` key of a request's revision.
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";
/// The largest message read, in bytes. A batch is far smaller.
const MAX_MESSAGE: u64 = 64 << 20;
/// How long a client may cache the tool list: it changes only with the binary.
const TTL_MS: u64 = 3_600_000;

const INSTRUCTIONS: &str = "stetdoc reads and edits Word documents (.docx) and writes every edit as a real tracked change. \
Before you work on a .docx, call skills_get and follow what it returns. The tools take the arguments of the stetdoc \
commands; the edit tool takes typed operations in `ops` or a text batch in `batch`, and the replace and comment tools \
change or comment on text found once in the document in one call. Never read or edit a .docx any other way (unzip, the XML, python-docx), \
and if these tools stop working, tell the user rather than work around them.";

/// Runs the server until stdin closes, and returns the exit status.
pub fn serve(args: &McpArgs) -> u8 {
    let root = match args.root.as_deref().map(Root::new).transpose() {
        Ok(root) => root,
        Err(failure) => return output::print_failure(&failure, false),
    };
    // A panic in a command becomes that call's error, and the server goes on.
    std::panic::set_hook(Box::new(|info| eprintln!("stetdoc mcp: {info}")));
    let server = Server {
        root,
        author: args.author.clone(),
        tools: tools::tools(),
    };
    eprintln!(
        "stetdoc mcp {}: {} tools on stdin and stdout{}",
        env!("CARGO_PKG_VERSION"),
        server.tools.len(),
        server
            .root
            .as_ref()
            .map(|r| format!(", paths confined to {}", r.dir().display()))
            .unwrap_or_default()
    );
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut out = std::io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let read = Read::by_ref(&mut input)
            .take(MAX_MESSAGE + 1)
            .read_until(b'\n', &mut line);
        let reply = match read {
            Ok(0) => break,
            Ok(_) if line.len() as u64 > MAX_MESSAGE => {
                // Skip the rest of the line without holding it, then say why.
                if !line.ends_with(b"\n") {
                    skip_line(&mut input);
                }
                Some(error(
                    Value::Null,
                    -32600,
                    "The message is larger than 64 MiB.",
                    None,
                ))
            }
            Ok(_) => server.handle(&line),
            Err(e) => {
                eprintln!("stetdoc mcp: could not read stdin: {e}");
                return 1;
            }
        };
        if let Some(reply) = reply {
            let text = serde_json::to_string(&reply).unwrap_or_default();
            if writeln!(out, "{text}").and_then(|()| out.flush()).is_err() {
                // The client went away.
                break;
            }
        }
    }
    0
}

struct Server {
    root: Option<Root>,
    author: Option<String>,
    tools: Vec<Tool>,
}

/// A JSON-RPC error: the code, the message, and the data.
type RpcError = (i64, String, Option<Value>);

impl Server {
    /// The reply to one message, or `None` for a notification or a response.
    fn handle(&self, line: &[u8]) -> Option<Value> {
        if line.iter().all(u8::is_ascii_whitespace) {
            return None;
        }
        let message: Value = match serde_json::from_slice(line) {
            Ok(message) => message,
            Err(e) => {
                return Some(error(
                    Value::Null,
                    -32700,
                    &format!("Parse error: {e}"),
                    None,
                ));
            }
        };
        let Some(object) = message.as_object() else {
            return Some(error(
                Value::Null,
                -32600,
                "A message is one JSON object.",
                None,
            ));
        };
        let id = object.get("id").cloned();
        let Some(method) = object.get("method").and_then(Value::as_str) else {
            // A response to a request this server never sends, or nothing at all.
            let is_response = object.contains_key("result") || object.contains_key("error");
            return (!is_response).then(|| {
                error(
                    id.unwrap_or(Value::Null),
                    -32600,
                    "A request needs a method.",
                    None,
                )
            });
        };
        let Some(id) = id else {
            // A notification: notifications/initialized, notifications/cancelled and the rest need no answer, and calls
            // run one at a time, so there is nothing in flight to cancel.
            return None;
        };
        if !(id.is_string() || id.is_i64() || id.is_u64()) {
            return Some(error(
                Value::Null,
                -32600,
                "A request's id is a string or an integer.",
                None,
            ));
        }
        let params = object.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = self.dispatch(method, &params);
        Some(match result {
            Ok(result) => reply(id, result),
            Err((code, message, data)) => error(id, code, &message, data),
        })
    }

    fn dispatch(&self, method: &str, params: &Value) -> Result<Value, RpcError> {
        let requested = params
            .get("_meta")
            .and_then(|m| m.get(VERSION_KEY))
            .and_then(Value::as_str);
        if let Some(version) = requested
            && method != "initialize"
            && !SUPPORTED.contains(&version)
        {
            return Err((
                -32022,
                "Unsupported protocol version".into(),
                Some(json!({ "supported": SUPPORTED, "requested": version })),
            ));
        }
        match method {
            "initialize" => Ok(initialize(params)),
            "server/discover" => Ok(json!({
                "supportedVersions": SUPPORTED,
                "capabilities": { "tools": {} },
                "instructions": INSTRUCTIONS,
                "ttlMs": TTL_MS,
                "cacheScope": "public",
            })),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self.tools.iter().map(|t| t.listing.clone()).collect::<Vec<_>>(),
                "ttlMs": TTL_MS,
                "cacheScope": "public",
            })),
            "tools/call" => self.call(params),
            _ => Err((-32601, format!("Method not found: {method}"), None)),
        }
    }

    fn call(&self, params: &Value) -> Result<Value, RpcError> {
        let name = params.get("name").and_then(Value::as_str).ok_or((
            -32602,
            "tools/call needs the tool's name.".to_string(),
            None,
        ))?;
        let tool = self
            .tools
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| (-32602, format!("Unknown tool: {name}"), None))?;
        let empty = Map::new();
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => &empty,
            Some(Value::Object(arguments)) => arguments,
            Some(_) => return Err((-32602, "A tool's arguments are a JSON object.".into(), None)),
        };
        let ran =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run(tool, arguments)));
        let ran = ran.unwrap_or_else(|_| {
            Err(Failure::Core(
                StetError::new(ErrorCode::Internal, format!("stetdoc crashed running {name}."))
                    .with_next("This is a bug in stetdoc. Report it with the call you made; do not retry with another edit."),
            ))
        });
        Ok(match ran {
            Ok(outcome) => {
                for note in &outcome.notes {
                    eprintln!("{note}");
                }
                let mut content = vec![json!({ "type": "text", "text": pretty(&outcome.json) })];
                if !outcome.notes.is_empty() {
                    content.push(json!({ "type": "text", "text": outcome.notes.join("\n") }));
                }
                let mut result = json!({ "content": content, "isError": false });
                if outcome.json.is_object()
                    && let Some(object) = result.as_object_mut()
                {
                    object.insert("structuredContent".into(), outcome.json);
                }
                result
            }
            Err(failure) => json!({
                "content": [{ "type": "text", "text": failure.to_text() }],
                "structuredContent": failure.to_json(),
                "isError": true,
            }),
        })
    }

    /// Runs a tool's command, as the CLI would parse it, in this process.
    fn run(&self, tool: &Tool, arguments: &Map<String, Value>) -> Result<output::Outcome, Failure> {
        let (argv, batch) = tools::argv(tool, arguments, self.root.as_ref())?;
        let cli = Cli::try_parse_from(argv).map_err(|e| {
            Failure::usage(
                output::argument_message(&e),
                output::argument_next(&e, Some(&tool.name)).unwrap_or_else(|| {
                    format!(
                        "Pass the arguments that the {} tool's input schema lists.",
                        tool.name
                    )
                }),
            )
        })?;
        let mut command = cli.command;
        let stamp = match &mut command {
            Command::Edit(args) => Some(&mut args.stamp),
            Command::Pack(args) => Some(&mut args.stamp),
            Command::Check(args) => Some(&mut args.stamp),
            Command::Compare(args) => Some(&mut args.stamp),
            Command::Replace(args) => Some(&mut args.find.stamp),
            Command::Comment(args) => Some(&mut args.find.stamp),
            _ => None,
        };
        if let Some(stamp) = stamp
            && stamp.author.is_none()
        {
            stamp.author.clone_from(&self.author);
        }
        match &mut command {
            Command::Edit(args) => match batch {
                Some(batch) => args.batch = Some(batch),
                // Stdin carries the protocol, so a batch never comes from it.
                None if args
                    .ops
                    .as_deref()
                    .is_none_or(|p| p == std::path::Path::new("-")) =>
                {
                    return Err(Failure::usage(
                        "No operations: the edit tool takes them in `ops`, as text in `batch`, or from a file in `ops_file`.",
                        "Pass the operations in `ops`, such as [{\"op\": \"replace\", \"ref\": \"p4\", \"find\": \"thirty\", \"with\": \"sixty\"}].",
                    ));
                }
                None => {}
            },
            Command::Unpack(args) if args.output.is_none() && !args.refresh => {
                if let Some(root) = &self.root {
                    args.output = Some(root.check(&super::unpack::default_dir(&args.input))?);
                }
            }
            Command::Pack(args) if args.output.is_none() => {
                if let Some(root) = &self.root {
                    args.output = Some(root.check(&super::pack::default_output(&args.dir))?);
                }
            }
            _ => {}
        }
        // An edit of a .docx with no output and no in_place writes NAME.edited.docx beside it, which must be in the root too.
        if let Some(root) = &self.root {
            let wants = match &command {
                Command::Edit(a) => {
                    Some((&a.input, a.output.is_none() && !a.in_place && !a.dry_run))
                }
                Command::Replace(a) => Some((
                    &a.input,
                    a.find.output.is_none() && !a.find.in_place && !a.find.dry_run,
                )),
                Command::Comment(a) => Some((
                    &a.input,
                    a.find.output.is_none() && !a.find.in_place && !a.find.dry_run,
                )),
                _ => None,
            };
            if let Some((input, true)) = wants
                && input.is_file()
            {
                root.check(&super::apply::default_output(input))?;
            }
        }
        super::run(&command)
    }
}

/// The result of `initialize`, for a client of a revision before 2026-07-28: its own revision when this server speaks it.
fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .filter(|v| *v != LATEST && SUPPORTED.contains(v))
        .unwrap_or("2025-11-25");
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": server_info(),
        "instructions": INSTRUCTIONS,
    })
}

fn server_info() -> Value {
    json!({ "name": "stetdoc", "title": "stetdoc", "version": env!("CARGO_PKG_VERSION") })
}

/// A result, with the fields every result of the current revision carries.
fn reply(id: Value, mut result: Value) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("resultType".into(), json!("complete"));
        object.insert(
            "_meta".into(),
            json!({ "io.modelcontextprotocol/serverInfo": server_info() }),
        );
    }
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut error = json!({ "code": code, "message": message });
    if let (Some(data), Some(object)) = (data, error.as_object_mut()) {
        object.insert("data".into(), data);
    }
    json!({ "jsonrpc": "2.0", "id": id, "error": error })
}

fn pretty(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
}

/// Discards input up to and including the next newline, a buffer at a time.
fn skip_line(input: &mut impl BufRead) {
    loop {
        let (done, used) = match input.fill_buf() {
            Ok([]) | Err(_) => return,
            Ok(buffer) => match buffer.iter().position(|b| *b == b'\n') {
                Some(at) => (true, at + 1),
                None => (false, buffer.len()),
            },
        };
        input.consume(used);
        if done {
            return;
        }
    }
}
