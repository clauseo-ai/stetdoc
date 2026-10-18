//! LibreOffice as an independent reader of stetdoc's outputs (`oracle::libreoffice`, `laws::libreoffice`).
//!
//! Every test skips where LibreOffice is not installed (CI has none). The sample run is ignored, because it takes minutes:
//! `cargo xtask oracle libreoffice` fetches the curated corpus and runs it, with `--sample N` (500) and `--seed S` (the fixed
//! default below, or `STETDOC_SEED`). It writes its report to `target/oracle/libreoffice/` (`report.md`, `outputs.tsv` and
//! the sampled outputs, which are never committed) and fails when an output does not load or convert, or when a mismatch is
//! not a known LibreOffice deviation or a limit of stetdoc's that FIDELITY.md documents: a stetdoc suspect fails the run.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "oracle_libreoffice/triage.rs"]
mod triage;
#[path = "oracle_libreoffice/white_space.rs"]
mod white_space;

use std::fmt::Write as _;
use std::path::PathBuf;
use stetdoc_core::{Batch, Document, Edit, Limits, OpenOptions, Stamp};
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::fixtures::contract;
use stetdoc_testkit::laws::libreoffice::{Output, Text, quotas, sample, text_of};
use stetdoc_testkit::oracle::libreoffice::{Reading, compare, normalize_all, read_all};

/// The seed of the sample when `STETDOC_SEED` is not set, so that two runs read the same 500 outputs.
const DEFAULT_SEED: u64 = 20_261_009;

fn edited(bytes: Vec<u8>, ops: &str) -> Vec<u8> {
    let mut doc = Document::open(bytes, &OpenOptions::default()).unwrap();
    let b = Batch::parse(
        &format!(r#"{{"base":"{}","ops":[{ops}]}}"#, doc.base()),
        &Limits::default(),
    )
    .unwrap();
    doc.edit(Edit::ops(b, Stamp::new("Clauseo", "2026-10-09T10:00:00")))
        .unwrap();
    doc.save().unwrap().bytes
}

fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("stetdoc-lo-{}-{name}.docx", std::process::id()));
    std::fs::write(&path, bytes).unwrap();
    path
}

/// What the comparison found on one output.
#[derive(Clone, Debug, Default)]
struct Verdict {
    load: bool,
    pdf: bool,
    /// `None` when the texts match, else the first difference.
    accepted: Option<String>,
    rejected: Option<String>,
    notes: Option<String>,
    /// The triage of a mismatch: a LibreOffice deviation or a stetdoc suspect, by name.
    triage: Option<String>,
    /// What is left of a mismatch in each ablation of the triage, when none made it go away.
    after: Vec<String>,
}

fn verdict(text: &Result<Text, String>, r: &Reading) -> Verdict {
    let mut v = Verdict {
        load: r.load.is_none() && r.error.is_none(),
        pdf: r.load.is_none() && r.pdf.is_none() && r.error.is_none(),
        ..Verdict::default()
    };
    let text = match text {
        Ok(t) => t,
        Err(e) => {
            v.accepted = Some(format!("stetdoc cannot read its own output: {e}"));
            return v;
        }
    };
    let (Some(acc), Some(rej)) = (&r.accept, &r.reject) else {
        v.accepted = Some(format!(
            "LibreOffice gave no text: {}",
            r.error.as_deref().or(r.load.as_deref()).unwrap_or("?")
        ));
        return v;
    };
    v.accepted = compare(&text.final_body, &normalize_all(acc)).err();
    v.rejected = compare(&text.original_body, &normalize_all(rej)).err();
    let notes = [
        compare(&text.final_notes, &normalize_all(&r.notes_accept)).err(),
        compare(&text.original_notes, &normalize_all(&r.notes_reject)).err(),
    ];
    v.notes = notes.into_iter().flatten().next();
    v
}

#[test]
fn a_tracked_edit_reads_the_same_in_libreoffice_and_in_stetdoc() {
    let bytes = edited(
        contract::package_rich(),
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"},{"op":"merge","ref":"p2","second":"p3"},{"op":"insert","ref":"p1","place":"after","paragraphs":["A new clause."]},{"op":"delete","ref":"p6"}"#,
    );
    let path = write_temp("tracked", &bytes);
    let read = read_all(std::slice::from_ref(&path));
    let _ = std::fs::remove_file(&path);
    let Some(read) = read else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let read = read.unwrap();
    let text = text_of(&bytes);
    let v = verdict(&text, &read[0]);
    assert!(v.load && v.pdf, "{:?}", read[0]);
    assert_eq!((&v.accepted, &v.rejected, &v.notes), (&None, &None, &None));
    let text = text.unwrap();
    assert!(text.final_body.iter().any(|l| l.contains("sixty")));
    assert!(text.original_body.iter().any(|l| l.contains("thirty")));
    // A failing case: stetdoc's Final text against LibreOffice's rejected text differs, and the difference is named.
    let e = compare(
        &text.final_body,
        &normalize_all(read[0].reject.as_ref().unwrap()),
    )
    .unwrap_err();
    assert!(e.contains("line"), "{e}");
}

