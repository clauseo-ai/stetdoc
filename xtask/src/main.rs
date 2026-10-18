//! Repository automation. Run it as `cargo xtask <command>`.
//!
//! - `check [--quick]`: what the check job of CI runs, or the inner loop for the crates you changed
//! - `docs [--check]`: write the generated docs, or only check them
//! - `codegen [--check]`: write the generated schema tables and the library's TypeScript types, or only check the tables
//!   (the tests check the types)
//! - `bench`: time the engine and measure its peak memory against the speed and memory budgets
//! - `corpus fetch`: download, verify and cache the test corpora
//! - `fuzz <target>`: run a fuzz target for a while, seeded from the corpus and the fixtures
//! - `oracle libreoffice`: LibreOffice reads a seeded sample of stetdoc's outputs (part of the Word check)
//! - `release check`: the npm packages and the release workflow agree with the workspace (the release is a workflow)

mod arch;
mod bench;
mod changed;
mod check;
mod codegen;
mod contracts;
mod corpus;
mod enumspec;
mod errors;
mod exceptions;
mod findingspec;
mod fuzz;
mod layering;
mod links;
mod opspec;
mod oracle;
mod release;
mod repo;
#[cfg(test)]
mod scaffold;
mod sizes;
mod tools;
mod violation;

use std::io::Write;
use std::process::ExitCode;

const HELP: &str = "cargo xtask <command>

commands:
  check            run what the check job of CI runs (CI also runs `fuzz <target> --time 60`)
  check --quick    format, clippy and tests for the crates changed relative to main
  docs             write the generated docs under docs/, the CLI reference and the skill stub
  docs --check     fail if a generated doc is out of date
  codegen          write the generated schema tables (crates/stetdoc-core/src/schema/generated)
                   and the library's TypeScript types (crates/stetdoc-napi/npm/types.d.ts)
  codegen --check  fail if the generated schema tables are stale
  corpus fetch     fetch, verify and cache the curated test corpus (--full for all corpora)
  corpus check     run the corpus manifest check alone
  corpus hash      print the manifest hash that names the sets in the cache
  corpus path      print the corpus cache directory
  fuzz <target>    fuzz `package` or `xml` for --time seconds (60), with the pinned nightly and cargo-fuzz
  bench            time the engine and measure its peak memory against the speed and memory budgets
                   (--ci for a shared runner, --filter WORD, --input WORD, --update-baseline, --list)
  oracle libreoffice  LibreOffice loads, converts, accepts and rejects a seeded sample of stetdoc's outputs
                   and its text is held to stetdoc's views (part of the Word check; --sample N, --seed S); needs LibreOffice
  release check    check that the npm packages and .github/workflows/release.yml agree with the workspace
                   (releases are built by that workflow on a v* tag, into a draft GitHub release)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match args.as_slice() {
        ["check"] => run_check(false),
        ["check", "--quick"] => run_check(true),
        ["docs"] => run_docs(false),
        ["docs", "--check"] => run_docs(true),
        ["codegen"] => run_codegen(false),
        ["codegen", "--check"] => run_codegen(true),
        ["corpus", rest @ ..] => corpus::run(rest),
        ["fuzz", rest @ ..] => fuzz::run(rest),
        ["bench", rest @ ..] => bench::run(rest),
        ["oracle", rest @ ..] => oracle::run(rest),
        ["release", rest @ ..] => release::run(rest),
        ["help" | "--help" | "-h"] => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {}\n\n{HELP}", other.join(" "));
            ExitCode::from(2)
        }
    }
}

fn run_check(quick: bool) -> ExitCode {
    let repo = match repo::Repo::discover() {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    let steps = if quick {
        check::quick_steps(&repo)
    } else {
        Ok(check::full_steps(&repo))
    };
    match steps {
        Ok(steps) => {
            let mut stdout = std::io::stdout();
            let ok = check::execute(&steps, &mut stdout);
            let _ = stdout.flush();
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => fail(&e),
    }
}

fn run_codegen(check_only: bool) -> ExitCode {
    let repo = match repo::Repo::discover() {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    if check_only {
        return match codegen::check(&repo) {
            Ok(found) if found.is_empty() => {
                println!("generated schema tables are up to date");
                ExitCode::SUCCESS
            }
            Ok(found) => {
                for v in found {
                    eprintln!("{v}");
                }
                ExitCode::FAILURE
            }
            Err(e) => fail(&e),
        };
    }
    match codegen::run(&repo) {
        Ok(summary) => println!("{summary}"),
        Err(e) => return fail(&e),
    }
    write_typescript_types(&repo)
}

/// The library's TypeScript types are generated from the core's `json_schema()`, which this binary is built without, so a test
/// of `stetdoc-napi` writes them under `STETDOC_BLESS=1`. Without it, that test is their drift check, and `cargo xtask check`
/// runs it.
fn write_typescript_types(repo: &repo::Repo) -> ExitCode {
    let status =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args([
                "test",
                "--locked",
                "-q",
                "-p",
                "stetdoc-napi",
                "--test",
                "types",
            ])
            .env("STETDOC_BLESS", "1")
            .current_dir(repo.root())
            .status();
    match status {
        Ok(s) if s.success() => {
            println!("wrote crates/stetdoc-napi/npm/types.d.ts");
            ExitCode::SUCCESS
        }
        Ok(_) => {
            fail("the stetdoc-napi test that writes crates/stetdoc-napi/npm/types.d.ts failed")
        }
        Err(e) => fail(&format!(
            "could not run cargo to write the TypeScript types: {e}"
        )),
    }
}

fn run_docs(check_only: bool) -> ExitCode {
    let repo = match repo::Repo::discover() {
        Ok(repo) => repo,
        Err(e) => return fail(&e),
    };
    if check_only {
        return match errors::check_generated(&repo) {
            Ok(found) if found.is_empty() => {
                println!("generated docs are up to date");
                ExitCode::SUCCESS
            }
            Ok(found) => {
                for v in found {
                    eprintln!("{v}");
                }
                ExitCode::FAILURE
            }
            Err(e) => fail(&e),
        };
    }
    match errors::write_generated(&repo) {
        Ok(written) => {
            for path in written {
                println!("wrote {path}");
            }
        }
        Err(e) => return fail(&e),
    }
    write_cli_docs(&repo)
}

/// The CLI reference and the skill stub are generated from the `stetdoc` binary's own definitions, which this binary is built
/// without, so a test of `stetdoc` writes them under `STETDOC_BLESS=1`. Without it, that test is their drift check, and
/// `cargo xtask check` runs it.
fn write_cli_docs(repo: &repo::Repo) -> ExitCode {
    let status =
        std::process::Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
            .args([
                "test",
                "--locked",
                "-q",
                "-p",
                "stetdoc",
                "--bin",
                "stetdoc",
                "generated",
            ])
            .env("STETDOC_BLESS", "1")
            .current_dir(repo.root())
            .status();
    match status {
        Ok(s) if s.success() => {
            println!("wrote docs/reference/cli.md");
            println!("wrote skills/stetdoc/SKILL.md");
            ExitCode::SUCCESS
        }
        Ok(_) => fail(
            "the stetdoc test that writes docs/reference/cli.md and skills/stetdoc/SKILL.md failed",
        ),
        Err(e) => fail(&format!("could not run cargo to write the CLI docs: {e}")),
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::FAILURE
}
