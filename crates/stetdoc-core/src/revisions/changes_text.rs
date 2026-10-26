//! The revision elements of a story as atoms, and the groups of them that make one change inside a
//! paragraph: pieces of text under revision wrappers, and formatting changes of consecutive runs.

use crate::wordml::{
    BlockKind, Branch, NodeId, RevKind, Revision, Story, TextMap, WrapperKind, revision_at,
};
use crate::xml::{NodeRef, SpanTree};
use std::collections::HashMap;
use std::ops::Range;

/// What makes two atoms one change: the side of the revision and the author and date as written.
pub(super) type Key = (RevKind, Option<String>, Option<String>);

/// Every revision element of a story, or of a window of its entries, in document order, with the entry
/// that owns it.
pub(super) struct Atoms {
    pub list: Vec<Revision>,
    /// The entry of the story that holds each element: the nearest block above it.
    pub block: Vec<usize>,
    pub by_node: HashMap<NodeId, usize>,
    /// The atoms of each entry.
    pub by_block: HashMap<usize, Vec<usize>>,
    /// The `rPrChange` atom of each run that has one.
    pub by_run: HashMap<NodeId, usize>,
    /// The entries the atoms were read from.
    pub window: Range<usize>,
}

/// Which branch of an `mc:AlternateContent` an element is in, looking at every ancestor: in an untaken
/// branch anywhere above makes it untaken.
fn branch_of(n: &NodeRef<'_>) -> Branch {
    let is_branch = |x: &NodeRef<'_>| {
        x.is_element()
            && x.namespace() == crate::xml::MC_NS
            && matches!(x.local_name(), "Choice" | "Fallback")
    };
    let mut branch = Branch::Outside;
    let mut child = *n;
    while let Some(parent) = child.parent() {
        if is_branch(&parent)
            && let Some(ac) = parent.parent()
        {
            if crate::schema::taken_branch(&ac).is_some_and(|t| t == parent) {
                branch = Branch::Taken;
            } else {
                return Branch::Untaken;
            }
        }
        child = parent;
    }
    branch
}

/// The revision elements at or below a node, in document order, not entering a block that is an entry of
/// the story (the blocks inside have their own entries).
fn revisions_below(story: &Story, root: &NodeRef<'_>, keep_root: bool, out: &mut Vec<Revision>) {
    let mut stack = vec![*root];
    while let Some(n) = stack.pop() {
        if (keep_root || n.id() != root.id())
            && let Some(mut r) = revision_at(&n)
        {
            r.branch = branch_of(&n);
            out.push(r);
        }
        let kids: Vec<NodeRef<'_>> = n
            .child_elements()
            .filter(|c| story.index_of(c.id()).is_none())
            .collect();
        stack.extend(kids.into_iter().rev());
    }
}

/// The first entry of the story that is `node` or inside it, in document order.
fn first_entry_in(story: &Story, node: &NodeRef<'_>) -> Option<usize> {
    let mut stack = vec![*node];
    while let Some(n) = stack.pop() {
        if let Some(i) = story.index_of(n.id()) {
            return Some(i);
        }
        let kids: Vec<NodeRef<'_>> = n.child_elements().collect();
        stack.extend(kids.into_iter().rev());
    }
    None
}

/// The entry that owns a node: the nearest block above it. A node that no block holds, a wrapper around
/// blocks or a marker between them, belongs to the first block at or after it, and one at the end of the
/// story (the section properties of the body) to the last block.
pub(super) fn owner_entry(story: &Story, node: &NodeRef<'_>) -> usize {
    let mut cur = Some(*node);
    while let Some(n) = cur {
        if let Some(i) = story.index_of(n.id()) {
            return i;
        }
        cur = n.parent();
    }
    if let Some(i) = first_entry_in(story, node) {
        return i;
    }
    let mut at = *node;
    loop {
        let mut next = at.next_sibling();
        while let Some(s) = next {
            if s.is_element()
                && let Some(i) = first_entry_in(story, &s)
            {
                return i;
            }
            next = s.next_sibling();
        }
        match at.parent() {
            Some(p) => at = p,
            None => return story.blocks().len().saturating_sub(1),
        }
    }
}

/// The elements that hold a paragraph's inline content, above which the blocks of a text box begin: a
/// block's neighbours are looked at inside block containers only.
fn is_inline_container(n: &NodeRef<'_>) -> bool {
    matches!(
        n.local_name(),
        "r" | "p"
            | "pict"
            | "drawing"
            | "object"
            | "txbx"
            | "AlternateContent"
            | "Choice"
            | "Fallback"
    )
}

