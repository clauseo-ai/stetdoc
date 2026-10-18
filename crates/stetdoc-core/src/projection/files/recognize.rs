//! Exact splits and merges among the lines of one file (design 6.4, step 6).
//!
//! The lines that cut a paragraph into pieces, or join paragraphs, with the text conserved exactly, are
//! found here, before the ordinary comparison, and are taken out of the steps that follow. The search is
//! by gap: the baseline paragraphs that the edited file does not have, and the lines that it has and the
//! baseline does not, between two paragraphs that are in both. Recognized means committed to: the edit
//! must not turn into a deletion and an insertion of the same text when the compiler would refuse it,
//! so the refusals that the baseline can answer are made here, and the rest come back from the compiler.

use super::align::Ctx;
use super::anchor::Lines;
use super::checks::{check_merge, check_split};
use super::matched::same;
use super::refuse;
use super::split::{Cut, Merge, Split, match_merge, match_split};
use crate::api::error::StetError;
use crate::projection::draft::DraftBlock;

/// The most pieces and parts that one recognition looks at, and the most removed paragraphs and new lines
/// that one gap pairs, so that a hostile edit costs a bounded time.
const MOST: usize = 64;

/// A paragraph cut into lines.
#[derive(Debug)]
pub(super) struct SplitEdit {
    /// The baseline paragraph.
    pub bi: usize,
    /// The cuts, in order.
    pub cuts: Vec<Cut>,
    /// The edited lines that are the pieces: the paragraph's own line first when it is a piece.
    pub pieces: Vec<usize>,
}

/// Paragraphs joined into one line.
#[derive(Debug)]
pub(super) struct MergeEdit {
    /// The baseline paragraphs, first to last. The first is the one that stays.
    pub parts: Vec<usize>,
    /// The edited line.
    pub line: usize,
    /// Whitespace to insert at the join of two parts, and where in the `Final` text of the part it goes in.
    pub insert: Option<(u32, String)>,
    /// The whitespace goes into the second part (at its start, before the merge) and not at the end of the
    /// first, because the first ends in a seam that takes no room.
    pub in_second: bool,
}

/// What the search found.
#[derive(Debug, Default)]
pub(super) struct Found {
    /// The splits.
    pub splits: Vec<SplitEdit>,
    /// The merges.
    pub merges: Vec<MergeEdit>,
    /// For each paragraph line of the edited file, whether a split or merge took it.
    pub used_e: Vec<bool>,
    /// For each paragraph of the baseline, whether a split or merge took it.
    pub used_b: Vec<bool>,
    /// For each paragraph line of the edited file, the gap it is in.
    pub gap_e: Vec<usize>,
    /// For each paragraph of the baseline, the gap it is in.
    pub gap_b: Vec<usize>,
}

impl Found {
    fn take_e(&mut self, k: usize) {
        if let Some(used) = self.used_e.get_mut(k) {
            *used = true;
        }
    }

    fn take_b(&mut self, b: usize) {
        if let Some(used) = self.used_b.get_mut(b) {
            *used = true;
        }
    }

    fn free_e(&self, k: usize) -> bool {
        self.used_e.get(k) == Some(&false)
    }

    fn free_b(&self, b: usize) -> bool {
        self.used_b.get(b) == Some(&false)
    }
}

/// Finds the exact splits and merges.
pub(super) fn recognize(ctx: &Ctx) -> Result<Found, StetError> {
    let mut found = Found {
        used_e: vec![false; ctx.elines.len()],
        used_b: vec![false; ctx.bpars.len()],
        gap_e: vec![0; ctx.elines.len()],
        gap_b: vec![0; ctx.bpars.len()],
        ..Found::default()
    };
    let baseline_lines = Lines::new(&ctx.bf.lines, ctx.bf.common_style.as_deref());
    let matched: Vec<usize> = (0..ctx.elines.len())
        .filter(|&k| ctx.elines.get(k).is_some_and(|e| e.bi.is_some()))
        .collect();
    let mut bounds: Vec<Option<usize>> = vec![None];
    bounds.extend(matched.iter().copied().map(Some));
    bounds.push(None);
    for (gap, pair) in bounds.windows(2).enumerate() {
        if let [prev, next] = pair {
            search_gap(ctx, &baseline_lines, &mut found, gap, *prev, *next)?;
        }
    }
    Ok(found)
}

