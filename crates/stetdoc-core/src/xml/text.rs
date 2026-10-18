//! Escaping and unescaping of XML character data, and the character rules for new content.

use super::errors::invalid;
use crate::api::error::StetError;
use std::borrow::Cow;

/// The XML 1.0 `Char` production: tab, line feed, carriage return, and everything from space up
/// except the surrogates (which `char` already excludes), U+FFFE and U+FFFF.
pub(crate) fn is_xml_char(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

/// Refuses text that XML cannot carry. Word does not open a part that holds such a character, so
/// the editor never writes one, whether literally or as a reference.
fn check_chars(s: &str) -> Result<(), StetError> {
    match s.chars().find(|&c| !is_xml_char(c)) {
        Some(c) => Err(invalid(format!(
            "U+{:04X} is not a character XML can carry, so it cannot be written",
            u32::from(c)
        ))),
        None => Ok(()),
    }
}

/// Escapes new text content. Non-ASCII characters stay literal unless the part is `us-ascii`. A
/// carriage return becomes a character reference so that it survives a parse. A character XML
/// cannot carry is `invalid_payload`.
pub(crate) fn escape_text(s: &str, ascii: bool) -> Result<String, StetError> {
    check_chars(s)?;
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            c if ascii && !c.is_ascii() => out.push_str(&format!("&#x{:X};", u32::from(c))),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    Ok(out)
}

/// What may be written outside the root element: white space, as itself. A reference is not allowed
/// there, so a carriage return is written as it is, and reads as a line feed. Anything else is
/// `invalid_payload`.
pub(crate) fn whitespace_only(s: &str) -> Result<&str, StetError> {
    match s.chars().find(|c| !matches!(c, ' ' | '\t' | '\r' | '\n')) {
        Some(c) => Err(invalid(format!(
            "text outside the root element has to be white space, written as itself, and this has U+{:04X}",
            u32::from(c)
        ))),
        None => Ok(s),
    }
}

/// Appends `piece` to `out`, where each is raw text that reads as intended on its own. Where they
/// touch, the bytes can read differently, so the join is written so that they do not:
/// - a carriage return that ends `out` and a line feed that starts `piece` would read as one line
///   break, where the tree has two (a carriage return by itself reads as a line feed). The carriage
///   return is written as the line feed it reads as, and so is every carriage return right before
///   it, since each of them is a line break of its own and would pair with the line feed that
///   now follows it.
/// - `]]` and `>` would make `]]>`, which XML does not allow in content. The `>` is written as
///   `&gt;`, which reads as the same character.
///
/// Two text nodes meet like this when an edit removes what was between them, and a splice does when
/// it removes or adds text next to a carriage return or a `]`.
pub(crate) fn join_bytes(out: &mut Vec<u8>, piece: &[u8]) {
    match piece.first() {
        Some(b'\n') if out.last() == Some(&b'\r') => {
            let run = out.iter().rev().take_while(|&&b| b == b'\r').count();
            out.truncate(out.len().saturating_sub(run));
            out.extend(std::iter::repeat_n(b'\n', run));
            out.extend_from_slice(piece);
        }
        Some(b'>') if out.ends_with(b"]]") => {
            out.extend_from_slice(b"&gt;");
            out.extend_from_slice(piece.get(1..).unwrap_or(&[]));
        }
        // The other way to split `]]>` across the join: one `]` before it, `]>` after it.
        Some(b']') if out.ends_with(b"]") && piece.get(1) == Some(&b'>') => {
            out.extend_from_slice(b"]&gt;");
            out.extend_from_slice(piece.get(2..).unwrap_or(&[]));
        }
        _ => out.extend_from_slice(piece),
    }
}

/// Escapes a new attribute value for the given quote byte. Tab, newline and carriage return
/// become character references, because a parser would normalise them to spaces. A character XML
/// cannot carry is `invalid_payload`.
pub(crate) fn escape_attr(s: &str, quote: u8, ascii: bool) -> Result<String, StetError> {
    check_chars(s)?;
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            c if ascii && !c.is_ascii() => out.push_str(&format!("&#x{:X};", u32::from(c))),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if quote == b'"' => out.push_str("&quot;"),
            '\'' if quote == b'\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    Ok(out)
}

/// `NameStartChar` of XML 1.0 (fifth edition), without the colon.
fn is_name_start(c: char) -> bool {
    matches!(
        c,
        'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
            | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
            | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
            | '\u{10000}'..='\u{EFFFF}'
    )
}

/// `NameChar` of XML 1.0 (fifth edition), without the colon.
fn is_name_char(c: char) -> bool {
    is_name_start(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}')
}

/// True for a name an element, attribute or prefix may have when the editor creates it: a
/// `NCName`, which is an XML name without a colon.
pub(super) fn is_ncname(s: &str) -> bool {
    let mut it = s.chars();
    it.next().is_some_and(is_name_start) && it.all(is_name_char)
}

/// Decodes one reference body (the text between `&` and `;`).
fn decode_ref(body: &str) -> Option<char> {
    match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let n = if let Some(h) = body.strip_prefix("#x") {
                u32::from_str_radix(h, 16).ok()?
            } else {
                body.strip_prefix('#')?.parse::<u32>().ok()?
            };
            char::from_u32(n).filter(|&c| is_xml_char(c))
        }
    }
}

