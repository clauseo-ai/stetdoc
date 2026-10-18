//! `xml_parse`, `xml_serialize`, `xml_serialize_all_tags` and `xml_clone_edit`: the span tree.

use super::support::{MAIN, tree_of};
use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeId, SpanTree};
use stetdoc_testkit::bench::{Bench, Input, Inputs, Kept, Limit, Op, Parses, Run};

/// Parsing the main part, which is most of what opening a document costs.
pub(super) const PARSE: Bench = Bench {
    name: "xml_parse",
    limit: Limit::Op(Op::Open),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: parse,
};

fn parse(input: &Input) -> Result<Run, String> {
    let bytes = Bytes::from(input.main_part.clone());
    Ok(Box::new(move || {
        let tree = SpanTree::parse(MAIN, bytes, &Limits::default()).map_err(|e| e.to_string())?;
        Ok(Box::new(tree) as Kept)
    }))
}

/// Writing the part after one edit: the clean nodes are copied by their spans, the edited one and
/// the elements above it are written again.
pub(super) const SERIALIZE: Bench = Bench {
    name: "xml_serialize",
    limit: Limit::Op(Op::Save),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: serialize_after_an_edit,
};

/// The first text node below an element named `t`.
fn first_text(tree: &SpanTree) -> Option<NodeId> {
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        if node.is_element() && node.local_name() == "t" {
            return node.first_child().map(|c| c.id());
        }
        let mark = stack.len();
        stack.extend(node.child_elements());
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    None
}

fn serialize_after_an_edit(input: &Input) -> Result<Run, String> {
    let mut tree = tree_of(input)?;
    let text = first_text(&tree).ok_or("the part has no text")?;
    tree.edit()
        .set_text(text, "edited")
        .map_err(|e| e.to_string())?;
    Ok(Box::new(move || {
        let written = tree.serialize();
        Ok(Box::new((tree, written)) as Kept)
    }))
}

/// Writing the part with every start tag regenerated, which is the most that writing can cost
/// when nothing is copied.
pub(super) const SERIALIZE_ALL_TAGS: Bench = Bench {
    name: "xml_serialize_all_tags",
    limit: Limit::Op(Op::Save),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: serialize_all_tags,
};

fn serialize_all_tags(input: &Input) -> Result<Run, String> {
    let mut tree = tree_of(input)?;
    tree.mark_all_tags_dirty();
    Ok(Box::new(move || {
        let written = tree.serialize();
        Ok(Box::new((tree, written)) as Kept)
    }))
}

/// What `state` does to edit a part: the tree is cloned, fifty paragraphs change in the copy, and the
/// copy is written. The tree it was cloned from stays, as it does until the edit is committed, so
/// the peak is of both trees and the part written. How much the clone costs is what separates this
/// from `xml_serialize`.
pub(super) const CLONE_EDIT: Bench = Bench {
    name: "xml_clone_edit",
    limit: Limit::Op(Op::Edit),
    parses: Parses::Main,
    inputs: Inputs::Documents,
    prepare: clone_edit,
};

/// The text nodes of the runs, `count` of them spread evenly through the part.
fn spread_texts(tree: &SpanTree, count: usize) -> Vec<NodeId> {
    let mut texts = Vec::new();
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        if node.is_element() && node.local_name() == "t" {
            texts.extend(node.first_child().map(|c| c.id()));
            continue;
        }
        let mark = stack.len();
        stack.extend(node.child_elements());
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    let step = (texts.len() / count).max(1);
    texts.into_iter().step_by(step).take(count).collect()
}

fn clone_edit(input: &Input) -> Result<Run, String> {
    let base = tree_of(input)?;
    let texts = spread_texts(&base, 50);
    if texts.is_empty() {
        return Err("the part has no text".to_owned());
    }
    Ok(Box::new(move || {
        let mut copy = base.clone();
        let mut editor = copy.edit();
        for text in &texts {
            editor
                .set_text(*text, "edited")
                .map_err(|e| e.to_string())?;
        }
        let written = copy.serialize();
        Ok(Box::new((base, copy, written)) as Kept)
    }))
}
