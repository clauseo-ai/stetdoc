//! The markup compatibility rules: which branch of an `mc:AlternateContent` Word reads, how an
//! ignorable element is read, and that code which holds `NodeRef`s (`wordml`) and code which
//! reads children through `effective_children` (insertion and validation) cannot disagree.

use super::super::effective::effective_children;
use super::super::scope::scope_at;
use super::*;
use crate::xml::NodeRef;

fn node<'a>(tree: &'a SpanTree, local: &str, n: usize) -> NodeRef<'a> {
    tree.node_ref(nth(tree, local, n))
}

/// A paragraph holding an `mc:AlternateContent` with one choice and, when asked, a fallback.
fn alternate(requires: Option<&str>, fallback: bool) -> String {
    let attr = requires.map_or(String::new(), |r| format!(r#" Requires="{r}""#));
    let fallback = if fallback {
        "<mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback>"
    } else {
        ""
    };
    doc_xml(&format!(
        r#"<w:p><mc:AlternateContent><mc:Choice{attr}><w:r><w:t>choice</w:t></w:r></mc:Choice>{fallback}</mc:AlternateContent></w:p>"#
    ))
}

/// The local name of the branch `taken_branch` picks.
fn branch(xml: &str) -> Option<String> {
    let tree = parse(xml);
    taken_branch(&node(&tree, "AlternateContent", 0)).map(|b| b.local_name().to_string())
}

#[test]
fn the_choice_is_taken_when_every_namespace_it_requires_is_known() {
    let choice = Some("Choice".to_string());
    let fallback = Some("Fallback".to_string());
    // wps and w14 are namespaces the tables know; x is not
    assert_eq!(branch(&alternate(Some("wps"), true)), choice);
    assert_eq!(branch(&alternate(Some("w14 wps"), true)), choice);
    assert_eq!(branch(&alternate(Some("x"), true)), fallback);
    assert_eq!(branch(&alternate(Some("w14 x"), true)), fallback);
    // a prefix that is not declared names nothing
    assert_eq!(branch(&alternate(Some("nosuchprefix"), true)), fallback);
    // no Requires, or one that names nothing, is a choice that cannot be taken
    assert_eq!(branch(&alternate(None, true)), fallback);
    assert_eq!(branch(&alternate(Some(""), true)), fallback);
    assert_eq!(branch(&alternate(Some("  \t "), true)), fallback);
    // and with no fallback the wrapper stands for nothing
    assert_eq!(branch(&alternate(Some("x"), false)), None);
    assert_eq!(branch(&alternate(Some(""), false)), None);
    assert_eq!(branch(&alternate(Some("wps"), false)), choice);
}

#[test]
fn the_namespaces_word_writes_in_requires_are_known() {
    // Chart extensions of Word 2016 and later, and ink of Word 2010: Word reads the choice. The
    // SDK data lists none of them, and the generator adds them (supplement/rules.rs). Namespaces
    // that only a pre-release Word, Word for Mac or another producer writes are not added: the
    // fallback is what is read.
    let with = |uri: &str| {
        doc_xml(&format!(
            r#"<w:p><mc:AlternateContent><mc:Choice Requires="n" xmlns:n="{uri}"><w:r><w:t>choice</w:t></w:r></mc:Choice><mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback></mc:AlternateContent></w:p>"#
        ))
    };
    let choice = Some("Choice".to_string());
    let fallback = Some("Fallback".to_string());
    for uri in [
        "http://schemas.microsoft.com/office/drawing/2014/chartex",
        "http://schemas.microsoft.com/office/drawing/2015/9/8/chartex",
        "http://schemas.microsoft.com/office/drawing/2015/10/21/chartex",
        "http://schemas.microsoft.com/office/drawing/2016/5/10/chartex",
        "http://schemas.microsoft.com/office/word/2010/wordprocessingInk",
    ] {
        assert_eq!(branch(&with(uri)), choice, "{uri}");
    }
    for uri in [
        "http://schemas.microsoft.com/office/word/2008/6/28/wordprocessingShape",
        "http://schemas.microsoft.com/office/mac/drawingml/2008/main",
        "http://www.wps.cn/officeDocument/2013/wpsCustomData",
    ] {
        assert_eq!(branch(&with(uri)), fallback, "{uri}");
    }
}

#[test]
fn the_first_choice_that_can_be_taken_is_the_one() {
    let xml = doc_xml(
        r#"<w:p><mc:AlternateContent>
             <mc:Choice Requires="x"><w:r><w:t>unknown</w:t></w:r></mc:Choice>
             <mc:Choice Requires="w14"><w:r><w:t>second</w:t></w:r></mc:Choice>
             <mc:Choice Requires="wps"><w:r><w:t>third</w:t></w:r></mc:Choice>
             <mc:Fallback><w:r><w:t>fallback</w:t></w:r></mc:Fallback>
           </mc:AlternateContent></w:p>"#,
    );
    let tree = parse(&xml);
    let taken = taken_branch(&node(&tree, "AlternateContent", 0)).expect("a branch");
    assert_eq!(taken.text_content(), "second");
}

#[test]
fn an_ignorable_element_is_skipped_unless_process_content_keeps_its_children() {
    let doc = |root_attrs: &str, inner: &str| {
        format!(
            r#"<w:document {NS_DECLS} {root_attrs}><w:body><w:p>{inner}</w:p></w:body></w:document>"#
        )
    };
    let read = |xml: &str, local: &str| {
        let tree = parse(xml);
        match disposition(&node(&tree, local, 0)) {
            Mc::Normal => "normal",
            Mc::Skip => "skip",
            Mc::ProcessContent => "process",
            Mc::Branch(_) => "branch",
            Mc::NoBranch => "no branch",
        }
    };
    let note = "<x:note><w:r/></x:note>";
    // a name the tables know, in a namespace they understand, is read as it stands, whatever the
    // document says; so is an element nothing makes ignorable
    assert_eq!(read(&doc(r#"mc:Ignorable="x""#, note), "p"), "normal");
    assert_eq!(read(&doc("", note), "note"), "normal");
    // ignorable, from the root, from an ancestor, or from the element itself
    assert_eq!(read(&doc(r#"mc:Ignorable="x""#, note), "note"), "skip");
    let on_p = r#"<x:note mc:Ignorable="x"><w:r/></x:note>"#;
    assert_eq!(read(&doc("", on_p), "note"), "skip");
    let under = r#"<w:r mc:Ignorable="x"><x:note/></w:r>"#;
    assert_eq!(read(&doc("", under), "note"), "skip");
    // listed in mc:ProcessContent, by name or with a wildcard, on the root or on the element
    let listed = r#"mc:Ignorable="x" mc:ProcessContent="x:note""#;
    assert_eq!(read(&doc(listed, note), "note"), "process");
    let wild = r#"mc:Ignorable="x" mc:ProcessContent="x:*""#;
    assert_eq!(read(&doc(wild, note), "note"), "process");
    let own = r#"<x:note mc:Ignorable="x" mc:ProcessContent="x:note"><w:r/></x:note>"#;
    assert_eq!(read(&doc("", own), "note"), "process");
    // listed for another name: skipped
    let other = r#"mc:Ignorable="x" mc:ProcessContent="x:wrap""#;
    assert_eq!(read(&doc(other, note), "note"), "skip");
    // an mc:AlternateContent is a branch, or nothing
    assert_eq!(
        read(&alternate(Some("wps"), true), "AlternateContent"),
        "branch"
    );
    assert_eq!(
        read(&alternate(Some("x"), false), "AlternateContent"),
        "no branch"
    );
}

/// What a reader that holds `NodeRef`s reads below `node`: it asks `disposition` of each child
/// and follows the answer, as `wordml` does.
fn read_children<'a>(node: NodeRef<'a>, out: &mut Vec<NodeRef<'a>>) {
    for child in node.child_elements() {
        match disposition(&child) {
            Mc::Normal | Mc::Skip => out.push(child),
            Mc::Branch(branch) => read_children(branch, out),
            Mc::NoBranch => {}
            Mc::ProcessContent => read_children(child, out),
        }
    }
}

/// The children of every element of `xml`, as `effective_children` (what insertion and the
/// validator read) and as `read_children` (what `wordml` reads) give them.
fn assert_the_readers_agree(xml: &str) {
    let tree = parse(xml);
    for parent in elements(&tree) {
        let mut scope = scope_at(&tree, parent, true);
        let eff = effective_children(&tree, parent, &mut scope.chain, scope.ign);
        let by_schema: Vec<NodeId> = eff.items.iter().map(|i| i.node).collect();
        let mut by_wordml = Vec::new();
        read_children(tree.node_ref(parent), &mut by_wordml);
        let by_wordml: Vec<NodeId> = by_wordml.iter().map(|n| n.id()).collect();
        assert_eq!(
            by_schema,
            by_wordml,
            "the children of {} differ in {xml}",
            tree.name(parent).1
        );
        // and the ones the schema reads are the ones it says each item stands in the place of
        assert!(eff.items.iter().all(|i| i.from < eff.raw));
    }
}

#[test]
fn nesting_has_no_depth_limit() {
    // 60 alternate contents, each inside the choice of the one before: the element in the
    // middle is the child the paragraph properties read. A depth limit of eight, as there once
    // was, would have stopped at a wrapper.
    let mut inner = r#"<w:jc w:val="left"/>"#.to_string();
    for _ in 0..60 {
        inner = format!(
            r#"<mc:AlternateContent><mc:Choice Requires="w14">{inner}</mc:Choice><mc:Fallback><w:ind w:left="1"/></mc:Fallback></mc:AlternateContent>"#
        );
    }
    let xml = doc_xml(&format!("<w:p><w:pPr>{inner}</w:pPr></w:p>"));
    assert_the_readers_agree(&xml);
    let tree = parse(&xml);
    let ppr = nth(&tree, "pPr", 0);
    let mut scope = scope_at(&tree, ppr, true);
    let eff = effective_children(&tree, ppr, &mut scope.chain, scope.ign);
    let names: Vec<&str> = eff.items.iter().map(|i| tree.name(i.node).1).collect();
    assert_eq!(names, ["jc"]);
    // so insertion, which reads the same sequence, puts an ind before the wrapper that stands for
    // the jc, and the validator is content with the result
    let got = place_in_xml(PartKind::Document, &xml, "pPr", w("ind"), Prefer::Last);
    assert_eq!(got.fit, Fit::Valid);
    assert_eq!(got.index, 0);
    assert!(violations(PartKind::Document, &xml).is_empty());
}

#[test]
fn a_wrapper_that_declares_what_is_ignorable_below_it_does_so_for_the_validator_too() {
    // x:outer is listed in mc:ProcessContent on itself, and declares y ignorable and y:inner
    // listed. y:inner is ignorable only through x:outer, which is above it, so it is read as a
    // wrapper and the jc in it is the paragraph's child.
    let xml = format!(
        r#"<w:document {NS_DECLS} mc:Ignorable="x"><w:body><w:p><w:pPr>
             <x:outer xmlns:y="urn:example:other" mc:Ignorable="y" mc:ProcessContent="x:outer y:inner">
               <y:inner><w:jc w:val="left"/></y:inner>
             </x:outer></w:pPr></w:p></w:body></w:document>"#
    );
    assert_the_readers_agree(&xml);
    let tree = parse(&xml);
    let inner = node(&tree, "inner", 0);
    assert_eq!(disposition(&inner), Mc::ProcessContent);
    // the validator reads the jc, finds nothing out of place and does not report y:inner
    assert!(
        violations(PartKind::Document, &xml).is_empty(),
        "{:?}",
        kinds(&violations(PartKind::Document, &xml))
    );
    // and an ind goes before all of it
    let got = place_in_xml(PartKind::Document, &xml, "pPr", w("ind"), Prefer::Last);
    assert_eq!(
        got,
        Placement {
            index: 0,
            fit: Fit::Valid
        }
    );
}

/// A small generator of nested wrappers: an `mc:AlternateContent` whose choices require known or
/// unknown namespaces, elements of a namespace the document makes ignorable and sometimes
/// processes, and the markup compatibility attributes on the elements that carry them.
pub(in crate::schema) struct Gen(pub u64);

impl Gen {
    pub(in crate::schema) fn below(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }

    fn attrs(&mut self) -> String {
        let ignorable = match self.below(4) {
            0 => r#" mc:Ignorable="x""#,
            _ => "",
        };
        let process = match self.below(5) {
            0 => r#" mc:ProcessContent="x:wrap""#,
            1 => r#" mc:ProcessContent="x:*""#,
            _ => "",
        };
        format!("{ignorable}{process}")
    }

    fn children(&mut self, depth: usize, out: &mut String) {
        for _ in 0..1 + self.below(3) {
            match self.below(9) {
                0 | 1 => out.push_str(r#"<w:jc w:val="left"/>"#),
                2 => out.push_str("<x:note/>"),
                3 => out.push_str(r#"<w:ind w:left="0"/>"#),
                4 | 5 if depth < 6 => {
                    let attrs = self.attrs();
                    out.push_str(&format!("<x:wrap{attrs}>"));
                    self.children(depth + 1, out);
                    out.push_str("</x:wrap>");
                }
                6 | 7 if depth < 6 => {
                    let attrs = self.attrs();
                    out.push_str(&format!("<mc:AlternateContent{attrs}>"));
                    for _ in 0..1 + self.below(2) {
                        let requires = ["w14", "x", "", "w14 x", "wps"][self.below(5) as usize];
                        let attrs = self.attrs();
                        out.push_str(&format!(r#"<mc:Choice Requires="{requires}"{attrs}>"#));
                        self.children(depth + 1, out);
                        out.push_str("</mc:Choice>");
                    }
                    if self.below(3) != 0 {
                        out.push_str("<mc:Fallback>");
                        self.children(depth + 1, out);
                        out.push_str("</mc:Fallback>");
                    }
                    out.push_str("</mc:AlternateContent>");
                }
                _ => out.push_str(r#"<w:pStyle w:val="A"/>"#),
            }
        }
    }
}

impl Gen {
    /// A part whose paragraph properties hold random wrappers, with the markup compatibility
    /// attributes on the root, on the properties and on the wrappers.
    pub(in crate::schema) fn part(&mut self) -> String {
        let root_attrs = self.attrs();
        let ppr_attrs = self.attrs();
        let mut inner = String::new();
        self.children(0, &mut inner);
        format!(
            r#"<w:document {NS_DECLS}{root_attrs}><w:body><w:p><w:pPr{ppr_attrs}>{inner}</w:pPr></w:p></w:body></w:document>"#
        )
    }
}

#[test]
fn reading_and_validating_agree_on_random_nestings() {
    let mut rng = Gen(0x2545_F491_4F6C_DD1D);
    for _ in 0..400 {
        assert_the_readers_agree(&rng.part());
    }
}
