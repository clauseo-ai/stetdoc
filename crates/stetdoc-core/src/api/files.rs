//! Readable files, the plan that an edit of them becomes, and the blocks a receipt shows.
//!
//! The types follow `docs/design/projection-and-index.md` (sections 2 and 8). They are plain
//! data: a ref crosses as its text, an offset is a count of Unicode scalar values in the
//! paragraph's `Final` text, and no type from inside the core is part of one. They derive
//! `serde` and `schemars`. `Planned`, `PlannedOp`, `BlockSpec` and `BlockRender` are not public:
//! they pass between `document` and `projection`.

use super::view::{FormatId, Warning};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// The files of a working copy as written by a format, and the base token they were written at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadableFiles {
    /// The base token of the state the files were rendered from.
    pub base: String,
    /// The format that wrote them, which must read them back.
    pub format: FormatId,
    /// `(name, text)` for each file, in the format's order.
    pub files: Vec<(String, String)>,
    /// `(name, SHA-256 in hex)` of each file as it was written by the release that wrote it. Empty for files that
    /// `render_files` just made. `WorkingCopy::pending_files` fills it from the manifest, so that an edit can tell that
    /// another release wrote the files (release skew).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rendered: Vec<(String, String)>,
}

/// Whether new paragraphs go before or after their anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Place {
    /// Before the anchor block.
    Before,
    /// After the anchor block.
    After,
}

/// A paragraph an `insert` adds.
///
/// **Each paragraph is modelled on the one before it in the list**, as repeated Enters would do it: the
/// second takes the style that follows the first's (`w:next`), and so on. The first is modelled on the
/// anchor when the insertion follows a paragraph. An insertion that has no paragraph before it (after a
/// table, at the start of a section or of a story) models its first paragraph on the paragraph it
/// precedes, with the copy rule. Design 6.4 and law T8.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewParagraph {
    /// The text, as typed: it may hold a tab or a line break and nothing else that is not text.
    pub text: String,
    /// The `w:styleId` the paragraph takes. `None` means the copy rule: the paragraph is a copy of the
    /// one it is modelled on (the previous paragraph of the list, or the anchor for the first). `Some`
    /// means that style and no direct paragraph property.
    pub style: Option<String>,
}

/// A paragraph with no style is written as its bare text, and one with a style as an object.
impl Serialize for NewParagraph {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match &self.style {
            None => serializer.serialize_str(&self.text),
            Some(style) => {
                let mut s = serializer.serialize_struct("NewParagraph", 2)?;
                s.serialize_field("text", &self.text)?;
                s.serialize_field("style", style)?;
                s.end()
            }
        }
    }
}

/// What a `NewParagraph` is read from: an object, or a bare string for a paragraph with no style.
#[derive(Deserialize)]
#[serde(untagged)]
enum NewParagraphRepr {
    Text(String),
    Full {
        text: String,
        #[serde(default)]
        style: Option<String>,
    },
}

impl<'de> Deserialize<'de> for NewParagraph {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<NewParagraph, D::Error> {
        Ok(match NewParagraphRepr::deserialize(deserializer)? {
            NewParagraphRepr::Text(text) => NewParagraph { text, style: None },
            NewParagraphRepr::Full { text, style } => NewParagraph { text, style },
        })
    }
}

impl JsonSchema for NewParagraph {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("NewParagraph")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "anyOf": [
                { "type": "string", "description": "The text of a paragraph with no style of its own." },
                {
                    "type": "object",
                    "properties": {
                        "text": { "type": "string" },
                        "style": { "type": ["string", "null"] }
                    },
                    "required": ["text"]
                }
            ]
        })
    }
}

/// One operation that an edit of readable files comes to. The `Op` allows each of these
/// shapes (design section 11) and maps them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlannedOp {
    /// Replace a range of a paragraph's `Final` text.
    Replace {
        /// The paragraph.
        block: String,
        /// The first scalar value replaced.
        start: u32,
        /// One past the last scalar value replaced.
        end: u32,
        /// The new text, as typed.
        with: String,
    },
    /// Insert paragraphs next to a block, in order.
    Insert {
        /// The block they go next to: a paragraph or a table.
        anchor: String,
        /// Before or after it.
        place: Place,
        /// The paragraphs, in order.
        paragraphs: Vec<NewParagraph>,
    },
    /// Delete a paragraph or a table whole.
    Delete {
        /// The block.
        block: String,
    },
    /// Delete a table row whole.
    RowDelete {
        /// The row.
        row: String,
    },
    /// Cut a paragraph in two at an offset in its `Final` text.
    Split {
        /// The paragraph.
        block: String,
        /// The offset of the cut.
        at: u32,
    },
    /// Join a paragraph and the one after it. Many paragraphs are pairs, in order.
    Merge {
        /// The paragraph that stays.
        first: String,
        /// The paragraph that is joined into it.
        second: String,
    },
}

/// Where an operation came from, so that an error that names an operation can name a file and a line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Origin {
    /// The file.
    pub file: String,
    /// The line in the edited file, from 1. Zero when the operation comes from a removed line.
    pub line: u32,
    /// The ref of the block the operation is about, when it has one.
    pub block: Option<String>,
    /// The second ref of a merge.
    pub also: Option<String>,
}

/// What `to_batch` makes of edited files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Planned {
    /// The operations, in the order they must be applied.
    pub ops: Vec<PlannedOp>,
    /// One origin for each operation.
    pub origins: Vec<Origin>,
    /// Warnings that are not errors: a typed number that repeats a computed one, a split at a wrap.
    pub notes: Vec<Warning>,
    /// The SHA-256 of each file the edit was compared with, as rendered, by name. `document` checks them against
    /// the hashes of the files as written (release skew) without rendering twice. It is the hash and not the text,
    /// because the text is a second copy of the whole story that nobody needs once the files are compared.
    pub baseline: Vec<(String, [u8; 32])>,
}

/// How a receipt shows the blocks it touched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockSpec {
    /// The format that writes the blocks.
    pub format: FormatId,
    /// How many neighbours to show around a new block.
    pub context: u8,
}

/// One block as a receipt shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenderedBlock {
    /// The block's ref, or `None` for a block that a dry run created and no commit has numbered.
    pub r#ref: Option<String>,
    /// The story it is in.
    pub story: String,
    /// Its lines, as the readable file will hold them.
    pub text: String,
    /// For a block with no ref, the ref of the block it follows.
    pub anchor: Option<String>,
}

/// The blocks a receipt shows and the warnings about them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockRender {
    /// The blocks, in document order.
    pub blocks: Vec<RenderedBlock>,
    /// The warnings about them.
    pub warnings: Vec<Warning>,
}
