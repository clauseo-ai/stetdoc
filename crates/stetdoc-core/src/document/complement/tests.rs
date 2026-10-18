//! The manifest and `pending_files`: the canonical form, every refusal, and what counts as a hand edit.

use super::*;
use crate::api::options::OpenOptions;
use crate::document::Document;
use crate::document::tests::{opened, without};

fn doc_md() -> String {
    "# one\n\nparagraph text\n".to_string()
}

/// A working copy with readable files, which the projection will render with step 5.
fn with_files(doc: &Document, files: &[(&str, &str)]) -> WorkingCopy {
    doc.export_with(
        files
            .iter()
            .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
            .collect(),
    )
    .unwrap()
}

fn replace_entry(wc: &mut WorkingCopy, name: &str, bytes: Vec<u8>) {
    for (n, b) in &mut wc.complement.entries {
        if n == name {
            *b = bytes.clone();
        }
    }
}

#[test]
fn the_manifest_is_one_canonical_text_that_names_every_entry() {
    let doc = without(&opened(), "p1");
    let wc = with_files(&doc, &[("notes.md", "n\n"), ("document.md", &doc_md())]);
    let text = String::from_utf8(wc.complement.entry(MANIFEST).unwrap().unwrap().to_vec()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "stetdoc-complement 1.0");
    assert_eq!(lines[1], "generation 1");
    assert_eq!(lines[2], "format stet/1");
    assert!(
        lines[3].starts_with("base ") && lines[3].contains(":1:"),
        "{}",
        lines[3]
    );
    assert!(lines[4].starts_with("original original.docx "));
    assert!(
        lines[5].starts_with("head head.1.") && lines[5].contains(".docx "),
        "{}",
        lines[5]
    );
    assert!(
        lines[6].starts_with("refs refs.1.") && lines[6].contains(".map "),
        "{}",
        lines[6]
    );
    // Files by name, whatever order they were given in.
    assert!(lines[7].starts_with("file document.md "), "{}", lines[7]);
    assert!(lines[8].starts_with("file notes.md "), "{}", lines[8]);
    assert!(lines[9].starts_with("manifest-sha256 "));
    assert_eq!(lines.len(), 10);
    let manifest = Manifest::parse(text.as_bytes()).unwrap();
    assert_eq!(manifest.render(), text);
    // Every entry the manifest names is in the complement, and nothing else is but the manifest.
    let mut names: Vec<&str> = wc
        .complement
        .entries
        .iter()
        .map(|(n, _)| n.as_str())
        .collect();
    names.sort_unstable();
    let mut wanted = manifest.entry_names();
    wanted.push(MANIFEST);
    wanted.sort_unstable();
    assert_eq!(names, wanted);
    // The digests in it are the digests of the bytes.
    let head = wc
        .complement
        .entry(&manifest.head.as_ref().unwrap().name)
        .unwrap()
        .unwrap();
    assert_eq!(sha256(head), manifest.head.as_ref().unwrap().sha256);
}

#[test]
fn an_unedited_document_stores_one_package_and_an_edited_one_two() {
    let unedited = opened().export().unwrap();
    let manifest = unedited.manifest().unwrap();
    assert_eq!(manifest.head, None);
    assert!(
        String::from_utf8_lossy(unedited.complement.entry(MANIFEST).unwrap().unwrap())
            .contains("\nhead original\n")
    );
    assert_eq!(unedited.complement.entries.len(), 3);
    let edited = without(&opened(), "p1").export().unwrap();
    assert!(edited.manifest().unwrap().head.is_some());
    assert_eq!(edited.complement.entries.len(), 4);
}

#[test]
fn an_entry_is_named_by_its_generation_and_its_digest() {
    // Two exports at one generation whose bytes differ get different names, so neither overwrites the other.
    let one = without(&opened(), "p1")
        .export()
        .unwrap()
        .manifest()
        .unwrap();
    let other = without(&opened(), "p2")
        .export()
        .unwrap()
        .manifest()
        .unwrap();
    assert_eq!(one.generation, other.generation);
    assert_ne!(
        one.head.as_ref().unwrap().name,
        other.head.as_ref().unwrap().name
    );
    assert_ne!(one.refs.name, other.refs.name);
    assert_eq!(one.original, other.original);
    for name in [&one.refs.name, &one.head.as_ref().unwrap().name] {
        assert!(is_generated(name), "{name}");
    }
    for name in [
        "head.1.zzzzzzzz.docx",
        "head.x.1a2b3c4d.docx",
        "refs.1.1a2b3c4d.docx",
        "original.docx",
        "manifest",
        "writing",
        "head.1.1a2b3c4d.docx.old",
    ] {
        assert!(!is_generated(name), "{name}");
    }
    assert!(is_generated("head.12.1a2b3c4d.docx") && is_generated("refs.0.00ff00ff.map"));
}

