//! The laws of the comment operations over the corpus (`docs/design/comments-and-notes.md`, section 10):
//! random batches of comment operations on every document, and what must hold after each.
//!
//! - C3 no new findings: the audit after a batch has nothing the audit before did not have.
//! - C4 own only: another author's `edit` and `delete` are `readonly_region`.
//! - C5 text untouched: no paragraph's text changes in any mode.
//! - C6 locality: a paragraph that is not anchored, and a part that is not a comment part, is not even dirty.
//! - C9 determinism: the same batches on the same bytes give the same bytes.
//! - C11 cleanup: after a deletion or a resolve of revisions and the cleanup, no comment that had a reference
//!   run and still has comments is left without one, and nothing new is wrong.
//! - C13 read after write: the reader shows what the operation did.
//!
//! The corpus is read through the testkit from `STETDOC_CORPUS_DIR` (the set `STETDOC_CORPUS_SET`), and the
//! test skips when it is unset. Documents over 4 MB are left to the full run.

use super::add::{add, reply};
use super::cleanup::reconcile;
use super::delete::delete;
use super::edit::edit;
use super::resolve::set_resolved;
use super::tests_support::stamp_of;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::api::view::Mode;
use crate::revisions::compiler::Compiler;
use crate::revisions::index::ChangeIndex;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::{RevStamp, Track};
use crate::revisions::types::{Action, Selector, Step};
use crate::state::{PartsView, Recorded, State};
use crate::threads::{Finding, Threads, audit};
use crate::wordml::{BlockKey, BlockKind, MARK_DELETED};
use std::collections::{BTreeSet, HashSet};
use stetdoc_testkit::corpus::Corpus;

struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        if n == 0 {
            0
        } else {
            ((self.0 >> 33) as usize) % n
        }
    }
}

#[derive(Default, Debug)]
struct Stats {
    log: Vec<String>,
    documents: usize,
    added: usize,
    replied: usize,
    resolved: usize,
    edited: usize,
    deleted: usize,
    foreign_refused: usize,
    refused: usize,
    cleaned: usize,
}

