//! The `stetdoc` command-line tool.
//!
//! **Owns.** Argument parsing (`cli.rs`), one file per command in `commands/`, `fsio` (the only file I/O in the project),
//! the clock (`clock.rs`) and printing (`output.rs`). It stays thin: each command maps onto calls to `stetdoc-core`, as the
//! table in `docs/design/cli-and-skill.md` lists, and renders and validates nothing itself.
//!
//! **Invariants.**
//! - Data goes to stdout and diagnostics to stderr. `NO_COLOR` is respected: the CLI prints no colour of its own.
//! - Exit statuses come from `ErrorCode::exit_class`. CLI argument errors are exit 2. A failed write, or a working copy that
//!   another process holds, is `io_error`, exit 1. A panic is exit 7, `internal`.
//! - The author comes from `--author`, then `STETDOC_AUTHOR`, then the name saved with `stetdoc init`, otherwise the command
//!   fails with a usage error. The author is never a default the user did not confirm.
//! - Every write is atomic: a temporary file, an fsync and a rename.
//!
//! **Start reading.** `cli.rs` for the commands, then `commands/mod.rs`.
//!
//! The design is in `docs/design/cli-and-skill.md`.

mod cli;
mod clock;
mod commands;
mod config;
mod fsio;
#[cfg(test)]
mod generated;
mod output;
mod skill;
mod stamp;
mod target;

use clap::Parser;
use output::Failure;
use std::process::ExitCode;
use stetdoc_core::{ErrorCode, ExitClass, StetError};

fn main() -> ExitCode {
    // `--json` is read before parsing too, so that an argument error and a crash are JSON when JSON was asked for.
    let json = std::env::args().skip(1).any(|a| a == "--json");
    std::panic::set_hook(Box::new(move |info| {
        let failure = Failure::Core(
            StetError::new(ErrorCode::Internal, format!("stetdoc crashed: {info}")).with_next(
                "This is a bug in stetdoc. Report it with the command you ran; do not retry with another edit.",
            ),
        );
        output::print_failure(&failure, json);
        std::process::exit(i32::from(ExitClass::Internal.number()));
    }));
    let cli = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => return argument_error(&error, json),
    };
    if let cli::Command::Mcp(args) = &cli.command {
        return ExitCode::from(commands::mcp::serve(args));
    }
    let status = match commands::run(&cli.command) {
        Ok(outcome) => output::print_outcome(&outcome, cli.json),
        Err(failure) => output::print_failure(&failure, cli.json),
    };
    ExitCode::from(status)
}

/// What clap reports: help and the version as they are, and an argument error as `invalid_payload` (exit 2), in JSON when
/// `--json` was given.
fn argument_error(error: &clap::Error, json: bool) -> ExitCode {
    let code = error.exit_code();
    if code == 0 || !json {
        let _ = error.print();
        return ExitCode::from(u8::try_from(code).unwrap_or(ExitClass::Usage.number()));
    }
    let failure = Failure::usage(
        output::argument_message(error),
        "Run stetdoc --help, or stetdoc COMMAND --help, for the arguments.",
    );
    ExitCode::from(output::print_failure(&failure, true))
}
