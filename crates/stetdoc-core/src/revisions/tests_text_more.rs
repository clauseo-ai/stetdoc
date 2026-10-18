//! More tests of `replace_text` on small documents: other authors' revisions, atoms, fields, markers,
//! containers, refusals and the untracked form.

use super::tests_support::*;
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

// --- other authors' revisions

fn mid_ins() -> String {
    p(&format!(
        "{}<w:ins {}>{}</w:ins>{}",
        r("ab "),
        eric(900),
        r("inserted text"),
        r(" cd")
    ))
}

#[test]
fn an_insertion_inside_another_authors_insertion_splits_it_and_reports_the_new_id() {
    let s = tracked_edits(&mid_ins(), &[(0, 6..6, "XX")]);
    s.assert_clean();
    assert_eq!(s.effect(0).split, [(900, 901)]);
    assert_eq!(
        s.xml,
        p(&format!(
            "{}<w:ins {e1}>{}</w:ins><w:ins w:id=\"902\" {AT}>{}</w:ins><w:ins {e2}>{}</w:ins>{}",
            r("ab "),
            r("ins"),
            new_run("XX"),
            r("erted text"),
            r(" cd"),
            e1 = eric(900),
            e2 = eric(901)
        ))
    );
    assert_eq!(created_ids(s.outcome.as_ref().unwrap()), [902]);
}

#[test]
fn an_insertion_at_an_end_of_another_authors_insertion_does_not_split_it() {
    let at_start = tracked_edits(&mid_ins(), &[(0, 3..3, "XX")]);
    at_start.assert_clean();
    assert!(at_start.effect(0).split.is_empty());
    assert!(
        at_start
            .xml
            .contains(&format!("{}<w:ins {}>", ins(901, "XX"), eric(900))),
        "{}",
        at_start.xml
    );
    let at_end = tracked_edits(&mid_ins(), &[(0, 16..16, "XX")]);
    at_end.assert_clean();
    assert!(at_end.effect(0).split.is_empty());
    assert!(
        at_end.xml.contains(&format!("</w:ins>{}", ins(901, "XX"))),
        "{}",
        at_end.xml
    );
}

#[test]
fn a_deletion_inside_another_authors_insertion_nests_our_del_in_their_ins() {
    let s = tracked_edits(&mid_ins(), &[(0, 6..9, "")]);
    s.assert_clean();
    assert_eq!(
        s.xml,
        p(&format!(
            "{}<w:ins {}>{}{}{}</w:ins>{}",
            r("ab "),
            eric(900),
            r("ins"),
            del(901, "ert"),
            r("ed text"),
            r(" cd")
        ))
    );
}

