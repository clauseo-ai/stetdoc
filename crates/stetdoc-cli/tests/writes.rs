//! Where a command writes: `edit --in-place` back to its .docx, `pack --done` removing the working copy, and what a
//! receipt and `check --all` say about the problems a document already had.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use stetdoc_testkit::fixtures::{contract, parts, zip::Zip};

fn stetdoc(cwd: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env("STETDOC_AUTHOR", "Clauseo")
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run stetdoc");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// The base token of a view's head line.
fn base(cwd: &Path, doc: &str) -> String {
    let view = stetdoc(cwd, &["view", doc, "--range", "p1..p1"], "");
    let head = text(&view.stdout);
    let at = head.find("base=").unwrap() + 5;
    head[at..].split_whitespace().next().unwrap().to_string()
}

fn with_contract() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    tmp
}

#[test]
fn in_place_writes_the_docx_back_and_a_second_batch_builds_on_it() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let first = format!(
        "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
        base(cwd, "contract.docx")
    );
    let out = stetdoc(
        cwd,
        &["--json", "edit", "contract.docx", "--in-place"],
        &first,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stdout));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["output"], "contract.docx");
    assert_eq!(json["in_place"], true);
    let names: Vec<String> = fs::read_dir(cwd)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["contract.docx"], "no temporary file is left");

    // The same redline, updated again: the second batch takes its base from a new view of the written file.
    let second = format!(
        "base: {}\n---\n@replace\nref: p5\nfind: English\nwith: New York\n",
        base(cwd, "contract.docx")
    );
    let out = stetdoc(cwd, &["edit", "contract.docx", "--in-place"], &second);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let view = stetdoc(cwd, &["view", "contract.docx", "--range", "p4..p5"], "");
    let shown = text(&view.stdout);
    assert!(
        shown.contains("{++sixty++}") && shown.contains("{++New York++}"),
        "{shown}"
    );

    // The first batch again names the old namespace, so it is stale and the file is untouched.
    let before = fs::read(cwd.join("contract.docx")).unwrap();
    let out = stetdoc(cwd, &["edit", "contract.docx", "--in-place"], &first);
    assert_eq!(out.status.code(), Some(4), "{}", text(&out.stderr));
    assert_eq!(fs::read(cwd.join("contract.docx")).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn in_place_keeps_the_permissions_and_writes_through_a_link() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let tmp = with_contract();
    let cwd = tmp.path();
    let file = cwd.join("contract.docx");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    symlink("contract.docx", cwd.join("link.docx")).unwrap();
    let batch = format!(
        "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
        base(cwd, "link.docx")
    );
    let out = stetdoc(cwd, &["edit", "link.docx", "--in-place"], &batch);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let link = fs::symlink_metadata(cwd.join("link.docx")).unwrap();
    assert!(link.file_type().is_symlink(), "the link stays a link");
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o640);
    let shown = text(&stetdoc(cwd, &["view", "contract.docx", "--range", "p4..p4"], "").stdout);
    assert!(shown.contains("{++sixty++}"), "{shown}");
}

