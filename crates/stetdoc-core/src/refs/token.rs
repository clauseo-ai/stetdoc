//! The ref token: a kind letter and a decimal number, such as `p42`, `t3` or `r17`.
//!
//! This is the only parser and the only writer of the token, so its spelling can change in one
//! place. Projection formats may wrap a ref in their own syntax, but must call [`BlockRef::parse`]
//! on what they extract.

use crate::api::error::{ErrorCode, StetError};
use crate::wordml::BlockKind;
use std::fmt;
use std::num::NonZeroU32;

/// What a ref names. A cell is not a block and has no ref.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) enum RefKind {
    /// A paragraph: `p`.
    Paragraph,
    /// A table: `t`.
    Table,
    /// A table row: `r`.
    Row,
}

impl RefKind {
    /// The kinds in the order refs are sorted and persisted: `p`, `t`, `r`.
    pub(crate) const ALL: [RefKind; 3] = [RefKind::Paragraph, RefKind::Table, RefKind::Row];

    /// The kind of ref a block gets.
    pub(crate) fn of(block: BlockKind) -> RefKind {
        match block {
            BlockKind::Paragraph => RefKind::Paragraph,
            BlockKind::Table => RefKind::Table,
            BlockKind::Row => RefKind::Row,
        }
    }

    /// The letter that starts a ref of this kind.
    pub(crate) fn letter(self) -> char {
        match self {
            RefKind::Paragraph => 'p',
            RefKind::Table => 't',
            RefKind::Row => 'r',
        }
    }

    /// The kind a letter names.
    pub(crate) fn from_letter(letter: char) -> Option<RefKind> {
        match letter {
            'p' => Some(RefKind::Paragraph),
            't' => Some(RefKind::Table),
            'r' => Some(RefKind::Row),
            _ => None,
        }
    }

    /// The position of the kind in [`RefKind::ALL`], for per-kind arrays.
    pub(crate) fn index(self) -> usize {
        match self {
            RefKind::Paragraph => 0,
            RefKind::Table => 1,
            RefKind::Row => 2,
        }
    }

    /// What the kind is called in a message.
    pub(crate) fn noun(self) -> &'static str {
        match self {
            RefKind::Paragraph => "paragraphs",
            RefKind::Table => "tables",
            RefKind::Row => "rows",
        }
    }
}

/// A ref: a kind and a number from 1 to 4,294,967,295. The order is by kind (`p`, `t`, `r`) and then
/// by number.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) struct BlockRef {
    kind: RefKind,
    number: NonZeroU32,
}

impl BlockRef {
    /// The ref of this kind and number, or `None` for number 0.
    pub(crate) fn new(kind: RefKind, number: u32) -> Option<BlockRef> {
        NonZeroU32::new(number).map(|number| BlockRef { kind, number })
    }

    /// What the ref names.
    pub(crate) fn kind(self) -> RefKind {
        self.kind
    }

    /// The number, from 1.
    pub(crate) fn number(self) -> u32 {
        self.number.get()
    }

    /// Reads a ref exactly as [`fmt::Display`] writes it: `[ptr]` and then a decimal number with no
    /// sign, no leading zero and no whitespace, at most 4,294,967,295. Anything else, `p0`, `p007`,
    /// `P42`, `p-1` and `banana` among it, is `invalid_payload`.
    pub(crate) fn parse(token: &str) -> Result<BlockRef, StetError> {
        let mut chars = token.chars();
        let kind = chars.next().and_then(RefKind::from_letter);
        let digits = chars.as_str();
        let parsed = kind.and_then(|kind| {
            let canonical = !digits.is_empty()
                && digits.len() <= 10
                && !digits.starts_with('0')
                && digits.bytes().all(|b| b.is_ascii_digit());
            let number = canonical.then(|| digits.parse::<u32>().ok()).flatten()?;
            BlockRef::new(kind, number)
        });
        parsed.ok_or_else(|| not_a_ref(token))
    }
}

impl fmt::Display for BlockRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}", self.kind.letter(), self.number)
    }
}

/// The token as a message shows it: quoted, and cut short so that hostile input cannot make a
/// message large.
pub(crate) fn shown(token: &str) -> String {
    const LIMIT: usize = 32;
    let mut out: String = token.chars().take(LIMIT).collect();
    let cut = token.chars().nth(LIMIT).is_some();
    out = format!("{out:?}");
    if cut {
        out.push_str(" (cut short)");
    }
    out
}

fn not_a_ref(token: &str) -> StetError {
    StetError::new(
        ErrorCode::InvalidPayload,
        format!(
            "{} is not a ref. A ref is p, t or r followed by a number, such as p42.",
            shown(token)
        ),
    )
    .with_next("Use a ref exactly as a view shows it, such as p42, t3 or r17.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(token: &str) -> BlockRef {
        BlockRef::parse(token).unwrap()
    }

    #[test]
    fn a_ref_is_a_letter_and_a_number() {
        let p = ok("p42");
        assert_eq!((p.kind(), p.number()), (RefKind::Paragraph, 42));
        assert_eq!(ok("t3").kind(), RefKind::Table);
        assert_eq!(ok("r17").kind(), RefKind::Row);
        assert_eq!(ok("p1").number(), 1);
        assert_eq!(ok("p4294967295").number(), u32::MAX);
    }

    #[test]
    fn display_is_the_inverse_of_parse() {
        for token in ["p1", "p42", "t3", "r17", "p4294967295", "r1000000"] {
            assert_eq!(ok(token).to_string(), token);
        }
    }

    #[test]
    fn every_other_spelling_is_invalid_payload() {
        for bad in [
            "",
            "p",
            "p0",
            "p007",
            "P42",
            "p-1",
            "p+1",
            " p42",
            "p42 ",
            "p4 2",
            "p42.0",
            "banana",
            "x9",
            "pp1",
            "p4294967296",
            "p99999999999",
            "pé",
            "p١",
            "\u{0}",
            "p1\n",
        ] {
            let e = BlockRef::parse(bad).unwrap_err();
            assert_eq!(e.code(), ErrorCode::InvalidPayload, "{bad:?}");
            assert!(e.message().contains("p42"), "{bad:?}: {}", e.message());
        }
    }

    #[test]
    fn refs_sort_by_kind_and_then_number() {
        let mut v = [ok("r1"), ok("t9"), ok("p10"), ok("p2"), ok("t1")];
        v.sort();
        let sorted: Vec<String> = v.iter().map(ToString::to_string).collect();
        assert_eq!(sorted, ["p2", "p10", "t1", "t9", "r1"]);
    }

    #[test]
    fn a_hostile_token_cannot_make_a_large_message() {
        let long = "p".repeat(100_000);
        let e = BlockRef::parse(&long).unwrap_err();
        assert!(e.message().len() < 200, "{}", e.message().len());
    }

    #[test]
    fn the_kinds_are_listed_in_sort_order_with_their_letters() {
        let letters: String = RefKind::ALL.iter().map(|k| k.letter()).collect();
        assert_eq!(letters, "ptr");
        for (i, kind) in RefKind::ALL.iter().enumerate() {
            assert_eq!(kind.index(), i);
            assert_eq!(RefKind::from_letter(kind.letter()), Some(*kind));
        }
        assert_eq!(RefKind::Row.noun(), "rows");
        assert_eq!(RefKind::from_letter('x'), None);
    }

    #[test]
    fn number_zero_is_not_a_ref() {
        assert!(BlockRef::new(RefKind::Paragraph, 0).is_none());
        assert_eq!(BlockRef::new(RefKind::Row, 5).unwrap().to_string(), "r5");
    }
}
