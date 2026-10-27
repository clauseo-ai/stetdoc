//! `Tx`: a private overlay on a committed `State`, with copy-on-write staging per part.
//!
//! A transaction never writes to the state it was begun on. A part is cloned when something first
//! asks to change it, and untouched parts stay shared. Commit builds the next `State` from the
//! staged trees, and discard drops the overlay, so atomicity is a property of the structure and
//! not of an undo log that has to be right.

use super::State;
use super::derived::Derived;
use super::errors::internal;
use super::failpoint;
use super::ids::IdMint;
use super::intents::{Intent, Overlay};
use super::parts::{PartId, PartIndex, TreeCell, parse};
use super::view::PartsView;
use crate::api::error::StetError;
use crate::package::{NewPart, Package, opc};
use crate::refs::BlockKey;
use crate::xml::SpanTree;
use std::cell::RefCell;
use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, OnceLock};

/// The name of the part that registers content types.
const CONTENT_TYPES: &str = "[Content_Types].xml";

/// A part that a transaction cloned: its tree, and the count the tree had when it was cloned. A
/// clone has the count of the tree it came from, so the generation of a part does not move because
/// the part was borrowed, and moves when the tree changes.
pub(super) struct Staged {
    pub(super) tree: SpanTree,
    cloned_at: u64,
}

impl Staged {
    /// How many changes the tree has had, the span tree's own count. It is the second half of the
    /// part's generation.
    pub(super) fn edits(&self) -> u64 {
        self.tree.edit_count()
    }

    /// True if the tree has changed since the transaction cloned it, which is what makes a part
    /// edited in this transaction. A tree that was edited in an earlier commit and only borrowed in
    /// this one is not.
    pub(super) fn changed(&self) -> bool {
        self.tree.edit_count() != self.cloned_at
    }
}

/// A part whose bytes come from the transaction's package and not from the base's, and the call of
/// `add_part` that last wrote it.
pub(super) struct Fresh {
    /// The tree, parsed on first use from the new package.
    pub(super) cell: TreeCell,
    /// The number of the call, counted from 1 in the transaction. The part's epoch is the base's
    /// plus this number, so each call that rewrites a part gives it an epoch it never had, and a
    /// second call in one transaction is told from the first.
    pub(super) call: u64,
}

/// What `add_part` did to the transaction's own copy of the package.
pub(super) struct Added {
    /// The package with the new parts, the content types and the relationships.
    pub(super) package: Package,
    /// How many times `add_part` was called. The generation of the set of parts is the base's plus
    /// this.
    pub(super) calls: u64,
    /// The parts whose bytes come from that package and not from the base's: the parts that were
    /// added, and the ones that were rewritten (the content types, the source's relationships), the
    /// raw set.
    pub(super) fresh: BTreeMap<PartId, Fresh>,
}

/// A transaction on a `State`.
pub(crate) struct Tx<'s> {
    base: &'s State,
    pub(super) staged: BTreeMap<PartId, Staged>,
    pub(super) derived: RefCell<Derived>,
    pub(super) index: Arc<PartIndex>,
    pub(super) added: Option<Added>,
    pub(super) intents: Vec<Intent>,
    pub(super) touched: Vec<BlockKey>,
    pub(super) touched_set: HashSet<BlockKey>,
    /// Every call of `touch`, repeats included, so that a story can be refreshed with only the
    /// calls since it was last built.
    pub(super) touch_log: Vec<BlockKey>,
    /// For each story part built in this transaction, how long `touch_log` was when it was built.
    pub(super) story_cursor: RefCell<HashMap<PartId, usize>>,
    pub(super) overlay: Overlay,
    pub(super) mint: IdMint,
}

impl State {
    /// Begins a transaction. The state is not changed by it, whatever it does.
    pub(crate) fn begin(&self) -> Tx<'_> {
        Tx {
            base: self,
            staged: BTreeMap::new(),
            derived: RefCell::new(Derived::new()),
            index: Arc::clone(&self.index),
            added: None,
            intents: Vec::new(),
            touched: Vec::new(),
            touched_set: HashSet::new(),
            touch_log: Vec::new(),
            story_cursor: RefCell::new(HashMap::new()),
            overlay: Overlay::default(),
            mint: IdMint::default(),
        }
    }
}

