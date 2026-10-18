//! Number formats: a counter value and a `w:numFmt` give the text Word shows.
//!
//! Tier 1 (the formats real documents use) is exact. Tier 2 (enclosed and width forms) is a table of
//! Unicode characters. Tier 3 (scripts) is not built yet: those formats show decimal and the caller
//! records a note. See the design, section 5.

use crate::wordml::NumFmt;

/// How a format rendered a value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Rendered {
    /// The text.
    pub text: String,
    /// What was approximate, if anything.
    pub approx: Option<Approx>,
}

/// Why a rendering is not what Word shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Approx {
    /// The format is not built, or not known: decimal was shown.
    Unsupported(Box<str>),
    /// The value is outside what the format can show: decimal was shown.
    OutOfRange(Box<str>),
}

fn ok(text: String) -> Rendered {
    Rendered { text, approx: None }
}

fn decimal_with(n: i64, approx: Approx) -> Rendered {
    Rendered {
        text: n.to_string(),
        approx: Some(approx),
    }
}

/// True if the format counts, so that a level with it has a number. `bullet` and `none` do not.
pub(super) fn is_counting(fmt: &NumFmt) -> bool {
    !matches!(fmt, NumFmt::Bullet | NumFmt::None)
}

/// Renders `n` in `fmt`. `bullet` and `none` render nothing.
pub(super) fn render(fmt: &NumFmt, n: i64) -> Rendered {
    match fmt {
        NumFmt::Bullet | NumFmt::None => ok(String::new()),
        NumFmt::Decimal => ok(n.to_string()),
        NumFmt::DecimalZero => {
            if (0..=9).contains(&n) {
                ok(format!("0{n}"))
            } else {
                ok(n.to_string())
            }
        }
        NumFmt::UpperRoman => roman(n, false),
        NumFmt::LowerRoman => roman(n, true),
        NumFmt::UpperLetter => letters(n, false),
        NumFmt::LowerLetter => letters(n, true),
        NumFmt::Ordinal => ok(format!("{n}{}", ordinal_suffix(n))),
        NumFmt::CardinalText => words(n, false, "cardinalText"),
        NumFmt::OrdinalText => words(n, true, "ordinalText"),
        NumFmt::Custom(pattern) => custom(n, pattern),
        NumFmt::Other(name) => other(n, name),
    }
}

