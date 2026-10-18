//! `changes()`: the revision elements of a story, grouped into the changes an agent sees (design
//! `revisions.md`, 6.1).
//!
//! An agent thinks "the deletion of that clause", not "the `w:del` with id 17", so the unit that `Document::
//! revisions` lists, that receipts report and that accept and reject by id act on is a **change**. The
//! grouping works on the items of each paragraph's text map in text order, not on XML siblings, because a
//! pending revision that someone split or nested into is no longer made of neighbours. The rules are in
//! `changes_text` (text, formatting) and here (marks, paragraphs, rows, moves, twins).

use super::changes_text::{Atoms, Key, run_format_groups, text_groups};
use crate::wordml::{Branch, NodeId, Paragraph, RevKind, Story};
use crate::xml::SpanTree;
use std::ops::Range;

/// What a change is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChangeKind {
    /// Text inserted.
    Insertion,
    /// Text deleted.
    Deletion,
    /// A move, both sides.
    Move,
    /// A whole paragraph inserted: its mark and its content, or an Enter and the text after it.
    ParagraphInserted,
    /// A whole paragraph deleted: its mark and its content.
    ParagraphDeleted,
    /// A paragraph mark inserted, with plain content (a split).
    MarkInserted,
    /// A paragraph mark deleted, with plain content (a merge).
    MarkDeleted,
    /// A row inserted, with what is inserted in its cells.
    RowInserted,
    /// A row deleted, with what is deleted in its cells.
    RowDeleted,
    /// A cell inserted.
    CellInserted,
    /// A cell deleted.
    CellDeleted,
    /// Cells merged.
    CellMerged,
    /// Character formatting changed.
    RunFormat,
    /// Paragraph formatting changed.
    ParagraphFormat,
    /// Section properties changed.
    SectionFormat,
    /// Table properties changed.
    TableFormat,
    /// Row properties changed.
    RowFormat,
    /// Cell properties changed.
    CellFormat,
    /// The table grid changed.
    GridChange,
    /// A numbering definition changed.
    NumberingChange,
}

/// One change: every revision element that belongs together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Change {
    /// What it is.
    pub kind: ChangeKind,
    /// Every id of the change, in document order. The ids of the untaken twins in a text box are here.
    pub ids: Vec<i64>,
    /// Every revision element of the change, in document order.
    pub nodes: Vec<NodeId>,
    /// The author of the first element.
    pub author: Option<String>,
    /// The date of the first element, as written.
    pub date: Option<String>,
    /// `Taken` or `Outside` unless every element is in an untaken branch.
    pub branch: Branch,
    /// The entries of the story that the change touches: the first to one past the last. A change that
    /// spans a row spans the entries of the row.
    pub blocks: Range<usize>,
    /// The entries that hold an element of the change, and those inside a row that the change is of, in
    /// order. A move from the third entry to the four hundredth has `blocks` of 3..401 and touches two
    /// entries, and a range of entries is touched by the changes that have one of these in it.
    pub touches: Vec<usize>,
    /// The text the change puts in, in document order: what its insertions and moved-in text hold, and a
    /// line break for each paragraph mark it inserts. At most [`CHANGE_TEXT_LIMIT`] characters, so a receipt
    /// can carry it.
    pub inserted: String,
    /// The text the change takes out, in the same way: what its deletions and moved-out text hold, and a line
    /// break for each paragraph mark it deletes.
    pub deleted: String,
    /// True if `inserted` or `deleted` was cut at the limit.
    pub text_cut: bool,
}

/// The most characters of inserted text, and of deleted text, that one [`Change`] carries.
pub(crate) const CHANGE_TEXT_LIMIT: usize = 1_000;

/// One group before it is a `Change`: the atoms that belong together.
#[derive(Clone, Debug)]
pub(super) struct Group {
    pub kind: ChangeKind,
    pub atoms: Vec<usize>,
    /// The entries the group spans, when it spans more than the entries of its atoms.
    pub span: Option<Range<usize>>,
}

