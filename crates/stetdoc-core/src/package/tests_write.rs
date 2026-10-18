//! Tests for what `write` and `add_part` produce, checked with a second reader (`reference_reader`),
//! and for the names a package answers to.

use super::reference_reader as reference;
use super::testzip::{E, Z, docx_entries};
use super::*;
use crate::api::error::ErrorCode;

fn open(bytes: Vec<u8>) -> Package {
    Package::open(bytes, &Limits::default()).expect("open")
}

/// What the reference reader finds, as (name, data) pairs. It refuses what Python's `zipfile`
/// refuses.
fn read_all(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    reference::read(bytes)
        .unwrap_or_else(|e| panic!("the reference reader refuses this archive: {e}"))
        .into_iter()
        .map(|f| (f.name, f.data))
        .collect()
}

fn comments(related: bool) -> NewPart<'static> {
    NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: b"<w:comments/>".to_vec(),
        related_from: related.then_some(("word/document.xml", "http://example.test/comments")),
    }
}

fn eocd_count(bytes: &[u8]) -> u16 {
    let at = bytes.len() - 22;
    u16::from_le_bytes([bytes[at + 10], bytes[at + 11]])
}

#[test]
fn the_zip64_record_is_patched_whether_or_not_the_end_record_uses_sentinels() {
    for (real, entry_zip64) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut es = docx_entries();
        for e in &mut es {
            e.zip64 = entry_zip64;
        }
        let mut z = Z::new(es);
        z.zip64_end = true;
        z.zip64_real_values = real;
        let original = z.build();
        let case = format!("real values {real}, zip64 entries {entry_zip64}");
        assert_eq!(read_all(&original).len(), 4, "{case}");
        let p = open(original.clone());
        assert_eq!(p.write_rebuilt().unwrap(), original, "{case}");

        // One replacement: the directory moves, and the record has to move with it.
        let mut p = open(original.clone());
        p.replace_part("word/document.xml", b"<w:document/>".to_vec())
            .unwrap();
        let out = p.write().unwrap();
        let got = read_all(&out);
        assert_eq!(got.len(), 4, "{case}");
        for e in &z.entries {
            let name = String::from_utf8(e.name.clone()).unwrap();
            let want: &[u8] = if name == "word/document.xml" {
                b"<w:document/>"
            } else {
                &e.data
            };
            assert!(
                got.contains(&(name.clone(), want.to_vec())),
                "{case}: {name}"
            );
        }
        assert!(
            open(out)
                .describe()
                .contains(&"zip64-end-records".to_owned()),
            "{case}"
        );

        // Adding parts changes the count: the record and a real end record both follow.
        let mut p = open(original.clone());
        p.add_part(comments(true)).unwrap();
        let out = p.write().unwrap();
        assert_eq!(read_all(&out).len(), 6, "{case}");
        if real {
            assert_eq!(eocd_count(&out), 6, "{case}");
        }
        assert_eq!(open(out).part_names().count(), 6, "{case}");
    }
}

#[test]
fn encrypted_entries_are_unsupported_and_cannot_be_replaced() {
    // Bit 0 is encryption, bit 6 strong encryption, bit 13 a masked local header. Either the
    // central record or the local header may carry it.
    for bit in [0x0001u16, 0x0040, 0x2000] {
        for in_central in [true, false] {
            let original = Z::new(docx_entries()).build();
            let probe = open(original.clone());
            let e = &probe.layout.entries[2];
            let at = if in_central {
                e.cd.start + 8
            } else {
                e.lfh + 6
            };
            let mut bytes = original.clone();
            let flags = u16::from_le_bytes([bytes[at], bytes[at + 1]]) | bit;
            bytes[at..at + 2].copy_from_slice(&flags.to_le_bytes());
            let case = format!("bit {bit:#x}, central {in_central}");

            let flagged = open(bytes.clone());
            let mut p = open(bytes.clone());
            assert_eq!(
                p.part("word/document.xml").unwrap_err().code(),
                ErrorCode::Unsupported,
                "{case}"
            );
            let e = p
                .replace_part("word/document.xml", b"<a/>".to_vec())
                .unwrap_err();
            assert_eq!(e.code(), ErrorCode::Unsupported, "{case}");
            assert!(e.message().contains("encrypted"), "{e}");
            assert!(!p.is_modified(), "{case}");
            // The rest of the package is untouched and the entry is copied as it was.
            assert_eq!(p.write().unwrap(), bytes, "{case}");
            p.replace_part("_rels/.rels", b"<Relationships/>".to_vec())
                .unwrap();
            let out = p.write().unwrap();
            assert_eq!(
                open(out).raw_region("word/document.xml"),
                flagged.raw_region("word/document.xml"),
                "{case}"
            );
        }
    }
}

