//! Tests of the ref table: numbering, resolution and every error code, tombstones, rebinding and
//! merging, the lineage, and the cap. They use `wordml::testing` for keys, so `refs` needs nothing
//! from `xml`.

use super::base::{BaseToken, Claim, Lineage, NamespaceId, NamespaceOrigin};
use super::layout::{BlockSpec, Layout, StoryBlocks};
use super::table::{Merged, Reason, RefTable, Slot};
use super::token::{BlockRef, RefKind};
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;
use crate::wordml::{BlockKey, Story, StoryKind, testing};
use std::sync::Arc;

/// p, table, row, p in a cell, row, p in a cell, p: the refs p1 t1 r1 p2 r2 p3 p4.
pub(super) const DOC: &str =
    "<w:p/><w:tbl><w:tr><w:tc><w:p/></w:tc></w:tr><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>";

/// The bytes the test documents were "opened" from.
pub(super) const ORIGINAL: &[u8] = b"the document";

pub(super) fn ns() -> NamespaceId {
    NamespaceId::of_package(ORIGINAL, crate::wordml::SCAN_VERSION)
}

/// A layout with one story, with a fingerprint that depends on the position.
pub(super) fn layout_of(story: &Story) -> Layout {
    Layout {
        stories: vec![story_blocks(story)],
    }
}

pub(super) fn story_blocks(story: &Story) -> StoryBlocks {
    StoryBlocks {
        part: Arc::clone(story.part()),
        kind: story.kind(),
        blocks: story
            .blocks()
            .iter()
            .enumerate()
            .map(|(i, e)| BlockSpec {
                key: story.key(e),
                kind: e.kind,
                fingerprint: 1000 + i as u64 * 7919,
            })
            .collect(),
    }
}

pub(super) fn open(layout: &Layout) -> RefTable {
    RefTable::open(ns(), NamespaceOrigin::Opened, layout, &Limits::default()).unwrap()
}

/// The table for `DOC`, its layout and its keys.
pub(super) fn doc_table() -> (RefTable, Layout, Vec<BlockKey>) {
    let story = testing::story(DOC);
    let layout = layout_of(&story);
    (open(&layout), layout, testing::keys(&story))
}

/// `n` keys of elements that no `DOC` key names: the elements are of another part, so the node
/// numbers of the two trees cannot collide.
pub(super) fn fresh_keys(n: usize) -> Vec<BlockKey> {
    let story = testing::story(&"<w:p/>".repeat(n));
    let part: Arc<str> = Arc::from("word/fresh.xml");
    testing::keys(&story)
        .iter()
        .map(|k| BlockKey::new(Arc::clone(&part), k.node()))
        .collect()
}

fn token(t: &RefTable, version: u32) -> BaseToken {
    BaseToken::new(t.namespace(), version, t.lineage_at(version).unwrap())
}

fn code(e: Result<BlockKey, crate::api::error::StetError>) -> ErrorCode {
    e.unwrap_err().code()
}

fn refs_of(t: &RefTable, keys: &[BlockKey]) -> Vec<String> {
    keys.iter()
        .map(|k| t.ref_of(k).map_or("-".to_owned(), |r| r.to_string()))
        .collect()
}

#[test]
fn open_mints_per_kind_in_document_order() {
    let (t, _, keys) = doc_table();
    assert_eq!(
        refs_of(&t, &keys),
        ["p1", "t1", "r1", "p2", "r2", "p3", "p4"]
    );
    assert_eq!(t.count(RefKind::Paragraph), 4);
    assert_eq!(t.count(RefKind::Table), 1);
    assert_eq!(t.count(RefKind::Row), 2);
    assert_eq!(t.version(), 0);
    assert_eq!(t.base_token(), BaseToken::initial(ns()));
}

#[test]
fn stories_are_numbered_in_story_order_and_the_same_bytes_give_the_same_numbers() {
    let body = testing::story("<w:p/><w:p/>");
    let header_tree = testing::part("word/header1.xml", "hdr", "<w:p/>");
    let header = Story::scan(
        &Arc::from("word/header1.xml"),
        StoryKind::Header,
        &header_tree,
    )
    .unwrap();
    let layout = Layout {
        stories: vec![story_blocks(&body), story_blocks(&header)],
    };
    let t = open(&layout);
    let all: Vec<BlockKey> = testing::keys(&body)
        .into_iter()
        .chain(testing::keys(&header))
        .collect();
    assert_eq!(refs_of(&t, &all), ["p1", "p2", "p3"]);
    let again = open(&layout);
    assert_eq!(refs_of(&again, &all), refs_of(&t, &all));
}

#[test]
fn a_slot_is_sixteen_bytes() {
    assert!(
        std::mem::size_of::<Slot>() <= 16,
        "{}",
        std::mem::size_of::<Slot>()
    );
}

