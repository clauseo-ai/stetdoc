//! The style cascade and the hidden-text rules.
//!
//! **Owns.** The style cascade, effective formatting with the source of each value, and
//! the hidden-text rules.
//!
//! **Interface.** `Cascade::{effective, hidden, looks_hidden}`, and the reads that build a `Site`
//! (`direct_paragraph`, `direct_run`, `cell_fill`, `page_background`).
//!
//! **Invariants.**
//! - The cascade order is document defaults, table style, numbering level, paragraph
//!   style, character style, direct formatting.
//! - Hidden-text rules live here, below the cascade they need. Formats render `Hidden`
//!   generically.
//!
//! **Depends on.** `api`, `xml`, `wordml`.
//!
//! **Start reading.** `styles/cascade.rs`. Design: `docs/design/styles-and-numbering.md`.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "`state` builds the cascade and the sites, and nothing asks them questions until `projection`, `ops` and `index` are built"
    )
)]

mod bag;
mod cascade;
mod effective;
mod hidden;
mod resolve;
mod site;
mod source;
mod tables;
#[cfg(test)]
mod tests_agree;
#[cfg(test)]
mod tests_cascade;
#[cfg(test)]
mod tests_hidden;
#[cfg(test)]
mod tests_site;
#[cfg(test)]
mod tests_support;
#[cfg(test)]
mod tests_tables_theme;
mod theme;
mod toggle;

#[expect(
    unused_imports,
    reason = "the interface that `state`, `numbering`, `projection`, `ops` and `index` build on, and not all of them name every type yet"
)]
pub use {
    bag::Entry,
    cascade::{Cascade, StyleNumbering},
    effective::{Effective, FontSlot},
    hidden::{HiddenReason, LooksHidden, MarkEffect},
    site::{
        DirectParagraph, LevelSite, RunSite, Site, cell_fill, direct_paragraph, direct_run,
        numbering_removed, page_background,
    },
    source::Source,
    theme::{Rgb, Theme},
};
