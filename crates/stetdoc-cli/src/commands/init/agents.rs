//! The agents `stetdoc init` knows: where each reads skills, how it is detected, and how it registers an MCP server.
//!
//! Detection reads `HOME` and `PATH` only, so a test sets both and sees exactly what it built.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Where an agent reads skills, relative to the home folder (user scope) or the project (project scope).
pub struct Agent {
    /// The name `--agent` takes.
    pub id: &'static str,
    /// The name people know it by.
    pub name: &'static str,
    /// The commands whose presence on `PATH` detects it.
    pub bins: &'static [&'static str],
    /// The folders under the home folder whose presence detects it.
    pub homes: &'static [&'static str],
    /// The skill folder under the home folder, or `None` when the agent has no user scope.
    pub user: Option<&'static str>,
    /// The skill folder under the project, or `None` when the agent has no project scope.
    pub project: Option<&'static str>,
    /// Other folders written by init that it reads too, under the home folder: a second copy of the same stub.
    pub also_user: &'static [&'static str],
    /// The same in a project.
    pub also_project: &'static [&'static str],
    /// Something to tell the user when it is picked, by scope.
    pub note: fn(Scope) -> Option<&'static str>,
    /// How to register `stetdoc mcp` with it, by scope.
    pub mcp: fn(Scope) -> String,
}

/// The two places init installs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// The home folder.
    User,
    /// A project folder.
    Project,
}

/// The folder several agents share, the Agent Skills convention, under the home folder and in a project alike.
pub const SHARED: &str = ".agents/skills/stetdoc";

/// Where a Claude Code user without the plugin finds its guard hook, which blocks other ways into a .docx.
const GUARD_NOTE: &str = "Claude Code can also block other ways into a .docx, such as unzip or python-docx, with the guard hook the stetdoc plugin carries. To add it to this install, see https://github.com/clauseo-ai/stetdoc/blob/main/docs/agents.md#the-claude-code-guard-hook";

/// What a Gemini CLI user is told: Google retired it, and Antigravity CLI replaces it.
const GEMINI_RETIRED: &str = "Google retired Gemini CLI on 18 June 2026. If you have moved to Antigravity CLI (agy), install for it with --agent antigravity.";

const SERVER_JSON: &str = r#"{ "command": "stetdoc", "args": ["mcp"] }"#;

