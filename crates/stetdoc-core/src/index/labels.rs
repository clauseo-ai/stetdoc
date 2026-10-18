//! Typed labels: what a paragraph's text starts with when its author typed the number.
//!
//! In the LAB documents 17% of the paragraphs start with a number the author typed, and 137 use list
//! numbering. So "clause 4.2" would not resolve in those documents unless a label that was typed counts
//! (design 7.2). The forms are the ones real contracts use: `1.`, `1.01`, `2.3.1`, `(a)`, `(ii)`, `A.`, `IV.`,
//! and after a noun, `Article III`, `Section 1.01`, `Schedule A`. Nothing here decides that a paragraph is a
//! heading: a typed label only says what the paragraph is called.

/// A word that names a part of a document: the nouns a label starts with, and the nouns a reference has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum Noun {
    Article,
    Section,
    Clause,
    Paragraph,
    Schedule,
    Exhibit,
    Part,
    Annex,
    Appendix,
    Recital,
}

impl Noun {
    /// The noun a word is, in either case, singular or plural. It allocates nothing and reads each byte once,
    /// because it is asked of every word of every paragraph of a document.
    pub(crate) fn of(word: &str) -> Option<Noun> {
        let mut bytes = word.as_bytes();
        while let Some((last, rest)) = bytes.split_last() {
            if !matches!(last, b'.' | b',' | b':' | b';') {
                break;
            }
            bytes = rest;
        }
        // A section sign is two bytes (U+00A7), alone or doubled.
        if bytes == "\u{a7}".as_bytes() || bytes == "\u{a7}\u{a7}".as_bytes() {
            return Some(Noun::Section);
        }
        let mut lower = [0u8; 13];
        let slot = lower.get_mut(..bytes.len())?;
        if bytes.len() < 2 {
            return None;
        }
        for (to, from) in slot.iter_mut().zip(bytes) {
            *to = from.to_ascii_lowercase();
        }
        Some(match &*slot {
            b"article" | b"articles" => Noun::Article,
            b"section" | b"sections" | b"sec" => Noun::Section,
            b"clause" | b"clauses" | b"subclause" | b"subclauses" | b"cl" => Noun::Clause,
            b"paragraph" | b"paragraphs" | b"subparagraph" | b"subparagraphs" => Noun::Paragraph,
            b"schedule" | b"schedules" => Noun::Schedule,
            b"exhibit" | b"exhibits" => Noun::Exhibit,
            b"part" | b"parts" => Noun::Part,
            b"annex" | b"annexes" | b"annexure" => Noun::Annex,
            b"appendix" | b"appendices" => Noun::Appendix,
            b"recital" | b"recitals" => Noun::Recital,
            _ => return None,
        })
    }

    /// The nouns that a label can start with: a paragraph or a recital is cited, and not numbered by its noun.
    pub(crate) fn starts_labels(self) -> bool {
        !matches!(self, Noun::Paragraph | Noun::Recital)
    }

    /// Nouns that name the same thing in a contract: `clause 4.2`, `Section 4.2` and `paragraph 4.2` are one
    /// place. The others are each their own: `Schedule 1` is not `clause 1`.
    pub(crate) fn same_family(self, other: Noun) -> bool {
        let generic = |n: Noun| matches!(n, Noun::Section | Noun::Clause | Noun::Paragraph);
        self == other || (generic(self) && generic(other))
    }

    /// The noun as a heading has it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Noun::Article => "Article",
            Noun::Section => "Section",
            Noun::Clause => "Clause",
            Noun::Paragraph => "Paragraph",
            Noun::Schedule => "Schedule",
            Noun::Exhibit => "Exhibit",
            Noun::Part => "Part",
            Noun::Annex => "Annex",
            Noun::Appendix => "Appendix",
            Noun::Recital => "Recital",
        }
    }
}

/// The kind of numbering a typed label is in, so that two labels of one kind are items of one list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Class {
    /// `1.`, `1.01`, `2.3.1`.
    Dotted,
    /// `(a)`.
    ParenLetter,
    /// `(1)`.
    ParenNumeral,
    /// `(ii)`.
    ParenRoman,
    /// `A.`.
    LetterDot,
    /// `IV.`.
    RomanDot,
    /// `Article III`, `Section 1.01`: a number after a noun.
    Named(Noun),
}

