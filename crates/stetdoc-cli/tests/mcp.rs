//! `stetdoc mcp` as a client sees it: a child process spoken to in JSON-RPC over stdin and stdout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use stetdoc_testkit::fixtures::contract;

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Server {
    fn start(cwd: &Path, args: &[&str]) -> Server {
        Server::spawn(cwd, &[&["--author", "Clauseo"], args].concat(), true)
    }

    fn spawn(cwd: &Path, args: &[&str], with_author: bool) -> Server {
        let mut command = Command::new(env!("CARGO_BIN_EXE_stetdoc"));
        if !with_author {
            command.env_remove("STETDOC_AUTHOR");
        }
        let mut child = command
            .arg("mcp")
            .args(args)
            .current_dir(cwd)
            .env("XDG_CONFIG_HOME", cwd.join("no-config"))
            .env("STETDOC_DATE", "2026-10-06T09:30:00")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("run stetdoc mcp");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Server {
            child,
            stdin,
            stdout,
            next: 0,
        }
    }

    fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        let reply: Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}: {line}"));
        assert_eq!(reply["id"], id, "{reply}");
        assert_eq!(reply["jsonrpc"], "2.0");
        reply
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let reply = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        reply["result"].clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn ok(result: &Value) -> &Value {
    assert_eq!(result["isError"], false, "{result}");
    &result["structuredContent"]
}

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
    let receipt = ok(&receipt).to_string();
    assert!(
        receipt.contains("Claude") && !receipt.contains("Mia Chen"),
        "{receipt}"
    );
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
