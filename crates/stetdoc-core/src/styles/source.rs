//! Where a value of effective formatting came from.

use crate::wordml::TableRegion;

/// The level of the style hierarchy that supplied a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The document defaults (`w:docDefaults`).
    Default,
    /// A table style, and the conditional format of it that applied, if one did.
    TableStyle {
        /// The style's id.
        id: Box<str>,
        /// The region, or `None` for the style's own properties.
        region: Option<TableRegion>,
    },
    /// The numbering level's paragraph properties.
    Numbering {
        /// The `w:numId`.
        num_id: i64,
        /// The level.
        ilvl: u8,
    },
    /// A paragraph style, which may be a parent on the `basedOn` chain.
    ParagraphStyle {
        /// The style's id.
        id: Box<str>,
    },
    /// A character style, which may be a parent on the `basedOn` chain.
    CharacterStyle {
        /// The style's id.
        id: Box<str>,
    },
    /// Direct formatting on the paragraph or run.
    Direct,
    /// A toggle property: the levels that set it and what each said. The value is their exclusive-or.
    Toggle(Vec<(Source, bool)>),
}
