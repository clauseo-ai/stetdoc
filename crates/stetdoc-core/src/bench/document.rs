//! `document_*`: what a caller pays through the public interface, `Document` and `Edit` (design `document-and-ops.md` 7.4).
//!
//! A fifty-operation tracked edit on a document just opened (`document_edit_ops`), which pays for keeping the package the
//! document was opened from, the same edit on a document that an edit has already made keep it (`document_edit_ops_again`),
//! its dry run (`document_dry_run`), an edit of the readable files with fifty changed lines (`document_edit_files`), the
//! first view after `open` (`document_view_cold`), the working copy (`document_export`) and its readable files alone
//! (`document_files`), `open` itself (`document_open`), the save of an edited document (`document_save`), and the resume
//! of the working copy an edited document exported (`document_resume`). The operations replace the first
//! word of fifty paragraphs spread through the body, fields or not: since a refresh rebuilds only the blocks touched since
//! the last one, a paragraph with a field costs no more than another.

use super::projection::edited;
use super::support::spread;
use crate::api::batch::Batch;
use crate::api::edit::Edit;
use crate::api::limits::Limits;
use crate::api::options::OpenOptions;
use crate::api::stamp::Stamp;
use crate::api::view::ViewSpec;
use crate::document::Document;
use crate::state::State;
use crate::wordml::{BlockKind, Mode};
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

const MAIN: &str = "word/document.xml";

/// How many operations a batch has: the budget of an edit is for fifty.
const OPERATIONS: usize = 50;

/// A fifty-operation tracked edit on a document just opened.
pub(super) const EDIT_OPS: Bench = Bench {
    name: "document_edit_ops",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: edit_ops,
};

/// The same edit on a document that has been edited once, so that it keeps its original already.
pub(super) const EDIT_OPS_AGAIN: Bench = Bench {
    name: "document_edit_ops_again",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: edit_ops_again,
};

/// The dry run of the fifty-operation edit, on a document just opened.
pub(super) const DRY_RUN: Bench = Bench {
    name: "document_dry_run",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: dry_run,
};

/// The readable files with fifty lines of `document.md` changed, applied as an edit.
pub(super) const EDIT_FILES: Bench = Bench {
    name: "document_edit_files",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: edit_files,
};

/// The first view after `open`.
pub(super) const VIEW_COLD: Bench = Bench {
    name: "document_view_cold",
    limit: Limit::Op(Op::View),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: view_cold,
};

/// The working copy of a document just opened: the readable files, the manifest and the complement.
pub(super) const EXPORT: Bench = Bench {
    name: "document_export",
    limit: Limit::Op(Op::Export),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: export,
};

/// `Document::open`: the block scan, with the parts parsed as they are read.
pub(super) const OPEN: Bench = Bench {
    name: "document_open",
    limit: Limit::Op(Op::Open),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: open_run,
};

/// The save of a document that a fifty-operation edit changed.
pub(super) const SAVE: Bench = Bench {
    name: "document_save",
    limit: Limit::Op(Op::Save),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: save,
};

/// The resume of the working copy that an edited document exported: the manifest, the complement and both packages. It is
/// held to the budget of the export it reverses, 1 s, as design 7.4 asks.
pub(super) const RESUME: Bench = Bench {
    name: "document_resume",
    limit: Limit::Op(Op::Export),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: resume,
};

/// The readable files of a document just opened: the first half of an export.
pub(super) const FILES: Bench = Bench {
    name: "document_files",
    limit: Limit::Op(Op::Export),
    parses: Parses::All,
    inputs: Inputs::Documents,
    prepare: files,
};

fn open(input: &Input) -> Result<Document, String> {
    Document::open(input.docx.clone(), &OpenOptions::default()).map_err(|e| e.to_string())
}

fn stamp() -> Stamp {
    Stamp::new("Bench", "2026-10-06T10:00:00")
}

/// The operations: the first word of fifty paragraphs spread through the body becomes another, by offsets. The refs are
/// read from a state opened on the same bytes, which has the same refs, because refs are a hash of the bytes.
fn operations(input: &Input) -> Result<Vec<String>, String> {
    Ok(candidates(input)?.0)
}

/// The fifty operations, and one more on a paragraph that none of them names, for a first edit.
fn candidates(input: &Input) -> Result<(Vec<String>, String), String> {
    let state = State::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())?;
    let view = state.parts();
    let id = view.id(MAIN).ok_or("the document has no main part")?;
    let story = view.story(id).map_err(|e| e.to_string())?;
    let tree = view.tree(id).map_err(|e| e.to_string())?;
    let mut usable: Vec<String> = Vec::new();
    for (i, e) in story.blocks().iter().enumerate() {
        if e.kind != BlockKind::Paragraph {
            continue;
        }
        let key = story.key(e);
        let Some(p) = story.paragraph(tree, i) else {
            continue;
        };
        let text = p.map().text(Mode::Final).into_owned();
        let Some((start, end)) = first_word(&text) else {
            continue;
        };
        let Some(r) = view.ref_of(&key) else {
            continue;
        };
        usable.push(format!(
            r#"{{"op":"replace","ref":"{r}","start":{start},"end":{end},"with":"REVISED"}}"#
        ));
    }
    let chosen = spread(&usable, OPERATIONS);
    let spare = usable
        .iter()
        .rev()
        .find(|op| !chosen.contains(op))
        .or(usable.first())
        .ok_or("the body has no paragraph with a word in it")?
        .clone();
    Ok((chosen, spare))
}

