//! Downloads blobs from a pinned commit with plain `git`, so xtask needs no HTTP or TLS
//! crate (cargo-deny only allows MIT, Apache-2.0 and Unicode-3.0 licences).
//!
//! For one source: fetch the commit's tree without blobs, list it, fetch exactly the wanted
//! blobs in chunks (one request per chunk, the object ids sent on stdin, never in argv),
//! stream them out of `git cat-file --batch`, and check each against its SHA-256 and size.
//!
//! A failure is classified, so the caller can stop at once with one line instead of trying
//! every source: [`FetchError::Unreachable`] when the first fetch of the commit fails for any
//! reason but a bad pin, [`FetchError::BadPin`] when the remote does not have the commit,
//! [`FetchError::Gone`] when the repository itself was deleted, renamed or made private, which
//! fails that source alone, [`FetchError::Problems`] for documents that do not match the
//! manifest, and
//! [`FetchError::Other`]. Only transient failures are retried. A failure that will not go
//! away on a retry (no DNS, refused connection, bad pin) sets the shared abort flag, so no
//! other worker starts another source or another retry.

use crate::corpus::manifest::{Doc, hex_sha256};
use crate::repo::Res;
use std::collections::BTreeMap;
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How many blobs one `git fetch` asks for.
const CHUNK: usize = 500;
/// Attempts for a network operation before giving up.
const ATTEMPTS: u32 = 3;
/// The pause before the second attempt; each later pause is one step longer.
const RETRY_STEP: Duration = Duration::from_secs(2);

/// Why fetching one source failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The remote could not be reached, or refused the first fetch for a reason other than
    /// a missing commit. `detail` is the first line of git's error.
    Unreachable {
        /// The clone URL.
        url: String,
        /// The first line of git's error.
        detail: String,
    },
    /// The remote does not have the pinned commit.
    BadPin {
        /// The clone URL.
        url: String,
        /// The pinned commit.
        commit: String,
    },
    /// The repository does not exist any more, or is private: the pin must be replaced.
    Gone {
        /// The clone URL.
        url: String,
    },
    /// Documents that are missing upstream or do not match the manifest, one line each.
    Problems(Vec<String>),
    /// Anything else, with a message.
    Other(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Unreachable { url, detail } => write!(f, "cannot reach {url} ({detail})"),
            FetchError::BadPin { url, commit } => {
                write!(f, "the pinned commit {commit} is not available from {url}")
            }
            FetchError::Gone { url } => write!(
                f,
                "the repository {url} was not found: it was deleted, renamed or made private, so its pin cannot be fetched and must be replaced"
            ),
            FetchError::Problems(lines) => write!(f, "{}", lines.join("\n")),
            FetchError::Other(message) => write!(f, "{message}"),
        }
    }
}

impl FetchError {
    /// Whether no other source is worth trying after this error.
    pub fn stops_everything(&self) -> bool {
        matches!(
            self,
            FetchError::Unreachable { .. } | FetchError::BadPin { .. }
        )
    }
}

/// How a failed git network command should be treated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// A glitch that a retry may fix: a timeout, an early EOF, a reset, an HTTP 5xx.
    Transient,
    /// The remote has no such commit.
    BadPin,
    /// The remote has no such repository: deleted, renamed, or private to an anonymous reader.
    Gone,
    /// Anything else: no DNS, a refused connection, a TLS error, a bad URL.
    Permanent,
}

/// Messages of git that say the remote lacks the requested commit.
const BAD_PIN: &[&str] = &[
    "not our ref",
    "couldn't find remote ref",
    "reference is not a tree",
    "no such remote ref",
];

/// Messages of git that say the repository does not exist or is not readable without a login.
/// GitHub answers "Repository not found" for both a deleted and a private repository.
const GONE: &[&str] = &["repository not found", "could not read username"];

/// Messages of git that a retry can fix.
const TRANSIENT: &[&str] = &[
    "timed out",
    "timeout",
    "early eof",
    "rpc failed",
    "unexpected disconnect",
    "hung up unexpectedly",
    "connection reset",
    "returned error: 5",
    "http 5",
    "http/2 stream",
];

/// Classifies the standard error of a failed git network command.
pub fn classify(stderr: &str) -> Failure {
    let text = stderr.to_ascii_lowercase();
    if BAD_PIN.iter().any(|m| text.contains(m)) {
        Failure::BadPin
    } else if GONE.iter().any(|m| text.contains(m))
        || (text.contains("repository '") && text.contains("' not found"))
    {
        Failure::Gone
    } else if TRANSIENT.iter().any(|m| text.contains(m)) {
        Failure::Transient
    } else {
        Failure::Permanent
    }
}