#[test]
fn a_short_zip64_field_in_a_local_header_is_left_alone() {
    // A ZIP64 field with room for one 8-byte value, and another extra field after it. Writing both
    // sizes into the short field would run over the field after it.
    let mut extra = vec![1, 0, 8, 0];
    extra.extend([0u8; 8]);
    extra.extend([0x55, 0x54, 5, 0, 1, 2, 3, 4, 5]);
    let mut es = docx_entries();
    es[2].lfh_extra = extra.clone();
    let original = Z::new(es).build();
    let mut p = open(original.clone());
    assert_eq!(p.write_rebuilt().unwrap(), original);
    p.replace_part("word/document.xml", b"<a/>".to_vec())
        .unwrap();
    let out = p.write().unwrap();
    let region = open(out.clone())
        .raw_region("word/document.xml")
        .unwrap()
        .to_vec();
    let name_len = "word/document.xml".len();
    assert_eq!(
        &region[30 + name_len..30 + name_len + extra.len()],
        extra.as_slice()
    );
    assert!(read_all(&out).contains(&("word/document.xml".to_owned(), b"<a/>".to_vec())));
}

#[test]
fn add_part_changes_nothing_when_a_step_fails() {
    // A relationships part that is not well-formed: the part and its content type were already
    // added by the time this was found, and must not stay.
    let mut es = docx_entries();
    es.push(E::new(
        "word/_rels/document.xml.rels",
        b"<Relationships xmlns=\"x\"><Relationship",
    ));
    let original = Z::new(es).build();
    let mut p = open(original.clone());
    let e = p.add_part(comments(true)).unwrap_err();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().contains("word/_rels/document.xml.rels"), "{e}");
    assert!(!p.is_modified());
    assert!(!p.contains("word/comments.xml"));
    assert_eq!(p.write().unwrap(), original);
    assert_eq!(
        p.content_type("word/comments.xml").unwrap().as_deref(),
        Some("application/xml"),
        "no override was registered"
    );
    // Without a relationship the same part is fine, and the package is as the failed attempt found it.
    p.add_part(comments(false)).unwrap();
    assert!(p.contains("word/comments.xml"));

    // A content type that disagrees with an override already registered for that name.
    let mut es = docx_entries();
    es[0] = E::new(
        "[Content_Types].xml",
        b"<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/pre.xml\" ContentType=\"text/a\"/></Types>",
    );
    let original = Z::new(es).build();
    let mut p = open(original.clone());
    let e = p
        .add_part(NewPart {
            name: "word/pre.xml",
            content_type: "text/b",
            bytes: vec![],
            related_from: Some(("word/document.xml", "http://example.test/pre")),
        })
        .unwrap_err();
    assert_eq!(e.code(), ErrorCode::InvalidPayload, "{e}");
    assert!(!p.is_modified());
    assert_eq!(p.write().unwrap(), original);
}

