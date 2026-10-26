//! Defined terms: where a term is defined, and where it is used.
//!
//! A definition is found two ways, both from text (design 7.3): a **definition paragraph**, which after an
//! optional label has a quoted term and then `means`, `has the meaning` and the like, and a **parenthetical
//! definition** in running text, `(the "Buyer")`. A use is the term as a whole word, case sensitive, with no
//! stemming, outside the quotes that define it. Bold-without-quotes definitions and glossary tables are not
//! detected.

use super::labels::after_label;
use crate::wordml::{BlockKey, StoryKind};

/// How a term was defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DefKind {
    /// The paragraph is the definition: `"Term" means ...`.
    Paragraph,
    /// A parenthesis in running text: `(the "Term")`.
    Parenthetical,
}

/// A place where a term is defined.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Definition {
    /// The term.
    pub term: String,
    /// The paragraph.
    pub block: BlockKey,
    /// The story it is in.
    pub story: StoryKind,
    /// How it was defined.
    pub kind: DefKind,
    /// The start of the paragraph.
    pub snippet: String,
}

/// The words that follow a quoted term in a definition paragraph.
const VERBS: [&str; 14] = [
    "shall mean",
    "shall have the meaning",
    "shall have the meanings",
    "shall include",
    "have the meaning",
    "have the meanings",
    "has the meaning",
    "has the meanings",
    "is defined",
    "are defined",
    "refers to",
    "includes",
    "means",
    "mean",
];

/// The words that may stand between the opening parenthesis and the quoted term of a parenthetical definition.
const LEAD: [&str; 18] = [
    "the",
    "a",
    "an",
    "each",
    "together",
    "collectively",
    "individually",
    "and",
    "hereinafter",
    "hereafter",
    "herein",
    "referred",
    "to",
    "as",
    "also",
    "jointly",
    "severally",
    "called",
];

/// The most characters of a term.
const LONGEST: usize = 60;

fn is_open(c: char) -> bool {
    matches!(c, '"' | '\u{201c}')
}

fn is_close(c: char) -> bool {
    matches!(c, '"' | '\u{201d}')
}

/// A quoted term at the start of `s`: the term and the rest after the closing quote.
fn quoted(s: &str) -> Option<(String, &str)> {
    let mut chars = s.char_indices();
    let (_, open) = chars.next()?;
    if !is_open(open) {
        return None;
    }
    let after = s.get(open.len_utf8()..)?;
    let end = after.find(is_close)?;
    let raw = after.get(..end)?;
    let term = raw.trim().trim_end_matches([',', '.', ';']).trim();
    if term.is_empty() || term.chars().count() > LONGEST || term.split_whitespace().count() > 8 {
        return None;
    }
    let rest = after.get(end + after[end..].chars().next()?.len_utf8()..)?;
    Some((term.to_string(), rest))
}

/// A list of quoted terms joined by commas, `and` or `or`, and what follows it.
fn quoted_list(mut s: &str) -> Option<(Vec<String>, &str)> {
    let mut terms = Vec::new();
    loop {
        let (term, rest) = quoted(s.trim_start())?;
        terms.push(term);
        let rest = rest.trim_start();
        let next = rest
            .strip_prefix(',')
            .or_else(|| rest.strip_prefix("and "))
            .or_else(|| rest.strip_prefix("or "))
            .or_else(|| rest.strip_prefix('&'))
            .map(str::trim_start);
        match next {
            Some(n) if n.starts_with(is_open) => s = n,
            Some(n) if n.starts_with("and ") || n.starts_with("or ") => s = n,
            _ => return Some((terms, rest)),
        }
        if terms.len() >= 12 {
            return Some((terms, rest));
        }
    }
}

/// True if a definition verb comes within five words of the start of `rest`, with no end of a sentence
/// before it.
fn verb_follows(rest: &str) -> bool {
    let lower = rest.to_lowercase();
    let mut cur = lower.trim_start();
    for _ in 0..=5 {
        let verb = VERBS.iter().any(|v| {
            cur.starts_with(v)
                && cur
                    .get(v.len()..)
                    .is_some_and(|r| r.is_empty() || r.starts_with(char::is_whitespace))
        });
        if verb {
            return true;
        }
        let (word, tail) = cur.split_once(char::is_whitespace).unwrap_or((cur, ""));
        if word.is_empty() || word.ends_with(['.', ';', ':']) {
            return false;
        }
        cur = tail.trim_start();
    }
    false
}

/// The terms a definition paragraph defines: after an optional label, quoted terms, a few words, and a verb.
fn paragraph_terms(text: &str) -> Vec<String> {
    match quoted_list(after_label(text)) {
        Some((terms, rest)) if verb_follows(rest) => terms,
        _ => Vec::new(),
    }
}

/// The terms that parentheses in running text define: `(the "Buyer")`, `(each, a "Party")`.
fn parenthetical_terms(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let after = rest.get(open + 1..).unwrap_or("");
        let Some(close) = after.find(')') else {
            break;
        };
        let inside = after.get(..close).unwrap_or("");
        rest = after.get(close + 1..).unwrap_or("");
        if inside.chars().count() > 160 {
            continue;
        }
        let Some(first_quote) = inside.find(is_open) else {
            continue;
        };
        let lead = inside.get(..first_quote).unwrap_or("");
        let lead_ok = lead
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|w| !w.is_empty())
            .all(|w| LEAD.contains(&w.to_lowercase().as_str()));
        if !lead_ok {
            continue;
        }
        let quoted_part = inside.get(first_quote..).unwrap_or("");
        if let Some((terms, tail)) = quoted_list(quoted_part)
            && tail.trim().trim_matches(',').trim().is_empty()
        {
            out.extend(terms);
        }
    }
    out
}

