//! The blocks, stories and projections of the generator in `testing_gen.rs`.

use super::model::*;
use super::testing::{author, base, pref};
use super::testing_gen::{Gen, STYLES};
use crate::api::view::{Mode, Position, Warning, WarningCode};
use crate::refs::{BlockRef, RefKind};
use crate::wordml::{SectionBreak, StoryKind, VMerge};
use std::sync::Arc;

impl Gen {
    fn prop_revs(&mut self) -> Vec<PropRev> {
        let n = if self.rng.chance(8) {
            1 + self.rng.below(2)
        } else {
            0
        };
        (0..n)
            .map(|_| PropRev {
                kind: *self.rng.pick(&[
                    ChangeKind::Run,
                    ChangeKind::Paragraph,
                    ChangeKind::Section,
                    ChangeKind::Table,
                    ChangeKind::Grid,
                    ChangeKind::Row,
                    ChangeKind::Cell,
                    ChangeKind::Numbering,
                ]),
                tag: self.rev_tag(),
            })
            .collect()
    }

    fn mark(&mut self) -> Option<MarkRev> {
        self.rng.chance(8).then(|| {
            let kind = *self.rng.pick(&[
                MarkKind::Inserted,
                MarkKind::Deleted,
                MarkKind::MovedTo,
                MarkKind::MovedFrom,
            ]);
            let mut tag = self.rev_tag();
            if matches!(kind, MarkKind::MovedTo | MarkKind::MovedFrom) {
                tag.peer = Some(self.next_id + 100);
            }
            MarkRev { kind, tag }
        })
    }

    /// A paragraph. With `refless` it has no ref and no tags.
    pub(super) fn paragraph(&mut self, refless: bool) -> Paragraph {
        let mut inlines = self.inlines(0);
        if self.rng.chance(4) {
            inlines = vec![Inline::Text(self.long_string())];
        }
        if refless {
            return Paragraph {
                r#ref: None,
                style: None,
                label: None,
                mark: None,
                revs: Vec::new(),
                flags: ParaFlags::default(),
                inlines,
                final_text: None,
            };
        }
        let label = self.rng.chance(15).then(|| Label {
            bullet: self.rng.chance(40),
            text: if self.rng.chance(40) {
                "\u{2022}".into()
            } else {
                self.words().into()
            },
            ghost: self.rng.chance(20),
            hidden: self.rng.chance(10),
        });
        Paragraph {
            r#ref: self.new_ref(RefKind::Paragraph),
            style: self.rng.chance(30).then(|| self.rng.word(STYLES).into()),
            label,
            mark: self.mark(),
            revs: self.prop_revs(),
            flags: ParaFlags {
                hidden: self.rng.chance(5),
                joins_next: self.rng.chance(5),
                readonly: self.rng.chance(5).then_some(ReadOnly::DeletedBlock),
            },
            inlines,
            final_text: self.rng.chance(10).then(|| self.words().into()),
        }
    }

    fn table(&mut self, depth: usize) -> Table {
        let rows = 1 + self.rng.below(3);
        let table_ref = self.new_ref(RefKind::Table);
        let revs = self.prop_revs();
        let mut out = Vec::new();
        for _ in 0..rows {
            let row_ref = self.new_ref(RefKind::Row);
            let cells = 1 + self.rng.below(3);
            let mut row_cells = Vec::new();
            for col in 0..cells {
                let n = 1 + self.rng.below(2);
                let mut blocks = Vec::new();
                for _ in 0..n {
                    if depth < 1 && self.rng.chance(5) {
                        blocks.push(Block::Table(self.table(depth + 1)));
                    } else {
                        blocks.push(Block::Paragraph(self.paragraph(false)));
                    }
                }
                row_cells.push(Cell {
                    col: col as u32,
                    span: if self.rng.chance(15) { 2 } else { 1 },
                    v_merge: *self.rng.pick(&[
                        VMerge::None,
                        VMerge::None,
                        VMerge::Restart,
                        VMerge::Continue,
                    ]),
                    change: self.rng.chance(8).then(|| CellRev {
                        kind: *self.rng.pick(&[
                            CellChange::Inserted,
                            CellChange::Deleted,
                            CellChange::Merged,
                        ]),
                        tag: self.rev_tag(),
                    }),
                    blocks,
                });
            }
            out.push(Row {
                r#ref: row_ref,
                change: self.rng.chance(8).then(|| RowRev {
                    kind: *self.rng.pick(&[RowChange::Inserted, RowChange::Deleted]),
                    tag: self.rev_tag(),
                }),
                header: self.rng.chance(20),
                revs: self.prop_revs(),
                cells: row_cells,
            });
        }
        Table {
            r#ref: table_ref,
            style: self.rng.chance(50).then(|| self.rng.word(STYLES).into()),
            cols: 1 + self.rng.below(4) as u32,
            revs,
            rows: out,
        }
    }

    fn section(&mut self, index: u32) -> SectionLine {
        SectionLine {
            index,
            break_type: *self.rng.pick(&[
                SectionBreak::NextPage,
                SectionBreak::Continuous,
                SectionBreak::EvenPage,
                SectionBreak::OddPage,
                SectionBreak::NextColumn,
            ]),
            title_pg: self.rng.chance(30),
            mark: self.mark(),
            revs: self.prop_revs(),
        }
    }

