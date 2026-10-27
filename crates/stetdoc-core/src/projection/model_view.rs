//! The parts of a model that only a view has: margin notes, the outline and the appendix.
//!
//! A file never holds any of them, because a file is edited and something that is not text would be a
//! read-only thing to keep in step. A page of a view holds margin notes after the blocks they are about,
//! an outline is a body of its own, and the appendix is the notes and comments that a page cites (design 4).

use super::model::NoteKind;
use crate::refs::BlockRef;
use std::sync::Arc;

/// A line in the margin of a page, after the block it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Margin {
    /// The block that precedes this line holds a find's needle this many times.
    Matches(u32),
    /// The block that precedes this line holds these needles of a find that has more than one, this many times each.
    Terms(Vec<TermCount>),
    /// This many blocks between the ones shown are not shown.
    Gap(u32),
    /// The table that follows was cut between rows on the page before: it goes on at this row.
    Continued {
        /// The table.
        table: BlockRef,
        /// The first row shown.
        from: BlockRef,
    },
    /// The block that precedes this line cites a clause in words, and `index` says what it came to.
    Mention {
        /// The citation as written, such as `clause 4.2(a)`.
        text: Box<str>,
        /// How it came out.
        state: MentionState,
        /// What it may mean, nearest to the citing block first: one when it resolved, several when it is
        /// ambiguous, the clauses of that number in this document when it is outside, and none when it is not
        /// found.
        targets: Vec<Target>,
        /// How many more candidates there are than are listed.
        more: u32,
    },
    /// The clause that precedes this line is cited by these blocks (a clause view only).
    CitedBy {
        /// The citing blocks, in document order.
        from: Vec<BlockRef>,
        /// How many more cite it than are listed.
        more: u32,
    },
}

/// How a mention of a clause in the text came out (design 7.4). `Relative` and `Outside` are never `Resolved`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MentionState {
    /// One clause.
    Resolved,
    /// One clause, found from the clauses around the citing block: the weakest kind of answer.
    Relative,
    /// Several clauses.
    Ambiguous,
    /// A citation of a law, or of something that is not this document.
    Outside,
    /// Nothing in the document has that number.
    Unresolved,
}

/// A clause a mention may mean: where it is, and enough of it to recognize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    /// The clause's block.
    pub r#ref: BlockRef,
    /// Its label and the start of its text.
    pub text: Box<str>,
}

/// Where the headings of an outline came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutlineSource {
    /// The effective outline level of a paragraph's style.
    Styles,
    /// Short paragraphs at the first level of a list.
    Numbered,
    /// Short bold lines that look like headings.
    BoldLines,
    /// No headings: runs of blocks of about the same size.
    Chunks,
}

/// The headings of a story, to a depth, with what each covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Outline {
    /// Where the headings came from.
    pub source: OutlineSource,
    /// The deepest level shown.
    pub depth: u32,
    /// How many entries the whole outline has at every level.
    pub total: u32,
    /// The entries shown, in order.
    pub entries: Vec<OutlineEntry>,
}

/// One heading and what it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OutlineEntry {
    /// The heading's block.
    pub r#ref: Option<BlockRef>,
    /// The level, from 1.
    pub level: u32,
    /// The number Word shows for the heading, if it is numbered.
    pub label: Option<Box<str>>,
    /// The heading's text, cut short.
    pub text: Box<str>,
    /// How many top-level blocks it covers, itself included.
    pub blocks: u32,
    /// How many characters of text it covers.
    pub chars: u32,
    /// The first block it covers.
    pub first: Option<BlockRef>,
    /// The last block it covers.
    pub last: Option<BlockRef>,
    /// How many entries under it are not shown.
    pub hidden: u32,
}

/// The comment threads of a `Comments` view, one entry per thread, in the order of their roots (design 4.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadList {
    /// How many threads the filter keeps, on every page.
    pub total: u32,
    /// How many of those are resolved.
    pub resolved: u32,
    /// The threads shown.
    pub threads: Vec<ThreadEntry>,
}

