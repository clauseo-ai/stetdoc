//! Escapes: injective and visible (design 5.4).
//!
//! A text slot, and every quoted string, is written with five escapes and read back with the same five:
//! `\\`, `\{` and `\}`, `\t`, `\n`, and `\u{2f}` with one to six hexadecimal digits for any other
//! character that must not appear raw. A quoted string also writes `\"`. `unescape(escape(s)) == s`
//! for every string, and no escaped text holds a character that ends a line, so no edit can turn text
//! into a new paragraph by accident. An escape that is not one of these is an error and is never read
//! as a literal backslash.

use super::invisible::must_escape;

/// Writes one character the way a text slot or a quoted string writes it.
pub(super) fn escape_char(c: char, quoted: bool, out: &mut String) {
    match c {
        '\\' => out.push_str("\\\\"),
        '{' => out.push_str("\\{"),
        '}' => out.push_str("\\}"),
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '"' if quoted => out.push_str("\\\""),
        c if must_escape(c) => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
        c => out.push(c),
    }
}

/// A string as a text slot writes it.
pub(super) fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        escape_char(c, false, &mut out);
    }
    out
}

/// A string as a quoted string writes it, with the quotes.
pub(super) fn escape_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        escape_char(c, true, &mut out);
    }
    out.push('"');
    out
}

/// Why an escape could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EscapeError {
    /// A backslash at the end of the text.
    Dangling,
    /// A backslash and a character that is not an escape.
    Unknown(char),
    /// `\u` and something that is not `{`, one to six hexadecimal digits and `}`.
    BadUnicode,
    /// `\u{..}` with a value that is not a Unicode scalar value.
    NotScalar,
}

impl EscapeError {
    /// What is wrong, in a sentence that names the fix.
    pub(super) fn message(&self) -> String {
        match self {
            EscapeError::Dangling => {
                "a backslash ends the line. Write \\\\ for a literal backslash".to_string()
            }
            EscapeError::Unknown(c) => format!(
                "\\{c} is not an escape. The escapes are \\\\, \\{{, \\}}, \\t, \\n and \\u{{2f}}, and \\\" inside quotes. Write \\\\ for a literal backslash"
            ),
            EscapeError::BadUnicode => {
                "\\u must be followed by { one to six hexadecimal digits } such as \\u{a0}"
                    .to_string()
            }
            EscapeError::NotScalar => "\\u{..} must name a Unicode scalar value".to_string(),
        }
    }
}

/// Reads the escape that starts at byte `at` of `s`, where `s` has a backslash at `at`. Returns the
/// character and the byte offset after the escape.
pub(super) fn read_escape(s: &str, at: usize, quoted: bool) -> Result<(char, usize), EscapeError> {
    let rest = s.get(at + 1..).unwrap_or("");
    let mut chars = rest.chars();
    let Some(c) = chars.next() else {
        return Err(EscapeError::Dangling);
    };
    let after = at + 1 + c.len_utf8();
    let plain = match c {
        '\\' => '\\',
        '{' => '{',
        '}' => '}',
        't' => '\t',
        'n' => '\n',
        '"' if quoted => '"',
        'u' => return read_unicode(s, after),
        other => return Err(EscapeError::Unknown(other)),
    };
    Ok((plain, after))
}

fn read_unicode(s: &str, at: usize) -> Result<(char, usize), EscapeError> {
    let rest = s.get(at..).unwrap_or("");
    let Some(inner) = rest.strip_prefix('{') else {
        return Err(EscapeError::BadUnicode);
    };
    let Some(end) = inner.find('}') else {
        return Err(EscapeError::BadUnicode);
    };
    let digits = inner.get(..end).unwrap_or("");
    if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(EscapeError::BadUnicode);
    }
    let value = u32::from_str_radix(digits, 16).map_err(|_| EscapeError::BadUnicode)?;
    let c = char::from_u32(value).ok_or(EscapeError::NotScalar)?;
    Ok((c, at + 1 + end + 1))
}

/// The inverse of `escape_text` and, with `quoted`, of the inside of `escape_quoted`: reads a whole
/// string. A raw brace is an error, because a raw brace is a marker.
pub(super) fn unescape(s: &str, quoted: bool) -> Result<String, EscapeError> {
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    while let Some(rest) = s.get(at..) {
        let Some(c) = rest.chars().next() else {
            break;
        };
        if c == '\\' {
            let (decoded, next) = read_escape(s, at, quoted)?;
            out.push(decoded);
            at = next;
        } else {
            out.push(c);
            at += c.len_utf8();
        }
    }
    Ok(out)
}
