//! The draft: what an agent's edit says, in no format's syntax.
//!
//! A format's `parse` returns a `Draft`, and `Projection::draft` (in `draft_build.rs`) returns the draft
//! that the unedited files must parse to. `files::to_batch` compares the two, so the alignment, the
//! refusals and the operations are written once, and a new format is only syntax. Every paragraph is
//! cut into `k + 1` editable text slots and `k` read-only seams between them (design 3.7). The text
//! between two seams is what an edit may change, and everything else is compared for equality.
//!
//! Equality is content equality. A line number and a seam's width are what a parse cannot know, and
//! `Facts` are what only the baseline knows, so none of the three is part of `==`.

use super::keys::Key;
use super::model::ReadOnly;
use crate::refs::BlockRef;

/// What a format writes its files with, apart from syntax: the names of the files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Policy {
    /// The file names for the body, the notes, the headers and footers, and the comments.
    pub files: [&'static str; 4],
    /// The name of the one file of a view.
    pub view: &'static str,
}

/// The files of an edit, in the order the format writes them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Draft {
    /// One draft for each file.
    pub files: Vec<DraftFile>,
}

/// One file's lines, and what its header said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DraftFile {
    /// The file's name.
    pub name: String,
    /// The base token that the header gave, as text.
    pub base: Option<String>,
    /// The common style that the header gave: the style of a line with no style tag.
    pub common_style: Option<Box<str>>,
    /// The lines after the head block.
    pub lines: Vec<DraftLine>,
}

/// A line of a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DraftLine {
    /// A paragraph, with a ref or without.
    Block(DraftBlock),
    /// Everything else: a table, row, section, opaque or unit line.
    Structure(Structure),
}

/// A structure line and where it was.
#[derive(Clone, Debug)]
pub(crate) struct Structure {
    /// What it says.
    pub key: Key,
    /// The line in the file, from 1. Zero in a baseline. Not part of equality.
    pub line: u32,
}

impl PartialEq for Structure {
    fn eq(&self, other: &Structure) -> bool {
        self.key == other.key
    }
}

impl Eq for Structure {}

/// What only the baseline knows about a paragraph. A parse leaves the default.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Facts {
    /// Why the paragraph as a whole cannot be edited, if it cannot.
    pub readonly: Option<ReadOnly>,
    /// How many trailing spaces the paragraph's text has that the draft dropped.
    pub trailing: u32,
}

/// A read-only thing inside a paragraph's text.
#[derive(Clone, Debug)]
pub(crate) struct Seam {
    /// What it is.
    pub key: Key,
    /// The characters it takes in the paragraph's `Final` text. The baseline fills it; a parse leaves 0.
    /// Not part of equality.
    pub width: u32,
}

impl PartialEq for Seam {
    fn eq(&self, other: &Seam) -> bool {
        self.key == other.key
    }
}

impl Eq for Seam {}

/// A paragraph line: tags, and text slots separated by seams.
#[derive(Clone, Debug)]
pub(crate) struct DraftBlock {
    /// The paragraph's ref. `None` is a new line.
    pub r#ref: Option<BlockRef>,
    /// Style, number, mark and revision tags, and cell tags: all read-only.
    pub tags: Vec<Key>,
    /// `k + 1` editable text slots. The trailing spaces of the last one are not part of the draft.
    pub texts: Vec<String>,
    /// `k` read-only things between the slots.
    pub seams: Vec<Seam>,
    /// The line in the file, from 1. Zero in a baseline. Not part of equality.
    pub line: u32,
    /// What only the baseline knows. Not part of equality.
    pub facts: Facts,
}

impl PartialEq for DraftBlock {
    fn eq(&self, other: &DraftBlock) -> bool {
        self.r#ref == other.r#ref
            && self.tags == other.tags
            && self.texts == other.texts
            && self.seams == other.seams
    }
}

impl Eq for DraftBlock {}

impl DraftBlock {
    /// A paragraph with one slot and no seam, as a ref-less line of plain text is.
    pub(crate) fn plain(r#ref: Option<BlockRef>, text: impl Into<String>) -> DraftBlock {
        DraftBlock {
            r#ref,
            tags: Vec::new(),
            texts: vec![text.into()],
            seams: Vec::new(),
            line: 0,
            facts: Facts::default(),
        }
    }

    /// Where each slot starts in the paragraph's `Final` text, in Unicode scalar values: the sum of the
    /// lengths of the slots and the widths of the seams before it.
    pub(crate) fn slot_starts(&self) -> Vec<u32> {
        let mut at = 0u32;
        let mut starts = Vec::with_capacity(self.texts.len());
        for (index, text) in self.texts.iter().enumerate() {
            starts.push(at);
            at = at.saturating_add(super::model::count(text));
            if let Some(seam) = self.seams.get(index) {
                at = at.saturating_add(seam.width);
            }
        }
        starts
    }
}

impl Seam {
    /// True for a seam that takes no room in the `Final` text (a deletion, a comment mark, an `Open` or
    /// a `Close`), so that an offset in that text cannot say which side of it a position is on.
    pub(crate) fn is_zero_width(&self) -> bool {
        self.width == 0
    }
}

impl DraftFile {
    /// The paragraph lines of the file.
    pub(crate) fn blocks(&self) -> impl Iterator<Item = &DraftBlock> {
        self.lines.iter().filter_map(|line| match line {
            DraftLine::Block(block) => Some(block),
            DraftLine::Structure(_) => None,
        })
    }
}

impl Draft {
    /// The file with this name.
    pub(crate) fn file(&self, name: &str) -> Option<&DraftFile> {
        self.files.iter().find(|file| file.name == name)
    }
}
