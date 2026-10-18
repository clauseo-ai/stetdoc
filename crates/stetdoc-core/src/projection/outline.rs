//! The headings of a story: which source they come from, their levels, what each covers, and how many fit.
//!
//! One source is chosen for the whole document, so a bold word in a styled document does not become a
//! heading (design 4.3): the outline levels of the paragraph styles if the body has three paragraphs with
//! one, else short paragraphs at the first level of a list if there are three, else short bold lines that
//! look like headings if there are two, and else runs of blocks, which is a page index. The constants are
//! starting values for the corpus run, which reports the source each document lands on.

use super::build::Top;
use super::build::scan::{ParaStat, SHORT, Scan};
use super::model::OutlineSource;
use super::page::weight;
use crate::api::error::StetError;
use std::sync::Arc;

/// The fewest headings of a source for the source to be used, by source.
const STYLED_AT: usize = 3;
const NUMBERED_AT: usize = 3;
const BOLD_AT: usize = 2;
/// The deepest level of an outline.
const MAX_LEVEL: u32 = 6;
/// The tokens that a chunk of a page index holds, about.
const CHUNK_TOKENS: u32 = 1_500;
/// How many characters of a heading are shown.
const SHOWN: usize = 80;

/// One heading, or one chunk, and what it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The top where it starts.
    pub top: usize,
    /// The entry of its paragraph, or of the first block of a chunk.
    pub entry: usize,
    /// The level, from 1.
    pub level: u32,
    /// The number Word shows for it.
    pub label: Option<Box<str>>,
    /// Its text, cut short.
    pub text: Box<str>,
    /// The top after the last one it covers.
    pub end: usize,
    /// How many tops it covers that exist in the mode.
    pub blocks: u32,
    /// How many characters it covers.
    pub chars: u32,
}

/// Every entry of a story at every level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Full {
    /// Where the headings came from.
    pub source: OutlineSource,
    /// The entries, in order.
    pub entries: Vec<Entry>,
}

/// The words that open a heading of a contract, and the level each is at.
const WORDS: [(&str, u32); 7] = [
    ("article", 1),
    ("part", 1),
    ("section", 2),
    ("schedule", 1),
    ("exhibit", 1),
    ("annex", 1),
    ("appendix", 1),
];

/// True if `token` is a number or a numeral: digits with dots, a Roman numeral, or one capital letter.
fn is_numeral(token: &str) -> bool {
    let token = token.trim_end_matches([':', '.', ')', '-', '\u{2013}', '\u{2014}']);
    if token.is_empty() {
        return false;
    }
    let digits = token.chars().all(|c| c.is_ascii_digit() || c == '.');
    let roman = token.chars().all(|c| "IVXLCDMivxlcdm".contains(c));
    let letter = token.chars().count() == 1 && token.chars().all(|c| c.is_ascii_alphabetic());
    (digits && token.starts_with(|c: char| c.is_ascii_digit())) || roman || letter
}

/// `ARTICLE I`, `Section 1.01`, `PART 2`: the level of the heading, if the text opens with a word and a number.
fn word_heading(text: &str) -> Option<u32> {
    let mut words = text.split_whitespace();
    let first = words
        .next()?
        .trim_end_matches([':', '.'])
        .to_ascii_lowercase();
    let level = WORDS.iter().find(|(w, _)| *w == first)?.1;
    is_numeral(words.next()?).then_some(level)
}

/// `1.`, `1)`, `1.01`, `2.3.1`: the number of parts, if the text opens with a typed number and then a space or
/// nothing. A bare number with nothing after it (`2020 was a year`) is a number in a sentence and not a label.
fn typed_number(text: &str) -> Option<u32> {
    let token = text.split_whitespace().next()?;
    let core = token.trim_end_matches(['.', ')']);
    let delimited = core.len() < token.len() || core.contains('.');
    if !delimited
        || core.is_empty()
        || !core
            .split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    u32::try_from(core.split('.').count()).ok()
}

fn is_caps(text: &str) -> bool {
    text.chars().any(char::is_alphabetic) && !text.chars().any(char::is_lowercase)
}

