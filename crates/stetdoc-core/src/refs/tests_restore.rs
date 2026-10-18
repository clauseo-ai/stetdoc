//! Tests of `restore` on hostile input: every rule of section 8.4 of the design has a blob that
//! breaks it, and the property that whatever `restore` accepts, `persist` writes back unchanged.

use super::layout::{Layout, StoryBlocks};
use super::table::RefTable;
use super::tests::{DOC, ORIGINAL, layout_of, ns, open};
use super::tests_persist::{code_of, good, r, replace_line, restore};
use crate::api::error::ErrorCode;
use crate::api::limits::Limits;
use crate::wordml::{BlockKind, StoryKind, testing};
use std::sync::Arc;

#[test]
fn the_format_and_scan_versions_are_checked_before_anything_else() {
    let (_, layout, blob) = good();
    for newer in ["stet-refs 2", "stet-refs 4294967295"] {
        let e = restore(&replace_line(&blob, "stet-refs 1", newer), &layout).unwrap_err();
        assert_eq!(e.code(), ErrorCode::UnsupportedVersion, "{newer}");
    }
    for bad in [
        "stet-refs 0",
        "stet-refs 01",
        "stet-refs",
        "stet-refs ",
        "stet-refs -1",
        "stet-refz 1",
        "STET-REFS 1",
        " stet-refs 1",
        "stet-refs 1 ",
    ] {
        assert_eq!(
            code_of(&replace_line(&blob, "stet-refs 1", bad), &layout),
            ErrorCode::CorruptComplement,
            "{bad:?}"
        );
    }
    for scan in ["scan 2 1", "scan 1 2", "scan 0 0"] {
        let e = restore(&replace_line(&blob, "scan 1 1", scan), &layout).unwrap_err();
        assert_eq!(e.code(), ErrorCode::UnsupportedVersion, "{scan}");
        assert!(
            e.message().contains("number the blocks differently"),
            "{}",
            e.message()
        );
    }
    // A version this release cannot read wins over damage further down.
    let damaged = replace_line(
        &replace_line(&blob, "scan 1 1", "scan 9 1"),
        "gone 2",
        "gone banana",
    );
    assert_eq!(code_of(&damaged, &layout), ErrorCode::UnsupportedVersion);
}

#[test]
fn a_blob_over_the_size_the_cap_allows_is_refused_before_it_is_read() {
    let (_, layout, _) = good();
    let tiny = Limits {
        max_refs: 1,
        ..Limits::default()
    };
    let big = vec![b'a'; 64 + 65_536 + 1];
    let e = RefTable::restore(&big, &layout, None, &tiny).unwrap_err();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
}

#[test]
fn counts_and_versions_are_bounded_by_the_blob_and_the_cap() {
    let (_, layout, blob) = good();
    let huge = replace_line(
        &blob,
        "counts 4 1 2",
        "counts 4294967295 4294967295 4294967295",
    );
    assert_eq!(code_of(&huge, &layout), ErrorCode::LimitExceeded);
    let more_than_fit = replace_line(&blob, "counts 4 1 2", "counts 400000 1 2");
    assert_eq!(code_of(&more_than_fit, &layout), ErrorCode::LimitExceeded);
    let few = Limits {
        max_refs: 6,
        ..Limits::default()
    };
    let e = RefTable::restore(blob.as_bytes(), &layout, Some(ORIGINAL), &few).unwrap_err();
    assert_eq!(
        e.code(),
        ErrorCode::LimitExceeded,
        "7 refs against a cap of 6"
    );
    // A version no blob of this size could carry.
    let long = replace_line(&blob, "version 2", "version 1000000");
    assert_eq!(code_of(&long, &layout), ErrorCode::CorruptComplement);
}

