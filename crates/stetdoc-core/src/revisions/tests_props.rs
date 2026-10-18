//! Tests of `set_props`: snapshots, the one change per container, folding, refusing, the direct edits,
//! and the schema order after every edit.

use super::stamp::Track;
use super::tests_support::*;
use super::types::{PropEdit, PropTarget};
use crate::api::error::ErrorCode;

fn run_xml(rpr: &str, text: &str) -> String {
    format!("<w:r>{rpr}<w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn rchange(id: u32, snapshot: &str) -> String {
    let inner = if snapshot.is_empty() {
        "<w:rPr/>".to_owned()
    } else {
        format!("<w:rPr>{snapshot}</w:rPr>")
    };
    format!("<w:rPrChange w:id=\"{id}\" {AT}>{inner}</w:rPrChange>")
}

fn format_runs(body: &str, track: Track, range: std::ops::Range<usize>, edit: &PropEdit) -> Snap {
    with_compiler(body, track, |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Run {
                block: ks[0].clone(),
                range,
            },
            edit,
        )]
    })
}

#[test]
fn formatting_a_run_with_no_properties_makes_its_rpr_first_with_an_empty_snapshot() {
    let s = format_runs(&p(&r("hello")), Track::Tracked(stamp()), 0..5, &bold());
    s.assert_clean();
    assert_eq!(
        s.xml,
        p(&format!(
            "<w:r><w:rPr><w:b/>{}</w:rPr><w:t xml:space=\"preserve\">hello</w:t></w:r>",
            rchange(0, "")
        ))
    );
}

#[test]
fn formatting_part_of_a_run_cuts_it_and_changes_only_the_middle() {
    let s = format_runs(
        &p(&r("Hello big world")),
        Track::Tracked(stamp()),
        6..9,
        &bold(),
    );
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 1, "{}", s.xml);
    assert_eq!(s.xml.matches("<w:r>").count(), 3, "{}", s.xml);
    assert_eq!(s.fin, ["Hello big world"]);
}

#[test]
fn each_run_of_a_range_gets_its_own_change_with_its_own_snapshot() {
    let body = p(&format!(
        "{}{}{}",
        run_xml("<w:rPr><w:i/></w:rPr>", "one"),
        run_xml("", "two"),
        run_xml("<w:rPr><w:u w:val=\"single\"/></w:rPr>", "three")
    ));
    let s = format_runs(&body, Track::Tracked(stamp()), 0..11, &bold());
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 3, "{}", s.xml);
    let ids = created_ids(s.outcome.as_ref().unwrap());
    assert_eq!(ids.len(), 3);
    assert!(
        s.xml.contains("<w:rPr><w:i/></w:rPr></w:rPrChange>"),
        "{}",
        s.xml
    );
    assert!(
        s.xml
            .contains("<w:rPr><w:u w:val=\"single\"/></w:rPr></w:rPrChange>"),
        "{}",
        s.xml
    );
}

#[test]
fn clearing_a_property_keeps_it_in_the_snapshot() {
    let body = p(&run_xml("<w:rPr><w:b/></w:rPr>", "bold"));
    let s = format_runs(&body, Track::Tracked(stamp()), 0..4, &clear_edit(&["b"]));
    s.assert_clean();
    assert_eq!(
        s.xml,
        p(&format!(
            "<w:r><w:rPr>{}</w:rPr><w:t xml:space=\"preserve\">bold</w:t></w:r>",
            rchange(0, "<w:b/>")
        ))
    );
}

#[test]
fn an_edit_that_changes_nothing_writes_nothing_and_takes_no_id() {
    let body = p(&run_xml("<w:rPr><w:b/></w:rPr>", "bold"));
    let s = format_runs(&body, Track::Tracked(stamp()), 0..4, &bold());
    s.assert_clean();
    assert_eq!(s.xml, p(&run_xml("<w:rPr><w:b/></w:rPr>", "bold")));
    assert!(s.outcome.as_ref().unwrap().created.is_empty());
}

