//! The numbering and cascade laws over the corpus (`docs/design/styles-and-numbering.md`, 7.2 and 8.3).
//!
//! 1. **Totality.** For every document that must open, the cascade builds, every story is numbered in all
//!    three modes, and `hidden`, `looks_hidden` and `effective` answer, through the sites that
//!    `PartsView::sites` builds, for every item of every paragraph of every story, and `outline_level` for
//!    every paragraph. Nothing is skipped: an accessor that fails, or a story that does not scan, is a
//!    failure that names the document.
//! 2. **Word's own results.** In a document that Word saved (the manifest says so), the cached result of a
//!    `REF ... \w` field is the paragraph's full-context number, and a table-of-contents line holds the number
//!    of its heading (and none for an unnumbered heading), as Word showed them.

use super::tests_corpus_docs::{each_document, main_of, saved_by_word};
use super::*;
use crate::api::view::Mode;
use crate::wordml::{FIELD_RESULT, ItemKind, NodeId, SpanTree, Story};
use std::collections::{BTreeMap, HashMap};
use stetdoc_testkit::corpus::Entry;

#[derive(Default)]
struct Totals {
    documents: usize,
    stories: usize,
    paragraphs: usize,
    numbered: usize,
    runs: usize,
    levels: usize,
    hidden: BTreeMap<String, usize>,
    notes: BTreeMap<String, usize>,
    failures: Vec<String>,
}

fn check_totality(entry: &Entry, state: &State, totals: &mut Totals) {
    let view = state.parts();
    let path = &entry.id;
    let mut fail = |what: &str, e: StetError| totals.failures.push(format!("{path}: {what}: {e}"));
    let cascade = match view.cascade() {
        Ok(c) => c,
        Err(e) => return fail("cascade", e),
    };
    let list = match view.story_specs() {
        Ok(l) => l,
        Err(e) => return fail("story list", e),
    };
    totals.documents += 1;
    for spec in &list.0 {
        let Some(part) = view.id(&spec.part) else {
            totals
                .failures
                .push(format!("{path}: {} has no part number", spec.part));
            continue;
        };
        let (story, tree) = match (view.story(part), view.tree(part)) {
            (Ok(s), Ok(t)) => (s, t),
            (Err(e), _) | (_, Err(e)) => {
                totals.failures.push(format!("{path}: {}: {e}", spec.part));
                continue;
            }
        };
        totals.stories += 1;
        for mode in Mode::ALL {
            match view.numbers(part, mode) {
                Ok(numbers) => {
                    for n in numbers.notes() {
                        let kind = format!("{n:?}");
                        let kind = kind.split([' ', '{']).next().unwrap_or("").to_owned();
                        *totals.notes.entry(kind).or_default() += 1;
                    }
                    if mode == Mode::Final {
                        totals.numbered += numbers.items().count();
                    }
                }
                Err(e) => {
                    totals
                        .failures
                        .push(format!("{path}: numbers of {}: {e}", spec.part));
                }
            }
            let Ok(mut sites) = view.sites(part, mode) else {
                totals
                    .failures
                    .push(format!("{path}: no sites for {}", spec.part));
                continue;
            };
            for (i, block) in story.blocks().iter().enumerate() {
                let Some(p) = story.paragraph(tree, i) else {
                    continue;
                };
                let map = p.map();
                for item in map.items() {
                    let key = RunKey {
                        item,
                        complex_script: false,
                    };
                    let site = match sites.site_for(block.node, Some(key)) {
                        Ok(site) => site,
                        Err(e) => {
                            totals.failures.push(format!("{path}: a run site: {e}"));
                            continue;
                        }
                    };
                    let hidden = cascade.hidden(&site);
                    // The tallies and the rest of the questions are for the final text, which has runs of
                    // every kind: the other modes differ only by what a tracked change replaced.
                    if mode == Mode::Final {
                        if let Some(h) = hidden {
                            *totals.hidden.entry(format!("{h:?}")).or_default() += 1;
                        }
                        totals.runs += 1;
                        if let Some(h) = cascade.looks_hidden(&site) {
                            *totals.hidden.entry(format!("{h:?}")).or_default() += 1;
                        }
                        let _ = cascade.effective(&site);
                    }
                }
                if mode != Mode::Final {
                    continue;
                }
                totals.paragraphs += 1;
                match sites.site_for(block.node, None) {
                    Ok(site) => {
                        totals.levels += usize::from(site.level.is_some());
                        let _ = (cascade.outline_level(&site), cascade.effective(&site));
                        let _ = (cascade.hidden(&site), cascade.mark_effect(&site));
                    }
                    Err(e) => totals
                        .failures
                        .push(format!("{path}: a paragraph site: {e}")),
                }
            }
        }
    }
}

