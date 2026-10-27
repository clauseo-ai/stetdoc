//! `cargo xtask release check`: the npm packages and the release workflow agree with the workspace.
//!
//! The release itself is `.github/workflows/release.yml`, run on a `v*` tag: it builds the CLI and the addon for the seven
//! targets, smoke-tests them and attaches the archives and the npm tarballs to a draft GitHub release. Nothing publishes to a
//! registry. This check keeps the hand-written parts in step: every package carries the workspace version, the main package
//! depends on exactly the seven target packages and has no install script, each target package declares the platform it
//! runs on, and the workflow builds every target. A test runs it, so `cargo xtask check` fails on drift.

use crate::repo::Repo;
use serde_json::Value;

/// One target: the npm platform name, and the Rust triples of its CLI and its addon.
pub struct Target {
    /// The suffix of `@stetdoc/node-<npm>` and of `stetdoc.<npm>.node`.
    pub npm: &'static str,
    /// The Rust target triple.
    pub triple: &'static str,
    /// The `os` of the npm package.
    pub os: &'static str,
    /// The `cpu` of the npm package.
    pub cpu: &'static str,
    /// The `libc` of the npm package, on Linux.
    pub libc: Option<&'static str>,
}

/// The seven release targets.
pub const TARGETS: [Target; 7] = [
    Target {
        npm: "linux-x64-gnu",
        triple: "x86_64-unknown-linux-gnu",
        os: "linux",
        cpu: "x64",
        libc: Some("glibc"),
    },
    Target {
        npm: "linux-arm64-gnu",
        triple: "aarch64-unknown-linux-gnu",
        os: "linux",
        cpu: "arm64",
        libc: Some("glibc"),
    },
    Target {
        npm: "linux-x64-musl",
        triple: "x86_64-unknown-linux-musl",
        os: "linux",
        cpu: "x64",
        libc: Some("musl"),
    },
    Target {
        npm: "linux-arm64-musl",
        triple: "aarch64-unknown-linux-musl",
        os: "linux",
        cpu: "arm64",
        libc: Some("musl"),
    },
    Target {
        npm: "darwin-x64",
        triple: "x86_64-apple-darwin",
        os: "darwin",
        cpu: "x64",
        libc: None,
    },
    Target {
        npm: "darwin-arm64",
        triple: "aarch64-apple-darwin",
        os: "darwin",
        cpu: "arm64",
        libc: None,
    },
    Target {
        npm: "win32-x64-msvc",
        triple: "x86_64-pc-windows-msvc",
        os: "win32",
        cpu: "x64",
        libc: None,
    },
];

const NPM: &str = "crates/stetdoc-napi/npm";
const WORKFLOW: &str = ".github/workflows/release.yml";
const INSTALL_SCRIPTS: [&str; 4] = ["preinstall", "install", "postinstall", "prepare"];

/// The skill every plugin carries. Each plugin folder holds a copy, because a plugin is installed from its own folder.
const SKILL: &str = "skills/stetdoc/SKILL.md";
const CLAUDE_MARKETPLACE: &str = ".claude-plugin/marketplace.json";
const CODEX_MARKETPLACE: &str = ".agents/plugins/marketplace.json";

/// One plugin: its folder under `plugins/`, its manifest, and the other files it must hold, all relative to the folder.
struct Plugin {
    folder: &'static str,
    manifest: &'static str,
    files: &'static [&'static str],
}

/// The three plugins. Each manifest, MCP config and hook file is the format its tool's docs name (see
/// packaging/README.md). The Claude Code plugin carries the guard hook and no MCP server: Claude Code uses the
/// `stetdoc` command.
const PLUGINS: [Plugin; 3] = [
    Plugin {
        folder: "plugins/claude",
        manifest: ".claude-plugin/plugin.json",
        files: &["hooks/hooks.json", "hooks/docx-guard.sh"],
    },
    Plugin {
        folder: "plugins/cursor",
        manifest: ".cursor-plugin/plugin.json",
        files: &["mcp.json"],
    },
    Plugin {
        folder: "plugins/openai",
        manifest: "plugin.json",
        files: &["mcp.json"],
    },
];

