//! Attribute value checking: evaluates a generated value specification against a string.
//!
//! A specification is an OR of clauses and a clause is an AND of predicates. The predicate
//! codes below are the ones `xtask/src/codegen/values.rs` prints (`Pred::code`).

use super::generated::attrs;
use super::tables::slice;

const P_ANY: u8 = 0;
const P_INT: u8 = 1;
const P_HEX: u8 = 2;
const P_ONOFF: u8 = 3;
const P_BOOL: u8 = 4;
const P_DATETIME: u8 = 5;
const P_BASE64: u8 = 6;
const P_ENUM: u8 = 7;
const P_PATTERN: u8 = 8;
const P_LEN: u8 = 9;

/// Whether `value` satisfies value specification `spec` (an index into `SPECS`).
pub(super) fn accepts(spec: u16, value: &str) -> bool {
    let Some(&(off, len)) = attrs::SPECS.get(usize::from(spec)) else {
        return true;
    };
    let clauses = slice(&attrs::SPEC_CLAUSES, off as usize, usize::from(len));
    if clauses.is_empty() {
        return true;
    }
    clauses.iter().any(|&c| clause_holds(c, value))
}

fn clause_holds(clause: u16, value: &str) -> bool {
    let Some(&(off, len)) = attrs::CLAUSES.get(usize::from(clause)) else {
        return true;
    };
    slice(&attrs::CLAUSE_PREDS, off as usize, usize::from(len))
        .iter()
        .all(|&p| {
            attrs::PREDS
                .get(usize::from(p))
                .is_none_or(|&(k, a, b)| pred_holds(k, a, b, value))
        })
}

fn pred_holds(kind: u8, a: i64, b: i64, value: &str) -> bool {
    let trimmed = value.trim_matches(is_xml_space);
    match kind {
        P_ANY => true,
        P_INT => parse_int(trimmed).is_some_and(|v| v >= i128::from(a) && v <= i128::from(b)),
        P_HEX => {
            is_hex_binary(trimmed).is_some_and(|bytes| (bytes as i64) >= a && (bytes as i64) <= b)
        }
        P_ONOFF => matches!(trimmed, "true" | "false" | "on" | "off" | "0" | "1"),
        P_BOOL => matches!(trimmed, "true" | "false" | "0" | "1"),
        P_DATETIME => is_date_time(trimmed),
        P_BASE64 => is_base64(value),
        P_ENUM => enum_has(a, trimmed),
        P_PATTERN => pattern_matches(a, value),
        P_LEN => {
            let n = value.chars().count() as i64;
            n >= a && n <= b
        }
        _ => true,
    }
}

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

fn parse_int(s: &str) -> Option<i128> {
    let (neg, digits) = match s.as_bytes().first()? {
        b'-' => (true, s.get(1..)?),
        b'+' => (false, s.get(1..)?),
        _ => (false, s),
    };
    if digits.is_empty() || digits.len() > 30 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let v: i128 = digits.parse().ok()?;
    Some(if neg { -v } else { v })
}

/// Number of bytes of an `xsd:hexBinary` value, or `None` if it is not one.
fn is_hex_binary(s: &str) -> Option<usize> {
    (s.len() & 1 == 0 && s.bytes().all(|c| c.is_ascii_hexdigit())).then_some(s.len() / 2)
}

fn enum_has(id: i64, value: &str) -> bool {
    let Some(&(off, len)) = usize::try_from(id)
        .ok()
        .and_then(|i| attrs::ENUM_SETS.get(i))
    else {
        return true;
    };
    slice(&attrs::ENUM_VALUES, off as usize, usize::from(len))
        .binary_search_by(|v| (*v).cmp(value))
        .is_ok()
}

fn is_base64(s: &str) -> bool {
    let body: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if body.len() & 3 != 0 {
        return false;
    }
    let pad = body.iter().rev().take_while(|c| **c == b'=').count();
    pad <= 2
        && body
            .iter()
            .take(body.len() - pad)
            .all(|c| c.is_ascii_alphanumeric() || *c == b'+' || *c == b'/')
}