#[test]
fn the_parser_refuses_every_spelling_that_is_not_canonical() {
    let (_, layout, blob) = good();
    let cases = [
        blob.replace('\n', "\r\n"),
        blob.replace("end\n", "end\n\n"),
        blob.replace("end\n", "end"),
        blob.replace("end\n", "end\nmore\n"),
        blob.replace("end\n", "end \n"),
        blob.replace(" opened", "  opened"),
        blob.replace("body 5", "body 05"),
        blob.replacen("p2 ", "p2  ", 1),
        blob.replace("p1 d 1", "p1 d 01"),
        blob.replace("counts 4 1 2", "counts 4 1 2 "),
        blob.to_uppercase(),
        blob.replace("version 2", "version 02"),
        blob.replace("end\n", "end\n\0"),
        blob.replace("body", "bodY"),
        format!("{blob}{}", "x".repeat(100_000)),
        blob.replace("end\n", &format!("{}\nend\n", "x".repeat(100_000))),
        blob.replace("stet-refs 1\n", "stet-refs 1\n\n"),
        blob.replace('é', "e").replace("scan", "sc\u{e9}n"),
    ];
    for (i, bad) in cases.iter().enumerate() {
        assert_eq!(
            code_of(bad, &layout),
            ErrorCode::CorruptComplement,
            "case {i}"
        );
    }
}

#[test]
fn the_history_must_have_a_link_for_every_version() {
    let (_, layout, blob) = good();
    let link = blob.lines().nth(5).unwrap().to_owned();
    for bad in [
        blob.replace("lineage 2", "lineage 1"),
        blob.replace("lineage 2", "lineage 3"),
        blob.replacen(&format!("{link}\n"), "", 1),
        blob.replacen(&link, &link.to_uppercase(), 1),
        blob.replacen(&link, &link[1..], 1),
    ] {
        assert_eq!(code_of(&bad, &layout), ErrorCode::CorruptComplement);
    }
    // A stamp above the version, a retirement above it and at 0.
    for bad in [
        blob.replace("p1 d 1", "p1 d 3"),
        blob.replace("p1 d 1", "p1 d 0"),
        blob.replace("p3 m p2 2", "p3 m p2 3"),
        blob.replace("p3 m p2 2", "p3 m p2 0"),
    ] {
        assert_eq!(code_of(&bad, &layout), ErrorCode::CorruptComplement);
    }
    let stamp = blob
        .lines()
        .find(|l| l.starts_with("r1 "))
        .unwrap()
        .to_owned();
    let hex: String = stamp.split(' ').nth(1).unwrap().to_owned();
    for bad in [
        format!("r1 {hex} 3"),
        format!("r1 {hex} 0"),
        format!("r1 {hex} 02"),
    ] {
        assert_eq!(
            code_of(&replace_line(&blob, &stamp, &bad), &layout),
            ErrorCode::CorruptComplement,
            "{bad}"
        );
    }
}

#[test]
fn the_table_must_be_whole_and_consistent() {
    let (_, layout, blob) = good();
    for (what, bad) in [
        ("a ref listed twice", blob.replace("p2 ", "p4 ")),
        ("a ref above the count", blob.replacen("p4 ", "p9 ", 1)),
        (
            "a gap: p1 is neither live nor retired",
            blob.replace("p1 d 1\n", "").replace("gone 2", "gone 1"),
        ),
        (
            "a ref both live and retired",
            blob.replace("p1 d 1", "p2 d 1"),
        ),
        (
            "retired refs out of order",
            blob.replace("p1 d 1\np3 m p2 2", "p3 m p2 2\np1 d 1"),
        ),
        ("a retired ref twice", blob.replace("p3 m p2 2", "p1 d 2")),
        (
            "a count that disagrees",
            blob.replace("counts 4 1 2", "counts 5 1 2"),
        ),
        (
            "a wrong number of retired refs",
            blob.replace("gone 2", "gone 3"),
        ),
        ("a wrong reason", blob.replace("p1 d 1", "p1 q 1")),
        ("a merge field missing", blob.replace("p3 m p2 2", "p3 m 2")),
    ] {
        assert_eq!(
            code_of(&bad, &layout),
            ErrorCode::CorruptComplement,
            "{what}"
        );
    }
}

