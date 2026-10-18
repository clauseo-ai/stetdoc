//! Tests of `insert_block` and `split_block`: the two shapes of an inserted paragraph (design 3.2.1).

use super::stamp::Track;
use super::tests_support::*;
use super::types::{NewParagraph, Place};
use crate::api::error::ErrorCode;

fn ppr(inner: &str) -> String {
    format!("<w:pPr>{inner}</w:pPr>")
}

fn mark_ins(id: u32) -> String {
    format!("<w:rPr><w:ins w:id=\"{id}\" {AT}/></w:rPr>")
}

fn new_para(text: &str) -> NewParagraph {
    NewParagraph {
        text: text.to_owned(),
        ..NewParagraph::default()
    }
}

fn styled(text: &str, style: &str) -> NewParagraph {
    NewParagraph {
        text: text.to_owned(),
        style: Some(style.to_owned()),
        ..NewParagraph::default()
    }
}

/// Three paragraphs: a heading with a language on its mark, a centred one, a plain one.
fn three() -> String {
    format!(
        "<w:p>{}{}</w:p><w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr("<w:pStyle w:val=\"H1\"/><w:rPr><w:lang w:val=\"fr\"/></w:rPr>"),
        r("Title"),
        ppr("<w:jc w:val=\"center\"/>"),
        r("Body"),
        r("Last")
    )
}

fn insert(body: &str, anchor: usize, place: Place, spec: NewParagraph) -> Snap {
    with_compiler(body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.insert_block(&ks[anchor], place, &spec)]
    })
}

#[test]
fn after_a_paragraph_that_has_a_successor_the_shape_is_y() {
    let s = insert(&three(), 0, Place::After, new_para("New"));
    s.assert_clean();
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"H1\"/><w:rPr><w:ins w:id=\"0\" {AT}/><w:lang w:val=\"fr\"/></w:rPr></w:pPr>{}</w:p>\
             <w:p><w:pPr><w:pStyle w:val=\"H1\"/><w:rPr><w:lang w:val=\"fr\"/></w:rPr></w:pPr>\
             <w:ins w:id=\"1\" {AT}><w:r><w:rPr><w:lang w:val=\"fr\"/></w:rPr><w:t>New</w:t></w:r></w:ins></w:p>\
             <w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>{}</w:p><w:p>{}</w:p>",
            r("Title"),
            r("Body"),
            r("Last")
        )
    );
    assert_eq!(s.fin, ["Title", "New", "Body", "Last"]);
    let fx = s.effect(0);
    assert_eq!(fx.new_blocks.len(), 1);
    assert_eq!(fx.origins.len(), 1);
    assert_eq!(fx.origins[0].block, fx.new_blocks[0]);
    assert!(fx.survivor.is_none());
}

#[test]
fn y_with_a_style_that_differs_writes_a_change_whose_snapshot_is_the_original_properties() {
    let s = insert(&three(), 0, Place::After, styled("New", "Body"));
    s.assert_clean();
    assert!(
        s.xml.contains(&format!(
            "<w:pPr><w:pStyle w:val=\"Body\"/><w:rPr><w:lang w:val=\"fr\"/></w:rPr><w:pPrChange w:id=\"2\" {AT}><w:pPr><w:pStyle w:val=\"H1\"/></w:pPr></w:pPrChange></w:pPr>"
        )),
        "{}",
        s.xml
    );
    // The first paragraph keeps the original properties and has the inserted mark.
    assert!(
        s.xml
            .starts_with("<w:p><w:pPr><w:pStyle w:val=\"H1\"/><w:rPr><w:ins w:id=\"0\""),
        "{}",
        s.xml
    );
}

#[test]
fn y_with_the_style_the_model_already_has_writes_no_change() {
    let s = insert(&three(), 0, Place::After, styled("New", "H1"));
    s.assert_clean();
    assert!(!s.xml.contains("pPrChange"), "{}", s.xml);
}

