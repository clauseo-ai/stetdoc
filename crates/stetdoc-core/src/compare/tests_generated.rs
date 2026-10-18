//! Generated pairs (design `compare.md`, 9.3), over the corpus named by `STETDOC_CORPUS_DIR`; with the variable unset the tests skip and
//! pass. A same-package pair is a document and the same document after a batch of untracked edits made through the compiler (text
//! replacements, new paragraphs, deleted paragraphs, splits and merges): the edits are the truth that the redline is read against. A
//! cross pair is two different documents, which makes the import carry styles, lists, notes, pictures and bookmarks in bulk. Each
//! pair is held to the laws of `tests_laws`, with the excluded blocks left out of both sides.

use super::tests_laws::check_pair;
use crate::api::limits::Limits;
use crate::revisions::compiler::Compiler;
use crate::revisions::stamp::Track;
use crate::revisions::types::{NewParagraph, Place, PropEdit};
use crate::state::State;
use crate::wordml::{BlockKey, BlockKind, Mode, Paragraph};
use stetdoc_testkit::corpus::Corpus;

/// The documents small enough to be compared many times in a debug build.
const MAX_BYTES: usize = 120_000;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }
}

/// The document after a batch of untracked edits: about one paragraph in six is touched, by an edit chosen at random.
fn mutate(state: &State, seed: u64) -> Option<Vec<u8>> {
    let mut rng = Lcg(seed);
    let mut tx = state.begin();
    let keys: Vec<(BlockKey, usize)> = {
        let view = tx.parts();
        let main = state.parts().story_specs().ok()?.0.first()?.part.clone();
        let id = view.id(&main)?;
        let story = view.story(id).ok()?;
        let tree = view.tree(id).ok()?;
        story
            .blocks()
            .iter()
            .filter(|e| e.kind == BlockKind::Paragraph)
            .filter_map(|e| {
                let p = Paragraph::new(tree, e.node)?;
                Some((story.key(e), p.text(Mode::Final).chars().count()))
            })
            .collect()
    };
    let mut c = Compiler::new(&mut tx, Track::Untracked).ok()?;
    let words = [
        "alpha",
        "the supplier",
        "notice",
        "within thirty days",
        "indemnify",
        "Schedule 2",
        "shall not",
    ];
    for (i, (key, len)) in keys.iter().enumerate() {
        if !rng.next().is_multiple_of(6) {
            continue;
        }
        let w = words[rng.next() % words.len()];
        match rng.next() % 6 {
            0 | 1 if *len > 12 => {
                let a = rng.next() % (len - 6);
                let b = a + 1 + rng.next() % 5;
                let _ = c.replace_text(key, a..b, w);
            }
            2 => {
                let spec = NewParagraph {
                    text: format!("{w} {i} was added by the batch"),
                    props: PropEdit::default(),
                    style: None,
                };
                let _ = c.insert_block(key, Place::After, &spec);
            }
            3 if *len > 0 => {
                let _ = c.delete_block(key);
            }
            4 if *len > 40 => {
                let _ = c.split_block(key, 10 + rng.next() % (len - 20));
            }
            5 => {
                if let Some((next, _)) = keys.get(i + 1) {
                    let _ = c.merge_blocks(key, next);
                }
            }
            _ => {}
        }
    }
    c.finish().ok()?;
    let (s, _) = tx.commit().ok()?;
    Some(s.export().ok()?.head)
}

struct Totals {
    pairs: usize,
    excluded: usize,
    failed: usize,
    errors: usize,
    broken: usize,
    exclusions: usize,
}

fn run(pairs: impl Iterator<Item = (String, Vec<u8>, Vec<u8>)>, name: &str) -> Totals {
    let mut t = Totals {
        pairs: 0,
        excluded: 0,
        failed: 0,
        errors: 0,
        broken: 0,
        exclusions: 0,
    };
    for (label, a, b) in pairs {
        t.pairs += 1;
        match check_pair(a, b) {
            Err(e) => {
                t.errors += 1;
                println!("{name} {label}: error {e}");
            }
            Ok(v) => {
                if v.receipt.broken > 0 {
                    println!(
                        "{name} {label}: {:?}",
                        v.receipt
                            .exclusions
                            .iter()
                            .map(|e| e.reason.clone())
                            .collect::<Vec<_>>()
                    );
                }
                t.broken += v.receipt.broken;
                t.exclusions += v.receipt.exclusions.len();
                if !v.receipt.exclusions.is_empty() {
                    t.excluded += 1;
                }
                if !v.failures.is_empty() {
                    t.failed += 1;
                    println!("{name} {label}: {}", v.failures.join("\n"));
                }
            }
        }
    }
    println!(
        "{name}: {} pairs, {} failed, {} errors, {} broken, {} with exclusions ({} exclusions)",
        t.pairs, t.failed, t.errors, t.broken, t.excluded, t.exclusions
    );
    t
}

fn small_documents(corpus: &Corpus) -> Vec<(String, Vec<u8>)> {
    corpus
        .expected_ok()
        .map(|e| (e.id.to_string(), e.bytes()))
        .filter(|(_, b)| b.len() < MAX_BYTES && State::open(b.clone(), &Limits::default()).is_ok())
        .collect()
}

#[test]
fn a_document_and_the_same_document_after_untracked_edits_hold_the_laws() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let docs = small_documents(&corpus);
    let pairs = docs
        .iter()
        .enumerate()
        .take(60)
        .filter_map(|(i, (id, bytes))| {
            let state = State::open(bytes.clone(), &Limits::default()).ok()?;
            let edited = mutate(&state, 7 + i as u64)?;
            Some((id.clone(), bytes.clone(), edited))
        });
    let t = run(pairs, "same-package");
    assert!(t.pairs > 15, "only {} pairs", t.pairs);
    assert!(t.failed == 0 && t.errors == 0 && t.broken == 0);
    assert!(
        t.excluded * 10 <= t.pairs,
        "{} of {} pairs have exclusions",
        t.excluded,
        t.pairs
    );
}

