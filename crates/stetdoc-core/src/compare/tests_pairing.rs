//! Tests of the model, the table pairing, the moves and groups, and the pairing of stories, on documents built
//! with the fixture builder.

use super::model::{Atoms, Body, story_model};
use super::stories::{Pairings, pair_documents};
use crate::api::limits::Limits;
use crate::state::State;
use stetdoc_testkit::fixtures::{
    parts::{self, Doc},
    zip::{Entry, Zip},
};

const EXTRA: &str = " xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"";
const TYPES: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

fn rel(id: &str, kind: &str, target: &str) -> String {
    format!(
        "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{target}\"/>"
    )
}

/// A state with a body and any extra parts: (name, content type suffix, relationship id, kind, xml).
fn package(body: &str, extra: &[(&str, &str, &str, &str, String)]) -> State {
    let doc = Doc::new().body(body).root_attrs(EXTRA).bytes();
    let mut types = parts::content_types();
    let mut rels = String::new();
    let mut entries = Vec::new();
    for (name, ct, id, kind, xml) in extra {
        types = types.replace(
            "</Types>",
            &format!(
                "<Override PartName=\"/word/{name}\" ContentType=\"{TYPES}.{ct}+xml\"/></Types>"
            ),
        );
        rels.push_str(&rel(id, kind, name));
        entries.push((name.to_string(), xml.clone()));
    }
    let mut all = parts::entries_with(&doc, &parts::root_rels(), &types);
    if !extra.is_empty() {
        all.push(Entry::new(
            "word/_rels/document.xml.rels",
            format!(
                "{}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>",
                parts::DECL
            ),
        ));
    }
    for (name, xml) in entries {
        all.push(Entry::new(format!("word/{name}"), xml));
    }
    State::open(Zip::new(all).build(), &Limits::default())
        .unwrap_or_else(|e| panic!("does not open: {e}"))
}

pub(super) fn state(body: &str) -> State {
    package(body, &[])
}

pub(super) fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

pub(super) fn table(rows: &[&[&str]]) -> String {
    let mut s = String::from("<w:tbl><w:tblPr/><w:tblGrid/>");
    for row in rows {
        s.push_str("<w:tr>");
        for cell in *row {
            s.push_str(&format!("<w:tc><w:tcPr/>{}</w:tc>", p(cell)));
        }
        s.push_str("</w:tr>");
    }
    s.push_str("</w:tbl>");
    s
}

fn pair(a: &str, b: &str) -> (Pairings, State, State) {
    let (sa, sb) = (state(a), state(b));
    let pairings = pair_documents(&sa, &sb, &mut Atoms::default()).unwrap();
    (pairings, sa, sb)
}

pub(super) fn sentence(n: usize) -> String {
    format!(
        "Clause number {n} says that the supplier shall deliver the goods within thirty days of the order and keep records"
    )
}

#[test]
fn the_model_reads_text_tables_and_cells_as_containers() {
    let body = format!(
        "{}{}{}",
        p("Alpha beta gamma"),
        table(&[&["a1 one", "a2 two"], &["b1 three", "b2 four"]]),
        p("Omega")
    );
    let s = state(&body);
    let v = s.parts();
    let id = v.id("word/document.xml").unwrap();
    let story = v.story(id).unwrap();
    let m = story_model(&v, id, &story, &mut Atoms::default()).unwrap();
    assert_eq!(m.roots.len(), 1);
    let root = &m.containers[m.roots[0].container];
    assert_eq!(root.blocks.len(), 3);
    let Body::Table(rows) = &root.blocks[1].body else {
        panic!("not a table")
    };
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].cells.len(), 2);
    assert_eq!(
        rows[0].block.norm,
        format!("a1 one{}a2 two", super::text::CELL_SEP)
    );
    // 1 body root, 4 cell containers
    assert_eq!(m.containers.len(), 5);
}

