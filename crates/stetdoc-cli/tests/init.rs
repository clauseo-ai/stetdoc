//! `stetdoc init` with a made-up home folder and `PATH`: the scopes, the agents it detects, the shared folder written once,
//! `--list`, `--remove`, `--mcp`, and an `AGENTS.md` that already has content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod init_world;

use init_world::{World, skill, stderr, stdout, stub};
use std::fs;
use std::path::Path;

#[test]
fn the_user_scope_installs_for_the_detected_agents_in_the_home_folder() {
    let w = World::new();
    fs::create_dir(w.home.join(".claude")).unwrap();
    w.command("codex");
    let out = w.ok(&[]);
    assert_eq!(
        fs::read_to_string(skill(&w.home, ".claude")).unwrap(),
        stub()
    );
    assert_eq!(
        fs::read_to_string(skill(&w.home, ".agents")).unwrap(),
        stub()
    );
    for folder in [".grok", ".factory", ".hermes", ".config/cmx2"] {
        assert!(!w.home.join(folder).exists(), "{folder} was not detected");
    }
    assert!(
        fs::read_dir(&w.project).unwrap().next().is_none(),
        "the user scope leaves the project alone, AGENTS.md included"
    );
    let text = stdout(&out);
    assert!(
        text.contains("wrote ~/.claude/skills/stetdoc/SKILL.md (Claude Code)"),
        "{text}"
    );
    assert!(
        text.contains("wrote ~/.agents/skills/stetdoc/SKILL.md (Codex)"),
        "{text}"
    );
}

