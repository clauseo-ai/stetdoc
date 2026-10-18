//! Clauses: computed and typed labels, composition across lists, and resolving a query (design 7.2).

use super::testing::*;
use super::{ClauseMatch, Index, Source};
use std::sync::Arc;

fn index_of(f: &Fixture) -> Arc<Index> {
    f.state().parts().doc_index().unwrap()
}

type Row = (String, String, Source, bool, u8);

fn rows(f: &Fixture) -> Vec<Row> {
    index_of(f)
        .clauses()
        .iter()
        .map(|c| {
            (
                c.label.clone(),
                c.full.clone(),
                c.source,
                c.composed,
                c.depth,
            )
        })
        .collect()
}

fn typed(label: &str, text: &str) -> String {
    p(&format!("{label} {text}"))
}

fn numbered(num: u32, level: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{num}\"/></w:numPr></w:pPr>{}</w:p>",
        r(text)
    )
}

#[test]
fn word_numbering_gives_the_label_and_the_label_with_its_ancestors() {
    let body = format!(
        "{}{}{}{}",
        numbered(1, 0, "Definitions"),
        numbered(1, 1, "Affiliate"),
        numbered(1, 1, "Control"),
        numbered(1, 0, "Payment")
    );
    let got = rows(&Fixture::new(&body).numbering());
    let want = |l: &str, f: &str, composed, depth| {
        (
            l.to_string(),
            f.to_string(),
            Source::Computed,
            composed,
            depth,
        )
    };
    assert_eq!(
        got,
        vec![
            want("1.", "1", false, 1),
            want("(a)", "1(a)", false, 2),
            want("(b)", "1(b)", false, 2),
            want("2.", "2", false, 1),
        ]
    );
}

#[test]
fn a_typed_list_of_sub_items_is_composed_under_the_heading_it_follows() {
    let body = format!(
        "{}{}{}{}{}{}{}{}{}",
        typed("4.1", "Price"),
        typed("4.2", "Payment"),
        typed("(a)", "in cash"),
        typed("(b)", "on time"),
        typed("(i)", "by wire"),
        typed("4.3", "Interest"),
        typed("(a)", "simple"),
        typed("(b)", "never compound"),
        p("A paragraph with no label.")
    );
    let got: Vec<(String, String, bool, u8)> = rows(&Fixture::new(&body))
        .into_iter()
        .map(|(_, full, source, composed, depth)| {
            assert_eq!(source, Source::Typed);
            (String::new(), full, composed, depth)
        })
        .collect();
    let fulls: Vec<&str> = got.iter().map(|g| g.1.as_str()).collect();
    assert_eq!(
        fulls,
        vec![
            "4.1",
            "4.2",
            "4.2(a)",
            "4.2(b)",
            "4.2(b)(i)",
            "4.3",
            "4.3(a)",
            "4.3(b)"
        ]
    );
    let depths: Vec<u8> = got.iter().map(|g| g.3).collect();
    assert_eq!(depths, vec![1, 1, 2, 2, 3, 1, 2, 2]);
    assert!(!got[0].2 && got[2].2 && got[4].2, "composed is flagged");
}

#[test]
fn computed_headings_and_a_typed_list_under_them_compose_across_the_two_lists() {
    let body = format!(
        "{}{}{}{}{}{}",
        numbered(1, 0, "Definitions"),
        typed("(a)", "first"),
        typed("(b)", "second"),
        numbered(1, 0, "Payment"),
        typed("(a)", "again first"),
        p("text")
    );
    let got: Vec<String> = rows(&Fixture::new(&body).numbering())
        .into_iter()
        .map(|r| r.1)
        .collect();
    assert_eq!(got, vec!["1", "1(a)", "1(b)", "2", "2(a)"]);
}

#[test]
fn a_lone_i_after_h_is_a_letter_and_otherwise_a_numeral() {
    let body = format!(
        "{}{}{}{}{}{}{}{}",
        typed("1.", "Heading"),
        typed("(g)", "g"),
        typed("(h)", "h"),
        typed("(i)", "i the letter"),
        typed("(j)", "j"),
        typed("(a)", "a again"),
        typed("(i)", "i the numeral, under (a)"),
        typed("(ii)", "ii")
    );
    let got = rows(&Fixture::new(&body));
    let fulls: Vec<&str> = got.iter().map(|r| r.1.as_str()).collect();
    assert_eq!(
        fulls,
        [
            "1", "1(g)", "1(h)", "1(i)", "1(j)", "1(a)", "1(a)(i)", "1(a)(ii)"
        ]
    );
    let depths: Vec<u8> = got.iter().map(|r| r.4).collect();
    assert_eq!(depths, [1, 2, 2, 2, 2, 2, 3, 3]);
}

