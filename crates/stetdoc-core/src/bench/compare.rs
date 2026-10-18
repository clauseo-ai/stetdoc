//! `compare_*`: a comparison of a 300-page contract with itself after a share of its blocks were changed, the whole of
//! `compare_states` (open both, accept, pair, plan, write the redline, commit). The target of the design (decision 5 of
//! `compare.md`) is one second at 10% of the blocks changed on the contract as built. Compare is not under the speed budgets, and the build does not
//! meet the target yet (about 1 s on the contract and 2.5 to 5 s on the dense contract at 10%), so the time of every row is held to the
//! baseline alone until the refresh-free read of section 10 of the design lands; then the 10% rows take a time limit of one second.
//! The memory is held to the budget of section 10 (`Limit::TwoInputs`): two states are open at once, so the budget is the sum of
//! the memory budgets of the two inputs.
//!
//! The revised document is made when the benchmark is prepared, so the clock does not see it: the paragraphs that change are
//! spread through the main part, and of each ten, seven have a word replaced, two are deleted and one gets a new paragraph after
//! it, all untracked through the compiler.

use crate::api::limits::Limits;
use crate::revisions::compiler::Compiler;
use crate::revisions::stamp::{RevStamp, Track};
use crate::revisions::types::{NewParagraph, Place, PropEdit};
use crate::state::State;
use crate::wordml::{BlockKey, BlockKind, Mode, Paragraph};
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Parses, Run};

const MAIN: &str = "word/document.xml";

/// 10% of the blocks changed, on the contract.
pub(super) const CONTRACT_10: Bench = Bench {
    name: "compare_10pct_contract",
    limit: Limit::TwoInputs,
    parses: Parses::All,
    inputs: Inputs::Only("contract-300p"),
    prepare: ten,
};

/// 10% of the blocks changed, on the dense contract.
pub(super) const DENSE_10: Bench = Bench {
    name: "compare_10pct_dense",
    limit: Limit::TwoInputs,
    parses: Parses::All,
    inputs: Inputs::Only("dense-contract-300p"),
    prepare: ten,
};

/// 25% of the blocks changed, on the contract.
pub(super) const CONTRACT_25: Bench = Bench {
    name: "compare_25pct_contract",
    limit: Limit::TwoInputs,
    parses: Parses::All,
    inputs: Inputs::Only("contract-300p"),
    prepare: twenty_five,
};

/// 25% of the blocks changed, on the dense contract.
pub(super) const DENSE_25: Bench = Bench {
    name: "compare_25pct_dense",
    limit: Limit::TwoInputs,
    parses: Parses::All,
    inputs: Inputs::Only("dense-contract-300p"),
    prepare: twenty_five,
};

fn ten(input: &Input) -> Result<Run, String> {
    prepare(input, 10)
}

fn twenty_five(input: &Input) -> Result<Run, String> {
    prepare(input, 25)
}

fn stamp() -> Result<RevStamp, String> {
    RevStamp::new("Bench", "2026-02-01T00:00:00Z").map_err(|e| e.to_string())
}

/// The input with `percent` of its paragraphs changed, as the bytes of a package.
pub(super) fn revised(original: &State, percent: usize) -> Result<Vec<u8>, String> {
    let mut tx = original.begin();
    let paragraphs: Vec<(BlockKey, usize)> = {
        let view = tx.parts();
        let id = view.id(MAIN).ok_or("no main part")?;
        let story = view.story(id).map_err(|e| e.to_string())?;
        let tree = view.tree(id).map_err(|e| e.to_string())?;
        story
            .blocks()
            .iter()
            .filter(|e| e.kind == BlockKind::Paragraph)
            .filter_map(|e| {
                let p = Paragraph::new(tree, e.node)?;
                Some((story.key(e), p.text(Mode::Final).chars().count()))
            })
            .collect()
    };
    let changed = (paragraphs.len() * percent / 100).max(1);
    let step = (paragraphs.len() / changed).max(1);
    let mut c = Compiler::new(&mut tx, Track::Untracked).map_err(|e| e.to_string())?;
    for (i, (key, len)) in paragraphs.iter().step_by(step).take(changed).enumerate() {
        // a refusal (a field, a control) leaves the paragraph as it is: the share is a target, not a promise
        let _ = match i % 10 {
            7 | 8 if *len > 0 => c.delete_block(key),
            9 => c.insert_block(
                key,
                Place::After,
                &NewParagraph {
                    text: format!("Clause {i} was added in the revised draft."),
                    props: PropEdit::default(),
                    style: None,
                },
            ),
            _ if *len > 12 => c.replace_text(key, 4..8, "amended"),
            _ => c.replace_text(key, 0..0, "New: "),
        };
    }
    c.finish().map_err(|e| e.to_string())?;
    let (s, _) = tx.commit().map_err(|e| e.to_string())?;
    Ok(s.export().map_err(|e| e.to_string())?.head)
}

fn prepare(input: &Input, percent: usize) -> Result<Run, String> {
    let original = input.docx.clone();
    let state = State::open(original.clone(), &Limits::default()).map_err(|e| e.to_string())?;
    let revised = revised(&state, percent)?;
    drop(state);
    let stamp = stamp()?;
    Ok(Box::new(move || {
        let out = crate::compare::compare_states(original, revised, &Limits::default(), &stamp)
            .map_err(|e| e.to_string())?;
        Ok(Box::new(out) as Kept)
    }))
}