fn drawing(descr: &str, id: u32) -> String {
    format!(
        "<w:p><w:r><w:drawing><wp:inline><wp:extent cx=\"10\" cy=\"10\"/><wp:docPr id=\"{id}\" name=\"Picture {id}\" descr=\"{descr}\"/></wp:inline></w:drawing></w:r></w:p>"
    )
}

#[test]
fn an_atom_is_a_character_of_its_identity() {
    let mut atoms = Atoms::default();
    let texts: Vec<String> = [drawing("logo", 1), drawing("logo", 7), drawing("chart", 1)]
        .iter()
        .map(|b| {
            let s = state(b);
            let v = s.parts();
            let id = v.id("word/document.xml").unwrap();
            let story = v.story(id).unwrap();
            let m = story_model(&v, id, &story, &mut atoms).unwrap();
            match &m.containers[0].blocks[0].body {
                Body::Para(p) => p.text.clone(),
                Body::Table(_) => String::new(),
            }
        })
        .collect();
    assert_eq!(
        texts[0], texts[1],
        "the same picture with another id is the same atom"
    );
    assert_ne!(texts[0], texts[2], "another picture is another atom");
    assert_eq!(texts[0].chars().count(), 1);
}

#[test]
fn an_inserted_row_leaves_the_others_paired_cell_by_cell() {
    let a = table(&[&["first row text", "x"], &["third row text", "y"]]);
    let b = table(&[
        &["first row text", "x"],
        &["inserted row text", "z"],
        &["third row text", "y"],
    ]);
    let (pairings, _, _) = pair(&a, &b);
    let root = &pairings.stories[0].roots[0].pairing;
    let table_pair = &root.containers[0].pairs[0];
    let rows = table_pair.rows.as_ref().unwrap();
    assert_eq!(
        rows.pairs.iter().map(|r| (r.a, r.b)).collect::<Vec<_>>(),
        vec![(0, 0), (1, 2)]
    );
    assert_eq!(rows.pairs[0].cells.len(), 2);
    assert_eq!(pairings.table_replaced, 0);
}

#[test]
fn a_row_whose_cells_changed_shape_is_not_paired_and_is_counted() {
    let a = table(&[&["same row", "one"], &["wide row text here"]]);
    let b = table(&[&["same row", "one"], &["wide row text", "here"]]);
    let (pairings, _, _) = pair(&a, &b);
    let rows = pairings.stories[0].roots[0].pairing.containers[0].pairs[0]
        .rows
        .as_ref()
        .unwrap();
    assert_eq!(rows.pairs.len(), 1);
    assert_eq!(pairings.table_replaced, 1);
}

#[test]
fn a_table_pairs_with_an_edited_table_but_not_with_a_paragraph() {
    let a = format!(
        "{}{}",
        p("Intro paragraph of the agreement stays"),
        table(&[&["alpha beta gamma delta"]])
    );
    let b = format!(
        "{}{}",
        p("Intro paragraph of the agreement stays"),
        table(&[&["alpha beta gamma delta epsilon"]])
    );
    let (pairings, _, _) = pair(&a, &b);
    assert_eq!(
        pairings.stories[0].roots[0].pairing.containers[0]
            .pairs
            .len(),
        2
    );
    let c = format!(
        "{}{}",
        p("Intro paragraph of the agreement stays"),
        p("alpha beta gamma delta")
    );
    let (pairings, _, _) = pair(&a, &c);
    assert_eq!(
        pairings.stories[0].roots[0].pairing.containers[0]
            .pairs
            .len(),
        1
    );
}

fn two_cell_table(first: &str, second_paragraphs: &[&str]) -> String {
    let second: String = second_paragraphs.iter().map(|t| p(t)).collect();
    format!(
        "<w:tbl><w:tblPr/><w:tblGrid/><w:tr><w:tc><w:tcPr/>{}</w:tc><w:tc><w:tcPr/>{second}</w:tc></w:tr></w:tbl>",
        p(first)
    )
}

