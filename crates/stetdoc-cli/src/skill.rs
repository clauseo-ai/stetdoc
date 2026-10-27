//! The agent skill embedded in the binary: the hand-written text of `skill/*.md`, with its tables filled from the `clap`
//! command tree and from `capabilities()`, so the skill, `--help` and the CLI reference cannot drift apart.

use crate::cli::Cli;
use clap::CommandFactory;
use std::fmt::Write;
use stetdoc_core::{Batch, Capabilities, Limits, OpInfo};

/// The skill's parts, in the order `--full` prints them.
pub const NAMES: [&str; 3] = ["core", "operations", "errors"];

/// The stub's source, with a `{{version}}` placeholder.
const STUB: &str = include_str!("../skill/stub.md");

/// The stub that `stetdoc init` installs and `skills/stetdoc/SKILL.md` holds: it tells an agent to run `stetdoc skills get core`.
/// Its front matter carries the version that wrote it, which `stetdoc init --list` reads back.
pub fn stub() -> String {
    STUB.replace("{{version}}", env!("CARGO_PKG_VERSION"))
}

/// The stubs earlier builds wrote, before the stub carried its version. Each is matched byte for byte.
const EARLIER: [&str; 1] = [include_str!("../skill/earlier/stub-1.md")];

/// The stubs earlier builds wrote with a version line, each with the `{{version}}` placeholder on it. A file is matched
/// against them with its version line normalised, as against the current stub.
const EARLIER_VERSIONED: [&str; 2] = [
    include_str!("../skill/earlier/stub-2.md"),
    include_str!("../skill/earlier/stub-3.md"),
];

/// The key of the front-matter line that carries the version.
const VERSION_KEY: &str = "stetdoc-version:";

/// Whether a skill file is a stub some stetdoc build wrote, unchanged: this stub or an earlier versioned one with any
/// version on its version line, or an earlier unversioned stub byte for byte. A file that only says `name: stetdoc`, such as a user's own skill or an edited stub, is
/// not, so init never replaces or removes it without being told to.
pub fn is_ours(text: &str) -> bool {
    let mut normal = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if line.trim_start().starts_with(VERSION_KEY) && line.ends_with("\"\n") {
            normal.push_str(&line[..indent]);
            normal.push_str(VERSION_KEY);
            normal.push_str(" \"{{version}}\"\n");
        } else {
            normal.push_str(line);
        }
    }
    normal == STUB || EARLIER_VERSIONED.contains(&normal.as_str()) || EARLIER.contains(&text)
}

/// Whether a skill file's front matter says `name: stetdoc`, as every stub does and a user's own skill may.
pub fn names_stetdoc(text: &str) -> bool {
    front_matter(text).any(|line| line.trim_end() == "name: stetdoc")
}

/// The version a stub says wrote it, when it says one. Earlier stubs did not.
pub fn stub_version(text: &str) -> Option<String> {
    front_matter(text).find_map(|line| {
        let value = line.trim().strip_prefix("stetdoc-version:")?;
        Some(value.trim().trim_matches('"').to_string())
    })
}

/// The lines of a file's YAML front matter, or none when it has none.
fn front_matter(text: &str) -> impl Iterator<Item = &str> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .unwrap_or("");
    rest.lines().take_while(|line| line.trim_end() != "---")
}

const CORE: &str = include_str!("../skill/core.md");
const OPERATIONS: &str = include_str!("../skill/operations.md");
const ERRORS: &str = include_str!("../skill/errors.md");

/// A base token for the examples. A real batch copies its base from a view.
const EXAMPLE_BASE: &str = "5737b772923cf8d3:0:5737b772923cf8d3";

/// One part of the skill, by name, or `None` for a name that is not one.
pub fn get(name: &str) -> Option<String> {
    let caps = stetdoc_core::capabilities();
    let source = match name {
        "core" => CORE,
        "operations" => OPERATIONS,
        "errors" => ERRORS,
        _ => return None,
    };
    Some(fill(source, &caps))
}

/// A table of the skill, made from the capabilities.
type Table = fn(&Capabilities) -> String;

