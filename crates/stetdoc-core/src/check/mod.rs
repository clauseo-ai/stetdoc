//! Validity under the no-new-violations rule, and check reports.
//!
//! **Owns.** The rules that decide whether a document is valid (the schema's keys, the Word
//! rules, the package rules), the finding vocabulary of `api::check`, and the reports built from
//! them.
//!
//! **Interface.** `edit(&Tx, &Declared) -> Result<Verified, StetError>` for the commit pipeline,
//! `document(&PartsView) -> CheckReport` for `Document::check`, `scan_unit` and `scan_part` for the
//! Word rules of one unit and of a whole part, which the compiler's post-conditions use as well,
//! `unit_keys` for `compare`'s origins in another package, and `undeclared`, the prefixes a
//! markup-compatibility attribute of a part names and nothing declares, which `document` repairs.
//!
//! **Invariants.**
//! - No new violations: after an edit, the keys of every unit that differs from the base are a
//!   subset of the keys of its origin. Problems already present in the input are reported, not
//!   blocked.
//! - The units are found from the span tree's own dirty flags and an equality test against the
//!   base, never from the operations' declarations. Declarations only say where a new block came
//!   from, and a missing one makes `check` stricter. A unit that absorbed a unit the edit removed
//!   (the survivor of a join) carries that unit's keys without a declaration.
//! - Only the parts a transaction changed are examined before commit, and a part that `add_part`
//!   made, which is not a clone of the base's, is examined whole and has no origin.
//! - A key names what is wrong and never where, and an element that the compiler renames
//!   (`w:t` to `w:delText`) keeps the key of the problems it had.
//! - A finding has a stable code, and the same bytes give the same findings in the same order.
//!
//! **Depends on.** `api`, `package`, `xml`, `schema`, `wordml`, `state`, `threads`.
//!
//! **Start reading.** `check/mod.rs`, then `check/units.rs` and `check/edit.rs`; the Word rules
//! are in `check/rules/mod.rs`, and `check/document.rs` is `Document::check`.
//!
//! The design is in
//! [`docs/design/check.md`](../../../../docs/design/check.md).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        unused_imports,
        reason = "`document` calls `edit_located`, `document` and `schema_parts`. `edit` and `unit_keys` are for `compare`, which arrives with step 8 of the plan in docs/design/document-and-ops.md, and `scan_unit` is for `revisions::finish`, which still has its own copy of the Word rules"
    )
)]

mod audit;
mod census;
mod document;
mod edit;
mod examine;
mod fields;
mod found;
mod keys;
mod origins;
mod package;
mod prefixes;
mod report;
mod rules;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_accept;
#[cfg(test)]
mod tests_audit;
#[cfg(test)]
mod tests_census;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_edit;
#[cfg(test)]
mod tests_fields;
#[cfg(test)]
mod tests_foreign_parts;
#[cfg(test)]
mod tests_keys;
#[cfg(test)]
mod tests_origins;
#[cfg(test)]
mod tests_package;
#[cfg(test)]
mod tests_props_keys;
#[cfg(test)]
mod tests_rules;
#[cfg(test)]
mod tests_units;
mod units;
mod verdict;

pub(crate) use document::{document, schema_parts};
pub(crate) use edit::{Refusal, edit, edit_located};
pub(crate) use found::Found;
pub(crate) use keys::{Key, KeySet};
pub(crate) use origins::{Declared, Origin, part_keys, unit_keys};
pub(crate) use prefixes::undeclared;
pub(crate) use rules::{scan_part, scan_unit};
pub(crate) use verdict::{Checked, Verified};