#[test]
fn a_style_replaces_every_base_property_of_the_model() {
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr(
            "<w:pStyle w:val=\"List\"/><w:numPr><w:ilvl w:val=\"1\"/><w:numId w:val=\"3\"/></w:numPr><w:ind w:left=\"720\"/>"
        ),
        r("item"),
        r("next")
    );
    let s = insert(&body, 0, Place::After, styled("body text", "Body"));
    s.assert_clean();
    // The new paragraph holds the original properties, which the edit rewrites to the style alone.
    assert!(
        s.xml
            .contains("<w:pPr><w:pStyle w:val=\"Body\"/><w:pPrChange"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains("<w:pPr><w:pStyle w:val=\"List\"/><w:numPr><w:ilvl w:val=\"1\"/><w:numId w:val=\"3\"/></w:numPr><w:ind w:left=\"720\"/></w:pPr></w:pPrChange>"),
        "{}",
        s.xml
    );
}

#[test]
fn before_a_paragraph_the_shape_is_x_with_a_copy_of_the_twins_properties() {
    let s = insert(&three(), 1, Place::Before, new_para("New"));
    s.assert_clean();
    assert!(
        s.xml.contains(&format!(
            "<w:p><w:pPr><w:jc w:val=\"center\"/>{}</w:pPr><w:ins w:id=\"1\" {AT}><w:r><w:t>New</w:t></w:r></w:ins></w:p><w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>{}</w:p>",
            mark_ins(0),
            r("Body")
        )),
        "{}",
        s.xml
    );
}

#[test]
fn x_with_a_style_that_differs_from_the_twin_is_aligned_to_the_twin_with_a_change() {
    let s = insert(&three(), 1, Place::Before, styled("New", "Body"));
    s.assert_clean();
    assert!(
        s.xml.contains(&format!(
            "<w:pPr><w:pStyle w:val=\"Body\"/>{}<w:pPrChange w:id=\"2\" {AT}><w:pPr><w:jc w:val=\"center\"/></w:pPr></w:pPrChange></w:pPr>",
            mark_ins(0)
        )),
        "{}",
        s.xml
    );
}

#[test]
fn after_the_last_paragraph_the_shape_is_y_and_the_new_paragraph_keeps_the_containers_mark() {
    // Word's Reject All cannot remove the mark that ends a container, so the anchor takes the inserted mark (C11).
    let s = insert(&three(), 2, Place::After, styled("New", "Body"));
    s.assert_clean();
    assert!(
        s.xml.ends_with(&format!(
            "<w:p><w:pPr>{}</w:pPr>{}</w:p><w:p><w:pPr><w:pStyle w:val=\"Body\"/><w:pPrChange w:id=\"2\" {AT}><w:pPr/></w:pPrChange></w:pPr><w:ins w:id=\"1\" {AT}><w:r><w:t>New</w:t></w:r></w:ins></w:p>",
            mark_ins(0),
            r("Last")
        )),
        "{}",
        s.xml
    );
}

#[test]
fn after_a_paragraph_whose_mark_holds_a_revision_the_shape_is_x_before_its_successor() {
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr(&format!("<w:rPr><w:ins {}/></w:rPr>", eric(700))),
        r("theirs"),
        r("next")
    );
    let s = insert(&body, 0, Place::After, new_para("New"));
    s.assert_clean();
    assert!(
        s.xml.contains(&format!("<w:ins {}/>", eric(700))),
        "{}",
        s.xml
    );
    assert_eq!(s.xml.matches("<w:rPr><w:ins ").count(), 2, "{}", s.xml);
    assert_eq!(s.fin, ["theirs", "New", "next"]);
}

