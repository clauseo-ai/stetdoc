//! `split_block_untracked`: the cut that `compare` makes in a revised paragraph before it deletes the mark between the parts,
//! tracked (design `compare.md`, 5.5 and 6.1).
//!
//! A merge of several original paragraphs into one is written as the revised paragraph cut where they met, untracked, and the
//! marks between the parts deleted, tracked. The cut is `split_block` as an untracked compiler writes it, with this compiler's
//! effects and ids, so that one compiler writes the whole redline and `finish` sees every call.

use super::compiler::Compiler;
use super::outcome::Effect;
use super::stamp::Track;
use crate::api::error::StetError;
use crate::wordml::BlockKey;

impl Compiler<'_, '_> {
    /// Splits `block` at the character `at` of its `Final` text without a revision: the new paragraph after the cut has a
    /// copy of the mark's properties, and neither mark is tracked.
    pub(crate) fn split_block_untracked(
        &mut self,
        block: &BlockKey,
        at: usize,
    ) -> Result<Effect, StetError> {
        let track = std::mem::replace(&mut self.track, Track::Untracked);
        let res = self.split_block_as(block, at, false);
        self.track = track;
        res
    }
}
