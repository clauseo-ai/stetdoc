//! What the new text of a replacement stays inside (design `revisions.md`, 4.2). Typing over a selection in Word puts the
//! new text where the selection was: a selection that starts inside a hyperlink, a content control, a bookmark, a
//! permission range or a comment's range (or at its start) and ends at its end is replaced inside it. The gap rule alone
//! would put the text after the element's end, as it does for a pure insertion there, so a replacement names the elements
//! that hold its whole selection, and `settle` keeps the boundary inside them.

use super::compiler::ParaView;
use super::names::w_attr;
use crate::wordml::{ItemKind, Mode, NodeId, WrapperKind};
use crate::xml::SpanTree;
use std::ops::Range;

/// The elements that hold a replacement's whole selection, which its new text stays inside.
#[derive(Clone, Debug, Default)]
pub(super) struct Keep {
    /// Hyperlinks, smart tags, custom XML elements, directional wrappers and content controls (the `w:sdt`).
    pub(super) containers: Vec<NodeId>,
    /// The range ends right after the selection whose start is before it: `w:bookmarkEnd`, `w:permEnd` and
    /// `w:commentRangeEnd`.
    pub(super) ends: Vec<NodeId>,
}

impl Keep {
    /// True if a boundary settled for new text stays inside `container`.
    pub(super) fn holds(&self, container: NodeId) -> bool {
        self.containers.contains(&container)
    }

    /// True if a boundary settled for new text stops before the range end `marker`.
    pub(super) fn stops_at(&self, marker: NodeId) -> bool {
        self.ends.contains(&marker)
    }
}

/// The elements that hold the characters `selection` of the paragraph's `Final` text. Empty for an empty selection, so a
/// pure insertion keeps the gap rule: at an element's end it goes after the element.
pub(super) fn keep_for(tree: &SpanTree, p: &ParaView, selection: Range<usize>) -> Keep {
    let mut keep = Keep::default();
    if selection.is_empty() {
        return keep;
    }
    let map = &p.map;
    let Some(loc) = map.locate(Mode::Final, selection) else {
        return keep;
    };
    let (Some(first), Some(last)) = (loc.spans.first(), loc.spans.last()) else {
        return keep;
    };
    let (first, last) = (first.item, last.item);
    let items = map.items();
    let container = |k: WrapperKind| {
        matches!(
            k,
            WrapperKind::Hyperlink
                | WrapperKind::Control
                | WrapperKind::SmartTag
                | WrapperKind::CustomXml
                | WrapperKind::Dir
                | WrapperKind::Bdo
        )
    };
    if let (Some(a), Some(b)) = (items.get(first), items.get(last)) {
        let at_end: Vec<NodeId> = map
            .context(b.ctx)
            .filter(|c| container(c.kind))
            .map(|c| c.node)
            .collect();
        keep.containers = map
            .context(a.ctx)
            .filter(|c| container(c.kind) && at_end.contains(&c.node))
            .map(|c| c.node)
            .collect();
    }
    // The range ends between the selection's last character and the next character of `Final`.
    let id_of = |node: NodeId| w_attr(&tree.node_ref(node), "id").map(|v| v.into_owned());
    for item in items.iter().skip(last + 1) {
        if item.len > 0 && item.vis.in_mode(Mode::Final) {
            break;
        }
        let start_kind = match item.kind {
            ItemKind::BookmarkEnd => ItemKind::BookmarkStart,
            ItemKind::PermEnd => ItemKind::PermStart,
            ItemKind::CommentEnd => ItemKind::CommentStart,
            _ => continue,
        };
        let Some(id) = id_of(item.node) else {
            continue;
        };
        // The range holds the whole selection if it starts before the selection's first character: in an earlier
        // paragraph, or in this one before that character. A start inside the selection, or after it, does not.
        let start = items
            .iter()
            .position(|s| s.kind == start_kind && id_of(s.node).as_deref() == Some(id.as_str()));
        if start.is_none_or(|s| s < first) {
            keep.ends.push(item.node);
        }
    }
    keep
}
