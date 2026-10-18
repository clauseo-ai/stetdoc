//! The identity oracle: random sequences of edits against a `State`, and an independent model of
//! which element each ref names.
//!
//! The model knows only the rules of section 6 of the design: a ref follows its element, a deleted
//! element's ref is gone for good, a merge keeps the survivor's ref on the merged element and
//! retires the others, a split that keeps the original element for the second part rebinds, and a
//! new element gets the next number at commit. After every commit, for every ref ever issued,
//! `resolve` must return the block the model says it names, or `stale_ref`, and never another
//! block. That is the test of "refs can never silently point at the wrong paragraph".
//!
//! The edits go through the `xml` editor and the declarations directly, so nothing depends on
//! `revisions`. The same random edits also check cache coherence: after every operation, and after
//! every commit, every cached value equals a rebuild from scratch, with a `touched` that omits
//! every new, removed and moved block.

use super::tests::*;
use super::tests_coherence::incoherent;
use super::tests_edit::snapshot;
use super::*;
use crate::api::error::ErrorCode;
use crate::refs::Claim;
use crate::xml::NodeId;
use std::collections::BTreeMap;

/// A small deterministic generator, so a failing seed can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

#[derive(Clone, Debug)]
struct Entry {
    node: NodeId,
    /// The ref the model says the element has, or `None` for an element that is new in this batch.
    rf: Option<String>,
}

#[derive(Clone, Debug)]
enum Fate {
    Live,
    Deleted,
    MergedInto(String),
}

#[derive(Clone)]
struct Model {
    doc: Vec<Entry>,
    issued: BTreeMap<String, Fate>,
    next: u32,
    /// The refs that merges of the batch in progress retired.
    fresh_merges: Vec<String>,
}

impl Model {
    fn of(s: &State) -> Model {
        let doc: Vec<Entry> = body_keys(s)
            .iter()
            .map(|k| Entry {
                node: k.node(),
                rf: s.refs.ref_of(k).map(|r| r.to_string()),
            })
            .collect();
        let issued = doc
            .iter()
            .filter_map(|e| e.rf.clone())
            .map(|r| (r, Fate::Live))
            .collect();
        Model {
            next: doc.len() as u32 + 1,
            doc,
            issued,
            fresh_merges: Vec::new(),
        }
    }

    /// `absorbed` are merged into `survivor`. A ref that a merge of this batch retired into one of
    /// them is merged into the survivor instead, so no chain forms inside one version.
    fn absorb(&mut self, survivor: &str, absorbed: &[String]) {
        for x in absorbed {
            for r in self.fresh_merges.clone() {
                if matches!(self.issued.get(&r), Some(Fate::MergedInto(t)) if t == x) {
                    self.issued.insert(r, Fate::MergedInto(survivor.to_owned()));
                }
            }
        }
        for x in absorbed {
            self.issued
                .insert(x.clone(), Fate::MergedInto(survivor.to_owned()));
            self.fresh_merges.push(x.clone());
        }
    }

    /// The indexes of `n` adjacent entries that all have refs, from a random start.
    fn run_of(&self, rng: &mut Rng, n: usize) -> Option<usize> {
        let starts: Vec<usize> = (0..self.doc.len().saturating_sub(n - 1))
            .filter(|&i| self.doc[i..i + n].iter().all(|e| e.rf.is_some()))
            .collect();
        (!starts.is_empty()).then(|| starts[rng.below(starts.len())])
    }
}

fn key(tx: &Tx<'_>, node: NodeId) -> BlockKey {
    key_in(tx, node)
}

/// The node of a new paragraph, put before the element `at`.
fn fresh_before(tx: &mut Tx<'_>, at: NodeId) -> NodeId {
    let n = new_paragraph(tx, "n");
    put_before(tx, n, &key(tx, at));
    n
}

