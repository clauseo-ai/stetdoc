//! `stetdoc init`: installs the skill stub for the agents you pick, in your home folder (the default) or in a project
//! (`--project`), where it also writes a short block into `AGENTS.md`.
//!
//! The agents and their folders are in `agents.rs`. Several agents read the shared `.agents/skills/` folder, and it is
//! written once however many of them are picked. OpenCode also reads `.claude/skills/`, so picking Claude Code and OpenCode
//! gives OpenCode two identical copies, which is harmless. `--list` shows what is installed, `--remove` deletes only folders
//! that hold nothing but a stetdoc stub, and `--mcp` prints how to register `stetdoc mcp` with each agent without editing
//! any agent's config.

mod agents;
mod block;
mod list;

use crate::cli::InitArgs;
use crate::config;
use crate::fsio;
use crate::output::{Failure, Outcome};
use crate::skill;
use agents::{AGENTS, Agent, Env, SHARED, Scope};
use serde_json::{Value, json};
use std::fmt::Write;
use std::io::{BufRead, IsTerminal};
use std::path::{Path, PathBuf};

/// The largest `AGENTS.md` or `SKILL.md` init will read.
const MAX_FILE: u64 = 1 << 20;

/// Runs `init`.
pub fn run(args: &InitArgs) -> Result<Outcome, Failure> {
    let env = Env::current();
    let scope = if args.project {
        Scope::Project
    } else {
        Scope::User
    };
    let base = base(args, &env, scope)?;
    if args.list {
        return Ok(list::list(&env, scope, &base));
    }
    let picked = choose(args, &env, scope, &base)?;
    if args.remove {
        remove(args, scope, &base, &picked)
    } else {
        install(args, scope, &base, &picked)
    }
}

/// The folder the scope installs into: the home folder, or the project.
fn base(args: &InitArgs, env: &Env, scope: Scope) -> Result<PathBuf, Failure> {
    let base = match scope {
        Scope::Project => args.dir.clone().unwrap_or_else(|| PathBuf::from(".")),
        Scope::User => env.home.clone().ok_or_else(|| {
            Failure::usage(
                "There is no home folder to install into: HOME is not set.",
                "Set HOME, or install into a project with stetdoc init --project.",
            )
        })?,
    };
    if !base.is_dir() {
        return Err(Failure::usage(
            format!("{} is not a folder.", base.display()),
            match scope {
                Scope::Project => {
                    "Run stetdoc init --project in the project's folder, or name it with --dir."
                }
                Scope::User => {
                    "Set HOME to your home folder, or install into a project with --project."
                }
            },
        ));
    }
    Ok(base)
}

/// The agents picked, and whether the shared folder is written because none was detected.
struct Picked {
    agents: Vec<&'static Agent>,
    shared_only: bool,
    named: bool,
    notes: Vec<String>,
}

fn choose(args: &InitArgs, env: &Env, scope: Scope, base: &Path) -> Result<Picked, Failure> {
    let mut picked = Picked {
        agents: Vec::new(),
        shared_only: false,
        named: false,
        notes: Vec::new(),
    };
    if !args.agent.is_empty() {
        picked.agents = by_name(args.agent.iter().map(String::as_str))?;
        picked.named = true;
    } else if args.remove {
        picked.agents = AGENTS.iter().collect();
    } else if interactive() {
        picked.agents = prompt(env, scope, base)?;
    } else {
        picked.agents = AGENTS.iter().filter(|a| env.detects(a)).collect();
    }
    if picked.agents.is_empty() && !args.remove {
        picked.shared_only = true;
        picked.notes.push(format!(
            "note: no agent was detected (none of their commands is on PATH and none of their folders is in the home \
             folder), so init writes the shared folder {}, which Codex, Cursor, OpenCode, Gemini CLI, Copilot and Amp read. \
             Name agents with --agent.",
            shown(scope, SHARED)
        ));
    }
    Ok(picked)
}

/// The agents named by `--agent` or at the prompt: ids, or `all`.
fn by_name<'a>(names: impl Iterator<Item = &'a str>) -> Result<Vec<&'static Agent>, Failure> {
    let mut chosen: Vec<&'static Agent> = Vec::new();
    for name in names.map(str::trim).filter(|n| !n.is_empty()) {
        if name.eq_ignore_ascii_case("all") {
            return Ok(AGENTS.iter().collect());
        }
        let agent = agents::find(&name.to_ascii_lowercase()).ok_or_else(|| {
            let ids: Vec<&str> = AGENTS.iter().map(|a| a.id).collect();
            Failure::usage(
                format!("{name} is not an agent stetdoc installs for."),
                format!("Name one or more of: {}, or all.", ids.join(", ")),
            )
        })?;
        if !chosen.iter().any(|a| a.id == agent.id) {
            chosen.push(agent);
        }
    }
    Ok(chosen)
}

