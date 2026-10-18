//! `cargo xtask corpus`: fetch, verify and cache the test corpora.
//!
//! - `corpus fetch [--full] [--pairs] [--verify] [--dir DIR] [--jobs N]` downloads the curated
//!   per-pull-request corpus, or with `--full` every corpus, checks every SHA-256 against
//!   `corpus/manifest/`, and builds the sets described in [`cache`]. The documents of the curated
//!   compare pairs of `corpus/pairs.tsv` come with the curated corpus, in a set of their own
//!   (`curatedpairs`), so the other corpus tests sample the same documents as before. `--pairs` adds the
//!   `pairs` set, the documents of every compare pair of `corpus/pairs.tsv`, for the full run of
//!   the compare laws without the whole corpus. With a complete cache it makes no network request at all.
//! - `corpus check` runs the manifest check alone.
//! - `corpus hash` prints the manifest hash that names the sets in the cache.
//! - `corpus path` prints the cache root.
//!
//! Third-party documents are never committed. Each is downloaded from its source at the
//! pinned commit with `git`. Git worktrees on one machine share the cache, so the cache is
//! built to be shared: see [`cache`] for the layout and [`lock`] and [`sets`] for how
//! concurrent fetches and readers stay safe. A fetch refuses a manifest that the check
//! rejects, and when the network is down it fails fast with one line.

pub mod cache;
mod download;
pub mod gitfetch;
pub mod lock;
pub mod manifest;
pub mod sets;
pub mod validate;

pub use validate::{check, check_cached};

use crate::repo::{Repo, Res};
use manifest::{Doc, How, Manifest};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

/// Flags of `corpus fetch`.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Fetch every corpus, not only the curated one.
    pub full: bool,
    /// Also build the `pairs` set: the documents of every compare pair.
    pub pairs: bool,
    /// Re-hash every cached file, not only check its size.
    pub verify: bool,
    /// Cache root, overriding the environment.
    pub dir: Option<String>,
    /// Sources fetched in parallel; 0 means 4.
    pub jobs: usize,
}

/// What a fetch did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Summary {
    /// The documents in the main set.
    pub docs: usize,
    /// Their total size in bytes.
    pub bytes: u64,
    /// Documents already in the cache.
    pub cached: usize,
    /// Documents downloaded in this run.
    pub downloaded: usize,
    /// Documents with no download location, left out.
    pub host_only: usize,
}

/// Runs a `corpus` subcommand.
pub fn run(args: &[&str]) -> ExitCode {
    match args {
        ["fetch", flags @ ..] => match parse_flags(flags).and_then(|o| {
            let repo = Repo::discover()?;
            let root = cache::dir_from_environment(o.dir.as_deref())?;
            let started = Instant::now();
            let s = fetch(&repo, &root, &o)?;
            println!(
                "corpus {}: {} documents, {:.1} MB; {} cached, {} downloaded, {} without a download location; {:.1}s\ncache: {}",
                if o.full {
                    "full"
                } else if o.pairs {
                    "pairs"
                } else {
                    "curated"
                },
                s.docs,
                s.bytes as f64 / 1e6,
                s.cached,
                s.downloaded,
                s.host_only,
                started.elapsed().as_secs_f64(),
                root.display()
            );
            Ok(())
        }) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => fail(&e),
        },
        ["check"] => match Repo::discover().and_then(|r| validate::check(&r)) {
            Ok(found) if found.is_empty() => {
                println!("corpus manifest ok");
                ExitCode::SUCCESS
            }
            Ok(found) => {
                for v in found {
                    eprintln!("{v}");
                }
                ExitCode::FAILURE
            }
            Err(e) => fail(&e),
        },
        ["hash"] => match Repo::discover().and_then(|r| Manifest::read(&r)) {
            Ok(m) => {
                println!("{}", m.hash);
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        ["path"] => match cache::dir_from_environment(None) {
            Ok(p) => {
                println!("{}", p.display());
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e),
        },
        _ => {
            eprintln!("usage: cargo xtask corpus fetch [--full] [--pairs] [--verify] [--dir DIR] [--jobs N]\n       cargo xtask corpus check\n       cargo xtask corpus hash\n       cargo xtask corpus path");
            ExitCode::from(2)
        }
    }
}

fn fail(message: &str) -> ExitCode {
    eprintln!("error: {message}");
    ExitCode::FAILURE
}

/// Parses the flags of `corpus fetch`.
pub fn parse_flags(flags: &[&str]) -> Res<Options> {
    let mut o = Options::default();
    let mut it = flags.iter();
    while let Some(flag) = it.next() {
        match *flag {
            "--full" => o.full = true,
            "--pairs" => o.pairs = true,
            "--verify" => o.verify = true,
            "--dir" => {
                o.dir = Some(it.next().ok_or("--dir needs a directory")?.to_string());
            }
            "--jobs" => {
                o.jobs = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--jobs needs a number")?;
            }
            other => return Err(format!("unknown flag `{other}` for corpus fetch")),
        }
    }
    Ok(o)
}

/// Where `cargo xtask check` and the tests find the curated corpus.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The cache root.
    pub root: PathBuf,
    /// The name of the immutable curated set directory under `<root>/sets` for the current
    /// manifest, `curated-<hash12>`.
    pub set: String,
}

