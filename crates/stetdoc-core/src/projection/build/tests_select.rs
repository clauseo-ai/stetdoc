//! Selections, the stories other than the body, and the blocks that are not shown.

use super::stories::{Pick, TopPick};
use super::testing::*;
use super::{Selection, StorySel, project};
use crate::api::view::{Mode, WarningCode};
use crate::projection::model::*;
use crate::wordml::StoryKind;

/// The model of the body with only these tops, and these rows of a table at either end.
fn picked(f: &Fixture, pick: Pick) -> Projection {
    let state = f.state();
    let parts = state.parts();
    let body = parts.id("word/document.xml").unwrap();
    let selection = Selection {
        stories: vec![(body, StorySel::Pick(pick))],
    };
    project(
        parts,
        state.base_token(),
        Mode::Redline,
        Output::Files,
        &selection,
    )
    .unwrap()
}

fn texts(m: &Projection) -> Vec<String> {
    body(m)
        .iter()
        .map(|b| match b {
            Block::Paragraph(p) => redline_text(&p.inlines),
            Block::Table(t) => format!("table of {} rows", t.rows.len()),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn a_pick_takes_a_run_of_tops_and_nothing_else() {
    let f = Fixture::new(&["a", "b", "c", "d", "e"].map(p).concat());
    let m = picked(&f, Pick::run(1..4));
    assert_eq!(texts(&m), vec!["b", "c", "d"]);
    let m = picked(&f, Pick::run(4..9));
    assert_eq!(texts(&m), vec!["e"]);
    let m = picked(&f, Pick::run(9..12));
    assert!(stories(&m).is_empty(), "a pick past the end holds no story");
}

#[test]
fn a_pick_can_cut_a_table_between_rows_at_either_end() {
    let row = |t: &str| format!("<w:tr><w:tc><w:tcPr/>{}</w:tc></w:tr>", p(t));
    let table = format!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1\"/></w:tblGrid>{}</w:tbl>",
        ["r0", "r1", "r2", "r3"].map(row).concat()
    );
    let f = Fixture::new(&format!("{}{}{}", p("before"), table, p("after")));
    let rows_of = |m: &Projection| -> Vec<String> {
        body(m)
            .iter()
            .flat_map(|b| match b {
                Block::Table(t) => t
                    .rows
                    .iter()
                    .map(|r| match &r.cells[0].blocks[0] {
                        Block::Paragraph(p) => redline_text(&p.inlines),
                        _ => String::new(),
                    })
                    .collect(),
                Block::Paragraph(p) => vec![redline_text(&p.inlines)],
                _ => Vec::new(),
            })
            .collect()
    };
    let cut = |top: usize, rows: std::ops::Range<usize>| TopPick {
        rows: Some(rows),
        ..TopPick::whole(top)
    };
    let first = picked(
        &f,
        Pick {
            tops: vec![TopPick::whole(0), cut(1, 0..2)],
            expanded: false,
        },
    );
    assert_eq!(rows_of(&first), vec!["before", "r0", "r1"]);
    let second = picked(
        &f,
        Pick {
            tops: vec![cut(1, 2..usize::MAX), TopPick::whole(2)],
            expanded: false,
        },
    );
    assert_eq!(rows_of(&second), vec!["r2", "r3", "after"]);
    let middle = picked(
        &f,
        Pick {
            tops: vec![cut(1, 1..3)],
            expanded: false,
        },
    );
    assert_eq!(rows_of(&middle), vec!["r1", "r2"]);
}

#[test]
fn endnotes_and_footers_are_stories_of_their_own() {
    let note = "<w:endnote w:id=\"2\"><w:p><w:r><w:endnoteRef/></w:r><w:r><w:t> an endnote</w:t></w:r></w:p></w:endnote>";
    let body = px(&format!(
        "{}<w:r><w:endnoteReference w:id=\"2\"/></w:r>",
        r("text")
    ));
    let f = Fixture::new(&body)
        .endnotes(note)
        .footer(1, &p("page footer"))
        .sect("<w:footerReference w:type=\"default\" r:id=\"rIdF1\"/>");
    let m = redline(&f);
    assert!(matches!(
        &para(&m, 0).inlines[1],
        Inline::Atom(Atom::NoteRef {
            kind: NoteKind::Endnote,
            ordinal: 1
        })
    ));
    let notes = stories(&m)
        .iter()
        .find(|s| s.kind == StoryKind::Endnotes)
        .unwrap();
    assert_eq!(notes.units[0].kind, UnitKind::Endnote);
    assert_eq!((notes.units[0].ordinal, notes.units[0].id), (1, Some(2)));
    let footer = stories(&m)
        .iter()
        .find(|s| s.kind == StoryKind::Footer)
        .unwrap();
    assert_eq!(
        footer.uses,
        vec![SectionUse {
            section: 1,
            slot: Slot::Default
        }]
    );
    assert_eq!(footer.units[0].kind, UnitKind::Footer);
    assert_eq!(m.head.summary.endnotes, 1);
}

#[test]
fn a_second_body_an_imported_part_and_an_unknown_element_are_warned_about() {
    let body = format!(
        "{}</w:body><w:body>{}",
        p("seen"),
        p("hidden in a second body")
    );
    let m = redline(&Fixture::new(&body));
    assert_eq!(texts(&m), vec!["seen"]);
    let w = m
        .head
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::ReadAnomaly)
        .expect("a warning for the second body");
    assert!(w.message.contains("second body"), "{}", w.message);
    assert_eq!(w.story.as_deref(), Some("body"));
    let body = format!("{}<w:altChunk r:id=\"rId77\"/>{}", p("one"), p("two"));
    let m = redline(&Fixture::new(&body));
    let kinds: Vec<&str> = body_kinds(&m);
    assert_eq!(kinds, vec!["paragraph", "opaque", "paragraph"]);
    let w = m
        .head
        .warnings
        .iter()
        .find(|w| w.code == WarningCode::OpaqueContent)
        .unwrap();
    assert!(w.message.contains("after p1"), "{}", w.message);
}

fn body_kinds(m: &Projection) -> Vec<&'static str> {
    body(m)
        .iter()
        .map(|b| match b {
            Block::Paragraph(_) => "paragraph",
            Block::Table(_) => "table",
            Block::Section(_) => "section",
            Block::Opaque(_) => "opaque",
            Block::Margin(_) => "margin",
        })
        .collect()
}

