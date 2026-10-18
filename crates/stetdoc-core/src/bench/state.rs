//! `state_*`: the committed state of a document, and what a batch of operations makes it do:
//! opening it, fifty writes each followed by a read of the story, their commit, saving the result,
//! exporting it and resuming from the export.
//!
//! The writes are plain text edits, one in each of fifty blocks spread through the main part, each
//! followed by the read of the story that the next operation of a batch makes, which the write made
//! stale. `state_edit` is the whole of a fifty-operation edit, so it is what the budget of an edit
//! is held against; `state_writes` is the same without the commit, so the commit is the difference.
//! A commit cannot be a benchmark of its own: the transaction it ends borrows the state it was
//! begun on, and a prepared run cannot hold both.

use super::support::{first_text_below, holds_field, spread};
use crate::api::limits::Limits;
use crate::state::{Recorded, State};
use crate::wordml::BlockKey;
use crate::xml::NodeId;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

/// How many operations a batch has: the budget of an edit is for fifty.
const OPERATIONS: usize = 50;

fn open_state(input: &Input) -> Result<State, String> {
    State::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())
}

/// Where a batch writes: the name of the main part, and in each of fifty blocks spread through it
/// the block's key and its first text node.
struct Batch {
    part: String,
    writes: Vec<(BlockKey, NodeId)>,
    /// True if the field pass is run on the story before the first write, as a reader that needs
    /// the fields (the compiler) has, so that each refresh has an index to carry or rebuild.
    fields: bool,
}

fn batch_of(state: &State) -> Result<Batch, String> {
    batch_with(state, false)
}

/// The batch. With `fields` set the first write is in a paragraph that holds a field and the others
/// are in paragraphs that hold none, which is an edit that touches a field once and then goes on.
fn batch_with(state: &State, fields: bool) -> Result<Batch, String> {
    let layout = state.parts().layout(false).map_err(|e| e.to_string())?;
    let story = layout.stories.first().ok_or("the document has no story")?;
    let part = story.part.to_string();
    let id = state.parts().id(&part).ok_or("the story has no part")?;
    let tree = state.parts().tree(id).map_err(|e| e.to_string())?;
    let write_of = |key: &BlockKey| first_text_below(tree, key.node()).map(|t| (key.clone(), t));
    let mut writes: Vec<(BlockKey, NodeId)> = Vec::new();
    let mut blocks = story.blocks.clone();
    if fields {
        let (with, without): (Vec<_>, Vec<_>) = blocks
            .into_iter()
            .partition(|b| holds_field(tree, b.key.node()));
        writes.extend(with.iter().find_map(|b| write_of(&b.key)));
        blocks = without;
    }
    let rest = OPERATIONS - writes.len();
    writes.extend(
        spread(&blocks, rest)
            .iter()
            .filter_map(|b| write_of(&b.key)),
    );
    if writes.is_empty() {
        return Err("the document has no text to write".to_owned());
    }
    Ok(Batch {
        part,
        writes,
        fields,
    })
}

/// The batch on `state`: each write, then the read of the story. With `commit` the transaction ends
/// in a commit and the new state is returned, and without it the transaction is dropped.
fn write_batch(state: &State, batch: &Batch, commit: bool) -> Result<Option<State>, String> {
    let fail = |e: crate::api::error::StetError| e.to_string();
    let mut tx = state.begin();
    let id = tx.parts().id(&batch.part).ok_or("the story has no part")?;
    if batch.fields {
        let before = tx.parts().story(id).map_err(fail)?;
        let tree = tx.parts().tree(id).map_err(fail)?;
        std::hint::black_box(before.fields(tree));
    }
    for (key, text) in &batch.writes {
        tx.part_mut(&batch.part)
            .map_err(fail)?
            .edit()
            .set_text(*text, "edited")
            .map_err(fail)?;
        tx.touch(key);
        tx.parts().story(id).map_err(fail)?;
    }
    if commit {
        Ok(Some(tx.commit().map_err(fail)?.0))
    } else {
        Ok(None)
    }
}