/// Every term the text defines, with how.
pub(crate) fn definitions(text: &str) -> Vec<(String, DefKind)> {
    // Both kinds of definition have a quoted term, so a paragraph with no quote defines nothing.
    if !text.contains('"') && !text.contains('\u{201c}') {
        return Vec::new();
    }
    let mut out: Vec<(String, DefKind)> = paragraph_terms(text)
        .into_iter()
        .map(|t| (t, DefKind::Paragraph))
        .collect();
    for term in parenthetical_terms(text) {
        if !out.iter().any(|(t, _)| *t == term) {
            out.push((term, DefKind::Parenthetical));
        }
    }
    out
}

/// The character offsets where `term` is used in `text`: as a whole word, case sensitive, and not directly
/// after an opening quote, which is where a term is defined or quoted. A term that starts in lower case
/// must have four characters or more.
pub(crate) fn uses(text: &str, term: &str) -> Vec<usize> {
    let length = term.chars().count();
    let lowercase = term.chars().next().is_some_and(char::is_lowercase);
    if term.is_empty() || (lowercase && length < 4) || !text.contains(term) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (byte, _) in text.match_indices(term) {
        let before = text.get(..byte).and_then(|t| t.chars().next_back());
        let after = text.get(byte + term.len()..).and_then(|t| t.chars().next());
        let word_before = before.is_some_and(char::is_alphanumeric);
        let word_after = after.is_some_and(char::is_alphanumeric);
        let quoted = before.is_some_and(is_open);
        if !word_before && !word_after && !quoted {
            out.push(text.get(..byte).map_or(0, |t| t.chars().count()));
        }
    }
    out
}

/// About 80 characters of `text` around a place, whitespace collapsed.
pub(crate) fn snippet(text: &str, at: usize, length: usize) -> String {
    let start = at.saturating_sub(30);
    let window: String = text.chars().skip(start).take(length + 60).collect();
    let collapsed = window.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(80).collect();
    if start > 0 {
        out.insert(0, '…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terms(text: &str) -> Vec<(String, DefKind)> {
        definitions(text)
    }

    #[test]
    fn a_definition_paragraph_has_a_quoted_term_and_a_verb() {
        let d = |t: &str| terms(t).into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        assert_eq!(
            d("\"Affiliate\" means any person under common control."),
            vec!["Affiliate"]
        );
        assert_eq!(
            d("1.01 \u{201c}Losses\u{201d} shall mean all damages."),
            vec!["Losses"]
        );
        assert_eq!(
            d("\"Buyer\" and \"Seller\" have the meanings given above."),
            vec!["Buyer", "Seller"]
        );
        assert_eq!(
            d("\"Affiliate\" of any Person means that Person's parent."),
            vec!["Affiliate"]
        );
        assert_eq!(d("\"Code\" is defined in Section 2."), vec!["Code"]);
        assert!(d("The \"Buyer\" pays on time.").is_empty());
        assert!(d("\"Buyer\" pays the price.").is_empty());
        assert!(
            d("Pay it. \"Buyer\" means someone.").is_empty(),
            "the definition starts the paragraph"
        );
    }

    #[test]
    fn a_parenthetical_definition_is_a_quoted_term_in_parentheses() {
        let d = |t: &str| terms(t);
        let kinds = |t: &str| d(t);
        assert_eq!(
            kinds(
                "This agreement is between Acme Inc. (the \"Seller\") and Beta LLC (the \u{201c}Buyer\u{201d})."
            ),
            vec![
                ("Seller".to_string(), DefKind::Parenthetical),
                ("Buyer".to_string(), DefKind::Parenthetical)
            ]
        );
        assert_eq!(kinds("(each, a \"Party\")")[0].0, "Party");
        assert_eq!(kinds("(together, the \"Parties\")")[0].0, "Parties");
        assert_eq!(kinds("(hereinafter \"Company\")")[0].0, "Company");
        assert_eq!(kinds("(\"Company,\")")[0].0, "Company");
        assert!(d("pay (as set out in the \"Schedule\" below) soon").is_empty());
        assert!(d("(see Section 4)").is_empty());
    }

    #[test]
    fn a_use_is_the_whole_word_case_sensitive_and_not_the_definition_itself() {
        assert_eq!(
            uses(
                "The Seller pays. Seller's duty. Sellers vary. \"Seller\" is quoted.",
                "Seller"
            ),
            vec![4, 17]
        );
        assert!(
            uses("losses happen", "loss").is_empty(),
            "no stemming and a lowercase term needs four letters"
        );
        assert_eq!(uses("personal data is data", "personal data"), vec![0]);
        assert!(
            uses("the law", "law").is_empty(),
            "a short lowercase term is not matched"
        );
        assert_eq!(uses("a Loss and Losses", "Loss"), vec![2]);
    }

    #[test]
    fn a_snippet_is_a_window_of_the_text() {
        assert_eq!(snippet("short text", 0, 5), "short text");
        let long = "word ".repeat(40);
        let s = snippet(&long, 100, 4);
        assert!(s.starts_with('…') && s.chars().count() <= 81, "{s}");
    }
}
