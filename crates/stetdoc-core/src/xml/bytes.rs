//! The shared, immutable byte buffer of a part.

use std::ops::Deref;
use std::sync::Arc;

/// Shared bytes. Cloning copies a pointer. A `Vec<u8>` becomes `Bytes` without copying its
/// contents, so an inflated part is read once into one allocation and then shared by the
/// package cache and the span tree. It lives in `xml` because the span tree is what holds the
/// source of a part; `package` hands the same buffer to it.
#[derive(Clone, Default, PartialEq, Eq, Hash)]
pub struct Bytes(Arc<Vec<u8>>);

impl Bytes {
    /// The bytes as a slice.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// An owned copy of the bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        self.0.as_ref().clone()
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Bytes {
        Bytes(Arc::new(v))
    }
}

impl From<&[u8]> for Bytes {
    fn from(v: &[u8]) -> Bytes {
        Bytes(Arc::new(v.to_vec()))
    }
}

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bytes({} bytes)", self.0.len())
    }
}
