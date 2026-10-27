//! The MCP edit tool's typed operations, and the tool schemas every client must accept.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::fs;
use stetdoc_testkit::fixtures::contract;

mod mcp_client;

use mcp_client::{Server, ok};

fn error_of(result: &Value) -> (String, String, String) {
    assert_eq!(result["isError"], true, "{result}");
    let error = &result["structuredContent"]["error"];
    let text = |k: &str| error[k].as_str().unwrap_or_default().to_string();
    (text("code"), text("message"), text("next"))
}

#[test]
fn edit_applies_typed_operations_of_several_kinds_to_a_docx_without_a_base() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);

    let edited = server.call(
        "edit",
        json!({
            "input": "contract.docx",
            "output": "out.docx",
            "ops": [
                { "op": "replace", "ref": "p4", "find": "thirty", "with": "sixty" },
                { "op": "format", "ref": "p1", "find": "indemnify", "bold": true },
                { "op": "comment", "ref": "p5", "find": "English law", "text": "Check the governing law." },
                { "op": "set", "ref": "p5", "props": [{ "name": "spacing", "attrs": ["before=6pt"] }] },
                { "op": "delete", "ref": "p3" },
                { "op": "insert", "ref": "p5", "place": "after", "paragraphs": ["A new clause."], "find": null },
            ],
        }),
    );
    let edited = ok(&edited);
    assert_eq!(edited["output"], "out.docx", "{edited}");
    let view = server.call("view", json!({ "input": "out.docx", "all": true }));
    let text = ok(&view)["text"].as_str().unwrap().to_string();
    for marker in ["{++sixty++}", "{++A new clause.++}", "{--Emoji"] {
        assert!(text.contains(marker), "{marker} in {text}");
    }
    let comments = server.call("view", json!({ "input": "out.docx", "comments": true }));
    let comments = ok(&comments)["text"].as_str().unwrap().to_string();
    assert!(comments.contains("Check the governing law."), "{comments}");
    let formatted = server.call("inspect", json!({ "input": "out.docx", "target": "p1" }));
    let formatted = ok(&formatted)["text"].as_str().unwrap().to_string();
    assert!(formatted.contains("19..28: b on"), "{formatted}");
    let spaced = server.call("inspect", json!({ "input": "out.docx", "target": "p5" }));
    let spaced = ok(&spaced)["text"].as_str().unwrap().to_string();
    assert!(spaced.contains("before=120"), "{spaced}");

    // A second call on the written file: reply to the comment and resolve it, then reject the replacement, by base.
    let view = server.call("view", json!({ "input": "out.docx", "revisions": true }));
    let base = ok(&view)["base"].as_str().unwrap().to_string();
    let replies = server.call(
        "edit",
        json!({
            "input": "out.docx",
            "in_place": true,
            "base": base,
            "ops": [
                { "op": "reply", "comment": "c3", "text": "Done." },
                { "op": "resolve", "comment": "c3" },
                { "op": "reject", "author": "Clauseo" },
            ],
        }),
    );
    ok(&replies);
    let comments = server.call("view", json!({ "input": "out.docx", "comments": true }));
    let comments = ok(&comments)["text"].as_str().unwrap().to_string();
    assert!(comments.contains("Done."), "{comments}");
    let view = server.call("view", json!({ "input": "out.docx", "all": true }));
    let text = ok(&view)["text"].as_str().unwrap().to_string();
    assert!(
        text.contains("thirty days") && !text.contains("sixty"),
        "{text}"
    );
}

#[test]
fn edit_refuses_mixed_or_missing_operations_and_says_what_to_pass() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let replace = json!([{ "op": "replace", "ref": "p4", "find": "thirty", "with": "sixty" }]);

    let both = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "ops": replace, "batch": "@merge\nref: p1\n" }),
    );
    let (_, message, next) = error_of(&both);
    assert!(message.contains("ops or batch"), "{message}");
    assert!(next.contains("ops"), "{next}");

    let lone = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "base": "x" }),
    );
    assert!(error_of(&lone).2.contains("ops"), "{lone}");

    // A field the operation does not have is refused with the operation's index.
    let unknown = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "ops": [{ "op": "replace", "ref": "p4", "with": "x", "bold": true }] }),
    );
    let (code, message, _) = error_of(&unknown);
    assert_eq!(code, "invalid_payload");
    assert!(message.contains("bold"), "{message}");

    // A text batch without a base works on a .docx too, in block text and as a bare JSON list.
    let block = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "batch": "@replace\nref: p4\nfind: thirty\nwith: sixty\n" }),
    );
    ok(&block);
    let list = server.call(
        "edit",
        json!({ "input": "contract.docx", "dry_run": true, "batch": replace.to_string() }),
    );
    ok(&list);

    // A working copy keeps its base, and the refusal says where to find it.
    ok(&server.call("unpack", json!({ "input": "contract.docx" })));
    let copy = server.call("edit", json!({ "input": "contract.stet", "ops": replace }));
    let (_, message, next) = error_of(&copy);
    assert!(message.contains("no base"), "{message}");
    assert!(next.contains("base"), "{next}");
}

/// Every keyword a schema uses, at any depth, must be one that Gemini's function declarations and Claude's tool input
/// schemas both take: no unions, references or `const`, every object declares its properties and every array its items.
fn check_portable(at: &str, schema: &Value) {
    let object = schema
        .as_object()
        .unwrap_or_else(|| panic!("{at} is not an object"));
    for key in object.keys() {
        assert!(
            [
                "type",
                "description",
                "properties",
                "required",
                "items",
                "enum",
                "default",
                "minimum",
                "minItems",
                "additionalProperties"
            ]
            .contains(&key.as_str()),
            "{at} uses `{key}`"
        );
    }
    let kind = object["type"]
        .as_str()
        .unwrap_or_else(|| panic!("{at} has no type"));
    match kind {
        "object" => {
            let properties = object["properties"].as_object().unwrap();
            assert!(!properties.is_empty(), "{at} has no properties");
            for (name, property) in properties {
                check_portable(&format!("{at}.{name}"), property);
            }
        }
        "array" => check_portable(&format!("{at}[]"), &object["items"]),
        "string" | "integer" | "boolean" => {}
        other => panic!("{at} has type {other}"),
    }
}

#[test]
fn every_tool_schema_is_portable_and_matches_its_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let listing = server.request("tools/list", json!({}));
    let tools = listing["result"]["tools"].as_array().unwrap().clone();
    for tool in &tools {
        let name = tool["name"].as_str().unwrap();
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object", "{name}");
        check_portable(name, schema);
        let description = tool["description"].as_str().unwrap();
        assert!(
            description.len() < 1000,
            "{name}'s description is {} bytes",
            description.len()
        );
    }
    insta::assert_snapshot!(serde_json::to_string_pretty(&tools).unwrap());
}
