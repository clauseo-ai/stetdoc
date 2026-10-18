//! Fixtures for the rows of the register of unverified choices (design 7.5) that the other numbering
//! tests do not pin, and the metamorphic checks of 7.6. Each test says what it proves.

use super::item::Note;
use super::tests_support::*;
use crate::api::view::Mode::Final;

fn lists(ids: (i64, i64, i64)) -> String {
    // A legal list under the ids given, plus a second w:num over it with an override.
    let (abstract_id, num_a, num_b) = ids;
    format!(
        "{}{}{}",
        legal_list(abstract_id),
        num(num_a, abstract_id, ""),
        num(num_b, abstract_id, &start_override(0, 4))
    )
}

#[test]
fn a_hidden_paragraph_still_advances_the_list() {
    // U4: numbering has no say about hidden text, so a paragraph whose mark is hidden or that joins
    // the next one by specVanish is numbered and counts.
    let n = format!("{}{}", abs(1, &lvl(0, "decimal", "%1.", "")), num(1, 1, ""));
    let hidden = pp(
        "<w:numPr><w:numId w:val=\"1\"/></w:numPr><w:rPr><w:vanish/></w:rPr>",
        "",
    );
    let joined = pp(
        "<w:numPr><w:numId w:val=\"1\"/></w:numPr><w:rPr><w:specVanish/></w:rPr>",
        "",
    );
    let got = run(
        &n,
        "",
        &format!("{}{hidden}{joined}{}", p(1, 0), p(1, 0)),
        Final,
    );
    assert_eq!(
        labels(&got),
        vec![Some("1."), Some("2."), Some("3."), Some("4.")]
    );
}

