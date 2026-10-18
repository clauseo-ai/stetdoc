//! Step 8 through `compare`: the redline through `check`, the receipt with its moves and warnings, and the foreign origins that
//! let a problem of the original's content, definitions or parts be carried and not refused (design `compare.md` 2.2 and 7.7).

use super::tests_edit::stamp;
use super::*;
use crate::api::edit::Edit;
use crate::api::limits::Limits;
use crate::api::options::CompareOptions;
use crate::api::revision::ChangeKind;
use crate::api::view::{Mode, ViewSpec, WarningCode};
use crate::threads::tests_support::Pkg;
use crate::wordml::BlockKey;

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

fn long(n: usize) -> String {
    format!(
        "Clause {n} says that the supplier shall deliver the goods within thirty days of the order and keep records of it"
    )
}

fn options() -> CompareOptions {
    CompareOptions {
        limits: Limits::default(),
        stamp: stamp(),
        format: crate::api::view::FormatId::stet(),
    }
}

/// The text of the body in a mode, one line a paragraph.
fn text(doc: &Document, mode: Mode) -> String {
    let spec = ViewSpec {
        mode,
        ..ViewSpec::default()
    };
    doc.view(&spec).unwrap().text
}

/// The body after accepting, or rejecting, every revision.
fn resolved(doc: &Document, op: &str) -> Document {
    let mut copy = Document::open(doc.save().unwrap().bytes, &OpenOptions::default()).unwrap();
    let b = super::tests_edit::batch(&copy, &format!(r#"{{"op":"{op}","all":true}}"#));
    copy.edit(Edit::ops(b, stamp())).unwrap();
    copy
}

#[test]
fn a_redline_accepts_to_the_revised_document_and_rejects_to_the_original() {
    let a = Pkg::new(&format!("{}{}{}", p(&long(1)), p(&long(2)), p(&long(3)))).bytes();
    let b = Pkg::new(&format!(
        "{}{}{}",
        p(&long(1).replace("thirty", "sixty")),
        p(&long(3)),
        p("A clause that is new.")
    ))
    .bytes();
    let compared = compare(a.clone(), b.clone(), &options()).unwrap();
    let r = &compared.receipt;
    assert!(r.operations.is_empty() && r.tracked && !r.dry_run);
    assert_eq!(r.revisions.len(), 1, "{:?}", r.revisions);
    assert_eq!(r.revisions[0].author, "Clauseo");
    let kinds: Vec<ChangeKind> = r.revisions[0].counts.iter().map(|(k, _)| *k).collect();
    assert!(kinds.contains(&ChangeKind::Insertion) && kinds.contains(&ChangeKind::Deletion));
    assert!(
        r.validation.units > 0,
        "the redline was checked: {:?}",
        r.validation
    );
    assert!(r.text.starts_with("compared: "), "{}", r.text);
    assert_eq!(r.base, compared.document.base());
    let doc = compared.document;
    assert!(doc.check().ok, "{:?}", doc.check());
    let revised = Document::open(b.clone(), &OpenOptions::default()).unwrap();
    let original = Document::open(a, &OpenOptions::default()).unwrap();
    let body = |d: &Document| -> Vec<String> {
        text(d, Mode::Final)
            .lines()
            .filter_map(|l| l.split_once(' ').map(|(_, t)| t.to_string()))
            .filter(|t| t.starts_with("Clause") || t.starts_with("A clause"))
            .collect()
    };
    assert_eq!(body(&resolved(&doc, "accept")), body(&revised));
    assert_eq!(body(&resolved(&doc, "reject")), body(&original));
    // The redline saves, and its working copy keeps the revised input as its original.
    let copy = doc.export().unwrap();
    assert!(
        copy.complement.entries.iter().any(|(_, bytes)| *bytes == b),
        "the revised input is the original of the working copy"
    );
}

#[test]
fn a_document_compared_with_itself_has_no_revision_and_saves_as_it_was() {
    let a = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(2)))).bytes();
    let compared = compare(a.clone(), a.clone(), &options()).unwrap();
    assert!(compared.receipt.revisions.is_empty());
    assert!(compared.receipt.moves.is_empty());
    assert_eq!(compared.document.save().unwrap().bytes, a);
}

