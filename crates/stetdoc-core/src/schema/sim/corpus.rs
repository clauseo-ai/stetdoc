//! Corpus runs of the edit simulation and of the validator, through the real span tree. They read
//! documents, so they are named in `xtask/core-io-exceptions.txt`. The corpus is fetched into a
//! cache by `cargo xtask corpus fetch` and never committed.
//!
//! Where the documents come from, by the rules of `package/corpus_laws.rs`:
//! - `STETDOC_CORPUS_LIST` names a file with one package per line, or
//! - `STETDOC_CORPUS_DIR` names directories, separated by `:`. In each, `sets/<name>` is read
//!   when it exists, where `<name>` is `STETDOC_CORPUS_SET` (the set `cargo xtask check` names),
//!   or `curated`; otherwise the directory itself is walked for `.docx` files. A set has an
//!   `INDEX.tsv`, and only the documents it expects to be `ok` are read.
//!
//! `STETDOC_SIM_STRIDE` samples every n-th parent (default 40), `STETDOC_SIM_ITEMS` caps the
//! insertions and removals tried per parent (default 6), `STETDOC_SIM_EVERY` reads every n-th
//! document (default 1) starting at the `STETDOC_SIM_OFFSET`-th (default 0), so that a corpus can
//! be split over several runs, and `STETDOC_SIM_MAX_PART` skips parts larger than that many bytes
//! (default 4,000,000).
//!
//! The runs that read a whole corpus are `#[ignore]`d. One run over the curated set is not: it
//! is the check that the span tree, its editor and the schema work together on real documents,
//! and it passes by skipping only when no corpus is named, which `cargo xtask check` always does.

use super::*;
use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

const PARTS: [(&str, PartKind); 7] = [
    ("word/document.xml", PartKind::Document),
    ("word/styles.xml", PartKind::Styles),
    ("word/numbering.xml", PartKind::Numbering),
    ("word/comments.xml", PartKind::Comments),
    ("word/footnotes.xml", PartKind::Footnotes),
    ("word/endnotes.xml", PartKind::Endnotes),
    ("word/settings.xml", PartKind::Settings),
];

/// One document to read, with what the set's index says about it.
#[cfg(test)]
#[derive(Clone, Debug)]
struct Doc {
    path: PathBuf,
    /// The tier of the set (`lab`, `word-authored`, ...), empty without an index.
    tier: String,
    /// Whether the index says the document carries Word's own fingerprints.
    word_fp: bool,
}

// The helpers carry `#[cfg(test)]` themselves: the I/O check reads one file at a time and
// decides what is test code from the attributes it finds in that file.
#[cfg(test)]
fn number(var: &str, default: usize) -> usize {
    std::env::var(var)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// The documents under `dir`, by walking it.
#[cfg(test)]
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "docx") {
                files.push(p);
            }
        }
    }
    files
}

/// The documents of one set, from its `INDEX.tsv`: those it expects to be `ok`.
#[cfg(test)]
fn indexed(set: &Path, index: &str) -> Vec<Doc> {
    let mut lines = index.lines();
    let header: Vec<&str> = lines.next().unwrap_or("").split('\t').collect();
    let col = |name: &str| header.iter().position(|h| *h == name);
    let (Some(path), Some(expect)) = (col("path"), col("expect")) else {
        return Vec::new();
    };
    let (tier, fp) = (col("tier"), col("word_fp"));
    lines
        .map(|l| l.split('\t').collect::<Vec<_>>())
        .filter(|c| c.get(expect) == Some(&"ok"))
        .filter_map(|c| {
            Some(Doc {
                path: set.join(c.get(path)?),
                tier: tier.and_then(|i| c.get(i)).unwrap_or(&"").to_string(),
                word_fp: fp.and_then(|i| c.get(i)) == Some(&"y"),
            })
        })
        .collect()
}

