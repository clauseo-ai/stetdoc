//! Prose mentions of clauses: `clause 4.2(a)`, `Sections 3.1 and 3.4`, `Article III`, `paragraph (b) above`.
//!
//! A mention is a noun followed by one or more references. This file finds them in a paragraph's text and
//! says what each is, and `edges.rs` resolves them against the clause table. The detection is a heuristic and
//! is described as one: 8% of the LAB paragraphs have a mention, in 43 of 45 documents (design 7.4).

use super::labels::Noun;

/// A mention as found in the text, before it is resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Raw {
    /// The noun as written (`Sections`).
    pub word: String,
    /// What the noun is.
    pub noun: Noun,
    /// The reference as written (`4.2(a)`).
    pub reference: String,
    /// The text of the mention: the noun and the reference.
    pub text: String,
    /// A word around it says it cites a law and not this document: `of the Code`, `45 CFR §`, `Article 6 GDPR`.
    pub outside: bool,
    /// It looks like a section of a statute, with a first number of 100 or more (`§ 547(b)`, `Section 704(c)`),
    /// which is `outside` when the document has no clause of that number and unresolved otherwise.
    pub statutory: bool,
    /// It is followed by `of` and a noun, `paragraph 2 of Article 9`: it is a part of something named, and the
    /// place of the citing paragraph says nothing of where it is.
    pub qualified: bool,
    /// The number of the word the noun is, from 0, so that a caller can tell a paragraph's own typed label
    /// (`Schedule 1. Fees`) from a citation.
    pub at: usize,
}

/// The names after `of` that are this document and not another: `Section 4 of the Agreement`, `Section 9.2 of
/// This Agreement`, `PARAGRAPH (a) OF THIS SECTION`. Compared in lower case.
const SELF: [&str; 15] = [
    "this",
    "these",
    "hereof",
    "agreement",
    "contract",
    "lease",
    "deed",
    "plan",
    "policy",
    "note",
    "schedule",
    "exhibit",
    "annex",
    "appendix",
    "article",
];

/// The words in front of a reference that say it is a citation of a law: `11 U.S.C. § 547`, `45 CFR § 164`,
/// `Sherman Act § 1`, `Treasury Regulations Section 1.704-1`. Compared without dots and in lower case.
const LAW_BEFORE: [&str; 20] = [
    "usc",
    "cfr",
    "irc",
    "ucc",
    "erisa",
    "gdpr",
    "code",
    "act",
    "regulation",
    "regulations",
    "rule",
    "rules",
    "directive",
    "treaty",
    "statute",
    "law",
    "tfeu",
    "teu",
    "ordinance",
    "frcp",
];

/// Short capital words that follow a reference in a heading and are not the name of a law.
const NOT_A_LAW: [&str; 10] = [
    "AND", "THE", "FOR", "ALL", "NOT", "ANY", "OF", "TO", "OR", "SEE",
];

/// A word is an acronym that names a law: `CFR`, `GDPR`, `EUMR`, `U.S.C.`.
fn is_law_name(word: &str) -> bool {
    let w = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
    let bare: String = w.chars().filter(|c| *c != '.').collect();
    if bare.eq_ignore_ascii_case("usc") && w.contains('.') || bare == "CFR" {
        return true;
    }
    (3..=5).contains(&bare.chars().count())
        && bare.chars().all(|c| c.is_ascii_uppercase())
        && !bare.chars().all(|c| "IVXLCDM".contains(c))
        && !NOT_A_LAW.contains(&bare.as_str())
}

/// The words that join references in a list or a range.
fn joins(word: &str) -> bool {
    matches!(
        word.to_lowercase().as_str(),
        "and" | "or" | "&" | "to" | "through" | "-" | "\u{2013}" | "and/or"
    )
}

/// `4.2`, `4.2(a)(ii)`, `3(b)`: digits in dotted groups, then parenthesised parts.
fn is_path(token: &str) -> bool {
    let (head, tail) = token.split_at(token.find('(').unwrap_or(token.len()));
    let head_ok = !head.is_empty()
        && head
            .split('.')
            .all(|g| !g.is_empty() && g.len() <= 4 && g.chars().all(|c| c.is_ascii_digit()));
    if !head_ok {
        return false;
    }
    parens_ok(tail)
}

/// Zero or more `(x)` groups with nothing else.
fn parens_ok(mut s: &str) -> bool {
    while !s.is_empty() {
        let Some(rest) = s.strip_prefix('(') else {
            return false;
        };
        let Some(close) = rest.find(')') else {
            return false;
        };
        let inner = rest.get(..close).unwrap_or("");
        if inner.is_empty() || inner.len() > 4 || !inner.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return false;
        }
        s = rest.get(close + 1..).unwrap_or("");
    }
    true
}

