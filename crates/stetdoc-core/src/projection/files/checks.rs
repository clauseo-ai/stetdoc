//! The refusals of a recognized split or merge that the baseline answers (design 6.4).
//!
//! Recognized means committed to: an edit that the compiler would refuse must stay refused, and must never
//! turn into a deletion and an insertion of the same text. What the baseline knows is checked here, with
//! the file, the line and the way out. What depends on the author of the edit (another author's pending
//! property change) comes back from the compiler as an error with an operation index.

use super::align::Ctx;
use super::anchor::Lines;
use super::recognize::MergeEdit;
use super::refuse;
use crate::api::error::{ErrorCode, StetError};
use crate::projection::draft::{DraftBlock, DraftLine};
use crate::projection::keys::Key;
use crate::projection::model::ReadOnly;

fn readonly_reason(block: &DraftBlock) -> Option<ReadOnly> {
    block.facts.readonly
}

/// The refusals of a split that the baseline answers.
pub(super) fn check_split(
    ctx: &Ctx,
    _baseline_lines: &Lines,
    bi: usize,
    line: u32,
) -> Result<(), StetError> {
    let Some(par) = ctx.bpars.get(bi) else {
        return Ok(());
    };
    if let Some(reason) = readonly_reason(par.block) {
        return Err(refuse::edit_refused(
            ctx.file,
            line,
            ErrorCode::ReadonlyRegion,
            format!(
                "{} cannot be split, because {}.",
                par.r,
                refuse::readonly_why(reason)
            ),
            "Leave the paragraph as it is.",
        ));
    }
    Ok(())
}

/// The refusals of a merge that the baseline answers: for each neighbouring pair of the parts.
pub(super) fn check_merge(
    ctx: &Ctx,
    baseline_lines: &Lines,
    merge: &MergeEdit,
    line: u32,
) -> Result<(), StetError> {
    for pair in merge.parts.windows(2) {
        let (Some(&a), Some(&b)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        let (Some(pa), Some(pb)) = (ctx.bpars.get(a), ctx.bpars.get(b)) else {
            continue;
        };
        for par in [pa, pb] {
            if let Some(reason) = par.block.facts.readonly {
                return Err(refuse::edit_refused(
                    ctx.file,
                    line,
                    ErrorCode::ReadonlyRegion,
                    format!(
                        "{} cannot be joined with another, because {}.",
                        par.r,
                        refuse::readonly_why(reason)
                    ),
                    match reason {
                        ReadOnly::DeletedBlock => "Accept or reject the deletion first.",
                        _ => "Leave the paragraphs as they are.",
                    },
                ));
            }
            if carries_section_break(ctx, par.line) {
                return Err(refuse::edit_refused(
                    ctx.file,
                    line,
                    ErrorCode::ReadonlyRegion,
                    format!(
                        "{} carries a section break, and joining would move or drop the break.",
                        par.r
                    ),
                    "Edit the two paragraphs without merging them.",
                ));
            }
            if spans_a_field(par.block) {
                return Err(refuse::edit_refused(
                    ctx.file,
                    line,
                    ErrorCode::ReadonlyRegion,
                    format!(
                        "{} begins or ends a field that spans paragraphs, and joining would cut the field.",
                        par.r
                    ),
                    "Edit the paragraphs without merging them.",
                ));
            }
        }
        if pa.block.tags.iter().any(|k| {
            matches!(
                k,
                Key::Mark {
                    kind: crate::projection::model::MarkKind::Deleted
                        | crate::projection::model::MarkKind::MovedFrom,
                    ..
                }
            )
        }) {
            return Err(refuse::edit_refused(
                ctx.file,
                line,
                ErrorCode::ReadonlyRegion,
                format!(
                    "the mark of {} is already deleted: a join is pending there.",
                    pa.r
                ),
                "Accept or reject that change first.",
            ));
        }
        if !neighbours(ctx, baseline_lines, pa.line, pb.line) {
            return Err(refuse::edit_refused(
                ctx.file,
                line,
                ErrorCode::CrossBlock,
                format!(
                    "{} and {} are not neighbours: something lies between them (a table, a row, a cell or a note).",
                    pa.r, pb.r
                ),
                "Join paragraphs that follow each other in one place.",
            ));
        }
    }
    Ok(())
}

fn carries_section_break(ctx: &Ctx, line: usize) -> bool {
    matches!(
        ctx.bf.lines.get(line + 1),
        Some(DraftLine::Structure(s)) if matches!(s.key, Key::Section(_))
    )
}

fn spans_a_field(block: &DraftBlock) -> bool {
    block
        .seams
        .iter()
        .any(|s| matches!(&s.key, Key::Field(f) if !(f.opens && f.closes)))
}

/// True if the paragraphs at these baseline lines are neighbours in one container: only deleted
/// paragraphs between them, and in a table the second continues the first's cell.
fn neighbours(ctx: &Ctx, baseline_lines: &Lines, a: usize, b: usize) -> bool {
    let between = ctx
        .bf
        .lines
        .iter()
        .skip(a + 1)
        .take(b.saturating_sub(a + 1));
    let only_deleted = between.into_iter().all(|line| {
        matches!(line, DraftLine::Block(block) if block.facts.readonly == Some(ReadOnly::DeletedBlock))
    });
    let second_continues = matches!(
        ctx.bf.lines.get(b),
        Some(DraftLine::Block(block)) if block.tags.contains(&Key::CellPlus)
    );
    only_deleted && (baseline_lines.depth_at(a) == 0 || second_continues)
}
