//! The derived-cache store, and the generations that say when an entry is stale.
//!
//! Later modules cache things computed from parts: the block index of a story, the style cascade,
//! numbering, the `Rels` of a part. An entry records the **generation** of every part it was
//! built from, and is valid exactly while all of them are unchanged. Nothing is invalidated: a stale
//! entry is recognized by comparing numbers when it is read.

use super::parts::PartId;
use crate::api::error::StetError;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Something an entry depends on.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) enum Dep {
    /// One part of the package.
    Part(PartId),
    /// The set of parts and their relationships. It changes when a part is added, so an entry that
    /// depends on which parts exist (the story list, the relationships of a source with no
    /// relationships part yet) names this.
    Package,
}

/// Whether a part's tree has changed: the epoch counts replacements of the tree object, and `edits`
/// counts the changes made to it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) struct Generation {
    /// How many times the tree object was replaced, for example by parsing new bytes.
    pub(crate) epoch: u64,
    /// How many changes the tree has had.
    pub(crate) edits: u64,
}

/// What an entry is stored under: the type, a caller's variant for parameters that are not parts,
/// and the sorted dependencies. Two values of one type over different parts have different keys, and
/// a position in a list is never a key.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    ty: TypeId,
    variant: u64,
    deps: Box<[Dep]>,
}

impl Key {
    /// The key of a `T` built from `deps`, with a `variant` for what the deps do not say.
    pub(crate) fn of<T: 'static>(variant: u64, deps: &[Dep]) -> Key {
        let mut sorted = deps.to_vec();
        sorted.sort();
        sorted.dedup();
        Key {
            ty: TypeId::of::<T>(),
            variant,
            deps: sorted.into_boxed_slice(),
        }
    }
}

#[derive(Clone)]
struct Entry {
    stamps: Box<[Generation]>,
    value: Arc<dyn Any + Send + Sync>,
}

/// The answer to a lookup.
pub(crate) enum Lookup<T> {
    /// An entry whose generations all still hold.
    Valid(Arc<T>),
    /// An entry that was built from older generations, which a builder may refresh from.
    Stale(Arc<T>),
    /// No entry.
    Missing,
}

/// The entries of one `State`, or of one `Tx`.
#[derive(Clone, Default)]
pub(crate) struct Derived {
    map: HashMap<Key, Entry>,
}

impl Derived {
    pub(crate) fn new() -> Derived {
        Derived::default()
    }

    /// The number of entries, valid or not.
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Finds the entry for `key`, and says whether it is valid under the generations `current`
    /// reports.
    pub(crate) fn lookup<T: Send + Sync + 'static>(
        &self,
        key: &Key,
        current: &dyn Fn(Dep) -> Generation,
    ) -> Lookup<T> {
        let Some(entry) = self.map.get(key) else {
            return Lookup::Missing;
        };
        let Ok(value) = Arc::clone(&entry.value).downcast::<T>() else {
            return Lookup::Missing;
        };
        if is_valid(key, &entry.stamps, current) {
            Lookup::Valid(value)
        } else {
            Lookup::Stale(value)
        }
    }

    /// Stores a value built from the generations `stamps`, one for each dependency of the key.
    pub(crate) fn insert<T: Send + Sync + 'static>(
        &mut self,
        key: Key,
        stamps: Box<[Generation]>,
        value: Arc<T>,
    ) {
        self.map.insert(key, Entry { stamps, value });
    }

    /// Moves in every entry of `other` that is valid under `current`, and drops the rest. An entry
    /// already here under the same key is replaced only if `other`'s is valid.
    pub(crate) fn promote(&mut self, other: &Derived, current: &dyn Fn(Dep) -> Generation) {
        for (key, entry) in &other.map {
            if is_valid(key, &entry.stamps, current) {
                self.map.insert(key.clone(), entry.clone());
            }
        }
    }
}

fn is_valid(key: &Key, stamps: &[Generation], current: &dyn Fn(Dep) -> Generation) -> bool {
    key.deps.len() == stamps.len()
        && key
            .deps
            .iter()
            .zip(stamps)
            .all(|(dep, stamp)| current(*dep) == *stamp)
}

/// Reads the generations of the dependencies of a key.
pub(crate) fn stamps_of(key: &Key, current: &dyn Fn(Dep) -> Generation) -> Box<[Generation]> {
    key.deps.iter().map(|d| current(*d)).collect()
}

/// A cache builder: it gets the stale previous value, if there is one, so that it can refresh
/// instead of rebuilding. A refresh must give exactly what a build from scratch gives.
pub(crate) type Builder<'a, T> = &'a dyn Fn(Option<&T>) -> Result<T, StetError>;
