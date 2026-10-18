//! Tests for the counters, labels and modes. Each test says what it proves.

use super::item::{ItemKind, Note};
use super::tests_support::*;
use crate::api::view::Mode::Final;

fn some(v: &[&'static str]) -> Vec<Option<&'static str>> {
    v.iter().map(|s| Some(*s)).collect()
}

fn final_labels(numbering: &str, styles: &str, body: &str) -> Vec<Option<String>> {
    run(numbering, styles, body, Final)
}

#[test]
fn a_plain_list_counts() {
    // Proves a single level counts from its start.
    let n = format!("{}{}", abs(1, &lvl(0, "decimal", "%1.", "")), num(1, 1, ""));
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 0), (1, 0)]));
    assert_eq!(labels(&got), some(&["1.", "2.", "3."]));
}

#[test]
fn deeper_levels_restart() {
    // Proves a level restarts after any shallower level, and the text names the higher levels.
    let n = format!("{}{}", legal_list(1), num(1, 1, ""));
    let b = body(&[
        (1, 0),
        (1, 1),
        (1, 1),
        (1, 2),
        (1, 2),
        (1, 0),
        (1, 1),
        (1, 2),
    ]);
    let got = final_labels(&n, "", &b);
    assert_eq!(
        labels(&got),
        some(&["1.", "1.1", "1.2", "(a)", "(b)", "2.", "2.1", "(a)"])
    );
}

#[test]
fn lvl_restart_zero_never_restarts_the_iso_example() {
    // Proves the ISO 29500 example: level 3 with lvlRestart 0 keeps counting after later level 1 and 2.
    let levels = format!(
        "{}{}{}",
        lvl(0, "decimal", "%1)", ""),
        lvl(1, "upperLetter", "%2)", ""),
        lvl(2, "lowerRoman", "%3)", "<w:lvlRestart w:val=\"0\"/>")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let b = body(&[
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 2),
    ]);
    let got = final_labels(&n, "", &b);
    assert_eq!(
        labels(&got),
        some(&["1)", "A)", "i)", "ii)", "2)", "A)", "iii)", "iv)"])
    );
}

#[test]
fn lvl_restart_names_the_level_that_restarts() {
    // Proves lvlRestart 1 restarts level 3 only after a level 1 paragraph, not after level 2.
    let levels = format!(
        "{}{}{}",
        lvl(0, "decimal", "%1.", ""),
        lvl(1, "decimal", "%1.%2", ""),
        lvl(2, "decimal", "%1.%2.%3", "<w:lvlRestart w:val=\"1\"/>")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let b = body(&[
        (1, 0),
        (1, 1),
        (1, 2),
        (1, 2),
        (1, 1),
        (1, 2),
        (1, 0),
        (1, 1),
        (1, 2),
    ]);
    let got = final_labels(&n, "", &b);
    assert_eq!(
        labels(&got),
        some(&[
            "1.", "1.1", "1.1.1", "1.1.2", "1.2", "1.2.3", "2.", "2.1", "2.1.1"
        ])
    );
}

#[test]
fn a_list_that_starts_deep_shows_the_start_of_its_parents() {
    // Register: U9.
    // Proves a shallower level with no value shows its start and counts as entered (not 0).
    let n = format!(
        "{}{}",
        abs(
            1,
            &format!(
                "{}{}{}",
                lvl(0, "decimal", "%1.", ""),
                lvl(1, "decimal", "%1.%2", ""),
                lvl(2, "decimal", "%1.%2.%3", "")
            )
        ),
        num(1, 1, "")
    );
    let got = final_labels(&n, "", &body(&[(1, 2), (1, 2), (1, 0)]));
    assert_eq!(labels(&got), some(&["1.1.1", "1.1.2", "2."]));
}

#[test]
fn a_missing_start_is_zero() {
    // Proves ISO 29500: a level without w:start starts at 0.
    let n = format!(
        "{}{}",
        abs(1, &lvl_start(0, None, "decimal", "%1.", "")),
        num(1, 1, "")
    );
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 0)]));
    assert_eq!(labels(&got), some(&["0.", "1."]));
}