/// One thread: its root, its replies and whether it is resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadEntry {
    /// The `w:id` of the root.
    pub root: i64,
    /// The root's `w15:done`, which is Word's reading of the thread.
    pub resolved: bool,
    /// The root first, then its replies in the order of `comments.xml`.
    pub comments: Vec<ThreadComment>,
}

/// One comment of a thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ThreadComment {
    /// The `w:id`.
    pub id: i64,
    /// The author, an index into the authors of the head.
    pub author: Option<u32>,
    /// The date as written.
    pub date: Option<Arc<str>>,
    /// The first block it is anchored on.
    pub on: Option<BlockRef>,
    /// The text it is anchored to, cut short.
    pub anchored: Option<Box<str>>,
    /// The comment's text, without deletions, its paragraphs joined by ` / `.
    pub text: Box<str>,
}

/// The tracked changes of a `Revisions` view: how many the filter keeps, and the ones on this page, in ledger order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangeList {
    /// How many changes the filter keeps, on every page.
    pub total: u32,
    /// The changes shown, as `Document::revisions` describes them.
    pub changes: Vec<crate::api::revision::RevisionInfo>,
    /// Show the whole text of each change, and do not cut its line short.
    pub full: bool,
}

/// One needle of a find, and how many times a block holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TermCount {
    /// The needle as it was given.
    pub term: Box<str>,
    /// How many times the block holds it.
    pub count: u32,
}

/// The compact page of a `Find` with no context: one line for each paragraph that matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HitList {
    /// How many needles the find has. With more than one, each line names those it holds.
    pub needles: u32,
    /// The paragraphs on this page, in document order.
    pub hits: Vec<FindHit>,
    /// The clause headings that matched, with how to read each whole.
    pub tips: Vec<Tip>,
}

/// One paragraph that a find matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FindHit {
    /// The paragraph.
    pub r#ref: BlockRef,
    /// The outermost clause it is in, as a heading line over the hits under it.
    pub under: Option<Target>,
    /// The full label of the clause it is in, such as `4.2(a)`.
    pub clause: Option<Box<str>>,
    /// The text around the first match, exactly as the paragraph has it.
    pub text: Box<str>,
    /// Text of the paragraph comes before `text` and is not shown.
    pub cut_before: bool,
    /// Text of the paragraph comes after `text` and is not shown.
    pub cut_after: bool,
    /// The needles it holds, each with how many times.
    pub terms: Vec<TermCount>,
    /// The needles are only in text that a tracked change deletes, so `text` is the start of the paragraph.
    pub deleted_only: bool,
}

/// A clause heading that a find matched: the clause to view whole, and how to name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Tip {
    /// The heading.
    pub r#ref: BlockRef,
    /// The clause as a view or a lookup takes it, such as `clause 11`.
    pub clause: Box<str>,
    /// The heading's text, cut short.
    pub text: Box<str>,
}

/// A note or a comment that a page cites, with its text cut short.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum AppendixEntry {
    /// A footnote or an endnote.
    Note {
        /// Footnote or endnote.
        kind: NoteKind,
        /// The number the page shows.
        ordinal: u32,
        /// The note's text.
        text: Box<str>,
        /// The text was cut, and the rest is in `notes.md`.
        cut: bool,
    },
    /// A comment.
    Comment {
        /// The comment's `w:id`.
        id: i64,
        /// The author, an index into the authors of the head.
        author: Option<u32>,
        /// The date as written.
        date: Option<Arc<str>>,
        /// The block it is anchored on.
        on: Option<BlockRef>,
        /// The text it is anchored to.
        anchored: Option<Box<str>>,
        /// The comment's text.
        text: Box<str>,
        /// The text was cut, and the rest is in `comments.md`.
        cut: bool,
    },
}
