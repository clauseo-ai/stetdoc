//! What the benchmarks that read or edit the main part have in common: its tree, and where an edit
//! goes.

use crate::api::limits::Limits;
use crate::xml::{Bytes, NodeId, SpanTree};
use stetdoc_testkit::bench::Input;

/// The main part of a document.
pub(super) const MAIN: &str = "word/document.xml";

/// The main part of the input, parsed.
pub(super) fn tree_of(input: &Input) -> Result<SpanTree, String> {
    let bytes = Bytes::from(input.main_part.clone());
    SpanTree::parse(MAIN, bytes, &Limits::default()).map_err(|e| e.to_string())
}

/// The first text node below `node`, or at it, in a `w:t`.
pub(super) fn first_text_below(tree: &SpanTree, node: NodeId) -> Option<NodeId> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        let r = tree.node_ref(n);
        if r.is_element() && r.local_name() == "t" {
            return r.first_child().map(|c| c.id());
        }
        let mark = stack.len();
        stack.extend(r.child_elements().map(|c| c.id()));
        if let Some(tail) = stack.get_mut(mark..) {
            tail.reverse();
        }
    }
    None
}

/// `count` of `items`, spread evenly through them.
pub(super) fn spread<T: Clone>(items: &[T], count: usize) -> Vec<T> {
    let step = (items.len() / count).max(1);
    items.iter().step_by(step).take(count).cloned().collect()
}

/// True if a paragraph holds a field: a field character, a simple field or a field instruction.
///
/// The paragraphs that the benchmarks edit hold none. Writing to a paragraph with a field makes the story's
/// refresh rebuild the field index, which costs a walk of the whole story (about nine milliseconds on the
/// fields input), and the transaction passes every block it has touched to every later refresh, so after one
/// such write each refresh in it pays that again. That is `wordml` and `state`'s cost, and
/// `wordml_field_edits` holds it to a budget of its own. A benchmark of the compiler that began with such a
/// paragraph would measure it in every step instead (the first paragraph of the fields input is one).
pub(super) fn holds_field(tree: &SpanTree, node: NodeId) -> bool {
    let mut stack = vec![tree.node_ref(node)];
    while let Some(n) = stack.pop() {
        if matches!(n.local_name(), "fldChar" | "fldSimple" | "instrText") {
            return true;
        }
        stack.extend(n.child_elements());
    }
    false
}