/// True if a revision element that no block holds is next to a block: a wrapper around it, or a marker
/// between it and the blocks beside it. Such an element belongs to a block by position (`owner_entry`),
/// so a window that has the block cannot know it without looking.
fn outside_atoms_near(story: &Story, node: &NodeRef<'_>) -> bool {
    let mut level = *node;
    loop {
        let Some(parent) = level.parent() else {
            return false;
        };
        if story.index_of(parent.id()).is_some() {
            // The block that holds this one: a row, a table, or the paragraph that has a text box. A
            // wrapper between them is an element of the block above, unless that is a row or a table.
            return false;
        }
        if is_inline_container(&parent) {
            return false;
        }
        if revision_at(&parent).is_some() {
            return true;
        }
        for forward in [false, true] {
            let mut sib = if forward {
                level.next_sibling()
            } else {
                level.prev_sibling()
            };
            while let Some(s) = sib {
                if s.is_element() {
                    if story.index_of(s.id()).is_some() {
                        break;
                    }
                    let mut found = Vec::new();
                    revisions_below(story, &s, true, &mut found);
                    if !found.is_empty() {
                        return true;
                    }
                    if first_entry_in(story, &s).is_some() {
                        break;
                    }
                }
                sib = if forward {
                    s.next_sibling()
                } else {
                    s.prev_sibling()
                };
            }
        }
        level = parent;
    }
}

impl Atoms {
    /// Reads the revisions of a whole story.
    pub(super) fn new(story: &Story, tree: &SpanTree) -> Atoms {
        let list = story.revisions(tree);
        let owners: Vec<usize> = list
            .iter()
            .map(|r| owner_entry(story, &tree.node_ref(r.node)))
            .collect();
        Atoms::build(tree, list, owners, 0..story.blocks().len())
    }

    /// Reads the revisions of the entries of a window: a paragraph whole, a row for its own properties
    /// and those of its cells, and a table for its own, because the blocks inside have entries of their
    /// own. The flag says that a revision element sits between blocks next to the window, which the
    /// window does not read and which belongs to a block by position: the result is not the story's.
    pub(super) fn in_window(story: &Story, tree: &SpanTree, window: Range<usize>) -> (Atoms, bool) {
        let entries = story.blocks();
        let mut list: Vec<Revision> = Vec::new();
        let mut owners: Vec<usize> = Vec::new();
        let mut outside = false;
        for i in window.clone() {
            let Some(e) = entries.get(i) else { break };
            let node = tree.node_ref(e.node);
            let mut here: Vec<Revision> = Vec::new();
            match e.kind {
                BlockKind::Paragraph => revisions_below(story, &node, true, &mut here),
                BlockKind::Row => {
                    for c in node.child_elements() {
                        match c.local_name() {
                            "tc" => {
                                for pr in c.child_elements().filter(|x| x.local_name() == "tcPr") {
                                    revisions_below(story, &pr, true, &mut here);
                                }
                            }
                            "trPr" | "tblPrEx" => revisions_below(story, &c, true, &mut here),
                            _ => {}
                        }
                    }
                }
                BlockKind::Table => {
                    for c in node
                        .child_elements()
                        .filter(|c| matches!(c.local_name(), "tblPr" | "tblGrid"))
                    {
                        revisions_below(story, &c, true, &mut here);
                    }
                }
            }
            outside = outside || outside_atoms_near(story, &node);
            owners.extend(std::iter::repeat_n(i, here.len()));
            list.extend(here);
        }
        (Atoms::build(tree, list, owners, window), outside)
    }

    /// Indexes a list of revisions with their owners.
    fn build(
        tree: &SpanTree,
        list: Vec<Revision>,
        block: Vec<usize>,
        window: Range<usize>,
    ) -> Atoms {
        let mut by_node = HashMap::new();
        let mut by_block: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut by_run = HashMap::new();
        for (a, r) in list.iter().enumerate() {
            by_node.insert(r.node, a);
            if let Some(&owner) = block.get(a) {
                by_block.entry(owner).or_default().push(a);
            }
            if r.kind == RevKind::RunProps
                && let Some(run) = tree.node_ref(r.node).parent().and_then(|p| p.parent())
                && run.local_name() == "r"
            {
                by_run.insert(run.id(), a);
            }
        }
        Atoms {
            list,
            block,
            by_node,
            by_block,
            by_run,
            window,
        }
    }

    /// The side, author and date of an atom, for the revisions that insert or delete: the wrapper, the
    /// paragraph mark, the row and the cell. `None` for the others.
    pub(super) fn key(&self, a: usize) -> Option<Key> {
        let r = self.list.get(a)?;
        let side = match r.kind {
            RevKind::Ins
            | RevKind::MarkIns
            | RevKind::RowIns
            | RevKind::CellIns
            | RevKind::NumPrIns => RevKind::Ins,
            RevKind::Del | RevKind::MarkDel | RevKind::RowDel | RevKind::CellDel => RevKind::Del,
            RevKind::MoveFrom | RevKind::MarkMoveFrom => RevKind::MoveFrom,
            RevKind::MoveTo | RevKind::MarkMoveTo => RevKind::MoveTo,
            _ => return None,
        };
        Some((side, r.author.clone(), r.date.clone()))
    }

