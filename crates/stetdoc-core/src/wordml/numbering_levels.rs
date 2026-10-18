//! One numbering level (`w:lvl`) and the small enumerations it uses.
//!
//! Split from `numbering_defs.rs` to keep both files short. The level is read the same way inside an
//! `w:abstractNum` and inside a `w:lvlOverride`.

use super::mc::{is_alternate_content, taken_branch};
use super::names::{int, is_w, is_word_ns, on_off, w_attr};
use super::numbering_defs::{LevelOwner, NumberingIssue};
use super::props::Props;
use crate::xml::NodeRef;

/// The number format of a level (`w:numFmt`). The renderers are `numbering`'s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NumFmt {
    /// `decimal`: 1, 2, 3.
    Decimal,
    /// `upperRoman`: I, II, III.
    UpperRoman,
    /// `lowerRoman`: i, ii, iii.
    LowerRoman,
    /// `upperLetter`: A, B, C.
    UpperLetter,
    /// `lowerLetter`: a, b, c.
    LowerLetter,
    /// `ordinal`: 1st, 2nd, 3rd.
    Ordinal,
    /// `cardinalText`: One, Two, Three.
    CardinalText,
    /// `ordinalText`: First, Second, Third.
    OrdinalText,
    /// `decimalZero`: 01, 02, 03.
    DecimalZero,
    /// `bullet`: the level's text is the bullet.
    Bullet,
    /// `none`: no number.
    None,
    /// `custom`: a format Word 2010 writes as a pattern such as `001, 002, 003, ...`, held here as
    /// the `w:format` text. It is empty when the element has no `w:format`.
    Custom(Box<str>),
    /// Any other value, as written. The corpus has about 80 distinct values.
    Other(Box<str>),
}

impl NumFmt {
    /// Reads a `w:numFmt` element: its `w:val`, and for `custom` its `w:format`. The names are
    /// case sensitive, as in the schema. A missing `w:val` is decimal.
    fn from_element(n: &NodeRef<'_>) -> NumFmt {
        match NumFmt::parse(w_attr(n, "val").as_deref().unwrap_or("decimal")) {
            NumFmt::Custom(_) => {
                NumFmt::Custom(w_attr(n, "format").as_deref().unwrap_or("").into())
            }
            other => other,
        }
    }

    /// Reads a `w:numFmt` value, without its `w:format`, so `custom` reads as an empty `Custom`.
    pub fn parse(value: &str) -> NumFmt {
        match value.trim() {
            "decimal" => NumFmt::Decimal,
            "upperRoman" => NumFmt::UpperRoman,
            "lowerRoman" => NumFmt::LowerRoman,
            "upperLetter" => NumFmt::UpperLetter,
            "lowerLetter" => NumFmt::LowerLetter,
            "ordinal" => NumFmt::Ordinal,
            "cardinalText" => NumFmt::CardinalText,
            "ordinalText" => NumFmt::OrdinalText,
            "decimalZero" => NumFmt::DecimalZero,
            "bullet" => NumFmt::Bullet,
            "none" => NumFmt::None,
            "custom" => NumFmt::Custom("".into()),
            other => NumFmt::Other(other.into()),
        }
    }
}

/// What follows the number (`w:suff`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Suff {
    /// A tab. The default.
    #[default]
    Tab,
    /// A space.
    Space,
    /// Nothing.
    Nothing,
}

/// The kind of list an abstract definition describes (`w:multiLevelType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiLevel {
    /// `singleLevel`.
    Single,
    /// `hybridMultilevel`.
    Hybrid,
    /// `multilevel`.
    Multi,
    /// Missing, or a value that is none of the three.
    Unknown,
}

/// One level of a list (`w:lvl`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level {
    /// The level, 0 to 8.
    pub ilvl: u8,
    /// `w:start`. **Zero when the element is omitted** (ISO 29500 Part 1, 17.9.26), not one.
    pub start: i64,
    /// `w:numFmt`. Decimal when the element is omitted. A direct `w:numFmt` wins over one inside
    /// the branch Word reads of an `mc:AlternateContent`.
    pub fmt: NumFmt,
    /// `w:lvlText`, such as `%1.%2(%3)`. Empty when omitted.
    pub text: Box<str>,
    /// `w:lvlRestart`. `None` means the default (restart after any higher level), and `Some(0)`
    /// means never restart.
    pub restart: Option<u8>,
    /// `w:isLgl`: show every level as a decimal number.
    pub legal: bool,
    /// `w:suff`.
    pub suff: Suff,
    /// `w:pStyle`: the paragraph style linked to this level.
    pub style: Option<Box<str>>,
    /// `w:lvlJc`: how the number is aligned, as written.
    pub justification: Option<Box<str>>,
    /// The level's `w:pPr`.
    pub paragraph: Props,
    /// The level's `w:rPr`.
    pub run: Props,
    /// `w:lvlPicBulletId`: the picture bullet.
    pub pic_bullet: Option<i64>,
}

