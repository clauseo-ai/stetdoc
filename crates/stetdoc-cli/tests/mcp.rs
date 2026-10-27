//! `stetdoc mcp` as a client sees it: a child process spoken to in JSON-RPC over stdin and stdout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, Write};
use stetdoc_testkit::fixtures::contract;

mod mcp_client;

use mcp_client::{Server, ok};

#[test]
fn a_client_unpacks_edits_and_packs_a_tracked_change() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);

    let init = server.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "stetdoc");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    // The instructions send the agent to the skill, and away from the package's XML.
    let instructions = init["result"]["instructions"].as_str().unwrap();
    assert!(instructions.contains("call skills_get"), "{instructions}");
    assert!(
        instructions.contains("Never read or edit a .docx any other way"),
        "{instructions}"
    );
    server.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let list = server.request("tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    for name in [
        "unpack",
        "view",
        "edit",
        "check",
        "pack",
        "skills_get",
        "capabilities",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    assert!(!names.contains(&"init") && !names.contains(&"mcp"));

    let unpacked = server.call("unpack", json!({ "input": "contract.docx" }));
    let base = ok(&unpacked)["base"].as_str().unwrap().to_string();
    assert!(tmp.path().join("contract.stet/.stet").is_dir());

    let batch =
        format!("base: {base}\n---\n@replace\nref: p4\nfind: thirty days\nwith: sixty days\n");
    let edited = server.call("edit", json!({ "input": "contract.stet", "batch": batch }));
    ok(&edited);

    let packed = server.call("pack", json!({ "dir": "contract.stet" }));
    assert_eq!(ok(&packed)["output"], "contract.edited.docx");
    assert!(tmp.path().join("contract.edited.docx").is_file());

    // The packed document shows the edit as a tracked change: an insertion and a deletion in the redline.
    let view = server.call(
        "view",
        json!({ "input": "contract.edited.docx", "range": "p4..p4" }),
    );
    let shown = ok(&view)["text"].as_str().unwrap().to_string();
    assert!(
        shown.contains("{++sixty") && shown.contains("{--thirty"),
        "{shown}"
    );

    // A command's failure is a tool result with the message and the next step, not a protocol error.
    let failed = server.call("view", json!({ "input": "missing.docx" }));
    assert_eq!(failed["isError"], true);
    assert_eq!(failed["structuredContent"]["error"]["code"], "not_found");
    assert!(
        failed["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("next:")
    );

    // An edit with no batch is refused rather than read from stdin, which carries the protocol.
    let no_batch = server.call("edit", json!({ "input": "contract.stet" }));
    assert_eq!(no_batch["isError"], true, "{no_batch}");
    let unknown = server.call("view", json!({ "input": "contract.docx", "colour": "red" }));
    assert_eq!(unknown["isError"], true, "{unknown}");
}

#[test]
fn the_current_revision_works_without_a_handshake() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let meta = json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } });
    let discover = server.request("server/discover", meta.clone());
    assert_eq!(discover["result"]["supportedVersions"][0], "2026-07-28");
    assert_eq!(discover["result"]["resultType"], "complete");
    let list = server.request("tools/list", meta);
    assert!(list["result"]["tools"].as_array().unwrap().len() >= 10);

    let old = json!({ "_meta": { "io.modelcontextprotocol/protocolVersion": "1900-01-01" } });
    let refused = server.request("tools/list", old);
    assert_eq!(refused["error"]["code"], -32022);
    assert_eq!(refused["error"]["data"]["requested"], "1900-01-01");

    assert_eq!(
        server.request("ping", json!({}))["result"]["resultType"],
        "complete"
    );
    assert_eq!(
        server.request("no/such", json!({}))["error"]["code"],
        -32601
    );
    let unknown_tool = server.request("tools/call", json!({ "name": "init", "arguments": {} }));
    assert_eq!(unknown_tool["error"]["code"], -32602);

    // A line that is not JSON gets a parse error, and the server goes on.
    writeln!(server.stdin, "not json").unwrap();
    server.stdin.flush().unwrap();
    let mut line = String::new();
    server.stdout.read_line(&mut line).unwrap();
    let reply: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(reply["error"]["code"], -32700);
    assert_eq!(
        server.request("ping", json!({}))["result"]["resultType"],
        "complete"
    );
}