/// Runs `cargo xtask release <args>`.
pub fn run(args: &[&str]) -> std::process::ExitCode {
    let repo = match Repo::discover() {
        Ok(repo) => repo,
        Err(e) => {
            eprintln!("{e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match args {
        ["check"] => match check(&repo) {
            Ok(problems) if problems.is_empty() => {
                println!("the npm packages and {WORKFLOW} agree with the workspace");
                std::process::ExitCode::SUCCESS
            }
            Ok(problems) => {
                for problem in problems {
                    eprintln!("{problem}");
                }
                std::process::ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!(
                "usage: cargo xtask release check\n\nReleases are built by {WORKFLOW} on a v* tag, into a draft GitHub \
                 release. This command checks that the npm packages and the workflow agree with the workspace."
            );
            std::process::ExitCode::from(2)
        }
    }
}

fn read_json(repo: &Repo, path: &str) -> Result<Value, String> {
    let text =
        std::fs::read_to_string(repo.root().join(path)).map_err(|e| format!("{path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path}: not JSON: {e}"))
}

/// The workspace version, from `[workspace.package]` in the root `Cargo.toml`.
fn workspace_version(repo: &Repo) -> Result<String, String> {
    let text = std::fs::read_to_string(repo.root().join("Cargo.toml"))
        .map_err(|e| format!("Cargo.toml: {e}"))?;
    let mut in_package = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_package = line == "[workspace.package]";
        } else if in_package && let Some(value) = line.strip_prefix("version") {
            let value = value
                .trim_start()
                .trim_start_matches('=')
                .trim()
                .trim_matches('"');
            return Ok(value.to_string());
        }
    }
    Err("Cargo.toml: no version in [workspace.package]".into())
}

fn has_install_script(package: &Value) -> Option<&'static str> {
    INSTALL_SCRIPTS
        .into_iter()
        .find(|name| package.get("scripts").and_then(|s| s.get(name)).is_some())
}

