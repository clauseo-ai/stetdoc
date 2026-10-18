//! The public handle: it is `Send`, what is not a package is refused, and the capabilities and the schema are whole.

use super::*;
use crate::api::capabilities::Capabilities;
use crate::api::error::ErrorCode;
use crate::api::op_spec::OPS;
use crate::api::options::CompareOptions;
use crate::refs::Claim;
use stetdoc_testkit::fixtures::{parts, zip::Zip};

fn assert_send<T: Send>() {}

#[test]
fn a_document_is_send() {
    // H10, at compile time.
    assert_send::<Document>();
}

#[test]
fn what_is_not_a_package_is_refused_by_open_and_by_compare() {
    let options = CompareOptions {
        limits: crate::api::limits::Limits::default(),
        stamp: crate::api::stamp::Stamp::new("Clauseo", "2026-10-06T10:00:00"),
        format: crate::api::view::FormatId::stet(),
    };
    let e = crate::document::compare(Vec::new(), Vec::new(), &options)
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput, "{e}");
    let e = Document::open(Vec::new(), &OpenOptions::default())
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::MalformedInput);
}

#[test]
fn capabilities_list_what_the_tables_hold() {
    let caps = capabilities();
    assert_eq!(caps.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(caps.complement_version, "1.0");
    assert_eq!(caps.formats.len(), 1);
    assert_eq!(
        (
            caps.formats[0].id.as_str(),
            caps.formats[0].extension.as_str()
        ),
        ("stet/1", "md")
    );
    let names: Vec<&str> = caps.ops.iter().map(|o| o.name.as_str()).collect();
    let table: Vec<&str> = OPS.iter().map(|o| o.name).collect();
    assert_eq!(names, table);
    assert_eq!(caps.ops.len(), 19);
    let replace = &caps.ops[0];
    assert!(replace.trackable && replace.reversible);
    assert!(
        replace
            .fields
            .iter()
            .any(|f| f.name == "with" && f.required && f.kind == "text")
    );
    let comment = caps.ops.iter().find(|o| o.name == "comment").unwrap();
    assert!(!comment.trackable);
    assert_eq!(caps.error_codes.len(), ErrorCode::ALL.len());
    let stale = caps
        .error_codes
        .iter()
        .find(|e| e.code == "stale_ref")
        .unwrap();
    assert_eq!(stale.exit, 4);
    assert!(!stale.next.is_empty());
    assert_eq!(caps.limits, crate::api::limits::Limits::default());
    assert!(caps.warning_codes.iter().any(|w| w == "quotes_curled"));
    // It is data: it survives a round trip through JSON.
    let json = serde_json::to_string(&caps).unwrap();
    assert_eq!(serde_json::from_str::<Capabilities>(&json).unwrap(), caps);
}

#[test]
fn the_schema_describes_every_public_type() {
    let schema: serde_json::Value = serde_json::from_str(&json_schema()).unwrap();
    let defs = schema["$defs"].as_object().expect("definitions");
    for name in [
        "Batch",
        "Op",
        "Receipt",
        "CheckResult",
        "ViewSpec",
        "View",
        "Lookup",
        "Inspection",
        "Capabilities",
        "Limits",
        "Stamp",
        "StetError",
    ] {
        assert!(defs.contains_key(name), "no schema for {name}");
    }
    // The wire names are in it: a code is its snake_case name, an operation is hyphenated.
    let text = json_schema();
    assert!(text.contains("\"stale_ref\""));
    assert!(text.contains("\"edit-comment\""));
}

#[test]
fn an_error_is_data_with_a_code_a_message_and_where_it_came_from() {
    let e = StetError::new(ErrorCode::InvalidPayload, "bad")
        .at_op(2)
        .at_line("document.md", 14);
    let json: serde_json::Value = serde_json::to_value(&e).unwrap();
    assert_eq!(json["code"], "invalid_payload");
    assert_eq!(json["op_index"], 2);
    assert_eq!(json["file"], "document.md");
    assert_eq!(json["line"], 14);
    assert!(e.to_string().contains("document.md line 14"), "{e}");
}

/// Four paragraphs, so that one can go and three remain.
const BODY: &str = "<w:p><w:r><w:t>one</w:t></w:r></w:p><w:p><w:r><w:t>two</w:t></w:r></w:p><w:p><w:r><w:t>three</w:t></w:r></w:p><w:p><w:r><w:t>four</w:t></w:r></w:p>";

/// The bytes of a small package.
pub(crate) fn package() -> Vec<u8> {
    let doc = parts::Doc::new().body(BODY).bytes();
    Zip::new(parts::entries_doc(&doc)).build()
}

/// A document of a different package: another namespace.
pub(crate) fn other() -> Document {
    let doc = parts::Doc::new()
        .body("<w:p><w:r><w:t>another</w:t></w:r></w:p>")
        .bytes();
    let bytes = Zip::new(parts::entries_doc(&doc)).build();
    Document::open(bytes, &OpenOptions::default()).unwrap()
}

/// An unedited document of that package.
pub(crate) fn opened() -> Document {
    Document::open(package(), &OpenOptions::default()).unwrap()
}

/// The document one version later, with the paragraph `token` removed: a stand-in for an edit until `edit` lands, with the real
/// transaction and commit, so its refs, head and token are real.
pub(crate) fn without(doc: &Document, token: &str) -> Document {
    let key = doc.state.resolve(Claim::Implied, token).unwrap();
    let mut tx = doc.state.begin();
    tx.part_mut("word/document.xml")
        .unwrap()
        .edit()
        .detach(key.node())
        .unwrap();
    let (state, _) = tx.commit().unwrap();
    let original = match &doc.original {
        Original::Opened => doc.state.opened_source(),
        Original::Kept(original) => original.clone(),
    };
    Document {
        state,
        limits: doc.limits.clone(),
        format: doc.format.clone(),
        original: Original::Kept(original),
    }
}

#[test]
fn an_unedited_document_saves_the_bytes_it_was_opened_with() {
    // L1, through the handle.
    let bytes = package();
    let doc = Document::open(bytes.clone(), &OpenOptions::default()).unwrap();
    let saved = doc.save().unwrap();
    assert_eq!(saved.bytes, bytes);
    assert!(!saved.receipt.edited);
    assert_eq!(saved.receipt.base, doc.base());
    assert_eq!(saved.receipt.sha256.len(), 64);
}

#[test]
fn an_edited_document_says_so_and_saves_its_own_bytes() {
    let one = opened();
    let two = without(&one, "p1");
    let saved = two.save().unwrap();
    assert!(saved.receipt.edited);
    assert_ne!(saved.bytes, package());
    assert_ne!(two.base(), one.base());
    assert!(two.base().contains(":1:"), "{}", two.base());
}

#[test]
fn a_format_that_has_no_readable_files_is_refused_at_open() {
    let options = OpenOptions {
        format: FormatId::new("json/1"),
        ..OpenOptions::default()
    };
    let e = Document::open(package(), &options).err().unwrap();
    assert_eq!(e.code(), ErrorCode::Unsupported);
}

#[test]
fn export_resume_export_is_stable_and_resume_continues_the_namespace() {
    // L4, for a document at version 0 and at version 1.
    for doc in [opened(), without(&opened(), "p2")] {
        let first = doc.export().unwrap();
        let back = Document::resume(&first, &OpenOptions::default()).unwrap();
        assert_eq!(back.base(), doc.base());
        assert_eq!(back.export().unwrap(), first);
        assert_eq!(back.save().unwrap().bytes, doc.save().unwrap().bytes);
        // The unedited head is not stored twice: the manifest says so.
        let manifest = first.manifest().unwrap();
        assert_eq!(manifest.head.is_none(), !doc.save().unwrap().receipt.edited);
    }
}

#[test]
fn a_resumed_document_that_is_unedited_saves_the_original_bytes() {
    let wc = opened().export().unwrap();
    let back = Document::resume(&wc, &OpenOptions::default()).unwrap();
    let saved = back.save().unwrap();
    assert_eq!(saved.bytes, package());
    assert!(!saved.receipt.edited);
}

#[test]
fn a_batch_written_before_an_export_still_names_the_base_after_it() {
    let doc = without(&opened(), "p1");
    let wc = doc.export().unwrap();
    let back = Document::resume(&wc, &OpenOptions::default()).unwrap();
    assert_eq!(back.base(), doc.base());
    // The resumed document continues from the same state: one more edit, and the tokens agree with an unbroken session.
    let unbroken = without(&doc, "p2");
    let resumed = without(&back, "p2");
    assert_eq!(resumed.base(), unbroken.base());
    assert_eq!(resumed.export().unwrap(), unbroken.export().unwrap());
}

#[test]
fn a_document_that_was_edited_and_did_not_keep_its_original_cannot_save_or_export() {
    // The check that step 3's `edit` must keep the original before it installs a state.
    let mut two = without(&opened(), "p1");
    two.original = Original::Opened;
    assert_eq!(two.save().err().unwrap().code(), ErrorCode::Internal);
    assert_eq!(two.export().err().unwrap().code(), ErrorCode::Internal);
}
