//! The compare pairs and the noise gate of N9 (`docs/design/compare.md`, 9.2 and 9.4).
//!
//! `corpus/pairs.tsv` lists each pair by the SHA-256 of its original and revised document, so a pair
//! resolves against any corpus set that holds both. A run on the curated set (what `cargo xtask check`
//! fetches) takes the `curated` pairs, from the `curatedpairs` set fetched beside it; a run on the `pairs` set (`cargo xtask corpus fetch --pairs`) or the
//! `full` set takes every pair. A pair in scope whose documents the set lacks is an error, never a skip.
//!
//! The noise gate is plain data too: [`Baseline`] reads `crates/stetdoc-testkit/bench/noise_baseline.tsv`,
//! [`Figures`] are what the core's test measured on one pair, and [`Gate`] says whether a pair is an
//! outlier and checks it against the reason recorded in the manifest.

use crate::corpus::{Corpus, ENV_DIR, ENV_SET, Entry};
use std::collections::BTreeMap;

/// The pair manifest, as committed.
pub const MANIFEST: &str = include_str!("../../../corpus/pairs.tsv");
/// The noise baseline, as committed.
pub const BASELINE: &str = include_str!("../bench/noise_baseline.tsv");

/// One line of `corpus/pairs.tsv`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pair {
    /// `lab`, `oxpt`, `dox` or `dcb`.
    pub class: String,
    /// Whether the pair runs on every pull request.
    pub curated: bool,
    /// SHA-256 of the original document.
    pub original: String,
    /// SHA-256 of the revised document.
    pub revised: String,
    /// A name for messages.
    pub label: String,
    /// Why the pair is a noise outlier, when it is one.
    pub outlier: Option<String>,
    /// Line in `corpus/pairs.tsv`.
    pub line: usize,
}

/// Which pairs a run takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The `curated` pairs: a run on the curated set.
    Curated,
    /// Every pair: a run on the `pairs` or the `full` set.
    Full,
}

impl Scope {
    /// The scope of a run on the set called `set` (`curated`, `curated-<hash>`, `pairs-<hash>`, `full`).
    pub fn of_set(set: &str) -> Scope {
        if set.starts_with("pairs") || set.starts_with("full") {
            Scope::Full
        } else {
            Scope::Curated
        }
    }

    /// The name of the scope in the baseline: `curated` or `full`.
    pub fn name(self) -> &'static str {
        match self {
            Scope::Curated => "curated",
            Scope::Full => "full",
        }
    }

    /// Whether the scope takes `pair`.
    pub fn takes(self, pair: &Pair) -> bool {
        self == Scope::Full || pair.curated
    }
}

/// Parses the text of `corpus/pairs.tsv`.
pub fn parse(text: &str) -> Result<Vec<Pair>, String> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let c: Vec<&str> = line.split('\t').collect();
        let [class, set, original, revised, label, outlier] = c[..] else {
            return Err(format!(
                "pairs.tsv:{}: expected 6 columns, found {}",
                i + 1,
                c.len()
            ));
        };
        out.push(Pair {
            class: class.into(),
            curated: match set {
                "curated" => true,
                "full" => false,
                _ => return Err(format!("pairs.tsv:{}: bad set `{set}`", i + 1)),
            },
            original: original.into(),
            revised: revised.into(),
            label: label.into(),
            outlier: (outlier != "-").then(|| outlier.to_owned()),
            line: i + 1,
        });
    }
    Ok(out)
}

/// A pair with its two documents found in a set.
#[derive(Clone, Debug)]
pub struct Resolved {
    /// The manifest line.
    pub pair: Pair,
    /// The original document.
    pub original: Entry,
    /// The revised document.
    pub revised: Entry,
}

/// The pairs of `scope` with their documents in `corpus`, or the list of those it lacks.
pub fn resolve(pairs: &[Pair], corpus: &Corpus, scope: Scope) -> Result<Vec<Resolved>, String> {
    let by_sha: BTreeMap<&str, &Entry> = corpus
        .entries
        .iter()
        .map(|e| (e.sha256.as_str(), e))
        .collect();
    let mut out = Vec::new();
    let mut missing = Vec::new();
    for p in pairs.iter().filter(|p| scope.takes(p)) {
        match (
            by_sha.get(p.original.as_str()),
            by_sha.get(p.revised.as_str()),
        ) {
            (Some(a), Some(b)) => out.push(Resolved {
                pair: p.clone(),
                original: (*a).clone(),
                revised: (*b).clone(),
            }),
            _ => missing.push(format!("pairs.tsv:{} {}", p.line, p.label)),
        }
    }
    if missing.is_empty() {
        Ok(out)
    } else {
        Err(format!(
            "the set lacks the documents of {} pairs (fetch again; `--pairs` for the full run): {}",
            missing.len(),
            missing
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ))
    }
}