#[test]
fn clearing_what_is_not_there_leaves_no_empty_rpr_behind() {
    let s = format_runs(
        &p(&r("plain")),
        Track::Tracked(stamp()),
        0..5,
        &clear_edit(&["b"]),
    );
    s.assert_clean();
    assert_eq!(s.xml, p(&r("plain")));
}

#[test]
fn text_inserted_by_this_transaction_is_formatted_directly() {
    let s = with_compiler(&p(&r("abcd")), Track::Tracked(stamp()), |c, _, ks| {
        vec![
            c.replace_text(&ks[0], 2..2, "XY"),
            c.set_props(
                &PropTarget::Run {
                    block: ks[0].clone(),
                    range: 2..4,
                },
                &bold(),
            ),
        ]
    });
    s.assert_clean();
    assert!(!s.xml.contains("rPrChange"), "{}", s.xml);
    assert!(
        s.xml.contains("<w:rPr><w:b/></w:rPr><w:t>XY</w:t>"),
        "{}",
        s.xml
    );
}

#[test]
fn text_of_the_authors_earlier_insertion_gets_a_change_as_word_writes() {
    let body = p(&format!(
        "<w:ins w:id=\"900\" {AT}>{}</w:ins>",
        r("earlier")
    ));
    let s = format_runs(&body, Track::Tracked(stamp()), 0..7, &bold());
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 1, "{}", s.xml);
}

#[test]
fn text_inside_another_authors_insertion_gets_a_change() {
    let body = p(&format!("<w:ins {}>{}</w:ins>", eric(900), r("theirs")));
    let s = format_runs(&body, Track::Tracked(stamp()), 1..3, &bold());
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 1, "{}", s.xml);
    assert_eq!(
        s.xml.matches("<w:ins ").count(),
        1,
        "their insertion is not split: {}",
        s.xml
    );
}

#[test]
fn another_authors_pending_formatting_change_is_never_touched() {
    let body = p(&run_xml(
        &format!(
            "<w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr>",
            eric(300)
        ),
        "theirs",
    ));
    let s = format_runs(&body, Track::Tracked(stamp()), 0..6, &clear_edit(&["b"]));
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
    assert_eq!(s.xml, body);
}

#[test]
fn a_refused_format_of_part_of_a_run_cuts_nothing() {
    let body = p(&run_xml(
        &format!(
            "<w:rPr><w:b/><w:rPrChange {}><w:rPr/></w:rPrChange></w:rPr>",
            eric(300)
        ),
        "theirs",
    ));
    let s = format_runs(&body, Track::Tracked(stamp()), 2..4, &clear_edit(&["b"]));
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
    assert_eq!(s.xml, body);
}

#[test]
fn the_snapshot_holds_no_change_tracking_element_at_any_depth() {
    let body = format!(
        "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"2\"/><w:ins {}/></w:numPr></w:pPr>{}</w:p>",
        eric(600),
        r("item")
    );
    let s = para_props(&body, Track::Tracked(stamp()), &style("H1"));
    s.assert_clean();
    let at = s.xml.find("<w:pPrChange").unwrap();
    assert!(
        s.xml[at..].contains("<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"2\"/></w:numPr>"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains("<w:ins w:id=\"600\""),
        "the live numPr keeps its own revision: {}",
        s.xml
    );
}

#[test]
fn an_own_earlier_change_takes_the_edit_and_is_reported_as_folded() {
    let body = p(&run_xml(
        &format!("<w:rPr><w:b/><w:rPrChange w:id=\"300\" {AT}><w:rPr/></w:rPrChange></w:rPr>"),
        "mine",
    ));
    let s = format_runs(
        &body,
        Track::Tracked(stamp()),
        0..4,
        &set_edit(vec![pel("i", &[], vec![])]),
    );
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 1);
    assert!(
        s.xml.contains("<w:b/><w:i/>") || s.xml.contains("<w:i/><w:b/>"),
        "{}",
        s.xml
    );
    let f = &s.effect(0).folded;
    assert_eq!(f.len(), 1);
    assert_eq!((f[0].id, f[0].cancelled), (300, false));
    assert!(s.outcome.as_ref().unwrap().created.is_empty());
}