#[test]
fn resolve_returns_the_block_a_ref_names() {
    let (t, _, keys) = doc_table();
    for (i, want) in ["p1", "t1", "r1", "p2", "r2", "p3", "p4"]
        .iter()
        .enumerate()
    {
        assert_eq!(t.resolve(Claim::Implied, want).unwrap(), keys[i], "{want}");
        let base = t.base_token();
        assert_eq!(
            t.resolve(Claim::Base(&base), want).unwrap(),
            keys[i],
            "{want}"
        );
    }
}

#[test]
fn a_malformed_token_is_invalid_payload_and_a_never_minted_one_is_not_found() {
    let (t, _, _) = doc_table();
    for bad in ["p0", "p007", "P1", "x1", "", "p"] {
        assert_eq!(
            code(t.resolve(Claim::Implied, bad)),
            ErrorCode::InvalidPayload,
            "{bad}"
        );
    }
    let e = t.resolve(Claim::Implied, "p9000").unwrap_err();
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.message().contains("p1 to p4"), "{}", e.message());
    let e = t.resolve(Claim::Implied, "t2").unwrap_err();
    assert!(e.message().contains("t1"), "{}", e.message());
    let empty = open(&Layout::default());
    let e = empty.resolve(Claim::Implied, "p1").unwrap_err();
    assert_eq!(e.code(), ErrorCode::NotFound);
    assert!(e.message().contains("no paragraphs"), "{}", e.message());
}

#[test]
fn the_claim_is_checked_before_the_token_is_read() {
    let (t, _, _) = doc_table();
    let other = BaseToken::initial(NamespaceId::of_package(b"another document", 1));
    // A wrong namespace with a malformed token is still stale_ref, and says nothing of the table.
    let e = t.resolve(Claim::Base(&other), "banana").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert!(e.message().contains("another document"), "{}", e.message());
    assert!(!e.message().contains("p1 to"), "{}", e.message());
}

#[test]
fn a_claim_from_another_namespace_the_future_or_another_history_is_stale_ref() {
    let (mut t, _, _) = doc_table();
    let digest = [9u8; 32];
    t.advance(&digest).unwrap();
    t.advance(&digest).unwrap();
    assert_eq!(t.version(), 2);
    // The document's own tokens, at every version, pass.
    for v in 0..=2 {
        assert!(
            t.resolve(Claim::Base(&token(&t, v)), "p1").is_ok(),
            "version {v}"
        );
    }
    // Another namespace.
    let other = BaseToken::initial(NamespaceId::of_package(b"elsewhere", 1));
    assert_eq!(
        code(t.resolve(Claim::Base(&other), "p1")),
        ErrorCode::StaleRef
    );
    // A version after this document's.
    let future = BaseToken::new(
        t.namespace(),
        3,
        Lineage::root(&t.namespace()).next(3, &digest),
    );
    let e = t.resolve(Claim::Base(&future), "p1").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert!(
        e.message().contains("version 3") && e.message().contains("version 2"),
        "{}",
        e.message()
    );
    // Another history: the right namespace and version, and a lineage this document never had.
    let fork = BaseToken::new(
        t.namespace(),
        2,
        Lineage::root(&t.namespace())
            .next(1, &[1; 32])
            .next(2, &digest),
    );
    let e = t.resolve(Claim::Base(&fork), "p1").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert!(e.message().contains("different history"), "{}", e.message());
    // The right lineage at the wrong version is another history too.
    let skewed = BaseToken::new(t.namespace(), 1, t.lineage_at(2).unwrap());
    assert_eq!(
        code(t.resolve(Claim::Base(&skewed), "p1")),
        ErrorCode::StaleRef
    );
}

#[test]
fn a_gone_ref_is_stale_ref_with_its_reason() {
    let (mut t, _, keys) = doc_table();
    let r = |s: &str| BlockRef::parse(s).unwrap();
    t.tombstone(r("p1"), Reason::Deleted, 1, &Merged::default())
        .unwrap();
    t.tombstone(r("p4"), Reason::Rejected, 2, &Merged::default())
        .unwrap();
    t.merge(r("p2"), r("p3"), &keys[3], 3, &mut Merged::default())
        .unwrap();
    let e = t.resolve(Claim::Implied, "p1").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert_eq!(e.message(), "p1 was deleted in version 1.");
    let e = t.resolve(Claim::Implied, "p4").unwrap_err();
    assert!(
        e.message().contains("insertion was rejected"),
        "{}",
        e.message()
    );
    let e = t.resolve(Claim::Implied, "p3").unwrap_err();
    assert_eq!(e.message(), "p3 was merged into p2 in version 3.");
    // A merge message looks at its target once.
    t.tombstone(r("p2"), Reason::Deleted, 4, &Merged::default())
        .unwrap();
    let e = t.resolve(Claim::Implied, "p3").unwrap_err();
    assert_eq!(
        e.message(),
        "p3 was merged into p2 in version 3, and p2 has since been removed."
    );
    // The number is never reused: the next paragraph is p5.
    let fresh = fresh_keys(1);
    let minted = t
        .allocate(&fresh[0], crate::wordml::BlockKind::Paragraph, 5)
        .unwrap();
    assert_eq!(minted.to_string(), "p5");
}