/// The kind of a change that is one element on its own.
fn single_kind(k: RevKind) -> ChangeKind {
    match k {
        RevKind::Ins | RevKind::CustomXmlInsStart | RevKind::CustomXmlInsEnd => {
            ChangeKind::Insertion
        }
        RevKind::Del | RevKind::CustomXmlDelStart | RevKind::CustomXmlDelEnd => {
            ChangeKind::Deletion
        }
        RevKind::MoveFrom
        | RevKind::MoveTo
        | RevKind::MarkMoveFrom
        | RevKind::MarkMoveTo
        | RevKind::CustomXmlMoveFromStart
        | RevKind::CustomXmlMoveFromEnd
        | RevKind::CustomXmlMoveToStart
        | RevKind::CustomXmlMoveToEnd => ChangeKind::Move,
        RevKind::MarkIns => ChangeKind::MarkInserted,
        RevKind::MarkDel => ChangeKind::MarkDeleted,
        RevKind::RunProps => ChangeKind::RunFormat,
        RevKind::ParagraphProps | RevKind::NumPrIns => ChangeKind::ParagraphFormat,
        RevKind::SectionProps => ChangeKind::SectionFormat,
        RevKind::TableProps | RevKind::TableExProps => ChangeKind::TableFormat,
        RevKind::TableGrid => ChangeKind::GridChange,
        RevKind::RowProps => ChangeKind::RowFormat,
        RevKind::CellProps => ChangeKind::CellFormat,
        RevKind::NumberingChange => ChangeKind::NumberingChange,
        RevKind::RowIns => ChangeKind::RowInserted,
        RevKind::RowDel => ChangeKind::RowDeleted,
        RevKind::CellIns => ChangeKind::CellInserted,
        RevKind::CellDel => ChangeKind::CellDeleted,
        RevKind::CellMerge => ChangeKind::CellMerged,
    }
}

/// The changes of a story, in document order. With `range`, only the changes that touch those entries,
/// and a change that straddles the edge of the range is whole. A range costs what its blocks cost: the
/// window of entries around it is read, and grows while a change reaches its edge.
pub(crate) fn changes(story: &Story, tree: &SpanTree, range: Option<Range<usize>>) -> Vec<Change> {
    let n = story.blocks().len();
    let Some(range) = range else {
        return compute(story, tree, &Atoms::new(story, tree)).0;
    };
    let range = range.start.min(n)..range.end.min(n);
    if range.is_empty() {
        return Vec::new();
    }
    let hits = |all: Vec<Change>| -> Vec<Change> {
        all.into_iter()
            .filter(|c| c.touches.iter().any(|i| range.contains(i)))
            .collect()
    };
    let mut margin = 2usize;
    loop {
        let window = whole_tables(
            story,
            range.start.saturating_sub(margin)..(range.end + margin).min(n),
        );
        if window.start == 0 && window.end == n {
            return hits(compute(story, tree, &Atoms::new(story, tree)).0);
        }
        let (atoms, outside) = Atoms::in_window(story, tree, window.clone());
        let (all, global) = compute(story, tree, &atoms);
        if global || outside {
            // A move is one change by its name wherever its halves are, a control by its markers, and a
            // wrapper around blocks or a marker between them belongs to a block by position.
            return hits(compute(story, tree, &Atoms::new(story, tree)).0);
        }
        let found = hits(all);
        // Changes link consecutive blocks (an Enter and the text after it, the formatting of paragraphs in
        // a row). A change of the range that reaches either edge may go on outside the window, and so the
        // window grows. And a block at the start of the window that holds a revision may be a link in a
        // chain that began before the window, which the window has cut: what follows it then looks like
        // changes of their own, though no change of the range reaches the edge. A clean block cannot be a
        // link, and a chain only goes forward from a block, so the end of the window has no such case.
        let busy = |i: usize| atoms.by_block.get(&i).is_some_and(|v| !v.is_empty());
        let reaches_edge = (window.start > 0 && busy(window.start))
            || found.iter().any(|c| {
                let (first, last) = (c.touches.first(), c.touches.last());
                (first.is_some_and(|f| *f <= window.start) && window.start > 0)
                    || (last.is_some_and(|l| l + 1 >= window.end) && window.end < n)
            });
        if !reaches_edge {
            return found;
        }
        margin = margin.saturating_mul(4);
    }
}