/// The first non-empty line of git's error.
fn first_line(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("git gave no error message")
        .to_string()
}

fn command(work: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.current_dir(work)
        .env("GIT_TERMINAL_PROMPT", "0")
        // Keep git's messages in English, which the classification reads.
        .env("LC_ALL", "C");
    cmd
}

/// Runs git with optional standard input and returns its output.
fn git(work: &Path, args: &[&str], input: Option<&str>) -> Res<Output> {
    let cannot =
        |e: std::io::Error| format!("cannot run git: {e}; the corpus fetch needs git on the PATH");
    let mut child = command(work)
        .args(args)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(cannot)?;
    let writer = match (input, child.stdin.take()) {
        (Some(text), Some(mut stdin)) => {
            let text = text.to_string();
            Some(std::thread::spawn(move || {
                let _ = stdin.write_all(text.as_bytes());
            }))
        }
        _ => None,
    };
    let out = child.wait_with_output().map_err(cannot);
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    out
}

fn git_ok(work: &Path, args: &[&str]) -> Res<String> {
    let out = git(work, args, None)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.join(" "),
            first_line(&String::from_utf8_lossy(&out.stderr))
        ))
    }
}

/// A git network command that did not succeed.
enum NetFailure {
    /// Git could not be started.
    Cannot(String),
    /// Git ran and failed.
    Failed(Failure, String),
}

/// Runs a git network command, retrying only transient failures, and not at all once the
/// abort flag is set. `input` is sent on stdin again for every attempt.
fn git_retry(
    work: &Path,
    args: &[&str],
    input: Option<&str>,
    abort: &AtomicBool,
) -> Result<(), NetFailure> {
    let mut attempt = 1;
    loop {
        let out = git(work, args, input).map_err(NetFailure::Cannot)?;
        if out.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        let kind = classify(&stderr);
        if kind != Failure::Transient || attempt >= ATTEMPTS || abort.load(Ordering::SeqCst) {
            return Err(NetFailure::Failed(kind, stderr));
        }
        std::thread::sleep(RETRY_STEP * attempt);
        attempt += 1;
    }
}

/// Parses `git ls-tree -r -z` output into path to object id.
pub fn parse_tree(raw: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for entry in raw.split('\0').filter(|e| !e.is_empty()) {
        if let Some((meta, path)) = entry.split_once('\t') {
            let mut parts = meta.split_whitespace();
            if let (Some(_mode), Some("blob"), Some(oid)) =
                (parts.next(), parts.next(), parts.next())
            {
                map.insert(path.to_string(), oid.to_string());
            }
        }
    }
    map
}

/// Reads one `git cat-file --batch` reply: the object bytes, or `None` when it is missing.
fn read_object(reader: &mut impl BufRead) -> Res<Option<Vec<u8>>> {
    let mut header = String::new();
    let n = reader
        .read_line(&mut header)
        .map_err(|e| format!("cannot read git output: {e}"))?;
    if n == 0 {
        return Err("git cat-file ended early".into());
    }
    let fields: Vec<&str> = header.split_whitespace().collect();
    if fields.get(1) == Some(&"missing") {
        return Ok(None);
    }
    let size: usize = fields
        .get(2)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("unexpected git cat-file header `{}`", header.trim()))?;
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take(size as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read a blob from git: {e}"))?;
    if bytes.len() != size {
        return Err("git cat-file ended early".into());
    }
    let mut newline = [0u8; 1];
    reader
        .read_exact(&mut newline)
        .map_err(|e| format!("cannot read git output: {e}"))?;
    Ok(Some(bytes))
}

