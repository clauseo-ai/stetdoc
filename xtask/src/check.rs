//! `cargo xtask check`: the one command that runs what the check job of CI runs.
//!
//! The full check is a list of steps. The check job of CI runs this same list, so a pass here
//! is a pass there. CI also fuzzes each target for a minute, which `cargo xtask fuzz` does. Every step runs even when an earlier one fails, so one run shows everything
//! that is wrong. `--quick` is the inner loop: format, every static check (they take
//! milliseconds), then clippy and tests for the crates changed relative to `main`.

use crate::changed::{self, Affected, Graph};
use crate::repo::{Repo, Res};
use crate::tools::{Cargo, Output, Scope};
use crate::violation::Violation;
use crate::{codegen, contracts, corpus, errors, layering, links, sizes};
use std::io::Write;
use std::rc::Rc;
use std::time::Instant;

/// One named piece of the check. `Err` carries the report to show.
pub struct Step {
    /// The name shown in the summary.
    pub name: &'static str,
    /// Runs the step.
    pub run: Box<dyn Fn() -> Result<(), String>>,
}

fn step(name: &'static str, run: impl Fn() -> Result<(), String> + 'static) -> Step {
    Step {
        name,
        run: Box::new(run),
    }
}

/// Turns a check's violations into a step result.
fn report(found: Res<Vec<Violation>>) -> Result<(), String> {
    let found = found?;
    if found.is_empty() {
        return Ok(());
    }
    let mut text = found
        .iter()
        .map(Violation::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    text.push_str(&format!(
        "\n{} problem{}",
        found.len(),
        if found.len() == 1 { "" } else { "s" }
    ));
    Err(text)
}

fn tool(name: &str, outcome: crate::tools::Outcome) -> Result<(), String> {
    if outcome.ok {
        Ok(())
    } else if outcome.output.is_empty() {
        Err(format!("{name} failed; its output is above"))
    } else {
        Err(format!("{name} failed\n{}", outcome.output))
    }
}

/// The checks that read files and need no other tool.
pub fn static_steps(repo: &Repo) -> Vec<Step> {
    let repo = Rc::new(repo.clone());
    let with = |name: &'static str, run: fn(&Repo) -> Res<Vec<Violation>>| {
        let repo = Rc::clone(&repo);
        step(name, move || report(run(&repo)))
    };
    vec![
        with("dependency table", layering::check),
        with("module contracts", contracts::check),
        with("file size", sizes::check),
        with("docs links", links::check),
        with("error table drift", errors::check_table),
        with("generated docs drift", errors::check_generated),
        with("generated schema drift", codegen::check),
        with("corpus manifest", corpus::check),
    ]
}

/// What CI runs.
pub fn full_steps(repo: &Repo) -> Vec<Step> {
    let cargo = Cargo::new(repo.root(), Output::Stream);
    let mut steps = Vec::new();
    let c = cargo.clone();
    steps.push(step("rustfmt", move || tool("rustfmt", c.fmt())));
    steps.extend(static_steps(repo));
    let c = cargo.clone();
    steps.push(step("clippy", move || {
        tool("clippy", c.clippy(&Scope::Workspace))
    }));
    let c = cargo.clone();
    steps.push(step("rustdoc", move || tool("rustdoc", c.doc())));
    let (fetch, c) = corpus_for_tests(repo, &cargo, true);
    steps.extend(fetch);
    let r = c.clone();
    steps.push(step("tests", move || {
        tool("the tests", c.test(&Scope::Workspace))
    }));
    // The random edit batches, in release: 50 batches and 10 edits of files per curated document. The debug run above
    // skips them and keeps the fixed smoke set.
    steps.push(step("random laws (release)", move || {
        tool(
            "the random laws",
            r.test_release("stetdoc-testkit", "laws_random"),
        )
    }));
    let c = cargo;
    steps.push(step("cargo deny", move || tool("cargo deny", c.deny_all())));
    steps
}

/// The set the tests read, named by the manifest hash so that two checkouts with different
/// manifests never read each other's set, whatever the shared `sets/curated` link points at.
pub(crate) const CORPUS_SET_ENV: &str = "STETDOC_CORPUS_SET";

