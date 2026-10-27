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

/// A document whose paragraphs each have run properties out of Word's order, a problem `check` reports and an edit never
/// adds.
fn faulty(paragraphs: usize) -> Vec<u8> {
    let body: String = (0..paragraphs)
        .map(|i| {
            format!("<w:p><w:r><w:rPr><w:i/><w:b/></w:rPr><w:t>Clause {i} text.</w:t></w:r></w:p>")
        })
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
    assert_eq!(
        all.matches("schema_non_canonical_order").count(),
        25,
        "{all}"
    );
    assert!(!all.contains("more;"), "{all}");

    // The edited paragraph's problem is carried once: the runs the edit splits off get their properties in order, so only
    // the run the paragraph had keeps it. One line counts it, and none is listed.
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
            "checked 1 unit. 1 problem was already in the input, and no new one was added: `stetdoc check --all` lists them.\n"
        ),
        "{receipt}"
    );
    assert!(!receipt.contains("schema_non_canonical_order"), "{receipt}");
}

#[test]
fn an_empty_run_property_is_a_note_and_not_a_violation() {
    // The schema allows an empty w:rPr, since every child of it is optional, so check passes with --strict and an edit of the
    // paragraph counts no problem.
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let body = "<w:p><w:r><w:rPr/><w:t>Clause text.</w:t></w:r></w:p>";
    let doc = parts::Doc::new().body(body).bytes();
    fs::write(
        cwd.join("empty.docx"),
        Zip::new(parts::entries_doc(&doc)).build(),
    )
    .unwrap();
    let out = stetdoc(cwd, &["check", "empty.docx", "--strict"], "");
    let check = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{check}");
    assert!(
        check.contains("Validation: no violations in 1 part, 1 note."),
        "{check}"
    );
    let batch = format!(
        "base: {}\n---\n@replace\nref: p1\nfind: text\nwith: words\n",
        base(cwd, "empty.docx")
    );
    let out = stetdoc(cwd, &["edit", "empty.docx", "--dry-run"], &batch);
    let receipt = text(&out.stdout);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!receipt.contains("problem"), "{receipt}");
}

/// The base on the head line of a receipt, if it has one.
fn receipt_base(receipt: &str) -> Option<String> {
    let head = receipt.lines().next()?;
    head.rsplit_once(". base ")
        .map(|(_, b)| b.trim().to_string())
}

#[test]
fn in_place_edits_chain_from_the_receipts_base() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let first = format!(
        "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
        base(cwd, "contract.docx")
    );
    let out = stetdoc(cwd, &["edit", "contract.docx", "--in-place"], &first);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let receipt = text(&out.stdout);
    let next = receipt_base(&receipt).unwrap_or_else(|| panic!("no base: {receipt}"));
    assert_eq!(
        next,
        base(cwd, "contract.docx"),
        "the base is the written file's"
    );

    // A second and a third batch, each from the receipt before it, with no view in between.
    let second = format!("base: {next}\n---\n@comment\nref: p5\ntext: Check the law.\n");
    let out = stetdoc(cwd, &["edit", "contract.docx", "--in-place"], &second);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let next = receipt_base(&text(&out.stdout)).unwrap();
    let third = format!("base: {next}\n---\n@replace\nref: p5\nfind: English\nwith: New York\n");
    let out = stetdoc(
        cwd,
        &["--json", "edit", "contract.docx", "--in-place"],
        &third,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["receipt"]["base"], base(cwd, "contract.docx"));
    let shown = text(&stetdoc(cwd, &["view", "contract.docx", "--range", "p4..p5"], "").stdout);
    assert!(
        shown.contains("{++sixty++}") && shown.contains("{++New York++}"),
        "{shown}"
    );
}

#[test]
fn a_written_file_that_numbers_its_blocks_again_gets_no_base_and_a_next_step() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let batch = format!(
        "base: {}\n---\n@insert\nref: p4\nplace: after\nparagraphs: A new clause.\n",
        base(cwd, "contract.docx")
    );
    let out = stetdoc(
        cwd,
        &["--json", "edit", "contract.docx", "-o", "edited.docx"],
        &batch,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(json["receipt"]["base"].is_null(), "{json}");
    assert!(
        json["next"]
            .as_str()
            .unwrap()
            .starts_with("stetdoc view edited.docx ("),
        "{json}"
    );
    let receipt =
        text(&stetdoc(cwd, &["edit", "contract.docx", "-o", "again.docx"], &batch).stdout);
    assert_eq!(receipt_base(&receipt), None, "{receipt}");
    assert!(
        receipt.contains("\nnext: stetdoc view again.docx ("),
        "{receipt}"
    );
}

