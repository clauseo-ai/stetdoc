//! The checks that run other tools: rustfmt, clippy, the tests and cargo-deny.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Whether a tool's output streams to the terminal or is captured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Stream to the terminal while it runs.
    Stream,
    /// Capture, so tests can look at it. Only tests construct it.
    #[cfg_attr(not(test), allow(dead_code))]
    Capture,
}

/// How a tool run ended.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// Whether the tool exited with success.
    pub ok: bool,
    /// Captured output, empty when streaming.
    pub output: String,
}

/// Which crates a cargo command covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Scope {
    /// `--workspace`.
    Workspace,
    /// `-p <name>` for each name.
    Packages(Vec<String>),
}

impl Scope {
    fn args(&self) -> Vec<String> {
        match self {
            Scope::Workspace => vec!["--workspace".into()],
            Scope::Packages(names) => names
                .iter()
                .flat_map(|n| ["-p".to_string(), n.clone()])
                .collect(),
        }
    }
}

/// Runs cargo commands in a directory.
#[derive(Clone, Debug)]
pub struct Cargo {
    dir: PathBuf,
    target_dir: Option<PathBuf>,
    output: Output,
    program: OsString,
    envs: Vec<(String, String)>,
}

impl Cargo {
    /// Cargo in `dir`. It uses the `CARGO` that started this process, so the pinned
    /// toolchain applies.
    pub fn new(dir: &Path, output: Output) -> Cargo {
        Cargo {
            dir: dir.to_path_buf(),
            target_dir: None,
            output,
            program: std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
            envs: Vec::new(),
        }
    }

    /// Sets an environment variable for every command this value runs.
    #[must_use]
    pub fn env(mut self, key: &str, value: &str) -> Cargo {
        self.envs.push((key.to_string(), value.to_string()));
        self
    }

    /// The environment variables set for every command, for tests.
    #[cfg(test)]
    pub fn envs(&self) -> &[(String, String)] {
        &self.envs
    }

    /// Builds into `dir` instead of the default target directory.
    #[cfg(test)]
    #[must_use]
    pub fn target_dir(mut self, dir: &Path) -> Cargo {
        self.target_dir = Some(dir.to_path_buf());
        self
    }

    /// Uses another cargo executable. Tests use this to simulate a missing tool.
    #[cfg(test)]
    #[must_use]
    pub fn program(mut self, program: impl Into<OsString>) -> Cargo {
        self.program = program.into();
        self
    }

    fn command(&self, args: &[String]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.current_dir(&self.dir).args(args);
        cmd.envs(self.envs.iter().map(|(k, v)| (k, v)));
        cmd
    }

    fn with_target(&self, mut args: Vec<String>) -> Vec<String> {
        if let Some(dir) = &self.target_dir {
            args.push("--target-dir".into());
            args.push(dir.display().to_string());
        }
        args
    }

    fn run(&self, mut cmd: Command) -> Outcome {
        match self.output {
            Output::Stream => match cmd.status() {
                Ok(status) => Outcome {
                    ok: status.success(),
                    output: String::new(),
                },
                Err(e) => Outcome {
                    ok: false,
                    output: format!("cannot start the tool: {e}"),
                },
            },
            Output::Capture => {
                cmd.stdin(Stdio::null());
                match cmd.output() {
                    Ok(out) => Outcome {
                        ok: out.status.success(),
                        output: format!(
                            "{}{}",
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        ),
                    },
                    Err(e) => Outcome {
                        ok: false,
                        output: format!("cannot start the tool: {e}"),
                    },
                }
            }
        }
    }

    /// `cargo fmt --all --check`.
    pub fn fmt(&self) -> Outcome {
        self.run(self.command(&["fmt".into(), "--all".into(), "--check".into()]))
    }

    /// `cargo clippy --all-targets --locked -D warnings`, twice: with the default features, which
    /// is what ships, and with all of them, so that the core's `fuzzing` module is linted too. The
    /// first run is the only one that builds the core as the binaries link it, so code that calls
    /// a function that exists only with `fuzzing` fails there and passes the second.
    pub fn clippy(&self, scope: &Scope) -> Outcome {
        let passes: [(&str, &[&str]); 2] = [
            ("the default features", &[]),
            ("all features", &["--all-features"]),
        ];
        let mut outcome = Outcome {
            ok: true,
            output: String::new(),
        };
        for (name, features) in passes {
            if let Output::Stream = self.output {
                eprintln!("clippy with {name}");
            }
            let run = self.clippy_with(scope, features);
            outcome.ok &= run.ok;
            if !run.ok {
                outcome
                    .output
                    .push_str(&format!("clippy with {name} failed\n{}", run.output));
            }
        }
        outcome
    }

