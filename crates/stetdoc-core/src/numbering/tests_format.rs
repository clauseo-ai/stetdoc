//! Tests for the number formats. Each test says what it proves.

use super::format::{Approx, render};
use super::label::{Piece, contains_ancestors, expand, parse, strip_trailing_periods};
use crate::wordml::NumFmt;

fn t(fmt: &NumFmt, n: i64) -> String {
    render(fmt, n).text
}

fn other(name: &str) -> NumFmt {
    NumFmt::Other(name.into())
}

#[test]
fn decimal_and_decimal_zero() {
    // Proves decimal is the number, negative included, and decimalZero pads only 0 to 9.
    assert_eq!(t(&NumFmt::Decimal, 7), "7");
    assert_eq!(t(&NumFmt::Decimal, -3), "-3");
    assert_eq!(t(&NumFmt::DecimalZero, 3), "03");
    assert_eq!(t(&NumFmt::DecimalZero, 0), "00");
    assert_eq!(t(&NumFmt::DecimalZero, 10), "10");
    assert_eq!(t(&NumFmt::DecimalZero, 123), "123");
}

#[test]
fn roman_numerals() {
    // Register: U21.
    // Proves subtractive numerals, both cases, and the decimal fallback outside 1 to 3,999.
    let cases = [
        (1, "I"),
        (4, "IV"),
        (9, "IX"),
        (14, "XIV"),
        (40, "XL"),
        (90, "XC"),
        (400, "CD"),
        (1994, "MCMXCIV"),
        (3999, "MMMCMXCIX"),
    ];
    for (n, want) in cases {
        assert_eq!(t(&NumFmt::UpperRoman, n), want);
        assert_eq!(t(&NumFmt::LowerRoman, n), want.to_lowercase());
    }
    for n in [0, -1, 4000, i64::MAX] {
        let r = render(&NumFmt::UpperRoman, n);
        assert_eq!(r.text, n.to_string());
        assert!(matches!(r.approx, Some(Approx::OutOfRange(_))));
    }
}

#[test]
fn letters_repeat_and_wrap() {
    // Proves Word's rule: a to z, then aa, bb, cc (the letter repeated), wrapping at 780.
    let lower = NumFmt::LowerLetter;
    assert_eq!(t(&lower, 1), "a");
    assert_eq!(t(&lower, 26), "z");
    assert_eq!(t(&lower, 27), "aa");
    assert_eq!(t(&lower, 28), "bb");
    assert_eq!(t(&lower, 52), "zz");
    assert_eq!(t(&lower, 53), "aaa");
    assert_eq!(
        t(&NumFmt::UpperLetter, 780),
        "ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ"
    );
    assert_eq!(t(&NumFmt::UpperLetter, 781), "A");
    assert_eq!(t(&NumFmt::UpperLetter, 3), "C");
    for n in [0, -5] {
        let r = render(&lower, n);
        assert_eq!(r.text, n.to_string());
        assert!(r.approx.is_some());
    }
}

#[test]
fn ordinals() {
    // Proves st, nd, rd, th, and the teens and 111 to 113 as th.
    let o = NumFmt::Ordinal;
    for (n, want) in [
        (1, "1st"),
        (2, "2nd"),
        (3, "3rd"),
        (4, "4th"),
        (11, "11th"),
        (12, "12th"),
        (13, "13th"),
        (21, "21st"),
        (22, "22nd"),
        (101, "101st"),
        (111, "111th"),
        (112, "112th"),
        (113, "113th"),
        (0, "0th"),
    ] {
        assert_eq!(t(&o, n), want, "ordinal {n}");
    }
}

#[test]
fn spelled_out_numbers() {
    // Proves English words with a capital first letter, hyphens above twenty, and the ordinal forms.
    let c = NumFmt::CardinalText;
    let o = NumFmt::OrdinalText;
    for (n, card, ord) in [
        (1, "One", "First"),
        (2, "Two", "Second"),
        (3, "Three", "Third"),
        (5, "Five", "Fifth"),
        (8, "Eight", "Eighth"),
        (9, "Nine", "Ninth"),
        (12, "Twelve", "Twelfth"),
        (13, "Thirteen", "Thirteenth"),
        (19, "Nineteen", "Nineteenth"),
        (20, "Twenty", "Twentieth"),
        (21, "Twenty-one", "Twenty-first"),
        (30, "Thirty", "Thirtieth"),
        (99, "Ninety-nine", "Ninety-ninth"),
        (100, "One hundred", "One hundredth"),
        (101, "One hundred one", "One hundred first"),
        (342, "Three hundred forty-two", "Three hundred forty-second"),
        (1000, "One thousand", "One thousandth"),
        (1001, "One thousand one", "One thousand first"),
        (
            21_345,
            "Twenty-one thousand three hundred forty-five",
            "Twenty-one thousand three hundred forty-fifth",
        ),
    ] {
        assert_eq!(t(&c, n), card, "cardinal {n}");
        assert_eq!(t(&o, n), ord, "ordinal {n}");
    }
    for n in [0, -1, 100_000] {
        assert!(render(&c, n).approx.is_some());
        assert_eq!(t(&c, n), n.to_string());
    }
}