fn bi_of(ctx: &Ctx, k: Option<usize>) -> Option<usize> {
    k.and_then(|k| ctx.elines.get(k)).and_then(|e| e.bi)
}

/// True if the paragraph lines `a` and `b` of the edited file are on consecutive lines of the file.
fn consecutive(ctx: &Ctx, a: usize, b: usize) -> bool {
    match (ctx.elines.get(a), ctx.elines.get(b)) {
        (Some(x), Some(y)) => y.line == x.line + 1,
        _ => false,
    }
}

fn block_of<'a>(ctx: &'a Ctx, k: usize) -> Option<&'a DraftBlock> {
    ctx.elines.get(k).map(|e| e.block)
}

fn par_block<'a>(ctx: &'a Ctx, b: usize) -> Option<&'a DraftBlock> {
    ctx.bpars.get(b).map(|p| p.block)
}

/// The run of consecutive free new lines that starts at `first` among `news`.
fn run_from(ctx: &Ctx, found: &Found, news: &[usize], first: usize) -> Vec<usize> {
    let mut run: Vec<usize> = Vec::new();
    for &k in news.iter().skip(first) {
        let follows = run.last().is_none_or(|&last| consecutive(ctx, last, k));
        if !follows || !found.free_e(k) {
            break;
        }
        run.push(k);
        if run.len() >= MOST {
            break;
        }
    }
    run
}

fn search_gap(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    gap: usize,
    prev: Option<usize>,
    next: Option<usize>,
) -> Result<(), StetError> {
    let news: Vec<usize> = (prev.map_or(0, |p| p + 1)..next.unwrap_or(ctx.elines.len())).collect();
    let lo = bi_of(ctx, prev).map_or(0, |b| b + 1);
    let hi = bi_of(ctx, next).unwrap_or(ctx.bpars.len());
    let rem: Vec<usize> = (lo..hi)
        .filter(|&b| {
            ctx.bpars
                .get(b)
                .is_some_and(|p| !ctx.in_removed.contains(&p.line))
        })
        .collect();
    for &k in &news {
        if let Some(slot) = found.gap_e.get_mut(k) {
            *slot = gap;
        }
    }
    for &b in &rem {
        if let Some(slot) = found.gap_b.get_mut(b) {
            *slot = gap;
        }
    }
    if let Some(p) = prev {
        natural_split(ctx, baseline_lines, found, p, &news)?;
        natural_merge(ctx, baseline_lines, found, p, &rem)?;
    }
    if let Some(n) = next {
        later_merge(ctx, baseline_lines, found, n, &rem)?;
        ref_on_later_line(ctx, n, &news)?;
    }
    paired(ctx, baseline_lines, found, &rem, &news)
}

/// The paragraph's own line is the first piece, and new lines follow it.
fn natural_split(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    p: usize,
    news: &[usize],
) -> Result<(), StetError> {
    let (Some(bi), true) = (bi_of(ctx, Some(p)), found.free_e(p)) else {
        return Ok(());
    };
    let (Some(par), Some(first)) = (par_block(ctx, bi), block_of(ctx, p)) else {
        return Ok(());
    };
    if first.tags != par.tags || !consecutive_after(ctx, p, news) {
        return Ok(());
    }
    let run = run_from(ctx, found, news, 0);
    for k in 1..=run.len() {
        let mut pieces: Vec<&DraftBlock> = vec![first];
        pieces.extend(run.iter().take(k).filter_map(|&n| block_of(ctx, n)));
        match match_split(par, &pieces) {
            Split::No => {}
            Split::Ambiguous { piece, seam } => {
                let line = piece_line(ctx, p, &run, piece);
                if let Some(entry) = ctx.bpars.get(bi) {
                    return Err(refuse::split_position_ambiguous(
                        ctx.file, line, entry.r, &seam,
                    ));
                }
                return Ok(());
            }
            Split::Yes(cuts) => {
                let mut lines = vec![p];
                lines.extend(run.iter().take(k).copied());
                return record_split(ctx, baseline_lines, found, bi, cuts, lines);
            }
        }
    }
    Ok(())
}

fn consecutive_after(ctx: &Ctx, p: usize, news: &[usize]) -> bool {
    news.first()
        .is_some_and(|&first| first == p + 1 && consecutive(ctx, p, first))
}

fn piece_line(ctx: &Ctx, p: usize, run: &[usize], piece: usize) -> u32 {
    let k = if piece == 0 {
        Some(p)
    } else {
        run.get(piece - 1).copied()
    };
    k.and_then(|k| ctx.elines.get(k))
        .map_or(0, |e| e.block.line)
}

