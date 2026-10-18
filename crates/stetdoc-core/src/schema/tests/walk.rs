//! `walk_unit`, `read_children` and `in_taken_branch`: the unit as the Word rules read it. The
//! law that matters is `walk_law`: every element the validator checks in a unit, and that Word
//! reads, is an element the walk visits, in document order and once.

use super::super::validate::visited;
use super::mc::Gen;
use super::*;
use std::collections::HashSet;

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn walked(tree: &SpanTree, root: NodeId, stop: bool) -> Vec<(NodeId, usize)> {
    let mut out = Vec::new();
    walk_unit(tree, root, stop, &mut |node, depth| out.push((node, depth)));
    out
}

/// The local names and depths of the walk of the first element named `root`.
fn shape(xml: &str, root: &str, stop: bool) -> Vec<(String, usize)> {
    let tree = parse(xml);
    walked(&tree, nth(&tree, root, 0), stop)
        .into_iter()
        .map(|(n, d)| (tree.name(n).1.to_string(), d))
        .collect()
}

fn pairs(list: &[(&str, usize)]) -> Vec<(String, usize)> {
    list.iter().map(|(n, d)| ((*n).to_string(), *d)).collect()
}

/// A paragraph with a text box in the branch Word reads and another in the one it does not.
fn text_box(requires: &str) -> String {
    doc_xml(&format!(
        r#"<w:p><w:r><mc:AlternateContent>
             <mc:Choice Requires="{requires}"><w:drawing><wp:inline xmlns:a="{A}"><a:graphic>
               <a:graphicData uri="u"><wps:wsp><wps:txbx><w:txbxContent>
                 <w:p><w:r><w:t>inner</w:t></w:r></w:p>
               </w:txbxContent></wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice>
             <mc:Fallback><w:pict><w:p><w:r><w:t>fallback</w:t></w:r></w:p></w:pict></mc:Fallback>
           </mc:AlternateContent></w:r></w:p>"#
    ))
}

#[test]
fn a_paragraph_is_walked_in_document_order_with_depths() {
    let xml = doc_xml(
        r#"<w:p><w:pPr><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:t>a</w:t></w:r><w:ins w:id="1" w:author="a"><w:r><w:t>b</w:t></w:r></w:ins></w:p>"#,
    );
    assert_eq!(
        shape(&xml, "p", true),
        pairs(&[
            ("p", 0),
            ("pPr", 1),
            ("rPr", 2),
            ("b", 3),
            ("r", 1),
            ("t", 2),
            ("ins", 1),
            ("r", 2),
            ("t", 3),
        ])
    );
}

#[test]
fn a_unit_stops_at_a_paragraph_below_it_and_a_whole_part_does_not() {
    let xml = text_box("wps");
    let stops = shape(&xml, "p", true);
    // the paragraph of the text box is visited, and not entered
    let inner = stops.iter().filter(|(n, _)| n == "p").count();
    assert_eq!(inner, 2);
    assert!(!stops.iter().any(|(n, _)| n == "t"));
    // the elements between the paragraphs are read: the drawing, the shape, the text box
    for name in ["drawing", "inline", "graphic", "wsp", "txbx", "txbxContent"] {
        assert!(stops.iter().any(|(n, _)| n == name), "{name} is walked");
    }
    // a walk that does not stop reads the text of the inner paragraph, and not the fallback's
    let all = shape(&xml, "p", false);
    assert_eq!(all.iter().filter(|(n, _)| n == "t").count(), 1);
    // a unit that starts at the inner paragraph is that paragraph alone
    let tree = parse(&xml);
    let inner = nth(&tree, "p", 1);
    assert_eq!(
        walked(&tree, inner, true)
            .iter()
            .map(|(n, d)| (tree.name(*n).1, *d))
            .collect::<Vec<_>>(),
        [("p", 0), ("r", 1), ("t", 2)]
    );
}