#[test]
fn a_moved_paragraph_is_listed_as_a_move_with_its_refs() {
    let ps: Vec<String> = (1..=6).map(|n| p(&long(n))).collect();
    let a = Pkg::new(&ps.concat()).bytes();
    let order = [0, 2, 3, 4, 1, 5];
    let b = Pkg::new(&order.iter().map(|&i| ps[i].clone()).collect::<String>()).bytes();
    let compared = compare(a, b, &options()).unwrap();
    let moves = &compared.receipt.moves;
    assert_eq!(moves.len(), 1, "{:?}", compared.receipt);
    let m = &moves[0];
    assert!(
        m.from.starts_with('p') && m.to.starts_with('p') && m.from != m.to,
        "{m:?}"
    );
    assert!(m.text.starts_with("Clause 2"), "{m:?}");
    assert!(
        compared
            .receipt
            .text
            .contains(&format!("moved {} to {}", m.from, m.to))
    );
}

#[test]
fn a_problem_of_the_originals_content_is_carried_into_the_redline() {
    // The original's second paragraph has its run properties out of order, and the revised document deletes it: the redline holds a
    // deleted copy with the problem, which was already there.
    let faulty = format!(
        "<w:p><w:r><w:rPr><w:i/><w:b/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        long(2)
    );
    let a = Pkg::new(&format!("{}{faulty}{}", p(&long(1)), p(&long(3)))).bytes();
    let b = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    let compared = compare(a, b, &options()).unwrap();
    let v = &compared.receipt.validation;
    assert!(v.carried_total >= 1, "{v:?}");
}

const NORMAL: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;

/// A style whose `w:basedOn` comes before its `w:name`, out of the schema's order.
const MISORDERED: &str = r#"<w:style w:type="paragraph" w:styleId="Quote"><w:basedOn w:val="Normal"/><w:name w:val="Quote"/></w:style>"#;

#[test]
fn an_imported_definition_and_a_copied_part_keep_the_problems_they_had() {
    // The deleted paragraph uses a style that only the original has, and the style's definition is out of order.
    let quoted = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Quote\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        long(2)
    );
    let a = Pkg::new(&format!("{}{quoted}{}", p(&long(1)), p(&long(3))))
        .styles(&format!("{NORMAL}{MISORDERED}"))
        .bytes();
    // The revised document has a style sheet without it: the definition is appended to it.
    let b = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3))))
        .styles(NORMAL)
        .bytes();
    let compared = compare(a.clone(), b, &options()).unwrap();
    assert!(
        compared.receipt.validation.carried_total >= 1,
        "{:?}",
        compared.receipt.validation
    );
    // The revised document has no style sheet at all: the part is added for the definition.
    let bare = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    let compared = compare(a, bare, &options()).unwrap();
    assert!(!compared.receipt.revisions.is_empty());
}

#[test]
fn compare_warns_of_the_revisions_it_accepted_first_and_checks_its_stamp() {
    let a = Pkg::new(&format!(
        "<w:p><w:ins w:id=\"9\" w:author=\"Ann\" w:date=\"2026-01-01T00:00:00Z\"><w:r><w:t>{}</w:t></w:r></w:ins></w:p>",
        long(1)
    ))
    .bytes();
    let b = Pkg::new(&p(&long(2))).bytes();
    let compared = compare(a.clone(), b.clone(), &options()).unwrap();
    assert!(
        compared
            .receipt
            .warnings
            .iter()
            .any(|w| w.code == WarningCode::CompareRevisionsAccepted),
        "{:?}",
        compared.receipt.warnings
    );
    let mut bad = options();
    bad.stamp = crate::api::stamp::Stamp::new("", "2026-10-06T10:00:00");
    assert_eq!(
        compare(a, b, &bad).err().unwrap().code(),
        ErrorCode::InvalidPayload
    );
}

#[test]
fn a_problem_inside_a_table_the_original_had_is_carried_with_the_table() {
    // The original's table holds a nested table without its required properties, and a paragraph with its run properties out
    // of order; the revised document deletes the table, so the redline imports a deleted copy of it. Each paragraph and table
    // inside it is a unit of its own, and each keeps the problems its unit had.
    let table = format!(
        concat!(
            "<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid>",
            "<w:tr><w:tc><w:p><w:r><w:rPr><w:i/><w:b/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            "<w:tbl><w:tblGrid><w:gridCol w:w=\"900\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
            "<w:p/></w:tc></w:tr></w:tbl>"
        ),
        long(2)
    );
    let a = Pkg::new(&format!("{}{table}{}", p(&long(1)), p(&long(3)))).bytes();
    let b = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    assert!(
        !Document::open(a.clone(), &OpenOptions::default())
            .unwrap()
            .check()
            .ok
    );
    let compared = compare(a, b, &options()).unwrap();
    let v = &compared.receipt.validation;
    assert!(v.carried_total >= 2, "{v:?}");
}

