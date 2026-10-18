//! Tests for the gates of the corpus laws: what makes a named corpus unusable, and what makes a
//! document that is expected to be refused fail when it is not.

use super::super::testzip::{E, Z, docx_entries};
use super::*;
use crate::api::error::ErrorCode;

/// A directory under the system's temporary directory that is removed afterwards.
struct Scratch(PathBuf);

#[cfg(test)]
impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("stetdoc-gating-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn put(&self, rel: &str, bytes: &[u8]) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn path(&self) -> String {
        self.0.display().to_string()
    }
}

#[cfg(test)]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn doc() -> Vec<u8> {
    Z::new(docx_entries()).build()
}

#[test]
fn a_set_has_to_be_whole_to_be_used() {
    assert!(
        load_from("", "curated")
            .unwrap_err()
            .contains("no directory")
    );
    assert!(
        load_from(":", "curated")
            .unwrap_err()
            .contains("no directory")
    );

    let s = Scratch::new("whole");
    s.put("a/one.docx", &doc());
    let e = load_from(&s.path(), "curated").unwrap_err();
    assert!(e.contains("no COMPLETE marker"), "{e}");

    s.put("COMPLETE", b"");
    assert_eq!(load_from(&s.path(), "curated").unwrap()[0].docs.len(), 1);

    let empty = Scratch::new("empty");
    empty.put("COMPLETE", b"");
    empty.put("notes.txt", b"not a document");
    let e = load_from(&empty.path(), "curated").unwrap_err();
    assert!(e.contains("holds no documents"), "{e}");

    // A named set that is complete next to one that is not fails as a whole.
    let e = load_from(&format!("{}:{}", s.path(), empty.path()), "curated").unwrap_err();
    assert!(e.contains("holds no documents"), "{e}");

    // `sets/<name>` is used when it exists.
    let nested = Scratch::new("nested");
    nested.put("sets/curated/COMPLETE", b"");
    nested.put("sets/curated/x/two.docx", &doc());
    nested.put("elsewhere/three.docx", &doc());
    let loaded = load_from(&nested.path(), "curated").unwrap();
    assert_eq!(loaded[0].docs.len(), 1);
    assert!(
        load_from(&nested.path(), "full")
            .unwrap_err()
            .contains("COMPLETE")
    );
}

#[test]
fn the_index_must_agree_with_the_files() {
    let s = Scratch::new("index");
    s.put("COMPLETE", b"");
    s.put("a.docx", &doc());
    s.put("b.docx", &doc());
    s.put("INDEX.tsv", b"id\tpath\texpect\nx\ta.docx\tok\n");
    let e = load_from(&s.path(), "curated").unwrap_err();
    assert!(e.contains("1 documents are not in INDEX.tsv"), "{e}");
    s.put(
        "INDEX.tsv",
        b"id\tpath\texpect\nx\ta.docx\tok\ny\tb.docx\tok\nz\tgone.docx\tok\n",
    );
    let e = load_from(&s.path(), "curated").unwrap_err();
    assert!(e.contains("1 files it lists are not there"), "{e}");
    s.put(
        "INDEX.tsv",
        b"id\tpath\texpect\nx\ta.docx\tok\ny\tb.docx\tok\n",
    );
    assert!(load_from(&s.path(), "curated").is_ok());
}

fn expect(code: &str, stage: Option<&str>, parts: &[&str]) -> Expect {
    Expect {
        code: code.into(),
        stage: stage.map(str::to_owned),
        parts: parts.iter().map(|p| (*p).to_owned()).collect(),
    }
}

