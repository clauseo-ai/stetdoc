//! The laws of `document` on the corpus: V1, V2 and V3 of `docs/design/check.md` 9.
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR` (the set `STETDOC_CORPUS_SET`,
//! `curated` by default). A test skips when the variable is unset, as the other corpus tests do:
//! `cargo xtask check` always sets it. One pass opens each document, checks it twice and compares
//! the check with `wordml`'s reading of the same text, and the laws read what that pass found. A
//! document that the manifest says must open and does not is a failure, not a skip.

use super::census::Census;
use super::document::{SchemaPart, listing};
use super::{Key, document, scan_part};
use crate::api::check::FindingCode;
use crate::api::limits::Limits;
use crate::state::{Dep, PartsView, State};
use crate::wordml::{AnomalyKind, BlockKind};
use crate::xml::{NodeId, SpanTree};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::OnceLock;
use stetdoc_testkit::corpus::{Corpus, Entry, named_set_kind};

/// What one document taught the pass.
struct Row {
    id: String,
    /// Whether the manifest says the document opens: the population of the census of the design.
    expected_to_open: bool,
    word_saved: bool,
    by_code: Vec<(FindingCode, usize)>,
    /// V1 failures: a report that is not the same twice, a part that changed.
    v1: Vec<String>,
    /// V3 failures: a difference between the two readers that has no reason.
    v3: Vec<String>,
    /// The differences of V3 that have a reason, by reason.
    explained: BTreeMap<&'static str, usize>,
}

impl Row {
    /// A row with nothing found yet.
    fn empty(entry: &Entry) -> Row {
        Row {
            id: entry.id.clone(),
            expected_to_open: entry.expect.is_ok(),
            word_saved: saved_by_word(entry),
            by_code: Vec::new(),
            v1: Vec::new(),
            v3: Vec::new(),
            explained: BTreeMap::new(),
        }
    }
}

/// The size in bytes up to which a document is checked twice.
const REPEAT_UP_TO: u64 = 400_000;

/// True if the manifest says Microsoft Word saved the file.
fn saved_by_word(entry: &Entry) -> bool {
    entry.producer == "Microsoft Word" && entry.word_fp == "y"
}

/// The rows of the corpus, or `None` when no corpus is named. The documents are read by as many
/// threads as the machine has, each with states of its own, and the rows are put back in the order
/// of the index.
fn rows() -> Option<&'static [Row]> {
    static ROWS: OnceLock<Option<Vec<Row>>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let corpus = Corpus::from_env()?.unwrap_or_else(|e| panic!("the corpus: {e}"));
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
        let entries = &corpus.entries;
        let mut done: Vec<(usize, Result<Row, String>)> = std::thread::scope(|scope| {
            let workers: Vec<_> = (0..threads)
                .map(|first| {
                    scope.spawn(move || {
                        entries
                            .iter()
                            .enumerate()
                            .skip(first)
                            .step_by(threads)
                            .filter_map(|(at, entry)| open_and_read(entry).map(|r| (at, r)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            workers
                .into_iter()
                .flat_map(|w| {
                    w.join()
                        .unwrap_or_else(|_| panic!("a thread of the pass failed"))
                })
                .collect()
        });
        done.sort_by_key(|(at, _)| *at);
        let mut rows = Vec::new();
        for (_, outcome) in done {
            rows.push(outcome.unwrap_or_else(|why| panic!("{why}")));
        }
        Some(rows)
    })
    .as_deref()
}

/// Opens a document and reads it. A document that does not open gives nothing, unless the manifest
/// says it must, which is an error; a check that panics is a row with that failure, because V1 is
/// that it never does.
fn open_and_read(entry: &Entry) -> Option<Result<Row, String>> {
    let must_open = entry.expect.is_ok() && entry.tier != "hostile";
    match State::open(entry.bytes(), &Limits::default()) {
        Ok(state) => Some(Ok(
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read(entry, &state))) {
                Ok(row) => row,
                Err(_) => Row {
                    v1: vec!["the check panicked".to_owned()],
                    ..Row::empty(entry)
                },
            },
        )),
        Err(why) if must_open => Some(Err(format!("{} must open and does not: {why}", entry.id))),
        Err(_) => None,
    }
}

/// Checks a document twice, and compares the check with `wordml`'s reading of it.
fn read(entry: &Entry, state: &State) -> Row {
    let view = state.parts();
    let mut row = Row::empty(entry);
    let parts = listing(&view).map(|l| l.parts).unwrap_or_default();
    let before: Vec<_> = parts
        .iter()
        .map(|p| view.generation(Dep::Part(p.id)))
        .collect();
    let first = document(&view);
    // the second check is of the documents that are not large: debug builds are slow, and the
    // report does not depend on size
    if entry.size <= REPEAT_UP_TO && first != document(&view) {
        row.v1.push("two checks of one document differ".into());
    }
    for (p, generation) in parts.iter().zip(&before) {
        if view.generation(Dep::Part(p.id)) != *generation {
            row.v1.push(format!("{} changed generation", p.name));
        }
        if view.tree(p.id).is_ok_and(SpanTree::is_dirty) {
            row.v1.push(format!("{} became dirty", p.name));
        }
    }
    agreement(&view, &parts, &mut row);
    row.by_code = first.by_code;
    row
}

/// Whether a node is in a text box, which `wordml` does not enter.
fn in_text_box(tree: &SpanTree, node: NodeId) -> bool {
    let mut at = tree.node_ref(node).parent();
    while let Some(n) = at {
        if n.local_name() == "txbxContent" {
            return true;
        }
        at = n.parent();
    }
    false
}

/// The paragraph that holds the node, if any.
fn paragraph_of(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut at = tree.node_ref(node).parent();
    while let Some(n) = at {
        if n.local_name() == "p" {
            return Some(n.id());
        }
        at = n.parent();
    }
    None
}

/// Whether an element with this local name is above the node, in its paragraph or around it.
fn above_has(tree: &SpanTree, node: NodeId, local: &str) -> bool {
    let mut at = tree.node_ref(node).parent();
    while let Some(n) = at {
        if n.local_name() == local {
            return true;
        }
        at = n.parent();
    }
    false
}

/// Whether the text of the node is in a `w:moveFrom` and in no `w:del`. Word writes `w:t` there
/// (RA001-Tracked-Revisions-02 is a Word file with it), and `check` reports a `w:delText` there as
/// outside a deletion, where `wordml` reads a move source as deleted for what a reader sees.
fn only_in_move_from(tree: &SpanTree, node: NodeId) -> bool {
    above_has(tree, node, "moveFrom") && !above_has(tree, node, "del")
}

/// Whether the paragraph that holds the text is inside a `w:del`. The schema has a `w:del` hold
/// runs and not paragraphs, but a file of `stemma` has one around a whole paragraph, and `wordml`
/// reads the `w:t` in it as text in a deletion. W1 is "within one paragraph", so `check` does not.
fn in_wrapped_paragraph(tree: &SpanTree, node: NodeId) -> bool {
    paragraph_of(tree, node).is_some_and(|p| above_has(tree, p, "del"))
}

/// Whether any bookmark element with this id is in a text box.
fn marker_in_text_box(tree: &SpanTree, id: u32) -> bool {
    let mut stack = vec![tree.root().id()];
    while let Some(n) = stack.pop() {
        let node = tree.node_ref(n);
        if matches!(node.local_name(), "bookmarkStart" | "bookmarkEnd")
            && node
                .attr(node.namespace(), "id")
                .is_some_and(|v| v.trim().parse::<u32>().ok() == Some(id))
            && in_text_box(tree, n)
        {
            return true;
        }
        stack.extend(node.child_elements().map(|c| c.id()));
    }
    false
}

/// The codes of the rules that `wordml`'s text map also reports.
const TEXT_RULES: [FindingCode; 3] = [
    FindingCode::TextInDeletion,
    FindingCode::DeltextOutsideDeletion,
    FindingCode::EdgeSpaceWithoutPreserve,
];

/// V3: the two readers of the text of a story agree. Every anomaly of the text map that `check`
/// has a rule for is a finding, and every finding that is not an anomaly has a reason.
fn agreement(view: &PartsView<'_>, parts: &[SchemaPart], row: &mut Row) {
    let Ok(specs) = view.story_specs() else {
        return;
    };
    for spec in &specs.0 {
        let Some(part) = parts.iter().find(|p| *p.name == *spec.part) else {
            continue;
        };
        let (Ok(tree), Ok(story)) = (view.tree(part.id), view.story(part.id)) else {
            continue;
        };
        let findings = scan_part(tree);
        let found: Vec<(NodeId, FindingCode, &str)> = findings
            .iter()
            .filter_map(|f| match (f.node, &f.key) {
                (Some(node), Key::Rule { code, subject }) if TEXT_RULES.contains(code) => {
                    Some((node, *code, subject.as_str()))
                }
                _ => None,
            })
            .collect();
        let known: HashSet<(NodeId, FindingCode)> =
            found.iter().map(|(n, c, _)| (*n, *c)).collect();
        let mut matched: HashSet<(NodeId, FindingCode)> = HashSet::new();
        // the anomalies of the text map of every paragraph the story lists
        let reached: HashSet<NodeId> = story
            .blocks()
            .iter()
            .filter(|b| b.kind == BlockKind::Paragraph)
            .map(|b| b.node)
            .collect();
        let anomalies: Vec<_> = (0..story.blocks().len())
            .filter_map(|i| story.paragraph(tree, i))
            .flat_map(|p| p.map().anomalies().to_vec())
            .collect();
        for a in &anomalies {
            let code = match a.kind {
                AnomalyKind::TextInDeletion => FindingCode::TextInDeletion,
                AnomalyKind::DelTextOutsideDeletion => FindingCode::DeltextOutsideDeletion,
                AnomalyKind::EdgeSpaceWithoutPreserve => FindingCode::EdgeSpaceWithoutPreserve,
                _ => continue,
            };
            // wordml puts an edge-space anomaly on the text node, and the others on the element
            let at = match tree.node_ref(a.node) {
                n if n.is_element() => a.node,
                n => n.parent().map_or(a.node, |p| p.id()),
            };
            if known.contains(&(at, code)) {
                matched.insert((at, code));
            } else if code == FindingCode::TextInDeletion && in_wrapped_paragraph(tree, at) {
                *row.explained
                    .entry(
                        "wordml reads text in a paragraph that a w:del wraps as text in a deletion",
                    )
                    .or_default() += 1;
            } else {
                row.v3.push(format!(
                    "{}: wordml reports {:?} at node {:?} and check does not",
                    part.name, a.kind, a.node
                ));
            }
        }
        for (node, code, subject) in &found {
            if matched.contains(&(*node, *code)) {
                continue;
            }
            let instruction = matches!(*subject, "instrText" | "delInstrText");
            let unreached = paragraph_of(tree, *node).is_some_and(|p| !reached.contains(&p));
            let reason = if in_text_box(tree, *node) {
                "a text box, which wordml does not enter"
            } else if unreached {
                "a paragraph that wordml does not list, in an element that is not where the schema puts it"
            } else if instruction {
                "instruction text, which wordml does not test"
            } else if *code == FindingCode::DeltextOutsideDeletion && only_in_move_from(tree, *node)
            {
                "wordml reads a w:delText in a w:moveFrom as inside a deletion"
            } else {
                row.v3.push(format!(
                    "{}: check reports {code:?} ({subject}) at node {node:?} and wordml does not",
                    part.name
                ));
                continue;
            };
            *row.explained.entry(reason).or_default() += 1;
        }
        // the bookmarks that do not pair, by id
        let mine: BTreeSet<u32> = Census::of(tree).unpaired_bookmarks().into_iter().collect();
        let theirs: BTreeSet<u32> = story
            .anchors(tree)
            .bookmarks
            .iter()
            .filter(|b| b.start.is_none() || b.end.is_none())
            .filter_map(|b| b.id.and_then(|id| u32::try_from(id).ok()))
            .collect();
        for id in mine.symmetric_difference(&theirs) {
            if marker_in_text_box(tree, *id) {
                *row.explained
                    .entry("a bookmark in a text box, which wordml does not enter")
                    .or_default() += 1;
            } else {
                row.v3.push(format!(
                    "{}: bookmark {id} is unpaired for {} only",
                    part.name,
                    if mine.contains(id) { "check" } else { "wordml" }
                ));
            }
        }
    }
}

macro_rules! rows_or_skip {
    () => {
        match rows() {
            Some(rows) => rows,
            None => {
                eprintln!("STETDOC_CORPUS_DIR is not set: the corpus is not read");
                return;
            }
        }
    };
}

#[test]
fn v1_a_check_never_panics_never_changes_a_byte_and_gives_the_same_report_twice() {
    let rows = rows_or_skip!();
    assert!(rows.len() > 200, "{} documents opened", rows.len());
    let problems: Vec<String> = rows
        .iter()
        .flat_map(|r| r.v1.iter().map(|p| format!("{}: {p}", r.id)))
        .collect();
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn v3_the_two_readers_of_the_text_agree_and_each_difference_has_a_reason() {
    let rows = rows_or_skip!();
    let problems: Vec<String> = rows
        .iter()
        .flat_map(|r| r.v3.iter().map(|p| format!("{}: {p}", r.id)))
        .collect();
    let mut explained: BTreeMap<&str, usize> = BTreeMap::new();
    for r in rows {
        for (reason, n) in &r.explained {
            *explained.entry(reason).or_default() += n;
        }
    }
    eprintln!("differences with a reason: {explained:?}");
    assert!(
        problems.is_empty(),
        "{} differences:\n{problems:#?}",
        problems.len()
    );
}

/// What the checks say, one line per document and code: the id, the wire name, the count.
fn ledger_of(rows: &[Row]) -> String {
    let mut out = String::new();
    for row in rows {
        for (code, n) in &row.by_code {
            out.push_str(&format!("{}\t{}\t{n}\n", row.id, code.as_str()));
        }
    }
    out
}

/// The ledger of the curated set: the findings of `document` on each document that has any. A
/// finding that appears, disappears or changes its count fails the test, and the ledger is changed
/// by hand, after the finding has been read and judged. No tool rewrites it in `cargo xtask check`.
const LEDGER: &str = include_str!("expected_findings.tsv");

#[test]
fn v2_the_findings_on_the_curated_set_are_the_ledger() {
    let rows = rows_or_skip!();
    if named_set_kind().is_some_and(|s| s != "curated") {
        eprintln!("the ledger is the curated set's: another set is named");
        return;
    }
    let actual = ledger_of(rows);
    let (want, got): (BTreeSet<&str>, BTreeSet<&str>) =
        (LEDGER.lines().collect(), actual.lines().collect());
    if want == got {
        return;
    }
    let join = |lines: Vec<&&str>| {
        lines
            .iter()
            .map(|l| l.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    };
    panic!(
        "the findings are not the ledger (crates/stetdoc-core/src/check/expected_findings.tsv).\nin the ledger and not found:\n{}\nfound and not in the ledger:\n{}",
        join(want.difference(&got).collect()),
        join(got.difference(&want).collect()),
    );
}

/// (Word-saved, all): how many documents have a finding, for each rule of the design.
type Figure = (usize, usize);

/// The figures of `docs/design/check.md` 3.2, for the full set: W1, W2, W3, W5, W7, W8, W9 and W10.
/// They are what `check` finds, which reads the parts the main part names and the elements Word
/// reads: the design lists the four documents where that differs from reading every part by name.
const FULL: [(&str, Figure); 8] = [
    ("W1", (1, 4)),
    ("W2", (9, 80)),
    ("W3", (2, 24)),
    ("W5", (0, 0)),
    ("W7", (8, 35)),
    ("W8", (0, 1)),
    ("W9", (6, 13)),
    ("W10", (15, 679)),
];

/// The documents of the full set that open, and the ones among them Word saved.
const FULL_DOCUMENTS: Figure = (4121, 48595);

fn codes_of(rule: &str) -> &'static [FindingCode] {
    use FindingCode as C;
    match rule {
        "W1" => &[C::TextInDeletion, C::DeltextOutsideDeletion],
        "W2" => &[C::EdgeSpaceWithoutPreserve],
        "W3" => &[C::RevisionIdDuplicate, C::BookmarkIdDuplicate],
        "W5" => &[C::RevisionNestedInItself],
        "W7" => &[
            C::BookmarkUnpaired,
            C::MoveRangeUnpaired,
            C::MoveNameUnpaired,
        ],
        "W8" => &[C::FieldSplitByDeletion],
        "W9" => &[
            C::CellWithoutFinalParagraph,
            C::TableWithoutRow,
            C::RowWithoutCell,
            C::ContainerWithoutBlock,
        ],
        _ => &[C::EmptyRevision, C::EmptyProperties],
    }
}

#[test]
fn the_census_of_the_design_holds_on_the_full_set() {
    let rows = rows_or_skip!();
    if named_set_kind().is_none_or(|s| s != "full") {
        eprintln!("the census figures are the full set's: STETDOC_CORPUS_SET=full is not set");
        return;
    }
    // the population of the design: the documents the manifest says open, which leaves out the
    // hostile ones that happen to open under the default limits
    let rows: Vec<&Row> = rows.iter().filter(|r| r.expected_to_open).collect();
    let mut wrong = Vec::new();
    for (rule, want) in FULL {
        let codes = codes_of(rule);
        let has = |r: &&&Row| r.by_code.iter().any(|(c, _)| codes.contains(c));
        let got = (
            rows.iter().filter(|r| r.word_saved).filter(has).count(),
            rows.iter().filter(has).count(),
        );
        if got != want {
            wrong.push(format!(
                "{rule}: found {got:?} documents, the design says {want:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let words = rows.iter().filter(|r| r.word_saved).count();
    assert_eq!((words, rows.len()), FULL_DOCUMENTS);
}