/// `-?YYYY-MM-DDThh:mm:ss(.s+)?(Z|(+|-)hh:mm)?`
fn is_date_time(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'-') {
        i += 1;
    }
    let digits = |from: usize, n: usize| {
        b.get(from..from + n)
            .is_some_and(|d| d.iter().all(u8::is_ascii_digit))
    };
    let mut year_len = 0;
    while b.get(i + year_len).is_some_and(u8::is_ascii_digit) {
        year_len += 1;
    }
    if year_len < 4 {
        return false;
    }
    i += year_len;
    let fixed = |i: usize, sep: u8, n: usize| b.get(i) == Some(&sep) && digits(i + 1, n);
    if !(fixed(i, b'-', 2)
        && fixed(i + 3, b'-', 2)
        && fixed(i + 6, b'T', 2)
        && fixed(i + 9, b':', 2)
        && fixed(i + 12, b':', 2))
    {
        return false;
    }
    i += 15;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let start = i;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match b.get(i) {
        None => true,
        Some(b'Z') => i + 1 == b.len(),
        Some(b'+' | b'-') => {
            b.len() == i + 6 && digits(i + 1, 2) && b.get(i + 3) == Some(&b':') && digits(i + 4, 2)
        }
        _ => false,
    }
}

/// The patterns the SDK data uses, in the order of `PATTERNS` in the generator.
fn pattern_matches(id: i64, value: &str) -> bool {
    match id {
        0 => is_length(value, false),
        1 => value.bytes().all(|c| c.is_ascii_hexdigit()),
        2 => is_length(value, true),
        3 => {
            let v = value.strip_suffix('%').unwrap_or("\u{0}");
            is_decimal(v.strip_prefix('-').unwrap_or(v))
        }
        4 => is_braced_guid(value),
        5 => value.bytes().all(|c| c == b'0' || c == b'1'),
        6 => !value.contains(','),
        7 => value.strip_suffix('%').is_some_and(|d| {
            !d.is_empty()
                && d.bytes().all(|c| c.is_ascii_digit())
                && d.trim_start_matches('0').len() <= 3
                && d.parse::<u32>().is_ok_and(|n| n <= 600)
        }),
        _ => true,
    }
}

fn is_decimal(s: &str) -> bool {
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (s, None),
    };
    !int.is_empty()
        && int.bytes().all(|c| c.is_ascii_digit())
        && frac.is_none_or(|f| !f.is_empty() && f.bytes().all(|c| c.is_ascii_digit()))
}

/// `[0-9]+(\.[0-9]+)?(mm|cm|in|pt|pc|pi)`, optionally signed.
fn is_length(s: &str, signed: bool) -> bool {
    let s = if signed {
        s.strip_prefix('-').unwrap_or(s)
    } else {
        s
    };
    ["mm", "cm", "in", "pt", "pc", "pi"]
        .iter()
        .any(|u| s.strip_suffix(u).is_some_and(is_decimal))
}

fn is_braced_guid(s: &str) -> bool {
    let Some(inner) = s.strip_prefix('{').and_then(|r| r.strip_suffix('}')) else {
        return false;
    };
    let parts: Vec<&str> = inner.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == 5
        && parts.iter().zip(lens).all(|(p, n)| {
            p.len() == n
                && p.bytes()
                    .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(&c))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_parsers() {
        assert_eq!(parse_int("-12"), Some(-12));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int("1.5"), None);
        assert_eq!(parse_int(""), None);
        assert_eq!(is_hex_binary("00A1B2C3"), Some(4));
        assert_eq!(is_hex_binary("0A1"), None);
        assert!(is_date_time("2024-01-02T10:11:12Z"));
        assert!(is_date_time("2024-01-02T10:11:12.5+05:30"));
        assert!(is_date_time("2024-01-02T10:11:12"));
        assert!(!is_date_time("2024-01-02"));
        assert!(!is_date_time("2024-01-02T10:11Z"));
        assert!(is_base64("AAAA"));
        assert!(is_base64("QQ=="));
        assert!(!is_base64("QQ="));
    }

    #[test]
    fn patterns() {
        assert!(pattern_matches(0, "12.5pt"));
        assert!(!pattern_matches(0, "-12pt"));
        assert!(pattern_matches(2, "-12pt"));
        assert!(pattern_matches(3, "-50.5%"));
        assert!(!pattern_matches(3, "50"));
        assert!(pattern_matches(4, "{0F3A2B1C-0000-4000-8000-000000000001}"));
        assert!(!pattern_matches(
            4,
            "{0f3a2b1c-0000-4000-8000-000000000001}"
        ));
        assert!(pattern_matches(5, "010"));
    }
}
