//! The span tree as a schema tree: what `SpanTree` and `Refs` answer, and that the two, and the
//! provided methods of `Nodes` that the span tree overrides, agree.

use super::super::span::Refs;
use super::*;

const DOC: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<!-- before -->
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" xmlns:x="urn:example:unknown" plain="2">
  <!-- inside -->
  <w:body>
    <w:p w14:paraId="0A1B2C3D" w:rsidR="00112233" xml:space="preserve">
      <?pi data?>
      <w:r><w:t>one &amp; two</w:t></w:r>
      <x:foo xmlns:x="urn:other" a="1"><![CDATA[   ]]></x:foo>
      <x:bar><![CDATA[text]]></x:bar>
    </w:p>
    <w:p>&#160;</w:p>
  </w:body>
</w:document>"#;

#[test]
fn the_root_has_no_parent_and_children_are_elements() {
    let tree = parse(DOC);
    let root = SchemaTree::root(&tree);
    assert_eq!(
        tree.parent(root),
        None,
        "the document node is not an element"
    );
    assert_eq!(tree.name(root), (W, "document"));
    // comments, processing instructions and white space between elements are not children
    let body = tree.children(root);
    assert_eq!(body.len(), 1);
    assert_eq!(tree.name(body[0]), (W, "body"));
    assert_eq!(tree.parent(body[0]), Some(root));
    let paragraphs = tree.children(body[0]);
    assert_eq!(paragraphs.len(), 2);
    let inside: Vec<&str> = tree
        .children(paragraphs[0])
        .iter()
        .map(|c| tree.name(*c).1)
        .collect();
    assert_eq!(inside, ["r", "foo", "bar"]);
}

#[test]
fn attributes_are_data_and_declarations_are_not() {
    let tree = parse(DOC);
    // the root declares three prefixes and has one plain attribute
    let root = tree.attributes(SchemaTree::root(&tree));
    assert_eq!(root.len(), 1);
    assert_eq!((root[0].namespace, root[0].local), ("", "plain"));
    // in the order written, with the namespace of each and the value decoded
    let p = nth(&tree, "p", 0);
    let got: Vec<(&str, &str, String)> = tree
        .attributes(p)
        .iter()
        .map(|a| (a.namespace, a.local, a.value.to_string()))
        .collect();
    assert_eq!(
        got,
        [
            (W14, "paraId", "0A1B2C3D".to_string()),
            (W, "rsidR", "00112233".to_string()),
            (XML_NS, "space", "preserve".to_string()),
        ]
    );
    // a declaration on an element is not an attribute either
    let foo = nth(&tree, "foo", 0);
    let got: Vec<(&str, &str)> = tree
        .attributes(foo)
        .iter()
        .map(|a| (a.namespace, a.local))
        .collect();
    assert_eq!(got, [("", "a")]);
    assert_eq!(tree.name(foo), ("urn:other", "foo"));
}

#[test]
fn prefixes_resolve_where_they_are_in_scope() {
    let tree = parse(DOC);
    let (body, foo, bar) = (
        nth(&tree, "body", 0),
        nth(&tree, "foo", 0),
        nth(&tree, "bar", 0),
    );
    assert_eq!(tree.resolve_prefix(body, "w").as_deref(), Some(W));
    assert_eq!(tree.resolve_prefix(body, "w14").as_deref(), Some(W14));
    // x is rebound on foo, and means the root's again on its sibling
    assert_eq!(tree.resolve_prefix(foo, "x").as_deref(), Some("urn:other"));
    assert_eq!(
        tree.resolve_prefix(bar, "x").as_deref(),
        Some("urn:example:unknown")
    );
    assert_eq!(tree.resolve_prefix(body, "nosuchprefix"), None);
}

#[test]
fn character_data_is_text_unless_it_is_white_space() {
    let tree = parse(DOC);
    let has_text = |local: &str, n: usize| tree.has_text(nth(&tree, local, n));
    assert!(has_text("t", 0), "text");
    assert!(has_text("bar", 0), "a CDATA section with text in it");
    assert!(!has_text("foo", 0), "a CDATA section of spaces");
    assert!(!has_text("body", 0), "the white space between elements");
    // a no-break space, written as a reference, is Unicode white space: not text
    assert!(!tree.has_text(nth(&tree, "p", 1)));
}

#[test]
fn refs_answer_as_the_span_tree_does() {
    let tree = parse(DOC);
    let refs = Refs::new();
    for id in elements(&tree) {
        let n = tree.node_ref(id);
        assert_eq!(refs.name(n), tree.name(id));
        let by_ref: Vec<NodeId> = refs.children(n).iter().map(|c| c.id()).collect();
        assert_eq!(by_ref, tree.children(id));
        assert_eq!(refs.parent(n).map(|p| p.id()), tree.parent(id));
        let attrs = |a: Vec<Attr<'_>>| -> Vec<(String, String, String)> {
            a.iter()
                .map(|a| (a.namespace.into(), a.local.into(), a.value.to_string()))
                .collect()
        };
        assert_eq!(attrs(refs.attributes(n)), attrs(tree.attributes(id)));
        for prefix in ["w", "w14", "x", "", "nosuchprefix"] {
            assert_eq!(
                refs.resolve_prefix(n, prefix),
                tree.resolve_prefix(id, prefix)
            );
        }
    }
}

#[test]
fn the_overrides_of_the_provided_methods_give_what_the_defaults_give() {
    // `find_child` and `attribute` have defaults over `children` and `attributes`; the span tree
    // implements them without collecting. They must answer alike, for every attribute there is,
    // for one that is not, and for a declaration, which is not an attribute.
    let tree = parse(DOC);
    for id in elements(&tree) {
        for a in tree.attributes(id) {
            let got = tree.attribute(id, a.namespace, a.local);
            assert_eq!(got.as_deref(), Some(&*a.value));
        }
        assert_eq!(tree.attribute(id, "", "nosuchattribute"), None);
        assert_eq!(
            tree.attribute(id, "http://www.w3.org/2000/xmlns/", "w"),
            None,
            "a namespace declaration is not an attribute"
        );
        let kids = tree.children(id);
        for (i, k) in kids.iter().enumerate() {
            let first = tree.find_child(id, &mut |c| tree.name(c) == tree.name(*k));
            let expected = kids
                .iter()
                .copied()
                .find(|c| tree.name(*c) == tree.name(*k));
            assert_eq!(first, expected, "child {i} of {}", tree.name(id).1);
        }
        assert_eq!(tree.find_child(id, &mut |_| false), None);
    }
}
