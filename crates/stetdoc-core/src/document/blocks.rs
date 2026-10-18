//! The blocks a receipt shows: which keys, in what order, how many (design `document-and-ops.md` 4.2).
//!
//! The keys are the blocks the batch touched or made and the survivors of merges, in document order and at most 200. A row or a
//! table that the batch made stands for the paragraphs inside it, which are what a reader reads. The text of each block is
//! `projection::render_blocks`, so a receipt shows a block as the readable file will hold it.

use crate::api::error::StetError;
use crate::api::files::{BlockSpec, RenderedBlock};
use crate::api::view::{FormatId, Warning};
use crate::ops::Applied;
use crate::projection::render_blocks;
use crate::refs::BaseToken;
use crate::state::PartsView;
use crate::wordml::{BlockKey, BlockKind};
use std::collections::{HashMap, HashSet};

/// The most blocks a receipt carries. The rest are counted.
pub(super) const MAX_BLOCKS: usize = 200;

/// The blocks of a receipt.
pub(super) struct Shown {
    /// The blocks, in document order.
    pub blocks: Vec<RenderedBlock>,
    /// How many more there were.
    pub omitted: u32,
    /// The warnings about them.
    pub warnings: Vec<Warning>,
}

/// True when entry `index` is inside the block at `ancestor`.
fn inside(entries: &[crate::wordml::BlockEntry], index: usize, ancestor: usize) -> bool {
    let mut at = entries.get(index).and_then(|e| e.parent);
    while let Some(parent) = at {
        if parent as usize == ancestor {
            return true;
        }
        at = entries.get(parent as usize).and_then(|e| e.parent);
    }
    false
}

/// The blocks to show, as keys in document order: touched, made and surviving blocks that are in the document (attached, in a
/// part that is still a story), with a new row or table replaced by its paragraphs.
fn keys(
    parts: &PartsView<'_>,
    applied: &Applied,
) -> Result<Vec<(usize, usize, BlockKey)>, StetError> {
    let order: HashMap<_, usize> = parts
        .story_specs()?
        .0
        .iter()
        .enumerate()
        .map(|(rank, s)| (s.part.clone(), rank))
        .collect();
    let made: HashSet<BlockKey> = applied.new_blocks().into_iter().collect();
    let mut seen: HashSet<BlockKey> = HashSet::new();
    let mut out: Vec<(usize, usize, BlockKey)> = Vec::new();
    for op in &applied.ops {
        for effect in &op.effects {
            let survivors = effect.survivor.iter();
            for key in effect
                .touched
                .iter()
                .chain(effect.new_blocks.iter())
                .chain(survivors)
            {
                if !seen.insert(key.clone()) || !parts.is_attached(key)? {
                    continue;
                }
                // A block of a header or a footer that no section names any more (a later operation of the batch accepted
                // the deletion of the section break that named it) is not in the document either: the commit retires its ref.
                let Some(part) = parts.id(key.part()) else {
                    continue;
                };
                if !parts.is_story(part)? {
                    continue;
                }
                let story = parts.story(part)?;
                let Some(index) = story.index_of(key.node()) else {
                    continue;
                };
                let rank = order.get(key.part()).copied().unwrap_or(usize::MAX);
                let entries = story.blocks();
                match entries.get(index).map(|e| e.kind) {
                    Some(BlockKind::Paragraph) => out.push((rank, index, key.clone())),
                    Some(_) if made.contains(key) => {
                        for (j, entry) in entries.iter().enumerate().skip(index + 1) {
                            if !inside(entries, j, index) {
                                break;
                            }
                            if entry.kind == BlockKind::Paragraph {
                                out.push((rank, j, story.key(entry)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out.sort_by_key(|(rank, index, _)| (*rank, *index));
    out.dedup_by(|a, b| a.2 == b.2);
    Ok(out)
}

/// The blocks a receipt shows, rendered from `parts`: the next state for an edit, the staged one for a dry run, where a block
/// that the batch made has no ref.
pub(super) fn shown(
    parts: PartsView<'_>,
    base: BaseToken,
    format: &FormatId,
    applied: &Applied,
) -> Result<Shown, StetError> {
    let all = keys(&parts, applied)?;
    let omitted = u32::try_from(all.len().saturating_sub(MAX_BLOCKS)).unwrap_or(u32::MAX);
    let wanted: Vec<BlockKey> = all
        .into_iter()
        .take(MAX_BLOCKS)
        .map(|(_, _, k)| k)
        .collect();
    if wanted.is_empty() {
        return Ok(Shown {
            blocks: Vec::new(),
            omitted: 0,
            warnings: Vec::new(),
        });
    }
    // No neighbours: `context` shows paragraphs around a block that has no ref, which only a dry run has, and a dry run's receipt
    // has the real one's blocks and no others. A new block is placed by its anchor.
    let spec = BlockSpec {
        format: format.clone(),
        context: 0,
    };
    let rendered = render_blocks(parts, base, &wanted, &spec)?;
    Ok(Shown {
        blocks: rendered.blocks,
        omitted,
        warnings: rendered.warnings,
    })
}
