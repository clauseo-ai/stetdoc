//! Tests for the Claude Code plugin's guard hook, `plugins/claude/hooks/docx-guard.sh`: a table of tool calls it must
//! allow and calls it must block, each run through `sh` the way Claude Code runs it, with the call as JSON on stdin.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

/// The hook, from the repository root.
fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/claude/hooks/docx-guard.sh")
}

/// Runs the hook on one tool call, written as `pretty` or compact JSON, and returns its exit code and stderr.
fn run(input: &Value, pretty: bool, env: &[(&str, &str)]) -> (i32, String) {
    let text = if pretty {
        serde_json::to_string_pretty(input).unwrap()
    } else {
        serde_json::to_string(input).unwrap()
    };
    let mut cmd = Command::new("sh");
    cmd.arg(script())
        .env_remove("STETDOC_GUARD")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("sh runs");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.stdout.is_empty(),
        "the hook printed to stdout: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn bash(command: &str) -> Value {
    json!({
        "session_id": "s",
        "transcript_path": "/tmp/t.jsonl",
        "cwd": "/Users/someone/contracts",
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command, "description": "a description that says unzip a.docx" },
        "tool_use_id": "toolu_1"
    })
}

fn file_tool(tool: &str, path: &str, content: &str) -> Value {
    let input = match tool {
        "Write" => json!({ "file_path": path, "content": content }),
        _ => json!({ "file_path": path, "old_string": "x", "new_string": content }),
    };
    json!({ "hook_event_name": "PreToolUse", "tool_name": tool, "tool_input": input, "cwd": "/w" })
}

/// Bash commands the hook allows: every stetdoc command, whatever its arguments or here-document say, and commands that
/// only move, list, hash or open a package.
const ALLOWED: &[&str] = &[
    "stetdoc view a.docx",
    "stetdoc unpack 'Master Supply Agreement.docx'",
    "~/.cargo/bin/stetdoc view a.docx --find \"unzip\"",
    "npx stetdoc view a.docx",
    "STETDOC_AUTHOR=Rohan stetdoc edit a.stet <<'EOF'\nbase: x\n---\n@replace\nref: p4\nfind: unzip the file with python-docx; import zipfile\nwith: don't\nEOF",
    "stetdoc edit a.stet <<EOF\n@replace\nref: p4\nfind: x.docx\nwith: python3 a.docx\nEOF\nstetdoc check a.stet && stetdoc pack a.stet -o a.redline.docx --done",
    "stetdoc compare old.docx new.docx -o redline.docx && open redline.docx",
    "stetdoc view a.docx | grep -n Seller",
    "stetdoc view a.docx | python3 -c 'import sys; print(len(sys.stdin.read()))'",
    "stetdoc replace a.docx --find 'unzip' --with 'zip' --in-place",
    "if stetdoc check a.stet; then echo ok; fi",
    "ls -la *.docx",
    "cp contract.docx /tmp/contract-copy.docx",
    "mv draft.docx final.docx",
    "shasum -a 256 a.docx b.docx",
    "open a.docx",
    "file a.docx; stat a.docx; du -h a.docx",
    "rm -rf a.stet",
    "cmp a.docx b.docx",
    "find . -name '*.docx'",
    "command -v stetdoc || echo 'stetdoc not found'",
    "git commit -m 'block unzip and python zipfile on .docx files'",
    "gh pr create --body 'the guard stops unzip a.docx and import docx'",
    "zip -r logs.zip logs/",
    "unzip archive.zip -d out",
    "tar -xzf release.tar.gz",
    "python3 -c 'import zipfile; zipfile.ZipFile(\"x.zip\").extractall()'",
    "python3 analyse.py data.csv",
    "rg 'word/document.xml' crates/",
    "grep -rn document.xml src/",
    "cargo test -p stetdoc-core docx",
    "echo done # unzip a.docx",
    "ls *.docx | sed 's/\\.docx$//'",
    "pip install requests",
    "npm install docx-preview",
];