    fn clippy_with(&self, scope: &Scope, features: &[&str]) -> Outcome {
        let mut args: Vec<String> = vec!["clippy".into()];
        args.extend(scope.args());
        args.extend(["--all-targets".into(), "--locked".into()]);
        args.extend(features.iter().map(|f| f.to_string()));
        let mut args = self.with_target(args);
        args.extend(["--".into(), "-D".into(), "warnings".into()]);
        self.run(self.command(&args))
    }

    /// `cargo test --all-features --locked`, with snapshot updates disabled as in CI. All
    /// features, so the tests of the core's `fuzzing` module run too.
    pub fn test(&self, scope: &Scope) -> Outcome {
        let mut args: Vec<String> = vec!["test".into()];
        args.extend(scope.args());
        args.extend(["--all-features".into(), "--locked".into()]);
        let args = self.with_target(args);
        let mut cmd = self.command(&args);
        cmd.env("INSTA_UPDATE", "no");
        self.run(cmd)
    }

    /// `cargo test --release --locked -p <package> --test <target>`: one integration test file, optimized. `cargo xtask check`
    /// runs the random laws of the edit checks (`laws_random`) like this, because a debug build of them takes ten times as long.
    pub fn test_release(&self, package: &str, target: &str) -> Outcome {
        let args: Vec<String> = [
            "test",
            "--release",
            "--locked",
            "-p",
            package,
            "--test",
            target,
        ]
        .map(String::from)
        .into();
        let args = self.with_target(args);
        let mut cmd = self.command(&args);
        cmd.env("INSTA_UPDATE", "no");
        self.run(cmd)
    }

    /// `cargo test --release --locked` for one ignored unit test of a crate, alone, optimized, with
    /// its output shown. `cargo xtask bench` runs the benchmarks like this.
    pub fn run_ignored_release(&self, package: &str, test: &str) -> Outcome {
        let args: Vec<String> = [
            "test",
            "--release",
            "--locked",
            "-p",
            package,
            "--lib",
            test,
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ]
        .map(String::from)
        .into();
        let args = self.with_target(args);
        self.run(self.command(&args))
    }

    /// `cargo test --release --locked` for one ignored test of an integration test file, alone, optimized, with its output
    /// shown. `cargo xtask oracle libreoffice` runs the LibreOffice oracle like this.
    pub fn run_ignored_test_file_release(
        &self,
        package: &str,
        target: &str,
        test: &str,
    ) -> Outcome {
        let args: Vec<String> = [
            "test",
            "--release",
            "--locked",
            "-p",
            package,
            "--test",
            target,
            test,
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
        ]
        .map(String::from)
        .into();
        let args = self.with_target(args);
        self.run(self.command(&args))
    }

    /// `cargo test --locked` for the unit tests of one crate whose names contain `filter`, with
    /// their output shown. `cargo xtask fuzz` uses it to run the test that writes the seeds.
    pub fn test_named(&self, package: &str, filter: &str) -> Outcome {
        let args: Vec<String> = [
            "test",
            "-p",
            package,
            "--locked",
            "--lib",
            filter,
            "--",
            "--nocapture",
        ]
        .map(String::from)
        .into();
        let args = self.with_target(args);
        self.run(self.command(&args))
    }

    /// `cargo doc --no-deps --all-features` over the workspace, private items included, with
    /// rustdoc warnings as errors, so broken intra-doc links fail.
    pub fn doc(&self) -> Outcome {
        let mut args: Vec<String> = vec!["doc".into(), "--workspace".into(), "--no-deps".into()];
        args.extend([
            "--document-private-items".into(),
            "--all-features".into(),
            "--locked".into(),
        ]);
        let args = self.with_target(args);
        let mut cmd = self.command(&args);
        cmd.env("RUSTDOCFLAGS", "-D warnings");
        self.run(cmd)
    }

    /// `cargo deny check` (or a part of it). Fails with an install hint when
    /// cargo-deny is missing, and never skips. A cargo-deny other than the version CI
    /// pins still runs, with a note, because its results may differ from CI's.
    pub fn deny(&self, checks: &[&str]) -> Outcome {
        self.deny_with(&[], checks)
    }