#[test]
fn the_index_is_read_by_column_name_and_may_have_a_stage() {
    let text = "id\ttier\tstage\tsize\texpect\tsha256\tpath\tproducer\tpart\nx\thostile\topen\t5\tmalformed_input\tabc\th/x.docx\tp\t-\ny\tlab\t\t7\tok\tdef\tl/y.docx\tq\t\nz\thostile\txml\t7\tmalformed_input\tdef\th/z.docx\tq\tword/a.xml;word/b.xml\n";
    let ix = parse_index(text).unwrap();
    let x = &ix[Path::new("h/x.docx")];
    assert_eq!(
        (x.code.as_str(), x.stage.as_deref()),
        ("malformed_input", Some("open"))
    );
    assert!(x.parts.is_empty());
    let y = &ix[Path::new("l/y.docx")];
    assert_eq!((y.code.as_str(), y.stage.as_deref()), ("ok", None));
    assert!(y.parts.is_empty());
    let z = &ix[Path::new("h/z.docx")];
    assert_eq!(z.parts, ["word/a.xml", "word/b.xml"]);
    // The index that has no stage column at all.
    let ix = parse_index("path\texpect\nq.docx\tok\n").unwrap();
    assert_eq!(ix[Path::new("q.docx")].stage, None);
    assert!(parse_index("id\tpath\nx\ty\n").is_err());
    assert!(parse_index("path\texpect\nq.docx\n").is_err());
    assert!(parse_index("path\texpect\nq.docx\tok\nq.docx\tok\n").is_err());
}

/// Flips a CRC so that reading the part fails after `open` succeeds.
fn with_bad_crc() -> Vec<u8> {
    let mut bytes = doc();
    let p = Package::open(bytes.clone(), &Limits::default()).unwrap();
    let at = p.layout.entries[2].cd.start + 16;
    bytes[at] ^= 0xFF;
    bytes
}

fn with_bad_xml() -> Vec<u8> {
    let mut es = docx_entries();
    es[2] = E::new("word/document.xml", b"<w:document>");
    Z::new(es).build()
}

#[test]
fn a_refusal_that_does_not_happen_is_a_failure() {
    let refuse = |bytes: Vec<u8>, code: &str, stage: Option<&str>| {
        check_refusal(bytes, &expect(code, stage, &[]))
    };
    // A good document is not refused, whatever the index expects.
    for stage in [None, Some("open"), Some("read"), Some("xml")] {
        let e = refuse(doc(), "malformed_input", stage).unwrap_err();
        assert!(e.contains("malformed_input"), "{stage:?}: {e}");
    }
    // It is refused with a different code than the index says.
    let e = refuse(vec![0; 64], "limit_exceeded", None).unwrap_err();
    assert!(e.contains("open returned"), "{e}");
    assert!(refuse(vec![0; 64], "malformed_input", Some("open")).is_ok());
    assert!(refuse(vec![0; 64], "malformed_input", None).is_ok());

    // A part that fails when it is read.
    let bad_crc = with_bad_crc();
    assert!(refuse(bad_crc.clone(), "malformed_input", Some("read")).is_ok());
    assert!(refuse(bad_crc.clone(), "malformed_input", None).is_ok());
    assert!(refuse(bad_crc.clone(), "malformed_input", Some("open")).is_err());
    assert!(refuse(bad_crc.clone(), "malformed_input", Some("xml")).is_err());
    assert!(refuse(bad_crc, "limit_exceeded", Some("read")).is_err());

    // A part that fails when it is parsed.
    let bad_xml = with_bad_xml();
    assert!(refuse(bad_xml.clone(), "malformed_input", Some("xml")).is_ok());
    assert!(refuse(bad_xml.clone(), "malformed_input", None).is_ok());
    assert!(refuse(bad_xml.clone(), "malformed_input", Some("read")).is_err());
    assert!(refuse(bad_xml, "limit_exceeded", Some("xml")).is_err());

    // A refusal from a layer above is checked as an ordinary document.
    assert!(refuse(doc(), "malformed_input", Some("document")).is_ok());
    assert!(refuse(doc(), "malformed_input", Some("surprise")).is_err());
}

#[test]
fn a_refusal_that_comes_earlier_than_the_index_says_is_a_failure() {
    // The package is refused at open. An index that says it is refused when a part is read, or
    // parsed, or by the layers above, is wrong about this document, and the law says so.
    for stage in ["read", "xml", "document"] {
        let e =
            check_refusal(vec![0; 64], &expect("malformed_input", Some(stage), &[])).unwrap_err();
        assert!(e.contains("earlier than the index says"), "{stage}: {e}");
        assert!(e.contains(stage), "{stage}: {e}");
    }
    // The same holds when a part is named.
    let e = check_refusal(
        vec![0; 64],
        &expect("malformed_input", Some("read"), &["word/document.xml"]),
    )
    .unwrap_err();
    assert!(e.contains("earlier than the index says"), "{e}");
    // And the same refusal at the stage the index gives passes.
    assert!(check_refusal(vec![0; 64], &expect("malformed_input", Some("open"), &[])).is_ok());
}