/// Every disagreement, each naming the file and how to fix it.
pub fn check(repo: &Repo) -> Result<Vec<String>, String> {
    let version = workspace_version(repo)?;
    let mut problems = Vec::new();
    let main_path = format!("{NPM}/package.json");
    let main = read_json(repo, &main_path)?;
    if main["version"] != Value::String(version.clone()) {
        problems.push(format!(
            "{main_path}: version is {}, but the workspace is {version}. Set it to {version}.",
            main["version"]
        ));
    }
    if main["name"] != Value::String("@stetdoc/node".into()) {
        problems.push(format!(
            "{main_path}: name is {}. Set it to @stetdoc/node, the library's name.",
            main["name"]
        ));
    }
    if let Some(script) = has_install_script(&main) {
        problems.push(format!(
            "{main_path}: the `{script}` script runs on install. Remove it: the package installs with no script."
        ));
    }
    let optional = main["optionalDependencies"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let wanted: Vec<String> = TARGETS
        .iter()
        .map(|t| format!("@stetdoc/node-{}", t.npm))
        .collect();
    for name in &wanted {
        match optional.get(name) {
            Some(Value::String(v)) if *v == version => {}
            Some(v) => problems.push(format!(
                "{main_path}: optionalDependencies pins {name} at {v}. Pin it at exactly {version}."
            )),
            None => problems.push(format!(
                "{main_path}: optionalDependencies lacks {name}. Add it at {version}."
            )),
        }
    }
    for name in optional.keys() {
        if !wanted.contains(name) {
            problems.push(format!(
                "{main_path}: optionalDependencies has {name}, which is not a target. Remove it."
            ));
        }
    }
    for target in &TARGETS {
        let path = format!("{NPM}/targets/{}/package.json", target.npm);
        let package = match read_json(repo, &path) {
            Ok(package) => package,
            Err(e) => {
                problems.push(format!("{e}. Add the package of target {}.", target.npm));
                continue;
            }
        };
        let file = format!("stetdoc.{}.node", target.npm);
        let expect = [
            (
                "name",
                Value::String(format!("@stetdoc/node-{}", target.npm)),
            ),
            ("version", Value::String(version.clone())),
            ("main", Value::String(file.clone())),
            ("os", serde_json::json!([target.os])),
            ("cpu", serde_json::json!([target.cpu])),
            ("files", serde_json::json!([file])),
        ];
        for (key, value) in expect {
            if package[key] != value {
                problems.push(format!(
                    "{path}: {key} is {}. Set it to {value}.",
                    package[key]
                ));
            }
        }
        let libc = target
            .libc
            .map(|libc| serde_json::json!([libc]))
            .unwrap_or(Value::Null);
        if package["libc"] != libc {
            problems.push(format!(
                "{path}: libc is {}. Set it to {libc}.",
                package["libc"]
            ));
        }
        if let Some(script) = has_install_script(&package) {
            problems.push(format!(
                "{path}: the `{script}` script runs on install. Remove it."
            ));
        }
    }
    let workflow = std::fs::read_to_string(repo.root().join(WORKFLOW))
        .map_err(|e| format!("{WORKFLOW}: {e}"))?;
    for target in &TARGETS {
        if !workflow.contains(target.triple) || !workflow.contains(target.npm) {
            problems.push(format!(
                "{WORKFLOW}: target {} ({}) is not built. Add it to the build matrix.",
                target.npm, target.triple
            ));
        }
    }
    problems.extend(plugin_problems(repo));
    Ok(problems)
}

/// Every disagreement among the plugins, their skill copies and the two marketplaces that index them.
fn plugin_problems(repo: &Repo) -> Vec<String> {
    let mut problems = Vec::new();
    let skill = match repo.read(SKILL) {
        Ok(text) => text,
        Err(e) => return vec![format!("{SKILL}: {e}")],
    };
    for plugin in &PLUGINS {
        let copy = format!("{}/skills/stetdoc/SKILL.md", plugin.folder);
        match repo.read(&copy) {
            Ok(text) if text == skill => {}
            Ok(_) => problems.push(format!(
                "{copy}: differs from {SKILL}. Copy it over: cp {SKILL} {copy}"
            )),
            Err(_) => problems.push(format!(
                "{copy}: missing. Copy {SKILL} there: cp {SKILL} {copy}"
            )),
        }
        for file in std::iter::once(&plugin.manifest).chain(plugin.files) {
            let path = format!("{}/{file}", plugin.folder);
            if !repo.exists(&path) {
                problems.push(format!(
                    "{path}: missing. Each plugin folder holds its manifest and the files packaging/README.md lists for it."
                ));
            }
        }
    }
    // A marketplace's source must name a plugin folder that holds that plugin's manifest.
    match read_json(repo, CLAUDE_MARKETPLACE) {
        Ok(json) => {
            for entry in json["plugins"].as_array().into_iter().flatten() {
                let source = entry["source"].as_str().unwrap_or_default();
                let folder = source.trim_start_matches("./");
                let manifest = PLUGINS
                    .iter()
                    .find(|p| p.folder == folder && p.manifest == ".claude-plugin/plugin.json");
                if manifest.is_none_or(|p| !repo.exists(&format!("{}/{}", p.folder, p.manifest))) {
                    problems.push(format!(
                        "{CLAUDE_MARKETPLACE}: source {source:?} is not a plugin folder. Point it at one of {}.",
                        PLUGINS.iter().map(|p| p.folder).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
        Err(e) => problems.push(e),
    }
    match read_json(repo, CODEX_MARKETPLACE) {
        Ok(json) => {
            for entry in json["plugins"].as_array().into_iter().flatten() {
                let path = entry["source"]["path"].as_str().unwrap_or_default();
                let folder = path.trim_start_matches("./");
                let manifest = PLUGINS
                    .iter()
                    .find(|p| p.folder == folder && p.manifest == "plugin.json");
                if manifest.is_none_or(|p| !repo.exists(&format!("{}/{}", p.folder, p.manifest))) {
                    problems.push(format!(
                        "{CODEX_MARKETPLACE}: source path {path:?} is not a plugin folder. Point it at one of {}.",
                        PLUGINS.iter().map(|p| p.folder).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
        }
        Err(e) => problems.push(e),
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_npm_packages_and_the_release_workflow_agree_with_the_workspace() {
        let repo = Repo::discover().unwrap();
        let problems = check(&repo).unwrap();
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn drift_is_named_with_its_file_and_its_fix() {
        let real = Repo::discover().unwrap();
        let mut files: Vec<(String, String)> = vec![
            (
                "Cargo.toml".into(),
                "[workspace.package]\nversion = \"1.2.3\"\n".into(),
            ),
            (
                WORKFLOW.into(),
                real.read(WORKFLOW)
                    .unwrap()
                    .replace("aarch64-apple-darwin", "gone"),
            ),
        ];
        let main = real.read(&format!("{NPM}/package.json")).unwrap();
        let main = main.replace("\"smoke\":", "\"postinstall\": \"x\", \"smoke\":");
        files.push((format!("{NPM}/package.json"), main));
        for target in TARGETS.iter().skip(1) {
            let path = format!("{NPM}/targets/{}/package.json", target.npm);
            files.push((path.clone(), real.read(&path).unwrap()));
        }
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();
        let (_dir, repo) = crate::repo::testutil::repo(&borrowed);
        let problems = check(&repo).unwrap().join("\n");
        assert!(
            problems.contains("version is \"1.0.0-dev\", but the workspace is 1.2.3"),
            "{problems}"
        );
        assert!(
            problems.contains("the `postinstall` script runs on install"),
            "{problems}"
        );
        assert!(
            problems.contains("targets/linux-x64-gnu/package.json"),
            "{problems}"
        );
        assert!(
            problems.contains("target darwin-arm64 (aarch64-apple-darwin) is not built"),
            "{problems}"
        );
    }

    #[test]
    fn the_plugins_carry_the_skill_and_the_marketplaces_point_at_them() {
        let repo = Repo::discover().unwrap();
        let problems = plugin_problems(&repo);
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn a_plugin_copy_of_the_skill_that_differs_is_named_with_its_fix() {
        let real = Repo::discover().unwrap();
        let skill = real.read(SKILL).unwrap();
        let mut files: Vec<(String, String)> = vec![
            (SKILL.into(), skill.clone()),
            (
                CLAUDE_MARKETPLACE.into(),
                real.read(CLAUDE_MARKETPLACE).unwrap(),
            ),
            (
                CODEX_MARKETPLACE.into(),
                real.read(CODEX_MARKETPLACE).unwrap(),
            ),
        ];
        for plugin in &PLUGINS {
            files.push((
                format!("{}/skills/stetdoc/SKILL.md", plugin.folder),
                skill.clone(),
            ));
            files.push((
                format!("{}/{}", plugin.folder, plugin.manifest),
                "{}".into(),
            ));
            for file in plugin.files {
                files.push((format!("{}/{file}", plugin.folder), "{}".into()));
            }
        }
        files.push((
            "plugins/cursor/skills/stetdoc/SKILL.md".into(),
            "stale\n".into(),
        ));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(p, c)| (p.as_str(), c.as_str()))
            .collect();
        let (_dir, repo) = crate::repo::testutil::repo(&borrowed);
        let problems = plugin_problems(&repo).join("\n");
        assert!(
            problems.contains(
                "plugins/cursor/skills/stetdoc/SKILL.md: differs from skills/stetdoc/SKILL.md"
            ),
            "{problems}"
        );
        assert!(
            problems.contains("cp skills/stetdoc/SKILL.md plugins/cursor/skills/stetdoc/SKILL.md"),
            "{problems}"
        );
        assert!(!problems.contains("plugins/claude/skills"), "{problems}");
    }

    #[test]
    fn the_targets_are_seven_and_distinct() {
        let mut names: Vec<&str> = TARGETS.iter().map(|t| t.npm).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 7);
    }
}