fn record_split(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    bi: usize,
    cuts: Vec<Cut>,
    lines: Vec<usize>,
) -> Result<(), StetError> {
    let line = lines
        .get(1)
        .and_then(|&k| ctx.elines.get(k))
        .map_or(0, |e| e.block.line);
    check_split(ctx, baseline_lines, bi, line)?;
    for &k in &lines {
        found.take_e(k);
    }
    found.take_b(bi);
    found.splits.push(SplitEdit {
        bi,
        cuts,
        pieces: lines,
    });
    Ok(())
}

/// The paragraph's own line has absorbed the paragraphs that follow it.
fn natural_merge(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    p: usize,
    rem: &[usize],
) -> Result<(), StetError> {
    let (Some(bi), true) = (bi_of(ctx, Some(p)), found.free_e(p)) else {
        return Ok(());
    };
    let (Some(par), Some(line)) = (par_block(ctx, bi), block_of(ctx, p)) else {
        return Ok(());
    };
    if line.tags != par.tags || same(par, line) {
        return Ok(());
    }
    let run: Vec<usize> = rem
        .iter()
        .copied()
        .enumerate()
        .take_while(|&(t, b)| b == bi + 1 + t && found.free_b(b))
        .map(|(_, b)| b)
        .take(MOST)
        .collect();
    for m in 1..=run.len() {
        let mut parts: Vec<usize> = vec![bi];
        parts.extend(run.iter().take(m).copied());
        if let Some(merge) = try_merge(ctx, &parts, p, line) {
            return record_merge(ctx, baseline_lines, found, merge?);
        }
    }
    Ok(())
}

/// The line has the ref of a later paragraph, and the ones before it are gone.
fn later_merge(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    n: usize,
    rem: &[usize],
) -> Result<(), StetError> {
    let (Some(bi), true) = (bi_of(ctx, Some(n)), found.free_e(n)) else {
        return Ok(());
    };
    let (Some(par), Some(line)) = (par_block(ctx, bi), block_of(ctx, n)) else {
        return Ok(());
    };
    if line.tags != par.tags || same(par, line) {
        return Ok(());
    }
    let mut run: Vec<usize> = Vec::new();
    for &b in rem.iter().rev() {
        if b + 1 + run.len() != bi || !found.free_b(b) || run.len() >= MOST {
            break;
        }
        run.push(b);
    }
    for s in 1..=run.len() {
        let mut parts: Vec<usize> = run.iter().take(s).rev().copied().collect();
        parts.push(bi);
        if let Some(merge) = try_merge(ctx, &parts, n, line) {
            return record_merge(ctx, baseline_lines, found, merge?);
        }
    }
    Ok(())
}

fn try_merge(
    ctx: &Ctx,
    parts: &[usize],
    line_k: usize,
    line: &DraftBlock,
) -> Option<Result<MergeEdit, StetError>> {
    let blocks: Vec<&DraftBlock> = parts.iter().filter_map(|&b| par_block(ctx, b)).collect();
    match match_merge(&blocks, line) {
        Merge::No => None,
        Merge::Yes { insert } => Some(Ok(MergeEdit {
            parts: parts.to_vec(),
            line: line_k,
            insert,
            in_second: false,
        })),
        Merge::YesInSecond { text } => Some(Ok(MergeEdit {
            parts: parts.to_vec(),
            line: line_k,
            insert: Some((0, text)),
            in_second: true,
        })),
        Merge::Ambiguous(seam) => parts
            .first()
            .and_then(|&b| ctx.bpars.get(b))
            .map(|p| Err(refuse::merge_ambiguous(ctx.file, line.line, p.r, &seam))),
    }
}

fn record_merge(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    merge: MergeEdit,
) -> Result<(), StetError> {
    let line = ctx.elines.get(merge.line).map_or(0, |e| e.block.line);
    check_merge(ctx, baseline_lines, &merge, line)?;
    found.take_e(merge.line);
    for &b in &merge.parts {
        found.take_b(b);
    }
    found.merges.push(merge);
    Ok(())
}

