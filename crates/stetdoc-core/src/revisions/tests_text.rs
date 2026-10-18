//! Tests of `replace_text` on small documents written by hand: the shapes of design 4.1 and 4.2, one
//! test for each decision that the text of the design makes.

use super::tests_support::*;

#[test]
fn deleting_inside_a_run_cuts_it_in_three_and_keeps_the_formatting_of_each() {
    let s = tracked_edits(
        "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello big world</w:t></w:r></w:p>",
        &[(0, 6..10, "")],
    );
    s.assert_clean();
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">Hello </w:t></w:r>\
             <w:del w:id=\"0\" {AT}><w:r><w:rPr><w:b/></w:rPr><w:delText xml:space=\"preserve\">big </w:delText></w:r></w:del>\
             <w:r><w:rPr><w:b/></w:rPr><w:t>world</w:t></w:r></w:p>"
        )
    );
    assert_eq!(s.fin, ["Hello world"]);
    assert_eq!(s.red, ["Hello big world"]);
}

#[test]
fn deleting_a_whole_run_wraps_it_and_cuts_nothing() {
    let s = tracked_edits(
        &p(&format!("{}{}{}", r("A"), r("B"), r("C"))),
        &[(0, 1..2, "")],
    );
    s.assert_clean();
    assert_eq!(s.xml, p(&format!("{}{}{}", r("A"), del(0, "B"), r("C"))));
}

