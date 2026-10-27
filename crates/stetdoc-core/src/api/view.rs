//! What a view shows and what it returns.
//!
//! `Mode` is the glossary's: how tracked changes appear in a view (because `wordml` reads text
//! in a mode). The rest is the data of `projection::view` and of the warnings that views, files
//! and receipts carry (from `docs/design/projection-and-index.md` section 2). Every type holds
//! strings, numbers, booleans, vectors and enums of this module, and nothing from inside the
//! core: a ref crosses as its text (`p42`) and a base token as its `namespace:version:lineage`
//! string. Like the other public types they derive `serde` and `schemars`.

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

/// How tracked changes appear in a view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// All revisions applied: insertions are in, deletions are out.
    Final,
    /// All revisions rejected: the document as it was before any tracked change.
    Original,
    /// Both, marked: inserted and deleted text together.
    Redline,
}

impl Mode {
    /// Every mode, in the order of the glossary.
    pub const ALL: [Mode; 3] = [Mode::Final, Mode::Original, Mode::Redline];
}

/// The id of a projection format, such as `stet/1`. A format with options is several ids, so one
/// string says which adapter wrote a file and which one must read it back.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct FormatId(String);

impl FormatId {
    /// The default format.
    pub const STET: &'static str = "stet/1";
    /// The `json` format: the neutral model serialized, for programs.
    pub const JSON: &'static str = "json/1";

    /// The format with this id. Whether the id is registered is the registry's question.
    pub fn new(id: impl Into<String>) -> FormatId {
        FormatId(id.into())
    }

    /// The default format, `stet/1`.
    pub fn stet() -> FormatId {
        FormatId::new(FormatId::STET)
    }

    /// The id as text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for FormatId {
    fn default() -> FormatId {
        FormatId::stet()
    }
}

/// A budget in tokens, by the estimate of design 4.4. Zero means no limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Budget(pub u32);

impl Budget {
    /// The default size of a page: 3,000 tokens, about 12 KB of text, so that a page stays well under what an agent's
    /// tool result holds before it is cut or spilled to a file, even with two or three pages in one call.
    pub const DEFAULT: u32 = 3_000;
}

impl Default for Budget {
    fn default() -> Budget {
        Budget(Budget::DEFAULT)
    }
}

/// Where a page continues: `p188` (after that block), `change:14` or `comment:7`. Opaque to callers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Cursor(String);

impl Cursor {
    /// A cursor from its text.
    pub fn new(text: impl Into<String>) -> Cursor {
        Cursor(text.into())
    }

    /// The text of the cursor.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One of the document's stories, by the name an agent uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StoryName {
    /// The body.
    Body,
    /// Footnotes and endnotes.
    Notes,
    /// Headers and footers.
    Headers,
    /// Comments.
    Comments,
}

/// Which tracked changes a `Revisions` view lists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RevisionFilter {
    /// Only changes by this author, by code or name.
    pub author: Option<String>,
    /// Only changes of this kind: `insert`, `delete`, `move` or `format`.
    pub kind: Option<String>,
    /// Only changes whose inserted or deleted text contains one of these texts. Each is case-insensitive unless it has
    /// an uppercase letter, as in a `Find`. Empty keeps every change. JSON takes one string or a list of them.
    #[serde(deserialize_with = "terms_or_none")]
    #[schemars(schema_with = "terms_schema")]
    pub find: Vec<String>,
    /// Carry the whole inserted and deleted text of each change, up to 1,000 characters, and not 200; a view shows it
    /// all and does not cut each line to 80 characters.
    pub full: bool,
}

/// Which comment threads a `Comments` view lists.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct CommentFilter {
    /// Only threads started by this author, by code or name.
    pub author: Option<String>,
    /// Only resolved threads (`true`) or only open ones (`false`).
    pub resolved: Option<bool>,
}

