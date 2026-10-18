//! Tests of `delete_block` for paragraphs and of `merge_blocks`: the mark rules of design 3.2.2 to 3.2.5.

use super::stamp::Track;
use super::tests_support::*;
use super::types::{NewParagraph, Place};
use crate::api::error::ErrorCode;
use crate::wordml::Mode;

fn ppr(inner: &str) -> String {
    format!("<w:pPr>{inner}</w:pPr>")
}

fn mark_del(id: u32) -> String {
    format!("<w:rPr><w:del w:id=\"{id}\" {AT}/></w:rPr>")
}

fn three() -> String {
    format!(
        "<w:p>{}{}</w:p><w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr("<w:pStyle w:val=\"H1\"/>"),
        r("Title"),
        ppr("<w:jc w:val=\"center\"/>"),
        r("Body"),
        r("Last")
    )
}

fn delete(body: &str, para: usize) -> Snap {
    with_compiler(body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.delete_block(&ks[para])]
    })
}

fn merge(body: &str, a: usize, b: usize) -> Snap {
    with_compiler(body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.merge_blocks(&ks[a], &ks[b])]
    })
}

#[test]
fn deleting_a_paragraph_deletes_its_text_and_its_mark_and_leaves_the_next_paragraph_alone() {
    let s = delete(&three(), 1);
    s.assert_clean();
    assert_eq!(
        s.xml,
        format!(
            "<w:p>{}{}</w:p><w:p><w:pPr><w:jc w:val=\"center\"/>{}</w:pPr><w:del w:id=\"0\" {AT}><w:r><w:delText xml:space=\"preserve\">Body</w:delText></w:r></w:del></w:p><w:p>{}</w:p>",
            ppr("<w:pStyle w:val=\"H1\"/>"),
            r("Title"),
            mark_del(1),
            r("Last")
        )
    );
    assert_eq!(s.fin, ["Title", "", "Last"]);
}

