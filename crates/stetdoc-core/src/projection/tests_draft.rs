//! The draft of a model: what the unedited files must parse to.

use super::draft::{Draft, DraftBlock, DraftLine, Facts, Policy, Seam};
use super::keys::{Key, UnitKey};
use super::model::*;
use super::testing::*;
use super::testing_gen_blocks::generate;

const POLICY: Policy = Policy {
    files: ["document.md", "notes.md", "headers.md", "comments.md"],
    view: "view",
};

fn blocks_of(draft: &Draft, name: &str) -> Vec<DraftBlock> {
    draft.file(name).unwrap().blocks().cloned().collect()
}

#[test]
fn a_model_is_a_draft_with_one_file_for_each_group_of_stories() {
    let draft = sample().draft(&POLICY);
    let names: Vec<&str> = draft.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["document.md", "notes.md"]);
    let file = draft.file("document.md").unwrap();
    assert_eq!(file.base.as_deref(), Some(base().to_string().as_str()));
    assert_eq!(file.common_style.as_deref(), Some("Body Text"));
}

#[test]
fn a_paragraph_line_has_its_tags_in_a_fixed_order() {
    let draft = sample().draft(&POLICY);
    let blocks = blocks_of(&draft, "document.md");
    let heading = blocks.first().unwrap();
    assert_eq!(heading.r#ref, Some(pref(1)));
    assert!(
        matches!(&heading.tags[..], [Key::Style(s), Key::Label(l)] if &**s == "Heading 1" && &*l.text == "1.")
    );
    assert_eq!(heading.texts, vec!["DEFINITIONS"]);
    assert!(heading.seams.is_empty());
}

#[test]
fn trailing_spaces_are_dropped_from_the_draft_and_counted_in_the_facts() {
    let draft = sample().draft(&POLICY);
    let blocks = blocks_of(&draft, "document.md");
    let loss = &blocks[1];
    assert_eq!(loss.texts.last().unwrap(), "\tliability");
    assert_eq!(loss.facts.trailing, 2);
}

#[test]
fn slot_starts_count_scalar_values_with_seam_widths() {
    // `In this Agreement, ` (19) then a deletion (0), an insertion (open, 6, close), then ` means any `, a note ref (1), `\tliability`
    let draft = sample().draft(&POLICY);
    let blocks = blocks_of(&draft, "document.md");
    let loss = &blocks[1];
    assert_eq!(loss.texts.len(), loss.seams.len() + 1);
    let starts = loss.slot_starts();
    assert_eq!(starts[0], 0);
    assert_eq!(
        starts[1], 19,
        "the slot after the deletion starts where the deletion stood"
    );
    assert_eq!(
        starts[2], 19,
        "the slot inside the insertion starts at the same place"
    );
    assert_eq!(
        starts[3],
        19 + 6,
        "the slot after the close starts after the inserted text"
    );
    assert_eq!(
        starts[4],
        19 + 6 + " means any ".chars().count() as u32 + 1,
        "a note reference is one character"
    );
}

#[test]
fn equality_is_content_equality() {
    let draft = sample().draft(&POLICY);
    let mut other = draft.clone();
    for file in &mut other.files {
        for line in &mut file.lines {
            match line {
                DraftLine::Block(b) => {
                    b.line = 99;
                    b.facts = Facts {
                        readonly: Some(ReadOnly::DeletedBlock),
                        trailing: 7,
                    };
                    for seam in &mut b.seams {
                        seam.width += 5;
                    }
                }
                DraftLine::Structure(s) => s.line = 4,
            }
        }
    }
    assert_eq!(
        draft, other,
        "a line number, a width and the facts are not content"
    );
    let mut changed = draft.clone();
    if let DraftLine::Block(b) = &mut changed.files[0].lines[0] {
        b.texts[0].push('x');
    }
    assert_ne!(draft, changed);
    let mut retagged = draft.clone();
    if let DraftLine::Block(b) = &mut retagged.files[0].lines[0] {
        b.tags.clear();
    }
    assert_ne!(draft, retagged);
    let mut reseamed = draft.clone();
    for line in &mut reseamed.files[0].lines {
        if let DraftLine::Block(DraftBlock { seams, .. }) = line
            && let Some(Seam { key, .. }) = seams.first_mut()
        {
            *key = Key::PageBreak;
            break;
        }
    }
    assert_ne!(draft, reseamed);
}

#[test]
fn a_table_is_structure_lines_around_the_paragraphs_of_its_cells() {
    let draft = sample().draft(&POLICY);
    let file = draft.file("document.md").unwrap();
    let kinds: Vec<&str> = file
        .lines
        .iter()
        .map(|line| match line {
            DraftLine::Block(b) => match b.r#ref {
                Some(_) => "p",
                None => "new",
            },
            DraftLine::Structure(s) => match &s.key {
                Key::Table(_) => "table",
                Key::Row(_) => "row",
                Key::EndTable => "end",
                Key::Section(_) => "section",
                _ => "other",
            },
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            "p", "p", "table", "row", "p", "p", "p", "end", "section", "p"
        ]
    );
    let blocks = blocks_of(&draft, "document.md");
    // the second paragraph of a cell carries `{+}` and the first carries nothing
    assert!(blocks[2].tags.is_empty(), "Item");
    assert!(blocks[3].tags.is_empty(), "Price");
    assert_eq!(blocks[4].tags, vec![Key::CellPlus], "per unit");
}

