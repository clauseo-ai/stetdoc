//! Tests of `validate_part`, the violation set and the part kinds.

use super::*;

// ---- validate_part --------------------------------------------------------------------

#[test]
fn a_plain_paragraph_validates() {
    let xml = doc_xml(
        r#"<w:p w14:paraId="1A2B3C4D"><w:pPr><w:pStyle w:val="Heading1"/><w:jc w:val="left"/></w:pPr>
           <w:r><w:rPr><w:b/><w:sz w:val="24"/></w:rPr><w:t xml:space="preserve">hi </w:t></w:r></w:p>
           <w:sectPr><w:pgSz w:w="12240" w:h="15840"/></w:sectPr>"#,
    );
    let set = violations(PartKind::Document, &xml);
    assert!(set.is_empty(), "{:?}", kinds(&set));
}

#[test]
fn out_of_order_unknown_and_repeated_children_are_distinguished() {
    let xml = doc_xml(
        r#"<w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/><w:jc w:val="right"/><w:tbl/></w:pPr></w:p>"#,
    );
    let got = reported(PartKind::Document, &xml);
    assert!(
        got.contains(&(ViolationKind::OutOfOrder, "w:pStyle".into())),
        "{got:?}"
    );
    assert!(
        got.contains(&(ViolationKind::TooMany, "w:jc".into())),
        "{got:?}"
    );
    assert!(
        got.contains(&(ViolationKind::UnexpectedChild, "w:tbl".into())),
        "{got:?}"
    );
    // a surplus element and a misplaced one are one kind of problem in a key
    let keyed = kinds(&violations(PartKind::Document, &xml));
    assert!(
        keyed.contains(&(ViolationKind::OutOfOrder, "w:jc".into())),
        "{keyed:?}"
    );
    assert!(!keyed.iter().any(|(k, _)| *k == ViolationKind::TooMany));
}