/// A label that a paragraph's text starts with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Typed {
    /// The noun in front of it, if there is one.
    pub noun: Option<Noun>,
    /// The label without the noun and without a closing period or colon: `4.2`, `(a)`, `III`.
    pub label: String,
    /// What kind of numbering it is.
    pub class: Class,
}

fn is_roman(s: &str) -> bool {
    !s.is_empty()
        && (s.chars().all(|c| "IVXLCDM".contains(c)) || s.chars().all(|c| "ivxlcdm".contains(c)))
}

/// `1`, `1.01`, `2.3.1`: digits in dotted groups.
fn is_dotted(s: &str) -> bool {
    !s.is_empty()
        && s.split('.')
            .all(|g| !g.is_empty() && g.len() <= 4 && g.chars().all(|c| c.is_ascii_digit()))
}

/// What a token is as a label, after a noun or not. `previous` is the label of the item before it, which
/// tells `(i)` the letter from `(i)` the numeral.
fn classify(token: &str, noun: Option<Noun>, previous: Option<&str>) -> Option<(String, Class)> {
    let named = |n: Noun| Class::Named(n);
    if let Some(inner) = token.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        let class =
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) && inner.len() <= 3 {
                Class::ParenNumeral
            } else if inner.chars().count() == 1 && inner.chars().all(|c| c.is_ascii_alphabetic()) {
                if inner.eq_ignore_ascii_case("i") && previous != Some("(h)") {
                    Class::ParenRoman
                } else {
                    Class::ParenLetter
                }
            } else if inner.chars().count() >= 2 && is_roman(inner) {
                Class::ParenRoman
            } else {
                return None;
            };
        return Some((token.to_string(), noun.map_or(class, named)));
    }
    let (core, dotted_end) = match token.strip_suffix('.') {
        Some(core) => (core, true),
        None => (token, false),
    };
    if is_dotted(core) {
        let inner_dot = core.contains('.');
        return (dotted_end || inner_dot || noun.is_some())
            .then(|| (core.to_string(), noun.map_or(Class::Dotted, named)));
    }
    let letter = core.chars().count() == 1 && core.chars().all(|c| c.is_ascii_uppercase());
    if let Some(n) = noun {
        if letter || is_roman(core) && core.chars().all(|c| c.is_ascii_uppercase()) {
            return Some((core.to_string(), named(n)));
        }
        return None;
    }
    if dotted_end && letter {
        return Some((core.to_string(), Class::LetterDot));
    }
    if dotted_end
        && core.chars().count() >= 2
        && is_roman(core)
        && core.chars().all(|c| c.is_ascii_uppercase())
    {
        return Some((core.to_string(), Class::RomanDot));
    }
    None
}

/// The typed label that `text` starts with, if it starts with one. The label must be followed by a space, a
/// tab or the end of the text, so `2020 was` and `A.B. Co` are not labels.
pub(crate) fn typed_label(text: &str, previous: Option<&str>) -> Option<Typed> {
    let text = text.trim_start();
    let mut words = text.split([' ', '\t']).filter(|w| !w.is_empty());
    let first = words.next()?;
    let noun = Noun::of(first)
        .filter(|n| n.starts_labels())
        .filter(|_| first != "§");
    if let Some(noun) = noun {
        let token = words
            .next()?
            .trim_end_matches([':', ';', '\u{2014}', '\u{2013}', '-']);
        // A heading goes on with its title, which starts with a capital: `Section 4 of the Code applies.`
        // is a sentence that cites a law, and it must not be a clause that takes the number 4.
        if words
            .next()
            .is_some_and(|w| w.chars().next().is_some_and(char::is_lowercase))
        {
            return None;
        }
        let (label, class) = classify(token, Some(noun), previous)?;
        return Some(Typed {
            noun: Some(noun),
            label,
            class,
        });
    }
    let token = first.trim_end_matches([':', ';']);
    let (label, class) = classify(token, None, previous)?;
    Some(Typed {
        noun: None,
        label,
        class,
    })
}

/// The text after the typed label that a paragraph starts with, or all of it when it has none.
pub(crate) fn after_label(text: &str) -> &str {
    let body = text.trim_start();
    let Some(label) = typed_label(body, None) else {
        return body;
    };
    let words = usize::from(label.noun.is_some()) + 1;
    let mut rest = body;
    for _ in 0..words {
        rest = rest.trim_start();
        rest = rest.split_once(char::is_whitespace).map_or("", |(_, r)| r);
    }
    rest.trim_start()
}