#[test]
fn starts_can_be_large_negative_or_saturating() {
    // Proves any i64 start is accepted and counting saturates instead of overflowing.
    for (start, want) in [(5, vec!["5", "6"]), (-2, vec!["-2", "-1"])] {
        let n = format!(
            "{}{}",
            abs(1, &lvl_start(0, Some(start), "decimal", "%1", "")),
            num(1, 1, "")
        );
        let got = final_labels(&n, "", &body(&[(1, 0), (1, 0)]));
        assert_eq!(labels(&got), some(&want));
    }
    let n = format!(
        "{}{}",
        abs(1, &lvl_start(0, Some(i64::MAX), "decimal", "%1", "")),
        num(1, 1, "")
    );
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 0)]));
    assert_eq!(
        labels(&got),
        some(&["9223372036854775807", "9223372036854775807"])
    );
}

#[test]
fn two_nums_over_one_list_share_counters() {
    // Proves counters belong to the abstract definition: a second w:num with no override continues.
    let n = format!(
        "{}{}{}",
        abs(1, &lvl(0, "decimal", "%1.", "")),
        num(1, 1, ""),
        num(2, 1, "")
    );
    let got = final_labels(&n, "", &body(&[(1, 0), (2, 0), (1, 0)]));
    assert_eq!(labels(&got), some(&["1.", "2.", "3."]));
}

#[test]
fn a_start_override_restarts_on_first_use() {
    // Register: U12.
    // Proves a w:num with a startOverride restarts the list the first time it numbers the level.
    let n = format!(
        "{}{}{}",
        abs(1, &lvl(0, "decimal", "%1.", "")),
        num(1, 1, ""),
        num(2, 1, &start_override(0, 1))
    );
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 0), (2, 0), (2, 0)]));
    assert_eq!(labels(&got), some(&["1.", "2.", "1.", "2."]));
}

#[test]
fn shared_list_continue_a_b_a() {
    // Register: U12.
    // Proves the A, B, A case: A has an override, B none. A restarts once, B continues, and A's
    // return continues too. The three numbers are 1, 2, 3, not 1, 2, 1.
    let n = format!(
        "{}{}{}",
        abs(1, &lvl(0, "decimal", "%1.", "")),
        num(1, 1, ""),
        num(2, 1, &start_override(0, 1))
    );
    let got = final_labels(&n, "", &body(&[(2, 0), (1, 0), (2, 0)]));
    assert_eq!(labels(&got), some(&["1.", "2.", "3."]));
    // And after another w:num has run: B, B, A (A restarts at 1 once), B, A (continues).
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 0), (2, 0), (1, 0), (2, 0)]));
    assert_eq!(labels(&got), some(&["1.", "2.", "1.", "2.", "3."]));
}

#[test]
fn overrides_on_two_levels_each_apply() {
    // Register: U12.
    // Proves the bookkeeping is per (numId, level): an override on level 1 applies on every restart of
    // that level, as well as the override on level 0 at the first use.
    let n = format!(
        "{}{}{}",
        abs(
            1,
            &format!(
                "{}{}",
                lvl(0, "decimal", "%1.", ""),
                lvl(1, "decimal", "%1.%2", "")
            )
        ),
        num(1, 1, ""),
        num(
            2,
            1,
            &format!("{}{}", start_override(0, 7), start_override(1, 3))
        )
    );
    let b = body(&[(1, 0), (1, 1), (2, 0), (2, 1), (2, 1), (2, 0), (2, 1)]);
    let got = final_labels(&n, "", &b);
    assert_eq!(
        labels(&got),
        some(&["1.", "1.1", "7.", "7.3", "7.4", "8.", "8.3"])
    );
}

#[test]
fn a_level_override_replaces_the_level() {
    // Proves a lvlOverride with a w:lvl replaces the abstract level's format and text.
    let replacement = format!(
        "<w:lvlOverride w:ilvl=\"0\">{}</w:lvlOverride>",
        lvl(0, "upperRoman", "Part %1", "")
    );
    let n = format!(
        "{}{}",
        abs(1, &lvl(0, "decimal", "%1.", "")),
        num(1, 1, &replacement)
    );
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 0)]));
    assert_eq!(labels(&got), some(&["Part I", "Part II"]));
}

