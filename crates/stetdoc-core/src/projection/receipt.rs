//! `render_blocks`: the blocks a receipt shows, as the readable files will hold them (design 8).
//!
//! A receipt carries the post-edit text of every block an edit touched, so that an agent can compare it with what
//! it wrote, and the warnings about those blocks, so that a receipt cannot show a block without the signals about
//! it. A block that a dry run created has no ref yet (refs are minted at commit), so it comes back with none and
//! with the ref of the block it follows, and the lines around it are shown too.

use super::build::{Selection, StorySel, project};
use super::formats::{Format, by_id};
use super::model::{Block, Label, Output};
use crate::api::error::StetError;
use crate::api::files::{BlockRender, BlockSpec, RenderedBlock};
use crate::api::view::{Mode, Warning, WarningCode};
use crate::refs::{BaseToken, BlockKey};
use crate::state::{PartId, PartsView};
use crate::wordml::{BlockKind, StoryKind};

/// What the story is called in a receipt and in a warning.
fn story_label(kind: StoryKind) -> &'static str {
    match kind {
        StoryKind::Body => "body",
        StoryKind::Header | StoryKind::Footer => "headers",
        StoryKind::Footnotes | StoryKind::Endnotes => "notes",
        StoryKind::Comments => "comments",
    }
}

/// True if `text` starts with the number `label` that Word shows for the paragraph, as a number typed by hand:
/// the label, and then a space, a tab, a closing mark or the end.
fn starts_with_label(text: &str, label: &Label) -> bool {
    if label.bullet || label.ghost {
        return false;
    }
    let wanted = label.text.trim().trim_end_matches('.');
    if wanted.is_empty() {
        return false;
    }
    let text = text.trim_start();
    let Some(rest) = text.strip_prefix(wanted) else {
        return false;
    };
    rest.chars()
        .next()
        .is_none_or(|c| c.is_whitespace() || matches!(c, '.' | ')' | ':'))
}

/// One paragraph as a receipt shows it, and the warnings about it.
struct One {
    text: String,
    warnings: Vec<Warning>,
}

/// The paragraph at `entry` of `part`: its lines, and its warnings, which include `duplicate_number` when its text
/// starts with the number that Word also adds.
fn one(
    parts: PartsView<'_>,
    base: BaseToken,
    format: &dyn Format,
    part: PartId,
    entry: usize,
) -> Result<Option<One>, StetError> {
    let selection = Selection {
        stories: vec![(part, StorySel::Paragraphs(vec![entry]))],
    };
    let model = project(parts, base, Mode::Redline, Output::Files, &selection)?;
    let texts = format.render_paragraphs(&model);
    let Some(text) = texts.into_iter().next() else {
        return Ok(None);
    };
    let mut warnings = model.head.warnings.clone();
    let story = parts.story(part)?;
    let tree = parts.tree(part)?;
    if let Some(p) = story.paragraph(tree, entry) {
        let paragraph = model
            .stories()
            .iter()
            .flat_map(|s| &s.units)
            .flat_map(|u| &u.blocks)
            .find_map(|b| match b {
                Block::Paragraph(par) => Some(par),
                _ => None,
            });
        if let Some(par) = paragraph
            && let Some(label) = &par.label
            && starts_with_label(&p.text(Mode::Final), label)
        {
            warnings.push(Warning {
                code: WarningCode::DuplicateNumber,
                block: par.r#ref.map(|r| r.to_string()),
                story: Some(story_label(story.kind()).to_string()),
                // The message names no ref, so that a dry run, whose new paragraph has none yet, says the same (H14): the
                // warning's block names the paragraph.
                message: format!(
                    "the paragraph is numbered {} by Word, and its text starts with {} as well, so the number shows twice",
                    label.text,
                    label.text.trim_end_matches('.')
                ),
                next: Some("Take the typed number out of the text.".to_string()),
            });
        }
    }
    Ok(Some(One { text, warnings }))
}

/// The ref of the nearest block before `entry` that has one: where a block with no ref is anchored.
fn anchor_of(
    parts: &PartsView<'_>,
    part: PartId,
    entry: usize,
) -> Result<Option<String>, StetError> {
    let story = parts.story(part)?;
    Ok(story
        .blocks()
        .get(..entry)
        .unwrap_or_default()
        .iter()
        .rev()
        .find_map(|e| parts.ref_of(&story.key(e)))
        .map(|r| r.to_string()))
}

/// The blocks of a receipt as the files will hold them, in document order, with the warnings about them. Only
/// paragraphs have text of their own: a key that names a table, a row or something that is not in the document is
/// left out. A block that has no ref gets `context` paragraphs on each side, and the ref of the block it follows.
/// `base` is the token of the state the receipt is for. It is only the head of a file, which a receipt does not
/// show.
pub(crate) fn render_blocks(
    parts: PartsView<'_>,
    base: BaseToken,
    blocks: &[BlockKey],
    spec: &BlockSpec,
) -> Result<BlockRender, StetError> {
    let format = by_id(spec.format.as_str())?;
    let list = parts.story_specs()?;
    let rank = |part: &str| {
        list.0
            .iter()
            .position(|s| &*s.part == part)
            .unwrap_or(usize::MAX)
    };
    // (story rank, entry, part, asked for)
    let mut wanted: Vec<(usize, usize, PartId, bool)> = Vec::new();
    for key in blocks {
        let Some(part) = parts.id(key.part()) else {
            continue;
        };
        let story = parts.story(part)?;
        let Some(entry) = story.index_of(key.node()) else {
            continue;
        };
        if story.blocks().get(entry).map(|e| e.kind) != Some(BlockKind::Paragraph) {
            continue;
        }
        wanted.push((rank(key.part()), entry, part, true));
        if parts.ref_of(key).is_none() {
            let reach = usize::from(spec.context);
            let around = |range: &mut dyn Iterator<Item = usize>| -> Vec<usize> {
                range
                    .filter(|i| {
                        story.blocks().get(*i).map(|e| e.kind) == Some(BlockKind::Paragraph)
                    })
                    .take(reach)
                    .collect()
            };
            let before = around(&mut (0..entry).rev());
            let after = around(&mut (entry + 1..story.blocks().len()));
            for i in before.into_iter().chain(after) {
                wanted.push((rank(key.part()), i, part, false));
            }
        }
    }
    wanted.sort_by_key(|w| (w.0, w.1, !w.3));
    wanted.dedup_by_key(|w| (w.0, w.1));
    let mut out = BlockRender {
        blocks: Vec::new(),
        warnings: Vec::new(),
    };
    for (_, entry, part, _) in wanted {
        let Some(made) = one(parts, base, format, part, entry)? else {
            continue;
        };
        let story = parts.story(part)?;
        let key = story.blocks().get(entry).map(|e| story.key(e));
        let r#ref = key.as_ref().and_then(|k| parts.ref_of(k));
        let anchor = if r#ref.is_none() {
            anchor_of(&parts, part, entry)?
        } else {
            None
        };
        out.blocks.push(RenderedBlock {
            r#ref: r#ref.map(|r| r.to_string()),
            story: story_label(story.kind()).to_string(),
            text: made.text,
            anchor,
        });
        for w in made.warnings {
            if !out.warnings.contains(&w) {
                out.warnings.push(w);
            }
        }
    }
    Ok(out)
}
