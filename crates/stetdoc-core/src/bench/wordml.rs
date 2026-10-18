//! `wordml_*`: reading the main part as WordprocessingML. The block scan, the field pass, the text
//! model of every paragraph, and the refresh of a story after an edit; then batches of fifty edits,
//! each followed by a refresh and a read of the edited paragraph, which is what `state` pays after
//! every operation: text edits, edits that insert a whole field into a paragraph, and paragraphs
//! inserted. The edits are made on the span tree, as `ops` will make them, and the story is read
//! through `wordml` alone, so what these measure is `wordml` and not `state`.

use super::support::{MAIN, first_text_below, spread, tree_of};
use crate::api::error::StetError;
use crate::api::view::Mode;
use crate::wordml::{BlockKind, Story, StoryKind};
use crate::xml::{Editor, NodeId, SpanTree};
use std::sync::Arc;
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// How many edits a batch has: the budget of an edit is for fifty operations.
const OPERATIONS: usize = 50;

fn scan_of(tree: &SpanTree) -> Result<Story, String> {
    Story::scan(&Arc::from(MAIN), StoryKind::Body, tree).map_err(|e| e.to_string())
}

/// The tree and its story with the field pass done, which is how an edit finds them: the first read
/// of a paragraph runs the pass, once for the story, and every refresh after it carries the index.
fn primed(input: &Input) -> Result<(SpanTree, Story), String> {
    let tree = tree_of(input)?;
    let story = scan_of(&tree)?;
    let _ = story.fields(&tree);
    Ok((tree, story))
}

/// The paragraphs of the story that have text, in order.
fn paragraphs(tree: &SpanTree, story: &Story) -> Vec<NodeId> {
    story
        .blocks()
        .iter()
        .filter(|e| e.kind == BlockKind::Paragraph)
        .map(|e| e.node)
        .filter(|n| first_text_below(tree, *n).is_some())
        .collect()
}

/// The block scan: what `open` pays `wordml` for.
pub(super) const SCAN: Bench = Bench {
    name: "wordml_scan",
    limit: Limit::Op(Op::Open),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: scan,
};

fn scan(input: &Input) -> Result<Run, String> {
    let tree = tree_of(input)?;
    Ok(Box::new(move || {
        let story = scan_of(&tree)?;
        Ok(Box::new((tree, story)) as Kept)
    }))
}

/// The field pass: a walk of the story that builds the index of its fields, run by the first read
/// of a paragraph.
pub(super) const FIELD_PASS: Bench = Bench {
    name: "wordml_field_pass",
    limit: Limit::Op(Op::View),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: field_pass,
};

fn field_pass(input: &Input) -> Result<Run, String> {
    let tree = tree_of(input)?;
    let story = scan_of(&tree)?;
    Ok(Box::new(move || {
        let fields = story.fields(&tree).len();
        Ok(Box::new((tree, story, std::hint::black_box(fields))) as Kept)
    }))
}

/// The text pass: the text model of every paragraph of the story, which is the most that reading
/// text can cost, since a page asks for about forty.
pub(super) const TEXT_PASS: Bench = Bench {
    name: "wordml_text_pass",
    limit: Limit::Op(Op::View),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: text_pass,
};

fn text_pass(input: &Input) -> Result<Run, String> {
    let (tree, story) = primed(input)?;
    Ok(Box::new(move || {
        let mut items = 0usize;
        for i in 0..story.blocks().len() {
            if let Some(paragraph) = story.paragraph(&tree, i) {
                items += paragraph.map().items().len();
            }
        }
        Ok(Box::new((tree, story, std::hint::black_box(items))) as Kept)
    }))
}

/// The refresh of a story after one text edit in a paragraph that has no field markup of its own
/// in the middle of the part: the scan again, and the field index carried.
pub(super) const REFRESH: Bench = Bench {
    name: "wordml_refresh",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: refresh,
};

fn refresh(input: &Input) -> Result<Run, String> {
    let (mut tree, story) = primed(input)?;
    let all = paragraphs(&tree, &story);
    let target = *all
        .get(all.len() / 2)
        .ok_or("the part has no paragraph with text")?;
    text_edit(&mut tree, target)?;
    Ok(Box::new(move || {
        let refreshed = Story::refresh(&story, &tree, &[target]).map_err(|e| e.to_string())?;
        Ok(Box::new((tree, story, refreshed)) as Kept)
    }))
}