/// The window widened to whole outermost tables: a row takes what is in its cells, so no window may cut
/// one.
fn whole_tables(story: &Story, window: Range<usize>) -> Range<usize> {
    let entries = story.blocks();
    let top = |mut i: usize| {
        while let Some(p) = entries.get(i).and_then(|e| e.parent) {
            i = p as usize;
        }
        i
    };
    let start = top(window.start).min(window.start);
    let last = window.end.saturating_sub(1).max(window.start);
    let end = super::changes_rows::subtree_end(story, top(last)).max(window.end);
    start..end.min(entries.len())
}

/// Groups the revision elements of a window into changes. The flag says that something in the window is
/// grouped by a name or an id that can be anywhere in the story (a move, a control), so the window is not
/// enough.
fn compute(story: &Story, tree: &SpanTree, atoms: &Atoms) -> (Vec<Change>, bool) {
    let mut claimed = vec![false; atoms.list.len()];
    let mut groups: Vec<Group> = Vec::new();
    let entries = story.blocks();
    let global = atoms.list.iter().any(|r| {
        matches!(
            r.kind,
            RevKind::MoveFrom
                | RevKind::MoveTo
                | RevKind::MarkMoveFrom
                | RevKind::MarkMoveTo
                | RevKind::CustomXmlInsStart
                | RevKind::CustomXmlInsEnd
                | RevKind::CustomXmlDelStart
                | RevKind::CustomXmlDelEnd
                | RevKind::CustomXmlMoveFromStart
                | RevKind::CustomXmlMoveFromEnd
                | RevKind::CustomXmlMoveToStart
                | RevKind::CustomXmlMoveToEnd
        )
    });
    // Rows first: they take the inserted or deleted content of their cells.
    super::changes_rows::row_groups(story, tree, atoms, &mut claimed, &mut groups);
    // Text groups of every paragraph, in entry order.
    let mut per_entry: Vec<(usize, Vec<Vec<usize>>)> = Vec::new();
    let mut formats: Vec<Vec<usize>> = Vec::new();
    for i in atoms.window.clone() {
        let Some(e) = entries.get(i) else { break };
        // A paragraph with no revision element in it has no map to read.
        if atoms.by_block.get(&i).is_none_or(Vec::is_empty) {
            continue;
        }
        if let Some(p) = Paragraph::new(tree, e.node) {
            let map = p.map();
            per_entry.push((i, text_groups(&map, atoms)));
            formats.extend(run_format_groups(&map, tree, atoms, e.node));
        }
    }
    for g in formats {
        take(&mut claimed, &mut groups, ChangeKind::RunFormat, g, None);
    }
    super::changes_marks::paragraph_groups(story, tree, atoms, &mut claimed, &mut groups);
    // What the marks did not take of the text groups.
    for (_, text) in per_entry {
        for g in text {
            let kind = g
                .first()
                .and_then(|&a| atoms.list.get(a))
                .map_or(ChangeKind::Insertion, |r| single_kind(r.kind));
            take(&mut claimed, &mut groups, kind, g, None);
        }
    }
    super::changes_marks::control_groups(atoms, &mut claimed, &mut groups);
    // Moves are one change per name, whatever side and wherever they are.
    if global {
        super::changes_marks::merge_moves(story, tree, atoms, &mut groups);
    }
    // Everything else is a change of its own, and a twin in an untaken branch joins the change of its taken
    // copy.
    for pass in 0..3 {
        for a in 0..atoms.list.len() {
            if claimed.get(a) != Some(&false) {
                continue;
            }
            let Some(r) = atoms.list.get(a) else { continue };
            let untaken = r.branch == Branch::Untaken;
            match pass {
                0 if !untaken => take(
                    &mut claimed,
                    &mut groups,
                    single_kind(r.kind),
                    vec![a],
                    None,
                ),
                1 if untaken => {
                    let twin = groups.iter().position(|g| {
                        g.atoms.iter().any(|&b| {
                            atoms.list.get(b).is_some_and(|o| {
                                o.branch != Branch::Untaken
                                    && o.kind == r.kind
                                    && o.id == r.id
                                    && o.author == r.author
                                    && o.date == r.date
                            })
                        })
                    });
                    if let Some(group) = twin.and_then(|g| groups.get_mut(g)) {
                        group.atoms.push(a);
                        if let Some(c) = claimed.get_mut(a) {
                            *c = true;
                        }
                    }
                }
                2 => take(
                    &mut claimed,
                    &mut groups,
                    single_kind(r.kind),
                    vec![a],
                    None,
                ),
                _ => {}
            }
        }
    }
    let mut out: Vec<Change> = groups
        .iter()
        .filter_map(|g| to_change(tree, atoms, g))
        .collect();
    out.sort_by_key(|c| atoms.first_index(&c.nodes));
    (out, global)
}