/// A reference token after a noun: a path, a `(b)`, a Roman numeral, or a capital letter for a schedule.
fn is_reference(token: &str, noun: Noun) -> bool {
    if is_path(token) {
        return true;
    }
    if token.starts_with('(') && parens_ok(token) {
        return true;
    }
    let roman = token.len() >= 2 && token.chars().all(|c| "IVXLCDM".contains(c));
    if roman {
        return true;
    }
    let letter = token.chars().count() == 1 && token.chars().all(|c| c.is_ascii_uppercase());
    letter
        && matches!(
            noun,
            Noun::Schedule | Noun::Exhibit | Noun::Annex | Noun::Appendix | Noun::Part
        )
        || token == "I" && matches!(noun, Noun::Article | Noun::Part)
}

/// A word without the punctuation that closes a phrase, and whether a comma closed it.
fn clean(word: &str) -> (&str, bool) {
    let comma = word.ends_with(',');
    let trimmed = word.trim_end_matches([',', ';', ':', '.']);
    // An unbalanced closing parenthesis belongs to the sentence: `(see clause 4)`.
    let opens = trimmed.matches('(').count();
    let closes = trimmed.matches(')').count();
    let trimmed = if closes > opens {
        trimmed.trim_end_matches(')')
    } else {
        trimmed
    };
    (trimmed, comma)
}

/// Whether a word may be a noun, as the first test of the search: the word without what opens and closes a
/// phrase. It is true of every word that `clean` and `Noun::of` call a noun, and of a few more.
fn may_be_noun(word: &str) -> bool {
    let word = word.trim_start_matches(['(', '"', '\u{201c}']);
    // Every noun starts with one of these letters, or is a section sign (`0xC2 0xA7`): most words are
    // turned away by their first byte.
    let first = word.as_bytes().first().map(u8::to_ascii_lowercase);
    if !matches!(first, Some(b'a' | b'c' | b'e' | b'p' | b'r' | b's' | 0xC2)) {
        return false;
    }
    Noun::of(word.trim_end_matches([',', ';', ':', '.', ')'])).is_some()
}

/// The words of a text, split at white space as `str::split_whitespace` splits it. Text that is ASCII and has
/// no vertical tab, which almost all text is, is split by its bytes, where `u8::is_ascii_whitespace` is the
/// same test as `char::is_whitespace`.
enum Words<'a> {
    Ascii(std::str::SplitAsciiWhitespace<'a>),
    Any(std::str::SplitWhitespace<'a>),
}

impl<'a> Words<'a> {
    fn of(text: &'a str) -> Words<'a> {
        if text.is_ascii() && !text.as_bytes().contains(&0x0B) {
            Words::Ascii(text.split_ascii_whitespace())
        } else {
            Words::Any(text.split_whitespace())
        }
    }
}

impl<'a> Iterator for Words<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        match self {
            Words::Ascii(it) => it.next(),
            Words::Any(it) => it.next(),
        }
    }
}

