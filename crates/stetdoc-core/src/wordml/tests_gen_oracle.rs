//! The independent statement of `docs/design/wordml.md` 4.2 for the tests: the wrappers around an item,
//! read from the XML by name up to the root, to compare with the contexts the text model records.
//! It shares no code with `text_build.rs` or `names.rs`.

use super::*;
use crate::xml::{NodeRef, SpanTree};

pub(super) fn is_w(n: &NodeRef<'_>, local: &str) -> bool {
    n.is_element()
        && n.local_name() == local
        && n.namespace().ends_with("wordprocessingml/2006/main")
            | n.namespace().ends_with("ooxml/wordprocessingml/main")
}

/// The wrapper an element is, read from its name alone: the independent statement of 4.2.
fn wrapper_of(a: &NodeRef<'_>) -> Option<WrapperKind> {
    let in_mc = a.namespace().ends_with("markup-compatibility/2006");
    if in_mc && matches!(a.local_name(), "Choice" | "Fallback") {
        return Some(WrapperKind::Alternate);
    }
    let local = [
        "ins",
        "del",
        "moveFrom",
        "moveTo",
        "hyperlink",
        "smartTag",
        "customXml",
    ]
    .into_iter()
    .chain(["sdt", "dir", "bdo", "fldSimple", "ruby"])
    .find(|l| is_w(a, l))?;
    let kind = match local {
        "ins" => WrapperKind::Ins,
        "del" => WrapperKind::Del,
        "moveFrom" => WrapperKind::MoveFrom,
        "moveTo" => WrapperKind::MoveTo,
        "hyperlink" => WrapperKind::Hyperlink,
        "smartTag" => WrapperKind::SmartTag,
        "customXml" => WrapperKind::CustomXml,
        "sdt" => WrapperKind::Control,
        "dir" => WrapperKind::Dir,
        "bdo" => WrapperKind::Bdo,
        "fldSimple" => WrapperKind::FieldSimple,
        _ => WrapperKind::Ruby,
    };
    Some(kind)
}

/// A row or a cell that a tracked change marks is a wrapper too, and one marked as both inserted and
/// deleted is two, so that it is in neither mode. Innermost first: the insertion is the inner one.
///
/// The properties are where Word writes them: before the content of the row or the cell. They are
/// looked for among the children that come before the first element of the content (a cell, or a
/// paragraph, table, control or custom XML element), and among no more than 64 of them, so a
/// `w:tcPr` that an edit moved after a paragraph is not read. This restates the rule of
/// `names::leading_child` in the test, so that the two are checked against each other.
fn row_or_cell_wrapper(a: &NodeRef<'_>) -> Vec<WrapperKind> {
    let (props, content, ins, del, ins_kind, del_kind): (&str, &[&str], _, _, _, _) =
        if is_w(a, "tr") {
            (
                "trPr",
                &["tc", "sdt", "customXml"],
                "ins",
                "del",
                WrapperKind::RowIns,
                WrapperKind::RowDel,
            )
        } else if is_w(a, "tc") {
            (
                "tcPr",
                &["p", "tbl", "sdt", "customXml"],
                "cellIns",
                "cellDel",
                WrapperKind::CellIns,
                WrapperKind::CellDel,
            )
        } else {
            return Vec::new();
        };
    let mut found = None;
    for c in a.child_elements().take(64) {
        if content.iter().any(|name| is_w(&c, name)) {
            break;
        }
        if is_w(&c, props) {
            found = Some(c);
            break;
        }
    }
    let Some(props) = found else {
        return Vec::new();
    };
    let inserted = props.child_elements().any(|c| is_w(&c, ins));
    let deleted = props.child_elements().any(|c| is_w(&c, del));
    let mut out = Vec::new();
    if inserted {
        out.push(ins_kind);
    }
    if deleted {
        out.push(del_kind);
    }
    out
}

/// The wrappers around an item, innermost first, read from the XML up to the root.
pub(super) fn wrappers_above(tree: &SpanTree, item: &Item) -> Vec<WrapperKind> {
    // the marks of a `w:fldSimple` sit inside it, whatever the kind of the item
    let node = tree.node_ref(item.node);
    let mut cur = if node.is_element() && is_w(&node, "fldSimple") {
        Some(node)
    } else {
        node.parent()
    };
    let mut out = Vec::new();
    while let Some(a) = cur {
        match wrapper_of(&a) {
            Some(kind) => out.push(kind),
            None => out.extend(row_or_cell_wrapper(&a)),
        }
        cur = a.parent();
    }
    out
}

pub(super) fn nearest_run(tree: &SpanTree, item: &Item) -> Option<NodeId> {
    let mut cur = tree.node_ref(item.node).parent();
    while let Some(a) = cur {
        if is_w(&a, "r") {
            return Some(a.id());
        }
        cur = a.parent();
    }
    None
}

#[test]
fn properties_written_after_the_content_are_not_the_properties_for_the_reader_or_the_oracle() {
    // Proves: a `w:trPr` after a cell, or a `w:tcPr` after a paragraph, is not where Word keeps the
    // properties, so the text model reads no revision from it, and this oracle says the same. A chaos
    // edit can move one there, and the two must agree on it.
    use super::testing::document;
    use std::sync::Arc;
    let late_row = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>\
                    <w:trPr><w:del w:id=\"1\"/></w:trPr></w:tr></w:tbl>";
    let row = "<w:tbl><w:tr><w:trPr><w:del w:id=\"2\"/></w:trPr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    let late_cell = "<w:tbl><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p>\
                     <w:tcPr><w:cellDel w:id=\"3\"/></w:tcPr></w:tc></w:tr></w:tbl>";
    let cell = "<w:tbl><w:tr><w:tc><w:tcPr><w:cellDel w:id=\"4\"/></w:tcPr><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl>";
    for (xml, expected) in [
        (late_row, vec![]),
        (row, vec![WrapperKind::RowDel]),
        (late_cell, vec![]),
        (cell, vec![WrapperKind::CellDel]),
    ] {
        let tree = document(xml);
        let story = Story::scan(&Arc::from("word/document.xml"), StoryKind::Body, &tree).unwrap();
        let i = (0..story.blocks().len())
            .find(|&i| story.paragraph(&tree, i).is_some())
            .unwrap();
        let m = story.paragraph(&tree, i).unwrap().map();
        let item = m.items().iter().find(|x| x.kind == ItemKind::Text).unwrap();
        let read: Vec<WrapperKind> = m.context(item.ctx).map(|c| c.kind).collect();
        assert_eq!(read, expected, "{xml}");
        assert_eq!(wrappers_above(&tree, item), expected, "{xml}");
    }
}
