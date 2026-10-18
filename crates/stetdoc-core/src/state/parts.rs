//! The parts of a package as the state sees them: a numbering, and one lazily parsed slot each.
//!
//! A part is parsed the first time something asks for its tree, through `&self`, and the result,
//! an error included, is kept in a cell that the next `State` shares. So a corrupt part costs one
//! parse, and a tree parsed in one state version is not parsed again in the next.

use super::derived::Generation;
use super::errors::internal;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::package::Package;
use crate::xml::SpanTree;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// A part's number: its place in the package order, with added parts after. It is never reused,
/// so it is safe in a cache key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct PartId(u32);

impl PartId {
    pub(super) fn index(self) -> usize {
        self.0 as usize
    }

    /// The number as an integer, for a cache variant.
    pub(crate) fn number(self) -> u32 {
        self.0
    }
}

/// The names of the parts, and the numbers they were given.
#[derive(Clone)]
pub(crate) struct PartIndex {
    names: Vec<Arc<str>>,
    lookup: HashMap<String, PartId>,
}

impl PartIndex {
    /// Numbers the parts in the order given. Names are the canonical entry names, which are unique
    /// ignoring ASCII case, as OPC compares them.
    pub(crate) fn new<'a>(names: impl Iterator<Item = &'a str>) -> Result<PartIndex, StetError> {
        let mut index = PartIndex {
            names: Vec::new(),
            lookup: HashMap::new(),
        };
        for name in names {
            index.add(name)?;
        }
        Ok(index)
    }

    /// Numbers one more part, after the others.
    pub(crate) fn add(&mut self, name: &str) -> Result<PartId, StetError> {
        let id = u32::try_from(self.names.len()).map(PartId).map_err(|_| {
            StetError::new(ErrorCode::LimitExceeded, "the package has too many parts")
        })?;
        self.lookup.insert(name.to_ascii_lowercase(), id);
        self.names.push(Arc::from(name));
        Ok(id)
    }

    /// The number of a part, matching the name ignoring ASCII case. A lookup lower-cases the name,
    /// so callers that look up often hold the [`PartId`].
    pub(crate) fn id(&self, name: &str) -> Option<PartId> {
        self.lookup.get(&name.to_ascii_lowercase()).copied()
    }

    /// The canonical name of a part, spelled as the package entry spells it.
    pub(crate) fn name(&self, id: PartId) -> Option<&Arc<str>> {
        self.names.get(id.index())
    }

    /// The canonical names, in part order.
    pub(crate) fn names(&self) -> impl Iterator<Item = &Arc<str>> {
        self.names.iter()
    }

    /// How many parts there are.
    pub(crate) fn len(&self) -> usize {
        self.names.len()
    }
}

/// The cell that holds a part's tree once it is parsed, or the error that parsing gave.
pub(crate) type TreeCell = OnceLock<Result<SpanTree, StetError>>;

/// One part: its name, its tree (parsed on first use) and its generation.
#[derive(Clone)]
pub(crate) struct PartSlot {
    pub(super) name: Arc<str>,
    pub(super) cell: Arc<TreeCell>,
    pub(super) generation: Generation,
}

impl PartSlot {
    /// A part that has not been parsed.
    pub(crate) fn unparsed(name: Arc<str>) -> PartSlot {
        PartSlot {
            name,
            cell: Arc::new(OnceLock::new()),
            generation: Generation::default(),
        }
    }

    /// A part whose bytes were replaced or added, so that nothing is known of its tree yet: a fresh,
    /// unparsed cell under a new epoch.
    pub(crate) fn fresh(name: Arc<str>, epoch: u64) -> PartSlot {
        PartSlot {
            generation: Generation { epoch, edits: 0 },
            ..PartSlot::unparsed(name)
        }
    }

    /// A part whose tree is known, such as one a transaction edited.
    pub(crate) fn with_tree(name: Arc<str>, tree: SpanTree, generation: Generation) -> PartSlot {
        PartSlot {
            name,
            cell: Arc::new(OnceLock::from(Ok(tree))),
            generation,
        }
    }

    /// The tree, parsed from the package on the first call. A failed parse is remembered, so the
    /// error comes back each time.
    pub(crate) fn tree(&self, package: &Package, limits: &Limits) -> Result<&SpanTree, StetError> {
        self.cell
            .get_or_init(|| parse(&self.name, package, limits))
            .as_ref()
            .map_err(Clone::clone)
    }

    /// The serialized tree if the part was ever edited since the package's bytes were read, and
    /// `None` otherwise, in which case the package's own bytes are the part. That is `is_dirty`, the
    /// question of whether the tree can differ from the package, and not whether an edit is in
    /// progress (a commit asks `Staged::changed` for that). It is serialized on each call, which
    /// takes about a millisecond for the largest body measured, so that no copy of an edited part
    /// stays in memory between saves.
    pub(crate) fn serialized(&self) -> Option<Vec<u8>> {
        match self.cell.get() {
            Some(Ok(tree)) if tree.is_dirty() => Some(tree.serialize()),
            _ => None,
        }
    }
}

fn missing(name: &str) -> StetError {
    internal(format!(
        "the package has no part named {name:?}, though the state numbered one"
    ))
}

/// Parses a part of `package`.
pub(super) fn parse(
    name: &Arc<str>,
    package: &Package,
    limits: &Limits,
) -> Result<SpanTree, StetError> {
    let bytes = package.part(name)?.ok_or_else(|| missing(name))?;
    SpanTree::parse(name, bytes, limits)
}