#[test]
fn a_paragraph_moved_into_a_cell_is_a_move_and_one_in_a_deleted_table_is_not() {
    let moved = sentence(9);
    let a = format!(
        "{}{}{}{}",
        p(&sentence(1)),
        p(&moved),
        p(&sentence(2)),
        two_cell_table("cell one text here", &["tail words of the cell"])
    );
    let b = format!(
        "{}{}{}",
        p(&sentence(1)),
        p(&sentence(2)),
        two_cell_table("cell one text here", &["tail words of the cell", &moved])
    );
    let (pairings, _, _) = pair(&a, &b);
    let root = &pairings.stories[0].roots[0];
    assert_eq!(root.regrouped.moves.len(), 1, "{:?}", root.regrouped);
    // the same paragraph inside a table that is deleted whole stays out of the pools
    let a2 = format!(
        "{}{}",
        p(&sentence(1)),
        table(&[&[&moved, "zzzz yyyy xxxx wwww vvvv uuuu"]])
    );
    let b2 = format!("{}{}", p(&sentence(1)), p(&moved));
    let (pairings, _, _) = pair(&a2, &b2);
    assert!(pairings.stories[0].roots[0].regrouped.moves.is_empty());
}

#[test]
fn a_split_and_a_merge_are_found_and_an_unrelated_neighbour_is_not() {
    let whole = "The supplier shall deliver the goods within thirty days of the order and shall keep complete records of every delivery for six years";
    let (first, second) = whole.split_at(whole.find(" and shall").unwrap());
    let a = format!("{}{}{}", p(&sentence(1)), p(whole), p(&sentence(2)));
    let b = format!(
        "{}{}{}{}",
        p(&sentence(1)),
        p(first),
        p(second.trim()),
        p(&sentence(2))
    );
    let (pairings, _, _) = pair(&a, &b);
    let g = &pairings.stories[0].roots[0].regrouped.groups;
    assert_eq!(g.len(), 1, "{g:?}");
    assert!(g[0].is_split());
    assert_eq!(g[0].b.len(), 2);
    // and the merge
    let (pairings, _, _) = pair(&b, &a);
    let g = &pairings.stories[0].roots[0].regrouped.groups;
    assert_eq!(g.len(), 1);
    assert!(!g[0].is_split() && g[0].a.len() == 2 && g[0].b.len() == 1);
    // a new, unrelated paragraph after an equal one is not a split
    let c = format!(
        "{}{}{}{}",
        p(&sentence(1)),
        p(whole),
        p("Notices shall be given in writing to the registered office"),
        p(&sentence(2))
    );
    let (pairings, _, _) = pair(&a, &c);
    assert!(
        pairings.stories[0].roots[0].regrouped.groups.is_empty(),
        "{:?} {:?}",
        pairings.stories[0].roots[0].regrouped,
        pairings.stories[0].roots[0].pairing
    );
}

#[test]
fn a_field_that_spans_paragraphs_is_one_group() {
    let begin = "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:t>Heading one 1</w:t></w:r></w:p>";
    let middle = p("Heading two 2");
    let end = "<w:p><w:r><w:t>Heading three 3</w:t></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:p>";
    let body = format!("{}{begin}{middle}{end}{}", p("Before"), p("After"));
    let s = state(&body);
    let v = s.parts();
    let id = v.id("word/document.xml").unwrap();
    let story = v.story(id).unwrap();
    let m = story_model(&v, id, &story, &mut Atoms::default()).unwrap();
    let groups: Vec<Option<u32>> = m.containers[0]
        .blocks
        .iter()
        .map(|n| match &n.body {
            Body::Para(p) => p.group,
            Body::Table(_) => None,
        })
        .collect();
    assert_eq!(groups, vec![None, Some(1), Some(1), Some(1), None]);
}

pub(super) fn note(id: i32, text: &str) -> String {
    format!("<w:footnote w:id=\"{id}\"><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:footnote>")
}

