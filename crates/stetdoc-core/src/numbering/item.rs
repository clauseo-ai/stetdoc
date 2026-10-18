//! What `numbering` says about one paragraph: a list item, and the notes about approximations.

use crate::wordml::Suff;

/// A list, named by the `w:abstractNum` that is the key of its counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ListId(pub i64);

/// What the paragraph's number is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// A number or letter, such as `1.2` or `(a)`: the level's text with the counters filled in.
    Counted {
        /// The label, trimmed, without the suffix.
        text: String,
    },
    /// A bullet. The glyph is the level's text as written, often a private-use code of a symbol font.
    Bullet {
        /// The level's text.
        glyph: String,
    },
    /// The level shows nothing: format `none`, or an empty label.
    Plain,
}

/// A paragraph that Word numbers or bullets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// The level, 0 to 8.
    pub level: u8,
    /// The `w:numId` the paragraph uses.
    pub num_id: i64,
    /// The list whose counters it advances.
    pub list: ListId,
    /// The number, the bullet, or nothing.
    pub kind: ItemKind,
    /// What follows the label: a tab, a space or nothing.
    pub suffix: Suff,
    /// Redline only: the paragraph is a deletion, and the label is where it would fall.
    pub ghost: bool,
    /// The numbering came from the paragraph style, not from a direct `w:numPr`.
    pub from_style: bool,
    /// The level's own `rPr` has `vanish`, so Word hides the label.
    pub label_hidden: bool,
    /// The label with the labels of its ancestors, as Word's `REF \w` shows it, trailing periods
    /// stripped. Empty unless the item is `Counted`.
    pub full: String,
}

/// Something `numbering` approximated or could not follow. Projection and the receipt show these as
/// warnings, so that an agent is told when a number may not match Word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Note {
    /// A number format that is not built: decimal was shown.
    UnsupportedFormat {
        /// The format as written.
        fmt: Box<str>,
    },
    /// A value outside the range a format can show (a Roman numeral above 3,999): decimal was shown.
    OutOfRange {
        /// The format.
        fmt: Box<str>,
    },
    /// A paragraph uses a level the list does not define. The nearest lower level was used.
    MissingLevel {
        /// The `w:numId`.
        num_id: i64,
        /// The level asked for.
        ilvl: u8,
    },
    /// A `numStyleLink` chain returned to a style it had already visited, or was too long.
    StyleLinkCycle {
        /// The style it was following.
        style: Box<str>,
    },
    /// The story mixes `LISTNUM` fields with numbered paragraphs, and Word counts them in one list.
    ListNumField,
    /// A paragraph names a `w:numId` that has no usable `w:num` or `w:abstractNum`.
    UnresolvedNumId {
        /// The `w:numId`.
        num_id: i64,
    },
}