#[test]
fn a_root_confines_every_path() {
    let tmp = tempfile::tempdir().unwrap();
    let inside = tmp.path().join("inside");
    fs::create_dir(&inside).unwrap();
    fs::write(inside.join("contract.docx"), contract::package()).unwrap();
    fs::write(tmp.path().join("secret.docx"), contract::package()).unwrap();
    // The server runs elsewhere: relative paths resolve against the root, not the working directory.
    let mut server = Server::start(tmp.path(), &["--root", "inside"]);

    let secret = tmp.path().join("secret.docx");
    for escape in [
        json!("../secret.docx"),
        json!(secret.to_str().unwrap()),
        json!("sub/../../secret.docx"),
    ] {
        let refused = server.call("view", json!({ "input": escape }));
        assert_eq!(refused["isError"], true, "{refused}");
        let text = refused["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("outside"), "{text}");
    }
    let out = server.call(
        "unpack",
        json!({ "input": "contract.docx", "output": "../escaped.stet" }),
    );
    assert_eq!(out["isError"], true);
    assert!(!tmp.path().join("escaped.stet").exists());

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(tmp.path(), inside.join("link")).unwrap();
        let through_link = server.call("view", json!({ "input": "link/secret.docx" }));
        assert_eq!(through_link["isError"], true, "{through_link}");
    }

    // Inside the root, the loop works, and a default output that would land outside is refused.
    let unpacked = server.call("unpack", json!({ "input": "contract.docx" }));
    ok(&unpacked);
    assert!(inside.join("contract.stet").is_dir());
    let packed = server.call("pack", json!({ "dir": "contract.stet" }));
    ok(&packed);
    assert!(inside.join("contract.edited.docx").is_file());
    let here = server.call("unpack", json!({ "input": "contract.docx", "output": "." }));
    assert_eq!(
        here["isError"], true,
        "the root itself is not empty: {here}"
    );
    let root_pack = server.call("pack", json!({ "dir": "." }));
    assert_eq!(root_pack["isError"], true, "{root_pack}");
    assert!(!tmp.path().join("inside.edited.docx").exists());
}

#[test]
fn an_edit_takes_the_servers_author_and_without_one_says_how_to_set_it() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut bare = Server::spawn(tmp.path(), &[], false);
    let base = ok(&bare.call("unpack", json!({ "input": "contract.docx" })))["base"]
        .as_str()
        .unwrap()
        .to_string();
    let batch =
        format!("base: {base}\n---\n@replace\nref: p4\nfind: thirty days\nwith: sixty days\n");
    let refused = bare.call("edit", json!({ "input": "contract.stet", "batch": batch }));
    assert_eq!(refused["isError"], true);
    let text = refused["content"][0]["text"].as_str().unwrap();
    assert!(
        text.contains("stetdoc init") && text.contains("STETDOC_AUTHOR"),
        "{text}"
    );
    drop(bare);

    let mut named = Server::spawn(tmp.path(), &["--author", "Mia Chen"], false);
    let edit_schema = named.request("tools/list", json!({}));
    for tool in ["edit", "pack", "compare", "check"] {
        let listed = edit_schema["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == tool)
            .unwrap();
        assert!(
            listed["inputSchema"]["properties"]["author"].is_object(),
            "{tool}"
        );
    }
    ok(&named.call("edit", json!({ "input": "contract.stet", "batch": batch })));
    let packed = named.call("pack", json!({ "dir": "contract.stet" }));
    let saved = ok(&packed).to_string();
    assert!(saved.contains("Mia Chen"), "{saved}");

    // A call's own author wins over the server's, for that call only.
    let base = ok(&packed)["saved"]["base"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| {
            let view = named.call(
                "view",
                json!({ "input": "contract.stet", "range": "p5..p5" }),
            );
            ok(&view)["base"].as_str().unwrap().to_string()
        });
    let batch =
        format!("base: {base}\n---\n@replace\nref: p5\nfind: English law\nwith: Scots law\n");
    let receipt = named.call(
        "edit",
        json!({ "input": "contract.stet", "batch": batch, "author": "Claude" }),
    );
    // Mia Chen's changes are now another author's, and the receipt says so.
    let receipt = ok(&receipt);
    assert_eq!(receipt["stamp"]["author"], "Claude");
    assert_eq!(receipt["others"]["by_author"][0]["author"], "Mia Chen");
}

