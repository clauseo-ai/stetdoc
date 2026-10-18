//! The characters XML 1.0 does not allow anywhere in a document.
//!
//! The scanner is lenient about content rules that do not change the structure, so a part holding
//! a NUL or another control character would otherwise parse. Word does not open such a part, so
//! neither does this, with `malformed_input`. The check runs once over the whole part, which is
//! valid UTF-8 by then, and so only has two kinds of byte sequence to look for.

/// A control character other than tab, line feed and carriage return.
fn is_forbidden_control(b: u8) -> bool {
    b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')
}

/// The first forbidden control byte, eight bytes at a time while there is none to be found.
fn first_control(bytes: &[u8]) -> Option<(usize, u32)> {
    const ONES: u64 = 0x0101_0101_0101_0101;
    let mut at = 0usize;
    let mut chunks = bytes.chunks_exact(8);
    for chunk in chunks.by_ref() {
        let Ok(eight) = <[u8; 8]>::try_from(chunk) else {
            break;
        };
        let word = u64::from_le_bytes(eight);
        // Nonzero exactly when some byte of the word is below 0x20. Tab, line feed and carriage
        // return are such bytes too, so a hit is looked at one byte at a time.
        if word.wrapping_sub(ONES * 0x20) & !word & (ONES * 0x80) != 0
            && let Some(i) = chunk.iter().position(|&b| is_forbidden_control(b))
        {
            return Some((at + i, u32::from(chunk.get(i).copied().unwrap_or(0))));
        }
        at += 8;
    }
    chunks
        .remainder()
        .iter()
        .position(|&b| is_forbidden_control(b))
        .map(|i| {
            let b = chunks.remainder().get(i).copied().unwrap_or(0);
            (at + i, u32::from(b))
        })
}

/// The first U+FFFE or U+FFFF, which are `EF BF BE` and `EF BF BF` in UTF-8.
fn first_noncharacter(bytes: &[u8]) -> Option<(usize, u32)> {
    memchr::memmem::find_iter(bytes, b"\xEF\xBF").find_map(|at| match bytes.get(at + 2) {
        Some(0xBE) => Some((at, 0xFFFE)),
        Some(0xBF) => Some((at, 0xFFFF)),
        _ => None,
    })
}

/// The byte offset and the code point of the first character in `bytes` that XML 1.0 does not
/// allow: a control character other than tab, line feed and carriage return, or U+FFFE or U+FFFF.
/// `bytes` is valid UTF-8, so a surrogate cannot be in it.
pub(super) fn first_forbidden(bytes: &[u8]) -> Option<(usize, u32)> {
    match (first_control(bytes), first_noncharacter(bytes)) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_every_forbidden_character_and_only_those() {
        for c in 0u8..0x20 {
            let forbidden = !matches!(c, 9 | 10 | 13);
            for pad in 0..20 {
                let mut text = vec![b'a'; pad];
                text.push(c);
                text.extend_from_slice(b"tail");
                let want = forbidden.then_some((pad, u32::from(c)));
                assert_eq!(first_forbidden(&text), want, "byte {c:#x} after {pad}");
            }
        }
        assert_eq!(first_forbidden("a\u{FFFE}b".as_bytes()), Some((1, 0xFFFE)));
        assert_eq!(first_forbidden("ab\u{FFFF}".as_bytes()), Some((2, 0xFFFF)));
        assert_eq!(
            first_forbidden("\u{FFFF}\u{1}".as_bytes()),
            Some((0, 0xFFFF))
        );
        assert_eq!(first_forbidden("\u{1}\u{FFFF}".as_bytes()), Some((0, 1)));
        for ok in [
            "\t\n\r",
            " ",
            "\u{7f}",
            "\u{80}",
            "\u{85}",
            "\u{9f}",
            "\u{D7FF}",
            "\u{E000}",
            "\u{FDD0}",
            "\u{FFFD}",
            "\u{10000}",
            "\u{1FFFF}",
            "\u{10FFFF}",
            "\u{FF21}\u{FFC0}\u{FFEF}",
        ] {
            assert_eq!(first_forbidden(ok.as_bytes()), None, "{ok:?}");
        }
    }

    #[test]
    fn the_word_at_a_time_scan_agrees_with_a_plain_one() {
        // Random bytes with plenty of low ones, at every length and alignment around a word.
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..4000 {
            let len = (next() % 40) as usize;
            let bytes: Vec<u8> = (0..len)
                .map(|_| match next() % 5 {
                    0 => (next() % 0x24) as u8,
                    1 => b'\n',
                    _ => 0x20 + (next() % 0x5F) as u8,
                })
                .collect();
            let plain = bytes
                .iter()
                .position(|&b| is_forbidden_control(b))
                .map(|i| (i, u32::from(bytes[i])));
            assert_eq!(first_control(&bytes), plain, "{bytes:?}");
        }
    }
}
