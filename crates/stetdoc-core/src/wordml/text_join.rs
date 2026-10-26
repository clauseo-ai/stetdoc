//! Paragraphs that a revised paragraph mark joins, and a text map across them.
//!
//! In `Final` a paragraph whose mark is deleted or moved away reads as one paragraph with the next,
//! and in `Original` so does one whose mark is inserted or moved in. `Story::join_run` finds which
//! entries read as one, and `Joined` maps a range of their joined text back to each paragraph's own
//! map. A deleted mark joins the text directly, as Word does, so there is no separator character.
//! `docs/design/wordml.md` 3.6 and 5.4.

use super::mc;
use super::names::{is_w, is_word_ns};
use super::paragraph::Paragraph;
use super::story::{BlockKind, Story};
use super::text_locate::{Gap, Located};
use super::text_model::TextMap;
use crate::api::view::Mode;
use crate::xml::{NodeId, NodeRef, SpanTree};
use std::ops::Range;

/// Elements a paragraph can sit in that only group blocks and are not a container of their own.
const TRANSPARENT: [&str; 8] = [
    "sdt",
    "sdtContent",
    "customXml",
    "smartTag",
    "ins",
    "del",
    "moveFrom",
    "moveTo",
];

/// The element that holds a block, reading through the elements that only group blocks: the cell,
/// the body, the note or comment, the header or footer. Two paragraphs join only in the same one.
/// Word reads through an element that `mc:ProcessContent` names as well, as the story scan does.
fn container_of(node: &NodeRef<'_>) -> Option<NodeId> {
    let mut cur = node.parent()?;
    loop {
        if !groups_blocks(&cur) {
            return Some(cur.id());
        }
        cur = cur.parent()?;
    }
}

/// True for an element that only groups blocks and is not a container of its own (`container_of`).
fn groups_blocks(n: &NodeRef<'_>) -> bool {
    TRANSPARENT.iter().any(|name| is_w(n, name))
        || mc::is_branch(n)
        || mc::is_alternate_content(n)
        || (!is_word_ns(n.namespace()) && matches!(mc::disposition(n), mc::Mc::ProcessContent))
}

/// Elements that sit between blocks and are not blocks: range markers, and the body's own section properties.
const BETWEEN_BLOCKS: [&str; 20] = [
    "bookmarkStart",
    "bookmarkEnd",
    "commentRangeStart",
    "commentRangeEnd",
    "permStart",
    "permEnd",
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
    "customXmlInsRangeStart",
    "customXmlInsRangeEnd",
    "customXmlDelRangeStart",
    "customXmlDelRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveToRangeStart",
    "customXmlMoveToRangeEnd",
    "proofErr",
    "sectPr",
];

/// True if no block follows `node` in its container (the cell, note, comment, header, footer or body), reading through
/// the elements that only group blocks as [`Story::join_run`] does. A paragraph there has a mark with nothing to join
/// into: the last paragraph of a cell ends the cell, and Word keeps it when its inserted mark is rejected (design
/// `revisions.md` 3.2.1).
pub fn ends_container(tree: &SpanTree, node: NodeId) -> bool {
    let mut cur = tree.node_ref(node);
    loop {
        // The other branch of a markup-compatibility choice is not a block after this one.
        if !mc::is_branch(&cur) {
            let mut sib = cur.next_sibling();
            while let Some(s) = sib {
                if s.is_element() && !BETWEEN_BLOCKS.iter().any(|name| is_w(&s, name)) {
                    return false;
                }
                sib = s.next_sibling();
            }
        }
        match cur.parent() {
            Some(parent) if groups_blocks(&parent) => cur = parent,
            _ => return true,
        }
    }
}

impl Story {
    /// The entries, starting at paragraph `i`, that read as one paragraph in `mode`: `i` and each
    /// following paragraph while the one before it has a mark that `mode` removes
    /// ([`Paragraph::joins_next`]). The join applies only when the next entry is a paragraph in the
    /// same container (the same cell, the same note, the same body) and is ignored at the end of a
    /// cell or story, as Word ignores it. An entry that is not a paragraph gives an empty range. The
    /// cost is the length of the run, so a caller that visits every paragraph continues from the end
    /// of the range it got and does not ask again for each entry inside it.
    pub fn join_run(&self, tree: &SpanTree, i: usize, mode: Mode) -> Range<usize> {
        if self
            .entries
            .get(i)
            .is_none_or(|e| e.kind != BlockKind::Paragraph)
        {
            return i..i;
        }
        let mut last = i;
        while self.joins_with_next(tree, last)
            && self
                .entries
                .get(last)
                .is_some_and(|e| Paragraph::new(tree, e.node).is_some_and(|p| p.joins_next(mode)))
        {
            last += 1;
        }
        i..last + 1
    }

