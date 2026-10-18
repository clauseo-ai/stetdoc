//! `Projection::draft`: the draft that the unedited files must parse to.
//!
//! The law for every format is `parse(render(p)) == p.draft(policy)`. The draft is built from the
//! model alone, in no syntax: it lists, for each file, its lines in order, and for each paragraph its
//! tags, its text slots and its seams. The baseline of an edit is this draft, with the widths of the
//! seams and the facts that a parse cannot know filled in.

use super::draft::{Draft, DraftBlock, DraftFile, DraftLine, Facts, Policy, Structure};
use super::keys::{Key, LabelKey, PropKey, RowKey, SectionKey, TableKey, UnitKey};
use super::model::{
    Author, Block, Cell, Head, Label, Output, Paragraph, Projection, PropRev, Row, SectionLine,
    Story, Table, Unit, UnitKind,
};
use super::model_region::{author_code, regions, rev_ref};
use crate::wordml::{StoryKind, VMerge};

/// Which file a story is written to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileGroup {
    /// The body.
    Document,
    /// Footnotes and endnotes.
    Notes,
    /// Headers and footers.
    Headers,
    /// Comments.
    Comments,
}

impl FileGroup {
    /// The groups in the order the files are written.
    pub(crate) const ALL: [FileGroup; 4] = [
        FileGroup::Document,
        FileGroup::Notes,
        FileGroup::Headers,
        FileGroup::Comments,
    ];

    /// The group a story belongs to.
    pub(crate) fn of(kind: StoryKind) -> FileGroup {
        match kind {
            StoryKind::Body => FileGroup::Document,
            StoryKind::Footnotes | StoryKind::Endnotes => FileGroup::Notes,
            StoryKind::Header | StoryKind::Footer => FileGroup::Headers,
            StoryKind::Comments => FileGroup::Comments,
        }
    }

    /// The position of the group in `ALL` and in `Policy::files`.
    pub(crate) fn index(self) -> usize {
        match self {
            FileGroup::Document => 0,
            FileGroup::Notes => 1,
            FileGroup::Headers => 2,
            FileGroup::Comments => 3,
        }
    }
}

impl Projection {
    /// The draft the unedited output must parse to.
    pub(crate) fn draft(&self, policy: &Policy) -> Draft {
        let stories = self.stories();
        let mut files = Vec::new();
        match &self.head.output {
            Output::View { .. } => {
                let all: Vec<&Story> = stories.iter().collect();
                files.push(file_draft(policy.view, &self.head, &all));
            }
            Output::Files => {
                for group in FileGroup::ALL {
                    let of_group: Vec<&Story> = stories
                        .iter()
                        .filter(|story| FileGroup::of(story.kind) == group)
                        .collect();
                    if of_group.is_empty() {
                        continue;
                    }
                    if let Some(name) = policy.files.get(group.index()) {
                        files.push(file_draft(name, &self.head, &of_group));
                    }
                }
            }
        }
        Draft { files }
    }
}

/// The draft of one file that holds these stories, whatever their kinds.
pub(crate) fn file_draft(name: &str, head: &Head, stories: &[&Story]) -> DraftFile {
    let mut build = Build {
        authors: &head.authors,
        lines: Vec::new(),
    };
    for story in stories {
        for unit in &story.units {
            build.unit(story, unit);
        }
    }
    DraftFile {
        name: name.to_string(),
        base: Some(head.base.to_string()),
        common_style: head.common_style.clone(),
        lines: build.lines,
    }
}

struct Build<'a> {
    authors: &'a [Author],
    lines: Vec<DraftLine>,
}

