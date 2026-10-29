//! Tests of the fixture builder: every fixture builds, is what it says it is, and the
//! containers that should be valid read back byte-exact with an independent reader.

use super::inspect::{self, extra_ids};
use super::*;
use std::collections::BTreeSet;

pub(super) fn find(name: &str) -> Fixture {
    all()
        .into_iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
}

pub(super) fn names(bytes: &[u8]) -> Vec<String> {
    inspect::read(bytes)
        .unwrap()
        .records
        .iter()
        .map(|r| String::from_utf8_lossy(&r.name).into_owned())
        .collect()
}

#[test]
fn there_are_85_edge_7_word_and_87_hostile_fixtures_with_unique_names() {
    assert_eq!(edge().len(), 85);
    assert_eq!(word().len(), 7);
    assert_eq!(hostile().len(), 87);
    let unique: BTreeSet<&str> = all().iter().map(|f| f.name).collect();
    assert_eq!(unique.len(), 179);
    assert!(edge().iter().all(|f| f.kind == Kind::Edge));
    assert!(word().iter().all(|f| f.kind == Kind::Word));
    assert!(hostile().iter().all(|f| f.kind == Kind::Hostile));
}

#[test]
fn edge_fixtures_read_back_with_an_independent_reader() {
    for f in edge() {
        let bytes = f.bytes();
        let archive = inspect::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.name));
        assert!(!archive.records.is_empty(), "{}", f.name);
        for rec in &archive.records {
            if rec.name.ends_with(b"/") {
                continue;
            }
            inspect::extract(&bytes, rec).unwrap_or_else(|e| {
                panic!("{}: {}: {e}", f.name, String::from_utf8_lossy(&rec.name))
            });
        }
    }
}

#[test]
fn every_building_is_deterministic() {
    for name in [
        "zip-stored-all",
        "zip-zip64-full",
        "xml-utf16le-bom",
        "zip-deflate-levels-mixed",
        "zip-truncated-in-entry",
    ] {
        assert_eq!(find(name).bytes(), find(name).bytes(), "{name}");
    }
}