/// Applies one random operation to the transaction and the model. Returns false if none applied.
fn step(rng: &mut Rng, tx: &mut Tx<'_>, m: &mut Model) -> Option<String> {
    match rng.below(8) {
        // Insert a new paragraph somewhere.
        0 => {
            let i = rng.below(m.doc.len() + 1);
            let n = new_paragraph(tx, "ins");
            if i == m.doc.len() {
                let last = key(tx, m.doc[i - 1].node);
                put_after(tx, n, &last);
            } else {
                let at = key(tx, m.doc[i].node);
                put_before(tx, n, &at);
            }
            m.doc.insert(i, Entry { node: n, rf: None });
            Some(format!("insert at {i}"))
        }
        // Delete a paragraph, new or not.
        1 if m.doc.len() > 3 => {
            let i = rng.below(m.doc.len());
            let e = m.doc.remove(i);
            detach(tx, &key(tx, e.node));
            let said = format!("delete {i} ({:?})", e.rf);
            if let Some(r) = e.rf {
                // What this batch merged into the block goes with it.
                for x in m.fresh_merges.clone() {
                    if matches!(m.issued.get(&x), Some(Fate::MergedInto(t)) if *t == r) {
                        m.issued.insert(x, Fate::Deleted);
                    }
                }
                m.issued.insert(r, Fate::Deleted);
            }
            Some(said)
        }
        // A split that keeps the original element for the first part.
        2 => {
            let i = rng.below(m.doc.len());
            let n = new_paragraph(tx, "tail");
            put_after(tx, n, &key(tx, m.doc[i].node));
            if let Some(r) = &m.doc[i].rf {
                let k = key(tx, m.doc[i].node);
                let _ = r;
                tx.touch(&k);
            }
            m.doc.insert(i + 1, Entry { node: n, rf: None });
            Some(format!("split first {i}"))
        }
        // A split that keeps the original element for the second part: rebind.
        3 => {
            let live: Vec<usize> = (0..m.doc.len())
                .filter(|&i| m.doc[i].rf.is_some())
                .collect();
            if live.is_empty() {
                return None;
            }
            let i = live[rng.below(live.len())];
            let orig = m.doc[i].node;
            let n = new_paragraph(tx, "head");
            let new_key = put_before(tx, n, &key(tx, orig));
            tx.rebind(&key(tx, orig), new_key).unwrap();
            let rf = m.doc[i].rf.take();
            m.doc[i] = Entry {
                node: orig,
                rf: None,
            };
            m.doc.insert(i, Entry { node: n, rf });
            Some(format!("split second {i}"))
        }
        // A merge of two adjacent blocks, keeping the first, the second or a new element.
        4 => {
            let i = m.run_of(rng, 2)?;
            let (a, b) = (m.doc[i].clone(), m.doc[i + 1].clone());
            let into = match rng.below(3) {
                0 => a.node,
                1 => b.node,
                _ => fresh_before(tx, a.node),
            };
            tx.merge(&key(tx, a.node), &key(tx, b.node), key(tx, into))
                .unwrap();
            for node in [a.node, b.node] {
                if node != into {
                    detach(tx, &key(tx, node));
                }
            }
            let (ra, rb) = (a.rf.unwrap(), b.rf.unwrap());
            let said = format!("merge2 at {i}: {ra} absorbs {rb}");
            m.absorb(&ra, &[rb]);
            m.doc.splice(
                i..i + 2,
                [Entry {
                    node: into,
                    rf: Some(ra),
                }],
            );
            Some(said)
        }
        // A merge of three adjacent blocks, in either order, with any choice of element.
        5 => {
            let i = m.run_of(rng, 3)?;
            let (a, b, c) = (m.doc[i].clone(), m.doc[i + 1].clone(), m.doc[i + 2].clone());
            let mut created = Vec::new();
            let mut pick =
                |tx: &mut Tx<'_>, rng: &mut Rng, survivor: NodeId, absorbed: NodeId| match rng
                    .below(3)
                {
                    0 => survivor,
                    1 => absorbed,
                    _ => {
                        let n = fresh_before(tx, a.node);
                        created.push(n);
                        n
                    }
                };
            let into2 = if rng.below(2) == 0 {
                let into1 = pick(tx, rng, a.node, b.node);
                tx.merge(&key(tx, a.node), &key(tx, b.node), key(tx, into1))
                    .unwrap();
                let into2 = pick(tx, rng, into1, c.node);
                tx.merge(&key(tx, into1), &key(tx, c.node), key(tx, into2))
                    .unwrap();
                into2
            } else {
                let into1 = pick(tx, rng, b.node, c.node);
                tx.merge(&key(tx, b.node), &key(tx, c.node), key(tx, into1))
                    .unwrap();
                let into2 = pick(tx, rng, a.node, into1);
                tx.merge(&key(tx, a.node), &key(tx, into1), key(tx, into2))
                    .unwrap();
                into2
            };
            for node in [a.node, b.node, c.node].into_iter().chain(created) {
                if node != into2 {
                    detach(tx, &key(tx, node));
                }
            }
            let (ra, rb, rc) = (a.rf.unwrap(), b.rf.unwrap(), c.rf.unwrap());
            m.absorb(&ra, &[rb, rc]);
            let said = format!("merge3 at {i}: {ra}");
            m.doc.splice(
                i..i + 3,
                [Entry {
                    node: into2,
                    rf: Some(ra),
                }],
            );
            Some(said)
        }
        // A change inside a block: the text, and a touch.
        6 => {
            let i = rng.below(m.doc.len());
            let tree = tx.part_mut(MAIN).unwrap();
            let text = tree
                .node_ref(m.doc[i].node)
                .first_child()
                .and_then(|r| r.first_child())
                .and_then(|t| t.first_child());
            if let Some(t) = text {
                let id = t.id();
                tree.edit().set_text(id, "edited").unwrap();
            }
            let k = key(tx, m.doc[i].node);
            tx.touch(&k);
            Some(format!("edit {i}"))
        }
        // A move: the same element, somewhere else.
        7 if m.doc.len() > 2 => {
            let from = rng.below(m.doc.len());
            let to = rng.below(m.doc.len());
            if from == to {
                return None;
            }
            let e = m.doc.remove(from);
            let tree = tx.part_mut(MAIN).unwrap();
            let parent = tree.node_ref(e.node).parent().unwrap().id();
            tree.edit().detach(e.node).unwrap();
            let before = m.doc.get(to).map(|x| x.node);
            tree.edit().insert_before(parent, before, e.node).unwrap();
            m.doc.insert(to.min(m.doc.len()), e);
            Some(format!("move {from} to {to}"))
        }
        _ => None,
    }
}

