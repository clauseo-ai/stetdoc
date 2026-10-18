//! Every revision shape Word writes, plus accept and reject.
//!
//! **Owns.** The compiler that turns edits into tracked changes, or applies them directly: the revision
//! shapes it writes (insertions, deletions, paragraph marks, the `*PrChange` family, rows), stamping, the
//! word-level difference of a text edit, list instances, and the placing of range markers. Also the grouping
//! of revision elements into changes, and the engine that accepts and rejects them.
//!
//! **Interface.** `Compiler::{new, replace_text, set_props, insert_block, insert_block_in, delete_block,
//! delete_block_with_mark, split_block, merge_blocks, insert_row, new_list_instance, place_markers,
//! place_markers_beside, remove_markers, remove_markers_in_part, parts_mut, tracked,
//! remove_unreferenced_notes, finish}`, and for compare `redline_paragraph`, `mark_inserted`, `place_deleted`,
//! `set_snapshot`, `add_deleted_note`, `split_block_untracked` and `import_writer` (design `compare.md`, 6.1 and 20)
//! over a `Tx`, with `Track`,
//! `RevStamp`, `Effect` and `Outcome`; `resolve(tx, &mut ChangeIndex, &[Step])`;
//! `changes(story, tree, range)`; `notes::remove_unreferenced_notes`; and `diff::edits`.
//!
//! **Invariants.**
//! - Operations, `Edit::files`, comment anchors, the editor and compare all end up as `Compiler` calls on a
//!   `Tx`. There is never a second route.
//! - A revision that existed before the transaction is never edited, extended or re-attributed. It can be
//!   split, nested into, have its own author's text removed from it, or be resolved.
//! - Another author's pending change is never rewritten or hidden: a tracked edit records what it does to it or
//!   is refused, and an untracked edit that would reach into it is `readonly_region` (`foreign.rs`). Untracked
//!   means the author's own changes go untracked (`Track::UntrackedBy`).
//! - New revision ids come from `Tx::next_annotation_id`. A date is `YYYY-MM-DDTHH:MM:SSZ`, the wall-clock
//!   time of the date the caller gave, validated and never converted. No `w:rsid*` attribute is added,
//!   removed or rewritten.
//! - Every copy of properties, a run or a row loses its change-tracking elements and its section break. The
//!   only exception is the halves of a carve, which keep a pending change under a new id.
//! - Every child written into a property container goes through `schema::insert_in_order`, and every
//!   removal is followed by `schema::reorder`.
//! - Text with white space at an end says `xml:space="preserve"` unless an ancestor does (XML inherits it), and a text edit
//!   never deletes a note's own number.
//! - A replace is the word-level difference, cleaned, deletion before insertion, applied right to left over
//!   one text map. A tracked edit never deletes a paragraph's runs to write the same text again.
//! - Accept and reject are selective and structural. A change is the unit, a move and a field are resolved
//!   whole, a range marker is hoisted and never deleted, and a join keeps the first paragraph's element and
//!   ref with the later paragraph's properties.
//! - What a call minted, split, removed, folded or swallowed is in its `Effect`. What a transaction created
//!   is derived from the ids it minted, and only an element it created is ever extended or edited in place.
//!
//! **Depends on.** `api`, `xml`, `schema`, `wordml`, `state`, `check` (the Word rules that the compiler's
//! post-conditions run on what a transaction created).
//!
//! **Start reading.** `revisions/compiler.rs`, then `revisions/text.rs`. The
//! design is in [`docs/design/revisions.md`](../../../../docs/design/revisions.md).

#![expect(
    dead_code,
    reason = "the compiler, the resolver, `ops` and `compare` build on these pieces, and none of them exists yet"
)]

mod blocks;
mod blocks_delete;
mod blocks_insert;
mod carve;
pub(crate) mod changes;
mod changes_marks;
mod changes_rows;
mod changes_text;
mod cleanup;
pub(crate) mod compiler;
pub(crate) mod diff;
mod errors;
mod foreign;
mod gap;
pub(crate) mod handle;
pub(crate) mod import_writer;
pub(crate) mod index;
mod insert_run;
mod join;
mod lists;
mod markers;
mod moves;
mod names;
pub(crate) mod notes;
pub(crate) mod outcome;
pub(crate) mod policy;
mod props;
mod props_edit;
mod redline_blocks;
pub(crate) mod redline_ends;
mod redline_grid;
mod redline_notes;
mod redline_split;
mod redline_text;
pub(crate) mod redline_types;
mod redline_wrap;
pub(crate) mod resolve;
mod resolve_control;
mod resolve_mark;
mod resolve_props;
mod resolve_run;
mod resolve_select;
mod resolve_table;
mod snapshot;
pub(crate) mod stamp;
mod tables;
mod text;
mod text_apply;
mod tree_ops;
pub(crate) mod types;
mod verify;
mod wrap;

#[cfg(test)]
mod laws;
#[cfg(test)]
mod laws_checks;
#[cfg(test)]
mod laws_gen;
#[cfg(test)]
mod laws_norm;
#[cfg(test)]
mod laws_refs;
#[cfg(test)]
mod laws_select;
#[cfg(test)]
mod laws_util;
#[cfg(test)]
mod tests_blocks;
#[cfg(test)]
mod tests_blocks_copy;
#[cfg(test)]
mod tests_blocks_delete;
#[cfg(test)]
mod tests_blocks_place;
#[cfg(test)]
mod tests_blocks_run;
#[cfg(test)]
mod tests_blocks_wrapped;
#[cfg(test)]
mod tests_calibrate;
#[cfg(test)]
mod tests_catalog;
#[cfg(test)]
mod tests_change_text;
#[cfg(test)]
mod tests_changes;
#[cfg(test)]
mod tests_changes_window;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_diff;
#[cfg(test)]
mod tests_diff_props;
#[cfg(test)]
mod tests_laws;
#[cfg(test)]
mod tests_lists;
#[cfg(test)]
mod tests_markers;
#[cfg(test)]
mod tests_notes;
#[cfg(test)]
mod tests_origins;
#[cfg(test)]
mod tests_oxpt;
#[cfg(test)]
mod tests_pandoc;
#[cfg(test)]
mod tests_props;
#[cfg(test)]
mod tests_props_dups;
#[cfg(test)]
mod tests_props_rand;
#[cfg(test)]
mod tests_props_tables;
#[cfg(test)]
mod tests_redline;
#[cfg(test)]
mod tests_redline_blocks;
#[cfg(test)]
mod tests_redline_law;
#[cfg(test)]
mod tests_refs;
#[cfg(test)]
mod tests_resolve;
#[cfg(test)]
mod tests_resolve_more;
#[cfg(test)]
mod tests_resolve_rows;
#[cfg(test)]
mod tests_rows;
#[cfg(test)]
mod tests_scenarios;
#[cfg(test)]
mod tests_select;
#[cfg(test)]
mod tests_stamp;
#[cfg(test)]
mod tests_support;
#[cfg(test)]
mod tests_text;
#[cfg(test)]
mod tests_text_edges;
#[cfg(test)]
mod tests_text_more;
#[cfg(test)]
mod tests_text_rand;
#[cfg(test)]
mod tests_wrapped_edits;
