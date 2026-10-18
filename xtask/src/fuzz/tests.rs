//! Tests of `cargo xtask fuzz`: the arguments, the tool checks, and the pins that must agree.

use super::*;
use crate::repo::Repo;

fn parse(args: &[&str]) -> Res<Plan> {
    Plan::parse(args)
}

#[test]
fn a_target_alone_runs_for_the_default_time() {
    let plan = parse(&["xml"]).expect("parses");
    assert_eq!(plan.target.name, "xml");
    assert_eq!(plan.seconds, DEFAULT_SECONDS);
    assert_eq!(plan.repro, None);
    assert!(plan.extra.is_empty());
}

#[test]
fn time_repro_and_libfuzzer_arguments_are_read() {
    let plan = parse(&[
        "package",
        "--time",
        "600",
        "--",
        "-fork=2",
        "-ignore_crashes=1",
    ])
    .expect("parses");
    assert_eq!(plan.target.name, "package");
    assert_eq!(plan.seconds, 600);
    assert_eq!(plan.extra, ["-fork=2", "-ignore_crashes=1"]);
    let plan = parse(&["xml", "--repro", "fuzz/artifacts/xml/crash-1"]).expect("parses");
    let here = std::env::current_dir().expect("current directory");
    assert_eq!(
        plan.repro,
        Some(here.join("fuzz/artifacts/xml/crash-1")),
        "a relative path is made absolute where the command runs, since cargo-fuzz runs elsewhere"
    );
    let plan = parse(&["xml", "--repro", "/tmp/crash-2"]).expect("parses");
    assert_eq!(plan.repro, Some(PathBuf::from("/tmp/crash-2")));
}

#[test]
fn wrong_arguments_say_what_is_wrong() {
    assert!(parse(&[]).expect_err("no target").contains("name a target"));
    let unknown = parse(&["document"]).expect_err("unknown target");
    assert!(
        unknown.contains("`document`") && unknown.contains("package, xml"),
        "{unknown}"
    );
    for bad in [
        ["xml", "--time", "0"],
        ["xml", "--time", "soon"],
        ["xml", "--time", "-5"],
    ] {
        let e = parse(&bad).expect_err("bad time");
        assert!(e.contains("--time"), "{e}");
    }
    assert!(
        parse(&["xml", "--time"])
            .expect_err("no value")
            .contains("--time")
    );
    assert!(
        parse(&["xml", "--repro"])
            .expect_err("no file")
            .contains("--repro")
    );
    assert!(
        parse(&["xml", "--sanitizer"])
            .expect_err("unknown flag")
            .contains("--sanitizer")
    );
}

#[test]
fn a_fuzzing_run_gives_libfuzzer_its_time_length_dictionary_and_hang_limit() {
    let args = parse(&["package", "--time", "90"])
        .expect("parses")
        .cargo_fuzz_args();
    assert_eq!(&args[..3], ["fuzz", "run", "package"]);
    let rest = &args[4..];
    assert!(rest.contains(&"-max_total_time=90".to_string()), "{args:?}");
    assert!(rest.contains(&"-max_len=262144".to_string()), "{args:?}");
    assert!(
        rest.contains(&"-dict=fuzz/dictionaries/package.dict".to_string()),
        "{args:?}"
    );
    assert!(rest.contains(&"-timeout=30".to_string()), "{args:?}");
    assert!(rest.contains(&"-rss_limit_mb=2048".to_string()), "{args:?}");
    assert!(
        rest.contains(&"-print_final_stats=1".to_string()),
        "{args:?}"
    );
}

#[test]
fn a_reproduction_runs_one_input_and_nothing_else() {
    let args = parse(&["xml", "--repro", "/tmp/crash-1"])
        .expect("parses")
        .cargo_fuzz_args();
    assert_eq!(args[..4], ["fuzz", "run", "xml", "/tmp/crash-1"]);
    assert!(args.contains(&"-rss_limit_mb=2048".to_string()), "{args:?}");
    assert!(
        !args
            .iter()
            .any(|a| a.starts_with("-max_total_time") || a.starts_with("-dict")),
        "{args:?}"
    );
}

