//! The plan for one pair of paragraphs, and for a split or a merge (design `compare.md`, 5.3 to 5.5).
//!
//! A pair becomes a [`ParaEdits`]: the replacements as ranges of the revised text to mark inserted and ranges of the
//! original's text to import as deleted, the ranges of changed run formatting, and whether the paragraph's properties and its
//! mark's differ. A split diffs the one original paragraph against the concatenation of the revised ones and hands each
//! replacement to the revised paragraph that holds its start. A merge diffs the concatenation of the original paragraphs
//! against the one revised paragraph, cuts it at the places the original paragraphs met, and gives each part its own original.

use super::edits::{Replace, map_through, replacements};
use super::fmt::{FmtChange, ParaDiff, Sigs, char_sigs, para_diff, run_changes};
use super::model::{Body, Node, ParaInfo};
use crate::wordml::{NodeId, Paragraph, SpanTree};
use std::ops::Range;

/// The share of the characters of the one paragraph of a split (or a merge) that the replacements must leave alone: the parts are
/// pieces of it, and not new text that happens to use its words.
pub(super) const GROUP_KEPT: f64 = 0.5;

/// One replacement of a paragraph's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlanEdit {
    /// The characters of the revised paragraph that are marked inserted, empty for a pure deletion. It is also where the
    /// deleted text goes.
    pub insert: Range<usize>,
    /// The characters of the original paragraph that are imported as deleted, if any.
    pub deleted: Option<Range<usize>>,
}

/// What is written to one paragraph of the revised document, or to one part of a paragraph that a merge cuts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ParaEdits {
    /// The original paragraph the deleted text and the old properties come from.
    pub a: Option<NodeId>,
    /// The revised paragraph. `None` for the part of a merge, which exists after the cut.
    pub b: Option<NodeId>,
    /// The replacements, in order.
    pub edits: Vec<PlanEdit>,
    /// The run formatting that changed over unchanged text.
    pub fmt: Vec<FmtChange>,
    /// The paragraph properties differ: a `pPrChange` with the original's as the snapshot.
    pub ppr: bool,
    /// The properties of the paragraph mark differ.
    pub mark: bool,
    /// The mark alone is inserted (the first parts of a split).
    pub mark_inserted: bool,
    /// The paragraph is the last live one before a run of trailing deletions, so its mark is deleted.
    pub trailing_mark: bool,
}

impl ParaEdits {
    /// True if nothing is written.
    pub(super) fn is_empty(&self) -> bool {
        self.edits.is_empty()
            && self.fmt.is_empty()
            && !self.ppr
            && !self.mark
            && !self.mark_inserted
            && !self.trailing_mark
    }
}

/// A merge: the original paragraphs, the revised one, where it is cut, and the edits of each part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct MergePlan {
    /// The revised paragraph that is cut.
    pub b: NodeId,
    /// The offsets in its text where it is cut: one fewer than there are parts.
    pub cuts: Vec<usize>,
    /// One entry per original paragraph, in order. Offsets are relative to the start of the part.
    pub parts: Vec<ParaEdits>,
}

/// A split: the original paragraph and the edits of each revised one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SplitPlan {
    /// The original paragraph.
    pub a: NodeId,
    /// One entry per revised paragraph, in order.
    pub parts: Vec<ParaEdits>,
}

/// What the plan reads to compare two paragraphs: the two trees, the list equivalence, and the signatures it interns.
pub(super) struct Reader<'a> {
    /// The original's tree for the story.
    pub a: &'a SpanTree,
    /// The revised tree.
    pub b: &'a SpanTree,
    /// Whether an original `numId` is the revised document's list.
    pub same_num: &'a dyn Fn(i64, i64) -> bool,
    /// Interned run properties.
    pub sigs: Sigs,
}

fn info(n: &Node) -> Option<&ParaInfo> {
    match &n.body {
        Body::Para(p) => Some(p),
        Body::Table(_) => None,
    }
}

