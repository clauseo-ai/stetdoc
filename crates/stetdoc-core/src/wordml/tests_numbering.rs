//! Tests for `NumberingDefs`. Each test names what it proves.

use super::names::{W_S, W_T};
use super::numbering_defs::{LevelOwner, NumberingDefs, NumberingIssue};
use super::numbering_levels::{MultiLevel, NumFmt, Suff};
use crate::api::limits::Limits;
use crate::xml::{Bytes, SpanTree};

fn tree(xml: &str) -> SpanTree {
    SpanTree::parse(
        "numbering.xml",
        Bytes::from(xml.as_bytes()),
        &Limits::default(),
    )
    .unwrap()
}

fn read_in(w: &str, body: &str) -> NumberingDefs {
    NumberingDefs::read(&tree(&format!(
        "<w:numbering xmlns:w=\"{w}\">{body}</w:numbering>"
    )))
}

fn read(body: &str) -> NumberingDefs {
    read_in(W_T, body)
}

fn lvl(ilvl: u32, inner: &str) -> String {
    format!("<w:lvl w:ilvl=\"{ilvl}\">{inner}</w:lvl>")
}

fn abstract_num(id: i64, inner: &str) -> String {
    format!("<w:abstractNum w:abstractNumId=\"{id}\">{inner}</w:abstractNum>")
}

fn num(id: i64, abstract_id: i64, inner: &str) -> String {
    format!("<w:num w:numId=\"{id}\"><w:abstractNumId w:val=\"{abstract_id}\"/>{inner}</w:num>")
}

#[test]
fn a_level_is_read_in_full() {
    // Proves every field of `AbstractNum` and `Level`, in both dialects.
    for w in [W_T, W_S] {
        let d = read_in(
            w,
            "<w:abstractNum w:abstractNumId=\"3\"><w:multiLevelType w:val=\"multilevel\"/>\
             <w:styleLink w:val=\"ListA\"/><w:numStyleLink w:val=\"ListB\"/>\
             <w:lvl w:ilvl=\"1\"><w:start w:val=\"4\"/><w:numFmt w:val=\"lowerLetter\"/>\
             <w:lvlRestart w:val=\"0\"/><w:pStyle w:val=\"Heading2\"/><w:isLgl/><w:suff w:val=\"space\"/>\
             <w:lvlText w:val=\"%1.%2(%3)\"/><w:lvlPicBulletId w:val=\"7\"/><w:lvlJc w:val=\"right\"/>\
             <w:pPr><w:ind w:left=\"720\"/></w:pPr><w:rPr><w:b/></w:rPr></w:lvl></w:abstractNum>\
             <w:num w:numId=\"1\"><w:abstractNumId w:val=\"3\"/></w:num>",
        );
        let a = d.abstract_of(1).unwrap();
        assert_eq!(a.id, 3);
        assert_eq!(a.multi_level, MultiLevel::Multi);
        assert_eq!(a.style_link.as_deref(), Some("ListA"));
        assert_eq!(a.num_style_link.as_deref(), Some("ListB"));
        let l = &a.levels[0];
        assert_eq!(l.ilvl, 1);
        assert_eq!(l.start, 4);
        assert_eq!(l.fmt, NumFmt::LowerLetter);
        assert_eq!(&*l.text, "%1.%2(%3)");
        assert_eq!(l.restart, Some(0));
        assert!(l.legal);
        assert_eq!(l.suff, Suff::Space);
        assert_eq!(l.style.as_deref(), Some("Heading2"));
        assert_eq!(l.justification.as_deref(), Some("right"));
        assert_eq!(l.paragraph.attr("ind", "left"), Some("720"));
        assert_eq!(l.run.on_off("b"), Some(true));
        assert_eq!(l.pic_bullet, Some(7));
        assert!(d.issues().is_empty(), "{:?}", d.issues());
    }
}

#[test]
fn start_defaults_to_zero_and_the_other_defaults_hold() {
    // Proves ISO 29500 17.9.26: an omitted `w:start` is 0, not 1. Also the other defaults.
    let d = read(&format!(
        "{}{}",
        abstract_num(0, &lvl(0, "<w:numFmt w:val=\"decimal\"/>")),
        num(1, 0, "")
    ));
    let l = d.level(1, 0).unwrap();
    assert_eq!(l.level.start, 0);
    assert_eq!(l.start, 0);
    assert_eq!(l.level.fmt, NumFmt::Decimal);
    assert_eq!(l.level.restart, None);
    assert_eq!(l.level.suff, Suff::Tab);
    assert!(!l.level.legal);
    assert_eq!(&*l.level.text, "");
    assert_eq!(d.abstract_of(1).unwrap().multi_level, MultiLevel::Unknown);
}

