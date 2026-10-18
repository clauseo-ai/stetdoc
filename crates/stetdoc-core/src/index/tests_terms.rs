//! Defined terms through the index: where they are defined across stories, twice-defined terms, and uses.

use super::terms::DefKind;
use super::testing::*;

fn defs(f: &Fixture) -> Vec<(String, DefKind, String)> {
    let doc = f.state();
    let parts = doc.parts();
    let index = parts.doc_index().unwrap();
    index
        .terms()
        .flat_map(|t| {
            index
                .definitions(t)
                .iter()
                .map(|d| {
                    (
                        d.term.clone(),
                        d.kind,
                        parts.ref_of(&d.block).unwrap().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn terms_are_listed_in_order_with_the_paragraph_that_defines_them() {
    let body = format!(
        "{}{}{}{}",
        p("This Agreement is between Acme Inc. (the \"Seller\") and Beta LLC (the \"Buyer\")."),
        p("1. \"Affiliate\" means any person under common control."),
        p("The Seller pays the Buyer."),
        p("\"Losses\" and \"Costs\" shall have the meanings in Schedule 1.")
    );
    let got = defs(&Fixture::new(&body));
    assert_eq!(
        got,
        vec![
            (
                "Affiliate".to_string(),
                DefKind::Paragraph,
                "p2".to_string()
            ),
            (
                "Buyer".to_string(),
                DefKind::Parenthetical,
                "p1".to_string()
            ),
            ("Costs".to_string(), DefKind::Paragraph, "p4".to_string()),
            ("Losses".to_string(), DefKind::Paragraph, "p4".to_string()),
            (
                "Seller".to_string(),
                DefKind::Parenthetical,
                "p1".to_string()
            ),
        ],
        "sorted by term, and a list of quoted terms at the start of a paragraph defines each"
    );
}

#[test]
fn a_term_defined_twice_lists_both_sites() {
    let body = format!(
        "{}{}",
        p("\"Business Day\" means a day banks are open."),
        p("In this clause (a \"Business Day\") means something else.")
    );
    let doc = Fixture::new(&body).state();
    let parts = doc.parts();
    let index = parts.doc_index().unwrap();
    let d = index.definitions("Business Day");
    assert_eq!(d.len(), 2);
    assert_eq!(
        (d[0].kind, d[1].kind),
        (DefKind::Paragraph, DefKind::Parenthetical)
    );
}

#[test]
fn the_notes_and_the_headers_are_indexed_and_the_comments_are_not() {
    let body = p("The body text.");
    let f = Fixture::new(&body)
        .footnotes("<w:footnote w:id=\"1\"><w:p><w:r><w:t>\"Note Term\" means a defined thing.</w:t></w:r></w:p></w:footnote>")
        .comments("<w:comment w:id=\"0\" w:author=\"JD\"><w:p><w:r><w:t>\"Comment Term\" means nothing.</w:t></w:r></w:p></w:comment>");
    let got = defs(&f);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, "Note Term");
}

#[test]
fn text_that_only_mentions_a_quoted_word_defines_nothing() {
    let body = format!(
        "{}{}{}",
        p("The word \"Seller\" appears in quotes."),
        p("He said \"means\" twice."),
        p("(see the \"Schedule\" for details)")
    );
    assert!(defs(&Fixture::new(&body)).is_empty());
}

#[test]
fn clean_text_is_borrowed_and_the_rest_is_made_clean() {
    use super::clean;
    use std::borrow::Cow;
    for same in ["", "a", "a b", "A “quoted” word and § 4"] {
        assert!(
            matches!(clean(same), Cow::Borrowed(t) if t == same),
            "{same:?}"
        );
    }
    // Only spaces at the ends: borrowed, and trimmed.
    for (text, want) in [
        (" a b", "a b"),
        ("a b ", "a b"),
        ("  a b  ", "a b"),
        (" ", ""),
        ("   ", ""),
    ] {
        assert!(
            matches!(clean(text), Cow::Borrowed(t) if t == want),
            "{text:?}"
        );
    }
    // What changes: runs of white space, other white space, the atom characters.
    for (text, want) in [
        ("a  b", "a b"),
        ("a\tb", "a b"),
        ("a\u{a0}b", "a b"),
        ("\ta b\n", "a b"),
        ("a\u{1}b", "ab"),
        ("a \u{1} b", "a b"),
        ("a\u{c}b\u{e}c", "abc"),
        ("a\u{2003}b", "a b"),
        ("é  é", "é é"),
    ] {
        assert!(
            matches!(clean(text), Cow::Owned(ref t) if t == want),
            "{text:?}"
        );
    }
}
