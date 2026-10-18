//! The plan: what `build` does to the revised document, as data (design `compare.md`, 5).
//!
//! The pairing says which blocks go together. The plan turns it into entries, in document order of the revised side, for each
//! container of each story: a paragraph pair that differs is an `Edit`, a run of unpaired revised blocks is an `Insert`, a run
//! of unpaired original blocks is a `Delete` with the revised block it goes next to, and a split or a merge is its own
//! entry. A pair that is equal in text and formatting is counted and not listed. Nothing here reads more than the two
//! trees, and nothing is written, so the plan of a fixture is a snapshot test.

use super::model::{Body, Container, Node};
use super::plan_fields::dissolve_field_groups;
use super::plan_para::{MergePlan, ParaEdits, Reader, SplitPlan};
use super::regroup::Group;
use super::stories::{Pairings, RootPair, StoryPair};
use super::tables::RowsPairing;
use crate::api::error::StetError;
use crate::state::State;
use crate::wordml::{NodeId, StoryKind};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Where imported blocks go: before or after a block of the revised document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct At {
    /// The revised block.
    pub anchor: NodeId,
    /// True to put them before it, false after it.
    pub before: bool,
}

/// One thing the build does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    /// A paired paragraph that differs.
    Edit(ParaEdits),
    /// Consecutive revised blocks (paragraphs, tables or rows) that have no original: they are marked inserted.
    Insert {
        /// The blocks.
        blocks: Vec<NodeId>,
    },
    /// Consecutive original blocks that have no revised partner: they are imported and placed as deleted.
    Delete {
        /// The original blocks.
        blocks: Vec<NodeId>,
        /// The revised block they go next to.
        at: At,
        /// For a run that ends its container: the last live revised paragraph before it, whose mark is deleted.
        trailing: Option<NodeId>,
    },
    /// One original paragraph that became several.
    Split(SplitPlan),
    /// Several original paragraphs that became one.
    Merge(MergePlan),
}

/// The plan of one story.
#[derive(Clone, Debug)]
pub(super) struct StoryPlan {
    /// What the story is.
    pub kind: StoryKind,
    /// The original's part, empty if there is none.
    pub part_a: Arc<str>,
    /// The revised part.
    pub part_b: Arc<str>,
    /// The entries, container by container.
    pub entries: Vec<Entry>,
    /// The moves: the original block that is deleted and the revised block that is inserted.
    pub moves: Vec<(NodeId, NodeId)>,
    /// How many pairs are equal and need nothing.
    pub kept: usize,
}

/// The plan of a comparison.
#[derive(Clone, Debug, Default)]
pub(super) struct Plan {
    /// The stories, the body first.
    pub stories: Vec<StoryPlan>,
    /// Header and footer slots that were skipped.
    pub unmatched_slots: usize,
    /// Rows that were left unpaired because their cells changed shape.
    pub table_replaced: usize,
    /// Blocks of the original that had no place to go in the revised document, which cannot happen in a document that has
    /// a block in every container. A count so that it is never silent.
    pub lost: usize,
}

/// Plans the whole comparison.
pub(super) fn plan(
    pairings: &Pairings,
    original: &State,
    revised: &State,
    same_num: &dyn Fn(i64, i64) -> bool,
) -> Result<Plan, StetError> {
    let mut out = Plan {
        stories: Vec::new(),
        unmatched_slots: pairings.unmatched_slots,
        table_replaced: pairings.table_replaced,
        lost: 0,
    };
    let (va, vb) = (original.parts(), revised.parts());
    let referenced = referenced_notes(pairings);
    for story in &pairings.stories {
        let tree_a = match va.id(&story.part_a) {
            Some(id) => va.tree(id)?,
            None => vb.tree(vb.id(&story.part_b).ok_or_else(|| {
                crate::api::error::StetError::new(
                    crate::api::error::ErrorCode::Internal,
                    "a revised part is missing",
                )
            })?)?,
        };
        let tree_b = vb.tree(vb.id(&story.part_b).ok_or_else(|| {
            crate::api::error::StetError::new(
                crate::api::error::ErrorCode::Internal,
                "a revised part is missing",
            )
        })?)?;
        let mut reader = Reader {
            a: tree_a,
            b: tree_b,
            same_num,
            sigs: super::fmt::Sigs::default(),
        };
        let mut sp = StoryPlan {
            kind: story.kind,
            part_a: Arc::clone(&story.part_a),
            part_b: Arc::clone(&story.part_b),
            entries: Vec::new(),
            moves: Vec::new(),
            kept: 0,
        };
        for root in &story.roots {
            for ci in 0..root.pairing.containers.len() {
                plan_container(story, root, ci, &mut reader, &mut sp, &mut out.lost);
            }
            for m in &root.regrouped.moves {
                if let (Some(a), Some(b)) = (
                    block_id(story, root, m.a.pairing, m.a.index, true),
                    block_id(story, root, m.b.pairing, m.b.index, false),
                ) {
                    sp.moves.push((a, b));
                }
            }
        }
        insert_new_notes(story, &referenced, &mut sp);
        out.stories.push(sp);
    }
    Ok(out)
}

