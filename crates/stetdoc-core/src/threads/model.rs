//! The plain data of the thread reader: a comment, where it is anchored, a thread and the counts.

use crate::wordml::{BlockKey, Loc};
use std::sync::Arc;

/// Where a comment is anchored in the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Anchor {
    /// The part that holds the markers. `None` when the comment has no marker at all.
    pub(crate) story: Option<Arc<str>>,
    /// The `w:commentRangeStart`, if there is one.
    pub(crate) start: Option<Loc>,
    /// The `w:commentRangeEnd`, if there is one.
    pub(crate) end: Option<Loc>,
    /// The run with the `w:commentReference`, if there is one.
    pub(crate) reference: Option<Loc>,
    /// The paragraphs the range touches, first to last. It is empty for a comment with no marker, and
    /// for a range whose markers both sit between blocks.
    pub(crate) blocks: Vec<BlockKey>,
}

impl Anchor {
    /// True if the comment has no range marker and no reference: Word shows no such comment.
    pub(crate) fn is_unanchored(&self) -> bool {
        self.start.is_none() && self.end.is_none() && self.reference.is_none()
    }
}

/// One comment, as `comments.xml` and the extension parts describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Comment {
    /// The `w:id`.
    pub(crate) id: i64,
    /// The index of the comment in the comments story's `units()`, so that a caller can find its blocks.
    pub(crate) unit: usize,
    /// The `w:author`, as written.
    pub(crate) author: Option<String>,
    /// The `w:initials`, as written.
    pub(crate) initials: Option<String>,
    /// The `w:date`, as written.
    pub(crate) date: Option<String>,
    /// The `w14:paraId` of the comment's last paragraph, which keys its rows in the extension parts.
    pub(crate) para_id: Option<u32>,
    /// The id of the parent comment, if the row names one that exists and its `w:id` is unique. A parent
    /// that would close a loop is dropped, and so is one whose id two comments share (`audit` says so), so
    /// climbing by id always ends and never lands on the wrong comment. The thread of a comment is exact
    /// either way: read `Threads::thread_of`.
    pub(crate) parent: Option<i64>,
    /// This comment's own `w15:done`.
    pub(crate) resolved: bool,
    /// The `w16cid:durableId`, if `commentsIds.xml` has a row for the comment.
    pub(crate) durable_id: Option<u32>,
    /// Where the comment is anchored.
    pub(crate) anchor: Anchor,
}

/// A comment and the comments that hang below it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Thread {
    /// The id of the comment that has no parent.
    pub(crate) root: i64,
    /// The ids of every comment below the root, in the order of `comments.xml`.
    pub(crate) replies: Vec<i64>,
    /// True if the root's `w15:done` is set. This is Word's reading of a thread.
    pub(crate) resolved: bool,
}

/// How many comments a document has, for a summary line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    /// Every comment, replies included.
    pub(crate) comments: usize,
    /// Threads.
    pub(crate) threads: usize,
    /// Threads whose root is resolved.
    pub(crate) resolved_threads: usize,
    /// Comments with no marker and no reference.
    pub(crate) unanchored: usize,
}