#[test]
fn text_deleted_by_another_author_is_not_in_final_and_is_left_alone() {
    let body = p(&format!(
        "{}<w:del {}><w:r><w:delText>XX</w:delText></w:r></w:del>{}",
        r("ab"),
        eric(910),
        r("cd")
    ));
    let s = tracked_edits(&body, &[(0, 1..3, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["ad"]);
    assert!(s.xml.contains("<w:delText>XX</w:delText>"), "{}", s.xml);
    assert_eq!(s.xml.matches("<w:del ").count(), 3, "{}", s.xml);
}

#[test]
fn removing_part_of_our_own_earlier_insertion_removes_it_outright() {
    let body = p(&format!(
        "{}<w:ins w:id=\"900\" {AT}>{}</w:ins>{}",
        r("ab "),
        r("inserted text"),
        r(" cd")
    ));
    let s = tracked_edits(&body, &[(0, 6..9, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["ab insed text cd"]);
    assert_eq!(s.effect(0).removed_own.len(), 1);
    assert_eq!(s.effect(0).removed_own[0].id, Some(900));
    assert!(!s.xml.contains("<w:del "), "{}", s.xml);
    assert!(s.xml.contains("<w:ins w:id=\"900\""), "{}", s.xml);
}

#[test]
fn removing_all_of_our_own_insertion_removes_the_wrapper_too() {
    let body = p(&format!(
        "{}<w:ins w:id=\"900\" {AT}>{}</w:ins>{}",
        r("ab "),
        r("new"),
        r(" cd")
    ));
    let s = tracked_edits(&body, &[(0, 3..6, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["ab  cd"]);
    assert!(!s.xml.contains("<w:ins "), "{}", s.xml);
    assert!(!s.xml.contains("<w:del "), "{}", s.xml);
}

#[test]
fn an_insertion_made_in_this_transaction_is_removed_by_deleting_it_again() {
    let s = tracked_edits(&p(&r("abcd")), &[(0, 2..2, "XYZ"), (0, 2..5, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["abcd"]);
    assert!(!s.xml.contains("<w:ins "), "{}", s.xml);
    assert!(!s.xml.contains("<w:del "), "{}", s.xml);
    assert_eq!(s.effect(1).removed_own.len(), 1);
    assert!(created_ids(s.outcome.as_ref().unwrap()).is_empty());
}

#[test]
fn two_insertions_next_to_each_other_in_one_transaction_share_one_ins() {
    let s = tracked_edits(&p(&r("abcd")), &[(0, 2..2, "XY"), (0, 4..4, "Z")]);
    s.assert_clean();
    assert_eq!(s.fin, ["abXYZcd"]);
    assert_eq!(s.xml.matches("<w:ins ").count(), 1, "{}", s.xml);
}

#[test]
fn an_insertion_right_before_one_made_in_this_transaction_joins_it() {
    let s = tracked_edits(
        &p(&format!("{}{}", r("ab"), r("cd"))),
        &[(0, 2..2, "X"), (0, 2..2, "Y")],
    );
    s.assert_clean();
    assert_eq!(s.fin, ["abYXcd"]);
    assert_eq!(s.xml.matches("<w:ins ").count(), 1, "{}", s.xml);
}

#[test]
fn two_deletions_next_to_each_other_in_one_transaction_share_one_del() {
    let s = tracked_edits(&p(&r("abcdef")), &[(0, 2..4, ""), (0, 2..3, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["abf"]);
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
}

#[test]
fn an_insertion_never_joins_an_ins_from_an_earlier_transaction_of_the_same_author() {
    let body = p(&format!(
        "{}<w:ins w:id=\"900\" {AT}>{}</w:ins>",
        r("ab"),
        r("old")
    ));
    let s = tracked_edits(&body, &[(0, 5..5, "new")]);
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:ins ").count(), 2, "{}", s.xml);
}

#[test]
fn an_insertion_inside_a_moved_in_range_splits_it_like_any_insertion() {
    let body = p(&format!(
        "<w:moveTo w:id=\"5\" w:name=\"m\" w:author=\"Eric\" w:date=\"2020-01-02T03:04:05Z\">{}</w:moveTo>",
        r("moved text")
    ));
    let s = tracked_edits(&body, &[(0, 5..5, "X")]);
    s.assert_clean();
    assert_eq!(s.fin, ["movedX text"]);
    assert_eq!(s.effect(0).split.len(), 1);
}

// --- atoms, fields, markers

#[test]
fn deleting_atoms_wraps_their_runs() {
    let body =
        p("<w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:noBreakHyphen/><w:t>c</w:t></w:r>");
    let s = tracked_edits(&body, &[(0, 1..5, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["ac"]);
    assert_eq!(s.red, ["a\tb\n\u{2011}c"]);
}

#[test]
fn deleting_an_equation_wraps_it() {
    let body = p(&format!(
        "{}<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>{}",
        r("a"),
        r("b")
    ));
    let s = tracked_edits(&body, &[(0, 1..2, "")]);
    s.assert_clean();
    assert_eq!(s.fin, ["ab"]);
    assert!(
        s.xml.contains("<w:del ") && s.xml.contains("<m:oMath>"),
        "{}",
        s.xml
    );
}

fn field(code: &str, result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r(result)
    )
}

#[test]
fn a_field_is_deleted_whole_with_its_code_renamed() {
    let s = tracked_edits(
        &p(&format!("{}{}{}", r("a "), field("PAGE", "3"), r(" b"))),
        &[(0, 2..3, "")],
    );
    s.assert_clean();
    assert_eq!(s.fin, ["a  b"]);
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
    assert!(s.xml.contains("<w:delInstrText"), "{}", s.xml);
    assert!(!s.xml.contains("<w:instrText"), "{}", s.xml);
}

#[test]
fn part_of_a_field_result_is_read_only() {
    let body = p(&format!("{}{}{}", r("a "), field("PAGE", "123"), r(" b")));
    let s = tracked_edits(&body, &[(0, 3..4, "")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
    let s = tracked_edits(&body, &[(0, 3..3, "x")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
    let s = tracked_edits(&body, &[(0, 1..3, "")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn text_goes_before_and_after_a_field_and_never_inside_it() {
    let body = p(&format!("{}{}{}", r("a "), field("PAGE", "3"), r(" b")));
    let before = tracked_edits(&body, &[(0, 2..2, "X")]);
    before.assert_clean();
    assert!(
        before.xml.find("<w:t>X</w:t>").unwrap()
            < before.xml.find("fldCharType=\"begin\"").unwrap(),
        "{}",
        before.xml
    );
    let after = tracked_edits(&body, &[(0, 3..3, "X")]);
    after.assert_clean();
    assert!(
        after.xml.find("<w:t>X</w:t>").unwrap() > after.xml.rfind("fldCharType=\"end\"").unwrap(),
        "{}",
        after.xml
    );
}

#[test]
fn range_markers_inside_the_deleted_range_go_into_the_del_and_outside_ones_stay() {
    let body = p(&format!(
        "<w:commentRangeStart w:id=\"1\"/>{}<w:bookmarkStart w:id=\"2\" w:name=\"in\"/>{}<w:bookmarkEnd w:id=\"2\"/>{}<w:commentRangeEnd w:id=\"1\"/>",
        r("aa"),
        r("bb"),
        r("cc")
    ));
    let s = tracked_edits(&body, &[(0, 1..5, "")]);
    s.assert_clean();
    let d = s.xml.find("<w:del ").unwrap();
    let e = s.xml.find("</w:del>").unwrap();
    let inside = &s.xml[d..e];
    assert!(
        inside.contains("bookmarkStart") && inside.contains("bookmarkEnd"),
        "{}",
        s.xml
    );
    assert!(s.xml.starts_with("<w:p><w:commentRangeStart"), "{}", s.xml);
    assert!(
        s.xml.ends_with("<w:commentRangeEnd w:id=\"1\"/></w:p>"),
        "{}",
        s.xml
    );
}

#[test]
fn a_pending_rpr_change_in_a_cut_run_is_copied_under_a_new_id_and_reported() {
    let body = format!(
        "<w:p><w:r><w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr><w:t>abcdef</w:t></w:r></w:p>",
        eric(300)
    );
    let s = tracked_edits(&body, &[(0, 2..4, "")]);
    s.assert_clean();
    let split = &s.effect(0).split;
    assert_eq!(split.len(), 2, "{:?}", split);
    assert!(split.iter().all(|(old, new)| *old == 300 && *new != 300));
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 3, "{}", s.xml);
    assert_eq!(s.xml.matches("w:author=\"Eric\"").count(), 3, "{}", s.xml);
}

// --- containers

#[test]
fn text_is_inserted_inside_a_hyperlink_only_between_its_runs() {
    let body = p(&format!(
        "{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}",
        r("ab "),
        r("link"),
        r(" cd")
    ));
    let mid = tracked_edits(&body, &[(0, 5..5, "X")]);
    mid.assert_clean();
    let h = mid.xml.find("<w:hyperlink").unwrap();
    let hc = mid.xml.find("</w:hyperlink>").unwrap();
    assert!(mid.xml[h..hc].contains("<w:ins "), "{}", mid.xml);
    let start = tracked_edits(&body, &[(0, 3..3, "X")]);
    start.assert_clean();
    assert!(
        start.xml.find("<w:ins ").unwrap() < start.xml.find("<w:hyperlink").unwrap(),
        "{}",
        start.xml
    );
    let end = tracked_edits(&body, &[(0, 7..7, "X")]);
    end.assert_clean();
    assert!(
        end.xml.find("<w:ins ").unwrap() > end.xml.find("</w:hyperlink>").unwrap(),
        "{}",
        end.xml
    );
}

#[test]
fn text_is_inserted_inside_a_content_control_between_its_runs_and_outside_at_its_edges() {
    let body = p(&format!(
        "{}<w:sdt><w:sdtPr><w:alias w:val=\"c\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>{}",
        r("ab "),
        r("control"),
        r(" cd")
    ));
    let mid = tracked_edits(&body, &[(0, 6..6, "X")]);
    mid.assert_clean();
    let c = mid.xml.find("<w:sdtContent>").unwrap();
    let cc = mid.xml.find("</w:sdtContent>").unwrap();
    assert!(mid.xml[c..cc].contains("<w:ins "), "{}", mid.xml);
    let end = tracked_edits(&body, &[(0, 10..10, "X")]);
    end.assert_clean();
    assert!(
        end.xml.find("<w:ins ").unwrap() > end.xml.find("</w:sdt>").unwrap(),
        "{}",
        end.xml
    );
}

#[test]
fn text_in_a_locked_content_control_is_read_only() {
    let body = p(&format!(
        "<w:sdt><w:sdtPr><w:lock w:val=\"sdtContentLocked\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>",
        r("locked")
    ));
    let s = tracked_edits(&body, &[(0, 1..3, "")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn text_in_a_simple_field_is_read_only() {
    let body = p(&format!(
        "{}<w:fldSimple w:instr=\" DATE \">{}</w:fldSimple>",
        r("a "),
        r("today")
    ));
    let s = tracked_edits(&body, &[(0, 3..4, "")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
}

// --- refusals

#[test]
fn offsets_outside_the_text_are_invalid_and_change_nothing() {
    let s = tracked_edits(&p(&r("abc")), &[(0, 2..9, "")]);
    assert_eq!(s.code(0), Some(ErrorCode::InvalidPayload));
    assert_eq!(s.xml, p(&r("abc")));
}

#[test]
fn a_character_that_stands_for_an_atom_cannot_be_written() {
    let s = tracked_edits(&p(&r("abc")), &[(0, 1..1, "x\u{1}y")]);
    assert_eq!(s.code(0), Some(ErrorCode::InvalidPayload));
    let s = tracked_edits(&p(&r("abc")), &[(0, 1..1, "x\u{0}y")]);
    assert_eq!(s.code(0), Some(ErrorCode::InvalidPayload));
}

#[test]
fn an_atom_that_the_difference_keeps_may_stay_in_the_new_text() {
    let body = p("<w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r>");
    let s = tracked_edits(&body, &[(0, 0..3, "a\tc")]);
    s.assert_clean();
    assert_eq!(s.fin, ["a\tc"]);
    assert!(
        !s.xml.contains("<w:del ") || s.xml.matches("<w:del ").count() == 1,
        "{}",
        s.xml
    );
}

#[test]
fn a_deleted_block_is_read_only() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:del {}/></w:rPr></w:pPr><w:del {}><w:r><w:delText>gone</w:delText></w:r></w:del></w:p><w:p><w:r><w:t>next</w:t></w:r></w:p>",
        eric(950),
        eric(951)
    );
    let s = tracked_edits(&body, &[(0, 0..0, "x")]);
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn a_cdata_text_can_be_deleted_whole_and_not_cut() {
    let body = "<w:p><w:r><w:t><![CDATA[abc]]></w:t></w:r></w:p>";
    let whole = tracked_edits(body, &[(0, 0..3, "")]);
    whole.assert_clean();
    assert_eq!(whole.fin, [""]);
    let part = tracked_edits(body, &[(0, 1..2, "")]);
    assert_eq!(part.code(0), Some(ErrorCode::Unsupported));
}

// --- untracked

#[test]
fn an_untracked_edit_inside_a_text_node_changes_the_node_and_nothing_else() {
    let s = untracked_edits(
        &p("<w:r><w:rPr><w:b/></w:rPr><w:t>Hello big world</w:t></w:r>"),
        &[(0, 6..10, "small ")],
    );
    assert!(s.effects[0].is_ok());
    assert_eq!(
        s.xml,
        "<w:p><w:r><w:rPr><w:b/></w:rPr><w:t>Hello small world</w:t></w:r></w:p>"
    );
    assert!(s.outcome.is_ok());
}

#[test]
fn untracked_edits_write_no_revision_and_take_no_id() {
    let s = untracked_edits(&p(&r("abcdef")), &[(0, 1..3, ""), (0, 2..2, "XY")]);
    assert!(s.effects.iter().all(|e| e.is_ok()), "{:?}", s.effects);
    assert!(
        !s.xml.contains("w:ins") && !s.xml.contains("w:del"),
        "{}",
        s.xml
    );
    assert_eq!(s.fin, ["adXYef"]);
    assert_eq!(s.outcome.as_ref().unwrap().minted, 0..0);
}

#[test]
fn an_untracked_deletion_across_runs_removes_the_runs_it_empties() {
    let body = p(&format!(
        "{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}",
        r("ab "),
        r("link"),
        r(" cd")
    ));
    let s = untracked_edits(&body, &[(0, 2..9, "")]);
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert_eq!(s.fin, ["abd"]);
    assert!(
        !s.xml.contains("<w:ins ") && !s.xml.contains("<w:del "),
        "{}",
        s.xml
    );
}

#[test]
fn an_untracked_edit_removes_atoms() {
    let s = untracked_edits(
        &p("<w:r><w:t>a</w:t><w:tab/><w:t>b</w:t></w:r>"),
        &[(0, 1..2, "")],
    );
    assert!(s.effects[0].is_ok());
    assert_eq!(s.fin, ["ab"]);
    assert!(!s.xml.contains("<w:tab/>"), "{}", s.xml);
}

#[test]
fn the_texts_of_all_modes_agree_after_a_deletion_and_an_insertion() {
    let s = tracked_edits(&p(&r("one two three")), &[(0, 4..7, ""), (0, 4..4, "2 ")]);
    s.assert_clean();
    assert_eq!(s.fin, ["one 2  three"]);
    let _ = Mode::Final;
}

// --- text written without `xml:space`

#[test]
fn deleting_text_that_a_converter_wrote_without_xml_space_keeps_its_white_space_as_deleted_text() {
    // `<w:t> </w:t>` with no `xml:space` is what tools that convert from PDF write for a space between
    // words. A deletion may not hold such text (W2), so the deleted text says `preserve`.
    let bare = |t: &str| format!("<w:r><w:t>{t}</w:t></w:r>");
    let body = p(&format!("{}{}{}", bare("one"), bare(" "), bare("two")));
    let s = tracked_edits(&body, &[(0, 3..4, "")]);
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:delText xml:space=\"preserve\"> </w:delText>"),
        "{}",
        s.xml
    );
    assert_eq!(s.fin, ["onetwo"]);
}

#[test]
fn white_space_at_the_ends_of_a_w_t_without_xml_space_counts_as_word_shows_it() {
    // Word for the web shows `<w:t>Unshown </w:t><w:t>space</w:t>` as "Unshown space" (hand check 37, 2026-10-10), so the
    // white space at the ends of a `w:t` that does not say `xml:space="preserve"` is text, and an edit beside it keeps it and
    // writes the attribute where it cuts the node.
    let bare = |t: &str| format!("<w:r><w:t>{t}</w:t></w:r>");
    let body = format!(
        "{}{}",
        p(&format!("{}{}", bare("Unshown "), bare("space"))),
        p(&bare(" one two "))
    );
    let s = tracked_edits(&body, &[(1, 5..8, "three")]);
    s.assert_clean();
    assert_eq!(s.fin, ["Unshown space", " one three "]);
    assert_eq!(s.orig, ["Unshown space", " one two "]);
    assert!(
        s.xml.contains("<w:t xml:space=\"preserve\"> one </w:t>"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains("<w:t xml:space=\"preserve\"> </w:t>"),
        "{}",
        s.xml
    );
}
