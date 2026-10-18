//! The plain data that `open`, `persist` and `restore` take: the blocks of a document, story by
//! story, in document order.
//!
//! `state` builds it from the scanned stories. It is the only thing `refs` knows about a document,
//! so the table never touches a tree.

use crate::wordml::{BlockKey, BlockKind, StoryKind};
use std::sync::Arc;

/// One block of a story.
#[derive(Clone, Debug)]
pub(crate) struct BlockSpec {
    /// The block's identity in this state.
    pub(crate) key: BlockKey,
    /// What the block is.
    pub(crate) kind: BlockKind,
    /// The fingerprint of its text, which `persist` writes and `restore` checks. `open` ignores it.
    pub(crate) fingerprint: u64,
}

/// The blocks of one story, in the order of `Story::blocks`.
#[derive(Clone, Debug)]
pub(crate) struct StoryBlocks {
    /// The part's canonical name.
    pub(crate) part: Arc<str>,
    /// What kind of story it is.
    pub(crate) kind: StoryKind,
    /// The blocks, in document order.
    pub(crate) blocks: Vec<BlockSpec>,
}

/// The stories of a document in story order, each with its blocks.
#[derive(Clone, Debug, Default)]
pub(crate) struct Layout {
    /// The stories, in the order `wordml::stories` lists them.
    pub(crate) stories: Vec<StoryBlocks>,
}
