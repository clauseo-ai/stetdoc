//! What makes `stetdoc mcp` cheap for an agent: compact text results, the check in every receipt, next steps in the tools'
//! own words, booleans sent as text, and working copies found by the bare name a client passes.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::fs;
use stetdoc_testkit::fixtures::contract;

mod mcp_client;

use mcp_client::{Server, ok};

/// The text a model reads of a result.
fn text(result: &Value) -> String {
    result["content"][0]["text"].as_str().unwrap().to_string()
}

#[test]
fn a_result_is_the_text_the_cli_prints_and_the_json_rides_in_structured_content() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let done = server.call(
        "replace",
        json!({ "input": "contract.docx", "pairs": [{ "old": "thirty days", "new": "sixty days" }] }),
    );
    let json = ok(&done);
    assert!(json["receipt"]["base"].is_string(), "{json}");
    let shown = text(&done);
    assert!(!shown.trim_start().starts_with('{'), "{shown}");
    assert!(shown.contains("wrote contract.edited.docx"), "{shown}");
    // The receipt says what the check found, so the agent has no reason to call check.
    assert!(shown.contains("needs no separate check"), "{shown}");
}

#[test]
fn an_ambiguous_text_lists_each_place_and_says_how_to_pick_one_or_all() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let refused = server.call(
        "replace",
        json!({ "input": "contract.docx", "pairs": [{ "old": "Supplier", "new": "Vendor" }] }),
    );
    assert_eq!(refused["isError"], true, "{refused}");
    let shown = text(&refused);
    assert!(shown.contains("error[ambiguous_match]"), "{shown}");
    assert!(shown.contains("p1") && shown.contains("p2"), "{shown}");
    // In the tool's words: a property and its value, not a flag of the CLI.
    assert!(shown.contains("pass all: true"), "{shown}");
    assert!(shown.contains("range p1..p1"), "{shown}");
    assert!(
        !shown.contains("--all") && !shown.contains("--range"),
        "{shown}"
    );

    // The retry it suggests works in one call.
    let one = server.call(
        "replace",
        json!({ "input": "contract.docx", "range": "p1..p1", "pairs": [{ "old": "Supplier", "new": "Vendor" }] }),
    );
    assert_eq!(ok(&one)["found"][0]["count"], 1);
    let every = server.call(
        "replace",
        json!({ "input": "contract.docx", "all": "true", "pairs": [{ "old": "Supplier", "new": "Vendor" }] }),
    );
    assert_eq!(ok(&every)["found"][0]["count"], 2);
}

#[test]
fn booleans_may_come_as_text_and_anything_else_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let dry = server.call(
        "replace",
        json!({ "input": "contract.docx", "dry_run": "true", "pairs": [{ "old": "thirty", "new": "sixty" }] }),
    );
    ok(&dry);
    assert!(!tmp.path().join("contract.edited.docx").exists());
    let ledger = server.call(
        "view",
        json!({ "input": "contract.docx", "revisions": "false", "outline": "TRUE" }),
    );
    ok(&ledger);
    let wrong = server.call(
        "view",
        json!({ "input": "contract.docx", "outline": "yes" }),
    );
    assert_eq!(wrong["isError"], true, "{wrong}");
    assert!(text(&wrong).contains("outline is true or false"), "{wrong}");

    // A typed operation's boolean and number fields take text too.
    let edited = server.call(
        "edit",
        json!({
            "input": "contract.docx",
            "dry_run": true,
            "ops": [
                { "op": "format", "ref": "p4", "find": "thirty", "bold": "true" },
                { "op": "replace", "ref": "p1", "find": "shall", "occurrence": "2", "with": "will" }
            ],
        }),
    );
    ok(&edited);
}