    /// The smallest atom index among nodes: where a change starts in document order.
    pub(super) fn first_index(&self, nodes: &[NodeId]) -> usize {
        nodes
            .iter()
            .filter_map(|n| self.by_node.get(n).copied())
            .min()
            .unwrap_or(usize::MAX)
    }
}

/// True for the wrappers that are revisions of text.
fn revision_wrapper(k: WrapperKind) -> bool {
    matches!(
        k,
        WrapperKind::Ins | WrapperKind::Del | WrapperKind::MoveFrom | WrapperKind::MoveTo
    )
}

/// For each item, whether it has width, and the atoms of the revision wrappers around it. An item with no
/// width (a field's `begin` or its code) says nothing about whether a paragraph is wholly under a revision,
/// but a revision around it is still one of the paragraph's.
pub(super) fn item_wrappers(map: &TextMap, atoms: &Atoms) -> Vec<(bool, Vec<usize>)> {
    map.items()
        .iter()
        .map(|i| {
            let wrappers = map
                .context(i.ctx)
                .filter(|c| revision_wrapper(c.kind))
                .filter_map(|c| atoms.by_node.get(&c.node).copied())
                .collect();
            (i.len > 0, wrappers)
        })
        .collect()
}

/// The pieces of text under revision wrappers, grouped: two pieces of one side, author and date are one
/// change when everything between them, in item order, is zero-width or is under another revision. A
/// plain item with width ends every open group.
pub(super) fn text_groups(map: &TextMap, atoms: &Atoms) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut open: Vec<(Key, usize)> = Vec::new();
    for item in map.items() {
        let wrappers: Vec<usize> = map
            .context(item.ctx)
            .filter(|c| revision_wrapper(c.kind))
            .filter_map(|c| atoms.by_node.get(&c.node).copied())
            .collect();
        if wrappers.is_empty() {
            if item.len > 0 {
                open.clear();
            }
            continue;
        }
        for a in wrappers {
            let Some(key) = atoms.key(a) else { continue };
            match open.iter().find(|(k, _)| *k == key).map(|(_, g)| *g) {
                Some(g) => {
                    if let Some(group) = groups.get_mut(g)
                        && !group.contains(&a)
                    {
                        group.push(a);
                    }
                }
                None => {
                    groups.push(vec![a]);
                    open.push((key, groups.len() - 1));
                }
            }
        }
    }
    groups
}

/// A string that is equal for two property changes with the same old properties: the author, the date and
/// the children of the change, without ids.
pub(super) fn snapshot_key(tree: &SpanTree, change: NodeId) -> String {
    fn walk(n: &NodeRef<'_>, out: &mut String) {
        if !n.is_element() {
            out.push_str(&n.text());
            return;
        }
        out.push('<');
        out.push_str(n.namespace());
        out.push('|');
        out.push_str(n.local_name());
        let mut attrs: Vec<(String, String, String)> = n
            .attrs()
            .filter(|a| !a.is_namespace_declaration())
            .map(|a| {
                (
                    a.namespace().to_owned(),
                    a.local_name().to_owned(),
                    a.value().into_owned(),
                )
            })
            .collect();
        attrs.sort();
        for (ns, l, v) in attrs {
            out.push_str(&format!(" {ns}|{l}={v}"));
        }
        out.push('>');
        for c in n.children() {
            walk(&c, out);
        }
        out.push_str("</>");
    }
    let n = tree.node_ref(change);
    let mut s = String::new();
    for a in ["author", "date"] {
        s.push_str(&super::names::w_attr(&n, a).unwrap_or_default());
        s.push('|');
    }
    for c in n.children() {
        walk(&c, &mut s);
    }
    s
}

/// The `rPrChange` atoms of consecutive runs with the same author, date and old properties, grouped: a
/// carve that copies a pending change under a new id does not split a change.
pub(super) fn run_format_groups(
    map: &TextMap,
    tree: &SpanTree,
    atoms: &Atoms,
    _para: NodeId,
) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut current: Option<(String, usize)> = None;
    for item in map.items().iter().filter(|i| i.len > 0) {
        let atom = item.run.and_then(|r| atoms.by_run.get(&r).copied());
        let Some(a) = atom else {
            current = None;
            continue;
        };
        let Some(r) = atoms.list.get(a) else { continue };
        let key = snapshot_key(tree, r.node);
        match &current {
            Some((k, g)) if *k == key => {
                if let Some(group) = groups.get_mut(*g)
                    && !group.contains(&a)
                {
                    group.push(a);
                }
            }
            _ => {
                groups.push(vec![a]);
                current = Some((key, groups.len() - 1));
            }
        }
    }
    groups
}