#[test]
fn y_gives_way_to_x_when_the_original_holds_another_authors_change_and_an_edit_is_wanted() {
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr(&format!(
            "<w:jc w:val=\"right\"/><w:pPrChange {}><w:pPr/></w:pPrChange>",
            eric(701)
        )),
        r("theirs"),
        r("next")
    );
    let s = insert(&body, 0, Place::After, styled("New", "H1"));
    s.assert_clean();
    assert_eq!(
        s.xml.matches("<w:pPrChange ").count(),
        2,
        "theirs stays and one is ours: {}",
        s.xml
    );
    assert!(
        s.xml.starts_with(&format!(
            "<w:p><w:pPr><w:jc w:val=\"right\"/><w:pPrChange {}",
            eric(701)
        )),
        "{}",
        s.xml
    );
}

#[test]
fn a_table_can_be_the_anchor_before_and_after() {
    let t = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>";
    let body = format!(
        "<w:p>{}{}</w:p>{t}<w:p>{}</w:p>",
        ppr("<w:pStyle w:val=\"A\"/>"),
        r("before"),
        r("after")
    );
    let after = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        let table = all
            .iter()
            .find(|k| k.node() != all[0].node())
            .unwrap()
            .clone();
        vec![c.insert_block(&table, Place::After, &new_para("N"))]
    });
    after.assert_clean();
    assert!(
        after.xml.contains(&format!(
            "</w:tbl><w:p><w:pPr>{}</w:pPr><w:ins ",
            mark_ins(0)
        )),
        "{}",
        after.xml
    );
    let before = with_compiler(&body, Track::Tracked(stamp()), |c, all, _| {
        let table = all
            .iter()
            .find(|k| k.node() != all[0].node())
            .unwrap()
            .clone();
        vec![c.insert_block(&table, Place::Before, &new_para("N"))]
    });
    before.assert_clean();
    assert!(
        before.xml.contains("<w:pStyle w:val=\"A\"/>"),
        "the model is the paragraph before the table: {}",
        before.xml
    );
    assert!(
        before
            .xml
            .contains("</w:p><w:p><w:pPr><w:pStyle w:val=\"A\"/><w:rPr><w:ins"),
        "{}",
        before.xml
    );
}

#[test]
fn a_new_paragraph_gets_a_new_para_id_only_when_its_model_has_one() {
    let body = "<w:p w14:paraId=\"11111111\" w14:textId=\"22222222\"><w:r><w:t>one</w:t></w:r></w:p><w:p w14:paraId=\"33333333\" w14:textId=\"44444444\"><w:r><w:t>two</w:t></w:r></w:p>";
    let s = insert(body, 0, Place::After, new_para("New"));
    s.assert_clean();
    assert!(
        s.xml
            .starts_with("<w:p w14:paraId=\"11111111\" w14:textId=\"22222222\">"),
        "the first keeps its ids: {}",
        s.xml
    );
    let second = s.xml.split("<w:p ").nth(2).unwrap();
    assert!(
        second.contains("w14:textId=\"77777777\"")
            && !second.starts_with("w14:paraId=\"11111111\""),
        "{}",
        s.xml
    );
    let plain = insert(&three(), 0, Place::After, new_para("New"));
    assert!(!plain.xml.contains("paraId"), "{}", plain.xml);
}

#[test]
fn the_text_of_a_new_paragraph_may_hold_tabs_and_line_breaks_and_never_atoms() {
    let s = insert(&three(), 2, Place::After, new_para("a\tb\nc"));
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t>"),
        "{}",
        s.xml
    );
    let bad = insert(&three(), 2, Place::After, new_para("a\u{1}b"));
    assert_eq!(bad.code(0), Some(ErrorCode::InvalidPayload));
}

#[test]
fn an_empty_new_paragraph_has_no_content_element() {
    let s = insert(&three(), 2, Place::After, new_para(""));
    s.assert_clean();
    assert!(
        s.xml.ends_with(&format!(
            "<w:p><w:pPr>{}</w:pPr>{}</w:p><w:p/>",
            mark_ins(0),
            r("Last")
        )),
        "{}",
        s.xml
    );
}

