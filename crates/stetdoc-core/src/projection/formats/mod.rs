//! The format seam: the one trait in the core with more than one adapter.
//!
//! A format renders the neutral model into files or a page and parses edited files back into a
//! `Draft`, and it is the only code that knows syntax. Formats are stateless values: an adapter with
//! options is several registered values with several ids, so `ViewSpec.format` and the complement's
//! recorded format are one string, and an experiment arm is an id. The registry lists them in a stable
//! order, and the conformance suite (`conformance.rs`) runs on every one.
//!
//! A format may vary spelling and never information. Every read-only thing and every span edge of the
//! model must be rendered, so that `parse` recovers the same `Key`s, and a format that wants to drop
//! information says so in its `policy`.

#[cfg(test)]
mod conformance;
pub(crate) mod json;
mod json_model;
pub(super) mod json_value;
mod json_view;
pub(crate) mod stet;

use super::draft::{Draft, Policy};
use super::model::Projection;
use crate::api::error::{ErrorCode, StetError};
use crate::api::limits::Limits;
use crate::api::view::FormatId;
use crate::refs::{BlockRef, sha256};

/// Where a renderer broke a long paragraph, so that a split at that place can be told from a split the
/// agent meant (`split_at_wrap`, design 5.3 and 6.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Wrap {
    /// The file.
    pub file: String,
    /// The paragraph.
    pub r#ref: BlockRef,
    /// The text slot that the continuation starts in.
    pub slot: u32,
    /// How many characters of that slot come before the continuation.
    pub at: u32,
}

/// What a format wrote.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rendered {
    /// `(name, text)` for each file. A view is one file.
    pub files: Vec<(String, String)>,
    /// Where long paragraphs were wrapped. Empty for a format that does not wrap.
    pub wraps: Vec<Wrap>,
}

/// What an edit of files is compared with: see `Format::baseline`.
#[derive(Debug)]
pub(crate) struct BaselineParts {
    /// The draft of the files.
    pub draft: Draft,
    /// Where the renderer broke long paragraphs.
    pub wraps: Vec<Wrap>,
    /// The SHA-256 of each file as rendered, by name.
    pub hashes: Vec<(String, [u8; 32])>,
}

/// The general way to make the baseline of a format: render the model, then make its draft.
pub(crate) fn baseline_by_rendering<F: Format + ?Sized>(
    format: &F,
    model: Projection,
) -> BaselineParts {
    let rendered = format.render(&model);
    let draft = model.draft(&format.policy());
    drop(model);
    BaselineParts {
        draft,
        wraps: rendered.wraps,
        hashes: rendered
            .files
            .iter()
            .map(|(name, text)| (name.clone(), sha256(text.as_bytes())))
            .collect(),
    }
}

/// A projection format.
pub(crate) trait Format: Send + Sync {
    /// The id: `stet/1`, `json/1`, or the id of an experiment arm.
    fn id(&self) -> &'static str;

    /// What the format writes apart from syntax, so that `Projection::draft` can predict `parse`.
    fn policy(&self) -> Policy;

    /// Writes every `Body` variant. A view is one file.
    fn render(&self, projection: &Projection) -> Rendered;

    /// What an edit of files is compared with, from the projection of those files, which it takes so that the
    /// projection can be dropped as soon as the draft is made: the draft, where `render` breaks long paragraphs,
    /// and the SHA-256 of each file as `render` writes it. It is `render(&model)` and `model.draft(..)` without
    /// the text and, for a format that can, without the model, which with the draft and the text are three copies
    /// of a story at once.
    fn baseline(&self, model: Projection) -> BaselineParts {
        baseline_by_rendering(self, model)
    }

    /// The text of each top-level paragraph of the projection as the files hold it, in order, with no head and no
    /// unit, table or row lines: the line or lines of a paragraph, as a receipt shows a block. A paragraph in a
    /// cell is shown without its cell tags, because it is shown alone.
    fn render_paragraphs(&self, projection: &Projection) -> Vec<String>;

    /// Reads edited files into a draft. Only files are edited, so only `Body::Stories` can come back.
    fn parse(&self, files: &[(String, String)], limits: &Limits) -> Result<Draft, StetError>;
}

/// The top-level paragraphs of a projection, with the story each is in, in order.
pub(crate) fn top_paragraphs(
    projection: &Projection,
) -> Vec<(
    crate::projection::model::Story,
    crate::projection::model::Paragraph,
)> {
    let mut out = Vec::new();
    for story in projection.stories() {
        for unit in &story.units {
            for block in &unit.blocks {
                if let crate::projection::model::Block::Paragraph(p) = block {
                    out.push((story.clone(), p.clone()));
                }
            }
        }
    }
    out
}

/// The registered formats, in a stable order.
pub(crate) fn registry() -> &'static [&'static dyn Format] {
    static REGISTRY: [&dyn Format; 2] = [&stet::StetFormat, &json::JsonFormat];
    &REGISTRY
}

/// The format of readable files with this id. `json/1` is for `--json` and for programs, and never a
/// working-copy format (design 5.8), so it is refused here as `unsupported`; an id that is not registered is
/// `invalid_payload`.
pub(crate) fn files_format(id: &str) -> Result<&'static dyn Format, StetError> {
    let format = by_id(id)?;
    if format.id() == FormatId::JSON {
        return Err(StetError::new(
            ErrorCode::Unsupported,
            format!(
                "{} is not a working-copy format: it is for --json and for programs, so no readable files are \
                 written or read in it.",
                FormatId::JSON
            ),
        )
        .with_next(format!("Write and edit the readable files in {}.", FormatId::STET)));
    }
    Ok(format)
}

/// The format with this id. An id that is not registered is `invalid_payload`.
pub(crate) fn by_id(id: &str) -> Result<&'static dyn Format, StetError> {
    registry()
        .iter()
        .copied()
        .find(|format| format.id() == id)
        .ok_or_else(|| {
            let known: Vec<&str> = registry().iter().map(|format| format.id()).collect();
            StetError::new(
                ErrorCode::InvalidPayload,
                format!(
                    "{id:?} is not a projection format. The formats are {}.",
                    known.join(", ")
                ),
            )
        })
}
