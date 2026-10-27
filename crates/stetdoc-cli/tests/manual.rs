//! Manual markup from the command line and the MCP server: `replace --manual`, `edit --manual-color` and the replace tool's
//! `manual`, each checked by `inspect` (the formatting of the changed characters) and `view --revisions` (no revision).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use stetdoc_testkit::fixtures::contract;

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

fn out(o: &Output) -> String {
    assert!(
        o.status.success(),
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn with_contract() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    tmp
}

fn no_revisions(cwd: &Path, doc: &str) {
    let ledger = out(&stetdoc(cwd, &["view", doc, "--revisions"], ""));
    assert!(ledger.contains("{changes: 0 changes}"), "{ledger}");
}

#[test]
fn replace_manual_strikes_old_and_underlines_new_with_no_revision() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let receipt = out(&stetdoc(
        cwd,
        &[
            "replace",
            "contract.docx",
            "thirty days",
            "sixty days",
            " on demand",
            "",
            "--manual",
            "-o",
            "out.docx",
        ],
        "",
    ));
    assert!(receipt.contains("as manual markup"), "{receipt}");
    assert!(receipt.contains("not Track Changes"), "{receipt}");
    assert!(
        receipt.contains("0 replace p4: struck \"thirty\", underlined \"sixty\"\n1 delete p1: struck \" on demand\"\n"),
        "{receipt}"
    );
    no_revisions(cwd, "out.docx");
    let p4 = out(&stetdoc(cwd, &["inspect", "out.docx", "p4"], ""));
    assert!(
        p4.contains("characters 0..22: plain\ncharacters 22..28: strike on\ncharacters 28..33: u val=single\ncharacters 33..39: plain"),
        "{p4}"
    );
    let p1 = out(&stetdoc(cwd, &["inspect", "out.docx", "p1"], ""));
    assert!(p1.contains("strike on") && !p1.contains("u val"), "{p1}");
    // Nothing else changed: a paragraph the edit did not name is plain.
    let p5 = out(&stetdoc(cwd, &["inspect", "out.docx", "p5"], ""));
    assert!(p5.contains("plain") && !p5.contains("strike"), "{p5}");
}

#[test]
fn edit_manual_color_marks_a_batch_and_refuses_a_split_with_what_to_do() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let view = out(&stetdoc(
        cwd,
        &["view", "contract.docx", "--range", "p1..p1"],
        "",
    ));
    let at = view.find("base=").unwrap() + 5;
    let base = view[at..].split_whitespace().next().unwrap();
    let batch = format!("base: {base}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n");
    let receipt = out(&stetdoc(
        cwd,
        &[
            "--json",
            "edit",
            "contract.docx",
            "--manual-color",
            "-o",
            "out.docx",
        ],
        &batch,
    ));
    let receipt: Value = serde_json::from_str::<Value>(&receipt).unwrap()["receipt"].clone();
    assert_eq!(receipt["markup"], "manual_color", "{receipt}");
    assert_eq!(receipt["tracked"], false);
    no_revisions(cwd, "out.docx");
    let p4 = out(&stetdoc(cwd, &["inspect", "out.docx", "p4"], ""));
    assert!(p4.contains("strike on, color val=FF0000"), "{p4}");
    assert!(p4.contains("u val=single, color val=0000FF"), "{p4}");

    let split = format!("base: {base}\n---\n@split\nref: p4\nat: 4\n");
    let refused = stetdoc(
        cwd,
        &["edit", "contract.docx", "--manual", "-o", "split.docx"],
        &split,
    );
    assert!(!refused.status.success());
    let said = String::from_utf8(refused.stderr).unwrap();
    assert!(said.contains("manual markup"), "{said}");
    assert!(said.contains("next: "), "{said}");
    assert!(!cwd.join("split.docx").exists());
}

#[test]
fn the_mcp_replace_tool_takes_manual() {
    let tmp = with_contract();
    let cwd = tmp.path();
    let mut child = Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(["mcp", "--author", "Clauseo"])
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut ask = |id: u64, method: &str, params: Value| -> Value {
        let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        writeln!(stdin, "{message}").unwrap();
        stdin.flush().unwrap();
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let listing = ask(1, "tools/list", json!({}));
    let tools = listing["result"]["tools"].as_array().unwrap();
    let schema = |name: &str| {
        tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"]["properties"].clone()
    };
    for name in ["replace", "edit"] {
        assert_eq!(schema(name)["manual"]["type"], "boolean", "{name}");
        assert_eq!(schema(name)["manual_color"]["type"], "boolean", "{name}");
    }
    let done = ask(
        2,
        "tools/call",
        json!({ "name": "replace", "arguments": {
            "input": "contract.docx", "in_place": true, "manual": true,
            "pairs": [{ "old": "thirty", "new": "sixty" }],
        }}),
    );
    let receipt = &done["result"]["structuredContent"]["receipt"];
    assert_eq!(receipt["markup"], "manual", "{done}");
    assert!(
        receipt["text"].as_str().unwrap().contains("manual markup"),
        "{done}"
    );
    let _ = child.kill();
    let _ = child.wait();
    no_revisions(cwd, "contract.docx");
    let p4 = out(&stetdoc(cwd, &["inspect", "contract.docx", "p4"], ""));
    assert!(
        p4.contains("strike on") && p4.contains("u val=single"),
        "{p4}"
    );
}
