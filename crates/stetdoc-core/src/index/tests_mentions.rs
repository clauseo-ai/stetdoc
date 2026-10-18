//! Edges: fields, internal links and prose mentions, and what each resolves to (design 7.4).

use super::testing::*;
use super::{BlockKey, Edge, EdgeKind, State};
use crate::state::State as Doc;

fn numbered(level: u32, text: &str) -> String {
    format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>{}</w:p>",
        r(text)
    )
}

fn bookmarked(id: u32, name: &str, text: &str) -> String {
    px(&format!(
        "<w:bookmarkStart w:id=\"{id}\" w:name=\"{name}\"/>{}<w:bookmarkEnd w:id=\"{id}\"/>",
        r(text)
    ))
}

fn field(instr: &str, result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {instr} </w:instrText></w:r>\
         <w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r(result)
    )
}

/// The edges of every paragraph, as (ref of the citing paragraph, kind, text, state, refs of the targets).
fn edges_of(doc: &Doc) -> Vec<(String, EdgeKind, String, State, Vec<String>)> {
    let parts = doc.parts();
    let edges = parts.edges().unwrap();
    let ref_of = |k: &BlockKey| parts.ref_of(k).unwrap().to_string();
    let mut out = Vec::new();
    for story in parts.story_specs().unwrap().0.iter() {
        let part = parts.id(&story.part).unwrap();
        let s = parts.story(part).unwrap();
        for entry in s.blocks() {
            let key = s.key(entry);
            for e in edges.outbound(&key) {
                let e: &Edge = e;
                out.push((
                    ref_of(&e.from),
                    e.kind,
                    e.text.clone(),
                    e.state,
                    e.to.iter().map(ref_of).collect(),
                ));
            }
        }
    }
    out
}

#[test]
fn a_ref_field_and_a_link_name_the_paragraph_of_their_bookmark() {
    let body = format!(
        "{}{}{}",
        bookmarked(1, "_Ref100", "The payment clause"),
        px(&format!(
            "{}{}",
            r("As in "),
            field("REF _Ref100 \\r \\h", "4.2")
        )),
        px(&format!(
            "<w:hyperlink w:anchor=\"_Ref100\">{}</w:hyperlink><w:hyperlink w:anchor=\"_Gone\">{}</w:hyperlink>",
            r("see above"),
            r("elsewhere")
        ))
    );
    let doc = Fixture::new(&body).state();
    let got = edges_of(&doc);
    let kinds: Vec<_> = got
        .iter()
        .map(|e| (e.0.as_str(), e.1, e.3, e.4.clone()))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (
                "p2",
                EdgeKind::Field,
                State::Resolved,
                vec!["p1".to_string()]
            ),
            (
                "p3",
                EdgeKind::Link,
                State::Resolved,
                vec!["p1".to_string()]
            ),
            ("p3", EdgeKind::Link, State::Unresolved, Vec::new()),
        ]
    );
    assert_eq!(got[0].2, "REF _Ref100 \\r \\h");
    assert_eq!(got[1].2, "#_Ref100");
}

#[test]
fn page_and_note_fields_and_a_hyperlink_with_a_target_name_edges_too() {
    let body = format!(
        "{}{}{}",
        bookmarked(1, "Target", "Here"),
        px(&format!(
            "{}{}{}{}",
            field("PAGEREF Target \\h", "3"),
            field("NOTEREF Target", "1"),
            field("HYPERLINK \\l \"Target\"", "go"),
            field("PAGE", "1")
        )),
        px(&field("REF Missing", "Error!"))
    );
    let got = edges_of(&Fixture::new(&body).state());
    let states: Vec<_> = got.iter().map(|e| (e.0.as_str(), e.3, e.4.len())).collect();
    assert_eq!(
        states,
        vec![
            ("p2", State::Resolved, 1),
            ("p2", State::Resolved, 1),
            ("p2", State::Resolved, 1),
            ("p3", State::Unresolved, 0)
        ],
        "PAGE is not an edge"
    );
}

