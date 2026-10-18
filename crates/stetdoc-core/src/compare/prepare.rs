//! Open both packages, give the revised one the namespace of the comparison, and accept every revision in each (design
//! `compare.md`, 2.2 and 8.1).

use crate::api::error::StetError;
use crate::api::limits::Limits;
use crate::revisions::index::ChangeIndex;
use crate::revisions::resolve::resolve;
use crate::revisions::stamp::RevStamp;
use crate::revisions::types::{Action, Selector, Step};
use crate::state::State;

/// The two documents of a comparison, as Word shows them with every change accepted.
pub(crate) struct Prepared {
    /// The original, accepted.
    pub original: State,
    /// The revised document, accepted, under the namespace of the comparison. The redline is built on it.
    pub revised: State,
    /// How many revisions were accepted in the original, and in the revised document.
    pub accepted: (usize, usize),
}

/// Accepts everything in `state`: the state, and how many revisions that took.
fn accepted(state: State) -> Result<(State, usize), StetError> {
    let mut tx = state.begin();
    let res = resolve(
        &mut tx,
        &mut ChangeIndex::new(),
        &[Step {
            action: Action::Accept,
            selector: Selector::All,
        }],
    )?;
    let count = res.first().map_or(0, |r| r.resolved.len());
    if count == 0 {
        tx.discard();
        return Ok((state, 0));
    }
    let (next, _) = tx.commit()?;
    Ok((next, count))
}

/// Opens both packages and accepts their revisions.
pub(crate) fn prepare(
    original_bytes: Vec<u8>,
    revised: Vec<u8>,
    limits: &Limits,
    stamp: &RevStamp,
) -> Result<Prepared, StetError> {
    let extra = [stamp.author().as_bytes(), b"\0", stamp.date().as_bytes()].concat();
    let (original, count_a) = accepted(State::open(original_bytes.clone(), limits)?)?;
    let (revised, count_b) = accepted(State::open_derived(
        &original_bytes,
        revised,
        &extra,
        limits,
    )?)?;
    Ok(Prepared {
        original,
        revised,
        accepted: (count_a, count_b),
    })
}