/// Bash commands the hook blocks: archive tools, scripts, converters and in-place editors on a package or its parts.
const BLOCKED: &[&str] = &[
    "unzip a.docx -d /tmp/a",
    "unzip -p 'Master Supply Agreement.docx' word/document.xml | head",
    "unzip -l a.DOCX",
    "cd /tmp && /usr/bin/unzip -o ~/contracts/a.docx",
    "mkdir -p /tmp/x && unzip -q a.docx -d /tmp/x && sed -i '' 's/Seller/Vendor/g' /tmp/x/word/document.xml",
    "cd /tmp/x && zip -r ../a.docx .",
    "ditto -x -k a.docx /tmp/a",
    "bsdtar -xf a.docm -C /tmp/a",
    "tar -xf template.dotx",
    "7z x a.docx",
    "zipinfo a.docx",
    "f=a.docx; unzip \"$f\"",
    "find . -name '*.docx' | xargs unzip -l",
    "find . -name '*.docx' -exec unzip -l {} \\;",
    "bash -c \"unzip a.docx\"",
    "cp a.docx /tmp/a.zip",
    "python3 -m zipfile -e a.docx /tmp/a",
    "python3 -c \"import zipfile; z = zipfile.ZipFile('a.docx'); print(z.read('word/document.xml'))\"",
    "python3 - <<'EOF'\nimport zipfile, re\nz = zipfile.ZipFile('/tmp/msa.docx')\nxml = z.read('word/document.xml').decode()\nprint(xml.count(\"Seller's\"))\nEOF",
    "python3 <<EOF\nfrom docx import Document\nd = Document('a.docx')\nEOF",
    "python3 -c 'import docx'",
    "pip install python-docx",
    "npm install docx",
    "node -e \"const JSZip = require('jszip'); JSZip.loadAsync(require('fs').readFileSync('a.docx'))\"",
    "node -e \"const { Document } = require('docx')\"",
    "cat > /tmp/fix.py <<'EOF'\nimport zipfile\nzipfile.ZipFile('/tmp/a.docx')\nEOF\npython3 /tmp/fix.py",
    "python3 /tmp/edit_docx.py contract.docx",
    "ruby -e 'require \"zip\"; Zip::File.open(\"a.docx\")'",
    "perl -MArchive::Zip -e 'Archive::Zip->new(\"a.docx\")'",
    "textutil -convert txt a.docx -stdout",
    "textutil -convert docx notes.txt -output a.docx",
    "pandoc a.docx -o a.md",
    "pandoc a.md -o a.docx",
    "soffice --headless --convert-to pdf a.docx",
    "sed -i '' 's/thirty/sixty/' a.docx",
    "perl -pi -e 's/thirty/sixty/' /tmp/x/word/document.xml",
    "cat /tmp/x/word/document.xml",
    "python3 -c \"print(open('/tmp/x/word/document.xml').read()[:500])\"",
    "xmllint --format /tmp/x/word/document.xml",
    "echo '<w:p/>' > /tmp/x/word/document.xml",
    "dd if=/dev/zero of=a.docx bs=1 count=1 conv=notrunc",
    "stetdoc view a.docx; unzip -l a.docx",
];

#[test]
fn the_guard_allows_stetdoc_and_harmless_commands() {
    for command in ALLOWED {
        for pretty in [false, true] {
            let (code, err) = run(&bash(command), pretty, &[]);
            assert_eq!(code, 0, "blocked {command:?} (pretty: {pretty}): {err}");
            assert!(err.is_empty(), "{command:?}: printed {err}");
        }
    }
}

#[test]
fn the_guard_blocks_other_ways_into_a_word_package() {
    for command in BLOCKED {
        for pretty in [false, true] {
            let (code, err) = run(&bash(command), pretty, &[]);
            assert_eq!(code, 2, "allowed {command:?} (pretty: {pretty})");
            assert!(err.contains("stetdoc view"), "{command:?}: {err}");
            assert!(err.contains("not installed"), "{command:?}: {err}");
        }
    }
}

#[test]
fn the_guard_blocks_writing_a_package_as_text_and_allows_other_files() {
    let blocked = [
        file_tool("Write", "/w/a.docx", "hello"),
        file_tool("Edit", "/w/A.DOCX", "hello"),
        file_tool("Write", "/w/t.dotx", "hello"),
        file_tool("Edit", "/tmp/x/word/document.xml", "<w:p/>"),
        file_tool("Write", "/tmp/x/[Content_Types].xml", "<Types/>"),
        file_tool(
            "Write",
            "/tmp/fix.py",
            "import zipfile\nz = zipfile.ZipFile('/w/a.docx')\n",
        ),
        file_tool(
            "Write",
            "/private/tmp/fix.py",
            "from docx import Document\n",
        ),
        file_tool(
            "Edit",
            "/tmp/fix.js",
            "const JSZip = require('jszip'); load('a.docx')",
        ),
    ];
    for input in &blocked {
        assert_eq!(run(input, true, &[]).0, 2, "allowed {input}");
    }
    let allowed = [
        file_tool("Write", "/w/notes.md", "unzip a.docx"),
        file_tool("Edit", "/w/a.stet/document.md", "thirty days"),
        file_tool(
            "Write",
            "/w/src/convert.py",
            "import zipfile\nzipfile.ZipFile('a.docx')\n",
        ),
        file_tool("Write", "/tmp/notes.txt", "import docx"),
        file_tool("Write", "/w/docx.rs", "fn main() {}"),
        json!({ "tool_name": "Read", "tool_input": { "file_path": "/w/a.docx" } }),
    ];
    for input in &allowed {
        assert_eq!(run(input, false, &[]).0, 0, "blocked {input}");
    }
}

#[test]
fn the_guard_reads_escaped_json_and_tolerates_odd_input() {
    // A command with \u escapes, as some encoders write them, still reads as `unzip a.docx`.
    let escaped = r#"{"tool_name":"Bash","tool_input":{"command":"unzip \u0022a.docx\u0022 \u0026\u0026 ls"}}"#;
    let quoted = r#"{ "tool_name" : "Bash" , "tool_input" : { "command" : "echo \"x\\\"y\" ; unzip a.docx" } }"#;
    for (text, want) in [
        (escaped, 2),
        (quoted, 2),
        ("not json", 0),
        ("", 0),
        ("{}", 0),
    ] {
        let mut child = Command::new("sh")
            .arg(script())
            .env_remove("STETDOC_GUARD")
            .stdin(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(text.as_bytes())
            .unwrap();
        assert_eq!(child.wait().unwrap().code(), Some(want), "{text}");
    }
}

#[test]
fn stetdoc_guard_off_turns_the_guard_off() {
    assert_eq!(
        run(&bash("unzip a.docx"), false, &[("STETDOC_GUARD", "off")]).0,
        0
    );
}