#[test]
fn the_branch_word_reads_is_walked_and_the_other_is_not() {
    let text = |xml: &str| {
        let tree = parse(xml);
        let unit = nth(&tree, "p", 0);
        walked(&tree, unit, false)
            .into_iter()
            .filter(|(n, _)| tree.name(*n).1 == "t")
            .map(|(n, _)| tree.node_ref(n).text_content())
            .collect::<Vec<_>>()
    };
    assert_eq!(text(&text_box("wps")), ["inner"]);
    // a namespace the tables do not know: Word reads the fallback
    assert_eq!(text(&text_box("x")), ["fallback"]);
    // the wrappers are not visited and are not a level
    let shaped = shape(&text_box("wps"), "p", true);
    assert!(
        !shaped
            .iter()
            .any(|(n, _)| n == "AlternateContent" || n == "Choice")
    );
    assert_eq!(shaped.get(2), Some(&("drawing".to_string(), 2)));
}

#[test]
fn an_ignorable_element_is_left_out_with_everything_in_it() {
    let xml = doc_xml(
        r#"<w:p mc:Ignorable="x"><x:note><w:r><w:t>hidden</w:t></w:r></x:note><w:r/></w:p>"#,
    );
    assert_eq!(shape(&xml, "p", true), pairs(&[("p", 0), ("r", 1)]));
    // listed in mc:ProcessContent, its children are read in its place
    let kept = doc_xml(
        r#"<w:p mc:Ignorable="x" mc:ProcessContent="x:note"><x:note><w:r/></x:note><w:r/></w:p>"#,
    );
    assert_eq!(
        shape(&kept, "p", true),
        pairs(&[("p", 0), ("r", 1), ("r", 1)])
    );
}