#[test]
fn a_refusal_has_to_come_from_the_part_the_index_names() {
    let (crc, xml) = (with_bad_crc(), with_bad_xml());
    // The part whose CRC is wrong is word/document.xml (the third entry).
    let read = |parts: &[&str]| {
        check_refusal(crc.clone(), &expect("malformed_input", Some("read"), parts))
    };
    assert!(read(&["word/document.xml"]).is_ok());
    assert!(read(&["word/document.xml", "_rels/.rels"]).is_ok());
    assert!(read(&[]).is_ok());
    let e = read(&["_rels/.rels"]).unwrap_err();
    assert!(e.contains("of _rels/.rels"), "{e}");
    let e = read(&["word/missing.xml"]).unwrap_err();
    assert!(e.contains("not in the package"), "{e}");
    let parse =
        |parts: &[&str]| check_refusal(xml.clone(), &expect("malformed_input", Some("xml"), parts));
    assert!(parse(&["word/document.xml"]).is_ok());
    assert!(parse(&[]).is_ok());
    assert!(parse(&["[Content_Types].xml"]).is_err());
    // A part that does not read is not one that fails to parse.
    let at_xml = check_refusal(
        crc,
        &expect("malformed_input", Some("xml"), &["word/document.xml"]),
    );
    assert!(at_xml.is_err());
}

#[test]
fn a_part_refused_with_another_code_is_named_and_not_called_a_clean_one() {
    // The CRC of word/document.xml is wrong: malformed_input at read. An index that expects
    // limit_exceeded there is told what happened, not that the part reads as a good one does.
    let e = check_refusal(
        with_bad_crc(),
        &expect("limit_exceeded", Some("read"), &["word/document.xml"]),
    )
    .unwrap_err();
    assert!(
        e.contains("reading word/document.xml is refused with malformed_input"),
        "{e}"
    );
    assert!(!e.contains("as expected of a good one"), "{e}");
    // The same for a part that does not parse, whether or not the index names it.
    for parts in [&[][..], &["word/document.xml"][..]] {
        let e = check_refusal(
            with_bad_xml(),
            &expect("limit_exceeded", Some("xml"), parts),
        )
        .unwrap_err();
        assert!(
            e.contains("parsing word/document.xml is refused with malformed_input"),
            "{parts:?}: {e}"
        );
        assert!(!e.contains("as expected of a good one"), "{e}");
    }
    // A part that fails when it is read, where the index says that it reads and fails to parse,
    // is a refusal that comes earlier than the index says, with the code the index gives.
    let e = check_refusal(
        with_bad_crc(),
        &expect("malformed_input", Some("xml"), &["word/document.xml"]),
    )
    .unwrap_err();
    assert!(
        e.contains("reading word/document.xml is refused already")
            && e.contains("earlier than the index says"),
        "{e}"
    );
}

/// A document whose main part and `_rels/.rels` are both not XML.
fn with_two_bad_xml_parts() -> Vec<u8> {
    let mut es = docx_entries();
    es[1] = E::new("_rels/.rels", b"<Relationships>");
    es[2] = E::new("word/document.xml", b"<w:document>");
    Z::new(es).build()
}

