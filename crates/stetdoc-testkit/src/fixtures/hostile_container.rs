//! More hostile fixtures about the ZIP container: names, offsets and sizes that lie, methods
//! the engine does not read, and entries that inflate far beyond what they cost on disk.

use super::hostile::{good, put, tweak_document, with_extra};
use super::parts::{Doc, entries, zeros_part};
use super::zip::{Entry, Zip, Zip64};
use super::{DOCUMENT_XML, Expect, Fixture, hostile_fixture as fx};

const MALFORMED: &str = "malformed_input";
const LIMIT: &str = "limit_exceeded";
const UNSUPPORTED: &str = "unsupported";

/// An archive whose comment holds a second end record that is consistent with the real one.
fn comment_with_fake_end_record() -> Vec<u8> {
    let probe = Zip::new(entries()).build();
    let at = probe.len() - 22;
    let real = &probe[at..];
    let tail = b"tail!";
    let mut comment = b"note: ".to_vec();
    comment.extend(b"PK\x05\x06");
    comment.extend([0u8; 4]);
    comment.extend(&real[10..12]);
    comment.extend(&real[10..12]);
    comment.extend(&real[12..16]);
    comment.extend(&real[16..20]);
    comment.extend((tail.len() as u16).to_le_bytes());
    comment.extend(tail);
    Zip::new(entries()).comment(&comment).build()
}

/// The compressed size of an entry, as the writer records it in the local header.
fn deflated_len(e: Entry) -> u32 {
    let z = Zip::new(vec![e]).build();
    u32::from_le_bytes([z[18], z[19], z[20], z[21]])
}

/// The first entry declares a compressed size that runs through the whole second entry.
fn entry_runs_into_next() -> Vec<u8> {
    let es = entries();
    let sizes: Vec<u32> = es.iter().cloned().map(deflated_len).collect();
    let second = 30 + es[1].name_bytes().len() as u32 + sizes[1];
    let es = es
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            if i == 0 {
                e.declared_csize(sizes[0] + second)
            } else {
                e
            }
        })
        .collect();
    Zip::new(es).build()
}

fn document_with_size(delta: i64) -> Vec<u8> {
    let real = Doc::new().bytes().len() as i64;
    tweak_document(|e| e.declared_usize((real + delta) as u32))
}

/// A stored `word/document.xml` that declares 16 bytes more than it holds, with 16 bytes of
/// padding after it so that the entry still fits in the file.
fn stored_size_too_big() -> Vec<u8> {
    let doc = Doc::new().bytes();
    let declared = doc.len() as u32 + 16;
    let es = entries()
        .into_iter()
        .enumerate()
        .map(|(i, e)| {
            if i == 2 {
                Entry::new("word/document.xml", &doc)
                    .stored()
                    .declared_csize(declared)
                    .declared_usize(declared)
            } else {
                e
            }
        })
        .collect();
    Zip::new(es).gap_before(3, b"PADDING!PADDING!").build()
}

fn with_zeros(name: &str, total: u32) -> Vec<u8> {
    with_extra(vec![zeros_part(name, total)])
}

fn many_empty_entries_with_zip64_count() -> Vec<u8> {
    let mut es = entries();
    es.extend((0..70_000).map(|i| Entry::new(format!("word/extra/{i:05}"), "").stored()));
    Zip::new(es).zip64(Zip64::EndRecordsOnly).build()
}

