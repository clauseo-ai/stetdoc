//! CRC-32, inflate with an output cap, and deterministic deflate.

use super::errors::malformed;
use crate::api::error::StetError;
use crate::xml::Bytes;
use miniz_oxide::deflate::compress_to_vec;
use miniz_oxide::inflate::TINFLStatus;
use miniz_oxide::inflate::core::{DecompressorOxide, decompress, inflate_flags};

/// Compression level for rewritten entries. Fixed so output is the same everywhere.
const DEFLATE_LEVEL: u8 = 6;

#[expect(
    clippy::indexing_slicing,
    reason = "const evaluation: an out-of-range index here is a compile error, never a runtime panic"
)]
const fn make_tables() -> [[u32; 256]; 8] {
    let mut t = [[0u32; 256]; 8];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        t[0][i] = c;
        i += 1;
    }
    let mut i = 0;
    while i < 256 {
        let mut s = 1;
        while s < 8 {
            let prev = t[s - 1][i];
            t[s][i] = (prev >> 8) ^ t[0][(prev & 0xFF) as usize];
            s += 1;
        }
        i += 1;
    }
    t
}

static TABLES: [[u32; 256]; 8] = make_tables();

fn tab(row: usize, idx: u32) -> u32 {
    TABLES
        .get(row)
        .and_then(|r| r.get((idx & 0xFF) as usize))
        .copied()
        .unwrap_or(0)
}

/// CRC-32 (IEEE), slice-by-8.
pub(crate) fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    let mut chunks = data.chunks_exact(8);
    for c in &mut chunks {
        let (a, b) = c.split_at(4);
        let lo = u32::from_le_bytes([
            a.first().copied().unwrap_or(0),
            a.get(1).copied().unwrap_or(0),
            a.get(2).copied().unwrap_or(0),
            a.get(3).copied().unwrap_or(0),
        ]) ^ crc;
        let hi = u32::from_le_bytes([
            b.first().copied().unwrap_or(0),
            b.get(1).copied().unwrap_or(0),
            b.get(2).copied().unwrap_or(0),
            b.get(3).copied().unwrap_or(0),
        ]);
        crc = tab(7, lo)
            ^ tab(6, lo >> 8)
            ^ tab(5, lo >> 16)
            ^ tab(4, lo >> 24)
            ^ tab(3, hi)
            ^ tab(2, hi >> 8)
            ^ tab(1, hi >> 16)
            ^ tab(0, hi >> 24);
    }
    for &b in chunks.remainder() {
        crc = tab(0, crc ^ u32::from(b)) ^ (crc >> 8);
    }
    !crc
}

/// Inflates a raw deflate stream into a buffer of exactly `expected` bytes. A stream that
/// wants more output, or ends short, is rejected. `expected` is already limit-checked, and the
/// decoder writes straight into the final allocation, so there is no growth and no copy.
pub(crate) fn inflate_to_bytes(
    data: &[u8],
    expected: usize,
    name: &str,
) -> Result<Bytes, StetError> {
    // `vec![0; n]` is a lazily zeroed allocation: its pages are touched once, by the decoder.
    let mut out = vec![0u8; expected];
    let buf = out.as_mut_slice();
    let mut state = Box::new(DecompressorOxide::new());
    let (status, _used, written) = decompress(
        &mut state,
        data,
        buf,
        0,
        inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
    );
    match status {
        TINFLStatus::Done if written == expected => Ok(Bytes::from_part(out)),
        TINFLStatus::Done => Err(malformed(format!(
            "part {name}: inflated to {written} bytes but the central directory says {expected}"
        ))),
        TINFLStatus::HasMoreOutput => Err(malformed(format!(
            "part {name}: inflates to more than the declared {expected} bytes"
        ))),
        other => Err(malformed(format!(
            "part {name}: corrupt or truncated deflate stream ({other:?})"
        ))),
    }
}

/// Raw deflate with the fixed level.
pub(crate) fn deflate(data: &[u8]) -> Vec<u8> {
    compress_to_vec(data, DEFLATE_LEVEL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_known_values() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        let long: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        // compare slice-by-8 against a bytewise computation
        let mut c = !0u32;
        for &b in &long {
            c = tab(0, c ^ u32::from(b)) ^ (c >> 8);
        }
        assert_eq!(crc32(&long), !c);
    }

    #[test]
    fn deflate_round_trip_and_cap() {
        let data = vec![b'a'; 10_000];
        let z = deflate(&data);
        assert_eq!(
            inflate_to_bytes(&z, data.len(), "t").ok().as_deref(),
            Some(&data[..])
        );
        assert!(inflate_to_bytes(&z, data.len() - 1, "t").is_err());
        assert!(inflate_to_bytes(&z, data.len() + 1, "t").is_err());
        // empty output, truncated and garbage input
        assert_eq!(
            inflate_to_bytes(&deflate(b""), 0, "t").ok().as_deref(),
            Some(&b""[..])
        );
        assert!(inflate_to_bytes(z.get(..z.len() / 2).unwrap(), data.len(), "t").is_err());
        assert!(inflate_to_bytes(&[0xFF; 20], 100, "t").is_err());
        assert!(inflate_to_bytes(&[], 0, "t").is_err());
    }
}