/// The level of a short bold line that looks like a heading, or `None` if it is not one.
fn bold_level(p: &ParaStat, bold: bool) -> Option<u32> {
    if p.in_table || p.chars > SHORT || p.text.is_empty() {
        return None;
    }
    let word = word_heading(&p.text);
    let typed = typed_number(&p.text);
    let pattern = word.is_some() || typed.is_some();
    let ends_like_prose = p.text.ends_with(['.', ';', ',', ':']);
    let counts = (bold && (pattern || !ends_like_prose)) || (word.is_some() && is_caps(&p.text));
    counts.then(|| word.or(typed).unwrap_or(1).clamp(1, MAX_LEVEL))
}

fn cut(text: &str) -> Box<str> {
    if text.chars().count() <= SHOWN {
        return text.into();
    }
    let mut out: String = text.chars().take(SHOWN).collect();
    out.push('…');
    out.into()
}

/// The entries of a story, from the one source that its paragraphs support. `first_text` gives the text of
/// the first block of a top that has any, for the names of chunks.
pub(crate) fn headings(
    scan: &Scan,
    tops: &[Top],
    bold: &dyn Fn() -> Result<Arc<Vec<bool>>, StetError>,
    first_text: &dyn Fn(usize) -> String,
) -> Result<Full, StetError> {
    let styled: Vec<(&ParaStat, u32)> = scan
        .paras
        .iter()
        .filter(|p| !p.in_table && !p.text.is_empty())
        .filter_map(|p| Some((p, u32::from(p.outline_level?) + 1)))
        .collect();
    if styled.len() >= STYLED_AT {
        return Ok(build(OutlineSource::Styles, scan, tops, styled));
    }
    let numbered: Vec<(&ParaStat, u32)> = scan
        .paras
        .iter()
        .filter(|p| p.list0 && !p.in_table && p.chars <= SHORT && !p.text.is_empty())
        .map(|p| (p, 1))
        .collect();
    if numbered.len() >= NUMBERED_AT {
        return Ok(build(OutlineSource::Numbered, scan, tops, numbered));
    }
    let flags = bold()?;
    let lines: Vec<(&ParaStat, u32)> = scan
        .paras
        .iter()
        .zip(flags.iter())
        .filter_map(|(p, b)| Some((p, bold_level(p, *b)?)))
        .collect();
    if lines.len() >= BOLD_AT {
        return Ok(build(OutlineSource::BoldLines, scan, tops, lines));
    }
    Ok(chunks(scan, tops, first_text))
}

/// The entries for headings with their levels, with what each covers.
fn build(source: OutlineSource, scan: &Scan, tops: &[Top], heads: Vec<(&ParaStat, u32)>) -> Full {
    let mut entries: Vec<Entry> = Vec::new();
    // A heading in a collapsed field or a table top is not a heading: only one entry for each top.
    for (p, level) in heads {
        let top = p.top as usize;
        if entries.last().is_some_and(|e| e.top == top) {
            continue;
        }
        entries.push(Entry {
            top,
            entry: p.entry as usize,
            level,
            label: p.label.clone(),
            text: cut(&p.text),
            end: tops.len(),
            blocks: 0,
            chars: 0,
        });
    }
    if let Some(first) = entries.first().map(|e| e.top)
        && first > 0
        && scan.tops.iter().take(first).any(|t| t.chars > 0)
    {
        let start = tops.first().map_or(0, |t| t.first);
        entries.insert(
            0,
            Entry {
                top: 0,
                entry: start,
                level: 1,
                label: None,
                text: "(before the first heading)".into(),
                end: tops.len(),
                blocks: 0,
                chars: 0,
            },
        );
    }
    sizes(&mut entries, scan);
    Full { source, entries }
}

/// Fills in what each entry covers: from its top to the next entry of the same level or a shallower one.
fn sizes(entries: &mut [Entry], scan: &Scan) {
    let ends: Vec<usize> = (0..entries.len())
        .map(|i| {
            let level = entries.get(i).map_or(1, |e| e.level);
            entries
                .iter()
                .skip(i + 1)
                .find(|e| e.level <= level)
                .map_or(scan.tops.len(), |e| e.top)
        })
        .collect();
    for (entry, end) in entries.iter_mut().zip(ends) {
        entry.end = end;
        let covered = scan.tops.get(entry.top..end).unwrap_or_default();
        entry.blocks =
            u32::try_from(covered.iter().filter(|t| t.lines > 0).count()).unwrap_or(u32::MAX);
        entry.chars = covered.iter().fold(0u32, |a, t| a.saturating_add(t.chars));
    }
}