#[test]
fn an_element_the_tables_do_not_know_is_entered_unless_it_is_ignorable() {
    // Word reads more than the tables hold: the drawing around a text box is one example
    let xml = doc_xml(r#"<w:p><x:note><w:r><w:t>read</w:t></w:r></x:note><w:r/></w:p>"#);
    assert_eq!(
        shape(&xml, "p", true),
        pairs(&[("p", 0), ("note", 1), ("r", 2), ("t", 3), ("r", 1)])
    );
}

#[test]
fn read_children_replaces_wrappers_and_leaves_out_what_is_skipped() {
    let names = |xml: &str, parent: &str| {
        let tree = parse(xml);
        read_children(&tree, nth(&tree, parent, 0))
            .into_iter()
            .map(|n| tree.name(n).1.to_string())
            .collect::<Vec<_>>()
    };
    let xml = doc_xml(
        r#"<w:p mc:Ignorable="x"><w:pPr/><x:note/><mc:AlternateContent>
             <mc:Choice Requires="w14"><w:r/><w:hyperlink/></mc:Choice>
             <mc:Fallback><w:bookmarkStart/></mc:Fallback>
           </mc:AlternateContent><w:ins/></w:p>"#,
    );
    assert_eq!(names(&xml, "p"), ["pPr", "r", "hyperlink", "ins"]);
    // with no branch the wrapper stands for nothing
    let none = doc_xml(
        r#"<w:p><w:r/><mc:AlternateContent><mc:Choice Requires="x"><w:r/></mc:Choice></mc:AlternateContent></w:p>"#,
    );
    assert_eq!(names(&none, "p"), ["r"]);
}

#[test]
fn a_node_is_in_a_taken_branch_unless_it_is_in_the_other_one() {
    let in_branch = |requires: &str, text: &str| {
        let tree = parse(&text_box(requires));
        let t = elements(&tree)
            .into_iter()
            .find(|n| tree.name(*n).1 == "t" && tree.node_ref(*n).text_content() == text)
            .expect("the text is in the document");
        in_taken_branch(&tree, t)
    };
    assert!(in_branch("wps", "inner"));
    assert!(!in_branch("wps", "fallback"));
    assert!(!in_branch("x", "inner"));
    assert!(in_branch("x", "fallback"));
    // outside any wrapper, and the root
    let tree = parse(&doc_xml("<w:p/>"));
    assert!(in_taken_branch(&tree, nth(&tree, "p", 0)));
    assert!(in_taken_branch(&tree, tree.root().id()));
}

#[test]
fn the_namespaces_and_relationship_types_have_both_spellings() {
    assert!(is_word_namespace(W));
    assert!(is_word_namespace(
        "http://purl.oclc.org/ooxml/wordprocessingml/main"
    ));
    assert!(!is_word_namespace(W14));
    assert!(!is_word_namespace(""));
    // the two spellings are the ones the tables treat as one namespace, so a name in either is
    // found as the same element
    assert_eq!(
        crate::schema::tables::ns_by_uri(crate::schema::validate::W_STRICT_NS),
        crate::schema::tables::ns_by_uri(W)
    );
    let transitional =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";
    let strict = "http://purl.oclc.org/ooxml/officeDocument/relationships/comments";
    assert_eq!(
        PartKind::from_relationship_type(transitional),
        Some(PartKind::Comments)
    );
    assert_eq!(
        PartKind::from_relationship_type(strict),
        Some(PartKind::Comments)
    );
    assert_eq!(
        PartKind::from_relationship_type(
            "http://schemas.microsoft.com/office/2011/relationships/commentsExtended"
        ),
        Some(PartKind::CommentsExtended)
    );
    assert_eq!(
        PartKind::from_relationship_type(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
        ),
        None
    );
    for kind in PartKind::ALL {
        assert_eq!(
            PartKind::from_relationship_type(kind.relationship_type()),
            Some(kind)
        );
    }
}

/// The law of the walk, for the unit that starts at `unit` in a part of `kind`: it visits each
/// element once, in document order, with the depth never growing by more than one, and it visits
/// every element that the validator checks in the unit and that Word reads.
pub(in crate::schema) fn walk_law(
    kind: PartKind,
    tree: &SpanTree,
    unit: NodeId,
) -> Result<(), String> {
    let walk = walked(tree, unit, true);
    if walk.first() != Some(&(unit, 0)) {
        return Err("the walk does not start at the unit".to_string());
    }
    let mut seen = HashSet::new();
    let mut last = 0usize;
    for (node, depth) in &walk {
        if !seen.insert(*node) {
            return Err(format!("{} is visited twice", tree.name(*node).1));
        }
        if *depth > last + 1 {
            return Err(format!(
                "the depth jumps to {depth} at {}",
                tree.name(*node).1
            ));
        }
        last = *depth;
    }
    let Some(ctx) = ctx_at(kind, tree, unit) else {
        return Ok(());
    };
    for node in visited(ctx, tree, unit, true) {
        if in_taken_branch(tree, node) && !seen.contains(&node) {
            return Err(format!(
                "the validator checks {} and the walk does not",
                tree.name(node).1
            ));
        }
    }
    Ok(())
}

#[test]
fn the_walk_covers_what_the_validator_checks_on_random_nestings() {
    let mut rng = Gen(0x9E37_79B9_7F4A_7C15);
    for _ in 0..400 {
        let xml = rng.part();
        let tree = parse(&xml);
        for unit in elements(&tree)
            .into_iter()
            .filter(|n| tree.name(*n) == w("p"))
        {
            if let Err(why) = walk_law(PartKind::Document, &tree, unit) {
                panic!("{why} in {xml}");
            }
        }
    }
}

#[test]
fn the_law_holds_for_a_text_box_and_a_table() {
    for xml in [
        text_box("wps"),
        text_box("x"),
        doc_xml(
            r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/></w:tc></w:tr></w:tbl>"#,
        ),
    ] {
        let tree = parse(&xml);
        for unit in elements(&tree)
            .into_iter()
            .filter(|n| matches!(tree.name(*n), (W, "p" | "tbl")))
        {
            walk_law(PartKind::Document, &tree, unit).unwrap_or_else(|why| panic!("{why}"));
        }
    }
}

#[test]
fn a_walk_of_a_whole_part_visits_every_element_once() {
    let xml = doc_xml(
        r#"<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="1"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r/></w:p><w:sectPr/>"#,
    );
    let tree = parse(&xml);
    let all = walked(&tree, tree.root().id(), false);
    let got: Vec<NodeId> = all.iter().map(|(n, _)| *n).collect();
    assert_eq!(got, elements(&tree));
    // the same walk that stops at boundaries visits the paragraph and the table and not what is in them
    let stops = walked(&tree, tree.root().id(), true);
    assert!(stops.len() < all.len());
    assert!(stops.iter().any(|(n, _)| tree.name(*n) == w("tbl")));
    assert!(!stops.iter().any(|(n, _)| tree.name(*n) == w("t")));
}