/// Every agent, in the order `--list` and the prompt show them.
pub const AGENTS: [Agent; 13] = [
    Agent {
        id: "claude-code",
        name: "Claude Code",
        bins: &["claude"],
        homes: &[".claude"],
        user: Some(".claude/skills/stetdoc"),
        project: Some(".claude/skills/stetdoc"),
        also_user: &[],
        also_project: &[],
        note: |_| Some(GUARD_NOTE),
        mcp: |scope| match scope {
            Scope::User => "claude mcp add --scope user stetdoc -- stetdoc mcp".into(),
            Scope::Project => "claude mcp add --scope project stetdoc -- stetdoc mcp".into(),
        },
    },
    Agent {
        id: "codex",
        name: "Codex",
        bins: &["codex"],
        homes: &[".codex"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[],
        also_project: &[],
        note: |_| None,
        mcp: |_| "codex mcp add stetdoc -- stetdoc mcp".into(),
    },
    Agent {
        id: "cursor",
        name: "Cursor",
        bins: &["cursor-agent", "cursor"],
        homes: &[".cursor"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[".claude/skills/stetdoc"],
        also_project: &[".claude/skills/stetdoc"],
        note: |_| None,
        mcp: |scope| {
            let file = pick(scope, "~/.cursor/mcp.json", ".cursor/mcp.json");
            format!(r#"in {file}: {{ "mcpServers": {{ "stetdoc": {SERVER_JSON} }} }}"#)
        },
    },
    Agent {
        id: "opencode",
        name: "OpenCode",
        bins: &["opencode"],
        homes: &[".config/opencode"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[".claude/skills/stetdoc"],
        also_project: &[".claude/skills/stetdoc"],
        note: |_| None,
        mcp: |scope| {
            let file = pick(scope, "~/.config/opencode/opencode.json", "opencode.json");
            format!(
                r#"in {file}: {{ "mcp": {{ "stetdoc": {{ "type": "local", "command": ["stetdoc", "mcp"], "enabled": true }} }} }}"#
            )
        },
    },
    Agent {
        id: "openclaw",
        name: "OpenClaw",
        bins: &["openclaw"],
        homes: &[".openclaw"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[],
        also_project: &[],
        note: |scope| {
            (scope == Scope::User).then_some("OpenClaw reads ~/.agents/skills in its default setup; if you changed its skill folders, point one at it.")
        },
        mcp: |_| {
            format!(
                r#"openclaw mcp add stetdoc --command stetdoc --arg mcp (or in ~/.openclaw/openclaw.json: {{ "mcp": {{ "servers": {{ "stetdoc": {SERVER_JSON} }} }} }})"#
            )
        },
    },
    Agent {
        id: "gemini",
        name: "Gemini CLI",
        bins: &["gemini"],
        homes: &[".gemini"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[],
        also_project: &[],
        note: |_| Some(GEMINI_RETIRED),
        mcp: |scope| {
            let file = pick(scope, "~/.gemini/settings.json", ".gemini/settings.json");
            format!(r#"in {file}: {{ "mcpServers": {{ "stetdoc": {SERVER_JSON} }} }}"#)
        },
    },
    Agent {
        id: "antigravity",
        name: "Antigravity CLI",
        bins: &["agy"],
        homes: &[".gemini/antigravity-cli", ".gemini/antigravity"],
        user: Some(".gemini/config/skills/stetdoc"),
        project: Some(SHARED),
        also_user: &[],
        also_project: &[],
        note: |_| None,
        // Antigravity loads MCP servers from the global file only: it reads a workspace's .agents/mcp_config.json and
        // ignores it (google-antigravity/antigravity-cli#60), so both scopes point at the global file.
        mcp: |_| {
            format!(
                r#"agy mcp add stetdoc stetdoc mcp (or in ~/.gemini/config/mcp_config.json: {{ "mcpServers": {{ "stetdoc": {SERVER_JSON} }} }})"#
            )
        },
    },
    Agent {
        id: "copilot",
        name: "GitHub Copilot CLI",
        bins: &["copilot"],
        homes: &[".copilot"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[".claude/skills/stetdoc"],
        also_project: &[".claude/skills/stetdoc"],
        note: |_| None,
        mcp: |_| {
            r#"in ~/.copilot/mcp-config.json: { "mcpServers": { "stetdoc": { "type": "local", "command": "stetdoc", "args": ["mcp"], "tools": ["*"] } } }"#.into()
        },
    },
    Agent {
        id: "amp",
        name: "Amp",
        bins: &["amp"],
        homes: &[".config/amp"],
        user: Some(SHARED),
        project: Some(SHARED),
        also_user: &[],
        also_project: &[],
        note: |_| None,
        mcp: |_| "amp mcp add stetdoc -- stetdoc mcp".into(),
    },
    Agent {
        id: "hermes",
        name: "Hermes Agent",
        bins: &["hermes"],
        homes: &[".hermes"],
        user: Some(".hermes/skills/stetdoc"),
        project: Some(".hermes/skills/stetdoc"),
        also_user: &[],
        also_project: &[],
        note: |_| {
            Some(
                "Hermes Agent also reads .agents/skills once you run `hermes skills trust` for it.",
            )
        },
        mcp: |_| {
            "in ~/.hermes/config.yaml: mcp_servers: { stetdoc: { command: stetdoc, args: [mcp] } }"
                .into()
        },
    },
    Agent {
        id: "grok",
        name: "Grok Build",
        bins: &["grok"],
        homes: &[".grok"],
        user: Some(".grok/skills/stetdoc"),
        project: Some(".grok/skills/stetdoc"),
        also_user: &[],
        also_project: &[],
        note: |_| None,
        mcp: |_| "grok mcp add stetdoc -- stetdoc mcp".into(),
    },
    Agent {
        id: "cmx2",
        name: "cmx2",
        bins: &["cmx2"],
        homes: &[".config/cmx2"],
        user: Some(".config/cmx2/skills/stetdoc"),
        project: Some(".cmx2/skills/stetdoc"),
        also_user: &[SHARED, ".claude/skills/stetdoc"],
        also_project: &[],
        note: |_| None,
        mcp: |_| {
            r#"in ~/.config/cmx2/cmx2.jsonc: "mcp": { "stetdoc": { "command": ["stetdoc", "mcp"] } }"#.into()
        },
    },
    Agent {
        id: "factory",
        name: "Factory Droid",
        bins: &["droid"],
        homes: &[".factory"],
        user: Some(".factory/skills/stetdoc"),
        project: Some(".factory/skills/stetdoc"),
        also_user: &[SHARED],
        also_project: &[SHARED],
        note: |_| None,
        mcp: |scope| {
            let file = pick(scope, "~/.factory/mcp.json", ".factory/mcp.json");
            format!(
                r#"in {file}: {{ "mcpServers": {{ "stetdoc": {{ "type": "stdio", "command": "stetdoc", "args": ["mcp"] }} }} }}"#
            )
        },
    },
];

fn pick(scope: Scope, user: &'static str, project: &'static str) -> &'static str {
    match scope {
        Scope::User => user,
        Scope::Project => project,
    }
}

impl Agent {
    /// The agent's skill folder in a scope, relative to the home folder or the project.
    pub fn folder(&self, scope: Scope) -> Option<&'static str> {
        match scope {
            Scope::User => self.user,
            Scope::Project => self.project,
        }
    }
}

/// The agent with this id.
pub fn find(id: &str) -> Option<&'static Agent> {
    AGENTS.iter().find(|a| a.id == id)
}

/// What detection reads: the home folder and the directories of `PATH`.
pub struct Env {
    /// The home folder, when `HOME` (or `USERPROFILE`) names one.
    pub home: Option<PathBuf>,
    /// The directories of `PATH`.
    pub path: Vec<PathBuf>,
}

impl Env {
    /// The process's environment.
    pub fn current() -> Env {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .filter(|h| !h.is_empty())
            .map(PathBuf::from);
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        Env { home, path }
    }

    /// Whether the agent's command is on `PATH` or its config folder is in the home folder.
    pub fn detects(&self, agent: &Agent) -> bool {
        let on_path = agent
            .bins
            .iter()
            .any(|bin| self.path.iter().any(|dir| is_command(dir, bin)));
        let has_home = self
            .home
            .as_deref()
            .is_some_and(|home| agent.homes.iter().any(|h| home.join(h).is_dir()));
        on_path || has_home
    }
}

fn is_command(dir: &Path, bin: &str) -> bool {
    let mut names = vec![OsString::from(bin)];
    if cfg!(windows) {
        names.extend([".exe", ".cmd", ".bat"].map(|ext| OsString::from(format!("{bin}{ext}"))));
    }
    names.iter().any(|name| {
        let path = dir.join(name);
        path.is_file() && executable(&path)
    })
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(_path: &Path) -> bool {
    true
}
