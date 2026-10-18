//! Law F3 (escapes) and the invisible characters.

use super::escape::{EscapeError, escape_quoted, escape_text, read_escape, unescape};
use super::invisible::must_escape;
use crate::projection::testing_gen::{PIECES, Rng};

/// Characters of every class that must be written as an escape, from independent knowledge of Unicode.
const MUST: &[char] = &[
    '\u{0}',
    '\u{1}',
    '\u{8}',
    '\t',
    '\n',
    '\u{b}',
    '\u{c}',
    '\r',
    '\u{1b}',
    '\u{1f}',
    '\u{7f}',
    '\u{80}',
    '\u{85}',
    '\u{9f}', // controls
    '\u{a0}',
    '\u{1680}',
    '\u{2000}',
    '\u{2003}',
    '\u{200a}',
    '\u{202f}',
    '\u{205f}',
    '\u{3000}', // spaces
    '\u{ad}',
    '\u{200b}',
    '\u{200c}',
    '\u{200d}',
    '\u{200e}',
    '\u{200f}',
    '\u{202a}',
    '\u{202e}',
    '\u{2060}',
    '\u{2066}',
    '\u{2069}',
    '\u{feff}',
    '\u{fff9}',
    '\u{fffb}',
    '\u{600}',
    '\u{61c}',
    '\u{180e}', // format
    '\u{2028}',
    '\u{2029}', // separators
    '\u{e000}',
    '\u{f0b7}',
    '\u{f8ff}',
    '\u{f0000}',
    '\u{10fffd}', // private use
    '\u{e0001}',
    '\u{e0041}',
    '\u{e007f}', // tag characters
    '\u{fdd0}',
    '\u{fdef}',
    '\u{fffe}',
    '\u{ffff}',
    '\u{1fffe}',
    '\u{10ffff}', // noncharacters
];

/// Characters that are written as they are: nearly all text.
const PLAIN: &[char] = &[
    'a',
    'Z',
    '0',
    ' ',
    '.',
    ',',
    '"',
    '\'',
    '\u{201c}',
    '\u{201d}',
    '\u{a7}',
    '\u{b6}',
    '\u{20b9}',
    '\u{928}',
    '\u{92e}',
    '\u{e9}',
    '\u{4e2d}',
    '\u{1f600}',
    '-',
    '+',
    '#',
    '|',
    '[',
    ']',
    '~',
    '\u{2022}',
    '\u{b7}',
    '\u{fffc}',
    '\u{fffd}',
];

#[test]
fn every_character_that_a_person_cannot_see_is_written_as_an_escape() {
    for &c in MUST {
        assert!(must_escape(c), "{:04x}", c as u32);
        let escaped = escape_text(&c.to_string());
        assert!(
            escaped.starts_with('\\'),
            "{:04x} is written {escaped:?}",
            c as u32
        );
        assert_eq!(unescape(&escaped, false).unwrap(), c.to_string());
    }
    for &c in PLAIN {
        assert!(!must_escape(c), "{:04x}", c as u32);
        let escaped = escape_text(&c.to_string());
        assert_eq!(escaped, c.to_string(), "{:04x}", c as u32);
    }
}

#[test]
fn the_five_escapes_have_their_spellings() {
    assert_eq!(escape_text("a\\b"), "a\\\\b");
    assert_eq!(escape_text("{x}"), "\\{x\\}");
    assert_eq!(escape_text("a\tb\nc"), "a\\tb\\nc");
    assert_eq!(escape_text("\u{a0}"), "\\u{a0}");
    assert_eq!(escape_text("\u{e0041}"), "\\u{e0041}");
    assert_eq!(
        escape_text("\"q\""),
        "\"q\"",
        "a text slot has no use for \\\""
    );
    assert_eq!(
        escape_quoted("say \"hi\" {now}"),
        "\"say \\\"hi\\\" \\{now\\}\""
    );
    assert_eq!(escape_text("§ ¶ ₹ नम"), "§ ¶ ₹ नम");
}

#[test]
fn every_string_comes_back_and_no_escaped_text_holds_a_line_end_or_a_forbidden_character() {
    let mut rng = Rng::new(11);
    for _ in 0..2000 {
        let n = 1 + rng.below(12);
        let mut s = String::new();
        for _ in 0..n {
            s.push_str(rng.word(PIECES));
        }
        for quoted in [false, true] {
            let escaped = if quoted {
                let q = escape_quoted(&s);
                q[1..q.len() - 1].to_string()
            } else {
                escape_text(&s)
            };
            assert_eq!(unescape(&escaped, quoted).unwrap(), s, "{s:?}");
            for c in escaped.chars() {
                assert!(
                    !matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}'),
                    "{escaped:?}"
                );
                assert!(!c.is_control(), "{escaped:?}");
                assert!(!must_escape(c), "{:04x} raw in {escaped:?}", c as u32);
            }
        }
    }
}

#[test]
fn every_character_class_round_trips_alone_and_in_company() {
    let all: String = MUST.iter().chain(PLAIN).collect();
    assert_eq!(unescape(&escape_text(&all), false).unwrap(), all);
    assert_eq!(
        unescape(&escape_text(&all.repeat(50)), false).unwrap(),
        all.repeat(50)
    );
}

#[test]
fn an_escape_that_is_not_one_of_the_five_is_an_error_and_never_a_literal_backslash() {
    for (bad, want) in [
        ("\\q", EscapeError::Unknown('q')),
        ("\\.", EscapeError::Unknown('.')),
        ("\\$", EscapeError::Unknown('$')),
        ("\\\"", EscapeError::Unknown('"')),
        ("abc\\", EscapeError::Dangling),
        ("\\u", EscapeError::BadUnicode),
        ("\\u41", EscapeError::BadUnicode),
        ("\\u{}", EscapeError::BadUnicode),
        ("\\u{41", EscapeError::BadUnicode),
        ("\\u{1234567}", EscapeError::BadUnicode),
        ("\\u{xyz}", EscapeError::BadUnicode),
        ("\\u{110000}", EscapeError::NotScalar),
        ("\\u{d800}", EscapeError::NotScalar),
    ] {
        assert_eq!(unescape(bad, false).unwrap_err(), want, "{bad:?}");
    }
    assert_eq!(unescape("\\\"", true).unwrap(), "\"", "inside quotes");
    let message = EscapeError::Unknown('q').message();
    assert!(
        message.contains("\\q") && message.contains("literal backslash"),
        "{message}"
    );
}

#[test]
fn an_escape_for_a_character_that_needs_none_is_read_as_that_character() {
    assert_eq!(unescape("\\u{41}\\u{1f600}", false).unwrap(), "A\u{1f600}");
    assert_eq!(read_escape("x\\u{a0}y", 1, false).unwrap(), ('\u{a0}', 7));
}

#[test]
fn a_raw_character_of_the_forbidden_set_is_read_as_itself() {
    assert_eq!(
        unescape("a\u{a0}b\u{200b}", false).unwrap(),
        "a\u{a0}b\u{200b}"
    );
}