#[test]
fn extra_libfuzzer_arguments_come_last() {
    let args = parse(&["xml", "--", "-jobs=4"])
        .expect("parses")
        .cargo_fuzz_args();
    assert_eq!(args.last().map(String::as_str), Some("-jobs=4"));
}

#[test]
fn the_hints_name_the_pins() {
    assert!(TOOLCHAIN_HINT.contains(&format!("rustup toolchain install {TOOLCHAIN} ")));
    assert!(CARGO_FUZZ_HINT.contains(&format!("--version {CARGO_FUZZ_VERSION}")));
}

/// A `rustup` that is a script, for a test of the checks that must not need the real one.
#[cfg(unix)]
fn fake_rustup(dir: &Path, listing: &str, fuzz_version: Option<&str>) -> Tools {
    use std::os::unix::fs::PermissionsExt;
    let fuzz = match fuzz_version {
        Some(v) => format!("echo cargo-fuzz {v}"),
        None => "echo no such command >&2; exit 101".to_string(),
    };
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = toolchain ]; then echo '{listing}'; exit 0; fi\nif [ \"$1\" = run ]; then {fuzz}; exit 0; fi\nexit 1\n"
    );
    let path = dir.join("rustup");
    std::fs::write(&path, script).expect("write the script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    Tools {
        rustup: path.into_os_string(),
    }
}

#[test]
fn a_missing_rustup_fails_with_the_hint_instead_of_skipping() {
    let tools = Tools {
        rustup: "definitely-not-rustup".into(),
    };
    assert_eq!(tools.check(), Err(RUSTUP_HINT.to_string()));
}

#[cfg(unix)]
#[test]
fn a_missing_toolchain_or_cargo_fuzz_fails_with_its_hint() {
    let dir = tempfile::tempdir().expect("dir");
    let tools = fake_rustup(
        dir.path(),
        "stable-x86_64-unknown-linux-gnu (default)",
        Some(CARGO_FUZZ_VERSION),
    );
    assert_eq!(tools.check(), Err(TOOLCHAIN_HINT.to_string()));
    let listing = format!("{TOOLCHAIN}-x86_64-unknown-linux-gnu");
    let tools = fake_rustup(dir.path(), &listing, None);
    assert_eq!(tools.check(), Err(CARGO_FUZZ_HINT.to_string()));
}

#[cfg(unix)]
#[test]
fn the_tools_pass_when_both_are_there_and_another_cargo_fuzz_version_gets_a_note() {
    let dir = tempfile::tempdir().expect("dir");
    let listing = format!("{TOOLCHAIN}-x86_64-unknown-linux-gnu");
    let tools = fake_rustup(dir.path(), &listing, Some(CARGO_FUZZ_VERSION));
    assert_eq!(tools.check(), Ok(None));
    let tools = fake_rustup(dir.path(), &listing, Some("0.1.0"));
    let note = tools.check().expect("runs").expect("a note");
    assert!(
        note.contains("0.1.0") && note.contains(CARGO_FUZZ_VERSION),
        "{note}"
    );
}