/// The cache root and the curated set name for the manifest in `repo`. It reads the
/// manifest files and makes no network request and no change on disk.
pub fn target(repo: &Repo) -> Res<Target> {
    let root = cache::dir_from_environment(None)?;
    let hash = Manifest::hash_of(repo)?;
    Ok(Target {
        root,
        set: cache::set_name("curated", &hash),
    })
}

/// Makes the curated corpus available for `cargo xtask check` and returns where it is.
pub fn ensure_curated(repo: &Repo) -> Res<Target> {
    let target = target(repo)?;
    fetch(repo, &target.root, &Options::default())?;
    Ok(target)
}

/// What a fetch works on, taken from the manifest and the options.
struct Plan<'a> {
    /// Documents to download or find in the cache.
    fetchable: Vec<&'a Doc>,
    /// Documents with no download location, left out.
    host_only: usize,
    /// The sets to build: name and documents.
    sets: Vec<(&'static str, Vec<&'a Doc>)>,
    /// Index into `sets` of the set the summary reports.
    main: usize,
    /// Whether to re-hash cached files, not only check their size.
    rehash: bool,
}

impl<'a> Plan<'a> {
    fn new(m: &'a Manifest, opts: &Options) -> Plan<'a> {
        let how = |d: &Doc| m.source(&d.source).map(|s| s.how);
        let curated = m.curated_docs();
        let paired = if opts.pairs {
            m.pair_docs()
        } else {
            Vec::new()
        };
        let curated_pairs = m.curated_pair_docs();
        let mut selected: Vec<&Doc> = if opts.full {
            m.docs.iter().collect()
        } else {
            curated.clone()
        };
        if !opts.full {
            let mut have: std::collections::BTreeSet<&str> =
                selected.iter().map(|d| d.sha256.as_str()).collect();
            let extra: Vec<&Doc> = curated_pairs
                .iter()
                .chain(&paired)
                .copied()
                .filter(|d| have.insert(d.sha256.as_str()))
                .collect();
            selected.extend(extra);
        }
        let (fetchable, host_only): (Vec<&Doc>, Vec<&Doc>) = selected
            .into_iter()
            .filter(|d| how(d) != Some(How::Generate))
            .partition(|d| how(d) == Some(How::Fetch));
        let curated_fetchable: Vec<&Doc> = curated
            .iter()
            .copied()
            .filter(|d| how(d) == Some(How::Fetch))
            .collect();
        let fetched = |docs: Vec<&'a Doc>| -> Vec<&'a Doc> {
            docs.into_iter()
                .filter(|d| how(d) == Some(How::Fetch))
                .collect()
        };
        let mut sets = vec![
            ("curated", curated_fetchable),
            ("curatedpairs", fetched(curated_pairs)),
        ];
        if opts.pairs {
            sets.push(("pairs", fetched(paired)));
        }
        if opts.full {
            sets.push(("full", fetchable.clone()));
        }
        Plan {
            fetchable,
            host_only: host_only.len(),
            main: if opts.full || opts.pairs {
                sets.len() - 1
            } else {
                0
            },
            sets,
            rehash: opts.verify || !opts.full,
        }
    }

    /// The documents without a good blob in the cache.
    fn missing(&self, root: &Path) -> Vec<&'a Doc> {
        self.fetchable
            .iter()
            .copied()
            .filter(|d| !cache::have(root, d, self.rehash))
            .collect()
    }