#[test]
fn a_merge_cannot_point_at_itself_a_missing_ref_or_into_a_loop() {
    let (_, layout, blob) = good();
    for (what, bad) in [
        ("itself", blob.replace("p3 m p2 2", "p3 m p3 2")),
        (
            "a ref that was never minted",
            blob.replace("p3 m p2 2", "p3 m p9 2"),
        ),
        (
            "a ref retired in the same version",
            blob.replace("p1 d 1\np3 m p2 2", "p1 d 1\np3 m p1 1"),
        ),
        (
            "a ref retired earlier",
            blob.replace("p1 d 1\np3 m p2 2", "p1 d 1\np3 m p1 2"),
        ),
        (
            "a loop",
            blob.replace("p1 d 1\np3 m p2 2", "p1 m p3 1\np3 m p1 2"),
        ),
    ] {
        assert_eq!(
            code_of(&bad, &layout),
            ErrorCode::CorruptComplement,
            "{what}"
        );
    }
    // A target retired in a later version is fine.
    let ok = blob.replace("p1 d 1\np3 m p2 2", "p1 m p3 1\np3 d 2");
    assert!(restore(&ok, &layout).is_ok(), "{ok}");
}

#[test]
fn the_blob_must_match_the_document_it_describes() {
    let (_, layout, blob) = good();
    // A story that is missing, or another one in its place.
    let mut none = layout.clone();
    none.stories.clear();
    assert_eq!(code_of(&blob, &none), ErrorCode::CorruptComplement);
    let mut renamed = layout.clone();
    renamed.stories[0].part = Arc::from("word/other.xml");
    assert_eq!(code_of(&blob, &renamed), ErrorCode::CorruptComplement);
    let mut kind = layout.clone();
    kind.stories[0].kind = StoryKind::Header;
    assert_eq!(code_of(&blob, &kind), ErrorCode::CorruptComplement);
    // A block whose text changed.
    let mut text = layout.clone();
    text.stories[0].blocks[1].fingerprint ^= 1;
    let e = restore(&blob, &text).unwrap_err();
    assert_eq!(e.code(), ErrorCode::CorruptComplement);
    assert!(
        e.message().contains("its text is not the text it had"),
        "{}",
        e.message()
    );
    // A block removed, or added.
    let mut fewer = layout.clone();
    fewer.stories[0].blocks.pop();
    assert_eq!(code_of(&blob, &fewer), ErrorCode::CorruptComplement);
    let mut more = layout.clone();
    more.stories[0]
        .blocks
        .push(layout.stories[0].blocks[0].clone());
    assert_eq!(code_of(&blob, &more), ErrorCode::CorruptComplement);
    // Two blocks swapped.
    let mut swapped = layout.clone();
    swapped.stories[0].blocks.swap(0, 4);
    assert_eq!(code_of(&blob, &swapped), ErrorCode::CorruptComplement);
    // A paragraph where a table was.
    let mut other_kind = layout.clone();
    other_kind.stories[0].blocks[0].kind = BlockKind::Paragraph;
    let e = restore(&blob, &other_kind).unwrap_err();
    assert_eq!(e.code(), ErrorCode::CorruptComplement);
    assert!(
        e.message().contains("another kind of block"),
        "{}",
        e.message()
    );
}

