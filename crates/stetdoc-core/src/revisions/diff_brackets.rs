//! The bracket rule of the cleaning (design `revisions.md`, 2.5 rule 5).
//!
//! An edit that cuts a bracket pair in half reads as a sloppy markup: `thirty (30)` to `forty-five (45)` matches
//! the closing `)` as unchanged, so the redline strikes `thirty (30` and leaves the `)` standing after the
//! insertion. A reviewer strikes the whole `thirty (30)` and inserts the whole `forty-five (45)`. So an edit that
//! leaves an opener open takes the unchanged closer right after it, and an edit that leaves a closer unopened takes
//! the unchanged opener right before it, one token at a time. The pairs are `()`, `[]`, `{}` and the curly double
//! quotes. A straight quote has no direction, and a curly single quote is also an apostrophe, so neither is a pair.
//!
//! The token beside an edit is in an unchanged island or in the common prefix or suffix, so it is matched, the same
//! token on both sides, and the edit grows by one token on both. The rule never takes a token that another edit
//! holds, and never one that the caller says must stay out (another author's pending change, or text that an edit
//! would be refused on). Glued punctuation that is not a bracket (`2%`, `Seller's`) stays outside the edit, as in
//! Word's own replacements.

use super::diff::{Kind, TokEdit};
use std::ops::Range;

/// The bracket pairs: opener, closer.
const PAIRS: [(char, char); 4] = [('(', ')'), ('[', ']'), ('{', '}'), ('\u{201C}', '\u{201D}')];

/// One text, as the rule reads it: its characters and its tokens.
pub(super) struct Side<'a> {
    pub(super) chars: &'a [char],
    pub(super) toks: &'a [(Range<usize>, Kind)],
}

impl Side<'_> {
    /// The character of token `t` if it is one character of punctuation.
    fn bracket(&self, t: usize) -> Option<char> {
        let (r, k) = self.toks.get(t)?;
        if *k != Kind::Other || r.len() != 1 {
            return None;
        }
        self.chars.get(r.start).copied()
    }

    /// What tokens `from..to` leave unpaired: the closer of the innermost opener they leave open, and the opener of
    /// the first closer they leave unopened.
    fn unpaired(&self, from: usize, to: usize) -> (Option<char>, Option<char>) {
        let mut open: Vec<char> = Vec::new();
        let mut unopened = None;
        for t in from..to {
            let Some(c) = self.bracket(t) else {
                continue;
            };
            if let Some(&(_, close)) = PAIRS.iter().find(|p| p.0 == c) {
                open.push(close);
            } else if let Some(&(opener, _)) = PAIRS.iter().find(|p| p.1 == c) {
                if open.last() == Some(&c) {
                    open.pop();
                } else if open.is_empty() && unopened.is_none() {
                    unopened = Some(opener);
                }
            }
        }
        (open.last().copied(), unopened)
    }
}

/// Widens each edit of `list` over the unchanged bracket beside it that pairs with one the edit leaves unpaired, on
/// either side. `keep` is true for a range of old characters that must stay out of an edit. Returns true if an edit
/// grew. Two edits can touch afterwards, and rule 3 makes them one.
pub(super) fn balance(
    list: &mut [TokEdit],
    old: &Side<'_>,
    new: &Side<'_>,
    keep: &dyn Fn(Range<usize>) -> bool,
) -> bool {
    let takes = |t: usize| old.toks.get(t).is_some_and(|(r, _)| !keep(r.clone()));
    let mut changed = false;
    for i in 0..list.len() {
        // What the edits beside this one hold is out of reach: the one before as it now is, the one after as it was.
        let floor = i
            .checked_sub(1)
            .and_then(|p| list.get(p))
            .map_or(0, |e| e.a1);
        let ceil = list.get(i + 1).map_or(old.toks.len(), |e| e.a0);
        let Some(e) = list.get_mut(i) else {
            continue;
        };
        loop {
            let (old_close, old_open) = old.unpaired(e.a0, e.a1);
            let (new_close, new_open) = new.unpaired(e.b0, e.b1);
            let after = if e.a1 < ceil { old.bracket(e.a1) } else { None };
            if after.is_some() && (after == old_close || after == new_close) && takes(e.a1) {
                e.a1 += 1;
                e.b1 += 1;
                changed = true;
                continue;
            }
            let at = e.a0.checked_sub(1).filter(|&t| t >= floor);
            let before = at.and_then(|t| old.bracket(t));
            if let (Some(t), Some(b0)) = (at, e.b0.checked_sub(1))
                && before.is_some()
                && (before == old_open || before == new_open)
                && takes(t)
            {
                e.a0 = t;
                e.b0 = b0;
                changed = true;
                continue;
            }
            break;
        }
    }
    changed
}