/// What `resolve` must say for every ref ever issued, in the state and in the transaction.
fn check_identity(
    m: &Model,
    resolve: &dyn Fn(&str) -> Result<BlockKey, StetError>,
    label: &str,
    messages: bool,
) {
    for (r, fate) in &m.issued {
        let answer = resolve(r);
        match fate {
            Fate::Live => {
                let want = m
                    .doc
                    .iter()
                    .find(|e| e.rf.as_deref() == Some(r))
                    .map(|e| e.node);
                let got = answer.unwrap_or_else(|e| panic!("{label}: {r} should be live: {e}"));
                assert_eq!(Some(got.node()), want, "{label}: {r} names another block");
            }
            Fate::Deleted => {
                let e = answer
                    .err()
                    .unwrap_or_else(|| panic!("{label}: {r} was deleted and resolves"));
                assert_eq!(e.code(), ErrorCode::StaleRef, "{label}: {r}");
                assert!(
                    !messages || e.message().starts_with(&format!("{r} was deleted")),
                    "{label}: {}",
                    e.message()
                );
            }
            Fate::MergedInto(into) => {
                let e = answer
                    .err()
                    .unwrap_or_else(|| panic!("{label}: {r} was merged and resolves"));
                assert_eq!(e.code(), ErrorCode::StaleRef, "{label}: {r}");
                assert!(
                    !messages || e.message().contains(&format!("merged into {into}")),
                    "{label}: {}",
                    e.message()
                );
            }
        }
    }
}

// ---- the oracle -----------------------------------------------------------------------------

/// How much of each kind of operation a run did, so that the oracle cannot pass by doing nothing.
#[derive(Default, Debug)]
struct Coverage(BTreeMap<String, usize>);

