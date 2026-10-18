//! Models built by hand and by a seeded generator, for the tests of the whole module.

use super::model::*;
use crate::api::view::Mode;
use crate::refs::{BaseToken, BlockRef, NamespaceId, RefKind};
use crate::wordml::{ControlKind, SectionBreak, StoryKind, VMerge};
use std::sync::Arc;

/// The base token of the test models.
pub(super) fn base() -> BaseToken {
    BaseToken::initial(NamespaceId::of_package(b"projection tests", 1))
}

pub(super) fn author(code: &str, name: &str) -> Author {
    Author {
        code: code.into(),
        name: name.into(),
    }
}

pub(super) fn authors() -> Vec<Author> {
    vec![author("JD", "Jane Doe"), author("EW", "Eric White")]
}

pub(super) fn pref(n: u32) -> BlockRef {
    BlockRef::new(RefKind::Paragraph, n).unwrap()
}

pub(super) fn tag(id: i64, author: u32) -> RevTag {
    RevTag {
        id,
        author,
        date: None,
        peer: None,
    }
}

pub(super) fn text(s: &str) -> Inline {
    Inline::Text(s.to_string())
}

pub(super) fn ins(id: i64, inlines: Vec<Inline>) -> Inline {
    Inline::Rev {
        kind: RevKind::Ins,
        tag: tag(id, 0),
        inlines,
    }
}

pub(super) fn del(id: i64, s: &str) -> Inline {
    Inline::Rev {
        kind: RevKind::Del,
        tag: tag(id, 0),
        inlines: vec![text(s)],
    }
}

pub(super) fn para(n: u32, inlines: Vec<Inline>) -> Paragraph {
    Paragraph {
        r#ref: Some(pref(n)),
        style: None,
        label: None,
        mark: None,
        revs: Vec::new(),
        flags: ParaFlags::default(),
        inlines,
        final_text: None,
    }
}

pub(super) fn plain(n: u32, s: &str) -> Block {
    Block::Paragraph(para(n, vec![text(s)]))
}

pub(super) fn head(output: Output) -> Head {
    Head {
        base: base(),
        mode: Mode::Redline,
        output,
        common_style: Some("Body Text".into()),
        authors: authors(),
        summary: Summary::default(),
        position: None,
        warnings: Vec::new(),
    }
}

pub(super) fn body_story(blocks: Vec<Block>) -> Story {
    Story {
        kind: StoryKind::Body,
        part: Arc::from("word/document.xml"),
        uses: Vec::new(),
        units: vec![Unit {
            kind: UnitKind::Body,
            id: None,
            ordinal: 0,
            mark: None,
            meta: UnitMeta::default(),
            blocks,
        }],
    }
}

pub(super) fn files_projection(stories: Vec<Story>) -> Projection {
    Projection {
        head: head(Output::Files),
        body: Body::Stories(stories),
        appendix: Vec::new(),
    }
}

pub(super) fn cell(blocks: Vec<Block>) -> Cell {
    Cell {
        col: 0,
        span: 1,
        v_merge: VMerge::None,
        change: None,
        blocks,
    }
}

/// A projection with a little of everything the model has.
pub(super) fn sample() -> Projection {
    let mut heading = para(1, vec![text("DEFINITIONS")]);
    heading.style = Some("Heading 1".into());
    heading.label = Some(Label {
        text: "1.".into(),
        bullet: false,
        ghost: false,
        hidden: false,
    });
    let loss = para(
        2,
        vec![
            text("In this Agreement, "),
            del(4, "\"Losses\""),
            ins(5, vec![text("\"Loss\"")]),
            text(" means any "),
            Inline::Atom(Atom::NoteRef {
                kind: NoteKind::Footnote,
                ordinal: 1,
            }),
            text("\tliability  "),
        ],
    );
    let table = Block::Table(Table {
        r#ref: Some(BlockRef::new(RefKind::Table, 1).unwrap()),
        style: Some("Table Grid".into()),
        cols: 2,
        revs: Vec::new(),
        rows: vec![Row {
            r#ref: Some(BlockRef::new(RefKind::Row, 1).unwrap()),
            change: None,
            header: true,
            revs: Vec::new(),
            cells: vec![
                cell(vec![plain(3, "Item")]),
                Cell {
                    span: 1,
                    ..cell(vec![plain(4, "Price"), plain(5, "per unit")])
                },
            ],
        }],
    });
    let body = body_story(vec![
        Block::Paragraph(heading),
        Block::Paragraph(loss),
        table,
        Block::Section(SectionLine {
            index: 1,
            break_type: SectionBreak::NextPage,
            title_pg: false,
            mark: None,
            revs: Vec::new(),
        }),
        Block::Paragraph(para(
            6,
            vec![Inline::Link {
                target: LinkTarget::External("https://example.com".into()),
                inlines: vec![text("the notices page")],
            }],
        )),
    ]);
    let notes = Story {
        kind: StoryKind::Footnotes,
        part: Arc::from("word/footnotes.xml"),
        uses: Vec::new(),
        units: vec![Unit {
            kind: UnitKind::Footnote,
            id: Some(1),
            ordinal: 1,
            mark: None,
            meta: UnitMeta::default(),
            blocks: vec![Block::Paragraph(para(
                7,
                vec![
                    Inline::Atom(Atom::NoteMark {
                        kind: NoteKind::Footnote,
                    }),
                    text(" The price excludes taxes."),
                ],
            ))],
        }],
    };
    files_projection(vec![body, notes])
}

/// Every kind of control, for the tests that go through all of them.
pub(super) const CONTROL_KINDS: [ControlKind; 14] = [
    ControlKind::PlainText,
    ControlKind::RichText,
    ControlKind::Date,
    ControlKind::DropDown,
    ControlKind::ComboBox,
    ControlKind::Checkbox,
    ControlKind::Picture,
    ControlKind::Citation,
    ControlKind::Bibliography,
    ControlKind::BuildingBlock,
    ControlKind::Group,
    ControlKind::RepeatingSection,
    ControlKind::Equation,
    ControlKind::Unknown,
];