/// The set that holds the pairs of a run on `set`: the curated pairs have a set of their own beside the
/// curated corpus (`curatedpairs` for `curated`, `curatedpairs-<hash>` for `curated-<hash>`), so that they do
/// not change what the other corpus tests sample; the `pairs` and `full` sets hold their pairs themselves.
pub fn pair_set(set: &str) -> String {
    match set.strip_prefix("curated") {
        Some(rest) if Scope::of_set(set) == Scope::Curated => format!("curatedpairs{rest}"),
        _ => set.to_owned(),
    }
}

/// The pairs of the set that the environment names, as the corpus tests read it: `None` when
/// `STETDOC_CORPUS_DIR` is unset (the test skips), else the scope and the resolved pairs.
pub fn from_env() -> Option<Result<(Scope, Vec<Resolved>), String>> {
    let dirs = std::env::var(ENV_DIR).ok().filter(|d| !d.is_empty())?;
    let set = std::env::var(ENV_SET).unwrap_or_else(|_| "curated".into());
    let scope = Scope::of_set(&set);
    let name = pair_set(&set);
    let mut corpus = Err(format!("{ENV_DIR} names no usable directory"));
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        corpus = Corpus::open(std::path::Path::new(dir), &name);
        if corpus.is_ok() {
            break;
        }
    }
    Some(corpus.and_then(|corpus| {
        let pairs = parse(MANIFEST)?;
        Ok((scope, resolve(&pairs, &corpus, scope)?))
    }))
}

/// The noise of one redline against the minimal diff, as the core's test measures it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Figures {
    /// Characters of the minimal diff.
    pub min_chars: usize,
    /// Edits of the minimal diff.
    pub min_spans: usize,
    /// Characters inserted and deleted by the redline.
    pub red_chars: usize,
    /// Text changes of the redline.
    pub red_spans: usize,
    /// Formatting changes of the redline.
    pub fmt_spans: usize,
}

/// One row of the noise baseline.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Pairs of the class in the set.
    pub pairs: usize,
    /// Characters of their minimal diffs.
    pub min_chars: usize,
    /// Characters of their redlines.
    pub red_chars: usize,
    /// Their formatting changes.
    pub fmt_spans: usize,
    /// The pairs with exclusions.
    pub excluded: usize,
    /// The per-pair factor of the gate.
    pub c: f64,
}

/// The noise baseline: rows by set (`curated`, `full`) and class.
#[derive(Clone, Debug, Default)]
pub struct Baseline {
    rows: BTreeMap<(String, String), Row>,
}

impl Baseline {
    /// Parses the text of `noise_baseline.tsv`.
    pub fn parse(text: &str) -> Result<Baseline, String> {
        let mut rows = BTreeMap::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let c: Vec<&str> = line.split('\t').collect();
            let bad = || {
                format!(
                    "noise_baseline.tsv:{}: expected 9 columns of numbers",
                    i + 1
                )
            };
            let [set, class, pairs, min, red, fmt, excluded, _p99, c] = c[..] else {
                return Err(bad());
            };
            let n = |s: &str| s.parse::<usize>().map_err(|_| bad());
            let row = Row {
                pairs: n(pairs)?,
                min_chars: n(min)?,
                red_chars: n(red)?,
                fmt_spans: n(fmt)?,
                excluded: n(excluded)?,
                c: c.parse().map_err(|_| bad())?,
            };
            rows.insert((set.to_owned(), class.to_owned()), row);
        }
        Ok(Baseline { rows })
    }

    /// The committed baseline.
    pub fn committed() -> Baseline {
        Baseline::parse(BASELINE).unwrap_or_else(|e| panic!("{e}"))
    }

    /// The row of a class in a scope.
    pub fn row(&self, scope: Scope, class: &str) -> Option<&Row> {
        self.rows.get(&(scope.name().to_owned(), class.to_owned()))
    }

    /// The gate of a class: its `C` from the full row.
    pub fn gate(&self, class: &str) -> Option<Gate> {
        self.row(Scope::Full, class).map(|r| Gate { c: r.c })
    }
}

/// The per-pair gate of N9: `red_chars <= C * min_chars + 40` and `red_spans <= min_spans + 2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gate {
    /// The factor of the class.
    pub c: f64,
}

