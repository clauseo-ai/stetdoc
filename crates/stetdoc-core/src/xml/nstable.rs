//! The namespace table of a tree: every distinct namespace URI once, addressed by a small index.

use super::errors::limit;
use crate::api::error::StetError;
use std::collections::HashMap;
use std::sync::Arc;

/// URIs by index, and the index of a URI. Index 0 is the empty string, which means "no
/// namespace". Looking up or adding a URI is a hash lookup, however many URIs the part declares.
#[derive(Clone)]
pub(super) struct NsTable {
    uris: Vec<Arc<str>>,
    index: HashMap<Arc<str>, u16>,
}

impl NsTable {
    pub(super) fn new() -> NsTable {
        let empty: Arc<str> = Arc::from("");
        NsTable {
            uris: vec![empty.clone()],
            index: HashMap::from([(empty, 0)]),
        }
    }

    /// The URI at an index. An index this table never issued reads as "no namespace".
    pub(super) fn get(&self, i: u16) -> &str {
        self.uris.get(usize::from(i)).map_or("", |u| &**u)
    }

    /// The index of a URI that is already in the table.
    pub(super) fn find(&self, uri: &str) -> Option<u16> {
        self.index.get(uri).copied()
    }

    /// The index of a URI, adding it if it is new. A part can name at most 65,535 different
    /// namespaces.
    pub(super) fn intern(&mut self, uri: &str) -> Result<u16, StetError> {
        if let Some(i) = self.index.get(uri) {
            return Ok(*i);
        }
        let next = u16::try_from(self.uris.len())
            .ok()
            .filter(|&i| i != u16::MAX)
            .ok_or_else(|| limit("too many distinct XML namespaces"))?;
        let shared: Arc<str> = Arc::from(uri);
        self.uris.push(shared.clone());
        self.index.insert(shared, next);
        Ok(next)
    }

    /// How many URIs the table holds, counting the empty one.
    pub(super) fn len(&self) -> usize {
        self.uris.len()
    }
}
