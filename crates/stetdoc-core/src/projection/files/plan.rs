//! The operations of one file: splits and merges, changed slots, removed blocks and new lines
//! (design 6.4, steps 6 to 9, and 6.8).
//!
//! Within a paragraph the operations go from the last slot to the first. Across the file they follow the
//! file's order: a block that is removed is placed where it stood, a run of new lines where it is. Every
//! operation has one origin, the file and the line that produced it, so that an error that names an
//! operation can be shown as "document.md line 57 (p42)".

use super::align::Ctx;
use super::anchor::Lines;
use super::matched::compare;
use super::recognize::Found;
use super::refuse::{self, Problems};
use super::style::styles;
use super::{Plan, StyleRef};
use crate::api::error::StetError;
use crate::api::files::{NewParagraph, Origin, PlannedOp};
use crate::api::view::{Warning, WarningCode};
use crate::projection::draft::DraftLine;
use crate::projection::keys::Key;
use crate::refs::BlockRef;
use std::collections::HashMap;

/// An operation with where it goes in the file's order.
struct Item {
    at: usize,
    sub: u8,
    op: PlannedOp,
    origin: Origin,
}

fn origin(file: &str, line: u32, block: Option<BlockRef>, also: Option<BlockRef>) -> Origin {
    Origin {
        file: file.to_string(),
        line,
        block: block.map(|r| r.to_string()),
        also: also.map(|r| r.to_string()),
    }
}

/// The story an `Origin` or a warning names, from the file's name.
fn story(file: &str) -> &'static str {
    if file.starts_with("notes") {
        "notes"
    } else if file.starts_with("headers") {
        "headers"
    } else if file.starts_with("comments") {
        "comments"
    } else {
        "body"
    }
}

/// Adds the operations of one file to the plan.
pub(super) fn build(ctx: &mut Ctx, found: &Found, plan: &mut Plan) -> Result<(), StetError> {
    let mut items: Vec<Item> = Vec::new();
    edits(ctx, found, &mut items, plan);
    matched_lines(ctx, found, &mut items)?;
    removed(ctx, found, &mut items)?;
    new_lines(ctx, found, &mut items)?;
    rewrites(ctx, found, plan)?;
    items.sort_by_key(|item| (item.at, item.sub));
    for item in items {
        plan.ops.push(item.op);
        plan.origins.push(item.origin);
    }
    Ok(())
}

/// The recognized splits and merges.
fn edits(ctx: &Ctx, found: &Found, items: &mut Vec<Item>, plan: &mut Plan) {
    for split in &found.splits {
        let Some(par) = ctx.bpars.get(split.bi) else {
            continue;
        };
        let start = split
            .pieces
            .first()
            .and_then(|&k| ctx.elines.get(k))
            .map_or(0, |e| e.line);
        for cut in split.cuts.iter().rev() {
            let line = split
                .pieces
                .get(cut.piece)
                .and_then(|&k| ctx.elines.get(k))
                .map_or(0, |e| e.block.line);
            items.push(Item {
                at: start,
                sub: 1,
                op: PlannedOp::Split {
                    block: par.r.to_string(),
                    at: cut.at,
                },
                origin: origin(ctx.file, line, Some(par.r), None),
            });
            let wrapped = ctx.baseline.wraps.iter().any(|w| {
                w.file == ctx.file
                    && w.r#ref == par.r
                    && usize::try_from(w.slot).is_ok_and(|s| s == cut.slot)
                    && w.at == cut.within
            });
            if wrapped {
                plan.notes.push(Warning {
                    code: WarningCode::SplitAtWrap,
                    block: Some(par.r.to_string()),
                    story: Some(story(ctx.file).to_string()),
                    message: format!(
                        "{} was split where its line had been wrapped: a continuation backslash was removed. If you meant to keep one paragraph, put the backslash back.",
                        par.r
                    ),
                    next: None,
                });
            }
        }
    }
    for merge in &found.merges {
        let line_of = ctx.elines.get(merge.line);
        let (at, line) = line_of.map_or((0, 0), |e| (e.line, e.block.line));
        // The whitespace that goes into the second paragraph goes in before the merge, which takes that
        // paragraph away.
        if merge.in_second
            && let (Some((offset, with)), Some(second)) = (
                &merge.insert,
                merge.parts.get(1).and_then(|&i| ctx.bpars.get(i)),
            )
        {
            items.push(Item {
                at,
                sub: 0,
                op: PlannedOp::Replace {
                    block: second.r.to_string(),
                    start: *offset,
                    end: *offset,
                    with: with.clone(),
                },
                origin: origin(ctx.file, line, Some(second.r), None),
            });
        }
        for pair in merge.parts.windows(2) {
            let (Some(a), Some(b)) = (
                pair.first().and_then(|&i| ctx.bpars.get(i)),
                pair.get(1).and_then(|&i| ctx.bpars.get(i)),
            ) else {
                continue;
            };
            items.push(Item {
                at,
                sub: 1,
                op: PlannedOp::Merge {
                    first: a.r.to_string(),
                    second: b.r.to_string(),
                },
                origin: origin(ctx.file, line, Some(a.r), Some(b.r)),
            });
        }
        if let (Some((offset, with)), Some(first), false) = (
            &merge.insert,
            merge.parts.first().and_then(|&i| ctx.bpars.get(i)),
            merge.in_second,
        ) {
            items.push(Item {
                at,
                sub: 1,
                op: PlannedOp::Replace {
                    block: first.r.to_string(),
                    start: *offset,
                    end: *offset,
                    with: with.clone(),
                },
                origin: origin(ctx.file, line, Some(first.r), None),
            });
        }
    }
}

