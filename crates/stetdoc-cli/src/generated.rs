//! The files generated from the binary's own sources, and their drift checks: `skills/stetdoc/SKILL.md` (the stub that
//! `stetdoc init` installs), `docs/reference/cli.md`, and `docs/reference/mcp-tools.md` (written in
//! `commands/mcp/reference.rs`). Each test compares the committed file with what the source gives, and fails with the
//! command that regenerates it. `STETDOC_BLESS=1` writes the files instead, which is what `cargo xtask docs` runs.

use crate::cli::Cli;
use crate::skill;
use clap::CommandFactory;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use stetdoc_core::ExitClass;

/// The repository root, from this crate's manifest directory.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Compares a generated file with the committed one, or writes it under `STETDOC_BLESS=1`.
pub fn check(relative: &str, expected: &str) {
    let path = root().join(relative);
    if std::env::var_os("STETDOC_BLESS").is_some() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(&path, expected).unwrap();
        return;
    }
    let found = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        found == expected,
        "{relative} is stale: it is generated from crates/stetdoc-cli. Run `cargo xtask docs` and commit the result."
    );
}

#[test]
fn the_skill_stub_is_the_one_init_installs() {
    check("skills/stetdoc/SKILL.md", &skill::stub());
}

#[test]
fn every_part_of_the_skill_fills_every_placeholder() {
    for name in skill::NAMES {
        let text = skill::get(name).unwrap_or_default();
        assert!(!text.is_empty(), "{name}");
        assert!(!text.contains("{{"), "{name} has a placeholder left");
    }
    assert!(skill::get("nothing").is_none());
}

#[test]
fn the_worked_batch_of_the_core_skill_parses() {
    let core = skill::get("core").unwrap_or_default();
    let start = core
        .find("<<'EOF'\n")
        .map(|i| i + "<<'EOF'\n".len())
        .unwrap_or_default();
    let end = core[start..]
        .find("\nEOF\n")
        .map(|i| start + i + 1)
        .unwrap_or_default();
    assert!(end > start, "the core skill shows a batch in a heredoc");
    let batch = stetdoc_core::Batch::parse(&core[start..end], &stetdoc_core::Limits::default());
    let batch = batch.unwrap_or_else(|e| panic!("the core skill's batch does not parse: {e}"));
    assert_eq!(batch.ops.len(), 2);
}

#[test]
fn every_operation_example_reads_as_block_text() {
    let ops = skill::get("operations").unwrap_or_default();
    for op in stetdoc_core::capabilities().ops {
        assert!(
            ops.contains(&format!("@{}\n", op.name)),
            "{} has no block-text example",
            op.name
        );
    }
}

