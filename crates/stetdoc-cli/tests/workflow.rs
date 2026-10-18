//! The working-copy loop with hand edits, which a `trycmd` session cannot make: edit a readable file, check, pack, and the
//! refusals around it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use stetdoc_testkit::fixtures::contract;

fn stetdoc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env_remove("STETDOC_AUTHOR")
        // No saved author either: a config file of the person running the tests must not count.
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .output()
        .expect("run stetdoc")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}

fn unpacked() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let out = stetdoc(tmp.path(), &["unpack", "contract.docx"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    tmp
}

fn hand_edit(dir: &Path, from: &str, to: &str) {
    let path = dir.join("contract.stet/document.md");
    let doc = fs::read_to_string(&path).unwrap();
    assert!(doc.contains(from), "{doc}");
    fs::write(&path, doc.replace(from, to)).unwrap();
}

#[test]
fn a_hand_edit_is_checked_then_packed_as_a_tracked_change() {
    let tmp = unpacked();
    let cwd = tmp.path();
    hand_edit(cwd, "thirty days", "sixty days");

    let view = stetdoc(cwd, &["view", "contract.stet", "--range", "p4..p4"]);
    assert!(view.status.success());
    assert!(
        text(&view.stdout).contains("thirty"),
        "a view shows the last export"
    );
    assert!(text(&view.stderr).contains("not packed"), "and says so");

    let check = stetdoc(cwd, &["check", "contract.stet"]);
    assert_eq!(check.status.code(), Some(2), "pending files need an author");

    let check = stetdoc(cwd, &["check", "contract.stet", "--author", "Clauseo"]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check.stderr));
    let shown = text(&check.stdout);
    assert!(
        shown.contains("Pending edits") && shown.contains("{++sixty++}"),
        "{shown}"
    );

    let batch = stetdoc(
        cwd,
        &["edit", "contract.stet", "--author", "Clauseo", "--ops", "-"],
    );
    assert_eq!(
        batch.status.code(),
        Some(4),
        "a batch waits for the hand edits: pending_edits"
    );

    let pack = stetdoc(
        cwd,
        &["--json", "pack", "contract.stet", "--author", "Clauseo"],
    );
    assert_eq!(pack.status.code(), Some(0), "{}", text(&pack.stdout));
    let json: serde_json::Value = serde_json::from_slice(&pack.stdout).unwrap();
    assert_eq!(json["output"], "contract.edited.docx");
    assert_eq!(json["saved"]["edited"], true);

    let after = fs::read_to_string(cwd.join("contract.stet/document.md")).unwrap();
    assert!(
        after.contains("{++sixty++}"),
        "the working copy shows the packed revision: {after}"
    );
    let check = stetdoc(cwd, &["check", "contract.stet"]);
    assert!(
        text(&check.stdout).contains("No pending edits"),
        "and is clean again"
    );

    let packed = stetdoc(
        cwd,
        &[
            "view",
            "contract.edited.docx",
            "--range",
            "p4..p4",
            "--mode",
            "final",
        ],
    );
    assert!(text(&packed.stdout).contains("sixty days"));
    assert!(
        cwd.join("contract.docx").is_file(),
        "the original is never written over"
    );
    assert_eq!(
        fs::read(cwd.join("contract.docx")).unwrap(),
        contract::package()
    );
}

#[test]
fn a_readable_file_that_was_never_written_is_refused() {
    let tmp = unpacked();
    let cwd = tmp.path();
    fs::write(
        cwd.join("contract.stet/notes.md"),
        "A footnote typed by hand.\n",
    )
    .unwrap();
    let check = stetdoc(cwd, &["check", "contract.stet", "--author", "Clauseo"]);
    assert_eq!(
        check.status.code(),
        Some(1),
        "a file that was never written is unsupported"
    );
    assert!(
        text(&check.stderr).starts_with("error[unsupported]"),
        "{}",
        text(&check.stderr)
    );
}

#[test]
fn a_command_waits_while_another_process_holds_the_working_copy() {
    let tmp = unpacked();
    let cwd = tmp.path();
    // This test process takes the lock that stetdoc takes, as another stetdoc would.
    let lock = fs::OpenOptions::new()
        .write(true)
        .open(cwd.join("contract.stet/.stet/lock"))
        .unwrap();
    lock.lock().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(["view", "contract.stet"])
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(700));
    assert!(
        child.try_wait().unwrap().is_none(),
        "stetdoc must wait while another process holds the lock"
    );
    lock.unlock().unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("p4 Payment is due"));
}