fn fill(source: &str, caps: &Capabilities) -> String {
    let mut out = source.replace("{{version}}", &caps.version);
    let tables: [(&str, Table); 6] = [
        ("{{commands}}", |_| commands()),
        ("{{ops_summary}}", ops_summary),
        ("{{ops_reference}}", ops_reference),
        ("{{errors_table}}", errors_table),
        ("{{errors_reference}}", errors_reference),
        ("{{limits}}", |c| limits(&c.limits)),
    ];
    for (placeholder, make) in tables {
        if out.contains(placeholder) {
            out = out.replace(placeholder, make(caps).trim_end());
        }
    }
    out
}

/// Each command with the first line of its help, from the `clap` tree.
pub fn commands() -> String {
    let mut out = String::from("| Command | Does |\n|---|---|\n");
    for sub in Cli::command().get_subcommands() {
        let about = sub.get_about().map(|a| a.to_string()).unwrap_or_default();
        let _ = writeln!(out, "| `stetdoc {}` | {} |", sub.get_name(), about);
    }
    out
}

fn ops_summary(caps: &Capabilities) -> String {
    let mut out = String::from("| Operation | Tracked | Does |\n|---|---|---|\n");
    for op in &caps.ops {
        let tracked = if op.trackable { "yes" } else { "no" };
        let _ = writeln!(out, "| `{}` | {tracked} | {} |", op.name, op.summary);
    }
    out
}

fn ops_reference(caps: &Capabilities) -> String {
    let mut out = String::new();
    for op in &caps.ops {
        let _ = writeln!(out, "## `{}`\n\n{}\n", op.name, op.summary);
        let tracked = if op.trackable {
            "Written as a tracked change."
        } else {
            "Never tracked."
        };
        let _ = writeln!(out, "{tracked}\n");
        if !op.one_of.is_empty() {
            let _ = writeln!(out, "Give exactly one of: {}.\n", op.one_of.join(", "));
        }
        out.push_str("| Field | Kind | Required | Meaning |\n|---|---|---|---|\n");
        for field in &op.fields {
            let required = if field.required { "yes" } else { "" };
            let mut doc = field.doc.replace('|', "\\|").replace('\n', " ");
            if !field.choices.is_empty() {
                doc.push_str(&format!(" One of: {}.", field.choices.join(", ")));
            }
            let _ = writeln!(
                out,
                "| `{}` | {} | {required} | {doc} |",
                field.name, field.kind
            );
        }
        let _ = writeln!(out, "\n```\n{}```\n", example(op));
    }
    out
}

/// The operation's example as block text, which is what an agent writes. The JSON form is the fallback for an example that
/// block text cannot say.
fn example(op: &OpInfo) -> String {
    let json = format!(
        "{{\"base\": \"{EXAMPLE_BASE}\", \"ops\": [{}]}}",
        op.example
    );
    let block = Batch::parse(&json, &Limits::default())
        .ok()
        .and_then(|batch| batch.to_block().ok());
    // The header of the reference explains the base line, so each example shows only its operation.
    let prefix = format!("base: {EXAMPLE_BASE}\n---\n");
    match block {
        Some(text) => text
            .strip_prefix(&prefix)
            .map_or(text.clone(), str::to_string),
        None => format!("{}\n", op.example),
    }
}

fn errors_table(caps: &Capabilities) -> String {
    let mut out = String::from("| Exit | Code | What to do |\n|---|---|---|\n");
    let mut codes: Vec<_> = caps.error_codes.iter().collect();
    codes.sort_by_key(|c| c.exit);
    for code in codes {
        let _ = writeln!(out, "| {} | `{}` | {} |", code.exit, code.code, code.next);
    }
    out
}

fn errors_reference(caps: &Capabilities) -> String {
    let mut out = String::new();
    let mut codes: Vec<_> = caps.error_codes.iter().collect();
    codes.sort_by_key(|c| c.exit);
    for code in codes {
        let _ = writeln!(
            out,
            "## `{}` (exit {})\n\n{}\n\nNext: {}\n",
            code.code, code.exit, code.description, code.next
        );
    }
    out
}

fn limits(limits: &Limits) -> String {
    format!(
        "- at most {} operations in a batch, and {} bytes of batch text\n- at most {} bytes in one text field\n- at most {} refs and ranges in a scope\n",
        limits.max_batch_ops,
        limits.max_batch_bytes,
        limits.max_op_text_bytes,
        limits.max_scope_refs
    )
}