/// What a view selects.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// The body whole if it fits the budget, otherwise its outline.
    Auto,
    /// The headings of the body, to a depth.
    Outline {
        /// The deepest level shown, or the deepest that fits.
        depth: Option<u8>,
    },
    /// The blocks from a ref to a ref inclusive, or to the end of the story.
    Range {
        /// The first ref.
        from: String,
        /// The last ref.
        to: Option<String>,
    },
    /// The block a clause number resolves to, with the blocks under it.
    Clause(String),
    /// The blocks whose text contains any of the needles. With a `context` of 0, one line for each paragraph that
    /// matches: its ref, its clause and the text around the match. With more, the blocks whole, with their neighbours.
    Find {
        /// The texts to find, each case-insensitive unless it has an uppercase letter. A block that holds any of them
        /// matches. JSON takes one string or a list of them, as `needles` or `needle`.
        #[serde(alias = "needle", deserialize_with = "terms")]
        #[schemars(schema_with = "terms_schema")]
        needles: Vec<String>,
        /// 0 for one line for each matching paragraph; otherwise the matching blocks whole, with this many blocks on
        /// each side.
        context: u8,
        /// Only the blocks from this ref on, as a `Range` starts.
        #[serde(default)]
        from: Option<String>,
        /// With `from`, only the blocks up to this ref; without it, to the end of the story of `from`.
        #[serde(default)]
        to: Option<String>,
    },
    /// The tracked changes, as a ledger.
    Revisions(RevisionFilter),
    /// The comment threads.
    Comments(CommentFilter),
    /// One story whole.
    Story(StoryName),
}

/// What a caller asks `view` for.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct ViewSpec {
    /// What to select.
    pub scope: Scope,
    /// How tracked changes appear.
    pub mode: Mode,
    /// The size of a page.
    pub budget: Budget,
    /// Where to continue, from a previous page.
    pub after: Option<Cursor>,
    /// The format that writes the page.
    pub format: FormatId,
    /// Margin notes and the appendix of the notes and comments a page cites.
    pub notes: bool,
}

impl Default for ViewSpec {
    fn default() -> ViewSpec {
        ViewSpec {
            scope: Scope::Auto,
            mode: Mode::Redline,
            budget: Budget::default(),
            after: None,
            format: FormatId::stet(),
            notes: true,
        }
    }
}

/// Where a page is in what it selects.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    /// What was selected, such as `p1..p8152` or `outline`.
    pub scope: String,
    /// The first block shown.
    pub first: Option<String>,
    /// The last block shown.
    pub last: Option<String>,
    /// How many blocks the page holds.
    pub shown: u32,
    /// How many blocks the scope holds.
    pub total: u32,
    /// An estimate of the tokens that remain after this page.
    pub remaining_tokens: u32,
}

