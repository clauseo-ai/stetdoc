//! Tests of where `insert_block` and `split_block` put a new paragraph and its properties: a copy whose mark held
//! only an insertion, a paragraph in a body-level move, the end of a content control and the edges of a field.

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

#[test]
fn untracked_a_copy_of_properties_whose_mark_held_only_an_insertion_leaves_no_empty_container() {
    // The copy rule drops the inserted mark; the `w:rPr` (and the `w:pPr`) it leaves empty go too, since an empty one is a
    // violation and an untracked edit writes no mark to fill it.
    let body = format!(
        "<w:p>{}{}</w:p><w:p>{}{}</w:p>",
        ppr(&format!("<w:jc w:val=\"center\"/>{}", mark_ins(9))),
        r("Centred"),
        ppr(&mark_ins(8)),
        r("Plain")
    );
    for (k, name) in ["split", "split bare", "insert before", "insert after"]
        .into_iter()
        .enumerate()
    {
        let s = with_compiler(&body, Track::Untracked, |c, _, ks| match k {
            0 => vec![c.split_block(&ks[0], 3)],
            1 => vec![c.split_block(&ks[1], 2)],
            2 => vec![c.insert_block(&ks[0], Place::Before, &new_para("New"))],
            _ => vec![c.insert_block(&ks[1], Place::After, &new_para("New"))],
        });
        assert!(
            s.effects[0].is_ok(),
            "{name}: {:?}",
            s.effects[0].as_ref().err()
        );
        for empty in ["<w:rPr/>", "<w:pPr/>", "<w:rPr></w:rPr>", "<w:pPr></w:pPr>"] {
            assert!(!s.xml.contains(empty), "{name}: {empty} in {}", s.xml);
        }
        assert_eq!(
            s.xml.matches("w:jc").count(),
            match name {
                "split" | "insert before" => 2,
                _ => 1,
            },
            "{name}: {}",
            s.xml
        );
    }
}

#[test]
fn a_paragraph_in_a_body_level_move_gets_its_properties_in_schema_order() {
    // Word's body-level w:moveTo around paragraphs has no place in the schema, so the walk from the root finds no context
    // there; the paragraph's own order still holds, or the w:pPr would go after the runs and be out of order once the move
    // is accepted.
    let body = format!(
        "{}<w:moveTo {}><w:p>{}</w:p></w:moveTo>{}",
        p(&r("Alpha")),
        eric(9),
        r("Charlie"),
        p(&r("Bravo"))
    );
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        vec![
            c.insert_block(&ks[1], Place::After, &new_para("New")),
            c.set_props(
                &super::types::PropTarget::Paragraph {
                    block: ks[1].clone(),
                },
                &set_edit(vec![pel("ind", &[("hanging", "360")], vec![])]),
            ),
        ]
    });
    assert!(s.effects.iter().all(Result::is_ok), "{:?}", s.effects);
    assert!(
        s.xml.contains("<w:p><w:pPr><w:ind w:hanging=\"360\"/>")
            && !s.xml.contains("</w:r><w:pPr>"),
        "{}",
        s.xml
    );
}

#[test]
fn an_insert_after_the_last_paragraph_of_a_content_control_stays_in_that_control() {
    let control = |id: u32, text: &str| {
        format!(
            "<w:sdt><w:sdtPr><w:id w:val=\"{id}\"/></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>",
            p(&r(text))
        )
    };
    let body = format!("{}{}", control(1, "one"), control(2, "two"));
    for track in [Track::Untracked, Track::Tracked(stamp())] {
        let s = with_compiler(&body, track, |c, _, ks| {
            vec![c.insert_block(&ks[0], Place::After, &new_para("New"))]
        });
        assert!(s.effects[0].is_ok(), "{:?}", s.effects[0]);
        assert_eq!(s.fin, ["one", "New", "two"]);
        assert!(
            s.xml
                .contains(&format!("<w:sdtContent>{}</w:sdtContent>", p(&r("two")))),
            "{}",
            s.xml
        );
    }
}

#[test]
fn an_insert_inside_the_span_of_a_field_that_crosses_paragraphs_is_read_only() {
    // A field whose result runs from the first paragraph into the second: a paragraph before the second, or after the
    // first, would be inside the field, as a split there would be.
    let body = format!(
        "{}{}{}",
        p(&format!(
            "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> ADDRESSBLOCK </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}",
            r("line one")
        )),
        p(&format!(
            "{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            r("line two")
        )),
        p(&r("after"))
    );
    for (anchor, place, refused) in [
        (1, Place::Before, true),
        (0, Place::After, true),
        (0, Place::Before, false),
        (1, Place::After, false),
    ] {
        for track in [Track::Untracked, Track::Tracked(stamp())] {
            let s = with_compiler(&body, track, |c, _, ks| {
                vec![c.insert_block(&ks[anchor], place, &new_para("New"))]
            });
            if refused {
                assert_eq!(
                    s.code(0),
                    Some(ErrorCode::ReadonlyRegion),
                    "{anchor} {place:?}"
                );
            } else {
                assert!(
                    s.effects[0].is_ok(),
                    "{anchor} {place:?}: {:?}",
                    s.effects[0]
                );
            }
        }
    }
}

#[test]
fn new_paragraph_properties_go_first_whatever_unknown_element_comes_first() {
    // CT_P puts w:pPr first: an element of an unknown namespace before the runs must not push it later, or a reader
    // that looks for it first misses the deleted mark (fixture xml-ns-prefix-rebound-in-siblings).
    let body = format!(
        "<w:p xmlns:q=\"urn:one\"><q:a q:x=\"1\"/>{}</w:p>{}",
        r("one"),
        p(&r("two"))
    );
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        vec![
            c.merge_blocks(&ks[0], &ks[1]),
            c.set_props(
                &super::types::PropTarget::Paragraph {
                    block: ks[0].clone(),
                },
                &set_edit(vec![pel("spacing", &[("before", "240")], vec![])]),
            ),
        ]
    });
    assert!(s.effects.iter().all(Result::is_ok), "{:?}", s.effects);
    assert!(
        s.xml.starts_with("<w:p xmlns:q=\"urn:one\"><w:pPr>"),
        "{}",
        s.xml
    );
}