#[test]
fn two_different_documents_hold_the_laws() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let docs = small_documents(&corpus);
    let n = docs.len();
    let pairs = (0..n.min(40)).filter_map(|i| {
        let (ida, a) = docs.get(i)?;
        let (idb, b) = docs.get((i * 7 + 3) % n)?;
        (ida != idb).then(|| (format!("{ida} / {idb}"), a.clone(), b.clone()))
    });
    let t = run(pairs, "cross");
    assert!(t.pairs > 15, "only {} pairs", t.pairs);
    assert!(t.failed == 0 && t.errors == 0 && t.broken == 0);
    // the documents of the corpus are the testkit's odd ones (charts, equations, diagrams): a third may hold something that no tracked change carries
    assert!(
        t.excluded * 5 <= t.pairs * 2,
        "{} of {} pairs have exclusions",
        t.excluded,
        t.pairs
    );
}

#[test]
fn a_document_compared_with_itself_has_no_revision_and_saves_as_it_was() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let mut checked = 0usize;
    for (id, bytes) in small_documents(&corpus).into_iter().take(80) {
        let Ok((redline, receipt)) = super::compare_states(
            bytes.clone(),
            bytes.clone(),
            &Limits::default(),
            &super::tests_laws::stamp(),
        ) else {
            panic!("{id}: does not compare with itself");
        };
        assert!(
            receipt.exclusions.is_empty() && receipt.declared.imports.is_empty(),
            "{id}"
        );
        let view = redline.parts();
        let mut revisions = 0usize;
        for spec in view.story_specs().expect("stories").0.iter() {
            let tid = view.id(&spec.part).expect("a part");
            revisions += crate::wordml::part_revisions(view.tree(tid).expect("a tree")).len();
        }
        // a document that has revisions of its own has them accepted first
        let own = {
            let s = State::open(bytes.clone(), &Limits::default()).expect("opens");
            let v = s.parts();
            let mut n = 0usize;
            for spec in v.story_specs().expect("stories").0.iter() {
                let tid = v.id(&spec.part).expect("a part");
                n += crate::wordml::part_revisions(v.tree(tid).expect("a tree")).len();
            }
            n
        };
        assert_eq!(
            revisions, 0,
            "{id}: a document compared with itself has revisions"
        );
        if own == 0 {
            assert_eq!(
                redline.export().expect("exports").head,
                bytes,
                "{id}: saves as something else"
            );
        }
        checked += 1;
    }
    assert!(checked > 30, "only {checked} documents");
}

#[test]
fn the_same_inputs_give_the_same_redline() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let docs = small_documents(&corpus);
    let mut checked = 0usize;
    for (i, (id, bytes)) in docs.iter().enumerate().take(30) {
        let Some(other) = docs.get((i * 5 + 1) % docs.len()) else {
            continue;
        };
        let go = || {
            super::compare_states(
                bytes.clone(),
                other.1.clone(),
                &Limits::default(),
                &super::tests_laws::stamp(),
            )
        };
        let (Ok((a, ra)), Ok((b, rb))) = (go(), go()) else {
            continue;
        };
        assert_eq!(
            a.export().expect("exports").head,
            b.export().expect("exports").head,
            "{id}: two runs, two redlines"
        );
        assert_eq!(ra.moves, rb.moves);
        assert_eq!(ra.warnings, rb.warnings);
        checked += 1;
    }
    assert!(checked > 10);
}

#[test]
fn comparing_the_accepted_inputs_gives_the_same_redline() {
    let Some(corpus) = Corpus::from_env() else {
        return;
    };
    let corpus = corpus.expect("the corpus set opens");
    let mut checked = 0usize;
    for (id, bytes) in small_documents(&corpus) {
        let state = State::open(bytes.clone(), &Limits::default()).expect("opens");
        let Ok(accepted) =
            super::tests_laws::resolved(&state, crate::revisions::types::Action::Accept)
        else {
            continue;
        };
        let Ok(accepted_bytes) = accepted.export().map(|e| e.head) else {
            continue;
        };
        if accepted_bytes == bytes {
            continue;
        }
        // a document with revisions: compare it with a plain one, as given and accepted first
        let other = {
            let s = State::open(bytes.clone(), &Limits::default()).expect("opens");
            mutate(&s, 99).unwrap_or_else(|| bytes.clone())
        };
        let (Ok((x, _)), Ok((y, _))) = (
            super::compare_states(
                other.clone(),
                bytes.clone(),
                &Limits::default(),
                &super::tests_laws::stamp(),
            ),
            super::compare_states(
                other,
                accepted_bytes,
                &Limits::default(),
                &super::tests_laws::stamp(),
            ),
        ) else {
            continue;
        };
        assert_eq!(
            x.parts().story_specs().map(|s| s.0.len()).ok(),
            y.parts().story_specs().map(|s| s.0.len()).ok(),
            "{id}"
        );
        let body = |s: &State| {
            let v = s.parts();
            let spec = v.story_specs().expect("stories").0[0].part.clone();
            String::from_utf8_lossy(
                &v.tree(v.id(&spec).expect("a part"))
                    .expect("a tree")
                    .serialize(),
            )
            .into_owned()
        };
        assert_eq!(
            body(&x),
            body(&y),
            "{id}: accepting first is not the same as comparing the accepted"
        );
        checked += 1;
    }
    assert!(checked > 3, "only {checked} documents with revisions");
}