#[test]
fn is_lgl_shows_every_level_as_decimal() {
    // Register: U11.
    // Proves isLgl turns the levels named in the text into decimal numbers, and `decimalZero` too.
    let levels = format!(
        "{}{}{}",
        lvl(0, "upperLetter", "%1.", ""),
        lvl(1, "lowerLetter", "%2.", ""),
        lvl(2, "lowerRoman", "%1.%2.%3", "<w:isLgl/>")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let got = final_labels(
        &n,
        "",
        &body(&[(1, 0), (1, 1), (1, 2), (1, 0), (1, 1), (1, 2)]),
    );
    assert_eq!(
        labels(&got),
        some(&["A.", "a.", "1.1.1", "B.", "a.", "2.1.1"])
    );
    let levels = format!(
        "{}{}",
        lvl(0, "decimalZero", "%1", ""),
        lvl(1, "decimalZero", "%1.%2", "<w:isLgl/>")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 1)]));
    assert_eq!(labels(&got), some(&["01", "1.1"]));
}

#[test]
fn a_placeholder_for_a_deeper_level_renders_nothing() {
    // Register: U10.
    // Proves ISO 29500: a number of a level higher than the paragraph's own is ignored.
    let n = format!(
        "{}{}",
        abs(1, &lvl(0, "decimal", "%1.%2", "")),
        num(1, 1, "")
    );
    let got = final_labels(&n, "", &body(&[(1, 0)]));
    assert_eq!(labels(&got), some(&["1."]));
}

#[test]
fn the_iso_level_text_example() {
    // Register: U10.
    // Proves literal text around placeholders, each formatted with the format of its own level.
    let levels = format!(
        "{}{}",
        lvl(0, "decimal", "%1", ""),
        lvl(1, "upperRoman", "StringA %2 StringB %1 StringC %3", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 1), (1, 1), (1, 0), (1, 1)]));
    assert_eq!(
        labels(&got),
        some(&[
            "1",
            "StringA I StringB 1 StringC",
            "StringA II StringB 1 StringC",
            "2",
            "StringA I StringB 2 StringC"
        ])
    );
}

#[test]
fn bullets_have_no_clause_number_but_are_list_items() {
    // Proves a bullet is a list item with its glyph, and has no clause number.
    let n = format!(
        "{}{}",
        abs(1, &lvl(0, "bullet", "\u{2022}", "")),
        num(1, 1, "")
    );
    let (got, numbers, facts) = detail(&n, "", &body(&[(1, 0)]), Final);
    assert_eq!(labels(&got), vec![None]);
    let item = numbers.item(facts[0].node).unwrap();
    assert_eq!(
        item.kind,
        ItemKind::Bullet {
            glyph: "\u{2022}".into()
        }
    );
}

#[test]
fn a_bullet_level_still_restarts_counting_levels_below_it() {
    // Proves a bullet level takes part in restarts: the counting level below it starts over.
    let levels = format!(
        "{}{}",
        lvl(0, "bullet", "o", ""),
        lvl(1, "decimal", "%2.", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 1), (1, 1), (1, 0), (1, 1)]));
    assert_eq!(
        labels(&got),
        vec![None, Some("1."), Some("2."), None, Some("1.")]
    );
}