type Key = (&'static str, Option<i64>, Option<String>);

fn keys(found: &[Finding]) -> BTreeSet<Key> {
    found
        .iter()
        .map(|f| (f.code, f.comment, f.part.clone()))
        .collect()
}

/// The body paragraphs that have text and are not deleted, with their final text.
fn paragraphs(view: &PartsView<'_>) -> Vec<(BlockKey, String)> {
    let Some(main) = view
        .story_specs()
        .ok()
        .and_then(|l| l.0.first().map(|s| s.part.clone()))
    else {
        return Vec::new();
    };
    let Some(id) = view.id(&main) else {
        return Vec::new();
    };
    let (Ok(story), Ok(tree)) = (view.story(id), view.tree(id)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, e) in story.blocks().iter().enumerate() {
        if e.kind != BlockKind::Paragraph || e.flags & MARK_DELETED != 0 {
            continue;
        }
        if let Some(p) = story.paragraph(tree, i) {
            let t = p.text(Mode::Final);
            if !t.is_empty() {
                out.push((story.key(e), t));
            }
        }
    }
    out
}

fn texts(view: &PartsView<'_>, key: &BlockKey) -> Option<[String; 3]> {
    let id = view.id(key.part())?;
    let (story, tree) = (view.story(id).ok()?, view.tree(id).ok()?);
    let p = story.paragraph(tree, story.index_of(key.node())?)?;
    Some([Mode::Final, Mode::Original, Mode::Redline].map(|m| p.text(m)))
}

fn okay(e: &StetError) -> bool {
    matches!(
        e.code(),
        ErrorCode::ReadonlyRegion
            | ErrorCode::Unsupported
            | ErrorCode::AmbiguousMatch
            | ErrorCode::StaleRef
            | ErrorCode::CrossBlock
    )
}

/// One batch of comment operations on `state`. Returns the next state, or `None` if an operation was
/// refused, in which case the batch is dropped as the real one would be.
fn batch(name: &str, state: &State, rng: &mut Rng, stats: &mut Stats) -> Option<State> {
    stats.log.clear();
    let me = stamp_of("Clauseo");
    let before_findings = keys(&audit(&state.parts()).unwrap());
    let before_paragraphs = paragraphs(&state.parts());
    let mut tx = state.begin();
    let mut anchored: HashSet<BlockKey> = HashSet::new();
    let mut added_ids: Vec<i64> = Vec::new();
    {
        let mut c = Compiler::new(&mut tx, Track::Tracked(me.clone())).unwrap();
        for _ in 0..1 + rng.below(5) {
            let result = one_op(name, &mut c, &me, rng, stats, &mut anchored, &mut added_ids);
            if let Err(e) = result {
                assert!(okay(&e), "{name}: {e}");
                stats.refused += 1;
                drop(c);
                tx.discard();
                return None;
            }
        }
        c.finish().unwrap_or_else(|e| panic!("{name}: finish: {e}"));
    }
    let view = tx.parts();
    // C3
    let after = keys(&audit(&view).unwrap());
    let new: Vec<_> = after.difference(&before_findings).collect();
    assert!(new.is_empty(), "{name}: new findings {new:?}");
    // C5, on the anchored paragraphs and a sample of the others
    for (i, (key, _)) in before_paragraphs.iter().enumerate() {
        if anchored.contains(key) || i % 40 == 0 {
            let was = texts(&state.parts(), key);
            assert_eq!(
                texts(&view, key),
                was,
                "{name}: the text of a paragraph changed"
            );
        }
    }
    // C6: nothing becomes dirty that is not a comment part or an anchored paragraph. A tree that an earlier
    // batch edited stays dirty after the commit, so the law is about what this batch newly dirtied.
    let dirty = |v: &PartsView<'_>, part: &str| {
        v.id(part)
            .and_then(|id| v.tree(id).ok())
            .is_some_and(|t| t.is_dirty())
    };
    let main = view.story_specs().unwrap().0[0].part.clone();
    for spec in view.story_specs().unwrap().0.iter() {
        let comment_part = matches!(spec.kind, crate::wordml::StoryKind::Comments);
        if !comment_part
            && spec.part != main
            && dirty(&view, &spec.part)
            && !dirty(&state.parts(), &spec.part)
        {
            panic!(
                "{name}: {} became dirty and is not a comment part",
                spec.part
            );
        }
    }
    let node_dirty = |v: &PartsView<'_>, key: &BlockKey| {
        v.id(key.part())
            .and_then(|id| v.tree(id).ok())
            .is_some_and(|t| t.node_ref(key.node()).is_dirty())
    };
    for (key, _) in &before_paragraphs {
        if !anchored.contains(key) {
            assert!(
                node_dirty(&view, key) == node_dirty(&state.parts(), key),
                "{name}: a paragraph that is not anchored became dirty; ops {:?}",
                stats.log
            );
        }
    }
    let _ = added_ids;
    let (next, _) = tx
        .commit()
        .unwrap_or_else(|e| panic!("{name}: commit: {e}"));
    Some(next)
}

