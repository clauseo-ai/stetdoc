//! The laws of the reader over the corpus (`docs/design/comments-and-notes.md`, C1 and C2).
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR`, and a test skips when the variable is
//! unset, as the other corpus tests do: `cargo xtask check` always sets it. The expected counts come from
//! a second reader that shares no code with `threads`: `roxmltree` over the parts at their usual names.

use super::*;
use crate::api::limits::Limits;
use crate::state::State;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Read;
use stetdoc_testkit::corpus::Corpus;

/// The last two parts of a document id.
fn short(id: &str) -> &str {
    let cut = id.rmatch_indices('/').nth(1).map_or(0, |(i, _)| i + 1);
    id.get(cut..).unwrap_or(id)
}

/// Reads `name` from the package, or nothing if it has no such part.
fn part(zip: &mut zip::ZipArchive<std::io::Cursor<Vec<u8>>>, name: &str) -> Option<String> {
    let mut file = zip.by_name(name).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    Some(text)
}

fn attr<'a>(n: roxmltree::Node<'a, '_>, local: &str) -> Option<&'a str> {
    n.attributes()
        .find(|a| a.name() == local)
        .map(|a| a.value())
}

/// What a second reader makes of a document: comments, threads, resolved threads, unanchored comments.
fn independent(bytes: &[u8]) -> Option<(usize, usize, usize, usize)> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.to_vec())).ok()?;
    let comments = part(&mut zip, "word/comments.xml")?;
    let doc = roxmltree::Document::parse(&comments).ok()?;
    let mut list: Vec<(i64, Option<String>)> = Vec::new();
    for c in doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("comment"))
    {
        let Some(id) = attr(c, "id").and_then(|v| v.parse::<i64>().ok()) else {
            continue;
        };
        let last = c
            .descendants()
            .rfind(|n| n.has_tag_name("p"))
            .and_then(|p| attr(p, "paraId"))
            .filter(|v| v.len() == 8)
            .map(str::to_owned);
        list.push((id, last));
    }
    let mut rows: HashMap<String, (Option<String>, bool)> = HashMap::new();
    if let Some(text) = part(&mut zip, "word/commentsExtended.xml") {
        let ex = roxmltree::Document::parse(&text).ok()?;
        for r in ex.descendants().filter(|n| n.has_tag_name("commentEx")) {
            if let Some(p) = attr(r, "paraId") {
                rows.entry(p.to_owned()).or_insert((
                    attr(r, "paraIdParent").map(str::to_owned),
                    matches!(attr(r, "done"), Some("1" | "true" | "on")),
                ));
            }
        }
    }
    let paras: HashSet<&str> = list.iter().filter_map(|(_, p)| p.as_deref()).collect();
    let parent_of = |p: &Option<String>| -> Option<String> {
        let parent = rows.get(p.as_deref()?)?.0.clone()?;
        paras.contains(parent.as_str()).then_some(parent)
    };
    let roots: Vec<&(i64, Option<String>)> = list
        .iter()
        .filter(|(_, p)| parent_of(p).is_none())
        .collect();
    let resolved = roots
        .iter()
        .filter(|(_, p)| p.as_deref().and_then(|p| rows.get(p)).is_some_and(|r| r.1))
        .count();
    let mut marked: HashSet<i64> = HashSet::new();
    let names: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with("word/") && n.ends_with(".xml") && *n != "word/comments.xml")
        .map(str::to_owned)
        .collect();
    for name in names {
        let Some(text) = part(&mut zip, &name) else {
            continue;
        };
        if !text.contains("comment") {
            continue;
        }
        let Ok(d) = roxmltree::Document::parse(&text) else {
            continue;
        };
        for n in d.descendants() {
            if matches!(
                n.tag_name().name(),
                "commentRangeStart" | "commentRangeEnd" | "commentReference"
            ) && let Some(id) = attr(n, "id").and_then(|v| v.parse().ok())
            {
                marked.insert(id);
            }
        }
    }
    let unanchored = list.iter().filter(|(id, _)| !marked.contains(id)).count();
    Some((list.len(), roots.len(), resolved, unanchored))
}

/// C1 and C2: every document reads and audits, and the counts agree with the second reader.
#[test]
fn the_reader_and_the_audit_are_total_and_agree_with_a_second_reader_on_the_corpus() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let (mut documents, mut with_comments) = (0, 0);
    let mut disagreements = Vec::new();
    let mut findings: BTreeMap<String, Vec<&'static str>> = BTreeMap::new();
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let bytes = entry.bytes();
        let state = State::open(bytes.clone(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open: {e}", entry.id));
        let view = state.parts();
        let t = Threads::of(&view).unwrap_or_else(|e| panic!("{}: threads: {e}", entry.id));
        let found = audit(&view).unwrap_or_else(|e| panic!("{}: audit: {e}", entry.id));
        documents += 1;
        for f in found {
            findings.entry(entry.id.clone()).or_default().push(f.code);
        }
        let c = t.counts();
        let ours = (c.comments, c.threads, c.resolved_threads, c.unanchored);
        match independent(&bytes) {
            Some(theirs) => {
                with_comments += usize::from(theirs.0 > 0);
                if ours != theirs {
                    disagreements.push(format!(
                        "{}: threads {ours:?}, second reader {theirs:?}",
                        entry.id
                    ));
                }
            }
            None if ours.0 > 0 => disagreements.push(format!(
                "{}: {ours:?} and no comments.xml for the second reader",
                entry.id
            )),
            None => {}
        }
        // Reading is read-only: the answer is the same the second time, from the cache.
        assert!(
            std::sync::Arc::ptr_eq(&t, &Threads::of(&view).unwrap()),
            "{}",
            entry.id
        );
    }
    assert!(documents >= 200, "only {documents} documents");
    assert!(
        with_comments >= 10,
        "only {with_comments} documents with comments"
    );
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
    // C1: the documents that have findings, and which ones. Every one is a document whose comments really
    // are like that: libraries and LibreOffice write no paraId, and so on. A new finding on a document
    // that had none is a change of the rules or a bug, and this list says which.
    let got: BTreeMap<&str, BTreeSet<&str>> = findings
        .iter()
        .map(|(id, codes)| (short(id), codes.iter().copied().collect()))
        .collect();
    let want: BTreeMap<&str, BTreeSet<&str>> = EXPECTED
        .iter()
        .map(|(id, codes)| (*id, codes.iter().copied().collect()))
        .collect();
    assert_eq!(got, want);
}

/// The documents of the curated corpus that have findings (the last two parts of the id, and the codes).
const EXPECTED: [(&str, &[&str]); 8] = [
    (
        "03_libreoffice_review_comments/input.docx",
        &["para_id_missing"],
    ),
    ("08_agent_nested_revisions/input.docx", &["para_id_missing"]),
    (
        "fixtures/wordprocessingml-comprehensive.docx",
        &["para_id_missing"],
    ),
    ("structured-export/principal.docx", &["unanchored"]),
    (
        "contract-finalize/contract-redlined.docx",
        &["para_id_range"],
    ),
    ("ptab/ptab.docx", &["part_unrelated"]),
    (
        "data/rprchange_closed.docx",
        &["para_id_missing", "unanchored"],
    ),
    (
        "CommentExTest/Invalid_Word15Comments.docx",
        &["row_missing"],
    ),
];