/// Claims the atoms for a new group.
pub(super) fn take(
    claimed: &mut [bool],
    groups: &mut Vec<Group>,
    kind: ChangeKind,
    mut atoms: Vec<usize>,
    span: Option<Range<usize>>,
) {
    atoms.retain(|&a| claimed.get(a) == Some(&false));
    for &a in &atoms {
        if let Some(c) = claimed.get_mut(a) {
            *c = true;
        }
    }
    if !atoms.is_empty() {
        groups.push(Group { kind, atoms, span });
    }
}

/// Adds the text below `node` to `out`, up to `room` more characters: `w:t` for an insertion, `w:delText` for a
/// deletion, tabs and line breaks as the characters they are, and nothing from a field's code, a text box
/// (its paragraphs are blocks of their own), the copy of a text box that `mc:Fallback` keeps, or a deletion
/// that someone else made inside an insertion. Returns true if the text was cut.
fn push_text(tree: &SpanTree, node: NodeId, deleted: bool, out: &mut String, room: usize) -> bool {
    let mut left = room;
    let mut stack = vec![tree.node_ref(node)];
    let skip_nested: &[&str] = if deleted { &[] } else { &["del", "moveFrom"] };
    while let Some(n) = stack.pop() {
        let piece: Option<String> = if !n.is_element() {
            None
        } else {
            match n.local_name() {
                "t" if !deleted => Some(n.text_content().to_string()),
                "delText" if deleted => Some(n.text_content().to_string()),
                "tab" => Some("\t".to_owned()),
                "br" | "cr" => Some("\n".to_owned()),
                "noBreakHyphen" => Some("-".to_owned()),
                _ => None,
            }
        };
        if let Some(text) = piece {
            for c in text.chars() {
                if left == 0 {
                    return true;
                }
                out.push(c);
                left -= 1;
            }
            continue;
        }
        if n.id() != node && n.is_element() {
            let name = n.local_name();
            if matches!(
                name,
                "txbxContent" | "Fallback" | "instrText" | "delInstrText"
            ) || skip_nested.contains(&name) && super::names::is_word_ns(n.namespace())
            {
                continue;
            }
        }
        let kids: Vec<_> = n.children().collect();
        stack.extend(kids.into_iter().rev());
    }
    false
}

