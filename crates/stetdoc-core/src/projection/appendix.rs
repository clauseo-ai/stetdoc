//! The appendix of a page: the notes and comments that its blocks cite, so that a page is self-contained.
//!
//! A footnote reference in the text and its note are on the same screen. The entries come from the blocks of
//! the page, in the order the citations appear, and each is cut to 400 characters with a pointer to the file
//! that holds the rest (design 4.6).

use super::build::{Selection, StorySel, project};
use super::model::{
    AppendixEntry, Atom, Block, Inline, Mark, NoteKind, Output, Projection, UnitKind, final_text,
};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::refs::BaseToken;
use crate::state::PartsView;
use crate::wordml::StoryKind;

/// How many characters of a note or a comment the appendix shows.
const CUT: usize = 400;

/// What the blocks of a page cite, in order of first appearance.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Cited {
    /// Footnotes and endnotes by kind and number.
    pub notes: Vec<(NoteKind, u32)>,
    /// Comments by `w:id`.
    pub comments: Vec<i64>,
}

impl Cited {
    fn note(&mut self, kind: NoteKind, ordinal: u32) {
        if ordinal > 0 && !self.notes.contains(&(kind, ordinal)) {
            self.notes.push((kind, ordinal));
        }
    }

    fn comment(&mut self, id: i64) {
        if !self.comments.contains(&id) {
            self.comments.push(id);
        }
    }

    fn inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            match inline {
                Inline::Atom(Atom::NoteRef { kind, ordinal }) => self.note(*kind, *ordinal),
                Inline::Mark(Mark::CommentStart(id) | Mark::CommentEnd(id)) => self.comment(*id),
                Inline::Rev { inlines, .. }
                | Inline::Link { inlines, .. }
                | Inline::Control { inlines, .. }
                | Inline::Hidden { inlines, .. }
                | Inline::LooksHidden { inlines, .. } => self.inlines(inlines),
                _ => {}
            }
        }
    }

    fn blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            match block {
                Block::Paragraph(p) => self.inlines(&p.inlines),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            self.blocks(&cell.blocks);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// What the stories of a projection cite.
pub(crate) fn cited(projection: &Projection) -> Cited {
    let mut out = Cited::default();
    for story in projection.stories() {
        for unit in &story.units {
            out.blocks(&unit.blocks);
        }
    }
    out
}

/// A unit's paragraphs as one string, with deletions left out and the paragraphs joined by ` / `.
pub(super) fn text_of(blocks: &[Block]) -> String {
    let mut lines: Vec<String> = Vec::new();
    for block in blocks {
        if let Block::Paragraph(p) = block {
            let t = final_text(&p.inlines);
            let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
            if !t.is_empty() {
                lines.push(t);
            }
        }
    }
    lines.join(" / ")
}

fn shorten(text: String) -> (Box<str>, bool) {
    if text.chars().count() <= CUT {
        return (text.into(), false);
    }
    let cut: String = text.chars().take(CUT).collect();
    (format!("{cut}…").into(), true)
}

/// The entries for what a projection cites: notes first, then comments.
pub(crate) fn appendix(
    parts: PartsView<'_>,
    base: &BaseToken,
    mode: Mode,
    projection: &Projection,
) -> Result<Vec<AppendixEntry>, StetError> {
    let cites = cited(projection);
    if cites.notes.is_empty() && cites.comments.is_empty() {
        return Ok(Vec::new());
    }
    let facts = parts.doc_facts()?;
    let list = parts.story_specs()?;
    let mut stories = Vec::new();
    for (story_kind, note_kind) in [
        (StoryKind::Footnotes, Some(NoteKind::Footnote)),
        (StoryKind::Endnotes, Some(NoteKind::Endnote)),
        (StoryKind::Comments, None),
    ] {
        let ids: Vec<i64> = match note_kind {
            Some(kind) => cites
                .notes
                .iter()
                .filter(|(k, _)| *k == kind)
                .filter_map(|(_, ordinal)| facts.note_id(kind, *ordinal))
                .collect(),
            None => cites.comments.clone(),
        };
        if ids.is_empty() {
            continue;
        }
        if let Some(part) = list
            .0
            .iter()
            .find(|s| s.kind == story_kind)
            .and_then(|s| parts.id(&s.part))
        {
            stories.push((part, StorySel::Units(ids)));
        }
    }
    let built = project(
        parts,
        *base,
        mode,
        Output::View {
            name: "appendix".into(),
        },
        &Selection { stories },
    )?;
    let mut notes: Vec<(NoteKind, u32, Box<str>, bool)> = Vec::new();
    let mut comments: Vec<AppendixEntry> = Vec::new();
    for story in built.stories() {
        for unit in &story.units {
            let (text, cut) = shorten(text_of(&unit.blocks));
            match unit.kind {
                UnitKind::Footnote => notes.push((NoteKind::Footnote, unit.ordinal, text, cut)),
                UnitKind::Endnote => notes.push((NoteKind::Endnote, unit.ordinal, text, cut)),
                UnitKind::Comment => comments.push(AppendixEntry::Comment {
                    id: unit.id.unwrap_or(0),
                    author: unit.meta.author,
                    date: unit.meta.date.clone(),
                    on: unit.meta.on,
                    anchored: unit.meta.anchored.clone(),
                    text,
                    cut,
                }),
                _ => {}
            }
        }
    }
    // The order the page cites them in.
    let mut out: Vec<AppendixEntry> = Vec::new();
    for (kind, ordinal) in &cites.notes {
        if let Some((k, o, text, cut)) = notes.iter().find(|n| n.0 == *kind && n.1 == *ordinal) {
            out.push(AppendixEntry::Note {
                kind: *k,
                ordinal: *o,
                text: text.clone(),
                cut: *cut,
            });
        }
    }
    for id in &cites.comments {
        if let Some(entry) = comments
            .iter()
            .find(|c| matches!(c, AppendixEntry::Comment { id: i, .. } if i == id))
        {
            out.push(entry.clone());
        }
    }
    Ok(out)
}
