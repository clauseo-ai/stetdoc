//! The notes that the import copied, added once their reference is in the story (design `compare.md`, 7.5 and 20).

use super::build::Builder;
use crate::api::error::StetError;
use crate::revisions::redline_types::Imported;

impl Builder<'_, '_, '_> {
    /// The paragraphs of the notes that import brought in are marked deleted: a note and its reference are written together.
    /// A note none of whose copies is in the story (they were abandoned for a fallback) is not added, so that no note is left
    /// without a reference.
    pub(super) fn flush_notes(&mut self) -> Result<(), StetError> {
        for note in self.imp.take_new_notes() {
            if !self.holds_reference(&note) {
                self.imp.forget_note(&note);
                continue;
            }
            let blocks: Vec<Imported> = note
                .paragraphs
                .iter()
                .map(|c| Imported {
                    node: c.node,
                    source: c.source.clone(),
                })
                .collect();
            // a refusal writes nothing and leaves the note out; a failure after the call has written is a defect
            if let Err(e) = self
                .c
                .check_deleted_note(&note.part, note.endnote, note.id, &blocks)
            {
                let original = blocks.iter().map(|b| b.source.clone()).collect();
                self.exclude(
                    Vec::new(),
                    original,
                    "not_restored",
                    format!("a note of deleted content was not added: {e}"),
                );
                continue;
            }
            match self
                .c
                .add_deleted_note(&note.part, note.endnote, note.id, &blocks)
            {
                Ok(fx) => self.record(&fx),
                Err(e) => match blocks.first() {
                    Some(b) => self.broken(Self::key(&note.part, b.node), &e),
                    None => self.rc.broken += 1,
                },
            }
        }
        Ok(())
    }
}
