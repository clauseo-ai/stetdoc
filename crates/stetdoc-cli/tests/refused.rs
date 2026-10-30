//! A finding that makes Word refuse the input, or offer to repair it, is the first thing `check` says about the input, on
//! every surface: a .docx, a working copy and the MCP check tool. The input here lists `w14` and `wp14` under
//! `mc:Ignorable` and declares neither, after more schema violations than the text lists without `--all` (decision 0014).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use stetdoc_testkit::fixtures::parts::{self, Doc};
use stetdoc_testkit::fixtures::zip::Zip;

mod mcp_client;

use mcp_client::{Server, ok};

const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const LINE: &str = "  mc_prefix_undeclared (word/document.xml): mc:Ignorable names w14, wp14, which no namespace declaration binds, so Word refuses the file or offers to repair it. stetdoc declares w14, wp14 when it writes the document.";

/// A main part with 30 paragraphs whose w:t carries an attribute the schema does not define, and a root that names
/// undeclared prefixes in mc:Ignorable.
fn package() -> Vec<u8> {
    let body: String = (1..=30)
        .map(|i| format!("<w:p><w:r><w:t w:space=\"x\">Clause {i}.</w:t></w:r></w:p>"))
        .collect();
    let root = format!(" xmlns:mc=\"{MC}\" mc:Ignorable=\"w14 wp14\"");
    let xml = Doc::new().root_attrs(&root).body(&body).xml();
    Zip::new(parts::entries_doc(xml.as_bytes())).build()
}

fn stetdoc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .output()
        .expect("run stetdoc")
}

fn input() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("broken.docx"), package()).unwrap();
    tmp
}

/// The finding lines of a check's text output.
fn findings(stdout: &[u8]) -> Vec<String> {
    String::from_utf8(stdout.to_vec())
        .unwrap()
        .lines()
        .skip_while(|l| !l.starts_with("Validation:"))
        .skip(1)
        .map(str::to_string)
        .collect()
}

#[test]
fn check_of_a_docx_lists_the_undeclared_prefixes_first() {
    let tmp = input();
    let out = stetdoc(tmp.path(), &["check", "broken.docx"]);
    assert_eq!(out.status.code(), Some(0));
    let lines = findings(&out.stdout);
    assert_eq!(lines.first().map(String::as_str), Some(LINE), "{lines:#?}");
    assert!(
        lines.last().unwrap().contains("--all lists them"),
        "{lines:#?}"
    );

    let json = stetdoc(tmp.path(), &["check", "broken.docx", "--json"]);
    let value: Value = serde_json::from_slice(&json.stdout).unwrap();
    let first = &value["report"]["findings"][0];
    assert_eq!(first["code"], "mc_prefix_undeclared", "{value}");
    assert_eq!(first["part"], "word/document.xml");
    assert_eq!(first["class"], "violation");
}

#[test]
fn check_of_a_working_copy_lists_the_undeclared_prefixes_first() {
    let tmp = input();
    let unpacked = stetdoc(tmp.path(), &["unpack", "broken.docx"]);
    assert!(unpacked.status.success(), "{unpacked:?}");
    let out = stetdoc(tmp.path(), &["check", "broken.stet"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let lines = findings(&out.stdout);
    assert_eq!(lines.first().map(String::as_str), Some(LINE), "{lines:#?}");
}

#[test]
fn the_mcp_check_tool_lists_the_undeclared_prefixes_first() {
    let tmp = input();
    let mut server = Server::start(tmp.path(), &[]);
    let result = server.call("check", json!({ "input": "broken.docx" }));
    let result = ok(&result);
    assert_eq!(
        result["report"]["findings"][0]["code"], "mc_prefix_undeclared",
        "{result}"
    );
    let message = result["report"]["findings"][0]["message"].as_str().unwrap();
    assert!(LINE.ends_with(message), "{message}");
}