#[test]
fn a_style_supplies_the_numbering_and_the_level_names_the_style() {
    // Register: U6.
    // Proves style numbering: the level comes from the level whose pStyle names the style, even when
    // the style's own ilvl says otherwise (ISO 29500, `pStyle` of a level).
    let levels = format!(
        "{}{}",
        lvl(0, "decimal", "%1.", "<w:pStyle w:val=\"H1\"/>"),
        lvl(1, "decimal", "%1.%2", "<w:pStyle w:val=\"H2\"/>")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let styles = format!(
        "{}{}",
        style_num("H1", None, "<w:numId w:val=\"1\"/>"),
        style_num("H2", None, "<w:ilvl w:val=\"0\"/><w:numId w:val=\"1\"/>")
    );
    let got = final_labels(
        &n,
        &styles,
        &format!("{}{}{}{}", ps("H1"), ps("H2"), ps("H2"), ps("H1")),
    );
    assert_eq!(labels(&got), some(&["1.", "1.1", "1.2", "2."]));
}

#[test]
fn a_style_without_a_matching_level_uses_its_own_level() {
    // Register: U6.
    // Proves the fallback: the style's ilvl, then 0, when no level names the style.
    let n = format!("{}{}", legal_list(1), num(1, 1, ""));
    let styles = style_num("Sub", None, "<w:ilvl w:val=\"1\"/><w:numId w:val=\"1\"/>");
    let got = final_labels(
        &n,
        &styles,
        &format!("{}{}{}", p(1, 0), ps("Sub"), ps("Sub")),
    );
    assert_eq!(labels(&got), some(&["1.", "1.1", "1.2"]));
}

#[test]
fn numbering_is_inherited_along_based_on_and_removed_by_zero() {
    // Proves numbering comes from the nearest numPr on the basedOn chain, child by child, and that a
    // style numId of 0 removes the parent's.
    let n = format!("{}{}", legal_list(1), num(1, 1, ""));
    let styles = format!(
        "{}{}{}",
        style_num("Base", None, "<w:numId w:val=\"1\"/>"),
        style_num("Child", Some("Base"), "<w:ilvl w:val=\"1\"/>"),
        style_num("Off", Some("Base"), "<w:numId w:val=\"0\"/>")
    );
    let b = format!("{}{}{}", ps("Base"), ps("Child"), ps("Off"));
    let got = final_labels(&n, &styles, &b);
    assert_eq!(labels(&got), vec![Some("1."), Some("1.1"), None]);
}

#[test]
fn direct_numbering_wins_over_the_style_as_a_unit() {
    // Register: U8.
    // Proves a direct numId 0 removes style numbering, a direct numPr without numId is not a list
    // item, and a direct numId without ilvl is level 0.
    let n = format!("{}{}", legal_list(1), num(1, 1, ""));
    let styles = style_num("Base", None, "<w:numId w:val=\"1\"/>");
    let zero = pp(
        "<w:pStyle w:val=\"Base\"/><w:numPr><w:numId w:val=\"0\"/></w:numPr>",
        "",
    );
    let no_id = pp(
        "<w:pStyle w:val=\"Base\"/><w:numPr><w:ilvl w:val=\"1\"/></w:numPr>",
        "",
    );
    let no_lvl = pp("<w:numPr><w:numId w:val=\"1\"/></w:numPr>", "");
    let got = final_labels(&n, &styles, &format!("{zero}{no_id}{no_lvl}{}", ps("Base")));
    assert_eq!(labels(&got), vec![None, None, Some("1."), Some("2.")]);
}

#[test]
fn unresolved_numbering_is_reported_not_numbered() {
    // Proves a numId with no w:num makes the paragraph unnumbered with a note, and a missing level
    // takes the nearest lower level with a note.
    let n = format!("{}{}", abs(1, &lvl(0, "decimal", "%1.", "")), num(1, 1, ""));
    let (got, numbers, _) = detail(&n, "", &body(&[(9, 0), (1, 1)]), Final);
    assert_eq!(labels(&got), vec![None, Some("1.")]);
    assert!(
        numbers
            .notes()
            .contains(&Note::UnresolvedNumId { num_id: 9 })
    );
    assert!(
        numbers
            .notes()
            .contains(&Note::MissingLevel { num_id: 1, ilvl: 1 })
    );
}

#[test]
fn an_empty_section_break_paragraph_is_not_numbered() {
    // Register: U5.
    // Proves the reference's rule (register U5): a paragraph that only carries a section break.
    let n = format!("{}{}", abs(1, &lvl(0, "decimal", "%1.", "")), num(1, 1, ""));
    let sect = pp(
        "<w:numPr><w:numId w:val=\"1\"/></w:numPr><w:sectPr><w:pgSz w:w=\"1\" w:h=\"1\"/></w:sectPr>",
        "",
    );
    let got = final_labels(&n, "", &format!("{}{sect}{}", p(1, 0), p(1, 0)));
    assert_eq!(labels(&got), vec![Some("1."), None, Some("2.")]);
}

#[test]
fn num_style_link_takes_levels_from_the_linked_list() {
    // Register: U13.
    // Proves a numStyleLink abstract definition uses the levels of the list the numbering style names.
    let a = abs(1, "<w:numStyleLink w:val=\"Legal\"/>");
    let b = abs(
        2,
        &format!(
            "<w:styleLink w:val=\"Legal\"/>{}",
            lvl(0, "upperRoman", "%1)", "")
        ),
    );
    let n = format!("{a}{b}{}{}", num(1, 1, ""), num(2, 2, ""));
    let styles = "<w:style w:type=\"numbering\" w:styleId=\"Legal\"><w:pPr><w:numPr>\
                  <w:numId w:val=\"2\"/></w:numPr></w:pPr></w:style>";
    let got = final_labels(&n, styles, &body(&[(1, 0), (1, 0)]));
    assert_eq!(labels(&got), some(&["I)", "II)"]));
}

#[test]
fn a_num_style_link_cycle_ends() {
    // Proves a numStyleLink chain that loops is cut, with a note, and does not hang.
    let a = abs(1, "<w:numStyleLink w:val=\"L\"/>");
    let n = format!("{a}{}", num(1, 1, ""));
    let styles = "<w:style w:type=\"numbering\" w:styleId=\"L\"><w:pPr><w:numPr>\
                  <w:numId w:val=\"1\"/></w:numPr></w:pPr></w:style>";
    let (got, numbers, _) = detail(&n, styles, &body(&[(1, 0)]), Final);
    assert_eq!(labels(&got), vec![None]);
    assert!(
        numbers
            .notes()
            .iter()
            .any(|n| matches!(n, Note::StyleLinkCycle { .. }))
    );
}

#[test]
fn full_context_and_find() {
    // Register: U16.
    // Proves full_context: a label that carries its ancestors is the answer, and a label that does not
    // gets the latest label of the shallower level in front, trailing periods stripped.
    let levels = format!(
        "{}{}{}",
        lvl_start(0, Some(26), "decimal", "%1.", ""),
        lvl(1, "decimal", "%1.%2", ""),
        lvl(2, "lowerLetter", "(%3)", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let b = body(&[(1, 0), (1, 1), (1, 1), (1, 2)]);
    let (got, numbers, facts) = detail(&n, "", &b, Final);
    assert_eq!(labels(&got), some(&["26.", "26.1", "26.2", "(a)"]));
    let full: Vec<_> = facts
        .iter()
        .map(|f| numbers.full_context(f.node).unwrap())
        .collect();
    assert_eq!(full, vec!["26", "26.1", "26.2", "26.2(a)"]);
    assert_eq!(
        numbers.find("26.2(a)").collect::<Vec<_>>(),
        vec![facts[3].node]
    );
    assert_eq!(numbers.find("26.").collect::<Vec<_>>(), vec![facts[0].node]);
    assert_eq!(numbers.find("(a)").collect::<Vec<_>>(), vec![facts[3].node]);
    assert_eq!(numbers.find("7").count(), 0);
}

#[test]
fn text_formats_are_used_in_labels() {
    // Proves each level's own format reaches its label, with an out-of-range value reported.
    let levels = format!(
        "{}{}",
        lvl(0, "upperRoman", "Article %1", ""),
        lvl(1, "cardinalText", "Section %2", "")
    );
    let n = format!("{}{}", abs(1, &levels), num(1, 1, ""));
    let got = final_labels(&n, "", &body(&[(1, 0), (1, 1), (1, 1)]));
    assert_eq!(
        labels(&got),
        some(&["Article I", "Section One", "Section Two"])
    );
}
