//! `cargo xtask bench`: the speed and memory benchmarks, in a release build.
//!
//! ```text
//! cargo xtask bench [--ci] [--strict-time] [--filter WORD] [--input WORD] [--out DIR]
//!                   [--update-baseline] [--list]
//! ```
//!
//! The benchmarks are one ignored test of `stetdoc-core`, `bench::g6`, which hands the benchmarks
//! that the core's test code registers to the harness of `stetdoc-testkit`. The harness builds the
//! inputs (the contract comes from the curated corpus, so this fetches it first), times each
//! benchmark, measures its peak memory, holds the result to the budgets and to the baseline, and
//! prints a table. A budget, a memory regression or a closed known gap fails the command; a time
//! regression against the baseline is a warning unless `--strict-time`. The arguments reach the
//! harness in `STETDOC_BENCH_ARGS`; `--ci` gives the margins of a shared runner, `--out DIR` also
//! writes the table and the measured rows into a directory (the nightly job uploads it), and
//! `--update-baseline` writes what was measured as the baseline of this platform.

use crate::check::CORPUS_SET_ENV;
use crate::corpus;
use crate::repo::{Repo, Res};
use crate::tools::{Cargo, Output};
use std::process::ExitCode;

/// The ignored test that runs the benchmarks, in `stetdoc-core`.
pub const TEST: &str = "bench::g6";

/// The variable that the harness reads its arguments from.
pub const ARGS_ENV: &str = "STETDOC_BENCH_ARGS";

const USAGE: &str = "usage: cargo xtask bench [--ci] [--strict-time] [--filter WORD] [--input WORD] [--out DIR] [--update-baseline] [--list]";

/// The flags with no value, and the flags with one. The harness reads the same ones.
const SWITCHES: [&str; 4] = ["--ci", "--strict-time", "--update-baseline", "--list"];
const OPTIONS: [&str; 3] = ["--filter", "--input", "--out"];

/// What the words are separated by in `STETDOC_BENCH_ARGS`, so that a directory may have spaces.
pub const SEPARATOR: char = '\u{1f}';

/// Checks the arguments and returns them as the harness reads them. The directory of `--out` is
/// made absolute, because the benchmarks run in the directory of the core crate and the path was
/// written for the one the command was run in.
pub fn parse(args: &[&str]) -> Res<Vec<String>> {
    let mut out = Vec::new();
    let mut words = args.iter().copied();
    while let Some(word) = words.next() {
        if OPTIONS.contains(&word) {
            let value = words
                .next()
                .ok_or_else(|| format!("{word} needs a value"))?;
            out.push(word.to_owned());
            out.push(if word == "--out" {
                std::path::absolute(value)
                    .map_err(|e| format!("cannot make `{value}` an absolute path: {e}"))?
                    .display()
                    .to_string()
            } else {
                value.to_owned()
            });
        } else if SWITCHES.contains(&word) {
            out.push(word.to_owned());
        } else {
            return Err(format!("unknown argument `{word}`"));
        }
    }
    Ok(out)
}

/// Runs `cargo xtask bench`.
pub fn run(args: &[&str]) -> ExitCode {
    let words = match parse(args) {
        Ok(words) => words,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let words = words.join(&SEPARATOR.to_string());
    match Repo::discover().and_then(|repo| execute(&repo, &words, !args.contains(&"--list"))) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!(
                "error: the benchmarks failed: a budget or the memory baseline is broken, a known gap has closed, or a benchmark did not run. The FAIL lines above say which"
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Fetches the corpus the contract comes from, unless only the list is wanted, and runs the
/// benchmarks. `Ok(false)` is a failing run, which has already said why.
fn execute(repo: &Repo, words: &str, needs_corpus: bool) -> Res<bool> {
    let mut cargo = Cargo::new(repo.root(), Output::Stream).env(ARGS_ENV, words);
    if needs_corpus {
        let at = corpus::ensure_curated(repo)?;
        cargo = cargo
            .env(corpus::cache::ENV, &at.root.display().to_string())
            .env(CORPUS_SET_ENV, &at.set);
    }
    Ok(cargo.run_ignored_release("stetdoc-core", TEST).ok)
}

#[cfg(test)]
mod tests;
