//! The model and file laws over the corpus: every document that must open is built in all three modes, and
//! M1 to M7, F2 and T1 hold (design 9.2).
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR`, and a test skips when the variable is
//! unset: `cargo xtask check` always sets it. A document that the manifest says must open and does not is a
//! failure, and so is a law that fails, which names the document.

use super::tests_laws::check_document;
use crate::api::limits::Limits;
use crate::state::State;
use std::panic::{AssertUnwindSafe, catch_unwind};
use stetdoc_testkit::corpus::Corpus;

fn message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "a panic".to_string())
}

#[test]
fn the_laws_hold_on_every_document_of_the_corpus() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut failures: Vec<String> = Vec::new();
    let (mut documents, mut compared) = (0usize, 0usize);
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let state = State::open(entry.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open and does not: {e}", entry.id));
        match catch_unwind(AssertUnwindSafe(|| check_document(&state))) {
            Ok(n) => compared += n,
            Err(payload) => failures.push(format!("{}: {}", entry.id, message(&*payload))),
        }
        documents += 1;
    }
    eprintln!("the laws held on {documents} documents, {compared} paragraphs compared in all");
    assert!(
        failures.is_empty(),
        "{} of {documents} documents fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The views over every document: the estimate is the formula (P7), a page is within the budget or one
/// block (P3), and following the cursors visits the blocks of the body once and in order (P2). It also
/// counts the outline source of each document, which the constants of the outline are tuned by.
#[test]
fn the_views_hold_on_every_document_of_the_corpus() {
    use crate::api::view::{Budget, Cursor, Mode, Scope, StoryName, ViewSpec};
    use crate::projection::page::estimate;
    use crate::projection::tests_view::refs_on;
    use crate::projection::view::view;
    use std::collections::BTreeMap;
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut failures: Vec<String> = Vec::new();
    let mut sources: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_tier: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut documents = 0usize;
    for entry in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile")
    {
        let state = State::open(entry.bytes(), &Limits::default())
            .unwrap_or_else(|e| panic!("{} must open and does not: {e}", entry.id));
        let spec = |scope: Scope, budget: u32, after: Option<Cursor>| ViewSpec {
            scope,
            budget: Budget(budget),
            after,
            ..ViewSpec::default()
        };
        let mut fail = |what: String| failures.push(format!("{}: {what}", entry.id));
        // The default view.
        match view(&state, &ViewSpec::default()) {
            Ok(v) => {
                if v.tokens != estimate(&v.text) {
                    fail("P7: the tokens of the default view are not the estimate".into());
                }
                if v.tokens > 8000 && v.position.shown > 1 {
                    fail(format!("P3: the default view is {} tokens", v.tokens));
                }
            }
            Err(e) => fail(format!("the default view: {}", e.message())),
        }
        // The outline and its source.
        match view(&state, &spec(Scope::Outline { depth: None }, 0, None)) {
            Ok(v) => {
                let line = v
                    .text
                    .lines()
                    .find(|l| l.starts_with("{outline:"))
                    .unwrap_or("");
                let source = line
                    .trim_start_matches("{outline: ")
                    .split(" · ")
                    .next()
                    .unwrap_or("")
                    .to_string();
                *by_tier
                    .entry((entry.tier.to_string(), source.clone()))
                    .or_default() += 1;
                *sources.entry(source).or_default() += 1;
            }
            Err(e) => fail(format!("the outline: {}", e.message())),
        }
        // P9: the sizes a page is guessed from, made from the lengths the index read, are the sizes the scan counts,
        // in every mode and with or without the collapsed fields, for every story.
        let parts = state.parts();
        for story in parts.story_specs().map(|l| l.0.clone()).unwrap_or_default() {
            let Some(part) = parts.id(&story.part) else {
                continue;
            };
            for mode in [Mode::Final, Mode::Original, Mode::Redline] {
                for expanded in [false, true] {
                    match (
                        parts.top_sizes(part, mode, expanded),
                        parts.scan(part, mode, expanded),
                    ) {
                        (Ok(sizes), Ok(scan)) if sizes.as_slice() == scan.tops.as_slice() => {}
                        (Ok(_), Ok(_)) => fail(format!(
                            "P9: the sizes of {} in {mode:?} (expanded {expanded}) are not the scan's",
                            story.part
                        )),
                        (Err(e), _) | (_, Err(e)) => {
                            fail(format!("P9: {}: {}", story.part, e.message()));
                        }
                    }
                }
            }
        }
        // Paging through the body.
        let whole = match view(&state, &spec(Scope::Story(StoryName::Body), 0, None)) {
            Ok(v) => refs_on(&v.text),
            Err(e) => {
                fail(format!("the whole body: {}", e.message()));
                continue;
            }
        };
        let flat = |refs: Vec<String>| -> Vec<String> {
            refs.into_iter().filter(|r| !r.starts_with('t')).collect()
        };
        let want = flat(whole);
        let mut seen: Vec<String> = Vec::new();
        let mut after: Option<Cursor> = None;
        let mut ended = false;
        // P2: follow every page, to the end, and the refs of the pages are exactly the refs of the whole body.
        for page in 0..5000 {
            let v = match view(
                &state,
                &spec(Scope::Story(StoryName::Body), 1500, after.clone()),
            ) {
                Ok(v) => v,
                Err(e) => {
                    fail(format!("page {page}: {}", e.message()));
                    break;
                }
            };
            if v.tokens != estimate(&v.text) {
                fail(format!("P7: page {page}"));
            }
            if v.tokens > 1500 && v.position.shown > 1 {
                fail(format!(
                    "P3: page {page} is {} tokens for {} blocks",
                    v.tokens, v.position.shown
                ));
            }
            seen.extend(flat(refs_on(&v.text)));
            match v.next {
                Some(c) => after = Some(c),
                None => {
                    ended = true;
                    break;
                }
            }
        }
        if !ended {
            fail("P2: the pages do not end".into());
        } else if seen != want {
            fail(format!(
                "P2: the pages do not visit the blocks of the body once and in order ({} of {} refs)",
                seen.len(),
                want.len()
            ));
        }
        documents += 1;
    }
    eprintln!("outline sources over {documents} documents: {sources:?}");
    eprintln!("by tier: {by_tier:?}");
    // The constants of the outline are tuned on the LAB documents, which have bold lines and few styles:
    // every one of them must get a real outline and not a page index.
    let lab_without = by_tier
        .iter()
        .filter(|((tier, source), _)| tier == "lab" && source.starts_with("no headings"))
        .map(|(_, n)| *n)
        .sum::<usize>();
    assert_eq!(lab_without, 0, "{lab_without} LAB documents got no outline");
    assert!(
        failures.is_empty(),
        "{} documents fail:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
