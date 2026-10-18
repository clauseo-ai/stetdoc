//! Tests of `cargo xtask bench`: the arguments, and the names it shares with the harness.

use super::*;

#[test]
fn the_arguments_are_passed_on_as_the_words_the_harness_reads() {
    assert!(parse(&[]).unwrap().is_empty());
    assert_eq!(
        parse(&[
            "--ci",
            "--strict-time",
            "--filter",
            "xml",
            "--input",
            "dense",
            "--update-baseline",
            "--list"
        ])
        .unwrap(),
        [
            "--ci",
            "--strict-time",
            "--filter",
            "xml",
            "--input",
            "dense",
            "--update-baseline",
            "--list"
        ]
    );
}

#[test]
fn the_output_directory_is_made_absolute_because_the_benchmarks_run_in_another_directory() {
    let words = parse(&["--out", "results/night"]).unwrap();
    assert_eq!(words[0], "--out");
    let dir = std::path::Path::new(&words[1]);
    assert!(dir.is_absolute(), "{dir:?}");
    assert!(dir.ends_with("results/night"), "{dir:?}");
    let already = parse(&["--out", "/tmp/bench out"]).unwrap();
    assert_eq!(
        already[1], "/tmp/bench out",
        "a path with a space stays as it is"
    );
}

#[test]
fn a_wrong_argument_is_refused_by_name_before_anything_is_built() {
    assert!(parse(&["--fast"]).unwrap_err().contains("`--fast`"));
    assert!(
        parse(&["--filter"])
            .unwrap_err()
            .contains("--filter needs a value")
    );
    assert!(parse(&["xml"]).unwrap_err().contains("`xml`"));
}

fn real(path: &str) -> String {
    Repo::discover()
        .expect("repository")
        .read(path)
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_flags_the_variable_and_the_test_are_the_ones_the_harness_and_the_core_have() {
    let args = real("crates/stetdoc-testkit/src/bench/args.rs");
    for flag in SWITCHES.iter().chain(&OPTIONS) {
        assert!(
            args.contains(&format!("\"{flag}\"")),
            "the harness does not read {flag}"
        );
    }
    assert!(
        args.contains(&format!("const ENV: &str = \"{ARGS_ENV}\";")),
        "the harness reads another variable than {ARGS_ENV}"
    );
    assert_eq!(SEPARATOR, '\u{1f}');
    assert!(
        args.contains("'\\u{1f}'"),
        "the harness splits its words at another character than the separator"
    );
    let core = real("crates/stetdoc-core/src/bench/mod.rs");
    let name = TEST.rsplit("::").next().unwrap();
    assert!(
        core.contains(&format!("fn {name}()")),
        "the core has no test {TEST}"
    );
    let at = core.find(&format!("fn {name}()")).unwrap();
    assert!(
        core[..at].trim_end().ends_with("]"),
        "{TEST} has attributes"
    );
    assert!(
        core[..at].contains("#[ignore"),
        "{TEST} must be ignored, so that `cargo test` skips it"
    );
}

#[test]
fn every_benchmark_of_the_core_is_in_the_list_that_runs() {
    let repo = Repo::discover().expect("repository");
    let list = real("crates/stetdoc-core/src/bench/mod.rs");
    let mut found = 0;
    for file in repo.files().expect("files") {
        let Some(name) = file
            .strip_prefix("crates/stetdoc-core/src/bench/")
            .and_then(|f| f.strip_suffix(".rs"))
            .filter(|n| *n != "mod" && !n.contains('/'))
        else {
            continue;
        };
        let source = real(&file);
        for line in source.lines() {
            let Some(rest) = line.strip_prefix("pub(super) const ") else {
                continue;
            };
            let Some((constant, kind)) = rest.split_once(':') else {
                continue;
            };
            if kind.trim_start().starts_with("Bench") {
                found += 1;
                assert!(
                    list.contains(&format!("{name}::{constant}")),
                    "{file} defines {constant}, which `all()` in bench/mod.rs does not list, so it never runs"
                );
            }
        }
        assert!(
            list.contains(&format!("mod {name};")),
            "bench/mod.rs does not declare {file}"
        );
    }
    assert!(found >= 7, "only {found} benchmarks found");
}
