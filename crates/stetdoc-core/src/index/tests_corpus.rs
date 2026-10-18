//! The index over the corpus: its laws hold on every document, and the counts that tune the heuristics are
//! reported (design 9.2). Skipped when `STETDOC_CORPUS_DIR` is unset, as every corpus test is.

use super::terms::DefKind;
use super::{EdgeKind, Source, State as Resolution};
use crate::api::limits::Limits;
use crate::state::State;
use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use stetdoc_testkit::corpus::Corpus;

#[derive(Default, Debug)]
struct Counts {
    documents: usize,
    clauses: usize,
    computed: usize,
    typed: usize,
    composed: usize,
    defs: usize,
    parentheticals: usize,
    twice: usize,
    edges: BTreeMap<String, usize>,
    with_clauses: usize,
    with_defs: usize,
    with_prose: usize,
    clause_paragraphs: usize,
}

/// I1, I2 and I6 on one document, and the counts of the document.
fn check_document(state: &State, counts: &mut Counts) {
    let parts = state.parts();
    let index = parts.doc_index().unwrap();
    let again = parts.doc_index().unwrap();
    assert!(
        std::sync::Arc::ptr_eq(&index, &again),
        "the index is cached"
    );
    counts.documents += 1;
    counts.clauses += index.clauses().len();
    counts.with_clauses += usize::from(!index.clauses().is_empty());
    for (at, c) in index.clauses().iter().enumerate() {
        match c.source {
            Source::Computed => counts.computed += 1,
            Source::Typed => counts.typed += 1,
        }
        counts.composed += usize::from(c.composed);
        // I1: a clause is a paragraph, and the table knows it by its block.
        assert_eq!(index.position_of(&c.block), Some(at), "{}", c.full);
        assert!(!c.label.is_empty() && !c.full.is_empty(), "{c:?}");
        // I2: a clause's own full label resolves to it, when the label is of a shape a query can have.
        if super::clauses::parse_query(&c.full).is_some() {
            let hits: Vec<usize> = index
                .resolve_clause(&c.full)
                .into_iter()
                .map(|m| m.at)
                .collect();
            assert!(
                hits.contains(&at),
                "{:?} does not resolve to itself: {hits:?}",
                c.full
            );
        }
    }
    // The clauses of each story are in document order.
    let mut last: BTreeMap<String, usize> = BTreeMap::new();
    for c in index.clauses() {
        let prev = last.insert(c.block.part().to_string(), c.entry);
        assert!(prev.is_none_or(|p| p < c.entry), "clauses are in order");
    }
    for t in index.terms() {
        let d = index.definitions(t);
        counts.defs += 1;
        counts.twice += usize::from(d.len() > 1);
        counts.parentheticals += d
            .iter()
            .filter(|d| d.kind == DefKind::Parenthetical)
            .count();
    }
    counts.with_defs += usize::from(index.terms().next().is_some());
    let edges = parts.edges().unwrap();
    let mut prose = false;
    for c in index.clauses() {
        counts.clause_paragraphs += 1;
        let _ = c;
    }
    for story in parts.story_specs().unwrap().0.iter() {
        let part = parts.id(&story.part).unwrap();
        let s = parts.story(part).unwrap();
        for entry in s.blocks() {
            for e in edges.outbound(&s.key(entry)) {
                let kind = match e.kind {
                    EdgeKind::Field => "field",
                    EdgeKind::Link => "link",
                    EdgeKind::Prose => "prose",
                };
                prose |= e.kind == EdgeKind::Prose;
                *counts
                    .edges
                    .entry(format!("{kind}/{}", e.state.word()))
                    .or_default() += 1;
                // The shape of the states.
                match e.state {
                    Resolution::Resolved => assert_eq!(e.to.len(), 1, "{e:?}"),
                    // The chosen target first, then the candidates it was chosen over.
                    Resolution::Relative => assert!(!e.to.is_empty(), "{e:?}"),
                    Resolution::Ambiguous => assert!(e.to.len() > 1, "{e:?}"),
                    Resolution::Unresolved => assert!(e.to.is_empty(), "{e:?}"),
                    Resolution::Outside => {}
                }
                // Every target is a block with a ref, and the edge is in the target's inbound list, unless the
                // mention is too wide to be about one of its candidates more than another.
                // A relative edge cites only its chosen target; the alternatives after it are not inbound.
                let cited = if e.state == Resolution::Relative {
                    1
                } else {
                    e.to.len()
                };
                for (n, t) in e.to.iter().enumerate() {
                    assert!(parts.ref_of(t).is_some(), "{e:?}");
                    let listed = edges.inbound(t).iter().any(|i| std::ptr::eq(*i, e));
                    let expected = n < cited && cited <= super::edges::WIDEST_INBOUND;
                    assert_eq!(listed, expected, "{e:?}");
                }
            }
        }
    }
    counts.with_prose += usize::from(prose);
    // I5: `lookup` of a term returns every definition, and `use_count` is a brute-force count of the uses.
    let terms: Vec<String> = index.terms().take(12).map(str::to_string).collect();
    if !terms.is_empty() {
        let texts = paragraph_texts(&parts);
        for term in terms {
            let got = super::lookup(parts, "base", &term, None).unwrap();
            let Some(crate::api::lookup::Hit::Term {
                definitions,
                use_count,
                matched,
                ..
            }) = got
                .hits
                .iter()
                .find(|h| matches!(h, crate::api::lookup::Hit::Term { .. }))
            else {
                panic!("no term hit for {term:?}");
            };
            assert_eq!(matched, "exact", "{term:?}");
            assert_eq!(
                definitions.len(),
                index.definitions(&term).len(),
                "{term:?}"
            );
            let want: usize = texts.iter().map(|t| brute_uses(t, &term)).sum();
            assert_eq!(*use_count as usize, want, "{term:?}");
        }
    }
}

