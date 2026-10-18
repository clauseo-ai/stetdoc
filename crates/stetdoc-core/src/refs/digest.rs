//! SHA-256, the one stable hash of the core.
//!
//! Everything that is persisted or minted from a hash goes through here: the namespace id, the
//! lineage, durable ids and the digests the complement records. The crate used is `sha2`, which
//! gives the same digest on every platform. Every field written into a [`Hasher`] is length
//! prefixed, so two different lists of fields can never hash the same bytes.
//!
//! The in-memory maps of the core use `std`'s hash maps and never hash anything that is persisted,
//! minted or ordered. The frozen block fingerprint is FNV-1a and belongs to `wordml`.

use sha2::{Digest, Sha256};

/// A SHA-256 over length-prefixed fields.
pub(crate) struct Hasher(Sha256);

impl Hasher {
    /// A hasher whose first field is `domain`, which names what is being hashed, so a digest made
    /// for one purpose can never equal one made for another.
    pub(crate) fn new(domain: &str) -> Hasher {
        let mut h = Hasher(Sha256::new());
        h.field(domain.as_bytes());
        h
    }

    /// Adds one field: its length as 8 little-endian bytes, then the bytes.
    pub(crate) fn field(&mut self, bytes: &[u8]) -> &mut Hasher {
        self.0.update((bytes.len() as u64).to_le_bytes());
        self.0.update(bytes);
        self
    }

    /// Adds a number as a field of 8 little-endian bytes.
    pub(crate) fn number(&mut self, n: u64) -> &mut Hasher {
        self.field(&n.to_le_bytes())
    }

    /// The digest.
    pub(crate) fn finish(self) -> [u8; 32] {
        self.0.finalize().into()
    }
}

/// The plain SHA-256 of some bytes, with no framing. The complement records these for the head
/// package and the ref map, so that another tool can check them.
pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// Lower-case hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from_digit(u32::from(b >> 4), 16).unwrap_or('0'));
        out.push(char::from_digit(u32::from(b & 15), 16).unwrap_or('0'));
    }
    out
}

fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Reads exactly `2 * N` lower-case hexadecimal digits into `N` bytes. Anything else, including
/// upper case, is `None`, so that a value has one spelling.
pub(crate) fn unhex<const N: usize>(text: &str) -> Option<[u8; N]> {
    let digits = text.as_bytes();
    if digits.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (slot, pair) in out.iter_mut().zip(digits.chunks_exact(2)) {
        let (hi, lo) = match pair {
            [hi, lo] => (nibble(*hi)?, nibble(*lo)?),
            _ => return None,
        };
        *slot = (hi << 4) | lo;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_the_published_vectors() {
        assert_eq!(
            hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn fields_are_framed_so_that_concatenations_differ() {
        let mut a = Hasher::new("t");
        a.field(b"ab").field(b"c");
        let mut b = Hasher::new("t");
        b.field(b"a").field(b"bc");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn the_domain_separates_digests_of_the_same_fields() {
        let mut a = Hasher::new("one");
        a.field(b"x");
        let mut b = Hasher::new("two");
        b.field(b"x");
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn a_hasher_is_deterministic() {
        let make = || {
            let mut h = Hasher::new("d");
            h.field(b"x").number(7);
            h.finish()
        };
        assert_eq!(make(), make());
    }

    #[test]
    fn hex_round_trips_and_has_one_spelling() {
        let bytes = [0x00, 0x0f, 0xa5, 0xff];
        assert_eq!(hex(&bytes), "000fa5ff");
        assert_eq!(unhex::<4>("000fa5ff"), Some(bytes));
        assert_eq!(unhex::<4>("000FA5FF"), None, "upper case");
        assert_eq!(unhex::<4>("000fa5f"), None, "short");
        assert_eq!(unhex::<4>("000fa5ff00"), None, "long");
        assert_eq!(unhex::<4>("000fa5fg"), None, "not hex");
        assert_eq!(unhex::<4>("000fa5é"), None, "not ASCII");
    }
}
