//! The needles of a find: smart case, how many times a text holds each, and the text around a match.
//!
//! A needle is case-insensitive unless it has an uppercase letter. A find in the blocks and a find in the ledger both
//! read their needles here, so a needle means the same in both.

use crate::api::error::{ErrorCode, StetError};
use std::ops::Range;

/// One needle, as given and as it is matched.
#[derive(Debug)]
struct Needle {
    /// The text as it was given.
    given: String,
    /// The text that is looked for: lowercased when the match ignores case.
    wanted: String,
    /// The match ignores case.
    fold: bool,
}

/// The needles of a find, in the order given, each once.
#[derive(Debug, Default)]
pub(crate) struct Needles(Vec<Needle>);

impl Needles {
    /// Reads the needles. A blank one is `invalid_payload`, and `what` names the find in its message, such as `a find`.
    pub(crate) fn parse(given: &[String], what: &str) -> Result<Needles, StetError> {
        let mut out: Vec<Needle> = Vec::new();
        for text in given {
            if text.trim().is_empty() {
                return Err(StetError::new(
                    ErrorCode::InvalidPayload,
                    format!("{what} needs some text to look for, and one of its texts is blank."),
                )
                .with_next("Give the text to find, or leave out the find."));
            }
            if out.iter().any(|n| n.given == *text) {
                continue;
            }
            let fold = !text.chars().any(char::is_uppercase);
            out.push(Needle {
                given: text.clone(),
                wanted: if fold {
                    text.to_lowercase()
                } else {
                    text.clone()
                },
                fold,
            });
        }
        Ok(Needles(out))
    }

    /// There are no needles.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many needles there are.
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    /// The needles as they were given, in order.
    pub(crate) fn given(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(|n| n.given.as_str())
    }

    /// True if the text holds any of the needles.
    pub(crate) fn any_in(&self, text: &str) -> bool {
        let lowered = self.folds().then(|| text.to_lowercase());
        self.0.iter().any(|n| {
            let hay = if n.fold {
                lowered.as_deref().unwrap_or(text)
            } else {
                text
            };
            hay.contains(n.wanted.as_str())
        })
    }

    /// How many times the text holds each needle, in the order of the needles, without overlaps.
    pub(crate) fn counts(&self, text: &str) -> Vec<u32> {
        let lowered = self.folds().then(|| text.to_lowercase());
        self.0
            .iter()
            .map(|n| {
                let hay = if n.fold {
                    lowered.as_deref().unwrap_or(text)
                } else {
                    text
                };
                u32::try_from(hay.matches(n.wanted.as_str()).count()).unwrap_or(u32::MAX)
            })
            .collect()
    }

    /// The bytes of the text that the first match of any needle covers.
    pub(crate) fn first_match(&self, text: &str) -> Option<Range<usize>> {
        let folded = self.folds().then(|| fold(text));
        self.0
            .iter()
            .filter_map(|n| {
                if n.fold {
                    let (lowered, map) = folded.as_ref()?;
                    let at = lowered.find(n.wanted.as_str())?;
                    let end = at + n.wanted.len();
                    Some(*map.get(at)?..*map.get(end)?)
                } else {
                    let at = text.find(n.wanted.as_str())?;
                    Some(at..at + n.wanted.len())
                }
            })
            .min_by_key(|r| r.start)
    }

    fn folds(&self) -> bool {
        self.0.iter().any(|n| n.fold)
    }
}

/// The text lowercased, and for each of its bytes and the end, the byte of `text` it came from, so that a match in the
/// lowercased text is a range of the original even where lowercasing changes the length of a character. A match that
/// ends inside the lowercase form of one character ends before that character.
fn fold(text: &str) -> (String, Vec<usize>) {
    let mut lowered = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    let mut buf = [0u8; 4];
    for (at, c) in text.char_indices() {
        for low in c.to_lowercase() {
            lowered.push(low);
            map.extend(std::iter::repeat_n(at, low.encode_utf8(&mut buf).len()));
        }
    }
    map.push(text.len());
    (lowered, map)
}

/// A piece of a text around a match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Snippet {
    /// The piece, exactly as the text has it.
    pub text: String,
    /// The text goes on before the piece.
    pub cut_before: bool,
    /// The text goes on after the piece.
    pub cut_after: bool,
}

