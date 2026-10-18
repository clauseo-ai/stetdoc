//! `cargo xtask fuzz`: runs one fuzz target of `fuzz/` under the pinned nightly toolchain.
//!
//! ```text
//! cargo xtask fuzz <target> [--time SECONDS] [--repro FILE] [-- <libFuzzer arguments>]
//! ```
//!
//! Fuzzing needs a nightly compiler and cargo-fuzz, which the rest of the workspace does not, so
//! the command checks for both and says how to install what is missing. It never skips. It then
//! fetches the curated corpus, writes the seeds (the fixtures, and the small documents and XML
//! parts of the corpus) into `fuzz/corpus/<target>`, and runs libFuzzer for the time asked, which
//! is 60 seconds by default. With `--repro` it runs one input instead, as a failure is reproduced.
//! A failure leaves its input under `fuzz/artifacts/<target>`, and the command names it.

use crate::check::CORPUS_SET_ENV;
use crate::corpus;
use crate::repo::{Repo, Res};
use crate::tools::{Cargo, Output};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::SystemTime;

/// One fuzz target: the file in `fuzz/fuzz_targets` that holds it, and what to give libFuzzer.
#[derive(Debug, PartialEq, Eq)]
pub struct Target {
    /// The name, which is also the name of the binary and of the dictionary.
    pub name: &'static str,
    /// The longest input libFuzzer makes, in bytes. The four bytes of choices at the start of
    /// every input are counted in it.
    pub max_len: u32,
    /// What the target covers, in a phrase.
    pub what: &'static str,
}

/// Every target. Each is a file in `fuzz/fuzz_targets` and a `[[bin]]` in `fuzz/Cargo.toml`, and a
/// test checks both.
pub const TARGETS: &[Target] = &[
    Target {
        name: "package",
        max_len: 262_144,
        what: "the ZIP container: open, read every part, write after replacing one part",
    },
    Target {
        name: "xml",
        max_len: 131_072,
        what: "the span tree: parse, serialize clean and with every tag dirty, make one edit",
    },
    Target {
        name: "batch",
        max_len: 65_536,
        what: "a batch through Document: parse JSON or block text, round-trip it as JSON, apply it to a small document",
    },
    Target {
        name: "files",
        max_len: 65_536,
        what: "the files path through Document: put the input into document.md and apply the readable files as an edit",
    },
];

/// The nightly toolchain fuzzing builds with, which no other command uses. Keep it equal to
/// `fuzz/rust-toolchain.toml` and `ci.yml`; a test checks.
pub const TOOLCHAIN: &str = "nightly-2026-09-28";

/// The cargo-fuzz version CI installs. Keep it equal to `ci.yml`; a test checks.
pub const CARGO_FUZZ_VERSION: &str = "0.13.2";

/// How long a run lasts when `--time` is not given.
pub const DEFAULT_SECONDS: u64 = 60;

/// How long libFuzzer lets one input run before it calls that a hang.
const HANG_SECONDS: u32 = 30;

/// How much memory libFuzzer lets a run use, in megabytes, before it calls that a failure. This is
/// libFuzzer's own default, written out so that the limit is seen and does not change with it.
const RSS_LIMIT_MB: u32 = 2048;

const USAGE: &str =
    "usage: cargo xtask fuzz <target> [--time SECONDS] [--repro FILE] [-- <libFuzzer arguments>]";

/// What to tell someone who has no rustup.
pub const RUSTUP_HINT: &str = "rustup is not installed, and fuzzing needs it for the nightly toolchain it is pinned to. Install it from https://rustup.rs and run the command again.";

/// What to tell someone who lacks the pinned nightly toolchain.
pub const TOOLCHAIN_HINT: &str = "The nightly toolchain that fuzzing is pinned to is not installed. Install it with: rustup toolchain install nightly-2026-09-28 --profile minimal\nOnly fuzzing uses it. The rest of the workspace keeps the toolchain in rust-toolchain.toml.\nThe command never skips, so install it and run the command again.";