#[test]
fn zip_variants_have_the_structure_they_claim() {
    let bytes = |n: &str| find(n).bytes();
    let stored = inspect::read(&bytes("zip-stored-all")).unwrap();
    assert!(stored.records.iter().all(|r| r.method == 0));

    let signed = bytes("zip-data-descriptor-signed");
    let a = inspect::read(&signed).unwrap();
    for r in &a.records {
        assert_eq!(r.flags & 8, 8);
        let end = r.offset as usize + 30 + r.name.len() + r.csize as usize;
        let local_len = u16::from_le_bytes([
            signed[r.offset as usize + 28],
            signed[r.offset as usize + 29],
        ]) as usize;
        assert_eq!(&signed[end + local_len..end + local_len + 4], b"PK\x07\x08");
    }

    let unsigned = bytes("zip-data-descriptor-unsigned");
    let a = inspect::read(&unsigned).unwrap();
    let r = &a.records[0];
    let at = r.offset as usize + 30 + r.name.len() + r.csize as usize;
    assert_ne!(&unsigned[at..at + 4], b"PK\x07\x08");
    assert_eq!(
        u32::from_le_bytes([
            unsigned[at],
            unsigned[at + 1],
            unsigned[at + 2],
            unsigned[at + 3]
        ]),
        r.crc
    );

    let z64 = bytes("zip-zip64-full");
    let a = inspect::read(&z64).unwrap();
    assert!(a.zip64);
    assert!(a.records.iter().all(|r| extra_ids(&r.extra) == [1]));

    let local_only = bytes("zip-zip64-extra-local-only");
    let a = inspect::read(&local_only).unwrap();
    assert!(!a.zip64 && a.records.iter().all(|r| r.extra.is_empty()));
    assert_eq!(&local_only[30 + 19..30 + 19 + 2], &[1, 0]);

    assert!(
        !inspect::read(&bytes("zip-archive-comment"))
            .unwrap()
            .comment
            .is_empty()
    );
    let c = inspect::read(&bytes("zip-entry-comments")).unwrap();
    assert!(
        c.records
            .iter()
            .all(|r| r.comment.starts_with(b"entry comment"))
    );

    let prefix = bytes("zip-prefix-sfx-stub");
    assert!(prefix.starts_with(b"MZ"));
    assert!(inspect::read(&prefix).unwrap().records[0].offset > 80);

    assert_eq!(
        names(&bytes("zip-central-order-differs"))[0],
        "docProps/app.xml"
    );
    assert_eq!(
        names(&bytes("zip-order-content-types-last"))
            .last()
            .map(String::as_str),
        Some("[Content_Types].xml")
    );

    let dirs = names(&bytes("zip-directory-entries"));
    assert_eq!(&dirs[..2], ["_rels/", "word/"]);

    let a220 = inspect::read(&bytes("zip-extra-a220-alignment")).unwrap();
    assert!(a220.records.iter().all(|r| r.extra.is_empty()));
    assert_eq!(
        &bytes("zip-extra-a220-alignment")[30 + 19..30 + 19 + 2],
        &[0x20, 0xA2]
    );

    let flags: BTreeSet<u16> = inspect::read(&bytes("zip-deflate-levels-mixed"))
        .unwrap()
        .records
        .iter()
        .map(|r| r.flags & 6)
        .collect();
    assert_eq!(flags, BTreeSet::from([0, 2, 4, 6]));

    let utf8 = inspect::read(&bytes("zip-nonascii-name-utf8-flag")).unwrap();
    assert!(
        utf8.records
            .iter()
            .any(|r| r.flags & 0x800 != 0 && !r.name.is_ascii())
    );
    let cp437 = inspect::read(&bytes("zip-nonascii-name-no-flag-cp437")).unwrap();
    assert!(
        cp437
            .records
            .iter()
            .any(|r| r.flags & 0x800 == 0 && !r.name.is_ascii())
    );
    let ascii = inspect::read(&bytes("zip-ascii-name-with-utf8-flag")).unwrap();
    assert!(
        ascii
            .records
            .iter()
            .all(|r| r.flags & 0x800 != 0 && r.name.is_ascii())
    );

    let zero = inspect::read(&bytes("zip-dos-time-zero")).unwrap();
    assert!(zero.records.iter().all(|r| (r.time, r.date) == (0, 0)));
    let host = inspect::read(&bytes("zip-unix-attrs")).unwrap();
    assert!(host.records.iter().all(|r| r.version_made >> 8 == 3));
}

