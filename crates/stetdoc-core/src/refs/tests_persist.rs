//! Tests of the persisted form: the exact blob, the round trips, and what `restore` does with
//! hostile input. Every rule of section 8.4 of the design has a blob that breaks it.

use super::base::{BaseToken, Claim, NamespaceId, NamespaceOrigin};
use super::layout::Layout;
use super::table::{Merged, Reason, RefTable};
use super::tests::{DOC, ORIGINAL, layout_of, ns, open};
use super::token::BlockRef;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::wordml::{BlockKind, Story, StoryKind, testing};
use std::sync::Arc;

pub(super) fn r(s: &str) -> BlockRef {
    BlockRef::parse(s).unwrap()
}

/// A table at version 2 with every kind of record: a retired ref (deleted), a merge, a stamp, and
/// a ref minted later. Returns it with its layout.
pub(super) fn busy() -> (RefTable, Layout) {
    let story = testing::story(DOC);
    let keys = testing::keys(&story);
    let mut layout = layout_of(&story);
    let mut t = open(&layout);
    t.tombstone(r("p1"), Reason::Deleted, 1, &Merged::default())
        .unwrap();
    t.advance(&[1; 32]).unwrap();
    t.merge(r("p2"), r("p3"), &keys[3], 2, &mut Merged::default())
        .unwrap();
    t.touch(&keys[2], 2);
    t.advance(&[2; 32]).unwrap();
    // The layout is the document after those edits: p1's block and p3's are gone from it.
    let block = |i: usize| layout.stories[0].blocks[i].clone();
    let kept: Vec<_> = [1, 2, 3, 4, 6].iter().map(|i| block(*i)).collect();
    layout.stories[0].blocks = kept;
    (t, layout)
}

pub(super) fn good() -> (RefTable, Layout, String) {
    let (t, layout) = busy();
    let blob = String::from_utf8(t.persist(&layout).unwrap()).unwrap();
    (t, layout, blob)
}

pub(super) fn restore(blob: &str, layout: &Layout) -> Result<RefTable, StetError> {
    RefTable::restore(blob.as_bytes(), layout, Some(ORIGINAL), &Limits::default())
}

pub(super) fn code_of(blob: &str, layout: &Layout) -> ErrorCode {
    match restore(blob, layout) {
        Ok(_) => panic!("accepted:\n{blob}"),
        Err(e) => e.code(),
    }
}

