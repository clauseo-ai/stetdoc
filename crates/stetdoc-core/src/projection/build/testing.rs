//! The model of a document for the tests of the build: the stories of a state, and what is in them. The documents
//! themselves are `crate::index::testing::Fixture`.

use super::{Selection, project};
use crate::api::view::Mode;
use crate::projection::model::*;
use crate::state::State;

pub(crate) use crate::index::testing::*;

/// The model of every story of a state, whole, in a mode.
pub(crate) fn model(state: &State, mode: Mode) -> Projection {
    let parts = state.parts();
    let selection = Selection::files(&parts).unwrap();
    project(parts, state.base_token(), mode, Output::Files, &selection).unwrap()
}

/// The model of a fixture in `Redline`, which is what the files are.
pub(crate) fn redline(f: &Fixture) -> Projection {
    model(&f.state(), Mode::Redline)
}

/// The stories of a projection.
pub(crate) fn stories(model: &Projection) -> &[Story] {
    let Body::Stories(s) = &model.body else {
        panic!("a model of files has stories");
    };
    s
}

/// The blocks of the first unit of the story of this kind.
pub(crate) fn blocks_of(model: &Projection, kind: crate::wordml::StoryKind) -> &[Block] {
    stories(model)
        .iter()
        .find(|s| s.kind == kind)
        .and_then(|s| s.units.first())
        .map_or(&[], |u| u.blocks.as_slice())
}

/// The body's blocks.
pub(crate) fn body(model: &Projection) -> &[Block] {
    blocks_of(model, crate::wordml::StoryKind::Body)
}

/// The paragraph at position `i` among the body's blocks.
pub(crate) fn para(model: &Projection, i: usize) -> &Paragraph {
    match body(model).get(i) {
        Some(Block::Paragraph(p)) => p,
        other => panic!("block {i} is not a paragraph: {other:?}"),
    }
}

/// What a list of inlines reads as in `Redline`: every character, atoms as nothing, a field as its text.
pub(crate) fn redline_text(inlines: &[Inline]) -> String {
    flatten(inlines)
}
