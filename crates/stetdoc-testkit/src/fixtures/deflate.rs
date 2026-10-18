//! A deflate stream built by hand, for the compression bombs.
//!
//! A bomb of 150 MB would need 150 MB of input to compress, several times over, on every call.
//! Deflate can say "repeat the last byte 258 times" in two bits, so the stream for a long run is
//! written directly and its CRC-32 comes from combining checksums. Building it takes a few
//! milliseconds and allocates about as much as the compressed size.
//!
//! The stream is: the prefix compressed normally and flushed to a byte boundary, one dynamic
//! Huffman block that holds the run, an empty stored block that ends the stream on a byte
//! boundary again, and the suffix compressed normally as the final block.

use flate2::{Compression, Crc, write::DeflateEncoder};
use std::io::Write;

/// How many bytes one match repeats: the longest deflate allows.
const MATCH: u64 = 258;

/// Writes bits in deflate order: fields least significant bit first, Huffman codes most
/// significant bit first.
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u32,
    used: u32,
}

impl Bits {
    fn put(&mut self, value: u32, count: u32) {
        for i in 0..count {
            self.acc |= ((value >> i) & 1) << self.used;
            self.used += 1;
            if self.used == 8 {
                self.out.push(self.acc as u8);
                self.acc = 0;
                self.used = 0;
            }
        }
    }

    fn code(&mut self, code: u32, len: u32) {
        for i in (0..len).rev() {
            self.put((code >> i) & 1, 1);
        }
    }

    fn align(&mut self) {
        if self.used > 0 {
            self.out.push(self.acc as u8);
            self.acc = 0;
            self.used = 0;
        }
    }
}

/// Writes a run of `n` zero code lengths (none when `n` is 0), as the symbol 18 of the code
/// length alphabet allows: 11 to 138 at a time.
fn zero_lengths(b: &mut Bits, mut n: u32) {
    while n > 0 {
        let take = if n > 138 {
            138 - if n - 138 < 11 { 11 } else { 0 }
        } else {
            n
        };
        assert!(
            take >= 11,
            "a run of {n} zero code lengths cannot be written"
        );
        b.code(0b0, 1); // code length symbol 18: a run of zero lengths
        b.put(take - 11, 7);
        n -= take;
    }
}

/// A non-final dynamic block: the literal `byte`, then `matches` copies of "repeat 258 bytes
/// from distance 1", then the end of the block; followed by an empty stored block, which leaves
/// the stream on a byte boundary. The byte must be 0 or between 11 and 244, so that the runs of
/// unused symbols around it can be written.
///
/// Code lengths: the literal and the end of block are 2 bits, the length symbol 285 (258 bytes)
/// is 1 bit, and the two distance codes are 1 bit each. So one match costs two bits, both zero.
fn run_block(byte: u8, matches: u32) -> Vec<u8> {
    assert!(
        byte == 0 || (11..=244).contains(&byte),
        "unsupported run byte {byte}"
    );
    let mut b = Bits::default();
    b.put(0, 1); // not the final block
    b.put(2, 2); // dynamic Huffman codes
    b.put(29, 5); // 286 literal and length codes
    b.put(1, 5); // 2 distance codes
    b.put(14, 4); // 18 code length codes
    // Code length code lengths, in the order 16 17 18 0 8 7 9 6 10 5 11 4 12 3 13 2 14 1 15:
    // symbol 18 has one bit, symbols 1 and 2 have two.
    for len in [0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2] {
        b.put(len, 3);
    }
    let len2 = |b: &mut Bits| b.code(0b11, 2); // code length symbol 2
    let len1 = |b: &mut Bits| b.code(0b10, 2); // code length symbol 1
    zero_lengths(&mut b, u32::from(byte)); // literals below the byte
    len2(&mut b); // the literal itself
    zero_lengths(&mut b, 255 - u32::from(byte)); // literals above it
    len2(&mut b); // end of block (256)
    zero_lengths(&mut b, 28); // lengths 257 to 284
    len1(&mut b); // length symbol 285
    len1(&mut b); // distance code 0 (one byte back)
    len1(&mut b); // distance code 1
    b.code(0b10, 2); // the literal
    for _ in 0..matches {
        b.put(0, 2); // length symbol 285, then distance code 0
    }
    b.code(0b11, 2); // end of block
    b.put(0, 3); // an empty stored block: not final, type 00
    b.align();
    let mut out = b.out;
    out.extend([0, 0, 0xFF, 0xFF]);
    out
}