#[test]
fn the_last_paragraph_of_a_container_keeps_its_mark() {
    let s = delete(&three(), 2);
    s.assert_clean();
    assert!(!s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
}

#[test]
fn a_paragraph_that_carries_a_section_break_keeps_its_mark() {
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr("<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr>"),
        r("section end"),
        r("next")
    );
    let s = delete(&body, 0);
    s.assert_clean();
    assert!(!s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
    assert!(s.xml.contains("<w:sectPr>"), "{}", s.xml);
}

#[test]
fn a_paragraph_before_a_table_keeps_its_mark_too() {
    let body = format!(
        "<w:p>{}</w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl>",
        r("before")
    );
    let s = delete(&body, 0);
    s.assert_clean();
    assert!(!s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
}

#[test]
fn deleting_what_is_already_deleted_writes_nothing_and_warns() {
    let body = format!(
        "<w:p>{}<w:del {}><w:r><w:delText>gone</w:delText></w:r></w:del></w:p><w:p>{}</w:p>",
        ppr(&format!("<w:rPr><w:del {}/></w:rPr>", eric(950))),
        eric(951),
        r("next")
    );
    let s = delete(&body, 0);
    assert!(s.effects[0].is_ok());
    assert_eq!(s.xml.matches("<w:del ").count(), 2, "{}", s.xml);
    assert!(s.outcome.as_ref().unwrap().created.is_empty());
    assert_eq!(s.outcome.as_ref().unwrap().warnings.len(), 1);
}

#[test]
fn a_paragraph_that_another_author_inserted_gets_our_del_after_their_ins_on_the_mark() {
    let body = format!(
        "<w:p>{}<w:ins {}>{}</w:ins></w:p><w:p>{}</w:p>",
        ppr(&format!("<w:rPr><w:ins {}/></w:rPr>", eric(960))),
        eric(961),
        r("theirs"),
        r("next")
    );
    let s = delete(&body, 0);
    s.assert_clean();
    assert!(
        s.xml
            .contains(&format!("<w:rPr><w:ins {}/><w:del w:id=\"", eric(960))),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains("<w:ins ") && s.xml.contains("<w:del "),
        "{}",
        s.xml
    );
}

#[test]
fn deleting_a_paragraph_that_the_author_inserted_removes_it_outright() {
    let s = with_compiler(&three(), Track::Tracked(stamp()), |c, _, ks| {
        let e = c.insert_block(
            &ks[1],
            Place::Before,
            &NewParagraph {
                text: "N".into(),
                ..NewParagraph::default()
            },
        );
        let k = e.as_ref().unwrap().new_blocks[0].clone();
        vec![e, c.delete_block(&k)]
    });
    s.assert_clean();
    assert_eq!(s.fin, ["Title", "Body", "Last"]);
    assert!(
        !s.xml.contains("w:ins") && !s.xml.contains("w:del"),
        "{}",
        s.xml
    );
    let fx = s.effect(1);
    assert_eq!(fx.removed_own.len(), 2);
    assert!(s.outcome.as_ref().unwrap().created.is_empty());
}

#[test]
fn deleting_an_own_earlier_paragraph_that_holds_markers_leaves_them_between_blocks_and_the_next_paragraph_as_it_was()
 {
    // Design 6.2: a first paragraph with no content left goes and the second is untouched; a range marker may stand
    // between blocks, so it stays where the paragraph was.
    let body = format!(
        "<w:p>{}<w:bookmarkStart w:id=\"4\" w:name=\"b\"/><w:ins w:id=\"500\" {AT}>{}</w:ins></w:p><w:p>{}<w:bookmarkEnd w:id=\"4\"/></w:p>",
        ppr(&format!("<w:rPr><w:ins w:id=\"501\" {AT}/></w:rPr>")),
        r("mine"),
        r("next")
    );
    let s = delete(&body, 0);
    s.assert_clean();
    assert!(
        s.xml
            .starts_with("<w:bookmarkStart w:id=\"4\" w:name=\"b\"/><w:p><w:r>"),
        "{}",
        s.xml
    );
    assert_eq!(s.xml.matches("<w:p>").count(), 1, "{}", s.xml);
    assert_eq!(s.fin, ["next"]);
}

#[test]
fn deleting_a_paragraph_nests_our_del_in_another_authors_insertion_and_marks_it() {
    let body = format!(
        "<w:p><w:ins {}>{}</w:ins></w:p><w:p>{}</w:p>",
        eric(970),
        r("theirs"),
        r("next")
    );
    let s = delete(&body, 0);
    s.assert_clean();
    assert!(
        s.xml.contains(&format!("<w:ins {}><w:del ", eric(970))),
        "{}",
        s.xml
    );
}

#[test]
fn untracked_a_deleted_paragraph_goes_unless_it_is_last_or_ends_a_section() {
    let s = with_compiler(&three(), Track::Untracked, |c, _, ks| {
        vec![c.delete_block(&ks[1])]
    });
    assert!(s.effects[0].is_ok());
    assert_eq!(s.fin, ["Title", "Last"]);
    let last = with_compiler(&three(), Track::Untracked, |c, _, ks| {
        vec![c.delete_block(&ks[2])]
    });
    assert_eq!(last.fin, ["Title", "Body", ""]);
}

// --- merge

#[test]
fn a_merge_deletes_the_first_mark_and_aligns_the_second_to_the_first() {
    let s = merge(&three(), 0, 1);
    s.assert_clean();
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"H1\"/>{}</w:pPr>{}</w:p><w:p><w:pPr><w:pStyle w:val=\"H1\"/><w:pPrChange w:id=\"0\" {AT}><w:pPr><w:jc w:val=\"center\"/></w:pPr></w:pPrChange></w:pPr>{}</w:p><w:p>{}</w:p>",
            mark_del(1),
            r("Title"),
            r("Body"),
            r("Last")
        )
    );
    // Nothing is joined, so there is no survivor and no ref is declared. The second paragraph was given the
    // first one's properties, and it declares that it is built from both.
    let fx = s.effect(0);
    assert!(
        fx.survivor.is_none(),
        "a tracked merge joins nothing: {fx:?}"
    );
    let (a, b) = (fx.touched[0].clone(), fx.touched[1].clone());
    assert_eq!(fx.origins.len(), 1, "{fx:?}");
    assert_eq!(fx.origins[0].block, b);
    assert_eq!(fx.origins[0].from, [b, a]);
}

#[test]
fn a_merge_of_paragraphs_with_equal_properties_writes_only_the_mark() {
    let body = format!("<w:p>{}</w:p><w:p>{}</w:p>", r("one"), r("two"));
    let s = merge(&body, 0, 1);
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:del ").count(), 1, "{}", s.xml);
    assert!(!s.xml.contains("pPrChange"), "{}", s.xml);
}