#[test]
fn a_second_copy_is_too_many_wherever_it_stands() {
    // spacing first makes pStyle misplaced, and pStyle twice makes the second a surplus copy:
    // the first is out of order, the second too many, and removing spacing leaves only the
    // surplus one. One key per element, whatever the reason.
    let xml = doc_xml(
        r#"<w:p><w:pPr><w:spacing w:before="0"/><w:pStyle w:val="A"/><w:pStyle w:val="A"/></w:pPr></w:p>"#,
    );
    let got = reported(PartKind::Document, &xml);
    assert_eq!(
        got,
        vec![
            (ViolationKind::OutOfOrder, "w:pStyle".into()),
            (ViolationKind::TooMany, "w:pStyle".into())
        ]
    );
    assert_eq!(violations(PartKind::Document, &xml).total(), 2);
    assert_eq!(kinds(&violations(PartKind::Document, &xml)).len(), 1);
    // an element that may repeat is not too many: a paragraph after the section properties is
    // out of order
    let xml = doc_xml(r#"<w:p/><w:sectPr/><w:p/>"#);
    let got = reported(PartKind::Document, &xml);
    assert!(
        got.iter().all(|(k, _)| *k != ViolationKind::TooMany),
        "{got:?}"
    );
    assert!(!got.is_empty(), "{got:?}");
}

#[test]
fn content_and_missing_children_are_keyed_by_the_parent_only() {
    // a table without properties: the order problem and the missing child name no element
    let xml = doc_xml(
        r#"<w:tbl><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>"#,
    );
    let set = violations(PartKind::Document, &xml);
    let keys: Vec<_> = set
        .iter()
        .map(|(k, _)| (k.kind, k.subject.as_str()))
        .collect();
    assert_eq!(
        keys,
        vec![
            (ViolationKind::OutOfOrder, ""),
            (ViolationKind::MissingChild, "")
        ]
    );
    // the report still says which
    let got = reported(PartKind::Document, &xml);
    assert!(got.len() >= 2, "{got:?}");
}

#[test]
fn an_element_that_declares_itself_ignorable_is_skipped() {
    // Part 3: an element is in the scope of its own mc:Ignorable, not only of its ancestors'
    let ok = doc_xml(r#"<w:p><x:note mc:Ignorable="x"/><w:r><w:t>x</w:t></w:r></w:p>"#);
    assert!(violations(PartKind::Document, &ok).is_empty());
    // without the declaration the same element is unknown
    let bad = doc_xml(r#"<w:p><x:note/><w:r><w:t>x</w:t></w:r></w:p>"#);
    assert_eq!(
        kinds(&violations(PartKind::Document, &bad)),
        vec![(
            ViolationKind::UnknownElement,
            "{urn:example:unknown}note".into()
        )]
    );
    // and one that lists a different namespace does not excuse itself
    let other = doc_xml(r#"<w:p><x:note mc:Ignorable="w14"/><w:r><w:t>x</w:t></w:r></w:p>"#);
    assert_eq!(violations(PartKind::Document, &other).total(), 1);
}

#[test]
fn attribute_problems() {
    let xml = doc_xml(
        r#"<w:p><w:r><w:rPr><w:sz w:val="big"/><w:b w:foo="1"/></w:rPr><w:t>x</w:t></w:r><w:bookmarkStart w:id="1"/></w:p>"#,
    );
    let got = kinds(&violations(PartKind::Document, &xml));
    assert!(
        got.contains(&(ViolationKind::InvalidAttributeValue, "w:val".into())),
        "{got:?}"
    );
    assert!(
        got.contains(&(ViolationKind::UnknownAttribute, "w:foo".into())),
        "{got:?}"
    );
    assert!(
        got.contains(&(ViolationKind::MissingAttribute, "w:name".into())),
        "{got:?}"
    );
}

#[test]
fn ignorable_namespaces_are_skipped_and_others_are_not() {
    let ok = doc_xml(r#"<w:p><w:pPr><w:pStyle w:val="A"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#);
    assert!(violations(PartKind::Document, &ok).is_empty());
    // w14 is ignorable in doc_xml. A name the tables have never heard of is skipped...
    let xml = doc_xml(r#"<w:p><w14:futureThing/><w:r><w:t>x</w:t></w:r></w:p>"#);
    assert!(violations(PartKind::Document, &xml).is_empty());
    // ...but a w14 element they know, in a place they do not allow it, is a violation.
    let xml = doc_xml(r#"<w:p><w14:ligatures/><w:r><w:t>x</w:t></w:r></w:p>"#);
    let got = kinds(&violations(PartKind::Document, &xml));
    assert_eq!(
        got,
        vec![(ViolationKind::UnexpectedChild, "w14:ligatures".into())]
    );
    // a namespace the tables do not understand, listed in mc:Ignorable, is skipped
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="w14 x"><w:body><w:p><x:thing a="1"><w:foo/></x:thing><w:r x:flag="1"><w:t>x</w:t></w:r></w:p></w:body></w:document>"#
    );
    assert!(violations(PartKind::Document, &xml).is_empty());
    // the same namespace without the declaration is a violation, as an element and as an attribute
    let xml = doc_xml(r#"<w:p><w:r x:flag="1"><w:t>x</w:t></w:r></w:p>"#);
    assert_eq!(
        kinds(&violations(PartKind::Document, &xml)),
        vec![(
            ViolationKind::UnknownAttribute,
            "{urn:example:unknown}flag".into()
        )]
    );
    // an unknown namespace that is not ignorable is a violation
    let xml = doc_xml(r#"<w:p><x:thing/><w:r><w:t>x</w:t></w:r></w:p>"#);
    let got = kinds(&violations(PartKind::Document, &xml));
    assert_eq!(
        got,
        vec![(
            ViolationKind::UnknownElement,
            "{urn:example:unknown}thing".into()
        )]
    );
}

#[test]
fn process_content_keeps_the_children_of_an_ignored_element() {
    // x:wrap is ignorable and not understood; with ProcessContent its runs count as the
    // paragraph's own children, so a misplaced pPr inside it is found
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="x" mc:ProcessContent="x:wrap"><w:body><w:p><w:r><w:t>a</w:t></w:r><x:wrap><w:pPr/></x:wrap></w:p></w:body></w:document>"#
    );
    assert_eq!(
        kinds(&violations(PartKind::Document, &xml)),
        vec![(ViolationKind::OutOfOrder, "".into())]
    );
    // without ProcessContent the whole element is skipped
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="x"><w:body><w:p><w:r><w:t>a</w:t></w:r><x:wrap><w:pPr/></x:wrap></w:p></w:body></w:document>"#
    );
    assert!(violations(PartKind::Document, &xml).is_empty());
}

#[test]
fn must_understand_names_a_namespace_the_tables_do_not_know() {
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="x"><w:body><w:p mc:MustUnderstand="x w14"/></w:body></w:document>"#
    );
    assert_eq!(
        kinds(&violations(PartKind::Document, &xml)),
        vec![(ViolationKind::MustUnderstand, "x".into())]
    );
}

#[test]
fn alternate_content_is_read_as_the_branch_word_takes() {
    let xml = doc_xml(
        r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:inline/></w:drawing></mc:Choice><mc:Fallback><w:pict/></mc:Fallback></mc:AlternateContent></w:r></w:p>"#,
    );
    let set = violations(PartKind::Document, &xml);
    assert!(set.is_empty(), "{:?}", kinds(&set));
    // a branch that puts rPr after content is wrong, found through the wrapper
    let xml = doc_xml(
        r#"<w:p><w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:inline/></w:drawing></mc:Choice></mc:AlternateContent><w:rPr/></w:r></w:p>"#,
    );
    let got = kinds(&violations(PartKind::Document, &xml));
    // a run is content: its order problems are keyed by the parent only
    assert!(
        got.contains(&(ViolationKind::OutOfOrder, "".into())),
        "{got:?}"
    );
}

#[test]
fn text_where_only_elements_belong_and_a_wrong_root() {
    let xml = doc_xml(r#"<w:p>stray text<w:r><w:t>x</w:t></w:r></w:p>"#);
    let got = kinds(&violations(PartKind::Document, &xml));
    assert_eq!(got, vec![(ViolationKind::UnexpectedText, "#text".into())]);
    let xml = format!(r#"<w:styles {NS_DECLS}/>"#);
    let got = kinds(&violations(PartKind::Document, &xml));
    assert_eq!(got, vec![(ViolationKind::WrongRoot, "w:styles".into())]);
}

#[test]
fn no_new_violations_is_a_set_difference_by_key() {
    // a document that already has one problem, inside a paragraph
    let before = violations(
        PartKind::Document,
        &doc_xml(r#"<w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr></w:p>"#),
    );
    assert_eq!(before.total(), 1);
    // an edit that inserts a clean paragraph in front, shifting the broken one: nothing new
    let after = violations(
        PartKind::Document,
        &doc_xml(
            r#"<w:p><w:r><w:t>new</w:t></w:r></w:p><w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr></w:p>"#,
        ),
    );
    assert!(after.added_since(&before).is_empty());
    // an edit that copies the broken paragraph repeats the same key: not new (this is what
    // lets the engine split a run or clone a paragraph that already has a problem)
    let after = violations(
        PartKind::Document,
        &doc_xml(
            r#"<w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr></w:p><w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr></w:p>"#,
        ),
    );
    assert!(after.added_since(&before).is_empty());
    assert_eq!(after.total(), 2);
    // a different wrong order is new: ind after the paragraph's jc, not pStyle after jc
    let other = violations(
        PartKind::Document,
        &doc_xml(
            r#"<w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr></w:p><w:p><w:pPr><w:jc w:val="left"/><w:ind w:left="1"/></w:pPr></w:p>"#,
        ),
    );
    let added = other.added_since(&before);
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].subject, "w:ind");
    // an edit that fixes the problem and adds nothing: nothing new
    let fixed = violations(
        PartKind::Document,
        &doc_xml(r#"<w:p><w:pPr><w:pStyle w:val="A"/></w:pPr></w:p>"#),
    );
    assert!(fixed.added_since(&before).is_empty());
    // an edit that adds a different problem: new
    let worse = violations(
        PartKind::Document,
        &doc_xml(
            r#"<w:p><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr><w:r><w:rPr><w:i/><w:b/></w:rPr></w:r></w:p>"#,
        ),
    );
    let added = worse.added_since(&before);
    assert_eq!(added.len(), 1);
    // w:rPr is unordered in the schema, so this is canonical order, not a schema violation
    assert_eq!(added[0].kind, ViolationKind::NonCanonicalOrder);
}

#[test]
fn other_parts_have_their_roots() {
    let comments = format!(
        r#"<w:comments {NS_DECLS}><w:comment w:id="0" w:author="A" w:date="2024-01-02T10:00:00Z" w:initials="A"><w:p><w:r><w:t>c</w:t></w:r></w:p></w:comment></w:comments>"#
    );
    let set = violations(PartKind::Comments, &comments);
    assert!(set.is_empty(), "{:?}", kinds(&set));
    assert_eq!(
        PartKind::from_content_type(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"
        ),
        Some(PartKind::Comments)
    );
    assert_eq!(PartKind::from_content_type("text/plain"), None);
}

#[test]
fn the_preview_w15_namespace_is_read_as_w15() {
    // `Comment042` and `Invalid_Word15Comments`: Word 2013's preview wrote `w15` in its 2010/11 namespace,
    // and Word reads it as `w15`. The tables do too: the part has its root, and its names are checked.
    let preview = "http://schemas.microsoft.com/office/word/2010/11/wordml";
    let part = |attrs: &str| {
        format!(
            r#"<w15:commentsEx xmlns:w15="{preview}"><w15:commentEx w15:paraId="0000000A" {attrs}/></w15:commentsEx>"#
        )
    };
    let set = violations(PartKind::CommentsExtended, &part(r#"w15:done="0""#));
    assert!(set.is_empty(), "{:?}", kinds(&set));
    let got = kinds(&violations(
        PartKind::CommentsExtended,
        &part(r#"w15:bogus="0""#),
    ));
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, ViolationKind::UnknownAttribute, "{got:?}");
    assert_eq!(
        crate::schema::tables::ns_by_uri(preview),
        crate::schema::tables::ns_by_uri("http://schemas.microsoft.com/office/word/2012/wordml")
    );
    assert!(!crate::schema::tables::is_strict_uri(preview));
}

#[test]
fn namespace_helpers() {
    assert!(is_extension_namespace(W14));
    assert!(!is_extension_namespace(W));
    assert_eq!(preferred_prefix(W14), Some("w14"));
    assert_eq!(
        preferred_prefix("http://purl.oclc.org/ooxml/wordprocessingml/main"),
        Some("w")
    );
    assert_eq!(
        display_qname("urn:example:unknown", "x"),
        "{urn:example:unknown}x"
    );
}

// ---- units ----------------------------------------------------------------------------

/// A table with one row of two cells holding the given paragraph contents.
fn table_xml(first: &str, second: &str) -> String {
    doc_xml(&format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="100"/><w:gridCol w:w="100"/></w:tblGrid>
           <w:tr><w:tc><w:p>{first}</w:p></w:tc><w:tc><w:p>{second}</w:p></w:tc></w:tr></w:tbl><w:sectPr/>"#
    ))
}

/// A run before the paragraph's properties: one order problem in content.
const BAD_P: &str = r#"<w:r><w:t>x</w:t></w:r><w:pPr><w:jc w:val="left"/></w:pPr>"#;
const GOOD_P: &str = r#"<w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t>x</w:t></w:r>"#;

fn ctx_for(tree: &SpanTree, local: &str, n: usize) -> (NodeId, Ctx) {
    let node = nth(tree, local, n);
    (
        node,
        ctx_at(PartKind::Document, tree, node).expect("a context"),
    )
}

#[test]
fn a_cell_paragraph_is_its_own_unit() {
    // The first cell paragraph already has a run before its pPr. An edit makes the second one
    // the same. Compared as one set the table hides that, because the key names the declaration
    // (w:p), and "some w:p has an order problem" was already there.
    let before_xml = table_xml(BAD_P, GOOD_P);
    let after_xml = table_xml(BAD_P, BAD_P);
    let (before, after) = (parse(&before_xml), parse(&after_xml));
    let (tbl_before, tbl_ctx) = ctx_for(&before, "tbl", 0);
    let (tbl_after, _) = ctx_for(&after, "tbl", 0);
    let whole_before = validate_subtree(tbl_ctx, &before, tbl_before).set;
    let whole_after = validate_subtree(tbl_ctx, &after, tbl_after).set;
    assert!(!whole_before.is_empty());
    assert!(
        whole_after.added_since(&whole_before).is_empty(),
        "a whole table as one unit hides the new problem"
    );
    // As units: the table's own element checks no paragraph, and each paragraph has its own
    // origin. The second paragraph was clean, so its problem is new.
    assert!(validate_unit(tbl_ctx, &after, tbl_after).set.is_empty());
    let (p2_before, p_ctx) = ctx_for(&before, "p", 1);
    let (p2_after, _) = ctx_for(&after, "p", 1);
    let origin = validate_unit(p_ctx, &before, p2_before).set;
    let now = validate_unit(p_ctx, &after, p2_after).set;
    assert!(origin.is_empty());
    assert_eq!(now.added_since(&origin).len(), 1, "{:?}", kinds(&now));
    // The first paragraph is unchanged: nothing new there.
    let (p1_before, _) = ctx_for(&before, "p", 0);
    let (p1_after, _) = ctx_for(&after, "p", 0);
    let origin = validate_unit(p_ctx, &before, p1_before).set;
    let now = validate_unit(p_ctx, &after, p1_after).set;
    assert!(now.added_since(&origin).is_empty());
    assert!(!origin.is_empty());
}

#[test]
fn a_table_unit_checks_its_structure_and_stops_at_paragraphs_and_nested_tables() {
    // A row before the grid is the table's own problem. A nested table whose row is before
    // its grid is the nested table's, and a paragraph with a run before its pPr is the
    // paragraph's.
    let nested = r#"<w:tbl><w:tblPr/><w:tr><w:tc><w:p/></w:tc></w:tr><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid></w:tbl>"#;
    let xml = doc_xml(&format!(
        r#"<w:tbl><w:tblPr/><w:tr><w:tc>{nested}<w:p>{BAD_P}</w:p></w:tc></w:tr><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid></w:tbl><w:sectPr/>"#
    ));
    let tree = parse(&xml);
    let (outer, ctx) = ctx_for(&tree, "tbl", 0);
    let own = validate_unit(ctx, &tree, outer).set;
    assert_eq!(
        kinds(&own),
        vec![(ViolationKind::OutOfOrder, String::new())]
    );
    assert_eq!(own.total(), 1);
    // everything below it, as validate_subtree reads it: the nested table and the paragraph too
    let all = validate_subtree(ctx, &tree, outer).set;
    assert_eq!(all.total(), 3, "{:?}", kinds(&all));
    // the nested table is a unit of its own, and so is the paragraph
    let (inner, inner_ctx) = ctx_for(&tree, "tbl", 1);
    assert_eq!(validate_unit(inner_ctx, &tree, inner).set.total(), 1);
    let (p, p_ctx) = ctx_for(&tree, "p", 1);
    assert_eq!(validate_unit(p_ctx, &tree, p).set.total(), 1);
    // the units of a part together are its keys
    let part = validate_part(PartKind::Document, &tree).set;
    assert_eq!(part.total(), 3);
}

#[test]
fn a_body_unit_does_not_look_inside_its_paragraphs() {
    let xml = doc_xml(&format!("<w:p>{BAD_P}</w:p><w:sectPr/>"));
    let tree = parse(&xml);
    let (body, ctx) = ctx_for(&tree, "body", 0);
    assert!(validate_unit(ctx, &tree, body).set.is_empty());
    assert_eq!(validate_subtree(ctx, &tree, body).set.total(), 1);
}

#[test]
fn the_unit_of_an_element_is_the_innermost_paragraph_or_table() {
    let nested = r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>in</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let xml = doc_xml(&format!(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p>{nested}</w:tc></w:tr></w:tbl><w:p><w:r><w:t>b</w:t></w:r></w:p><w:sectPr/>"#
    ));
    let tree = parse(&xml);
    let root = |local: &str, n: usize| unit_root(&tree, nth(&tree, local, n));
    // a run belongs to its paragraph, a cell, a row and the grid to their table
    assert_eq!(root("r", 0), nth(&tree, "p", 0));
    assert_eq!(root("tc", 0), nth(&tree, "tbl", 0));
    assert_eq!(root("tr", 0), nth(&tree, "tbl", 0));
    assert_eq!(root("gridCol", 0), nth(&tree, "tbl", 0));
    // a paragraph in a cell is its own unit, and so is a nested table, and what is in it
    assert_eq!(root("p", 0), nth(&tree, "p", 0));
    assert_eq!(root("tc", 1), nth(&tree, "tbl", 1));
    assert_eq!(root("r", 1), nth(&tree, "p", 1));
    assert_eq!(root("p", 2), nth(&tree, "p", 2));
    // the body and an element above every paragraph and table are their own unit
    assert_eq!(root("body", 0), nth(&tree, "body", 0));
    assert_eq!(root("sectPr", 0), nth(&tree, "sectPr", 0));
}

#[test]
fn a_merged_block_may_carry_the_problems_of_every_block_merged_into_it() {
    // Two paragraphs are joined. One had a run before its pPr, the other a jc before pStyle.
    // The merged paragraph has both. Against either origin alone the other problem is new,
    // against the union of the two nothing is.
    let first = r#"<w:r><w:t>a</w:t></w:r><w:pPr><w:pStyle w:val="A"/></w:pPr>"#;
    let second =
        r#"<w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr><w:r><w:t>b</w:t></w:r>"#;
    let merged = r#"<w:r><w:t>a</w:t></w:r><w:pPr><w:jc w:val="left"/><w:pStyle w:val="A"/></w:pPr><w:r><w:t>b</w:t></w:r>"#;
    let unit = |inner: &str| {
        let tree = parse(&doc_xml(&format!("<w:p>{inner}</w:p><w:sectPr/>")));
        let (p, ctx) = ctx_for(&tree, "p", 0);
        validate_unit(ctx, &tree, p).set
    };
    let (a, b, both) = (unit(first), unit(second), unit(merged));
    assert_eq!(a.total(), 1);
    assert_eq!(b.total(), 1);
    assert_eq!(both.total(), 2);
    assert_eq!(both.added_since(&a).len(), 1);
    assert_eq!(both.added_since(&b).len(), 1);
    assert!(both.added_since(&a.union(&b)).is_empty());
    // the union keeps the counts of both
    assert_eq!(a.union(&b).total(), 2);
}

#[test]
fn a_known_element_inside_an_unknown_one_is_checked_while_the_budget_lasts() {
    use super::super::validate::KNOWN_BUDGET;
    // A run with its w:rPr after its text, inside an element of an unknown namespace in a paragraph, after `filler` empty
    // unknown elements. The search below the unknown element looks at it, at each filler and at the run.
    let xml = |filler: usize| {
        doc_xml(&format!(
            r#"<w:p><u:a xmlns:u="urn:example:unknown">{}<w:r><w:t>x</w:t><w:rPr><w:b/></w:rPr></w:r></u:a></w:p>"#,
            "<u:f/>".repeat(filler)
        ))
    };
    let unknown = (
        ViolationKind::UnknownElement,
        "{urn:example:unknown}a".to_string(),
    );
    let order =
        |got: &[(ViolationKind, String)]| got.iter().any(|(k, _)| *k == ViolationKind::OutOfOrder);
    // The run is the last node the budget reaches, and its own content model sees the order.
    let got = reported(PartKind::Document, &xml(KNOWN_BUDGET - 2));
    assert!(got.contains(&unknown), "{got:?}");
    assert!(order(&got), "{got:?}");
    // One node more and the search stops before the run: the unknown element is still reported, and the run goes unchecked.
    let got = reported(PartKind::Document, &xml(KNOWN_BUDGET - 1));
    assert!(got.contains(&unknown), "{got:?}");
    assert!(!order(&got), "{got:?}");
}