#[test]
fn an_edit_that_brings_the_properties_back_removes_our_earlier_change_and_says_so() {
    let body = p(&run_xml(
        &format!("<w:rPr><w:b/><w:rPrChange w:id=\"300\" {AT}><w:rPr/></w:rPrChange></w:rPr>"),
        "mine",
    ));
    let s = format_runs(&body, Track::Tracked(stamp()), 0..4, &clear_edit(&["b"]));
    s.assert_clean();
    assert!(!s.xml.contains("rPrChange"), "{}", s.xml);
    assert_eq!(
        (s.effect(0).folded[0].id, s.effect(0).folded[0].cancelled),
        (300, true)
    );
}

#[test]
fn a_second_edit_in_one_transaction_goes_into_the_change_it_made() {
    let s = with_compiler(&p(&r("word")), Track::Tracked(stamp()), |c, _, ks| {
        let t = PropTarget::Run {
            block: ks[0].clone(),
            range: 0..4,
        };
        vec![
            c.set_props(&t, &bold()),
            c.set_props(&t, &set_edit(vec![pel("i", &[], vec![])])),
        ]
    });
    s.assert_clean();
    assert_eq!(s.xml.matches("<w:rPrChange ").count(), 1, "{}", s.xml);
    assert!(s.effect(1).folded.is_empty());
    assert_eq!(created_ids(s.outcome.as_ref().unwrap()), [0]);
}

#[test]
fn formatting_and_unformatting_in_one_transaction_leaves_no_change() {
    let s = with_compiler(&p(&r("word")), Track::Tracked(stamp()), |c, _, ks| {
        let t = PropTarget::Run {
            block: ks[0].clone(),
            range: 0..4,
        };
        vec![
            c.set_props(&t, &bold()),
            c.set_props(&t, &clear_edit(&["b"])),
        ]
    });
    s.assert_clean();
    assert!(!s.xml.contains("rPrChange"), "{}", s.xml);
    assert!(s.outcome.as_ref().unwrap().created.is_empty());
}

#[test]
fn untracked_formatting_writes_the_properties_and_nothing_else() {
    let s = format_runs(&p(&r("hello")), Track::Untracked, 0..5, &bold());
    assert!(s.effects[0].is_ok());
    assert_eq!(
        s.xml,
        p("<w:r><w:rPr><w:b/></w:rPr><w:t xml:space=\"preserve\">hello</w:t></w:r>")
    );
    assert_eq!(s.outcome.as_ref().unwrap().minted, 0..0);
}

// --- paragraphs

fn para_props(body: &str, track: Track, edit: &PropEdit) -> Snap {
    with_compiler(body, track, |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Paragraph {
                block: ks[0].clone(),
            },
            edit,
        )]
    })
}

fn style(id: &str) -> PropEdit {
    set_edit(vec![pel("pStyle", &[("val", id)], vec![])])
}

#[test]
fn a_style_change_makes_the_ppr_first_and_ends_it_with_the_change() {
    let s = para_props(&p(&r("Title")), Track::Tracked(stamp()), &style("Heading1"));
    s.assert_clean();
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/><w:pPrChange w:id=\"0\" {AT}><w:pPr/></w:pPrChange></w:pPr>{}</w:p>",
            r("Title")
        )
    );
}

#[test]
fn a_style_change_keeps_the_old_style_and_direct_properties_in_the_snapshot_and_the_schema_order() {
    let body = format!(
        "<w:p><w:pPr><w:pStyle w:val=\"Body\"/><w:jc w:val=\"center\"/></w:pPr>{}</w:p>",
        r("Title")
    );
    let s = para_props(
        &body,
        Track::Tracked(stamp()),
        &set_edit(vec![
            pel("keepNext", &[], vec![]),
            pel("pStyle", &[("val", "H1")], vec![]),
        ]),
    );
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:pStyle w:val=\"H1\"/><w:keepNext/><w:jc w:val=\"center\"/><w:pPrChange"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains(
            "<w:pPr><w:pStyle w:val=\"Body\"/><w:jc w:val=\"center\"/></w:pPr></w:pPrChange>"
        ),
        "{}",
        s.xml
    );
}

