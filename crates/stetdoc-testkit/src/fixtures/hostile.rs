//! Hostile fixtures about the ZIP container and the package structure.

use super::deflate;
use super::parts::{Doc, content_types, entries, entries_with, root_rels};
use super::zip::{Entry, Zip, raw_eocd};
use super::{DOCUMENT_XML, Expect, Fixture, hostile_fixture as fx};

const MALFORMED: &str = "malformed_input";
const LIMIT: &str = "limit_exceeded";

pub(super) fn good() -> Vec<u8> {
    Zip::new(entries()).build()
}

pub(super) fn with_extra(extra: Vec<Entry>) -> Vec<u8> {
    let mut es = entries();
    es.extend(extra);
    Zip::new(es).build()
}

pub(super) fn tweak_document(f: impl Fn(Entry) -> Entry) -> Vec<u8> {
    let es = entries()
        .into_iter()
        .enumerate()
        .map(|(i, e)| if i == 2 { f(e) } else { e })
        .collect();
    Zip::new(es).build()
}

pub(super) fn put(bytes: &mut [u8], from_end: usize, value: &[u8]) {
    let at = bytes.len() - from_end;
    bytes[at..at + value.len()].copy_from_slice(value);
}

/// 41 central entries that all point at the last local header, and its 8 MB payload.
fn shared_local_header() -> Vec<u8> {
    let mut es = entries();
    let mut payload = b"<a>".to_vec();
    payload.extend(vec![b'B'; 8 * 1024 * 1024]);
    payload.extend(b"</a>");
    es.push(Entry::new("customXml/bomb.xml", payload).level(1));
    let out = Zip::new(es).build();
    let cd_off = u32::from_le_bytes([
        out[out.len() - 6],
        out[out.len() - 5],
        out[out.len() - 4],
        out[out.len() - 3],
    ]) as usize;
    let cd = &out[cd_off..out.len() - 22];
    let mut records = Vec::new();
    let mut p = 0;
    while p < cd.len() {
        let nl = usize::from(u16::from_le_bytes([cd[p + 28], cd[p + 29]]));
        let el = usize::from(u16::from_le_bytes([cd[p + 30], cd[p + 31]]));
        let cl = usize::from(u16::from_le_bytes([cd[p + 32], cd[p + 33]]));
        let len = 46 + nl + el + cl;
        records.push(cd[p..p + len].to_vec());
        p += len;
    }
    let last = records.last().cloned().unwrap_or_default();
    let mut new_cd: Vec<u8> = records.concat();
    for i in 0..40 {
        let name = format!("customXml/clone{i:02}.xml");
        let mut r = last[..46].to_vec();
        r[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
        r[30..34].copy_from_slice(&[0, 0, 0, 0]);
        r.extend(name.as_bytes());
        new_cd.extend(r);
    }
    let count = (records.len() + 40) as u16;
    let mut bytes = out[..cd_off].to_vec();
    bytes.extend(&new_cd);
    bytes.extend(raw_eocd(count, new_cd.len() as u32, cd_off as u32, 0));
    bytes
}

/// `word/document.xml` that inflates to about 150 MB from about 150 KB, written without ever
/// building the 150 MB: see [`deflate::run_stream`].
fn bomb() -> Vec<u8> {
    let marker = b"@@RUN@@";
    let doc = Doc::new()
        .body("<w:p><w:r><w:t>@@RUN@@</w:t></w:r></w:p>")
        .bytes();
    let at = doc
        .windows(marker.len())
        .position(|w| w == marker)
        .unwrap_or(0);
    let stream = deflate::run_stream(&doc[..at], b'A', 610_000, &doc[at + marker.len()..]);
    let mut es = entries();
    es[2] = Entry::new("word/document.xml", Vec::new())
        .raw(&stream.deflated)
        .declared_usize(stream.inflated_len as u32)
        .crc(stream.crc);
    Zip::new(es).build()
}

/// The ZIP-level and package-level hostile fixtures.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "zip-empty-file",
            "a zero-byte file",
            Expect::open(MALFORMED),
            "zip.not_zip.other",
            Vec::new,
        ),
        fx(
            "zip-eocd-only",
            "a valid ZIP with no entries: only an end record",
            Expect::document(MALFORMED),
            "zip.missing_content_types;opc.missing_root_rels",
            || raw_eocd(0, 0, 0, 0),
        ),
        fx(
            "zip-random-garbage",
            "not a ZIP: 5 KB of a repeating byte ramp",
            Expect::open(MALFORMED),
            "zip.not_zip.other",
            || (0..=255u8).cycle().take(5120).collect(),
        ),
        fx(
            "zip-ole-cfb-header",
            "an OLE compound file header, which is what a password-protected .docx looks like",
            Expect::open("unsupported"),
            "zip.not_zip.ole_cfb",
            || {
                let mut d = vec![0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
                d.extend(vec![0u8; 512]);
                d
            },
        ),
        fx(
            "zip-truncated-in-entry",
            "the archive cut in the middle of the first entry",
            Expect::open(MALFORMED),
            "zip.no_eocd",
            || good()[..300].to_vec(),
        ),
        fx(
            "zip-truncated-before-eocd",
            "the end record removed",
            Expect::open(MALFORMED),
            "zip.no_eocd",
            || {
                let g = good();
                g[..g.len() - 22].to_vec()
            },
        ),
        fx(
            "zip-truncated-in-central-directory",
            "the central directory shortened but the end record intact",
            Expect::open(MALFORMED),
            "zip.cd_truncated",
            || {
                let g = good();
                let mut t = g[..g.len() - 22 - 100].to_vec();
                t.extend(&g[g.len() - 22..]);
                t
            },
        ),
        fx(
            "zip-cd-offset-past-eof",
            "the end record puts the central directory far beyond the end of the file",
            Expect::open(MALFORMED),
            "zip.cd_not_found",
            || {
                let mut g = good();
                put(&mut g, 6, &0x7FFF_FF00u32.to_le_bytes());
                g
            },
        ),
        fx(
            "zip-entry-count-lies",
            "the end record claims 1000 entries, the central directory has 5",
            Expect::open(MALFORMED),
            "zip.cd_entry_count_mismatch",
            || {
                let mut g = good();
                put(&mut g, 14, &1000u16.to_le_bytes());
                put(&mut g, 12, &1000u16.to_le_bytes());
                g
            },
        ),
        fx(
            "zip-comment-length-lies",
            "the end record's comment length is 500 with no bytes after it",
            Expect::open(MALFORMED),
            "zip.eocd_comment_truncated",
            || {
                let mut g = good();
                put(&mut g, 2, &500u16.to_le_bytes());
                g
            },
        ),
        fx(
            "zip-crc-mismatch",
            "the CRC-32 of word/document.xml is wrong",
            Expect::read(MALFORMED),
            "zip.crc_mismatch",
            || tweak_document(|e| e.crc(0xDEAD_BEEF)),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-corrupt-deflate-stream",
            "the compressed data of word/document.xml is not a deflate stream",
            Expect::read(MALFORMED),
            "zip.deflate_error",
            || tweak_document(|e| e.raw(b"\xff\xfe\xfd garbage not deflate")),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-usize-lies-4gb",
            "the central directory says word/document.xml expands to 4 GB",
            Expect::open(LIMIT),
            "zip.entry_larger_than_cap;zip.declared_usize_ge_256mb",
            || tweak_document(|e| e.declared_usize(0xFFFF_FFF0)),
        ),
        fx(
            "zip-encrypted-flag",
            "general-purpose bit 0 (encrypted) set on word/document.xml",
            Expect::read("unsupported"),
            "zip.encrypted_entry",
            || tweak_document(|e| e.flags(1)),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-unsupported-method-99",
            "compression method 99 (AES) on word/document.xml",
            Expect::read("unsupported"),
            "zip.method.other.99",
            || tweak_document(|e| e.method(99).raw(b"xx")),
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-duplicate-entry-names",
            "two entries named word/document.xml, which readers disagree about",
            Expect::open(MALFORMED),
            "zip.duplicate_entry_name",
            || {
                with_extra(vec![Entry::new(
                    "word/document.xml",
                    Doc::new()
                        .body("<w:p><w:r><w:t>EVIL second document.xml</w:t></w:r></w:p>")
                        .bytes(),
                )])
            },
        ),
        fx(
            "zip-duplicate-case-names-valid-parts",
            "two entry names that differ only by case (part names are case-insensitive in OPC)",
            Expect::open(MALFORMED),
            "zip.case_insensitive_duplicate_name",
            || {
                let mut es = entries();
                es.push(Entry::new("customXml/Item.xml", "<a/>"));
                es.push(Entry::new("customXml/item.xml", "<b/>"));
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-path-traversal-dotdot",
            "entry names with .. segments",
            Expect::open(MALFORMED),
            "zip.name.dotdot_segment",
            || {
                with_extra(vec![
                    Entry::new("../../evil.xml", "<a/>"),
                    Entry::new("word/../../evil2.xml", "<a/>"),
                ])
            },
        ),
        fx(
            "zip-path-absolute-and-backslash",
            "absolute, drive-letter and backslash entry names",
            Expect::open(MALFORMED),
            "zip.name.absolute;zip.name.backslash",
            || {
                with_extra(vec![
                    Entry::new("/etc/evil.xml", "<a/>"),
                    Entry::new("C:\\Windows\\evil.xml", "<a/>"),
                    Entry::new("word\\evil3.xml", "<a/>"),
                ])
            },
        ),
        fx(
            "zip-path-normalisation-tricks",
            "names that normalise to the main part",
            Expect::open(MALFORMED),
            "zip.name.empty_segment;zip.name.dot_segment;zip.case_insensitive_duplicate_name",
            || {
                with_extra(vec![
                    Entry::new("word//document.xml", "<a/>"),
                    Entry::new("word/./document.xml", "<a/>"),
                    Entry::new("WORD/DOCUMENT.XML", "<a/>"),
                ])
            },
        ),
        fx(
            "zip-name-with-nul",
            "an entry name containing a NUL byte",
            Expect::open(MALFORMED),
            "zip.name.nul",
            || with_extra(vec![Entry::new(b"word/ev\x00il.xml", "<a/>")]),
        ),
        fx(
            "zip-local-name-differs-from-central",
            "the local header name differs from the central directory name",
            Expect::open(MALFORMED),
            "zip.lh_cd_disagree.name",
            || tweak_document(|e| e.local_name(b"word/other.xml")),
        ),
        fx(
            "zip-local-method-differs",
            "the first local header says stored, the central directory says deflated",
            Expect::open(MALFORMED),
            "zip.lh_cd_disagree.method",
            || {
                let mut g = good();
                let pattern = b"PK\x03\x04\x14\x00\x00\x00\x08\x00";
                if let Some(at) = g.windows(pattern.len()).position(|w| w == pattern) {
                    g[at + 8] = 0;
                }
                g
            },
        ),
        fx(
            "zip-bomb-ratio-1000",
            "word/document.xml expands to about 150 MB from about 150 KB",
            Expect::read(LIMIT),
            "zip.high_compression_ratio",
            bomb,
        )
        .in_parts(DOCUMENT_XML),
        fx(
            "zip-many-entries-20000",
            "20000 empty entries in the central directory",
            Expect::open(LIMIT),
            "zip.entries_ge_10000",
            || {
                let extra = (0..20_000)
                    .map(|i| Entry::new(format!("customXml/e{i:05}.xml"), "").stored())
                    .collect();
                with_extra(extra)
            },
        ),
        fx(
            "zip-overlapping-entries-shared-data",
            "41 central entries share one local header and one 8 MB payload",
            Expect::open(MALFORMED),
            "zip.overlapping_or_nonmonotonic_entries",
            shared_local_header,
        ),
        fx(
            "opc-missing-content-types",
            "[Content_Types].xml is missing",
            Expect::document(MALFORMED),
            "zip.missing_content_types",
            || Zip::new(entries().split_off(1)).build(),
        ),
        fx(
            "opc-missing-root-rels",
            "_rels/.rels is missing",
            Expect::document(MALFORMED),
            "opc.missing_root_rels",
            || {
                Zip::new(
                    entries()
                        .into_iter()
                        .filter(|e| !is_named(e, "_rels/.rels"))
                        .collect(),
                )
                .build()
            },
        ),
        fx(
            "opc-main-part-missing",
            "the root relationship points at a part that is not in the archive",
            Expect::document(MALFORMED),
            "opc.main_part_missing",
            || {
                Zip::new(
                    entries()
                        .into_iter()
                        .enumerate()
                        .filter(|(i, _)| *i != 2)
                        .map(|(_, e)| e)
                        .collect(),
                )
                .build()
            },
        ),
        fx(
            "opc-rel-target-traversal",
            "a relationship target that walks above the package root",
            Expect::Ok,
            "opc.rel_target_escapes_package",
            || {
                let rels = root_rels().replace(
                    "Target=\"docProps/core.xml\"",
                    "Target=\"../../../etc/passwd\"",
                );
                Zip::new(entries_with(&Doc::new().bytes(), &rels, &content_types())).build()
            },
        ),
        fx(
            "opc-content-types-malformed",
            "[Content_Types].xml is truncated",
            Expect::xml(MALFORMED),
            "xml.not_wellformed.core_part",
            || {
                Zip::new(entries_with(
                    &Doc::new().bytes(),
                    &root_rels(),
                    "<Types><Default Extension=\"xml\"",
                ))
                .build()
            },
        )
        .in_parts(&["[Content_Types].xml"]),
        fx(
            "opc-zip-in-zip-embedded",
            "a complete docx stored as an embedded part: an opaque part, never recursed into",
            Expect::Ok,
            "part.nested_zip_inside_part",
            || with_extra(vec![Entry::new("word/embeddings/inner.docx", good())]),
        ),
    ]
}

fn is_named(e: &Entry, name: &str) -> bool {
    e.name_bytes() == name.as_bytes()
}
