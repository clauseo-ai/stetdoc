//! H4 for what no block holds: the body's last section properties and what lies between blocks. A scope never reaches them
//! through `accept` or `reject`, and the second pass refuses a change to them that nobody declared.

use super::tests_edit::*;
use super::tests_tamper::arm;
use super::*;
use crate::api::edit::Edit;
use crate::refs::Claim;
use stetdoc_testkit::fixtures::{contract, parts, zip::Zip};

const MAIN: &str = "word/document.xml";

/// The contract with a tracked change of its page size in the body's own section properties.
fn with_section_change() -> Document {
    let xml = String::from_utf8(parts::Doc::new().body(contract::BODY).bytes()).unwrap();
    let sect = concat!(
        r#"<w:sectPr><w:pgSz w:w="12240" w:h="15840"/>"#,
        r#"<w:sectPrChange w:id="90" w:author="Ann" w:date="2026-01-01T00:00:00Z">"#,
        r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/></w:sectPr></w:sectPrChange></w:sectPr>"#,
    );
    let xml = xml.replace("<w:sectPr/></w:body>", &format!("{sect}</w:body>"));
    let package = Zip::new(parts::entries_doc(xml.as_bytes())).build();
    Document::open(package, &OpenOptions::default()).unwrap()
}

/// The elements of the main part with this local name.
fn count(doc: &Document, local: &str) -> usize {
    let view = doc.state.parts();
    let tree = view.tree(view.id(MAIN).unwrap()).unwrap();
    let mut stack = vec![tree.root()];
    let mut n = 0;
    while let Some(x) = stack.pop() {
        n += usize::from(x.local_name() == local);
        stack.extend(x.child_elements());
    }
    n
}

/// The page width the body's own section properties give.
fn page_width(doc: &Document) -> String {
    let view = doc.state.parts();
    let tree = view.tree(view.id(MAIN).unwrap()).unwrap();
    let body = tree
        .root()
        .child_elements()
        .find(|c| c.local_name() == "body")
        .unwrap();
    let sect = body
        .child_elements()
        .find(|c| c.local_name() == "sectPr")
        .unwrap();
    let size = sect
        .child_elements()
        .find(|c| c.local_name() == "pgSz")
        .unwrap();
    size.attrs()
        .find(|a| a.local_name() == "w")
        .map(|a| a.value().into_owned())
        .unwrap()
}

#[test]
fn accept_or_reject_of_everything_under_a_scope_leaves_the_body_section_alone() {
    // D gives the change of the body's sectPr to the last block, so a scope that holds p10 once took it.
    for action in ["accept", "reject"] {
        let mut doc = with_section_change();
        run(
            &mut doc,
            r#"{"op":"replace","ref":"p10","find":"After","with":"Below"}"#,
        );
        let b = batch(&doc, &format!(r#"{{"op":"{action}","all":true}}"#));
        doc.edit(Edit::ops(b, stamp()).scope(vec!["p10".to_string()]))
            .unwrap();
        assert_eq!(count(&doc, "sectPrChange"), 1, "{action}");
        assert_eq!(page_width(&doc), "12240", "{action}");
        assert_eq!(count(&doc, "ins") + count(&doc, "del"), 0, "{action}");
    }
    // Named by its id it is refused, and says why.
    let mut doc = with_section_change();
    let before = (doc.base(), doc.save().unwrap().bytes);
    let b = batch(&doc, r#"{"op":"reject","ids":[90]}"#);
    let e = doc
        .edit(Edit::ops(b, stamp()).scope(vec!["p10".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message()
            .starts_with("revision #90 changes the last section's properties"),
        "{e}"
    );
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    // Without a scope nothing is refused.
    let b = batch(&doc, r#"{"op":"reject","ids":[90]}"#);
    doc.edit(Edit::ops(b, stamp())).unwrap();
    assert_eq!(
        (count(&doc, "sectPrChange"), page_width(&doc).as_str()),
        (0, "11906")
    );
}

/// Arms a change of the body's section properties that no operation made.
fn arm_page_width() {
    arm(|tx| {
        let tree = tx.part_mut(MAIN).unwrap();
        let mut stack = vec![tree.root().id()];
        let mut size = None;
        while let Some(x) = stack.pop() {
            let n = tree.node_ref(x);
            if n.local_name() == "pgSz" {
                size = Some((x, n.namespace().to_owned()));
            }
            stack.extend(n.child_elements().map(|c| c.id()));
        }
        let (size, ns) = size.unwrap();
        tree.edit().set_attr(size, &ns, "w", "9999").unwrap();
    });
}

/// Arms a marker between blocks, at body level, before the block `before`.
fn arm_marker_before(doc: &Document, before: &str) {
    let key = doc.state.resolve(Claim::Implied, before).unwrap();
    arm(move |tx| {
        let tree = tx.part_mut(key.part()).unwrap();
        let ns = tree.node_ref(key.node()).namespace().to_owned();
        let body = tree.node_ref(key.node()).parent().unwrap().id();
        let marker = tree.edit().create_element(&ns, "proofErr").unwrap();
        tree.edit()
            .set_attr(marker, &ns, "type", "spellStart")
            .unwrap();
        tree.edit()
            .insert_before(body, Some(key.node()), marker)
            .unwrap();
    });
}

#[test]
fn the_second_pass_refuses_a_change_of_the_frame_that_no_block_of_the_scope_holds() {
    let ops = r#"{"op":"replace","ref":"p4","find":"thirty","with":"sixty"}"#;
    // The body's section properties belong to no block.
    let mut doc = with_section_change();
    let before = (doc.base(), doc.save().unwrap().bytes);
    arm_page_width();
    let e = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4..p10".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message()
            .contains("the body's last section properties, or what lies after the last block"),
        "{e}"
    );
    assert_eq!((doc.base(), doc.save().unwrap().bytes), before);
    // Without a scope the guard has nothing to hold.
    arm_page_width();
    doc.edit(Edit::ops(batch(&doc, ops), stamp())).unwrap();
    assert_eq!(page_width(&doc), "9999");
    // A marker between blocks belongs to the block after it.
    let mut doc = rich();
    arm_marker_before(&doc, "p5");
    let e = doc
        .edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .err()
        .unwrap();
    assert_eq!(e.code(), ErrorCode::ScopeViolation, "{e}");
    assert!(
        e.message().starts_with("p5 is outside the scope (p4)."),
        "{e}"
    );
    let mut doc = rich();
    arm_marker_before(&doc, "p4");
    doc.edit(Edit::ops(batch(&doc, ops), stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
    // A paragraph inserted into the body changes the frame's list of children, and that is no difference of the frame.
    let mut doc = rich();
    let b = batch(
        &doc,
        r#"{"op":"insert","ref":"p4","place":"after","paragraphs":["A new clause."]}"#,
    );
    doc.edit(Edit::ops(b, stamp()).scope(vec!["p4".to_string()]))
        .unwrap();
}
