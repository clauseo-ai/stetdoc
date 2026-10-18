//! The spelling of a ref in a file: `p42`, `t3`, `r17`.
//!
//! `BlockRef::parse` is the only parser of the token (refs-and-state 3.1), and this file is the only place
//! the format decides where a ref can start a line. The knob `refs` of design 5.7 (a check digit, a
//! content hash) would change only this file.

use crate::api::error::StetError;
use crate::refs::BlockRef;

/// How a ref is written.
pub(super) fn write_ref(r: BlockRef) -> String {
    r.to_string()
}

/// If a line starts with a ref, the token and the rest of the line after it. A line starts with a ref when
/// its first character is `p`, `t` or `r`, digits follow, and then the line ends or a space follows:
/// `page 4` and `r2d2` do not start with a ref, `p07` and `p0` do and are refused by `BlockRef::parse`.
pub(super) fn split_ref(line: &str) -> Option<(&str, &str)> {
    let mut chars = line.chars();
    let first = chars.next()?;
    if !matches!(first, 'p' | 't' | 'r') {
        return None;
    }
    let digits = chars.as_str();
    let n = digits.bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return None;
    }
    let end = 1 + n;
    let rest = line.get(end..)?;
    if rest.is_empty() || rest.starts_with(' ') {
        Some((line.get(..end)?, rest))
    } else {
        None
    }
}

/// Reads a token that `split_ref` found.
pub(super) fn read_ref(token: &str) -> Result<BlockRef, StetError> {
    BlockRef::parse(token)
}
