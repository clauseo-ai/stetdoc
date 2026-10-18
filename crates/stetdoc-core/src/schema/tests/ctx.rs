//! `ctx_at`: the schema context of an element of a tree.

use super::*;

fn body(xml_inside: &str) -> SpanTree {
    parse(&doc_xml(xml_inside))
}

fn path(names: &[&str]) -> Option<Ctx> {
    let names: Vec<QName<'_>> = names.iter().map(|n| w(n)).collect();
    Ctx::from_path(PartKind::Document, &names)
}

fn at(tree: &SpanTree, local: &str, n: usize) -> Option<Ctx> {
    ctx_at(PartKind::Document, tree, nth(tree, local, n))
}

#[test]
fn the_context_is_the_declaration_found_by_walking_down_from_the_root() {
    let tree = body(
        r#"<w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:rPr><w:i/></w:rPr><w:t>x</w:t></w:r></w:p>
           <w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#,
    );
    // the root is the part's root, and an element's context is that of its own children
    assert_eq!(
        ctx_at(PartKind::Document, &tree, tree.root().id()),
        Ctx::root(PartKind::Document)
    );
    assert_eq!(
        at(&tree, "body", 0),
        path(&[]).and_then(|c| c.child(w("body")))
    );
    assert_eq!(at(&tree, "p", 0), path(&["body", "p"]));
    assert_eq!(at(&tree, "pPr", 0), path(&["body", "p", "pPr"]));
    assert_eq!(at(&tree, "r", 0), path(&["body", "p", "r"]));
    // w:rPr is declared differently in w:pPr and in w:r, which is the reason for walking down
    let in_ppr = at(&tree, "rPr", 0);
    let in_run = at(&tree, "rPr", 1);
    assert_eq!(in_ppr, path(&["body", "p", "pPr", "rPr"]));
    assert_eq!(in_run, path(&["body", "p", "r", "rPr"]));
    assert_ne!(in_ppr, in_run);
    assert_eq!(at(&tree, "tc", 0), path(&["body", "tbl", "tr", "tc"]));
    // the p in the cell is a paragraph like any other
    assert_eq!(at(&tree, "p", 1), path(&["body", "p"]));
}

#[test]
fn an_alternate_content_is_not_a_level() {
    // the run in the choice, and the one in the fallback, are runs of the paragraph; so are the
    // wrapper and its branches, whose children belong to the paragraph's content model
    let tree = body(
        r#"<w:p><mc:AlternateContent>
             <mc:Choice Requires="wps"><w:r><w:rPr><w:b/></w:rPr></w:r></mc:Choice>
             <mc:Fallback><w:r/></mc:Fallback>
           </mc:AlternateContent></w:p>"#,
    );
    let p = path(&["body", "p"]);
    assert!(p.is_some());
    assert_eq!(at(&tree, "AlternateContent", 0), p);
    assert_eq!(at(&tree, "Choice", 0), p);
    assert_eq!(at(&tree, "Fallback", 0), p);
    assert_eq!(at(&tree, "r", 0), path(&["body", "p", "r"]));
    assert_eq!(at(&tree, "r", 1), path(&["body", "p", "r"]));
    assert_eq!(at(&tree, "rPr", 0), path(&["body", "p", "r", "rPr"]));
    // nested wrappers are passed through the same way
    let nested = body(
        r#"<w:p><mc:AlternateContent><mc:Choice Requires="w14"><mc:AlternateContent>
             <mc:Choice Requires="wps"><w:r/></mc:Choice></mc:AlternateContent></mc:Choice></mc:AlternateContent></w:p>"#,
    );
    assert_eq!(at(&nested, "r", 0), path(&["body", "p", "r"]));
}

#[test]
fn a_processed_wrapper_is_not_a_level_and_a_skipped_element_has_no_context() {
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="x" mc:ProcessContent="x:wrap"><w:body><w:p><w:pPr>
             <x:wrap><w:rPr/></x:wrap><x:note><w:rPr/></x:note></w:pPr></w:p></w:body></w:document>"#
    );
    let tree = parse(&xml);
    let ppr = path(&["body", "p", "pPr"]);
    // what the wrapper keeps is read as children of the properties, and so is the wrapper itself
    assert_eq!(at(&tree, "wrap", 0), ppr);
    assert_eq!(at(&tree, "rPr", 0), path(&["body", "p", "pPr", "rPr"]));
    // what is skipped is not read at all
    assert_eq!(at(&tree, "note", 0), None);
    assert_eq!(at(&tree, "rPr", 1), None);
}

#[test]
fn there_is_no_context_where_the_schema_does_not_place_an_element() {
    // an element the paragraph does not name has none; a run inside it has a run's own context, so its children are checked
    // by a run's content model wherever it stands
    let tree = body("<w:p><w:bogus><w:r/><w:x/></w:bogus></w:p>");
    assert_eq!(at(&tree, "bogus", 0), None);
    assert_eq!(at(&tree, "x", 0), None);
    assert_eq!(at(&tree, "r", 0), path(&["body", "p", "r"]));
    // a part whose root is not the one asked for still has the root's context, and nothing below
    // that matches
    let comments = parse(&format!("<w:comments {NS_DECLS}><w:comment/></w:comments>"));
    let comment = nth(&comments, "comment", 0);
    assert_eq!(ctx_at(PartKind::Document, &comments, comment), None);
    assert!(ctx_at(PartKind::Comments, &comments, comment).is_some());
}

#[test]
fn code_that_holds_node_refs_finds_the_same_context() {
    // the wrapper and the processed element of the other tests, and plain content: every element
    // gets from `Refs` the context the tree gives it
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="x" mc:ProcessContent="x:wrap"><w:body><w:p><w:pPr>
             <x:wrap><w:rPr/></x:wrap><x:note><w:rPr/></x:note></w:pPr>
             <mc:AlternateContent><mc:Choice Requires="wps"><w:r><w:rPr/></w:r></mc:Choice></mc:AlternateContent>
           </w:p></w:body></w:document>"#
    );
    let tree = parse(&xml);
    let refs = crate::schema::Refs::new();
    for id in elements(&tree) {
        assert_eq!(
            ctx_at(PartKind::Document, &refs, tree.node_ref(id)),
            ctx_at(PartKind::Document, &tree, id),
            "{}",
            tree.name(id).1
        );
    }
}
