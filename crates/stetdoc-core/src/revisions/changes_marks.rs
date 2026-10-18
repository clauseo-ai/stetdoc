//! Paragraphs as changes: a mark and the content that goes with it, an Enter and the text after it,
//! formatting changes of consecutive paragraphs, and moves by name (design `revisions.md`, 6.1, rules 4,
//! 5 and 7).

use super::changes::{ChangeKind, Group, take};
use super::changes_text::{Atoms, Key, item_wrappers, snapshot_key};
use crate::wordml::{BlockKind, NodeId, Paragraph, RevKind, Story};
use crate::xml::SpanTree;
use std::collections::HashMap;

/// True for a mark revision.
fn is_mark(k: RevKind) -> bool {
    matches!(
        k,
        RevKind::MarkIns | RevKind::MarkDel | RevKind::MarkMoveFrom | RevKind::MarkMoveTo
    )
}

/// The change kind of a whole paragraph for a side.
fn whole(side: RevKind) -> ChangeKind {
    match side {
        RevKind::Ins => ChangeKind::ParagraphInserted,
        RevKind::Del => ChangeKind::ParagraphDeleted,
        _ => ChangeKind::Move,
    }
}

/// The wrapper atoms of the items that have `key` among their wrappers, and whether every item with width
/// has it.
fn covered(wrappers: &[Vec<usize>], atoms: &Atoms, key: &Key) -> (Vec<usize>, bool) {
    let mut out = Vec::new();
    let mut all = true;
    for item in wrappers {
        let mine: Vec<usize> = item
            .iter()
            .copied()
            .filter(|&a| atoms.key(a).as_ref() == Some(key))
            .collect();
        if mine.is_empty() {
            all = false;
        }
        out.extend(mine);
    }
    (out, all)
}

/// The leading items of a paragraph that have `key` among their wrappers: their atoms, and whether they are
/// all the items with width.
fn leading(wrappers: &[Vec<usize>], atoms: &Atoms, key: &Key) -> (Vec<usize>, bool) {
    let mut out = Vec::new();
    for item in wrappers {
        let mine: Vec<usize> = item
            .iter()
            .copied()
            .filter(|&a| atoms.key(a).as_ref() == Some(key))
            .collect();
        if mine.is_empty() {
            return (out, false);
        }
        out.extend(mine);
    }
    (out, true)
}

/// What the grouping reads: the story, its tree and the atoms.
struct Read<'a> {
    story: &'a Story,
    tree: &'a SpanTree,
    atoms: &'a Atoms,
}

impl Read<'_> {
    /// The element that holds the block at an entry.
    fn parent_node(&self, i: usize) -> Option<NodeId> {
        self.story
            .blocks()
            .get(i)
            .and_then(|e| self.tree.node_ref(e.node).parent().map(|p| p.id()))
    }
}

/// Forms the changes of paragraph marks, and the formatting changes of consecutive paragraphs.
pub(super) fn paragraph_groups(
    story: &Story,
    tree: &SpanTree,
    atoms: &Atoms,
    claimed: &mut [bool],
    groups: &mut Vec<Group>,
) {
    let entries = story.blocks();
    let read = Read { story, tree, atoms };
    for i in atoms.window.clone() {
        let Some(e) = entries.get(i) else { break };
        if e.kind != BlockKind::Paragraph {
            continue;
        }
        let marks: Vec<usize> = atoms
            .by_block
            .get(&i)
            .map(|v| {
                v.iter()
                    .copied()
                    .filter(|&a| atoms.list.get(a).is_some_and(|r| is_mark(r.kind)))
                    .collect()
            })
            .unwrap_or_default();
        if marks.is_empty() {
            continue;
        }
        let Some(para) = Paragraph::new(tree, e.node) else {
            continue;
        };
        let wrappers = item_wrappers(&para.map(), atoms);
        for m in marks {
            if claimed.get(m) != Some(&false) {
                continue;
            }
            let Some(key) = atoms.key(m) else { continue };
            let (own, all) = covered(&wrappers, atoms, &key);
            if all {
                let mut members = vec![m];
                members.extend(own);
                take(claimed, groups, whole(key.0), members, None);
                continue;
            }
            if key.0 == RevKind::Ins {
                enter_pair(&read, claimed, groups, (i, m), &key);
                continue;
            }
            let kind = match key.0 {
                RevKind::Del => ChangeKind::MarkDeleted,
                _ => ChangeKind::Move,
            };
            take(claimed, groups, kind, vec![m], None);
        }
    }
    format_groups(story, tree, atoms, claimed, groups);
}

/// An inserted mark with plain content, and the inserted content that starts the next paragraph: one change.
/// If the next paragraph is nothing but that content and has an inserted mark of its own, that mark and the
/// content that starts the paragraph after it are part of the change too.
fn enter_pair(
    read: &Read<'_>,
    claimed: &mut [bool],
    groups: &mut Vec<Group>,
    (first, mark): (usize, usize),
    key: &Key,
) {
    let (story, tree, atoms) = (read.story, read.tree, read.atoms);
    let entries = story.blocks();
    let mut members = vec![mark];
    let mut i = first;
    let mut next_mark = Some(mark);
    while next_mark.is_some() {
        let j = i + 1;
        let Some(e) = entries.get(j) else { break };
        if e.kind != BlockKind::Paragraph || read.parent_node(j) != read.parent_node(i) {
            break;
        }
        let Some(para) = Paragraph::new(tree, e.node) else {
            break;
        };
        let wrappers = item_wrappers(&para.map(), atoms);
        let (lead, everything) = leading(&wrappers, atoms, key);
        if lead.is_empty() {
            break;
        }
        members.extend(lead);
        next_mark = None;
        if everything {
            next_mark = atoms
                .by_block
                .get(&j)
                .into_iter()
                .flatten()
                .copied()
                .find(|&a| {
                    atoms
                        .list
                        .get(a)
                        .is_some_and(|r| r.kind == RevKind::MarkIns)
                        && atoms.key(a).as_ref() == Some(key)
                        && claimed.get(a) == Some(&false)
                });
            members.extend(next_mark);
        }
        i = j;
    }
    let kind = if members.len() > 1 {
        ChangeKind::ParagraphInserted
    } else {
        ChangeKind::MarkInserted
    };
    take(claimed, groups, kind, members, None);
}

