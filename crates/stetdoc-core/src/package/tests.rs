use super::testzip::{Z, docx_entries};
use super::*;
use crate::api::error::ErrorCode;

fn open(bytes: Vec<u8>) -> Package {
    Package::open(bytes, &Limits::default()).expect("open")
}

/// L1 on a built archive: both write paths reproduce the input, and every part reads back.
fn assert_identity(z: &Z) -> Package {
    let bytes = z.build();
    let p = open(bytes.clone());
    assert_eq!(p.write().unwrap(), bytes, "shortcut");
    assert_eq!(p.write_rebuilt().unwrap(), bytes, "rebuild");
    for e in &z.entries {
        let name = String::from_utf8_lossy(&e.name).into_owned();
        if !name.ends_with('/') {
            assert_eq!(
                p.part(&name).unwrap().unwrap().as_ref(),
                e.data.as_slice(),
                "{name}"
            );
        }
    }
    p
}

/// Replacing one part leaves every other region and central record alone.
fn assert_locality(z: &Z, victim: &str) {
    let bytes = z.build();
    let p = open(bytes);
    let mut q = p.clone();
    let new = b"<replaced/>".to_vec();
    q.replace_part(victim, new.clone()).unwrap();
    let out = q.write().unwrap();
    let r = open(out);
    assert_eq!(r.part(victim).unwrap().unwrap().as_ref(), new.as_slice());
    for name in p.part_names() {
        if name == victim || name.ends_with('/') {
            continue;
        }
        assert_eq!(p.raw_region(name), r.raw_region(name), "region of {name}");
        assert_eq!(
            p.part(name).unwrap(),
            r.part(name).unwrap(),
            "content of {name}"
        );
        let (a, b) = (
            p.raw_central_record(name).unwrap(),
            r.raw_central_record(name).unwrap(),
        );
        assert_eq!(a.len(), b.len());
        let mask = |x: &[u8]| {
            let mut v = x.to_vec();
            if let Some(s) = v.get_mut(42..46) {
                s.fill(0);
            }
            v
        };
        if a.get(42..46) != Some(&[255; 4]) {
            assert_eq!(mask(a), mask(b), "central record of {name}");
        }
    }
}

#[test]
fn plain_archive_round_trips() {
    let z = Z::new(docx_entries());
    let p = assert_identity(&z);
    assert!(!p.is_modified());
    assert_eq!(p.main_part(), Some("word/document.xml"));
    assert_locality(&z, "word/document.xml");
    assert_locality(&z, "word/media/blob.bin");
    assert_locality(&z, "[Content_Types].xml");
}

#[test]
fn data_descriptors_in_every_shape() {
    for (sig, wide) in [(true, false), (false, false), (true, true), (false, true)] {
        let mut es = docx_entries();
        for e in &mut es {
            e.descriptor = Some((sig, wide));
        }
        let z = Z::new(es);
        let p = assert_identity(&z);
        assert!(
            p.describe().iter().any(|f| f.starts_with("dd-")),
            "{sig} {wide}"
        );
        assert_locality(&z, "word/document.xml");
        // The rewritten entry keeps the descriptor shape.
        let mut q = p.clone();
        q.replace_part("word/document.xml", b"<a/>".to_vec())
            .unwrap();
        let r = open(q.write().unwrap());
        let want = if sig {
            "dd-with-signature"
        } else {
            "dd-without-signature"
        };
        assert!(r.describe().contains(&want.to_owned()), "{sig} {wide}");
    }
}

#[test]
fn comment_prefix_and_order() {
    let mut es = docx_entries();
    es[0].comment = b"entry comment".to_vec();
    es[2].lfh_extra = vec![0x20, 0xA2, 0x04, 0x00, 1, 2, 3, 4];
    es[2].cd_extra = vec![0x55, 0x54, 0x05, 0x00, 1, 9, 9, 9, 9];
    es[2].lfh_time_date = Some((1, 2));
    es[1].flags = 0x0802;
    let mut z = Z::new(es);
    z.prefix = b"MZ stub bytes".to_vec();
    z.comment = b"archive comment".to_vec();
    z.cd_order = Some(vec![2, 0, 3, 1]);
    let p = assert_identity(&z);
    let d = p.describe();
    assert!(d.contains(&"prefix-bytes".to_owned()), "{d:?}");
    assert!(d.contains(&"archive-comment".to_owned()));
    assert!(d.contains(&"cd-order-differs".to_owned()));
    assert_locality(&z, "word/document.xml");
    assert_locality(&z, "_rels/.rels");
}

#[test]
fn relative_offsets_with_a_prefix() {
    let mut z = Z::new(docx_entries());
    z.prefix = vec![0x5A; 100];
    z.relative_offsets = true;
    let p = assert_identity(&z);
    assert!(p.describe().contains(&"offset-base-shift".to_owned()));
    assert_locality(&z, "word/document.xml");
}

