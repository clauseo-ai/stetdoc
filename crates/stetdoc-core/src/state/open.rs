//! Opening a document: the namespace, the package, the stories and the refs.

use super::State;
use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::package::Package;
use crate::refs::{Layout, NamespaceId, NamespaceOrigin, RefTable};
use crate::wordml::SCAN_VERSION;

impl State {
    /// Opens the bytes of a package. The namespace is a hash of the whole input, so the same bytes
    /// always give the same namespace and the same refs, and any change to the bytes gives a new
    /// namespace.
    ///
    /// Opening parses the stories (the body, headers, footers, notes and comments) and nothing else,
    /// because refs are minted for every story and a scan needs a tree. Styles, numbering, settings
    /// and every other part stay as compressed bytes until something asks for them.
    pub(crate) fn open(bytes: Vec<u8>, limits: &Limits) -> Result<State, StetError> {
        let namespace = NamespaceId::of_package(&bytes, SCAN_VERSION);
        State::open_in(namespace, NamespaceOrigin::Opened, bytes, limits)
    }

    /// Opens `revised` under the namespace that a comparison of `original` with it, with this `extra`, gives
    /// (`NamespaceId::derived`, label `compare`): the same two files and the same extra always give the same refs, and a redline never
    /// shares a namespace with either input.
    pub(crate) fn open_derived(
        original: &[u8],
        revised: Vec<u8>,
        extra: &[u8],
        limits: &Limits,
    ) -> Result<State, StetError> {
        let parents = [
            NamespaceId::of_package(original, SCAN_VERSION),
            NamespaceId::of_package(&revised, SCAN_VERSION),
        ];
        let namespace = NamespaceId::derived("compare", &parents, extra);
        let origin = NamespaceOrigin::Derived {
            label: "compare".to_owned(),
            parents: parents.to_vec(),
            extra: extra.to_vec(),
        };
        State::open_in(namespace, origin, revised, limits)
    }

    /// Opens the bytes of a package under a namespace that the caller names, for a state that is
    /// not the result of opening bytes (`compare`'s redline).
    pub(crate) fn open_in(
        namespace: NamespaceId,
        origin: NamespaceOrigin,
        bytes: Vec<u8>,
        limits: &Limits,
    ) -> Result<State, StetError> {
        let package = Package::open(bytes, limits)?;
        let empty = RefTable::open(namespace, origin.clone(), &Layout::default(), limits)?;
        let mut state = State::new(package, limits, empty)?;
        let layout = state.parts().layout(false)?;
        state.refs = RefTable::open(namespace, origin, &layout, limits)?;
        Ok(state)
    }
}
