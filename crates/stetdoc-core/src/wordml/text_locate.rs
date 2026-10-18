//! The operations of the map: from text offsets to XML positions and back.
//!
//! `locate` turns a text range into the XML it covers, `gap` finds the insertion point between two
//! characters, `offset_of` goes back from an XML position to an offset, and `find_all` finds a string.
//! Every offset is a count of Unicode scalar values, which is what `Editor::replace_text_range` counts.
//! `docs/design/wordml.md` section 5.

use super::text_model::{Item, TextMap};
use crate::api::view::Mode;
use crate::xml::NodeId;
use std::ops::Range;

/// One item a text range touches, with the characters of it that are in the range, in the terms of
/// the node: `start..end` are character positions in the node's decoded text, exactly what
/// `Editor::replace_text_range(node, start, end, with)` takes. For an atom, `0..1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    /// The index of the item in [`TextMap::items`].
    pub item: usize,
    /// The text node, or for an atom the element.
    pub node: NodeId,
    /// Where in the node the range starts.
    pub start: u32,
    /// Where in the node the range ends.
    pub end: u32,
    /// True if the range covers the whole item.
    pub whole: bool,
}

/// What a text range covers in XML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Located {
    /// The items the range has characters of, in order.
    pub spans: Vec<Span>,
    /// The indexes of the items from the first span to the last, so the marks and the items the mode
    /// excludes (a deleted run, in `Final`) that sit inside the range are in reach. Empty for an
    /// empty range, at the place the range sits.
    pub items: Range<usize>,
}

/// The place between two characters, in XML terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    /// The item that holds the character before the gap, if there is one.
    pub left: Option<usize>,
    /// The item that holds the character after the gap, if there is one.
    pub right: Option<usize>,
    /// The indexes of every item strictly between `left` and `right` in XML order: marks, and items
    /// the mode excludes, such as a deleted run that sits at the gap. When `left` and `right` are the
    /// same item, the gap is inside it and this is empty.
    pub between: Range<usize>,
}

impl TextMap {
    /// The mode offset at which each item starts, with the end as the last element.
    fn starts(&self, mode: Mode) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.items.len() + 1);
        let mut at = 0usize;
        for item in &self.items {
            out.push(at);
            if item.vis.in_mode(mode) {
                at += item.len as usize;
            }
        }
        out.push(at);
        out
    }

    /// The XML a range of the text covers in a mode. `None` if the range starts after it ends or goes
    /// past the end of the text.
    pub fn locate(&self, mode: Mode, range: Range<usize>) -> Option<Located> {
        let starts = self.starts(mode);
        let total = starts.last().copied().unwrap_or(0);
        if range.start > range.end || range.end > total {
            return None;
        }
        let mut spans = Vec::new();
        for (i, item) in self.items.iter().enumerate() {
            if item.len == 0 || !item.vis.in_mode(mode) {
                continue;
            }
            let from = starts.get(i).copied().unwrap_or(0);
            let to = from + item.len as usize;
            let (a, b) = (range.start.max(from), range.end.min(to));
            if a >= b {
                continue;
            }
            let base = item.from as usize;
            spans.push(Span {
                item: i,
                node: item.node,
                start: u32::try_from(base + (a - from)).unwrap_or(u32::MAX),
                end: u32::try_from(base + (b - from)).unwrap_or(u32::MAX),
                whole: a == from && b == to,
            });
        }
        let items = match (spans.first(), spans.last()) {
            (Some(first), Some(last)) => first.item..last.item + 1,
            _ => {
                let at = self.gap(mode, range.start).map_or(0, |g| g.between.start);
                at..at
            }
        };
        Some(Located { spans, items })
    }

    /// The insertion point between the characters at `offset - 1` and `offset` in a mode. `None` if the
    /// offset is past the end of the text.
    pub fn gap(&self, mode: Mode, offset: usize) -> Option<Gap> {
        let starts = self.starts(mode);
        if offset > starts.last().copied().unwrap_or(0) {
            return None;
        }
        let holds = |item: &Item| item.len > 0 && item.vis.in_mode(mode);
        let mut left = None;
        let mut right = None;
        for (i, item) in self.items.iter().enumerate() {
            if !holds(item) {
                continue;
            }
            let from = starts.get(i).copied().unwrap_or(0);
            let to = from + item.len as usize;
            if offset > from && offset <= to {
                left = Some(i);
            }
            if offset >= from && offset < to {
                right = Some(i);
            }
        }
        let n = self.items.len();
        let between = match (left, right) {
            (Some(l), Some(r)) if l == r => l + 1..l + 1,
            (Some(l), Some(r)) => l + 1..r,
            (Some(l), None) => l + 1..n,
            (None, Some(r)) => 0..r,
            (None, None) => 0..n,
        };
        Some(Gap {
            left,
            right,
            between,
        })
    }

    /// The offset in a mode's text of a position in the XML: a character position inside a text node, or
    /// an atom or a mark by its element (`in_node` is then ignored). For an item the mode excludes, the
    /// offset where it would sit. `None` if no item is made from the node.
    pub fn offset_of(&self, mode: Mode, node: NodeId, in_node: usize) -> Option<usize> {
        let i = self.items.iter().position(|item| item.node == node)?;
        let item = self.items.get(i)?;
        let before: usize = self
            .items
            .iter()
            .take(i)
            .filter(|x| x.vis.in_mode(mode))
            .map(|x| x.len as usize)
            .sum();
        if item.kind != super::text_model::ItemKind::Text || !item.vis.in_mode(mode) {
            return Some(before);
        }
        let inside = in_node
            .saturating_sub(item.from as usize)
            .min(item.len as usize);
        Some(before + inside)
    }

    /// The occurrences of `needle` in a mode's text, not overlapping, as character ranges. Byte
    /// offsets never leave this module. An empty needle has none.
    pub fn find_all(&self, mode: Mode, needle: &str) -> Vec<Range<usize>> {
        if needle.is_empty() {
            return Vec::new();
        }
        let text = self.text(mode);
        let width = needle.chars().count();
        let mut out = Vec::new();
        let (mut byte, mut chars) = (0usize, 0usize);
        for (at, _) in text.match_indices(needle) {
            chars += text.get(byte..at).map_or(0, |s| s.chars().count());
            byte = at;
            out.push(chars..chars + width);
        }
        out
    }

    /// The UTF-16 offset of a character offset in a mode's text. `None` if it is past the end.
    pub fn char_to_utf16(&self, mode: Mode, offset: usize) -> Option<usize> {
        let text = self.text(mode);
        let mut units = 0usize;
        let mut chars = text.chars();
        for _ in 0..offset {
            units += chars.next()?.len_utf16();
        }
        Some(units)
    }

    /// The character offset of a UTF-16 offset in a mode's text. `None` if it is past the end or
    /// between the two halves of a surrogate pair.
    pub fn utf16_to_char(&self, mode: Mode, offset: usize) -> Option<usize> {
        let text = self.text(mode);
        let (mut units, mut chars) = (0usize, 0usize);
        for c in text.chars() {
            if units == offset {
                return Some(chars);
            }
            if units > offset {
                return None;
            }
            units += c.len_utf16();
            chars += 1;
        }
        (units == offset).then_some(chars)
    }
}