/// What one operation of a batch does to the tree. It returns the paragraph that holds the change,
/// which is the new paragraph when the operation made one.
type Edit = fn(&mut SpanTree, NodeId) -> Result<NodeId, String>;

fn fail(e: StetError) -> String {
    e.to_string()
}

fn text_edit(tree: &mut SpanTree, paragraph: NodeId) -> Result<NodeId, String> {
    let text = first_text_below(tree, paragraph).ok_or("the paragraph has no text")?;
    tree.edit().set_text(text, "edited").map_err(fail)?;
    Ok(paragraph)
}

/// Appends a run to `paragraph` with one element in it: a `w:fldChar` of type `kind`, or an element
/// named `local` that holds `text`.
fn append_run(
    ed: &mut Editor<'_>,
    paragraph: NodeId,
    local: &str,
    kind: Option<&str>,
    text: Option<&str>,
) -> Result<(), StetError> {
    let run = ed.create_element(W, "r")?;
    let inner = ed.create_element(W, local)?;
    if let Some(kind) = kind {
        ed.set_attr(inner, W, "fldCharType", kind)?;
    }
    if let Some(text) = text {
        let node = ed.create_text(text)?;
        ed.append_child(inner, node)?;
    }
    ed.append_child(run, inner)?;
    ed.append_child(paragraph, run)
}

/// Writes a whole field into the paragraph: begin, code, separate, result and end, in five runs.
fn field_edit(tree: &mut SpanTree, paragraph: NodeId) -> Result<NodeId, String> {
    let mut ed = tree.edit();
    let ed = &mut ed;
    let steps: [(&str, Option<&str>, Option<&str>); 5] = [
        ("fldChar", Some("begin"), None),
        ("instrText", None, Some("PAGE")),
        ("fldChar", Some("separate"), None),
        ("t", None, Some("1")),
        ("fldChar", Some("end"), None),
    ];
    for (local, kind, text) in steps {
        append_run(ed, paragraph, local, kind, text).map_err(fail)?;
    }
    Ok(paragraph)
}

/// Inserts a plain paragraph with one run before `before`.
fn paragraph_insert(tree: &mut SpanTree, before: NodeId) -> Result<NodeId, String> {
    let parent = tree
        .node_ref(before)
        .parent()
        .ok_or("the paragraph has no parent")?
        .id();
    let mut ed = tree.edit();
    let paragraph = ed.create_element(W, "p").map_err(fail)?;
    append_run(&mut ed, paragraph, "t", None, Some("a new paragraph")).map_err(fail)?;
    ed.insert_before(parent, Some(before), paragraph)
        .map_err(fail)?;
    Ok(paragraph)
}

/// A batch of fifty edits at paragraphs spread through the part, each followed by the refresh of the
/// story and a read of the edited paragraph's text.
fn batch(input: &Input, edit: Edit) -> Result<Run, String> {
    let (tree, story) = primed(input)?;
    let targets = spread(&paragraphs(&tree, &story), OPERATIONS);
    if targets.is_empty() {
        return Err("the part has no paragraph with text".to_owned());
    }
    Ok(Box::new(move || {
        let (mut tree, mut story, mut read) = (tree, story, 0usize);
        for target in targets {
            let node = edit(&mut tree, target)?;
            story = Story::refresh(&story, &tree, &[node]).map_err(fail)?.story;
            let at = story
                .index_of(node)
                .ok_or("the edited paragraph is not in the story")?;
            read += story
                .paragraph(&tree, at)
                .map_or(0, |p| p.text(Mode::Final).len());
        }
        Ok(Box::new((tree, story, std::hint::black_box(read))) as Kept)
    }))
}

/// Fifty text edits, each followed by a refresh and a read.
pub(super) const TEXT_EDITS: Bench = Bench {
    name: "wordml_text_edits",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: |input| batch(input, text_edit),
};

/// Fifty edits that each write a whole field into a paragraph, followed by a refresh and a read.
/// The refresh walks the paragraphs that hold fields, so it grows with their number.
pub(super) const FIELD_EDITS: Bench = Bench {
    name: "wordml_field_edits",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: |input| batch(input, field_edit),
};

/// Fifty paragraphs inserted, each followed by a refresh and a read.
pub(super) const PARAGRAPH_INSERTS: Bench = Bench {
    name: "wordml_paragraph_inserts",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: |input| batch(input, paragraph_insert),
};
