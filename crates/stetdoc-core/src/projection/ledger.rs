//! The change ledger: every tracked change of a document, as `RevisionInfo` (design 4.6).
//!
//! `Document::revisions` and the `Ledger` scope of a view both list the changes through here, so a filter means the
//! same in both. `changes_of` reads every change in story order, `LedgerFilter` resolves a filter against the authors
//! once, and `describe` says what one change is in the public terms. The grouping of the changes is `revisions`' own
//! (`revisions::changes`), so nothing here decides what a change is.

use super::authors::authors;
use super::needles::Needles;
use crate::api::error::{ErrorCode, StetError};
use crate::api::revision::{ChangeKind, RevisionInfo};
use crate::api::view::RevisionFilter;
use crate::refs::BlockRef;
use crate::revisions::changes::{Change, ChangeKind as Kind, changes};
use crate::state::PartsView;
use crate::wordml::{Story, StoryKind};

/// The most characters of inserted or deleted text that a `RevisionInfo` carries.
const TEXT_CUT: usize = 200;

/// The most a `RevisionInfo` carries when the filter asks for the full text: all that the change carries.
const FULL_CUT: usize = crate::revisions::changes::CHANGE_TEXT_LIMIT;

/// The families a `kind` filter names, in the words `view --kind` takes.
const FAMILIES: [&str; 4] = ["insert", "delete", "move", "format"];

/// The public name of a change kind: the two enums have the same twenty names.
pub(crate) fn kind_of(kind: Kind) -> ChangeKind {
    match kind {
        Kind::Insertion => ChangeKind::Insertion,
        Kind::Deletion => ChangeKind::Deletion,
        Kind::Move => ChangeKind::Move,
        Kind::ParagraphInserted => ChangeKind::ParagraphInserted,
        Kind::ParagraphDeleted => ChangeKind::ParagraphDeleted,
        Kind::MarkInserted => ChangeKind::MarkInserted,
        Kind::MarkDeleted => ChangeKind::MarkDeleted,
        Kind::RowInserted => ChangeKind::RowInserted,
        Kind::RowDeleted => ChangeKind::RowDeleted,
        Kind::CellInserted => ChangeKind::CellInserted,
        Kind::CellDeleted => ChangeKind::CellDeleted,
        Kind::CellMerged => ChangeKind::CellMerged,
        Kind::RunFormat => ChangeKind::RunFormat,
        Kind::ParagraphFormat => ChangeKind::ParagraphFormat,
        Kind::SectionFormat => ChangeKind::SectionFormat,
        Kind::TableFormat => ChangeKind::TableFormat,
        Kind::RowFormat => ChangeKind::RowFormat,
        Kind::CellFormat => ChangeKind::CellFormat,
        Kind::GridChange => ChangeKind::GridChange,
        Kind::NumberingChange => ChangeKind::NumberingChange,
    }
}

/// The family a change belongs to for a filter: `insert`, `delete`, `move` or `format`.
pub(crate) fn family(kind: ChangeKind) -> &'static str {
    match kind {
        ChangeKind::Insertion
        | ChangeKind::ParagraphInserted
        | ChangeKind::MarkInserted
        | ChangeKind::RowInserted
        | ChangeKind::CellInserted => "insert",
        ChangeKind::Deletion
        | ChangeKind::ParagraphDeleted
        | ChangeKind::MarkDeleted
        | ChangeKind::RowDeleted
        | ChangeKind::CellDeleted => "delete",
        ChangeKind::Move => "move",
        _ => "format",
    }
}

/// What the story is called in `RevisionInfo`.
pub(crate) fn story_name(kind: StoryKind) -> &'static str {
    match kind {
        StoryKind::Body => "body",
        StoryKind::Header | StoryKind::Footer => "headers",
        StoryKind::Footnotes | StoryKind::Endnotes => "notes",
        StoryKind::Comments => "comments",
    }
}

/// Text cut at `limit` characters, `None` for none, and whether it was cut.
fn cut(text: &str, already: bool, limit: usize) -> (Option<String>, bool) {
    if text.is_empty() {
        return (None, already);
    }
    let mut chars = text.chars();
    let head: String = chars.by_ref().take(limit).collect();
    let more = chars.next().is_some();
    (Some(head), more || already)
}

/// The public description of one change, which is in the story `kind` of `story`, its text cut at 200 characters.
pub(crate) fn describe(
    change: &Change,
    parts: &PartsView<'_>,
    story: &Story,
    kind: StoryKind,
) -> RevisionInfo {
    describe_cut(change, parts, story, kind, TEXT_CUT)
}