/// Checks that every `&` in `raw` starts a well-formed reference to a predefined entity or a
/// character. Returns the offset of the first bad one. No other entity can be defined,
/// because DOCTYPE is rejected.
pub(crate) fn check_references(raw: &[u8]) -> Result<(), usize> {
    let mut from = 0usize;
    while let Some(i) = memchr::memchr(b'&', raw.get(from..).unwrap_or(&[])) {
        let at = from + i;
        let tail = raw.get(at + 1..).unwrap_or(&[]);
        let limit = tail.len().min(12);
        let semi = memchr::memchr(b';', tail.get(..limit).unwrap_or(&[])).ok_or(at)?;
        let body = std::str::from_utf8(tail.get(..semi).unwrap_or(&[])).map_err(|_| at)?;
        decode_ref(body).ok_or(at)?;
        from = at + 1 + semi + 1;
    }
    Ok(())
}

/// The offset of the first `]]>` in the text of an element, which XML does not allow there: only
/// the end of a CDATA section is written so, and a `>` is written `&gt;` where it follows `]]`.
/// Most text has no `>` at all, which is one search.
pub(crate) fn find_cdata_end(raw: &[u8]) -> Option<usize> {
    memchr::memchr_iter(b'>', raw).find_map(|i| {
        let start = i.checked_sub(2)?;
        (raw.get(start..i) == Some(b"]]".as_slice())).then_some(start)
    })
}

/// What a decoded character is when it is written in the source.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Text, CDATA and comments: a carriage return, alone or before a line feed, reads as one
    /// line feed.
    Text,
    /// Attribute values: as `Text`, and then a tab or line feed written literally reads as a space.
    Attr,
}

/// Decodes references and applies the line-ending and attribute-value rules of XML 1.0, on text
/// already checked by `check_references`. A reference that does not decode is kept as written. A
/// character written as a reference is never normalized.
fn decode(raw: &str, mode: Mode) -> Cow<'_, str> {
    let bytes = raw.as_bytes();
    let changes = match mode {
        // `\n` only matters after a `\r`, which is found on its own.
        Mode::Text => memchr::memchr2(b'&', b'\r', bytes).is_some(),
        Mode::Attr => {
            memchr::memchr3(b'&', b'\r', b'\n', bytes).is_some()
                || memchr::memchr(b'\t', bytes).is_some()
        }
    };
    if !changes {
        return Cow::Borrowed(raw);
    }
    let literal_ws = if mode == Mode::Attr { ' ' } else { '\n' };
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        match c {
            '&' => match rest
                .find(';')
                .and_then(|s| Some((s, decode_ref(rest.get(1..s)?)?)))
            {
                Some((s, d)) => {
                    out.push(d);
                    rest = rest.get(s + 1..).unwrap_or("");
                }
                None => {
                    out.push('&');
                    rest = rest.get(1..).unwrap_or("");
                }
            },
            '\r' => {
                out.push(literal_ws);
                rest = rest.get(1..).unwrap_or("");
                if let Some(after) = rest.strip_prefix('\n') {
                    rest = after;
                }
            }
            '\t' | '\n' if mode == Mode::Attr => {
                out.push(' ');
                rest = rest.get(1..).unwrap_or("");
            }
            c => {
                out.push(c);
                rest = rest.get(c.len_utf8()..).unwrap_or("");
            }
        }
    }
    Cow::Owned(out)
}

/// The characters of text, CDATA or a comment as a parser reports them.
pub(crate) fn unescape_text(raw: &str) -> Cow<'_, str> {
    decode(raw, Mode::Text)
}

/// The value of an attribute as a parser reports it.
pub(crate) fn unescape_attr(raw: &str) -> Cow<'_, str> {
    decode(raw, Mode::Attr)
}

/// The content of a CDATA section or a comment as a parser reports it: references are not
/// decoded there, but line endings are normalized as in text.
pub(crate) fn normalize_line_endings(raw: &str) -> Cow<'_, str> {
    if !raw.contains('\r') {
        return Cow::Borrowed(raw);
    }
    Cow::Owned(raw.replace("\r\n", "\n").replace('\r', "\n"))
}

