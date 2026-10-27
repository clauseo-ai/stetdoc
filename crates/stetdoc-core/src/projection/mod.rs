//! Rendering, selection, paging and parsing of edited files.
//!
//! **Owns.** The neutral model (`model`), the view pipeline (scope, mode, paging,
//! budget), the `formats/` seam, and `files`, which turns readable files into a batch.
//!
//! **Interface.** `view`, `render_files`, `render_blocks`, `lookup`, `render_lookup`, `occurrences`, `files::to_batch`, and
//! the `Format` trait with its registry (`stet/1` and `json/1`).
//!
//! **Invariants.**
//! - `projection::formats::Format` is the only trait in the core with more than one
//!   adapter.
//! - Formats render and parse the neutral model, and know syntax that nothing else in the
//!   core knows.
//! - `parse(render(x)) == draft(x)` for every format and every model: the draft is what the
//!   unedited files say, and an edit is the difference between two drafts.
//! - Output is deterministic and ordered by story and document position, for a given document and
//!   format version. Budgets use a character-based estimate, never a model tokenizer.
//! - Nothing an agent wrote is applied except as written or refused with its line, and `to_batch` on
//!   unedited files is empty.
//!
//! **Depends on.** `api`, `wordml`, `styles`, `numbering`, `refs`, `state`, `revisions`, `index`, `threads`.
//!
//! **Start reading.** `projection/model.rs`. The design notes are in
//! [`docs/design/projection-and-index.md`](../../../../docs/design/projection-and-index.md).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing outside this module uses the projection until `document` is built"
    )
)]

mod appendix;
mod authors;
mod build;
mod comments_view;
mod draft;
mod draft_build;
mod files;
mod formats;
mod keys;
mod ledger;
mod ledger_view;
mod lookup;
mod model;
mod model_inline;
mod model_region;
mod model_view;
mod needles;
mod notes;
mod occurrences;
mod outline;
mod page;
mod place;
mod receipt;
mod render;
mod select;
mod select_changes;
mod select_find;
mod view;

pub(crate) use {
    authors::authors, authors::note_ids, files::to_batch, ledger::LedgerFilter, ledger::changes_of,
    ledger::describe, ledger::family, ledger::story_name, lookup::lookup, lookup::render_lookup,
    occurrences::occurrences, receipt::render_blocks, render::render_files, view::view,
};

#[cfg(test)]
mod testing;
#[cfg(test)]
mod testing_gen;
#[cfg(test)]
mod testing_gen_blocks;
#[cfg(test)]
mod tests_clauses;
#[cfg(test)]
mod tests_comments;
#[cfg(test)]
mod tests_draft;
#[cfg(test)]
mod tests_find;
#[cfg(test)]
mod tests_find_hits;
#[cfg(test)]
mod tests_interface;
#[cfg(test)]
mod tests_json;
#[cfg(test)]
mod tests_ledger;
#[cfg(test)]
mod tests_model;
#[cfg(test)]
mod tests_notes;
#[cfg(test)]
mod tests_outline;
#[cfg(test)]
mod tests_page;
#[cfg(test)]
mod tests_place;
#[cfg(test)]
mod tests_scopes;
#[cfg(test)]
mod tests_view;