#[test]
fn a_block_changed_after_the_claims_version_is_stale_ref_and_one_untouched_is_not() {
    let (mut t, _, keys) = doc_table();
    let digest = [3u8; 32];
    t.advance(&digest).unwrap();
    let v1 = token(&t, 1);
    t.touch(&keys[0], 2);
    t.advance(&digest).unwrap();
    t.advance(&digest).unwrap();
    let v0 = token(&t, 0);
    let v2 = token(&t, 2);
    // p1 changed in version 2.
    let e = t.resolve(Claim::Base(&v1), "p1").unwrap_err();
    assert_eq!(e.code(), ErrorCode::StaleRef);
    assert!(
        e.message()
            .contains("changed in version 2, after version 1"),
        "{}",
        e.message()
    );
    assert!(e.next().contains("View the block again"), "{}", e.next());
    assert!(t.resolve(Claim::Base(&v0), "p1").is_err());
    // At version 2 or later it is fine, and so is a block nothing touched, at any version.
    assert!(t.resolve(Claim::Base(&v2), "p1").is_ok());
    assert!(t.resolve(Claim::Base(&t.base_token()), "p1").is_ok());
    assert!(t.resolve(Claim::Base(&v0), "p2").is_ok());
    assert!(t.resolve(Claim::Base(&v0), "t1").is_ok());
    // The read path with no claim does not apply the rule.
    assert!(t.resolve(Claim::Implied, "p1").is_ok());
}

#[test]
fn a_ref_minted_after_the_claims_version_is_stale_ref() {
    let (mut t, _, _) = doc_table();
    t.advance(&[1; 32]).unwrap();
    let v0 = token(&t, 0);
    let fresh = fresh_keys(1);
    let minted = t
        .allocate(&fresh[0], crate::wordml::BlockKind::Paragraph, 2)
        .unwrap();
    t.advance(&[2; 32]).unwrap();
    let v1 = token(&t, 1);
    for claim in [v0, v1] {
        let e = t
            .resolve(Claim::Base(&claim), &minted.to_string())
            .unwrap_err();
        assert_eq!(e.code(), ErrorCode::StaleRef, "{claim}");
    }
    assert!(
        t.resolve(Claim::Base(&t.base_token()), &minted.to_string())
            .is_ok()
    );
}

#[test]
fn the_lineage_chain_is_deterministic_and_forks_stay_forks() {
    let (mut a, _, _) = doc_table();
    let (mut b, _, _) = doc_table();
    let (mut c, _, _) = doc_table();
    assert_eq!(a.base_token(), b.base_token());
    let x = a.advance(&[1; 32]).unwrap();
    assert_eq!(
        x,
        b.advance(&[1; 32]).unwrap(),
        "the same history, the same token"
    );
    assert_ne!(
        x,
        c.advance(&[2; 32]).unwrap(),
        "another change, another token"
    );
    assert_eq!(x.version(), 1);
    assert_ne!(x.lineage(), BaseToken::initial(ns()).lineage());
    // A token from one fork is refused by the other, at the same version.
    let from_c = c.base_token();
    assert_eq!(
        code(a.resolve(Claim::Base(&from_c), "p1")),
        ErrorCode::StaleRef
    );
    // And a token from an earlier history is accepted after the history moves on.
    a.advance(&[5; 32]).unwrap();
    assert!(a.resolve(Claim::Base(&x), "p1").is_ok());
}

#[test]
fn the_table_and_the_history_stop_at_max_refs() {
    let story = testing::story(DOC);
    let layout = layout_of(&story);
    let few = Limits {
        max_refs: 5,
        ..Limits::default()
    };
    let e = RefTable::open(ns(), NamespaceOrigin::Opened, &layout, &few).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    let exact = Limits {
        max_refs: 7,
        ..Limits::default()
    };
    let mut t = RefTable::open(ns(), NamespaceOrigin::Opened, &layout, &exact).unwrap();
    let fresh = fresh_keys(1);
    let e = t
        .allocate(&fresh[0], crate::wordml::BlockKind::Paragraph, 1)
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert_eq!(t.total(), 7, "a refused ref is not minted");
    for i in 0..7u8 {
        t.advance(&[i; 32]).unwrap();
    }
    let e = t.advance(&[9; 32]).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    assert_eq!(t.version(), 7);
}

