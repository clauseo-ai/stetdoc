//! The quote policy of typed text: one pure function of the text, the paragraph it goes into and that paragraph's
//! neighbours (`docs/design/document-and-ops.md` 2.8).
//!
//! Agents type straight quotes into documents that use curly ones, and Word's own typing converts them, so a paragraph that
//! mixes the two shows in a lawyer's review. The policy follows the document: curly where the paragraph is curly, straight
//! where it is straight, and as typed where there is no evidence. It goes both ways. A straight quote right after a digit
//! stays straight (`6'` and `12"` are feet and inches) unless a quotation of that kind is open in the typed text, which it
//! then closes (`"Clause 12"` stays matched). It depends on nothing but its arguments, so the same edit gives the same
//! bytes after an `export` and a `resume`.

use crate::revisions::diff::{self, Cleanup};

/// What the surrounding text says about quotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Context {
    /// The text uses curly quotes.
    Curly,
    /// The text uses straight quotes and no curly ones.
    Straight,
    /// There is no quote to learn from.
    Unknown,
}

/// The result of following a context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Followed {
    /// The text, with its quotes changed.
    pub text: String,
    /// How many straight quotes became curly.
    pub curled: u32,
    /// How many curly quotes became straight.
    pub straightened: u32,
}

fn is_curly(c: char) -> bool {
    matches!(c, '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}')
}

fn is_straight(c: char) -> bool {
    matches!(c, '\'' | '"')
}

/// What one text says: curly if it holds a curly quote, straight if it holds only straight ones, unknown if it holds none.
fn context_of(text: &str) -> Context {
    if text.chars().any(is_curly) {
        Context::Curly
    } else if text.chars().any(is_straight) {
        Context::Straight
    } else {
        Context::Unknown
    }
}

/// The context of a paragraph: its own text decides, and when that holds no quote the nearest neighbour that holds one does.
/// `neighbours` are in order of distance, the five on each side taken alternately by the caller.
pub(crate) fn context<'a>(own: &str, neighbours: impl IntoIterator<Item = &'a str>) -> Context {
    match context_of(own) {
        Context::Unknown => neighbours
            .into_iter()
            .map(context_of)
            .find(|c| *c != Context::Unknown)
            .unwrap_or(Context::Unknown),
        known => known,
    }
}

/// Whether a quote here opens: at the start, or after a space, an opening bracket or a dash.
fn opens(previous: Option<char>) -> bool {
    match previous {
        None => true,
        Some(c) => {
            c.is_whitespace()
                || matches!(
                    c,
                    '(' | '[' | '{' | '\u{2013}' | '\u{2014}' | '-' | '/' | '\u{2018}' | '\u{201C}'
                )
        }
    }
}

/// Changes the quotes of `text` to follow `context`, as text at the start of a paragraph. With `Unknown` nothing changes.
#[cfg(test)]
pub(crate) fn follow(text: &str, context: Context) -> Followed {
    follow_after(text, context, None)
}

/// Changes the quotes of `text` to follow `context`, where `before` is the character the text goes after in its paragraph,
/// or `None` at the start of one: `'s` typed after `Supplier` is an apostrophe and not an opening quote. With `Unknown`
/// nothing changes.
pub(crate) fn follow_after(text: &str, context: Context, before: Option<char>) -> Followed {
    let mut out = String::with_capacity(text.len());
    let (mut curled, mut straightened) = (0u32, 0u32);
    // Whether a quotation is open in the text so far: a quote of the kind that follows a digit then closes it ("Clause 12"),
    // and with none open it is feet or inches. The state follows the characters written, so a text that was followed once
    // is followed again to the same text.
    let (mut open_double, mut open_single) = (false, false);
    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let previous = match i.checked_sub(1) {
            Some(p) => chars.get(p).copied(),
            None => before,
        };
        let next = chars.get(i + 1).copied();
        let after_digit = previous.is_some_and(|p| p.is_ascii_digit());
        let replacement = match (context, c) {
            (Context::Curly, '"') => match (after_digit, open_double) {
                // 12" is inches, unless a double quote is open: "Clause 12".
                (true, false) => None,
                (true, true) => Some('\u{201D}'),
                (false, _) if opens(previous) => Some('\u{201C}'),
                (false, _) => Some('\u{201D}'),
            },
            (Context::Curly, '\'') => {
                if next.is_some_and(|n| n.is_ascii_digit()) && opens(previous) {
                    // An apostrophe before digits: '90s.
                    Some('\u{2019}')
                } else if after_digit {
                    // 6' is feet, unless a single quote is open: 'Clause 12'.
                    open_single.then_some('\u{2019}')
                } else if previous.is_some_and(char::is_alphabetic) || !opens(previous) {
                    Some('\u{2019}')
                } else {
                    Some('\u{2018}')
                }
            }
            (Context::Straight, '\u{2018}' | '\u{2019}') => Some('\''),
            (Context::Straight, '\u{201C}' | '\u{201D}') => Some('"'),
            _ => None,
        };
        match replacement.unwrap_or(c) {
            '\u{201C}' => open_double = true,
            '\u{201D}' => open_double = false,
            '\u{2018}' => open_single = true,
            '\u{2019}' if !previous.is_some_and(char::is_alphabetic) => open_single = false,
            _ => {}
        }
        match replacement {
            Some(r) => {
                out.push(r);
                if context == Context::Curly {
                    curled += 1;
                } else {
                    straightened += 1;
                }
            }
            None => out.push(c),
        }
    }
    Followed {
        text: out,
        curled,
        straightened,
    }
}

/// Changes the quotes of what an edit types and of nothing it carries over (2.8, rule 1): `new` replaces `old`, and only the
/// stretches of `new` that differ from `old` follow `context`. A paragraph that mixes curly and straight quotes keeps the ones
/// the agent copied unchanged, so changing the last word of a line changes nothing else in it. A quote typed over one that is
/// there differs from it, follows the context, and so becomes what was there when the context agrees: the difference then
/// records no change (rule 4). `before` is the character `old` follows in its paragraph, or `None` at its start.
pub(crate) fn follow_edit(
    old: &str,
    new: &str,
    context: Context,
    before: Option<char>,
) -> Followed {
    if context == Context::Unknown {
        return follow_after(new, context, before);
    }
    let new_chars: Vec<char> = new.chars().collect();
    let mut out = String::with_capacity(new.len());
    let (mut curled, mut straightened) = (0u32, 0u32);
    let (mut at_old, mut at_new) = (0usize, 0usize);
    let take = |out: &mut String, from: usize, to: usize| {
        out.extend(new_chars.get(from..to).unwrap_or_default());
    };
    for edit in diff::edits(old, new, Cleanup::Minimal) {
        // What lies between two edits is the same in both texts.
        let same = edit.range.start.saturating_sub(at_old);
        take(&mut out, at_new, at_new + same);
        at_new += same;
        let typed_len = edit.with.chars().count();
        let previous = out.chars().next_back().or(before);
        let followed = follow_after(&edit.with, context, previous);
        out.push_str(&followed.text);
        curled += followed.curled;
        straightened += followed.straightened;
        at_new += typed_len;
        at_old = edit.range.end;
    }
    take(&mut out, at_new, new_chars.len());
    Followed {
        text: out,
        curled,
        straightened,
    }
}

/// The text with curly quotes folded to straight ones, for the second try of a `find` that matched nothing exactly.
pub(crate) fn fold(text: &str) -> String {
    crate::wordml::fold_quotes(text)
}

#[cfg(test)]
mod tests;
