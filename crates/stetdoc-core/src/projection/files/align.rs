//! Aligning an edited file with its baseline (design 6.4).
//!
//! The steps run in this order, and each can end the plan: refs, order, structure, then (in
//! `recognize.rs` and `plan.rs`) splits and merges, matched paragraphs, removed blocks and new lines.
//! Blocks are aligned by ref and never by position. This file is the first three steps and the loop over
//! the files.

use super::recognize::recognize;
use super::refuse::{self, Problems};
use super::{Baseline, Outside, Plan};
use crate::api::error::StetError;
use crate::projection::draft::{DraftBlock, DraftFile, DraftLine};
use crate::projection::keys::Key;
use crate::refs::BlockRef;
use std::collections::{HashMap, HashSet};

/// A paragraph of the baseline.
pub(super) struct Par<'a> {
    /// Its line in the baseline file.
    pub line: usize,
    /// Its draft.
    pub block: &'a DraftBlock,
    /// Its ref.
    pub r: BlockRef,
}

/// A paragraph line of the edited file.
pub(super) struct ELine<'a> {
    /// Its line in the edited file's list of lines.
    pub line: usize,
    /// Its draft.
    pub block: &'a DraftBlock,
    /// The baseline paragraph it is, by ref.
    pub bi: Option<usize>,
}

/// What every step of one file needs.
pub(super) struct Ctx<'a> {
    pub file: &'a str,
    pub bf: &'a DraftFile,
    pub ef: &'a DraftFile,
    pub baseline: &'a Baseline,
    pub outside: &'a dyn Outside,
    pub bpars: Vec<Par<'a>>,
    pub elines: Vec<ELine<'a>>,
    /// The lines of the baseline that sit in a table or a row that the edit removes whole.
    pub in_removed: HashSet<usize>,
    /// The head lines of the removed tables and rows, outermost only.
    pub removed_heads: Vec<usize>,
}

/// The ref of a line, if it has one: a paragraph, a table or a row.
fn ref_of(line: &DraftLine) -> Option<BlockRef> {
    match line {
        DraftLine::Block(block) => block.r#ref,
        DraftLine::Structure(s) => match &s.key {
            Key::Table(t) => t.r#ref,
            Key::Row(r) => r.r#ref,
            _ => None,
        },
    }
}

fn line_number(line: &DraftLine) -> u32 {
    match line {
        DraftLine::Block(block) => block.line,
        DraftLine::Structure(s) => s.line,
    }
}

fn is_comments(file: &DraftFile) -> bool {
    file.lines.iter().find_map(|line| match line {
        DraftLine::Structure(s) => Some(matches!(
            &s.key,
            Key::Unit(crate::projection::keys::UnitKey::Comment { .. })
        )),
        DraftLine::Block(_) => None,
    }) == Some(true)
}

/// Turns the difference between the baseline and the edited files into operations.
pub(crate) fn align(
    baseline: &Baseline,
    edited: &crate::projection::draft::Draft,
    outside: &dyn Outside,
) -> Result<Plan, StetError> {
    let mut given: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for ef in &edited.files {
        if !given.insert(ef.name.as_str()) {
            return Err(refuse::file_twice(&ef.name));
        }
        let Some(bf) = baseline.draft.file(&ef.name) else {
            return Err(refuse::no_such_file(&ef.name));
        };
        if ef.base != bf.base {
            return Err(refuse::stale_header(
                &ef.name,
                ef.base.as_deref().unwrap_or("none"),
                bf.base.as_deref().unwrap_or("none"),
            ));
        }
        // The head is read-only: its common style decides the style of every line with no tag, and so the style
        // that a new line is modelled on.
        if ef.common_style != bf.common_style {
            return Err(refuse::head_changed(
                &ef.name,
                bf.common_style.as_deref(),
                ef.common_style.as_deref(),
            ));
        }
    }
    let mut elsewhere: HashMap<BlockRef, &str> = HashMap::new();
    for file in &baseline.draft.files {
        for line in &file.lines {
            if let Some(r) = ref_of(line) {
                elsewhere.insert(r, &file.name);
            }
        }
    }
    let mut plan = Plan::default();
    for ef in &edited.files {
        let Some(bf) = baseline.draft.file(&ef.name) else {
            continue;
        };
        if is_comments(bf) {
            comments(&ef.name, bf, ef)?;
            continue;
        }
        let mut ctx = refs_order_structure(&ef.name, bf, ef, baseline, outside, &elsewhere)?;
        let found = recognize(&ctx)?;
        super::plan::build(&mut ctx, &found, &mut plan)?;
    }
    Ok(plan)
}