/// About `width` characters of `text` around the bytes `at`, the match in the middle, cut at spaces where that loses
/// little. A text no longer than `width` is whole.
pub(crate) fn snippet(text: &str, at: Range<usize>, width: usize) -> Snippet {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    if n <= width {
        return Snippet {
            text: text.to_string(),
            cut_before: false,
            cut_after: false,
        };
    }
    let first = chars.partition_point(|(b, _)| *b < at.start);
    let last = chars.partition_point(|(b, _)| *b < at.end).max(first);
    let inside = last - first;
    let room = width.saturating_sub(inside);
    let mut lo = first.saturating_sub(room / 2);
    let mut hi = (last + room - (first - lo)).min(n);
    if hi == n {
        lo = n.saturating_sub(width.max(inside)).min(first);
    }
    // Start after a space and end before one, if that gives up at most a sixth of the room.
    let slack = (width / 6).max(1);
    if lo > 0
        && let Some(k) = (lo..first.min(lo + slack)).find(|&k| is_space(&chars, k))
    {
        lo = k + 1;
    }
    if hi < n
        && let Some(k) = (last.max(hi.saturating_sub(slack))..hi)
            .rev()
            .find(|&k| is_space(&chars, k))
    {
        hi = k;
    }
    let byte = |k: usize| chars.get(k).map_or(text.len(), |(b, _)| *b);
    Snippet {
        text: text.get(byte(lo)..byte(hi)).unwrap_or_default().to_string(),
        cut_before: lo > 0,
        cut_after: hi < n,
    }
}

fn is_space(chars: &[(usize, char)], k: usize) -> bool {
    chars.get(k).is_some_and(|(_, c)| *c == ' ')
}

#[cfg(test)]
mod tests {
    use super::{Needles, snippet};

    fn needles(list: &[&str]) -> Needles {
        let given: Vec<String> = list.iter().map(|s| (*s).to_string()).collect();
        Needles::parse(&given, "a find").unwrap()
    }

    #[test]
    fn smart_case_holds_for_each_needle() {
        let n = needles(&["days", "Due Date"]);
        assert_eq!(n.counts("Days and days; due date, Due Date"), vec![2, 1]);
        assert!(n.any_in("DAYS"));
        assert!(!n.any_in("due date"));
    }

    #[test]
    fn a_blank_needle_is_refused_and_a_repeated_one_counts_once() {
        let err = Needles::parse(&["x".into(), " ".into()], "a find").unwrap_err();
        assert!(err.message().contains("blank"), "{}", err.message());
        assert_eq!(needles(&["x", "x"]).len(), 1);
    }

    #[test]
    fn the_first_match_is_a_range_of_the_original_text() {
        // `İ` is two bytes and lowercases to three, so the offsets of the lowercased text are not the original's.
        let text = "İİ then Days, then days";
        let at = needles(&["days", "then"]).first_match(text).unwrap();
        assert_eq!(&text[at], "then");
        let at = needles(&["days"]).first_match(text).unwrap();
        assert_eq!(&text[at.clone()], "Days", "{at:?}");
        let at = needles(&["days", "Days"])
            .first_match("x days Days")
            .unwrap();
        assert_eq!(at, 2..6);
    }

    #[test]
    fn a_snippet_is_exact_text_around_the_match() {
        let text = "one two three four five six seven eight nine ten eleven twelve thirteen";
        let at = text.find("seven").unwrap();
        let s = snippet(text, at..at + 5, 30);
        assert!(text.contains(&s.text));
        assert!(s.text.contains("seven"), "{s:?}");
        assert!(s.cut_before && s.cut_after);
        assert!(s.text.chars().count() <= 30, "{s:?}");
        assert!(!s.text.starts_with(' ') && !s.text.ends_with(' '), "{s:?}");
        let whole = snippet("short", 0..1, 30);
        assert!(!whole.cut_before && !whole.cut_after);
        let end = text.find("thirteen").unwrap();
        let s = snippet(text, end..end + 8, 30);
        assert!(s.text.ends_with("thirteen") && !s.cut_after, "{s:?}");
    }
}