#[test]
fn a_noun_before_the_label_is_kept_and_helps_the_query() {
    let body = format!(
        "{}{}{}{}",
        typed("Article", "I: Definitions"),
        typed("Article", "II: Payment"),
        typed("Section", "2.01 Price"),
        typed("Schedule", "A Fees")
    );
    let f = Fixture::new(&body);
    let idx = index_of(&f);
    let nouns: Vec<Option<&str>> = idx.clauses().iter().map(|c| c.noun).collect();
    assert_eq!(
        nouns,
        vec![
            Some("Article"),
            Some("Article"),
            Some("Section"),
            Some("Schedule")
        ]
    );
    let hit = |q: &str| -> Vec<String> {
        idx.resolve_clause(q)
            .into_iter()
            .map(|m| idx.clause(m.at).unwrap().full.clone())
            .collect()
    };
    assert_eq!(hit("Article II"), vec!["II"]);
    assert_eq!(hit("article ii"), vec!["II"]);
    assert_eq!(hit("Section 2.01"), vec!["2.01"]);
    assert_eq!(hit("Schedule A"), vec!["A"]);
    assert_eq!(
        hit("clause 2.01"),
        vec!["2.01"],
        "a noun that does not match is a rank and not a filter"
    );
    assert!(hit("Article IX").is_empty());
}

#[test]
fn a_query_is_a_path_with_or_without_a_noun_and_a_sentence_is_not_a_query() {
    let body = format!(
        "{}{}{}",
        typed("4.2", "Payment"),
        typed("(a)", "cash"),
        typed("(ii)", "wire")
    );
    let f = Fixture::new(&body);
    let idx = index_of(&f);
    let at = |q: &str| -> Vec<usize> {
        idx.resolve_clause(q)
            .into_iter()
            .map(|m: ClauseMatch| m.at)
            .collect()
    };
    assert_eq!(at("clause 4.2(a)"), vec![1]);
    assert_eq!(at("4.2(a)"), vec![1]);
    assert_eq!(at("Section 4.2 (a)."), vec![1]);
    assert_eq!(at("4.2"), vec![0]);
    assert_eq!(at("4.2(a)(ii)"), vec![2]);
    assert!(at("pay the price").is_empty());
    assert!(at("clause").is_empty());
    assert!(at("").is_empty());
}

#[test]
fn a_label_that_two_clauses_have_gives_both_and_the_reason_is_stated() {
    let body = format!(
        "{}{}{}{}",
        typed("1.", "First"),
        typed("(a)", "x"),
        typed("1.", "Again"),
        typed("(a)", "y")
    );
    let idx = index_of(&Fixture::new(&body));
    let m = idx.resolve_clause("1(a)");
    assert_eq!(m.len(), 2);
    assert!(m.iter().all(|m| m.reason.words() == "the full label"));
}

#[test]
fn i1_and_i2_every_clause_is_a_labelled_paragraph_and_resolves_from_its_own_label() {
    let body = format!(
        "{}{}{}{}{}",
        numbered(1, 0, "Definitions"),
        numbered(1, 1, "Affiliate"),
        typed("(i)", "typed under a computed item"),
        typed("2.1", "typed heading"),
        p("no label here")
    );
    let f = Fixture::new(&body).numbering();
    let idx = index_of(&f);
    assert_eq!(
        idx.clauses().len(),
        4,
        "the paragraph with no label is not a clause"
    );
    for (at, c) in idx.clauses().iter().enumerate() {
        let hits: Vec<usize> = idx
            .resolve_clause(&c.full)
            .into_iter()
            .map(|m| m.at)
            .collect();
        assert!(
            hits.contains(&at),
            "{} does not resolve to itself: {hits:?}",
            c.full
        );
        assert_eq!(idx.clause_of(&c.block).map(|x| &x.full), Some(&c.full));
    }
}

#[test]
fn the_blocks_under_a_clause_run_to_the_next_clause_of_the_same_or_a_shallower_depth() {
    let body = format!(
        "{}{}{}{}{}{}",
        typed("1.", "One"),
        p("text under one"),
        typed("(a)", "sub"),
        p("text under sub"),
        typed("2.", "Two"),
        p("text under two")
    );
    let idx = index_of(&Fixture::new(&body));
    let ends: Vec<Option<usize>> = (0..idx.clauses().len()).map(|i| idx.ends_at(i)).collect();
    assert_eq!(ends, vec![Some(4), Some(4), None]);
}

#[test]
fn a_division_starts_the_document_over_and_a_heading_ends_the_sub_items_before_it() {
    let body = format!(
        "{}{}{}{}{}{}{}{}{}",
        typed("(a)", "a recital"),
        typed("(b)", "another recital"),
        typed("Article", "I Definitions"),
        typed("Section", "1.01 Terms"),
        typed("(a)", "first"),
        typed("(b)", "second"),
        typed("Article", "II Payment"),
        typed("Section", "2.01 Price"),
        typed("(a)", "price")
    );
    let got: Vec<String> = rows(&Fixture::new(&body))
        .into_iter()
        .map(|r| r.1)
        .collect();
    assert_eq!(
        got,
        vec![
            "(a)", "(b)", "I", "1.01", "1.01(a)", "1.01(b)", "II", "2.01", "2.01(a)"
        ]
    );
}