/// One random operation, with its read-after-write check (C13).
fn one_op(
    name: &str,
    c: &mut Compiler<'_, '_>,
    me: &RevStamp,
    rng: &mut Rng,
    stats: &mut Stats,
    anchored: &mut HashSet<BlockKey>,
    added: &mut Vec<i64>,
) -> Result<(), StetError> {
    let (list, mine, any): (Vec<(BlockKey, String)>, Vec<i64>, Vec<i64>) = {
        let view = c.tx().parts();
        let t = Threads::of(&view)?;
        (
            paragraphs(&view),
            t.comments()
                .iter()
                .filter(|c| c.author.as_deref() == Some(me.author()))
                .map(|c| c.id)
                .collect(),
            t.comments().iter().map(|c| c.id).collect(),
        )
    };
    let pick = |v: &[i64], rng: &mut Rng| v.get(rng.below(v.len())).copied();
    let which = rng.below(9);
    stats.log.push(format!("op {which}"));
    match which {
        0..=3 => {
            let Some((block, text)) = list.get(rng.below(list.len())) else {
                return Ok(());
            };
            let len = text.chars().count();
            let a = rng.below(len);
            let b = (a + 1 + rng.below(len - a)).min(len);
            let fx = add(c, me, block, a..b, "A comment.\nSecond line.", None)?;
            stats.added += 1;
            anchored.insert(block.clone());
            added.push(fx.id);
            let view = c.tx().parts();
            let t = Threads::of(&view)?;
            let got = t
                .comment(fx.id)
                .unwrap_or_else(|| panic!("{name}: the new comment is not read"));
            assert_eq!(got.author.as_deref(), Some(me.author()), "{name}");
            assert!(got.anchor.blocks.contains(block), "{name}: anchor blocks");
            let want: String = text.chars().skip(a).take(b - a).collect();
            assert_eq!(
                t.covered_text(&view, fx.id, Mode::Final, usize::MAX)?,
                Some(want),
                "{name}: covered text"
            );
        }
        4 => {
            let Some(id) = pick(&any, rng) else {
                return Ok(());
            };
            let fx = reply(c, me, id, "A reply.", None)?;
            stats.replied += 1;
            anchored.extend(fx.blocks.iter().cloned());
            let view = c.tx().parts();
            let t = Threads::of(&view)?;
            assert_eq!(
                t.thread_of(fx.id).map(|t| t.root),
                Some(fx.thread),
                "{name}: reply thread"
            );
        }
        5 => {
            let Some(id) = pick(&any, rng) else {
                return Ok(());
            };
            let on = rng.below(2) == 0;
            let fx = set_resolved(c, id, on)?;
            stats.resolved += 1;
            anchored.extend(fx.blocks.iter().cloned());
            let view = c.tx().parts();
            let t = Threads::of(&view)?;
            let thread = t
                .thread_of(id)
                .unwrap_or_else(|| panic!("{name}: thread of c{id}"));
            assert_eq!(thread.resolved, on, "{name}: resolved");
        }
        6 => {
            let Some(id) = pick(&mine, rng) else {
                return Ok(());
            };
            edit(c, me, id, "Rewritten.")?;
            stats.edited += 1;
            let view = c.tx().parts();
            let t = Threads::of(&view)?;
            assert_eq!(
                t.comment(id).map(|c| c.author.clone()),
                Some(Some(me.author().to_owned())),
                "{name}: edit"
            );
        }
        7 => {
            let Some(id) = pick(&mine, rng) else {
                return Ok(());
            };
            let view_blocks = {
                let view = c.tx().parts();
                let t = Threads::of(&view)?;
                t.comment(id)
                    .map(|c| c.anchor.blocks.clone())
                    .unwrap_or_default()
            };
            let fx = delete(c, me, id)?;
            stats.deleted += 1;
            anchored.extend(view_blocks);
            for f in &fx {
                anchored.extend(f.blocks.iter().cloned());
            }
            let view = c.tx().parts();
            let t = Threads::of(&view)?;
            assert!(
                fx.iter().all(|f| t.comment(f.id).is_none()),
                "{name}: deleted comments are gone"
            );
        }
        _ => {
            // C4: someone else cannot edit or delete a comment of this author.
            let Some(id) = pick(&mine, rng) else {
                return Ok(());
            };
            let other = stamp_of("Somebody Else");
            let e = edit(c, &other, id, "Mine now.").unwrap_err();
            assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{name}");
            let e = delete(c, &other, id).unwrap_err();
            assert_eq!(e.code(), ErrorCode::ReadonlyRegion, "{name}");
            stats.foreign_refused += 1;
        }
    }
    Ok(())
}