#[test]
fn the_view_tool_finds_within_a_range_and_names_a_pair_it_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let within = server.call(
        "view",
        json!({ "input": "contract.docx", "find": "days", "range": "p3..p5", "context": 0 }),
    );
    let page = ok(&within);
    assert_eq!(
        page["position"]["scope"],
        "find \"days\" in p3..p5: 1 match in 1 block"
    );
    let outside = server.call(
        "view",
        json!({ "input": "contract.docx", "find": "days", "range": "p1..p2" }),
    );
    assert_eq!(ok(&outside)["position"]["shown"], 0);
    // The schema says which pairs go apart, and a call that sends one gets a next step that names both.
    let list = server.request("tools/list", json!({}));
    let view = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "view")
        .unwrap()
        .clone();
    let find = view["inputSchema"]["properties"]["find"]["description"]
        .as_str()
        .unwrap();
    assert!(
        find.contains("Cannot be used with `outline`, `clause`"),
        "{find}"
    );
    assert_eq!(view["inputSchema"]["properties"]["budget"]["default"], 3000);
    let refused = server.call(
        "view",
        json!({ "input": "contract.docx", "find": "days", "clause": "1" }),
    );
    assert_eq!(refused["isError"], true, "{refused}");
    let next = refused["structuredContent"]["error"]["next"]
        .as_str()
        .unwrap();
    assert!(
        next.contains("`find`") && next.contains("`clause`"),
        "{next}"
    );
    assert!(!next.contains("input schema lists"), "{next}");
}

#[test]
fn the_view_tool_takes_one_find_or_a_list_and_lists_one_line_for_each_hit() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let list = server.request("tools/list", json!({}));
    let view = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "view")
        .unwrap()
        .clone();
    let find = &view["inputSchema"]["properties"]["find"];
    // A list, and not a union, which some clients refuse; a lone string is still taken, as the test goes on to show.
    assert_eq!(find["type"], "array", "{find}");
    assert_eq!(view["inputSchema"]["properties"]["context"]["default"], 0);
    // A list finds any of its texts, and each line says which it holds.
    let many = server.call(
        "view",
        json!({ "input": "contract.docx", "find": ["thirty", "law", "Due Date"] }),
    );
    let page = ok(&many);
    assert_eq!(
        page["position"]["scope"],
        "find \"thirty\" (1), \"law\" (1), \"Due Date\" (0): 2 matches in 2 blocks"
    );
    let text = page["text"].as_str().unwrap();
    assert!(
        text.contains("p4 Payment is due within thirty days. {~ \"thirty\" 1}"),
        "{text}"
    );
    assert!(text.contains("{~ \"law\" 1}"), "{text}");
    // One string is one text, and context gives the blocks whole.
    let one = server.call(
        "view",
        json!({ "input": "contract.docx", "find": "thirty", "context": 1 }),
    );
    let text = ok(&one)["text"].as_str().unwrap().to_string();
    assert!(text.contains("p3 ") && text.contains("p5 "), "{text}");
    assert!(text.contains("{~ 1 match}"), "{text}");
}

#[test]
fn edit_writes_a_docx_in_place_and_pack_done_removes_the_working_copy() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);

    let listing = server.request("tools/list", json!({}));
    let tools = listing["result"]["tools"].as_array().unwrap();
    let schema = |name: &str| {
        tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"]["properties"].clone()
    };
    assert_eq!(schema("edit")["in_place"]["type"], "boolean");
    assert_eq!(schema("pack")["done"]["type"], "boolean");
    assert_eq!(schema("check")["all"]["type"], "boolean");

    let view = server.call(
        "view",
        json!({ "input": "contract.docx", "range": "p4..p4" }),
    );
    let base = ok(&view)["base"].as_str().unwrap().to_string();
    let batch = format!("base: {base}\n---\n@replace\nref: p4\nfind: thirty\nwith: sixty\n");
    let same = server.call(
        "edit",
        json!({ "input": "contract.docx", "output": "contract.docx", "batch": batch }),
    );
    assert_eq!(same["isError"], true, "{same}");
    assert!(
        same["structuredContent"]["error"]["next"]
            .as_str()
            .unwrap()
            .contains("--in-place"),
        "{same}"
    );
    let edited = server.call(
        "edit",
        json!({ "input": "contract.docx", "in_place": true, "batch": batch }),
    );
    assert_eq!(ok(&edited)["output"], "contract.docx");
    // The receipt's base is the written file's, so a second in-place batch chains from it without a view.
    let next = ok(&edited)["receipt"]["base"].as_str().unwrap().to_string();
    let second = format!("base: {next}\n---\n@comment\nref: p4\ntext: Agreed.\n");
    let chained = server.call(
        "edit",
        json!({ "input": "contract.docx", "in_place": true, "batch": second }),
    );
    ok(&chained);
    let view = server.call(
        "view",
        json!({ "input": "contract.docx", "range": "p4..p4" }),
    );
    assert!(ok(&view)["text"].as_str().unwrap().contains("{++sixty++}"));

    ok(&server.call("unpack", json!({ "input": "contract.docx" })));
    let packed = server.call("pack", json!({ "dir": "contract.stet", "done": true }));
    assert_eq!(ok(&packed)["working_copy_removed"], true);
    assert!(!tmp.path().join("contract.stet").exists());
    assert!(tmp.path().join("contract.edited.docx").is_file());
}