#[test]
fn a_merge_whose_first_paragraph_has_no_text_skips_the_alignment() {
    let body = format!(
        "<w:p>{}</w:p><w:p>{}{}</w:p>",
        ppr("<w:pStyle w:val=\"H1\"/>"),
        ppr("<w:jc w:val=\"center\"/>"),
        r("two")
    );
    let s = merge(&body, 0, 1);
    s.assert_clean();
    assert!(!s.xml.contains("pPrChange"), "{}", s.xml);
}

#[test]
fn merging_three_paragraphs_pair_by_pair_aligns_each_to_the_first() {
    let s = with_compiler(&three(), Track::Tracked(stamp()), |c, _, ks| {
        vec![
            c.merge_blocks(&ks[0], &ks[1]),
            c.merge_blocks(&ks[1], &ks[2]),
        ]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:pPrChange ").count(), 2, "{}", s.xml);
    assert_eq!(
        s.xml.matches("<w:pStyle w:val=\"H1\"/>").count(),
        3,
        "{}",
        s.xml
    );
}

#[test]
fn a_merge_skips_the_deleted_blocks_between_the_two_paragraphs() {
    let s = with_compiler(&three(), Track::Tracked(stamp()), |c, _, ks| {
        vec![c.delete_block(&ks[1]), c.merge_blocks(&ks[0], &ks[2])]
    });
    s.assert_clean();
    assert!(s.effects.iter().all(|e| e.is_ok()), "{:?}", s.effects);
    assert_eq!(
        s.xml.matches("<w:pPrChange ").count(),
        1,
        "the alignment lands on the last paragraph: {}",
        s.xml
    );
}

#[test]
fn a_merge_is_refused_across_a_section_break_a_pending_merge_and_neighbours_that_are_not() {
    let sect = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr("<w:sectPr/>"),
        r("a"),
        r("b")
    );
    assert_eq!(merge(&sect, 0, 1).code(0), Some(ErrorCode::ReadonlyRegion));
    let after_sect = format!(
        "<w:p>{}</w:p><w:p>{}{}</w:p>",
        r("a"),
        ppr("<w:sectPr/>"),
        r("b")
    );
    assert_eq!(
        merge(&after_sect, 0, 1).code(0),
        Some(ErrorCode::ReadonlyRegion)
    );
    let pending = format!(
        "<w:p>{}{}</w:p><w:p>{}</w:p>",
        ppr(&format!("<w:rPr><w:del {}/></w:rPr>", eric(980))),
        r("a"),
        r("b")
    );
    assert_eq!(
        merge(&pending, 0, 1).code(0),
        Some(ErrorCode::ReadonlyRegion)
    );
    assert_eq!(merge(&three(), 0, 2).code(0), Some(ErrorCode::CrossBlock));
    assert_eq!(merge(&three(), 1, 0).code(0), Some(ErrorCode::CrossBlock));
    let table = format!(
        "<w:p>{}</w:p><w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1\"/></w:tblGrid><w:tr><w:tc><w:p>{}</w:p></w:tc></w:tr></w:tbl><w:p>{}</w:p>",
        r("a"),
        r("t"),
        r("c")
    );
    let s = with_compiler(&table, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.merge_blocks(&ks[0], &ks[2])]
    });
    assert_eq!(s.code(0), Some(ErrorCode::CrossBlock));
}

