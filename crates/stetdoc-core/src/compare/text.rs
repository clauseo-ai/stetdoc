//! The text of a block as the aligner sees it: the alignment key, the small hash and the word bag.
//!
//! Everything here is pure and uses nothing but `std`, so that the alignment can be run on plain strings. The key of a
//! block is its comparison text with white space collapsed, the ends trimmed, curly quotes straight and the
//! non-breaking space a space (design `compare.md`, 3). The exact text is what the word diff uses, so a difference
//! of quotes or of spaces is still shown. The hash has a fixed seed and no randomness: a seed that varied would break
//! determinism.

/// The seed of the hash. Never changes: a different seed would give different keys for the same text.
const SEED: u64 = 0xcbf2_9ce4_8422_2325;

/// The prime of FNV-1a, 64 bits.
const PRIME: u64 = 0x0000_0100_0000_01b3;

/// The separator of the cells of a row in the text of the row (U+001F).
pub(super) const CELL_SEP: char = '\u{1f}';

/// The alignment key text of `text`: runs of white space become one space, the ends are trimmed, curly quotes are
/// straight and the non-breaking space is a space.
pub(super) fn key_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        let c = match c {
            '\u{201c}' | '\u{201d}' => '"',
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{a0}' => ' ',
            c => c,
        };
        if c.is_whitespace() {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

/// A 64-bit FNV-1a hash of `bytes` with the fixed seed.
/// True for a character that may stand for an atom (`model::Atoms`): a private-use character.
pub(super) fn is_atom(c: char) -> bool {
    matches!(c, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{10FFFD}')
}

pub(super) fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = SEED;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// The hash of a block of the given kind tag and key text. The tag keeps a table from anchoring a paragraph.
pub(super) fn hash_keyed(tag: u8, key: &str) -> u64 {
    let mut h = SEED;
    h ^= u64::from(tag);
    h = h.wrapping_mul(PRIME);
    for &b in key.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(PRIME);
    }
    h
}

/// The word bag of a key text: a sorted vector of hashes of its tokens, lowercased. A token is a run of letters and
/// digits, or one other character that is not white space. Duplicates are kept, so the bag is a multiset.
pub(super) fn bag(key: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Vec<u64>| {
        if !word.is_empty() {
            out.push(hash_bytes(word.as_bytes()));
            word.clear();
        }
    };
    for c in key.chars() {
        if c.is_alphanumeric() {
            word.extend(c.to_lowercase());
        } else {
            flush(&mut word, &mut out);
            if !c.is_whitespace() {
                let mut buf = [0u8; 4];
                out.push(hash_bytes(c.encode_utf8(&mut buf).as_bytes()));
            }
        }
    }
    flush(&mut word, &mut out);
    out.sort_unstable();
    out
}

/// How many tokens of the bag are words (letters and digits). Used by the move rule, which wants five words.
pub(super) fn word_count(key: &str) -> usize {
    let mut n = 0;
    let mut in_word = false;
    for c in key.chars() {
        if c.is_alphanumeric() {
            if !in_word {
                n += 1;
                in_word = true;
            }
        } else {
            in_word = false;
        }
    }
    n
}