/// `docs/reference/cli.md`: every command's usage, description and flags, walked from the `clap` tree, with its examples
/// from `doc/<command>.md`, which the `trycmd` harness runs, then the exit codes and the environment.
fn cli_reference() -> String {
    let mut cmd = Cli::command();
    cmd.build();
    let mut out = String::from(
        "<!-- Generated from the stetdoc binary's own definitions by `cargo xtask docs`: do not edit it by hand. The \
         examples come from crates/stetdoc-cli/doc/. -->\n\n\
         # CLI reference\n\n\
         Each stetdoc command is here with its flags and examples, followed by the exit codes, the JSON output and the \
         environment variables. The descriptions are the `--help` text, and every example runs as a test. The workflow \
         and the rules for agents are in the skill: `stetdoc skills get core`.\n\n\
         Every command takes `--json`, which prints the result, or the error, as JSON on stdout. Data goes to stdout and \
         diagnostics to stderr. Document text never goes in an argument: batches come from stdin or `--ops`.\n\n## Commands\n\n",
    );
    // the index links each command to its section below
    let mut index = skill::commands();
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        index = index.replace(
            &format!("| `stetdoc {name}` |"),
            &format!("| [`stetdoc {name}`](#stetdoc-{name}) |"),
        );
    }
    out.push_str(&index);
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        let _ = write!(out, "\n## `stetdoc {name}`\n\n");
        let about = sub
            .get_long_about()
            .or(sub.get_about())
            .map(|a| a.to_string())
            .unwrap_or_default();
        let _ = writeln!(out, "{about}.\n");
        let usage = sub.clone().render_usage().to_string();
        let usage = usage.trim_start_matches("Usage: ");
        let _ = writeln!(out, "```text\n{usage}\n```\n");
        let args: Vec<_> = sub
            .get_arguments()
            .filter(|a| !matches!(a.get_id().as_str(), "help" | "version" | "json"))
            .collect();
        if !args.is_empty() {
            out.push_str("| Argument | Meaning |\n|---|---|\n");
            for arg in args {
                let _ = writeln!(out, "| {} | {} |", arg_form(arg), arg_help(arg));
            }
        }
        for action in sub.get_subcommands() {
            let help = action
                .get_about()
                .map(|a| a.to_string())
                .unwrap_or_default();
            let usage = action.clone().render_usage().to_string();
            let _ = writeln!(
                out,
                "\n`{}`: {help}.\n",
                usage.trim_start_matches("Usage: ")
            );
            let args: Vec<_> = action
                .get_arguments()
                .filter(|a| !matches!(a.get_id().as_str(), "help" | "version" | "json"))
                .collect();
            if !args.is_empty() {
                out.push_str("| Argument | Meaning |\n|---|---|\n");
                for arg in args {
                    let _ = writeln!(out, "| {} | {} |", arg_form(arg), arg_help(arg));
                }
            }
        }
        let examples = examples(name);
        if !examples.trim().is_empty() {
            let _ = write!(out, "\n### Examples\n\n{}\n", examples.trim_end());
        }
    }
    out.push_str("\n## Exit codes\n\nThe exit status is the class of the error's code. The codes are in the [error reference](errors.md).\n\n| Exit | Meaning | Codes |\n|---|---|---|\n| 0 | success | |\n");
    let caps = stetdoc_core::capabilities();
    for class in ExitClass::ALL {
        let codes: Vec<String> = caps
            .error_codes
            .iter()
            .filter(|c| c.exit == class.number())
            .map(|c| format!("`{}`", c.code))
            .collect();
        let extra = match class {
            ExitClass::Usage => ", and argument errors",
            ExitClass::Validation => ", and `check --strict` with any violation",
            _ => "",
        };
        let _ = writeln!(
            out,
            "| {} | {} | {}{extra} |",
            class.number(),
            class.label(),
            codes.join(", ")
        );
    }
    out.push_str(
        "\n## JSON output\n\n\
         With `--json`, a command prints one JSON value on stdout. Field names are snake_case, fields may be added, and none is \
         removed before 2.0. The core's types are described by `stetdoc capabilities --schema`.\n\n\
         | Command | Prints |\n|---|---|\n\
         | `view`, `inspect`, `lookup`, `check`, `capabilities` | `View`, `Inspection`, `Lookup`, `CheckResult`, `Capabilities` |\n\
         | `edit` on a working copy | `Receipt` |\n\
         | `edit -o`, `compare` | `{ output, receipt, saved }` |\n\
         | `pack` | `{ output, receipt (or null), saved, working_copy }` |\n\
         | `unpack` | `{ working_copy, base, files }` |\n\
         | `skills get` | `{ name, text }`, or `{ parts }` with `--full` |\n\
         | `init` | `{ scope, root, dry_run, agents, written, unchanged, author }`, and `mcp` with `--mcp`; `--list` gives `{ scope, root, agents, author, config }` and `--remove` `{ scope, root, dry_run, removed, agents_md }` |\n\
         | any failure | `{ \"error\": { code, message, next, op_index, file, line } }`, the last three when known |\n",
    );
    out.push_str(
        "\n## Environment\n\n\
         | Variable | Meaning |\n|---|---|\n\
         | `STETDOC_AUTHOR` | The author of revisions when `--author` is absent. Then comes the name saved with `stetdoc init` in `stetdoc/config.toml` (under `XDG_CONFIG_HOME`, else `%APPDATA%` on Windows, else `~/.config`). There is no other default. |\n\
         | `XDG_CONFIG_HOME` | Where the config file is, as above. |\n\
         | `STETDOC_DATE` | The date of revisions when `--date` is absent, `YYYY-MM-DDTHH:MM:SS`. Otherwise the local clock. |\n\
         | `TZ` | The zone of the local clock: an IANA name such as `Asia/Kolkata`, a POSIX rule, or a path to zone data. Otherwise the system's zone. A zone stetdoc cannot read gives UTC, with a warning. |\n\
         | `NO_COLOR` | Respected. stetdoc prints no colour of its own. |\n",
    );
    out
}

/// `-o, --output <DIR>` or `<INPUT>`.
fn arg_form(arg: &clap::Arg) -> String {
    let value = arg
        .get_value_names()
        .and_then(|v| v.first())
        .map(|v| v.to_string())
        .unwrap_or_else(|| arg.get_id().as_str().to_uppercase());
    let takes_value = arg.get_num_args().is_some_and(|n| n.takes_values());
    if arg.is_positional() {
        return format!("`<{value}>`");
    }
    let mut form = String::new();
    if let Some(short) = arg.get_short() {
        let _ = write!(form, "-{short}, ");
    }
    if let Some(long) = arg.get_long() {
        let _ = write!(form, "--{long}");
    }
    if takes_value {
        let _ = write!(form, " <{value}>");
    }
    format!("`{form}`")
}

/// The help of an argument, with its choices and default.
fn arg_help(arg: &clap::Arg) -> String {
    // clap drops the full stop of a one-line doc comment, so the sentence gets it back.
    let mut help = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
    if !help.is_empty() && !help.ends_with('.') {
        help.push('.');
    }
    let choices: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| format!("`{}`", v.get_name()))
        .collect();
    let takes_value = arg.get_num_args().is_some_and(|n| n.takes_values());
    if takes_value && !choices.is_empty() {
        let _ = write!(help, " One of {}.", choices.join(", "));
    }
    let defaults: Vec<String> = arg
        .get_default_values()
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();
    if takes_value && !defaults.is_empty() {
        let _ = write!(help, " Default `{}`.", defaults.join(","));
    }
    help.replace('|', "\\|")
}

/// The examples of a command, from `doc/<name>.md`.
fn examples(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("doc")
        .join(format!("{name}.md"));
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn the_cli_reference_is_generated_from_the_command_tree() {
    check("docs/reference/cli.md", &cli_reference());
}

#[test]
fn every_command_has_an_example() {
    for sub in Cli::command().get_subcommands() {
        assert!(
            !examples(sub.get_name()).trim().is_empty(),
            "doc/{}.md is missing",
            sub.get_name()
        );
    }
}
