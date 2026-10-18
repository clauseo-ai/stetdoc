//! Which crates changed, for `cargo xtask check --quick`.
//!
//! Changes are measured against `main`, including uncommitted and untracked files. A
//! crate that changed also puts the crates that depend on it in scope, because a
//! change to the core can break the CLI. Two kinds of change reach further:
//! - a root file (the workspace manifest, the lockfile, the toolchain, lint
//!   configuration) puts everything in scope;
//! - a change to a data directory selects the crates whose tests read it. Today that is
//!   `corpus/`, read by the core's L1 harness and by the testkit.
//!
//! The workspace graph comes from `cargo metadata`, so every form of dependency
//! declaration counts.

use crate::repo::{Repo, Res};
use crate::tools::Scope;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// A workspace crate.
#[derive(Clone, Debug, Default)]
pub struct Member {
    /// Its directory, relative to the repository root.
    pub dir: String,
    /// The workspace crates it depends on, in any dependency section.
    pub deps: BTreeSet<String>,
}

/// The workspace crates and how they depend on each other.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// crate name to crate.
    pub members: BTreeMap<String, Member>,
}

/// Directories of test data, and the crates whose tests read them.
pub const DATA_DIRS: &[(&str, &[&str])] = &[("corpus/", &["stetdoc-core", "stetdoc-testkit"])];