/// The step that makes the curated corpus available (when `fetch` says the tests in scope read
/// it), and the cargo that runs the tests with `STETDOC_CORPUS_DIR` and `STETDOC_CORPUS_SET`
/// pointing at it. A failed fetch fails the step, and the tests still run with the variables
/// set, so a corpus test fails by name. If the cache location or the manifest cannot be read
/// the step fails with the reason and the tests run without the variables, which skips the
/// corpus tests; the step is what fails the check.
fn corpus_for_tests(repo: &Repo, cargo: &Cargo, fetch: bool) -> (Option<Step>, Cargo) {
    if !fetch {
        return (None, cargo.clone());
    }
    match corpus::target(repo) {
        Ok(target) => {
            let with_env = cargo
                .clone()
                .env(corpus::cache::ENV, &target.root.display().to_string())
                .env(CORPUS_SET_ENV, &target.set);
            let repo = repo.clone();
            let fetch = step("corpus fetch", move || {
                corpus::ensure_curated(&repo).map(|_| ())
            });
            (Some(fetch), with_env)
        }
        Err(e) => (
            Some(step("corpus fetch", move || Err(e.clone()))),
            cargo.clone(),
        ),
    }
}

/// Whether the tests of these crates read the corpus: the core's laws and the testkit do, and
/// so does a run of the whole workspace.
fn reads_corpus(affected: &Affected) -> bool {
    match affected {
        Affected::Nothing => false,
        Affected::Everything => true,
        Affected::Crates(names) => names
            .iter()
            .any(|n| n == "stetdoc-core" || n == "stetdoc-testkit"),
    }
}

