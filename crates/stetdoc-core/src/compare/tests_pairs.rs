//! The compare laws on the compare pairs of the corpus: `corpus/pairs.tsv` lists each pair by the hashes of its original and revised document,
//! and the testkit's `pairs` module resolves them against the set that `STETDOC_CORPUS_DIR` and `STETDOC_CORPUS_SET` name. On the
//! curated set (what `cargo xtask check` fetches) the curated pairs run; on the `pairs` set (`cargo xtask corpus fetch --pairs`)
//! or the `full` set every pair runs. With the variable unset the test skips and passes, as the corpus tests do.
//!
//! For each pair the laws N1 (accepting gives the revised document), N2 (rejecting gives the original), N4 (what no call wrote to
//! is untouched) and N7 (a move is a move) are checked, and the noise of N9 is measured. The blocks that the receipt excludes (each
//! with its reason) are left out of both dumps and everything else is compared, so a pair with any difference left fails the test,
//! and so does a pair with an error (a call that failed half way is one).
//!
//! The noise gate (design 9.4): a pair above `red_chars <= C * min_chars + 40` or `red_spans <= min_spans + 2` is an outlier, with
//! `C` of its class from `stetdoc-testkit/bench/noise_baseline.tsv`. An outlier must be recorded with its reason in the pair
//! manifest, and a recorded one must still be above the gate. Per class, the aggregate character ratio and the formatting changes
//! may not grow by more than 5% over the baseline row of the set, and the pairs with exclusions not at all. The rows this run would
//! make are printed, for re-taking the baseline by hand. With `STETDOC_N_OUT` set each pair is written there with its verdict and
//! its noise, for the review of outliers, and `STETDOC_N_SHOW` prints the redline's XML around a needle.

use super::tests_laws::{check_pair, resolved};
use crate::revisions::types::Action;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use stetdoc_testkit::pairs::{Baseline, Figures, Totals, p99};

