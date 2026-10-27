//! The committed State and the copy-on-write Tx.
//!
//! **Owns.** The committed `State`, the copy-on-write `Tx` that stages edits, the audit that
//! keeps the ref table true to the trees at commit, and the derived caches.
//!
//! **Interface.** `State::{open, resume, export, parts, resolve, opened_bytes, opened_source, cascade, numbers}`, `PartsView`
//! (with `content_type`),
//! and `Tx::{part_mut, add_part, add_relationship, resolve, touch, retire, rebind, merge, mint_id,
//! next_annotation_id, minted_annotation_ids, note_copied_annotation_id, copied_annotation_ids,
//! changed_parts, edited_clones, commit, discard}`.
//! `cascade` and `numbers` are cached accessors over `PartsView::derived` (`cascade.rs`), with
//! `PartsView::{cascade_deps, numbers_deps}` as the lists they are cached under and `PartsView::{sites,
//! site}` for the sites the cascade answers questions about (`sites.rs`).
//!
//! **Invariants.**
//! - Each part is copied on first write, untouched parts are shared, and the committed
//!   `State` is never written to. A `Tx` is an overlay: commit builds the next `State`, and
//!   on discard or an error the `State` is unchanged: its parts, refs, token and version, and
//!   so its `export`. The one thing a commit writes to the base is a cache. The audit refreshes
//!   the stories it scans from the base's, so it reads the base's story scan and field pass
//!   through the base's own caches, and a commit that fails or is a dry run leaves them filled.
//!   A cache holds only what the first read of it would build from the committed parts, so
//!   filling one changes nothing that a caller can see.
//! - Commit audits the staged parts against the ref table, extends the lineage, bumps the
//!   state version, and keeps the derived caches that were built from parts that did not
//!   change.
//! - A derived cache records the generation of every part it was built from, and is rebuilt
//!   on the next read when one has changed. Nothing is invalidated, and an entry built in a
//!   `Tx` reaches the committed `State` only at commit.
//! - New identifiers come from the namespace, the version and the position, never from a
//!   clock or a random number generator.
//!
//! **Depends on.** `api`, `package`, `xml`, `wordml`, `refs`, `styles`, `numbering`.
//!
//! **Start reading.** `state/mod.rs`. The design notes are in
//! [`docs/design/refs-and-state.md`](../../../../docs/design/refs-and-state.md).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "nothing outside this module uses the state until `document` and `ops` are built"
    )
)]

mod audit;
mod cascade;
mod commit;
mod derived;
mod errors;
mod export;
mod failpoint;
mod ids;
mod intents;
mod open;
mod parts;
mod sites;
mod stories;
#[cfg(test)]
pub(crate) use stories::story_builds;
mod tx;
mod view;

use crate::api::limits::Limits;
use crate::package::Package;
use crate::xml::Bytes;
// `Tx::add_part` takes it, and the modules above `state` that add a part name it through here.
pub(crate) use crate::package::NewPart;
use crate::refs::{BaseToken, BlockKey, Claim, RefTable};
use derived::Derived;
use errors::{internal, stale_files};
use parts::{PartIndex, PartSlot};
use std::cell::RefCell;
use std::sync::Arc;

use crate::api::error::StetError;

#[cfg_attr(
    not(test),
    expect(
        unused_imports,
        reason = "the interface that `ops`, `revisions`, `check` and `document` build on"
    )
)]
pub(crate) use {
    commit::Committed,
    derived::Dep,
    export::{Exported, Recorded},
    ids::IdSet,
    intents::{Intent, Retired},
    parts::PartId,
    sites::{RunKey, SiteHandle},
    stories::StoryList,
    tx::Tx,
    view::PartsView,
};

/// A committed state of a document. It is never written to: a `Tx` stages edits over it and commit
/// builds the next one, sharing every part that was not changed.
#[derive(Clone)]
pub(crate) struct State {
    package: Package,
    limits: Limits,
    index: Arc<PartIndex>,
    slots: Vec<PartSlot>,
    package_epoch: u64,
    /// The main document part, whose number never changes.
    main: PartId,
    refs: RefTable,
    /// Interior mutability, so that a read through `&State` can parse a part and cache a derived
    /// value. It is also why a `State`, and so a `Document`, is `Send` and not `Sync`.
    derived: RefCell<Derived>,
}