#[test]
fn a_readable_file_that_is_not_utf8_is_set_aside_by_a_refresh() {
    let tmp = unpacked();
    let cwd = tmp.path();
    let doc = cwd.join("contract.stet/document.md");
    fs::write(&doc, b"p1 \xff\xfe not text\n").unwrap();
    let view = stetdoc(cwd, &["view", "contract.stet"]);
    assert_eq!(view.status.code(), Some(2));
    assert!(
        text(&view.stderr).contains("--refresh"),
        "{}",
        text(&view.stderr)
    );

    let refresh = stetdoc(cwd, &["unpack", "contract.stet", "--refresh"]);
    assert!(refresh.status.success(), "{}", text(&refresh.stderr));
    assert!(text(&refresh.stderr).contains(".stale"));
    let saved: Vec<_> = fs::read_dir(cwd.join("contract.stet"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".stale"))
        .collect();
    assert_eq!(saved.len(), 1);
    assert_eq!(
        fs::read(saved[0].path()).unwrap(),
        b"p1 \xff\xfe not text\n"
    );
    assert!(
        fs::read_to_string(&doc)
            .unwrap()
            .contains("p4 Payment is due")
    );
    assert!(stetdoc(cwd, &["view", "contract.stet"]).status.success());
}

#[test]
fn an_output_that_is_an_input_or_inside_the_working_copy_is_refused() {
    let tmp = unpacked();
    let cwd = tmp.path();
    let original = fs::read(cwd.join("contract.docx")).unwrap();
    let cases: [&[&str]; 4] = [
        &[
            "edit",
            "contract.docx",
            "-o",
            "contract.docx",
            "--author",
            "A",
            "--ops",
            "-",
        ],
        &[
            "edit",
            "contract.docx",
            "-o",
            "./sub/../contract.docx",
            "--author",
            "A",
            "--ops",
            "-",
        ],
        &[
            "pack",
            "contract.stet",
            "-o",
            "contract.stet/out.docx",
            "--author",
            "A",
        ],
        &[
            "compare",
            "contract.docx",
            "contract.docx",
            "-o",
            "contract.docx",
            "--author",
            "A",
        ],
    ];
    fs::create_dir(cwd.join("sub")).unwrap();
    for args in cases {
        let out = stetdoc(cwd, args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
    assert_eq!(fs::read(cwd.join("contract.docx")).unwrap(), original);
    assert!(!cwd.join("contract.stet/out.docx").exists());
}

#[test]
fn pack_overwrites_only_the_original_or_its_own_output_unless_forced() {
    let tmp = unpacked();
    let cwd = tmp.path();
    hand_edit(cwd, "thirty days", "sixty days");
    let pack = |extra: &[&str]| {
        let mut args = vec!["pack", "contract.stet", "--author", "Clauseo"];
        args.extend_from_slice(extra);
        stetdoc(cwd, &args)
    };
    // The document the working copy came from may be packed over, as an agent's flow does.
    let out = pack(&["-o", "contract.docx"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let packed = fs::read(cwd.join("contract.docx")).unwrap();
    assert_ne!(packed, contract::package());
    // So may a file it packed itself.
    hand_edit(cwd, "English law", "Scots law");
    assert!(pack(&["-o", "contract.docx"]).status.success());

    // Someone saves a newer version there: the next pack refuses, and writes nothing.
    let newer = b"PK a newer document that someone else saved".to_vec();
    fs::write(cwd.join("contract.docx"), &newer).unwrap();
    hand_edit(cwd, "Payment", "Each payment");
    let refused = pack(&["-o", "contract.docx"]);
    assert_eq!(refused.status.code(), Some(4), "{}", text(&refused.stderr));
    assert!(
        text(&refused.stderr).starts_with("error[stale_target]"),
        "{}",
        text(&refused.stderr)
    );
    assert!(text(&refused.stderr).contains("--force"));
    assert_eq!(fs::read(cwd.join("contract.docx")).unwrap(), newer);
    let pending = stetdoc(cwd, &["check", "contract.stet", "--author", "Clauseo"]);
    assert!(
        text(&pending.stdout).contains("Pending edits"),
        "the hand edit is still pending"
    );

    // The default output is held to the same rule.
    fs::write(cwd.join("contract.edited.docx"), &newer).unwrap();
    assert_eq!(pack(&[]).status.code(), Some(4));

    let forced = pack(&["-o", "contract.docx", "--force"]);
    assert!(forced.status.success(), "{}", text(&forced.stderr));
    assert_ne!(fs::read(cwd.join("contract.docx")).unwrap(), newer);
}

#[test]
fn the_default_date_is_the_wall_clock_of_the_zone_tz_names() {
    let tmp = unpacked();
    let cwd = tmp.path();
    let date = |tz: &str| {
        let out = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
            .args(["--json", "edit", "contract.docx", "-o", "x.docx", "--dry-run", "--author", "A", "--ops", "-"])
            .current_dir(cwd)
            .env_remove("STETDOC_DATE")
            .env("TZ", tz)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                let batch = "base: 5737b772923cf8d3:0:5737b772923cf8d3\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n";
                child.stdin.take().unwrap().write_all(batch.as_bytes())?;
                child.wait_with_output()
            })
            .unwrap();
        assert!(out.status.success(), "{}", text(&out.stdout));
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let date = json["stamp"]["date"].as_str().unwrap().to_string();
        let civil: jiff::civil::DateTime = date[..19].parse().unwrap();
        (civil, text(&out.stderr))
    };
    let (utc, quiet) = date("UTC");
    assert!(quiet.is_empty(), "{quiet}");
    let (kolkata, _) = date("Asia/Kolkata");
    let (posix, _) = date("<+0530>-5:30");
    for local in [kolkata, posix] {
        let ahead = utc.until(local).unwrap().total(jiff::Unit::Second).unwrap();
        assert!(
            (19_790.0..=19_810.0).contains(&ahead),
            "{local} is not 5:30 ahead of {utc}"
        );
    }
    let (fallback, warning) = date("Not/AZone");
    let behind = utc
        .until(fallback)
        .unwrap()
        .total(jiff::Unit::Second)
        .unwrap();
    assert!(behind.abs() <= 10.0, "an unknown zone falls back to UTC");
    assert!(warning.contains("TZ=Not/AZone"), "and says so: {warning}");
}

#[test]
fn a_refused_hand_edit_names_its_place_once() {
    let tmp = unpacked();
    let cwd = tmp.path();
    // A changed structure line and a duplicated ref: each refusal carries the file (and line) as fields and in its message.
    for (from, to) in [("{table cols=2}", "{table cols=3}"), ("p5 This", "p4 This")] {
        let doc = cwd.join("contract.stet/document.md");
        let before = fs::read_to_string(&doc).unwrap();
        hand_edit(cwd, from, to);
        let out = stetdoc(cwd, &["check", "contract.stet", "--author", "Clauseo"]);
        assert_ne!(out.status.code(), Some(0));
        let first = text(&out.stderr)
            .lines()
            .next()
            .unwrap_or_default()
            .to_string();
        assert_eq!(first.matches("document.md").count(), 1, "{first}");
        let json = stetdoc(
            cwd,
            &["--json", "check", "contract.stet", "--author", "Clauseo"],
        );
        let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
        assert_eq!(value["error"]["file"], "document.md", "{value}");
        fs::write(&doc, before).unwrap();
    }
}

#[test]
fn an_edited_complement_is_refused() {
    let tmp = unpacked();
    let cwd = tmp.path();
    let manifest = cwd.join("contract.stet/.stet/manifest");
    let mut bytes = fs::read(&manifest).unwrap();
    bytes[0] ^= 1;
    fs::write(&manifest, bytes).unwrap();
    let view = stetdoc(cwd, &["view", "contract.stet"]);
    assert_eq!(view.status.code(), Some(6));
    assert!(text(&view.stderr).starts_with("error[corrupt_complement]"));
}

#[test]
fn a_forged_marker_is_never_taken_for_a_write() {
    let tmp = unpacked();
    let cwd = tmp.path();
    // A marker whose manifest digests match nothing on disk is not an interrupted write stetdoc made.
    fs::write(
        cwd.join("contract.stet/.stet/writing"),
        "stetdoc-writing 1\n",
    )
    .unwrap();
    let view = stetdoc(cwd, &["view", "contract.stet"]);
    assert_ne!(
        view.status.code(),
        Some(0),
        "a forged marker is never taken as a write"
    );
}