    /// The documents whose blob is in the cache but writable: blobs of an earlier version.
    fn unprotected(&self, root: &Path) -> Vec<&'a Doc> {
        self.fetchable
            .iter()
            .copied()
            .filter(|d| cache::have(root, d, false) && !cache::is_protected(root, d))
            .collect()
    }

    /// Whether every set is built and published for this manifest hash.
    fn published(&self, root: &Path, hash: &str) -> bool {
        self.sets
            .iter()
            .all(|(name, _)| sets::is_published(root, name, hash))
    }

    fn summary(&self, downloaded: usize) -> Summary {
        let main = &self.sets[self.main].1;
        Summary {
            docs: main.len(),
            bytes: main.iter().map(|d| d.size).sum(),
            cached: self.fetchable.len() - downloaded,
            downloaded,
            host_only: self.host_only,
        }
    }
}

/// Refuses a manifest that the check rejects. The tier and the source name end up in file
/// paths, so a bad manifest must never reach the file system.
fn refuse_bad_manifest(repo: &Repo) -> Res<()> {
    let found = validate::check_manifest_cached(repo)?;
    if found.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = found.iter().map(ToString::to_string).collect();
    Err(lines.join("\n"))
}

/// Fetches and verifies the selected documents into `root` and builds the sets.
///
/// The manifest is checked first. With every blob present and read-only and every set
/// published the function returns without taking the lock. Otherwise it takes the exclusive lock, works
/// out again what is missing (another process may have done it), downloads that, builds and
/// publishes the sets and prunes the superseded ones.
pub fn fetch(repo: &Repo, root: &Path, opts: &Options) -> Res<Summary> {
    refuse_bad_manifest(repo)?;
    let m = Manifest::read(repo)?;
    let plan = Plan::new(&m, opts);
    if plan.missing(root).is_empty()
        && plan.published(root, &m.hash)
        && plan.unprotected(root).is_empty()
    {
        for (name, _) in &plan.sets {
            sets::touch(&sets::immutable_dir(root, name, &m.hash));
        }
        return Ok(plan.summary(0));
    }
    let _lock = lock::acquire(root)?;
    sets::clean_leftovers(root);
    let missing = plan.missing(root);
    // A blob that exists but is bad is replaced, and the links to the old file with it.
    let repaired = missing
        .iter()
        .any(|d| cache::blob_path(root, &d.sha256).exists());
    if !missing.is_empty() {
        let work = lock::Work::create(root)?;
        download::run(
            &m,
            root,
            work.path(),
            &missing,
            plan.fetchable.len(),
            opts.jobs,
        )?;
    }
    for doc in plan.unprotected(root) {
        cache::protect(root, doc)?;
    }
    for (name, docs) in &plan.sets {
        sets::build_set(root, name, docs, &m.hash, repaired)?;
        sets::publish(root, name, &m.hash)?;
        sets::prune(root, name, &m.hash, sets::KEEP);
    }
    Ok(plan.summary(missing.len()))
}

#[cfg(test)]
mod concurrency_tests;
#[cfg(test)]
mod tests;
