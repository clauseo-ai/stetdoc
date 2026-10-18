//! Turning a state back into a package, and a working copy's saved form: `export` and `resume`.

use super::State;
use super::errors::corrupt;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::package::Package;
use crate::refs::{BaseToken, Layout, NamespaceOrigin, RefTable, sha256};

/// What a working copy keeps of a state: the head package bytes, the ref map, the token, and the
/// SHA-256 of the two, which `resume` checks first.
#[derive(Clone, Debug)]
pub(crate) struct Exported {
    /// The package as `to_package` writes it.
    pub(crate) head: Vec<u8>,
    /// The ref map, in its canonical text form.
    pub(crate) refs: Vec<u8>,
    /// The token of the exported state.
    pub(crate) base: BaseToken,
    /// SHA-256 of `head`.
    pub(crate) head_sha256: [u8; 32],
    /// SHA-256 of `refs`.
    pub(crate) refs_sha256: [u8; 32],
}

/// What `document` read back from the complement: the token and the two digests.
#[derive(Clone, Debug)]
pub(crate) struct Recorded {
    /// The token recorded at export.
    pub(crate) base: BaseToken,
    /// SHA-256 of the head bytes recorded at export.
    pub(crate) head_sha256: [u8; 32],
    /// SHA-256 of the ref map recorded at export.
    pub(crate) refs_sha256: [u8; 32],
}

/// A saved document that no longer opens is damaged, except when it is over a limit.
fn unreadable(e: StetError) -> StetError {
    match e.code() {
        ErrorCode::LimitExceeded => e,
        _ => corrupt(format!(
            "The saved document no longer opens: {}",
            e.message()
        )),
    }
}

impl State {
    /// The package this state stands for. If no part was edited it is the package the state was
    /// opened from, and writing it gives the input bytes (law L1). Otherwise each edited part is
    /// serialized and replaces the part's bytes, one at a time, and every other part is copied raw.
    pub(crate) fn to_package(&self) -> Result<Package, StetError> {
        let mut package = self.package.clone();
        for slot in &self.slots {
            if let Some(bytes) = slot.serialized() {
                package.replace_part(&slot.name, bytes)?;
            }
        }
        Ok(package)
    }

    /// The saved form of this state: the package, the ref map, the token and two digests. A
    /// working copy stores them, and `resume` reads them back.
    pub(crate) fn export(&self) -> Result<Exported, StetError> {
        let head = self.to_package()?.write()?;
        let layout = self.parts().layout(true)?;
        let refs = self.refs.persist(&layout)?;
        Ok(Exported {
            head_sha256: sha256(&head),
            refs_sha256: sha256(&refs),
            base: self.refs.base_token(),
            head,
            refs,
        })
    }

    /// Builds the state that `export` saved. `original` is the original package bytes, which an
    /// opened namespace is checked against.
    ///
    /// The digests are checked first. Then the head bytes are opened and every story scanned afresh,
    /// the ref map is restored against that scan, which refuses anything that is not exactly what
    /// `persist` wrote for this document, and the restored token must equal the recorded one. A head
    /// that no longer opens is `corrupt_complement`, because the original opened. A limit violation
    /// stays `limit_exceeded`.
    pub(crate) fn resume(
        head: Vec<u8>,
        refs: &[u8],
        recorded: &Recorded,
        original: Option<&[u8]>,
        limits: &Limits,
    ) -> Result<State, StetError> {
        if sha256(&head) != recorded.head_sha256 {
            return Err(corrupt(
                "The saved document does not match the digest recorded with it.",
            ));
        }
        if sha256(refs) != recorded.refs_sha256 {
            return Err(corrupt(
                "The saved ref map does not match the digest recorded with it.",
            ));
        }
        let package = Package::open(head, limits).map_err(unreadable)?;
        let namespace = recorded.base.namespace();
        let empty = RefTable::open(
            namespace,
            NamespaceOrigin::Opened,
            &Layout::default(),
            limits,
        )?;
        let mut state = State::new(package, limits, empty).map_err(unreadable)?;
        let layout = state.parts().layout(true).map_err(unreadable)?;
        let table = RefTable::restore(refs, &layout, original, limits)?;
        if table.base_token() != recorded.base {
            return Err(corrupt(
                "The saved ref map is for another state than the one recorded with it.",
            ));
        }
        state.refs = table;
        Ok(state)
    }
}
