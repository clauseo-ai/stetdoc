//! What a check reports: findings with a stable code, and the report of `Document::check`.
//!
//! A **finding** is one thing wrong with a document, or one thing worth saying. A **violation** is a
//! departure from the schema or from a Word rule, which an edit may not add. A **note** is legal
//! and worth saying. Codes are stable once released, as error codes are: new ones may be added, and
//! existing ones are never repurposed or removed outside a major version.
//! `docs/design/check.md` says what each rule is, and what the corpus says about it.
//!
//! **Adding a code.** Add one entry to the `finding_codes!` list below (variant, wire name, class,
//! documentation), and raise it where it belongs.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

/// How serious a finding is.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Class {
    /// A departure from the schema or from a Word rule. An edit may not add one.
    Violation,
    /// Legal, and worth saying.
    Note,
}

macro_rules! finding_codes {
    ($(
        $variant:ident {
            wire: $wire:literal,
            class: $class:ident,
            doc: $doc:literal $(,)?
        }
    )*) => {
        /// A stable, documented identifier for a finding.
        ///
        /// The wire name (see [`FindingCode::as_str`]) is what appears in reports and JSON.
        /// New codes may be added, so match with a wildcard arm.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        #[non_exhaustive]
        pub enum FindingCode {
            $(
                #[doc = $doc]
                $variant,
            )*
        }

        impl FindingCode {
            /// Every code, in the order of the table.
            pub const ALL: &'static [FindingCode] = &[$(FindingCode::$variant),*];

            /// The snake_case name used on the wire.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(FindingCode::$variant => $wire,)*
                }
            }

            /// Whether a finding with this code is a violation or a note.
            pub const fn class(self) -> Class {
                match self {
                    $(FindingCode::$variant => Class::$class,)*
                }
            }

            /// What the code means, in a sentence.
            pub const fn description(self) -> &'static str {
                match self {
                    $(FindingCode::$variant => $doc,)*
                }
            }

            /// The code with this wire name, if there is one.
            pub fn from_name(name: &str) -> Option<FindingCode> {
                match name {
                    $($wire => Some(FindingCode::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

finding_codes! {
    SchemaWrongRoot {
        wire: "schema_wrong_root",
        class: Violation,
        doc: "The root element is not the one the part must have.",
    }
    SchemaUnexpectedChild {
        wire: "schema_unexpected_child",
        class: Violation,
        doc: "A known element that the content model does not allow in this parent.",
    }
    SchemaUnknownElement {
        wire: "schema_unknown_element",
        class: Violation,
        doc: "An element in a namespace the schema does not know, which the part does not list in mc:Ignorable.",
    }
    SchemaOutOfOrder {
        wire: "schema_out_of_order",
        class: Violation,
        doc: "An element that is allowed in its parent but not after what precedes it, or a second copy of one that may occur once.",
    }
    SchemaNonCanonicalOrder {
        wire: "schema_non_canonical_order",
        class: Violation,
        doc: "In a property container that the schema leaves unordered, a child is not in the order Word writes.",
    }
    SchemaMissingChild {
        wire: "schema_missing_child",
        class: Violation,
        doc: "The content of an element ends before a required child.",
    }
    SchemaMustUnderstand {
        wire: "schema_must_understand",
        class: Violation,
        doc: "mc:MustUnderstand names a namespace that Word does not understand.",
    }
    SchemaUnexpectedText {
        wire: "schema_unexpected_text",
        class: Violation,
        doc: "Character data in an element that has element-only content.",
    }
    SchemaUnknownAttribute {
        wire: "schema_unknown_attribute",
        class: Violation,
        doc: "An attribute the schema does not define on this element.",
    }
    SchemaMissingAttribute {
        wire: "schema_missing_attribute",
        class: Violation,
        doc: "A required attribute is absent.",
    }
    SchemaInvalidAttributeValue {
        wire: "schema_invalid_attribute_value",
        class: Violation,
        doc: "The value of an attribute does not satisfy its type.",
    }
    TextInDeletion {
        wire: "text_in_deletion",
        class: Violation,
        doc: "A w:t or w:instrText inside a w:del, where Word writes w:delText and w:delInstrText.",
    }
    DeltextOutsideDeletion {
        wire: "deltext_outside_deletion",
        class: Violation,
        doc: "A w:delText or w:delInstrText with no w:del around it in its paragraph.",
    }
    EdgeSpaceWithoutPreserve {
        wire: "edge_space_without_preserve",
        class: Violation,
        doc: "Text that starts or ends with white space in an element that has no xml:space=\"preserve\", so a reader may drop the space.",
    }
    RevisionIdDuplicate {
        wire: "revision_id_duplicate",
        class: Violation,
        doc: "Two tracked changes of the document have the same w:id.",
    }
    BookmarkIdDuplicate {
        wire: "bookmark_id_duplicate",
        class: Violation,
        doc: "Two bookmarks of one part have the same w:id.",
    }
    RevisionNestedInItself {
        wire: "revision_nested_in_itself",
        class: Violation,
        doc: "A w:ins inside a w:ins, or a w:del inside a w:del, in one paragraph.",
    }
    BookmarkUnpaired {
        wire: "bookmark_unpaired",
        class: Violation,
        doc: "A bookmark has a start and no end, or an end and no start.",
    }
    MoveRangeUnpaired {
        wire: "move_range_unpaired",
        class: Violation,
        doc: "A move range has a start and no end, or an end and no start.",
    }
    MoveNameUnpaired {
        wire: "move_name_unpaired",
        class: Violation,
        doc: "A move has a source and no destination, or a destination and no source.",
    }
    FieldSplitByDeletion {
        wire: "field_split_by_deletion",
        class: Violation,
        doc: "A field whose begin, separator and end are not all inside a deletion or all outside one.",
    }
    CellWithoutFinalParagraph {
        wire: "cell_without_final_paragraph",
        class: Violation,
        doc: "A table cell whose last block is not a paragraph.",
    }
    TableWithoutRow {
        wire: "table_without_row",
        class: Violation,
        doc: "A table with no row.",
    }
    RowWithoutCell {
        wire: "row_without_cell",
        class: Violation,
        doc: "A table row with no cell.",
    }
    ContainerWithoutBlock {
        wire: "container_without_block",
        class: Violation,
        doc: "A footnote, endnote or comment that holds no paragraph or table.",
    }
    EmptyRevision {
        wire: "empty_revision",
        class: Violation,
        doc: "A w:ins, w:del, w:moveFrom or w:moveTo that holds nothing.",
    }
    EmptyProperties {
        wire: "empty_properties",
        class: Violation,
        doc: "A w:rPr or w:pPr that holds nothing, outside the snapshot of a property change.",
    }
    PartWithoutContentType {
        wire: "part_without_content_type",
        class: Violation,
        doc: "A part that has neither an Override nor a Default for its extension in `[Content_Types].xml`.",
    }
    RelationshipTargetMissing {
        wire: "relationship_target_missing",
        class: Violation,
        doc: "An internal relationship whose target is not a part of the package.",
    }
    RelationshipIdDuplicate {
        wire: "relationship_id_duplicate",
        class: Violation,
        doc: "Two relationships of one part have the same id.",
    }
    CommentIdDuplicate {
        wire: "comment_id_duplicate",
        class: Violation,
        doc: "Two comments have the same w:id.",
    }
    MarkerUnpaired {
        wire: "marker_unpaired",
        class: Violation,
        doc: "A comment range has a start and no end, or an end and no start.",
    }
    MarkerWithoutComment {
        wire: "marker_without_comment",
        class: Violation,
        doc: "A comment range marker or reference whose id is no comment.",
    }
    ReferenceMissing {
        wire: "reference_missing",
        class: Violation,
        doc: "A comment with a range and no reference run.",
    }
    ReferenceDuplicate {
        wire: "reference_duplicate",
        class: Violation,
        doc: "Two reference runs for one comment.",
    }
    Unanchored {
        wire: "unanchored",
        class: Note,
        doc: "A comment with no marker and no reference.",
    }
    RowWithoutComment {
        wire: "row_without_comment",
        class: Violation,
        doc: "A row of a comment extension part whose paragraph id names no comment.",
    }
    RowMissing {
        wire: "row_missing",
        class: Note,
        doc: "A comment with a paragraph id and no row in an extension part that exists.",
    }
    ParaIdMissing {
        wire: "para_id_missing",
        class: Note,
        doc: "A comment whose last paragraph has no w14:paraId, so it cannot be threaded.",
    }
    ParaIdDuplicate {
        wire: "para_id_duplicate",
        class: Violation,
        doc: "Two paragraphs, at least one of them a comment paragraph, have the same w14:paraId.",
    }
    ParaIdRange {
        wire: "para_id_range",
        class: Note,
        doc: "A w14:paraId at or above 0x80000000, which the schema forbids.",
    }
    ParentMissing {
        wire: "parent_missing",
        class: Note,
        doc: "A comment whose parent paragraph id names no comment.",
    }
    ThreadCycle {
        wire: "thread_cycle",
        class: Violation,
        doc: "The parents of some comments form a loop.",
    }
    DurableMismatch {
        wire: "durable_mismatch",
        class: Violation,
        doc: "Two comment parts disagree about a durable id, or a durable id is used twice.",
    }
    PartUnrelated {
        wire: "part_unrelated",
        class: Violation,
        doc: "A comment extension part that the main part has no relationship to.",
    }
    PartUnreadable {
        wire: "part_unreadable",
        class: Note,
        doc: "A part that could not be read (encrypted, or stored with an unsupported method), so it was not checked.",
    }
}

/// One thing a check found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    /// The stable code.
    pub code: FindingCode,
    /// Whether it is a violation or a note.
    pub class: Class,
    /// The part it is in, as the package names it. Empty for a finding about the document as a
    /// whole, such as a revision id that two parts use.
    pub part: String,
    /// The ref of the block it is in, when the block has one. A new block, a paragraph in a
    /// text box and a problem at the level of the part have none.
    pub block: Option<String>,
    /// A sentence that says what is wrong.
    pub message: String,
}

/// What `Document::check` reports.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CheckReport {
    /// True when there is no violation. Notes do not count.
    pub ok: bool,
    /// How many violations there are, all of them.
    pub violations: usize,
    /// How many notes there are, all of them.
    pub notes: usize,
    /// The findings in the order they were found, violations first and then notes, at most
    /// [`CheckReport::MAX_FINDINGS`] of them.
    pub findings: Vec<Finding>,
    /// Every code that occurred and how many times, in the order of [`FindingCode::ALL`]. It
    /// counts all the findings, and not only the ones in `findings`.
    pub by_code: Vec<(FindingCode, usize)>,
    /// How many parts were examined.
    pub parts: usize,
    /// How many parts were not: the ones the main part names that have no schema (an image, a
    /// theme, a chart), and the ones that could not be read. Neither is listed, and the second is
    /// a note.
    pub unchecked: usize,
}

impl CheckReport {
    /// How many findings a report lists. Counts are kept for all of them.
    pub const MAX_FINDINGS: usize = 200;
}

// A code is its wire name in JSON, so these are written by hand and not derived.
impl Serialize for FindingCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for FindingCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<FindingCode, D::Error> {
        let name = String::deserialize(deserializer)?;
        FindingCode::from_name(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("`{name}` is not a finding code")))
    }
}

impl JsonSchema for FindingCode {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("FindingCode")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        let names: Vec<&str> = FindingCode::ALL.iter().map(|c| c.as_str()).collect();
        json_schema!({ "type": "string", "enum": names })
    }
}
