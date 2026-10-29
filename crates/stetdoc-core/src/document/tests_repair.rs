//! The repair of prefixes that a markup-compatibility attribute names and nothing declares (decision 0014). Word refuses
//! such a file and the schema allows it, so `check` reports it as a violation that was already there, and every write
//! declares a prefix Word knows, drops one nobody knows, and changes nothing else.

use super::tests_edit::*;
use super::*;
use crate::api::check::FindingCode;
use crate::api::edit::Edit;
use crate::api::options::Markup;
use crate::api::view::WarningCode;
use stetdoc_testkit::fixtures::parts::{self, DECL, Doc, NS_W};
use stetdoc_testkit::fixtures::zip::{Entry, Zip};

const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
const WP14: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing";
const BODY: &str = "<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p><w:p><w:r><w:t>The law of New York governs.</w:t></w:r></w:p>";

/// The root start tag of the defective main part: `w14`, `wp14` and `zz` are listed and never declared, as python-docx
/// leaves a template's root when it rewrites the body (the Harvey indenture of the word-open bug).
fn root_attrs() -> String {
    format!(" xmlns:mc=\"{MC}\" xmlns:x=\"urn:x\" mc:Ignorable=\"w14 wp14 zz x\"")
}

fn main_part() -> String {
    Doc::new().root_attrs(&root_attrs()).body(BODY).xml()
}

/// The defective main part, and a settings part that the edit never touches with the same defect.
fn package() -> Vec<u8> {
    let rels = format!(
        "{DECL}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings\" Target=\"settings.xml\"/></Relationships>"
    );
    let mut entries = parts::entries_doc(main_part().as_bytes());
    entries.push(Entry::new("word/_rels/document.xml.rels", rels));
    entries.push(Entry::new("word/settings.xml", settings()));
    Zip::new(entries).build()
}

fn settings() -> String {
    format!(
        "{DECL}<w:settings xmlns:w=\"{NS_W}\" xmlns:mc=\"{MC}\" mc:Ignorable=\"w14 w15\"><w:zoom w:percent=\"100\"/></w:settings>"
    )
}

fn part(doc: &Document, name: &str) -> String {
    let saved = doc.save().unwrap();
    let reopened = Document::open(saved.bytes, &OpenOptions::default()).unwrap();
    let view = reopened.state.parts();
    let id = view.id(name).unwrap();
    String::from_utf8(view.tree(id).unwrap().serialize()).unwrap()
}

fn undeclared(doc: &Document) -> Vec<(String, String)> {
    doc.check()
        .findings
        .into_iter()
        .filter(|f| f.code == FindingCode::McPrefixUndeclared)
        .map(|f| (f.part, f.message))
        .collect()
}

#[test]
fn check_reports_every_undeclared_prefix_as_a_violation_the_input_has() {
    let doc = Document::open(package(), &OpenOptions::default()).unwrap();
    let found = undeclared(&doc);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].0, "word/document.xml");
    assert!(
        found[0].1.starts_with("mc:Ignorable names w14, wp14, zz, which no namespace declaration binds, so Word will not open the file."),
        "{found:?}"
    );
    assert_eq!(found[1].0, "word/settings.xml");
    assert!(!doc.check().ok);
    // Reading changes nothing: an unedited document still saves its input bytes (L1).
    assert_eq!(doc.save().unwrap().bytes, package());
}

#[test]
fn an_edit_declares_the_known_prefixes_drops_the_unknown_one_and_changes_nothing_else() {
    let mut doc = Document::open(package(), &OpenOptions::default()).unwrap();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p1","find":"thirty","with":"sixty"}"#,
    );
    let receipt = doc
        .edit(Edit::ops(b, stamp()).markup(Markup::Manual))
        .unwrap();

    // The root gains the two declarations after its last one, and `zz`, which nobody knows, leaves mc:Ignorable.
    let before = main_part();
    let after = part(&doc, "word/document.xml");
    let old_root = format!("<w:document xmlns:w=\"{NS_W}\"{}>", root_attrs());
    let new_root = format!(
        "<w:document xmlns:w=\"{NS_W}\" xmlns:mc=\"{MC}\" xmlns:x=\"urn:x\" xmlns:w14=\"{W14}\" xmlns:wp14=\"{WP14}\" mc:Ignorable=\"w14 wp14 x\">"
    );
    assert!(after.starts_with(&format!("{DECL}{new_root}")), "{after}");
    // Apart from the root start tag and the edited paragraph, every byte is the input's.
    let p1 = "<w:p><w:r><w:t>Payment is due within thirty days.</w:t></w:r></w:p>";
    let marked = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">Payment is due within </w:t></w:r>"#,
        r#"<w:r><w:rPr><w:strike/></w:rPr><w:t>thirty</w:t></w:r>"#,
        r#"<w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>sixty</w:t></w:r>"#,
        r#"<w:r><w:t xml:space="preserve"> days.</w:t></w:r></w:p>"#,
    );
    assert_eq!(
        after,
        before.replace(&old_root, &new_root).replace(p1, marked)
    );
    assert!(before.contains(&old_root));

    // The settings part, which the edit did not touch, is repaired too, and nothing else in it changes.
    let settings_after = part(&doc, "word/settings.xml");
    assert_eq!(
        settings_after,
        settings().replace(
            &format!("xmlns:mc=\"{MC}\""),
            &format!(
                "xmlns:mc=\"{MC}\" xmlns:w14=\"{W14}\" xmlns:w15=\"http://schemas.microsoft.com/office/word/2012/wordml\""
            )
        )
    );

    // The receipt says what was repaired, one warning for each part.
    let repaired: Vec<&str> = receipt
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::Repaired)
        .map(|w| w.message.as_str())
        .collect();
    assert_eq!(repaired.len(), 2, "{:?}", receipt.warnings);
    assert!(
        repaired[0].starts_with(
            "declared w14, wp14 and removed the unknown zz from the markup-compatibility attributes in word/document.xml so Word can open the file."
        ),
        "{repaired:?}"
    );
    assert!(
        receipt.text.contains(
            "warning repaired: declared w14, w15 in word/settings.xml so Word can open the file."
        ),
        "{}",
        receipt.text
    );
    // And the defect is gone.
    assert!(undeclared(&doc).is_empty(), "{:?}", undeclared(&doc));
}

#[test]
fn a_dry_run_shows_the_repair_and_changes_nothing() {
    let mut doc = Document::open(package(), &OpenOptions::default()).unwrap();
    let b = batch(
        &doc,
        r#"{"op":"replace","ref":"p1","find":"thirty","with":"sixty"}"#,
    );
    let receipt = doc.edit(Edit::ops(b, stamp()).dry_run()).unwrap();
    assert!(
        receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::Repaired),
        "{:?}",
        receipt.warnings
    );
    assert_eq!(doc.save().unwrap().bytes, package());
}

#[test]
fn a_document_without_the_defect_gets_no_repair() {
    let mut doc = rich();
    let receipt = run(
        &mut doc,
        r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#,
    );
    assert!(
        receipt
            .warnings
            .iter()
            .all(|w| w.code != WarningCode::Repaired),
        "{:?}",
        receipt.warnings
    );
}
