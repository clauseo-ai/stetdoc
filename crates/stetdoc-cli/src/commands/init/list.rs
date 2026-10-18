//! `stetdoc init --list`: every agent, whether it is detected, its folder, what is installed there, and the saved author.

use super::agents::{AGENTS, Env, Scope};
use super::{MAX_FILE, scope_name, shown};
use crate::config;
use crate::fsio;
use crate::output::Outcome;
use crate::skill;
use serde_json::json;
use std::fmt::Write;
use std::path::Path;

/// What is installed in one folder: the version, `None` for nothing, or a word for something else.
fn installed(base: &Path, folder: &str) -> (bool, Option<String>, &'static str) {
    let path = base.join(folder).join("SKILL.md");
    match fsio::read_text_if_any(&path, MAX_FILE) {
        Ok(None) => (false, None, "no"),
        Ok(Some(text)) if skill::is_ours(&text) => {
            let version = skill::stub_version(&text);
            let label = if version.is_some() {
                ""
            } else {
                "yes, an earlier stub"
            };
            (true, version, label)
        }
        Ok(Some(text)) if skill::names_stetdoc(&text) => (false, None, "edited or your own"),
        _ => (false, None, "another skill"),
    }
}

/// `--list`: every agent, whether it is detected, its folder in the scope, and what is installed there.
pub fn list(env: &Env, scope: Scope, base: &Path) -> Outcome {
    let mut rows = Vec::new();
    for agent in &AGENTS {
        let folder = agent.folder(scope);
        let (is_installed, version) = match folder {
            Some(folder) => {
                let (yes, version, _) = installed(base, folder);
                (yes, version)
            }
            None => (false, None),
        };
        rows.push(json!({
            "id": agent.id,
            "name": agent.name,
            "detected": env.detects(agent),
            "folder": folder.map(|f| shown(scope, f)),
            "path": folder.map(|f| base.join(f).display().to_string()),
            "installed": is_installed,
            "version": version,
        }));
    }
    let json =
        json!({ "scope": scope_name(scope), "root": base.display().to_string(), "agents": rows });
    let author = config::author();
    let config_path = config::path().map(|p| p.display().to_string());
    let mut text = table(env, scope, base, false);
    let shown_path = config_path
        .clone()
        .unwrap_or_else(|| "no config folder".into());
    match &author {
        Some(name) => {
            let _ = writeln!(text, "\nauthor: {name} (saved in {shown_path})");
        }
        None => {
            let _ = writeln!(
                text,
                "\nauthor: none saved ({shown_path}). Run stetdoc init --author \"Your Name\", or set STETDOC_AUTHOR."
            );
        }
    }
    let mut json = json;
    if let Some(object) = json.as_object_mut() {
        object.insert("author".into(), json!(author));
        object.insert("config".into(), json!(config_path));
    }
    Outcome::new(text, json)
}

/// The table of `--list` and of the prompt, which numbers its rows.
pub fn table(env: &Env, scope: Scope, base: &Path, numbered: bool) -> String {
    let mut text = match scope {
        Scope::User => {
            "Agents, with their skill folders in your home folder (--project for a project):\n"
                .to_string()
        }
        Scope::Project => format!(
            "Agents, with their skill folders in the project {}:\n",
            base.display()
        ),
    };
    let _ = writeln!(
        text,
        "{}{:<12} {:<9} {:<21} folder",
        if numbered { "     " } else { "  " },
        "agent",
        "detected",
        "installed"
    );
    for (i, agent) in AGENTS.iter().enumerate() {
        let detected = if env.detects(agent) { "yes" } else { "no" };
        let (installed, folder) = match agent.folder(scope) {
            Some(folder) => {
                let (_, version, label) = installed(base, folder);
                (
                    version.unwrap_or_else(|| label.to_string()),
                    shown(scope, folder),
                )
            }
            None => (
                "-".to_string(),
                format!(
                    "none in a project; {} reads {}",
                    agent.name,
                    agent
                        .user
                        .map_or_else(String::new, |f| shown(Scope::User, f))
                ),
            ),
        };
        let number = if numbered {
            format!("{:>3}. ", i + 1)
        } else {
            "  ".into()
        };
        let _ = writeln!(
            text,
            "{number}{:<12} {detected:<9} {installed:<21} {folder}",
            agent.id
        );
    }
    text
}