#[test]
fn a_prose_mention_resolves_to_the_clause_it_names() {
    let body = format!(
        "{}{}{}{}",
        numbered(0, "Definitions"),
        numbered(0, "Payment"),
        numbered(1, "in cash"),
        p("Subject to clause 2(a) and Section 9 and clause 1, the Buyer pays.")
    );
    let doc = Fixture::new(&body).numbering().state();
    let got = edges_of(&doc);
    let prose: Vec<_> = got
        .iter()
        .filter(|e| e.1 == EdgeKind::Prose)
        .map(|e| (e.0.as_str(), e.2.as_str(), e.3, e.4.clone()))
        .collect();
    assert_eq!(
        prose,
        vec![
            ("p4", "clause 2(a)", State::Resolved, vec!["p3".to_string()]),
            ("p4", "Section 9", State::Unresolved, Vec::new()),
            ("p4", "clause 1", State::Resolved, vec!["p1".to_string()]),
        ]
    );
}

#[test]
fn a_bare_sub_item_is_relative_to_the_clause_that_cites_it_and_is_never_called_resolved() {
    let body = format!(
        "{}{}{}{}{}{}",
        numbered(0, "Definitions"),
        numbered(1, "first"),
        numbered(0, "Payment"),
        numbered(1, "in cash"),
        numbered(1, "by wire, as in paragraph (a) above"),
        numbered(1, "or see paragraph (z)")
    );
    let got = edges_of(&Fixture::new(&body).numbering().state());
    let prose: Vec<_> = got
        .iter()
        .filter(|e| e.1 == EdgeKind::Prose)
        .map(|e| (e.0.as_str(), e.2.as_str(), e.3, e.4.clone()))
        .collect();
    assert_eq!(
        prose,
        vec![
            (
                "p5",
                "paragraph (a)",
                State::Relative,
                vec!["p4".to_string()]
            ),
            ("p6", "paragraph (z)", State::Unresolved, Vec::new()),
        ]
    );
}

#[test]
fn a_reference_that_matches_several_clauses_is_ambiguous_with_all_of_them() {
    let body = format!(
        "{}{}{}",
        p("1. First part"),
        p("1. Second part, which repeats the number"),
        p("See clause 1.")
    );
    let got = edges_of(&Fixture::new(&body).state());
    assert_eq!(got.len(), 1);
    assert_eq!(
        (got[0].3, got[0].4.clone()),
        (State::Ambiguous, vec!["p2".to_string(), "p1".to_string()]),
        "the nearest candidate to the citing paragraph is first"
    );
}

#[test]
fn a_citation_of_a_law_is_outside_and_lists_the_clause_of_that_number_if_there_is_one() {
    let body = format!(
        "{}{}{}",
        p("501. A clause that happens to have the number"),
        p("Section 501 of the Code applies."),
        p("Section 7 of the Securities Act applies.")
    );
    let got = edges_of(&Fixture::new(&body).state());
    let shape: Vec<_> = got
        .iter()
        .map(|e| (e.0.as_str(), e.3, e.4.clone()))
        .collect();
    assert_eq!(
        shape,
        vec![
            ("p2", State::Outside, vec!["p1".to_string()]),
            ("p3", State::Outside, Vec::new())
        ]
    );
}

#[test]
fn a_story_with_no_mentions_has_no_edges_and_a_deleted_paragraph_makes_none() {
    let body = format!(
        "{}{}",
        p("Nothing here refers to anything."),
        px(&del(1, "JD", "Subject to clause 4."))
    );
    assert!(edges_of(&Fixture::new(&body).state()).is_empty());
}

#[test]
fn inbound_edges_come_in_document_order() {
    let body = format!(
        "{}{}{}{}",
        p("4. Target"),
        p("See clause 4 here."),
        p("And again, clause 4."),
        p("Nothing.")
    );
    let doc = Fixture::new(&body).state();
    let parts = doc.parts();
    let edges = parts.edges().unwrap();
    let story = parts.story(parts.id("word/document.xml").unwrap()).unwrap();
    let target = story.key(story.blocks().first().unwrap());
    let from: Vec<String> = edges
        .inbound(&target)
        .iter()
        .map(|e| parts.ref_of(&e.from).unwrap().to_string())
        .collect();
    assert_eq!(from, vec!["p2", "p3"]);
    assert_eq!(edges.len(), 2);
}

fn states(f: Fixture) -> Vec<(String, State, Vec<String>)> {
    edges_of(&f.numbering().state())
        .into_iter()
        .filter(|e| e.1 == EdgeKind::Prose)
        .map(|e| (e.2, e.3, e.4))
        .collect()
}

