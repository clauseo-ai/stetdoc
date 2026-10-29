//! Tests that pin the scaffold's own promises: the toolchain pin, CI, the entry-point
//! files and the lint policy. They read the real repository, so a change that breaks one
//! fails `cargo xtask check`.

use crate::repo::Repo;
use std::collections::BTreeMap;

fn repo() -> Repo {
    Repo::discover().expect("repository root")
}

fn read(path: &str) -> String {
    repo().read(path).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn ci_runs_the_full_check_on_both_platforms_with_least_privilege() {
    let ci = read(".github/workflows/ci.yml");
    assert!(
        ci.contains("run: cargo xtask check\n"),
        "CI must run exactly `cargo xtask check`"
    );
    assert!(!ci.contains("--quick"), "CI must not run the quick check");
    assert!(ci.contains("ubuntu-latest") && ci.contains("macos-latest"));
    assert!(ci.contains("permissions:\n  contents: read\n"));
    assert!(ci.contains("concurrency:") && ci.contains("cancel-in-progress: true"));
    assert!(ci.contains("Swatinem/rust-cache"), "CI caches dependencies");
}

#[test]
fn ci_actions_are_pinned_by_sha_and_cargo_deny_by_version() {
    let ci = read(".github/workflows/ci.yml");
    let mut workflows = vec![ci.clone()];
    let dir = repo().root().join(".github/workflows");
    for entry in std::fs::read_dir(&dir)
        .expect("workflows directory")
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".yml") && name != "ci.yml" {
            workflows.push(read(&format!(".github/workflows/{name}")));
        }
    }
    let uses: Vec<&str> = workflows
        .iter()
        .flat_map(|w| w.lines())
        .filter(|l| l.trim_start().starts_with("- uses:") || l.trim_start().starts_with("uses:"))
        .collect();
    assert!(
        uses.len() >= 3,
        "expected the checkout, cache and install actions: {uses:?}"
    );
    for line in uses {
        let (_, after) = line.split_once('@').expect("uses: owner/repo@ref");
        let sha: String = after.chars().take(40).collect();
        assert!(
            sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()),
            "pin by commit SHA: {line}"
        );
        assert!(
            after[40..].trim_start().starts_with("# v"),
            "name the tag in a comment: {line}"
        );
    }
    let tool = format!("tool: cargo-deny@{}", crate::tools::DENY_VERSION);
    assert!(
        ci.contains(&tool),
        "CI must install {tool}, the version xtask expects"
    );
}