#[test]
fn the_snapshot_of_a_paragraph_leaves_out_the_mark_the_section_break_and_other_changes() {
    let body = format!(
        "<w:p><w:pPr><w:jc w:val=\"left\"/><w:rPr><w:b/></w:rPr><w:sectPr/></w:pPr>{}</w:p>",
        r("end")
    );
    let s = para_props(
        &body,
        Track::Tracked(stamp()),
        &set_edit(vec![pel("jc", &[("val", "right")], vec![])]),
    );
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:pPr><w:jc w:val=\"left\"/></w:pPr></w:pPrChange>"),
        "{}",
        s.xml
    );
    assert!(
        s.xml
            .contains("<w:rPr><w:b/></w:rPr><w:sectPr/><w:pPrChange"),
        "{}",
        s.xml
    );
}

#[test]
fn another_authors_pending_paragraph_change_is_never_touched() {
    let body = format!(
        "<w:p><w:pPr><w:jc w:val=\"right\"/><w:pPrChange {}><w:pPr/></w:pPrChange></w:pPr>{}</w:p>",
        eric(400),
        r("text")
    );
    let s = para_props(&body, Track::Tracked(stamp()), &style("H1"));
    assert_eq!(s.code(0), Some(ErrorCode::ReadonlyRegion));
}

#[test]
fn a_paragraph_whose_mark_is_the_authors_earlier_insertion_is_edited_directly_and_reported() {
    // A paragraph follows, so rejecting the insertion joins this one into it and its properties go.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins w:id=\"500\" {AT}/></w:rPr></w:pPr>{}</w:p><w:p>{}</w:p>",
        r("new"),
        r("after")
    );
    let s = para_props(&body, Track::Tracked(stamp()), &style("H1"));
    s.assert_clean();
    assert!(!s.xml.contains("pPrChange"), "{}", s.xml);
    assert!(
        s.xml
            .contains("<w:pStyle w:val=\"H1\"/><w:rPr><w:ins w:id=\"500\""),
        "{}",
        s.xml
    );
    assert_eq!(
        (s.effect(0).folded[0].id, s.effect(0).folded[0].cancelled),
        (500, false)
    );
}

#[test]
fn the_earlier_inserted_paragraph_that_nothing_follows_gets_a_change_so_a_reject_can_bring_its_properties_back()
 {
    // Rejecting the mark of the last paragraph keeps the paragraph, with the properties it has then.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins w:id=\"500\" {AT}/></w:rPr></w:pPr>{}</w:p>",
        r("new")
    );
    let s = para_props(&body, Track::Tracked(stamp()), &style("H1"));
    s.assert_clean();
    assert!(s.xml.contains("<w:pPrChange"), "{}", s.xml);
    assert!(s.effect(0).folded.is_empty());
}

#[test]
fn an_earlier_inserted_paragraph_followed_only_by_the_authors_inserted_paragraphs_gets_a_change() {
    // Rejecting all of the author's insertions removes every paragraph after it too.
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins w:id=\"500\" {AT}/></w:rPr></w:pPr>{}</w:p><w:p><w:pPr><w:rPr><w:ins w:id=\"501\" {AT}/></w:rPr></w:pPr>{}</w:p>",
        r("new"),
        r("newer")
    );
    let s = para_props(&body, Track::Tracked(stamp()), &style("H1"));
    s.assert_clean();
    assert!(s.xml.contains("<w:pPrChange"), "{}", s.xml);
}

#[test]
fn a_paragraph_whose_mark_is_another_authors_insertion_gets_a_change() {
    let body = format!(
        "<w:p><w:pPr><w:rPr><w:ins {}/></w:rPr></w:pPr>{}</w:p>",
        eric(500),
        r("theirs")
    );
    let s = para_props(&body, Track::Tracked(stamp()), &style("H1"));
    s.assert_clean();
    assert!(s.xml.contains("pPrChange"), "{}", s.xml);
}