impl<'s> Tx<'s> {
    /// The state this transaction was begun on, untouched.
    pub(crate) fn base(&self) -> &'s State {
        self.base
    }

    /// Read access to the parts as the transaction sees them: staged trees over committed ones.
    pub(crate) fn parts(&self) -> PartsView<'_> {
        PartsView {
            base: self.base,
            tx: Some(self),
        }
    }

    /// The epoch of a part in this transaction: the base's, plus the number of the call of
    /// `add_part` that last wrote it, if one did. A part that `add_part` made has no epoch in the
    /// base, which counts as 0.
    pub(super) fn epoch_of(&self, id: PartId) -> u64 {
        let base = self.base.slot(id).map_or(0, |s| s.generation.epoch);
        let call = self
            .added
            .as_ref()
            .and_then(|a| a.fresh.get(&id))
            .map_or(0, |f| f.call);
        base + call
    }

    /// The names of the parts this transaction changed or added, in part order: a staged tree that was edited, and a part that
    /// `add_part` wrote or rewrote. A tree that was only borrowed is not among them. The scope guard reads it to refuse a change
    /// to a part that no block covers.
    pub(crate) fn changed_parts(&self) -> Vec<Arc<str>> {
        let mut ids: std::collections::BTreeSet<PartId> = self
            .staged
            .iter()
            .filter(|(_, s)| s.changed())
            .map(|(id, _)| *id)
            .collect();
        if let Some(added) = &self.added {
            ids.extend(added.fresh.keys().copied());
        }
        ids.into_iter()
            .filter_map(|id| self.index.name(id).cloned())
            .collect()
    }

    /// The names of the staged trees this transaction edited that are clones of the base's trees, in part order: not a part
    /// that `add_part` wrote or rewrote. A node of such a tree whose id is at least the base tree's `node_count` was made by the
    /// transaction.
    pub(crate) fn edited_clones(&self) -> Vec<Arc<str>> {
        let fresh = |id: &PartId| {
            self.added
                .as_ref()
                .is_some_and(|a| a.fresh.contains_key(id))
        };
        self.staged
            .iter()
            .filter(|(id, s)| s.changed() && !fresh(id))
            .filter_map(|(id, _)| self.index.name(*id).cloned())
            .collect()
    }

    /// The generation of the set of parts: it moves with every call of `add_part`.
    pub(super) fn package_epoch(&self) -> u64 {
        self.base.package_epoch + self.added.as_ref().map_or(0, |a| a.calls)
    }

    /// The tree of a part, to change it. The first call parses the part if it was not parsed and
    /// clones its tree into the transaction. Later calls return the same clone. A part that nobody
    /// asks for is never copied.
    ///
    /// A part that could not be saved cannot be staged: an encrypted part, or one stored with a
    /// method other than stored or deflate, is `unsupported` when it is read, and a tree needs a
    /// read. `[Content_Types].xml` and the relationship parts change only through `add_part`. A
    /// name that is not a part is a bug in the caller, and is `internal`.
    pub(crate) fn part_mut(&mut self, name: &str) -> Result<&mut SpanTree, StetError> {
        failpoint::here("tx:part_mut")?;
        let base = self.base;
        let id = self
            .index
            .id(name)
            .ok_or_else(|| internal(format!("there is no part named {name:?} to change")))?;
        if is_package_plumbing(name) {
            return Err(internal(format!(
                "{name} changes only through add_part, and cannot be staged as a tree"
            )));
        }
        let (index, added) = (&self.index, &self.added);
        let staged = match self.staged.entry(id) {
            Entry::Occupied(staged) => staged.into_mut(),
            Entry::Vacant(vacant) => {
                let part = index
                    .name(id)
                    .ok_or_else(|| internal(format!("part {name:?} has no name")))?;
                let fresh = added
                    .as_ref()
                    .and_then(|a| a.fresh.get(&id).map(|f| (a, &f.cell)));
                let tree = match fresh {
                    Some((a, cell)) => cell
                        .get_or_init(|| parse(part, &a.package, &base.limits))
                        .as_ref()
                        .map_err(Clone::clone)?,
                    None => {
                        let slot = base
                            .slot(id)
                            .ok_or_else(|| internal(format!("part {name:?} has no slot")))?;
                        slot.tree(&base.package, &base.limits)?
                    }
                };
                vacant.insert(Staged {
                    cloned_at: tree.edit_count(),
                    tree: tree.clone(),
                })
            }
        };
        Ok(&mut staged.tree)
    }

    /// Adds a part, registers its content type and, if asked, a relationship to it, all or nothing,
    /// on the transaction's own copy of the package. Returns the new relationship id, if one was
    /// added.
    ///
    /// `Package::add_part` rewrites `[Content_Types].xml` and the source's relationships part, or
    /// makes it. So those parts, and the new one, get a fresh cell in the transaction, parsed from the
    /// new package, and a new epoch, and the set of parts has a new generation. Without that, what was
    /// derived from the relationships or the story list would stay stale after, say, comments are
    /// added. Each call counts: the epochs are the base's plus the number of the call, so a second
    /// call in one transaction is told from the first, and a story list or a `Rels` cached between the
    /// two is stale after the second.
    pub(crate) fn add_part(&mut self, new: NewPart<'_>) -> Result<Option<String>, StetError> {
        failpoint::here("tx:add_part")?;
        let mut package = self
            .added
            .as_ref()
            .map_or_else(|| self.base.package.clone(), |a| a.package.clone());
        let mut touched = vec![new.name.to_owned(), CONTENT_TYPES.to_owned()];
        if let Some((source, _)) = new.related_from {
            touched.push(opc::rels_part_name(source));
        }
        let relationship = package.add_part(new)?;
        // The package accepted it. What follows works on copies, and only the last lines change `self`.
        let mut index = (*self.index).clone();
        let mut ids = Vec::new();
        for name in &touched {
            if !package.contains(name) {
                continue;
            }
            let id = match index.id(name) {
                Some(id) => id,
                None => index.add(name)?,
            };
            ids.push(id);
        }
        self.adopt(package, index, &ids);
        Ok(relationship)
    }

    /// Adds a relationship from `source` (empty for the package itself) to an external target such
    /// as a URL, which names no part, and returns its id. It is for the hyperlinks of content that
    /// `compare` imports. The source's relationships part is rewritten, or made with its content
    /// type registered, so it and `[Content_Types].xml` get a fresh cell and epoch as in
    /// `add_part`, and what was derived from the relationships goes stale. The target is the
    /// string as it is written in the package, `TargetMode="External"`, and is not checked as a
    /// URL. A `source` that is not a part is `invalid_payload`, and nothing changes.
    pub(crate) fn add_relationship(
        &mut self,
        source: &str,
        rel_type: &str,
        target: &str,
    ) -> Result<String, StetError> {
        failpoint::here("tx:add_relationship")?;
        let mut package = self
            .added
            .as_ref()
            .map_or_else(|| self.base.package.clone(), |a| a.package.clone());
        let id = package.add_external_relationship(source, rel_type, target)?;
        let mut index = (*self.index).clone();
        let mut ids = Vec::new();
        for name in [opc::rels_part_name(source), CONTENT_TYPES.to_owned()] {
            if !package.contains(&name) {
                continue;
            }
            let part = match index.id(&name) {
                Some(part) => part,
                None => index.add(&name)?,
            };
            ids.push(part);
        }
        self.adopt(package, index, &ids);
        Ok(id)
    }

    /// Makes the transaction's package the one that `add_part` or `add_relationship` built, and
    /// gives the parts it wrote a fresh cell and a new epoch.
    fn adopt(&mut self, package: Package, index: PartIndex, ids: &[PartId]) {
        self.index = Arc::new(index);
        // A package change can replace a story's tree, so the next refresh gets every touched block.
        self.story_cursor.get_mut().clear();
        let added = self.added.get_or_insert_with(|| Added {
            package: package.clone(),
            calls: 0,
            fresh: BTreeMap::new(),
        });
        added.package = package;
        added.calls += 1;
        for id in ids {
            added.fresh.insert(
                *id,
                Fresh {
                    cell: OnceLock::new(),
                    call: added.calls,
                },
            );
        }
    }

    /// Drops the transaction: the staged trees, the declarations, the added package and everything it
    /// cached. Nothing was written to the state it was begun on.
    pub(crate) fn discard(self) {}
}

/// True for `[Content_Types].xml` and the relationship parts, which `Package::add_part` edits.
fn is_package_plumbing(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "[content_types].xml"
        || (lower.ends_with(".rels") && (lower.starts_with("_rels/") || lower.contains("/_rels/")))
}