#[test]
fn a_start_of_one_is_kept_as_one() {
    // Proves an explicit start is read as written.
    let d = read(&format!(
        "{}{}",
        abstract_num(0, &lvl(0, "<w:start w:val=\"1\"/>")),
        num(1, 0, "")
    ));
    assert_eq!(d.level(1, 0).unwrap().start, 1);
}

#[test]
fn the_multi_level_type_values() {
    // Proves the three values and that anything else is `Unknown`.
    let d = read(&format!(
        "{}{}{}{}",
        abstract_num(1, "<w:multiLevelType w:val=\"singleLevel\"/>"),
        abstract_num(2, "<w:multiLevelType w:val=\"hybridMultilevel\"/>"),
        abstract_num(3, "<w:multiLevelType w:val=\"multilevel\"/>"),
        abstract_num(4, "<w:multiLevelType w:val=\"weird\"/>"),
    ));
    let got: Vec<MultiLevel> = d.abstracts().iter().map(|a| a.multi_level).collect();
    assert_eq!(
        got,
        [
            MultiLevel::Single,
            MultiLevel::Hybrid,
            MultiLevel::Multi,
            MultiLevel::Unknown
        ]
    );
}

#[test]
fn number_formats_and_unknown_ones_in_other() {
    // Proves the named formats and that any other value is kept in `Other`.
    let names = [
        ("decimal", NumFmt::Decimal),
        ("upperRoman", NumFmt::UpperRoman),
        ("lowerRoman", NumFmt::LowerRoman),
        ("upperLetter", NumFmt::UpperLetter),
        ("lowerLetter", NumFmt::LowerLetter),
        ("ordinal", NumFmt::Ordinal),
        ("cardinalText", NumFmt::CardinalText),
        ("ordinalText", NumFmt::OrdinalText),
        ("decimalZero", NumFmt::DecimalZero),
        ("bullet", NumFmt::Bullet),
        ("none", NumFmt::None),
        ("chineseCounting", NumFmt::Other("chineseCounting".into())),
        ("Decimal", NumFmt::Other("Decimal".into())),
    ];
    for (name, want) in names {
        let d = read(&abstract_num(
            0,
            &lvl(0, &format!("<w:numFmt w:val=\"{name}\"/>")),
        ));
        assert_eq!(d.abstracts()[0].levels[0].fmt, want, "{name}");
    }
}

#[test]
fn suffix_values() {
    // Proves `w:suff`: tab is the default, space and nothing are read, an unknown value is tab.
    let d = read(&abstract_num(
        0,
        &format!(
            "{}{}{}{}",
            lvl(0, "<w:suff w:val=\"nothing\"/>"),
            lvl(1, "<w:suff w:val=\"tab\"/>"),
            lvl(2, "<w:suff w:val=\"space\"/>"),
            lvl(3, "<w:suff w:val=\"zzz\"/>"),
        ),
    ));
    let s: Vec<Suff> = d.abstracts()[0].levels.iter().map(|l| l.suff).collect();
    assert_eq!(s, [Suff::Nothing, Suff::Tab, Suff::Space, Suff::Tab]);
}