impl Build<'_> {
    fn structure(&mut self, key: Key) {
        self.lines
            .push(DraftLine::Structure(Structure { key, line: 0 }));
    }

    fn unit(&mut self, story: &Story, unit: &Unit) {
        match unit.kind {
            UnitKind::Body => {}
            UnitKind::Header | UnitKind::Footer => self.structure(Key::Unit(UnitKey::Header {
                footer: unit.kind == UnitKind::Footer,
                uses: story.uses.clone(),
            })),
            UnitKind::Footnote | UnitKind::Endnote => self.structure(Key::Unit(UnitKey::Note {
                endnote: unit.kind == UnitKind::Endnote,
                ordinal: unit.ordinal,
                mark: unit.mark,
            })),
            UnitKind::Comment => {
                let meta = &unit.meta;
                self.structure(Key::Unit(UnitKey::Comment {
                    id: unit.id.unwrap_or(0),
                    author: meta.author.map(|a| author_code(self.authors, a)),
                    date: meta.date.as_deref().map(Into::into),
                    on: meta.on,
                    resolved: meta.resolved,
                    reply_to: meta.parent,
                }));
                if let Some(text) = &meta.anchored {
                    self.structure(Key::Anchored(text.clone()));
                }
            }
        }
        for block in &unit.blocks {
            self.block(block, Vec::new());
        }
    }

    fn block(&mut self, block: &Block, cell_tags: Vec<Key>) {
        match block {
            Block::Paragraph(paragraph) => self.paragraph(paragraph, cell_tags),
            Block::Table(table) => self.table(table, cell_tags),
            Block::Section(section) => self.section(section),
            Block::Opaque(opaque) => self.structure(Key::Opaque(opaque.clone())),
            Block::Margin(margin) => self.structure(Key::Margin(margin.clone())),
        }
    }

    fn prop_keys(&self, revs: &[PropRev]) -> Vec<PropKey> {
        revs.iter()
            .map(|rev| PropKey {
                kind: rev.kind,
                rev: rev_ref(&rev.tag, self.authors),
            })
            .collect()
    }

    fn paragraph(&mut self, paragraph: &Paragraph, cell_tags: Vec<Key>) {
        let mut tags = Vec::new();
        if let Some(style) = &paragraph.style {
            tags.push(Key::Style(style.clone()));
        }
        if let Some(label) = &paragraph.label {
            tags.push(Key::Label(label_key(label)));
        }
        if let Some(mark) = &paragraph.mark {
            tags.push(Key::Mark {
                kind: mark.kind,
                rev: rev_ref(&mark.tag, self.authors),
            });
        }
        tags.extend(self.prop_keys(&paragraph.revs).into_iter().map(Key::Rev));
        if paragraph.flags.hidden {
            tags.push(Key::HiddenPara);
        }
        if paragraph.flags.joins_next {
            tags.push(Key::JoinsNext);
        }
        tags.extend(cell_tags);
        let cut = regions(&paragraph.inlines, self.authors);
        let mut texts = cut.texts;
        let trailing = texts.last_mut().map_or(0, trim_trailing);
        let blank = paragraph.r#ref.is_none()
            && tags.is_empty()
            && cut.seams.is_empty()
            && texts.iter().all(String::is_empty);
        if blank {
            // A line with no ref that is blank is ignored by every parse, so it is not part of a draft.
            return;
        }
        self.lines.push(DraftLine::Block(DraftBlock {
            r#ref: paragraph.r#ref,
            tags,
            texts,
            seams: cut.seams,
            line: 0,
            facts: Facts {
                readonly: paragraph.flags.readonly,
                trailing,
            },
        }));
    }

    fn table(&mut self, table: &Table, cell_tags: Vec<Key>) {
        self.structure(Key::Table(TableKey {
            r#ref: table.r#ref,
            cols: table.cols,
            style: table.style.clone(),
            revs: self.prop_keys(&table.revs),
            cell: cell_tags,
        }));
        for row in &table.rows {
            self.row(row);
        }
        self.structure(Key::EndTable);
    }

    fn row(&mut self, row: &Row) {
        self.structure(Key::Row(RowKey {
            r#ref: row.r#ref,
            header: row.header,
            change: row
                .change
                .as_ref()
                .map(|change| (change.kind, rev_ref(&change.tag, self.authors))),
            revs: self.prop_keys(&row.revs),
        }));
        for cell in &row.cells {
            let first = self.cell_tags(cell);
            for (index, block) in cell.blocks.iter().enumerate() {
                let tags = if index == 0 {
                    first.clone()
                } else {
                    vec![Key::CellPlus]
                };
                self.block(block, tags);
            }
        }
    }

    fn cell_tags(&self, cell: &Cell) -> Vec<Key> {
        let mut tags = Vec::new();
        if let Some(change) = &cell.change {
            tags.push(Key::CellChange {
                kind: change.kind,
                rev: rev_ref(&change.tag, self.authors),
            });
        }
        if cell.span > 1 {
            tags.push(Key::CellSpan(cell.span));
        }
        match cell.v_merge {
            VMerge::None => {}
            VMerge::Restart => tags.push(Key::CellVMerge { restart: true }),
            VMerge::Continue => tags.push(Key::CellVMerge { restart: false }),
        }
        tags
    }

    fn section(&mut self, section: &SectionLine) {
        self.structure(Key::Section(SectionKey {
            index: section.index,
            break_type: section.break_type,
            title_pg: section.title_pg,
            mark: section
                .mark
                .as_ref()
                .map(|mark| (mark.kind, rev_ref(&mark.tag, self.authors))),
            revs: self.prop_keys(&section.revs),
        }));
    }
}

fn label_key(label: &Label) -> LabelKey {
    LabelKey {
        text: label.text.clone(),
        bullet: label.bullet,
        ghost: label.ghost,
        hidden: label.hidden,
    }
}

/// Drops the trailing U+0020 spaces of a text and says how many there were.
fn trim_trailing(text: &mut String) -> u32 {
    let kept = text.trim_end_matches(' ').len();
    let dropped = text.len() - kept;
    text.truncate(kept);
    u32::try_from(dropped).unwrap_or(u32::MAX)
}