/// `comments.md` is read-only: every difference is `readonly_region`.
fn comments(file: &str, bf: &DraftFile, ef: &DraftFile) -> Result<(), StetError> {
    for (i, line) in ef.lines.iter().enumerate() {
        if bf.lines.get(i) != Some(line) {
            return Err(if i >= bf.lines.len() {
                refuse::new_line_in_comments(file, line_number(line))
            } else {
                refuse::comments_changed(file, line_number(line))
            });
        }
    }
    if bf.lines.len() > ef.lines.len() {
        return Err(refuse::comments_changed(file, 0));
    }
    Ok(())
}

fn refs_order_structure<'a>(
    file: &'a str,
    bf: &'a DraftFile,
    ef: &'a DraftFile,
    baseline: &'a Baseline,
    outside: &'a dyn Outside,
    elsewhere: &HashMap<BlockRef, &str>,
) -> Result<Ctx<'a>, StetError> {
    let mut b_line_of: HashMap<BlockRef, usize> = HashMap::new();
    for (i, line) in bf.lines.iter().enumerate() {
        if let Some(r) = ref_of(line) {
            b_line_of.insert(r, i);
        }
    }
    step_refs(file, ef, &b_line_of, elsewhere, outside)?;
    step_order(file, ef, &b_line_of)?;
    let present: HashSet<BlockRef> = ef.lines.iter().filter_map(ref_of).collect();
    let (in_removed, removed_heads) = removed_groups(file, bf, ef, &present)?;
    let matched = step_structure(file, bf, ef, &in_removed, &b_line_of)?;
    step_interleaving(file, ef, &b_line_of, &matched)?;
    let mut bpars = Vec::new();
    let mut bpar_of = HashMap::new();
    for (line, entry) in bf.lines.iter().enumerate() {
        if let DraftLine::Block(block) = entry
            && let Some(r) = block.r#ref
        {
            bpar_of.insert(r, bpars.len());
            bpars.push(Par { line, block, r });
        }
    }
    let elines = ef
        .lines
        .iter()
        .enumerate()
        .filter_map(|(line, entry)| match entry {
            DraftLine::Block(block) => Some(ELine {
                line,
                block,
                bi: block.r#ref.and_then(|r| bpar_of.get(&r).copied()),
            }),
            DraftLine::Structure(_) => None,
        })
        .collect();
    Ok(Ctx {
        file,
        bf,
        ef,
        baseline,
        outside,
        bpars,
        elines,
        in_removed,
        removed_heads,
    })
}

/// Step 3: a duplicated ref, a ref of another file, a ref that is gone or never was.
fn step_refs(
    file: &str,
    ef: &DraftFile,
    b_line_of: &HashMap<BlockRef, usize>,
    elsewhere: &HashMap<BlockRef, &str>,
    outside: &dyn Outside,
) -> Result<(), StetError> {
    let mut problems = Problems::default();
    let mut seen: HashMap<BlockRef, u32> = HashMap::new();
    for line in &ef.lines {
        let Some(r) = ref_of(line) else {
            continue;
        };
        let number = line_number(line);
        if let Some(first) = seen.get(&r) {
            problems.push(
                refuse::duplicate_ref(file, r, *first, number),
                "Give each paragraph one line. To add a paragraph, write a line with no ref.",
                (file, number),
            );
            continue;
        }
        seen.insert(r, number);
        if b_line_of.contains_key(&r) {
            continue;
        }
        let refusal = match elsewhere.get(&r) {
            Some(other) => refuse::other_file(file, number, r, other),
            None => match outside.fate(r) {
                super::Fate::Unknown { range } => refuse::unknown_ref(file, number, r, &range),
                super::Fate::Gone { what } => refuse::gone_ref(file, number, r, &what),
            },
        };
        problems.push(
            refusal,
            "Use refs exactly as the file shows them.",
            (file, number),
        );
    }
    problems.finish()
}