#[test]
fn every_single_byte_change_of_the_manifest_is_refused() {
    // H7. The digest line and the canonical form make this complete.
    let wc = with_files(&without(&opened(), "p1"), &[("document.md", &doc_md())]);
    let good = wc.complement.entry(MANIFEST).unwrap().unwrap().to_vec();
    assert!(Manifest::parse(&good).is_ok());
    let allowed = [
        ErrorCode::CorruptComplement,
        ErrorCode::UnsupportedVersion,
        ErrorCode::LimitExceeded,
    ];
    for at in 0..good.len() {
        for value in 0..=u8::MAX {
            if good[at] == value {
                continue;
            }
            let mut bad = good.clone();
            bad[at] = value;
            let e = Manifest::parse(&bad)
                .err()
                .unwrap_or_else(|| panic!("byte {at} -> {value} was accepted"));
            assert!(allowed.contains(&e.code()), "byte {at} -> {value}: {e}");
        }
    }
    // Cutting it short, and adding to it, too.
    for len in 0..good.len() {
        assert!(Manifest::parse(&good[..len]).is_err(), "cut at {len}");
    }
    let mut longer = good.clone();
    longer.push(b'\n');
    assert!(Manifest::parse(&longer).is_err());
}

#[test]
fn a_newer_version_is_unsupported_and_an_oversized_manifest_is_over_the_limit() {
    let good = String::from_utf8(
        opened()
            .export()
            .unwrap()
            .complement
            .entry(MANIFEST)
            .unwrap()
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    for newer in ["1.1", "1.9", "2.0", "10.0"] {
        let text = good.replacen("1.0", newer, 1);
        let e = Manifest::parse(text.as_bytes()).err().unwrap();
        assert_eq!(e.code(), ErrorCode::UnsupportedVersion, "{newer}: {e}");
    }
    let e = Manifest::parse(&vec![b'x'; MAX_MANIFEST_BYTES + 1])
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::LimitExceeded);
    for nonsense in [
        "",
        "hello\n",
        "stetdoc-complement\n",
        "stetdoc-complement one.two\n",
    ] {
        assert_eq!(
            Manifest::parse(nonsense.as_bytes()).err().unwrap().code(),
            ErrorCode::CorruptComplement,
            "{nonsense:?}"
        );
    }
}

#[test]
fn resume_refuses_what_the_manifest_does_not_vouch_for() {
    let doc = without(&opened(), "p1");
    let wc = doc.export().unwrap();
    let manifest = wc.manifest().unwrap();
    let options = OpenOptions::default();
    // A changed byte in each entry.
    for name in manifest.entry_names() {
        let mut bad = wc.clone();
        let mut bytes = bad.complement.entry(name).unwrap().unwrap().to_vec();
        let middle = bytes.len() / 2;
        bytes[middle] ^= 1;
        replace_entry(&mut bad, name, bytes);
        let e = Document::resume(&bad, &options)
            .err()
            .unwrap_or_else(|| panic!("{name} was accepted"));
        assert_eq!(e.code(), ErrorCode::CorruptComplement, "{name}: {e}");
    }
    // A missing entry, a missing manifest and a duplicate entry.
    for name in manifest.entry_names().into_iter().chain([MANIFEST]) {
        let mut bad = wc.clone();
        bad.complement.entries.retain(|(n, _)| n != name);
        assert_eq!(
            Document::resume(&bad, &options).err().unwrap().code(),
            ErrorCode::CorruptComplement,
            "{name}"
        );
    }
    let mut twice = wc.clone();
    twice
        .complement
        .entries
        .push(twice.complement.entries[0].clone());
    assert_eq!(
        Document::resume(&twice, &options).err().unwrap().code(),
        ErrorCode::CorruptComplement
    );
    // Entries the manifest does not name are ignored, and so are readable files.
    let mut extra = wc.clone();
    extra.complement.entries.push(("lock".to_string(), vec![1]));
    extra
        .files
        .push(("document.md".to_string(), "anything".to_string()));
    assert_eq!(
        Document::resume(&extra, &options).unwrap().base(),
        doc.base()
    );
}