#[test]
fn the_laws_hold_on_the_compare_pairs_of_the_corpus() {
    let Some(found) = stetdoc_testkit::pairs::from_env() else {
        return;
    };
    let (scope, pairs) = found.unwrap_or_else(|e| panic!("the compare pairs: {e}"));
    let baseline = Baseline::committed();
    let (mut n, mut failed, mut errors, mut excluded_pairs, mut exclusions, mut broken) =
        (0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut out = String::new();
    let mut details = String::new();
    let mut totals: BTreeMap<String, Totals> = BTreeMap::new();
    let mut figures: BTreeMap<String, Vec<Figures>> = BTreeMap::new();
    let mut gate_problems: Vec<String> = Vec::new();
    let mut outliers = 0usize;
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for r in &pairs {
        let label = r.pair.label.as_str();
        let class = r.pair.class.as_str();
        let (x, y) = (r.original.bytes(), r.revised.bytes());
        let verdict = check_pair(x, y);
        n += 1;
        let (a, b) = (
            r.original.id.rsplit('/').next().unwrap_or(&r.original.id),
            r.revised.id.rsplit('/').next().unwrap_or(&r.revised.id),
        );
        match verdict {
            Err(e) => {
                errors += 1;
                let _ = writeln!(out, "{label}\terror\t{a}\t{b}\t{e}");
                let _ = writeln!(details, "ERROR {label} {a} {b}: {e}");
            }
            Ok(v) => {
                let excl = v.receipt.exclusions.len();
                exclusions += excl;
                broken += v.receipt.broken;
                // the excluded blocks are already left out of both dumps: any difference that is left is a failure
                let state = if v.failures.is_empty() { "ok" } else { "FAIL" };
                if excl > 0 {
                    excluded_pairs += 1;
                    for e in &v.receipt.exclusions {
                        let reason: String = e.reason.chars().take(60).collect();
                        *reasons.entry(reason).or_insert(0usize) += 1;
                    }
                }
                if state == "FAIL" {
                    failed += 1;
                    let _ = writeln!(details, "FAIL {label} ({a} {b})\n{}", v.failures.join("\n"));
                    for e in &v.receipt.exclusions {
                        let _ = writeln!(
                            details,
                            "  excluded: {} redline {:?} original {:?}",
                            e.reason,
                            e.redline.iter().map(|k| k.node()).collect::<Vec<_>>(),
                            e.original.iter().map(|k| k.node()).collect::<Vec<_>>()
                        );
                    }
                }
                let nz = &v.noise;
                let f = Figures {
                    min_chars: nz.min_chars,
                    min_spans: nz.min_spans,
                    red_chars: nz.red_chars,
                    red_spans: nz.red_spans,
                    fmt_spans: nz.fmt_spans,
                };
                totals
                    .entry(class.to_owned())
                    .or_default()
                    .add(&f, excl > 0);
                figures.entry(class.to_owned()).or_default().push(f);
                match baseline.gate(class) {
                    None => gate_problems
                        .push(format!("class {class} has no row in the noise baseline")),
                    Some(gate) => {
                        outliers += usize::from(gate.over(&f).is_some());
                        gate_problems.extend(gate.verdict(&r.pair, &f));
                    }
                }
                let _ = writeln!(
                    out,
                    "{label}\t{state}\t{a}\t{b}\texcl {excl}\tbroken {}\tchars {}/{}\tspans {}/{}\tfmt {}\twarn {}\t{:?}",
                    v.receipt.broken,
                    nz.red_chars,
                    nz.min_chars,
                    nz.red_spans,
                    nz.min_spans,
                    nz.fmt_spans,
                    v.receipt
                        .warnings
                        .iter()
                        .map(|w| format!("{}x{}", w.code, w.count))
                        .collect::<Vec<_>>()
                        .join(","),
                    v.receipt
                        .exclusions
                        .iter()
                        .map(|e| e.reason.clone())
                        .collect::<Vec<_>>()
                );
                if let Ok(needle) = std::env::var("STETDOC_N_SHOW") {
                    let view = v.redline.parts();
                    let id = view.id("word/document.xml").expect("a body");
                    let xml = String::from_utf8_lossy(&view.tree(id).expect("a tree").serialize())
                        .into_owned();
                    if let Some(i) = xml.find(&needle) {
                        let mut from = xml[..i]
                            .rfind("<w:p>")
                            .or_else(|| xml[..i].rfind("<w:p "))
                            .unwrap_or(i)
                            .saturating_sub(1500);
                        let mut to = (i + 2500).min(xml.len());
                        while !xml.is_char_boundary(from) {
                            from += 1;
                        }
                        while !xml.is_char_boundary(to) {
                            to += 1;
                        }
                        let _ = writeln!(details, "XML {label}\n{}", &xml[from..to]);
                        if let Ok(rej) = resolved(&v.redline, Action::Reject) {
                            let view = rej.parts();
                            let id = view.id("word/document.xml").expect("a body");
                            let rx = String::from_utf8_lossy(
                                &view.tree(id).expect("a tree").serialize(),
                            )
                            .into_owned();
                            let b = rx.find("<w:body>").unwrap_or(0);
                            let _ = writeln!(
                                details,
                                "REJECTED {label}\n{}",
                                &rx[b..(b + 3000).min(rx.len())]
                            );
                        }
                    }
                }
            }
        }
    }
    println!(
        "compare pairs ({}): {n}: {failed} fail the laws, {errors} errors, {broken} broken calls, {excluded_pairs} pairs with exclusions ({exclusions} exclusions), {outliers} noise outliers",
        scope.name()
    );
    let mut over_baseline = Vec::new();
    for (class, t) in &totals {
        let p = p99(figures.get(class).map(Vec::as_slice).unwrap_or_default());
        println!(
            "noise {class}: {} pairs, chars {:.3}, p99 {p:.3}, fmt spans {}; row: {}",
            t.pairs,
            t.red_chars as f64 / t.min_chars.max(1) as f64,
            t.fmt_spans,
            t.row_text(scope, class, p)
        );
        match baseline.row(scope, class) {
            Some(row) => over_baseline.extend(t.against(class, row)),
            None => over_baseline.push(format!(
                "class {class} has no {} row in the noise baseline",
                scope.name()
            )),
        }
    }
    for (r, k) in &reasons {
        println!("exclusion x{k}: {r}");
    }
    if let Ok(path) = std::env::var("STETDOC_N_OUT") {
        std::fs::write(&path, &out).expect("the verdicts write");
        std::fs::write(format!("{path}.details"), &details).expect("the details write");
    }
    assert!(
        failed == 0 && errors == 0 && broken == 0,
        "the laws fail on {failed} pairs, {errors} errors, {broken} broken\n{details}"
    );
    assert!(
        gate_problems.is_empty() && over_baseline.is_empty(),
        "the noise gate of N9:\n{}",
        gate_problems
            .iter()
            .chain(&over_baseline)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