#[test]
fn numbering_and_the_cascade_are_total_over_the_corpus() {
    let mut totals = Totals::default();
    let Some(opened) = each_document(|entry, state| check_totality(entry, state, &mut totals))
    else {
        return;
    };
    let report = format!(
        "opened {opened}, documents {}, stories {}, paragraphs {}, numbered {}, with a level site {}, runs {}, hidden {:?}, notes {:?}",
        totals.documents,
        totals.stories,
        totals.paragraphs,
        totals.numbered,
        totals.levels,
        totals.runs,
        totals.hidden,
        totals.notes
    );
    assert!(
        totals.failures.is_empty(),
        "{report}\n{:#?}",
        totals.failures
    );
    // Every item of every paragraph of every story is visited, so the counts are the corpus's, and the floors
    // sit just under what the curated set holds, so that a corpus that shrinks or a story that is no longer
    // visited is a failure.
    assert_eq!(opened, totals.documents, "{report}");
    assert!(totals.documents >= FLOOR_DOCUMENTS, "{report}");
    assert!(totals.stories >= FLOOR_STORIES, "{report}");
    assert!(totals.numbered >= FLOOR_NUMBERED, "{report}");
    assert!(totals.levels >= FLOOR_NUMBERED, "{report}");
    assert!(totals.runs >= FLOOR_RUNS, "{report}");
    // Hidden text and the safety signal are seen in the corpus, so a cascade that stopped finding any is a
    // regression.
    assert!(
        totals.hidden.get("Vanish").copied().unwrap_or(0) >= FLOOR_VANISH,
        "{report}"
    );
    assert!(
        totals.hidden.get("WhiteText").copied().unwrap_or(0) >= FLOOR_WHITE,
        "{report}"
    );
}

const FLOOR_DOCUMENTS: usize = 286;
const FLOOR_STORIES: usize = 810;
const FLOOR_NUMBERED: usize = 7_900;
const FLOOR_RUNS: usize = 85_900;
const FLOOR_VANISH: usize = 18;
const FLOOR_WHITE: usize = 34;

#[derive(Default)]
struct Evidence {
    documents: usize,
    ref_checked: usize,
    ref_dangling: usize,
    ref_other_switch: usize,
    toc_checked: usize,
    toc_numbered: usize,
    toc_skipped: usize,
    failures: Vec<String>,
}

/// The text a field shows, from the items of its paragraph.
fn result_text(story: &Story, tree: &SpanTree, field: u32, para: NodeId) -> Option<String> {
    let i = story.index_of(para)?;
    let p = story.paragraph(tree, i)?;
    let map = p.map();
    let text = map.text(Mode::Redline);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for it in map.items() {
        if it.field == field && it.flags & FIELD_RESULT != 0 && it.kind == ItemKind::Text {
            let from = it.at as usize;
            out.extend(chars.iter().skip(from).take(it.len as usize));
        }
    }
    Some(out)
}

fn strip(s: &str) -> &str {
    s.trim().trim_end_matches(['.', ' ', '\t'])
}