/// Maps a range of decoded characters to the raw byte range that encodes them, counting each
/// reference as one character, and a carriage return line feed pair, or a carriage return alone,
/// as one character (the way `unescape_text` reports them).
pub(crate) fn raw_range_for_chars(raw: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    if start > end {
        return None;
    }
    let mut count = 0usize;
    let (mut a, mut b) = (None, None);
    let mut i = 0usize;
    while i <= raw.len() {
        if count == start && a.is_none() {
            a = Some(i);
        }
        if count == end {
            b = Some(i);
            break;
        }
        if i == raw.len() {
            break;
        }
        let rest = raw.get(i..)?;
        let step = if rest.starts_with('&') {
            rest.find(';').map_or(1, |s| s + 1)
        } else if rest.starts_with("\r\n") {
            2
        } else {
            rest.chars().next()?.len_utf8()
        };
        i += step;
        count += 1;
    }
    Some((a?, b?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes() {
        assert_eq!(
            escape_text("a&b<c>d\r\u{a0}", false).unwrap(),
            "a&amp;b&lt;c&gt;d&#13;\u{a0}"
        );
        assert_eq!(
            escape_text("\u{e9}\u{1F600}", true).unwrap(),
            "&#xE9;&#x1F600;"
        );
        assert_eq!(
            escape_attr("a\"b'\n", b'"', false).unwrap(),
            "a&quot;b'&#10;"
        );
        assert_eq!(escape_attr("a\"b'", b'\'', false).unwrap(), "a\"b&apos;");
    }

    #[test]
    fn characters_xml_cannot_carry_are_refused() {
        for bad in [
            "\u{0}", "a\u{1}b", "\u{8}", "\u{b}", "\u{c}", "\u{1f}", "\u{FFFE}", "\u{FFFF}",
        ] {
            let e = escape_text(bad, false).unwrap_err();
            assert_eq!(
                e.code(),
                crate::api::error::ErrorCode::InvalidPayload,
                "{bad:?}"
            );
            assert!(escape_attr(bad, b'"', false).is_err(), "{bad:?}");
            assert!(escape_text(bad, true).is_err(), "{bad:?}");
        }
        for ok in [
            "\t\n\r",
            " ",
            "\u{7f}",
            "\u{80}",
            "\u{9f}",
            "\u{D7FF}",
            "\u{E000}",
            "\u{FFFD}",
            "\u{10000}",
            "\u{10FFFF}",
        ] {
            assert!(escape_text(ok, false).is_ok(), "{ok:?}");
            assert!(escape_attr(ok, b'"', false).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn name_rules_follow_xml_1_0() {
        for ok in [
            "w",
            "p1",
            "_a",
            "a-b.c",
            "caf\u{e9}",
            "\u{4e2d}\u{6587}",
            "a\u{b7}b",
            "\u{10000}x",
        ] {
            assert!(is_ncname(ok), "{ok:?}");
        }
        for bad in [
            "",
            "1a",
            "-a",
            ".a",
            "a:b",
            "a b",
            "a\u{d7}",
            "\u{a0}",
            "a\u{FFFF}",
            "a\u{1}",
            "\u{b7}a",
        ] {
            assert!(!is_ncname(bad), "{bad:?}");
        }
    }

    #[test]
    fn unescapes_and_checks() {
        assert_eq!(unescape_text("a&amp;b&#x41;&#66;&quot;"), "a&bAB\"");
        assert!(check_references(b"a&amp;b&#xA0;").is_ok());
        assert!(check_references(b"&nbsp;").is_err());
        assert!(check_references(b"a & b").is_err());
        assert!(check_references(b"&#0;").is_err());
        assert!(check_references(b"&#xD800;").is_err());
        assert!(check_references(b"&#xFFFF;").is_err());
        assert!(check_references(b"&#1;").is_err());
    }

    #[test]
    fn line_endings_and_attribute_whitespace_follow_xml_1_0() {
        assert_eq!(unescape_text("a\r\nb\rc\nd"), "a\nb\nc\nd");
        assert_eq!(unescape_text("a&#13;\nb&#10;"), "a\r\nb\n");
        assert!(matches!(unescape_text("plain"), Cow::Borrowed(_)));
        assert_eq!(unescape_attr("a\r\nb\rc\nd\te"), "a b c d e");
        assert_eq!(unescape_attr("a&#9;b&#10;c&#13;d"), "a\tb\nc\rd");
        assert!(matches!(unescape_attr("plain value"), Cow::Borrowed(_)));
    }

    #[test]
    fn cdata_and_comment_content_only_has_its_line_endings_normalized() {
        assert_eq!(normalize_line_endings("a&amp;\r\nb\rc"), "a&amp;\nb\nc");
        assert!(matches!(normalize_line_endings("a&b"), Cow::Borrowed(_)));
    }

    #[test]
    fn char_ranges_skip_references() {
        let raw = "a&#xA0;bc";
        assert_eq!(raw_range_for_chars(raw, 1, 2), Some((1, 7)));
        assert_eq!(raw_range_for_chars(raw, 2, 4), Some((7, 9)));
        assert_eq!(raw_range_for_chars(raw, 0, 4), Some((0, 9)));
        assert_eq!(raw_range_for_chars(raw, 5, 5), None);
    }

    #[test]
    fn char_ranges_count_a_line_ending_as_one_character() {
        let raw = "a\r\nb\rc";
        assert_eq!(unescape_text(raw).chars().count(), 5);
        assert_eq!(raw_range_for_chars(raw, 1, 2), Some((1, 3)));
        assert_eq!(raw_range_for_chars(raw, 2, 3), Some((3, 4)));
        assert_eq!(raw_range_for_chars(raw, 3, 4), Some((4, 5)));
        assert_eq!(raw_range_for_chars(raw, 0, 5), Some((0, 6)));
    }
}