#[test]
fn a_working_copy_is_found_by_the_bare_name_beside_the_document_it_came_from() {
    let tmp = tempfile::tempdir().unwrap();
    let docs = tmp.path().join("session");
    fs::create_dir(&docs).unwrap();
    fs::write(docs.join("contract.docx"), contract::package()).unwrap();
    // The server runs in the parent folder, as a client that keeps documents in a folder of its own runs it.
    let mut server = Server::start(tmp.path(), &[]);
    let unpacked = server.call("unpack", json!({ "input": "session/contract.docx" }));
    let base = ok(&unpacked)["base"].as_str().unwrap().to_string();
    assert!(docs.join("contract.stet/.stet").is_dir());
    let edited = server.call(
        "edit",
        json!({
            "input": "contract.stet",
            "base": base,
            "ops": [{ "op": "replace", "ref": "p4", "find": "thirty", "with": "sixty" }],
        }),
    );
    ok(&edited);
    let packed = server.call("pack", json!({ "dir": "contract.stet" }));
    ok(&packed);
    assert!(docs.join("contract.edited.docx").is_file());
    // A name that is nowhere is still not found, and nothing else is picked.
    let missing = server.call("view", json!({ "input": "other.stet" }));
    assert_eq!(missing["isError"], true, "{missing}");
}

#[test]
fn the_view_tool_finds_within_one_story() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let body = server.call(
        "view",
        json!({ "input": "contract.docx", "story": "body", "find": ["thirty"] }),
    );
    assert!(text(&body).contains("p4"), "{body}");
    let headers = server.call(
        "view",
        json!({ "input": "contract.docx", "story": "headers", "find": ["thirty"] }),
    );
    assert!(text(&headers).contains("no matches"), "{headers}");
}

#[test]
fn the_comment_tool_adds_several_comments_in_one_call_or_none() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let added = server.call(
        "comment",
        json!({
            "input": "contract.docx",
            "anchor": "thirty days",
            "text": "Why not sixty?",
            "comments": [
                { "anchor": "English law", "text": "Check the governing law." },
                { "anchor": "After the table", "text": "A third." }
            ],
        }),
    );
    let json = ok(&added);
    let anchors: Vec<&str> = json["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["anchor"].as_str().unwrap())
        .collect();
    assert_eq!(anchors, ["thirty days", "English law", "After the table"]);
    assert_eq!(
        json["receipt"]["comments"].as_array().unwrap().len(),
        3,
        "{json}"
    );

    // One anchor that is not there refuses them all, and nothing is written.
    let refused = server.call(
        "comment",
        json!({
            "input": "contract.docx",
            "output": "none.docx",
            "comments": [
                { "anchor": "English law", "text": "fine" },
                { "anchor": "Scottish law", "text": "missing" }
            ],
        }),
    );
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(text(&refused).contains("pair 2"), "{refused}");
    assert!(!tmp.path().join("none.docx").exists());

    // Half a comment is refused with what to pass.
    let half = server.call(
        "comment",
        json!({ "input": "contract.docx", "anchor": "thirty" }),
    );
    assert_eq!(half["isError"], true, "{half}");
}

#[test]
fn a_large_receipt_is_a_summary_unless_detail_asks_for_all_of_it() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    // A hundred and fifty operations: their lines alone are far over 2 KB.
    let ops: Vec<Value> = (0..150)
        .map(|i| json!({ "op": "comment", "ref": format!("p{}", 1 + i % 5), "text": format!("A comment numbered {i} with some words in it.") }))
        .collect();
    let short = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "ops": ops }),
    );
    let shown = text(&short);
    assert!(shown.len() <= 2_600, "{} bytes: {shown}", shown.len());
    assert!(shown.contains("not listed here"), "{shown}");
    assert!(shown.contains("detail: true"), "{shown}");
    assert!(shown.contains("needs no separate check"), "{shown}");
    // The JSON has every operation either way.
    assert_eq!(ok(&short)["operations"].as_array().unwrap().len(), 150);
    let full = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "detail": true, "ops": ops }),
    );
    let whole = text(&full);
    assert!(whole.len() > shown.len(), "{whole}");
    assert!(!whole.contains("not listed here"), "{whole}");
}