#[test]
fn untracked_a_new_paragraph_has_no_marks_and_copies_the_model() {
    let s = with_compiler(&three(), Track::Untracked, |c, _, ks| {
        vec![c.insert_block(&ks[0], Place::After, &styled("New", "Body"))]
    });
    assert!(s.effects[0].is_ok());
    assert!(s.xml.contains("<w:p><w:pPr><w:pStyle w:val=\"Body\"/><w:rPr><w:lang w:val=\"fr\"/></w:rPr></w:pPr><w:r><w:rPr><w:lang w:val=\"fr\"/></w:rPr><w:t>New</w:t></w:r></w:p>"), "{}", s.xml);
    assert!(
        !s.xml.contains("w:ins") && !s.xml.contains("w:del"),
        "{}",
        s.xml
    );
}

#[test]
fn a_row_cannot_be_the_anchor_of_a_paragraph() {
    let t = "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>";
    let s = with_compiler(t, Track::Tracked(stamp()), |c, all, _| {
        vec![c.insert_block(&all[1], Place::After, &new_para("N"))]
    });
    assert_eq!(s.code(0), Some(ErrorCode::CrossBlock));
}

// --- split

fn split(body: &str, para: usize, at: usize) -> Snap {
    with_compiler(body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.split_block(&ks[para], at)]
    })
}

#[test]
fn a_split_keeps_the_first_element_with_a_copy_of_the_properties_and_an_inserted_mark() {
    let s = split(&three(), 1, 2);
    s.assert_clean();
    assert_eq!(
        s.xml,
        format!(
            "<w:p>{}{}</w:p><w:p><w:pPr><w:jc w:val=\"center\"/>{}</w:pPr><w:r><w:t xml:space=\"preserve\">Bo</w:t></w:r></w:p>\
             <w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t xml:space=\"preserve\">dy</w:t></w:r></w:p><w:p>{}</w:p>",
            ppr("<w:pStyle w:val=\"H1\"/><w:rPr><w:lang w:val=\"fr\"/></w:rPr>"),
            r("Title"),
            mark_ins(0),
            r("Last")
        )
    );
    assert_eq!(s.fin, ["Title", "Bo", "dy", "Last"]);
    let fx = s.effect(0);
    assert_eq!(fx.new_blocks.len(), 1);
    assert_eq!(fx.origins[0].from.len(), 1);
}

#[test]
fn a_split_at_the_start_or_the_end_leaves_one_half_empty() {
    let end = split(&three(), 1, 4);
    end.assert_clean();
    assert_eq!(end.fin, ["Title", "Body", "", "Last"]);
    let start = split(&three(), 1, 0);
    start.assert_clean();
    assert_eq!(start.fin, ["Title", "", "Body", "Last"]);
}

#[test]
fn a_split_moves_the_section_break_to_the_second_paragraph() {
    let body = format!(
        "<w:p>{}{}</w:p>",
        ppr("<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>"),
        r("end of section")
    );
    let s = split(&body, 0, 3);
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:sectPr>").count(), 1, "{}", s.xml);
    let first_end = s.xml.find("</w:p>").unwrap();
    assert!(!s.xml[..first_end].contains("sectPr"), "{}", s.xml);
}

#[test]
fn a_split_inside_a_hyperlink_cuts_the_link_into_two_with_the_same_target() {
    let body = format!(
        "<w:p>{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}</w:p>",
        r("ab "),
        r("link"),
        r(" cd")
    );
    let s = split(&body, 0, 5);
    s.assert_clean();
    assert_eq!(
        s.xml.matches("<w:hyperlink w:anchor=\"x\">").count(),
        2,
        "{}",
        s.xml
    );
    assert_eq!(s.fin, ["ab li", "nk cd"]);
}

#[test]
fn a_split_inside_another_authors_insertion_cuts_it_and_reports_the_new_id() {
    let body = format!(
        "<w:p>{}<w:ins {}>{}</w:ins>{}</w:p>",
        r("ab "),
        eric(9),
        r("inserted"),
        r(" cd")
    );
    let s = split(&body, 0, 6);
    s.assert_clean();
    assert_eq!(s.effect(0).split, [(9, 10)]);
    assert_eq!(s.xml.matches("w:author=\"Eric\"").count(), 2, "{}", s.xml);
}

