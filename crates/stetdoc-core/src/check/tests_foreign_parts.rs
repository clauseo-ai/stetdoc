//! Origins from another package that are not blocks of a story (`check.md` 2.3, the ask of `compare`, 12): a style or numbering
//! definition that `compare`'s import appends to a part that is not a story brings the keys its element had in the original, and
//! a part it copies whole brings the keys of the whole part. A problem the original had is carried, and one it did not have is new.

use super::keys::Key;
use super::tests::{W, state, state_with};
use super::tests_edit::{MAIN, MISORDERED_RPR, accepted, refused, say};
use super::tests_units::comments_part;
use super::{Declared, part_keys, unit_keys};
use crate::api::check::FindingCode as C;
use crate::wordml::BlockKey;
use crate::xml::NodeId;
use std::sync::Arc;

const STYLES: &str = "word/styles.xml";

/// A style sheet with these styles.
fn styles(inner: &str) -> String {
    format!(r#"<w:styles xmlns:w="{W}">{inner}</w:styles>"#)
}

const NORMAL: &str = r#"<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>"#;

/// A style whose `w:basedOn` comes before its `w:name`: out of the schema's order.
const MISORDERED: &str = r#"<w:style w:type="paragraph" w:styleId="Quote"><w:basedOn w:val="Normal"/><w:name w:val="Quote"/></w:style>"#;

fn with_styles(inner: &str) -> crate::state::State {
    let xml = styles(inner);
    state_with(
        "<w:p><w:r><w:t>a</w:t></w:r></w:p>",
        &[(STYLES, "styles", "styles", xml.as_str())],
    )
}

/// The last `w:style` of the style sheet.
fn last_style(tree: &crate::xml::SpanTree) -> NodeId {
    tree.root()
        .child_elements()
        .filter(|c| c.local_name() == "style")
        .last()
        .map(|c| c.id())
        .expect("a style")
}

#[test]
fn an_imported_definition_brings_the_keys_its_element_had_in_the_original() {
    // The original's style sheet has the misordered style, and its keys say so.
    let original = with_styles(&format!("{NORMAL}{MISORDERED}"));
    let view = original.parts();
    let theirs = BlockKey::new(
        Arc::from(STYLES),
        last_style(view.tree(view.id(STYLES).expect("styles")).expect("parses")),
    );
    let keys = unit_keys(&view, &theirs).expect("keys");
    assert!(
        keys.iter()
            .any(|k| matches!(k, Key::Schema(v) if v.subject.contains("name"))),
        "{keys:?}"
    );
    // The redline's style sheet gets a copy of it, appended as a new definition.
    let revised = with_styles(NORMAL);
    let mut tx = revised.begin();
    let copy = {
        let tree = tx.part_mut(STYLES).expect("styles");
        let root = tree.root().id();
        let style = tree.edit().create_element(W, "style").expect("creates");
        tree.edit()
            .set_attr(style, W, "type", "paragraph")
            .expect("sets");
        tree.edit()
            .set_attr(style, W, "styleId", "Quote")
            .expect("sets");
        for (local, val) in [("basedOn", "Normal"), ("name", "Quote")] {
            let child = tree.edit().create_element(W, local).expect("creates");
            tree.edit().set_attr(child, W, "val", val).expect("sets");
            tree.edit().append_child(style, child).expect("appends");
        }
        tree.edit().append_child(root, style).expect("appends");
        style
    };
    let refusal = say(&tx, &Declared::default());
    refused(&refusal);
    assert_eq!(refusal.new, [C::SchemaOutOfOrder], "{:?}", refusal.new);
    let declared = Declared {
        foreign: vec![(BlockKey::new(Arc::from(STYLES), copy), keys)],
        ..Declared::default()
    };
    accepted(&say(&tx, &declared));
    // A definition from an original that had no such problem brings nothing to carry.
    let clean = with_styles(NORMAL);
    let view = clean.parts();
    let ours = BlockKey::new(
        Arc::from(STYLES),
        last_style(view.tree(view.id(STYLES).expect("styles")).expect("parses")),
    );
    let none = Declared {
        foreign: vec![(
            BlockKey::new(Arc::from(STYLES), copy),
            unit_keys(&view, &ours).expect("keys"),
        )],
        ..Declared::default()
    };
    refused(&say(&tx, &none));
}

#[test]
fn a_part_copied_whole_brings_the_keys_of_the_part_it_was_copied_from() {
    let faulty = format!(
        r#"<w:comment w:id="0" w:author="a"><w:p><w:r>{MISORDERED_RPR}<w:t>note</w:t></w:r></w:p></w:comment>"#
    );
    // The original has the comments part with a run property out of canonical order.
    let xml = format!(r#"<w:comments xmlns:w="{W}">{faulty}</w:comments>"#);
    let original = state_with(
        "<w:p><w:r><w:t>a</w:t></w:r></w:p>",
        &[("word/comments.xml", "comments", "comments", xml.as_str())],
    );
    let keys = part_keys(&original.parts(), "word/comments.xml").expect("keys");
    assert!(
        keys.iter().any(|k| k.code() == C::SchemaNonCanonicalOrder),
        "{keys:?}"
    );
    // The redline gets a copy of the part: its problem is carried, and a problem the original lacked is new.
    let s = state("<w:p><w:r><w:t>a</w:t></w:r></w:p>");
    let mut tx = s.begin();
    tx.add_part(comments_part(&faulty)).expect("adds");
    refused(&say(&tx, &Declared::default()));
    let declared = Declared {
        parts: vec![(Arc::from("word/comments.xml"), keys.clone())],
        ..Declared::default()
    };
    let said = say(&tx, &declared);
    let v = accepted(&said);
    assert!(v.carried_total >= 1, "{v:?}");
    let mut tx = s.begin();
    tx.add_part(comments_part(r#"<w:comment w:id="0" w:author="a"/>"#))
        .expect("adds");
    let said = say(&tx, &declared);
    refused(&said);
    assert_eq!(said.new, [C::ContainerWithoutBlock]);
    // A part that has no schema has no keys to give.
    let e = part_keys(&original.parts(), "word/media/image1.png").expect_err("no schema");
    assert_eq!(e.code(), crate::api::error::ErrorCode::Internal);
    let _ = MAIN;
}