/// The documents to read, sorted, or `None` when no corpus is named.
#[cfg(test)]
fn corpus_docs() -> Option<Vec<Doc>> {
    let plain = |p: PathBuf| Doc {
        path: p,
        tier: String::new(),
        word_fp: false,
    };
    let mut docs: Vec<Doc> = Vec::new();
    if let Ok(list) = std::env::var("STETDOC_CORPUS_LIST") {
        let text = std::fs::read_to_string(list).ok()?;
        docs.extend(
            text.lines()
                .filter(|l| !l.is_empty())
                .map(|l| plain(PathBuf::from(l))),
        );
    } else {
        let dirs = std::env::var("STETDOC_CORPUS_DIR").ok()?;
        let name = std::env::var("STETDOC_CORPUS_SET").unwrap_or_else(|_| "curated".to_owned());
        for dir in dirs.split(':').filter(|d| !d.is_empty()) {
            let set = Path::new(dir).join("sets").join(&name);
            match std::fs::read_to_string(set.join("INDEX.tsv")) {
                Ok(index) => docs.extend(indexed(&set, &index)),
                Err(_) => docs.extend(walk(Path::new(dir)).into_iter().map(plain)),
            }
        }
    }
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    let every = number("STETDOC_SIM_EVERY", 1).max(1);
    let offset = number("STETDOC_SIM_OFFSET", 0);
    Some(docs.into_iter().skip(offset).step_by(every).collect())
}

/// Calls `f` with the document, the kind and the bytes of every part of every readable package.
/// Returns the number of documents and of parts.
#[cfg(test)]
fn for_each_part(docs: &[Doc], mut f: impl FnMut(&Doc, PartKind, &[u8])) -> (usize, usize) {
    let cap = number("STETDOC_SIM_MAX_PART", 4_000_000) as u64;
    let (mut read, mut parts) = (0, 0);
    for doc in docs {
        let Ok(file) = std::fs::File::open(&doc.path) else {
            continue;
        };
        let Ok(mut zip) = zip::ZipArchive::new(file) else {
            continue;
        };
        read += 1;
        for (name, kind) in PARTS {
            let Ok(entry) = zip.by_name(name) else {
                continue;
            };
            let mut bytes = Vec::new();
            if entry.size() > cap || entry.take(cap).read_to_end(&mut bytes).is_err() {
                continue;
            }
            parts += 1;
            f(doc, kind, &bytes);
        }
    }
    (read, parts)
}

/// Runs the simulation over the documents and returns the tally with the counts read.
#[cfg(test)]
fn run_simulation(docs: &[Doc], stride: usize, items: usize) -> (Tally, usize, usize) {
    let mut tally = Tally::default();
    let (read, parts) = for_each_part(docs, |doc, kind, bytes| {
        tally.context = doc.path.display().to_string();
        simulate(bytes, kind, stride, items, &mut tally);
    });
    (tally, read, parts)
}

#[cfg(test)]
fn print_tally(tally: &Tally, docs: usize, parts: usize) {
    println!(
        "documents {docs}, parts {parts}, inserts {}, by fit {:?}, refused inserts {}",
        tally.inserts, tally.by_fit, tally.refused_inserts
    );
    println!(
        "removals {} ({} rewrote a property container), refused removals {}, refused by the editor {}",
        tally.removals, tally.rewritten_removals, tally.refused_removals, tally.editor_refusals
    );
    println!(
        "parents clean before: {}, rule A failures: {}, rule B failures: {}",
        tally.parent_clean,
        tally.rule_a_failures.len(),
        tally.rule_b_failures.len()
    );
    let mut seen = BTreeSet::new();
    for f in tally
        .rule_a_failures
        .iter()
        .filter(|f| seen.insert(f.split(" children ").next().unwrap_or("").to_string()))
        .take(40)
    {
        println!("A: {f}");
    }
    for f in tally.rule_b_failures.iter().take(15) {
        println!("B: {f}");
    }
}