    /// The cargo-deny check of the whole repository: the root workspace, then the fuzz crate.
    pub fn deny_all(&self) -> Outcome {
        let mut outcome = self.deny(&[]);
        if outcome.output == DENY_HINT {
            return outcome;
        }
        let fuzz = self.deny_fuzz();
        outcome.ok &= fuzz.ok;
        if !fuzz.output.is_empty() {
            outcome.output.push_str("\nthe fuzz crate:\n");
            outcome.output.push_str(&fuzz.output);
        }
        outcome
    }

    /// `cargo deny check` for the fuzz crate, which is a workspace of its own, so `deny` does
    /// not see its lock file. The policy is the root one with the licence of libFuzzer added.
    pub fn deny_fuzz(&self) -> Outcome {
        let options = [
            "--manifest-path",
            "fuzz/Cargo.toml",
            "--config",
            "fuzz/deny.toml",
        ];
        self.deny_with(&options, &[])
    }

    fn deny_with(&self, options: &[&str], checks: &[&str]) -> Outcome {
        let probe = self
            .command(&["deny".into(), "--version".into()])
            .stdin(Stdio::null())
            .output();
        let version = match &probe {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => {
                return Outcome {
                    ok: false,
                    output: DENY_HINT.to_string(),
                };
            }
        };
        let note = (!version.ends_with(DENY_VERSION)).then(|| {
            format!("note: CI pins cargo-deny {DENY_VERSION} but this is `{version}`; results may differ from CI's\n")
        });
        if let (Some(note), Output::Stream) = (&note, self.output) {
            eprint!("{note}");
        }
        let mut args: Vec<String> = vec!["deny".into(), "--locked".into()];
        args.extend(options.iter().map(|o| o.to_string()));
        args.push("check".into());
        args.extend(checks.iter().map(|c| c.to_string()));
        let mut outcome = self.run(self.command(&args));
        if let (Some(note), Output::Capture) = (note, self.output) {
            outcome.output.insert_str(0, &note);
        }
        outcome
    }
}

/// The cargo-deny version CI installs. Keep it equal to `ci.yml`; a test checks.
pub const DENY_VERSION: &str = "0.20.2";