#[test]
fn each_block_compare_could_not_redline_is_named_once_with_its_ref() {
    // An inserted paragraph that holds an equation stays when its insertion is rejected: compare says so for that block.
    let math = format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{} </w:t></w:r><m:oMath xmlns:m=\"http://schemas.openxmlformats.org/officeDocument/2006/math\"><m:r><m:t>x</m:t></m:r></m:oMath></w:p>",
        long(2)
    );
    let a = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    let b = Pkg::new(&format!("{}{math}{}", p(&long(1)), p(&long(3)))).bytes();
    let compared = compare(a, b, &options()).unwrap();
    let r = &compared.receipt;
    let named: Vec<&crate::api::view::Warning> = r
        .warnings
        .iter()
        .filter(|w| {
            matches!(
                w.code,
                WarningCode::CompareNotRedlined | WarningCode::CompareNotRestored
            )
        })
        .collect();
    assert_eq!(named.len(), 1, "{:?}", r.warnings);
    assert_eq!(named[0].code, WarningCode::CompareNotRestored);
    let at = named[0].block.as_deref().expect("a ref");
    assert!(at.starts_with('p'), "{at}");
    assert!(
        r.text.contains(&format!(
            "warning compare_not_restored {at}: rejecting the changes"
        )),
        "{}",
        r.text
    );
}

/// The line of a block in the redline's view, by its ref.
fn line_of(doc: &Document, at: &str) -> String {
    text(doc, Mode::Redline)
        .lines()
        .find(|l| l.split_once(' ').is_some_and(|(r, _)| r == at))
        .map(str::to_string)
        .unwrap_or_default()
}

#[test]
fn a_block_of_the_original_that_cannot_be_copied_is_named_by_the_block_its_copy_would_stand_next_to()
 {
    // No document makes the import fail, so a test makes the copy of the deleted paragraph fail.
    let a = Pkg::new(&format!("{}{}{}", p(&long(1)), p(&long(2)), p(&long(3)))).bytes();
    let b = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    crate::compare::failpoint::arm("Clause 2 says");
    let compared = compare(a, b, &options());
    crate::compare::failpoint::disarm();
    let compared = compared.unwrap();
    let r = &compared.receipt;
    let named: Vec<&crate::api::view::Warning> = r
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::CompareNotRestored)
        .collect();
    assert_eq!(named.len(), 1, "{:?}", r.warnings);
    let at = named[0].block.as_deref().expect("the block beside it");
    // The ref is a block of the redline, a neighbour of the paragraph that was deleted, and never a key of the original read
    // in the redline's tree.
    let line = line_of(&compared.document, at);
    assert!(
        line.contains("Clause 1 says") || line.contains("Clause 3 says"),
        "{at} is {line:?}"
    );
    assert!(
        named[0].message.contains(
            "the original's block that starts \"Clause 2 says that the supplier shall deliver...\""
        ),
        "{}",
        named[0].message
    );
    assert!(
        named[0]
            .message
            .contains(&format!("whose copy would have stood next to {at}")),
        "{}",
        named[0].message
    );
}

#[test]
fn an_exclusion_without_a_block_of_the_redline_names_no_ref_and_quotes_the_original() {
    use crate::compare::{Exclusion, Site};
    // A key of the original: a node of its tree.
    let state = Pkg::new(&p(&long(1))).open();
    let view = state.parts();
    let main = "word/document.xml";
    let node = view.tree(view.id(main).unwrap()).unwrap().root().id();
    let original = BlockKey::new(std::sync::Arc::from(main), node);
    let exclusion = |site: Site, text: &str| Exclusion {
        site,
        text: text.to_string(),
        kind: "not_restored",
        reason: "a note of deleted content was not added: no room.".to_string(),
        redline: Vec::new(),
        copies: Vec::new(),
        original: vec![original.clone()],
    };
    // Whatever the redline's refs would make of the key, an exclusion of the original's block alone asks them nothing.
    let asked = std::cell::Cell::new(0);
    let shown = |_: &BlockKey| {
        asked.set(asked.get() + 1);
        Some("p9".to_string())
    };
    let w =
        super::compare::exclusion_warning(&exclusion(Site::Original, "The old note says"), &shown);
    assert_eq!(asked.get(), 0);
    assert_eq!(w.code, WarningCode::CompareNotRestored);
    assert_eq!(w.block, None);
    assert_eq!(
        w.message,
        "rejecting the changes does not give back the original's block that starts \"The old note says\": a note of deleted content was not added: no room."
    );
    let w = super::compare::exclusion_warning(&exclusion(Site::Original, ""), &shown);
    assert!(
        w.message
            .contains("a block of the original that has no text"),
        "{}",
        w.message
    );
    // A block of the redline that has no ref is not named by a made-up one.
    let w =
        super::compare::exclusion_warning(&exclusion(Site::Block(original.clone()), ""), &|_| None);
    assert_eq!(w.block, None);
}

