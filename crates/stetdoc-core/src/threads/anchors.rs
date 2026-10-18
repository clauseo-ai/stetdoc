//! The range markers of every story, the paragraphs a comment's range touches, and the text it covers.

use super::model::Anchor;
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::state::{Dep, PartsView};
use crate::wordml::{BlockKind, CommentAnchor, Loc, NodeId, Story, StoryKind};
use crate::xml::{NodeRef, SpanTree};
use std::sync::Arc;

/// One comment's markers, as `Story::anchors` paired them, and the part they are in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Marker {
    /// The part of the story that holds the markers.
    pub(super) story: Arc<str>,
    /// The paired markers.
    pub(super) anchor: CommentAnchor,
}

/// Every comment marker of the document, in story order and then document order. It is a cache over
/// the story parts, so it is read once per state and not once per question.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Markers(pub(super) Vec<Marker>);

/// The markers of every story but the comments story, whose text holds none.
pub(super) fn markers(view: &PartsView<'_>) -> Result<Arc<Markers>, StetError> {
    let list = view.story_specs()?;
    let mut parts = Vec::new();
    for spec in list.0.iter().filter(|s| s.kind != StoryKind::Comments) {
        if let Some(id) = view.id(&spec.part) {
            parts.push((id, Arc::clone(&spec.part)));
        }
    }
    let deps: Vec<Dep> = std::iter::once(Dep::Package)
        .chain(parts.iter().map(|(id, _)| Dep::Part(*id)))
        .collect();
    let build = |_: Option<&Markers>| -> Result<Markers, StetError> {
        let mut out = Vec::new();
        for (id, name) in &parts {
            let story = view.story(*id)?;
            let tree = view.tree(*id)?;
            for anchor in story.anchors(tree).comments {
                out.push(Marker {
                    story: Arc::clone(name),
                    anchor,
                });
            }
        }
        Ok(Markers(out))
    };
    view.derived(0, &deps, &build)
}

/// The anchor of a comment from its markers: the part, the three markers and the paragraphs between
/// the start and the end. A marker that sits between blocks has no paragraph, so a range whose markers
/// both do touches none.
pub(super) fn anchor_of(view: &PartsView<'_>, marker: &Marker) -> Result<Anchor, StetError> {
    let a = &marker.anchor;
    let mut anchor = Anchor {
        story: Some(Arc::clone(&marker.story)),
        start: a.start,
        end: a.end,
        reference: a.reference,
        blocks: Vec::new(),
    };
    let Some(id) = view.id(&marker.story) else {
        return Ok(anchor);
    };
    let story = view.story(id)?;
    let tree = view.tree(id)?;
    // The paragraphs between the markers, and the one that holds the reference run, which a range that ends
    // between blocks can have after it: a caller that writes at the markers writes there too.
    let span = blocks_span(&story, tree, a.start, a.end, a.reference);
    let reference = a
        .reference
        .and_then(|l| l.paragraph)
        .and_then(|p| story.index_of(p));
    anchor.blocks = story
        .blocks()
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            e.kind == BlockKind::Paragraph
                && (Some(*i) == reference || span.is_some_and(|(lo, hi)| (lo..=hi).contains(i)))
        })
        .map(|(_, e)| story.key(e))
        .collect();
    Ok(anchor)
}

/// The first and the last block entry of a range, from its markers.
///
/// A marker in a paragraph is at that paragraph. A marker between blocks starts the blocks after it, and
/// ends the blocks before the next one, so a range between blocks covers what lies between, and a range
/// that has no block between its markers has no span. When one end is missing, the paragraph of the
/// reference run, where the range ends, stands in for it. `None` when no block can be found.
fn blocks_span(
    story: &Story,
    tree: &SpanTree,
    start: Option<Loc>,
    end: Option<Loc>,
    reference: Option<Loc>,
) -> Option<(usize, usize)> {
    let first = start.and_then(|l| match l.paragraph {
        Some(p) => story.index_of(p),
        None => next_entry(story, tree, l.node),
    });
    let end_between = end.is_some_and(|l| l.paragraph.is_none());
    let last = end.and_then(|l| match l.paragraph {
        Some(p) => story.index_of(p),
        None => match next_entry(story, tree, l.node) {
            Some(k) => k.checked_sub(1),
            None => story.blocks().len().checked_sub(1),
        },
    });
    let reference = reference
        .and_then(|l| l.paragraph)
        .and_then(|p| story.index_of(p));
    match (first, last) {
        // The end is before the start, between blocks: nothing lies between the markers.
        (Some(i), Some(j)) if end_between && j < i => None,
        (Some(i), Some(j)) => Some((i.min(j), i.max(j))),
        (Some(i), None) | (None, Some(i)) => Some(match reference {
            Some(r) => (i.min(r), i.max(r)),
            None => (i, i),
        }),
        (None, None) => reference.map(|r| (r, r)),
    }
}