fn crc_of(parts: &[&[u8]]) -> Crc {
    let mut crc = Crc::new();
    for p in parts {
        crc.update(p);
    }
    crc
}

/// The CRC-32 of `n` copies of `byte`, in a number of steps that grows with the length of `n`.
fn crc_of_run(byte: u8, mut n: u64) -> Crc {
    let mut result = Crc::new();
    let mut block = crc_of(&[&[byte]]);
    while n > 0 {
        if n & 1 == 1 {
            result.combine(&block);
        }
        let mut doubled = Crc::new();
        doubled.combine(&block);
        doubled.combine(&block);
        block = doubled;
        n >>= 1;
    }
    result
}

/// A raw deflate stream and what it inflates to.
pub(super) struct Stream {
    /// The compressed bytes.
    pub deflated: Vec<u8>,
    /// The size of the inflated data.
    pub inflated_len: u64,
    /// The CRC-32 of the inflated data.
    pub crc: u32,
}

/// A stream that inflates to `prefix`, then `1 + 258 * matches` copies of `byte`, then `suffix`.
pub(super) fn run_stream(prefix: &[u8], byte: u8, matches: u32, suffix: &[u8]) -> Stream {
    let mut head = DeflateEncoder::new(Vec::new(), Compression::new(9));
    head.write_all(prefix).unwrap();
    head.flush().unwrap(); // a sync flush: everything so far, on a byte boundary, not final
    let mut deflated = head.get_ref().clone();
    deflated.extend(run_block(byte, matches));
    let mut tail = DeflateEncoder::new(Vec::new(), Compression::new(9));
    tail.write_all(suffix).unwrap();
    deflated.extend(tail.finish().unwrap());
    let run = 1 + MATCH * u64::from(matches);
    let mut crc = crc_of(&[prefix]);
    crc.combine(&crc_of_run(byte, run));
    crc.combine(&crc_of(&[suffix]));
    Stream {
        deflated,
        inflated_len: prefix.len() as u64 + run + suffix.len() as u64,
        crc: crc.sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::DeflateDecoder;
    use std::io::Read;

    fn inflate(deflated: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        DeflateDecoder::new(deflated).read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn a_small_run_inflates_to_prefix_run_and_suffix() {
        let s = run_stream(b"<a>", b'A', 40, b"</a>");
        let mut expected = b"<a>".to_vec();
        expected.extend(vec![b'A'; 1 + 258 * 40]);
        expected.extend(b"</a>");
        assert_eq!(inflate(&s.deflated), expected);
        assert_eq!(s.inflated_len, expected.len() as u64);
        assert_eq!(s.crc, crc_of(&[&expected]).sum());
    }

    #[test]
    fn the_checksum_of_a_run_matches_the_checksum_of_the_bytes() {
        for n in [1u64, 2, 3, 257, 258, 1000, 65_537] {
            let bytes = vec![b'A'; n as usize];
            assert_eq!(crc_of_run(b'A', n).sum(), crc_of(&[&bytes]).sum(), "{n}");
        }
    }

    #[test]
    fn an_empty_prefix_and_suffix_still_make_a_valid_stream() {
        let s = run_stream(b"", b'A', 1, b"");
        assert_eq!(inflate(&s.deflated), vec![b'A'; 259]);
    }

    #[test]
    fn a_run_of_zeros_inflates_to_zeros() {
        let s = run_stream(b"head", 0, 300, b"tail");
        let mut expected = b"head".to_vec();
        expected.extend(vec![0u8; 1 + 258 * 300]);
        expected.extend(b"tail");
        assert_eq!(inflate(&s.deflated), expected);
        assert_eq!(s.crc, crc_of(&[&expected]).sum());
    }

    #[test]
    fn other_bytes_work_when_the_runs_of_unused_symbols_can_be_written() {
        for byte in [11u8, 12, 100, 243, 244] {
            let s = run_stream(b"", byte, 3, b"");
            assert_eq!(inflate(&s.deflated), vec![byte; 1 + 258 * 3], "{byte}");
        }
    }

    #[test]
    fn the_stream_is_about_two_bits_per_258_bytes() {
        let s = run_stream(b"", b'A', 100_000, b"");
        assert!(s.deflated.len() < 100_000 / 4 + 100, "{}", s.deflated.len());
        assert_eq!(s.inflated_len, 1 + 258 * 100_000);
    }
}