/// Runs the simulation over every package named and reports the failures of the two rules.
#[test]
#[ignore = "reads the corpus named by STETDOC_CORPUS_LIST or STETDOC_CORPUS_DIR"]
fn corpus_edit_simulation() {
    let Some(docs) = corpus_docs() else {
        return;
    };
    let (stride, items) = (
        number("STETDOC_SIM_STRIDE", 40),
        number("STETDOC_SIM_ITEMS", 6),
    );
    let (tally, read, parts) = run_simulation(&docs, stride, items);
    print_tally(&tally, read, parts);
    assert!(tally.rule_b_failures.is_empty());
    assert!(tally.rule_a_failures.is_empty());
    assert_eq!(tally.editor_refusals, 0);
}

/// The two rules on the curated set, with the real span tree and its editor. A sample of the
/// parents of every document the index expects to be `ok`, small enough to run with every
/// `cargo xtask check`. It skips when no corpus is named, and `cargo xtask check` always names
/// one.
#[test]
fn the_two_rules_hold_on_the_curated_corpus() {
    if std::env::var("STETDOC_CORPUS_LIST").is_err() && std::env::var("STETDOC_CORPUS_DIR").is_err()
    {
        eprintln!("STETDOC_CORPUS_DIR is not set: the curated corpus is not read");
        return;
    }
    let Some(docs) = corpus_docs() else {
        panic!("the corpus named by STETDOC_CORPUS_DIR or STETDOC_CORPUS_LIST cannot be read");
    };
    // Every fourth document, so that a debug build takes seconds. `STETDOC_SIM_EVERY` already
    // thinned the list when it is set.
    let docs: Vec<Doc> = docs.into_iter().step_by(4).collect();
    let (tally, read, parts) = run_simulation(&docs, 300, 3);
    print_tally(&tally, read, parts);
    // It must have read something: a corpus that is empty or unreadable is not a pass.
    assert!(
        read >= docs.len() / 2 && read > 0,
        "{read} of {} documents read",
        docs.len()
    );
    assert!(tally.inserts > 500, "{} insertions", tally.inserts);
    assert!(tally.removals > 300, "{} removals", tally.removals);
    assert!(
        tally.rule_b_failures.is_empty(),
        "{:#?}",
        &tally.rule_b_failures[..tally.rule_b_failures.len().min(10)]
    );
    assert!(
        tally.rule_a_failures.is_empty(),
        "{:#?}",
        &tally.rule_a_failures[..tally.rule_a_failures.len().min(10)]
    );
    assert_eq!(tally.editor_refusals, 0);
}

/// The law of `walk_unit` on real documents: for a sample of the paragraphs and tables of the
/// curated set, the walk visits each element once, in document order, and every element the
/// validator checks in the unit that Word reads. It skips when no corpus is named, and
/// `cargo xtask check` always names one.
#[test]
fn the_unit_walk_covers_what_the_validator_checks_on_the_curated_corpus() {
    if std::env::var("STETDOC_CORPUS_LIST").is_err() && std::env::var("STETDOC_CORPUS_DIR").is_err()
    {
        eprintln!("STETDOC_CORPUS_DIR is not set: the curated corpus is not read");
        return;
    }
    let Some(docs) = corpus_docs() else {
        panic!("the corpus named by STETDOC_CORPUS_DIR or STETDOC_CORPUS_LIST cannot be read");
    };
    let docs: Vec<Doc> = docs.into_iter().step_by(4).collect();
    let stride = number("STETDOC_SIM_STRIDE", 20).max(1);
    let (mut units, mut failures) = (0usize, Vec::new());
    for_each_part(&docs, |doc, kind, bytes| {
        let Some(sim) = SimTree::parse(bytes) else {
            return;
        };
        let roots = sim
            .elements()
            .into_iter()
            .filter(|n| is_unit_boundary(&sim.tree, *n));
        for unit in roots.step_by(stride) {
            units += 1;
            if let Err(why) = super::super::tests::walk::walk_law(kind, &sim.tree, unit) {
                failures.push(format!("{}: {why}", doc.path.display()));
            }
        }
    });
    println!("units {units}, failures {}", failures.len());
    assert!(units > 600, "{units} units");
    assert!(
        failures.is_empty(),
        "{:#?}",
        &failures[..failures.len().min(10)]
    );
}