#[test]
fn resume_meets_the_limits_before_it_reads() {
    // H9, the complement side.
    let wc = without(&opened(), "p1").export().unwrap();
    let small = OpenOptions {
        limits: crate::api::limits::Limits {
            max_archive_bytes: 100,
            ..crate::api::limits::Limits::default()
        },
        ..OpenOptions::default()
    };
    assert_eq!(
        Document::resume(&wc, &small).err().unwrap().code(),
        ErrorCode::LimitExceeded
    );
    let few = OpenOptions {
        limits: crate::api::limits::Limits {
            max_entry_bytes: 4,
            ..crate::api::limits::Limits::default()
        },
        ..OpenOptions::default()
    };
    assert_eq!(
        Document::resume(&wc, &few).err().unwrap().code(),
        ErrorCode::LimitExceeded
    );
}

#[test]
fn a_manifest_of_another_format_is_unsupported_version() {
    let wc = opened().export().unwrap();
    let mut manifest = wc.manifest().unwrap();
    manifest.format = "stet/2".to_string();
    let mut other = wc.clone();
    replace_entry(&mut other, MANIFEST, manifest.render().into_bytes());
    let e = Document::resume(&other, &OpenOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::UnsupportedVersion, "{e}");
}

#[test]
fn nothing_is_pending_until_a_file_changes() {
    let doc = opened();
    let mut wc = with_files(&doc, &[("document.md", &doc_md()), ("notes.md", "notes\n")]);
    assert_eq!(wc.pending_files().unwrap(), None);
    wc.ensure_clean().unwrap();
    // A hand edit to one file: only that file is pending, with the manifest's hash of what was written.
    wc.files[0].1.push_str("an edit\n");
    let pending = wc.pending_files().unwrap().unwrap();
    assert_eq!(pending.base, doc.base());
    assert_eq!(pending.format.as_str(), "stet/1");
    assert_eq!(
        pending.files,
        vec![("document.md".to_string(), format!("{}an edit\n", doc_md()))]
    );
    assert_eq!(
        pending.rendered,
        vec![("document.md".to_string(), hex(&sha256(doc_md().as_bytes())))]
    );
    let e = wc.ensure_clean().err().unwrap();
    assert_eq!(e.code(), ErrorCode::PendingEdits);
    assert!(
        e.message().contains("`document.md`") && !e.message().contains("notes.md"),
        "{e}"
    );
    // Putting the text back makes it clean again, and a trailing newline is an edit like any other.
    wc.files[0].1 = doc_md();
    assert_eq!(wc.pending_files().unwrap(), None);
    wc.files[0].1.pop();
    assert!(wc.pending_files().unwrap().is_some());
}

#[test]
fn a_file_that_was_written_and_is_missing_and_a_file_that_was_never_written_are_refused() {
    let doc = opened();
    let mut wc = with_files(&doc, &[("document.md", &doc_md())]);
    wc.files.clear();
    let e = wc.pending_files().err().unwrap();
    assert_eq!(e.code(), ErrorCode::InvalidPayload);
    assert!(
        e.message()
            .contains("`document.md` was written and is now missing"),
        "{e}"
    );
    // Never written: the message says what to do instead.
    for (name, hint) in [
        ("notes.md", "footnote"),
        ("headers.md", "header"),
        ("comments.md", "`comment`"),
        ("appendix.md", "dropped"),
    ] {
        let mut wc = with_files(&doc, &[("document.md", &doc_md())]);
        wc.files.push((name.to_string(), "text".to_string()));
        let e = wc.pending_files().err().unwrap();
        assert_eq!(e.code(), ErrorCode::Unsupported, "{name}");
        assert!(
            e.message().contains(hint) && e.message().contains(name),
            "{name}: {e}"
        );
        assert_eq!(
            wc.ensure_clean().err().unwrap().code(),
            ErrorCode::Unsupported
        );
    }
    // Two files of one name are refused, and a working copy without a manifest has nothing to compare with.
    let mut twice = with_files(&doc, &[("document.md", &doc_md())]);
    twice
        .files
        .push(("document.md".to_string(), "again".to_string()));
    assert_eq!(
        twice.pending_files().err().unwrap().code(),
        ErrorCode::InvalidPayload
    );
    let bare = WorkingCopy::default();
    assert_eq!(
        bare.pending_files().err().unwrap().code(),
        ErrorCode::CorruptComplement
    );
}

#[test]
fn only_a_plain_name_can_be_the_name_of_a_readable_file() {
    let doc = opened();
    for bad in [
        "",
        ".hidden.md",
        "two words.md",
        "a/b.md",
        "a\\b.md",
        "line\nbreak.md",
    ] {
        let e = doc
            .export_with(vec![(bad.to_string(), "x".to_string())])
            .err()
            .unwrap();
        assert_eq!(e.code(), ErrorCode::Internal, "{bad:?}");
    }
    let e = doc
        .export_with(vec![
            ("a.md".to_string(), "1".to_string()),
            ("a.md".to_string(), "2".to_string()),
        ])
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::Internal);
}
