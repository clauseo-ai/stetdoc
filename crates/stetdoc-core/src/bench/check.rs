//! `check_edit`, `check_frame` and `check_document`: the check of a batch, of a batch that changes
//! the frame, and of a whole document.
//!
//! A batch is fifty text edits, one in each of fifty blocks spread through the main part, as in
//! `state_writes`, and the run is `check::edit` over the transaction that holds them: the walk, the
//! units, the census of the part that changed, and the rules over the document. `check_frame` adds a
//! paragraph in the body, which is what makes the frame a unit to validate, before and after.
//! `check_document` is `Document::check`, which reads every part.

use super::support::{first_text_below, spread};
use crate::api::limits::Limits;
use crate::check::{Declared, document, edit};
use crate::state::State;
use crate::xml::NodeId;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// How many edits a batch has: the budget of an edit is for fifty.
const OPERATIONS: usize = 50;

fn open_state(input: &Input) -> Result<State, String> {
    State::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())
}

/// Where a batch writes: the name of the main part, the text node of each of fifty blocks, and the
/// last element of the body, which a paragraph is put before.
struct Batch {
    part: String,
    writes: Vec<NodeId>,
    before: Option<NodeId>,
    body: Option<NodeId>,
}

fn batch_of(state: &State) -> Result<Batch, String> {
    let layout = state.parts().layout(false).map_err(|e| e.to_string())?;
    let story = layout.stories.first().ok_or("the document has no story")?;
    let part = story.part.to_string();
    let id = state.parts().id(&part).ok_or("the story has no part")?;
    let tree = state.parts().tree(id).map_err(|e| e.to_string())?;
    let writes: Vec<NodeId> = spread(&story.blocks, OPERATIONS)
        .iter()
        .filter_map(|b| first_text_below(tree, b.key.node()))
        .collect();
    if writes.is_empty() {
        return Err("the document has no text to write".to_owned());
    }
    let body = tree
        .root()
        .child_elements()
        .find(|c| c.local_name() == "body")
        .map(|b| b.id());
    let before = body.and_then(|b| {
        tree.node_ref(b)
            .child_elements()
            .find(|c| c.local_name() == "sectPr")
            .map(|c| c.id())
    });
    Ok(Batch {
        part,
        writes,
        before,
        body,
    })
}

/// Makes the batch in a transaction, and checks it. With `frame` a paragraph is also put in the
/// body, before the section properties.
fn check_batch(state: &State, batch: &Batch, frame: bool) -> Result<(), String> {
    let fail = |e: crate::api::error::StetError| e.to_string();
    let mut tx = state.begin();
    for text in &batch.writes {
        tx.part_mut(&batch.part)
            .map_err(fail)?
            .edit()
            .set_text(*text, "edited")
            .map_err(fail)?;
    }
    if frame {
        let body = batch.body.ok_or("the main part has no body")?;
        let mut ed = tx.part_mut(&batch.part).map_err(fail)?.edit();
        let p = ed.create_element(W, "p").map_err(fail)?;
        ed.insert_before(body, batch.before, p).map_err(fail)?;
    }
    let verified = edit(&tx, &Declared::default()).map_err(fail)?;
    std::hint::black_box(verified);
    Ok(())
}

/// Fifty edits and their check.
pub(super) const EDIT: Bench = Bench {
    name: "check_edit",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: edit_run,
};

fn edit_run(input: &Input) -> Result<Run, String> {
    let state = open_state(input)?;
    let batch = batch_of(&state)?;
    Ok(Box::new(move || {
        check_batch(&state, &batch, false)?;
        Ok(Box::new(state) as Kept)
    }))
}

/// Fifty edits and a paragraph in the body, and their check: the frame is validated as well.
pub(super) const FRAME: Bench = Bench {
    name: "check_frame",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: frame_run,
};

fn frame_run(input: &Input) -> Result<Run, String> {
    let state = open_state(input)?;
    let batch = batch_of(&state)?;
    Ok(Box::new(move || {
        check_batch(&state, &batch, true)?;
        Ok(Box::new(state) as Kept)
    }))
}

/// Checking a whole document: the schema, the Word rules and the audit over every part.
pub(super) const DOCUMENT: Bench = Bench {
    name: "check_document",
    limit: Limit::None,
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: document_run,
};

fn document_run(input: &Input) -> Result<Run, String> {
    let state = open_state(input)?;
    Ok(Box::new(move || {
        let report = document(&state.parts());
        let found = std::hint::black_box(report.violations + report.notes);
        Ok(Box::new((state, found)) as Kept)
    }))
}