#[test]
fn levels_are_sorted_and_at_most_nine() {
    // Proves sorting by `ilvl`, and that a tenth level (ilvl 9) and a repeated one are not kept.
    let levels: String = [8u32, 3, 0, 1, 2, 4, 5, 6, 7, 9]
        .iter()
        .map(|i| lvl(*i, ""))
        .chain(std::iter::once(lvl(3, "<w:start w:val=\"9\"/>")))
        .collect();
    let d = read(&abstract_num(0, &levels));
    let a = &d.abstracts()[0];
    let order: Vec<u8> = a.levels.iter().map(|l| l.ilvl).collect();
    assert_eq!(order, [0, 1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(a.levels[3].start, 0);
    assert_eq!(
        d.issues(),
        [
            NumberingIssue::InvalidLevel {
                owner: LevelOwner::AbstractNum(0),
                value: "9".into()
            },
            NumberingIssue::DuplicateLevel {
                abstract_id: 0,
                ilvl: 3
            }
        ]
    );
}

#[test]
fn a_level_without_a_usable_ilvl_is_skipped() {
    // Proves a missing or non-numeric `w:ilvl` is reported and the level dropped.
    let d = read(&abstract_num(
        0,
        "<w:lvl/><w:lvl w:ilvl=\"x\"/><w:lvl w:ilvl=\"-1\"/>",
    ));
    assert!(d.abstracts()[0].levels.is_empty());
    assert_eq!(d.issues().len(), 3);
}

#[test]
fn restart_none_default_and_zero_never() {
    // Proves `lvlRestart`: omitted is `None`, 0 is `Some(0)`, and an out of range value is reported.
    let d = read(&abstract_num(
        0,
        &format!(
            "{}{}{}{}",
            lvl(0, ""),
            lvl(1, "<w:lvlRestart w:val=\"0\"/>"),
            lvl(2, "<w:lvlRestart w:val=\"1\"/>"),
            lvl(3, "<w:lvlRestart w:val=\"12\"/>"),
        ),
    ));
    let r: Vec<Option<u8>> = d.abstracts()[0].levels.iter().map(|l| l.restart).collect();
    assert_eq!(r, [None, Some(0), Some(1), None]);
    assert_eq!(
        d.issues(),
        [NumberingIssue::InvalidValue {
            owner: LevelOwner::AbstractNum(0),
            ilvl: 3,
            element: "w:lvlRestart",
            value: "12".into()
        }]
    );
}

#[test]
fn an_invalid_start_is_reported_and_defaults_to_zero() {
    // Proves a non-numeric `w:start` is an issue with the default applied.
    let d = read(&abstract_num(0, &lvl(0, "<w:start w:val=\"one\"/>")));
    assert_eq!(d.abstracts()[0].levels[0].start, 0);
    assert!(matches!(
        &d.issues()[0],
        NumberingIssue::InvalidValue {
            element: "w:start",
            ..
        }
    ));
}

#[test]
fn start_override_changes_only_the_start() {
    // Proves `w:startOverride` keeps the abstract level and changes the start.
    let d = read(&format!(
        "{}{}",
        abstract_num(
            0,
            &lvl(0, "<w:start w:val=\"1\"/><w:numFmt w:val=\"upperRoman\"/>")
        ),
        num(
            5,
            0,
            "<w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"7\"/></w:lvlOverride>"
        )
    ));
    let l = d.level(5, 0).unwrap();
    assert_eq!(l.start, 7);
    assert_eq!(l.level.start, 1);
    assert_eq!(l.level.fmt, NumFmt::UpperRoman);
    assert!(!l.replaced);
    assert_eq!(l.abstract_id, 0);
}

#[test]
fn a_level_override_replaces_the_level() {
    // Proves an override with `w:lvl` replaces the level, and that startOverride then applies to it.
    let d = read(&format!(
        "{}{}{}",
        abstract_num(
            0,
            &format!(
                "{}{}",
                lvl(0, "<w:start w:val=\"1\"/><w:numFmt w:val=\"decimal\"/>"),
                lvl(1, "<w:numFmt w:val=\"decimal\"/>")
            )
        ),
        num(
            1,
            0,
            "<w:lvlOverride w:ilvl=\"0\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"3\"/>\
             <w:numFmt w:val=\"bullet\"/></w:lvl></w:lvlOverride>\
             <w:lvlOverride w:ilvl=\"1\"><w:startOverride w:val=\"9\"/><w:lvl w:ilvl=\"1\">\
             <w:start w:val=\"2\"/></w:lvl></w:lvlOverride>"
        ),
        num(2, 0, ""),
    ));
    let a = d.level(1, 0).unwrap();
    assert!(a.replaced);
    assert_eq!(a.level.fmt, NumFmt::Bullet);
    assert_eq!(a.start, 3);
    let b = d.level(1, 1).unwrap();
    assert!(b.replaced);
    assert_eq!((b.level.start, b.start), (2, 9));
    let plain = d.level(2, 0).unwrap();
    assert_eq!(plain.level.fmt, NumFmt::Decimal);
    assert_eq!(plain.start, 1);
}

#[test]
fn a_repeated_override_and_a_bad_override_level_are_reported() {
    // Proves the first override of a level wins and a bad `w:ilvl` is reported.
    let d = read(&format!(
        "{}{}",
        abstract_num(0, &lvl(0, "")),
        num(
            1,
            0,
            "<w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"5\"/></w:lvlOverride>\
             <w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"6\"/></w:lvlOverride>\
             <w:lvlOverride w:ilvl=\"11\"/>\
             <w:lvlOverride w:ilvl=\"1\"><w:startOverride w:val=\"q\"/></w:lvlOverride>"
        )
    ));
    assert_eq!(d.level(1, 0).unwrap().start, 5);
    assert_eq!(d.num(1).unwrap().overrides.len(), 2);
    assert_eq!(d.issues().len(), 3);
}

#[test]
fn level_of_a_missing_level_or_list_is_none() {
    // Proves `level`, `num` and `abstract_of` return `None` for what does not exist.
    let d = read(&format!(
        "{}{}",
        abstract_num(0, &lvl(0, "")),
        num(1, 0, "")
    ));
    assert!(d.level(1, 1).is_none());
    assert!(d.level(2, 0).is_none());
    assert!(d.num(2).is_none());
    assert!(d.abstract_of(2).is_none());
    assert!(d.abstract_num(0).is_some());
}

#[test]
fn a_num_without_its_abstract_is_reported_and_resolves_to_nothing() {
    // Proves the dangling abstract issue and that `abstract_of` and `level` give nothing.
    let d = read(&num(4, 9, ""));
    assert_eq!(d.num(4).unwrap().abstract_id, 9);
    assert!(d.abstract_of(4).is_none());
    assert!(d.level(4, 0).is_none());
    assert_eq!(
        d.issues(),
        [NumberingIssue::MissingAbstract {
            num_id: 4,
            abstract_id: 9
        }]
    );
}

#[test]
fn duplicate_ids_keep_the_first() {
    // Proves the first `abstractNumId` and the first `numId` win, and the rest are issues.
    let d = read(&format!(
        "{}{}{}{}",
        abstract_num(1, "<w:multiLevelType w:val=\"singleLevel\"/>"),
        abstract_num(1, "<w:multiLevelType w:val=\"multilevel\"/>"),
        num(7, 1, ""),
        num(7, 2, ""),
    ));
    assert_eq!(d.abstracts().len(), 1);
    assert_eq!(d.abstract_num(1).unwrap().multi_level, MultiLevel::Single);
    assert_eq!(d.nums().len(), 1);
    assert_eq!(d.num(7).unwrap().abstract_id, 1);
    assert_eq!(
        d.issues(),
        [
            NumberingIssue::DuplicateAbstractNum { id: 1 },
            NumberingIssue::DuplicateNum { id: 7 }
        ]
    );
}

#[test]
fn ids_that_are_not_whole_numbers_are_skipped() {
    // Proves a bad id on either element, and a bad `abstractNumId` reference, skip it with an issue.
    let d = read(
        "<w:abstractNum w:abstractNumId=\"x\"/><w:abstractNum/>\
         <w:num w:numId=\"1.5\"><w:abstractNumId w:val=\"0\"/></w:num>\
         <w:num w:numId=\"3\"><w:abstractNumId w:val=\"y\"/></w:num>\
         <w:num w:numId=\"4\"/>",
    );
    assert!(d.abstracts().is_empty() && d.nums().is_empty());
    assert_eq!(
        d.issues(),
        [
            NumberingIssue::InvalidAbstractNumId { value: "x".into() },
            NumberingIssue::InvalidAbstractNumId { value: "".into() },
            NumberingIssue::InvalidNumId {
                value: "1.5".into()
            },
            NumberingIssue::InvalidNumAbstractId { num_id: 3 },
            NumberingIssue::InvalidNumAbstractId { num_id: 4 },
        ]
    );
}

#[test]
fn negative_and_large_ids_are_whole_numbers() {
    // Proves ids are `i64`: negative and beyond `u32` read.
    let d = read(&format!(
        "{}{}",
        abstract_num(-2, &lvl(0, "")),
        num(5_000_000_000, -2, "")
    ));
    assert!(d.abstract_of(5_000_000_000).is_some());
    assert!(d.issues().is_empty());
}

#[test]
fn style_links_are_recorded_and_not_followed() {
    // Proves `numStyleLink` and `styleLink` are stored on the abstract definition and the linked
    // definition is not substituted.
    let d = read(&format!(
        "{}{}{}{}",
        abstract_num(0, "<w:styleLink w:val=\"MyList\"/>"),
        abstract_num(
            1,
            &format!("<w:numStyleLink w:val=\"MyList\"/>{}", lvl(0, ""))
        ),
        num(1, 0, ""),
        num(2, 1, ""),
    ));
    assert_eq!(
        d.abstract_of(1).unwrap().style_link.as_deref(),
        Some("MyList")
    );
    let two = d.abstract_of(2).unwrap();
    assert_eq!(two.num_style_link.as_deref(), Some("MyList"));
    assert_eq!(two.levels.len(), 1);
    assert!(d.abstract_of(1).unwrap().levels.is_empty());
}

#[test]
fn a_root_that_is_not_numbering_gives_empty_definitions_and_an_issue() {
    // Proves reading is total for the wrong root.
    let d = NumberingDefs::read(&tree(&format!("<w:styles xmlns:w=\"{W_T}\"/>")));
    assert!(d.abstracts().is_empty() && d.nums().is_empty());
    assert_eq!(
        d.issues(),
        [NumberingIssue::NotNumbering {
            root: "styles".into()
        }]
    );
}

#[test]
fn an_empty_numbering_part_reads_as_empty_without_issues() {
    // Proves the empty part, in both dialects.
    for w in [W_T, W_S] {
        let d = read_in(w, "");
        assert!(d.abstracts().is_empty() && d.nums().is_empty() && d.issues().is_empty());
        assert!(d.level(1, 0).is_none());
    }
}

/// A numbering part whose one level holds `inner`, with the `mc` and `w14` prefixes declared.
fn with_mc(inner: &str) -> NumberingDefs {
    NumberingDefs::read(&tree(&format!(
        "<w:numbering xmlns:w=\"{W_T}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" \
         xmlns:w14=\"http://schemas.microsoft.com/office/word/2010/wordml\" \
         xmlns:zzz=\"urn:not-understood\"><w:abstractNum w:abstractNumId=\"0\">\
         <w:lvl w:ilvl=\"0\">{inner}</w:lvl></w:abstractNum></w:numbering>"
    )))
}

fn alternate(requires: &str) -> String {
    format!(
        "<mc:AlternateContent><mc:Choice Requires=\"{requires}\">\
         <w:numFmt w:val=\"custom\" w:format=\"001, 002, 003, ...\"/></mc:Choice>\
         <mc:Fallback><w:numFmt w:val=\"decimal\"/></mc:Fallback></mc:AlternateContent>"
    )
}

#[test]
fn a_custom_format_in_the_taken_choice_is_read() {
    // Proves the `mc:Choice` branch Word 2010 writes is read, and `w:format` is kept.
    let d = with_mc(&alternate("w14"));
    assert_eq!(
        d.abstracts()[0].levels[0].fmt,
        NumFmt::Custom("001, 002, 003, ...".into())
    );
    assert!(d.issues().is_empty());
}

#[test]
fn the_fallback_is_read_when_the_choice_is_not_understood() {
    // Proves a `Requires` prefix that names an unknown namespace sends the read to the fallback.
    let d = with_mc(&alternate("zzz"));
    assert_eq!(d.abstracts()[0].levels[0].fmt, NumFmt::Decimal);
}

#[test]
fn a_direct_format_wins_over_the_alternate_content() {
    // Proves a direct `w:numFmt` wins, wherever it sits relative to the `mc:AlternateContent`.
    for inner in [
        format!("<w:numFmt w:val=\"upperRoman\"/>{}", alternate("w14")),
        format!("{}<w:numFmt w:val=\"upperRoman\"/>", alternate("w14")),
    ] {
        let d = with_mc(&inner);
        assert_eq!(d.abstracts()[0].levels[0].fmt, NumFmt::UpperRoman);
    }
}

#[test]
fn a_custom_format_without_a_pattern_is_an_empty_custom() {
    // Proves `custom` with no `w:format` is `Custom("")`, directly and in a choice.
    let direct = with_mc("<w:numFmt w:val=\"custom\"/>");
    assert_eq!(
        direct.abstracts()[0].levels[0].fmt,
        NumFmt::Custom("".into())
    );
    let choice = with_mc(
        "<mc:AlternateContent><mc:Choice Requires=\"w14\"><w:numFmt w:val=\"custom\"/></mc:Choice>\
         <mc:Fallback><w:numFmt w:val=\"decimal\"/></mc:Fallback></mc:AlternateContent>",
    );
    assert_eq!(
        choice.abstracts()[0].levels[0].fmt,
        NumFmt::Custom("".into())
    );
    assert_eq!(NumFmt::parse("custom"), NumFmt::Custom("".into()));
}

#[test]
fn an_alternate_content_without_a_format_leaves_the_default() {
    // Proves a taken branch with no `w:numFmt`, or no usable branch, leaves decimal.
    let none = with_mc(
        "<mc:AlternateContent><mc:Choice Requires=\"w14\"><w:suff w:val=\"space\"/></mc:Choice></mc:AlternateContent>",
    );
    assert_eq!(none.abstracts()[0].levels[0].fmt, NumFmt::Decimal);
    let empty = with_mc("<mc:AlternateContent/>");
    assert_eq!(empty.abstracts()[0].levels[0].fmt, NumFmt::Decimal);
}