/// The kind of a warning. A code is stable once released, as an error code is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WarningCode {
    /// Text a reader cannot see (white or tiny) that Word shows.
    LooksHidden,
    /// Text Word hides (`vanish`).
    HiddenText,
    /// A clause number that may not match Word's.
    NumberingApproximate,
    /// Content exists that the view cannot show.
    ReadAnomaly,
    /// A block that is not shown and has no ref.
    OpaqueContent,
    /// A note that no reference points at.
    UnreferencedNote,
    /// A typed number that Word also adds.
    DuplicateNumber,
    /// A deletion and an insertion of the same text, not next to each other.
    LooksLikeRewrite,
    /// A split that fell where the renderer had wrapped a long paragraph.
    SplitAtWrap,
    /// An operation changed nothing.
    NoChange,
    /// An operation removed the author's own pending text, mark or row outright.
    OwnTextRemoved,
    /// An operation edited the author's own pending property change in place.
    PropertyChangeFolded,
    /// A revision of another author went with text that an operation removed.
    RevisionRemovedWithText,
    /// Straight quotes in typed text were curled to match the paragraph.
    QuotesCurled,
    /// Curly quotes in typed text were straightened to match the paragraph.
    QuotesStraightened,
    /// A `find` matched only after curly and straight quotes were treated as equal.
    FindMatchedLoosely,
    /// A thread was already resolved.
    AlreadyResolved,
    /// A reply was added to a resolved thread, which stays resolved.
    ReplyToResolved,
    /// A comment's range is empty after a tracked deletion, and the comment stays.
    AnchorEmptied,
    /// A comment was removed because the text it was attached to was removed.
    CommentRemoved,
    /// A side of a comparison had revisions, which were accepted before it was compared.
    CompareRevisionsAccepted,
    /// A paragraph that could not be edited in place was shown as a deleted copy of the original and the revised paragraph inserted.
    CompareParagraphReplaced,
    /// A block that could not be written as changes is shown as the revised document has it.
    CompareNotRedlined,
    /// Rejecting every change does not give back a block of the original exactly: it holds something that no tracked change can
    /// carry, which stays.
    CompareNotRestored,
    /// A block that compare copied from the original holds paragraphs and tables that did not pair one to one with the
    /// original block's, so each of them may keep any problem that any of the original's had.
    CompareKeysMerged,
    /// Rows whose cells changed shape were written as a deleted row and an inserted row.
    CompareTableReplaced,
    /// A header or footer that exists on one side only was not compared.
    CompareStoryUnmatched,
    /// Comments in deleted content were not carried into the redline.
    CompareCommentsDropped,
    /// A bookmark of deleted content has a name the revised document uses, and was not carried.
    CompareBookmarksDropped,
    /// A hyperlink in deleted content kept its text and lost its address.
    CompareHyperlinksUnlinked,
    /// A picture or object in deleted content could not be carried.
    CompareObjectsDropped,
    /// A note of deleted content could not be carried.
    CompareNotesDropped,
    /// A list of deleted content had no numbering part to go in.
    CompareListsDropped,
    /// A view of the whole body showed its outline, because the body is more than one page.
    OutlineShown,
}

impl WarningCode {
    /// Every code, in the order of the design's table.
    pub const ALL: [WarningCode; 34] = [
        WarningCode::LooksHidden,
        WarningCode::HiddenText,
        WarningCode::NumberingApproximate,
        WarningCode::ReadAnomaly,
        WarningCode::OpaqueContent,
        WarningCode::UnreferencedNote,
        WarningCode::DuplicateNumber,
        WarningCode::LooksLikeRewrite,
        WarningCode::SplitAtWrap,
        WarningCode::NoChange,
        WarningCode::OwnTextRemoved,
        WarningCode::PropertyChangeFolded,
        WarningCode::RevisionRemovedWithText,
        WarningCode::QuotesCurled,
        WarningCode::QuotesStraightened,
        WarningCode::FindMatchedLoosely,
        WarningCode::AlreadyResolved,
        WarningCode::ReplyToResolved,
        WarningCode::AnchorEmptied,
        WarningCode::CommentRemoved,
        WarningCode::CompareRevisionsAccepted,
        WarningCode::CompareParagraphReplaced,
        WarningCode::CompareNotRedlined,
        WarningCode::CompareNotRestored,
        WarningCode::CompareKeysMerged,
        WarningCode::CompareTableReplaced,
        WarningCode::CompareStoryUnmatched,
        WarningCode::CompareCommentsDropped,
        WarningCode::CompareBookmarksDropped,
        WarningCode::CompareHyperlinksUnlinked,
        WarningCode::CompareObjectsDropped,
        WarningCode::CompareNotesDropped,
        WarningCode::CompareListsDropped,
        WarningCode::OutlineShown,
    ];