/// Whether `commit` is an object id, so it cannot be taken for an option.
fn is_object_id(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Fetches `wants` from `commit` of `url` into a new bare repository at `work` and passes
/// each verified document to `sink`. Every document problem is collected and returned
/// together. A failure that no retry can fix sets `abort`.
pub fn fetch(
    work: &Path,
    url: &str,
    commit: &str,
    wants: &[&Doc],
    abort: &AtomicBool,
    mut sink: impl FnMut(&Doc, &[u8]) -> Res<()>,
) -> Result<(), FetchError> {
    let stop = |e: FetchError| {
        if e.stops_everything() {
            abort.store(true, Ordering::SeqCst);
        }
        e
    };
    if !is_object_id(commit) || url.is_empty() || url.starts_with('-') {
        return Err(FetchError::Other(format!(
            "the source {url} with the pin {commit} is not usable: the pin must be a full commit id"
        )));
    }
    std::fs::create_dir_all(work)
        .map_err(|e| FetchError::Other(format!("cannot create {}: {e}", work.display())))?;
    git_ok(work, &["init", "--bare", "-q"]).map_err(FetchError::Other)?;
    git_ok(work, &["remote", "add", "origin", url]).map_err(FetchError::Other)?;
    let first = [
        "fetch",
        "-q",
        "--depth",
        "1",
        "--filter=blob:none",
        "--no-tags",
        "origin",
        commit,
    ];
    match git_retry(work, &first, None, abort) {
        Ok(()) => {}
        Err(NetFailure::Cannot(m)) => return Err(FetchError::Other(m)),
        Err(NetFailure::Failed(Failure::BadPin, _)) => {
            return Err(stop(FetchError::BadPin {
                url: url.into(),
                commit: commit.into(),
            }));
        }
        Err(NetFailure::Failed(Failure::Gone, _)) => {
            return Err(FetchError::Gone { url: url.into() });
        }
        Err(NetFailure::Failed(_, stderr)) => {
            return Err(stop(FetchError::Unreachable {
                url: url.into(),
                detail: first_line(&stderr),
            }));
        }
    }
    let head = git_ok(work, &["rev-parse", "FETCH_HEAD"]).map_err(FetchError::Other)?;
    if head.trim() != commit {
        return Err(FetchError::Other(format!(
            "{url} returned commit {} for the pin {commit}",
            head.trim()
        )));
    }
    let listing =
        git_ok(work, &["ls-tree", "-r", "-z", "FETCH_HEAD"]).map_err(FetchError::Other)?;
    let tree = parse_tree(&listing);
    let mut problems = Vec::new();
    let mut jobs: Vec<(&str, &Doc)> = Vec::new();
    for doc in wants {
        match tree.get(&doc.path) {
            Some(oid) => jobs.push((oid.as_str(), doc)),
            None => problems.push(format!(
                "{} is not in {url} at {commit} (path `{}`)",
                doc.origin.0, doc.path
            )),
        }
    }
    for chunk in jobs.chunks(CHUNK) {
        let ids: String = chunk.iter().map(|(oid, _)| format!("{oid}\n")).collect();
        let args = [
            "fetch",
            "-q",
            "--stdin",
            "--no-tags",
            "--filter=blob:none",
            "origin",
        ];
        match git_retry(work, &args, Some(&ids), abort) {
            Ok(()) => {}
            Err(NetFailure::Cannot(m)) => return Err(FetchError::Other(m)),
            Err(NetFailure::Failed(Failure::Gone, _)) => {
                return Err(FetchError::Gone { url: url.into() });
            }
            Err(NetFailure::Failed(Failure::BadPin, stderr)) => {
                return Err(FetchError::Other(format!(
                    "cannot fetch blobs from {url}: {}",
                    first_line(&stderr)
                )));
            }
            Err(NetFailure::Failed(_, stderr)) => {
                return Err(stop(FetchError::Unreachable {
                    url: url.into(),
                    detail: first_line(&stderr),
                }));
            }
        }
        read_chunk(work, chunk, commit, &mut problems, &mut sink);
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(FetchError::Problems(problems))
    }
}

/// Reads the blobs of one chunk out of `git cat-file --batch`, verifies each and gives it to
/// `sink`. A mismatch, a read error and a sink error are all recorded as problems; on a read
/// or sink error the child is killed. The writer thread is always joined and the child is
/// always waited for, so nothing is left running. Git never fetches a missing object on
/// demand here, it reports it missing.
fn read_chunk(
    work: &Path,
    chunk: &[(&str, &Doc)],
    commit: &str,
    problems: &mut Vec<String>,
    sink: &mut impl FnMut(&Doc, &[u8]) -> Res<()>,
) {
    let spawned = command(work)
        .args(["cat-file", "--batch"])
        .env("GIT_NO_LAZY_FETCH", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            problems.push(format!("cannot run git cat-file: {e}"));
            return;
        }
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        problems.push("git cat-file has no standard input or output".into());
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    let ids: String = chunk.iter().map(|(oid, _)| format!("{oid}\n")).collect();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(ids.as_bytes());
    });
    let mut reader = BufReader::new(stdout);
    let mut broken = false;
    for (_, doc) in chunk {
        match read_object(&mut reader) {
            Err(e) => {
                problems.push(format!("{}: {e}", doc.path));
                broken = true;
            }
            Ok(None) => problems.push(format!("{}: blob missing after fetch", doc.path)),
            Ok(Some(bytes)) => {
                let sha = hex_sha256(&bytes);
                if bytes.len() as u64 != doc.size || sha != doc.sha256 {
                    problems.push(format!(
                        "{}:{}: {} at {commit} has SHA-256 {sha} ({} bytes), the manifest says {} ({} bytes)",
                        doc.origin.0,
                        doc.origin.1,
                        doc.path,
                        bytes.len(),
                        doc.sha256,
                        doc.size
                    ));
                } else if let Err(e) = sink(doc, &bytes) {
                    problems.push(format!("{}: cannot store the verified blob: {e}", doc.path));
                    broken = true;
                }
            }
        }
        if broken {
            break;
        }
    }
    if broken {
        let _ = child.kill();
    }
    drop(reader);
    let _ = writer.join();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_parser_keeps_blobs_and_paths_with_spaces() {
        let raw =
            "100644 blob aaa\ta b/c d.docx\x0040000 tree bbb\tdir\x00100755 blob ccc\tx.docx\x00";
        let tree = parse_tree(raw);
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.get("a b/c d.docx").map(String::as_str), Some("aaa"));
    }

    #[test]
    fn a_cat_file_reply_is_read_with_its_trailing_newline() {
        let mut data: &[u8] = b"abc blob 3\nxyz\nabc missing\n";
        assert_eq!(read_object(&mut data).expect("read"), Some(b"xyz".to_vec()));
        assert_eq!(read_object(&mut data).expect("read"), None);
        assert!(read_object(&mut data).is_err());
        let mut short: &[u8] = b"abc blob 9\nxyz";
        assert!(
            read_object(&mut short).is_err(),
            "a truncated blob is an error"
        );
    }

    #[test]
    fn failures_are_classified_for_retry_and_for_stopping() {
        let kind = |s: &str| classify(s);
        for transient in [
            "fatal: unable to access 'https://x/': Operation timed out after 300000 ms",
            "error: RPC failed; curl 56 GnuTLS recv error",
            "fatal: early EOF",
            "error: The requested URL returned error: 503",
            "fatal: the remote end hung up unexpectedly",
            "fatal: Connection reset by peer",
        ] {
            assert_eq!(kind(transient), Failure::Transient, "{transient}");
        }
        for permanent in [
            "fatal: unable to access 'https://x/': Could not resolve host: x",
            "fatal: unable to access 'https://x/': Failed to connect to x port 443: Connection refused",
            "ssh: connect to host x port 22: Network is unreachable",
            "fatal: unable to access 'https://x/': SSL certificate problem",
            "fatal: '/nope' does not appear to be a git repository",
        ] {
            assert_eq!(kind(permanent), Failure::Permanent, "{permanent}");
        }
        for gone in [
            "remote: Repository not found.\nfatal: repository 'https://github.com/o/r/' not found",
            "fatal: repository 'https://github.com/o/r/' not found",
            "fatal: could not read Username for 'https://github.com': terminal prompts disabled",
        ] {
            assert_eq!(kind(gone), Failure::Gone, "{gone}");
        }
        for bad in [
            "fatal: remote error: upload-pack: not our ref 0000",
            "fatal: couldn't find remote ref 0000",
            "error: Server does not allow request for unadvertised object; reference is not a tree",
        ] {
            assert_eq!(kind(bad), Failure::BadPin, "{bad}");
        }
    }

    #[test]
    fn the_first_line_of_a_git_error_is_the_first_non_empty_one() {
        assert_eq!(first_line("\n  fatal: no\nhint: yes\n"), "fatal: no");
        assert_eq!(first_line(""), "git gave no error message");
    }

    #[test]
    fn a_pin_that_could_be_an_option_is_refused_before_git_runs() {
        let dir = tempfile::tempdir().expect("dir");
        let abort = AtomicBool::new(false);
        let err = fetch(
            dir.path(),
            "file:///x",
            "--upload-pack=x",
            &[],
            &abort,
            |_, _| Ok(()),
        )
        .expect_err("refused");
        assert!(matches!(err, FetchError::Other(_)), "{err}");
        let err = fetch(
            dir.path(),
            "-x",
            &"a".repeat(40),
            &[],
            &abort,
            |_, _| Ok(()),
        )
        .expect_err("refused");
        assert!(matches!(err, FetchError::Other(_)), "{err}");
        assert!(!abort.load(Ordering::SeqCst));
    }
}