/// A page index: runs of whole tops of about the same size, each named by its first block.
fn chunks(scan: &Scan, tops: &[Top], first_text: &dyn Fn(usize) -> String) -> Full {
    let mut entries: Vec<Entry> = Vec::new();
    let mut start = 0usize;
    let mut sum = 0u32;
    for (t, stat) in scan.tops.iter().enumerate() {
        sum = sum.saturating_add(weight(stat));
        let last = t + 1 == scan.tops.len();
        if sum >= CHUNK_TOKENS || last {
            let first = tops.get(start).map_or(0, |top| top.first);
            let name: String = collapse_name(&first_text(start));
            entries.push(Entry {
                top: start,
                entry: first,
                level: 1,
                label: None,
                text: name.into(),
                end: t + 1,
                blocks: 0,
                chars: 0,
            });
            start = t + 1;
            sum = 0;
        }
    }
    sizes(&mut entries, scan);
    Full {
        source: OutlineSource::Chunks,
        entries,
    }
}

/// The first sixty characters of a block's text, for the name of a chunk.
fn collapse_name(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let joined = words.join(" ");
    let mut out: String = joined.chars().take(60).collect();
    if joined.chars().count() > 60 {
        out.push('…');
    }
    if out.is_empty() {
        "(blank)".to_string()
    } else {
        out
    }
}

impl Full {
    /// The deepest level of any entry.
    pub(crate) fn deepest(&self) -> u32 {
        self.entries.iter().map(|e| e.level).max().unwrap_or(1)
    }

    /// The entries down to `depth`, each with how many entries under it are left out.
    pub(crate) fn shown(&self, depth: u32) -> Vec<(&Entry, u32)> {
        let mut out = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if e.level > depth {
                continue;
            }
            let hidden = if e.level == depth {
                let below = self
                    .entries
                    .iter()
                    .skip(i + 1)
                    .take_while(|x| x.level > e.level)
                    .count();
                u32::try_from(below).unwrap_or(u32::MAX)
            } else {
                0
            };
            out.push((e, hidden));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_headings_and_typed_numbers_are_told_from_prose() {
        assert_eq!(word_heading("ARTICLE I: DEFINITIONS"), Some(1));
        assert_eq!(word_heading("Section 1.01 Defined Terms"), Some(2));
        assert_eq!(word_heading("PART 2"), Some(1));
        assert_eq!(word_heading("Schedule A"), Some(1));
        assert_eq!(word_heading("Section of the act"), None);
        assert_eq!(word_heading("The Part 2"), None);
        assert_eq!(typed_number("1. PURPOSE"), Some(1));
        assert_eq!(typed_number("1.01 Terms"), Some(2));
        assert_eq!(typed_number("2.3.1 Detail"), Some(3));
        assert_eq!(typed_number("5) Payment"), Some(1));
        assert_eq!(typed_number("2020 was a year"), None);
        assert_eq!(typed_number("5"), None);
        assert_eq!(typed_number("1.a"), None);
        assert_eq!(typed_number("Item 1"), None);
    }

    fn bold(text: &str) -> ParaStat {
        ParaStat {
            chars: u32::try_from(text.chars().count()).unwrap(),
            text: text.into(),
            ..ParaStat::default()
        }
    }

    #[test]
    fn a_bold_line_is_a_heading_unless_it_reads_as_a_sentence() {
        assert_eq!(bold_level(&bold("DEFINITIONS"), true), Some(1));
        assert_eq!(bold_level(&bold("Section 1.01: Terms"), true), Some(2));
        assert_eq!(
            bold_level(&bold("1. Purpose."), true),
            Some(1),
            "a typed number counts"
        );
        assert_eq!(bold_level(&bold("Pay on time."), true), None);
        assert_eq!(
            bold_level(&bold("Warranties"), false),
            None,
            "not bold and no pattern"
        );
        assert_eq!(
            bold_level(&bold("ARTICLE III"), false),
            Some(1),
            "capitals and a pattern"
        );
        assert_eq!(
            bold_level(
                &ParaStat {
                    in_table: true,
                    ..bold("Heading")
                },
                true
            ),
            None
        );
    }
}