#[test]
fn an_empty_entry_with_a_wide_descriptor_is_not_read_as_a_narrow_one() {
    // No data means the first 16 bytes of the wide descriptor also read as a narrow one. Without
    // a ZIP64 field in the local header, nothing but what follows says which it is.
    let with = |content: &[u8]| {
        let mut es = docx_entries();
        let mut e = E::new("word/empty.xml", content);
        e.descriptor = Some((true, true));
        es.insert(3, e);
        Z::new(es).build()
    };
    let original = with(b"");
    let mut p = open(original.clone());
    assert_eq!(p.write_rebuilt().unwrap(), original);
    p.replace_part("word/empty.xml", b"<a/>".to_vec()).unwrap();
    // The 8 bytes that would be left over from a wide descriptor read as narrow are not there.
    assert_eq!(p.write().unwrap(), with(b"<a/>"));
}

#[test]
fn an_end_record_with_all_ones_in_the_this_disk_count_is_written_back_unchanged() {
    let mut bytes = Z::new(docx_entries()).build();
    let eocd = bytes.len() - 22;
    bytes[eocd + 8] = 0xFF;
    bytes[eocd + 9] = 0xFF;
    let p = open(bytes.clone());
    assert_eq!(p.write_rebuilt().unwrap(), bytes);
}

#[test]
fn directory_entries_are_not_parts() {
    let mut es = docx_entries();
    let mut dir = E::new("word/media/", b"");
    dir.method = 0;
    es.insert(3, dir);
    let bytes = Z::new(es).build();
    let mut p = open(bytes.clone());
    assert_eq!(p.write().unwrap(), bytes);
    assert_eq!(p.write_rebuilt().unwrap(), bytes);
    let names: Vec<&str> = p.part_names().collect();
    assert_eq!(
        names,
        [
            "[Content_Types].xml",
            "_rels/.rels",
            "word/document.xml",
            "word/media/blob.bin"
        ]
    );
    assert!(!p.contains("word/media/"));
    assert!(p.part("word/media/").unwrap().is_none());
    assert_eq!(
        p.replace_part("word/media/", vec![1]).unwrap_err().code(),
        ErrorCode::NotFound
    );
    // A part cannot take the name of a directory, with or without the slash, in any case.
    for name in ["word/media", "WORD/Media"] {
        let e = p
            .add_part(NewPart {
                name,
                content_type: "text/plain",
                bytes: vec![],
                related_from: None,
            })
            .unwrap_err();
        assert_eq!(e.code(), ErrorCode::InvalidPayload, "{name}");
    }
    assert!(!p.is_modified());
}

#[test]
fn part_names_are_compared_ignoring_ascii_case() {
    let bytes = Z::new(docx_entries()).build();
    let mut p = open(bytes);
    let exact = p.part("word/document.xml").unwrap().unwrap();
    assert_eq!(p.part("WORD/Document.XML").unwrap().unwrap(), exact);
    assert!(p.contains("[CONTENT_TYPES].XML"));
    assert!(!p.contains("word/documents.xml"));
    // A replacement by a differently cased name replaces that part and keeps the stored name.
    p.replace_part("WORD/DOCUMENT.XML", b"<a/>".to_vec())
        .unwrap();
    let out = p.write().unwrap();
    assert!(read_all(&out).contains(&("word/document.xml".to_owned(), b"<a/>".to_vec())));
    assert_eq!(open(out).main_part(), Some("word/document.xml"));
}

#[test]
fn a_relationship_target_for_a_new_part_is_a_uri_reference() {
    let mut p = open(Z::new(docx_entries()).build());
    let name = "word/media/my image \u{e9}#1.png";
    p.add_part(NewPart {
        name,
        content_type: "image/png",
        bytes: vec![1, 2, 3],
        related_from: Some(("word/document.xml", "http://example.test/image")),
    })
    .unwrap();
    let r = open(p.write().unwrap());
    let rels = r.relationships("word/document.xml").unwrap();
    assert_eq!(rels.items[0].target, "media/my%20image%20%C3%A9%231.png");
    assert_eq!(
        opc::resolve_target("word/document.xml", &rels.items[0].target).as_deref(),
        Some(name)
    );
    assert!(r.contains(name));
}
