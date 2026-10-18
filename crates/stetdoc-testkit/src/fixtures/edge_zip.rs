//! Edge fixtures about the ZIP container: valid archives written in unusual ways.

use super::parts::{Doc, entries};
use super::zip::{DOS_1980, Descriptor, Entry, Zip, Zip64};
use super::{Expect, Fixture, edge_fixture as fx};

const OK: Expect = Expect::Ok;
const XML_ROOT: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<root/>";

fn all(f: impl Fn(Entry) -> Entry) -> Vec<Entry> {
    entries().into_iter().map(f).collect()
}

fn ut_ux() -> Vec<u8> {
    let mut e = Vec::new();
    e.extend(0x5455u16.to_le_bytes());
    e.extend(5u16.to_le_bytes());
    e.push(1);
    e.extend(1_700_000_000u32.to_le_bytes());
    e.extend(0x7875u16.to_le_bytes());
    e.extend(11u16.to_le_bytes());
    e.extend([1, 4]);
    e.extend(501u32.to_le_bytes());
    e.push(4);
    e.extend(20u32.to_le_bytes());
    e
}

fn field(id: u16, data: &[u8]) -> Vec<u8> {
    let mut e = Vec::new();
    e.extend(id.to_le_bytes());
    e.extend((data.len() as u16).to_le_bytes());
    e.extend(data);
    e
}

