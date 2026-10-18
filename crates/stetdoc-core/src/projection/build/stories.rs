//! Stories and units: the body, the headers and footers with the sections that use them, the notes in the
//! order the agent sees them, and the comments with their author, date and anchor.
//!
//! Notes are listed by ordinal, which is the position of the first reference in the body, and a note that
//! nothing references follows them with the next ordinals and a warning (design 3.2).

use super::cx::StoryCx;
use super::warnings;
use crate::api::error::StetError;
use crate::projection::model::{
    Block, Margin, NoteKind, SectionUse, Slot, Story, Unit, UnitKind, UnitMeta,
};
use crate::wordml::{BlockKind, NO_UNIT, StoryKind, UnitKind as WordUnit};
use std::ops::Range;
use std::sync::Arc;

/// Which blocks of a story a build takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StorySel {
    /// Every block.
    Whole,
    /// Some tops, each with what is said about it.
    Pick(Pick),
    /// The units of a story of notes or comments that have these ids, whole.
    Units(Vec<i64>),
    /// Some paragraphs by their entry, each as a block of its own whatever it is in (a cell, a row), in this
    /// order: what a receipt shows.
    Paragraphs(Vec<usize>),
}

/// Some tops of a story, in the order they are shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Pick {
    /// The tops.
    pub tops: Vec<TopPick>,
    /// The tops are those of the layout with no field collapsed.
    pub expanded: bool,
}

/// One top of a pick: which rows of it, if it is a table that is cut, and the margin notes around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TopPick {
    /// The top, by position in the layout.
    pub top: usize,
    /// For a table: the rows to show. `None` is all of them.
    pub rows: Option<Range<usize>>,
    /// Margin notes before the top.
    pub before: Vec<Margin>,
    /// Margin notes after the top.
    pub after: Vec<Margin>,
}

impl TopPick {
    /// A top shown whole, with no margin notes.
    pub(crate) fn whole(top: usize) -> TopPick {
        TopPick {
            top,
            rows: None,
            before: Vec::new(),
            after: Vec::new(),
        }
    }
}

impl Pick {
    /// A run of tops, each whole, with no margin notes.
    pub(crate) fn run(tops: Range<usize>) -> Pick {
        Pick {
            tops: tops.map(TopPick::whole).collect(),
            expanded: false,
        }
    }
}