/// Validates every part of every package named and reports how many documents have a violation,
/// by tier and by Word fingerprint, and which violations are the commonest. It asserts nothing:
/// documents in the wild have problems, and the point is to read them.
#[test]
#[ignore = "reads the corpus named by STETDOC_CORPUS_LIST or STETDOC_CORPUS_DIR"]
fn corpus_validation() {
    let Some(docs) = corpus_docs() else {
        return;
    };
    // (kind, parent, subject) -> (documents, violations)
    let mut by_key: BTreeMap<(String, String, String), (BTreeSet<PathBuf>, u64)> = BTreeMap::new();
    let mut with_violation: BTreeSet<PathBuf> = BTreeSet::new();
    let mut refused_parts = 0usize;
    let (read, parts) = for_each_part(&docs, |doc, kind, bytes| {
        let Some(tree) = SimTree::parse(bytes) else {
            refused_parts += 1;
            return;
        };
        let report = validate_part(kind, &tree.tree);
        if !report.set.is_empty() {
            with_violation.insert(doc.path.clone());
        }
        // The report keeps the precise kind (`too_many`); the first MAX_DETAILS of a part.
        for v in &report.violations {
            let e = by_key
                .entry((
                    v.kind.name().to_string(),
                    v.key.parent.clone(),
                    v.key.subject.clone(),
                ))
                .or_default();
            e.0.insert(doc.path.clone());
            e.1 += 1;
        }
    });
    println!(
        "documents {read}, parts {parts}, parts the span tree refused {refused_parts}, with a violation {}",
        with_violation.len()
    );
    let mut groups: BTreeMap<(String, bool), (usize, usize)> = BTreeMap::new();
    for d in &docs {
        let g = groups.entry((d.tier.clone(), d.word_fp)).or_default();
        g.0 += 1;
        g.1 += usize::from(with_violation.contains(&d.path));
    }
    for ((tier, fp), (n, bad)) in &groups {
        println!(
            "tier {tier:<16} word fingerprint {:<3} documents {n:>6}  with a violation {bad:>6}",
            if *fp { "yes" } else { "no" }
        );
    }
    let mut rows: Vec<_> = by_key.iter().collect();
    rows.sort_by_key(|(k, (d, n))| {
        (
            std::cmp::Reverse(d.len()),
            std::cmp::Reverse(*n),
            (*k).clone(),
        )
    });
    for ((kind, parent, subject), (d, n)) in rows.into_iter().take(40) {
        println!(
            "{:>6} docs {n:>8} x  {kind} | {parent} | {subject}",
            d.len()
        );
    }
}

/// Every XML part under `word/` of every readable package, with its name and bytes.
#[cfg(test)]
fn for_each_word_part(docs: &[Doc], mut f: impl FnMut(&Doc, &str, &[u8])) {
    let cap = number("STETDOC_SIM_MAX_PART", 4_000_000) as u64;
    for doc in docs {
        let Ok(file) = std::fs::File::open(&doc.path) else {
            continue;
        };
        let Ok(mut zip) = zip::ZipArchive::new(file) else {
            continue;
        };
        for i in 0..zip.len() {
            let Ok(entry) = zip.by_index(i) else {
                continue;
            };
            let Ok(name) = entry.name().map(|n| n.into_owned()) else {
                continue;
            };
            if !name.starts_with("word/") || !name.ends_with(".xml") || entry.size() > cap {
                continue;
            }
            let mut bytes = Vec::new();
            if entry.take(cap).read_to_end(&mut bytes).is_ok() {
                f(doc, &name, &bytes);
            }
        }
    }
}