fn check_evidence(path: &str, state: &State, ev: &mut Evidence) {
    let view = state.parts();
    let main = main_of(state);
    let (Ok(story), Ok(tree), Ok(numbers)) = (
        view.story(main),
        view.tree(main),
        view.numbers(main, Mode::Final),
    ) else {
        ev.failures.push(format!("{path}: the body does not read"));
        return;
    };
    let anchors = story.anchors(tree);
    let target: HashMap<&str, NodeId> = anchors
        .bookmarks
        .iter()
        .filter_map(|b| Some((b.name.as_str(), b.start.as_ref()?.paragraph?)))
        .collect();
    let fields = story.fields(tree);
    ev.documents += 1;
    for f in fields {
        let tokens: Vec<&str> = f.instr.split_whitespace().collect();
        if &*f.keyword == "REF" {
            let Some(name) = tokens.get(1) else { continue };
            let switches: Vec<&&str> = tokens.iter().skip(2).collect();
            let has = |s: &str| switches.iter().any(|t| **t == s);
            if !(has("\\w") || has("\\n") || has("\\r")) {
                continue;
            }
            let Some(&node) = target.get(name) else {
                ev.ref_dangling += 1;
                continue;
            };
            if !has("\\w") {
                ev.ref_other_switch += 1;
                continue;
            }
            let Some(shown) = result_text(&story, tree, f.id, f.paragraph_begin) else {
                continue;
            };
            let want = numbers.full_context(node).unwrap_or("");
            ev.ref_checked += 1;
            if strip(&shown) != want {
                ev.failures.push(format!(
                    "{path}: REF {name} shows {shown:?}, full context is {want:?}"
                ));
            }
        } else if &*f.keyword == "TOC" {
            let (Some(first), Some(last)) = (
                story.index_of(f.paragraph_begin),
                f.paragraph_end.and_then(|n| story.index_of(n)),
            ) else {
                continue;
            };
            for i in first..=last {
                let Some(p) = story.paragraph(tree, i) else {
                    continue;
                };
                let Some(line_node) = story.blocks().get(i).map(|e| e.node) else {
                    continue;
                };
                let bookmark = fields.iter().find_map(|g| {
                    if g.paragraph_begin != line_node {
                        return None;
                    }
                    let t: Vec<&str> = g.instr.split_whitespace().collect();
                    match &*g.keyword {
                        "PAGEREF" => t.get(1).map(|s| s.to_string()),
                        "HYPERLINK" => t
                            .iter()
                            .position(|x| *x == "\\l")
                            .and_then(|k| t.get(k + 1))
                            .map(|s| s.trim_matches('"').to_string()),
                        _ => None,
                    }
                });
                let Some(&heading) = bookmark.as_deref().and_then(|b| target.get(b)) else {
                    ev.toc_skipped += 1;
                    continue;
                };
                let Some(hp) = story
                    .index_of(heading)
                    .and_then(|hi| story.paragraph(tree, hi))
                else {
                    continue;
                };
                let heading_text = hp.text(Mode::Final);
                let heading_text = heading_text.trim();
                let line = p.text(Mode::Final);
                let Some(pos) = line
                    .rfind(heading_text)
                    .filter(|_| !heading_text.is_empty())
                else {
                    ev.toc_skipped += 1;
                    continue;
                };
                let shown = strip(&line[..pos]).to_string();
                let want = numbers.clause_number(heading).map(strip).unwrap_or("");
                ev.toc_checked += 1;
                if !want.is_empty() {
                    ev.toc_numbered += 1;
                }
                if shown != want {
                    ev.failures.push(format!(
                        "{path}: TOC line for {heading_text:?} starts {shown:?}, the number is {want:?}"
                    ));
                }
            }
        }
    }
}

#[test]
fn word_cached_numbers_equal_ours() {
    let mut ev = Evidence::default();
    let seen = each_document(|entry, state| {
        if saved_by_word(entry) {
            check_evidence(&entry.id, state, &mut ev);
        }
    });
    if seen.is_none() {
        return;
    }
    let report = format!(
        "documents {}, REF \\w checked {}, dangling {}, \\n or \\r skipped {}, TOC lines checked {} ({} numbered), skipped {}",
        ev.documents,
        ev.ref_checked,
        ev.ref_dangling,
        ev.ref_other_switch,
        ev.toc_checked,
        ev.toc_numbered,
        ev.toc_skipped
    );
    assert!(
        ev.failures.is_empty(),
        "{report}\n{}",
        ev.failures.join("\n")
    );
    // The floors are the curated set's counts: 11 `REF \w` results (one more `\r` field is not a test of
    // full context), no bookmark that dangles, 123 TOC lines of which 93 are for numbered headings. The 10
    // lines skipped are those whose heading text is not in the line (a field result or a different
    // spelling), counted and capped so a change shows.
    assert_eq!(ev.ref_dangling, 0, "{report}");
    assert!(ev.ref_checked >= 11, "{report}");
    assert!(ev.toc_checked >= 123, "{report}");
    assert!(ev.toc_numbered >= 93, "{report}");
    assert!(ev.toc_skipped <= 10, "{report}");
    assert!(ev.documents >= 110, "{report}");
}