/// The text of every paragraph of the indexed stories in `Final`, with atoms cut out and spaces collapsed.
fn paragraph_texts(parts: &crate::state::PartsView<'_>) -> Vec<String> {
    let mut out = Vec::new();
    for spec in parts.story_specs().unwrap().0.iter() {
        if spec.kind == crate::wordml::StoryKind::Comments {
            continue;
        }
        let id = parts.id(&spec.part).unwrap();
        let story = parts.story(id).unwrap();
        let tree = parts.tree(id).unwrap();
        for (i, e) in story.blocks().iter().enumerate() {
            if e.kind != crate::wordml::BlockKind::Paragraph {
                continue;
            }
            let raw = story
                .paragraph(tree, i)
                .unwrap()
                .map()
                .text(crate::api::view::Mode::Final)
                .to_string();
            let cleaned: String = raw
                .chars()
                .filter(|c| !matches!(c, '\u{1}'..='\u{6}' | '\u{c}' | '\u{e}'))
                .collect();
            out.push(cleaned.split_whitespace().collect::<Vec<_>>().join(" "));
        }
    }
    out
}

/// The uses of a term by the definition: a whole word, case sensitive, not after an opening quote, and a
/// lowercase term needs four letters. Written with a sliding window of characters, apart from `terms::uses`.
fn brute_uses(text: &str, term: &str) -> usize {
    let t: Vec<char> = term.chars().collect();
    let c: Vec<char> = text.chars().collect();
    if t.is_empty() || (t[0].is_lowercase() && t.len() < 4) || c.len() < t.len() {
        return 0;
    }
    (0..=c.len() - t.len())
        .filter(|&i| {
            c[i..i + t.len()] == t[..]
                && (i == 0 || !c[i - 1].is_alphanumeric())
                && c.get(i + t.len()).is_none_or(|n| !n.is_alphanumeric())
                && (i == 0 || !matches!(c[i - 1], '"' | '\u{201c}'))
        })
        .count()
}

/// I6: two states opened from the same bytes give the same clauses, definitions and edges, in the same order.
fn same_index(a: &State, b: &State) {
    let (pa, pb) = (a.parts(), b.parts());
    let (ia, ib) = (pa.doc_index().unwrap(), pb.doc_index().unwrap());
    assert_eq!(format!("{:?}", ia.clauses()), format!("{:?}", ib.clauses()));
    let terms = |i: &super::Index| -> String {
        i.terms()
            .map(|t| format!("{t}={:?}", i.definitions(t)))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(terms(&ia), terms(&ib));
    let (ea, eb) = (pa.edges().unwrap(), pb.edges().unwrap());
    assert_eq!(ea.len(), eb.len());
    for spec in pa.story_specs().unwrap().0.iter() {
        let (ida, idb) = (pa.id(&spec.part).unwrap(), pb.id(&spec.part).unwrap());
        let (sa, sb) = (pa.story(ida).unwrap(), pb.story(idb).unwrap());
        for (x, y) in sa.blocks().iter().zip(sb.blocks()) {
            let (ka, kb) = (sa.key(x), sb.key(y));
            assert_eq!(
                format!("{:?}", ea.outbound(&ka)),
                format!("{:?}", eb.outbound(&kb))
            );
            assert_eq!(
                format!("{:?}", ea.inbound(&ka)),
                format!("{:?}", eb.inbound(&kb))
            );
        }
    }
}

#[test]
fn the_index_laws_hold_on_every_document_of_the_corpus() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut failures: Vec<String> = Vec::new();
    let mut counts = Counts::default();
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let state = State::open(entry.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open and does not: {e}", entry.id));
        let again = State::open(entry.bytes(), &Limits::default()).unwrap();
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            check_document(&state, &mut counts);
            same_index(&state, &again);
        }));
        if let Err(payload) = outcome {
            let text = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
                .unwrap_or_default();
            failures.push(format!("{}: {text}", entry.id));
        }
    }
    eprintln!("index over the corpus: {counts:#?}");
    assert!(
        failures.is_empty(),
        "{} documents fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