/// Step 7: the lines whose ref is in the baseline, that no split or merge took.
fn matched_lines(ctx: &Ctx, found: &Found, items: &mut Vec<Item>) -> Result<(), StetError> {
    for (k, eline) in ctx.elines.iter().enumerate() {
        let (Some(bi), true) = (eline.bi, found.used_e.get(k) == Some(&false)) else {
            continue;
        };
        let Some(par) = ctx.bpars.get(bi) else {
            continue;
        };
        for op in compare(ctx.file, par.r, par.block, eline.block)? {
            items.push(Item {
                at: eline.line,
                sub: 1,
                op,
                origin: origin(ctx.file, eline.block.line, Some(par.r), None),
            });
        }
    }
    Ok(())
}

/// Where in the edited file a block removed at gap `g` stood: just after the paragraph that the gap follows.
/// `starts[g]` is that place, for each gap.
fn gap_starts(ctx: &Ctx) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        ctx.elines
            .iter()
            .filter(|e| e.bi.is_some())
            .map(|e| e.line + 1),
    );
    starts
}

/// The line of the edited file that a removal at `at` (an index into the file's lines) is next to: the line
/// before it, or the one after when it is at the start. A block that is gone has no line of its own, and an
/// origin that says "line 0" tells a reader nothing; this is the line where the removal shows.
fn line_near(ctx: &Ctx, at: usize) -> u32 {
    let number = |line: &DraftLine| match line {
        DraftLine::Block(block) => block.line,
        DraftLine::Structure(s) => s.line,
    };
    at.checked_sub(1)
        .and_then(|i| ctx.ef.lines.get(i))
        .or_else(|| ctx.ef.lines.get(at))
        .map_or(0, number)
}