/// The files in a folder, without the config folder that a command may create for itself.
fn without_config(dir: &Path) -> Vec<String> {
    names(dir)
        .into_iter()
        .filter(|n| n != "no-config")
        .collect()
}

#[test]
fn a_dry_run_of_a_docx_needs_neither_output_nor_in_place_and_writes_nothing() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let before = fs::read(cwd.join("contract.docx")).unwrap();
    let batch = format!(
        "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
        base(cwd, "contract.docx")
    );
    let out = stetdoc(
        cwd,
        &["edit", "contract.docx", "--dry-run", "--author", "Jane Doe"],
        &batch,
    );
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(
        shown.starts_with("dry run: nothing was applied."),
        "{shown}"
    );
    assert!(shown.contains("{++sixty++}"), "{shown}");
    assert_eq!(fs::read(cwd.join("contract.docx")).unwrap(), before);
    assert_eq!(without_config(cwd), ["contract.docx"]);
}

#[test]
fn a_dry_run_with_output_or_in_place_ignores_it_and_says_so() {
    let batch_for = |cwd: &Path| {
        format!(
            "base: {}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n",
            base(cwd, "contract.docx")
        )
    };
    let flags: [(&[&str], &str); 3] = [
        (&["-o", "edited.docx"], "-o is"),
        (&["--in-place"], "--in-place is"),
        (
            &["-o", "edited.docx", "--in-place"],
            "-o and --in-place are",
        ),
    ];
    for (given, phrase) in flags {
        let tmp = with_contract();
        let cwd = tmp.path();
        let before = fs::read(cwd.join("contract.docx")).unwrap();
        let mut args = vec!["edit", "contract.docx"];
        args.extend_from_slice(given);
        args.extend_from_slice(&["--dry-run", "--author", "Jane Doe"]);
        let out = stetdoc(cwd, &args, &batch_for(cwd));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{given:?}: {}",
            text(&out.stderr)
        );
        let shown = text(&out.stdout);
        let first = shown.lines().next().unwrap_or_default();
        assert_eq!(
            first,
            format!("--dry-run writes nothing, so {phrase} ignored."),
            "{given:?}"
        );
        assert!(shown.contains("dry run: nothing was applied."), "{shown}");
        assert_eq!(fs::read(cwd.join("contract.docx")).unwrap(), before);
        assert_eq!(without_config(cwd), ["contract.docx"], "{given:?}");
    }
}

#[test]
fn replace_and_comment_dry_runs_take_output_or_in_place_and_write_nothing() {
    let commands: [(&str, &[&str]); 2] = [
        ("replace", &["thirty", "sixty"]),
        ("comment", &["thirty", "Check the number."]),
    ];
    for (verb, operands) in commands {
        for given in [&["-o", "edited.docx"][..], &["--in-place"][..]] {
            let tmp = with_contract();
            let cwd = tmp.path();
            let before = fs::read(cwd.join("contract.docx")).unwrap();
            let mut args = vec![verb, "contract.docx"];
            args.extend_from_slice(operands);
            args.extend_from_slice(given);
            args.extend_from_slice(&["--dry-run", "--author", "Jane Doe"]);
            let out = stetdoc(cwd, &args, "");
            assert_eq!(
                out.status.code(),
                Some(0),
                "{verb} {given:?}: {}",
                text(&out.stderr)
            );
            let shown = text(&out.stdout);
            assert!(
                shown.contains("dry run: nothing was applied."),
                "{verb}: {shown}"
            );
            assert_eq!(
                fs::read(cwd.join("contract.docx")).unwrap(),
                before,
                "{verb}"
            );
            assert_eq!(without_config(cwd), ["contract.docx"], "{verb} {given:?}");
        }
    }
}