impl Reader<'_> {
    /// The signatures of the run properties of a paragraph, and the paragraph itself.
    fn sigs_of(&mut self, tree_is_a: bool, node: NodeId) -> Option<Vec<u32>> {
        let tree = if tree_is_a { self.a } else { self.b };
        let p = Paragraph::new(tree, node)?;
        let map = p.map();
        Some(char_sigs(&p, &map, &mut self.sigs))
    }

    fn diff(&self, a: NodeId, b: NodeId) -> ParaDiff {
        match (Paragraph::new(self.a, a), Paragraph::new(self.b, b)) {
            (Some(x), Some(y)) => para_diff(&x, &y, self.same_num),
            _ => ParaDiff::default(),
        }
    }

    /// The plan for two paired paragraphs.
    pub(super) fn pair(&mut self, a: &Node, b: &Node) -> ParaEdits {
        let (Some(pa), Some(pb)) = (info(a), info(b)) else {
            return ParaEdits::default();
        };
        let reps = if pa.text == pb.text {
            Vec::new()
        } else {
            replacements(&pa.text, &pb.text, &pa.fields, &pb.fields)
        };
        let fmt = match (self.sigs_of(true, a.id), self.sigs_of(false, b.id)) {
            (Some(sa), Some(sb)) => run_changes(&sa, &sb, &reps),
            _ => Vec::new(),
        };
        let d = self.diff(a.id, b.id);
        ParaEdits {
            a: Some(a.id),
            b: Some(b.id),
            edits: edits_of(&reps),
            fmt,
            ppr: d.ppr,
            mark: d.mark,
            mark_inserted: false,
            trailing_mark: false,
        }
    }

    /// The plan for one original paragraph split into the revised paragraphs `bs`.
    pub(super) fn split(&mut self, a: &Node, bs: &[&Node]) -> Option<SplitPlan> {
        let pa = info(a)?;
        let mut text = String::new();
        let mut spans_b: Vec<Range<usize>> = Vec::new();
        let mut starts = Vec::new();
        let mut sigs_b: Vec<u32> = Vec::new();
        for b in bs {
            let pb = info(b)?;
            let at = text.chars().count();
            starts.push(at);
            text.push_str(&pb.text);
            spans_b.extend(pb.fields.iter().map(|r| r.start + at..r.end + at));
            sigs_b.extend(self.sigs_of(false, b.id)?);
        }
        let reps = replacements(&pa.text, &text, &pa.fields, &spans_b);
        let len_a = pa.text.chars().count();
        let removed: usize = reps.iter().map(|r| r.a.len()).sum();
        if (len_a.saturating_sub(removed) as f64) < GROUP_KEPT * len_a as f64 {
            return None;
        }
        let fmt = run_changes(&self.sigs_of(true, a.id)?, &sigs_b, &reps);
        let ends: Vec<usize> = starts
            .iter()
            .skip(1)
            .copied()
            .chain(std::iter::once(text.chars().count()))
            .collect();
        let mut parts: Vec<ParaEdits> = Vec::new();
        let last = bs.len().saturating_sub(1);
        for (i, b) in bs.iter().enumerate() {
            let (s, e) = (*starts.get(i)?, *ends.get(i)?);
            let d = self.diff(a.id, b.id);
            parts.push(ParaEdits {
                a: Some(a.id),
                b: Some(b.id),
                edits: Vec::new(),
                fmt: cut_fmt(&fmt, s..e),
                ppr: i == last && d.ppr,
                mark: i == last && d.mark,
                mark_inserted: i != last,
                trailing_mark: false,
            });
        }
        for r in &reps {
            // a deletion alone at a boundary belongs to the earlier part, anything else to the part that holds its start
            let at = parts_index(&starts, &ends, r.b.start, r.b.is_empty());
            let s = *starts.get(at)?;
            let end_in_part = r.b.end.min(*ends.get(at)?);
            if let Some(part) = parts.get_mut(at) {
                part.edits.push(PlanEdit {
                    insert: r.b.start - s..end_in_part - s,
                    deleted: (!r.a.is_empty()).then(|| r.a.clone()),
                });
            }
            // the rest of an insertion that crosses a boundary goes to the parts it covers
            let mut next = at + 1;
            while let (Some(&ns), Some(&ne)) = (starts.get(next), ends.get(next)) {
                if ns >= r.b.end {
                    break;
                }
                if let Some(part) = parts.get_mut(next) {
                    part.edits.push(PlanEdit {
                        insert: 0..r.b.end.min(ne) - ns,
                        deleted: None,
                    });
                }
                next += 1;
            }
        }
        Some(SplitPlan { a: a.id, parts })
    }

    /// The plan for the original paragraphs `as_` merged into the revised paragraph `b`.
    pub(super) fn merge(&mut self, as_: &[&Node], b: &Node) -> Option<MergePlan> {
        let pb = info(b)?;
        let mut text = String::new();
        let mut starts = Vec::new();
        let mut spans_a: Vec<Range<usize>> = Vec::new();
        let mut sigs_a: Vec<u32> = Vec::new();
        for a in as_ {
            let pa = info(a)?;
            let at = text.chars().count();
            starts.push(at);
            text.push_str(&pa.text);
            spans_a.extend(pa.fields.iter().map(|r| r.start + at..r.end + at));
            sigs_a.extend(self.sigs_of(true, a.id)?);
        }
        let reps = replacements(&text, &pb.text, &spans_a, &pb.fields);
        let len_b = pb.text.chars().count();
        let added: usize = reps.iter().map(|r| r.b.len()).sum();
        if (len_b.saturating_sub(added) as f64) < GROUP_KEPT * len_b as f64 {
            return None;
        }
        let fmt = run_changes(&sigs_a, &self.sigs_of(false, b.id)?, &reps);
        let ends: Vec<usize> = starts
            .iter()
            .skip(1)
            .copied()
            .chain(std::iter::once(text.chars().count()))
            .collect();
        // where the original paragraphs met, in the revised text: the start of a replacement that holds the boundary, or the
        // mapped offset
        let cuts: Vec<usize> = starts
            .iter()
            .skip(1)
            .map(|&t| {
                map_through(&reps, t).unwrap_or_else(|| {
                    reps.iter()
                        .find(|r| r.a.start < t && t < r.a.end)
                        .map_or(pb.text.chars().count(), |r| r.b.start)
                })
            })
            .collect();
        let mut b_starts = vec![0usize];
        b_starts.extend(cuts.iter().copied());
        // every original paragraph has text of its own in the revised one: the cuts rise and lie inside it
        if b_starts
            .iter()
            .zip(b_starts.iter().skip(1))
            .any(|(x, y)| x >= y)
            || b_starts.last().is_some_and(|&l| l >= len_b)
        {
            return None;
        }
        let region = |k: usize| -> Range<usize> {
            let s = b_starts.get(k).copied().unwrap_or(len_b);
            let e = b_starts.get(k + 1).copied().unwrap_or(len_b);
            s..e
        };
        let mut parts: Vec<ParaEdits> = Vec::new();
        for a in as_ {
            let d = self.diff(a.id, b.id);
            parts.push(ParaEdits {
                a: Some(a.id),
                b: None,
                edits: Vec::new(),
                fmt: Vec::new(),
                ppr: d.ppr,
                mark: d.mark,
                mark_inserted: false,
                trailing_mark: false,
            });
        }
        let part_of_b = |at: usize| b_starts.iter().rposition(|&s| s <= at).unwrap_or(0);
        // the part of a range of the revised text that lies in a region, relative to the region's start
        let within = |r: &Range<usize>, k: usize| -> Range<usize> {
            let reg = region(k);
            let s = r.start.clamp(reg.start, reg.end);
            let e = r.end.clamp(reg.start, reg.end);
            s - reg.start..e - reg.start
        };
        for r in &reps {
            // one piece of the deletion for each original paragraph that it crosses, in that paragraph's part; the insertion goes
            // with the last piece, and what does not fit in its part goes to the parts after it
            let first = starts.iter().rposition(|&s| s <= r.a.start).unwrap_or(0);
            let lastp = if r.a.is_empty() {
                first
            } else {
                starts.iter().rposition(|&s| s < r.a.end).unwrap_or(first)
            };
            for pi in first..=lastp {
                let (ps, pe) = (*starts.get(pi)?, *ends.get(pi)?);
                let piece = r.a.start.max(ps)..r.a.end.min(pe);
                let insert = if pi == lastp {
                    within(&r.b, pi)
                } else {
                    let at = within(&(r.b.start..r.b.start), pi).start;
                    at..at
                };
                if let Some(part) = parts.get_mut(pi) {
                    part.edits.push(PlanEdit {
                        insert,
                        deleted: (!piece.is_empty()).then(|| piece.start - ps..piece.end - ps),
                    });
                }
            }
            for k in lastp + 1..parts.len() {
                let ins = within(&r.b, k);
                if !ins.is_empty()
                    && let Some(part) = parts.get_mut(k)
                {
                    part.edits.push(PlanEdit {
                        insert: ins,
                        deleted: None,
                    });
                }
            }
            for k in 0..first {
                let ins = within(&r.b, k);
                if !ins.is_empty()
                    && let Some(part) = parts.get_mut(k)
                {
                    part.edits.push(PlanEdit {
                        insert: ins,
                        deleted: None,
                    });
                }
            }
        }
        for change in &fmt {
            let pi = part_of_b(change.b.start);
            let (bs, ps) = (*b_starts.get(pi)?, *starts.get(pi)?);
            // the snapshot is read from the paragraph of the part: a change whose original characters are in another one is left out
            if starts.iter().rposition(|&s| s <= change.a.start) != Some(pi) {
                continue;
            }
            let be = b_starts.get(pi + 1).copied().unwrap_or(usize::MAX);
            let ae = ends.get(pi).copied().unwrap_or(usize::MAX);
            let len = change
                .b
                .end
                .min(be)
                .saturating_sub(change.b.start)
                .min(ae.saturating_sub(change.a.start));
            if len == 0 {
                continue;
            }
            if let Some(part) = parts.get_mut(pi) {
                part.fmt.push(FmtChange {
                    b: change.b.start - bs..change.b.start - bs + len,
                    a: change.a.start - ps..change.a.start - ps + len,
                });
            }
        }
        Some(MergePlan {
            b: b.id,
            cuts,
            parts,
        })
    }
}

