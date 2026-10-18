//! The ref table: block to ref, namespaces, base tokens and tombstones.
//!
//! **Owns.** The ref table: the mapping between blocks and refs, ref namespaces, the lineage
//! that tells two histories of one namespace apart, tombstones and persistence.
//!
//! **Interface.** `RefTable::{open, resolve, ref_of, allocate, tombstone, rebind, merge,
//! persist, restore}` and `BaseToken`. Only `state` calls the methods that change the table
//! (`allocate`, `tombstone`, `rebind`, `merge`, `touch` and `advance`). They are `pub(crate)`
//! because Rust cannot name a sibling module in a visibility (`state` is a sibling of `refs`), so
//! nothing but review keeps `ops` or `projection`, which may read the table, from calling them.
//! `merge` and `tombstone` take the commit's `Merged` list, which is why the audit is the natural
//! caller.
//!
//! **Invariants.**
//! - Refs are assigned in document order at `open`. A ref follows its block's element and
//!   never changes within its namespace, and a number is never reused. A block that an edit
//!   adds gets its ref when the edit commits.
//! - A ref whose block left the document resolves to `stale_ref`, a ref that never existed
//!   to `not_found`, and a malformed ref to `invalid_payload`. A tracked-deleted block keeps
//!   its ref.
//! - A batch names the base token it was written against (namespace, state version and
//!   lineage). A token from another namespace or another history, and a block that changed
//!   after the token's version, resolve to `stale_ref`.
//! - The persisted form is canonical text. `restore` accepts only that exact form, aligned
//!   with a fresh scan of the head package, and answers `corrupt_complement` or
//!   `unsupported_version` to anything else.
//! - Refs are stored in the complement and never written into the package.
//!
//! **Depends on.** `api`, `wordml`.
//!
//! **Start reading.** `refs/table.rs`. The design notes are in
//! [`docs/design/refs-and-state.md`](../../../../docs/design/refs-and-state.md).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing outside this module uses the ref table until `state` and `document` are built"
    )
)]

mod base;
mod digest;
mod errors;
mod layout;
mod persist;
mod restore;
mod table;
mod token;

pub(crate) use {
    base::{BaseToken, Claim, NamespaceId, NamespaceOrigin},
    digest::{Hasher, hex, sha256, unhex},
    layout::{BlockSpec, Layout, StoryBlocks},
    table::{Merged, Reason, RefTable},
    token::{BlockRef, RefKind},
};
// `BlockKey` is the key of a block, and `wordml` defines it.
pub(crate) use crate::wordml::BlockKey;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_merge;
#[cfg(test)]
mod tests_persist;
#[cfg(test)]
mod tests_restore;