/// What to tell someone who has no cargo-deny.
pub const DENY_HINT: &str = "cargo-deny is not installed. Install the version CI uses with: cargo install cargo-deny --locked --version 0.20.2\nThe check never skips, so install it and run the check again.";

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A tiny crate in a temporary directory, with the pinned toolchain.
    fn project(lib_rs: &str, extra_manifest: &str) -> TempDir {
        let dir = tempfile::tempdir().expect("temp dir");
        let toolchain = include_str!("../../rust-toolchain.toml");
        fs::write(dir.path().join("rust-toolchain.toml"), toolchain).expect("write");
        fs::write(
            dir.path().join("Cargo.toml"),
            format!("[package]\nname = \"seed\"\nversion = \"0.0.0\"\nedition = \"2024\"\n{extra_manifest}\n"),
        )
        .expect("write");
        fs::create_dir_all(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/lib.rs"), lib_rs).expect("write");
        fs::write(
            dir.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"seed\"\nversion = \"0.0.0\"\n",
        )
        .expect("write");
        dir
    }

    fn cargo(dir: &TempDir) -> Cargo {
        Cargo::new(dir.path(), Output::Capture).target_dir(&dir.path().join("target"))
    }

    #[test]
    fn a_clean_project_passes_fmt_clippy_and_test() {
        let dir = project(
            "pub fn add(a: u8, b: u8) -> u8 {\n    a + b\n}\n\n#[test]\nfn adds() {\n    assert_eq!(add(1, 2), 3);\n}\n",
            "",
        );
        let c = cargo(&dir);
        assert!(c.fmt().ok);
        assert!(c.clippy(&Scope::Workspace).ok);
        assert!(c.test(&Scope::Workspace).ok);
    }

    #[test]
    fn fmt_fails_on_badly_formatted_code() {
        let dir = project("pub fn f( ) -> u8 {   1 }\n", "");
        let outcome = cargo(&dir).fmt();
        assert!(!outcome.ok);
        assert!(outcome.output.contains("lib.rs"), "{}", outcome.output);
    }

    #[test]
    fn clippy_fails_on_a_warning() {
        let dir = project("pub fn f() -> u8 {\n    return 1;\n}\n", "");
        let outcome = cargo(&dir).clippy(&Scope::Workspace);
        assert!(!outcome.ok);
        assert!(
            outcome.output.contains("needless_return"),
            "{}",
            outcome.output
        );
    }

    #[test]
    fn clippy_builds_what_ships_so_a_hook_that_only_a_feature_adds_cannot_be_called_from_it() {
        let dir = project(
            "#[cfg(feature = \"extra\")]\nfn hook() {}\n\npub fn ships() {\n    hook();\n}\n",
            "[features]\nextra = []",
        );
        let outcome = cargo(&dir).clippy(&Scope::Workspace);
        assert!(!outcome.ok, "the default features do not have the hook");
        assert!(
            outcome.output.contains("with the default features") && outcome.output.contains("hook"),
            "{}",
            outcome.output
        );
        assert!(
            !outcome.output.contains("with all features"),
            "all features have the hook: {}",
            outcome.output
        );
    }

    #[test]
    fn test_fails_on_a_failing_test() {
        let dir = project("#[test]\nfn broken() {\n    assert_eq!(1, 2);\n}\n", "");
        let outcome = cargo(&dir).test(&Scope::Packages(vec!["seed".into()]));
        assert!(!outcome.ok);
        assert!(outcome.output.contains("broken"), "{}", outcome.output);
    }

    #[test]
    fn deny_fails_on_a_licence_that_is_not_allowed() {
        let dir = project("", "license = \"GPL-3.0-only\"");
        fs::write(
            dir.path().join("deny.toml"),
            "[licenses]\nallow = [\"MIT\"]\n",
        )
        .expect("write");
        let outcome = cargo(&dir).deny(&["licenses"]);
        assert!(!outcome.ok, "{}", outcome.output);
        assert!(
            outcome.output.contains("GPL-3.0-only"),
            "{}",
            outcome.output
        );
        assert!(
            !outcome.output.contains("not installed"),
            "cargo-deny must be installed to run the checks:\n{}",
            outcome.output
        );
    }

    #[test]
    fn deny_passes_when_the_licence_is_allowed() {
        let dir = project("", "license = \"MIT\"");
        fs::write(
            dir.path().join("deny.toml"),
            "[licenses]\nallow = [\"MIT\"]\n",
        )
        .expect("write");
        let outcome = cargo(&dir).deny(&["licenses"]);
        assert!(outcome.ok, "{}", outcome.output);
    }

    #[test]
    fn deny_takes_the_policy_it_is_given() {
        let dir = project("", "license = \"GPL-3.0-only\"");
        fs::write(
            dir.path().join("deny.toml"),
            "[licenses]\nallow = [\"MIT\"]\n",
        )
        .expect("write");
        fs::write(
            dir.path().join("other.toml"),
            "[licenses]\nallow = [\"GPL-3.0-only\"]\n",
        )
        .expect("write");
        let c = cargo(&dir);
        assert!(!c.deny_with(&[], &["licenses"]).ok);
        let outcome = c.deny_with(&["--config", "other.toml"], &["licenses"]);
        assert!(outcome.ok, "{}", outcome.output);
    }

    #[test]
    fn deny_missing_fails_with_an_install_hint_instead_of_skipping() {
        let dir = project("", "");
        let outcome = Cargo::new(dir.path(), Output::Capture)
            .program("definitely-not-cargo")
            .deny(&[]);
        assert!(!outcome.ok);
        assert!(outcome.output.contains("cargo install cargo-deny --locked"));
        let all = Cargo::new(dir.path(), Output::Capture)
            .program("definitely-not-cargo")
            .deny_all();
        assert!(!all.ok);
        assert_eq!(all.output.matches("cargo install cargo-deny").count(), 1);
    }

    #[test]
    fn doc_fails_on_a_broken_intra_doc_link() {
        let dir = project("/// See [`missing_item`].\npub fn f() {}\n", "");
        let outcome = cargo(&dir).doc();
        assert!(!outcome.ok);
        assert!(
            outcome.output.contains("missing_item"),
            "{}",
            outcome.output
        );
        let clean = project("/// Adds nothing.\npub fn f() {}\n", "");
        assert!(cargo(&clean).doc().ok);
    }

    #[test]
    fn the_install_hint_names_the_pinned_version() {
        assert!(DENY_HINT.contains(&format!("--version {DENY_VERSION}")));
    }

    #[test]
    fn scopes_become_cargo_arguments() {
        assert_eq!(Scope::Workspace.args(), ["--workspace"]);
        assert_eq!(
            Scope::Packages(vec!["a".into(), "b".into()]).args(),
            ["-p", "a", "-p", "b"]
        );
    }
}