#[test]
fn a_number_that_starts_over_in_every_article_means_the_one_in_the_citing_article() {
    let body = format!(
        "{}{}{}{}{}{}{}",
        p("Article 1 First"),
        p("1. one"),
        p("2. two"),
        p("Article 2 Second"),
        p("1. uno"),
        p("2. dos, subject to paragraph 1 above"),
        p("Also paragraph 2 of Article 1 applies.")
    );
    let got = states(Fixture::new(&body));
    assert_eq!(
        got,
        vec![
            (
                "paragraph 1".to_string(),
                State::Relative,
                vec!["p5".to_string(), "p2".to_string()]
            ),
            (
                "paragraph 2".to_string(),
                State::Ambiguous,
                vec!["p6".to_string(), "p3".to_string()]
            ),
            (
                "Article 1".to_string(),
                State::Resolved,
                vec!["p1".to_string()]
            ),
        ],
        "a part of something named (`of Article 1`) is not read in the citing article"
    );
}

#[test]
fn a_citation_with_a_law_around_it_is_outside() {
    let body = format!(
        "{}{}{}{}{}{}{}",
        p("1. First"),
        p("Under 11 U.S.C. § 1 the court may act."),
        p("Under 45 CFR § 1 a covered entity must comply."),
        p("Under Sherman Act § 1 restraints are unlawful."),
        p("Under Article 1 GDPR the data subject has rights."),
        p("Parts 1 and 2 of 45 CFR apply."),
        p("See § 547(b) and Section 704(c) for the rules.")
    );
    let got = states(Fixture::new(&body));
    for (text, state, _) in &got {
        assert_eq!(*state, State::Outside, "{text}");
    }
    let named: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
    assert_eq!(
        named,
        vec![
            "§ 1",
            "§ 1",
            "§ 1",
            "Article 1",
            "Parts 1",
            "Parts 2",
            "§ 547(b)",
            "Section 704(c)"
        ]
    );
}

#[test]
fn a_bare_sub_item_is_read_against_the_citing_clause_before_any_clause_of_that_name() {
    let body = format!(
        "{}{}{}{}{}",
        p("(b) a recital that is also called b"),
        p("4. Payment"),
        p("(a) in cash"),
        p("(b) by wire"),
        p("(c) or as in clause (b) above")
    );
    let got = states(Fixture::new(&body));
    assert_eq!(
        got,
        vec![(
            "clause (b)".to_string(),
            State::Relative,
            vec!["p4".to_string(), "p1".to_string()]
        )],
        "4(b), not the recital, which follows as the alternative"
    );
}

#[test]
fn of_this_in_any_case_is_this_document_and_keeps_the_relative_reading() {
    let body = format!(
        "{}{}{}{}{}{}",
        p("(a) a recital that is also called a"),
        p("4. Payment"),
        p("(a) in cash"),
        p("(b) by wire, as in PARAGRAPH (a) OF THIS SECTION"),
        p("9.2 Notices"),
        p("Notices go as Section 9.2 of This Agreement says.")
    );
    let got = states(Fixture::new(&body));
    assert_eq!(
        got,
        vec![
            (
                "PARAGRAPH (a)".to_string(),
                State::Relative,
                vec!["p3".to_string(), "p1".to_string()]
            ),
            (
                "Section 9.2".to_string(),
                State::Resolved,
                vec!["p5".to_string()]
            ),
        ],
        "`of This` and `OF THIS` name this document, not another"
    );
}

#[test]
fn a_paragraph_that_names_itself_makes_no_edge_and_the_lines_of_a_contents_make_none() {
    let body = format!(
        "{}{}",
        p("Schedule 1. Fees. These are the fees in this Schedule 1."),
        p("5. In this clause 5 the Buyer pays.")
    );
    assert!(states(Fixture::new(&body)).is_empty());
}

#[test]
fn a_recital_numbered_in_parentheses_is_cited_by_its_number() {
    let body = format!(
        "{}{}",
        p("(61) This recital explains the rule."),
        p("As recital 61 says, the rule applies.")
    );
    let got = states(Fixture::new(&body));
    assert_eq!(
        got,
        vec![(
            "recital 61".to_string(),
            State::Resolved,
            vec!["p1".to_string()]
        )]
    );
}

#[test]
fn a_reference_in_words_beside_the_field_that_makes_it_is_one_edge() {
    let body = format!(
        "{}{}",
        bookmarked(1, "_Ref1", "4. Payment"),
        px(&format!(
            "{}{}",
            r("As set out in clause "),
            field("REF _Ref1 \\r", "4")
        ))
    );
    let doc = Fixture::new(&body).state();
    let edges = edges_of(&doc);
    assert_eq!(edges.len(), 1, "{edges:?}");
    assert_eq!(edges[0].1, EdgeKind::Field);
}