/// The agent kept the ref of the paragraph on a later line than the first piece.
fn ref_on_later_line(ctx: &Ctx, n: usize, news: &[usize]) -> Result<(), StetError> {
    let (Some(bi), Some(last)) = (bi_of(ctx, Some(n)), block_of(ctx, n)) else {
        return Ok(());
    };
    let Some(par) = par_block(ctx, bi) else {
        return Ok(());
    };
    let mut run: Vec<usize> = Vec::new();
    let mut after = n;
    for &k in news.iter().rev() {
        if k + 1 != after || !consecutive(ctx, k, after) || run.len() >= MOST {
            break;
        }
        run.push(k);
        after = k;
        let pieces: Vec<&DraftBlock> = run
            .iter()
            .rev()
            .filter_map(|&j| block_of(ctx, j))
            .chain(std::iter::once(last))
            .collect();
        if matches!(
            match_split(par, &pieces),
            Split::Yes(_) | Split::Ambiguous { .. }
        ) {
            let r = ctx.bpars.get(bi).map(|p| p.r);
            if let Some(r) = r {
                return Err(refuse::split_ref_on_later_line(ctx.file, last.line, r));
            }
        }
    }
    Ok(())
}

/// Splits and merges in a gap that the paragraphs on its edges have no part in: a paragraph that was
/// removed and written again as lines, and lines written for paragraphs that were removed.
fn paired(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    rem: &[usize],
    news: &[usize],
) -> Result<(), StetError> {
    let rem_free: Vec<usize> = rem.iter().copied().filter(|&b| found.free_b(b)).collect();
    let news_free: Vec<usize> = news.iter().copied().filter(|&k| found.free_e(k)).collect();
    let (mut i, mut cur) = (0usize, 0usize);
    while i < rem_free.len().min(MOST) {
        let Some(&b) = rem_free.get(i) else {
            break;
        };
        let mut advanced = false;
        let mut j = cur;
        while j < news_free.len().min(MOST) {
            if let Some((used_rem, used_news)) =
                try_pair(ctx, baseline_lines, found, &rem_free, i, &news_free, j)?
            {
                i += used_rem;
                cur = j + used_news;
                advanced = true;
                break;
            }
            j += 1;
        }
        let _ = b;
        if !advanced {
            i += 1;
        }
    }
    Ok(())
}

/// Tries the removed paragraph `rem[i]` against the new lines from `news[j]` as a split, and the new line
/// `news[j]` against the removed paragraphs from `rem[i]` as a merge. Returns how many of each it took.
fn try_pair(
    ctx: &Ctx,
    baseline_lines: &Lines,
    found: &mut Found,
    rem: &[usize],
    i: usize,
    news: &[usize],
    j: usize,
) -> Result<Option<(usize, usize)>, StetError> {
    let (Some(&b), Some(&first_new)) = (rem.get(i), news.get(j)) else {
        return Ok(None);
    };
    // a split: the removed paragraph written again as lines
    if let Some(par) = par_block(ctx, b) {
        let run = run_from(ctx, found, news, j);
        for k in 2..=run.len() {
            let pieces: Vec<&DraftBlock> = run
                .iter()
                .take(k)
                .filter_map(|&n| block_of(ctx, n))
                .collect();
            match match_split(par, &pieces) {
                Split::No => {}
                Split::Ambiguous { piece, seam } => {
                    let line = run
                        .get(piece)
                        .and_then(|&n| ctx.elines.get(n))
                        .map_or(0, |e| e.block.line);
                    if let Some(entry) = ctx.bpars.get(b) {
                        return Err(refuse::split_position_ambiguous(
                            ctx.file, line, entry.r, &seam,
                        ));
                    }
                    return Ok(None);
                }
                Split::Yes(cuts) => {
                    let lines: Vec<usize> = run.iter().take(k).copied().collect();
                    record_split(ctx, baseline_lines, found, b, cuts, lines)?;
                    return Ok(Some((1, k)));
                }
            }
        }
    }
    // a merge: removed paragraphs written again as one line
    let Some(line) = block_of(ctx, first_new) else {
        return Ok(None);
    };
    let mut available = 0usize;
    while available < MOST
        && rem.get(i + available) == Some(&(b + available))
        && found.free_b(b + available)
    {
        available += 1;
    }
    for m in 2..=available {
        let parts: Vec<usize> = (0..m).map(|t| b + t).collect();
        if let Some(merge) = try_merge(ctx, &parts, first_new, line) {
            record_merge(ctx, baseline_lines, found, merge?)?;
            return Ok(Some((m, 1)));
        }
    }
    Ok(None)
}