/// Step 4: the refs of the edited lines go up in the baseline's order, or a block was moved.
fn step_order(
    file: &str,
    ef: &DraftFile,
    b_line_of: &HashMap<BlockRef, usize>,
) -> Result<(), StetError> {
    let mut last: Option<(usize, BlockRef)> = None;
    for line in &ef.lines {
        let Some(r) = ref_of(line) else {
            continue;
        };
        let Some(&at) = b_line_of.get(&r) else {
            continue;
        };
        if let Some((before, before_ref)) = last
            && at < before
        {
            return Err(refuse::moved(
                file,
                line_number(line),
                &r.to_string(),
                &before_ref.to_string(),
            ));
        }
        last = Some((at, r));
    }
    Ok(())
}

/// The end of the table that starts at line `start`, exclusive.
fn table_end(lines: &[DraftLine], start: usize) -> usize {
    let mut depth = 0u32;
    for (i, line) in lines.iter().enumerate().skip(start) {
        if let DraftLine::Structure(s) = line {
            match &s.key {
                Key::Table(_) => depth += 1,
                Key::EndTable => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i + 1;
                    }
                }
                _ => {}
            }
        }
    }
    lines.len()
}

/// The end of the row that starts at line `start`, exclusive: the next row of the same table or the end of
/// the table, whichever comes first.
fn row_end(lines: &[DraftLine], start: usize) -> usize {
    let mut depth = 0u32;
    for (i, line) in lines.iter().enumerate().skip(start + 1) {
        if let DraftLine::Structure(s) = line {
            match &s.key {
                Key::Table(_) => depth += 1,
                Key::EndTable if depth == 0 => return i,
                Key::EndTable => depth -= 1,
                Key::Row(_) if depth == 0 => return i,
                _ => {}
            }
        }
    }
    lines.len()
}

/// The tables and rows of the baseline whose line is not in the edited file: each is a group that the
/// edit removes whole. A group whose paragraphs are still in the file is unbalanced.
fn removed_groups(
    file: &str,
    bf: &DraftFile,
    ef: &DraftFile,
    present: &HashSet<BlockRef>,
) -> Result<(HashSet<usize>, Vec<usize>), StetError> {
    let mut in_removed = HashSet::new();
    let mut heads = Vec::new();
    let mut i = 0;
    while let Some(line) = bf.lines.get(i) {
        let group = match line {
            DraftLine::Structure(s) => match &s.key {
                Key::Table(t) => t
                    .r#ref
                    .filter(|r| !present.contains(r))
                    .map(|_| table_end(&bf.lines, i)),
                Key::Row(r) => r
                    .r#ref
                    .filter(|r| !present.contains(r))
                    .map(|_| row_end(&bf.lines, i)),
                _ => None,
            },
            DraftLine::Block(_) => None,
        };
        let Some(end) = group else {
            i += 1;
            continue;
        };
        heads.push(i);
        for k in i..end {
            in_removed.insert(k);
            if k > i
                && let Some(entry) = bf.lines.get(k)
                && let Some(r) = ref_of(entry)
                && present.contains(&r)
            {
                let head = ref_of(line).map_or_else(String::new, |h| h.to_string());
                let number = ef
                    .lines
                    .iter()
                    .find(|l| ref_of(l) == Some(r))
                    .map_or(0, line_number);
                return Err(refuse::structure_unbalanced(
                    file,
                    number,
                    &format!(
                        "{head} was removed, but {r}, which belongs to it, is still here. A table or a row must be removed whole, with all its paragraphs and its end line."
                    ),
                ));
            }
        }
        i = end;
    }
    Ok((in_removed, heads))
}

