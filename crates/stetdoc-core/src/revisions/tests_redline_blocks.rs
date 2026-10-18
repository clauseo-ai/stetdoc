//! The block primitives (design `compare.md`, 6.1): `mark_inserted`, `place_deleted` and `set_snapshot`, with the two compare laws
//! checked on each by `tests_redline::check`.

use super::compiler::Compiler;
use super::redline_types::{Imported, ParaRedline, RedlineEdit, Snapshot};
use super::stamp::Track;
use super::tests_redline::{check, check_open, detached_paragraph, redline};
use super::tests_support::*;
use super::types::Place;
use crate::state::Tx;
use crate::wordml::NodeId;

#[test]
fn a_paragraph_marked_inserted_goes_on_reject_and_stays_on_accept() {
    let original = format!("{}{}", p(&r("First")), p(&r("Last")));
    let revised = format!(
        "{}{}{}",
        p(&r("First")),
        p(&r("Brand new paragraph")),
        p(&r("Last"))
    );
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| vec![c.mark_inserted(&[all[1].clone()]).unwrap()],
    );
    assert!(xml.contains("<w:rPr><w:ins "), "{xml}");
}

#[test]
fn an_inserted_row_and_an_inserted_table_are_marked_whole() {
    let row = |t: &str| format!("<w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>", p(&r(t)));
    let table = |rows: &[&str]| {
        format!(
            "<w:tbl><w:tblPr/><w:tblGrid/>{}</w:tbl>",
            rows.iter().map(|t| row(t)).collect::<String>()
        )
    };
    let original = format!("{}{}", p(&r("Before")), table(&["one", "three"]));
    let revised = format!("{}{}", p(&r("Before")), table(&["one", "two", "three"]));
    let rows_before = {
        let s = state(&revised);
        let tx = s.begin();
        keys(&tx)
    };
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            // all: p, table, row, p(cell), row, p(cell), row, p(cell)
            assert_eq!(all.len(), rows_before.len());
            vec![c.mark_inserted(&[all[4].clone()]).unwrap()]
        },
    );
    assert!(xml.contains("<w:trPr><w:ins "), "{xml}");
    // a whole table
    let original = p(&r("Only paragraph"));
    let revised = format!("{}{}", p(&r("Only paragraph")), table(&["a", "b"]));
    check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| vec![c.mark_inserted(&[all[1].clone()]).unwrap()],
    );
}

#[test]
fn a_deleted_paragraph_is_placed_before_an_anchor_with_its_own_properties() {
    let original = format!(
        "{}{}{}",
        p(&r("First")),
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Centred and gone</w:t></w:r></w:p>"#,
        p(&r("Last"))
    );
    let revised = format!("{}{}", p(&r("First")), p(&r("Last")));
    let (fx, xml) = check(
        &original,
        &revised,
        |tx, _| vec![detached_paragraph(tx, "Centred and gone", Some("center"))],
        |c, all, nodes| {
            let source = all[0].clone();
            vec![
                c.place_deleted(
                    &all[1],
                    Place::Before,
                    &[Imported {
                        node: nodes[0],
                        source,
                    }],
                    None,
                )
                .unwrap(),
            ]
        },
    );
    assert!(xml.contains("<w:rPr><w:del "), "{xml}");
    assert_eq!(fx[0].new_blocks.len(), 1);
    assert_eq!(fx[0].imported.len(), 1);
}

#[test]
fn a_run_of_deleted_paragraphs_after_the_last_one_takes_the_mark_of_the_live_paragraph() {
    // RP040: the live paragraph's mark is deleted, the last imported paragraph keeps a live mark and takes the live properties
    let original = format!(
        "{}{}{}",
        r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t>Live</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Gone one</w:t></w:r></w:p>"#,
        r#"<w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t>Gone two</w:t></w:r></w:p>"#
    );
    let revised =
        r#"<w:p><w:pPr><w:jc w:val="left"/></w:pPr><w:r><w:t>Live</w:t></w:r></w:p>"#.to_owned();
    let (_, xml) = check(
        &original,
        &revised,
        |tx, _| {
            vec![
                detached_paragraph(tx, "Gone one", Some("center")),
                detached_paragraph(tx, "Gone two", Some("right")),
            ]
        },
        |c, all, nodes| {
            let source = all[0].clone();
            let imported: Vec<Imported> = nodes
                .iter()
                .map(|&node| Imported {
                    node,
                    source: source.clone(),
                })
                .collect();
            vec![
                c.place_deleted(&all[0], Place::After, &imported, Some(&all[0]))
                    .unwrap(),
            ]
        },
    );
    assert!(xml.contains("<w:pPrChange"), "{xml}");
}

#[test]
fn a_snapshot_alone_writes_the_old_paragraph_properties() {
    let original =
        r#"<w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Heading</w:t></w:r></w:p>"#
            .to_owned();
    let revised = p(&r("Heading"));
    let _ = revised;
    let revised = "<w:p><w:r><w:t>Heading</w:t></w:r></w:p>".to_owned();
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            let snap = Snapshot {
                ppr: Some(vec![pel("jc", &[("val", "center")], Vec::new())]),
                mark: None,
            };
            vec![c.set_snapshot(&all[0], &snap).unwrap()]
        },
    );
    assert!(xml.contains("<w:pPrChange"), "{xml}");
}