#[test]
fn a_text_box_is_read_in_the_branch_word_reads_and_shown_at_its_drawing() {
    let content = |t: &str| {
        format!(
            "<w:txbxContent>{}{}</w:txbxContent>",
            p(t),
            p("second line")
        )
    };
    let drawing = format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline><wp:docPr id=\"3\" name=\"Box\"/>\
         <a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:txbx>{}</wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></mc:Choice>\
         <mc:Fallback><w:pict><v:shape><v:textbox>{}</v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r>",
        content("first line"),
        content("first line")
    );
    let m = redline(&Fixture::new(&px(&format!("{}{}", r("see "), drawing))));
    let Inline::Atom(Atom::Object(info)) = &para(&m, 0).inlines[1] else {
        panic!("{:?}", para(&m, 0).inlines)
    };
    assert_eq!(info.kind, ObjectKind::TextBox);
    assert_eq!(info.text_box.as_deref(), Some("first line / second line"));
}

#[test]
fn the_summary_counts_what_the_document_holds() {
    let body = px(&format!(
        "{}{}",
        del(1, "Ann Lee", "x"),
        ins(2, "Bo Chan", "y")
    ));
    let f = Fixture::new(&body)
        .footnotes("<w:footnote w:id=\"1\"><w:p><w:r><w:t>n</w:t></w:r></w:p></w:footnote>")
        .comments("<w:comment w:id=\"0\" w:author=\"Cy Dee\"><w:p><w:r><w:t>c</w:t></w:r></w:p></w:comment>");
    let m = redline(&f);
    let s = &m.head.summary;
    assert_eq!(
        (s.footnotes, s.endnotes, s.comments, s.sections),
        (1, 0, 1, 1)
    );
    assert_eq!(s.changes, vec![1, 1, 0]);
    let codes: Vec<&str> = m.head.authors.iter().map(|a| &*a.code).collect();
    assert_eq!(codes, vec!["AL", "BC", "CD"]);
}

#[test]
fn two_authors_with_the_same_initials_get_codes_that_tell_them_apart() {
    let body = px(&format!(
        "{}{}",
        del(1, "Jane Doe", "x"),
        ins(2, "John Davis", "y")
    ));
    let m = redline(&Fixture::new(&body));
    let codes: Vec<&str> = m.head.authors.iter().map(|a| &*a.code).collect();
    assert_eq!(codes, vec!["JD", "JD2"]);
}

#[test]
fn margin_notes_ride_with_the_tops_they_are_about_and_are_not_in_a_file() {
    let f = Fixture::new(&["a", "b", "c"].map(p).concat());
    let m = picked(
        &f,
        Pick {
            tops: vec![
                TopPick {
                    after: vec![Margin::Matches(2), Margin::Gap(1)],
                    ..TopPick::whole(0)
                },
                TopPick::whole(2),
            ],
            expanded: false,
        },
    );
    let kinds: Vec<&'static str> = body(&m)
        .iter()
        .map(|b| match b {
            Block::Paragraph(_) => "p",
            Block::Margin(Margin::Matches(_)) => "matches",
            Block::Margin(Margin::Gap(_)) => "gap",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["p", "matches", "gap", "p"]);
}