/// The ZIP-level edge fixtures.
pub(super) fn list() -> Vec<Fixture> {
    vec![
        fx(
            "zip-stored-all",
            "every entry stored (method 0)",
            OK,
            || Zip::new(all(Entry::stored)).build(),
        ),
        fx(
            "zip-data-descriptor-signed",
            "flag bit 3, zeroed local sizes, descriptor with the PK 07 08 signature",
            OK,
            || Zip::new(all(|e| e.descriptor(Descriptor::Signed))).build(),
        ),
        fx(
            "zip-data-descriptor-unsigned",
            "flag bit 3, descriptor without the signature",
            OK,
            || Zip::new(all(|e| e.descriptor(Descriptor::Unsigned))).build(),
        ),
        fx(
            "zip-data-descriptor-zip64-sizes",
            "data descriptor with 8-byte sizes and a zip64 extra field in the local header",
            OK,
            || Zip::new(all(|e| e.descriptor(Descriptor::Zip64Signed))).build(),
        ),
        fx(
            "zip-zip64-full",
            "forced ZIP64: sentinel end-record fields, EOCD64 record and locator, zip64 extras in local and central records",
            OK,
            || Zip::new(entries()).zip64(Zip64::Full).build(),
        ),
        fx(
            "zip-zip64-extra-local-only",
            "Java style: a zip64 extra field in local headers only, real 32-bit sizes in the central directory",
            OK,
            || Zip::new(entries()).zip64(Zip64::LocalExtraOnly).build(),
        ),
        fx(
            "zip-archive-comment",
            "an archive comment with non-ASCII UTF-8 bytes",
            OK,
            || {
                Zip::new(entries())
                    .comment("stetdoc edge: archive comment \u{2713}".as_bytes())
                    .build()
            },
        ),
        fx(
            "zip-entry-comments",
            "comments in the central directory records",
            OK,
            || {
                let es = entries()
                    .into_iter()
                    .enumerate()
                    .map(|(i, e)| e.comment(format!("entry comment {i}").as_bytes()))
                    .collect();
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-prefix-sfx-stub",
            "bytes before the first local header, with offsets counted from the start of the file",
            OK,
            || {
                let mut stub = b"MZ".to_vec();
                stub.extend([0u8; 62]);
                stub.extend(b"self-extractor stub bytes\n");
                Zip::new(entries()).prefix(&stub).build()
            },
        ),
        fx(
            "zip-trailing-bytes",
            "bytes after the end of central directory record",
            OK,
            || Zip::new(entries()).trailing(b"\r\n\0\0").build(),
        ),
        fx(
            "zip-nonascii-name-utf8-flag",
            "an entry name with non-ASCII characters and the UTF-8 flag (bit 11)",
            OK,
            || {
                let mut es = entries();
                es.push(Entry::new(
                    "customXml/caf\u{e9}-\u{65e5}\u{672c}.xml",
                    XML_ROOT,
                ));
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-nonascii-name-no-flag-cp437",
            "a non-ASCII name stored as CP437 bytes without the UTF-8 flag",
            OK,
            || {
                let mut es = entries();
                es.push(Entry::new(b"customXml/caf\x82.xml", XML_ROOT).utf8_flag(false));
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-ascii-name-with-utf8-flag",
            "the UTF-8 flag set although every name is ASCII (python-docx output)",
            OK,
            || Zip::new(all(|e| e.utf8_flag(true))).build(),
        ),
        fx(
            "zip-extra-fields-ut-ux",
            "Info-ZIP extended timestamp 0x5455 and Unix 0x7875 extra fields, made by host 3",
            OK,
            || Zip::new(all(|e| e.extra_both(&ut_ux()).host(3, 30, 0, 0))).build(),
        ),
        fx(
            "zip-extra-a220-alignment",
            "Word's 0xa220 padding extra field in local headers only, so local and central extras differ",
            OK,
            || Zip::new(all(|e| e.extra_local(&field(0xA220, &[0u8; 6])))).build(),
        ),
        fx(
            "zip-extra-cafe-java",
            "the Java 0xcafe marker extra field",
            OK,
            || Zip::new(all(|e| e.extra_both(&field(0xCAFE, &[])))).build(),
        ),
        fx(
            "zip-extra-unknown-id",
            "an extra field with an unknown id",
            OK,
            || Zip::new(all(|e| e.extra_both(&field(0x1234, b"abc")))).build(),
        ),
        fx(
            "zip-directory-entries",
            "directory entries (names ending in a slash)",
            OK,
            || {
                let mut es = vec![
                    Entry::new("_rels/", "").stored(),
                    Entry::new("word/", "").stored().host(0, 20, 0, 0x10),
                ];
                es.extend(entries());
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-order-content-types-last",
            "entry order reversed, so [Content_Types].xml is last (Apple Pages)",
            OK,
            || {
                let mut es = entries();
                es.reverse();
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-central-order-differs",
            "the central directory lists the entries in a different order than the local headers",
            OK,
            || Zip::new(entries()).cd_order(&[4, 3, 2, 1, 0]).build(),
        ),
        fx(
            "zip-dos-time-zero",
            "DOS date and time are both 0, an invalid date",
            OK,
            || Zip::new(all(|e| e.mtime(None))).build(),
        ),
        fx(
            "zip-dos-time-1980",
            "DOS timestamp 1980-01-01 00:00:00, as Word writes",
            OK,
            || Zip::new(all(|e| e.mtime(Some(DOS_1980)))).build(),
        ),
        fx(
            "zip-dos-time-invalid-month",
            "DOS timestamp fields out of range (month 13, day 32, hour 25)",
            OK,
            || {
                let time = (25u16 << 11) | (61 << 5) | 31;
                let date = (44u16 << 9) | (13 << 5) | 32;
                Zip::new(all(|e| e.mtime(Some((time, date))))).build()
            },
        ),
        fx(
            "zip-deflate-levels-mixed",
            "deflate level hint bits that differ per entry (0x6, 0x2, 0x0, 0x4)",
            OK,
            || {
                let es = entries()
                    .into_iter()
                    .zip([1u32, 9, 6, 2, 6])
                    .map(|(e, level)| e.level(level))
                    .collect();
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-unix-attrs",
            "made-by host Unix with a mode in the external attributes and the text-file internal attribute",
            OK,
            || Zip::new(all(|e| e.host(3, 30, 1, 0o100_644 << 16))).build(),
        ),
        fx(
            "zip-empty-entry-deflated-and-stored",
            "zero-length entries, one deflated (03 00) and one stored",
            OK,
            || {
                let mut es = entries();
                es.push(Entry::new("customXml/empty1.xml", ""));
                es.push(Entry::new("customXml/empty2.xml", "").stored());
                Zip::new(es).build()
            },
        ),
        fx(
            "zip-method-mixed",
            "stored and deflated entries in one archive",
            OK,
            || {
                let es = entries()
                    .into_iter()
                    .zip([false, true, false, true, false])
                    .map(|(e, stored)| if stored { e.stored() } else { e })
                    .collect();
                Zip::new(es).build()
            },
        ),
        fx(
            "opc-main-part-renamed",
            "the main part is word/main-doc.xml, found through the root relationship",
            OK,
            || {
                use super::parts::{content_types, entries_with, root_rels};
                let mut es = entries_with(
                    b"",
                    &root_rels().replace("word/document.xml", "word/main-doc.xml"),
                    &content_types().replace("/word/document.xml", "/word/main-doc.xml"),
                );
                es[2] = Entry::new("word/main-doc.xml", Doc::new().bytes());
                Zip::new(es).build()
            },
        ),
        fx(
            "opc-rels-absolute-and-external-targets",
            "an absolute internal relationship target and an external hyperlink target",
            OK,
            || {
                use super::parts::{content_types, entries_with, root_rels};
                let rels = root_rels()
                    .replace("Target=\"word/document.xml\"", "Target=\"/word/document.xml\"")
                    .replace(
                        "</Relationships>",
                        "<Relationship Id=\"rId9\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://example.com/?a=1&amp;b=2\" TargetMode=\"External\"/></Relationships>",
                    );
                Zip::new(entries_with(&Doc::new().bytes(), &rels, &content_types())).build()
            },
        ),
        fx(
            "opc-content-types-attr-order",
            "ContentType before Extension in a Default element",
            OK,
            || {
                use super::parts::{content_types, entries_with, root_rels};
                let ct = content_types()
                    .replace("Extension=\"rels\" ContentType", "ContentType")
                    .replace(
                        "application/vnd.openxmlformats-package.relationships+xml\"/>",
                        "application/vnd.openxmlformats-package.relationships+xml\" Extension=\"rels\"/>",
                    );
                Zip::new(entries_with(&Doc::new().bytes(), &root_rels(), &ct)).build()
            },
        ),
    ]
}
