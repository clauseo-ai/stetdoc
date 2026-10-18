//! The quote policy, as tables.

use super::*;

fn curly(text: &str) -> Followed {
    follow(text, Context::Curly)
}

fn straight(text: &str) -> Followed {
    follow(text, Context::Straight)
}

#[test]
fn a_curly_context_curls_straight_quotes_by_where_they_stand() {
    let table = [
        ("He said \"hello\"", "He said \u{201C}hello\u{201D}"),
        (
            "\"Losses\" means any loss",
            "\u{201C}Losses\u{201D} means any loss",
        ),
        ("the Supplier's costs", "the Supplier\u{2019}s costs"),
        ("'quoted' text", "\u{2018}quoted\u{2019} text"),
        ("(\"the Company\")", "(\u{201C}the Company\u{201D})"),
        ("in the '90s", "in the \u{2019}90s"),
        ("end.\" Next", "end.\u{201D} Next"),
        ("a\u{2014}\"b\"", "a\u{2014}\u{201C}b\u{201D}"),
    ];
    for (typed, expected) in table {
        let followed = curly(typed);
        assert_eq!(followed.text, expected, "{typed}");
        assert_eq!(followed.straightened, 0);
        assert!(followed.curled > 0);
    }
}

#[test]
fn a_quote_after_a_digit_is_feet_or_inches_when_no_quotation_is_open() {
    for typed in ["a 6' pole", "a 12\" pipe", "5'11\"", "x 3\" by 4\""] {
        assert_eq!(curly(typed).text, typed, "{typed}");
        assert_eq!(curly(typed).curled, 0, "{typed}");
    }
    // Curly quotes after a digit are straightened in a straight context, like any other.
    assert_eq!(straight("6\u{2019}").text, "6'");
}

#[test]
fn a_straight_quote_after_a_digit_closes_the_quotation_that_is_open() {
    let table = [
        ("\"Clause 12\"", "\u{201C}Clause 12\u{201D}"),
        ("\"12\"", "\u{201C}12\u{201D}"),
        (
            "see \"clause 12\" and \"clause 3.1\"",
            "see \u{201C}clause 12\u{201D} and \u{201C}clause 3.1\u{201D}",
        ),
        // A curly quote typed to open it, and a straight one to close it.
        ("\u{201C}Clause 12\"", "\u{201C}Clause 12\u{201D}"),
        ("'Clause 12'", "\u{2018}Clause 12\u{2019}"),
        // Feet and inches inside and outside a quotation keep their marks.
        ("\"a 6' pole\" he said", "\u{201C}a 6' pole\u{201D} he said"),
        ("5'11\" and \"x\"", "5'11\" and \u{201C}x\u{201D}"),
        ("12\" then \"a 3\"", "12\" then \u{201C}a 3\u{201D}"),
    ];
    for (typed, expected) in table {
        assert_eq!(curly(typed).text, expected, "{typed}");
    }
}

#[test]
fn a_straight_context_straightens_curly_quotes() {
    let followed = straight("\u{201C}Losses\u{201D} and the Supplier\u{2019}s");
    assert_eq!(followed.text, "\"Losses\" and the Supplier's");
    assert_eq!(followed.straightened, 3);
    assert_eq!(followed.curled, 0);
}

#[test]
fn an_unknown_context_changes_nothing() {
    for typed in ["plain", "\"x\"", "it's", "\u{201C}x\u{201D}"] {
        let followed = follow(typed, Context::Unknown);
        assert_eq!(followed.text, typed);
        assert_eq!((followed.curled, followed.straightened), (0, 0));
    }
}

#[test]
fn the_paragraph_decides_and_then_the_nearest_neighbour_that_has_a_quote() {
    assert_eq!(
        context("the \u{201C}Company\u{201D}", ["a \"b\""]),
        Context::Curly
    );
    assert_eq!(
        context("the \"Company\"", ["\u{201C}b\u{201D}"]),
        Context::Straight
    );
    assert_eq!(
        context("no quote", ["none either", "it\u{2019}s", "'x'"]),
        Context::Curly
    );
    assert_eq!(
        context("no quote", ["none", "'x'", "\u{201C}y\u{201D}"]),
        Context::Straight
    );
    assert_eq!(context("no quote", ["none"]), Context::Unknown);
    assert_eq!(context("no quote", []), Context::Unknown);
}