    /// True if the entry after paragraph `i` is a paragraph in the same container: the same cell,
    /// note, comment, header, footer or body, read through the elements that only group blocks. This is
    /// the structural half of the join rule, independent of mode: a revised mark joins only when it
    /// holds, and it is ignored at the end of a cell or story.
    pub(super) fn joins_with_next(&self, tree: &SpanTree, i: usize) -> bool {
        let paragraph = |k: usize| {
            self.entries
                .get(k)
                .filter(|e| e.kind == BlockKind::Paragraph)
        };
        let (Some(here), Some(next)) = (paragraph(i), paragraph(i + 1)) else {
            return false;
        };
        here.unit == next.unit
            && container_of(&tree.node_ref(here.node)) == container_of(&tree.node_ref(next.node))
    }

    /// `joins_with_next`, for a caller that asks it of every entry in order: `known` keeps the container of the
    /// last paragraph looked at, which is the next one's own, so each container is found once.
    pub(super) fn joins_with_next_in_order(
        &self,
        tree: &SpanTree,
        i: usize,
        known: &mut Option<(NodeId, Option<NodeId>)>,
    ) -> bool {
        let paragraph = |k: usize| {
            self.entries
                .get(k)
                .filter(|e| e.kind == BlockKind::Paragraph)
        };
        let (Some(here), Some(next)) = (paragraph(i), paragraph(i + 1)) else {
            return false;
        };
        let here_container = match *known {
            Some((node, container)) if node == here.node => container,
            _ => container_of(&tree.node_ref(here.node)),
        };
        let next_container = container_of(&tree.node_ref(next.node));
        *known = Some((next.node, next_container));
        here.unit == next.unit && here_container == next_container
    }

    /// The paragraphs [`Story::join_run`] finds from entry `i`, with their text joined: the range of
    /// entries and the [`Joined`] over their maps. `None` if entry `i` is not a paragraph, or if `tree`
    /// was edited since the story was scanned ([`Story::is_current`]).
    pub fn joined(&self, tree: &SpanTree, i: usize, mode: Mode) -> Option<(Range<usize>, Joined)> {
        let run = self.join_run(tree, i, mode);
        if run.is_empty() || !self.is_current(tree) {
            return None;
        }
        let maps: Vec<TextMap> = run
            .clone()
            .filter_map(|k| self.paragraph(tree, k))
            .map(|p| p.map())
            .collect();
        Some((run, Joined::new(maps, mode)))
    }
}

/// The maps of paragraphs that read as one, in order, and the text of a mode across them.
#[derive(Clone, Debug)]
pub struct Joined {
    maps: Vec<TextMap>,
    mode: Mode,
    /// The offset in the joined text at which each map starts.
    starts: Vec<usize>,
    text: String,
}

impl Joined {
    /// The joined text of `mode` over the maps of a run, in order.
    pub fn new(maps: Vec<TextMap>, mode: Mode) -> Joined {
        let mut starts = Vec::with_capacity(maps.len());
        let mut text = String::new();
        let mut at = 0usize;
        for m in &maps {
            starts.push(at);
            at += m.len(mode);
            text.push_str(&m.text(mode));
        }
        Joined {
            maps,
            mode,
            starts,
            text,
        }
    }

    /// The mode's texts, concatenated with no separator.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The length of the joined text in characters.
    pub fn len(&self) -> usize {
        self.starts
            .last()
            .zip(self.maps.last())
            .map_or(0, |(s, m)| s + m.len(self.mode))
    }

    /// True if the joined text is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The maps, in order.
    pub fn maps(&self) -> &[TextMap] {
        &self.maps
    }

    /// The map that holds the character at `offset`, or for an offset at the end of a map, that map:
    /// a position on a join belongs to the paragraph on its left. Offset 0 belongs to the first.
    fn owner(&self, offset: usize) -> Option<usize> {
        if offset > self.len() {
            return None;
        }
        self.starts
            .iter()
            .zip(&self.maps)
            .enumerate()
            .position(|(k, (start, m))| {
                offset <= start + m.len(self.mode) && (offset > *start || k == 0)
            })
    }

    /// What a range of the joined text covers: one [`Located`] for each map the range has characters
    /// in, with the offsets of that map. An empty range gives the one `Located` of the map that owns
    /// the offset. `None` if the range starts after it ends or goes past the end.
    pub fn locate(&self, range: Range<usize>) -> Option<Vec<(usize, Located)>> {
        if range.start > range.end || range.end > self.len() {
            return None;
        }
        if range.is_empty() {
            let k = self.owner(range.start)?;
            let start = self.starts.get(k).copied()?;
            let at = range.start - start;
            return Some(vec![(k, self.maps.get(k)?.locate(self.mode, at..at)?)]);
        }
        let mut out = Vec::new();
        for (k, (start, m)) in self.starts.iter().zip(&self.maps).enumerate() {
            let end = start + m.len(self.mode);
            let (a, b) = (range.start.max(*start), range.end.min(end));
            if a < b {
                out.push((k, m.locate(self.mode, a - start..b - start)?));
            }
        }
        Some(out)
    }

    /// The insertion point at `offset` of the joined text, with the index of the map it is in. A gap on
    /// a join belongs to the map on its left. `None` if the offset is past the end.
    pub fn gap(&self, offset: usize) -> Option<(usize, Gap)> {
        let k = self.owner(offset)?;
        let start = self.starts.get(k).copied()?;
        Some((k, self.maps.get(k)?.gap(self.mode, offset - start)?))
    }
}