/// The inner loop: format and the static checks, then clippy and tests for the changed
/// crates and the crates that depend on them.
pub fn quick_steps(repo: &Repo) -> Res<Vec<Step>> {
    let files = changed::changed_files(repo)?;
    let graph = Graph::read(repo)?;
    let affected = changed::affected(&files, &graph);
    let cargo = Cargo::new(repo.root(), Output::Stream);
    let mut steps = Vec::new();
    let c = cargo.clone();
    steps.push(step("rustfmt", move || tool("rustfmt", c.fmt())));
    steps.extend(static_steps(repo).into_iter().map(|s| {
        if s.name == "corpus manifest" {
            // Rows of the manifest that already passed are not validated again.
            let repo = Rc::new(repo.clone());
            step("corpus manifest", move || {
                report(corpus::check_cached(&repo))
            })
        } else {
            s
        }
    }));
    match affected.scope() {
        None => println!("no crate changed relative to main: skipping clippy and tests"),
        Some(scope) => {
            if let Affected::Crates(names) = &affected {
                println!(
                    "crates in scope: {}",
                    names.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            let (c, s) = (cargo.clone(), scope.clone());
            steps.push(step("clippy", move || tool("clippy", c.clippy(&s))));
            let (fetch, c) = corpus_for_tests(repo, &cargo, reads_corpus(&affected));
            steps.extend(fetch);
            steps.push(step("tests", move || tool("the tests", c.test(&scope))));
        }
    }
    Ok(steps)
}

/// Runs the steps, printing progress to `out`. Returns whether every step passed.
pub fn execute(steps: &[Step], out: &mut dyn Write) -> bool {
    let mut results: Vec<(&str, bool, f64)> = Vec::new();
    for step in steps {
        let _ = writeln!(out, "==> {}", step.name);
        let _ = out.flush();
        let started = Instant::now();
        let outcome = (step.run)();
        let secs = started.elapsed().as_secs_f64();
        match &outcome {
            Ok(()) => {
                let _ = writeln!(out, "ok ({secs:.1}s)");
            }
            Err(text) => {
                let _ = writeln!(out, "{text}\nFAILED ({secs:.1}s)");
            }
        }
        results.push((step.name, outcome.is_ok(), secs));
    }
    let failed = results.iter().filter(|(_, ok, _)| !ok).count();
    let _ = writeln!(out, "\nsummary");
    for (name, ok, secs) in &results {
        let _ = writeln!(
            out,
            "  {:<4} {:<22} {:>6.1}s",
            if *ok { "ok" } else { "FAIL" },
            name,
            secs
        );
    }
    if failed == 0 {
        let _ = writeln!(out, "check passed");
    } else {
        let _ = writeln!(
            out,
            "check failed: {failed} of {} steps failed",
            results.len()
        );
    }
    failed == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::testutil::repo;

    fn run(steps: Vec<Step>) -> (bool, String) {
        let mut out = Vec::new();
        let ok = execute(&steps, &mut out);
        (ok, String::from_utf8(out).expect("utf-8"))
    }

    #[test]
    fn all_passing_steps_pass() {
        let (ok, text) = run(vec![step("a", || Ok(())), step("b", || Ok(()))]);
        assert!(ok);
        assert!(text.contains("check passed"));
    }

    #[test]
    fn a_failing_step_fails_the_check_but_later_steps_still_run() {
        let (ok, text) = run(vec![
            step("first", || Err("boom".into())),
            step("second", || Ok(())),
        ]);
        assert!(!ok);
        assert!(text.contains("boom"));
        assert!(text.contains("==> second"));
        assert!(text.contains("FAIL first"));
        assert!(text.contains("check failed: 1 of 2 steps failed"));
    }

    #[test]
    fn violations_are_reported_with_file_line_and_fix() {
        let result = report(Ok(vec![Violation::new("a.rs", 3, "bad", "do better")]));
        let text = result.expect_err("fails");
        assert!(text.contains("a.rs:3: bad"));
        assert!(text.contains("fix: do better"));
        assert!(text.ends_with("1 problem"));
        assert_eq!(report(Ok(vec![])), Ok(()));
        assert_eq!(report(Err("io".into())), Err("io".to_string()));
    }

    #[test]
    fn a_seeded_violation_makes_a_static_step_fail_through_the_runner() {
        let (_dir, repo) = repo(&[
            (
                "ARCHITECTURE.md",
                "### Allowed dependencies\n\n| Module | May depend on |\n|---|---|\n| `api` | nothing |\n",
            ),
            ("crates/stetdoc-core/src/api/mod.rs", "use crate::xml::X;\n"),
        ]);
        let steps: Vec<Step> = static_steps(&repo)
            .into_iter()
            .filter(|s| s.name == "dependency table")
            .collect();
        let (ok, text) = run(steps);
        assert!(!ok);
        assert!(
            text.contains("crates/stetdoc-core/src/api/mod.rs:1:"),
            "{text}"
        );
    }

    fn corpus_repo() -> (tempfile::TempDir, Repo) {
        repo(&[
            (
                "corpus/sources.tsv",
                "demo\thttps://example.test/demo\t0123456789abcdef0123456789abcdef01234567\tMIT\tfetch\tyes\n",
            ),
            ("corpus/curated.tsv", ""),
        ])
    }

    #[test]
    fn the_tests_read_the_set_named_by_the_manifest_hash() {
        let (_dir, repo) = corpus_repo();
        let cargo = Cargo::new(repo.root(), Output::Capture);
        let (fetch, c) = corpus_for_tests(&repo, &cargo, true);
        assert_eq!(fetch.map(|s| s.name), Some("corpus fetch"));
        let hash = corpus::manifest::Manifest::hash_of(&repo).expect("hash");
        let envs: std::collections::BTreeMap<&str, &str> = c
            .envs()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            envs.get(CORPUS_SET_ENV),
            Some(&format!("curated-{}", &hash[..12]).as_str())
        );
        assert!(envs.contains_key(corpus::cache::ENV), "{envs:?}");
    }

    #[test]
    fn a_run_that_needs_no_corpus_neither_fetches_nor_sets_the_variables() {
        let (_dir, repo) = corpus_repo();
        let cargo = Cargo::new(repo.root(), Output::Capture);
        let (fetch, c) = corpus_for_tests(&repo, &cargo, false);
        assert!(fetch.is_none());
        assert!(c.envs().is_empty());
    }

    #[test]
    fn a_manifest_that_cannot_be_read_fails_the_fetch_step_and_sets_nothing() {
        let (_dir, repo) = repo(&[]);
        let cargo = Cargo::new(repo.root(), Output::Capture);
        let (fetch, c) = corpus_for_tests(&repo, &cargo, true);
        let step = fetch.expect("a step that fails");
        assert!((step.run)().is_err());
        assert!(c.envs().is_empty(), "no empty STETDOC_CORPUS_DIR");
    }

    #[test]
    fn only_the_crates_that_read_the_corpus_fetch_it() {
        let crates =
            |names: &[&str]| Affected::Crates(names.iter().map(|n| n.to_string()).collect());
        assert!(!reads_corpus(&Affected::Nothing));
        assert!(reads_corpus(&Affected::Everything));
        assert!(reads_corpus(&crates(&["stetdoc-core", "stetdoc"])));
        assert!(reads_corpus(&crates(&["stetdoc-testkit"])));
        assert!(!reads_corpus(&crates(&[
            "stetdoc",
            "stetdoc-editor",
            "xtask"
        ])));
    }

    #[test]
    fn the_full_check_lists_every_check_ci_needs() {
        let (_dir, repo) = repo(&[]);
        let names: Vec<&str> = full_steps(&repo).iter().map(|s| s.name).collect();
        assert_eq!(
            names,
            [
                "rustfmt",
                "dependency table",
                "module contracts",
                "file size",
                "docs links",
                "error table drift",
                "generated docs drift",
                "generated schema drift",
                "corpus manifest",
                "clippy",
                "rustdoc",
                "corpus fetch",
                "tests",
                "random laws (release)",
                "cargo deny"
            ]
        );
    }
}