/// The id of a block of a container pairing, on the original's side or the revised.
fn block_id(
    story: &StoryPair,
    root: &RootPair,
    pairing: usize,
    index: usize,
    side_a: bool,
) -> Option<NodeId> {
    let c = root.pairing.containers.get(pairing)?;
    let (model, container) = if side_a {
        (&story.model_a, c.ca)
    } else {
        (&story.model_b, c.cb)
    };
    model
        .containers
        .get(container)?
        .blocks
        .get(index)
        .map(|n| n.id)
}

/// The notes that a revised body paragraph references: (id, is an endnote).
fn referenced_notes(pairings: &Pairings) -> HashSet<(i64, bool)> {
    let mut out = HashSet::new();
    if let Some(body) = pairings.stories.first() {
        for c in &body.model_b.containers {
            for n in &c.blocks {
                if let Body::Para(p) = &n.body {
                    out.extend(p.notes.iter().map(|&(_, id, end)| (id, end)));
                }
            }
        }
    }
    out
}

/// The notes that a revised paragraph references and no original note pairs with are new: their blocks are inserted.
fn insert_new_notes(story: &StoryPair, referenced: &HashSet<(i64, bool)>, sp: &mut StoryPlan) {
    let endnote = match story.kind {
        StoryKind::Footnotes => false,
        StoryKind::Endnotes => true,
        _ => return,
    };
    let paired: HashSet<i64> = story.roots.iter().filter_map(|r| r.note_b).collect();
    for root in &story.model_b.roots {
        let Some(id) = root.note else { continue };
        if paired.contains(&id) || !referenced.contains(&(id, endnote)) {
            continue;
        }
        let blocks: Vec<NodeId> = story
            .model_b
            .containers
            .get(root.container)
            .map(|c| c.blocks.iter().map(|n| n.id).collect())
            .unwrap_or_default();
        if !blocks.is_empty() {
            sp.entries.push(Entry::Insert { blocks });
        }
    }
}

/// The gaps between consecutive pairs: for each, the unpaired ranges of the two lists, and the paired revised indexes on its
/// two sides. The last gap has no next pair.
struct Gap {
    a: std::ops::Range<usize>,
    b: std::ops::Range<usize>,
    prev_b: Option<usize>,
    next_b: Option<usize>,
}

fn gaps(len_a: usize, len_b: usize, pairs: &[(usize, usize)]) -> Vec<Gap> {
    let mut out = Vec::with_capacity(pairs.len() + 1);
    let (mut a, mut b) = (0usize, 0usize);
    let mut prev_b = None;
    for &(pa, pb) in pairs {
        out.push(Gap {
            a: a..pa,
            b: b..pb,
            prev_b,
            next_b: Some(pb),
        });
        a = pa + 1;
        b = pb + 1;
        prev_b = Some(pb);
    }
    out.push(Gap {
        a: a..len_a,
        b: b..len_b,
        prev_b,
        next_b: None,
    });
    out
}

/// Consecutive runs of the indexes of `range` that are not in `skip`.
fn runs(range: std::ops::Range<usize>, skip: &HashSet<usize>) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut open = false;
    for i in range {
        if skip.contains(&i) {
            open = false;
            continue;
        }
        match out.last_mut() {
            Some(run) if open => run.push(i),
            _ => {
                out.push(vec![i]);
                open = true;
            }
        }
    }
    out
}