#[test]
fn the_toolchain_pin_and_rust_version_agree() {
    let pin = read("rust-toolchain.toml");
    let manifest = read("Cargo.toml");
    let channel = pin
        .lines()
        .find_map(|l| l.strip_prefix("channel = \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("channel in rust-toolchain.toml");
    let rust_version = manifest
        .lines()
        .find_map(|l| l.strip_prefix("rust-version = \""))
        .and_then(|l| l.strip_suffix('"'))
        .expect("rust-version in Cargo.toml");
    assert!(
        channel.starts_with(&format!("{rust_version}.")),
        "toolchain {channel} does not match rust-version {rust_version}"
    );
}

#[test]
fn the_workspace_declares_edition_licence_version_and_the_lint_policy() {
    let manifest = read("Cargo.toml");
    for needle in [
        "edition = \"2024\"",
        "license = \"MIT OR Apache-2.0\"",
        "[workspace.package]\nversion = \"",
        "publish = false",
        "unsafe_code = \"forbid\"",
        "unwrap_used = \"deny\"",
        "expect_used = \"deny\"",
        "panic = \"deny\"",
    ] {
        assert!(manifest.contains(needle), "Cargo.toml lacks {needle}");
    }
    assert!(repo().exists("Cargo.lock"), "Cargo.lock is committed");
}

#[test]
fn every_crate_uses_the_workspace_lints_except_napi() {
    for name in [
        "stetdoc-core",
        "stetdoc-cli",
        "stetdoc-editor",
        "stetdoc-testkit",
    ] {
        let manifest = read(&format!("crates/{name}/Cargo.toml"));
        assert!(
            manifest.contains("[lints]\nworkspace = true"),
            "{name} must use the workspace lints"
        );
    }
    assert!(read("xtask/Cargo.toml").contains("[lints]\nworkspace = true"));
}

/// The `key = value` lines of the `rust` and `clippy` lint tables under `prefix`.
fn lint_tables(manifest: &str, prefix: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut section: Option<String> = None;
    for line in manifest.lines().map(str::trim) {
        if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = header
                .strip_prefix(&format!("{prefix}."))
                .map(str::to_string);
        } else if let (Some(section), Some((key, value))) = (&section, line.split_once('=')) {
            map.insert(
                format!("{section}.{}", key.trim()),
                value.trim().to_string(),
            );
        }
    }
    map
}

#[test]
fn napis_lint_table_is_the_workspace_table_except_for_unsafe_code() {
    let workspace = lint_tables(&read("Cargo.toml"), "workspace.lints");
    let napi = lint_tables(&read("crates/stetdoc-napi/Cargo.toml"), "lints");
    assert!(
        workspace.len() >= 4,
        "the workspace lint table is not empty: {workspace:?}"
    );
    assert_eq!(workspace["rust.unsafe_code"], "\"forbid\"");
    assert_eq!(napi["rust.unsafe_code"], "\"allow\"");
    let mut workspace = workspace;
    let mut napi = napi;
    workspace.remove("rust.unsafe_code");
    napi.remove("rust.unsafe_code");
    assert_eq!(
        workspace, napi,
        "napi's lint table must equal the workspace's apart from unsafe_code"
    );
}

#[test]
fn library_crates_deny_missing_docs_and_the_core_denies_the_panicking_lints() {
    for name in [
        "stetdoc-core",
        "stetdoc-editor",
        "stetdoc-napi",
        "stetdoc-testkit",
    ] {
        let lib = read(&format!("crates/{name}/src/lib.rs"));
        assert!(
            lib.contains("#![deny(missing_docs)]"),
            "{name} must deny missing_docs"
        );
    }
    let lib = read("crates/stetdoc-core/src/lib.rs");
    for lint in [
        "clippy::indexing_slicing",
        "clippy::unreachable",
        "clippy::todo",
        "clippy::unimplemented",
        "clippy::dbg_macro",
        "clippy::print_stdout",
        "clippy::print_stderr",
        "clippy::allow_attributes",
        "clippy::allow_attributes_without_reason",
    ] {
        assert!(lib.contains(lint), "the core must deny {lint}");
    }
    let testkit = read("crates/stetdoc-testkit/src/lib.rs");
    assert!(testkit.contains("#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]"));
    assert!(
        testkit.contains("Panics are allowed here"),
        "the policy is stated in testkit's docs"
    );
}

#[test]
fn clippy_toml_exempts_tests_from_every_lint_the_core_denies_in_library_code() {
    let clippy = read("clippy.toml");
    for setting in [
        "allow-unwrap-in-tests = true",
        "allow-expect-in-tests = true",
        "allow-panic-in-tests = true",
        "allow-indexing-slicing-in-tests = true",
        "allow-print-in-tests = true",
        "allow-dbg-in-tests = true",
    ] {
        assert!(
            clippy.contains(setting),
            "clippy.toml must set {setting}, or unit tests in the core cannot index or print"
        );
    }
}

#[test]
fn agents_md_is_short_and_claude_md_is_a_symlink_to_it() {
    let lines = read("AGENTS.md").lines().count();
    assert!(lines <= 150, "AGENTS.md has {lines} lines, over 150");
    let link = repo().root().join("CLAUDE.md");
    let target = std::fs::read_link(&link).expect("CLAUDE.md is a symlink");
    assert_eq!(target.to_string_lossy(), "AGENTS.md");
}

#[test]
fn licence_files_and_notice_carry_the_copyright_notice() {
    for file in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"] {
        assert!(read(file).contains("Copyright (c) 2026 Clauseo"), "{file}");
    }
    assert!(read("LICENSE-APACHE").contains("Apache License"));
    assert!(read("LICENSE-APACHE").contains("END OF TERMS AND CONDITIONS"));
}

#[test]
fn docs_do_not_frame_the_project_as_a_prototype() {
    for file in [
        "README.md",
        "AGENTS.md",
        "CONTRIBUTING.md",
        "CHANGELOG.md",
        "SECURITY.md",
    ] {
        let text = read(file);
        // AGENTS.md names the banned words in order to ban them.
        let text = text.replace("\"prototype\" or \"v0.1\"", "");
        assert!(!text.to_lowercase().contains("prototype"), "{file}");
        assert!(!text.contains("v0.1"), "{file}");
    }
}