/// Step 5: every structure line of the baseline is present and equal, in order, except a group removed
/// whole. Returns, for each structure line of the edited file, the baseline line it is.
fn step_structure(
    file: &str,
    bf: &DraftFile,
    ef: &DraftFile,
    in_removed: &HashSet<usize>,
    b_line_of: &HashMap<BlockRef, usize>,
) -> Result<Vec<(usize, usize)>, StetError> {
    let expected: Vec<(usize, &Key)> = bf
        .lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !in_removed.contains(i))
        .filter_map(|(i, line)| match line {
            DraftLine::Structure(s) => Some((i, &s.key)),
            DraftLine::Block(_) => None,
        })
        .collect();
    let edited: Vec<(usize, &Key, u32)> = ef
        .lines
        .iter()
        .enumerate()
        .filter_map(|(i, line)| match line {
            DraftLine::Structure(s) => Some((i, &s.key, s.line)),
            DraftLine::Block(_) => None,
        })
        .collect();
    let mut matched = Vec::new();
    let (mut x, mut e) = (0usize, 0usize);
    while x < expected.len() || e < edited.len() {
        match (expected.get(x), edited.get(e)) {
            (Some((bi, want)), Some((ei, got, number))) if want == got => {
                matched.push((*ei, *bi));
                x += 1;
                e += 1;
                let _ = number;
            }
            (Some((_, want)), Some((_, got, number))) => {
                // the edited line is the next expected one: the expected one was removed
                if expected.get(x + 1).is_some_and(|(_, next)| next == got) {
                    return Err(missing_structure(file, want));
                }
                // the line after the edited one is the expected one: the edited one is extra
                if edited.get(e + 1).is_some_and(|(_, next, _)| next == want) {
                    return Err(extra_structure(file, *number, got, b_line_of));
                }
                if is_new_structure(got, b_line_of) {
                    return Err(extra_structure(file, *number, got, b_line_of));
                }
                return Err(refuse::structure_changed(file, *number, want, got));
            }
            (Some((_, want)), None) => return Err(missing_structure(file, want)),
            (None, Some((_, got, number))) => {
                return Err(extra_structure(file, *number, got, b_line_of));
            }
            (None, None) => break,
        }
    }
    Ok(matched)
}

fn is_new_structure(key: &Key, b_line_of: &HashMap<BlockRef, usize>) -> bool {
    match key {
        Key::Table(t) => t.r#ref.is_none_or(|r| !b_line_of.contains_key(&r)),
        Key::Row(r) => r.r#ref.is_none_or(|r| !b_line_of.contains_key(&r)),
        _ => false,
    }
}

fn missing_structure(file: &str, was: &Key) -> StetError {
    match was {
        Key::Row(_) | Key::Table(_) => refuse::structure_unbalanced(
            file,
            0,
            &format!(
                "{} is missing, and its paragraphs are still here. A table or a row must be removed whole.",
                was.describe()
            ),
        ),
        Key::EndTable => refuse::structure_unbalanced(
            file,
            0,
            "the end of a table (`{/table}`) is missing. A table must keep its end line, or be removed whole.",
        ),
        Key::Unit(_) | Key::Anchored(_) => refuse::note_or_comment_removed(file, was),
        _ => refuse::structure_missing(file, was),
    }
}

fn extra_structure(
    file: &str,
    line: u32,
    key: &Key,
    b_line_of: &HashMap<BlockRef, usize>,
) -> StetError {
    match key {
        Key::EndTable => refuse::structure_unbalanced(
            file,
            line,
            "a `{/table}` with no table before it. A table has one end line.",
        ),
        Key::Table(_) | Key::Row(_) if is_new_structure(key, b_line_of) => refuse::new_structure(
            file,
            line,
            "a new table or row line is a request to create structure.",
        ),
        _ => refuse::new_structure(file, line, &format!("{} was added.", key.describe())),
    }
}

/// The paragraphs, tables, rows and structure lines of the edited file that are in the baseline come in the
/// baseline's order: a paragraph moved across the end of a table, a row or a section was moved.
fn step_interleaving(
    file: &str,
    ef: &DraftFile,
    b_line_of: &HashMap<BlockRef, usize>,
    matched: &[(usize, usize)],
) -> Result<(), StetError> {
    let structure: HashMap<usize, usize> = matched.iter().copied().collect();
    let mut last: Option<usize> = None;
    for (e, line) in ef.lines.iter().enumerate() {
        let b = match ref_of(line) {
            Some(r) => b_line_of.get(&r).copied(),
            None => structure.get(&e).copied(),
        };
        let Some(b) = b else {
            continue;
        };
        if let Some(before) = last
            && b < before
        {
            let what = match ref_of(line) {
                Some(r) => r.to_string(),
                None => "a structure line".to_string(),
            };
            return Err(refuse::moved_across(file, line_number(line), &what));
        }
        last = Some(b);
    }
    Ok(())
}