    fn opaque(&mut self) -> OpaqueBlock {
        match self.rng.below(4) {
            0 => OpaqueBlock::AltChunk,
            1 => OpaqueBlock::Math(self.words().into()),
            2 => OpaqueBlock::Unknown,
            _ => OpaqueBlock::Collapsed {
                keyword: "toc".into(),
                first: BlockRef::new(RefKind::Paragraph, 10),
                last: self
                    .rng
                    .chance(80)
                    .then(|| BlockRef::new(RefKind::Paragraph, 67))
                    .flatten(),
                blocks: 1 + self.rng.below(60) as u32,
            },
        }
    }

    fn blocks(&mut self, body: bool, refless: bool) -> Vec<Block> {
        let n = 1 + self.rng.below(7);
        let mut out = Vec::new();
        let mut section = 1;
        for _ in 0..n {
            let roll = self.rng.below(100);
            if refless {
                out.push(Block::Paragraph(self.paragraph(true)));
            } else if roll < 76 {
                out.push(Block::Paragraph(self.paragraph(false)));
            } else if roll < 88 {
                out.push(Block::Table(self.table(0)));
            } else if roll < 94 && body {
                out.push(Block::Section(self.section(section)));
                section += 1;
            } else {
                out.push(Block::Opaque(self.opaque()));
            }
        }
        out
    }

    fn unit(&mut self, kind: UnitKind, ordinal: u32) -> Unit {
        let comment = kind == UnitKind::Comment;
        let meta = if comment {
            UnitMeta {
                author: Some(u32::try_from(self.rng.below(self.authors)).unwrap()),
                initials: Some("JD".into()),
                date: self
                    .rng
                    .chance(70)
                    .then(|| Arc::from("2026-09-30T10:00:00Z")),
                parent: self.rng.chance(30).then_some(1),
                resolved: self.rng.chance(30),
                on: BlockRef::new(RefKind::Paragraph, 1 + self.rng.below(20) as u32),
                anchored: self.opt_words(),
            }
        } else {
            UnitMeta::default()
        };
        let note = matches!(kind, UnitKind::Footnote | UnitKind::Endnote);
        let mark = (note && self.rng.chance(30)).then(|| 1 + self.rng.below(9) as u32);
        Unit {
            kind,
            id: Some(i64::from(ordinal)),
            ordinal,
            mark,
            meta,
            blocks: self.blocks(kind == UnitKind::Body, comment),
        }
    }

    fn story(&mut self, kind: StoryKind) -> Story {
        let (unit_kind, count) = match kind {
            StoryKind::Body => (UnitKind::Body, 1),
            StoryKind::Header => (UnitKind::Header, 1),
            StoryKind::Footer => (UnitKind::Footer, 1),
            StoryKind::Footnotes => (UnitKind::Footnote, 1 + self.rng.below(2)),
            StoryKind::Endnotes => (UnitKind::Endnote, 1 + self.rng.below(2)),
            StoryKind::Comments => (UnitKind::Comment, 1 + self.rng.below(3)),
        };
        let uses = if matches!(kind, StoryKind::Header | StoryKind::Footer) {
            (1..=1 + self.rng.below(3) as u32)
                .map(|section| SectionUse {
                    section,
                    slot: *self.rng.pick(&[Slot::Default, Slot::First, Slot::Even]),
                })
                .collect()
        } else {
            Vec::new()
        };
        let units = (0..count)
            .map(|i| self.unit(unit_kind, 1 + i as u32))
            .collect();
        Story {
            kind,
            part: Arc::from(format!("word/{kind:?}.xml").to_lowercase()),
            uses,
            units,
        }
    }

    /// A projection of the readable files of a document: the body, and some of the other stories.
    pub(super) fn projection(&mut self) -> Projection {
        let mut stories = vec![self.story(StoryKind::Body)];
        for (kind, percent) in [
            (StoryKind::Footnotes, 40),
            (StoryKind::Endnotes, 20),
            (StoryKind::Header, 30),
            (StoryKind::Footer, 30),
            (StoryKind::Comments, 40),
        ] {
            if self.rng.chance(percent) {
                stories.push(self.story(kind));
            }
        }
        let mut authors = vec![author("JD", "Jane Doe"), author("EW", "Eric White")];
        if self.rng.chance(30) {
            authors.push(author("A1", "Ann \"A\" Lee"));
            self.authors = 3;
        }
        Projection {
            head: Head {
                base: base(),
                mode: Mode::Redline,
                output: Output::Files,
                common_style: self.rng.chance(80).then(|| "Body Text".into()),
                authors,
                summary: Summary {
                    changes: vec![9, 3],
                    comments: 4,
                    resolved: 1,
                    footnotes: 2,
                    endnotes: 0,
                    sections: 3,
                    readonly: 0,
                },
                position: self.rng.chance(30).then(|| Position {
                    scope: "p1..p9".to_string(),
                    first: Some("p1".to_string()),
                    last: Some("p9".to_string()),
                    shown: 9,
                    total: 20,
                    remaining_tokens: 123,
                }),
                warnings: if self.rng.chance(30) {
                    vec![Warning {
                        code: WarningCode::LooksHidden,
                        block: Some(pref(3).to_string()),
                        story: Some("body".to_string()),
                        message: self.string(3),
                        next: None,
                    }]
                } else {
                    Vec::new()
                },
            },
            body: Body::Stories(stories),
            appendix: Vec::new(),
        }
    }
}

/// The projection for a seed.
pub(super) fn generate(seed: u64) -> Projection {
    Gen::new(seed).projection()
}
