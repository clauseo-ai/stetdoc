//! The word-level difference of a text edit.
//!
//! A tracked replacement is the difference between the old text of a range and the new text,
//! cut into tokens and cleaned so that dense edits read as one replacement (design `revisions.md`,
//! 2.2 step 2 and 2.5). Everything here is pure: text in, edits out, no document and no clock, and
//! the same input always gives the same edits. `compare` shares it.
//!
//! **Tokens.** A token is a run of letters and digits, a run of spaces, or one other character
//! (punctuation, a tab, a line break, an atom). A combining mark, a joiner or a variation selector
//! stays with the character before it, and so does the character after a zero width joiner, so an
//! edit never cuts a grapheme. Edits begin and end between tokens.
//!
//! **Matching.** The common prefix and suffix are trimmed. The rest is matched on the tokens that
//! occur exactly once on each side, kept in the longest increasing order, and the stretches between
//! those anchors are trimmed and matched again. A stretch with no anchor is matched by the longest
//! common subsequence, if it has at most [`DP_CELLS_MAX`] cells and the work budget
//! ([`DP_CELLS_TOTAL`]) allows it. A stretch that is over either limit is one edit.
//!
//! **Cleaning.** With [`Cleanup::Readable`] the edits are cleaned by the rules of 2.5: an unchanged
//! island of no word, or of at most two words that is no longer than the larger of the two edits
//! around it, is dissolved into them, until nothing changes, and then, if more than half of the
//! words between the first and the last edit are inside edits, the whole span is one edit.
//! [`Cleanup::Minimal`] keeps the matching as it is, which is what the noise measure of `compare`
//! and the tests use as the denominator. Cleaning only merges edits and never moves an outer end,
//! and it never leaves two edits without an unchanged island between them.

use std::collections::HashMap;
use std::ops::Range;

/// The most cells (old tokens times new tokens) that one stretch may have for the longest common
/// subsequence. A stretch with more is one edit.
pub(crate) const DP_CELLS_MAX: usize = 250_000;

/// The most cells that all the stretches of one difference may use together. The stretches that
/// would pass it are one edit each, so the cost of a difference is bounded whatever the input.
pub(crate) const DP_CELLS_TOTAL: usize = 4_000_000;

/// Whether the edits are cleaned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cleanup {
    /// The matching as it is, with no cleaning rule applied.
    Minimal,
    /// The rules of 2.5, so that dense edits become one replacement.
    Readable,
}

/// The constants of the cleaning rules of 2.5.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cleaning {
    /// An island of at most this many word tokens is dissolved when it is no longer than the larger of the
    /// two edits around it.
    pub island_words: usize,
    /// If the words inside edits are more than `whole.0 / whole.1` of the words between the first edit and
    /// the last, they become one replacement.
    pub whole: (usize, usize),
}

impl Cleaning {
    /// The constants `edits` uses: two tokens, one half. They were starting values, and the 139 pairs that
    /// Word wrote (`tests_calibrate.rs`) say they are right to keep.
    pub(crate) const DEFAULT: Cleaning = Cleaning {
        island_words: 2,
        whole: (1, 2),
    };
}

/// One replacement. `range` counts Unicode scalar values of the old text, and `with` is the new text
/// that takes its place: empty for a deletion, and an empty range for an insertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    /// The scalar values of the old text that are replaced.
    pub range: Range<usize>,
    /// What replaces them.
    pub with: String,
}

/// What a token is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A run of letters and digits.
    Word,
    /// A run of spaces (not a tab or a line break, which are controls).
    Space,
    /// One character: punctuation, a tab, a line break, an atom.
    Other,
}

/// The difference between two texts, as the replacements that turn `old` into `new`.
pub(crate) fn edits(old: &str, new: &str, cleanup: Cleanup) -> Vec<Edit> {
    edits_with(old, new, cleanup, Cleaning::DEFAULT)
}