pub(super) fn with_notes(body: &str, notes: &[String]) -> State {
    let xml = format!(
        "{}<w:footnotes xmlns:w=\"{}\">{}{}{}</w:footnotes>",
        parts::DECL,
        parts::NS_W,
        "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>",
        "<w:footnote w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote>",
        notes.concat()
    );
    package(
        body,
        &[("footnotes.xml", "footnotes", "rId1", "footnotes", xml)],
    )
}

pub(super) fn with_ref(text: &str, id: i32) -> String {
    format!(
        "<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r><w:r><w:footnoteReference w:id=\"{id}\"/></w:r></w:p>"
    )
}

#[test]
fn a_note_pairs_through_its_reference_in_an_unchanged_stretch_only() {
    let a = with_notes(
        &format!(
            "{}{}",
            with_ref(
                "The seller warrants the title to the goods sold under this agreement",
                1
            ),
            with_ref(
                "The buyer shall pay within thirty days of the invoice date",
                2
            )
        ),
        &[
            note(1, "See the schedule of goods"),
            note(2, "Payment terms are in the annex"),
        ],
    );
    let b = with_notes(
        &format!(
            "{}{}",
            with_ref(
                "The seller warrants the title to the goods sold under this agreement",
                7
            ),
            with_ref(
                "The buyer shall pay within sixty days of the invoice date and interest",
                8
            )
        ),
        &[
            note(7, "See the schedule of goods and the annex"),
            note(8, "Payment terms are in the annex"),
        ],
    );
    let pairings = pair_documents(&a, &b, &mut Atoms::default()).unwrap();
    let notes = pairings
        .stories
        .iter()
        .find(|s| s.kind == crate::wordml::StoryKind::Footnotes)
        .unwrap();
    let ids: Vec<(Option<i64>, Option<i64>)> =
        notes.roots.iter().map(|r| (r.note_a, r.note_b)).collect();
    // the first reference sits at the end of an unchanged paragraph, and the second after an edit: it is the last character, so unchanged
    assert!(ids.contains(&(Some(1), Some(7))), "{ids:?}");
    // a reference inside the edited words would not pair: the reference is the character after the text and stays mapped
    assert!(ids.len() <= 2);
}

fn header(text: &str) -> String {
    format!(
        "{}<w:hdr xmlns:w=\"{}\"><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:hdr>",
        parts::DECL,
        parts::NS_W
    )
}

fn body_with_header(rid: &str) -> String {
    format!(
        "{}<w:p><w:pPr><w:sectPr><w:headerReference w:type=\"default\" r:id=\"{rid}\"/></w:sectPr></w:pPr></w:p>{}",
        p("Section one text of the agreement"),
        p("Section two text of the agreement")
    )
}

#[test]
fn headers_pair_by_section_slot_and_a_slot_on_one_side_only_is_skipped() {
    let a = package(
        &body_with_header("rId5"),
        &[(
            "header1.xml",
            "header",
            "rId5",
            "header",
            header("Draft one"),
        )],
    );
    let b = package(
        &body_with_header("rId9"),
        &[(
            "header2.xml",
            "header",
            "rId9",
            "header",
            header("Draft two"),
        )],
    );
    let pairings = pair_documents(&a, &b, &mut Atoms::default()).unwrap();
    let h: Vec<_> = pairings
        .stories
        .iter()
        .filter(|s| s.kind == crate::wordml::StoryKind::Header)
        .collect();
    assert_eq!(h.len(), 1);
    assert_eq!(&*h[0].part_a, "word/header1.xml");
    assert_eq!(&*h[0].part_b, "word/header2.xml");
    assert_eq!(pairings.unmatched_slots, 0);
    // one side has no header
    let c = state(&format!(
        "{}{}",
        p("Section one text of the agreement"),
        p("Section two text of the agreement")
    ));
    let pairings = pair_documents(&a, &c, &mut Atoms::default()).unwrap();
    assert!(
        pairings
            .stories
            .iter()
            .all(|s| s.kind != crate::wordml::StoryKind::Header)
    );
}
