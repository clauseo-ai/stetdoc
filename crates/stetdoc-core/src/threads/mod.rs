//! Comments and their threads, read from the comment parts and the stories.
//!
//! **Owns.** The reading of `comments.xml` and the parts that extend it (`commentsExtended`, `commentsIds`,
//! `commentsExtensible`, `people`): each comment with its author, date, parent, resolved flag and durable id;
//! the threads; where each comment is anchored in the stories; and an audit of how consistent the parts are.
//! It never writes. The `comments` module writes comments and calls this one.
//!
//! **Interface.** `Threads::{of, comments, threads, comment, thread_of, covered_text, counts}` and
//! `audit(&PartsView) -> Vec<Finding>`.
//!
//! **Invariants.**
//! - Total and read-only: any document that opens reads, and a comment part that is missing, empty or
//!   inconsistent gives fewer comments or more findings, never an error.
//! - A thread is a comment and every comment whose chain of parents reaches it. It is resolved when its
//!   root's `w15:done` is set. A chain that loops is cut at the member that comes first in the file.
//! - The walks are linear in the number of comments, whatever the shape of the chains (`chains.rs`), and a
//!   duplicated `w:id` never makes one climb by id.
//! - A comment is keyed in the extension parts by the `w14:paraId` of its last paragraph.
//! - `Threads::of` is cached under the generation of every story part and every comment part, and of the
//!   package, so a view over a `Tx` sees the staged comments.
//! - `audit` gives findings with stable codes and a class, so `check` can compare the findings before and
//!   after a batch.
//!
//! **Depends on.** `api`, `xml`, `schema`, `wordml`, `state`.
//!
//! **Start reading.** `threads/read.rs`. Design: `docs/design/comments-and-notes.md`.

mod anchors;
mod audit;
mod chains;
mod model;
mod parts;
mod read;
mod rows;
#[cfg(test)]
mod tests_audit;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_read;
#[cfg(test)]
pub(crate) mod tests_support;

#[expect(
    unused_imports,
    reason = "the interface that `projection`, `check` and `ops` build on, and not all of them name every type yet"
)]
pub(crate) use {
    audit::{Class, Finding, audit},
    model::{Anchor, Comment, Counts, Thread},
    parts::{CommentParts, is_kind},
    read::Threads,
    rows::{NS_CEX, NS_CID, NS_W14, NS_W15, NS_W15_PREVIEW, is_w15, para_id, w15_attr},
};