impl Graph {
    /// Reads the graph with `cargo metadata --no-deps`.
    pub fn read(repo: &Repo) -> Res<Graph> {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let out = Command::new(cargo)
            .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
            .current_dir(repo.root())
            .output()
            .map_err(|e| format!("cannot run cargo metadata: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "cargo metadata failed:\n{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Graph::from_metadata(&String::from_utf8_lossy(&out.stdout))
    }

    /// Builds the graph from `cargo metadata --no-deps` output.
    pub fn from_metadata(json: &str) -> Res<Graph> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| format!("cannot read cargo metadata: {e}"))?;
        let packages = value["packages"]
            .as_array()
            .ok_or("cargo metadata has no packages")?;
        let workspace_root = value["workspace_root"].as_str().unwrap_or("");
        let names: BTreeSet<&str> = packages.iter().filter_map(|p| p["name"].as_str()).collect();
        let mut members = BTreeMap::new();
        for package in packages {
            let name = package["name"].as_str().ok_or("a package has no name")?;
            let manifest = package["manifest_path"].as_str().unwrap_or("");
            let dir = manifest
                .strip_prefix(workspace_root)
                .and_then(|m| m.strip_suffix("Cargo.toml"))
                .map(|m| m.trim_matches('/').to_string())
                .unwrap_or_default();
            let deps = package["dependencies"]
                .as_array()
                .map(|all| {
                    all.iter()
                        .filter_map(|d| d["name"].as_str())
                        .filter(|d| names.contains(d) && *d != name)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            members.insert(name.to_string(), Member { dir, deps });
        }
        Ok(Graph { members })
    }

    /// The crates in `changed` and every crate that depends on one of them.
    pub fn with_dependents(&self, changed: &BTreeSet<String>) -> BTreeSet<String> {
        let mut all = changed.clone();
        loop {
            let before = all.len();
            for (name, member) in &self.members {
                if member.deps.iter().any(|e| all.contains(e)) {
                    all.insert(name.clone());
                }
            }
            if all.len() == before {
                return all;
            }
        }
    }

    fn owner_of(&self, path: &str) -> Option<&str> {
        self.members
            .iter()
            .filter(|(_, m)| !m.dir.is_empty() && path.starts_with(&format!("{}/", m.dir)))
            .max_by_key(|(_, m)| m.dir.len())
            .map(|(name, _)| name.as_str())
    }
}

/// What a change touches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Affected {
    /// Nothing that needs building (for example, only docs changed).
    Nothing,
    /// These crates.
    Crates(BTreeSet<String>),
    /// Everything.
    Everything,
}

impl Affected {
    /// The cargo scope, or `None` when there is nothing to build.
    pub fn scope(&self) -> Option<Scope> {
        match self {
            Affected::Nothing => None,
            Affected::Crates(names) => Some(Scope::Packages(names.iter().cloned().collect())),
            Affected::Everything => Some(Scope::Workspace),
        }
    }
}

const ROOT_FILES: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "clippy.toml",
    "rustfmt.toml",
    "deny.toml",
];

/// Maps changed paths to the crates they affect.
pub fn affected(changed: &[String], graph: &Graph) -> Affected {
    let mut crates = BTreeSet::new();
    for path in changed {
        if ROOT_FILES.contains(&path.as_str()) || path.starts_with(".cargo/") {
            return Affected::Everything;
        }
        if let Some(owner) = graph.owner_of(path) {
            crates.insert(owner.to_string());
        }
        for (dir, readers) in DATA_DIRS {
            if path.starts_with(dir) {
                crates.extend(
                    readers
                        .iter()
                        .filter(|r| graph.members.contains_key(**r))
                        .map(|r| r.to_string()),
                );
            }
        }
    }
    if crates.is_empty() {
        Affected::Nothing
    } else {
        Affected::Crates(graph.with_dependents(&crates))
    }
}

fn git(repo: &Repo, args: &[&str]) -> Res<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo.root())
        .output()
        .map_err(|e| format!("cannot run git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Files changed relative to `main`: committed on this branch, modified in the
/// working tree, staged, or untracked. Tries `main`, then `origin/main`.
pub fn changed_files(repo: &Repo) -> Res<Vec<String>> {
    let base = ["main", "origin/main"]
        .iter()
        .find_map(|b| git(repo, &["merge-base", "HEAD", b]).ok())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| {
            "cannot find `main` or `origin/main` to compare against; run `cargo xtask check` (without --quick) instead".to_string()
        })?;
    let mut files: BTreeSet<String> = BTreeSet::new();
    for args in [
        vec!["diff", "--name-only", base.as_str()],
        vec!["ls-files", "--others", "--exclude-standard"],
    ] {
        files.extend(
            git(repo, &args)?
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string),
        );
    }
    Ok(files.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed `cargo metadata --no-deps` document. `stetdoc` declares its
    /// dependency in the table form, `[dependencies.stetdoc-core]`, which line-based
    /// parsing of manifests would miss; metadata reports it like any other.
    const METADATA: &str = r#"{
      "workspace_root": "/w",
      "packages": [
        {"name": "stetdoc-core", "manifest_path": "/w/crates/stetdoc-core/Cargo.toml", "dependencies": []},
        {"name": "stetdoc-editor", "manifest_path": "/w/crates/stetdoc-editor/Cargo.toml",
         "dependencies": [{"name": "stetdoc-core", "kind": null}]},
        {"name": "stetdoc-napi", "manifest_path": "/w/crates/stetdoc-napi/Cargo.toml",
         "dependencies": [{"name": "stetdoc-core", "kind": null}, {"name": "stetdoc-editor", "kind": null}]},
        {"name": "stetdoc", "manifest_path": "/w/crates/stetdoc-cli/Cargo.toml",
         "dependencies": [{"name": "stetdoc-core", "kind": null}, {"name": "serde", "kind": null}]},
        {"name": "stetdoc-testkit", "manifest_path": "/w/crates/stetdoc-testkit/Cargo.toml",
         "dependencies": [{"name": "stetdoc-core", "kind": "dev"}]},
        {"name": "xtask", "manifest_path": "/w/xtask/Cargo.toml", "dependencies": [{"name": "syn", "kind": null}]}
      ]
    }"#;

    fn graph() -> Graph {
        Graph::from_metadata(METADATA).expect("metadata parses")
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn paths(p: &[&str]) -> Vec<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn dependencies_come_from_metadata_in_every_section() {
        let g = graph();
        assert_eq!(
            g.members["stetdoc-napi"].deps,
            set(&["stetdoc-core", "stetdoc-editor"])
        );
        assert_eq!(g.members["stetdoc"].deps, set(&["stetdoc-core"]));
        assert_eq!(g.members["stetdoc-testkit"].deps, set(&["stetdoc-core"]));
        assert!(g.members["xtask"].deps.is_empty());
        assert_eq!(g.members["xtask"].dir, "xtask");
        assert_eq!(g.members["stetdoc-core"].dir, "crates/stetdoc-core");
    }

    #[test]
    fn the_real_workspace_graph_has_the_expected_edges() {
        let repo = Repo::discover().expect("repo");
        let g = Graph::read(&repo).expect("cargo metadata");
        for name in [
            "stetdoc-core",
            "stetdoc",
            "stetdoc-napi",
            "stetdoc-editor",
            "stetdoc-testkit",
            "xtask",
        ] {
            assert!(g.members.contains_key(name), "{name}");
        }
        assert!(g.members["stetdoc-editor"].deps.contains("stetdoc-core"));
        assert!(g.members["stetdoc-napi"].deps.contains("stetdoc-editor"));
        assert!(
            g.members["xtask"].deps.is_empty(),
            "xtask must not depend on the core"
        );
    }

    #[test]
    fn a_change_brings_in_its_dependents() {
        let a = affected(&paths(&["crates/stetdoc-editor/src/lib.rs"]), &graph());
        assert_eq!(
            a,
            Affected::Crates(set(&["stetdoc-editor", "stetdoc-napi"]))
        );
        let a = affected(&paths(&["crates/stetdoc-core/src/api/error.rs"]), &graph());
        assert_eq!(
            a,
            Affected::Crates(set(&[
                "stetdoc-core",
                "stetdoc-editor",
                "stetdoc-napi",
                "stetdoc",
                "stetdoc-testkit"
            ]))
        );
    }

    #[test]
    fn xtask_is_a_leaf_and_a_core_change_does_not_select_it() {
        assert_eq!(
            affected(&paths(&["xtask/src/main.rs"]), &graph()),
            Affected::Crates(set(&["xtask"]))
        );
        match affected(&paths(&["crates/stetdoc-core/src/lib.rs"]), &graph()) {
            Affected::Crates(c) => assert!(!c.contains("xtask")),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_change_under_corpus_selects_the_crates_that_read_it_and_their_dependents() {
        let a = affected(&paths(&["corpus/word/simple.docx"]), &graph());
        assert_eq!(
            a,
            Affected::Crates(set(&[
                "stetdoc-core",
                "stetdoc-editor",
                "stetdoc-napi",
                "stetdoc",
                "stetdoc-testkit"
            ]))
        );
        assert_eq!(
            affected(&paths(&["corpus/SOURCES.md"]), &graph()),
            affected(&paths(&["corpus/a.docx"]), &graph())
        );
    }

    #[test]
    fn root_files_affect_everything_and_docs_nothing() {
        for root in [
            "Cargo.lock",
            "Cargo.toml",
            "rust-toolchain.toml",
            ".cargo/config.toml",
            "clippy.toml",
        ] {
            assert_eq!(
                affected(&paths(&[root]), &graph()),
                Affected::Everything,
                "{root}"
            );
        }
        assert_eq!(
            affected(&paths(&["README.md", "CONTRIBUTING.md"]), &graph()),
            Affected::Nothing
        );
        assert_eq!(Affected::Nothing.scope(), None);
        assert_eq!(Affected::Everything.scope(), Some(Scope::Workspace));
    }

    fn run_git(dir: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(dir)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn changed_files_include_committed_staged_modified_and_untracked_changes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let p = dir.path();
        std::fs::create_dir_all(p.join("crates/a")).expect("mkdir");
        std::fs::write(p.join("crates/a/x.rs"), "1").expect("write");
        std::fs::write(p.join("keep.md"), "1").expect("write");
        run_git(p, &["init", "-q", "-b", "main"]);
        run_git(p, &["add", "."]);
        run_git(p, &["commit", "-q", "-m", "base"]);
        run_git(p, &["checkout", "-q", "-b", "work"]);
        std::fs::write(p.join("committed.rs"), "1").expect("write");
        run_git(p, &["add", "."]);
        run_git(p, &["commit", "-q", "-m", "work"]);
        std::fs::write(p.join("crates/a/x.rs"), "2").expect("write");
        std::fs::write(p.join("untracked.rs"), "1").expect("write");
        let repo = Repo::at(p);
        let files = changed_files(&repo).expect("changed files");
        assert_eq!(files, ["committed.rs", "crates/a/x.rs", "untracked.rs"]);
    }

    #[test]
    fn without_a_main_branch_it_says_what_to_do() {
        let dir = tempfile::tempdir().expect("temp dir");
        run_git(dir.path(), &["init", "-q", "-b", "trunk"]);
        let err = changed_files(&Repo::at(dir.path())).expect_err("no main");
        assert!(err.contains("cargo xtask check"), "{err}");
    }
}
