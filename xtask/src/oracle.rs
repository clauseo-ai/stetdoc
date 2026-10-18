//! `cargo xtask oracle libreoffice`: LibreOffice as an independent reader of stetdoc's outputs, for the Word check.
//!
//! ```text
//! cargo xtask oracle libreoffice [--sample N] [--seed S]
//! ```
//!
//! It fetches the curated corpus (and its compare pairs) and runs the ignored test
//! `libreoffice_reads_a_seeded_sample_of_stetdoc_outputs` of `stetdoc-testkit` in a release build. The test makes a seeded
//! sample of N outputs (500), has a headless LibreOffice load each one, convert it to PDF, and accept and reject all its
//! tracked changes, compares the texts with stetdoc's Final and Original views, triages every mismatch, and writes
//! `target/oracle/libreoffice/report.md`. It needs LibreOffice (`soffice`, or `STETDOC_SOFFICE`); without it the test says so
//! and passes, so this command fails instead when no `soffice` answers.

use crate::check::CORPUS_SET_ENV;
use crate::corpus;
use crate::repo::{Repo, Res};
use crate::tools::{Cargo, Output};
use std::process::{Command, ExitCode, Stdio};

/// The ignored test that runs the oracle.
pub const TEST: &str = "libreoffice_reads_a_seeded_sample_of_stetdoc_outputs";

const USAGE: &str = "usage: cargo xtask oracle libreoffice [--sample N] [--seed S]";

/// The environment the test reads: the sample size and the seed.
pub fn parse(args: &[&str]) -> Res<Vec<(&'static str, String)>> {
    let mut env = Vec::new();
    let mut words = args.iter().copied();
    match words.next() {
        Some("libreoffice") => {}
        Some(other) => return Err(format!("unknown oracle `{other}`")),
        None => return Err("name the oracle: libreoffice".to_string()),
    }
    while let Some(word) = words.next() {
        let var = match word {
            "--sample" => "STETDOC_LO_SAMPLE",
            "--seed" => "STETDOC_SEED",
            _ => return Err(format!("unknown argument `{word}`")),
        };
        let value = words
            .next()
            .ok_or_else(|| format!("{word} needs a value"))?;
        value
            .parse::<u64>()
            .map_err(|_| format!("{word} takes a number, not `{value}`"))?;
        env.push((var, value.to_owned()));
    }
    Ok(env)
}

fn soffice_answers() -> bool {
    let mut candidates = vec![std::env::var("STETDOC_SOFFICE").unwrap_or_default()];
    candidates.extend(
        [
            "soffice",
            "/Applications/LibreOffice.app/Contents/MacOS/soffice",
            "/usr/bin/soffice",
            "/usr/lib/libreoffice/program/soffice",
        ]
        .map(String::from),
    );
    candidates.iter().filter(|c| !c.is_empty()).any(|c| {
        Command::new(c)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// Runs `cargo xtask oracle`.
pub fn run(args: &[&str]) -> ExitCode {
    let env = match parse(args) {
        Ok(env) => env,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    if !soffice_answers() {
        eprintln!(
            "error: LibreOffice is not installed: install it (macOS: `brew install --cask libreoffice`), or set STETDOC_SOFFICE to its soffice"
        );
        return ExitCode::FAILURE;
    }
    match Repo::discover().and_then(|repo| execute(&repo, &env)) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!(
                "error: the LibreOffice oracle failed: an output did not load or convert, or a mismatch is not triaged. target/oracle/libreoffice/report.md says which"
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn execute(repo: &Repo, env: &[(&'static str, String)]) -> Res<bool> {
    let at = corpus::ensure_curated(repo)?;
    let mut cargo = Cargo::new(repo.root(), Output::Stream)
        .env(corpus::cache::ENV, &at.root.display().to_string())
        .env(CORPUS_SET_ENV, &at.set);
    for (k, v) in env {
        cargo = cargo.env(k, v);
    }
    Ok(cargo
        .run_ignored_test_file_release("stetdoc-testkit", "oracle_libreoffice", TEST)
        .ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_arguments_become_the_environment_of_the_test() {
        assert_eq!(parse(&["libreoffice"]).unwrap(), []);
        assert_eq!(
            parse(&["libreoffice", "--sample", "50", "--seed", "7"]).unwrap(),
            [
                ("STETDOC_LO_SAMPLE", "50".to_string()),
                ("STETDOC_SEED", "7".to_string())
            ]
        );
        assert!(parse(&[]).unwrap_err().contains("libreoffice"));
        assert!(parse(&["word"]).unwrap_err().contains("unknown oracle"));
        assert!(
            parse(&["libreoffice", "--sample"])
                .unwrap_err()
                .contains("needs a value")
        );
        assert!(
            parse(&["libreoffice", "--sample", "x"])
                .unwrap_err()
                .contains("number")
        );
        assert!(
            parse(&["libreoffice", "--fast"])
                .unwrap_err()
                .contains("unknown argument")
        );
    }
}