/// The text a change puts in and takes out, bounded: see [`Change::inserted`].
fn change_text(tree: &SpanTree, atoms: &Atoms, idx: &[usize]) -> (String, String, bool) {
    let (mut inserted, mut deleted) = (String::new(), String::new());
    let mut cut = false;
    // In reading order: a paragraph mark is in the properties at the start of its paragraph in the XML, and
    // ends the paragraph in the text, so it goes after the text of the same paragraph.
    let mut order: Vec<usize> = idx.to_vec();
    order.sort_by_key(|&a| {
        let is_mark = atoms.list.get(a).is_some_and(|r| {
            matches!(
                r.kind,
                RevKind::MarkIns | RevKind::MarkDel | RevKind::MarkMoveFrom | RevKind::MarkMoveTo
            )
        });
        (atoms.block.get(a).copied().unwrap_or(0), is_mark, a)
    });
    for a in order {
        let Some(r) = atoms.list.get(a) else { continue };
        if r.branch == Branch::Untaken {
            continue;
        }
        let (into, is_deleted) = match r.kind {
            RevKind::Ins | RevKind::MoveTo => (&mut inserted, false),
            RevKind::Del | RevKind::MoveFrom => (&mut deleted, true),
            RevKind::MarkIns | RevKind::MarkMoveTo => {
                if inserted.chars().count() < CHANGE_TEXT_LIMIT {
                    inserted.push('\n');
                } else {
                    cut = true;
                }
                continue;
            }
            RevKind::MarkDel | RevKind::MarkMoveFrom => {
                if deleted.chars().count() < CHANGE_TEXT_LIMIT {
                    deleted.push('\n');
                } else {
                    cut = true;
                }
                continue;
            }
            _ => continue,
        };
        let room = CHANGE_TEXT_LIMIT.saturating_sub(into.chars().count());
        cut |= push_text(tree, r.node, is_deleted, into, room);
    }
    (inserted, deleted, cut)
}

/// The change a group makes.
fn to_change(tree: &SpanTree, atoms: &Atoms, g: &Group) -> Option<Change> {
    let mut idx = g.atoms.clone();
    idx.sort_unstable();
    idx.dedup();
    let first = atoms.list.get(*idx.first()?)?;
    let mut ids: Vec<i64> = Vec::new();
    for &a in &idx {
        if let Some(id) = atoms.list.get(a).and_then(|r| r.id)
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    let nodes: Vec<NodeId> = idx
        .iter()
        .filter_map(|&a| atoms.list.get(a).map(|r| r.node))
        .collect();
    let owners: Vec<usize> = idx
        .iter()
        .filter_map(|&a| atoms.block.get(a).copied())
        .collect();
    let (lo, hi) = (owners.iter().min().copied()?, owners.iter().max().copied()?);
    let mut blocks = lo..hi + 1;
    let mut touches = owners;
    if let Some(s) = &g.span {
        blocks = blocks.start.min(s.start)..blocks.end.max(s.end);
        touches.extend(s.clone());
    }
    touches.sort_unstable();
    touches.dedup();
    let all_untaken = idx.iter().all(|&a| {
        atoms
            .list
            .get(a)
            .is_some_and(|r| r.branch == Branch::Untaken)
    });
    let any_taken = idx
        .iter()
        .any(|&a| atoms.list.get(a).is_some_and(|r| r.branch == Branch::Taken));
    let (inserted, deleted, text_cut) = change_text(tree, atoms, &idx);
    Some(Change {
        kind: g.kind,
        ids,
        nodes,
        author: first.author.clone(),
        date: first.date.clone(),
        branch: if all_untaken {
            Branch::Untaken
        } else if any_taken {
            Branch::Taken
        } else {
            Branch::Outside
        },
        blocks,
        touches,
        inserted,
        deleted,
        text_cut,
    })
}

/// The key of an atom: what makes two pieces one change.
pub(super) fn key_of(atoms: &Atoms, a: usize) -> Option<Key> {
    atoms.key(a)
}