#[test]
fn cell_tags_go_on_the_first_block_of_a_cell() {
    let mut c = cell(vec![plain(1, "a"), plain(2, "b")]);
    c.span = 3;
    c.v_merge = crate::wordml::VMerge::Restart;
    let row = Row {
        r#ref: Some(crate::refs::BlockRef::new(crate::refs::RefKind::Row, 1).unwrap()),
        change: None,
        header: false,
        revs: Vec::new(),
        cells: vec![c],
    };
    let table = Block::Table(Table {
        r#ref: Some(crate::refs::BlockRef::new(crate::refs::RefKind::Table, 1).unwrap()),
        style: None,
        cols: 3,
        revs: Vec::new(),
        rows: vec![row],
    });
    let draft = files_projection(vec![body_story(vec![table])]).draft(&POLICY);
    let blocks = blocks_of(&draft, "document.md");
    assert_eq!(
        blocks[0].tags,
        vec![Key::CellSpan(3), Key::CellVMerge { restart: true }]
    );
    assert_eq!(blocks[1].tags, vec![Key::CellPlus]);
}

#[test]
fn notes_headers_and_comments_open_with_a_unit_line() {
    let mut projection = sample();
    let Body::Stories(stories) = &mut projection.body else {
        panic!("a model of files has stories");
    };
    stories.push(Story {
        kind: crate::wordml::StoryKind::Header,
        part: "word/header1.xml".into(),
        uses: vec![
            SectionUse {
                section: 1,
                slot: Slot::Default,
            },
            SectionUse {
                section: 3,
                slot: Slot::Default,
            },
        ],
        units: vec![Unit {
            kind: UnitKind::Header,
            id: None,
            ordinal: 0,
            mark: None,
            meta: UnitMeta::default(),
            blocks: vec![plain(8, "ACME")],
        }],
    });
    stories.push(Story {
        kind: crate::wordml::StoryKind::Comments,
        part: "word/comments.xml".into(),
        uses: Vec::new(),
        units: vec![Unit {
            kind: UnitKind::Comment,
            id: Some(3),
            ordinal: 0,
            mark: None,
            meta: UnitMeta {
                author: Some(0),
                date: Some("2026-09-30".into()),
                on: Some(pref(2)),
                resolved: true,
                anchored: Some("the Purchase Price".into()),
                ..UnitMeta::default()
            },
            blocks: vec![Block::Paragraph(Paragraph {
                r#ref: None,
                ..para(1, vec![text("Capitalise?")])
            })],
        }],
    });
    let draft = projection.draft(&POLICY);
    let names: Vec<&str> = draft.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["document.md", "notes.md", "headers.md", "comments.md"]
    );
    let notes = draft.file("notes.md").unwrap();
    assert!(
        matches!(&notes.lines[0], DraftLine::Structure(s) if s.key == Key::Unit(UnitKey::Note { endnote: false, ordinal: 1, mark: None }))
    );
    let headers = draft.file("headers.md").unwrap();
    assert!(matches!(&headers.lines[0], DraftLine::Structure(s)
        if matches!(&s.key, Key::Unit(UnitKey::Header { footer: false, uses }) if uses.len() == 2)));
    let comments = draft.file("comments.md").unwrap();
    assert!(matches!(&comments.lines[0], DraftLine::Structure(s)
        if matches!(&s.key, Key::Unit(UnitKey::Comment { id: 3, resolved: true, on: Some(_), .. }))));
    assert!(
        matches!(&comments.lines[1], DraftLine::Structure(s) if matches!(&s.key, Key::Anchored(t) if &**t == "the Purchase Price"))
    );
}

#[test]
fn a_view_is_one_file() {
    let mut projection = sample();
    projection.head.output = Output::View {
        name: "body".into(),
    };
    let draft = projection.draft(&POLICY);
    assert_eq!(draft.files.len(), 1);
    assert_eq!(draft.files[0].name, "view");
}

#[test]
fn every_key_has_a_name_for_a_message() {
    for seed in 0..40 {
        let draft = generate(seed).draft(&POLICY);
        for file in &draft.files {
            for line in &file.lines {
                match line {
                    DraftLine::Block(b) => {
                        for key in b.tags.iter().chain(b.seams.iter().map(|s| &s.key)) {
                            assert!(key.describe().starts_with("the "), "{key:?}");
                        }
                    }
                    DraftLine::Structure(s) => assert!(!s.key.describe().is_empty()),
                }
            }
        }
    }
}

#[test]
fn a_draft_is_deterministic() {
    assert_eq!(generate(3).draft(&POLICY), generate(3).draft(&POLICY));
}

#[test]
fn a_seam_with_no_width_cannot_say_which_side_of_it_a_position_is_on() {
    let draft = sample().draft(&POLICY);
    let blocks = blocks_of(&draft, "document.md");
    let widths: Vec<bool> = blocks[1].seams.iter().map(Seam::is_zero_width).collect();
    // the deletion, the insertion's open and close take no room, the note reference takes one character
    assert_eq!(widths, vec![true, true, true, false]);
}

#[test]
fn the_facts_say_why_a_paragraph_is_read_only() {
    let mut projection = sample();
    let Body::Stories(stories) = &mut projection.body else {
        panic!("a model of files has stories");
    };
    if let Block::Paragraph(first) = &mut stories[0].units[0].blocks[0] {
        first.flags.readonly = Some(ReadOnly::LockedControl);
    }
    let draft = projection.draft(&POLICY);
    let blocks = blocks_of(&draft, "document.md");
    assert_eq!(blocks[0].facts.readonly, Some(ReadOnly::LockedControl));
    assert_eq!(blocks[1].facts.readonly, None);
}

#[test]
fn a_plain_line_is_one_slot() {
    let line = DraftBlock::plain(None, "new text");
    assert_eq!(line.texts, vec!["new text"]);
    assert!(line.seams.is_empty() && line.tags.is_empty());
    assert_eq!(line.slot_starts(), vec![0]);
}