#[test]
fn live_refs_of_a_part_come_in_slot_order() {
    let (mut t, _, keys) = doc_table();
    let r = |s: &str| BlockRef::parse(s).unwrap();
    t.tombstone(r("p2"), Reason::Deleted, 1, &Merged::default())
        .unwrap();
    let live: Vec<String> = t
        .live_in("word/document.xml")
        .iter()
        .map(|(r, _)| r.to_string())
        .collect();
    assert_eq!(live, ["p1", "p3", "p4", "t1", "r1", "r2"]);
    assert!(t.live_in("word/other.xml").is_empty());
    assert_eq!(t.live_in("word/document.xml")[0].1, keys[0].node());
}

/// A model of the table that the property test compares against: for each block, the number of
/// live refs it has, and the numbers that were minted.
fn invariants(t: &RefTable) {
    let mut live = 0;
    for kind in RefKind::ALL {
        for n in 1..=t.count(kind) {
            let r = BlockRef::new(kind, n).unwrap();
            match t.slot(r).unwrap() {
                Slot::Live { part, node, .. } => {
                    live += 1;
                    let name = t.part_name(*part).unwrap().clone();
                    assert_eq!(t.ref_of(&BlockKey::new(name, *node)), Some(r), "{r}");
                }
                Slot::Gone { .. } => {}
            }
        }
        assert!(
            t.slot(BlockRef::new(kind, t.count(kind) + 1).unwrap())
                .is_none()
        );
    }
    assert_eq!(
        live,
        t.by_key_len(),
        "the reverse map is one to one with the live slots"
    );
}

#[test]
fn random_sequences_keep_the_table_dense_one_to_one_and_forward_only() {
    let story = testing::story(&"<w:p/>".repeat(60));
    let all = testing::keys(&story);
    let (initial, spare) = all.split_at(30);
    let layout = Layout {
        stories: vec![StoryBlocks {
            part: Arc::clone(story.part()),
            kind: story.kind(),
            blocks: initial
                .iter()
                .map(|k| BlockSpec {
                    key: k.clone(),
                    kind: crate::wordml::BlockKind::Paragraph,
                    fingerprint: 0,
                })
                .collect(),
        }],
    };
    for seed in 1..=40u64 {
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        let mut next = move || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let mut t = open(&layout);
        let mut merged = Merged::default();
        let mut free: Vec<BlockKey> = spare.to_vec();
        let mut version = 0;
        let mut gone: Vec<BlockRef> = Vec::new();
        let mut counter_before = t.count(RefKind::Paragraph);
        for _ in 0..200 {
            let live: Vec<BlockRef> = (1..=t.count(RefKind::Paragraph))
                .filter_map(|n| BlockRef::new(RefKind::Paragraph, n))
                .filter(|r| matches!(t.slot(*r), Some(Slot::Live { .. })))
                .collect();
            let pick = |n: u64| live.get((n % live.len().max(1) as u64) as usize).copied();
            match next() % 6 {
                0 => {
                    if let Some(key) = free.pop() {
                        t.allocate(&key, crate::wordml::BlockKind::Paragraph, version)
                            .unwrap();
                    }
                }
                1 => {
                    if let Some(r) = pick(next()) {
                        t.tombstone(r, Reason::Deleted, version, &merged).unwrap();
                        gone.push(r);
                    }
                }
                2 => {
                    if let (Some(r), Some(key)) = (pick(next()), free.pop()) {
                        t.rebind(r, &key).unwrap();
                    }
                }
                3 => {
                    let (a, b) = (pick(next()), pick(next()));
                    if let (Some(a), Some(b), Some(key)) = (a, b, free.pop()) {
                        if a != b {
                            t.merge(a, b, &key, version, &mut merged).unwrap();
                            gone.push(b);
                        } else {
                            free.push(key);
                        }
                    }
                }
                4 => {
                    if let Some(r) = pick(next())
                        && let Ok(key) = t.resolve(Claim::Implied, &r.to_string())
                    {
                        t.touch(&key, version);
                    }
                }
                _ => {
                    version += 1;
                    merged = Merged::default();
                    t.advance(&[(next() % 256) as u8; 32]).unwrap();
                }
            }
            invariants(&t);
            assert!(
                t.count(RefKind::Paragraph) >= counter_before,
                "a counter never goes down"
            );
            counter_before = t.count(RefKind::Paragraph);
            for r in &gone {
                assert!(
                    matches!(t.slot(*r), Some(Slot::Gone { .. })),
                    "{r} came back"
                );
            }
            assert_eq!(t.version(), version);
        }
    }
}