#[test]
fn replace_and_comment_take_pairs_and_an_anchor_in_one_call() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let mut server = Server::start(tmp.path(), &[]);

    let listing = server.request("tools/list", json!({}));
    let tools = listing["result"]["tools"].as_array().unwrap();
    let schema =
        |name: &str| tools.iter().find(|t| t["name"] == name).unwrap()["inputSchema"].clone();
    let replace = schema("replace");
    assert_eq!(replace["properties"]["pairs"]["type"], "array");
    assert_eq!(
        replace["properties"]["pairs"]["items"]["required"],
        json!(["old", "new"])
    );
    assert_eq!(
        schema("comment")["required"],
        json!(["input", "anchor", "text"])
    );

    // Two places: refused with both listed, and nothing written.
    let several = server.call(
        "replace",
        json!({ "input": "contract.docx", "output": "out.docx", "pairs": [{ "old": "Supplier", "new": "Vendor" }] }),
    );
    assert_eq!(several["isError"], true, "{several}");
    assert_eq!(
        several["structuredContent"]["error"]["code"],
        "ambiguous_match"
    );
    let message = several["structuredContent"]["error"]["message"]
        .as_str()
        .unwrap();
    assert!(
        message.contains("p1") && message.contains("p2"),
        "{message}"
    );
    assert!(!tmp.path().join("out.docx").exists());

    // A pair that is not an object of old and new is a usage error.
    let wrong = server.call(
        "replace",
        json!({ "input": "contract.docx", "output": "out.docx", "pairs": ["thirty", "sixty"] }),
    );
    assert_eq!(
        wrong["structuredContent"]["error"]["code"], "invalid_payload",
        "{wrong}"
    );

    // Two pairs, one an empty new text, in place, with the server's author; the receipt carries the written file's base.
    let done = server.call(
        "replace",
        json!({
            "input": "contract.docx",
            "in_place": true,
            "pairs": [{ "old": "thirty days", "new": "sixty days" }, { "old": " on demand", "new": "" }],
        }),
    );
    let done = ok(&done);
    assert_eq!(done["found"][0]["count"], 1);
    assert_eq!(done["found"][1]["hits"][0]["block"], "p1");
    assert!(done["receipt"]["base"].is_string(), "{done}");
    let text = done["receipt"]["text"].as_str().unwrap();
    assert!(
        text.contains("{++sixty++}") && text.contains("{-- on demand--}"),
        "{text}"
    );

    let commented = server.call(
        "comment",
        json!({ "input": "contract.docx", "anchor": "sixty days", "text": "Agreed.", "in_place": true }),
    );
    let commented = ok(&commented);
    assert_eq!(commented["found"][0]["hits"][0]["block"], "p4");
    let view = server.call(
        "view",
        json!({ "input": "contract.docx", "comments": true }),
    );
    assert!(ok(&view)["text"].as_str().unwrap().contains("Agreed."));
}

#[test]
fn a_dry_run_writes_nothing_whether_or_not_it_is_given_an_output_or_in_place() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("contract.docx"), contract::package()).unwrap();
    let before = fs::read(tmp.path().join("contract.docx")).unwrap();
    let mut server = Server::start(tmp.path(), &[]);
    let base = ok(&server.call("unpack", json!({ "input": "contract.docx" })))["base"]
        .as_str()
        .unwrap()
        .to_string();
    let batch =
        format!("base: {base}\n---\n@replace\nref: p4\nfind: thirty days\nwith: sixty days\n");
    let flags = [
        json!({}),
        json!({ "output": "edited.docx" }),
        json!({ "in_place": true }),
        json!({ "output": "edited.docx", "in_place": true }),
    ];
    for extra in flags {
        let mut arguments = json!({ "input": "contract.docx", "batch": batch, "dry_run": true });
        arguments
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let result = server.call("edit", arguments.clone());
        assert_eq!(ok(&result)["dry_run"], true, "{arguments}");
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("dry run: nothing was applied."), "{text}");
        assert_eq!(fs::read(tmp.path().join("contract.docx")).unwrap(), before);
        assert!(!tmp.path().join("edited.docx").exists(), "{arguments}");
    }
    let pairs = json!([{ "old": "thirty", "new": "sixty" }]);
    let replaced = server.call(
        "replace",
        json!({ "input": "contract.docx", "pairs": pairs, "dry_run": true, "output": "edited.docx" }),
    );
    ok(&replaced);
    assert_eq!(fs::read(tmp.path().join("contract.docx")).unwrap(), before);
    assert!(!tmp.path().join("edited.docx").exists());
}