#[test]
fn a_style_is_matched_to_a_level_by_its_own_id_only() {
    // U18: a style whose parent is the one a level names is not matched through the parent, so it
    // falls back to the ilvl its chain supplies, then to 0.
    let levels = format!(
        "{}{}",
        lvl(0, "decimal", "%1.", ""),
        lvl(1, "decimal", "%1.%2", "<w:pStyle w:val=\"Heading2\"/>")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let styles = format!(
        "{}{}",
        style_num("Heading2", None, "<w:numId w:val=\"1\"/>"),
        style_num("Sub", Some("Heading2"), "")
    );
    let got = run(
        &n,
        &styles,
        &format!("{}{}{}", p(1, 0), ps("Heading2"), ps("Sub")),
        Final,
    );
    assert_eq!(labels(&got), vec![Some("1."), Some("1.1"), Some("2.")]);
}

#[test]
fn the_continuation_heuristic_of_the_reference_is_not_applied() {
    // U22: a level-1 item whose counter is the parent's plus one is numbered 3.4, not as a flat 4.
    let levels = format!(
        "{}{}",
        lvl(0, "decimal", "%1.", ""),
        lvl_start(1, Some(4), "decimal", "%1.%2", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let got = run(&n, "", &body(&[(1, 0), (1, 0), (1, 0), (1, 1)]), Final);
    assert_eq!(
        labels(&got),
        vec![Some("1."), Some("2."), Some("3."), Some("3.4")]
    );
}

#[test]
fn full_context_shapes() {
    // U16: a level whose text carries its ancestors is the answer; one that does not is joined to
    // the latest label of the level above, and trailing periods go from each part.
    let levels = format!(
        "{}{}{}",
        lvl(0, "decimal", "%1.", ""),
        lvl(1, "lowerLetter", "(%2)", ""),
        lvl(2, "lowerRoman", "(%3)", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let (_, numbers, facts) = detail(
        &n,
        "",
        &body(&[(1, 0), (1, 1), (1, 2), (1, 2), (1, 1)]),
        Final,
    );
    let full: Vec<_> = facts
        .iter()
        .map(|f| numbers.full_context(f.node).unwrap().to_string())
        .collect();
    assert_eq!(full, vec!["1", "1(a)", "1(a)(i)", "1(a)(ii)", "1(b)"]);
}

fn same(a: &[Option<String>], b: &[Option<String>]) -> bool {
    a == b
}

#[test]
fn renaming_the_ids_changes_nothing() {
    // Metamorphic: permuting numId and abstractNumId, and reordering the definitions, gives the same labels.
    let items = [(1, 0), (1, 1), (2, 0), (1, 1), (2, 1), (1, 2), (2, 0)];
    let original = run(&lists((1, 1, 2)), "", &body(&items), Final);
    let renamed_items: Vec<(i64, u8)> = items
        .iter()
        .map(|(n, l)| (if *n == 1 { 90 } else { 7 }, *l))
        .collect();
    let renamed = run(&lists((55, 90, 7)), "", &body(&renamed_items), Final);
    assert!(same(&original, &renamed));
    // Reversing the definitions' order: the num elements before the abstractNum.
    let n = format!(
        "{}{}{}",
        num(1, 1, ""),
        num(2, 1, &start_override(0, 4)),
        legal_list(1)
    );
    assert!(same(&original, &run(&n, "", &body(&items), Final)));
}

#[test]
fn splitting_a_num_into_two_copies_changes_nothing() {
    // Metamorphic: two w:num over one list with no overrides count as one.
    let one = format!("{}{}", legal_list(1), num(1, 1, ""));
    let two = format!("{}{}{}", legal_list(1), num(1, 1, ""), num(2, 1, ""));
    let a = run(
        &one,
        "",
        &body(&[(1, 0), (1, 1), (1, 1), (1, 0), (1, 2)]),
        Final,
    );
    let b = run(
        &two,
        "",
        &body(&[(1, 0), (1, 1), (2, 1), (2, 0), (1, 2)]),
        Final,
    );
    assert!(same(&a, &b));
}

#[test]
fn unnumbered_paragraphs_between_change_nothing() {
    // Metamorphic: unnumbered paragraphs, plain and styled, do not move any number.
    let n = format!("{}{}", legal_list(1), num(1, 1, ""));
    let plain = "<w:p><w:r><w:t>text</w:t></w:r></w:p>";
    let numbered = [(1, 0), (1, 1), (1, 1), (1, 0)];
    let a = run(&n, "", &body(&numbered), Final);
    let mixed: String = numbered
        .iter()
        .map(|(num_id, l)| format!("{plain}{}{plain}", p(*num_id, *l)))
        .collect();
    let b = run(&n, "", &mixed, Final);
    let only_numbered: Vec<_> = b.into_iter().filter(Option::is_some).collect();
    assert_eq!(a, only_numbered);
}

#[test]
fn counters_of_style_linked_lists_are_keyed_by_the_nums_own_abstract_definition() {
    // U13: two w:num over one numStyleLink definition share counters, and two w:num over two such
    // definitions that reach the same numbering style do not.
    let linked = |id: i64| abs(id, "<w:numStyleLink w:val=\"Legal\"/>");
    let target = abs(
        9,
        &format!(
            "<w:styleLink w:val=\"Legal\"/>{}",
            lvl(0, "decimal", "%1.", "")
        ),
    );
    let styles = "<w:style w:type=\"numbering\" w:styleId=\"Legal\"><w:pPr><w:numPr>\
                  <w:numId w:val=\"9\"/></w:numPr></w:pPr></w:style>";
    let shared = format!(
        "{}{target}{}{}{}",
        linked(1),
        num(9, 9, ""),
        num(1, 1, ""),
        num(2, 1, "")
    );
    let got = run(&shared, styles, &body(&[(1, 0), (2, 0), (1, 0)]), Final);
    assert_eq!(labels(&got), vec![Some("1."), Some("2."), Some("3.")]);
    let separate = format!(
        "{}{}{target}{}{}{}",
        linked(1),
        linked(2),
        num(9, 9, ""),
        num(1, 1, ""),
        num(2, 2, "")
    );
    let got = run(&separate, styles, &body(&[(1, 0), (2, 0), (1, 0)]), Final);
    assert_eq!(labels(&got), vec![Some("1."), Some("1."), Some("2.")]);
}

#[test]
fn full_context_re_renders_ancestors_from_the_current_counters() {
    // U16: a level that never restarts (lvlRestart 0) keeps its old value under a new parent, so the
    // ancestor label of a deeper paragraph must show the new parent's number with that old value
    // (2.1), not the label as it was rendered under the first parent (1.1).
    let levels = format!(
        "{}{}{}",
        lvl(0, "decimal", "%1.", ""),
        lvl(1, "decimal", "%1.%2", "<w:lvlRestart w:val=\"0\"/>"),
        lvl(2, "lowerLetter", "(%3)", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let (got, numbers, facts) = detail(&n, "", &body(&[(1, 0), (1, 1), (1, 0), (1, 2)]), Final);
    assert_eq!(
        labels(&got),
        vec![Some("1."), Some("1.1"), Some("2."), Some("(a)")]
    );
    assert_eq!(numbers.full_context(facts[3].node), Some("2.1(a)"));
}

#[test]
fn a_note_is_at_the_paragraphs_it_arose_for() {
    // Where a note arose: a paragraph on a level the list lacks, a numId with no definition, and a label
    // in a format that is not built are each at their own paragraph, and the story's list of notes has
    // each distinct note once.
    let numbering = format!(
        "{}{}{}",
        abs(
            1,
            &format!(
                "{}{}",
                lvl(0, "decimal", "%1.", ""),
                lvl(1, "hindiVowels", "%2.", "")
            )
        ),
        num(1, 1, ""),
        ""
    );
    let b = format!("{}{}{}{}{}", p(1, 0), p(1, 1), p(1, 1), p(1, 4), p(9, 0));
    let (_, numbers, facts) = detail(&numbering, "", &b, Final);
    let at = |i: usize| numbers.notes_at(facts[i].node).cloned().collect::<Vec<_>>();
    let unsupported = Note::UnsupportedFormat {
        fmt: "hindiVowels".into(),
    };
    assert!(at(0).is_empty(), "a plain decimal label has no note");
    assert_eq!(at(1), vec![unsupported.clone()]);
    assert_eq!(
        at(2),
        vec![unsupported.clone()],
        "every paragraph that shows it"
    );
    assert_eq!(
        at(3),
        vec![
            Note::MissingLevel { num_id: 1, ilvl: 4 },
            unsupported.clone()
        ]
    );
    assert_eq!(at(4), vec![Note::UnresolvedNumId { num_id: 9 }]);
    assert_eq!(
        numbers
            .notes()
            .iter()
            .filter(|n| **n == unsupported)
            .count(),
        1
    );
}
