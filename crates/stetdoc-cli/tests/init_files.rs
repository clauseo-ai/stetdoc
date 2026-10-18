//! `stetdoc init` and the files outside the skill folders: the `AGENTS.md` block in a project, and the saved author.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod init_world;

use init_world::{World, stderr, stdout};
use std::fs;
use std::process::Command;

#[test]
fn an_existing_agents_md_keeps_its_content_and_the_block_is_replaced_in_place() {
    let w = World::new();
    let path = w.project.join("AGENTS.md");
    fs::write(&path, "# Project\n\nBuild with make.\n").unwrap();
    w.ok(&["--project", "--agent", "codex"]);
    let once = fs::read_to_string(&path).unwrap();
    assert!(
        once.starts_with("# Project\n\nBuild with make.\n\n<!-- stetdoc:begin -->"),
        "{once}"
    );

    // An old block between other content is replaced where it is, and the rest is kept.
    let old =
        once.replace("Never edit a .docx with other tools.", "An older text.") + "\n## After\n";
    fs::write(&path, &old).unwrap();
    w.ok(&["--project", "--agent", "codex"]);
    let again = fs::read_to_string(&path).unwrap();
    assert_eq!(again, once.clone() + "\n## After\n");
    assert_eq!(again.matches("stetdoc:begin").count(), 1);
}

#[test]
fn unpaired_markers_are_refused_and_nothing_is_written() {
    let w = World::new();
    fs::write(
        w.project.join("AGENTS.md"),
        "<!-- stetdoc:begin -->\nhalf a block\n",
    )
    .unwrap();
    let out = w.init(&["--project", "--agent", "codex"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(!w.project.join(".agents").exists());
}

#[test]
fn markers_inside_a_line_of_text_are_not_a_block() {
    let w = World::new();
    let path = w.project.join("AGENTS.md");
    let prose = "# Project\n\nWe use <!-- stetdoc:begin --> and <!-- stetdoc:end --> as markers.\n";
    fs::write(&path, prose).unwrap();
    w.ok(&["--project", "--agent", "codex"]);
    let with = fs::read_to_string(&path).unwrap();
    assert!(with.starts_with(prose), "{with}");
    assert_eq!(with.matches("<!-- stetdoc:begin -->\n").count(), 1);
    w.ok(&["--project", "--remove"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), prose);
}

#[test]
fn mixed_line_endings_are_kept_as_they_are() {
    let w = World::new();
    let path = w.project.join("AGENTS.md");
    let mixed = "# Project\r\nOne line.\nAnother.\r\n";
    fs::write(&path, mixed).unwrap();
    w.ok(&["--project", "--agent", "codex"]);
    let with = fs::read_to_string(&path).unwrap();
    assert!(with.starts_with(mixed), "{with:?}");
    assert!(with.contains("<!-- stetdoc:end -->\r\n"), "{with:?}");
    w.ok(&["--project", "--remove"]);
    assert_eq!(fs::read_to_string(&path).unwrap(), mixed);
}

#[test]
fn an_agents_md_with_crlf_line_endings_keeps_them() {
    let w = World::new();
    let path = w.project.join("AGENTS.md");
    fs::write(&path, "# Project\r\n\r\nBuild with make.\r\n").unwrap();
    w.ok(&["--project", "--agent", "codex"]);
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("stetdoc skills get core"));
    assert!(
        !text.replace("\r\n", "").contains('\n'),
        "every line ends with CRLF: {text:?}"
    );
    w.ok(&["--project", "--agent", "codex"]);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        text,
        "and a second run changes nothing"
    );
    w.ok(&["--project", "--remove"]);
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "# Project\r\n\r\nBuild with make.\r\n",
        "and remove takes the block out again"
    );
}

#[cfg(unix)]
#[test]
fn a_dangling_agents_md_link_is_refused() {
    let w = World::new();
    std::os::unix::fs::symlink("nowhere.md", w.project.join("AGENTS.md")).unwrap();
    assert_eq!(
        w.init(&["--project", "--agent", "codex"]).status.code(),
        Some(2)
    );
    assert!(!w.project.join("nowhere.md").exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_agents_md_stays_a_link() {
    let w = World::new();
    fs::write(w.project.join("REAL.md"), "# Real\n").unwrap();
    std::os::unix::fs::symlink("REAL.md", w.project.join("AGENTS.md")).unwrap();
    w.ok(&["--project", "--agent", "codex"]);
    let link = w.project.join("AGENTS.md");
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(
        fs::read_to_string(w.project.join("REAL.md"))
            .unwrap()
            .contains("stetdoc skills get core")
    );
}

#[test]
fn author_is_saved_listed_and_used_by_an_edit() {
    let w = World::new();
    let out = w.ok(&["--agent", "codex"]);
    assert!(
        stderr(&out).contains("no author is saved"),
        "{}",
        stderr(&out)
    );

    let dry = w.ok(&["--agent", "codex", "--author", "Mia Chen", "--dry-run"]);
    assert!(stdout(&dry).contains("would save author Mia Chen"));
    let config = w.home.join(".config/stetdoc/config.toml");
    assert!(!config.exists());

    let saved = w.ok(&["--agent", "codex", "--author", "Mia Chen"]);
    assert!(
        stdout(&saved).contains("saved author Mia Chen"),
        "{}",
        stdout(&saved)
    );
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        "author = \"Mia Chen\"\n"
    );
    let list = stdout(&w.ok(&["--list"]));
    assert!(list.contains("author: Mia Chen"), "{list}");
    assert!(w.init(&["--list", "--author", "x"]).status.code() == Some(2));

    // An edit with no --author and no STETDOC_AUTHOR takes the saved name.
    fs::write(
        w.project.join("contract.docx"),
        stetdoc_testkit::fixtures::contract::package(),
    )
    .unwrap();
    let run = |args: &[&str], stdin: &str| {
        let mut child = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
            .args(args)
            .current_dir(&w.project)
            .env("HOME", &w.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("STETDOC_AUTHOR")
            .env("STETDOC_DATE", "2026-10-06T09:30:00")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write as _;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };
    let unpacked = run(&["--json", "unpack", "contract.docx"], "");
    let json: serde_json::Value = serde_json::from_slice(&unpacked.stdout).unwrap();
    let base = json["base"].as_str().unwrap();
    let batch =
        format!("base: {base}\n---\n@replace\nref: p4\nfind: thirty days\nwith: sixty days\n");
    let edited = run(&["--json", "edit", "contract.stet"], &batch);
    assert!(
        edited.status.success(),
        "{}",
        String::from_utf8_lossy(&edited.stdout)
    );
    assert!(String::from_utf8_lossy(&edited.stdout).contains("Mia Chen"));
}

#[test]
fn xdg_config_home_holds_the_config_when_set() {
    let w = World::new();
    let xdg = w.home.join("xdg");
    let out = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(["init", "--agent", "codex", "--author", "Ana"])
        .current_dir(&w.project)
        .env("HOME", &w.home)
        .env("PATH", &w.bin)
        .env("XDG_CONFIG_HOME", &xdg)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(xdg.join("stetdoc/config.toml").is_file());
    assert!(!w.home.join(".config").exists());
}
