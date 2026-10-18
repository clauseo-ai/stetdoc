//! `PartsView`: the one way to read parts and derived values, from a committed `State` or from
//! a `Tx` in progress.
//!
//! It is a concrete type, not a trait, because the core has exactly one seam and a view with a base
//! and an optional overlay has no second adapter. A view over a `Tx` sees the transaction's staged
//! trees and the parts its `add_part` made, and every cache value it builds goes into the
//! transaction's store, never the base's, so a dry run cannot leak what it computed into the
//! document.

use super::State;
use super::derived::{Builder, Dep, Generation, Key, Lookup, stamps_of};
use super::errors::internal;
use super::parts::{PartId, PartIndex, parse};
use super::tx::Tx;
use crate::api::error::StetError;
use crate::refs::{BlockKey, BlockRef};
use crate::xml::SpanTree;
use std::sync::Arc;

/// Read access to the parts and the derived values of one state, or of a state with a transaction's
/// edits on top.
#[derive(Clone, Copy)]
pub(crate) struct PartsView<'a> {
    pub(super) base: &'a State,
    pub(super) tx: Option<&'a Tx<'a>>,
}

impl<'a> PartsView<'a> {
    pub(super) fn index(&self) -> &'a PartIndex {
        self.tx.map_or(&*self.base.index, |tx| &*tx.index)
    }

    /// The number of a part, matching the name ignoring ASCII case.
    pub(crate) fn id(&self, name: &str) -> Option<PartId> {
        self.index().id(name)
    }

    /// True if the package has a part with this name.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.id(name).is_some()
    }

    /// The canonical name of a part.
    pub(crate) fn name(&self, id: PartId) -> Option<&'a Arc<str>> {
        self.index().name(id)
    }

    /// The tree of a part: the transaction's staged copy if it has one, then the tree of a part its
    /// `add_part` made or rewrote, and otherwise the committed tree, parsed on the first call. A
    /// part number that is not this package's is a bug in the caller, and is `internal`.
    pub(crate) fn tree(&self, part: PartId) -> Result<&'a SpanTree, StetError> {
        if let Some(tx) = self.tx {
            if let Some(staged) = tx.staged.get(&part) {
                return Ok(&staged.tree);
            }
            if let Some((added, cell)) = tx
                .added
                .as_ref()
                .and_then(|a| a.fresh.get(&part).map(|f| (a, &f.cell)))
            {
                let name = tx.index.name(part).ok_or_else(|| no_such_part(part))?;
                return cell
                    .get_or_init(|| parse(name, &added.package, &self.base.limits))
                    .as_ref()
                    .map_err(Clone::clone);
            }
        }
        let slot = self.base.slot(part).ok_or_else(|| no_such_part(part))?;
        slot.tree(&self.base.package, &self.base.limits)
    }

    /// The tree of a part that may legitimately be missing, such as `word/styles.xml`.
    pub(crate) fn tree_opt(&self, name: &str) -> Result<Option<&'a SpanTree>, StetError> {
        self.id(name).map(|id| self.tree(id)).transpose()
    }

    /// The content type registered for a part, by an override or by the default for its extension,
    /// or `None` if `[Content_Types].xml` registers none, which is what check's rule P1 reports. A
    /// part that is not in the package has none. It reads the content types of the package this view
    /// reads, so a part added in a transaction has the type it was added with.
    pub(crate) fn content_type(&self, part: PartId) -> Result<Option<String>, StetError> {
        let name = self.name(part).ok_or_else(|| no_such_part(part))?;
        self.package().content_type(name)
    }

    /// The generation of a dependency as this view sees it.
    pub(crate) fn generation(&self, dep: Dep) -> Generation {
        let base_generation = |id: PartId| {
            self.base
                .slot(id)
                .map_or_else(Generation::default, |s| s.generation)
        };
        let Some(tx) = self.tx else {
            return match dep {
                Dep::Package => Generation {
                    epoch: self.base.package_epoch,
                    edits: 0,
                },
                Dep::Part(id) => base_generation(id),
            };
        };
        match dep {
            Dep::Package => Generation {
                epoch: tx.package_epoch(),
                edits: 0,
            },
            Dep::Part(id) => {
                let epoch = tx.epoch_of(id);
                match tx.staged.get(&id) {
                    Some(staged) => Generation {
                        epoch,
                        edits: staged.edits(),
                    },
                    None if epoch != base_generation(id).epoch => Generation { epoch, edits: 0 },
                    None => base_generation(id),
                }
            }
        }
    }

    /// The live ref of a block, or `None` if it has none. A view of a committed state answers from its
    /// table. A view of a transaction answers as the transaction's declarations leave it: a block that
    /// the transaction created has no ref yet (refs are minted at commit, so a dry run never issues
    /// one), and a ref that a `rebind` or a `merge` moved is on the element it names now.
    pub(crate) fn ref_of(&self, key: &BlockKey) -> Option<BlockRef> {
        match self.tx {
            Some(tx) => tx.ref_of(key),
            None => self.base.refs.ref_of(key),
        }
    }

    /// True if the element is in the document of its part: its parent links lead to the document
    /// node within the nesting limit. An element that was detached, or never inserted, is not.
    pub(crate) fn is_attached(&self, key: &BlockKey) -> Result<bool, StetError> {
        let part = self
            .id(key.part())
            .ok_or_else(|| internal(format!("{} is not a part", key.part())))?;
        let tree = self.tree(part)?;
        let mut node = tree.node_ref(key.node());
        let bound = self.base.limits.max_xml_depth as usize + 16;
        for _ in 0..bound {
            match node.parent() {
                Some(parent) => node = parent,
                None => break,
            }
        }
        Ok(node.id() == tree.document().id())
    }

    /// Stores a value that the caller has already built from the current generations of `deps`,
    /// unless the store holds a valid one. The audit uses it to leave the stories it scanned for the
    /// next state.
    pub(super) fn seed<T: Send + Sync + 'static>(&self, variant: u64, deps: &[Dep], value: Arc<T>) {
        let key = Key::of::<T>(variant, deps);
        let current = |d: Dep| self.generation(d);
        let store = match self.tx {
            Some(tx) => &tx.derived,
            None => &self.base.derived,
        };
        let valid = matches!(store.borrow().lookup::<T>(&key, &current), Lookup::Valid(_));
        if !valid {
            let stamps = stamps_of(&key, &current);
            store.borrow_mut().insert(key, stamps, value);
        }
    }

    /// The value of a cache that is a function of the parts in `deps` and of nothing else.
    ///
    /// The entry is found by the type, `variant` and the sorted dependencies, and is used if every
    /// dependency has the generation it had when the entry was built. Otherwise `build` makes the
    /// value, and gets the stale previous value, if there is one, so that it can refresh. The new
    /// value goes into the transaction's store when there is a transaction, and the state's otherwise.
    /// `build` may call `derived` itself: no store is borrowed while it runs.
    pub(crate) fn derived<T: Send + Sync + 'static>(
        &self,
        variant: u64,
        deps: &[Dep],
        build: Builder<'_, T>,
    ) -> Result<Arc<T>, StetError> {
        let key = Key::of::<T>(variant, deps);
        let current = |d: Dep| self.generation(d);
        let mut stale: Option<Arc<T>> = None;
        if let Some(tx) = self.tx {
            match tx.derived.borrow().lookup::<T>(&key, &current) {
                Lookup::Valid(v) => return Ok(v),
                Lookup::Stale(v) => stale = Some(v),
                Lookup::Missing => {}
            }
        }
        match self.base.derived.borrow().lookup::<T>(&key, &current) {
            Lookup::Valid(v) => return Ok(v),
            Lookup::Stale(v) if stale.is_none() => stale = Some(v),
            Lookup::Stale(_) | Lookup::Missing => {}
        }
        let stamps = stamps_of(&key, &current);
        let value = Arc::new(build(stale.as_deref())?);
        match self.tx {
            Some(tx) => tx.derived.borrow_mut().insert(key, stamps, value.clone()),
            None => self
                .base
                .derived
                .borrow_mut()
                .insert(key, stamps, value.clone()),
        }
        Ok(value)
    }
}

fn no_such_part(part: PartId) -> StetError {
    internal(format!(
        "part number {part:?} is not a part of this package"
    ))
}