/// Asks in the terminal: numbers or names, `all`, or Enter for the detected agents.
fn prompt(env: &Env, scope: Scope, base: &Path) -> Result<Vec<&'static Agent>, Failure> {
    eprintln!("{}", list::table(env, scope, base, true));
    eprint!(
        "Install for which agents? Numbers or names separated by commas, all, or Enter for the detected ones: "
    );
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).map_err(|e| {
        Failure::io(
            format!("Could not read the answer: {e}."),
            "Run stetdoc init --agent NAME instead.",
        )
    })?;
    if line.trim().is_empty() {
        return Ok(AGENTS.iter().filter(|a| env.detects(a)).collect());
    }
    let mut names = Vec::new();
    for token in line.split(',').map(str::trim).filter(|t| !t.is_empty()) {
        match token.parse::<usize>() {
            Ok(n) if (1..=AGENTS.len()).contains(&n) => {
                names.push(AGENTS.get(n - 1).map_or("", |a| a.id))
            }
            Ok(n) => {
                return Err(Failure::usage(
                    format!("{n} is not a number in the list."),
                    format!(
                        "Answer with numbers from 1 to {}, names, or all.",
                        AGENTS.len()
                    ),
                ));
            }
            Err(_) => names.push(token),
        }
    }
    by_name(names.into_iter())
}

/// The skill folders the picked agents read in a scope, each once, with the agents that read it.
fn folders(
    picked: &Picked,
    scope: Scope,
    notes: &mut Vec<String>,
) -> Vec<(&'static str, Vec<&'static Agent>)> {
    let mut out: Vec<(&'static str, Vec<&'static Agent>)> = Vec::new();
    if picked.shared_only {
        out.push((SHARED, Vec::new()));
    }
    for agent in &picked.agents {
        let Some(folder) = agent.folder(scope) else {
            if picked.named {
                notes.push(format!(
                    "note: {} reads skills only from {}, so there is nothing to do for it in a project. Run stetdoc init \
                     without --project for it.",
                    agent.name,
                    agent.user.map_or_else(String::new, |f| shown(Scope::User, f))
                ));
            }
            continue;
        };
        match out.iter_mut().find(|(f, _)| *f == folder) {
            Some((_, readers)) => readers.push(agent),
            None => out.push((folder, vec![agent])),
        }
    }
    out
}

/// A folder as the output shows it: under `~` for the home folder, relative for a project.
fn shown(scope: Scope, relative: &str) -> String {
    match scope {
        Scope::User => format!("~/{relative}"),
        Scope::Project => relative.to_string(),
    }
}

fn readers(agents: &[&Agent]) -> String {
    if agents.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = agents.iter().map(|a| a.name).collect();
    format!(" ({})", names.join(", "))
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::User => "user",
        Scope::Project => "project",
    }
}