#[test]
fn zip64_entries_and_end_records() {
    let mut es = docx_entries();
    for e in &mut es {
        e.zip64 = true;
    }
    let mut z = Z::new(es);
    z.zip64_end = true;
    let p = assert_identity(&z);
    let d = p.describe();
    assert!(
        d.contains(&"zip64-end-records".to_owned()) && d.contains(&"entry-zip64".to_owned()),
        "{d:?}"
    );
    assert_locality(&z, "word/document.xml");
    assert_locality(&z, "[Content_Types].xml");

    // Entry-level ZIP64 alone, and with descriptors.
    let mut es = docx_entries();
    for e in &mut es {
        e.zip64 = true;
        e.descriptor = Some((true, true));
    }
    let z = Z::new(es);
    assert_identity(&z);
    assert_locality(&z, "word/document.xml");
}

#[test]
fn stored_and_deflated_entries_keep_their_method() {
    let z = Z::new(docx_entries());
    let mut p = open(z.build());
    p.replace_part("word/media/blob.bin", vec![7; 100]).unwrap();
    let r = open(p.write().unwrap());
    assert!(r.describe().contains(&"method-0".to_owned()));
    assert_eq!(
        r.part("word/media/blob.bin").unwrap().unwrap().as_ref(),
        vec![7u8; 100].as_slice()
    );
}

#[test]
fn add_part_registers_content_type_and_relationship() {
    let z = Z::new(docx_entries());
    let mut p = open(z.build());
    let id = p
        .add_part(NewPart {
            name: "word/comments.xml",
            content_type:
                "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
            bytes: b"<w:comments/>".to_vec(),
            related_from: Some(("word/document.xml", "http://example.test/comments")),
        })
        .unwrap();
    assert_eq!(id.as_deref(), Some("rId1"));
    let out = p.write().unwrap();
    let r = open(out.clone());
    assert_eq!(
        r.part("word/comments.xml").unwrap().unwrap().as_ref(),
        b"<w:comments/>"
    );
    assert_eq!(
        r.content_type("word/comments.xml").unwrap().as_deref(),
        Some("application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml")
    );
    let rels = r.relationships("word/document.xml").unwrap();
    assert_eq!(rels.items.len(), 1);
    assert_eq!(rels.items[0].target, "comments.xml");
    // Deterministic: the same operations give the same bytes.
    let mut p2 = open(z.build());
    p2.add_part(NewPart {
        name: "word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        bytes: b"<w:comments/>".to_vec(),
        related_from: Some(("word/document.xml", "http://example.test/comments")),
    })
    .unwrap();
    assert_eq!(p2.write().unwrap(), out);
    // The new entry's timestamp is copied from the main document part (central record bytes 12..16).
    let main = r.raw_central_record("word/document.xml").unwrap();
    let added = r.raw_central_record("word/comments.xml").unwrap();
    assert_eq!(main[12..16], added[12..16]);
    // Existing entries are untouched, except content types and the new relationships part.
    for name in ["word/media/blob.bin", "_rels/.rels", "word/document.xml"] {
        assert_eq!(
            open(z.build()).raw_region(name),
            r.raw_region(name),
            "{name}"
        );
    }
}

#[test]
fn add_part_rejects_bad_names_and_duplicates() {
    let z = Z::new(docx_entries());
    let mut p = open(z.build());
    let mk = |name: &'static str| NewPart {
        name,
        content_type: "text/plain",
        bytes: vec![],
        related_from: None,
    };
    for bad in ["../x", "/x", "a\\b", "word/"] {
        assert_eq!(
            p.add_part(mk(bad)).unwrap_err().code(),
            ErrorCode::InvalidPayload,
            "{bad}"
        );
    }
    assert_eq!(
        p.add_part(mk("word/document.xml")).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    assert_eq!(
        p.add_part(mk("WORD/Document.xml")).unwrap_err().code(),
        ErrorCode::InvalidPayload
    );
    assert_eq!(
        p.replace_part("nope.xml", vec![]).unwrap_err().code(),
        ErrorCode::NotFound
    );
}

#[test]
fn a_zip64_look_alike_in_a_central_record_comment_is_refused() {
    // A real file (the LibreOffice test file `zip64-eocd.docx`) hides a second directory, a ZIP64
    // end record and a ZIP64 locator in the comment of its last central record, so that the locator
    // sits right before the end record. Python's `zipfile` and `bsdtar` take their directory from
    // that record, and `unzip` takes it from the end record, so the readers see two different
    // archives. A locator that does not lead to a record that agrees with the end record is
    // refused.
    let mut fake = Vec::new();
    fake.extend(0x0606_4b50u32.to_le_bytes());
    fake.extend(44u64.to_le_bytes());
    fake.extend([0u8; 4]);
    fake.extend([0u8; 8]);
    fake.extend(4u64.to_le_bytes());
    fake.extend(4u64.to_le_bytes());
    fake.extend(976u64.to_le_bytes());
    fake.extend(11177u64.to_le_bytes());
    fake.extend(0x0706_4b50u32.to_le_bytes());
    fake.extend(0u32.to_le_bytes());
    fake.extend(0u64.to_le_bytes());
    fake.extend(1u32.to_le_bytes());
    let mut es = docx_entries();
    if let Some(last) = es.last_mut() {
        last.comment = fake;
    }
    let bytes = Z::new(es).build();
    let e = Package::open(bytes, &Limits::default())
        .err()
        .expect("refused");
    assert_eq!(e.code(), ErrorCode::MalformedInput);
    assert!(e.message().contains("disagrees"), "{}", e.message());
}