fn real(path: &str) -> String {
    Repo::discover()
        .expect("repository")
        .read(path)
        .unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_pinned_toolchain_is_the_same_in_xtask_the_fuzz_directory_and_ci() {
    let pin = real("fuzz/rust-toolchain.toml");
    assert!(pin.contains(&format!("channel = \"{TOOLCHAIN}\"")), "{pin}");
    let ci = real(".github/workflows/ci.yml");
    assert!(
        ci.contains(&format!("rustup toolchain install {TOOLCHAIN} ")),
        "ci.yml must install {TOOLCHAIN}, the toolchain xtask runs fuzzing with"
    );
    assert!(
        ci.contains(&format!(
            "run: cargo install cargo-fuzz --locked --version {CARGO_FUZZ_VERSION}"
        )),
        "ci.yml must install cargo-fuzz {CARGO_FUZZ_VERSION}, the version xtask expects"
    );
}

#[test]
fn every_target_has_a_binary_a_source_file_and_a_dictionary_and_ci_runs_it() {
    let manifest = real("fuzz/Cargo.toml");
    let ci = real(".github/workflows/ci.yml");
    let repo = Repo::discover().expect("repository");
    for t in TARGETS {
        assert!(
            manifest.contains(&format!(
                "name = \"{}\"\npath = \"fuzz_targets/{}.rs\"",
                t.name, t.name
            )),
            "fuzz/Cargo.toml has no [[bin]] for {}",
            t.name
        );
        assert!(
            repo.exists(&format!("fuzz/fuzz_targets/{}.rs", t.name)),
            "{}",
            t.name
        );
        assert!(
            repo.exists(&format!("fuzz/dictionaries/{}.dict", t.name)),
            "{}",
            t.name
        );
        assert!(
            ci.contains(&format!("cargo xtask fuzz {} --time 60", t.name))
                || ci.contains("cargo xtask fuzz ${{ matrix.target }} --time 60"),
            "ci.yml does not run the {} smoke test",
            t.name
        );
        let source = real(&format!("fuzz/fuzz_targets/{}.rs", t.name));
        assert!(
            source.contains(&format!("stetdoc_core::fuzz::{}(data)", t.name)),
            "the {} target does not call its entry point",
            t.name
        );
    }
    let mut in_matrix = ci_matrix(&ci);
    in_matrix.sort();
    let mut targets: Vec<&str> = TARGETS.iter().map(|t| t.name).collect();
    targets.sort();
    assert_eq!(
        in_matrix, targets,
        "the matrix of the fuzz job in ci.yml must list every target once and nothing else"
    );
    let declared = manifest.matches("[[bin]]").count();
    assert_eq!(
        declared,
        TARGETS.len(),
        "a [[bin]] in fuzz/Cargo.toml has no target here"
    );
}

#[test]
fn every_max_len_leaves_room_for_the_choices_and_the_seeds_are_cut_to_fit() {
    for t in TARGETS {
        assert!(t.max_len > 4096 && t.max_len <= 1 << 20, "{}", t.name);
    }
}

#[test]
fn artifacts_lists_only_what_a_run_wrote_newest_first() {
    let dir = tempfile::tempdir().expect("dir");
    std::fs::write(dir.path().join("old"), "x").expect("write");
    std::thread::sleep(std::time::Duration::from_millis(50));
    let since = SystemTime::now();
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(dir.path().join("first"), "x").expect("write");
    std::thread::sleep(std::time::Duration::from_millis(50));
    std::fs::write(dir.path().join("second"), "x").expect("write");
    let names: Vec<String> = artifacts(dir.path(), since)
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    assert_eq!(names, ["second", "first"]);
    assert!(artifacts(&dir.path().join("none"), since).is_empty());
}

#[test]
fn only_the_core_declares_the_fuzzing_feature_and_only_the_fuzz_crate_turns_it_on() {
    let repo = Repo::discover().expect("repository");
    let manifests = repo.files().expect("files");
    let manifests = manifests.iter().filter(|f| f.ends_with("Cargo.toml"));
    for file in manifests {
        let mentions = repo.read(file).expect("reads").contains("fuzzing");
        let may = file == "crates/stetdoc-core/Cargo.toml" || file == "fuzz/Cargo.toml";
        assert_eq!(
            mentions, may,
            "{file}: no crate that ships may enable `fuzzing`, which makes `fuzz` public"
        );
    }
}

/// The lines of a cargo-deny policy that mean something: no comments and no blank lines.
fn policy_lines(text: &str) -> std::collections::BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_fuzz_policy_is_the_root_policy_with_ncsa_and_with_unused_licences_allowed() {
    let repo = Repo::discover().expect("repository");
    let root = policy_lines(&repo.read("deny.toml").expect("reads"));
    let fuzz = policy_lines(&repo.read("fuzz/deny.toml").expect("reads"));
    let missing: Vec<_> = root.difference(&fuzz).collect();
    assert!(
        missing.is_empty(),
        "fuzz/deny.toml lacks {missing:?} of deny.toml"
    );
    let extra: Vec<&str> = fuzz.difference(&root).map(String::as_str).collect();
    assert_eq!(
        extra,
        ["\"NCSA\",", "unused-allowed-license = \"allow\""],
        "fuzz/deny.toml may differ from deny.toml only by the NCSA licence and the unused allowance"
    );
}

#[test]
fn ci_keeps_the_failing_input_of_a_fuzz_run_where_xtask_says_it_leaves_it() {
    let ci = real(".github/workflows/ci.yml");
    let after = ci.split("- name: Keep the failing input").nth(1);
    let rest = after.expect("ci.yml has a step that keeps the failing input");
    let lines = rest.lines().skip(1);
    let step = lines
        .take_while(|l| !l.trim().is_empty())
        .collect::<Vec<_>>();
    let step = step.join("\n");
    assert!(step.contains("if: failure()"), "{step}");
    assert!(step.contains("actions/upload-artifact@"), "{step}");
    assert!(
        step.contains("path: fuzz/artifacts/${{ matrix.target }}"),
        "xtask leaves a failing input under fuzz/artifacts/<target>:\n{step}"
    );
}

/// The targets in the matrix of the fuzz job of `ci.yml`, in the order it lists them.
fn ci_matrix(ci: &str) -> Vec<String> {
    let lines = ci.lines().skip_while(|l| l.trim() != "target:").skip(1);
    lines
        .map(str::trim)
        .map_while(|l| l.strip_prefix("- "))
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_matrix_of_the_fuzz_job_is_read_as_a_list_of_names() {
    let ci = "jobs:\n  fuzz:\n    strategy:\n      matrix:\n        target:\n          - package\n          - xml\n    steps:\n      - uses: x\n";
    assert_eq!(ci_matrix(ci), ["package", "xml"]);
    assert!(ci_matrix("no matrix here").is_empty());
    let twice = ci.replace("- xml", "- package");
    assert_eq!(ci_matrix(&twice), ["package", "package"]);
}

/// What a `Cargo.lock` locks: for each package name, the versions with their checksums.
fn locked(
    text: &str,
) -> std::collections::BTreeMap<String, std::collections::BTreeSet<(String, String)>> {
    let mut out: std::collections::BTreeMap<String, std::collections::BTreeSet<(String, String)>> =
        std::collections::BTreeMap::new();
    for block in text.split("[[package]]").skip(1) {
        let field = |key: &str| {
            let start = format!("{key} = \"");
            block
                .lines()
                .find_map(|l| l.strip_prefix(start.as_str())?.strip_suffix('"'))
                .unwrap_or("")
                .to_owned()
        };
        out.entry(field("name"))
            .or_default()
            .insert((field("version"), field("checksum")));
    }
    out
}

#[test]
fn a_lock_file_is_read_as_versions_and_checksums_by_name() {
    let text = "version = 4\n\n[[package]]\nname = \"a\"\nversion = \"1.0.0\"\nsource = \"registry+x\"\nchecksum = \"abc\"\n\n[[package]]\nname = \"a\"\nversion = \"2.0.0\"\n\n[[package]]\nname = \"b\"\nversion = \"0.1.0\"\n";
    let lock = locked(text);
    assert_eq!(lock.len(), 2);
    assert_eq!(lock["a"].len(), 2);
    assert!(lock["a"].contains(&("1.0.0".to_owned(), "abc".to_owned())));
    assert!(lock["b"].contains(&("0.1.0".to_owned(), String::new())));
}

#[test]
fn the_fuzz_crate_locks_no_other_version_of_a_package_than_the_workspace_does() {
    let repo = Repo::discover().expect("repository");
    let shipped = locked(&repo.read("Cargo.lock").expect("reads"));
    let fuzzed = locked(&repo.read("fuzz/Cargo.lock").expect("reads"));
    let mut shared = 0;
    for (name, versions) in &fuzzed {
        let Some(in_the_workspace) = shipped.get(name) else {
            continue;
        };
        shared += 1;
        assert!(
            versions.is_subset(in_the_workspace),
            "fuzz/Cargo.lock locks {name} as {versions:?} and Cargo.lock as {in_the_workspace:?}, so the fuzz targets would not test what ships; run `cargo update --manifest-path fuzz/Cargo.toml -p {name} --precise <the version in Cargo.lock>`"
        );
    }
    assert!(
        shared >= 3,
        "the two lock files share memchr, miniz_oxide and stetdoc-core at least"
    );
}