fn run(seed: u64, batches: usize, coverage: &mut Coverage) {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let texts: Vec<String> = (0..6).map(|i| format!("t{i}")).collect();
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let original = package_bytes(&paragraphs(&texts));
    let mut s = state_from(original.clone(), &Limits::default());
    let mut m = Model::of(&s);
    let mut history = vec![s.base_token()];
    for batch in 0..batches {
        let label = format!("seed {seed} batch {batch}");
        m.fresh_merges.clear();
        let saved = m.clone();
        let before = snapshot(&s);
        let base = s.base_token();
        let mut tx = s.begin();
        let ops = 1 + rng.below(3);
        let mut done = 0;
        let mut trace: Vec<String> = Vec::new();
        for _ in 0..ops {
            if let Some(said) = step(&mut rng, &mut tx, &mut m) {
                let kind = said.split([' ', ':']).next().unwrap_or("").to_owned();
                *coverage.0.entry(kind).or_default() += 1;
                trace.push(said);
                done += 1;
                let label = format!("{label} {trace:?}");
                // Inside the batch, every ref issued before it resolves to what the model says.
                let issued_before: Model = Model {
                    doc: m.doc.clone(),
                    issued: m
                        .issued
                        .iter()
                        .filter(|(r, _)| saved.issued.contains_key(*r))
                        .map(|(r, f)| (r.clone(), f.clone()))
                        .collect(),
                    next: m.next,
                    fresh_merges: Vec::new(),
                };
                check_identity(
                    &issued_before,
                    &|t| tx.resolve(Claim::Base(&base), t),
                    &format!("{label} in the batch"),
                    false,
                );
                let bad = incoherent(&tx.parts());
                assert!(
                    bad.is_empty(),
                    "{label}: stale caches in the transaction: {bad:?}"
                );
            }
        }
        if rng.below(5) == 0 {
            // A dry run: the state is as it was, and so is the model.
            tx.discard();
            assert_eq!(snapshot(&s), before, "{label}: a discard changed the state");
            m = saved;
            *coverage.0.entry("dry run".to_owned()).or_default() += 1;
            continue;
        }
        let committed = tx
            .commit()
            .unwrap_or_else(|e| panic!("{label} {trace:?}: {e}"));
        let (next, report) = committed;
        // New elements get the next numbers, in document order.
        let new_nodes: Vec<NodeId> = m
            .doc
            .iter()
            .filter(|e| e.rf.is_none())
            .map(|e| e.node)
            .collect();
        let minted: Vec<NodeId> = report.minted.iter().map(|(k, _)| k.node()).collect();
        assert_eq!(
            minted, new_nodes,
            "{label}: refs are minted for the new blocks in document order"
        );
        let mut fresh_entries = m.doc.iter_mut().filter(|e| e.rf.is_none());
        for (_, r) in &report.minted {
            let want = format!("p{}", m.next);
            assert_eq!(r.to_string(), want, "{label}: numbers are never reused");
            m.next += 1;
            m.issued.insert(r.to_string(), Fate::Live);
            if let Some(e) = fresh_entries.next() {
                e.rf = Some(r.to_string());
            }
        }
        assert_eq!(
            next.base_token().version(),
            s.base_token().version() + u32::from(done > 0),
            "{label}"
        );
        s = next;
        *coverage.0.entry("commit".to_owned()).or_default() += 1;
        check_identity(&m, &|t| s.resolve(Claim::Implied, t), &label, true);
        let now = s.base_token();
        check_identity(
            &m,
            &|t| s.begin().resolve(Claim::Base(&now), t),
            &label,
            true,
        );
        // One to one: every block has a ref, and the live refs are exactly the blocks.
        let blocks = body_keys(&s);
        assert_eq!(blocks.len(), m.doc.len(), "{label}");
        let live = m
            .issued
            .values()
            .filter(|f| matches!(f, Fate::Live))
            .count();
        assert_eq!(live, blocks.len(), "{label}: one ref for each block");
        let bad = incoherent(&s.parts());
        assert!(
            bad.is_empty(),
            "{label}: stale caches in the state: {bad:?}"
        );
        // A claim from any earlier version names the right block or is refused.
        for old in &history {
            for (r, fate) in &m.issued {
                if let Ok(k) = s.begin().resolve(Claim::Base(old), r) {
                    let want = m
                        .doc
                        .iter()
                        .find(|e| e.rf.as_deref() == Some(r))
                        .map(|e| e.node);
                    assert!(
                        matches!(fate, Fate::Live),
                        "{label}: an old claim resolved the dead {r}"
                    );
                    assert_eq!(Some(k.node()), want, "{label}: {r} under an old claim");
                }
            }
        }
        history.push(now);
        if batch % 8 == 7 {
            // The saved form brings the same state back.
            let saved_form = s.export().unwrap();
            let rec = Recorded {
                base: saved_form.base,
                head_sha256: saved_form.head_sha256,
                refs_sha256: saved_form.refs_sha256,
            };
            let back = State::resume(
                saved_form.head,
                &saved_form.refs,
                &rec,
                Some(&original),
                &Limits::default(),
            )
            .unwrap_or_else(|e| panic!("{label}: resume: {e}"));
            assert_eq!(back.base_token(), s.base_token(), "{label}");
            for r in m.issued.keys() {
                match (
                    s.resolve(Claim::Implied, r),
                    back.resolve(Claim::Implied, r),
                ) {
                    (Ok(_), Ok(_)) => {}
                    (Err(a), Err(b)) => assert_eq!(a, b, "{label}: {r}"),
                    (a, b) => panic!("{label}: {r}: {a:?} against {b:?}"),
                }
            }
        }
    }
}

#[test]
fn random_edits_never_make_a_ref_name_another_block() {
    let mut coverage = Coverage::default();
    for seed in 1..=60 {
        run(seed, 25, &mut coverage);
    }
    let did = |kind: &str| coverage.0.get(kind).copied().unwrap_or(0);
    for (kind, at_least) in [
        ("insert", 100),
        ("delete", 100),
        ("split", 150),
        ("merge2", 100),
        ("merge3", 50),
        ("edit", 100),
        ("move", 50),
        ("commit", 1000),
        ("dry run", 100),
    ] {
        assert!(
            did(kind) >= at_least,
            "{kind}: {} of at least {at_least}: {coverage:?}",
            did(kind)
        );
    }
}