impl State {
    /// A state over an opened package, with the given ref table. No part is parsed.
    fn new(package: Package, limits: &Limits, refs: RefTable) -> Result<State, StetError> {
        // A usable package has content types, root relationships, and a main document part. The
        // container reads packages that lack them, because it keeps any ZIP byte for byte.
        for (name, why) in [
            ("[Content_Types].xml", "so no part has a content type"),
            (
                "_rels/.rels",
                "so nothing says which part is the main document",
            ),
        ] {
            if !package.contains(name) {
                return Err(StetError::new(
                    crate::api::error::ErrorCode::MalformedInput,
                    format!("The package has no {name}, {why}."),
                ));
            }
        }
        let index = PartIndex::new(package.part_names())?;
        let main = package
            .main_part()
            .and_then(|name| index.id(name))
            .ok_or_else(|| {
                StetError::new(
                    crate::api::error::ErrorCode::MalformedInput,
                    "the package has no main document part",
                )
            })?;
        // Read the content types now. A package whose `[Content_Types].xml` is not XML is not one that
        // Word opens, and the first `add_part` would fail on it, long after the document was opened.
        if let Some(name) = index.name(main) {
            package.content_type(name)?;
        }
        let slots = index.names().cloned().map(PartSlot::unparsed).collect();
        Ok(State {
            package,
            limits: limits.clone(),
            index: Arc::new(index),
            slots,
            package_epoch: 0,
            main,
            refs,
            derived: RefCell::new(Derived::new()),
        })
    }

    /// Read access to the committed parts and derived values.
    pub(crate) fn parts(&self) -> PartsView<'_> {
        PartsView {
            base: self,
            tx: None,
        }
    }

    fn slot(&self, id: PartId) -> Option<&PartSlot> {
        self.slots.get(id.index())
    }

    /// The token of this state: the namespace, the version and the lineage. Views, receipts and
    /// readable files carry it.
    pub(crate) fn base_token(&self) -> BaseToken {
        self.refs.base_token()
    }

    /// The block a ref names, on the read path. `Claim::Implied` is accepted here, for `inspect` and
    /// for the operations that `Edit::files` derives. An edit goes through `Tx::resolve`.
    pub(crate) fn resolve(&self, claim: Claim<'_>, token: &str) -> Result<BlockKey, StetError> {
        self.refs.resolve(claim, token)
    }

    /// Checks that readable files were written for this state: the namespace, the history and the
    /// version all equal the state's. Anything else is `stale_files`, and the message says which
    /// differs. It is the only comparison, so no caller can forget a case.
    pub(crate) fn check_files_base(&self, recorded: &BaseToken) -> Result<(), StetError> {
        let now = self.refs.base_token();
        if recorded.namespace() != now.namespace() {
            return Err(stale_files(format!(
                "These files were exported from another document (namespace {}, not {}). Export \
                 the document again.",
                recorded.namespace(),
                now.namespace()
            )));
        }
        if recorded.version() > now.version() {
            return Err(stale_files(format!(
                "These files were exported at version {} of a document that is at version {}, so \
                 they come from another copy. Export the document again.",
                recorded.version(),
                now.version()
            )));
        }
        if self.refs.lineage_at(recorded.version()) != Some(recorded.lineage()) {
            return Err(stale_files(
                "These files were exported from a different history of this document: another \
                 working copy, or another session opened from the same file. Export the document \
                 again.",
            ));
        }
        if recorded.version() < now.version() {
            return Err(stale_files(format!(
                "These files were exported at version {}, and the document is at version {}. \
                 Export the document again.",
                recorded.version(),
                now.version()
            )));
        }
        Ok(())
    }

    /// The size in bytes of the package as it was opened (or resumed), which no commit changes:
    /// for the limits and receipts of the layers above.
    pub(crate) fn opened_bytes(&self) -> usize {
        self.package.opened_len()
    }

    /// The bytes the state's package was read from, as a shared handle that no commit changes: the input of `open`, or the head of
    /// a resumed complement. `document` keeps it as the original of a document it opened, without copying it.
    pub(crate) fn opened_source(&self) -> Bytes {
        self.package.opened()
    }

    /// The number of the main part, for callers inside this module.
    fn main_part(&self) -> Result<PartId, StetError> {
        self.slot(self.main)
            .map(|_| self.main)
            .ok_or_else(|| internal("the main part has no slot"))
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_add;
#[cfg(test)]
mod tests_addmany;
#[cfg(test)]
mod tests_annot;
#[cfg(test)]
mod tests_atomic;
#[cfg(test)]
mod tests_cascade;
#[cfg(test)]
mod tests_cascade_corpus;
#[cfg(test)]
mod tests_claims;
#[cfg(test)]
mod tests_coherence;
#[cfg(test)]
mod tests_context;
#[cfg(test)]
mod tests_corpus;
#[cfg(test)]
mod tests_corpus_docs;
#[cfg(test)]
mod tests_deleted;
#[cfg(test)]
mod tests_derived;
#[cfg(test)]
mod tests_edit;
#[cfg(test)]
mod tests_export;
#[cfg(test)]
mod tests_fixtures;
#[cfg(test)]
mod tests_ids;
#[cfg(test)]
mod tests_merge;
#[cfg(test)]
mod tests_numbers_corpus;
#[cfg(test)]
mod tests_oracle;
#[cfg(test)]
mod tests_package_api;
#[cfg(test)]
mod tests_probe;
#[cfg(test)]
mod tests_sites;
#[cfg(test)]
mod tests_tables;