#[test]
fn the_character_before_the_text_decides_its_first_quote() {
    // Typed after a word, a single quote is an apostrophe; after a space or at the start of a paragraph it opens.
    let table = [
        ("'s", Some('r'), "\u{2019}s"),
        ("'s", Some(' '), "\u{2018}s"),
        ("'s", None, "\u{2018}s"),
        ("\" said", Some('d'), "\u{201D} said"),
        ("\"Term\"", Some('('), "\u{201C}Term\u{201D}"),
        ("\" long", Some('6'), "\" long"),
    ];
    for (typed, before, expected) in table {
        let followed = follow_after(typed, Context::Curly, before);
        assert_eq!(followed.text, expected, "{typed} after {before:?}");
    }
    // A straight context does not read it.
    assert_eq!(
        follow_after("\u{2019}s", Context::Straight, Some('r')).text,
        "'s"
    );
}

#[test]
fn following_a_context_twice_is_the_same_as_once() {
    for typed in [
        "He said \"hello\" and it's 6' tall",
        "'x' \"y\" 5\"",
        "\u{201C}a\u{201D} \u{2019}",
        "\"Clause 12\" and 'clause 3' and 5\" by 6'",
        "12\" \"x",
    ] {
        for context in [Context::Curly, Context::Straight, Context::Unknown] {
            let once = follow(typed, context).text;
            assert_eq!(follow(&once, context).text, once, "{typed} {context:?}");
        }
    }
}

#[test]
fn folding_makes_curly_and_straight_the_same() {
    assert_eq!(
        fold("\u{201C}the \u{2018}Supplier\u{2019}\u{201D}"),
        "\"the 'Supplier'\""
    );
    assert_eq!(fold("no quotes"), "no quotes");
}

#[test]
fn following_a_context_twice_is_the_same_as_once_for_any_mix_of_quotes_digits_and_letters() {
    // H6, the idempotence half, over generated text: the state that a quotation is open follows the characters written, so the
    // second pass decides each remaining quote as the first did.
    const ALPHABET: [char; 16] = [
        '"', '\'', '\u{201C}', '\u{201D}', '\u{2018}', '\u{2019}', '1', '2', 'a', 'b', ' ', ' ',
        '.', '(', ')', '-',
    ];
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut next = |n: usize| {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((state >> 33) as usize) % n
    };
    for round in 0..20_000 {
        let length = 1 + next(14);
        let typed: String = (0..length)
            .map(|_| ALPHABET[next(ALPHABET.len())])
            .collect();
        for context in [Context::Curly, Context::Straight] {
            let once = follow(&typed, context).text;
            assert_eq!(
                follow(&once, context).text,
                once,
                "round {round}: {typed:?} {context:?}"
            );
        }
    }
}

#[test]
fn an_edit_follows_the_context_only_where_it_differs_from_the_old_text() {
    // A mixed paragraph: the straight apostrophe in O'Neill is carried over, and only the typed quotes change.
    let old = "Brian O'Neill (\u{201C}Employee\u{201D}) and the agreement";
    let new = "Brian O'Neill (\u{201C}Employee\u{201D}) and the \"deal\"";
    let followed = follow_edit(old, new, Context::Curly, None);
    assert_eq!(
        followed.text,
        "Brian O'Neill (\u{201C}Employee\u{201D}) and the \u{201C}deal\u{201D}"
    );
    assert_eq!((followed.curled, followed.straightened), (2, 0));
    // A straight quote typed over a curly one becomes the curly one, so nothing changes.
    let followed = follow_edit(
        "the Supplier\u{2019}s",
        "the Supplier's",
        Context::Curly,
        None,
    );
    assert_eq!(followed.text, "the Supplier\u{2019}s");
    // A curly quote carried over in a straight paragraph stays, a typed one is straightened.
    let followed = follow_edit(
        "it\u{2019}s",
        "it\u{2019}s \u{2018}x\u{2019}",
        Context::Straight,
        None,
    );
    assert_eq!(followed.text, "it\u{2019}s 'x'");
    // With no old text everything is typed.
    assert_eq!(
        follow_edit("", "the Supplier's", Context::Curly, None),
        follow("the Supplier's", Context::Curly)
    );
}