/// `61`, `IV`: a number or a Roman numeral with nothing around it.
pub(crate) fn is_bare_numeral(s: &str) -> bool {
    !s.is_empty()
        && (s.chars().all(|c| c.is_ascii_digit()) || (s.chars().count() >= 2 && is_roman(s)))
}

/// A label without spaces and without a closing period, for comparing a typed or a composed label with what a
/// reference says. Case is ignored outside parentheses (`ARTICLE III` is `Article III`) and kept inside them,
/// because `(A)` and `(a)` are the two levels of one outline, and a citation of `(A)` is not one of `(a)`.
pub(crate) fn normalize(label: &str) -> String {
    let mut depth = 0u32;
    let mut out = String::with_capacity(label.len());
    for c in label.chars().filter(|c| !c.is_whitespace()) {
        match c {
            '(' => {
                depth += 1;
                out.push(c);
            }
            ')' => {
                depth = depth.saturating_sub(1);
                out.push(c);
            }
            c if depth > 0 => out.push(c),
            c => out.extend(c.to_lowercase()),
        }
    }
    out.trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> Option<(Option<Noun>, String, Class)> {
        typed_label(text, None).map(|t| (t.noun, t.label, t.class))
    }

    #[test]
    fn the_forms_real_contracts_use_are_labels() {
        assert_eq!(typed("1. PURPOSE"), Some((None, "1".into(), Class::Dotted)));
        assert_eq!(
            typed("1.01\tDefined Terms"),
            Some((None, "1.01".into(), Class::Dotted))
        );
        assert_eq!(
            typed("2.3.1 Detail"),
            Some((None, "2.3.1".into(), Class::Dotted))
        );
        assert_eq!(
            typed("(a) the Buyer"),
            Some((None, "(a)".into(), Class::ParenLetter))
        );
        assert_eq!(
            typed("(ii) second"),
            Some((None, "(ii)".into(), Class::ParenRoman))
        );
        assert_eq!(
            typed("(3) third"),
            Some((None, "(3)".into(), Class::ParenNumeral))
        );
        assert_eq!(
            typed("A. First"),
            Some((None, "A".into(), Class::LetterDot))
        );
        assert_eq!(
            typed("IV. Fourth"),
            Some((None, "IV".into(), Class::RomanDot))
        );
        assert_eq!(
            typed("Article III: Payment"),
            Some((
                Some(Noun::Article),
                "III".into(),
                Class::Named(Noun::Article)
            ))
        );
        assert_eq!(
            typed("SECTION 1.01 Terms"),
            Some((
                Some(Noun::Section),
                "1.01".into(),
                Class::Named(Noun::Section)
            ))
        );
        assert_eq!(
            typed("Schedule A"),
            Some((
                Some(Noun::Schedule),
                "A".into(),
                Class::Named(Noun::Schedule)
            ))
        );
        assert_eq!(
            typed("Section 5"),
            Some((Some(Noun::Section), "5".into(), Class::Named(Noun::Section)))
        );
        assert_eq!(
            typed("Section 2.01. Purchase and Sale."),
            Some((
                Some(Noun::Section),
                "2.01".into(),
                Class::Named(Noun::Section)
            ))
        );
    }

    #[test]
    fn text_that_only_starts_with_a_number_is_not_a_label() {
        for text in [
            "2020 was a good year",
            "A.B. Co pays",
            "Section of the act",
            "(the Buyer) pays",
            "Pay on time",
            "",
            "1,000 units",
            "Section 501 of the Code applies.",
            "Article 5 shall prevail",
        ] {
            assert_eq!(typed(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_lone_i_is_a_numeral_unless_it_follows_h() {
        assert_eq!(typed_label("(i) x", None).unwrap().class, Class::ParenRoman);
        assert_eq!(
            typed_label("(i) x", Some("(h)")).unwrap().class,
            Class::ParenLetter
        );
        assert_eq!(
            typed_label("(j) x", Some("(i)")).unwrap().class,
            Class::ParenLetter
        );
    }

    #[test]
    fn normalize_ignores_case_outside_parentheses_spaces_and_a_closing_period() {
        assert_eq!(normalize("4.2 (A)."), "4.2(A)");
        assert_eq!(normalize("4.2 (a)."), "4.2(a)");
        assert_eq!(normalize("III"), "iii");
        assert_eq!(normalize("Section 4.2"), "section4.2");
    }
}