#[test]
fn the_paragraphs_of_a_numbered_article_can_be_cited_the_way_eu_instruments_do() {
    let body = format!(
        "{}{}{}{}{}",
        typed("Article", "6 Classification"),
        typed("1.", "Where a system is high risk"),
        typed("(a)", "it is a safety component"),
        typed("2.", "In addition"),
        typed("3.", "Delegated acts")
    );
    let idx = index_of(&Fixture::new(&body));
    let hit = |q: &str| -> Vec<usize> { idx.resolve_clause(q).into_iter().map(|m| m.at).collect() };
    assert_eq!(hit("Article 6(1)"), vec![1]);
    assert_eq!(hit("Article 6(1)(a)"), vec![2]);
    assert_eq!(hit("Article 6(2)"), vec![3]);
    assert_eq!(
        hit("1(a)"),
        vec![2],
        "the label as the document numbers it still works"
    );
    let aliases: Vec<Option<&str>> = idx.clauses().iter().map(|c| c.alias.as_deref()).collect();
    assert_eq!(
        aliases,
        vec![
            None,
            Some("6(1)"),
            Some("6(1)(a)"),
            Some("6(2)"),
            Some("6(3)")
        ]
    );
}

#[test]
fn the_lines_of_a_table_of_contents_are_not_clauses() {
    let field = |instr: &str, inner: &str, last: bool| {
        format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {instr} </w:instrText></w:r>\
             <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{inner}{}",
            if last {
                "<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
            } else {
                ""
            }
        )
    };
    let body = format!(
        "{}{}{}{}{}",
        px(&field("TOC \\o \"1-3\"", &r("Section 1.01 Terms 3"), false)),
        px(&r("Section 1.02 Price 4")),
        px(&format!(
            "{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            r("Section 1.03 Closing 5")
        )),
        typed("Section", "1.01 Terms"),
        typed("Section", "1.02 Price")
    );
    let got: Vec<String> = rows(&Fixture::new(&body))
        .into_iter()
        .map(|r| r.1)
        .collect();
    assert_eq!(
        got,
        vec!["1.01", "1.02"],
        "only the headings, not their entries in the contents"
    );
    let _ = field;
}

#[test]
fn a_clause_is_heading_text_a_reader_recognizes() {
    let body = format!(
        "{}{}{}{}",
        typed(
            "Section",
            "4.2 Payment of the Price, in full and on time, by the Buyer to the Seller"
        ),
        typed("(a)", "in cash"),
        numbered(1, 0, "Definitions"),
        numbered(1, 1, "Affiliate")
    );
    let idx = index_of(&Fixture::new(&body).numbering());
    let h: Vec<String> = idx.clauses().iter().map(|c| c.heading()).collect();
    assert_eq!(
        h[0],
        "Section 4.2 Payment of the Price, in full and on\u{2026}"
    );
    assert_eq!(h[1], "Section 4.2(a) in cash");
    assert_eq!(h[2], "1 Definitions");
    assert_eq!(h[3], "1(a) Affiliate");
}

#[test]
fn a_noun_is_found_in_any_case_with_the_punctuation_of_a_phrase_and_nothing_else_is() {
    use super::labels::Noun;
    for (word, noun) in [
        ("Section", Noun::Section),
        ("SECTIONS", Noun::Section),
        ("sec.", Noun::Section),
        ("§", Noun::Section),
        ("§§", Noun::Section),
        ("clause,", Noun::Clause),
        ("Subclauses", Noun::Clause),
        ("CL", Noun::Clause),
        ("Paragraph:", Noun::Paragraph),
        ("subparagraphs", Noun::Paragraph),
        ("Article", Noun::Article),
        ("Schedules;", Noun::Schedule),
        ("exhibit", Noun::Exhibit),
        ("Part", Noun::Part),
        ("Annexure", Noun::Annex),
        ("Annexes", Noun::Annex),
        ("Appendices", Noun::Appendix),
        ("Recital", Noun::Recital),
    ] {
        assert_eq!(Noun::of(word), Some(noun), "{word}");
    }
    for word in [
        "",
        "a",
        "s",
        "Sectionx",
        "xSection",
        "Clauses)",
        "Çlause",
        "particular",
        "Sec.tion",
        "sectionssectionss",
        "§§§",
        "\u{a7}x",
    ] {
        assert_eq!(Noun::of(word), None, "{word:?}");
    }
}