pub(super) fn replace_line(blob: &str, find: &str, with: &str) -> String {
    assert!(
        blob.lines().any(|l| l == find),
        "no line {find:?} in\n{blob}"
    );
    blob.lines()
        .map(|l| if l == find { with } else { l })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

#[test]
fn the_blob_is_canonical_text() {
    let story = testing::story(DOC);
    let layout = layout_of(&story);
    let t = open(&layout);
    let blob = String::from_utf8(t.persist(&layout).unwrap()).unwrap();
    let fp = |i: u64| format!("{:016x}", 1000 + i * 7919);
    let expected = format!(
        "stet-refs 1\nnamespace {} opened\nscan 1 1\nversion 0\nlineage 0\ncounts 4 1 2\nstories 1\n\
         story word/document.xml body 7\np1 {}\nt1 {}\nr1 {}\np2 {}\nr2 {}\np3 {}\np4 {}\ngone 0\nend\n",
        ns(),
        fp(0),
        fp(1),
        fp(2),
        fp(3),
        fp(4),
        fp(5),
        fp(6)
    );
    assert_eq!(blob, expected);
}

#[test]
fn a_table_with_history_writes_every_kind_of_record() {
    let (_, _, blob) = good();
    let lines: Vec<&str> = blob.lines().collect();
    assert!(lines.contains(&"version 2") && lines.contains(&"lineage 2"));
    assert!(lines.contains(&"gone 2"));
    assert!(lines.contains(&"p1 d 1"), "{blob}");
    assert!(lines.contains(&"p3 m p2 2"), "{blob}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("r1 ") && l.ends_with(" 2")),
        "the stamp of r1: {blob}"
    );
    assert_eq!(lines.last(), Some(&"end"));
}

#[test]
fn persist_is_deterministic() {
    let (t, layout) = busy();
    assert_eq!(t.persist(&layout).unwrap(), t.persist(&layout).unwrap());
    let (t2, layout2) = busy();
    assert_eq!(t.persist(&layout).unwrap(), t2.persist(&layout2).unwrap());
}

fn same_answers(a: &RefTable, b: &RefTable) {
    assert_eq!(a.base_token(), b.base_token());
    for token in ["p1", "p2", "p3", "p4", "p5", "t1", "t2", "r1", "r2", "r3"] {
        for claim in [Claim::Implied, Claim::Base(&a.base_token())] {
            let (x, y) = (a.resolve(claim, token), b.resolve(claim, token));
            match (x, y) {
                (Ok(x), Ok(y)) => assert_eq!(x, y, "{token}"),
                (Err(x), Err(y)) => assert_eq!(x, y, "{token}"),
                (x, y) => panic!("{token}: {x:?} against {y:?}"),
            }
        }
    }
}

#[test]
fn restore_gives_back_the_table_and_writing_it_again_gives_the_blob() {
    let (t, layout, blob) = good();
    let back = restore(&blob, &layout).unwrap();
    same_answers(&t, &back);
    assert_eq!(back.persist(&layout).unwrap(), blob.as_bytes());
    // Every number, live or gone, is the same, and the next ref continues the sequence.
    let story = testing::story(DOC);
    let fresh = testing::keys(&story);
    let part: Arc<str> = Arc::from("word/fresh.xml");
    let key = crate::wordml::BlockKey::new(part, fresh[0].node());
    let mut back = back;
    let mut t = t;
    let a = t.allocate(&key, BlockKind::Paragraph, 3).unwrap();
    let b = back.allocate(&key, BlockKind::Paragraph, 3).unwrap();
    assert_eq!(a, b);
    assert_eq!(a.to_string(), "p5");
    // And the lineage continues the same way.
    assert_eq!(
        t.advance(&[7; 32]).unwrap(),
        back.advance(&[7; 32]).unwrap()
    );
}

#[test]
fn a_fresh_table_round_trips() {
    let story = testing::story(DOC);
    let layout = layout_of(&story);
    let t = open(&layout);
    let blob = String::from_utf8(t.persist(&layout).unwrap()).unwrap();
    let back = restore(&blob, &layout).unwrap();
    same_answers(&t, &back);
    assert_eq!(back.base_token(), BaseToken::initial(ns()));
}

#[test]
fn stories_with_unusual_names_round_trip() {
    let body = testing::story("<w:p/>");
    let tree = testing::part("word/head er%1é.xml", "hdr", "<w:p/><w:p/>");
    let header = Story::scan(&Arc::from("word/head er%1é.xml"), StoryKind::Header, &tree).unwrap();
    let layout = Layout {
        stories: vec![
            super::tests::story_blocks(&body),
            super::tests::story_blocks(&header),
        ],
    };
    let t = open(&layout);
    let blob = String::from_utf8(t.persist(&layout).unwrap()).unwrap();
    assert!(
        blob.contains("story word/head%20er%251%C3%A9.xml header 2"),
        "{blob}"
    );
    let back = restore(&blob, &layout).unwrap();
    assert_eq!(back.persist(&layout).unwrap(), blob.as_bytes());
}

#[test]
fn a_derived_namespace_is_checked_by_recomputing_it() {
    let story = testing::story(DOC);
    let layout = layout_of(&story);
    let a = NamespaceId::of_package(b"a", 1);
    let origin = NamespaceOrigin::Derived {
        label: "compare".into(),
        parents: vec![a, ns()],
        extra: b"stamp".to_vec(),
    };
    let id = NamespaceId::derived("compare", &[a, ns()], b"stamp");
    let t = RefTable::open(id, origin, &layout, &Limits::default()).unwrap();
    let blob = String::from_utf8(t.persist(&layout).unwrap()).unwrap();
    assert!(
        blob.contains(&format!(
            "namespace {id} derived compare 2 {a} {} 7374616d70",
            ns()
        )),
        "{blob}"
    );
    // It needs no original bytes, and restores.
    let back = RefTable::restore(blob.as_bytes(), &layout, None, &Limits::default()).unwrap();
    assert_eq!(back.namespace(), id);
    assert_eq!(back.persist(&layout).unwrap(), blob.as_bytes());
    // Edit an input of the derivation and the namespace no longer follows.
    let forged = blob.replace("7374616d70", "7374616d71");
    assert_eq!(
        RefTable::restore(forged.as_bytes(), &layout, None, &Limits::default())
            .unwrap_err()
            .code(),
        ErrorCode::CorruptComplement
    );
}

#[test]
fn an_opened_namespace_is_checked_against_the_original_bytes() {
    let (_, layout, blob) = good();
    let lim = Limits::default();
    let none = RefTable::restore(blob.as_bytes(), &layout, None, &lim).unwrap_err();
    assert_eq!(none.code(), ErrorCode::CorruptComplement);
    let wrong =
        RefTable::restore(blob.as_bytes(), &layout, Some(b"another document"), &lim).unwrap_err();
    assert_eq!(wrong.code(), ErrorCode::CorruptComplement);
    let forged = blob.replace(
        &ns().to_string(),
        &NamespaceId::of_package(b"x", 1).to_string(),
    );
    assert_eq!(code_of(&forged, &layout), ErrorCode::CorruptComplement);
}