#[test]
fn a_split_at_the_edge_of_a_hyperlink_or_after_a_deleted_word_cuts_no_container() {
    let body = format!(
        "<w:p>{}<w:hyperlink w:anchor=\"x\">{}</w:hyperlink>{}</w:p>",
        r("ab "),
        r("link"),
        r(" cd")
    );
    let s = split(&body, 0, 7);
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:hyperlink").count(), 1, "{}", s.xml);
}

#[test]
fn a_split_inside_a_field_a_content_control_or_a_simple_field_is_read_only() {
    let field = format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r("123")
    );
    let in_field = split(&format!("<w:p>{}{}{}</w:p>", r("a "), field, r(" b")), 0, 3);
    assert_eq!(in_field.code(0), Some(ErrorCode::ReadonlyRegion));
    let sdt = format!(
        "<w:p>{}<w:sdt><w:sdtPr/><w:sdtContent>{}</w:sdtContent></w:sdt></w:p>",
        r("a "),
        r("control")
    );
    assert_eq!(split(&sdt, 0, 5).code(0), Some(ErrorCode::ReadonlyRegion));
    let simple = format!(
        "<w:p><w:fldSimple w:instr=\" DATE \">{}</w:fldSimple></w:p>",
        r("today")
    );
    assert_eq!(
        split(&simple, 0, 2).code(0),
        Some(ErrorCode::ReadonlyRegion)
    );
    // Before and after the field is fine.
    let before = split(&format!("<w:p>{}{}{}</w:p>", r("a "), field, r(" b")), 0, 2);
    before.assert_clean();
    assert!(before.fin[1].starts_with("123"), "{:?}", before.fin);
}

#[test]
fn a_split_outside_the_text_or_in_a_deleted_block_is_refused() {
    assert_eq!(
        split(&three(), 1, 99).code(0),
        Some(ErrorCode::InvalidPayload)
    );
    let body = format!(
        "<w:p>{}<w:del {}><w:r><w:delText>gone</w:delText></w:r></w:del></w:p><w:p>{}</w:p>",
        ppr(&format!("<w:rPr><w:del {}/></w:rPr>", eric(950))),
        eric(951),
        r("next")
    );
    assert_eq!(split(&body, 0, 0).code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn the_first_half_keeps_its_para_id_and_the_second_gets_a_new_one() {
    let body = "<w:p w14:paraId=\"11111111\" w14:textId=\"22222222\"><w:r><w:t>onetwo</w:t></w:r></w:p><w:p><w:r><w:t>x</w:t></w:r></w:p>";
    let s = split(body, 0, 3);
    s.assert_clean();
    assert!(
        s.xml
            .starts_with("<w:p w14:paraId=\"11111111\" w14:textId=\"22222222\">"),
        "{}",
        s.xml
    );
    assert!(s.xml.contains("w14:textId=\"77777777\""), "{}", s.xml);
}

#[test]
fn untracked_a_split_adds_no_mark() {
    let s = with_compiler(&three(), Track::Untracked, |c, _, ks| {
        vec![c.split_block(&ks[1], 2)]
    });
    assert!(s.effects[0].is_ok());
    assert!(!s.xml.contains("w:ins"), "{}", s.xml);
    assert_eq!(s.fin, ["Title", "Bo", "dy", "Last"]);
}

#[test]
fn several_splits_from_the_last_cut_to_the_first_keep_the_offsets_and_the_order() {
    let s = with_compiler(
        &p(&r("one two three")),
        Track::Tracked(stamp()),
        |c, _, ks| vec![c.split_block(&ks[0], 8), c.split_block(&ks[0], 4)],
    );
    s.assert_clean();
    assert_eq!(s.fin, ["one ", "two ", "three"]);
}