#[test]
fn the_paragraph_mark_has_its_own_change_inside_its_rpr() {
    let s = with_compiler(&p(&r("text")), Track::Tracked(stamp()), |c, _, ks| {
        vec![c.set_props(
            &PropTarget::ParagraphMark {
                block: ks[0].clone(),
            },
            &bold(),
        )]
    });
    s.assert_clean();
    assert!(
        s.xml.starts_with(&format!(
            "<w:p><w:pPr><w:rPr><w:b/>{}</w:rPr></w:pPr>",
            rchange(0, "")
        )),
        "{}",
        s.xml
    );
}

#[test]
fn section_properties_get_a_sect_pr_change_and_the_body_section_too() {
    let body = format!(
        "<w:p><w:pPr><w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:pPr>{}</w:p>",
        r("end of section")
    );
    let edit = set_edit(vec![pel("pgSz", &[("w", "11906"), ("h", "16838")], vec![])]);
    let s = with_compiler(&body, Track::Tracked(stamp()), |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Section {
                block: ks[0].clone(),
            },
            &edit,
        )]
    });
    s.assert_clean();
    assert!(
        s.xml
            .contains("<w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:sectPrChange"),
        "{}",
        s.xml
    );
    assert!(
        s.xml.contains(
            "<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/></w:sectPr></w:sectPrChange>"
        ),
        "{}",
        s.xml
    );
    let none = with_compiler(&p(&r("no section")), Track::Tracked(stamp()), |c, _, ks| {
        vec![c.set_props(
            &PropTarget::Section {
                block: ks[0].clone(),
            },
            &edit,
        )]
    });
    assert_eq!(none.code(0), Some(ErrorCode::InvalidPayload));
}

#[test]
fn an_element_the_schema_does_not_allow_is_refused_and_nothing_stays() {
    let s = para_props(
        &p(&r("text")),
        Track::Tracked(stamp()),
        &set_edit(vec![pel("t", &[], vec![])]),
    );
    assert_eq!(s.code(0), Some(ErrorCode::InvalidPayload));
}

#[test]
fn untracked_paragraph_properties_are_written_directly() {
    let s = para_props(&p(&r("Title")), Track::Untracked, &style("Heading1"));
    assert!(s.effects[0].is_ok());
    assert_eq!(
        s.xml,
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr>{}</w:p>",
            r("Title")
        )
    );
}

// --- tables

#[test]
fn clearing_the_last_property_of_a_run_takes_its_empty_run_properties_with_it() {
    // A run of the author's own insertion is formatted directly: set, then cleared, it is left with no
    // property, and an empty `w:rPr` is W10, which `finish` refuses. Untracked is the same.
    for track in [Track::Tracked(stamp()), Track::Untracked] {
        let s = with_compiler(&p(&r("abc")), track, |c, _, ks| {
            let t = PropTarget::Run {
                block: ks[0].clone(),
                range: 0..1,
            };
            vec![
                c.replace_text(&ks[0], 0..0, " "),
                c.set_props(&t, &set_edit(vec![pel("strike", &[], vec![])])),
                c.set_props(&t, &clear_edit(&["strike"])),
            ]
        });
        assert!(s.effects.iter().all(Result::is_ok), "{:?}", s.effects);
        assert!(s.outcome.is_ok(), "{:?}\n{}", s.outcome, s.xml);
        assert!(!s.xml.contains("<w:rPr/>"), "{}", s.xml);
    }
    // An empty `w:rPr` that the document had stays.
    let s = with_compiler(
        &p("<w:r><w:rPr/><w:t>abc</w:t></w:r>"),
        Track::Untracked,
        |c, _, ks| {
            let t = PropTarget::Run {
                block: ks[0].clone(),
                range: 0..3,
            };
            vec![c.set_props(&t, &clear_edit(&["strike"]))]
        },
    );
    assert!(s.xml.contains("<w:rPr/>"), "{}", s.xml);
}
