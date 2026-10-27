//! One file per command. Each maps its arguments onto calls of the core, as the table in `docs/design/cli-and-skill.md`
//! says, and returns what to print.

mod capabilities;
mod check;
mod compare;
mod edit;
mod init;
mod inspect;
mod lookup;
pub mod mcp;
mod pack;
mod skills;
mod unpack;
mod view;
mod written;

use crate::cli::Command;
use crate::output::{Failure, Outcome};

/// Runs a command.
pub fn run(command: &Command) -> Result<Outcome, Failure> {
    match command {
        Command::Unpack(args) => unpack::run(args),
        Command::Pack(args) => pack::run(args),
        Command::View(args) => view::run(args),
        Command::Inspect(args) => inspect::run(args),
        Command::Edit(args) => edit::run(args),
        Command::Compare(args) => compare::run(args),
        Command::Check(args) => check::run(args),
        Command::Lookup(args) => lookup::run(args),
        Command::Skills(args) => skills::run(args),
        Command::Init(args) => init::run(args),
        Command::Capabilities(args) => capabilities::run(args),
        Command::Mcp(_) => Err(Failure::usage(
            "stetdoc mcp is a server, not a command that prints a result.",
            "Run stetdoc mcp from an agent's MCP configuration.",
        )),
    }
}
