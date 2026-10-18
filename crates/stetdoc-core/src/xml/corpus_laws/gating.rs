//! Tests for the gates of the span tree law: what a good document must have, what makes a refusal
//! that does not happen a failure, and what makes a corpus unusable.

use super::*;

fn zip_of(parts: &[(&str, &[u8])]) -> Vec<u8> {
    // A stored archive, written here so this test does not need the package module.
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in parts {
        let offset = out.len() as u32;
        let mut crc = !0u32;
        for &b in *data {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        let crc = !crc;
        for (sink, record) in [(&mut out, false), (&mut central, true)] {
            sink.extend(if record {
                b"PK\x01\x02".as_slice()
            } else {
                b"PK\x03\x04".as_slice()
            });
            if record {
                sink.extend([20, 0]);
            }
            sink.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            sink.extend(crc.to_le_bytes());
            sink.extend((data.len() as u32).to_le_bytes());
            sink.extend((data.len() as u32).to_le_bytes());
            sink.extend((name.len() as u16).to_le_bytes());
            sink.extend([0, 0]);
            if record {
                sink.extend([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                sink.extend(offset.to_le_bytes());
            }
            sink.extend(name.as_bytes());
            if !record {
                sink.extend(*data);
            }
        }
    }
    let start = out.len() as u32;
    let size = central.len() as u32;
    out.extend(central);
    out.extend(b"PK\x05\x06\0\0\0\0");
    out.extend((parts.len() as u16).to_le_bytes());
    out.extend((parts.len() as u16).to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend([0, 0]);
    out
}

const RELS: &[u8] = b"<Relationships xmlns=\"x\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"/word/main.xml\"/></Relationships>";

fn good() -> Vec<u8> {
    zip_of(&[
        ("_rels/.rels", RELS),
        ("word/main.xml", b"<w:document xmlns:w=\"urn:w\"/>"),
    ])
}

fn ok() -> Expect {
    Expect {
        code: "ok".into(),
        stage: None,
        parts: vec![],
    }
}

fn refused(code: &str, stage: Option<&str>) -> Expect {
    Expect {
        code: code.into(),
        stage: stage.map(str::to_owned),
        parts: vec![],
    }
}

fn refused_in(code: &str, parts: &[&str]) -> Expect {
    Expect {
        parts: parts.iter().map(|p| (*p).to_owned()).collect(),
        ..refused(code, Some("xml"))
    }
}

#[test]
fn a_good_document_needs_a_main_part_that_this_reader_can_read_and_parse() {
    assert!(check_document(&good(), Some(&ok()), false).is_ok());
    assert!(check_document(&good(), None, false).is_ok());
    // The relationships name the main part, so a document without `word/document.xml` is fine.
    let missing = zip_of(&[("_rels/.rels", RELS)]);
    let e = check_document(&missing, Some(&ok()), false).unwrap_err();
    assert!(e.contains("no main part"), "{e}");
    let broken = zip_of(&[("_rels/.rels", RELS), ("word/main.xml", b"<w:document>")]);
    let e = check_document(&broken, Some(&ok()), false).unwrap_err();
    assert!(e.contains("does not parse"), "{e}");
    // A container this reader cannot read does not pass a good document.
    let e = check_document(b"not a zip at all", Some(&ok()), false).unwrap_err();
    assert!(e.contains("cannot read the container"), "{e}");
    assert!(check_document(b"not a zip at all", None, true).is_ok());
    // A relationship with no target, or with one that is not in the container, does not hide
    // the conventional name.
    let rel = |target: &str| {
        format!(
            "<Relationships xmlns=\"x\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"{target}\"/></Relationships>"
        )
    };
    for target in ["", "/missing.xml"] {
        let z = zip_of(&[
            ("_rels/.rels", rel(target).as_bytes()),
            ("word/document.xml", b"<w:document xmlns:w=\"urn:w\"/>"),
        ]);
        assert!(check_document(&z, Some(&ok()), false).is_ok(), "{target:?}");
        let broken = zip_of(&[
            ("_rels/.rels", rel(target).as_bytes()),
            ("word/document.xml", b"<w:document>"),
        ]);
        assert!(
            check_document(&broken, Some(&ok()), false).is_err(),
            "{target:?}"
        );
    }
    // A part that does not parse is fine when it is not the main part.
    let junk = zip_of(&[
        ("_rels/.rels", RELS),
        ("word/main.xml", b"<a/>"),
        ("customXml/x.xml", b"junk"),
    ]);
    assert!(check_document(&junk, Some(&ok()), false).is_ok());
}

#[test]
fn the_hostile_tier_is_not_asked_for_a_main_part() {
    // A valid ZIP file of another kind, which the index calls `ok` because nothing at this
    // layer refuses it.
    let other = zip_of(&[("mimetype", b"application/x"), ("content.xml", b"<a/>")]);
    assert!(check_document(&other, Some(&ok()), true).is_ok());
    assert!(check_document(&other, Some(&ok()), false).is_err());
}

#[test]
fn a_refusal_at_the_xml_stage_has_to_happen() {
    let broken = zip_of(&[("_rels/.rels", RELS), ("word/main.xml", b"<w:document>")]);
    let at_xml = refused("malformed_input", Some("xml"));
    assert!(check_document(&broken, Some(&at_xml), false).is_ok());
    let wrong = refused("limit_exceeded", Some("xml"));
    assert!(check_document(&broken, Some(&wrong), false).is_err());
    let e = check_document(&good(), Some(&at_xml), false).unwrap_err();
    assert!(e.contains("none was"), "{e}");
    // With the parts named, the refusal has to be in one of them. A document whose named parts
    // are none that this test parses is not asked for one.
    let broken = zip_of(&[
        ("_rels/.rels", RELS),
        ("word/main.xml", b"<w:document>"),
        ("customXml/ok.xml", b"<a/>"),
    ]);
    for (parts, passes) in [
        (vec!["word/main.xml"], true),
        (vec!["WORD/main.xml"], true),
        (vec!["customXml/ok.xml", "word/main.xml"], true),
        (vec!["customXml/ok.xml"], false),
        (vec!["word/media/odd.bin"], true),
    ] {
        let r = check_document(&broken, Some(&refused_in("malformed_input", &parts)), false);
        assert_eq!(r.is_ok(), passes, "{parts:?}: {r:?}");
    }
    // Other stages are checked by the container laws, and need nothing here.
    for stage in [Some("open"), Some("read"), Some("document"), None] {
        assert!(check_document(&good(), Some(&refused("malformed_input", stage)), false).is_ok());
    }
}

#[test]
fn the_index_is_read_by_column_name_and_may_have_a_stage() {
    let text = "id\ttier\tstage\texpect\tpath\tpart\nx\thostile\txml\tmalformed_input\th/x.docx\tword/a.xml;word/b.xml\ny\tlab\t\tok\tl/y.docx\t-\n";
    let ix = parse_index(text).unwrap();
    let x = &ix[Path::new("h/x.docx")];
    assert_eq!(
        (x.code.as_str(), x.stage.as_deref()),
        ("malformed_input", Some("xml"))
    );
    assert_eq!(x.parts, ["word/a.xml", "word/b.xml"]);
    assert_eq!(ix[Path::new("l/y.docx")].stage, None);
    assert!(ix[Path::new("l/y.docx")].parts.is_empty());
    assert!(parse_index("path\tstage\nq\txml\n").is_err());
    assert!(parse_index("path\texpect\nq.docx\tok\nq.docx\tok\n").is_err());
}

#[test]
fn a_set_has_to_be_whole_and_a_file_that_cannot_be_read_fails() {
    let dir = std::env::temp_dir().join(format!("stetdoc-xml-gating-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let name = dir.display().to_string();
    assert!(
        load_from("", "curated")
            .unwrap_err()
            .contains("no directory")
    );
    assert!(
        load_from(&name, "curated")
            .unwrap_err()
            .contains("no COMPLETE marker")
    );
    std::fs::write(dir.join("COMPLETE"), b"").unwrap();
    assert!(
        load_from(&name, "curated")
            .unwrap_err()
            .contains("holds no documents")
    );
    std::fs::write(dir.join("a.docx"), good()).unwrap();
    let set = load_from(&name, "curated").unwrap().remove(0);
    assert!(check_path(&dir.join("a.docx"), &set).is_ok());
    assert!(
        check_path(&dir.join("gone.docx"), &set)
            .unwrap_err()
            .contains("cannot be read")
    );
    let _ = std::fs::remove_dir_all(&dir);
}