#[test]
fn pack_done_removes_the_working_copy_and_keeps_what_stetdoc_did_not_write() {
    let tmp = with_contract();
    let cwd = tmp.path();
    assert!(
        stetdoc(cwd, &["unpack", "contract.docx"], "")
            .status
            .success()
    );
    // Files of the user's, one that looks like a temporary file, and stetdoc's own leftover of a crashed write.
    let copy = cwd.join("contract.stet");
    fs::write(copy.join("notes-to-self.txt"), "mine").unwrap();
    fs::write(copy.join(".mynotes.tmp"), "mine").unwrap();
    fs::write(copy.join(".document.md.notes.tmp"), "mine").unwrap();
    fs::write(copy.join(".document.md.4242.tmp"), "stetdoc's").unwrap();
    let out = stetdoc(cwd, &["--json", "pack", "contract.stet", "--done"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["working_copy_removed"], true);
    assert!(cwd.join("contract.edited.docx").is_file());
    assert_eq!(
        names(&copy),
        [
            ".document.md.notes.tmp",
            ".mynotes.tmp",
            "notes-to-self.txt"
        ]
    );

    // With nothing else in it, the folder goes too.
    let tmp = with_contract();
    let cwd = tmp.path();
    assert!(
        stetdoc(cwd, &["unpack", "contract.docx"], "")
            .status
            .success()
    );
    let out = stetdoc(cwd, &["pack", "contract.stet", "--done"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("removed the working copy contract.stet"));
    assert!(!cwd.join("contract.stet").exists());
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn pack_done_keeps_a_file_that_stetdoc_did_not_write_in_the_complement() {
    let tmp = with_contract();
    let cwd = tmp.path();
    assert!(
        stetdoc(cwd, &["unpack", "contract.docx"], "")
            .status
            .success()
    );
    let stet = cwd.join("contract.stet/.stet");
    fs::write(stet.join("x.tmp"), "mine").unwrap();
    let out = stetdoc(cwd, &["pack", "contract.stet", "--done"], "");
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(
        shown.contains("which stays in the folder: .stet/x.tmp"),
        "{shown}"
    );
    assert_eq!(names(&stet), ["x.tmp"]);
    assert_eq!(names(&cwd.join("contract.stet")), [".stet"]);
}

#[cfg(unix)]
#[test]
fn pack_done_through_a_link_removes_the_folder_it_names_and_then_the_link() {
    let tmp = with_contract();
    let cwd = tmp.path();
    assert!(
        stetdoc(cwd, &["unpack", "contract.docx"], "")
            .status
            .success()
    );
    std::os::unix::fs::symlink("contract.stet", cwd.join("link.stet")).unwrap();
    let out = stetdoc(
        cwd,
        &["--json", "pack", "link.stet", "-o", "out.docx", "--done"],
        "",
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stderr).is_empty(), "{}", text(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["working_copy_removed"], true);
    assert!(!cwd.join("contract.stet").exists());
    assert!(cwd.join("link.stet").symlink_metadata().is_err());
    assert!(cwd.join("out.docx").is_file());
}

#[test]
fn in_place_refuses_a_file_that_someone_wrote_during_the_edit() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let file = cwd.join("contract.docx");
    let batch = format!(
        "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
        base(cwd, "contract.docx")
    );
    // stetdoc reads the .docx and then waits for the batch on stdin, so a write in that wait comes during the edit. A
    // write that lands before the read makes the batch stale instead (`stale_ref`): either way exit 4 and the newer file
    // stays, and a few tries make the first case certain in practice.
    let newer = parts::Doc::new()
        .body("<w:p><w:r><w:t>Newer.</w:t></w:r></w:p>")
        .bytes();
    let newer = Zip::new(parts::entries_doc(&newer)).build();
    let mut seen = false;
    for _ in 0..5 {
        fs::write(&file, contract::package()).unwrap();
        let mut child = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
            .args(["edit", "contract.docx", "--in-place"])
            .current_dir(cwd)
            .env("STETDOC_DATE", "2026-10-06T09:30:00")
            .env("STETDOC_AUTHOR", "Clauseo")
            .env("XDG_CONFIG_HOME", cwd.join("no-config"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(400));
        // Through a rename, as an editor saves, so stetdoc never reads half of it.
        let competing = cwd.join(".competing");
        fs::write(&competing, &newer).unwrap();
        fs::rename(&competing, &file).unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(batch.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(4), "{}", text(&out.stderr));
        assert_eq!(fs::read(&file).unwrap(), newer, "the newer file stays");
        assert_eq!(
            names(cwd),
            ["contract.docx", "no-config"]
                .map(String::from)
                .into_iter()
                .filter(|n| cwd.join(n).exists())
                .collect::<Vec<_>>()
        );
        let stderr = text(&out.stderr);
        if stderr.starts_with("error[stale_target]") {
            assert!(
                stderr.contains("contract.docx changed while the edit ran"),
                "{stderr}"
            );
            seen = true;
            break;
        }
    }
    assert!(seen, "a write during the edit is stale_target");
}

#[test]
fn pack_done_keeps_the_working_copy_when_the_pack_fails() {
    let tmp = with_contract();
    let cwd = tmp.path();
    assert!(
        stetdoc(cwd, &["unpack", "contract.docx"], "")
            .status
            .success()
    );
    // A hand edit the pack refuses: a changed structure line.
    let doc = cwd.join("contract.stet/document.md");
    let edited = fs::read_to_string(&doc)
        .unwrap()
        .replace("{table cols=2}", "{table cols=3}");
    fs::write(&doc, &edited).unwrap();
    let out = stetdoc(cwd, &["pack", "contract.stet", "--done"], "");
    assert_ne!(out.status.code(), Some(0));
    assert!(cwd.join("contract.stet/.stet/manifest").is_file());
    assert_eq!(
        fs::read_to_string(&doc).unwrap(),
        edited,
        "the hand edit stays"
    );
    assert!(!cwd.join("contract.edited.docx").exists());
}

/// A document whose paragraphs each have an empty `w:rPr`, a problem `check` reports and an edit never adds.
fn faulty(paragraphs: usize) -> Vec<u8> {
    let body: String = (0..paragraphs)
        .map(|i| format!("<w:p><w:r><w:rPr/><w:t>Clause {i} text.</w:t></w:r></w:p>"))
        .collect();
    let doc = parts::Doc::new().body(&body).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

#[test]
fn a_receipt_counts_the_problems_already_there_and_check_all_lists_them() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    fs::write(cwd.join("faulty.docx"), faulty(25)).unwrap();

    let check = text(&stetdoc(cwd, &["check", "faulty.docx"], "").stdout);
    assert!(check.contains("... 5 more; --all lists them."), "{check}");
    let all = text(&stetdoc(cwd, &["check", "faulty.docx", "--all"], "").stdout);
    assert_eq!(all.matches("empty_properties").count(), 25, "{all}");
    assert!(!all.contains("more;"), "{all}");

    // The edited paragraph's problem is carried, once for each run the edit splits it into: one line counts them, and
    // none is listed.
    let batch = format!(
        "base: {}\n---\n@replace\nref: p1\nfind: text\nwith: words\n",
        base(cwd, "faulty.docx")
    );
    let out = stetdoc(
        cwd,
        &["edit", "faulty.docx", "-o", "edited.docx", "--dry-run"],
        &batch,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let receipt = text(&out.stdout);
    assert!(
        receipt.contains(
            "checked 1 unit. 3 problems were already in the input, and no new one was added: `stetdoc check --all` lists them.\n"
        ),
        "{receipt}"
    );
    assert!(!receipt.contains("empty_properties"), "{receipt}");
}