/// Reads the `w:ilvl` attribute of `n`: 0 to 8, or an issue.
pub(super) fn read_ilvl(
    n: &NodeRef<'_>,
    owner: LevelOwner,
    issues: &mut Vec<NumberingIssue>,
) -> Option<u8> {
    let raw = w_attr(n, "ilvl");
    let ilvl = raw
        .as_deref()
        .and_then(int)
        .and_then(|v| u8::try_from(v).ok())
        .filter(|v| *v <= 8);
    if ilvl.is_none() {
        issues.push(NumberingIssue::InvalidLevel {
            owner,
            value: raw.as_deref().unwrap_or("").into(),
        });
    }
    ilvl
}

/// Tracks which child elements of a level were seen, so the first of each wins.
#[derive(Default)]
struct Seen(u16);

impl Seen {
    /// True the first time `bit` is asked for.
    fn first(&mut self, bit: u16) -> bool {
        let fresh = self.0 & bit == 0;
        self.0 |= bit;
        fresh
    }
}

/// Reads one `w:lvl` as level `ilvl`. A value that cannot be read is reported and left at its default.
pub(super) fn read_level(
    n: &NodeRef<'_>,
    ilvl: u8,
    owner: LevelOwner,
    issues: &mut Vec<NumberingIssue>,
) -> Level {
    let mut l = Level {
        ilvl,
        start: 0,
        fmt: NumFmt::Decimal,
        text: "".into(),
        restart: None,
        legal: false,
        suff: Suff::Tab,
        style: None,
        justification: None,
        paragraph: Props::new(),
        run: Props::new(),
        pic_bullet: None,
    };
    let mut seen = Seen::default();
    let mut direct_fmt = None;
    let mut branch_fmt = None;
    for c in n.child_elements() {
        if is_alternate_content(&c) {
            // Word 2010 writes a custom format as a choice with a decimal fallback.
            if branch_fmt.is_none()
                && let Some(b) = taken_branch(&c)
                && let Some(f) = b.child_elements().find(|f| is_w(f, "numFmt"))
            {
                branch_fmt = Some(NumFmt::from_element(&f));
            }
            continue;
        }
        if !is_word_ns(c.namespace()) {
            continue;
        }
        let val = w_attr(&c, "val");
        let mut bad = |element: &'static str, v: &str| {
            issues.push(NumberingIssue::InvalidValue {
                owner,
                ilvl,
                element,
                value: v.into(),
            });
        };
        match c.local_name() {
            "start" if seen.first(1) => match val.as_deref().and_then(int) {
                Some(v) => l.start = v,
                None => bad("w:start", val.as_deref().unwrap_or("")),
            },
            "numFmt" if seen.first(2) => direct_fmt = Some(NumFmt::from_element(&c)),
            "lvlText" if seen.first(4) => {
                l.text = val.as_deref().unwrap_or("").into();
            }
            "lvlRestart" if seen.first(8) => {
                match val
                    .as_deref()
                    .and_then(int)
                    .and_then(|v| u8::try_from(v).ok())
                    .filter(|v| *v <= 9)
                {
                    Some(v) => l.restart = Some(v),
                    None => bad("w:lvlRestart", val.as_deref().unwrap_or("")),
                }
            }
            "isLgl" if seen.first(16) => l.legal = on_off(val.as_deref()),
            "suff" if seen.first(32) => {
                l.suff = match val.as_deref().map(str::trim) {
                    Some("space") => Suff::Space,
                    Some("nothing") => Suff::Nothing,
                    _ => Suff::Tab,
                };
            }
            "pStyle" if seen.first(64) => l.style = val.as_deref().map(Into::into),
            "lvlJc" if seen.first(128) => l.justification = val.as_deref().map(Into::into),
            "pPr" if seen.first(256) => l.paragraph = Props::read(c),
            "rPr" if seen.first(512) => l.run = Props::read(c),
            "lvlPicBulletId" if seen.first(1024) => match val.as_deref().and_then(int) {
                Some(v) => l.pic_bullet = Some(v),
                None => bad("w:lvlPicBulletId", val.as_deref().unwrap_or("")),
            },
            _ => {}
        }
    }
    l.fmt = direct_fmt.or(branch_fmt).unwrap_or(NumFmt::Decimal);
    l
}