fn edits_of(reps: &[Replace]) -> Vec<PlanEdit> {
    reps.iter()
        .map(|r| PlanEdit {
            insert: r.b.clone(),
            deleted: (!r.a.is_empty()).then(|| r.a.clone()),
        })
        .collect()
}

/// The index of the part that holds `at`: the last whose start is not after it, or the part before when the edit is a pure
/// deletion at a boundary.
fn parts_index(starts: &[usize], ends: &[usize], at: usize, pure_deletion: bool) -> usize {
    let mut i = starts.iter().rposition(|&s| s <= at).unwrap_or(0);
    if pure_deletion
        && i > 0
        && starts.get(i).copied() == Some(at)
        && ends.get(i - 1).copied() == Some(at)
    {
        i -= 1;
    }
    i
}

/// The formatting changes of the concatenation that fall in the part `range`, relative to its start.
fn cut_fmt(fmt: &[FmtChange], range: Range<usize>) -> Vec<FmtChange> {
    fmt.iter()
        .filter_map(|c| {
            let s = c.b.start.max(range.start);
            let e = c.b.end.min(range.end);
            (s < e).then(|| FmtChange {
                b: s - range.start..e - range.start,
                a: c.a.start + (s - c.b.start)..c.a.start + (e - c.b.start),
            })
        })
        .collect()
}