/// The first word of at least two characters of a text, as a range of characters.
fn first_word(text: &str) -> Option<(usize, usize)> {
    let mut start = None;
    for (i, c) in text.chars().enumerate() {
        match (start, c.is_alphanumeric()) {
            (None, true) => start = Some(i),
            (Some(s), false) if i - s >= 2 => return Some((s, i)),
            (Some(_), false) => start = None,
            _ => {}
        }
    }
    let end = text.chars().count();
    start.filter(|s| end - s >= 2).map(|s| (s, end))
}

fn batch(doc: &Document, ops: &[String]) -> Result<Batch, String> {
    let json = format!(r#"{{"base":"{}","ops":[{}]}}"#, doc.base(), ops.join(","));
    Batch::parse(&json, &Limits::default()).map_err(|e| e.to_string())
}

fn edit_ops(input: &Input) -> Result<Run, String> {
    let mut doc = open(input)?;
    let batch = batch(&doc, &operations(input)?)?;
    Ok(Box::new(move || {
        let receipt = doc
            .edit(Edit::ops(batch, stamp()))
            .map_err(|e| e.to_string())?;
        Ok(Box::new((doc, receipt)) as Kept)
    }))
}

fn edit_ops_again(input: &Input) -> Result<Run, String> {
    let mut doc = open(input)?;
    let (ops, spare) = candidates(input)?;
    // A first edit of another paragraph makes the document keep its original; the timed one is the second.
    doc.edit(Edit::ops(batch(&doc, &[spare])?, stamp()))
        .map_err(|e| e.to_string())?;
    let batch = batch(&doc, &ops)?;
    Ok(Box::new(move || {
        let receipt = doc
            .edit(Edit::ops(batch, stamp()))
            .map_err(|e| e.to_string())?;
        Ok(Box::new((doc, receipt)) as Kept)
    }))
}

fn dry_run(input: &Input) -> Result<Run, String> {
    let mut doc = open(input)?;
    let batch = batch(&doc, &operations(input)?)?;
    Ok(Box::new(move || {
        let receipt = doc
            .edit(Edit::ops(batch, stamp()).dry_run())
            .map_err(|e| e.to_string())?;
        Ok(Box::new((doc, receipt)) as Kept)
    }))
}

fn edit_files(input: &Input) -> Result<Run, String> {
    let mut doc = open(input)?;
    let files = edited(&doc.files().map_err(|e| e.to_string())?);
    Ok(Box::new(move || {
        let receipt = doc
            .edit(Edit::files(files, stamp()))
            .map_err(|e| e.to_string())?;
        Ok(Box::new((doc, receipt)) as Kept)
    }))
}

fn view_cold(input: &Input) -> Result<Run, String> {
    let doc = open(input)?;
    Ok(Box::new(move || {
        let page = doc.view(&ViewSpec::default()).map_err(|e| e.to_string())?;
        Ok(Box::new((doc, page)) as Kept)
    }))
}

fn export(input: &Input) -> Result<Run, String> {
    let doc = open(input)?;
    Ok(Box::new(move || {
        let copy = doc.export().map_err(|e| e.to_string())?;
        Ok(Box::new((doc, copy)) as Kept)
    }))
}

fn open_run(input: &Input) -> Result<Run, String> {
    let bytes = input.docx.clone();
    Ok(Box::new(move || {
        let doc = Document::open(bytes, &OpenOptions::default()).map_err(|e| e.to_string())?;
        Ok(Box::new(doc) as Kept)
    }))
}

/// A document after the fifty-operation edit.
fn edited_document(input: &Input) -> Result<Document, String> {
    let mut doc = open(input)?;
    let batch = batch(&doc, &operations(input)?)?;
    doc.edit(Edit::ops(batch, stamp()))
        .map_err(|e| e.to_string())?;
    Ok(doc)
}

fn save(input: &Input) -> Result<Run, String> {
    let doc = edited_document(input)?;
    Ok(Box::new(move || {
        let saved = doc.save().map_err(|e| e.to_string())?;
        Ok(Box::new((doc, saved)) as Kept)
    }))
}

fn resume(input: &Input) -> Result<Run, String> {
    let copy = edited_document(input)?
        .export()
        .map_err(|e| e.to_string())?;
    Ok(Box::new(move || {
        let doc = Document::resume(&copy, &OpenOptions::default()).map_err(|e| e.to_string())?;
        Ok(Box::new((copy, doc)) as Kept)
    }))
}

fn files(input: &Input) -> Result<Run, String> {
    let doc = open(input)?;
    Ok(Box::new(move || {
        let files = doc.files().map_err(|e| e.to_string())?;
        Ok(Box::new((doc, files)) as Kept)
    }))
}