/// How many nodes a search for the next block looks at before it gives up.
const SEARCH_BUDGET: usize = 100_000;

/// The first block entry at or inside `n`, in document order.
fn first_entry(story: &Story, n: &NodeRef<'_>, budget: &mut usize, depth: usize) -> Option<usize> {
    if let Some(i) = story.index_of(n.id()) {
        return Some(i);
    }
    if depth == 0 {
        return None;
    }
    let mut child = n.first_child();
    while let Some(c) = child {
        if *budget == 0 {
            return None;
        }
        *budget -= 1;
        if c.is_element()
            && let Some(i) = first_entry(story, &c, budget, depth - 1)
        {
            return Some(i);
        }
        child = c.next_sibling();
    }
    None
}

/// The index of the first block that follows the element `node`, which sits between blocks: the next
/// sibling that is a block or holds one, and if there is none, the next one after the parent.
fn next_entry(story: &Story, tree: &SpanTree, node: NodeId) -> Option<usize> {
    let mut budget = SEARCH_BUDGET;
    let mut at = tree.node_ref(node);
    loop {
        match at.next_sibling() {
            Some(s) => {
                if budget == 0 {
                    return None;
                }
                budget -= 1;
                if s.is_element()
                    && let Some(i) = first_entry(story, &s, &mut budget, 64)
                {
                    return Some(i);
                }
                at = s;
            }
            None => {
                let parent = at.parent()?;
                if !parent.is_element() || parent.parent().is_none_or(|p| !p.is_element()) {
                    // the root element holds the blocks, and nothing follows its last child
                    return None;
                }
                at = parent;
            }
        }
    }
}

/// The text a comment's range covers in `mode`, paragraphs joined by a line feed, cut to `limit`
/// characters. `None` when the comment has no start and end in paragraphs. A point comment covers
/// the empty string.
pub(super) fn covered_text(
    view: &PartsView<'_>,
    anchor: &Anchor,
    mode: Mode,
    limit: usize,
) -> Result<Option<String>, StetError> {
    let (Some(part), Some(start), Some(end)) = (&anchor.story, anchor.start, anchor.end) else {
        return Ok(None);
    };
    let Some(id) = view.id(part) else {
        return Ok(None);
    };
    let story = view.story(id)?;
    let tree = view.tree(id)?;
    // The blocks are `anchor`'s own. A start or an end between blocks is the edge of the first or the
    // last block, and a range with no block between its markers covers nothing.
    let Some((lo, hi)) = blocks_span(&story, tree, Some(start), Some(end), anchor.reference) else {
        return Ok(Some(String::new()));
    };
    let at = |loc: crate::wordml::Loc| loc.paragraph.and_then(|p| story.index_of(p));
    let (start_at, end_at) = (at(start), at(end));
    let mut out = String::new();
    let mut taken = 0usize;
    for (k, entry) in story.blocks().iter().enumerate().skip(lo).take(hi - lo + 1) {
        if entry.kind != BlockKind::Paragraph {
            continue;
        }
        let Some(paragraph) = story.paragraph(tree, k) else {
            continue;
        };
        let map = paragraph.map();
        let text = map.text(mode);
        let len = text.chars().count();
        let from = if Some(k) == start_at {
            map.offset_of(mode, start.node, 0).unwrap_or(0).min(len)
        } else {
            0
        };
        let to = if Some(k) == end_at {
            map.offset_of(mode, end.node, 0).unwrap_or(len).min(len)
        } else {
            len
        };
        if !out.is_empty() || k != lo {
            if taken >= limit {
                break;
            }
            out.push('\n');
            taken += 1;
        }
        for c in text.chars().skip(from).take(to.saturating_sub(from)) {
            if taken >= limit {
                return Ok(Some(out));
            }
            out.push(c);
            taken += 1;
        }
    }
    Ok(Some(out))
}
