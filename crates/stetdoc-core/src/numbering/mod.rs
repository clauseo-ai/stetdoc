//! Clause numbers, as a pure computation.
//!
//! **Owns.** Clause numbers: a pure computation over numbering definitions and styles,
//! covering restarts, overrides and style links.
//!
//! **Interface.** `StoryNumbers::{build, clause_number, full_context, notes_at}` and `level_site`.
//!
//! **Invariants.**
//! - Pure: the same facts, definitions and mode always give the same table of clause numbers.
//! - Clause numbers are read-only in projections and are never stored as text.
//!
//! **Depends on.** `api`, `wordml`, `styles`.
//!
//! **Start reading.** `numbering/counters.rs`. Design: `docs/design/styles-and-numbering.md`.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`state` builds the numbers and nothing reads them until `projection`, `index` and `compare` are built"
    )
)]

mod bind;
mod counters;
mod ctx;
mod format;
mod item;
mod label;
mod site;
#[cfg(test)]
mod tests_counters;
#[cfg(test)]
mod tests_format;
#[cfg(test)]
mod tests_modes;
#[cfg(test)]
mod tests_register;
#[cfg(test)]
mod tests_site;
#[cfg(test)]
mod tests_support;

#[expect(
    unused_imports,
    reason = "the interface that `state`, `projection`, `index` and `compare` build on, and not all of them name every type yet"
)]
pub use {
    counters::StoryNumbers,
    item::{ItemKind, ListId, ListItem, Note},
    site::level_site,
};