impl Gate {
    /// The characters a redline may have over its minimal diff on a small document.
    pub const SLACK_CHARS: f64 = 40.0;
    /// The changes a redline may have over the edits of the minimal diff.
    pub const SLACK_SPANS: usize = 2;

    /// Why the figures are above the gate, or `None` when they are within it.
    pub fn over(&self, f: &Figures) -> Option<String> {
        let mut why = Vec::new();
        if f.red_chars as f64 > self.c * f.min_chars as f64 + Self::SLACK_CHARS {
            why.push(format!(
                "chars {} > {:.2} * {} + 40",
                f.red_chars, self.c, f.min_chars
            ));
        }
        if f.red_spans > f.min_spans + Self::SLACK_SPANS {
            why.push(format!("spans {} > {} + 2", f.red_spans, f.min_spans));
        }
        (!why.is_empty()).then(|| why.join(", "))
    }

    /// What is wrong with a pair and its recorded outlier: an outlier that is not recorded, or a
    /// recorded one whose figures fell under the gate.
    pub fn verdict(&self, pair: &Pair, f: &Figures) -> Option<String> {
        match (self.over(f), &pair.outlier) {
            (Some(why), None) => Some(format!(
                "pairs.tsv:{} {}: an outlier that is not recorded ({why}); look at the redline and record the reason",
                pair.line, pair.label
            )),
            (None, Some(reason)) => Some(format!(
                "pairs.tsv:{} {}: recorded as an outlier (\"{reason}\") but within the gate (chars {}/{}, spans {}/{}); set its outlier column to -",
                pair.line, pair.label, f.red_chars, f.min_chars, f.red_spans, f.min_spans
            )),
            _ => None,
        }
    }
}

/// The totals of one class in a run, held against its baseline row: the aggregate character ratio and
/// the formatting changes may grow by 5% at most, and the pairs with exclusions not at all.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Pairs measured.
    pub pairs: usize,
    /// Sum of the minimal diffs' characters.
    pub min_chars: usize,
    /// Sum of the redlines' characters.
    pub red_chars: usize,
    /// Sum of the formatting changes.
    pub fmt_spans: usize,
    /// Pairs with exclusions.
    pub excluded: usize,
}

impl Totals {
    /// Adds one pair.
    pub fn add(&mut self, f: &Figures, excluded: bool) {
        self.pairs += 1;
        self.min_chars += f.min_chars;
        self.red_chars += f.red_chars;
        self.fmt_spans += f.fmt_spans;
        self.excluded += usize::from(excluded);
    }

    /// What exceeds the baseline row, if anything.
    pub fn against(&self, class: &str, row: &Row) -> Vec<String> {
        let mut out = Vec::new();
        let ratio = |red: usize, min: usize| red as f64 / min.max(1) as f64;
        let (now, then) = (
            ratio(self.red_chars, self.min_chars),
            ratio(row.red_chars, row.min_chars),
        );
        if now > then * 1.05 + 1e-9 {
            out.push(format!(
                "noise {class}: the aggregate char ratio {now:.3} is more than 5% over the baseline {then:.3}"
            ));
        }
        if self.fmt_spans as f64 > row.fmt_spans as f64 * 1.05 + 2.0 {
            out.push(format!(
                "noise {class}: {} formatting changes, more than 5% over the baseline {}",
                self.fmt_spans, row.fmt_spans
            ));
        }
        if self.excluded > row.excluded {
            out.push(format!(
                "{class}: {} pairs with exclusions, the baseline has {}",
                self.excluded, row.excluded
            ));
        }
        out
    }

    /// The baseline row these totals would make, for a person to paste when re-taking it.
    pub fn row_text(&self, scope: Scope, class: &str, p99: f64) -> String {
        let c = (p99 * 1.10 * 100.0 - 1e-9).ceil() / 100.0;
        format!(
            "{}\t{class}\t{}\t{}\t{}\t{}\t{}\t{p99:.3}\t{c:.2}",
            scope.name(),
            self.pairs,
            self.min_chars,
            self.red_chars,
            self.fmt_spans,
            self.excluded
        )
    }
}

/// The p99 of `red_chars / min_chars` over the figures whose minimal diff has 40 characters or more,
/// where the slack of the gate does not decide; 1.0 when there is none.
pub fn p99(figures: &[Figures]) -> f64 {
    let mut r: Vec<f64> = figures
        .iter()
        .filter(|f| f.min_chars >= 40)
        .map(|f| f.red_chars as f64 / f.min_chars as f64)
        .collect();
    r.sort_by(f64::total_cmp);
    let i = ((r.len() as f64) * 0.99) as usize;
    r.get(i.min(r.len().saturating_sub(1)))
        .copied()
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests;