/// The entries for the unpaired blocks of one gap: the deletions first and then the insertions, as Word writes a replacement.
#[expect(
    clippy::too_many_arguments,
    reason = "one gap of one list, with what it needs to name its nodes"
)]
fn gap_entries(
    g: &Gap,
    skip_a: &HashSet<usize>,
    skip_b: &HashSet<usize>,
    id_a: &dyn Fn(usize) -> Option<NodeId>,
    id_b: &dyn Fn(usize) -> Option<NodeId>,
    last_b: usize,
    paired_para: &dyn Fn(usize) -> bool,
    groups: &[&Group],
    out: &mut Vec<Entry>,
    lost: &mut usize,
    trailing_marks: &mut Vec<usize>,
) {
    for run in runs(g.a.clone(), skip_a) {
        let blocks: Vec<NodeId> = run.iter().filter_map(|&i| id_a(i)).collect();
        let (first, last) = (
            run.first().copied().unwrap_or(0),
            run.last().copied().unwrap_or(0),
        );
        // a split or a merge that lies wholly before the run in the original has to be before it in the redline too, and one that
        // lies wholly after it, after: else rejecting would put the blocks in another order
        let after_b = groups
            .iter()
            .filter(|gr| gr.a.iter().all(|&x| x < first))
            .filter_map(|gr| gr.b.iter().max().copied())
            .max();
        let before_b = groups
            .iter()
            .filter(|gr| gr.a.iter().all(|&x| x > last))
            .filter_map(|gr| gr.b.iter().min().copied())
            .min();
        let lo = after_b.map_or(g.b.start, |m| (m + 1).max(g.b.start));
        let hi = before_b.map_or(g.b.end, |m| m.min(g.b.end));
        let free = (lo..hi).find(|i| !skip_b.contains(i));
        // before the first free revised block of the gap, else before the next pair, else after the previous pair
        let (anchor, before, trailing) = if let Some(i) = free {
            (id_b(i), true, None)
        } else if let Some(m) = before_b.filter(|m| g.b.contains(m)) {
            (id_b(m), true, None)
        } else if let Some(m) = after_b.filter(|m| g.b.contains(m)) {
            let live = m == last_b && paired_para(m);
            (id_b(m), false, live.then_some(m))
        } else if let Some(n) = g.next_b {
            (id_b(n), true, None)
        } else if let Some(p) = g.prev_b {
            let live = p == last_b && paired_para(p);
            (id_b(p), false, live.then_some(p))
        } else {
            (None, false, None)
        };
        match anchor {
            Some(anchor) => {
                if let Some(t) = trailing {
                    trailing_marks.push(t);
                }
                out.push(Entry::Delete {
                    blocks,
                    at: At { anchor, before },
                    trailing: trailing.and_then(id_b),
                });
            }
            None => *lost += blocks.len(),
        }
    }
    for run in runs(g.b.clone(), skip_b) {
        let blocks: Vec<NodeId> = run.iter().filter_map(|&i| id_b(i)).collect();
        out.push(Entry::Insert { blocks });
    }
}