/// Installs the stub into each folder, and in a project the `AGENTS.md` block. A refusal writes nothing.
fn install(
    args: &InitArgs,
    scope: Scope,
    base: &Path,
    picked: &Picked,
) -> Result<Outcome, Failure> {
    let mut notes = picked.notes.clone();
    let stub = skill::stub();
    // (path, text, shown, agents, is a skill)
    let mut plan: Vec<(PathBuf, String, String, String, bool)> = Vec::new();
    let chosen = folders(picked, scope, &mut notes);
    notes.extend(agent_notes(picked, scope, &chosen));
    for (folder, agents) in chosen {
        let path = base.join(folder).join("SKILL.md");
        plan.push((
            path,
            stub.clone(),
            shown(scope, &format!("{folder}/SKILL.md")),
            readers(&agents),
            true,
        ));
    }
    if scope == Scope::Project {
        let agents_md = block::resolve_link(&base.join("AGENTS.md"))?;
        let current = fsio::read_text_if_any(&agents_md, MAX_FILE)?;
        let text = block::with_block(current.as_deref().unwrap_or(""), &agents_md)?;
        plan.push((agents_md, text, "AGENTS.md".into(), String::new(), false));
    }

    // Decide everything before writing anything, so a refusal leaves every folder as it was.
    let mut writes = Vec::new();
    let mut unchanged = Vec::new();
    for (path, text, shown, agents, is_skill) in &plan {
        let linked = *is_skill && is_link(path);
        if linked && !args.force {
            return Err(Failure::usage(
                format!(
                    "{shown} is a symbolic link, so init leaves it and what it points at alone."
                ),
                "Remove the link, or run stetdoc init --force to replace it with the stetdoc stub.",
            ));
        }
        let current = fsio::read_file_if_any(path, MAX_FILE)?;
        if current.as_deref() == Some(text.as_bytes()) {
            unchanged.push((shown, agents));
            continue;
        }
        if *is_skill && !args.force && current.as_deref().is_some_and(|c| !replaceable(c)) {
            return Err(Failure::usage(
                format!(
                    "{shown} exists and is not a stub stetdoc wrote (it may be edited, or your own skill), so init leaves it alone."
                ),
                "Move it aside, or run stetdoc init --force to replace it with the stetdoc stub.",
            ));
        }
        writes.push((path, text, shown, agents));
    }
    for (path, text, _, _) in &writes {
        if !args.dry_run {
            fsio::ensure_dir(&fsio::parent(path))?;
            fsio::write_atomic(path, text.as_bytes())?;
        }
    }

    let (author_line, author_notes, author) = author_step(args)?;
    notes.extend(author_notes);
    let verb = if args.dry_run { "would write" } else { "wrote" };
    let mut text = String::new();
    for (_, _, shown, agents) in &writes {
        let _ = writeln!(text, "{verb} {shown}{agents}");
    }
    for (shown, agents) in &unchanged {
        let _ = writeln!(text, "unchanged {shown}{agents}");
    }
    let mut json = json!({
        "scope": scope_name(scope),
        "root": base.display().to_string(),
        "dry_run": args.dry_run,
        "agents": picked.agents.iter().map(|a| a.id).collect::<Vec<_>>(),
        "written": writes.iter().map(|(_, _, s, _)| s.as_str()).collect::<Vec<_>>(),
        "unchanged": unchanged.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
        "author": author,
    });
    text.push_str(&author_line);
    if args.mcp {
        let _ = writeln!(
            text,
            "\nTo use stetdoc as an MCP server as well, keep the skill installed with it: without the skill, an agent whose MCP server fails to start may edit the .docx's XML itself.\n\
Register `stetdoc mcp` with each agent (init edits no agent's config):"
        );
        let mut mcp = Vec::new();
        for agent in &picked.agents {
            let register = (agent.mcp)(scope);
            let _ = writeln!(text, "  {}: {register}", agent.name);
            mcp.push(json!({ "agent": agent.id, "register": register }));
        }
        if let Some(object) = json.as_object_mut() {
            object.insert("mcp".into(), Value::Array(mcp));
        }
    }
    Ok(Outcome::new(text, json).with_notes(notes))
}

/// What to tell the user about the picked agents: a note of their own, and a second copy for an agent that reads more
/// than one of the folders written.
fn agent_notes(picked: &Picked, scope: Scope, chosen: &[(&str, Vec<&Agent>)]) -> Vec<String> {
    let mut notes = Vec::new();
    for agent in &picked.agents {
        let also = match scope {
            Scope::User => agent.also_user,
            Scope::Project => agent.also_project,
        };
        let reads: Vec<String> = agent
            .folder(scope)
            .into_iter()
            .chain(also.iter().copied())
            .filter(|f| chosen.iter().any(|(c, _)| c == f))
            .map(|f| shown(scope, f))
            .collect();
        if reads.len() > 1 {
            notes.push(format!(
                "note: {} reads {}, so it sees the stub more than once. The copies are identical.",
                agent.name,
                reads.join(" and ")
            ));
        }
        if let Some(note) = (agent.note)(scope) {
            notes.push(format!("note: {note}"));
        }
    }
    notes
}

/// Whether init can ask: stdin and stdout are both a terminal.
fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Saves the author named with `--author`, or asks for one in a terminal when none is saved. Returns the line for the
/// output, notes, and the author for the JSON. Without a terminal and without `--author`, nothing is saved.
fn author_step(args: &InitArgs) -> Result<(String, Vec<String>, Option<String>), Failure> {
    let path = config::path().map_or_else(
        || "the config file".to_string(),
        |p| p.display().to_string(),
    );
    let save = |name: String| -> Result<(String, Vec<String>, Option<String>), Failure> {
        if args.dry_run {
            return Ok((
                format!("would save author {name} to {path}\n"),
                Vec::new(),
                Some(name),
            ));
        }
        let saved = config::save_author(&name)?;
        Ok((
            format!("saved author {name} to {}\n", saved.display()),
            Vec::new(),
            Some(name),
        ))
    };
    if let Some(name) = &args.author {
        let name = name.trim();
        if name.is_empty() {
            return Err(Failure::usage(
                "--author is empty.",
                "Pass your name as Word should show it on your revisions.",
            ));
        }
        return save(name.to_string());
    }
    if let Some(saved) = config::author() {
        return Ok((String::new(), Vec::new(), Some(saved)));
    }
    let hint = "note: no author is saved, and every revision needs one. Run stetdoc init --author \"Your Name\", or set \
                STETDOC_AUTHOR, before you edit."
        .to_string();
    if !interactive() || args.dry_run {
        return Ok((String::new(), vec![hint], None));
    }
    let suggestion = config::suggested_author();
    match &suggestion {
        Some(s) => eprint!("Your name, as Word shows it on your revisions [{s}]: "),
        None => eprint!("Your name, as Word shows it on your revisions: "),
    }
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    let typed = line.trim();
    let name = if typed.is_empty() {
        suggestion
    } else {
        Some(typed.to_string())
    };
    match name {
        Some(name) => save(name),
        None => Ok((String::new(), vec![hint], None)),
    }
}

