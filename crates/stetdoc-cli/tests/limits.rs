//! The limits that refuse an input: the `limit_exceeded` error names the limit it passed, in its JSON (`limit`, `value`,
//! `max`) and in its message, and the command line tells the caller what it can do about it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use stetdoc_testkit::fixtures::{
    parts,
    zip::{Entry, Zip},
};

fn stetdoc(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_stetdoc"))
        .args(args)
        .current_dir(cwd)
        .env("STETDOC_DATE", "2026-10-06T09:30:00")
        .env("STETDOC_AUTHOR", "Jane Doe")
        .env("XDG_CONFIG_HOME", cwd.join("no-config"))
        .output()
        .expect("run stetdoc")
}

/// The error of a command that must fail with `limit_exceeded` (exit 6), as the JSON form reads it.
fn refused(output: &Output) -> Value {
    assert_eq!(output.status.code(), Some(6), "{output:?}");
    let json: Value = serde_json::from_slice(&output.stdout).expect("JSON error");
    assert_eq!(json["error"]["code"], "limit_exceeded", "{json}");
    json["error"].clone()
}

/// Writes a package whose document holds `body`, with `extra` entries added beside the parts every package has.
fn package(cwd: &Path, name: &str, body: &str, extra: Vec<Entry>) {
    let doc = parts::Doc::new().body(body).bytes();
    let mut entries = parts::entries_doc(&doc);
    entries.extend(extra);
    fs::write(cwd.join(name), Zip::new(entries).build()).unwrap();
}

#[test]
fn an_archive_over_the_size_limit_names_max_archive_bytes_and_says_what_to_do() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    // A sparse file: the size is checked before the file is read, so none of it is written.
    let big = fs::File::create(cwd.join("big.docx")).unwrap();
    big.set_len(513 << 20).unwrap();

    let out = stetdoc(cwd, &["view", "big.docx", "--json"]);
    let error = refused(&out);
    assert_eq!(error["limit"], "max_archive_bytes", "{error}");
    assert_eq!(error["value"], 513u64 << 20, "{error}");
    assert_eq!(error["max"], 512u64 << 20, "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("archive size limit of 536870912 bytes"),
        "{error}"
    );
    // The command line has no option to raise the limit, so its hint does not name one.
    let next = error["next"].as_str().unwrap();
    assert!(next.contains("cannot raise this limit"), "{next}");
    assert!(!next.contains("raise the limit in"), "{next}");

    let text = stetdoc(cwd, &["view", "big.docx"]);
    let said = String::from_utf8(text.stderr).unwrap();
    assert!(
        said.starts_with("error[limit_exceeded]: big.docx is larger than the archive size limit of 536870912 bytes."),
        "{said}"
    );
    assert!(said.contains("\nnext: Use a smaller input"), "{said}");
}

#[test]
fn a_package_with_too_many_entries_names_max_entries() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let extra: Vec<Entry> = (0..9998)
        .map(|i| Entry::new(format!("extra/{i}.bin"), b"x"))
        .collect();
    package(
        cwd,
        "many.docx",
        "<w:p><w:r><w:t>a</w:t></w:r></w:p>",
        extra,
    );

    let error = refused(&stetdoc(cwd, &["view", "many.docx", "--json"]));
    assert_eq!(error["limit"], "max_entries", "{error}");
    assert_eq!(error["value"], 10003, "{error}");
    assert_eq!(error["max"], 10000, "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("entry count limit of 10000"),
        "{error}"
    );
}

#[test]
fn xml_nested_deeper_than_the_limit_names_max_xml_depth() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    let body = format!(
        "{}<w:p><w:r><w:t>deep</w:t></w:r></w:p>{}",
        "<w:sdt><w:sdtContent>".repeat(300),
        "</w:sdtContent></w:sdt>".repeat(300)
    );
    package(cwd, "deep.docx", &body, Vec::new());

    let error = refused(&stetdoc(cwd, &["view", "deep.docx", "--json"]));
    assert_eq!(error["limit"], "max_xml_depth", "{error}");
    assert_eq!(error["max"], 256, "{error}");
    assert!(error["value"].as_u64().unwrap() > 256, "{error}");
    assert!(
        error["message"]
            .as_str()
            .unwrap()
            .contains("nesting depth limit of 256 levels"),
        "{error}"
    );
}

#[test]
fn a_part_declared_over_the_entry_size_limit_names_max_entry_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path();
    // 257 MiB declared, and a few hundred kilobytes compressed: the declared size is refused before the part is read.
    let big = parts::zeros_part("word/big.bin", 257 << 20);
    package(
        cwd,
        "bigpart.docx",
        "<w:p><w:r><w:t>a</w:t></w:r></w:p>",
        vec![big],
    );

    let error = refused(&stetdoc(cwd, &["view", "bigpart.docx", "--json"]));
    assert_eq!(error["limit"], "max_entry_bytes", "{error}");
    assert_eq!(error["value"], 257u64 << 20, "{error}");
    assert_eq!(error["max"], 256u64 << 20, "{error}");
}