impl<'a> StoryCx<'a> {
    /// The model of the story, or `None` when the selection holds nothing of it.
    pub(super) fn story_model(&mut self, sel: &StorySel) -> Result<Option<Story>, StetError> {
        if let StorySel::Paragraphs(entries) = sel {
            return self.paragraphs_model(entries);
        }
        let expanded = matches!(sel, StorySel::Pick(p) if p.expanded);
        let tops = self.parts.tops(self.part, self.mode, expanded)?;
        let owned: Vec<TopPick>;
        let picks: &[TopPick] = match sel {
            StorySel::Whole => {
                owned = (0..tops.len()).map(TopPick::whole).collect();
                &owned
            }
            StorySel::Pick(p) => &p.tops,
            StorySel::Paragraphs(_) => &[],
            StorySel::Units(ids) => {
                let units = self.story.units();
                owned = tops
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| {
                        units
                            .get(t.unit as usize)
                            .and_then(|u| u.id)
                            .is_some_and(|id| ids.contains(&id))
                    })
                    .map(|(i, _)| TopPick::whole(i))
                    .collect();
                &owned
            }
        };
        let mut groups: Vec<(u32, Vec<Block>)> = Vec::new();
        let mut leading = if picks.first().is_some_and(|p| p.top == 0) {
            self.opaque_first()
        } else {
            Vec::new()
        };
        for pick in picks {
            let Some(top) = tops.get(pick.top) else {
                continue;
            };
            let mut blocks: Vec<Block> = pick.before.iter().cloned().map(Block::Margin).collect();
            blocks.extend(self.top_blocks(top, pick.rows.clone())?);
            blocks.extend(self.opaque_after(top));
            blocks.extend(pick.after.iter().cloned().map(Block::Margin));
            if groups.last().map(|g| g.0) != Some(top.unit) {
                groups.push((top.unit, std::mem::take(&mut leading)));
            }
            if let Some(g) = groups.last_mut() {
                g.1.extend(blocks);
            }
        }
        if groups.is_empty() && !leading.is_empty() {
            groups.push((NO_UNIT, leading));
        }
        if groups.is_empty() && !matches!(sel, StorySel::Whole) {
            return Ok(None);
        }
        let units = self.units(groups)?;
        self.story_warnings();
        let uses = self.uses()?;
        if units.is_empty() {
            return Ok(None);
        }
        Ok(Some(Story {
            kind: self.kind,
            part: self.name.clone(),
            uses,
            units,
        }))
    }

    /// The model of some paragraphs of the story, each a top-level block of the unit it is in.
    fn paragraphs_model(&mut self, entries: &[usize]) -> Result<Option<Story>, StetError> {
        let mut groups: Vec<(u32, Vec<Block>)> = Vec::new();
        for &i in entries {
            let Some(entry) = self.entry(i) else {
                continue;
            };
            if entry.kind != BlockKind::Paragraph {
                continue;
            }
            let Some((paragraph, _)) = self.paragraph(i, None, false)? else {
                continue;
            };
            if groups.last().map(|g| g.0) != Some(entry.unit) {
                groups.push((entry.unit, Vec::new()));
            }
            if let Some(g) = groups.last_mut() {
                g.1.push(Block::Paragraph(paragraph));
            }
        }
        if groups.is_empty() {
            return Ok(None);
        }
        let units = self.units(groups)?;
        let uses = self.uses()?;
        if units.is_empty() {
            return Ok(None);
        }
        Ok(Some(Story {
            kind: self.kind,
            part: self.name.clone(),
            uses,
            units,
        }))
    }

    /// The units of the story from the blocks of each, in the order the agent sees them.
    fn units(&mut self, groups: Vec<(u32, Vec<Block>)>) -> Result<Vec<Unit>, StetError> {
        let story = self.story.clone();
        match self.kind {
            StoryKind::Body | StoryKind::Header | StoryKind::Footer => {
                let blocks: Vec<Block> = groups.into_iter().flat_map(|g| g.1).collect();
                Ok(vec![Unit {
                    kind: self.single_unit_kind(),
                    id: None,
                    ordinal: 0,
                    mark: None,
                    meta: UnitMeta::default(),
                    blocks,
                }])
            }
            StoryKind::Footnotes | StoryKind::Endnotes => {
                let kind = if self.kind == StoryKind::Footnotes {
                    NoteKind::Footnote
                } else {
                    NoteKind::Endnote
                };
                let mut next_extra = self.facts.referenced_count(kind);
                let mut out: Vec<(u32, Unit)> = Vec::new();
                for (u, blocks) in groups {
                    let Some(unit) = story.units().get(u as usize) else {
                        continue;
                    };
                    // A note whose reference is a pending deletion goes when it is accepted, so it is not in `Final`; one
                    // whose reference is a pending insertion goes when it is rejected, so it is not in `Original`.
                    // The others keep the number refs and operations name them by in every view (`fn3`), and carry the
                    // number Word shows as their mark once the mode's changes are resolved, when that is another.
                    if unit
                        .id
                        .is_some_and(|id| self.facts.gone_in(self.mode, kind, id))
                    {
                        continue;
                    }
                    let known = unit.id.and_then(|id| self.facts.ordinal(kind, id));
                    let mark = unit
                        .id
                        .and_then(|id| self.facts.ordinal_in(self.mode, kind, id))
                        .filter(|m| Some(*m) != known);
                    let ordinal = known.unwrap_or_else(|| {
                        next_extra += 1;
                        next_extra
                    });
                    if known.is_none() {
                        let word = match kind {
                            NoteKind::Footnote => "footnote",
                            NoteKind::Endnote => "endnote",
                        };
                        let w = warnings::unreferenced_note(self.story_label(), ordinal, word);
                        self.warnings.push(w);
                    }
                    out.push((
                        ordinal,
                        Unit {
                            kind: match unit.kind {
                                WordUnit::Endnote => UnitKind::Endnote,
                                _ => UnitKind::Footnote,
                            },
                            id: unit.id,
                            ordinal,
                            mark,
                            meta: UnitMeta::default(),
                            blocks,
                        },
                    ));
                }
                out.sort_by_key(|(ordinal, _)| *ordinal);
                Ok(out.into_iter().map(|(_, u)| u).collect())
            }
            StoryKind::Comments => {
                let threads = crate::threads::Threads::of(&self.parts)?;
                let mut out = Vec::new();
                for (u, blocks) in groups {
                    let Some(unit) = story.units().get(u as usize) else {
                        continue;
                    };
                    out.push(Unit {
                        kind: UnitKind::Comment,
                        id: unit.id,
                        ordinal: 0,
                        mark: None,
                        meta: self.comment_meta(unit.id, unit, &threads),
                        blocks,
                    });
                }
                Ok(out)
            }
        }
    }

    /// What a comment adds to its unit: who, when, where it is anchored, the comment it replies to and whether
    /// its thread is resolved. The thread comes from `Threads::thread_of`, which is exact when ids repeat, and
    /// a comment whose id two comments share says nothing about its thread.
    fn comment_meta(
        &self,
        id: Option<i64>,
        unit: &crate::wordml::Unit,
        threads: &crate::threads::Threads,
    ) -> UnitMeta {
        let thread = id
            .filter(|id| !threads.is_ambiguous(*id))
            .and_then(|id| Some((id, threads.thread_of(id)?)));
        let parent = thread.and_then(|(id, t)| {
            let named = threads.comment(id).and_then(|c| c.parent);
            named.or((t.root != id).then_some(t.root))
        });
        let at = id.and_then(|id| self.facts.comment_at.get(&id));
        let on = at.and_then(|a| {
            let node = a.node?;
            self.parts
                .ref_of(&crate::refs::BlockKey::new(a.part.clone(), node))
        });
        UnitMeta {
            author: self.facts.author_of(unit.author.as_deref()),
            initials: unit.initials.clone(),
            date: unit.date.as_deref().map(Arc::from),
            parent,
            resolved: thread.is_some_and(|(_, t)| t.resolved),
            on,
            anchored: at.and_then(|a| a.text.clone()),
        }
    }

    /// The warnings that are about the whole story and not one block.
    pub(super) fn story_warnings(&mut self) {
        let label = self.story_label();
        let mut counts: Vec<(crate::wordml::AnomalyKind, usize)> = Vec::new();
        for a in self.story.anomalies() {
            match counts.iter_mut().find(|(k, _)| *k == a.kind) {
                Some(c) => c.1 += 1,
                None => counts.push((a.kind, 1)),
            }
        }
        for (kind, n) in counts {
            if let Some(w) = warnings::read_anomaly(label, None, kind, n) {
                self.warnings.push(w);
            }
        }
        if let Some(numbers) = self.sites.numbers() {
            let list_num = numbers
                .notes()
                .iter()
                .any(|n| matches!(n, crate::numbering::Note::ListNumField));
            if list_num {
                let w = warnings::numbering(label, None, &crate::numbering::Note::ListNumField);
                self.warnings.push(w);
            }
        }
    }

    /// For a header or footer: the sections that name this part, and as which slot.
    fn uses(&self) -> Result<Vec<SectionUse>, StetError> {
        let header = match self.kind {
            StoryKind::Header => true,
            StoryKind::Footer => false,
            _ => return Ok(Vec::new()),
        };
        let list = self.parts.story_specs()?;
        let Some(main) = list.0.iter().find(|s| s.kind == StoryKind::Body) else {
            return Ok(Vec::new());
        };
        let Some(main_id) = self.parts.id(&main.part) else {
            return Ok(Vec::new());
        };
        let body = self.parts.story(main_id)?;
        let tree = self.parts.tree(main_id)?;
        let rels = self.parts.rels(main_id)?;
        let mut out = Vec::new();
        for (k, section) in body.sections(tree, &rels).iter().enumerate() {
            if !section.exists_in(self.mode) {
                continue;
            }
            let slots = if header {
                &section.own_headers
            } else {
                &section.own_footers
            };
            for (slot, part) in [
                (Slot::Default, &slots.default),
                (Slot::First, &slots.first),
                (Slot::Even, &slots.even),
            ] {
                if part.as_deref() == Some(&*self.name) {
                    out.push(SectionUse {
                        section: u32::try_from(k + 1).unwrap_or(u32::MAX),
                        slot,
                    });
                }
            }
        }
        Ok(out)
    }
}