/// Opening the package: the container, then every story parsed and scanned, which is what a ref
/// needs, and the table of refs.
pub(super) const OPEN: Bench = Bench {
    name: "state_open",
    limit: Limit::Op(Op::Open),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: open,
};

fn open(input: &Input) -> Result<Run, String> {
    let file = input.docx.clone();
    Ok(Box::new(move || {
        let state = State::open(file, &Limits::default()).map_err(|e| e.to_string())?;
        Ok(Box::new(state) as Kept)
    }))
}

/// Fifty writes, each followed by a read of the story, in one transaction that is dropped.
pub(super) const WRITES: Bench = Bench {
    name: "state_writes",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: writes,
};

fn writes(input: &Input) -> Result<Run, String> {
    let state = open_state(input)?;
    let batch = batch_of(&state)?;
    Ok(Box::new(move || {
        write_batch(&state, &batch, false)?;
        Ok(Box::new(state) as Kept)
    }))
}

/// Fifty writes, each in a paragraph that holds a field and followed by a read of the story, in one
/// transaction that is dropped. After the first write a refresh must not rebuild the field index
/// again, so this costs about what `state_writes` costs plus one pass of the field index. The field pass
/// runs once before the first write, so there is an index to refresh.
pub(super) const FIELD_WRITES: Bench = Bench {
    name: "state_field_writes",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: field_writes,
};

fn field_writes(input: &Input) -> Result<Run, String> {
    let state = open_state(input)?;
    let batch = batch_with(&state, true)?;
    Ok(Box::new(move || {
        write_batch(&state, &batch, false)?;
        Ok(Box::new(state) as Kept)
    }))
}

/// A fifty-operation edit: the writes and their commit.
pub(super) const EDIT: Bench = Bench {
    name: "state_edit",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: edit,
};

fn edit(input: &Input) -> Result<Run, String> {
    let state = open_state(input)?;
    let batch = batch_of(&state)?;
    Ok(Box::new(move || {
        let next = write_batch(&state, &batch, true)?;
        Ok(Box::new((state, next)) as Kept)
    }))
}

/// The state that an edit committed, which is what the operations after it work on.
fn edited(input: &Input) -> Result<State, String> {
    let state = open_state(input)?;
    let batch = batch_of(&state)?;
    write_batch(&state, &batch, true)?.ok_or_else(|| "the batch was not committed".to_owned())
}

/// Saving an edited state: each part that changed is written, the rest copied, and the package
/// built.
pub(super) const SAVE: Bench = Bench {
    name: "state_save",
    limit: Limit::Op(Op::Save),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: save,
};

fn save(input: &Input) -> Result<Run, String> {
    let state = edited(input)?;
    Ok(Box::new(move || {
        let package = state.to_package().map_err(|e| e.to_string())?;
        let bytes = package.write().map_err(|e| e.to_string())?;
        Ok(Box::new((state, package, bytes)) as Kept)
    }))
}

/// Exporting an edited state: the package, the ref map, and the digests that bind them.
pub(super) const EXPORT: Bench = Bench {
    name: "state_export",
    limit: Limit::Op(Op::Export),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: export,
};

fn export(input: &Input) -> Result<Run, String> {
    let state = edited(input)?;
    Ok(Box::new(move || {
        let exported = state.export().map_err(|e| e.to_string())?;
        Ok(Box::new((state, exported)) as Kept)
    }))
}

/// Resuming from an export: the digests are checked, the head package opened and every story
/// scanned again, and the ref map restored against the scan and the original package.
pub(super) const RESUME: Bench = Bench {
    name: "state_resume",
    limit: Limit::Op(Op::Open),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: resume,
};

fn resume(input: &Input) -> Result<Run, String> {
    let exported = edited(input)?.export().map_err(|e| e.to_string())?;
    let recorded = Recorded {
        base: exported.base,
        head_sha256: exported.head_sha256,
        refs_sha256: exported.refs_sha256,
    };
    let (head, refs) = (exported.head, exported.refs);
    let original = input.docx.clone();
    Ok(Box::new(move || {
        let state = State::resume(head, &refs, &recorded, Some(&original), &Limits::default())
            .map_err(|e| e.to_string())?;
        Ok(Box::new((state, refs, original)) as Kept)
    }))
}
