//! `undeclared_prefixes` and `prefix_namespace` (decision 0014): a prefix that a markup-compatibility attribute names is
//! found wherever no declaration in scope binds it, on the root or on any element below, in each of the three attributes.

use super::*;

const MC_DECL: &str = r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006""#;

fn found(xml: &str) -> Vec<(String, Vec<(&'static str, String)>)> {
    let tree = parse(xml);
    undeclared_prefixes(&tree)
        .into_iter()
        .map(|u| (tree.node_ref(u.node).local_name().to_string(), u.names))
        .collect()
}

fn names(list: &[(&'static str, &str)]) -> Vec<(&'static str, String)> {
    list.iter().map(|(a, p)| (*a, (*p).to_string())).collect()
}

#[test]
fn an_ignorable_prefix_on_the_root_that_nothing_declares_is_found() {
    let xml = format!(
        r#"<w:document xmlns:w="{W}" {MC_DECL} mc:Ignorable="w14 wp14"><w:body/></w:document>"#
    );
    assert_eq!(
        found(&xml),
        vec![(
            "document".to_string(),
            names(&[("Ignorable", "w14"), ("Ignorable", "wp14")])
        )]
    );
}

#[test]
fn a_declared_prefix_is_not_found_and_a_declaration_on_an_ancestor_counts() {
    let xml = format!(
        r#"<w:document xmlns:w="{W}" xmlns:w14="{W14}" {MC_DECL} mc:Ignorable="w14"><w:body><w:p mc:Ignorable="w14"/></w:body></w:document>"#
    );
    assert!(found(&xml).is_empty());
}

#[test]
fn process_content_and_must_understand_on_an_inner_element_are_read_too() {
    let xml = format!(
        r#"<w:document xmlns:w="{W}" {MC_DECL}><w:body><w:p mc:Ignorable="" mc:ProcessContent="v:shape w:x" mc:MustUnderstand="wps"/></w:body></w:document>"#
    );
    assert_eq!(
        found(&xml),
        vec![(
            "p".to_string(),
            names(&[("ProcessContent", "v"), ("MustUnderstand", "wps")])
        )]
    );
}

#[test]
fn a_declaration_out_of_scope_does_not_count() {
    // `w14` is declared on the first paragraph only, so the second one names it undeclared.
    let xml = format!(
        r#"<w:document xmlns:w="{W}" {MC_DECL}><w:body><w:p xmlns:w14="{W14}" mc:Ignorable="w14"/><w:p mc:Ignorable="w14"/></w:body></w:document>"#
    );
    let got = found(&xml);
    assert_eq!(got.len(), 1, "{got:?}");
}

#[test]
fn the_namespace_of_a_prefix_is_the_one_word_writes() {
    assert_eq!(prefix_namespace("w14"), Some(W14));
    // The tables call this namespace w16cur; Word writes w16.
    assert_eq!(
        prefix_namespace("w16"),
        Some("http://schemas.microsoft.com/office/word/2018/wordml")
    );
    // The tables number the chart extension namespaces of 2016 one lower than Word does.
    assert_eq!(
        prefix_namespace("cx3"),
        Some("http://schemas.microsoft.com/office/drawing/2016/5/9/chartex")
    );
    // A preferred prefix of the tables that Word's list does not have.
    assert_eq!(
        prefix_namespace("w16cur"),
        Some("http://schemas.microsoft.com/office/word/2018/wordml")
    );
    assert_eq!(prefix_namespace("zz"), None);
    assert_eq!(prefix_namespace("xml"), None);
    assert_eq!(prefix_namespace(""), None);
}