#[test]
fn a_merge_is_refused_when_either_paragraph_has_another_authors_pending_change() {
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}{}</w:p>",
        ppr("<w:jc w:val=\"left\"/>"),
        r("a"),
        ppr(&format!(
            "<w:jc w:val=\"right\"/><w:pPrChange {}><w:pPr/></w:pPrChange>",
            eric(990)
        )),
        r("b")
    );
    assert_eq!(merge(&body, 0, 1).code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn a_merge_that_follows_our_own_alignment_folds_into_it() {
    let s = with_compiler(&three(), Track::Tracked(stamp()), |c, _, ks| {
        vec![
            c.set_props(
                &super::types::PropTarget::Paragraph {
                    block: ks[1].clone(),
                },
                &set_edit(vec![pel("jc", &[("val", "right")], vec![])]),
            ),
            c.merge_blocks(&ks[0], &ks[1]),
        ]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:pPrChange ").count(), 1, "{}", s.xml);
}

#[test]
fn merging_over_the_authors_own_inserted_mark_joins_for_real() {
    let s = with_compiler(&three(), Track::Tracked(stamp()), |c, _, ks| {
        let e = c.split_block(&ks[1], 2);
        let k = e.as_ref().unwrap().new_blocks[0].clone();
        vec![e, c.merge_blocks(&ks[1], &k)]
    });
    s.assert_clean();
    assert_eq!(s.fin, ["Title", "Body", "Last"]);
    assert!(
        !s.xml.contains("w:ins") && !s.xml.contains("w:del"),
        "{}",
        s.xml
    );
    let fx = s.effect(1);
    assert_eq!(fx.removed_own.len(), 1);
    assert!(fx.survivor.is_some());
}

#[test]
fn an_untracked_merge_joins_for_real_and_the_first_element_survives_with_the_later_properties() {
    let s = with_compiler(&three(), Track::Untracked, |c, _, ks| {
        vec![c.merge_blocks(&ks[0], &ks[1])]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects);
    assert_eq!(s.fin, ["TitleBody", "Last"]);
    assert!(
        s.xml
            .starts_with("<w:p><w:pPr><w:pStyle w:val=\"H1\"/></w:pPr>"),
        "the later paragraph's properties are aligned to the first's: {}",
        s.xml
    );
    assert!(s.effect(0).survivor.is_some());
}

// --- across the edge of a content control

fn control_between() -> String {
    format!(
        "{}<w:sdt><w:sdtPr/><w:sdtContent>{}</w:sdtContent></w:sdt>{}",
        p(&r("before")),
        p(&r("inside")),
        p(&r("after"))
    )
}

#[test]
fn a_paragraph_at_the_end_of_a_content_control_is_not_merged_with_the_one_after_the_control() {
    // The resolver joins paragraphs that are siblings, so a mark that is deleted at the end of a control
    // would read as joined and resolve as not joined.
    let s = with_compiler(&control_between(), Track::Tracked(stamp()), |c, _, ks| {
        vec![c.merge_blocks(&ks[1], &ks[2])]
    });
    assert_eq!(
        s.effects[0].as_ref().err().map(|e| e.code()),
        Some(ErrorCode::CrossBlock),
        "{:?}",
        s.effects
    );
}

#[test]
fn deleting_the_last_paragraph_of_a_content_control_empties_it_and_keeps_its_mark() {
    let s = with_compiler(&control_between(), Track::Tracked(stamp()), |c, _, ks| {
        vec![c.delete_block(&ks[1])]
    });
    s.assert_clean();
    assert_eq!(s.fin, ["before", "", "after"]);
    assert!(
        !s.xml.contains("<w:pPr><w:rPr><w:del"),
        "the mark is not deleted: {}",
        s.xml
    );
}

const ALT_CHUNK: &str = "<w:altChunk xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rIdImport\"/>";

/// Accepts every revision of the transaction's document.
fn accept_all(tx: &mut crate::state::Tx<'_>) {
    super::resolve::resolve(
        tx,
        &mut super::index::ChangeIndex::new(),
        &[super::types::Step {
            action: super::types::Action::Accept,
            selector: super::types::Selector::All,
        }],
    )
    .unwrap();
}

#[test]
fn two_paragraphs_with_an_imported_part_between_are_not_merged() {
    // docx-js `word-imported-styles`: the text is read across a `w:altChunk`, so a deleted mark before one
    // reads as joining the paragraphs, but accepting it cannot join them (R8). The merge is refused.
    let body = format!(
        "<w:p>{}</w:p>{ALT_CHUNK}<w:p>{}</w:p>",
        r("above"),
        r("below")
    );
    let s = merge(&body, 0, 1);
    assert_eq!(s.code(0), Some(ErrorCode::CrossBlock), "{:?}", s.effects);
    assert!(
        s.effects[0]
            .as_ref()
            .unwrap_err()
            .message()
            .contains("imported")
    );
    assert!(!s.xml.contains("w:del"), "{}", s.xml);
}

#[test]
fn a_paragraph_deleted_before_an_imported_part_keeps_its_mark_and_accepting_reads_as_final() {
    let body = format!(
        "<w:p>{}</w:p><w:p>{}</w:p>{ALT_CHUNK}<w:p>{}</w:p>",
        r("first"),
        r("gone"),
        r("after")
    );
    let state = state(&body);
    let mut tx = state.begin();
    let ks = para_keys(&tx);
    {
        let mut c = tracked(&mut tx);
        c.delete_block(&ks[1]).unwrap();
        c.finish().unwrap();
    }
    let xml = body_xml(&tx);
    assert!(!xml.contains("<w:rPr><w:del"), "the mark stays: {xml}");
    let fin = texts(&tx, Mode::Final);
    assert_eq!(fin, ["first", "", "after"]);
    accept_all(&mut tx);
    assert_eq!(texts(&tx, Mode::Final), fin, "{}", body_xml(&tx));
    // With a paragraph right after it, the mark is deleted as before.
    let body = format!(
        "<w:p>{}</w:p>{ALT_CHUNK}<w:p>{}</w:p><w:p>{}</w:p>",
        r("first"),
        r("gone"),
        r("after")
    );
    let s = delete(&body, 1);
    s.assert_clean();
    assert!(s.xml.contains("<w:rPr><w:del"), "{}", s.xml);
}

#[test]
fn untracked_deleting_a_paragraph_with_a_bookmark_leaves_the_next_paragraph_as_it_was() {
    let second = format!(
        "<w:p><w:pPr><w:jc w:val=\"right\"/></w:pPr>{}</w:p>",
        r("kept")
    );
    let body = format!(
        "<w:p><w:pPr/><w:bookmarkStart w:id=\"0\" w:name=\"_GoBack\"/><w:bookmarkEnd w:id=\"0\"/>{}<w:proofErr w:type=\"gramStart\"/></w:p>{second}",
        r("test text")
    );
    let s = with_compiler(&body, Track::Untracked, |c, _, ks| {
        vec![c.delete_block(&ks[0])]
    });
    assert!(s.effects[0].is_ok(), "{:?}", s.effects[0]);
    assert_eq!(
        s.xml,
        format!(
            "<w:bookmarkStart w:id=\"0\" w:name=\"_GoBack\"/><w:bookmarkEnd w:id=\"0\"/>{second}"
        )
    );
}

#[test]
fn a_merge_that_removes_an_own_inserted_mark_moves_nothing_across_the_deleted_paragraphs_between() {
    // A range deleted across a paragraph the batch inserted: the first paragraph's mark is the author's own
    // insertion, so the merge removes it outright, and that joins the first paragraph with the one right after it.
    // Joining it with the last instead moved the last's text before the deleted paragraphs, out of the Original's
    // order (Invalid_Word15Comments, seed 555555).
    let body = format!(
        "{}{}{}{}",
        para("aaaa"),
        para("bbbb"),
        para("cccc"),
        para("dddd")
    );
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        let e = c.insert_block(
            &ks[0],
            Place::After,
            &NewParagraph {
                text: "new".to_owned(),
                ..NewParagraph::default()
            },
        );
        let made = e.as_ref().unwrap().new_blocks[0].clone();
        vec![
            e,
            c.replace_text(&ks[0], 2..4, ""),
            c.delete_block(&made),
            c.delete_block(&ks[1]),
            c.delete_block(&ks[2]),
            c.replace_text(&ks[3], 0..2, ""),
            c.merge_blocks(&ks[0], &ks[3]),
        ]
    });
    s.assert_clean();
    assert_eq!(s.fin.concat(), "aadd", "{}", s.xml);
    let j = super::tests_resolve::run(&s.xml, &[super::tests_resolve::reject_all()]);
    assert_eq!(j.fin.concat(), "aaaabbbbccccdddd", "{}", j.xml);
    assert_eq!(
        j.xml.matches("<w:p>").count() + j.xml.matches("<w:p ").count(),
        4,
        "{}",
        j.xml
    );
}

#[test]
fn a_merge_into_a_paragraph_that_begins_a_field_spanning_paragraphs_is_read_only() {
    // A table of contents: the field begins in the paragraph after "Contents" and ends two paragraphs later, and the
    // view shows those paragraphs as one read-only block, so a merge cannot move "Contents" into it (aspose-words-net
    // Formatted elements, seed 44444).
    let body = format!(
        "{}<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}</w:p><w:p>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>{}",
        para("Contents"),
        r("First heading"),
        r("Second heading"),
        para("After")
    );
    for track in [Track::Tracked(stamp()), Track::Untracked] {
        let s = with_compiler(&body, track, |c, _, ks| {
            vec![c.merge_blocks(&ks[0], &ks[1])]
        });
        assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion), "{}", s.xml);
    }
}