/// Step 8: a baseline paragraph whose line is gone is deleted, a table or a row that is gone whole is
/// deleted whole.
fn removed(ctx: &Ctx, found: &Found, items: &mut Vec<Item>) -> Result<(), StetError> {
    let present: std::collections::HashSet<usize> =
        ctx.elines.iter().filter_map(|e| e.bi).collect();
    let starts = gap_starts(ctx);
    // the matched paragraphs of the edited file, by the line of their baseline paragraph and their own line
    let anchors: Vec<(usize, usize)> = ctx
        .elines
        .iter()
        .filter_map(|e| {
            e.bi.and_then(|bi| ctx.bpars.get(bi))
                .map(|p| (p.line, e.line))
        })
        .collect();
    for (b, par) in ctx.bpars.iter().enumerate() {
        if present.contains(&b)
            || found.used_b.get(b) == Some(&true)
            || ctx.in_removed.contains(&par.line)
        {
            continue;
        }
        if let Some(reason) = par.block.facts.readonly {
            return Err(refuse::removed_readonly(ctx.file, par.r, reason));
        }
        let gap = found.gap_b.get(b).copied().unwrap_or(0);
        let at = starts.get(gap).copied().unwrap_or(0);
        items.push(Item {
            at,
            sub: 0,
            op: PlannedOp::Delete {
                block: par.r.to_string(),
            },
            origin: origin(ctx.file, line_near(ctx, at), Some(par.r), None),
        });
    }
    for &head in &ctx.removed_heads {
        let Some(DraftLine::Structure(s)) = ctx.bf.lines.get(head) else {
            continue;
        };
        let before = anchors.partition_point(|(line, _)| *line < head);
        let after = before
            .checked_sub(1)
            .and_then(|i| anchors.get(i))
            .map_or(0, |(_, e_line)| e_line + 1);
        match &s.key {
            Key::Table(t) => {
                if let Some(r) = t.r#ref {
                    items.push(Item {
                        at: after,
                        sub: 0,
                        op: PlannedOp::Delete {
                            block: r.to_string(),
                        },
                        origin: origin(ctx.file, line_near(ctx, after), Some(r), None),
                    });
                }
            }
            Key::Row(row) => {
                if let Some(r) = row.r#ref {
                    items.push(Item {
                        at: after,
                        sub: 0,
                        op: PlannedOp::RowDelete { row: r.to_string() },
                        origin: origin(ctx.file, line_near(ctx, after), Some(r), None),
                    });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Step 9: the runs of lines with no ref that no split or merge took are inserted.
fn new_lines(ctx: &Ctx, found: &Found, items: &mut Vec<Item>) -> Result<(), StetError> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for (k, eline) in ctx.elines.iter().enumerate() {
        if eline.bi.is_some() || eline.block.r#ref.is_some() || found.used_e.get(k) != Some(&false)
        {
            continue;
        }
        let follows = runs
            .last()
            .and_then(|run| run.last())
            .and_then(|&last| ctx.elines.get(last))
            .is_some_and(|last| last.line + 1 == eline.line);
        if follows {
            if let Some(run) = runs.last_mut() {
                run.push(k);
            }
        } else {
            runs.push(vec![k]);
        }
    }
    let mut problems = Problems::default();
    for run in &runs {
        for &k in run {
            if let Some(eline) = ctx.elines.get(k) {
                if !eline.block.seams.is_empty() {
                    problems.push(
                        refuse::marker_in_new_line(ctx.file, eline.block.line),
                        "Write the text without the markers. A tracked change is recorded by editing the text.",
                        (ctx.file, eline.block.line),
                    );
                } else if !eline.block.tags.is_empty() {
                    problems.push(
                        refuse::tag_on_new_line(ctx.file, eline.block.line),
                        "Write the text only.",
                        (ctx.file, eline.block.line),
                    );
                }
            }
        }
    }
    problems.finish()?;
    let lines = Lines::new(&ctx.ef.lines, ctx.bf.common_style.as_deref());
    let edited_refs = edit_refs(ctx, found);
    for run in &runs {
        let (Some(&first), Some(&last)) = (run.first(), run.last()) else {
            continue;
        };
        let (Some(first_line), Some(last_line)) = (ctx.elines.get(first), ctx.elines.get(last))
        else {
            continue;
        };
        let number = first_line.block.line;
        if let Some(r) = consumed_before(ctx, found, first_line.line) {
            return Err(refuse::new_line_next_to_edit(ctx.file, number, r));
        }
        let anchor = lines
            .anchor(first_line.line, last_line.line + 1)
            .map_err(|why| refuse::anchor_ambiguous(ctx.file, number, &why))?;
        if edited_refs.contains_key(&anchor.block) {
            return Err(refuse::new_line_next_to_edit(
                ctx.file,
                number,
                anchor.block,
            ));
        }
        let chosen: Vec<Option<StyleRef>> = styles(run.len(), &anchor.previous, ctx.outside);
        let paragraphs: Vec<NewParagraph> = run
            .iter()
            .zip(chosen)
            .filter_map(|(&k, style)| {
                ctx.elines.get(k).map(|e| NewParagraph {
                    text: e.block.texts.first().cloned().unwrap_or_default(),
                    style: style.map(|s| s.id),
                })
            })
            .collect();
        items.push(Item {
            at: first_line.line,
            sub: 2,
            op: PlannedOp::Insert {
                anchor: anchor.block.to_string(),
                place: anchor.place,
                paragraphs,
            },
            origin: origin(ctx.file, number, Some(anchor.block), None),
        });
    }
    Ok(())
}

/// The refs of the paragraphs that a split or a merge touches.
fn edit_refs(ctx: &Ctx, found: &Found) -> HashMap<BlockRef, BlockRef> {
    let mut out = HashMap::new();
    for split in &found.splits {
        if let Some(par) = ctx.bpars.get(split.bi) {
            out.insert(par.r, par.r);
        }
    }
    for merge in &found.merges {
        for &b in &merge.parts {
            if let Some(par) = ctx.bpars.get(b) {
                out.insert(par.r, par.r);
            }
        }
    }
    out
}

/// The ref of the paragraph that the line before `line` is a part of, when a split or a merge took that line:
/// the paragraph that is cut into it, or the one that stays when lines are joined into it.
fn consumed_before(ctx: &Ctx, found: &Found, line: usize) -> Option<BlockRef> {
    let k = ctx
        .elines
        .binary_search_by_key(&line.checked_sub(1)?, |e| e.line)
        .ok()?;
    if found.used_e.get(k) != Some(&true) {
        return None;
    }
    let paragraph = found
        .splits
        .iter()
        .find(|s| s.pieces.contains(&k))
        .map(|s| s.bi)
        .or_else(|| {
            found
                .merges
                .iter()
                .find(|m| m.line == k)
                .and_then(|m| m.parts.first().copied())
        })?;
    ctx.bpars.get(paragraph).map(|p| p.r)
}

/// A line with no ref that has the text of a paragraph that was removed elsewhere is a move that the file
/// cannot say. It is a deletion and an insertion, as written, and the agent is told. The same text in the same
/// place is a line that lost its ref, which is refused: it would become a paragraph with none of the original's
/// style.
fn rewrites(ctx: &Ctx, found: &Found, plan: &mut Plan) -> Result<(), StetError> {
    let mut removed: HashMap<&str, Vec<(usize, BlockRef)>> = HashMap::new();
    let present: std::collections::HashSet<usize> =
        ctx.elines.iter().filter_map(|e| e.bi).collect();
    for (b, par) in ctx.bpars.iter().enumerate() {
        if present.contains(&b)
            || found.used_b.get(b) == Some(&true)
            || ctx.in_removed.contains(&par.line)
        {
            continue;
        }
        if let ([text], true) = (par.block.texts.as_slice(), par.block.seams.is_empty())
            && !text.is_empty()
        {
            removed
                .entry(text.as_str())
                .or_default()
                .push((found.gap_b.get(b).copied().unwrap_or(0), par.r));
        }
    }
    for (k, eline) in ctx.elines.iter().enumerate() {
        if eline.block.r#ref.is_some() || found.used_e.get(k) != Some(&false) {
            continue;
        }
        let ([text], true) = (eline.block.texts.as_slice(), eline.block.seams.is_empty()) else {
            continue;
        };
        let gap = found.gap_e.get(k).copied().unwrap_or(0);
        if let Some(candidates) = removed.get(text.as_str()) {
            if let Some((_, r)) = candidates.iter().find(|(g, _)| *g == gap) {
                // Same text in the same place: the line of `r` lost its ref and nothing else.
                return Err(refuse::ref_dropped_text_kept(
                    ctx.file,
                    eline.block.line,
                    *r,
                ));
            }
            if let Some((_, r)) = candidates.first() {
                plan.notes.push(Warning {
                    code: WarningCode::LooksLikeRewrite,
                    block: Some(r.to_string()),
                    story: Some(story(ctx.file).to_string()),
                    message: format!(
                        "{r} was removed and a new line with the same text was written elsewhere. That is a deletion and an insertion, not a move."
                    ),
                    next: Some("To move a paragraph, use an operation.".to_string()),
                });
            }
        }
    }
    Ok(())
}