/// The container-level hostile fixtures that the first set did not cover.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "zip-gap-between-entries",
            "17 unreferenced bytes between two entries, where a reader that goes through the file in order can read an entry that the central directory does not list",
            Expect::open(MALFORMED),
            "zip.bytes_between_entries",
            || Zip::new(entries()).gap_before(2, &[0u8; 17]).build(),
        ),
        fx(
            "zip-gap-before-cd",
            "unreferenced bytes between the last entry and the central directory",
            Expect::open(MALFORMED),
            "zip.bytes_between_entries",
            || {
                Zip::new(entries())
                    .gap_before_central_directory(b"GAPGAPGAP")
                    .build()
            },
        ),
        fx(
            "zip-comment-fake-end-record",
            "the archive comment holds PK 05 06 and a self-consistent second end record with the real field values, so the last end record signature leads nowhere: Python and bsdtar refuse the file, and unzip reports an error and carries on",
            Expect::open(MALFORMED),
            "zip.cd_not_found",
            comment_with_fake_end_record,
        ),
        fx(
            "zip-ten-byte-file",
            "a file of ten bytes, shorter than an end record",
            Expect::open(MALFORMED),
            "zip.not_zip.other",
            || b"0123456789".to_vec(),
        ),
        fx(
            "zip-path-control-char",
            "an entry name containing the control character U+0001",
            Expect::open(MALFORMED),
            "zip.name.control_char",
            || with_extra(vec![Entry::new(b"word/evil\x01.xml", "<a/>")]),
        ),
        fx(
            "zip-path-dot-prefix",
            "an entry named ./word/document.xml next to word/document.xml",
            Expect::open(MALFORMED),
            "zip.name.dot_segment",
            || with_extra(vec![Entry::new("./word/document.xml", "<a/>")]),
        ),
        fx(
            "zip-path-name-65535-bytes",
            "an entry name of 65535 bytes, the longest a ZIP name field holds",
            Expect::open(MALFORMED),
            "zip.name.longer_than_255",
            || {
                let name = format!("word/{}", "a".repeat(65_530));
                with_extra(vec![Entry::new(name, "<a/>")])
            },
        ),
        fx(
            "zip-overlap-two-records-share-header",
            "the last central record points at the first local header, so two records share one local header whose name does not match",
            Expect::open(MALFORMED),
            "zip.overlapping_or_nonmonotonic_entries;zip.lh_cd_disagree.name",
            || with_extra(vec![Entry::new("word/dup.xml", "<a/>").central_offset(0)]),
        ),
        fx(
            "zip-overlap-entry-runs-into-next",
            "the first entry declares a compressed size that runs through the next entry, the small form of the overlapping bomb",
            Expect::open(MALFORMED),
            "zip.overlapping_or_nonmonotonic_entries",
            entry_runs_into_next,
        ),
        fx(
            "zip-entry-offset-past-eof",
            "the central record of word/document.xml has an offset beyond the end of the file",
            Expect::open(MALFORMED),
            "zip.local_header_missing",
            || {
                let len = good().len() as u32;
                tweak_document(|e| e.central_offset(len + 1000))
            },
        ),
        fx(
            "zip-cd-size-past-file",
            "the end record gives a central directory size larger than the whole file",
            Expect::open(MALFORMED),
            "zip.cd_truncated",
            || {
                let mut g = good();
                let size = g.len() as u32 + 5000;
                put(&mut g, 10, &size.to_le_bytes());
                g
            },
        ),
        fx(
            "zip-csize-past-file",
            "the compressed size of word/document.xml is larger than the file",
            Expect::open(MALFORMED),
            "zip.csize_beyond_file",
            || tweak_document(|e| e.declared_csize(1_000_000)),
        ),
        fx(
            "zip-multi-disk",
            "the end record says the central directory is on disk 1",
            Expect::open(MALFORMED),
            "zip.multi_disk",
            || {
                let mut g = good();
                put(&mut g, 18, &1u16.to_le_bytes());
                put(&mut g, 16, &1u16.to_le_bytes());
                g
            },
        ),
        fx(
            "zip-entries-70000-zip64-count",
            "70000 empty entries, counted by a ZIP64 end record because the 16-bit count cannot hold them",
            Expect::open(LIMIT),
            "zip.entries_ge_10000",
            many_empty_entries_with_zip64_count,
        ),
        fx(
            "zip-bomb-1000mib-declared",
            "word/media/bomb.bin holds 1000 MiB of zeros that deflate to about 1 MB, and says so: past the cap on one entry",
            Expect::open(LIMIT),
            "zip.declared_usize_ge_256mb;zip.entry_larger_than_cap",
            || with_zeros("word/media/bomb.bin", 1000 << 20),
        ),
        fx(
            "zip-usize-smaller-than-real",
            "the declared size of word/document.xml is 5 bytes less than the real size",
            Expect::read(MALFORMED),
            "zip.usize_mismatch",
            || document_with_size(-5),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-usize-larger-than-real",
            "the declared size of word/document.xml is 5 bytes more than the real size",
            Expect::read(MALFORMED),
            "zip.usize_mismatch",
            || document_with_size(5),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-stored-size-too-big",
            "a stored word/document.xml declares 16 bytes more than it holds, and 16 bytes of padding follow, so the entry fits but its CRC does not",
            Expect::read(MALFORMED),
            "zip.stored_size_mismatch",
            stored_size_too_big,
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-unsupported-method-12-bzip2",
            "compression method 12 (bzip2) on word/document.xml",
            Expect::read(UNSUPPORTED),
            "zip.method.other.12",
            || tweak_document(|e| e.method(12)),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-unsupported-method-14-lzma",
            "compression method 14 (LZMA) on word/document.xml",
            Expect::read(UNSUPPORTED),
            "zip.method.other.14",
            || tweak_document(|e| e.method(14)),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-ratio-20mib-in-media-part",
            "word/media/bomb.bin holds 20 MiB of zeros that deflate to about 20 KB: opens, and refuses a layer that reads that part, because a part of 16 MiB or more is held to the ratio limit when read",
            Expect::read(LIMIT),
            "zip.high_compression_ratio",
            || with_zeros("word/media/bomb.bin", 20 << 20),
        )
        .in_parts(&["word/media/bomb.bin"]),
    ]
}