/// A skill file that init may replace without `--force`: an empty one, or a stub some stetdoc build wrote, unchanged.
fn replaceable(current: &[u8]) -> bool {
    std::str::from_utf8(current).is_ok_and(|text| text.trim().is_empty() || skill::is_ours(text))
}

fn is_link(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
}

/// Removes the folders that hold nothing but a stetdoc stub, and in a project without `--agent` the `AGENTS.md` block. A folder named with
/// `--agent` that stetdoc did not write is refused, and nothing is removed; without `--agent` it is skipped with a note.
fn remove(args: &InitArgs, scope: Scope, base: &Path, picked: &Picked) -> Result<Outcome, Failure> {
    let mut notes = picked.notes.clone();
    let mut doomed = Vec::new();
    for (folder, _) in folders(picked, scope, &mut notes) {
        let dir = base.join(folder);
        if !dir.exists() {
            continue;
        }
        let shown = shown(scope, folder);
        match not_ours(&dir)? {
            None => doomed.push((dir, shown)),
            Some(reason) if picked.named => {
                return Err(Failure::usage(
                    format!("{shown} {reason}, so init does not remove it."),
                    "Remove it by hand if you mean to, and name only the agents whose folder stetdoc wrote.",
                ));
            }
            Some(reason) => notes.push(format!("note: left {shown}: it {reason}.")),
        }
    }
    let mut agents_md = None;
    // The block serves every agent, so it goes only when no agent is named, which removes them all.
    if scope == Scope::Project && !picked.named {
        let path = block::resolve_link(&base.join("AGENTS.md"))?;
        if let Some(current) = fsio::read_text_if_any(&path, MAX_FILE)? {
            agents_md = block::without_block(&current, &path)?.map(|text| (path, text));
        }
    }
    if !args.dry_run {
        for (dir, _) in &doomed {
            fsio::remove_file(&dir.join("SKILL.md"))?;
            fsio::remove_empty_dir(dir)?;
        }
        if let Some((path, text)) = &agents_md {
            fsio::write_atomic(path, text.as_bytes())?;
        }
    }
    let verb = if args.dry_run {
        "would remove"
    } else {
        "removed"
    };
    let mut text = String::new();
    for (_, shown) in &doomed {
        let _ = writeln!(text, "{verb} {shown}");
    }
    if agents_md.is_some() {
        let _ = writeln!(text, "{verb} the stetdoc block from AGENTS.md");
    }
    if text.is_empty() {
        text.push_str("Nothing to remove: no skill folder that stetdoc wrote was found.\n");
    }
    let json = json!({
        "scope": scope_name(scope),
        "root": base.display().to_string(),
        "dry_run": args.dry_run,
        "removed": doomed.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>(),
        "agents_md": agents_md.is_some(),
    });
    Ok(Outcome::new(text, json).with_notes(notes))
}

/// Why a skill folder is not one stetdoc wrote, or `None` when it holds nothing but a stetdoc stub.
fn not_ours(dir: &Path) -> Result<Option<String>, Failure> {
    if !dir.is_dir()
        || dir
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Ok(Some("is not a plain folder".into()));
    }
    let others: Vec<String> = fsio::list_dir(dir)?
        .into_iter()
        .filter(|name| name != "SKILL.md")
        .collect();
    if !others.is_empty() {
        return Ok(Some(format!(
            "holds files stetdoc did not write ({})",
            others.join(", ")
        )));
    }
    let skill = dir.join("SKILL.md");
    if is_link(&skill) {
        return Ok(Some("has a SKILL.md that is a symbolic link".into()));
    }
    let text = fsio::read_file_if_any(&skill, MAX_FILE)?;
    Ok(match text.as_deref().map(std::str::from_utf8) {
        None => Some("has no SKILL.md".into()),
        Some(Ok(text)) if skill::is_ours(text) => None,
        Some(Ok(text)) if skill::names_stetdoc(text) => Some(
            "has a SKILL.md named stetdoc that is not a stub stetdoc wrote, such as an edited stub or your own skill".into(),
        ),
        Some(_) => Some("has a SKILL.md that is not a stetdoc stub".into()),
    })
}