#[test]
fn the_parts_the_index_does_not_name_have_to_read_and_parse_cleanly() {
    // `Expect::seen_by_reading` tells a layer that reads none of the named parts that the
    // document opens. That holds only if no other part is damaged, so the law checks it.
    let two = with_two_bad_xml_parts();
    let named = check_refusal(
        two.clone(),
        &expect("malformed_input", Some("xml"), &["word/document.xml"]),
    )
    .unwrap_err();
    assert!(
        named.contains("but _rels/.rels, which it does not name, fails too")
            && named.contains("parsing it is refused with malformed_input"),
        "{named}"
    );
    assert!(
        check_refusal(
            two.clone(),
            &expect(
                "malformed_input",
                Some("xml"),
                &["word/document.xml", "_rels/.rels"]
            )
        )
        .is_ok(),
        "naming both parts is the fix"
    );
    // A part that does not read, next to the one that is named.
    let mut two_crcs = with_bad_crc();
    let p = Package::open(two_crcs.clone(), &Limits::default()).unwrap();
    two_crcs[p.layout.entries[1].cd.start + 16] ^= 0xFF;
    let crc = with_bad_crc();
    let e = check_refusal(
        two_crcs.clone(),
        &expect("malformed_input", Some("read"), &["word/document.xml"]),
    )
    .unwrap_err();
    assert!(
        e.contains("but _rels/.rels, which it does not name, fails too")
            && e.contains("reading it is refused with malformed_input"),
        "{e}"
    );
    assert!(
        check_refusal(
            two_crcs,
            &expect(
                "malformed_input",
                Some("read"),
                &["word/document.xml", "_rels/.rels"]
            )
        )
        .is_ok()
    );
    // With no part named, any part may carry the refusal, so the law asks nothing of the others.
    assert!(check_refusal(two, &expect("malformed_input", Some("xml"), &[])).is_ok());
    assert!(check_refusal(crc, &expect("malformed_input", Some("read"), &[])).is_ok());
}

#[test]
fn the_other_reader_has_to_see_what_this_one_sees() {
    let bytes = doc();
    let pkg = Package::open(bytes.clone(), &Limits::default()).unwrap();
    let ours = summarize(&pkg).unwrap();
    assert_eq!(ours.len(), 4);
    assert!(another_reader_agrees("the input", &bytes, &ours).is_ok());
    // A part that is missing, extra or different in the other reader's view fails the law.
    let mut fewer = summarize(&pkg).unwrap();
    fewer.pop();
    let e = another_reader_agrees("the input", &bytes, &fewer).unwrap_err();
    assert!(e.contains("parts here"), "{e}");
    let mut other = summarize(&pkg).unwrap();
    other[2].crc ^= 1;
    let e = another_reader_agrees("the input", &bytes, &other).unwrap_err();
    assert!(e.contains("reads differently"), "{e}");
    let mut renamed = summarize(&pkg).unwrap();
    renamed[1].name = "other.xml".into();
    assert!(another_reader_agrees("the input", &bytes, &renamed).is_err());
    // An archive the other reader refuses is a failure too, even though it opens here.
    let mut es = docx_entries();
    es[1].comment = vec![b'x'; 10];
    let mut z = Z::new(es);
    z.pre_eocd = vec![0xAA; 5];
    let e = another_reader_agrees("the input", &z.build(), &ours).unwrap_err();
    assert!(e.contains("the other reader refuses it"), "{e}");
    // A package with a part that does not read here is not summarized.
    let broken = Package::open(with_bad_crc(), &Limits::default()).unwrap();
    assert!(summarize(&broken).is_none());
}

#[test]
fn documents_are_checked_as_the_index_says_and_files_that_cannot_be_read_fail() {
    let ok = expect("ok", None, &[]);
    let refused = expect(ErrorCode::MalformedInput.as_str(), Some("open"), &[]);
    assert!(check_document(doc(), Some(&ok), false).is_ok());
    assert!(check_document(vec![1, 2, 3], Some(&ok), false).is_err());
    assert!(check_document(vec![1, 2, 3], Some(&refused), false).is_ok());
    assert!(check_document(doc(), Some(&refused), false).is_err());
    // No index: a document that is not under `hostile` has to round trip, and one that is has
    // to fail cleanly.
    assert!(check_document(vec![1, 2, 3], None, false).is_err());
    assert!(check_document(vec![1, 2, 3], None, true).is_ok());
    // A file that cannot be read fails, and is not skipped.
    let s = Scratch::new("unreadable");
    s.put("COMPLETE", b"");
    s.put("a.docx", &doc());
    let set = load_from(&s.path(), "curated").unwrap().remove(0);
    assert!(check_path(&s.0.join("a.docx"), &set).is_ok());
    let e = check_path(&s.0.join("gone.docx"), &set).unwrap_err();
    assert!(e.contains("cannot be read"), "{e}");
}