#[test]
fn bullet_and_none_render_nothing() {
    // Proves the two formats that have no counter text.
    assert_eq!(t(&NumFmt::Bullet, 5), "");
    assert_eq!(t(&NumFmt::None, 5), "");
}

#[test]
fn enclosed_and_width_forms() {
    // Proves the tier 2 tables and their fallback beyond 20.
    assert_eq!(t(&other("decimalEnclosedCircle"), 1), "\u{2460}");
    assert_eq!(t(&other("decimalEnclosedCircle"), 20), "\u{2473}");
    assert_eq!(t(&other("decimalEnclosedCircle"), 21), "21");
    assert_eq!(t(&other("decimalEnclosedParen"), 3), "\u{2476}");
    assert_eq!(t(&other("decimalEnclosedFullstop"), 1), "\u{2488}");
    assert_eq!(t(&other("decimalFullWidth"), 12), "\u{FF11}\u{FF12}");
    assert_eq!(t(&other("decimalHalfWidth"), 12), "12");
    assert_eq!(t(&other("numberInDash"), 7), "-7-");
    assert_eq!(t(&other("chicago"), 1), "*");
    assert_eq!(t(&other("chicago"), 2), "\u{2020}");
    assert_eq!(t(&other("chicago"), 5), "**");
    assert_eq!(t(&other("hex"), 255), "FF");
}

#[test]
fn custom_patterns_and_unknown_formats() {
    // Proves a digits pattern gives the width, and an unknown format is decimal and reported.
    assert_eq!(t(&NumFmt::Custom("001, 002, 003, ...".into()), 7), "007");
    assert_eq!(t(&NumFmt::Custom("01, 02, 03, ...".into()), 7), "07");
    let r = render(&NumFmt::Custom("".into()), 7);
    assert_eq!(r.text, "7");
    assert!(matches!(r.approx, Some(Approx::Unsupported(_))));
    let r = render(&other("hindiVowels"), 4);
    assert_eq!(r.text, "4");
    assert_eq!(r.approx, Some(Approx::Unsupported("hindiVowels".into())));
}

#[test]
fn level_text_is_split_into_literals_and_placeholders() {
    // Proves %1 to %9, %% as a percent, and a lone % as itself.
    use Piece::{Level, Literal};
    assert_eq!(
        parse("%1.%2(%3)"),
        vec![
            Level(0),
            Literal(".".into()),
            Level(1),
            Literal("(".into()),
            Level(2),
            Literal(")".into())
        ]
    );
    assert_eq!(parse("100%%"), vec![Literal("100%".into())]);
    assert_eq!(parse("50%"), vec![Literal("50%".into())]);
    assert_eq!(parse("%0 %a"), vec![Literal("%0 %a".into())]);
    assert_eq!(parse("%9"), vec![Level(8)]);
    assert_eq!(parse(""), Vec::<Piece>::new());
}

#[test]
fn expansion_trims_and_asks_for_each_level() {
    // Proves the label is trimmed and placeholders are asked for in order.
    assert_eq!(expand(" %1.%2 ", |j| format!("<{j}>")), "<0>.<1>");
    assert_eq!(expand("(%3)", |_| "a".into()), "(a)");
    assert_eq!(expand("Article %1", |_| String::new()), "Article");
}

#[test]
fn ancestors_and_trailing_periods() {
    // Proves which level texts carry their ancestors, and that only periods are stripped.
    assert!(contains_ancestors("%1.%2.%3", 2));
    assert!(!contains_ancestors("%3", 2));
    assert!(!contains_ancestors("%1.%3", 2));
    assert!(contains_ancestors("%1", 0));
    assert_eq!(strip_trailing_periods("26."), "26");
    assert_eq!(strip_trailing_periods("(a)"), "(a)");
    assert_eq!(strip_trailing_periods("1.2.. "), "1.2");
}