#[test]
fn a_deletion_across_runs_is_one_del_around_all_of_them() {
    let body = p(
        "<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t xml:space=\"preserve\"> and </w:t></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r>",
    );
    let s = tracked_edits(&body, &[(0, 2..11, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["boalic"]);
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
    assert_eq!(
        s.xml.matches("<w:r>").count() + s.xml.matches("<w:r ").count(),
        5,
        "{}",
        s.xml
    );
}

#[test]
fn a_deletion_across_a_hyperlink_is_a_del_in_the_paragraph_and_one_in_the_link() {
    let body = p(&format!(
        "{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}",
        r("ab "),
        r("link"),
        r(" cd")
    ));
    let s = tracked_edits(&body, &[(0, 2..9, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["abd"]);
    let inside = s.xml.find("<w:hyperlink").unwrap();
    let close = s.xml.find("</w:hyperlink>").unwrap();
    assert_eq!(s.xml.matches("<w:del ").count(), 3, "{}", s.xml);
    let link = &s.xml[inside..close];
    assert!(
        link.contains("<w:del ") && link.contains("link"),
        "{}",
        s.xml
    );
}

#[test]
fn text_is_inserted_at_the_start_and_at_the_end_of_a_paragraph() {
    let s = tracked_edits(&p(&r("middle")), &[(0, 6..6, " end"), (0, 0..0, "start ")]);
    s.assert_clean();
    assert_eq!(s.fin, ["start middle end"]);
    assert!(s.xml.starts_with("<w:p><w:ins "), "{}", s.xml);
    assert!(s.xml.ends_with("</w:ins></w:p>"), "{}", s.xml);
}

#[test]
fn an_insertion_inside_a_run_cuts_it_and_goes_between_the_halves() {
    let s = tracked_edits(&p(&r("Hello world")), &[(0, 5..5, " big")]);
    s.assert_clean();
    assert_eq!(
        s.xml,
        p(&format!("{}{}{}", r("Hello"), ins(0, " big"), r(" world")))
    );
}

#[test]
fn a_range_end_marker_stays_with_the_old_text_and_the_new_text_goes_after_it() {
    let body = p(&format!(
        "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}<w:bookmarkEnd w:id=\"1\"/>{}",
        r("abc"),
        r("def")
    ));
    let s = tracked_edits(&body, &[(0, 3..3, "X")]);
    s.assert_clean();
    assert_eq!(
        s.xml,
        p(&format!(
            "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}<w:bookmarkEnd w:id=\"1\"/>{}{}",
            r("abc"),
            ins(2, "X"),
            r("def")
        ))
    );
}

#[test]
fn a_range_start_marker_stays_with_the_text_after_it_and_the_new_text_goes_before_it() {
    let body = p(&format!(
        "{}<w:commentRangeStart w:id=\"4\"/>{}<w:commentRangeEnd w:id=\"4\"/>",
        r("abc"),
        r("def")
    ));
    let s = tracked_edits(&body, &[(0, 3..3, "X")]);
    s.assert_clean();
    assert!(
        s.xml
            .contains(&format!("{}<w:commentRangeStart", ins(5, "X"))),
        "{}",
        s.xml
    );
}

#[test]
fn a_tab_and_a_line_break_in_the_new_text_become_elements() {
    let s = tracked_edits(&p(&r("ab")), &[(0, 2..2, "x\ty\nz")]);
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:t>x</w:t><w:tab/><w:t>y</w:t><w:br/><w:t>z</w:t>"),
        "{}",
        s.xml
    );
    assert_eq!(s.fin, ["abx\ty\nz"]);
}

#[test]
fn white_space_at_an_end_of_new_text_gets_xml_space() {
    let s = tracked_edits(&p(&r("ab")), &[(0, 1..1, " x ")]);
    s.assert_clean();
    assert!(
        s.xml.contains("<w:t xml:space=\"preserve\"> x </w:t>"),
        "{}",
        s.xml
    );
}

#[test]
fn replacing_a_word_is_a_del_and_an_ins_right_after_it() {
    let s = tracked_edits(&p(&r("the colour red")), &[(0, 0..14, "the color red")]);
    s.assert_clean();
    assert_eq!(s.fin, ["the color red"]);
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
    assert_eq!(s.xml.matches("<w:ins ").count(), 1, "{}", s.xml);
    assert!(
        s.xml.contains("</w:del><w:ins "),
        "the insertion follows the deletion: {}",
        s.xml
    );
}

#[test]
fn replacing_a_paragraph_with_a_related_text_makes_word_level_edits_only() {
    let s = tracked_edits(
        &p(&r("The quick brown fox jumps")),
        &[(0, 0..25, "The slow brown dog jumps")],
    );
    s.assert_clean();
    // The cleaning rules turn two dense edits around one short word into one replacement.
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
    assert_eq!(s.xml.matches("<w:ins ").count(), 1, "{}", s.xml);
    assert!(
        s.xml
            .starts_with("<w:p><w:r><w:t xml:space=\"preserve\">The </w:t></w:r><w:del "),
        "{}",
        s.xml
    );
    assert_eq!(s.fin, ["The slow brown dog jumps"]);
}

#[test]
fn new_text_takes_the_formatting_of_the_first_deleted_run() {
    let body = p("<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t> plain</w:t></w:r>");
    let s = tracked_edits(&body, &[(0, 0..4, "BOLD")]);
    s.assert_clean();
    assert!(s.xml.contains("<w:ins w:id=\"1\""), "{}", s.xml);
    let ins_at = s.xml.find("<w:ins ").unwrap();
    assert!(
        s.xml[ins_at..].starts_with(&format!(
            "<w:ins w:id=\"1\" {AT}><w:r><w:rPr><w:b/></w:rPr>"
        )),
        "{}",
        s.xml
    );
}

#[test]
fn a_pure_insertion_takes_the_formatting_of_the_text_on_its_left() {
    let body = p("<w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t> plain</w:t></w:r>");
    let s = tracked_edits(&body, &[(0, 4..4, "!")]);
    s.assert_clean();
    assert!(
        s.xml.contains(&format!(
            "<w:ins w:id=\"0\" {AT}><w:r><w:rPr><w:b/></w:rPr><w:t>!</w:t>"
        )),
        "{}",
        s.xml
    );
}

#[test]
fn an_insertion_after_an_atom_alone_takes_the_formatting_of_the_text_on_its_right() {
    let body = p(
        "<w:r><w:rPr><w:sz w:val=\"40\"/></w:rPr><w:tab/></w:r><w:r><w:rPr><w:i/></w:rPr><w:t>it</w:t></w:r>",
    );
    let s = tracked_edits(&body, &[(0, 1..1, "X")]);
    s.assert_clean();
    assert!(
        s.xml.contains(&format!(
            "<w:ins w:id=\"0\" {AT}><w:r><w:rPr><w:i/></w:rPr><w:t>X</w:t>"
        )),
        "{}",
        s.xml
    );
}

#[test]
fn an_empty_paragraph_gets_the_formatting_of_its_mark() {
    let body = "<w:p><w:pPr><w:rPr><w:lang w:val=\"fr-FR\"/></w:rPr></w:pPr></w:p>";
    let s = tracked_edits(body, &[(0, 0..0, "Bonjour")]);
    s.assert_clean();
    assert!(
        s.xml.contains(&format!(
            "<w:ins w:id=\"0\" {AT}><w:r><w:rPr><w:lang w:val=\"fr-FR\"/></w:rPr><w:t>Bonjour</w:t>"
        )),
        "{}",
        s.xml
    );
}

#[test]
fn the_formatting_copy_drops_a_pending_rpr_change_and_marks() {
    let body = format!(
        "<w:p><w:r><w:rPr><w:b/><w:rPrChange {} ><w:rPr/></w:rPrChange></w:rPr><w:t>bold</w:t></w:r></w:p>",
        eric(300)
    );
    let s = tracked_edits(&body, &[(0, 4..4, "!")]);
    s.assert_clean();
    let at = s.xml.find("<w:ins ").unwrap();
    assert!(!s.xml[at..].contains("rPrChange"), "{}", s.xml);
}