#[test]
fn the_quotas_of_a_sample_add_up_and_cover_every_kind() {
    let q = quotas(500);
    assert_eq!(q.iter().map(|(_, n)| n).sum::<usize>(), 500);
    assert_eq!(q.len(), 10);
    assert!(q.iter().all(|(_, n)| *n >= 35), "{q:?}");
    assert_eq!(quotas(7).iter().map(|(_, n)| n).sum::<usize>(), 7);
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|s| s.trim().parse().ok())
}

#[test]
#[ignore = "minutes long; run by `cargo xtask oracle libreoffice`"]
fn libreoffice_reads_a_seeded_sample_of_stetdoc_outputs() {
    if stetdoc_testkit::oracle::libreoffice::soffice().is_none() {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    }
    let Some(corpus) = Corpus::from_env() else {
        eprintln!(
            "skipped: STETDOC_CORPUS_DIR is not set (cargo xtask oracle libreoffice sets it)"
        );
        return;
    };
    let corpus = corpus.unwrap();
    let pairs = stetdoc_testkit::pairs::from_env()
        .map(|p| p.unwrap().1)
        .unwrap_or_default();
    let seed = env_u64("STETDOC_SEED").unwrap_or(DEFAULT_SEED);
    let n = env_u64("STETDOC_LO_SAMPLE").unwrap_or(500) as usize;
    eprintln!(
        "LibreOffice oracle: a sample of {n} outputs, seed {seed} (STETDOC_SEED={seed} makes the same sample)"
    );
    let mut outputs = sample(&corpus, &pairs, seed, n);
    // `STETDOC_LO_ONLY=3,17` reads only those outputs of the sample (numbered as in the report), to look at them again.
    if let Ok(only) = std::env::var("STETDOC_LO_ONLY") {
        let keep: Vec<usize> = only
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        outputs = outputs
            .into_iter()
            .enumerate()
            .filter(|(i, _)| keep.contains(i))
            .map(|(_, o)| o)
            .collect();
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/oracle/libreoffice");
    let docs = dir.join("outputs");
    let _ = std::fs::remove_dir_all(&docs);
    std::fs::create_dir_all(&docs).unwrap();
    let paths: Vec<PathBuf> = outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let p = docs.join(format!("{i:03}.docx"));
            std::fs::write(&p, &o.bytes).unwrap();
            p
        })
        .collect();
    let readings = read_all(&paths).unwrap().unwrap();
    assert_eq!(readings.len(), outputs.len());
    let diffs = dir.join("diffs");
    let _ = std::fs::remove_dir_all(&diffs);
    std::fs::create_dir_all(&diffs).unwrap();
    let mut verdicts: Vec<Verdict> = outputs
        .iter()
        .zip(&readings)
        .enumerate()
        .map(|(i, (o, r))| {
            let text = text_of(&o.bytes);
            let v = verdict(&text, r);
            if mismatch(&v) {
                std::fs::write(diffs.join(format!("{i:03}.txt")), side_by_side(o, r, &text))
                    .unwrap();
            }
            v
        })
        .collect();
    triage::all(&outputs, &paths, &readings, &mut verdicts);
    let report = report(n, seed, &outputs, &readings, &verdicts);
    std::fs::write(dir.join("report.md"), &report.0).unwrap();
    std::fs::write(dir.join("outputs.tsv"), &report.1).unwrap();
    eprintln!("{}", report.0);
    eprintln!("written to {}", dir.display());
    let untriaged: Vec<String> = outputs
        .iter()
        .zip(&verdicts)
        .enumerate()
        .filter(|(_, (_, v))| {
            !v.load
                || !v.pdf
                || (mismatch(v)
                    && v.triage.as_deref().is_none_or(|t| {
                        t.contains("not yet explained") || t.contains("stetdoc suspect")
                    }))
        })
        .map(|(i, (o, _))| format!("{i:03} {} {} seed {}", o.kind, o.id, o.seed))
        .collect();
    assert!(
        untriaged.is_empty(),
        "{} outputs failed to load or convert or have an untriaged mismatch:\n{}",
        untriaged.len(),
        untriaged.join("\n")
    );
}

fn mismatch(v: &Verdict) -> bool {
    v.accepted.is_some() || v.rejected.is_some() || v.notes.is_some()
}

/// Both sides of a mismatch in full, for triage: stetdoc's lines and LibreOffice's, normalized, in four sections.
fn side_by_side(o: &Output, r: &Reading, text: &Result<Text, String>) -> String {
    let mut s = format!(
        "{} {} {} seed {}\n{}\n",
        o.kind, o.family, o.id, o.seed, o.batch
    );
    let Ok(t) = text else {
        return s;
    };
    let empty = Vec::new();
    for (name, a, b) in [
        (
            "Final / accept",
            &t.final_body,
            r.accept.as_ref().unwrap_or(&empty),
        ),
        (
            "Original / reject",
            &t.original_body,
            r.reject.as_ref().unwrap_or(&empty),
        ),
        ("notes Final / accept", &t.final_notes, &r.notes_accept),
        (
            "notes Original / reject",
            &t.original_notes,
            &r.notes_reject,
        ),
    ] {
        let b = normalize_all(b);
        writeln!(
            s,
            "\n== {name}: {}",
            compare(a, &b).err().unwrap_or("ok".into())
        )
        .unwrap();
        for k in 0..a.len().max(b.len()) {
            let x = a.get(k).map_or(String::new(), |l| {
                l.replace('\u{e000}', "{F}").replace('\u{e001}', "{OPAQUE}")
            });
            writeln!(
                s,
                "{k:4} S {x}\n     L {}",
                b.get(k).map_or("", |l| l.as_str())
            )
            .unwrap();
        }
    }
    s
}