/// C11: text removal by other means, the cleanup, and what must hold after it.
fn removals(name: &str, state: &State, rng: &mut Rng, stats: &mut Stats) {
    let me = stamp_of("Clauseo");
    let before = keys(&audit(&state.parts()).unwrap());
    let have = Threads::of(&state.parts()).unwrap();
    if have.comments().is_empty() {
        return;
    }
    let had_reference: HashSet<i64> = have
        .comments()
        .iter()
        .filter(|c| c.anchor.reference.is_some())
        .map(|c| c.id)
        .collect();
    let list = paragraphs(&state.parts());
    let mut tx = state.begin();
    let attempt: Result<(), StetError> = (|| {
        match rng.below(3) {
            0 => {
                let mut c = Compiler::new(&mut tx, Track::Tracked(stamp_of("Counsel")))?;
                if let Some((block, text)) = list.get(rng.below(list.len())) {
                    c.replace_text(block, 0..text.chars().count(), "")?;
                }
                c.finish()?;
            }
            1 => {
                let mut c = Compiler::new(&mut tx, Track::Untracked)?;
                if let Some((block, _)) = list.get(rng.below(list.len())) {
                    c.delete_block(block)?;
                }
                c.finish()?;
            }
            _ => {
                let step = if rng.below(2) == 0 {
                    Action::Accept
                } else {
                    Action::Reject
                };
                resolve(
                    &mut tx,
                    &mut ChangeIndex::new(),
                    &[Step {
                        action: step,
                        selector: Selector::All,
                    }],
                )?;
            }
        }
        Ok(())
    })();
    if attempt.is_err() {
        tx.discard();
        return;
    }
    let mut c = Compiler::new(&mut tx, Track::Tracked(me)).unwrap();
    let done = reconcile(&mut c).unwrap_or_else(|e| panic!("{name}: reconcile: {e}"));
    stats.cleaned += done.removed.len();
    c.finish().unwrap_or_else(|e| panic!("{name}: finish: {e}"));
    let view = tx.parts();
    let now = Threads::of(&view).unwrap();
    for comment in now.comments() {
        if had_reference.contains(&comment.id) {
            assert!(
                comment.anchor.reference.is_some(),
                "{name}: c{} lost its reference and was left",
                comment.id
            );
        }
    }
    let after = keys(&audit(&view).unwrap());
    let new: Vec<_> = after.difference(&before).collect();
    assert!(
        new.is_empty(),
        "{name}: new findings after the cleanup {new:?}"
    );
    tx.commit()
        .unwrap_or_else(|e| panic!("{name}: commit: {e}"));
}

fn run(name: &str, bytes: Vec<u8>, seed: u64, stats: &mut Stats) -> Option<Vec<u8>> {
    let original = bytes.clone();
    let mut state = State::open(bytes, &Limits::default()).ok()?;
    if paragraphs(&state.parts()).is_empty() {
        return None;
    }
    let mut rng = Rng(seed);
    for _ in 0..3 {
        if let Some(next) = batch(name, &state, &mut rng, stats) {
            state = next;
        }
    }
    removals(name, &state, &mut rng, stats);
    stats.documents += 1;
    // C10: a working copy that is exported and resumed reads the same comments and has the same findings.
    let e = state
        .export()
        .unwrap_or_else(|e| panic!("{name}: export: {e}"));
    let rec = Recorded {
        base: e.base,
        head_sha256: e.head_sha256,
        refs_sha256: e.refs_sha256,
    };
    let back = State::resume(
        e.head.clone(),
        &e.refs,
        &rec,
        Some(&original),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("{name}: resume: {e}"));
    let (a, b) = (
        Threads::of(&state.parts()).unwrap(),
        Threads::of(&back.parts()).unwrap(),
    );
    assert_eq!(a.counts(), b.counts(), "{name}: resume");
    assert_eq!(
        keys(&audit(&state.parts()).unwrap()),
        keys(&audit(&back.parts()).unwrap()),
        "{name}: resume"
    );
    Some(e.head)
}

#[test]
fn the_comment_laws_hold_on_random_batches_over_the_corpus() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.unwrap_or_else(|e| panic!("the corpus: {e}"));
    let mut stats = Stats::default();
    // The curated set is run whole under a size bound that keeps the check fast. The full set, which has tens
    // of thousands of documents, is sampled: every document would take hours.
    let (bound, stride) = if corpus.entries.len() > 1000 {
        (4_000_000, corpus.entries.len() / 2000)
    } else {
        (1_000_000, 1)
    };
    for (n, entry) in corpus
        .entries
        .iter()
        .filter(|e| e.expect.is_ok() && e.tier != "hostile" && e.size < bound)
        .enumerate()
        .filter(|(n, _)| n % stride.max(1) == 0)
    {
        let seed = 0x9E37_79B9_7F4A_7C15_u64.wrapping_mul(n as u64 + 1);
        if n % 100 == 0 {
            eprintln!("{n}: {} ({} bytes)", entry.id, entry.size);
        }
        let a = run(&entry.id, entry.bytes(), seed, &mut stats);
        // C9: the same batches on the same bytes give the same bytes (on every other document, to keep the
        // check fast).
        if n % 2 == 0 {
            let b = run(&entry.id, entry.bytes(), seed, &mut Stats::default());
            assert_eq!(a, b, "{}: not deterministic", entry.id);
        }
    }
    eprintln!("{stats:?}");
    assert!(stats.documents >= 100 && stats.added >= 200, "{stats:?}");
}