#[test]
fn two_ref_map_lines_with_equal_fingerprints_swapped_are_left_to_the_digests() {
    // The alignment compares each block with its own fingerprint, so lines of two blocks with equal
    // fingerprints can be swapped. The swapped blob is a coherent table (p2 names the first block
    // and p1 the second), so `restore` accepts it, and it must: its canonical form is itself.
    // `state` records a SHA-256 of the whole blob, and `resume` checks it before anything else, so
    // an accident or a truncated copy is caught there. An agent that rewrites the complement and
    // the digest as well has made itself another working copy, which is its own business.
    let story = testing::story("<w:p/><w:p/>");
    let keys = testing::keys(&story);
    let mut layout = layout_of(&story);
    for b in &mut layout.stories[0].blocks {
        b.fingerprint = 5;
    }
    let t = open(&layout);
    let blob = String::from_utf8(t.persist(&layout).unwrap()).unwrap();
    let swapped = blob.replace(
        "p1 0000000000000005\np2 0000000000000005\n",
        "p2 0000000000000005\np1 0000000000000005\n",
    );
    assert_ne!(swapped, blob);
    let back = restore(&swapped, &layout).unwrap();
    assert_eq!(back.ref_of(&keys[0]), Some(r("p2")));
    assert_eq!(back.ref_of(&keys[1]), Some(r("p1")));
    assert_eq!(back.persist(&layout).unwrap(), swapped.as_bytes());
    assert_ne!(swapped, blob, "and the bytes, and so their digest, differ");
}

#[test]
fn truncation_at_every_length_is_refused() {
    let (_, layout, blob) = good();
    for n in 0..blob.len() {
        let e = RefTable::restore(
            &blob.as_bytes()[..n],
            &layout,
            Some(ORIGINAL),
            &Limits::default(),
        );
        assert!(e.is_err(), "accepted the first {n} bytes");
    }
}

#[test]
fn a_changed_byte_is_refused_or_gives_a_blob_that_writes_back_the_same() {
    let (_, layout, blob) = good();
    let mut accepted = 0;
    for i in 0..blob.len() {
        for replacement in [b'0', b'x', b' ', b'\n', b'p', b'9', 0x7f] {
            let mut bytes = blob.clone().into_bytes();
            if bytes[i] == replacement {
                continue;
            }
            bytes[i] = replacement;
            match RefTable::restore(&bytes, &layout, Some(ORIGINAL), &Limits::default()) {
                Err(_) => {}
                Ok(table) => {
                    accepted += 1;
                    assert_eq!(
                        table.persist(&layout).unwrap(),
                        bytes,
                        "byte {i} to {replacement:#x}"
                    );
                }
            }
        }
    }
    // Almost every change is refused.
    assert!(
        accepted < blob.len() / 4,
        "{accepted} of {}",
        blob.len() * 7
    );
}

#[test]
fn restore_never_allocates_more_than_a_small_multiple_of_the_blob() {
    // The counts say a million refs and the blob has none of them: refused before any table is made.
    let (_, layout, blob) = good();
    let lie = replace_line(&blob, "counts 4 1 2", "counts 1000000 0 0");
    assert_eq!(code_of(&lie, &layout), ErrorCode::LimitExceeded);
    let tiny = format!(
        "stet-refs 1\nnamespace {} opened\nscan 1 1\nversion 1000000\n",
        ns()
    );
    assert_eq!(code_of(&tiny, &layout), ErrorCode::CorruptComplement);
}

#[test]
fn persist_refuses_a_layout_that_contradicts_the_table() {
    let story = testing::story(DOC);
    let layout = layout_of(&story);
    let t = open(&layout);
    // A block with no ref.
    let mut extra = layout.clone();
    extra.stories[0].blocks.push(super::layout::BlockSpec {
        key: crate::wordml::BlockKey::new(
            Arc::from("word/fresh.xml"),
            layout.stories[0].blocks[0].key.node(),
        ),
        kind: BlockKind::Paragraph,
        fingerprint: 0,
    });
    assert_eq!(t.persist(&extra).unwrap_err().code(), ErrorCode::Internal);
    // A live ref no story lists.
    let mut missing: Layout = layout.clone();
    let StoryBlocks { blocks, .. } = &mut missing.stories[0];
    blocks.pop();
    assert_eq!(t.persist(&missing).unwrap_err().code(), ErrorCode::Internal);
    // A block listed with the wrong kind.
    let mut wrong = layout.clone();
    wrong.stories[0].blocks[0].kind = BlockKind::Row;
    assert_eq!(t.persist(&wrong).unwrap_err().code(), ErrorCode::Internal);
}