/// Plans one container pairing.
fn plan_container(
    story: &StoryPair,
    root: &RootPair,
    ci: usize,
    reader: &mut Reader<'_>,
    sp: &mut StoryPlan,
    lost: &mut usize,
) {
    let Some(c) = root.pairing.containers.get(ci) else {
        return;
    };
    let (Some(ca), Some(cb)) = (
        story.model_a.containers.get(c.ca),
        story.model_b.containers.get(c.cb),
    ) else {
        return;
    };
    // a split or a merge whose parts cannot be planned (a part with no text of its own) is dissolved: its paragraphs are paired,
    // deleted and inserted one by one
    let mut groups: Vec<&Group> = Vec::new();
    let mut group_entries: Vec<Entry> = Vec::new();
    for g in root.regrouped.groups.iter().filter(|g| g.pairing == ci) {
        if let Some(e) = group_entry(ca, cb, g, reader) {
            groups.push(g);
            group_entries.push(e);
        }
    }
    let skip_a: HashSet<usize> = groups.iter().flat_map(|g| g.a.iter().copied()).collect();
    let skip_b: HashSet<usize> = groups.iter().flat_map(|g| g.b.iter().copied()).collect();
    let pairs = dissolve_field_groups(ca, cb, &c.pairs, reader);
    let mut entries: Vec<Entry> = Vec::new();
    let id_a = |i: usize| ca.blocks.get(i).map(|n| n.id);
    let id_b = |i: usize| cb.blocks.get(i).map(|n| n.id);
    let a_of_b: HashMap<usize, usize> = pairs.iter().map(|p| (p.b, p.a)).collect();
    let paired_para = |i: usize| {
        a_of_b.contains_key(&i) && matches!(cb.blocks.get(i).map(|n| &n.body), Some(Body::Para(_)))
    };
    let list: Vec<(usize, usize)> = pairs.iter().map(|p| (p.a, p.b)).collect();
    let mut trailing_marks: Vec<usize> = Vec::new();
    for g in gaps(ca.blocks.len(), cb.blocks.len(), &list) {
        gap_entries(
            &g,
            &skip_a,
            &skip_b,
            &id_a,
            &id_b,
            cb.blocks.len().saturating_sub(1),
            &paired_para,
            &groups,
            &mut entries,
            lost,
            &mut trailing_marks,
        );
    }
    for p in &pairs {
        if skip_a.contains(&p.a) || skip_b.contains(&p.b) {
            continue;
        }
        let (Some(x), Some(y)) = (ca.blocks.get(p.a), cb.blocks.get(p.b)) else {
            continue;
        };
        match (&x.body, &y.body) {
            (Body::Para(_), Body::Para(_)) => {
                let e = reader.pair(x, y);
                if e.is_empty() {
                    sp.kept += 1;
                } else {
                    entries.push(Entry::Edit(e));
                }
            }
            (Body::Table(ra), Body::Table(rb)) => {
                if let Some(rows) = &p.rows {
                    plan_rows(
                        ra.len(),
                        rb.len(),
                        rows,
                        &|i| ra.get(i).map(|r| r.id),
                        &|i| rb.get(i).map(|r| r.id),
                        &mut entries,
                        lost,
                    );
                }
            }
            _ => {}
        }
    }
    entries.extend(group_entries);
    for t in trailing_marks {
        let Some(node) = id_b(t) else { continue };
        let found = entries.iter_mut().find_map(|e| match e {
            Entry::Edit(pe) if pe.b == Some(node) => Some(pe),
            _ => None,
        });
        match found {
            Some(pe) => pe.trailing_mark = true,
            None => {
                let a = a_of_b.get(&t).and_then(|&i| id_a(i));
                entries.push(Entry::Edit(ParaEdits {
                    a,
                    b: Some(node),
                    trailing_mark: true,
                    ..ParaEdits::default()
                }));
                sp.kept = sp.kept.saturating_sub(1);
            }
        }
    }
    sp.entries.extend(entries);
}

/// The rows of two paired tables: unpaired revised rows are inserted, unpaired original rows are deleted next to a revised row.
fn plan_rows(
    len_a: usize,
    len_b: usize,
    rows: &RowsPairing,
    id_a: &dyn Fn(usize) -> Option<NodeId>,
    id_b: &dyn Fn(usize) -> Option<NodeId>,
    out: &mut Vec<Entry>,
    lost: &mut usize,
) {
    let list: Vec<(usize, usize)> = rows.pairs.iter().map(|r| (r.a, r.b)).collect();
    let none = HashSet::new();
    for g in gaps(len_a, len_b, &list) {
        gap_entries(
            &g,
            &none,
            &none,
            id_a,
            id_b,
            usize::MAX,
            &|_| false,
            &[],
            out,
            lost,
            &mut Vec::new(),
        );
    }
}

/// A split or a merge as an entry.
fn group_entry(
    ca: &Container,
    cb: &Container,
    g: &Group,
    reader: &mut Reader<'_>,
) -> Option<Entry> {
    let nodes_a: Vec<&Node> = g.a.iter().filter_map(|&i| ca.blocks.get(i)).collect();
    let nodes_b: Vec<&Node> = g.b.iter().filter_map(|&i| cb.blocks.get(i)).collect();
    if g.is_split() {
        reader.split(nodes_a.first()?, &nodes_b).map(Entry::Split)
    } else {
        reader.merge(&nodes_a, nodes_b.first()?).map(Entry::Merge)
    }
}