#[test]
fn a_new_violation_in_a_part_that_compare_added_is_refused() {
    // The revised document has no style sheet, so compare adds one for the original's style, whose definition is out of order.
    let quoted = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Quote\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        long(2)
    );
    let a = Pkg::new(&format!("{}{quoted}{}", p(&long(1)), p(&long(3))))
        .styles(&format!("{NORMAL}{MISORDERED}"))
        .bytes();
    let b = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    let stamp = crate::revisions::stamp::RevStamp::new("Clauseo", "2026-10-06T10:00:00").unwrap();
    let prepared = crate::compare::prepare(a, b, &Limits::default(), &stamp).unwrap();
    let crate::compare::Redline { mut tx, receipt } =
        crate::compare::redline(&prepared, &stamp).unwrap();
    let told = super::compare::declared_of(&prepared, &tx, &receipt).unwrap();
    const STYLES: &str = "word/styles.xml";
    assert!(tx.base().parts().id(STYLES).is_none() && tx.parts().id(STYLES).is_some());
    // What the original's style sheet had is carried.
    let verified = crate::check::edit_located(&tx, &told.declared)
        .map_err(|r| r.error)
        .unwrap();
    assert!(verified.carried_total >= 1, "{verified:?}");
    // A style with an element the schema does not know, which no source of the part had, is new.
    {
        let tree = tx.part_mut(STYLES).unwrap();
        let root = tree.root().id();
        let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
        let style = tree.edit().create_element(w, "style").unwrap();
        tree.edit().set_attr(style, w, "styleId", "Made").unwrap();
        let name = tree.edit().create_element(w, "name").unwrap();
        tree.edit().set_attr(name, w, "val", "Made").unwrap();
        tree.edit().append_child(style, name).unwrap();
        let bogus = tree.edit().create_element(w, "bogus").unwrap();
        tree.edit().append_child(style, bogus).unwrap();
        tree.edit().append_child(root, style).unwrap();
    }
    let refusal = crate::check::edit_located(&tx, &told.declared).expect_err("refused");
    assert_eq!(
        refusal.error.code(),
        ErrorCode::ValidationFailed,
        "{}",
        refusal.error
    );
    assert!(
        refusal.error.message().contains("bogus"),
        "{}",
        refusal.error
    );
}

#[test]
fn a_copy_whose_paragraphs_do_not_pair_with_the_originals_is_named_in_a_warning() {
    // The deleted paragraph holds a picture with a text box, and the picture's image cannot be carried, so the copy has one
    // paragraph where the original had two: each unit of the copy may keep what either had, and the receipt says so.
    let boxed = format!(
        concat!(
            "<w:p><w:r><w:t xml:space=\"preserve\">{} </w:t></w:r><w:r><w:pict xmlns:v=\"urn:schemas-microsoft-com:vml\" ",
            "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><v:shape><v:imagedata r:id=\"rId99\"/>",
            "<v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r></w:p>"
        ),
        long(2)
    );
    let a = Pkg::new(&format!("{}{boxed}{}", p(&long(1)), p(&long(3)))).bytes();
    let b = Pkg::new(&format!("{}{}", p(&long(1)), p(&long(3)))).bytes();
    let compared = compare(a, b, &options()).unwrap();
    let r = &compared.receipt;
    let merged: Vec<&crate::api::view::Warning> = r
        .warnings
        .iter()
        .filter(|w| w.code == WarningCode::CompareKeysMerged)
        .collect();
    assert_eq!(merged.len(), 1, "{:?}", r.warnings);
    let at = merged[0].block.as_deref().expect("a ref");
    assert!(
        line_of(&compared.document, at).contains("Clause 2 says"),
        "{at}"
    );
    assert!(
        r.text
            .contains(&format!("warning compare_keys_merged {at}: ")),
        "{}",
        r.text
    );
}