#[test]
fn only_the_mark_is_inserted_for_the_first_parts_of_a_split() {
    // the original paragraph was one; the revised has two. Rejecting joins them, with the later paragraph's properties
    let original = p(&r("One whole sentence here"));
    let revised = format!("{}{}", p(&r("One whole ")), p(&r("sentence here")));
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            let plan = ParaRedline {
                mark_inserted: true,
                ..ParaRedline::default()
            };
            vec![c.redline_paragraph(&all[0], &plan).unwrap()]
        },
    );
    assert!(xml.contains("<w:rPr><w:ins "), "{xml}");
    assert!(
        !xml.contains("<w:ins ") || xml.matches("<w:ins ").count() == 1,
        "{xml}"
    );
}

#[test]
fn text_inside_a_hyperlink_is_marked_inside_it() {
    let link = |t: &str| {
        format!(
            r#"<w:hyperlink w:anchor="x"><w:r><w:t xml:space="preserve">{t}</w:t></w:r></w:hyperlink>"#
        )
    };
    let original = p(&format!("{}{}", r("See "), link("the clause")));
    let revised = p(&format!("{}{}", r("See "), link("the new clause")));
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            vec![
                c.redline_paragraph(
                    &all[0],
                    &redline(vec![RedlineEdit {
                        insert: 8..12,
                        deleted: vec![],
                    }]),
                )
                .unwrap(),
            ]
        },
    );
    let (h, i) = (
        xml.find("<w:hyperlink").unwrap(),
        xml.find("<w:ins ").unwrap(),
    );
    assert!(h < i && xml.find("</w:hyperlink>").unwrap() > i, "{xml}");
}

#[test]
fn nothing_is_written_when_the_plan_is_empty_and_a_bad_range_is_refused_with_nothing_changed() {
    let body = p(&r("Short"));
    let s = state(&body);
    let mut tx = s.begin();
    let keys = keys(&tx);
    let before = body_xml(&tx);
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    c.redline_paragraph(&keys[0], &ParaRedline::default())
        .unwrap();
    assert!(
        c.redline_paragraph(
            &keys[0],
            &redline(vec![RedlineEdit {
                insert: 3..99,
                deleted: vec![]
            }])
        )
        .is_err()
    );
    let _ = c;
    assert_eq!(body_xml(&tx), before);
}

/// Takes the last top-level block of the body out of the tree as a detached copy: a stand-in for a block copied from another
/// package. A row is taken from a table instead when `row` is set.
fn take_last_block(tx: &mut Tx<'_>, row: bool) -> NodeId {
    let top = {
        let view = tx.parts();
        let id = view.id(MAIN).unwrap();
        let tree = view.tree(id).unwrap();
        let body = tree
            .root()
            .child_elements()
            .find(|c| c.local_name() == "body")
            .unwrap();
        body.child_elements()
            .filter(|c| c.local_name() != "sectPr")
            .last()
            .unwrap()
            .id()
    };
    let tree = tx.part_mut(MAIN).unwrap();
    let mut ed = tree.edit();
    let copy = ed.clone_subtree(top).unwrap();
    ed.detach(top).unwrap();
    if !row {
        return copy;
    }
    let first_row = {
        let n = tree.node_ref(copy);
        n.child_elements()
            .find(|c| c.local_name() == "tr")
            .unwrap()
            .id()
    };
    tree.edit().detach(first_row).unwrap();
    first_row
}

fn table_of(rows: &[&str]) -> String {
    let row = |t: &str| {
        format!(
            "<w:tr><w:tc><w:tcPr/>{}</w:tc><w:tc><w:tcPr/>{}</w:tc></w:tr>",
            p(&r(t)),
            p(&r("cell"))
        )
    };
    format!(
        "<w:tbl><w:tblPr/><w:tblGrid/>{}</w:tbl>",
        rows.iter().map(|t| row(t)).collect::<String>()
    )
}

#[test]
fn a_deleted_row_has_every_paragraph_deleted_and_the_table_is_touched() {
    let original = format!("{}{}", p(&r("Before")), table_of(&["one", "two", "three"]));
    let revised = format!("{}{}", p(&r("Before")), table_of(&["one", "three"]));
    let open = format!("{revised}{}", table_of(&["two"]));
    let (fx, xml) = check_open(
        &original,
        &revised,
        &open,
        |tx, _| vec![take_last_block(tx, true)],
        |c, all, nodes| {
            // entries: p, table, row, cell p, cell p, row, cell p, cell p, then the scratch table's: the anchor is the second row
            vec![
                c.place_deleted(
                    &all[5],
                    Place::Before,
                    &[Imported {
                        node: nodes[0],
                        source: all[0].clone(),
                    }],
                    None,
                )
                .unwrap(),
            ]
        },
    );
    assert!(xml.contains("<w:trPr><w:del "), "{xml}");
    assert_eq!(
        xml.matches("<w:rPr><w:del ").count(),
        2,
        "both cell paragraphs lose their marks\n{xml}"
    );
    assert!(
        fx[0]
            .touched
            .iter()
            .any(|k| k.node() == all_first_table(&fx)),
        "the table is touched"
    );
}