/// Standard subtractive numerals for 1 to 3,999. Anything else shows decimal.
fn roman(n: i64, lower: bool) -> Rendered {
    if !(1..=3999).contains(&n) {
        let name = if lower { "lowerRoman" } else { "upperRoman" };
        return decimal_with(n, Approx::OutOfRange(name.into()));
    }
    const TABLE: [(i64, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut rest = n;
    let mut out = String::new();
    for (value, text) in TABLE {
        while rest >= value {
            out.push_str(text);
            rest -= value;
        }
    }
    ok(if lower { out.to_lowercase() } else { out })
}

/// `a` to `z`, then `aa`, `bb` to `zz`, then `aaa`: the letter repeated, not base 26. The count
/// wraps at 780, as Word's does. Zero and negative values show decimal.
fn letters(n: i64, lower: bool) -> Rendered {
    if n < 1 {
        let name = if lower { "lowerLetter" } else { "upperLetter" };
        return decimal_with(n, Approx::OutOfRange(name.into()));
    }
    let wrapped = match n % 780 {
        0 => 780,
        r => r,
    };
    let index = ((wrapped - 1) % 26) as u8;
    let repeat = (wrapped - 1) / 26 + 1;
    let base = if lower { b'a' } else { b'A' };
    let letter = char::from(base.saturating_add(index));
    ok(std::iter::repeat_n(letter, usize::try_from(repeat).unwrap_or(1)).collect())
}

/// `st`, `nd`, `rd` or `th`: `11th`, `12th` and `13th` are the exceptions.
fn ordinal_suffix(n: i64) -> &'static str {
    let last_two = n.unsigned_abs() % 100;
    if (11..=13).contains(&last_two) {
        return "th";
    }
    match n.unsigned_abs() % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];

const TENS: [&str; 10] = [
    "", "ten", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// Words for 1 to 99, hyphenated above twenty (`twenty-one`).
fn below_hundred(n: usize) -> String {
    if n < 20 {
        return ONES.get(n).copied().unwrap_or("").to_string();
    }
    let tens = TENS.get(n / 10).copied().unwrap_or("");
    match n % 10 {
        0 => tens.to_string(),
        r => format!("{tens}-{}", ONES.get(r).copied().unwrap_or("")),
    }
}

/// Words for 1 to 999.
fn below_thousand(n: usize) -> String {
    let hundreds = n / 100;
    let rest = n % 100;
    let mut parts = Vec::new();
    if hundreds > 0 {
        parts.push(format!(
            "{} hundred",
            ONES.get(hundreds).copied().unwrap_or("")
        ));
    }
    if rest > 0 {
        parts.push(below_hundred(rest));
    }
    parts.join(" ")
}

/// The ordinal form of the last word: `one` is `first`, `twenty` is `twentieth`, `hundred` is `hundredth`.
fn ordinalize(word: &str) -> String {
    let (head, last) = match word.rfind('-') {
        Some(i) => (&word[..=i], &word[i + 1..]),
        None => ("", word),
    };
    let ordinal = match last {
        "one" => "first".to_string(),
        "two" => "second".to_string(),
        "three" => "third".to_string(),
        "five" => "fifth".to_string(),
        "eight" => "eighth".to_string(),
        "nine" => "ninth".to_string(),
        "twelve" => "twelfth".to_string(),
        l if l.ends_with('y') => format!("{}ieth", &l[..l.len() - 1]),
        l => format!("{l}th"),
    };
    format!("{head}{ordinal}")
}

/// `One` to `Ninety-nine thousand nine hundred ninety-nine`, or the ordinal form (`First`), with a
/// capital first letter. Zero, negative and larger values show decimal.
fn words(n: i64, ordinal: bool, name: &str) -> Rendered {
    if !(1..=99_999).contains(&n) {
        return decimal_with(n, Approx::OutOfRange(name.into()));
    }
    let n = n as usize;
    let mut parts = Vec::new();
    if n >= 1000 {
        parts.push(format!("{} thousand", below_hundred(n / 1000)));
    }
    if !n.is_multiple_of(1000) {
        parts.push(below_thousand(n % 1000));
    }
    let mut text = parts.join(" ");
    if ordinal {
        let cut = text.rfind(' ').map_or(0, |i| i + 1);
        let last = text[cut..].to_string();
        text.truncate(cut);
        text.push_str(&ordinalize(&last));
    }
    let mut chars = text.chars();
    let first = chars.next().map(|c| c.to_uppercase().collect::<String>());
    ok(first.unwrap_or_default() + chars.as_str())
}

/// A custom format such as `001, 002, 003, ...`: the digits pattern gives the width. Anything
/// else shows decimal and is reported.
fn custom(n: i64, pattern: &str) -> Rendered {
    let first = pattern.split(',').next().unwrap_or("").trim();
    if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) && n >= 0 {
        return ok(format!("{n:0width$}", width = first.chars().count()));
    }
    decimal_with(n, Approx::Unsupported(format!("custom:{pattern}").into()))
}

/// Tier 2 formats, and the formats not built yet.
fn other(n: i64, name: &str) -> Rendered {
    let in_20 = (1..=20).contains(&n);
    let from = |base: u32| {
        // `n` is between 1 and 20 here.
        char::from_u32(base.saturating_add(u32::try_from(n - 1).unwrap_or(0)))
            .map(|c| c.to_string())
    };
    let enclosed = match name {
        "decimalEnclosedCircle" | "decimalEnclosedCircleChinese" if in_20 => from(0x2460),
        "decimalEnclosedParen" if in_20 => from(0x2474),
        "decimalEnclosedFullstop" if in_20 => from(0x2488),
        _ => None,
    };
    if let Some(text) = enclosed {
        return ok(text);
    }
    match name {
        "decimalEnclosedCircle"
        | "decimalEnclosedCircleChinese"
        | "decimalEnclosedParen"
        | "decimalEnclosedFullstop" => decimal_with(n, Approx::OutOfRange(name.into())),
        "decimalHalfWidth" => ok(n.to_string()),
        "decimalFullWidth" | "decimalFullWidth2" => ok(n
            .to_string()
            .chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => char::from_u32(0xFF10 + d).unwrap_or(c),
                None => c,
            })
            .collect()),
        "numberInDash" => ok(format!("-{n}-")),
        "chicago" => chicago(n),
        "hex" if n >= 0 => ok(format!("{n:X}")),
        _ => decimal_with(n, Approx::Unsupported(name.into())),
    }
}

/// `*`, `†`, `‡`, `§`, then each repeated: `**`, `††`, and so on.
fn chicago(n: i64) -> Rendered {
    if n < 1 {
        return decimal_with(n, Approx::OutOfRange("chicago".into()));
    }
    const MARKS: [char; 4] = ['*', '\u{2020}', '\u{2021}', '\u{00A7}'];
    let index = usize::try_from((n - 1) % 4).unwrap_or(0);
    let repeat = usize::try_from((n - 1) / 4 + 1).unwrap_or(1);
    let mark = MARKS.get(index).copied().unwrap_or('*');
    ok(std::iter::repeat_n(mark, repeat.min(64)).collect())
}
