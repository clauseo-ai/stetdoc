//! What a table or a row owns itself, apart from the blocks nested in it.
//!
//! A table's bytes contain its rows and a row's its cells, so comparing a table's bytes before and
//! after an edit would call every cell edit an edit of the table and of the row. `Story::own_parts`
//! splits a block into the nodes whose own bytes count and the list of what is nested in it.
//! `docs/design/wordml.md` 3.6.

use super::mc;
use super::names::is_w;
use super::story::{BlockKind, Story};
use crate::xml::{NodeId, NodeRef, SpanTree};

/// Elements that only group what they contain and are read through to find rows and cells, as is the
/// branch of an `mc:AlternateContent` that Word reads.
const GROUPS: [&str; 8] = [
    "sdt",
    "sdtContent",
    "customXml",
    "smartTag",
    "ins",
    "del",
    "moveFrom",
    "moveTo",
];

/// What a table or a row owns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnParts {
    /// The nodes whose bytes (the node and all it contains) count as the block's own. For a table:
    /// `w:tblPr`, `w:tblGrid` and every other child that is not a row. For a row: `w:trPr`,
    /// `w:tblPrEx`, the `w:tcPr` of each cell and every other child that is not a cell.
    pub nodes: Vec<NodeId>,
    /// What is nested in the block, in order: the rows of a table, the cells of a row. A reorder,
    /// an insertion or a removal changes the list.
    pub list: Vec<NodeId>,
}

/// Splits the children of `parent` into what it owns and the elements named `target`, reading
/// through grouping elements.
fn split(parent: &NodeRef<'_>, target: &str) -> (Vec<NodeId>, Vec<NodeId>) {
    let (mut nodes, mut list) = (Vec::new(), Vec::new());
    let mut stack = vec![parent.children()];
    while let Some(top) = stack.last_mut() {
        let Some(c) = top.next() else {
            stack.pop();
            continue;
        };
        if !c.is_element() {
            continue;
        }
        if is_w(&c, target) {
            list.push(c.id());
        } else if GROUPS.iter().any(|g| is_w(&c, g)) {
            stack.push(c.children());
        } else if let mc::Mc::Branch(branch) = mc::disposition(&c) {
            stack.push(branch.children());
        } else if matches!(mc::disposition(&c), mc::Mc::ProcessContent) {
            stack.push(c.children());
        } else {
            nodes.push(c.id());
        }
    }
    (nodes, list)
}

impl Story {
    /// What the table or row at entry `i` owns itself. `None` for a paragraph, which owns all of its
    /// bytes, and for an entry that does not exist.
    pub fn own_parts(&self, tree: &SpanTree, i: usize) -> Option<OwnParts> {
        let entry = self.entries.get(i)?;
        let node = tree.node_ref(entry.node);
        match entry.kind {
            BlockKind::Paragraph => None,
            BlockKind::Table => {
                let (nodes, list) = split(&node, "tr");
                Some(OwnParts { nodes, list })
            }
            BlockKind::Row => {
                let (mut nodes, list) = split(&node, "tc");
                for cell in &list {
                    let cell = tree.node_ref(*cell);
                    let props = cell
                        .child_elements()
                        .filter(|c| is_w(c, "tcPr"))
                        .map(|c| c.id());
                    nodes.extend(props);
                }
                Some(OwnParts { nodes, list })
            }
        }
    }
}
