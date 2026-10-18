//! The differential test against OpenXmlPowerTools' `RevisionProcessor` (design `revisions.md`, 8.2 b). The
//! `RP` documents of its test files come with the output it made for them, `-Accepted` and `-Rejected`. For
//! each document and each side, the text of every story after `accept_all` or `reject_all` is compared with
//! the text of the baseline, paragraph by paragraph, paragraphs with no text left out (a deleted row or a
//! cell that is left empty is a paragraph one of the two keeps). The baselines come from that code, not from
//! Word, so they find disagreements and do not prove correctness: every document where the two differ is in
//! [`DIVERGENCES`] with the row of design 6.4 that says why, and a document that differs without a row, or a
//! row whose document no longer differs, fails the test. With `STETDOC_CORPUS_DIR` unset the test skips.

use super::index::ChangeIndex;
use super::laws_util::runs_in;
use super::resolve::resolve;
use super::types::{Action, Selector, Step};
use crate::api::limits::Limits;
use crate::state::State;
use crate::wordml::Mode;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use stetdoc_testkit::corpus::Corpus;
use stetdoc_testkit::expect::Expect;

/// The documents where stetdoc's `accept_all` or `reject_all` reads differently from the baseline of
/// OpenXmlPowerTools: the document, the side, the row of design 6.4 that explains it, and what the difference is.
const DIVERGENCES: &[(&str, &str, u8, &str)] = &[
    (
        "RP015-MoveFrom-MoveTo",
        "accept",
        18,
        "the paragraph that the text was moved from keeps an empty paragraph in the baseline, because its `moveFrom` mark is not resolved there",
    ),
    (
        "RP015-MoveFrom-MoveTo",
        "reject",
        18,
        "the paragraph that the text was moved to keeps an empty paragraph in the baseline, because its `moveTo` mark is not resolved there",
    ),
];

/// The parts of a state that are stories, the main part first.
fn story_parts(state: &State) -> Vec<Arc<str>> {
    let view = state.parts();
    let mut out: Vec<Arc<str>> = Vec::new();
    if let Ok(specs) = view.story_specs() {
        out.extend(specs.0.iter().map(|s| s.part.clone()));
    }
    out
}

/// The text of every story of a view, one entry for each paragraph, or for each paragraph with text.
fn reading(
    view: &crate::state::PartsView<'_>,
    parts: &[Arc<str>],
    with_empty: bool,
) -> BTreeMap<String, Vec<String>> {
    parts
        .iter()
        .map(|p| {
            let texts = runs_in(view, p, Mode::Final, true, &[])
                .into_iter()
                .filter(|t| with_empty || !t.is_empty())
                .collect();
            (p.to_string(), texts)
        })
        .collect()
}

/// Where two readings first differ, as a sentence, or `None` if they are the same.
fn first_difference(
    mine: &BTreeMap<String, Vec<String>>,
    theirs: &BTreeMap<String, Vec<String>>,
) -> Option<String> {
    for (part, a) in mine {
        let Some(b) = theirs.get(part) else {
            continue;
        };
        if a != b {
            let at = a
                .iter()
                .zip(b)
                .position(|(x, y)| x != y)
                .unwrap_or(a.len().min(b.len()));
            let show = |v: &Vec<String>| v.get(at).map(|s| s.chars().take(50).collect::<String>());
            return Some(format!(
                "{part}: {} paragraphs against {}, first difference at {at}: {:?} against {:?}",
                a.len(),
                b.len(),
                show(a),
                show(b)
            ));
        }
    }
    None
}

#[test]
fn accept_all_and_reject_all_read_as_the_baselines_of_openxmlpowertools_except_where_the_table_says()
 {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let by_id: HashMap<&str, &stetdoc_testkit::corpus::Entry> = corpus
        .entries
        .iter()
        .filter(|e| e.expect == Expect::Ok)
        .map(|e| (e.id.as_str(), e))
        .collect();
    let mut compared = 0usize;
    let mut found: Vec<(String, &'static str, &'static str, String)> = Vec::new();
    let mut ids: Vec<&&str> = by_id.keys().collect();
    ids.sort();
    for id in ids {
        let Some(stem) = id
            .strip_prefix("open-xml-powertools:TestFiles/RP/")
            .and_then(|r| r.strip_suffix(".docx"))
        else {
            continue;
        };
        if stem.ends_with("-Accepted") || stem.ends_with("-Rejected") {
            continue;
        }
        let Some(input) = by_id.get(*id) else {
            continue;
        };
        let Ok(state) = State::open(input.bytes(), &Limits::default()) else {
            continue;
        };
        for (suffix, action, side) in [
            ("Accepted", Action::Accept, "accept"),
            ("Rejected", Action::Reject, "reject"),
        ] {
            let base_id = format!("open-xml-powertools:TestFiles/RP/{stem}-{suffix}.docx");
            let Some(base) = by_id.get(base_id.as_str()) else {
                continue;
            };
            let Ok(baseline) = State::open(base.bytes(), &Limits::default()) else {
                continue;
            };
            let mut tx = state.begin();
            let step = Step {
                action,
                selector: Selector::All,
            };
            resolve(&mut tx, &mut ChangeIndex::new(), &[step])
                .unwrap_or_else(|e| panic!("{stem} {side}: {e}"));
            let parts = story_parts(&state);
            compared += 1;
            for (with_empty, level) in [(false, "text"), (true, "paragraphs")] {
                let (mine, theirs) = (
                    reading(&tx.parts(), &parts, with_empty),
                    reading(&baseline.parts(), &parts, with_empty),
                );
                if let Some(why) = first_difference(&mine, &theirs) {
                    found.push((stem.to_owned(), side, level, why));
                }
            }
        }
    }
    eprintln!(
        "OpenXmlPowerTools: {compared} comparisons, {} differ",
        found.len()
    );
    for (stem, side, level, why) in &found {
        eprintln!("DIVERGES {level} {stem} {side}: {why}");
    }
    let mut problems: Vec<String> = Vec::new();
    for (stem, side, level, why) in &found {
        let listed = DIVERGENCES
            .iter()
            .any(|(s, d, _, _)| s == stem && d == side);
        // The text of the stories is the same as the baseline's everywhere. The paragraphs with no text are
        // where the two differ, and each document that differs says why.
        if *level == "text" || !listed {
            problems.push(format!(
                "{stem} {side} differs in {level} and no row of 6.4 says why: {why}"
            ));
        }
    }
    if compared > 0 {
        for (s, d, row, _) in DIVERGENCES {
            if !found
                .iter()
                .any(|(stem, side, _, _)| stem == s && side == d)
            {
                problems.push(format!(
                    "{s} {d} is listed under row {row} and no longer differs: remove it"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
