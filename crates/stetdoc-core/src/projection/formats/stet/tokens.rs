//! Reading a marker: where it ends, and the tokens inside it.
//!
//! A marker is a raw `{`, a body and the next raw `}`. A raw brace never appears inside a marker's text,
//! because a text that a marker shows is a quoted string and every brace in it is escaped, but an
//! escape `\u{2f}` has braces of its own, so the scan skips escapes as units. The body is a list of
//! tokens separated by single spaces: a bare word, a quoted string, or `name=` and a bare word or a
//! quoted string. A quoted string may follow a word with no space (`§"1.1"`).

use super::escape::{read_escape, unescape};

/// A token of a marker's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Tok {
    /// A bare word.
    Word(String),
    /// A quoted string, decoded.
    Quoted(String),
    /// `name=value`.
    Named(String, Box<Tok>),
}

/// The longest text a marker may show, in characters.
pub(super) const MAX_PAYLOAD: usize = 65_536;

/// The byte offset of the raw `}` that ends the marker that opens at `open`, where `s` has a `{` at
/// `open`. An escape is skipped whole.
pub(super) fn marker_end(s: &str, open: usize) -> Result<usize, String> {
    let mut at = open + 1;
    while let Some(rest) = s.get(at..) {
        let Some(c) = rest.chars().next() else {
            break;
        };
        match c {
            '}' => return Ok(at),
            '{' => return Err("a { inside a marker. Write \\{ for a literal brace".to_string()),
            '\\' => {
                let (_, next) = read_escape(s, at, true).map_err(|e| e.message())?;
                at = next;
            }
            _ => at += c.len_utf8(),
        }
    }
    Err("a marker that is not closed with }".to_string())
}

/// The tokens of a marker's body.
pub(super) fn tokenize(body: &str) -> Result<Vec<Tok>, String> {
    let mut toks = Vec::new();
    let mut at = 0;
    while let Some(rest) = body.get(at..) {
        if rest.is_empty() {
            break;
        }
        if let Some(after) = rest.strip_prefix(' ') {
            at += 1;
            if after.is_empty() || after.starts_with(' ') {
                return Err("two spaces in a row inside a marker".to_string());
            }
            continue;
        }
        let (tok, next) = token(body, at)?;
        toks.push(tok);
        at = next;
    }
    Ok(toks)
}

fn token(body: &str, at: usize) -> Result<(Tok, usize), String> {
    let rest = body.get(at..).unwrap_or("");
    if rest.starts_with('"') {
        return quoted(body, at).map(|(text, next)| (Tok::Quoted(text), next));
    }
    let end = rest.find([' ', '"', '=']).unwrap_or(rest.len());
    let word = rest.get(..end).unwrap_or("").to_string();
    let after = at + end;
    match body.get(after..).and_then(|r| r.chars().next()) {
        Some('=') => {
            let value_at = after + 1;
            let value_rest = body.get(value_at..).unwrap_or("");
            if value_rest.is_empty() || value_rest.starts_with(' ') {
                return Err(format!("{word}= has no value"));
            }
            let (value, next) = if value_rest.starts_with('"') {
                quoted(body, value_at).map(|(text, next)| (Tok::Quoted(text), next))?
            } else {
                let stop = value_rest.find([' ', '"']).unwrap_or(value_rest.len());
                if value_rest.get(stop..).is_some_and(|r| r.starts_with('"')) {
                    return Err(format!("a quote in the middle of the value of {word}="));
                }
                (
                    Tok::Word(value_rest.get(..stop).unwrap_or("").to_string()),
                    value_at + stop,
                )
            };
            Ok((Tok::Named(word, Box::new(value)), next))
        }
        _ => Ok((Tok::Word(word), after)),
    }
}

/// A quoted string that starts at `at`: the decoded text and the offset after the closing quote.
fn quoted(body: &str, at: usize) -> Result<(String, usize), String> {
    let mut i = at + 1;
    let start = i;
    while let Some(rest) = body.get(i..) {
        let Some(c) = rest.chars().next() else {
            break;
        };
        match c {
            '"' => {
                let inner = body.get(start..i).unwrap_or("");
                let text = unescape(inner, true).map_err(|e| e.message())?;
                if text.chars().count() > MAX_PAYLOAD {
                    return Err(format!("a quoted text over {MAX_PAYLOAD} characters"));
                }
                let after = i + 1;
                let next = body.get(after..).and_then(|r| r.chars().next());
                if next.is_some_and(|n| n != ' ') {
                    return Err(
                        "a quoted string followed by something that is not a space".to_string()
                    );
                }
                return Ok((text, after));
            }
            '\\' => {
                let (_, next) = read_escape(body, i, true).map_err(|e| e.message())?;
                i = next;
            }
            _ => i += c.len_utf8(),
        }
    }
    Err("a quoted string that does not end".to_string())
}