/// `describe`, with the text cut at `limit` characters.
fn describe_cut(
    change: &Change,
    parts: &PartsView<'_>,
    story: &Story,
    kind: StoryKind,
    limit: usize,
) -> RevisionInfo {
    let block: Option<String> = story
        .blocks()
        .get(change.blocks.start)
        .and_then(|entry| parts.ref_of(&story.key(entry)))
        .map(|r: BlockRef| r.to_string());
    let (inserted, cut_in) = cut(&change.inserted, change.text_cut, limit);
    let (deleted, cut_out) = cut(&change.deleted, change.text_cut, limit);
    RevisionInfo {
        ids: change.ids.clone(),
        kind: kind_of(change.kind),
        author: change.author.clone(),
        date: change.date.clone(),
        block,
        story: story_name(kind).to_string(),
        inserted,
        deleted,
        text_truncated: cut_in || cut_out,
    }
}

/// One change in ledger order: its revision ids, and its description when the filter keeps it.
pub(crate) struct Listed {
    /// Every revision id of the change, in document order.
    pub ids: Vec<i64>,
    /// The change as `Document::revisions` describes it, when the filter keeps it. Only a kept change is described, so
    /// a change the filter drops reads no block and issues no ref.
    pub shown: Option<RevisionInfo>,
}

/// Every change of the document, in the order of the stories and then of the document, each with its description when
/// `keep` keeps it.
pub(crate) fn changes_of(
    parts: PartsView<'_>,
    keep: &LedgerFilter,
) -> Result<Vec<Listed>, StetError> {
    let mut out = Vec::new();
    for spec in &parts.story_specs()?.0 {
        let Some(part) = parts.id(&spec.part) else {
            continue;
        };
        let story = parts.story(part)?;
        let tree = parts.tree(part)?;
        for change in changes(&story, tree, None) {
            let shown = (keep.keeps(change.author.as_deref(), kind_of(change.kind))
                && keep.holds(&change))
            .then(|| describe_cut(&change, &parts, &story, spec.kind, keep.cut));
            out.push(Listed {
                ids: change.ids.clone(),
                shown,
            });
        }
    }
    Ok(out)
}

/// A `RevisionFilter` resolved against the document: the author is the name the changes carry, and the kind is a family.
pub(crate) struct LedgerFilter {
    /// The name of the one author kept, when the filter names one.
    author: Option<String>,
    /// The family of the one kind kept, when the filter names one.
    family: Option<&'static str>,
    /// The texts a kept change holds one of. Empty keeps every change.
    needles: Needles,
    /// How many characters of text a description carries.
    cut: usize,
}

impl LedgerFilter {
    /// Resolves `filter`. An author is a name or the short code the views show, and a kind is one of `insert`, `delete`,
    /// `move` or `format`.
    pub(crate) fn of(
        parts: PartsView<'_>,
        filter: &RevisionFilter,
    ) -> Result<LedgerFilter, StetError> {
        let family = match filter.kind.as_deref() {
            None => None,
            Some(given) => Some(
                FAMILIES
                    .into_iter()
                    .find(|f| *f == given)
                    .ok_or_else(|| {
                        StetError::new(
                            ErrorCode::InvalidPayload,
                            format!(
                                "the kind of a change is `insert`, `delete`, `move` or `format`, not `{given}`."
                            ),
                        )
                        .with_next("Give one of those four words, or leave out the kind.")
                    })?,
            ),
        };
        let author = match &filter.author {
            None => None,
            Some(given) => {
                let listed = authors(parts)?;
                let name = listed
                    .iter()
                    .find(|(_, name)| name == given)
                    .or_else(|| listed.iter().find(|(code, _)| code == given))
                    .map(|(_, name)| name.clone());
                Some(name.unwrap_or_else(|| given.clone()))
            }
        };
        let needles = Needles::parse(&filter.find, "a find in the changes")?;
        let cut = if filter.full { FULL_CUT } else { TEXT_CUT };
        Ok(LedgerFilter {
            author,
            family,
            needles,
            cut,
        })
    }

    /// True if the change's inserted or deleted text holds one of the needles of the filter, or the filter has none. The
    /// text is all the change carries, up to 1,000 characters each way, and not only what a description shows.
    pub(crate) fn holds(&self, change: &Change) -> bool {
        self.needles.is_empty()
            || [change.inserted.as_str(), change.deleted.as_str()]
                .into_iter()
                .any(|text| self.needles.any_in(text))
    }

    /// True if a change by `author` of `kind` is kept: by the author the filter names, and of the kind it names.
    pub(crate) fn keeps(&self, author: Option<&str>, kind: ChangeKind) -> bool {
        let who = author
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .unwrap_or("Unknown");
        if self.author.as_deref().is_some_and(|w| w != who) {
            return false;
        }
        self.family.is_none_or(|k| k == family(kind))
    }
}