#[test]
fn xml_variants_have_the_bytes_they_claim() {
    let doc = |n: &str| {
        let bytes = find(n).bytes();
        let a = inspect::read(&bytes).unwrap();
        let rec = a
            .records
            .iter()
            .find(|r| r.name == b"word/document.xml")
            .unwrap();
        inspect::extract(&bytes, rec).unwrap()
    };
    assert!(doc("xml-bom-utf8").starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(doc("xml-utf16le-bom").starts_with(&[0xFF, 0xFE, b'<', 0]));
    assert!(doc("xml-utf16be-bom").starts_with(&[0xFE, 0xFF, 0, b'<']));
    assert!(doc("xml-no-declaration").starts_with(b"<w:document"));
    assert!(doc("xml-declaration-single-quotes").starts_with(b"<?xml version='1.0'"));
    let crlf = doc("xml-crlf-everywhere");
    assert!(
        crlf.windows(2).any(|w| w == b"\r\n")
            && !crlf.windows(2).any(|w| w[0] != b'\r' && w[1] == b'\n')
    );
    assert!(
        doc("xml-cr-in-text-content")
            .windows(3)
            .any(|w| w == b"a\rb")
    );
    assert!(doc("xml-no-trailing-newline").ends_with(b"</w:document>"));
    assert!(
        doc("xml-cdata-in-text")
            .windows(9)
            .any(|w| w == b"<![CDATA[")
    );
    assert!(
        doc("xml-strict-ooxml")
            .windows(31)
            .any(|w| w == b"http://purl.oclc.org/ooxml/word")
    );
    assert!(
        doc("xml-default-namespace-for-w")
            .windows(9)
            .any(|w| w == b"<document")
    );
    let refs = doc("xml-char-refs-numeric");
    for needle in [&b"&#xA0;"[..], b"&#xa9;", b"&#169;", b"&#x1F600;"] {
        assert!(refs.windows(needle.len()).any(|w| w == needle));
    }
}

#[test]
fn hostile_fixtures_are_broken_in_the_way_they_claim() {
    let bytes = |n: &str| find(n).bytes();
    assert!(bytes("zip-empty-file").is_empty());
    assert!(inspect::read(&bytes("zip-random-garbage")).is_err());
    assert!(bytes("zip-ole-cfb-header").starts_with(&[0xD0, 0xCF, 0x11, 0xE0]));
    assert!(inspect::read(&bytes("zip-truncated-in-entry")).is_err());
    assert!(inspect::read(&bytes("zip-truncated-before-eocd")).is_err());
    assert!(inspect::read(&bytes("zip-truncated-in-central-directory")).is_err());
    assert!(inspect::read(&bytes("zip-entry-count-lies")).is_err());
    let crc = bytes("zip-crc-mismatch");
    let a = inspect::read(&crc).unwrap();
    let rec = a
        .records
        .iter()
        .find(|r| r.name == b"word/document.xml")
        .unwrap();
    assert_eq!(inspect::extract(&crc, rec).unwrap_err(), "crc mismatch");
    let corrupt = bytes("zip-corrupt-deflate-stream");
    let a = inspect::read(&corrupt).unwrap();
    assert!(
        inspect::extract(
            &corrupt,
            a.records
                .iter()
                .find(|r| r.name == b"word/document.xml")
                .unwrap()
        )
        .is_err()
    );
    let lies = inspect::read(&bytes("zip-usize-lies-4gb")).unwrap();
    assert!(lies.records.iter().any(|r| r.usize == 0xFFFF_FFF0));
    let enc = inspect::read(&bytes("zip-encrypted-flag")).unwrap();
    assert!(enc.records.iter().any(|r| r.flags & 1 == 1));
    let m99 = inspect::read(&bytes("zip-unsupported-method-99")).unwrap();
    assert!(m99.records.iter().any(|r| r.method == 99));
    let dup = names(&bytes("zip-duplicate-entry-names"));
    assert_eq!(dup.iter().filter(|n| *n == "word/document.xml").count(), 2);
    assert!(
        names(&bytes("zip-path-traversal-dotdot"))
            .iter()
            .any(|n| n.starts_with("../"))
    );
    assert!(
        names(&bytes("zip-path-absolute-and-backslash"))
            .iter()
            .any(|n| n.starts_with('/'))
    );
    assert!(
        inspect::read(&bytes("zip-name-with-nul"))
            .unwrap()
            .records
            .iter()
            .any(|r| r.name.contains(&0))
    );
    assert_eq!(
        inspect::read(&bytes("zip-many-entries-20000"))
            .unwrap()
            .records
            .len(),
        20_005
    );
    let shared = inspect::read(&bytes("zip-overlapping-entries-shared-data")).unwrap();
    let last = shared.records[5].offset;
    assert_eq!(
        shared.records.iter().filter(|r| r.offset == last).count(),
        41
    );
    assert_eq!(
        inspect::read(&bytes("zip-local-method-differs"))
            .unwrap()
            .records[0]
            .method,
        8
    );
    assert!(
        inspect::read(&bytes("zip-eocd-only"))
            .unwrap()
            .records
            .is_empty()
    );
}

#[test]
fn the_bomb_is_small_on_disk_and_large_when_read() {
    let bytes = find("zip-bomb-ratio-1000").bytes();
    let a = inspect::read(&bytes).unwrap();
    let rec = a
        .records
        .iter()
        .find(|r| r.name == b"word/document.xml")
        .unwrap();
    assert!(bytes.len() < 400_000, "{}", bytes.len());
    assert!(rec.usize > 150 * 1024 * 1024);
    assert!(rec.usize / rec.csize >= 500);
}

#[test]
fn xml_hostile_fixtures_carry_their_payload() {
    let doc = |n: &str| {
        let bytes = find(n).bytes();
        let a = inspect::read(&bytes).unwrap();
        let rec = a
            .records
            .iter()
            .find(|r| r.name == b"word/document.xml")
            .unwrap();
        String::from_utf8_lossy(&inspect::extract(&bytes, rec).unwrap()).into_owned()
    };
    assert!(doc("xml-billion-laughs").contains("<!ENTITY h \"&g;&g;"));
    assert!(doc("xml-external-entity-file").contains("SYSTEM \"file:///etc/passwd\""));
    assert!(doc("xml-deep-nesting-20000").matches("<w:ins ").count() == 20_000);
    assert!(doc("xml-attributes-100000").contains("a99999=\"1\""));
    assert!(doc("xml-xml11-declaration").starts_with("<?xml version=\"1.1\""));
    assert!(doc("xml-two-root-elements").ends_with(
        "<w:extra xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>"
    ));
}

pub(super) fn part_bytes(bytes: &[u8], name: &str) -> Vec<u8> {
    let a = inspect::read(bytes).unwrap();
    let rec = a
        .records
        .iter()
        .find(|r| r.name == name.as_bytes())
        .unwrap_or_else(|| panic!("no part {name}"));
    inspect::extract(bytes, rec).unwrap()
}

fn part(bytes: &[u8], name: &str) -> String {
    String::from_utf8(part_bytes(bytes, name)).unwrap()
}

fn position(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not found"))
}

#[test]
fn the_word_fixtures_have_the_shape_the_validator_needs() {
    let clean = word::clean();
    let doc = part(&clean, "word/document.xml");
    assert_eq!(doc.matches("<w:tbl>").count(), 1);
    assert_eq!(doc.matches("<w:sectPr>").count(), 1);
    assert!(position(&doc, "<w:pStyle") < position(&doc, "<w:jc"));
    assert!(position(&doc, "<w:tblBorders>") < position(&doc, "<w:tblLook"));
    assert!(position(&doc, "<w:tblPr>") < position(&doc, "<w:tblGrid>"));
    assert!(position(&doc, "<w:tblGrid>") < position(&doc, "<w:tr>"));
    assert!(part(&clean, "word/comments.xml").contains("<w:comment w:id=\"0\""));
    assert!(
        part(&clean, "word/_rels/document.xml.rels")
            .contains("/comments\" Target=\"comments.xml\"")
    );
    assert!(part(&clean, "[Content_Types].xml").contains("/word/comments.xml"));

    let bad = part(&word::misordered(), "word/document.xml");
    assert!(position(&bad, "<w:jc") < position(&bad, "<w:pStyle"));
    assert!(position(&bad, "<w:tblLook") < position(&bad, "<w:tblBorders>"));
    assert_eq!(
        bad.len(),
        doc.len(),
        "the two differ only in the order of children"
    );

    let on = part(&word::extensions(true), "word/document.xml");
    let off = part(&word::extensions(false), "word/document.xml");
    assert!(on.contains("mc:Ignorable=\"w14\"") && !off.contains("Ignorable"));
    for d in [&on, &off] {
        assert!(d.contains("w14:paraId=\"1A2B3C4D\"") && d.contains("w14:textId="));
    }

    let alt = part(&word::alternate_content(), "word/document.xml");
    assert!(position(&alt, "<mc:Choice Requires=\"wps\">") < position(&alt, "<mc:Fallback>"));
    assert_eq!(alt.matches("box text").count(), 2);

    let tag = part(&word::smart_tag(false), "word/document.xml");
    assert!(tag.contains("<w:smartTag ") && tag.contains(NS_W_FOR_TEST));
    let strict = word::smart_tag(true);
    assert!(
        part(&strict, "word/document.xml").contains("purl.oclc.org/ooxml/wordprocessingml/main")
    );
    assert!(
        part(&strict, "_rels/.rels")
            .contains("purl.oclc.org/ooxml/officeDocument/relationships/officeDocument")
    );
}

const NS_W_FOR_TEST: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
