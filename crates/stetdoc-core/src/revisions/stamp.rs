//! Who is editing, and when: the author and date of a revision, checked once so that every
//! revision element the compiler writes carries an author and a date that Word reads (design
//! `revisions.md`, 5.2 and 5.3).
//!
//! The author is checked, never defaulted. The date is **validated, not converted**: it is the
//! wall-clock time of the date the caller gives, as Word writes `w:date` (the author's local clock
//! labelled `Z`), so an offset or a fraction is dropped and the digits are kept as they are.

use crate::api::error::{ErrorCode, StetError};

/// The longest author, in characters. The schema type of `w:author` is a string of at most 255.
pub(crate) const MAX_AUTHOR_CHARS: usize = 255;

/// The longest date text that is looked at. A valid date with a long fraction is far shorter.
const MAX_DATE_BYTES: usize = 64;

/// Whether an edit is recorded, and by whom. There is no default stamp, as in the public interface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Track {
    /// Every edit becomes a revision with this author and date.
    Tracked(RevStamp),
    /// Edits are applied directly and write no revision. No author is known, so every pending change in the document is
    /// another author's, and an edit that would rewrite one is refused (`foreign.rs`).
    Untracked,
    /// Edits are applied directly and write no revision, as this author's: the author's own pending changes are edited in
    /// place, and another author's are refused (`foreign.rs`).
    UntrackedBy(String),
}

/// An author and a date that are known to be writable: the author is non-empty, at most 255
/// characters and free of control characters, and the date is a real date and time of day.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RevStamp {
    author: String,
    date: String,
}

impl RevStamp {
    /// Checks the author and the date. `date` is `YYYY-MM-DDTHH:MM:SS` with an optional fraction and
    /// an optional `Z` or `+HH:MM` or `-HH:MM`. The stamp holds the first 19 characters followed by
    /// `Z`. Anything else is `invalid_payload`.
    pub(crate) fn new(author: &str, date: &str) -> Result<RevStamp, StetError> {
        check_author(author)?;
        Ok(RevStamp {
            author: author.to_owned(),
            date: normalize_date(date)?,
        })
    }

    /// The author, as written to `w:author`.
    pub(crate) fn author(&self) -> &str {
        &self.author
    }

    /// The date as written to `w:date`: `YYYY-MM-DDTHH:MM:SSZ`.
    pub(crate) fn date(&self) -> &str {
        &self.date
    }
}

fn invalid(message: String, next: &str) -> StetError {
    StetError::new(ErrorCode::InvalidPayload, message).with_next(next)
}

/// The author rules: not empty, at most 255 characters, no control character and no character that
/// XML cannot carry.
fn check_author(author: &str) -> Result<(), StetError> {
    let count = author.chars().count();
    if count == 0 {
        return Err(invalid(
            "the author of a revision is empty".to_owned(),
            "give the name that Word should show for the author of the change",
        ));
    }
    if count > MAX_AUTHOR_CHARS {
        return Err(invalid(
            format!(
                "the author of a revision has {count} characters, and the most is {MAX_AUTHOR_CHARS}"
            ),
            "give a shorter author name",
        ));
    }
    if author
        .chars()
        .any(|c| c.is_control() || c == '\u{FFFE}' || c == '\u{FFFF}')
    {
        return Err(invalid(
            "the author of a revision has a control character".to_owned(),
            "give an author name made of printable characters",
        ));
    }
    Ok(())
}

/// A cursor over the bytes of a date.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    /// `n` ASCII digits as a number, or none.
    fn digits(&mut self, n: usize) -> Option<u32> {
        let mut value = 0u32;
        for _ in 0..n {
            let b = *self.bytes.get(self.at)?;
            if !b.is_ascii_digit() {
                return None;
            }
            value = value * 10 + u32::from(b - b'0');
            self.at += 1;
        }
        Some(value)
    }

    /// Takes `byte` if it is next.
    fn eat(&mut self, byte: u8) -> bool {
        if self.bytes.get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }
}

fn is_leap(year: u32) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        _ => 28,
    }
}

/// Checks a date and returns it as `YYYY-MM-DDTHH:MM:SSZ`: the first 19 characters, then `Z`. The
/// date is not converted to another time zone.
pub(crate) fn normalize_date(date: &str) -> Result<String, StetError> {
    let bad = || {
        invalid(
            format!("{date:?} is not a date and time that a revision can carry"),
            "give an ISO 8601 date and time such as 2026-10-05T14:03:27, with an optional fraction and an optional Z or offset",
        )
    };
    if date.len() > MAX_DATE_BYTES {
        return Err(bad());
    }
    let mut c = Cursor {
        bytes: date.as_bytes(),
        at: 0,
    };
    let year = c.digits(4).ok_or_else(bad)?;
    let ok = c.eat(b'-');
    let month = c.digits(2).ok_or_else(bad)?;
    let ok = ok && c.eat(b'-');
    let day = c.digits(2).ok_or_else(bad)?;
    let ok = ok && c.eat(b'T');
    let hour = c.digits(2).ok_or_else(bad)?;
    let ok = ok && c.eat(b':');
    let minute = c.digits(2).ok_or_else(bad)?;
    let ok = ok && c.eat(b':');
    let second = c.digits(2).ok_or_else(bad)?;
    if !ok
        || year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(bad());
    }
    if c.eat(b'.') {
        let before = c.at;
        while c.bytes.get(c.at).is_some_and(u8::is_ascii_digit) {
            c.at += 1;
        }
        if c.at == before {
            return Err(bad());
        }
    }
    if !c.eat(b'Z') && (c.bytes.get(c.at) == Some(&b'+') || c.bytes.get(c.at) == Some(&b'-')) {
        c.at += 1;
        let offset_hour = c.digits(2).ok_or_else(bad)?;
        if !c.eat(b':') {
            return Err(bad());
        }
        let offset_minute = c.digits(2).ok_or_else(bad)?;
        if offset_hour > 14 || offset_minute > 59 || (offset_hour == 14 && offset_minute != 0) {
            return Err(bad());
        }
    }
    if !c.done() {
        return Err(bad());
    }
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z"
    ))
}
