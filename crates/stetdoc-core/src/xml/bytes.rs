//! The shared, immutable byte buffer of a part.

use std::ops::Deref;
use std::sync::Arc;

/// Shared bytes. Cloning copies a pointer. A `Vec<u8>` becomes `Bytes` without copying its
/// contents, so an inflated part is read once into one allocation and then shared by the
/// package cache and the span tree. It lives in `xml` because the span tree is what holds the
/// source of a part; `package` hands the same buffer to it.
///
/// Bytes made by [`Bytes::from_part`] remember whether they are UTF-8, which every UTF-8 XML part
/// is, so that the span tree can read its names and text as `str` without checking them again.
#[derive(Clone, Default)]
pub struct Bytes(Arc<Buf>);

/// The buffer: bytes, or bytes known to be UTF-8.
enum Buf {
    Raw(Vec<u8>),
    Text(String),
}

impl Default for Buf {
    fn default() -> Buf {
        Buf::Raw(Vec::new())
    }
}

impl Buf {
    fn as_slice(&self) -> &[u8] {
        match self {
            Buf::Raw(v) => v.as_slice(),
            Buf::Text(s) => s.as_bytes(),
        }
    }
}

impl Bytes {
    /// The bytes of a part read from a package. They are the same as `Bytes::from(v)`; checking once
    /// here whether they are UTF-8 (a binary part fails at its first bytes) lets the span tree parsed
    /// from them skip the check on every name and text it reads.
    pub fn from_part(v: Vec<u8>) -> Bytes {
        match String::from_utf8(v) {
            Ok(s) => Bytes(Arc::new(Buf::Text(s))),
            Err(e) => Bytes(Arc::new(Buf::Raw(e.into_bytes()))),
        }
    }

    /// The bytes as a slice.
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// The bytes as a string, if they were found to be UTF-8 when they were made.
    pub(super) fn as_str(&self) -> Option<&str> {
        match self.0.as_ref() {
            Buf::Text(s) => Some(s.as_str()),
            Buf::Raw(_) => None,
        }
    }

    /// An owned copy of the bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        self.as_slice().to_vec()
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Bytes) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl Eq for Bytes {}

impl std::hash::Hash for Bytes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(v: Vec<u8>) -> Bytes {
        Bytes(Arc::new(Buf::Raw(v)))
    }
}

impl From<&[u8]> for Bytes {
    fn from(v: &[u8]) -> Bytes {
        Bytes(Arc::new(Buf::Raw(v.to_vec())))
    }
}

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bytes({} bytes)", self.len())
    }
}