/// What to tell someone who has no cargo-fuzz.
pub const CARGO_FUZZ_HINT: &str = "cargo-fuzz is not installed. Install the version CI uses with: cargo install cargo-fuzz --locked --version 0.13.2\nThe command never skips, so install it and run the command again.";

/// What a run is to do.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    /// The target.
    pub target: &'static Target,
    /// How long to fuzz.
    pub seconds: u64,
    /// One input to run, instead of fuzzing.
    pub repro: Option<PathBuf>,
    /// Arguments for libFuzzer, after `--`.
    pub extra: Vec<String>,
}

impl Plan {
    /// Reads the arguments that follow `fuzz`.
    pub fn parse(args: &[&str]) -> Res<Plan> {
        let mut args = args.iter().copied();
        let name = args.next().ok_or("name a target")?;
        let target = TARGETS.iter().find(|t| t.name == name).ok_or_else(|| {
            let known: Vec<&str> = TARGETS.iter().map(|t| t.name).collect();
            format!(
                "no fuzz target named `{name}`; the targets are {}",
                known.join(", ")
            )
        })?;
        let mut plan = Plan {
            target,
            seconds: DEFAULT_SECONDS,
            repro: None,
            extra: Vec::new(),
        };
        while let Some(flag) = args.next() {
            match flag {
                "--time" => {
                    let value = args.next().ok_or("--time needs a number of seconds")?;
                    plan.seconds =
                        value
                            .parse::<u64>()
                            .ok()
                            .filter(|s| *s > 0)
                            .ok_or_else(|| {
                                format!(
                                    "--time needs a whole number of seconds above 0, not `{value}`"
                                )
                            })?;
                }
                "--repro" => {
                    // Absolute, because cargo-fuzz runs in the root of the repository and the
                    // path was written for the directory the command was run in.
                    let file = args.next().ok_or("--repro needs a file")?;
                    plan.repro = Some(
                        std::path::absolute(file)
                            .map_err(|e| format!("cannot make `{file}` an absolute path: {e}"))?,
                    );
                }
                "--" => {
                    plan.extra = args.by_ref().map(str::to_owned).collect();
                }
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(plan)
    }

    /// The arguments of `cargo fuzz run`, after `rustup run <toolchain> cargo`.
    pub fn cargo_fuzz_args(&self) -> Vec<String> {
        let mut args: Vec<String> = ["fuzz", "run", self.target.name].map(String::from).into();
        if let Some(input) = &self.repro {
            args.push(input.display().to_string());
        }
        args.push("--".into());
        if self.repro.is_none() {
            args.push(format!("-max_total_time={}", self.seconds));
            args.push(format!("-max_len={}", self.target.max_len));
            args.push(format!("-dict=fuzz/dictionaries/{}.dict", self.target.name));
            args.push("-print_final_stats=1".into());
        }
        args.push(format!("-timeout={HANG_SECONDS}"));
        args.push(format!("-rss_limit_mb={RSS_LIMIT_MB}"));
        args.extend(self.extra.iter().cloned());
        args
    }
}

/// The tools fuzzing needs besides cargo. `rustup` is a field so a test can point it elsewhere.
#[derive(Debug)]
pub struct Tools {
    rustup: OsString,
}

impl Tools {
    /// The tools on the path.
    pub fn new() -> Tools {
        Tools {
            rustup: "rustup".into(),
        }
    }

    fn rustup(&self) -> Command {
        let mut cmd = Command::new(&self.rustup);
        cmd.stdin(Stdio::null());
        cmd
    }

    /// `rustup run <toolchain> cargo`, to which the arguments are added.
    pub fn cargo(&self) -> Command {
        let mut cmd = self.rustup();
        cmd.args(["run", TOOLCHAIN, "cargo"]);
        cmd
    }

    /// Checks that the pinned toolchain and cargo-fuzz are installed. `Err` holds what to tell the
    /// person who runs the command. `Ok` holds a note when cargo-fuzz is not the version CI
    /// installs, because its results may then differ from CI's.
    pub fn check(&self) -> Result<Option<String>, String> {
        let listed = self
            .rustup()
            .args(["toolchain", "list"])
            .output()
            .map_err(|_| RUSTUP_HINT.to_string())?;
        let installed = String::from_utf8_lossy(&listed.stdout);
        if !installed.lines().any(|l| l.starts_with(TOOLCHAIN)) {
            return Err(TOOLCHAIN_HINT.to_string());
        }
        let probe = self.cargo().args(["fuzz", "--version"]).output();
        let version = match probe {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => return Err(CARGO_FUZZ_HINT.to_string()),
        };
        Ok((!version.ends_with(CARGO_FUZZ_VERSION)).then(|| {
            format!("note: CI pins cargo-fuzz {CARGO_FUZZ_VERSION} but this is `{version}`; results may differ from CI's")
        }))
    }
}

/// Runs `cargo xtask fuzz`.
pub fn run(args: &[&str]) -> ExitCode {
    let plan = match Plan::parse(args) {
        Ok(plan) => plan,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match Repo::discover().and_then(|repo| execute(&repo, &plan, &Tools::new())) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Checks the tools, seeds the corpus unless one input is being reproduced, and runs libFuzzer.
/// `Ok(false)` is a failing run, which has already said so.
fn execute(repo: &Repo, plan: &Plan, tools: &Tools) -> Res<bool> {
    if let Some(note) = tools.check()? {
        eprintln!("{note}");
    }
    if plan.repro.is_none() {
        seed(repo, plan.target)?;
    }
    println!(
        "fuzzing `{}` ({}) for {}",
        plan.target.name,
        plan.target.what,
        plan.repro
            .as_ref()
            .map_or(format!("{} seconds", plan.seconds), |f| format!(
                "the input {}",
                f.display()
            ))
    );
    let started = SystemTime::now();
    let status = tools
        .cargo()
        .args(plan.cargo_fuzz_args())
        .current_dir(repo.root())
        .status()
        .map_err(|e| format!("cannot start cargo-fuzz: {e}"))?;
    if status.success() {
        return Ok(true);
    }
    eprintln!("\nlibFuzzer stopped on a failure (exit {status}).");
    let found = artifacts(
        &repo.root().join("fuzz/artifacts").join(plan.target.name),
        started,
    );
    for input in &found {
        eprintln!(
            "input: {}\nreproduce: cargo xtask fuzz {} --repro {}",
            input.display(),
            plan.target.name,
            input.display()
        );
    }
    if found.is_empty() {
        eprintln!(
            "it left no input under fuzz/artifacts/{}; the output above says why",
            plan.target.name
        );
    }
    Ok(false)
}

/// The files in `dir` that were written since `since`, newest first.
fn artifacts(dir: &Path, since: SystemTime) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(SystemTime, PathBuf)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(modified, _)| *modified >= since)
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, path)| path).collect()
}

/// Fetches the curated corpus and writes the seeds for `target` into `fuzz/corpus/<target>`. The
/// writing is a test of the testkit, which knows the fixtures and how to read the documents.
fn seed(repo: &Repo, target: &Target) -> Res<()> {
    let at = corpus::ensure_curated(repo)?;
    let dir = repo.root().join("fuzz/corpus").join(target.name);
    let cargo = Cargo::new(repo.root(), Output::Stream)
        .env(corpus::cache::ENV, &at.root.display().to_string())
        .env(CORPUS_SET_ENV, &at.set)
        .env("STETDOC_FUZZ_SEEDS", &dir.display().to_string())
        .env("STETDOC_FUZZ_TARGET", target.name)
        .env("STETDOC_FUZZ_MAX", &(target.max_len - 4).to_string());
    let outcome = cargo.test_named(
        "stetdoc-testkit",
        "fuzz_seeds_can_be_written_for_cargo_xtask_fuzz",
    );
    if outcome.ok {
        Ok(())
    } else {
        Err(format!(
            "cannot write the seeds for `{}`: {}",
            target.name, outcome.output
        ))
    }
}

#[cfg(test)]
mod tests;
