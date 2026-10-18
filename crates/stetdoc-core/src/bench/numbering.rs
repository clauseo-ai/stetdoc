//! `numbering_*`: the style cascade and the clause numbers of a story, on the body that their
//! gates are written for (`inputs::NUMBERED`: 20,000 paragraphs, 8,000 of them numbered, 561 styles
//! in chains of ten). Design 6 and 8.5 of styles and numbering.

use super::support::{MAIN, first_text_below, spread};
use crate::api::limits::Limits;
use crate::api::view::Mode;
use crate::numbering::StoryNumbers;
use crate::package::Package;
use crate::state::State;
use crate::styles::Cascade;
use crate::wordml::{NumberingDefs, Story, StoryKind, StyleDefs};
use crate::xml::SpanTree;
use std::sync::Arc;
use stetdoc_testkit::bench::inputs::NUMBERED;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

/// How many writes the interleaved batch has.
const OPERATIONS: usize = 50;

/// A part of the package, parsed.
fn part(package: &Package, name: &str) -> Result<SpanTree, String> {
    let bytes = package
        .part(name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("the package has no {name}"))?;
    SpanTree::parse(name, bytes, &Limits::default()).map_err(|e| e.to_string())
}

/// `Cascade::build`, `Story::num_facts` and `StoryNumbers::build`. The parts are parsed, the
/// definitions read and the story scanned before the clock starts. It is held to the budget of
/// the first view, which it is part of. The design's own target of 25 ms is
/// watched by the baseline instead, so a slower runner warns rather than fails.
pub(super) const BUILD: Bench = Bench {
    name: "numbering_build",
    limit: Limit::Op(Op::View),
    parses: Parses::All,
    inputs: Inputs::Only(NUMBERED),
    prepare: build,
};

fn build(input: &Input) -> Result<Run, String> {
    let package =
        Package::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())?;
    let main = part(&package, MAIN)?;
    let styles = part(&package, "word/styles.xml").unwrap_or_else(|_| empty_tree());
    let numbering = part(&package, "word/numbering.xml").unwrap_or_else(|_| empty_tree());
    let style_defs = StyleDefs::read(&styles);
    let numbering_defs = NumberingDefs::read(&numbering);
    let story = Story::scan(&Arc::from(MAIN), StoryKind::Body, &main).map_err(|e| e.to_string())?;
    Ok(Box::new(move || {
        let cascade = Cascade::build(&style_defs, None, None);
        let facts = story.num_facts(&main);
        let numbers = StoryNumbers::build(&facts, &numbering_defs, &cascade, Mode::Final);
        Ok(Box::new((
            main,
            story,
            style_defs,
            numbering_defs,
            cascade,
            facts,
            numbers,
        )) as Kept)
    }))
}

/// A part with nothing in it, for a document that has no styles or no numbering, as the smoke test
/// of the benchmarks has.
fn empty_tree() -> SpanTree {
    let xml =
        "<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"/>";
    SpanTree::parse(
        "word/styles.xml",
        crate::xml::Bytes::from(xml.as_bytes().to_vec()),
        &Limits::default(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

/// Fifty writes interleaved with reads of the story, then one read of the numbers: the writes must
/// not build the numbers each, and the one read builds them once. The numbers are of the staged
/// part, so the read builds them from the edited tree.
pub(super) const EDIT: Bench = Bench {
    name: "numbering_edit",
    limit: Limit::Op(Op::Edit),
    parses: Parses::All,
    inputs: Inputs::Only(NUMBERED),
    prepare: edit,
};

fn edit(input: &Input) -> Result<Run, String> {
    let state = State::open(input.docx.clone(), &Limits::default()).map_err(|e| e.to_string())?;
    let layout = state.parts().layout(false).map_err(|e| e.to_string())?;
    let story = layout.stories.first().ok_or("the document has no story")?;
    let part = story.part.to_string();
    let id = state.parts().id(&part).ok_or("the story has no part")?;
    let tree = state.parts().tree(id).map_err(|e| e.to_string())?;
    let writes: Vec<_> = spread(&story.blocks, OPERATIONS)
        .iter()
        .filter_map(|b| first_text_below(tree, b.key.node()).map(|t| (b.key.clone(), t)))
        .collect();
    if writes.is_empty() {
        return Err("the document has no text to write".to_owned());
    }
    Ok(Box::new(move || {
        let fail = |e: crate::api::error::StetError| e.to_string();
        let mut tx = state.begin();
        for (key, text) in &writes {
            tx.part_mut(&part)
                .map_err(fail)?
                .edit()
                .set_text(*text, "edited")
                .map_err(fail)?;
            tx.touch(key);
            tx.parts().story(id).map_err(fail)?;
        }
        let numbers = tx.parts().numbers(id, Mode::Final).map_err(fail)?;
        drop(tx);
        Ok(Box::new((state, numbers)) as Kept)
    }))
}
