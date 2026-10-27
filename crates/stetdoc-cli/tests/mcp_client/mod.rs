//! A client of `stetdoc mcp` for the tests: the binary as a child process, spoken to in JSON-RPC over stdin and stdout.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

pub struct Server {
    child: Child,
    pub stdin: ChildStdin,
    pub stdout: BufReader<ChildStdout>,
    next: u64,
}

impl Server {
    pub fn start(cwd: &Path, args: &[&str]) -> Server {
        Server::spawn(cwd, &[&["--author", "Clauseo"], args].concat(), true)
    }

    pub fn spawn(cwd: &Path, args: &[&str], with_author: bool) -> Server {
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

    pub fn send(&mut self, message: &Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    pub fn request(&mut self, method: &str, params: Value) -> Value {
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

    pub fn call(&mut self, tool: &str, arguments: Value) -> Value {
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

pub fn ok(result: &Value) -> &Value {
    assert_eq!(result["isError"], false, "{result}");
    &result["structuredContent"]
}
