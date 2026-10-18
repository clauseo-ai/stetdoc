//! `projection_*` and `index_*`: what an agent's calls cost on a 300-page document (design 4.7 and 6.9 of
//! projection and index): the first view after `open`, a page with margin notes, the next page, the readable
//! files, and an edit of them turned into operations, which builds the model of the whole story as its baseline.
//!
//! The **cold** rows prepare a state that has just been opened, so the run pays for everything a first call
//! builds (the document facts, the cascade, the numbers, the clause table, the edges). The **warm** rows prepare
//! a state that a first call has already used. `to_batch` has no cold and warm: it is always the whole story.

use crate::api::files::ReadableFiles;
use crate::api::limits::Limits;
use crate::api::view::{Budget, Cursor, FormatId, Scope, ViewSpec};
use crate::projection::{lookup, render_files, to_batch, view};
use crate::state::State;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

fn open(input: &Input) -> Result<State, String> {
    State::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())
}

/// The ref of the paragraph that is `part` per cent of the way through the main part.
fn ref_at(state: &State, percent: usize) -> Result<String, String> {
    let parts = state.parts();
    let layout = parts.layout(false).map_err(|e| e.to_string())?;
    let story = layout.stories.first().ok_or("the document has no story")?;
    let paragraphs: Vec<_> = story
        .blocks
        .iter()
        .filter(|b| b.kind == crate::wordml::BlockKind::Paragraph)
        .collect();
    let at = paragraphs.len().saturating_mul(percent) / 100;
    let block = paragraphs
        .get(at)
        .or(paragraphs.first())
        .ok_or("the document has no paragraph")?;
    parts
        .ref_of(&block.key)
        .map(|r| r.to_string())
        .ok_or_else(|| "the block has no ref".to_string())
}

fn range_from(state: &State, after: Option<Cursor>) -> Result<ViewSpec, String> {
    Ok(ViewSpec {
        scope: Scope::Range {
            from: ref_at(state, 40)?,
            to: None,
        },
        budget: Budget(8000),
        after,
        ..ViewSpec::default()
    })
}

/// The first view after `open`, with the default scope: the whole body if it fits a page, and the outline of
/// it otherwise, which for a 300-page document it does not.
pub(super) const VIEW_COLD: Bench = Bench {
    name: "projection_view_cold",
    limit: Limit::Op(Op::View),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: view_cold,
};

fn view_cold(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    Ok(Box::new(move || {
        let page = view(&state, &ViewSpec::default()).map_err(|e| e.to_string())?;
        Ok(Box::new((state, page)) as Kept)
    }))
}

/// A first range of a page of text with its margin notes and its appendix: the clause table and the edges
/// are built by it. This is the first thing an agent does after the outline.
pub(super) const RANGE_NOTES_COLD: Bench = Bench {
    name: "projection_range_notes_cold",
    limit: Limit::Op(Op::View),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: range_cold,
};

fn range_cold(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    let spec = range_from(&state, None)?;
    Ok(Box::new(move || {
        let page = view(&state, &spec).map_err(|e| e.to_string())?;
        Ok(Box::new((state, page)) as Kept)
    }))
}

/// The page after a page: everything a first call builds is there already, and only the blocks of the page cost.
pub(super) const VIEW_WARM: Bench = Bench {
    name: "projection_view_warm",
    limit: Limit::Op(Op::View),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: view_warm,
};

fn view_warm(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    let first = view(&state, &range_from(&state, None)?).map_err(|e| e.to_string())?;
    let spec = range_from(&state, first.next.clone())?;
    Ok(Box::new(move || {
        let page = view(&state, &spec).map_err(|e| e.to_string())?;
        Ok(Box::new((state, first, page)) as Kept)
    }))
}

/// The readable files of the state, written: the model of every story and its rendering.
pub(super) const RENDER_FILES: Bench = Bench {
    name: "projection_render_files",
    limit: Limit::Op(Op::Export),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: render,
};

fn render(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    Ok(Box::new(move || {
        let files = render_files(&state, &FormatId::stet()).map_err(|e| e.to_string())?;
        Ok(Box::new((state, files)) as Kept)
    }))
}

/// The files, unedited, turned into operations: the baseline is the model of the whole story, rendered and
/// drafted, and the edited files are parsed and aligned with it. There must be no operation. It is part of an
/// edit, so it is held to the budget of one.
pub(super) const TO_BATCH_UNEDITED: Bench = Bench {
    name: "projection_to_batch_unedited",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: batch_unedited,
};

fn batch_unedited(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    let files = render_files(&state, &FormatId::stet()).map_err(|e| e.to_string())?;
    Ok(Box::new(move || {
        let plan = to_batch(&state, &files, &Limits::default()).map_err(|e| e.to_string())?;
        if !plan.ops.is_empty() {
            return Err(format!("{} operations from unedited files", plan.ops.len()));
        }
        Ok(Box::new((state, files, plan)) as Kept)
    }))
}

/// How many paragraphs an edit changes: the budget of an edit is for fifty operations.
const OPERATIONS: usize = 50;

/// Fifty paragraphs of the body changed in the files, turned into fifty operations.
pub(super) const TO_BATCH_EDIT: Bench = Bench {
    name: "projection_to_batch_edit",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: batch_edit,
};

/// The files with fifty lines of `document.md` changed, spread through it. A line is a paragraph with a ref and
/// plain text: it has no marker or tag after its ref that a word at its end could be mistaken for.
pub(super) fn edited(files: &ReadableFiles) -> ReadableFiles {
    let mut out = files.clone();
    if let Some((_, text)) = out.files.iter_mut().find(|(name, _)| name == "document.md") {
        let lines: Vec<&str> = text.lines().collect();
        let candidates: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| {
                l.starts_with('p') && l.contains(' ') && !l.contains('{') && !l.ends_with('\\')
            })
            .map(|(i, _)| i)
            .collect();
        let step = (candidates.len() / OPERATIONS).max(1);
        let chosen: std::collections::HashSet<usize> = candidates
            .iter()
            .step_by(step)
            .take(OPERATIONS)
            .copied()
            .collect();
        let mut changed = String::with_capacity(text.len() + OPERATIONS * 8);
        for (i, line) in lines.iter().enumerate() {
            changed.push_str(line);
            if chosen.contains(&i) {
                changed.push_str(" edited");
            }
            changed.push('\n');
        }
        *text = changed;
    }
    out
}

fn batch_edit(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    let files = edited(&render_files(&state, &FormatId::stet()).map_err(|e| e.to_string())?);
    Ok(Box::new(move || {
        let plan = to_batch(&state, &files, &Limits::default()).map_err(|e| e.to_string())?;
        if plan.ops.is_empty() {
            return Err("fifty edits gave no operation".to_string());
        }
        Ok(Box::new((state, files, plan)) as Kept)
    }))
}

/// The first lookup of a term: the clause table and the definitions are built, and every use of the term is
/// found in one pass over the text. A document without the term gives a miss, after the same work.
pub(super) const LOOKUP_COLD: Bench = Bench {
    name: "index_lookup_cold",
    limit: Limit::Op(Op::View),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: lookup_cold,
};

fn lookup_cold(input: &Input) -> Result<Run, String> {
    let state = open(input)?;
    Ok(Box::new(move || {
        let found = lookup(&state, "Borrower");
        if let Err(e) = &found
            && e.code() != crate::api::error::ErrorCode::NotFound
        {
            return Err(e.to_string());
        }
        Ok(Box::new((state, found.ok())) as Kept)
    }))
}