#[test]
fn with_no_agent_detected_the_shared_folder_is_written_and_said() {
    let w = World::new();
    let out = w.ok(&[]);
    assert!(skill(&w.home, ".agents").is_file());
    assert_eq!(
        fs::read_dir(&w.home).unwrap().count(),
        1,
        "only .agents is written"
    );
    assert!(
        stderr(&out).contains("no agent was detected"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_folder_several_agents_share_is_written_once() {
    let w = World::new();
    let out = w.ok(&["--agent", "codex,cursor", "--agent", "opencode", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        json["written"],
        serde_json::json!(["~/.agents/skills/stetdoc/SKILL.md"])
    );
    assert_eq!(
        json["agents"],
        serde_json::json!(["codex", "cursor", "opencode"])
    );
    let text = stdout(&w.ok(&["--agent", "codex,cursor,opencode"]));
    assert_eq!(
        text.trim(),
        "unchanged ~/.agents/skills/stetdoc/SKILL.md (Codex, Cursor, OpenCode)"
    );
}

#[test]
fn claude_code_and_opencode_both_get_their_folder() {
    let w = World::new();
    let out = w.ok(&["--agent", "claude-code,opencode"]);
    assert!(skill(&w.home, ".claude").is_file());
    assert!(skill(&w.home, ".agents").is_file());
    assert!(
        stderr(&out)
            .contains("OpenCode reads ~/.agents/skills/stetdoc and ~/.claude/skills/stetdoc")
    );
}

#[test]
fn the_user_folders_of_cmx2_hermes_and_grok() {
    let w = World::new();
    w.ok(&["--agent", "cmx2,hermes,grok,factory"]);
    for folder in [".config/cmx2", ".hermes", ".grok", ".factory"] {
        assert!(skill(&w.home, folder).is_file(), "{folder}");
    }
}

#[test]
fn the_project_scope_writes_the_project_and_agents_md() {
    let w = World::new();
    let out = w.ok(&["--project", "--agent", "claude-code,cmx2,grok,codex"]);
    for folder in [".claude", ".cmx2", ".agents"] {
        assert_eq!(
            fs::read_to_string(skill(&w.project, folder)).unwrap(),
            stub()
        );
    }
    assert!(skill(&w.project, ".grok").is_file(), "{}", stderr(&out));
    let agents = fs::read_to_string(w.project.join("AGENTS.md")).unwrap();
    assert!(agents.contains("stetdoc skills get core"));
    assert!(
        fs::read_dir(&w.home).unwrap().next().is_none(),
        "the project scope leaves the home folder alone"
    );
}

#[test]
fn dir_names_the_project() {
    let w = World::new();
    let other = w.project.join("sub");
    fs::create_dir(&other).unwrap();
    w.ok(&[
        "--project",
        "--dir",
        other.to_str().unwrap(),
        "--agent",
        "codex",
    ]);
    assert!(skill(&other, ".agents").is_file());
    assert_eq!(
        w.init(&["--dir", "x"]).status.code(),
        Some(2),
        "--dir needs --project"
    );
}

#[test]
fn an_unknown_agent_is_refused_and_names_the_known_ones() {
    let w = World::new();
    let out = w.init(&["--agent", "nobody"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("claude-code"), "{}", stderr(&out));
    assert!(fs::read_dir(&w.home).unwrap().next().is_none());
}

#[test]
fn dry_run_writes_nothing() {
    let w = World::new();
    let out = w.ok(&["--agent", "all", "--dry-run"]);
    assert!(stdout(&out).contains("would write ~/.claude/skills/stetdoc/SKILL.md"));
    assert!(fs::read_dir(&w.home).unwrap().next().is_none());
}

#[test]
fn list_shows_detection_and_the_installed_version() {
    let w = World::new();
    w.command("droid");
    w.ok(&["--agent", "claude-code"]);
    let other = skill(&w.home, ".grok");
    fs::create_dir_all(other.parent().unwrap()).unwrap();
    fs::write(&other, "---\nname: something-else\n---\n").unwrap();

    let out = w.ok(&["--list", "--json"]);
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let agent = |id: &str| {
        json["agents"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(json["scope"], "user");
    assert_eq!(agent("claude-code")["installed"], true);
    assert_eq!(agent("claude-code")["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(agent("hermes")["detected"], false);
    assert_eq!(agent("factory")["detected"], true, "droid is on PATH");
    assert_eq!(agent("grok")["installed"], false, "another skill");
    assert_eq!(agent("codex")["folder"], "~/.agents/skills/stetdoc");

    let text = stdout(&w.ok(&["--list"]));
    assert!(text.contains("claude-code"), "{text}");
    assert!(text.contains("another skill"), "{text}");
    let project = stdout(&w.ok(&["--list", "--project"]));
    assert!(project.contains(".hermes/skills/stetdoc"), "{project}");
    assert!(project.contains(".cmx2/skills/stetdoc"), "{project}");
}

#[test]
fn remove_deletes_only_what_stetdoc_wrote() {
    let w = World::new();
    w.ok(&["--project", "--agent", "claude-code,codex"]);
    fs::write(w.project.join("AGENTS.md"), "# Project\n").unwrap();
    w.ok(&["--project", "--agent", "claude-code,codex"]);

    // A folder holding something else, and one whose SKILL.md is not stetdoc's, are left alone.
    let mine = skill(&w.project, ".grok");
    fs::create_dir_all(mine.parent().unwrap()).unwrap();
    fs::write(&mine, "---\nname: stetdoc-fork\n---\nMine.\n").unwrap();
    fs::create_dir_all(w.project.join(".factory/skills/stetdoc")).unwrap();
    fs::write(skill(&w.project, ".factory"), stub()).unwrap();
    fs::write(w.project.join(".factory/skills/stetdoc/notes.txt"), "mine").unwrap();

    let dry = w.ok(&["--project", "--remove", "--dry-run"]);
    assert!(stdout(&dry).contains("would remove .claude/skills/stetdoc"));
    assert!(skill(&w.project, ".claude").is_file());

    let out = w.ok(&["--project", "--remove"]);
    assert!(!w.project.join(".claude/skills/stetdoc").exists());
    assert!(!w.project.join(".agents/skills/stetdoc").exists());
    assert!(
        w.project.join(".claude/skills").is_dir(),
        "the parent folder stays"
    );
    assert_eq!(
        fs::read_to_string(&mine).unwrap(),
        "---\nname: stetdoc-fork\n---\nMine.\n"
    );
    assert!(
        w.project
            .join(".factory/skills/stetdoc/notes.txt")
            .is_file()
    );
    assert!(
        stderr(&out).contains("left .grok/skills/stetdoc"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        fs::read_to_string(w.project.join("AGENTS.md")).unwrap(),
        "# Project\n"
    );
}

#[test]
fn remove_refuses_a_named_folder_that_stetdoc_did_not_write() {
    let w = World::new();
    w.ok(&["--agent", "codex"]);
    let theirs = skill(&w.home, ".claude");
    fs::create_dir_all(theirs.parent().unwrap()).unwrap();
    fs::write(&theirs, "# Not a stub\n").unwrap();
    let out = w.init(&["--remove", "--agent", "codex,claude-code"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("not a stetdoc stub"),
        "{}",
        stderr(&out)
    );
    assert_eq!(fs::read_to_string(&theirs).unwrap(), "# Not a stub\n");
    assert!(
        skill(&w.home, ".agents").is_file(),
        "a refusal removes nothing"
    );
}

#[test]
fn mcp_prints_how_to_register_the_server_and_edits_nothing() {
    let w = World::new();
    let out = w.ok(&[
        "--agent",
        "claude-code,codex,cursor,opencode,hermes",
        "--mcp",
    ]);
    let text = stdout(&out);
    for line in [
        "claude mcp add --scope user stetdoc -- stetdoc mcp",
        "codex mcp add stetdoc -- stetdoc mcp",
        "~/.cursor/mcp.json",
        "~/.config/opencode/opencode.json",
        "mcp_servers",
        "keep the skill installed with it",
    ] {
        assert!(text.contains(line), "{line} in {text}");
    }
    assert!(!w.home.join(".cursor").exists() && !w.home.join(".config/opencode").exists());
}

#[test]
fn a_skill_md_that_is_not_a_stetdoc_stub_is_refused_unless_forced() {
    let w = World::new();
    let path = skill(&w.project, ".agents");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "---\nname: something-else\n---\nMine.\n").unwrap();
    let out = w.init(&["--project", "--agent", "codex,cmx2"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("--force"));
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "---\nname: something-else\n---\nMine.\n"
    );
    assert!(
        !w.project.join(".cmx2").exists(),
        "a refusal writes nothing"
    );
    assert!(
        !w.project.join("AGENTS.md").exists(),
        "a refusal writes nothing"
    );

    w.ok(&["--project", "--agent", "codex,cmx2", "--force"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), stub());
}

#[test]
fn an_empty_skill_md_or_an_earlier_stub_is_replaced() {
    let w = World::new();
    let agents = skill(&w.project, ".agents");
    let cmx2 = skill(&w.project, ".cmx2");
    let claude = skill(&w.project, ".claude");
    let grok = skill(&w.project, ".grok");
    for path in [&agents, &cmx2, &claude, &grok] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    fs::write(&agents, "").unwrap();
    let earlier =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/earlier/stub-1.md"))
            .unwrap();
    fs::write(&cmx2, earlier).unwrap();
    // A stub of another version differs only on its version line.
    fs::write(&claude, stub().replace(env!("CARGO_PKG_VERSION"), "0.0.1")).unwrap();
    // An earlier stub with a version line is matched with any version on it.
    let versioned =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/earlier/stub-2.md"))
            .unwrap();
    fs::write(&grok, versioned.replace("{{version}}", "0.9.0")).unwrap();
    w.ok(&["--project", "--agent", "codex,cmx2,claude-code,grok"]);
    for path in [&agents, &cmx2, &claude, &grok] {
        assert_eq!(fs::read_to_string(path).unwrap(), stub());
    }
    // So is the stub that sent agents with MCP tools to `skills_get` before it carried the everyday commands.
    let previous =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("skill/earlier/stub-3.md"))
            .unwrap();
    fs::write(&grok, previous.replace("{{version}}", "1.0.0-dev")).unwrap();
    w.ok(&["--project", "--agent", "grok"]);
    assert_eq!(fs::read_to_string(&grok).unwrap(), stub());
}

#[test]
fn an_edited_stub_or_a_users_skill_named_stetdoc_is_never_replaced_or_removed() {
    let w = World::new();
    let edited = skill(&w.home, ".claude");
    let own = skill(&w.home, ".agents");
    for path in [&edited, &own] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    let edited_text = stub().replace("and stop.", "and stop. My own note.");
    let own_text = "---\nname: stetdoc\ndescription: my notes\n---\nPrivate.\n";
    fs::write(&edited, &edited_text).unwrap();
    fs::write(&own, own_text).unwrap();

    for agent in ["claude-code", "codex"] {
        let out = w.init(&["--agent", agent]);
        assert_eq!(out.status.code(), Some(2), "{agent}");
        assert!(stderr(&out).contains("--force"));
    }
    let named = w.init(&["--remove", "--agent", "claude-code"]);
    assert_eq!(named.status.code(), Some(2));
    assert!(
        stderr(&named).contains("named stetdoc"),
        "{}",
        stderr(&named)
    );
    let all = w.ok(&["--remove"]);
    assert!(
        stderr(&all).contains("left ~/.claude/skills/stetdoc"),
        "{}",
        stderr(&all)
    );
    assert!(
        stderr(&all).contains("left ~/.agents/skills/stetdoc"),
        "{}",
        stderr(&all)
    );
    assert_eq!(fs::read_to_string(&edited).unwrap(), edited_text);
    assert_eq!(fs::read_to_string(&own).unwrap(), own_text);

    let list = stdout(&w.ok(&["--list"]));
    assert!(list.contains("edited or your own"), "{list}");

    w.ok(&["--agent", "claude-code", "--force"]);
    assert_eq!(fs::read_to_string(&edited).unwrap(), stub());
}

#[cfg(unix)]
#[test]
fn a_skill_md_that_is_a_link_is_left_alone_unless_forced() {
    let w = World::new();
    let target = w.home.join("elsewhere.md");
    fs::write(&target, stub()).unwrap();
    let link = skill(&w.home, ".claude");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let out = w.init(&["--agent", "claude-code"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("symbolic link"), "{}", stderr(&out));
    let remove = w.init(&["--remove", "--agent", "claude-code"]);
    assert_eq!(remove.status.code(), Some(2));
    assert!(target.is_file() && link.symlink_metadata().is_ok());
}

#[test]
fn an_agent_that_reads_two_written_folders_is_told_and_agents_say_what_they_need() {
    let w = World::new();
    let out = w.ok(&[
        "--agent",
        "claude-code,cursor,cmx2,hermes,openclaw,grok",
        "--mcp",
    ]);
    let notes = stderr(&out);
    assert!(
        notes.contains("Cursor reads ~/.agents/skills/stetdoc and ~/.claude/skills/stetdoc"),
        "{notes}"
    );
    assert!(notes.contains("cmx2 reads ~/.config/cmx2/skills/stetdoc and ~/.agents/skills/stetdoc and ~/.claude/skills/stetdoc"), "{notes}");
    assert!(notes.contains("hermes skills trust"), "{notes}");
    assert!(
        notes.contains("OpenClaw reads ~/.agents/skills in its default setup"),
        "{notes}"
    );
    let text = stdout(&out);
    assert!(
        text.contains("grok mcp add stetdoc -- stetdoc mcp"),
        "{text}"
    );
    assert!(
        text.contains("openclaw mcp add stetdoc --command stetdoc --arg mcp"),
        "{text}"
    );
    let project = w.ok(&["--project", "--agent", "hermes"]);
    assert!(
        skill(&w.project, ".hermes").is_file(),
        "{}",
        stderr(&project)
    );
}