/// [`edits`] with other constants for the cleaning, which the calibration tries.
pub(crate) fn edits_with(old: &str, new: &str, cleanup: Cleanup, cleaning: Cleaning) -> Vec<Edit> {
    let old_chars: Vec<char> = old.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let old_toks = tokenize(&old_chars);
    let new_toks = tokenize(&new_chars);

    let (old_ids, new_ids) = ids(&old_chars, &old_toks, &new_chars, &new_toks);
    let pairs = match_ids(&old_ids, &new_ids);
    let mut list = raw_edits(&pairs, old_ids.len(), new_ids.len());

    let old_pre = Prefix::new(&old_toks);
    let new_pre = Prefix::new(&new_toks);
    if cleanup == Cleanup::Readable {
        clean(&mut list, &old_pre, &new_pre, cleaning);
    }

    list.iter()
        .map(|e| {
            let from = old_pre.chars_before(e.a0);
            let to = old_pre.chars_before(e.a1);
            let new_from = new_pre.chars_before(e.b0);
            let new_to = new_pre.chars_before(e.b1);
            Edit {
                range: from..to,
                with: new_chars
                    .get(new_from..new_to)
                    .map(|s| s.iter().collect())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// The text that results from applying `edits` to `old`. The edits must be in order and not overlap,
/// as [`edits`] returns them. It is what the tests and the string model use to check a difference.
pub(crate) fn apply(old: &str, edits: &[Edit]) -> String {
    let chars: Vec<char> = old.chars().collect();
    let mut out = String::new();
    let mut at = 0usize;
    for e in edits {
        if let Some(keep) = chars.get(at..e.range.start) {
            out.extend(keep);
        }
        out.push_str(&e.with);
        at = e.range.end;
    }
    if let Some(rest) = chars.get(at..) {
        out.extend(rest);
    }
    out
}

// ---- tokens -----------------------------------------------------------------------------------

/// A mark or joiner that belongs to the character before it.
fn is_joiner(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036F}'
        | '\u{0483}'..='\u{0489}'
        | '\u{0591}'..='\u{05BD}'
        | '\u{0610}'..='\u{061A}'
        | '\u{064B}'..='\u{065F}'
        | '\u{0900}'..='\u{0903}'
        | '\u{093A}'..='\u{094F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{200C}' | '\u{200D}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{3099}' | '\u{309A}'
        | '\u{FE00}'..='\u{FE0F}'
        | '\u{FE20}'..='\u{FE2F}'
        | '\u{1F3FB}'..='\u{1F3FF}'
        | '\u{E0100}'..='\u{E01EF}')
}

fn kind_of(c: char) -> Kind {
    if c.is_alphanumeric() {
        Kind::Word
    } else if c.is_whitespace() && !c.is_control() {
        Kind::Space
    } else {
        Kind::Other
    }
}

/// The tokens of a text: their ranges, in scalar values, and their kinds. The ranges are
/// contiguous and cover the text.
pub(crate) fn tokenize(chars: &[char]) -> Vec<(Range<usize>, Kind)> {
    let mut toks: Vec<(Range<usize>, Kind)> = Vec::new();
    let mut start = 0usize;
    let mut kind = Kind::Other;
    let mut open = false;
    let mut after_zwj = false;
    for (i, &c) in chars.iter().enumerate() {
        let joins = open && (is_joiner(c) || after_zwj);
        if !joins {
            let k = kind_of(c);
            let extends = open && k != Kind::Other && k == kind;
            if !extends {
                if open {
                    toks.push((start..i, kind));
                }
                start = i;
                kind = k;
                open = true;
            }
        }
        after_zwj = c == '\u{200D}';
    }
    if open {
        toks.push((start..chars.len(), kind));
    }
    toks
}

/// Running totals over the tokens of one text, so that the cleaning rules can ask how many
/// characters and words lie between two tokens in constant time.
struct Prefix {
    chars: Vec<usize>,
    words: Vec<usize>,
}

impl Prefix {
    fn new(toks: &[(Range<usize>, Kind)]) -> Prefix {
        let mut chars = Vec::with_capacity(toks.len() + 1);
        let mut words = Vec::with_capacity(toks.len() + 1);
        chars.push(0);
        words.push(0);
        for (r, k) in toks {
            chars.push(r.end);
            words.push(words.last().copied().unwrap_or(0) + usize::from(*k == Kind::Word));
        }
        Prefix { chars, words }
    }

    /// The scalar values before token `t`.
    fn chars_before(&self, t: usize) -> usize {
        self.chars.get(t).copied().unwrap_or(0)
    }

    fn chars_between(&self, from: usize, to: usize) -> usize {
        self.chars_before(to)
            .saturating_sub(self.chars_before(from))
    }

    fn words_between(&self, from: usize, to: usize) -> usize {
        let at = |t: usize| self.words.get(t).copied().unwrap_or(0);
        at(to).saturating_sub(at(from))
    }
}

/// Gives every distinct token text one number, so that matching compares integers.
fn ids(
    old: &[char],
    old_toks: &[(Range<usize>, Kind)],
    new: &[char],
    new_toks: &[(Range<usize>, Kind)],
) -> (Vec<u32>, Vec<u32>) {
    let mut seen: HashMap<&[char], u32> = HashMap::new();
    let a = number(&mut seen, old, old_toks);
    let b = number(&mut seen, new, new_toks);
    (a, b)
}

fn number<'a>(
    seen: &mut HashMap<&'a [char], u32>,
    chars: &'a [char],
    toks: &[(Range<usize>, Kind)],
) -> Vec<u32> {
    let mut out = Vec::with_capacity(toks.len());
    for (r, _) in toks {
        let text: &[char] = chars.get(r.clone()).unwrap_or(&[]);
        let next = u32::try_from(seen.len()).unwrap_or(u32::MAX);
        out.push(*seen.entry(text).or_insert(next));
    }
    out
}

// ---- matching ---------------------------------------------------------------------------------

/// The matched token pairs of two token sequences, in increasing order in both.
fn match_ids(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut budget = DP_CELLS_TOTAL;
    let mut stack: Vec<(usize, usize, usize, usize)> = vec![(0, a.len(), 0, b.len())];
    while let Some((a0, a1, b0, b1)) = stack.pop() {
        let (mut s0, mut t0, mut s1, mut t1) = (a0, b0, a1, b1);
        while s0 < s1 && t0 < t1 && a.get(s0) == b.get(t0) {
            pairs.push((s0, t0));
            s0 += 1;
            t0 += 1;
        }
        while s1 > s0 && t1 > t0 && a.get(s1 - 1) == b.get(t1 - 1) {
            s1 -= 1;
            t1 -= 1;
            pairs.push((s1, t1));
        }
        if s0 >= s1 || t0 >= t1 {
            continue;
        }
        let anchors = anchors(a, b, (s0, s1), (t0, t1));
        if anchors.is_empty() {
            let cells = (s1 - s0).saturating_mul(t1 - t0);
            if cells <= DP_CELLS_MAX && cells <= budget {
                budget -= cells;
                let (sa, sb) = (a.get(s0..s1).unwrap_or(&[]), b.get(t0..t1).unwrap_or(&[]));
                pairs.extend(lcs(sa, sb).into_iter().map(|(i, j)| (s0 + i, t0 + j)));
            }
            continue;
        }
        let (mut from_a, mut from_b) = (s0, t0);
        for (i, j) in anchors {
            stack.push((from_a, i, from_b, j));
            pairs.push((i, j));
            from_a = i + 1;
            from_b = j + 1;
        }
        stack.push((from_a, s1, from_b, t1));
    }
    pairs.sort_unstable();
    pairs
}

/// The tokens that occur exactly once in `a[range]` and exactly once in `b[range]`, as pairs, in the
/// longest order that increases in both.
fn anchors(a: &[u32], b: &[u32], ar: (usize, usize), br: (usize, usize)) -> Vec<(usize, usize)> {
    // (count in a, count in b, position in b of the last one)
    let mut seen: HashMap<u32, (u32, u32, usize)> = HashMap::new();
    for i in ar.0..ar.1 {
        if let Some(&id) = a.get(i) {
            seen.entry(id).or_insert((0, 0, 0)).0 += 1;
        }
    }
    for j in br.0..br.1 {
        if let Some(&id) = b.get(j) {
            let e = seen.entry(id).or_insert((0, 0, 0));
            e.1 += 1;
            e.2 = j;
        }
    }
    let mut candidates: Vec<(usize, usize)> = Vec::new();
    for i in ar.0..ar.1 {
        if let Some(&(1, 1, j)) = a.get(i).and_then(|id| seen.get(id)) {
            candidates.push((i, j));
        }
    }
    longest_increasing(&candidates)
}

/// The longest subsequence of `pairs` (already in increasing first element) whose second elements
/// increase. Of equal length the one that ends earliest is kept.
fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let second = |k: usize| pairs.get(k).map_or(usize::MAX, |p| p.1);
    let mut tails: Vec<usize> = Vec::new();
    let mut before: Vec<Option<usize>> = vec![None; pairs.len()];
    for (k, &(_, j)) in pairs.iter().enumerate() {
        let at = tails.partition_point(|&t| second(t) < j);
        if let Some(slot) = before.get_mut(k) {
            *slot = at.checked_sub(1).and_then(|p| tails.get(p).copied());
        }
        match tails.get_mut(at) {
            Some(slot) => *slot = k,
            None => tails.push(k),
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut cur = tails.last().copied();
    while let Some(k) = cur {
        if let Some(&p) = pairs.get(k) {
            out.push(p);
        }
        cur = before.get(k).copied().flatten();
    }
    out.reverse();
    out
}

/// A table of `u32` with `rows` rows of `cols` cells, read and written without indexing.
struct Grid {
    cols: usize,
    cells: Vec<u32>,
}

impl Grid {
    fn new(rows: usize, cols: usize) -> Grid {
        Grid {
            cols,
            cells: vec![0; rows * cols],
        }
    }

    fn get(&self, row: usize, col: usize) -> u32 {
        self.cells.get(row * self.cols + col).copied().unwrap_or(0)
    }

    fn set(&mut self, row: usize, col: usize, value: u32) {
        if let Some(c) = self.cells.get_mut(row * self.cols + col) {
            *c = value;
        }
    }
}

/// The longest common subsequence of two token sequences as matched pairs. Of the possible
/// answers it takes the one that skips old tokens first, so the result is always the same.
fn lcs(a: &[u32], b: &[u32]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    let mut t = Grid::new(n + 1, m + 1);
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let v = if a.get(i) == b.get(j) {
                t.get(i + 1, j + 1) + 1
            } else {
                t.get(i + 1, j).max(t.get(i, j + 1))
            };
            t.set(i, j, v);
        }
    }
    let mut out = Vec::new();
    let (mut i, mut j) = (0usize, 0usize);
    while i < n && j < m {
        if a.get(i) == b.get(j) {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t.get(i + 1, j) >= t.get(i, j + 1) {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

// ---- edits and cleaning -------------------------------------------------------------------------

/// An edit in token numbers: old tokens `a0..a1` become new tokens `b0..b1`.
#[derive(Clone, Copy, Debug)]
struct TokEdit {
    a0: usize,
    a1: usize,
    b0: usize,
    b1: usize,
}

/// The edits between the matched pairs: every stretch of tokens that no pair covers.
fn raw_edits(pairs: &[(usize, usize)], n: usize, m: usize) -> Vec<TokEdit> {
    let mut list = Vec::new();
    let (mut a, mut b) = (0usize, 0usize);
    for &(i, j) in pairs {
        if i > a || j > b {
            list.push(TokEdit {
                a0: a,
                a1: i,
                b0: b,
                b1: j,
            });
        }
        a = i + 1;
        b = j + 1;
    }
    if a < n || b < m {
        list.push(TokEdit {
            a0: a,
            a1: n,
            b0: b,
            b1: m,
        });
    }
    list
}

/// The size of an edit in characters: what it deletes and what it inserts, together.
fn size(e: &TokEdit, old: &Prefix, new: &Prefix) -> usize {
    old.chars_between(e.a0, e.a1) + new.chars_between(e.b0, e.b1)
}

/// The cleaning rules of 2.5.
fn clean(list: &mut Vec<TokEdit>, old: &Prefix, new: &Prefix, cleaning: Cleaning) {
    loop {
        let mut changed = false;
        let mut out: Vec<TokEdit> = Vec::with_capacity(list.len());
        for e in list.drain(..) {
            if let Some(last) = out.last_mut() {
                let words = old.words_between(last.a1, e.a0);
                let chars = old.chars_between(last.a1, e.a0);
                let larger = size(last, old, new).max(size(&e, old, new));
                if words == 0 || (words <= cleaning.island_words && chars <= larger) {
                    last.a1 = e.a1;
                    last.b1 = e.b1;
                    changed = true;
                    continue;
                }
            }
            out.push(e);
        }
        *list = out;
        if !changed {
            break;
        }
    }
    if let (Some(first), Some(last)) = (list.first().copied(), list.last().copied())
        && list.len() > 1
    {
        let inside: usize = list
            .iter()
            .map(|e| old.words_between(e.a0, e.a1) + new.words_between(e.b0, e.b1))
            .sum();
        let span = old.words_between(first.a0, last.a1) + new.words_between(first.b0, last.b1);
        if inside * cleaning.whole.1 > span * cleaning.whole.0 {
            *list = vec![TokEdit {
                a0: first.a0,
                a1: last.a1,
                b0: first.b0,
                b1: last.b1,
            }];
        }
    }
}