fn report(
    n: usize,
    seed: u64,
    outputs: &[Output],
    readings: &[Reading],
    verdicts: &[Verdict],
) -> (String, String) {
    let count = |f: &dyn Fn(&Verdict) -> bool| verdicts.iter().filter(|v| f(v)).count();
    let mut md = String::new();
    writeln!(
        md,
        "# LibreOffice oracle: {} outputs (asked {n}), seed {seed}\n",
        outputs.len()
    )
    .unwrap();
    writeln!(md, "- load: {}", count(&|v| v.load)).unwrap();
    writeln!(md, "- convert to PDF: {}", count(&|v| v.pdf)).unwrap();
    writeln!(
        md,
        "- accept-all body matches the Final view: {}",
        count(&|v| v.load && v.accepted.is_none())
    )
    .unwrap();
    writeln!(
        md,
        "- reject-all body matches the Original view: {}",
        count(&|v| v.load && v.rejected.is_none())
    )
    .unwrap();
    writeln!(
        md,
        "- notes match in both: {}",
        count(&|v| v.load && v.notes.is_none())
    )
    .unwrap();
    writeln!(
        md,
        "- all three match: {}",
        count(&|v| v.load && !mismatch(v))
    )
    .unwrap();
    writeln!(
        md,
        "\n## By kind\n\n| kind | outputs | load | pdf | all match |\n|---|---|---|---|---|"
    )
    .unwrap();
    let mut kinds: Vec<&str> = outputs.iter().map(|o| o.kind.as_str()).collect();
    kinds.dedup();
    for k in kinds {
        let of: Vec<&Verdict> = outputs
            .iter()
            .zip(verdicts)
            .filter(|(o, _)| o.kind == k)
            .map(|(_, v)| v)
            .collect();
        writeln!(
            md,
            "| {k} | {} | {} | {} | {} |",
            of.len(),
            of.iter().filter(|v| v.load).count(),
            of.iter().filter(|v| v.pdf).count(),
            of.iter().filter(|v| v.load && !mismatch(v)).count()
        )
        .unwrap();
    }
    writeln!(md, "\n## Mismatches by triage\n").unwrap();
    let mut by: std::collections::BTreeMap<String, Vec<usize>> = Default::default();
    for (i, v) in verdicts.iter().enumerate() {
        if mismatch(v) || !v.load || !v.pdf {
            by.entry(v.triage.clone().unwrap_or_else(|| "untriaged".into()))
                .or_default()
                .push(i);
        }
    }
    for (t, list) in &by {
        writeln!(md, "### {t} ({})\n", list.len()).unwrap();
        for i in list {
            let (o, v, r) = (&outputs[*i], &verdicts[*i], &readings[*i]);
            writeln!(
                md,
                "- {i:03} {} {} `{}` seed {} left {:?}/{:?}\n  - batch: {}\n  - accept: {}\n  - reject: {}\n  - notes: {}",
                o.kind,
                o.family,
                o.id,
                o.seed,
                r.left_accept,
                r.left_reject,
                cut(&o.batch),
                v.accepted.as_deref().unwrap_or("ok"),
                v.rejected.as_deref().unwrap_or("ok"),
                v.notes.as_deref().unwrap_or("ok"),
            )
            .unwrap();
            for a in &v.after {
                writeln!(md, "  - after the ablation {a}").unwrap();
            }
            if !v.load || !v.pdf {
                writeln!(
                    md,
                    "  - load {:?} pdf {:?} error {:?}",
                    r.load, r.pdf, r.error
                )
                .unwrap();
            }
        }
        md.push('\n');
    }
    let mut tsv =
        String::from("n\tkind\tfamily\tid\tseed\tload\tpdf\taccepted\trejected\tnotes\ttriage\n");
    for (i, (o, v)) in outputs.iter().zip(verdicts).enumerate() {
        let ok = |x: &Option<String>| if x.is_none() { "ok" } else { "differs" };
        writeln!(
            tsv,
            "{i:03}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            o.kind,
            o.family,
            o.id,
            o.seed,
            v.load,
            v.pdf,
            ok(&v.accepted),
            ok(&v.rejected),
            ok(&v.notes),
            v.triage.as_deref().unwrap_or("-")
        )
        .unwrap();
    }
    (md, tsv)
}

fn cut(s: &str) -> String {
    if s.chars().count() > 300 {
        format!("{}...", s.chars().take(300).collect::<String>())
    } else {
        s.to_string()
    }
}