/// The `pPrChange` of consecutive paragraphs with the same author, date and old properties are one
/// `ParagraphFormat` change, and an inserted list number joins the paragraph's own change.
fn format_groups(
    story: &Story,
    tree: &SpanTree,
    atoms: &Atoms,
    claimed: &mut [bool],
    groups: &mut Vec<Group>,
) {
    let mut current: Option<(String, usize)> = None;
    for i in atoms.window.clone() {
        let Some(e) = story.blocks().get(i) else {
            break;
        };
        let change = (e.kind == BlockKind::Paragraph)
            .then(|| {
                atoms
                    .by_block
                    .get(&i)
                    .into_iter()
                    .flatten()
                    .copied()
                    .find(|&a| {
                        atoms
                            .list
                            .get(a)
                            .is_some_and(|r| r.kind == RevKind::ParagraphProps)
                    })
            })
            .flatten();
        let Some(a) = change.filter(|&a| claimed.get(a) == Some(&false)) else {
            current = None;
            continue;
        };
        let Some(r) = atoms.list.get(a) else { continue };
        let key = snapshot_key(tree, r.node);
        let numbering: Vec<usize> = atoms
            .by_block
            .get(&i)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&n| {
                atoms.list.get(n).is_some_and(|x| {
                    x.kind == RevKind::NumPrIns && x.author == r.author && x.date == r.date
                })
            })
            .collect();
        match &current {
            Some((k, g)) if *k == key => {
                if let Some(group) = groups.get_mut(*g) {
                    group.atoms.push(a);
                    group.atoms.extend(numbering.iter().copied());
                }
                for x in std::iter::once(a).chain(numbering) {
                    if let Some(c) = claimed.get_mut(x) {
                        *c = true;
                    }
                }
            }
            _ => {
                let mut members = vec![a];
                members.extend(numbering);
                take(claimed, groups, ChangeKind::ParagraphFormat, members, None);
                current = Some((key, groups.len().saturating_sub(1)));
            }
        }
    }
}

/// Merges the moves that have one name, from and to, into one change. A wrapper's move is the one whose
/// range markers surround it (`moves::ranges_of`).
pub(super) fn merge_moves(story: &Story, tree: &SpanTree, atoms: &Atoms, groups: &mut Vec<Group>) {
    let wrappers: Vec<NodeId> = atoms
        .list
        .iter()
        .filter(|r| matches!(r.kind, RevKind::MoveFrom | RevKind::MoveTo))
        .map(|r| r.node)
        .collect();
    let ranges = super::moves::ranges_of(story, tree, &wrappers);
    let name_of = |g: &Group| -> Option<String> {
        g.atoms.iter().find_map(|&a| {
            let r = atoms.list.get(a)?;
            ranges.get(&r.node).map(|m| m.name.clone())
        })
    };
    let mut by_name: HashMap<String, usize> = HashMap::new();
    let mut merged: Vec<Group> = Vec::new();
    for g in std::mem::take(groups) {
        let name = (g.kind == ChangeKind::Move).then(|| name_of(&g)).flatten();
        match name.as_ref().and_then(|n| by_name.get(n).copied()) {
            Some(i) => {
                if let Some(target) = merged.get_mut(i) {
                    target.atoms.extend(g.atoms);
                }
            }
            None => {
                if let Some(n) = name {
                    by_name.insert(n, merged.len());
                }
                merged.push(g);
            }
        }
    }
    *groups = merged;
}

/// The side of a revision of a custom XML range marker, and whether it starts the range.
fn custom_side(k: RevKind) -> Option<(RevKind, bool)> {
    Some(match k {
        RevKind::CustomXmlInsStart => (RevKind::Ins, true),
        RevKind::CustomXmlInsEnd => (RevKind::Ins, false),
        RevKind::CustomXmlDelStart => (RevKind::Del, true),
        RevKind::CustomXmlDelEnd => (RevKind::Del, false),
        RevKind::CustomXmlMoveFromStart => (RevKind::MoveFrom, true),
        RevKind::CustomXmlMoveFromEnd => (RevKind::MoveFrom, false),
        RevKind::CustomXmlMoveToStart => (RevKind::MoveTo, true),
        RevKind::CustomXmlMoveToEnd => (RevKind::MoveTo, false),
        _ => return None,
    })
}

/// The side and the id that pair the two markers of a control.
type ControlKey = (RevKind, Option<i64>);

/// The two markers that surround a content control, a start and the end with the same id, are one change.
pub(super) fn control_groups(atoms: &Atoms, claimed: &mut [bool], groups: &mut Vec<Group>) {
    let mut by_key: Vec<(ControlKey, Vec<usize>)> = Vec::new();
    for (a, r) in atoms.list.iter().enumerate() {
        let Some((side, _)) = custom_side(r.kind) else {
            continue;
        };
        let key = (side, r.id);
        match by_key.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(a),
            None => by_key.push((key, vec![a])),
        }
    }
    for ((side, _), members) in by_key {
        let kind = match side {
            RevKind::Ins => ChangeKind::Insertion,
            RevKind::Del => ChangeKind::Deletion,
            _ => ChangeKind::Move,
        };
        take(claimed, groups, kind, members, None);
    }
}