/// What `Requires` names in the corpus, and which of those namespaces the tables know. The rule
/// is that a choice is taken when every namespace it requires is a row of the tables; this lists
/// each namespace with its number of choices, so that one Word understands and the tables lack
/// shows. A reader that counts every Office or Open XML namespace as understood is the comparison.
#[test]
#[ignore = "reads the corpus named by STETDOC_CORPUS_LIST or STETDOC_CORPUS_DIR"]
fn requires_in_the_corpus() {
    let Some(docs) = corpus_docs() else {
        return;
    };
    // The reading `wordml` first carried: any Office or Open XML vocabulary is understood.
    let by_uri_prefix = |uri: &str| {
        uri.starts_with("http://schemas.microsoft.com/office/")
            || uri.starts_with("http://schemas.openxmlformats.org/")
            || uri.starts_with("http://purl.oclc.org/ooxml/")
            || uri.starts_with("urn:schemas-microsoft-com:")
    };
    // uri -> (choices, choices in documents with Word's fingerprint, documents, tables know it, Office or Open XML)
    let mut uris: BTreeMap<String, (usize, usize, BTreeSet<PathBuf>, bool, bool)> = BTreeMap::new();
    let (mut choices, mut agree, mut differ, mut empty, mut unbound) = (0usize, 0, 0, 0, 0);
    let mut documents: BTreeSet<PathBuf> = BTreeSet::new();
    for_each_word_part(&docs, |doc, _, bytes| {
        // A part with no `Requires` in it has no choice: not worth parsing.
        if !bytes.windows(8).any(|w| w == b"Requires") {
            return;
        }
        let Some(tree) = SimTree::parse(bytes) else {
            return;
        };
        for id in tree.elements() {
            if tree.qname(id) != (MC_NS.to_string(), "Choice".to_string()) {
                continue;
            }
            choices += 1;
            documents.insert(doc.path.clone());
            let attrs = tree.tree.attributes(id);
            let requires = attrs
                .iter()
                .find(|a| a.local == "Requires" && a.namespace.is_empty())
                .map(|a| a.value.to_string())
                .unwrap_or_default();
            let (mut tables_all, mut prefix_all, mut named) = (true, true, false);
            for prefix in requires.split_whitespace() {
                named = true;
                let uri = tree.tree.resolve_prefix(id, prefix);
                let known = uri
                    .as_deref()
                    .is_some_and(|u| tables::ns_by_uri(u).is_some());
                let by_prefix = uri.as_deref().is_some_and(by_uri_prefix);
                tables_all &= known;
                prefix_all &= by_prefix;
                unbound += usize::from(uri.is_none());
                let key = uri.unwrap_or_else(|| format!("(prefix {prefix} is not declared)"));
                let e = uris
                    .entry(key)
                    .or_insert((0, 0, BTreeSet::new(), known, by_prefix));
                e.0 += 1;
                e.1 += usize::from(doc.word_fp);
                e.2.insert(doc.path.clone());
            }
            empty += usize::from(!named);
            if (named && tables_all) == (named && prefix_all) {
                agree += 1;
            } else {
                differ += 1;
            }
        }
    });
    println!(
        "choices {choices} in {} documents; the two readings agree on {agree} and differ on {differ}; no prefix named in {empty}; undeclared prefixes {unbound}",
        documents.len()
    );
    let mut rows: Vec<_> = uris.iter().collect();
    rows.sort_by_key(|(u, (n, ..))| (std::cmp::Reverse(*n), (*u).clone()));
    for (uri, (n, fp, docs, known, by_prefix)) in rows {
        println!(
            "{n:>7} x ({fp:>6} with Word's fingerprint, {:>5} documents)  tables {:<3}  office-or-openxml {:<3}  {uri}",
            docs.len(),
            if *known { "yes" } else { "NO" },
            if *by_prefix { "yes" } else { "NO" }
        );
    }
}