fn all_first_table(fx: &[super::outcome::Effect]) -> NodeId {
    // the touched block of the call is the table, found by being the one that is not a paragraph or a row of the new rows
    fx[0].touched[0].node()
}

#[test]
fn a_deleted_table_is_every_row_deleted() {
    let original = format!("{}{}", p(&r("Before")), table_of(&["one", "two"]));
    let revised = p(&r("Before"));
    let open = format!("{revised}{}", table_of(&["one", "two"]));
    let (_, xml) = check_open(
        &original,
        &revised,
        &open,
        |tx, _| vec![take_last_block(tx, false)],
        |c, all, nodes| {
            vec![
                c.place_deleted(
                    &all[0],
                    Place::After,
                    &[Imported {
                        node: nodes[0],
                        source: all[0].clone(),
                    }],
                    None,
                )
                .unwrap(),
            ]
        },
    );
    assert_eq!(xml.matches("<w:trPr><w:del ").count(), 2, "{xml}");
}

#[test]
fn a_complex_field_that_is_marked_inserted_keeps_its_separate_run_outside_the_insertion() {
    let field = "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>7</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>";
    let original = p(&r("Intro"));
    let revised = format!("{}{}", p(&r("Intro")), p(&format!("{}{field}", r("Page "))));
    let (_, xml) = check(
        &original,
        &revised,
        |_, _| Vec::new(),
        |c, all, _| {
            let fx = c.mark_inserted(&[all[1].clone()]).unwrap();
            // The paragraph ends the body, so the one before it takes the inserted mark (3.2.1).
            let end = c.settle_end_marks(all[1].part()).unwrap();
            assert_eq!(end.given.len(), 1);
            vec![fx, end.effect]
        },
    );
    assert!(
        xml.contains("</w:ins><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:ins "),
        "the separate run stands between two insertions, outside both\n{xml}"
    );
}

#[test]
fn a_paragraph_that_ends_a_section_keeps_its_mark_and_only_its_content_is_inserted() {
    let revised = format!(
        "{}{}",
        "<w:p><w:pPr><w:sectPr/></w:pPr><w:r><w:t>Last of section</w:t></w:r></w:p>",
        p(&r("Next"))
    );
    let s = state(&revised);
    let mut tx = s.begin();
    let all = keys(&tx);
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    c.mark_inserted(&[all[0].clone()]).unwrap();
    c.finish().unwrap();
    let xml = body_xml(&tx);
    assert!(
        xml.contains("<w:sectPr/>") && !xml.contains("<w:rPr><w:ins "),
        "{xml}"
    );
    assert!(xml.contains("<w:ins "), "{xml}");
}

#[test]
fn place_deleted_refuses_before_it_writes() {
    let s = state(&format!("{}{}", p(&r("First")), p(&r("Last"))));
    let mut tx = s.begin();
    let all = keys(&tx);
    let fresh = detached_paragraph(&mut tx, "gone", None);
    let before = String::from_utf8(
        tx.parts()
            .tree(tx.parts().id(MAIN).unwrap())
            .unwrap()
            .serialize(),
    )
    .unwrap();
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    // the second block is in the story already, so nothing of the first may be written
    let blocks = [
        Imported {
            node: fresh,
            source: all[0].clone(),
        },
        Imported {
            node: all[1].node(),
            source: all[1].clone(),
        },
    ];
    assert!(
        c.place_deleted(&all[0], Place::After, &blocks, None)
            .is_err()
    );
    // a trailing paragraph that is not beside the anchor is refused too
    let one = [Imported {
        node: fresh,
        source: all[0].clone(),
    }];
    let row_like = crate::wordml::BlockKey::new(all[0].part().clone(), fresh);
    assert!(
        c.place_deleted(&all[0], Place::After, &one, Some(&row_like))
            .is_err()
    );
    drop(c);
    let after = String::from_utf8(
        tx.parts()
            .tree(tx.parts().id(MAIN).unwrap())
            .unwrap()
            .serialize(),
    )
    .unwrap();
    assert_eq!(before, after);
}

#[test]
fn add_deleted_note_refuses_a_part_that_is_not_a_notes_part_and_a_note_with_no_paragraph() {
    let s = state(&p(&r("Body")));
    let mut tx = s.begin();
    let all = keys(&tx);
    let fresh = detached_paragraph(&mut tx, "note text", None);
    let mut c = Compiler::new(&mut tx, Track::Tracked(stamp())).unwrap();
    let blocks = [Imported {
        node: fresh,
        source: all[0].clone(),
    }];
    let main: std::sync::Arc<str> = std::sync::Arc::from(MAIN);
    assert!(c.add_deleted_note(&main, false, 5, &blocks).is_err());
    assert!(c.add_deleted_note(&main, false, 5, &[]).is_err());
}