/// Every mention in a text, in order.
pub(crate) fn find(text: &str) -> Vec<Raw> {
    // Most paragraphs have no noun at all, and they are not worth a vector of their words.
    let Some(first) = Words::of(text).position(may_be_noun) else {
        return Vec::new();
    };
    // A word and the space after it are rarely fewer than four bytes, so the vector is not grown.
    let mut words: Vec<&str> = Vec::with_capacity(text.len() / 4 + 1);
    words.extend(Words::of(text));
    let mut out = Vec::new();
    let mut i = first;
    while let Some(word) = words.get(i) {
        if !may_be_noun(word) {
            i += 1;
            continue;
        }
        let (bare, _) = clean(word);
        let bare = bare.trim_start_matches(['(', '"', '\u{201c}']);
        let Some(noun) = Noun::of(bare).filter(|_| {
            bare.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '§')
        }) else {
            i += 1;
            continue;
        };
        // The references after the noun: a list joined by commas, `and`, `or` and `to`.
        let mut refs: Vec<String> = Vec::new();
        let mut j = i + 1;
        let mut expect = true;
        while let Some(next) = words.get(j) {
            if expect {
                let (token, comma) = clean(next);
                if is_reference(token, noun) {
                    refs.push(token.to_string());
                    j += 1;
                    expect = comma || next.ends_with(',');
                    if !expect && words.get(j).is_some_and(|w| joins(clean(w).0)) {
                        j += 1;
                        expect = true;
                    }
                    continue;
                }
                break;
            }
            break;
        }
        if refs.is_empty() {
            i += 1;
            continue;
        }
        let before_law = i
            .checked_sub(1)
            .and_then(|b| words.get(b))
            .is_some_and(|w| {
                let w: String = clean(w)
                    .0
                    .chars()
                    .filter(|c| *c != '.')
                    .collect::<String>()
                    .to_lowercase();
                LAW_BEFORE.contains(&w.as_str())
            });
        let after_law = words.get(j).is_some_and(|w| is_law_name(w));
        let of_other = words.get(j).is_some_and(|w| w.eq_ignore_ascii_case("of"))
            && words.get(j + 1..).is_some_and(|rest| {
                let mut it = rest.iter();
                let mut name = it.next().copied();
                if name.is_some_and(|w| w.eq_ignore_ascii_case("the")) {
                    name = it.next().copied();
                }
                let Some(w) = name else {
                    return false;
                };
                let w = clean(w).0;
                if w.chars().all(|c| c.is_ascii_digit()) && !w.is_empty() {
                    // `of 45 CFR`, `of 11 U.S.C.`
                    return it.next().is_some_and(|n| is_law_name(n));
                }
                w.chars().next().is_some_and(char::is_uppercase)
                    && !SELF.contains(&w.to_lowercase().as_str())
            });
        let outside = before_law || after_law || of_other;
        let qualified = words.get(j).is_some_and(|w| w.eq_ignore_ascii_case("of"))
            && words
                .get(j + 1)
                .is_some_and(|w| Noun::of(w).is_some_and(|n| n != Noun::Paragraph));
        let section_like = matches!(noun, Noun::Section)
            && (bare == "§"
                || bare == "§§"
                || bare.starts_with("Section")
                || bare.starts_with("section"));
        let first_group = |r: &str| -> u32 {
            r.split(['.', '('])
                .next()
                .and_then(|g| g.parse().ok())
                .unwrap_or(0)
        };
        for reference in refs {
            out.push(Raw {
                word: bare.to_string(),
                noun,
                text: format!("{bare} {reference}"),
                statutory: section_like && first_group(&reference) >= 100,
                reference,
                outside,
                qualified,
                at: i,
            });
        }
        i = j.max(i + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(String, bool)> {
        find(text)
            .into_iter()
            .map(|r| (r.text, r.outside))
            .collect()
    }

    #[test]
    fn a_noun_and_a_reference_is_a_mention() {
        assert_eq!(
            found("as set out in clause 4.2(a)(ii) above"),
            vec![("clause 4.2(a)(ii)".to_string(), false)]
        );
        assert_eq!(
            found("See Article III."),
            vec![("Article III".to_string(), false)]
        );
        assert_eq!(
            found("under Schedule A and Exhibit B"),
            vec![
                ("Schedule A".to_string(), false),
                ("Exhibit B".to_string(), false)
            ]
        );
        assert_eq!(
            found("paragraph (b) above"),
            vec![("paragraph (b)".to_string(), false)]
        );
        assert_eq!(
            found("subject to Section 9."),
            vec![("Section 9".to_string(), false)]
        );
    }

    #[test]
    fn lists_and_ranges_give_each_reference() {
        assert_eq!(
            found("clauses 4.2 and 4.3"),
            vec![
                ("clauses 4.2".to_string(), false),
                ("clauses 4.3".to_string(), false)
            ]
        );
        assert_eq!(
            found("Sections 3.1 to 3.4 apply"),
            vec![
                ("Sections 3.1".to_string(), false),
                ("Sections 3.4".to_string(), false)
            ]
        );
        assert_eq!(
            found("Sections 2.1, 2.2 and 2.5"),
            vec![
                ("Sections 2.1".to_string(), false),
                ("Sections 2.2".to_string(), false),
                ("Sections 2.5".to_string(), false)
            ]
        );
    }

    #[test]
    fn a_citation_of_a_law_is_outside_and_a_citation_of_this_agreement_is_not() {
        assert_eq!(
            found("Section 501(c)(6) of the Code"),
            vec![("Section 501(c)(6)".to_string(), true)]
        );
        assert_eq!(
            found("Section 4 of the Securities Act"),
            vec![("Section 4".to_string(), true)]
        );
        assert_eq!(
            found("Section 4 of this Agreement"),
            vec![("Section 4".to_string(), false)]
        );
        assert_eq!(
            found("Section 4 of the Agreement"),
            vec![("Section 4".to_string(), false)]
        );
        assert_eq!(
            found("Section 9.2 of This Agreement"),
            vec![("Section 9.2".to_string(), false)]
        );
        assert_eq!(
            found("PARAGRAPH (a) OF THIS SECTION"),
            vec![("PARAGRAPH (a)".to_string(), false)]
        );
        assert_eq!(
            found("Sections 3 and 4 of These Terms"),
            vec![
                ("Sections 3".to_string(), false),
                ("Sections 4".to_string(), false)
            ]
        );
    }

    #[test]
    fn a_noun_with_no_reference_after_it_is_not_a_mention() {
        for text in [
            "this section applies",
            "the part that remains",
            "Part of the price",
            "an article of faith",
            "I agree",
        ] {
            assert!(found(text).is_empty(), "{text}");
        }
        assert_eq!(
            found("(see clause 4)"),
            vec![("clause 4".to_string(), false)]
        );
    }
}