    /// The snake_case name used on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            WarningCode::LooksHidden => "looks_hidden",
            WarningCode::HiddenText => "hidden_text",
            WarningCode::NumberingApproximate => "numbering_approximate",
            WarningCode::ReadAnomaly => "read_anomaly",
            WarningCode::OpaqueContent => "opaque_content",
            WarningCode::UnreferencedNote => "unreferenced_note",
            WarningCode::DuplicateNumber => "duplicate_number",
            WarningCode::LooksLikeRewrite => "looks_like_rewrite",
            WarningCode::SplitAtWrap => "split_at_wrap",
            WarningCode::NoChange => "no_change",
            WarningCode::OwnTextRemoved => "own_text_removed",
            WarningCode::PropertyChangeFolded => "property_change_folded",
            WarningCode::RevisionRemovedWithText => "revision_removed_with_text",
            WarningCode::QuotesCurled => "quotes_curled",
            WarningCode::QuotesStraightened => "quotes_straightened",
            WarningCode::FindMatchedLoosely => "find_matched_loosely",
            WarningCode::AlreadyResolved => "already_resolved",
            WarningCode::ReplyToResolved => "reply_to_resolved",
            WarningCode::AnchorEmptied => "anchor_emptied",
            WarningCode::CommentRemoved => "comment_removed",
            WarningCode::CompareRevisionsAccepted => "compare_revisions_accepted",
            WarningCode::CompareParagraphReplaced => "compare_paragraph_replaced",
            WarningCode::CompareNotRedlined => "compare_not_redlined",
            WarningCode::CompareNotRestored => "compare_not_restored",
            WarningCode::CompareKeysMerged => "compare_keys_merged",
            WarningCode::CompareTableReplaced => "compare_table_replaced",
            WarningCode::CompareStoryUnmatched => "compare_story_unmatched",
            WarningCode::CompareCommentsDropped => "compare_comments_dropped",
            WarningCode::CompareBookmarksDropped => "compare_bookmarks_dropped",
            WarningCode::CompareHyperlinksUnlinked => "compare_hyperlinks_unlinked",
            WarningCode::CompareObjectsDropped => "compare_objects_dropped",
            WarningCode::CompareNotesDropped => "compare_notes_dropped",
            WarningCode::CompareListsDropped => "compare_lists_dropped",
            WarningCode::OutlineShown => "outline_shown",
        }
    }

    /// The code a wire name stands for.
    pub fn from_name(name: &str) -> Option<WarningCode> {
        WarningCode::ALL
            .into_iter()
            .find(|code| code.as_str() == name)
    }
}

/// Something the reader must know that is not in the text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    /// What kind of warning it is.
    pub code: WarningCode,
    /// The ref of the block it is about, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block: Option<String>,
    /// The story it is in: `body`, `notes`, `headers` or `comments`. A warning about an operation and not about a block has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub story: Option<String>,
    /// What to know and what to do.
    pub message: String,
    /// What to run next, when there is something.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
}

/// One page of a view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct View {
    /// The base token of the state the page describes.
    pub base: String,
    /// The mode it is in.
    pub mode: Mode,
    /// What the CLI prints: the head, the lines, the appendix and the position line.
    pub text: String,
    /// Where the page is.
    pub position: Position,
    /// Where the next page starts, when there is one.
    pub next: Option<Cursor>,
    /// The size of `text` by the estimate of design 4.4.
    pub tokens: u32,
    /// The warnings of the page, for programs.
    pub warnings: Vec<Warning>,
}

// A code is its wire name in JSON, so these are written by hand and not derived.
impl Serialize for WarningCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WarningCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<WarningCode, D::Error> {
        let name = String::deserialize(deserializer)?;
        WarningCode::from_name(&name)
            .ok_or_else(|| serde::de::Error::custom(format!("`{name}` is not a warning code")))
    }
}

impl JsonSchema for WarningCode {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("WarningCode")
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        let names: Vec<&str> = WarningCode::ALL.iter().map(|c| c.as_str()).collect();
        json_schema!({ "type": "string", "enum": names })
    }
}

/// One string or a list of strings, as a list.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

/// Reads the needles of a find: one string or a list of them.
fn terms<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Ok(match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(one) => vec![one],
        OneOrMany::Many(many) => many,
    })
}

/// Reads the texts of a find in the ledger: nothing, one string or a list of them.
fn terms_or_none<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Ok(match Option::<OneOrMany>::deserialize(deserializer)? {
        None => Vec::new(),
        Some(OneOrMany::One(one)) => vec![one],
        Some(OneOrMany::Many(many)) => many,
    })
}

/// The schema of one string or a list of strings.
fn terms_schema(_generator: &mut SchemaGenerator) -> Schema {
    json_schema!({
        "anyOf": [
            { "type": "string" },
            { "type": "array", "items": { "type": "string" } }
        ]
    })
}
